//! Platzhalter M4-K02 (Plan M4 E4): Kriterien aus AI-003, AI-004 und AI-017, die erst mit späteren Paketen bzw. einer
//! Mock-/Fixture-Ergänzung vollständig prüfbar sind. Die Adapterteile sind in `adapter_openai_responses.rs` und
//! `adapter_openai_decisions.rs` geprüft. Aufgelöst durch M4-N01.

// spec: AI-004
// „Mock validiert gegen JSON-Schema `tests/fixtures/ai/decisions-request.schema.json`“: Die Fixture fehlt und ai-mock
// prüft den Request-Aufbau nicht (nur `questions.length ≤ 10`). Der Adaptertest prüft den Aufbau gegen ein
// gleichwertiges Schema im Test (`adapter_openai_decisions::three_question_request_matches_the_documented_shape`).
#[test]
#[ignore = "wartet auf M4-N01: Fixture decisions-request.schema.json und Prüfung im ai-mock"]
fn mock_validates_decisions_requests_against_the_request_schema_fixture() {
    panic!(
        "Fixture tests/fixtures/ai/decisions-request.schema.json und Validierung im ai-mock fehlen"
    );
}

// spec: AI-003
// Lauf-Ebene: Kontingent (`insufficient-quota`, gleichzeitige Anfragen = 1) → genau 1 Request, alle offenen
// OpenAI-Einträge des Laufs (Responses und Decisions) mit Status `Quota exceeded` (AI-020 Fehlerfall-Matrix: 22 ×).
// Adapterebene (gemeinsamer Kontostand, 0 weitere Requests) ist geprüft in
// `adapter_openai_responses::insufficient_quota_stops_responses_and_decisions_without_further_requests`.
#[test]
#[ignore = "wartet auf M4-D01: Lauf über die Fixture-Site mit Statusspalten"]
fn quota_run_marks_every_open_openai_entry_quota_exceeded() {
    panic!("Lauf der Seitentyp-Frage über os-ai::decisions (D01) gegen ai-mock");
}

// spec: AI-017
// Rate-Limit-Header über ai-mock: Der Mock sendet keine `x-ratelimit-*`-Header (Lücke aus T01, Nachtrag 24.11.2026
// Punkt 7). Die Auswertung im Adapter ist gegen eine eigene Route geprüft
// (`adapter_openai_responses::rate_limit_headers_are_handed_to_the_limiter`).
#[test]
#[ignore = "wartet auf M4-N01: ai-mock mit x-ratelimit-Headern"]
fn openai_rate_limit_headers_from_ai_mock_pause_the_next_request() {
    panic!("ai-mock /openai/* mit x-ratelimit-remaining-requests/-tokens und Reset");
}
