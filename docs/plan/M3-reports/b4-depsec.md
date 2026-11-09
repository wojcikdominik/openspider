# B4-DEPSEC – Sicherheits-Updates rmcp und SQLite für beta.4

Stand: 09.11.2026. Worktree `c-depsec` (Mac) bzw. `m3-depsec` (Server), Branch `m3/depsec`, Basis lokales `main`
`72c84d3f`. Nicht nach origin gepusht. Anlass: CI-Lauf 42609912521, Job `cargo-deny (licenses, bans, sources,
advisories)`.

## Ergebnis in Kürze

| Advisory | Paket | vorher | nachher | Behoben ab | Bei uns erreichbar? |
|---|---|---|---|---|---|
| RUSTSEC-2026-0459 (CVE-2026-119375) | `libsqlite3-sys` | 0.35.0 (SQLite 3.50.2) | 0.39.1 (SQLite 3.54.1) | 0.39.0 | ja, eingeschränkt (siehe 1) |
| RUSTSEC-2026-0457 | `rmcp` | 3.5.1 | 3.9.0 | 3.7.0 | nein, OAuth-Client nicht einkompiliert |
| RUSTSEC-2026-0461 | `rmcp` | 3.5.1 | 3.9.0 | 3.6.0 | ja, nur hinter Bearer-Token auf Loopback |
| RUSTSEC-2026-0462 | `rmcp` | 3.5.1 | 3.9.0 | 3.9.0 | nein, HTTP-Client nicht einkompiliert |

`cargo deny check` meldet danach `advisories ok, bans ok, licenses ok, sources ok`. Weitere offene Advisories gibt es
nicht. Alle Ausnahmen in `deny.toml` sind unverändert und treffen weiter zu: cargo-deny meldet für keine von ihnen
„advisory not detected“. `deny.toml` ist nicht geändert, kein Advisory wurde ignoriert.

Tokio bleibt auf 1.53.3. Keine große Abhängigkeit kam mit.

## 1. Je Advisory: Nutzung bei uns

### RUSTSEC-2026-0459 – SQLite liefert nicht initialisierten Page-Cache-Speicher

SQLite vor 3.54.0 kann nicht initialisierten Speicher zurückgeben, wenn eine Datenbank zusammen mit einer `-wal`-Datei
mit abweichender Seitengröße geöffnet wird. Der Angriff ist lokal und braucht ein präpariertes Dateipaar, das das
Opfer öffnet.

**Nutzung:** Wir bauen SQLite gebündelt (`rusqlite` mit `bundled`). Jede Crawl-Datei (`.ospider`) läuft im WAL-Modus.
Die App, die CLI und der MCP-Server öffnen auch Crawl-Dateien, die von anderen stammen: Import, Projektordner,
Fixture-Archiv, MCP-Werkzeuge mit Pfad. Eine fremde `.ospider` mit präparierter `-wal` daneben ist also ein realistischer
Weg. Die Folge wären fremde Speicherbytes in Abfrageergebnissen, etwa in einem Export. **Betroffen, Fix nötig.**

**Behebung:** `rusqlite` 0.37 → 0.41.1. Erst `rusqlite` 0.41 verwendet `libsqlite3-sys` 0.39; ein Update von
`libsqlite3-sys` allein ist wegen `links = "sqlite3"` nicht möglich. Gebündelt ist jetzt SQLite **3.54.1**
(`SQLITE_SOURCE_ID 2026-10-16 …`), vorher 3.50.2. Mindest-Rust: `rusqlite` 0.41 verlangt 1.89, unser Workspace 1.90.

### RUSTSEC-2026-0457 – SSRF-Filter im OAuth-Client von rmcp

**Nutzung:** Die OAuth-Funktion wird **nicht genutzt und nicht einkompiliert.** Workspace-Features von `rmcp`:
`server`, `client`, `transport-io`, `transport-child-process`, `transport-streamable-http-server`. Das Feature `auth`
fehlt. Geprüft mit `cargo tree -e features -i rmcp`: kein `auth`, kein `transport-streamable-http-client*`. Das Feature
`client` dient nur den Prozess-Tests von os-mcp (rmcp-Client über Kindprozess). **Nicht erreichbar.** Behoben mit, weil
cargo-deny nach Version urteilt.

### RUSTSEC-2026-0461 – unbegrenzter Tool-Schema-Cache im Streamable-HTTP-Server

**Nutzung:** Ja. `crates/os-mcp/src/http.rs` nutzt `StreamableHttpService` (OUT-285, AGENT-002). Die betroffene
Protokollversion `2026-07-28` war in rmcp 3.5.1 schon `LATEST`. Der Server lauscht aber nur auf `127.0.0.1`/`::1`, und
die Middleware `check` (Bearer-Token, `Host`, `Origin`) liegt *vor* dem rmcp-Dienst. Den Cache konnte also nur ein
lokaler Prozess mit gültigem Token füllen. **Erreichbar nur authentifiziert; geringe Schwere.** In 3.6.0 ist der Cache
entfernt.

### RUSTSEC-2026-0462 – unbegrenztes Puffern von Antwortkörpern im Streamable-HTTP-Client

**Nutzung:** Nein. Betroffen sind nur die Client-Features `transport-streamable-http-client-reqwest` und
`-unix-socket`; keines ist aktiv. OpenSpider ist selbst kein HTTP-MCP-Client. **Nicht erreichbar.**

## 2. Änderungen

**`Cargo.lock`, gezielt:**

- `cargo update -p rmcp --precise 3.9.0`: nur `rmcp` 3.5.1 → 3.9.0 und dessen Kanten (+`libc`, +`sha2 0.11.1`; beide
  standen schon im Lockfile).
- `cargo update -p rusqlite --precise 0.41.1`: `rusqlite` 0.37.0 → 0.41.1, `libsqlite3-sys` 0.35.0 → 0.39.1, `hashlink`
  0.10.0 → 0.12.2 (auf `hashbrown` 0.17.1, Feature `default-hasher` mit `foldhash` 0.2.0, beides schon im Lockfile).
  Neu sind `sqlite-wasm-rs` 0.6.2 und `rsqlite-vfs` 0.2.0 (MIT). Beide hängen nur am Ziel `wasm32`, erscheinen also
  weder in unseren Builds noch im cargo-deny-Graph.
- Cargo 1.99 auf dem Server hat bei jedem `cargo update -p …`, auch bei einem No-op mit der gleichen Version, rund 25
  fremde Kanten umgehängt: `windows-sys` 0.61.2 → 0.60/0.52/0.45, `getrandom`, `hashbrown`, `windows-core`. Diese
  Kanten habe ich wie in N33 auf den Stand von `main` zurückgesetzt. Übernommen sind nur die Paketblöcke der genannten
  Pakete. `cargo metadata --locked` und alle Läufe mit `--locked` akzeptieren das Lockfile.

**`Cargo.toml`:** `rusqlite = "0.41"` mit zusätzlichem Feature `fallible_uint`. Seit `rusqlite` 0.38 liegt
`ToSql`/`FromSql` für `u64`/`usize` hinter diesem Feature. Die Umsetzung ist dieselbe wie in 0.37: Werte über
`i64::MAX` ergeben einen Fehler, negative Werte beim Lesen ebenfalls. Damit bleibt das Verhalten gleich, ohne 31
Fehlerstellen einzeln umzuschreiben.

**Breaking Changes von rusqlite 0.38–0.41 im Code:**

- `crates/os-store/src/writer.rs`: `Connection::busy_handler` verlangt jetzt `&mut Connection` und legt eine Closure in
  den Client-Daten ab. `checkpoint_within` (CRAWL-126, CRAWL-141) erhält die Verbindung nur geteilt, aus `CrawlFile` und
  aus der Analyse (`AnalysisBusy` hält dort `&Connection` über die Phasen). Ein `&mut` hätte die öffentliche API von
  os-store und os-analyze geändert. Der begrenzte Busy-Handler wird deshalb über `sqlite3_busy_handler` direkt gesetzt
  (`set_bounded_busy`). Das Muster ist dasselbe wie beim Autorisierer in `readonly_sql.rs`. Wie bisher ist es ein
  Funktionszeiger ohne Nutzdaten, Panics werden abgefangen, und `busy_timeout` ersetzt ihn danach wieder. Abgedeckt
  durch `fix_wal_checkpoint` (CRAWL-126), `pause_merge`, `wal_live_owner*`.
- `Statement::column_names()` liefert jetzt einen Iterator statt `Vec<&str>`. Angepasst in `readonly_sql.rs` und
  `tests/read_api_views.rs`.
- Die Windows-Pfade (`open_with_flags_and_vfs(…, "win32-longpath")`) sind unverändert gültig. Die Signatur ist gleich,
  `Name` gilt weiter für `&str`, und das VFS `win32-longpath` ist in SQLite 3.54.1 enthalten. Unter Linux nicht
  übersetzt, siehe offene Punkte.

**rmcp 3.5.1 → 3.9.0:** Innerhalb von SemVer 3, keine Code-Anpassung nötig. Verhaltensrelevant laut Changelog:

- POST ohne Sitzung, der kein `initialize` ist, ergibt jetzt 400 (3.6.0).
- Eingehende Zeilen im stdio-Transport sind auf 16 MiB begrenzt (3.6.0). Unsere Anfragen sind um Größenordnungen
  kleiner, und Antworten sind nicht betroffen.
- Zustandsbehaftete Session-Worker beachten jetzt `config.cancellation_token`, das wir beim Beenden auslösen (3.9.0).
- SIGTERM vor dem Kill beim Beenden eines Kindprozesses (3.9.0), betrifft nur die Prozess-Tests.

Alle MCP-Tests (Rust und Playwright, siehe 3) sind unverändert grün, auch Status, Token, Host/Origin und Sitzungsende.

**Tests, die an die alte SQLite-/rusqlite-Version gebunden waren:**

- `crates/os-app/tests/runtime_lint.rs`: Das Fixture-Manifest nannte `rusqlite = "0.37"` und kollidierte über
  `links = "sqlite3"` mit 0.41. Jetzt steht dort `"0.41"`, wie im Workspace.
- `crates/os-analyze/tests/k83_duplicate_plans.rs`, Gegenprobe `the_check_would_have_caught_the_old_formulation`
  (ONPAGE-143, K-83). SQLite 3.54 hat die **EXISTS-to-JOIN-Optimierung**: Ein `EXISTS` als oberstes `AND`-Glied wird
  zum Join mit automatischem Index und Bloom-Filter. Der Plan der alten Formulierung ohne die fünf Indizes enthält
  deshalb keine korrelierte Unterabfrage mehr (`SEARCH d EXISTS USING AUTOMATIC PARTIAL COVERING INDEX`), und die
  Gegenprobe prüfte die Erkennung nicht mehr. Die Gegenprobe schaltet die Optimierung jetzt für ihre Verbindung ab
  (`sqlite3_test_control(SQLITE_TESTCTRL_OPTIMIZATIONS, …)`, Maske `SQLITE_ExistsToJoin`). Damit prüft sie die
  Erkennung wieder gegen den unveränderten Wortlaut. Ändert SQLite die interne Maske, schlägt der Test fehl, statt leer
  durchzulaufen. Die eigentlichen Prüfungen (fünf Duplicate-Filter ohne korrelierte Unterabfrage, kein Filter nutzt die
  fünf Indizes) sind unverändert und grün.

## 3. SQLite-Prüfung: Pragmas, VFS, Sperren, alte Dateien

- **Pragmas** (`journal_mode`, `synchronous`, `wal_autocheckpoint`, `wal_checkpoint(PASSIVE|TRUNCATE)`,
  `journal_size_limit`, `cache_size`, `temp_store`, `threads`, `busy_timeout`, `application_id`, `user_version`,
  `data_version`, `query_only`, `foreign_keys`, `integrity_check`, `table_info`): In den Release-Notes von 3.51 bis 3.54
  steht für keines eine Änderung. Alle Tests dazu sind grün. **Korrektur (Nachtrag A1):** Die Pragmas sind unverändert,
  das Sperrverhalten des Unix-VFS aber nicht; die Release-Notes erwähnen das nicht. Siehe Abschnitt 3a.
- **Kompilieroptionen:** `libsqlite3-sys` 0.39 lässt für `bundled` nur `SQLITE_ENABLE_MEMORY_MANAGEMENT` weg.
  `sqlite3_release_memory` nutzen wir nicht.
- **VFS-Header-Lesen** (`open.rs`, `read_vfs`/`read_header`): `header_reads_agree_for_short_empty_foreign_and_real_files`
  und `immutable_opens_paths_with_special_characters_without_companions` sind grün.
- **Sperr- und WAL-Tests:** `wal_live_owner` 11/11, `listing_live_owner` 8/8, `wal_live_owner_app` 7/7,
  `registry_management` 15/15, `pause_merge` 4/4, `fix_wal_checkpoint` 1/1. Unter 3.54 sind diese Tests allein aber
  kein vollständiger Wächter mehr für den B4-LISTFIX-Fix: Die neue Weiche in SQLite verdeckt den Schaden, siehe 3a.
  Seit dem Nachtrag hat `wal_live_owner` 12 Tests.
- **Alte Crawl-Dateien (OUT-255, Fixture-Archiv):** `compat_archive` in os-store, os-cli, os-model und os-app, zusammen
  11 Tests grün. Das Dateiformat von SQLite ist rückwärtskompatibel; 3.54 ändert kein Standardformat.
- **Weitere Hinweise aus den Release-Notes:** 3.53 rundet Fließkomma → Text standardmäßig auf 17 statt 15 Stellen.
  Kein Golden, kein Export und kein Paritätsvergleich hat sich dadurch geändert (Paritätslauf 218/218). 3.54 ruft den
  Autorisierer auch für Funktionen in `DEFAULT`-Klauseln auf; `readonly_sql` erlaubt nur `SELECT`, nicht betroffen.
  3.54 setzt unter Windows mindestens Vista voraus; unsere Mindestversion ist höher.

## 3a. Nachtrag (Auflage A1): Sperrverhalten des Unix-VFS in SQLite 3.54

Geprüft am Quelltext (`os_unix.c` im Amalgamat von `libsqlite3-sys` 0.35.0 gegen 0.39.1, `diff` der Abschnitte):

- **Neue Weiche `unixIsSharingShmNode`.** `unixLock` lehnt eine RESERVED- oder EXCLUSIVE-Sperre jetzt mit
  `SQLITE_BUSY` ab, wenn die Verbindung eine `-shm` offen hat und ein anderer Prozess dort eine Sperre auf dem DMS-Byte
  hält. Geprüft wird mit `F_GETLK`, also nur gegen fremde Prozesse. Im WAL-Modus braucht SQLite diese Sperren auf der
  Hauptdatei nur beim Schließen der letzten Verbindung (Zurückführen, `-wal`/`-shm` löschen), beim Verlassen des
  WAL-Modus und im Modus `locking_mode=EXCLUSIVE`. Der Kommentar im Quelltext nennt als Anlass ausdrücklich verlorene
  Sperren durch ein „rogue close(open(dbFile))“. Das ist genau das beta.3-Szenario aus B4-LISTFIX: Der Besitzer verliert
  seine POSIX-Sperren auf der Hauptdatei, die Sperren auf der `-shm` behält er aber, denn das ist eine andere Datei.
  Eine fremde Verbindung, die sich beim Schließen für die letzte hält, bekommt jetzt `SQLITE_BUSY` und lässt `-wal`/`-shm`
  liegen, statt sie zurückzuführen und zu löschen.
- **Für uns eine zweite Verteidigungslinie.** Die erste bleibt unser Fix: Kopf über das VFS lesen, Verwaltung nur
  lesend. Nebenwirkung laut Quelltext: Hält ein fremder Prozess die `-shm`, kann die letzte Verbindung nicht aufräumen
  und `PRAGMA journal_mode=DELETE` nicht ausführen. Wir verlassen den WAL-Modus nie (`profile.rs` setzt nur `WAL`).
  Liegengebliebene `-wal`/`-shm` gab es neben lesender Verwaltung schon vorher (B4-LISTFIX, Nebenwirkungen).
- **Blockierende Sperren (`osSetPosixAdvisoryLock` mit `iBusyTimeout`):** Der Platzhalter ist umgebaut. Bisher wurde
  jede Millisekunde neu versucht, jetzt gibt es außerhalb von `SQLITE_TEST` einen einzigen nicht blockierenden Versuch.
  Der Code existiert nur mit `SQLITE_ENABLE_SETLK_TIMEOUT`; `libsqlite3-sys` setzt das nicht. Das Gleiche gilt für das
  Steuerkommando `SQLITE_FCNTL_LOCK_TIMEOUT`, das wir nicht benutzen. **Bei uns nicht wirksam.**
- **Zusätzliches `fsync` unter Apple:** Auf MSDOS-Dateisystemen (FAT/exFAT, etwa USB-Sticks) schreibt SQLite in eine
  leere Datei ein Byte `S`, um die Inode-Nummer stabil zu machen, und ruft jetzt danach `fsync` auf. Betrifft nur
  macOS bei einer neuen, leeren Crawl-Datei auf einem solchen Datenträger: ein Sync mehr beim Anlegen. Schlägt er fehl,
  kommt `SQLITE_IOERR_FSYNC` statt eines stillen Weiterlaufens.
- **Sonst im Unix-VFS:** `mremap`-Pfad und `-shm`-Größen mit 64 Bit gerechnet, Diagnose `SQLITE_FCNTL_FILESTAT` (nur
  mit `SQLITE_DEBUG`/`SQLITE_ENABLE_FILESTAT`, bei uns aus), Auswahl der Temp-Verzeichnisse neu, `O_TMPFILE` für
  Temp-Dateien. Für unsere Sperren ohne Bedeutung.

**Gegenprobe B4-LISTFIX unter 3.54 (F2).** Auf dem Server habe ich `read_header` probeweise wieder über `std::fs`
gelesen. Das war nur lokal und ist nicht committet; danach habe ich zurückgesetzt und per Prüfsumme verglichen.

- `the_owner_process_keeps_its_locks_when_it_reads_or_lists_its_own_file` ist mit dem alten Lesepfad jetzt **grün**.
  Unter 3.50 war er rot. Auch die übrigen 10 Tests in `wal_live_owner` bleiben grün. Die neue Weiche in SQLite verdeckt
  den Schaden also: Der fremde Schreiber bekommt beim Schließen `SQLITE_BUSY` und räumt nicht auf. Unser Fix wäre
  damit unbewacht.
- **Neuer Test** `the_owner_process_still_holds_its_shared_lock_after_reading_and_listing_its_own_file`
  (`crates/os-store/tests/wal_live_owner.rs`, nur Unix). Der Besitzer öffnet, liest, listet und prüft seine Datei im
  eigenen Prozess. Danach fragt ein zweiter Prozess per `F_GETLK` den SHARED-Bereich der Hauptdatei ab (`0x4000_0002`,
  510 Bytes) und erwartet eine Lesesperre mit der PID des Besitzers. Zur Gegenprobe läuft dieselbe Abfrage vorher
  einmal und sieht die Sperre. Mit dem alten Lesepfad ist der Test **3 von 3 Läufen rot** („Der Besitzer hat seine
  gemeinsame Sperre verloren“, `left: None`), mit dem VFS-Lesepfad grün. Damit bewacht er unseren Fix unabhängig vom
  Verhalten von SQLite.

## 3b. Folgepunkte aus der Fable-Prüfung

- **F1, Fixture für RUSTSEC-2026-0459** (`crates/os-store/tests/compat_archive.rs`,
  `a_foreign_wal_with_a_smaller_page_size_yields_no_stale_memory_and_fails_the_check`, `// spec: OUT-255`).
  Ausgangspunkt ist die Archivdatei `schema-v5` mit Seitengröße 4096. Daneben erzeugt der Test eine gültige `-wal`
  mit Seitengröße 1024 (Kopf, Salze, Prüfsummen) und einem Commit-Frame für die Wurzelseite von `urls`. Kein Binär-Blob
  im Repo. Vor jedem Lesen füllt er freigegebenen Heap-Speicher mit einer Markierung. Ergebnis unter 3.54:
  - kein Panic;
  - beide Leseläufe liefern dasselbe Ergebnis, und keine Markierung taucht auf (die Seite ist ab Byte 1024 genullt,
    `memset` in `sqlite3WalReadFrame`);
  - `integrity_check` ist falsch, `files::verify` meldet `Corrupt`;
  - Öffnen mit Schreibrechten scheitert mit `DatabaseCorrupt`;
  - Hauptdatei und `-wal` bleiben byte-gleich.

  **Gegenprobe mit altem SQLite:** Dasselbe Dateipaar mit dem System-SQLite 3.46.1 des Servers (Python, nur Prototyp)
  liefert „Rowid 76 out of order“. Die Fixture hat zwei Zeilen; die falsche Rowid stammt aus nicht initialisiertem
  Speicher.

  **Befund, Entscheidung offen:** „Sauber scheitern“ gilt unter 3.54 nur teilweise. Lesend öffnen und `SELECT` laufen
  **ohne Fehler** und liefern für die überdeckte Seite Zeilen aus Nullen und Spalten-Defaults (2 Zeilen, `id` 0). Erst
  `integrity_check`/`files::verify` und das Öffnen mit Schreibrechten erkennen die Beschädigung. Speicherinhalt kommt
  nicht mehr heraus; das war der Kern des Advisorys. Eine eigene Ablehnung beim Öffnen wäre ein kleiner Zusatzcode und
  ist von keiner Spec gedeckt: Seitengröße im Kopf der `-wal` gegen den Dateikopf vergleichen und mit
  `StoreError::Corrupt` abweisen. Nicht umgesetzt, bitte entscheiden.
- **F2:** siehe 3a.
- **F3, direkter Test für `set_bounded_busy`** (`crates/os-store/src/writer.rs`, Modul `bounded_busy_tests`,
  `// spec: CRAWL-126`). Ein Leser hält einen älteren Stand.
  - `checkpoint_within_waits_for_the_deadline_then_restores_the_busy_timeout`: `checkpoint_within` mit 300 ms
    Frist wartet mindestens die Frist und gibt deutlich unter 3 s auf (der Busy-Timeout wären 5 s) mit `busy = 1`.
    Danach meldet `PRAGMA busy_timeout` wieder 5000, auch wenn er vorher auf 0 stand. Die Frist ist zurückgesetzt, und
    ohne Leser läuft der nächste Checkpoint durch.
  - `the_bounded_handler_gives_up_at_once_without_a_deadline`: Nach `set_bounded_busy` meldet `PRAGMA busy_timeout` 0,
    der Handler ist also gesetzt. Ohne Frist gibt er sofort auf statt nach 5 s.
- **F4:** `runtime_lint.rs` liest die Versionsanforderung von `rusqlite` aus dem Workspace-`Cargo.toml`
  (`workspace_rusqlite_version`), statt sie fest einzutragen.
- **F6:** `.github/workflows/integrations.yml` läuft jetzt auch bei Änderungen an `Cargo.lock` und `Cargo.toml`, für
  `push` und `pull_request`.

## 4. Tests (Build-Server `openspider-build`, Worktree `m3-depsec`)

Alle Rust-Läufe mit `CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0` und `--locked`.

| Befehl | Ergebnis |
|---|---|
| `cargo deny check` | `advisories ok, bans ok, licenses ok, sources ok`; eine Warnung `yanked` für `clipboard-win` 5.4.1, schon auf `main` vorhanden (siehe 5) |
| `cargo fmt --all --check` | OK |
| `cargo clippy --workspace --all-targets -- -D warnings` | OK |
| `cargo test --workspace --no-fail-fast` (über `~/ws-slot.sh`) | **4492 bestanden, 0 fehlgeschlagen, 51 ignoriert**, 617 Suiten, Exit 0 (Endstand `8fb0dc0b`) |
| `os-parity run --all` | „Paritätslauf: bestanden“, 218/218 Vergleiche ok, 3 vorgesehene Auslassungen, 10 Sites `present` |
| `npm run -w apps/desktop typecheck` | OK |
| `npm run -w apps/desktop test` | CSS-Lint ok, Vitest 122 Dateien, 1039 Tests bestanden |
| Playwright MCP-Suite `e2e/mcp` (`dialog`, `bridge` mit echter `os-app-bridge`), `--workers=1 --retries=0` | Chromium 12/12 (10,6 min), WebKit 12/12 (10,9 min) |
| Playwright `beta/n04-mcp.spec.ts` (echte Bridge) | Chromium 1/1, WebKit 1/1 |

**Nachtrag (A1, F1–F6), Server, gezielt:**

| Befehl | Ergebnis |
|---|---|
| `cargo fmt --all --check` | OK |
| `cargo clippy --workspace --all-targets -- -D warnings` | OK |
| `cargo test -p os-store -p os-app --no-fail-fast` | 885 bestanden, 0 fehlgeschlagen, 13 ignoriert, 155 Suiten, darunter `wal_live_owner` 12/12, `wal_live_owner_app` 7/7, `registry_management` 15/15, `compat_archive` (os-store) 4/4, `runtime_lint` 3/3, `bounded_busy_tests` 2/2 |
| Gegenprobe F2 (`read_header` über `std::fs`, nur Server) | neuer Sperrtest 3 von 3 Läufen rot, alle übrigen 11 Tests von `wal_live_owner` grün (siehe 3a) |

Der Workspace-Test wurde nicht wiederholt: Geändert sind nur Tests in os-store und os-app sowie ein Pfadfilter der CI.
Nach dem Lint-Fix (`as_chunks` in `compat_archive.rs`) liefen clippy und `compat_archive` noch einmal, beide OK.

`cargo fmt`, `cargo deny check` und `cargo clippy` liefen am Endstand noch einmal, alle OK. Ein Zwischenlauf des
Workspace-Tests brach ab, weil `disk-guard` bei 20 GB freiem Platz unveränderte Test-Binaries gelöscht hatte, die
älter als 60 Minuten waren („never executed“, 0 Testfehler). Die Wiederholung oben hält die Binaries frisch.

Erster Workspace-Lauf (vor den beiden Testanpassungen): 4490 bestanden, 2 fehlgeschlagen (`k83_duplicate_plans`
Gegenprobe, `runtime_lint`), 51 ignoriert, 617 Suiten. Beide Fehler sind in Abschnitt 2 erklärt und behoben. Gegenprobe
zur Anpassung: Ohne das Abschalten von EXISTS-to-JOIN ist die Gegenprobe unter SQLite 3.54 rot (erster Lauf), mit dem
Abschalten grün.

## 5. Offene Punkte

1. **Windows und macOS nicht übersetzt:** Der Server baut nur Linux. Die Windows-Zweige in `open.rs` habe ich am
   Quelltext von rusqlite 0.41.1 geprüft (Signatur, `Name for &str`, VFS im Amalgamat). Die CI nach dem Merge muss das
   bestätigen.
2. **`clipboard-win` 5.4.1 ist gezogen (yanked)**, über `arboard` → `tauri-plugin-clipboard-manager`. Nur eine
   Warnung, schon vor diesem Paket vorhanden und kein Advisory. Nicht angefasst (kein allgemeines Update). Vorschlag:
   eigenes kleines Paket.
3. **Lockfile-Kanten:** Cargo 1.99 hängt bei jedem `cargo update -p` fremde `windows-sys`-Kanten um (siehe 2). Das nächste
   gezielte Update sollte wieder nur die betroffenen Blöcke übernehmen.

## Release Notes beta.4

- **EN:** Security: the bundled SQLite is updated to 3.54.1 (CVE-2026-119375) and the MCP library rmcp to 3.9.0
  (RUSTSEC-2026-0457, -0461, -0462); OpenSpider behaves as before.
- **DE:** Sicherheit: Das mitgelieferte SQLite ist auf 3.54.1 aktualisiert (CVE-2026-119375), die MCP-Bibliothek rmcp
  auf 3.9.0 (RUSTSEC-2026-0457, -0461, -0462); OpenSpider verhält sich wie bisher.
