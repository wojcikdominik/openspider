# M3 – Nightly `ui-e2e-b.yml` wieder grün

Stand: 09.11.2026 · Branch `m3/nightly-b` ab `origin/main` (`bb0b27e1`) · Server-Worktree `m3-nightly-b`
Ergebnis: PAKET FERTIG (Nachweis auf macOS/Windows erst nach dem Merge per `workflow_dispatch`, siehe unten)

## Befund

Seit 21.10.2026 waren 28 von 29 Läufen rot, immer im Schritt „Native check“. Ausgewertet wurden alle
52 Jobs seit 20.10. (Logs per `gh api …/actions/jobs/<id>/logs`, Artefakte `s02-smoke-*` per `gh run download`).
Es gibt **zwei unabhängige Fehler**, je einen pro Plattform:

| Plattform | Fehler in jedem Nightly seit 22.10. | Letzter Beleg |
|---|---|---|
| macOS-15 | `closing during a running fixture crawl asks before quitting: AssertionError … true !== false` | Job 128686304415 (Lauf 42466238058, 08.11.) |
| Windows | `Error: Failed to create session. session not created: DevToolsActivePort file doesn't exist` | Job 128686304289 (Lauf 42466238058, 08.11.) |

Der einzige grüne Lauf seit 21.10. ist 39654811661 (`workflow_dispatch`, nur Windows, Commit `e0091b1d`).
macOS war außerdem in zwei Läufen vom 21.10. grün (Jobs 119353174520 auf `f3aaad4e` und 119399648382 auf
`01990a8d`), deren Gesamtergebnis wegen Windows rot war.

### Ursache 1 (macOS, gilt für beide Plattformen): Prüfreihenfolge passt nicht zum Produkt

`app.requestQuit` sendet bei einer `confirm`-Entscheidung die Notice `app-quit-confirmation`
(`crates/os-app/src/runtime/mod.rs`, `App::request_quit`), und `QuitDialog.tsx` öffnet darauf den Dialog
„Quit OpenSpider?“. Das ist so gewollt: Native Schließen, Menü und `requestQuit` laufen über denselben
Kerneinstieg (F-11; ebenso `apps/desktop/e2e/beta/quit.spec.ts`, das genau diesen Dialog nach
`app.requestQuit()` erwartet). Die Smoke-Prüfung rief aber zuerst `requestQuit` auf und verlangte danach,
dass **kein** Dialog offen ist, bevor sie das Fenster schließt. Beleg aus `report.json` vom 08.11.:

```
app.requestQuit confirms a running fixture crawl          passed {"kind":"confirm","reasons":["crawl"]}
closing during a running fixture crawl asks before quitting failed "…true !== false"
```

`true !== false` ist die Zeile `assert.equal(<Dialog offen?>, false)`, und `window.png` zeigt den offenen
Beenden-Dialog. Im grünen macOS-Job 119399648382 (`01990a8d`) steht die Reihenfolge umgekehrt
(erst Schließen, dann `requestQuit`), und beide Prüfungen bestehen.

### Ursache 2 (Windows): WebView2 bekommt keinen Debug-Port

msedgedriver gibt WebView2 `--remote-debugging-port=0` über `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS` mit.
wry setzt aber eigene Zusatzargumente über die API. Die Umgebungsvariable wirkt deshalb nicht, und es entsteht
keine `DevToolsActivePort`-Datei. Am 21.10. wurde das in acht Diagnoseläufen eingegrenzt (Kommandozeilen von App
und WebView2 im Bericht, Registry-Richtlinie wirkungslos). Gelöst wurde es mit einem **nur im Testbuild** gesetzten
`TAURI_CONFIG`: `additionalBrowserArgs` mit Debug-Port für beide Fenster. Weil WebViews mit demselben
Datenverzeichnis identische Argumente brauchen, wird das Einstellungsfenster dort versteckt beim Start angelegt.
Grüner Beleg: Job 119412158688 (`e0091b1d`), alle fünf Prüfungen bestanden, Session beim ersten Versuch.

### Warum es wieder rot wurde

Beide Korrekturen (`f3aaad4e` … `e0091b1d`, 21.10. abends) lagen auf `main`. Commit `d9d24f1d`
(„app: fix M2-N01b review findings“, 22.10. 01:10) hat sie wieder entfernt
(„withdraw unrelated native E2E and timing changes“): `run.mjs`, `README.md` und `ui-e2e-b.yml` wurden auf den
Stand vor dem 21.10. zurückgesetzt. Danach kamen sie nicht wieder hinein. Der erste Nightly danach
(22.10., `f137e743`) zeigt wieder genau die beiden alten Fehler. Die U32-Erweiterung vom 30.10.
(`09e9cd60`, `569ea631`) baute auf diesem zurückgesetzten Stand auf.

### Einordnung

- macOS: **Testfehler.** Das Produkt verhält sich wie spezifiziert, die Prüfung nahm an, `requestQuit` frage
  nur ab, ohne den Dialog zu öffnen.
- Windows: **Umgebungsfehler** im Zusammenspiel von WebDriver, WebView2 und wry. Das ist kein Produktfehler,
  die Produktkonfiguration bleibt unverändert.
- Ursache der Dauer: Die Korrekturen gingen bei einem Merge verloren (Prozessfehler).

Eine Produktänderung ist nicht nötig.

## Änderung (Commit `815b7c70`)

- `apps/desktop/e2e-b/run.mjs`
  - Zuerst läuft „closing during a running fixture crawl asks before quitting“: Crawl starten, falls noch keiner
    läuft, dann prüfen, dass kein Dialog offen ist, das Fenster schließen und auf den Dialog warten.
    Danach folgt „app.requestQuit confirms a running fixture crawl“ (`kind === "confirm"`,
    `reasons` enthält `crawl`) auf demselben Crawl. Im Modus `u32` läuft der Abbruch-Check unverändert danach.
  - Windows: Der Runner startet `msedgedriver` selbst (`--verbose`, `msedgedriver.log` als Evidenz) mit den
    Capabilities, die `tauri-driver` bauen würde. `S02_WINDOWS_DRIVER=tauri` schaltet zurück auf `tauri-driver`.
    Es gibt **keine** Wiederholung. Schlägt die Session fehl, werden die Kommandozeilen von App und WebView2 in
    `report.processes` festgehalten.
  - Einstellungsfenster: Die Prüfung findet das Fenster über das Label `settings`, weil es im Windows-Testbuild
    schon versteckt existiert. Außerhalb von Windows gilt neu `assert.ok(!before.includes(settings))`: Das
    Fenster muss dort weiterhin ein neues Fenster sein. Die Prüfung ist damit so streng wie vorher.
- `.github/workflows/ui-e2e-b.yml`
  - Der Schritt „Build Windows app“ setzt `TAURI_CONFIG` (`additionalBrowserArgs` mit wry-Standard
    `--disable-features=…` und `--remote-debugging-port=0` für beide Fenster; Einstellungsfenster mit
    `url: settings.html` statt `create: false`). `cargo install tauri-driver` entfällt.
  - Die `workflow_dispatch`-Eingaben `mode` und `platforms` sind wieder da, damit die Prüfung nach dem Merge
    gezielt gestartet werden kann.
- `apps/desktop/e2e-b/README.md`: Windows-Ablauf mit Begründung und Hinweis zur Prüfreihenfolge beim Beenden.

Zu K2-63: Keine Assertion wurde entfernt oder gelockert, und es gibt kein `continue-on-error`. Weggefallen ist nur
die Vorbedingung `report.quitDecision?.kind === "confirm"` im Schließen-Check, weil dieser jetzt zuerst läuft.
Ersetzt wird sie durch `startFixture()`, das selbst Fixture-Anfragen und `running`-Ticks verlangt. Die
`confirm`-Assertion steht unverändert im folgenden `requestQuit`-Check. Den Diagnose-Probe (20 s) und die
einmalige Session-Wiederholung aus der Fassung vom 21.10. habe ich bewusst **nicht** übernommen.

## Nachweis

Auf dem Server (Linux, Worktree `m3-nightly-b`; der Native-Check selbst läuft nur auf macOS/Windows):

- `node --check apps/desktop/e2e-b/run.mjs`: ok
- `node --test apps/desktop/e2e-b/{measurement,readiness,focus}.test.mjs`: 11/11 bestanden
- `npx tsc -p apps/desktop/e2e-b/tsconfig.u32.json`, `npx tsc -p apps/desktop/spikes/tsconfig.json`: ok
- `actionlint` 1.7.12 (`~/b3rel-tools/actionlint12/actionlint`) für `ui-e2e-b.yml` und `spike-platform.yml`: ok
- `npm run -w apps/desktop typecheck`: ok; `npm run -w apps/desktop test`: 122 Dateien, 1037 Tests bestanden
- `cargo fmt --all --check`: ok. `cargo clippy`/`cargo test --workspace` habe ich nicht ausgeführt: Es ist kein
  Rust-Code geändert, und ein Workspace-Lauf belegt etwa 120 GB bei 32 GB frei.
- Gleichheit zum grünen Windows-Stand: `tauri` 2.10.2 / `tauri-utils` 2.12.3 / `wry` 0.57.0 sind in
  `Cargo.lock` unverändert seit `e0091b1d`, und der Abschnitt `app` (Fenster) von `tauri.conf.json` ist
  identisch. Das `TAURI_CONFIG`-Override trifft also dieselbe Konfiguration.

## Prüfung nach dem Merge (Orchestrator)

Die Eingaben von `workflow_dispatch` gibt es erst, wenn dieser Stand auf `main` liegt:

```sh
cd ~/Projects/openspider
gh workflow run ui-e2e-b.yml --ref main -f mode=smoke -f platforms='["macos-15", "windows-latest"]'
gh run list --workflow ui-e2e-b.yml --limit 1      # Run-ID
gh run watch <run-id> --exit-status
```

Erwartet wird auf beiden Plattformen `status: passed` mit den Checks main window, active native desktop,
separate settings window, closing …, app.requestQuit …. Auf Windows liegt `msedgedriver.log` im Artefakt
`s02-smoke-windows-latest`.

## Offene Punkte

1. **Nicht in CI beobachtet:** Seit `e0091b1d` wartet das native Schließen zuerst auf das Speichern des
   Frontends (`flush_frontend`, bis zu 3 s) und erst dann auf `request_quit`. Der Schließen-Check nach dem
   Fenster-Schließen ist seit dem 22.10. nie gelaufen, weil der Lauf vorher abbrach. Der Listener
   `os:flush-ui-state` ist in `persistence.ts` vorhanden, und das Warten auf den Dialog hat 30 s Zeit. Bestätigen
   kann das erst der Lauf oben.
2. **U32 auf Windows:** „U32 multiple windows: singleton settings and independent close“ (`u32-cases.mjs`)
   erwartet nach `settings()` einen neuen Handle (`before.length + 1`). Im Windows-Testbuild existiert das
   Einstellungsfenster aber schon. Auf Windows ist dieser U32-Check nie gelaufen, weil die Session vorher immer
   scheiterte. Nightly und `smoke` betrifft das nicht. Für einen Windows-U32-Lauf muss entschieden werden, ob die
   Prüfung dort das Sichtbarwerden des vorhandenen Fensters prüft. Das habe ich nicht geändert, weil es nicht zum
   Auftrag gehört.
3. Damit Korrekturen nicht erneut verloren gehen: Paket-Reviews, die „unrelated changes“ zurückziehen, sollten
   prüfen, ob diese schon auf `main` liegen. `d9d24f1d` hat Stände von `main` überschrieben, statt nur eigene
   Änderungen zurückzunehmen.
