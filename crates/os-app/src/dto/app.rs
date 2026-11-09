//! App, Einstellungen, UI-Zustand und Diagnose (`app.*`, `settings.*`, `uistate.*`, `diag.*`).

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::common::NameValue;

/// Wirksame Pfade (OUT-227, „Hilfe → Debug“).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(optional_fields = nullable)]
pub struct AppPaths {
    /// Einstellungen (`settings.toml`, `default.osconfig`, `ui-state.json`).
    pub settings: String,
    /// Crawl-Dateien.
    pub crawls: String,
    /// Logdateien.
    pub logs: String,
    /// Cache (Chromium, temporäre Exporte).
    pub cache: String,
    /// Wirksamer Exportordner (OUT-222).
    pub exports: String,
    /// Wert von `OPENSPIDER_HOME`, falls gesetzt.
    pub home_override: Option<String>,
}

/// Antwort von `app.info` (UI-127).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    /// Version, z. B. `0.1.0`.
    pub version: String,
    /// Build-Kennung (Commit, Datum).
    pub build: String,
    /// Build-Nummer (Anzahl der Commits bis zum gebauten Stand, UI-127); fehlt ohne Git und in Entwickler-Builds.
    #[ts(optional = nullable)]
    pub build_number: Option<String>,
    /// `macos` oder `windows` (Linux nur für den Kernbetrieb).
    pub platform: String,
    /// `aarch64` oder `x86_64`.
    pub arch: String,
    /// Pfade.
    pub paths: AppPaths,
}

/// Allgemeine Einstellungen (Spiegel von `os_model::settings::GeneralSettings`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct GeneralSettingsDto {
    /// `system`, `de` oder `en`.
    pub language: String,
    /// Systembenachrichtigungen.
    pub notifications: bool,
}

/// Erscheinungsbild (Spiegel von `AppearanceSettings`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AppearanceSettingsDto {
    /// `system`, `light` oder `dark`.
    pub theme: String,
    /// Akzentfarbe `#rrggbb`.
    pub accent: String,
    /// Zoom in Prozent.
    pub zoom_pct: u16,
    /// `compact` oder `normal`.
    pub row_density: String,
    /// Alternierende Zeilenfarben.
    pub zebra: bool,
    /// Live-Intervall in ms (250 – 5000).
    pub live_interval_ms: u32,
}

/// Speicher (Spiegel von `StorageSettings`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(optional_fields = nullable)]
pub struct StorageSettingsDto {
    /// Speicherort der Crawls.
    pub crawls_dir: Option<String>,
    /// `balanced`, `max_speed` oder `min_memory`.
    pub performance_profile: String,
    /// RAM-Obergrenze in GB.
    pub ram_limit_gb: Option<f64>,
    /// Obergrenze des Caches in Megabyte.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub cache_limit_mb: Option<u32>,

    /// Maximale Anzahl paralleler Rendering-Prozesse.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub render_process_limit: Option<u32>,
}

/// Aufbewahrung (Spiegel von `RetentionSettings`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(optional_fields = nullable)]
pub struct RetentionSettingsDto {
    /// `never`, `one_week`, `one_month`, `three_months`, `six_months`, `one_year` oder `custom`.
    pub policy: String,
    /// Tage bei `custom`.
    pub custom_days: u32,
    /// Höchstens N ungesperrte Crawls behalten.
    pub keep_last: Option<u32>,
}

/// Export-Standards (Spiegel von `ExportSettings`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(optional_fields = nullable)]
pub struct ExportSettingsDto {
    /// Exportordner; `null` = Standard.
    pub directory: Option<String>,
    /// `csv` oder `xlsx`.
    pub format: String,
}

/// Proxy (Spiegel von `ProxySettings`; das Kennwort liegt im Schlüsselbund).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(optional_fields = nullable)]
pub struct ProxySettingsDto {
    /// `system`, `none` oder `manual`.
    pub mode: String,
    /// `http`, `https` oder `socks5`.
    pub kind: String,
    /// Host.
    pub host: String,
    /// Port.
    pub port: u16,
    /// Benutzername.
    pub username: Option<String>,
    /// Ausnahmeliste.
    pub bypass: Vec<String>,
}

/// Netzwerk (Spiegel von `NetworkSettings`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct NetworkSettingsDto {
    /// Proxy.
    pub proxy: ProxySettingsDto,
    /// Vertrauenswürdige Zertifikate.
    pub trusted_certificates: Vec<String>,
}

/// Verweise auf Konten (Spiegel von `AccountSettings`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(optional_fields = nullable)]
pub struct AccountSettingsDto {
    /// Kennungen der E-Mail-Konten.
    pub mail_accounts: Vec<String>,
    /// Konto für Benachrichtigungen.
    pub notify_account: Option<String>,
    #[serde(default)]
    pub notify_to: Option<Vec<String>>,
    #[serde(default)]
    pub notify_on_success: Option<bool>,
    #[serde(default)]
    pub notify_on_failure: Option<bool>,
    /// Beim Start automatisch zu verbindende API-Quellen.
    pub auto_connect: Vec<String>,
}

/// MCP (Spiegel von `McpSettings`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct McpSettingsDto {
    /// Beim Start der App starten.
    pub autostart: bool,
    /// Gespeicherte Klassenfreigaben; fehlt der Wert, gilt nur `read` (AGENT-003).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub classes: Option<Vec<String>>,
    /// Konfigurierter HTTP-Port; ohne Wert wählt der Server einen Port (OUT-285).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub port: Option<u16>,
}

/// Antwort von `settings.get` / `settings.set`: App-Einstellungen (`settings.toml`, OUT-222).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AppSettingsDto {
    /// Allgemein.
    pub general: GeneralSettingsDto,
    /// Erscheinungsbild.
    pub appearance: AppearanceSettingsDto,
    /// Speicher.
    pub storage: StorageSettingsDto,
    /// Aufbewahrung.
    pub retention: RetentionSettingsDto,
    /// Export.
    pub export: ExportSettingsDto,
    /// Netzwerk.
    pub network: NetworkSettingsDto,
    /// Konten.
    pub accounts: AccountSettingsDto,
    /// MCP.
    pub mcp: McpSettingsDto,
    /// Schlüssel (TOML-Pfade), deren Änderung durch diesen Aufruf einen Neustart erfordert.
    pub restart_required: Vec<String>,
    /// Einstellungen für die Protokollierung.
    pub logging: LoggingSettingsDto,
    /// App-weite Update-Einstellungen (OUT-250).
    pub updates: super::update::UpdateSettingsDto,
    /// Absturzberichte (OUT-247).
    pub crash_reports: super::crash::CrashReportSettingsDto,
}

/// Argument von `settings.set`: JSON-Merge-Patch (RFC 7396) über `AppSettingsDto` in camelCase; `null` setzt einen
/// optionalen Wert zurück.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SettingsPatch {
    /// Der Patch.
    pub patch: serde_json::Value,
}

/// Versionierter UI-Zustand (`ui-state.json`, UI-125); der Kern speichert den Inhalt unverändert.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct UiStateJson {
    /// Schreibrevision aus load; save lehnt veraltete Revisionen ab.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional = nullable)]
    pub revision: Option<u64>,
    /// Schemaversion des Zustands.
    pub version: u32,
    /// Inhalt (Fenster, Trenner, Tabs, Spalten, Verlauf).
    pub state: serde_json::Value,
}

/// Antwort von `diag.debugInfo` (UI-127, CRAWL-129).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(optional_fields = nullable)]
pub struct DebugInfo {
    /// App.
    pub app: AppInfo,
    /// Betriebssystem und Version.
    pub os: String,
    /// Prozess-ID.
    pub pid: u32,
    /// RSS der Engine in Byte.
    pub rss_engine_bytes: Option<u64>,
    /// RSS aller Renderer-Prozesse in Byte.
    pub rss_renderer_bytes: Option<u64>,
    /// RAM-Obergrenze in Byte.
    pub ram_limit_bytes: u64,
    /// Größe der geöffneten Crawl-Datei.
    pub crawl_file_bytes: Option<u64>,
    /// Freier Speicherplatz am Speicherort.
    pub free_disk_bytes: Option<u64>,
    /// Aktives Leistungsprofil.
    pub performance_profile: String,
    /// Tatsächlich gesetzter SQLite-Cache in KiB.
    pub sqlite_cache_kib: Option<u64>,
    /// Weitere Name-Wert-Paare (Versionen der Bibliotheken, Chromium-Pfad).
    pub extra: Vec<NameValue>,
}

/// Argument von `diag.logFrontend`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(optional_fields = nullable)]
pub struct LogFrontendArgs {
    /// `error`, `warn`, `info` oder `debug`.
    pub level: String,
    /// Meldung.
    pub message: String,
    /// Kontext (Komponente, Stack), wird redigiert (OUT-243).
    pub context: Option<serde_json::Value>,
}

/// Argument von `diag.bundle`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct BundleArgs {
    /// Zieldatei (`.zip`).
    pub dest: String,
}

/// Zeitraum der Nutzungsstatistik.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum UsageRange {
    /// Letzte 30 Tage.
    Last30Days,
    /// Letzte 12 Monate.
    Last12Months,
    /// Alles.
    All,
}

/// Argument von `diag.usageStats`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct UsageStatsArgs {
    /// Zeitraum.
    pub range: UsageRange,
}

/// Nutzung je Monat.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct UsageBucket {
    /// Monat `YYYY-MM`.
    pub month: String,
    /// Anzahl Crawls.
    pub crawls: u32,
    /// Gecrawlte URLs.
    pub urls: u64,
    /// Dateigröße in Byte.
    pub bytes: u64,
}

/// Antwort von `diag.usageStats` (OUT-245): rein lokal, ohne Übertragung.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct UsageStats {
    /// Anzahl Crawls im Zeitraum.
    pub crawls: u32,
    /// Gecrawlte URLs.
    pub urls: u64,
    /// Gesamtgröße der Crawl-Dateien.
    pub bytes: u64,
    /// Verteilung je Monat.
    pub per_month: Vec<UsageBucket>,
}

/// Argument von `events.subscribe`: ein Kanal je Fenster.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SubscribeArgs {
    /// Fenstername (`main`, `settings`, `viz-<n>`).
    pub window: String,
}

/// Antwort von `events.subscribe`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SubscriptionId {
    /// Kennung des Abonnements.
    pub id: String,
}

/// Argument von `events.setVisibility` (UI-143: ≤ 1 Hz bei verdecktem Fenster).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SetVisibilityArgs {
    /// Fenstername.
    pub window: String,
    /// Sichtbar.
    pub visible: bool,
}

/// Argument von `events.setInterval` (UI-018: 250 – 5000 ms).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SetIntervalArgs {
    /// Intervall in ms.
    pub ms: u32,
}

/// Argument von `op.cancel`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct OpCancelArgs {
    /// Operation.
    pub op_id: String,
}

/// Command-Vertrag LoggingSettingsDto (M2.md 3.14.9).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct LoggingSettingsDto {
    /// Schweregrad der zu protokollierenden Meldungen.
    pub level: String,

    /// Aufbewahrungsdauer der Protokolle in Tagen.
    pub retention_days: u32,
}

/// Command-Vertrag BundlePreview (M2.md 3.14.9).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct BundlePreview {
    /// Pfade der erzeugten Dateien.
    pub files: Vec<String>,

    /// Warnungen bei der Ausführung.
    pub warnings: Vec<String>,
}

/// Command-Vertrag CertificateProbeArgs (M2.md 3.14.9).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CertificateProbeArgs {
    /// Zu prüfende URL.
    pub url: String,
}

/// Command-Vertrag CertificateChainDto (M2.md 3.14.9).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CertificateChainDto {
    /// Zertifikate der vom Server gelieferten Kette.
    pub certificates: Vec<String>,

    /// Ob die Zertifikatskette vertrauenswürdig ist.
    pub trusted: bool,
}

/// Command-Vertrag ProxyPasswordArgs (M2.md 3.14.9).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ProxyPasswordArgs {
    /// Passwort für den angefragten Vorgang; nur Eingabe.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub password: Option<String>,
}

/// Entscheidung beim Beenden; die Sitzungslogik liefert M2-K04.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum QuitDecision {
    /// Ohne weitere Rückfrage beenden.
    Quit,
    /// Aktive Arbeit erfordert eine Bestätigung durch den Benutzer.
    Confirm { reasons: Vec<QuitReason> },
}

/// Aktive Arbeit, die vor dem Beenden gesichert oder beendet werden muss (UI-001).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum QuitReason {
    /// Ein Crawl läuft.
    Crawl,
    /// Eine Crawl-Analyse läuft.
    Analysis,
    /// Ein Export läuft.
    Export,
    /// HTTP-MCP-Sitzungen sind aktiv.
    McpSessions,
}
