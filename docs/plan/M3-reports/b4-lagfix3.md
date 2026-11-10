# B4-LAGFIX3: Regression aus B4-LAGFIX2 in den Bridge-Tests Export (OUT-020) und Visualisierung (UI-090)

Branch `m3/lagfix3` ab `main` `bc0bbc18`. Soll ausschließlich `docs/spec/*` (UI-018, UI-090, OUT-020, CRAWL-141);
keine Referenzprodukt-Sichtung, keine Abhängigkeitsänderung, kein Push. Server `openspider-build`, Worktree
`m3-b4flaky`. Die beiden Bridge-Tests, Zeitlimits und Budgets sind unverändert.

**Ergebnis:** Beide Bridge-Tests in WebKit und Chromium je `--repeat-each=20` ohne Rot (120/120), vorher auf demselben
Server Export in WebKit 10/10, 8/10 und 4/10 rot (drei Durchgänge), Viz 2/20 rot. Ursache ist keine neue Fehlfunktion in LAGFIX2,
sondern zwei schon vorhandene Lücken, die LAGFIX2 durch den früheren End-Tick sichtbar gemacht hat.

## Ursache (belegt mit vorübergehender Instrumentierung, wieder entfernt)

Messaufbau: `eprintln!`-Zeitstempel (Wanduhr) in der Bridge an jedem Schreibvorgang auf `crawl_meta.change_seq`
(Writer-Batch, Faktenaktualisierung und alle 14 übrigen Stellen in `os-store`, `os-analyze`, `os-engine`,
`os-integrations`), an jedem gesendeten Tick mit Crawl-Status, an `view.open`, `export.view` (Versionen und
`change_seq` im Schnappschuss), Start/Ende der automatischen Crawl-Analyse, `crawl.addUrls` und dem Viz-Wächter
(`viz-data-changed`: Vergleich und Zustellerfolg). `git diff` des Endstands enthält keine dieser Ausgaben.

### Export-Test (`e2e/export/bridge.spec.ts:11`)

1. Das Crawl-Ende startet in `control::settle` die automatische Crawl-Analyse (CRAWL-141, Schalter Standard an). Ihr
   Schritt Link Score committet ~150–250 ms später und erhöht `change_seq` (2 → 3).
2. Mit LAGFIX2 geht der End-Tick sofort hinaus (erster Tick eines frisch angehängten Crawls ist beschleunigt, der
   Poll ist fällig), also **vor** diesem Commit. Spur (WebKit, rot): `analysis start 46953`, `tick phase=Completed
   46984`, `view.open version=2 49862`, `link_score change_seq 49958`, `export args.version=2`, Schnappschuss
   `change_seq=3` → `view::stale()` → „The data is out of date.“
3. Vor LAGFIX2 kam der erste Tick an ein neu gesehenes Fenster erst nach einer vollen Intervallperiode (500 ms). Gegenprobe
   mit den Quellständen von `0aba2a4c` und derselben Instrumentierung: Analyse fertig nach 150–200 ms, End-Tick
   ~330–475 ms nach dem Start der Analyse, `view.open version=3`, 10/10 grün. Der Test lief also nur dank dieser
   Verzögerung grün.
4. Abschalten nur der End-Beschleunigung (`expedite_tick` bei `ended`) reicht nicht (8/10 rot): Der Poll des
   Crawl-Endes ist bei diesem kurzen Crawl ohnehin fällig.
5. Nach der ersten Korrektur (End-Tick wartet auf die Analyse) blieben Chromium 10/20 und WebKit 2/20 rot. Spur: Ein
   Tick `phase=Completed` ging **während** des Wartens hinaus. Sender war `session::merge::finished` (Ende der
   Hintergrund-Rückführung des WAL nach dem Crawl-Ende, CRAWL-126), das unabhängig von der Sitzungsschleife
   `flush_tick` aufruft – derselbe Fehlertyp wie der MCP-Taktgeber, den LAGFIX2 schon abgestellt hatte
   (`delivers_ticks`).

### Viz-Test (`e2e/viz/bridge.spec.ts:74`)

1. Der Test öffnet den Schnappschuss, ohne auf das Crawl-Ende zu warten (nur „5 Knoten“). Schreibt die automatische
   Analyse danach, meldet der Wächter des Schnappschusses „Daten geändert“ – **einmalig** (`notified = true`).
2. Das Viz-Fenster abonniert seine Ereignisse in `bootWindow` vor dem ersten Rendern; der Listener in `VizWindow`
   hängt erst nach `runtime.ready` und dem Rendern. Kommt die Meldung in dieser Lücke, ist `send_event` erfolgreich
   (Spur: `viz notice sent=true` 683 ms vor `crawl.addUrls`), der Hinweis erscheint aber nie, und weil der Kern nur
   einmal meldet, bleibt auch die Änderung durch `crawl.addUrls` stumm (Spur: `addUrls added=1`, danach keine
   Meldung an dieses Fenster) → Timeout nach 15 s.
3. LAGFIX2 hat die Lage zeitlich verschoben (gezählte URLs werden bei fälligem Poll sofort geschrieben, die Daten sind
   früher vollständig, der Schnappschuss liegt öfter vor dem Analyse-Commit). Die Lücke selbst besteht unabhängig
   davon und verletzt UI-090 („ändern sich danach die Daten, erscheint … innerhalb von 2 s der Hinweis“).
   `crawl.addUrls` auf einen beendeten Crawl selbst funktioniert (Schreibweg über die Warteschlange, neuer Engine-Lauf,
   Meldung an das Fenster) – in allen grünen Läufen belegt.

## Änderung

- `crates/os-app/src/session/mod.rs`: Der End-Tick einer von selbst beendeten Crawls wartet zusätzlich zur exakten
  Endzählung auf die automatische Crawl-Analyse, die das Ende gestartet hat – innerhalb **derselben** Frist
  `TICK_SETTLE_COUNTS` (3 s, keine neue Wartezeit). Ist sie fertig, veröffentlicht `poll_final` die von ihr geänderten
  Zeilen sofort (wie die Endzeilen, `ended`), und die View-Pflege wird innerhalb der Restfrist abgewartet; der Tick
  trägt dann Endstatus, Analysestatus „fertig“ und die neuen View-Versionen gemeinsam. Eine längere Analyse meldet sich
  im End-Tick als laufend und ihre Zeilen kommen mit einem späteren Tick (unverändert gegenüber vorher).
- `crates/os-app/src/session/analysis.rs`: `running(app)` (läuft eine Crawl-Analyse?).
- `crates/os-app/src/session/merge.rs`: `finished` sendet keinen eigenen Tick mehr, solange die Sitzungsschleife die
  Ticks liefert (`delivers_ticks`); der Status ohne Rückführungsvermerk geht mit dem nächsten fälligen Tick hinaus.
- `apps/desktop/src/features/viz/VizWindow.tsx`: Beim Einhängen werden die schon empfangenen Sitzungsmeldungen
  (`useSession.notices`) wie neue behandelt; nach „Aktualisieren“ wird die alte Meldung des Schnappschusses verworfen,
  damit sie beim erneuten Einhängen nicht wieder auftaucht.

Steuerbefehle (Pause/Stopp) quittieren weiterhin sofort; LAGFIX2-Eigenschaften (Zeilen nur im fälligen Poll, Tick mit
Summe desselben Polls, beschleunigter erster Tick, Endzählung) sind unverändert.

## Tests

- `crates/os-app/tests/b4_lagfix3.rs` (`// spec: UI-018`, `OUT-020`, `CRAWL-141`): echter Crawl (30 Seiten) mit
  automatischer Analyse, nur die laufende Sitzungsschleife liefert Ticks. Der erste Tick mit `Completed` meldet die
  Analyse als fertig; eine sofort danach geöffnete Ansicht hat die endgültige Version, `change_seq` ändert sich in den
  folgenden 1,5 s nicht, `export.view` mit dieser Version gelingt. Ohne die Korrektur 3/3 rot
  (`the end tick reports the automatic analysis as finished`, left `Some(true)`).
- `apps/desktop/src/features/viz/b4-lagfix3-notice.test.tsx` (`// spec: UI-090`, 3 Fälle): Meldung zwischen Abonnement
  und Rendern → Hinweis sichtbar; nach „Aktualisieren“ und erneutem Einhängen kein alter Hinweis; Meldung zu anderem
  Schnappschuss → kein Hinweis. Ohne die Korrektur 2/3 rot.

## Nachweis (Server, Endstand)

| Lauf | Ergebnis |
|---|---|
| Vorher (`bc0bbc18`), Export + Viz, WebKit, `--workers=2 --repeat-each=10`, zwei Durchgänge | Export 10/10, 8/10, 4/10 rot (je Durchgang); Viz 0/20, 2/20 rot |
| Gegenprobe Quellstand `0aba2a4c`, Export, WebKit, `--repeat-each=10` | 10/10 grün (End-Tick erst nach dem Analyse-Commit) |
| Zwischenstand nur mit Analyse-Warten (ohne `merge.rs`), Export + Viz `--repeat-each=20` | WebKit 2/60 rot, Chromium 10/60 rot (alle Export) |
| Endstand, `e2e/export/bridge.spec.ts` + `e2e/viz/bridge.spec.ts`, WebKit, `--workers=2 --retries=0 --repeat-each=20` | **60/60 grün** |
| Endstand, dasselbe in Chromium | **60/60 grün** |
| `e2e/alpha/detail-sidebar.spec.ts`, WebKit, `--workers=2 --retries=0 --repeat-each=10` | **130/130 grün** |
| `cargo test -p os-app --test b4_lagfix2 --test b4_lagfix3` | 5/5 und 1/1 grün |
| `cargo fmt --all --check` | grün |
| `cargo clippy --workspace --all-targets -- -D warnings` | grün |
| `cargo test --workspace --no-fail-fast` | 627 Testbinärdateien, 4585 bestanden, 0 fehlgeschlagen, 51 ignoriert |
| `npm run -w apps/desktop typecheck` | grün |
| `npm run -w apps/desktop test` | 133 Dateien, 1139 Tests grün |

Ein erster `cargo test --workspace` (ohne `--no-fail-fast`) hatte einen roten Lauf in
`stop_analysis_merge.rs::quit_runs_a_pending_view_cache_write_first_and_leaves_an_empty_wal` (Zeile 205: nach Stopp
und Rückführung noch 4152 Byte WAL; der Test läuft ohne automatische Analyse, berührt also den geänderten Pfad nur über
`merge::finished`, das keine Daten schreibt). Danach 15/15 Einzelwiederholungen und der volle Lauf grün: als
lastabhängiger Wackler gewertet, Ursache nicht untersucht (offener Punkt 5).

## Nicht geprüft

- Andere Playwright-Suiten (u. a. `e2e/acceptance/livedetail-*`) wurden nicht neu gemessen; der End-Tick kommt jetzt
  um die Analysedauer später (hier 150–300 ms, höchstens innerhalb der bestehenden 3-s-Frist).
- Windows/macOS-Läufe; nur Linux-Server.

## Offene Punkte

1. **Stopp per Befehl** (`crawl.stop`) quittiert sofort mit `Completed`, die Analyse startet danach (CRAWL-141). Eine
   unmittelbar danach geöffnete Ansicht kann durch den Analyse-Commit veraltet sein. Nicht Teil dieser Regression;
   ob der Export dann still den neuen Stand nehmen oder „veraltet“ melden soll, regelt die Spec nicht ausdrücklich.
2. Dauert die automatische Analyse länger als die Frist (große Crawls), geht der End-Tick mit „Analyse läuft“ hinaus,
   und Ansichten ändern danach noch einmal ihre Version (neue Daten der Analyse). Für Exportdialoge, die die Version
   des Endstatus festhalten, bleibt dann „Daten veraltet“ möglich – fachlich korrekt, aber ein Hinweis in der Spec
   (OUT-020/CRAWL-141) wäre hilfreich.
3. Enden zwei Crawls innerhalb desselben Wartefensters, wartet der End-Tick nur auf die Analyse des ersten (der zweite
   wird in `poll_final` abgeschlossen und ohne eigenes Warten gemeldet). Selten; nicht getestet.
4. Der Viz-Wächter meldet weiterhin nur einmal je Schnappschuss; geht eine Meldung trotz der Korrektur verloren (z. B.
   Fenster lädt neu), kommt keine zweite. Ein Abgleich beim Einhängen über einen Kern-Command wäre robuster.
5. Wackler `stop_analysis_merge.rs::quit_runs_a_pending_view_cache_write_first_and_leaves_an_empty_wal` (1 von 17
   Läufen, unter Volllast des Workspace-Tests); ob er auch auf `main` auftritt, ist ungeprüft.
