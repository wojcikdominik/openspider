# B4-LISTFIX – Beschädigung von Crawl-Dateien neben der Crawl-Verwaltung – Bericht

Stand 06.11.2026. Branch `fix/list-crawls-corruption` (Basis `main` e3c10e72), Mac-Worktree `openspider-wt/c-listfix`,
Server-Worktree `m3-listfix`. Befund 1 aus `M3-reports/b4-resume-tests.md` (`m3/resume-tests`). Der Fix hängt nicht von
`m3/resume` ab. Fable: `approved 82` mit vier Auflagen, umgesetzt (Abschnitt „Auflagen aus der Fable-Prüfung“).

## Ursache (belegt)

Zwei Fehler, die erst zusammen schaden:

1. **Sperrverlust beim Besitzer (POSIX):** `check_sqlite_header`, `probe_header` und der Fingerprint des Metadaten-Index lasen
   die Crawl-Datei per `std::fs::File::open`. Schließt ein Prozess irgendeinen Deskriptor einer Datei, gibt POSIX **alle** seine
   Sperren darauf frei, auch die gemeinsame Sperre seiner laufenden SQLite-Verbindung. Das geschieht im Betrieb ständig:
   Analyse-Worker und `crawl_status` (`CrawlFile::open`), `--json-progress`, `list_crawls` und die Verwaltung im eigenen Prozess.
2. **Aufräumen beim anderen Prozess:** `open_raw_metadata` öffnete mit Schreibrechten („als letzte Verbindung entfernt SQLite
   `-wal`/`-shm`“). Beim Schließen bekommt diese Verbindung die Exklusivsperre, weil der Besitzer seine verloren hat, hält sich
   für die letzte, führt das WAL zurück und löscht `-wal`/`-shm`.

Der Besitzer schreibt in das gelöschte WAL weiter; seine Commits gehen verloren, die Datei ist beschädigt (`wrong # of entries in
index`, `btreeInitPage() returns error code 11`, `database disk image is malformed`, `disk I/O error`).

**Beleg ohne Zeitabhängigkeit** (`crates/os-store/tests/wal_live_owner.rs`, zwei echte Prozesse), Stand vor dem Fix:

| Test | Vor dem Fix | Aussage |
|---|---|---|
| `another_process_listing_keeps_the_wal_of_a_live_owner` | grün | Fehler 2 allein schadet nicht |
| `…_after_the_owner_process_reopened_the_file` | rot | Fehler 1 + 2: `-wal` weg |
| `…_after_the_owner_process_listed_its_folder` | rot | ebenso, Auslöser ist die Verwaltung im Besitzerprozess |
| `reading_metadata_never_writes_or_merges_a_foreign_wal` | rot | Der Listenlauf führt das fremde WAL zurück |
| `the_owner_process_keeps_its_locks_when_it_reads_or_lists_its_own_file` (neu, Auflage c) | siehe unten | Sperrerhalt für sich |

**Sperrerhalt für sich belegt:** Im neuen Test öffnet und schließt der zweite Prozess absichtlich mit Schreibrechten (altes Verhalten), nachdem der
Besitzer in seinem Prozess geöffnet, gelesen, gelistet und geprüft hat. Gegenprobe nur auf dem Server: `read_header` auf `std::fs` zurückgestellt,
der Rest des Fixes bleibt. Ergebnis: genau dieser Test rot (`-wal/-shm` weg), die übrigen 7 grün; mit `read_header` über das VFS 8/8. Damit trägt jeder der
beiden Fixe für sich: Ohne den Kopf-Fix schützt die schreibgeschützte Verwaltung, ohne sie schützt der Sperrerhalt.

Korrektur zum Zwischenbericht: `…_listed_its_folder` schlug zunächst aus einem Testfehler fehl (Dateiname ohne `<id>.ospider`).
Nach der Korrektur ging er auf dem alten Stand zuerst durch, weil der zweite Prozess den Index-Treffer nutzte und die Datei gar nicht
öffnete. Er liest deshalb jetzt auch ohne Index. Erst damit ist er auf dem alten Stand aus dem richtigen Grund rot. Die übrigen
Aussagen des Zwischenberichts bleiben gültig.

## Betroffenheit

Rot auf dem ungefixten Stand e3c10e72 (`crates/os-cli/tests/listing_live_owner.rs`, `crates/os-app/tests/wal_live_owner_app.rs`,
je echte Prozesse):

| Weg | Vor dem Fix | Nach dem Fix |
|---|---|---|
| `--list-crawls` neben endendem MCP-Crawl (Test unverändert aus `m3/resume-tests`) | rot | grün |
| `--list-crawls` neben MCP-Stopp (Test unverändert) | rot | grün |
| `--list-crawls` plus `crawl_status`-Polling mitten im Crawl | rot | grün |
| `list_crawls` im zweiten MCP-Server | rot, `integrity_check` schlägt an | grün |
| `list_crawls` im selben Server, kein zweiter Prozess | grün | grün |
| CLI-Crawl plus `--list-crawls` | rot, `integrity_check` schlägt an | grün |
| CLI-Crawl mit `--json-progress` plus `--list-crawls` | rot | grün |
| App: Crawl-Verwaltung neben Fremdprozess (Besitzer simuliert, echter zweiter Prozess) | rot, `-wal` weg | grün |
| App-eigener Crawl: App listet, anderer Prozess liest daneben | rot, `-wal` weg | grün |
| App: fremden Crawl öffnen und pollen | grün | grün |

- **Voraussetzung:** zwei Prozesse auf derselben laufenden Datei. Ein Prozess allein ist sicher, weil SQLite Verbindungen im selben
  Prozess selbst verfolgt.
- **Geplante Aufgaben:** laufen als `openspider --run-task` (derselbe CLI-Crawl), `run_task.rs` listet außerdem selbst. Nicht
  eigens getestet.
- **Aufbewahrungsrunde der App:** nutzt denselben `Manager`. Nicht eigens getestet.
- **Versionen:** Store-Test auf `v1.0.0-beta.2` ausgeführt, dieselben Tests rot. beta.1 und `rel/beta3`: gleicher Code gelesen, nicht
  ausgeführt. Der Fehler besteht seit M1 (13.10.2026).
- **macOS:** gleiche POSIX-Sperren, daher vermutlich betroffen. Auf dem Mac nicht ausgeführt (Regel). **Windows:** vermutlich nicht
  betroffen, siehe unten.

## Fix

- **Kein `std::fs`-Öffnen einer Crawl-Datei unter Unix:** `open::read_header` liest den Kopf über das VFS von SQLite. SQLite kennt die
  Sperren je Inode und schiebt das Schließen auf, solange welche bestehen. Benutzt von `check_sqlite_header`, `probe_header`
  und dem Fingerprint (`probe_header` liefert den Kopf jetzt zurück, kein zweites Lesen).
- **Die Verwaltung liest nie mit Schreibrechten:** `files::MetadataConn`/`open_raw_metadata`. Mit Begleitdateien schreibgeschützt unter
  den normalen Sperren, sie lässt sie liegen. Ohne Begleitdateien `immutable`, damit die Verwaltung weiterhin nichts hinterlässt.
- **Schreibrechte nur noch gewollt:** `open_raw_write` für `put` (Umbenennen, Projekt, Sperren) und den Checkpoint vor dem Löschen.
  Alle stehen hinter `guard`, das laufende Crawls abweist; bei `Manager::lock` fehlte das bis zur Auflage b.
- `copy_snapshot` (Duplizieren, Export) und `read_listing` prüfen die Quelle nach dem Lesen, siehe nächster Abschnitt.

### Geprüft: Wettlauf mit `immutable`

Frage: Datei ohne Begleitdateien, der Besitzer startet danach.

- **Den Besitzer stören:** nicht möglich. `immutable` nimmt keine Sperren, legt nichts an und schreibt nichts. Auch das Schließen im selben
  Prozess läuft über das VFS und hebt keine Sperren auf.
- **Falsche Daten:** ohne Gegenmaßnahme möglich, solange der Besitzer während des Lesens zurückführt. Solange er nur ins WAL schreibt, sieht
  der Leser den alten, in sich stimmigen Stand; führt er während des Lesens zurück, ändert sich die Hauptdatei unter dem Leser. Mein erster Stand
  (Zwischenbericht) hatte diese Lücke. **Jetzt:** `MetadataConn::still_valid` prüft **nach** dem Lesen, dass weiter keine Begleitdateien liegen und
  Größe, Änderungszeit und Kopf der Hauptdatei gleich sind. Sonst wird neu gelesen, zuletzt unter Sperren. `copy_snapshot` veröffentlicht
  in dem Fall die Kopie nicht und kopiert unter Sperren neu.
- **Lesefehler ohne Sperren** (Auflage a): Ein Fehler aus `immutable` wird nie gemeldet, auch wenn die Datei danach unverändert aussieht, sondern immer unter
  Sperren wiederholt (`read_listing`, `find`, `copy_snapshot`). Ein zerrissenes Lesen erscheint also nicht als „beschädigt“; eine wirklich beschädigte Datei wird
  unter Sperren weiter als Fehler gemeldet. Bei `copy_snapshot` werden nur Lesefehler der Quelle wiederholt, nicht Abbruch durch den Nutzer oder ein belegtes Ziel.
- **Tests (Prüfnaht `read_listing_checked`):**
  - `a_writer_starting_during_the_read_is_seen_and_not_disturbed`: Besitzer startet zwischen Lesen und Prüfung und bleibt offen. Ergebnis ist
    sein Stand, sein WAL bleibt, er schreibt weiter, `integrity_check` ok.
  - `a_writer_that_started_and_finished_during_the_read_is_seen`: Schreiber ist schon wieder fertig. Ergebnis ist sein Stand, keine Begleitdateien.
  - `a_quiet_file_is_read_without_companions_and_stays_valid`.
  - `a_read_error_without_locks_is_never_published_and_repeated_under_locks`, `a_read_error_under_locks_is_reported`.
  - `a_copy_read_error_without_locks_is_repeated_under_locks`, `a_copy_invalidated_by_a_writer_is_not_published_and_repeated_under_locks`,
    `a_cancelled_copy_and_an_occupied_target_are_not_repeated`.
- **Restrisiko:** Ein Schreiber, der in einer Lesezeit von wenigen Millisekunden startet, zurückführt und endet, ohne dass sich Größe, Kopf oder
  `mtime` (Raster bis 10 ms unter Linux) ändern. Dann bliebe nur eine Auflistung veraltet. Die Datei wird davon nicht berührt, und der
  Index speichert solche Ergebnisse wegen seines eigenen Vergleichs nur bei gleichem Fingerprint.

### Geprüft: Windows

- **Kopf-Lesepfad:** unter Windows bleibt es beim Weg über `std::fs` wie bisher (`read_header_plain`). Sperren gehören dort dem Handle
  (`LockFileEx`), ein weiteres Handle hebt sie nicht auf. Der Fingerprint liest den Kopf jetzt nur einmal statt zweimal, die Werte sind gleich.
- **Nachweis hier:** `read_header_plain` wird auch unter Linux im Test übersetzt und mit dem VFS-Weg verglichen
  (`header_reads_agree_for_short_empty_foreign_and_real_files`).
- **`immutable` unter Windows:** nutzt dieselbe `win32-longpath`-VFS wie `open_sqlite`, der Pfad steht prozentkodiert in der URI und wird
  zu derselben Zeichenkette dekodiert. Schlägt das Öffnen fehl, fällt `open_raw_metadata` auf die schreibgeschützte Verbindung zurück.
- **Nicht ausgeführt:** Auf dem Server gibt es kein Windows-Ziel. Die neuen Tests laufen erst in der Windows-CI. **Bitte dort vor der
  Veröffentlichung prüfen**, vor allem `wal_live_owner` und `paths_registry` (keine Begleitdateien).

## Testergebnisse (Server, Worktree `m3-listfix`, Endstand mit allen Auflagen)

- `cargo fmt --all --check`: ok. `cargo clippy --workspace --all-targets -- -D warnings`: ok.
- `cargo test --workspace --exclude openspider-desktop -j 6 --locked --no-fail-fast` (über `ws-slot.sh`): **4 385 bestanden, 0 fehlgeschlagen,
  49 ignoriert**, 604 Testprogramme, Exit 0 (keine neuen Ignores).
- `cargo test -p os-store -p os-engine -p os-cli -p os-mcp -p os-app --no-fail-fast`: **1 663 bestanden, 0 fehlgeschlagen, 16 ignoriert**,
  261 Testprogramme.
- **20× unter Last, 20/20 grün:** auf CPU 0–3 gepinnt, dort vier `yes`-Prozesse (mit `timeout`, danach beendet, 0 Reste). Je Lauf
  `wal_live_owner` (8 Tests einschließlich Prozessrolle, rund 0,9 s), `wal_live_owner_app` (5 einschließlich zwei Prozessrollen, rund 1,2 s),
  `listing_live_owner` (7 Tests nacheinander, rund 45,5 s). Die zwei übernommenen Tests sind per Quelltextvergleich unverändert.
- **Gegenprobe auf dem alten Quelltext** (die fünf `os-store`-Dateien von `main`, Tests vom ersten Fix-Stand): `wal_live_owner` 4 grün, 3 rot, wie oben; die
  CLI- und App-Tests siehe Tabelle „Betroffenheit“. Gegenprobe Sperrerhalt siehe „Ursache“.
- Einheitentests in `os-store`: `registry::tests` 5, `open::header_tests` 2 (auch Pfade mit Leerzeichen, `#`, `%`, `?`, `&`, `=`, Umlaut), drei neue in
  `registry::management::tests`; `registry_management` zusätzlich mit dem Test zu `lock`.
- `npm run -w apps/desktop typecheck`: ok. `npm run -w apps/desktop test`: CSS-Lint ok, Vitest 117 Dateien, 1 011 Tests bestanden.
- Playwright mit echter Bridge (`--workers=1 --retries=0`): `crawls/management.spec.ts` und `crawls/bridge.spec.ts`, Chromium 7/7, WebKit 7/7.
- `bash scripts/check-neutral.sh`: keine Fundstellen.
- Nicht ausgeführt: macOS und Windows (siehe oben).

## Auflagen aus der Fable-Prüfung

| Auflage | Umsetzung | Nachweis |
|---|---|---|
| (a) `Err` aus `immutable` nie veröffentlichen | `read_listing_checked`, `copy_snapshot_checked`, `find` | Tests siehe „Wettlauf mit `immutable`“ |
| (b) `guard` in `Manager::lock` | `lock(id, locked, now)` weist laufende Crawls ab; `crawls.lock` übergibt die Uhr der App | `a_running_crawl_cannot_be_locked_or_unlocked_until_it_is_no_longer_running` |
| (c) Sperrerhalt-Test | zweiter Prozess öffnet absichtlich mit Schreibrechten | Gegenprobe oben |
| (d) Bericht und Folgepunkte | dieser Abschnitt, „Folgepunkte“ | – |

**Zu (b), über den Einzeiler hinaus:** Die Verwaltung bot „Sperren“/„Entsperren“ auch für laufende Crawls an (`model.ts`, `lock: any`). Damit wäre die Schaltfläche in einen Fehler
gelaufen; beide sind für laufende Crawls jetzt deaktiviert, mit dem vorhandenen Hinweis „Crawl läuft“. **Verhaltensänderung:** Ein laufender Crawl lässt sich erst nach Pause oder Ende
sperren. OUT-228 sagt dazu nichts Ausdrückliches.

## Folgepunkte (nicht in diesem Fix)

- **Migrations-Sicherung `open.rs:581`:** Im zweiten Zweig von `open_inner` läuft `std::fs::copy` auf die Crawl-Datei, während die eigene Schreibverbindung offen ist; das hebt deren Sperren
  auf. Vorschlag: `wal_checkpoint` mit einer temporären Verbindung, diese schließen, dann ohne offene Verbindung kopieren. Eine ältere Datei läuft dabei nicht, das Fenster ist klein.
- **`os-sched`, Merkmal `uuid/v4`:** `cargo test -p os-sched` allein übersetzt nicht (`Uuid::new_v4` fehlt); das Merkmal kommt nur über andere Crates. Vorschlag: in `crates/os-sched/Cargo.toml` selbst anfordern.
  Im Workspace-Lauf unschädlich.

## Nebenwirkungen und offene Punkte

- **Begleitdateien bleiben länger liegen:** Liest die Verwaltung gerade unter Sperren, wenn der Besitzer schließt, kann dieser nicht aufräumen.
  `-wal`/`-shm` bleiben dann liegen, bis die nächste Schreibverbindung kommt. Die Engine führt das WAL vorher zurück; Daten gehen nicht verloren.
- **Übrige `std::fs`-Zugriffe auf Crawl-Dateien:** nur noch `std::fs::copy` für die Sicherungskopie vor einer Migration (`open_inner`, siehe Folgepunkte). Der Rest des Workspace wurde per
  `git grep` durchsucht, kein weiterer Fund.
- **Beschädigte Dateien:** Der letzte Leseversuch unter Sperren kann neben einer wirklich beschädigten Datei leere `-wal`/`-shm` anlegen, die liegen bleiben.
- **Schon beschädigte Dateien** repariert der Fix nicht; dafür bleibt die Reparatur-Kopie der Verwaltung.
- `m3/resume-tests` enthält dieselben zwei Tests in `acceptance_resume.rs`; beim Merge ist die Doppelung harmlos (andere Datei).
- Spec: CRAWL-125 und OUT-228 decken das Verhalten ab, keine Spec-Frage.

## Fremde Dateien (K2-92)

| Datei | Änderung | Grund |
|---|---|---|
| `crates/os-app/tests/wal_live_owner_app.rs` | neu | Betroffenheit App |
| `crates/os-cli/tests/listing_live_owner.rs` | neu | die zwei Tests unverändert, plus je Aufrufweg ein Test |
| `crates/os-app/src/commands/crawls.rs` | `crawls.lock` übergibt die Uhr | Auflage b |
| `apps/desktop/src/features/crawls/model.ts`, `crawls.test.tsx` | Sperren/Entsperren für laufende Crawls aus | folgt Auflage b |

Sonst Produktivcode nur in `crates/os-store/src/{open,files,registry}.rs` und `registry/{index,management}.rs`.
