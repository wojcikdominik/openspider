# M4-K10 – Bericht der unabhängigen Abnahme

Zweig `ai/k10-tests` auf dem Bauer-Endstand `9bf50749`. Erwartungen nur aus `docs/spec/08-ai.md` (Nachträge „24.11.2026, Q00“ an AI-002, AI-003, AI-005, AI-006, AI-012 … AI-015, AI-017, AI-018, AI-020, AI-021; Nachträge „25.11.2026“ an AI-010 und AI-011) und dem Zuschnitt in `docs/plan/M4.md` Abschnitt 7 bzw. `M4-packages.json` (M4-K10). Bauer-Bericht, Bauer-Tests und Bauer-Fixtures wurden nicht gelesen. Gelesen wurden die öffentlichen Schnittstellen (Signaturen, Doc-Kommentare) der Module `cost`, `cache`, `schema`, `limits`, `policy`, `provider`, `os-model::ai`, `os-store::ai`, die Migration 0006, `resources/ai/*` und – um die Zusammensetzung zu bauen – die öffentlichen Signaturen der Adapter. Alle Tests laufen gegen den ai-mock (`os-testsite`) bzw. eigene Fixtures und einen eigenen Testserver auf `127.0.0.1`; keine echten Anbieter, keine Schlüssel.

Weil die Ausführung über die Aufgabenwarteschlange erst mit M4-D01 kommt, prüfen die Tests die Zusammensetzung aus Adapter → `limits::Limiter` → `schema::run_with_repair` → `cost::log` → `os_store::ai::requests` bzw. `cache::ResultCache`.

## Dateien

- `crates/os-ai/tests/acceptance_k10_support/mod.rs` – Hilfen (Mock, Adapter, Limiter, eigener Preis-Loader, Crawl-Datei, Testserver)
- `crates/os-ai/tests/acceptance_k10_status.rs` – AI-020, AI-021, AI-017 (Schutzschalter), AI-002/003/005/006 (Ausgabegrenze)
- `crates/os-ai/tests/acceptance_k10_cache.rs` – AI-015
- `crates/os-ai/tests/acceptance_k10_cost.rs` – AI-012, AI-013, AI-010
- `crates/os-ai/tests/acceptance_k10_log.rs` – AI-014, AI-005 (Ist-Kosten)
- `crates/os-ai/tests/acceptance_k10_limits.rs` – AI-017 (Go-Dauern)
- `crates/os-ai/tests/acceptance_k10_policy.rs` – AI-018
- `crates/os-ai/tests/acceptance_k10_defaults.rs` – AI-011 (bestätigte Standardmodelle, `max_tokens`-Regel)
- `crates/os-store/tests/acceptance_k10_requests.rs` – AI-014 (Spalten, Speicher, Migration)
- `tests/fixtures/ai/k10-acceptance/` – Go-Dauern, `provider-limits` mit G = 150, Richtlinien mit `storage` (gültig und 5 Negativfälle)

## Prüfpunkte je Spec-ID

| Spec-ID | Kriterium (Kurzform) | Test | Ergebnis |
|---|---|---|---|
| AI-002 | `output-limit` mit/ohne Schema → `Error: Output limit reached`, `Error Code = max_tokens`, HTTP 200, 1 Request, `Output Tokens` = `max_tokens`, `Cost` > 0, kein Ergebnis | `status::anthropic_output_limit_with_and_without_schema` | grün |
| AI-003 | `output-limit` → `max_output_tokens`, 1 Request; HTTP 200, Tokens, Kosten; `store: false` | `status::responses_output_limit_with_and_without_schema` | grün |
| AI-005 | `output-limit` → `length`, 1 Request; HTTP 200, Tokens, Kosten | `status::openrouter_output_limit_with_and_without_schema` | grün |
| AI-006 | Ollama `output-limit` → `length`, 1 Request, Kosten 0 | `status::ollama_output_limit_with_and_without_schema` | grün |
| AI-020 | Neuer Wert in der Statusmenge, im Filter „Errors“ | `status::status_value_is_in_the_table_and_in_the_errors_filter` | grün |
| AI-020 | 22 × Ersatzweg mit `output-limit` → 22 × Status, 22 Requests, Schutzschalter greift nicht (auch > 10) | `status::output_limit_does_not_trip_the_circuit_breaker` | grün (ohne Fixture-Site, s. u.) |
| AI-020/AI-017 | keine automatische Wiederholung, zählt nicht im Schutzschalter | `status::output_limit_error_class_is_final_and_not_counted` | grün |
| AI-021 | `output-limit` bei Schema → 1 Request; `malformed-json` einmal, dann `ok` → 2 Requests, gültig; `schema-violation` → 2 Requests, `Error: Invalid output`; Reparatur mit gleicher Stufe/`max_tokens` | `status::invalid_output_keeps_its_repair_attempt` | grün |
| AI-021 | `malformed-json`, dann `output-limit` → 2 Requests, `Error: Output limit reached`, gleiche Stufe und `max_tokens` | `status::repair_that_ends_at_the_output_limit` | grün |
| AI-015 | Stufe `off` → `low` neue Requests, Rückwechsel 0 Requests | `cache::level_change_requests_again_and_switching_back_is_free` | grün |
| AI-015 | nur `max_tokens` geändert → 0 Requests | `cache::max_tokens_change_alone_is_free` | grün |
| AI-015 | eigene Schlüssel je Stufe auf Wegen mit Stufe | `cache::every_level_has_its_own_key_on_paths_with_levels` | grün |
| AI-015 | Decisions und Embeddings unabhängig von der Stufe | `cache::decisions_and_embeddings_ignore_the_level`, `cache::decisions_run_is_cached_across_level_settings` | grün |
| AI-015 | Feld `reasoning` mit Wert `null` wörtlich im Schlüssel | `cache::decisions_key_names_reasoning_null_literally` | **rot** (B2-1) |
| AI-015 | `Error: Output limit reached` nicht gecacht, nächster Lauf fragt erneut | `cache::output_limit_is_not_cached_and_requested_again` | grün |
| AI-012 | Decisions-Prüfwert 500 Tokens / 50 µUSD, Mock-Ist 464 (+7,8 %) | `cost::decisions_example_per_question` | grün |
| AI-012 | Bündelung ändert die Tokenschätzung nicht, weniger Requests | `cost::bundling_saves_requests_not_tokens` | grün |
| AI-012 | G aus `provider-limits.json` (Quelle, `verified_at`), andere Datei → andere Schätzung | `cost::question_overhead_comes_from_provider_limits` | grün |
| AI-012 | Ausgabe `off` 512, `medium` 1.024, Decisions 0; Tabelle E; `decision-w` | `cost::output_estimate_by_effective_level` | grün |
| AI-012 | Voranschlag führt die effektive Stufe (`low` auf Modell B) | `cost::estimate_carries_the_effective_level` | grün (Anzeige U01) |
| AI-012 | Ausgabe-Korrekturfaktor nach 20 Requests `reasoning=500`/`medium`, ±5 %, andere Stufe/Aufgabe unberührt | `cost::output_calibration_per_model_task_and_level`, `requests::output_calibration_samples_are_per_level_and_task` | grün |
| AI-012 | Schema-Zuschlag +350 (Anthropic), 80, 80, 0 | `cost::schema_surcharge_per_provider` | grün |
| AI-010 | 0,064 / 0,128 USD, Schwelle über alle Eingabe-Tokens, Ausgabe 0 | `cost::decisions_price_example_from_ai_010` | grün |
| AI-010/AI-004 | dasselbe Beispiel über `decisions_billing_rates` (Protokollweg Decisions) | `cost::decisions_billing_rates_follow_the_ai_010_prices` | **rot** (B1-1) |
| AI-013 | Modell B, `low`, `max_tokens = 2048`, 400 Tokens → 10.740 µUSD | `cost::reservation_uses_max_tokens_from_ai_011` | grün |
| AI-013/AI-005 | `overrides` ab ⌈geschätzt × 1,25⌉ (800/799), `internal_reasoning` 10.000 µUSD | `cost::reservation_with_openrouter_overrides_and_internal_reasoning` | grün |
| AI-013 | Reservierung genau im Budget / 1 µUSD darüber → `Budget exceeded`; Rest wird frei | `cost::reservation_against_the_budget_at_the_edge` | grün |
| AI-013 | `cost-mismatch` über Reservierung → `over_reservation`, gemeldeter Betrag verbucht, danach kein Request (auch nach Neuöffnen) | `cost::reported_cost_over_the_reservation_halts_the_budget` | grün |
| AI-014 | Summenzeile 3000/600/360/0,006000 | `log::sum_row_shows_reasoning_tokens_without_double_cost` | grün |
| AI-014/AI-005 | `reported`, `Computed Cost`, `cost_mismatch`; `no-cost`/`byok` → `api`; Budgetverbrauch enthält gemeldete Beträge | `log::openrouter_reported_costs_are_booked` | grün |
| AI-005 | Schwelle `cost_mismatch` (1 %, 1 µUSD) an und neben der Grenze | `log::cost_mismatch_threshold` | grün |
| AI-005/AI-014 | Bildrequest: `Fixed Fees = 0.003000`, `Token Cost = Cost − Gebühren`; Gebühren > Cost | `log::reported_cost_split_with_fees` | grün |
| AI-005 | M-LC ohne gemeldete Kosten 2.400 / 1.199 µUSD, `Long Context From 1000` | `log::long_context_without_reported_cost` | grün |
| AI-014 | `Reasoning Level`/`Reasoning Tokens` je Weg, leer wo verlangt | `log::reasoning_columns_per_path` | grün |
| AI-014 | Property: `Price Source`-Menge, Stufenmenge, Invariante über Szenarien aller Anbieter | `log::every_row_has_known_price_source_and_level` | grün |
| AI-014 | Spaltennamen und -reihenfolge | `requests::log_columns_follow_the_spec_order` | grün |
| AI-014 | frische Datei, Werte und Leerwerte, CHECKs der Migration, Regeln `Computed Cost`/`cost_mismatch` | `requests::fresh_file_stores_the_new_columns`, `requests::column_rules_are_enforced`, `requests::totals_include_reasoning_tokens_and_computed_cost` | grün |
| AI-014 | bestehende v5-Datei nach Migration (von der Spec nicht verlangt, zusätzlich) | `requests::v5_archive_takes_the_new_columns_after_migration` | grün |
| AI-017 | Go-Dauern (Fixture), ungültige Werte | `limits::go_durations_are_read`, `limits::unreadable_durations_are_no_information` | grün |
| AI-017 | `1m30.5s` ≥ 90,5 s, `2ms`, `0s` sofort, `soon` keine Pause (Testuhr) | `limits::openai_request_reset_pauses_the_next_request` | grün |
| AI-017 | Token-Reset in Go-Schreibweise | `limits::openai_token_reset_in_go_notation` | grün |
| AI-017 | `anthropic-ratelimit-output-tokens-remaining: 0` allein keine Pause | `limits::output_token_and_image_headers_are_ignored` | grün |
| AI-017 | `soon` kein Fehler im Adapter | `limits::unreadable_reset_is_no_adapter_error` | grün |
| AI-018 | `no (store: false)` für Responses, `unknown` für Decisions ohne `storage`, keine leere Zelle, CSV | `policy::stored_by_provider_column`, `policy::delivered_policies_never_leave_the_cell_empty` | grün |
| AI-018 | ungültiges `storage` abgewiesen | `policy::invalid_storage_entries_are_rejected` | grün |
| AI-018 | jede Zeile `no (store: false)` → jeder Request `"store": false` | `policy::every_responses_request_of_the_task_sends_store_false` | grün |
| AI-011 | ausgelieferte `task-defaults.json` = bestätigte Tabelle, Preise mit Quelle | `defaults::delivered_defaults_match_the_confirmed_table` | grün |
| AI-011 | `max_tokens`-Regel (1024, 2048, 576, 640, 6144, 4000) | `defaults::max_tokens_rule_examples` | grün |

## Testzahlen

Eigene Abnahmetests: **54**, davon **52 grün, 2 rot, 0 ignoriert** (status 9, cache 7, cost 13, log 7, limits 6, policy 4, defaults 2, os-store requests 6). Rot sind genau die Befunde B1-1 und B2-1.

Vollständiger Lauf `cargo test --locked -p os-ai -p os-store --no-fail-fast` auf Tests `23181e2a` über Bauer `9bf50749`: 1.085 bestanden, 2 rot (die beiden oben), 77 ignoriert. `cargo fmt --all --check` und `cargo clippy --locked -p os-ai -p os-store --all-targets -- -D warnings` grün.

## Befunde

**B1-1 – Ist-Kosten der Decisions API rechnen Cache-Lesen zum Eingabepreis.** AI-010, Nachtrag 25.11.2026: „Der Eintrag (`openai`, `gpt-6-luna`, `decisions`) trägt Eingabe 0,10, Ausgabe 0, Cache-Lesen und Cache-Schreiben … wie der `responses`-Eintrag“, Beispiel „1.000.000 Eingabe-Tokens, davon 400.000 aus dem Cache … 0,128 USD – nicht 0,10 USD pauschal“; AI-004 (Verweis 25.11.2026): „keine Kosten für … Cache-Lesen oder Cache-Schreiben“ gilt nicht mehr. Erwartet: Protokollweg der Decisions API 128.000 µUSD. Beobachtet: `adapters::openai_decisions::decisions_billing_rates` setzt `cache_read_pico` und `cache_write_pico` auf den Eingabepreis → 200.000 µUSD (0,20 USD). Der Voranschlag (`cost::estimate_requests`) und die Preistabelle selbst sind richtig (grün); falsch ist nur die Funktion, die laut Doc-Kommentar von `DecisionsRequestRecord::request_outcome` für die Ist-Kosten zu verwenden ist. Die Datei gehört nicht zu K10 (Adapter K02/K09), betrifft aber die K10-Verbuchung. Test: `cost::decisions_billing_rates_follow_the_ai_010_prices` (rot).

**B2-1 – `reasoning = null` im Cache-Schlüssel: Feld mit `null` oder kein Feld?** AI-015, Nachtrag: „geht als Feld `reasoning` in den Cache-Schlüssel ein (Wert `null` auf Wegen ohne Stufe: Decisions API, Embeddings)“; dazu „Die Golden-Datei … wird dafür neu erzeugt“. Wörtlich steht das Feld mit `null` in der kanonischen Darstellung (dann ändern sich auch alle Decisions-Schlüssel). Gebaut ist die zweite Lesart: ohne Stufe fehlt das Feld, Decisions-Schlüssel bleiben byte-gleich zur Schlüsselversion 1. Beobachtbar ist der Unterschied nur im Hash; alle Verhaltenskriterien (Unabhängigkeit von der Stufe) sind grün. Test für die wörtliche Lesart: `cache::decisions_key_names_reasoning_null_literally` (rot). Bitte in der Spec festlegen.

**B3-1 – Ausgelieferte `provider-policies.json` ohne `storage`.** Die Datei trägt für keinen Anbieter ein `storage`-Feld; damit zeigt jede Zeile außer OpenAI Responses `unknown` (auch Ollama, das laut Richtlinie lokal arbeitet). Spec-konform („fehlt das Feld, `unknown`“), aber die Spalte sagt dadurch fast nichts. Hinweis für die Datenpflege.

**B3-2 – `Stored by Provider` hängt am Feld `TaskDataFlow::api`.** Fehlt dort der Endpunkt, wird auch eine OpenAI-Responses-Aufgabe `unknown` statt `no (store: false)`. Für D01/U01: den Endpunkt immer setzen.

**B3-3 – Kein fertiger Protokollweg für den Responses-Abbruch.** Für Anthropic/OpenRouter/Ollama gibt es `RequestOutcome::output_limit(&OutputLimitReached)`; für Responses muss der Aufrufer `RequestMeta::request_outcome(...)` mit dem Status aus `FinalError` nutzen (im Test so gebaut, ergibt korrekte Zeile). `Limiter::run_provider` über `AiProvider::execute` liefert für den Abbruch dagegen `HTTP Status` leer (die Fehlerklasse trägt keinen Status) – dieser Weg darf für das Protokoll nicht genutzt werden. Hinweis für D01.

**B3-4 – Mock-Abhängigkeit beim Aufwärmen.** Ein global gesetztes Szenario (`POST /__mock/scenario`) wird auch von einem Modelllisten-Abruf des Adapters verbraucht; die Tests senden deshalb vorher einen normalen Request. Kein Produktbefund.

## Nicht geprüft (mit Grund)

- AI-020: Lauf auf der Fixture-Site mit 3 × `Skipped (scope)` und Tooltip der Statuszelle – Aufgabenwarteschlange und Geltungsbereich baut D01, Tabs/Tooltip D02/V01. Geprüft sind 22 Einträge über die Zusammensetzung.
- AI-020: „Bei gebündelten Fragen im Ersatzweg erhalten alle Fragen des Requests den Status“ – Bündelung im Ersatzweg ist D01 (AI-034).
- AI-012: Spalte `Reasoning` im Dialog (U01); ±25 %/±5 % der Seitentyp-Frage auf der Fixture-Site (Lauf über D01; die Formel ist mit dem Mock-Ist-Wert geprüft).
- AI-014: Export „AI Request Log“ als Datei – Besitzer des Exports offen (Plan, Nachträge Welle 2 Punkt 8); geprüft ist die kanonische Spaltenliste `COLUMNS`.
- AI-014: Kriterium mit `reasoning=120` und „passender Antwortlänge“ über den Mock – die Mock-Antwortlänge ist nicht frei wählbar; geprüft mit festen `usage`-Werten.
- AI-018: Anzeige der Spalte im Dialog (U01).
- AI-011: Modellwahl bei nur einem Schlüssel (K04/K4-20, abgenommen).
- AI-016 (Batch-Zeilen mit Abbruch) – nicht im K10-Zuschnitt (1.2-Etappe von K06).
- Migration einer bereits mit 0006 angelegten Datei – von der Spec nicht verlangt (0006 in keinem Release).

## Offene Fragen an die Spec

1. AI-015: `reasoning: null` als Feld im Schlüssel oder Weglassen (B2-1)?
2. AI-020: „zählt nicht für den Schutzschalter“ – soll der Abbruch die Zählung zurücksetzen (wie eine Antwort) oder nur nicht erhöhen? Gebaut: weder noch; die Tests prüfen nur, dass er nicht zählt.
3. AI-012: Der Ausgabe-Korrekturfaktor bei `decision-w` braucht A je Request; das Protokoll speichert die Ausgabe-Schätzung nicht. Soll eine Spalte `estimated_output_tokens` dazu?

## Läufe

Lauf 1 (`4b3d3fd9`): fmt/Clippy grün, eigene Ziele 3 rot – neben B1-1/B2-1 ein Fehler im eigenen Test (Kostenvergleich bei Bündelung ohne Rundung je Request; korrigiert, die Spec vergleicht die Tokenschätzung). Lauf 2 (`23181e2a`): Zahlen oben. Build-Maschine Mac mini, eigener Baum `/Volumes/osbuild/openspider/wt/ai-k10-tests`, eigener `CARGO_TARGET_DIR`, jeder Lauf unter `heavy.lock`.
