# B4-STARTFIX – Fehler-Mail bei „Fehlgeschlagen“, Start-Dialog „per robots.txt gesperrt“ – Bericht

Stand 09.11.2026. Branch `m3/startfix`, Mac-Worktree `openspider-wt/c-startfix` ab dem **lokalen** `main` c8a0d85f
(B4-RESUME, B4-UNREACH und B4-RELCHECK gemergt, nicht gepusht). Server-Worktree `m3-startfix` ab `origin/main`, per
`srv-sync.sh` auf den Stand des Mac-Worktrees gebracht. Grundlage: UI-019 (05), CRAWL-128 (01), OUT-281, OUT-182,
OUT-152 (06). Herkunft: Fables Prüfung von B4-UNREACH.

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

## Spec-Fragen und Befunde

- **S1 Historie des Planers:** Ein Aufgabenlauf mit nicht erreichbarer Start-URL steht ohne Auto-Vergleich als
  „Completed with warnings“ (Exit 0, OUT-182) in der Historie, mit Auto-Vergleich als „Failed“, weil der Vergleich einen
  erfolgreich abgeschlossenen Crawl verlangt. Die Mail ist jetzt in beiden Fällen eine Fehler-Mail. Soll die Historie
  einheitlich „Failed“ zeigen (CRAWL-128) oder bleibt es bei OUT-182? Nicht geändert.
- **S2 Meldung beim Auto-Vergleich:** Der Lauf meldet dann „Crawl erfolgreich, aber Auto-Vergleich fehlgeschlagen“, obwohl
  der Crawl als „Fehlgeschlagen“ endete. Der Text ist irreführend; nicht geändert (CLI-Ergebnistexte, nicht dieses Paket).
- **S3 CLI-Warnung bei robots-Sperre:** `start_url_problem` (CLI) wertet jede Start-URL-Zeile mit Status 0 als „nicht
  erreichbar“, also auch die Sperre per `Disallow` („Blocked by Robots.txt“). Mit `--fail-on-start-error` endet ein
  gesperrter Start deshalb mit Exit 3, obwohl CRAWL-128 die Sperre nicht als Fehlschlag zählt. Nicht geändert; Klärung zu
  OUT-182/OUT-204 nötig.
- **S4 Treffende Zeile:** Der Kern liefert die Zeile als „Zeile N: Disallow: /“ (deutscher Präfix aus `os-fetch`). Die
  Meldung trägt sie als `line`, der Dialog zeigt sie nicht an, um keinen deutschen Text in der englischen Oberfläche zu
  zeigen; er zeigt die sperrende robots.txt-URL.
- **S5 Wiederholung:** Die robots-Meldung erscheint bei jedem `settle` eines als „Fertig“ endenden Laufs mit gesperrter
  Start-URL, also auch nach „URLs hinzufügen“ auf einem solchen Crawl. Die Spec sagt dazu nichts.
