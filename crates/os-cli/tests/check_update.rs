#![allow(clippy::disallowed_types, clippy::disallowed_methods)]
use os_cli::update::{self, policy::UpdateContext};
use os_model::ProxyMode;

// spec: OUT-250
#[tokio::test]
async fn manual_cli_returns_zero_for_current_and_ten_for_newer() {
    let site = os_testsite::mocks::update::https_fixture().await.unwrap();
    let dir = tempfile::tempdir().unwrap();
    let ca = dir.path().join("ca.pem");
    std::fs::write(&ca, site.ca_cert_pem()).unwrap();
    let mut ctx = UpdateContext {
        endpoint: format!("{}/latest.json", site.https_base_url()),
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
    ctx.network.proxy.mode = ProxyMode::None;
    ctx.network
        .trusted_certificates
        .push(ca.to_string_lossy().into_owned());
    assert_eq!(update::check_with_context(&ctx).await.unwrap(), 10);
    ctx.current = "1.0.1".parse().unwrap();
    assert_eq!(update::check_with_context(&ctx).await.unwrap(), 0);
    ctx.current = "2.0.0".parse().unwrap();
    assert_eq!(update::check_with_context(&ctx).await.unwrap(), 0);
    assert_eq!(site.requests().len(), 3);
}

// spec: OUT-250
#[tokio::test]
async fn scheduled_and_regular_cli_runs_never_contact_update_proxy() {
    let site = os_testsite::mocks::update::https_fixture().await.unwrap();
    let proxy = os_testsite::mocks::update::FixtureProxy::start(site.addr())
        .await
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let mut settings = os_model::settings::AppSettings::default();
    settings.network.proxy.mode = ProxyMode::Manual;
    settings.network.proxy.host = proxy.addr.ip().to_string();
    settings.network.proxy.port = proxy.addr.port();
    settings.network.proxy.bypass = vec!["127.0.0.1".into()];
    settings.save(dir.path()).unwrap();
    let crawl = os_testsite::TestSite::start("out-basic-site")
        .await
        .unwrap();
    let task: os_sched::tasks::Task = serde_json::from_value(serde_json::json!({
        "id":"update-fixture-task", "name":"Update fixture task",
        "schedule":{"kind":"daily","time":"03:00"},
        "crawl":{"source":{"kind":"spider","url":crawl.base_url()},
                 "overrides":{"threads":"1", "robots.retries":"0"}}
    }))
    .unwrap();
    os_sched::tasks::TaskStore::in_settings(dir.path())
        .save(task)
        .unwrap();
    for args in [vec!["--run-task", "update-fixture-task"], vec!["--help"]] {
        let home = dir.path().to_owned();
        let output = tokio::task::spawn_blocking(move || {
            std::process::Command::new(env!("CARGO_BIN_EXE_openspider"))
                .args(args)
                .env("OPENSPIDER_HOME", home)
                .env("OPENSPIDER_SECRET_STORE", "memory")
                .output()
                .unwrap()
        })
        .await
        .unwrap();
        assert_eq!(
            output.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    assert!(!crawl.requests().is_empty());
    assert!(proxy.requests().is_empty());
    assert!(site.requests().is_empty());
}

// spec: OUT-250
#[tokio::test]
async fn environment_disable_is_honored_by_real_cli_before_network() {
    let site = os_testsite::mocks::update::https_fixture().await.unwrap();
    let proxy = os_testsite::mocks::update::FixtureProxy::start(site.addr())
        .await
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let mut settings = os_model::settings::AppSettings::default();
    settings.network.proxy.mode = ProxyMode::Manual;
    settings.network.proxy.host = proxy.addr.ip().to_string();
    settings.network.proxy.port = proxy.addr.port();
    settings.network.proxy.bypass.clear();
    settings.save(dir.path()).unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_openspider"))
        .arg("--check-update")
        .env("OPENSPIDER_HOME", dir.path())
        .env("OPENSPIDER_DISABLE_UPDATE_CHECK", "1")
        .env("OPENSPIDER_SECRET_STORE", "memory")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("Environment"));
    assert!(proxy.requests().is_empty());
    assert!(site.requests().is_empty());
}
