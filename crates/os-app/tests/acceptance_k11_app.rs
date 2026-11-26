//! Unabhängige Abnahme M4-K11 (os-app): Crawl-Verwaltung, Öffnen und Prüfung beim Auslösen bei einem aktiven
//! Besitzer außerhalb von `Running`; unverändertes Verhalten für Dateien aus 1.0.
//!
//! Erwartetes Verhalten ausschließlich aus 08 AI-020, Nachtrag 26.11.2026, Unterpunkt „Aktiver Besitzer außerhalb
//! von `Running`“ (Wirkung a und b, Kriterien 2 und 6, K4-13-Kriterium „Besitzer eines KI-Laufs“, zweiter Satz ohne
//! ai-mock), OUT-228 (`busy`), AGENT-020 (nur lesend) und 01 CRAWL-125, Nachtrag K4-22. Echte Dispatcher-Aufrufe;
//! Heartbeat simuliert, Uhr der App eine `ManualClock`.
#![allow(clippy::disallowed_types, clippy::disallowed_methods)]

#[path = "../../os-store/tests/acceptance_k11_support/mod.rs"]
mod k11;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use k11::*;
use os_app::dto::{CmdError, CrawlSummaryDto, OpState, OwnerKind};
use os_app::{App, AppConfig, CallCtx, ErrorCode};
use os_model::{Clock, CrawlState, ManualClock, SeededIds, Timestamp};
use serde_json::{json, Value};

fn app_at(home: &Path, now: Timestamp) -> (App, Arc<ManualClock>) {
    let clock = Arc::new(ManualClock::new(now));
    let config = AppConfig::new(home)
        .with_ids(Arc::new(SeededIds::new(11)))
        .with_clock(clock.clone());
    (App::new(config).unwrap(), clock)
}

fn crawls_dir(app: &App) -> PathBuf {
    let dir = app.config().paths.crawls.clone();
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn ctx() -> CallCtx {
    CallCtx::for_window("main")
}

/// Ergebnis eines Befehls; Operationen werden abgewartet und ihr Fehler zurückgegeben.
async fn outcome(app: &App, name: &str, args: Value) -> Result<Value, CmdError> {
    let value = app.dispatch(ctx(), name, args).await?;
    let Some(op) = value.get("opId").and_then(Value::as_str) else {
        return Ok(value);
    };
    let status = tokio::time::timeout(Duration::from_secs(30), app.ops().wait(op))
        .await
        .unwrap()
        .expect("Ergebnis der Operation");
    match status.error {
        Some(e) => Err(e),
        None => {
            assert_eq!(status.state, OpState::Done, "{name}");
            Ok(status.result.unwrap_or(Value::Null))
        }
    }
}

async fn list(app: &App) -> Vec<Value> {
    app.dispatch(ctx(), "crawls.list", json!({"show": "all"}))
        .await
        .unwrap()
        .as_array()
        .unwrap()
        .clone()
}

fn entry<'a>(items: &'a [Value], id: &str) -> &'a Value {
    items
        .iter()
        .find(|i| i["crawl"]["crawlId"] == id)
        .unwrap_or_else(|| panic!("{id} nicht in der Liste"))
}

async fn open(app: &App, path: &Path) -> CrawlSummaryDto {
    let v = app
        .dispatch(ctx(), "crawl.open", json!({"kind": "path", "path": path}))
        .await
        .unwrap();
    serde_json::from_value(v).unwrap()
}

fn phase(state: CrawlState) -> &'static str {
    match state {
        CrawlState::Running => "running",
        CrawlState::Paused => "paused",
        CrawlState::Completed => "completed",
        CrawlState::Incomplete => "incomplete",
        CrawlState::Failed => "failed",
    }
}

fn owner_keys(path: &Path) -> Vec<Option<String>> {
    [
        "state",
        "owner_pid",
        "owner_kind",
        "heartbeat_at",
        "end_reason",
    ]
    .iter()
    .map(|k| get(path, k))
    .collect()
}

// spec: AI-020 [M4]
// spec: OUT-228
// acceptance: independent
#[tokio::test]
async fn management_answers_busy_for_any_active_owner_and_allows_deleting_eleven_seconds_later() {
    // Kriterium 2 und Wirkung (a): Löschen, Verschieben, Exportieren, Sperren und Entsperren → `busy`, für jede PID;
    // 11 s später ohne neuen Heartbeat gelingt das Löschen.
    for (i, (state, pid)) in [
        (CrawlState::Completed, FOREIGN_PID.to_string()),
        (CrawlState::Completed, own_pid()),
        (CrawlState::Paused, FOREIGN_PID.to_string()),
        (CrawlState::Incomplete, own_pid()),
    ]
    .into_iter()
    .enumerate()
    {
        let home = TempDir::new("app-manage");
        let (app, clock) = app_at(home.path(), base_now());
        let dir = crawls_dir(&app);
        let (id, path) = new_crawl(&dir, 10 + i as u64, state);
        set_owner(&path, "mcp-stdio", &pid, Some(clock.now()));
        let id = id.to_string();
        let label = format!("{state:?}, PID {pid}");
        let before = owner_keys(&path);
        let export = home.path().join("export.ospider");
        let other = home.path().join("anderswo");
        std::fs::create_dir_all(&other).unwrap();
        for (name, args) in [
            ("crawls.delete", json!({"ids": [id]})),
            ("crawls.move", json!({"ids": [id], "project": null})),
            (
                "crawls.move",
                json!({"ids": [id], "project": null, "destinationDir": other}),
            ),
            ("crawls.export", json!({"ids": [id], "dest": export})),
            ("crawls.lock", json!({"ids": [id]})),
            ("crawls.unlock", json!({"ids": [id]})),
        ] {
            let err = outcome(&app, name, args)
                .await
                .expect_err(&format!("{name}: {label}"));
            assert_eq!(err.code, ErrorCode::Busy, "{name}: {label}");
        }
        assert!(path.exists() && !export.exists(), "{label}");
        assert_eq!(owner_keys(&path), before, "{label}");

        let items = list(&app).await;
        let e = entry(&items, &id);
        assert_eq!(
            e["crawl"]["phase"],
            phase(state),
            "abgeleiteter Zustand: {label}"
        );
        assert_eq!(e["locked"], false);

        clock.advance(Duration::from_millis(10_000));
        let err = outcome(&app, "crawls.delete", json!({"ids": [id]}))
            .await
            .expect_err("10 s: noch belegt");
        assert_eq!(err.code, ErrorCode::Busy, "{label}");
        clock.advance(Duration::from_millis(1_000));
        outcome(&app, "crawls.delete", json!({"ids": [id]}))
            .await
            .unwrap_or_else(|e| panic!("11 s später löschbar: {label}: {e:?}"));
        assert!(!path.exists(), "{label}");
    }
}

// spec: AI-020 [M4]
// spec: CRAWL-128
// acceptance: independent
#[tokio::test]
async fn the_list_keeps_the_state_and_offers_no_resume_while_a_foreign_owner_is_active() {
    // „Der abgeleitete Zustand … bleibt“; Wirkung (b): Fortsetzen abgewiesen wie bei einem fremd geführten laufenden
    // Crawl – die Verwaltung bietet es nicht an, bis der Heartbeat veraltet ist.
    let home = TempDir::new("app-list");
    let (app, clock) = app_at(home.path(), base_now());
    let dir = crawls_dir(&app);
    let (foreign, fpath) = new_crawl(&dir, 20, CrawlState::Paused);
    set_owner(&fpath, "cli", FOREIGN_PID, Some(clock.now()));
    let (mine, mpath) = new_crawl(&dir, 21, CrawlState::Paused);
    set_owner(&mpath, "app", &own_pid(), Some(clock.now()));
    let (completed, cpath) = new_crawl(&dir, 22, CrawlState::Completed);
    set_owner(&cpath, "mcp-stdio", FOREIGN_PID, Some(clock.now()));

    let items = list(&app).await;
    let f = entry(&items, &foreign.to_string());
    assert_eq!(f["crawl"]["phase"], "paused");
    assert_eq!(f["canResume"], false);
    assert_eq!(f["crawl"]["owner"]["controllable"], false);
    let m = entry(&items, &mine.to_string());
    assert_eq!(m["crawl"]["phase"], "paused");
    assert_eq!(m["canResume"], true, "eigene PID: (b) greift nicht");
    let c = entry(&items, &completed.to_string());
    assert_eq!(c["crawl"]["phase"], "completed");
    assert_eq!(c["canResume"], false);

    clock.advance(Duration::from_millis(10_001));
    let items = list(&app).await;
    assert_eq!(entry(&items, &foreign.to_string())["canResume"], true);
    assert_eq!(
        entry(&items, &foreign.to_string())["crawl"]["phase"],
        "paused"
    );
}

// spec: AI-020 [M4]
// spec: AGENT-020
// acceptance: independent
#[tokio::test]
async fn a_completed_crawl_with_a_foreign_mcp_owner_opens_read_only() {
    // K4-13 „Besitzer eines KI-Laufs“, zweiter Satz (ohne ai-mock): Crawl-Datei mit `Completed`,
    // `owner_kind = mcp-stdio`, `owner_pid` gesetzt und aktuellem (simuliertem) Heartbeat → die App zeigt nur lesend.
    // Wirkung (b): Fortsetzen, erneutes Crawlen und Stoppen stehen in der nur lesend geöffneten Sitzung nicht zur
    // Verfügung.
    let home = TempDir::new("app-open-ro");
    let (app, clock) = app_at(home.path(), base_now());
    let dir = crawls_dir(&app);
    let (_, path) = new_crawl(&dir, 30, CrawlState::Completed);
    set_owner(&path, "mcp-stdio", FOREIGN_PID, Some(clock.now()));
    let urls_before = count(&path, "urls");
    let summary = open(&app, &path).await;
    assert!(!summary.owner.controllable, "nur lesend");
    assert!(summary.owner.live);
    assert_eq!(summary.owner.kind, OwnerKind::Mcp);
    assert_eq!(summary.owner.pid, Some(FOREIGN_PID.parse::<u32>().unwrap()));
    assert_eq!(
        serde_json::to_value(summary.phase).unwrap(),
        "completed",
        "Zustand bleibt der gespeicherte"
    );
    let session = app
        .sessions()
        .current(&os_app::WindowId("main".into()))
        .unwrap();
    assert!(!session.writable);
    for cmd in [
        "crawl.resume",
        "crawl.stop",
        "crawl.pause",
        "crawl.recrawl",
        "crawl.remove",
        "crawl.saveAs",
    ] {
        let args = match cmd {
            "crawl.remove" | "crawl.recrawl" => json!({"selection": {"kind": "ids", "ids": [1]}}),
            "crawl.saveAs" => json!({"dest": home.path().join("kopie.ospider")}),
            _ => Value::Null,
        };
        let err = app.dispatch(ctx(), cmd, args).await.unwrap_err();
        assert_eq!(err.code, ErrorCode::Unsupported, "{cmd}");
    }
    let err = outcome(
        &app,
        "crawl.addUrls",
        json!({"urls": ["https://k11.test/neu"]}),
    )
    .await
    .expect_err("URLs hinzufügen in nur lesender Sitzung");
    assert_ne!(err.code, ErrorCode::Internal, "{err:?}");
    assert_eq!(count(&path, "urls"), urls_before);
    assert_eq!(get(&path, "owner_pid").as_deref(), Some(FOREIGN_PID));
    assert_eq!(get(&path, "state").as_deref(), Some("Completed"));
}

// spec: AI-020 [M4]
// spec: AGENT-020
// acceptance: independent
#[tokio::test]
async fn with_the_own_pid_as_active_owner_the_crawl_opens_writable() {
    // „(b) Ist `owner_pid` nicht der eigene Prozess …“ – mit eigener PID öffnet die App den Crawl schreibbar.
    let home = TempDir::new("app-open-own");
    let (app, clock) = app_at(home.path(), base_now());
    let dir = crawls_dir(&app);
    let (_, path) = new_crawl(&dir, 40, CrawlState::Completed);
    set_owner(&path, "app", &own_pid(), Some(clock.now()));
    let summary = open(&app, &path).await;
    assert!(summary.owner.controllable);
    assert_eq!(serde_json::to_value(summary.phase).unwrap(), "completed");
    let session = app
        .sessions()
        .current(&os_app::WindowId("main".into()))
        .unwrap();
    assert!(session.writable);
}

// spec: AI-020 [M4]
// spec: AGENT-020
// acceptance: independent
#[tokio::test]
async fn an_open_session_rejects_resume_stop_and_recrawl_once_a_foreign_owner_becomes_active() {
    // Wirkung (b) bei der Prüfung beim Auslösen: Die Sitzung wurde ohne Besitzer schreibbar geöffnet; danach trägt
    // sich ein anderer Prozess als Besitzer eines KI-Laufs ein. Fortsetzen, Stoppen und erneutes Crawlen werden
    // abgewiesen wie bei einem fremd geführten laufenden Crawl; die Datei bleibt unverändert.
    let home = TempDir::new("app-trigger");
    let (app, clock) = app_at(home.path(), base_now());
    let dir = crawls_dir(&app);
    let (_, path) = new_crawl(&dir, 50, CrawlState::Paused);
    let summary = open(&app, &path).await;
    assert!(summary.owner.controllable);
    set_owner(&path, "mcp-stdio", FOREIGN_PID, Some(clock.now()));
    let before = owner_keys(&path);
    for (cmd, args) in [
        ("crawl.resume", Value::Null),
        ("crawl.stop", Value::Null),
        (
            "crawl.recrawl",
            json!({"selection": {"kind": "ids", "ids": [1]}}),
        ),
    ] {
        let err = outcome(&app, cmd, args)
            .await
            .expect_err(&format!("{cmd} trotz fremdem aktivem Besitzer"));
        // Wie bei einem fremd geführten laufenden Crawl (Prüfung beim Auslösen, 1.0: `busy`).
        assert_eq!(err.code, ErrorCode::Busy, "{cmd}: {err:?}");
        assert_eq!(owner_keys(&path), before, "{cmd}: Datei unverändert");
    }
    assert_eq!(get(&path, "state").as_deref(), Some("Paused"));
}

// spec: AI-020 [M4]
// spec: AGENT-020
// acceptance: independent
#[tokio::test]
async fn an_open_session_adds_no_urls_while_a_foreign_owner_is_active() {
    // Wirkung (b): „erneutes Crawlen von URLs … abgewiesen wie bei einem fremd geführten laufenden Crawl“ – auch das
    // Hinzufügen von URLs in einer zuvor schreibbar geöffneten Sitzung schreibt nicht in die Datei eines fremden
    // aktiven Besitzers (Nacharbeit M4-K11, `commands/crawl_add.rs`).
    let home = TempDir::new("app-add");
    let (app, clock) = app_at(home.path(), base_now());
    let dir = crawls_dir(&app);
    let (_, path) = new_crawl(&dir, 60, CrawlState::Completed);
    let summary = open(&app, &path).await;
    assert!(summary.owner.controllable);
    set_owner(&path, "mcp-stdio", FOREIGN_PID, Some(clock.now()));
    let urls_before = count(&path, "urls");
    let queue_before = count(&path, "queue");
    let result = outcome(
        &app,
        "crawl.addUrls",
        json!({"urls": ["https://k11.test/neu"]}),
    )
    .await;
    assert!(
        result.is_err(),
        "URLs trotz fremdem aktivem Besitzer hinzugefügt: {result:?}"
    );
    assert_eq!(count(&path, "urls"), urls_before);
    assert_eq!(count(&path, "queue"), queue_before);
    assert_eq!(get(&path, "owner_pid").as_deref(), Some(FOREIGN_PID));
}

// spec: AI-020 [M4]
// spec: CRAWL-125
// spec: OUT-228
// acceptance: independent
#[tokio::test]
async fn files_from_1_0_list_and_delete_exactly_as_crawl_125_alone() {
    // Kriterium 6 für die Crawl-Verwaltung der App: Zustand, Fortsetzbarkeit, „live“ und `busy` beim Löschen wie
    // nach CRAWL-125 allein.
    let (now, cases) = v10_cases();
    assert_fixture_consistent(&cases);
    let home = TempDir::new("app-v10");
    let (app, _clock) = app_at(home.path(), now);
    let dir = crawls_dir(&app);
    let built: Vec<(String, &V10Case)> = cases
        .iter()
        .enumerate()
        .map(|(i, c)| (c.build(&dir, 100 + i as u64, now).0.to_string(), c))
        .collect();
    let items = list(&app).await;
    for (id, case) in &built {
        let e = entry(&items, id);
        assert_eq!(
            e["crawl"]["phase"],
            phase(case.oracle_state()),
            "{}",
            case.name
        );
        assert_eq!(e["canResume"], case.oracle_resumable(), "{}", case.name);
        assert_eq!(
            e["crawl"]["owner"]["live"],
            case.oracle_running(),
            "{}",
            case.name
        );
    }
    for (id, case) in &built {
        let result = outcome(&app, "crawls.delete", json!({"ids": [id]})).await;
        match result {
            Err(e) => {
                assert!(case.oracle_running(), "{}: {e:?}", case.name);
                assert_eq!(e.code, ErrorCode::Busy, "{}", case.name);
            }
            Ok(_) => assert!(!case.oracle_running(), "{}: gelöscht", case.name),
        }
    }
}

// spec: AI-020 [M4]
// spec: OUT-228
// acceptance: independent
#[tokio::test]
async fn rename_project_assignment_and_duplicate_answer_busy_for_any_active_owner() {
    // 08 AI-020, Nachtrag 26.11.2026, Wirkung (a) in der Fassung nach der Abnahme M4-K11: Umbenennen, Projekt zuordnen
    // und Duplizieren eines Crawls mit aktivem Besitzer → `busy`; 11 s später gelingen sie.
    for (i, pid) in [FOREIGN_PID.to_string(), own_pid()].into_iter().enumerate() {
        let home = TempDir::new("app-rename");
        let (app, clock) = app_at(home.path(), base_now());
        let dir = crawls_dir(&app);
        let (id, path) = new_crawl(&dir, 70 + i as u64, CrawlState::Completed);
        set_owner(&path, "mcp-stdio", &pid, Some(clock.now()));
        let id = id.to_string();
        let project = app
            .dispatch(ctx(), "projects.create", json!({"name": "Projekt K11"}))
            .await
            .unwrap();
        let project_id = project["id"].as_str().unwrap().to_string();
        let before = owner_keys(&path);
        let name_before = get(&path, "name");
        let actions = [
            (
                "crawls.rename",
                json!({"crawlId": id, "name": "Neuer Name"}),
            ),
            ("crawls.move", json!({"ids": [id], "project": project_id})),
            ("crawls.duplicate", json!({"ids": [id]})),
        ];
        for (name, args) in actions.clone() {
            let err = outcome(&app, name, args)
                .await
                .expect_err(&format!("{name}: PID {pid}"));
            assert_eq!(err.code, ErrorCode::Busy, "{name}: PID {pid}");
        }
        assert_eq!(owner_keys(&path), before);
        assert_eq!(get(&path, "name"), name_before);
        assert_eq!(get(&path, "project").as_deref(), Some(""));
        assert_eq!(list(&app).await.len(), 1, "keine Kopie angelegt");

        clock.advance(Duration::from_millis(11_000));
        for (name, args) in actions {
            outcome(&app, name, args)
                .await
                .unwrap_or_else(|e| panic!("{name} 11 s später: {e:?}"));
        }
        assert_eq!(list(&app).await.len(), 2);
    }
}

// spec: AI-020 [M4]
// spec: CRAWL-125
// acceptance: independent
#[tokio::test]
async fn an_own_running_crawl_without_owner_pid_is_listed_like_in_1_0() {
    // Befund B3-2, Entscheidung des Orchestrators: Rückschritt gegenüber 1.0. Gespeichertes `Running` ohne
    // `owner_pid` (Zustand vor dem ersten Batch), geführt von der Engine dieses Prozesses: CRAWL-125 allein ergibt
    // „laufend“, 1.0 zeigt den eigenen Crawl in der Liste als live und steuerbar (eigene Sitzung mit Engine) und nicht
    // fortsetzbar. Der Zustand vor dem ersten Batch wird hergestellt, indem `owner_pid` geleert wird (die Engine
    // schreibt ihn nur im ersten Batch, CRAWL-125).
    use os_testsite::{SiteOptions, TestSite};
    let site = TestSite::start_spec(
        "synthetic:n=40,links=4,seed=7,delay_ms=1000",
        SiteOptions {
            port: 0,
            log_requests: true,
        },
    )
    .await
    .unwrap();
    let home = TempDir::new("app-own-running");
    let app = App::new(AppConfig::new(home.path())).unwrap();
    let mut cfg = os_model::CrawlConfig::default();
    cfg.analysis.auto_at_end = false;
    cfg.speed.threads = 1;
    cfg.save_default(&app.config().paths.settings).unwrap();
    app.dispatch(
        ctx(),
        "crawl.start",
        json!({"source": {"kind": "spider", "url": site.url("/")}, "scope": "domain", "profile": null}),
    )
    .await
    .unwrap_or_else(|e| panic!("crawl.start: {e:?}"));
    let window = os_app::WindowId("main".into());
    let (id, path) = tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            if let Some(s) = app.sessions().current(&window) {
                if s.engine.is_some() && s.path.exists() && get(&s.path, "owner_pid").is_some() {
                    return (s.key.0.clone(), s.path.clone());
                }
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("Crawl läuft");
    set(&path, "owner_pid", "");
    let items = list(&app).await;
    let e = entry(&items, &id);
    assert_eq!(e["crawl"]["phase"], "running");
    assert_eq!(e["canResume"], false);
    assert_eq!(e["crawl"]["owner"]["live"], true);
    assert_eq!(
        e["crawl"]["owner"]["controllable"], true,
        "eigener laufender Crawl ist wie in 1.0 steuerbar"
    );
    let _ = app.dispatch(ctx(), "crawl.stop", Value::Null).await;
}
