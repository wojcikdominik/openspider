#![allow(clippy::disallowed_types, clippy::disallowed_methods)]
//! Unabhängige Abnahme B4-UPDDLG (Testagent; Grundlage allein die Spec, Bauer-Bericht nicht gelesen).
//!
//! Spec: OUT-251 „Diese Version überspringen“ merkt sich die Version in den App-Einstellungen (`updates.skipped_version`,
//! OUT-222); automatische Prüfungen melden sie danach nicht mehr, erst eine neuere Version. Die manuelle Prüfung zeigt auch
//! die übersprungene Version. „Später erinnern“ schließt nur; das Update wird bei der nächsten regulären Prüfung (OUT-250,
//! höchstens einmal je 24 h beim Start) wieder gemeldet. UI-127: `buildNumber` und `versionDisplay` in Debug-Info und
//! Diagnosepaket, Format „<Version> (Build <n>)“, ohne Nummer nur die Version.
//!
//! Fixture: Mock-Update-Server (`os-testsite`), bietet 1.0.1 an; installiert ist 1.0.0.

use os_app::commands::update::run_check_with_context;
use os_app::update::{UpdateContext, UpdateState};
use os_app::{App, AppConfig};
use os_model::{settings::NetworkSettings, ProxyMode};
use os_testsite::mocks::update::https_fixture;
use serde_json::json;

const OFFERED: &str = "1.0.1";

fn context(site: &os_testsite::TestSite, ca: &std::path::Path) -> UpdateContext {
    let mut network = NetworkSettings::default();
    network.proxy.mode = ProxyMode::None;
    network
        .trusted_certificates
        .push(ca.to_string_lossy().into_owned());
    UpdateContext {
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
    }
}

/// Frische App (eigener Datenordner, damit die 24-Stunden-Drosselung eines früheren Laufs nicht stört), optional mit
/// übersprungener Version in den App-Einstellungen — wie der Knopf „Diese Version überspringen“ sie schreibt.
fn app_with_skipped(skipped: Option<&str>) -> (App, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let app = App::new(AppConfig::new(dir.path())).unwrap();
    if let Some(version) = skipped {
        app.settings()
            .apply_patch(&json!({"updates": {"skippedVersion": version}}))
            .unwrap();
    }
    (app, dir)
}

async fn check(skipped: Option<&str>, manual: bool) -> UpdateState {
    let site = https_fixture().await.unwrap();
    let ca_dir = tempfile::tempdir().unwrap();
    let ca = ca_dir.path().join("ca.pem");
    std::fs::write(&ca, site.ca_cert_pem()).unwrap();
    let (app, _dir) = app_with_skipped(skipped);
    run_check_with_context(&app, manual, context(&site, &ca)).await
}

fn is_available(state: &UpdateState) -> bool {
    matches!(state, UpdateState::Available(found) if found.version == OFFERED)
}

// spec: OUT-251
#[tokio::test]
async fn without_a_skipped_version_both_checks_report_the_update() {
    assert!(is_available(&check(None, false).await));
    assert!(is_available(&check(None, true).await));
}

// spec: OUT-251
#[tokio::test]
async fn an_automatic_check_no_longer_reports_the_skipped_version() {
    let state = check(Some(OFFERED), false).await;
    assert!(!matches!(state, UpdateState::Available(_)), "{state:?}");
}

// spec: OUT-251
#[tokio::test]
async fn the_manual_check_shows_the_skipped_version_anyway() {
    assert!(is_available(&check(Some(OFFERED), true).await));
}

// spec: OUT-251
#[tokio::test]
async fn a_newer_version_than_the_skipped_one_is_reported_by_automatic_checks_again() {
    for skipped in ["1.0.0", "0.9.0", "1.0.1-beta.9", "1.0.0-rc.1"] {
        let state = check(Some(skipped), false).await;
        assert!(is_available(&state), "übersprungen {skipped}: {state:?}");
    }
}

// spec: OUT-251
// Randfall der Formulierung „erst eine neuere Version“: eine Version, die nicht neuer als die übersprungene ist, bleibt
// still (hier: das Manifest bietet 1.0.1 an, übersprungen wurde 1.0.2).
#[tokio::test]
async fn a_version_older_than_the_skipped_one_stays_silent_for_automatic_checks() {
    let state = check(Some("1.0.2"), false).await;
    assert!(!matches!(state, UpdateState::Available(_)), "{state:?}");
}

// spec: OUT-251
// spec: OUT-222
#[test]
fn the_skipped_version_is_stored_in_the_app_settings_as_updates_skipped_version() {
    let (app, dir) = app_with_skipped(Some(OFFERED));
    assert_eq!(
        app.settings_snapshot().updates.skipped_version.as_deref(),
        Some(OFFERED)
    );
    let toml = std::fs::read_to_string(app.settings().path()).unwrap();
    assert!(toml.contains("[updates]"), "{toml}");
    assert!(
        toml.contains(&format!("skipped_version = \"{OFFERED}\"")),
        "{toml}"
    );
    // Neustart: die Einstellung bleibt erhalten.
    drop(app);
    let again = App::new(AppConfig::new(dir.path())).unwrap();
    assert_eq!(
        again.settings_snapshot().updates.skipped_version.as_deref(),
        Some(OFFERED)
    );
}

// spec: OUT-251
// spec: OUT-250
#[tokio::test]
async fn remind_me_later_changes_nothing_so_the_next_regular_check_reports_the_update_again() {
    let site = https_fixture().await.unwrap();
    let ca_dir = tempfile::tempdir().unwrap();
    let ca = ca_dir.path().join("ca.pem");
    std::fs::write(&ca, site.ca_cert_pem()).unwrap();
    let (app, _dir) = app_with_skipped(None);
    let first = run_check_with_context(&app, false, context(&site, &ca)).await;
    assert!(is_available(&first), "{first:?}");
    // „Später erinnern“ schreibt nichts: keine übersprungene Version.
    assert_eq!(app.settings_snapshot().updates.skipped_version, None);
}

// spec: UI-127
// spec: OUT-181
#[test]
fn app_info_and_the_diagnostic_system_info_carry_build_number_and_display_version() {
    let (app, _dir) = app_with_skipped(None);
    let info = os_app::runtime::info::app_info(app.config(), &app.settings_snapshot());
    let system = os_app::diag::system_info();
    let cargo =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../Cargo.toml")).unwrap();
    let workspace = cargo
        .split("[workspace.package]")
        .nth(1)
        .and_then(|s| {
            s.lines()
                .find_map(|l| l.trim().strip_prefix("version = \""))
        })
        .and_then(|r| r.strip_suffix('"'))
        .unwrap();
    assert_eq!(info.version, workspace);
    assert_eq!(system["version"], workspace);
    let given = std::env::var("OPENSPIDER_BUILD_NUMBER")
        .ok()
        .map(|v| v.trim().to_owned())
        .filter(|v| !v.is_empty());
    match given {
        Some(number) => {
            assert_eq!(info.build_number.as_deref(), Some(number.as_str()));
            assert_eq!(system["buildNumber"], number);
            assert_eq!(
                system["versionDisplay"],
                format!("{workspace} (Build {number})")
            );
        }
        None if cfg!(debug_assertions) => {
            assert_eq!(info.build_number, None);
            assert!(system["buildNumber"].is_null(), "{system}");
            assert_eq!(system["versionDisplay"], workspace);
        }
        None => {}
    }
}
