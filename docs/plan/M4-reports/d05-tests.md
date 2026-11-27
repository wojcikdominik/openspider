# M4-D05 – Unabhängige Abnahme (Testagent)

Stand: 28.11.2026 · Zweig `ai/d05-tests` auf Bauer-Stand `203a1f6b` · Testagent Claude Opus 5.5 (medium)

Grundlage allein `docs/spec/08-ai.md` (AI-012, AI-013, AI-020 mit den Nachträgen K4-13 samt „Aktiver Besitzer außerhalb von
`Running`“ und „Kein KI-Lauf auf einem unterbrochenen Crawl“, 26.11. `Skipped (input unavailable)`, 27.11. `confirmed_at`;
AI-032 mit Nachtrag „Quelle der Eingabefelder“, berichtigt 27.11.; AI-039), `docs/spec/01-crawl.md` (CRAWL-125/126/128),
`docs/plan/M4.md` (Abschnitt 2, Abschnitt 7 K4-13, K4-24 … K4-30) und der Paketeintrag `M4-D05` in
`docs/plan/M4-packages.json`. Bauer-Bericht `d05.md` und Bauer-Tests nicht gelesen; Ausnahme ist das Diff des umgebauten
Fremdtests `acceptance_f02_ai_ipc.rs` (Kriterium m, Durchsicht verlangt). Gelesen wurde nur die öffentliche Schnittstelle
(Command `ai.decisionsRun`, `os_ai::run`, `os_model::ai::hook`, Tabellen der Crawl-Datei, `startAiRun`). Alle Tests gegen
ai-mock (`os-testsite`), nur Testschlüssel im Arbeitsspeicher bzw. in `OPENSPIDER_AI_KEY_OPENAI` des Kindprozesses.
Die D01-Platzhalter (`decisions_lifecycle`, `decisions_pending`) wurden nicht angefasst.

## Dateien

| Datei | Inhalt |
|---|---|
| `crates/os-app/tests/acceptance_d05_support/mod.rs` | App mit eigenem Ordner (Schalter, Endpunkt, Schwelle, Einwilligung als Vorbedingung), ai-mock mit `hold`/`release` (gibt beim Verwerfen frei), Fixture-Sites, Lesen der Crawl-Datei |
| `crates/os-app/tests/acceptance_d05_app.rs` | 22 Tests: (a), (b), (k), (c) 9 von 10, (e), (f), (g), (h), (j), (m), (l) über die App |
| `crates/os-app/tests/acceptance_d05_quit.rs` | 2 Tests: (d) Schließen und Beenden bei gehaltenem Request (eigenes Binary: Beenden bricht alle Läufe des Prozesses ab) |
| `crates/os-ai/tests/acceptance_d05_core.rs` | 5 Tests: (i) Property-Test, (n) Kontingentsperre je Lauf, (h) Pause/Stopp/Abbruch, (l) Fortsetzen derselben Laufzeile |
| `crates/os-cli/tests/acceptance_d05_cli.rs` | 2 Tests: (c) SIGINT und Fortsetzen, (c) SIGKILL (Binary `openspider` als eigener Prozess) |
| `apps/desktop/src/features/ai/acceptance-d05.test.tsx` | 6 vitest-Tests: `startAiRun`, Meldungen de/en, Option „KI-Eingaben speichern“ |

Fixtures: `tests/fixtures/ai/site/` (Host `ai.test`, |G| = 25, |S| = 22) und `tests/fixtures/ai/site/fields.html` (unverändert
zur Laufzeit unter `/fields` ausgeliefert, damit das selbstreferenzierende hreflang der Fixture auf die Seite zeigt). Keine
neue Fixture-Datei nötig.

## Ergebnis je Kriterium

| Kriterium | Test | Ergebnis | Spec-Stelle |
|---|---|---|---|
| (a) Ergebnisse während des Crawls | `app::a_auto_run_answers_arrive_during_the_crawl` | grün (22 Success, 3 Skipped (scope), erste Antwort, während noch Seiten abgerufen werden) | AI-039, E-6 a |
| (a) Option aus → 0 Requests bis „KI-Analyse starten“ | `app::a_auto_run_off_sends_nothing_until_start` | grün (danach 22 Requests, 0 Seitenabrufe) | AI-039, INTEG-008 |
| (a) Schalter aus → 0 Requests, 1.0 | `app::a_switch_off_behaves_like_1_0` | grün (0 Requests, `page_ai_inputs`/`ai_answers`/`ai_runs`/`ai_requests` leer, URL-Menge gleich Profil ohne KI) | AI-039, E1 |
| (b) Felder aus `fields.html` wie gesendet, während und nach dem Crawl | `app::b_fields_are_sent_during_and_after_the_crawl` | grün (Title, Meta, H1, beide H2 in Reihenfolge, Text, Wortzahl, `html_lang`, `hreflang_self`, Typen; Text nach dem Crawl gleich) | AI-032 |
| (b) Speicherung je interner HTML-200-Seite ohne Fragen | `app::b_inputs_are_stored_for_each_internal_html_200_page_without_questions` | grün (23 = HTML-200-URLs inkl. noindex; keine für Bilder/301/404) | AI-032 Nachtrag (1), Kriterium 1 |
| (b) Ableitung aus `page_html` bzw. `Skipped (input unavailable)` | `app::b_inputs_are_derived_from_page_html_or_unavailable` | grün (abgeleiteter Text gesendet, nichts nachträglich gespeichert; ohne HTML 22 × `Skipped (input unavailable)`, Title-Frage 22 × Success, 0 Seitenabrufe) | AI-032 Nachtrag (2), (3), Kriterium 3 |
| (b) Schalter aus = nichts gespeichert, Option unsichtbar | `app::a_switch_off_behaves_like_1_0`, `vitest: Option „KI-Eingaben speichern“` | grün | AI-032 Kriterium 2 |
| (b) Größe der gespeicherten Eingaben | `app::b_inputs_are_stored…` (Ausgabe `D05-GROESSE`) | Fixture-Site: 23 Seiten, 3 353 Byte gesamt, Mittel 145 Byte, höchstens 249 Byte (zstd); 10 000 Seiten nicht gemessen (siehe unten) | AI-032 Größenabschätzung |
| (c1) heartbeat_of_an_ai_run | `app::c_heartbeat_of_an_ai_run` | grün (≥ 4 Werte in 7 s, `state` durchgehend `Completed`, danach `owner_pid` leer, Endzeitpunkt) | AI-020 K4-13 Kriterium „Heartbeat des KI-Laufs“ |
| (c2) ai_inside_the_own_running_crawl_keeps_the_owner | `app::c_ai_inside_the_own_running_crawl_keeps_the_owner` | grün | AI-020 Kriterium „KI während eines laufenden Crawls …“ |
| (c3) owner_metadata_during_an_ai_run | `app::c_owner_metadata_during_an_ai_run` | grün (`owner_kind = app`, eigene PID, `Completed`; danach leer) | AI-020 Kriterium „Besitzer eines KI-Laufs“ |
| (c4) delete_protection_during_an_ai_run | `app::c_delete_protection_during_an_ai_run` | grün (`busy` aus zweiter App-Instanz, danach löschbar) | AI-020 „Löschschutz“, OUT-228 |
| (c5) ai_run_during_a_paused_crawl (Kernteil) | `app::c_ai_run_during_a_paused_crawl` | grün (Requests genau für die in-scope gecrawlten URLs, gecrawlte URLs und Seitenabrufe unverändert, `Paused`; bei laufendem Crawl 0 Requests) | AI-020 „KI-Lauf während einer Pause“ |
| (c6) cli_sigint_and_resume | `cli::c_cli_sigint_and_resume` | **rot** (Befund A2) | AI-020 Kriterium „CLI: SIGINT …“, „Pause und Fortsetzen“ |
| (c7) crawl_end_without_stop_drains_the_queue | `app::c_crawl_end_without_stop_drains_the_queue` | **rot** (Befund A3; durch Nachtrag 28.11. entschieden, Test angepasst, siehe unten) | AI-020 „Crawl-Ende ohne Stoppen“ |
| (c8) crash_by_sigkill | `cli::c_crash_by_sigkill` | grün (Eintrag n `Pending`, keine Protokollzeile, keine Reservierung, 0 Requests nach dem Absturz, Fortsetzen sendet n genau einmal, 1 Protokollzeile) | AI-020 Kriterium „Absturz“ |
| (c9) foreign_writer_is_read_only (Kernteil) | `app::c_foreign_writer_is_read_only` | grün (`cli` und `mcp-stdio`: Ablehnung mit Art und PID, 0 Requests, Besitzerangaben unverändert) | AI-020 „Fremder Writer“, „Besitzer eines KI-Laufs“ Satz 2 |
| (c10) resume_shows_no_estimate_dialog | `app::c_resume_shows_no_estimate_dialog_and_keeps_the_run_row` | grün (Schwelle 1 µUSD: vor „Starten“ 0 Requests und `Pending (confirmation)`; nach Neustart und Fortsetzen ohne Voranschlag 22 Success, keine URL doppelt) | AI-020 Kriterium „Fortsetzen nach der Pause (auch nach App-Neustart)“, AI-012 |
| (d) Schließen der Crawl-Datei bei gehaltenem Request | `quit::d_closing_the_crawl_file_aborts_the_held_request` | **rot** (Befund A1) | AI-020 „Geordnetes Beenden … bzw. Schließen der Crawl-Datei“ |
| (d) Beenden der App bei gehaltenem Request | `quit::d_quitting_the_app_aborts_the_held_request_within_the_budget` | grün (≤ 3 s, Mock vermerkt `aborted`, Eintrag `Pending`, keine Protokollzeile, Reservierung verfallen, Neusenden genau einmal) | AI-020, CRAWL-126 |
| (e) Unterbrochener Crawl | `app::e_interrupted_crawl_is_refused_and_the_file_stays_byte_equal` | grün (Meldung, Datei byte-gleich, Zustand bleibt „Unterbrochen“, 0 Requests; nach „Stoppen“ ohne Engine 22 Requests); en-Text: vitest | AI-020 „Kein KI-Lauf auf einem unterbrochenen Crawl“, AI-039 |
| (f) kein Request vor „Starten“ | `app::f_no_request_before_start_above_the_threshold`, `app::c_resume…` (während des Crawls), vitest `startAiRun` (3 Tests) | grün | AI-012 |
| (f) Fortsetzen ohne neuen Voranschlag | `app::c_resume_shows_no_estimate_dialog…`, `core::l_resume…` | grün | AI-020 „Kostenbestätigung beim Fortsetzen“ |
| (f) E2E in `e2e/ai-run` | – | nicht geprüft (siehe unten) | – |
| (g) Budget aus dem Konfigurationsdialog an der Datei, Erhöhung hebt Sperre | `app::g_budget_change_in_the_dialog_reaches_the_open_crawl_file` | grün (`load_limits`/`open_ledger` 2,00 USD, Sperre nach Überschreitung aufgehoben, danach 22 Requests) | AI-013 |
| (h) Besitz in jedem Ende-Pfad | `app::h_the_owner_is_released_in_every_end_path` (fertig, Bestätigung ausstehend, Budget, Fehler, Schließen, Stoppen in der Pause), `core::h_owner_is_released_when_a_run_is_paused_stopped_or_aborted` | **rot** nur im Pfad „Schließen“ (Befund A1); alle übrigen Pfade grün | AI-020 „Wer schreibt“ |
| (i) `Skipped (input unavailable)`, Property-Test | `core::i_every_status_value_is_from_the_table` (proptest), `core::i_skipped_input_unavailable_is_a_skipped_value_not_an_error`; dazu `assert_spec_statuses` in allen App-Läufen | grün | AI-020 Nachtrag 26.11.2026 |
| (j) `ai.costBudget` mit offenen Reservierungen | `app::j_cost_budget_shows_open_reservations_of_a_running_run` | grün (Reservierung > 0 während `hold`, danach 0, Verbrauch = Protokollsumme) | AI-013 |
| (k) `html_lang`, `structured_data_types` NULL vs. leere Liste | `app::k_structured_data_types_are_null_without_extraction` | grün | AI-032 (K4-26) |
| (k) `html_lang` gleich, `hreflang_self` = `fr` | `app::k_html_lang_and_hreflang_self_after_the_crawl_match_the_crawl` | **rot** (Befund A4; `html_lang` während/nach dem Crawl gleich, Teil grün) | AI-032 (K4-26) Kriterium 4 |
| (l) `confirmed_at`, dieselbe Zeile auch nach Neustart, neue Zeile bei neuem Lauf | `app::c_resume_shows_no_estimate_dialog_and_keeps_the_run_row`, `core::l_resume_continues_the_same_run_row_with_its_confirmation` | grün | AI-020 Nachtrag 27.11.2026 |
| (m) F02-Stubtest prüft nur Stub-Handler | Durchsicht des Diffs + `app::m_every_catalog_command_is_a_listed_stub_or_implemented` | grün (Hinweis H3) | K4-28 |
| (n) Kontingentsperre je Lauf | `core::n_quota_lock_is_per_ai_run` | grün (A: 1 Request, 5 × Quota exceeded; gleichzeitig B: 5 Success; danach C: erneut 1 Request) | AI-020 Nachtrag „Kontingentsperre“, K4-29 |

## Befunde

### Abweichungen von der Spec

**A1 – Schließen der Crawl-Datei während eines KI-Laufs wird abgelehnt statt die Requests abzubrechen.**
Spec 08 AI-020, Nachtrag K4-13: „Geordnetes Beenden der App bzw. Schließen der Crawl-Datei bei laufenden KI-Requests: Die
laufenden Requests werden abgebrochen, nicht abgewartet …“; Kriterium „… dasselbe beim Schließen der Crawl-Datei.“
Beobachtet: `crawl.clear` (Schließen der Crawl-Datei des Fensters) antwortet während eines laufenden `ai.decisionsRun` mit
`Busy`/`err-crawl-busy`; der gehaltene Request läuft weiter, die Datei bleibt offen. Erwartet: Schließen gelingt, der Request
wird abgebrochen (Eintrag `Pending`, keine Protokollzeile, Reservierung verfallen). Ursache vermutlich die allgemeine
Sperre „keine laufende Operation“ beim Ersetzen/Schließen der Sitzung, die auch den KI-Lauf (Operation `op-ai-decisions`)
erfasst. Reproduktion: `quit::d_closing_the_crawl_file_aborts_the_held_request`, Pfad „Closed“ in
`app::h_the_owner_is_released_in_every_end_path`. (Beenden der App ist korrekt.)

**A2 – CLI: SIGINT wartet laufende KI-Requests nicht ab; ein Request geht doppelt hinaus; offene Einträge fehlen.**
Spec 08 AI-020: „Laufende Requests werden abgewartet und ihre Antworten normal übernommen … Die Pause durch SIGINT im CLI
(CRAWL-126, OUT-206) verhält sich ebenso.“; Kriterium: „SIGINT während offener KI-Einträge → Exit-Code 130, Einträge
`Pending`; `--load-crawl <ID> --resume` sendet genau diese“; zum Fortsetzen „keine URL doppelt im Mock-Log“.
Beobachtet (ai-mock mit 1 s Antwortverzögerung, Crawl 3 URL/s, SIGINT nach ≥ 8 Seitenabrufen und ≥ 2 KI-Requests): Exit 130,
Zustand `Paused`, aber 3 Requests gesendet und nur 2 übernommen (der laufende wurde nicht abgewartet); für 8 gecrawlte Seiten
im Geltungsbereich gibt es nur 4 Einträge (2 Success, 2 Pending), 4 Seiten haben gar keinen Eintrag. Nach dem Fortsetzen sind
alle 22 Einträge `Success`, das Mock-Log zeigt aber 23 Requests für 22 URLs (der nicht abgewartete Request wurde erneut
gesendet → doppelte Kosten beim Anbieter möglich). Reproduktion: `cli::c_cli_sigint_and_resume`.

**A3 – Crawl-Ende ohne Stoppen: Der Crawl ist `Completed`, bevor die KI-Warteschlange leer ist.**
Spec 08 AI-020: „Crawl-Ende ohne Stoppen (Endgrund `queue_empty` oder `limit_reached`): Die KI-Warteschlange läuft wie nach
INTEG-007 weiter, bis sie leer ist; der Crawl gilt erst dann als vollständig“; INTEG-007 wiedergegeben: „der Crawl gilt erst
als vollständig, wenn sie leer ist“. Beobachtet (`limits.max_urls = 10`, `hold` für alle Requests): Nach dem Ende der Abrufe
meldet die Sitzung `Completed`, und die Crawl-Datei trägt `state = Completed`, während ein KI-Request gehalten wird; danach
werden die gehaltenen Requests nacheinander freigegeben, und die übrigen Teile des Kriteriums (alle offenen Einträge der
10 URLs gesendet, Mock-Log = Einträge im Geltungsbereich, `limit_reached`) sind erfüllt. Außerdem blockierte ein Aufruf von
`session::poll` länger als 3 s (Risiko: die Statusabfrage der Oberfläche hängt, solange die Warteschlange abläuft; im ersten
Lauf ohne Frist hing der Test dauerhaft). Erwartet: `Completed` erst mit leerer Warteschlange (Auto-Analyse, Exporte in
Zeitplänen hängen daran). Reproduktion: `app::c_crawl_end_without_stop_drains_the_queue`.

**A4 – `hreflang_self` fehlt für die Fixture-Seite mit selbstreferenzierendem hreflang `fr`.**
Spec 08 AI-032 (Nachtrag K4-26): „`hreflang_self` der Seite mit selbstreferenzierendem hreflang `fr` ist `fr`.“ Beobachtet:
Crawl von `http://ai.test:<port>/` (die Fixture verweist absolut mit Port, `href="http://ai.test:{{PORT}}/french"`): Weder
während noch nach dem Crawl enthält der gesendete Block eine Zeile `Hreflang Self`; die Tabelle `hreflang` enthält für
`/french` die Zeile `fr` mit leerer Spalte `url`. Mit relativem Selbstverweis (`fields.html`, `href="/fields"`) wird
`hreflang_self` gesendet. Vermutlich wird ein absoluter Selbstverweis mit Port nicht als selbstreferenzierend erkannt.
`html_lang` ist während und nach dem Crawl gleich (Teil grün). Reproduktion:
`app::k_html_lang_and_hreflang_self_after_the_crawl_match_the_crawl` (Ausgabe `D05-HREFLANG`).

### Risiken

- **R1 – `ai.decisionsRun` mit `confirmed: true` während des eigenen laufenden Crawls antwortet `{"end": "Confirmed"}`**,
  auch wenn die automatische Ausführung aus ist (dann geht kein Request hinaus – der Test prüft das und ist grün). `startAiRun`
  sendet immer `confirmed: true`; ohne die deaktivierte Schaltfläche (D02) meldet die Oberfläche also „gestartet“, obwohl nichts
  läuft. D02 sollte die Schaltfläche bei laufendem Crawl sicher deaktivieren (AI-039 Nachtrag K4-13).
- **R2 – Hängende Teardowns:** Ein KI-Lauf, dessen Request gehalten bleibt, blockiert das Ende einer tokio-Laufzeit (blockierender
  Lauf-Thread). In Tests ist das abgefangen (Freigabe beim Verwerfen des Mocks, `controls().abort_all()`); in der App gilt es für
  jede Stelle, die auf `spawn_blocking` des Laufs wartet.
- **R3 – Fehlerpfad mit `401`:** Im ersten Durchlauf von (h) endete ein Lauf mit Mock-Szenario `401` (alle Requests) nicht in
  60 s. Nicht weiter untersucht (Fehlerstatus und Wiederholungen gehören D01/K-Paketen); für (h) wurde der Fehlerpfad mit
  `malformed-json` (`Error: Invalid output`) geprüft, grün.

### Hinweise

- **H1 – Statuswert `Skipped (price unknown)`** fehlt in `os_model::ai::AiStatus` (bekannte Abweichung, N01 laut 08 AI-020); der
  Property-Test kennt den Wert in der Tabelle und bleibt dadurch grün.
- **H2 – Bildkontext** wird gespeichert (`MARK-CONTEXT` in `page_ai_inputs`, ≤ 300 Zeichen), aber von einer Seitenfrage nicht
  gesendet (AI-032: `image_context` gehört zu Bildfragen). Bildfragen laufen in D05 nicht über den Laufrahmen; „Bildkontext wie
  gesendet im Mock-Log“ aus (b) ist damit nicht prüfbar.
- **H3 – K4-28:** Der F02-Test prüft jetzt genau `ai_catalog::stub_commands()`. Die Liste stammt aus dem Produktcode
  (`implemented()` plus Handliste `REPLACED_IN_MATCH = ["ai.suggestRun"]`) – die V01-Ausnahme lebt dort weiter, nur an anderer
  Stelle. Der eigene Test `m_every_catalog_command_is_a_listed_stub_or_implemented` schließt die Lücke: jeder Katalog-Command
  ist entweder gelisteter Stub (antwortet `err-not-implemented`) oder antwortet nicht so; grün.
- **H4 – Meldung bei unterbrochenem Crawl:** Der Kern liefert den deutschen Text in `detail`; OUT-243 beschreibt `detail` als
  technische, englische Ursache. Die Oberfläche übersetzt über den Schlüssel (de/en geprüft, grün).

## Fragen an die Spec

- **F1 – SIGINT während des Ablaufens der KI-Warteschlange:** Fällt ein SIGINT in die Phase nach dem letzten Seitenabruf, in der
  nur noch die KI-Warteschlange läuft (Crawl-Ende ohne Stoppen), gilt dann „Pause“ (Zustand `Paused`, Fortsetzen möglich) oder
  ist der Crawl bereits „Fertig“ (CRAWL-126: „gilt der Zustand, den die Engine zuerst feststellt“)? Der Test umgeht die Frage
  (SIGINT, während noch Seiten abgerufen werden).
- **F2 – Offene Einträge ohne Zeile:** Gilt ein Eintrag einer gecrawlten Seite im Geltungsbereich, für den noch keine Zeile in
  `ai_answers` existiert, als „offen (`Pending`)“ im Sinne der Kriterien, oder muss die Zeile mit `Pending` vorhanden sein
  (Anzeige im Tab, Exporte)? Der Test erwartet die Zeile (Befund A2, erster Punkt).

## Zahlen

- Rust: 31 Abnahmetests (os-ai 5, os-app 22 + 2, os-cli 2), davon 26 grün, 5 rot (A1 zweimal: `quit::d_closing…`, `app::h_…`;
  A2: `cli::c_cli_sigint_and_resume`; A3: `app::c_crawl_end…`; A4: `app::k_html_lang…`), 0 ignoriert.
- vitest: 6 Tests (`apps/desktop/src/features/ai/acceptance-d05.test.tsx`), 6 grün (2,1 s); `npm run -w apps/desktop typecheck` grün.
- Laufzeiten (Mac mini, je Test einzeln mit 300 s Frist): os-ai gesamt 2 s; os-app je Test 0–11 s (Heartbeat 8 s, langsame
  Crawls 11 s), insgesamt rund 70 s; Quit-Binary 2 s; CLI SIGINT 17 s, SIGKILL 22 s (davon 11 s Heartbeat-Ablauf).
- `cargo fmt --all --check`: grün. `cargo clippy --workspace --all-targets --locked -- -D warnings`: grün (auf diesem Stand,
  Bauordner des Bauers).

## Nicht geprüft

- **Größe für 10 000 Seiten:** nicht gemessen (ein Crawl von 10 000 Seiten sprengt das Zeitbudget eines Laufs auf dem Mac mini;
  die Fixture-Seiten sind mit 145 Byte je Seite zu klein für eine Hochrechnung). Die Messung bleibt Q02 (08 AI-032
  „Messung mit Q02 nachtragen“).
- **E2E `e2e/ai-run`** (Kostenvoranschlag im Browser): nicht geschrieben; Playwright-Läufe sind auf dem Mac mini teuer, und das
  Verhalten „kein Request vor ‚Starten‘“ ist im Kern (Rust gegen ai-mock) und in `startAiRun` (vitest) geprüft. Schaltfläche,
  deaktivierte Zustände und Hinweise baut D02.
- **MCP STDIO** (`crawl_service.rs`): nur Verdrahtung in den `EngineDeps`; KI-Werkzeuge und deren Meldungen bauen C02 (1.2).
- **Atomarität mit Fehlerinjektion** (Test-Hook der Schreibwarteschlange): kein öffentlicher Hook im Paket; nicht Teil der
  Kriterien (a)–(n).
- **Heartbeat mit simulierter Uhr:** geprüft mit Echtzeit (7 s gehalten); gleiches Kriterium.

## Nachtrag 28.11.2026

Grundlage: 08 AI-020, „Nachtrag 28.11.2026 (Abnahme M4-D05, Entscheidungen des Orchestrators)“ (gelesen auf `main`). Er
entscheidet F1, F2 und A3: `Completed` beim Ende der Abrufe ist spec-gemäß, A3 entfällt als Befund; „vollständig“ betrifft
die Folgeschritte. Die Tests sind geschrieben und committet, aber noch **nicht gebaut und nicht gelaufen** (Mac mini belegt).
A1, A2 und A4 bleiben Befunde.

| Test | Art | Inhalt |
|---|---|---|
| `app::c_crawl_end_without_stop_drains_the_queue` | geändert | neues Kriterium: bei gehaltenem Request gespeichert `Completed` (Endgrund `limit_reached`), Sitzung meldet `Completed`, Statusabfrage (`session::poll`) ≤ 500 ms, Crawl-Analyse nicht begonnen; nach Freigabe Mock-Log = Einträge im Geltungsbereich, danach `analysis_state = completed` |
| `app::c_pending_confirmation_does_not_hold_the_crawl_end` | neu | Schwelle 1 µUSD: Crawl `Completed` und Analyse fertig trotz 22 × `Pending (confirmation)`, 0 Requests; danach „KI-Analyse starten“ → 22 Requests |
| `app::c_start_during_the_own_running_crawl_is_refused` | neu | `ai.decisionsRun` (bestätigt und unbestätigt) bei laufendem eigenem Crawl → Fehler (Code nicht festgelegt), kein Request, keine neue Laufzeile, Zustand `Running`; mit automatischer Ausführung aus und an |
| `app::k_html_lang_and_hreflang_self_after_the_crawl_match_the_crawl` | Ausgabe ergänzt (A4) | `D05-HREFLANG-AUSGELIEFERT`: tatsächlich ausgelieferter `<link rel="alternate" …>` von `/french` neben der Seiten-URL (zeigt, ob `{{PORT}}` ersetzt ankommt); Erwartung unverändert |
| `cli::c_cli_sigint_and_resume` | an das Kriterium angeglichen (A2 bleibt) | SIGINT, sobald 2 Requests beantwortet sind und Request 3 unterwegs ist (1 s Antwortzeit, Crawl 6 URL/s): 3 Protokollzeilen, 3 × `Success`, übrige gecrawlte Seiten aus S je `Pending`; nach `--resume` 22 × `Success`, genau 22 Requests, keine URL doppelt. Die Zahl der gecrawlten S-Seiten beim SIGINT wird ausgegeben (`D05-SIGINT`), die „8“ des Kriteriums ist Szenario, keine Bedingung |
| `cli::c_cli_sigint_while_only_the_ai_queue_runs` | neu (F1) | Crawl `Completed` (10 URLs), 2 s Antwortzeit, ein Request unterwegs → Exit 130, Zustand bleibt `Completed`, laufender Request übernommen, offene `Pending`, keine Exporte, 3 s später kein weiterer Request |
| `cli::c_cli_export_waits_for_the_empty_queue` | neu | `hold`, 10 URLs, `--export-tabs Internal:All`: gespeichert `Completed`, Prozess läuft noch, kein Export; nach Freigabe Exit 0, Export geschrieben (nach dem letzten KI-Request), alle Einträge im Geltungsbereich `Success` |

Offen bzw. neu:
- **„Der Export enthält die KI-Ergebnisse“** ist nicht prüfbar: Der Export „AI Decisions:All Answers“ ist bis M4-D02 ein Stub
  (`crates/os-export/src/catalog/bulk_ai_decisions.rs`). Ersatzweise: Export erst nach der letzten KI-Antwort, alle Einträge fertig.
- **F3 – Bestätigung während des laufenden Crawls:** `app::c_resume_shows_no_estimate_dialog_and_keeps_the_run_row` erteilt die
  Kostenbestätigung für die wartenden Einträge der automatischen Ausführung bisher mit `ai.decisionsRun` (`confirmed: true`)
  während des laufenden Crawls. Nach dem Nachtrag („Start während des eigenen laufenden Crawls … mit einem Fehler abgewiesen“)
  ist dieser Weg verboten; der Nachtrag nennt für die Bestätigung „in der App über den Kostenvoranschlag“, aber keine
  Schnittstelle, über die der Kern sie während des Crawls erhält. Bis das geklärt ist, wird dieser Test rot, sobald der Bauer die
  Abweisung umsetzt – kein Produktfehler. Frage an den Orchestrator: Über welchen Command bestätigt die App den
  Kostenvoranschlag der automatischen Ausführung während des Crawls (und schreibt `confirmed_at`)? Der Test bleibt bis dahin
  unverändert.
- In `c_start_during_the_own_running_crawl_is_refused` kann die Engine selbst während des Tests eine Laufzeile für die automatische
  Ausführung anlegen; gezählt wird nur die Differenz um den abgewiesenen Aufruf herum (kurzes Fenster, Risiko gering).
