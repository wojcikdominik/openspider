use os_app::{App, AppConfig, CallCtx};
use serde_json::{json, Value};
async fn call(app: &App, name: &str, args: Value) -> Value {
    app.dispatch(CallCtx::for_window("main"), name, args)
        .await
        .unwrap()
}
// spec: AGENT-001 [M2]
// spec: AGENT-003 [M2]
// spec: UI-129 [P1]
#[tokio::test]
async fn persistent_permissions_and_markdown_share_the_tool_list() {
    let home = tempfile::tempdir().unwrap();
    let app = App::new(AppConfig::new(home.path())).unwrap();
    assert_eq!(
        call(&app, "mcp.status", Value::Null).await,
        json!({"kind":"off"})
    );
    let tools = call(&app, "mcp.tools", Value::Null).await;
    assert!(tools
        .as_array()
        .unwrap()
        .iter()
        .filter(|t| t["enabled"] == true)
        .all(|t| t["class"] == "read"));
    call(&app, "mcp.setClasses", json!({"classes":["write","cost"]})).await;
    call(
        &app,
        "mcp.setToolEnabled",
        json!({"name":"query_urls","enabled":false}),
    )
    .await;
    let prefs = call(&app, "settings.get", Value::Null).await;
    assert!(prefs["mcp"]["classes"]
        .as_array()
        .unwrap()
        .contains(&json!("cost")));
    let saved = os_mcp::settings::load(&home.path().join("mcp.toml")).unwrap();
    assert!(saved.classes.contains(&os_mcp::ToolClass::Cost));
    assert!(!saved.tools["query_urls"]);
    let read_only = os_mcp::permissions::Permissions::resolve(&[], true, &saved).unwrap();
    assert_eq!(read_only.classes(), [os_mcp::ToolClass::Read]);
    drop(app);
    let app = App::new(AppConfig::new(home.path())).unwrap();
    let tools = call(&app, "mcp.tools", Value::Null).await;
    assert!(!tools
        .as_array()
        .unwrap()
        .iter()
        .any(|t| t["name"] == "query_urls" && t["enabled"] == true));
    let path = home.path().join("tools.md");
    call(&app, "mcp.saveToolsMarkdown", json!({"path":path})).await;
    assert_eq!(
        std::fs::read_to_string(path)
            .unwrap()
            .matches("\n## ")
            .count(),
        tools
            .as_array()
            .unwrap()
            .iter()
            .filter(|t| t["enabled"] == true)
            .count()
    );
}
// spec: AGENT-002 [M2]
// spec: OUT-285 [P1]
#[tokio::test]
async fn server_lifecycle_token_rotation_and_autostart() {
    let home = tempfile::tempdir().unwrap();
    let config = AppConfig::new(home.path());
    let app = App::new(config.clone()).unwrap();
    let hidden = call(&app, "mcp.token", json!({"reveal":false})).await;
    assert!(hidden["value"].is_null());
    let token = call(&app, "mcp.token", json!({"reveal":true})).await;
    assert_eq!(token["value"].as_str().unwrap().len(), 64);
    let changed = call(&app, "mcp.regenerateToken", json!({"reveal":true})).await;
    assert_ne!(token["value"], changed["value"]);
    let ready = call(&app, "mcp.start", json!({"port":0})).await;
    assert_eq!(ready["kind"], "ready");
    call(&app, "mcp.setAutostart", json!({"value":true})).await;
    call(&app, "mcp.stop", Value::Null).await;
    drop(app);
    let app = App::new(config).unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            if call(&app, "mcp.status", Value::Null).await["kind"] == "ready" {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    call(&app, "mcp.stop", Value::Null).await;
}
// Client-Konfigurationen (AGENT-043, Verbinden mit einem Klick): `tests/mcp_clients.rs`.
// spec: AGENT-043 [P1]
#[tokio::test]
async fn invocation_log_redacts_credentials_and_records_sizes() {
    let home = tempfile::tempdir().unwrap();
    let mut config = AppConfig::new(home.path());
    let now = os_model::Timestamp::from_db("2026-01-02T03:04:05Z").unwrap();
    config.clock = std::sync::Arc::new(os_model::ManualClock::new(now));
    let app = App::new(config).unwrap();
    call(&app, "mcp.start", json!({"port":0})).await;
    let service = app.ext::<os_app::mcp::Service>();
    let server = service.state.lock().await.server.clone().unwrap();
    for tool in ["list_crawls", "query_urls", "sql_query"] {
        server.call_as(tool, Some(json!({"sql":"secret-token","url":"https://user:password@site.test/?token=secret-token"}).as_object().unwrap().clone()), Some("fixture-client".into())).await;
    }
    let log = call(&app, "mcp.callLog", json!({"offset":0,"limit":10})).await;
    assert_eq!(log.as_array().unwrap().len(), 3);
    assert!(!log.to_string().contains("secret-token"));
    assert!(!log.to_string().contains("password"));
    for row in log.as_array().unwrap() {
        assert!(row["resultBytes"].as_u64().unwrap() > 0);
        assert_eq!(row["client"], "fixture-client");
        assert_eq!(row["at"], "2026-01-02T03:04:05Z");
        assert_eq!(row["durationMs"], 0);
    }
    call(&app, "mcp.stop", Value::Null).await;
}

// spec: AGENT-020 [M2]
#[test]
fn http_crawl_uses_app_session_and_target_policy() {
    // Exercise the real MCP/store path on small worker AND blocking stacks.
    // RUST_MIN_STACK alone depends on the runtime's builder configuration.
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_stack_size(320 * 1024)
        .enable_all()
        .build()
        .unwrap()
        .block_on(http_crawl_fixture());
}

async fn http_crawl_fixture() {
    use os_mcp::SessionAccess;
    let home = tempfile::tempdir().unwrap();
    let app = App::new(AppConfig::new(home.path())).unwrap();
    let site = os_testsite::TestSite::start_dir(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/out-basic-site"),
        0,
    )
    .await
    .unwrap();
    let mut settings = os_mcp::McpSettings {
        classes: vec![os_mcp::ToolClass::Crawl],
        ..Default::default()
    };
    os_mcp::settings::save(&home.path().join("mcp.toml"), &settings).unwrap();
    let server = os_app::mcp::server(&app, settings.clone()).unwrap();
    let args = json!({"url":format!("http://127.0.0.1:{}/", site.port()),"overrides":{"limits.max_urls":1}});
    assert_eq!(
        server
            .call("start_crawl", args.as_object().cloned())
            .await
            .is_error,
        Some(true)
    );
    settings.allow_private_targets = true;
    let server = os_app::mcp::server(&app, settings).unwrap();
    let result = server.call("start_crawl", args.as_object().cloned()).await;
    assert_eq!(result.is_error, Some(false), "{:?}", result.content);
    let body: Value = serde_json::from_str(&result.content[0].as_text().unwrap().text).unwrap();
    let id = body["summary"]["crawl_id"].as_str().unwrap();
    let access = os_app::mcp::session::Access(app.downgrade());
    assert!(access
        .crawls()
        .iter()
        .any(|c| c.crawl_id == id && c.controllable));
    let s = app
        .sessions()
        .all()
        .into_iter()
        .find(|s| s.key.0 == id)
        .unwrap();
    assert_eq!(s.config.limits.max_urls, 1);
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            os_app::session::poll(&app).await.unwrap();
            if access.status(id).unwrap().state != os_model::CrawlState::Running {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let owner = app
        .read_pool()
        .run(s.key.clone(), |f, _| f.get_meta("owner_kind"))
        .await
        .unwrap();
    assert_eq!(owner.as_deref(), Some("app"));
    assert_eq!(
        server
            .call(
                "crawl_status",
                Some(json!({"id":id}).as_object().unwrap().clone())
            )
            .await
            .is_error,
        Some(false)
    );
}

// spec: AGENT-020
// spec: CRAWL-128
#[test]
fn http_pause_resume_and_stop_follow_the_app_session_and_stopped_is_final() {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap()
        .block_on(http_pause_resume_stop_fixture());
}

async fn http_pause_resume_stop_fixture() {
    use os_mcp::SessionAccess;
    let home = tempfile::tempdir().unwrap();
    let app = App::new(AppConfig::new(home.path())).unwrap();
    // Kette ohne natürliches Ende vor der Pause (bei 2 URLs/s mindestens 6 s, B3-FLAKES).
    let site = os_testsite::TestSite::start_dir(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/agent-m2-crawl"),
        0,
    )
    .await
    .unwrap();
    let settings = os_mcp::McpSettings {
        classes: vec![os_mcp::ToolClass::Crawl],
        allow_private_targets: true,
        ..Default::default()
    };
    os_mcp::settings::save(&home.path().join("mcp.toml"), &settings).unwrap();
    let server = os_app::mcp::server(&app, settings).unwrap();
    let args = |v: Value| v.as_object().cloned();
    // Text blocks of a tool result (rmcp types stay behind os-mcp).
    macro_rules! text {
        ($r:expr) => {
            $r.content
                .iter()
                .filter_map(|c| c.as_text().map(|t| t.text.clone()))
                .collect::<Vec<_>>()
                .join(" ")
        };
    }
    let started = server
        .call(
            "start_crawl",
            args(
                json!({"url":format!("http://127.0.0.1:{}/chain/1", site.port()),
                "overrides":{"speed.threads":1,"speed.max_urls_per_sec":2.0}}),
            ),
        )
        .await;
    assert_eq!(started.is_error, Some(false), "{}", text!(started));
    let body: Value = serde_json::from_str(&started.content[0].as_text().unwrap().text).unwrap();
    let id = body["summary"]["crawl_id"].as_str().unwrap().to_owned();
    let access = os_app::mcp::session::Access(app.downgrade());

    // In der App kehrt `pause_crawl` zurück, wenn „Pausiert“ dauerhaft ist (derselbe Weg wie die Kopfleiste).
    let paused = server
        .call("pause_crawl", args(json!({"crawl_id":id})))
        .await;
    assert_eq!(paused.is_error, Some(false), "{}", text!(paused));
    assert_eq!(
        access.status(&id).unwrap().state,
        os_model::CrawlState::Paused
    );
    let resumed = server
        .call("resume_crawl", args(json!({"crawl_id":id})))
        .await;
    assert_eq!(resumed.is_error, Some(false), "{}", text!(resumed));
    assert_eq!(
        access.status(&id).unwrap().state,
        os_model::CrawlState::Running
    );
    let stopped = server
        .call("stop_crawl", args(json!({"crawl_id":id})))
        .await;
    assert_eq!(stopped.is_error, Some(false), "{}", text!(stopped));
    let s = app
        .sessions()
        .all()
        .into_iter()
        .find(|s| s.key.0 == id)
        .unwrap();
    let meta = app
        .read_pool()
        .run(s.key.clone(), |f, _| f.meta())
        .await
        .unwrap();
    assert_eq!(meta.end_reason, Some(os_model::EndReason::Stopped));
    let requests = site.total_requests();
    let refused = server
        .call("resume_crawl", args(json!({"crawl_id":id})))
        .await;
    assert_eq!(refused.is_error, Some(true));
    assert!(
        text!(refused).contains("gestoppt – endgültig beendet"),
        "{}",
        text!(refused)
    );
    assert!(refused.structured_content.is_none(), "not retryable");
    assert_eq!(site.total_requests(), requests);
    // Ein beendeter Crawl lässt sich nicht pausieren; kein `busy`, sondern ein korrigierbarer Fehler.
    let again = server
        .call("pause_crawl", args(json!({"crawl_id":id})))
        .await;
    assert_eq!(again.is_error, Some(true));
    assert!(again.structured_content.is_none());
}

// spec: OUT-285 [P1]
#[tokio::test]
async fn occupied_autostart_port_emits_notice_and_stays_off() {
    let home = tempfile::tempdir().unwrap();
    let occupied = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .await
        .unwrap();
    let mut settings = os_mcp::McpSettings::default();
    settings.http.autostart = true;
    settings.http.port = Some(occupied.local_addr().unwrap().port());
    os_mcp::settings::save(&home.path().join("mcp.toml"), &settings).unwrap();
    let app = App::new(AppConfig::new(home.path())).unwrap();
    let sink = std::sync::Arc::new(os_app::events::CollectingSink::new());
    app.subscribe(os_app::WindowId("main".into()), sink.clone());
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            let noticed = sink.take().iter().any(|event| match event {
                os_app::dto::UiEvent::Tick { tick } => {
                    tick.notices.iter().any(|n| n.code == "mcp-start-failed")
                }
                _ => false,
            });
            if noticed {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(call(&app, "mcp.status", Value::Null).await["kind"], "off");
}
