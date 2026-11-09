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
  steht für keines eine Änderung. Alle Tests dazu sind grün.
- **Kompilieroptionen:** `libsqlite3-sys` 0.39 lässt für `bundled` nur `SQLITE_ENABLE_MEMORY_MANAGEMENT` weg.
  `sqlite3_release_memory` nutzen wir nicht.
- **VFS-Header-Lesen** (`open.rs`, `read_vfs`/`read_header`): `header_reads_agree_for_short_empty_foreign_and_real_files`
  und `immutable_opens_paths_with_special_characters_without_companions` sind grün.
- **Sperr- und WAL-Tests:** `wal_live_owner` 11/11, `listing_live_owner` 8/8, `wal_live_owner_app` 7/7,
  `registry_management` 15/15, `pause_merge` 4/4, `fix_wal_checkpoint` 1/1.
- **Alte Crawl-Dateien (OUT-255, Fixture-Archiv):** `compat_archive` in os-store, os-cli, os-model und os-app, zusammen
  11 Tests grün. Das Dateiformat von SQLite ist rückwärtskompatibel; 3.54 ändert kein Standardformat.
- **Weitere Hinweise aus den Release-Notes:** 3.53 rundet Fließkomma → Text standardmäßig auf 17 statt 15 Stellen.
  Kein Golden, kein Export und kein Paritätsvergleich hat sich dadurch geändert (Paritätslauf 218/218). 3.54 ruft den
  Autorisierer auch für Funktionen in `DEFAULT`-Klauseln auf; `readonly_sql` erlaubt nur `SELECT`, nicht betroffen.
  3.54 setzt unter Windows mindestens Vista voraus; unsere Mindestversion ist höher.

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
