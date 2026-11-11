//! CRAWL-151: Die Debug-Info nennt je Crawl den eingestellten Wert „Verbindungen pro Host“.
#![allow(clippy::disallowed_types, clippy::disallowed_methods)]
#[path = "runtime_common.rs"]
mod common;

use os_app::pool::CrawlKey;
use os_app::runtime::sessions::SessionInfo;
use os_app::WindowId;
use os_model::{ConnectionsPerHost, CrawlConfig, CrawlState};
use serde_json::{json, Value};
use std::sync::Arc;

async fn debug_value(cfg: Option<CrawlConfig>) -> Option<Value> {
    let dir = tempfile::tempdir().unwrap();
    let app = common::new_app(dir.path());
    if let Some(cfg) = cfg {
        let path = common::make_crawl(dir.path(), "c151.ospider");
        let key = CrawlKey("c151".into());
        app.register_crawl(key.clone(), path.clone());
        app.sessions().set_current(
            WindowId("main".into()),
            SessionInfo {
                key,
                path,
                config: Arc::new(cfg),
                state: CrawlState::Paused,
                writable: true,
                engine: None,
            },
        );
    }
    let debug = app
        .dispatch(common::ctx(), "diag.debugInfo", Value::Null)
        .await
        .unwrap();
    debug["extra"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["name"] == "connectionsPerHost")
        .map(|e| e["value"].clone())
}

// spec: CRAWL-151
#[tokio::test]
async fn debug_info_names_the_configured_connections_per_host_of_the_crawl() {
    assert_eq!(
        debug_value(Some(CrawlConfig::default())).await,
        Some(json!("auto"))
    );
    let mut cfg = CrawlConfig::default();
    cfg.speed.connections_per_host = ConnectionsPerHost::Fixed(3);
    assert_eq!(debug_value(Some(cfg)).await, Some(json!(3)));
    assert_eq!(debug_value(None).await, None, "ohne Crawl kein Wert");
}
