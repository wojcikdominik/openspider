//! App-Einstellungen (`settings.toml`, OUT-222) – getrennt von der Crawl-Konfiguration (`.osconfig`).
//!
//! Rechner- und benutzerbezogene Werte: Sprache, Theme, Speicherort, Ressourcenlimits, Proxy, Zertifikate,
//! Benachrichtigungen, Aufbewahrung, Export-Standards, Verweise auf Konten. Eine `.osconfig` enthält nie einen
//! dieser Werte (Test `settings_split`: die Schlüsselmengen beider Formate sind disjunkt). Geheimnisse stehen nie
//! hier, nur Kennungen für den Schlüsselbund (OUT-225).
//!
//! Das Einstellungsverzeichnis kommt als Parameter (OUT-227), damit `os-model` keine Benutzerpfade kennen muss. Die
//! Auflösung des Exportordners (`<Dokumente>/OpenSpider/Exports`, OUT-222) gehört in die Service-Schicht.

use std::fs;
use std::path::{Path, PathBuf};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::enums_m2::{
    ExportFormat, PerformanceProfile, ProxyKind, ProxyMode, RetentionPolicy, RowDensity, ThemeMode,
    UiLanguage,
};
use crate::error::ModelError;

/// Version des Dateiformats `settings.toml`.
pub const SETTINGS_SCHEMA_VERSION: u32 = 1;

/// Dateiname im Einstellungsordner (OUT-227).
pub const SETTINGS_FILE: &str = "settings.toml";

/// Erlaubte UI-Zoomstufen in Prozent (UI-120).
pub const ZOOM_STEPS: [u16; 6] = [80, 90, 100, 110, 125, 150];

/// Kleinstes und größtes Live-Intervall in Millisekunden (UI-018).
pub const LIVE_INTERVAL_RANGE_MS: (u32, u32) = (250, 5000);

/// Schlüssel (TOML-Pfade), deren Änderung einen Neustart der App erfordert (OUT-222, letztes Kriterium).
/// Alle übrigen Einstellungen wirken ohne Neustart, spätestens ab dem nächsten Crawl-Start.
pub const RESTART_REQUIRED_KEYS: &[&str] = &["storage.crawls_dir"];

/// Alle App-Einstellungen.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct AppSettings {
    /// Version des Dateiformats.
    pub schema_version: u32,
    /// Allgemein.
    pub general: GeneralSettings,
    /// Oberfläche (UI-120).
    pub appearance: AppearanceSettings,
    /// Speicher (CRAWL-124, CRAWL-129).
    pub storage: StorageSettings,
    /// Aufbewahrung von Crawls (OUT-229).
    pub retention: RetentionSettings,
    /// Export-Standards (OUT-222).
    pub export: ExportSettings,
    /// Netzwerk (CRAWL-122, OUT-269/270).
    pub network: NetworkSettings,
    /// Verweise auf verbundene Konten (nie Geheimnisse).
    pub accounts: AccountSettings,
    /// MCP-Server in der App (UI-129).
    pub mcp: McpSettings,
    /// Automatische Update-Prüfung (OUT-250), ausschließlich app-weit.
    pub updates: UpdateSettings,
    /// Logging und Aufbewahrung.
    pub logging: LoggingSettings,
    /// Absturzberichte (OUT-247, M3-C01 additiv nach K2-61/K2-92).
    pub crash_reports: CrashReportSettings,
}

impl Default for AppSettings {
    fn default() -> Self {
        AppSettings {
            schema_version: SETTINGS_SCHEMA_VERSION,
            general: GeneralSettings::default(),
            appearance: AppearanceSettings::default(),
            storage: StorageSettings::default(),
            retention: RetentionSettings::default(),
            export: ExportSettings::default(),
            network: NetworkSettings::default(),
            accounts: AccountSettings::default(),
            mcp: McpSettings::default(),
            logging: LoggingSettings::default(),
            updates: UpdateSettings::default(),
            crash_reports: CrashReportSettings::default(),
        }
    }
}

/// Allgemeine Einstellungen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct GeneralSettings {
    /// Oberflächensprache; `system` folgt der Systemsprache (UI-121).
    pub language: UiLanguage,
    /// Systembenachrichtigung bei Crawl-Ende und Fehlern.
    pub notifications: bool,
}

impl Default for GeneralSettings {
    fn default() -> Self {
        GeneralSettings {
            language: UiLanguage::System,
            notifications: true,
        }
    }
}

/// Erscheinungsbild (UI-018, UI-120).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct AppearanceSettings {
    /// Theme (Standard System).
    pub theme: ThemeMode,
    /// Akzentfarbe als `#rrggbb`.
    pub accent: String,
    /// UI-Zoom in Prozent (80, 90, 100, 110, 125, 150).
    pub zoom_pct: u16,
    /// Zeilenhöhe der Datenansichten.
    pub row_density: RowDensity,
    /// Alternierende Zeilenfarben.
    pub zebra: bool,
    /// Live-Intervall der Oberfläche in Millisekunden (250 – 5000).
    pub live_interval_ms: u32,
}

impl Default for AppearanceSettings {
    fn default() -> Self {
        AppearanceSettings {
            theme: ThemeMode::System,
            accent: "#2f855a".to_string(),
            zoom_pct: 100,
            row_density: RowDensity::Compact,
            zebra: true,
            live_interval_ms: 500,
        }
    }
}

/// Speicher (CRAWL-124, CRAWL-129).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(default)]
pub struct StorageSettings {
    /// Speicherort der Crawls; nicht gesetzt = Standard laut OUT-227.
    pub crawls_dir: Option<String>,
    /// Cache-Leistungsprofil.
    pub performance_profile: PerformanceProfile,
    /// RAM-Obergrenze in GB mit einer Nachkommastelle; nicht gesetzt = 50 % des physischen RAM, begrenzt auf
    /// [1 GB, physischer RAM − 2 GB] (CRAWL-129).
    pub ram_limit_gb: Option<f64>,
    /// None = max(512 MiB, 25 % RAM).
    pub cache_limit_mb: Option<u32>,
    /// None = Kerne/2, höchstens 8.
    pub render_process_limit: Option<u32>,
}

/// Aufbewahrung (OUT-229).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct RetentionSettings {
    /// Richtlinie (Standard: nie löschen).
    pub policy: RetentionPolicy,
    /// Tage bei `policy = "custom"`.
    pub custom_days: u32,
    /// Höchstens N ungesperrte Crawls behalten ([P2]); nicht gesetzt = unbegrenzt.
    pub keep_last: Option<u32>,
}

impl Default for RetentionSettings {
    fn default() -> Self {
        RetentionSettings {
            policy: RetentionPolicy::Never,
            custom_days: 30,
            keep_last: None,
        }
    }
}

/// Export-Standards (OUT-222).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(default)]
pub struct ExportSettings {
    /// Standard-Exportordner (`export.directory`); nicht gesetzt = `<Dokumente>/OpenSpider/Exports`.
    pub directory: Option<String>,
    /// Standardformat.
    pub format: ExportFormat,
}

/// Proxy (CRAWL-122): genau ein manueller Proxy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct ProxySettings {
    /// Modus (Standard: Systemproxy).
    pub mode: ProxyMode,
    /// Typ bei `mode = "manual"`.
    pub kind: ProxyKind,
    /// Host bei `mode = "manual"`.
    pub host: String,
    /// Port bei `mode = "manual"`.
    pub port: u16,
    /// Benutzername; das Kennwort liegt im Schlüsselbund (`openspider/proxy/password`, OUT-225).
    pub username: Option<String>,
    /// Ausnahmeliste (Hosts, die nicht über den Proxy laufen).
    pub bypass: Vec<String>,
}

impl Default for ProxySettings {
    fn default() -> Self {
        ProxySettings {
            mode: ProxyMode::System,
            kind: ProxyKind::Http,
            host: String::new(),
            port: 8080,
            username: None,
            bypass: Vec::new(),
        }
    }
}

/// Netzwerk (App-weit; nicht Teil des Crawl-Profils).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(default)]
pub struct NetworkSettings {
    /// Proxy.
    pub proxy: ProxySettings,
    /// Vertrauenswürdige Zertifikate (Pfade zu `.crt`, `.pem`, `.cer`, `.der`; OUT-270).
    pub trusted_certificates: Vec<String>,
}

/// Verweise auf Konten; die Zugangsdaten liegen im Schlüsselbund.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct AccountSettings {
    /// Kennungen der E-Mail-Konten (OUT-280).
    pub mail_accounts: Vec<String>,
    /// Konto für Benachrichtigungen bei Crawl-Ende; nicht gesetzt = keine Mail.
    pub notify_account: Option<String>,
    /// Global completion recipients (OUT-281); empty disables mail.
    #[serde(default)]
    pub notify_to: Vec<String>,
    #[serde(default)]
    pub notify_on_success: bool,
    #[serde(default = "notify_failure_default")]
    pub notify_on_failure: bool,
    /// API-Quellen, die beim Start automatisch verbunden werden (INTEG-011), als Quellen-Kennung (`ga4`, `gsc`, …).
    pub auto_connect: Vec<String>,
}

impl Default for AccountSettings {
    fn default() -> Self {
        Self {
            mail_accounts: vec![],
            notify_account: None,
            auto_connect: vec![],
            notify_to: vec![],
            notify_on_success: false,
            notify_on_failure: true,
        }
    }
}

fn notify_failure_default() -> bool {
    true
}

/// MCP in der App.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(default)]
pub struct McpSettings {
    /// MCP-Server beim Start der App starten.
    pub autostart: bool,
}

/// App-weite Logeinstellungen (OUT-241/242).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct LoggingSettings {
    pub level: String,
    pub retention_days: u32,
}
impl Default for LoggingSettings {
    fn default() -> Self {
        Self {
            level: "info".into(),
            retention_days: 30,
        }
    }
}

/// Eine Meldung der Validierung mit TOML-Pfad.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsIssue {
    /// Pfad, z. B. `appearance.live_interval_ms`.
    pub path: String,
    /// Beschreibung.
    pub message: String,
}

impl AppSettings {
    /// Liest `settings.toml`; fehlende Schlüssel erhalten Standardwerte, unbekannte werden ignoriert.
    pub fn from_toml_str(s: &str) -> Result<Self, ModelError> {
        toml::from_str(s.strip_prefix('\u{feff}').unwrap_or(s))
            .map_err(|e| ModelError::Config(format!("settings.toml: {e}")))
    }

    /// Schreibt alle Werte in stabiler Reihenfolge (Schreiben → Laden → Schreiben ist byte-identisch).
    pub fn to_toml_string(&self) -> Result<String, ModelError> {
        let mut stamped = self.clone();
        stamped.schema_version = SETTINGS_SCHEMA_VERSION;
        toml::to_string_pretty(&stamped)
            .map_err(|e| ModelError::Config(format!("settings.toml: {e}")))
    }

    /// Pfad der Datei im Einstellungsordner.
    pub fn path_in(settings_dir: &Path) -> PathBuf {
        settings_dir.join(SETTINGS_FILE)
    }

    /// Lädt `<settings_dir>/settings.toml`; ohne Datei die Standardwerte.
    pub fn load(settings_dir: &Path) -> Result<Self, ModelError> {
        let path = Self::path_in(settings_dir);
        match fs::read_to_string(&path) {
            Ok(text) => Self::from_toml_str(&text),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(ModelError::Config(format!("{}: {e}", path.display()))),
        }
    }

    /// Schreibt die Datei atomar (temporäre Datei, dann Umbenennen).
    pub fn save(&self, settings_dir: &Path) -> Result<(), ModelError> {
        let path = Self::path_in(settings_dir);
        let fail = |what: &str, e: std::io::Error| {
            ModelError::Config(format!("{}: {what}: {e}", path.display()))
        };
        fs::create_dir_all(settings_dir).map_err(|e| fail("Ordner nicht anlegbar", e))?;
        let text = self.to_toml_string()?;
        let tmp = path.with_file_name(format!("{SETTINGS_FILE}.tmp{}", std::process::id()));
        fs::write(&tmp, text.as_bytes()).map_err(|e| fail("nicht schreibbar", e))?;
        fs::rename(&tmp, &path).map_err(|e| {
            let _ = fs::remove_file(&tmp);
            fail("nicht ersetzbar", e)
        })
    }

    /// Prüft Wertebereiche (UI-018: 250 – 5000 ms, UI-120: Zoomstufen und Akzentfarbe, CRAWL-129: RAM-Obergrenze).
    pub fn validate(&self) -> Vec<SettingsIssue> {
        let mut out = Vec::new();
        let mut bad = |path: &str, message: String| {
            out.push(SettingsIssue {
                path: path.to_string(),
                message,
            })
        };
        let (lo, hi) = LIVE_INTERVAL_RANGE_MS;
        if !(lo..=hi).contains(&self.appearance.live_interval_ms) {
            bad(
                "appearance.live_interval_ms",
                format!("muss zwischen {lo} und {hi} ms liegen"),
            );
        }
        if !ZOOM_STEPS.contains(&self.appearance.zoom_pct) {
            bad(
                "appearance.zoom_pct",
                format!("muss eine der Stufen {ZOOM_STEPS:?} sein"),
            );
        }
        let a = &self.appearance.accent;
        if !(a.len() == 7 && a.starts_with('#') && a[1..].chars().all(|c| c.is_ascii_hexdigit())) {
            bad(
                "appearance.accent",
                "Farbe als #rrggbb erwartet".to_string(),
            );
        }
        if let Some(gb) = self.storage.ram_limit_gb {
            if !gb.is_finite() || gb < 1.0 {
                bad("storage.ram_limit_gb", "mindestens 1.0 GB".to_string());
            }
        }
        if self.retention.policy == RetentionPolicy::Custom && self.retention.custom_days == 0 {
            bad("retention.custom_days", "mindestens 1 Tag".to_string());
        }
        if self.retention.keep_last == Some(0) {
            bad("retention.keep_last", "mindestens 1".to_string());
        }
        if !["error", "warn", "info", "debug", "trace"].contains(&self.logging.level.as_str()) {
            bad("logging.level", "ungültige Logstufe".into());
        }
        if !(1..=365).contains(&self.logging.retention_days) {
            bad("logging.retention_days", "1 bis 365 Tage".into());
        }
        if self.storage.cache_limit_mb.is_some_and(|v| v < 256) {
            bad("storage.cache_limit_mb", "mindestens 256 MiB".into());
        }
        if self
            .storage
            .render_process_limit
            .is_some_and(|v| !(1..=32).contains(&v))
        {
            bad("storage.render_process_limit", "1 bis 32".into());
        }
        let p = &self.network.proxy;
        if p.mode == ProxyMode::Manual {
            if p.host.trim().is_empty() {
                bad(
                    "network.proxy.host",
                    "bei mode = \"manual\" erforderlich".to_string(),
                );
            }
            if p.port == 0 {
                bad("network.proxy.port", "1 bis 65535 erwartet".to_string());
            }
        }
        out
    }

    /// Welche der Schlüssel in `changed` (TOML-Pfade) erfordern einen Neustart ([`RESTART_REQUIRED_KEYS`])?
    pub fn restart_required<'a>(changed: impl IntoIterator<Item = &'a str>) -> Vec<&'a str> {
        changed
            .into_iter()
            .filter(|k| RESTART_REQUIRED_KEYS.contains(k))
            .collect()
    }

    /// TOML-Pfade (Blätter) aller Werte, die sich zwischen `self` und `other` unterscheiden, sortiert.
    pub fn changed_keys(&self, other: &AppSettings) -> Vec<String> {
        fn leaves(
            prefix: &str,
            v: &serde_json::Value,
            out: &mut std::collections::BTreeMap<String, serde_json::Value>,
        ) {
            match v {
                serde_json::Value::Object(map) => {
                    for (k, child) in map {
                        let path = if prefix.is_empty() {
                            k.clone()
                        } else {
                            format!("{prefix}.{k}")
                        };
                        leaves(&path, child, out);
                    }
                }
                other => {
                    out.insert(prefix.to_string(), other.clone());
                }
            }
        }
        let (mut a, mut b) = (Default::default(), Default::default());
        leaves("", &serde_json::to_value(self).unwrap_or_default(), &mut a);
        leaves("", &serde_json::to_value(other).unwrap_or_default(), &mut b);
        let keys: std::collections::BTreeSet<_> = a.keys().chain(b.keys()).cloned().collect();
        keys.into_iter().filter(|k| a.get(k) != b.get(k)).collect()
    }
}

/// Absturzberichte (OUT-247): nach einem Absturz beim Start fragen, ob ein Bericht gesendet werden soll.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct CrashReportSettings {
    /// Hinweis beim Start zeigen (Standard: an). Aus: Berichte bleiben lokal und sind über das Hilfe-Menü sendbar.
    pub prompt: bool,
}
impl Default for CrashReportSettings {
    fn default() -> Self {
        Self { prompt: true }
    }
}

/// Update-Einstellungen (OUT-250, OUT-222); nie Teil eines Konfigurationsprofils.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct UpdateSettings {
    pub check: bool,
    /// Letzte Prüfung als ISO-8601-UTC-Zeitstempel; None vor der ersten Prüfung.
    pub last_check: Option<String>,
    /// OUT-251: version skipped in the update dialog; automatic checks do not report it again.
    pub skipped_version: Option<String>,
}
impl Default for UpdateSettings {
    fn default() -> Self {
        Self {
            check: true,
            last_check: None,
            skipped_version: None,
        }
    }
}
