//! OpenSpider M4-Q00 – frühe Live-Probe der KI-Anbieter gegen `docs/spec/08-ai.md` (Stand 13.10.2026).
//!
//! Ohne `OPENSPIDER_LIVE_API_TESTS=1` **und** `--live` gibt das Programm nur den Plan aus (kein Netzwerk, kein
//! Schlüsselbund). Im Live-Lauf liest es die Schlüssel aus dem Schlüsselbund ([`secret::KeyringSource`]), sendet
//! jede geplante Anfrage höchstens einmal (keine Wiederholungen) unter der Kostenbremse ([`budget::Ledger`]) und
//! schreibt `results/<anbieter>.json` und `results/summary.json`.

pub mod budget;
pub mod cli;
pub mod plan;
pub mod png;
pub mod record;
pub mod requests;
pub mod runner;
pub mod secret;

use std::path::PathBuf;

/// Ergebnisordner neben dem Programm (`<Probe-Ordner>/results`).
pub fn default_results_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("results")
}
