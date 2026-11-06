//! Crawl-Verwaltung und Projekte (`crawls.*`, `projects.*`, OUT-228).

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::crawl::CrawlSummaryDto;
use super::view::SortDir;

/// Filter der Crawl-Liste.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum CrawlShow {
    /// Alle.
    All,
    /// Nur laufende.
    Running,
    /// Nur abgeschlossene.
    Completed,
    /// Nur unterbrochene.
    Incomplete,
    /// Nur gesperrte.
    Locked,
    /// Crawl-Dateien anzeigen.
    Crawls,
    /// Vergleichsdateien anzeigen.
    Comparisons,
}

/// Sortierung der Crawl-Liste.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CrawlSort {
    /// Spalte (`name`, `startedAt`, `urls`, `fileBytes`, `project`).
    pub column: String,
    /// Richtung.
    pub dir: SortDir,
}

/// Argument von `crawls.list` (höchstens 1 000 Zeilen, aus dem Index, < 500 ms).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(optional_fields = nullable)]
pub struct CrawlListQuery {
    /// Welche anzeigen.
    pub show: CrawlShow,
    /// Suchtext (Name, Start-URL).
    pub search: Option<String>,
    /// Sortierung.
    pub sort: Option<CrawlSort>,
    /// Nur dieses Projekt.
    pub project: Option<String>,
}

/// Eine Zeile der Crawl-Verwaltung.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(optional_fields = nullable)]
pub struct CrawlListing {
    /// Zusammenfassung des Crawls.
    pub crawl: CrawlSummaryDto,
    /// Projekt.
    pub project: Option<String>,
    /// Gesperrt (von der Aufbewahrung ausgenommen).
    pub locked: bool,
    /// Start-URL des Crawls.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub start_url: Option<String>,

    /// Bisherige Laufzeit in Millisekunden.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub duration_ms: Option<u64>,

    /// Ob der Crawl fortgesetzt werden kann.
    pub can_resume: bool,

    /// Endgrund (`queue_empty`, `limit_reached`, `stopped`) eines beendeten Crawls; die Verwaltung zeigt bei `stopped`
    /// „Abgeschlossen (gestoppt)“ (CRAWL-128). Additiv: ältere Aufrufer ignorieren das Feld.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub end_reason: Option<String>,
}

/// Argument von `crawls.rename`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CrawlRenameArgs {
    /// Crawl.
    pub crawl_id: String,
    /// Neuer Name.
    pub name: String,
}

/// Argument von `crawls.move`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(optional_fields = nullable)]
pub struct CrawlMoveArgs {
    /// Crawls.
    pub ids: Vec<String>,
    /// Zielprojekt; `null` = ohne Projekt.
    pub project: Option<String>,
    /// Zielverzeichnis für die Crawl-Datei.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub destination_dir: Option<String>,
}

/// Argument von `crawls.export`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CrawlsExportArgs {
    /// Crawls.
    pub ids: Vec<String>,
    /// Zielordner bzw. -datei.
    pub dest: String,
}

/// Argument von `crawls.import`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CrawlsImportArgs {
    /// Dateien (`.ospider`).
    pub paths: Vec<String>,
}

/// Ergebnis von `crawls.import`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    /// Aufgenommene Crawls.
    pub crawl_ids: Vec<String>,
    /// Abgewiesene Dateien mit Grund (i18n-Schlüssel).
    pub rejected: Vec<ImportRejected>,
}

/// Eine abgewiesene Importdatei.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ImportRejected {
    /// Datei.
    pub path: String,
    /// i18n-Schlüssel des Grundes.
    pub reason_key: String,
}

/// Ein Projekt der Crawl-Verwaltung.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ProjectInfo {
    /// Kennung.
    pub id: String,
    /// Name.
    pub name: String,
    /// Gesperrt (gilt auch für künftige und geplante Crawls).
    pub locked: bool,
    /// Anzahl Crawls.
    pub crawls: u32,
}

/// Argument von `projects.create`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ProjectCreateArgs {
    /// Name.
    pub name: String,
}

/// Argument von `projects.rename`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ProjectRenameArgs {
    /// Projekt.
    pub id: String,
    /// Neuer Name.
    pub name: String,
}

/// Argument von `projects.lock`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ProjectLockArgs {
    /// Projekt.
    pub id: String,
    /// Sperren oder entsperren.
    pub locked: bool,
}
