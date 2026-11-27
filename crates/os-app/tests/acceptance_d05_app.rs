//! Unabhängige Abnahme M4-D05 (os-app): KI-Lauf über `ai.decisionsRun`, automatische Ausführung während des Crawls,
//! Eingabefelder und gespeicherte KI-Eingaben, Besitz, Heartbeat, Kostenbestätigung, Budget und Lebenszyklus.
//!
//! Erwartungen ausschließlich aus 08 AI-012, AI-013, AI-020 (Nachträge K4-13 mit „Aktiver Besitzer außerhalb von
//! `Running`“ und „Kein KI-Lauf auf einem unterbrochenen Crawl“, 26.11. `Skipped (input unavailable)`, 27.11.
//! `confirmed_at`), AI-032 (Nachtrag „Quelle der Eingabefelder“, berichtigt 27.11.), AI-039, 01 CRAWL-126 sowie den
//! Klärungen K4-13, K4-24 … K4-28 in `docs/plan/M4.md`. Echte Dispatcher-Aufrufe, echter Crawl der Fixture-Site
//! `tests/fixtures/ai/site/`, ai-mock mit `hold`. Nie echte Anbieter, nur Testschlüssel im Arbeitsspeicher.
//!
//! Geordnetes Beenden und Schließen bei gehaltenem Request (Kriterium d) stehen in `acceptance_d05_quit.rs` (eigenes
//! Binary, weil das Beenden der App alle KI-Läufe des Prozesses abbricht).
#![allow(clippy::disallowed_types, clippy::disallowed_methods)]

mod acceptance_d05_support;
#[path = "../../os-store/tests/acceptance_k11_support/mod.rs"]
mod k11;

use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

use acceptance_d05_support::*;
use os_app::dto::config::CrawlConfigDto;
use os_app::ErrorCode;
use os_model::budget::MicroUsd;
use os_model::{Clock, CrawlConfig, CrawlState, SystemClock};
use serde_json::{json, Value};

fn own_pid() -> String {
    std::process::id().to_string()
}

fn owner_pid(path: &std::path::Path) -> String {
    meta(path, "owner_pid").unwrap_or_default()
}

fn slow(cfg: &mut CrawlConfig) {
    cfg.speed.max_urls_per_sec = Some(3.0);
}

fn auto_off(cfg: &mut CrawlConfig) {
    cfg.ai.auto_run.0 = false;
}

/// Crawl der Fixture-Site bis „Fertig“ mit `cfg`; Rückgabe der Crawl-ID.
async fn crawled(h: &Home, cfg: &CrawlConfig) -> String {
    h.profile(cfg);
    let started = crawl_to_end(&h.app, START).await;
    started["crawlId"].as_str().unwrap_or_default().to_string()
}

/// Decisions-Requests je Adresse: Text des Eingabeblocks.
fn texts_by_url(mock: &Mock) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for r in mock.decisions() {
        let b = body(&r);
        if let Some(url) = request_url(&b) {
            out.insert(url, input_text(&b));
        }
    }
    out
}

fn assert_no_duplicate_urls(mock: &Mock) {
    let urls = mock.decision_urls();
    let set: BTreeSet<&String> = urls.iter().collect();
    assert_eq!(
        set.len(),
        urls.len(),
        "keine URL doppelt im Mock-Log: {urls:?}"
    );
}

fn error_text(e: &os_app::CmdError) -> String {
    serde_json::to_string(e).unwrap_or_else(|_| format!("{e:?}"))
}

fn table_exists(path: &std::path::Path, table: &str) -> bool {
    q_i64(
        path,
        &format!("SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = '{table}'"),
    ) > 0
}

fn rows_if_exists(path: &std::path::Path, table: &str) -> i64 {
    if table_exists(path, table) {
        q_i64(path, &format!("SELECT COUNT(*) FROM {table}"))
    } else {
        0
    }
}

// =================================================================================================================
// (a) E-6 a / AI-039: automatische Ausführung während des Crawls
// =================================================================================================================

// spec: AI-039 [M4]
// spec: AI-020 [M4]
// acceptance: independent
// Kriterium (a): „Crawl der Fixture-Site mit aktiver Seitentyp-Frage → Ergebnisse während des Crawls“ (AI-039: automatische
// Ausführung während des Crawls, Standard an, wenn Fragen aktiv sind). Eine Seitenfrage im Standard-Geltungsbereich:
// 22 Einträge `Success`, 3 `Skipped (scope)` (|G| = 25, |S| = 22, 08 Konventionen); erste Antworten liegen vor, während
// die Site noch Seiten ausliefert.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_auto_run_answers_arrive_during_the_crawl() {
    let _s = serial().await;
    let mock = Mock::start().await;
    let site = ai_site().await;
    let h = home(&mock, Opts::default());
    let mut cfg = profile(&site, &[question("d05-auto", &["url", "title"])]);
    slow(&mut cfg);
    h.profile(&cfg);
    start_crawl(&h.app, START).await;

    let start = Instant::now();
    let mut fetches_at_first_answer = None;
    loop {
        let _ = os_app::session::poll(&h.app).await;
        let st = state(&h.app);
        if st == Some(CrawlState::Completed) {
            break;
        }
        if fetches_at_first_answer.is_none() && st == Some(CrawlState::Running) {
            let path = crawl_path(&h.app);
            if count_status(&path, "d05-auto", "Success") >= 1 {
                fetches_at_first_answer = Some(page_fetches(&site));
            }
        }
        assert!(
            start.elapsed() < Duration::from_secs(120),
            "Crawl endet nicht"
        );
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    let first = fetches_at_first_answer.expect("Antwort während des laufenden Crawls");
    assert!(
        page_fetches(&site) > first,
        "nach der ersten Antwort ruft der Crawl noch Seiten ab ({first} → {})",
        page_fetches(&site)
    );
    let path = crawl_path(&h.app);
    assert_eq!(
        count_status(&path, "d05-auto", "Success"),
        22,
        "{:?}",
        statuses(&path, "d05-auto")
    );
    assert_eq!(
        count_status(&path, "d05-auto", "Skipped (scope)"),
        3,
        "{:?}",
        statuses(&path, "d05-auto")
    );
    assert_eq!(mock.decisions().len(), 22);
    assert_no_duplicate_urls(&mock);
    assert_spec_statuses(&path);
}

// spec: AI-039 [M4]
// acceptance: independent
// Kriterium (a): „Option aus → 0 Requests bis ‚KI-Analyse starten‘“; danach sendet der Lauf genau die 22 Einträge des
// Geltungsbereichs und ruft keine Seite ab (INTEG-008: nur KI-Requests).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_auto_run_off_sends_nothing_until_start() {
    let _s = serial().await;
    let mock = Mock::start().await;
    let site = ai_site().await;
    let h = home(&mock, Opts::default());
    let mut cfg = profile(&site, &[question("d05-manual", &["url", "title"])]);
    auto_off(&mut cfg);
    crawled(&h, &cfg).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(
        mock.provider_requests(),
        0,
        "ohne automatische Ausführung kein Request"
    );
    let path = crawl_path(&h.app);
    assert_eq!(count_status(&path, "d05-manual", "Success"), 0);

    let fetches = page_fetches(&site);
    run_ai(&h.app, true).await.expect("KI-Analyse starten");
    assert_eq!(mock.decisions().len(), 22);
    assert_eq!(count_status(&path, "d05-manual", "Success"), 22);
    assert_eq!(
        page_fetches(&site),
        fetches,
        "ein KI-Lauf ruft keine Seiten ab"
    );
    assert_no_duplicate_urls(&mock);
    assert_spec_statuses(&path);
}

// spec: AI-039 [M4]
// spec: AI-032 [M4]
// acceptance: independent
// Kriterium (a) und Punkt 5 des Auftrags: Schalter „KI (Vorschau)“ aus → 0 Requests, nichts gespeichert (keine
// gespeicherten KI-Eingaben, keine Antworten, keine Laufzeile), `ai.decisionsRun` lehnt ab; die gecrawlten URLs sind
// dieselben wie bei einem Profil ohne KI-Einstellungen (Verhalten von 1.0).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_switch_off_behaves_like_1_0() {
    let _s = serial().await;
    let mock = Mock::start().await;
    let site = ai_site().await;
    let off = Opts {
        preview: false,
        ..Opts::default()
    };
    let h = home(&mock, off);
    let cfg = profile(
        &site,
        &[question("d05-off", &["url", "title", "page_text"])],
    );
    crawled(&h, &cfg).await;
    let path = crawl_path(&h.app);
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(mock.provider_requests(), 0);
    for table in ["page_ai_inputs", "ai_answers", "ai_runs", "ai_requests"] {
        assert_eq!(rows_if_exists(&path, table), 0, "{table} bleibt leer");
    }
    let err = run_ai(&h.app, true)
        .await
        .expect_err("Schalter aus: kein KI-Lauf");
    assert_ne!(err.code, ErrorCode::NotFound, "{err:?}");
    assert_eq!(mock.provider_requests(), 0);

    let plain = home(&mock, off);
    crawled(&plain, &profile(&site, &[])).await;
    let urls = |p: &std::path::Path| -> BTreeSet<(String, i64)> {
        let f = ro(p);
        let mut stmt = f
            .conn()
            .prepare("SELECT address, COALESCE(status_code, -1) FROM urls")
            .unwrap();
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))
            .unwrap()
            .collect::<Result<BTreeSet<_>, _>>()
            .unwrap();
        rows
    };
    assert_eq!(urls(&path), urls(&crawl_path(&plain.app)));
}

// =================================================================================================================
// (b), (k) AI-032: Eingabefelder und gespeicherte KI-Eingaben
// =================================================================================================================

const FIELD_INPUTS: [&str; 10] = [
    "url",
    "title",
    "meta_description",
    "h1",
    "h2",
    "page_text",
    "word_count",
    "html_lang",
    "hreflang_self",
    "structured_data_types",
];

fn fields_request_text(mock: &Mock) -> String {
    texts_by_url(mock)
        .into_iter()
        .find(|(u, _)| u.ends_with("/fields"))
        .map(|(_, t)| t)
        .unwrap_or_else(|| panic!("kein Request für /fields: {:?}", mock.decision_urls()))
}

// spec: AI-032 [M4]
// spec: AI-039 [M4]
// acceptance: independent
// Kriterium (b) mit (k): Eingabefelder aus `tests/fixtures/ai/site/fields.html` – Title, Meta Description, H1, alle H2 in
// Dokumentreihenfolge, Text des Inhaltsbereichs, Wortzahl, `html_lang`, `hreflang_self`, strukturierte Datentypen – stehen
// wie gesendet im Mock-Log, während des Crawls und nach dem Crawl („KI-Analyse starten“) mit demselben Text. Der
// Bildkontext steht in den gespeicherten KI-Eingaben (AI-032 Nachtrag (1)); eine Seitenfrage sendet ihn nicht (AI-032:
// `image_context` gehört zu Bildfragen).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn b_fields_are_sent_during_and_after_the_crawl() {
    let _s = serial().await;
    let dir = tempfile::tempdir().unwrap();
    let site = fields_site(dir.path()).await;
    let mut cfg = profile(&site, &[question("d05-fields", &FIELD_INPUTS)]);
    cfg.extraction.structured_data.json_ld = true;

    // Während des Crawls (automatische Ausführung).
    let during_mock = Mock::start().await;
    let during = home(&during_mock, Opts::default());
    crawled(&during, &cfg).await;
    let text = fields_request_text(&during_mock);
    for marker in [
        "MARK-TITLE",
        "MARK-META",
        "MARK-H1",
        "Pflege MARK-H2",
        "Größen MARK-H2",
        "MARK-TEXT",
        "MARK-LANG",
        "MARK-HREFLANG",
        "MARK-SD",
        "Product",
    ] {
        assert!(
            text.contains(marker),
            "{marker} fehlt im gesendeten Text:\n{text}"
        );
    }
    let pflege = text.find("Pflege MARK-H2").unwrap();
    let groessen = text.find("Größen MARK-H2").unwrap();
    assert!(pflege < groessen, "H2 in Dokumentreihenfolge");
    let path = crawl_path(&during.app);
    let words = q_i64(
        &path,
        "SELECT word_count FROM urls WHERE address LIKE '%/fields'",
    );
    assert!(words > 0);
    assert!(
        labelled(&text, "Word Count").as_deref() == Some(words.to_string().as_str())
            || text.contains(&words.to_string()),
        "Wortzahl {words} gesendet:\n{text}"
    );

    // Gespeicherte KI-Eingaben der Seite (Nachtrag K4-24 Punkt 1, K4-26).
    let f = ro(&path);
    let url_id: i64 = f
        .conn()
        .query_row(
            "SELECT id FROM urls WHERE address LIKE '%/fields'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let stored = os_store::ai::inputs::read(f.conn(), url_id)
        .unwrap()
        .expect("gespeicherte KI-Eingaben für /fields");
    assert!(stored
        .page_text
        .as_deref()
        .unwrap_or_default()
        .contains("MARK-TEXT"));
    assert_eq!(
        stored.h2,
        vec!["Pflege MARK-H2".to_string(), "Größen MARK-H2".to_string()]
    );
    assert_eq!(stored.html_lang.as_deref(), Some("MARK-LANG"));
    let types = stored
        .structured_data_types
        .clone()
        .expect("Extraktion an: Liste");
    assert!(
        types.iter().any(|t| t == "Product") && types.iter().any(|t| t == "MARK-SD"),
        "{types:?}"
    );
    assert!(
        stored.images.iter().any(|i| i
            .context
            .as_deref()
            .unwrap_or_default()
            .contains("MARK-CONTEXT")),
        "Bildkontext gespeichert: {:?}",
        stored.images
    );
    assert!(stored
        .images
        .iter()
        .all(|i| i.context.as_deref().unwrap_or_default().chars().count() <= 300));
    drop(f);

    // Nach dem Crawl („KI-Analyse starten“) derselbe Text, 0 Seitenabrufe.
    let after_mock = Mock::start().await;
    let after = home(&after_mock, Opts::default());
    let mut manual = cfg.clone();
    auto_off(&mut manual);
    crawled(&after, &manual).await;
    assert_eq!(after_mock.provider_requests(), 0);
    let fetches = page_fetches(&site);
    run_ai(&after.app, true).await.expect("KI-Analyse starten");
    assert_eq!(page_fetches(&site), fetches, "0 Seitenabrufe im KI-Lauf");
    assert_eq!(
        fields_request_text(&after_mock),
        text,
        "gleiche Eingabe wie während des Crawls"
    );
}

// spec: AI-032 [M4]
// acceptance: independent
// Nachtrag K4-24, Kriterium 1: Schalter an, Option „KI-Eingaben speichern“ an (Standard), ohne aktive Fragen → für jede
// interne HTML-Seite mit Status 200 ein Eintrag gespeicherter KI-Eingaben (23 auf der Fixture-Site, nicht indexierbare
// eingeschlossen), für Bilder, Weiterleitungen und Fehlerseiten keiner. Die Größe wird für den Bericht ausgegeben.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn b_inputs_are_stored_for_each_internal_html_200_page_without_questions() {
    let _s = serial().await;
    let mock = Mock::start().await;
    let site = ai_site().await;
    let h = home(&mock, Opts::default());
    let cfg = profile(&site, &[]);
    assert!(cfg.ai.store_inputs.0, "Standard der Option ist an");
    crawled(&h, &cfg).await;
    let path = crawl_path(&h.app);
    let ids = |sql: &str| -> BTreeSet<i64> {
        let f = ro(&path);
        let mut stmt = f.conn().prepare(sql).unwrap();
        let rows = stmt
            .query_map([], |r| r.get::<_, i64>(0))
            .unwrap()
            .collect::<Result<BTreeSet<_>, _>>()
            .unwrap();
        rows
    };
    let stored = ids("SELECT url_id FROM page_ai_inputs");
    let html_200 = ids(
        "SELECT id FROM urls WHERE is_internal = 1 AND lower(content_class) = 'html' AND status_code = 200",
    );
    assert_eq!(html_200.len(), 23, "Fixture: 23 interne HTML-200-URLs");
    assert_eq!(
        stored, html_200,
        "genau die internen HTML-Seiten mit Status 200"
    );
    let others = ids(
        "SELECT id FROM urls WHERE NOT (is_internal = 1 AND lower(content_class) = 'html' AND status_code = 200)",
    );
    assert!(stored.is_disjoint(&others));
    assert_eq!(mock.provider_requests(), 0, "ohne Fragen kein Request");
    let bytes = q_i64(
        &path,
        "SELECT COALESCE(SUM(length(body)), 0) FROM page_ai_inputs",
    );
    let max = q_i64(
        &path,
        "SELECT COALESCE(MAX(length(body)), 0) FROM page_ai_inputs",
    );
    eprintln!(
        "D05-GROESSE: Fixture-Site {} Seiten, gespeicherte KI-Eingaben (Spalte body, zstd) {} Byte gesamt, {} Byte je Seite im Mittel, höchstens {} Byte",
        stored.len(),
        bytes,
        bytes / stored.len().max(1) as i64,
        max
    );
}

// spec: AI-032 [M4]
// spec: AI-020 [M4]
// acceptance: independent
// Nachtrag K4-24, Kriterium 3: Crawl ohne gespeicherte Eingaben, mit gespeichertem HTML → eine `page_text`-Frage läuft mit
// aus `page_html` abgeleitetem Text, nichts wird nachträglich gespeichert; ohne gespeichertes HTML → `Skipped (input
// unavailable)`, kein Request dafür, eine Title-Frage läuft normal; kein Seitenabruf.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn b_inputs_are_derived_from_page_html_or_unavailable() {
    let _s = serial().await;
    let site = ai_site().await;

    // Mit gespeichertem HTML.
    let mock = Mock::start().await;
    let h = home(&mock, Opts::default());
    let mut cfg = profile(&site, &[question("d05-text", &["url", "page_text"])]);
    auto_off(&mut cfg);
    cfg.ai.store_inputs.0 = false;
    cfg.extraction.store_html = true;
    crawled(&h, &cfg).await;
    let path = crawl_path(&h.app);
    assert_eq!(
        rows_if_exists(&path, "page_ai_inputs"),
        0,
        "Option aus: nichts gespeichert"
    );
    let fetches = page_fetches(&site);
    run_ai(&h.app, true).await.expect("KI-Lauf");
    assert_eq!(
        count_status(&path, "d05-text", "Success"),
        22,
        "{:?}",
        statuses(&path, "d05-text")
    );
    let texts = texts_by_url(&mock);
    assert_eq!(texts.len(), 22);
    for (url, t) in &texts {
        let page_text = labelled(t, "Page Text").unwrap_or_default();
        assert!(
            t.contains("MOCK-ID:"),
            "{url}: abgeleiteter Seitentext gesendet ({page_text:?})\n{t}"
        );
    }
    assert_eq!(
        rows_if_exists(&path, "page_ai_inputs"),
        0,
        "nichts nachträglich gespeichert"
    );
    assert_eq!(page_fetches(&site), fetches);

    // Ohne gespeichertes HTML.
    let mock = Mock::start().await;
    let h = home(&mock, Opts::default());
    let mut cfg = profile(
        &site,
        &[
            question("d05-text", &["url", "page_text"]),
            question("d05-title", &["url", "title"]),
        ],
    );
    auto_off(&mut cfg);
    cfg.ai.store_inputs.0 = false;
    cfg.extraction.store_html = false;
    crawled(&h, &cfg).await;
    let path = crawl_path(&h.app);
    let fetches = page_fetches(&site);
    run_ai(&h.app, true).await.expect("KI-Lauf");
    assert_eq!(
        count_status(&path, "d05-text", "Skipped (input unavailable)"),
        22,
        "{:?}",
        statuses(&path, "d05-text")
    );
    assert_eq!(
        count_status(&path, "d05-title", "Success"),
        22,
        "{:?}",
        statuses(&path, "d05-title")
    );
    assert_eq!(mock.decisions().len(), 22);
    for r in mock.decisions() {
        let b = body(&r);
        let names = question_names(&b);
        assert!(
            names.len() == 1 && names[0].contains("title") && !names[0].contains("text"),
            "nur die Title-Frage wird gesendet: {names:?}"
        );
        assert!(labelled(&input_text(&b), "Page Text").is_none());
    }
    assert_eq!(page_fetches(&site), fetches, "kein Seitenabruf");
    assert_spec_statuses(&path);
}

// spec: AI-032 [M4]
// acceptance: independent
// Kriterium (k), K4-26: Extraktion strukturierter Daten aus (Standard) → `structured_data_types` ist für jede Seite NULL,
// eine Frage mit diesem Feld erhält nach dem Crawl `Skipped (input unavailable)`; mit JSON-LD-Extraktion → Liste der
// Typen, für eine Seite ohne strukturierte Daten eine leere Liste (nicht NULL), die Frage läuft.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn k_structured_data_types_are_null_without_extraction() {
    let _s = serial().await;
    let site = ai_site().await;
    let mock = Mock::start().await;
    let h = home(&mock, Opts::default());
    let mut cfg = profile(
        &site,
        &[question("d05-sd", &["url", "structured_data_types"])],
    );
    auto_off(&mut cfg);
    assert!(
        !cfg.extraction.structured_data.json_ld,
        "Standard: Extraktion aus"
    );
    crawled(&h, &cfg).await;
    let path = crawl_path(&h.app);
    assert_eq!(q_i64(&path, "SELECT COUNT(*) FROM page_ai_inputs"), 23);
    assert_eq!(
        q_i64(
            &path,
            "SELECT COUNT(*) FROM page_ai_inputs WHERE structured_data_types IS NOT NULL"
        ),
        0,
        "ohne Extraktion NULL"
    );
    run_ai(&h.app, true).await.expect("KI-Lauf");
    assert_eq!(
        count_status(&path, "d05-sd", "Skipped (input unavailable)"),
        22,
        "{:?}",
        statuses(&path, "d05-sd")
    );
    assert_eq!(mock.decisions().len(), 0);

    let dir = tempfile::tempdir().unwrap();
    let fsite = fields_site(dir.path()).await;
    let mock = Mock::start().await;
    let h = home(&mock, Opts::default());
    let mut cfg = profile(
        &fsite,
        &[question("d05-sd", &["url", "structured_data_types"])],
    );
    auto_off(&mut cfg);
    cfg.extraction.structured_data.json_ld = true;
    crawled(&h, &cfg).await;
    let path = crawl_path(&h.app);
    let f = ro(&path);
    let id_of = |suffix: &str| -> i64 {
        f.conn()
            .query_row(
                "SELECT id FROM urls WHERE address LIKE ?1",
                [format!("%{suffix}")],
                |r| r.get(0),
            )
            .unwrap()
    };
    let fields = os_store::ai::inputs::read(f.conn(), id_of("/fields"))
        .unwrap()
        .expect("gespeicherte Eingaben /fields");
    let types = fields.structured_data_types.expect("Liste");
    assert!(
        types.iter().any(|t| t == "Product") && types.iter().any(|t| t == "MARK-SD"),
        "{types:?}"
    );
    let start = os_store::ai::inputs::read(f.conn(), id_of("ai.test/"))
        .unwrap()
        .expect("gespeicherte Eingaben Startseite");
    assert_eq!(
        start.structured_data_types,
        Some(Vec::new()),
        "extrahiert, nichts gefunden: leere Liste"
    );
    drop(f);
    run_ai(&h.app, true).await.expect("KI-Lauf");
    assert!(!mock.decisions().is_empty(), "die Frage läuft");
    assert_eq!(
        status_of(&path, "http://ai.test/fields", "d05-sd").as_deref(),
        Some("Success")
    );
}

// spec: AI-032 [M4]
// acceptance: independent
// Kriterium (k), K4-26: Crawl ohne gespeichertes HTML → eine Frage mit `html_lang` erhält nach dem Crawl denselben Wert
// wie während des Crawls (Mock-Log); `hreflang_self` der Seite mit selbstreferenzierendem hreflang `fr` ist `fr`.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn k_html_lang_and_hreflang_self_after_the_crawl_match_the_crawl() {
    let _s = serial().await;
    let site = ai_site().await;
    // Die Fixture verweist mit Port (`http://ai.test:{{PORT}}/french`, Host `ai.test:8080` in 08 Konventionen).
    let start = format!("http://ai.test:{}/", site.port());
    let q = question("d05-lang", &["url", "html_lang", "hreflang_self"]);

    let during_mock = Mock::start().await;
    let during = home(&during_mock, Opts::default());
    let cfg = profile(&site, std::slice::from_ref(&q));
    assert!(!cfg.extraction.store_html);
    during.profile(&cfg);
    crawl_to_end(&during.app, &start).await;

    let after_mock = Mock::start().await;
    let after = home(&after_mock, Opts::default());
    let mut manual = cfg.clone();
    auto_off(&mut manual);
    after.profile(&manual);
    crawl_to_end(&after.app, &start).await;
    run_ai(&after.app, true).await.expect("KI-Lauf");

    let a = texts_by_url(&during_mock);
    let b = texts_by_url(&after_mock);
    assert_eq!(a.len(), 22);
    assert_eq!(a, b, "gleiche Eingaben während und nach dem Crawl");
    let french = a
        .iter()
        .find(|(u, _)| u.ends_with("/french"))
        .map(|(_, t)| t.clone())
        .expect("Request der Seite /french");
    let hreflang_rows: Vec<String> = {
        let f = ro(&crawl_path(&after.app));
        let mut stmt = f
            .conn()
            .prepare("SELECT u.address || ' | ' || COALESCE(h.code_raw, '') || ' | ' || COALESCE(h.url, '') FROM hreflang h JOIN urls u ON u.id = h.url_id")
            .unwrap();
        let rows = stmt
            .query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        rows
    };
    eprintln!("D05-HREFLANG: {hreflang_rows:?}");
    assert_eq!(
        labelled(&french, "Hreflang Self").as_deref(),
        Some("fr"),
        "{french}"
    );
    let english = a
        .iter()
        .find(|(u, _)| u.ends_with("/english"))
        .map(|(_, t)| t.clone())
        .expect("Request der Seite /english");
    assert_eq!(
        labelled(&english, "HTML Lang").as_deref(),
        Some("de"),
        "{english}"
    );
}

// =================================================================================================================
// (c) Platzhalter aus D01 – je ein eigener Abnahmetest
// =================================================================================================================

// spec: AI-020 [M4]
// acceptance: independent
// (c) heartbeat_of_an_ai_run – „KI-Lauf auf einem abgeschlossenen Crawl …, Szenario `hold` für Request 1, 7 s gehalten →
// mindestens vier Werte `heartbeat_at` (Start, 2 s, 4 s, 6 s), `state` durchgehend `Completed`; nach der Freigabe und
// dem Ende des Laufs ist `owner_pid` leer und `heartbeat_at` der Endzeitpunkt.“ (Echtzeit statt simulierter Uhr.)
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn c_heartbeat_of_an_ai_run() {
    let _s = serial().await;
    let mock = Mock::start().await;
    let site = ai_site().await;
    let h = home(&mock, Opts::default());
    let mut cfg = profile(&site, &[question("d05-beat", &["url", "title"])]);
    auto_off(&mut cfg);
    crawled(&h, &cfg).await;
    let path = crawl_path(&h.app);
    mock.hold(1).await;
    let op = start_ai(&h.app, true).await;
    mock.wait_held(1).await;
    let mut beats = Vec::<String>::new();
    let mut states = BTreeSet::new();
    let start = Instant::now();
    while start.elapsed() < Duration::from_millis(7_000) {
        if let Some(b) = meta(&path, "heartbeat_at") {
            if beats.last() != Some(&b) {
                beats.push(b);
            }
        }
        states.insert(meta(&path, "state").unwrap_or_default());
        assert_eq!(owner_pid(&path), own_pid(), "Besitz während des Laufs");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert_eq!(mock.release(None).await, 1);
    wait_op(&h.app, &op).await.expect("Lauf endet");
    assert!(
        beats.len() >= 4,
        "mindestens vier Heartbeats in 7 s: {beats:?}"
    );
    assert_eq!(states, BTreeSet::from(["Completed".to_string()]));
    assert_eq!(owner_pid(&path), "", "owner_pid leer nach dem Ende");
    let last = meta(&path, "heartbeat_at").unwrap();
    assert!(
        last >= *beats.last().unwrap(),
        "heartbeat_at = Endzeitpunkt ({last} vs {beats:?})"
    );
    assert_eq!(meta(&path, "state").as_deref(), Some("Completed"));
}

// spec: AI-020 [M4]
// acceptance: independent
// (c) ai_inside_the_own_running_crawl_keeps_the_owner – „KI während eines laufenden Crawls desselben Prozesses: Endet der
// KI-Teil vor dem Crawl, bleibt `owner_pid` gesetzt und der Crawl `Running`.“
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn c_ai_inside_the_own_running_crawl_keeps_the_owner() {
    let _s = serial().await;
    let mock = Mock::start().await;
    let site = ai_site().await;
    let h = home(&mock, Opts::default());
    let mut cfg = profile(&site, &[question("d05-own", &["url", "title"])]);
    slow(&mut cfg);
    h.profile(&cfg);
    start_crawl(&h.app, START).await;
    let start = Instant::now();
    let mut idle_checks = 0;
    loop {
        let _ = os_app::session::poll(&h.app).await;
        if state(&h.app) == Some(CrawlState::Completed) {
            break;
        }
        if state(&h.app) == Some(CrawlState::Running) {
            let path = crawl_path(&h.app);
            let answered = count_status(&path, "d05-own", "Success") as usize;
            // KI-Teil fertig (alle gesendeten Requests beantwortet), Crawl ruft weiter Seiten ab.
            if answered >= 2 && answered == mock.decisions().len() && page_fetches(&site) < 25 {
                assert_eq!(owner_pid(&path), own_pid(), "owner_pid bleibt gesetzt");
                assert_eq!(meta(&path, "state").as_deref(), Some("Running"));
                idle_checks += 1;
            }
        }
        assert!(start.elapsed() < Duration::from_secs(120));
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
    assert!(
        idle_checks >= 1,
        "mindestens eine Prüfung mit fertigem KI-Teil vor Crawl-Ende"
    );
    let path = crawl_path(&h.app);
    assert_eq!(owner_pid(&path), "", "nach dem Ende des Crawls leer");
}

// spec: AI-020 [M4]
// acceptance: independent
// (c) owner_metadata_during_an_ai_run – „Besitzer eines KI-Laufs: KI-Lauf auf einem abgeschlossenen Crawl, Szenario
// `hold` → solange Request 1 gehalten wird, tragen die Crawl-Metadaten `owner_kind` und `owner_pid` des laufenden
// Prozesses, der Crawl-Zustand bleibt `Completed`; nach dem Ende des Laufs ist `owner_pid` leer.“
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn c_owner_metadata_during_an_ai_run() {
    let _s = serial().await;
    let mock = Mock::start().await;
    let site = ai_site().await;
    let h = home(&mock, Opts::default());
    let mut cfg = profile(&site, &[question("d05-owner", &["url", "title"])]);
    auto_off(&mut cfg);
    crawled(&h, &cfg).await;
    let path = crawl_path(&h.app);
    assert_eq!(owner_pid(&path), "");
    mock.hold(1).await;
    let op = start_ai(&h.app, true).await;
    mock.wait_held(1).await;
    assert_eq!(meta(&path, "owner_kind").as_deref(), Some("app"));
    assert_eq!(owner_pid(&path), own_pid());
    assert_eq!(meta(&path, "state").as_deref(), Some("Completed"));
    mock.release(None).await;
    wait_op(&h.app, &op).await.expect("Lauf endet");
    assert_eq!(owner_pid(&path), "");
    assert_eq!(meta(&path, "state").as_deref(), Some("Completed"));
}

// spec: AI-020 [M4]
// spec: OUT-228
// acceptance: independent
// (c) delete_protection_during_an_ai_run – „Solange der Mock Request 1 eines KI-Laufs auf einem abgeschlossenen Crawl
// hält, lehnt die Crawl-Verwaltung das Löschen dieses Crawls ab wie bei einem laufenden Crawl (OUT-228); nach dem Ende des
// Laufs ist Löschen möglich.“ Gelöscht wird aus einer zweiten App-Instanz, die den Crawl nicht geöffnet hat.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn c_delete_protection_during_an_ai_run() {
    let _s = serial().await;
    let mock = Mock::start().await;
    let site = ai_site().await;
    let h = home(&mock, Opts::default());
    let mut cfg = profile(&site, &[question("d05-delete", &["url", "title"])]);
    auto_off(&mut cfg);
    let id = crawled(&h, &cfg).await;
    let path = crawl_path(&h.app);
    let other = h.second();
    call(&other, "crawls.list", json!({ "show": "all" })).await;
    mock.hold(1).await;
    let op = start_ai(&h.app, true).await;
    mock.wait_held(1).await;
    let err = outcome(&other, "crawls.delete", json!({ "ids": [id] }))
        .await
        .expect_err("Löschen während des KI-Laufs abgelehnt");
    assert_eq!(err.code, ErrorCode::Busy, "{err:?}");
    assert!(path.exists());
    mock.release(None).await;
    wait_op(&h.app, &op).await.expect("Lauf endet");
    close(&h.app).await;
    outcome(&other, "crawls.delete", json!({ "ids": [id] }))
        .await
        .expect("nach dem Ende des Laufs löschbar");
    assert!(!path.exists());
}

// spec: AI-020 [M4]
// spec: AI-039 [M4]
// spec: CRAWL-126 [M4]
// acceptance: independent
// (c) ai_run_during_a_paused_crawl (Kernteil) – „automatische Ausführung aus, Crawl pausiert mit 10 gecrawlten URLs,
// ‚KI-Analyse starten‘ → KI-Requests nur für Einträge dieser URLs, gecrawlte URLs bleiben 10, Crawl-Zustand `Paused`; bei
// laufendem Crawl ist die Schaltfläche deaktiviert.“ Kernteil: `ai.decisionsRun` lehnt während des laufenden Crawls ab.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn c_ai_run_during_a_paused_crawl() {
    let _s = serial().await;
    let mock = Mock::start().await;
    let site = ai_site().await;
    let h = home(&mock, Opts::default());
    let mut cfg = profile(&site, &[question("d05-pause", &["url", "title"])]);
    auto_off(&mut cfg);
    slow(&mut cfg);
    h.profile(&cfg);
    start_crawl(&h.app, START).await;
    let path = crawl_path(&h.app);
    wait_until("10 gecrawlte URLs", Duration::from_secs(60), || {
        q_i64(&path, CRAWLED) >= 10
    })
    .await;
    // Kernteil von „bei laufendem Crawl ist die Schaltfläche deaktiviert“: `startAiRun` (bestätigt) startet während des
    // laufenden Crawls keinen KI-Lauf – mit ausgeschalteter automatischer Ausführung geht kein Request hinaus.
    let during = run_ai(&h.app, true).await;
    assert!(
        !matches!(&during, Err(e) if e.code == ErrorCode::NotFound),
        "{during:?}"
    );
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(
        mock.provider_requests(),
        0,
        "bei laufendem Crawl kein KI-Lauf ({during:?})"
    );
    call(&h.app, "crawl.pause", Value::Null).await;
    wait_state(&h.app, CrawlState::Paused, Duration::from_secs(30)).await;
    assert_eq!(mock.provider_requests(), 0);
    let crawled_before = q_i64(&path, CRAWLED);
    let fetches = page_fetches(&site);
    let scope = in_scope_addresses(&path);
    assert!(
        !scope.is_empty() && scope.len() < 22,
        "Pause vor dem Ende: {scope:?}"
    );

    run_ai(&h.app, true).await.expect("KI-Lauf in der Pause");
    let sent: BTreeSet<String> = mock.decision_urls().into_iter().collect();
    assert_eq!(
        sent, scope,
        "KI-Requests genau für die Einträge der gecrawlten URLs"
    );
    assert_eq!(mock.decisions().len(), scope.len());
    assert_eq!(
        q_i64(&path, CRAWLED),
        crawled_before,
        "gecrawlte URLs unverändert"
    );
    assert_eq!(page_fetches(&site), fetches, "nur KI-Requests");
    assert_eq!(state(&h.app), Some(CrawlState::Paused));
    assert_eq!(meta(&path, "state").as_deref(), Some("Paused"));
}

// spec: AI-020 [M4]
// spec: CRAWL-128
// acceptance: independent
// (c) crawl_end_without_stop_drains_the_queue – „Crawl der Fixture-Site mit `limits.max_urls` = 10 und Szenario `hold`;
// nach dem Ende (`limit_reached`) gibt der Test die gehaltenen Requests nacheinander frei → alle offenen Einträge der 10
// URLs werden gesendet, bis die Warteschlange leer ist; Mock-Log = Zahl der Einträge im Geltungsbereich dieser 10 URLs.“
// Dazu: „der Crawl gilt erst dann als vollständig“ – solange Requests gehalten werden, ist der Crawl nicht `Completed`.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn c_crawl_end_without_stop_drains_the_queue() {
    let _s = serial().await;
    let mock = Mock::start().await;
    let site = ai_site().await;
    let h = home(&mock, Opts::default());
    let mut cfg = profile(&site, &[question("d05-drain", &["url", "title"])]);
    cfg.limits.max_urls = 10;
    h.profile(&cfg);
    mock.hold(1_000).await;
    start_crawl(&h.app, START).await;
    let path = crawl_path(&h.app);
    // `session::poll` darf das Testende nicht blockieren: jeder Aufruf mit Frist, blockierte Aufrufe werden gezählt.
    let mut blocked_polls = 0usize;
    // Ende der Abrufe abwarten (keine neue Seite in 1,5 s) bei gehaltenem Request.
    let mut last = page_fetches(&site);
    let mut stable_since = Instant::now();
    let start = Instant::now();
    loop {
        if tokio::time::timeout(Duration::from_secs(3), os_app::session::poll(&h.app))
            .await
            .is_err()
        {
            blocked_polls += 1;
        }
        let now = page_fetches(&site);
        if now != last {
            last = now;
            stable_since = Instant::now();
        }
        if mock.held() >= 1 && stable_since.elapsed() > Duration::from_millis(1_500) {
            break;
        }
        assert!(
            start.elapsed() < Duration::from_secs(60),
            "Crawl ruft weiter ab"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let mut findings: Vec<String> = Vec::new();
    if state(&h.app) == Some(CrawlState::Completed) {
        findings.push(format!(
            "Sitzung meldet `Completed`, während {} Request(s) gehalten werden (Warteschlange nicht leer)",
            mock.held()
        ));
    }
    if meta(&path, "state").as_deref() == Some("Completed") {
        findings.push("Crawl-Datei trägt `Completed`, während Requests gehalten werden".into());
    }
    let start = Instant::now();
    loop {
        mock.release(Some(1)).await;
        tokio::time::sleep(Duration::from_millis(150)).await;
        if tokio::time::timeout(Duration::from_secs(3), os_app::session::poll(&h.app))
            .await
            .is_err()
        {
            blocked_polls += 1;
        }
        let open = q_i64(
            &path,
            "SELECT COUNT(*) FROM ai_answers WHERE question_id = 'd05-drain' AND status = 'Pending'",
        );
        if (state(&h.app) == Some(CrawlState::Completed)
            || meta(&path, "state").as_deref() == Some("Completed"))
            && open == 0
            && count_status(&path, "d05-drain", "Success") as i64 >= q_i64(&path, IN_SCOPE)
        {
            break;
        }
        assert!(
            start.elapsed() < Duration::from_secs(90),
            "Warteschlange wird nicht leer (Zustand {:?}, Datei {:?}, gehalten {}, gesendet {}, blockierte poll-Aufrufe {blocked_polls})",
            state(&h.app),
            meta(&path, "state"),
            mock.held(),
            mock.decisions().len()
        );
    }
    eprintln!("D05-DRAIN: blockierte poll-Aufrufe {blocked_polls}");
    if blocked_polls > 0 {
        findings.push(format!(
            "{blocked_polls} Aufruf(e) von session::poll länger als 3 s blockiert"
        ));
    }
    let in_scope = q_i64(&path, IN_SCOPE) as usize;
    assert!(in_scope >= 1);
    assert_eq!(
        mock.decisions().len(),
        in_scope,
        "Mock-Log = Einträge im Geltungsbereich der 10 URLs"
    );
    assert_eq!(
        count_status(&path, "d05-drain", "Success") as usize,
        in_scope
    );
    assert_eq!(count_status(&path, "d05-drain", "Pending"), 0);
    assert_eq!(meta(&path, "end_reason").as_deref(), Some("limit_reached"));
    assert_no_duplicate_urls(&mock);
    assert!(findings.is_empty(), "{findings:#?}");
}

// spec: AI-020 [M4]
// spec: AGENT-020
// acceptance: independent
// (c) foreign_writer_is_read_only (Kernteil) – „Crawl-Datei mit `owner_kind = cli`, `owner_pid` gesetzt und aktuellem
// (simuliertem) Heartbeat, offene KI-Einträge, alle Schlüssel verbunden → … der ai-mock der App zählt 0 Requests“; dazu der
// zweite Satz des Kriteriums „Besitzer eines KI-Laufs“ (`Completed`, `owner_kind = mcp-stdio`). Die Ablehnung nennt den
// Hinweis aus AGENT-020 („gestartet von <kind> (PID <n>) – Steuerung nur dort“).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn c_foreign_writer_is_read_only() {
    let _s = serial().await;
    let site = ai_site().await;
    for kind in ["cli", "mcp-stdio"] {
        let mock = Mock::start().await;
        let h = home(&mock, Opts::default());
        let mut cfg = profile(&site, &[question("d05-foreign", &["url", "title"])]);
        auto_off(&mut cfg);
        crawled(&h, &cfg).await;
        let path = crawl_path(&h.app);
        close(&h.app).await;
        k11::set_owner(&path, kind, k11::FOREIGN_PID, Some(SystemClock.now()));
        open(&h.app, &path).await;
        let err = run_ai(&h.app, true)
            .await
            .expect_err("fremd geführter Crawl: kein KI-Lauf");
        let text = error_text(&err);
        assert!(
            text.contains(kind) && text.contains(k11::FOREIGN_PID),
            "{kind}: Hinweis nennt Art und PID: {text}"
        );
        let _ = try_call(
            &h.app,
            "ai.costEstimate",
            json!({ "tasks": [], "sample": null, "budgetMicroUsd": null }),
        )
        .await;
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert_eq!(mock.provider_requests(), 0, "{kind}: 0 Requests");
        assert_eq!(
            owner_pid(&path),
            k11::FOREIGN_PID,
            "{kind}: Besitzerangaben unverändert"
        );
    }
}

// spec: AI-020 [M4]
// spec: AI-012 [M4]
// spec: CRAWL-126 [M4]
// acceptance: independent
// (c) resume_shows_no_estimate_dialog und (l) K4-27: Schwelle 1 µUSD (jede Schätzung bestätigungspflichtig): während des
// Crawls kein Request vor „Starten“ (`Pending (confirmation)`, AI-012); nach „Starten“ senden die Einträge; Pause, Neustart
// der App, Fortsetzen → dieselbe `ai_runs`-Zeile mit derselben `confirmed_at`, die übrigen Einträge gehen ohne neuen
// Voranschlag und ohne `Pending (confirmation)` hinaus, Endstand 22 `Success`, keine URL doppelt. „KI-Analyse starten“
// danach legt eine neue Zeile mit neuer Bestätigung an.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn c_resume_shows_no_estimate_dialog_and_keeps_the_run_row() {
    let _s = serial().await;
    let mock = Mock::start().await;
    let site = ai_site().await;
    let opts = Opts {
        threshold: 1,
        ..Opts::default()
    };
    let h = home(&mock, opts);
    let mut cfg = profile(&site, &[question("d05-resume", &["url", "title"])]);
    slow(&mut cfg);
    h.profile(&cfg);
    start_crawl(&h.app, START).await;
    let path = crawl_path(&h.app);
    wait_until(
        "Einträge warten auf Bestätigung",
        Duration::from_secs(60),
        || count_status(&path, "d05-resume", "Pending (confirmation)") >= 2,
    )
    .await;
    assert_eq!(mock.provider_requests(), 0, "kein Request vor „Starten“");
    call(&h.app, "ai.decisionsRun", json!({ "confirmed": true })).await;
    wait_until(
        "Antworten nach „Starten“",
        Duration::from_secs(60),
        || count_status(&path, "d05-resume", "Success") >= 3,
    )
    .await;
    call(&h.app, "crawl.pause", Value::Null).await;
    wait_state(&h.app, CrawlState::Paused, Duration::from_secs(30)).await;
    assert!(q_i64(&path, CRAWLED) < 25, "Pause vor dem Ende des Crawls");
    assert_eq!(q_i64(&path, "SELECT COUNT(*) FROM ai_runs"), 1);
    let confirmed = meta_row(&path);
    assert!(confirmed.is_some(), "confirmed_at gesetzt");

    close(&h.app).await;
    let h = h.restart();
    open(&h.app, &path).await;
    call(&h.app, "crawl.resume", Value::Null).await;
    wait_state(&h.app, CrawlState::Completed, Duration::from_secs(120)).await;
    assert_eq!(
        count_status(&path, "d05-resume", "Success"),
        22,
        "{:?}",
        statuses(&path, "d05-resume")
    );
    assert_eq!(
        count_status(&path, "d05-resume", "Pending (confirmation)"),
        0
    );
    assert_eq!(count_status(&path, "d05-resume", "Pending"), 0);
    assert_eq!(mock.decisions().len(), 22);
    assert_no_duplicate_urls(&mock);
    assert_eq!(
        q_i64(&path, "SELECT COUNT(*) FROM ai_runs"),
        1,
        "Fortsetzen = dieselbe Laufzeile"
    );
    assert_eq!(meta_row(&path), confirmed, "Bestätigung übernommen");

    run_ai(&h.app, true).await.expect("KI-Analyse starten");
    assert_eq!(
        q_i64(&path, "SELECT COUNT(*) FROM ai_runs"),
        2,
        "neuer Lauf = neue Zeile"
    );
    assert_eq!(
        q_i64(&path, "SELECT COUNT(*) FROM ai_runs WHERE id = (SELECT MAX(id) FROM ai_runs) AND confirmed_at IS NOT NULL"),
        1,
        "neue Zeile mit neuer Bestätigung"
    );
}

fn meta_row(path: &std::path::Path) -> Option<String> {
    ro(path)
        .conn()
        .query_row(
            "SELECT confirmed_at FROM ai_runs ORDER BY id LIMIT 1",
            [],
            |r| r.get::<_, Option<String>>(0),
        )
        .ok()
        .flatten()
}

// =================================================================================================================
// (e) Unterbrochener Crawl
// =================================================================================================================

// spec: AI-020 [M4]
// spec: AI-039 [M4]
// acceptance: independent
// Kriterium (e), „Kein KI-Lauf auf einem unterbrochenen Crawl“: gespeichert `Running`, `owner_pid` gesetzt, Heartbeat
// 11 s alt → `ai.decisionsRun` lehnt mit „Crawl ist unterbrochen – zuerst fortsetzen oder stoppen“ ab, die Datei ist
// byte-gleich, der abgeleitete Zustand bleibt „Unterbrochen“, 0 Requests; nach „Stoppen“ ohne Engine gelingt der Lauf.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn e_interrupted_crawl_is_refused_and_the_file_stays_byte_equal() {
    let _s = serial().await;
    let mock = Mock::start().await;
    let site = ai_site().await;
    let h = home(&mock, Opts::default());
    let mut cfg = profile(&site, &[question("d05-interrupted", &["url", "title"])]);
    auto_off(&mut cfg);
    crawled(&h, &cfg).await;
    let path = crawl_path(&h.app);
    close(&h.app).await;
    let now = SystemClock.now();
    {
        let f = os_store::CrawlFile::open(&path, os_store::OpenMode::ReadWrite).unwrap();
        f.set_meta("state", "Running").unwrap();
        k11::delete_meta(&f, "end_reason");
        k11::delete_meta(&f, "ended_at");
    }
    k11::set_owner(&path, "app", k11::FOREIGN_PID, Some(k11::ago(now, 11_000)));
    open(&h.app, &path).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(state(&h.app), Some(CrawlState::Incomplete));
    let before = k11::file_bytes(&path);
    let err = run_ai(&h.app, true)
        .await
        .expect_err("unterbrochen: abgelehnt");
    let text = error_text(&err);
    assert!(
        text.contains("Crawl ist unterbrochen – zuerst fortsetzen oder stoppen")
            || text.contains("Crawl was interrupted – resume or stop it first"),
        "Meldung: {text}"
    );
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(k11::file_bytes(&path), before, "Datei byte-gleich");
    assert_eq!(mock.provider_requests(), 0);
    let _ = os_app::session::poll(&h.app).await;
    assert_eq!(state(&h.app), Some(CrawlState::Incomplete));

    // „Stoppen“ ohne Engine wie CRAWL-125 (Commit `Completed`, Endgrund `stopped`, `owner_pid` leer).
    close(&h.app).await;
    k11::stop_without_engine(&path, SystemClock.now());
    open(&h.app, &path).await;
    run_ai(&h.app, true)
        .await
        .expect("nach „Stoppen“ gelingt der Start");
    assert_eq!(mock.decisions().len(), 22);
}

// =================================================================================================================
// (f) Kostenbestätigung
// =================================================================================================================

// spec: AI-012 [M4]
// acceptance: independent
// Kriterium (f): über der Schwelle kein Request vor „Starten“ – `ai.decisionsRun` ohne Bestätigung lässt die Einträge mit
// `Pending (confirmation)` warten (0 Requests), mit Bestätigung gehen alle 22 hinaus.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn f_no_request_before_start_above_the_threshold() {
    let _s = serial().await;
    let mock = Mock::start().await;
    let site = ai_site().await;
    let h = home(
        &mock,
        Opts {
            threshold: 1,
            ..Opts::default()
        },
    );
    let mut cfg = profile(&site, &[question("d05-confirm", &["url", "title"])]);
    auto_off(&mut cfg);
    crawled(&h, &cfg).await;
    let path = crawl_path(&h.app);
    let estimate = call(
        &h.app,
        "ai.costEstimate",
        json!({ "tasks": [], "sample": null, "budgetMicroUsd": null }),
    )
    .await;
    assert_eq!(
        mock.provider_requests(),
        0,
        "die Schätzung sendet nichts: {estimate}"
    );
    let _ = run_ai(&h.app, false).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(mock.provider_requests(), 0, "kein Request ohne „Starten“");
    assert_eq!(
        count_status(&path, "d05-confirm", "Pending (confirmation)"),
        22,
        "{:?}",
        statuses(&path, "d05-confirm")
    );
    run_ai(&h.app, true).await.expect("nach „Starten“");
    assert_eq!(mock.decisions().len(), 22);
    assert_eq!(count_status(&path, "d05-confirm", "Success"), 22);
}

// =================================================================================================================
// (g) Budget aus dem Konfigurationsdialog an der geöffneten Crawl-Datei
// =================================================================================================================

// spec: AI-013 [M4]
// acceptance: independent
// Kriterium (g): Änderung von `ai.budget` im Konfigurationsdialog (`config.set`) bei geöffneter Crawl-Datei →
// `open_ledger` liefert die neue Grenze; eine Erhöhung hebt eine Sperre nach AI-013 (nicht quittierte Überschreitung einer
// Reservierung) auf, danach sendet ein KI-Lauf wieder.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn g_budget_change_in_the_dialog_reaches_the_open_crawl_file() {
    let _s = serial().await;
    let mock = Mock::start().await;
    let site = ai_site().await;
    let h = home(&mock, Opts::default());
    let mut cfg = profile(&site, &[question("d05-budget", &["url", "title"])]);
    auto_off(&mut cfg);
    crawled(&h, &cfg).await;
    let path = crawl_path(&h.app);
    close(&h.app).await;
    {
        let f = os_store::CrawlFile::open(&path, os_store::OpenMode::ReadWrite).unwrap();
        let limits = os_model::budget::BudgetLimits {
            crawl: MicroUsd(1_000_000),
            ..Default::default()
        };
        os_store::ai::budget::save_limits(f.conn(), &limits).unwrap();
        f.conn()
            .execute(
                "INSERT INTO ai_runs(id, started_at, ended_at, config_json) VALUES (900, '2026-11-27T10:00:00.000Z', '2026-11-27T10:01:00.000Z', '{}')",
                [],
            )
            .unwrap();
        f.conn()
            .execute(
                "INSERT INTO ai_requests(run_id, timestamp, task, provider, model, endpoint, reservation_microusd, \
                 token_cost_microusd, cost_microusd, price_source, status, duration_ms, over_reservation) \
                 VALUES (900, '2026-11-27T10:00:30.000Z', 'decision', 'openai', 'gpt-6-luna', 'decisions', 5, 20, 20, 'table', 'Success', 1, 1)",
                [],
            )
            .unwrap();
    }
    let halted = |p: &std::path::Path| {
        let f = ro(p);
        let l = os_store::ai::budget::open_ledger(f.conn(), Default::default(), &[]).unwrap();
        os_model::budget::BudgetLedger::snapshot(&l).unwrap().halted
    };
    assert!(halted(&path), "Vorbedingung: Sperre nach Überschreitung");
    open(&h.app, &path).await;

    let dto = call(&h.app, "config.get", Value::Null).await;
    let mut current: CrawlConfig =
        serde_json::from_value(dto["config"].clone()).expect("CrawlConfig");
    current.ai.budget.crawl = MicroUsd(2_000_000);
    let set = serde_json::to_value(CrawlConfigDto {
        schema_version: current.schema_version,
        generator: current.generator.clone(),
        config: serde_json::to_value(&current).unwrap(),
    })
    .unwrap();
    call(&h.app, "config.set", set).await;

    let f = ro(&path);
    let limits = os_store::ai::budget::load_limits(f.conn())
        .unwrap()
        .expect("Grenzen gespeichert");
    assert_eq!(
        limits.crawl,
        MicroUsd(2_000_000),
        "open_ledger liefert die neue Grenze"
    );
    let ledger = os_store::ai::budget::open_ledger(f.conn(), Default::default(), &[]).unwrap();
    let snap = os_model::budget::BudgetLedger::snapshot(&ledger).unwrap();
    assert_eq!(snap.limits.crawl, MicroUsd(2_000_000));
    drop(f);
    assert!(!halted(&path), "Erhöhung hebt die Sperre auf");
    let budget = call(&h.app, "ai.costBudget", Value::Null).await;
    assert_eq!(
        find_key(&budget, "budgetMicroUsd"),
        Some(&json!(2_000_000)),
        "{budget}"
    );
    run_ai(&h.app, true).await.expect("KI-Lauf");
    assert_eq!(
        mock.decisions().len(),
        22,
        "nach der Erhöhung wird wieder gesendet"
    );
}

// =================================================================================================================
// (j) B3-2: ai.costBudget zeigt offene Reservierungen eines laufenden KI-Laufs
// =================================================================================================================

// spec: AI-013 [M4]
// acceptance: independent
// Kriterium (j): „Verbrauch = … + offene Reservierungen“ (AI-013), Tooltip mit offenen Reservierungen: Während Request 1
// gehalten wird, zeigt `ai.costBudget` eine offene Reservierung; nach dem Ende keine, Verbrauch = Summe der Protokollzeilen.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn j_cost_budget_shows_open_reservations_of_a_running_run() {
    let _s = serial().await;
    let mock = Mock::start().await;
    let site = ai_site().await;
    let h = home(&mock, Opts::default());
    let mut cfg = profile(&site, &[question("d05-reserved", &["url", "title"])]);
    auto_off(&mut cfg);
    crawled(&h, &cfg).await;
    let path = crawl_path(&h.app);
    mock.hold(1).await;
    let op = start_ai(&h.app, true).await;
    mock.wait_held(1).await;
    let during = call(&h.app, "ai.costBudget", Value::Null).await;
    let reserved = find_key(&during, "reservedMicroUsd")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    assert!(reserved > 0, "offene Reservierung sichtbar: {during}");
    let spent = find_key(&during, "spentMicroUsd")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    assert!(
        spent >= reserved,
        "Verbrauch enthält die offene Reservierung: {during}"
    );
    mock.release(None).await;
    wait_op(&h.app, &op).await.expect("Lauf endet");
    let after = call(&h.app, "ai.costBudget", Value::Null).await;
    assert_eq!(
        find_key(&after, "reservedMicroUsd"),
        Some(&json!(0)),
        "{after}"
    );
    let (_, _, logged) = budget_on_open(&path);
    assert_eq!(
        find_key(&after, "spentMicroUsd").and_then(Value::as_u64),
        Some(logged),
        "{after}"
    );
}

// =================================================================================================================
// (h) Besitz in jedem Ende-Pfad
// =================================================================================================================

/// Wartet höchstens 30 s auf das Ende einer Operation; Rückgabe `false` bei Zeitüberschreitung.
async fn ends_within(app: &os_app::App, op: &str) -> bool {
    tokio::time::timeout(Duration::from_secs(30), app.ops().wait(op))
        .await
        .is_ok()
}

#[derive(Debug, Clone, Copy)]
enum EndPath {
    Completed,
    NeedsConfirmation,
    Budget,
    ProviderError,
    Closed,
    StoppedInPause,
}

// spec: AI-020 [M4]
// acceptance: independent
// Kriterium (h): Jeder KI-Lauf trägt den Besitz und gibt ihn in jedem Ende-Pfad frei („am Ende des KI-Laufs (fertig,
// pausiert, gestoppt, Budget erreicht, Fehler, geordnetes Beenden) einmal `heartbeat_at` = jetzt mit leerem `owner_pid`“).
// Über die App erreichbar: fertig, Bestätigung ausstehend, Budget erreicht, Anbieterfehler, Schließen der Datei, Stoppen
// des pausierten Crawls während des Laufs. Die Pause eines Laufs prüft `crates/os-ai/tests/acceptance_d05_core.rs`.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn h_the_owner_is_released_in_every_end_path() {
    let _s = serial().await;
    let site = ai_site().await;
    let mut failures: Vec<String> = Vec::new();
    for path_kind in [
        EndPath::Completed,
        EndPath::NeedsConfirmation,
        EndPath::Budget,
        EndPath::ProviderError,
        EndPath::Closed,
        EndPath::StoppedInPause,
    ] {
        let mock = Mock::start().await;
        let opts = Opts {
            threshold: if matches!(path_kind, EndPath::NeedsConfirmation) {
                1
            } else {
                1_000_000
            },
            ..Opts::default()
        };
        let h = home(&mock, opts);
        let mut cfg = profile(&site, &[question("d05-end", &["url", "title"])]);
        auto_off(&mut cfg);
        if matches!(path_kind, EndPath::Budget) {
            cfg.ai.budget.crawl = MicroUsd(0);
        }
        let label = format!("{path_kind:?}");
        eprintln!("D05-ENDE: {label}");
        let path;
        match path_kind {
            EndPath::StoppedInPause => {
                slow(&mut cfg);
                h.profile(&cfg);
                start_crawl(&h.app, START).await;
                path = crawl_path(&h.app);
                wait_until("einige URLs", Duration::from_secs(60), || {
                    q_i64(&path, CRAWLED) >= 6
                })
                .await;
                call(&h.app, "crawl.pause", Value::Null).await;
                wait_state(&h.app, CrawlState::Paused, Duration::from_secs(30)).await;
            }
            _ => {
                crawled(&h, &cfg).await;
                path = crawl_path(&h.app);
            }
        }
        let before = meta(&path, "heartbeat_at").unwrap_or_default();
        match path_kind {
            EndPath::Completed => {
                mock.hold(1).await;
                let op = start_ai(&h.app, true).await;
                mock.wait_held(1).await;
                assert_eq!(
                    owner_pid(&path),
                    own_pid(),
                    "{label}: Besitz während des Laufs"
                );
                mock.release(None).await;
                if !ends_within(&h.app, &op).await {
                    failures.push(format!("{label}: Lauf endet nicht in 30 s"));
                    continue;
                }
            }
            EndPath::NeedsConfirmation => {
                if tokio::time::timeout(Duration::from_secs(60), run_ai(&h.app, false))
                    .await
                    .is_err()
                {
                    failures.push(format!("{label}: Lauf endet nicht in 60 s"));
                    continue;
                }
                assert_eq!(mock.provider_requests(), 0, "{label}");
            }
            EndPath::Budget => {
                if tokio::time::timeout(Duration::from_secs(60), run_ai(&h.app, true))
                    .await
                    .is_err()
                {
                    failures.push(format!("{label}: Lauf endet nicht in 60 s"));
                    continue;
                }
                assert_eq!(
                    count_status(&path, "d05-end", "Budget exceeded"),
                    22,
                    "{label}: {:?}",
                    statuses(&path, "d05-end")
                );
            }
            EndPath::ProviderError => {
                // Antwort nicht parsebar → `Error: Invalid output` (AI-020), endgültig ohne Wiederholung nach INTEG-005.
                mock.scenario("openai", "malformed-json", 1_000).await;
                if tokio::time::timeout(Duration::from_secs(60), run_ai(&h.app, true))
                    .await
                    .is_err()
                {
                    failures.push(format!(
                        "{label}: Lauf endet nicht in 60 s ({} Requests)",
                        mock.decisions().len()
                    ));
                    continue;
                }
                assert!(!mock.decisions().is_empty(), "{label}");
            }
            EndPath::Closed => {
                mock.hold(1).await;
                let op = start_ai(&h.app, true).await;
                mock.wait_held(1).await;
                assert_eq!(
                    owner_pid(&path),
                    own_pid(),
                    "{label}: Besitz während des Laufs"
                );
                if let Err(e) = close_during_run(&h.app, &mock).await {
                    failures.push(format!(
                        "{label}: Schließen während des KI-Laufs abgelehnt: {e:?}"
                    ));
                    let _ = ends_within(&h.app, &op).await;
                    continue;
                }
            }
            EndPath::StoppedInPause => {
                mock.hold(1).await;
                let op = start_ai(&h.app, true).await;
                mock.wait_held(1).await;
                assert_eq!(
                    owner_pid(&path),
                    own_pid(),
                    "{label}: Besitz während des Laufs"
                );
                call(&h.app, "crawl.stop", Value::Null).await;
                mock.release(None).await;
                if !ends_within(&h.app, &op).await {
                    failures.push(format!(
                        "{label}: Lauf endet nach dem Stoppen nicht in 30 s"
                    ));
                    continue;
                }
            }
        }
        let start = Instant::now();
        while !owner_pid(&path).is_empty() && start.elapsed() < Duration::from_secs(10) {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        if !owner_pid(&path).is_empty() {
            failures.push(format!("{label}: owner_pid nach dem Ende nicht leer"));
            continue;
        }
        let after = meta(&path, "heartbeat_at").unwrap_or_default();
        if after <= before {
            failures.push(format!(
                "{label}: heartbeat_at am Ende nicht geschrieben ({before} → {after})"
            ));
        }
        assert_spec_statuses(&path);
    }
    // Aufräumen: hängende Läufe dieses Tests beenden, damit das Testende nicht auf sie wartet.
    os_ai::run::controls().abort_all();
    assert!(failures.is_empty(), "{failures:#?}");
}

// =================================================================================================================
// (m) K4-28: Stub-Test prüft genau die Commands mit Stub-Handler
// =================================================================================================================

// spec: UI-004 [M4]
// acceptance: independent
// Kriterium (m), K4-28 a: Ergänzung zum umgebauten F02-Test. Jeder Command des KI-Katalogs ist entweder in der Liste der
// Stubs (und antwortet `err-not-implemented`) oder umgesetzt (antwortet nicht `err-not-implemented`); `ai.decisionsRun`
// und `ai.costEstimate` sind keine Stubs. So kann die Liste keinen Stub verbergen.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn m_every_catalog_command_is_a_listed_stub_or_implemented() {
    let _s = serial().await;
    let mock = Mock::start().await;
    let h = home(
        &mock,
        Opts {
            preview: false,
            key: false,
            ..Opts::default()
        },
    );
    let stubs = os_app::commands::ai_catalog::stub_commands();
    assert!(
        !stubs.contains(&"ai.decisionsRun") && !stubs.contains(&"ai.costEstimate"),
        "{stubs:?}"
    );
    for c in os_app::commands::ai_catalog::CATALOG {
        let result = try_call(&h.app, c.name, json!({})).await;
        let is_stub_answer = matches!(&result, Err(e) if e.message_key == "err-not-implemented");
        assert_eq!(
            is_stub_answer,
            stubs.contains(&c.name),
            "{}: Stub-Liste und Verhalten stimmen überein ({result:?})",
            c.name
        );
    }
    assert_eq!(mock.provider_requests(), 0);
}
