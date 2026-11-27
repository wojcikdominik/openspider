//! M4-D05: KI-Lauf über den Laufrahmen `os_ai::run` gegen den ai-mock – unterbrochener und fremd geführter Crawl,
//! Kostenbestätigung und ihre Fortgeltung über einen Neustart (K4-27), Kontingentsperre je KI-Lauf (Auflage E8, K4-29),
//! gemeinsamer Ledger (Befund B3-2 U01), `Skipped (input unavailable)` (AI-032, Punkt 3, K4-26).
#[path = "run_support.rs"]
mod rs;

use os_ai::decisions::exec::{RunEnd, Selection};
use os_ai::run::{controls, run_decisions, JobConfirmation, JobError, INTERRUPTED_DE, INTERRUPTED_EN};
use os_model::ai::hook::InputSource;
use os_model::budget::MicroUsd;
use rs::*;

fn meta_set(path: &std::path::Path, pairs: &[(&str, &str)]) {
    let f = os_store::CrawlFile::open(path, os_store::OpenMode::ReadWrite).unwrap();
    for (k, v) in pairs {
        f.set_meta(k, v).unwrap();
    }
}

fn iso(offset_ms: i64) -> String {
    use os_model::Clock;
    let now = os_model::SystemClock.now();
    os_model::Timestamp::from_unix_millis(now.unix_millis() + offset_ms).to_db()
}

/// Bytes der Crawl-Datei samt WAL (byte-gleich = nichts geschrieben).
fn bytes(path: &std::path::Path) -> Vec<u8> {
    let mut b = std::fs::read(path).unwrap();
    if let Ok(w) = std::fs::read(path.with_extension("ospider-wal")) {
        b.extend(w);
    }
    b
}

// spec: AI-020 [M4]
// „Kein KI-Lauf auf einem unterbrochenen Crawl“: gespeichert `Running`, `owner_pid` gesetzt, Heartbeat 11 s alt → Ablehnung
// mit „Crawl ist unterbrochen – zuerst fortsetzen oder stoppen“ (en „Crawl was interrupted – resume or stop it first“),
// Datei byte-gleich, 0 Requests; nach „Stoppen“ (gespeichert `Completed`) gelingt der Start.
#[tokio::test]
async fn interrupted_crawl_is_rejected_without_request_and_unchanged() {
    let mock = Mock::start().await;
    let dir = tmp("d05-interrupted");
    let pages = site_pages("site");
    let path = crawl_file(&dir.0, &pages);
    meta_set(
        &path,
        &[("state", "Running"), ("owner_kind", "app"), ("owner_pid", "424242"), ("heartbeat_at", &iso(-11_000))],
    );
    {
        // WAL in die Hauptdatei übernehmen, damit der Vergleich nur echte Schreibzugriffe sieht.
        let f = os_store::CrawlFile::open(&path, os_store::OpenMode::ReadWrite).unwrap();
        f.conn().execute_batch("PRAGMA wal_checkpoint(TRUNCATE);").unwrap();
    }
    let before = bytes(&path);
    let cfg = config(&[question_json("page-type")], 5_000_000);
    let env = env(&mock, &["openai"], NO_CONFIRMATION);
    let err = run_decisions(&env, job(&path, &cfg, &pages, JobConfirmation::Confirmed))
        .await
        .unwrap_err();
    assert_eq!(err, JobError::Interrupted);
    assert_eq!(err.to_string(), INTERRUPTED_DE);
    assert_eq!(INTERRUPTED_DE, "Crawl ist unterbrochen – zuerst fortsetzen oder stoppen");
    assert_eq!(INTERRUPTED_EN, "Crawl was interrupted – resume or stop it first");
    assert_eq!(bytes(&path), before, "Datei byte-gleich");
    assert_eq!(decisions_posts(&mock), 0);
    // Nach „Stoppen“ ohne Engine: abgeschlossener Crawl, der Lauf startet.
    meta_set(&path, &[("state", "Completed"), ("owner_pid", "")]);
    let outcome = run_decisions(&env, job(&path, &cfg, &pages, JobConfirmation::Confirmed))
        .await
        .unwrap();
    assert_eq!(outcome.end, RunEnd::Completed);
    assert_eq!(decisions_posts(&mock), 22);
}

// spec: AI-020 [M4]
// Fremder Writer (`owner_kind = cli` bzw. `mcp-stdio`, fremde PID, aktueller Heartbeat): Ablehnung mit Art und PID, 0 Requests.
#[tokio::test]
async fn foreign_active_owner_is_rejected_without_request() {
    let mock = Mock::start().await;
    let dir = tmp("d05-foreign");
    let pages = site_pages("site");
    let path = crawl_file(&dir.0, &pages);
    for kind in ["cli", "mcp-stdio"] {
        meta_set(&path, &[("owner_kind", kind), ("owner_pid", "424242"), ("heartbeat_at", &iso(0))]);
        let cfg = config(&[question_json("page-type")], 5_000_000);
        let env = env(&mock, &["openai"], NO_CONFIRMATION);
        let err = run_decisions(&env, job(&path, &cfg, &pages, JobConfirmation::Confirmed))
            .await
            .unwrap_err();
        assert_eq!(err, JobError::Occupied { kind: kind.into(), pid: "424242".into() });
        assert!(err.to_string().contains(&format!("gestartet von {kind} (PID 424242)")));
    }
    assert_eq!(decisions_posts(&mock), 0);
}

// spec: AI-012 [M4]
// spec: AI-020 [M4]
// Kostenbestätigung (K4-27): über der Schwelle ohne Bestätigung kein Request, Einträge `Pending (confirmation)`; mit
// Bestätigung `confirmed_at` in der Laufzeile. Pause mitten im Lauf, „Neustart“ (neue Umgebung, Datei neu geöffnet),
// Fortsetzen: dieselbe Laufzeile, kein neuer Voranschlag, genau die offenen Einträge. „KI-Analyse starten“ danach: neue
// Zeile, neue Bestätigung.
#[tokio::test]
async fn confirmation_is_kept_in_the_run_row_across_a_restart() {
    let mock = Mock::start().await;
    let dir = tmp("d05-confirm");
    let pages = site_pages("site");
    let path = crawl_file(&dir.0, &pages);
    let cfg = config(&[question_json("page-type")], 5_000_000);
    let threshold = MicroUsd(100); // Testprofil AI-012: 0,0001 USD
    {
        let env = env(&mock, &["openai"], threshold);
        let outcome = run_decisions(&env, job(&path, &cfg, &pages, JobConfirmation::Unconfirmed))
            .await
            .unwrap();
        assert_eq!(outcome.end, RunEnd::NeedsConfirmation);
        assert!(outcome.estimate.is_some(), "Voranschlag");
        assert_eq!(decisions_posts(&mock), 0, "kein Request vor „Starten“");
        assert_eq!(counts(&path, "page-type").get("Pending (confirmation)"), Some(&22));
    }
    let first_run;
    {
        let env = env(&mock, &["openai"], threshold);
        mock.hold("openai", 6).await;
        let run = run_decisions(&env, job(&path, &cfg, &pages, JobConfirmation::Confirmed));
        let driver = async {
            for i in 1..=5 {
                mock.until("Request gehalten", |_| mock.posts(DECISIONS).len() >= i).await;
                mock.release("openai", 1).await;
            }
            mock.until("Request 6 gehalten", |_| mock.posts(DECISIONS).len() >= 6).await;
            controls().pause(&path);
            mock.release("openai", 1).await;
        };
        let (outcome, ()) = tokio::join!(run, driver);
        let outcome = outcome.unwrap();
        assert_eq!(outcome.end, RunEnd::Paused);
        assert!(outcome.estimate.is_none());
        first_run = outcome.run_id;
        let f = os_store::CrawlFile::open(&path, os_store::OpenMode::ReadOnly).unwrap();
        assert!(os_store::ai::answers::confirmed_at(f.conn(), first_run).unwrap().is_some());
    }
    assert_eq!(decisions_posts(&mock), 6);
    assert_eq!(counts(&path, "page-type").get("Success"), Some(&6));
    // Neustart: neue Umgebung, neue Adapter; Fortsetzen derselben Zeile ohne Voranschlag trotz Schwelle 0,0001 USD.
    let env = env(&mock, &["openai"], threshold);
    let outcome = run_decisions(&env, job(&path, &cfg, &pages, JobConfirmation::Resume))
        .await
        .unwrap();
    assert_eq!(outcome.run_id, first_run, "dieselbe Laufzeile");
    assert_eq!(outcome.end, RunEnd::Completed);
    assert!(outcome.estimate.is_none(), "kein neuer Kostenvoranschlag");
    assert_eq!(decisions_posts(&mock), 22, "genau 16 weitere");
    assert_eq!(counts(&path, "page-type").get("Success"), Some(&22));
    // Neuer Lauf: neue Zeile, ohne Bestätigung → Voranschlag (hier nichts offen, also nichts zu bestätigen).
    let outcome = run_decisions(&env, job(&path, &cfg, &pages, JobConfirmation::Unconfirmed))
        .await
        .unwrap();
    assert_ne!(outcome.run_id, first_run, "neue Laufzeile");
}

// spec: AI-020 [M4]
// Auflage E8 (K4-29, 08 AI-020 „Kontingentsperre je KI-Lauf“): zwei gleichzeitige Läufe mit eigenen Adapter-Instanzen; der
// eine erhält `insufficient-quota` (1 Request, Rest `Quota exceeded` ohne Request), der andere sendet weiter; ein danach
// gestarteter Lauf versucht es wieder.
#[tokio::test]
async fn quota_latch_is_per_run() {
    let mock = Mock::start().await;
    let (da, db) = (tmp("d05-quota-a"), tmp("d05-quota-b"));
    let pages = site_pages("site");
    let (pa, pb) = (crawl_file(&da.0, &pages), crawl_file(&db.0, &pages));
    let cfg = config(&[question_json("page-type")], 5_000_000);
    let env = env(&mock, &["openai"], NO_CONFIRMATION);
    mock.scenario("openai", "insufficient-quota", 1).await;
    let (a, b) = tokio::join!(
        run_decisions(&env, job(&pa, &cfg, &pages, JobConfirmation::Confirmed)),
        run_decisions(&env, job(&pb, &cfg, &pages, JobConfirmation::Confirmed)),
    );
    a.unwrap();
    b.unwrap();
    let (ca, cb) = (counts(&pa, "page-type"), counts(&pb, "page-type"));
    let (tripped, other) = if ca.get("Quota exceeded") == Some(&22) { (&pa, cb.clone()) } else { (&pb, ca.clone()) };
    assert_eq!(counts(tripped, "page-type").get("Quota exceeded"), Some(&22), "{ca:?} {cb:?}");
    assert_eq!(other.get("Success"), Some(&22), "der andere Lauf sendet weiter: {other:?}");
    assert_eq!(decisions_posts(&mock), 23, "1 Kontingent-Antwort + 22 Requests des anderen Laufs");
    // Ein danach gestarteter Lauf („Fehlgeschlagene erneut anfragen“) versucht es wieder.
    let mut retry = job(tripped, &cfg, &pages, JobConfirmation::Confirmed);
    retry.selection = Selection::Failed;
    run_decisions(&env, retry).await.unwrap();
    assert_eq!(counts(tripped, "page-type").get("Success"), Some(&22));
    assert_eq!(decisions_posts(&mock), 45);
}

// spec: AI-013 [M4]
// Befund B3-2 der Abnahme U01: Während ein Request gehalten wird, zeigt der gemeinsame Ledger der Datei die offene
// Reservierung des laufenden KI-Laufs; nach dem Ende gibt es keinen aktiven Ledger mehr.
#[tokio::test]
async fn active_ledger_shows_open_reservations_of_a_running_run() {
    let mock = Mock::start().await;
    let dir = tmp("d05-ledger");
    let pages = site_pages("site");
    let path = crawl_file(&dir.0, &pages);
    let cfg = config(&[question_json("page-type")], 5_000_000);
    let env = env(&mock, &["openai"], NO_CONFIRMATION);
    mock.hold("openai", 1).await;
    let run = run_decisions(&env, job(&path, &cfg, &pages, JobConfirmation::Confirmed));
    let driver = async {
        mock.until("gehalten", |_| mock.held(DECISIONS) >= 1).await;
        let ledger = controls().active_ledger(&path).expect("Ledger des laufenden Laufs");
        let snapshot = os_model::budget::BudgetLedger::snapshot(&*ledger).unwrap();
        assert!(snapshot.reserved.0 > 0, "offene Reservierung sichtbar: {snapshot:?}");
        mock.release("openai", 1).await;
    };
    let (outcome, ()) = tokio::join!(run, driver);
    outcome.unwrap();
    assert!(controls().active_ledger(&path).is_none());
}

// spec: AI-032 [M4]
// Nachtrag „Quelle der Eingabefelder“, Punkt 3 (K4-24) und K4-26: Seiten ohne gespeicherte bzw. ableitbare Eingaben →
// Fragen mit Seitentext `Skipped (input unavailable)` ohne Request, eine Title-Frage läuft normal; `structured_data_types`
// NULL (Extraktion aus) → `Skipped (input unavailable)`, leere Liste → die Frage läuft.
#[tokio::test]
async fn missing_inputs_are_skipped_and_title_questions_run() {
    let mock = Mock::start().await;
    let dir = tmp("d05-unavailable");
    let mut pages = site_pages("site");
    for p in &mut pages {
        p.source = InputSource::Unavailable;
        p.page_text = None;
    }
    let path = crawl_file(&dir.0, &pages);
    let cfg = config(
        &[run_question("text-only"), run_question("title-only"), run_question("structured-types")],
        5_000_000,
    );
    let env = env(&mock, &["openai"], NO_CONFIRMATION);
    let outcome = run_decisions(&env, job(&path, &cfg, &pages, JobConfirmation::Confirmed))
        .await
        .unwrap();
    assert_eq!(outcome.end, RunEnd::Completed);
    assert_eq!(counts(&path, "text-only").get("Skipped (input unavailable)"), Some(&22));
    assert_eq!(counts(&path, "structured-types").get("Skipped (input unavailable)"), Some(&22));
    assert_eq!(counts(&path, "title-only").get("Success"), Some(&22));
    assert_eq!(counts(&path, "title-only").get("Skipped (scope)"), Some(&3));
    // Mit Extraktion (leere Liste = extrahiert, nichts gefunden) läuft die Frage.
    let dir2 = tmp("d05-unavailable-2");
    let mut with_types = site_pages("site");
    for p in &mut with_types {
        p.structured_data_types = Some(Vec::new());
    }
    let path2 = crawl_file(&dir2.0, &with_types);
    let cfg2 = config(&[run_question("structured-types")], 5_000_000);
    run_decisions(&env, job(&path2, &cfg2, &with_types, JobConfirmation::Confirmed))
        .await
        .unwrap();
    assert_eq!(counts(&path2, "structured-types").get("Success"), Some(&22));
}

// spec: AI-032 [M4]
// Speichergrenze 20.000 Zeichen (bestätigt 26.11.2026): Das Fragenschema begrenzt `max_chars` auf 20.000.
#[test]
fn question_schema_limits_max_chars_to_twenty_thousand() {
    let mut q = run_question("all-fields");
    assert!(os_ai::decisions::format::parse(&q.to_string()).is_ok());
    q["inputs"][5]["max_chars"] = serde_json::json!(20_001);
    assert!(os_ai::decisions::format::parse(&q.to_string()).is_err());
}
