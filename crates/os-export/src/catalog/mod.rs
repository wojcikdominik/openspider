//! Namensregister der Exporte: Tab-Exporte, Bulk-Exporte und Reports.
//!
//! * Tab-Exporte: `Tab:Filter` aus der Registry von `os-analyze` (`tabs::all()`), Auflösung über
//!   `os_analyze::names` (case-insensitiv, Levenshtein-Vorschläge).
//! * Bulk-Exporte: `bulk_links` (Links, Response-Codes-Inlinks) und `bulk_issues` (`Issues:All Issues`); ab M2 je Gruppe
//!   eine Datei `bulk_<gruppe>.rs` mit Einträgen vom Typ [`TableEntry`] (Stub: nur Kopfzeile, K-14).
//! * Reports: `reports` (Crawl Overview, Redirects); ab M2 je Report eine Datei `reports/<name>.rs` mit [`TableEntry`].
//!
//! Alle Namen einer Art stehen in [`list`]; [`resolve`] löst sie case-insensitiv auf und liefert bei einem
//! Tippfehler Vorschläge über alle Namen der Art (Levenshtein ≤ 3).

pub mod bulk_a11y;
pub mod bulk_ai_decisions;
pub mod bulk_ai_embeddings;
pub mod bulk_ai_media;
pub mod bulk_ai_prompts;
pub mod bulk_ai_suggestions;
pub mod bulk_canonicals;
pub mod bulk_content;
pub mod bulk_custom_extraction;
pub mod bulk_custom_search;
pub mod bulk_directives;
pub mod bulk_generic;
pub mod bulk_images;
pub mod bulk_issues;
pub mod bulk_javascript;
pub mod bulk_links;
pub mod bulk_pagination;
pub mod bulk_response_codes;
pub mod bulk_screenshots;
pub mod bulk_sd;
pub mod bulk_security;
pub mod bulk_sitemaps;
pub mod bulk_web;
pub mod page_content;
pub mod reports;
pub mod reports_ai_plan;
pub mod reports_ai_search;

use std::path::Path;

use os_analyze::defs::TabDef;
use os_analyze::{tabs, NameError};
use os_model::CrawlConfig;
use os_store::CrawlFile;

use crate::catalog::reports::ReportRequirement;
use crate::error::ExportError;
use crate::output::open_writer;
use crate::types::{ExportFileInfo, Format, TableWriter};

/// Art des Exports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportKind {
    /// Tab-Export (`--export-tabs "Tab:Filter"`).
    Tab,
    /// Bulk-Export (`--bulk-export`).
    Bulk,
    /// Report (`--save-report`).
    Report,
}

/// Aufgelöster Export.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExportRef {
    /// Tab und Filter in kanonischer Schreibweise.
    Tab {
        /// Tab.
        tab: String,
        /// Filter.
        filter: String,
    },
    /// Bulk-Export (kanonischer Name).
    Bulk(String),
    /// Report (kanonischer Name).
    Report(String),
}

/// Löst einen Namen (case-insensitiv) auf.
pub fn resolve(kind: ExportKind, name: &str) -> Result<ExportRef, NameError> {
    resolve_with(kind, name, &CrawlConfig::default())
}

/// Auflösung mit der wirksamen Crawl-Konfiguration.
pub fn resolve_with(
    kind: ExportKind,
    name: &str,
    cfg: &CrawlConfig,
) -> Result<ExportRef, NameError> {
    match kind {
        ExportKind::Tab => {
            let (tab, filter) = os_analyze::names::resolve_in(&tabs::configured(cfg), name)?;
            Ok(ExportRef::Tab { tab, filter })
        }
        ExportKind::Bulk => {
            if page_content::matches(name) {
                return Ok(ExportRef::Bulk(page_content::NAME.into()));
            }
            if let Some(entry) = dynamic_entries(cfg)
                .into_iter()
                .find(|e| normalize_name(&e.name) == normalize_name(name))
            {
                return Ok(ExportRef::Bulk(entry.name));
            }
            if let Some(entry) = bulk_links::find(name) {
                return Ok(ExportRef::Bulk(entry.name.clone()));
            }
            if let Ok(issues) = bulk_issues::resolve(name) {
                return Ok(ExportRef::Bulk(issues.to_string()));
            }
            if let Some(entry) = find_table(&bulk_tables(), name) {
                return Ok(ExportRef::Bulk(entry.name.to_string()));
            }
            Err(NameError::Unknown {
                input: name.to_string(),
                suggestions: bulk_suggestions(name),
            })
        }
        ExportKind::Report => match reports::resolve(name) {
            Ok(def) => Ok(ExportRef::Report(def.name.to_string())),
            Err(e) => match find_table(&crate::reports::report_tables(), name) {
                Some(entry) => Ok(ExportRef::Report(entry.name.to_string())),
                None => Err(e),
            },
        },
    }
}

/// Alle gültigen Namen der Art, für Hilfetexte und Vorschläge. Tab-Namen tragen für Schwellen ein `X`
/// (`Page Titles:Over X Characters`).
pub fn list(kind: ExportKind) -> Vec<String> {
    list_with(kind, &CrawlConfig::default())
}

/// Namen der konfigurierten Registry.
pub fn list_with(kind: ExportKind, cfg: &CrawlConfig) -> Vec<String> {
    match kind {
        ExportKind::Tab => tabs::configured(cfg)
            .iter()
            .flat_map(|tab| {
                tab.filters
                    .iter()
                    .map(|f| format!("{}:{}", tab.id, placeholders_to_x(f.id)))
                    .collect::<Vec<_>>()
            })
            .collect(),
        ExportKind::Bulk => {
            let mut names = bulk_links::names();
            names.push(page_content::NAME.into());
            names.extend(bulk_issues::names());
            names.extend(dynamic_entries(cfg).into_iter().map(|e| e.name));
            names.extend(bulk_tables().iter().map(|e| e.name.to_string()));
            names
        }
        ExportKind::Report => {
            let mut names = reports::names();
            names.extend(
                crate::reports::report_tables()
                    .iter()
                    .map(|e| e.name.to_string()),
            );
            names
        }
    }
}

/// Höchstabstand und -zahl der Vorschläge für Bulk-Namen (wie in `bulk_links`).
const MAX_DISTANCE: usize = 3;
const MAX_SUGGESTIONS: usize = 5;

/// Ähnliche Bulk-Namen über alle Kataloge (Links und Issues), nächste zuerst.
fn bulk_suggestions(input: &str) -> Vec<String> {
    let norm = |s: &str| {
        s.split(':')
            .map(|p| p.split_whitespace().collect::<Vec<_>>().join(" "))
            .collect::<Vec<_>>()
            .join(":")
            .to_lowercase()
    };
    let wanted = norm(input);
    let mut scored: Vec<(usize, String)> = list(ExportKind::Bulk)
        .into_iter()
        .filter_map(|n| {
            let d = bulk_links::levenshtein(&wanted, &norm(&n));
            (d <= MAX_DISTANCE).then_some((d, n))
        })
        .collect();
    scored.sort();
    scored.truncate(MAX_SUGGESTIONS);
    scored.into_iter().map(|(_, n)| n).collect()
}

/// `Over {title.max_chars} Characters` → `Over X Characters`.
pub(crate) fn placeholders_to_x(id: &str) -> String {
    let mut out = String::new();
    let mut rest = id;
    while let Some(open) = rest.find('{') {
        let Some(close) = rest[open..].find('}') else {
            break;
        };
        out.push_str(&rest[..open]);
        out.push('X');
        rest = &rest[open + close + 1..];
    }
    out.push_str(rest);
    out
}

// ---------------------------------------------------------------------------------------------------------------
// Tabellen-Einträge der M2-Kataloge (Stub: nur Kopfzeile, K-14)

/// Kopfzeile des Standard-Linkformats (OUT-031); gleich `links_export::LINK_COLUMNS`.
pub const LINK_HEADER: &[&str] = &[
    "Type",
    "Source",
    "Destination",
    "Size (Bytes)",
    "Alt Text",
    "Anchor",
    "Status Code",
    "Status",
    "Follow",
    "Target",
    "Rel",
    "Path Type",
    "Link Path",
    "Link Position",
    "Link Origin",
];

/// Standard-Linkformat plus die Spalte `Internal` (Insecure Content, OUT-097).
pub const LINK_INTERNAL_HEADER: &[&str] = &[
    "Type",
    "Source",
    "Destination",
    "Size (Bytes)",
    "Alt Text",
    "Anchor",
    "Status Code",
    "Status",
    "Follow",
    "Target",
    "Rel",
    "Path Type",
    "Link Path",
    "Link Position",
    "Link Origin",
    "Internal",
];

/// Kopfzeile der Bild-Exporte (OUT-053, ONPAGE-181).
pub const IMAGE_HEADER: &[&str] = &[
    "From",
    "To",
    "Alt Text",
    "Size",
    "Real Dimensions (WxH)",
    "Dimensions in Attributes (WxH)",
    "Display Dimensions (WxH)",
    "Potential Savings",
    "Path Type",
    "Link Path",
    "Link Position",
    "Link Origin",
];

/// Feste Spalten der Kettenreports (OUT-081); die Spalten je Sprung hängen von der längsten Kette ab und folgen mit der
/// Umsetzung.
pub const CHAIN_HEADER: &[&str] = &[
    "Chain Type",
    "Number of Redirects",
    "Redirect Loop",
    "Temp Redirect in Chain",
    "Address",
    "Final Address",
    "Final Status Code",
    "Final Status",
    "Final Content Type",
    "Final Indexability",
    "Final Indexability Status",
    "Inlinks",
    "Sample Source",
];

/// Eingabe eines Tabellen-Eintrags: Tab-Definitionen, Crawl-Datei und Konfiguration.
pub struct TableJob<'a> {
    /// Aufgerufener Exportname.
    pub entry_name: &'a str,
    /// Tab-Definitionen (`tabs::all()` oder eine eigene Liste).
    pub tabs: &'a [TabDef],
    /// Die Crawl-Datei.
    pub file: &'a CrawlFile,
    /// Konfiguration des Crawls.
    pub cfg: &'a CrawlConfig,
}

/// Schreibt die Datenzeilen eines Eintrags (ohne Kopfzeile, ohne `finish`); liefert die Zeilenzahl.
pub type TableRun = fn(&TableJob<'_>, &mut dyn TableWriter) -> Result<u64, ExportError>;

/// Ein Eintrag der Kataloge, dessen Umsetzung ein Fachpaket liefert. Der Stub ([`stub_run`]) schreibt nur die Kopfzeile
/// (K-14); das Paket ersetzt `run`.
pub struct TableEntry {
    /// Kanonischer Name (`--bulk-export`/`--save-report`), z. B. `Security:Mixed Content Inlinks`.
    pub name: &'static str,
    /// Weitere gültige Schreibweisen (z. B. der Name aus der Spec).
    pub aliases: &'static [&'static str],
    /// Grundlage des Dateinamens (OUT-008); meist gleich `name`.
    pub file_stem: &'static str,
    /// Kopfzeile der Datei.
    pub header: &'static [&'static str],
    /// Datenabhängige Kopfzeile.
    pub header_fn: Option<HeaderFn>,
    /// Voraussetzungen (OUT-112).
    pub requires: &'static [ReportRequirement],
    /// Paket, das den Eintrag umsetzt (`M2-D11`).
    pub owner: &'static str,
    /// Schreibt die Datenzeilen.
    pub run: TableRun,
}

impl TableEntry {
    /// Dateiname mit Endung (`csv`, `xlsx`).
    pub fn file_name(&self, ext: &str) -> String {
        crate::naming::file_name(self.file_stem, None, ext)
    }
}

/// Stub-Umsetzung: keine Datenzeilen (K-14: nur die Kopfzeile).
pub fn stub_run(_job: &TableJob<'_>, _writer: &mut dyn TableWriter) -> Result<u64, ExportError> {
    Ok(0)
}

/// Alle Bulk-Tabelleneinträge der M2-Gruppen in Katalogreihenfolge.
pub fn bulk_tables() -> Vec<&'static TableEntry> {
    let groups: [&'static [TableEntry]; 22] = [
        bulk_ai_prompts::entries(),
        bulk_ai_decisions::entries(),
        bulk_ai_suggestions::entries(),
        bulk_ai_embeddings::entries(),
        bulk_ai_media::entries(),
        bulk_security::entries(),
        bulk_response_codes::entries(),
        bulk_canonicals::entries(),
        bulk_directives::entries(),
        bulk_images::entries(),
        bulk_content::entries(),
        bulk_pagination::entries(),
        bulk_sd::entries(),
        bulk_sitemaps::entries(),
        bulk_javascript::entries(),
        bulk_screenshots::entries(),
        bulk_custom_search::entries(),
        bulk_custom_extraction::entries(),
        bulk_a11y::entries(),
        bulk_web::entries(),
        bulk_generic::entries(),
        crate::reports::queued_urls::entries(),
    ];
    groups.iter().flat_map(|g| g.iter()).collect()
}

/// Sucht einen Eintrag nach Name oder Alias (case-insensitiv, Leerräume um `:` und innerhalb zusammengefasst).
pub fn find_table<'a>(entries: &[&'a TableEntry], name: &str) -> Option<&'a TableEntry> {
    let wanted = normalize_name(name);
    entries.iter().copied().find(|e| {
        normalize_name(e.name) == wanted || e.aliases.iter().any(|a| normalize_name(a) == wanted)
    })
}

fn normalize_name(name: &str) -> String {
    name.split(':')
        .map(|part| part.split_whitespace().collect::<Vec<_>>().join(" "))
        .collect::<Vec<_>>()
        .join(":")
        .to_lowercase()
}

/// Schreibt einen Tabellen-Eintrag als Datei: Kopfzeile, dann die Zeilen von `run`, dann die Hinweise zu den
/// Voraussetzungen (OUT-112) in [`ExportFileInfo::warnings`].
pub fn export_table(
    entry: &TableEntry,
    job: &TableJob<'_>,
    path: &Path,
    format: Format,
) -> Result<ExportFileInfo, ExportError> {
    export_table_with_options(
        entry,
        job,
        path,
        format,
        &crate::pipeline::writer_options(job.cfg),
    )
}

/// Exports with explicit writer options, including presentation header labels.
#[allow(clippy::too_many_arguments)]
pub fn export_table_with_options(
    entry: &TableEntry,
    job: &TableJob<'_>,
    path: &Path,
    format: Format,
    opts: &crate::types::WriterOptions,
) -> Result<ExportFileInfo, ExportError> {
    let mut writer = open_writer(path, format, entry.file_stem, opts)?;
    let header = match entry.header_fn {
        Some(make) => make(job)?,
        None => entry.header.iter().map(|h| (*h).to_string()).collect(),
    };
    writer.header(&header)?;
    (entry.run)(job, &mut *writer)?;
    let mut info = writer.finish()?;
    let warnings =
        crate::reports::prereq::requirement_warnings(entry.name, entry.requires, job.cfg, &[]);
    for w in &warnings {
        tracing::warn!("{w}");
    }
    info.warnings.extend(warnings);
    Ok(info)
}

/// Erzeugt eine datenabhängige Kopfzeile.
pub type HeaderFn = fn(&TableJob<'_>) -> Result<Vec<String>, ExportError>;
/// Dynamischer Tabellenexport einer konfigurierten Suche oder Extraktion.
pub struct DynamicEntry {
    pub name: String,
    /// Display metadata; canonical command/file names remain unchanged.
    pub label_key: Option<String>,
    pub file_stem: String,
    pub header: Vec<String>,
    pub run: Box<DynamicRun>,
}
/// Structured UI label; user text is never inspected for translated words or suffixes.
pub(crate) fn custom_label_key(
    group: &str,
    name: &str,
    inlinks: bool,
    generated_index: Option<usize>,
) -> String {
    format!(
        "custom-export:{}",
        serde_json::json!({
            "group": group,
            "name": name,
            "inlinks": inlinks,
            "generatedIndex": generated_index,
        })
    )
}
/// Schreibt Daten eines dynamischen Exports.
pub type DynamicRun =
    dyn Fn(&TableJob<'_>, &mut dyn TableWriter) -> Result<u64, ExportError> + Send + Sync;
/// Konfigurationsabhängige Exporte.
pub fn dynamic_entries(cfg: &CrawlConfig) -> Vec<DynamicEntry> {
    let mut entries = bulk_custom_search::dynamic_entries(cfg);
    entries.extend(bulk_images::dynamic_entries(cfg));
    entries.extend(bulk_custom_extraction::dynamic_entries(cfg));
    // M4-V01 (AI-076, additiv): Exporte der Optimierungsvorschläge.
    entries.extend(bulk_ai_suggestions::dynamic_entries(cfg));
    entries
}

#[cfg(test)]
mod m4_tests {
    use super::*;
    // spec: AI-001
    #[test]
    fn ai_export_fixture_resolves_bulk_and_report_entries() {
        assert!(bulk_ai_prompts::entries().is_empty());
        assert!(bulk_ai_suggestions::entries().is_empty());
        let fixture = [
            (ExportKind::Bulk, "AI Decisions:All Answers"),
            (ExportKind::Bulk, "AI:Embeddings"),
            (ExportKind::Bulk, "AI:All Generated Images"),
            (ExportKind::Bulk, "AI:All Generated Audio"),
            (ExportKind::Report, "AI:Action Plan"),
            (ExportKind::Report, "AI:AI Crawler Accessibility"),
            (ExportKind::Report, "AI:AI Search Readiness"),
        ];
        for (kind, name) in fixture {
            assert!(list(kind).contains(&name.to_owned()));
            assert!(resolve(kind, &name.to_lowercase()).is_ok());
        }
        assert_eq!(
            bulk_ai_embeddings::entries()[0].header,
            [
                "Address",
                "Prompt",
                "Embedding Model",
                "Dimensions",
                "Embedding"
            ]
        );
    }
}
