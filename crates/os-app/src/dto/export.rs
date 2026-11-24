//! Exporte, Berichte und Sitemaps (`export.*`, OUT-020 ff., OUT-030 ff., OUT-120 ff.).

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::common::Selection;
use super::view::ViewSpec;

/// Ausgabeformat.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum ExportFormatDto {
    /// CSV.
    Csv,
    /// Excel.
    Xlsx,
    /// JSON-Ausgabe.
    Json,
}

/// Ein Eintrag des Export-Katalogs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ExportItem {
    /// Kanonischer Name (Dateiname und Aufruf).
    pub name: String,
    /// i18n-Schlüssel des Anzeigenamens.
    pub label_key: String,
    /// Ob der Export im aktuellen Kontext verfügbar ist.
    pub enabled: bool,

    /// Grund für einen deaktivierten Export (i18n-Schlüssel).
    #[serde(default)]
    #[ts(optional = nullable)]
    pub disabled_reason: Option<String>,
}

/// Gruppe von Exporten im Menü.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ExportGroup {
    /// Gruppenname.
    pub key: String,
    /// i18n-Schlüssel der Gruppe.
    pub label_key: String,
    /// Einträge.
    pub items: Vec<ExportItem>,
}

/// Antwort von `export.catalog` (UI-002, OUT-030 ff.).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ExportCatalog {
    /// Bulk-Exporte.
    pub bulk: Vec<ExportGroup>,
    /// Berichte.
    pub reports: Vec<ExportGroup>,
    /// Sitemap-Arten.
    pub sitemaps: Vec<ExportItem>,
}

/// Ziel eines Exports (für `export.suggestName`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ExportTarget {
    /// Ansicht eines Tabs.
    #[serde(rename_all = "camelCase")]
    View {
        /// Tab.
        tab: String,
        /// Filter.
        filter: String,
        /// Format.
        format: ExportFormatDto,
    },
    /// Detailtabelle.
    #[serde(rename_all = "camelCase")]
    Detail {
        /// Beschreibung.
        spec: ViewSpec,
        /// Format.
        format: ExportFormatDto,
    },
    /// Bulk-Export.
    #[serde(rename_all = "camelCase")]
    Bulk {
        /// Name.
        name: String,
        /// Format.
        format: ExportFormatDto,
    },
    /// Bericht.
    #[serde(rename_all = "camelCase")]
    Report {
        /// Name.
        name: String,
        /// Format.
        format: ExportFormatDto,
    },
    /// Sitemap.
    #[serde(rename_all = "camelCase")]
    Sitemap {
        /// `pages` oder `images`.
        sitemap_kind: String,
    },
}

/// Argument von `export.suggestName`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SuggestNameArgs {
    /// Ziel.
    pub target: ExportTarget,
}

/// Argument von `export.view` (OUT-020).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ExportViewArgs {
    /// View.
    pub view_id: String,
    /// Version, die exportiert wird (Schnappschuss).
    pub version: u64,
    /// Zieldatei.
    pub path: String,
    /// Format.
    pub format: ExportFormatDto,
    /// Auch ausgeblendete Spalten exportieren.
    pub all_columns: bool,
    /// Für die Ausgabe ausgewählte Spalten.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub columns: Option<Vec<String>>,

    /// URL-Reihenfolge der hochgeladenen Liste beibehalten; Standard true.
    #[serde(default = "default_upload_order")]
    #[ts(as = "Option<bool>", optional)]
    pub preserve_upload_order: bool,
}

/// Argument von `export.detail` (OUT-021).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ExportDetailArgs {
    /// Beschreibung der Detailtabelle.
    pub spec: ViewSpec,
    /// Auswahl der Quell-URLs.
    pub selection: Selection,
    /// Zieldatei.
    pub path: String,
    /// Format.
    pub format: ExportFormatDto,
    /// Crawl, aus dessen Ansicht die Auswahl stammt (UI-018, fix-sel). Ist er gesetzt und nicht der Crawl der Sitzung,
    /// lehnt der Kern ab (`err-selection-other-crawl`) und schreibt keine Datei; fehlt er, gilt das bisherige Verhalten.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub crawl_id: Option<String>,
}

/// Argument von `export.bulk` und `export.report` (OUT-030 ff., OUT-080 ff.).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ExportBulkArgs {
    /// Name aus dem Katalog.
    pub name: String,
    /// Zielordner.
    pub dir: String,
    /// Format.
    pub format: ExportFormatDto,
    /// In einer Arbeitsmappe konsolidieren (OUT-007).
    pub single_workbook: bool,
}

/// Argument von `export.sitemap`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ExportSitemapArgs {
    /// `pages` oder `images`.
    pub sitemap_kind: String,
    /// Optionen: Inhalt von `os_model::SitemapGenConfig` als JSON (OUT-121 ff.); `null` = die der Konfiguration.
    #[ts(optional = nullable)]
    pub options: Option<serde_json::Value>,
    /// Zielordner.
    pub dir: String,
}

/// Seitenleisten-Inhalt für `export.sidebar`/`export.issues` (UI-060, UI-062).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum PanelKind {
    /// Overview.
    Overview,
    /// Issues.
    Issues,
    /// Site Structure.
    SiteStructure,
    /// Response Times.
    ResponseTimes,
    /// Segmente.
    Segments,
    /// API-Status.
    Api,
    /// Rechtschreib- und Grammatikbefunde.
    SpellingGrammar,
}

/// Argument von `export.issues` und `export.sidebar`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ExportPanelArgs {
    /// Welches Panel.
    pub which: PanelKind,
    /// Zieldatei.
    pub path: String,
    /// Format.
    pub format: ExportFormatDto,
}

fn default_upload_order() -> bool {
    true
}

/// Command-Vertrag ExportSel (M2.md 3.14.9).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ExportSel {
    /// Art des Eintrags.
    pub kind: String,

    /// Name des Eintrags.
    pub name: String,
}

/// Konsolidierter Sheets-Export (OUT-005); Schnittstelle für Dialog und Aufgaben.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ExportSheetsArgs {
    /// Verbundenes Drive-Konto.
    pub account: String,
    /// Tabellen aus dem gemeinsamen Exportkatalog.
    pub selections: Vec<ExportSel>,
    /// Name der Google-Tabelle.
    pub name: String,
}

/// Command-Vertrag ExportMultiArgs (M2.md 3.14.9).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ExportMultiArgs {
    /// Ausgewählte Exportaufträge.
    pub items: Vec<ExportSel>,

    /// Ausgabeformat der Exportdateien.
    pub format: ExportFormatDto,

    /// Zielverzeichnis der Exportdateien.
    pub dir: String,

    /// Alle Exporte in einer Arbeitsmappe zusammenfassen.
    pub single_workbook: bool,

    /// Exporte ohne Datenzeilen überspringen.
    pub skip_empty: bool,

    /// Benannte Exportvorlage.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub preset: Option<String>,
}

/// Command-Vertrag ExportBatchResult (M2.md 3.14.9).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ExportBatchResult {
    /// Pfade der erzeugten Dateien.
    pub files: Vec<super::common::FileResult>,

    /// Warnungen bei der Ausführung.
    pub warnings: Vec<String>,

    /// Übersprungene Exportaufträge.
    pub skipped: Vec<String>,
}

/// Command-Vertrag ExportPreset (M2.md 3.14.9).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ExportPreset {
    /// Name des Eintrags.
    pub name: String,

    /// Ausgewählte Exportaufträge.
    pub items: Vec<ExportSel>,

    /// Alle Exporte in einer Arbeitsmappe zusammenfassen.
    pub single_workbook: bool,

    /// Exporte ohne Datenzeilen überspringen.
    pub skip_empty: bool,
}

/// Command-Vertrag ExportPresetArgs (M2.md 3.14.9).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ExportPresetArgs {
    /// Name des Eintrags.
    pub name: String,

    /// Benannte Exportvorlage.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub preset: Option<ExportPreset>,
}

/// Command-Vertrag CustomSummaryArgs (M2.md 3.14.9).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CustomSummaryArgs {
    /// Kennung des verwendeten API-Kontos.
    pub account: String,

    /// Ziel der Zusammenfassung.
    pub target: String,

    /// Ausgewählte Kennzahlen der Zusammenfassung.
    pub metrics: Vec<String>,
}

/// Command-Vertrag CustomSummaryMetric (M2.md 3.14.9).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CustomSummaryMetric {
    /// Eindeutige Kennung.
    pub id: String,

    /// Übersetzungsschlüssel der Bezeichnung.
    pub label_key: String,
}
