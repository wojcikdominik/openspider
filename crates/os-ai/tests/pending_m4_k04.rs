//! Platzhalter M4-K04 (Plan M4 E4): Teilkriterien aus AI-009, AI-010, AI-011 und INTEG-162, die erst mit späteren
//! Paketen prüfbar sind. Die Datenseite (Schlüsselablage, Preisauflösung, Modellwahl, effektive Stufe, `max_tokens`)
//! ist in `keys.rs`, `catalog_pricing.rs`, `catalog_resources.rs` und `model_choice.rs` geprüft. Aufgelöst durch
//! M4-N01 bzw. M4-N02.

// spec: AI-009
// Dialog „KI-Anbieter“: OpenRouter läuft durch denselben INTEG-001-Testsatz (fünf KI-Einträge), Anzeige „aus
// Umgebungsvariable“, nach `Verbunden` Anzahl Modelle und (nur OpenRouter) Kontostand. Daten: `AiKeyStore::status`
// mit `KeySource::Environment` (geprüft).
#[test]
#[ignore = "wartet auf M4-U01: Dialog KI-Anbieter mit Schlüsselfeldern und Verbinden"]
fn provider_dialog_runs_integ_001_suite_for_openrouter_and_shows_env_source() {
    panic!("Oberfläche des Dialogs KI-Anbieter (AI-009, INTEG-001)");
}

// spec: AI-009
// Dateisystem-Scan nach `sk-TEST-AI-009` über Crawl-Datei, Profil, Protokoll (AI-014), Exporte, Logs und
// Absturzberichte nach einem KI-Lauf → 0 Treffer. Geprüft sind bisher Debug-Ausgaben, Status, Fehlertexte und das
// exportierte Aufgabenprofil.
#[test]
#[ignore = "wartet auf M4-D01: KI-Lauf über die Fixture-Site mit Protokoll und Exporten"]
fn no_key_in_any_file_after_an_ai_run() {
    panic!("Volltextsuche nach sk-TEST-AI-009 im Datenverzeichnis nach einem Lauf");
}

// spec: AI-009
// spec: AI-011
// Fehlt der Schlüssel einer konfigurierten Aufgabe, startet der Crawl trotzdem; alle Einträge der Aufgabe erhalten
// `Error: Missing API key`, der Kostenvoranschlag listet sie als „nicht ausführbar“; ohne geeignetes Modell
// `Skipped (no capable model)`. Daten: `plan_task` liefert den Statuswert (geprüft).
#[test]
#[ignore = "wartet auf M4-D01: Ausführung der Einträge eines Laufs und Kostenvoranschlag"]
fn run_without_key_marks_entries_and_estimate() {
    panic!(
        "Lauf über die Fixture-Site mit Profilen nur Anthropic, nur OpenAI, nur OpenRouter, keiner"
    );
}

// spec: AI-010
// Modell ohne Preis: Kostenschätzung zeigt „Preis unbekannt“, ein Lauf mit Budget startet die Aufgabe erst nach Eingabe
// eines Preises; Hinweis „Preis möglicherweise veraltet“ im Kostenvoranschlag. Daten: `PriceCatalog::resolve` = `None`,
// `ResolvedPrice::possibly_stale` (geprüft).
#[test]
#[ignore = "wartet auf M4-D01: Kostenvoranschlag und Budgetstart einer Aufgabe"]
fn unknown_and_stale_prices_in_the_estimate() {
    panic!("Kostenvoranschlag mit unbekanntem und veraltetem Preis");
}

// spec: AI-010
// Ein gewähltes Modell, das in der Live-Liste fehlt, wird wie in INTEG-160 gemeldet („nicht mehr verfügbar“). Daten:
// `choice::availability` (geprüft).
#[test]
#[ignore = "wartet auf M4-U01: Modellwahl mit Meldung nicht mehr verfügbar"]
fn missing_model_is_reported_like_integ_160() {
    panic!("Oberfläche der Modellwahl (AI-010, INTEG-160)");
}

// spec: AI-011
// Spalte `<Q> Model` zeigt das tatsächlich verwendete Modell, `<Q> Source = self-reported` im Ersatzweg; die
// Kostenspalte verwendet den Preis des genutzten Endpunkts (Protokoll `Endpoint = decisions` bzw. `responses`). Daten:
// `TaskPlan::model_ref`, `uses_decisions_api` (geprüft).
#[test]
#[ignore = "wartet auf M4-D01: Ergebnisspalten und Protokoll eines Laufs"]
fn result_columns_show_the_used_model_and_endpoint_price() {
    panic!("Lauf mit gpt-6-luna in decision und suggest-title gegen ai-mock");
}

// spec: AI-011
// Request-Felder im Mock-Log: `suggest-title` auf A mit `thinking.type = "disabled"`, `max_tokens = 1024`; auf B
// `thinking.type = "adaptive"` mit `output_config.effort = "low"`, `max_tokens = 2048`; `ask-sql` mit
// `reasoning.effort = "medium"`, `max_output_tokens = 6144`; `audit-summary` auf B `max_tokens = 4000`. K04 setzt
// `Request.reasoning` und `Request.max_tokens` (geprüft); die Übersetzung in die Anbieterfelder baut K09.
#[test]
#[ignore = "wartet auf M4-K09: Adapter übersetzen Request.reasoning und max_tokens, Mock-Modelle A–D"]
fn reasoning_and_max_tokens_reach_the_provider_requests() {
    panic!("Mock-Log der Anbieter-Requests für die Prüfwerte aus AI-011 (Nachtrag)");
}

// spec: AI-011
// Aufgabendialog: Spalte „Reasoning“ mit Dropdown, Warnungen „aus nicht verfügbar – niedrigste Stufe: low“ und
// „Ausgabegrenze knapp …“, „Auf Standard zurücksetzen“. Daten: `ReasoningChoice`, `OutputLimit::tight` (geprüft).
#[test]
#[ignore = "wartet auf M4-U01: Aufgabendialog der Modellwahl"]
fn task_dialog_shows_reasoning_column_and_warnings() {
    panic!("Oberfläche der Modellwahl (AI-011)");
}

// spec: INTEG-162 [M4]
// Chat-Prompt über den Anthropic-Mock → `max_tokens = 6144` ohne `thinking`; über OpenAI Responses
// `max_output_tokens = 6144` ohne `reasoning`; Modell mit Grenze 4.000 → 4.000 (Mock-Log). Die Regel
// `chat_prompt_max_tokens` ist geprüft.
#[test]
#[ignore = "wartet auf M4-P02: Ausführung der Chat-Prompts aus 04"]
fn chat_prompt_requests_carry_the_output_limit() {
    panic!("Mock-Log eines Chat-Prompts (INTEG-162, Nachtrag)");
}
