# M4-F01 – Unabhängige Abnahmetests

Zweig `ai/f01-tests` ab `ai/f01` (`80cba3f5`). Erwartungen nur aus `docs/spec/08-ai.md`, `docs/spec/04-custom-integrations.md` (INTEG-179 … 184), `docs/spec/06-output-ops.md` (OUT-201), `docs/spec/10-agent-interface.md` (AGENT-003/030) und `docs/plan/M4.md` (E1, E-2 b). Bericht, Commit-Texte und Tests des Bauers wurden nicht gelesen. Jeder Test trägt `// spec: <ID>` und `// acceptance: independent`.

## Dateien

- `crates/os-ai/tests/acceptance_f01_preview.rs` – Schalter „KI (Vorschau)“, Routing-Sperren, Statusabbildung über `provider::execute` (Anbieter-Attrappe mit echtem lokalem TCP-Lauscher, um „kein Netzwerk-Request“ von außen zu prüfen)
- `crates/os-ai/tests/acceptance_f01_status.rs` – Statuswerte AI-020, Errors-Menge, `usage`, Fähigkeitsspalten
- `crates/os-model/tests/acceptance_f01_ai_config.rs` – Profil, App-Einstellungen, Budget-Typen, Rundreisen, alte Dateien
- `crates/os-store/tests/acceptance_f01_ai_schema.rs` – Migration v5 → v6, Schema-Gleichheit, Datenhaltung
- `crates/os-cli/tests/acceptance_f01_ai_cli.rs`, `crates/os-mcp/tests/acceptance_f01_ai_tools.rs`, `crates/os-analyze/tests/acceptance_f01_ai_tabs.rs` – Register-Stubs
- Fixtures: `tests/fixtures/ai-acceptance/{profile-ai-full.osconfig,settings-1.0.toml,settings-preview-on.toml}`; außerdem die Archiv-Fixtures `tests/fixtures/migrations/{schema-v5/crawl.ospider,config-v1,config-v2,app-v1/settings.toml}` (Stand vor M4)

## Ergebnis

| Spec-Kriterium | Test | Ergebnis |
|---|---|---|
| E-2 b: Schalter Standard aus | `preview_switch_is_off_by_default` | grün |
| E-2 b: Schalter aus → kein Request, kein Adapteraufruf, keine Verbindung | `switch_off_sends_no_request_and_returns_disabled`, `switch_off_blocks_every_kind_of_request` | grün |
| E-2 b: Schalter an → genau ein Request, Ergebnis unverändert | `switch_on_routes_capable_request_once` | grün |
| E-2 b: kein KI-Schritt bei Schalter aus (`restrict`), nur KI-Teile betroffen | `restrict_disables_only_ai_parts_of_the_run_copy`, `profile_fixture_with_ai_tasks_is_restricted_under_default_settings` | grün |
| AI-001 Bildaufgabe nie an Nicht-Vision-Modell → `Skipped (no capable model)`, 0 Requests | `image_task_never_reaches_model_without_vision` | grün |
| AI-001 Routing nur bei allen Fähigkeiten (je Spalte) | `every_missing_required_capability_blocks_routing`, `schema_request_needs_structured_output`, `capability_check_requires_all_requested_columns` | grün |
| AI-001 Matrix „OpenAI Decisions – Decisions (Text und Bild)“ | `decisions_model_accepts_text_and_image_decision_requests` | **rot** (Befund 1) |
| AI-001 `usage` vier Felder, fehlend = `null` | `usage_has_the_four_spec_fields_and_missing_means_null` | grün |
| AI-020 Statuswerte vollständig und exakt (23 Werte, 13 Zeilen, erschöpfende Zuordnung) | `every_status_value_has_the_exact_spec_text`, `timeout_status_carries_configured_seconds`, `plain_status_values_roundtrip_through_their_spec_text` | grün |
| AI-020 Filter „Errors“ = `Error: …` + `Quota exceeded` + `Budget exceeded` | `errors_membership_is_exactly_error_quota_and_budget` | grün |
| AI-020 Anbieterergebnisse → Statuswerte; 429/529 endgültig behalten HTTP-Code | `provider_outcomes_map_to_spec_status_values`, `final_rate_limit_and_overload_keep_http_code` | grün |
| AI-013 µUSD-Ganzzahl, 1 USD = 1.000.000 | `micro_usd_is_an_integer_with_one_million_per_dollar` | grün |
| AI-013 Standard Crawl-Budget 5,00 USD, unbegrenzt aus, Teilbudgets leer, Warnschwelle 80 % | `budget_defaults_follow_the_spec` | grün |
| AI-013 Budget 0 bleibt 0, nie unbegrenzt | `zero_budget_stays_zero_and_not_unlimited` | grün |
| AI-019 Richtlinie Standard „Keine Einschränkung“, drei Werte | `policy_default_is_unrestricted_and_all_three_roundtrip` | grün |
| AI-012/017/006/024/INTEG-179 Standardwerte App-Einstellungen (1,00 USD, Token-Zählung aus, Schwärzung an, 120 s/300 s, 4/1 gleichzeitig) | `ai_app_settings_defaults_follow_the_spec` | grün |
| INTEG-180 genau zehn `embeddings.*`-Schlüssel, Standardwerte 0,95/0,4/nur indexierbar | `embeddings_section_has_exactly_the_spec_keys_and_defaults`, `embeddings_keys_from_spec_load_without_warnings` | grün |
| Rundreise Profil (Fixture, Datei, stabil) | `full_ai_profile_fixture_roundtrips_losslessly_through_a_file` | grün |
| Rundreise App-Einstellungen / Schalter | `preview_switch_roundtrips_through_settings_file`, `ai_app_settings_roundtrip_losslessly` | grün |
| Alte Profile/Einstellungen ohne KI-Abschnitt laden fehlerfrei | `legacy_profile_without_ai_section_loads_cleanly`, `version_one_profile_loads_with_ai_off`, `legacy_settings_without_ai_section_load_with_preview_off` | grün |
| AI-011 Profil ohne Schlüssel | `saved_ai_profile_contains_no_api_key_fields` | grün |
| Datenhaltung: Schema 6, 13 Tabellen | `fresh_file_has_schema_6_and_all_ai_tables` | grün |
| Migration v5 → v6: Daten erhalten, Backup byte-gleich, neue Tabellen leer | `v5_archive_migrates_to_v6_without_losing_data` | grün |
| Frische DB = migrierte DB | `fresh_and_migrated_schema_are_identical` | grün |
| Keine Schlüssel in der Crawl-Datei | `no_ai_table_stores_credentials` | grün |
| `api_budget` nur Grenzwerte, Ganzzahl | `api_budget_holds_limits_only_as_integers` | grün |
| AI-014 Protokollspalten in `ai_requests` | `ai_requests_covers_the_ai_014_log_columns` | **rot** (Befund 2) |
| AI-014 `Price Source` ∈ {api, table, user, estimate} | `price_source_accepts_exactly_the_four_spec_values` | grün |
| AI-031 Fragenversionen unveränderlich | `stored_question_versions_are_immutable` | grün |
| Neue Tabellen mit Alt-URLs nutzbar (float32-Vektoren) | `migrated_file_accepts_prompt_results_and_embeddings_for_existing_urls` | grün |
| AI-022/OUT-201 KI-Optionen registriert, `--ai-batch auto\|on\|off` | `all_ai_options_from_the_spec_are_registered`, `ai_options_parse_with_spec_value_forms`, `ai_batch_accepts_exactly_auto_on_off` | grün |
| AGENT-030 vier KI-Werkzeuge in Klasse `cost` mit `confirm` | `ai_tools_are_registered_in_class_cost`, `ai_tools_accept_a_confirm_flag` | grün |
| AI-020/INTEG-174/AI-070 KI-Tabs mit Filter „Errors“, Filter „AI Suggestions“ | `ai_tabs_are_registered_with_an_errors_filter`, `ai_suggestions_tab_has_the_spec_filters` | grün |

Summe: 49 Tests, 47 grün, 2 rot.

## Befunde

1. **Bildeingabe an OpenAI Decisions wird abgewiesen (AI-001, Matrix).** Die Spec gibt OpenAI Decisions die Fähigkeiten „Decisions (Text und Bild)“; die Spalte `Vision` ist für diesen Anbieter nicht vorgesehen. Ein Modell mit genau `decisions = true` und einer Decision-Frage mit Bild (Aufgabentyp `decision-vision`, AI-011) ergibt `Outcome::Status(Skipped (no capable model))`, weil `Request::requirements()` bei jedem Bild `vision` verlangt. Entweder bindet das Routing Bild + `decisions` an die Decisions-Fähigkeit, oder es wird festgelegt (Spec/K02-Katalog), dass Decisions-Modelle zusätzlich `Vision` melden. Im zweiten Fall weicht die Matrix-Anzeige von AI-001 ab → Entscheidung nötig.
2. **`ai_requests` hat keine Spalte `Refusal Reason` (AI-014).** AI-014: „Refusal-Texte stehen gekürzt (≤ 500 Zeichen) in einer eigenen Spalte `Refusal Reason`.“ Vorhanden ist nur `error_message`. Da die Tabelle mit Migration 0006 festgelegt wird, braucht ein späteres Nachziehen eine weitere Migration. AI-014 gehört nicht zu den Spec-IDs von F01; der Befund betrifft das Schema `ai_*`, das zum Umfang von F01 gehört.

## Hinweise (kein roter Test)

- `AiStatus` serialisiert `Timeout` und `HttpError` per serde als Objekte (`{"Timeout":{"seconds":30}}`), nicht als Spec-Text; nur `Display` liefert `Error: Timeout nach 30 s`. Landet `AiStatus` über serde in einer Statusspalte, in IPC oder in MCP (AGENT-030: „gleiche Statuswerte nach AI-020“), steht dort kein Wert der Tabelle. Die Spec legt die Serialisierungsform nicht fest; Empfehlung: einheitlich der Spec-Text.
- `ProviderError::RateLimited`/`Overloaded` werden zu `Error: 429 Rate limit` bzw. `Error: 529 Overloaded`; `<Meldung>` ist damit ein fester Text, nicht die Meldung des Anbieters. Geprüft wird nur der HTTP-Code.
- Beim Öffnen der v5-Archivdatei ergänzt `crawl_meta` die Schlüssel `analysis_auto`, `config_sha256` und `user_agent`. Das ist kein Datenverlust; bestehende Einträge bleiben erhalten. Mein erster Testentwurf war hier zu streng und wurde auf „nichts geht verloren“ korrigiert.

## Spec-Lücken / nicht prüfbar in F01

- **Fähigkeitsmatrix je Anbieter (AI-001, erstes Kriterium):** Die Adapter sind Stubs (K01–K03) und die Fähigkeiten kommen aus Live-Quellen bzw. dem Katalog (K04). F01 hat keine statische Matrix, die sich gegen die Spec-Zeilen prüfen ließe.
- **Contract-Test über alle Adapter, `usage` je Adapter, Paritäts-Prompts (AI-001):** erst mit den Adaptern prüfbar.
- **Budget-Rechenregeln (AI-013: Reservierungsformel × 1,25, Teilbudgets, Nebenläufigkeit, `over_reservation`):** `BudgetLedger` ist in F01 nur Trait ohne Implementierung (K05). Prüfbar waren nur Typen und Standardwerte.
- **Darstellung Transportfehler ohne HTTP-Antwort (K4-15):** offen. `ProviderError::Transport` hat keinen Status und wurde nicht getestet.
- **Schalter und MCP/CLI:** Plan E-2 b sagt nicht, ob KI-Werkzeuge in `tools/list` bzw. KI-Optionen im CLI bei Schalter aus verborgen oder abgewiesen werden. Nicht getestet.
- **AI-020 „Errors“ je KI-Tab:** Unklar, ob „AI Search“ (AI-120 ff., ohne Anbieter und ohne AI-020-Status) ein KI-Tab im Sinne von AI-020 ist. Geprüft wurden nur „AI“, „AI Decisions“ und „AI Suggestions“.
- **Weitere Einstellungen ohne Gegenstück in F01:** Schutzschalter je Anbieter (AI-017, Standard an), Reparaturversuche (AI-021, Standard 1), Batch-Modus/Abfrageintervall (AI-016), Stichprobengröße (AI-023), „Cache verwenden“ (AI-015). Die Spec ordnet sie keinem Paket zu; gehören sie in Profil oder App-Einstellungen, fehlen sie noch.
- **`--ai-sample`:** steht im Plan, aber in keiner Spec-Stelle als CLI-Name. Nicht getestet.
- **Wertebereiche** (Warnschwelle, Budget 0–10.000 aus D4S, Timeout 10–3.600): Validierung in F01 nicht verlangt, nicht getestet.

## Ausgeführt

- Lokal: `cargo fmt --all --check` grün.
- Mac mini (Baum `/Volumes/osbuild/openspider/wt/ai-f01`, nur die neuen Dateien per rsync, jeder cargo-Befehl unter `lockf heavy.lock`): `cargo test --locked -p <crate> --test <datei>` für alle sieben Dateien; `cargo clippy --locked -p os-ai -p os-model -p os-store -p os-cli -p os-mcp -p os-analyze --all-targets -- -D warnings`.
- Nicht ausgeführt: Workspace-Gate, `npm`-Tests (keine App-Tests geschrieben).
