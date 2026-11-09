# B4-CMPUI – Vergleichsansicht: Layout, Spalten, Dateifehler, Virtualisierung

Auslöser: Nutzerbericht beta.3 (macOS, Fenster etwa 2000 px), Vergleich zweier Crawls von caloone.de.
Maßgeblich: `docs/spec/06-output-ops.md` OUT-140 bis OUT-151 (führend OUT-142), `docs/spec/01-crawl.md` CRAWL-032 bis 036,
CRAWL-127, CRAWL-144, `docs/spec/05-ui.md` UI-007, UI-013, UI-121, UI-140, UI-142.

## Befund 1 – Steuerleiste und Tabelle als kleiner Block in der Mitte

**Ursache.** Der Tabellenbereich der Shell (`.os-app-main`, `apps/desktop/src/app/app.css`) ist ein Flex-Container mit
`align-items:center; justify-content:center`. Die normale Tabelle (`.u10-table`) setzt sich selbst auf
`width:100%; height:100%`, die Vergleichstabelle (`.os-compare-table`) nicht. Dazu hatte `ComparisonGrid` eine feste
Höhe von 360 px, die Vergleichsübersicht in der Seitenleiste `max-height:45vh` (im Screenshot bei etwa 600 px
abgeschnitten, darunter leer) und das Detailfenster keinen Innenabstand („URL auswählen …“ am linken Rand).

**Fix.** `compare.css`: Tabelle, Detailbereich, Linktabelle und Seitenleiste sind Flex-Spalten mit 100 % Breite und
Höhe; Werkzeugleiste oben links (`.os-compare-toolbar`), das Raster nimmt die Resthöhe (`.os-compare-grid-host`,
`flex:1`), darunter der Zeilenzähler wie `.u10-counter`. Die gemeinsame `app.css` bleibt unverändert. Die
Spaltenauswahl („+“) steht jetzt in der Werkzeugleiste statt in einer eigenen Zeile.

**Spec.** Kein Widerspruch zum Wunsch: 05-ui.md verlangt nur, dass das Layout den Vergleichsmodus tragen kann; UI-007
gilt für jede obere Tabelle.

**Test.** `apps/desktop/e2e/compare/layout.spec.ts` (echter Kern über die E2E-Bridge, Hauptfenster 2000 × 1100):
Vergleichsbereich ≥ Breite/Höhe des Tabellenbereichs − 2 px, Raster ≥ Bereichshöhe − 140 px, Filter höchstens 160 px
vom linken und 60 px vom oberen Rand, Seitenleisten-Übersicht in voller Höhe.
- Gegenprobe mit dem alten Frontend (`git checkout a675fefa -- apps/desktop/src/features/compare …`):
  `Expected: >= 1492, Received: 631.796875` (Tabellenbreite) → rot.
- Neu: `npx playwright test e2e/compare --workers=1 --project=chromium` → 2 passed; `--project=webkit` → passed.

## Befund 2 – Spalten

**Spec.** Je Filter zeigt die *Übersicht* (Seitenleiste) `Current`, `Previous`, `Change`, `Added`, `New`, `Removed`,
`Missing` (OUT-142, Ausgabe); das ist im Screenshot rechts korrekt. Die *Haupttabelle* hat laut OUT-142 die
Unterfilter Added/New/Removed/Missing (Auswahl „Vergleichsstatus“), laut OUT-140 „alle Tabs/Filter mit
Vergleichsspalten“; welche das sind, nennt nur OUT-151 (für Exporte): die Spalten des Current-Crawls plus
`Comparison Status`, bei aktivem URL-Mapping `Previous Address`, im Tab Change Detection die `Previous …`-Spalten
(TECH-301). Zeilenweise Current/Previous-Werte je Spalte sieht die Spec für die URL-Tabs nicht vor.

**Ursache.** Der Kern lieferte die richtigen Spalten (Standardspalten des Tabs + `Comparison Status`, ggf.
`Previous Address`), aber `Comparison Status` steht am Ende von etwa 60 Spalten und war ohne langes horizontales
Scrollen unsichtbar. Das Frontend behandelte jede Spalte als Text mit 180 px Breite und zeigte den englischen
Kanon-Namen als Kopf (im deutschen UI „Address“ statt „Adresse“). Zahlen standen deshalb linksbündig. Im Screenshot ist
das Raster horizontal verschoben: Die „fast leere“ Spalte ist eine Spalte (vermutlich `Title 1 Length`, die
Standardspalte vor `Title 1 Pixel Width`), die zum größten Teil unter der angehefteten Spalte `Address` liegt. Ihre
linksbündigen Werte und der Anfang ihres Kopfs sind verdeckt. Die Auswahl der Spalten war also richtig, falsch waren
Typen, Titel und die Position der Vergleichsspalte.

**Fix.** `ComparisonGrid.tsx`: Spaltenmetadaten (Typ, Ausrichtung, Breite) aus `catalog.columns` des Tabs wie im
normalen Modus (UI-007), Kopf über `columnLabel` lokalisiert (UI-121), Kanon-Namen bleiben Schlüssel für Sortierung,
Export und Konfiguration. `Comparison Status` und `Previous Address` bleiben Text, nicht sortierbar, und stehen direkt
nach `Address`. Für Change Detection liefert der Kern über `catalog.columns` eigene Metadaten; Link-Spalten ohne
Katalog bleiben Text.

**Test.** `comparison-grid.test.tsx` „shows the tab's ordinary columns with catalog kinds and localized headers …“
(`// spec: OUT-142`, `OUT-151`, `UI-007`): Kopfzeile in de = `Adresse, Comparison Status, Statuscode, Titel 1 Länge`,
Zellen typisiert. E2E `layout.spec.ts`: erste zwei Köpfe `Address`, `Comparison Status`, `Status Code` vorhanden.

**Unklar, bitte entscheiden.**
- Position der Vergleichsspalten in der Tabelle: OUT-151 sagt „zusätzlich zu“, nennt aber keine Reihenfolge. Ich habe
  sie direkt nach `Address` gesetzt (OS-Festlegung zur Bestätigung). Exportreihenfolge bleibt unverändert (angehängt).
- `Comparison Status`/`Previous Address` haben keine deutschen Spaltentitel im Kanon (`src/i18n/canon`); sie erscheinen
  englisch. Vorschlag „Vergleichsstatus“/„Vorherige Adresse“, falls der Kanon das zulässt.
- Der Kern projiziert nur die Standardspalten des Tabs (`ColumnSelection::Default`). Vom Nutzer im normalen Modus
  zusätzlich eingeblendete Spalten fehlen im Vergleich. Ob die normale Spaltenkonfiguration gelten soll, sagt die Spec
  nicht.

## Befund 3 – „Ein Dateivorgang ist fehlgeschlagen.: No such file or directory (os error 2)“

**Ursache.** Der Vergleich verursacht die Meldung nicht. Das App-Log (`~/Library/Logs/OpenSpider/openspider.log`) zeigt
um 15:06:52, knapp zwei Minuten vor dem Screenshot (15:08):

```
Crawl-Datei öffnen: begonnen [request={"kind":"path","path":"/private/tmp/claude-501/…/scratchpad/demo-home/crawls/01a1a16a-c208-76dc-8aa2-5b66f91f79ce.ospider"}]
Crawl-Datei öffnen: fehlgeschlagen [code="Io" detail="No such file or directory (os error 2)" reason="err-io"]
```

Das ist ein `crawl.open` mit einem **Pfad**, der auf ein nicht mehr existierendes Demo-Verzeichnis im
Scratchpad einer früheren Agent-Sitzung zeigt. Pfad-Öffnungen kommen im Frontend nur aus „Zuletzt geöffnet“ (Menü
Datei bzw. Liste in der Crawl-Verwaltung), Datei › Öffnen und dem Import-Ergebnis. Der Eintrag ist inzwischen aus
`ui-state.json` verschwunden; das passt zur Crawl-Verwaltung, die einen fehlenden Recent-Eintrag nach dem Fehler
entfernt. `compare.open` löst Crawls über die Verwaltung bzw. per ID auf, ruft nie `crawl.open` auf und erzeugt kein
`crawl-open-failed`. Die Statusleiste zeigt dieses Ereignis, bis das nächste Öffnen beginnt; deshalb stand es während
des Vergleichs noch da.

Zwei echte Fehler dabei:
1. Der Kern meldet eine fehlende Datei beim Öffnen als `Io`/`err-io` (Fehler aus `canonicalize`) statt als
   `NotFound`. CRAWL-127 verlangt: „nicht mehr vorhandene Dateien werden beim Anklicken mit Hinweis entfernt“. Das Menü
   Datei › Zuletzt geöffnet (`features/help/install.ts`) entfernt aber nur bei `NotFound`.
2. Die Statusleiste hängt das englische technische Detail mit `": "` an den lokalisierten Satz an. Daher „.: …“.
   Außerdem verletzt das die Regel in `ipc/errors.ts` (M2-ui-concept 2.6), nach der `detail` nie im Haupttext steht.

**Fix.**
- `crates/os-app/src/session/control.rs`: `ErrorKind::NotFound` aus `canonicalize` → `StoreError::NotFound(path)` →
  `ErrorCode::NotFound`, `err-file-not-found` mit Argument `path`.
- `features/statusbar/Statusbar.tsx`: Satz aus `messageKey` mit Argumenten, sonst Fehlerklasse; das Detail nur
  aufklappbar unter „Technische Details“. Neuer Text `err-file-not-found` („Die Datei wurde nicht gefunden: { $path }“)
  in `src/i18n/errors/i18n/{de,en}.ftl`.

**Tests.**
- Rust `crates/os-app/tests/crawl_cmd_open.rs::opening_a_missing_path_reports_not_found_with_the_path`
  (`// spec: CRAWL-127`): `cargo test -p os-app --test crawl_cmd_open` → 4 passed.
- Vitest `Statusbar.test.tsx` „keeps the technical detail out of the failure sentence and names a missing file“
  (`// spec: CRAWL-127`, `UI-124`): Text genau „Ein Dateivorgang ist fehlgeschlagen.“, kein „.:“, Detail in den
  Details; NotFound → „Die Datei wurde nicht gefunden: /tmp/gone/x.ospider“.
- E2E `layout.spec.ts`: Vergleich zweier gültiger Crawls → kein `crawl-open-error` und kein Alert mit
  „file operation/Dateivorgang/os error“; echtes `crawl.open` auf einen fehlenden Pfad → `NotFound`,
  `err-file-not-found`, Pfad im Argument.

**Nebenbefund (nicht behoben).** Ein `crawl-open-failed` öffnet im Hauptfenster zusätzlich einen modalen Dialog
„Crawl notice – Cause: crawl-open-failed“ mit dem rohen Ereigniscode (in der E2E-Spur gesehen). Ob die Statusleiste den
Fehler beim Öffnen eines Vergleichs löschen soll, lässt die Spec offen; unverändert.

## Befund 4 – „Vorherige Seite / Nächste Seite“ statt virtualisierter Tabelle

**Spec.** UI-007: „Jede obere Tabelle ist eine virtualisierte Datenansicht (UI-140)“. UI-140: „Alle Tabellen (oben,
Detailfenster, …)“ rendern nur Sichtbares, „Scrollbalken repräsentieren die volle Zeilenzahl“. UI-142: Fenster aus dem
Kern. Die Seitenknöpfe widersprechen der Spec; OUT-140 ff. sehen keine Ausnahme vor.

**Fix.** `ComparisonSource` (Unterklasse von `ViewDataSource`) zeigt den ganzen Bestand in einem Raster und holt
200-Zeilen-Fenster bei Bedarf über dieselbe zustandslose Lesung (`view.rows` mit `comparison`). Es gibt keine neuen
IPC-Kommandos. Das gilt für die Haupttabelle und die Inlinks/Outlinks-Tabelle (OUT-149). Neue Abfrage, Sortierung oder
Richtung → wieder ab Zeile 0. Kopieren läuft seitenweise über alle Fenster. Partner-IDs (OUT-146) werden je geladener
Zeile gemerkt.

**Tests.** `comparison-grid.test.tsx` „… loads further windows on demand“ (`// spec: TECH-301`, `UI-140`):
Zeile 201 ist zuerst `LOADING`, nach `ensureRange(195, 201)` geladen, dabei wird `load(200, 200)` aufgerufen.
`comparison.test.tsx` „shows all 105 Inlinks in one virtualized table …“: 105 Zeilen ohne Seitenknopf. E2E:
`aria-rowcount` = Zähler + 1, keine Knöpfe „Next/Previous page“.

**Risiko (nicht gemessen).** Jede Fensterabfrage projiziert serverseitig über die vollständige ID-Menge des Filters
(`query_url_ids_in`). Die Budgets von UI-141/142 bei Millionen Zeilen sind für den Vergleich nicht gemessen.

## Befund 5 – Weitere Abweichungen im Screenshot

1. Statusleiste und Kopf zeigen im Vergleich „Modus: Spider“ und Fortschritt/Datei des allein geöffneten Crawls. UI-013
   nennt `Modus` = Vergleich.
2. Tab-Leiste im Vergleich: einfache Knöpfe mit englischen Kanon-Namen (keine Lokalisierung über `tabLabel`, UI-121),
   keine gruppierte Navigation (K2-77, „Change Detection erscheint im Vergleichsmodus als eigener Eintrag“) und kein
   Pfeil-Dropdown für Tabs, die nicht passen (UI, Tab-Leiste). Bei 2000 px endet die Leiste bei „Links“;
   Change Detection und weitere Tabs sind nur durch verstecktes Scrollen erreichbar.
3. Filter-, Vergleichsstatus- und Quelle-Auswahl zeigen englische Kanon-Werte (All, Added, New, Current …). Für die
   Linkänderung nennt OUT-149 deutsche Anzeigewerte („Alle / Hinzugefügt / Entfernt / Ankertext geändert /
   Unverändert“), angezeigt werden die englischen Exportwerte.
4. Übersicht in der Seitenleiste: Zeilentitel englisch („Internal: All“), Zahlen nicht nach UI-Sprache formatiert
   (4438 statt 4.438), Spalten Removed/Missing bei Standardbreite abgeschnitten. Höhe und Rechtsbündigkeit sind jetzt
   behoben (Befund 1), Formatierung und Titel nicht.
5. „Ansicht exportieren (CSV)“ umgeht den gemeinsamen Exportdialog mit Formatwahl (UI-015/OUT-006). OUT-151 regelt
   nur den Inhalt; ob der Dialog Pflicht ist, ist unklar.
6. Die Kennzeichnung „https://caloone.de/ / …“ unterscheidet zwei gleichnamige Crawls nicht (kein Datum). Die Spec hat
   dazu keine Regel, nur ein Hinweis.

## Visuelle Baselines

Nicht geändert. Unter `apps/desktop/e2e/visual/baselines` gibt es keine Szene mit Vergleichsansicht oder
Öffnungsfehler (`change-detection.spec.ts` bindet nur `compare/bridge.spec.ts` ein, ohne Screenshot).

## Angepasste bestehende Tests (nicht gelockert)

- `comparison.test.tsx` „resets the page when switching from 105 Inlinks …“ prüfte die Seitenknöpfe. Jetzt prüft der Test
  dieselbe Rücksetzung (Anfrage mit `offset:0` nach Richtungswechsel) und zusätzlich alle 105 Zeilen ohne Knopf
  (UI-140). Die Fixture liefert jetzt wie der Kern einen `rowKey` je Zeile (vorher `rowKeys: []`).
- `comparison.test.tsx` „toggles Change % sorting …“: Der Klick auf „Next page“ entfällt; alle Sortier-, `offset:0`- und
  Exportprüfungen bleiben, dazu kommt „kein Seitenknopf“.
- `comparison-grid.test.tsx`: Die zwei Tests der entfernten `ComparisonPageSource` prüfen dieselben Eigenschaften an
  `ComparisonSource` (stabile Schlüssel, `locate`, `keys`, exakte Zahl `1.23456` jetzt als Rohwert statt String,
  Sortierbarkeit) und zusätzlich das Nachladen.
- `e2e/compare/harness.tsx`: Gibt Tabelle und Detailbereich eine feste Höhe, weil die Panels jetzt ihren Bereich füllen
  wie in der Shell.

## Prüfungen (Build-Server, Worktree `b4-cmpui`)

| Befehl | Ergebnis |
|---|---|
| `npm run -w apps/desktop typecheck` | ok |
| `npm run -w apps/desktop test` | 123 Dateien, 1051 Tests passed |
| `npx playwright test e2e/compare e2e/a11y/views.spec.ts e2e/config-fach/fach.spec.ts e2e/crawls --workers=1 --project=chromium` | 19 passed |
| `npx playwright test e2e/compare e2e/a11y/views.spec.ts:72 e2e/crawls/compare.pending.spec.ts --workers=1 --project=webkit` | 4 passed |
| `cargo fmt --all --check` | ok |
| `cargo clippy --workspace --all-targets -- -D warnings` | ok |
| `cargo test -p os-app` (mit `CARGO_PROFILE_*_DEBUG=0`) | 112 Testbinaries, 638 passed, 0 failed |

Kein workspaceweites `cargo test`: Rust geändert ist nur `os-app` (eine Fehlerabbildung), getestet wurde die ganze Crate.

## Branch und Commits

Branch `m3/b4-cmpui` (lokal, nicht gepusht), Basis `a675fefa`.

| Commit | Inhalt |
|---|---|
| `f8d4f28e` | ui: Vergleichsansicht füllt Bereiche, virtualisierte Tabellen, Spalten wie im normalen Modus |
| `562d52b7` | ui: Statusleiste – Öffnungsfehler als lokalisierter Satz, Details aufklappbar |
| `0ece723c` | app: fehlende Crawl-Datei beim Öffnen als NotFound mit Pfad (CRAWL-127) |
| `e2dca2e3` | ui: E2E-Prüfung des Vergleichslayouts, README der Vergleichsfunktion |
| (dieser) | docs: Bericht B4-CMPUI |

## Fremde Dateien (K2-92)

| Datei | Bereich | Art der Änderung | Spec-ID |
|---|---|---|---|
| `apps/desktop/src/features/statusbar/Statusbar.tsx`, `Statusbar.test.tsx` | UI Statusleiste | Fehlersatz ohne angehängtes Detail, `messageKey` mit Argumenten | CRAWL-127, UI-124 |
| `apps/desktop/src/i18n/errors/i18n/de.ftl`, `en.ftl` | Fehlertexte | neuer Schlüssel `err-file-not-found` | CRAWL-127 |
| `crates/os-app/src/session/control.rs` | App-Dienstschicht | fehlende Datei beim Öffnen → `NotFound` | CRAWL-127 |
| `crates/os-app/tests/crawl_cmd_open.rs` | Tests os-app | ein Test ergänzt | CRAWL-127 |
| `apps/desktop/e2e/compare/harness.tsx` | E2E-Harness Vergleich | feste Höhe für die Panels | OUT-142 |

Nicht angefasst: `apps/desktop/src/features/crawls/CrawlsHost.tsx`, `apps/desktop/src/ui/**`, `apps/desktop/src/app/app.css`.
Der Kommentar in `CrawlsHost.tsx` Z. 147 („Path canonicalization currently reports a missing file as Io“) ist nach diesem
Paket überholt. Die Prüfung über `desktop.check` bleibt dort harmlos; bitte den Kommentar beim Paket, dem die Datei
gehört, anpassen.

---

## Runde 2 (Ziel beta.4, Entscheidungen des Orchestrators vom 09.11.2026)

### Entscheidungen umgesetzt
- `Comparison Status` und `Previous Address` stehen direkt nach `Address`. Titel de „Vergleichsstatus“/„Vorherige Adresse“,
  en kanonisch (`src/i18n/canon/i18n/{de,en}.ftl`). Als OS-Festlegung in OUT-142 und OUT-151 eingetragen; die
  Exportspaltennamen bleiben kanonisch.
- **Spaltenkonfiguration des normalen Modus gilt im Vergleich (ohne Kernumbau).** Die Vergleichstabelle fordert die
  sichtbaren Spalten des Tabs aus `useColumns` in deren Reihenfolge an (`columns` von `view.rows`, das der Kern für
  Vergleiche schon als `ColumnSelection::Named` auswertet). Breiten und Anheftung kommen ebenfalls von dort. Änderungen
  im Vergleich (ein-/ausblenden, verschieben, Breite) speichert die normale Konfiguration (`useColumns.change`). Die
  zwei Vergleichsspalten behalten ihren Zustand für die Sitzung. Lehnt der Kern die Projektion ab (z. B. eine eigene
  Spalte, die der verglichene Crawl nicht hat), zeigt die Tabelle die Standardspalten des Tabs, ohne Fehlermeldung.
  Change Detection behält seine eigenen Spalten (TECH-301). Auch der CSV-Export nutzt diese Projektion.

### Befund 5 – behoben
| Punkt | Umsetzung | Test |
|---|---|---|
| Modus „Vergleich“ (UI-013) | Statusleiste und Modusauswahl im Kopf zeigen bei offenem Vergleich „Vergleich“; ein anderer Modus schließt den Vergleich zuerst | `Statusbar.test.tsx` „names the comparison mode …“, `Header.test.tsx` „shows the comparison mode …“, E2E `layout.spec.ts` („Mode: Comparison“, Auswahlwert `compare`) |
| Tab-Leiste wie im normalen Modus | Keine zweite Komponente mehr: `TopTabs` zeigt immer die gruppierte Leiste (`DataTabs`, früher `RegularTopTabs`) mit lokalisierten Namen, Gruppen, Überlauf-/Konfigurationsmenü. Der Kern liefert Change Detection während eines Vergleichs im Katalog; die Navigation zeigt es als eigenen Eintrag (Gruppe `changes`, K2-77). Die Tab-Leiste steuert den Vergleich (`model.ts`, Abo auf `useTabs`); Sprünge aus der Übersicht wählen den Tab dort. Nach dem Schließen fällt ein verschwundener Tab auf Internal zurück (`state/tabs.ts`). „Übersicht“ (Dashboard) zeigt im Vergleich den Hinweis „Im Vergleichsmodus nicht verfügbar“ (UI-150). Die Vergleichstabelle ist das `tabpanel` der Leiste (`id="data-panel"`). | `toptabs.test.tsx` „keeps the ordinary tab bar during a comparison …“, E2E `layout.spec.ts`, `bridge.spec.ts` (Change Detection über den Navigationseintrag), axe-Lauf `a11y/views.spec.ts:72` |
| Lokalisierte Auswahlwerte (OUT-149) | Linkänderung „Alle/Hinzugefügt/Entfernt/Ankertext geändert/Unverändert“, Vergleichsstatus „Hinzugefügt/Neu/Entfernt/Fehlend“ (Change Detection „Geändert/Nicht geändert“), Quelle „Automatisch“, Filter über `filterLabel`, Change-Detection-Elemente über Tab-/Spaltennamen. Optionswerte bleiben kanonisch (IPC/Export). | `comparison.test.tsx` „shows localized select values …“ |
| Übersicht rechts | Zeilentitel über `tabLabel`/`filterLabel`, Zahlen über `useFormat().integer` (de: 4.438), Spaltenköpfe lokalisiert, Zahlen rechtsbündig. Engere Spalten; passen sie nicht, scrollt die Tabelle seitlich mit fester Titelspalte und dem sichtbaren Hinweis „Weitere Spalten rechts – horizontal scrollen“. | `comparison.test.tsx` „shows the overview with localized titles and numbers …“, E2E: `Missing` liegt in der Seitenleiste, sonst ist der Hinweis sichtbar |
| Kein Modaldialog „Cause: crawl-open-failed“ | `crawl-open-failed` ist kein Modal mehr; die Statusleiste meldet einmal, Details unter „Technische Details“ | `Notifications.test.tsx` „does not open a dialog …“, E2E: Hauptfenster nach fehlgeschlagenem Öffnen zeigt genau „The file was not found: /os-b4-cmpui-missing/gone.ospider“ und keinen Dialog |

### Folgepunkt – CSV-Export über den gemeinsamen Exportdialog (nicht umgesetzt, > 1 h)
Der Exportdialog startet Kernoperationen, die selbst in die gewählte Datei schreiben (`export.view` mit `viewId`).
Für den Vergleich gibt es keine solche Operation, die die Vergleichsabfrage der Oberfläche (Quelle, Vergleichsstatus,
Suche, Sortierung, Spalten) kennt. `os_export::compare::export_tab` (CLI) kennt nur Tab/Filter/Unterfilter. Nötig:
1. In `os-app` eine additive Abzweigung in `export.view` für `viewId` `comparison:*` mit dem Feld `comparison` wie bei
   `view.rows`, als Operation mit Fortschritt und Abbruch, die die Zeilen wie `compare/ui.rs::rows` fensterweise
   liest und über `os_export`-Writer (CSV/XLSX) schreibt.
2. Im Frontend `ExportRequest` „view“ mit diesem Zusatz über `openExport` öffnen (Formatwahl, Dateiname); der
   Vergleichsexport im Browser entfällt.
3. Tests: os-app-Test (Spalten, Unterfilter, Sortierung, XLSX) und E2E über den Dialog.

Schätzung 2–3 h. Ein Workspace-Test ist nicht nötig, die Änderung bleibt in `os-app`.

### Offen bzw. bewusst so gelassen
- „Current“/„Previous“ bleiben in Auswahl und Übersicht unübersetzt. Es sind die Rollennamen der beiden Crawls, wie im
  Vergleichsdialog, in „Current / Previous tauschen“ und in den Spec-Texten. Wenn deutsche Namen gewünscht sind, bitte
  festlegen.
- Die Zellwerte der Spalte `Comparison Status` (`Added`, `Unchanged` …) sind kanonisch wie im Export; eine Übersetzung
  in der Tabelle wäre eine eigene Festlegung.

### Prüfungen Runde 2 (Build-Server)
| Befehl | Ergebnis |
|---|---|
| `npm run -w apps/desktop typecheck` | ok |
| `npm run -w apps/desktop test` | 123 Dateien, 1059 Tests passed |
| `npx playwright test e2e/compare e2e/a11y/views.spec.ts e2e/crawls e2e/header e2e/statusbar e2e/table e2e/shell e2e/beta/n04-live.spec.ts --workers=1 --project=chromium` | 40 passed. Erster Lauf: axe-Befund `aria-controls="data-panel"` (behoben: Vergleichstabelle ist das tabpanel), `table/bridge.spec.ts` einmal Zeitüberschreitung (einzeln wiederholt: passed), `n04-live` ohne gebautes `os-testsite` (nach `cargo build -p os-testsite`: passed) |
| erneut `e2e/compare`, `e2e/crawls/compare.pending.spec.ts` (chromium) | 3 passed |
| `npx playwright test e2e/compare e2e/a11y/views.spec.ts:72 e2e/crawls/compare.pending.spec.ts --workers=1 --project=webkit` | 4 passed |
| Visual-Suite `--config e2e/visual/playwright.config.ts --project=chromium-1` | 31 passed, 8 `screenshots.spec.ts` rot. Dieselben Fälle sind auch mit dem unveränderten Frontend (`a675fefa`) rot (Gegenprobe `en/light/normal`). Die Fixture enthält keinen Vergleich und keinen Change-Detection-Tab, die Navigation ist dort unverändert. Baselines nicht geändert. |
| Rust | in Runde 2 unverändert (fmt/clippy/`cargo test -p os-app` aus Runde 1 gelten) |

### Fremde Dateien Runde 2 (K2-92)
| Datei | Bereich | Art der Änderung | Spec-ID |
|---|---|---|---|
| `docs/spec/06-output-ops.md` | Spec | OS-Festlegung (Orchestrator) in OUT-142/OUT-151 | OUT-142, OUT-151 |
| `apps/desktop/src/i18n/canon/i18n/de.ftl`, `en.ftl` | Kanon-Titel | `column-comparison-status`, `column-previous-address` | OUT-142, UI-121 |
| `apps/desktop/src/features/header/Header.tsx`, `Header.test.tsx` | Kopf | Modusanzeige „Vergleich“, Modus wechseln schließt den Vergleich | UI-013 |
| `apps/desktop/src/features/statusbar/Statusbar.tsx`, `Statusbar.test.tsx` | Statusleiste | Modus „Vergleich“ | UI-013 |
| `apps/desktop/src/features/notifications/Notifications.tsx`, `Notifications.test.tsx` | Meldungen | `crawl-open-failed` nicht modal | CRAWL-127, UI-124 |
| `apps/desktop/src/features/toptabs/TopTabs.tsx`, `GroupNav.tsx`, `toptabs.test.tsx` | Tab-Leiste | Leiste auch im Vergleich, Nachladen bei Vergleichswechsel, Eintrag Change Detection | UI-004, K2-77 |
| `apps/desktop/src/state/tabs.ts` | Tab-Zustand | Gruppe `changes`; aktiver Tab außerhalb des Katalogs → Internal | UI-004, K2-77 |
| `apps/desktop/src/features/table/UpperTable.tsx` | Tabellenbereich | Hinweis statt Dashboard im Vergleich | UI-150 |
| `apps/desktop/e2e/compare/bridge.spec.ts`, `harness.tsx` | E2E | Change Detection über den Navigationseintrag; Harness mit `DataTabs` | K2-77 |

Nicht angefasst: `apps/desktop/src/features/crawls/**`, `apps/desktop/src/ui/**` (nur `tabId` aus `ui/primitives/TabBar` importiert).

### Commits Runde 2
| Commit | Inhalt |
|---|---|
| `8e66daec` | docs: OS-Festlegung OUT-142/OUT-151 |
| `32792a67` | ui: Modus „Vergleich“ in Kopf und Statusleiste |
| `898bec0b` | ui: kein Modaldialog für fehlgeschlagenes Öffnen |
| `56a82dc5` | ui: normale gruppierte Tab-Leiste im Vergleich |
| `9ba7c52c` | ui: normale Spaltenkonfiguration, lokalisierte Auswahlwerte und Übersicht |
| (dieser) | docs: Bericht Runde 2 |

---

## Runde 3 (Nachzug, Entscheidungen des Orchestrators vom 09.11.2026)

### 1. „Current“/„Previous“
Bleiben unübersetzt (Rollennamen, Spec). Keine Änderung.

### 2. Werte der Spalte Vergleichsstatus lokalisiert
- **Umsetzung:** `ComparisonSource.getCell` übersetzt nur die Anzeige der Spalte `Comparison Status`, über dieselben
  Texte wie Unterfilter und Übersicht (`compare-class-*`). Die Kernanfrage für Filter und Sortierung bleibt
  unverändert. Kopieren (`api.copy`, liest Rohwerte) und Export (`comparisonCsv`) behalten die englischen Werte.
- **Begriffe (de):** „Hinzugefügt“, „Neu“, „Entfernt“, „Fehlend“, „Unverändert“. Den Vorschlag „Hinzugekommen/Fehlt“
  habe ich nicht übernommen, damit Zellen, die Auswahl „Vergleichsstatus“, die Spaltenköpfe der Übersicht und die
  Linkänderung (OUT-149: „Hinzugefügt“, „Entfernt“, „Unverändert“) dieselben Wörter benutzen. Falls die Formulierung
  aus der Nachricht gewünscht ist: Es sind nur die Schlüssel `compare-class-*` in
  `features/compare/i18n/de.ftl` zu ändern, an einer Stelle für alle Orte.
- **Spec:** OS-Festlegung in OUT-142 eingetragen.
- **Tests:** `comparison-grid.test.tsx` „localizes every comparison status value in the display only“ (alle fünf Werte,
  `// spec: OUT-142`, `UI-121`). Dazu im Test mit echtem Raster: Zelle „Unverändert“ in de, CSV mit `Unchanged`.

### 3. Visuelle Tests (`screenshots.spec.ts`, Projekt chromium-1, Build-Server)
Alle 8 Fälle sind auch mit dem unveränderten Frontend (`git checkout a675fefa -- apps/desktop/src`) rot. Meine Änderungen
machen keinen Fall rot.

**Vorgehen:**
1. Beide Stände laufen gelassen, normal und mit nur auf dem Server vorübergehend weichen Screenshot-Prüfungen
   (`expect.soft`, danach per `git checkout` zurückgesetzt, nicht committet). So wird jede der sieben Szenen eines
   Falls verglichen.
2. Danach die erzeugten Bilder byteweise verglichen.

**Ergebnis:**
- 71 von 72 Ausgabebildern sind zwischen `a675fefa` und `m3/b4-cmpui` byte-identisch.
- Die einzige Abweichung (`viz-en-dark-normal-actual.png`, 12 Pixel, x 28–29, y 220–296) war ein einmaliges Flackern:
  Zwei Wiederholungen auf `m3/b4-cmpui` sind pixelgleich mit `a675fefa` (0 Pixel).
- In jedem Fall weichen dieselben drei Szenen von der Baseline ab, auf beiden Ständen gleich:

| Szene (je Fall de/en × hell/dunkel × kompakt/normal) | Abweichung von der Baseline | Ursache |
|---|---|---|
| `shell-*` | 41 Pixel, Bereich 888,625–894,632 (Statusleiste rechts) | Zahl in „Unavailable menu actions (…)“ hat sich seit der Baseline geändert (Menüeinträge anderer Pakete); nicht Tab-Leiste/Kopf/Statusleiste dieses Pakets |
| `settings-*` | 4 453–6 104 Pixel, Bereich 220,256–749,328 | neue Zeile „After a crash, ask whether to send a report“ mit Hinweistext in den Einstellungen (Absturzbericht-Paket) |
| `viz-*` | ≈ 372 000 Pixel, Bereich 296,16–1007,539 | andere Anordnung der Wortwolke als in der Baseline |

Die übrigen vier Szenen (`empty`, `primitives`, `serp`, `source`) stimmen mit der Baseline überein. Kein
„Baseline-Update nötig“ aus diesem Paket; die Baselines `baselines/linux` sind für die drei Szenen veraltet (Sache der
Pakete, die Einstellungen, Menü und Wortwolke geändert haben). Baselines nicht geändert.

### 4. Statusleistenmeldung in deutscher Oberfläche
Erscheint deutsch: „Die Datei wurde nicht gefunden: <Pfad>“ (`err-file-not-found` in
`src/i18n/errors/i18n/de.ftl`). Nachweis: `Statusbar.test.tsx` „keeps the technical detail out of the failure sentence
and names a missing file“ (de, exakter Text `^Die Datei wurde nicht gefunden: /tmp/gone/x\.ospider$`). Unbekannte
Schlüssel fallen auf den deutschen Satz der Fehlerklasse zurück („Ein Dateivorgang ist fehlgeschlagen.“), nie auf
englischen Text oder Rohcodes.

### Prüfungen Runde 3
| Befehl | Ergebnis |
|---|---|
| `npm run -w apps/desktop typecheck` | ok |
| `npm run -w apps/desktop test` | 123 Dateien, 1060 Tests passed |
| `npx playwright test e2e/compare e2e/a11y/views.spec.ts:72 --workers=1 --project=chromium` | 3 passed |
| `npx playwright test e2e/compare --workers=1 --project=webkit` | 2 passed |
| Visual chromium-1 (`screenshots.spec.ts`), Stand `a675fefa` und `m3/b4-cmpui` | je 8 rot, Bilder identisch (siehe oben) |
| Rust | unverändert seit Runde 1 |

### Commits Runde 3
| Commit | Inhalt |
|---|---|
| `2e0651a4` | docs: OS-Festlegung OUT-142 (lokalisierte Statuswerte) |
| `bec5750d` | ui: Werte der Spalte Vergleichsstatus lokalisiert, Kopieren/Export kanonisch |
| (dieser) | docs: Bericht Runde 3 |
