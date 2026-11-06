//! compare.* (OUT-140 ff., M2-C01): eine Vergleichssitzung je Fenster.
//!
//! `compare.open` öffnet beide Crawls (Kennung aus der Crawl-Verwaltung oder Pfad einer Crawl-Datei) und ordnet sie nach
//! Startzeitpunkt (der spätere ist Current, OUT-140); ältere Schemaversionen werden als Kopie im Cache-Ordner migriert.
//! `compare.swap` vertauscht die Rollen (OUT-141), `compare.testMapping` ist der Test-Reiter des URL-Mappings (OUT-146),
//! `compare.close` beendet die Sitzung und löscht migrierte Kopien.
//!
//! Die Vergleichssitzung hält zwei eigene, nur lesende Verbindungen (zwei Dateien gleichzeitig, das gibt der Lese-Pool
//! je Aufruf nicht her); jeder Zugriff läuft in `spawn_blocking`, nie auf einem Tokio-Worker (2.3 Regel 1). Die
//! Berechnung prüft das Token der Operation: der Ressourcenwächter (`os_engine::memory::watch_operation`) pausiert sie
//! bei Speicherknappheit, statt sie abzubrechen (CRAWL-129, K2-56).
//!
//! Für Overview, Issues, Views und Exporte im Vergleichsmodus stellen [`with_active`] und [`active_version`] die
//! Sitzung des Fensters bereit; die `compare`-Parameter von `catalog.*`, `sidebar.*` und `view.*` gehören deren
//! Besitzern (Schnittstellenbedarf im Bericht `docs/plan/M2-reports/c01.md`).

pub(crate) mod ui;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use futures_util::future::BoxFuture;
use os_analyze::compare::change;
use os_analyze::compare::{CompareError, CompareNote, Comparison, CrawlSide, Side, UrlMapping};
use os_model::{ChangeDetectionConfig, ChangeElement, CrawlState, MappingRule};

use crate::dto::*;
use crate::registry::{CmdResult, CommandMeta, Registry};
use crate::runtime::error::{internal, parse_args, store_error, to_json};
use crate::runtime::{App, CallCtx, WindowId};

/// Eine geöffnete Vergleichssitzung.
pub struct ActiveComparison {
    /// Der Vergleich.
    pub comparison: Comparison,
    /// Aktive Elemente der Change Detection (Kennungen laut TECH-300, M2-C02).
    pub elements: Vec<String>,
    /// Schwelle für Inhaltsänderungen in Prozent.
    pub content_threshold_pct: u8,
    /// Ansichten der Change Detection dieser Vergleichssitzung.
    change_views: HashMap<String, ChangeView>,
}

#[derive(Default)]
struct CompareState {
    sessions: Mutex<HashMap<WindowId, Arc<Mutex<ActiveComparison>>>>,
}

fn state(app: &App) -> Arc<CompareState> {
    app.ext::<CompareState>()
}

fn session(app: &App, window: &WindowId) -> Option<Arc<Mutex<ActiveComparison>>> {
    state(app)
        .sessions
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .get(window)
        .cloned()
}

fn no_comparison() -> CmdError {
    CmdError::new(ErrorCode::NotFound, "err-no-comparison")
}

/// Fehler des Vergleichs als Command-Fehler.
pub fn compare_error(e: CompareError) -> CmdError {
    match e {
        CompareError::Store(e) => store_error(&e),
        CompareError::Sql(e) => store_error(&os_store::StoreError::from(e)),
        CompareError::Cancelled => CmdError::new(ErrorCode::Cancelled, "err-cancelled"),
        CompareError::Mapping(e) => CmdError::invalid_input(
            "err-mapping-pattern",
            format!("mapping[{}].pattern", e.rule),
        )
        .with_arg("rule", e.rule)
        .with_arg("position", e.position)
        .with_detail(e.message),
        CompareError::View(e) => super::catalog::view_error(e),
        CompareError::Invalid(detail) => {
            CmdError::invalid_input("err-compare-input", "compare").with_detail(detail)
        }
    }
}

/// Führt `f` mit der Vergleichssitzung des Fensters in `spawn_blocking` aus (für Overview, Issues, Views, Exporte im
/// Vergleichsmodus). Ohne Sitzung: `NotFound`.
pub async fn with_active<T: Send + 'static>(
    app: &App,
    window: &WindowId,
    f: impl FnOnce(&mut ActiveComparison) -> Result<T, CmdError> + Send + 'static,
) -> Result<T, CmdError> {
    let active = session(app, window).ok_or_else(no_comparison)?;
    tokio::task::spawn_blocking(move || {
        let mut guard = active.lock().unwrap_or_else(|p| p.into_inner());
        f(&mut guard)
    })
    .await
    .map_err(|e| internal("compare", e.to_string()))?
}

/// Ist `name` der Report „Crawl Changes“ (OUT-154)? Er läuft immer über die Vergleichssitzung des Fensters.
pub fn is_crawl_changes(name: &str) -> bool {
    os_export::catalog::reports::resolve(name)
        .is_ok_and(|d| d.kind == os_export::catalog::reports::ReportKind::CrawlChanges)
}

/// Grund, wenn „Crawl Changes“ nicht verfügbar ist (kein Vergleich im Fenster); dieselbe Meldung wie die CLI.
pub fn crawl_changes_unavailable(app: &App, window: &WindowId) -> Option<String> {
    session(app, window).is_none().then(|| {
        "report 'Crawl Changes' requires a crawl comparison (--crawl-comparison or --compare-with-previous)"
            .to_string()
    })
}

/// Dateiname-Vorschlag für „Crawl Changes“ aus der Startzeit des Current-Crawls (wie OUT-008 für andere Reports).
pub async fn crawl_changes_name(
    app: &App,
    window: &WindowId,
    format: os_export::Format,
) -> Result<String, CmdError> {
    with_active(app, window, move |active| {
        let started = active.comparison.current().meta.started_at;
        Ok(os_export::tab_export::suggest_file_name(
            "Crawl Changes",
            None,
            started,
            None,
            format,
        ))
    })
    .await
}

/// `export.report` für „Crawl Changes“ (OUT-154): schreibt den Report der Vergleichssitzung in `dir`. Ohne Vergleich
/// `Unsupported` mit Grund (OUT-112).
pub async fn crawl_changes_export(
    app: &App,
    ctx: &CallCtx,
    dir: PathBuf,
    format: os_export::Format,
) -> CmdResult {
    if let Some(reason) = crawl_changes_unavailable(app, &ctx.window) {
        return Err(
            CmdError::new(ErrorCode::Unsupported, "err-export-unavailable")
                .with_arg("reason", reason),
        );
    }
    let app_worker = app.clone();
    let window = ctx.window.clone();
    let op = app
        .ops()
        .spawn("export-crawl-changes", true, move |token, _| {
            Box::pin(async move {
                with_active(&app_worker, &window, move |active| {
                    let opts =
                        os_export::pipeline::writer_options(&active.comparison.current().config);
                    let result = os_export::compare::save_report_with_options(
                        &mut active.comparison,
                        "Crawl Changes",
                        &dir,
                        format,
                        &token,
                        &opts,
                    )
                    .map_err(|e| {
                        if token.is_cancelled() {
                            CmdError::new(ErrorCode::Cancelled, "err-cancelled")
                        } else {
                            internal("export.crawlChanges", e.to_string())
                        }
                    })?;
                    let info = result
                        .files
                        .into_iter()
                        .next()
                        .ok_or_else(|| internal("export.crawlChanges", "no output"))?;
                    to_json(&FileResult {
                        path: info.path.to_string_lossy().into_owned(),
                        bytes: info.bytes,
                    })
                })
                .await
            })
        });
    to_json(&op)
}

/// Version der Klassifikation der Sitzung des Fensters (`None` ohne Vergleich).
pub fn active_version(app: &App, window: &WindowId) -> Option<u64> {
    session(app, window).map(|s| {
        s.lock()
            .unwrap_or_else(|p| p.into_inner())
            .comparison
            .version()
    })
}

/// Löst eine Auswahl auf: vorhandene Datei oder Kennung aus der Crawl-Verwaltung.
async fn resolve(app: &App, selection: &str, field: &'static str) -> Result<PathBuf, CmdError> {
    let path = Path::new(selection);
    if tokio::fs::metadata(path).await.is_ok_and(|m| m.is_file()) {
        return Ok(path.to_path_buf());
    }
    let id: os_model::CrawlId = selection
        .parse()
        .map_err(|_| CmdError::invalid_input("err-crawl-id", field))?;
    let listing = super::crawls::admin(app, |m| m.list().map_err(|e| store_error(&e))).await?;
    listing
        .crawls
        .into_iter()
        .find(|l| l.meta.crawl_id == id)
        .map(|l| l.path)
        .ok_or_else(|| {
            CmdError::new(ErrorCode::NotFound, "err-crawl-not-found")
                .with_arg("id", selection)
                .with_arg("field", field)
        })
}

/// Im Blocking-Worker erfasste Angaben einer Seite; `summary` benötigt keinen Dateizugriff.
struct SideInfo {
    meta: os_store::CrawlMeta,
    source: PathBuf,
    urls: u64,
    file_bytes: u64,
}

impl SideInfo {
    fn of(c: &Comparison, side: Side) -> SideInfo {
        let p = c.pairing();
        SideInfo {
            meta: c.side(side).meta.clone(),
            source: c.side(side).source.clone(),
            file_bytes: std::fs::metadata(&c.side(side).source).map_or(0, |d| d.len()),
            urls: match side {
                Side::Current => p.current.len(),
                Side::Previous => p.previous.len(),
            },
        }
    }

    fn summary(&self, now: os_model::Timestamp) -> CrawlSummaryDto {
        let m = &self.meta;
        let state = os_store::registry::effective_state(m, now);
        CrawlSummaryDto {
            crawl_id: m.crawl_id.to_string(),
            file: self.source.to_string_lossy().into_owned(),
            name: m.name.clone(),
            phase: crate::session::feed::phase(state),
            mode: crate::session::feed::mode(m.mode),
            started_at: m.started_at.unwrap_or_default().to_db(),
            finished_at: m.ended_at.map(|t| t.to_db()),
            // Gecrawlte URLs (Existenz im Sinne von OUT-142).
            urls: self.urls,
            file_bytes: self.file_bytes,
            schema_version: m.schema_version,
            owner: CrawlOwner {
                kind: OwnerKind::App,
                pid: None,
                live: state == CrawlState::Running,
                controllable: false,
            },
        }
    }
}

// Nur aus spawn_blocking/with_active aufrufen: SideInfo::of liest die Dateimetadaten.
fn dto(active: &ActiveComparison, now: os_model::Timestamp) -> CompareSession {
    let c = &active.comparison;
    CompareSession {
        current: SideInfo::of(c, Side::Current).summary(now),
        previous: SideInfo::of(c, Side::Previous).summary(now),
        mapping: c
            .rules()
            .iter()
            .map(|r| MappingRuleDto {
                pattern: r.pattern.clone(),
                replacement: r.replacement.clone(),
            })
            .collect(),
        elements: active.elements.clone(),
        content_threshold_pct: active.content_threshold_pct,
        version: c.version(),
    }
}

fn notes_log(notes: &[CompareNote]) {
    for n in notes {
        match n {
            CompareNote::DifferentModes { current, previous } => tracing::info!(
                current = current.as_str(),
                previous = previous.as_str(),
                "Vergleich: Crawls mit unterschiedlichem Modus"
            ),
            CompareNote::Migrated { side, copy } => tracing::info!(
                side = side.as_str(),
                copy = %copy.display(),
                "Vergleich: Crawl-Datei als Kopie migriert"
            ),
            CompareNote::Collision(c) => tracing::warn!(
                current = %c.current_url,
                winner = %c.winner,
                losers = ?c.losers,
                "Vergleich: Kollision im URL-Mapping"
            ),
            CompareNote::MappingWarning(w) => {
                tracing::warn!(rule = w.rule, pattern = %w.pattern, "Vergleich: {}", w.message)
            }
        }
    }
}

fn rules(mapping: &[MappingRuleDto]) -> Vec<MappingRule> {
    mapping
        .iter()
        .map(|r| MappingRule {
            pattern: r.pattern.clone(),
            replacement: r.replacement.clone(),
        })
        .collect()
}

fn remove_copies(copies: &[PathBuf]) {
    for copy in copies {
        for suffix in ["", "-wal", "-shm"] {
            let mut p = copy.as_os_str().to_owned();
            p.push(suffix);
            let _ = std::fs::remove_file(PathBuf::from(p));
        }
    }
}

/// Entfernt die Sitzung des Fensters; Verbindungen schließen und Kopien löschen außerhalb des Tokio-Workers.
async fn close_window(app: &App, window: &WindowId) -> Result<(), CmdError> {
    let old = state(app)
        .sessions
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .remove(window);
    if let Some(old) = old {
        tokio::task::spawn_blocking(move || {
            // Hält ein laufender Aufruf die Sitzung noch, schließt er sie; die Kopien bleiben dann im Cache.
            if let Ok(m) = Arc::try_unwrap(old) {
                let active = m.into_inner().unwrap_or_else(|p| p.into_inner());
                let copies: Vec<PathBuf> = [Side::Current, Side::Previous]
                    .into_iter()
                    .filter_map(|s| active.comparison.side(s).migrated.clone())
                    .collect();
                drop(active);
                remove_copies(&copies);
            }
        })
        .await
        .map_err(|e| internal("compare.close", e.to_string()))?;
    }
    Ok(())
}

/// Wartet vor dem Start, solange der Ressourcenwächter Stufe 2 meldet (CRAWL-129): der Vergleich beginnt pausiert und
/// läuft weiter, sobald der Speicher wieder reicht; während der Berechnung pausiert `watch_operation` das Token.
async fn wait_for_memory(cancel: &os_model::CancelToken) -> Result<(), CmdError> {
    let mut warned = false;
    while os_engine::memory::sample().stage() == os_model::MemoryStage::Paused {
        if cancel.is_cancelled() {
            return Err(CmdError::new(ErrorCode::Cancelled, "err-cancelled"));
        }
        if !warned {
            tracing::warn!("Wenig Arbeitsspeicher – Vergleichsberechnung pausiert");
            warned = true;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    Ok(())
}

fn open<'a>(app: &'a App, ctx: CallCtx, args: serde_json::Value) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move {
        let args: CompareOpenArgs = parse_args(args)?;
        if args.content_threshold_pct.is_some_and(|p| p > 100) {
            return Err(CmdError::invalid_input(
                "err-compare-threshold",
                "contentThresholdPct",
            ));
        }
        for e in &args.elements {
            e.parse::<ChangeElement>().map_err(|_| {
                CmdError::invalid_input("err-compare-element", "elements")
                    .with_arg("element", e.as_str())
            })?;
        }
        let mapping = rules(&args.mapping);
        UrlMapping::compile(&mapping).map_err(|e| compare_error(e.into()))?;
        let current = resolve(app, &args.current, "current").await?;
        let previous = resolve(app, &args.previous, "previous").await?;
        let work = app.config().paths.cache.join("compare");
        let cancel = ctx.cancel.clone();
        wait_for_memory(&cancel).await?;
        let _guard = os_engine::memory::watch_operation(cancel.clone());
        let now = app.config().clock.now();
        let (opened, result) = tokio::task::spawn_blocking(move || -> Result<_, CmdError> {
            std::fs::create_dir_all(&work).map_err(|e| {
                store_error(&os_store::StoreError::Io {
                    path: work.clone(),
                    source: e,
                })
            })?;
            let a = CrawlSide::open(&current, &work).map_err(compare_error)?;
            let b = CrawlSide::open(&previous, &work).map_err(compare_error)?;
            let comparison = Comparison::open(a, b, &mapping, &cancel).map_err(compare_error)?;
            let defaults = &comparison.current().config.compare.change_detection;
            let elements = if args.elements.is_empty() {
                defaults
                    .elements
                    .iter()
                    .map(|e| e.as_str().to_string())
                    .collect()
            } else {
                args.elements.clone()
            };
            let content_threshold_pct = args
                .content_threshold_pct
                .unwrap_or(defaults.content_threshold_pct);
            notes_log(&comparison.notes());
            let elements: Vec<String> = change::ELEMENTS
                .iter()
                .filter(|(e, _, _)| elements.iter().any(|s: &String| s == e.as_str()))
                .map(|(e, _, _)| e.as_str().to_string())
                .collect();
            let opened = ActiveComparison {
                comparison,
                elements,
                content_threshold_pct,
                change_views: HashMap::new(),
            };
            change::calculate(&opened.comparison, &opened.change_config(), &cancel)
                .map_err(compare_error)?;
            let result = dto(&opened, now);
            Ok((opened, result))
        })
        .await
        .map_err(|e| internal("compare.open", e.to_string()))??;
        close_window(app, &ctx.window).await?;
        state(app)
            .sessions
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(ctx.window.clone(), Arc::new(Mutex::new(opened)));
        to_json(&result)
    })
}

fn swap<'a>(app: &'a App, ctx: CallCtx, _: serde_json::Value) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move {
        let now = app.config().clock.now();
        let session = with_active(app, &ctx.window, move |active| {
            active.comparison.swap();
            change::calculate(
                &active.comparison,
                &active.change_config(),
                &os_model::CancelToken::new(),
            )
            .map_err(compare_error)?;
            active.change_views.clear();
            Ok(dto(active, now))
        })
        .await?;
        to_json(&session)
    })
}

fn test_mapping<'a>(_: &'a App, _: CallCtx, args: serde_json::Value) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move {
        let args: TestMappingArgs = parse_args(args)?;
        let mut output = args.input.clone();
        let mut steps = Vec::with_capacity(args.rules.len());
        for (index, r) in args.rules.iter().enumerate() {
            let rule = MappingRule {
                pattern: r.pattern.clone(),
                replacement: r.replacement.clone(),
            };
            match UrlMapping::compile(std::slice::from_ref(&rule)) {
                Ok(one) => {
                    let trace = one.trace(&output);
                    let step = trace.steps.into_iter().next().expect("eine Regel");
                    steps.push(RuleStep {
                        index: index as u32,
                        matched: step.matched,
                        before: step.before,
                        after: step.after.clone(),
                        error: step.error,
                    });
                    output = step.after;
                }
                Err(e) => steps.push(RuleStep {
                    index: index as u32,
                    matched: false,
                    before: output.clone(),
                    after: output.clone(),
                    error: Some(match e.position {
                        Some(p) => {
                            format!("invalid regular expression at position {p}: {}", e.message)
                        }
                        None => format!("invalid regular expression: {}", e.message),
                    }),
                }),
            }
        }
        to_json(&RuleTrace {
            output,
            included: None,
            steps,
        })
    })
}

fn close<'a>(app: &'a App, ctx: CallCtx, _: serde_json::Value) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move {
        close_window(app, &ctx.window).await?;
        Ok(serde_json::Value::Null)
    })
}

/// Read-only capability for TECH-300: selection requires stored content on both sides.
fn content_available<'a>(
    app: &'a App,
    ctx: CallCtx,
    _: serde_json::Value,
) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move {
        if active_version(app, &ctx.window).is_none() {
            return to_json(&false);
        }
        let available = with_active(app, &ctx.window, |active| {
            active
                .comparison
                .content_comparable()
                .map_err(compare_error)
        })
        .await?;
        to_json(&available)
    })
}

/// Trägt die Commands dieser Domäne in die Registry ein (Namen und Typen unverändert laut Katalog).
pub fn register(r: &mut Registry) {
    r.register(
        CommandMeta {
            name: "compare.contentAvailable",
            args: "void",
            ret: "JsonValue",
            long_running: false,
        },
        content_available,
    );
    for (name, args, ret, handler) in [
        (
            "compare.open",
            "CompareOpenArgs",
            "CompareSession",
            open as crate::registry::Handler,
        ),
        ("compare.swap", "void", "CompareSession", swap),
        (
            "compare.testMapping",
            "TestMappingArgs",
            "RuleTrace",
            test_mapping,
        ),
        ("compare.close", "void", "void", close),
    ] {
        r.register(
            CommandMeta {
                name,
                args,
                ret,
                long_running: false,
            },
            handler,
        );
    }
}

#[derive(Clone)]
struct ChangeView {
    spec: ViewSpec,
    columns: Vec<String>,
    version: u64,
}
impl ActiveComparison {
    fn change_config(&self) -> ChangeDetectionConfig {
        ChangeDetectionConfig {
            elements: self
                .elements
                .iter()
                .filter_map(|e| e.parse().ok())
                .collect(),
            content_threshold_pct: self.content_threshold_pct,
        }
    }
    fn change_tab(&self, changed: bool) -> os_analyze::defs::TabDef {
        change::tab(&self.change_config(), changed)
    }
}

/// Tab-Metadaten im Vergleich; außerhalb des Vergleichs ist Change Detection verborgen (TECH-300/301).
pub async fn change_tabs(app: &App, ctx: &CallCtx) -> Result<Vec<TabInfo>, CmdError> {
    let cfg = app
        .sessions()
        .current(&ctx.window)
        .map(|s| s.config)
        .unwrap_or_default();
    let mut result: Vec<TabInfo> = os_analyze::tabs::configured(&cfg)
        .iter()
        .filter(|t| t.id != "Change Detection")
        .enumerate()
        .map(|(i, t)| TabInfo {
            id: t.id.into(),
            default_order: i as u16,
            filters: t
                .filters
                .iter()
                .map(|f| super::catalog::info(t.id, f, &cfg))
                .collect(),
            detail_capable: true,
        })
        .collect();
    let filters = change_counts(app, ctx, None)
        .await?
        .into_iter()
        .map(|r| r.filter)
        .collect::<Vec<_>>();
    if !filters.is_empty() {
        result.push(TabInfo {
            id: "Change Detection".into(),
            default_order: 30,
            filters,
            detail_capable: true,
        });
    }
    Ok(result)
}

/// Vergleichsspezifische Spalten; ohne Sitzung keine fremde Command-Logik ersetzen.
pub async fn change_columns(app: &App, ctx: &CallCtx, args: &ColumnsArgs) -> Option<CmdResult> {
    if args.tab != "Change Detection" {
        return None;
    }
    Some(
        with_active(app, &ctx.window, |active| {
            let tab = active.change_tab(true);
            let cfg = &active.comparison.current().config;
            to_json(
                &tab.columns
                    .iter()
                    .map(|c| super::catalog::column(c, c.key, cfg))
                    .collect::<Vec<_>>(),
            )
        })
        .await,
    )
}

fn change_query(
    view: &ChangeView,
    columns: &[String],
    offset: u64,
    limit: u32,
) -> Result<os_analyze::views::ViewQuery, CmdError> {
    let mut spec = view.spec.clone();
    if let ViewSpec::Tab {
        compare, segment, ..
    } = &mut spec
    {
        if segment.is_some() {
            return Err(CmdError::unsupported("segment filter", "M2-K14"));
        }
        *compare = None;
    }
    super::view::tab_query(&spec, columns, offset, limit)
}
fn changed(spec: &ViewSpec) -> bool {
    !matches!(
        spec,
        ViewSpec::Tab {
            compare: Some(CompareClass::NotChanged),
            ..
        }
    )
}
fn own_spec(spec: &ViewSpec) -> bool {
    matches!(spec,ViewSpec::Tab{tab,..} if tab=="Change Detection")
}
fn change_handle(
    active: &ActiveComparison,
    id: String,
    view: &ChangeView,
) -> Result<ViewHandle, CmdError> {
    let mut q = change_query(view, &view.columns, 0, 1)?;
    let tab = change::tab_for_query(&active.change_config(), changed(&view.spec), &mut q);
    let file = &active.comparison.current().file;
    let cfg = &active.comparison.current().config;
    let page = os_analyze::views::query_view_in(std::slice::from_ref(&tab), file, cfg, &q)
        .map_err(super::catalog::view_error)?;
    Ok(ViewHandle {
        initial_rows: None,
        view_id: id,
        version: view.version,
        total: Some(page.total),
        columns: super::catalog::expanded_columns(&tab, &page.columns, cfg),
        state: ViewState::Ready,
        op: None,
        empty_state: None,
    })
}

fn change_page(
    active: &ActiveComparison,
    view: &ChangeView,
    columns: &[String],
    offset: u64,
) -> Result<os_analyze::views::ViewPage, CmdError> {
    let mut q = change_query(view, columns, offset, 500)?;
    let tab = change::tab_for_query(&active.change_config(), changed(&view.spec), &mut q);
    os_analyze::views::query_view_in(
        &[tab],
        &active.comparison.current().file,
        &active.comparison.current().config,
        &q,
    )
    .map_err(super::catalog::view_error)
}

/// Clipboard-Anbindung für fenstergebundene Change-Views (TECH-301/307).
pub async fn change_copy(app: &App, ctx: &CallCtx, args: &serde_json::Value) -> Option<CmdResult> {
    if !args
        .get("viewId")
        .and_then(|id| id.as_str())
        .is_some_and(|id| id.starts_with("change:"))
    {
        return None;
    }
    let args = args.clone();
    Some(
        async {
            let args: CopyArgs = parse_args(args)?;
            let (text, count) = with_active(app, &ctx.window, move |active| {
                let view = active
                    .change_views
                    .get(&args.view_id)
                    .ok_or_else(|| CmdError::new(ErrorCode::NotFound, "err-view-not-found"))?;
                let (ids, except) = match &args.selection {
                    Selection::Ids { ids } => (
                        Some(
                            ids.iter()
                                .copied()
                                .collect::<std::collections::HashSet<_>>(),
                        ),
                        std::collections::HashSet::new(),
                    ),
                    Selection::All {
                        view_id,
                        version,
                        except,
                    } => {
                        if view_id != &args.view_id || *version != view.version {
                            return Err(super::view::stale());
                        }
                        (None, except.iter().copied().collect())
                    }
                };
                let mut text = String::new();
                let mut count = 0u64;
                let mut offset = 0;
                loop {
                    let page = change_page(active, view, &args.columns, offset)?;
                    let width = page.columns.len() as u64;
                    if width == 0 {
                        return Err(CmdError::invalid_input("err-copy-columns", "columns"));
                    }
                    if offset == 0 && args.with_header {
                        text.push_str(&crate::clipboard::tsv_row(
                            &page
                                .columns
                                .iter()
                                .map(|c| serde_json::json!(c))
                                .collect::<Vec<_>>(),
                            cfg!(windows),
                        ));
                    }
                    for (id, row) in page.row_keys.iter().zip(&page.rows) {
                        if ids.as_ref().is_none_or(|ids| ids.contains(&id.0))
                            && !except.contains(&id.0)
                        {
                            count += 1;
                            if count > crate::clipboard::MAX_COPY_CELLS / width {
                                return Err(crate::clipboard::too_large());
                            }
                            let values = row
                                .iter()
                                .map(|v| serde_json::to_value(v).unwrap_or_default())
                                .collect::<Vec<_>>();
                            text.push_str(&crate::clipboard::tsv_row(&values, cfg!(windows)));
                        }
                    }
                    offset += page.row_keys.len() as u64;
                    if page.row_keys.is_empty() || offset >= page.total {
                        break;
                    }
                }
                Ok((text, count))
            })
            .await?;
            let desktop = app
                .desktop_actions()
                .ok_or_else(|| CmdError::new(ErrorCode::Unsupported, "err-desktop-unavailable"))?;
            let bytes = text.len() as u64;
            tokio::task::spawn_blocking(move || desktop.write_clipboard(&text))
                .await
                .map_err(|e| internal("view.copy", e.to_string()))??;
            to_json(&CopyResult { rows: count, bytes })
        }
        .await,
    )
}

/// Anbindung des bisherigen leeren Tabs an `view.*`; alle anderen Views behalten ihre Handler (K2-71).
pub async fn change_view(
    app: &App,
    ctx: &CallCtx,
    command: &str,
    args: &serde_json::Value,
) -> Option<CmdResult> {
    let own_id = args
        .get("viewId")
        .and_then(|v| v.as_str())
        .is_some_and(|id| id.starts_with("change:"));
    let own_tab = args
        .get("spec")
        .and_then(|s| s.get("tab"))
        .and_then(|s| s.as_str())
        == Some("Change Detection");
    if !own_id && !own_tab {
        return None;
    }
    let args = args.clone();
    let command = command.to_string();
    let id = format!(
        "change:{}",
        app.config().ids.new_crawl_id(app.config().clock.now())
    );
    Some(
        with_active(app, &ctx.window, move |active| match command.as_str() {
            "view.open" => {
                let args: ViewOpenArgs = parse_args(args)?;
                if !own_spec(&args.spec) || active.elements.is_empty() {
                    return Err(CmdError::invalid_input("err-change-disabled", "spec"));
                }
                let view = ChangeView {
                    spec: args.spec,
                    columns: args.columns,
                    version: active.comparison.version(),
                };
                let handle = change_handle(active, id.clone(), &view)?;
                active.change_views.insert(id, view);
                to_json(&handle)
            }
            "view.update" => {
                let args: ViewUpdateArgs = parse_args(args)?;
                if !own_spec(&args.spec) {
                    return Err(CmdError::invalid_input("err-change-tab", "spec"));
                }
                let old = active
                    .change_views
                    .get(&args.view_id)
                    .ok_or_else(|| CmdError::new(ErrorCode::NotFound, "err-view-not-found"))?;
                let view = ChangeView {
                    spec: args.spec,
                    columns: old.columns.clone(),
                    version: old.version + 1,
                };
                let handle = change_handle(active, args.view_id.clone(), &view)?;
                active.change_views.insert(args.view_id, view);
                to_json(&handle)
            }
            "view.rows" => {
                let args: RowsArgs = parse_args(args)?;
                let view = active
                    .change_views
                    .get(&args.view_id)
                    .ok_or_else(|| CmdError::new(ErrorCode::NotFound, "err-view-not-found"))?;
                if args.version != view.version {
                    return Err(super::view::stale());
                }
                let columns = if args.columns.is_empty() {
                    &view.columns
                } else {
                    &args.columns
                };
                let mut q = change_query(view, columns, args.offset, args.limit)?;
                let tab =
                    change::tab_for_query(&active.change_config(), changed(&view.spec), &mut q);
                let page = os_analyze::views::query_view_in(
                    &[tab],
                    &active.comparison.current().file,
                    &active.comparison.current().config,
                    &q,
                )
                .map_err(super::catalog::view_error)?;
                to_json(&RowWindow {
                    view_id: args.view_id,
                    version: view.version,
                    offset: args.offset,
                    row_keys: page.row_keys.iter().map(|i| i.0).collect(),
                    columns: page.columns,
                    cells: page
                        .rows
                        .iter()
                        .map(|r| {
                            r.iter()
                                .map(|v| serde_json::to_value(v).unwrap_or_default())
                                .collect()
                        })
                        .collect(),
                })
            }
            "view.locate" => {
                let args: LocateArgs = parse_args(args)?;
                let view = active
                    .change_views
                    .get(&args.view_id)
                    .ok_or_else(|| CmdError::new(ErrorCode::NotFound, "err-view-not-found"))?;
                let mut offset = 0;
                loop {
                    let page = change_page(active, view, &["Address".into()], offset)?;
                    if let Some(i) = page.row_keys.iter().position(|id| id.0 == args.url_id) {
                        return to_json(&Some(offset + i as u64));
                    }
                    offset += page.row_keys.len() as u64;
                    if page.row_keys.is_empty() || offset >= page.total {
                        return to_json(&Option::<u64>::None);
                    }
                }
            }
            "view.range" => {
                let args: RangeArgs = parse_args(args)?;
                let view = active
                    .change_views
                    .get(&args.view_id)
                    .ok_or_else(|| CmdError::new(ErrorCode::NotFound, "err-view-not-found"))?;
                let mut ids = Vec::new();
                loop {
                    let page = change_page(active, view, &["Address".into()], ids.len() as u64)?;
                    let done = page.row_keys.is_empty()
                        || ids.len() as u64 + page.row_keys.len() as u64 >= page.total;
                    ids.extend(page.row_keys.iter().map(|id| id.0));
                    if done {
                        break;
                    }
                }
                let position = |id: i64| ids.iter().position(|k| *k == id).map(|i| i as u64);
                let (from_row, to_row) = (position(args.from), position(args.to));
                let keys = match (from_row, to_row) {
                    (Some(a), Some(b)) => ids[a.min(b) as usize..=a.max(b) as usize].to_vec(),
                    _ => Vec::new(),
                };
                to_json(&KeyRange {
                    view_id: args.view_id,
                    version: view.version,
                    from_row,
                    to_row,
                    total: ids.len() as u64,
                    keys,
                    all: false,
                })
            }
            "view.close" => {
                let args: ViewCloseArgs = parse_args(args)?;
                active
                    .change_views
                    .remove(&args.view_id)
                    .ok_or_else(|| CmdError::new(ErrorCode::NotFound, "err-view-not-found"))?;
                to_json(&())
            }
            _ => Err(CmdError::invalid_input("err-change-command", "command")),
        })
        .await,
    )
}

/// Exporte nutzen denselben SQL-Plan einschließlich Changed/Not Changed (TECH-307).
pub async fn change_export(
    app: &App,
    ctx: &CallCtx,
    args: &serde_json::Value,
) -> Option<CmdResult> {
    if !args
        .get("viewId")
        .and_then(|v| v.as_str())
        .is_some_and(|id| id.starts_with("change:"))
    {
        return None;
    }
    let args: ExportViewArgs = match parse_args(args.clone()) {
        Ok(args) => args,
        Err(e) => return Some(Err(e)),
    };
    let app_worker = app.clone();
    let window = ctx.window.clone();
    let op = app.ops().spawn("export-change", true, move |token, _| {
        Box::pin(async move {
            with_active(&app_worker, &window, move |active| {
                let view = active
                    .change_views
                    .get(&args.view_id)
                    .ok_or_else(|| CmdError::new(ErrorCode::NotFound, "err-view-not-found"))?;
                if args.version != view.version {
                    return Err(super::view::stale());
                }
                let mut q =
                    change_query(view, args.columns.as_deref().unwrap_or(&view.columns), 0, 1)?;
                if args.all_columns {
                    q.columns = os_analyze::views::ColumnSelection::All;
                }
                let tab =
                    change::tab_for_query(&active.change_config(), changed(&view.spec), &mut q);
                let format = match args.format {
                    ExportFormatDto::Csv => os_export::Format::Csv,
                    ExportFormatDto::Xlsx => os_export::Format::Xlsx,
                    _ => return Err(CmdError::invalid_input("err-export-format", "format")),
                };
                if token.is_cancelled() {
                    return Err(CmdError::new(ErrorCode::Cancelled, "err-cancelled"));
                }
                let mut export_ctx = os_export::types::ExportCtx {
                    cancel: &token,
                    progress: &|_| {},
                    skip_empty: false,
                    workbook: None,
                    skipped: vec![],
                };
                let info = os_export::pipeline::export_view_with(
                    &[tab],
                    &active.comparison.current().file,
                    &active.comparison.current().config,
                    &q,
                    Path::new(&args.path),
                    format,
                    "Change Detection",
                    false,
                    &mut export_ctx,
                )
                .map_err(|e| internal("export.change", e.to_string()))?
                .ok_or_else(|| internal("export.change", "no output"))?;
                to_json(&FileResult {
                    path: info.path.to_string_lossy().into_owned(),
                    bytes: info.bytes,
                })
            })
            .await
        })
    });
    Some(to_json(&op))
}

/// Zahlenquelle für Overview und Katalog; kein Element aktiv → kein Tab (TECH-300/301).
pub async fn change_counts(
    app: &App,
    ctx: &CallCtx,
    scope: Option<CompareClass>,
) -> Result<Vec<FilterCountRow>, CmdError> {
    if session(app, &ctx.window).is_none() {
        return Ok(vec![]);
    }
    with_active(app, &ctx.window, move |active| {
        if active.elements.is_empty() {
            return Ok(vec![]);
        }
        let tab = active.change_tab(scope != Some(CompareClass::NotChanged));
        let cfg = &active.comparison.current().config;
        let all = change::ids(&active.comparison, "All", true)
            .map_err(compare_error)?
            .len();
        tab.filters
            .iter()
            .map(|f| {
                let mut info = super::catalog::info(tab.id, f, cfg);
                let n = change::ids(
                    &active.comparison,
                    f.id,
                    scope != Some(CompareClass::NotChanged),
                )
                .map_err(compare_error)?
                .len();
                info.count = Some(n);
                let changed_count = change::ids(&active.comparison, f.id, true)
                    .map_err(compare_error)?
                    .len();
                info.comparison = Some(ChangeCountDto {
                    current: changed_count,
                    previous: changed_count,
                    change: changed_count,
                });
                info.pct = (all > 0).then(|| (n as f64 * 1000.0 / all as f64).round() / 10.0);
                if f.id == "Content"
                    && (!change::content_available(&active.comparison, Side::Current)
                        .map_err(compare_error)?
                        || !change::content_available(&active.comparison, Side::Previous)
                            .map_err(compare_error)?)
                {
                    info.state = FilterStateDto::NeedsConfig {
                        key: "content.text or minhash in both crawls".into(),
                    };
                }
                Ok(FilterCountRow {
                    tab: tab.id.into(),
                    filter: info,
                })
            })
            .collect()
    })
    .await
}

/// Die Vergleichssitzung liefert ihre eigenen Lesedateien; ein weiterer offener Crawl ist nicht erforderlich.
pub async fn change_overview(
    app: &App,
    ctx: &CallCtx,
    args: &ScopeArgs,
    overview: bool,
) -> Option<CmdResult> {
    session(app, &ctx.window)?;
    if args.segment.is_some()
        || matches!(
            args.compare,
            Some(
                CompareClass::Added
                    | CompareClass::New
                    | CompareClass::Removed
                    | CompareClass::Missing
            )
        )
    {
        return None;
    }
    let change_rows = match change_counts(app, ctx, args.compare).await {
        Ok(r) => r,
        Err(e) => return Some(Err(e)),
    };
    Some(
        with_active(app, &ctx.window, move |active| {
            let side = active.comparison.current();
            let tabs = os_analyze::tabs::configured_for_file(&side.config, &side.file);
            let counts = os_analyze::views::filter_counts_in(&tabs, &side.file, &side.config)
                .map_err(super::catalog::view_error)?;
            let mut rows = super::catalog::count_rows(&tabs, &side.config, &counts);
            rows.retain(|r| r.tab != "Change Detection");
            rows.extend(change_rows);
            if !overview {
                return to_json(&FilterCounts {
                    version: active.comparison.version(),
                    filters: rows,
                });
            }
            let summary = os_store::queries::app_support::overview_summary(side.file.conn())
                .map_err(|e| store_error(&e))?;
            let mut grouped: Vec<OverviewTab> = tabs
                .iter()
                .filter(|t| t.id != "Change Detection")
                .filter_map(|t| {
                    let filters = rows
                        .iter()
                        .filter(|r| r.tab == t.id)
                        .map(|r| r.filter.clone())
                        .collect::<Vec<_>>();
                    (!filters.is_empty()).then(|| OverviewTab {
                        tab: t.id.into(),
                        filters,
                        partition: None,
                    })
                })
                .collect();
            let filters = rows
                .into_iter()
                .filter(|r| r.tab == "Change Detection")
                .map(|r| r.filter)
                .collect::<Vec<_>>();
            if !filters.is_empty() {
                grouped.push(OverviewTab {
                    tab: "Change Detection".into(),
                    filters,
                    partition: None,
                });
            }
            to_json(&OverviewDto {
                summary: super::sidebar::summary_rows(summary),
                tabs: grouped,
                age_ms: None,
            })
        })
        .await,
    )
}

#[cfg(test)]
#[path = "../../../os-analyze/tests/change_detection_fixture.rs"]
mod change_fixture;

#[cfg(test)]
mod change_tests {
    use super::*;
    use serde_json::json;
    async fn setup() -> (tempfile::TempDir, App) {
        let home = tempfile::tempdir().unwrap();
        let old =
            super::change_fixture::fixture(home.path(), 1, 1, os_model::CrawlConfig::default());
        let new =
            super::change_fixture::fixture(home.path(), 2, 2, os_model::CrawlConfig::default());
        let app = App::new(crate::AppConfig::new(home.path())).unwrap();
        app.dispatch(CallCtx::for_window("main"),"compare.open",json!({"current":new.path(),"previous":old.path(),"mapping":[],"contentThresholdPct":10})).await.unwrap();
        (home, app)
    }

    // spec: TECH-300
    // spec: TECH-301
    #[tokio::test]
    async fn empty_or_missing_ipc_elements_use_current_configuration_defaults() {
        for configured in [
            os_model::ChangeDetectionConfig::default().elements,
            vec![ChangeElement::PageTitles],
            vec![],
        ] {
            let home = tempfile::tempdir().unwrap();
            let old =
                super::change_fixture::fixture(home.path(), 1, 1, os_model::CrawlConfig::default());
            let mut config = os_model::CrawlConfig::default();
            config.compare.change_detection.elements = configured.clone();
            config.compare.change_detection.content_threshold_pct = 23;
            let new = super::change_fixture::fixture(home.path(), 2, 2, config);
            let app = App::new(crate::AppConfig::new(home.path())).unwrap();
            let expected: Vec<_> = configured.iter().map(|e| e.as_str()).collect();
            for elements in [None, Some(json!([])), Some(json!(["h1"]))] {
                let mut args = json!({
                    "current": old.path(), "previous": new.path(), "mapping": []
                });
                if let Some(elements) = &elements {
                    args["elements"] = elements.clone();
                }
                let opened = app
                    .dispatch(CallCtx::for_window("main"), "compare.open", args)
                    .await
                    .unwrap();
                let opened: CompareSession = serde_json::from_value(opened).unwrap();
                let expected = if elements == Some(json!(["h1"])) {
                    vec!["h1"]
                } else {
                    expected.clone()
                };
                assert_eq!(opened.elements, expected);
                assert_eq!(opened.content_threshold_pct, 23);
                let tabs = app
                    .dispatch(CallCtx::for_window("main"), "catalog.tabs", json!(null))
                    .await
                    .unwrap();
                assert_eq!(
                    tabs.as_array()
                        .unwrap()
                        .iter()
                        .any(|tab| tab["id"] == "Change Detection"),
                    !expected.is_empty()
                );
            }
        }
    }
    #[derive(Default)]
    struct Clipboard(Mutex<String>);
    impl crate::runtime::hooks::DesktopActions for Clipboard {
        fn write_clipboard(&self, text: &str) -> Result<(), CmdError> {
            *self.0.lock().unwrap() = text.into();
            Ok(())
        }
        fn reveal_paths(&self, _: &[PathBuf]) -> Result<(), CmdError> {
            Ok(())
        }
    }
    // spec: TECH-301
    // spec: TECH-307
    #[tokio::test]
    async fn change_locate_and_copy_follow_sort_selection_version_and_window() {
        let (_home, app) = setup().await;
        let clipboard = Arc::new(Clipboard::default());
        app.set_desktop_actions(clipboard.clone());
        let call = |name, args| app.dispatch(CallCtx::for_window("main"), name, args);
        let mut spec = json!({"kind":"tab","tab":"Change Detection","filter":"Page Titles","search":null,"sort":[{"column":"Address","dir":"desc"}],"segment":null,"compare":"notChanged"});
        let view = call("view.open", json!({"spec":spec,"columns":["Address"]}))
            .await
            .unwrap();
        let id = &view["viewId"];
        assert_eq!(
            call("view.locate", json!({"viewId":id,"urlId":11}))
                .await
                .unwrap(),
            0
        );
        assert_eq!(
            call("view.locate", json!({"viewId":id,"urlId":1}))
                .await
                .unwrap(),
            json!(null)
        );
        let copied = call("view.copy",json!({"viewId":id,"selection":{"kind":"ids","ids":[2,11,11,1,999]},"columns":["Title 1 (Current)","Address"],"withHeader":true})).await.unwrap();
        assert_eq!(copied["rows"], 2);
        let eol = if cfg!(windows) { "\r\n" } else { "\n" };
        assert_eq!(*clipboard.0.lock().unwrap(),format!("Title 1 (Current)\tAddress{eol}Shop\thttps://site.test/k{eol}Shop\thttps://site.test/b{eol}"));
        let copied = call("view.copy",json!({"viewId":id,"selection":{"kind":"all","viewId":id,"version":view["version"],"except":[11,999]},"columns":["Address"],"withHeader":false})).await.unwrap();
        assert_eq!(copied["rows"], 9);
        let before = clipboard.0.lock().unwrap().clone();
        spec["compare"] = json!("changed");
        call("view.update", json!({"viewId":id,"spec":spec}))
            .await
            .unwrap();
        let err = call("view.copy",json!({"viewId":id,"selection":{"kind":"all","viewId":id,"version":view["version"],"except":[]},"columns":["Address"],"withHeader":false})).await.unwrap_err();
        assert_eq!(err.code, ErrorCode::Stale);
        assert_eq!(*clipboard.0.lock().unwrap(), before);
        assert_eq!(
            call("view.locate", json!({"viewId":id,"urlId":1}))
                .await
                .unwrap(),
            0
        );
        for name in ["view.locate", "view.copy"] {
            let args = if name == "view.locate" {
                json!({"viewId":id,"urlId":1})
            } else {
                json!({"viewId":id,"selection":{"kind":"ids","ids":[1]},"columns":["Address"],"withHeader":false})
            };
            assert_eq!(
                app.dispatch(CallCtx::for_window("other"), name, args)
                    .await
                    .unwrap_err()
                    .code,
                ErrorCode::NotFound
            );
        }
        call("compare.swap", json!({})).await.unwrap();
        assert_eq!(
            call("view.locate", json!({"viewId":id,"urlId":1}))
                .await
                .unwrap_err()
                .code,
            ErrorCode::NotFound
        );
    }
    // spec: TECH-301
    // spec: TECH-307
    #[tokio::test]
    async fn change_view_works_without_another_open_crawl_and_inverts() {
        let (_home, app) = setup().await;
        let call = |name, args| app.dispatch(CallCtx::for_window("main"), name, args);
        let spec = json!({"kind":"tab","tab":"Change Detection","filter":"Page Titles","search":null,"sort":[],"segment":null,"compare":"changed"});
        let opened = call("view.open", json!({"spec":spec,"columns":[]}))
            .await
            .unwrap();
        assert_eq!(opened["total"], 1);
        let id = opened["viewId"].as_str().unwrap();
        let rows=call("view.rows",json!({"viewId":id,"version":1,"offset":0,"limit":500,"columns":["Address","Title 1 (Current)","Title 1 (Previous)"]})).await.unwrap();
        assert_eq!(
            rows["cells"][0],
            json!(["https://site.test/a", "shop", "Shop"])
        );
        let mut spec = spec;
        spec["compare"] = json!("notChanged");
        let updated = call("view.update", json!({"viewId":id,"spec":spec}))
            .await
            .unwrap();
        assert_eq!(updated["total"], 10);
        assert_eq!(updated["version"], 2);
        let counts = call(
            "catalog.filterCounts",
            json!({"segment":null,"compare":null}),
        )
        .await
        .unwrap();
        assert!(counts["filters"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["tab"] == "Change Detection"
                && r["filter"]["id"] == "Page Titles"
                && r["filter"]["count"] == 1));
        let overview = call("sidebar.overview", json!({"segment":null,"compare":null}))
            .await
            .unwrap();
        assert_eq!(
            overview["tabs"].as_array().unwrap().last().unwrap()["tab"],
            "Change Detection"
        );
        let err = app
            .dispatch(
                CallCtx::for_window("other"),
                "view.rows",
                json!({"viewId":id,"version":2,"offset":0,"limit":1,"columns":[]}),
            )
            .await
            .unwrap_err();
        assert_eq!(err.code, ErrorCode::NotFound);
    }
    // spec: TECH-307
    #[tokio::test]
    async fn export_uses_the_inverted_view_and_the_file_result_contract() {
        let (home, app) = setup().await;
        let opened=app.dispatch(CallCtx::for_window("main"),"view.open",json!({"spec":{"kind":"tab","tab":"Change Detection","filter":"Page Titles","search":null,"sort":[],"segment":null,"compare":"notChanged"},"columns":["Address","Title 1 (Current)","Title 1 (Previous)"]})).await.unwrap();
        let path = home.path().join("unchanged.csv");
        let operation=app.dispatch(CallCtx::for_window("main"),"export.view",json!({"viewId":opened["viewId"],"version":opened["version"],"path":path,"format":"csv","allColumns":false})).await.unwrap();
        let id = operation["opId"].as_str().unwrap();
        let result = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                let status = app.ops().status(id).unwrap();
                if status.state != OpState::Running {
                    break status;
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        assert_eq!(result.state, OpState::Done, "{:?}", result.error);
        let file: FileResult = serde_json::from_value(result.result.unwrap()).unwrap();
        assert_eq!(file.path, path.to_string_lossy());
        assert!(file.bytes > 0);
        let csv = std::fs::read_to_string(path).unwrap();
        assert_eq!(csv.lines().count(), 11);
        assert!(!csv.contains("https://site.test/a,"));
    }

    // spec: OUT-152
    #[test]
    fn mixed_empty_and_populated_exports_keep_the_shared_workbook() {
        let dir = tempfile::tempdir().unwrap();
        let file =
            super::change_fixture::fixture(dir.path(), 1, 1, os_model::CrawlConfig::default());
        let cfg = file.config().unwrap();
        let mut workbook = os_export::xlsx::XlsxWriter::create(
            &dir.path().join("mixed.xlsx"),
            "Export",
            &os_export::WriterOptions::default(),
        )
        .unwrap();
        let cancel = os_model::CancelToken::new();
        {
            let mut ctx = os_export::types::ExportCtx {
                cancel: &cancel,
                progress: &|_| {},
                skip_empty: true,
                workbook: Some(&mut workbook),
                skipped: vec![],
            };
            let options = os_export::pipeline::BatchOptions {
                format: os_export::Format::Xlsx,
                directory: dir.path(),
                crawl_name: "mixed",
                single_workbook: false,
            };
            let result = os_export::pipeline::export_batch(
                &file,
                &cfg,
                &[
                    os_export::types::ExportSelection {
                        kind: "tab".into(),
                        name: "Page Titles:Missing".into(),
                    },
                    os_export::types::ExportSelection {
                        kind: "tab".into(),
                        name: "Internal:All".into(),
                    },
                ],
                &options,
                &mut ctx,
            )
            .unwrap();
            assert_eq!(result.skipped.len(), 1);
            assert!(result.files.is_empty());
        }
        assert!(workbook.has_tables());
        let empty = os_export::xlsx::XlsxWriter::create(
            &dir.path().join("empty.xlsx"),
            "Export",
            &os_export::WriterOptions::default(),
        )
        .unwrap();
        assert!(!empty.has_tables());
    }

    // spec: TECH-300
    #[test]
    fn ordinary_crawl_overview_has_no_change_detection_section() {
        let dir = tempfile::tempdir().unwrap();
        let file =
            super::change_fixture::fixture(dir.path(), 1, 1, os_model::CrawlConfig::default());
        let cfg = file.config().unwrap();
        let tabs = os_analyze::tabs::configured_for_file(&cfg, &file);
        let overview = os_export::reports::overview::build(&file, &cfg, &tabs).unwrap();
        assert!(!overview
            .rows
            .iter()
            .any(|row| row.section == "Change Detection"));
    }

    // spec: TECH-300
    #[tokio::test]
    async fn tab_is_hidden_without_comparison_and_when_configuration_has_no_elements() {
        let (_home, app) = setup().await;
        with_active(&app, &WindowId("main".into()), |a| {
            a.elements.clear();
            Ok(())
        })
        .await
        .unwrap();
        let tabs = app
            .dispatch(CallCtx::for_window("main"), "catalog.tabs", json!(null))
            .await
            .unwrap();
        assert!(!tabs
            .as_array()
            .unwrap()
            .iter()
            .any(|t| t["id"] == "Change Detection"));
        app.dispatch(CallCtx::for_window("main"), "compare.close", json!(null))
            .await
            .unwrap();
        let tabs = app
            .dispatch(CallCtx::for_window("main"), "catalog.tabs", json!(null))
            .await
            .unwrap();
        assert!(!tabs
            .as_array()
            .unwrap()
            .iter()
            .any(|t| t["id"] == "Change Detection"));
    }
}
