//! Reports: Crawl Overview (OUT-080) und Redirects (OUT-081, OUT-082), Voraussetzungen (OUT-112). Ab M2 kommen die
//! Reports der Fachpakete als Tabellen-Einträge (`catalog::TableEntry`, je Report eine Datei in diesem Ordner; Stub: nur
//! Kopfzeile, K-14) dazu: [`report_tables`].
//!
//! Einstieg ist [`save_report`] (`--save-report "Crawl Overview,Redirects:Redirect Chains"`). Namen, Dateinamen
//! und Voraussetzungen stehen im Register `catalog::reports`. Jeder Report schreibt über die gemeinsamen
//! Tabellenschreiber (CSV/XLSX); fehlende Voraussetzungen führen zu Warnungen in [`ReportResult::warnings`],
//! nie zum Abbruch (OUT-112).

pub mod a11y_summary;
pub mod canonical_chains;
pub(crate) mod common;
pub mod cookies;
pub mod crawl_path;
pub mod hreflang;
pub mod http_headers;
pub mod insecure_content;
pub mod non_indexable_canonicals;
pub mod orphan_pages;
pub mod overview;
pub mod prereq;
pub mod queued_urls;
pub mod redirect_canonical_chains;
pub mod redirects;
pub mod serp_summary;
pub mod sidebar;
pub mod structured_data;

use std::path::Path;

use os_analyze::{tabs, TabDef};
use os_model::CrawlConfig;
use os_store::CrawlFile;

use crate::catalog::reports::{self, ReportKind};
use crate::catalog::TableEntry;
use crate::error::ExportError;
use crate::output::open_writer;
use crate::types::{Format, ReportResult};

use self::common::writer_options;
use self::redirects::RedirectReport;

/// Speichert einen Report aus dem Katalog in den Ausgabeordner.
///
/// `name` ist der kanonische Name (`"Crawl Overview"`, `"Redirects:All Redirects"`,
/// `"Redirects:Redirect Chains"`; Groß-/Kleinschreibung und Leerräume um `:` spielen keine Rolle). Der Report
/// rechnet gegen die Tab-Definitionen von `os_analyze::tabs::all()`.
pub fn save_report(
    file: &CrawlFile,
    cfg: &CrawlConfig,
    name: &str,
    out_dir: &Path,
    format: Format,
) -> Result<ReportResult, ExportError> {
    save_report_with_options(file, cfg, name, out_dir, format, &writer_options(cfg))
}

/// Exports with explicit writer options, including presentation header labels.
#[allow(clippy::too_many_arguments)]
pub fn save_report_with_options(
    file: &CrawlFile,
    cfg: &CrawlConfig,
    name: &str,
    out_dir: &Path,
    format: Format,
    opts: &crate::types::WriterOptions,
) -> Result<ReportResult, ExportError> {
    save_report_in_with_options(
        &tabs::configured_for_file(cfg, file),
        file,
        cfg,
        name,
        out_dir,
        format,
        opts,
    )
}

/// Wie [`save_report`], gegen eine eigene Tab-Liste (Tests, abweichende Registry).
pub fn save_report_in(
    tabs: &[TabDef],
    file: &CrawlFile,
    cfg: &CrawlConfig,
    name: &str,
    out_dir: &Path,
    format: Format,
) -> Result<ReportResult, ExportError> {
    save_report_in_with_options(tabs, file, cfg, name, out_dir, format, &writer_options(cfg))
}

/// Exports with explicit writer options, including presentation header labels.
#[allow(clippy::too_many_arguments)]
pub fn save_report_in_with_options(
    tabs: &[TabDef],
    file: &CrawlFile,
    cfg: &CrawlConfig,
    name: &str,
    out_dir: &Path,
    format: Format,
    opts: &crate::types::WriterOptions,
) -> Result<ReportResult, ExportError> {
    // Reports der Fachpakete von M2 (Tabellen-Einträge).
    if reports::resolve(name).is_err() {
        if let Some(entry) = crate::catalog::find_table(&report_tables(), name) {
            let path = out_dir.join(entry.file_name(format.extension()));
            let job = crate::catalog::TableJob {
                tabs,
                file,
                cfg,
                entry_name: name,
            };
            let info = crate::catalog::export_table_with_options(entry, &job, &path, format, opts)?;
            let warnings = info.warnings.clone();
            return Ok(ReportResult {
                files: vec![info],
                warnings,
            });
        }
    }
    let def = reports::resolve(name)?;
    let path = out_dir.join(def.file_name(format.extension()));
    let mut result = ReportResult::default();
    let mut pending: Vec<String> = Vec::new();
    let info = match def.kind {
        ReportKind::CrawlOverview => {
            let ov = overview::build(file, cfg, tabs)?;
            pending = ov.pending_analysis.clone();
            let mut writer = open_writer(&path, format, def.file_stem, opts)?;
            let header: Vec<String> = overview::HEADER.iter().map(|h| (*h).to_string()).collect();
            writer.header(&header)?;
            for row in &ov.rows {
                let (kinds, values) = row.to_values();
                writer.row(&kinds, &values)?;
            }
            writer.finish()?
        }
        ReportKind::AllRedirects => redirects::write_report_with_options(
            file,
            cfg,
            RedirectReport::All,
            &path,
            def.file_stem,
            format,
            opts,
        )?,
        ReportKind::RedirectChains => redirects::write_report_with_options(
            file,
            cfg,
            RedirectReport::Chains,
            &path,
            def.file_stem,
            format,
            opts,
        )?,
        // Ohne Vergleich: nur die Kopfzeile, die Warnung kommt aus den Voraussetzungen (OUT-112, OUT-154).
        ReportKind::CrawlChanges => {
            let mut writer = open_writer(&path, format, def.file_stem, opts)?;
            writer.header(&crate::compare::crawl_changes_header())?;
            writer.finish()?
        }
    };
    result.warnings = prereq::requirement_warnings(def.name, def.requires, cfg, &pending);
    for w in &result.warnings {
        tracing::warn!("{w}");
    }
    result.files.push(info);
    Ok(result)
}

/// Alle Reports von M2 als Tabellen-Einträge in Menüreihenfolge (Stub: nur Kopfzeile).
pub fn report_tables() -> Vec<&'static TableEntry> {
    let groups: [&'static [TableEntry]; 15] = [
        crate::catalog::reports_ai_plan::entries(),
        crate::catalog::reports_ai_search::entries(),
        serp_summary::entries(),
        canonical_chains::entries(),
        non_indexable_canonicals::entries(),
        redirects::entries(),
        redirect_canonical_chains::entries(),
        hreflang::entries(),
        insecure_content::entries(),
        orphan_pages::entries(),
        structured_data::entries(),
        a11y_summary::entries(),
        http_headers::entries(),
        cookies::entries(),
        sidebar::entries(),
    ];
    groups.iter().flat_map(|g| g.iter()).collect()
}

/// Report in einen bereits geöffneten Writer (OUT-010/075).
pub fn save_report_to(
    job: &crate::catalog::TableJob<'_>,
    writer: &mut dyn crate::types::TableWriter,
    ctx: &mut crate::types::ExportCtx<'_>,
) -> Result<u64, ExportError> {
    ctx.checkpoint()?;
    let count = if let Ok(def) = reports::resolve(job.entry_name) {
        match def.kind {
            ReportKind::CrawlOverview => {
                let overview = overview::build(job.file, job.cfg, job.tabs)?;
                writer.header(
                    &overview::HEADER
                        .iter()
                        .map(|s| (*s).to_owned())
                        .collect::<Vec<_>>(),
                )?;
                let mut count = 0;
                for row in overview.rows {
                    ctx.checkpoint()?;
                    let (kinds, values) = row.to_values();
                    writer.row(&kinds, &values)?;
                    count += 1;
                    (ctx.progress)(count);
                }
                count
            }
            ReportKind::AllRedirects => {
                redirects::write_report_to(job.file, RedirectReport::All, writer, ctx)?
            }
            ReportKind::RedirectChains => {
                redirects::write_report_to(job.file, RedirectReport::Chains, writer, ctx)?
            }
            ReportKind::CrawlChanges => {
                writer.header(&crate::compare::crawl_changes_header())?;
                0
            }
        }
    } else {
        let tables = report_tables();
        let entry = crate::catalog::find_table(&tables, job.entry_name)
            .ok_or_else(|| ExportError::Unsupported(job.entry_name.into()))?;
        let header = match entry.header_fn {
            Some(f) => f(job)?,
            None => entry.header.iter().map(|s| (*s).to_owned()).collect(),
        };
        writer.header(&header)?;
        (entry.run)(job, writer)?
    };
    if count == 0 && ctx.skip_empty {
        ctx.skipped.push(job.entry_name.into());
    }
    Ok(count)
}
