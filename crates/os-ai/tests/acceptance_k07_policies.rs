//! Unabhängige Abnahmetests M4-K07: Datendatei `resources/ai/provider-policies.json` (AI-018).
//!
//! AI-018: Die Datenflussübersicht zeigt je Anbieter, ob er laut seiner Dokumentation Eingaben für Training nutzen
//! darf, „Angabe aus `resources/ai/provider-policies.json` mit Quelle und Prüfdatum“. Geprüft wird nur die formale
//! Vollständigkeit der Datei (kein Abgleich mit den Anbieterseiten). Die Anbieter kommen aus AI-001: Anthropic,
//! OpenAI (Responses und Decisions), OpenRouter, Ollama, Gemini.
// acceptance: independent

mod acceptance_k07_support;

use acceptance_k07_support::{read_fixture, repo_file};
use os_ai::policy::{ConsentBook, ConsentGate, DataFlowOverview, ProviderPolicies, TaskDataFlow};
use serde_json::Value;

const FILE: &str = "resources/ai/provider-policies.json";
const PROVIDERS: [&str; 5] = ["anthropic", "openai", "openrouter", "ollama", "gemini"];

fn raw() -> Value {
    serde_json::from_str(&read_fixture(FILE)).expect("gültiges JSON")
}

fn is_calendar_date(s: &str) -> bool {
    let parts: Vec<&str> = s.split('-').collect();
    if parts.len() != 3 || parts[0].len() != 4 || parts[1].len() != 2 || parts[2].len() != 2 {
        return false;
    }
    let (Ok(y), Ok(m), Ok(d)) = (
        parts[0].parse::<u32>(),
        parts[1].parse::<u32>(),
        parts[2].parse::<u32>(),
    ) else {
        return false;
    };
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let days = match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return false,
    };
    (2024..=2100).contains(&y) && (1..=days).contains(&d)
}

// spec: AI-018
#[test]
fn ai018_provider_policies_jeder_anbieter_hat_trainingsangabe_und_pruefdatum() {
    // Die mitgelieferte Datei ist die, die geladen wird.
    let loaded = ProviderPolicies::load(&repo_file(FILE)).expect("Datei lädt");
    assert_eq!(loaded, ProviderPolicies::embedded());

    let v = raw();
    let providers = v["providers"].as_object().expect("Objekt providers");
    for id in PROVIDERS {
        let p = providers
            .get(id)
            .unwrap_or_else(|| panic!("{id}: fehlt in {FILE}"));
        let use_ = p["training_use"].as_str().unwrap_or_default();
        assert!(
            ["no", "yes", "depends"].contains(&use_),
            "{id}: training_use = {use_:?}"
        );
        assert!(
            !p["summary"].as_str().unwrap_or_default().trim().is_empty(),
            "{id}: Zusammenfassung fehlt"
        );
        let date = p["verified_at"].as_str().unwrap_or_default();
        assert!(is_calendar_date(date), "{id}: Prüfdatum {date:?}");
    }
}

// spec: AI-018
// „mit Quelle": jeder Anbieter nennt eine Quelle (URL der Anbieterdokumentation).
#[test]
fn ai018_provider_policies_jeder_anbieter_hat_quelle() {
    let v = raw();
    let mut missing = Vec::new();
    for id in PROVIDERS {
        let src = v["providers"][id]["source_url"]
            .as_str()
            .unwrap_or_default();
        if !src.starts_with("https://") || src.len() <= "https://".len() {
            missing.push(id);
        }
    }
    assert!(missing.is_empty(), "ohne Quelle: {missing:?}");
}

// spec: AI-018
// Die Spalte `Training Use (per provider policy)` zeigt für jeden Anbieter Angabe, Quelle und Prüfdatum aus der Datei.
#[test]
fn ai018_datenfluss_zeigt_trainingsangabe_mit_quelle_und_pruefdatum() {
    let policies = ProviderPolicies::embedded();
    let gate = ConsentGate::dialog(ConsentBook::new());
    let v = raw();
    for id in PROVIDERS {
        let flows = [TaskDataFlow {
            task: "summarize".into(),
            provider: id.into(),
            model: "m".into(),
            endpoint: "https://example.invalid/v1".into(),
            region: None,
            fields: Default::default(),
            max_chars: None,
            api: None,
        }];
        let row = &DataFlowOverview::build(&flows, &policies, &gate, true).rows[0];
        let cell = &row.cells()[9];
        let p = &v["providers"][id];
        assert!(
            cell.contains(p["training_use"].as_str().unwrap()),
            "{id}: {cell}"
        );
        assert!(
            cell.contains(p["verified_at"].as_str().unwrap()),
            "{id}: {cell}"
        );
        if let Some(src) = p["source_url"].as_str() {
            assert!(cell.contains(src), "{id}: {cell}");
        }
    }
}
