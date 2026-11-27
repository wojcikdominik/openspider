# Abnahme fix/overview-stale (unabhängig)

Stand: 26.11.2026. Geprüft wurde Zweig `fix/overview-stale` (`f59614fc` Produktcode, `d6261222` Test des Bauers) über main
`c7d9c424`. Abnahmezweig `fix/overview-stale-tests`. Grundlage sind ausschließlich `docs/spec/05-ui.md` UI-018/UI-061/UI-062,
die Koordinationsentscheidung „exakt, sobald Completed“ (`b4-lagfix2.md`, Nachtrag 09.11.2026, Punkt 2) und der Produktcode der
Korrektur. Test, Commit-Nachrichten und Berichte des Bauers wurden nicht gelesen.

Rust-Code: `git diff c7d9c424..HEAD -- crates` ist leer. Ein `rsync -nc` von `crates/` und `core/` gegen den Baum des Bauers auf
dem Mini meldet keine abweichende Datei. Das Bridge-Binary des Bauers (`wt/fix-overview-stale/target/debug/os-app-bridge`,
26.11. 11:28) gilt deshalb auch für diesen Stand.

## Tests der Abnahme

| Datei | Inhalt |
|---|---|
| `apps/desktop/src/features/sidebar/frame/acceptance-overview-end.test.tsx` | Integration mit `SidebarFrame`/`Dashboard`, Fake-Timer, gesteuerte Antworten; nach jedem einzelnen Timer gilt die DOM-Invariante „steht ‚completed‘ im DOM, dann der Endstand“ |
| `apps/desktop/src/features/sidebar/frame/acceptance-overview-end-contract.test.tsx` | Vertrag „Antwort im Cache ≠ gezeichnet“ mit einem Prüf-Verbraucher über `useRenderedAnswer` (simuliert den zurückgestellten Wert) |
| `apps/desktop/src/state/acceptance-end-barrier.test.ts` | Endschranke im Sitzungszustand bei Crawl-Wechsel |
| `apps/desktop/e2e/alpha/acceptance-overview-end.spec.ts` | echte Bridge, Fixture fx-ui-basic; DOM-Abzug per MutationObserver im Augenblick von „Completed“, Latenz Endtick → „Completed“ per `performance.now()` |

### Läufe (Mac mini, unter `heavy.lock`, 26.11.2026 19:53–20:00)

- `npm ci --ignore-scripts`: ok. `npm run -w apps/desktop typecheck`: ok.
- Vitest, eigene Dateien: **18 Tests, 14 grün, 4 rot**. Ein rotes Ergebnis (Zeitstempel-Test) war ein Harness-Fehler
  (die erste Antwort lag nach fünf 0-ms-Schritten noch nicht vor). Der Test wurde auf ein angehaltenes `Date.now()` umgestellt
  und im Nachlauf geprüft. Nachlauf 20:31 (typecheck ok, gleiche 18 Tests): 14 grün, 4 rot. Die vier roten sind jetzt alle echte Befunde: B2-1, B2-2, B2-5, B3-1.
- Vitest, Umfeld (`src/features/sidebar`, `src/features/dashboard`, `src/state`; darin der Test des Bauers und die
  bestehenden Sidebar-, n27- und Endschranken-Tests): 108 Tests, 104 grün. Rot sind nur die vier eigenen Tests von oben.
- Playwright, `--workers=1 --retries=0`, Bridge des Bauers:

| Szenario | WebKit (20×) | Chromium (5×) | Latenz Endtick → „Completed“ WebKit / Chromium (min–median–max) |
|---|---|---|---|
| Seitenleiste, Overview aktiv | 20/20 grün | 5/5 grün | 511–528–690 ms / 558–582–597 ms |
| Dashboard offen + Issues-Reiter aktiv | **0/20** (Dashboard beim „Completed“ nicht im DOM) | 5/5 grün | 254–530–665 ms / 559–628–655 ms |
| Issues-Reiter aktiv (Overview nicht gezeichnet) | 20/20 grün | 5/5 grün | 267–517–688 ms / 253–571–615 ms |
| Seitenleiste eingeklappt | 20/20 grün | **3/5** (2× Overview beim „Completed“ ohne Zahlen) | 270–523–631 ms / 257–543–678 ms |

Die Latenzgrenze von 1500 ms hielt in allen 100 Läufen; das Maximum lag bei 690 ms. Die 1500 ms liegen deutlich unter der
Obergrenze von 3 s. Eine Freigabe über die Obergrenze schlägt damit sicher an. Die Schranke braucht eine Antwort zur Endversion
plus ein Zeichnen. Die gemessenen gut 500 ms sind im Median genau ein Intervall der gebündelten Aktualisierung (500 ms) und
lassen auf dem belasteten Mini noch Faktor 2 Reserve.

## Durchsicht

Fundstellen beziehen sich auf den Stand `d6261222`.

**A1 – Unnötiges Warten bis zur Obergrenze.** Gewartet wird nur, wenn ein Verbraucher schon einmal gemeldet hat
(`useSidebarQuery.ts:42`: `!this.reported` → sofort). Der Verbraucher meldet in einem Layout-Effekt
(`useSidebarQuery.ts:52`), Hook-Instanz und Verbraucher liegen jeweils in derselben Komponente (`SidebarFrame.tsx:33/39`,
`Dashboard.tsx:19/24/26/27`, `Issues.tsx:76/78`).
- Seitenleiste eingeklappt (`Shell.tsx:80`, nur `hidden`): `SidebarFrame` bleibt gemountet und committet weiter. Kein Warten.
  Playwright: 20/20 WebKit, Latenz ≤ 631 ms. In Chromium hatte die Overview 2/5-mal beim „Completed“ keine Zahlen, siehe
  Befund B2-3.
- Anderer Reiter aktiv: `SidebarFrame` meldet `overviewData` unabhängig vom Reiter (`SidebarFrame.tsx:39`). Kein Warten
  (Vitest „(i) Issues“, Playwright „Issues aktiv“).
- Issues-Reiter nie geöffnet: Es gibt keine Instanz und damit keine Anmeldung (Vitest „A1 Issues tab never opened“).
- Dashboard ohne Seitenleiste sowie Dashboard zusammen mit der Seitenleiste (gemeinsame Abfrage): grün in Vitest. Lesart zur
  stillen Abbruch-Übergabe von TanStack: `query.js:452` reicht das Promise der neuen Anfrage weiter, die erste Schranke wartet
  deshalb korrekt mit.
- Fenster im Hintergrund oder minimiert: nur gelesen. Layout-Effekte laufen beim Commit, und Reacts Scheduler hängt nicht an
  `requestAnimationFrame`. Gedrosselte Timer verzögern höchstens die 0-ms-Pause in `session.ts:61`. Kein Test.
- Zweites Fenster: Jedes Fenster hat eigenen Modulzustand (`endBarriers`, `answers`). Visualisierung und Einstellungen nutzen
  `useOverview`/`useIssues` nicht (grep). Keine Wirkung.
- Abfrage deaktiviert (`enabled: !!crawl`): Ohne Crawl gibt es keine Anmeldung (`useSidebarQuery.ts:93`). Fehlerzustand:
  „Completed“ erscheint sofort (Vitest A4, grün).
- Unveränderte Antwort (Structural Sharing): `note()` setzt den neuen Zeitstempel auf dasselbe Objekt und gibt frei
  (`useSidebarQuery.ts:31–34`). Grün.
- Verbraucher wird während des Wartens abgebaut: **Befund B2-1**. Die Schranke wartet bis zur Obergrenze.

**A2 – Speicher und Abmeldung.** `answers` und `stamps` sind WeakMaps (Schlüssel: die an den Observer gebundene
`refetch`-Funktion bzw. das Antwortobjekt). Die Anmeldung wird beim Abbau und bei Crawl-Wechsel per Effekt-Cleanup entfernt
(`useSidebarQuery.ts:93–97`). Vitest „no registration survives unmounting“ ist grün. Liegengebliebene `waiters` (nach einer
Freigabe über die Obergrenze) werden mit der nächsten gezeichneten Antwort aufgelöst oder mit der Instanz verworfen.
Ein fremder Stand erfüllt nichts, weil die Zeitstempel monoton sind (Vitest „crawl switch“, grün). Ausnahme ist die
Millisekunden-Gleichheit, **Befund B3-1**. Im unveränderten `session.ts:58–66` kann jedoch eine noch laufende Freigabe des
vorigen Crawls den gehaltenen Endstatus des nächsten Crawls freigeben, **Befund B2-2** (vor der Korrektur schon vorhanden).

**A3 – Pause.** Die Schranke greift nur bei `running → completed` (`session.ts:88`), „Paused“ gilt sofort. Die Werte
erreichen den pausierten Stand ohne Nutzeraktion über die gebündelte Aktualisierung (Vitest A3, grün, < 1,5 s Fake-Zeit).
Ob auch „Paused“ erst mit exakten Werten erscheinen muss, regelt die Entscheidung „exakt, sobald Completed“ nicht:
**B2-4 (Unklarheit)**. Dasselbe gilt für die Phasen `incomplete` und `failed`.

**A4 – Fehlerfall.** „Completed“ erscheint ohne Obergrenze (Vitest, grün). Ob danach nachgeladen wird, hängt vom
Zeitpunkt ab. Scheitert die Endanfrage, bevor das 500-ms-Intervall fällig ist, schickt das Intervall eine eigene Anfrage
(grün). Läuft sie noch, wenn das Intervall fällig wird, übernimmt das Intervall die scheiternde Anfrage und setzt danach
`requested` auf die Endversion (`useSidebarQuery.ts:82`). Dann wird nie wieder geladen, und die Overview bleibt ohne
Nutzeraktion auf dem alten Stand: **Befund B2-5** (vor der Korrektur ebenso). Der Abbruch einer laufenden Endanfrage
ist nicht als Test gefasst.

**A5 – Mehrfaches Ende.** Fortsetzen mit erneutem Ende sowie eine Datenänderung im Zustand „Completed“ (Entfernen):
Jedes Ende ist exakt, keine Schranke hängt, und über 5 s kommt keine weitere Anfrage (Vitest A5, grün).

**A6 – Große Crawls (nur gelesen).** `useDeferredValue` bleibt wirksam. Neu je Render sind nur `note()` (WeakMap) und je
neuer Datenreferenz ein Layout-Effekt mit `report()`. Es gibt kein `setState` im Layout-Effekt und kein `flushSync`, also
keine zusätzliche synchrone Renderlast. Nebenwirkung **B3-2**: `useRenderedAnswers` liest `dataUpdatedAt`, und TanStack
verfolgt diese Eigenschaft. `RegularIssues` und `Dashboard` (Issues-Teil) rendern deshalb jetzt auch bei inhaltlich
unveränderter Antwort neu. Das passiert höchstens einmal je Zählung. **B3-3**: Teilen sich Dashboard und Seitenleiste eine
Abfrage, kann `requested = started.current ?? requested` (`useSidebarQuery.ts:82`) nach Übernahme einer fremden Anfrage
eine zusätzliche Anfrage für dieselbe Version auslösen. Das ist nur gelesen, nicht gemessen.

## Befunde

| ID | Schwere | Befund | erwartet / beobachtet | Fundstelle, Test |
|---|---|---|---|---|
| B2-1 | B2 (Regression in schmalem Fenster) | Wird der wartende Verbraucher abgebaut, nachdem die Endantwort im Cache liegt und bevor er sie zeichnet, wartet „Completed“ bis zur Obergrenze. | erwartet: „Completed“ ohne die 3 s, sobald nichts mehr zu zeichnen ist; beobachtet: 2900 ms nach dem Abbau (3000 ms nach dem Endtick) | `useSidebarQuery.ts:42–45, 93–97` (das Promise der Schranke hängt am abgebauten `RenderedAnswers`), `session.ts:59` (Momentaufnahme der Schranken); Test `acceptance-overview-end-contract.test.tsx` „A1 the waiting consumer is unmounted …“ (rot) |
| B2-2 | B2 (bestand vorher, kein Fehler der Korrektur) | Eine noch laufende Freigabe von Crawl A gibt den gehaltenen Endstatus von Crawl B frei, sobald A's Ansicht fertig ist, obwohl B's Ansicht noch lädt. | erwartet: B bleibt „running“, bis B's Schranke erfüllt ist; beobachtet: „completed“ | `session.ts:62` (`const crawl = heldEnd` liest den Status des neuen Crawls); Test `acceptance-end-barrier.test.ts` (rot) |
| B2-3 | B2 (offen, Ursache nicht geklärt) | Chromium, Seitenleiste eingeklappt: 2/5-mal stand beim Erscheinen von „Completed“ keine Overview-Tabelle im DOM. Latenz dabei 257/260 ms, es gab also keine falschen Zahlen, aber auch keine Zahlen. WebKit 20/20 grün. | erwartet: Endstand im DOM; beobachtet: keine Summary-Tabelle | Playwright „sidebar collapsed“, Traces lagen unter `test-results/…chromium-repeat{1,2}` auf dem Mini |
| B2-4 | B2 (Unklarheit Spec) | Gilt „exakt, sobald …“ auch für „Paused“ (UI-018 nennt Pause und Ende)? Heute gilt „Paused“ sofort, die Werte folgen ohne Nutzeraktion. | Entscheidung nötig | `session.ts:88` |
| B2-5 | B2 (bestand vorher) | Scheitert die Endanfrage, während das 500-ms-Intervall sie übernommen hat, lädt die Overview nie wieder ohne Nutzeraktion. | erwartet: finaler Stand ohne Nutzeraktion; beobachtet: bleibt bei „3“ | `useSidebarQuery.ts:82`; Test „A4 … (answer after 700 ms)“ (rot), die Variante mit 5 ms ist grün |
| B2-6 | B2 (offen, vermutlich nicht Teil der Korrektur) | WebKit: Das vor dem Start geöffnete Dashboard war beim „Completed“ 20/20-mal nicht mehr im DOM, in Chromium 5/5-mal schon (27/27 korrekt). Die Issues-Zähler im selben Szenario waren exakt. | erwartet: Dashboard bleibt offen und zeigt den Endstand | Playwright „Dashboard open and sidebar Issues active“ (rot in WebKit). Die WebKit-Traces wurden vom Chromium-Lauf überschrieben, die Ursache ist nicht untersucht (Verdacht: `state/tabs.ts` `load()` setzt `view` beim Crawl-Start zurück) |
| B3-1 | B3 | Freigabe über gleiche Millisekunde: Tragen Start- und Endantwort dasselbe `dataUpdatedAt`, zählt die gezeichnete Startantwort als „Endstand gezeichnet“. Praktisch kaum erreichbar. | erwartet: kein „Completed“, solange die Startantwort im DOM steht; beobachtet: „completed“, Prüf-Verbraucher zeigt „3“ statt „27“ | `useSidebarQuery.ts:25–26, 42` (Vergleich `w.at <= at` über Millisekunden); Test „same millisecond“ |
| B3-2 | B3 | Zusätzliche Renders von Issues/Dashboard bei unveränderter Antwort (siehe A6). | – | `useSidebarQuery.ts:104, 118, 133` |
| B3-3 | B3 | Mögliche Doppelanfrage je Version bei gemeinsamer Abfrage (siehe A6). Nur gelesen. | – | `useSidebarQuery.ts:82` |

## Nicht geprüft

- Fenster im Hintergrund oder minimiert sowie ein zweites Fenster: nur durch Lesen beurteilt (siehe A1).
- Laufzeitbudgets großer Crawls (UI-143/UI-146): Sache des Gates, hier nur gelesen (A6).
- Abbruch einer laufenden Endanfrage durch einen Schlüsselwechsel (Scope-Wechsel genau im Endfenster): nicht als Test gefasst.
- Chromium nur 5×, WebKit 20×; Windows/WebView2 nicht.

## Empfehlung

**Mergen.** Der gemeldete Fehler ist behoben. Endtick bei laufender Startanfrage (Overview und Issues), Antwort im Cache
ohne Zeichnen, Dashboard, gemeinsame Abfrage und Mehrfachende sind in Vitest grün. Die Playwright-Szenarien mit Overview bzw.
Issues: WebKit 60/60, Chromium 13/15. Die Latenz liegt überall unter 700 ms. Die einzige Regression (B2-1) betrifft ein
schmales Fenster und ist durch die 3 s begrenzt; sie kann nachgezogen werden. Vorgeschlagen ist, beim Abbau des
Verbrauchers dessen Wartende aufzulösen. B2-2 und B2-5 bestanden schon vorher. B2-3 und B2-6 sollten vor dem nächsten
Release gesondert untersucht werden. Die roten Abnahmetests bleiben als Nachweis stehen.

## Zweite Durchsicht 27.11.2026

Geprüft wurde `fix/overview-stale` = `d85cf9b1` (umbasiert auf main `8ae20980`). Ich habe nur gelesen:
`git diff 8ae20980..d85cf9b1 -- apps/desktop/src` und die Commits `2288100f`, `2cf61724`, `107481b5`, `e86bd12c`. Den Test
des Bauers habe ich nicht gelesen. Neue Tests stehen in `apps/desktop/src/state/acceptance-end-barrier-pfad3.test.tsx`.
Sie sind geschrieben, aber nicht gelaufen; den Lauf stößt der Orchestrator an. Die Tests der ersten Abnahme sind unverändert
übernommen.

**1. Pfad 3 – Befund B2-7, dazu B3-4.**
- Normal lange Crawls werden nicht verzögert. Die Markierung `startedHere` wird vom ersten Status des Crawls verbraucht,
  gleich welcher Phase (`session.ts:131`). Pfad 3 greift nur, wenn dieser erste Status `completed` ist (`session.ts:148`).
  Ein erster Status `running` läuft den bisherigen Weg (`session.ts:157`).
- Start schlägt fehl (`command()` wirft): Es wird keine Markierung gesetzt. Kommt die Antwort spät: Ein inzwischen
  eingetroffener Status ist neuer, dann wird nichts gesetzt (`statusSeq`-Vergleich, `session.ts:80`).
- Zwei Starts oder ein Wechsel der Tick-Folge, bevor ein Status des gestarteten Crawls kommt: Die Markierung bleibt
  liegen (`startedHere`, nie verbraucht). Dasselbe passiert bei „Öffnen und Fortsetzen“ (`CrawlsHost.tsx:189`), wenn der
  Tick des geöffneten Crawls noch nicht angewendet ist. `resumeHere` liest dann die ID des **vorher** angezeigten Crawls
  (`session.ts:86–89`). Folge (B3-4): Wird dieser Crawl später als fertiger Crawl geöffnet, läuft das Öffnen über Pfad 3 statt
  sofort, also bis zu 3 s mit dem alten Crawl im Kopf. Ein fremdes Ende hält die Markierung nicht zurück, weil sie an der
  Crawl-ID hängt. Test P3-2.
- **B2-7:** Während Pfad 3 lädt, zeigen Overview und Issues den **neuen** Crawl (`viewCrawlId`, `useSidebarQuery.ts:132/147`).
  Kopfbereich, Statusleiste, Tabelle und die Dashboard-Karte „Remaining“ zeigen dagegen noch den **vorigen** Crawl
  (`session.ts:148–156`: nur `pendingCrawlId` wird gesetzt, `crawl` bleibt). Das sind typisch rund 0,5 s, höchstens 3 s.
  Die Frage des Orchestrators trifft also umgekehrt zu: Es erscheinen die Zahlen des neuen Crawls unter der Überschrift
  des vorigen, oft neben dessen „Completed“. Vor der Nacharbeit war die Overview in diesem Fall kurz leer bzw. „Loading…“.
  Gab es noch keinen Crawl, steht „kein Crawl“ neben Zahlen. Test P3-1: Nach jedem Timer müssen die Overview-Zahlen zum
  angezeigten Crawl passen. Vorschlag: Statt des vorigen Stands kurz den neuen Crawl als „running“ zeigen, oder die
  Overview bis zur Freigabe ohne Zahlen lassen.
- Ob die Anmeldung in `releaseEnd(…, true)` rechtzeitig kommt (eine 0-ms-Pause, bis die Effekte für die neue Crawl-ID
  laufen, `session.ts:97`), habe ich nur gelesen. Bei Sync-Lane-Updates führt React passive Effekte vor dem Timer aus, das
  sollte also reichen. Kommt die Anmeldung zu spät, gibt die Schranke ohne Warten frei. Falsche Zahlen entstehen dabei
  nicht, die Overview zeigt dann „Loading…“.

**2. B2-6 (`tabs.ts`) – in Ordnung.** `unsavedView` wird nur gesetzt, solange die Tab-Leiste nicht geladen ist
(`tabs.ts:141/156`). Es wird im nächsten `load()` übernommen und sofort gelöscht (`keepUnsavedView`). Gespeichert wird nur
`topTabs.view`, das übrige Layout bleibt (`tabs.ts:102`). Eine spätere bewusste Wahl nach dem Laden speichert normal und
in der Reihenfolge nach dem Merken. Eine Wahl vor dem Laden überschreibt `unsavedView` wieder, es zählt also stets die
letzte. Mehrere Fenster: Nur das Hauptfenster hat die obere Tab-Leiste, und das Speichern verschmilzt mit dem aktuellen
Dateistand. Kein Befund.

**3. Frühere Befunde.**
- B2-1: behoben. `attach()` zählt die Verbraucher, beim letzten Abbau wird `flush(true)` aufgerufen (`useSidebarQuery.ts:40–47, 64`).
- B3-1: behoben. Die Reihenfolge des ersten Sehens (`rank`) ersetzt die Uhr. Eine Antwort, die erst in `until()` gesehen
  wird, bekommt einen höheren Rang als die gezeigte. Unveränderte Objekte behalten ihren Rang.
- B2-5: behoben. Eine fehlgeschlagene Antwort wird je Version genau einmal wiederholt (`retried`, `useSidebarQuery.ts:99`).
  Bei dauerhaftem Fehler gibt es keine Schleife: höchstens eine Zusatzanfrage je Datenversion. Nur der Zweig für ein
  abgelehntes Promise (`() => { requested = null; }`) würde alle 500 ms neu fragen. TanStack lehnt `refetch` ohne
  `throwOnError` aber nie ab, deshalb ist das nur ein Hinweis (B3-5).
- B2-2: behoben. `endRelease` zählt die Freigaben, eine ältere gibt nichts mehr frei (`session.ts:101`). Der gehaltene
  Status des neuen Crawls bleibt bis zu dessen eigener Freigabe stehen.

**4. Speicher – B3-6.** `RenderedAnswers` hängt an der Hook-Instanz, `order` ist eine WeakMap, die Wartenden werden beim
Abbau aufgelöst: kein Leck. Dauerhaft wachsen nur zwei Maps: `statusSeq` (ein Eintrag je jemals gesehener Crawl-ID,
`session.ts:62`) und liegengebliebene `startedHere`-Einträge (siehe 1). Das sind wenige Bytes je Crawl, praktisch
unerheblich. Sauberer wäre, beim Wechsel des Crawls alle Einträge außer dem aktuellen zu löschen. Kein eigener Test,
weil beide Maps nicht exportiert sind.

**Empfehlung zweite Durchsicht: erst nachbessern (B2-7).** Die Korrekturen zu B2-1, B3-1, B2-5, B2-2 und B2-6 sind in
Ordnung. Pfad 3 tauscht aber einen kurz leeren Stand gegen einen gemischten Stand (neue Zahlen unter dem vorigen
Crawl). Das widerspricht dem Ziel von UI-018, dass die gezeigten Werte zum gezeigten Stand gehören. Wird B2-7 nicht
rechtzeitig behoben, ist es besser, `e86bd12c`/`d85cf9b1` (Pfad 3) aus dem Merge zu lassen und den Rest zu mergen.
B3-4 und B3-6 können nachgezogen werden.
