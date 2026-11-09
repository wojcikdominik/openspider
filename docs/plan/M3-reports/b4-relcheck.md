# B4-RELCHECK – Auflagen aus der Prüfung von B4-MSIUPD (vor 1.0)

Stand: 09.11.2026. Worktree `c-relcheck` (Mac) bzw. `m3-relcheck` (Server), Branch `m3/relcheck`, Basis lokales `main`
`72c84d3f` (B4-MSIUPD und M3-Q02 gemergt, nicht gepusht). Soll: `docs/spec/06-output-ops.md` OUT-250, OUT-253,
OUT-262. Nicht nach origin gepusht.

## Ergebnis in Kürze

R1, R2, R3, die WARN-Zeile, die Berichtskorrektur und der Handbuchsatz sind umgesetzt. Zwei Punkte weichen vom Auftrag ab
oder gehen darüber hinaus:

1. **Handbuch:** Der Satz nennt `OPENSPIDER_DISABLE_UPDATE_CHECK=1`, nicht `DISABLEUPDATECHECK=1` bzw. die Richtlinie.
   Grund: Beides ist im aktuellen Stand nicht wirksam (siehe Abschnitt 5). Ein Handbuch für 1.0 soll Administratoren
   nichts zusagen, was nicht wirkt.
2. **Mögliches Rennen beim Start behoben:** Die App meldete die Bundle-Kennung erst nach `App::new`. `App::new` startet aber
   schon die erste Update-Prüfung als Aufgabe. Die Meldung kommt jetzt vor `App::new` (Abschnitt 4).

## 1. R1 – Bundle-Kennung im Windows-Smoke

Neu ist `scripts/release/check-bundle-marker.ps1 -InstallDir <ordner> -Kind msi|nsis`. Es startet kein Programm und
vergleicht nur Bytes. Die Datei wird als ISO-8859-1 gelesen, jedes Byte ist also genau ein Zeichen, und dann ordinal
durchsucht.

- **Welche EXE:** Geprüft wird die eine EXE im Installationsordner, die eine `__TAURI_BUNDLE_TYPE_VAR_*`-Zeichenkette
  enthält. Die CLI `openspider.exe` und der NSIS-Uninstaller scheiden aus. Der Dateiname der Desktop-EXE wird nicht
  vorausgesetzt. Ein `OpenSpider.exe` wie im Auftrag kann es neben `openspider.exe` nicht geben, weil NTFS Groß- und
  Kleinschreibung nicht unterscheidet. Tauri v2 nimmt ohne `mainBinaryName` den Cargo-Namen, also vermutlich
  `openspider-desktop.exe`. Unter Windows belegt ist das nicht. Keine oder mehr als eine solche EXE macht den Lauf rot.
- **Regel:** `__TAURI_BUNDLE_TYPE_VAR_UNK` darf nicht mehr vorkommen. Die erwartete Kennung (`_NSS` bzw. `_MSI`) muss
  häufiger vorkommen als die des anderen Windows-Pakets. Ein einfaches „enthält `_MSI`“ reicht nicht. Die Vergleichstexte
  der Match-Arme in `tauri_utils::platform::bundle_type()` stehen in der EXE, im Linux-Debug-Build von
  `openspider-desktop` je einmal (`UNK`, `MSI`, `NSS`, `DEB`, `RPM`, `APP`, nachgezählt auf dem Server). Der Bundler
  ersetzt das erste `…_UNK` (`tauri-bundler-2.10.2/src/bundle.rs:90-96`). Gepatcht gilt also: `UNK` 0, erwartet n+1,
  anderes Paket n. Werden die Vergleichstexte im Release wegoptimiert, gilt das mit n = 0 genauso.
- **Meldungen:** „… enthält noch `…_UNK`. tauri-bundler hat die EXE nicht gepatcht; die App gälte als unbekannte
  Installationsart und bekäme keine Updates“ bzw. „… trägt nicht `…_MSI` (falsche oder fehlende Kennung …)“. Beide
  nennen die Fundstellen aller Kennungen. Im Erfolgsfall erscheint eine OK-Zeile mit den Zählungen.
- **Aufrufe:**
  - `apps/desktop/src-tauri/windows/smoke.ps1`: im Schritt „Stille Installation und Deinstallation beider Installer“ von
    `release-windows.yml`, nach jeder Installation, NSIS und MSI je zweimal.
  - `scripts/smoke/windows.ps1`: R03, `smoke-installed.yml`, mit den hochgeladenen Release-Artefakten.
  - Die Zusammenfassung in `release-windows.yml` nennt die Prüfung.
- **Nicht ausgeführt:** Hier gibt es kein Windows und kein `pwsh`, auch nicht auf dem Server. Belegt wird das Skript erst
  im nächsten Release-Trockenlauf. Bewusst einfach gehalten: `[System.Text.Encoding]::GetEncoding(28591)` läuft in
  Windows PowerShell und pwsh, `ordered`-Dictionary, keine Pipeline-Tricks. `actionlint` 1.7.12 für
  `release-windows.yml` und `smoke-installed.yml`: ok.

## 2. R2 – Vertragstest im Desktop-Crate

`apps/desktop/src-tauri/src/update.rs`, `tests::bundle_type_display_matches_the_installer_detection_in_os_app`
(`// spec: OUT-262`):

- `BundleType::Msi.to_string() == "msi"` und `BundleType::Nsis.to_string() == "nsis"`.
- Diese Zeichenketten ergeben über `Installer::detect("windows", …)` `Msi` bzw. `Nsis`.
- Mit der Fixture `tests/fixtures/update/release-manifest.json` wählt `UpdateContext::select` die `.msi`- bzw.
  `-setup.exe`-URL.

Ändert Tauri die Darstellung, schlägt der Test fehl, bevor jede Windows-Installation als unbekannt gilt.

## 3. R3 – `tests/install/update_matrix.py`

- `updater_target(installer)` folgt unter Windows der Installationsart: `msi` → `windows-x86_64-msi`, `nsis` →
  `windows-x86_64-nsis`. Andere Werte lösen `ValueError` aus. Unter macOS wird der Wert ignoriert. Der falsche Kommentar
  „immer nsis“ ist entfernt.
- Aufrufer: `main` übergibt `--installer`. Die Nachbildung `update_replica` hatte für MSI schon fest `-msi` und nutzt
  jetzt dieselbe Funktion. Für einen MSI-Lauf prüft Q-03 damit Signatur und Kippbyte am MSI-Artefakt, das der
  Updater einer MSI-Installation wirklich lädt.
- `tests/install/test_matrix_helpers.py`, Klasse `UpdaterTarget`, vier Tests:
  - Windows-Zuordnung und `ValueError`.
  - Die Schlüssel stehen in der Release-Manifest-Fixture, mit der richtigen Dateiendung.
  - macOS liefert unabhängig vom Argument den Schlüssel der Architektur.
  - Die Nachbildung greift für `msi` auf `windows-x86_64-msi` zu (Fixture ohne NSIS-Eintrag).
- `test_cli_integration.py` ist unverändert. Die CLI nimmt jeden Eintrag ihrer Plattform (`AnyReportOnly`), `-nsis`
  passt dort weiter.

## 4. WARN-Zeile und Startreihenfolge

- **`os-app` (`update/policy.rs`):**
  - `unknown_installer_warning(os, bundle_type)` liefert den Text, wenn `Installer::detect` „unbekannt“ ergibt. Das ist
    nur unter Windows der Fall, ohne Kennung oder mit fremder Kennung.
  - `log_unknown_installer()` schreibt den Text mit `tracing::warn!` ins App-Log. Das Verhalten ändert sich nicht. Die
    Prüfung schaltet sich wie bisher über `Disabled::UnknownInstaller` ab.
  - Text: „Installationsart unbekannt: keine MSI- oder NSIS-Kennung in der EXE (gefunden: keine); automatische Updates
    sind abgeschaltet, manueller Download: https://github.com/wojcikdominik/openspider/releases/latest“.
  - Die Systemabbildung `macos` → `darwin` liegt jetzt in `current_os()` und wird von `UpdateContext::default()`
    mitbenutzt.
- **Desktop:** `update::register_installer()` meldet die Kennung. `lib.rs` ruft es als erstes nach
  `enable_session_tracking()` auf, vor `App::new`. Die WARN-Zeile kommt im `setup` des Update-Plugins. Dann ist das Log
  eingerichtet (`logging::init_for_app` in `App::new`), und nur die Desktop-App kommt dorthin. CLI oder MCP über STDIO
  schreiben die Zeile also nicht.
- **Warum die Reihenfolge:** `App::new` → `services::start` → `commands::update::on_startup` startet mit
  `check_updates_on_startup` (in `AppConfig::from_env` an) sofort `run_check` als Aufgabe. Dort liest
  `..Default::default()` die registrierte Kennung. Bisher meldete `update::init()` sie erst nach `App::new`. Die
  Startprüfung konnte unter Windows deshalb `Disabled(UnknownInstaller)` ergeben und den Hinweis auf den manuellen
  Download zeigen, obwohl die EXE richtig gekennzeichnet war. Beobachtet ist das nicht. Es hängt davon ab, ob die
  Aufgabe vor dem Weiterlaufen von `run()` startet. Jetzt ist die Kennung vor `App::new` gesetzt.
- **Test:** `policy::installer_tests::warns_at_startup_only_when_the_windows_installer_is_unknown` (`// spec: OUT-262`,
  läuft in `os-app` und `os-cli`). Der Text erscheint ohne Kennung und mit fremder Kennung, aber nicht bei `msi`/`nsis`,
  macOS oder Linux. Die Zeile wird mit einem eigenen `fmt`-Subscriber mitgeschnitten und hat die Stufe `WARN`. Bei
  bekannter Art wird nichts geschrieben.

## 5. Berichtskorrektur und Handbuch

- **`docs/plan/M3-reports/b4-msiupd.md`, offene Frage 2:** korrigiert nach Fables Befund.
  - Der Updater startet `msiexec /i … /passive /promptrestart` per `ShellExecuteW("open")` und beendet die App sofort
    (`updater.rs:1028-1050`, `/passive` aus `config.rs:31`).
  - Die UAC-Abfrage kommt von Windows Installer. Bei Ablehnung endet `msiexec` mit 1602. Die App ist schon zu und zeigt
    nichts an.
  - Ergänzt gegenüber dem Auftrag: „Beim nächsten Start erneut angeboten“ gilt erst nach Ablauf der 24-Stunden-Sperre.
    `reserve` vergleicht mit dem gespeicherten `last_check`. Sofort geht es über Hilfe → „Nach Updates suchen“.
- **Handbuch DE/EN, Abschnitt Windows:** Ein Satz steht direkt nach der MSI-Beschreibung, sonst ist nichts geändert. MSI-Updates
  verlangen ebenfalls Administratorrechte, Windows fragt per Benutzerkontensteuerung. Wer Versionen selbst verteilt,
  schaltet die Prüfung mit `OPENSPIDER_DISABLE_UPDATE_CHECK=1` ab. Der Satz verweist auf den Abschnitt Updates.
  `node website/check-site.mjs` auf dem Server: ok.
- **Warum nicht `DISABLEUPDATECHECK=1` bzw. Richtlinie:**
  - `SystemPolicy` in `policy.rs` liest weder HKLM noch HKCU. `machine()` und `user()` geben `Missing` zurück, wirksam
    ist nur die Umgebungsvariable.
  - Die MSI-Quellen (`apps/desktop/src-tauri/windows/*.wxs`, `build-config.ps1`) kennen keine Eigenschaft
    `DISABLEUPDATECHECK`.
  - Beides ist als N-Paket „Richtlinienleser“ in `docs/plan/M2-backlog.md` vermerkt, in `q02.md` steht es als
    Handprüfung Q03 offen.
  - **Bitte entscheiden:**
    - Entweder kommt der Richtlinienleser samt MSI-Eigenschaft vor 1.0. Dann wird der Satz erweitert, etwa „… mit
      `msiexec … DISABLEUPDATECHECK=1`, per Richtlinie `DisableUpdateCheck` oder der Umgebungsvariable …“.
    - Oder OUT-253 bleibt für 1.0 bei der Umgebungsvariable.

## 6. Gates (Server, Linux, `CARGO_PROFILE_DEV_DEBUG=0`, `CARGO_PROFILE_TEST_DEBUG=0`, `CARGO_INCREMENTAL=0`)

- `cargo fmt --all --check`: ok
- `cargo clippy --workspace --all-targets -- -D warnings`: ok, einschließlich `openspider-desktop`
- Gezielt: `os-app` und `os-cli` `installer_tests` 4/4, `openspider-desktop` `update::tests` 1/1
- `cargo test --workspace --locked -j 4` nach `ws-slot`: 4495 bestanden, 0 fehlgeschlagen, 51 ignoriert (Exit 0)
- `python3 -W error::ResourceWarning -m unittest discover -s tests/install`: 41 Tests, ok, 4 übersprungen (`Q02_CLI`
  nicht gesetzt). Gleiches Ergebnis auf dem Mac.
- `actionlint` 1.7.12: ok. `node website/check-site.mjs`: ok.
- PowerShell: kein Lauf möglich, Prüfung im nächsten Release-Trockenlauf (`release-windows.yml`, Schritt „Stille
  Installation …“).

`target/` im Server-Worktree ist danach gelöscht.

## 7. Offene Punkte

1. Handbuch und Richtlinie: siehe Abschnitt 5, Entscheidung nötig.
2. R1 ist ohne Windows-Lauf geschrieben. Bricht der Trockenlauf mit „genau eine Desktop-EXE … erwartet“ ab, bitte die
   genannten EXE-Dateien ansehen. Die Auswahl beruht auf der Annahme, dass nur die App-EXE Tauri-Kennungen enthält.
