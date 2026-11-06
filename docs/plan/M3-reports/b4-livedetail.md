# Bericht B4-LIVEDETAIL: Live-Latenz Detailfenster (M2-N32)

Paket für beta.4. Branch `m3/livedetail`, Basis `816e51d2`. Budgets und Tests sind nicht gelockert (K2-63), Schema und
Abhängigkeiten unverändert, kein Push.

## Ergebnis

1. **Die Renderlast im Live-Crawl ist gesunken, die Klicklatenz kaum.** Hauptthread-Belegung im Leerlauf (Produktionsbuild):
   WebKit 7,5–8,1 % → 3,0–3,2 %, Chromium 8,0–8,3 % → 4,7–5,9 %. Die Zeit Klick bis Zeile bleibt innerhalb der
   Messstreuung, denn sie besteht aus einmaliger Arbeit, die diese Änderungen kaum berühren.
2. **Auf dem Server liegen die Mediane im Browser-Release-Aufbau unter 150 ms**, und zwar schon vor dem Paket. Das
   Backlog-Bild (300–600 ms, 1,8 s, Tab-Klick 1 610 ms) stammt aus Debug-Bridge und Dev-Server auf ausgelasteten Runnern
   und ließ sich hier nicht reproduzieren. Ob UI-145 gilt, entscheidet erst die Messung auf dem Mac mini.
3. **„Erste Seite ohne Gesamtzahl“ ist nicht umgesetzt**, weil es nichts spart (Abschnitt „Kernabfrage“).
4. **Das Budget wird im Live-Crawl ab rund 1,2 Mio. Links erreicht.** Allein die Kernabfrage für Inlinks braucht dort auf dem
   Server 100–150 ms, bei 3,1 Mio. Links rund 320 ms. Das Paket behebt das nicht; es braucht eine Entscheidung (Offene Punkte, Nr. 1).
5. **Der Mac selbst ist ungeprüft:** Das Messrezept lief unter Xvfb/WebKitGTK auf dem Server, nie auf macOS.

## Messaufbau

Alle Läufe: synthetische Site 5 000 Seiten × 30 Links, 150 URL/s, 4 Threads (wie `n04-live`), Klick nach 300 gecrawlten
URLs; acht Runden je Lauf, jede mit neuer Tabellenzeile: Inlinks → Outlinks → Inlinks (das dritte ist „zurückgehalten“, ohne
`view.open`). Server `openspider-build` (16 Kerne, gemeinsam genutzt), Start jedes Engine-Laufs bei Last < 4. Zwei
Durchgänge A/B, vorher und nachher abwechselnd. Der Stand „vorher“ ist `apps/desktop/src` von `816e51d2` mit demselben
Messwerkzeug. Werte: Median/Maximum in ms.

| Aufbau | Frontend | Kern und Transport |
|---|---|---|
| Release-Browser | Produktionsbuild (`vite preview`) | Release-Bridge (WebSocket) |
| Dev (wie E2E-Suite) | Vite-Dev-Server | Debug-Bridge (WebSocket) |
| Nativ | in die App eingebettet | Release-App, Tauri-IPC, WebKitGTK unter Xvfb (Software-Rendering, kein Zielsystem) |

## Messaufteilung vorher/nachher

Inlinks, erster Klick, je Durchgang A / B:

| Aufbau, Engine | Stand | Gesamt | Klick-Render | `view.open` | Tabellen-Render | Malen |
|---|---|---|---|---|---|---|
| Release, Chromium | vorher | 74/112 · 62/105 | 16/44 · 17/46 | 24/57 · 22/34 | 14/15 · 13/16 | 17/26 · 14/20 |
| | nachher | 74/97 · 71/94 | 17/37 · 17/39 | 24/49 · 23/26 | 14/18 · 12/17 | 17/25 · 18/24 |
| Release, WebKit | vorher | 103/123 · 92/158 | 24/29 · 23/53 | 25/31 · 28/61 | 17/28 · 16/21 | 34/51 · 27/54 |
| | nachher | 90/118 · 91/149 | 23/52 · 22/62 | 22/27 · 30/35 | 16/20 · 16/27 | 30/44 · 33/34 |
| Dev, Chromium | vorher | 163/191 · 135/202 | 53/69 · 53/76 | 64/106 · 57/92 | 23/51 · 24/28 | 7/18 · 7/40 |
| | nachher | 153/189 · 144/173 | 54/76 · 46/57 | 58/65 · 70/99 | 23/33 · 25/28 | 8/41 · 7/20 |
| Dev, WebKit | vorher | 157/216 · 146/177 | 40/88 · 35/90 | 72/79 · 60/81 | 21/24 · 16/25 | 31/36 · 28/42 |
| | nachher | 152/202 · 153/227 | 36/90 · 39/94 | 60/115 · 61/123 | 21/26 · 19/26 | 21/29 · 27/42 |
| Nativ, WebKitGTK | vorher | 142/183 · 163/217 | 23/42 · 24/41 | 38/44 · 48/75 | 31/39 · 40/66 | 40/68 · 53/72 |
| | nachher | 154/168 · 133/170 | 27/47 · 25/43 | 41/68 · 40/44 | 38/65 · 25/36 | 40/47 · 40/68 |

Outlinks, Gesamt (Median/Maximum), je Durchgang A / B:

| Aufbau, Engine | vorher | nachher |
|---|---|---|
| Release, Chromium | 60/89 · 63/70 | 51/61 · 57/66 |
| Release, WebKit | 88/101 · 89/101 | 84/96 · 89/98 |
| Dev, Chromium | 105/125 · 120/131 | 96/132 · 96/108 |
| Dev, WebKit | 118/147 · 106/120 | 102/123 · 98/153 |
| Nativ, WebKitGTK | 150/164 · 153/187 | 134/161 · 136/174 |

Hauptthread-Belegung (Anteil verspäteter 10-ms-Timer; Leerlauf = nur Live-Aktualisierungen, 5 s; Messphase = mit Klicks):

| Aufbau, Engine | Leerlauf vorher | Leerlauf nachher | Messphase vorher | Messphase nachher |
|---|---|---|---|---|
| Release, Chromium | 8,3 · 8,0 % | 5,9 · 4,7 % | 8,4 · 8,9 % | 7,4 · 7,5 % |
| Release, WebKit | 7,5 · 8,1 % | 3,2 · 3,0 % | 11,6 · 11,7 % | 7,8 · 7,9 % |
| Dev, Chromium | 19,8 · 18,9 % | 12,2 · 12,2 % | 23,7 · 23,0 % | 18,3 · 17,5 % |
| Dev, WebKit | 10,8 · 12,5 % | 6,6 · 8,6 % | 17,6 · 16,7 % | 12,1 · 12,7 % |
| Nativ, WebKitGTK | 20,4 · 21,0 % | 16,5 · 21,5 % | 22,9 · 22,7 % | 19,8 · 19,5 % |

Lesart:

- Eingabeverzögerung und Zeit bis zum Absenden von `view.open` liegen im Median überall bei rund 1–3 ms (Maximum 26 ms). Der
  Hauptthread blockierte den Klick in diesen Läufen nicht; der längste beobachtete Rückstau im Release-Aufbau lag bei 66 ms. Das 1 610-ms-Bild aus
  `b3-flakes.md` zeigte sich bei dieser Last nicht.
- Der Rückgang der Belegung ist in den Browser-Aufbauten in beiden Durchgängen eindeutig. Nativ ist er es nicht: Leerlauf nur
  in einem von zwei Durchgängen besser, Messphase leicht besser.
- Die Klickzeiten nachher sind nicht verlässlich besser. Inlinks in Chromium sind unverändert, Outlinks etwas besser. Die
  Unterschiede liegen in der Größenordnung der Streuung zwischen den Durchgängen.
- Nativ überschreiten die Maxima in allen vier Läufen 150 ms (nachher 168–174 ms, vorher 183–217 ms), Mediane um 150 ms. Das
  ist ein Software-Rendering-Aufbau, also kein Urteil über den Mac. Auffällig ist `view.open` bei Outlinks: 36–39 ms nativ,
  obwohl der Kern nur rund 2 ms braucht und die Bridge 7–13 ms. Der Tauri-IPC-Anteil beträgt hier also über 30 ms (UI-142
  verlangt ≤ 30 ms p95). Ob das auf WKWebView auch gilt, zeigt das Mac-Rezept.

### Kernabfrage (ohne UI, Release, `b4_livedetail_core`)

Anteile bei 5 000 Seiten, Messpunkt 90 % (149 327 Links), Median in ms: Inlinks `view.open` 31 (Link-IDs 28, erste Seite 0,5),
Outlinks 2,3 (Link-IDs 0,5, erste Seite 0,4). JSON-Serialisierung 0,04 ms bei rund 11 KB, erstes `view.rows` aus dem
Erstfenster 1 ms.

Ungedrosselt bei 100 000 Seiten (8 Threads), Inlinks der Start-URL bzw. einer Seite aus der Mitte:

| Links im Crawl | `view.open` Median Start/Mitte | davon Link-IDs | Outlinks `view.open` |
|---|---|---|---|
| 475 230 | 41 / 64 | 30 / 40 | 5 |
| 1 162 159 | 103 / 150 | 87 / 111 | 5 |
| 2 226 079 | 261 / 208 | 231 / 195 | 5–9 |
| 3 100 000 | 321 / 320 | 292 / 290 | 2–5 |

Am letzten Messpunkt waren alle 100 001 URLs gespeichert; die Dauer zeigt, dass weiter der Lauf über `links` stattfand.

Die Kosten wachsen also linear mit der Größe von `links`, weil `links_target` während des Crawls bewusst fehlt (K-74 ff.,
`docs/ARCHITECTURE.md`). Die Gesamtzahl ist dabei nur `ids.len()` und kostet nichts. Die Reihenfolge nach Quelladresse
verlangt alle Treffer, bevor die erste Zeile feststeht. **Eine erste Seite ohne Gesamtzahl spart deshalb nichts.** UI-145
erlaubt sie zwar, sie brächte aber nur eine zusätzliche Antwort und einen weiteren Zustand in der Oberfläche.

### Woher die Renderlast kam (Attribution)

CPU-Profil und Trace in Chromium vor den Änderungen, ein Lauf von rund 20 s: das Malen der Canvas war der größte einzelne
JS-Posten (117 ms), dazu kamen `Button`/`Wrapper` aus React Aria (bis 220 ms), `formatCount` für nie sichtbare Optionen
(28 ms), `overviewRows` (23 ms), `ColumnConfigurator`/`TabsConfigDialog` (je rund 7 ms). Im Leerlauf-Trace machten
`PrePaint` und `Paint` 265 von 755 ms aus. Nicht umgesetzt, obwohl sichtbar: `RegularUpperTable` je Tick, `TabBar.measureNow`,
Menümodell (Offene Punkte, Nr. 3).

## Änderungen

| Commit | Inhalt |
|---|---|
| `4c1d8b64`, `fdef5f74`, `8a0c2be4` | Messwerkzeug: `b4_livedetail_core.rs` (`--ignored`), `e2e/perf/livedetail*.ts`, `e2e-b/perf/livedetail.mjs`, `livedetail-mac.sh` |
| `60ec3163`, `9a23ac84` | `DataGrid`: Zellwerte einmal je Render, Malen nur bei geänderter Signatur, memoisierte A11y-Zeilen, keine Arbeit in verborgenen Tabellen, kein Zweitrender bei unveränderter Position |
| `0f99618f` | `TablePanel`: Datenstand wird im 500-ms-Takt gelesen, ein Tick rendert die Detailtabelle nicht mehr |
| `8725022d` | `ColumnConfigurator`, `TabsConfigDialog` rendern geschlossen nichts; `ComboBox` baut die Optionsliste nur geöffnet |
| `bc29afec` | `Dialog`-Primitive rendert geschlossen nichts (betrifft alle Dialoge; Fokusrückgabe per Test geprüft) |
| `e689cae8` | `SidebarFrame`: `staleSteps` nach Inhalt, `overviewRows` memoisiert; `PanelActions` mit `memo` |

**Harness-Korrektur, nicht rein additiv:** `e2e/perf/shell.ts` abonnierte Ticks selbst über `client.subscribe("main")`. Je
Fenster gibt es aber genau einen Ereigniskanal (`windows.rs`: `subs.insert` ersetzt). Im nativen Build bekam die Sitzung so
keine Ticks mehr, mein Rezept brach beim Warten auf den Crawl ab (`crawl null`). Das Shell liest jetzt vom Fenster-Bus. Das
weicht von „Harness nur additiv“ ab und ist deshalb hier genannt. Die Schwellen von `perf.bench.ts` sind nicht angefasst, die
Perf-Suite selbst lief aber nicht erneut.

Tests: `DataGrid.test.tsx`, `ComboBox.test.tsx`, `table-advanced.test.tsx`, `primitives.test.tsx`, `n32-memo.test.tsx`, jeweils
`// spec: UI-145`. Das Rezept selbst ist ein Messwerkzeug ohne Budget-Gate in der Suite.

## Nachtrag: Befunde des unabhängigen Testagenten

Testbericht: `docs/plan/M3-reports/b4-livedetail-tests.md`, in diesen Branch gemerged; die Abnahmetests sind unverändert. Alle Befunde
waren laut Gegenprobe auch auf `main` rot.

| Befund | Ursache | Fix | Test |
|---|---|---|---|
| **F2** Crawl-Pfad-Dialog ohne Zeilen (UI-050) | `.os-data-grid` füllt 100 % seines Elternteils. `.os-detail-grid` im Detailfenster bekommt die Höhe von `.os-detail-body`; der Dialogkörper hat keine, also war die Zeichenfläche 0 px hoch und es entstanden auch keine A11y-Zeilen. | Container `.os-crawl-path` mit fester Höhe (360 px) in `CrawlPath.tsx`. Zusätzlich liest die Ansicht jetzt von der Start-URL zur gewählten URL (`selected_crawl_paths`, Spec-Frage S8 unten); der Bericht OUT-023 hat einen eigenen Pfad und bleibt „Zielseite zuerst“. | Abnahme `the Crawl Path dialog …`; `b4_view_range.rs` (Reihenfolge, 150 Öffnungen im Live-Crawl) |
| **F3** Umschalt+Klick/-Pfeil geht im Live-Crawl verloren (UI-012, UI-018) | Bericht bestätigt und erweitert: (a) `selectRow` las Anker und Kennungen in mehreren Rundreisen und verwarf alles bei geänderter Version. (b) `view.rows` antwortet auf eine alte Version mit `Stale`. (c) Das Nachschlagen des Fokus setzte die Tastaturzeile auf einen älteren Stand zurück, sodass bei schnellen Tasten Schritte fehlten. | Neues Kommando **`view.range`**: Bereich zwischen zwei URLs aus genau einem Stand (Tab-View: unveränderlicher Snapshot ohne Datenbankzugriff, Linkliste, sonst Seitenlauf). Damit keine Abhängigkeit mehr von der Crawl-Rate. Fokus und Scrollposition antworten sofort, ältere Antworten bewegen die Tastaturzeile nicht mehr. | Abnahme `every Shift+click …` (15× je Engine grün); eigene E2E `e2e/beta/shift-range-live.spec.ts` bei 300 URL/s, 5× je Engine grün; `b4_view_range.rs`, `DataGrid.test.tsx` |
| **F1** Spaltenwahl geht beim Auswahlwechsel verloren (UI-008, UI-030) | Speichern erfolgt 1 s verzögert; die neu aufgebaute Tabelle lud den älteren gespeicherten Stand. | `columns.ts` führt noch nicht gespeicherte Änderungen je Layout; eine neue Tabelle liest sie sofort, und auch eine Ladeantwort, die das Speichern überholt, überschreibt sie nicht. | Abnahme `a column change … is not lost`; `columns.test.tsx` |
| **F5** Hochscrollen im Tail-Modus wirkungslos (UI-018) | Ursache gefunden: Ein laufender Versionswechsel hatte beim Start `tail` gelesen, sprang beim Abschluss ans Ende zurück und setzte `tail` erneut. | Scrollgesten melden sich sofort bei der Datenquelle; ein Versionswechsel übernimmt eine währenddessen geänderte Position. | Abnahme Tail-Modus 20× je Engine: in keinem der 40 Läufe nötig, ein zweites Mal zu scrollen (vorher rund jeder zehnte); `ViewDataSource.test.ts` |

**„A file operation failed.“ (F2, einmalig im Live-Lauf): nicht reproduziert, Ursache nicht belegt.** Mit der Höhe hat es nichts zu tun, das
war reines CSS. Der Text steht für den Fehlercode `Io`. Als Quelle im Dialog kommen nur `view.*`-Antworten infrage. Der Code `Io` entsteht dort
(a) beim Öffnen einer Leseverbindung (`check_sqlite_header` in `os-store/src/open.rs`, nur bei Dateisystemfehlern) oder (b) in der E2E-Bridge, wenn
der WebSocket schließt (`bridge.ts`, `err-bridge-closed`); (b) gibt es im Produkt nicht. Gegenprobe: 150 Öffnungen im Live-Crawl auf Kernebene ohne
Fehler (`b4_view_range.rs`), die Abnahmetests prüfen `alert` im Dialog und blieben in allen späteren Läufen grün (Crawl-Pfad 20×). Bei
erneutem Auftreten hilft der `messageKey` in `DetailError`, der heute nicht angezeigt wird.

**Zusatzbefund aus den Wiederholungen:** Der letzte verbliebene Fehlschlag von F3 (1 von 10 Läufen) war kein Verlust der Auswahl. Der Tail-Modus
schob die Tabelle zwischen Lesen der Zeile und Klick um zwei Zeilen, der Klick traf also eine andere URL. Das kann auch echten Nutzern
passieren. **Verhaltensänderung:** Eine markierte Zeile hält die Tabelle jetzt am Platz, bis zur nächsten Scrollgeste (Spec-Frage S9).

**Grenzen von `view.range`:**
- Überdeckt der Bereich eine View mit mehr als 20 000 Zeilen vollständig, entsteht weiter eine „Alle“-Auswahl. Sie gehört zu einem Stand und fällt im
  Live-Crawl beim nächsten Stand weg, wie Strg+A schon vorher.
- Sehr große Teilbereiche kommen in einer Antwort (rund 8 Byte je Zeile als JSON); gemessen ist das nicht.
- Fehlt der Befehl (Testdouble, ältere Bridge), läuft der alte Weg weiter. Genau das hatte zuerst verdeckt, dass die E2E-Läufe mit einer nicht neu gebauten Bridge liefen.

**Weiterer Befund aus den vollen Gates:** `e2e/config/dialog.spec.ts:135` (Regex-Tester) wackelt in WebKit, unabhängig von diesem Paket: 4 von 40 Läufen rot auf dem
Frontend von `816e51d2`, 6 von 40 auf dem Paketstand. Ursache: Der Fokus auf das Sprungziel kommt per `requestAnimationFrame`; kam dieser Frame spät, zog er den Fokus aus dem schon
bearbeiteten Feld, die Eingabe landete im Sprungziel. Fix in `ConfigDialog.tsx`: Ein bearbeitetes Feld behält den Fokus. Danach 60 von 60 grün, `e2e/config` in beiden Engines 48 von 48. Der Test blieb unverändert.

Die Messtabellen oben stammen vom Stand vor diesem Nachtrag. Der Klickpfad zu Inlinks/Outlinks ist hier nicht geändert, neu gemessen wurde aber
nicht; die Mac-Messung läuft auf dem Endstand.

## Mac-Messrezept

Vom Repository-Wurzelverzeichnis, in einer aktiven, entsperrten GUI-Sitzung:

```sh
apps/desktop/e2e-b/perf/livedetail-mac.sh
# nur erneut messen, ohne Bau:
apps/desktop/e2e-b/perf/livedetail-mac.sh --measure-only
# zusätzlich Kernmessung und Playwright-WebKit mit Release-Bridge:
OS_LIVEDETAIL_CORE=1 OS_LIVEDETAIL_WEBKIT=1 apps/desktop/e2e-b/perf/livedetail-mac.sh
```

- Baut den Release-Build mit `--features e2e-webdriver`, legt die Sidecar-Datei vorübergehend ab und entfernt sie danach.
- Misst 10 Runden. Größen über `OS_LIVEDETAIL_PAGES/RATE/THREADS/WARMUP/ROUNDS/IDLE_MS`; wartet auf Last < 2,5
  (`OS_PERF_LOAD_LIMIT`).
- Ergebnis: `target/loadtest/livedetail-native.json`, darin `budget`, `samples` und `summary`. Hardware-Angaben in
  `target/loadtest/livedetail-hardware.txt`.
- **Exit 1 bei Fehler oder wenn ein frischer Klick über 150 ms liegt** (strengste Lesart: Maximum, Klick bis gemalter Frame).
- **IPC-Anteil:** `openMs` bei Outlinks minus `open_ms` aus der Kernmessung.

**Stand der Prüfung:** Skriptablauf und `livedetail.mjs` liefen auf dem Server gegen die echte Tauri-App unter Xvfb. Dabei fielen
die Sidecar-Datei und der Ereigniskanal auf. Die macOS-Teile des Wrappers (`caffeinate`, `ioreg`, `system_profiler`) sind nur auf
Syntax geprüft (`bash -n`), weil auf dem Mac keine Läufe erlaubt waren.

## Nachweise

Stand vor dem Nachtrag (`8a0c2be4`), Server, `--workers=1 --retries=0`, Dev-Server und Debug-Bridge:

| Nachweis | Ergebnis |
|---|---|
| `n04-live`, Playwright-WebKit, 10× | **10/10 grün.** Klick bis Tabelle aus Sicht von Playwright (Budget 1 s): Inlinks 277/483, Outlinks 251/290, Inlinks zurückgehalten 215/320 ms |
| `detail-during-crawl`, Chromium und WebKit, je 10× | **20/20 grün.** `externalMs` (Budget 300 ms): Chromium 27/66, WebKit 29/46 ms, kein erneutes Öffnen nötig |

Das ist der Server (16 Kerne, Last in diesen Läufen rund 3–11), kein CI-Runner; die Fehlerrate auf `linux-webkit` in der CI bleibt nach dem Merge zu beobachten. Beide Specs
liefen danach je einmal in der gesamten Suite mit.

Endstand mit den Befunden des Testagenten (`6f888719` plus dieser Bericht):

| Prüfung (Server) | Ergebnis |
|---|---|
| `cargo fmt --all --check` | grün |
| `cargo clippy --workspace --all-targets -- -D warnings` | grün |
| `cargo test --workspace --exclude openspider-desktop` (ws-slot) | grün: 4 352 bestanden, 0 Fehler, 51 ignoriert |
| `npm run -w apps/desktop typecheck` | grün |
| `npm run -w apps/desktop test` | grün: 1 022 in 119 Dateien |
| `npm run -w apps/desktop check:static` (u. a. `gen:ipc --check`), `os-app-manifest --check` | grün |
| Gesamte E2E-Suite Chromium einschließlich Abnahme, `--workers=1 --retries=0` | grün: 556 bestanden, 10 übersprungen |
| Gesamte E2E-Suite WebKit einschließlich Abnahme | 550 bestanden, 13 übersprungen, 1 rot: der Regex-Tester (siehe Nachtrag) |
| Nach dem Fokus-Fix: Regex-Tester WebKit 60×, `e2e/config` Chromium und WebKit | 60/60, 48/48 |
| Abnahme `every Shift+click …` auf dem Endstand, 10× je Engine | 20/20 |
| `shift-range-live.spec.ts` (300 URL/s), 5× je Engine | 10/10 |
| Abnahme Tail-Modus, 20× je Engine | 40/40, kein zweites Scrollen nötig |
| `bash scripts/check-neutral.sh` | keine Fundstellen |

Die gesamte E2E-Suite lief vor dem letzten Commit (`ConfigDialog.tsx`), danach nur die genannten gezielten Läufe. Der Workspace-Test lief nach dem Katalog-Commit vollständig neu. Nicht ausführbar: macOS und
Windows; dort zählt die CI nach dem Merge. „linux-webkit“ heißt hier Playwright-WebKit auf dem Server, nicht der CI-Runner.

## Spec-Fragen

1. **UI-145 im Live-Crawl großer Dateien:** Gilt „erste Seite ≤ 150 ms“ auch für Inlinks während eines Crawls, in dem
   `links_target` bewusst fehlt? Das Akzeptanzkriterium nennt den Live-Fall nicht ausdrücklich, und `docs/ARCHITECTURE.md` nimmt
   den Lauf über `links` in Kauf. Gemessen ist das Budget auf dem Server ab rund 1,2 Mio. Links allein durch den Kern ausgeschöpft.
2. **Lesart des Budgets:** Median, p95 oder jeder einzelne Klick? Das Rezept prüft streng jeden Klick. UI-145 nennt keine
   Statistik (UI-142 nennt p95).

3. **S1–S7 aus dem Testbericht** gehen unentschieden an Fable.
4. **S8 (neu), UI-050:** Reihenfolge der Ansicht. Der Abnahmetest verlangt Start-URL zuerst. UI-050 sagt „von der Start-URL“ und „regelt die Ansicht“; OUT-023
   legt „Zielseite zuerst“ nur für den Bericht fest. Umgesetzt wie vom Test verlangt; zu bestätigen, dass Ansicht und Bericht verschieden sortiert sein sollen.
5. **S9 (neu), UI-018:** Beendet eine markierte Zeile den Tail-Modus? Die Spec nennt nur manuelles Hochscrollen. Umgesetzt: Die Tabelle bleibt stehen, bis der Nutzer
   wieder scrollt. Scrollen ans Ende nimmt den Tail-Modus wieder auf.

Keine Spec-Änderung vorgenommen.

## Fremde Dateien (K2-92)

| Datei | Änderung | Grund |
|---|---|---|
| `apps/desktop/src/ui/primitives/Dialog.tsx`, `primitives.test.tsx` | geschlossen kein Render, Test | Punkt 3 des Auftrags; die Primitive betrifft auch Kopfleiste und obere Tabelle |
| `apps/desktop/src/ui/primitives/ComboBox.tsx`, `ComboBox.test.tsx` | Optionsliste nur geöffnet | im Auftrag genannt, liegt in den Primitiven |
| `apps/desktop/e2e/perf/shell.ts` | Ticks vom Bus, Import des Rezepts | siehe Harness-Korrektur |
| `crates/os-app/tests/b4_livedetail_core.rs` | neue Messung (`--ignored`) | Messung Kernabfrage |
| `crates/os-analyze/src/details/mod.rs` | Crawl-Pfad-Ansicht Start → Ziel | F2 |
| `crates/os-app/tests/manifest_contract.rs`, `docs/plan/M2-ui-concept.md` | `view.range` im Katalog ergänzt, keine Prüfung gelockert | F3 |
| `crates/os-analyze/src/views/session/mod.rs` | `ViewSnapshot::total`, `keys` | F3 |
| `crates/os-app/src/commands/compare.rs`, `stub/mod.rs` | `view.range` für Change-Views und Stub | F3 |
| `crates/os-app/src/dto/{mod,view}.rs`, `apps/desktop/src/ipc/commands.json`, `ipc/view.ts`, `ipc/gen/KeyRange.ts`, `ipc/gen/RangeArgs.ts` | neues Kommando, generiert (`os-app-manifest`, `gen:ipc`) | F3 |
| `apps/desktop/src/data/ViewDataSource.ts` | `range`, `holdPosition`, `userScrolled`, `settled` | F3, F5 |
| `apps/desktop/src/features/detail/frame/persistence.ts`, `columns.ts` | Rückruf nach dem Speichern, ungespeicherte Änderungen | F1 |
| `apps/desktop/src/features/config/frame/ConfigDialog.tsx` | Sprungziel nimmt einem bearbeiteten Feld nicht den Fokus | wackelnder Regex-Test |
| `apps/desktop/playwright.config.ts`, `e2e/acceptance/**`, `docs/plan/M3-reports/b4-livedetail-tests.md` | aus `m3/livedetail-tests` gemerged, unverändert | Abnahme |

## Offene Punkte

1. **Entscheidung nötig – Inlinks im Live-Crawl ab rund 1 Mio. Links.** Der einzige erkennbare Hebel ist ein Index oder Cache
   für das Linkziel. Vorschlag für ein eigenes Paket: temporäre, verwerfbare Tabelle (Ziel → Link-ID) auf der gepinnten
   Leseverbindung, die nur neue `links.id` nachliest. Kosten: Plattenplatz und eine langsame erste Abfrage. Ein Speicher-Index im
   Prozess scheidet wegen der Speicherbudgets aus. Nicht begonnen, weil die Entwurfsfrage über dieses Paket hinausgeht.
2. **Mac-Messung steht aus** (Rezept oben); sie entscheidet UI-145 und den IPC-Anteil nach UI-142.
3. **Weitere Renderlast, nicht umgesetzt** (außerhalb des Auftrags): `RegularUpperTable` rendert je Tick samt Werkzeugleiste,
   `TabBar.measureNow`, Menümodell (`mainMenuContributions`, `MenuAvailability`). Das sind die nächsten Hebel für UI-143.
4. **`e2e/perf/run-reference.sh` und `e2e-b/perf/README.md` legen die Sidecar-Datei nicht ab.** Der native Teil dieser Referenzmessung
   dürfte deshalb beim Bau scheitern (auf dem Server für denselben `tauri build`-Aufruf belegt, auf dem Mac nicht geprüft). Nicht
   geändert, weil außerhalb des Pakets.
5. **`detail-during-crawl.spec.ts`:** Die Stale-Reaktion aus M2-N34 war bereits auf `main`; hier nur der Nachweis, kein Eingriff.

## Server und Aufräumen

- Kein Push, keine neue Abhängigkeit, kein `cargo update`; Tokio 1.53.3 und Lockfile unverändert. Keine Schemaänderung, kein neuer Index.
- Messungen starteten je Engine bei Last < 4 (Werte in den Serverprotokollen; nach dem Start konnte die Last durch parallele Pakete
  steigen, höchster Endwert der Messreihe rund 5). Die ersten Zwischenmessungen, die mit Parallelläufen überlappten, sind nicht verwendet.
- Auf dem Server sind `target/`, die Vorher-Kopie `m3-livedetail-base`, Testergebnisse und die Roh-JSON-Dateien gelöscht; zum
  erneuten Messen muss neu gebaut werden. `~/ws-slot.sh` verlangt inzwischen 130 GB freien Platz; mein `target/debug` hatte 117 GB belegt.
