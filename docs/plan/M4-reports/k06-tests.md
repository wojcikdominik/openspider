# M4-K06 – Unabhängige Abnahmetests (Cache, Limits, Schema)

Zweig `ai/k06-tests` (Basis `64356d38`). Erwartungen nur aus `docs/spec/08-ai.md` (AI-015, AI-017, AI-020 inkl. Nachtrag 24.11.2026, AI-021, AI-050 für Seitentyp-Werte) und `docs/spec/04-custom-integrations.md` (INTEG-005, INTEG-171, INTEG-179). Bericht, Verlauf und Tests des Bauers wurden nicht als Vorlage genutzt; Produktcode nur für Aufrufsignaturen gelesen. AI-016 (Batch) ist nicht Teil dieser Etappe und wurde nicht geprüft.

Ausgeführt auf dem Mac mini (`/Volumes/osbuild/openspider/wt/ai-k06`, unter `heavy.lock`), Logs in `/Volumes/osbuild/openspider/wt/ai-k06/k06-tests-log/`:

| Lauf | Log | Ergebnis |
|---|---|---|
| `cargo test -p os-ai --test acceptance_k06_cache --test acceptance_k06_limits --test acceptance_k06_schema --no-fail-fast` | `run1-os-ai.log` | 14 + 23 + 10 grün, EXIT 0 |
| `cargo test -p os-store --test acceptance_k06_ai_cache --no-fail-fast` | `run1-os-store.log` | 4 grün, EXIT 0 |
| `cargo clippy -p os-model -p os-store -p os-ai --all-targets -- -D warnings` (1. Lauf) | `clippy.log` | rot: `drop` auf geliehenem Wert in meinem Test (Testfehler, behoben) |
| dasselbe nach Korrektur | `clippy2.log` | EXIT 0 |
| `cargo test -p os-ai --test acceptance_k06_cache` nach Korrektur | `run2-os-ai-cache.log` | 14 grün, EXIT 0 |

Kein SIGBUS oder anderer Mini-Wackler aufgetreten.

## 1. Kriterium → Test → Ergebnis

Dateien: C = `crates/os-ai/tests/acceptance_k06_cache.rs`, L = `crates/os-ai/tests/acceptance_k06_limits.rs`, S = `crates/os-ai/tests/acceptance_k06_schema.rs`, St = `crates/os-store/tests/acceptance_k06_ai_cache.rs`.

| Kriterium (Spec-ID) | Test | Ergebnis |
|---|---|---|
| AI-015 Schlüssel = SHA-256 über kanonisches JSON, eigene Bildung im Test | C `key_is_sha256_over_canonical_json_of_all_components` | grün |
| AI-015 jeder Bestandteil ändert den Schlüssel (Aufgabentyp, ID, Version, Vorlagenparameter, Weg, Anbieter, Modell, Endpunkt EU/global, ergebnisrelevante Parameter, Eingabe-Hash) | C `every_component_changes_the_key` | grün |
| AI-015 Reihenfolge von Objektschlüsseln irrelevant | C `object_key_order_in_parameters_does_not_change_the_key` | grün |
| AI-015 plattformunabhängig, Golden-Test mit 10 Schlüsseln (Fixture, unabhängig mit Python berechnet) | C `golden_ten_keys_are_platform_independent` | grün |
| AI-015 Eingabe-Hash, Bilder über Byte-Hash | C `input_hash_covers_sent_text_and_image_bytes` | grün |
| AI-015 zweiter Lauf 0 Requests, `Success (cached)`; Neustart (Datei schließen/öffnen) | C `second_run_sends_no_requests_and_survives_reopen` | grün |
| AI-015 neue Version → neu; Modellwechsel 22, Rückwechsel 0, beide bleiben | C `new_version_and_model_switch_request_again_switch_back_is_cached` | grün |
| AI-015 zwei Sprachparameter → 2 Einträge, erneut 0 | C `parameterised_question_two_language_codes_two_entries` | grün |
| AI-015/AI-020 Fehler nie gecacht, nächster Lauf fragt erneut | C `errors_are_never_cached`, St `only_success_and_refusal_rows_are_stored` | grün |
| AI-015 Refusal mit Kennzeichen, nur nach „Refusals erneut versuchen“ erneut | C `refusals_are_cached_with_flag_and_only_retried_on_request` | grün |
| AI-015 Option „Cache verwenden“ (Standard an; aus → jeder Eintrag angefragt) | C `option_use_cache_defaults_on_and_off_always_requests` | grün |
| AI-015 „Cache für Aufgabe/Frage leeren“ | C `clear_cache_for_one_question_keeps_others`, St `clearing_one_question_leaves_other_questions` | grün |
| AI-015 Statistik (Treffer, Einträge, Größe) | C `statistics_report_hits_entries_and_size` | grün |
| AI-015 Übernahme: Quelle nur gelesen, byte-identisch (Hash, Begleitdateien, mtime), danach nur 3 Requests für 3 geänderte Eingaben | C `imported_cache_from_other_file_is_read_only_and_recrawl_requests_only_changed`, St `import_reads_source_only_and_source_stays_byte_identical` | grün |
| AI-015 Ergebnisse überstehen Schließen/Neuöffnen (Speicherebene) | St `rows_survive_close_and_reopen` | grün |
| INTEG-005 Backoff-Parameter fest (1 s, ×2, 60 s, ±20 %), Standard 5 Wiederholungen | L `backoff_policy_parameters_are_fixed_as_specified` | grün |
| INTEG-005 Abstände n→n+1 in [0,8·2^(n−1), min(60, 1,2·2^(n−1))] (fester und zufälliger Jitter) | L `waits_between_attempts_follow_backoff_window` | grün |
| INTEG-005 Obergrenze 60 s | L `backoff_is_capped_at_60_seconds` | grün |
| INTEG-005 `Retry-After: 7` → ≥ 7 s, auch für einen anderen Eintrag desselben Anbieters; anderer Anbieter frei | L `retry_after_seven_waits_at_least_seven_seconds_for_this_provider` | grün |
| INTEG-005 `Retry-After` als Sekunden und HTTP-Datum | L `retry_after_accepts_seconds_and_http_date` | grün |
| INTEG-005 zweimal 429, dann 200 → Erfolg | L `two_429_then_200_succeeds` | grün |
| INTEG-005 400/401/403/404 genau 1 Request, Status `Error: <Code> …` | L `client_errors_are_never_retried` | grün |
| INTEG-005/AI-020 5 Wiederholungen (6 Requests) → endgültiger Fehler | L `five_failed_retries_end_as_final_error_after_six_requests` | grün |
| INTEG-005/AI-017 gleichzeitige Anfragen je Anbieter ≤ Einstellung (2; Standard 4 wird erreicht, nie überschritten) | L `concurrent_requests_per_provider_never_exceed_setting` | grün |
| AI-017 TPM 10.000 bei 2.500 Tokens → max. 4 je 60-s-Fenster | L `tpm_10000_with_2500_tokens_allows_at_most_4_per_window` | grün |
| INTEG-171 RPM 20 → max. 20 je 60-s-Fenster; Modelle getrennt gedrosselt | L `rpm_20_limits_window_and_models_are_throttled_separately` | grün |
| AI-017 `anthropic-ratelimit-requests-remaining: 0`, Reset 20 s → Start ≥ 20 s; Pause-Hinweis vorhanden | L `anthropic_requests_remaining_zero_pauses_until_reset` | grün |
| AI-017 `anthropic-ratelimit-input-tokens-remaining: 0`: 300 (und 499) → keine Pause, 2.500 → Pause bis Reset | L `anthropic_input_tokens_rounded_interval_decides_pause` | grün |
| AI-017 `anthropic-ratelimit-tokens-remaining: 0` allein → keine Pause | L `anthropic_combined_tokens_header_alone_never_pauses` | grün |
| AI-017 OpenAI-Header (Requests 0 → Pause bis Reset; Tokens 299 < 300 → Pause, 300 → keine) | L `openai_rate_limit_headers_pause_until_reset` | grün |
| AI-017 Header-Auswertung (Namen, Reset, Rundungsintervall [r−500, r+499]) | L `header_parsing_reads_the_specified_headers` | grün |
| AI-017 Schutzschalter: 10 endgültige Fehler (500-always) → 5 min keine Anfrage an A, B unbeeinflusst | L `breaker_pauses_provider_five_minutes_after_ten_final_errors_other_provider_unaffected` | grün |
| AI-020/AI-017 `Connection failed` zählt für den Schutzschalter | L `breaker_counts_connection_failures` | grün |
| AI-017 Ausnahmen zählen nicht (Spend-Cap-429, OpenRouter-402, Refusal-403; je 1 Request) | L `breaker_ignores_spend_cap_openrouter_402_and_refusal_403` | grün |
| AI-017 Schutzschalter Standard an, abschaltbar | L `breaker_can_be_switched_off_and_defaults_on` | grün |
| AI-017/INTEG-179 `hang`, Timeout 30 s → Abbruch 30 ± 2 s, 1 + 5 Versuche, `Error: Timeout nach 30 s`, kein HTTP-Status | L `hang_with_timeout_30s_aborts_after_30s_retries_then_timeout_status` | grün |
| INTEG-179 Standard-Timeouts 120 s / Ollama 300 s | L `default_timeouts_follow_integ_179` | grün |
| AI-020 (Nachtrag 24.11.) geschlossener Loopback-Port, echte reqwest-Verbindung → 6 Versuche, `Error: Connection failed`, `transport/connect`, HTTP-Status leer, Retries 5 | L `unreachable_provider_ends_connection_failed_after_one_plus_five_attempts` | grün |
| AI-021 `malformed-json` einmal, dann ok → gültig nach 2 Requests | S `malformed_json_once_then_ok_is_valid_after_two_requests` | grün |
| AI-021 dauerhafter Schema-Verstoß → 2 Requests, `Error: Invalid output`, Validator-Fehler abrufbar | S `permanent_schema_violation_ends_invalid_output_after_two_requests` | grün |
| AI-021 Reparaturaufruf enthält Validator-Meldung | S `repair_request_carries_the_validator_message` | grün |
| AI-021 Reparaturversuche 0/2, Bereich 0–2 (3, 4, 255 abgelehnt) | S `repair_attempts_zero_and_two_and_range_check` | grün |
| AI-021 Refusal wird nicht validiert/repariert | S `refusal_is_neither_validated_nor_repaired` | grün |
| AI-021 zusätzliche Felder (oben und verschachtelt) und Enum `shop` sind Verstöße | S `additional_fields_and_enum_values_outside_list_are_violations`, `extra_field_in_answer_leads_to_repair_and_invalid_output` | grün |
| AI-021 Normalisierung (trimmen, Steuerzeichen entfernen) | S `normalisation_trims_whitespace_and_removes_control_characters`, `answers_are_normalised_before_being_accepted` | grün |
| AI-021 Prüfer „strikt verträglich“ (optional / offen verschachtelt fällt durch; korrekt besteht; Fixture und `contract-response.schema.json` bestehen) | S `strict_mode_checker_accepts_correct_and_rejects_optional_or_open_nested` | grün |

Alle 51 Tests grün; kein roter Test.

## 2. Befunde

Keine Produktbefunde. Einziger roter Lauf war ein Testfehler von mir (Clippy `drop_non_drop` auf einem geliehenen `ResultCache` in C), behoben und erneut grün gelaufen.

## 3. Spec-Unklarheiten, Lücken, nicht prüfbar

Unklarheiten (nicht geraten, gemeldet):

1. **AI-015 kanonisches JSON nicht festgelegt:** Die Spec nennt die Bestandteile, aber weder Feldnamen, Struktur, Versionsfeld noch die Kanonisierungsregel (Sortierung nach Codepunkten/UTF-8 oder nach UTF-16 wie RFC 8785, Zahlformat, Escapes). Mein Golden-Test nutzt daher die öffentlichen Feldnamen von `CacheKeyInput` plus `v = KEY_FORMAT_VERSION`; er sichert Plattformgleichheit dieser Implementierung, aber nicht die Übereinstimmung mit einer unabhängigen Implementierung. Vorschlag: Struktur und Regel (z. B. RFC 8785) in AI-015 festschreiben. Auch das Format des Eingabe-Hashes ist offen (geprüft nur: gleiche Bildbytes → gleicher Hash).
2. **„Cache verwenden“ aus:** Die Spec sagt nicht, ob dann noch geschrieben wird. Geprüft wurde nur, dass jeder Eintrag angefragt wird.
3. **Schutzschalter-Ausnahmen (AI-017):** „zählen nicht“ lässt offen, ob sie die Folge nur nicht erhöhen oder sie unterbrechen (zurücksetzen). Der Test ist so gebaut, dass beide Lesarten bestehen.
4. **AI-021 „alle Schemas unter `resources/ai/schemas/`“:** Dort liegen auch Konfigurationsschemas (`pricing.schema.json`, `task-defaults.schema.json`), die keine Antwortschemas sind; Antwortschemas der Aufgaben gibt es dort noch nicht (nur `contract-response.schema.json`). Geprüft wurden nur dieses Schema und meine Fixture. Bitte klären, ob der CI-Test nur Antwortschemas meint.
5. **Normalisierung (AI-021):** Ob Zeilenumbruch und Tabulator *innerhalb* eines Textes als Steuerzeichen entfernt werden, ist offen; geprüft wurden nur eindeutige Fälle (Rand-Leerraum, BEL, NUL, ESC, DEL).
6. **Schwellwertänderung → 0 Requests:** Auf Bibliotheksebene nur strukturell erfüllt (der Schlüssel hat kein Schwellwertfeld); der eigentliche Nachweis braucht den Fragenlauf.
7. **Ausnahmen-Klassifikation:** Spend-Cap-429, OpenRouter-402 und Refusal-403 werden laut Spec von den Adaptern (AI-002, AI-005) erkannt. Hier ist modelliert, was der Adapter liefern muss (`QuotaExceeded` bzw. `Refusal`). Dass die Adapter so abbilden, prüft dieser Test nicht.

In dieser Etappe nicht prüfbar (braucht echten Lauf, Adapter, ai-mock oder UI):
- AI-015: 22/0 Requests auf der Fixture-Site mit `Skipped (scope)` und Kosten 0; Schwellwert-/`instructions`-Änderung im Fragenlauf; Recrawl `site-changed/` mit genau 3 Requests über echte Seiten; Plattformgleichheit macOS ↔ Windows (nur Literal-Golden, nicht auf Windows gelaufen); Dateiauswahl-Dialog.
- AI-017: Mock-Zeitstempel über ai-mock; Szenarien `429x2`, `retry-after-7`, `500-always`, `hang` über echte Anbieter-Endpunkte; „kein 429 entsteht“; Hinweis „<Anbieter> pausiert bis <Uhrzeit> (Rate-Limit)“ in der Fortschrittsanzeige (nur Vorhandensein der Pause-Information geprüft); ein Timeout für alle Fragen eines Mehrfragen-Requests.
- AI-020: Protokollzeile in `ai_requests` (`Cost = 0`, Spalten), Teilfehler-Lauf auf `site-partial/`, Fehlerfall-Matrix, Filter „Errors“, Kontextmenü.
- AI-021: Native Schema-Funktion der Anbieter; „gleiche Eingabe“ im Reparaturaufruf (nur der Hinweis ist geprüft); CI-Test über die automatisch erzeugten Fallback-Schemas (AI-034).
- INTEG-005: Isolation gegenüber dem Crawl, Reiter „Erweitert“. INTEG-171: Hinweis „RPM für <Modell> reduzieren“. INTEG-179: Bereich 10–3.600 im Dialog, Bildgenerierung höchstens 1 Wiederholung (nicht abgedeckt), Prompt-Tester.

## 4. Angelegte Dateien

| Datei | Inhalt |
|---|---|
| `crates/os-ai/tests/acceptance_k06_cache.rs` | 14 Tests AI-015 (Schlüssel, Golden, Läufe über echter Crawl-Datei, Übernahme) |
| `crates/os-ai/tests/acceptance_k06_limits.rs` | 23 Tests AI-017, INTEG-005/171/179, AI-020 `Connection failed` (Testuhr; echte Loopback-Verbindung) |
| `crates/os-ai/tests/acceptance_k06_schema.rs` | 10 Tests AI-021 |
| `crates/os-store/tests/acceptance_k06_ai_cache.rs` | 4 Tests `ai_cache` (Speicherregeln, Neuöffnen, Leeren, Übernahme byte-identisch) |
| `tests/fixtures/ai/k06-acceptance/golden-cache-keys.json` | 10 Golden-Schlüssel (Eingaben, kanonische Form, SHA-256; mit Python berechnet) |
| `tests/fixtures/ai/k06-acceptance/page-type-answer.schema.json` | Antwortschema Seitentyp (Werte aus AI-050), strikt verträglich |
| `docs/plan/M4-reports/k06-tests.md` | dieser Bericht |
