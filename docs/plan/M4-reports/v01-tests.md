# M4-V01 – Unabhängige Abnahmetests (1.1-Etappe)

- Zweig `ai/v01-tests`, Prüfstand `ddbaca5a` (enthält Bauer-Stand `919bf2cb` per `git merge --no-edit ai/v01`; erste
  Runde gegen `f3890fe1`). Spec-Stand `main` `8ae20980`.
- Umfang: AI-070, AI-071, AI-072, AI-075, AI-076 (release_split 1.1) mit den [M4]-Kriterien von UI-004 und UI-037,
  OUT-001 ff./OUT-016; übergreifend AI-013, AI-014, AI-015, AI-017, AI-018, AI-020, AI-021, AI-038. AI-077 (1.2),
  AI-073/AI-074 (V02) nicht geprüft.
- Quellen der Erwartungen: nur `docs/spec/*` und die Festlegungen des Orchestrators im Auftrag. Bauer-Bericht,
  Bauer-Tests und Commit-Texte nicht gelesen (Ausnahme: Durchsicht unten). Beim Lesen der öffentlichen Schnittstelle sind
  mir zwei Modul-Unit-Tests des Bauers kurz vor Augen gekommen (`tabs/ai_suggestions.rs`, `os-mcp/src/tools/mod.rs`,
  jeweils am Dateiende); sie haben keine Erwartung beeinflusst.
- Läufe: Mac mini, Sperre `heavy.lock`, `CARGO_TARGET_DIR=wt/ai-v01/target`, Logs `wt/ai-v01-tests/v01t-log/r1*`, `r2*`.

## Dateien

| Datei | Inhalt |
|---|---|
| `crates/os-ai/tests/acceptance_v01_support/mod.rs` | eigene Crawl-Datei, geskripteter Anbieter hinter `policy::guard::execute`, Tabellen-Pixelmessung, ai-mock mit eigenen Fragmenten |
| `crates/os-ai/tests/acceptance_v01_core.rs` | Prüfregeln, Dublettenprüfung, Runden, Cache, Budget, Stoppen, Atomarität, Review (35 Tests) |
| `crates/os-ai/tests/acceptance_v01_mock.rs` | dieselben Abläufe gegen den ai-mock mit dem Anthropic-Adapter (8 Tests, 1 ignoriert) |
| `crates/os-store/tests/acceptance_v01_store.rs` | Persistenz über Schließen/Öffnen, Commit alles-oder-nichts (2) |
| `crates/os-analyze/tests/acceptance_v01_tab.rs` | Filter, Spalten, Sortierung, Zähler des Tabs (4) |
| `crates/os-export/tests/acceptance_v01_export.rs` | Exporte CSV/XLSX, Spalten, Zeilen, Formelschutz (4) |
| `crates/os-app/tests/acceptance_v01_app.rs` | Commands, Schalter, Sichtbarkeit (App, CLI, MCP), UI-038, Review, Geltungsbereich, Neuprüfung, Fokus (13) |
| `crates/os-app/tests/acceptance_v01_mail.rs` | Mail-Zusammenfassung/Vergleich ohne „AI Suggestions“ bei Schalter aus (1) |
| `crates/os-app/tests/acceptance_v01_e2e_fixture.rs` | Erzeuger und Prüfung der E2E-Fixture (1) |
| `apps/desktop/e2e/ai-suggestions/acceptance-v01.spec.ts` | Playwright gegen die Bridge (7 Tests je Engine) |
| `tests/fixtures/ai/v01-acceptance/` | `states.rs` (Zustands-Fixture), `mock-answers.d/v01-acceptance.json`, `e2e/v01-states.ospider` |

## Testzahlen (Runde 2, Prüfstand `ddbaca5a`, BAUER=`919bf2cb`)

| Ziel | grün | rot | ignoriert |
|---|---|---|---|
| os-ai `acceptance_v01_core` | 35 | 0 | 0 |
| os-ai `acceptance_v01_mock` | 6 | 1 | 1 |
| os-store `acceptance_v01_store` | 2 | 0 | 0 |
| os-analyze `acceptance_v01_tab` | 4 | 0 | 0 |
| os-export `acceptance_v01_export` | 3 | 1 | 0 |
| os-app `acceptance_v01_app` | 12 | 1 | 0 |
| os-app `acceptance_v01_mail` | 1 | 0 | 0 |
| os-app `acceptance_v01_e2e_fixture` | 1 | 0 | 0 |
| Playwright chromium | 7 | 0 | 0 |
| Playwright webkit | 7 | 0 | 0 |

`cargo fmt --all --check` grün; `cargo clippy -p os-ai -p os-store -p os-analyze -p os-export -p os-app --all-targets -D warnings` grün;
`npm run -w apps/desktop typecheck` grün. Gesamt: Rust 64 grün / 3 rot / 1 ignoriert, Playwright 14 grün.

Runde 1 (gegen `f3890fe1`) zeigte zusätzlich rot: MCP `list_catalog` mit „AI Suggestions“ bei Schalter aus – mit `919bf2cb`
behoben (Test prüft jetzt über `os_mcp::tools::offered(switch)`, also die Werkzeugauswahl des Servers). Die übrigen roten
Ergebnisse aus Runde 1 waren eigene Testfehler (Clippy-Ausnahmen für os-app-Tests, `px` als Zahl mit Nachkommastelle in
`detail.serpMeasure`, Fehlerform im Frontend, deutscher Tab-Name, Auswahl während des Tabellen-Nachladens in WebKit).

## Prüfpunkte je Spec-ID

| ID | geprüft | Ergebnis |
|---|---|---|
| AI-070 | Standard-Geltungsbereich Titles = Vereinigung der Problemfilter + `Title Not Matching Content (AI)`, je URL einmal, nur S; Meta `Missing`; genau ein Request je URL und Runde, Kandidaten in Antwortreihenfolge; Fokus-Begriff im Request nur bei URLs mit Eintrag; Kontext (Title, H1, Meta, Text, Seitensprache) im Request; Persistenz über Schließen/Öffnen; Filter des Tabs; Schalter aus → kein Request, Vorschau-Fehler; App-Lauf → `Pending (confirmation)` mit Voranschlag; Fokus-CSV-Import | grün |
| AI-071 | Pixelbreite = UI-038 (Testvektoren, Umlaute, Emoji, CJK; gleich `detail.serpMeasure`); Länge in Codepoints; Stufe 1 gegen andere indexierbare URLs (case-insensitiv, Whitespace), nicht gegen nicht indexierbare/eigene; Stufe 2 mit `duplicate_candidate:http://ai.test:8080/a/`, Nachrücken, `No valid suggestion` ohne Request; Determinismus 20 × 4 Worker mit gesäter Verzögerung und `/b/` vor `/a/`; vorläufig bis zum Nachlauf; Spalten | grün |
| AI-072 | Grenzen 70/155 Zeichen, 400/985 px je ±1; Zeilenumbruch/Anführungszeichen vor der Prüfung entfernt; 14-px-Profil (100 × a = 800 …); Präfix `Meta Description Suggestion …`, Basisspalten | grün |
| AI-075 | Grenzen Title 30/60, 200/561 je ±1 (`pixel_width>561`); Regeln einzeln abschaltbar außer Pixel; Platzhalter/Umbruch/Anführungszeichen; Sprache (≥ 0,95, < 20 Zeichen nicht geprüft, Primär-Subtag); 612/580/590 → 540 px mit 2 Requests und den drei Verstößen im zweiten; nach 2 Reparaturen gültig; nie gültig → 3 Requests, sichtbar der mit den wenigsten Verstößen (Gleichstand: erster); Rundenzahl 0/3, Bereiche; Neuprüfung ohne Request (Kern und `ai.suggestRecheck`) | grün, **außer** automatischer Neuprüfung bei Grenzwertänderung (B1-3) |
| AI-076 | Übernehmen/Ablehnen/Bearbeiten/Zurücksetzen (Kern, Commands, UI); bearbeitet mit Verstößen als `Edited` mit Verstoßliste, keine stille Korrektur; Live-Prüfung mit Pixelbreite; Massenaktion 10 URLs (Kern, Command) und über Auswahl (UI); Export „Accepted“ genau Accepted/Edited, Spalten und Reihenfolge, `Origin`; Export „All“ mit allen Kandidaten und Verstößen; CSV-Formelschutz `= + - @` | grün, **außer** XLSX (B1-2) |
| UI-004 [M4] | Schalter aus: kein Tab, keine Filter/Zähler/Übersicht/Exporte in der App, keine Einträge in CLI-Listen, MCP-Katalog, Mail-Zusammenfassung und Vergleich; Schalter an: Tab mit allen Filtern; Zähler und Sortierung; UI-Tab-Leiste (Playwright) | grün (MCP erst ab `919bf2cb`) |
| UI-037 [M4] | SERP-Vorschau folgt dem gewählten Kandidaten; 570-px-Kandidat wird an derselben Stelle gekürzt wie der gleichlautende echte Title im Detail-Tab SERP-Snippet (chromium, webkit) | grün |
| OUT-001/002/003/016 | Export über `export_bulk` (CSV/XLSX), Kopfzeile, Zahlenzellen, keine Formeln, CSV-Maskierung | grün außer B1-2 |
| AI-013 | Budget 0 → 0 Requests; knappes Budget → genau die passenden Requests, Rest `Budget exceeded`; Ledger = Summe Protokoll, keine offene Reservierung | grün |
| AI-014 | eine Protokollzeile je Request/Runde mit Kosten; Cache-Treffer `Cost = 0`; Ausgabegrenze-Zeile | grün außer B1-1 (bekannt) |
| AI-015 | identischer Reparatur-Request → Cache-Treffer ohne Request; zweiter Lauf 0 Requests; Refusal gecacht; Ausgabegrenze nicht gecacht | grün |
| AI-017 | je 12 Refusals bzw. Ausgabegrenzen nacheinander werden gesendet (neutral für den Schutzschalter) | grün |
| AI-018 | ohne Einwilligung 0 Requests, `Pending (consent)`, keine Reservierung | grün |
| AI-020 | Stoppen: laufender Request übernommen, danach keiner, Rest `Pending` ohne Protokollzeile; Atomarität per Fehlerinjektion (Kern und Store); `Error: Output limit reached`; Filter „Errors“ | grün; `hold`-Test ignoriert (wartet auf M4-D01) |
| AI-021 | Schemaverstoß → genau ein Reparaturversuch, dauerhaft `Error: Invalid output` | grün |
| AI-038 | Refusal ohne Reparatur | grün |

## Befunde

| Nr. | Schwere | Spec | erwartet | beobachtet | Test |
|---|---|---|---|---|---|
| B1-1 | B1 (bekannt) | AI-014 Nachtrag 24.11.2026: „Szenario `output-limit` … `Output Tokens` = gesendete Grenze, `Cost` > 0“ | Zeile mit Ausgabe-Tokens und Kosten | `output_tokens` leer/0, Kosten 0 (über `guard::execute` keine `usage` verbucht) – bekannte Lücke, bestätigt | `os-ai acceptance_v01_mock::output_limit_row_books_the_reported_tokens` |
| B1-2 | B1 | OUT-003: „Kein Zellwert wird je als Formel geschrieben; ein Title `=SUM(A1)` erscheint in Excel als Text `=SUM(A1)`“; OUT-016 ist ein Schutz „für CSV“, AI-076 verweist nur für CSV darauf | XLSX zeigt `=SUM(A1) …` als Text | XLSX-Zelle enthält `'=SUM(A1) …` (Maskierung auch im XLSX; das Apostroph ist dort echter Zellinhalt) | `os-export acceptance_v01_export::xlsx_shows_formula_like_values_as_plain_text` |
| B1-3 | B1 | AI-075: „Grenzwertänderung in den Preferences (Title max. 50 Zeichen) → bestehende Kandidaten werden ohne Request neu geprüft; Status ändert sich entsprechend.“ | nach `config.set` (Title max. 40) ist `/valid-open/` (43 Zeichen) `No valid suggestion` | bleibt `Valid`, bis jemand `ai.suggestRecheck` aufruft; die Oberfläche ruft ihn nur bei `settingsChanged` (App-Einstellungen), nicht bei Änderung der Crawl-Konfiguration | `os-app acceptance_v01_app::changing_title_limit_rechecks_existing_candidates` (Gegenprobe `explicit_recheck_applies_current_limits` grün) |
| B2-1 | B2 | AI-075 „dauerhaft ungültig → nach 1 + 2 Requests“ vs. AI-015 „Gleiche Eingabe … erzeugt keinen neuen Request“ | – | Liefert der Anbieter jede Runde dieselbe Antwort, ist der dritte Request identisch mit dem zweiten → Cache-Treffer: 2 HTTP-Requests, 3 Protokollzeilen. Die Tests prüfen beides (verschiedene Antworten: 3 Requests; gleiche: 2 + Cache). Spec sollte den Fall nennen | – |
| B2-2 | B2 | AI-070 Filter `Open` | – | Einträge ohne Kandidaten (Pending, Fehler, Budget) zeigen `Review Status = Open`, stehen aber nicht im Filter `Open`. Nicht festgelegt, ob `Open` = Review-Status oder „prüfbar und offen“; getestet nur die eindeutigen Fälle | – |
| B2-3 | B2 | AI-075 Beispiel `duplicate_of:/b/`, AI-071 `duplicate_candidate:http://ai.test:8080/a/` | – | Stufe 1 nennt die volle Adresse; Beispiel zeigt Pfad. Format von `duplicate_of` festlegen | – |
| B2-4 | B2 | AI-076/OUT-016 | – | OUT-016 ist eine P2-Option (Standard aus); die Vorschlagsexporte maskieren immer. Ob AI-076 den Schutz unbedingt verlangt, klarstellen | – |
| B2-5 | B2 | AI-071 „vorläufig bis Nachlauf“ + AI-020 Stoppen | – | Nach Stoppen läuft kein Nachlauf; fertige Einträge bleiben vorläufig. Nicht geregelt | – |
| B3-1 | B3 | AI-076 Testfixture `tests/golden/ai/suggestions-accepted.csv` | – | Pfad existiert nicht (Bauer: `tests/fixtures/ai/suggest/golden/`); die Abnahme prüft die Zeilen direkt | – |
| B3-2 | B3 | AI-070 Kandidatenzahl n | – | Antwortet das Modell mit mehr als n Kandidaten, werden nur n gespeichert; nicht geregelt | – |

## Entscheidungen 27.11.2026 (Orchestrator)

- **B1-1** ist eine Lücke im Kern (nicht V01), Behebung als Auflage in M4-N01. Test
  `acceptance_v01_mock::output_limit_row_books_the_reported_tokens` trägt `#[ignore = "wartet auf M4-N01: …"]`,
  Testkörper unverändert. Damit os-ai mock: 6 grün, 0 rot, 2 ignoriert (ohne neuen Lauf).
- **B1-2** (XLSX mit `'`) und **B1-3** (keine Neuprüfung nach `config.set`) behebt der Bauer; die Tests bleiben unverändert,
  Nachprüfung nach dem nächsten Bauer-Stand.
- **B2-4** entfällt: Der CSV-Formelschutz ist für beide Vorschlags-Exporte immer an (KI-Text nicht vertrauenswürdig); kein Befund.
- **B2-1** geklärt: gezählt werden HTTP-Requests; die dritte Runde aus dem Cache ergibt eine Protokollzeile mit `Cache Hit`
  und Kosten 0. Die Tests (`acceptance_v01_core::identical_repair_request_is_a_cache_hit`,
  `acceptance_v01_mock::permanently_invalid_answer_runs_three_rounds_with_one_cache_hit`) prüfen genau das.
- **B2-3** geklärt: `duplicate_of` nennt die volle Adresse. Die Tests prüfen nur Präfix und Pfad (`/a/`), sind also mit der
  Festlegung verträglich.
- **B2-2** (Filter `Open`) und **B2-5** („vorläufig“ nach dem Stoppen) klärt der Orchestrator mit dem Planer. Kein
  Abnahmetest erzwingt eine Deutung: `acceptance_v01_tab::open_filter_contains_unreviewed_suggestions_only` prüft nur die
  eindeutigen Fälle (offene Einträge mit Kandidaten gehören dazu; Accepted/Edited/Rejected nicht); die Fixture-Liste
  `expected_filters()` enthält `Open` bewusst nicht. Zum Stoppen prüft
  `acceptance_v01_core::stop_sends_nothing_more_and_keeps_open_entries_pending` das Kennzeichen „vorläufig“ nicht.

## Durchsicht der Bauer-Änderungen an fremden Tests und Golden-Dateien

| Datei | Änderung | Urteil |
|---|---|---|
| `crates/os-app/tests/acceptance_f02_ai_ipc.rs` | 7 umgesetzte `ai.suggest*`-Commands aus „Stub antwortet `err-not-implemented`“ ausgenommen | gedeckt: sie sind keine Stubs mehr. Die frühere Zusicherung (kein `NotFound`, Antwort mit Fehler bei Schalter aus) prüft jetzt `acceptance_v01_app::suggestion_commands_answer_with_the_preview_error_when_off` (grün); im F02-Test selbst fehlt ein Ersatz – keine Lockerung für andere Commands |
| `crates/os-analyze/src/tabs/ai.rs` (Stub-Prüfung) | für „AI Suggestions“ keine Prüfung `population == "0"` und Stub-Filter | gedeckt: der Tab ist umgesetzt; die Filterliste wird weiter gegen `docs/plan/M4-filters.toml` geprüft, andere Tabs unverändert |
| `crates/os-analyze/tests/filters_registry.rs` | Stub-Tabs 4 → 3, Stub-Filter 27 → 18 | gedeckt: genau ein Tab mit 9 Filtern (ohne `All`) wurde umgesetzt, Zahlen stimmen |
| `crates/os-app/tests/manifest_contract.rs` | 6 Command-Namen ergänzt | gedeckt: additiv im Bereich `ai.suggest.*` aus M4.md 3.3; der Test prüft weiter Gleichheit der Mengen |
| `tests/fixtures/out-m2-mail/compare.txt` | in `f3890fe1` 10 Zeilen „AI Suggestions / … \| 0 \| 0“ ergänzt | in `f3890fe1` **nicht gedeckt** (schrieb die Sichtbarkeit bei Schalter aus fest, Punkt 6); mit `6fa6f887`/`919bf2cb` zurückgenommen (Golden wie `main`), Mail-Test grün |

## Nicht geprüft

- Atomarität/Absturz mit dem Mock-Szenario `hold`: fehlt auf diesem Zweig → `#[ignore = "wartet auf M4-D01: …"]`;
  Atomarität ersatzweise per Fehlerinjektion (Kern: Speicher, Store: Transaktion) geprüft.
- Massenaktion „über Filter“: die Oberfläche bietet die Massenaktion für die Auswahl; geprüft über Auswahl (UI) und
  Commands; eine Auswahl aller Zeilen eines Filters über die Tabelle nicht.
- `Duplicate` im Standard-Geltungsbereich: setzt die Crawl-Analyse voraus, in der Fixture nicht belegt.
- Echter Vorschlagslauf aus der App (bis M4-D05/N01 laut Festlegung), Schutzschalter nach 10 echten Fehlern (5-Minuten-Pause),
  Teilbudget je Aufgabe, Batch.
- Dialoge des Prüfbereichs: es gibt keine; axe auf dem Prüfbereich (auch im Bearbeiten-Zustand), de und en.
- Vergleichsbericht als Export (`Crawl Overview` im Vergleichsmodus) – nur Mail-Vergleich und App-Listen geprüft.
- AI-077, AI-073, AI-074.
