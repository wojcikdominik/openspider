//! M4-D05: KI-Warteschlange der App während des Crawls (08 AI-039, E-6 a; AI-020 [M4], Nachtrag K4-13; AI-032 mit den
//! Nachträgen K4-24/K4-26) gegen den ai-mock: Ergebnisse während des Crawls, Option „automatische Ausführung“ aus und
//! Schalter aus (0 Requests), Eingabefelder aus `fields.html` wie gesendet – während des Crawls und nach dem Crawl –,
//! gespeicherte KI-Eingaben (Größe für den Bericht), Besitz bei KI während des eigenen laufenden Crawls.
#![allow(clippy::disallowed_types, clippy::disallowed_methods)]
#[path = "ai_run_support.rs"]
mod support;

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use os_app::App;
use os_engine::testing::{crawl_fixture_with, test_deps, FixtureCrawl};
use os_model::{CrawlConfig, SystemClock};
use os_store::{CrawlFile, OpenMode};
use serde_json::{json, Value};
use support::*;

fn deps(app: &App) -> os_engine::EngineDeps {
    let mut d = test_deps();
    // Uhr wie in der App (Besitz und Heartbeat des KI-Laufs nach der Systemuhr).
    d.clock = Arc::new(SystemClock);
    d.owner_kind = "app";
    d.ai_queue = os_app::commands::ai_run::app_queue(app);
    d
}

async fn app_for(mock: &Mock, home: &Path, on: bool) -> App {
    let app = app_with(home, mock, on, NO_CONFIRMATION);
    grant(
        &app,
        &[
            page_type(),
            question("tests/fixtures/ai/run/questions/all-fields.question.json"),
        ],
    );
    app
}

fn bodies(mock: &Mock) -> Vec<Value> {
    mock.log()
        .iter()
        .filter(|r| r.path == DECISIONS)
        .map(|r| serde_json::from_str(&r.body).unwrap())
        .collect()
}

fn inputs_size(fx: &FixtureCrawl) -> os_store::ai::inputs::InputsSize {
    os_store::ai::inputs::size(fx.file.conn()).unwrap()
}

// spec: AI-039 [M4]
// spec: AI-020 [M4]
// Crawl der Fixture-Site mit aktiver Seitentyp-Frage (automatische Ausführung, Standard an): Ergebnisse entstehen während
// des Crawls (Protokollzeilen vor dem Ende des Crawls), am Ende 22 `Success`, 3 `Skipped (scope)`, 22 Requests.
#[tokio::test]
async fn results_arrive_during_the_crawl() {
    let mock = Mock::start().await;
    let home = tempfile::tempdir().unwrap();
    let app = app_for(&mock, home.path(), true).await;
    let mut cfg = CrawlConfig::default();
    with_questions(&mut cfg, &[page_type()]);
    let fx = crawl_fixture_with("ai/site", "http://ai.test/", cfg, deps(&app)).await;
    let path = fx.dir.path().join("crawl.ospider");
    let c = counts(&path);
    assert_eq!(
        c.get("Success"),
        Some(&22),
        "{c:?}; {:?}",
        os_ai::run::notes()
    );
    assert_eq!(c.get("Skipped (scope)"), Some(&3));
    assert_eq!(mock.decisions(), 22);
    let ended = fx.file.get_meta("ended_at").unwrap().unwrap();
    let first: String = fx
        .file
        .conn()
        .query_row("SELECT MIN(timestamp) FROM ai_requests", [], |r| r.get(0))
        .unwrap();
    assert!(
        first < ended,
        "erste Antwort ({first}) vor dem Ende des Crawls ({ended})"
    );
    let size = inputs_size(&fx);
    assert_eq!(size.entries, 23, "interne HTML-Seiten mit Status 200");
    eprintln!(
        "D05-GRÖSSE ai/site: {} Einträge, body {} Bytes, übrige Spalten {} Bytes",
        size.entries, size.body_bytes, size.other_bytes
    );
}

// spec: AI-039 [M4]
// Option „automatische Ausführung“ aus → 0 Requests bis „KI-Analyse starten“; Schalter „KI (Vorschau)“ aus → 0 Requests,
// keine gespeicherten KI-Eingaben (Verhalten wie 1.0).
#[tokio::test]
async fn automatic_run_off_and_switch_off_send_nothing() {
    let mock = Mock::start().await;
    let home = tempfile::tempdir().unwrap();
    let app = app_for(&mock, home.path(), true).await;
    let mut cfg = CrawlConfig::default();
    with_questions(&mut cfg, &[page_type()]);
    cfg.ai.auto_run = os_model::config::DefaultOn(false);
    let fx = crawl_fixture_with("ai/site", "http://ai.test/", cfg.clone(), deps(&app)).await;
    assert_eq!(mock.log().len(), 0);
    assert_eq!(
        inputs_size(&fx).entries,
        23,
        "Eingaben trotzdem gespeichert"
    );

    let off = app_for(&mock, home.path(), false).await;
    cfg.ai.auto_run = os_model::config::DefaultOn(true);
    let fx = crawl_fixture_with("ai/site", "http://ai.test/", cfg, deps(&off)).await;
    assert_eq!(mock.log().len(), 0);
    assert_eq!(
        inputs_size(&fx).entries,
        0,
        "Schalter aus: nichts gespeichert"
    );
}

fn input_of(bodies: &[Value], marker: &str) -> Value {
    bodies
        .iter()
        .find(|b| b.to_string().contains(marker))
        .map(|b| b["input"].clone())
        .unwrap_or_else(|| panic!("kein Request mit {marker}"))
}

// spec: AI-032 [M4]
// Eingabefelder aus `fields.html` (Title, Meta Description, H1, alle H2, ONPAGE-140-Text, Wortzahl, `html_lang`,
// `hreflang_self`, strukturierte Datentypen) wie gesendet im Mock-Log während des Crawls; nach dem Crawl sendet
// „KI-Analyse starten“ mit einer Frage derselben Felder dieselbe Eingabe aus den gespeicherten KI-Eingaben – ohne
// gespeichertes HTML und ohne Seitenabruf (K4-24, K4-26).
#[tokio::test]
async fn fields_are_sent_alike_during_and_after_the_crawl() {
    let mock = Mock::start().await;
    let dir = tempfile::tempdir().unwrap();
    let app = app_for(&mock, &dir.path().join("home"), true).await;
    let all = question("tests/fixtures/ai/run/questions/all-fields.question.json");
    let mut cfg = CrawlConfig::default();
    cfg.extraction.structured_data.json_ld = true;
    assert!(!cfg.extraction.store_html, "ohne gespeichertes HTML");
    with_questions(&mut cfg, std::slice::from_ref(&all));
    let fx = crawl_fixture_with("ai/run/site", "/", cfg.clone(), deps(&app)).await;
    let during = input_of(&bodies(&mock), "MOCK-ID:fields");
    let text = during.to_string();
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
            "{marker} fehlt im gesendeten Text: {text}"
        );
    }
    assert_eq!(inputs_size(&fx).entries, 4);
    let size = inputs_size(&fx);
    eprintln!(
        "D05-GRÖSSE ai/run/site: {} Einträge, body {} Bytes, übrige Spalten {} Bytes",
        size.entries, size.body_bytes, size.other_bytes
    );

    // Nach dem Crawl: „KI-Analyse starten“ mit einer zweiten Frage derselben Felder.
    let site_requests = fx.site.requests().len();
    let mut second = all.clone();
    second["id"] = json!("all-fields-after");
    second["name"] = json!("All Fields After");
    let mut after_cfg = cfg.clone();
    with_questions(&mut after_cfg, &[all, second]);
    let path = dir.path().join("after.ospider");
    fx.file.checkpoint_and_truncate().unwrap();
    std::fs::copy(fx.dir.path().join("crawl.ospider"), &path).unwrap();
    {
        let f = CrawlFile::open(&path, OpenMode::ReadWrite).unwrap();
        f.set_config(&after_cfg).unwrap();
    }
    open_in(&app, &path);
    let before = bodies(&mock).len();
    let op = call(
        &app,
        "ai.decisionsRun",
        json!({"tasks": ["all-fields-after"], "confirmed": true}),
    )
    .await
    .unwrap();
    let status = wait_op(&app, &op).await;
    assert_eq!(status.result.unwrap()["end"], "Completed");
    let after_bodies = bodies(&mock)[before..].to_vec();
    let after = input_of(&after_bodies, "MOCK-ID:fields");
    assert_eq!(after, during, "gleiche Eingabe wie während des Crawls");
    assert_eq!(fx.site.requests().len(), site_requests, "0 Seitenabrufe");
}

// spec: AI-020 [M4]
// Ersetzt den Platzhalter `decisions_lifecycle::ai_inside_the_own_running_crawl_keeps_the_owner`: KI während des laufenden
// Crawls desselben Prozesses – nach den ersten KI-Antworten trägt die Datei weiter `owner_pid` des Prozesses, der Crawl
// ist `Running`; den Heartbeat schreibt die Engine.
#[tokio::test]
async fn ai_inside_the_own_running_crawl_keeps_the_owner() {
    let mock = Mock::start().await;
    let home = tempfile::tempdir().unwrap();
    let app = app_for(&mock, home.path(), true).await;
    // Langsame Kopie der KI-Fixture-Site (300 ms je Seite).
    let site_dir = tempfile::tempdir().unwrap();
    let src = os_testsite::fixtures_root().join("ai/site/scenario.toml");
    let toml = std::fs::read_to_string(src)
        .unwrap()
        .replace("[[route]]", "[[route]]\ndelay_ms = 300");
    std::fs::write(site_dir.path().join("scenario.toml"), toml).unwrap();
    let site = os_testsite::TestSite::start_dir(site_dir.path(), 0)
        .await
        .unwrap();
    let mut cfg = CrawlConfig::default();
    cfg.speed.threads = 1;
    cfg.network.resolve = vec![format!("*.test={}", site.addr())];
    with_questions(&mut cfg, &[page_type()]);
    let work = tempfile::tempdir().unwrap();
    let path = work.path().join("crawl.ospider");
    let deps = deps(&app);
    let fetch =
        os_fetch::FetchConfig::from_crawl_config(&cfg, &*deps.secrets, deps.clock.clone()).unwrap();
    let handle = os_engine::start_crawl(
        Arc::new(os_fetch::HttpFetcher::new(fetch).unwrap()),
        os_engine::CrawlRequest {
            config: cfg,
            source: os_engine::CrawlSource::Spider {
                start_url: "http://ai.test/".into(),
            },
            file: path.clone(),
            project: "d05".into(),
            name: "d05".into(),
        },
        deps,
    )
    .await
    .unwrap();
    mock.until("erste KI-Antworten", |m| m.decisions() >= 2)
        .await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    {
        let f = CrawlFile::open(&path, OpenMode::ReadOnly).unwrap();
        assert_eq!(f.get_meta("state").unwrap().as_deref(), Some("Running"));
        assert_eq!(
            f.get_meta("owner_pid").unwrap(),
            Some(std::process::id().to_string())
        );
    }
    handle.stop();
    let summary = handle.wait().await.unwrap();
    assert_eq!(summary.end_reason, Some(os_model::EndReason::Stopped));
}

// spec: AI-020 [M4]
// spec: AI-039 [M4]
// Kernteil von `decisions_pending::ai_run_during_a_paused_crawl` (die Schaltfläche baut M4-D02): automatische Ausführung
// aus, Crawl pausiert nach mindestens 10 gecrawlten URLs, `ai.decisionsRun` → KI-Requests nur für Einträge dieser URLs,
// die Zahl der gecrawlten URLs ändert sich nicht, der Crawl bleibt `Paused`.
#[tokio::test]
async fn ai_run_during_a_paused_crawl_touches_only_crawled_urls() {
    let mock = Mock::start().await;
    let home = tempfile::tempdir().unwrap();
    let app = app_for(&mock, home.path(), true).await;
    let site_dir = tempfile::tempdir().unwrap();
    let src = os_testsite::fixtures_root().join("ai/site/scenario.toml");
    let toml = std::fs::read_to_string(src)
        .unwrap()
        .replace("[[route]]", "[[route]]\ndelay_ms = 200");
    std::fs::write(site_dir.path().join("scenario.toml"), toml).unwrap();
    let site = os_testsite::TestSite::start_dir(site_dir.path(), 0)
        .await
        .unwrap();
    let mut cfg = CrawlConfig::default();
    cfg.speed.threads = 1;
    cfg.network.resolve = vec![format!("*.test={}", site.addr())];
    with_questions(&mut cfg, &[page_type()]);
    cfg.ai.auto_run = os_model::config::DefaultOn(false);
    let work = tempfile::tempdir().unwrap();
    let path = work.path().join("crawl.ospider");
    let deps = deps(&app);
    let fetch =
        os_fetch::FetchConfig::from_crawl_config(&cfg, &*deps.secrets, deps.clock.clone()).unwrap();
    let handle = os_engine::start_crawl(
        Arc::new(os_fetch::HttpFetcher::new(fetch).unwrap()),
        os_engine::CrawlRequest {
            config: cfg,
            source: os_engine::CrawlSource::Spider {
                start_url: "http://ai.test/".into(),
            },
            file: path.clone(),
            project: "d05".into(),
            name: "d05".into(),
        },
        deps,
    )
    .await
    .unwrap();
    for _ in 0..3000 {
        if handle.progress().crawled >= 10 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    handle.pause();
    let paused = handle.wait().await.unwrap();
    assert_eq!(paused.state, os_model::CrawlState::Paused);
    assert_eq!(mock.log().len(), 0, "automatische Ausführung aus");
    let crawled = |p: &Path| -> i64 {
        CrawlFile::open(p, OpenMode::ReadOnly)
            .unwrap()
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM urls WHERE status_code IS NOT NULL",
                [],
                |r| r.get(0),
            )
            .unwrap()
    };
    let before = crawled(&path);
    let in_scope: i64 = CrawlFile::open(&path, OpenMode::ReadOnly)
        .unwrap()
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM urls WHERE is_internal = 1 AND status_code = 200 AND content_class = 'HTML' \
             AND indexability = 'Indexable'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    open_in(&app, &path);
    let op = call(
        &app,
        "ai.decisionsRun",
        json!({"tasks": [], "confirmed": true}),
    )
    .await
    .unwrap();
    let status = wait_op(&app, &op).await;
    assert_eq!(status.result.unwrap()["end"], "Completed");
    assert_eq!(
        mock.decisions() as i64,
        in_scope,
        "Requests nur für die gecrawlten URLs im Geltungsbereich"
    );
    assert_eq!(crawled(&path), before, "gecrawlte URLs unverändert");
    let f = CrawlFile::open(&path, OpenMode::ReadOnly).unwrap();
    assert_eq!(f.get_meta("state").unwrap().as_deref(), Some("Paused"));
    assert_eq!(f.get_meta("owner_pid").unwrap().unwrap_or_default(), "");
}
