//! Platzhalter für Kriterien der Nachträge 24.11.2026 (Q00) zu AI-001 … AI-006, die erst spätere Pakete prüfbar machen
//! (M4-K09). Die Adapterseite ist umgesetzt und in `adapter_q00.rs` bzw. `contract_q00.rs` getestet; hier fehlen die
//! Oberfläche (M4-U01), die Batch-Zeilen (M4-K06, Etappe 1.2) und der Crawl-Lauf (M4-D01). Die Platzhalter für M4-K10
//! sind aufgelöst (`status_values_q00.rs`, `cost_q00.rs`).

// spec: AI-002 [M4]
// Stufe `off` auf Modell B → der Aufgabendialog zeigt „aus nicht verfügbar – niedrigste Stufe: low“. Request-Seite:
// adapter_q00.rs `anthropic_levels_map_to_thinking_and_effort`.
#[test]
#[ignore = "wartet auf M4-U01: Hinweis im Aufgabendialog (Reasoning-Spalte der Modellwahl)"]
fn anthropic_off_unavailable_hint_in_task_dialog() {
    unimplemented!("M4-U01");
}

// spec: AI-003 [M4]
// Jede JSONL-Zeile eines Batch-Auftrags mit Ziel `/v1/responses` enthält `"store": false` (Teil des Property-Tests
// „jeder Request an /openai/v1/responses“; der synchrone Teil steht in adapter_q00.rs).
#[test]
#[ignore = "wartet auf M4-K06: Batch-Zeilen der Responses API (Etappe 1.2, AI-016)"]
fn responses_batch_lines_send_store_false() {
    unimplemented!("M4-K06");
}

// spec: AI-005 [M4]
// Die Modellauswahl zeigt für `M-LC` `Long Context From 1000` (Wert: `OpenRouterModel::long_context_from`).
#[test]
#[ignore = "wartet auf M4-U01: Spalte Long Context From in der Modellauswahl"]
fn openrouter_long_context_from_column() {
    unimplemented!("M4-U01");
}

// spec: AI-001 [M4]
// Volltextsuche nach `MOCK-REASONING-TEXT`, `MOCK-ENCRYPTED` und `MOCK-SIGNATURE` in Crawl-Datei, `ai_requests`,
// `ai_cache` und Anwendungslogs nach einem Lauf mit Szenario `reasoning=40` → 0 Treffer (Ergebnisseite: contract_q00.rs).
#[test]
#[ignore = "wartet auf M4-D01: KI-Lauf über die Crawl-Datei mit Protokoll und Cache"]
fn reasoning_content_never_reaches_crawl_file_or_logs() {
    unimplemented!("M4-D01");
}
