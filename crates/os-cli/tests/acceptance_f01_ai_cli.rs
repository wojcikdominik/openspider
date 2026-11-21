//! Unabhängige Abnahmetests M4-F01: Register-Stubs der KI-Optionen im CLI.
//!
//! Erwartungen ausschließlich aus `docs/spec/08-ai.md` (AI-022, AI-013, AI-105) und
//! `docs/spec/06-output-ops.md` (OUT-201, KI- und Embedding-Flags). Die Semantik der Optionen gehört zu M4-C01;
//! F01 muss sie nur unter den Spec-Namen registrieren.
// acceptance: independent

use clap::Parser;
use os_cli::args::long_option_names;
use os_cli::Cli;

/// Optionsnamen laut AI-022 und OUT-201 [M4].
const AI_OPTIONS: [&str; 10] = [
    "--ai-tasks",
    "--ai-questions",
    "--api-budget",
    "--ai-budget",
    "--ai-consent",
    "--ai-batch",
    "--ai-reuse-cache",
    "--use-ai",
    "--use-embeddings",
    "--export-query",
];

// spec: AI-022
// acceptance: independent
#[test]
fn all_ai_options_from_the_spec_are_registered() {
    let names = long_option_names();
    let missing: Vec<&str> = AI_OPTIONS
        .iter()
        .copied()
        .filter(|o| !names.iter().any(|n| n == o))
        .collect();
    assert!(missing.is_empty(), "nicht registriert: {missing:?}");
}

// spec: AI-022
// acceptance: independent
// Die Optionen werden mit den Wertformen der Spec gelesen (Liste, IDs, USD-Betrag, Schalter, Datei).
#[test]
fn ai_options_parse_with_spec_value_forms() {
    let cli = Cli::try_parse_from([
        "openspider",
        "--ai-tasks",
        "decisions,suggest-title",
        "--ai-questions",
        "page-type,ymyl",
        "--api-budget",
        "0.01",
        "--ai-budget",
        "0.01",
        "--ai-consent",
        "--ai-batch",
        "auto",
        "--ai-reuse-cache",
        "vorlauf.ospider",
        "--use-ai",
        "--use-embeddings",
        "--export-query",
        "Seiten ohne H1",
    ]);
    assert!(cli.is_ok(), "{:?}", cli.err().map(|e| e.to_string()));
}

// spec: AI-022
// acceptance: independent
// `--ai-batch auto|on|off` – genau diese drei Werte.
#[test]
fn ai_batch_accepts_exactly_auto_on_off() {
    for v in ["auto", "on", "off"] {
        assert!(
            Cli::try_parse_from(["openspider", "--ai-batch", v]).is_ok(),
            "{v}"
        );
    }
    for v in ["always", "yes", ""] {
        assert!(
            Cli::try_parse_from(["openspider", "--ai-batch", v]).is_err(),
            "{v:?} darf nicht angenommen werden"
        );
    }
}
