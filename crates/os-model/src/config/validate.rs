//! Validierung einer [`CrawlConfig`] (CRAWL-140, OUT-221): Wertebereiche, `min ≤ max`, Regex-Syntax.
//!
//! `validate` lädt oder ändert nichts; es sammelt Fehler (Konfiguration ist so nicht verwendbar) und Warnungen
//! (Konfiguration wird angenommen, der Nutzer bekommt einen Hinweis). Die Bereichstabelle [`INT_RANGES`] ist die
//! einzige Quelle für Ganzzahl-Grenzen: `set` liest sie über das JSON-Schema, das sie als `minimum`/`maximum`
//! einträgt, und `validate` prüft damit die geladenen Werte.

use std::collections::BTreeMap;
use std::fmt;
use std::net::{IpAddr, SocketAddr};

use serde_json::Value;

use crate::config::{CrawlConfig, TextLimits};

/// Art einer Meldung zu einer Konfiguration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConfigIssueKind {
    /// Unbekannter Schlüssel in der Datei (wird ignoriert, OUT-221).
    UnknownField,
    /// `schema_version` ist neuer als die unterstützte Version.
    NewerSchemaVersion,
    /// Ungültiger Wert (Validierungsfehler).
    InvalidValue,
    /// Wert wird angenommen, verdient aber einen Hinweis.
    Warning,
}

/// Eine Meldung mit Feldpfad und – bei geladenen Dateien – Zeile/Spalte des Schlüssels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigIssue {
    /// Maschinenlesbarer Fehlercode.
    pub code: String,
    /// Position im geprüften Ausdruck.
    pub position: Option<u32>,
    /// Art der Meldung.
    pub kind: ConfigIssueKind,
    /// Feldpfad, z. B. `speed.threads` oder `auth.basic[0].prefix`.
    pub path: String,
    /// Zeile des Schlüssels (1-basiert), wenn die Konfiguration aus einer Datei stammt.
    pub line: Option<u32>,
    /// Spalte des Schlüssels (1-basiert).
    pub column: Option<u32>,
    /// Beschreibung (deutsch).
    pub message: String,
}

impl ConfigIssue {
    pub(crate) fn new(
        kind: ConfigIssueKind,
        path: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        ConfigIssue {
            code: format!("{kind:?}"),
            position: None,
            kind,
            path: path.into(),
            line: None,
            column: None,
            message: message.into(),
        }
    }
}

impl fmt::Display for ConfigIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.path)?;
        if let (Some(l), Some(c)) = (self.line, self.column) {
            write!(f, " (Zeile {l}, Spalte {c})")?;
        }
        write!(f, ": {}", self.message)
    }
}

/// Ergebnis von [`CrawlConfig::validate`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ValidationReport {
    /// Fehler: die Konfiguration darf so nicht verwendet bzw. gespeichert werden.
    pub errors: Vec<ConfigIssue>,
    /// Warnungen: die Konfiguration wird angenommen.
    pub warnings: Vec<ConfigIssue>,
}

impl ValidationReport {
    /// Keine Fehler (Warnungen sind erlaubt).
    pub fn is_ok(&self) -> bool {
        self.errors.is_empty()
    }

    /// `Ok(Warnungen)` ohne Fehler, sonst `Err(Fehler)`.
    pub fn into_result(self) -> Result<Vec<ConfigIssue>, Vec<ConfigIssue>> {
        if self.errors.is_empty() {
            Ok(self.warnings)
        } else {
            Err(self.errors)
        }
    }

    pub(crate) fn locate(&mut self, locations: &BTreeMap<String, (u32, u32)>) {
        for i in self.errors.iter_mut().chain(self.warnings.iter_mut()) {
            // Listenelemente (`scope.exclude[0]`) haben keine eigene Position: dann die der Liste.
            let list_path = i.path.rfind('[').map(|at| &i.path[..at]);
            let found = locations
                .get(&i.path)
                .or_else(|| list_path.and_then(|p| locations.get(p)));
            if let Some(&(l, c)) = found {
                i.line = Some(l);
                i.column = Some(c);
            }
        }
    }
}

/// Grenzen einer Ganzzahl-Option (inklusive).
#[derive(Debug, Clone, Copy)]
pub(crate) struct IntRange {
    pub path: &'static str,
    pub min: i64,
    pub max: Option<i64>,
}

const fn r(path: &'static str, min: i64, max: Option<i64>) -> IntRange {
    IntRange { path, min, max }
}

/// Obergrenze für `u64`-Werte: TOML-Ganzzahlen sind `i64`.
pub(crate) const TOML_INT_MAX: i64 = i64::MAX;

/// Wertebereiche aller Ganzzahl-Optionen mit Einschränkung (CRAWL-112: Threads 1–200; ONPAGE-144: 50–100 %).
pub(crate) const INT_RANGES: &[IntRange] = &[
    r("images.incorrect_size_min_saving_kb", 0, Some(1_000_000)),
    r("custom_search.regex_timeout_ms", 1, Some(u32::MAX as i64)),
    r("serp.desktop.title_max_px", 1, Some(4000)),
    r("serp.desktop.description_max_px", 1, Some(4000)),
    r("serp.mobile.title_max_px", 1, Some(4000)),
    r("serp.mobile.description_max_px", 1, Some(4000)),
    r("serp.tablet.title_max_px", 1, Some(4000)),
    r("serp.tablet.description_max_px", 1, Some(4000)),
    r("api.ga4.limits.retries", 0, Some(20)),
    r("api.ga4.limits.concurrency", 1, Some(50)),
    r("api.ga4.limits.window_s", 1, Some(4294967295)),
    r("api.gsc.limits.retries", 0, Some(20)),
    r("api.gsc.limits.concurrency", 1, Some(50)),
    r("api.gsc.limits.window_s", 1, Some(4294967295)),
    r("api.psi.limits.retries", 0, Some(20)),
    r("api.psi.limits.concurrency", 1, Some(50)),
    r("api.psi.limits.window_s", 1, Some(4294967295)),
    r("api.majestic.limits.retries", 0, Some(20)),
    r("api.majestic.limits.concurrency", 1, Some(50)),
    r("api.majestic.limits.window_s", 1, Some(4294967295)),
    r("api.ahrefs.limits.retries", 0, Some(20)),
    r("api.ahrefs.limits.concurrency", 1, Some(50)),
    r("api.ahrefs.limits.window_s", 1, Some(4294967295)),
    r("api.moz.limits.retries", 0, Some(20)),
    r("api.moz.limits.concurrency", 1, Some(50)),
    r("api.moz.limits.window_s", 1, Some(4294967295)),
    r("api.drive.limits.retries", 0, Some(20)),
    r("api.drive.limits.concurrency", 1, Some(50)),
    r("api.drive.limits.window_s", 1, Some(4294967295)),
    r("schema_version", 1, Some(u32::MAX as i64)),
    r("limits.max_urls", 1, Some(TOML_INT_MAX)),
    r("limits.max_page_kb", 0, Some(1 << 30)),
    r("speed.threads", 1, Some(200)),
    r("speed.max_per_host", 1, Some(200)),
    r("advanced.response_timeout_s", 1, Some(3600)),
    r("robots.retries", 0, Some(10)),
    r("robots.retry_delay_s", 0, Some(3600)),
    r("title.max_chars", 1, None),
    r("meta_description.max_chars", 1, None),
    r("url.max_chars", 1, None),
    r("h1.max_chars", 1, None),
    r("h2.max_chars", 1, None),
    r("image.max_kb", 1, None),
    r("image.alt_max_chars", 1, None),
    r("duplicates.near_threshold_pct", 50, Some(100)),
    r("validation.html_doc_max_bytes", 1, Some(TOML_INT_MAX)),
    r("validation.resource_max_bytes", 1, Some(TOML_INT_MAX)),
    // Konfiguration v2 (M2-F01)
    r("limits.max_url_length", 0, Some(1 << 24)),
    r("limits.max_links_per_url", 0, Some(1 << 30)),
    r("limits.max_urls_per_subdomain", 1, Some(TOML_INT_MAX)),
    r("rendering.tabs", 1, Some(64)),
    r("rendering.restart_after_pages", 1, Some(1 << 30)),
    r("rendering.restart_memory_mb", 64, Some(1 << 20)),
    r("rendering.page_timeout_s", 1, Some(3600)),
    r("rendering.viewport.width", 1, Some(16_384)),
    r("rendering.viewport.height", 1, Some(16_384)),
    r("rendering.screenshots.width", 1, Some(16_384)),
    r("rendering.screenshots.height", 1, Some(16_384)),
    r("rendering.screenshots.wait_ms", 0, Some(10_000)),
    r("rendering.screenshots.jpeg_quality", 1, Some(100)),
    r("http.retries_5xx", 0, Some(10)),
    r("http.retries_network", 0, Some(10)),
    r(
        "compare.change_detection.content_threshold_pct",
        0,
        Some(100),
    ),
];

/// Obergrenze der Einträge in Custom Search, Custom Extraction und Custom JavaScript (INTEG-020/040/060).
pub(crate) const MAX_CUSTOM_ENTRIES: usize = 100;

/// Grenzen der Ganzzahl-Option `path`, falls eingeschränkt.
pub(crate) fn int_range(path: &str) -> Option<IntRange> {
    INT_RANGES.iter().copied().find(|x| x.path == path)
}

/// „1–200“ bzw. „≥ 1“.
pub(crate) fn range_text(min: i64, max: Option<i64>) -> String {
    match max {
        Some(max) => format!("{min}–{max}"),
        None => format!("≥ {min}"),
    }
}

fn as_i128(v: &Value) -> Option<i128> {
    v.as_i64()
        .map(i128::from)
        .or_else(|| v.as_u64().map(i128::from))
}

impl CrawlConfig {
    /// Prüft die Konfiguration (CRAWL-140): Wertebereiche, `min ≤ max`, Regex-Syntax, Host-Auflösung,
    /// Basic-Auth-Einträge. Negative oder nicht numerische Werte kann es im Modell nicht geben; sie scheitern
    /// schon beim Laden bzw. in `set` als Typfehler.
    pub fn validate(&self) -> ValidationReport {
        let mut rep = ValidationReport::default();
        let json = serde_json::to_value(self).unwrap_or(Value::Null);

        for range in INT_RANGES {
            let ptr = format!("/{}", range.path.replace('.', "/"));
            let Some(value) = json.pointer(&ptr).and_then(as_i128) else {
                continue; // nicht gesetzte optionale Werte
            };
            let too_small = value < i128::from(range.min);
            let too_big = range.max.is_some_and(|m| value > i128::from(m));
            if too_small || too_big {
                let message = match range.max {
                    Some(max) => {
                        format!("muss zwischen {} und {max} liegen (ist {value})", range.min)
                    }
                    None => format!("muss mindestens {} sein (ist {value})", range.min),
                };
                rep.errors.push(ConfigIssue::new(
                    ConfigIssueKind::InvalidValue,
                    range.path,
                    message,
                ));
            }
        }

        for range in FLOAT_RANGES {
            let ptr = format!("/{}", range.path.replace('.', "/"));
            if !json
                .pointer(&ptr)
                .and_then(Value::as_f64)
                .is_some_and(|n| range.accepts(n))
            {
                rep.errors.push(ConfigIssue::new(
                    ConfigIssueKind::InvalidValue,
                    range.path,
                    "ungültiger Dezimalwert",
                ));
            }
        }
        for (path, entries) in [
            (
                "url.internal_search_segments",
                &self.url.internal_search_segments,
            ),
            (
                "url.internal_search_params",
                &self.url.internal_search_params,
            ),
        ] {
            for (i, entry) in entries.iter().enumerate() {
                if entry.trim().is_empty() {
                    rep.errors.push(ConfigIssue::new(
                        ConfigIssueKind::InvalidValue,
                        format!("{path}[{i}]"),
                        "darf nicht leer sein",
                    ));
                }
            }
        }
        for (host, content) in &self.robots.custom_files {
            let valid = !host.is_empty()
                && host.len() <= 253
                && host.split('.').all(|label| {
                    !label.is_empty()
                        && label.len() <= 63
                        && !label.starts_with('-')
                        && !label.ends_with('-')
                        && label
                            .bytes()
                            .all(|c| c.is_ascii_alphanumeric() || c == b'-')
                });
            if !valid || content.len() > 512 * 1024 {
                rep.errors.push(ConfigIssue::new(
                    ConfigIssueKind::InvalidValue,
                    format!("robots.custom_files.{host}"),
                    "gültiger Hostname und höchstens 512 KiB erforderlich",
                ));
            }
        }

        if let Some(x) = self.speed.max_urls_per_sec {
            if !x.is_finite() || x <= 0.0 {
                rep.errors.push(ConfigIssue::new(
                    ConfigIssueKind::InvalidValue,
                    "speed.max_urls_per_sec",
                    format!("muss eine Zahl größer als 0 sein (ist {x})"),
                ));
            }
        }

        if let crate::config::ConnectionsPerHost::Fixed(n) = self.speed.connections_per_host {
            let max = crate::config::ConnectionsPerHost::MAX;
            if !(1..=max).contains(&n) {
                rep.errors.push(ConfigIssue::new(
                    ConfigIssueKind::InvalidValue,
                    "speed.connections_per_host",
                    format!("muss \"auto\" oder eine Ganzzahl von 1 bis {max} sein (ist {n})"),
                ));
            }
        }

        check_text_limits("title", &self.title, &mut rep);
        check_text_limits("meta_description", &self.meta_description, &mut rep);

        check_patterns("scope.include", &self.scope.include, &mut rep);
        check_patterns("scope.exclude", &self.scope.exclude, &mut rep);

        for (i, entry) in self.network.resolve.iter().enumerate() {
            if let Err(why) = check_resolve(entry) {
                rep.errors.push(ConfigIssue::new(
                    ConfigIssueKind::InvalidValue,
                    format!("network.resolve[{i}]"),
                    why,
                ));
            }
        }

        for (i, a) in self.auth.basic.iter().enumerate() {
            for (field, value) in [
                ("prefix", &a.prefix),
                ("username", &a.username),
                ("secret_ref", &a.secret_ref),
            ] {
                if value.trim().is_empty() {
                    rep.errors.push(ConfigIssue::new(
                        ConfigIssueKind::InvalidValue,
                        format!("auth.basic[{i}].{field}"),
                        "darf nicht leer sein",
                    ));
                }
            }
        }

        if self.user_agent.preset.trim().is_empty() {
            rep.errors.push(ConfigIssue::new(
                ConfigIssueKind::InvalidValue,
                "user_agent.preset",
                "darf nicht leer sein",
            ));
        }
        if let Some(ua) = &self.user_agent.http {
            if ua.trim().is_empty() || ua.chars().any(char::is_control) {
                rep.errors.push(ConfigIssue::new(
                    ConfigIssueKind::InvalidValue,
                    "user_agent.http",
                    "darf nicht leer sein und keine Steuerzeichen (z. B. Zeilenumbruch) enthalten",
                ));
            }
        }
        if let Some(t) = &self.user_agent.robots_token {
            if t.trim().is_empty() || t.chars().any(|c| c.is_control() || c.is_whitespace()) {
                rep.errors.push(ConfigIssue::new(
                    ConfigIssueKind::InvalidValue,
                    "user_agent.robots_token",
                    "darf nicht leer sein und keine Leerzeichen oder Steuerzeichen enthalten",
                ));
            }
        }

        self.validate_v2(&mut rep);

        // CRAWL-140 [P1]: Annehmen, aber warnen – ein Grenzwert über der Seitengröße ist nie erkennbar.
        let page_bytes = i128::from(self.limits.max_page_kb).saturating_mul(1024);
        for (path, value) in [
            (
                "validation.html_doc_max_bytes",
                self.validation.html_doc_max_bytes,
            ),
            (
                "validation.resource_max_bytes",
                self.validation.resource_max_bytes,
            ),
        ] {
            if self.limits.max_page_kb != 0 && i128::from(value) > page_bytes {
                rep.warnings.push(ConfigIssue::new(
                    ConfigIssueKind::Warning,
                    path,
                    "Grenzwert kann nicht erkannt werden, wenn Seiten vorher abgeschnitten werden \
                     (größer als limits.max_page_kb)",
                ));
            }
        }

        rep
    }
}

fn check_text_limits(name: &str, t: &TextLimits, rep: &mut ValidationReport) {
    if t.min_chars > t.max_chars {
        rep.errors.push(ConfigIssue::new(
            ConfigIssueKind::InvalidValue,
            format!("{name}.min_chars"),
            format!(
                "darf nicht größer sein als {name}.max_chars ({} > {})",
                t.min_chars, t.max_chars
            ),
        ));
    }
    if t.min_px > t.max_px {
        rep.errors.push(ConfigIssue::new(
            ConfigIssueKind::InvalidValue,
            format!("{name}.min_px"),
            format!(
                "darf nicht größer sein als {name}.max_px ({} > {})",
                t.min_px, t.max_px
            ),
        ));
    }
}

/// Prüft Regex-Syntax mit `fancy-regex`: roh und in der Vollübereinstimmungs-Form `^(?:…)$`.
fn check_patterns(path: &str, patterns: &[String], rep: &mut ValidationReport) {
    for (i, p) in patterns.iter().enumerate() {
        let message = match fancy_regex::Regex::new(p) {
            Err(e) => Some(format!("ungültiger regulärer Ausdruck: {e}")),
            Ok(_) => fancy_regex::Regex::new(&format!("^(?:{p})$"))
                .err()
                .map(|e| format!("als Vollübereinstimmung nicht verwendbar: {e}")),
        };
        if let Some(message) = message {
            rep.errors.push(ConfigIssue::new(
                ConfigIssueKind::InvalidValue,
                format!("{path}[{i}]"),
                message,
            ));
        }
    }
}

/// `host=ip[:port]`, `*.domain=ip[:port]` (CRAWL-123, K-11).
fn check_resolve(entry: &str) -> Result<(), String> {
    let Some((host, target)) = entry.split_once('=') else {
        return Err(format!("{entry:?}: Format host=ip[:port] erwartet"));
    };
    let host = host.trim();
    let name = host.strip_prefix("*.").unwrap_or(host);
    let host_ok = !name.is_empty()
        && !name.starts_with('.')
        && !name.ends_with('.')
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '-' | '.' | '_'));
    if !host_ok {
        return Err(format!(
            "{entry:?}: Host oder *.domain erwartet (ungültiger Host {host:?})"
        ));
    }
    let target = target.trim();
    let bare_v6 = target
        .strip_prefix('[')
        .and_then(|t| t.strip_suffix(']'))
        .unwrap_or(target);
    if bare_v6.parse::<IpAddr>().is_ok() || target.parse::<SocketAddr>().is_ok() {
        Ok(())
    } else {
        Err(format!(
            "{entry:?}: IP-Adresse (optional mit :Port) erwartet (ungültig: {target:?})"
        ))
    }
}

fn bad(rep: &mut ValidationReport, path: impl Into<String>, message: impl Into<String>) {
    rep.errors.push(ConfigIssue::new(
        ConfigIssueKind::InvalidValue,
        path,
        message,
    ));
}

fn warn(rep: &mut ValidationReport, path: impl Into<String>, message: impl Into<String>) {
    rep.warnings
        .push(ConfigIssue::new(ConfigIssueKind::Warning, path, message));
}

fn check_regex_list<'a>(
    path: &str,
    patterns: impl IntoIterator<Item = &'a String>,
    rep: &mut ValidationReport,
) {
    for (i, p) in patterns.into_iter().enumerate() {
        if let Err(e) = fancy_regex::Regex::new(p) {
            bad(
                rep,
                format!("{path}[{i}]"),
                format!("ungültiger regulärer Ausdruck: {e}"),
            );
        }
    }
}

fn is_hex_color(s: &str) -> bool {
    s.len() == 7 && s.starts_with('#') && s[1..].chars().all(|c| c.is_ascii_hexdigit())
}

impl CrawlConfig {
    /// Prüfungen der Abschnitte ab Konfiguration v2.
    fn validate_v2(&self, rep: &mut ValidationReport) {
        use crate::enums::RenderingMode;
        use crate::enums_m2::{
            ExtractorMode, JsSnippetKind, LastmodSource, SearchKind, SearchScope, SegmentOp,
            SpellingMode,
        };

        // Rendering: Optionen, die den JavaScript-Modus voraussetzen, sind im Textmodus wirkungslos (Hinweis).
        if self.rendering.mode == RenderingMode::Text {
            for (path, on) in [
                ("extraction.accessibility", self.extraction.accessibility),
                (
                    "extraction.store_rendered_html",
                    self.extraction.store_rendered_html,
                ),
            ] {
                if on {
                    warn(rep, path, "wirkt nur mit rendering.mode = \"javascript\"");
                }
            }
            if !self.custom_js.snippets.is_empty() {
                warn(
                    rep,
                    "custom_js.snippets",
                    "Snippets laufen nur mit rendering.mode = \"javascript\"",
                );
            }
        }

        // Spelling
        if self.spelling.mode == SpellingMode::Manual
            && self.spelling.manual_language.trim().is_empty()
        {
            bad(
                rep,
                "spelling.manual_language",
                "darf bei mode = \"manual\" nicht leer sein",
            );
        }

        // Custom Search (INTEG-020)
        if self.custom_search.filters.len() > MAX_CUSTOM_ENTRIES {
            bad(rep, "custom_search.filters", "Maximal 100 Suchfilter");
        }
        let mut names = std::collections::BTreeSet::new();
        for (i, f) in self.custom_search.filters.iter().enumerate() {
            let name = if f.name.trim().is_empty() {
                format!("Filter {}", i + 1)
            } else {
                f.name.trim().to_string()
            };
            if !names.insert(name.to_lowercase()) {
                bad(
                    rep,
                    format!("custom_search.filters[{i}].name"),
                    "Name ist nicht eindeutig",
                );
            }
            if f.expression.trim().is_empty() {
                bad(
                    rep,
                    format!("custom_search.filters[{i}].expression"),
                    "darf nicht leer sein",
                );
            } else if f.kind == SearchKind::Regex {
                if let Err(e) = fancy_regex::Regex::new(&f.expression) {
                    bad(
                        rep,
                        format!("custom_search.filters[{i}].expression"),
                        format!("ungültiger regulärer Ausdruck: {e}"),
                    );
                }
            }
            if f.scope == SearchScope::XPath
                && f.xpath.as_deref().is_none_or(|x| x.trim().is_empty())
            {
                bad(
                    rep,
                    format!("custom_search.filters[{i}].xpath"),
                    "bei scope = \"xpath\" erforderlich",
                );
            }
        }

        // Custom Extraction (INTEG-040)
        if self.custom_extraction.extractors.len() > MAX_CUSTOM_ENTRIES {
            bad(
                rep,
                "custom_extraction.extractors",
                "Maximal 100 Extraktoren",
            );
        }
        let mut names = std::collections::BTreeSet::new();
        for (i, e) in self.custom_extraction.extractors.iter().enumerate() {
            let name = if e.name.trim().is_empty() {
                format!("Extractor {}", i + 1)
            } else {
                e.name.trim().to_string()
            };
            if !names.insert(name.to_lowercase()) {
                bad(
                    rep,
                    format!("custom_extraction.extractors[{i}].name"),
                    "Name ist nicht eindeutig",
                );
            }
            if e.expression.trim().is_empty() {
                bad(
                    rep,
                    format!("custom_extraction.extractors[{i}].expression"),
                    "darf nicht leer sein",
                );
            } else if e.mode == ExtractorMode::Regex {
                if let Err(err) = fancy_regex::Regex::new(&e.expression) {
                    bad(
                        rep,
                        format!("custom_extraction.extractors[{i}].expression"),
                        format!("ungültiger regulärer Ausdruck: {err}"),
                    );
                }
            }
        }

        // Custom JavaScript (INTEG-060, INTEG-062)
        if self.custom_js.snippets.len() > MAX_CUSTOM_ENTRIES {
            bad(rep, "custom_js.snippets", "Maximal 100 Snippets");
        }
        let mut names = std::collections::BTreeSet::new();
        for (i, sn) in self.custom_js.snippets.iter().enumerate() {
            let name = if sn.name.trim().is_empty() {
                format!("Snippet {}", i + 1)
            } else {
                sn.name.trim().to_string()
            };
            if !names.insert(name.to_lowercase()) {
                bad(
                    rep,
                    format!("custom_js.snippets[{i}].name"),
                    "Name ist nicht eindeutig",
                );
            }
            if sn.kind == JsSnippetKind::Action && !(1..=120).contains(&sn.timeout_s) {
                bad(
                    rep,
                    format!("custom_js.snippets[{i}].timeout_s"),
                    format!(
                        "Aktions-Snippets brauchen ein Zeitlimit von 1 bis 120 Sekunden (ist {})",
                        sn.timeout_s
                    ),
                );
            }
        }

        // Link-Positionen (INTEG-081): Auffangregel zuletzt
        match self.link_positions.rules.last() {
            None => bad(
                rep,
                "link_positions.rules",
                "mindestens die Auffangregel \"Content\" erforderlich",
            ),
            Some(last) => {
                if last.position != "Content" || !last.contains.iter().all(String::is_empty) {
                    bad(
                        rep,
                        "link_positions.rules",
                        "die letzte Regel muss Content mit leerer Teilstringliste sein",
                    );
                }
            }
        }
        for (i, r) in self.link_positions.rules.iter().enumerate() {
            if i + 1 < self.link_positions.rules.len()
                && (r.position == "Content"
                    || r.contains.is_empty()
                    || r.contains.iter().any(String::is_empty))
            {
                bad(
                    rep,
                    format!("link_positions.rules[{i}]"),
                    "vor der Auffangregel sind Content und leere Teilstrings unzulässig",
                );
            }
            if r.position.trim().is_empty() {
                bad(
                    rep,
                    format!("link_positions.rules[{i}].position"),
                    "darf nicht leer sein",
                );
            }
        }

        // Umschreibung, CDNs, Mapping, Sitemap-Ausschlüsse
        for (i, r) in self.rewrite.regex_replace.iter().enumerate() {
            if let Err(e) = fancy_regex::Regex::new(&r.pattern) {
                bad(
                    rep,
                    format!("rewrite.regex_replace[{i}].pattern"),
                    format!("ungültiger regulärer Ausdruck: {e}"),
                );
            }
        }
        for (i, r) in self.compare.mapping.iter().enumerate() {
            if let Err(e) = fancy_regex::Regex::new(&r.pattern) {
                bad(
                    rep,
                    format!("compare.mapping[{i}].pattern"),
                    format!("ungültiger regulärer Ausdruck: {e}"),
                );
            }
        }
        check_regex_list(
            "sitemap_gen.exclude_patterns",
            &self.sitemap_gen.exclude_patterns,
            rep,
        );
        check_regex_list(
            "sitemap_gen.images.external_regex",
            &self.sitemap_gen.images.external_regex,
            rep,
        );
        for (i, c) in self.cdns.iter().enumerate() {
            if c.trim().is_empty() || c.chars().any(|ch| ch.is_whitespace() || ch.is_control()) {
                bad(
                    rep,
                    format!("cdns[{i}]"),
                    "Host oder Host/Pfad ohne Leerzeichen erwartet",
                );
            }
        }

        // HTTP
        for (i, h) in self.http.headers.iter().enumerate() {
            let token_ok = !h.name.is_empty()
                && h.name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "!#$%&'*+-.^_`|~".contains(c));
            if !token_ok {
                bad(
                    rep,
                    format!("http.headers[{i}].name"),
                    "ungültiger Header-Name",
                );
            }
            if h.value.chars().any(|c| c == '\r' || c == '\n' || c == '\0') {
                bad(
                    rep,
                    format!("http.headers[{i}].value"),
                    "darf keine Zeilenumbrüche enthalten",
                );
            }
        }
        if let Some(al) = &self.http.accept_language {
            if al.trim().is_empty() || al.chars().any(char::is_control) {
                bad(
                    rep,
                    "http.accept_language",
                    "darf nicht leer sein und keine Steuerzeichen enthalten",
                );
            }
        }

        // Segmente (CRAWL-134)
        let mut seen: Vec<String> = Vec::new();
        for (i, seg) in self.segments.list.iter().enumerate() {
            let n = seg.name.chars().count();
            if !(1..=64).contains(&n) {
                bad(
                    rep,
                    format!("segments.list[{i}].name"),
                    "1 bis 64 Zeichen erwartet",
                );
            }
            let lower = seg.name.to_lowercase();
            if seen.contains(&lower) {
                bad(
                    rep,
                    format!("segments.list[{i}].name"),
                    "Name ist ohne Beachtung der Groß-/Kleinschreibung nicht eindeutig",
                );
            }
            if !seg.color.is_empty() && !is_hex_color(&seg.color) {
                bad(
                    rep,
                    format!("segments.list[{i}].color"),
                    "Farbe als #rrggbb erwartet",
                );
            }
            for (g, group) in seg.groups.iter().enumerate() {
                for (c, cond) in group.conditions.iter().enumerate() {
                    let path = format!("segments.list[{i}].groups[{g}].conditions[{c}]");
                    match cond.op {
                        SegmentOp::RegexMatches | SegmentOp::RegexNotMatches => {
                            if let Err(e) = fancy_regex::Regex::new(&cond.value) {
                                bad(
                                    rep,
                                    format!("{path}.value"),
                                    format!("ungültiger regulärer Ausdruck: {e}"),
                                );
                            }
                        }
                        SegmentOp::InSegment if !seen.contains(&cond.value.to_lowercase()) => {
                            bad(
                                rep,
                                format!("{path}.value"),
                                "\"ist in Segment\" darf nur auf weiter oben stehende Segmente verweisen",
                            );
                        }
                        _ => {}
                    }
                }
            }
            seen.push(lower);
        }

        // Sitemap-Generator (OUT-123 … 125)
        let sg = &self.sitemap_gen;
        if sg.lastmod.source == LastmodSource::Custom
            && sg.lastmod.enabled
            && sg
                .lastmod
                .custom_date
                .as_deref()
                .is_none_or(|d| d.trim().is_empty())
        {
            bad(
                rep,
                "sitemap_gen.lastmod.custom_date",
                "bei source = \"custom\" erforderlich",
            );
        }
        if sg.priority.by_depth.is_empty()
            || sg
                .priority
                .by_depth
                .iter()
                .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
        {
            bad(
                rep,
                "sitemap_gen.priority.by_depth",
                "mindestens ein Wert, alle zwischen 0.0 und 1.0",
            );
        }
        if sg.changefreq.by_depth.is_empty() {
            bad(
                rep,
                "sitemap_gen.changefreq.by_depth",
                "mindestens ein Wert erforderlich",
            );
        }

        // Change Detection (TECH-300): jedes Element höchstens einmal, in Filterreihenfolge
        let order: Vec<_> = self
            .compare
            .change_detection
            .elements
            .iter()
            .map(|e| {
                crate::enums_m2::ChangeElement::ALL
                    .iter()
                    .position(|x| x == e)
            })
            .collect();
        if order.windows(2).any(|w| w[0] >= w[1]) {
            bad(
                rep,
                "compare.change_detection.elements",
                "Elemente höchstens einmal und in der Reihenfolge von TECH-300",
            );
        }
        if self.rendering.screenshots.fit_to_content {
            // Höhe an den Inhalt anpassen: feste Höhe wirkungslos, kein Fehler
            if self.rendering.screenshots.height.is_some() {
                warn(
                    rep,
                    "rendering.screenshots.height",
                    "bei fit_to_content = true bestimmt der Inhalt die Höhe (höchstens 8 192 px)",
                );
            }
        }
    }
}

/// Gemeinsame Grenzen für Schema, --set und Validierung.
#[derive(Clone, Copy)]
pub(crate) struct FloatRange {
    pub path: &'static str,
    pub min: f64,
    pub max: f64,
    pub multiple: Option<f64>,
}
impl FloatRange {
    pub fn accepts(self, n: f64) -> bool {
        n.is_finite()
            && n >= self.min
            && n <= self.max
            && self
                .multiple
                .is_none_or(|m| (n / m - (n / m).round()).abs() <= 1e-9)
    }
}
pub(crate) const FLOAT_RANGES: &[FloatRange] = &[
    FloatRange {
        path: "rendering.ajax_timeout_s",
        min: 0.0,
        max: 60.0,
        multiple: Some(0.1),
    },
    FloatRange {
        path: "serp.desktop.title_font_px",
        min: 6.0,
        max: 64.0,
        multiple: None,
    },
    FloatRange {
        path: "serp.desktop.description_font_px",
        min: 6.0,
        max: 64.0,
        multiple: None,
    },
    FloatRange {
        path: "serp.mobile.title_font_px",
        min: 6.0,
        max: 64.0,
        multiple: None,
    },
    FloatRange {
        path: "serp.mobile.description_font_px",
        min: 6.0,
        max: 64.0,
        multiple: None,
    },
    FloatRange {
        path: "serp.tablet.title_font_px",
        min: 6.0,
        max: 64.0,
        multiple: None,
    },
    FloatRange {
        path: "serp.tablet.description_font_px",
        min: 6.0,
        max: 64.0,
        multiple: None,
    },
];
