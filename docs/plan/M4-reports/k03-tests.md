# M4-K03 – Unabhängige Abnahmetests (AI-006, AI-008, INTEG-005 [M4])

Stand 24.11.2026, Zweig `ai/k03-tests` auf Bauer-Stand `624e4996`. Erwartungen allein aus `docs/spec/08-ai.md`
(AI-006, AI-008, dazu AI-001, AI-015, AI-020, AI-021, ai-mock-Konventionen) und `docs/spec/04-custom-integrations.md`
(INTEG-005, INTEG-160, INTEG-179, INTEG-180) sowie den Festlegungen des Orchestrators (kein Gemini; Statuswert
`Error: Connection failed` plus Meldung mit Server-URL als Text; `Usage.input_tokens` = alle Eingabe-Tokens;
Server-URL aus gemeinsamer Einstellung, keine Umgebungsvariable). Bericht und Tests des Bauers nicht gelesen;
Produktionscode nur für die öffentliche Schnittstelle gelesen, nicht geändert.

## Dateien

- `crates/os-ai/tests/acceptance_k03_ollama.rs` – AI-006, INTEG-005 [M4] (16 Tests)
- `crates/os-ai/tests/acceptance_k03_embeddings.rs` – AI-008 (15 Tests)
- `crates/os-ai/tests/acceptance_k03_env.rs` – AI-006 Kriterium 4, Umgebungsvariable (1 Test, eigene Datei wegen Prozess-Umgebung)
- `crates/os-ai/tests/acceptance_k03_support/mod.rs` – Testuhr, ai-mock-Hülle, eigener Testserver mit Zeitstempeln und Zähler offener Anfragen
- `tests/fixtures/ai/k03-acceptance/mock-answers.d/k03-acceptance.json` – eigenes Fragment mit nicht normalisierten Vektoren und Nullvektor (nur über `AiMock::from_fragments` geladen, nicht im gemeinsamen Fragmentverzeichnis)

Testmittel: ai-mock (`os_testsite::mocks::ai`), eigener Testserver auf `127.0.0.1`, für „nicht erreichbar“ ein freigegebener
lokaler Port. Testuhr: Backoff/`Retry-After` vergehen virtuell, Timeouts (≥ 100 s) nie. Kein echter Anbieter, kein
echter Ollama-Server, keine echten Schlüssel.

## Kriterium → Test → Ergebnis

| Kriterium | Test(s) | Ergebnis |
|---|---|---|
| AI-006 K1: Schema im Feld `format` | `schema_task_sends_schema_in_format_field_and_gets_validated_answer` | grün |
| AI-006 K1: Antwort wird validiert (AI-021) | `schema_violation_is_rejected_after_one_repair` (2 Requests → `Error: Invalid output`), `malformed_json_once_then_ok_is_repaired` | grün |
| AI-006 K2: `Cost = 0`, Tokens aus `prompt_eval_count`/`eval_count` | `chat_log_line_has_zero_cost_and_eval_counts` (Testserver 123/45 → Protokollzeile), `chat_usage_matches_mock_eval_counts` (Token-Zählung des Mocks), `embedding_log_line_has_zero_cost_and_prompt_eval_count` | grün |
| AI-006 K3: nicht erreichbarer Server, nacheinander laufende Einträge | `unreachable_server_fails_all_open_entries_without_new_requests` (1. Eintrag 1+5 Versuche, `Error: Connection failed`, `HTTP Status` leer, `transport/connect`; Meldung enthält URL und ist im Statustext enthalten; danach Testserver auf demselben Port: 4 weitere Einträge scheitern, **0** Requests) | grün |
| AI-006 K3: nicht erreichbarer Server, **gleichzeitig wartende** Einträge (Standard 1 gleichzeitige Anfrage) | `unreachable_server_with_queued_entries_retries_only_once` | **rot – Befund B1** |
| AI-006 K3 / AI-020: Embeddings bei nicht erreichbarem Server | `unreachable_server_fails_all_embedding_texts` (130 Texte, alle `Error: Connection failed`, nur 1 Bündel mit Wiederholungen; Protokollzeile `HTTP Status` leer, `Error Code = transport/connect`, `Retries = 5`, `Cost = 0`; Folgelauf: 0 Requests) | grün |
| AI-006 K3: andere Anbieter laufen weiter | `other_providers_keep_running_when_ollama_is_unreachable` (12 Ollama-Fehler, danach OpenAI- und OpenRouter-Embeddings über dieselbe Begrenzung `Success`) | grün |
| AI-006 K4: gemeinsame Server-URL | `changed_shared_server_url_is_used_without_further_input`, `shared_setting_without_url_falls_back_to_integ_160_default`, `adapter_ignores_environment_and_uses_shared_setting` | grün |
| AI-006 K5: höchstens 1 offene Anfrage (Zeitstempel) | `by_default_at_most_one_ollama_request_is_open` (4 Einträge parallel, Server-Zeitstempel ohne Überlappung) | grün |
| INTEG-005 [M4]: Ollama Standard 1 | `ollama_default_concurrency_is_one` (Einstellung, Limits, Timeout 300 s), Gegenprobe `ollama_concurrency_setting_two_allows_two_open_requests` (Einstellung 2 → genau 2 offen) | grün |
| AI-006 Beschreibung / INTEG-160: Modellliste `/api/tags` | `model_list_comes_from_api_tags` | grün |
| AI-008 K1: 130 Texte, Bündel 64 → 3 Requests (64, 64, 2) | `texts_130_with_batch_size_64_give_three_requests_per_provider` (OpenAI, OpenRouter, Ollama; Mock-Protokoll; jeder Text genau einmal in Reihenfolge), `configured_batch_size_is_respected` | grün |
| AI-008 K2: L2-Norm 1 ± 1e-6 bei nicht normalisierten Vektoren | `non_normalized_mock_vectors_are_stored_with_unit_norm` (eigene Fragmente, alle 3 Anbieter, Richtung erhalten), `non_normalized_high_dimension_vectors_are_normalized` (768 Dim., Ollama- und OpenAI-Format) | grün |
| AI-008 K2 Randfall Nullvektor (Beobachtung) | `zero_vector_is_never_stored_with_norm_other_than_one` (hart nur: kein gespeicherter Vektor mit Norm ≠ 1, Nachbartexte erfolgreich) | grün |
| AI-008 Reihenfolge Ergebnis ↔ Eingabe | `each_text_gets_its_own_vector_in_input_order` (Vergleich mit den deterministischen Mock-Vektoren je Text, 3 Bündel, 3 Anbieter), `openai_format_with_reversed_data_is_mapped_by_index` | grün |
| AI-008 K3: Cache-Treffer senden nichts | `cached_texts_are_not_sent_again`, `cache_key_covers_content_model_and_dimensions` (gemischt alt/neu → nur neue gesendet; anderes Modell/andere Dimensionen → neue Requests; Rückwechsel → 0), `crawl_file_cache_survives_reopen` (Cache in der Crawl-Datei, AI-015) | grün |
| AI-008 K4: Bündelfehler → Einzelversuche, nur fehlerhafte Texte mit Fehlerstatus | `context_length_in_batch_fails_only_the_marked_text` (alle 3 Anbieter; nur der markierte Text `Error: 400 …`), `context_length_error_is_confined_to_its_batch` (70 Texte: nur das 2. Bündel wird aufgelöst) | grün |
| AI-008 K5: Anthropic nicht in der Embedding-Modellwahl | `anthropic_never_appears_in_embedding_model_choice`, `live_model_lists_yield_only_embedding_capable_models` (Live-Listen Anthropic + OpenRouter aus dem Mock → nur `mock/embedding`) | grün |
| INTEG-180 (Verweis): leere Eingabe nicht senden | `empty_text_is_not_sent` | grün |

## Befunde

**B1 – Ollama nicht erreichbar: wartende Einträge versuchen es jeweils selbst erneut (AI-006 K3, Orchestrator-Festlegung (2)).**
Spec: „Nicht erreichbarer Server → alle offenen Ollama-Einträge erhalten `Error: <Meldung aus INTEG-160>`“; hart zu prüfen:
„ohne dass für jeden erneut Anfragen an den Server gehen“. Beobachtet: 4 gleichzeitig gestartete Einträge bei Standard
1 gleichzeitiger Anfrage (geschlossener Port) – Versuche je Eintrag `[6, 5, 5, 5]`, also 21 Verbindungsversuche und
20 Backoff-Wartezeiten statt 6 bzw. 5. Ursache laut öffentlicher Schnittstelle/Verhalten: Der Merker „nicht erreichbar“
wird erst nach dem endgültigen Fehler des ersten Eintrags gesetzt; während dessen Backoff gibt die Begrenzung den
einzigen Platz frei, und die wartenden Einträge senden eigene Versuche, bis der Merker greift. Mit echtem Backoff
(1, 2, 4, 8, 16 s) verlängert sich dadurch auch die Laufzeit. Nacheinander ausgeführte Einträge und Embeddings
(sequenzielle Bündel) sind nicht betroffen (grün). Test bleibt rot.

Keine weiteren Abweichungen.

## Hier nicht prüfbar

- AI-006 K3 Anzeige in den Statusspalten der Oberfläche (Übertragung der Meldung in `<Q> Status` einer KI-Tabelle) – braucht Lauf/Speicherung; geprüft nur `status_text` und `FinalError` auf Bibliotheksebene.
- AI-006 K4 „in 04 geändert“: Eine 04-Seite (INTEG-160-Konto in `os-integrations` bzw. UI) mit Ollama-Einstellung existiert im Code nicht; geprüft ist nur, dass der Adapter ausschließlich die übergebene gemeinsame Einstellung `AiAppSettings.provider_endpoints["ollama"]` nutzt.
- INTEG-005 [M4] Anzeige von Standard und Bereich im Reiter „Erweitert“ (UI) – nur Konfigurationswerte geprüft.
- AI-008 Ausgabe „Vektor je (URL, Embedding-Satz) in der Crawl-Datei“ (INTEG-166/180, Speicherung/Anzeige) – gehört laut Code zu M4-E01.
- AI-008 Kosten nach AI-014 für OpenAI/OpenRouter-Embeddings (Preise aus Katalog/API, M4-K04) – nur Ollama `Cost = 0` geprüft.

## Auslegungsfragen

1. **Wortlaut der Meldung (INTEG-160 vs. AI-020):** INTEG-160 nennt `Fehler: Server nicht erreichbar (http://localhost:11434)`, AI-020 den Statuswert `Error: Connection failed`. Der Adapter liefert `FinalError.status = Error: Connection failed`, `message = "Server nicht erreichbar (<URL>)"` und `status_text = "Error: Server nicht erreichbar (<URL>)"` (deutscher Text hinter englischem Präfix). Welcher Text in der Statusspalte steht, ist offen; nicht hart geprüft.
2. **Fehlercode der nicht gesendeten Folgeeinträge:** Einträge, die wegen des Merkers nichts senden, tragen `Error Code = transport/connect`, obwohl kein Verbindungsversuch stattfand. Ob sie überhaupt eine Protokollzeile erhalten sollen, lässt AI-014/AI-020 offen (bei Embeddings: keine Zeile).
3. **Server kommt im selben Lauf zurück:** Nach dem Ausfall bleibt der Adapter für den Lauf gesperrt (auch wenn der Server wieder lauscht). Spec spricht von „allen offenen Einträgen“ – plausibel, aber nicht ausdrücklich.
4. **Zahl der Requests bei Teilfehler:** AI-008 K4 legt nicht fest, ob nach einem Bündelfehler alle Texte einzeln (Ist: 1 + n Requests) oder z. B. per Halbierung erneut versucht werden; nur das Ergebnis je Text ist hart geprüft.
5. **Standard gleichzeitiger Anfragen anderer Anbieter:** INTEG-005 nennt Standard 5 (Bereich 1–50), AI-017 Standard 4 je Anbieter; `ProviderEndpoint::default()` hat 4. Nicht Gegenstand von K03, Widerspruch der Spec.
6. **Bereich für Ollama:** INTEG-005 [M4] nennt für Ollama nur „Standard 1“; der Code führt Bereich 1–50 (`CONCURRENT_REQUESTS_RANGE`). Nicht geprüft.

## Beobachtungen

- Nullvektor: Im Bündel führt er zu einem Bündelfehler und Einzelversuchen; der Text selbst endet (laut Schnittstelle `l2_normalize` → `NotNormalizable`) mit `Error: Invalid output`, die Nachbartexte mit `Success`. Nur die Norm-Folge ist hart geprüft.
- Ollama `/api/embed` wird mit `truncate: false` gesendet (keine automatische Kürzung, passt zu INTEG-180).
- Ollama-Chat ohne Schema: ob `format` fehlt, ist nicht spezifiziert; nicht geprüft.
- Vektoren werden als `f32` gespeichert; Norm in `f64` nachgerechnet liegt bei allen Testvektoren innerhalb 1e-6.
- Nach 12 endgültigen Ollama-Fehlern laufen OpenAI und OpenRouter über dieselbe Begrenzung ungestört (Schutzschalter je Anbieter).

## Abweichungen des ai-mock von der Spec

- Der Mock liefert ohne vordefinierten Vektor immer **normalisierte** Vektoren; „nicht normalisierte Vektoren“ (AI-008 K2) gibt es nur über eigene Fragmente mit `MOCK-ID` und passender Dimension. Deshalb eigenes Fragment unter `tests/fixtures/ai/k03-acceptance/` und eigener Testserver.
- Das Request-Protokoll hat nur Eingangszeitstempel in Millisekunden, kein Antwortende; „höchstens 1 offene Anfrage (Mock-Zeitstempel)“ daher mit eigenem Testserver (Eingang/Antwortende, Zähler offener Anfragen) gemessen.
- `context-length` wirkt je Request über die Markierung `MOCK-SCENARIO:` im Body (passt zur Spec „für einen markierten Text“).
- Ollama-Fehlerkörper `{"error": "<Meldung>"}` ohne Typ (entspricht dem Ollama-Format).

## Testzahlen letzter Lauf (Mac mini, Lauf 3)

- `cargo fmt --all --check`: grün; `cargo clippy --locked -p os-ai --all-targets -- -D warnings`: grün.
- Eigene Tests: 32 (31 grün, 1 rot = B1): `acceptance_k03_ollama` 15/16, `acceptance_k03_embeddings` 15/15, `acceptance_k03_env` 1/1.
- `cargo test --locked -p os-ai --no-fail-fast`: alle übrigen Testziele grün; einziger Fehlschlag ist B1.
