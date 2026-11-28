//! M4-D05, Nacharbeit nach der Abnahme (28.11.2026): KI-Warteschlange im Crawl-Lebenszyklus der App-Sitzung
//! (`crawl.start`, `session::poll`) gegen den ai-mock – Crawl-Ende ohne Stoppen: Endstatus ohne Wartezeit, Crawl-Analyse
//! erst mit leerer Warteschlange (Befund A3, 08 AI-020 Nachtrag 28.11.2026); Start bei laufendem eigenem Crawl wird
//! abgewiesen, außer als Antwort auf einen wartenden Kostenvoranschlag (R1).
#![allow(clippy::disallowed_types, clippy::disallowed_methods)]
#[path = "ai_run_support.rs"]
mod support;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use os_app::dto::{StartRequest, StartScope, StartSource};
use os_app::{App, ErrorCode, WindowId};
use os_model::{CrawlConfig, CrawlState};
use os_store::{CrawlFile, OpenMode};
use serde_json::{json, Value};
use support::*;

fn window() -> WindowId {
    WindowId("main".into())
}

fn state(app: &App) -> Option<CrawlState> {
    app.sessions().current(&window()).map(|s| s.state)
}

fn path_of(app: &App) -> PathBuf {
    app.sessions().current(&window()).unwrap().path
}

fn meta(path: &Path, key: &str) -> Option<String> {
    CrawlFile::open(path, OpenMode::ReadOnly)
        .unwrap()
        .get_meta(key)
        .unwrap()
}

fn q(path: &Path, sql: &str) -> i64 {
    CrawlFile::open(path, OpenMode::ReadOnly)
        .unwrap()
        .conn()
        .query_row(sql, [], |r| r.get::<_, Option<i64>>(0))
        .unwrap()
        .unwrap_or(0)
}

/// KI-Fixture-Site, je Seite `delay_ms` verzögert.
async fn site(delay_ms: u64) -> (tempfile::TempDir, os_testsite::TestSite) {
    let dir = tempfile::tempdir().unwrap();
    let src = os_testsite::fixtures_root().join("ai/site/scenario.toml");
    let toml = std::fs::read_to_string(src)
        .unwrap()
        .replace("[[route]]", &format!("[[route]]\ndelay_ms = {delay_ms}"));
    std::fs::write(dir.path().join("scenario.toml"), toml).unwrap();
    let site = os_testsite::TestSite::start_dir(dir.path(), 0)
        .await
        .unwrap();
    (dir, site)
}

fn profile(site: &os_testsite::TestSite) -> CrawlConfig {
    let mut cfg = CrawlConfig::default();
    cfg.speed.threads = 1;
    cfg.analysis.auto_at_end = false;
    cfg.network.resolve = vec![format!("*.test={}", site.addr())];
    with_questions(&mut cfg, &[page_type()]);
    cfg
}

async fn start(app: &App, cfg: &CrawlConfig) {
    cfg.save_default(&app.config().paths.settings).unwrap();
    let req = StartRequest {
        source: StartSource::Spider {
            url: "http://ai.test/".into(),
        },
        scope: StartScope::Domain,
        profile: None,
    };
    call(app, "crawl.start", serde_json::to_value(req).unwrap())
        .await
        .expect("crawl.start");
}

async fn timed_poll(app: &App) -> Duration {
    let t0 = Instant::now();
    let _ = tokio::time::timeout(Duration::from_secs(5), os_app::session::poll(app)).await;
    t0.elapsed()
}

fn analysis_started(path: &Path) -> bool {
    matches!(
        meta(path, "analysis_state").as_deref(),
        Some("running") | Some("completed")
    )
}

// spec: AI-020 [M4]
// spec: CRAWL-141
// spec: UI-018 [M4]
// Befund A3, Nachtrag 28.11.2026 („Vollständig“ beim Crawl-Ende ohne Stoppen): `limits.max_urls` = 10, alle Requests
// gehalten. Solange gehalten wird: gespeicherter Zustand `Completed` (`limit_reached`), die Sitzung meldet `Completed`,
// jede Statusabfrage (`session::poll`) antwortet innerhalb von 500 ms, die automatische Crawl-Analyse hat nicht begonnen.
// Nach der Freigabe aller Requests: alle Einträge im Geltungsbereich `Success`, danach läuft die Analyse.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn crawl_end_reports_completed_at_once_and_analysis_waits_for_the_queue() {
    let mock = Mock::start().await;
    let home = tempfile::tempdir().unwrap();
    let app = app_with(home.path(), &mock, true, NO_CONFIRMATION);
    grant(&app, &[page_type()]);
    let (_dir, site) = site(0).await;
    let mut cfg = profile(&site);
    cfg.limits.max_urls = 10;
    cfg.analysis.auto_at_end = true;
    mock.hold(1_000).await;
    start(&app, &cfg).await;
    let path = path_of(&app);
    let mut slowest = Duration::ZERO;
    let t0 = Instant::now();
    while meta(&path, "state").as_deref() != Some("Completed") {
        slowest = slowest.max(timed_poll(&app).await);
        assert!(t0.elapsed() < Duration::from_secs(60), "Crawl endet nicht");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    mock.until("gehaltener Request", |m| m.held() >= 1).await;
    // Phase „nur noch KI-Warteschlange“.
    let watch = Instant::now();
    while watch.elapsed() < Duration::from_millis(1_500) {
        slowest = slowest.max(timed_poll(&app).await);
        assert_eq!(meta(&path, "end_reason").as_deref(), Some("limit_reached"));
        assert!(
            !analysis_started(&path),
            "Analyse wartet auf die leere Warteschlange"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert_eq!(
        state(&app),
        Some(CrawlState::Completed),
        "Sitzung meldet den Endzustand"
    );
    assert!(
        slowest <= Duration::from_millis(500),
        "Statusabfrage ohne Wartezeit (längste {slowest:?})"
    );
    let in_scope = q(
        &path,
        "SELECT COUNT(*) FROM urls WHERE is_internal = 1 AND status_code = 200 AND content_class = 'HTML' \
         AND indexability = 'Indexable'",
    ) as u64;
    assert!(in_scope >= 1);
    let t0 = Instant::now();
    while counts(&path).get("Success").copied().unwrap_or(0) < in_scope {
        assert!(
            !analysis_started(&path),
            "Analyse erst mit leerer Warteschlange"
        );
        mock.release(1).await;
        tokio::time::sleep(Duration::from_millis(100)).await;
        slowest = slowest.max(timed_poll(&app).await);
        assert!(
            t0.elapsed() < Duration::from_secs(90),
            "Warteschlange wird nicht leer"
        );
    }
    let t0 = Instant::now();
    while meta(&path, "analysis_state").as_deref() != Some("completed") {
        let _ = os_app::session::poll(&app).await;
        assert!(
            t0.elapsed() < Duration::from_secs(60),
            "Analyse nach leerer Warteschlange"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(slowest <= Duration::from_millis(500), "längste {slowest:?}");
    assert_eq!(
        mock.decisions() as u64,
        in_scope,
        "Mock-Log = Einträge im Geltungsbereich"
    );
    assert_eq!(counts(&path).get("Pending"), None);
}

// spec: AI-020 [M4]
// spec: AI-012 [M4]
// spec: AI-039 [M4]
// R1, Nachtrag 28.11.2026 („Start während des eigenen laufenden Crawls“): `ai.decisionsRun` bei laufendem Crawl desselben
// Prozesses wird abgewiesen (`CrawlRunning`, kein Request, keine Laufzeile, Zustand `Running`) – mit automatischer
// Ausführung aus und mit automatischer Ausführung an, wenn nichts auf eine Bestätigung wartet. Einzige Ausnahme: Der
// KI-Teil wartet auf die Kostenbestätigung (Einträge `Pending (confirmation)`) – dann wird der bestätigte Auftrag
// angenommen (`{"end": "Confirmed"}`, `confirmed_at` in der Laufzeile dieses Crawls, keine neue Zeile), die Einträge gehen
// hinaus.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn start_during_the_own_running_crawl_is_refused_except_as_confirmation() {
    let (_dir, site) = site(300).await;
    // (1) automatische Ausführung aus bzw. (2) an ohne wartende Einträge → abgewiesen.
    for auto in [false, true] {
        let mock = Mock::start().await;
        let home = tempfile::tempdir().unwrap();
        let app = app_with(home.path(), &mock, true, NO_CONFIRMATION);
        grant(&app, &[page_type()]);
        let mut cfg = profile(&site);
        cfg.ai.auto_run = os_model::config::DefaultOn(auto);
        mock.hold(1_000).await;
        start(&app, &cfg).await;
        let path = path_of(&app);
        let t0 = Instant::now();
        while q(
            &path,
            "SELECT COUNT(*) FROM urls WHERE status_code IS NOT NULL",
        ) < 3
        {
            assert!(t0.elapsed() < Duration::from_secs(60));
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        let runs = q(&path, "SELECT COUNT(*) FROM ai_runs");
        let requests = mock.decisions();
        for confirmed in [true, false] {
            let err = call(
                &app,
                "ai.decisionsRun",
                json!({"tasks": [], "confirmed": confirmed}),
            )
            .await
            .expect_err("abgewiesen");
            assert_eq!(err.code, ErrorCode::CrawlRunning, "auto {auto}: {err:?}");
            assert_eq!(err.message_key, "err-ai-run-crawl-running");
        }
        tokio::time::sleep(Duration::from_millis(300)).await;
        if !auto {
            assert_eq!(mock.decisions(), 0, "kein Request");
        } else {
            // Die gehaltenen Requests gehören zur automatischen Ausführung; der Auftrag sendet nichts zusätzlich.
            assert!(mock.decisions() <= requests + 1);
        }
        assert_eq!(
            q(&path, "SELECT COUNT(*) FROM ai_runs"),
            runs,
            "keine Laufzeile"
        );
        let _ = os_app::session::poll(&app).await;
        assert_eq!(
            state(&app),
            Some(CrawlState::Running),
            "Zustand unverändert"
        );
        mock.hold(0).await;
        mock.release(1_000).await;
        call(&app, "crawl.stop", Value::Null).await.unwrap();
    }

    // (3) Automatische Ausführung an, Einträge warten auf die Bestätigung → angenommen.
    let mock = Mock::start().await;
    let home = tempfile::tempdir().unwrap();
    let app = app_with(home.path(), &mock, true, 1);
    grant(&app, &[page_type()]);
    start(&app, &profile(&site)).await;
    let path = path_of(&app);
    let t0 = Instant::now();
    while counts(&path)
        .get("Pending (confirmation)")
        .copied()
        .unwrap_or(0)
        < 2
    {
        assert!(
            t0.elapsed() < Duration::from_secs(60),
            "{:?}",
            counts(&path)
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(mock.decisions(), 0, "kein Request vor der Bestätigung");
    let runs = q(&path, "SELECT COUNT(*) FROM ai_runs");
    let unconfirmed = call(
        &app,
        "ai.decisionsRun",
        json!({"tasks": [], "confirmed": false}),
    )
    .await
    .expect_err("ohne Bestätigung abgewiesen");
    assert_eq!(unconfirmed.code, ErrorCode::CrawlRunning);
    let answer = call(
        &app,
        "ai.decisionsRun",
        json!({"tasks": [], "confirmed": true}),
    )
    .await
    .expect("Antwort auf den Kostenvoranschlag");
    assert_eq!(answer, json!({"end": "Confirmed"}));
    mock.until("Requests nach der Bestätigung", |m| m.decisions() >= 2)
        .await;
    assert_eq!(
        q(&path, "SELECT COUNT(*) FROM ai_runs"),
        runs,
        "keine neue Laufzeile"
    );
    assert_eq!(
        q(
            &path,
            "SELECT COUNT(*) FROM ai_runs WHERE confirmed_at IS NOT NULL"
        ),
        1,
        "confirmed_at in der Laufzeile dieses Crawls"
    );
    call(&app, "crawl.stop", Value::Null).await.unwrap();
}
