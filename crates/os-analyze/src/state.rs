//! Filterzustände (`FilterState`): bereit, Crawl-Analyse nötig, Konfiguration nötig, Rendering nötig, veraltet
//! (ONPAGE-007, TECH-005, CRAWL-141).
//!
//! Reihenfolge der Prüfung: (1) Konfigurationsschlüssel aus `FilterDef.requires` → `NeedsConfig`,
//! (2) Rendering → `NeedsRendering`, (3) Live-Filter → `Ready`, (4) Nachlauf-Filter: Teilschritt abgewählt
//! oder nicht (erfolgreich) gelaufen → `NeedsCrawlAnalysis`; gelaufen, aber seither veraltet →
//! `Stale`; sonst `Ready`. Ein Filter in einem anderen Zustand als `Ready` liefert nie eine Trefferzahl
//! und keine Zeilen – nie 0 an Stelle eines Zustands.

use std::collections::BTreeMap;

use os_model::{AnalysisStep, CrawlConfig, FilterState, RenderingMode, StepState};
use os_store::CrawlFile;
use serde_json::Value as Json;

use crate::analysis::persist::{self, StaleSet, StepRecord};
use crate::defs::{FilterDef, FilterKind, Requirement};
use crate::error::AnalyzeError;
use crate::names;

/// Konfigurationsabschnitte, deren Änderung einen Teilschritt veralten lässt (CRAWL-141, Punkt 6).
///
/// Der Schritt „Sitemaps“ hängt zusätzlich an `spider.*`: Linkspeicherung, hreflang und Canonical bestimmen,
/// welche URLs als verlinkt gelten und damit die Orphan-Bewertung (TECH-005).
fn relevant_sections(step: AnalysisStep) -> &'static [&'static str] {
    match step {
        AnalysisStep::LinkScore | AnalysisStep::Links => &["links"],
        AnalysisStep::Canonicals | AnalysisStep::Pagination | AnalysisStep::Amp => &["spider"],
        AnalysisStep::Hreflang => &["hreflang"],
        AnalysisStep::Sitemaps => &["sitemaps", "spider"],
        // NearDuplicates (K2-19, M2-D02): Schwelle und Schalter stehen in `duplicates`, der Inhaltsbereich in `content`
        AnalysisStep::Embeddings => &["embeddings"],
        AnalysisStep::AiSearch => &[],
        AnalysisStep::Content => &["content.area", "duplicates.exact_enabled"],
        AnalysisStep::NearDuplicates => &[
            "duplicates.near_enabled",
            "duplicates.near_threshold_pct",
            "duplicates.only_indexable",
            "content.area",
        ],
        AnalysisStep::Images => &["image", "images", "rendering.mode"],
        AnalysisStep::Analytics | AnalysisStep::SearchConsole | AnalysisStep::LinkMetrics => &[],
    }
}

/// Hash (16 Hexziffern, FNV-1a) der für den Teilschritt relevanten Konfiguration.
pub fn step_config_hash(step: AnalysisStep, cfg: &CrawlConfig) -> String {
    let json = serde_json::to_value(cfg).unwrap_or(Json::Null);
    step_config_hash_json(step, &json)
}

/// Wie [`step_config_hash`], mit bereits in JSON gewandelter Konfiguration.
pub fn step_config_hash_json(step: AnalysisStep, cfg_json: &Json) -> String {
    let mut text = String::new();
    for section in relevant_sections(step) {
        text.push_str(section);
        text.push('=');
        if let Some(value) = names::config_value(cfg_json, section) {
            text.push_str(&value.to_string());
        }
        text.push(';');
    }
    crate::cursor::query_hash(&[&text])
}

/// Wie ein Teilschritt zum aktuellen Zeitpunkt dasteht.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Freshness {
    /// Fertig und aktuell.
    Fresh,
    /// Fertig, aber seither veraltet (Markierung oder geänderte Konfiguration).
    Stale,
    /// Nicht fertig (nie gelaufen, abgebrochen, übersprungen, nicht anwendbar).
    Missing,
}

/// Stand der Crawl-Analyse einer Datei, einmal gelesen und für viele Filter verwendet.
#[derive(Debug, Clone, Default)]
pub struct AnalysisSnapshot {
    steps: BTreeMap<AnalysisStep, StepRecord>,
    stale: StaleSet,
}

impl AnalysisSnapshot {
    /// Liest `analysis_steps` und die Veraltet-Markierungen.
    pub fn load(file: &CrawlFile) -> Result<Self, AnalyzeError> {
        Ok(AnalysisSnapshot {
            steps: persist::read_steps(file)?,
            stale: persist::read_stale(file)?,
        })
    }

    /// Gespeicherter Zustand eines Teilschritts, falls vorhanden.
    pub fn record(&self, step: AnalysisStep) -> Option<&StepRecord> {
        self.steps.get(&step)
    }

    /// Frische des Teilschritts bezogen auf die aktuelle Konfiguration.
    pub fn freshness(&self, step: AnalysisStep, cfg_json: &Json) -> Freshness {
        match self.steps.get(&step) {
            Some(rec) if rec.state == StepState::Done => {
                if self.stale.contains(step) {
                    return Freshness::Stale;
                }
                let now = step_config_hash_json(step, cfg_json);
                match &rec.config_hash {
                    Some(stored) if *stored != now => Freshness::Stale,
                    _ => Freshness::Fresh,
                }
            }
            _ => Freshness::Missing,
        }
    }
}

/// Ist der Konfigurationsschlüssel (`duplicates.near_enabled`) vorhanden und `true`?
pub fn config_key_enabled(cfg_json: &Json, key: &str) -> bool {
    matches!(names::config_value(cfg_json, key), Some(Json::Bool(true)))
}

/// Zustand eines Filters. `cfg_json` ist `serde_json::to_value(cfg)`.
pub fn filter_state(
    def: &FilterDef,
    cfg: &CrawlConfig,
    cfg_json: &Json,
    analysis: &AnalysisSnapshot,
) -> FilterState {
    for req in &def.requires {
        match req {
            Requirement::AnyConfig(keys)
                if !keys.iter().any(|key| config_key_enabled(cfg_json, key)) =>
            {
                return FilterState::NeedsConfig(keys.join(" | "))
            }
            Requirement::RenderingOrConfig(key)
                if cfg.rendering.mode != RenderingMode::JavaScript
                    && !config_key_enabled(cfg_json, key) =>
            {
                return FilterState::NeedsRendering
            }
            Requirement::Coverage { config, .. } if !config_key_enabled(cfg_json, config) => {
                return FilterState::NeedsConfig((*config).into())
            }
            _ => {}
        }
    }
    for req in &def.requires {
        if let Requirement::Config(key) = req {
            if !config_key_enabled(cfg_json, key) {
                return FilterState::NeedsConfig((*key).to_string());
            }
        }
    }
    if def.requires.contains(&Requirement::Rendering)
        && cfg.rendering.mode != RenderingMode::JavaScript
    {
        return FilterState::NeedsRendering;
    }
    match def.kind {
        FilterKind::Live => FilterState::Ready,
        FilterKind::PostCrawl(step) => {
            if !cfg.analysis.step_enabled(step) {
                return FilterState::NeedsCrawlAnalysis;
            }
            match analysis.freshness(step, cfg_json) {
                Freshness::Fresh => FilterState::Ready,
                Freshness::Stale => FilterState::Stale,
                Freshness::Missing => FilterState::NeedsCrawlAnalysis,
            }
        }
    }
}

/// Coverage-Zustand bezogen auf die Grundmenge eines Tabs (TECH-008).
pub fn filter_state_for_file(
    def: &FilterDef,
    tab: &crate::defs::TabDef,
    file: &CrawlFile,
    cfg: &CrawlConfig,
    cfg_json: &Json,
    analysis: &AnalysisSnapshot,
) -> Result<FilterState, crate::error::ViewError> {
    Ok(filter_states_for_file(
        std::slice::from_ref(def),
        tab,
        file,
        cfg,
        cfg_json,
        analysis,
    )?
    .remove(0))
}

/// Ein Coverage-Durchlauf je Tab, auch wenn mehrere Filter denselben Marker verwenden.
pub(crate) fn filter_states_for_file(
    defs: &[FilterDef],
    tab: &crate::defs::TabDef,
    file: &CrawlFile,
    cfg: &CrawlConfig,
    cfg_json: &Json,
    analysis: &AnalysisSnapshot,
) -> Result<Vec<FilterState>, crate::error::ViewError> {
    let mut states: Vec<_> = defs
        .iter()
        .map(|def| filter_state(def, cfg, cfg_json, analysis))
        .collect();
    // INTEG-015: API orphan filters retain their canonical live-SQL definitions,
    // while the file-bound state supplies the required configuration/analysis hints.
    for (def, state) in defs.iter().zip(&mut states) {
        if state.yields_rows() {
            if let Some(required) =
                crate::tabs::dynamic_api::orphan_state(def, tab, cfg, cfg_json, analysis)
            {
                *state = required;
            }
        }
    }
    let markers: std::collections::BTreeSet<_> = defs
        .iter()
        .zip(&states)
        .filter(|(_, state)| state.yields_rows())
        .flat_map(|(def, _)| {
            def.requires.iter().filter_map(|req| match req {
                Requirement::Coverage { marker, .. } => Some(*marker),
                _ => None,
            })
        })
        .collect();
    if markers.is_empty() {
        return Ok(states);
    }
    let markers: Vec<_> = markers.into_iter().collect();
    let domains: Vec<_> = markers
        .iter()
        .enumerate()
        .filter_map(|(i, marker)| {
            crate::defs::coverage_domain(tab.id, marker).map(|domain| (i, domain))
        })
        .collect();
    let domain_sums = domains
        .iter()
        .map(|(i, domain)| {
            format!(
                ", COALESCE(SUM(CASE WHEN ({domain}) THEN 1 ELSE 0 END), 0), \
         COALESCE(SUM(CASE WHEN ({domain}) THEN EXISTS(SELECT 1 FROM url_ext e \
         WHERE e.url_id = u.id AND e.key = :coverage_{i} AND e.v_int = 1) ELSE 0 END), 0)"
            )
        })
        .collect::<String>();
    let sums = markers.iter().enumerate().map(|(i, _)| format!(
        "COALESCE(SUM(EXISTS(SELECT 1 FROM url_ext e WHERE e.url_id = u.id AND e.key = :coverage_{i} AND e.v_int = 1)), 0)"
    )).collect::<Vec<_>>().join(", ");
    let population = match tab.hidden_population {
        Some(hidden) => format!("({}) OR ({hidden})", tab.population),
        None => tab.population.to_string(),
    };
    let sql = format!("SELECT COUNT(*), {sums}{domain_sums} FROM urls u WHERE ({population})");
    let mut stmt = file.conn().prepare(&sql)?;
    for (name, value) in (tab.params)(cfg) {
        let name = if name.starts_with(':') {
            name.to_owned()
        } else {
            format!(":{name}")
        };
        if let Some(i) = stmt.parameter_index(&name)? {
            let value = match value {
                os_model::Value::Null => rusqlite::types::Value::Null,
                os_model::Value::Bool(v) => rusqlite::types::Value::Integer(i64::from(v)),
                os_model::Value::Int(v) => rusqlite::types::Value::Integer(v),
                os_model::Value::Float(v) => rusqlite::types::Value::Real(v),
                os_model::Value::Text(v) => rusqlite::types::Value::Text(v),
            };
            stmt.raw_bind_parameter(i, value)?;
        }
    }
    for (i, marker) in markers.iter().enumerate() {
        stmt.raw_bind_parameter(
            stmt.parameter_index(&format!(":coverage_{i}"))?
                .expect("marker"),
            marker,
        )?;
    }
    let mut rows = stmt.raw_query();
    let row = rows.next()?.ok_or(rusqlite::Error::QueryReturnedNoRows)?;
    let total: u64 = row.get(0)?;
    let checked: BTreeMap<_, u64> = markers
        .iter()
        .enumerate()
        .map(|(i, marker)| Ok((*marker, row.get(i + 1)?)))
        .collect::<Result<_, rusqlite::Error>>()?;
    let domain_counts: BTreeMap<_, (u64, u64)> = domains
        .iter()
        .enumerate()
        .map(|(offset, (i, _))| {
            let column = 1 + markers.len() + offset * 2;
            Ok((markers[*i], (row.get(column)?, row.get(column + 1)?)))
        })
        .collect::<Result<_, rusqlite::Error>>()?;
    for (def, state) in defs.iter().zip(&mut states) {
        if !state.yields_rows() {
            continue;
        }
        let mut unchecked = 0;
        for req in &def.requires {
            if let Requirement::Coverage { marker, .. } = req {
                let covered = checked[marker];
                let (total, covered) = domain_counts
                    .get(marker)
                    .copied()
                    .unwrap_or((total, covered));
                if total > 0 && covered == 0 {
                    *state = FilterState::NeedsRecrawl;
                    break;
                }
                unchecked = unchecked.max(total.saturating_sub(covered));
            }
        }
        if state.yields_rows() && unchecked > 0 {
            *state = FilterState::Incomplete { unchecked };
        }
    }
    Ok(states)
}
