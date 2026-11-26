# M4-U01 – Unabhängige Abnahme (Testagent)

Stand: 26.11.2026 · Zweig `ai/u01-tests` · Bauer-Stand `21286ea5` · Testagent Claude Opus 5.5 (medium)

Grundlage allein `docs/spec/05-ui.md` (UI-013 [M4]), `docs/spec/08-ai.md` (AI-011, AI-012, AI-018, AI-019 mit den
Nachträgen 24./25./26.11.2026), `docs/spec/04-custom-integrations.md` (INTEG-005 [M4]) und `docs/plan/M4.md` (Abschnitt 2,
E-5 a, Abschnitt 7). Bauer-Bericht, Bauer-Tests und Bauer-Commit-Nachrichten nicht gelesen; einzige Ausnahme ist das Diff
der zwei geänderten Fremdtests (Abschnitt „Durchsicht“). Gelesen wurde die öffentliche Schnittstelle: Commands und DTOs in
`crates/os-app/src/commands/{ai_settings,ai_cost,ai_consent}.rs`, Rollen/Beschriftungen/Attribute der Komponenten unter
`apps/desktop/src/features/ai/{settings,cost,consent}/`, die E2E-Hilfen (`e2e/fixtures/app.ts`, `e2e/support/bridge.ts`).
Alle Tests laufen gegen den ai-mock (`os-testsite`), Fixtures oder die E2E-Bridge, nie gegen echte Anbieter, nur mit
Testschlüsseln im Arbeitsspeicher.

## Dateien

| Datei | Inhalt |
|---|---|
| `crates/os-app/tests/acceptance_u01_support/mod.rs` | App mit Testschlüsseln, `config.get/set`, ai-mock mit Test-Adapter, Live-Modelllisten A–D aus dem Mock |
| `crates/os-app/tests/acceptance_u01_settings.rs` | AI-011, INTEG-005 [M4], AI-019 (12 Tests) |
| `crates/os-app/tests/acceptance_u01_cost.rs` | UI-013 [M4], AI-013 (Anzeige, Budget erhöhen), AI-012 (6 Tests) |
| `crates/os-app/tests/acceptance_u01_consent.rs` | AI-018, AI-019-Folge (7 Tests, Wächter `guard::execute` gegen ai-mock) |
| `apps/desktop/src/features/ai/acceptance-u01.test.tsx` | Oberfläche: Feld Kosten, Dialoge, Schalter aus, Hinweise, de/en (15 Tests) |
| `apps/desktop/e2e/ai-cost/acceptance-u01.spec.ts` | E2E-A mit Bridge, chromium und webkit (6 Tests je Engine, axe, Tastatur, IPC-Zähler über WebSocket-Frames) |

## Prüfpunkte je Spec-ID

**UI-013 [M4] – Feld `Kosten`**
- Format `Cost: 3.2100 / 5.0000 USD` / `Kosten: 3,2100 / 5,0000 USD` mit dem Prüfwert der Spec (3,21 von 5 USD), Sprachwechsel
  ohne Neustart (dasselbe DOM-Element; in E2E per `settings.set` ohne Neuladen) – vitest, E2E.
- Werte aus dem `BudgetLedger`: ohne Crawl 0 / 5,00 USD (E-5 a); geöffnete Crawl-Datei mit `ai_requests` von zusammen
  3,21 USD → `spentMicroUsd = aiMicroUsd = 3 210 000`; Grenzen der Crawl-Datei; „Unbegrenzt“ → `Kosten: 3,2100 USD
  (unbegrenzt)` – Rust, vitest.
- Tooltip `KI: …`, `DataForSEO: …`, `Offene Reservierungen: …` im selben Zahlenformat, en mit Punkt – vitest, E2E.
- Sichtbar nur bei aktiver KI-Aufgabe; Embeddings und eigene Prompts zählen (Nutzerentscheidung 24.11.2026); Schalter an
  ohne Aufgabe → kein Feld – vitest, E2E.
- Warnschwelle 80 % (Standard) und einstellbar – vitest.
- Nicht geprüft: `--json-progress` (gehört C01, nicht U01).

**AI-012 – Dialog „Kostenvoranschlag“**
- Titel „Kostenvoranschlag“; Spalten exakt `Task, Model, Reasoning, Items, Cached, Est. Input Tokens, Est. Output Tokens,
  Est. Cost (low/expected/high), Batch Savings, Executable` – vitest, E2E.
- Zahlen = Kernschätzung (`TaskEstimate` über einen angemeldeten Schätzer, `ai.costEstimate`); Summe nur der ausführbaren
  Aufgaben; `Executable = nein` mit `Missing API key` bzw. `Price unknown`; Budget/Verbrauch/Rest aus dem Ledger – Rust,
  vitest, E2E (Budget des Kerns über die Bridge).
- Bestätigungsschwelle Standard 1,00 USD, über 50 % des Restbudgets, Testprofil 0,0001 USD – Rust; Dialog nur bei
  verlangter Bestätigung – vitest.
- Abbrechen/Escape geben nichts frei und lösen kein Kommando aus; Starten/Enter gibt frei; kein `ai.*Run` – vitest, E2E.

**AI-018 – Einwilligung und Datenfluss**
- 12 Spalten in Spec-Reihenfolge, `Stored by Provider` nach `Training Use (per provider policy)`; OpenAI Responses →
  `no (store: false)`, Decisions ohne `storage` → `unknown`, keine Zelle leer; CSV mit der Spalte – Rust, vitest, E2E.
- Ohne Einwilligung `Pending (consent)`, ai-mock zählt 0; nach Bestätigung genau 1 Request – Rust (Gate der App +
  `guard::execute`).
- Je Installation: `ai-consent.json` im Einstellungsordner, gilt nach App-Neustart und für einen zweiten Crawl; in E2E nach
  Neuladen kein Dialog – Rust, E2E.
- Neues Feld (`page_text`) → erneute Anfrage mit `addedFields = [page_text]` – Rust.
- Widerruf je Anbieter: sofort `Pending (consent)`, kein Request, gilt nach Neustart; Widerruf-Knopf der Übersicht – Rust, E2E.
- Schwärzung an/aus mit PII-Hinweis nur bei aus + `page_text` – Rust, vitest.
- axe ohne critical/serious, Fokus bleibt im Dialog, Escape schließt – E2E.

**AI-011 – Modellwahl je Aufgabe**
- 12 Aufgaben mit Standardmodell, Ersatzliste (Reihenfolge laut Nachtrag 25.11.) und Standardstufe (Nachtrag 24.11.);
  Decisions/Embeddings ohne Stufe – Rust.
- Nur verfügbare Stufen: gpt-6-luna = `reasoning_efforts` aus `pricing.json` + `model-default`; Mock-Modell A `off`/1024,
  B ohne `off`, effektiv `low`, Warnung, 2048; D ohne Stufen; `audit-summary` auf B gekappt auf 4000 – Rust.
- Gespeicherte Stufe nicht verfügbar → Warnung, Wert bleibt; knappe Ausgabegrenze → Hinweis (wörtlich laut Spec in de) – Rust,
  vitest.
- Ungültig abgelehnt: Nicht-Vision-Modell für `suggest-alt` (Feld `ai.tasks.suggest-alt.model`), unbekannte Stufe, unbekannte
  Aufgabe, negative/nicht-numerische Ausgabegrenze – Rust.
- Ersatzliste Reihenfolge, „Auf Standard zurücksetzen“, Profil-Roundtrip von Stufe und Ausgabegrenze; Zustand ohne Schlüssel
  `Error: Missing API key`, nur OpenAI-Schlüssel → Modelle laut Nachtrag 25.11. – Rust.
- Spalte „Reasoning“ auf der Konfigurationsseite KI, axe – E2E.

**INTEG-005 [M4]** – „Erweitert“: Ollama Standard 1, übrige KI-Anbieter 4 (AI-017, Hinweis des Orchestrators), Bereich 1–50;
51, 0 und Text werden mit Meldung abgelehnt und nicht gespeichert, 50 wird gespeichert – Rust, E2E (mit axe).

**AI-019 (1.2)** – nur die Entscheidung des Orchestrators vom 26.11.2026 und ihre Folgen geprüft: kein Richtlinienschalter
auf der Seite KI; `ai.settingsSet` lehnt `eu-only`/`local-only` ab (`InvalidInput`), `unrestricted` bleibt; keine EU-Schalter
in `ai.settingsGet` und im Einstellungsabschnitt KI; Datenflussübersicht ohne Zeile „Richtlinie“ und ohne Datenort (Spalte
`Region` leer, CSV ohne `EU`) – Rust, E2E. Übrige AI-019-Kriterien nicht geprüft (Auftrag).

**Schalter „KI (Vorschau)“ aus** – kein Feld, kein Dialog (auch bei direktem Aufruf von `showCostEstimate`), kein Abschnitt KI,
keine Seite KI, keine `ai.*`-Frames des Haupt- und Einstellungsfensters – vitest, E2E.

**de/en** – Katalog der Bereiche `settings`, `cost`, `consent` vollständig, keine identischen Texte; jeder im Code benutzte
Schlüssel (auch die dynamischen Stufen- und Feldnamen) in de und en vorhanden – vitest.

## Läufe und Testzahlen

Mac mini, unter `heavy.lock`, Playwright `--workers=1`, Bridge-Transport erzwungen (`OS_E2E_TRANSPORT=bridge`). Ab Runde 2
Bauordner `wt/ai-u01/target` (Anweisung des Orchestrators wegen Plattenplatz); der eigene Ordner `wt/ai-u01-tests/target`
aus Runde 1 ist gelöscht. Logs: `wt/ai-u01-tests/u01t-log/` auf dem Mini.

| Runde | Prüfzeile | Ergebnis |
|---|---|---|
| 1 | `aab28d69`, BAUER=`21286ea5` | fmt rot (nur Formatierung eigener Dateien), clippy rot (eigener Typ zu komplex), Rust 21/23 (1 eigener Testfehler: Modell D ohne `capabilities` ist korrekt nicht zuweisbar; 1 rot erwartbar AI-019), vitest 15/15, Playwright je Engine 4/5 (AI-019 erwartbar rot) |
| 2 | `ed7b7876`, BAUER=`49f8179e` | fmt, clippy -p os-app, typecheck grün; Rust settings 12/12, cost 6/6, consent 6/7 (neuer eigener Test: Einrichtung über `settings.set` scheitert, siehe B3-6); vitest 15/15; Playwright chromium 6/6, webkit 6/6 |
| 3 | `cf138d50`, BAUER=`49f8179e` | fmt, clippy -p os-app grün; consent 7/7 |

**Endstand (BAUER=`49f8179e`):** Rust 25 Tests (settings 12, cost 6, consent 7) – 25 grün, 0 rot; vitest 15 grün, 0 rot;
Playwright 12 grün (6 chromium, 6 webkit), 0 rot; ignoriert 0.

## Befunde

| Nr. | Schwere | Befund |
|---|---|---|
| B1-1 | B1 (erledigt) | AI-019 in 1.1 (Entscheidung 26.11.2026): Stand `21286ea5` zeigte auf der Seite KI die Richtlinie („Where may data go?“) und `ai.settingsSet` nahm `eu-only`/`local-only` an (Antwort `Ok`). Mit `49f8179e` behoben: Tests `ai019_*` und E2E „Konfigurationsseite KI … ohne Richtlinienschalter“ grün. |
| B2-1 | B2 | AI-011 „Das Reasoning-Dropdown bietet … für Modell D nur `model-default`“: Modell D hat keine `capabilities` und ist nach AI-002 nicht zuweisbar (`err-ai-model-incapable`), ein Dropdown der Aufgabe mit D entsteht also nie. Geprüft ist die Stufenliste der Auswahl (`reasoningLevels = []`). Spec sollte sagen, ob das Kriterium so gemeint ist. |
| B2-2 | B2 | Bestätigungsschwelle „über“ 1,00 USD: Gleichstand (genau 1,00 USD) nicht geprüft; die Spec sagt „über“, der Kern rechnet `>`. Ebenso Rundung von niedrig/hoch (× 0,75/1,25) bei krummen µUSD-Beträgen nicht festgelegt. |
| B2-3 | B2 | INTEG-005: „Eingabe außerhalb abgelehnt“ ist in der Oberfläche umgesetzt (51, 0, Text → Meldung, nicht gespeichert). Ob auch `settings.set` (IPC/MCP) Werte außerhalb 1–50 ablehnen muss, sagt die Spec nicht; nicht geprüft. |
| B3-1 | B3 | Die Dialoge „Kostenvoranschlag“ und „Datenfluss bestätigen“ haben in diesem Stand keinen Einstieg in der App (`requestCostConfirmation`/`requestConsent` ruft niemand auf; der Bereich `cost` mit „Kostenvoranschlag anzeigen“ ist nirgends eingehängt). Die E2E-Tests öffnen sie über die Modulfunktionen im laufenden Fenster. Einstieg folgt mit M4-D01. |
| B3-2 | B3 | `ai.costBudget` öffnet je Aufruf einen neuen Ledger über `open_ledger`; offene Reservierungen eines laufenden KI-Laufs erscheinen damit im Tooltip nur, wenn D01 denselben Ledger bzw. dieselbe Quelle teilt (heute immer 0). Mit D01 prüfen. |
| B3-3 | B3 | Im Command-Katalog steht `ai.costEstimate` mit Besitzer `M4-K05`, umgesetzt hat es U01 (Ausnahmeliste im F02-Test nennt es). Besitzer im Katalog nachziehen. |
| B3-4 | B3 | Feld `Kosten`: Embeddings und eigene Prompts machen das Feld auch bei `ai.enabled = false` sichtbar. Das entspricht der Nutzerentscheidung 24.11.2026 (zählen als aktive KI-Aufgabe); Hinweis nur, weil `ai.enabled` sonst als Hauptschalter gelesen werden könnte. |
| B3-5 | B3 | Die Übersicht „KI-Datenfluss“ im Einstellungsfenster zeigte in E2E die Aufgaben des Hauptfensters (Runde 1 und 2 grün). Welche Fensterkonfiguration sie bei mehreren Hauptfenstern zeigt, ist nicht festgelegt. |
| B3-6 | B3 | `settings.set` mit einem neuen Eintrag `aiApp.providerEndpoints.openai` → `err-unknown-setting`. In 1.1 passend (EU-Endpunkte ausgeblendet); für 1.2 (AI-019 „je Anbieter Schalter“) muss die Einstellung wieder annehmbar sein. Der eigene Test richtet deshalb keinen EU-Endpunkt mehr ein. |

## Durchsicht der geänderten Fremdtests (Diff `c7d9c424...21286ea5`)

| Datei | Geänderte Zusicherung | Beurteilung |
|---|---|---|
| `crates/os-app/tests/acceptance_f02_ai_ipc.rs` | Neue Ausnahmeliste `U01_IMPLEMENTED` (`ai.settingsGet`, `ai.settingsSet`, `ai.costEstimate`, `ai.costBudget`, `ai.consentStatus`, `ai.consentGrant`); diese sechs werden in `ai_stubs_answer_not_implemented_with_switch_off_and_on` übersprungen (Schalter aus und an). | **Gedeckt.** Der Test prüft die Stub-Eigenschaft (Plan M4 3.3: der Besitzer ersetzt den Stub); für umgesetzte Commands kann sie nicht mehr gelten. Die Liste ist eng (genau die sechs, Name für Name), alle übrigen Commands bleiben geprüft. Keine Lockerung einer Spec-Zusicherung: Die Spec verlangt nicht, dass diese Commands bei Schalter aus scheitern (E-2 b: keine KI-Oberfläche, kein KI-Request; beides prüfen diese Abnahme in vitest/E2E und der Wächter). Kleine Lücke: Der übersprungene Teil „ein bekannter Command ist nie `NotFound`“ entfällt für die sechs; durch die Abnahmetests (erfolgreiche Aufrufe) gedeckt. Hinweis: `ai.costEstimate` gehört laut Katalog K05 (B3-3). |
| `apps/desktop/e2e/ai/preview.spec.ts` | Erwarteter Text des Feldes nach Aktivierung von `Cost: – / – USD` (Platzhalter) zu `Cost: 0.0000 / 5.0000 USD`. | **Gedeckt, strenger als vorher.** UI-013 [M4] verlangt Werte aus dem `BudgetLedger` mit 4 Nachkommastellen; ohne Crawl-Verbrauch und mit dem Standardbudget 5,00 USD (E-5 a) ist `0.0000 / 5.0000` der richtige Wert. Der Platzhalter war ausdrücklich „bis M4-U01“. Übrige Zusicherungen des Tests unverändert (Feld fehlt ohne aktive Aufgabe und bei Schalter aus). |

## Nicht geprüft (mit Grund)

- `--json-progress` mit Rohwerten (UI-013): gehört C01.
- AI-019 außer der 1.1-Entscheidung (EU-Routing, Nur lokal, Szenarien `region-*`): Release 1.2, Auftrag.
- AI-012 [M5] (Abschnitt DataForSEO, Summenzeile beider Bereiche): M5.
- AI-018 CLI `--ai-consent` und Feldmarker-Prüfung gegen echte Requests: gehören D01/K07 (Wächter und Feldauswahl im Kern).
- Kostenvoranschlag mit echter Kernschätzung aus Aufgaben eines Crawls: Schätzer melden die Fachpakete (D01, V01) an; geprüft
  wurde die Abbildung jeder Kernschätzung (`TaskEstimate`) in den Dialog.
- Drag & Drop der Ersatzliste in der Oberfläche: geprüft ist die Reihenfolge im Command (Entwurf `ai.tasks`).

## Offene Fragen an die Spec

1. AI-011: Kriterium „Dropdown für Modell D nur `model-default`“ bei einem nicht zuweisbaren Modell (B2-1).
2. AI-012: Gleichstand an der Bestätigungsschwelle und Rundung von niedrig/hoch (B2-2).
3. INTEG-005: Bereichsprüfung auch über IPC/MCP oder nur im Reiter „Erweitert“ (B2-3).
4. AI-018: Welche Konfiguration zeigt die Übersicht „KI-Datenfluss“ im Einstellungsfenster bei mehreren Fenstern (B3-5).
