# B4-MSIUPD – Updater unter Windows: MSI- oder NSIS-Paket

Stand: 09.11.2026. Worktree `c-msiupd` (Mac) bzw. `m3-msiupd` (Server), Branch `m3/msiupd`, Basis `origin/main`
`bb0b27e1`. Soll: `docs/spec/06-output-ops.md` OUT-250 bis 255, OUT-261, OUT-262, OUT-266. Nicht nach origin gepusht.

## Ergebnis in Kürze

**Der Befund aus M3-Q02 bestätigt sich in seiner Folge nicht.** Eine MSI-Installation lädt beim automatischen Update
das MSI-Paket und startet `msiexec`. Eine zweite NSIS-Installation pro Benutzer entsteht nicht. Richtig am Befund ist:
`policy.rs` setzte unter Windows fest `installer = "nsis"`. Dieser Schlüssel diente aber nur als Vorprüfung „gibt es
ein Paket für diese Plattform?“. Das herunterzuladende Paket wählt `tauri-plugin-updater` selbst, und zwar richtig.

Dabei fielen zwei echte, kleinere Lücken auf. Beide sind behoben:

1. Vorprüfung und Download nutzten verschiedene Quellen für die Installationsart. Die Vorprüfung sah immer NSIS, der
   Download die eingebettete Kennung.
2. Für eine EXE ohne Installer-Kennung meldete die Prüfung „Update verfügbar“. Danach scheiterte der Download mit der
   allgemeinen Meldung „Das Update konnte nicht abgeschlossen werden“. Einen Hinweis auf den manuellen Download gab es
   nicht.

## 1. Prüfung am Code und an den Manifesten

**Manifeste** (`scripts/release/manifest.mjs`): Ein Release schreibt in `latest.json` und `latest-beta.json` genau vier
Schlüssel: `darwin-aarch64`, `darwin-x86_64`, `windows-x86_64-nsis` (`*_x64-setup.exe`) und `windows-x86_64-msi`
(`*_x64_<Sprache>.msi`). Fehlt einer davon, entsteht kein Manifest. Einen Schlüssel `windows-x86_64` ohne Installer-Typ
gibt es nie.

**Prüfpfad** (`crates/os-app/src/update/policy.rs`, `fetch`): Die Prüfung verlangte bisher nur, dass
`windows-x86_64-nsis` im Manifest steht. Danach wurde das ganze Manifest gespeichert, ohne Plattformauswahl. Weil jedes
Release beide Windows-Schlüssel hat, war das Ergebnis für MSI und NSIS gleich.

**Download- und Installationspfad** (`apps/desktop/src-tauri/src/update.rs`): Die App reicht das gespeicherte Manifest
an `tauri-plugin-updater` 2.14.1 (`Updater::restore_update`). Das Plugin wählt den Eintrag so (`updater.rs`, `get_urls`):

- Es liest die Installer-Kennung mit `tauri_utils::platform::bundle_type()`.
- Gesucht wird zuerst `{os}-{arch}-{kennung}`, dann `{os}-{arch}`.
- Die Kennung ist der statische Wert `__TAURI_BUNDLE_TYPE` in der EXE.

**Bau** (`.github/workflows/release-windows.yml`): Gebaut wird mit `tauri build --bundles nsis,msi`, also mit
`@tauri-apps/cli` 2.12.2 und `tauri-bundler` 2.10.2. Der Bundler (`bundle.rs`, `bundle_project`) patcht die Kennung je
Pakettyp in die Haupt-EXE: MSI → `__TAURI_BUNDLE_TYPE_VAR_MSI`, NSIS → `…_NSS`. Danach signiert er die EXE über das
`signCommand` aus `build-config.ps1` und packt sie. Vor dem nächsten Pakettyp stellt er die ungepatchte EXE wieder her.
Binary-Patching ist im Release-Lauf nicht abgeschaltet.

**Folge:** In der MSI steckt eine EXE mit der Kennung `msi`. Das Plugin wählt also `windows-x86_64-msi`, erkennt die Bytes
als MSI (`infer::archive::is_msi`) und startet `msiexec /i … /promptrestart`. Für NSIS gilt dasselbe mit
`windows-x86_64-nsis` und `/UPDATE`. Das bestätigt die Lesart der Bibliotheksquellen. Ein Lauf unter Windows war von
hier aus nicht möglich. Diesen Nachweis liefert Q-18 der Matrix `install-test.yml` (Branch `m3/q02`, nicht geändert).

**Restrisiko vor der Änderung:** Kann der Bundler nicht patchen, schreibt er nur eine Warnung (`log::warn!`), und der Bau
läuft weiter. Die EXE hat dann keine Kennung. Das Plugin sucht `windows-x86_64`, findet es nicht und installiert nichts.
Ein falsches Paket kommt also auch dann nicht. Die App bot das Update aber trotzdem an und scheiterte erst beim Download,
mit einer allgemeinen Fehlermeldung.

## 2. Änderung

- **`policy.rs`:** Neu ist `Installer` mit den Werten `Platform`, `Msi`, `Nsis`, `Unknown` und `AnyReportOnly`. Das ersetzt
  das bisherige `Option<String>`.
  - `Installer::detect(os, bundle_type)` gibt unter Windows für `msi` → `Msi`, für `nsis` → `Nsis` und sonst `Unknown`
    zurück. Andere Systeme bekommen immer `Platform`.
  - `UpdateContext::default()` liest die Kennung, die `register_bundle_type` beim Start gemeldet hat.
  - `UpdateContext::select(manifest)` wählt den Eintrag. Es gibt keinen Rückfall auf das andere Installer-Paket.
    `fetch` verlangt jetzt diesen Eintrag statt fest `-nsis`.
  - Bei `Unknown` gibt `disabled()` den neuen Grund `Disabled::UnknownInstaller` zurück. Er wird vor jeder Anfrage
    geprüft: kein Request, keine Reservierung, kein Download. Richtlinie, Einstellung und Schlüsselfehler haben Vorrang
    (OUT-253).
- **Desktop** (`apps/desktop/src-tauri/src/update.rs`): `init()` meldet `tauri::utils::platform::bundle_type()`. Das ist
  dieselbe Quelle, mit der das Plugin beim Download wählt, also gibt es nur eine Quelle für Prüfung und Download.
  `os-app` bleibt Tauri-frei.
- **CLI** (`crates/os-cli/src/update.rs`): Die CLI-EXE wird nicht gepatcht und installiert nie. Sie nutzt
  `Installer::AnyReportOnly` und meldet weiter nur die Verfügbarkeit (Exit 0/10, OUT-250).
- **DTO und UI:** Neu ist `DisableReason::UnknownInstaller` (`unknownInstaller`, ts-rs-Datei angepasst). Der Update-Dialog
  zeigt dafür einen Text auf Deutsch und Englisch. Er sagt, dass nicht sicher erkannt wurde, ob per MSI oder per Setup
  installiert wurde, und nennt `https://github.com/wojcikdominik/openspider/releases/latest` für den manuellen Download.
  Einen Download-Knopf gibt es in diesem Fall nicht.
- Nicht geändert: `manifest.mjs`, Workflows, `install-test.yml`, die Installer-Konfiguration und die Abfrageparameter.
  Die Anfrage enthält weiter nur Version, System und Architektur, nicht die Installationsart.

**Warum die Bundle-Kennung und nicht Installationsort oder Registry:** Installationsort und Registry würden eine zweite
Quelle einführen, die der Download-Pfad nicht kennt. Der Ort ist außerdem nicht eindeutig: `INSTALLDIR="…"` bei MSI und
`/D=<pfad>` bei NSIS sind laut OUT-266 frei wählbar. Ein Registry-Leser bräuchte zudem eine neue Abhängigkeit. Die
Kennung entsteht beim Bau, steckt in der signierten EXE und ist genau die Größe, nach der installiert wird.

## 3. Tests

Alle Tests laufen unter Linux. Die Windows-Umgebung wird über `os`/`arch` und die gemockte Kennung nachgestellt.

| Test | Spec | Prüft |
|---|---|---|
| `policy::installer_tests::detects_msi_nsis_and_unknown_from_the_embedded_bundle_marker` (os-app und os-cli) | OUT-262, OUT-253 | `msi`/`MSI` → MSI, `nsis` → NSIS. Keine, leere oder fremde Kennung (`app`, `deb`, `exe`, `msix`) → unbekannt. macOS und Linux → `Platform`. Die registrierte Kennung landet in `UpdateContext::default()`. |
| `…::selects_only_the_manifest_entry_of_the_own_installer` | OUT-262, OUT-261 | Gegen die neue Fixture `tests/fixtures/update/release-manifest.json` (Form von `manifest.mjs`): MSI → `.msi`-URL, NSIS → `-setup.exe`-URL, macOS je Architektur. Fehlt der eigene Eintrag, wird nicht auf das andere Paket ausgewichen. Bei `Unknown` gibt es keinen Eintrag. CLI-Modus nur für die eigene Plattform. |
| `…::unknown_installer_disables_the_check_before_any_request` | OUT-262 | `Disabled::UnknownInstaller` ohne Reservierung. Richtlinie geht vor. Andere Arten werden nicht abgeschaltet. |
| `update_network::windows_check_follows_the_installer_and_unknown_sends_no_request` | OUT-262, OUT-250 | Mock-Update-Server: MSI und NSIS → verfügbar, gewählter Schlüssel = `target()`. Die Anfrage-URL enthält keine Installationsart. Unbekannt → kein Request. Windows arm64 hat in der Fixture nur NSIS: MSI → Fehler, NSIS → verfügbar. |
| `update_network::app_reports_unknown_installer_as_disable_reason_without_request` | OUT-262 | App-Ebene: `UpdateState::Disabled(UnknownInstaller)`, kein Request, kein `last_check`. |
| `UpdateHost.test.tsx` „tells the user to download manually …“ | OUT-262 | Hinweis mit Link auf Deutsch und Englisch, kein Download-Knopf, kein Download-Aufruf. |

Bestehende Tests mit `installer: None` verwenden jetzt `Installer::Platform` (`update_crawl`, `update_network`,
`os-cli/tests/check_update`). Ihr Verhalten ändert sich nicht.

**Gates (Server, Linux, `CARGO_PROFILE_*_DEBUG=0`, `CARGO_INCREMENTAL=0`):**

- `cargo fmt --all --check`: ok
- `cargo clippy --workspace --all-targets -- -D warnings`: ok, das schließt `openspider-desktop` mit ein
- `cargo test --workspace` nach `ws-slot`: 4427 bestanden, 0 fehlgeschlagen, 51 ignoriert. Vorab
  `-p os-app -p os-cli -p openspider-desktop`: 856/0/4.
- `npm run -w apps/desktop typecheck`: ok
- `npm run -w apps/desktop test`: 122 Dateien, 1038 Tests bestanden

`target/` und `node_modules` im Server-Worktree sind danach gelöscht.

## 4. Offene Fragen (Spec lässt offen)

1. **Verhalten bei unbekannter Installationsart.** Die Spec regelt diesen Fall nicht. Umgesetzt ist: Prüfung abgeschaltet
   mit Hinweis, ohne Netzwerkzugriff. Die Alternative wäre, trotzdem zu prüfen und „Version X verfügbar, bitte manuell
   herunterladen“ zu zeigen. Das braucht einen neuen UI-Zustand. Welche Variante soll gelten?
2. **Automatisches Update bei MSI-Installationen pro Maschine.** `msiexec` braucht Administratorrechte.
   *Korrigiert mit B4-RELCHECK (Prüfung Fable):* Der Updater startet `msiexec /i … /passive /promptrestart` per
   `ShellExecuteW("open")` und beendet die App sofort danach (`tauri-plugin-updater-2.14.1/src/updater.rs:1028-1050`).
   Die UAC-Abfrage kommt von Windows Installer, nicht von der App. Lehnt der Nutzer sie ab, endet `msiexec` mit 1602
   (vom Benutzer abgebrochen). Die App ist dann schon beendet und zeigt nichts an, also auch kein „Update
   fehlgeschlagen“. Die Installation bleibt auf der alten Version. Beim nächsten Start wird das Update erneut angeboten,
   sobald die Startprüfung wieder läuft (höchstens einmal in 24 Stunden, OUT-250). Sofort geht es über Hilfe → „Nach
   Updates suchen“. OUT-253 nennt Softwareverteilung und `DisableUpdateCheck`, sagt aber nicht, ob eine MSI-Installation ohne
   Richtlinie selbst installieren soll oder nur auf die neue Version hinweisen soll. Soll das so bleiben?
3. **Absicherung im Release-Lauf** (außerhalb dieses Pakets): Patcht der Bundler die Kennung nicht, gibt es nur eine
   Warnung. Vorschlag für den Besitzer von `release-windows.yml` bzw. `verify-windows.ps1`: Die EXE aus beiden Installern
   auf `__TAURI_BUNDLE_TYPE_VAR_MSI` bzw. `…_NSS` prüfen. Damit würde die Lücke schon beim Bau auffallen, nicht erst beim
   Nutzer.
