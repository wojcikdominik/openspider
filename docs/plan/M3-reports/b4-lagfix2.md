# B4-LAGFIX2: Live-Zähler und Tabellenfuß innerhalb von 1 s (UI-018)

Branch `m3/lagfix2` ab `origin/main` `bb0b27e1` (enthält B4-LAGFIX). Soll ausschließlich `docs/spec/*`
(UI-018, UI-143, PERF-004); keine Referenzprodukt-Sichtung, keine Abhängigkeitsänderung, kein Push.
Server `openspider-build`, Worktree `m3-lagfix2`; Abnahme im Wegwerf-Worktree `m3-lagfix2-acc` (siehe Nachweis).

**Ergebnis:** Die 1-s-Grenze für Tabellenfuß und aktiven Chip wird in der unveränderten Abnahme eingehalten:
Endstand `55786c7d`, 5 Läufe je Engine auf je zwei CPUs, **290/290 grün, 0 Verstöße**, Kettenlatenz in allen
10 Läufen Median und p99 0 ms, Maximum 373 ms (vorher Runde 3: 1/10 grün, p99 0,91–0,995 s, Maximum 1,0–1,39 s).
Ursachen: Fenster nach dem Neuladen als verdeckt geführt (1 Hz), Tabellensummen auf eigener Uhr mit zweitem
Taktgeber (MCP), gezählte, aber noch nicht geschriebene URLs, Anlauf. **Offen als Frage:** Leerlauf-Renderlast und
Klick-Median sind nicht gestiegen, das Klick-Maximum in Chromium aber in 6/6 Durchgängen (83–96 → 94–121 ms, alle
< 150 ms), und die Klickphase in WebKit liegt 0,4 Punkte höher (innerhalb der Streuung); ein bestehender Alpha-Test
(`detail-sidebar.spec.ts:135`) ist in 1/30 Läufen rot (main 0/30). Siehe Renderlast und Gates.

## Ursache mit Messwerten

### Messaufbau (vorübergehend, nicht committet)

Zeitstempel (Wanduhr) in einer Spur-Datei je Bridge-Prozess, nur mit `OS_LAG2_TRACE`: Engine `crawled N`
(`ProgressTracker::record_completion`), Writer-Commit, jeder Feed-Poll, Zeilenveröffentlichung, View-Pflege (Auftrag,
Ende mit Summe, Fehler), jeder gesendete Tick (Zähler und View-Summen), Schreibanforderung, `events.setVisibility`
und zeitweise die Dauer jedes Commands. Im Browser die Empfangszeit jedes Ticks. Gemessen mit einer **Kopie** des
Zählertests `livedetail-live.spec.ts:46` aus `adb2371c`, die nur ein zweites Attachment (`timeOrigin`, Tick-Empfang)
anhängt; Last, Grenzen und Auswertung unverändert; für den Anlauf einmal zusätzlich ein Chromium-CPU-Profil.
Debug-Bridge und Vite-Dev-Server wie in der Abnahme, `--workers=2`, Chromium `taskset -c 2,3`, WebKit
`taskset -c 4,5`, je Runde 3–4 Wiederholungen je Engine. Die Instrumentierung ist vollständig entfernt
(`git diff bb0b27e1` enthält keine Spuraufrufe); Patch und Auswerter: `/tmp/lagfix2/instrument.py`,
`/tmp/lagfix2/{analyze,late2,late3,chips,window,startup,prof}.py`, Rohdaten `/tmp/lagfix2/diag{1..7}-*` (Server).

### Befund 1: Das Fenster galt nach dem Neuladen als verdeckt – Ticks mit 1 Hz statt 2 Hz

Der Test lädt die Seite zweimal (`alpha.open()` im Test und in `startGated`). Beim Verlassen des ersten Dokuments
meldet `bindVisibility` „verdeckt“; das neue Dokument meldete sich nur bei einer **Änderung**, also nie. Spur:
`vis frontend win=main visible=false` 2,9 s vor Messbeginn, danach keine Meldung. Verdeckte Fenster bekommen
höchstens 1 Hz (UI-143). Vorher, unveränderter `main` (diag1, 3 Läufe je Engine, alle 6 rot):

| Lauf | Tickabstand Median / Max | URLs mit Fußsumme im selben Tick wie der Zähler | einen Tick (≈ 1 s) später | ≥ 2 Ticks später |
|---|---|---|---|---|
| Chromium 1 / 2 / 3 | 1004/1162 · 1004/2007 · 1001/1100 ms | 105 · 107 · 125 von ≈ 180 | 72 · 71 · 52 | 0 · 2 · 3 |
| WebKit 1 / 2 / 3 | 1008/1177 · 1004/1181 · 1001/1119 ms | 104 · 109 · 126 | 70 · 68 · 51 | 3 · 3 · 3 |

Ein Fuß „einen Tick später“ liegt damit ≈ 1000 ms + Zeichnen hinter dem Zähler. Das erklärt die Runde-3-Werte
(p99 0,91–0,995 s unmittelbar an der Grenze, Maxima 1,0–1,39 s mit 1–3 URLs Differenz). Im echten Fenster ohne
Neuladen bleibt es bei 2 Hz; dort trifft der Fehler nur ein neu geladenes Fenster.

### Befund 2: Tabellensummen liefen auf einer eigenen Uhr und hatten einen zweiten Taktgeber

Kette je URL vorher (Median / Max über 6 Läufe, ab Abschluss in der Engine): Writer-Commit 104–109 / 462 ms;
Commit bis Zeilenveröffentlichung 254–299 / 615 ms (eigene 500-ms-Uhr `rows_at`, unabhängig vom Tick);
View-Pflege 46–159 ms; **Pflegeende bis zum nächsten Tick 449–595 / 1088–1587 ms**.

- Tabellen- und Tick-Uhr hatten keinen gemeinsamen Takt: Eine kurz nach einem Tick veröffentlichte Summe wartete
  auf den nächsten Tick, der Zähler desselben Polls war schon unterwegs.
- **Zweiter Taktgeber:** Die MCP-Schleife (`mcp/mod.rs`) ruft alle 100 ms `flush_tick()` auf und sendet einen fälligen
  Tick an beliebiger Stelle. Mit der Ausrichtung allein (Zwischenstand `fix1`) wurden Zeilen deshalb nur noch ≈ 1/s
  veröffentlicht (59 bei 132 Ticks), der Fuß fiel bis 13 s zurück: 6/6 rot, 111–270 Verstöße. Verworfen.

### Befund 3: Gezählt, aber noch nicht geschrieben

Nach Ausrichtung, MCP-Korrektur und Sichtbarkeit (Zwischenstand `fix2`, 6/6 grün in der Diagnose, Kettenlatenz
max. 535–711 ms) kamen noch 8–20 von ≈ 180 URLs einen Tick später. Spur: **alle** diese Fälle sind „im Fortschritt
gezählt, im Schnappschuss nicht enthalten“ (z. B. URL 54 gezählt bei −156 ms, Poll −112 ms, Commit −46 ms): Worker
senden die Seite vor dem Zählen, der Writer sammelt sie aber bis 100 ms. In der unveränderten Abnahme (Serie 1,
siehe Nachweis) waren genau diese Fälle zusammen mit Browser-Stillständen von 0,4–1,8 s die verbleibenden Verstöße.

### Befund 4: Anlauf

- Der Coalescer startet die Uhr eines Fensters beim ersten Blick und sendet frühestens eine Periode später. Vorher:
  Sitzung aktiv 375 ms nach Messbeginn, erster Tick erst 1351 ms.
- Die obere Tabelle öffnet die View eines neuen Crawls erst nach dem Rendern des ganzen Fensters (CPU-Profil:
  React-Rendern, Menüverfügbarkeit, Overview-Zeilen, Diagramm; in parallelen Läufen 1–2 s Hauptthread-Stillstand):
  Chip ohne Zahl bis 1,8 s nach „Running“ (Serie 2).
- Beim Start lädt die Segmentliste neu; die Tabelle öffnete deshalb **ein zweites Mal** (Fuß/Chip wieder leer).
  Fiel das in eine laufende View-Erstellung, blieb im Kern eine zweite View zurück und wurde den ganzen Lauf gepflegt
  (Spuren diag3–diag5: zwei gepflegte Views mit je 76–98 Pflegeläufen; einmal auch schon auf `main`).
- Ein Versuch, die View schon mit dem ersten Tick aus dem Sitzungsstore zu öffnen (`8ccd650b`), behob den Chip im
  Anlauf, änderte aber die Scrollposition der leer geöffneten Tabelle nach dem Crawl (`alpha/table.spec.ts:39` in
  WebKit 7/10 rot, `main` 10/10 grün). Zurückgenommen (`55786c7d`).

### Befund 5: Pflegedauer im Debug-Aufbau

Mit einer View: Pflege im Median 120–180 ms, p90 261 ms, p99 669 ms (WebKit, 2 CPUs); 15 von 100 Läufen über
250 ms. Ein fester Wartebudget von 250 ms ließ diese Ticks ohne Summe gehen (diag6: 2–11 solche Ticks je Lauf).
Außerdem prüfte ein Poll mit fälligem Tick zweimal getrennt, ob der Tick fällig ist (Schreiben und Zeilen); fiel die
Grenze dazwischen, gingen Zeilen ohne Schreiben hinaus. Und ein fälliger Poll konnte von der 50-ms-Entprellung
übersprungen werden; der Tick ging dann ohne die Zeilen seines Polls (Spur diag4).

### Befund 6: Endstand nach „Completed“

Mit den Änderungen oben kam die Endmeldung eines von selbst beendeten Crawls (Steuerpfad `settle`, sofortiger Flush)
teils vor den letzten Zeilen: `livedetail-dialogs.spec.ts:311` (main-Fassung) in WebKit 5/30 rot (main 0/10 bzw.
0/30 mit `table.spec.ts:39`), die Tabelle sortierte nach dem Klick um. Mit Endmeldung und Endstand in einem Tick
(`7da47060`, `b7859d48`): 10/10 bzw. 6/6 grün.

## Abwägung der Ansätze

| Ansatz | Bewertung |
|---|---|
| Zahlen direkt aus dem Fortschrittsereignis | Verworfen. Der Fortschritt kennt nur gecrawlt/eingereiht/gefunden; Fuß und Chip zeigen die Zeilenzahl der **gefilterten** View. Eine eigene Zählung je Filter neben der View wäre eine zweite Wahrheit. |
| Tick für Zahlen enger takten | Verworfen. Mehr Ereignisse und Renderarbeit je Sekunde (UI-018 Kriterium 1: ≤ 2 je Sekunde und Ansicht); behebt den fehlenden Abgleich nicht. |
| Writer-Takt für Zählungen entkoppeln | Übernommen in schmaler Form (Befund 3): Nur der Poll eines fälligen Ticks lässt den Writer die gezählten URLs sofort schreiben, und nur wenn im Kanal nichts wartet. Kein kürzerer Writer-Takt (PERF-002/003). |
| Ersten Tick sofort nach Start | Übernommen (Befund 4), mit dem bestehenden Mindestabstand von 100 ms (PERF-004). Die frühe Öffnung der Tabelle selbst ist zurückgenommen (Befund 4). |
| Zusätzlich: Tabellenuhr an den Tick binden, MCP-Taktgeber still | Übernommen (Befund 2): kein zusätzliches Ereignis, keine zusätzliche Pflege. |
| Zusätzlich: Sichtbarkeit beim Laden melden | Übernommen (Befund 1). |
| Zusätzlich: eigene Lesespur für die View-Pflege | Ausprobiert (`af329580`), zurückgenommen (`408e5d38`): Sie serialisierte mehrere Views, die Pflege überschritt dann öfter das Warten (Serie 2: ein Lauf mit 77 Verstößen). Die Spur zeigte vorher keine Pool-Wartezeit als Ursache verpasster Ticks. |

## Änderung

- `apps/desktop/src/ipc/events.ts`: `bindVisibility` meldet den aktuellen Zustand sofort.
- `crates/os-app/src/session/mod.rs`: Die Sitzungsschleife entscheidet einmal je Durchlauf, ob ein Tick fällig ist,
  pollt mit dieser Entscheidung, wartet auf die gestartete View-Pflege bis höchstens 1 s nach dem vorigen Tick
  (100–500 ms, `settle_budget`) und sendet erst dann. Kein Vorab-Flush vor dem Poll mehr; Steuerbefehle quittieren
  weiter selbst, und solange sie die Sitzungssperre halten, kehrt `poll` sofort zurück. `delivers_ticks` für andere Sender.
- `crates/os-app/src/session/feed.rs`: Zeilen nur in einem Poll mit fälligem Tick (höchstens einmal je Live-Intervall;
  Rückfall nach zwei Intervallen); ein fälliger Poll wird nicht entprellt und lässt vor seinem Schnappschuss die
  gezählten URLs schreiben (`CrawlHandle::flush_counted`, höchstens 100 ms Warten).
- `crates/os-app/src/views/mod.rs`: Zähler laufender Pflege-Worker und `settle(app, max)`.
- `crates/os-app/src/events/coalescer.rs`, `runtime/hooks.rs`, `runtime/mod.rs`: `TickPolicy::any_due` (Vorschau ohne
  Verbrauch; ein neues Fenster startet seine Uhr auch dabei) und `expedite` (ein sofortiger Tick für sichtbare
  Fenster, Mindestabstand 100 ms; verdeckte Fenster bleiben bei 1 Hz).
- `crates/os-app/src/session/control.rs`: nach dem Anhängen eines Crawls (Start, Fortsetzen) `expedite_tick()`;
  `settle` ohne Steueraktion (von selbst beendet) sendet die Endmeldung nicht mehr vorab (Befund 6).
- Ende im Poll (`session/mod.rs`, `feed.rs`): die letzten Zeilen werden sofort veröffentlicht, der Tick wartet bis
  1 s auf ihre Pflege und kommt beschleunigt (≥ 100 ms nach dem vorigen), mit Endmeldung und Endstand zusammen.
- `crates/os-app/src/mcp/mod.rs`: Die MCP-Schleife sendet keine Ticks, solange die Sitzungsschleife sie zustellt.
- `crates/os-engine/src/writer_thread.rs`, `crawl.rs`: `WeakWriterSender::flush_if_idle`, `CrawlHandle::flush_counted`
  (schwache Schreibseite, `finish` endet wie bisher; bei wartenden Nachrichten keine Anforderung).
- `apps/desktop/src/features/table-advanced/stores.ts`, `SegmentControls.tsx`: Die Segmentliste eines anderen Crawls
  lädt ohne Revisionssprung (einziger Verbraucher der Revision ist die obere Tabelle; nach einer Neuzuordnung springt
  sie weiter). Die zweite Öffnung beim Start entfällt.
- `apps/desktop/src/features/table/controller.ts`: unverändert gegenüber `main` (frühe Öffnung zurückgenommen).

Kein neuer Timer, kein neues IPC, keine zusätzliche Ereignisart. Tickabstand im Debug-Aufbau jetzt 690–720 ms im
Median, höchstens ≈ 1000 ms (vorher im verdeckten Zustand 1000 ms, sonst ≈ 500–600 ms); Ticks je Sekunde also nicht
mehr als vorher im sichtbaren Fenster.

### Tests

- `crates/os-app/tests/b4_lagfix2.rs` (`// spec: UI-018`, PERF-004, UI-143, ONPAGE-193):
  Vorschau/Beschleunigung des Coalescers; Zeilen nur in Polls mit fälligem Tick (Fenster 200 ms versetzt, nur die
  laufende Sitzungsschleife); Zähler N und Summe N im selben Tick (Datei-Fixture, Handuhr); laufender Crawl
  (`synthetic:n=400`, 20 URL/s, 2 Threads): jede Tick-Summe enthält alle gezählten URLs; Wartebudget.
  Gegen den alten Stand rot: Zeilen bei 500 ms ohne Tick; kein Tick mit Zähler 2 innerhalb einer Periode; ohne
  Schreibanforderung Summe < Zähler in 8 von 10 Ticks (z. B. 11/3, 23/15, 35/25).
- `crates/os-engine/src/writer_thread.rs` (`// spec: UI-018`): untätiger Writer schreibt auf Anforderung sofort,
  bei wartender Nachricht keine Anforderung, schwache Seite hält den Writer nicht offen.
- `apps/desktop/src/ipc/b4-lagfix2-visibility.test.ts` (UI-018, UI-143): Neuladen nach „verdeckt“ meldet sofort
  „sichtbar“; alter Stand 2/2 rot.
- `apps/desktop/src/features/table/b4-lagfix2-segments.test.tsx` (UI-018): Laden der Segmente des neuen Crawls
  öffnet die Tabelle nicht ein zweites Mal (ohne die Store-Änderung rot: zwei `view.open`).
- Kein bestehender Test geändert; unabhängige Abnahme unverändert.

## Nachweis: unveränderte Abnahme aus `m3/livedetail-tests-ci` (`adb2371c`)

Aufbau: Wegwerf-Worktree `m3-lagfix2-acc` (Server) = `origin/main` + Produktstand dieses Branches (per Sync,
nicht committet) + die Testdateien aus `adb2371c` (`apps/desktop/e2e/acceptance/livedetail-*`, `playwright.config.ts`),
**byteidentisch** (SHA-256-Vergleich gegen `/tmp/lagfix2/acc-orig/`). Der Testbranch enthält `bb0b27e1`; zwischen
`bb0b27e1` und `adb2371c` gibt es keine Änderung in `apps/desktop/src` oder `crates`. Je Lauf alle `livedetail-*`-Specs
außer `budget` (29 Tests, wie Runde 3), `--workers=2 --retries=0`, Chromium `taskset -c 2,3`, WebKit gleichzeitig
`taskset -c 4,5` (je ein SMT-Paar), eigene Ports, `nice -n 10`, Debug-Bridge und Werkzeuge des jeweiligen Stands,
5 Läufe je Engine nacheinander (`/tmp/lagfix2/run-acc.sh`). Während der gewerteten Serie liefen keine eigenen Jobs.
Ein Sampler zeichnete je Sekunde fremde lauffähige Threads auf den vier CPUs auf (`/tmp/lagfix2/sampler.py`).

### Endnachweis (Serie 5, Endstand `55786c7d`, 19:24–20:28 UTC)

Rust-Stand `b7859d48` (seitdem nur Frontend), Debug-Bridge `/tmp/lagfix2/bin-final5`, Frontend `55786c7d` per Vite.

| Lauf | Engine | grün/gesamt | Dauer | Zähler/Chip: Grenze, Verstöße / Stichproben (Zufluss, Schluss) | Kettenlatenz Median / p99 / Max | längster Recorder-Stillstand | Inlinks: Grenze, Verstöße / Stichproben |
|---|---|---|---|---|---|---|---|
| 1 | Chromium | 29/29 | 551 s | 1000 ms, **0**/0 / 460 (380, 71) | 0 / 0 / 0 ms | 1030 ms | 3582 ms, 0 / 607 |
| 2 | Chromium | 29/29 | 535 s | 1000 ms, **0**/0 / 488 (403, 73) | 0 / 0 / 0 ms | 1855 ms | 3462 ms, 0 / 620 |
| 3 | Chromium | 29/29 | 525 s | 1000 ms, **0**/0 / 471 (387, 72) | 0 / 0 / 0 ms | 893 ms | 3567 ms, 0 / 621 |
| 4 | Chromium | 29/29 | 530 s | 1000 ms, **0**/0 / 455 (372, 71) | 0 / 0 / 0 ms | 1451 ms | 3690 ms, 0 / 616 |
| 5 | Chromium | 29/29 | 539 s | 1000 ms, **0**/0 / 437 (358, 69) | 0 / 0 / 373 ms | 856 ms | 3516 ms, 0 / 617 |
| 1 | WebKit | 29/29 | 706 s | 1000 ms, **0**/0 / 229 (150, 71) | 0 / 0 / 0 ms | 1802 ms | 3930 ms, 0 / 422 |
| 2 | WebKit | 29/29 | 679 s | 1000 ms, **0**/0 / 260 (178, 72) | 0 / 0 / 130 ms | 2080 ms | 3870 ms, 0 / 440 |
| 3 | WebKit | 29/29 | 681 s | 1000 ms, **0**/0 / 258 (177, 75) | 0 / 0 / 0 ms | 2284 ms | 3900 ms, 0 / 432 |
| 4 | WebKit | 29/29 | 695 s | 1000 ms, **0**/0 / 246 (169, 71) | 0 / 0 / 0 ms | 1213 ms | 3840 ms, 0 / 427 |
| 5 | WebKit | 29/29 | 678 s | 1000 ms, **0**/0 / 245 (164, 74) | 0 / 0 / 0 ms | 2167 ms | 3900 ms, 0 / 436 |

**290/290 grün, 0 Verstöße in allen 20 `live-lag`-Attachments** (Zähler, Chip, Inlinks), alle Exit 0. „Kettenlatenz“
wie in Runde 3: Alter der ältesten URL, die der Sitzungszähler enthält, der Fuß aber nicht. Die Browser blieben bis
2,28 s stehen (Recorder-Lücken), ohne dass Fuß und Zähler auseinanderliefen: beide kommen im selben Tick.
Fremde lauffähige Threads auf den vier CPUs in 0,2–2,8 % der Sekunden; Serverlast 5–53 (andere Pakete).
Rohdaten `/tmp/lagfix2/acc5-{chromium,webkit}-{1..5}/`, Auswertung `/tmp/lagfix2/acc5-eval.jsonl`,
Sampler `/tmp/lagfix2/acc5-sampler.jsonl`, Auswerter `/tmp/lagfix2/acc-eval.py`, Skript `/tmp/lagfix2/run-acc.sh`.

### Alle Serien (gleicher Aufbau, rote Läufe nicht ausgeblendet)

| Serie | Stand | Zählertest grün | Bemerkung |
|---|---|---|---|
| Runde 3 (Tester) | `bb0b27e1` (main) | 1/10 | Befund 1–3 |
| 1 | `a4bc9da8` | 6/10 (286/290) | Befund 3 + Browser-Stillstände; Anlauf (Chip ohne Zahl) |
| 2 | `af329580` | 6/10 (286/290) | Chip ohne Zahl im Anlauf (2×); eigene Lesespur: Fuß 9,6 s bei 179/180 (1×); WebKit 5 von eigener Diagnose gestört |
| 3 | `c7f3cc0d` | 7/10 (287/290) | alle drei roten Läufe überlappten den eigenen Workspace-Test (7–10 min); nicht gewertet |
| 4 | `9bc17cf7` | 10/10 (290/290) | mit früher Tabellenöffnung (später zurückgenommen, Befund 4) |
| **5** | **`55786c7d`** | **10/10 (290/290)** | **Endstand** |

Zwischen den Serien liegen die Diagnoserunden diag3–diag7 (Spuren, je 4 Läufe je Engine; diag7 8/8 grün,
Tickabstand höchstens 1002 ms).

## Renderlast vorher/nachher

Messrezept aus `b4-livedetail.md`/`b4-lagfix.md` unverändert (`e2e/perf/livedetail.measure.ts`): Produktionsbuild
(`vite preview`), Release-Bridge (WebSocket), 5 000 Seiten × 30 Links, 150 URL/s, 4 Threads, 300 URLs Anlauf, acht
Klickrunden (24 Detailwechsel), 5 s Leerlauf, `--workers=2`, ohne CPU-Bindung. Vorher: Release-Bridge und
Perf-Frontend aus `bb0b27e1`; nachher: Release-Bridge aus Rust `b7859d48`, Frontend `55786c7d`. Je Engine vorher →
nachher abwechselnd, sechs Durchgänge, Serverlast beim Start 2,6–9,8. Alle 24 Messläufe Exit 0.

Hauptthread-Belegung (verspätete 10-ms-Timer) und erster Klick (UI-145, Klick bis gemalter Frame), Durchgang 1–6:

| Engine, Stand | Leerlauf | Klickphase | Klick Median | Klick Max |
|---|---|---|---|---|
| Chromium vorher | 6,3 · 3,8 · 4,4 · 6,2 · 6,1 · 5,8 % (Ø 5,43) | 8,0 · 7,7 · 6,5 · 6,8 · 7,4 · 6,8 % (Ø 7,20) | 62 · 60 · 61 · 62 · 64 · 62 ms | 85 · 96 · 83 · 93 · 91 · 85 ms |
| Chromium nachher | 4,8 · 5,2 · 5,4 · 4,6 · 4,4 · 4,6 % (Ø **4,83**) | 7,5 · 6,5 · 6,5 · 6,7 · 7,4 · 6,7 % (Ø **6,88**) | 64 · 65 · 61 · 62 · 65 · 64 ms | 121 · 94 · 102 · 108 · 104 · 110 ms |
| WebKit vorher | 3,1 · 2,7 · 3,0 · 2,4 · 2,8 · 2,0 % (Ø 2,67) | 8,0 · 7,5 · 8,1 · 6,3 · 8,9 · 7,2 % (Ø 7,67) | 85 · 96 · 88 · 78 · 87 · 88 ms | 136 · 128 · 99 · 133 · 136 · 110 ms |
| WebKit nachher | 2,4 · 2,8 · 2,8 · 2,4 · 3,1 · 1,7 % (Ø **2,53**) | 7,8 · 9,6 · 9,2 · 8,2 · 7,5 · 6,1 % (Ø **8,07**) | 83 · 101 · 98 · 94 · 87 · 81 ms | 130 · 143 · 135 · 139 · 136 · 143 ms |

- **Leerlauf-Renderlast nicht gestiegen**, auch in WebKit (2,67 → 2,53 %); Chromium 5,43 → 4,83 %.
- Klickphase: Chromium 7,20 → 6,88 %, **WebKit 7,67 → 8,07 %** (+0,4 Punkte; Streuung der Durchgänge 6,1–9,6 %).
- Klick-Median: Chromium 61,9 → 63,6 ms, WebKit 87,0 → 90,7 ms (Streuung je Durchgang 78–101 ms).
- **Klick-Maximum Chromium höher in 6 von 6 Durchgängen** (83–96 → 94–121 ms), WebKit 99–136 → 130–143 ms;
  alle unter 150 ms. Vermutung, nicht belegt: Ein Tick trägt jetzt Status und View-Änderung zusammen; ein Klick,
  der auf diesen Tick fällt, wartet auf eine längere Aufgabe (mehr Renderarbeit insgesamt zeigt die Belegung nicht).
- Ein früherer Lauf auf `9bc17cf7` (3 Durchgänge, Serverlast 0,5–3,2) zeigte überall leichte Verbesserungen
  (Chromium Leerlauf 5,80 → 5,37 %, WebKit 2,93 → 2,47 %, Klickphase 7,67 → 6,97 / 7,87 → 7,17 %, Klick-Max
  134 → 101 / 150 → 152 ms; `/tmp/lagfix2/perf-9bc17cf7/`). Die Unterschiede zwischen beiden Läufen derselben
  Vorher-Binärdatei (Chromium Leerlauf 5,80 vs. 5,43 %) liegen in derselben Größenordnung wie die gemessenen Effekte.

Rohdaten `/tmp/lagfix2/perf-final/livedetail-*-lagfix2-*.json` und `summary.txt`, Protokolle
`/tmp/lagfix2/perf-final-a/`, `/tmp/lagfix2/perf/`, Auswerter `/tmp/lagfix2/perf-eval.py`, Skript `/tmp/lagfix2/run-perf.sh`.
Ein erster Versuch scheiterte an einem Skriptfehler (Symlink statt Kopie des Produktionsbuilds; Vorschauserver ohne
Inhalt, alle Läufe nach 60 s abgebrochen; `/tmp/lagfix2/perf-failed-symlink/`) und ist nicht gewertet.

## Gates

Server, Worktree `m3-lagfix2` auf dem Endstand (Inhalt geprüft gleich `55786c7d`), mit `CARGO_PROFILE_DEV_DEBUG=0
CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0`, Workspace-Test nach `~/ws-slot.sh`.

| Prüfung | Ergebnis |
|---|---|
| `cargo fmt --all --check` | Exit 0 |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | Exit 0 |
| `cargo test --workspace --locked` | **4425 bestanden, 0 fehlgeschlagen, 51 vorhandene Ignorierungen**, 614 Ergebnisgruppen, Exit 0 (20:40–21:04 UTC) |
| `cargo test -p os-app` (nach den letzten Rust-Änderungen) | 629 bestanden, 0 fehlgeschlagen, 3 vorhandene Ignorierungen |
| `npm run -w apps/desktop typecheck` | Exit 0 |
| `npm run -w apps/desktop test` | **1040/1040**, 124 Dateien, Exit 0 |
| `scripts/check-neutral.sh` | Exit 0 |
| Playwright `e2e/acceptance` (main-Fassung), `e2e/alpha`, `e2e/statusbar`, `e2e/sidebar`, beide Engines, `--workers=2 --retries=0` | **195 bestanden, 1 rot, 2 übersprungen** (16,6 min): `alpha/detail-sidebar.spec.ts:135` Chromium (siehe Beobachtungen) |
| Abnahme `adb2371c` (Nachweis) | 290/290 |

Gezielte Wiederholungen der in Gate-Läufen roten Tests (WebKit bzw. beide Engines, Endstand gegen `main`-Worktree
`m3-lagfix2-base` mit `main`-Bridge): `table.spec.ts:39` WebKit 10/10 (main 10/10),
`livedetail-dialogs.spec.ts:311` WebKit 10/10 (main 10/10), `detail-sidebar.spec.ts:135` WebKit 10/10 und beide
Engines 29/30 (main 30/30).
`livedetail-detail.spec.ts:198/244/305` (main-Fassung, in einem früheren Gate-Lauf rot) 3/3 grün wie auf main; die
Testbranch-Fassung lief in der Abnahme grün. Protokolle `/tmp/lagfix2/{gate-pw,rerun*,sb-*}/`, `*-final2.log`.

## Fremde Dateien

Alle Änderungen in eigenen Commits auf `m3/lagfix2`; keine fremden Änderungen übernommen, nichts zurückgesetzt.
Dateien außerhalb des engen Tabellen-/Sitzungsbereichs, die für den Fix nötig waren (K2-92):

- `crates/os-engine/src/writer_thread.rs`, `crates/os-engine/src/crawl.rs` (Engine): Schreibanforderung für gezählte
  URLs; bestehende Writer-Logik unverändert, neuer Unit-Test.
- `crates/os-app/src/mcp/mod.rs`: zweiter Tick-Sender (MCP-Schleife) während aktiver Sitzung still.
- `crates/os-app/src/events/coalescer.rs`, `runtime/hooks.rs`, `runtime/mod.rs`: Vorschau/Beschleunigung des Takts.
- `apps/desktop/src/ipc/events.ts`: Sichtbarkeit beim Laden.
- `apps/desktop/src/features/table-advanced/stores.ts`, `SegmentControls.tsx`: Segmentliste eines anderen Crawls
  ohne Revisionssprung (einziger Verbraucher der Revision ist die obere Tabelle).

Nicht committet, nur temporär: Instrumentierung (Server, zurückgesetzt), Kopien der Abnahmedateien
(`apps/desktop/e2e/lagfix2-acc/`, Wegwerf-Worktree `m3-lagfix2-acc`), Diagnose-Spec `diag-live.spec.ts`/`diag-startup.spec.ts`.
Die Dateien `apps/desktop/e2e/acceptance/livedetail-*` dieses Branches sind unverändert.

## Beobachtungen außerhalb des Auftrags

- **Lastspitzen je Tick im Debug-Aufbau:** `export.catalog` (≈ 400 ms), `catalog.filterCounts` (≈ 290 ms) und
  `sidebar.overview` (≈ 290 ms) laufen in der E2E-Bridge etwa einmal je Tick (Spur `fix2`: 118/119/107 Aufrufe in
  ≈ 80 s). Das ist ein großer Teil der Rechenzeit der Bridge auf zwei CPUs und verlängert die View-Pflege beim Start
  (zweite Pflege 330–790 ms). Die gemeinsame Zählung ist laut UI-018 auf max(2 s, 3 × Dauer) gedrosselt; ob die
  Aufrufe je Tick nur Cache-Treffer sein sollten, wäre zu prüfen (nicht geändert).
- **Kosten des Crawlwechsels im Frontend (Dev-Build):** Nach dem ersten Tick eines neuen Crawls rendert das Fenster
  1–2 s (CPU-Profil: React, Menüverfügbarkeit, Overview-Zeilen, Diagramm-`measure`). Renderlast-Thema, nicht geändert.
- **Schließen während einer View-Erstellung:** Ein `view.close` während `view.open` noch „building“ meldet, ließ
  vermutlich eine View im Kern zurück, die weiter gepflegt wurde (zweite gepflegte View in den Spuren, einmal auch auf
  `main`). Mit dieser Änderung tritt der Fall beim Crawlstart nicht mehr auf; der Kernpfad selbst ist unverändert.
- **Live-Intervall über 1 s:** Zeilen und damit Fuß/Chip folgen weiterhin dem eingestellten Intervall (bis 5 s),
  während K2-39 für Zähler ≤ 1 s festlegt. Unverändert (vorbestehend), nur der Standard 500 ms ist hier geprüft.
- **Zähleralter im Debug-Aufbau:** Abschluss einer URL bis zum Tick mit ihrem Zähler (Spur diag7): Median 540–610 ms,
  p99 0,94–1,17 s, Max 1,02–1,24 s (vorher, verdeckt 1 Hz: Median 610–680 ms, Max 1,2–2,1 s). Der Tick wartet bis
  zu 500 ms auf die Pflege; im Release-Build dauert sie 10–30 ms.
- **Flake:** `table.test.tsx` „cancels removal …“ einmal rot im gemeinsamen Vitest-Lauf mit `table-advanced`/
  `segments-dialog` unter Last, danach 5/5 grün (allein und gemeinsam) und im vollen Endlauf grün.
- **Overview direkt nach „Completed“:** `alpha/detail-sidebar.spec.ts:135` liest die Overview ohne Warten unmittelbar
  nach „Completed“; die exakte Endzählung (UI-018, eigener Takt) kommt mit einem eigenen Tick. Endstand 1/30 rot
  (Chromium) plus einmal im Gate-Lauf, `main` 0/30; Varianten der Endmeldung (sofort/mit nächstem Tick) lagen bei
  1–2/30. Nicht behoben; ob die Endmeldung auf die exakte Endzählung warten soll, ist eine Produktfrage.

## Nachtrag (Entscheidung Koordination, 09.11.2026)

1. **Klick-Maximum Chromium:** Die Verschiebung 83–96 → 94–121 ms (Budget 150 ms, Median unverändert) ist als
   **bekannte Verschiebung** akzeptiert.
2. **„Completed“ erst mit exakter Endzählung:** umgesetzt (`ff83ad2a`, `d9f80135`).
   - Kern (`session/mod.rs`, `commands/sidebar/counts.rs`): Endet ein Crawl im Poll, sendet die Sitzungsschleife den
     Endstatus erst, wenn die exakte Endzählung (`observe_state`, Overview/Issues/Filterzähler) angekündigt ist,
     höchstens 3 s (`TICK_SETTLE_COUNTS`); Endstatus, letzte Zeilen und neue Zählversion kommen in einem Tick.
   - Oberfläche (`state/session.ts`, `features/sidebar/frame/useSidebarQuery.ts`): Overview und Issues melden sich
     als Endschranke an. Der Übergang „running → completed“ eines Crawls wird erst angewendet, wenn sie mit der neuen
     Version neu geladen und ihre Antwort veröffentlicht haben (höchstens 3 s); der Rest des Ticks gilt sofort. Ohne
     angemeldete Ansicht gilt „Completed“ sofort. Damit sind Overview, Fuß und Chip unmittelbar nach „Completed“
     exakt, auch bei einem Stopp über den Steuerpfad (die Overview-Anfrage wartet dort auf die Endzählung).
   - Fixture-Test `apps/desktop/src/state/b4-lagfix2-end-gate.test.ts` (UI-018): Endstatus erst nach dem Neuladen,
     mit der Zählversion des Endticks; späterer Tick ersetzt den gehaltenen Status; ohne Ansicht sofort; neuer Crawl
     verwirft den gehaltenen Status.

### Nachweis nach dem Nachzug (Orchestrator, 10.11.2026, Server, Integrationsstand `m3/int-lagfix2` = main `0aba2a4c` + `m3/lagfix2` + `m3/livedetail-tests-ci`)

- `cargo fmt --all --check`: ok. `cargo clippy --workspace --all-targets -- -D warnings`: ok.
- `cargo test --workspace`: 4584 bestanden, 0 fehlgeschlagen, 51 ignoriert.
- `npm run -w apps/desktop typecheck`: ok. `npm run -w apps/desktop test`: 132 Dateien, 1136 Tests bestanden.
- `alpha/detail-sidebar.spec.ts:135` mit `--repeat-each=30 --workers=2 --retries=0`: 60/60 in Chromium und WebKit
  (vor dem Nachzug 29/30 je Engine).
- Playwright `e2e/acceptance e2e/alpha e2e/statusbar e2e/sidebar`, beide Engines, `--workers=2 --retries=0`:
  363 bestanden, 3 fehlgeschlagen, 2 übersprungen, 14 nicht gelaufen. Alle drei Fehler liegen in
  `acceptance/b4-cmpui.spec.ts` (Vorbereitung `crawlOnce`, 150 s). Gegenprobe: dieselbe Spec einzeln auf diesem Stand
  mit `--workers=1` 26/26, mit `--workers=2` 1 Fehler; auf unverändertem `main` (`0aba2a4c`) mit `--workers=2` in zwei
  Läufen 2 bzw. 1 Fehler. Der Fehler besteht also schon auf `main` und hängt an der parallelen Ausführung der Spec
  in beiden Engines; die Ursache ist nicht untersucht (Vermutung: gemeinsames vorbereitetes Home
  `os-cmpui-<ppid>`). Folgepunkt, nicht Teil dieses Pakets.
- Merge-Konflikte bei der Integration: `crates/os-engine/src/crawl.rs` (`is_finished` aus main und `flush_counted`
  behalten), `apps/desktop/playwright.config.ts` (Ausnahme `visual/language.spec.ts` aus main und
  Nightly-/Lag-Aufteilung behalten).

### Folgepunkte aus der Diff-Prüfung (nicht blockierend)

- Endet ein Crawl, wartet die Sitzungsschleife bis zu 4 s (`TICK_SETTLE_ENDED` + `TICK_SETTLE_COUNTS`); andere
  Sitzungen werden in dieser Zeit nicht abgefragt.
- Bricht ein View-Wartungslauf mit Panic ab, wird `busy` nicht zurückgezählt; `views::settle` wartet dann je Tick
  sein volles Budget (höchstens 500 ms).

## Abschluss

Produktcommits auf `m3/lagfix2` (kein Push): `033ae618`, `a4bc9da8`, `2c4a04c3`, `af329580`, `8ccd650b`, `408e5d38`,
`c7f3cc0d`, `9bc17cf7`, `7da47060`, `b7859d48`, `55786c7d` (Zwischenstände mit Lesespur und früher Tabellenöffnung
sind im Verlauf enthalten und wieder zurückgenommen). Kein bestehender Test, keine Grenze und keine Abhängigkeit
geändert; die Abnahmedateien des Testbranches wurden nur in Wegwerf-Worktrees genutzt.

Aufgeräumt: `target/` des Server-Worktrees gelöscht, Wegwerf-Worktrees `m3-lagfix2-acc` und `m3-lagfix2-base`
entfernt, Diagnose-Spec und Kopien entfernt, Zwischen-Binärdateien unter `/tmp/lagfix2/` gelöscht (behalten:
`bin-final5`, `bin-base`, `bin-rel-before`, `bin-rel-after6`). Keine eigenen Prozesse mehr aktiv.
