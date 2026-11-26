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
use os_model::budget::MicroUsd;
use os_model::{CrawlConfig, UrlId};
use os_store::ai::suggestions as store;
use os_store::{CrawlFile, StoreError};
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
        CmdError::invalid_input("err-suggest-element", "element").with_arg("element", id.to_string())
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

/// Lädt Seiten, Seitentext (nur mit gespeichertem HTML), Sprache (`<html lang>`, sonst lokale Erkennung), Seitentyp und
/// Suchintention (AI-050/051, sofern vorhanden) und Fokus-Begriffe.
pub fn load_context(file: &CrawlFile, cfg: &CrawlConfig) -> Result<SiteContext, StoreError> {
    let conn = file.conn();
    let rows = store::pages(conn)?;
    let focus = store::focus_map(conn)?;
    let page_types: BTreeMap<i64, String> = store::answers(conn, "page-type")?
        .into_iter()
        .filter_map(|(id, v)| answer_text(&v).map(|t| (id, t)))
        .collect();
    let intents: BTreeMap<i64, String> = store::answers(conn, "search-intent")?
        .into_iter()
        .filter_map(|(id, v)| answer_text(&v).map(|t| (id, t)))
        .collect();
    let mut inputs = BTreeMap::new();
    for row in &rows {
        let mut text = None;
        let mut language = None;
        if cfg.extraction.store_html {
            if let Some(stored) = os_store::queries::stored_html(file, UrlId(row.url_id))? {
                let content_type = stored.charset.map(|c| format!("text/html; charset={c}"));
                let html = os_parse::charset::decode(&stored.bytes, content_type.as_deref(), &[]).0;
                language = html_lang(&html);
                let visible = excerpt(&crate::source::visible_text(&html));
                text = (!visible.is_empty()).then_some(visible);
            }
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
                focus: focus.get(&row.url_id).cloned(),
            },
        );
    }
    Ok(SiteContext {
        index: SiteIndex::from_pages(&rows),
        rows,
        inputs,
    })
}

/// Geltungsbereich (AI-070): Problemfilter (Standard), Segment, markierte URLs, alle indexierbaren.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Default)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Scope {
    #[default]
    Default,
    All,
    #[serde(rename_all = "camelCase")]
    Selected { url_ids: Vec<i64> },
    #[serde(rename_all = "camelCase")]
    Segment { segment_id: i64 },
}

const TITLE_FILTERS: [&str; 7] = [
    "Missing",
    "Duplicate",
    "Over {title.max_chars} Characters",
    "Below {title.min_chars} Characters",
    "Over {title.max_px} Pixels",
    "Below {title.min_px} Pixels",
    "Same as H1",
];
const META_FILTERS: [&str; 6] = [
    "Missing",
    "Duplicate",
    "Over {meta_description.max_chars} Characters",
    "Below {meta_description.min_chars} Characters",
    "Over {meta_description.max_px} Pixels",
    "Below {meta_description.min_px} Pixels",
];

/// URL-IDs des Geltungsbereichs, jede höchstens einmal, aufsteigend; immer innerhalb des Standard-Geltungsbereichs S
/// (interne HTML-200-URLs, indexierbar; 08 Konventionen).
pub fn scope_ids(
    file: &CrawlFile,
    cfg: &CrawlConfig,
    ctx: &SiteContext,
    element: Element,
    scope: &Scope,
) -> Result<Vec<i64>, CmdError> {
    let s: Vec<i64> = ctx
        .rows
        .iter()
        .filter(|r| r.in_standard_scope)
        .map(|r| r.url_id)
        .collect();
    let set: BTreeSet<i64> = match scope {
        Scope::All => s.iter().copied().collect(),
        Scope::Selected { url_ids } => url_ids.iter().copied().filter(|id| s.contains(id)).collect(),
        Scope::Segment { segment_id } => {
            let mut stmt = file
                .conn()
                .prepare("SELECT url_id FROM url_segments WHERE segment_id = ?1")
                .map_err(|e| store_error(&StoreError::from(e)))?;
            let ids: Vec<i64> = stmt
                .query_map([segment_id], |r| r.get(0))
                .and_then(|rows| rows.collect())
                .map_err(|e| store_error(&StoreError::from(e)))?;
            ids.into_iter().filter(|id| s.contains(id)).collect()
        }
        Scope::Default => {
            let (tab, wanted): (_, &[&str]) = match element {
                Element::Title => (os_analyze::tabs::page_titles::tab(), &TITLE_FILTERS),
                Element::MetaDescription => {
                    (os_analyze::tabs::meta_description::tab(), &META_FILTERS)
                }
            };
            let members = os_analyze::views::filter_members_in(&[tab], file, cfg, &s)
                .map_err(super::catalog::view_error)?;
            let mut set: BTreeSet<i64> = members
                .into_iter()
                .filter(|m| wanted.contains(&m.filter.as_str()))
                .flat_map(|m| m.ids)
                .collect();
            // `Title Not Matching Content (AI)` bzw. `Meta Description Not Matching Content (AI)` (AI-054/AI-055).
            for (id, v) in store::answers(file.conn(), element.mismatch_question())
                .map_err(|e| store_error(&e))?
            {
                if answer_false(&v) && s.contains(&id) {
                    set.insert(id);
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
        answer_tokens: defaults.answer_tokens(task).or(Some(AnswerTokens::Fixed(512))),
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
    os_ai::keys::AiKeyProvider::from_id(provider).is_some_and(|p| {
        os_ai::keys::AiKeyStore::new(app.config().secrets.clone()).has_key(p)
    })
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

/// Geltungsbereich und Voranschlag je Element.
fn plan(
    file: &CrawlFile,
    cfg: &CrawlConfig,
    args: &PlanArgs,
    key_for: &dyn Fn(&str) -> bool,
    threshold: MicroUsd,
) -> Result<(Value, Vec<(Element, Vec<i64>)>), CmdError> {
    let ctx = load_context(file, cfg).map_err(|e| store_error(&e))?;
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
        let mut ids = scope_ids(file, cfg, &ctx, element, &args.scope)?;
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
    let confirmation =
        os_ai::cost::confirmation_required(MicroUsd(total), threshold, None);
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
                Ok(plan(file, &cfg, &args, &|p| has_key(&app2, p), threshold))
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
                value: a.value.clone().ok_or_else(|| CmdError::invalid_input("err-suggest-value", "value"))?,
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
            .run_snapshot(s.key.clone(), move |file, _| load_context(file, &ctx_cfg))
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
                let context = load_context(file, &cfg)?;
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
            .run_snapshot(s.key.clone(), move |file, _| load_context(file, &ctx_cfg))
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
    let unquote = |s: &str| s.trim().trim_matches('"').replace("\"\"", "\"").trim().to_string();
    text.trim_start_matches('\u{feff}')
        .lines()
        .filter_map(|line| {
            let sep = if line.contains(',') { ',' } else { ';' };
            let (address, keyword) = line.split_once(sep)?;
            let (address, keyword) = (unquote(address), unquote(keyword));
            (!address.is_empty()
                && !keyword.is_empty()
                && !address.eq_ignore_ascii_case("address"))
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


/// Fixture der Vorschläge (`tests/fixtures/ai/suggest/`): Crawl-Datei mit den Seiten aus `pages.json` (Pixelbreiten nach
/// UI-038) und Vorschlägen aus einem Lauf über den Kern mit den Antworten aus `mock-answers.d/v01.json`. Dient den
/// Tests unten und als Crawl-Datei der E2E-Tests (`tests/fixtures/ai/suggest/e2e/suggest.ospider`, neu erzeugt mit
/// `UPDATE_GOLDEN=1`).
#[cfg(test)]
pub(crate) mod fixture {
    use super::*;
    use os_ai::adapters::openai_responses::RESPONSES_CAPABILITIES;
    use os_ai::cache::CacheOptions;
    use os_ai::limits::{Limiter, LimitsConfig, TokioClock};
    use os_ai::policy::{ConsentBook, ConsentGate, ConsentKey, RedactionSettings, Redactor};
    use os_ai::provider::{AiProvider, Input, ProviderFuture, Request, Response, Usage};
    use os_ai::tasks::suggest::{ElementJob, Engine, FileStore, RunControl};
    use os_ai::PreviewSwitch;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    pub const BASE: &str = "http://ai.test:8080";

    pub fn root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/ai")
    }

    pub fn pages() -> Vec<Value> {
        let doc: Value = serde_json::from_str(
            &std::fs::read_to_string(root().join("suggest/pages.json")).unwrap(),
        )
        .unwrap();
        doc["pages"].as_array().unwrap().clone()
    }

    fn lit(v: &Value) -> String {
        match v {
            Value::Null => "NULL".into(),
            Value::Number(n) => n.to_string(),
            Value::String(s) => format!("'{}'", s.replace('\'', "''")),
            other => format!("'{}'", other.to_string().replace('\'', "''")),
        }
    }

    /// Leere Crawl-Datei mit den Fixture-Seiten (URL-IDs 1 … n in Fixture-Reihenfolge).
    pub fn crawl(path: &Path) -> CrawlFile {
        use os_model::time::{IdSource, SeededIds};
        let created = os_model::Timestamp::from_db("2026-11-26T08:00:00.000Z").unwrap();
        let mut cfg = CrawlConfig::default();
        cfg.ai.enabled = true;
        cfg.ai.suggestions.title = true;
        cfg.ai.suggestions.meta = true;
        let f = CrawlFile::create(
            path,
            &os_store::CrawlInit {
                crawl_id: SeededIds::new(71).new_crawl_id(created),
                mode: os_model::CrawlMode::Spider,
                start_url: Some(format!("{BASE}/")),
                list_source: None,
                project: "V01".into(),
                name: "ai-suggestions".into(),
                config: cfg,
                created_at: created,
            },
            os_store::StorageProfile::Balanced,
        )
        .unwrap();
        for (i, p) in pages().iter().enumerate() {
            let address = format!("{BASE}{}", p["path"].as_str().unwrap());
            let text = |k: &str| p[k].as_str().map(|s| s.to_string());
            let len = |v: &Option<String>| v.as_ref().map(|s| json!(s.chars().count()));
            let px = |v: &Option<String>, size| v.as_ref().map(|s| json!(ui038(s, size)));
            let (title, meta, h1) = (text("title"), text("meta"), text("h1"));
            let indexable = p["indexable"].as_bool().unwrap_or(true);
            let values = [
                json!(i + 1),
                json!(address),
                json!(address.len()),
                json!(if indexable { "Indexable" } else { "Non-Indexable" }),
                json!(if indexable { Value::Null } else { json!("noindex") }),
                json!(title.is_some() as i64),
                json!(title),
                len(&title).unwrap_or(Value::Null),
                px(&title, 20.0).unwrap_or(Value::Null),
                json!(meta.is_some() as i64),
                json!(meta),
                len(&meta).unwrap_or(Value::Null),
                px(&meta, 14.0).unwrap_or(Value::Null),
                json!(h1.is_some() as i64),
                json!(h1),
                len(&h1).unwrap_or(Value::Null),
            ];
            let v: Vec<String> = values.iter().map(lit).collect();
            f.conn()
                .execute_batch(&format!(
                    "INSERT INTO urls(id,url,address,origin,is_internal,state,discovered_via,folder_depth,url_length,
                       content_class,content_type,status_code,status,indexability,indexability_status,title_count,title_1,
                       title_1_len,title_1_px,meta_desc_count,meta_desc_1,meta_desc_1_len,meta_desc_1_px,h1_count,h1_1,h1_1_len)
                     VALUES({id},{a},{a},'{BASE}',1,'Done','Seed',1,{l},'HTML','text/html; charset=utf-8',200,'OK',{rest});",
                    id = v[0],
                    a = v[1],
                    l = v[2],
                    rest = v[3..].join(","),
                ))
                .unwrap();
        }
        f.set_meta("state", "Completed").unwrap();
        f.set_meta("end_reason", "queue_empty").unwrap();
        f
    }

    /// Antworten je MOCK-ID aus `mock-answers.d/v01.json`, ohne HTTP.
    pub struct Answers(BTreeMap<String, Value>);

    impl Answers {
        pub fn load() -> Self {
            let doc: BTreeMap<String, Value> = serde_json::from_str(
                &std::fs::read_to_string(root().join("mock-answers.d/v01.json")).unwrap(),
            )
            .unwrap();
            Self(
                doc.into_iter()
                    .map(|(k, v)| (k, v["default"].clone()))
                    .collect(),
            )
        }
    }

    impl AiProvider for Answers {
        fn provider_id(&self) -> &str {
            "openai"
        }
        fn models(&self) -> ProviderFuture<'_, Vec<Model>> {
            Box::pin(async { Ok(Vec::new()) })
        }
        fn execute(&self, request: Request) -> ProviderFuture<'_, Response> {
            Box::pin(async move {
                let text: String = request
                    .input
                    .iter()
                    .filter_map(|i| match i {
                        Input::Text { text } => Some(text.as_str()),
                        _ => None,
                    })
                    .collect();
                let id = text
                    .split("MOCK-ID:")
                    .nth(1)
                    .and_then(|r| r.split_whitespace().next())
                    .unwrap_or("");
                Ok(Response {
                    content: Some(self.0.get(id).cloned().unwrap_or(json!({"candidates": []}))),
                    usage: Usage {
                        input_tokens: Some(300),
                        output_tokens: Some(60),
                        cached_input_tokens: Some(0),
                        cache_write_tokens: Some(0),
                        reasoning_tokens: Some(0),
                    },
                    refusal: false,
                    provider_request_id: None,
                    duration_ms: 1,
                })
            })
        }
    }

    /// Lauf beider Elemente über den Kern (Title: /a/ … /g/, Meta Description: /m1/ … /m5/), Text aus `pages.json`.
    pub async fn run(file: CrawlFile) -> CrawlFile {
        let cfg = file.config().unwrap();
        let rows = store::pages(file.conn()).unwrap();
        let index = SiteIndex::from_pages(&rows);
        let fixture = pages();
        let store = FileStore::new(file);
        let ledger = store.with(|f| {
            os_store::ai::budget::open_ledger(
                f.conn(),
                os_model::budget::BudgetLimits {
                    unlimited: true,
                    ..Default::default()
                },
                &[],
            )
            .unwrap()
        });
        let run_id = store.with(|f| {
            store::begin_run(f.conn(), "2026-11-26T09:00:00Z", &json!({"task": "suggest"})).unwrap()
        });
        let consent = ConsentGate::dialog(ConsentBook::new());
        for e in Element::ALL {
            consent
                .grant(ConsentKey::new("openai", e.task().as_str(), e.fields()), 0, None)
                .unwrap();
        }
        let redactor = Redactor::new(RedactionSettings::default());
        let limiter = Limiter::new(LimitsConfig::default(), Arc::new(TokioClock::new()));
        let clock = os_model::time::ManualClock::new(
            os_model::Timestamp::from_db("2026-11-26T09:00:00.000Z").unwrap(),
        );
        let control = RunControl::default();
        let provider = Answers::load();
        for (element, prefix) in [(Element::Title, "/"), (Element::MetaDescription, "/m")] {
            let (mut plan, rates, _) = catalog_plan(element, &cfg).expect("Katalogmodell");
            plan.model.capabilities = RESPONSES_CAPABILITIES;
            let inputs: Vec<PageInput> = rows
                .iter()
                .zip(&fixture)
                .filter(|(r, p)| {
                    let path = p["path"].as_str().unwrap();
                    r.in_standard_scope
                        && path.len() <= 4
                        && (prefix == "/m") == path.starts_with("/m")
                })
                .map(|(r, p)| PageInput {
                    url_id: r.url_id,
                    address: r.address.clone(),
                    title: r.title.clone(),
                    meta_description: r.meta_description.clone(),
                    h1: r.h1.clone(),
                    text: p["text"].as_str().map(str::to_string),
                    language: p["lang"].as_str().map(str::to_string),
                    ..PageInput::default()
                })
                .collect();
            let engine = Engine {
                switch: PreviewSwitch::ON,
                consent: &consent,
                redactor: &redactor,
                provider: &provider,
                limiter: &limiter,
                ledger: &ledger,
                plan: &plan,
                price: PriceBasis::Rates(&rates),
                store: &store,
                meter: &ui038,
                style: &NoStyleRules,
                clock: &clock,
                control: &control,
                run_id,
                base_url: "https://api.openai.com/v1".into(),
                cache: CacheOptions::default(),
                schema_overhead_tokens: 80,
                workers: 4,
            };
            engine
                .run(&ElementJob {
                    element,
                    pages: &inputs,
                    index: &index,
                    limits: ElementLimits::from_config(element, &cfg),
                    options: SuggestOptions::default(),
                })
                .await
                .unwrap();
        }
        store.with(|f| store::end_run(f.conn(), run_id, "2026-11-26T09:01:00Z").unwrap());
        store.into_inner()
    }
}

#[cfg(test)]
mod tests {
    use super::fixture::{self, BASE};
    use super::*;
    use crate::pool::CrawlKey;
    use crate::runtime::sessions::SessionInfo;
    use crate::{AppConfig, WindowId};
    use os_analyze::defs::ColumnSql;
    use std::sync::Arc;

    fn column_value(file: &CrawlFile, key: &str, url_id: i64) -> Value {
        let tab = os_analyze::tabs::ai_suggestions::tab();
        let column = tab.columns.iter().find(|c| c.key == key).unwrap_or_else(|| panic!("{key}"));
        let ColumnSql::Expr(sql) = column.sql else {
            panic!("{key}: kein Ausdruck")
        };
        file.conn()
            .query_row(&format!("SELECT {sql} FROM urls u WHERE u.id = ?1"), [url_id], |r| {
                Ok(match r.get_ref(0)? {
                    rusqlite::types::ValueRef::Null => Value::Null,
                    rusqlite::types::ValueRef::Integer(n) => json!(n),
                    rusqlite::types::ValueRef::Text(t) => json!(String::from_utf8_lossy(t)),
                    other => json!(format!("{other:?}")),
                })
            })
            .unwrap()
    }

    async fn built() -> (tempfile::TempDir, CrawlFile) {
        let dir = tempfile::tempdir().unwrap();
        let file = fixture::crawl(&dir.path().join("suggest.ospider"));
        let file = fixture::run(file).await;
        (dir, file)
    }

    // spec: AI-071 [M4]
    // spec: AI-072 [M4]
    // `Title Suggestion k Pixel Width` bzw. `Meta Description Suggestion k Pixel Width` sind für jeden Kandidaten der
    // Fixture identisch mit dem Wert, den UI-038 für denselben Text liefert (14 px bzw. 20 px, Desktop-Profil); die
    // Basisspalten `… 1 Pixel Width` bei gleichem Text ebenso.
    #[tokio::test]
    async fn stored_and_shown_pixel_widths_equal_ui038() {
        let (_dir, file) = built().await;
        let mut compared = 0;
        for element in Element::ALL {
            let prefix = if element == Element::Title { "Title" } else { "Meta Description" };
            for r in store::load_all(file.conn(), element.id()).unwrap() {
                for (slot, c) in r.displayed().iter().enumerate().take(5) {
                    let measured = c.value.split_whitespace().collect::<Vec<_>>().join(" ");
                    let expected = ui038(&measured, element.font_px());
                    assert_eq!(c.pixel_width, Some(expected), "{}", c.value);
                    let shown = column_value(
                        &file,
                        &format!("{prefix} Suggestion {} Pixel Width", slot + 1),
                        r.url_id,
                    );
                    assert_eq!(shown, json!(expected), "{prefix} {}: {}", slot + 1, c.value);
                    compared += 1;
                }
            }
        }
        assert_eq!(compared, 7 * 3 + 5 * 3);
        // Gleicher Text wie die Basisspalte → gleicher Wert (Meta Description 1 Pixel Width aus der Extraktion).
        let meta = file
            .conn()
            .query_row("SELECT meta_desc_1 FROM urls WHERE id = 1", [], |r| r.get::<_, String>(0))
            .unwrap();
        assert_eq!(column_value(&file, "Meta Description 1 Pixel Width", 1), json!(ui038(&meta, 14.0)));
    }

    // spec: AI-071 [M4]
    // spec: AI-070 [M4]
    // Tab „AI Suggestions“ zeigt bei /b/ nach dem Nachlauf „Y“ als Kandidat 1, bei /c/ (ohne Title) Kandidaten mit
    // leerem `Title 1`; Filter `Titles`, `Meta Descriptions`, `Open`.
    #[tokio::test]
    async fn tab_columns_and_filters_reflect_the_run() {
        let (_dir, file) = built().await;
        assert_eq!(
            column_value(&file, "Title Suggestion 1", 2),
            json!("Rucksäcke für Wanderungen im großen Vergleich")
        );
        assert_eq!(
            column_value(&file, "Title Suggestion 2", 2),
            json!("Leichte Wanderschuhe für Damen und Herren")
        );
        assert_eq!(column_value(&file, "Title 1", 3), Value::Null);
        assert_eq!(column_value(&file, "Title Suggestion Status", 3), json!("Valid"));
        assert_eq!(column_value(&file, "Review Status", 3), json!("Open"));
        let tab = os_analyze::tabs::ai_suggestions::tab();
        let count = |filter: &str| -> i64 {
            let f = tab.filters.iter().find(|f| f.id == filter).unwrap();
            file.conn()
                .query_row(
                    &format!("SELECT count(*) FROM urls u WHERE ({}) AND ({})", tab.population, f.predicate),
                    [],
                    |r| r.get(0),
                )
                .unwrap()
        };
        assert_eq!(count("All"), 12);
        assert_eq!(count("Titles"), 7);
        assert_eq!(count("Meta Descriptions"), 5);
        assert_eq!(count("Open"), 12);
        assert_eq!(count("Accepted"), 0);
        assert_eq!(count("Errors"), 0);
    }

    // spec: AI-070 [M4]
    // spec: AI-072 [M4]
    // Standard-Geltungsbereich Titles = Vereinigung von Missing, Duplicate, Over/Below Characters, Over/Below Pixels,
    // Same as H1 (aktuelle Grenzwerte) und Title Not Matching Content (AI); Meta Descriptions entsprechend; jede URL
    // höchstens einmal; nicht indexierbare URLs nie.
    #[test]
    fn default_scope_is_the_union_of_the_problem_filters() {
        let dir = tempfile::tempdir().unwrap();
        let file = fixture::crawl(&dir.path().join("scope.ospider"));
        // AI-054: /m1/ (ID 8) hat einen unpassenden Title, die noindex-Seite (ID 13) ebenso (bleibt außen vor).
        file.conn()
            .execute_batch(
                "INSERT INTO ai_questions VALUES('title-matches-content','1.0.0','{}');
                 INSERT INTO ai_answers(url_id,question_id,question_version,answer_json,status)
                   VALUES(8,'title-matches-content','1.0.0','{\"value\":false}','Success'),
                         (9,'title-matches-content','1.0.0','{\"value\":true}','Success'),
                         (13,'title-matches-content','1.0.0','false','Success');",
            )
            .unwrap();
        let cfg = CrawlConfig::default();
        let ctx = load_context(&file, &cfg).unwrap();
        let titles = scope_ids(&file, &cfg, &ctx, Element::Title, &Scope::Default).unwrap();
        assert_eq!(titles, vec![1, 2, 3, 4, 5, 6, 7, 8]);
        let metas = scope_ids(&file, &cfg, &ctx, Element::MetaDescription, &Scope::Default).unwrap();
        assert_eq!(metas, vec![8, 9, 10, 11, 12]);
        // Mit geänderten Grenzwerten (Title max. 40 Zeichen) wächst der Bereich entsprechend.
        let mut strict = cfg.clone();
        strict.title.max_chars = 40;
        let ctx = load_context(&file, &strict).unwrap();
        let more = scope_ids(&file, &strict, &ctx, Element::Title, &Scope::Default).unwrap();
        assert!(more.len() > titles.len(), "{more:?}");
        assert_eq!(
            scope_ids(&file, &cfg, &ctx, Element::Title, &Scope::Selected { url_ids: vec![2, 13, 2] })
                .unwrap(),
            vec![2]
        );
        assert_eq!(
            scope_ids(&file, &cfg, &ctx, Element::Title, &Scope::All).unwrap().len(),
            12
        );
    }

    // spec: AI-075 [M4]
    // Konsistenz: Für 50 Texte stimmt „gültig nach Länge und Pixelbreite“ mit „in keinem der Filter Over/Below X
    // Characters und Over/Below X Pixels (ONPAGE-093 – 096)“ überein.
    #[test]
    fn validity_matches_the_page_title_filters_for_fifty_texts() {
        let dir = tempfile::tempdir().unwrap();
        let file = fixture::crawl(&dir.path().join("consistency.ospider"));
        file.conn().execute_batch("DELETE FROM urls").unwrap();
        let mut texts = Vec::new();
        for n in [28, 29, 30, 31, 45, 59, 60, 61, 62, 70] {
            texts.push("a".repeat(n));
            texts.push(format!("Ti{}", "W".repeat(n.min(40) - 2)));
            texts.push(format!("{} {}", "Wanderschuhe".repeat(n / 12), "x".repeat(n % 12)));
            texts.push("i".repeat(n * 2));
            texts.push(format!("Seite {n:02} – Ratgeber für {}", "Ü".repeat(n / 3)));
        }
        assert_eq!(texts.len(), 50);
        for (i, t) in texts.iter().enumerate() {
            let collapsed = t.split_whitespace().collect::<Vec<_>>().join(" ");
            file.conn()
                .execute(
                    "INSERT INTO urls(id,url,address,origin,is_internal,state,discovered_via,folder_depth,url_length,
                       content_class,status_code,indexability,title_count,title_1,title_1_len,title_1_px)
                     VALUES(?1,?2,?2,'http://ai.test:8080',1,'Done','Seed',1,30,'HTML',200,'Indexable',1,?3,?4,?5)",
                    rusqlite::params![
                        i as i64 + 1,
                        format!("{BASE}/t{i}/"),
                        t,
                        collapsed.chars().count() as i64,
                        ui038(&collapsed, 20.0)
                    ],
                )
                .unwrap();
        }
        let cfg = CrawlConfig::default();
        let ids: Vec<i64> = (1..=50).collect();
        let members = os_analyze::views::filter_members_in(
            &[os_analyze::tabs::page_titles::tab()],
            &file,
            &cfg,
            &ids,
        )
        .unwrap();
        let filtered: BTreeSet<i64> = members
            .into_iter()
            .filter(|m| TITLE_FILTERS[2..6].contains(&m.filter.as_str()))
            .flat_map(|m| m.ids)
            .collect();
        let index = SiteIndex::default();
        let check = Checker {
            rules: RuleSet {
                duplicates: false,
                format: false,
                language: false,
                style: false,
                length: true,
            },
            ..checker(Element::Title, &cfg, &index)
        };
        let (mut valid, mut invalid) = (0, 0);
        for (i, t) in texts.iter().enumerate() {
            let ok = check.check(i as i64 + 1, None, t).valid();
            let in_filter = filtered.contains(&(i as i64 + 1));
            assert_eq!(ok, !in_filter, "{t:?}: gültig {ok}, in Filter {in_filter}");
            if ok { valid += 1 } else { invalid += 1 }
        }
        assert!(valid > 5 && invalid > 5, "{valid}/{invalid}");
    }

    fn app_for(file: CrawlFile, dir: &std::path::Path) -> App {
        let path = file.path().to_path_buf();
        let cfg = file.config().unwrap();
        file.checkpoint_and_truncate().unwrap();
        drop(file);
        let app = App::new(AppConfig::new(dir.join("home"))).unwrap();
        let key = CrawlKey("v01".into());
        app.register_crawl(key.clone(), path.clone());
        app.sessions().set_current(
            WindowId("main".into()),
            SessionInfo {
                key,
                path,
                config: Arc::new(cfg),
                state: os_model::CrawlState::Completed,
                writable: true,
                engine: None,
            },
        );
        app
    }

    async fn call(app: &App, name: &str, args: Value) -> Result<Value, CmdError> {
        app.dispatch(CallCtx::for_window("main"), name, args).await
    }

    async fn preview(app: &App, on: bool) {
        call(app, "settings.set", json!({"patch": {"aiApp": {"previewEnabled": on}}}))
            .await
            .unwrap();
    }

    // Plan M4 E1 (E-2 b): mit Schalter „KI (Vorschau)“ aus antwortet jeder Vorschlags-Command ohne Wirkung.
    // spec: AI-070 [M4]
    #[tokio::test]
    async fn every_command_refuses_while_the_preview_switch_is_off() {
        let (dir, file) = built().await;
        let app = app_for(file, dir.path());
        for name in [
            "ai.suggestRun",
            "ai.suggestPlan",
            "ai.suggestDetail",
            "ai.suggestReview",
            "ai.suggestCheck",
            "ai.suggestRecheck",
            "ai.suggestFocusImport",
        ] {
            let err = call(&app, name, json!({"urlId": 1})).await.unwrap_err();
            assert_eq!(err.code, ErrorCode::Unsupported, "{name}");
            assert_eq!(err.message_key, "err-ai-preview-off", "{name}");
        }
    }

    // spec: AI-076 [M4]
    // Prüfansicht, Live-Prüfung (Pixelbreite aus dem Kern), Übernehmen, Bearbeiten mit Verstoß (`Edited` mit
    // Verstoßliste), Massenaktion mit gezählten URLs ohne gültigen Kandidaten, Zurücksetzen.
    #[tokio::test]
    async fn review_commands_accept_edit_bulk_and_reset() {
        let (dir, file) = built().await;
        let app = app_for(file, dir.path());
        preview(&app, true).await;
        let detail = call(&app, "ai.suggestDetail", json!({"urlId": 2})).await.unwrap();
        let title = &detail["elements"][0];
        assert_eq!(title["element"], "title");
        assert_eq!(title["candidates"][0]["value"], "Rucksäcke für Wanderungen im großen Vergleich");
        assert_eq!(title["candidates"][0]["selected"], true);
        assert_eq!(
            title["candidates"][1]["violations"][0],
            format!("duplicate_candidate:{BASE}/a/")
        );
        let check = call(
            &app,
            "ai.suggestCheck",
            json!({"urlId": 2, "element": "title", "value": "Viel zu kurz"}),
        )
        .await
        .unwrap();
        assert_eq!(check["pixelWidth"], json!(ui038("Viel zu kurz", 20.0)));
        assert_eq!(check["valid"], false);
        let edited = call(
            &app,
            "ai.suggestReview",
            json!({"urlIds": [2], "element": "title", "action": "edit", "value": "Viel zu kurz"}),
        )
        .await
        .unwrap();
        assert_eq!(edited["updated"], 1);
        let detail = call(&app, "ai.suggestDetail", json!({"urlId": 2})).await.unwrap();
        assert_eq!(detail["elements"][0]["review"], "Edited");
        assert_eq!(detail["elements"][0]["accepted"]["violations"], check["violations"]);
        // „Kandidat 1 übernehmen“ für /a/ … /g/ und /noindex/ (ohne Eintrag): alle sieben gültig.
        let bulk = call(
            &app,
            "ai.suggestReview",
            json!({"urlIds": [1, 3, 4, 5, 6, 7, 13], "element": "title", "action": "acceptFirst"}),
        )
        .await
        .unwrap();
        assert_eq!((bulk["accepted"].clone(), bulk["skipped"].clone()), (json!(6), json!(0)));
        let reset = call(
            &app,
            "ai.suggestReview",
            json!({"urlIds": [1], "element": "title", "action": "reset"}),
        )
        .await
        .unwrap();
        assert_eq!(reset["updated"], 1);
        let detail = call(&app, "ai.suggestDetail", json!({"urlId": 1})).await.unwrap();
        assert_eq!(detail["elements"][0]["review"], "Open");
        let bad = call(
            &app,
            "ai.suggestReview",
            json!({"urlIds": [1], "element": "h9", "action": "reset"}),
        )
        .await
        .unwrap_err();
        assert_eq!(bad.code, ErrorCode::InvalidInput);
    }

    // spec: AI-075 [M4]
    // Neuprüfung ohne Request nach Grenzwertänderung (Title max. 40 Zeichen): Status ändert sich entsprechend.
    #[tokio::test]
    async fn recheck_applies_new_limits_without_a_request() {
        let (dir, file) = built().await;
        let app = app_for(file, dir.path());
        preview(&app, true).await;
        let requests_before = {
            let s = app.sessions().current(&WindowId("main".into())).unwrap();
            let f = CrawlFile::open(&s.path, os_store::OpenMode::ReadOnly).unwrap();
            f.conn().query_row("SELECT count(*) FROM ai_requests", [], |r| r.get::<_, i64>(0)).unwrap()
        };
        let mut s = app.sessions().current(&WindowId("main".into())).unwrap();
        let mut cfg = (*s.config).clone();
        cfg.title.max_chars = 40;
        s.config = Arc::new(cfg);
        app.sessions().set_current(WindowId("main".into()), s.clone());
        let out = call(&app, "ai.suggestRecheck", Value::Null).await.unwrap();
        assert_eq!(out["updated"], 12);
        let detail = call(&app, "ai.suggestDetail", json!({"urlId": 3})).await.unwrap();
        // /c/: alle Kandidaten über 40 Zeichen.
        assert_eq!(detail["elements"][0]["status"], "No valid suggestion");
        assert!(detail["elements"][0]["violations"][0].as_str().unwrap().starts_with("length>40"));
        let f = CrawlFile::open(&s.path, os_store::OpenMode::ReadOnly).unwrap();
        let after: i64 = f.conn().query_row("SELECT count(*) FROM ai_requests", [], |r| r.get(0)).unwrap();
        assert_eq!(after, requests_before);
    }

    // spec: AI-070 [M4]
    // spec: AI-012 [M4]
    // Fokus-Begriffe aus `Address,Keyword`; Voranschlag je Element (Zahlen für den Dialog von M4-U01); `ai.suggestRun`
    // legt die Einträge des Geltungsbereichs mit `Pending (confirmation)` an.
    #[tokio::test]
    async fn focus_import_plan_and_run_prepare_the_confirmation() {
        let dir = tempfile::tempdir().unwrap();
        let file = fixture::crawl(&dir.path().join("plan.ospider"));
        let app = app_for(file, dir.path());
        preview(&app, true).await;
        let csv = format!("Address,Keyword\n{BASE}/a/,wanderschuhe damen\n{BASE}/zz/,unbekannt\n");
        let imported = call(&app, "ai.suggestFocusImport", json!({"csv": csv})).await.unwrap();
        assert_eq!(imported["imported"], 1);
        assert_eq!(imported["unknown"], json!([format!("{BASE}/zz/")]));
        let plan = call(&app, "ai.suggestPlan", json!({"elements": ["title", "meta_description"]}))
            .await
            .unwrap();
        assert_eq!(plan["elements"][0]["entries"], 7);
        assert_eq!(plan["elements"][1]["entries"], 5);
        let est = &plan["elements"][0]["estimate"];
        assert_eq!(est["requests"], 7);
        assert_eq!(est["executable"], "Missing API key");
        assert!(est["expectedMicroUsd"].as_u64().unwrap() > 0);
        let handle = call(&app, "ai.suggestRun", json!({"tasks": ["suggest-title"]})).await.unwrap();
        let op_id = handle["opId"].as_str().unwrap().to_string();
        let status = tokio::time::timeout(std::time::Duration::from_secs(20), app.ops().wait(&op_id))
            .await
            .unwrap()
            .unwrap();
        let result = status.result.unwrap();
        assert_eq!(result["state"], "pendingConfirmation");
        assert_eq!(result["elements"][0]["entries"], 7);
        let detail = call(&app, "ai.suggestDetail", json!({"urlId": 1})).await.unwrap();
        assert_eq!(detail["elements"][0]["status"], "Pending (confirmation)");
        assert_eq!(detail["elements"][0]["focus"], "wanderschuhe damen");
    }

    // Crawl-Datei der E2E-Tests (`apps/desktop/e2e/ai-suggestions`): mit `UPDATE_GOLDEN=1` neu erzeugt, sonst geprüft.
    // spec: AI-070 [M4]
    #[tokio::test]
    async fn e2e_fixture_crawl_is_current() {
        let target = fixture::root().join("suggest/e2e/suggest.ospider");
        if std::env::var("UPDATE_GOLDEN").is_ok_and(|v| !v.is_empty() && v != "0") {
            let _ = std::fs::remove_file(&target);
            std::fs::create_dir_all(target.parent().unwrap()).unwrap();
            let file = fixture::run(fixture::crawl(&target)).await;
            file.checkpoint_and_truncate().unwrap();
            drop(file);
            for ext in ["-wal", "-shm"] {
                let _ = std::fs::remove_file(format!("{}{ext}", target.display()));
            }
        }
        let dir = tempfile::tempdir().unwrap();
        let copy = dir.path().join("copy.ospider");
        std::fs::copy(&target, &copy).expect("Fixture fehlt (UPDATE_GOLDEN=1)");
        let file = CrawlFile::open(&copy, os_store::OpenMode::ReadOnly).unwrap();
        assert_eq!(store::load_all(file.conn(), "title").unwrap().len(), 7);
        assert_eq!(store::load_all(file.conn(), "meta_description").unwrap().len(), 5);
        assert_eq!(store::pages(file.conn()).unwrap().len(), 13);
    }

    // spec: AI-070 [M4]
    #[test]
    fn focus_csv_and_html_lang_are_parsed() {

        let csv = "\u{feff}Address,Keyword\nhttp://ai.test:8080/a/,wanderschuhe damen\n\"http://ai.test:8080/b/\";\"rucksack\"\n,leer\n";
        assert_eq!(
            parse_focus_csv(csv),
            vec![
                ("http://ai.test:8080/a/".to_string(), "wanderschuhe damen".to_string()),
                ("http://ai.test:8080/b/".to_string(), "rucksack".to_string()),
            ]
        );
        assert_eq!(html_lang("<!doctype html><HTML LANG=\"de-DE\"><head>").as_deref(), Some("de-DE"));
        assert_eq!(html_lang("<html lang=en class=x>").as_deref(), Some("en"));
        assert_eq!(html_lang("<html><body lang=\"fr\">"), None);
    }
}
