//! Platzhalter M4-K03 (Plan M4 E4): Teilkriterien aus AI-006, AI-008 und INTEG-005 [M4], die erst mit späteren
//! Paketen prüfbar sind, sowie die Gemini-Teile (AI-007, P2, nach 1.2 laut E-1 b und Orchestrator-Entscheidung vom
//! 24.11.2026). Die Adapter- und Embedding-Teile sind in `adapter_ollama.rs` und `embeddings.rs` geprüft. Aufgelöst
//! durch M4-N01 bzw. die Gemini-Etappe.

// spec: AI-006
// Die Oberfläche zeigt bei nicht erreichbarem Server neben dem Statuswert `Error: Connection failed` die Meldung nach
// INTEG-160 in der UI-Sprache (i18n) mit der Server-URL. Daten: `OllamaAdapter::unreachable_notice` (URL und
// kanonische Meldung `Server not reachable (<URL>)`, geprüft in adapter_ollama.rs).
#[test]
#[ignore = "wartet auf M4-D01: Zusatztext aus OllamaAdapter::unreachable_notice in Statusspalte/Tooltip übernehmen"]
fn status_columns_show_the_integ_160_message_for_an_unreachable_ollama_server() {
    panic!("Statusspalte und Zusatztext einer Aufgabe mit Ollama bei geschlossenem Port");
}

// spec: INTEG-005 [M4]
// Der Reiter „Erweitert“ des Ollama-Kontos zeigt Standard 1 und Bereich 1–50 für gleichzeitige Anfragen. Vorgabe:
// `ollama::DEFAULT_CONCURRENT_REQUESTS`, `CONCURRENT_REQUESTS_RANGE` (geprüft in adapter_ollama.rs).
#[test]
#[ignore = "wartet auf M4-U01: Anzeige im Reiter „Erweitert“"]
fn advanced_tab_shows_ollama_default_of_one_concurrent_request() {
    panic!("Konfigurationstest über alle Einträge aus INTEG-001 in der Oberfläche");
}

// spec: AI-008
// Ausgabe: Vektor je (URL, Embedding-Satz) in der Crawl-Datei (`ai_embeddings`, INTEG-166/180). Daten:
// `embed_texts` liefert normalisierte f32-Vektoren je Text (geprüft in embeddings.rs).
#[test]
#[ignore = "wartet auf M4-E01: Speicherung in ai_embeddings"]
fn vectors_are_stored_per_url_and_embedding_set() {
    panic!("Embedding-Lauf über die Fixture-Site, Vektoren in ai_embeddings");
}

// spec: AI-008
// Die Embedding-Modellwahl der Oberfläche bietet kein Anthropic-Modell an. Daten: `embeddings::embedding_models`
// (geprüft in embeddings.rs).
#[test]
#[ignore = "wartet auf M4-E01: Embedding-Modellwahl in der Oberfläche"]
fn embedding_model_choice_in_the_ui_offers_no_anthropic_model() {
    panic!("Modellwahl des Embedding-Prompts (INTEG-166)");
}

// spec: AI-008
// Gemini als Embedding-Anbieter („später Gemini (AI-007)“): `outputDimensionality`, Bündelung, Normalisierung.
#[test]
#[ignore = "P2: AI-007 Gemini-Adapter nach 1.2 (E-1 b, Orchestrator 24.11.2026)"]
fn gemini_embeddings_are_bundled_and_normalized() {
    panic!("Embedding-Anbieter Gemini gegen den Gemini-Mock (/gemini/v1beta/…)");
}

// spec: AI-007
// Contract-Test aus AI-001 läuft mit dem Gemini-Adapter grün.
#[test]
#[ignore = "P2: AI-007 Gemini-Adapter nach 1.2 (E-1 b, Orchestrator 24.11.2026)"]
fn contract_test_runs_with_the_gemini_adapter() {
    panic!("Gemini-Adapter und Gemini-Mock in os-testsite fehlen in der 1.1-Etappe");
}

// spec: AI-007
// Vor Fertigstellung des Adapters bietet die Modellwahl (AI-011) kein Gemini-Modell an; INTEG-Prompts mit Gemini
// funktionieren unverändert. Für Embeddings erfüllt `embedding_models` das bereits (geprüft in embeddings.rs).
#[test]
#[ignore = "P2: AI-007 Gemini-Adapter nach 1.2 (E-1 b, Orchestrator 24.11.2026)"]
fn model_choice_offers_no_gemini_model_before_the_adapter_exists() {
    panic!("Modellwahl AI-011 (K04/U01) und INTEG-Prompts mit Gemini (P01/P02)");
}
