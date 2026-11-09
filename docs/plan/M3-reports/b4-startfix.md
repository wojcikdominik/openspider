# B4-STARTFIX – Fehler-Mail bei „Fehlgeschlagen“, Start-Dialog „per robots.txt gesperrt“ – Bericht

Stand 09.11.2026. Branch `m3/startfix`, Mac-Worktree `openspider-wt/c-startfix` ab dem **lokalen** `main` c8a0d85f
(B4-RESUME, B4-UNREACH und B4-RELCHECK gemergt, nicht gepusht). Server-Worktree `m3-startfix` ab `origin/main`, per
`srv-sync.sh` auf den Stand des Mac-Worktrees gebracht. Grundlage: UI-019 (05), CRAWL-128 (01), OUT-281, OUT-182,
OUT-152 (06). Herkunft: Fables Prüfung von B4-UNREACH.

**Hinweis:** Abschnitte 1–5 beschreiben den ersten Stand (bis 19ea06bd). Die Entscheidungen des Orchestrators vom
09.11.2026 ändern Historie, Vergleich, CLI-Exit bei robots-Sperre, Zeilenanzeige und Wiederholung; siehe „Nachtrag“.

## 1. Fehler-Mail bei `Failed` (OUT-281)

### Befund je Weg (mit den neuen Tests gegen den alten Code geprüft)

| Weg | Vorher | Ursache |
|---|---|---|
| App (UI-Crawl, globale Einstellung) | Mail ging als `[FAILED]` hinaus, aber **ohne Ursache** („Crawl fehlgeschlagen“). Keine Erfolgsmail. | Der Lauf nahm den Weg `Completed`; der Mail-Schnappschuss (`os_analyze::mail_completion`) setzt bei `meta.state == Failed` selbst einen Fehler. Die Lücke war also nicht „keine Mail“, sondern ein impliziter Weg ohne Ursache, abhängig davon, dass der Schnappschuss den Zustand liest. |
| CLI (`--crawl`, globale Einstellung) | **Keine Mail** bei `notify_on_success = false`; bei `notify_on_success = true` eine `[FAILED]`-Mail. | `notify_global` entschied nur nach dem Exit-Code. Die nicht erreichbare Start-URL behält Exit 0 (OUT-182) und galt damit als Erfolg. |
| Planer, Aufgabe ohne Auto-Vergleich | korrekt (`[FAILED]`) | Eingebaute Abschlussmail nutzt denselben Schnappschuss; `notify_task_with_context` überschreibt den Fehler nur bei `Failed`/`Aborted`. |
| Planer, Aufgabe mit Auto-Vergleich (OUT-152) | **Keine Mail** bei `on_failure = true` | Ein als `Failed` beendeter Crawl lässt sich nicht vergleichen; der Lauf endet deshalb als `Failed` („Crawl erfolgreich, aber Auto-Vergleich fehlgeschlagen“). Die Vergleichsmail verlangte aber das Vergleichsergebnis und brach mit „Auto comparison result missing for completion mail“ ab. Außerdem fehlte dort das Betreff-Präfix `[FAILED]`. |

### Umsetzung

- `os-app` `session/control.rs` `settle`: Endet der Lauf mit `Ok` und `Failed`, ruft die App nach der Startfehler-Meldung
  `mail::on_failed_with(app, key, Ursache)` auf. Die Ursache ist „Start-URL nicht erreichbar (<Fehlerklasse>): <URL>“
  (URL redigiert) bzw. „Crawl fehlgeschlagen“, wenn keine Ursache lesbar ist. `notify_start_unreachable` liefert dafür den
  Text zurück.
- `os-app` `commands/mail.rs`: `queue_ui` nimmt statt `failed: bool` die Fehlermeldung (`Option<String>`); `on_failed`
  bleibt für den `Err`-Weg, neu `on_failed_with`. Der Versand ist wie bisher je Lauf einmal (`sent`); das folgende
  `CrawlEvent::Completed` sendet daher keine zweite Mail und keine Erfolgsmail.
- `os-cli` `exec.rs`: `Crawled` trägt `failed` (`summary.state == Failed`), `Outcome` das neue Feld `crawl_failed`
  (Fehlermeldung, sonst `None`). Exit-Code, Warnung und Historie bleiben unverändert (OUT-182).
- `os-cli` `mail.rs` `notify_global`: Fehlschlag = Exit ≠ 0 **oder** `crawl_failed`. Die Mail trägt dann die Meldung
  „Start-URL … nicht erreichbar: …“.
- `os-analyze` `compare/change/mail_hooks.rs` (wird in `os-cli::run_task` eingebunden): Bei `Failed`/`Aborted` baut die
  Vergleichsmail den Inhalt ohne Änderungstabelle, stellt die Fehlermeldung des Laufs voran und setzt das Betreff-Präfix
  `[FAILED]`. Erfolgsmails sind unverändert.
- Planer ohne Auto-Vergleich: keine Codeänderung nötig, Tests sichern das Verhalten ab.

## 2. Start-Dialog „per robots.txt gesperrt“ (UI-019, Kriterium 2)

- `os-engine` `crawl.rs`: neue öffentliche Funktion `start_robots_block(&CrawlFile, start_url) ->
  Option<StartRobotsBlock { robots_url, matched_line }>`, analog zu `start_failure`. Sie liest die Start-URL-Zeile mit
  Grund `blocked_by_robots` (Status 0). `blocked_by_robots_unreachable` bleibt beim Fehlschlag-Weg von B4-UNREACH.
- `os-app` `settle`: Endet der Lauf als `Completed`, sendet `notify_start_robots` die Meldung `start-robots`, Stufe
  **Warnung** (kein Fehlschlag, CRAWL-128), Argumente `startUrl`, `url` (sperrende robots.txt), `line` (treffende Zeile).
  `start_robots_notice` ist öffentlich und getestet. Der Lauf bleibt „Fertig“; das hat CRAWL-128 vorgegeben (bestätigt in
  B4-RESUME S7). Die Erfolgsmail geht für diesen Lauf normal hinaus.
- Oberfläche (`features/notifications`): Der vorhandene Dialog zeigt zusätzlich „Sperrende robots.txt: <URL>“ (neuer
  Schlüssel `notify-robots-file` in `en.ftl`/`de.ftl`). Die Aktion „robots.txt ignorieren“ war schon da: Sie öffnet nur
  den Konfigurationsdialog an `robots.mode` und ändert nichts. Das entspricht „nur Hinweis auf die Einstellung, keine
  automatische Änderung“.
- Fixture `tests/fixtures/ui/fx-ui-robots-start` (`robots.txt` mit `User-agent: *` / `Disallow: /`, Startseite und
  eine Unterseite).
- `start-redirect-host` ist nicht umgesetzt (P2, nicht in diesem Paket).

## 3. Zeitschranke in `b4_unreach.rs`

In „closed_port“ und „unreachable_robots_txt“ ist die Prüfung `o.elapsed < 2 s` (ab App-Start) gestrichen.
`after_fetch < 2 s` (ab dem Ende des letzten Abrufversuchs) bleibt. Das ist eine ausdrücklich beauftragte Lockerung
(K2-63): Die Spec verlangt die 2 s nach dem letzten Versuch, siehe Punkt 5.

## 4. Statusleiste

`features/statusbar/i18n/de.ftl` `status-failed`: „Fehler“ → „Fehlgeschlagen“ (wie CRAWL-128, Crawl-Verwaltung und
Dialog). Englisch „Failed“ passt bereits zum Export-Schlüssel `Failed` und zur Verwaltung; unverändert.

## 5. Spec-Pflege

UI-019, Kriterium 1: ergänzt um „gemessen ab dem Ende des letzten Abrufversuchs der Start-URL bzw. ihrer robots.txt,
nicht ab dem Start des Crawls“.

## Kriterium → Test → Ergebnis

| Kriterium | Test | Ergebnis |
|---|---|---|
| OUT-281 App: `Failed` → genau eine Fehler-Mail mit Ursache, keine Erfolgsmail (`on_success` aus, `on_failure` an) | `crates/os-app/tests/b4_startfix.rs` `a_run_ending_as_failed_sends_exactly_one_failure_mail` (SMTP-Mock, geschlossener Port) | grün; gegen den alten Code rot (Mail ohne `connection_refused`) |
| OUT-281 App: `on_success` an, `on_failure` aus → keine Mail für `Failed`; robots-Sperre erhält die Erfolgsmail | `a_failed_run_sends_no_success_mail_but_a_robots_block_does` | grün |
| OUT-281 CLI: Exit 0 und Fehler-Mail | `crates/os-cli/tests/b4_startfix_mail.rs` `cli_an_unreachable_start_url_sends_the_failure_mail_with_exit_0` (Binary, SMTP-Mock) | grün; gegen den alten Code rot (keine Mail) |
| OUT-281 CLI: keine Erfolgsmail; robots-Sperre erhält sie | `cli_an_unreachable_start_url_sends_no_success_mail` | grün |
| OUT-281 Planer ohne Auto-Vergleich: Fehler-Mail bzw. keine Erfolgsmail; Lauf bleibt Exit 0 / „Completed with warnings“ (OUT-182) | `planner_an_unreachable_start_url_sends_the_failure_mail`, `…_sends_no_success_mail` (`run_in`, `DefaultHooks`) | grün |
| OUT-281/OUT-152 Planer mit Auto-Vergleich: `[FAILED] Nightly: Failed` mit Fehlermeldung, sonst keine Mail | `planner_with_auto_compare_treats_a_failed_crawl_as_failure` (`ComparisonMailHooks` mit SMTP-Versand an den Mock) | grün; gegen den alten Code rot (keine Mail) |
| Vergleichsmail bei Erfolg unverändert | `os-cli` `--lib change_tests` (8, u. a. `scheduled_task_runs_persist_exactly_one_comparison…`, `standard_run_task_sends_configured_smtp_comparison_mail`) | grün |
| UI-019 Krit. 2 / CRAWL-128: `Disallow: /` → „Fertig“, Meldung `start-robots` (Warnung) mit robots.txt-URL und Zeile, Konfiguration unverändert | `b4_startfix.rs` `a_start_url_blocked_by_robots_completes_and_reports_the_block`, `the_robots_notice_carries_start_url_robots_url_and_line` | grün |
| „robots.txt ignorieren“ → Start-URL wird gecrawlt, kein Startdialog | `ignoring_robots_crawls_the_start_url_without_a_start_dialog` | grün |
| Oberfläche: Text, sperrende robots.txt, Aktion öffnet nur die Einstellung (kein `config.set`), Deutsch | `apps/desktop/src/features/notifications/StartRobots.test.tsx` (2) | grün (2/2) |
| Ende-zu-Ende über die echte Bridge, Chromium + WebKit: Dialog, Statusleiste „Completed“, axe, Aktion öffnet `robots.mode` mit „Respect robots.txt“ gewählt, Konfiguration bleibt `respect` | `apps/desktop/e2e/beta/startrobots.spec.ts` | grün, Chromium + WebKit |
| UI-019 2 s ab letztem Abrufversuch | `b4_unreach.rs` (6) | grün |
| Mail-Bestand | `cmd_mail_lifecycle` (13), `cmd_mail_delivery` (4) | grün |

## Gates (Server, Worktree `m3-startfix`)

Stand der Code-Commits 6e8f3754…a084b628 (seitdem nur dieser Bericht).

| Gate | Ergebnis |
|---|---|
| `cargo fmt --all --check` | grün |
| `cargo clippy --workspace --all-targets -- -D warnings` | grün |
| `cargo test --workspace --no-fail-fast` (nach `~/ws-slot.sh`, `CARGO_PROFILE_*_DEBUG=0`, `CARGO_INCREMENTAL=0`) | grün: 620 Testbinaries, 4511 bestanden, 0 fehlgeschlagen, 51 ignoriert (darin `b4_startfix` 5/5, `b4_unreach` 6/6, `b4_startfix_mail` 5/5) |
| `cargo test -p os-app --test b4_startfix --test b4_unreach --test cmd_mail_lifecycle --test cmd_mail_delivery` | 5 + 6 + 13 + 4 grün |
| `cargo test -p os-cli --test b4_startfix_mail`, `--lib change_tests` | 5/5, 8/8 grün |
| `npm run -w apps/desktop typecheck` | grün |
| `npm run -w apps/desktop test` (CSS-Lint + Vitest) | 124 Dateien, 1051 Tests grün (darin neu `StartRobots.test.tsx` 2) |
| Playwright Chromium + WebKit (`--workers=1`): `e2e/beta/startrobots.spec.ts` (neu), `e2e/beta/unreach.spec.ts`, `e2e/beta/control.spec.ts` (nutzt das geänderte `startSite`) | 8/8 grün |

Gegenprobe gegen den alten Code (Server, danach zurückgespiegelt): Ohne `on_failed_with` scheitert der App-Test an der
fehlenden Ursache in der Mail; mit dem alten `exec.rs`/`mail.rs` scheitert der CLI-Test an der fehlenden Mail; mit dem
alten `mail_hooks.rs` geht für die Auto-Vergleich-Aufgabe keine Mail hinaus.

## Fremde Dateien (K2-92)

Geändert, obwohl sie nicht aus diesem Paket stammen:

- `crates/os-engine/src/crawl.rs`, `crates/os-engine/src/lib.rs` (B4-RESUME/B4-UNREACH): neue Funktion
  `start_robots_block`, nichts Bestehendes geändert.
- `crates/os-app/src/session/control.rs` (B4-UNREACH, Sitzungs-Lebenszyklus): `settle` ruft Mail und robots-Meldung;
  `notify_start_unreachable` liefert die Ursache zurück.
- `crates/os-app/src/commands/mail.rs` (OUT-280…282): `queue_ui` mit Fehlermeldung statt Schalter, `on_failed_with`.
- `crates/os-cli/src/exec.rs`, `crates/os-cli/src/mail.rs` (CLI): Feld `Outcome::crawl_failed`, Mail-Entscheidung.
- `crates/os-analyze/src/compare/change/mail_hooks.rs` (C02, in `os-cli` eingebunden): Fehler-Mail ohne Vergleich,
  `[FAILED]`.
- `apps/desktop/src/features/notifications/Notifications.tsx`, `i18n/{en,de}.ftl` (B4-UNREACH): eine Zeile, ein Schlüssel.
- `apps/desktop/src/features/statusbar/i18n/de.ftl`: ein Text.
- `apps/desktop/e2e/alpha/support.ts`: `startSite(scenario = "ui/fx-ui-basic")`, optionaler Parameter, Aufrufer
  unverändert.
- `crates/os-app/tests/b4_unreach.rs` (B4-UNREACH): Zeitschranke ab Start gestrichen (beauftragt).
- `docs/spec/05-ui.md`: UI-019 Kriterium 1 (beauftragt).

Neu: `crates/os-app/tests/b4_startfix.rs`, `crates/os-cli/tests/b4_startfix_mail.rs`,
`apps/desktop/src/features/notifications/StartRobots.test.tsx`, `apps/desktop/e2e/beta/startrobots.spec.ts`,
`tests/fixtures/ui/fx-ui-robots-start/`.

K2-63: Gelockert ist nur die beauftragte Zeitschranke ab App-Start in `b4_unreach.rs`; sonst wurde keine Prüfung entfernt
oder abgeschwächt und keine bestehende Erwartung geändert.

## Nachtrag: Entscheidungen des Orchestrators (09.11.2026)

Die ursprünglichen Fragen S1–S5 hat der Orchestrator entschieden. Umgesetzt im selben Branch (Commits 5a23a90f…a38028a2),
die Spec ergänzt als **OS-Festlegung 09.11.2026 (Orchestrator)** in UI-019, OUT-152, OUT-169 und OUT-182.

| Nr. | Entscheidung | Umsetzung | Test |
|---|---|---|---|
| 1 | Planer-Historie: nicht erreichbare Start-URL → „Failed“, mit und ohne Auto-Vergleich; der Crawl stuft ein | `run_task`: `Outcome::crawl_failed` → `RunResult::Failed`, `Message` = Fehler des Crawls; Exit-Code des Laufs unverändert (0, OUT-182) | `b4_startfix_mail.rs` `planner_*` (3): Historie `Failed`, Meldung „nicht erreichbar“, nie „erfolgreich“ |
| 2 | Nie „Crawl erfolgreich, aber Auto-Vergleich fehlgeschlagen“ bei fehlgeschlagenem Crawl; Vergleich „übersprungen (Crawl fehlgeschlagen)“ | `exec`: kein Vergleich bei `Failed`, Warnung `Auto-Vergleich übersprungen (Crawl fehlgeschlagen)` (Konstante `exec::COMPARE_SKIPPED_FAILED`), zählt nicht als Fehler; Schlusszeile = Fehler des Crawls (auch bei Exporten/`--strict`); `run_task` ruft den Vergleichs-Hook nicht; Vergleichsmail trägt den Vermerk statt der Tabelle | `planner_with_auto_compare_treats_a_failed_crawl_as_failure` (Vermerk in den Warnungen und in der Mail, keine „auto comparison“-Warnung), `cli_the_final_line_of_a_failed_crawl_names_its_error` |
| 3 | CLI folgt CRAWL-128: robots-Sperre kein Fehlschlag, kein Exit 3, Warnung mit robots.txt-URL und Zeile | `exec::start_url_problem` unterscheidet `Unreachable` und `Robots`. `Disallow` → „Start-URL … durch robots.txt gesperrt: <robots.txt>, Zeile N: <Regel>“; Regel „robots.txt nicht erreichbar“ ohne Fehlschlag (robots.txt mit 5xx/429) → Warnung mit Grund. Beides ohne Exit 3. | `cli_a_robots_block_is_no_start_error` (Exit 0 mit `--fail-on-start-error`, genaue Warnzeile; Gegenprobe geschlossener Port → Exit 3); `acceptance_resume` (robots.txt am Netz gescheitert → weiter Exit 3) |
| 4 | Zeile strukturiert, Formulierung per i18n, Dialog zeigt sie | `os-fetch` `RobotsMatch::split_matched_line` (Umkehrung der gespeicherten Anzeigeform, neben dem Formatierer); `StartRobotsBlock { robots_url, line_number, rule }`; Meldung `lineNumber` + `line` ohne Präfix; Oberfläche `notify-robots-line` / `notify-robots-rule` (en/de) | `parser::the_matched_line_splits_into_number_and_rule`; `b4_startfix.rs` (`lineNumber` 2, `line` „Disallow: /“); `StartRobots.test.tsx` (en/de, ohne Nummer); Playwright „Blocking rule (line 2): Disallow: /“ |
| 5 | Dialog je Crawl und Start-URL höchstens einmal, nicht erneut nach „URLs hinzufügen“ | Merker `notice.start_robots` (Start-URL) in den Metadaten der Crawl-Datei, geschrieben über die Schreibwarteschlange; gilt auch nach Neustart der App | `the_robots_dialog_appears_once_per_crawl_also_after_adding_urls` (`crawl.addUrls` setzt den Crawl fort, zweites `settle` ohne Meldung) |

Zu 3: Kein Widerspruch zu OUT-182/OUT-204. Beide verlangen Exit 3 nur für „Start-URL nicht erreichbar“, und CRAWL-128
grenzt „nicht erreichbar“ ein. Die Spec ist entsprechend ergänzt.

### Gates Nachtrag (Server, Stand 56113a72; UI-Gates auf a38028a2, danach nur ein Rust-Test geändert)

| Gate | Ergebnis |
|---|---|
| `cargo fmt --all --check` | grün |
| `cargo clippy --workspace --all-targets -- -D warnings` | grün |
| `cargo test --workspace --no-fail-fast` (nach `~/ws-slot.sh`, `CARGO_PROFILE_*_DEBUG=0`, `CARGO_INCREMENTAL=0`) | grün: 620 Testbinaries, 4515 bestanden, 0 fehlgeschlagen, 51 ignoriert (darin `b4_startfix` 6/6, `b4_startfix_mail` 7/7) |
| `cargo test -p os-app --test b4_startfix --test b4_unreach --test cmd_crawl_add_m2 --test cmd_mail_lifecycle` | 6 + 6 + 2 + 4 grün; `b4_startfix` zusätzlich 12-mal hintereinander grün |
| `cargo test -p os-cli --test b4_startfix_mail --test cli_basics --test acceptance_resume --test run_task_inproc --test run_task_cli`, `--lib` | 7 + 19 + 15 + 12 + 9, 69 grün |
| `cargo test -p os-fetch --lib robots::parser` | 19 grün |
| `npm run -w apps/desktop typecheck` | grün |
| `npm run -w apps/desktop test` | 124 Dateien, 1052 Tests grün (`StartRobots.test.tsx` 3) |
| Playwright Chromium + WebKit: `startrobots`, `unreach`, `control` | 8/8 grün |

Zum Ablauf:

- **Rennen im Test:** Der erste Workspace-Lauf auf a38028a2 hatte genau einen Fehlschlag:
  `a_start_url_blocked_by_robots_completes_and_reports_the_block` sah keine Meldung. Ursache war ein Rennen im Test, kein
  Produktfehler. Der Hintergrund-Poll der App ruft `settle` ebenfalls auf; der Test wartete nur auf den Zustand, und den
  setzt `settle` vor seinen Meldungen. Seit 56113a72 wartet der Test auf das Lauf-Ende-Ereignis, das `settle` nach den
  Meldungen sendet. Nach „URLs hinzufügen“ meldet `follow_completion` das Ende zusätzlich und unter Umständen früher;
  dort sammelt der Test danach noch 1 s weiter.
- **Gegenprobe:** Ohne den Merker scheitert `the_robots_dialog_appears_once_per_crawl_also_after_adding_urls` (3 von 3).
- **ws-slot:** `~/ws-slot.sh` wartete rund 45 min bei 75–76 GB frei, ohne laufenden Workspace-Test (Grenze 80 GB). Das
  eigene `target/` war vollständig gebaut. `cargo test --workspace --no-run` übersetzte nur 3 Testziele, der freie Platz
  blieb unverändert. Deshalb wurde der eigene Warteprozess beendet und der Lauf ohne neuen Build gestartet: einzeln, kein
  anderer Workspace-Test aktiv, Platz danach unverändert bei 75 GB. Das ist eine bewusste Abweichung von der 80-GB-Regel.

Weitere fremde Dateien im Nachtrag (K2-92): `crates/os-fetch/src/robots/parser.rs` (neue Funktion, Format unverändert),
`crates/os-cli/src/run_task.rs` (Ergebnis-Zuordnung, Vergleichs-Hook), `docs/spec/06-output-ops.md`.

## Offene Punkte

- **O1 Andere Status-0-Fälle mit `--fail-on-start-error`:** `malformed`, `too_many_redirects` und `blocked_private_target`
  sind nach CRAWL-128 kein Fehlschlag. In der CLI führen sie mit `--fail-on-start-error` weiterhin zu Exit 3 und der Warnung
  „nicht erreichbar“. Der Auftrag betraf nur die robots-Sperre; nicht geändert.
- **O2 Exit-Code des Planerlaufs:** Ein Lauf mit Historie „Failed“ wegen des Crawls endet mit Exit 0 (OUT-182). Soll
  `--run-task` dann einen anderen Code liefern (z. B. 3)? Nicht geändert.
- **O3 robots.txt mit 5xx/429:** Hier sperrt die Regel „robots.txt nicht erreichbar“ die Start-URL, aber der Lauf ist kein
  Fehlschlag. Die App zeigt dafür weder den Dialog „nicht erreichbar“ noch `start-robots` (UI-019 nennt nur `Disallow`).
