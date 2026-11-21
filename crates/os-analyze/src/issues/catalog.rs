//! Parsen und Prüfen des Katalogs (UI-063): gültige Werte, Regelcodes, Präfixe, `override_reason`.

use std::collections::{BTreeMap, BTreeSet};

use os_model::{IssuePriority, IssueType};
use serde::Deserialize;

use super::render;
use super::CatalogEntry;

/// Gültige Typ-Regeln (UI-063) mit dem Typ, den sie ergeben.
pub const TYPE_RULES: &[(&str, IssueType)] = &[
    ("T-I1", IssueType::Issue),
    ("T-I2", IssueType::Issue),
    ("T-I3", IssueType::Issue),
    ("T-I4", IssueType::Issue),
    ("T-W1", IssueType::Warning),
    ("T-W2", IssueType::Warning),
    ("T-W3", IssueType::Warning),
    ("T-O1", IssueType::Opportunity),
    ("T-O2", IssueType::Opportunity),
];

/// Gültige Prioritäts-Regeln (UI-063) mit der Priorität, die sie ergeben.
pub const PRIORITY_RULES: &[(&str, IssuePriority)] = &[
    ("P-H1", IssuePriority::High),
    ("P-H2", IssuePriority::High),
    ("P-H3", IssuePriority::High),
    ("P-M1", IssuePriority::Medium),
    ("P-M2", IssuePriority::Medium),
    ("P-L1", IssuePriority::Low),
    ("P-L2", IssuePriority::Low),
];

/// Erlaubte Anfänge einer `override_reason` (UI-063: u. a. `benchmark-parity`).
pub const OVERRIDE_REASON_PREFIXES: &[&str] = &["benchmark-parity"];

/// Erlaubte Platzhalter in `text_params` und in den Texten.
pub const ALLOWED_PLACEHOLDERS: &[&str] = &["max", "min", "unit", "threshold"];

/// Fehler beim Lesen der Katalogdatei.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CatalogError {
    /// Kein gültiges TOML oder unzulässiger Wert (z. B. Typ außerhalb Issue/Warning/Opportunity).
    #[error("Katalog nicht lesbar: {0}")]
    Parse(String),
}

/// Ein Verstoß gegen die Katalogregeln.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogProblem {
    /// Eintrag (`id`), bei Dateiproblemen leer.
    pub id: String,
    /// Beschreibung.
    pub message: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawCatalog {
    #[serde(default)]
    #[allow(dead_code)]
    schema: u32,
    #[serde(default)]
    issue: Vec<RawEntry>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawEntry {
    id: String,
    tab: String,
    filter: String,
    #[serde(rename = "type")]
    issue_type: IssueType,
    priority: IssuePriority,
    type_rule: String,
    priority_rule: String,
    #[serde(default)]
    override_reason: Option<String>,
    #[serde(default)]
    needs_crawl_analysis: bool,
    doc_key: String,
    #[serde(default)]
    text_params: BTreeMap<String, String>,
}

/// Liest einen Katalog aus TOML. Typ und Priorität müssen gültige Schlüssel sein.
pub fn parse_catalog(src: &str) -> Result<Vec<CatalogEntry>, CatalogError> {
    let raw: RawCatalog = toml::from_str(src).map_err(|e| CatalogError::Parse(e.to_string()))?;
    Ok(raw
        .issue
        .into_iter()
        .map(|r| CatalogEntry {
            id: r.id,
            tab: r.tab,
            filter: r.filter,
            issue_type: r.issue_type,
            priority: r.priority,
            type_rule: r.type_rule,
            priority_rule: r.priority_rule,
            override_reason: r.override_reason,
            needs_crawl_analysis: r.needs_crawl_analysis,
            doc_key: r.doc_key,
            text_params: r.text_params.into_iter().collect(),
        })
        .collect())
}

fn is_slug(s: &str) -> bool {
    !s.is_empty()
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// Prüft die Katalogregeln aus UI-063; leere Liste = in Ordnung.
///
/// - `id`/`doc_key` sind Kleinbuchstaben-Kennungen und eindeutig, `(tab, filter)` ebenfalls,
/// - `type_rule`/`priority_rule` stammen aus den Tabellen von UI-063,
/// - passt der Präfix der Regel nicht zu Typ bzw. Priorität (`T-I*` ↔ Issue, `P-H*` ↔ High …), ist eine
///   `override_reason` Pflicht; umgekehrt braucht ein Eintrag ohne Abweichung keine,
/// - Platzhalter und Konfigurationspfade in `text_params` und im Filternamen sind gültig.
pub fn validate(entries: &[CatalogEntry]) -> Vec<CatalogProblem> {
    let mut problems = Vec::new();
    let mut add = |id: &str, message: String| {
        problems.push(CatalogProblem {
            id: id.to_string(),
            message,
        })
    };
    let mut ids = BTreeSet::new();
    let mut keys = BTreeSet::new();
    let mut docs = BTreeSet::new();
    for e in entries {
        if !is_slug(&e.id) {
            add(&e.id, "id muss aus a-z, 0-9 und - bestehen".into());
        }
        if !ids.insert(e.id.clone()) {
            add(&e.id, "doppelte id".into());
        }
        if !is_slug(&e.doc_key) {
            add(&e.id, "doc_key muss aus a-z, 0-9 und - bestehen".into());
        }
        if !docs.insert(e.doc_key.clone()) {
            add(&e.id, "doppelter doc_key".into());
        }
        if !keys.insert((e.tab.to_lowercase(), e.filter.to_lowercase())) {
            add(&e.id, "Tab und Filter kommen doppelt vor".into());
        }

        let rule_type = TYPE_RULES
            .iter()
            .find(|(c, _)| *c == e.type_rule)
            .map(|(_, t)| *t);
        let rule_prio = PRIORITY_RULES
            .iter()
            .find(|(c, _)| *c == e.priority_rule)
            .map(|(_, p)| *p);
        if rule_type.is_none() {
            add(&e.id, format!("unbekannte type_rule {:?}", e.type_rule));
        }
        if rule_prio.is_none() {
            add(
                &e.id,
                format!("unbekannte priority_rule {:?}", e.priority_rule),
            );
        }
        let deviates = rule_type.is_some_and(|t| t != e.issue_type)
            || rule_prio.is_some_and(|p| p != e.priority);
        match (&e.override_reason, deviates) {
            (None, true) => add(
                &e.id,
                format!(
                    "Typ/Priorität {}/{} weichen von den Regeln {}/{} ab: override_reason fehlt",
                    e.issue_type, e.priority, e.type_rule, e.priority_rule
                ),
            ),
            (Some(reason), true) => {
                if !OVERRIDE_REASON_PREFIXES
                    .iter()
                    .any(|p| reason.starts_with(p))
                {
                    add(
                        &e.id,
                        format!("override_reason {reason:?} nennt keine erlaubte Begründung"),
                    );
                }
            }
            (Some(_), false) => add(
                &e.id,
                "override_reason ohne Abweichung von den Regeln".into(),
            ),
            (None, false) => {}
        }

        for (placeholder, path) in &e.text_params {
            if !ALLOWED_PLACEHOLDERS.contains(&placeholder.as_str()) {
                add(&e.id, format!("unzulässiger Platzhalter {{{placeholder}}}"));
            }
            if render::default_value(path).is_none() {
                add(
                    &e.id,
                    format!("Konfigurationspfad {path:?} existiert nicht oder ist kein Skalar"),
                );
            }
        }
        for path in render::filter_paths(&e.filter) {
            if render::default_value(&path).is_none() {
                add(
                    &e.id,
                    format!("Filtername verweist auf unbekannten Pfad {path:?}"),
                );
            }
        }
    }
    problems
}

/// Anteil der Einträge mit `override_reason` (UI-063: Ziel < 10 %).
pub fn override_share(entries: &[CatalogEntry]) -> f64 {
    if entries.is_empty() {
        return 0.0;
    }
    let n = entries
        .iter()
        .filter(|e| e.override_reason.is_some())
        .count();
    n as f64 / entries.len() as f64
}

// ---------------------------------------------------------------------------------------------------------------
// Loader (M2-F02): M1-Datei plus je Tab eine Datei in `catalog.d/` und `texts/{de,en}.d/`

const CATALOG_M1: &str = include_str!("../../../../core/issues/catalog.toml");
const TEXTS_DE_M1: &str = include_str!("../../../../core/issues/texts/de.toml");
const TEXTS_EN_M1: &str = include_str!("../../../../core/issues/texts/en.toml");

/// Die Dateien eines Tabs ab M2: Katalog und Texte (de, en). Der Name `module` ist der Modulname des Tabs in
/// `tabs/` (`page_titles`) und der Dateiname ohne Endung.
#[derive(Debug, Clone, Copy)]
pub struct SplitFiles {
    /// Modulname des Tabs.
    pub module: &'static str,
    /// Inhalt von `core/issues/catalog.d/<module>.toml`.
    pub catalog: &'static str,
    /// Inhalt von `core/issues/texts/de.d/<module>.toml`.
    pub de: &'static str,
    /// Inhalt von `core/issues/texts/en.d/<module>.toml`.
    pub en: &'static str,
}

/// Alle Dateien ab M2, ein Eintrag je Tab in der Reihenfolge von `tabs::all()`. Eine neue Datei in einem der Ordner
/// braucht einen Eintrag hier (der Test `catalog_split.rs` prüft Ordner und Liste gegeneinander).
pub const SPLIT_FILES: &[SplitFiles] = &[
    SplitFiles {
        module: "internal",
        catalog: include_str!("../../../../core/issues/catalog.d/internal.toml"),
        de: include_str!("../../../../core/issues/texts/de.d/internal.toml"),
        en: include_str!("../../../../core/issues/texts/en.d/internal.toml"),
    },
    SplitFiles {
        module: "external",
        catalog: include_str!("../../../../core/issues/catalog.d/external.toml"),
        de: include_str!("../../../../core/issues/texts/de.d/external.toml"),
        en: include_str!("../../../../core/issues/texts/en.d/external.toml"),
    },
    SplitFiles {
        module: "security",
        catalog: include_str!("../../../../core/issues/catalog.d/security.toml"),
        de: include_str!("../../../../core/issues/texts/de.d/security.toml"),
        en: include_str!("../../../../core/issues/texts/en.d/security.toml"),
    },
    SplitFiles {
        module: "response_codes",
        catalog: include_str!("../../../../core/issues/catalog.d/response_codes.toml"),
        de: include_str!("../../../../core/issues/texts/de.d/response_codes.toml"),
        en: include_str!("../../../../core/issues/texts/en.d/response_codes.toml"),
    },
    SplitFiles {
        module: "url",
        catalog: include_str!("../../../../core/issues/catalog.d/url.toml"),
        de: include_str!("../../../../core/issues/texts/de.d/url.toml"),
        en: include_str!("../../../../core/issues/texts/en.d/url.toml"),
    },
    SplitFiles {
        module: "page_titles",
        catalog: include_str!("../../../../core/issues/catalog.d/page_titles.toml"),
        de: include_str!("../../../../core/issues/texts/de.d/page_titles.toml"),
        en: include_str!("../../../../core/issues/texts/en.d/page_titles.toml"),
    },
    SplitFiles {
        module: "meta_description",
        catalog: include_str!("../../../../core/issues/catalog.d/meta_description.toml"),
        de: include_str!("../../../../core/issues/texts/de.d/meta_description.toml"),
        en: include_str!("../../../../core/issues/texts/en.d/meta_description.toml"),
    },
    SplitFiles {
        module: "meta_keywords",
        catalog: include_str!("../../../../core/issues/catalog.d/meta_keywords.toml"),
        de: include_str!("../../../../core/issues/texts/de.d/meta_keywords.toml"),
        en: include_str!("../../../../core/issues/texts/en.d/meta_keywords.toml"),
    },
    SplitFiles {
        module: "h1",
        catalog: include_str!("../../../../core/issues/catalog.d/h1.toml"),
        de: include_str!("../../../../core/issues/texts/de.d/h1.toml"),
        en: include_str!("../../../../core/issues/texts/en.d/h1.toml"),
    },
    SplitFiles {
        module: "h2",
        catalog: include_str!("../../../../core/issues/catalog.d/h2.toml"),
        de: include_str!("../../../../core/issues/texts/de.d/h2.toml"),
        en: include_str!("../../../../core/issues/texts/en.d/h2.toml"),
    },
    SplitFiles {
        module: "content",
        catalog: include_str!("../../../../core/issues/catalog.d/content.toml"),
        de: include_str!("../../../../core/issues/texts/de.d/content.toml"),
        en: include_str!("../../../../core/issues/texts/en.d/content.toml"),
    },
    SplitFiles {
        module: "images",
        catalog: include_str!("../../../../core/issues/catalog.d/images.toml"),
        de: include_str!("../../../../core/issues/texts/de.d/images.toml"),
        en: include_str!("../../../../core/issues/texts/en.d/images.toml"),
    },
    SplitFiles {
        module: "canonicals",
        catalog: include_str!("../../../../core/issues/catalog.d/canonicals.toml"),
        de: include_str!("../../../../core/issues/texts/de.d/canonicals.toml"),
        en: include_str!("../../../../core/issues/texts/en.d/canonicals.toml"),
    },
    SplitFiles {
        module: "pagination",
        catalog: include_str!("../../../../core/issues/catalog.d/pagination.toml"),
        de: include_str!("../../../../core/issues/texts/de.d/pagination.toml"),
        en: include_str!("../../../../core/issues/texts/en.d/pagination.toml"),
    },
    SplitFiles {
        module: "directives",
        catalog: include_str!("../../../../core/issues/catalog.d/directives.toml"),
        de: include_str!("../../../../core/issues/texts/de.d/directives.toml"),
        en: include_str!("../../../../core/issues/texts/en.d/directives.toml"),
    },
    SplitFiles {
        module: "hreflang",
        catalog: include_str!("../../../../core/issues/catalog.d/hreflang.toml"),
        de: include_str!("../../../../core/issues/texts/de.d/hreflang.toml"),
        en: include_str!("../../../../core/issues/texts/en.d/hreflang.toml"),
    },
    SplitFiles {
        module: "javascript",
        catalog: include_str!("../../../../core/issues/catalog.d/javascript.toml"),
        de: include_str!("../../../../core/issues/texts/de.d/javascript.toml"),
        en: include_str!("../../../../core/issues/texts/en.d/javascript.toml"),
    },
    SplitFiles {
        module: "links",
        catalog: include_str!("../../../../core/issues/catalog.d/links.toml"),
        de: include_str!("../../../../core/issues/texts/de.d/links.toml"),
        en: include_str!("../../../../core/issues/texts/en.d/links.toml"),
    },
    SplitFiles {
        module: "structured_data",
        catalog: include_str!("../../../../core/issues/catalog.d/structured_data.toml"),
        de: include_str!("../../../../core/issues/texts/de.d/structured_data.toml"),
        en: include_str!("../../../../core/issues/texts/en.d/structured_data.toml"),
    },
    SplitFiles {
        module: "sitemaps",
        catalog: include_str!("../../../../core/issues/catalog.d/sitemaps.toml"),
        de: include_str!("../../../../core/issues/texts/de.d/sitemaps.toml"),
        en: include_str!("../../../../core/issues/texts/en.d/sitemaps.toml"),
    },
    SplitFiles {
        module: "pagespeed",
        catalog: include_str!("../../../../core/issues/catalog.d/pagespeed.toml"),
        de: include_str!("../../../../core/issues/texts/de.d/pagespeed.toml"),
        en: include_str!("../../../../core/issues/texts/en.d/pagespeed.toml"),
    },
    SplitFiles {
        module: "mobile",
        catalog: include_str!("../../../../core/issues/catalog.d/mobile.toml"),
        de: include_str!("../../../../core/issues/texts/de.d/mobile.toml"),
        en: include_str!("../../../../core/issues/texts/en.d/mobile.toml"),
    },
    SplitFiles {
        module: "accessibility",
        catalog: include_str!("../../../../core/issues/catalog.d/accessibility.toml"),
        de: include_str!("../../../../core/issues/texts/de.d/accessibility.toml"),
        en: include_str!("../../../../core/issues/texts/en.d/accessibility.toml"),
    },
    SplitFiles {
        module: "custom_search",
        catalog: include_str!("../../../../core/issues/catalog.d/custom_search.toml"),
        de: include_str!("../../../../core/issues/texts/de.d/custom_search.toml"),
        en: include_str!("../../../../core/issues/texts/en.d/custom_search.toml"),
    },
    SplitFiles {
        module: "custom_extraction",
        catalog: include_str!("../../../../core/issues/catalog.d/custom_extraction.toml"),
        de: include_str!("../../../../core/issues/texts/de.d/custom_extraction.toml"),
        en: include_str!("../../../../core/issues/texts/en.d/custom_extraction.toml"),
    },
    SplitFiles {
        module: "custom_js",
        catalog: include_str!("../../../../core/issues/catalog.d/custom_js.toml"),
        de: include_str!("../../../../core/issues/texts/de.d/custom_js.toml"),
        en: include_str!("../../../../core/issues/texts/en.d/custom_js.toml"),
    },
    SplitFiles {
        module: "analytics",
        catalog: include_str!("../../../../core/issues/catalog.d/analytics.toml"),
        de: include_str!("../../../../core/issues/texts/de.d/analytics.toml"),
        en: include_str!("../../../../core/issues/texts/en.d/analytics.toml"),
    },
    SplitFiles {
        module: "search_console",
        catalog: include_str!("../../../../core/issues/catalog.d/search_console.toml"),
        de: include_str!("../../../../core/issues/texts/de.d/search_console.toml"),
        en: include_str!("../../../../core/issues/texts/en.d/search_console.toml"),
    },
    SplitFiles {
        module: "validation",
        catalog: include_str!("../../../../core/issues/catalog.d/validation.toml"),
        de: include_str!("../../../../core/issues/texts/de.d/validation.toml"),
        en: include_str!("../../../../core/issues/texts/en.d/validation.toml"),
    },
    SplitFiles {
        module: "link_metrics",
        catalog: include_str!("../../../../core/issues/catalog.d/link_metrics.toml"),
        de: include_str!("../../../../core/issues/texts/de.d/link_metrics.toml"),
        en: include_str!("../../../../core/issues/texts/en.d/link_metrics.toml"),
    },
    SplitFiles {
        module: "ai",
        catalog: include_str!("../../../../core/issues/catalog.d/ai.toml"),
        de: include_str!("../../../../core/issues/texts/de.d/ai.toml"),
        en: include_str!("../../../../core/issues/texts/en.d/ai.toml"),
    },
    SplitFiles {
        module: "ai_decisions",
        catalog: include_str!("../../../../core/issues/catalog.d/ai_decisions.toml"),
        de: include_str!("../../../../core/issues/texts/de.d/ai_decisions.toml"),
        en: include_str!("../../../../core/issues/texts/en.d/ai_decisions.toml"),
    },
    SplitFiles {
        module: "ai_suggestions",
        catalog: include_str!("../../../../core/issues/catalog.d/ai_suggestions.toml"),
        de: include_str!("../../../../core/issues/texts/de.d/ai_suggestions.toml"),
        en: include_str!("../../../../core/issues/texts/en.d/ai_suggestions.toml"),
    },
    SplitFiles {
        module: "ai_search",
        catalog: include_str!("../../../../core/issues/catalog.d/ai_search.toml"),
        de: include_str!("../../../../core/issues/texts/de.d/ai_search.toml"),
        en: include_str!("../../../../core/issues/texts/en.d/ai_search.toml"),
    },
    SplitFiles {
        module: "change_detection",
        catalog: include_str!("../../../../core/issues/catalog.d/change_detection.toml"),
        de: include_str!("../../../../core/issues/texts/de.d/change_detection.toml"),
        en: include_str!("../../../../core/issues/texts/en.d/change_detection.toml"),
    },
];

/// Liest den ganzen Katalog: zuerst die M1-Datei, dann die Dateien je Tab in der Reihenfolge von [`SPLIT_FILES`].
pub(crate) fn load_all() -> Result<Vec<CatalogEntry>, CatalogError> {
    let mut entries = parse_catalog(CATALOG_M1)?;
    for f in SPLIT_FILES {
        entries.extend(
            parse_catalog(f.catalog)
                .map_err(|e| CatalogError::Parse(format!("catalog.d/{}.toml: {e}", f.module)))?,
        );
    }
    Ok(entries)
}

/// Die M1-Datei des Katalogs allein (für Tests der Aufteilung).
pub fn m1_source() -> &'static str {
    CATALOG_M1
}

/// Texte einer Sprache als ein TOML-Dokument: M1-Datei, dann die Dateien je Tab. Die Dateien enthalten nur Tabellen
/// `[issues.<id>]`; zusammenhängend gelesen ergibt das ein gültiges Dokument (doppelte Kennungen scheitern beim Parsen).
pub(crate) fn texts_all(lang: super::Lang) -> &'static str {
    use std::sync::OnceLock;
    static DE: OnceLock<String> = OnceLock::new();
    static EN: OnceLock<String> = OnceLock::new();
    let (cell, m1) = match lang {
        super::Lang::De => (&DE, TEXTS_DE_M1),
        super::Lang::En => (&EN, TEXTS_EN_M1),
    };
    cell.get_or_init(|| {
        let mut out = String::from(m1);
        for f in SPLIT_FILES {
            out.push('\n');
            out.push_str(match lang {
                super::Lang::De => f.de,
                super::Lang::En => f.en,
            });
        }
        out
    })
}

/// Die Rohtexte der M1-Datei einer Sprache (für Tests der Aufteilung).
pub fn texts_m1_source(lang: super::Lang) -> &'static str {
    match lang {
        super::Lang::De => TEXTS_DE_M1,
        super::Lang::En => TEXTS_EN_M1,
    }
}
