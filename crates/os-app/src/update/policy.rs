//! Shared, Tauri-free check policy. Also compiled by the CLI through a source module.
use super::manifest::{https_url, platform_key, PlatformEntry, UpdateManifest};
use os_model::settings::{NetworkSettings, UpdateSettings};
use std::{
    sync::{Mutex, OnceLock},
    time::Duration,
};
pub const USER_AGENT: &str = "OpenSpider-Updater";
pub const STABLE_ENDPOINT: &str =
    "https://github.com/wojcikdominik/openspider/releases/latest/download/latest.json";
pub const BETA_ENDPOINT: &str =
    "https://github.com/wojcikdominik/openspider/releases/latest/download/latest-beta.json";
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Disabled {
    Settings,
    Environment,
    MissingPublicKey,
    InvalidPublicKey,
    /// Windows: Installationsart (MSI/NSIS) nicht sicher bestimmbar; kein Paket wird angefordert.
    UnknownInstaller,
}
/// Release-Seite für den manuellen Download, wenn kein automatisches Update möglich ist.
pub const RELEASES_PAGE: &str = "https://github.com/wojcikdominik/openspider/releases/latest";

/// Welches Paket der Update-Kern aus dem Manifest anfordert (OUT-266: MSI pro Maschine, NSIS pro Benutzer).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Installer {
    /// Plattform ohne Installer-Varianten (macOS): Schlüssel `{os}-{arch}`.
    Platform,
    /// MSI-Installation (pro Maschine): Schlüssel `{os}-{arch}-msi`.
    Msi,
    /// NSIS-Installation (pro Benutzer): Schlüssel `{os}-{arch}-nsis`.
    Nsis,
    /// Windows, Art nicht sicher bestimmbar: keine Anfrage, Hinweis auf manuellen Download.
    Unknown,
    /// Nur Verfügbarkeit melden (CLI, installiert nie): irgendein Eintrag dieser Plattform genügt.
    AnyReportOnly,
}
impl Installer {
    /// Installationsart aus der beim Bau eingebetteten Bundle-Kennung. Der Tauri-Bundler schreibt sie je Paket in
    /// die ausgelieferte EXE (MSI → `msi`, NSIS → `nsis`); `tauri-plugin-updater` wählt beim Herunterladen mit
    /// derselben Kennung, Prüfung und Download nutzen also eine Quelle. Fehlt sie oder ist sie fremd (ungepatchter
    /// Build), gilt unter Windows „unbekannt“. Andere Systeme haben keine Installer-Varianten.
    pub fn detect(os: &str, bundle_type: Option<&str>) -> Self {
        if os != "windows" {
            return Self::Platform;
        }
        match bundle_type.map(str::to_ascii_lowercase).as_deref() {
            Some("msi") => Self::Msi,
            Some("nsis") => Self::Nsis,
            _ => Self::Unknown,
        }
    }
    fn suffix(self) -> Option<&'static str> {
        match self {
            Self::Msi => Some("msi"),
            Self::Nsis => Some("nsis"),
            Self::Platform | Self::Unknown | Self::AnyReportOnly => None,
        }
    }
}

static BUNDLE_TYPE: OnceLock<Option<String>> = OnceLock::new();
/// Die Desktop-App meldet beim Start die eingebettete Bundle-Kennung (Tauri-frei gekapselt); das CLI tut es nie.
pub fn register_bundle_type(bundle_type: Option<&str>) {
    let _ = BUNDLE_TYPE.set(bundle_type.map(str::to_owned));
}
fn registered_bundle_type() -> Option<&'static str> {
    BUNDLE_TYPE.get().and_then(|b| b.as_deref())
}
#[derive(Debug)]
pub enum Outcome {
    Disabled(Disabled),
    Throttled,
    Current,
    Available(UpdateManifest),
}
pub struct UpdateContext {
    pub endpoint: String,
    pub beta: bool,
    pub current: semver::Version,
    pub os: String,
    pub arch: String,
    pub installer: Installer,
    pub pubkey: String,
    pub settings: UpdateSettings,
    pub disabled_by_environment: bool,
    pub network: NetworkSettings,
    pub proxy_password: Option<String>,
    pub now: time::OffsetDateTime,
    /// Serializes checks and reserves the timestamp before any request, including failed requests.
    pub last_attempt: Mutex<Option<time::OffsetDateTime>>,
}
impl Default for UpdateContext {
    fn default() -> Self {
        let beta = option_env!("OPENSPIDER_UPDATE_BETA") == Some("1");
        let os: String = match std::env::consts::OS {
            "macos" => "darwin",
            other => other,
        }
        .into();
        Self {
            endpoint: if beta { BETA_ENDPOINT } else { STABLE_ENDPOINT }.into(),
            beta,
            current: semver::Version::parse(env!("CARGO_PKG_VERSION")).expect("package SemVer"),
            installer: Installer::detect(&os, registered_bundle_type()),
            os,
            arch: std::env::consts::ARCH.into(),
            pubkey: option_env!("OPENSPIDER_UPDATER_PUBKEY")
                .unwrap_or("")
                .into(),
            settings: UpdateSettings::default(),
            // Gemeinsamer Richtlinien-Resolver (OUT-253; M3-C01 nutzt ihn für DisableCrashReports).
            disabled_by_environment: update_check_disabled(&SystemPolicy),
            network: NetworkSettings::default(),
            proxy_password: None,
            now: os_model::Clock::now(&os_model::SystemClock).0,
            last_attempt: Mutex::new(None),
        }
    }
}
impl UpdateContext {
    pub fn disabled(&self) -> Option<Disabled> {
        if self.disabled_by_environment {
            Some(Disabled::Environment)
        } else if !self.settings.check {
            Some(Disabled::Settings)
        } else if self.pubkey.is_empty() {
            Some(Disabled::MissingPublicKey)
        } else if super::verify::public_key(&self.pubkey).is_err() {
            Some(Disabled::InvalidPublicKey)
        } else if self.installer == Installer::Unknown {
            Some(Disabled::UnknownInstaller)
        } else {
            None
        }
    }
    /// Manifest-Schlüssel des Pakets, das zu dieser Installation passt; `None`, wenn keins angefordert werden darf.
    pub fn target(&self) -> Option<String> {
        match self.installer {
            Installer::Unknown | Installer::AnyReportOnly => None,
            kind => Some(platform_key(&self.os, &self.arch, kind.suffix())),
        }
    }
    /// Passender Manifest-Eintrag. Kein Rückfall auf ein anderes Installer-Paket: eine MSI-Installation bekommt nie
    /// das NSIS-Paket und umgekehrt. Nur `AnyReportOnly` (CLI) nimmt irgendeinen Eintrag dieser Plattform.
    pub fn select<'a>(&self, manifest: &'a UpdateManifest) -> Option<(&'a str, &'a PlatformEntry)> {
        if self.installer == Installer::AnyReportOnly {
            let base = platform_key(&self.os, &self.arch, None);
            return manifest
                .platforms
                .iter()
                .find(|(key, _)| {
                    key.as_str() == base
                        || key
                            .strip_prefix(base.as_str())
                            .is_some_and(|rest| rest.starts_with('-'))
                })
                .map(|(key, entry)| (key.as_str(), entry));
        }
        let target = self.target()?;
        manifest
            .platforms
            .get_key_value(&target)
            .map(|(key, entry)| (key.as_str(), entry))
    }
}
/// Five seconds covers DNS, TLS, redirects and response streaming together.
pub fn reserve(ctx: &UpdateContext, manual: bool) -> Result<Option<Outcome>, String> {
    if let Some(reason) = ctx.disabled() {
        return Ok(Some(Outcome::Disabled(reason)));
    }
    let endpoint = https_url(&ctx.endpoint)?;
    if endpoint.query().is_some() {
        return Err("Update-Endpunkt darf keine Query enthalten".into());
    }
    {
        let mut last = ctx.last_attempt.lock().unwrap_or_else(|p| p.into_inner());
        let persisted = ctx.settings.last_check.as_deref().and_then(|s| {
            time::OffsetDateTime::parse(s, &time::format_description::well_known::Rfc3339).ok()
        });
        if !manual
            && last
                .max(persisted)
                .is_some_and(|at| ctx.now - at < time::Duration::hours(24))
        {
            return Ok(Some(Outcome::Throttled));
        }
        *last = Some(ctx.now);
    }
    Ok(None)
}
pub async fn check(ctx: &UpdateContext, manual: bool) -> Result<Outcome, String> {
    match reserve(ctx, manual)? {
        Some(outcome) => Ok(outcome),
        None => fetch(ctx).await,
    }
}
/// Call only after reserve; the app persists the reservation before fetching.
pub async fn fetch(ctx: &UpdateContext) -> Result<Outcome, String> {
    let mut endpoint = https_url(&ctx.endpoint)?;
    endpoint.query_pairs_mut().extend_pairs([
        ("version", ctx.current.to_string().as_str()),
        ("os", &ctx.os),
        ("arch", &ctx.arch),
    ]);
    let request = async {
        let client = os_fetch::net::client_builder_with_password(
            &ctx.network,
            ctx.proxy_password.as_deref(),
        )
        .user_agent(USER_AGENT)
        .referer(false)
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= 5 || https_url(attempt.url().as_str()).is_err() {
                attempt.error("Unsichere Update-Weiterleitung")
            } else {
                attempt.follow()
            }
        }))
        .timeout(Duration::from_secs(5))
        .build()
        .map_err(|_| "Update-Client nicht verfügbar")?;
        let mut response = client
            .get(endpoint)
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|_| "Update-Server nicht erreichbar")?
            .error_for_status()
            .map_err(|_| "Update-Server meldet Fehler")?;
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| "Update-Antwort abgebrochen")?
        {
            if bytes.len() + chunk.len() > 1024 * 1024 {
                return Err("Update-Manifest zu groß".to_string());
            }
            bytes.extend_from_slice(&chunk);
        }
        let manifest: UpdateManifest =
            serde_json::from_slice(&bytes).map_err(|_| "Update-Manifest ungültig")?;
        manifest.validate(ctx.beta)?;
        if !manifest.is_newer_than(&ctx.current) {
            return Ok(Outcome::Current);
        }
        if ctx.select(&manifest).is_none() {
            return Err("Kein Update für diese Plattform".into());
        }
        Ok(Outcome::Available(manifest))
    };
    let result = tokio::time::timeout(Duration::from_secs(5), request)
        .await
        .map_err(|_| "Update-Prüfung: Zeitüberschreitung".to_string())
        .and_then(|r| r);
    if result.is_err() {
        tracing::info!("Update-Prüfung fehlgeschlagen (Netzwerk oder Manifest)");
    }
    result
}

// --- Verwaltete Richtlinien (OUT-253, gemeinsamer Resolver; M3-C01 ergänzt `DisableCrashReports`, OUT-247) ---

/// Name des Richtlinienwerts für Absturzberichte (OUT-247).
pub const POLICY_DISABLE_CRASH_REPORTS: &str = "DisableCrashReports";
/// Umgebungsvariable für Absturzberichte (OUT-247).
pub const ENV_DISABLE_CRASH_REPORTS: &str = "OPENSPIDER_DISABLE_CRASH_REPORTS";
/// Name des Richtlinienwerts für die Update-Prüfung (OUT-253).
pub const POLICY_DISABLE_UPDATE_CHECK: &str = "DisableUpdateCheck";
/// Umgebungsvariable für die Update-Prüfung (OUT-250).
pub const ENV_DISABLE_UPDATE_CHECK: &str = "OPENSPIDER_DISABLE_UPDATE_CHECK";

/// Ein gelesener Richtlinienwert.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolicyValue {
    /// Nicht vorhanden.
    Missing,
    /// `1`/`true` bzw. `0`/`false`.
    Set(bool),
    /// Vorhanden, aber falscher Typ (z. B. REG_SZ „ja“): wird ignoriert, Warnung im Log.
    Invalid,
}

/// Quellen der Richtlinie in der Rangfolge von OUT-253: Rechner (HKLM bzw. macOS-Konfigurationsprofil) > Benutzer
/// (HKCU) > Umgebungsvariable. Die Benutzereinstellung (`settings.toml`) entscheidet nur, wenn keine Quelle einen
/// gültigen Wert hat.
pub trait PolicySource {
    /// HKLM bzw. Konfigurationsprofil (Domain `org.openspider.desktop`).
    fn machine(&self, name: &str) -> PolicyValue;
    /// HKCU.
    fn user(&self, name: &str) -> PolicyValue;
    /// Rohwert der Umgebungsvariable.
    fn env(&self, var: &str) -> Option<String>;
}

/// Wer hat entschieden?
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolicyOrigin {
    Machine,
    User,
    Environment,
}

/// Umgebungswert lesen: `1`/`true` → an, `0`/`false` → aus, leer → fehlt, sonst ungültig.
pub fn env_value(raw: Option<&str>) -> PolicyValue {
    match raw.map(|v| v.trim().to_ascii_lowercase()) {
        None => PolicyValue::Missing,
        Some(v) if v.is_empty() => PolicyValue::Missing,
        Some(v) if v == "1" || v == "true" => PolicyValue::Set(true),
        Some(v) if v == "0" || v == "false" => PolicyValue::Set(false),
        Some(_) => PolicyValue::Invalid,
    }
}

/// Gemeinsamer Resolver: der erste gültige Wert in der Rangfolge entscheidet; `None` = keine Richtlinie, die
/// Benutzereinstellung gilt. Ungültige Werte werden mit Warnung übergangen.
pub fn resolve_policy(
    source: &dyn PolicySource,
    name: &str,
    env_var: &str,
) -> Option<(bool, PolicyOrigin)> {
    let candidates = [
        (source.machine(name), PolicyOrigin::Machine),
        (source.user(name), PolicyOrigin::User),
        (
            env_value(source.env(env_var).as_deref()),
            PolicyOrigin::Environment,
        ),
    ];
    for (value, origin) in candidates {
        match value {
            PolicyValue::Set(v) => return Some((v, origin)),
            PolicyValue::Invalid => {
                tracing::warn!(
                    policy = name,
                    ?origin,
                    "Richtlinienwert hat einen ungültigen Typ und wird ignoriert"
                );
            }
            PolicyValue::Missing => {}
        }
    }
    None
}

/// Quellen dieses Rechners. Registry- (Windows) und Profil-Leser (macOS) gibt es in dieser Fassung noch nicht
/// (OUT-253, manuelle Prüfung M3-Q03); wirksam ist die Umgebungsvariable.
pub struct SystemPolicy;

impl PolicySource for SystemPolicy {
    fn machine(&self, _: &str) -> PolicyValue {
        PolicyValue::Missing
    }
    fn user(&self, _: &str) -> PolicyValue {
        PolicyValue::Missing
    }
    fn env(&self, var: &str) -> Option<String> {
        std::env::var(var).ok()
    }
}

/// Sind Absturzberichte durch eine Richtlinie deaktiviert (`DisableCrashReports`, OUT-247)?
pub fn crash_reports_disabled(source: &dyn PolicySource) -> bool {
    resolve_policy(
        source,
        POLICY_DISABLE_CRASH_REPORTS,
        ENV_DISABLE_CRASH_REPORTS,
    )
    .is_some_and(|(disabled, _)| disabled)
}

/// Ist die Update-Prüfung durch eine Richtlinie deaktiviert (`DisableUpdateCheck`, OUT-253)? Derselbe Resolver.
pub fn update_check_disabled(source: &dyn PolicySource) -> bool {
    resolve_policy(
        source,
        POLICY_DISABLE_UPDATE_CHECK,
        ENV_DISABLE_UPDATE_CHECK,
    )
    .is_some_and(|(disabled, _)| disabled)
}

#[cfg(test)]
mod policy_tests {
    use super::*;

    struct Fixture {
        machine: PolicyValue,
        user: PolicyValue,
        env: Option<&'static str>,
    }
    impl PolicySource for Fixture {
        fn machine(&self, _: &str) -> PolicyValue {
            self.machine
        }
        fn user(&self, _: &str) -> PolicyValue {
            self.user
        }
        fn env(&self, _: &str) -> Option<String> {
            self.env.map(str::to_string)
        }
    }

    // spec: OUT-247
    // spec: OUT-253 [M3]
    #[test]
    fn rank_order_machine_user_environment_for_both_policies() {
        use PolicyValue::{Invalid, Missing, Set};
        let values = [Missing, Set(false), Set(true)];
        for machine in values {
            for user in values {
                for env in [None, Some("0"), Some("1")] {
                    let f = Fixture { machine, user, env };
                    let expected = match (machine, user) {
                        (Set(m), _) => m,
                        (Missing, Set(u)) => u,
                        _ => env == Some("1"),
                    };
                    assert_eq!(
                        crash_reports_disabled(&f),
                        expected,
                        "{machine:?} {user:?} {env:?}"
                    );
                    assert_eq!(
                        update_check_disabled(&f),
                        expected,
                        "{machine:?} {user:?} {env:?}"
                    );
                }
            }
        }
        // HKLM=0, HKCU=1 → aktiv
        let f = Fixture {
            machine: Set(false),
            user: Set(true),
            env: Some("1"),
        };
        assert!(!crash_reports_disabled(&f));
        // Ungültiger Typ wird ignoriert, die nächste Quelle entscheidet.
        let f = Fixture {
            machine: Invalid,
            user: Missing,
            env: Some("1"),
        };
        assert_eq!(
            resolve_policy(&f, POLICY_DISABLE_CRASH_REPORTS, ENV_DISABLE_CRASH_REPORTS),
            Some((true, PolicyOrigin::Environment))
        );
        let f = Fixture {
            machine: Invalid,
            user: Invalid,
            env: Some("ja"),
        };
        assert_eq!(
            resolve_policy(&f, POLICY_DISABLE_CRASH_REPORTS, ENV_DISABLE_CRASH_REPORTS),
            None
        );
    }

    // spec: OUT-247
    #[test]
    fn environment_values() {
        assert_eq!(env_value(Some("1")), PolicyValue::Set(true));
        assert_eq!(env_value(Some(" TRUE ")), PolicyValue::Set(true));
        assert_eq!(env_value(Some("0")), PolicyValue::Set(false));
        assert_eq!(env_value(Some("")), PolicyValue::Missing);
        assert_eq!(env_value(None), PolicyValue::Missing);
        assert_eq!(env_value(Some("ja")), PolicyValue::Invalid);
    }
}

#[cfg(test)]
mod installer_tests {
    use super::*;

    const RELEASE: &str = include_str!("../../../../tests/fixtures/update/release-manifest.json");
    const PUBKEY: &str = include_str!("../../../../tests/fixtures/update/test.key.pub");

    fn release() -> UpdateManifest {
        serde_json::from_str(RELEASE).unwrap()
    }
    fn ctx(os: &str, arch: &str, installer: Installer) -> UpdateContext {
        UpdateContext {
            os: os.into(),
            arch: arch.into(),
            installer,
            pubkey: PUBKEY.trim().into(),
            disabled_by_environment: false,
            ..Default::default()
        }
    }
    fn selected(ctx: &UpdateContext, manifest: &UpdateManifest) -> Option<(String, String)> {
        ctx.select(manifest)
            .map(|(key, entry)| (key.to_owned(), entry.url.clone()))
    }

    // spec: OUT-262
    // spec: OUT-253
    #[test]
    fn detects_msi_nsis_and_unknown_from_the_embedded_bundle_marker() {
        // Gemockte Umgebung: die Kennung, die der Bundler je Paket in die EXE schreibt.
        assert_eq!(Installer::detect("windows", Some("msi")), Installer::Msi);
        assert_eq!(Installer::detect("windows", Some("MSI")), Installer::Msi);
        assert_eq!(Installer::detect("windows", Some("nsis")), Installer::Nsis);
        // Ungepatchter Build, fremde oder leere Kennung: nicht sicher bestimmbar.
        for marker in [
            None,
            Some(""),
            Some("app"),
            Some("deb"),
            Some("exe"),
            Some("msix"),
        ] {
            assert_eq!(
                Installer::detect("windows", marker),
                Installer::Unknown,
                "{marker:?}"
            );
        }
        // Ohne Installer-Varianten (macOS, Linux) spielt die Kennung keine Rolle.
        for marker in [None, Some("app"), Some("msi")] {
            assert_eq!(Installer::detect("darwin", marker), Installer::Platform);
            assert_eq!(Installer::detect("linux", marker), Installer::Platform);
        }
        // Die Desktop-App meldet die Kennung beim Start; UpdateContext::default() übernimmt sie.
        register_bundle_type(Some("msi"));
        let os = UpdateContext::default().os;
        assert_eq!(
            UpdateContext::default().installer,
            Installer::detect(&os, Some("msi"))
        );
    }

    // spec: OUT-262
    // spec: OUT-261
    #[test]
    fn selects_only_the_manifest_entry_of_the_own_installer() {
        let manifest = release();
        manifest.validate(false).unwrap();
        let base = "https://github.com/wojcikdominik/openspider/releases/download/v1.0.1/";
        let msi = ctx("windows", "x86_64", Installer::Msi);
        assert_eq!(msi.target().as_deref(), Some("windows-x86_64-msi"));
        assert_eq!(
            selected(&msi, &manifest),
            Some((
                "windows-x86_64-msi".into(),
                format!("{base}OpenSpider_1.0.1_x64_en-US.msi")
            ))
        );
        let nsis = ctx("windows", "x86_64", Installer::Nsis);
        assert_eq!(
            selected(&nsis, &manifest),
            Some((
                "windows-x86_64-nsis".into(),
                format!("{base}OpenSpider_1.0.1_x64-setup.exe")
            ))
        );
        let mac = ctx("darwin", "aarch64", Installer::Platform);
        assert_eq!(
            selected(&mac, &manifest),
            Some((
                "darwin-aarch64".into(),
                format!("{base}OpenSpider_aarch64.app.tar.gz")
            ))
        );
        // Kein Rückfall auf das andere Installer-Paket, wenn der eigene Eintrag fehlt.
        let mut only_nsis = release();
        only_nsis.platforms.remove("windows-x86_64-msi");
        assert_eq!(selected(&msi, &only_nsis), None);
        let mut only_msi = release();
        only_msi.platforms.remove("windows-x86_64-nsis");
        assert_eq!(selected(&nsis, &only_msi), None);
        // Unbekannte Installationsart: es wird kein Paket angefordert.
        let unknown = ctx("windows", "x86_64", Installer::Unknown);
        assert_eq!(unknown.target(), None);
        assert_eq!(selected(&unknown, &manifest), None);
        // Das CLI meldet nur Verfügbarkeit: irgendein Eintrag der eigenen Plattform genügt, eine fremde Plattform nicht.
        assert!(selected(
            &ctx("windows", "x86_64", Installer::AnyReportOnly),
            &only_msi
        )
        .is_some());
        assert!(selected(
            &ctx("windows", "aarch64", Installer::AnyReportOnly),
            &manifest
        )
        .is_none());
        assert!(selected(&ctx("linux", "x86_64", Installer::AnyReportOnly), &manifest).is_none());
        assert!(selected(
            &ctx("darwin", "x86_64", Installer::AnyReportOnly),
            &manifest
        )
        .is_some());
    }

    // spec: OUT-262
    #[test]
    fn unknown_installer_disables_the_check_before_any_request() {
        let unknown = ctx("windows", "x86_64", Installer::Unknown);
        assert_eq!(unknown.disabled(), Some(Disabled::UnknownInstaller));
        assert!(matches!(
            reserve(&unknown, true),
            Ok(Some(Outcome::Disabled(Disabled::UnknownInstaller)))
        ));
        // Keine Reservierung: es folgt keine Anfrage an den Update-Server.
        assert!(unknown.last_attempt.lock().unwrap().is_none());
        // Richtlinie und Einstellung gehen vor (OUT-253): ihr Hinweis bleibt sichtbar.
        let mut by_policy = ctx("windows", "x86_64", Installer::Unknown);
        by_policy.disabled_by_environment = true;
        assert_eq!(by_policy.disabled(), Some(Disabled::Environment));
        for installer in [
            Installer::Msi,
            Installer::Nsis,
            Installer::Platform,
            Installer::AnyReportOnly,
        ] {
            assert_eq!(ctx("windows", "x86_64", installer).disabled(), None);
        }
    }
}
