//! Namensregister der Exporte für Validierung, Hilfe und Auflistung (OUT-181, OUT-203).
//!
//! Eine Quelle für alles: die Kataloge von `os-export` (Tab-Exporte aus `os_analyze::tabs::all()`,
//! Link- und Issues-Bulk-Exporte, Reports). `--help export-tabs`, `--list-exports` und die Prüfung der
//! Eingaben lesen dieselben Listen, damit sie nicht auseinanderlaufen.

use os_export::catalog::{self, bulk_issues, bulk_links, reports, ExportKind};

/// Kategorie eines Namens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// `--export-tabs`.
    Tab,
    /// `--bulk-export`.
    Bulk,
    /// `--save-report`.
    Report,
}

impl Kind {
    /// Wort für Fehlermeldungen (`unknown export …` / `unknown report …`).
    pub fn noun(self) -> &'static str {
        match self {
            Kind::Report => "report",
            _ => "export",
        }
    }

    /// Hilfethema (`--help <thema>`).
    pub fn topic(self) -> &'static str {
        match self {
            Kind::Tab => "export-tabs",
            Kind::Bulk => "bulk-export",
            Kind::Report => "save-report",
        }
    }
}

/// Aufgelöster Name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolved {
    /// Tab und Filter in kanonischer Schreibweise.
    Tab {
        /// Tab.
        tab: String,
        /// Filter (mit Platzhaltern der Schwellen).
        filter: String,
    },
    /// Tab und Filter mit Vergleichs-Unterfilter (`"Internal:HTML:Missing"`, OUT-151; nur im Vergleich).
    CompareTab {
        /// Tab.
        tab: String,
        /// Filter (mit Platzhaltern der Schwellen).
        filter: String,
        /// Unterfilter Added/New/Removed/Missing.
        class: os_analyze::compare::CompareClass,
    },
    /// Link-/Response-Codes-Bulk-Export.
    Bulk(String),
    /// Issues-Export (`Issues:All Issues`).
    Issues,
    /// Report.
    Report(String),
}

impl Resolved {
    /// Tab, Filter und ggf. Vergleichs-Unterfilter eines Tab-Exports.
    pub fn tab_parts(&self) -> Option<(&str, &str, Option<os_analyze::compare::CompareClass>)> {
        match self {
            Resolved::Tab { tab, filter } => Some((tab, filter, None)),
            Resolved::CompareTab { tab, filter, class } => Some((tab, filter, Some(*class))),
            _ => None,
        }
    }

    /// Schlüssel zum Entfernen von Duplikaten (kanonischer Name in Kleinbuchstaben).
    pub fn key(&self) -> String {
        match self {
            Resolved::Tab { tab, filter } => format!("tab:{tab}:{filter}").to_lowercase(),
            Resolved::CompareTab { tab, filter, class } => {
                format!("tab:{tab}:{filter}:{}", class.as_str()).to_lowercase()
            }
            Resolved::Bulk(n) => format!("bulk:{n}").to_lowercase(),
            Resolved::Issues => "bulk:issues".to_string(),
            Resolved::Report(n) => format!("report:{n}").to_lowercase(),
        }
    }
}

/// Unbekannter Name samt ähnlichen gültigen Namen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unknown {
    /// Eingabe.
    pub input: String,
    /// Ähnliche gültige Namen (Levenshtein ≤ 3), nächste zuerst.
    pub suggestions: Vec<String>,
}

const MAX_DISTANCE: usize = 3;
const MAX_SUGGESTIONS: usize = 5;

fn union(mut first: Vec<String>, more: Vec<String>) -> Vec<String> {
    for name in more {
        if !first.iter().any(|n| n.eq_ignore_ascii_case(&name)) {
            first.push(name);
        }
    }
    first
}

/// Alle gültigen Namen der Kategorie in Menüreihenfolge. Schwellen in Tab-Namen stehen als `X`.
pub fn names(kind: Kind) -> Vec<String> {
    let mut names = match kind {
        Kind::Tab => catalog::list(ExportKind::Tab),
        Kind::Bulk => union(catalog::list(ExportKind::Bulk), bulk_issues::names()),
        Kind::Report => union(catalog::list(ExportKind::Report), reports::names()),
    };
    names.sort();
    names.dedup();
    names
}

/// Vergleichsform: Kleinschreibung, zusammengefasste Leerräume und Legacy-Filteralias.
fn normalize(name: &str) -> String {
    name.split(':')
        .map(|part| {
            let part = part.split_whitespace().collect::<Vec<_>>().join(" ");
            if part.eq_ignore_ascii_case("flash") {
                "Plugins".to_string()
            } else {
                part
            }
        })
        .collect::<Vec<_>>()
        .join(":")
        .to_lowercase()
}

fn suggestions(kind: Kind, input: &str) -> Vec<String> {
    let wanted = normalize(input);
    let mut scored: Vec<(usize, String)> = names(kind)
        .into_iter()
        .filter_map(|n| {
            let d = bulk_links::levenshtein(&wanted, &normalize(&n));
            (d <= MAX_DISTANCE).then_some((d, n))
        })
        .collect();
    scored.sort();
    // Nur die nächsten Treffer (kleinste Entfernung) – bei Gleichstand mehrere.
    let best = scored.first().map(|(d, _)| *d);
    scored.retain(|(d, _)| Some(*d) == best);
    scored.truncate(MAX_SUGGESTIONS);
    scored.into_iter().map(|(_, n)| n).collect()
}

/// Löst einen Namen auf (Groß-/Kleinschreibung und Leerräume um `:` spielen keine Rolle).
pub fn resolve(kind: Kind, input: &str) -> Result<Resolved, Unknown> {
    resolve_with(kind, input, &os_model::CrawlConfig::default())
}

/// Auflösung nach Laden der Konfiguration.
pub fn resolve_with(
    kind: Kind,
    input: &str,
    cfg: &os_model::CrawlConfig,
) -> Result<Resolved, Unknown> {
    let unknown = || Unknown {
        input: input.to_string(),
        suggestions: suggestions(kind, input),
    };
    match kind {
        Kind::Tab => match catalog::resolve_with(ExportKind::Tab, &normalize(input), cfg) {
            Ok(catalog::ExportRef::Tab { tab, filter }) => Ok(Resolved::Tab { tab, filter }),
            _ => {
                // OUT-151 (Nutzerentscheidung 04.11.2026): letzter Namensteil als Vergleichs-Unterfilter, nur wenn der
                // ganze Name kein Tab-Export ist.
                let (rest, last) = input.rsplit_once(':').ok_or_else(unknown)?;
                let class = os_analyze::compare::CompareClass::parse(last).ok_or_else(unknown)?;
                match catalog::resolve_with(ExportKind::Tab, &normalize(rest), cfg) {
                    Ok(catalog::ExportRef::Tab { tab, filter }) => {
                        Ok(Resolved::CompareTab { tab, filter, class })
                    }
                    _ => Err(unknown()),
                }
            }
        },
        Kind::Bulk => {
            if bulk_issues::resolve(input).is_ok() {
                return Ok(Resolved::Issues);
            }
            match catalog::resolve_with(ExportKind::Bulk, input, cfg) {
                Ok(catalog::ExportRef::Bulk(name)) => Ok(Resolved::Bulk(name)),
                _ => Err(unknown()),
            }
        }
        Kind::Report => match catalog::resolve_with(ExportKind::Report, input, cfg) {
            Ok(catalog::ExportRef::Report(name)) => Ok(Resolved::Report(name)),
            _ => Err(unknown()),
        },
    }
}

/// Meldung zu einem unbekannten Namen (OUT-203): mit Vorschlag, sonst mit der vollständigen Liste
/// (eine Zeile je Name, identisch mit `--help <thema>`).
pub fn unknown_message(kind: Kind, unknown: &Unknown) -> String {
    let noun = kind.noun();
    match unknown.suggestions.as_slice() {
        [] => {
            let mut msg =
                format!(
                "unknown {noun} '{}' – no similar name; openspider --list-{} lists valid names:",
                unknown.input, if kind == Kind::Report { "reports" } else { "exports" }
            );
            for name in names(kind) {
                msg.push('\n');
                msg.push_str(&name);
            }
            msg
        }
        [one] => format!(
            "unknown {noun} '{}' – did you mean '{one}'? (openspider --list-{})",
            unknown.input,
            if kind == Kind::Report {
                "reports"
            } else {
                "exports"
            }
        ),
        many => {
            let list: Vec<String> = many.iter().map(|s| format!("'{s}'")).collect();
            format!(
                "unknown {noun} '{}' – did you mean {}? (openspider --list-{})",
                unknown.input,
                list.join(" or "),
                if kind == Kind::Report {
                    "reports"
                } else {
                    "exports"
                }
            )
        }
    }
}

/// Zerlegt eine Liste `"A, B,C"` an unmaskierten Kommas. `\,` steht für ein Komma im Namen; Leerräume
/// um die Einträge entfallen, leere Einträge werden übersprungen (OUT-192).
pub fn split_list(list: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut chars = list.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' if chars.peek() == Some(&',') => {
                current.push(',');
                chars.next();
            }
            ',' => {
                out.push(std::mem::take(&mut current));
            }
            other => current.push(other),
        }
    }
    out.push(current);
    out.into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // spec: ONPAGE-023, ONPAGE-026 (SQ-07)
    #[test]
    fn resource_filters_are_always_public_and_flash_remains_an_input_alias() {
        let mut cfg = os_model::CrawlConfig::default();
        for parity in [true, false] {
            cfg.ui.parity_filters = parity;
            for tab in ["Internal", "External"] {
                for filter in ["Plugins", "Fonts", "Media", "XML"] {
                    let name = format!("{tab}:{filter}");
                    assert!(names(Kind::Tab).contains(&name));
                    assert_eq!(
                        resolve_with(Kind::Tab, &name, &cfg),
                        Ok(Resolved::Tab {
                            tab: tab.into(),
                            filter: filter.into()
                        })
                    );
                }
                for alias in ["Flash", "fLaSh", " FLASH "] {
                    assert_eq!(
                        resolve_with(Kind::Tab, &format!(" {tab} : {alias}"), &cfg),
                        resolve_with(Kind::Tab, &format!("{tab}:Plugins"), &cfg)
                    );
                }
                assert!(!names(Kind::Tab).contains(&format!("{tab}:Flash")));
            }
        }
        assert!(resolve(Kind::Tab, "Internal:Flashlight").is_err());
    }

    // spec: OUT-192
    #[test]
    fn lists_split_at_unescaped_commas_and_tolerate_spaces() {
        assert_eq!(
            split_list(" Internal : All ,  Response Codes:Client Error (4xx) "),
            vec!["Internal : All", "Response Codes:Client Error (4xx)"]
        );
        assert_eq!(split_list(r"A\,B,C"), vec!["A,B", "C"]);
        assert_eq!(split_list(",,x,,"), vec!["x"]);
        assert!(split_list("").is_empty());
    }

    // spec: OUT-192
    #[test]
    fn names_resolve_case_insensitively_and_with_spaces_around_the_colon() {
        for input in ["internal:all", "  INTERNAL : All ", "Internal:All"] {
            assert_eq!(
                resolve(Kind::Tab, input),
                Ok(Resolved::Tab {
                    tab: "Internal".into(),
                    filter: "All".into()
                }),
                "{input}"
            );
        }
        assert!(resolve(Kind::Tab, "Response Codes:Client Error (4xx)").is_ok());
        // Schwelle als Zahl oder als X
        assert!(resolve(Kind::Tab, "Page Titles:Over 60 Characters").is_ok());
        assert!(resolve(Kind::Tab, "Page Titles:Over X Characters").is_ok());
    }

    // spec: OUT-203
    #[test]
    fn unknown_names_get_a_suggestion_within_levenshtein_three() {
        let unknown = resolve(Kind::Tab, "Internal:Al").unwrap_err();
        assert_eq!(
            unknown_message(Kind::Tab, &unknown),
            "unknown export 'Internal:Al' – did you mean 'Internal:All'? (openspider --list-exports)"
        );
    }

    // spec: OUT-203 [P1]
    #[test]
    fn without_a_similar_name_the_whole_list_follows() {
        let unknown = resolve(Kind::Report, "Gibtsnicht-Gibtsnicht-Gibtsnicht").unwrap_err();
        assert!(unknown.suggestions.is_empty());
        let msg = unknown_message(Kind::Report, &unknown);
        let mut lines = msg.lines();
        assert!(lines
            .next()
            .unwrap()
            .starts_with("unknown report 'Gibtsnicht"));
        let listed: Vec<&str> = lines.collect();
        assert_eq!(listed, names(Kind::Report));
        assert!(listed.contains(&"Crawl Overview"));
    }

    // spec: OUT-181
    #[test]
    fn the_registry_lists_every_filter_of_the_tab_registry() {
        let expected: usize = os_analyze::tabs::configured(&os_model::CrawlConfig::default())
            .iter()
            .map(|t| t.filters.len())
            .sum();
        assert_eq!(names(Kind::Tab).len(), expected);
        assert!(names(Kind::Bulk).contains(&"Links:All Inlinks".to_string()));
        assert!(names(Kind::Bulk).contains(&"Issues:All Issues".to_string()));
    }

    // spec: OUT-050, OUT-193
    #[test]
    fn every_bulk_name_resolves_including_the_three_level_response_codes_names() {
        for name in names(Kind::Bulk) {
            assert!(resolve(Kind::Bulk, &name).is_ok(), "{name}");
        }
        // 12 Einträge × 3 Bereiche (Internal & External, Internal, External)
        let response_codes = names(Kind::Bulk)
            .into_iter()
            .filter(|n| n.starts_with("Response Codes:"))
            .count();
        assert_eq!(response_codes, 36);
        assert_eq!(
            resolve(
                Kind::Bulk,
                "Response Codes:Internal:Redirection (HTTP Refresh) Inlinks"
            ),
            Ok(Resolved::Bulk(
                "Response Codes:Internal:Redirection (HTTP Refresh) Inlinks".to_string()
            ))
        );
        assert!(resolve(Kind::Bulk, "Links:All Inlinks").is_ok());
    }

    #[test]
    fn issues_export_resolves_to_its_own_variant() {
        assert_eq!(
            resolve(Kind::Bulk, "issues : all issues"),
            Ok(Resolved::Issues)
        );
    }
}

#[cfg(test)]
mod w1_contracts {
    // spec: INTEG-020
    // Infrastruktur (M2-F05a) für INTEG-020; Fachprüfung M2-D14.
    #[test]
    fn cli_resolves_filters_from_a_configured_column_source() {
        os_analyze::tabs::testing::with_fixture_source(|| {
            assert_eq!(
                super::resolve_with(
                    super::Kind::Tab,
                    "Internal:Fixture Filter",
                    &os_model::CrawlConfig::default()
                )
                .unwrap(),
                super::Resolved::Tab {
                    tab: "Internal".into(),
                    filter: "Fixture Filter".into()
                }
            );
        });
        assert!(super::resolve(super::Kind::Tab, "Internal:Fixture Filter").is_err());
    }
}
