//! Platzhalter M4-K05 (Plan M4 E4): Kriterien von AI-012 – AI-014, die einen Crawl-Lauf mit KI-Ausführung, den
//! Dialog „Kostenvoranschlag“ oder das Statusleistenfeld brauchen. Die Bibliotheksteile sind in den `cost_*`-Tests
//! geprüft; hier steht je Kriterium der Ende-zu-Ende-Test gegen ai-mock, den das genannte Paket ausfüllt
//! (Auflösung durch M4-N01).

// spec: AI-012
#[test]
#[ignore = "wartet auf M4-D01: Lauf der Seitentyp-Frage auf tests/fixtures/ai/site/ gegen ai-mock (Eingabeaufbereitung AI-032, Ausführung AI-033)"]
fn estimate_for_the_page_type_question_is_within_25_then_5_percent_of_logged_cost() {
    // Voranschlag `Items = 22`; erwartete Kosten ±25 % der protokollierten Ist-Kosten der 22 Requests,
    // nach Kalibrierung (20 Requests) ±5 %. Bibliotheksteil: cost_estimate::estimate_for_the_fixture_site_scope_and_calibration.
    unimplemented!("M4-D01");
}

// spec: AI-012
#[test]
#[ignore = "wartet auf M4-U01: Dialog Kostenvoranschlag und Status Pending (confirmation) im Lauf (mit M4-D01)"]
fn no_ai_request_before_start_when_confirmation_is_required() {
    // Testprofil Bestätigungsschwelle 0,0001 USD → KI-Einträge `Pending (confirmation)`, Mock-Zähler 0, Crawl läuft;
    // nach „Starten“ werden die Requests gesendet. Entscheidungsregel: cost_estimate::confirmation_threshold_and_half_of_the_remaining_budget.
    unimplemented!("M4-U01");
}

// spec: AI-012
#[test]
#[ignore = "wartet auf M4-D01: Voranschlag im Lauf gegen das Mock-Log (Stichprobenzählung AI-002 mit M4-K01)"]
fn the_estimate_sends_no_content_without_the_sample_option() {
    // Mock-Log: 0 Requests während der Schätzung ohne Option „Token-Zählung über Anbieter-API für Stichprobe“.
    unimplemented!("M4-D01");
}

// spec: AI-012
#[test]
#[ignore = "wartet auf M4-K01: OpenRouter-Bildrequest gegen ai-mock (Gebühren in Voranschlag und Protokoll)"]
fn openrouter_image_fees_appear_in_estimate_and_logged_cost() {
    // 3 Bilder über mock/vision → Voranschlag 9.000 µUSD Gebühren + Tokenkosten; Ist-Kosten im Protokoll mit
    // denselben Gebühren. Bibliotheksteil: cost_estimate::image_task_estimate_contains_request_and_image_fees.
    unimplemented!("M4-K01");
}

// spec: AI-013
#[test]
#[ignore = "wartet auf M4-D01: Lauf gegen ai-mock mit Budget 0,010 USD und 0,004 USD je Request"]
fn budget_limits_the_mock_counter_to_two_requests() {
    // Mock-Zähler = 2, übrige Einträge `Budget exceeded`, Crawl läuft weiter.
    // Bibliotheksteil: cost_budget::budget_0_010_with_0_004_per_request_sends_exactly_two.
    unimplemented!("M4-D01");
}

// spec: AI-013
#[test]
#[ignore = "wartet auf M4-D01: 4 parallele Worker gegen ai-mock (200 Einträge)"]
fn four_workers_against_ai_mock_stay_within_the_budget() {
    // Summe der Ist-Kosten in ai_requests ≤ Crawl-Budget. Bibliotheksteil: cost_budget::four_workers_200_entries_never_exceed_the_budget.
    unimplemented!("M4-D01");
}

// spec: AI-013
#[test]
#[ignore = "wartet auf M4-D01: Szenario max-tokens von ai-mock (Antwort über der Reservierung) im Lauf"]
fn response_above_its_reservation_stops_the_run_in_ai_mock() {
    // over_reservation = true im Protokoll, Mehrbetrag verbucht, danach kein weiterer Request (Mock-Zähler).
    unimplemented!("M4-D01");
}

// spec: AI-013
#[test]
#[ignore = "wartet auf M4-U01: Schaltfläche „Budget erhöhen und fortsetzen“ (mit M4-D01)"]
fn raising_the_budget_resends_only_budget_exceeded_entries() {
    // Nur Einträge mit `Budget exceeded` werden erneut angefragt; erfolgreiche nicht (Mock-Zähler).
    unimplemented!("M4-U01");
}

// spec: AI-013
#[test]
#[ignore = "wartet auf M4-K03: Ollama-Adapter; Lauf mit Budget 0 gegen ai-mock (mit M4-D01, Cache M4-K06)"]
fn budget_zero_runs_only_ollama_and_cache_hits_against_ai_mock() {
    // Bibliotheksteil: cost_budget::budget_zero_runs_only_ollama_and_cache_hits.
    unimplemented!("M4-K03");
}

// spec: AI-013
#[test]
#[ignore = "wartet auf M4-U01: Statusleistenfeld Kosten mit Warnschwelle und Tooltip KI / DataForSEO / Reservierungen"]
fn status_bar_shows_budget_warning_and_split() {
    // `Kosten: 3,2100 / 5,0000 USD`; Warnung ab 80 %. Datenquelle: BudgetSnapshot (warning, areas, reserved).
    unimplemented!("M4-U01");
}

// spec: AI-014
#[test]
#[ignore = "wartet auf M4-K06: Cache-Treffer im Lauf ohne HTTP-Request (Mock-Log unverändert)"]
fn cache_hit_rows_do_not_touch_the_mock_log() {
    // Bibliotheksteil: cost_log::cache_hits_log_a_zero_cost_row_without_tokens.
    unimplemented!("M4-K06");
}

// spec: AI-014
#[test]
#[ignore = "wartet auf M4-K06: Wiederholungen (Szenario 429x2) im Lauf gegen ai-mock"]
fn scenario_429x2_logs_one_row_with_retries_2() {
    // Bibliotheksteil: cost_log::retries_are_one_row_and_failed_attempts_without_usage_cost_nothing.
    unimplemented!("M4-K06");
}

// spec: AI-014
#[test]
#[ignore = "wartet auf M4-U01: Tooltip des Statusleistenfelds Kosten gegen die Summenzeile des KI-Protokolls"]
fn log_sum_and_status_bar_tooltip_agree_to_one_micro_usd() {
    // Bibliotheksteil: cost_log::the_log_holds_no_page_content_and_its_sum_matches_the_budget_display.
    unimplemented!("M4-U01");
}
