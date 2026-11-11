//! Crawl-Konfiguration (`.osconfig`, TOML, OUT-221) – Strukturen, Defaults und einfache (De-)Serialisierung.
//!
//! Die TOML-Pfade entsprechen den Spec-Schlüsseln (M1-Plan 3.4). Fehlende Schlüssel erhalten beim Laden
//! den Standardwert (Vorwärtskompatibilität, CRAWL-132); unbekannte Schlüssel werden hier ignoriert –
//! ihre Meldung, die Validierung, `--set` und das JSON-Schema ergänzt M1-P13.
//!
//! M1-P13 liegt in den Untermodulen `io` (Laden/Speichern, Standard-Profil), `validate`, `set`
//! (`--set`, Voreinstellungsgruppen) und `schema` (JSON-Schema); ihre öffentlichen Typen und Funktionen
//! sind hier re-exportiert.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

mod io;
mod migrate;
mod schema;
mod set;
mod v2;
mod validate;
mod w1;

pub use io::{ConfigLoadError, LoadedConfig, DEFAULT_PROFILE_FILE};
pub use migrate::{migrate_toml, MigrationReport};
pub use schema::{json_schema, json_schema_string};
pub use set::{settable_keys, suggest_key, PresetGroup, SetError, SHORT_KEYS};
pub use v2::*;
pub use validate::{ConfigIssue, ConfigIssueKind, ValidationReport};
pub use w1::*;

use crate::enums::{
    CookieMode, CrawlMode, HreflangOverLimit, LineEnding, RenderingMode, RobotsMode,
    RobotsUnreachablePolicy,
};
use crate::error::ModelError;

/// Aktuelle Version des `.osconfig`-Formats. Version 2 (M2-F01) ergänzt die Abschnitte `rendering`, `extraction`,
/// `limits`, `advanced`, `content`, `spelling`, `grammar`, `custom_search`, `custom_extraction`, `custom_js`,
/// `link_positions`, `rewrite`, `cdns`, `http`, `auth`, `api`, `segments`, `compare` und `sitemap_gen`; Dateien der
/// Version 1 lädt [`CrawlConfig::load_str`] unverändert (fehlende Schlüssel = Standardwert), [`migrate_toml`] hebt sie
/// samt Sicherungskopie auf Version 2 (OUT-226).
pub const CONFIG_SCHEMA_VERSION: u32 = 2;

/// Die gesamte Konfiguration eines Crawls.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct CrawlConfig {
    /// Version des Dateiformats (OUT-221).
    pub schema_version: u32,
    /// Erzeuger, z. B. `openspider 0.1.0`.
    pub generator: String,
    /// Crawl-Modus.
    pub mode: CrawlMode,
    /// Bereich und Include/Exclude.
    pub scope: ScopeConfig,
    /// Was gespeichert und gecrawlt wird.
    pub spider: SpiderConfig,
    /// Limits.
    pub limits: LimitsConfig,
    /// Geschwindigkeit.
    pub speed: SpeedConfig,
    /// User-Agent.
    pub user_agent: UserAgentConfig,
    /// robots.txt.
    pub robots: RobotsConfig,
    /// Erweiterte Einstellungen.
    pub advanced: AdvancedConfig,
    /// Authentifizierung.
    pub auth: AuthConfig,
    /// Netzwerk.
    pub network: NetworkConfig,
    /// Rendering.
    pub rendering: RenderingConfig,
    /// Extraktion.
    pub extraction: ExtractionConfig,
    /// Inhaltsbereich und Content-Filter.
    pub content: ContentConfig,
    /// Schwellen für Page Titles.
    pub title: TextLimits,
    /// Schwellen für Meta Descriptions.
    pub meta_description: TextLimits,
    /// Schwelle für URLs.
    pub url: UrlLimits,
    /// Schwelle für H1.
    pub h1: HeadingLimits,
    /// Schwelle für H2.
    pub h2: HeadingLimits,
    /// Schwellen für Bilder.
    pub image: ImageLimits,
    /// Duplikaterkennung.
    pub duplicates: DuplicatesConfig,
    /// Issue-Berechnung.
    pub issues: IssuesConfig,
    /// PDF.
    pub pdf: PdfConfig,
    /// Links-Tab.
    pub links: LinksConfig,
    /// hreflang.
    pub hreflang: HreflangConfig,
    /// Sitemaps.
    pub sitemaps: SitemapsConfig,
    /// Crawl-Analyse.
    pub analysis: AnalysisConfig,
    /// Validierung.
    pub validation: ValidationConfig,
    /// Export.
    pub export: ExportConfig,
    /// Rechtschreibprüfung (ab v2).
    pub spelling: SpellingConfig,
    /// Grammatikprüfung (ab v2).
    pub grammar: GrammarConfig,
    /// Custom Search (ab v2).
    pub custom_search: CustomSearchConfig,
    /// Custom Extraction (ab v2).
    pub custom_extraction: CustomExtractionConfig,
    /// Custom JavaScript (ab v2).
    pub custom_js: CustomJsConfig,
    /// Link-Positionen (ab v2).
    pub link_positions: LinkPositionsConfig,
    /// URL-Umschreibung (ab v2).
    pub rewrite: RewriteConfig,
    /// CDN-Einträge, die als intern behandelt werden (CRAWL-108, ab v2): `cdn.other.test` oder `other.test/assets/`.
    pub cdns: Vec<String>,
    /// HTTP-Verhalten (ab v2).
    pub http: HttpConfig,
    /// API-Quellen (ab v2).
    pub api: ApiConfig,
    /// Segmente (ab v2).
    pub segments: SegmentsConfig,
    /// Vergleich zweier Crawls (ab v2).
    pub compare: CompareConfig,
    /// Sitemap-Generator (ab v2).
    pub sitemap_gen: SitemapGenConfig,
    /// Gerätespezifische SERP-Pixelgrenzen und Schriften (ONPAGE-194).
    pub serp: SerpConfig,
    /// Profilgebundene Filter und sichtbare Tabellenspalten (ONPAGE-023, OUT-009, OUT-220).
    pub ui: CrawlUiConfig,
    /// Zusätzliche Bildmessungen und Einsparschwelle (ONPAGE-172, ONPAGE-178, ONPAGE-180).
    pub images: ImagesConfig,
    /// Als veraltet zu meldende MIME-Typen (ONPAGE-043).
    pub security: SecurityConfig,
}

impl Default for CrawlConfig {
    fn default() -> Self {
        CrawlConfig {
            serp: SerpConfig::default(),
            ui: CrawlUiConfig::default(),
            images: ImagesConfig::default(),
            security: SecurityConfig::default(),

            schema_version: CONFIG_SCHEMA_VERSION,
            generator: format!("openspider {}", env!("CARGO_PKG_VERSION")),
            mode: CrawlMode::Spider,
            scope: ScopeConfig::default(),
            spider: SpiderConfig::default(),
            limits: LimitsConfig::default(),
            speed: SpeedConfig::default(),
            user_agent: UserAgentConfig::default(),
            robots: RobotsConfig::default(),
            advanced: AdvancedConfig::default(),
            auth: AuthConfig::default(),
            network: NetworkConfig::default(),
            rendering: RenderingConfig::default(),
            extraction: ExtractionConfig::default(),
            content: ContentConfig::default(),
            title: TextLimits {
                max_chars: 60,
                min_chars: 30,
                max_px: 561,
                min_px: 200,
            },
            meta_description: TextLimits {
                max_chars: 155,
                min_chars: 70,
                max_px: 985,
                min_px: 400,
            },
            url: UrlLimits::default(),
            h1: HeadingLimits::default(),
            h2: HeadingLimits::default(),
            image: ImageLimits::default(),
            duplicates: DuplicatesConfig::default(),
            issues: IssuesConfig::default(),
            pdf: PdfConfig::default(),
            links: LinksConfig::default(),
            hreflang: HreflangConfig::default(),
            sitemaps: SitemapsConfig::default(),
            analysis: AnalysisConfig::default(),
            validation: ValidationConfig::default(),
            export: ExportConfig::default(),
            spelling: SpellingConfig::default(),
            grammar: GrammarConfig::default(),
            custom_search: CustomSearchConfig::default(),
            custom_extraction: CustomExtractionConfig::default(),
            custom_js: CustomJsConfig::default(),
            link_positions: LinkPositionsConfig::default(),
            rewrite: RewriteConfig::default(),
            cdns: Vec::new(),
            http: HttpConfig::default(),
            api: ApiConfig::default(),
            segments: SegmentsConfig::default(),
            compare: CompareConfig::default(),
            sitemap_gen: SitemapGenConfig::default(),
        }
    }
}

impl CrawlConfig {
    /// Liest eine `.osconfig`; fehlende Schlüssel erhalten Standardwerte. Eine Datei älterer Version (OUT-226) ergibt
    /// im Speicher eine Konfiguration der aktuellen Version: die neuen Abschnitte tragen ihre Standardwerte
    /// (`LoadedConfig::migrated_from` meldet das beim Laden mit [`CrawlConfig::load_str`]).
    pub fn from_toml_str(s: &str) -> Result<Self, ModelError> {
        let mut config: CrawlConfig =
            toml::from_str(s).map_err(|e| ModelError::Config(e.to_string()))?;
        if (1..CONFIG_SCHEMA_VERSION).contains(&config.schema_version) {
            config.schema_version = CONFIG_SCHEMA_VERSION;
        }
        Ok(config)
    }

    /// Schreibt alle Werte (Reihenfolge = Felddeklaration, daher stabil). Nicht gesetzte optionale Werte
    /// (`None`) erscheinen nicht; beim Laden ergeben sie wieder `None`.
    pub fn to_toml_string(&self) -> Result<String, ModelError> {
        toml::to_string_pretty(self).map_err(|e| ModelError::Config(e.to_string()))
    }
}

/// Bereich des Crawls und Include/Exclude (CRAWL-007/009/109/110).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct ScopeConfig {
    /// Alle Subdomains der Start-Domain crawlen.
    pub all_subdomains: bool,
    /// Nur den Ordner der Start-URL crawlen.
    pub subfolder_only: bool,
    /// Ordner der Start-URL bei verlinkten Zielen verlassen (CRAWL-010).
    pub crawl_outside_start_folder: bool,
    /// Regex-Regeln treffen auch Teilstrings.
    pub regex_partial_match: bool,
    /// Include-Regeln (Regex).
    pub include: Vec<String>,
    /// Exclude-Regeln (Regex).
    pub exclude: Vec<String>,
}

/// Speichern und Crawlen je Linktyp.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct StoreCrawl {
    /// Im Crawl speichern.
    pub store: bool,
    /// Abrufen.
    pub crawl: bool,
}

impl StoreCrawl {
    /// Speichern und crawlen.
    pub const ON: StoreCrawl = StoreCrawl {
        store: true,
        crawl: true,
    };
    /// Nur speichern.
    pub const STORE_ONLY: StoreCrawl = StoreCrawl {
        store: true,
        crawl: false,
    };
    /// Weder speichern noch crawlen.
    pub const OFF: StoreCrawl = StoreCrawl {
        store: false,
        crawl: false,
    };
}

impl Default for StoreCrawl {
    fn default() -> Self {
        StoreCrawl::ON
    }
}

/// Spider-Einstellungen (CRAWL-038 ff.).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct SpiderConfig {
    /// Bilder.
    pub images: StoreCrawl,
    /// CSS.
    pub css: StoreCrawl,
    /// Style-Attribute ohne Rendering nach CSS-Verweisen durchsuchen (CRAWL-040).
    pub css_style_attributes: bool,
    /// JavaScript.
    pub javascript: StoreCrawl,
    /// Medien.
    pub media: StoreCrawl,
    /// Interne Hyperlinks.
    pub internal: StoreCrawl,
    /// Externe Links.
    pub external: StoreCrawl,
    /// Canonicals.
    pub canonicals: StoreCrawl,
    /// hreflang.
    pub hreflang: StoreCrawl,
    /// Paginierung.
    pub pagination: StoreCrawl,
    /// Meta Refresh.
    pub meta_refresh: StoreCrawl,
    /// Iframes.
    pub iframes: StoreCrawl,
    /// AMP.
    pub amp: StoreCrawl,
    /// Mobile Alternate.
    pub mobile_alternate: StoreCrawl,
    /// Interne nofollow-Links folgen.
    pub follow_internal_nofollow: bool,
    /// Externe nofollow-Links folgen.
    pub follow_external_nofollow: bool,
    /// Nicht abrufbare Links als Zeilen speichern (CRAWL-053, TECH-143, OUT-041).
    pub store_uncrawlable_links: bool,
}

impl Default for SpiderConfig {
    fn default() -> Self {
        SpiderConfig {
            store_uncrawlable_links: false,

            images: StoreCrawl::ON,
            css: StoreCrawl::ON,
            css_style_attributes: false,
            javascript: StoreCrawl::ON,
            media: StoreCrawl::ON,
            internal: StoreCrawl::ON,
            external: StoreCrawl::ON,
            canonicals: StoreCrawl::ON,
            hreflang: StoreCrawl::STORE_ONLY,
            pagination: StoreCrawl::STORE_ONLY,
            meta_refresh: StoreCrawl::ON,
            iframes: StoreCrawl::ON,
            amp: StoreCrawl::OFF,
            mobile_alternate: StoreCrawl::STORE_ONLY,
            follow_internal_nofollow: false,
            follow_external_nofollow: false,
        }
    }
}

/// Limits (CRAWL-066/067/072/075).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct LimitsConfig {
    /// Höchstzahl gecrawlter URLs.
    pub max_urls: u64,
    /// Höchste Crawl-Tiefe; nicht gesetzt = unbegrenzt (im Listenmodus wendet die Engine 0 an).
    pub max_depth: Option<u32>,
    /// Höchstzahl gefolgter Weiterleitungen.
    pub max_redirects: u32,
    /// Höchste Seitengröße in KB.
    pub max_page_kb: u64,
    /// Höchste Ordnertiefe einer URL; nicht gesetzt = unbegrenzt (CRAWL-069, ab v2).
    pub max_folder_depth: Option<u32>,
    /// Höchstzahl Query-Parameter einer URL; nicht gesetzt = unbegrenzt (CRAWL-070, ab v2).
    pub max_query_params: Option<u32>,
    /// Höchstzahl URLs je Subdomain; nicht gesetzt = unbegrenzt (CRAWL-071, ab v2).
    pub max_urls_per_subdomain: Option<u64>,
    /// Höchste URL-Länge in Zeichen (CRAWL-073, ab v2).
    pub max_url_length: u32,
    /// Höchstzahl Links je URL (CRAWL-074, ab v2).
    pub max_links_per_url: u32,
}

impl Default for LimitsConfig {
    fn default() -> Self {
        LimitsConfig {
            max_urls: 5_000_000,
            max_depth: None,
            max_redirects: 10,
            max_page_kb: 51_200,
            max_folder_depth: None,
            max_query_params: None,
            max_urls_per_subdomain: None,
            max_url_length: 10_000,
            max_links_per_url: 10_000,
        }
    }
}

/// Geschwindigkeit (CRAWL-112/113).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct SpeedConfig {
    /// Parallele Threads.
    pub threads: u32,
    /// Höchstzahl paralleler Abrufe je Host.
    pub max_per_host: Option<u32>,
    /// Höchstzahl Abrufe pro Sekunde.
    pub max_urls_per_sec: Option<f64>,
    /// Abrufrate bei Überlastung automatisch senken (CRAWL-146).
    pub adaptive_throttle: bool,
    /// Threadzahl automatisch anpassen (CRAWL-147).
    pub auto: bool,
    /// TCP-Verbindungen je Host (Schema, Host, Port): `auto` oder 1–6 (CRAWL-151).
    pub connections_per_host: ConnectionsPerHost,
}

impl Default for SpeedConfig {
    fn default() -> Self {
        SpeedConfig {
            adaptive_throttle: true,
            auto: false,

            threads: 5,
            max_per_host: None,
            max_urls_per_sec: None,
            connections_per_host: ConnectionsPerHost::Auto,
        }
    }
}

/// Verbindungen pro Host (CRAWL-151): `auto` (so viele wie Threads, höchstens
/// [`ConnectionsPerHost::MAX`]) oder ein fester Wert 1–6.
///
/// In TOML und JSON steht `"auto"` oder eine Ganzzahl. Beim Laden wird jede nicht negative Ganzzahl
/// angenommen; den Bereich prüft [`CrawlConfig::validate`] mit Feldpfad.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConnectionsPerHost {
    /// So viele Verbindungen wie Threads, höchstens [`ConnectionsPerHost::MAX`].
    #[default]
    Auto,
    /// Höchstens n Verbindungen je Host.
    Fixed(u32),
}

impl ConnectionsPerHost {
    /// Größter Wert (die Zahl, die Browser unter HTTP/1.1 je Host öffnen).
    pub const MAX: u32 = 6;

    /// Wirksame Verbindungszahl bei `threads` Threads (1 bis [`ConnectionsPerHost::MAX`]).
    pub fn resolve(self, threads: u32) -> u32 {
        match self {
            ConnectionsPerHost::Auto => threads,
            ConnectionsPerHost::Fixed(n) => n,
        }
        .clamp(1, Self::MAX)
    }
}

impl std::fmt::Display for ConnectionsPerHost {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConnectionsPerHost::Auto => f.write_str("auto"),
            ConnectionsPerHost::Fixed(n) => write!(f, "{n}"),
        }
    }
}

impl Serialize for ConnectionsPerHost {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            ConnectionsPerHost::Auto => s.serialize_str("auto"),
            ConnectionsPerHost::Fixed(n) => s.serialize_u32(*n),
        }
    }
}

impl<'de> Deserialize<'de> for ConnectionsPerHost {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl serde::de::Visitor<'_> for Visitor {
            type Value = ConnectionsPerHost;

            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("\"auto\" oder eine Ganzzahl von 1 bis 6")
            }

            fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<Self::Value, E> {
                u32::try_from(v)
                    .map(ConnectionsPerHost::Fixed)
                    .map_err(|_| E::invalid_value(serde::de::Unexpected::Unsigned(v), &self))
            }

            fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<Self::Value, E> {
                u64::try_from(v)
                    .map_err(|_| E::invalid_value(serde::de::Unexpected::Signed(v), &self))
                    .and_then(|v| self.visit_u64(v))
            }

            fn visit_f64<E: serde::de::Error>(self, v: f64) -> Result<Self::Value, E> {
                // JSON aus der Oberfläche kann ganze Zahlen als Gleitkommazahl liefern.
                if v.fract() == 0.0 && (0.0..=f64::from(u32::MAX)).contains(&v) {
                    Ok(ConnectionsPerHost::Fixed(v as u32))
                } else {
                    Err(E::invalid_value(serde::de::Unexpected::Float(v), &self))
                }
            }

            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Self::Value, E> {
                let v = v.trim();
                if v.eq_ignore_ascii_case("auto") {
                    return Ok(ConnectionsPerHost::Auto);
                }
                v.parse::<u32>()
                    .map(ConnectionsPerHost::Fixed)
                    .map_err(|_| E::invalid_value(serde::de::Unexpected::Str(v), &self))
            }
        }
        d.deserialize_any(Visitor)
    }
}

impl JsonSchema for ConnectionsPerHost {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        std::borrow::Cow::Borrowed("ConnectionsPerHost")
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "anyOf": [
                { "type": "string", "enum": ["auto"] },
                { "type": "integer", "minimum": 1, "maximum": ConnectionsPerHost::MAX }
            ]
        })
    }
}

/// User-Agent (CRAWL-114).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct UserAgentConfig {
    /// Name des Voreinstellung.
    pub preset: String,
    /// Eigener HTTP-User-Agent-String.
    pub http: Option<String>,
    /// Eigenes robots.txt-Token.
    pub robots_token: Option<String>,
}

impl Default for UserAgentConfig {
    fn default() -> Self {
        UserAgentConfig {
            preset: "openspider".to_string(),
            http: None,
            robots_token: None,
        }
    }
}

/// robots.txt (CRAWL-099/100).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct RobotsConfig {
    /// Umgang mit robots.txt (Listenmodus: die Engine wendet `ignore` an).
    pub mode: RobotsMode,
    /// Verhalten bei nicht erreichbarer robots.txt.
    pub unreachable_policy: RobotsUnreachablePolicy,
    /// Wiederholungen.
    pub retries: u32,
    /// Wartezeit zwischen Wiederholungen in Sekunden.
    pub retry_delay_s: u32,
    /// Durch robots.txt gesperrte interne URLs anzeigen (CRAWL-101).
    pub show_blocked_internal: bool,
    /// Durch robots.txt gesperrte externe URLs anzeigen (CRAWL-101).
    pub show_blocked_external: bool,
    /// Eigene robots.txt je Host ohne Schema, höchstens 512 KiB je Inhalt (CRAWL-102, CRAWL-132).
    pub custom_files: std::collections::BTreeMap<String, String>,
    /// Crawl-delay aus robots.txt beachten (CRAWL-145).
    pub respect_crawl_delay: bool,
}

impl Default for RobotsConfig {
    fn default() -> Self {
        RobotsConfig {
            show_blocked_internal: true,
            show_blocked_external: false,
            custom_files: Default::default(),
            respect_crawl_delay: false,

            mode: RobotsMode::Respect,
            unreachable_policy: RobotsUnreachablePolicy::BlockHost,
            retries: 2,
            retry_delay_s: 5,
        }
    }
}

/// Erweiterte Einstellungen (CRAWL-088/095, ONPAGE-020, TECH-068).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct AdvancedConfig {
    /// Cookie-Verhalten.
    pub cookies: CookieMode,
    /// Antwort-Timeout in Sekunden.
    pub response_timeout_s: u32,
    /// Antworten ohne Content-Type als HTML behandeln.
    pub assume_html: bool,
    /// Selbstverweisenden Meta Refresh berücksichtigen (Indexierbarkeit „Self Referencing Meta Refresh“,
    /// TECH-068). Standard aus wie die Referenz (K-57); der Selbst-Refresh wird unabhängig davon als
    /// Weiterleitung auf sich selbst geführt.
    pub respect_self_ref_meta_refresh: bool,
    /// Fragment-Bezeichner (`#…`) als eigene URLs crawlen (CRAWL-004, ab v2).
    pub crawl_fragments: bool,
    /// `noindex` respektieren: solche Seiten nicht speichern (CRAWL-091, ab v2).
    pub respect_noindex: bool,
    /// Canonical respektieren: kanonisierte Seiten nicht speichern (CRAWL-092, ab v2).
    pub respect_canonical: bool,
    /// HTML-Validierung auf Token-Ebene durchführen (TECH-008, `advanced.html_validation`, ab v2).
    pub html_validation: bool,
    /// Alle internen URLs liegen auf grünem Hosting (CRAWL-139, `advanced.green_hosting`, ab v2).
    pub green_hosting: bool,
    /// Weiterleitungsziele auch außerhalb des Crawl-Bereichs verfolgen (CRAWL-030, OUT-085).
    pub always_follow_redirects: bool,
    /// Canonical-Ziele auch außerhalb des Crawl-Bereichs verfolgen (CRAWL-030, OUT-085).
    pub always_follow_canonicals: bool,
}

impl Default for AdvancedConfig {
    fn default() -> Self {
        AdvancedConfig {
            always_follow_redirects: false,
            always_follow_canonicals: false,

            cookies: CookieMode::SessionOnly,
            response_timeout_s: 20,
            assume_html: false,
            respect_self_ref_meta_refresh: false,
            crawl_fragments: false,
            respect_noindex: false,
            respect_canonical: false,
            html_validation: true,
            green_hosting: false,
        }
    }
}

/// Authentifizierung (CRAWL-117, OUT-225). Passwörter stehen nie hier, nur der Verweis in den Schlüsselbund.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(default)]
pub struct AuthConfig {
    /// Basic-Auth-Einträge (Digest wird aus der Server-Challenge erkannt, CRAWL-118).
    pub basic: Vec<BasicAuth>,
    /// Formularbasierte Anmeldungen (CRAWL-120, ab v2).
    pub form: Vec<FormLogin>,
    /// Sitzungsschutz-Muster: URLs, die nie abgerufen werden, damit die Sitzung nicht endet (z. B. Logout; ab v2).
    pub session_guard: Vec<String>,
    /// Verweise auf Authentifizierungsprofile (OUT-223, ab v2); die Profile selbst liegen verschlüsselt im Profilordner.
    pub profile_refs: Vec<String>,
}

/// Ein Basic-Auth-Eintrag.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct BasicAuth {
    /// URL-Präfix, für das der Eintrag gilt.
    pub prefix: String,
    /// Benutzername.
    pub username: String,
    /// Verweis auf das Passwort im Schlüsselbund (`SecretStore`).
    pub secret_ref: String,
}

/// Netzwerk (CRAWL-123).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(default)]
pub struct NetworkConfig {
    /// Host-Auflösung `host=ip[:port]`; `*.domain` erlaubt (Port ist eine Test-Erweiterung, K-11).
    pub resolve: Vec<String>,
}

/// Rendering (CRAWL-077).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct RenderingConfig {
    /// Modus (`text` oder `javascript`, CRAWL-077).
    pub mode: RenderingMode,
    /// AJAX-Timeout in Sekunden, 0 – 60 (CRAWL-080, ab v2).
    pub ajax_timeout_s: f64,
    /// JavaScript-Fehlerberichte und Konsolen-Log mitschneiden (CRAWL-084, `rendering.js_error_reporting`, ab v2).
    pub js_error_reporting: bool,
    /// Fenstergröße und Gerät (CRAWL-081, ab v2).
    pub viewport: ViewportConfig,
    /// Screenshots (CRAWL-082, ab v2).
    pub screenshots: ScreenshotConfig,
    /// Anzahl paralleler Browser-Tabs; nicht gesetzt = Threads, gedeckelt auf die CPU-Kerne (CRAWL-078, ab v2).
    pub tabs: Option<u32>,
    /// Browser nach so vielen Seiten neu starten (PERF-006, ab v2).
    pub restart_after_pages: u32,
    /// Browser bei diesem Speicherverbrauch in MB neu starten (PERF-006, ab v2).
    pub restart_memory_mb: u32,
    /// Zeitlimit je Seite in Sekunden; danach Fehler `Rendering fehlgeschlagen` (PERF-006, ab v2).
    pub page_timeout_s: u32,
    /// Pfad zu Chromium; nicht gesetzt = gefundene oder mitgelieferte Installation (ab v2).
    pub chromium_path: Option<String>,
}

impl Default for RenderingConfig {
    fn default() -> Self {
        RenderingConfig {
            mode: RenderingMode::Text,
            ajax_timeout_s: 5.0,
            js_error_reporting: true,
            viewport: ViewportConfig::default(),
            screenshots: ScreenshotConfig::default(),
            tabs: None,
            restart_after_pages: 500,
            restart_memory_mb: 1500,
            page_timeout_s: 60,
            chromium_path: None,
        }
    }
}

/// Was aus einer Seite extrahiert wird (ONPAGE-018/090 ff., ONPAGE-140).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct ExtractionConfig {
    /// Page Titles.
    pub page_titles: bool,
    /// Meta Descriptions.
    pub meta_description: bool,
    /// Meta Keywords.
    pub meta_keywords: bool,
    /// H1.
    pub h1: bool,
    /// H2.
    pub h2: bool,
    /// Hash (MD5).
    pub hash: bool,
    /// Antwortzeit.
    pub response_time: bool,
    /// Last-Modified.
    pub last_modified: bool,
    /// HTTP-Header speichern.
    pub http_headers: bool,
    /// Roh-HTML speichern (Vorhalt, CRAWL-063).
    pub store_html: bool,
    /// Gerendertes HTML speichern (CRAWL-063, nur mit JavaScript-Rendering; ab v2).
    pub store_rendered_html: bool,
    /// Indexierbarkeit (CRAWL-061, ab v2).
    pub indexability: bool,
    /// Wortanzahl (CRAWL-061, ab v2).
    pub word_count: bool,
    /// Lesbarkeit (ONPAGE-149, ab v2).
    pub readability: bool,
    /// Text-zu-Code-Verhältnis (CRAWL-061, ab v2).
    pub text_ratio: bool,
    /// Seitengröße (CRAWL-061, ab v2).
    pub page_size: bool,
    /// Formulare (CRAWL-061, ab v2).
    pub forms: bool,
    /// Barrierefreiheit mit axe-core (TECH-250, erfordert JavaScript-Rendering; Standard aus; ab v2).
    pub accessibility: bool,
    /// Cookies speichern (CRAWL-062, OS-Default aus; ab v2).
    pub cookies: bool,
    /// Meta Robots (CRAWL-065, ab v2).
    pub meta_robots: bool,
    /// X-Robots-Tag (CRAWL-065, ab v2).
    pub x_robots_tag: bool,
    /// Bilder aus `srcset` extrahieren (CRAWL-055, Standard aus; ab v2).
    pub srcset: bool,
    /// Strukturierte Daten (CRAWL-065, ab v2).
    pub structured_data: StructuredDataConfig,
}

impl Default for ExtractionConfig {
    fn default() -> Self {
        ExtractionConfig {
            page_titles: true,
            meta_description: true,
            meta_keywords: true,
            h1: true,
            h2: true,
            hash: true,
            response_time: true,
            last_modified: true,
            http_headers: false,
            store_html: false,
            store_rendered_html: false,
            indexability: true,
            word_count: true,
            readability: true,
            text_ratio: true,
            page_size: true,
            forms: true,
            accessibility: false,
            cookies: false,
            meta_robots: true,
            x_robots_tag: true,
            srcset: false,
            structured_data: StructuredDataConfig::default(),
        }
    }
}

/// Inhaltsbereich (ONPAGE-140).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct ContentAreaConfig {
    /// Nur Text innerhalb dieser Elemente.
    pub include_elements: Vec<String>,
    /// Nur Text innerhalb dieser Klassen.
    pub include_classes: Vec<String>,
    /// Nur Text innerhalb dieser IDs.
    pub include_ids: Vec<String>,
    /// Text in diesen Elementen ausschließen.
    pub exclude_elements: Vec<String>,
    /// Text in diesen Klassen ausschließen.
    pub exclude_classes: Vec<String>,
    /// Text in diesen IDs ausschließen.
    pub exclude_ids: Vec<String>,
}

impl Default for ContentAreaConfig {
    fn default() -> Self {
        ContentAreaConfig {
            include_elements: Vec::new(),
            include_classes: Vec::new(),
            include_ids: Vec::new(),
            exclude_elements: vec![
                "nav".to_string(),
                "footer".to_string(),
                "header".to_string(),
                "aside".to_string(),
            ],
            exclude_classes: Vec::new(),
            exclude_ids: Vec::new(),
        }
    }
}

/// Inhalt und Content-Filter (ONPAGE-140/146/147).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct ContentConfig {
    /// Inhaltsbereich.
    pub area: ContentAreaConfig,
    /// Wortanzahl, unter der eine Seite als „Low Content“ gilt.
    pub low_word_count: u32,
    /// Soft-404-Phrasen (leer = Filter aus).
    pub soft404_phrases: Vec<String>,
    /// Lesbarkeit (ONPAGE-152, ab v2).
    pub readability: ReadabilityConfig,
}

impl Default for ContentConfig {
    fn default() -> Self {
        ContentConfig {
            area: ContentAreaConfig::default(),
            low_word_count: 200,
            readability: ReadabilityConfig::default(),
            soft404_phrases: [
                "page not found",
                "404 not found",
                "error 404",
                "404 error",
                "page cannot be found",
                "page can't be found",
                "page could not be found",
                "seite nicht gefunden",
                "seite wurde nicht gefunden",
                "fehler 404",
                "404 fehler",
                "page introuvable",
                "página no encontrada",
                "pagina non trovata",
                "pagina niet gevonden",
            ]
            .iter()
            .map(|s| s.to_string())
            .collect(),
        }
    }
}

/// Schwellen für Zeichen und Pixel (Titel, Meta-Description; 02 §0.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct TextLimits {
    /// Höchstzahl Zeichen.
    pub max_chars: u32,
    /// Mindestzahl Zeichen.
    pub min_chars: u32,
    /// Höchstbreite in Pixeln.
    pub max_px: u32,
    /// Mindestbreite in Pixeln.
    pub min_px: u32,
}

impl Default for TextLimits {
    fn default() -> Self {
        TextLimits {
            max_chars: 60,
            min_chars: 30,
            max_px: 561,
            min_px: 200,
        }
    }
}

/// Schwelle für URLs (ONPAGE-081).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct UrlLimits {
    /// Höchstzahl Zeichen.
    pub max_chars: u32,
    /// Pfadsegmente zur Erkennung interner Suchseiten (ONPAGE-077).
    pub internal_search_segments: Vec<String>,
    /// Query-Parameter zur Erkennung interner Suchseiten (ONPAGE-077).
    pub internal_search_params: Vec<String>,
}

impl Default for UrlLimits {
    fn default() -> Self {
        UrlLimits {
            internal_search_segments: [
                "search",
                "suche",
                "recherche",
                "buscar",
                "busca",
                "cerca",
                "zoeken",
                "szukaj",
                "sok",
                "haku",
                "catalogsearch",
            ]
            .map(String::from)
            .to_vec(),
            internal_search_params: [
                "q",
                "query",
                "search",
                "search_query",
                "searchterm",
                "keyword",
                "keywords",
                "k",
                "term",
            ]
            .map(String::from)
            .to_vec(),
            max_chars: 115,
        }
    }
}

/// Schwelle für Überschriften (ONPAGE-123/133).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct HeadingLimits {
    /// Höchstzahl Zeichen.
    pub max_chars: u32,
}

impl Default for HeadingLimits {
    fn default() -> Self {
        HeadingLimits { max_chars: 70 }
    }
}

/// Schwellen für Bilder (ONPAGE-174/177).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct ImageLimits {
    /// Höchstgröße in KB (1 KB = 1024 Byte).
    pub max_kb: u32,
    /// Höchstzahl Zeichen im Alt-Text.
    pub alt_max_chars: u32,
}

impl Default for ImageLimits {
    fn default() -> Self {
        ImageLimits {
            max_kb: 100,
            alt_max_chars: 100,
        }
    }
}

/// Duplikaterkennung (ONPAGE-005/143/144).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct DuplicatesConfig {
    /// Nur indexierbare Seiten vergleichen.
    pub only_indexable: bool,
    /// Paginierte Seiten ausnehmen.
    pub ignore_paginated: bool,
    /// Exakte Duplikate suchen.
    pub exact_enabled: bool,
    /// Nahezu-Duplikate suchen.
    pub near_enabled: bool,
    /// Schwelle für Nahezu-Duplikate in Prozent.
    pub near_threshold_pct: u8,
}

impl Default for DuplicatesConfig {
    fn default() -> Self {
        DuplicatesConfig {
            only_indexable: true,
            ignore_paginated: true,
            exact_enabled: true,
            near_enabled: false,
            near_threshold_pct: 90,
        }
    }
}

/// Issue-Berechnung (CRAWL-089).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct IssuesConfig {
    /// Nicht indexierbare URLs aus Problemfiltern ausnehmen.
    pub ignore_non_indexable: bool,
}

impl Default for IssuesConfig {
    fn default() -> Self {
        IssuesConfig {
            ignore_non_indexable: true,
        }
    }
}

/// PDF (ONPAGE-009/166).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(default)]
pub struct PdfConfig {
    /// PDF-Dateien speichern.
    pub store: bool,
    /// PDF-Eigenschaften extrahieren (CRAWL-064, ab v2; Titel und Schlüsselwörter werden immer gelesen).
    pub extract_properties: bool,
    /// Linktext aus PDF extrahieren (CRAWL-064, ab v2; speicher- und zeitintensiv).
    pub extract_link_text: bool,
}

/// Links-Tab (TECH-144).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct LinksConfig {
    /// Crawl-Tiefe, ab der „High Crawl Depth“ gilt.
    pub high_crawl_depth: u32,
    /// Anzahl interner Outlinks, ab der „High Internal Outlinks“ gilt.
    pub high_internal_outlinks: u32,
    /// Anzahl externer Outlinks, ab der „High External Outlinks“ gilt.
    pub high_external_outlinks: u32,
    /// Nichtssagende Ankertexte.
    pub non_descriptive_anchors: Vec<String>,
}

impl Default for LinksConfig {
    fn default() -> Self {
        LinksConfig {
            high_crawl_depth: 4,
            high_internal_outlinks: 1000,
            high_external_outlinks: 100,
            non_descriptive_anchors: [
                "click here",
                "click",
                "here",
                "this",
                "this page",
                "link",
                "more",
                "read more",
                "learn more",
                "more info",
                "more information",
                "find out more",
                "details",
                "continue",
                "go",
                "start",
                "hier",
                "hier klicken",
                "klicken sie hier",
                "mehr",
                "weiter",
                "weiterlesen",
                "mehr erfahren",
                "mehr lesen",
                "mehr info",
                "details ansehen",
            ]
            .iter()
            .map(|s| s.to_string())
            .collect(),
        }
    }
}

/// hreflang (TECH-004/072).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct HreflangConfig {
    /// Verhalten bei mehr als 500 Annotationen.
    pub over_limit: HreflangOverLimit,
    /// Skript-Subtag im Sprachcode zulassen.
    pub allow_script_subtag: bool,
}

impl Default for HreflangConfig {
    fn default() -> Self {
        HreflangConfig {
            over_limit: HreflangOverLimit::Truncate,
            allow_script_subtag: true,
        }
    }
}

/// Sitemaps (TECH-210).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct SitemapsConfig {
    /// Sitemaps aus Links crawlen.
    pub crawl_linked: bool,
    /// Sitemaps aus der robots.txt automatisch finden.
    pub auto_discover_robots: bool,
    /// Zusätzliche Sitemap-URLs.
    pub urls: Vec<String>,
}

impl Default for SitemapsConfig {
    fn default() -> Self {
        SitemapsConfig {
            crawl_linked: false,
            auto_discover_robots: true,
            urls: Vec::new(),
        }
    }
}

/// Crawl-Analyse (CRAWL-141): automatischer Start und je Teilschritt ein Schalter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct AnalysisConfig {
    /// Analyse am Crawl-Ende automatisch starten.
    pub auto_at_end: bool,
    /// Link Score.
    pub link_score: bool,
    /// Kanonisierungen.
    pub canonicals: bool,
    /// Paginierung.
    pub pagination: bool,
    /// hreflang.
    pub hreflang: bool,
    /// Links.
    pub links: bool,
    /// Sitemaps.
    pub sitemaps: bool,
    /// Inhalt.
    pub content: bool,
    /// Bilder.
    pub images: bool,
    /// Analytics.
    pub analytics: bool,
    /// Search Console.
    pub search_console: bool,
    /// Link-Metriken.
    pub link_metrics: bool,
    /// AMP.
    pub amp: bool,
    /// Nahezu-Duplikate per MinHash (K2-19, ab v2; wirkt nur mit `duplicates.near_enabled`).
    pub near_duplicates: bool,
}

impl Default for AnalysisConfig {
    fn default() -> Self {
        AnalysisConfig {
            auto_at_end: true,
            link_score: true,
            canonicals: true,
            pagination: true,
            hreflang: true,
            links: true,
            sitemaps: true,
            content: true,
            images: true,
            analytics: true,
            search_console: true,
            link_metrics: true,
            amp: true,
            near_duplicates: true,
        }
    }
}

impl AnalysisConfig {
    /// Ist der Teilschritt eingeschaltet?
    pub fn step_enabled(&self, step: crate::enums::AnalysisStep) -> bool {
        use crate::enums::AnalysisStep as S;
        match step {
            S::LinkScore => self.link_score,
            S::Canonicals => self.canonicals,
            S::Pagination => self.pagination,
            S::Hreflang => self.hreflang,
            S::Links => self.links,
            S::Sitemaps => self.sitemaps,
            S::Content => self.content,
            S::Images => self.images,
            S::Analytics => self.analytics,
            S::SearchConsole => self.search_console,
            S::LinkMetrics => self.link_metrics,
            S::Amp => self.amp,
            S::NearDuplicates => self.near_duplicates,
        }
    }
}

/// Validierung (CRAWL-140 [P1]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct ValidationConfig {
    /// Höchstgröße eines HTML-Dokuments in Byte.
    pub html_doc_max_bytes: u64,
    /// Höchstgröße einer Ressource in Byte.
    pub resource_max_bytes: u64,
}

impl Default for ValidationConfig {
    fn default() -> Self {
        ValidationConfig {
            html_doc_max_bytes: 2_097_152,
            resource_max_bytes: 2_097_152,
        }
    }
}

/// Export (OUT-002).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct ExportConfig {
    /// Zugangsdaten in Request-Header-Exporten anzeigen (OUT-048, Standard aus).
    pub show_credentials: bool,
    /// UTF-8-BOM in CSV-Dateien.
    pub utf8_bom: bool,
    /// Zeilenende in CSV-Dateien.
    pub line_ending: LineEnding,
    /// Inlink-Exporte für alle Filter anbieten (OUT-066).
    pub inlinks_for_all_filters: bool,
}

impl Default for ExportConfig {
    fn default() -> Self {
        ExportConfig {
            show_credentials: false,
            inlinks_for_all_filters: true,

            utf8_bom: true,
            line_ending: LineEnding::Crlf,
        }
    }
}

impl Default for ScopeConfig {
    fn default() -> Self {
        Self {
            all_subdomains: false,
            subfolder_only: false,
            crawl_outside_start_folder: true,
            regex_partial_match: false,
            include: Vec::new(),
            exclude: Vec::new(),
        }
    }
}
