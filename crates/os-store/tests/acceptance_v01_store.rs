//! Unabhängige Abnahmetests M4-V01 (1.1-Etappe): Persistenz der Vorschläge in der Crawl-Datei (`ai_suggestions`).
//!
//! Erwartungen aus `docs/spec/08-ai.md`: AI-070 („Vorschläge, Prüfstatus und Nutzerentscheidungen bleiben nach Speichern und
//! Öffnen der Crawl-Datei erhalten“; Kandidaten in der Reihenfolge der Antwort), AI-020 Nachtrag 26.11.2026
//! (Verarbeitung einer Antwort ist atomar: Ergebnis, Cache-Eintrag und Protokollzeile in einer Transaktion).
// acceptance: independent

#[path = "../../../tests/fixtures/ai/v01-acceptance/states.rs"]
mod states;

use os_store::ai::cache::CacheRow;
use os_store::ai::requests::{PriceSource, RequestDetails, RequestRecord};
use os_store::ai::suggestions::{self as store, ReviewStatus, SuggestionCommit};
use os_store::{CrawlFile, OpenMode};

fn count(f: &CrawlFile, sql: &str) -> i64 {
    f.conn().query_row(sql, [], |r| r.get(0)).unwrap()
}

// spec: AI-070 [M4]
// spec: AI-076 [M4]
// acceptance: independent
// Alle Einträge (Kandidaten in Antwortreihenfolge mit Länge, Pixelbreite, Gültigkeit, Verstößen, Runde; Status,
// sichtbarer Kandidat, Review, übernommener Wert mit Herkunft) sind nach Schließen und Öffnen der Datei unverändert.
#[test]
fn suggestions_and_decisions_survive_close_and_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let path = {
        let f = states::create(dir.path(), "persist.ospider", os_model::CrawlConfig::default());
        f.path().to_path_buf()
    };
    let f = CrawlFile::open(&path, OpenMode::ReadWrite).unwrap();
    for expected in states::records() {
        let got = store::load(f.conn(), expected.url_id, &expected.element)
            .unwrap()
            .unwrap_or_else(|| panic!("Eintrag {} {} fehlt", expected.url_id, expected.element));
        assert_eq!(got.status, expected.status);
        assert_eq!(got.review, expected.review);
        assert_eq!(got.accepted, expected.accepted);
        assert_eq!(got.candidates, expected.candidates, "Kandidaten in Antwortreihenfolge");
        assert_eq!(got.selected, expected.selected);
        assert_eq!(
            got.displayed().first().map(|c| c.index),
            expected.order.first().copied(),
            "sichtbarer Kandidat 1"
        );
        assert_eq!(got.model, expected.model);
    }
    let edited = store::load(f.conn(), states::id("/edited/"), store::ELEMENT_TITLE)
        .unwrap()
        .unwrap();
    assert_eq!(edited.review, ReviewStatus::Edited);
    assert_eq!(
        edited.accepted.unwrap().violations,
        ["length>60", "pixel_width>561"],
        "Verstoßliste der bearbeiteten Übernahme"
    );
}

fn request(url_id: i64) -> (RequestRecord, RequestDetails) {
    (
        RequestRecord {
            run_id: 1,
            timestamp: "2026-11-27T10:00:00Z".into(),
            task: "suggest-title".into(),
            provider: "anthropic".into(),
            model: "mock-claude".into(),
            endpoint: "messages".into(),
            url_id: Some(url_id),
            definition_version: Some("1".into()),
            reservation: os_model::budget::MicroUsd(500),
            redactions: 0,
            input_tokens: Some(100),
            cached_input_tokens: Some(0),
            cache_write_tokens: Some(0),
            output_tokens: Some(20),
            images: 0,
            token_cost: os_model::budget::MicroUsd(200),
            fixed_fees: os_model::budget::MicroUsd(0),
            regional_uplift: os_model::budget::MicroUsd(0),
            cost: os_model::budget::MicroUsd(200),
            price_source: PriceSource::Table,
            batch_id: None,
            cache_hit: false,
            status: "Success".into(),
            http_status: Some(200),
            error_code: None,
            error_message: None,
            refusal_reason: None,
            duration_ms: 5,
            provider_request_id: Some("req_v01".into()),
            retries: 0,
            over_reservation: false,
            confidence_mismatch: false,
            estimated_input_tokens: Some(90),
        },
        RequestDetails::default(),
    )
}

fn cache_row(key: &str) -> CacheRow {
    CacheRow {
        cache_key: key.into(),
        provider: "anthropic".into(),
        model: "mock-claude".into(),
        endpoint: "http://127.0.0.1/anthropic".into(),
        definition_id: "suggest-title".into(),
        definition_version: "1".into(),
        input_hash: "00".into(),
        params_hash: "00".into(),
        result_json: r#"{"candidates":["x"]}"#.into(),
        status: "Success".into(),
        created_at: "2026-11-27T10:00:00Z".into(),
    }
}

// spec: AI-020 [M4]
// spec: AI-014 [M4]
// spec: AI-015 [M4]
// acceptance: independent
// Ohne Fehler schreibt eine Übernahme Protokollzeile, Cache-Eintrag und Elementeintrag gemeinsam; scheitert ein Teil
// (Fehlerinjektion: Cache-Tabelle fehlt), steht danach keiner der drei Teile in der Datei.
#[test]
fn commit_is_all_or_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let f = states::create(dir.path(), "atomic.ospider", os_model::CrawlConfig::default());
    let id = states::id("/no-suggestion/");
    let mut record = store::ElementRecord::pending(id, store::ELEMENT_TITLE);
    record.status = "Valid".into();
    record.candidates = vec![states::cand(1, "Ein neuer gültiger Title für die Seite", 350, &[], 1)];
    record.selected = Some(1);
    record.order = vec![1];
    let before_requests = count(&f, "SELECT count(*) FROM ai_requests");
    let ids = store::commit(
        f.conn(),
        &SuggestionCommit {
            requests: vec![request(id)],
            cache: vec![cache_row("k-ok")],
            record: record.clone(),
        },
    )
    .unwrap();
    assert_eq!(ids.len(), 1);
    assert_eq!(count(&f, "SELECT count(*) FROM ai_requests"), before_requests + 1);
    assert_eq!(count(&f, "SELECT count(*) FROM ai_cache WHERE cache_key = 'k-ok'"), 1);
    assert!(store::load(f.conn(), id, store::ELEMENT_TITLE).unwrap().is_some());

    // Zweite URL, Fehlerinjektion.
    let id2 = states::id("/pending/");
    f.conn().execute_batch("DROP TABLE ai_cache;").unwrap();
    let mut r2 = record.clone();
    r2.url_id = id2;
    let before = store::load(f.conn(), id2, store::ELEMENT_TITLE).unwrap();
    let result = store::commit(
        f.conn(),
        &SuggestionCommit {
            requests: vec![request(id2)],
            cache: vec![cache_row("k-fail")],
            record: r2,
        },
    );
    assert!(result.is_err(), "Fehlerinjektion muss scheitern");
    assert_eq!(
        count(&f, &format!("SELECT count(*) FROM ai_requests WHERE url_id = {id2}")),
        0,
        "keine Protokollzeile"
    );
    assert_eq!(
        store::load(f.conn(), id2, store::ELEMENT_TITLE).unwrap(),
        before,
        "Elementeintrag unverändert"
    );
}
