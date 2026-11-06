# B4-LIVEDETAIL – unabhängige Abnahmetests

Testagent: Claude Sonnet 5.5, nur aus `docs/spec/05-ui.md` (UI-018, UI-004/006/008/009/012, UI-030 bis UI-033, UI-050, UI-060/061/062,
UI-120/121, UI-126, UI-140 bis UI-145, UI-148) und `docs/spec/07-performance.md`. Code nur gelesen, um Aufrufe, Selektoren und
Hilfen zu finden (Bericht und Commit-Texte des Bauers nicht gelesen). Branch `m3/livedetail-tests` (Stand des Bau-Branches plus Port-Fix).
Nur Testdateien geändert: `apps/desktop/e2e/acceptance/livedetail-*.spec.ts` und `livedetail-support.ts`.

## Ergebnis auf einen Blick

- 28 Tests je Engine (Playwright: Chromium und WebKit), zusammen 56. **48 grün, 8 rot** – die 8 sind 4 Befunde, je in beiden Engines.
- Das Paket selbst zeigt **keine veraltete Anzeige**: Tabellenfuß, Chip, Overview, Dashboard, Issues, Inlinks/Outlinks, Sprache,
  Theme, Spaltenwahl und Dialoge folgen dem laufenden Crawl und den Einstellungen (alle zugehörigen Tests grün).
- Die 4 roten Tests sind Fehler, die **schon auf `main` bestehen** (Gegenprobe: gleicher Testsatz gegen `main`, Frontend von `main`,
  dieselben Bridge-Binärdateien: Chromium vollständig – dieselben 4 Tests rot, alle übrigen 24 grün; WebKit für die 4 roten Tests – ebenfalls rot). Sie sind nicht durch das Paket entstanden, bleiben aber
  spec-treu rot.
- Zeitbudget UI-145: Median erster Klick im Live-Crawl gegen den Dev-Server gemessen, kein Rückschritt gegenüber `main` (siehe unten);
  die Schwelle für Dev-Server-E2E ist in der Spec nicht beziffert (Spec-Frage S1).

## Umgebung und Durchführung

- Build-Server (Linux), Debug-Build der echten Bridge `os-app-bridge`, Fixture-Server `os-testsite` (`ui/fx-ui-basic` mit 50 ms
  Verzögerung je Seite; `synthetic:n=8000,links=30,seed=7` für die Zeitmessung), Vite-Dev-Server, Playwright `--workers=1`.
- Crawl gedrosselt (1 Thread, 1 bis 3 URL/s), damit der Lauf lange genug sichtbar bleibt. Alle Prüfungen laufen gegen die
  Oberfläche (Zugänglichkeitsschicht der Tabellen, Statusleiste, Tastatur, Maus); als unabhängige „Wahrheit“ dienen die Zahlen
  der Fixture (`expected.json`: 27 URLs, 25 intern, 2 extern; Startseite hat 20 Inlinks von page-1 … page-20 und 25 Outlinks) und
  der Kern selbst (`view.open`/`view.rows`, derselbe IPC-Befehl, den die Oberfläche benutzt) für „exakt nach Pause“.
- Wiederholungen: alle Tests ohne Befund mindestens 5× je Engine ohne Wackler (Chromium 6×, WebKit 6 bis 7×; Zeitmessung: Chromium 6 Läufe,
  WebKit 2). Der Mehrfachauswahl-Test war während der Entwicklung wackelig, weil er den Befund F3 traf; er hält jetzt den Crawl für die
  Auswahl an (16 von 16 grün). Der Tail-Modus-Test trifft gelegentlich die Beobachtung F5 und wiederholt dann einmal das Scrollen (vermerkt
  als Anmerkung `observation-F5`). Die 4 roten Tests sind in jedem Lauf rot (F3 mit Umschalt+Klick 6 von 6, F1/F2 jeweils immer).
- Aufruf: `srv-sync.sh` + `srv-run.sh`, dort `npx playwright test e2e/acceptance --workers=1` (Port je Worktree über `OS_E2E_PORT`).

## Kriterium → Test → Ergebnis

Spalten C = Chromium, W = WebKit; ✓ grün, ✘ rot (Befund).

| Kriterium (Spec) | Test (Datei: Titel) | C | W |
|---|---|---|---|
| UI-018: Tabellenfuß/Chip des aktiven Filters wachsen, ≤ 1 s (Messspielraum 2 s); Overview wächst mehrfach und höchstens 6 s hinter dem Crawl (OS-Festlegung); nach Pause alle Werte exakt, auch Statusleiste; am Ende 27/25/2, Filterliste 25/1/1, kein „Stand vor …“ mehr | live: *table footer, filter chip and Overview grow while the crawl runs and are exact after pause and at the end* (UI-018, UI-006, UI-060) | ✓ | ✓ |
| UI-018: Pause nach der Hälfte, Fortsetzen, Ende = ununterbrochener Crawl (Overview und Issues, zwei App-Instanzen) | live: *after pause, resume and completion Overview and Issues equal an uninterrupted crawl* (UI-018, UI-061, UI-062) | ✓ | ✓ |
| UI-018/UI-061: Dashboard „Übersicht“ folgt dem Crawl ohne Nutzeraktion, exakt nach Pause und Ende, Verteilung 25/1/1, Top-Probleme | live: *the Overview dashboard follows the running crawl …* (UI-018, UI-060, UI-061, UI-004) | ✓ | ✓ |
| UI-018/UI-062: Issues der Seitenleiste füllen sich ohne Nutzeraktion, Endstand genau (Client Error 4XX = 1) | live: *the sidebar Issues list fills during the crawl …* (UI-018, UI-062) | ✓ | ✓ |
| UI-018: Tail-Modus (unsortiert, ganz unten: folgt neuen Zeilen; Hochscrollen beendet ihn) | live: *an unsorted table scrolled to the bottom follows new rows (tail mode) …* (UI-018, UI-140) | ✓ | ✓ |
| UI-032/UI-018/UI-145: Inlinks der gewählten URL wachsen mit dem Crawl, fallen nie zurück, hinken höchstens 3 s hinterher, nach Pause = Kern, am Ende 20 | detail: *Inlinks of the selected URL grow with the running crawl …* (UI-032, UI-018, UI-145) | ✓ | ✓ |
| UI-145/UI-142/UI-032/UI-033/UI-031: Auswahlwechsel zeigt die Zeilen der neuen URL (≤ 3 s), danach nie wieder Zeilen der vorigen; Outlinks und URL Details der neuen Auswahl | detail: *switching the selection shows the new URL's Inlinks and Outlinks and never rows of the previous URL* | ✓ | ✓ |
| UI-145: schnelle Auswahlwechsel (8 in 5 ms-Abständen) werden entprellt, Ende auf der Fokus-URL | detail: *holding an arrow key leads to few detail requests …* (UI-145, UI-012) | ✓ | ✓ |
| UI-145/UI-030/UI-018: verborgener, zurückgehaltener Tab zeigt nach Rückkehr den aktuellen Stand | detail: *a detail tab that was hidden while the crawl ran shows the current data …* | ✓ | ✓ |
| UI-010/UI-032/UI-018: sortierte Inlinks bleiben beim Eintreffen neuer Links sortiert (auf-/absteigend), Gesamtzahl = Kern | detail: *a sorted Inlinks table keeps its order while links arrive …* | ✓ | ✓ |
| UI-033/UI-018: Outlinks der Startseite vollständig (25) und stabil während des Crawls | detail: *Outlinks of the start page stay complete and correct …* | ✓ | ✓ |
| UI-030/UI-012/UI-018: Mehrfachauswahl → Vereinigung der Inlinks; Markierung und Detail bleiben beim Weiter-Crawlen stehen | detail: *a multi-row selection keeps its rows while rows arrive …* (Crawl für die Auswahl angehalten, siehe F3) | ✓ | ✓ |
| UI-032/UI-018/UI-142: später entdeckter Link (/redirect → page-1) erscheint in den Inlinks der schon gewählten URL | detail: *a link discovered later (redirect target) appears …* | ✓ | ✓ |
| UI-012/UI-018: jeder Umschalt+Klick im laufenden Crawl erweitert die Auswahl | detail: *every Shift+click extends the selection while the crawl runs* | **✘ F3** | **✘ F3** |
| UI-121/UI-018/UI-007: Sprachwechsel de/en wirkt sofort auf obere Tabelle, Detailtabelle, Overview, Chips, Tabellenfuß; Daten wachsen weiter; zurück identisch | memo: *a language change relabels the open tables, the detail window and the Overview at once …* | ✓ | ✓ |
| UI-120/UI-018: Theme-Wechsel zeichnet obere Tabelle, Detailtabelle (Canvas-Pixel), Seitenleiste und Body sofort um; neue Zeilen bleiben im neuen Thema; zurück identisch | memo: *a theme change repaints the open tables, the detail window and the sidebar at once …* | ✓ | ✓ |
| UI-121/UI-120/UI-004: Dashboard wechselt Sprache und Theme sofort, Zahlen wachsen weiter | memo: *the Overview dashboard changes language and theme at once …* | ✓ | ✓ |
| UI-008/UI-009/UI-018: Spaltenwahl der oberen Tabelle (Spaltenmenü „+“ und Konfigurator): sofort, Abbrechen verwirft, Anwenden gilt, neue Daten erscheinen, Wiedereinblenden | memo: *column changes in the upper table act on the open table at once …* | ✓ | ✓ |
| UI-030/UI-008/UI-018: Spaltenwahl einer Detailtabelle gilt sofort, mit neuen Zeilen, nach späterem Auswahlwechsel | memo: *column choices of a detail table apply at once …* | ✓ | ✓ |
| UI-030/UI-008: Spaltenwahl einer Detailtabelle geht nicht verloren, wenn die Auswahl gleich danach wechselt | memo: *a column change of a detail table is not lost when the selection changes right after it* | **✘ F1** | **✘ F1** |
| UI-009/UI-126/UI-018: Spaltendialoge per Tastatur öffnen, Fokus im Dialog, Esc = Abbrechen und Fokus zurück, erneut öffnen zeigt aktuellen Stand, Suchfeld leer | dialogs: *the column dialogs open from the keyboard, hold focus, restore it on Escape …* | ✓ | ✓ |
| UI-004/UI-030/UI-018: Tab-Konfiguration (obere Tabs, Detail-Tabs): Abbrechen verwirft, OK gilt, erneut öffnen zeigt aktuellen Stand, bleibt über Auswahlwechsel | dialogs: *the tab configuration dialogs … show the current state and keep applied changes* | ✓ | ✓ |
| UI-006/UI-018/UI-126: Filterliste während des Crawls: Zähler bei jedem Öffnen aktuell, Tastaturbedienung, Fokus, nach Pause = Kern (Liste, Eingabefeld, Chip, Tabellenfuß) | dialogs: *the filter list opens during the crawl, shows current counts each time …* | ✓ | ✓ |
| UI-032/UI-034/UI-030/UI-018: Auswahllisten „View“ (Links/Anchors) und „Filter“ der Inlinks-Tabelle im Live-Crawl | dialogs: *the View and Filter lists of the Inlinks table apply during the crawl …* | ✓ | ✓ |
| UI-050/UI-126: Crawl-Pfad-Dialog (fertiger Crawl) zeigt Startseite → gewählte Seite (Tiefe + 1 Einträge) | dialogs: *the Crawl Path dialog of a finished crawl shows the path …* | **✘ F2** | **✘ F2** |
| UI-050/UI-126/UI-018: Crawl-Pfad-Dialog im laufenden Crawl | dialogs: *the Crawl Path dialog works during a running crawl* | **✘ F2** | **✘ F2** |
| UI-050/UI-126: Crawl-Pfad-Dialog per Tastatur öffnen/schließen, Fokus zurück, je Auswahl neu aufgebaut (`aria-rowcount` = Tiefe + 2) | dialogs: *the Crawl Path dialog opens and closes with the keyboard and is built anew …* | ✓ | ✓ |
| UI-145/UI-143/UI-148: Median erster Klick im Live-Crawl (Dev-Server): jede Messung liefert Daten; Regressionsprüfung nur mit Basis | budget: *first click on a row during a running crawl: median …* | ✓ | ✓ |

## Befunde (alle rot, alle mit Gegenprobe gegen `main`: dort ebenso rot)

**F1 – Spaltenwahl einer Detailtabelle geht verloren, wenn die Auswahl gleich danach wechselt (UI-008, UI-030).**
Spalte „Anchor Text“ in der Inlinks-Tabelle abwählen, dann sofort (Pfeil nach unten) eine andere URL markieren: die neue Tabelle zeigt
die Spalte wieder, und sie bleibt mindestens 3 s sichtbar. Ursache laut Ablauf: Die Spaltenwahl wird mit 1 s Verzögerung gespeichert;
eine neu aufgebaute Detailtabelle lädt den gespeicherten Stand und kennt die noch ausstehende Änderung nicht. Wartet man mehr als etwa
1 s, bleibt die Wahl erhalten (grüner Schwestertest). Test: `memo: a column change of a detail table is not lost …` (Chromium und WebKit rot, `main` rot).

**F2 – Crawl-Pfad-Dialog zeigt keine Zeilen (UI-050).**
Der Dialog „Crawl Path“ öffnet, die Kopfzeile und `aria-rowcount` stimmen (Tiefe + 2), aber die Zeichenfläche der Tabelle hat die Höhe
0 px (`canvas.style.height = 0px`); es sind keine Zeilen sichtbar, der Pfad ist also nicht lesbar. Gilt für fertigen und laufenden
Crawl. Einmal (Live-Lauf) stand zusätzlich die Meldung „A file operation failed.“ im Dialog; nicht wieder aufgetreten, nicht untersucht.
Tests: `dialogs: the Crawl Path dialog of a finished crawl …` und `… works during a running crawl` (beide Engines rot, `main` rot). Der Test
`… opens and closes with the keyboard and is built anew …` (nur `aria-rowcount`, Fokus) ist grün.

**F3 – Umschalt+Klick (auch Umschalt+Pfeil) im laufenden Crawl geht häufig verloren (UI-012, UI-018).**
Bei 2 bis 3 URL/s wurde die Bereichsauswahl in etwa drei von vier Fällen ohne Meldung verworfen (Beispiele: „Shift+Klick auf page-3:
1 statt 2 Zeilen markiert“); in 2 von 14 Läufen gingen acht Versuche nacheinander verloren. Sobald der Crawl steht, wirkt jeder Klick.
Im Code verwirft `DataGrid.selectRow` eine Umschalt-Auswahl, wenn sich die Ansichtsversion während des asynchronen Nachschlagens
ändert (der einfache Klick ist davor geschützt, A01-16). Bei höheren Crawl-Raten wird es schlimmer. Test:
`detail: every Shift+click extends the selection while the crawl runs` (beide Engines rot, `main` rot).

**F4 (kein Fehler, Beobachtung) – Zeitbudget:** siehe nächster Abschnitt.

**F5 (Beobachtung, nicht als rot gewertet) – Hochscrollen im Tail-Modus bleibt gelegentlich wirkungslos (UI-018).**
Wird in einer unsortierten, ganz unten stehenden Tabelle nach oben gescrollt, springt die Ansicht in etwa jedem zehnten Lauf nicht nach
oben, sondern folgt weiter den neuen Zeilen (WebKit 2 von 14 und 1 von 8, Chromium 1 von 16, `main` 1 von 24); ein zweites Mausrad-Ereignis
wirkt. Ob die Ursache die Oberfläche (Neuzeichnen und Nachführen im selben Takt) oder die Zustellung des Ereignisses ist, ließ sich von
außen nicht trennen; deshalb wiederholt der Test das Scrollen einmal und bleibt grün. Test: `live: an unsorted table scrolled to the bottom …`.

Weiter wurde **nichts Rotes** gefunden: keine Zeilen der vorigen URL nach einem Auswahlwechsel, keine zurückfallenden Zähler, kein
Hängenbleiben des Tabellenfußes, der Chips, der Overview oder des Dashboards, keine Konsolenfehler (die Fixture prüft jede Seite),
keine verlorenen Änderungen in den Dialogen (Spaltenmenü, Konfigurator, Tab-Konfiguration, Auswahllisten).

## Zeitbudget UI-145 (Regressionsmessung)

Einordnung nach `07-performance.md` Z. 5: Budgets (URL Details ≤ 100 ms, Inlinks/Outlinks erste Seite ≤ 150 ms) gelten im Release-Build auf
Referenzhardware; E2E gegen den Dev-Server prüft nur Regressionen „mit eigenen Schwellen“. Gemessen wird der Median über je 7 bis 9 frische
Seitenladungen im laufenden Crawl (4 Threads, 100 URL/s, 8000 synthetische Seiten): Zeit vom Zeiger-Druck auf eine Zeile der oberen
Tabelle bis die Detailansicht die Daten genau dieser Auswahl zeigt (einschließlich 100 ms Entprellung). Dev-Server, Debug-Bridge, ausgelasteter
Build-Server – absolute Werte sind nur untereinander vergleichbar.

| Messung (Median, ms) | Bau-Branch (Chromium) | `main` (Chromium) | Bau-Branch (WebKit) |
|---|---|---|---|
| URL Details | 150 / 157 / 152 / 158 (4 Läufe zu 7 bis 9 Messungen) | 176 / 183 / 174 (3 Läufe) | 161 |
| Inlinks, erste Seite | 275 / 253 / 269 / 223 | 249 / 263 / 258 | 213 |
| Auswahl sichtbar („1 selected“) | 26 bis 35 | 31 bis 33 | 27 bis 34 |

Ergebnis: URL Details im Mittel etwa 13 % schneller als `main` (154 gegen 178 ms), Inlinks im Rahmen der Streuung gleich (255 gegen 257 ms). Kein Rückschritt.
Der Test prüft immer, dass jede Messung Daten liefert (Wächter 5 s), und bei gesetzter Basis `OS_LD_BASELINE_URLDETAILS_MS` /
`OS_LD_BASELINE_LINKS_MS` den Median gegen Basis × 1,10 (UI-148). Ohne Basis wird der Median nur berichtet (Anmerkung `spec-question` am Test),
eine eigene Schwelle wurde nicht erfunden. Die Release-Budgets selbst sind auf dieser Umgebung nicht prüfbar.

## Spec-Fragen

- **S1 (07-performance.md Z. 5, UI-145, UI-148):** „E2E-Läufe gegen den Entwicklungs-Server prüfen nur Regressionen mit eigenen Schwellen“ – die
  Schwellen sind nirgends beziffert. UI-148 nennt +10 % gegenüber dem gleitenden Median der letzten 7 Läufe, setzt aber eine Historie und
  eine feste Maschine voraus. Welche Basis gilt für die Dev-Server-E2E (z. B. Median von `main` im selben Lauf) und welche Toleranz auf
  einem geteilten Server?
- **S2 (UI-018, UI-142):** Wie schnell müssen neue Zeilen in einer offenen Detailtabelle (Inlinks/Outlinks) erscheinen? UI-018 beziffert
  Tabellenzähler (≤ 1 s) und die Overview (nächste Zählung nach max(2 s, 3 × Dauer)), nicht die Detailtabellen. Die Tests nehmen 3 s,
  für „Auswahlwechsel zeigt neue Daten“ ebenfalls 3 s (Spec: Folgen ≤ 150 ms, erste Seite ≤ 150 ms im Release-Build).
- **S3 („Veraltet-Hinweis“):** Der Auftrag nennt ihn für Detailtabellen; die Spec kennt nur den Veraltet-Hinweis der Analyse (UI-082) und
  verwirft veraltete Antworten stillschweigend (UI-142). Es gibt keinen Hinweis in Inlinks/Outlinks zu prüfen. Soll es einen geben?
- **S4 (UI-126):** Fokus nach dem Schließen eines Dialogs: Die Spec verlangt Tastaturbedienbarkeit, sagt aber nicht ausdrücklich, dass der
  Fokus zum Auslöser zurückkehrt. Die Tests verlangen es bei Öffnen per Tastatur (üblich, vom Dialog-Baustein auch so beschrieben).
- **S5 (UI-012):** Die Spec nennt Umschalt+Klick und Pfeiltasten, nicht Umschalt+Pfeil. F3 gilt durchgehend für Umschalt+Klick; Umschalt+Pfeil
  zeigte dasselbe Verlustmuster (Beobachtung, nicht als eigener Test).
- **S6 (UI-018/UI-061):** Soll der Hinweis „Stand vor N s“ der Overview nach Pause/Ende verschwinden? Die Spec nennt ihn nur für den laufenden
  Crawl und verlangt danach „exakte“ Werte; der Test verlangt, dass er nach Ende fehlt (grün).
- **S7 (UI-010/UI-030):** Reihenfolge einer unsortierten Detailtabelle (Inlinks) ist nicht festgelegt (UI-010 beschreibt die obere Tabelle);
  die Tests prüfen nur sortierte Zustände und Mengen.

## Grenzen

- Debug-Build und Dev-Server, 27 bis 8000 URLs, 1 bis 100 URL/s; die Skalenkriterien aus UI-140 bis UI-145 (5 Mio. Zeilen, 1 Mio. Inlinks) sind
  nicht Gegenstand dieser Tests. Native Menüs/Fenster der Tauri-App (Stufe B) nicht geprüft.
- Ein-/Ausblenden der Seitenleisten-Tab-Konfiguration wurde nicht gesondert getestet (gleicher Baustein wie obere und Detail-Tabs).
