//! Prüfung aller Argumente vor dem ersten Netzzugriff (OUT-203) und Aufbau des Ablaufplans.
//!
//! Hier läuft nichts im Netz: Quelle, Konfiguration (Datei → `--set` → Validierung), Namen der Exporte,
//! Format, Listen- und Crawl-Dateien sowie der Ausgabeordner werden geprüft. Jeder Fehler ist ein
//! Aufruffehler (Exit-Code 2); die Meldungen nennen den Namen und – wo möglich – einen Vorschlag.

use std::path::{Path, PathBuf};
use std::str::FromStr;

use os_engine::list_input::{self, ListCounts};
use os_export::output::{self, OutputMode};
use os_export::Format;
use os_model::{CrawlConfig, CrawlId, LoadedConfig, SetError, SystemClock};
use os_store::paths::AppPaths;
use os_store::{registry, CrawlFile, OpenMode};

use crate::args::RunArgs;
use crate::exit::CliError;
use crate::registry::{self as names, Kind, Resolved};
use crate::reporter::Reporter;

/// Woher der Lauf seine Daten nimmt.
#[derive(Debug)]
pub enum Source {
    /// Spider-Modus ab einer Start-URL.
    Spider {
        /// Normalisierte Start-URL (mit Schema).
        start_url: String,
    },
    /// Listenmodus.
    List {
        /// Eingabezeilen bzw. -URLs in Reihenfolge.
        items: Vec<String>,
        /// Original spelling of corrected standalone inputs, indexed by upload position.
        originals: Vec<(u64, String)>,
        /// Bezeichnung der Quelle (Dateiname bzw. Host der Sitemap).
        label: String,
    },
    /// Gespeicherter Crawl.
    Load {
        /// Pfad der Crawl-Datei.
        path: PathBuf,
    },
}

/// Zusätzliche Exportoptionen geplanter Aufgaben (OUT-007, OUT-012, OUT-075): Die Exporte laufen als
/// Stapel gegen einen gemeinsamen Lesestand; unbekannte Exportnamen sind dann Warnungen statt Fehler.
#[derive(Debug, Clone, Copy, Default)]
pub struct BatchSettings {
    /// Tabellen als Blätter einer Arbeitsmappe (nur XLSX).
    pub single_workbook: bool,
    /// Leere Exporte auslassen.
    pub skip_empty: bool,
}

/// Ablaufplan nach der Validierung.
#[derive(Debug)]
pub struct Plan {
    /// Optional API credential view supplied by a scheduled host (default None).
    pub api_secrets: Option<os_integrations::crawl_glue::SecretSnapshot>,
    /// Require every requested API; default false.
    pub require_apis: bool,
    pub header_labels: std::collections::BTreeMap<String, String>,
    /// Auto-Vergleich nach erfolgreichem Abschluss (OUT-152).
    pub compare_with_previous: bool,
    /// Explicit comparison partner, resolved before any execution.
    pub explicit_previous: Option<PathBuf>,
    /// Invocation-specific mapping and element selection for an explicit comparison.
    pub explicit_compare_config: Option<os_model::CompareConfig>,
    /// Stapelmodus der Exporte (geplante Aufgaben); `None` = einzeln wie auf der Kommandozeile.
    pub batch: Option<BatchSettings>,
    /// Decrypted credentials, held only for this process.
    pub auth: crate::auth::AuthSession,
    /// XML exports requested after a successful crawl.
    pub create_sitemap: bool,
    pub create_images_sitemap: bool,
    /// Legacy binary output, generated from the common table stream.
    pub xls: bool,
    /// Validated Google target; local staging uses CSV.
    pub google: Option<crate::google::GoogleTarget>,
    /// Optionaler Segmentkontext; ohne Flag bleibt der Export ungefiltert.
    pub segment: Option<String>,
    /// Quelle.
    pub source: Source,
    /// Wirksame Konfiguration (bei `Load` ohne `--resume` die gespeicherte).
    pub config: CrawlConfig,
    /// Beim Fortsetzen: Konfiguration, die der Aufruf ändert (sonst `None` = gespeicherte behalten).
    pub resume_config: Option<CrawlConfig>,
    /// Exporte in Aufrufreihenfolge (Tabs, Bulk, Reports), ohne Duplikate.
    pub exports: Vec<Resolved>,
    /// Normalisierte Auswahl für den Crawl-Path-Export (OUT-023).
    pub crawl_path_urls: Vec<String>,
    /// Ausgabeformat.
    pub format: Format,
    /// Vorbereiteter Ausgabeordner (nur wenn Dateien entstehen).
    pub output: Option<PathBuf>,
    /// `--save-crawl`.
    pub save_crawl: bool,
    /// `--no-store`.
    pub no_store: bool,
    /// `--resume`.
    pub resume: bool,
    /// `--fail-on-start-error`.
    pub fail_on_start_error: bool,
    /// `--strict`.
    pub strict: bool,
    /// Projektname.
    pub project: String,
    /// Crawl-Name.
    pub name: String,
    /// Speicherorte der Anwendung.
    pub paths: AppPaths,
}

fn set_error(e: &SetError) -> CliError {
    let mut msg = e.to_string();
    if let SetError::UnknownKey {
        suggestion: None, ..
    } = e
    {
        msg.push_str(" – die setzbaren Schlüssel listet `openspider --help set`");
    }
    CliError::usage(msg)
}

fn parse_format(args: &RunArgs) -> Result<Format, CliError> {
    let Some(raw) = args.export_format.as_deref() else {
        return Ok(Format::Csv);
    };
    match raw.trim().to_ascii_lowercase().as_str() {
        "csv" => Ok(Format::Csv),
        "xlsx" => Ok(Format::Xlsx),
        "xls" | "gsheet" => Ok(Format::Csv),
        other => Err(CliError::usage(format!(
            "unbekanntes Exportformat '{other}' – gültig: csv, xls, xlsx, gsheet \
            "
        ))),
    }
}

/// Konfiguration: Datei laden → `--set` (und `--max-urls`) → `--resolve` → Validierung.
/// Warnungen aus Datei und Validierung gehen als Warnungen nach stderr.
fn build_config(
    mut loaded: LoadedConfig,
    args: &RunArgs,
    list_mode: bool,
    rep: &Reporter,
) -> Result<CrawlConfig, CliError> {
    for w in &loaded.warnings {
        rep.warn(format!("Konfiguration: {w}"));
    }
    if list_mode {
        // Listenmodus: robots.txt ignorieren, Tiefe 0 (CRAWL-028); `--set` kann es wieder ändern.
        loaded.config = list_input::list_config(loaded.config);
    }
    for assignment in &args.set {
        loaded
            .config
            .apply_set(assignment)
            .map_err(|e| set_error(&e))?;
    }
    if let Some(n) = &args.max_urls {
        loaded
            .config
            .set_value("limits.max_urls", n)
            .map_err(|e| set_error(&e))?;
    }
    for rule in &args.resolve {
        os_fetch::resolve::ResolveRule::parse(rule).map_err(CliError::usage)?;
        loaded.config.network.resolve.push(rule.clone());
    }
    for issue in os_engine::config_check::validate(&loaded.config) {
        if issue.code == "custom-js-network" {
            rep.warn(format!("Konfiguration: {issue}"));
        }
    }
    // OUT-201 / INTEG-017: apply selections before validating and before creating a crawl.
    apply_api_options(&mut loaded.config, args)?;
    let report = loaded.validate();
    for w in &report.warnings {
        rep.warn(format!("Konfiguration: {w}"));
    }
    if !report.errors.is_empty() {
        let lines: Vec<String> = report
            .errors
            .iter()
            .map(|e| format!("Konfiguration: {e}"))
            .collect();
        return Err(CliError::usage(lines.join("\n")));
    }
    Ok(loaded.config)
}

fn load_config_file(path: &Path) -> Result<LoadedConfig, CliError> {
    CrawlConfig::load_file(path).map_err(|e| CliError::usage(format!("Konfigurationsdatei: {e}")))
}

/// Standardprofil des Einstellungsordners (OUT-220), falls vorhanden.
fn default_profile(paths: &AppPaths) -> Result<LoadedConfig, CliError> {
    CrawlConfig::load_default(&paths.settings)
        .map_err(|e| CliError::usage(format!("Standardprofil: {e}")))
}

fn host_of(url: &str) -> String {
    os_url::parse_start_url(url)
        .ok()
        .and_then(|n| n.url.host_str().map(str::to_string))
        .unwrap_or_else(|| url.to_string())
}

fn is_http_url(s: &str) -> bool {
    let lower = s.trim().to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://")
}

fn file_label(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

/// Liest `--crawl-list`/`--crawl-sitemap`: Dateien als Zeilen bzw. Zellen, URLs unverändert (Sitemaps ruft
/// die Engine ab).
fn read_list_sources(sources: &[String]) -> Result<(Vec<String>, String), CliError> {
    let mut items = Vec::new();
    let mut label: Option<String> = None;
    for source in sources {
        let source = source.trim();
        if is_http_url(source) {
            label.get_or_insert_with(|| host_of(source));
            items.push(source.to_string());
            continue;
        }
        let path = Path::new(source);
        if !path.is_file() {
            return Err(CliError::usage(format!(
                "Listenquelle '{source}' nicht gefunden (Datei oder http(s)-URL einer Sitemap erwartet)"
            )));
        }
        label.get_or_insert_with(|| file_label(path));
        let read = list_input::read_items(path)
            .map_err(|e| CliError::usage(format!("Listenquelle '{source}' nicht lesbar: {e}")))?;
        items.extend(read);
    }
    Ok((items, label.unwrap_or_else(|| "Liste".to_string())))
}

fn list_counter_line(c: &ListCounts) -> String {
    format!(
        "Liste: Gelesen {}, Ungültig {}, Ignorierte Zeilen {}, Duplikate entfernt {}, \
         Durch Exclude/Include entfernt {}, Sitemap-Dateien {}, Zu crawlen {}",
        c.read,
        c.invalid,
        c.ignored_lines,
        c.duplicates,
        c.excluded + c.not_included,
        c.sitemaps,
        c.to_crawl
    )
}

/// Öffnet den Crawl zu `--load-crawl` (Pfad oder ID) und liefert Pfad und gespeicherte Konfiguration.
fn open_loaded(arg: &str, paths: &AppPaths) -> Result<(PathBuf, CrawlConfig), CliError> {
    let candidate = Path::new(arg);
    let path = if candidate.is_file() {
        candidate.to_path_buf()
    } else if let Ok(id) = CrawlId::from_str(arg.trim()) {
        registry::find(&paths.crawls, &id).ok_or_else(|| {
            CliError::usage(format!(
                "Crawl mit der ID {id} nicht gefunden – `openspider --list-crawls` zeigt die vorhandenen Crawls"
            ))
        })?
    } else {
        return Err(CliError::usage(format!(
            "'{arg}' ist weder eine Crawl-Datei noch eine Crawl-ID – \
             `openspider --list-crawls` zeigt die vorhandenen Crawls"
        )));
    };
    let file = CrawlFile::open(&path, OpenMode::ReadWrite)
        .map_err(|e| CliError::usage(format!("Crawl-Datei nicht ladbar: {e}")))?;
    let config = file
        .config()
        .map_err(|e| CliError::usage(format!("Crawl-Datei nicht ladbar: {e}")))?;
    Ok((path, config))
}

fn output_mode(args: &RunArgs, rep: &Reporter) -> OutputMode {
    match (args.timestamped_output, args.overwrite) {
        (true, true) => {
            rep.warn(
                "--timestamped-output und --overwrite zusammen angegeben – \
                 --timestamped-output hat Vorrang",
            );
            OutputMode::Timestamped
        }
        (true, false) => OutputMode::Timestamped,
        (false, true) => OutputMode::Overwrite,
        (false, false) => OutputMode::RequireEmpty,
    }
}

fn resolve_exports(
    args: &RunArgs,
    cfg: &os_model::CrawlConfig,
    lenient: Option<&Reporter>,
) -> Result<Vec<Resolved>, CliError> {
    let mut exports: Vec<Resolved> = Vec::new();
    let mut problems: Vec<String> = Vec::new();
    for (kind, lists) in [
        (Kind::Tab, &args.export_tabs),
        (Kind::Bulk, &args.bulk_export),
        (Kind::Report, &args.save_report),
    ] {
        for list in lists {
            for name in names::split_list(list) {
                match names::resolve_with(kind, &name, cfg) {
                    Ok(resolved) => {
                        if !exports.iter().any(|e| e.key() == resolved.key()) {
                            exports.push(resolved);
                        }
                    }
                    Err(unknown) => match lenient {
                        // Vorlage aus einer neueren Version: Warnung, die übrigen Exporte laufen (OUT-075).
                        Some(rep) => rep.warn(format!(
                            "Export übersprungen – {}",
                            names::unknown_message(kind, &unknown)
                        )),
                        None => problems.push(names::unknown_message(kind, &unknown)),
                    },
                }
            }
        }
    }
    if problems.is_empty() {
        Ok(exports)
    } else {
        Err(CliError::usage(problems.join("\n")))
    }
}

/// Prüft alle Argumente und bereitet den Lauf vor. Kein Netzzugriff.
pub fn build(args: &RunArgs, paths: AppPaths, rep: &Reporter) -> Result<Plan, CliError> {
    checked(build_impl(args, paths, rep, None, None, &SystemClock))
}

/// Vergleichs-Unterfilter (OUT-151) nur mit Vergleich und nicht am Tab „Change Detection“ (OUT-203).
fn checked(plan: Result<Plan, CliError>) -> Result<Plan, CliError> {
    let plan = plan?;
    let comparison = plan.compare_with_previous || plan.explicit_previous.is_some();
    for item in &plan.exports {
        if let Resolved::CompareTab { tab, filter, class } = item {
            let name = format!("{tab}:{filter}:{}", class.as_str());
            if tab == os_analyze::compare::CHANGE_DETECTION_TAB {
                return Err(CliError::usage(format!(
                    "'{name}': Change Detection hat keine Unterfilter Added/New/Removed/Missing (TECH-307)"
                )));
            }
            if !comparison {
                return Err(CliError::usage(format!(
                    "'{name}': Unterfilter nur im Vergleich – --crawl-comparison <id1> <id2> oder --project-name <Name> --compare-with-previous (OUT-151)"
                )));
            }
        }
    }
    Ok(plan)
}

/// Wie [`build`] für geplante Aufgaben: Exporte als Stapel mit den Optionen aus `batch`, unbekannte
/// Exportnamen werden zu Warnungen.
pub fn build_for_task(
    args: &RunArgs,
    paths: AppPaths,
    rep: &Reporter,
    batch: BatchSettings,
) -> Result<Plan, CliError> {
    checked(build_impl(
        args,
        paths,
        rep,
        Some(batch),
        None,
        &SystemClock,
    ))
}

/// Task execution keeps its injected secret backend, including machine-bound auth profiles.
pub fn build_for_task_with_secrets(
    args: &RunArgs,
    paths: AppPaths,
    rep: &Reporter,
    batch: BatchSettings,
    secrets: &dyn os_fetch::SecretStore,
) -> Result<Plan, CliError> {
    build_for_task_with_clock(args, paths, rep, batch, secrets, &SystemClock)
}

/// Scheduling uses its own clock for imported cookie expiry as well as task timestamps.
pub fn build_for_task_with_clock(
    args: &RunArgs,
    paths: AppPaths,
    rep: &Reporter,
    batch: BatchSettings,
    secrets: &dyn os_fetch::SecretStore,
    clock: &dyn os_model::Clock,
) -> Result<Plan, CliError> {
    checked(build_impl(
        args,
        paths,
        rep,
        Some(batch),
        Some(secrets),
        clock,
    ))
}

fn build_impl(
    args: &RunArgs,
    paths: AppPaths,
    rep: &Reporter,
    batch: Option<BatchSettings>,
    secrets: Option<&dyn os_fetch::SecretStore>,
    clock: &dyn os_model::Clock,
) -> Result<Plan, CliError> {
    if args.use_google_analytics.is_some() {
        return Err(CliError::usage("--use-google-analytics (Universal Analytics) wird nicht mehr unterstützt; --use-google-analytics-4 verwenden"));
    }
    let unsupported = args.unsupported();
    if !unsupported.is_empty() {
        return Err(CliError::usage(format!(
            "{} wird in dieser Version noch nicht unterstützt",
            unsupported.join(", ")
        )));
    }

    let compare_with_previous = match args.project_crawl_comparison.as_ref() {
        None => args.compare_with_previous,
        Some(None) => true,
        Some(Some(value)) if value.eq_ignore_ascii_case("true") => true,
        Some(Some(value)) if value.eq_ignore_ascii_case("false") && !args.compare_with_previous => false,
        _ => return Err(CliError::usage("--project-crawl-comparison: gültige Werte true oder false; widersprüchliche Angaben. Syntax: --project-name \"MeinProjekt\" --project-crawl-comparison true")),
    };
    if compare_with_previous
        && args
            .project_name
            .as_ref()
            .is_none_or(|p| p.trim().is_empty())
    {
        return Err(CliError::usage(
            "Projektname fehlt: --project-name <Name> --project-crawl-comparison true",
        ));
    }
    if compare_with_previous && args.no_store {
        return Err(CliError::usage("Auto comparison requires a stored crawl"));
    }
    if let Some(ids) = &args.crawl_comparison {
        if ids.len() != 2
            || args.crawl.is_some()
            || !args.crawl_list.is_empty()
            || !args.crawl_sitemap.is_empty()
            || args.load_crawl.is_some()
            || args.resume
            || args.no_store
            || compare_with_previous
        {
            return Err(CliError::usage("--crawl-comparison erwartet zwei Crawls und schließt andere Quellen und Auto-Vergleich aus"));
        }
        let mut sides = Vec::new();
        for id in ids {
            let (path, _) = open_loaded(id, &paths)?;
            let file = CrawlFile::open(&path, OpenMode::ReadOnly)
                .map_err(|e| CliError::usage(e.to_string()))?;
            let meta = file.meta().map_err(|e| CliError::usage(e.to_string()))?;
            sides.push((meta.started_at, meta.crawl_id, path));
        }
        if sides[0].1 == sides[1].1 {
            return Err(CliError::usage(
                "Vergleich mit sich selbst ist nicht erlaubt",
            ));
        }
        sides.sort_by_key(|(at, id, _)| (*at, id.to_string()));
        let comparison_config = args
            .config
            .as_ref()
            .map(|path| {
                let loaded = load_config_file(path)?;
                build_config(loaded, args, false, rep).map(|cfg| cfg.compare)
            })
            .transpose()?;
        let mut effective = args.clone();
        effective.config = None;
        effective.crawl_comparison = None;
        effective.load_crawl = Some(sides[1].2.to_string_lossy().into_owned());
        let mut plan = build_impl(&effective, paths, rep, batch, secrets, clock)?;
        plan.explicit_previous = Some(sides[0].2.clone());
        plan.explicit_compare_config = comparison_config;
        return Ok(plan);
    }
    let format = parse_format(args)?;
    let xls = args
        .export_format
        .as_deref()
        .is_some_and(|s| s.eq_ignore_ascii_case("xls"));
    let mut google = crate::google::validate(args, &paths)?;

    // Quelle
    let list_sources: Vec<String> = args
        .crawl_list
        .iter()
        .chain(args.crawl_sitemap.iter())
        .cloned()
        .collect();
    let source_count = usize::from(args.crawl.is_some())
        + usize::from(!list_sources.is_empty())
        + usize::from(args.load_crawl.is_some());
    if source_count == 0 {
        return Err(CliError::usage(
            "keine Quelle angegeben: --crawl <url>, --crawl-list <quelle> oder --load-crawl \
             <datei|id> (Hilfe: openspider --help)",
        ));
    }
    if source_count > 1 {
        return Err(CliError::usage(
            "--crawl, --crawl-list/--crawl-sitemap und --load-crawl schließen sich gegenseitig aus",
        ));
    }
    if args.resume && args.load_crawl.is_none() {
        return Err(CliError::usage("--resume braucht --load-crawl <datei|id>"));
    }
    if args.no_store && args.load_crawl.is_some() {
        return Err(CliError::usage(
            "--no-store gilt nur für neue Crawls; ein geladener Crawl wird nie gelöscht",
        ));
    }

    let has_overrides = args.config.is_some()
        || !args.set.is_empty()
        || args.max_urls.is_some()
        || !args.resolve.is_empty();

    let (source, mut config, mut resume_config, project_default_name) =
        if let Some(arg) = &args.load_crawl {
            let (path, stored) = open_loaded(arg, &paths)?;
            let mut resume_config = None;
            let mut config = stored.clone();
            if has_overrides {
                if args.resume {
                    let base = match &args.config {
                        Some(file) => load_config_file(file)?,
                        None => LoadedConfig::from_config(stored),
                    };
                    let cfg = build_config(base, args, false, rep)?;
                    config = cfg.clone();
                    resume_config = Some(cfg);
                } else {
                    rep.warn(
                    "--config, --set, --max-urls und --resolve wirken nur beim Crawlen oder mit \
                     --resume; die gespeicherte Konfiguration des Crawls bleibt maßgeblich",
                );
                }
            }
            (Source::Load { path }, config, resume_config, None)
        } else {
            let list_mode = args.crawl.is_none();
            let base = match &args.config {
                Some(file) => load_config_file(file)?,
                None => default_profile(&paths)?,
            };
            let mut config = build_config(base, args, list_mode, rep)?;
            if let Some(url) = &args.crawl {
                if !url.contains("://") {
                    rep.info(format!(
                        "Hinweis: URL ohne Schema – 'http://' wird ergänzt ({})",
                        os_fetch::redact::url(url)
                    ));
                }
                let norm = os_url::parse_start_url(url).map_err(|e| {
                    CliError::usage(format!(
                        "ungültige Start-URL '{}': {e}",
                        os_fetch::redact::url(url)
                    ))
                })?;
                let name = host_of(url);
                (
                    Source::Spider {
                        start_url: norm.encoded,
                    },
                    config,
                    None,
                    Some(name),
                )
            } else {
                let (mut items, label) = read_list_sources(&list_sources)?;
                let originals = crate::upload::prepare(&mut items);
                let plan = list_input::plan(&items, &config)
                    .map_err(|e| CliError::usage(format!("Liste nicht auswertbar: {e}")))?;
                rep.info(list_counter_line(&plan.counts));
                if plan.counts.read == 0 && plan.counts.sitemaps == 0 {
                    return Err(CliError::usage(
                    "Leere Liste: keine einzige URL gefunden (erwartet: http(s)-URLs, eine Sitemap \
                     oder eine Datei mit URLs)",
                ));
                }
                config.mode = os_model::CrawlMode::List;
                (
                    Source::List {
                        items,
                        originals,
                        label: label.clone(),
                    },
                    config,
                    None,
                    Some(label),
                )
            }
        };

    let task_mode = batch.is_some();
    let mut effective = args.clone();
    let mut batch = batch;
    if let Some(preset) = args.export_preset.as_deref() {
        let candidate = Path::new(preset);
        let path = if candidate.is_file() {
            candidate.to_path_buf()
        } else {
            if candidate.components().count() != 1 || preset.contains(['/', '\\']) || preset == ".."
            {
                return Err(CliError::usage("Export-Preset nicht gefunden"));
            }
            paths
                .settings
                .join("export-presets")
                .join(format!("{preset}.ospreset"))
        };
        let preset = os_export::types::ExportPreset::load(&path)
            .map_err(|e| CliError::usage(format!("Export-Preset: {e}")))?;
        batch = Some(BatchSettings {
            single_workbook: preset.single_workbook,
            skip_empty: preset.skip_empty,
        });
        for item in preset.items {
            let name = item.name.replace(',', "\\,");
            match item.kind.to_ascii_lowercase().as_str() {
                "tab" | "view" => effective.export_tabs.push(name),
                "bulk" => effective.bulk_export.push(name),
                "report" => effective.save_report.push(name),
                other => {
                    return Err(CliError::usage(format!(
                        "Unbekannte Exportart im Preset: {other}"
                    )))
                }
            }
        }
    }
    if args.single_workbook || args.skip_empty_exports {
        let settings = batch.get_or_insert_with(BatchSettings::default);
        settings.single_workbook |= args.single_workbook;
        settings.skip_empty |= args.skip_empty_exports;
    }
    let exports = resolve_exports(
        &effective,
        &config,
        args.export_preset
            .is_some()
            .then_some(rep)
            .or_else(|| batch.is_some().then_some(rep)),
    )?;
    let crawl_path_urls = args
        .export_crawl_path
        .iter()
        .flat_map(|arg| arg.split(','))
        .map(|raw| {
            os_url::parse_start_url(raw)
                .map(|url| url.encoded)
                .map_err(|e| CliError::usage(format!("--export-crawl-path: {e}")))
        })
        .collect::<Result<Vec<_>, _>>()?;
    // Explicit CLI names always fail before a crawl, including when a preset is used.
    if !task_mode {
        resolve_exports(args, &config, None)?;
    }
    if batch.is_some_and(|b| b.single_workbook) && xls {
        return Err(CliError::usage(
            "--single-workbook benötigt xlsx oder gsheet",
        ));
    }
    if let Some(segment) = args.segment.as_deref() {
        if segment != "all" && !config.segments.list.iter().any(|s| s.name == segment) {
            return Err(CliError::usage(format!("Unbekanntes Segment: {segment}")));
        }
    }
    if let Some(profile) = &args.auth_config {
        config
            .auth
            .profile_refs
            .push(profile.to_string_lossy().into_owned());
    }
    let auth = if matches!(source, Source::Load { .. }) && !args.resume {
        crate::auth::AuthSession::default()
    } else if let Some(secrets) = secrets {
        crate::auth::load_with_clock(
            &config,
            &paths,
            rep,
            secrets,
            std::env::var("OPENSPIDER_AUTH_PASSPHRASE").ok().as_deref(),
            clock,
        )?
    } else {
        crate::auth::load(&config, &paths, rep)?
    };
    if resume_config.is_some() || (args.resume && args.auth_config.is_some()) {
        resume_config = Some(config.clone());
    }
    if config.rendering.chromium_path.is_none() && std::env::var_os("OPENSPIDER_CHROMIUM").is_none()
    {
        config.rendering.chromium_path =
            crate::browser::installed(&paths).map(|p| p.to_string_lossy().into_owned());
        if resume_config.is_some() {
            resume_config = Some(config.clone());
        }
    }
    if config.rendering.mode == os_model::RenderingMode::JavaScript
        && (!matches!(source, Source::Load { .. }) || args.resume)
    {
        os_render::api::check_available(&os_render::LaunchOpts {
            chromium_path: config.rendering.chromium_path.as_ref().map(Into::into),
            ..Default::default()
        })
        .map_err(|e| {
            CliError::usage(format!(
                "JavaScript-Rendering: {e}; Chromium installieren: openspider --install-browser"
            ))
        })?;
    }

    // Ausgabeordner (nur, wenn Dateien entstehen)
    let output = if !exports.is_empty()
        || !crawl_path_urls.is_empty()
        || args.save_crawl
        || args.create_sitemap
        || args.create_images_sitemap
    {
        let mode = output_mode(args, rep);
        let base = match &args.output_folder {
            Some(dir) => dir.clone(),
            None => std::env::current_dir()
                .map_err(|e| CliError::usage(format!("Arbeitsverzeichnis nicht lesbar: {e}")))?,
        };
        let dir = output::prepare(&base, mode, &SystemClock)
            .map_err(|e| CliError::usage(format!("Ausgabeordner: {e}")))?;
        Some(dir)
    } else {
        None
    };

    if args.timestamped_output {
        if let Some(target) = &mut google {
            target.timestamp = output
                .as_ref()
                .and_then(|p| p.file_name())
                .map(|s| s.to_string_lossy().into_owned());
        }
    }

    // Plan M4 E1 (E-2 b, M4-F02): ohne Schalter „KI (Vorschau)“ der App-Einstellungen läuft der Crawl (auch als
    // geplante Aufgabe) ohne KI-Schritte; eingeschränkt wird die Laufkopie, nicht das gespeicherte Profil.
    let preview = names::preview_in(&paths);
    preview.restrict(&mut config);
    if let Some(resume) = resume_config.as_mut() {
        preview.restrict(resume);
    }

    let project = args
        .project_name
        .clone()
        .filter(|p| !p.trim().is_empty())
        .unwrap_or_else(|| "Default".to_string());
    let name = args
        .task_name
        .clone()
        .filter(|n| !n.trim().is_empty())
        .or(project_default_name)
        .unwrap_or_default();

    Ok(Plan {
        api_secrets: None,
        require_apis: args.require_apis,
        header_labels: crate::column_headers::labels(args.column_headers.as_deref(), &paths)?,
        auth,
        create_sitemap: args.create_sitemap,
        create_images_sitemap: args.create_images_sitemap,
        xls,
        google,
        compare_with_previous,
        explicit_previous: None,
        explicit_compare_config: None,
        batch,
        segment: args.segment.clone(),
        source,
        config,
        resume_config,
        exports,
        crawl_path_urls,
        format,
        output,
        save_crawl: args.save_crawl,
        no_store: args.no_store,
        resume: args.resume,
        fail_on_start_error: args.fail_on_start_error,
        strict: args.strict,
        project,
        name,
        paths,
    })
}

fn apply_api_options(cfg: &mut CrawlConfig, args: &RunArgs) -> Result<(), CliError> {
    for (flag, values, count) in [
        (
            "--use-google-analytics-4",
            args.use_google_analytics_4.as_deref(),
            4,
        ),
        (
            "--use-google-search-console",
            args.use_google_search_console.as_deref(),
            2,
        ),
    ] {
        if let Some(values) = values {
            if values.len() != count || values.iter().any(|s| s.trim().is_empty()) {
                return Err(CliError::usage(format!(
                    "{flag}: {count} nicht leere Werte für Konto und Property erforderlich"
                )));
            }
        }
    }
    os_integrations::crawl_glue::configure_google(
        cfg,
        args.use_google_analytics_4.as_deref(),
        args.use_google_search_console.as_deref(),
    )
    .map_err(|e| CliError::usage(e.to_string()))?;
    cfg.api.psi.enabled |= args.use_pagespeed;
    cfg.api.ahrefs.enabled |= args.use_ahrefs;
    cfg.api.majestic.enabled |= args.use_majestic;
    cfg.api.moz.enabled |= args.use_mozscape;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rep() -> std::sync::Arc<Reporter> {
        Reporter::new(false)
    }

    fn paths(dir: &Path) -> AppPaths {
        os_store::paths::app_paths_with_home(Some(dir.as_os_str()))
    }

    fn args(list: &[&str]) -> RunArgs {
        use clap::Parser;
        let mut full = vec!["openspider"];
        full.extend_from_slice(list);
        crate::args::Cli::try_parse_from(full).unwrap().run
    }

    // spec: OUT-186
    #[test]
    fn set_overrides_the_config_file_and_an_unknown_key_suggests_the_closest() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("a.osconfig");
        std::fs::write(&file, "[speed]\nthreads = 5\n").unwrap();
        let cfg_path = file.to_string_lossy().into_owned();

        let plan = build(
            &args(&[
                "--crawl",
                "http://site.test/",
                "--config",
                &cfg_path,
                "--set",
                "threads=10",
            ]),
            paths(dir.path()),
            &rep(),
        )
        .unwrap();
        assert_eq!(plan.config.speed.threads, 10);

        let err = build(
            &args(&["--crawl", "http://site.test/", "--set", "threadz=1"]),
            paths(dir.path()),
            &rep(),
        )
        .unwrap_err();
        assert_eq!(err.exit, crate::exit::Exit::Usage);
        assert!(err.message.contains("threads"), "{}", err.message);

        let err = build(
            &args(&["--crawl", "http://site.test/", "--set", "threads=abc"]),
            paths(dir.path()),
            &rep(),
        )
        .unwrap_err();
        assert!(
            err.message.contains("ungültiger Wert für threads"),
            "{}",
            err.message
        );
    }

    // spec: OUT-186
    #[test]
    fn max_urls_is_a_short_form_of_the_limit_key() {
        let dir = tempfile::tempdir().unwrap();
        let plan = build(
            &args(&["--crawl", "http://site.test/", "--max-urls", "7"]),
            paths(dir.path()),
            &rep(),
        )
        .unwrap();
        assert_eq!(plan.config.limits.max_urls, 7);
    }

    // spec: OUT-182, OUT-191
    #[test]
    fn spider_defaults_project_and_name_from_the_host() {
        let dir = tempfile::tempdir().unwrap();
        let plan = build(
            &args(&["--crawl", "Site.Test/start"]),
            paths(dir.path()),
            &rep(),
        )
        .unwrap();
        assert_eq!(plan.project, "Default");
        assert_eq!(plan.name, "site.test");
        let Source::Spider { start_url } = &plan.source else {
            panic!("Spider erwartet");
        };
        assert_eq!(start_url, "http://site.test/start");
    }

    // spec: OUT-182
    #[test]
    fn an_invalid_start_url_is_a_usage_error() {
        let dir = tempfile::tempdir().unwrap();
        let err = build(&args(&["--crawl", "ftp://x"]), paths(dir.path()), &rep()).unwrap_err();
        assert_eq!(err.exit, crate::exit::Exit::Usage);
    }

    // spec: OUT-183
    #[test]
    fn an_empty_list_is_a_usage_error() {
        let dir = tempfile::tempdir().unwrap();
        let list = dir.path().join("leer.txt");
        std::fs::write(&list, "# nichts\n\n").unwrap();
        let err = build(
            &args(&["--crawl-list", &list.to_string_lossy()]),
            paths(dir.path()),
            &rep(),
        )
        .unwrap_err();
        assert!(err.message.contains("Leere Liste"), "{}", err.message);
    }

    // spec: OUT-183
    #[test]
    fn list_mode_uses_the_list_defaults_and_the_file_name() {
        let dir = tempfile::tempdir().unwrap();
        let list = dir.path().join("urls.txt");
        std::fs::write(&list, "http://site.test/a\nhttp://site.test/b\n").unwrap();
        let plan = build(
            &args(&["--crawl-list", &list.to_string_lossy()]),
            paths(dir.path()),
            &rep(),
        )
        .unwrap();
        assert_eq!(plan.name, "urls.txt");
        assert_eq!(plan.config.limits.max_depth, Some(0));
        assert_eq!(plan.config.robots.mode, os_model::RobotsMode::Ignore);
    }

    // spec: OUT-195
    #[test]
    fn unsupported_and_unknown_formats_are_usage_errors() {
        let dir = tempfile::tempdir().unwrap();
        for (format, needle) in [("gsheet", "gsheet"), ("pdf", "csv, xls, xlsx, gsheet")] {
            let err = build(
                &args(&["--crawl", "http://site.test/", "--export-format", format]),
                paths(dir.path()),
                &rep(),
            )
            .unwrap_err();
            assert_eq!(err.exit, crate::exit::Exit::Usage);
            assert!(err.message.contains(needle), "{format}: {}", err.message);
        }
    }

    // spec: OUT-192
    #[test]
    fn duplicate_exports_are_resolved_once() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("out");
        let plan = build(
            &args(&[
                "--crawl",
                "http://site.test/",
                "--export-tabs",
                "Internal:All, internal : all",
                "--output-folder",
                &out.to_string_lossy(),
            ]),
            paths(dir.path()),
            &rep(),
        )
        .unwrap();
        assert_eq!(plan.exports.len(), 1);
    }

    // spec: OUT-190
    #[test]
    fn a_non_empty_output_folder_without_overwrite_is_rejected_before_the_crawl() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("out");
        std::fs::create_dir_all(&out).unwrap();
        std::fs::write(out.join("alt.csv"), "x").unwrap();
        let base = [
            "--crawl",
            "http://site.test/",
            "--export-tabs",
            "Internal:All",
            "--output-folder",
        ];
        let out_arg = out.to_string_lossy().into_owned();
        let mut argv: Vec<&str> = base.to_vec();
        argv.push(&out_arg);
        let err = build(&args(&argv), paths(dir.path()), &rep()).unwrap_err();
        assert_eq!(err.exit, crate::exit::Exit::Usage);
        argv.push("--overwrite");
        assert!(build(&args(&argv), paths(dir.path()), &rep()).is_ok());
    }

    #[test]
    fn sources_are_mutually_exclusive_and_required() {
        let dir = tempfile::tempdir().unwrap();
        let none = build(&args(&[]), paths(dir.path()), &rep()).unwrap_err();
        assert!(none.message.contains("keine Quelle"));
        let both = build(
            &args(&["--crawl", "http://a.test/", "--load-crawl", "x"]),
            paths(dir.path()),
            &rep(),
        )
        .unwrap_err();
        assert!(both.message.contains("schließen sich"));
        let resume = build(
            &args(&["--crawl", "http://a.test/", "--resume"]),
            paths(dir.path()),
            &rep(),
        )
        .unwrap_err();
        assert!(resume.message.contains("--resume"));
    }
}
