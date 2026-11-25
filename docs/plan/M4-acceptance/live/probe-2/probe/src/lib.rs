//! OpenSpider M4-Q00b – zweite Live-Probe: Form und Wirkung der Reasoning-Parameter (V-A6) und Speicherung bei OpenAI
//! (V-A8) nach `docs/spec/08-ai.md` mit den Nachträgen vom 24.11.2026. Kopie des Programms aus Probe 1 (Kostenbremse,
//! Schlüsselschutz, `record::sanitize` unverändert), mit eigenem Plan.
//!
//! Ohne `OPENSPIDER_LIVE_API_TESTS=1` **und** `--live` gibt das Programm nur den Plan aus (kein Netzwerk, kein
//! Schlüsselbund). Im Live-Lauf liest es die Schlüssel aus dem Schlüsselbund ([`secret::KeyringSource`]), sendet
//! jede geplante Anfrage höchstens einmal (keine Wiederholungen) unter der Kostenbremse ([`budget::Ledger`]) und
//! schreibt `results/<anbieter>.json` und `results/summary.json`.

pub mod budget;
pub mod cli;
pub mod plan;
pub mod record;
pub mod requests;
pub mod runner;
pub mod secret;

use std::path::PathBuf;

/// Ergebnisordner neben dem Programm (`<Probe-Ordner>/results`).
pub fn default_results_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("results")
}
