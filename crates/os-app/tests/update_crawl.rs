#![allow(clippy::disallowed_types, clippy::disallowed_methods)]
#[path = "session_common.rs"]
mod common;
use common::*;
use os_app::{
    runtime::lifecycle_bus::CrawlEvent,
    update::{self, UpdateContext},
};
use os_model::{CrawlState, ProxyMode};
use std::sync::{Arc, Mutex};

// spec: CRAWL-142
// spec: OUT-250
#[tokio::test]
async fn check_and_simulated_background_download_do_not_change_running_crawl_state() {
    let site = os_testsite::TestSite::start_dir(fixture(), 0)
        .await
        .unwrap();
    let updates = os_testsite::mocks::update::https_fixture().await.unwrap();
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    cfg().save_default(&app.config().paths.settings).unwrap();
    let transitions = Arc::new(Mutex::new(Vec::new()));
    let observed = transitions.clone();
    app.crawl_events().subscribe(Box::new(move |_, event| {
        if matches!(
            event,
            CrawlEvent::Started(_)
                | CrawlEvent::Paused(_)
                | CrawlEvent::Resumed(_)
                | CrawlEvent::Completed(_)
        ) {
            observed.lock().unwrap().push(());
        }
    }));
    call(
        &app,
        "crawl.start",
        serde_json::to_value(request(site.url("/"))).unwrap(),
    )
    .await;
    assert_eq!(
        app.sessions().current(&window()).unwrap().state,
        CrawlState::Running
    );
    let before = transitions.lock().unwrap().len();
    let crawl_requests_before = site.requests().len();
    let ca = dir.path().join("ca.pem");
    std::fs::write(&ca, updates.ca_cert_pem()).unwrap();
    let mut context = UpdateContext {
        endpoint: format!("{}/latest.json", updates.https_base_url()),
        pubkey: include_str!("../../../tests/fixtures/update/test.key.pub")
            .trim()
            .into(),
        current: "1.0.0".parse().unwrap(),
        os: "darwin".into(),
        arch: "aarch64".into(),
        installer: update::policy::Installer::Platform,
        disabled_by_environment: false,
        ..Default::default()
    };
    context.network.proxy.mode = ProxyMode::None;
    context
        .network
        .trusted_certificates
        .push(ca.to_string_lossy().into_owned());
    let checking = update::check(&context, true);
    // U02 owns actual staging/installation. This exercises a concurrent network download without
    // touching engine control; Q02 measures throughput on the reference machine.
    let downloading = async {
        let client = os_fetch::net::client_builder_with_password(&context.network, None)
            .build()
            .unwrap();
        let bytes = client
            .get(format!("{}/background-artifact", updates.https_base_url()))
            .send()
            .await
            .unwrap()
            .bytes()
            .await
            .unwrap();
        update::verify_release(
            &bytes,
            include_str!("../../../tests/fixtures/update/artifact.sig"),
            &context.pubkey,
            &"1.0.1".parse().unwrap(),
        )
        .unwrap();
    };
    let (state, ()) = tokio::join!(checking, downloading);
    assert!(matches!(state, update::UpdateState::Available(_)));
    assert_eq!(
        app.sessions().current(&window()).unwrap().state,
        CrawlState::Running
    );
    assert_eq!(transitions.lock().unwrap().len() - before, 0);
    assert_eq!(updates.requests().len(), 2);
    assert!(site.requests().len() > crawl_requests_before);
    call(&app, "crawl.pause", serde_json::Value::Null).await;
}

// spec: OUT-224 [M3]
// spec: OUT-222 [M3]
#[tokio::test]
async fn update_settings_are_persisted_but_never_enter_config_diff() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    let path = dir.path().join("diff.ospider");
    drop(create(&path, os_model::CrawlConfig::default()));
    open(&app, &path).await;
    let args = serde_json::json!({"base":{"kind":"defaults"}});
    let before = call(&app, "config.diff", args.clone()).await;
    call(&app, "update.set_auto", serde_json::json!({"check":false})).await;
    let after = call(&app, "config.diff", args).await;
    assert_eq!(before, after);
    assert_eq!(after, serde_json::json!([]));
    let persisted = os_model::settings::AppSettings::from_toml_str(
        &std::fs::read_to_string(app.settings().path()).unwrap(),
    )
    .unwrap();
    assert!(!persisted.updates.check);
}
