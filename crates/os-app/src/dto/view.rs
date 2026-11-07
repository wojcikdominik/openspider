//! Views, Suche, Spalten und Zeilenfenster (`M2-ui-concept.md` 4.3, `catalog.*`, `view.*`, `search.*`).

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::common::{OpHandle, Selection};

/// Sortierrichtung.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum SortDir {
    /// Aufsteigend.
    Asc,
    /// Absteigend.
    Desc,
}

/// Ein Sortierschlüssel (Kanon-Name der Spalte, UI-010).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SortKey {
    /// Kanon-Name der Spalte.
    pub column: String,
    /// Richtung.
    pub dir: SortDir,
}

/// Verknüpfung in einem Regelbaum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum RuleBool {
    /// UND.
    And,
    /// ODER.
    Or,
}

/// Operator einer Suchbedingung (UI-011).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum RuleOp {
    /// enthält
    Contains,
    /// enthält nicht
    NotContains,
    /// ist gleich
    Equals,
    /// ist ungleich
    NotEquals,
    /// beginnt mit
    StartsWith,
    /// endet mit
    EndsWith,
    /// entspricht Regex
    Regex,
    /// entspricht nicht Regex
    NotRegex,
    /// größer als
    GreaterThan,
    /// kleiner als
    LessThan,
    /// ist leer
    IsEmpty,
    /// ist nicht leer
    IsNotEmpty,
}

/// Knoten des Suchregelbaums (UI-011); die Textsyntax übersetzt `search.parse`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum RuleNode {
    /// Gruppe mit UND/ODER (mindestens zwei Ebenen verschachtelbar).
    #[serde(rename_all = "camelCase")]
    Group {
        /// Verknüpfung.
        op: RuleBool,
        /// Kinder.
        children: Vec<RuleNode>,
    },
    /// Bedingung auf einer Spalte.
    #[serde(rename_all = "camelCase")]
    Cond {
        /// Kanon-Name der Spalte; `null` = alle sichtbaren Spalten.
        column: Option<String>,
        /// Operator.
        op: RuleOp,
        /// Wert (bei `IsEmpty`/`IsNotEmpty` ohne Bedeutung).
        value: Option<String>,
    },
}

/// Suche in der oberen Tabelle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SearchSpec {
    /// Klartext-Teilstring in „Address“.
    #[serde(rename_all = "camelCase")]
    Simple {
        /// Suchtext.
        text: String,
    },
    /// Regelbaum.
    #[serde(rename_all = "camelCase")]
    Rules {
        /// Baum.
        tree: RuleNode,
        /// Groß-/Kleinschreibung beachten.
        case_sensitive: bool,
    },
}

/// Segmenteinschränkung einer View (UI-020).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SegmentFilter {
    /// Nur dieses Segment.
    #[serde(rename_all = "camelCase")]
    Segment {
        /// Name des Segments.
        name: String,
    },
    /// Nur URLs ohne Segment.
    None,
}

/// Klassifikation im Vergleichsmodus (OUT-142 ff., TECH-307).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum CompareClass {
    /// Neu im Filter.
    Added,
    /// Neue URL.
    New,
    /// Aus dem Filter entfernt.
    Removed,
    /// URL fehlt im aktuellen Crawl.
    Missing,
    /// Geändert (Change Detection).
    Changed,
    /// Nicht geändert (Change Detection).
    NotChanged,
}

/// Filter der Inlinks-/Outlinks-Ansicht (kanonische Kennung, z. B. `All`, `Hyperlinks`, `Images`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct LinkFilter {
    /// Kennung des Filters.
    pub id: String,
    /// Filter für die Position des Links im Dokument.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub position: Option<String>,

    /// Links auf dieselbe URL ausschließen.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub exclude_self: Option<bool>,

    /// Links mit nofollow ausschließen.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub exclude_nofollow: Option<bool>,

    /// Filter für den Ankertext des Links.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub anchor: Option<String>,

    /// Ausgewählter Auswertungsmodus.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub mode: Option<LinkMode>,
}

/// Filter der Ressourcen-Ansicht (kanonische Kennung).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ResourceFilter {
    /// Kennung des Filters.
    pub id: String,
}

/// Tabellarische Detail-Tabs (laufen über `view.*` mit `ViewSpec::Detail`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum DetailTable {
    /// HTTP-Header.
    Headers,
    /// Cookies.
    Cookies,
    /// Chrome-Konsolenlog.
    ConsoleLog,
    /// Beim Rendern blockierte Ressourcen.
    BlockedResources,
    /// Netzwerkanfragen des Browsers.
    Network,
    /// Rechtschreib- und Grammatikbefunde.
    Spelling,
    /// Accessibility-Regeln.
    A11yRules,
    /// Accessibility-Knoten.
    A11yNodes,
    /// Duplikat-Partner.
    Duplicates,
    /// N-Gramme des Inhalts.
    NGrams,
    /// Strukturierte Daten: Elemente.
    StructuredDataItems,
    /// Strukturierte Daten: Befunde.
    StructuredDataIssues,
    /// Weiterleitungskette.
    RedirectChain,
    /// Sitemap-Zugehörigkeit.
    SitemapMembership,
    /// Custom-Search-Treffer.
    CustomSearch,
    /// Custom-Extraction-Werte.
    CustomExtraction,
    /// Custom-JavaScript-Werte.
    CustomJs,
    /// Werte der API-Quellen.
    ApiValues,
    /// Formulare.
    Forms,
    /// Hreflang-Verknüpfungen.
    Hreflang,
    /// Pfad der Entdeckung im Crawl.
    CrawlPath,
    /// Ankertexte der Links.
    Anchors,
}

/// Beschreibung einer View (`view.open`, `view.update`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(optional_fields = nullable)]
pub enum ViewSpec {
    /// Tab der oberen Tabelle mit Filter, Suche, Sortierung.
    #[serde(rename_all = "camelCase")]
    Tab {
        /// Tab-Kennung.
        tab: String,
        /// Filter-Kennung.
        filter: String,
        /// Suche.
        search: Option<SearchSpec>,
        /// Sortierung.
        sort: Vec<SortKey>,
        /// Segment.
        segment: Option<SegmentFilter>,
        /// Vergleichsklasse.
        compare: Option<CompareClass>,
    },
    /// Inlinks der gewählten URLs.
    #[serde(rename_all = "camelCase")]
    Inlinks {
        /// URLs.
        url_ids: Vec<i64>,
        /// Filter.
        filter: LinkFilter,
        /// Suchtext.
        search: Option<String>,
        /// Sortierung.
        sort: Vec<SortKey>,
        /// Vergleichsklasse.
        compare: Option<CompareClass>,
    },
    /// Outlinks der gewählten URLs.
    #[serde(rename_all = "camelCase")]
    Outlinks {
        /// URLs.
        url_ids: Vec<i64>,
        /// Filter.
        filter: LinkFilter,
        /// Suchtext.
        search: Option<String>,
        /// Sortierung.
        sort: Vec<SortKey>,
        /// Vergleichsklasse.
        compare: Option<CompareClass>,
    },
    /// Ressourcen der gewählten URLs.
    #[serde(rename_all = "camelCase")]
    Resources {
        /// URLs.
        url_ids: Vec<i64>,
        /// Filter.
        filter: ResourceFilter,
        /// Suchtext.
        search: Option<String>,
        /// Sortierung.
        sort: Vec<SortKey>,
    },
    /// Bilder der gewählten URLs.
    #[serde(rename_all = "camelCase")]
    Images {
        /// URLs.
        url_ids: Vec<i64>,
        /// Suchtext.
        search: Option<String>,
        /// Sortierung.
        sort: Vec<SortKey>,
    },
    /// Tabellarischer Detail-Tab.
    #[serde(rename_all = "camelCase")]
    Detail {
        /// Welche Tabelle.
        detail: DetailTable,
        /// URLs.
        url_ids: Vec<i64>,
        /// Parameter der Tabelle (je Tabelle definiert).
        params: serde_json::Value,
        /// Sortierung.
        sort: Vec<SortKey>,
    },
}

/// Zustand einer View.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum ViewState {
    /// Zeilen abrufbar.
    Ready,
    /// Wird aufgebaut (`op` meldet den Fortschritt).
    Building,
}

/// Datentyp einer Spalte (Sortierung und Formatierung in der Oberfläche).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ColumnKind {
    /// Text (Kollation `OS_TEXT`).
    Text,
    /// URL (bytewise sortiert).
    Url,
    /// Ganzzahl.
    Int,
    /// Dezimalzahl.
    #[serde(rename_all = "camelCase")]
    Float {
        /// Nachkommastellen.
        decimals: u8,
    },
    /// Wahrheitswert.
    Bool,
    /// Zeitstempel.
    Timestamp,
    /// Hash.
    Hash,
    /// Sekunden mit drei Nachkommastellen.
    Seconds3,
}

/// Ausrichtung einer Spalte.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum Align {
    /// Links.
    Left,
    /// Rechts.
    Right,
    /// Zentriert.
    Center,
}

/// Metadaten einer Spalte (`catalog.columns`, `ViewHandle.columns`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(optional_fields = nullable)]
pub struct ColumnMeta {
    /// Kanon-Name.
    pub key: String,
    /// Datentyp.
    pub kind: ColumnKind,
    /// Standardmäßig sichtbar.
    pub default_visible: bool,
    /// Standardbreite in Pixeln (bei 100 % Zoom).
    pub default_width: u16,
    /// Ausrichtung.
    pub align: Align,
    /// Sortierbar.
    pub sortable: bool,
    /// Operatoren, die für die Suche in dieser Spalte zulässig sind.
    pub searchable: Vec<RuleOp>,
    /// `[OS-Erweiterung]`.
    pub extension: bool,
    /// Voraussetzung, die fehlt (dann ist die Spalte leer).
    pub needs: Option<EmptyState>,
}

/// Code eines Leer-/Voraussetzungszustands (UI-016), in der Rangfolge der Spec.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EmptyCode {
    /// Detail-Tab ohne Auswahl.
    NoSelection,
    /// Im aktuellen Modus nicht verfügbar.
    ModeUnsupported,
    /// Braucht den JavaScript-Modus.
    NeedsJsRendering,
    /// Erhebungsoption aus (`arg` = Schlüssel).
    FeatureDisabled,
    /// Quelltext nicht gespeichert.
    HtmlNotStored,
    /// Gerenderter Quelltext nicht gespeichert.
    RenderedHtmlNotStored,
    /// Datenquelle nicht verbunden (`arg` = Quelle).
    ApiNotConnected,
    /// Analyse-Teilschritt fehlt (`arg` = Schritt).
    NeedsCrawlAnalysis,
    /// Noch keine Daten.
    NoDataYet,
    /// Filter/Suche ohne Treffer.
    NoMatch,
    /// Daten existieren für diesen URL-Typ nicht.
    NotApplicable,
}

/// Aktion der Hinweisfläche.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum EmptyAction {
    /// Einstellungsdialog mit fokussiertem Ziel öffnen.
    #[serde(rename_all = "camelCase")]
    OpenSetting {
        /// Anker, z. B. `rendering.mode`.
        anchor: String,
    },
    /// Crawl-Analyse starten.
    StartAnalysis,
    /// Suche aufheben.
    ClearSearch,
    /// Verbindungsdialog einer API-Quelle öffnen.
    #[serde(rename_all = "camelCase")]
    ConnectApi {
        /// Quelle (`ga4`, `gsc`, …).
        api: String,
    },
}

/// Leer-/Voraussetzungszustand einer Tabelle (UI-016, `data-empty-state`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(optional_fields = nullable)]
pub struct EmptyState {
    /// Code.
    pub code: EmptyCode,
    /// Argument (Schlüssel, Quelle oder Teilschritt).
    pub arg: Option<String>,
    /// Aktionsschaltfläche.
    pub action: Option<EmptyAction>,
}

/// Antwort auf `view.open` / `view.update`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(optional_fields = nullable)]
pub struct ViewHandle {
    /// Kennung der View.
    pub view_id: String,
    /// Version (= `change_seq` des Standes).
    pub version: u64,
    /// Zeilenzahl; `null` = wird berechnet.
    pub total: Option<u64>,
    /// Spalten in Anzeigereihenfolge.
    pub columns: Vec<ColumnMeta>,
    /// Zustand.
    pub state: ViewState,
    /// Aufbau-Operation bei `Building`.
    pub op: Option<OpHandle>,
    /// Leerzustand, falls die View strukturell leer ist.
    pub empty_state: Option<EmptyState>,
    /// Optionales erstes, begrenztes Zeilenfenster ohne zweiten IPC-Umlauf (UI-145).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional = nullable)]
    pub initial_rows: Option<RowWindow>,
}

/// Ein Zeilenfenster (`view.rows`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct RowWindow {
    /// View.
    pub view_id: String,
    /// Version, für die die Zeilen gelten.
    pub version: u64,
    /// Index der ersten Zeile.
    pub offset: u64,
    /// Zeilenschlüssel (URL-Kennung bzw. Kennung der Detailzeile).
    pub row_keys: Vec<i64>,
    /// Kanon-Namen der gelieferten Spalten.
    pub columns: Vec<String>,
    /// Zellen je Zeile (`null` = leer).
    #[ts(type = "Array<Array<string | number | boolean | null>>")]
    pub cells: Vec<Vec<serde_json::Value>>,
}

/// Argumente von `view.open`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ViewOpenArgs {
    /// Beschreibung.
    pub spec: ViewSpec,
    /// Gewünschte Spalten (Kanon-Namen) in Reihenfolge.
    pub columns: Vec<String>,
}

/// Argumente von `view.update`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ViewUpdateArgs {
    /// View.
    pub view_id: String,
    /// Neue Beschreibung.
    pub spec: ViewSpec,
}

/// Argumente von `view.rows` (höchstens 500 Zeilen).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct RowsArgs {
    /// View.
    pub view_id: String,
    /// Version, die der Aufrufer kennt; bei Abweichung `Stale`.
    pub version: u64,
    /// Index der ersten Zeile.
    pub offset: u64,
    /// Anzahl Zeilen (≤ 500).
    pub limit: u32,
    /// Gewünschte Spalten.
    pub columns: Vec<String>,
}

/// Argumente von `view.locate`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct LocateArgs {
    /// View.
    pub view_id: String,
    /// URL.
    pub url_id: i64,
}

/// Argumente von `view.range`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct RangeArgs {
    /// View.
    pub view_id: String,
    /// Zeilenkennung am Anker: URL-ID in URL-Views, Link-ID bei materialisierten Linklisten.
    pub from: i64,
    /// Zeilenkennung am Ziel: URL-ID in URL-Views, Link-ID bei materialisierten Linklisten.
    pub to: i64,
}

/// Antwort auf `view.range`: der Bereich zwischen zwei Zeilenkennungen aus genau einem Stand der View (UI-012, UI-018).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct KeyRange {
    /// View.
    pub view_id: String,
    /// Stand der View, aus dem der Bereich stammt.
    pub version: u64,
    /// Zeile des Ankers; `None`, wenn die Kennung nicht (mehr) in der View ist.
    pub from_row: Option<u64>,
    /// Zeile des Ziels; `None`, wenn die Kennung nicht (mehr) in der View ist.
    pub to_row: Option<u64>,
    /// Zeilen der View in diesem Stand.
    pub total: u64,
    /// Zeilenkennungen (bei Linklisten Link-IDs) in Tabellenreihenfolge; höchstens 20 000; leer bei fehlendem Endpunkt, `all` oder `exportRequired`.
    pub keys: Vec<i64>,
    /// Der Bereich umfasst eine große View vollständig: Auswahl „alle“ statt einzelner Kennungen.
    pub all: bool,
    /// Teilbereich über 20 000 Zeilen: keine Kennungsliste; Oberfläche zeigt einen Export-Hinweis (UI-012).
    pub export_required: bool,
}

/// Argumente von `view.copy`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CopyArgs {
    /// View.
    pub view_id: String,
    /// Zeilen.
    pub selection: Selection,
    /// Spalten (sichtbare, in Anzeigereihenfolge).
    pub columns: Vec<String>,
    /// Kopfzeile mitkopieren.
    pub with_header: bool,
}

/// Ergebnis von `view.copy` (Zwischenablage wird im Kern befüllt).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CopyResult {
    /// Kopierte Zeilen.
    pub rows: u64,
    /// Kopierte Bytes.
    pub bytes: u64,
}

/// Argumente von `view.close`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ViewCloseArgs {
    /// View.
    pub view_id: String,
}

/// Argumente von `search.parse`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SearchParseArgs {
    /// Textsyntax, z. B. `Address ~ "/blog/" AND "Word Count" < 200`.
    pub text: String,
}

/// Argumente von `search.validateRegex`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ValidateRegexArgs {
    /// Regulärer Ausdruck.
    pub pattern: String,
}

/// Vertrag M2-F05b.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum LinkMode {
    Links,
    Anchors,
}

/// Stand der einmaligen Overview-/Issues-Zählung eines Crawls (UI-141, UI-145).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CountStatus {
    /// Die Zählung läuft.
    pub running: bool,
    /// Gezählte Tabs.
    pub done: u64,
    /// Alle Tabs (0, solange unbekannt).
    pub total: u64,
}
