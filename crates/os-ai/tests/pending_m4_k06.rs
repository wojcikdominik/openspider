//! Platzhalter M4-K06 (Plan M4 E4): Kriterien aus AI-015, AI-017, AI-020 (K4-15) und AI-021, die erst mit späteren
//! Paketen auf Lauf-, Adapter-, Protokoll- oder Oberflächenebene prüfbar sind. Die Bibliotheksteile sind in
//! `cache_*.rs`, `limits_*.rs` und `schema_*.rs` geprüft. Aufgelöst durch M4-N01.

// spec: AI-015
// Erster Lauf der Seitentyp-Frage auf der Fixture-Site → 22 Requests (Mock-Log); zweiter Lauf → 0 Requests, 22 ×
// `Success (cached)`, 3 × `Skipped (scope)`, Kosten 0. Braucht Decision-Ausführung (D01) und Kostenprotokoll (K05).
#[test]
#[ignore = "wartet auf M4-D01"]
fn cache_second_run_of_page_type_question_sends_no_request() {
    panic!("Lauf der Seitentyp-Frage über os-ai::decisions gegen ai-mock; Bibliotheksteil: cache_results.rs");
}

// spec: AI-015
// Nur Schwellwerte geändert → 0 Requests; `instructions` geändert (neue Version nach AI-031) → 22 Requests;
// Modellwechsel → 22, Rückwechsel → 0. Braucht Fragenversionierung und Ausführung (D01).
#[test]
#[ignore = "wartet auf M4-D01"]
fn cache_threshold_change_is_free_and_new_version_or_model_requests_again() {
    panic!("Versionierung AI-031 und Lauf gegen ai-mock; Bibliotheksteil: cache_results.rs");
}

// spec: AI-015
// Parametrisierte Systemfrage AI-057 (lokale Vorprüfung aus), zwei Seiten mit gleichem Text und `lang="de"` bzw.
// `lang="en"` → 2 Cache-Einträge; erneuter Lauf → 0 Requests.
#[test]
#[ignore = "wartet auf M4-D03"]
fn cache_parameterised_language_question_keeps_one_entry_per_code() {
    panic!("AI-057 gehört M4-D03 (Release 1.2); Bibliotheksteil: cache_results.rs");
}

// spec: AI-015
// Recrawl `site-changed/` (3 von 22 Seiten in S mit geändertem Text) mit übernommenem Cache aus
// `tests/fixtures/ai/crawls/site.ospider` → genau 3 Requests; die übernommene Datei bleibt byte-identisch. Beide
// Fixtures fehlen noch (Testfixture laut AI-015).
#[test]
#[ignore = "wartet auf M4-D01"]
fn cache_recrawl_with_imported_cache_requests_only_changed_pages() {
    panic!("Fixtures site-changed/ und crawls/site.ospider fehlen; Bibliotheksteil: cache_results.rs, os-store ai_cache.rs");
}

// spec: AI-017
// Rate-Limit-Header über HTTP: ai-mock liefert bisher keine `anthropic-ratelimit-*`/`x-ratelimit-*`-Header; die
// Auswertung im Adapter (Aufruf von `Limiter::observe_headers`) gehört K01 (Anthropic) und K02 (OpenAI).
#[test]
#[ignore = "wartet auf M4-K01"]
fn limits_provider_headers_over_http_pause_the_adapter() {
    panic!("Header-Szenario im Mock und Adapter-Anbindung; Bibliotheksteil: limits_rates.rs mit Header-Fixture");
}

// spec: AI-017
// spec: INTEG-005
// Szenarien `429x2`, `retry-after-7`, `500-always`, `hang` über die echten Adapter gegen ai-mock mit
// Mock-Zeitstempeln (`GET /__mock/log`). `os-ai` hat `os-testsite` nicht als Dev-Abhängigkeit (Manifest außerhalb von
// K06); die Adapter-Pakete prüfen das über `Limiter::run_provider`.
#[test]
#[ignore = "wartet auf M4-K01"]
fn limits_mock_scenarios_through_real_adapters() {
    panic!("Adapter K01/K02 und os-testsite als Dev-Abhängigkeit; Bibliotheksteil: limits_retry.rs, limits_breaker.rs");
}

// spec: AI-017
// Ein Request mit mehreren Decision-Fragen: ein Timeout gilt für alle Fragen dieses Requests (Statusspalten aller
// Fragen `Error: Timeout nach 30 s`).
#[test]
#[ignore = "wartet auf M4-D01"]
fn limits_multi_question_request_shares_one_timeout() {
    panic!("Verteilung des Endstatus auf die Fragen in D01; Bibliotheksteil: limits_retry.rs");
}

// spec: AI-017
// Hinweis „<Anbieter> pausiert bis <Uhrzeit> (Rate-Limit)“ in der API-Fortschrittsanzeige (INTEG-007).
#[test]
#[ignore = "wartet auf M4-U01"]
fn limits_pause_notice_in_progress_display() {
    panic!("Anzeige in der Oberfläche; Bibliotheksteil: Limiter::pauses, limits_rates.rs");
}

// spec: AI-020
// Anbieter nicht erreichbar → Protokollzeile (AI-014) mit `HTTP Status` leer, `Error Code = transport/connect`,
// `Retries = 5`, `Cost = 0`.
#[test]
#[ignore = "wartet auf M4-K05"]
fn connection_failed_log_row() {
    panic!("Zeile in ai_requests schreibt K05 aus Execution; Bibliotheksteil: limits_network.rs");
}

// spec: AI-021
// `malformed-json` einmalig bzw. `schema-violation` dauerhaft über ai-mock: Protokoll zeigt 2 Requests für den
// Eintrag bzw. den Validator-Fehler.
#[test]
#[ignore = "wartet auf M4-K05"]
fn schema_repair_requests_in_the_request_log() {
    panic!("Protokollzeilen je Request schreibt K05 aus StructuredRun; Bibliotheksteil: schema_validation.rs");
}

// spec: AI-021
// CI-Test „strikt verträglich“ für alle automatisch erzeugten Fallback-Schemas (AI-034) der Fragenbibliothek.
#[test]
#[ignore = "wartet auf M4-D01"]
fn schema_generated_fallback_schemas_are_strict() {
    panic!("Fallback-Schemas erzeugt D01; Prüfer: os_ai::schema::strict_violations");
}
