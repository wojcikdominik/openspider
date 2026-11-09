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
