# M4-D01 – Unabhängige Abnahmetests (Bericht)

- **Paket:** M4-D01 „Decision-Engine Kern“ (AI-030 … AI-035, dazu AI-020 Nachtrag 26.11.2026 / K4-13, Mock `hold`)
- **Zweig:** `ai/d01-tests`, Tests auf dem Bauer-Stand `cadb8e1a` (Basis `14d42e7a`); Testcommits `54afdd51`,
  `2f6b6f62`, `43d7d8fd` (+ Bericht)
- **Vorgehen:** nur aus `docs/spec/08-ai.md`; Bauer-Bericht, Bauer-Tests und Bauer-Commit-Nachrichten nicht gelesen.
  Angebunden über die öffentliche Schnittstelle (`DecisionRun`, `format`, `library`, `input`, `fallback`, `routing`,
  `os_store::ai::{questions,answers}`), gegen den ai-mock mit eigenem Antwortfragment
  (`tests/fixtures/ai/d01-acceptance/mock-answers.d/d01t.json`) und erfundenen Schlüsseln.
- **Spec-Stand:** `main` = `cb882d40`. Der Auftrag nannte `ce882f71`; `main` hat seitdem drei Spec-Commits erhalten
  (`4d403bf5`, `e68f1fe8`, `cb882d40`: bestätigte Auslegungen aus D01, `normalized`, `Skipped (price unknown)`,
  K4-24, Pfade an den Bau angepasst). Die Tests folgen diesem Stand. Festlegungen, die erst nach dem Bau kamen und
  laut `docs/plan/M4-reports/n01-auflagen.md` einem anderen Paket zugewiesen sind, stehen als `wartet auf M4-N01`.
- **Läufe** (Mini, unter `heavy.lock`, `CARGO_TARGET_DIR=wt/ai-d01/target`, Logs `d01t-log/run{1,2,3}.log`):
  Runde 1 Kompilat, fmt und ein Clippy-Fund in meinem Test; Runde 2 fmt/Clippy grün; Runde 3 Endstand (siehe unten).

## Testzahlen (Endstand, Runde 3, Prüfzeile `TESTS=43d7d8fd BAUER=cadb8e1a`; fmt und Clippy grün)

| Crate / Testziel | grün | rot | ignoriert |
|---|---|---|---|
| os-ai `acceptance_d01_format` | 22 | 0 | 0 |
| os-ai `acceptance_d01_library` | 8 | 2 | 0 |
| os-ai `acceptance_d01_input` | 14 | 0 | 1 |
| os-ai `acceptance_d01_exec` | 25 | 1 | 2 |
| os-ai `acceptance_d01_fallback` | 14 | 1 | 0 |
| os-ai `acceptance_d01_routing` | 6 | 0 | 0 |
| os-ai `acceptance_d01_lifecycle` | 7 | 0 | 0 |
| **os-ai gesamt** | **96** | **4** | **3** |
| os-store `acceptance_d01_questions` | 2 | 0 | 0 |
| os-testsite `acceptance_d01_hold` | 4 | 0 | 0 |
| **gesamt** | **102** | **4** | **3** |

Die 3 ignorierten Tests laufen mit `--ignored` rot (erwartet, siehe B1-4 … B1-6). Rot: B1-1 (2 Tests), B1-2, B1-3.

## Prüfpunkte je Spec-ID

| Spec-ID | Geprüft (Testdatei) | Ergebnis |
|---|---|---|
| AI-030 | Pflichtfelder mit Feldangabe; `id`-Muster und 48 Zeichen; `name` 1–60 Zeichen (Zeichen, nicht Bytes) und eindeutig unter aktiven Fragen; `choices` 2/20 ok, 0/1/21 abgelehnt, eindeutig, Muster `^[a-z0-9_-]{1,32}$`; `levels` 2/10 ok, 1/11 abgelehnt, eindeutige Labels; `instructions` 1/2.000 ok, 0/2.001 abgelehnt (auch mit Umlauten); `predicate` ohne `choices`/`levels`; geschlossene Mengen `type`/`subject`/SemVer; Bildfrage nur Bild + `alt`/`image_context`/`url`, Seitenfrage mit Seitenbild ja, `alt`/`image_context` nein; `max_chars` nur für `page_text`; optionale `issues`/`segment`; `wire_name`; Rundreise laden → kanonisch/Export → laden identisch; Schema-Datei gegen Spec-Grenzen; Vorlagen: Platzhalter in `value`/`label`/`name` bzw. ohne Parameter abgelehnt, Kandidatenwerte `c1…ck`+`none`, k ≤ 10; Property-Test k = 1…10 mit Umlauten, Query-Strings und `{{…}}` in Adressen → jede Instanz schema- und regelgültig; String-Vorlage; `page_type`-Reihenfolge (`acceptance_d01_format.rs`, `acceptance_d01_store`) | grün |
| AI-031 | Minor bei `instructions`/`inputs`/hinzugefügtem Wert/`params`/Level-Beschreibung; Major bei `type` und entferntem `choice`-Wert; keine Erhöhung bei `name`, `thresholds`, `scope`; **Major bei entferntem Level und bei `subject`-Wechsel (Nachtrag 26.11.)**; Speichern einer Benutzerfrage erhöht die Version; Systemfrage schreibgeschützt, „Bearbeiten“ → `-custom` / „<Name> (custom)“; Export → Import identisch, Konflikt, „als Kopie“ → `-copy`, `-copy-2`, „Ersetzen“; Hinweistext „Frage aktualisiert (1.0.0 → 1.1.0) – neu analysieren?“; Vorlagen nur als Systemfrage bzw. Kopie; `ai_questions` genau eine unveränderliche Zeile je Version (auch SQL-`UPDATE` scheitert); 30 Quell-URLs mit Kandidatenvorlage → 1 Zeile in `ai_questions`, 30 in `ai_answers` mit eigenen Parametern; alte Antworten bleiben der alten Version zugeordnet (`acceptance_d01_library.rs`, `os-store/tests/acceptance_d01_questions.rs`) | rot: Level/`subject` (B1-1) |
| AI-032 | Kürzung an Codepoint-Grenzen (Emoji, ZWJ-Folge, kombinierende Zeichen, CJK, Umlaut an 4.999/5.000); Satzende-Regel 7.912/4.900, Fenstergrenzen 7.599 (nicht) / 7.600 (ja), ohne Satzende harte Grenze; INTEG-172 unverändert 5.000; Höchstlänge je Frage bzw. Modell bzw. 5.000; nur gewählte Felder (Feldmarker im Mock-Log); `h2` mit Zeilenumbruch; leere Felder → `Skipped (empty input)` ohne Request; `image_context` ≤ 300; Bildgrößen 20 MB / Format / 2.048 px-Ziel; Bündelung 3 + 1 Fragen → 2 Requests je URL; Geltungsbereich S: 22 `Success`, 3 `Skipped (scope)`, 0 Requests außerhalb; nur Segment verfehlt → `Skipped (segment)`; Bildfrage ohne Indexierbarkeitsprüfung; `page_type`-Ergebnis in der Eingabe der unteren Frage (`acceptance_d01_input.rs`) | grün; gesendetes Bild 2.048 × 1.365 ignoriert (N01, A1) |
| AI-033 | Bündelung 5/10/11/12/21 Fragen → 1/1/2/2/3 Requests je URL, jede Frage genau einmal; Property-Test 1–40 Fragen × max 1/3/7/10 → kein Request über max, keiner über 10; Einstellung 0/11 abgelehnt (auch per JSON); Werte exakt aus dem Mock (Produktseite `product`, Confidence 0,93, 14 Wahrscheinlichkeiten); `score`/`predicate` unverändert; Summe 1,03 normiert (Confidence neu, `normalized` in `answer_json`), 1,005 nicht; `confidence_mismatch` bei 0,13 Abweichung ja, bei 0,005 nein, rohe Konfidenz gespeichert; Voranschlag je Frage = Formel AI-012 aus den gesendeten Requests, gebündelt = einzeln (Tokens), Kosten ±1 µUSD je Request, Ist-Kosten innerhalb ±25 %; Bildfragen 1 Bild je Request + Kontextfeld (`acceptance_d01_exec.rs`) | grün bis auf ignorierte Protokollspalte `normalized` (N01, B7) |
| AI-034 | Schemaformen je Typ, strikter Modus rekursiv (auch Bündelschema); Ableitung `choice`/`score`/`confidence`; Gleichstand → zuerst definierter Wert; Normierung; Summe 0 / negativ → `Error: Invalid output`; `abstain` → `Refusal` mit „model abstained“, kein Wiederholen, nicht in „Errors“; Schemaverstöße (ohne `abstain`, fehlender bzw. zusätzlicher Wert) → genau 1 Reparaturversuch; Anthropic-Request `output_config.format` mit `abstain`, `thinking.type = disabled`, `max_tokens` 256 / 576 (W = 14) / 640 (W = 16); OpenRouter M-R1 `reasoning.effort = none`; gebündelt 3 Fragen → 1 Request; `output-limit` gebündelt → alle 3 `Error: Output limit reached`, 1 Request (ungebündelt 3); malformed einmal → 2 Requests mit gleicher Stufe und gleichem `max_tokens`; beide Wege liefern gleiche Felder und Werte (choice, score) (`acceptance_d01_fallback.rs`) | rot: Validator-Fehler fehlt im Protokoll (B1-3) |
| AI-035 | automatisch mit OpenAI-Schlüssel → Decisions, ohne → erstes Ersatzmodell; „immer Decisions API“ ohne Schlüssel → `Error: Missing API key`, 0 Requests bei verfügbarem Ersatzweg; Richtlinie verbietet Decisions → Ersatzweg; fest „Modell X“; „Nur lokal“ → Ollama bzw. `Skipped (policy)` ohne Request; beide Schlüssel → 0 Requests an `/anthropic/v1/messages`; Schlüssel entfernt → nur neue Einträge über Anthropic, alte unverändert; Wegwechsel ändert den Cache-Schlüssel; Protokollzeile zeigt Anbieter/Modell/Endpunkt des Wegs (`acceptance_d01_routing.rs`) | grün |
| AI-020 K4-13 | Pause während Request 6 → Request 6 übernommen, Mock-Log 6, 6/16/3, Verbrauch = Summe der Zeilen, keine Reservierung; Fortsetzen nach Neuöffnen der Datei → 16 weitere, 22 Zeilen, jede URL einmal, kein Voranschlag; Stopp → kein weiterer Request, 16 `Pending`, „KI-Analyse starten“ mit Schwelle 100 µUSD → Voranschlag über 16, `Pending (confirmation)`, danach genau 16; geordnetes Beenden während gehaltenem Request 3 → Lauf wartet nicht, Mock `aborted`, Eintrag `Pending`, keine Zeile, Reservierung verfallen, Neusenden genau einmal; Absturz (Lauf verworfen) → dasselbe, beim Öffnen 0 Requests; Atomarität (Fehlerinjektion vor dem Commit) → weder Ergebnis, Cache, Zeile noch Kosten, ohne Injektion alle vier; fertige Einträge (`Error: 500`) beim Fortsetzen unberührt, „Fehlgeschlagene“ sendet genau diese; Pause auch im Ersatzweg; `Pending (consent)` übersteht einen Lauf ohne Einwilligung und wird danach genau einmal gesendet (`acceptance_d01_lifecycle.rs`, `acceptance_d01_exec.rs`) | grün |
| AI-020/021/038 Status | fehlender Schlüssel 22 × `Error: Missing API key`, 0 Requests; Kontingent 1 Request, 22 × `Quota exceeded`; Kontingentsperre je Lauf ohne weitere Zeilen, neuer Lauf versucht es wieder; Spend-Cap-429 → `Quota exceeded`; 5xx → 1 + 5 Versuche, eine Zeile mit `Retries = 5`, `HTTP Status 500`, Kosten 0; vorübergehender 5xx → `Success`, `Retries = 2`; unlesbare Antwort → `Error: Invalid output`; `missing-answer` → genau eine Frage `Error: Missing answer`; Refusal nur für seine Frage, Wertefelder leer; alle Statuswerte aus der Tabelle | grün; `Skipped (price unknown)` ignoriert (N01, B8) |
| AI-012/013/014/017/018 übergreifend | eine Protokollzeile je Request (`HTTP Status`, Tokens laut Mock-`usage`, Kosten 0,10 USD/1 M, Cost = Summe der Teile), gebündelter Request = 1 Zeile mit den Versionen in Fragenreihenfolge; Budget 600 µUSD mit 4 Workern und 200 Einträgen → Ist-Summe ≤ Budget, keine Überschreitung der Reservierung, Rest `Budget exceeded`; Refusal und `output-limit` lösen keinen Schutzschalter aus (12 bzw. 22 Requests); ohne Einwilligung bzw. mit Vorschau-Schalter aus 0 Requests; Bestätigungsschwelle einschließlich (Nachtrag 26.11.) | rot: Schwelle (B1-2) |
| Mock `hold` | `count = 2` → beide `held: true` (auch über `GET /__mock/log`), unbeantwortet, dritter sofort; `release` 1 → genau der ältere mit dem Inhalt ohne `hold`, zweites → der andere, drittes `{"released": 0}`; abgebrochener Request `aborted: true`, `release` zählt ihn nicht; ohne gehaltenen Request `{"released": 0}`; unbekannter Anbieter 400 (`os-testsite/tests/acceptance_d01_hold.rs`) | grün |

## Befunde

| Nr. | Schwere | Spec | Erwartet | Beobachtet |
|---|---|---|---|---|
| B1-1 | B1 | AI-031, Nachtrag 26.11.2026: „Das Entfernen eines Levels und eine Änderung von `subject` erhöhen die Major-Version“ | `next_version` 1.4.2 → 2.0.0 (Level entfernt) bzw. 1.2.0 → 2.0.0 (`subject` page → image) | 1.5.0 bzw. 1.3.0 (Minor). Die Festlegung kam nach dem Bau; in der N01-Auflagenliste nicht enthalten. Tests `ai031_levels_and_params_changes`, `ai031_version_rules` |
| B1-2 | B1 | AI-012, Nachtrag 26.11.2026 (B2-2 der Abnahme U01): „Erwartete Kosten **gleich** der Schwelle sind bestätigungspflichtig (≥)“ | `Confirmation::AskAbove(E)` mit E = erwartete Kosten aus `DecisionRun::estimate` derselben Einträge → `NeedsConfirmation`, 0 Requests | `Completed`, Requests gesendet (Prüfung „>“). Festlegung nach dem Bau; Regel vermutlich in `cost::confirmation_required` (Zuständigkeit klären). Test `ai012_confirmation_threshold_is_inclusive` |
| B1-3 | B1 | AI-021: „Szenario `schema-violation` dauerhaft → nach 2 Requests `Error: Invalid output`; der Validator-Fehler steht im Protokoll“ | eine Protokollzeile des Eintrags mit Validator-Meldung | 2 Requests und `Error: Invalid output` korrekt, aber `error_message` aller Zeilen leer. Test `ai021_schema_violation_exactly_one_repair` |
| B1-4 | B1 (bekannt) | AI-032: Bild 3.000 × 2.000 → gesendet 2.048 × 1.365 | gesendete Bytes 2.048 × 1.365 | Originalbytes 3.000 × 2.000 (kein Resizer). Laut Plan N01 Auflage A1 → `#[ignore = "wartet auf M4-N01: …"]` |
| B1-5 | B1 (bekannt) | AI-033/AI-014, Nachtrag 26.11.2026: Spalte `normalized` in `ai_requests` | Spalte vorhanden, 1 bei Summe 1,03 | Spalte fehlt (`no such column`). N01 Auflage B7 → ignoriert |
| B1-6 | B1 (bekannt) | AI-020, Nachtrag 26.11.2026: `Skipped (price unknown)` | 22 × `Skipped (price unknown)`, 0 Requests | 0 Requests, Status nicht `Skipped (price unknown)` (laut Spec bekannte Abweichung: `Skipped (no capable model)`). N01 Auflage B8 → ignoriert |
| B2-1 | B2 | AI-030 Beschreibung nennt `scope` und `thresholds` ohne „optional“ (nur `issues` und `segment` sind „optional“) | – | Schema und Prüfung behandeln `scope`/`thresholds` als optional (fehlt `scope`, gilt S). Klarstellung, ob beide Pflichtfelder sind. Kein Test |
| B2-2 | B2 | AI-020 Nachtrag 26.11.2026 „Kontingentsperre je KI-Lauf“ | – | D01 bindet die Sperre an die Adapter-Instanz (Doku `Adapters`: „Instanzen gelten für einen Lauf“). Ein Aufrufer, der Adapter über Läufe hinweg wiederverwendet, behält die Sperre: im ersten Durchlauf meines Tests blieb der zweite Lauf ohne Request. Spec bzw. D05 sollte festhalten, dass je Lauf neue Adapter entstehen (oder die Sperre im Lauf liegt) |
| B3-1 | B3 | AI-020 „Kostenbestätigung beim Fortsetzen … auch nach App-Neustart“ | – | D01 stellt dafür nur `Confirmation::Granted` bereit; wer sich die erteilte Bestätigung über einen Neustart merkt, liegt beim Aufrufer (D05/U01). Geprüft ist nur: mit `Granted` kein Voranschlag, kein `Pending (confirmation)` |
| B3-2 | B3 | 08 Konventionen `hold` | – | `hold` hält **jeden** Request des Anbieters, auch `GET …/models`. Spec-konform („die nächsten n Requests dieses Anbieters“), Tests müssen Modelllisten vor dem Scharfschalten laden |
| B3-3 | B3 | AI-013 | – | Fehlt der Bild-Schätzwert in den Preisen, liegt die Ist-Antwort einer Bildfrage über der Reservierung; danach sperrt der Ledger (spec-konform, `Budget exceeded` für den Rest). Testpreise daher mit b = 85 |
| B3-4 | B3 | – | – | Bauer-Platzhalter mit `wartet auf M4-N01` für CLI, automatische Ausführung und SIGKILL-Prozess gehören nach `n01-auflagen.md` Z. 11 inzwischen zu M4-D05 (D05 löst die D01-Platzhalter auf) |

## Durchsicht (Zusatzauftrag)

### (a) Dateien des Bauers gegen `owns` (`git diff --stat 14d42e7a...cadb8e1a`, 66 Dateien)

| Datei(en) | `owns`-Eintrag | Bewertung |
|---|---|---|
| `crates/os-ai/src/decisions/{exec,fallback,format,input,library,routing}/mod.rs` | `crates/os-ai/src/decisions/{format,library,input,exec,fallback,routing}/**` | ok |
| `crates/os-ai/src/decisions/mod.rs` (nur Modul-Doku) | `crates/os-ai/src/decisions/mod.rs` | ok |
| `crates/os-ai/tests/decisions_{exec,fallback,format,input,library,lifecycle,pending,routing,support}.rs` | `crates/os-ai/tests/decisions_*.rs` | ok |
| `crates/os-store/src/ai/{answers,questions}.rs` | `crates/os-store/src/ai/{questions,answers}.rs` | ok |
| `crates/os-testsite/src/mocks/ai/hold.rs` (neu), `mod.rs` | `…/mocks/ai/{mod.rs,hold.rs}` (nur `hold`) | ok – Diff in `mod.rs` betrifft nur `hold`/`release`, Felder `held`/`aborted`, `PROVIDERS`-Konstante |
| `crates/os-testsite/tests/ai_mock_hold.rs` | gleichnamig | ok |
| `resources/ai/questions/schema/question.schema.json` | `resources/ai/questions/schema/**` | ok (Spec-Pfad inzwischen angepasst) |
| `tests/fixtures/ai/decisions/**` (lib-v1/2, long.html, questions/valid, questions/invalid) | `tests/fixtures/ai/decisions/**` | ok |
| `tests/fixtures/ai/mock-answers.d/d01.json` | gleichnamig | ok |
| `docs/plan/M4-reports/d01.md` | – | **nicht in `owns`**, aber von `prompt_notes` verlangt („Bericht docs/plan/M4-reports/<kurz-id>.md“) – erwartete Abweichung |

Ergebnis: Die Meldung „keine Dateien außerhalb von `owns`“ stimmt bis auf den vorgeschriebenen Bericht.

### (b) `#[ignore]` des Bauers in `crates/os-ai/tests/decisions_*.rs`

| Stelle | Grund | Form | Paket plausibel? |
|---|---|---|---|
| `decisions_pending.rs:10` | wartet auf M4-D02: Schaltfläche „KI-Analyse starten“ (AI-039) und Anbindung an die Crawl-Verwaltung | ok | ja (D02 besitzt AI-039); Anbindung an die Verwaltung eher D05 |
| `decisions_pending.rs:19` | wartet auf M4-N01: CLI-Anbindung der Decision-Engine | ok | inzwischen M4-D05 (Verdrahtung mit CLI); N01 laut Auflagenliste Z. 11 nicht mehr zuständig |
| `decisions_pending.rs:28` | wartet auf M4-N01: automatische Ausführung während des Crawls (E-6 a) | ok | inzwischen M4-D05 |
| `decisions_pending.rs:37` | wartet auf M4-N01: Prozess mit KI-Lauf (CLI) für den SIGKILL-Test | ok | inzwischen M4-D05 |
| `decisions_pending.rs:47` | wartet auf M4-K11: Leseseite „aktiver Besitzer“ (App nur lesend); Zähler der App: M4-N01 | ok (zwei Pakete in einem Grund) | K11 ja; „Zähler der App“ eher D05 |
| `decisions_pending.rs:57` | wartet auf M4-U01: Dialog Kostenvoranschlag beim Fortsetzen | ok | ja (U01 besitzt den Dialog) |
| `decisions_pending.rs:66` | wartet auf M4-N01: Manifest os-ai (Bildbibliothek) und Fixture big.jpg | ok | ja (N01 Auflage A1) |
| `decisions_lifecycle.rs:580` | wartet auf M4-K11: `os_ai::run_owner::RunOwner` (Heartbeat alle 2 s, Ende mit leerem owner_pid) | ok | ja (K11 owns `crates/os-ai/src/run_owner.rs`); Anbindung an `DecisionRun` laut Plan D05 |
| `decisions_lifecycle.rs:589` | wartet auf M4-K11: passiver RunOwner, wenn der eigene Prozess den Crawl führt | ok | ja (K11/D05) |
| `decisions_lifecycle.rs:598` | wartet auf M4-K11: Schreibseite der Besitzerangaben (RunOwner) | ok | ja |
| `decisions_lifecycle.rs:609` | wartet auf M4-K11: Leseseite „aktiver Besitzer“ in der Crawl-Verwaltung | ok | ja |

Alle elf Gründe haben die Form `wartet auf M4-<ID>: …`.

## Eigene `#[ignore]`

| Test | Grund |
|---|---|
| `acceptance_d01_input::ai032_big_image_is_sent_as_2048x1365` | wartet auf M4-N01: Bildbibliothek (Auflage A1) – im Lauf mit `--ignored` rot (3.000 × 2.000 gesendet) |
| `acceptance_d01_exec::ai020_model_without_prices_is_skipped_price_unknown` | wartet auf M4-N01: Auflage B8 – mit `--ignored` rot |
| `acceptance_d01_exec::ai033_normalization_is_flagged_in_the_request_log` | wartet auf M4-N01: Auflage B7 – mit `--ignored` rot (`no such column: normalized`) |

## Nicht geprüft (mit Grund)

- Heartbeat/Besitz eines KI-Laufs, Löschschutz, fremder Writer, unterbrochener Crawl (K11/D05; D01 kapselt sie hinter `OwnerStart`, im Test `owner: None`).
- Echter Prozessabbruch per SIGKILL und SIGINT-Exit-Code 130 (CLI, D05); der Absturz ist durch Verwerfen des Lauf-Futures während eines gehaltenen Requests nachgebildet, das Öffnen durch Schließen und Neuöffnen der Crawl-Datei.
- Fixture-Site per Crawl: D01 crawlt nicht; die 25 URLs (22 in S, `noindex`, 404, 301) sind als `RunItem`s nachgebildet. Crawl-Ende ohne Stoppen und KI-Lauf in der Pause mit gecrawlten URLs brauchen die Engine (D05).
- Spalten, Filter, Tab, Export (D02, N01 B10), Inhalte der Systemfragenbibliothek (D03), Batch (AI-016).
- Kalibrierter Voranschlag (±5 %) und Kostenbestätigung über einen App-Neustart (siehe B3-1).
- Schutzschalter nach 10 endgültigen Fehlern (AI-017, K10 abgenommen); hier nur die Neutralität von Refusal und Ausgabegrenze.

## Offene Fragen an die Spec

1. Sind `scope` und `thresholds` Pflichtfelder einer Frage (B2-1)?
2. Wo liegt die Kontingentsperre „je KI-Lauf“: im Lauf oder in der Adapter-Instanz, die der Aufrufer je Lauf neu anlegt (B2-2)?
3. Welches Paket setzt B1-1 (Major bei Level/`subject`) und B1-2 (Schwelle ≥) um? Beide fehlen in der N01-Auflagenliste.
