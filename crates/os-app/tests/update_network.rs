#![allow(clippy::disallowed_types, clippy::disallowed_methods)]
use os_app::update::{
    policy::{self, Outcome},
    UpdateContext,
};
use os_model::{settings::NetworkSettings, ProxyMode};
use os_testsite::mocks::update::{https_fixture, FixtureProxy};
use std::sync::Mutex;

async fn fixture() -> (os_testsite::TestSite, tempfile::TempDir, UpdateContext) {
    let site = https_fixture().await.unwrap();
    let dir = tempfile::tempdir().unwrap();
    let ca = dir.path().join("ca.pem");
    std::fs::write(&ca, site.ca_cert_pem()).unwrap();
    let mut network = NetworkSettings::default();
    network.proxy.mode = ProxyMode::None;
    network
        .trusted_certificates
        .push(ca.to_string_lossy().into_owned());
    let ctx = UpdateContext {
        endpoint: format!("{}/latest.json", site.https_base_url()),
        network,
        pubkey: include_str!("../../../tests/fixtures/update/test.key.pub")
            .trim()
            .into(),
        current: "1.0.0".parse().unwrap(),
        os: "darwin".into(),
        arch: "aarch64".into(),
        installer: os_app::update::policy::Installer::Platform,
        now: time::OffsetDateTime::parse(
            "2026-10-30T12:00:00Z",
            &time::format_description::well_known::Rfc3339,
        )
        .unwrap(),
        disabled_by_environment: false,
        ..Default::default()
    };
    (site, dir, ctx)
}

// spec: OUT-250
#[tokio::test]
async fn privacy_throttle_manual_and_daily_boundary() {
    let (site, _dir, mut ctx) = fixture().await;
    assert!(matches!(
        policy::check(&ctx, false).await.unwrap(),
        Outcome::Available(_)
    ));
    assert!(matches!(
        policy::check(&ctx, false).await.unwrap(),
        Outcome::Throttled
    ));
    assert_eq!(site.requests().len(), 1);
    assert!(matches!(
        policy::check(&ctx, true).await.unwrap(),
        Outcome::Available(_)
    ));
    ctx.now += time::Duration::hours(24);
    assert!(matches!(
        policy::check(&ctx, false).await.unwrap(),
        Outcome::Available(_)
    ));
    let log = site.requests();
    assert_eq!(log.len(), 3);
    for request in log {
        assert!(request.tls);
        assert_eq!(request.method, "GET");
        assert_eq!(
            request.path_and_query,
            "/latest.json?version=1.0.0&os=darwin&arch=aarch64"
        );
        assert_eq!(request.header("user-agent"), Some(policy::USER_AGENT));
        for (name, _) in request.headers {
            assert!(
                ["host", "user-agent", "accept"].contains(&name.to_ascii_lowercase().as_str()),
                "unexpected header: {name}"
            );
        }
    }
}

// spec: OUT-250
// spec: OUT-222 [M3]
#[tokio::test]
async fn settings_environment_missing_key_and_persisted_timestamp_produce_zero_requests() {
    let (site, _dir, mut ctx) = fixture().await;
    ctx.settings.check = false;
    for manual in [false, true] {
        assert!(matches!(
            policy::check(&ctx, manual).await.unwrap(),
            Outcome::Disabled(_)
        ));
    }
    ctx.settings.check = true;
    ctx.disabled_by_environment = true;
    for manual in [false, true] {
        assert!(matches!(
            policy::check(&ctx, manual).await.unwrap(),
            Outcome::Disabled(_)
        ));
    }
    ctx.disabled_by_environment = false;
    ctx.settings.last_check = Some("2026-10-30T11:59:59Z".into());
    assert!(matches!(
        policy::check(&ctx, false).await.unwrap(),
        Outcome::Throttled
    ));
    ctx.settings.last_check = None;
    ctx.pubkey.clear();
    assert!(matches!(
        policy::check(&ctx, true).await.unwrap(),
        Outcome::Disabled(_)
    ));
    assert!(site.requests().is_empty());
}

// spec: CRAWL-122 [M3]
// spec: OUT-269 [M3]
#[tokio::test]
async fn app_wide_proxy_observes_https_connect_and_target_request() {
    let (site, _dir, mut ctx) = fixture().await;
    let proxy = FixtureProxy::start(site.addr()).await.unwrap();
    ctx.network.proxy.mode = ProxyMode::Manual;
    ctx.network.proxy.host = proxy.addr.ip().to_string();
    ctx.network.proxy.port = proxy.addr.port();
    ctx.network.proxy.bypass.clear();
    assert!(matches!(
        policy::check(&ctx, true).await.unwrap(),
        Outcome::Available(_)
    ));
    let log = proxy.requests();
    assert_eq!(log.len(), 1);
    assert!(log[0].starts_with(&format!("CONNECT {} HTTP/1.1", site.addr())));
    assert_eq!(site.requests().len(), 1);
    assert_eq!(
        site.requests()[0].header("user-agent"),
        Some(policy::USER_AGENT)
    );
}

// spec: OUT-252
#[tokio::test]
async fn http_redirect_and_artifact_urls_are_rejected_beta_is_explicit() {
    let (site, _dir, mut ctx) = fixture().await;
    ctx.endpoint = format!("{}/latest.json", site.base_url());
    assert!(policy::check(&ctx, true).await.is_err());
    assert!(site.requests().is_empty());
    for path in ["http.json", "redirect.json", "beta.json"] {
        ctx.endpoint = format!("{}/{path}", site.https_base_url());
        assert!(policy::check(&ctx, true).await.is_err(), "{path}");
    }
    assert_eq!(site.requests().len(), 3); // no insecure redirect request
    ctx.beta = true;
    ctx.endpoint = format!("{}/beta.json", site.https_base_url());
    assert!(matches!(
        policy::check(&ctx, true).await.unwrap(),
        Outcome::Available(_)
    ));
    ctx.endpoint = format!("{}/older.json", site.https_base_url());
    assert!(matches!(
        policy::check(&ctx, true).await.unwrap(),
        Outcome::Current
    ));
}

// spec: OUT-250
#[tokio::test]
async fn five_second_timeout_and_offline_attempts_are_throttled() {
    let (site, _dir, mut ctx) = fixture().await;
    ctx.endpoint = format!("{}/slow.json", site.https_base_url());
    let before = tokio::time::Instant::now();
    assert!(policy::check(&ctx, false).await.is_err());
    assert!(before.elapsed() >= std::time::Duration::from_secs(4));
    assert!(before.elapsed() < std::time::Duration::from_secs(7));
    assert!(matches!(
        policy::check(&ctx, false).await.unwrap(),
        Outcome::Throttled
    ));
    assert_eq!(site.requests().len(), 1);
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);
    ctx.endpoint = format!("https://{addr}/latest.json");
    ctx.last_attempt = Mutex::new(None);
    assert!(policy::check(&ctx, false).await.is_err());
    assert!(matches!(
        policy::check(&ctx, false).await.unwrap(),
        Outcome::Throttled
    ));
}

// spec: OUT-250
// spec: OUT-222 [M3]
#[tokio::test]
async fn app_persists_attempt_before_fetch_and_restart_obeys_throttle() {
    use os_app::{commands::update::run_check_with_context, update::UpdateState, App, AppConfig};
    let (site, dir, ctx) = fixture().await;
    let app = App::new(AppConfig::new(dir.path())).unwrap();
    assert!(matches!(
        run_check_with_context(&app, false, ctx).await,
        UpdateState::Available(_)
    ));
    assert!(app.settings_snapshot().updates.last_check.is_some());
    let persisted = std::fs::read_to_string(app.settings().path()).unwrap();
    assert!(persisted.contains("last_check"));
    let restarted = App::new(AppConfig::new(dir.path())).unwrap();
    let (_, _other_dir, mut ctx) = fixture().await;
    ctx.endpoint = format!("{}/latest.json", site.https_base_url());
    assert!(matches!(
        run_check_with_context(&restarted, false, ctx).await,
        UpdateState::Idle
    ));
    assert_eq!(site.requests().len(), 1);
}

// spec: OUT-250
#[tokio::test]
async fn app_startup_network_errors_only_log_manual_errors_have_state() {
    use os_app::{commands::update::run_check_with_context, update::UpdateState, App, AppConfig};
    let (_site, dir, mut ctx) = fixture().await;
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);
    ctx.endpoint = format!("https://{addr}/latest.json");
    let app = App::new(AppConfig::new(dir.path())).unwrap();
    assert!(matches!(
        run_check_with_context(&app, false, ctx).await,
        UpdateState::Idle
    ));
    let (_, _dir, mut ctx) = fixture().await;
    ctx.endpoint = format!("https://{addr}/latest.json");
    assert!(matches!(
        run_check_with_context(&app, true, ctx).await,
        UpdateState::Failed(_)
    ));
}

// spec: OUT-262
// spec: OUT-250
#[tokio::test]
async fn windows_check_follows_the_installer_and_unknown_sends_no_request() {
    use os_app::update::policy::{Disabled, Installer};
    let (site, _dir, mut ctx) = fixture().await;
    ctx.os = "windows".into();
    ctx.arch = "x86_64".into();
    for installer in [Installer::Msi, Installer::Nsis] {
        ctx.installer = installer;
        let Outcome::Available(manifest) = policy::check(&ctx, true).await.unwrap() else {
            panic!("{installer:?}: Update erwartet");
        };
        assert_eq!(
            ctx.select(&manifest).map(|(key, _)| key.to_owned()),
            ctx.target()
        );
    }
    let log = site.requests();
    assert_eq!(log.len(), 2);
    for request in log {
        // Die Anfrage verrät die Installationsart nicht (nur Version, System, Architektur; OUT-250).
        assert_eq!(
            request.path_and_query,
            "/latest.json?version=1.0.0&os=windows&arch=x86_64"
        );
    }
    // Unbekannte Installationsart: kein Request, eigener Abschaltgrund.
    ctx.installer = Installer::Unknown;
    assert!(matches!(
        policy::check(&ctx, true).await.unwrap(),
        Outcome::Disabled(Disabled::UnknownInstaller)
    ));
    assert_eq!(site.requests().len(), 2);
    // Das Manifest der Fixture hat für Windows arm64 nur NSIS: eine MSI-Installation bekommt kein NSIS-Paket.
    ctx.arch = "aarch64".into();
    ctx.installer = Installer::Msi;
    assert!(policy::check(&ctx, true).await.is_err());
    ctx.installer = Installer::Nsis;
    assert!(matches!(
        policy::check(&ctx, true).await.unwrap(),
        Outcome::Available(_)
    ));
}

// spec: OUT-262
#[tokio::test]
async fn app_reports_unknown_installer_as_disable_reason_without_request() {
    use os_app::{
        commands::update::run_check_with_context,
        update::{policy::Installer, DisableReason, UpdateState},
        App, AppConfig,
    };
    let (site, dir, mut ctx) = fixture().await;
    ctx.os = "windows".into();
    ctx.arch = "x86_64".into();
    ctx.installer = Installer::Unknown;
    let app = App::new(AppConfig::new(dir.path())).unwrap();
    assert_eq!(
        run_check_with_context(&app, true, ctx).await,
        UpdateState::Disabled(DisableReason::UnknownInstaller)
    );
    assert!(site.requests().is_empty());
    assert!(app.settings_snapshot().updates.last_check.is_none());
}

// spec: OUT-251
// spec: OUT-250
#[tokio::test]
async fn skipped_version_is_silent_for_automatic_checks_but_shown_by_a_manual_check() {
    use os_app::{commands::update::run_check_with_context, update::UpdateState, App, AppConfig};
    let (site, dir, ctx) = fixture().await;
    let app = App::new(AppConfig::new(dir.path())).unwrap();
    // Later contexts talk to this site, so they trust its CA (each fixture has its own).
    let ca = dir.path().join("ca.pem").to_string_lossy().into_owned();
    let trust_first_site =
        |ctx: &mut UpdateContext| ctx.network.trusted_certificates = vec![ca.clone()];
    let UpdateState::Available(offered) = run_check_with_context(&app, true, ctx).await else {
        panic!("fixture offers an update");
    };
    // "Skip this version" (Nutzerentscheidung 09.11.2026) stores the version in the app settings.
    let other = tempfile::tempdir().unwrap();
    let skipping = App::new(AppConfig::new(other.path())).unwrap();
    skipping
        .settings()
        .apply_patch(&serde_json::json!({"updates":{"skippedVersion": offered.version}}))
        .unwrap();
    let persisted = std::fs::read_to_string(skipping.settings().path()).unwrap();
    assert!(persisted.contains("skipped_version"), "{persisted}");
    let (_, _automatic_dir, mut automatic) = fixture().await;
    automatic.endpoint = format!("{}/latest.json", site.https_base_url());
    trust_first_site(&mut automatic);
    assert_eq!(
        run_check_with_context(&skipping, false, automatic).await,
        UpdateState::UpToDate
    );
    assert!(skipping
        .ext::<os_app::commands::update::UpdateService>()
        .manifest
        .lock()
        .unwrap()
        .is_none());
    // A manual check (Hilfe → „Nach Updates suchen“) still reports it.
    let (_, _manual_dir, mut manual) = fixture().await;
    manual.endpoint = format!("{}/latest.json", site.https_base_url());
    trust_first_site(&mut manual);
    assert!(matches!(
        run_check_with_context(&skipping, true, manual).await,
        UpdateState::Available(found) if found.version == offered.version
    ));
    // A newer version than the skipped one is reported by automatic checks again.
    let newer = tempfile::tempdir().unwrap();
    let later = App::new(AppConfig::new(newer.path())).unwrap();
    later
        .settings()
        .apply_patch(&serde_json::json!({"updates":{"skippedVersion": "0.9.0"}}))
        .unwrap();
    let (_, _later_dir, mut automatic) = fixture().await;
    automatic.endpoint = format!("{}/latest.json", site.https_base_url());
    trust_first_site(&mut automatic);
    assert!(matches!(
        run_check_with_context(&later, false, automatic).await,
        UpdateState::Available(_)
    ));
}

// spec: OUT-251
#[tokio::test]
async fn skipping_follows_semver_order_including_pre_releases() {
    use os_app::{commands::update::run_check_with_context, update::UpdateState, App, AppConfig};
    // The fixture offers one version; the skipped value decides by SemVer order whether automatic checks stay silent.
    let (site, dir, ctx) = fixture().await;
    let probe = App::new(AppConfig::new(dir.path())).unwrap();
    let UpdateState::Available(offered) = run_check_with_context(&probe, true, ctx).await else {
        panic!("fixture offers an update");
    };
    let offered: semver::Version = offered.version.parse().unwrap();
    assert!(
        "1.0.0-beta.4".parse::<semver::Version>().unwrap()
            < "1.0.0-rc.1".parse::<semver::Version>().unwrap()
            && "1.0.0-rc.1".parse::<semver::Version>().unwrap()
                < "1.0.0".parse::<semver::Version>().unwrap()
    );
    let pre = |tag: &str| {
        let mut v = offered.clone();
        v.pre = semver::Prerelease::new(tag).unwrap();
        v.to_string()
    };
    let next_patch = semver::Version::new(offered.major, offered.minor, offered.patch + 1);
    let ca = dir.path().join("ca.pem").to_string_lossy().into_owned();
    for (skipped, silent) in [
        // Skipped a pre-release of the offered version: the final release is newer → reported.
        (pre("beta.4"), false),
        (pre("rc.1"), false),
        // Skipped the same or a newer version (also a newer pre-release): silent.
        (offered.to_string(), true),
        (format!("{next_patch}-beta.1"), true),
        (next_patch.to_string(), true),
        // Unreadable stored value: skips nothing.
        ("not-a-version".to_string(), false),
    ] {
        let home = tempfile::tempdir().unwrap();
        let app = App::new(AppConfig::new(home.path())).unwrap();
        app.settings()
            .apply_patch(&serde_json::json!({"updates":{"skippedVersion": skipped}}))
            .unwrap();
        let (_, _other, mut automatic) = fixture().await;
        automatic.endpoint = format!("{}/latest.json", site.https_base_url());
        automatic.network.trusted_certificates = vec![ca.clone()];
        let state = run_check_with_context(&app, false, automatic).await;
        assert_eq!(
            state == UpdateState::UpToDate,
            silent,
            "skipped {skipped}, offered {offered}: {state:?}"
        );
        assert_eq!(
            matches!(state, UpdateState::Available(_)),
            !silent,
            "skipped {skipped}"
        );
    }
}

// spec: OUT-251
// spec: OUT-250
#[tokio::test]
async fn switching_automatic_checks_off_keeps_a_visible_release_downloadable() {
    use os_app::{
        commands::update::{run_check_with_context, UpdateService},
        runtime::CallCtx,
        update::{DisableReason, UpdateState},
        App, AppConfig,
    };
    let (_site, dir, ctx) = fixture().await;
    let app = App::new(AppConfig::new(dir.path())).unwrap();
    assert!(matches!(
        run_check_with_context(&app, true, ctx).await,
        UpdateState::Available(_)
    ));
    // The background download failed; the dialog shows the release with "Download in background".
    let service = app.ext::<UpdateService>();
    *service.state.lock().unwrap() = UpdateState::Failed("download".into());
    app.dispatch(
        CallCtx::for_window("main"),
        "update.set_auto",
        serde_json::json!({"check": false}),
    )
    .await
    .unwrap();
    assert!(!app.settings_snapshot().updates.check);
    assert_eq!(
        *service.state.lock().unwrap(),
        UpdateState::Failed("download".into())
    );
    assert!(service.manifest.lock().unwrap().is_some(), "manifest kept");
    // Without a known release the setting still disables checks at once.
    let other = tempfile::tempdir().unwrap();
    let idle = App::new(AppConfig::new(other.path())).unwrap();
    idle.dispatch(
        CallCtx::for_window("main"),
        "update.set_auto",
        serde_json::json!({"check": false}),
    )
    .await
    .unwrap();
    assert_eq!(
        *idle.ext::<UpdateService>().state.lock().unwrap(),
        UpdateState::Disabled(DisableReason::Settings)
    );
}
