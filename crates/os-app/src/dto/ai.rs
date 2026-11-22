//! DTOs des KI-Command-Katalogs (Plan M4 3.3, M4-F02). Gerüst: Die Fachpakete füllen die Commands und dürfen die
//! Typen additiv erweitern; Geldbeträge in Mikro-USD wie im `BudgetLedger` (AI-013, 08 Konventionen).
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Argumente eines KI-Laufs über bereits gecrawlte URLs (Aufgaben wie `--ai-tasks`, Stichprobe wie `--ai-sample`,
/// Budget wie `--ai-budget`; AI-022, AI-023, AI-013).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(optional_fields = nullable)]
pub struct AiRunArgs {
    /// Aufgaben bzw. Fragen-IDs; leer = die im Profil aktiven.
    #[serde(default)]
    pub tasks: Vec<String>,
    /// Stichprobengröße (AI-023); ohne = alle URLs des Geltungsbereichs.
    #[serde(default)]
    pub sample: Option<u32>,
    /// Budget dieses Laufs in Mikro-USD (AI-013); ohne = Crawl-Budget.
    #[serde(default)]
    pub budget_micro_usd: Option<u64>,
}

/// Kostenstand des gemeinsamen `BudgetLedger` für das Statusleistenfeld `Kosten` (UI-013 [M4], AI-013); alle
/// Beträge in Mikro-USD. Das Anzeigeformat setzt die Oberfläche (M4-U01).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(optional_fields = nullable)]
pub struct AiCostSnapshot {
    /// Verbraucht insgesamt.
    pub spent_micro_usd: u64,
    /// Budget; ohne = unbegrenzt (AI-013).
    pub budget_micro_usd: Option<u64>,
    /// Davon KI.
    pub ai_micro_usd: u64,
    /// Davon DataForSEO (ab M5).
    pub data_for_seo_micro_usd: u64,
    /// Offene Reservierungen.
    pub reserved_micro_usd: u64,
}
