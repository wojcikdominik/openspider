//! Platzhalter M4-K01 (Plan M4 E4): Teilkriterien aus AI-002 und AI-005, die erst mit späteren Paketen bzw. einer
//! Mock-Erweiterung prüfbar sind. Die Adapterteile sind in `adapter_anthropic.rs` und `adapter_openrouter.rs` geprüft.
//! Aufgelöst durch M4-N01.

// spec: AI-002
// `pricing.json` enthält für Anthropic keine Fähigkeitsfelder (CI-Schema, AI-010). `resources/ai/pricing.json` gibt es
// noch nicht; Datei und Schema liefert M4-K04.
#[test]
#[ignore = "wartet auf M4-K04: resources/ai/pricing.json mit Anthropic-Einträgen"]
fn anthropic_price_entries_carry_no_capability_fields() {
    panic!("Prüfung der Anthropic-Einträge in resources/ai/pricing.json gegen das CI-Schema (K04)");
}

// spec: AI-002
// Modelle C und D sind in der Modellwahl ausgegraut, D mit Hinweis „Fähigkeiten unbekannt“; eine gespeicherte, nicht
// mehr unterstützte Aufwandsstufe zeigt eine Warnung im Aufgabendialog. Daten: `AnthropicModel::selectable_for`,
// `capabilities_known`, `effort_unsupported` (geprüft in adapter_anthropic.rs).
#[test]
#[ignore = "wartet auf M4-U01: Modellwahl und Aufgabendialog"]
fn model_choice_greys_out_c_and_d_and_warns_about_unsupported_effort() {
    panic!("Oberfläche der Modellwahl (AI-011)");
}

// spec: AI-002
// Im Batch-Modus (AI-016) laufen Aufgaben mit Modell C synchron. Daten: `AnthropicModel::runs_in_batch` (geprüft);
// der Batch-Weg ist in 1.1 nicht gebaut (K4-18).
#[test]
#[ignore = "Klärung K4-18: Batch-Modus AI-016 nach 1.1"]
fn tasks_with_model_c_run_synchronously_in_batch_mode() {
    panic!("Batch-Ausführung über os-ai::batch mit Modell C");
}

// spec: AI-005
// Modellauswahl mit `Model`, `Context`, `Input $/1M`, `Output $/1M`, `Per Request $`, `Per Image $`, `Structured
// Output`, `Vision`; Schema-Aufgaben ausgegraut mit Hinweis. Daten: `OpenRouterModel` (geprüft).
#[test]
#[ignore = "wartet auf M4-U01: Modellauswahl mit Suche und Preisspalten"]
fn model_picker_shows_price_columns_and_greys_out_models_without_structured_output() {
    panic!("Oberfläche der OpenRouter-Modellauswahl");
}

// spec: AI-005
// Kontostand im Dialog „KI-Anbieter“: `Limit Remaining 5.80`, `Limit Reset monthly`, Variante `Limit Remaining:
// unbegrenzt`; nach einem endgültigen Kontingentfehler neu geladen. Daten: `KeyBalance`, `last_balance` (geprüft).
#[test]
#[ignore = "wartet auf M4-U01: Anzeige des Kontostands"]
fn balance_display_shows_limit_remaining_and_unlimited() {
    panic!("Oberfläche des Kontostands (AI-009)");
}

// spec: AI-005
// `/key`-Varianten des ai-mock „mit periodischem Limit und ohne Limit“ (Testfixture AI-005): der Mock liefert nur eine
// feste Antwort (`limit = 100`, `limit_remaining = 90`, ohne `limit_reset`). Die Varianten sind als Antwort-JSON in
// adapter_openrouter.rs geprüft; über HTTP fehlt die Mock-Erweiterung (außerhalb von K01).
#[test]
#[ignore = "wartet auf M4-N01: /key-Varianten im ai-mock"]
fn key_variants_over_http() {
    panic!("ai-mock /openrouter/api/v1/key mit limit=10, usage=42.00, limit_remaining=5.80, limit_reset=monthly bzw. null");
}

// spec: AI-002
// spec: AI-005
// spec: AI-020
// Lauf über die Fixture-Site: bei Kontingent erhalten alle offenen Einträge des Anbieters im Lauf `Quota exceeded`
// (Statusspalten, Protokollzeilen); die Kontingent-Sperre des Adapters (`QuotaLatch`) ist geprüft.
#[test]
#[ignore = "wartet auf M4-D01: Ausführung der Einträge eines Laufs"]
fn quota_stop_marks_every_open_entry_of_a_run() {
    panic!("Lauf der Seitentyp-Frage mit Kontingent-Szenario gegen ai-mock");
}
