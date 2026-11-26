//! M4-K11 in der App: Crawl-Verwaltung (Liste, Löschen, Verschieben, Exportieren, Sperren, Entsperren), lesendes Öffnen
//! und die Prüfung beim Auslösen (Fortsetzen, Stoppen) bei einem aktiven Besitzer außerhalb von `Running` (08 AI-020
//! [M4], Unterpunkt „Aktiver Besitzer“, Wirkung a und b; Kriterien (b) und (d) des Pakets). Simulierter Heartbeat über
//! die Metadaten, Zeit über die `ManualClock` der App; Zustandsfälle aus `tests/fixtures/crawl-files/active-owner/`.
#![allow(clippy::disallowed_types, clippy::disallowed_methods)]
#[path = "session_common.rs"]
mod common;
use common::*;
use os_app::dto::*;
use serde_json::json;
use std::path::{Path, PathBuf};
use std::time::Duration;

fn foreign_pid() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/crawl-files/active-owner/cases.json");
    let cases: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let pid = cases["foreign_pid"].as_str().unwrap().to_string();
    assert_ne!(pid, std::process::id().to_string());
    pid
}

/// Crawl-Datei im Crawl-Ordner der App mit Kennung `id`, Zustand `state` und Besitzer (`kind`, `pid`) mit Heartbeat
/// „jetzt“ nach der Uhr der App.
fn managed(app: &os_app::App, id: &str, state: &str, kind: &str, pid: &str) -> PathBuf {
    let dir = &app.config().paths.crawls;
    std::fs::create_dir_all(dir).unwrap();
    let path = dir.join(format!("{id}.ospider"));
    let f = create(&path, cfg());
    f.set_meta("crawl_id", id).unwrap();
    f.set_meta("state", state).unwrap();
    insert(&f, 1, 1);
    if state == "Completed" {
        // Limit erreicht mit Restwarteschlange: ohne Besitzer fortsetzbar.
        f.set_meta("end_reason", "limit_reached").unwrap();
        f.conn()
            .execute_batch(
                "INSERT INTO urls(id,url,address,origin,is_internal,state,discovered_via,folder_depth,url_length) \
                 VALUES (2,'https://fixture.test/q','https://fixture.test/q','https://fixture.test',1,'queued','link',0,22); \
                 INSERT INTO queue(url_id,seq) VALUES (2,1);",
            )
            .unwrap();
    }
    f.set_meta("owner_kind", kind).unwrap();
    f.set_meta("owner_pid", pid).unwrap();
    f.set_meta(
        "heartbeat_at",
        &os_model::Clock::now(app.config().clock.as_ref()).to_db(),
    )
    .unwrap();
    path
}

fn item<'a>(list: &'a serde_json::Value, id: &str) -> &'a serde_json::Value {
    list.as_array()
        .unwrap()
        .iter()
        .find(|i| i["crawl"]["crawlId"] == id)
        .unwrap_or_else(|| panic!("{id} fehlt in {list}"))
}

async fn rejected(app: &os_app::App, name: &str, args: serde_json::Value) -> os_app::ErrorCode {
    match app.dispatch(ctx(), name, args).await {
        Err(e) => e.code,
        Ok(handle) => {
            let status = app
                .ops()
                .wait(handle["opId"].as_str().unwrap())
                .await
                .unwrap();
            status.error.expect("abgewiesen").code
        }
    }
}

/// Kriterium (b), Teil App: `Completed` mit fremdem aktivem Besitzer – Liste `live`, nicht steuerbar, nicht
/// fortsetzbar, Zustand unverändert; Löschen, Verschieben, Exportieren, Sperren, Entsperren → `busy`. Ein aktiver
/// Besitzer mit der PID der App schützt in der Verwaltung genauso, bleibt aber fortsetzbar. 11 s später ohne neuen
/// Heartbeat gelingt das Löschen.
// spec: AI-020 [M4]
// spec: OUT-228
// spec: CRAWL-128
#[tokio::test]
async fn management_treats_an_active_owner_like_a_running_crawl_until_its_heartbeat_is_stale() {
    let dir = tempfile::tempdir().unwrap();
    let (app, clock) = manual_app(dir.path());
    let foreign_id = "0199f000-0000-7000-8000-0000000c0011";
    let own_id = "0199f000-0000-7000-8000-0000000c0012";
    let foreign = managed(&app, foreign_id, "Completed", "mcp-stdio", &foreign_pid());
    managed(
        &app,
        own_id,
        "Paused",
        "app",
        &std::process::id().to_string(),
    );
    let list = call(&app, "crawls.list", json!({"show":"all"})).await;
    let f = item(&list, foreign_id);
    assert_eq!(
        f["crawl"]["phase"], "completed",
        "abgeleiteter Zustand bleibt"
    );
    assert_eq!(f["crawl"]["owner"]["live"], true);
    assert_eq!(f["crawl"]["owner"]["controllable"], false);
    assert_eq!(f["canResume"], false);
    let o = item(&list, own_id);
    assert_eq!(o["crawl"]["phase"], "paused");
    assert_eq!(o["crawl"]["owner"]["live"], true);
    assert_eq!(o["canResume"], true, "eigener Prozess darf fortsetzen");
    assert!(call(&app, "crawls.list", json!({"show":"running"}))
        .await
        .as_array()
        .unwrap()
        .is_empty());

    for id in [foreign_id, own_id] {
        for (name, args) in [
            ("crawls.delete", json!({"ids":[id]})),
            ("crawls.move", json!({"ids":[id],"project":null})),
            (
                "crawls.export",
                json!({"ids":[id],"dest":dir.path().join("export.ospider")}),
            ),
            ("crawls.lock", json!({"ids":[id]})),
            ("crawls.unlock", json!({"ids":[id]})),
        ] {
            assert_eq!(
                rejected(&app, name, args).await,
                os_app::ErrorCode::Busy,
                "{name} {id}"
            );
        }
    }
    assert!(foreign.exists());
    assert!(!dir.path().join("export.ospider").exists());

    clock.advance(Duration::from_secs(11));
    let list = call(&app, "crawls.list", json!({"show":"all"})).await;
    let f = item(&list, foreign_id);
    assert_eq!(f["crawl"]["owner"]["live"], false);
    assert_eq!(f["canResume"], true);
    let status = operation(&app, "crawls.delete", json!({"ids":[foreign_id]})).await;
    assert_eq!(status.state, OpState::Done, "{:?}", status.error);
    assert!(!foreign.exists());
}

/// Kriterium (d): `Completed`, `owner_kind = mcp-stdio`, fremde `owner_pid`, aktueller Heartbeat → die App öffnet nur
/// lesend; Schreibbefehle `Unsupported` wie bei einem fremd laufenden Crawl (`session_feed.rs`,
/// `live_foreign_crawl_is_read_only_and_uses_persisted_progress`). Der Zustand bleibt `Completed`.
// spec: AI-020 [M4]
// spec: AGENT-020
#[tokio::test]
async fn a_completed_crawl_with_a_foreign_active_owner_opens_read_only() {
    let dir = tempfile::tempdir().unwrap();
    let (app, clock) = manual_app(dir.path());
    let pid = foreign_pid();
    let path = managed(
        &app,
        "0199f000-0000-7000-8000-0000000c0013",
        "Completed",
        "mcp-stdio",
        &pid,
    );
    let summary = open(&app, &path).await;
    assert!(!summary.owner.controllable);
    assert!(summary.owner.live);
    assert_eq!(summary.owner.kind, OwnerKind::Mcp);
    assert_eq!(summary.owner.pid, pid.parse().ok());
    assert_eq!(summary.phase, CrawlPhase::Completed);
    os_app::session::poll(&app).await.unwrap();
    let status = app.assemble_tick().unwrap().crawl.unwrap();
    assert_eq!(status.phase, CrawlPhase::Completed);
    assert!(!status.owner.controllable && status.owner.live);
    for cmd in [
        "crawl.pause",
        "crawl.resume",
        "crawl.stop",
        "analysis.start",
        "crawl.remove",
        "crawl.recrawl",
        "crawl.saveAs",
    ] {
        let args = match cmd {
            "analysis.start" => json!({"steps":null}),
            "crawl.remove" | "crawl.recrawl" => json!({"selection":{"kind":"ids","ids":[1]}}),
            "crawl.saveAs" => json!({"dest":dir.path().join("copy.ospider")}),
            _ => serde_json::Value::Null,
        };
        let err = app.dispatch(ctx(), cmd, args).await.unwrap_err();
        assert_eq!(err.code, os_app::ErrorCode::Unsupported, "{cmd}");
    }
    let write = app
        .write_queue()
        .submit_file(&key(&app), |_| Ok(()))
        .await
        .unwrap_err();
    assert_eq!(write.code, os_app::ErrorCode::NoCrawl);
    let file = os_store::CrawlFile::open(&path, os_store::OpenMode::ReadOnly).unwrap();
    assert_eq!(file.get_meta("owner_pid").unwrap(), Some(pid));
    assert_eq!(
        file.get_meta("state").unwrap().as_deref(),
        Some("Completed")
    );
    drop(file);
    // Mehr als 10 s ohne Heartbeat: kein aktiver Besitzer mehr, der Zustand bleibt der gespeicherte.
    clock.advance(Duration::from_secs(11));
    let (summary, _, state) = os_app::session::feed::snapshot(&app, key(&app))
        .await
        .unwrap();
    assert_eq!(state, os_model::CrawlState::Completed);
    assert!(!summary.owner.live && summary.owner.controllable);
}

/// Prüfung beim Auslösen (`ensure_idle`, `stop_checkpoint`): Die App hat einen pausierten Crawl schreibend geöffnet;
/// danach trägt sich ein anderer Prozess als Besitzer eines KI-Laufs ein. Fortsetzen und Stoppen werden mit `busy`
/// abgewiesen, ohne die Datei zu ändern; 11 s später gelingt das Stoppen.
// spec: AI-020 [M4]
// spec: CRAWL-125
#[tokio::test]
async fn resume_and_stop_of_an_open_crawl_are_rejected_while_another_process_owns_it() {
    let dir = tempfile::tempdir().unwrap();
    let (app, clock) = manual_app(dir.path());
    let path = dir.path().join("paused.ospider");
    let f = create(&path, cfg());
    f.set_meta("state", "Paused").unwrap();
    insert(&f, 1, 1);
    drop(f);
    let summary = open(&app, &path).await;
    assert!(summary.owner.controllable);
    let pid = foreign_pid();
    let f = os_store::CrawlFile::open(&path, os_store::OpenMode::ReadWrite).unwrap();
    f.set_meta("owner_kind", "cli").unwrap();
    f.set_meta("owner_pid", &pid).unwrap();
    f.set_meta(
        "heartbeat_at",
        &os_model::Clock::now(clock.as_ref()).to_db(),
    )
    .unwrap();
    drop(f);
    for cmd in ["crawl.resume", "crawl.stop"] {
        let err = app
            .dispatch(ctx(), cmd, serde_json::Value::Null)
            .await
            .unwrap_err();
        assert_eq!(err.code, os_app::ErrorCode::Busy, "{cmd}: {err:?}");
    }
    let file = os_store::CrawlFile::open(&path, os_store::OpenMode::ReadOnly).unwrap();
    assert_eq!(file.get_meta("state").unwrap().as_deref(), Some("Paused"));
    assert_eq!(file.get_meta("owner_pid").unwrap(), Some(pid));
    drop(file);
    clock.advance(Duration::from_secs(11));
    call(&app, "crawl.stop", serde_json::Value::Null).await;
    let file = os_store::CrawlFile::open(&path, os_store::OpenMode::ReadOnly).unwrap();
    assert_eq!(
        file.get_meta("state").unwrap().as_deref(),
        Some("Completed")
    );
    assert_eq!(
        file.get_meta("end_reason").unwrap().as_deref(),
        Some("stopped")
    );
    assert_eq!(file.get_meta("owner_pid").unwrap().as_deref(), Some(""));
}

/// Nacharbeit 26.11.2026: `crawl.addUrls` ohne Engine auf einem schreibend geöffneten Crawl, den danach ein anderer
/// Prozess als aktiver Besitzer führt, wird abgewiesen wie in einer nur lesend geöffneten Sitzung (`Permission`,
/// `err-crawl-readonly`); keine URL wird geschrieben. 11 s später gelingt das Nachreichen.
// spec: AI-020 [M4]
// spec: CRAWL-131
#[tokio::test]
async fn adding_urls_is_rejected_while_another_process_owns_the_open_crawl() {
    let dir = tempfile::tempdir().unwrap();
    let (app, clock) = manual_app(dir.path());
    let path = dir.path().join("add.ospider");
    let f = create(&path, cfg());
    f.set_meta("state", "Paused").unwrap();
    insert(&f, 1, 1);
    drop(f);
    assert!(open(&app, &path).await.owner.controllable);
    let f = os_store::CrawlFile::open(&path, os_store::OpenMode::ReadWrite).unwrap();
    f.set_meta("owner_kind", "cli").unwrap();
    f.set_meta("owner_pid", &foreign_pid()).unwrap();
    f.set_meta(
        "heartbeat_at",
        &os_model::Clock::now(clock.as_ref()).to_db(),
    )
    .unwrap();
    drop(f);
    let urls = || json!({"urls":["https://fixture.test/added"]});
    let op = call(&app, "crawl.addUrls", urls()).await;
    let status = app.ops().wait(op["opId"].as_str().unwrap()).await.unwrap();
    let err = status.error.expect("abgewiesen");
    assert_eq!(err.code, os_app::ErrorCode::Permission);
    assert_eq!(err.message_key, "err-crawl-readonly");
    let count = |p: &Path| -> i64 {
        os_store::CrawlFile::open(p, os_store::OpenMode::ReadOnly)
            .unwrap()
            .conn()
            .query_row("SELECT COUNT(*) FROM urls", [], |r| r.get(0))
            .unwrap()
    };
    assert_eq!(count(&path), 1);
    clock.advance(Duration::from_secs(11));
    let op = call(&app, "crawl.addUrls", urls()).await;
    let status = app.ops().wait(op["opId"].as_str().unwrap()).await.unwrap();
    assert!(status.error.is_none(), "{:?}", status.error);
    assert_eq!(count(&path), 2);
}
