# M4-K02 – Unabhängige Abnahmetests (OpenAI Responses AI-003, OpenAI Decisions AI-004)

Testagent, unabhängig vom Bauer. Grundlage nur `docs/spec/08-ai.md` (AI-003, AI-004; dazu AI-001, AI-017, AI-020, AI-021,
AI-012/AI-014 soweit verwiesen, Szenariotabelle des ai-mock). Bauer-Bericht, Bauer-Tests und Fragment `k02.json` wurden nicht
gelesen; Produktionscode nur für die öffentliche Schnittstelle. Produktionscode und ai-mock unverändert.

Hinweis zur Benennung: Der Auftrag nannte „Decisions (AI-003)“ und „Responses (AI-004)“. In der Spec ist es umgekehrt:
**AI-003 = Responses, AI-004 = Decisions**. Die Tests verwenden die Spec-IDs.

## Dateien

- `crates/os-ai/tests/acceptance_k02_responses.rs` – AI-003 (18 Tests)
- `crates/os-ai/tests/acceptance_k02_decisions.rs` – AI-004 (24 Tests)
- `crates/os-ai/tests/acceptance_k02_support/mod.rs` – ai-mock mit eigenem Fragment, Client mit `*.mock.test → 127.0.0.1`,
  kleiner lokaler HTTP-Testserver (für Antwortformen, die der Mock nicht liefert), eigene Base64-Kodierung
- `tests/fixtures/ai/k02-acceptance/mock-answers.d/k02-acceptance.json` – eigene Mock-Antworten (Level-Prüfwerte,
  Confidence 0,70/0,80, Refusal nur Frage 2, Zusatzfelder/unbekannter Typ, Seitentyp `shop`)
- `tests/fixtures/ai/k02-acceptance/decisions-request.schema.json` – Request-Aufbau laut AI-004 als JSON-Schema
  (die in der Spec genannte Datei `tests/fixtures/ai/decisions-request.schema.json` existiert nicht, s. u.)

## Ergebnis des letzten Laufs (Mac mini)

- `cargo fmt --all --check`: grün
- `cargo clippy --locked -p os-ai --all-targets -- -D warnings`: grün
- `cargo test --locked -p os-ai --no-fail-fast`: grün, gesamt 266 bestanden, 0 fehlgeschlagen, 30 ignoriert (fremde
  Platzhalter); davon diese Abnahme: **42 bestanden, 0 rot** (Responses 18, Decisions 24)

## AI-003 Adapter OpenAI Responses

| Kriterium | Test(s) | Ergebnis |
|---|---|---|
| K1 Schema-Aufgaben senden `text.format.type = "json_schema"` mit `strict: true` | `k1_schema_task_sends_text_format_json_schema_strict` | grün |
| K1 Ergebnis aus Ausgabetext geparst und gegen Schema validiert (AI-021) | `k1_schema_violation_is_rejected_after_one_repair`, `k1_enum_value_outside_schema_is_invalid_output`, `k1_malformed_json_once_then_ok_is_valid_with_two_requests`, `contract_ok_true_via_common_interface` (AI-001) | grün |
| K2 Ausgabeteil `refusal` → Refusal-Kennzeichen, Text im Protokoll, nicht als Antwort | `k2_refusal_part_sets_flag_and_text_goes_to_log_not_answer`, `k2_refusal_via_common_interface_has_no_content` | grün |
| K3 `input_tokens`, `output_tokens`, `input_tokens_details.cached_tokens` übertragen | `k3_usage_fields_are_transferred`, `k3_missing_cached_tokens_is_null_not_zero` (AI-001: `null` statt 0) | grün |
| K4 429 `insufficient_quota` → `Quota exceeded` ohne Wiederholung, Grund in `Error Code` | `k4_insufficient_quota_is_final_without_retry` | grün |
| K4 alle offenen OpenAI-Einträge (Responses und Decisions) ebenfalls `Quota exceeded`, ohne weiteren Request | `k4_quota_stop_applies_to_all_open_openai_entries_without_requests` | grün |
| K4 ältere Form `error.code = "insufficient_quota"` gleich behandelt | `k4_legacy_error_code_insufficient_quota_is_treated_the_same` (lokaler Testserver) | grün |
| K4 429 ohne diesen Typ → Backoff nach INTEG-005 | `k4_plain_429_is_retried_with_backoff_and_succeeds` (`429x2` → 3 Requests, `Retries = 2`, `Success`) | grün |
| K5 Bilder nur als `data:<mime>;base64,…`, nie als HTTP-URL | `k5_images_are_sent_only_as_data_urls` | grün |
| Beschreibung: Bearer-Token, optionale Organisation/Projekt-Header | `description_bearer_token_and_optional_org_project_headers` | grün |
| Eingaben: Basis-URL (EU, AI-019) | `description_base_url_selects_eu_host` | grün |
| AI-020: fehlender Schlüssel → `Error: Missing API key`, 0 Requests | `missing_api_key_sends_nothing` | grün |
| AI-001 Fähigkeitszeile Responses (Text, Structured Output, Vision, Batch) | `capabilities_of_responses_match_matrix` | grün |

## AI-004 Adapter OpenAI Decisions

| Kriterium | Test(s) | Ergebnis |
|---|---|---|
| K1 Request mit 3 Fragen (predicate, choice, score) entspricht dem Aufbau (JSON-Schema) | `k1_request_with_three_question_types_matches_structure`, `k1_request_with_image_input_matches_structure` | grün |
| K2 `score` übernommen, `<Q> Level` = Label von `round(score)`; Prüfwerte 1,5 → c, 1,49 → b, 1,0 bei [0,45; 0,10; 0,45] → b | `k2_score_level_rounding_check_values`, `k2_level_index_rounds_half_up` | grün |
| K3 Refusal-Eintrag wirkt nur auf seine Frage | `k3_refusal_of_question_two_affects_only_that_question` (eigenes Fragment, Frage 2), `k3_mock_refusal_scenario_affects_only_one_question` | grün |
| K4 fehlende Frage → `Error: Missing answer`, übrige übernommen | `k4_missing_answer_only_for_absent_question` | grün |
| K5 Kosten = Eingabe-Tokens × Eingabepreis, Ausgabe trägt 0 bei | `k5_cost_is_input_tokens_times_input_price_only` (1 Mio. Eingabe → 0,100000 USD), `k5_cost_ignores_cache_split`, `k5_mock_reported_output_tokens_cost_nothing` | grün |
| K6 Confidence = höchste Einzelwahrscheinlichkeit, roher Wert erhalten, `confidence_mismatch` | `k6_confidence_is_max_probability_and_mismatch_is_logged`, `k6_matching_confidence_is_not_flagged`, `k6_score_confidence_is_max_probability` | grün |
| K7 ohne `usage` → Tokenfelder `null`, Kosten = Schätzung, `Price Source = estimate` | `k7_missing_usage_gives_null_tokens_and_estimated_cost` | grün (Budgetteil s. „nicht prüfbar“) |
| K8 unbekannte Zusatzfelder ignoriert; unbekannter `type` → `Error: Invalid output` nur für diese Frage | `k8_unknown_fields_ignored_unknown_type_invalid_for_that_question`, `k8_unknown_top_level_fields_are_ignored` | grün |
| K9 Bilder ohne Data-URL werden geladen und kodiert | `k9_url_image_is_loaded_and_sent_as_data_url` | grün |
| K9 Bilder über 20 MB → `Skipped (image too large)` | `k9_image_over_20_mb_is_skipped_without_request` (Bytes), `k9_url_image_over_20_mb_is_skipped_without_request` (URL), Gegenprobe `k9_image_below_20_mb_is_sent` | grün |
| Eingaben: höchstens 10 Fragen je Request, einstellbar 1–10 | `config_at_most_ten_questions_per_request` (23 → 10/10/3), `config_max_questions_is_adjustable_between_one_and_ten` | grün |
| AI-017: Timeout gilt für alle Fragen eines Requests | `timeout_applies_to_all_questions_of_the_request` | grün |
| AI-001 Fähigkeitszeile Decisions | `capabilities_of_decisions_match_matrix` | grün |

## Befunde (rot)

Keine. Alle 42 Abnahmetests sind grün.

## Hier nicht prüfbar (Oberfläche, Datenhaltung oder ganzer Lauf nötig)

- AI-004 K7, Teil „der Betrag geht in den Budgetverbrauch (AI-013) ein“: braucht den `BudgetLedger`-Lauf; geprüft nur,
  dass die Protokollzeile den Schätzbetrag (> 0) trägt.
- AI-004 K6, Teil „roher Wert 0,70 in `ai_answers`“: Speicherung gehört zur Decision-Engine/Crawl-Datei; geprüft nur, dass
  der Adapter den rohen Wert (`confidence_raw`) liefert und das Protokoll `confidence_mismatch` trägt.
- AI-004 K2, Spalte `<Q> Level` selbst (AI-036, Tab/Export); geprüft ist der vom Adapter gelieferte Level.
- AI-003 K2/K4, Persistenz in `ai_requests` (AI-014): geprüft ist die erzeugte `RequestRecord`-Zeile, nicht das Schreiben.
- AI-003 K4 „alle offenen Einträge“ in einem echten Lauf mit mehreren gleichzeitigen Anfragen; geprüft sequenziell über ein
  gemeinsames Konto.
- Auswertung der OpenAI-Rate-Limit-Header `x-ratelimit-remaining-*` (AI-017): kein Kriterium von AI-003/AI-004, der Mock
  sendet sie nicht; nicht geprüft.
- Batch (AI-016) gehört nicht zu diesem Paket.

## Auslegungsfragen

1. **Inhalt der Spalte `Error Code` bei `insufficient_quota`:** AI-003 sagt, der Grund aus `error.code` (z. B.
   `credit_balance_exhausted`) wird in `Error Code` gespeichert; AI-014 nennt als Beispiel `insufficient_quota/credit_balance_exhausted`.
   Der Test prüft nur „enthält `credit_balance_exhausted`“. Der Adapter speichert `credit_balance_exhausted` ohne Präfix.
2. **20 MB:** dezimal (20.000.000 Byte) oder binär (20 × 1024² Byte)? Getestet nur eindeutig darunter (19.000.000 Byte →
   gesendet) und darüber (21 × 1024² Byte → übersprungen); der Bereich dazwischen nicht.
3. **Schätzbetrag ohne `usage` (AI-012):** fester Anteil für Anweisungen, Einbeziehung der Fragentexte und
   Korrekturfaktor sind nicht exakt festgelegt. Getestet: Schätzung ≥ ⌈Zeichen der Eingabe / 3,5⌉ und
   Kosten = Schätzung × Eingabepreis.
4. **Cache-Tokens bei Decisions:** „keine Kosten für … Cache-Lesen“ habe ich als „alle Eingabe-Tokens zum Eingabepreis“
   gelesen (Kosten = Eingabe-Tokens × Eingabepreis). Das gilt nur, wenn der Aufrufer die Abrechnungssätze
   `decisions_billing_rates` verwendet. Mit den rohen Tabellensätzen (Cache-Lesen 0) wären gecachte Tokens kostenlos.
   Das sollte die Decision-Engine (D01) beachten.
5. **Fähigkeitszeile Decisions „Decisions (Text und Bild)“:** offen, ob die Matrixspalten `Text`/`Vision` dafür gesetzt
   sein sollen. Getestet wird nur `Decisions = ja` und keine fremden Fähigkeiten (Structured Output, Embeddings, Batch …).

## Abweichungen des ai-mock von der Spec

- **Szenario `refusal` bei Decisions** verweigert die **erste** Frage; die Testfixture der Spec nennt „`refusal` (nur Frage 2)“.
  Für den Prüffall Frage 2 wurde ein eigenes Fragment verwendet.
- **`tests/fixtures/ai/decisions-request.schema.json` fehlt**, und der Mock validiert Decisions-Requests nicht gegen ein
  Schema (AI-004 K1: „Mock validiert gegen JSON-Schema …“). Ersatz: eigenes Schema unter
  `tests/fixtures/ai/k02-acceptance/` und Validierung im Test.
- Ältere Fehlerform `error.code = "insufficient_quota"` ist kein Mock-Szenario. Geprüft wurde sie über den lokalen Testserver.
- Der Mock sendet keine Rate-Limit-Header (bekannt).
- Request-Bodies mit Bildern nahe 20 MB nimmt der Mock vermutlich nicht an (axum-Standardgrenze für Bodies). Der Fall
  19.000.000 Byte wurde deshalb gegen den lokalen Testserver geprüft und nicht gegen den Mock.
- Zusatzszenario `missing-usage` (nicht in der Spec-Tabelle, im Fixture-README dokumentiert) wurde für AI-004 K7 genutzt.
