# B4-CMPUI – unabhängige Abnahmetests (Testagent)

Gegenstand: Branch `m3/b4-cmpui` (820f8aab), Vergleichsmodus der Desktop-App. Grundlage ist allein die Spec (OUT-140 bis OUT-151, führend OUT-142, OS-Festlegungen vom 09.11.2026; CRAWL-127, CRAWL-144; UI-001, UI-004, UI-007, UI-013, UI-121, UI-140). Der Bauer-Bericht wurde nicht geöffnet. Der Produktcode wurde nur zum Aufrufen gelesen (Selektoren, Aktions-IDs, Modul-Hüllen für den nativen Dateidialog).

Ablage (nur neue Dateien, nichts Bestehendes geändert):
- `apps/desktop/e2e/acceptance/b4-cmpui.spec.ts` (Playwright, echter Kern über `os-app-bridge`)
- `apps/desktop/src/features/compare/acceptance-cmpui.test.tsx` (Vitest, Komponentenebene mit der vorhandenen Fixture `out-compare`)

## Aufbau der Playwright-Spec

- Fixture-Site im Test (Node-HTTP-Server): zwei Versionen. v1: 1.214 interne HTML-Seiten (Startseite, 13 Hubs, 1.200 Seiten, `p5` liefert 404). v2: 1.274 Seiten, 40 Seiten nicht mehr verlinkt (Fehlend), 100 neue (Neu), `p5` jetzt 200. Wortanzahl der Seiten 5, 120 und 1.234, damit Zahlen < 1.000 und > 1.000 vorkommen.
- Zwei echte Crawls über den Kern, Vergleich danach nur über die Oberfläche (Schaltfläche „Crawls vergleichen“, Dialog, „Vergleichen“). Die Bridge läuft mit festem Home je Lauf, damit ein Worker-Neustart nach einem roten Test die Crawls nicht neu aufbauen muss.
- Eingaben: Klicks, Tastatur (Home/End, Strg+O), Mausrad (vertikal und horizontal), Auswahl in Auswahlfeldern, Download-Klick. Gelesen wird über Bedienelemente, die ARIA-Spiegelung der Tabelle, Bounding-Boxen und die Zeichenaufrufe der Canvas (`fillText`: Text, x, Ausrichtung; Pixel-Hash für den angehefteten Bereich).
- Zwei Stellvertreter, weil der Browser keine native Menüleiste und keinen nativen Dateidialog hat: (a) `nativeCrawlDesktop.chooseOpen` liefert den Pfad (wie `chooseExportPath` in der Alpha-Fixture), geöffnet wird mit dem Tastenkürzel; (b) der Klick auf einen „Zuletzt geöffnet“-Eintrag wird als dieselbe Aktion (`runAction`) ausgelöst, die die native Menüleiste auslöst. Die Einträge werden aus dem Menümodell (`window.__osMenu`) gelesen.

## Testliste und Ergebnis

Ergebnis „820f8aab“: Stand des Pakets. „Mutation“: Produktstand `a675fefa` (vor dem Paket), gleiche Tests, Kern neu gebaut.

| # | Test | Spec-ID | Chromium | WebKit | Mutation (a675fefa) |
|---|------|---------|----------|--------|--------------------|
| P1 | Tabelle füllt den Bereich bei 1280 px (Breite, linker Rand, Höhe, Canvas, Detailbereich, keine Seitenknöpfe) | OUT-142, UI-007, UI-001 | grün | grün | rot (Tabelle 646 px in 954 px Bereich) |
| P2 | dasselbe bei 2000 px | OUT-142, UI-007, UI-001 | grün | grün | rot (646 px in 1494 px) |
| P3 | Steuerleiste oben links; Tausch/Schließen im oberen Bereich | OUT-142, UI-001 | grün | grün | rot (erste Steuerung 154 px vom linken Rand) |
| P4 | Virtualisiertes Scrollen: Mausrad, Ende/Anfang, letzte Zeile, DOM < 300, Zähler „1.274 Zeilen“, keine Seitenknöpfe | UI-140, UI-007, OUT-142 | grün | grün | rot (Seitenmodell: Zähler „100“) |
| P5 | Spalten: Address, dann Vergleichsstatus, dann Tab-Spalten; Zahlen rechtsbündig (gleiche rechte Kante), 1.234 mit Tausenderpunkt, Text linksbündig | OUT-142, OUT-151, UI-007 | grün | grün | rot (Vergleichsstatus nur englisch, Spalten fehlen) |
| P6 | Horizontales Scrollen: angeheftete Spalte unverändert (Pixel-Hash) | UI-007, OUT-142 | grün | grün | grün (nicht unterscheidend, s. u.) |
| P7 | Deutsche Oberfläche: Tab-Leiste, Auswahlwerte, Statuswerte, Übersichtstitel/-köpfe, Zahlformat 1.274 | UI-004, UI-121, OUT-142 | grün | grün | rot (Tab-Leiste englisch) |
| P8 | Entfernt/Fehlend in der Übersicht erreichbar; Klick listet genau diese Zeilen | OUT-142, OUT-143 | grün | grün | rot |
| P9 | Modusanzeige „Vergleich“ in Statusleiste und Kopf; Tausch (Added↔Removed, New↔Missing, Current↔Previous); Schließen bringt „Spider“ zurück | UI-013, OUT-140, OUT-141 | grün | grün | rot (Modus „Spider“) |
| P10 | CSV-Export der Ansicht (Fehlend, Neu): englische Spaltennamen und Statuswerte, 40/100 Zeilen, Previous-Adressen | OUT-151, OUT-142 | grün | grün | rot |
| P11 | Englische Oberfläche: kanonische Titel, „Mode: Comparison“, 1,274 | OUT-142, UI-121 | grün | grün | rot |
| P12 | Fehlende Crawl-Datei: deutscher Satz mit Dateiname, kein „.:“, kein „fehlgeschlagen“, kein roher Code, keine rohen Dialoge, Eintrag im angeklickten Untermenü entfernt | CRAWL-127, UI-013 | grün | grün | rot (Statusleiste „Ein Dateivorgang ist fehlgeschlagen.: No such file …“) |
| P13 | Fehlende Crawl-Datei: der Eintrag verschwindet aus „Zuletzt geöffnet“ vollständig (alle Untermenüs) | CRAWL-127 | **rot** | **rot** | rot |
| V1 | Übersicht deutsch: Titel, Köpfe Hinzugefügt/Neu/Entfernt/Fehlend, 4.438 | OUT-142, UI-121, UI-007 | Vitest grün | – | rot |
| V2 | Übersicht englisch: 4,438 | OUT-142, UI-007 | grün | – | rot |
| V3 | Auswahlwerte der Kategorien deutsch, Werte kanonisch | OUT-142, UI-121 | grün | – | rot |
| V4 | Spaltenreihenfolge Deutsch inkl. „Vorherige Adresse“ (Mapping) | OUT-142, OUT-151 | grün | – | rot |
| V5 | Spaltentitel englisch kanonisch | OUT-142 | grün | – | rot |
| V6 | Statuswerte in Zellen deutsch, `comparisonCsv` englisch kanonisch | OUT-142, OUT-151 | grün | – | rot |
| V7 | Kopf und Statusleiste „Vergleich“ | UI-013 | grün | – | rot |
| V8 | Nach Schließen wieder gewählter Modus | UI-013 | grün | – | rot |
| V9 | Öffnungsfehler NotFound mit Dateiname, deutsch, ohne „.:“ | CRAWL-127 | grün | – | rot |
| V10 | Öffnungsfehler ohne Meldungsschlüssel bleibt deutsch ohne „.:“ | CRAWL-127 | grün | – | rot |

Mutationsprobe gesamt: Playwright 12 von 13 rot (Chromium und WebKit je), Vitest 10 von 10 rot. Der Test P13 ist dort rot, weil der Mangel (zwei Listen) auch im Paketstand besteht, P6 ist dort grün.

Stand 820f8aab, Zusammenfassung: Vitest 10/10 grün; Playwright Chromium 12 grün, 1 rot (P13); WebKit 12 grün, 1 rot (P13). Der Test P6 war im ersten WebKit-Lauf wegen einer zu engen Zählschwelle im Test selbst rot (46 statt > 50 Zeichenaufrufe, Testfehler, Schwelle auf 20 gesenkt, danach einzeln in WebKit grün).

Befehle: `npm run -w apps/desktop typecheck` (sauber); `npx vitest run src/features/compare/acceptance-cmpui.test.tsx`; `OS_E2E_PROJECTS=chromium|webkit npx playwright test e2e/acceptance/b4-cmpui.spec.ts --workers=1` mit gebauter Bridge (`cargo build -p os-app --features e2e-bridge --bin os-app-bridge`). Auf dem Mac wurde nichts ausgeführt; `target/`, Home-Verzeichnisse und Testergebnisse auf dem Server sind gelöscht.

## Produktfehler

**PF-1 (P13): Zwei parallele „Zuletzt geöffnet“-Listen, der Eintrag bleibt in der zweiten stehen.** Reproduktion (Test P13): Crawl über Strg+O öffnen, Ansicht leeren, Crawl-Datei löschen, den Eintrag aus „Datei › Zuletzt geöffnet“ anklicken. Das Menümodell enthält zwei Untermenüs gleichen Namens: `file.recent` (Quelle: Oberflächenstand `recentCrawls`, Aktion `recent.N`, Code in `features/help/install.ts`) und `crawls.recent` (Quelle: `localStorage`-Schlüssel `openspider.recent-crawls.v1`, Aktion `crawls.recent.N`, Code in `features/crawls/CrawlsHost.tsx`). Der Klick auf `recent.0` entfernt den Eintrag nur dort und zeigt den Hinweisdialog („Die Datei … existiert nicht mehr und wurde aus der Liste entfernt“); der Eintrag in `crawls.recent` bleibt samt Pfad stehen (`localStorage` danach weiter mit dem Pfad). Spec CRAWL-127: „nicht mehr vorhandene Dateien werden beim Anklicken mit Hinweis entfernt“, eine Liste mit zehn Einträgen. Ursache liegt außerhalb des Pakets (Altlast der Basis `a675fefa`); auf `main` ist `CrawlsHost` laut Quelltext bereits auf `loadRecent`/`onRecentChanged` umgestellt (Paket B4-PROJDLG, „ein Untermenü und eine Liste“), dort habe ich den Test nicht ausgeführt. Der Test P12 (Meldung, Dialoge, Entfernen im angeklickten Untermenü) ist grün.

Keine weiteren Produktfehler gefunden. Alle Befunde aus dem Nutzerbefund (kleiner Block mittig, Seitenknöpfe, englische Tab-Leiste/Auswahlwerte/Übersicht, nicht lokalisierte Zahlen, Modus „Spider“, „.:“ in der Statusleiste) sind im Paketstand behoben und in der Mutationsprobe auf `a675fefa` reproduziert.

## Grenzen der Abdeckung

- P6 unterscheidet die Stände nicht: Die angeheftete Spalte wird auch auf `a675fefa` nicht überzeichnet. Der Befund „Spalten verschoben“ zeigt sich in P1/P2/P5 (Tabellenbreite, Spaltenreihenfolge), nicht im Scrollverhalten.
- URL-Mapping mit „Vorherige Adresse“ ist nur auf Komponentenebene (V4, Fixture) geprüft; die echte Kern-Crawl-Paarung der Playwright-Spec nutzt kein Mapping.
- Der Tausch (P9) ist nur für `Intern: HTML` gegengeprüft; die Invariante `Current − Previous = Added + New − Removed − Missing` wird dort über die beiden Seiten des Tauschs und die absoluten Zahlen (1.274, 1.214, 100, 40) geprüft, nicht für alle Filter.
- Die Rechtsbündigkeit wird über die Zeichenaufrufe der Canvas belegt (`textAlign`, gleiche rechte Kante bei verschieden langen Zahlen), nicht über Pixel.
- Kein Test für Vergleich von Crawls unterschiedlicher Modi, Schema-Migration, gespeicherte Vergleiche (OUT-150), Issues-Panel (OUT-143 außer der Navigation), Site Structure (OUT-148); diese gehören nicht zum Auftrag.

## Spec-Unklarheiten

1. **„Steuerleiste oben links“**: Die Spec beschreibt keine Lage der Vergleichssteuerung. Ich habe geprüft, dass die erste Steuerung der Tabellenzeile (Filter, Vergleichsstatus, Quelle, Suche, Export) ≤ 24 px vom linken/oberen Rand des Tabellenbereichs liegt und Tausch/Schließen oberhalb des Tabellenbereichs liegen. Tausch/Schließen stehen im Paketstand rechts neben der Tab-Leiste, nicht links. Falls „oben links“ diese Schaltflächen meint, braucht es eine Spec-Aussage.
2. **Spaltenposition im Export**: OUT-142 (OS-Festlegung 09.11.2026) setzt `Comparison Status` und `Previous Address` direkt nach `Address` für die Haupttabelle; OUT-151 sagt „zusätzlich zu den Spalten des Current-Crawls“. Im CSV-Export der Ansicht steht `Comparison Status` nicht an zweiter Stelle (der Test prüft nur Namen und Werte, nicht die Position). Gewollt oder Lücke?
3. **Beschriftung „Current“/„Previous“** in der deutschen Oberfläche (Auswahlfelder im Vergleichsdialog, Quelle „Current/Previous“): Die Spec nennt die Rollen so (OUT-140), regelt aber nicht, ob sie in der deutschen Oberfläche übersetzt werden. Nicht getestet.
4. **Bytes/„Größe“**: UI-007 verlangt „Rohwert“ und Zahlen nach UI-Sprache formatiert; für Größe ist unklar, ob „2.468“ oder „2468“ erwartet wird. Der Test bewertet nur Wortanzahl und Eingehende Links.
5. **Hinweisdialog bei fehlender Datei**: CRAWL-127 verlangt einen „Hinweis“; die App zeigt zusätzlich die Meldung in der Statusleiste. Ob beides gewollt ist, regelt die Spec nicht; der Test lässt beides zu, verlangt aber deutschen Text ohne technische Codes.
6. **Hinweis „Weitere Spalten rechts – horizontal scrollen“** in der Übersicht ist in der Spec nicht vorgesehen; der Test prüft nur, dass Entfernt und Fehlend per Mausrad erreichbar sind und im sichtbaren Bereich der Seitenleiste liegen.
