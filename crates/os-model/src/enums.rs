//! Enums des gemeinsamen Modells.
//!
//! Jedes Enum (außer [`FilterState`]) hat `as_str()` (= DB-Wert = kanonischer Exportwert), `FromStr`,
//! `Display`, `ALL` sowie Serde-/JSON-Schema-Unterstützung über genau diese Zeichenketten.
//! Wo die Spec einen Anzeige-/Exportwert festlegt, ist dieser der `as_str()`-Wert (z. B. `ContentClass`,
//! `LinkType`, `IndexabilityStatus`); alle übrigen Werte sind `snake_case` (auch als TOML-Wert der Konfiguration).

/// Erzeugt ein Enum mit `as_str`, `FromStr`, `Display`, Serde (als String) und `JsonSchema`.
macro_rules! string_enum {
    (
        $(#[$meta:meta])*
        $vis:vis enum $name:ident {
            $( $(#[$vmeta:meta])* $variant:ident => $text:literal ),+ $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        $vis enum $name {
            $( $(#[$vmeta])* $variant ),+
        }

        impl $name {
            /// Alle Werte in Deklarationsreihenfolge.
            pub const ALL: &'static [$name] = &[ $( $name::$variant ),+ ];

            /// DB-Wert und kanonischer Exportwert.
            pub fn as_str(&self) -> &'static str {
                match self { $( $name::$variant => $text ),+ }
            }
        }

        impl ::std::fmt::Display for $name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl ::std::str::FromStr for $name {
            type Err = $crate::error::ModelError;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                match s {
                    $( $text => Ok($name::$variant), )+
                    _ => Err($crate::error::ModelError::UnknownValue {
                        kind: stringify!($name),
                        value: s.to_string(),
                    }),
                }
            }
        }

        impl ::serde::Serialize for $name {
            fn serialize<S: ::serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                s.serialize_str(self.as_str())
            }
        }

        impl<'de> ::serde::Deserialize<'de> for $name {
            fn deserialize<D: ::serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                let s = <::std::borrow::Cow<'de, str> as ::serde::Deserialize>::deserialize(d)?;
                <$name as ::std::str::FromStr>::from_str(&s).map_err(|_| {
                    ::serde::de::Error::unknown_variant(&s, &[ $( $text ),+ ])
                })
            }
        }

        impl ::schemars::JsonSchema for $name {
            fn schema_name() -> ::std::borrow::Cow<'static, str> {
                ::std::borrow::Cow::Borrowed(stringify!($name))
            }

            fn json_schema(_: &mut ::schemars::SchemaGenerator) -> ::schemars::Schema {
                ::schemars::json_schema!({
                    "type": "string",
                    "enum": [ $( $text ),+ ]
                })
            }
        }
    };
}
pub(crate) use string_enum;

string_enum! {
    /// Inhaltsklasse einer URL (ONPAGE-020).
    #[derive(Default)]
    pub enum ContentClass {
        Html => "HTML",
        JavaScript => "JavaScript",
        Css => "CSS",
        Images => "Images",
        Pdf => "PDF",
        Plugins => "Plugins",
        Media => "Media",
        Fonts => "Fonts",
        Xml => "XML",
        Other => "Other",
        #[default]
        Unknown => "Unknown",
    }
}

string_enum! {
    /// Grund, warum keine HTTP-Antwort vorliegt (Statuscode 0). `as_str()` ist der DB-Wert;
    /// der Anzeigetext steht in [`NoResponseReason::status_text`] (nicht eindeutig, K-04).
    pub enum NoResponseReason {
        DnsLookupFailed => "dns_lookup_failed",
        ConnectionRefused => "connection_refused",
        ConnectionTimeout => "connection_timeout",
        ResponseTimeout => "response_timeout",
        ConnectionError => "connection_error",
        TlsError => "tls_error",
        Malformed => "malformed",
        BlockedByRobots => "blocked_by_robots",
        BlockedByRobotsUnreachable => "blocked_by_robots_unreachable",
        TooManyRedirects => "too_many_redirects",
        BlockedPrivateTarget => "blocked_private_target",
    }
}

impl NoResponseReason {
    /// Statustext in Tabellen und Exporten.
    pub fn status_text(&self) -> &'static str {
        match self {
            NoResponseReason::DnsLookupFailed => "DNS Lookup Failed",
            NoResponseReason::ConnectionRefused => "Connection Refused",
            NoResponseReason::ConnectionTimeout => "Connection Timeout",
            // K-04: Antwort-Timeout erscheint ebenfalls als „Connection Timeout“.
            NoResponseReason::ResponseTimeout => "Connection Timeout",
            NoResponseReason::ConnectionError => "Connection Error",
            NoResponseReason::TlsError => "TLS Error",
            NoResponseReason::Malformed => "Malformed",
            NoResponseReason::BlockedByRobots => "Blocked by Robots.txt",
            NoResponseReason::BlockedByRobotsUnreachable => {
                "Blocked by Robots.txt (robots.txt unreachable)"
            }
            NoResponseReason::TooManyRedirects => "Too Many Redirects",
            NoResponseReason::BlockedPrivateTarget => "Blocked (private target)",
        }
    }
}

string_enum! {
    /// Art einer Weiterleitung.
    pub enum RedirectType {
        Http => "http",
        HstsPolicy => "hsts_policy",
        JavaScript => "javascript",
        MetaRefresh => "meta_refresh",
        HttpRefresh => "http_refresh",
    }
}

impl RedirectType {
    /// Bezeichnung in den Tabs (ONPAGE-018).
    pub fn tab_label(&self) -> &'static str {
        match self {
            RedirectType::Http => "HTTP Redirect",
            RedirectType::HstsPolicy => "HSTS Policy",
            RedirectType::JavaScript => "JavaScript Redirect",
            RedirectType::MetaRefresh => "Meta Refresh Redirect",
            RedirectType::HttpRefresh => "HTTP Refresh Redirect",
        }
    }

    /// Bezeichnung in den Reports (OUT-081, K-03): wie [`tab_label`](Self::tab_label), aber „HTTP Refresh“.
    pub fn report_label(&self) -> &'static str {
        match self {
            RedirectType::HttpRefresh => "HTTP Refresh",
            other => other.tab_label(),
        }
    }
}

string_enum! {
    /// Linktyp (OUT-031). `as_str()` ist der Wert der Spalte `Type`.
    #[derive(Default)]
    pub enum LinkType {
        #[default]
        Hyperlink => "Hyperlink",
        Image => "Image",
        Css => "CSS",
        JavaScript => "JavaScript",
        Media => "Media",
        Iframe => "Iframe",
        Canonical => "Canonical",
        HttpCanonical => "HTTP Canonical",
        RelNext => "Rel Next",
        RelPrev => "Rel Prev",
        Hreflang => "Hreflang",
        HttpHreflang => "HTTP Hreflang",
        Amp => "AMP",
        MobileAlternate => "Mobile Alternate",
        MetaRefresh => "Meta Refresh",
        HttpRefresh => "HTTP Refresh",
        HttpRedirect => "HTTP Redirect",
        HstsPolicy => "HSTS Policy",
        JavaScriptRedirect => "JavaScript Redirect",
        XmlSitemap => "XML Sitemap",
        PdfLink => "PDF Link",
        /// `object`/`embed`/`preload` u. ä. ohne eigenen Typ.
        Other => "Other",
    }
}

impl LinkType {
    /// Navigations-Hyperlink (zählt für `inlinks`/`outlinks`, ONPAGE-015).
    pub fn is_hyperlink(&self) -> bool {
        matches!(self, LinkType::Hyperlink)
    }

    /// Eingebundene Ressource (Bild, CSS, JavaScript, Medien, Iframe); zählt nur in `inlinks_all`.
    pub fn is_resource(&self) -> bool {
        matches!(
            self,
            LinkType::Image
                | LinkType::Css
                | LinkType::JavaScript
                | LinkType::Media
                | LinkType::Iframe
        )
    }
}

string_enum! {
    /// Pfadart eines Links (OUT-031).
    #[derive(Default)]
    pub enum PathType {
        #[default]
        Absolute => "Absolute",
        ProtocolRelative => "Protocol-Relative",
        RootRelative => "Root-Relative",
        PathRelative => "Path-Relative",
    }
}

/// Offene Linkposition (INTEG-082); unbekannte Namen bleiben erhalten.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    Default,
    serde::Serialize,
    serde::Deserialize,
    schemars::JsonSchema,
)]
#[serde(from = "String", into = "String")]
#[schemars(with = "String")]
pub enum LinkPosition {
    Navigation,
    Header,
    #[default]
    Content,
    Sidebar,
    Aside,
    Footer,
    Head,
    Custom(String),
}
impl LinkPosition {
    pub const ALL: &'static [Self] = &[
        Self::Navigation,
        Self::Header,
        Self::Content,
        Self::Sidebar,
        Self::Aside,
        Self::Footer,
        Self::Head,
    ];
    pub fn as_str(&self) -> &str {
        match self {
            Self::Navigation => "Navigation",
            Self::Header => "Header",
            Self::Content => "Content",
            Self::Sidebar => "Sidebar",
            Self::Aside => "Aside",
            Self::Footer => "Footer",
            Self::Head => "Head",
            Self::Custom(s) => s,
        }
    }
}
impl From<String> for LinkPosition {
    fn from(s: String) -> Self {
        Self::ALL
            .iter()
            .find(|v| v.as_str() == s)
            .cloned()
            .unwrap_or(Self::Custom(s))
    }
}
impl From<LinkPosition> for String {
    fn from(p: LinkPosition) -> Self {
        p.as_str().to_owned()
    }
}
impl std::str::FromStr for LinkPosition {
    type Err = crate::error::ModelError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(s.to_owned().into())
    }
}
impl std::fmt::Display for LinkPosition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

string_enum! {
    /// Herkunft eines Links (nur mit JS-Rendering ungleich `Html`).
    #[derive(Default)]
    pub enum LinkOrigin {
        #[default]
        Html => "HTML",
        RenderedHtml => "Rendered HTML",
        Both => "HTML & Rendered HTML",
        DynamicallyLoaded => "Dynamically Loaded",
    }
}

string_enum! {
    /// Indexierbarkeit einer URL.
    pub enum Indexability {
        Indexable => "Indexable",
        NonIndexable => "Non-Indexable",
    }
}

string_enum! {
    /// Begründung der Nicht-Indexierbarkeit (TECH-001). Die Variantenreihenfolge ist die Prüfreihenfolge.
    pub enum IndexabilityStatus {
        BlockedByRobots => "Blocked by Robots.txt",
        NoResponse => "No Response",
        Redirected => "Redirected",
        ClientError => "Client Error",
        ServerError => "Server Error",
        SelfReferencingMetaRefresh => "Self Referencing Meta Refresh",
        Noindex => "noindex",
        UnavailableAfter => "Unavailable After",
        Canonicalised => "Canonicalised",
    }
}

string_enum! {
    /// Wie eine URL entdeckt wurde.
    pub enum DiscoveryKind {
        Start => "start",
        ListInput => "list_input",
        Link => "link",
        Redirect => "redirect",
        Sitemap => "sitemap",
        RobotsTxt => "robots_txt",
    }
}

string_enum! {
    /// Bearbeitungszustand einer URL.
    pub enum UrlState {
        Discovered => "discovered",
        Queued => "queued",
        Done => "done",
    }
}

string_enum! {
    /// Grund, warum eine URL nicht gecrawlt wurde (AGENT-012); nur gesetzt, wenn der Zustand nicht `Done` ist.
    pub enum NotCrawledReason {
        SessionGuard => "session_guard",
        Excluded => "excluded",
        NotIncluded => "not_included",
        OutsideScope => "outside_scope",
        StoreOnly => "store_only",
        CrawlDisabled => "crawl_disabled",
        Nofollow => "nofollow",
        DepthLimit => "depth_limit",
        UrlLimit => "url_limit",
        RedirectLimit => "redirect_limit",
        ExternalNotChecked => "external_not_checked",
        ListModeDepth => "list_mode_depth",
    }
}

string_enum! {
    /// Crawl-Modus (CRAWL-026/028).
    #[derive(Default)]
    pub enum CrawlMode {
        #[default]
        Spider => "spider",
        List => "list",
        Serp => "serp",
    }
}

string_enum! {
    /// Zustand eines Crawls (CRAWL-128; Schlüssel wie OUT-228). `Incomplete` = „Unterbrochen“.
    pub enum CrawlState {
        Running => "Running",
        Paused => "Paused",
        Completed => "Completed",
        Incomplete => "Incomplete",
        Failed => "Failed",
    }
}

string_enum! {
    /// Grund für das Ende eines Crawls.
    pub enum EndReason {
        QueueEmpty => "queue_empty",
        LimitReached => "limit_reached",
        Stopped => "stopped",
    }
}

string_enum! {
    /// Umgang mit robots.txt (CRAWL-099).
    #[derive(Default)]
    pub enum RobotsMode {
        #[default]
        Respect => "respect",
        Ignore => "ignore",
        IgnoreButReport => "ignore_but_report",
    }
}

string_enum! {
    /// Verhalten bei nicht erreichbarer robots.txt (CRAWL-100).
    #[derive(Default)]
    pub enum RobotsUnreachablePolicy {
        #[default]
        BlockHost => "block_host",
        AllowAll => "allow_all",
    }
}

string_enum! {
    /// Cookie-Verhalten (CRAWL-088).
    #[derive(Default)]
    pub enum CookieMode {
        #[default]
        SessionOnly => "session_only",
        Persistent => "persistent",
        None => "none",
    }
}

string_enum! {
    /// Rendering-Modus (CRAWL-077); M1 nur `Text`.
    #[derive(Default)]
    pub enum RenderingMode {
        #[default]
        Text => "text",
        JavaScript => "javascript",
    }
}

string_enum! {
    /// Herkunft von Canonical/Hreflang/Pagination-Angaben.
    #[derive(Default)]
    pub enum HintSource {
        #[default]
        Html => "html",
        Http => "http",
        Sitemap => "sitemap",
    }
}

string_enum! {
    /// Quelle einer Robots-Direktive.
    pub enum DirectiveSource {
        MetaRobots => "meta_robots",
        XRobotsTag => "x_robots_tag",
    }
}

string_enum! {
    /// Quelle einer Refresh-Angabe.
    pub enum RefreshSource {
        Meta => "meta",
        Header => "header",
    }
}

string_enum! {
    /// Quelle einer Bildinstanz.
    #[derive(Default)]
    pub enum ImageSource {
        #[default]
        Img => "img",
        A => "a",
        Srcset => "srcset",
        InputImage => "input_image",
        Poster => "poster",
        Css => "css",
    }
}

string_enum! {
    /// Teilschritt der Crawl-Analyse (CRAWL-141); `as_str()` entspricht den Schlüsseln `analysis.<schritt>`.
    pub enum AnalysisStep {
        LinkScore => "link_score",
        Canonicals => "canonicals",
        Pagination => "pagination",
        Hreflang => "hreflang",
        Links => "links",
        Sitemaps => "sitemaps",
        Content => "content",
        Images => "images",
        Analytics => "analytics",
        SearchConsole => "search_console",
        LinkMetrics => "link_metrics",
        Amp => "amp",
        NearDuplicates => "near_duplicates",
        Embeddings => "embeddings",
        AiSearch => "ai_search",
    }
}

string_enum! {
    /// Zustand eines Analyse-Teilschritts.
    pub enum StepState {
        Pending => "pending",
        Done => "done",
        Skipped => "skipped",
        NotApplicable => "not_applicable",
    }
}

string_enum! {
    /// Art einer Sitemap-Datei.
    pub enum SitemapKind {
        Urlset => "urlset",
        Index => "index",
        Invalid => "invalid",
    }
}

string_enum! {
    /// Umgang mit mehr als 500 hreflang-Annotationen (TECH-004).
    #[derive(Default)]
    pub enum HreflangOverLimit {
        #[default]
        Truncate => "truncate",
        Skip => "skip",
    }
}

string_enum! {
    /// Zeilenende der CSV-Exporte (OUT-002).
    #[derive(Default)]
    pub enum LineEnding {
        #[default]
        Crlf => "crlf",
        Lf => "lf",
    }
}

/// Zustand eines Filters (ONPAGE-007 / TECH-005). Trägt Daten (`NeedsConfig`) und hat deshalb kein `FromStr`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FilterState {
    /// Zahl liegt vor (live oder nach abgeschlossener Analyse).
    Ready,
    /// Nachlauf-Filter, Crawl-Analyse noch nicht gelaufen („Crawl-Analyse erforderlich“).
    NeedsCrawlAnalysis,
    /// Konfigurationsschlüssel, der `true` sein muss, ist aus.
    NeedsConfig(String),
    /// Nur mit JS-Rendering befüllbar.
    NeedsRendering,
    /// Analyse veraltet (nach „Neu crawlen“/Entfernen).
    Stale,
    NeedsRecrawl,
    Incomplete {
        unchecked: u64,
    },
}

impl FilterState {
    pub fn yields_rows(&self) -> bool {
        matches!(self, Self::Ready | Self::Incomplete { .. })
    }
    /// Kurzname des Zustands (ohne Nutzdaten).
    pub fn as_str(&self) -> &'static str {
        match self {
            FilterState::Ready => "ready",
            FilterState::NeedsCrawlAnalysis => "needs_crawl_analysis",
            FilterState::NeedsConfig(_) => "needs_config",
            FilterState::NeedsRendering => "needs_rendering",
            FilterState::Stale => "stale",
            FilterState::NeedsRecrawl => "needs_recrawl",
            FilterState::Incomplete { .. } => "incomplete",
        }
    }
}

string_enum! {
    /// Grund eines gespeicherten, nicht crawlbaren Verweises (CRAWL-053).
    pub enum UncrawlableReason {
        NonAnchorHref => "non_anchor_href", AnchorOnclickNavigation => "anchor_onclick_navigation",
        JavascriptHref => "javascript_href", AnchorReturnFalse => "anchor_return_false",
        AnchorMissingHref => "anchor_missing_href", AnchorEmptyHref => "anchor_empty_href", AnchorFragmentOnly => "anchor_fragment_only",
    }
}
impl UncrawlableReason {
    /// Anzeigetext für gespeicherte, nicht crawlbare Links (K2-27, OUT-041).
    pub fn label(self) -> &'static str {
        match self {
            Self::NonAnchorHref => "href on non-anchor element",
            Self::AnchorOnclickNavigation => "onclick navigation",
            Self::JavascriptHref => "javascript: URL",
            Self::AnchorReturnFalse => "onclick return false",
            Self::AnchorMissingHref => "missing href",
            Self::AnchorEmptyHref => "empty href",
            Self::AnchorFragmentOnly => "fragment only",
        }
    }
}

string_enum! {
    /// Herkunft eines Cookies (CRAWL-062).
    #[derive(Default)]
    pub enum CookieOrigin { #[default] Http => "http", JavaScript => "javascript" }
}
string_enum! {
    /// Art einer Sitemap-Erweiterung (CRAWL-060).
    pub enum SitemapExtKind { Image => "image", Video => "video", News => "news" }
}

/// Zweck des Abrufs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FetchPurpose {
    /// HTML-Seite oder andere URL.
    Page,
    /// Ressource einer Seite (Bild, CSS, JavaScript …).
    Resource,
    /// Externe URL (Statusprüfung).
    External,
    /// robots.txt.
    RobotsTxt,
    /// XML-Sitemap.
    Sitemap,
}
