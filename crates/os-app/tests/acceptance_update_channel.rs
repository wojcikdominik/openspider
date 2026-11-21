#![allow(clippy::disallowed_types, clippy::disallowed_methods)]
//! Unabhängige Abnahme „Update-Kanal-Einstellung“ (Testagent; Grundlage allein die Spec, Bauer-Bericht nicht gelesen).
//! acceptance: independent
//!
//! Spec: OUT-254 (Testmatrix, Standardregel, Persistenz, Wechsel ohne Request, CLI schreibt nie, ungültige Werte,
//! Abschaltung, feste URLs), OUT-250 (Anfrage nur Version/OS/Architektur), OUT-252 (stabiler Kanal lehnt Vorabversion
//! ab, kein Downgrade), OUT-222 (Schlüssel `updates.channel` in `settings.toml`, ohne geschriebenen Standardwert).
//!
//! Fixture: `os_testsite::mocks::update::ChannelFixture` (je Kanal ein Manifest-Pfad, Request-Log je Pfad) und die
//! Profile unter `tests/fixtures/update/acceptance-channel/`. Die laufende Version wird injiziert.

use os_app::commands::update::{
    channel_state, persist_startup_channel, run_check_with_context, set_channel_value,
};
use os_app::dto::update::UpdateChannel;
use os_app::update::policy::{
    channel_to_store_at_startup, default_channel, effective_channel, stored_channel, Channel,
    Installer, BETA_ENDPOINT, STABLE_ENDPOINT,
};
use os_app::update::{UpdateContext, UpdateState};
use os_app::{App, AppConfig};
use os_model::settings::{AppSettings, NetworkSettings, UpdateSettings};
use os_model::ProxyMode;
use os_testsite::mocks::update::ChannelFixture;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

const PUBKEY: &str = include_str!("../../../tests/fixtures/update/test.key.pub");
const STABLE_URL: &str =
    "https://github.com/wojcikdominik/openspider/releases/latest/download/latest.json";
const BETA_URL: &str =
    "https://github.com/wojcikdominik/openspider/releases/download/update-beta/latest-beta.json";

fn profile_source(channel: Option<&str>) -> PathBuf {
    let name = channel.unwrap_or("missing");
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/update/acceptance-channel")
        .join(format!("{name}.toml"))
}

/// Einstellungsordner (= `OPENSPIDER_HOME`) mit dem Profil der Fixture als `settings.toml`.
fn profile(channel: Option<&str>) -> tempfile::TempDir {
    let home = tempfile::tempdir().unwrap();
    std::fs::copy(profile_source(channel), AppSettings::path_in(home.path())).unwrap();
    home
}

fn settings_file(home: &Path) -> String {
    std::fs::read_to_string(AppSettings::path_in(home)).unwrap_or_default()
}

/// Rohwert von `channel` im Abschnitt `[updates]` der Datei (Textprüfung, unabhängig vom Modell).
fn updates_channel_line(toml: &str) -> Option<String> {
    let mut in_updates = false;
    for line in toml.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_updates = line == "[updates]";
            continue;
        }
        if in_updates {
            if let Some((key, value)) = line.split_once('=') {
                if key.trim() == "channel" {
                    return Some(value.trim().trim_matches('"').to_string());
                }
            }
        }
    }
    None
}

fn any_channel_key(toml: &str) -> bool {
    toml.lines()
        .any(|l| l.trim_start().starts_with("channel") && l.contains('='))
}

struct Mock {
    fx: ChannelFixture,
    _ca_dir: tempfile::TempDir,
    ca: PathBuf,
}

async fn mock(stable: Option<&str>, beta: Option<&str>) -> Mock {
    let fx = ChannelFixture::start(stable, beta).await.unwrap();
    let ca_dir = tempfile::tempdir().unwrap();
    let ca = ca_dir.path().join("ca.pem");
    std::fs::write(&ca, fx.site.ca_cert_pem()).unwrap();
    Mock {
        fx,
        _ca_dir: ca_dir,
        ca,
    }
}

fn network(ca: &Path) -> NetworkSettings {
    let mut network = NetworkSettings::default();
    network.proxy.mode = ProxyMode::None;
    network
        .trusted_certificates
        .push(ca.to_string_lossy().into_owned());
    network
}

fn now() -> time::OffsetDateTime {
    time::OffsetDateTime::parse(
        "2026-11-21T12:00:00Z",
        &time::format_description::well_known::Rfc3339,
    )
    .unwrap()
}

/// Desktop-Kontext gegen den Mock: nur die Basis der beiden festen URLs ist ersetzt.
fn desktop_ctx(m: &Mock, installed: &str) -> UpdateContext {
    UpdateContext {
        endpoint: m.fx.stable_url(),
        beta_endpoint: m.fx.beta_url(),
        network: network(&m.ca),
        pubkey: PUBKEY.trim().into(),
        current: installed.parse().unwrap(),
        os: "darwin".into(),
        arch: "aarch64".into(),
        installer: Installer::Platform,
        now: now(),
        disabled_by_environment: false,
        ..Default::default()
    }
}

/// CLI-Kontext aus `settings.toml` (wie `--check-update`), Basis der URLs auf den Mock umgebogen.
fn cli_ctx(m: &Mock, home: &Path, installed: &str) -> os_cli::update::policy::UpdateContext {
    let mut ctx = os_cli::update::context_from_settings(home).unwrap();
    ctx.endpoint = m.fx.stable_url();
    ctx.beta_endpoint = m.fx.beta_url();
    ctx.network = network(&m.ca);
    ctx.pubkey = PUBKEY.trim().into();
    ctx.current = installed.parse().unwrap();
    ctx.os = "darwin".into();
    ctx.arch = "aarch64".into();
    ctx.now = now();
    ctx.disabled_by_environment = false;
    ctx
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Res {
    Current,
    Update(String),
    Rejected,
}

#[derive(Clone, Copy)]
struct Row {
    nr: u32,
    channel: Option<&'static str>,
    installed: &'static str,
    stable: Option<&'static str>,
    beta: Option<&'static str>,
    requested: Channel,
    result: Option<&'static str>, // None = aktuell, Some("rejected"), sonst Update-Version
    /// Erwarteter Rohwert von `updates.channel` in der Datei nach Desktop-Start und Prüfung (None = kein Schlüssel).
    desktop_file: Option<&'static str>,
}

impl Row {
    fn expected(&self) -> Res {
        match self.result {
            None => Res::Current,
            Some("rejected") => Res::Rejected,
            Some(v) => Res::Update(v.into()),
        }
    }
}

/// OUT-254, Testmatrix Zeilen 1–13 (abgeschrieben aus der Spec).
const MATRIX: [Row; 13] = [
    Row {
        nr: 1,
        channel: None,
        installed: "1.0.1",
        stable: Some("1.0.1"),
        beta: Some("1.1.0-beta.1"),
        requested: Channel::Stable,
        result: None,
        desktop_file: None,
    },
    Row {
        nr: 2,
        channel: Some("stable"),
        installed: "1.0.1",
        stable: Some("1.0.2"),
        beta: Some("1.1.0-beta.1"),
        requested: Channel::Stable,
        result: Some("1.0.2"),
        desktop_file: Some("stable"),
    },
    Row {
        nr: 3,
        channel: Some("beta"),
        installed: "1.0.1",
        stable: Some("1.0.1"),
        beta: Some("1.1.0-beta.1"),
        requested: Channel::Beta,
        result: Some("1.1.0-beta.1"),
        desktop_file: Some("beta"),
    },
    Row {
        nr: 4,
        channel: Some("beta"),
        installed: "1.0.1",
        stable: Some("1.0.1"),
        beta: Some("1.0.1"),
        requested: Channel::Beta,
        result: None,
        desktop_file: Some("beta"),
    },
    Row {
        nr: 5,
        channel: None,
        installed: "1.1.0-beta.1",
        stable: Some("1.0.1"),
        beta: Some("1.1.0-beta.2"),
        requested: Channel::Beta,
        result: Some("1.1.0-beta.2"),
        desktop_file: Some("beta"),
    },
    Row {
        nr: 6,
        channel: None,
        installed: "1.1.0-rc.1",
        stable: Some("1.0.1"),
        beta: Some("1.1.0"),
        requested: Channel::Beta,
        result: Some("1.1.0"),
        desktop_file: Some("beta"),
    },
    Row {
        nr: 7,
        channel: Some("beta"),
        installed: "1.1.0-beta.2",
        stable: Some("1.1.0"),
        beta: Some("1.1.0"),
        requested: Channel::Beta,
        result: Some("1.1.0"),
        desktop_file: Some("beta"),
    },
    Row {
        nr: 8,
        channel: Some("beta"),
        installed: "1.1.0",
        stable: Some("1.1.0"),
        beta: Some("1.2.0-beta.1"),
        requested: Channel::Beta,
        result: Some("1.2.0-beta.1"),
        desktop_file: Some("beta"),
    },
    Row {
        nr: 9,
        channel: Some("beta"),
        installed: "1.1.0-beta.2",
        stable: Some("1.0.1"),
        beta: Some("1.1.0-beta.1"),
        requested: Channel::Beta,
        result: None,
        desktop_file: Some("beta"),
    },
    Row {
        nr: 10,
        channel: Some("stable"),
        installed: "1.1.0-beta.3",
        stable: Some("1.0.1"),
        beta: Some("1.1.0-beta.4"),
        requested: Channel::Stable,
        result: None,
        desktop_file: Some("stable"),
    },
    Row {
        nr: 11,
        channel: Some("stable"),
        installed: "1.1.0-beta.3",
        stable: Some("1.1.0"),
        beta: Some("1.1.0"),
        requested: Channel::Stable,
        result: Some("1.1.0"),
        desktop_file: Some("stable"),
    },
    Row {
        nr: 12,
        channel: Some("stable"),
        installed: "1.0.1",
        stable: Some("1.1.0-beta.1"),
        beta: None,
        requested: Channel::Stable,
        result: Some("rejected"),
        desktop_file: Some("stable"),
    },
    Row {
        nr: 13,
        channel: Some("nightly"),
        installed: "1.0.1",
        stable: Some("1.0.1"),
        beta: Some("1.1.0-beta.1"),
        requested: Channel::Stable,
        result: None,
        desktop_file: Some("nightly"),
    },
];

fn assert_requested(m: &Mock, row: &Row, who: &str) {
    let (stable, beta) = (m.fx.stable_requests(), m.fx.beta_requests());
    match row.requested {
        Channel::Stable => assert_eq!(
            (stable, beta),
            (1, 0),
            "Zeile {} ({who}): genau ein Request an den stabilen Pfad erwartet",
            row.nr
        ),
        Channel::Beta => assert_eq!(
            (stable, beta),
            (0, 1),
            "Zeile {} ({who}): genau ein Request an den Beta-Pfad erwartet",
            row.nr
        ),
    }
}

fn desktop_result(state: &UpdateState) -> Res {
    match state {
        UpdateState::UpToDate => Res::Current,
        UpdateState::Available(found) => Res::Update(found.version.clone()),
        UpdateState::Failed(_) => Res::Rejected,
        other => panic!("unerwarteter Zustand nach manueller Prüfung: {other:?}"),
    }
}

/// Desktop: Start (Kanal ggf. speichern) und manuelle Prüfung. Prüft, dass ein zu speicherndes `beta` vor dem ersten
/// Request in der Datei steht.
async fn desktop_row(row: &Row) -> Res {
    let m = mock(row.stable, row.beta).await;
    let home = profile(row.channel);
    let app = App::new(AppConfig::new(home.path())).unwrap();
    let installed: semver::Version = row.installed.parse().unwrap();
    persist_startup_channel(&app, &installed);
    let before_check = settings_file(home.path());
    assert_eq!(
        m.fx.stable_requests() + m.fx.beta_requests(),
        0,
        "Zeile {}: der Start darf noch keinen Request auslösen",
        row.nr
    );
    if !installed.pre.is_empty() && row.channel.is_none() {
        assert_eq!(
            updates_channel_line(&before_check).as_deref(),
            Some("beta"),
            "Zeile {}: `beta` muss gespeichert sein, bevor der Request ausgeht",
            row.nr
        );
    }
    let state = run_check_with_context(&app, true, desktop_ctx(&m, row.installed)).await;
    assert!(
        !matches!(
            state,
            UpdateState::Downloading { .. } | UpdateState::Ready(_)
        ),
        "Zeile {}: kein Download erwartet, war {state:?}",
        row.nr
    );
    assert_requested(&m, row, "Desktop");
    let after = settings_file(home.path());
    assert_eq!(
        updates_channel_line(&after).as_deref(),
        row.desktop_file,
        "Zeile {}: Wert von `updates.channel` in settings.toml nach Start und Prüfung\n{after}",
        row.nr
    );
    if row.desktop_file.is_none() {
        assert!(
            !any_channel_key(&after),
            "Zeile {}: kein Schlüssel `channel` erwartet\n{after}",
            row.nr
        );
    }
    desktop_result(&state)
}

/// CLI `--check-update` (Kern des Aufrufs): Exit 0 = aktuell, 10 = Update, Fehler = abgelehnt; `settings.toml` bytegleich.
async fn cli_row(row: &Row) -> Res {
    let m = mock(row.stable, row.beta).await;
    let home = profile(row.channel);
    let path = AppSettings::path_in(home.path());
    let before = std::fs::read(&path).unwrap();
    let ctx = cli_ctx(&m, home.path(), row.installed);
    let result = os_cli::update::check_with_context(&ctx).await;
    assert_eq!(
        std::fs::read(&path).unwrap(),
        before,
        "Zeile {}: das CLI darf settings.toml nie ändern",
        row.nr
    );
    let entries: Vec<_> = std::fs::read_dir(home.path())
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(
        entries.len(),
        1,
        "Zeile {}: das CLI darf im Einstellungsordner nichts anlegen: {entries:?}",
        row.nr
    );
    assert_requested(&m, row, "CLI");
    match result {
        Ok(0) => Res::Current,
        // Exit 10 nennt die Version nur auf stdout; verglichen wird hier die Ergebnisart.
        Ok(10) => Res::Update(String::new()),
        Ok(code) => panic!("Zeile {}: unerwarteter Exit-Code {code}", row.nr),
        Err(_) => Res::Rejected,
    }
}

// spec: OUT-254
// spec: OUT-252
// spec: OUT-250
// acceptance: independent
#[tokio::test]
async fn testmatrix_desktop_core_manual_check_rows_1_to_13() {
    let mut failures = Vec::new();
    for row in MATRIX.iter() {
        let got = desktop_row(row).await;
        if got != row.expected() {
            failures.push(format!(
                "Zeile {}: erwartet {:?}, Desktop {:?}",
                row.nr,
                row.expected(),
                got
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

// spec: OUT-254
// spec: OUT-252
// spec: OUT-250
// acceptance: independent
#[tokio::test]
async fn testmatrix_cli_check_update_rows_1_to_13_never_writes_settings() {
    let mut failures = Vec::new();
    for row in MATRIX.iter() {
        let got = cli_row(row).await;
        if std::mem::discriminant(&got) != std::mem::discriminant(&row.expected()) {
            failures.push(format!(
                "Zeile {}: erwartet {:?}, CLI {:?}",
                row.nr,
                row.expected(),
                got
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Desktop und CLI liefern je Zeile dieselbe Ergebnisart (aktuell / Update / abgelehnt).
// spec: OUT-254
// acceptance: independent
#[tokio::test]
async fn testmatrix_desktop_and_cli_agree_per_row() {
    for row in MATRIX.iter() {
        let desktop = desktop_row(row).await;
        let cli = cli_row(row).await;
        let kind = |r: &Res| std::mem::discriminant(r);
        assert_eq!(
            kind(&desktop),
            kind(&cli),
            "Zeile {}: Desktop {desktop:?}, CLI {cli:?}",
            row.nr
        );
    }
}

// --- Standardregel und ungültige Werte -------------------------------------------------------------------------

fn settings_with(channel: Option<&str>) -> UpdateSettings {
    UpdateSettings {
        channel: channel.map(str::to_string),
        ..UpdateSettings::default()
    }
}

fn v(s: &str) -> semver::Version {
    s.parse().unwrap()
}

// spec: OUT-254
// acceptance: independent
#[test]
fn standard_rule_prerelease_is_beta_otherwise_stable() {
    for (version, expected) in [
        ("1.0.1", Channel::Stable),
        ("1.1.0", Channel::Stable),
        ("1.0.1+build.7", Channel::Stable),
        ("1.1.0-beta.1", Channel::Beta),
        ("1.1.0-rc.1", Channel::Beta),
        ("1.0.0-beta.3.1", Channel::Beta),
        ("2.0.0-alpha", Channel::Beta),
    ] {
        assert_eq!(default_channel(&v(version)), expected, "{version}");
        assert_eq!(
            effective_channel(&settings_with(None), &v(version)),
            expected,
            "{version} ohne Schlüssel"
        );
    }
}

// spec: OUT-254
// acceptance: independent
#[test]
fn valid_stored_value_always_wins_over_the_rule() {
    assert_eq!(
        effective_channel(&settings_with(Some("stable")), &v("1.1.0-beta.3")),
        Channel::Stable
    );
    assert_eq!(
        effective_channel(&settings_with(Some("beta")), &v("1.0.1")),
        Channel::Beta
    );
    assert_eq!(stored_channel(&settings_with(None)), Ok(None));
    assert_eq!(
        stored_channel(&settings_with(Some("stable"))),
        Ok(Some(Channel::Stable))
    );
    assert_eq!(
        stored_channel(&settings_with(Some("beta"))),
        Ok(Some(Channel::Beta))
    );
}

// spec: OUT-254
// acceptance: independent
#[test]
fn everything_except_stable_and_beta_counts_as_unset() {
    for raw in [
        "nightly", "", "Beta", "STABLE", " beta", "beta ", "stabil", "dev",
    ] {
        let s = settings_with(Some(raw));
        assert!(stored_channel(&s).is_err(), "{raw:?} muss ungültig sein");
        assert_eq!(
            effective_channel(&s, &v("1.0.1")),
            Channel::Stable,
            "{raw:?} auf 1.0.1"
        );
        assert_eq!(
            effective_channel(&s, &v("1.1.0-beta.1")),
            Channel::Beta,
            "{raw:?} auf 1.1.0-beta.1"
        );
    }
}

// spec: OUT-254
// spec: OUT-222
// acceptance: independent
#[test]
fn startup_stores_only_beta_and_only_for_a_prerelease_without_valid_value() {
    let cases: [(Option<&str>, &str, Option<Channel>); 8] = [
        (None, "1.0.1", None),
        (None, "1.1.0-beta.1", Some(Channel::Beta)),
        (None, "1.1.0-rc.1", Some(Channel::Beta)),
        (Some("nightly"), "1.1.0-beta.1", Some(Channel::Beta)),
        (Some("nightly"), "1.0.1", None),
        (Some("stable"), "1.1.0-beta.3", None),
        (Some("beta"), "1.1.0-beta.3", None),
        (Some("beta"), "1.0.1", None),
    ];
    for (stored, version, expected) in cases {
        let got = channel_to_store_at_startup(&settings_with(stored), &v(version));
        assert_eq!(got, expected, "gespeichert {stored:?}, Version {version}");
        assert_ne!(
            got,
            Some(Channel::Stable),
            "`stable` wird nie automatisch gespeichert"
        );
    }
}

// --- Persistenz über Versionswechsel ---------------------------------------------------------------------------

async fn start_and_check(home: &Path, installed: &str, m: &Mock) -> UpdateState {
    let app = App::new(AppConfig::new(home)).unwrap();
    persist_startup_channel(&app, &v(installed));
    run_check_with_context(&app, true, desktop_ctx(m, installed)).await
}

// spec: OUT-254
// acceptance: independent
#[tokio::test]
async fn beta_stored_on_prerelease_start_survives_update_to_stable() {
    let home = profile(None);
    let m = mock(Some("1.0.1"), Some("1.1.0-beta.2")).await;
    start_and_check(home.path(), "1.1.0-beta.1", &m).await;
    let file = settings_file(home.path());
    assert_eq!(
        updates_channel_line(&file).as_deref(),
        Some("beta"),
        "{file}"
    );
    let parsed = AppSettings::from_toml_str(&file).unwrap();
    assert_eq!(parsed.updates.channel.as_deref(), Some("beta"));

    // Gleiches Profil, jetzt 1.1.0 (stabil): Wert bleibt, Prüfung fragt den Beta-Pfad an.
    let m = mock(Some("1.1.0"), Some("1.2.0-beta.1")).await;
    let state = start_and_check(home.path(), "1.1.0", &m).await;
    assert_eq!(
        updates_channel_line(&settings_file(home.path())).as_deref(),
        Some("beta")
    );
    assert_eq!((m.fx.stable_requests(), m.fx.beta_requests()), (0, 1));
    assert!(
        matches!(&state, UpdateState::Available(u) if u.version == "1.2.0-beta.1"),
        "{state:?}"
    );
}

// spec: OUT-254
// spec: OUT-222
// acceptance: independent
#[tokio::test]
async fn stable_version_without_key_writes_no_channel_key() {
    let home = profile(None);
    let m = mock(Some("1.0.2"), Some("1.1.0-beta.1")).await;
    start_and_check(home.path(), "1.0.1", &m).await;
    start_and_check(home.path(), "1.0.1", &m).await;
    let file = settings_file(home.path());
    assert!(!any_channel_key(&file), "{file}");
    assert_eq!((m.fx.stable_requests(), m.fx.beta_requests()), (2, 0));
}

// spec: OUT-254
// acceptance: independent
#[tokio::test]
async fn stored_stable_on_prerelease_is_kept_across_restarts() {
    let home = profile(Some("stable"));
    let m = mock(Some("1.0.1"), Some("1.1.0-beta.4")).await;
    for _ in 0..3 {
        let state = start_and_check(home.path(), "1.1.0-beta.3", &m).await;
        assert_eq!(state, UpdateState::UpToDate, "kein Downgrade auf 1.0.1");
        assert_eq!(
            updates_channel_line(&settings_file(home.path())).as_deref(),
            Some("stable")
        );
    }
    assert_eq!((m.fx.stable_requests(), m.fx.beta_requests()), (3, 0));
}

// spec: OUT-254
// spec: OUT-222
// acceptance: independent
#[tokio::test]
async fn invalid_value_stays_byte_for_byte_in_the_file_and_is_logged() {
    let home = profile(Some("nightly"));
    let m = mock(Some("1.0.1"), Some("1.1.0-beta.1")).await;
    let log = LogCapture::default();
    let state = {
        let _guard = tracing::subscriber::set_default(log.subscriber());
        start_and_check(home.path(), "1.0.1", &m).await
    };
    assert_eq!(state, UpdateState::UpToDate);
    assert_eq!(
        updates_channel_line(&settings_file(home.path())).as_deref(),
        Some("nightly")
    );
    assert_eq!((m.fx.stable_requests(), m.fx.beta_requests()), (1, 0));
    assert!(
        log.warnings().iter().any(|l| l.contains("nightly")
            || l.to_lowercase().contains("kanal")
            || l.to_lowercase().contains("channel")),
        "Warnung zum ungültigen Kanal im Log erwartet:\n{}",
        log.text()
    );
}

// spec: OUT-254
// acceptance: independent
#[tokio::test]
async fn cli_logs_a_warning_for_an_invalid_value() {
    let home = profile(Some("nightly"));
    let m = mock(Some("1.0.1"), Some("1.1.0-beta.1")).await;
    let ctx = cli_ctx(&m, home.path(), "1.0.1");
    let log = LogCapture::default();
    let result = {
        let _guard = tracing::subscriber::set_default(log.subscriber());
        os_cli::update::check_with_context(&ctx).await
    };
    assert_eq!(result.ok(), Some(0));
    assert!(
        log.warnings().iter().any(|l| l.contains("nightly")
            || l.to_lowercase().contains("kanal")
            || l.to_lowercase().contains("channel")),
        "Warnung zum ungültigen Kanal im Log erwartet:\n{}",
        log.text()
    );
}

// --- Wechsel ---------------------------------------------------------------------------------------------------

// spec: OUT-254
// spec: UI-131
// acceptance: independent
#[tokio::test]
async fn switching_stable_to_beta_sends_no_request_then_manual_check_hits_beta_once() {
    let home = profile(Some("stable"));
    let m = mock(Some("1.0.1"), Some("1.1.0-beta.1")).await;
    let app = App::new(AppConfig::new(home.path())).unwrap();
    persist_startup_channel(&app, &v("1.0.1"));
    let state = set_channel_value(&app, UpdateChannel::Beta).unwrap();
    assert_eq!(state.channel, UpdateChannel::Beta);
    // Sofort gespeichert, ohne Neustart.
    assert_eq!(
        updates_channel_line(&settings_file(home.path())).as_deref(),
        Some("beta")
    );
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    assert_eq!(
        (m.fx.stable_requests(), m.fx.beta_requests()),
        (0, 0),
        "Umschalten darf keinen Request auslösen"
    );
    let checked = run_check_with_context(&app, true, desktop_ctx(&m, "1.0.1")).await;
    assert_eq!((m.fx.stable_requests(), m.fx.beta_requests()), (0, 1));
    assert!(
        matches!(&checked, UpdateState::Available(u) if u.version == "1.1.0-beta.1"),
        "{checked:?}"
    );
}

// spec: OUT-254
// acceptance: independent
#[tokio::test]
async fn switching_beta_to_stable_on_prerelease_offers_no_downgrade() {
    let home = profile(Some("beta"));
    let m = mock(Some("1.0.1"), Some("1.1.0-beta.4")).await;
    let app = App::new(AppConfig::new(home.path())).unwrap();
    persist_startup_channel(&app, &v("1.1.0-beta.3"));
    set_channel_value(&app, UpdateChannel::Stable).unwrap();
    assert_eq!(
        updates_channel_line(&settings_file(home.path())).as_deref(),
        Some("stable")
    );
    let state = run_check_with_context(&app, true, desktop_ctx(&m, "1.1.0-beta.3")).await;
    assert_eq!(state, UpdateState::UpToDate);
    assert_eq!((m.fx.stable_requests(), m.fx.beta_requests()), (1, 0));
}

// spec: OUT-254
// spec: OUT-251
// acceptance: independent
#[tokio::test]
async fn channel_switch_keeps_skipped_version_and_check_flag() {
    let home = tempfile::tempdir().unwrap();
    std::fs::write(
        AppSettings::path_in(home.path()),
        "[updates]\ncheck = false\nskipped_version = \"1.0.2\"\nlast_check = \"2026-11-20T08:00:00Z\"\n",
    )
    .unwrap();
    let app = App::new(AppConfig::new(home.path())).unwrap();
    for channel in [
        UpdateChannel::Beta,
        UpdateChannel::Stable,
        UpdateChannel::Beta,
    ] {
        set_channel_value(&app, channel).unwrap();
        let parsed = AppSettings::from_toml_str(&settings_file(home.path())).unwrap();
        assert!(!parsed.updates.check, "updates.check muss aus bleiben");
        assert_eq!(parsed.updates.skipped_version.as_deref(), Some("1.0.2"));
    }
    let parsed = AppSettings::from_toml_str(&settings_file(home.path())).unwrap();
    assert_eq!(parsed.updates.channel.as_deref(), Some("beta"));
}

// spec: OUT-222
// acceptance: independent
#[tokio::test]
async fn channel_lives_in_settings_toml_and_not_in_any_crawl_config() {
    let home = profile(None);
    let app = App::new(AppConfig::new(home.path())).unwrap();
    set_channel_value(&app, UpdateChannel::Beta).unwrap();
    assert_eq!(
        updates_channel_line(&settings_file(home.path())).as_deref(),
        Some("beta")
    );
    let crawl = os_model::CrawlConfig::default().to_toml_string().unwrap();
    assert!(!crawl.contains("[updates]"), "{crawl}");
    assert!(!any_channel_key(&crawl), "{crawl}");
    // Keine andere Datei im Einstellungsordner trägt den Kanal (insbesondere keine `.osconfig`).
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let p = entry.path();
            if p.is_dir() {
                walk(&p, out);
            } else {
                out.push(p);
            }
        }
    }
    let mut files = Vec::new();
    walk(home.path(), &mut files);
    for file in files {
        if file.file_name().and_then(|n| n.to_str()) == Some("settings.toml") {
            continue;
        }
        if let Ok(text) = std::fs::read_to_string(&file) {
            assert!(
                !(file.extension().and_then(|e| e.to_str()) == Some("osconfig")
                    && any_channel_key(&text)),
                "{} enthält den Kanal",
                file.display()
            );
        }
    }
}

// spec: OUT-222
// acceptance: independent
#[tokio::test]
async fn settings_without_channel_key_from_1_0_0_read_without_migration() {
    let home = profile(None);
    let before = std::fs::read(AppSettings::path_in(home.path())).unwrap();
    let parsed = AppSettings::from_toml_str(std::str::from_utf8(&before).unwrap()).unwrap();
    assert_eq!(parsed.updates.channel, None);
    assert!(parsed.updates.check);
    let app = App::new(AppConfig::new(home.path())).unwrap();
    assert_eq!(app.settings_snapshot().updates.channel, None);
    assert!(!any_channel_key(&settings_file(home.path())));
}

// --- UI-131 Zustand -------------------------------------------------------------------------------------------

// spec: UI-131
// spec: OUT-254
// acceptance: independent
#[test]
fn channel_state_reports_effective_channel_and_version_without_build_number() {
    let s = channel_state(&settings_with(None), &v("1.0.1"));
    assert_eq!(
        (s.channel, s.version.as_str(), s.prerelease),
        (UpdateChannel::Stable, "1.0.1", false)
    );
    let s = channel_state(&settings_with(None), &v("1.1.0-beta.1"));
    assert_eq!(
        (s.channel, s.version.as_str(), s.prerelease),
        (UpdateChannel::Beta, "1.1.0-beta.1", true)
    );
    let s = channel_state(&settings_with(Some("stable")), &v("1.1.0-beta.3+42"));
    assert_eq!(
        (s.channel, s.version.as_str(), s.prerelease),
        (UpdateChannel::Stable, "1.1.0-beta.3", true)
    );
    let s = channel_state(&settings_with(Some("nightly")), &v("1.0.1+7"));
    assert_eq!(
        (s.channel, s.version.as_str(), s.prerelease),
        (UpdateChannel::Stable, "1.0.1", false)
    );
}

// --- Abschaltung, Anfrageinhalt, feste URLs --------------------------------------------------------------------

// spec: OUT-254
// spec: OUT-250
// spec: OUT-253
// acceptance: independent
#[tokio::test]
async fn environment_switch_disables_checks_in_both_channels_for_app_and_cli() {
    for channel in ["stable", "beta"] {
        let m = mock(Some("9.0.0"), Some("9.1.0-beta.1")).await;
        let home = profile(Some(channel));
        let app = App::new(AppConfig::new(home.path())).unwrap();
        let mut ctx = desktop_ctx(&m, "1.0.1");
        ctx.disabled_by_environment = true;
        let state = run_check_with_context(&app, true, ctx).await;
        assert!(
            matches!(state, UpdateState::Disabled(_)),
            "{channel}: {state:?}"
        );

        let mut cli = cli_ctx(&m, home.path(), "1.0.1");
        cli.disabled_by_environment = true;
        assert!(
            os_cli::update::check_with_context(&cli).await.is_err(),
            "{channel}: CLI muss die Prüfung ablehnen"
        );
        assert_eq!(
            (m.fx.stable_requests(), m.fx.beta_requests()),
            (0, 0),
            "{channel}: kein Request bei Abschaltung"
        );
        assert_eq!(
            updates_channel_line(&settings_file(home.path())).as_deref(),
            Some(channel),
            "gespeicherter Wert bleibt erhalten"
        );
    }
}

// spec: OUT-250
// spec: OUT-254
// acceptance: independent
#[tokio::test]
async fn request_carries_only_version_os_arch_in_both_channels() {
    for (channel, installed) in [
        ("stable", "1.0.1"),
        ("beta", "1.0.1"),
        ("missing", "1.1.0-beta.1"),
    ] {
        let m = mock(Some("1.0.2"), Some("1.1.0-beta.2")).await;
        let home = profile((channel != "missing").then_some(channel));
        let app = App::new(AppConfig::new(home.path())).unwrap();
        persist_startup_channel(&app, &v(installed));
        run_check_with_context(&app, true, desktop_ctx(&m, installed)).await;
        let cli = cli_ctx(&m, home.path(), installed);
        let _ = os_cli::update::check_with_context(&cli).await;
        let log = m.fx.site.requests();
        assert_eq!(log.len(), 2, "{channel}: ein Request je App und CLI");
        for request in log {
            let query = request
                .path_and_query
                .split_once('?')
                .map(|(_, q)| q)
                .unwrap_or("");
            let mut keys: Vec<&str> = query
                .split('&')
                .filter(|p| !p.is_empty())
                .map(|p| p.split('=').next().unwrap())
                .collect();
            keys.sort_unstable();
            assert_eq!(
                keys,
                ["arch", "os", "version"],
                "{channel}: nur Version, OS und Architektur, kein Kanal-Parameter: {}",
                request.path_and_query
            );
            assert!(!request.path_and_query.contains("channel"));
        }
    }
}

// spec: OUT-254
// acceptance: independent
#[test]
fn built_in_urls_match_the_spec_exactly_in_app_and_cli() {
    assert_eq!(STABLE_ENDPOINT, STABLE_URL);
    assert_eq!(BETA_ENDPOINT, BETA_URL);
    assert_eq!(os_cli::update::policy::STABLE_ENDPOINT, STABLE_URL);
    assert_eq!(os_cli::update::policy::BETA_ENDPOINT, BETA_URL);
    let app = UpdateContext::default();
    assert_eq!(app.endpoint, STABLE_URL);
    assert_eq!(app.beta_endpoint, BETA_URL);
    let empty = tempfile::tempdir().unwrap();
    let cli = os_cli::update::context_from_settings(empty.path()).unwrap();
    assert_eq!(cli.endpoint, STABLE_URL);
    assert_eq!(cli.beta_endpoint, BETA_URL);
}

// spec: OUT-254
// acceptance: independent
#[test]
fn channel_follows_settings_and_version_only_not_the_build() {
    for (stored, version, url) in [
        (None, "1.0.1", STABLE_URL),
        (None, "1.1.0-beta.1", BETA_URL),
        (Some("beta"), "1.0.1", BETA_URL),
        (Some("stable"), "1.1.0-beta.1", STABLE_URL),
        (Some("nightly"), "1.0.1", STABLE_URL),
    ] {
        let ctx = UpdateContext {
            settings: settings_with(stored),
            current: v(version),
            ..Default::default()
        };
        assert_eq!(ctx.channel_endpoint(), url, "{stored:?} / {version}");
    }
    // CLI: gleicher Kanal aus derselben settings.toml.
    for (stored, url) in [
        (Some("beta"), BETA_URL),
        (Some("stable"), STABLE_URL),
        (None, STABLE_URL),
    ] {
        let home = profile(stored);
        let mut cli = os_cli::update::context_from_settings(home.path()).unwrap();
        cli.current = v("1.0.1");
        assert_eq!(cli.channel_endpoint(), url, "CLI {stored:?}");
    }
    let home = profile(None);
    let mut cli = os_cli::update::context_from_settings(home.path()).unwrap();
    cli.current = v("1.1.0-rc.1");
    assert_eq!(
        cli.channel_endpoint(),
        BETA_URL,
        "CLI Standardregel Vorabversion"
    );
}

// --- Log-Mitschnitt --------------------------------------------------------------------------------------------

#[derive(Clone, Default)]
struct LogCapture(Arc<Mutex<Vec<u8>>>);
struct LogWriter(Arc<Mutex<Vec<u8>>>);
impl std::io::Write for LogWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
impl LogCapture {
    fn subscriber(&self) -> impl tracing::Subscriber + Send + Sync {
        let buf = self.0.clone();
        tracing_subscriber::fmt()
            .with_ansi(false)
            .with_max_level(tracing::Level::TRACE)
            .with_writer(move || LogWriter(buf.clone()))
            .finish()
    }
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.0.lock().unwrap()).into_owned()
    }
    fn warnings(&self) -> Vec<String> {
        self.text()
            .lines()
            .filter(|l| l.contains("WARN"))
            .map(str::to_string)
            .collect()
    }
}
