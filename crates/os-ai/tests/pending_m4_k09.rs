//! Platzhalter für Kriterien der Nachträge 24.11.2026 (Q00) zu AI-001 … AI-006, die erst spätere Pakete prüfbar machen
//! (M4-K09). Die Adapterseite ist umgesetzt und in `adapter_q00.rs` bzw. `contract_q00.rs` getestet; hier fehlen der
//! Statuswert `Error: Output limit reached` und die neuen Protokollspalten (M4-K10), die Oberfläche (M4-U01) und der
//! Crawl-Lauf (M4-D01).

// spec: AI-002 [M4]
// Szenario `output-limit` bei einer Schema- und einer Freitext-Aufgabe → `Error: Output limit reached`,
// `Error Code = max_tokens`, `HTTP Status = 200`, `Output Tokens` = gesendetes `max_tokens`, `Cost` > 0, Ergebnisspalten
// leer. Adapterseite: adapter_q00.rs `anthropic_output_limit_is_its_own_class_without_repair`.
#[test]
#[ignore = "wartet auf M4-K10: Statuswert Error: Output limit reached und Protokollzeile mit Error Code/Output Tokens"]
fn anthropic_output_limit_status_in_log() {
    unimplemented!("M4-K10");
}

// spec: AI-002 [M4]
// Szenario `reasoning=40` → Protokollspalte `Reasoning Tokens = 40`, `Output Tokens` = Antwort + 40, `Token Cost` aus
// `Output Tokens` einmal berechnet.
#[test]
#[ignore = "wartet auf M4-K10: Protokollspalte Reasoning Tokens in ai_requests"]
fn anthropic_reasoning_tokens_column() {
    unimplemented!("M4-K10");
}

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

// spec: AI-003 [M4]
// Szenario `output-limit` → `Error: Output limit reached`, `Error Code = max_output_tokens`, genau 1 Request; Szenario
// `reasoning=27` → Protokollspalte `Reasoning Tokens = 27`. Adapterseite: adapter_q00.rs.
#[test]
#[ignore = "wartet auf M4-K10: Statuswert Error: Output limit reached und Spalte Reasoning Tokens"]
fn responses_output_limit_status_and_reasoning_column() {
    unimplemented!("M4-K10");
}

// spec: AI-005 [M4]
// Szenario `output-limit` → `Error: Output limit reached`, `Error Code = length`, genau 1 Request.
#[test]
#[ignore = "wartet auf M4-K10: Statuswert Error: Output limit reached"]
fn openrouter_output_limit_status() {
    unimplemented!("M4-K10");
}

// spec: AI-005 [M4]
// Ist-Kosten im Protokoll: `Cost (USD)` = `usage.cost` (µUSD), `Price Source = reported`, `Computed Cost (USD)`,
// `cost_mismatch` (Szenario `cost-mismatch`), Budgetverbrauch mit dem gemeldeten Betrag; `no-cost`/`byok` →
// `Price Source = api`; Bildrequest → `Fixed Fees = 0.003000`, `Token Cost = Cost − Fixed Fees`. Die Adapterseite
// (`AdapterReply::reported_cost`, `ReportedCost::usable`) prüft adapter_q00.rs.
#[test]
#[ignore = "wartet auf M4-K10: Verbuchung reported, Computed Cost, cost_mismatch und Aufteilung Fixed Fees"]
fn openrouter_reported_cost_in_log_and_budget() {
    unimplemented!("M4-K10");
}

// spec: AI-005 [M4]
// Langkontext ohne gemeldete Kosten (`no-cost`) auf `M-LC` im Protokoll (2.400 bzw. 1.199 µUSD) und `M-IR` in der
// Reservierung (10.000 µUSD bei `max_tokens = 1.000`). Preisseite: `OpenRouterModel::rates_for`,
// `reserve_output_pico` (adapter_q00.rs).
#[test]
#[ignore = "wartet auf M4-K10: Langkontext über overrides und internal_reasoning in Kostenrechnung und Reservierung"]
fn openrouter_long_context_and_internal_reasoning_in_cost_core() {
    unimplemented!("M4-K10");
}

// spec: AI-005 [M4]
// Die Modellauswahl zeigt für `M-LC` `Long Context From 1000` (Wert: `OpenRouterModel::long_context_from`).
#[test]
#[ignore = "wartet auf M4-U01: Spalte Long Context From in der Modellauswahl"]
fn openrouter_long_context_from_column() {
    unimplemented!("M4-U01");
}

// spec: AI-006 [M4]
// Ollama-Szenario `output-limit` → `Error: Output limit reached`, `Error Code = length`, genau 1 Request, Kosten 0.
#[test]
#[ignore = "wartet auf M4-K10: Statuswert Error: Output limit reached"]
fn ollama_output_limit_status() {
    unimplemented!("M4-K10");
}

// spec: AI-001 [M4]
// Volltextsuche nach `MOCK-REASONING-TEXT`, `MOCK-ENCRYPTED` und `MOCK-SIGNATURE` in Crawl-Datei, `ai_requests`,
// `ai_cache` und Anwendungslogs nach einem Lauf mit Szenario `reasoning=40` → 0 Treffer (Ergebnisseite: contract_q00.rs).
#[test]
#[ignore = "wartet auf M4-D01: KI-Lauf über die Crawl-Datei mit Protokoll und Cache"]
fn reasoning_content_never_reaches_crawl_file_or_logs() {
    unimplemented!("M4-D01");
}

// spec: AI-001 [M4]
// Anthropic-Fehlerantwort ohne Header, aber mit `request_id` im Körper → `Provider Request ID` = dieser Wert im
// Protokoll. Die Ermittlung steht in `adapters::anthropic::request_id` (adapter_q00.rs); der Fehlerweg
// (`AttemptFailure`/`FinalError`) trägt bisher keine Request-ID.
#[test]
#[ignore = "wartet auf M4-K10: Request-ID im Fehlerweg (AttemptFailure/FinalError, Dateien von K06)"]
fn anthropic_error_request_id_in_log() {
    unimplemented!("M4-K10");
}
