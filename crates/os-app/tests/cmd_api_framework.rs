// Fixture-Assertions lesen die gespeicherten Fakten direkt.
#![allow(clippy::disallowed_methods)]
#[path = "cmd_api_support.rs"]
mod support;
use support::*;
// spec: INTEG-001
#[tokio::test]
async fn connections_list_exactly_six_probe_once_disconnect_keeps_credentials() {
    let h = Harness::new().await;
    let rows = h.call("api.list", json!(null)).await.unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 7);
    assert!(rows
        .as_array()
        .unwrap()
        .iter()
        .all(|r| r["state"] == "disconnected"));
    h.connect().await;
    assert_eq!(h.mock.log.calls().len(), 1);
    let rows = h.call("api.list", json!(null)).await.unwrap();
    assert_eq!(rows[2]["state"], "connected");
    assert_eq!(rows[2]["accounts"][0]["keySuffix"], "-002");
    assert!(!rows
        .to_string()
        .contains(fixture()["secret"].as_str().unwrap()));
    let test = h
        .call("api.testConnection", json!({"kind":"psi"}))
        .await
        .unwrap();
    assert_eq!(test["ok"], true);
    assert_eq!(h.mock.log.calls().len(), 2);
    h.call("api.disconnect", json!({"kind":"psi"}))
        .await
        .unwrap();
    assert_eq!(
        h.call("api.list", json!(null)).await.unwrap()[2]["state"],
        "disconnected"
    );
    let failed = h
        .op("api.connect", json!({"kind":"psi","apiKey":"bad"}))
        .await
        .unwrap_err();
    assert_eq!(failed.code, ErrorCode::Permission);
    assert!(failed.detail.as_deref().unwrap().contains("401"));
    assert!(failed
        .detail
        .as_deref()
        .unwrap()
        .contains("invalid_credentials"));
    assert_eq!(h.mock.log.calls().len(), 3);
    assert_eq!(
        h.call("api.list", json!(null)).await.unwrap()[2]["state"],
        "error"
    );
    h.server.stop().await;
}
// spec: INTEG-008
#[tokio::test]
async fn fetch_after_crawl_and_missing_scope_do_not_resume_crawl() {
    let h = Harness::new().await;
    h.connect().await;
    let source = h.source(ApiKind::Psi);
    let result = h
        .op(
            "api.fetch",
            json!({"kinds":["psi"],"scope":{"kind":"missing"}}),
        )
        .await
        .unwrap();
    assert_eq!(
        (result["requested"].as_u64(), result["fetched"].as_u64()),
        (Some(6), Some(6))
    );
    assert_eq!(
        h.app
            .sessions()
            .current(&os_app::WindowId("main".into()))
            .unwrap()
            .state,
        CrawlState::Paused
    );
    h.op(
        "api.fetch",
        json!({"kinds":["psi"],"scope":{"kind":"missing"}}),
    )
    .await
    .unwrap();
    assert_eq!(source.calls.load(Ordering::SeqCst), 6);
    assert_eq!(h.mock.log.calls().len(), 1);
    h.server.stop().await;
}
// spec: INTEG-009
#[tokio::test]
async fn selected_urls_replace_values_and_clear_final_errors() {
    let h = Harness::new().await;
    h.connect().await;
    let source = h.source(ApiKind::Psi);
    source.fail.store(true, Ordering::SeqCst);
    let args = json!({"kinds":["psi"],"scope":{"kind":"selection","selection":{"kind":"ids","ids":[1,2,3,3]}}});
    assert_eq!(h.op("api.fetch", args.clone()).await.unwrap()["failed"], 3);
    assert_eq!(
        h.call("api.errors", json!({"kind":"psi","offset":0,"limit":100}))
            .await
            .unwrap()
            .as_array()
            .unwrap()
            .len(),
        3
    );
    source.fail.store(false, Ordering::SeqCst);
    assert_eq!(h.op("api.fetch", args.clone()).await.unwrap()["fetched"], 3);
    source.value.store(91, Ordering::SeqCst);
    h.op("api.fetch", args).await.unwrap();
    let rows = h
        .app
        .read_pool()
        .run(h.key.clone(), |file, _| {
            Ok((
                file.conn()
                    .query_row("SELECT COUNT(*) FROM api_values", [], |r| {
                        r.get::<_, u64>(0)
                    })?,
                file.conn()
                    .query_row("SELECT v_real FROM api_values WHERE url_id=1", [], |r| {
                        r.get::<_, f64>(0)
                    })?,
            ))
        })
        .await
        .unwrap();
    assert_eq!(rows, (3, 91.));
    assert_eq!(
        h.call("api.status", json!(null)).await.unwrap()[0]["errors"],
        0
    );
    h.server.stop().await;
}
// spec: UI-070 [M2]
#[tokio::test]
async fn sidebar_status_counts_completed_errors_and_hides_without_connections() {
    let h = Harness::new().await;
    assert_eq!(h.call("api.status", json!(null)).await.unwrap(), json!([]));
    h.connect().await;
    let source = h.source(ApiKind::Psi);
    source.fail.store(true, Ordering::SeqCst);
    h.op("api.fetch", json!({"scope":{"kind":"all"}}))
        .await
        .unwrap();
    let rows = h.call("sidebar.apiStatus", json!(null)).await.unwrap();
    assert_eq!(rows, h.call("api.status", json!(null)).await.unwrap());
    assert_eq!(rows[0]["requested"], 6);
    assert_eq!(rows[0]["completed"], 6);
    assert_eq!(rows[0]["errors"], 6);
    h.server.stop().await;
}
// spec: INTEG-010
#[tokio::test]
async fn data_settings_ask_keep_reload_and_limits_do_not_invalidate() {
    let h = Harness::new().await;
    h.connect_ga4().await;
    h.source(ApiKind::Ga4);
    h.op("api.fetch", json!({"kinds":["ga4"],"scope":{"kind":"all"}}))
        .await
        .unwrap();
    let mut settings = h
        .call("api.getSettings", json!({"kind":"ga4"}))
        .await
        .unwrap();
    settings["values"]["metrics"] = json!(["sessions"]);
    let args = json!({"kind":"ga4","settings":settings,"decision":"ask"});
    assert_eq!(
        h.call("api.setSettings", args.clone()).await.unwrap()["confirmationRequired"],
        true
    );
    let mut keep = args.clone();
    keep["decision"] = json!("keep");
    assert_eq!(
        h.call("api.setSettings", keep).await.unwrap()["dataStale"],
        true
    );
    settings["values"]["metrics"] = json!(["screenPageViews"]);
    h.app.ops().take_updates();
    let reload = json!({"kind":"ga4","settings":settings,"decision":"reload"});
    assert_eq!(
        h.call("api.setSettings", reload).await.unwrap()["invalidated"],
        true
    );
    let reload_op = h
        .app
        .ops()
        .take_updates()
        .into_iter()
        .find(|op| op.label_key == "op-api-fetch")
        .unwrap();
    let result = if reload_op.state == os_app::dto::OpState::Running {
        h.app.ops().wait(&reload_op.op_id).await.unwrap()
    } else {
        reload_op
    };
    assert!(result.error.is_none());
    assert_eq!(result.result.unwrap()["fetched"], 6);
    let mut settings = h
        .call("api.getSettings", json!({"kind":"ga4"}))
        .await
        .unwrap();
    settings["values"]["limits"]["concurrency"] = json!(2);
    assert_eq!(
        h.call("api.setSettings", json!({"kind":"ga4","settings":settings}))
            .await
            .unwrap()["confirmationRequired"],
        false
    );
    h.server.stop().await;
}
// spec: INTEG-002
#[test]
fn profile_never_contains_secrets_and_remove_account_clears_secret_store() {
    // Windows enforces SQLite byte-range locks against raw filesystem reads.
    // Stop the fixture runtime (including startup jobs) before scanning every
    // persisted byte. Keep credentials intact until after this offline scan.
    let (dir, config) = {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let h = Harness::new().await;
            h.connect().await;
            let settings = h
                .call("api.getSettings", json!({"kind":"psi"}))
                .await
                .unwrap();
            assert!(!settings
                .to_string()
                .contains(fixture()["secret"].as_str().unwrap()));
            let mut malicious = settings;
            malicious["values"]["api_key"] = fixture()["secret"].clone();
            assert_eq!(
                h.call(
                    "api.setSettings",
                    json!({"kind":"psi","settings":malicious})
                )
                .await
                .unwrap_err()
                .code,
                ErrorCode::InvalidInput
            );
            let conn = h
                .app
                .read_pool()
                .run(h.key.clone(), |file, _| Ok(file.config().unwrap()))
                .await
                .unwrap();
            assert!(!serde_json::to_string(&conn)
                .unwrap()
                .contains(fixture()["secret"].as_str().unwrap()));
            let config = h.app.config().clone();
            h.server.stop().await;
            (h._dir, config)
        })
    };
    fn scan(path: &std::path::Path, secret: &[u8]) {
        if path.is_dir() {
            for entry in std::fs::read_dir(path).unwrap() {
                scan(&entry.unwrap().path(), secret);
            }
        } else if path.is_file() {
            let bytes =
                std::fs::read(path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
            assert!(
                !bytes.windows(secret.len()).any(|w| w == secret),
                "Geheimnis in {}",
                path.display()
            );
        }
    }
    assert!(config.secrets.get("api/psi").is_some());
    for path in config.paths.all() {
        scan(path, fixture()["secret"].as_str().unwrap().as_bytes());
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let app = App::new(config).unwrap();
        app.dispatch(
            CallCtx::for_window("main"),
            "api.removeAccount",
            json!({"kind":"psi","accountId":"default"}),
        )
        .await
        .unwrap();
        assert!(app.config().secrets.get("api/psi").is_none());
    });
    drop(runtime);
    drop(dir);
}
// spec: INTEG-019
#[tokio::test]
async fn unregistered_wave_two_source_is_explicitly_unsupported() {
    let h = Harness::new().await;
    h.connect().await;
    let e = h
        .op("api.fetch", json!({"kinds":["psi"],"scope":{"kind":"all"}}))
        .await
        .unwrap_err();
    assert_eq!(e.code, ErrorCode::Unsupported);
    assert_eq!(e.args["owner"], "M2-I04");
    h.server.stop().await;
}

fn contract_rows<'a>(
    _: &'a App,
    _: CallCtx,
    args: Value,
) -> futures_util::future::BoxFuture<'a, os_app::CmdResult> {
    Box::pin(async move {
        if args["version"] != 7 {
            return Err(os_app::CmdError::new(ErrorCode::Stale, "err-view-stale"));
        }
        Ok(
            json!({"viewId":args["viewId"],"version":7,"offset":0,"rowKeys":[1,2,3],"columns":["Address"],"cells":[[],[],[]]}),
        )
    })
}
// spec: INTEG-009
#[tokio::test]
async fn selection_all_uses_view_rows_contract_and_rejects_stale_versions() {
    let base = os_app::default_registry();
    let mut registry = os_app::Registry::new();
    for meta in base.manifest() {
        let handler = if meta.name == "view.rows" {
            contract_rows as os_app::Handler
        } else {
            base.get(meta.name).unwrap().1
        };
        registry.register(meta, handler);
    }
    let h = Harness::new_with_registry(registry).await;
    h.connect().await;
    let source = h.source(ApiKind::Psi);
    let mut args = json!({"scope":{"kind":"selection","selection":{"kind":"all","viewId":"fixture","version":7,"except":[2]}}});
    assert_eq!(h.op("api.fetch", args.clone()).await.unwrap()["fetched"], 2);
    assert_eq!(source.calls.load(Ordering::SeqCst), 2);
    args["scope"]["selection"]["version"] = json!(6);
    assert_eq!(
        h.op("api.fetch", args).await.unwrap_err().code,
        ErrorCode::Stale
    );
    assert_eq!(source.calls.load(Ordering::SeqCst), 2);
    h.server.stop().await;
}
// spec: INTEG-006
#[tokio::test]
async fn custom_quota_stops_before_provider_calls_and_is_visible_in_status() {
    let h = Harness::new().await;
    h.connect().await;
    let source = h.source(ApiKind::Psi);
    let mut settings = h
        .call("api.getSettings", json!({"kind":"psi"}))
        .await
        .unwrap();
    settings["values"]["limits"]["quota_limit"] = json!(5);
    h.call("api.setSettings", json!({"kind":"psi","settings":settings}))
        .await
        .unwrap();
    let summary = h
        .op("api.fetch", json!({"scope":{"kind":"all"}}))
        .await
        .unwrap();
    assert_eq!(summary["fetched"], 5);
    assert_eq!(summary["failed"], 1);
    assert_eq!(summary["quotaLeft"], 0);
    assert_eq!(source.calls.load(Ordering::SeqCst), 5);
    let status = h.call("api.status", json!(null)).await.unwrap();
    assert_eq!(status[0]["quotaLeft"], 0);
    assert!(status[0]["resetAt"].is_null());
    assert!(h
        .call("api.errors", json!({"kind":"psi","offset":0,"limit":10}))
        .await
        .unwrap()[0]["message"]
        .as_str()
        .unwrap()
        .contains("Quota exceeded"));
    h.server.stop().await;
}

struct ControlledSource {
    started: tokio::sync::Notify,
    release: tokio::sync::Notify,
    calls: AtomicU64,
}
impl ApiSource for ControlledSource {
    fn kind(&self) -> ApiKind {
        ApiKind::Psi
    }
    fn status(&self) -> ApiStatus {
        ApiStatus {
            kind: self.kind(),
            state: os_integrations::ApiState::Idle,
            urls_total: 0,
            urls_done: 0,
            urls_failed: 0,
            detail: None,
        }
    }
    fn fetch<'a>(
        &'a self,
        urls: &'a [UrlRef],
        _: &'a ApiCtx,
    ) -> os_integrations::BoxFuture<'a, Result<Vec<ApiValueWrite>, ApiError>> {
        Box::pin(async move {
            if self.calls.fetch_add(1, Ordering::SeqCst) == 0 {
                self.started.notify_one();
                self.release.notified().await;
            }
            Ok(urls
                .iter()
                .map(|u| ApiValueWrite {
                    url_id: u.url_id,
                    url: u.url.clone(),
                    source: "psi".into(),
                    metric: "score".into(),
                    level: "url".into(),
                    v_real: Some(42.),
                    v_text: None,
                    fetched_at: Timestamp::UNIX_EPOCH,
                })
                .collect())
        })
    }
}
// spec: INTEG-007
#[tokio::test]
async fn lifecycle_pause_live_values_and_completion_barrier_include_commits() {
    use os_app::runtime::lifecycle_bus::CrawlEvent;
    let h = Harness::new().await;
    h.connect().await;
    let mut settings = h
        .call("api.getSettings", json!({"kind":"psi"}))
        .await
        .unwrap();
    settings["values"]["limits"]["concurrency"] = json!(1);
    h.call("api.setSettings", json!({"kind":"psi","settings":settings}))
        .await
        .unwrap();
    let source = Arc::new(ControlledSource {
        started: Default::default(),
        release: Default::default(),
        calls: AtomicU64::new(0),
    });
    h.app.ext::<ApiState>().register_source(source.clone());
    let op = h
        .call("api.fetch", json!({"scope":{"kind":"all"}}))
        .await
        .unwrap();
    source.started.notified().await;
    h.app
        .crawl_events()
        .emit(&h.app, CrawlEvent::Paused(h.key.clone()));
    source.release.notify_one();
    let wait_live = async {
        loop {
            let status = h.call("api.status", json!(null)).await.unwrap();
            if status[0]["completed"] == 1 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    };
    tokio::time::timeout(Duration::from_secs(5), wait_live)
        .await
        .unwrap();
    assert_eq!(source.calls.load(Ordering::SeqCst), 1);
    let count = h
        .app
        .read_pool()
        .run(h.key.clone(), |file, _| {
            Ok(file
                .conn()
                .query_row("SELECT COUNT(*) FROM api_values", [], |r| {
                    r.get::<_, u64>(0)
                })?)
        })
        .await
        .unwrap();
    assert_eq!(count, 1);
    let app = h.app.clone();
    let key = h.key.clone();
    let mut barrier = tokio::spawn(async move {
        app.crawl_events().wait_barriers(&key).await;
    });
    assert!(
        tokio::time::timeout(Duration::from_millis(50), &mut barrier)
            .await
            .is_err()
    );
    h.app
        .crawl_events()
        .emit(&h.app, CrawlEvent::Resumed(h.key.clone()));
    barrier.await.unwrap();
    let status = h
        .app
        .ops()
        .wait(op["opId"].as_str().unwrap())
        .await
        .unwrap();
    assert!(status.error.is_none());
    let count = h
        .app
        .read_pool()
        .run(h.key.clone(), |file, _| {
            Ok(file
                .conn()
                .query_row("SELECT COUNT(*) FROM api_values", [], |r| {
                    r.get::<_, u64>(0)
                })?)
        })
        .await
        .unwrap();
    assert_eq!(count, 6);
    h.server.stop().await;
}

struct HangingProbe {
    started: tokio::sync::Notify,
}
impl ApiSource for HangingProbe {
    fn kind(&self) -> ApiKind {
        ApiKind::Psi
    }
    fn status(&self) -> ApiStatus {
        ApiStatus {
            kind: self.kind(),
            state: os_integrations::ApiState::Idle,
            urls_total: 0,
            urls_done: 0,
            urls_failed: 0,
            detail: None,
        }
    }
    fn fetch<'a>(
        &'a self,
        _: &'a [UrlRef],
        _: &'a ApiCtx,
    ) -> os_integrations::BoxFuture<'a, Result<Vec<ApiValueWrite>, ApiError>> {
        Box::pin(std::future::pending())
    }
    fn test_connection<'a>(
        &'a self,
        _: &'a ApiCtx,
        _: &'a Credential,
    ) -> os_integrations::BoxFuture<'a, Result<Credential, ApiError>> {
        Box::pin(async move {
            self.started.notify_one();
            std::future::pending().await
        })
    }
}
// spec: INTEG-001
#[tokio::test]
async fn connect_timeout_is_thirty_seconds_and_cancel_clears_connecting() {
    let h = Harness::new().await;
    let source = Arc::new(HangingProbe {
        started: Default::default(),
    });
    h.app.ext::<ApiState>().register_source(source.clone());
    let handle = h
        .call("api.connect", json!({"kind":"psi","apiKey":"fixture"}))
        .await
        .unwrap();
    source.started.notified().await;
    assert_eq!(
        h.call("api.list", json!(null)).await.unwrap()[2]["state"],
        "connecting"
    );
    tokio::time::pause();
    tokio::time::advance(Duration::from_secs(30)).await;
    let status = h
        .app
        .ops()
        .wait(handle["opId"].as_str().unwrap())
        .await
        .unwrap();
    assert_eq!(status.error.unwrap().code, ErrorCode::Timeout);
    tokio::time::resume();
    assert_eq!(
        h.call("api.list", json!(null)).await.unwrap()[2]["state"],
        "error"
    );
    let handle = h
        .call("api.connect", json!({"kind":"psi","apiKey":"fixture"}))
        .await
        .unwrap();
    source.started.notified().await;
    h.app.ops().cancel(handle["opId"].as_str().unwrap());
    let status = h
        .app
        .ops()
        .wait(handle["opId"].as_str().unwrap())
        .await
        .unwrap();
    assert_eq!(status.state, os_app::dto::OpState::Cancelled);
    assert_eq!(
        h.call("api.list", json!(null)).await.unwrap()[2]["state"],
        "disconnected"
    );
    h.server.stop().await;
}
// spec: INTEG-001
#[tokio::test]
async fn test_connection_timeout_is_ten_seconds() {
    let h = Harness::new().await;
    h.connect().await;
    let source = Arc::new(HangingProbe {
        started: Default::default(),
    });
    h.app.ext::<ApiState>().register_source(source.clone());
    let app = h.app.clone();
    let task = tokio::spawn(async move {
        app.dispatch(
            CallCtx::for_window("main"),
            "api.testConnection",
            json!({"kind":"psi"}),
        )
        .await
    });
    source.started.notified().await;
    tokio::time::pause();
    tokio::time::advance(Duration::from_secs(10)).await;
    let result = task.await.unwrap().unwrap();
    tokio::time::resume();
    assert_eq!(result["ok"], false);
    assert_eq!(result["error"]["code"], "Timeout");
    h.server.stop().await;
}
// spec: INTEG-004
#[tokio::test]
async fn startup_warns_thirty_days_before_ahrefs_expiry() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = AppConfig::new(dir.path());
    let now = Timestamp::from_db("2026-10-18T12:00:00Z").unwrap();
    config.clock = Arc::new(os_model::ManualClock::new(now));
    Credentials::new(config.secrets.clone())
        .put(
            ApiKind::Ahrefs,
            Credential {
                account_id: "fixture".into(),
                label: "Fixture".into(),
                api_key: Some(fixture()["secret"].as_str().unwrap().into()),
                access_token: None,
                refresh_token: None,
                expires_at: Some(now.plus(Duration::from_secs(30 * 24 * 3600))),
                client_secret: None,
                scopes: vec![],
            },
            true,
        )
        .unwrap();
    let app = App::new(config).unwrap();
    assert!(app
        .assemble_tick()
        .unwrap()
        .notices
        .iter()
        .any(|n| n.code == "notice-api-token-expiring"));
}

struct QuotaSource;
impl ApiSource for QuotaSource {
    fn kind(&self) -> ApiKind {
        ApiKind::Ga4
    }
    fn status(&self) -> ApiStatus {
        ApiStatus {
            kind: self.kind(),
            state: os_integrations::ApiState::Idle,
            urls_total: 0,
            urls_done: 0,
            urls_failed: 0,
            detail: None,
        }
    }
    fn fetch<'a>(
        &'a self,
        _: &'a [UrlRef],
        _: &'a ApiCtx,
    ) -> os_integrations::BoxFuture<'a, Result<Vec<ApiValueWrite>, ApiError>> {
        Box::pin(async { Ok(vec![]) })
    }
    fn quotas(&self) -> Vec<os_integrations::core::ProviderQuota> {
        vec![
            os_integrations::core::ProviderQuota {
                dimension: "tokensPerDay".into(),
                used: 40,
                limit: Some(100),
                resets_at: None,
            },
            os_integrations::core::ProviderQuota {
                dimension: "tokensPerHour".into(),
                used: 20,
                limit: Some(30),
                resets_at: None,
            },
        ]
    }
}
// spec: INTEG-006
#[tokio::test]
async fn provider_daily_and_hourly_quota_dimensions_remain_separate() {
    let h = Harness::new().await;
    h.app
        .ext::<ApiState>()
        .register_source(Arc::new(QuotaSource));
    h.connect_ga4().await;
    let info = h.call("api.list", json!(null)).await.unwrap();
    assert_eq!(
        h.call("api.status", json!(null)).await.unwrap()[0]["quotaLeft"],
        10
    );
    assert_eq!(info[0]["quotaDimensions"][0]["dimension"], "tokensPerDay");
    assert_eq!(info[0]["quotaDimensions"][0]["used"], 40);
    assert_eq!(info[0]["quotaDimensions"][1]["dimension"], "tokensPerHour");
    assert_eq!(info[0]["quotaDimensions"][1]["used"], 20);
    assert_eq!(
        h.op("api.fetch", json!({"kinds":["ga4"],"scope":{"kind":"all"}}))
            .await
            .unwrap()["quotaLeft"],
        10
    );
    h.connect().await;
    h.source(ApiKind::Psi);
    let mut settings = h
        .call("api.getSettings", json!({"kind":"psi"}))
        .await
        .unwrap();
    settings["values"]["limits"]["quota_limit"] = json!(9);
    h.call("api.setSettings", json!({"kind":"psi","settings":settings}))
        .await
        .unwrap();
    assert_eq!(
        h.op(
            "api.fetch",
            json!({"kinds":["ga4","psi"],"scope":{"kind":"all"}})
        )
        .await
        .unwrap()["quotaLeft"],
        3
    );
    h.server.stop().await;
}

// spec: INTEG-006
#[tokio::test]
#[allow(clippy::disallowed_types)]
async fn a_new_crawl_gets_its_own_custom_cap_while_sharing_the_provider_day() {
    let h = Harness::new().await;
    h.connect().await;
    let source = h.source(ApiKind::Psi);
    let mut settings = h
        .call("api.getSettings", json!({"kind":"psi"}))
        .await
        .unwrap();
    settings["values"]["limits"]["quota_limit"] = json!(5);
    h.call("api.setSettings", json!({"kind":"psi","settings":settings}))
        .await
        .unwrap();
    assert_eq!(
        h.op("api.fetch", json!({"scope":{"kind":"all"}}))
            .await
            .unwrap()["fetched"],
        5
    );
    let session = h
        .app
        .sessions()
        .current(&os_app::WindowId("main".into()))
        .unwrap();
    let now = h.app.config().clock.now();
    let path = h._dir.path().join("second.ospider");
    let key = CrawlKey("second-crawl".into());
    let init = CrawlInit {
        crawl_id: SeededIds::new(999).new_crawl_id(now),
        mode: CrawlMode::Spider,
        start_url: None,
        list_source: None,
        project: "quota fixture".into(),
        name: "second".into(),
        config: (*session.config).clone(),
        created_at: now,
    };
    let file = CrawlFile::create(&path, &init, StorageProfile::default()).unwrap();
    for (i, url) in fixture()["urls"].as_array().unwrap()[..3]
        .iter()
        .enumerate()
    {
        let url = url.as_str().unwrap();
        file.conn().execute("INSERT INTO urls(id,url,address,origin,is_internal,state,discovered_via,folder_depth,url_length) VALUES(?1,?2,?2,'https://integ.test:8080',1,'fetched','seed',0,?3)",rusqlite::params![i as i64+1,url,url.len()]).unwrap();
    }
    drop(file);
    h.app.register_crawl(key.clone(), path.clone());
    h.app.sessions().set_current(
        os_app::WindowId("main".into()),
        SessionInfo {
            key,
            path,
            ..session
        },
    );
    let result = h
        .op("api.fetch", json!({"scope":{"kind":"all"}}))
        .await
        .unwrap();
    assert_eq!(result["fetched"], 3);
    assert_eq!(result["failed"], 0);
    assert_eq!(source.calls.load(Ordering::SeqCst), 8);
    h.server.stop().await;
}

struct ExpiredSource {
    calls: AtomicU64,
}
impl ApiSource for ExpiredSource {
    fn kind(&self) -> ApiKind {
        ApiKind::Ga4
    }
    fn status(&self) -> ApiStatus {
        ApiStatus {
            kind: self.kind(),
            state: os_integrations::ApiState::Idle,
            urls_total: 0,
            urls_done: 0,
            urls_failed: 0,
            detail: None,
        }
    }
    fn fetch<'a>(
        &'a self,
        _: &'a [UrlRef],
        _: &'a ApiCtx,
    ) -> os_integrations::BoxFuture<'a, Result<Vec<ApiValueWrite>, ApiError>> {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Err(ApiError::Unauthorized("fixture access expired".into()))
        })
    }
}
// spec: INTEG-004
#[tokio::test]
async fn invalid_grant_is_a_reconnection_error_for_every_pending_url() {
    let h = Harness::new().await;
    h.connect_ga4().await;
    let mut settings = h
        .call("api.getSettings", json!({"kind":"ga4"}))
        .await
        .unwrap();
    settings["values"]["limits"]["concurrency"] = json!(1);
    h.call("api.setSettings", json!({"kind":"ga4","settings":settings}))
        .await
        .unwrap();
    let oauth = MockServer::start(vec![Arc::new(
        os_testsite::mocks::oauth::OAuthMock::default(),
    )])
    .await
    .unwrap();
    h.app
        .ext::<ApiState>()
        .set_oauth_endpoints(os_integrations::oauth::Endpoints {
            authorize: format!("{}/authorize", oauth.base_url("oauth")),
            token: format!("{}/token?scenario=invalid_grant", oauth.base_url("oauth")),
            revoke: format!("{}/revoke", oauth.base_url("oauth")),
        });
    let source = Arc::new(ExpiredSource {
        calls: AtomicU64::new(0),
    });
    h.app.ext::<ApiState>().register_source(source.clone());
    let summary = h
        .op("api.fetch", json!({"kinds":["ga4"],"scope":{"kind":"all"}}))
        .await
        .unwrap();
    assert_eq!(summary["failed"], 6);
    assert_eq!(source.calls.load(Ordering::SeqCst), 1);
    let errors = h
        .call("api.errors", json!({"kind":"ga4","offset":0,"limit":100}))
        .await
        .unwrap();
    assert!(errors
        .as_array()
        .unwrap()
        .iter()
        .all(|error| error["message"].as_str().unwrap().contains("neu verbinden")));
    assert_eq!(
        h.call("api.list", json!(null)).await.unwrap()[0]["state"],
        "error"
    );
    assert_eq!(
        h.app
            .sessions()
            .current(&os_app::WindowId("main".into()))
            .unwrap()
            .state,
        CrawlState::Paused
    );
    oauth.stop().await;
    h.server.stop().await;
}

// spec: INTEG-002
#[tokio::test]
async fn entering_a_new_key_works_when_the_keychain_is_locked() {
    struct Locked;
    impl os_fetch::SecretStore for Locked {
        fn get(&self, _: &str) -> Option<String> {
            None
        }
        fn try_get(&self, _: &str) -> Result<Option<String>, os_fetch::secrets::SecretError> {
            Err(os_fetch::secrets::SecretError::new("locked fixture"))
        }
    }
    impl os_fetch::SecretStoreMut for Locked {
        fn set(&self, _: &str, _: &str) -> Result<(), os_fetch::secrets::SecretError> {
            Err(os_fetch::secrets::SecretError::new("locked fixture"))
        }
        fn delete(&self, _: &str) -> Result<(), os_fetch::secrets::SecretError> {
            Err(os_fetch::secrets::SecretError::new("locked fixture"))
        }
        fn backend(&self) -> os_fetch::secrets::SecretBackend {
            os_fetch::secrets::SecretBackend::Keyring
        }
    }
    let dir = tempfile::tempdir().unwrap();
    let mut config = AppConfig::new(dir.path());
    config.secrets = Arc::new(Locked);
    let app = App::new(config).unwrap();
    let server = MockServer::start(vec![Arc::new(FixtureApi::default())])
        .await
        .unwrap();
    let context = CallCtx::for_window("main");
    let mut settings = app
        .dispatch(context.clone(), "api.getSettings", json!({"kind":"psi"}))
        .await
        .unwrap();
    settings["values"]["base_url"] = json!(server.base_url("common"));
    app.dispatch(
        context.clone(),
        "api.setSettings",
        json!({"kind":"psi","settings":settings}),
    )
    .await
    .unwrap();
    let op = app
        .dispatch(
            context.clone(),
            "api.connect",
            json!({"kind":"psi","apiKey":fixture()["secret"],"remember":true}),
        )
        .await
        .unwrap();
    let status = app.ops().wait(op["opId"].as_str().unwrap()).await.unwrap();
    assert_eq!(status.state, os_app::dto::OpState::Done);
    assert!(app
        .dispatch(context.clone(), "api.testConnection", json!({"kind":"psi"}))
        .await
        .unwrap()["ok"]
        .as_bool()
        .unwrap());
    assert!(app
        .assemble_tick()
        .unwrap()
        .notices
        .iter()
        .any(|n| n.code == "notice-api-session-only"));
    assert_eq!(
        app.dispatch(context, "api.list", json!(null))
            .await
            .unwrap()[2]["state"],
        "connected"
    );
    server.stop().await;
}

// spec: INTEG-010
#[tokio::test]
async fn reload_returns_while_fetch_is_pending_and_is_cancellable() {
    let h = Harness::new().await;
    h.connect().await;
    h.source(ApiKind::Psi);
    h.op("api.fetch", json!({"kinds":["psi"],"scope":{"kind":"all"}}))
        .await
        .unwrap();
    let source = Arc::new(ControlledSource {
        started: Default::default(),
        release: Default::default(),
        calls: Default::default(),
    });
    h.app.ext::<ApiState>().register_source(source.clone());
    let mut settings = h
        .call("api.getSettings", json!({"kind":"psi"}))
        .await
        .unwrap();
    settings["values"]["device"] = json!("desktop");
    h.app.ops().take_updates();
    let result = tokio::time::timeout(
        Duration::from_secs(2),
        h.call(
            "api.setSettings",
            json!({"kind":"psi","settings":settings,"decision":"reload"}),
        ),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(result["invalidated"], true);
    source.started.notified().await;
    let op = h
        .app
        .ops()
        .take_updates()
        .into_iter()
        .find(|op| op.label_key == "op-api-fetch")
        .unwrap();
    assert_eq!(op.state, os_app::dto::OpState::Running);
    assert!(h.app.ops().cancel(&op.op_id));
    assert_eq!(
        h.app.ops().wait(&op.op_id).await.unwrap().state,
        os_app::dto::OpState::Cancelled
    );
    h.server.stop().await;
}

// spec: INTEG-010
#[tokio::test]
async fn reload_without_registered_source_succeeds_and_reports_fetch_error_in_ops() {
    let h = Harness::new().await;
    h.connect().await;
    h.app
        .write_queue()
        .submit(h.key.clone(), |conn| {
            os_store::queries::api::replace_values(
                conn,
                "psi",
                &[],
                &[ApiValueWrite {
                    url_id: Some(os_model::UrlId(1)),
                    url: fixture()["urls"][0].as_str().unwrap().into(),
                    source: "psi".into(),
                    metric: "fixture.score".into(),
                    level: "url".into(),
                    v_real: Some(42.),
                    v_text: None,
                    fetched_at: Timestamp::UNIX_EPOCH,
                }],
            )
        })
        .await
        .unwrap();
    let mut settings = h
        .call("api.getSettings", json!({"kind":"psi"}))
        .await
        .unwrap();
    settings["values"]["device"] = json!("desktop");
    h.app.ops().take_updates();
    let result = h
        .call(
            "api.setSettings",
            json!({"kind":"psi","settings":settings,"decision":"reload"}),
        )
        .await
        .unwrap();
    assert_eq!(result["invalidated"], true);
    let op = h
        .app
        .ops()
        .take_updates()
        .into_iter()
        .find(|op| op.label_key == "op-api-fetch")
        .unwrap();
    let result = if op.state == os_app::dto::OpState::Running {
        h.app.ops().wait(&op.op_id).await.unwrap()
    } else {
        op
    };
    assert_eq!(result.error.unwrap().code, ErrorCode::Unsupported);
    assert_eq!(
        h.call("api.getSettings", json!({"kind":"psi"}))
            .await
            .unwrap()["values"]["device"],
        "desktop"
    );
    let has_data = h
        .app
        .read_pool()
        .run(h.key.clone(), |file, _| {
            os_store::queries::api::has_values(file.conn(), "psi")
        })
        .await
        .unwrap();
    assert!(!has_data);
    h.server.stop().await;
}

// spec: INTEG-009
#[tokio::test]
async fn selection_rejects_ga4_and_gsc_before_resolving_sources_or_views() {
    let h = Harness::new().await;
    for kind in ["ga4", "gsc"] {
        for selection in [
            json!({"kind":"ids","ids":[1]}),
            json!({"kind":"all","viewId":"missing","version":1,"except":[]}),
        ] {
            let error = h
                .op(
                    "api.fetch",
                    json!({"kinds":[kind],"scope":{"kind":"selection","selection":selection}}),
                )
                .await
                .unwrap_err();
            assert_eq!(error.code, ErrorCode::InvalidInput);
        }
    }
    h.server.stop().await;
}

// spec: INTEG-006
#[tokio::test]
async fn provider_day_and_crawl_cap_have_independent_usage_and_summary_minimum() {
    let h = Harness::new().await;
    h.connect().await;
    let source = h.source(ApiKind::Psi);
    let ledger = os_integrations::quota::QuotaLedger::new(
        h.app.config().paths.settings.join("api-quotas.json"),
    );
    let now = h.app.config().clock.now();
    ledger.exhaust("psi", "", now, 24999).unwrap();
    let mut settings = h
        .call("api.getSettings", json!({"kind":"psi"}))
        .await
        .unwrap();
    settings["values"]["limits"]["quota_limit"] = json!(5);
    h.call("api.setSettings", json!({"kind":"psi","settings":settings}))
        .await
        .unwrap();
    let rows = h.call("api.list", json!(null)).await.unwrap();
    assert_eq!(rows[2]["quota"]["dimension"], "urlsPerCrawl");
    assert_eq!(rows[2]["quota"]["used"], 0);
    assert_eq!(rows[2]["quotaDimensions"][0]["used"], 24999);
    let summary = h
        .op("api.fetch", json!({"kinds":["psi"],"scope":{"kind":"all"}}))
        .await
        .unwrap();
    assert_eq!(summary["quotaLeft"], 0);
    assert_eq!(source.calls.load(Ordering::SeqCst), 1);
    let rows = h.call("api.list", json!(null)).await.unwrap();
    assert_eq!(rows[2]["quota"]["used"], 1);
    assert_eq!(rows[2]["quotaDimensions"][0]["used"], 25000);
    h.app.crawl_events().emit(
        &h.app,
        os_app::runtime::lifecycle_bus::CrawlEvent::Closed(h.key.clone()),
    );
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let rows = h.call("api.list", json!(null)).await.unwrap();
            if rows[2]["quota"]["used"] == 0 {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(ledger.remaining("psi", "", now, 25000).unwrap(), 0);
    h.server.stop().await;
}
// RC1-PSILOCAL (Issue 21): „Verbinden“ mit Quelle Lokal prüft die Lighthouse-Laufzeit, nicht einen Schlüssel.
// spec: INTEG-130
// spec: INTEG-132
#[tokio::test]
async fn local_psi_connect_checks_the_runtime_and_remote_still_needs_a_key() {
    let h = Harness::new().await;
    let window = os_app::WindowId("main".into());
    let set_local = |local: bool| {
        let mut session = h.app.sessions().current(&window).unwrap();
        let mut cfg = (*session.config).clone();
        cfg.api.psi.local = local;
        session.config = Arc::new(cfg);
        h.app.sessions().set_current(window.clone(), session);
    };
    set_local(true);
    let failed = h
        .op("api.connect", json!({"kind":"psi","remember":true}))
        .await
        .unwrap_err();
    assert_eq!(failed.message_key, "err-api-lighthouse-missing");
    assert_eq!(failed.code, ErrorCode::Permission);
    assert_eq!(h.mock.log.calls().len(), 0, "keine Anfrage an die PSI-API");
    assert_eq!(
        h.call("api.list", json!(null)).await.unwrap()[2]["state"],
        "error"
    );
    h.call("api.disconnect", json!({"kind":"psi"}))
        .await
        .unwrap();
    // Remote bleibt unverändert: ohne Schlüssel kein Verbindungsversuch, keine Umstellung auf Lokal.
    set_local(false);
    let failed = h
        .op("api.connect", json!({"kind":"psi","remember":true}))
        .await
        .unwrap_err();
    assert_eq!(failed.message_key, "err-api-key");
    assert_eq!(h.mock.log.calls().len(), 0);
    h.server.stop().await;
}
// RC1-PSILOCAL: „Verbinden“ nutzt Quelle und Lighthouse-Pfad aus dem Dialog, auch ungespeichert.
// spec: INTEG-132
#[tokio::test]
async fn local_psi_connect_uses_the_lighthouse_path_from_the_dialog() {
    let h = Harness::new().await;
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("fehlt").join("lighthouse");
    let failed = h
        .op(
            "api.connect",
            json!({"kind":"psi","psiLocal":true,"lighthousePath":missing.to_string_lossy()}),
        )
        .await
        .unwrap_err();
    assert_eq!(failed.message_key, "err-api-lighthouse-missing");
    assert_eq!(h.mock.log.calls().len(), 0, "keine Anfrage an die PSI-API");
    h.call("api.disconnect", json!({"kind":"psi"}))
        .await
        .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let cli = dir.path().join("lighthouse");
        std::fs::write(&cli, "#!/bin/sh\necho 13.0.0\n").unwrap();
        std::fs::set_permissions(&cli, std::fs::Permissions::from_mode(0o755)).unwrap();
        let account = h
            .op(
                "api.connect",
                json!({"kind":"psi","psiLocal":true,"lighthousePath":cli.to_string_lossy(),"remember":false}),
            )
            .await
            .unwrap();
        assert_eq!(account["account"], "Lighthouse 13.0.0");
        let rows = h.call("api.list", json!(null)).await.unwrap();
        assert_eq!(rows[2]["state"], "connected");
        assert_eq!(rows[2]["account"], "Lighthouse 13.0.0");
        assert_eq!(h.mock.log.calls().len(), 0);
    }
    h.server.stop().await;
}
