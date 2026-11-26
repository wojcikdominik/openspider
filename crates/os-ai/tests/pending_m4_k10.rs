//! Platzhalter für Kriterien der Nachträge 24.11.2026 (Q00) zu AI-012, AI-018 und AI-020, die erst spätere Pakete
//! prüfbar machen (M4-K10). Der Kern ist umgesetzt und in `cost_q00.rs`, `policy_storage.rs` und `status_values_q00.rs`
//! getestet; hier fehlen die Oberfläche (M4-U01) und der Crawl-Lauf (M4-D01).

// spec: AI-012 [M4]
// Der Dialog „Kostenvoranschlag“ zeigt die Spalte `Reasoning` zwischen `Model` und `Items` (Kern:
// `RequestTaskEstimate::reasoning`, geprüft in cost_q00.rs `estimate_shows_the_effective_level`).
#[test]
#[ignore = "wartet auf M4-U01: Spalte Reasoning im Dialog Kostenvoranschlag"]
fn estimate_dialog_shows_reasoning_column() {
    unimplemented!("M4-U01");
}

// spec: AI-018 [M4]
// Die Datenflusstabelle der Oberfläche zeigt `Stored by Provider` nach `Training Use (per provider policy)`, der
// CSV-Export der Oberfläche enthält die Spalte (Kern: `DataFlowRow::stored_by_provider`, `DataFlowOverview::to_csv`).
#[test]
#[ignore = "wartet auf M4-U01: Spalte Stored by Provider in Dialog und Übersicht KI-Datenfluss"]
fn dataflow_dialog_shows_stored_by_provider() {
    unimplemented!("M4-U01");
}

// spec: AI-020 [M4]
// Seitentyp-Frage im Ersatzweg als echter KI-Lauf auf der Fixture-Site mit Szenario `output-limit` → 22 × `Error:
// Output limit reached` in der Ergebnisspalte, 3 × `Skipped (scope)`, Mock-Log 22 Requests, Schutzschalter greift nicht
// (Kern als Schleife über den Limiter: status_values_q00.rs `fallback_run_with_output_limit_never_trips_the_breaker`).
#[test]
#[ignore = "wartet auf M4-D01: Ersatzweg im KI-Lauf mit Statusspalte"]
fn crawl_run_fallback_output_limit() {
    unimplemented!("M4-D01");
}
