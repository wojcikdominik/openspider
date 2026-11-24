# M4-K09 – Unabhängige Abnahmetests (Q00-Nachträge AI-001 … AI-006, ai-mock)

Stand 24.11.2026, Zweig `ai/k09-tests`; erste Abnahme auf Bauer-Stand `2e15c009`, Wiederholung nach Merge von `ai/k09` (Endstand `edca3073`, Nacharbeit zu B1-1) – siehe Abschnitt „Nachtrag: Wiederholung auf `edca3073`“. Erwartungen allein aus `docs/spec/08-ai.md`
(Nachträge „24.11.2026, Q00“ an AI-001 … AI-006 und „Konventionen“, jeweils mit dem ursprünglichen Eintrag; Regel der
effektiven Stufe aus AI-011, Nachtrag) und `docs/plan/M4.md` Abschnitt 7 („Auftragskern K09“, Zeilen K09 der Tabelle
„Q00-Nachträge → Paket“). Bericht, neue Tests und Commit-Nachrichten des Bauers nicht gelesen; Produktcode nur für die
öffentliche Schnittstelle gelesen, nicht geändert. Ausnahme laut Auftrag: Diff der geänderten Bestandstests
(Abschnitt „Durchsicht“).

## Dateien

- `crates/os-ai/tests/acceptance_k09_reasoning_level.rs` – Typ `ReasoningLevel`, `resolve_reasoning_level`, neue Felder (6 Tests)
- `crates/os-ai/tests/acceptance_k09_anthropic.rs` – AI-002 (12 Tests)
- `crates/os-ai/tests/acceptance_k09_responses.rs` – AI-003 (8 Tests)
- `crates/os-ai/tests/acceptance_k09_decisions.rs` – AI-004 (5 Tests)
- `crates/os-ai/tests/acceptance_k09_openrouter.rs` – AI-005 (9 Tests)
- `crates/os-ai/tests/acceptance_k09_ollama.rs` – AI-006 (3 Tests)
- `crates/os-ai/tests/acceptance_k09_contract.rs` – AI-001 Contract-Test und Request-IDs (2 Tests)
- `crates/os-ai/tests/acceptance_k09_mock.rs` – Selbsttests der Mock-Erweiterungen mit rohen HTTP-Requests (10 Tests)
- `crates/os-ai/tests/acceptance_k09_support/mod.rs` – Mock-Hülle, Adapter-Fabriken, eigene Fassung der Regel aus
  AI-011 (Nachtrag), kleiner eigener HTTP-Testserver für Antwortformen, die der Mock nicht liefert

Testmittel: ai-mock (`AiMock::fixtures()`), eigener Testserver auf `127.0.0.1`. Keine eigenen Fixture-Dateien nötig
(Standardantwort des Mocks `{"ok":true}`). Kein echter Anbieter, nur erfundene Schlüssel.

## Läufe (Mac mini, unter `heavy.lock`)

| Lauf | Commit | fmt | clippy (`os-ai`, `os-testsite`, `-D warnings`) | Tests |
|---|---|---|---|---|
| 1 | `81045723` | grün | grün | 42 grün, 3 rot (2 davon Testfehler, s. u.), 0 ignoriert |
| 2 | `bdbcaaad` | grün | grün | **44 grün, 1 rot (B1-1), 0 ignoriert** (45 Tests) |
| 3 (Bauer `edca3073`, gemergt) | `efec8be1` | grün | grün, dazu `cargo check --locked --workspace --all-targets` grün | **62 grün, 0 rot, 0 ignoriert** |

Lauf 1: Die beiden Prüfungen „Bildrequest im Log“ (`store_false_in_every_responses_request`,
`decisions_requests_never_carry_store_or_reasoning`) suchten `input_image` als JSON-Schlüssel statt als `type`-Wert –
Fehler im Test, in `bdbcaaad` korrigiert (keine Lockerung: die Prüfung auf `store`/`reasoning` blieb unverändert).

## Kriterium → Test → Ergebnis

| Spec-ID / Kriterium | Test(s) | Ergebnis |
|---|---|---|
| AI-011 (Vertrag K09 P. 1): Varianten, Serde-/Anzeige-/`FromStr`-Namen, Standard `model-default`, Hash | `names_are_identical_for_serde_display_and_parsing` | grün |
| AI-011: Ordnung nur über `rank()`, `ModelDefault` ohne Rang, `EXPLICIT` aufsteigend | `rank_orders_the_scale_and_model_default_has_none` | grün (Rang von `Max` s. B2-1) |
| AI-011: `resolve_reasoning_level` – leere Liste, `model-default`, `off` nicht verfügbar, nächstniedrigere, sonst niedrigste | `resolve_matches_the_spec_examples`, `resolve_agrees_with_the_rule_for_every_subset` (8 × 128 Teilmengen, auch absteigend), Property `resolve_on_arbitrary_lists` (unsortiert, Doppelungen, `model-default` in der Liste) | grün |
| AI-001: `Request.reasoning` (Standard `model-default`), `Model.reasoning_levels` (leer), `Usage.reasoning_tokens` (`null`) | `new_fields_default_to_model_default_empty_and_null` | grün |
| AI-002: Stufe `off` auf A/B/C/D | `off_per_model_a_to_d` | grün |
| AI-002: `medium` auf A, `max` auf B → `xhigh`, `model-default` → keines; `minimal` auf B → `low` | `medium_on_a_max_on_b_and_model_default` | grün |
| AI-002: Property A–D × alle Stufen: nie `enabled`, nie `budget_tokens`, kein Top-Level-`effort`, B nie `max` | `no_enabled_thinking_and_no_budget_over_all_models_and_levels` (64 Requests) | grün |
| AI-002/AI-001: verfügbare Stufen live (A mit `off`, B ohne `off`/`max`, C/D leer) | `reasoning_levels_come_live_from_the_model_list` | grün |
| AI-002: ohne `thinking` bzw. `supported = false` nie `thinking`; `adaptive`/`disabled` nur laut `types` | `thinking_follows_the_reported_types_only` (eigene Modellliste über `set_models`) | grün |
| AI-002: `max_tokens` aus dem Request | `max_tokens_is_taken_from_the_request` | grün |
| AI-002/AI-001: `thinking_tokens` → `reasoning_tokens`, `Output Tokens` = Antwort + 40, `thinking`-Block verworfen, gleiches Ergebnis | `thinking_tokens_become_reasoning_tokens_and_thinking_is_discarded`, `missing_thinking_tokens_stay_null` | grün |
| AI-002: `output-limit` Schema- und Freitext-Aufgabe → eigene Antwortklasse, Grund `max_tokens`, HTTP 200, 1 Request, `Output Tokens` = `max_tokens`, kein Teilergebnis | `output_limit_is_its_own_class_without_repair`, `truncated_answers_are_never_partial_results` | grün |
| AI-001/AI-002: Request-ID = Header `request-id`, nie `msg_…` | `request_id_is_the_header_never_the_message_id` | grün |
| AI-001/AI-002: Fehlerantwort ohne Header, `request_id` im Körper → dieser Wert | `error_request_id_falls_back_to_the_body` | rot auf `2e15c009` (B1-1), **grün auf `edca3073`** |
| AI-003: `store: false` in jedem Responses-Request (Freitext, Schema, Reparatur nach `malformed-json`, Bild, Refusal, Abbruch, 429-Wiederholung, `AiProvider`), nie `prompt_cache_retention`/`previous_response_id` | `store_false_in_every_responses_request`, `other_incomplete_reasons_keep_the_repair_attempt` | grün |
| AI-003: `reasoning.effort` nur mit Katalogstufen (`off`→`none`, `high`, `max`→`high`, ohne Katalog / `model-default` kein Schlüssel, ohne `none` → niedrigste) | `reasoning_effort_only_with_catalog_levels`, `reasoning_values_are_always_catalog_values`, `model_list_carries_the_catalog_levels` | grün |
| AI-003: `output-limit` → `Error Code = max_output_tokens`, 1 Request, `max_output_tokens` gesendet; automatische Abbruchform | `output_limit_ends_the_entry_without_repair` | grün |
| AI-003: andere `incomplete_details.reason` bleiben bei Auslegung 9 (Reparatur) | `other_incomplete_reasons_keep_the_repair_attempt` | grün |
| AI-003/AI-001: `cache_write_tokens` 0 → 0, fehlend → leer; `reasoning=27` → 27; `reasoning`-Element verworfen | `usage_carries_reasoning_and_cache_write_tokens` | grün |
| AI-001/AI-003: Request-ID `x-request-id` | `request_id_comes_from_x_request_id` | grün |
| AI-004: weder `store` noch `reasoning` (Szenarien ok, refusal, missing-answer, 429x2, reasoning=40, Bild, `AiProvider` mit jeder Stufe) | `decisions_requests_never_carry_store_or_reasoning` | grün |
| AI-004: Antwort ohne `id`, `x-request-id: req_mock_3` → `req_mock_3`; `cache_write_tokens` 0 → 0 | `request_id_header_and_cache_write_tokens` | grün |
| AI-004 / Konventionen: Zählung je Frage (3 Fragen; 11 Fragen → 10 + 1 Requests, Summe = 11 Fragen; Bild +85 je Frage); Antworten weiter korrekt ausgewertet | `three_questions_are_billed_per_question_and_still_evaluated`, `eleven_questions_are_billed_per_question` | grün |
| AI-004/AI-011: Decisions ohne Stufen | `decisions_models_have_no_reasoning_levels` | grün |
| AI-005: `off` auf M-R1 → `none`, M-R2 → `low`, M-R3 / `model-default` → kein `reasoning`; Stufen je Modell | `reasoning_follows_supported_efforts_and_mandatory` | grün |
| AI-005: nie `reasoning_effort`/`include_reasoning`, `reasoning` nur aus `supported_efforts` (alle 8 Modelle × 8 Stufen × mit/ohne Schema) | `one_reasoning_form_over_all_models_and_levels` | grün |
| AI-005: `output-limit` → `length`, 1 Request; automatische Abbruchform | `output_limit_is_length_without_repair` | grün |
| AI-005/AI-001: `reasoning_tokens`, `cached`/`cache_write_tokens`, Reasoning-Inhalte verworfen | `usage_fields_and_discarded_reasoning` | grün |
| AI-005 (M3, Adapterseite): `usage.cost`/`cost_details`/`is_byok` im Ergebnis; `cost-mismatch`, `no-cost`, `byok`; Gebühren im Bildrequest; Override auf M-LC; Rundung auf µUSD, ungültige Werte | `reported_cost_is_passed_through`, `reported_cost_rounding_and_invalid_values` | grün |
| AI-005 (M4): `overrides` (≥, größter zutreffender, Teilfelder), `internal_reasoning` (max mit `completion`), Cache-Preise, `Long Context From`; Prüfwerte 2.400 / 1.199 / 10.000 µUSD | `extended_price_fields_are_read` | grün |
| AI-005 (N3): `unknown-model` → `Error: 400 <model> is not a valid model ID`, kein `Error Code`, kein Kontingent, Folgeeintrag läuft, `mock-user` nicht im Ergebnis | `unknown_model_is_a_plain_400` | grün |
| AI-001/AI-005: `x-generation-id`, ersatzweise Antwort-`id` | `request_id_from_generation_header_or_body` | grün |
| AI-006: nur `model-default` (kein Parameter), keine Stufen | `no_reasoning_parameter_at_any_level` | grün |
| AI-006/AI-001: `reasoning_tokens` `null`, keine Request-ID | `reasoning_tokens_stay_null_and_no_request_id` | grün |
| AI-006: `output-limit` → `length`, 1 Request, Kosten 0, kein Teilergebnis | `output_limit_is_length_with_one_request_and_no_cost` | grün |
| AI-001: Contract-Test `reasoning=40` (3 Cloud-Adapter 40 und gleiches Ergebnis, Ollama `null`), keine Reasoning-Marker im Ergebnis | `contract_with_reasoning_40` | grün |
| AI-001: Request-ID je Anbieter im gemeinsamen Ergebnis | `provider_request_ids_per_provider` | grün |
| Konventionen: `reasoning=<n>` (global und Markierung) je Anbieterform, ohne Szenario 0, Ollama ohne Felder | `reasoning_scenario_forms_per_provider` | grün |
| Konventionen: automatische Abbruchform (Grenze = Antwort + n kein Abbruch, eins darunter Abbruch; n = 0 abgeschnittener Text; Responses nur `max_output_tokens`) | `automatic_output_limit_forms` | grün |
| Konventionen: `output-limit` erzwingt (Reasoning = Grenze), Ollama `done_reason: "length"`, `eval_count` 16 | `forced_output_limit_forms` | grün |
| Konventionen: `usage.cost`, `no-cost`, `cost-mismatch`, `byok`, Gebühren | `openrouter_cost_scenarios` | grün |
| Konventionen: OpenRouter-Liste (8 Modelle, M-LC, M-IR, M-R1…3, `max_completion_tokens = 4000`, Ablauf 30/200/`null`), Mock-Kosten mit Override | `openrouter_model_list_roles` | grün |
| Konventionen: Anthropic-Liste A–D mit `thinking`, B `max_tokens = 4000`, `retires_at` 30/200/`null` | `anthropic_model_list_thinking_variants` | grün |
| Konventionen: `unknown-model`-Form | `unknown_model_form` | grün |
| Konventionen: Decisions-Zählung je Frage, `reasoning_tokens = 0`, `cache_write_tokens` (Decisions, Responses), ohne `id` | `decisions_counting_and_openai_usage_fields` | grün |
| Konventionen: Request-IDs je Anbieter, laufende Nummer, Fehler mit `request_id` im Körper | `request_ids_per_provider` | grün |
| Konventionen: `GET /__mock/log` mit Body | `request_log_carries_the_body` | grün |

## Befunde

**B1-1 (Abweichung, AI-001 Nachtrag N2 / AI-002):** Spec: „Anbieter-Request-ID … Anthropic Header `request-id` (bei
Fehlern ersatzweise `request_id` im Antwortkörper) … Begründung: Diese Kennungen nennen die Anbieter für
Supportanfragen; die Anthropic-Antwort-`id` fehlt bei Fehlern.“ Kriterium: „Anthropic-Fehlerantwort ohne Header, aber
mit `request_id` im Körper → dieser Wert.“ Erwartet: Das Fehlerergebnis eines Versuchs trägt die Request-ID, damit sie
ins Protokoll (`Provider Request ID`) kommt. Beobachtet: `anthropic::request_id(headers, body)` liefert den Wert korrekt,
aber `AnthropicAdapter::send` gibt bei Fehlerantworten ein `AttemptFailure` ohne jede Request-ID zurück
(`AttemptFailure { error: Http { status: 400, message: "bad" }, cause: None, http_status: None, error_code:
Some("invalid_request_error"), retry_after: None }`) – auch der Header `request-id` geht bei Fehlern verloren.
`AttemptFailure`/`FinalError` haben dafür kein Feld; OpenRouter- und OpenAI-Fehler sind strukturell gleich betroffen
(dort nennt die Spec den Fehlerfall nicht ausdrücklich). Test `error_request_id_falls_back_to_the_body` bleibt rot.
Nebenbefund: `AttemptFailure.http_status` ist hier `None`, obwohl die Antwort HTTP 400 war (Status steckt nur in
`ProviderError::Http`).

**B2-1 (Unklarheit, Vertrag M4.md Abschnitt 7 P. 1):** „Ordnung nur über `fn rank(self) -> Option<u8>` (`Off` = 0 …
`Max` = 7 …)“ – bei sieben Stufen ergibt eine lückenlose Zählung `Max` = 6 (so umgesetzt). Getestet nur Reihenfolge und
`Off` = 0. Vertrag berichtigen.

**B2-2 (Unklarheit, AI-002 Nachtrag vs. AI-011 Nachtrag):** Stufe `minimal` auf Anthropic-Modell A (abschaltbar,
Aufwand `low` … `max`). AI-002: „`minimal` … `max` → `thinking: {"type": "adaptive"}` … plus `output_config.effort` …
(nicht unterstützte Stufe → nächstniedrigere unterstützte)“ – das spräche für `adaptive` mit niedrigster
Aufwandsstufe. AI-011: nächstniedrigere *verfügbare* Stufe – die ist `off`, also `thinking: disabled`. Der Adapter folgt
AI-011 (`disabled`). Nicht getestet; klären, welche Lesart gilt (Planvertrag: „`Minimal` fällt mangels Stufe auf die
niedrigste verfügbare“ – bei A ebenfalls `off`).

**B2-3 (Unklarheit, Konventionen):** Ablaufdaten der Mock-Listen „relativ zur Testuhr (AI-025)“; der Mock rechnet mit
der Systemuhr und hat keine injizierbare Uhr. Getestet relativ zum Systemdatum. Klären, ob eine Testuhr für den Mock
nötig ist (K08).

**B3-1 (Hinweis, AI-002):** Mit `reasoning = model-default` und gesetztem älterem Feld `Request.effort` sendet der
Adapter weiterhin `output_config.effort`; AI-002 Nachtrag: „`model-default` → weder `thinking` noch
`output_config.effort`“. Mit dem Standard `effort: None` kein Problem; zwei überlappende Felder im Request-Typ – Rolle von
`effort` klären oder entfernen.

**B3-2 (Hinweis, AI-002):** Fehlt `max_tokens` im Request, sendet der Adapter weiter den festen Wert 4096
(`FALLBACK_MAX_TOKENS`); der Nachtrag ersetzt diesen Standard durch AI-011. Solange K04/D01 immer einen Wert setzen,
folgenlos.

**B3-3 (Hinweis, Konventionen / AI-005, nicht K09-verursacht):** „die Zählungen aus AI-005 und AI-019 gelten unverändert“
– die Mock-Liste hat seit T01 nur 1 Modell ohne Structured Output (AI-005-Fixture: 3) und 1 Vision-Modell (AI-005: 2).
Nicht getestet, da vor K09 so.

**B3-4 (Hinweis):** OpenRouter sendet `reasoning` nur bei geladener Modellliste; schlägt das Laden fehl, geht der Request
still ohne Parameter (mit Modellstandard). V-A6 verlangt „kein stilles Weglassen“ für Anbieter-Ablehnungen; für diesen
Fall wäre eine Warnung bzw. ein Fehler zu erwägen.

## Durchsicht der geänderten Bestandstests (Diff `1416c1fc..2e15c009`)

| Datei / Zusicherung | Änderung | Deckung durch Spec | Urteil |
|---|---|---|---|
| `acceptance_f01_status.rs` – Feldliste von `usage` | 4 → 5 Felder (`reasoning_tokens`) | AI-001 Nachtrag „Einheitliche `usage` um `reasoning_tokens` erweitert“ | gedeckt, additiv |
| `acceptance_k01_anthropic.rs` K4 – Request-ID | `msg_k01` → Header `req_k01` (Stub um Header ergänzt) | AI-001 Nachtrag N2 „nie die Antwort-`id` (`msg_…`)“ | gedeckt |
| `acceptance_k01_anthropic.rs` K5 – `max_tokens` bei Schema-Aufgabe | `InvalidOutput` nach Reparatur → `Failed("max_tokens")`, genau 1 Request | AI-002 Nachtrag: „**Ersetzt** das Kriterium … → `Error: Invalid output` … K01-Auslegung 1 überholt“; Teilsatz „nicht teilweise übernommen“ bleibt durch `Failed` erfüllt | gedeckt; Statuswert folgt mit K10 |
| `acceptance_k02_decisions.rs` k5 – Ausgabe-Tokens des Mocks | `> 0` → `== 0` | Konventionen Nachtrag „Decisions … Ausgabe-Tokens 0“; „auch wenn der Mock welche meldet“ weiter durch Stub-Test mit 500.000 Ausgabe-Tokens (Zeilen 293/317) geprüft | gedeckt (Decisions-Zählung), keine Lockerung |
| `acceptance_k02_responses.rs` k3 – erwartete `usage` | `reasoning_tokens: Some(0)` ergänzt; der Stub liefert `output_tokens_details.reasoning_tokens: 0` | AI-003 Nachtrag `reasoning_tokens` | gedeckt, additiv |
| Hilfs-/Konstruktoränderungen (`k01/k03/k05/k07`-Support, `k03_embeddings`, `k05_log`, `adapter_*_support`, `adapter_ollama`, `adapter_openai_*` Builder, `provider_matrix` Request) | nur neue Felder `reasoning_levels`, `reasoning`, `reasoning_tokens` mit Standardwert | Pflichtfelder der neuen Typen | gedeckt, keine Zusicherung geändert |
| `adapter_anthropic.rs` – `max_tokens` bei Schema-Aufgabe | 2 Requests/`InvalidOutput` → `Failed` mit `error_code = max_tokens`, HTTP 200, +1 Request; `into_response`/`execute` weiter `InvalidOutput` | AI-002 Nachtrag (ersetzt) | gedeckt |
| `adapter_anthropic.rs` – Aufwandsstufen A/B | A `[low, medium, high, max]` → `[…, xhigh, max]`, B `[low, medium, high]` → `[…, xhigh]` | weder Kriteriumsersatz noch Decisions-Zählung, sondern Fixture-Änderung des Mocks; notwendig für den neuen Prüfwert „`max` auf B → `xhigh`“ (AI-002 Nachtrag) und mit dem Original („A alle Stufen“, „B `max` false“) verträglich | sachlich gedeckt, formal außerhalb der zwei erlaubten Kategorien (Hinweis) |
| `adapter_openai_decisions.rs` – Eingabe-Tokens / Ausgabe | `token_count` → `decisions_input_tokens`, Ausgabe `> 0` → `Some(0)`; Satz „Ausgabe trägt 0 bei“ weiter über Sätze mit Ausgabepreis/Stub | Konventionen Nachtrag (Decisions-Zählung) | gedeckt |
| `adapter_openai_responses.rs` – `cache_write_tokens` | `None` → `Some(0)` (zweimal) | AI-003 Nachtrag N1 „der Adapter setzte bisher immer `null`“ | gedeckt |
| `adapter_openai_responses.rs` – Request-ID | `mock-request` → `req_mock_1` | Konventionen Nachtrag „Request-IDs“ (Mock-Header geändert) | gedeckt |
| `provider_matrix.rs` – `usage`-JSON | `reasoning_tokens: null` ergänzt | AI-001 Nachtrag | gedeckt, additiv |
| `ai_mock_contract.rs` – Decisions-`usage` | `input_tokens` 2 → 101, Ausgabe → 0 | Konventionen Nachtrag (Decisions-Zählung) | gedeckt |

Ergebnis: keine Lockerung gefunden; eine Fixture-bedingte Anpassung außerhalb der beiden Kategorien (Aufwandsstufen A/B),
sachlich durch den neuen Prüfwert gedeckt.

## Nicht geprüft (mit Grund)

- Status `Error: Output limit reached`, Protokollspalten (`Reasoning Tokens`, `Cache Write Tokens`, `Computed Cost`,
  `cost_mismatch`, `Price Source = reported`), Verbuchung im Budget, Kostenschätzung, Cache-Schlüssel – K10.
- Volltextsuche nach Reasoning-Markern in Crawl-Datei, `ai_requests`, `ai_cache`, Anwendungslogs (AI-001) – nur auf
  Adapterebene (Ergebnis- und Debug-Ausgabe) geprüft; Speicherorte gehören K05/K06/K10/D01.
- `store: false` in Batch-JSONL-Zeilen (AI-016) – laut Tabelle K06 (1.2-Etappe).
- Aufgabendialog-Hinweise („aus nicht verfügbar – niedrigste Stufe: low“), Dropdown, `max_tokens`-Regel und Katalog
  `reasoning_efforts`/`task-defaults.json` – K04/U01.
- „Fehlerform unbekanntes Modell je Anbieter“ außer OpenRouter – die Spec legt nur OpenRouter fest (N3).
- Live-Formen (V-A6, V-A8) – Q00b.

## Nachtrag: Wiederholung auf `edca3073`

`ai/k09` (Endstand `edca3073`: Request-ID im Fehlerweg als Feld `provider_request_id` in `AttemptFailure`/`FinalError`
für Anthropic, OpenAI Responses/Decisions und OpenRouter sowie beim Abbruch an der Ausgabegrenze; Zähler
`reasoning_skipped_without_model_list` für OpenRouter) in `ai/k09-tests` gemergt (`a45a6537`). Gelesen wurde dafür nur
das Diff der öffentlichen Schnittstelle, nicht die Tests oder der Bericht des Bauers.

Entscheidungen des Orchestrators: B2-1 `rank` 0 … 6 (nur Reihenfolge maßgeblich, Plantext wird korrigiert);
B2-2 Fallback allein nach `resolve_reasoning_level` (AI-002 wird angeglichen); B2-3 Ablaufdaten am Systemdatum
akzeptiert. Damit sind B2-1 … B2-3 geschlossen. B1-1 ist behoben.

Neue bzw. geänderte Tests (Commit `efec8be1`):

| Kriterium | Test | Ergebnis |
|---|---|---|
| AI-001/AI-002: Fehlerergebnis trägt `request_id` aus dem Körper (B1-1, jetzt über das Feld statt Debug-Text) | `error_request_id_falls_back_to_the_body` | grün |
| AI-001/AI-002: Fehlerweg mit Header `request-id` (Vorrang vor Körper), ai-mock `401` → `req_mock_<n>`, Abbruch an der Ausgabegrenze mit Request-ID | `error_and_output_limit_results_carry_the_request_id` | grün |
| AI-002/AI-011 (B2-2): `minimal` auf A → `thinking: disabled` ohne Aufwand; ausdrückliche Stufe gewinnt über `Request.effort` | `minimal_on_a_falls_back_to_off_and_level_beats_legacy_effort` | grün |
| AI-001/AI-003: Fehlerantwort mit `x-request-id` → `FinalError` und `RequestMeta`; ai-mock `401`; Abbruch an der Ausgabegrenze | `error_results_carry_x_request_id` (Responses) | grün |
| AI-001/AI-004 (N6): Fehlerantwort mit `x-request-id` → `AttemptFailure` und Protokolldaten von `decide`; ai-mock `401` | `error_results_carry_x_request_id` (Decisions) | grün |
| AI-001/AI-005: Fehlerantwort mit `x-generation-id`; ai-mock `unknown-model` → `gen-mock-<n>`; Abbruch an der Ausgabegrenze | `error_results_carry_the_generation_id` | grün |
| AI-005 (B3-4): ohne ladbare Modellliste kein erfundenes `reasoning`, Fall wird gezählt; mit Liste Zähler 0 | `missing_model_list_is_counted` | grün |
| AI-001/AI-006: Ollama ohne Request-ID auch im Fehlerweg | `ollama_errors_have_no_request_id` | grün |

Testzahlen Lauf 3 (Prüfzeile `PRUEF Tester efec8be1 BAUER=edca3073`): anthropic 14, contract 2, decisions 6, mock 10,
ollama 4, openrouter 11, reasoning_level 6, responses 9 – **62 grün, 0 rot, 0 ignoriert**; `cargo fmt --all --check`,
`cargo clippy --locked -p os-ai -p os-testsite --all-targets -- -D warnings` und
`cargo check --locked --workspace --all-targets` grün.

Offen bleiben nur Hinweise: B3-1 teilweise (mit `model-default` und gesetztem `Request.effort` geht weiter
`output_config.effort` hinaus; nicht entschieden), B3-2 (fester Ersatzwert 4096 ohne `max_tokens`), B3-3 (Zählungen der
OpenRouter-Mock-Liste, vor K09). Die Request-ID im Fehlerweg nennt die Spec ausdrücklich nur für Anthropic
(`request_id` im Körper); für OpenAI und OpenRouter wurde die jeweils im Nachtrag genannte Header-Quelle auch im
Fehlerweg geprüft.
