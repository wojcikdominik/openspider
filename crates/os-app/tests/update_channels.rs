//! OUT-254 Update-Kanäle im Desktop-Kern: Standardregel, Speicherung beim Start, Wechsel, feste URLs und die
//! Testmatrix gegen den Mock-Update-Server (`os_testsite::mocks::update::ChannelFixture`, je Kanal ein Pfad).
#![allow(clippy::disallowed_types, clippy::disallowed_methods)]
use os_app::{
    commands::update::{
        channel_state, persist_startup_channel, run_check_with_context, UpdateService,
    },
    dto::update::{UpdateChannel, UpdateChannelState},
    runtime::CallCtx,
    update::{
        policy::{self, Channel, Installer, BETA_ENDPOINT, STABLE_ENDPOINT},
        UpdateContext, UpdateState,
    },
    App, AppConfig,
};
use os_model::{
    settings::{AppSettings, UpdateSettings},
    ProxyMode,
};
use os_testsite::mocks::update::ChannelFixture;
use serde_json::{json, Value};
use std::path::Path;

const MATRIX: &str = include_str!("../../../tests/fixtures/update/channel-matrix.json");

fn rows() -> Vec<Value> {
    serde_json::from_str::<Value>(MATRIX).unwrap()["rows"]
        .as_array()
        .unwrap()
        .clone()
}
fn text(row: &Value, key: &str) -> Option<String> {
    row[key].as_str().map(str::to_owned)
}
fn version(v: &str) -> semver::Version {
    v.parse().unwrap()
}
/// Profilverzeichnis: ohne Schlüssel, mit `stable`, mit `beta` oder mit ungültigem Wert.
fn profile(dir: &Path, channel: Option<&str>) {
    let body = match channel {
        Some(c) => format!("[updates]\ncheck = true\nchannel = \"{c}\"\n"),
        None => "[updates]\ncheck = true\n".to_string(),
    };
    std::fs::write(AppSettings::path_in(dir), body).unwrap();
}
fn settings_text(dir: &Path) -> String {
    std::fs::read_to_string(AppSettings::path_in(dir)).unwrap()
}
/// Context against the mock: only the two manifest URLs (base = mock server) and the injected running version differ.
fn context(fixture: &ChannelFixture, dir: &Path, installed: &str) -> UpdateContext {
    let ca = dir.join("ca.pem");
    std::fs::write(&ca, fixture.site.ca_cert_pem()).unwrap();
    let mut ctx = UpdateContext {
        endpoint: fixture.stable_url(),
        beta_endpoint: fixture.beta_url(),
        pubkey: include_str!("../../../tests/fixtures/update/test.key.pub")
            .trim()
            .into(),
        current: version(installed),
        os: "darwin".into(),
        arch: "aarch64".into(),
        installer: Installer::Platform,
        disabled_by_environment: false,
        ..Default::default()
    };
    ctx.network.proxy.mode = ProxyMode::None;
    ctx.network
        .trusted_certificates
        .push(ca.to_string_lossy().into_owned());
    ctx
}
#[derive(Clone, Default)]
struct Log(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);
impl std::io::Write for Log {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(data);
        Ok(data.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
impl Log {
    fn text(&self) -> String {
        String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
    }
}

// spec: OUT-254
// spec: OUT-250
// spec: OUT-252
#[tokio::test]
async fn desktop_core_matrix_rows_1_to_13() {
    let rows = rows();
    assert_eq!(rows.len(), 13);
    for row in rows {
        let nr = row["nr"].as_u64().unwrap();
        let channel = text(&row, "channel");
        let installed = text(&row, "installed").unwrap();
        let fixture = ChannelFixture::start(
            text(&row, "stable").as_deref(),
            text(&row, "beta").as_deref(),
        )
        .await
        .unwrap();
        let dir = tempfile::tempdir().unwrap();
        profile(dir.path(), channel.as_deref());
        let log = Log::default();
        let writer = log.clone();
        let subscriber = tracing_subscriber::fmt()
            .with_writer(move || writer.clone())
            .with_ansi(false)
            .finish();
        let _guard = tracing::subscriber::set_default(subscriber);

        // Desktop start of the injected running version: the channel is stored before the first request goes out.
        let app = App::new(AppConfig::new(dir.path())).unwrap();
        persist_startup_channel(&app, &version(&installed));
        let stored = app.settings_snapshot().updates.channel;
        match text(&row, "desktopStores") {
            Some(value) => {
                assert_eq!(stored.as_deref(), Some(value.as_str()), "row {nr}");
                assert!(
                    settings_text(dir.path()).contains(&format!("channel = \"{value}\"")),
                    "row {nr}"
                );
            }
            None => assert_eq!(stored, channel, "row {nr}: nothing stored"),
        }
        assert_eq!(fixture.site.requests().len(), 0, "row {nr}: before check");

        let state =
            run_check_with_context(&app, true, context(&fixture, dir.path(), &installed)).await;
        // Requested = the only path with a request; the other path counts 0.
        let (stable, beta) = (fixture.stable_requests(), fixture.beta_requests());
        match row["requested"].as_str().unwrap() {
            "stable" => assert_eq!((stable, beta), (1, 0), "row {nr}"),
            _ => assert_eq!((stable, beta), (0, 1), "row {nr}"),
        }
        // OUT-250: only version, OS and architecture; no channel parameter, the URL alone carries the channel.
        let request = &fixture.site.requests()[0];
        assert!(
            request
                .path_and_query
                .ends_with(&format!("?version={installed}&os=darwin&arch=aarch64")),
            "row {nr}: {}",
            request.path_and_query
        );
        assert!(!request.path_and_query.contains("channel"), "row {nr}");
        let offered = app.ext::<UpdateService>().manifest.lock().unwrap().clone();
        match row["result"].as_str().unwrap() {
            "current" => {
                assert_eq!(state, UpdateState::UpToDate, "row {nr}");
                assert!(offered.is_none(), "row {nr}: no download");
            }
            "update" => {
                let UpdateState::Available(update) = state else {
                    panic!("row {nr}: {state:?}");
                };
                assert_eq!(Some(update.version), text(&row, "version"), "row {nr}");
                assert!(offered.is_some(), "row {nr}");
            }
            _ => {
                // OUT-252: a pre-release on the stable channel is an invalid manifest (manual check: message).
                assert!(
                    matches!(state, UpdateState::Failed(_)),
                    "row {nr}: {state:?}"
                );
                assert!(offered.is_none(), "row {nr}: no download");
            }
        }
        let file = settings_text(dir.path());
        match nr {
            1 => assert!(!file.contains("channel"), "row 1 writes no key: {file}"),
            13 => {
                assert!(file.contains("channel = \"nightly\""), "{file}");
                let logged = log.text();
                assert!(logged.contains("WARN"), "{logged}");
                assert!(logged.contains("updates.channel"), "{logged}");
            }
            _ => {}
        }
        // Row 10: no downgrade; the UI shows the stay-on hint (UI-131) because a pre-release runs on `stable`.
        if nr == 10 {
            assert_eq!(
                channel_state(&app.settings_snapshot().updates, &version(&installed)),
                UpdateChannelState {
                    channel: UpdateChannel::Stable,
                    version: "1.1.0-beta.3".into(),
                    prerelease: true,
                }
            );
        }
    }
}

// spec: OUT-254
// spec: OUT-252
#[tokio::test]
async fn row_12_rejection_at_startup_only_logs() {
    let fixture = ChannelFixture::start(Some("1.1.0-beta.1"), None)
        .await
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    profile(dir.path(), Some("stable"));
    let app = App::new(AppConfig::new(dir.path())).unwrap();
    let state = run_check_with_context(&app, false, context(&fixture, dir.path(), "1.0.1")).await;
    assert_eq!(state, UpdateState::Idle);
    assert!(app
        .ext::<UpdateService>()
        .manifest
        .lock()
        .unwrap()
        .is_none());
    assert_eq!((fixture.stable_requests(), fixture.beta_requests()), (1, 0));
}

// spec: OUT-254
// spec: OUT-222
#[tokio::test]
async fn persistence_beta_stays_after_updating_to_a_stable_version() {
    let dir = tempfile::tempdir().unwrap();
    profile(dir.path(), None);
    let app = App::new(AppConfig::new(dir.path())).unwrap();
    persist_startup_channel(&app, &version("1.1.0-beta.1"));
    let file = settings_text(dir.path());
    let section = file.split("[updates]").nth(1).expect("[updates]");
    assert!(
        section
            .split("\n[")
            .next()
            .unwrap()
            .contains("channel = \"beta\""),
        "{file}"
    );
    drop(app);
    // Same profile, now 1.1.0 (stable): the stored value stays `beta`, the check asks the beta path.
    let fixture = ChannelFixture::start(Some("1.1.0"), Some("1.2.0-beta.1"))
        .await
        .unwrap();
    let app = App::new(AppConfig::new(dir.path())).unwrap();
    persist_startup_channel(&app, &version("1.1.0"));
    assert_eq!(
        app.settings_snapshot().updates.channel.as_deref(),
        Some("beta")
    );
    let state = run_check_with_context(&app, true, context(&fixture, dir.path(), "1.1.0")).await;
    assert!(matches!(state, UpdateState::Available(ref u) if u.version == "1.2.0-beta.1"));
    assert_eq!((fixture.stable_requests(), fixture.beta_requests()), (0, 1));
}

// spec: OUT-254
#[tokio::test]
async fn stable_version_without_key_never_writes_a_channel() {
    let fixture = ChannelFixture::start(Some("1.0.1"), Some("1.1.0-beta.1"))
        .await
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    profile(dir.path(), None);
    for _ in 0..2 {
        let app = App::new(AppConfig::new(dir.path())).unwrap();
        persist_startup_channel(&app, &version("1.0.1"));
        run_check_with_context(&app, true, context(&fixture, dir.path(), "1.0.1")).await;
        assert!(!settings_text(dir.path()).contains("channel"));
        assert_eq!(app.settings_snapshot().updates.channel, None);
    }
    assert_eq!((fixture.stable_requests(), fixture.beta_requests()), (2, 0));
}

// spec: OUT-254
#[test]
fn stored_stable_on_a_prerelease_survives_restarts() {
    let dir = tempfile::tempdir().unwrap();
    profile(dir.path(), Some("stable"));
    let before = settings_text(dir.path());
    for _ in 0..3 {
        let app = App::new(AppConfig::new(dir.path())).unwrap();
        persist_startup_channel(&app, &version("1.1.0-beta.3"));
        assert_eq!(
            app.settings_snapshot().updates.channel.as_deref(),
            Some("stable")
        );
        assert_eq!(
            channel_state(&app.settings_snapshot().updates, &version("1.1.0-beta.3")).channel,
            UpdateChannel::Stable
        );
    }
    assert_eq!(settings_text(dir.path()), before);
}

// spec: OUT-254
// spec: UI-131
// spec: OUT-222
#[tokio::test]
async fn switching_stable_to_beta_sends_nothing_and_the_next_manual_check_asks_beta_once() {
    let fixture = ChannelFixture::start(Some("1.0.1"), Some("1.1.0-beta.1"))
        .await
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        AppSettings::path_in(dir.path()),
        "[updates]\ncheck = false\nskipped_version = \"1.0.5\"\nchannel = \"stable\"\n",
    )
    .unwrap();
    let app = App::new(AppConfig::new(dir.path())).unwrap();
    let state = app
        .dispatch(
            CallCtx::for_window("settings"),
            "update.set_channel",
            json!({"channel": "beta"}),
        )
        .await
        .unwrap();
    assert_eq!(state["channel"], "beta");
    assert_eq!(
        fixture.site.requests().len(),
        0,
        "switching sends no request"
    );
    let updates = app.settings_snapshot().updates;
    assert_eq!(updates.channel.as_deref(), Some("beta"));
    // OUT-254: a channel switch changes neither `skipped_version` nor `check`.
    assert_eq!(updates.skipped_version.as_deref(), Some("1.0.5"));
    assert!(!updates.check);
    let file = settings_text(dir.path());
    assert!(file.contains("channel = \"beta\""), "{file}");
    assert!(file.contains("skipped_version = \"1.0.5\""), "{file}");
    assert!(file.contains("check = false"), "{file}");
    // OUT-222: the key lives in settings.toml only, never in a crawl profile.
    let profile = os_model::CrawlConfig::default()
        .to_osconfig_string()
        .unwrap();
    assert!(!profile.contains("[updates]") && !profile.contains("channel ="));
    // Next manual check: exactly one request, to the beta path. `check = false` disables every check (OUT-250), so
    // the profile switches it back on first; the channel switch itself left it untouched (asserted above).
    app.settings()
        .apply_patch(&json!({"updates": {"check": true}}))
        .unwrap();
    let state = run_check_with_context(&app, true, context(&fixture, dir.path(), "1.0.1")).await;
    assert!(matches!(state, UpdateState::Available(ref u) if u.version == "1.1.0-beta.1"));
    assert_eq!((fixture.stable_requests(), fixture.beta_requests()), (0, 1));
    // Back to stable: stored at once, nothing else changes.
    let state = app
        .dispatch(
            CallCtx::for_window("settings"),
            "update.set_channel",
            json!({"channel": "stable"}),
        )
        .await
        .unwrap();
    assert_eq!(state["channel"], "stable");
    assert_eq!(
        app.settings_snapshot().updates.skipped_version.as_deref(),
        Some("1.0.5")
    );
    assert_eq!(fixture.site.requests().len(), 1);
}

// spec: UI-131
// spec: OUT-254
#[tokio::test]
async fn channel_command_reports_effective_channel_and_rejects_invalid_values() {
    let dir = tempfile::tempdir().unwrap();
    let app = App::new(AppConfig::new(dir.path())).unwrap();
    let state: UpdateChannelState = serde_json::from_value(
        app.dispatch(
            CallCtx::for_window("settings"),
            "update.channel",
            Value::Null,
        )
        .await
        .unwrap(),
    )
    .unwrap();
    let running = policy::running_version();
    assert_eq!(state, channel_state(&UpdateSettings::default(), &running));
    assert!(!state.version.contains('+'));
    let err = app
        .dispatch(
            CallCtx::for_window("settings"),
            "update.set_channel",
            json!({"channel": "nightly"}),
        )
        .await
        .unwrap_err();
    assert_eq!(err.code, os_app::ErrorCode::InvalidInput);
    assert!(
        !AppSettings::path_in(dir.path()).exists()
            || !settings_text(dir.path()).contains("channel")
    );
    // Pure rule with injected versions (UI-131 preselection).
    let none = UpdateSettings::default();
    for (installed, channel, prerelease) in [
        ("1.0.1", UpdateChannel::Stable, false),
        ("1.1.0-beta.1", UpdateChannel::Beta, true),
        ("1.1.0-rc.1+42", UpdateChannel::Beta, true),
    ] {
        let state = channel_state(&none, &version(installed));
        assert_eq!((state.channel, state.prerelease), (channel, prerelease));
        assert!(!state.version.contains('+'), "{}", state.version);
    }
}

// spec: OUT-254
#[test]
fn standard_rule_stored_value_and_invalid_values() {
    let with = |c: Option<&str>| UpdateSettings {
        channel: c.map(str::to_owned),
        ..Default::default()
    };
    let stable = version("1.0.1");
    let beta = version("1.1.0-beta.1");
    assert_eq!(
        policy::effective_channel(&with(None), &stable),
        Channel::Stable
    );
    assert_eq!(policy::effective_channel(&with(None), &beta), Channel::Beta);
    assert_eq!(
        policy::effective_channel(&with(Some("stable")), &beta),
        Channel::Stable
    );
    assert_eq!(
        policy::effective_channel(&with(Some("beta")), &stable),
        Channel::Beta
    );
    for invalid in ["nightly", "Beta", " stable", ""] {
        assert_eq!(policy::stored_channel(&with(Some(invalid))), Err(invalid));
        assert_eq!(
            policy::effective_channel(&with(Some(invalid)), &stable),
            Channel::Stable
        );
        assert_eq!(
            policy::effective_channel(&with(Some(invalid)), &beta),
            Channel::Beta
        );
    }
    assert_eq!(
        policy::channel_to_store_at_startup(&with(None), &beta),
        Some(Channel::Beta)
    );
    assert_eq!(
        policy::channel_to_store_at_startup(&with(Some("nightly")), &beta),
        Some(Channel::Beta)
    );
    assert_eq!(
        policy::channel_to_store_at_startup(&with(Some("stable")), &beta),
        None
    );
    assert_eq!(
        policy::channel_to_store_at_startup(&with(None), &stable),
        None
    );
    assert_eq!(
        policy::channel_to_store_at_startup(&with(Some("nightly")), &stable),
        None
    );
}

// spec: OUT-254
#[test]
fn built_in_urls_are_exactly_the_spec_urls_and_no_build_switch_selects_a_channel() {
    assert_eq!(
        STABLE_ENDPOINT,
        "https://github.com/wojcikdominik/openspider/releases/latest/download/latest.json"
    );
    assert_eq!(
        BETA_ENDPOINT,
        "https://github.com/wojcikdominik/openspider/releases/download/update-beta/latest-beta.json"
    );
    assert_eq!(Channel::Stable.manifest_url(), STABLE_ENDPOINT);
    assert_eq!(Channel::Beta.manifest_url(), BETA_ENDPOINT);
    let context = UpdateContext::default();
    assert_eq!(context.endpoint, STABLE_ENDPOINT);
    assert_eq!(context.beta_endpoint, BETA_ENDPOINT);
    // The channel comes from the setting at run time, never from the build.
    let source = include_str!("../src/update/policy.rs");
    assert!(!source.contains(&["option_env!(\"OPENSPIDER", "_UPDATE_BETA\")"].concat()));
    for (stored, current, url) in [
        (None, "1.0.1", STABLE_ENDPOINT),
        (None, "1.1.0-beta.1", BETA_ENDPOINT),
        (Some("beta"), "1.0.1", BETA_ENDPOINT),
        (Some("stable"), "1.1.0-beta.1", STABLE_ENDPOINT),
    ] {
        let ctx = UpdateContext {
            current: version(current),
            settings: UpdateSettings {
                channel: stored.map(str::to_owned),
                ..Default::default()
            },
            ..Default::default()
        };
        assert_eq!(ctx.channel_endpoint(), url, "{stored:?} {current}");
    }
}

// spec: OUT-254
// spec: OUT-253
#[tokio::test]
async fn environment_disable_stops_both_channels_before_any_request() {
    let fixture = ChannelFixture::start(Some("1.0.2"), Some("1.1.0-beta.1"))
        .await
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    for channel in ["stable", "beta"] {
        let mut ctx = context(&fixture, dir.path(), "1.0.1");
        ctx.settings.channel = Some(channel.into());
        ctx.disabled_by_environment = true;
        for manual in [false, true] {
            assert!(matches!(
                policy::check(&ctx, manual).await.unwrap(),
                policy::Outcome::Disabled(policy::Disabled::Environment)
            ));
        }
    }
    assert_eq!(fixture.site.requests().len(), 0);
}

// spec: OUT-222
#[test]
fn settings_without_channel_from_1_0_0_load_without_migration() {
    let loaded =
        AppSettings::from_toml_str(include_str!("../../../tests/fixtures/update/settings.toml"))
            .unwrap();
    assert_eq!(loaded.updates.channel, None);
    assert!(!loaded.to_toml_string().unwrap().contains("channel"));
}
