# B4-PROJDLG – Tippen in den Formularen der Crawl-Verwaltung; lesbare „Zuletzt geöffnet“-Einträge

Stand: 09.11.2026 · Branch `m3/b4-projdlg` (Basis `main` `a675fefa`) · Tests: Linux-Build-Server (Worktree `b4-projdlg`)
Ergebnis: PAKET FERTIG (nicht gepusht)

## 1. Fehler „Projekt anlegen: man kann nichts eintippen“

### Ursache

Die vermutete Ursache hat sich **nicht bestätigt**. Dass die Unterdialoge als React-Geschwister des
Verwaltungsdialogs gerendert werden, ist mit react-aria-components 1.22.1 unschädlich. Gegenprobe: Mit dem
eigentlichen Fix und weiterhin als Geschwister bestehen dieselben E2E-Tests in Chromium und WebKit (Tippen,
Tab-Fokusfalle im Formular, Esc, Fokus-Rückgabe). Dasselbe zeigt der Aufgaben-Editor über dem Planungsdialog
(siehe „Suche nach demselben Muster“).

Die tatsächliche Ursache liegt in `CrawlsHost.tsx`, in der Fokus-Haltung aus U15 (`useLayoutEffect` mit
`keepFocus`). Sie hing an **jedem** `focusout` und holte den Fokus per `queueMicrotask` auf den obersten
`[role=dialog]`, wenn `document.activeElement` gerade `body` war. Beim echten Mausklick in ein Feld löst der
Browser erst `focusout` am alten Element aus und erst danach `focusin` am Feld. Nach einem vom Browser
ausgelösten Listener werden Mikrotasks sofort abgearbeitet. `keepFocus` lief also genau dazwischen, sah `body`
und fokussierte den Dialog. Der Browser bricht den laufenden Fokuswechsel dann ab, und das Feld bekommt den
Fokus nie. Belegt ist das mit einem Fokus-Ereignisprotokoll: `focusout` mit `relatedTarget=INPUT`, danach kein
`focusin` am Feld, sondern `focusin` am Dialog mit dem Aufrufer `keepFocus`.

Betroffen waren **alle** Textfelder der Verwaltung, auch die Suche im großen Dialog, nicht nur die Formulare.
Die bisherigen Tests merkten es nicht, denn `fireEvent.change`, Playwright-`fill()` und `pressSequentially()`
setzen den Fokus per Skript, und dabei laufen Mikrotasks erst danach.

Zweite, kleinere Ursache: Das Formularfeld hatte natives `autoFocus`. React setzt den Fokus in der
Commit-Phase, bevor der Fokusbereich des neuen Modals registriert ist. Der Fokusbereich des Verwaltungsdialogs
holte den Fokus deshalb sofort zurück, und Tippen direkt nach dem Öffnen ging ins Leere.

### Fix (`apps/desktop/src/features/crawls/CrawlsHost.tsx`)

- `keepFocus` greift nur noch, wenn der Fokus wirklich ins Leere fällt (`relatedTarget` leer) und das
  verlassene Element noch im DOM ist. Den ursprünglichen Zweck behält es: Wird ein gedrückter Button
  deaktiviert, holt es den Fokus wie bisher sofort zurück. Wird ein Element zusammen mit einem schließenden
  Formular entfernt, gibt die Fokus-Rückgabe von react-aria den Fokus wieder an den Auslöser.
- Das Formularfeld erhält den Fokus per Effekt nach dem Mounten (`ref` statt `autoFocus`), der Inhalt wird
  markiert. Tippen ersetzt also den bisherigen Namen.
- Keine Änderung an `Dialog.tsx` und an der Struktur der Unterdialoge. Unverändert bleiben auch Esc, die
  Fokus-Rückgabe an „Verwalten“ und die Hinweise bei geschlossenem Dialog (`os-crawls-notices`).

### Testnachweis

Neu in `apps/desktop/e2e/crawls/management.spec.ts` (`// spec: OUT-228`, `UI-124`):

- „typing reaches the create project, rename and move forms and enables Apply“: Klick ins Feld bzw. Tippen
  direkt nach dem Öffnen, immer mit `page.keyboard.type` (kein `fill`, kein `pressSequentially`). Abgedeckt
  sind Suche, Projekt anlegen, Projekt umbenennen (Enter), Umbenennen und Verschieben. Geprüft werden Fokus,
  Wert, aktives „Übernehmen“ und die IPC-Argumente.
- „Escape in a form closes only the form and returns focus to its trigger“.

Gegenprobe gegen den unveränderten `CrawlsHost.tsx` auf dem Server (`git checkout -- …CrawlsHost.tsx`):
beide neuen Tests **rot** in Chromium und WebKit (4 failed). Suche `Expected "Alpha", Received ""`, die
Fokus-Rückgabe schlägt fehl. Mit dem Fix sind sie grün.

### Suche nach demselben Muster (zweiter Dialog, während ein anderer modal offen ist)

| Stelle | Befund | Maßnahme |
|---|---|---|
| `features/tasks/TasksHost.tsx` – `TaskEditor` als Geschwister des Planungsdialogs (Textfelder) | gleich gelagert; funktioniert, kein `keepFocus`-Muster | neuer E2E-Test mit echtem Tippen in `e2e/tasks/tasks.spec.ts` (grün) |
| `features/crash/CrashHost.tsx` – `ReportDialog` (Notiz, E-Mail) öffnet über der offenen `ListDialog` | gleich gelagert, laut Analyse unkritisch (kein `keepFocus`) | nur gelistet; E2E braucht Bridge und Mock-Server, nicht ausgeführt |
| `features/help/HelpDialogs.tsx` – `PathDialog` (Pfadfeld) neben dem Hilfe-Dialog | nur Browser-/Bridge-Ersatz für native Dateidialoge (`files.ts`), in der App nicht erreichbar | nur gelistet |
| `features/mcp/McpHost.tsx` – Verbindungsdialog | `openConnect` schließt den MCP-Dialog vorher, nie gleichzeitig offen | keine |
| `features/detail/frame/DetailFrame.tsx`, `table/UpperTable.tsx`, `header/Header.tsx`, `settings/sections/storage`, `settings/sections/email` | Geschwister-Dialoge, nie gleichzeitig offen bzw. ohne Texteingabe | keine |
| `focusout`-Listener mit Fokus-Rückholung | nur in `CrawlsHost.tsx` (`help/menuContexts.ts` liest nur) | behoben |

## 2. „Zuletzt geöffnet“ lesbar, Pfad in der Mitte gekürzt (Zusatzaufträge 09.11.)

- **Hilfsfunktion** `apps/desktop/src/ui/recentCrawl.ts`: `middleEllipsis` (Trenner `/` und `\`; Anfang,
  ein kurzer Elternordner und der Dateiname bleiben, nur ein überlanger Dateiname wird selbst gekürzt, mit
  Endung), `splitPath`, `fileName`, `recentCrawlTitle`. (Stand nach den Nachträgen: `recentMenuLabel` gibt es nicht mehr; das Menü
  nutzt `entry.label ?? middleEllipsis(path)` in `ui/menu/contributions/main.ts`.)
- **Crawl-Verwaltung:** Die Einträge sind gleich breit und linksbündig (`ul`/`li`). Hauptzeile:
  Crawl-Name, ersatzweise der Host der Start-URL, dazu „ – “ und Datum mit Uhrzeit des Crawlstarts, im Format
  der UI-Sprache. Ist der Crawl unbekannt, steht dort der Dateiname. Darunter klein und gedämpft der Pfad,
  einzeilig. **Variante: breitenabhängig per CSS.** Der Pfad steht in zwei Spans: Der Anfang schrumpft mit
  `text-overflow: ellipsis`, Elternordner und Dateiname schrumpfen nicht. Der volle Pfad steht als `title` am
  `li` und im zugänglichen Namen (`<Titel>, <Pfad>`, bei unbekannten Crawls nur der Pfad). Ein Layoutfehler
  wurde mitbehoben: Lange Pfade machten das Raster des Dialogs breiter (`min-width: 0`,
  `minmax(0, 1fr)`, `box-sizing`).
- **Menü:** Native Menüs können Text nicht messen und haben keine Tooltips (`MenuItemSpec` kennt keinen). Sie
  zeigen deshalb eine Zeile `Name – Datum Uhrzeit`. Unbekannte Crawls zeigen den Pfad, auf höchstens 64 Zeichen
  in der Mitte gekürzt (**feste Grenze**). Das zunächst doppelte Untermenü ist seit dem Nachtrag „Abnahme“ (unten)
  auf eines reduziert.
- **Datenquelle (Stand nach den Nachträgen):** Die Labels bildet `loadRecent` (`help/recent.ts`) aus einer
  einzigen Index-Abfrage `crawls.list` für die ganze Liste, keine Abfrage je Datei und kein Backend-Eingriff. Die
  Verwaltung hat keine eigene Index-Abfrage mehr; ist sie offen, haben ihre frischen Zeilen Vorrang (z. B. nach
  Umbenennen). Neu geladen wird bei Crawl-Wechsel, Phasenwechsel, Änderung der Liste und Sprachwechsel.
- **Spec:** CRAWL-127 hat ein neues Akzeptanzkriterium als OS-Festlegung (Nutzerwunsch 09.11.2026).
- **Tests:** `src/ui/recentCrawl.test.ts` prüft Mac- und Windows-Pfad, kurze Pfade unverändert, den Erhalt
  des Dateinamens, ein überlanges Dateiende sowie Titel in de und en. `crawls.test.tsx` prüft
  (Stand nach den Nachträgen) die Übernahme der alten Liste, das Entfernen bei `NotFound` und den Render-Test (Tooltip und zugänglicher Name mit vollem
  Pfad, Titel, Kopf- und End-Span). `help.test.tsx` deckt das neue `loadRecent`-Label ab. Der E2E-Test „recent
  entries …“ in `management.spec.ts` zeigt bei 640 px Breite eine Pfadzeile mit Ellipse, den vollständigen
  Dateinamen, gleiche Breite und linksbündige Einträge ohne Überlauf.
- **Angepasste bestehende Tests** (geänderte Anforderung, nicht gelockert): Die Erwartung an das
  `loadRecent`-Label in `help.test.tsx` ist neu. In `e2e/beta/n04-live.spec.ts` prüft der Regex jetzt `– … Jahr`
  statt eines ISO-Datums. Dieser 16-Minuten-Lauf (5 000 URLs) wurde **nicht** ausgeführt.
- **Visuelle Baselines:** keine betroffen. Kein Visual-Spec zeigt die Crawl-Verwaltung.

## Testläufe (Server, `srv-run.sh b4-projdlg`)

| Befehl | Ergebnis |
|---|---|
| `npm run -w apps/desktop typecheck && npm run -w apps/desktop test` | CSS-Lint OK, 124 Dateien / 1 057 Tests grün |
| `npx playwright test -c e2e/crawls/playwright.config.ts --workers=1 --project=chromium --project=webkit` (mit eigener Bridge `cargo build --locked -p os-app --features e2e-bridge --bin os-app-bridge`) | 22 passed (management + bridge, beide Engines) |
| `npx playwright test -c e2e/tasks/playwright.config.ts --workers=1 --project=chromium --project=webkit` | 18 passed |
| Gegenprobe ohne Fix (nur `CrawlsHost.tsx` zurückgesetzt), neue Crawls-Tests | 4 failed (erwartet) |

Kein Rust geändert, daher kein Workspace-`cargo test`.

## Fremde Dateien (K2-92)

| Datei | Bereich | Änderung |
|---|---|---|
| `apps/desktop/src/features/help/recent.ts` | Hilfe/Menü (u04) | Label über `recentCrawlTitle`; `onRecentChanged` (Nachtrag Abnahme) |
| `apps/desktop/src/features/help/help.test.tsx` | Hilfe/Menü (u04) | Erwartung an das neue Label |
| `apps/desktop/src/features/help/install.ts` | Hilfe/Menü (u04) | lädt „Zuletzt geöffnet“ bei `onRecentChanged` neu (Nachtrag Abnahme) |
| `apps/desktop/src/ui/menu/contributions/main.ts` | Menü | unbekannte Pfade per `middleEllipsis` |
| `apps/desktop/e2e/beta/n04-live.spec.ts` | Beta-Live-E2E | Regex für das Label (nicht ausgeführt) |
| `apps/desktop/e2e/tasks/tasks.spec.ts` | Planung | neuer Tipp-Test |
| `docs/spec/01-crawl.md` | Spec | CRAWL-127 Akzeptanzkriterium |

`apps/desktop/src/features/compare/` wurde nicht angefasst.

## Nachtrag: fehlende Crawl-Datei als `NotFound` (Kern-Fix aus `m3/b4-cmpui`)

- Prüfung: Die Verwaltung behandelt `NotFound` schon direkt (`missing = err.code === "NotFound"`). Der Eintrag
  wird aus beiden Listen entfernt (lokale Liste und `uistate` per `updateRecent`), der Hinweis
  „<Pfad>: The file was not found.“ erscheint. Das gilt für die Liste in der Verwaltung und für Datei →
  `crawls.recent`, die über dieselbe Funktion `openNow` laufen. `file.recent` (`help/install.ts`) entfernt bei
  `NotFound` ebenfalls mit Hinweis `u04-recent-missing`. Es war **kein Fix nötig**.
- Der Kommentar in `CrawlsHost.tsx` ist angepasst. Der Ausweg über `Io` und die Dateiprüfung bleibt für
  ältere Kerne erhalten und ist harmlos.
- Neuer Test in `crawls.test.tsx`: „removes a recent entry the core reports as NotFound with its path …“. Der
  Fehler hat die Form des Kerns (`err-file-not-found` mit `path`). Geprüft werden das Entfernen, der
  Hinweistext und dass `desktop.check` nicht aufgerufen wird. Der Test ist auf diesem Branch allein grün.
  **Kein Test braucht beide Branches.** Den Kern-Fix habe ich nicht nachgebaut.
- Lauf: `npm run -w apps/desktop typecheck && npm run -w apps/desktop test` mit 124 Dateien / 1 058 Tests grün.

## Nachtrag: Befunde der unabhängigen Abnahme (`m3/t-projdlg` e9f8d25a)

### 1. „Zuletzt geöffnet“ stand zweimal im Menü Datei

Nach der Code-Lage entfällt die Registrierung in der Verwaltung (`crawls.recent` in `CrawlsHost.tsx`). Es bleibt
`file.recent` aus `installMainMenus()` (`help/install.ts`, `ui/menu/contributions/main.ts`). Gründe:

- Es sitzt an der Stelle, die die Spec vorsieht (05-ui, Menü Datei: nach „Öffnen“).
- Es speichert im UI-Zustand des Kerns statt im `localStorage` einer Webview.
- Es lädt seine Labels beim Start aus dem Index, funktioniert also auch, wenn die Verwaltung nie geöffnet wurde.
- Es entfernt bei `NotFound` den Eintrag mit Hinweis (`u04-recent-missing`).
- `n04-live` und `main.test.ts` prüfen schon diese IDs (`recent.N`).

Umsetzung:

- **Eine Liste:** Die Verwaltung liest und schreibt jetzt dieselbe Liste im UI-Zustand (`loadRecent` /
  `updateRecent`). Öffnen setzt den Eintrag an den Anfang, `NotFound` entfernt ihn.
- **Änderungsmeldung:** `help/recent.ts` meldet jede Schreibänderung (`onRecentChanged`). Verwaltung und
  Hauptmenü laden daraufhin neu, ohne auf einen Sitzungswechsel zu warten.
- **Einmalige Übernahme:** Die frühere lokale Liste (`openspider.recent-crawls.v1`) wird beim ersten Start
  übernommen, hinter die Einträge im UI-Zustand, ohne Dubletten, höchstens 10. Danach wird der Schlüssel
  gelöscht. Schlägt das Speichern fehl, bleibt die alte Liste erhalten.
- **Titel in der Verwaltung:** Name/Domain und Datum aus den frischen Zeilen der offenen Verwaltung, sonst das
  Label aus dem Index (`loadRecent`). Die zusätzliche eigene Index-Abfrage der Verwaltung ist entfallen.
- **Spec:** CRAWL-127 ergänzt um „eine gemeinsame Liste, genau ein Untermenü“.
- **Tests:** `crawls.test.tsx` liest die Liste jetzt aus dem UI-Zustand. Neue Tests: kein zweites Untermenü aus
  der Verwaltung; einmalige Übernahme der alten Liste mit erneutem Mount ohne Dubletten; Öffnen aus der
  Verwaltung setzt den Eintrag an den Anfang der gemeinsamen Liste.

### 2. Mod+F, solange der Dialog selbst den Fokus hat

Der Handler hing an `.os-crawls` und sah deshalb keine Tasten, wenn der Fokus auf `section.os-dialog` lag (direkt
nach dem Öffnen). Jetzt hängt ein Listener am Dokument, solange die Verwaltung offen ist. Mod+F fokussiert die
Suche, wenn das Ziel im Verwaltungsdialog liegt, auch wenn das Ziel der Dialog selbst ist. In einem Formular
darüber (eigenes Modal) bleibt Mod+F beim Formular. Der globale Kürzel-Dispatcher ignoriert Tasten bei offenem
Modal, es gibt also keinen Konflikt. Neuer E2E-Test in `management.spec.ts` (OUT-228).

### Läufe (Server), Abnahmetests nur temporär eingespielt

| Befehl | Ergebnis |
|---|---|
| `npm run -w apps/desktop typecheck && npm run -w apps/desktop test` (mit `acceptance-projdlg.test.tsx`) | 1 067 grün, **1 rot** (Abnahmetest, siehe unten); ohne den Abnahmetest alle eigenen Tests grün |
| `playwright test -c e2e/crawls/playwright.config.ts --workers=1`, Chromium und WebKit | 24 passed |
| `playwright test e2e/acceptance/b4-projdlg.spec.ts --workers=1`, Chromium und WebKit | 52 passed (inkl. Mod+F, „genau einmal“ in der echten App-Hülle) |
| `playwright test -c e2e/tasks/playwright.config.ts --workers=1`, Chromium und WebKit | 18 passed |

Die beiden Abnahmedateien wurden danach wieder entfernt und nicht committet.

### Abnahmetest, den ich für falsch halte

Der Test „Menü Datei, Zuletzt geöffnet: bekannter Crawl als „<Name> – <Datum Uhrzeit>“ in einer Zeile“ in
`acceptance-projdlg.test.tsx` sucht Menüeinträge mit den IDs `crawls.recent.N`. Das sind die Implementierungs-IDs
des zweiten Untermenüs, das nach dem Abnahmebefund 1 und der Entscheidung des Orchestrators entfällt. Der Test
widerspricht damit dem Abnahmetest „ein Eintrag erscheint nicht doppelt, das Untermenü ist genau einmal
vorhanden“: Beide zugleich sind nur mit zwei Untermenüs bzw. mit Verwaltungs-IDs im einzigen Untermenü
erfüllbar. Er baut das Hauptmenü zudem mit leerer Liste (`fileMenu()`), sodass nur ein zweites Untermenü Einträge
liefern könnte. Das geforderte Verhalten (bekannter Crawl als „Name – Datum Uhrzeit“ in einer Zeile im Menü
Datei) prüfen der grüne Abnahmetest „… aus dem Speicher trägt Name/Domain mit Datum und Uhrzeit …“ (IDs
`recent.N`) sowie `help.test.tsx`. Vorschlag für den Testagenten: dieselbe Prüfung über `recent.N` mit den
Einträgen aus `loadRecent()`.

## Nachzug B4-PROJDLG2 (Folgepunkte A1–A6 nach Fable approved 88)

Branch `m3/b4-projdlg2` ab `main` `dd855e51`.

- **A1** `help/install.ts`: Die Menü-Labels von „Zuletzt geöffnet“ werden nach einem Sprachwechsel neu gebildet
  (`onLanguageChanged` → `reloadRecent`). Test in `help.test.tsx`: Das Label wechselt von
  „uuid – Oct 29, 2026, 12:00 PM“ zu „uuid – 29.10.2026, 12:00“.
- **A4** `crawls/model.ts` und `CrawlsHost.tsx`: Der Legacy-Schlüssel wird erst nach erfolgreichem
  `updateRecent` gelöscht (`readRecent` → `updateRecent` → `clearLegacyRecent`). Schlägt das Speichern fehl,
  bleibt er für den nächsten Start; die Übernahme ist idempotent. `takeLegacyRecent` entfällt. Test in
  `crawls.test.tsx`: Speichern schlägt fehl, der Schlüssel bleibt; beim nächsten Mount wird übernommen und erst
  dann gelöscht.
- **A5** CRAWL-127: Tooltip-Satz präzisiert („In der Verwaltung …“, das native Menü kennt keine Tooltips).
- **A6** Abschnitt 2 dieses Berichts korrigiert (`recentMenuLabel` und die eigene Index-Abfrage der Verwaltung
  gibt es nicht mehr).
- **A3** `crawls.test.tsx`: Der Kappungs-Test prüft jetzt die gemeinsame Liste im UI-Zustand. Er ist strenger:
  exakte Liste mit 10 Einträgen, neueste zuerst, Dubletten und leere Pfade entfernt. Dazu kommt `normalizeRecent`
  mit ungültigen Einträgen und erhaltenem Label.
- **A2** Seeding der eigenen Recent-Tests über den UI-Zustand: In `crawls.test.tsx` war das schon so (`seedRecent`),
  nur der Übernahme-Test nutzt bewusst den Legacy-Schlüssel. `e2e/crawls/management.spec.ts` seedet jetzt über
  `window.crawlsFixture.seedRecent` (neu in `e2e/crawls/harness.tsx`, ruft `updateRecent`). Die Abnahmedateien des
  Testagenten (`e2e/acceptance/b4-projdlg.spec.ts`, `src/features/crawls/acceptance-projdlg.test.tsx`) seeden
  weiter über den Legacy-Schlüssel und laufen damit über die Übernahme. Sie sind **nicht geändert**.

Läufe (Server, `srv-run.sh b4-projdlg`): `npm run -w apps/desktop typecheck && npm run -w apps/desktop test` mit
125 Dateien / 1 070 Tests grün (die Abnahmedateien sind seit dem Merge in `main` enthalten und liefen mit).
`e2e/crawls` (Chromium und WebKit, `--workers=1`) 24 passed; `e2e/acceptance/b4-projdlg.spec.ts` 52 passed. Die
Bridge für `e2e/crawls/bridge.spec.ts` ist der Build vom Stand `a675fefa`. Kein Rust geändert.
