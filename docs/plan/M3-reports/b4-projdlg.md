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
  Endung), `splitPath`, `fileName`, `recentCrawlTitle`, `recentMenuLabel`.
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
  in der Mitte gekürzt (**feste Grenze**). Datei → enthält **zwei** gleichnamige Untermenüs „Zuletzt geöffnet“:
  `file.recent` aus `ui/menu/contributions/main.ts` (Labels aus `help/recent.ts` `loadRecent`) und
  `crawls.recent` aus `CrawlsHost.tsx`. Beide nutzen jetzt dieselbe Hilfsfunktion. Ob eines davon entfallen
  soll, ist eine **offene Produktfrage**.
- **Datenquelle:** Ist die Verwaltung offen, kommen die Daten aus ihren Zeilen. Ist sie geschlossen, liefert
  eine Index-Abfrage `crawls.list` (die gleiche Abfrage, die `loadRecent` schon nutzt) die Daten. Sie läuft
  einmal bei Änderung der Liste bzw. des Crawl-Zustands, keine Abfrage je Datei und kein Backend-Eingriff.
- **Spec:** CRAWL-127 hat ein neues Akzeptanzkriterium als OS-Festlegung (Nutzerwunsch 09.11.2026).
- **Tests:** `src/ui/recentCrawl.test.ts` prüft Mac- und Windows-Pfad, kurze Pfade unverändert, den Erhalt
  des Dateinamens, ein überlanges Dateiende sowie Titel in de und en. `crawls.test.tsx` prüft die
  Menübeschriftung aus genau einer Index-Abfrage und den Render-Test (Tooltip und zugänglicher Name mit vollem
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
| `apps/desktop/src/features/help/recent.ts` | Hilfe/Menü (u04) | Label über `recentCrawlTitle` |
| `apps/desktop/src/features/help/help.test.tsx` | Hilfe/Menü (u04) | Erwartung an das neue Label |
| `apps/desktop/src/ui/menu/contributions/main.ts` | Menü | unbekannte Pfade per `middleEllipsis` |
| `apps/desktop/e2e/beta/n04-live.spec.ts` | Beta-Live-E2E | Regex für das Label (nicht ausgeführt) |
| `apps/desktop/e2e/tasks/tasks.spec.ts` | Planung | neuer Tipp-Test |
| `docs/spec/01-crawl.md` | Spec | CRAWL-127 Akzeptanzkriterium |

`apps/desktop/src/features/compare/` wurde nicht angefasst.
