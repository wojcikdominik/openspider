# Abnahme fix/maintenance-teardown (unabhängig)

Geprüft wurde der Zweig `fix/maintenance-teardown` (`78c42aea` über main `c7d9c424`). Der Produktcode wurde durchgesehen. Den Test des Bauers und seine Commit-Nachrichten habe ich nicht gelesen. Die Abnahmetests liegen auf `fix/maintenance-teardown-tests`.

## A. Durchsicht

**A1 – Hält eine Hintergrundaufgabe eine starke `App`?** Urteil: **über keinem `await`, aber über blockierenden Aufrufen ja (B2).**
- `retention_round` (`crates/os-app/src/commands/crawls.rs:649–685`): Die `App` aus `app()?` in Z. 650 ist ein Temporärwert und fällt am Ende der Anweisung. Über dem Warten auf Gate, Registersperre (Z. 651–652) und `pending.await` (Z. 685) liegt keine `App`. Das gilt auch für `sleep(3600)` (Z. 702).
- Im synchronen Abschnitt Z. 654–684 hält die Runde aber eine hochgestufte `App`. Sie liest dort Einstellungen, Sitzungen und die Uhr (Z. 677) und reiht den Auftrag ein (Z. 678). Lässt der Besitzer seine `App` genau in diesem Fenster fallen, ist sein Abbau nicht der letzte. Der Abbau der Schreibwarteschlange läuft dann erst danach in einem Tokio-Worker. Der eben eingereihte Auftrag kann also noch starten, wenn der Besitzer sein Verzeichnis schon löscht. Das ist das ursprüngliche Fehlerbild, nur mit einem Fenster von Mikrosekunden statt einer ganzen Runde.
  - Belegt mit dem roten Test `acceptance_maintenance_teardown_race`. Eine Uhr hält die Startrunde in `retention_round` an. Danach liefert `weak.upgrade()` nach dem Abbau durch den Besitzer noch eine `App`.
  - Dasselbe Muster findet sich in den Fehlerzweigen Z. 697 und Z. 722 (`weak.upgrade().is_some()`). Außerhalb des Pakets liegt es in `session/mod.rs:105` (alle 100 ms) und `mcp/mod.rs:131/166`.
- Endet die stündliche Schleife? Ja. `retention_round` liefert `None`, sobald die schwache Referenz nicht mehr hochstuft, und die Schleife bricht ab (Z. 696). Zusätzlich bricht `ManagementState::drop` die Aufgabe ab (Z. 44–55). Weil die Runde `ManagementState` über ihr `await` hält, geschieht dieser Abbruch erst nach dem Ende der Runde, und zwar in der Aufgabe selbst (beim nächsten `sleep`). Einen Pfad, auf dem die Schleife ewig läuft, habe ich nicht gefunden.
- `tasks::on_startup` (`commands/tasks.rs:1322–1345`) hält keine `App` mehr, sondern nur Pfade, Uhr, `Arc<TasksState>` und den Basisordner. In Ordnung.

**A2 – Reihenfolge, Sperren, Deadlock?** Urteil: **kein Deadlock gefunden.**
- Die Sperrreihenfolge ist Gate → Register (Z. 651–652), wie bei den Verwaltungsaufträgen (`management_guard` → `admin`). Es gibt keine Umkehrung.
- `WriteQueue::shutdown` (`write_queue.rs:473–483`) schließt erst die Hauptspur, dann die Wartungsspur. `Lane::shutdown` (Z. 175–200) setzt `closed`, nimmt Sender und Handle und wartet höchstens 5 s. Aus dem Thread der Spur selbst kehrt es sofort zurück (Z. 187).
- Getestet sind zwei Fälle, beide ohne Hänger: Abbau aus dem laufenden Wartungsauftrag und letzter `App`-Drop im Wartungsauftrag.
- Abbau in einem Tokio-Worker: Der Worker blockiert höchstens 5 s plus die Hauptspur. Das ist kein Deadlock, weil die Aufträge synchron sind und nicht auf Tokio warten.
- Während einer wartenden `run_retention` lebt die `App`, weil der Aufrufer sie hält. Ein ausdrückliches `write_queue().shutdown()` führt dort zu „Warteschlange geschlossen“ und gibt die Sperren frei.
- Ein verworfener Auftrag bekommt `ErrorCode::Internal` mit dem Detail „Auftrag verworfen“ (`write_queue.rs:407`). Danach eingereichte Aufträge bekommen „Warteschlange geschlossen“ (Z. 403). Im Hintergrund wird der Fehler nur protokolliert, solange die App noch lebt (`crawls.rs:697`, `722`).

**A3 – Ist OUT-229 unverändert?** Urteil: **für die Löschregeln ja; der Nebeneffekt „Speicherort vermerken“ fehlt bei „Nie“ (siehe A4/B1).**
- Die Runden laufen weiterhin beim Start (`start_retention`), stündlich und nach jeder Änderung von Aufbewahrung oder Speicher (`on_settings_changed`, `update_retention_settings` Z. 30–40).
- Die Fristenzuordnung (Z. 663–671) und `retain_with` (os-store, unverändert) sind gleich geblieben.
- Geöffnete, laufende und gesperrte Crawls bleiben geschützt.
- Der frühe Ausstieg in Z. 656 gilt nur für `Never` ohne `keep_last` und ist für das Löschen ergebnisgleich, weil mit `days=None` und `excess=0` nichts gelöscht würde.
- „Höchstens N“ bei „Nie“ führt weiterhin eine Runde aus, beim Start und nach einer Änderung (beides getestet).
- Ein Wechsel von „Nie“ auf eine Frist löst sofort eine Runde aus. Register und Index entstehen dabei, der fällige Crawl wird gelöscht (getestet).

**A4 – Spätes Anlegen von Register und Index.** Urteil: **eine Regression (B1), sonst unkritisch.**
- **B1:** Bisher vermerkte jede Runde, auch bei „Nie“, über `maintenance` → `add_location(destination)` den aktuellen Speicherort im Register (`crawls.rs:101`). Nach der Korrektur fehlt das, solange keine Verwaltungsaktion läuft. Ablauf:
  1. Der Nutzer stellt den Speicherort auf A.
  2. Er crawlt, ohne die Crawl-Verwaltung zu öffnen.
  3. Er stellt auf B um.
  4. Ergebnis: Die Crawl-Verwaltung zeigt die Crawls in A nicht mehr.

  Das verletzt CRAWL-124 (`docs/spec/01-crawl.md:1773`): „die Crawl-Verwaltung findet Crawls an beiden Orten bis zur Verschiebung“. Betroffen ist die Standardrichtlinie, also die meisten Installationen. Die Dateien selbst gehen nicht verloren. Belegt mit dem roten Test `crawls_at_the_previous_location_stay_listed_after_a_location_change_with_policy_never`.

  Die Regression ist aus dem Code abgeleitet. Ich habe den Test nicht gegen `c7d9c424` laufen lassen, weil der geteilte Bauordner sonst neu gebaut würde.
- Nicht betroffen sind: der Start eines Crawls (`CrawlFile::create` legt Ordner an), das Öffnen per Kennung (`registry::find` hat einen Rückfall ohne Register), MCP und CLI (`list_crawls_report` liest ohne Register roh) und Vergleiche (`list_crawls`).
- **B3:** CLI und MCP listen ohne Register leicht anders: keine Zusammenfassung doppelter Kennungen, kein Indexcache. Das betrifft eine neue Installation, bis zur ersten Verwaltungsaktion.
- Update von 1.0.x: Die Dateien sind vorhanden, `Manager::open` arbeitet mit `CREATE TABLE IF NOT EXISTS`. Ich habe keine Formatmigration gefunden, die beim Start laufen müsste. Index und Register werden nun erst bei der ersten Crawl-Liste geöffnet und aufgefrischt statt im Hintergrund beim Start. Die erste Liste kann bei vielen geänderten Dateien etwas länger brauchen (B3).
- Auch der Crawl-Ordner `paths.crawls` selbst entsteht bei „Nie“ nicht mehr beim Start (`MetadataIndex::open` → `create_dir_all`). Ich habe keinen Verbraucher gefunden, der das voraussetzt.

**A5 – OUT-166.** Urteil: **ergebnisgleich.**
- `planner_in` ist identisch mit dem früheren `planner_of`, gibt aber `settings` als Parameter weiter (`tasks.rs:158–174`). `planner_home` wird wie vorher aus `home_override` bestimmt (Z. 707–712).
- `HistoryStore` und `TaskStore` zeigen auf denselben Einstellungsordner.
- Getestet: Varianten `dry` und `off` (über `OPENSPIDER_PLANNER`) und ein eingesetzter Launchd-Dry-Planer, jeweils mit `home_override` und sofortigem Abbau.
- **B3:** Der Abgleich läuft nach dem Abbau der App losgelöst weiter. Er kann JSON-Dateien bzw. Ordner in ein schon gelöschtes Verzeichnis schreiben. Das gab es schon vorher; SQLite ist hier nicht beteiligt.

**A6 – 5-s-Obergrenze.** Urteil: **für Tests und die Desktop-App vertretbar, aber keine Garantie (B3).**
- Läuft eine Runde länger als 5 s, bleibt der Thread losgelöst und schreibt weiter. Löscht der Besitzer dann sein Verzeichnis, besteht das ursprüngliche Risiko wieder.
- Realistische lange Runden:
  - viele Crawls, weil `list()` die Metadaten jeder geänderten Datei liest;
  - langsame externe oder Netzwerk-Speicherorte;
  - große Crawl-Dateien (Checkpoint vor dem Löschen);
  - eine Index- oder Crawl-Datei, die ein anderer Prozess (CLI, `--run-task`) sperrt, mit jeweils bis zu `BUSY_TIMEOUT` = 5 s.
- Die Desktop-App baut die `App` beim Beenden nicht ab (`quit::drain`), dort spielt die Grenze also keine Rolle. In Tests sind die Runden klein.
- Bei einer Überschreitung wird gewarnt (`write_queue.rs:195`). Die Begründung ist schlüssig, sofern man Einbettungen mit langen Runden ausschließt.

## B. Abnahmetests

Es gibt vier Dateien `crates/os-app/tests/acceptance_maintenance_teardown*.rs`. Jeder Test trägt `// spec: OUT-229`, `OUT-166` bzw. `CRAWL-124` und `// acceptance: independent`.

| Datei / Test | Auftrag | Ergebnis |
|---|---|---|
| `acceptance_maintenance_teardown_cycles` · `dropping_the_last_app_during_a_running_round_leaves_no_writer_in_the_directory` (40 Zyklen, Runde am Haltepunkt, Abbau, Prüfung: alle Löschungen fertig, kein losgelöster Thread, keine `-wal`/`-shm` von Index/Register, Verzeichnis sofort löschbar) | i | grün |
| `default_policy_creates_neither_registry_nor_index_before_the_first_management_action` (pausierte Zeit, +1 h) | ii | grün |
| `hourly_round_deletes_a_crawl_that_became_due_after_the_start` (6 → 8 Tage, +1 h) | ii/OUT-229 | grün |
| `switching_from_never_to_one_week_runs_a_round_at_once_and_spares_protected_crawls` | iii | grün |
| `keep_last_with_policy_never_deletes_the_oldest_beyond_n_at_start` | iv | grün |
| `setting_keep_last_with_policy_never_runs_a_round_that_deletes_the_oldest_beyond_n` | iv | grün |
| `startup_reconcile_completes_when_the_app_is_dropped_right_after_it_started` (eingesetzter Planer, `home_override`) | v | grün |
| `acceptance_maintenance_teardown_planner_env` · `startup_reconcile_with_planner_dry_and_off_survives_an_immediate_drop` | v | grün (Lauf 2) |
| `a_discarded_maintenance_job_reports_an_error_instead_of_hanging` | vi | grün |
| `dropping_the_last_app_inside_a_maintenance_job_does_not_hang` | vi/A2 | grün |
| `crawls_at_the_previous_location_stay_listed_after_a_location_change_with_policy_never` | A4 | **rot (B1)** |
| `acceptance_maintenance_teardown_race` · `owner_drop_is_the_last_reference_even_while_a_round_reads_its_settings` | A1 | **rot (B2)** |

Bestehende Nachbartests (Lauf 1): `n31_retention_lane` 1/1, `cmd_crawls_management` 8/8, `cmd_tasks_history` 5/5, `write_queue_serial` 6/6.

## C. Wiederholung (Mac mini, Lauf 1, 27.11. 00:31–00:35)

- `update_channels`: 20/20 grün, je 11 Tests.
- `multi_export_commands`: 20/20 grün, je 5 Tests.
- Neue `*.ips` in `~/Library/Logs/DiagnosticReports` seit Laufbeginn: **keine**.

## Läufe

- **Lauf 1** (Prüfzeile `8d62522f`):
  - fmt grün.
  - clippy rot wegen eines ungenutzten Imports in meinem Test `_cycles`. In `2ff2c955` behoben.
  - `_planner_env` rot wegen einer falschen Annahme meines Tests (neue Aufgaben werden erst mit `tasks.register` eingetragen). In `2ff2c955` behoben.
- **Lauf 2** (Prüfzeile `2ff2c955`): fmt grün, clippy (`-p os-app --all-targets -D warnings`) grün; `_planner_env` 1/1 grün, `_cycles` 1/1 grün (erneut); keine neuen `*.ips`.

## Befunde

- **B1** – Speicherortwechsel bei Standardrichtlinie: Crawls am vorherigen Ort fehlen in der Crawl-Verwaltung (CRAWL-124). Ursache ist der frühe Ausstieg in `crawls.rs:656` vor `add_location`. Vorschlag: Den Speicherort beim Ändern von `storage.crawls_dir` auf der Hauptspur vermerken. Dort hält der Aufrufer die `App`. Vermerkt werden sollte der alte und der neue Ort, oder der aktuelle Ort beim Start, aber nicht mehr über die Hintergrundrunde.
- **B2** – Restfenster: Die Runde hält im synchronen Abschnitt (`crawls.rs:654–684`) eine starke `App`. Ein Abbau durch den Besitzer in diesem Fenster ist nicht der letzte, und der eingereihte Wartungsauftrag kann noch in das gelöschte Verzeichnis schreiben. Vorschlag:
  - Alle Eingaben (Einstellungen, offene Sitzungen, Uhr, Wurzel, Ziel und einen `WriteQueue`-Griff ohne `App`) vorher aus Teilen holen, die die `App` nicht verlängern.
  - Oder die Uhr- und Einstellungslesung vor das Hochstufen ziehen und nur `enqueue` unter der `App` ausführen. Das verkleinert das Fenster, schließt es aber nicht.
  - Die Lesevorgänge in `session`/`mcp` (100-ms-Takt) haben dasselbe Muster ohne Wartungsauftrag.
- **B3** – 5-s-Obergrenze ohne Garantie bei langen Runden (A6); losgelöster Aufgabenabgleich nach dem Abbau (A5); die erste Crawl-Liste übernimmt das Auffrischen des Index; CLI/MCP-Liste ohne Register bis zur ersten Verwaltungsaktion (A4).

## Nicht geprüft

- Planer-Variante `system`: Sie trägt in den echten Planer ein.
- Windows-Planer mit `home_override`: Der Windows-Dry-Planer kennt keinen Basisordner.
- Ein Lauf des B1-Tests gegen `c7d9c424`.
- Der Test des Bauers.
- Lange Runden über 5 s.
- Linux/Windows-CI.

## Empfehlung

**Erst nachbessern (B1).** Die Korrektur beseitigt das gemeldete SIGBUS-Bild nachweislich: 40 Abbauzyklen sind sauber, 2×20 Wiederholungen liefen ohne Absturzbericht. Sie ist also klar besser als main.

Der frühe Ausstieg bei „Nie“ nimmt aber ein beobachtbares Verhalten aus CRAWL-124 weg, und zwar bei der Standardrichtlinie. B2 ist ein kleines Restfenster des ursprünglichen Fehlers. Es kann im selben Zug geschlossen oder als Auflage verfolgt werden.

Falls die Absturzrate in der CI drängt: Ein Merge mit B1 als sofortiger Folgeauflage ist vertretbar. Dafür muss der rote B1-Test bis zur Korrektur stehen bleiben.
