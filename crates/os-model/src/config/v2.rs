//! Neue und erweiterte Abschnitte der Konfiguration v2 (M2-F01, `docs/plan/M2.md` 3.3, OUT-226).
//!
//! Alle Standardwerte stammen aus der jeweiligen Spec-Zeile (Verweis an jedem Abschnitt). Geheimnisse stehen nie
//! hier: Zugangsdaten liegen im Schlüsselbund (OUT-225), die Konfiguration verweist nur per Kennung darauf.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::enums_m2::{
    ApiLevel, ChangeElement, ChangeFreq, ChangeFreqBasis, DateRangePreset, ExtractorMode,
    ExtractorOutput, HttpVersion, JsSnippetKind, LastmodSource, PsiDevice, ScreenshotFormat,
    SearchKind, SearchScope, SearchType, SegmentOp, SpellingMode,
};

// --- Rendering (CRAWL-078 … 084, PERF-006) -------------------------------------------------------------------

/// Fenstergröße und Gerät des Browsers (CRAWL-081). Standard: Googlebot Mobil 411 × 731.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct ViewportConfig {
    /// Voreinstellung (`googlebot_mobile`, `googlebot_desktop`, Gerätename oder `custom`).
    pub preset: String,
    /// Breite in CSS-Pixeln.
    pub width: u32,
    /// Höhe in CSS-Pixeln.
    pub height: u32,
    /// Skalierungsfaktor (Device Pixel Ratio).
    pub scale_factor: f64,
    /// Mobil-Flag.
    pub mobile: bool,
    /// Touch-Flag.
    pub touch: bool,
}

impl Default for ViewportConfig {
    fn default() -> Self {
        ViewportConfig {
            preset: "googlebot_mobile".to_string(),
            width: 411,
            height: 731,
            scale_factor: 1.0,
            mobile: true,
            touch: true,
        }
    }
}

/// Gerenderte Seiten-Screenshots (CRAWL-082).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct ScreenshotConfig {
    /// Screenshots aufnehmen (wirksam nur im JavaScript-Modus; OS-Default: an).
    pub enabled: bool,
    /// Breite in Pixeln; nicht gesetzt = Breite des Viewports.
    pub width: Option<u32>,
    /// Höhe in Pixeln; nicht gesetzt = Höhe des Viewports.
    pub height: Option<u32>,
    /// Höhe an den Inhalt anpassen (höchstens 8 192 px).
    pub fit_to_content: bool,
    /// Wartezeit nach der Größenänderung in ms (0–10 000).
    pub wait_ms: u32,
    /// Bildformat.
    pub format: ScreenshotFormat,
    /// JPEG-Qualität (1–100).
    pub jpeg_quality: u8,
}

impl Default for ScreenshotConfig {
    fn default() -> Self {
        ScreenshotConfig {
            enabled: true,
            width: None,
            height: None,
            fit_to_content: false,
            wait_ms: 300,
            format: ScreenshotFormat::Png,
            jpeg_quality: 80,
        }
    }
}

// --- Extraktion (CRAWL-055, 061 … 065, TECH-180 ff., TECH-250) ----------------------------------------------

/// Strukturierte Daten (CRAWL-065): Formate und Validierung. Alle Schalter sind OS-Defaults und standardmäßig aus.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct StructuredDataConfig {
    /// JSON-LD extrahieren.
    pub json_ld: bool,
    /// Microdata extrahieren.
    pub microdata: bool,
    /// RDFa extrahieren.
    pub rdfa: bool,
    /// Validierung gegen schema.org.
    pub schema_org_validation: bool,
    /// Validierung gegen die Google-Rich-Result-Regeln.
    pub rich_results_validation: bool,
    /// Groß-/Kleinschreibung der Typen und Eigenschaften beachten.
    pub case_sensitive: bool,
}

// --- Erweiterte Einstellungen -------------------------------------------------------------------------------

/// Lesbarkeit (ONPAGE-149, ONPAGE-152).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(default)]
pub struct ReadabilityConfig {
    /// Formeln für weitere Sprachen als Englisch verwenden (`content.readability.language_aware`, Standard aus).
    pub language_aware: bool,
}

// --- Rechtschreibung und Grammatik (ONPAGE-153/154) ---------------------------------------------------------

/// Rechtschreibprüfung (ONPAGE-153).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct SpellingConfig {
    /// Prüfung einschalten (Standard aus).
    pub enabled: bool,
    /// Sprache automatisch erkennen oder fest vorgeben.
    pub mode: SpellingMode,
    /// Feste Sprache bei `mode = "manual"`, z. B. `de-DE`.
    pub manual_language: String,
    /// Abbildung Sprache → Variante, z. B. `en = "en-US"`.
    pub default_variants: BTreeMap<String, String>,
    /// Sprache, wenn die Erkennung scheitert.
    pub default_language: String,
}

impl Default for SpellingConfig {
    fn default() -> Self {
        SpellingConfig {
            enabled: false,
            mode: SpellingMode::Auto,
            manual_language: "en-US".to_string(),
            default_variants: [
                ("en", "en-US"),
                ("de", "de-DE"),
                ("fr", "fr-FR"),
                ("es", "es-ES"),
                ("it", "it-IT"),
            ]
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
            default_language: "en".to_string(),
        }
    }
}

/// Grammatikprüfung (ONPAGE-154).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(default)]
pub struct GrammarConfig {
    /// Prüfung einschalten (Standard aus).
    pub enabled: bool,
    /// Abgeschaltete Regel-Kennungen.
    pub disabled_rules: Vec<String>,
}

// --- Benutzerdefiniert (04 B – D, INTEG-020 ff.) ------------------------------------------------------------

/// Ein Custom-Search-Filter (INTEG-020 … 024).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct SearchFilter {
    /// Name (Spaltenkopf); leer = `Filter <n>`.
    pub name: String,
    /// `true` = „enthält“, `false` = „enthält nicht“.
    pub contains: bool,
    /// Abfragetyp.
    pub kind: SearchKind,
    /// Suchausdruck (darf mehrzeilig sein).
    pub expression: String,
    /// Suchbereich.
    pub scope: SearchScope,
    /// XPath bei `scope = "xpath"`.
    pub xpath: Option<String>,
    /// Groß-/Kleinschreibung beachten (nur Typ Text).
    pub case_sensitive: bool,
}

impl Default for SearchFilter {
    fn default() -> Self {
        SearchFilter {
            name: String::new(),
            contains: true,
            kind: SearchKind::Text,
            expression: String::new(),
            scope: SearchScope::Html,
            xpath: None,
            case_sensitive: false,
        }
    }
}

/// Custom Search: bis zu 100 Filter (INTEG-020).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct CustomSearchConfig {
    /// Filter in Spaltenreihenfolge.
    pub filters: Vec<SearchFilter>,
    /// Zeitbudget je Regex (INTEG-028).
    pub regex_timeout_ms: u32,
}

/// Ein Extraktor (INTEG-040 … 043).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(default)]
pub struct Extractor {
    /// Name (Spaltenkopf); leer = `Extractor <n>`.
    pub name: String,
    /// Modus.
    pub mode: ExtractorMode,
    /// Ausdruck (XPath, CSS-Selektor oder Regex).
    pub expression: String,
    /// Attribut (nur CSS).
    pub attribute: Option<String>,
    /// Ausgabeart (nur XPath/CSS).
    pub output: ExtractorOutput,
}

/// Custom Extraction: bis zu 100 Extraktoren (INTEG-040).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(default)]
pub struct CustomExtractionConfig {
    /// Extraktoren in Spaltenreihenfolge.
    pub extractors: Vec<Extractor>,
}

/// Ein JavaScript-Snippet (INTEG-060 … 062).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct JsSnippet {
    /// Name (Spaltenkopf); leer = `Snippet <n>`.
    pub name: String,
    /// Art.
    pub kind: JsSnippetKind,
    /// Inhaltstypen (MIME ohne Parameter; `*` = alle außer PDF).
    pub content_types: Vec<String>,
    /// Zeitlimit in Sekunden (Aktion: Pflicht, 1–120).
    pub timeout_s: u32,
    /// JavaScript-Code.
    pub code: String,
}

impl Default for JsSnippet {
    fn default() -> Self {
        JsSnippet {
            name: String::new(),
            kind: JsSnippetKind::Extraction,
            content_types: vec!["text/html".to_string()],
            timeout_s: 0,
            code: String::new(),
        }
    }
}

/// Custom JavaScript: bis zu 100 Snippets (INTEG-060).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(default)]
pub struct CustomJsConfig {
    /// Snippets in Ausführungs- und Spaltenreihenfolge.
    pub snippets: Vec<JsSnippet>,
    /// Freigegebenes Ausgabeverzeichnis für Dateizugriffe der Snippets.
    pub output_dir: Option<std::path::PathBuf>,
}

/// Eine Regel der Link-Positions-Klassifikation (INTEG-080 … 082).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(default)]
pub struct LinkPositionRule {
    /// Positionsname, z. B. `Navigation`.
    pub position: String,
    /// Teilstrings des gespeicherten Pfades (case-sensitiv); der leere Teilstring passt immer.
    pub contains: Vec<String>,
}

/// Link-Positionen: geordnete Regelliste, die erste passende gewinnt (INTEG-081: Standardregeln).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct LinkPositionsConfig {
    /// „Link-Positionen speichern“ (INTEG-083, Standard: an). Aus: Linkzeilen erhalten weder `Link Path` noch
    /// `Link Position`; das spart Platz in der Crawl-Datei, eine spätere Neuzuordnung (INTEG-082) hat dann keine
    /// gespeicherten Pfade.
    pub store: bool,
    /// Regeln von oben nach unten; `Content` (leerer Teilstring) steht als Auffangregel zuletzt.
    pub rules: Vec<LinkPositionRule>,
}

impl Default for LinkPositionsConfig {
    fn default() -> Self {
        let rule = |position: &str, contains: &[&str]| LinkPositionRule {
            position: position.to_string(),
            contains: contains.iter().map(|s| s.to_string()).collect(),
        };
        LinkPositionsConfig {
            store: true,
            rules: vec![
                rule("Navigation", &["nav", "menu"]),
                rule("Header", &["header"]),
                rule("Aside", &["aside"]),
                rule("Sidebar", &["sidebar"]),
                rule("Footer", &["footer"]),
                rule("Content", &[]),
            ],
        }
    }
}

// --- URL-Umschreibung und CDNs (CRAWL-103/104/108) ----------------------------------------------------------

/// Eine Regex-Ersetzung (CRAWL-104).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(default)]
pub struct RegexReplace {
    /// Regulärer Ausdruck (Suche).
    pub pattern: String,
    /// Ersetzung mit `$1`-Gruppen.
    pub replacement: String,
}

/// URL-Umschreibung (CRAWL-103/104).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(default)]
pub struct RewriteConfig {
    /// Query-Parameter, die entfernt werden (exakter Name, case-sensitiv).
    pub remove_parameters: Vec<String>,
    /// Gesamten Query-String entfernen.
    pub remove_all_parameters: bool,
    /// Regex-Ersetzungen in Reihenfolge.
    pub regex_replace: Vec<RegexReplace>,
}

// --- HTTP (CRAWL-018/023/024/096/097/115/116) --------------------------------------------------------------

/// Ein eigener Request-Header (CRAWL-115).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(default)]
pub struct HeaderEntry {
    /// Name.
    pub name: String,
    /// Wert.
    pub value: String,
    /// Nur für diesen Host senden; nicht gesetzt = alle Hosts der Crawl-Domain (extern: nie).
    pub host: Option<String>,
}

/// HTTP-Verhalten. Proxy und vertrauenswürdige Zertifikate sind App-Einstellungen (CRAWL-122, OUT-222).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct HttpConfig {
    /// Eigene Request-Header.
    pub headers: Vec<HeaderEntry>,
    /// `Accept-Language`, z. B. `de-DE,de;q=0.9`.
    pub accept_language: Option<String>,
    /// HTTP-Version (CRAWL-024).
    pub version: HttpVersion,
    /// Wiederholungen bei 5xx (CRAWL-096, OS-Default 0).
    pub retries_5xx: u32,
    /// Wiederholungen bei Verbindungsfehlern (CRAWL-097, OS-Default 1).
    pub retries_network: u32,
    /// Bei 429 automatisch drosseln (CRAWL-097, OS-Default an).
    pub throttle_on_429: bool,
    /// HSTS-Richtlinie respektieren (CRAWL-018, Standard an).
    pub respect_hsts: bool,
    /// Zertifikatsfehler ignorieren (CRAWL-023, OS-Erweiterung, Standard aus).
    pub ignore_certificate_errors: bool,
}

impl Default for HttpConfig {
    fn default() -> Self {
        HttpConfig {
            headers: Vec::new(),
            accept_language: None,
            version: HttpVersion::Auto,
            retries_5xx: 0,
            retries_network: 1,
            throttle_on_429: true,
            respect_hsts: true,
            ignore_certificate_errors: false,
        }
    }
}

// --- Authentifizierung (CRAWL-118/120/121, OUT-223) --------------------------------------------------------

/// Ein formularbasierter Login (CRAWL-120). Cookies und Speicherinhalte liegen verschlüsselt im Profil.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(default)]
pub struct FormLogin {
    /// Login-URL.
    pub login_url: String,
    /// Name des Authentifizierungsprofils (`auth.listProfiles`), das die Sitzung hält.
    pub profile: Option<String>,
    /// Zeitpunkt der Anmeldung (ISO 8601 UTC).
    pub captured_at: Option<String>,
    /// Anzahl gespeicherter Cookies.
    pub cookie_count: u32,
}

// --- Sitemap-Generator (OUT-121 … 128) ---------------------------------------------------------------------

/// `lastmod` im Sitemap-Generator (OUT-123).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct SitemapLastmod {
    /// `lastmod` aufnehmen.
    pub enabled: bool,
    /// Quelle.
    pub source: LastmodSource,
    /// Datum bei `source = "custom"` (`YYYY-MM-DD`).
    pub custom_date: Option<String>,
}

impl Default for SitemapLastmod {
    fn default() -> Self {
        SitemapLastmod {
            enabled: true,
            source: LastmodSource::Server,
            custom_date: None,
        }
    }
}

/// `priority` im Sitemap-Generator (OUT-124).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct SitemapPriority {
    /// `priority` aufnehmen.
    pub enabled: bool,
    /// Wert je Tiefe (Index = Tiefe, der letzte Wert gilt für alle größeren Tiefen), 0.0 – 1.0.
    pub by_depth: Vec<f64>,
}

impl Default for SitemapPriority {
    fn default() -> Self {
        SitemapPriority {
            enabled: true,
            by_depth: vec![1.0, 0.9, 0.8, 0.7, 0.6, 0.5],
        }
    }
}

/// `changefreq` im Sitemap-Generator (OUT-125).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct SitemapChangefreq {
    /// `changefreq` aufnehmen.
    pub enabled: bool,
    /// Basis.
    pub basis: ChangeFreqBasis,
    /// Wert je Tiefe (Index = Tiefe, der letzte Wert gilt für alle größeren Tiefen).
    pub by_depth: Vec<ChangeFreq>,
}

impl Default for SitemapChangefreq {
    fn default() -> Self {
        SitemapChangefreq {
            enabled: true,
            basis: ChangeFreqBasis::LastModified,
            by_depth: vec![
                ChangeFreq::Daily,
                ChangeFreq::Daily,
                ChangeFreq::Weekly,
                ChangeFreq::Weekly,
                ChangeFreq::Monthly,
            ],
        }
    }
}

/// Bilder im Sitemap-Generator (OUT-126/127).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(default)]
pub struct SitemapImages {
    /// Bilder aufnehmen (Standard aus).
    pub enabled: bool,
    /// Regex für externe Bilder (CDN); leer = keine externen.
    pub external_regex: Vec<String>,
    /// Nur Bilder mit höchstens so vielen IMG-Inlinks (inklusive Obergrenze); nicht gesetzt = deaktiviert.
    pub max_inlinks: Option<u32>,
    /// noindex-Bilder ausschließen ([P2]).
    pub exclude_noindex: bool,
}

/// hreflang im Sitemap-Generator (OUT-128).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct SitemapHreflang {
    /// hreflang aufnehmen (Standard aus).
    pub enabled: bool,
    /// Nur Ziele mit Status 200 und indexierbar (Standard an).
    pub only_valid_targets: bool,
}

impl Default for SitemapHreflang {
    fn default() -> Self {
        SitemapHreflang {
            enabled: false,
            only_valid_targets: true,
        }
    }
}

/// Sitemap-Generator (OUT-120 … 128): alle Einschlussoptionen standardmäßig aus.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(default)]
pub struct SitemapGenConfig {
    /// Noindex-Seiten einschließen.
    pub include_noindex: bool,
    /// Kanonisierte Seiten einschließen.
    pub include_canonicalised: bool,
    /// Paginierte URLs (`rel=prev`) einschließen.
    pub include_paginated: bool,
    /// PDFs einschließen.
    pub include_pdfs: bool,
    /// 3xx einschließen.
    pub include_3xx: bool,
    /// 4xx einschließen.
    pub include_4xx: bool,
    /// 5xx einschließen.
    pub include_5xx: bool,
    /// Keine Antwort einschließen.
    pub include_no_response: bool,
    /// Ausgeschlossene URLs (Regex, eine pro Eintrag).
    pub exclude_patterns: Vec<String>,
    /// `lastmod`.
    pub lastmod: SitemapLastmod,
    /// `priority`.
    pub priority: SitemapPriority,
    /// `changefreq`.
    pub changefreq: SitemapChangefreq,
    /// Bilder.
    pub images: SitemapImages,
    /// hreflang.
    pub hreflang: SitemapHreflang,
}

// --- Vergleich (OUT-146, TECH-300) -------------------------------------------------------------------------

/// Eine Regel des URL-Mappings zwischen Crawls (OUT-146): Suche, nicht Vollübereinstimmung.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(default)]
pub struct MappingRule {
    /// Suchmuster (Regex).
    pub pattern: String,
    /// Ersetzung mit `$1`-Gruppen.
    pub replacement: String,
}

/// Change Detection (TECH-300): Elementauswahl in Filterreihenfolge und Schwelle für Inhaltsänderungen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct ChangeDetectionConfig {
    /// Aktive Elemente; Standard: alle außer `content` (Elemente 1 – 13, 15, 16; TECH-300).
    pub elements: Vec<ChangeElement>,
    /// Schwelle für Inhaltsänderungen in Prozent (0 – 100, Standard 10).
    pub content_threshold_pct: u8,
}

impl Default for ChangeDetectionConfig {
    fn default() -> Self {
        ChangeDetectionConfig {
            elements: ChangeElement::ALL
                .iter()
                .copied()
                .filter(|e| *e != ChangeElement::Content)
                .collect(),
            content_threshold_pct: 10,
        }
    }
}

/// Vergleich zweier Crawls (OUT-140 ff.).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(default)]
pub struct CompareConfig {
    /// URL-Mapping, nacheinander angewendet.
    pub mapping: Vec<MappingRule>,
    /// Change Detection.
    pub change_detection: ChangeDetectionConfig,
}

// --- Segmente (CRAWL-134) ----------------------------------------------------------------------------------

/// Eine Bedingung einer Segmentregel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(default)]
pub struct SegmentCondition {
    /// Feld (beliebige Spalte des Crawls, Kanon-Name).
    pub field: String,
    /// Operator.
    pub op: SegmentOp,
    /// Wert (leer bei Operatoren ohne Wert).
    pub value: String,
}

/// Eine Regelgruppe: Bedingungen UND-verknüpft.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(default)]
pub struct SegmentGroup {
    /// Bedingungen der Gruppe.
    pub conditions: Vec<SegmentCondition>,
}

/// Ein Segment: benannt, farbig, Gruppen ODER-verknüpft; die Listenreihenfolge ist die Priorität.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(default)]
pub struct SegmentConfig {
    /// Name (1 – 64 Zeichen, ohne Beachtung der Groß-/Kleinschreibung eindeutig).
    pub name: String,
    /// Farbe als `#rrggbb`.
    pub color: String,
    /// Regelgruppen.
    pub groups: Vec<SegmentGroup>,
}

/// Segmente einer Konfiguration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(default)]
pub struct SegmentsConfig {
    /// Segmente in Prüfreihenfolge.
    pub list: Vec<SegmentConfig>,
}

// --- API-Quellen (04 A – J, nie Geheimnisse) ----------------------------------------------------------------

/// Zeitraum einer Abfrage (GA4, GSC).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(default)]
pub struct DateRangeConfig {
    /// Voreinstellung.
    pub preset: DateRangePreset,
    /// Beginn bei `custom` (`YYYY-MM-DD`).
    pub from: Option<String>,
    /// Ende bei `custom` (`YYYY-MM-DD`).
    pub to: Option<String>,
}

/// Google Analytics 4 (INTEG-0xx, M2-I02).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(default)]
pub struct Ga4Config {
    /// Selected GA4 Analytics account (OUT-201), distinct from the OAuth account.
    pub analytics_account: Option<String>,
    /// Crawl same-host URLs discovered in this API (INTEG-015); disabled by default.
    pub crawl_new_urls: bool,
    /// Quelle beim Crawl abfragen.
    pub enabled: bool,
    /// Alternative Basis-URL (nur Debug-/Testbuilds oder `--allow-api-override`, INTEG-019).
    pub base_url: Option<String>,
    /// Property-Kennung.
    pub property: Option<String>,
    /// Zeitraum.
    pub date_range: DateRangeConfig,
    /// Metriken.
    pub metrics: Vec<String>,
    /// Seitendimension (z. B. `landingPage`).
    pub page_dimension: Option<String>,
    /// Dimensionsfilter als Ausdruck.
    pub filter: Option<String>,
    /// Konto-Kennung ohne Geheimnis (INTEG-003).
    pub account: Option<String>,
    /// Anfragegrenzen (INTEG-005/006).
    pub limits: super::ApiLimitsConfig,
    /// OAuth-Client-Kennung des Nutzers (INTEG-003).
    pub oauth_client_id: Option<String>,
    /// URL-Abgleich (INTEG-014).
    pub url_match: super::UrlMatchConfig,
}

/// Google Search Console (M2-I03).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(default)]
pub struct GscConfig {
    /// Crawl same-host URLs discovered in this API (INTEG-015); disabled by default.
    pub crawl_new_urls: bool,
    /// Quelle beim Crawl abfragen.
    pub enabled: bool,
    /// Alternative Basis-URL (INTEG-019).
    pub base_url: Option<String>,
    /// Website-Kennung der Search Console.
    pub site: Option<String>,
    /// Zeitraum.
    pub date_range: DateRangeConfig,
    /// Suchtyp.
    pub search_type: SearchType,
    /// Dimensionsfilter als Ausdruck.
    pub filter: Option<String>,
    /// URL Inspection abfragen (Tageskontingent).
    pub url_inspection: bool,
    /// Konto-Kennung ohne Geheimnis (INTEG-003).
    pub account: Option<String>,
    /// Anfragegrenzen (INTEG-005/006).
    pub limits: super::ApiLimitsConfig,
    /// OAuth-Client-Kennung des Nutzers (INTEG-003).
    pub oauth_client_id: Option<String>,
    /// URL-Abgleich (INTEG-014).
    pub url_match: super::UrlMatchConfig,
}

/// PageSpeed Insights und Lighthouse (M2-I04).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(default)]
pub struct PsiConfig {
    /// Quelle beim Crawl abfragen.
    pub enabled: bool,
    /// Alternative Basis-URL (INTEG-019).
    pub base_url: Option<String>,
    /// Gerätetyp.
    pub device: PsiDevice,
    /// Lighthouse lokal ausführen statt über die PSI-API.
    pub local: bool,
    /// Ausgewählte Metrikgruppen.
    pub metric_groups: Vec<String>,
    /// Konto-Kennung ohne Geheimnis (INTEG-003).
    pub account: Option<String>,
    /// Anfragegrenzen (INTEG-005/006).
    pub limits: super::ApiLimitsConfig,
}

/// Eine Linkmetrik-Quelle (Majestic, Ahrefs, Moz; M2-I05).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(default)]
pub struct LinkMetricsConfig {
    /// Quelle beim Crawl abfragen.
    pub enabled: bool,
    /// Alternative Basis-URL (INTEG-019).
    pub base_url: Option<String>,
    /// Ebenen (URL, Subdomain, Domain).
    pub levels: Vec<ApiLevel>,
    /// Ausgewählte Metriken.
    pub metrics: Vec<String>,
    /// Konto-Kennung ohne Geheimnis (INTEG-003).
    pub account: Option<String>,
    /// Nicht geheime Anbieteroptionen (INTEG-152/153/154/157).
    pub options: LinkMetricsOptions,
    /// Anfragegrenzen (INTEG-005/006).
    pub limits: super::ApiLimitsConfig,
}

/// Optionen der Linkdatenanbieter; fehlende Werte erhalten die Spec-Standards.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct LinkMetricsOptions {
    pub historic: bool,
    pub include_external: bool,
    pub oauth_client_id: Option<String>,
    pub workspace: Option<String>,
    pub country: Option<String>,
    pub average_volume: bool,
    pub moz_paid: bool,
    pub moz_free_interval_s: u32,
}
impl Default for LinkMetricsOptions {
    fn default() -> Self {
        Self {
            historic: false,
            include_external: false,
            oauth_client_id: None,
            workspace: None,
            country: None,
            average_volume: false,
            moz_paid: false,
            moz_free_interval_s: 10,
        }
    }
}

/// Google Drive für den Sheets-Export (M2-X03).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(default)]
pub struct DriveConfig {
    /// Eigene Google-OAuth-Client-ID für den Systembrowser.
    pub oauth_client_id: Option<String>,
    /// Export nach Google Sheets einschalten.
    pub enabled: bool,
    /// Alternative Basis-URL (INTEG-019).
    pub base_url: Option<String>,
    /// Wurzelordner (Standard `OpenSpider`).
    pub root_folder: Option<String>,
    /// Konto-Kennung ohne Geheimnis (INTEG-003).
    pub account: Option<String>,
    /// Anfragegrenzen (INTEG-005/006).
    pub limits: super::ApiLimitsConfig,
}

/// Alle API-Quellen einer Konfiguration; Zugangsdaten stehen im Schlüsselbund (INTEG-002, OUT-225).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(default)]
pub struct ApiConfig {
    /// Google Analytics 4.
    pub ga4: Ga4Config,
    /// Google Search Console.
    pub gsc: GscConfig,
    /// PageSpeed Insights.
    pub psi: PsiConfig,
    /// Majestic.
    pub majestic: LinkMetricsConfig,
    /// Ahrefs.
    pub ahrefs: LinkMetricsConfig,
    /// Moz.
    pub moz: LinkMetricsConfig,
    /// Google Drive.
    pub drive: DriveConfig,
}

impl Default for CustomSearchConfig {
    fn default() -> Self {
        Self {
            filters: Vec::new(),
            regex_timeout_ms: 2000,
        }
    }
}
impl Default for StructuredDataConfig {
    fn default() -> Self {
        Self {
            json_ld: false,
            microdata: false,
            rdfa: false,
            schema_org_validation: true,
            rich_results_validation: true,
            case_sensitive: false,
        }
    }
}
