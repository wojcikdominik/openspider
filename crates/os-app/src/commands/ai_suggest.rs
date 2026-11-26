//! Commands der Optimierungsvorschläge (M4-V01; 08 AI-070 … AI-076), alle hinter dem Schalter „KI (Vorschau)“.
//!
//! * `ai.suggestRun` (Katalog M4-F02, lang laufend): Geltungsbereich und Kostenvoranschlag je Element; die Einträge
//!   warten danach mit `Pending (confirmation)` auf die Bestätigung. Den Dialog und die Bestätigung baut M4-U01, die
//!   Ausführung in der App (Anbieter, Schlüssel, Einwilligung aus dem App-Speicher) verdrahtet M4-N01; der Kern-Weg
//!   (`os_ai::tasks::suggest::Engine`) ist gegen den ai-mock getestet (Platzhalter `pending_m4_v01.rs`).
//! * `ai.suggestPlan`: wie `ai.suggestRun`, ohne etwas zu speichern (Zahlen für den Dialog).
//! * `ai.suggestDetail`: Prüfansicht einer URL (Kandidaten, Prüfwerte, Status, Review).
//! * `ai.suggestReview`: Übernehmen, Bearbeiten, Ablehnen, Zurücksetzen, Massenaktion „Kandidat 1 übernehmen“ (AI-076).
//! * `ai.suggestCheck`: Live-Prüfung eines bearbeiteten Texts inkl. Pixelbreite (AI-076, UI-038 im Kern).
//! * `ai.suggestRecheck`: Neuprüfung aller Kandidaten mit den aktuellen Grenzwerten ohne Request (AI-075).
//! * `ai.suggestFocusImport`: Fokus-Begriffe aus einer CSV `Address,Keyword` (AI-070).
//!
//! Pixelbreiten berechnet ausschließlich `os_parse::pixel::width_px` (UI-038); das Frontend misst nie selbst.
use std::collections::{BTreeMap, BTreeSet};

use futures_util::future::BoxFuture;
use os_ai::catalog::{
    choose_reasoning, AnswerTokens, ModelRef, PriceKey, PriceTable, ProviderLimitsFile,
    TaskDefaults, TaskPlan,
};
use os_ai::cost::{Calibration, NoCache, PriceBasis, Rates};
use os_ai::provider::{CapabilitySource, Model};
use os_ai::tasks::suggest::{
    self as sg, estimate::SuggestEstimate, Checker, Element, ElementLimits, NoStyleRules,
    PageInput, RuleSet, SiteIndex, SuggestOptions,
};
use os_analyze::views::FilterMembers;
use os_model::budget::MicroUsd;
use os_model::CrawlConfig;
use os_store::ai::suggestions as store;
use serde::Deserialize;
use serde_json::{json, Value};

use super::ai_preview;
use super::catalog::session;
use crate::dto::{CmdError, ErrorCode};
use crate::registry::{CmdResult, CommandMeta, Registry};
use crate::runtime::error::{parse_args, store_error};
use crate::{App, CallCtx};

/// Höchstlänge des Seitentext-Auszugs im Kontext (Zeichen, Kürzung an Zeichengrenzen wie INTEG-172).
pub const TEXT_EXCERPT_CHARS: usize = 2_000;

/// Pixelbreite nach UI-038 (einzige Berechnung, `os-parse`).
pub fn ui038(text: &str, size_px: f64) -> u32 {
    os_parse::pixel::width_px(text, size_px, false)
}

fn preview_required(app: &App) -> Result<(), CmdError> {
    if ai_preview::switch(app).is_on() {
        Ok(())
    } else {
        Err(CmdError::new(ErrorCode::Unsupported, "err-ai-preview-off"))
    }
}

fn element_of(id: &str) -> Result<Element, CmdError> {
    Element::from_id(id).ok_or_else(|| {
        CmdError::invalid_input("err-suggest-element", "element")
            .with_arg("element", id.to_string())
    })
}

/// `<html lang="…">` aus dem gespeicherten HTML.
pub fn html_lang(html: &str) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    let start = lower.find("<html")?;
    let end = start + lower[start..].find('>')?;
    let tag = &html[start..end];
    let tag_lower = &lower[start..end];
    let at = tag_lower.find("lang=")? + 5;
    let rest = &tag[at..];
    let value: String = match rest.chars().next()? {
        q @ ('"' | '\'') => rest[1..].chars().take_while(|c| *c != q).collect(),
        _ => rest
            .chars()
            .take_while(|c| !c.is_whitespace() && *c != '>')
            .collect(),
    };
    let value = value.trim().to_string();
    (!value.is_empty()).then_some(value)
}

fn excerpt(text: &str) -> String {
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    collapsed.chars().take(TEXT_EXCERPT_CHARS).collect()
}

fn answer_text(v: &Value) -> Option<String> {
    v.get("choice")
        .or_else(|| v.get("value"))
        .and_then(Value::as_str)
        .or_else(|| v.as_str())
        .map(str::to_string)
}

fn answer_false(v: &Value) -> bool {
    *v == Value::Bool(false)
        || v.get("value") == Some(&Value::Bool(false))
        || v.get("answer") == Some(&Value::Bool(false))
}

/// Seiten, Eingaben und Index eines Crawls (lesend).
pub struct SiteContext {
    pub rows: Vec<store::PageRow>,
    pub inputs: BTreeMap<i64, PageInput>,
    pub index: SiteIndex,
    /// Antworten der Kontextfragen (`store::CONTEXT_QUESTIONS`).
    pub answers: BTreeMap<String, Vec<(i64, Value)>>,
}

impl SiteContext {
    /// Seitensprachen je URL (für die Sprachregel, AI-075 (5)).
    pub fn languages(&self) -> BTreeMap<i64, Option<String>> {
        self.inputs
            .iter()
            .map(|(id, p)| (*id, p.language.clone()))
            .collect()
    }
}

/// Baut Seiten, Seitentext (nur mit gespeichertem HTML), Sprache (`<html lang>`, sonst lokale Erkennung), Seitentyp und
/// Suchintention (AI-050/051, sofern vorhanden) und Fokus-Begriffe aus den gelesenen Daten (`store::site_data`, im Pool).
pub fn context_from(data: store::SiteData) -> SiteContext {
    let by_question = |q: &str| -> BTreeMap<i64, String> {
        data.answers
            .get(q)
            .into_iter()
            .flatten()
            .filter_map(|(id, v)| answer_text(v).map(|t| (*id, t)))
            .collect()
    };
    let page_types = by_question("page-type");
    let intents = by_question("search-intent");
    let mut inputs = BTreeMap::new();
    for row in &data.pages {
        let mut text = None;
        let mut language = None;
        if let Some(stored) = data.html.get(&row.url_id) {
            let content_type = stored
                .charset
                .as_ref()
                .map(|c| format!("text/html; charset={c}"));
            let html = os_parse::charset::decode(&stored.bytes, content_type.as_deref(), &[]).0;
            language = html_lang(&html);
            let visible = excerpt(&crate::source::visible_text(&html));
            text = (!visible.is_empty()).then_some(visible);
        }
        if language.is_none() {
            let sample = [
                text.as_deref(),
                row.title.as_deref(),
                row.h1.as_deref(),
                row.meta_description.as_deref(),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" ");
            language = sg::lang::detect(&sample, sg::lang::MIN_CONFIDENCE);
        }
        inputs.insert(
            row.url_id,
            PageInput {
                url_id: row.url_id,
                address: row.address.clone(),
                title: row.title.clone(),
                meta_description: row.meta_description.clone(),
                h1: row.h1.clone(),
                text,
                language,
                page_type: page_types.get(&row.url_id).cloned(),
                search_intent: intents.get(&row.url_id).cloned(),
                focus: data.focus.get(&row.url_id).cloned(),
            },
        );
    }
    SiteContext {
        index: SiteIndex::from_pages(&data.pages),
        rows: data.pages,
        inputs,
        answers: data.answers,
    }
}

/// Geltungsbereich (AI-070): Problemfilter (Standard), Segment, markierte URLs, alle indexierbaren.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Default)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Scope {
    #[default]
    Default,
    All,
    #[serde(rename_all = "camelCase")]
    Selected {
        url_ids: Vec<i64>,
    },
    #[serde(rename_all = "camelCase")]
    Segment {
        segment_id: i64,
    },
}

pub const TITLE_FILTERS: [&str; 7] = [
    "Missing",
    "Duplicate",
    "Over {title.max_chars} Characters",
    "Below {title.min_chars} Characters",
    "Over {title.max_px} Pixels",
    "Below {title.min_px} Pixels",
    "Same as H1",
];
pub const META_FILTERS: [&str; 6] = [
    "Missing",
    "Duplicate",
    "Over {meta_description.max_chars} Characters",
    "Below {meta_description.min_chars} Characters",
    "Over {meta_description.max_px} Pixels",
    "Below {meta_description.min_px} Pixels",
];

/// Mitgliedschaften der Filter eines Tabs unter den URL-IDs (im Pool: `os_analyze::views::filter_members_in`).
pub type MembersFn<'a> =
    &'a dyn Fn(os_analyze::defs::TabDef, &[i64]) -> Result<Vec<FilterMembers>, CmdError>;
/// URLs eines Segments (im Pool: `store::segment_url_ids`).
pub type SegmentFn<'a> = &'a dyn Fn(i64) -> Result<Vec<i64>, CmdError>;

/// URL-IDs des Geltungsbereichs, jede höchstens einmal, aufsteigend; immer innerhalb des Standard-Geltungsbereichs S
/// (interne HTML-200-URLs, indexierbar; 08 Konventionen).
pub fn scope_ids(
    ctx: &SiteContext,
    element: Element,
    scope: &Scope,
    members: MembersFn<'_>,
    segment: SegmentFn<'_>,
) -> Result<Vec<i64>, CmdError> {
    let s: Vec<i64> = ctx
        .rows
        .iter()
        .filter(|r| r.in_standard_scope)
        .map(|r| r.url_id)
        .collect();
    let set: BTreeSet<i64> = match scope {
        Scope::All => s.iter().copied().collect(),
        Scope::Selected { url_ids } => url_ids
            .iter()
            .copied()
            .filter(|id| s.contains(id))
            .collect(),
        Scope::Segment { segment_id } => segment(*segment_id)?
            .into_iter()
            .filter(|id| s.contains(id))
            .collect(),
        Scope::Default => {
            let (tab, wanted): (_, &[&str]) = match element {
                Element::Title => (os_analyze::tabs::page_titles::tab(), &TITLE_FILTERS),
                Element::MetaDescription => {
                    (os_analyze::tabs::meta_description::tab(), &META_FILTERS)
                }
            };
            let mut set: BTreeSet<i64> = members(tab, &s)?
                .into_iter()
                .filter(|m| wanted.contains(&m.filter.as_str()))
                .flat_map(|m| m.ids)
                .collect();
            // `Title Not Matching Content (AI)` bzw. `Meta Description Not Matching Content (AI)` (AI-054/AI-055).
            for (id, v) in ctx
                .answers
                .get(element.mismatch_question())
                .into_iter()
                .flatten()
            {
                if answer_false(v) && s.contains(id) {
                    set.insert(*id);
                }
            }
            set
        }
    };
    Ok(set.into_iter().collect())
}

/// Prüfer mit den aktuellen Grenzwerten der Konfiguration (Preferences) und der Funktion aus UI-038.
pub fn checker<'a>(element: Element, cfg: &CrawlConfig, index: &'a SiteIndex) -> Checker<'a> {
    Checker {
        element,
        limits: ElementLimits::from_config(element, cfg),
        rules: RuleSet::default(),
        meter: &ui038,
        index,
        style: &NoStyleRules,
    }
}

/// Modell und Preise einer Aufgabe ohne Live-Modellliste (Standardmodell bzw. Wahl im Profil aus dem Katalog,
/// AI-010/AI-011): Grundlage des Voranschlags.
pub fn catalog_plan(element: Element, cfg: &CrawlConfig) -> Option<(TaskPlan, Rates, u64)> {
    let task = element.task();
    let defaults = TaskDefaults::bundled().ok()?;
    let table = PriceTable::bundled().ok()?;
    let model_ref = match cfg
        .ai
        .tasks
        .get(task.as_str())
        .and_then(|t| t.model.as_ref())
    {
        Some(choice) => ModelRef::try_from(choice).ok()?,
        None => defaults.get(task).model.clone()?,
    };
    let key = PriceKey::new(&model_ref.provider, &model_ref.model, model_ref.endpoint);
    let entry = table.get(&key)?;
    let levels = entry.reasoning_levels();
    let model = Model {
        provider: model_ref.provider.clone(),
        id: model_ref.model.clone(),
        endpoint: model_ref.endpoint.as_str().to_string(),
        capabilities: entry.capabilities.map(Into::into).unwrap_or_default(),
        capability_source: CapabilitySource::ModelCatalog,
        reasoning_levels: levels.clone(),
    };
    let user_max_tokens = cfg.ai.tasks.get(task.as_str()).and_then(|t| t.max_tokens);
    let plan = TaskPlan {
        task,
        model_ref: model_ref.clone(),
        model,
        candidate_index: 0,
        reasoning: choose_reasoning(defaults.reasoning(task), &levels),
        answer_tokens: defaults
            .answer_tokens(task)
            .or(Some(AnswerTokens::Fixed(512))),
        model_output_limit: table.max_output_tokens(&key),
        user_max_tokens,
    };
    let rates = entry.rates(None).ok()?;
    let overhead = ProviderLimitsFile::bundled()
        .map(|l| l.schema_overhead(&model_ref.provider, model_ref.endpoint, &model_ref.model))
        .unwrap_or(0);
    Some((plan, rates, overhead))
}

fn has_key(app: &App, provider: &str) -> bool {
    os_ai::keys::AiKeyProvider::from_id(provider)
        .is_some_and(|p| os_ai::keys::AiKeyStore::new(app.config().secrets.clone()).has_key(p))
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct PlanArgs {
    /// Elementkennungen (`title`, `meta_description`); leer = die im Profil aktiven, sonst beide.
    elements: Vec<String>,
    /// Aufgaben wie in `AiRunArgs.tasks` (`suggest-title`, `suggest-meta`).
    tasks: Vec<String>,
    scope: Scope,
    candidates: Option<u8>,
    repair_rounds: Option<u8>,
    sample: Option<u32>,
}

fn elements_of(args: &PlanArgs, cfg: &CrawlConfig) -> Result<Vec<Element>, CmdError> {
    let mut out = Vec::new();
    for id in &args.elements {
        out.push(element_of(id)?);
    }
    for task in &args.tasks {
        match Element::ALL.into_iter().find(|e| e.task().as_str() == task) {
            Some(e) => out.push(e),
            None if task.starts_with("suggest-") => {
                return Err(CmdError::invalid_input("err-suggest-element", "tasks")
                    .with_arg("element", task.clone()))
            }
            None => {}
        }
    }
    if out.is_empty() {
        if cfg.ai.suggestions.title {
            out.push(Element::Title);
        }
        if cfg.ai.suggestions.meta {
            out.push(Element::MetaDescription);
        }
    }
    if out.is_empty() {
        out = Element::ALL.to_vec();
    }
    out.sort();
    out.dedup();
    Ok(out)
}

/// Geltungsbereiche je Element.
type Scopes = Vec<(Element, Vec<i64>)>;

/// Geltungsbereich und Voranschlag je Element.
fn plan(
    ctx: &SiteContext,
    cfg: &CrawlConfig,
    args: &PlanArgs,
    key_for: &dyn Fn(&str) -> bool,
    threshold: MicroUsd,
    members: MembersFn<'_>,
    segment: SegmentFn<'_>,
) -> Result<(Value, Scopes), CmdError> {
    let options = SuggestOptions {
        candidates: args.candidates.unwrap_or(3),
        repair_rounds: args.repair_rounds.unwrap_or(2),
        ..SuggestOptions::default()
    }
    .validated()
    .map_err(|e| CmdError::invalid_input("err-suggest-options", "candidates").with_detail(e.0))?;
    let redactor = os_ai::policy::Redactor::new(os_ai::policy::RedactionSettings::default());
    let mut elements = Vec::new();
    let mut scopes = Vec::new();
    let mut total = 0u64;
    for element in elements_of(args, cfg)? {
        let mut ids = scope_ids(ctx, element, &args.scope, members, segment)?;
        if let Some(n) = args.sample {
            ids.truncate(n as usize);
        }
        let pages: Vec<PageInput> = ids
            .iter()
            .filter_map(|id| ctx.inputs.get(id).cloned())
            .collect();
        let estimate = catalog_plan(element, cfg).map(|(plan, rates, overhead)| {
            let e = sg::estimate::estimate(
                &SuggestEstimate {
                    element,
                    pages: &pages,
                    limits: ElementLimits::from_config(element, cfg),
                    options,
                    plan: &plan,
                    basis: Some(PriceBasis::Rates(&rates)),
                    api_key_present: key_for(&plan.model.provider),
                    schema_overhead_tokens: overhead,
                    calibration: Calibration::UNCALIBRATED,
                    output_calibration: Calibration::UNCALIBRATED,
                    redactor: &redactor,
                    style: &NoStyleRules,
                },
                &NoCache,
            );
            total = total.saturating_add(e.estimate.expected.map_or(0, |m| m.0));
            json!({
                "task": e.estimate.task,
                "model": e.estimate.model,
                "provider": plan.model.provider,
                "reasoning": e.reasoning.map(|r| r.as_str()),
                "items": e.estimate.items,
                "requests": e.requests,
                "cached": e.estimate.cached,
                "estInputTokens": e.estimate.est_input_tokens,
                "estOutputTokens": e.estimate.est_output_tokens,
                "lowMicroUsd": e.estimate.low.map(|m| m.0),
                "expectedMicroUsd": e.estimate.expected.map(|m| m.0),
                "highMicroUsd": e.estimate.high.map(|m| m.0),
                "executable": e.estimate.executable.reason(),
                "calibrated": e.estimate.calibrated,
            })
        });
        elements.push(json!({
            "element": element.id(),
            "task": element.task().as_str(),
            "entries": ids.len(),
            "urlIds": ids,
            "estimate": estimate,
        }));
        scopes.push((element, ids));
    }
    let confirmation = os_ai::cost::confirmation_required(MicroUsd(total), threshold, None);
    Ok((
        json!({
            "elements": elements,
            "expectedMicroUsd": total,
            "confirmationRequired": confirmation,
            "candidates": options.candidates,
            "repairRounds": options.repair_rounds,
        }),
        scopes,
    ))
}

fn plan_cmd<'a>(app: &'a App, ctx: CallCtx, args: Value, run: bool) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move {
        preview_required(app)?;
        let s = session(app, &ctx)?;
        let args: PlanArgs = if args.is_null() {
            PlanArgs::default()
        } else {
            parse_args(args)?
        };
        let cfg = s.config.clone();
        let app2 = app.clone();
        let threshold = app.settings_snapshot().ai_app.confirmation_threshold;
        let (plan, scopes) = app
            .read_pool()
            .run_snapshot(s.key.clone(), move |file, _| {
                let ctx = context_from(store::site_data(file, cfg.extraction.store_html)?);
                Ok(plan(
                    &ctx,
                    &cfg,
                    &args,
                    &|p| has_key(&app2, p),
                    threshold,
                    &|tab, ids| {
                        os_analyze::views::filter_members_in(&[tab], file, &cfg, ids)
                            .map_err(super::catalog::view_error)
                    },
                    &|segment| {
                        store::segment_url_ids(file.conn(), segment).map_err(|e| store_error(&e))
                    },
                ))
            })
            .await??;
        if run {
            if !s.writable {
                return Err(CmdError::new(ErrorCode::Permission, "err-db-read-only"));
            }
            // AI-020: Die Einträge warten auf die Kostenbestätigung (Dialog M4-U01; Ausführung M4-N01).
            app.write_queue()
                .submit(s.key.clone(), move |conn| {
                    for (element, ids) in &scopes {
                        store::mark_pending(conn, element.id(), ids, None)?;
                        let status = os_model::ai::AiStatus::PendingConfirmation.to_string();
                        let mut records = Vec::new();
                        for id in ids {
                            if let Some(mut r) = store::load(conn, *id, element.id())? {
                                r.status = status.clone();
                                records.push(r);
                            }
                        }
                        store::save_all(conn, &records)?;
                    }
                    Ok(())
                })
                .await?;
            let mut plan = plan;
            plan["state"] = json!("pendingConfirmation");
            return Ok(plan);
        }
        Ok(plan)
    })
}

/// `ai.suggestRun` (Katalog M4-F02, lang laufend).
pub fn suggest_run<'a>(app: &'a App, ctx: CallCtx, args: Value) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move {
        preview_required(app)?;
        let owned = app.clone();
        let handle = app.ops().spawn("op-ai-suggest", true, move |_, _| {
            Box::pin(async move { plan_cmd(&owned, ctx, args, true).await })
        });
        crate::runtime::error::to_json(&handle)
    })
}

fn suggest_plan<'a>(app: &'a App, ctx: CallCtx, args: Value) -> BoxFuture<'a, CmdResult> {
    plan_cmd(app, ctx, args, false)
}

/// Prüfansicht eines Eintrags.
fn element_json(
    element: Element,
    page: Option<&PageInput>,
    r: Option<&store::ElementRecord>,
    current_px: Option<u32>,
) -> Value {
    let current = page.and_then(|p| element.current(p)).map(str::to_string);
    let Some(r) = r else {
        return json!({
            "element": element.id(),
            "label": element.label(),
            "current": current,
            "currentPixelWidth": current_px,
            "status": null,
            "candidates": [],
        });
    };
    let candidates: Vec<Value> = r
        .displayed()
        .into_iter()
        .map(|c| {
            json!({
                "index": c.index,
                "value": c.value,
                "length": c.length,
                "pixelWidth": c.pixel_width,
                "valid": c.valid,
                "violations": c.violations,
                "round": c.round,
                "selected": r.selected == Some(c.index),
            })
        })
        .collect();
    json!({
        "element": element.id(),
        "label": element.label(),
        "current": current,
        "currentPixelWidth": current_px,
        "status": r.status,
        "provisional": r.provisional,
        "violations": r.violations,
        "review": r.review.label(),
        "accepted": r.accepted.as_ref().map(|a| json!({
            "value": a.value,
            "length": a.length,
            "pixelWidth": a.pixel_width,
            "violations": a.violations,
            "origin": a.origin.as_str(),
            "candidate": a.candidate,
            "at": a.at,
        })),
        "model": r.model,
        "generatedAt": r.generated_at,
        "focus": r.focus,
        "rounds": r.rounds,
        "candidates": candidates,
    })
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DetailArgs {
    url_id: i64,
}

fn suggest_detail<'a>(app: &'a App, ctx: CallCtx, args: Value) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move {
        preview_required(app)?;
        let s = session(app, &ctx)?;
        let a: DetailArgs = parse_args(args)?;
        let cfg = s.config.clone();
        app.read_pool()
            .run_snapshot(s.key.clone(), move |file, _| {
                let conn = file.conn();
                let rows = store::pages(conn)?;
                let row = rows.iter().find(|r| r.url_id == a.url_id);
                let page = row.map(|r| PageInput {
                    url_id: r.url_id,
                    address: r.address.clone(),
                    title: r.title.clone(),
                    meta_description: r.meta_description.clone(),
                    h1: r.h1.clone(),
                    ..PageInput::default()
                });
                let mut elements = Vec::new();
                for element in Element::ALL {
                    let record = store::load(conn, a.url_id, element.id())?;
                    let px = row.and_then(|r| match element {
                        Element::Title => r.title_px,
                        Element::MetaDescription => r.meta_description_px,
                    });
                    elements.push(element_json(element, page.as_ref(), record.as_ref(), px));
                }
                let limits: Vec<Value> = Element::ALL
                    .into_iter()
                    .map(|e| json!({"element": e.id(), "limits": ElementLimits::from_config(e, &cfg)}))
                    .collect();
                Ok(json!({
                    "urlId": a.url_id,
                    "address": row.map(|r| r.address.clone()),
                    "elements": elements,
                    "limits": limits,
                }))
            })
            .await
    })
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReviewArgs {
    url_ids: Vec<i64>,
    element: String,
    /// `accept`, `edit`, `reject`, `reset`, `acceptFirst` (Massenaktion).
    action: String,
    #[serde(default)]
    candidate: Option<u32>,
    #[serde(default)]
    value: Option<String>,
}

fn suggest_review<'a>(app: &'a App, ctx: CallCtx, args: Value) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move {
        preview_required(app)?;
        let s = session(app, &ctx)?;
        if !s.writable {
            return Err(CmdError::new(ErrorCode::Permission, "err-db-read-only"));
        }
        let a: ReviewArgs = parse_args(args)?;
        let element = element_of(&a.element)?;
        let action = match a.action.as_str() {
            "accept" => Some(sg::ReviewAction::Accept {
                candidate: a.candidate,
            }),
            "edit" => Some(sg::ReviewAction::Edit {
                value: a
                    .value
                    .clone()
                    .ok_or_else(|| CmdError::invalid_input("err-suggest-value", "value"))?,
            }),
            "reject" => Some(sg::ReviewAction::Reject),
            "reset" => Some(sg::ReviewAction::Reset),
            "acceptFirst" => None,
            other => {
                return Err(CmdError::invalid_input("err-suggest-action", "action")
                    .with_arg("action", other.to_string()))
            }
        };
        let cfg = s.config.clone();
        let now = app.config().clock.now().to_db();
        let ctx_cfg = cfg.clone();
        let context = app
            .read_pool()
            .run_snapshot(s.key.clone(), move |file, _| {
                Ok(context_from(store::site_data(
                    file,
                    ctx_cfg.extraction.store_html,
                )?))
            })
            .await?;
        let result = app
            .write_queue()
            .submit(s.key.clone(), move |conn| {
                let mut records = Vec::new();
                for id in &a.url_ids {
                    if let Some(r) = store::load(conn, *id, element.id())? {
                        records.push(r);
                    }
                }
                let languages = context.languages();
                let check = checker(element, &cfg, &context.index);
                let mut errors = Vec::new();
                let bulk = match &action {
                    None => Some(sg::accept_first(&mut records, &now)),
                    Some(action) => {
                        for r in &mut records {
                            let lang = languages.get(&r.url_id).cloned().flatten();
                            if let Err(e) = sg::apply(r, action, &check, lang.as_deref(), &now) {
                                errors.push(json!({"urlId": r.url_id, "error": e.to_string()}));
                            }
                        }
                        None
                    }
                };
                store::save_all(conn, &records)?;
                Ok(json!({
                    "updated": records.len() - errors.len(),
                    "accepted": bulk.as_ref().map(|b| b.accepted),
                    "skipped": bulk.as_ref().map(|b| b.skipped),
                    "skippedUrlIds": bulk.as_ref().map(|b| b.skipped_url_ids.clone()),
                    "errors": errors,
                }))
            })
            .await?;
        Ok(result)
    })
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CheckArgs {
    url_id: i64,
    element: String,
    value: String,
}

fn suggest_check<'a>(app: &'a App, ctx: CallCtx, args: Value) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move {
        preview_required(app)?;
        let s = session(app, &ctx)?;
        let a: CheckArgs = parse_args(args)?;
        let element = element_of(&a.element)?;
        if a.value.len() > 10_000 {
            return Err(CmdError::invalid_input("err-suggest-value", "value"));
        }
        let cfg = s.config.clone();
        app.read_pool()
            .run_snapshot(s.key.clone(), move |file, _| {
                let context = context_from(store::site_data(file, cfg.extraction.store_html)?);
                let lang = context
                    .inputs
                    .get(&a.url_id)
                    .and_then(|p| p.language.clone());
                let c = sg::check_text(
                    &checker(element, &cfg, &context.index),
                    a.url_id,
                    lang.as_deref(),
                    &a.value,
                );
                Ok(json!({
                    "value": c.value,
                    "length": c.length,
                    "pixelWidth": c.pixel_width,
                    "violations": c.violations,
                    "valid": c.valid(),
                }))
            })
            .await
    })
}

fn suggest_recheck<'a>(app: &'a App, ctx: CallCtx, _args: Value) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move {
        preview_required(app)?;
        let s = session(app, &ctx)?;
        if !s.writable {
            return Err(CmdError::new(ErrorCode::Permission, "err-db-read-only"));
        }
        let cfg = s.config.clone();
        let ctx_cfg = cfg.clone();
        let context = app
            .read_pool()
            .run_snapshot(s.key.clone(), move |file, _| {
                Ok(context_from(store::site_data(
                    file,
                    ctx_cfg.extraction.store_html,
                )?))
            })
            .await?;
        app.write_queue()
            .submit(s.key.clone(), move |conn| {
                let mut updated = 0usize;
                for element in Element::ALL {
                    let records = store::load_all(conn, element.id())?;
                    updated += records.len();
                    let check = checker(element, &cfg, &context.index);
                    let rechecked =
                        sg::revalidate(records, &check, &context.languages(), &context.index);
                    store::save_all(conn, &rechecked)?;
                }
                Ok(json!({ "updated": updated }))
            })
            .await
    })
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FocusArgs {
    csv: String,
}

/// Liest `Address,Keyword` (Kopfzeile optional, Trenner Komma oder Semikolon, Anführungszeichen erlaubt).
pub fn parse_focus_csv(text: &str) -> Vec<(String, String)> {
    let unquote = |s: &str| {
        s.trim()
            .trim_matches('"')
            .replace("\"\"", "\"")
            .trim()
            .to_string()
    };
    text.trim_start_matches('\u{feff}')
        .lines()
        .filter_map(|line| {
            let sep = if line.contains(',') { ',' } else { ';' };
            let (address, keyword) = line.split_once(sep)?;
            let (address, keyword) = (unquote(address), unquote(keyword));
            (!address.is_empty() && !keyword.is_empty() && !address.eq_ignore_ascii_case("address"))
                .then_some((address, keyword))
        })
        .collect()
}

fn suggest_focus_import<'a>(app: &'a App, ctx: CallCtx, args: Value) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move {
        preview_required(app)?;
        let s = session(app, &ctx)?;
        if !s.writable {
            return Err(CmdError::new(ErrorCode::Permission, "err-db-read-only"));
        }
        let a: FocusArgs = parse_args(args)?;
        if a.csv.len() > 10_000_000 {
            return Err(CmdError::invalid_input("err-suggest-focus", "csv"));
        }
        let entries = parse_focus_csv(&a.csv);
        app.write_queue()
            .submit(s.key.clone(), move |conn| {
                let mut known = Vec::new();
                let mut unknown = Vec::new();
                for (address, keyword) in entries {
                    match store::url_id_of(conn, &address)? {
                        Some(id) => known.push((id, keyword)),
                        None => unknown.push(address),
                    }
                }
                store::replace_focus(conn, &known)?;
                Ok(json!({ "imported": known.len(), "unknown": unknown }))
            })
            .await
    })
}

const COMMANDS: &[(&str, &str, &str)] = &[
    ("ai.suggestPlan", "JsonValue", "JsonValue"),
    ("ai.suggestDetail", "JsonValue", "JsonValue"),
    ("ai.suggestReview", "JsonValue", "JsonValue"),
    ("ai.suggestCheck", "JsonValue", "JsonValue"),
    ("ai.suggestRecheck", "void", "JsonValue"),
    ("ai.suggestFocusImport", "JsonValue", "JsonValue"),
];

/// Trägt die Commands ein (`ai.suggestRun` steht im KI-Katalog von M4-F02).
pub fn register(r: &mut Registry) {
    for (name, args, ret) in COMMANDS {
        let handler: crate::registry::Handler = match *name {
            "ai.suggestPlan" => suggest_plan,
            "ai.suggestDetail" => suggest_detail,
            "ai.suggestReview" => suggest_review,
            "ai.suggestCheck" => suggest_check,
            "ai.suggestRecheck" => suggest_recheck,
            _ => suggest_focus_import,
        };
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
