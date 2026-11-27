//! M4-D05: KI-Warteschlange während des Crawls (`os_ai::run::DecisionQueue`, Umsetzung von `os_model::ai::hook::AiQueue`)
//! gegen den ai-mock – gespeicherte KI-Eingaben auch ohne aktive Fragen (AI-032, Nachtrag K4-24), automatische
//! Ausführung (AI-039, E-6 a) und ihre Abschaltung, Crawl-Ende ohne Stoppen (Warteschlange bis leer), Pause und Fortsetzen
//! (dieselbe Laufzeile, genau die offenen Einträge), Abbruch (08 AI-020 [M4], Nachtrag K4-13). Die Seiten liefert der Test
//! so, wie die Engine sie übergibt.
#[path = "run_support.rs"]
mod rs;

use std::sync::Arc;
use std::time::Duration;

use os_ai::run::{DecisionQueue, EnvFactory, RunEnv};
use os_model::ai::hook::{AiQueue, AiQueueEnd, AiQueueStart, InputSource, PageAiFields};
use os_model::CrawlConfig;
use rs::*;

fn queue(env: RunEnv) -> DecisionQueue {
    let factory: EnvFactory = Arc::new(move |_, _| Ok(env.clone()));
    DecisionQueue::new(factory)
}

fn start(path: &std::path::Path, cfg: &CrawlConfig, resumed: bool, pages: Option<Vec<PageAiFields>>) -> AiQueueStart {
    AiQueueStart {
        crawl_file: path.to_path_buf(),
        config: cfg.clone(),
        resumed,
        owner_kind: "app",
        existing: pages.map(|p| {
            Arc::new(move |_: &std::path::Path, _: &CrawlConfig| Ok(p.clone())) as os_model::ai::hook::PageLoader
        }),
    }
}

fn parsed(mut pages: Vec<PageAiFields>) -> Vec<PageAiFields> {
    for p in &mut pages {
        p.source = InputSource::Parsed;
    }
    pages
}

fn stored_inputs(path: &std::path::Path) -> u64 {
    let f = os_store::CrawlFile::open(path, os_store::OpenMode::ReadOnly).unwrap();
    os_store::ai::inputs::size(f.conn()).unwrap().entries
}

// spec: AI-032 [M4]
// Schalter an, Option „KI-Eingaben speichern“ an (Standard), keine aktiven Fragen → je interner HTML-Seite mit Status 200
// ein Eintrag, für Weiterleitungen und Fehlerseiten keiner; kein KI-Request. Option aus und keine Fragen → keine Sitzung.
#[tokio::test]
async fn inputs_are_stored_for_html_200_pages_without_active_questions() {
    let mock = Mock::start().await;
    let dir = tmp("d05-q-store");
    let pages = parsed(site_pages("site"));
    let path = crawl_file(&dir.0, &pages);
    let cfg = CrawlConfig::default();
    assert!(cfg.ai.store_inputs.get(), "Standard an");
    let session = queue(env(&mock, &["openai"], NO_CONFIRMATION))
        .begin(start(&path, &cfg, false, None))
        .expect("Sitzung zum Speichern");
    for p in &pages {
        session.page(p.clone());
    }
    session.end(AiQueueEnd::Drain).await;
    assert_eq!(stored_inputs(&path), 23, "23 interne HTML-Seiten mit Status 200 (ohne 404 und 301)");
    assert_eq!(mock.log().len(), 0, "kein KI-Request");

    let mut off = CrawlConfig::default();
    off.ai.store_inputs = os_model::config::DefaultOn(false);
    assert!(queue(env(&mock, &["openai"], NO_CONFIRMATION))
        .begin(start(&path, &off, false, None))
        .is_none());
}

// spec: AI-039 [M4]
// spec: AI-020 [M4]
// Automatische Ausführung (Standard an): Ergebnisse während des Crawls; Crawl-Ende ohne Stoppen → die Zukunft von `end`
// wird erst fertig, wenn die Warteschlange leer ist (gehaltener Request), danach 22 `Success`, 3 `Skipped (scope)`.
#[tokio::test]
async fn automatic_run_drains_before_the_end_is_reported() {
    let mock = Mock::start().await;
    let dir = tmp("d05-q-auto");
    let pages = parsed(site_pages("site"));
    let path = crawl_file(&dir.0, &pages);
    let cfg = config(&[question_json("page-type")], 5_000_000);
    let session = queue(env(&mock, &["openai"], NO_CONFIRMATION))
        .begin(start(&path, &cfg, false, None))
        .unwrap();
    mock.hold("openai", 1).await;
    for p in &pages {
        session.page(p.clone());
    }
    let mut end = session.end(AiQueueEnd::Drain);
    mock.until("gehalten", |_| mock.held(DECISIONS) >= 1).await;
    assert!(
        tokio::time::timeout(Duration::from_millis(300), &mut end).await.is_err(),
        "Ende erst mit leerer Warteschlange"
    );
    mock.release("openai", 1).await;
    end.await;
    let c = counts(&path, "page-type");
    assert_eq!(c.get("Success"), Some(&22), "{c:?}");
    assert_eq!(c.get("Skipped (scope)"), Some(&3));
    assert_eq!(decisions_posts(&mock), 22);
    assert_eq!(logged(&path), 22);
}

// spec: AI-039 [M4]
// Option „automatische Ausführung“ aus → 0 Requests bis „KI-Analyse starten“ (die Eingaben werden trotzdem gespeichert).
#[tokio::test]
async fn automatic_run_off_sends_nothing() {
    let mock = Mock::start().await;
    let dir = tmp("d05-q-off");
    let pages = parsed(site_pages("site"));
    let path = crawl_file(&dir.0, &pages);
    let mut cfg = config(&[question_json("page-type")], 5_000_000);
    cfg.ai.auto_run = os_model::config::DefaultOn(false);
    let session = queue(env(&mock, &["openai"], NO_CONFIRMATION))
        .begin(start(&path, &cfg, false, None))
        .unwrap();
    for p in &pages {
        session.page(p.clone());
    }
    session.end(AiQueueEnd::Drain).await;
    assert_eq!(mock.log().len(), 0);
    assert_eq!(stored_inputs(&path), 23);
    assert!(counts(&path, "page-type").is_empty());
}

// spec: AI-020 [M4]
// Pause (Request 6 gehalten) → nach dessen Freigabe kein weiterer Request; Fortsetzen (neue Sitzung, `resumed`, Leser der
// gecrawlten Seiten) → genau die offenen Einträge, Endstand 22 `Success`, 22 Protokollzeilen, keine URL doppelt im Mock-Log,
// dieselbe Laufzeile.
#[tokio::test]
async fn pause_and_resume_send_each_entry_once() {
    let mock = Mock::start().await;
    let dir = tmp("d05-q-pause");
    let pages = parsed(site_pages("site"));
    let path = crawl_file(&dir.0, &pages);
    let cfg = config(&[question_json("page-type")], 5_000_000);
    let q = queue(env(&mock, &["openai"], NO_CONFIRMATION));
    let session = q.begin(start(&path, &cfg, false, None)).unwrap();
    mock.hold("openai", 6).await;
    for p in &pages {
        session.page(p.clone());
    }
    for i in 1..=5 {
        mock.until("Request gehalten", |_| mock.posts(DECISIONS).len() >= i).await;
        mock.release("openai", 1).await;
    }
    mock.until("Request 6 gehalten", |_| mock.posts(DECISIONS).len() >= 6).await;
    let paused = session.end(AiQueueEnd::Paused);
    paused.await;
    mock.release("openai", 1).await;
    // Die Sitzung übernimmt Request 6 und sendet danach nichts mehr.
    let runs_before = os_ai::run::controls().running(&path);
    for _ in 0..500 {
        if os_ai::run::controls().running(&path) == 0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(runs_before <= 1);
    assert_eq!(decisions_posts(&mock), 6, "nach der Pause kein weiterer Request");
    let c = counts(&path, "page-type");
    assert_eq!(c.get("Success"), Some(&6), "{c:?}");
    let run_id = {
        let f = os_store::CrawlFile::open(&path, os_store::OpenMode::ReadOnly).unwrap();
        os_ai::run::job::open_run(f.conn()).unwrap().expect("offene Laufzeile")
    };
    // Fortsetzen.
    let resumed = q.begin(start(&path, &cfg, true, Some(pages.clone()))).unwrap();
    resumed.end(AiQueueEnd::Drain).await;
    assert_eq!(decisions_posts(&mock), 22, "genau 16 weitere");
    assert_eq!(logged(&path), 22);
    assert_eq!(counts(&path, "page-type").get("Success"), Some(&22));
    let bodies = mock.bodies(DECISIONS);
    let mut ids: Vec<String> = bodies
        .iter()
        .map(|b| {
            let text = b.to_string();
            let at = text.find("MOCK-ID:").unwrap();
            text[at..].split(|c: char| c.is_whitespace() || c == '"' || c == '\\').next().unwrap().to_string()
        })
        .collect();
    ids.sort();
    let before = ids.len();
    ids.dedup();
    assert_eq!(ids.len(), before, "keine URL doppelt im Mock-Log");
    let f = os_store::CrawlFile::open(&path, os_store::OpenMode::ReadOnly).unwrap();
    let rows: i64 = f
        .conn()
        .query_row("SELECT COUNT(*) FROM ai_requests WHERE run_id = ?1", [run_id], |r| r.get(0))
        .unwrap();
    assert_eq!(rows, 22, "dieselbe Laufzeile");
}

// spec: AI-020 [M4]
// Abbruch (Fehler bzw. geordnetes Beenden): laufender Request verworfen, Eintrag `Pending`, keine Protokollzeile.
#[tokio::test]
async fn abort_discards_the_running_request() {
    let mock = Mock::start().await;
    let dir = tmp("d05-q-abort");
    let pages = parsed(site_pages("site"));
    let path = crawl_file(&dir.0, &pages);
    let cfg = config(&[question_json("page-type")], 5_000_000);
    let session = queue(env(&mock, &["openai"], NO_CONFIRMATION))
        .begin(start(&path, &cfg, false, None))
        .unwrap();
    mock.hold("openai", 1).await;
    for p in &pages {
        session.page(p.clone());
    }
    mock.until("gehalten", |_| mock.held(DECISIONS) >= 1).await;
    session.end(AiQueueEnd::Abort).await;
    for _ in 0..500 {
        if os_ai::run::controls().running(&path) == 0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(logged(&path), 0);
    assert_eq!(decisions_posts(&mock), 1);
    assert_eq!(counts(&path, "page-type").get("Success"), None);
}

// spec: AI-012 [M4]
// spec: AI-020 [M4]
// Während des Crawls über der Schwelle (Testprofil 0,0001 USD): kein Request ohne „Starten“, Einträge
// `Pending (confirmation)`; „Starten“ (`confirm_running`) → `confirmed_at` der Laufzeile, die wartenden Einträge gehen hinaus.
// Pause mitten im Lauf und Fortsetzen (neue Sitzung): kein neuer Voranschlag, die offenen Einträge gehen ohne
// `Pending (confirmation)` hinaus (ersetzt den Kernteil von `decisions_pending::resume_shows_no_estimate_dialog`).
#[tokio::test]
async fn confirmation_during_the_crawl_and_resume_without_a_new_estimate() {
    let mock = Mock::start().await;
    let dir = tmp("d05-q-confirm");
    let pages = parsed(site_pages("site"));
    let path = crawl_file(&dir.0, &pages);
    let cfg = config(&[question_json("page-type")], 5_000_000);
    let threshold = os_model::budget::MicroUsd(100);
    let q = queue(env(&mock, &["openai"], threshold));
    let session = q.begin(start(&path, &cfg, false, None)).unwrap();
    for p in &pages {
        session.page(p.clone());
    }
    let waiting = || counts(&path, "page-type").get("Pending (confirmation)").copied() == Some(22);
    for _ in 0..1000 {
        if waiting() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(waiting(), "{:?}", counts(&path, "page-type"));
    assert_eq!(decisions_posts(&mock), 0, "kein Request vor „Starten“");
    mock.hold("openai", 6).await;
    assert!(os_ai::run::confirm_running(&path));
    for i in 1..=5 {
        mock.until("Request gehalten", |_| mock.posts(DECISIONS).len() >= i).await;
        mock.release("openai", 1).await;
    }
    mock.until("Request 6 gehalten", |_| mock.posts(DECISIONS).len() >= 6).await;
    session.end(AiQueueEnd::Paused).await;
    mock.release("openai", 1).await;
    for _ in 0..500 {
        if os_ai::run::controls().running(&path) == 0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(decisions_posts(&mock), 6);
    {
        let f = os_store::CrawlFile::open(&path, os_store::OpenMode::ReadOnly).unwrap();
        let run = os_ai::run::job::open_run(f.conn()).unwrap().unwrap();
        assert!(os_store::ai::answers::confirmed_at(f.conn(), run).unwrap().is_some(), "confirmed_at");
    }
    // Fortsetzen (wie nach einem Neustart: neue Sitzung) mit derselben Schwelle: kein neuer Voranschlag.
    let resumed = queue(env(&mock, &["openai"], threshold))
        .begin(start(&path, &cfg, true, Some(pages.clone())))
        .unwrap();
    resumed.end(AiQueueEnd::Drain).await;
    let c = counts(&path, "page-type");
    assert_eq!(c.get("Success"), Some(&22), "{c:?}");
    assert_eq!(c.get("Pending (confirmation)"), None);
    assert_eq!(decisions_posts(&mock), 22);
}
