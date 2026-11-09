# B4-STOREFIX – Folgepunkte aus B4-LISTFIX

Stand: 07.11.2026. Worktree `m3-storefix`, Branch `m3/storefix`, Basis `origin/main`
mit B4-LISTFIX (`bdf2d545`). Clean-Room: Soll nur aus `docs/spec/*`, kein Referenzprodukt.

## 1. Migrations-Sicherung (CRAWL-125, OUT-233, OUT-184)

**Ursache:** `std::fs::copy` öffnete und schloss die Hauptdatei bei offener
SQLite-Schreibverbindung. POSIX gibt dabei auch die Sperren anderer Verbindungen
des Besitzerprozesses frei. Eine Hauptdatei ohne zurückgeführte WAL-Frames ist
außerdem keine vollständige Sicherung.

**Änderung:** Bei WAL-Frames führt eine temporäre Verbindung zuerst
`wal_checkpoint(TRUNCATE)` aus und wird vor der Kopie geschlossen. Bei `busy`
bricht das Öffnen vor Sicherung und Migration ab. Erst nach der Sicherung wird
die eigentliche Migrationsverbindung geöffnet. Unter Unix kopiert der bestehende
SQLite-VFS-Lesepfad jetzt auch ganze Dateien in begrenzten Blöcken; Windows behält
`std::fs::copy` (Sperren gehören dem Handle).

**Wahl:** Das alleinige Schließen der temporären Verbindung genügt nicht, wenn
noch eine zweite Verbindung desselben Prozesses offen ist. Deshalb liest auch
die Kopie über das VFS. Anders als `VACUUM INTO` verändert dieser Weg weder
Seitenanordnung noch Inhalt der Sicherung. Die vorhandenen Bytegleichheits-Tests
bleiben unverändert. Es wird keine zusätzliche Abhängigkeit benötigt.

**Test:** `wal_live_owner` migriert eine kopierte Schema-v2-Fixture mit und ohne
WAL-Frames, während eine zweite Verbindung im selben Prozess offen bleibt.
Sicherung: altes Schema, 42 URLs, WAL-Metadatum, `integrity_check=ok`;
Ziel: vollständige Leseschnittstelle. Ein echter zweiter Prozess öffnet und
schließt danach absichtlich schreibend; WAL/SHM und spätere Commits bleiben erhalten.
Gegenprobe mit `std::fs::copy` statt VFS: derselbe neue Test rot, WAL/SHM entfernt.
Ein weiterer Test blockiert den Checkpoint durch einen alten Leseschnappschuss:
`DatabaseBusy`, keine Sicherung veröffentlicht, Schema weiterhin v2.
Vorhandene Migrations- und Bytegleichheits-Tests unverändert.

## 2. Eigenständiges os-sched

**Ursache:** `mail/accounts.rs` verwendet `Uuid::new_v4`, aber das Feature kam
bislang nur durch Feature-Vereinigung im Workspace hinzu.

**Änderung:** `uuid` fordert `v4` in `crates/os-sched/Cargo.toml` selbst an.
Kein `cargo update`, keine Änderung an `Cargo.lock`; Tokio bleibt 1.53.3.

**Test:** `cargo test -p os-sched -j 4 --locked` eigenständig: **42 bestanden,
0 fehlgeschlagen, 0 ignoriert**, einschließlich der bestehenden Mail-Fixtures.

## 3. Überschattete Uhrzeit

**Ursache:** Der Lock/Unlock-Zweig deklarierte `now` erneut direkt nach der
bereits vorhandenen Zeitaufnahme für kurze Verwaltungsbefehle.

**Änderung:** Zweites `let now` entfernt. Alle kurzen Aktionen verwenden dieselbe
Zeitaufnahme; die Zustandsregeln bleiben gleich.

**Test:** App-Fixtures für Lock/Unlock sowie Workspace-Gate (Ergebnis unten).

## 4. Lock/Unlock vor Schreibzugriff prüfen (OUT-228, CRAWL-125)

**Ursache:** `Manager::lock` prüfte bereits vor `put` den Zustand, der App-Befehl
prüfte in seiner ersten Schleife jedoch nur die Existenz. Bei einer gemischten
Auswahl konnte ein früherer Eintrag schon verändert sein, bevor ein späterer
laufender Crawl abgewiesen wurde.

**Änderung:** Bekannte laufende eigene und fremde Sitzungen weist die App vor
jedem Zugriff auf Crawl-Dateien ab. Für unbekannte Dateien lädt sie ausschließlich
lesend über das SQLite-VFS die Zustände und prüft die gesamte Auswahl, bevor
sie `Manager::lock` aufruft. Dessen erneute Prüfung bleibt direkt vor `put`;
nur `put` öffnet die Crawl-Datei schreibend. Der Pfad wurde vom App-Befehl über
`admin`, `Manager::get`, Index/Fingerprint, `read_listing` bis `put` geprüft.
Verwaltungsindex und Projektregister sind eigene Dateien.

**Test:** App-Befehl gegen eine Fixture mit echtem lebendem Fremdbesitzer, jeweils
Lock und Unlock; Hauptdatei und WAL unverändert, zulässiger erster Eintrag einer
Mehrfachauswahl unverändert, Besitzer schreibt anschließend weiter und prüft
Integrität. Eine bekannte laufende Sitzung wird selbst bei inzwischen fehlendem
Dateipfad mit `Busy/err-crawl-running` abgewiesen. Store-Fixture prüft denselben
Sperrerhalt für direkte Aufrufe von `Manager::lock`.

**Abgrenzung / Frage zur Formulierung:** Bei einer bislang unbekannten fremden
Datei ist eine lesende Metadatenprüfung nötig, um „läuft“ überhaupt zu erkennen.
„Keine Datei öffnen“ ist hier als „keine schreibende Crawl-Verbindung vor der
Zustandsprüfung“ umgesetzt; bekannte Live-Sitzungen benötigen auch keinen
lesenden Zugriff. Falls selbst das lesende VFS-Prüfen unbekannter Dateien
verboten sein soll, ist ein zusätzlicher verbindlicher Besitzer-/Zustandskanal
außerhalb der Crawl-Datei zu spezifizieren. OUT-228 legt einen solchen nicht fest.

## 5. Liste auch nach fehlgeschlagenem Lock/Unlock aktualisieren (OUT-228)

**Ursache:** `refresh()` lag hinter dem erfolgreichen `await lock/unlock` und
wurde bei Fehlern übersprungen. Die Auswahl zeigte veraltete Zustände.

**Änderung:** Gemeinsamer Lock/Unlock-Zweig mit `refresh()` im `finally`.
Ein zusätzlicher Refresh-Fehler verdrängt die ursprüngliche Aktionsfehlermeldung
nicht; ein alleiniger Refresh-Fehler bleibt sichtbar.

**Test:** Drei neue Vitest-Fälle mit der Verwaltungs-Fixture: fehlgeschlagenes
Lock und Unlock mit anschließendem Live-Zustand, sowie gleichzeitiger Aktions-
und Refresh-Fehler. Gezielte Datei: **27/27 bestanden**.

## Gates auf dem Endstand

- `cargo fmt --all --check`: Exit 0.
- `cargo clippy --workspace --all-targets -j 4 --locked -- -D warnings`: Exit 0,
  einschließlich Desktop, keine Warnungen.
- `cargo test --workspace -j 4 --locked`: **4 415 bestanden, 0 fehlgeschlagen,
  51 bestehende Ignores**, 611 Test-/Doc-Testprogramme, **Exit 0**, ohne Ausschlüsse
  (einschließlich Desktop). Alle neuen Regressionen sowie N04 grün.
- Eigenständiges `os-sched`: **42 bestanden, 0 fehlgeschlagen, 0 ignoriert**.
- Gezielte Store-Migrationstests zunächst **22/22**; abschließend
  `wal_live_owner` **11/11**, App `wal_live_owner_app` **7/7**.
- `npm ci`: Exit 0, 421 Pakete installiert (Lockfile unverändert).
- `npm run -w apps/desktop typecheck`: Exit 0.
- `npm run -w apps/desktop test`: CSS-Lint grün, **119 Dateien, 1 025 Tests
  bestanden**, 0 fehlgeschlagen.
- Playwright `crawls/management.spec.ts`, `crawls/bridge.spec.ts`: **Chromium 7/7,
  WebKit 7/7**, echte App-Bridge, keine Retries.
- `bash scripts/check-neutral.sh`: Exit 0, keine Fundstellen.

Schwere Befehle mit
`timeout 3600s nice -n 10`, Cargo mit `-j 4 --locked`; Workspace-Test nach
`~/ws-slot.sh` mit
`CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0`.
Der erfolgreiche vollständige Lauf verwendete zusätzlich das installierte
Chromium über `OPENSPIDER_CHROMIUM` und `OPENSPIDER_SECRET_STORE=memory`.
Playwright ausschließlich auf `OS_E2E_PORT=14611`, `--workers=2`, Chromium und WebKit.

### Umgebungsbedingter Neustart des Workspace-Gates

Der erste Workspace-Lauf stoppte in `os-analyze/tab_accessibility`: zwei Tests
meldeten `ChromiumMissing`, weil `OPENSPIDER_CHROMIUM` noch nicht gesetzt war.
Mit dem vorhandenen Playwright-Chromium
`/home/build/.cache/ms-playwright/chromium-1248/chrome-linux64/chrome`
bestand dieselbe unveränderte Suite direkt **6/6**. Der vollständige Lauf wurde
danach erneut über `~/ws-slot.sh` gestartet, zusätzlich mit
`OPENSPIDER_SECRET_STORE=memory` (Build-Server ohne Secret Service).
Für dessen Platzschwelle wurden ausschließlich 125 eigene reproduzierbare
Test-Binaries (30,02 GiB) entfernt und anschließend von Cargo neu gebaut;
keine fremden Arbeitsverzeichnisse berührt. Keine Tests oder Budgets geändert.

### N04-Zeitgrenze im zweiten vollständigen Lauf

Der zweite Workspace-Lauf stoppte nach 945 bestandenen Tests am unveränderten
`beta_n04::synthetic_5000_live_details_filter_pause_resume_and_reopen`:
`session progress blocked by aggregate readers` (bestehende 1-s-Grenze).
Der einzelne Fall bestand auf demselben Binärstand in 34,33 s, danach auch die
gesamte N04-Suite mit normaler Parallelität **5/5** in 38,71 s. Die Ursache des
einmaligen Zeitfehlers ist damit nicht bewiesen; kein Test, Budget oder
Produktpfad wurde dafür geändert. Das vollständige Workspace-Gate wurde mit
identischer Umgebung und Parallelität erneut über `~/ws-slot.sh` gestartet;
für die Platzschwelle wurden nochmals ausschließlich 128 eigene reproduzierbare
Test-Binaries (30,13 GiB) freigegeben. Der dritte vollständige Lauf
bestand einschließlich N04 mit den oben genannten Gesamtzahlen.

## Commits

| Punkt | Commit |
|---|---|
| 1 – Sicherung und Sperrerhalt | `2bd89369f` |
| 2 – uuid/v4 für os-sched | `14c656ea3` |
| 3 – gemeinsame Uhrzeit | `ecd70ba1f` |
| 4 – App-Vorprüfung und ergänzende Store-Prozesstests | `3004335f9` |
| 5 – Refresh nach Fehlern und Vitest | `ee3f7992a` |

Alle Commits mit Bereichspräfix und dem geforderten Co-Author-Trailer, kein Push.

## Fremde Dateien (K2-92)

| Datei | Grund |
|---|---|
| `crates/os-store/tests/wal_live_owner.rs` | Fixture-Nachweis für Sicherung, blockierten Checkpoint und direkten Manager-Sperrerhalt |
| `crates/os-store/src/registry/management.rs` | Dokumentiert die geprüfte Grenze vor dem schreibenden Öffnen |
| `crates/os-app/tests/wal_live_owner_app.rs` | Nachweis des vollständigen App-Befehlspfads neben echtem Fremdbesitzer |
| `apps/desktop/src/features/crawls/crawls.test.tsx` | Fixture-Regression für Refresh nach Lock-/Unlock-Fehlern |

Weitere geänderte Produktivpfade sind ausdrücklich Teil des Auftrags.
Keine bestehenden Tests oder Budgets gelockert, keine Tests mit
`// acceptance: independent` geändert. Keine neuen Abhängigkeiten/Lizenzen.
Linux ausgeführt; Windows und macOS hier nicht ausgeführt.

## Abschluss

`target/`, `node_modules`, `apps/desktop/node_modules` und die eigenen
Playwright-Ergebnisse wurden entfernt. Arbeitsbaum sauber, kein Push.
