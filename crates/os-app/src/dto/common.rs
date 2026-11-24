//! Gemeinsame DTOs: Operationen, Auswahl, Validierung, kleine Argumenttypen (`M2-ui-concept.md` 4.3/4.4).
//!
//! Schlüssel und Kennungen: `UrlId` ist eine JSON-Zahl (< 2^53), `CrawlId` ein String (UUID v7), Zeitstempel sind
//! ISO 8601 UTC, Bytes und Dauern Rohzahlen (die Formatierung übernimmt die Oberfläche).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::error::CmdError;

/// Sofortige Antwort eines lang laufenden Commands (`long_running`); das Ergebnis kommt über `Tick.ops[].result`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct OpHandle {
    /// Kennung der Operation (für `op.cancel`).
    pub op_id: String,
    /// i18n-Schlüssel der Bezeichnung.
    pub label_key: String,
    /// Lässt sich abbrechen.
    pub cancellable: bool,
}

/// Zustand einer Operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum OpState {
    /// Läuft.
    Running,
    /// Erfolgreich beendet; das Ergebnis steht in `result`.
    Done,
    /// Mit Fehler beendet; siehe `error`.
    Failed,
    /// Abgebrochen.
    Cancelled,
}

/// Fortschritt einer Operation im Tick.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(optional_fields = nullable)]
pub struct OpStatus {
    /// Kennung.
    pub op_id: String,
    /// i18n-Schlüssel der Bezeichnung.
    pub label_key: String,
    /// Bisher erledigt.
    pub done: u64,
    /// Gesamtumfang, falls bekannt.
    pub total: Option<u64>,
    /// Zustand.
    pub state: OpState,
    /// Ergebnis (Typ laut Katalog `ret` des Commands), nur bei `Done`.
    pub result: Option<serde_json::Value>,
    /// Fehler, nur bei `Failed`.
    pub error: Option<CmdError>,
}

/// Auswahl von URLs: explizite Kennungen oder „alle der View außer …“ (UI-012, UI-114).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Selection {
    /// Genau diese URLs.
    #[serde(rename_all = "camelCase")]
    Ids {
        /// URL-Kennungen.
        ids: Vec<i64>,
    },
    /// Alle Zeilen der View in dieser Version, außer den genannten.
    #[serde(rename_all = "camelCase")]
    All {
        /// View.
        view_id: String,
        /// Version der View.
        version: u64,
        /// Ausnahmen.
        except: Vec<i64>,
    },
}

/// Ein Befund der Validierung (`config.validate`, `segment.validate`, `tasks.validate`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(optional_fields = nullable)]
pub struct ValidationIssue {
    /// Pfad des Feldes (TOML-Pfad bzw. Feldname).
    pub path: String,
    /// i18n-Schlüssel der Meldung.
    pub message_key: String,
    /// Argumente der Meldung.
    pub args: BTreeMap<String, serde_json::Value>,
    /// Zeile (bei Dateien).
    pub line: Option<u32>,
    /// Spalte (bei Dateien).
    pub column: Option<u32>,
    /// `true` = Warnung (wird angenommen), sonst Fehler.
    pub warning: bool,
}

/// Name und Wert (Detail „URL Details“, Reihenfolge UI-031).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(optional_fields = nullable)]
pub struct NameValue {
    /// Kanon-Name der Spalte.
    pub name: String,
    /// Wert (`null` = leer).
    pub value: Option<serde_json::Value>,
}

/// Ein HTTP-Header.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Header {
    /// Name.
    pub name: String,
    /// Wert (redigiert, OUT-243).
    pub value: String,
}

/// Argument: genau eine URL.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct UrlIdArgs {
    /// Image preview only: user explicitly confirmed a live download.
    #[serde(default)]
    #[ts(optional)]
    pub allow_live: Option<bool>,
    /// URL-Kennung.
    pub url_id: i64,
}

/// Argument: eine Auswahl.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SelectionArgs {
    /// Auswahl.
    pub selection: Selection,
    /// Crawl, aus dessen Ansicht die Auswahl stammt (UI-018, CRAWL-131, fix-sel). Ist er gesetzt und nicht der Crawl der
    /// Sitzung, lehnt der Kern ab (`err-selection-other-crawl`), ohne etwas zu ändern; fehlt er, gilt das bisherige
    /// Verhalten.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub crawl_id: Option<String>,
}

/// Argument: ein Name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct NameArgs {
    /// Name.
    pub name: String,
}

/// Argument: ein Pfad.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PathArgs {
    /// Dateipfad (absolut, vom nativen Dialog).
    pub path: String,
}

/// Argument: eine Kennung.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct IdArgs {
    /// Kennung.
    pub id: String,
}

/// Argument: mehrere Kennungen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct IdsArgs {
    /// Kennungen.
    pub ids: Vec<String>,
}

/// Argument: ein Wahrheitswert.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct BoolArgs {
    /// Wert.
    pub value: bool,
}

/// Argument: Seitenabruf mit Offset und Limit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PageArgs {
    /// Offset.
    pub offset: u64,
    /// Höchstzahl Zeilen.
    pub limit: u32,
}

/// Ergebnis mit Anzahl (Operationen auf mehreren Objekten).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CountResult {
    /// Anzahl betroffener Objekte.
    pub count: u64,
}

/// Ergebnis einer Dateiausgabe.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct FileResult {
    /// Geschriebene Datei (oder Ordner).
    pub path: String,
    /// Größe in Byte (bei Ordnern Summe).
    pub bytes: u64,
}
