# macOS: Signieren, Notarisieren und Updater-Artefakte (M3-R01)

Der Workflow `.github/workflows/release-macos.yml` baut die Desktop-App, signiert sie mit einer **Developer ID Application**, notarisiert sie bei Apple, heftet das Ticket und liefert **DMGs für arm64, x64 und Universal als Actions-Artefakte** (14 Tage). App und CLI sind nativ; Chromium wird für die Beta nicht gebündelt (vorläufige Entscheidung K3-01 vom 30.10.2026). Er veröffentlicht nichts: kein GitHub Release, kein Upload außerhalb von GitHub Actions.

## Voraussetzungen (einmalig, beim Apple-Developer-Konto)

1. **Apple Developer Program** (Mitgliedschaft aktiv). Die **Team-ID** steht unter developer.apple.com → Account → Membership details.
2. **Zertifikat „Developer ID Application"** erzeugen (nicht „Apple Development" oder „Mac App Distribution"):
   - Xcode → Settings → Accounts → Team wählen → Manage Certificates → `+` → *Developer ID Application*, oder
   - developer.apple.com → Certificates → `+` → *Developer ID Application* (CSR aus Schlüsselbundverwaltung → Zertifikatsassistent → Zertifikat von einer Zertifizierungsstelle anfordern).
3. **Als .p12 exportieren:** Schlüsselbundverwaltung → Anmelde → Meine Zertifikate → das Zertifikat „Developer ID Application: Name (TEAMID)" aufklappen, Zertifikat samt privatem Schlüssel markieren → Rechtsklick → *Exportieren* → Format .p12, ein Exportpasswort vergeben (das wird `APPLE_CERTIFICATE_PASSWORD`).
4. **Base64 erzeugen** (eine Zeile, in die Zwischenablage): `base64 -i zertifikat.p12 | pbcopy`. Die .p12-Datei danach sicher löschen oder in den Passwortmanager legen.
5. **App-spezifisches Passwort** für die Notarisierung: account.apple.com → Anmeldung und Sicherheit → App-spezifische Passwörter → neues erzeugen (Name z. B. „OpenSpider notarytool"). Das normale Apple-ID-Passwort funktioniert nicht.
6. **Name der Signaturidentität** auslesen: `security find-identity -v -p codesigning` → Eintrag in der Form `Developer ID Application: Vorname Nachname (ABCDE12345)`.

## Secrets und Variablen im GitHub-Repo

Repo → Settings → Secrets and variables → Actions.

| Name | Art | Inhalt |
|---|---|---|
| `APPLE_CERTIFICATE` | Secret | .p12 als Base64 (Schritt 4) |
| `APPLE_CERTIFICATE_PASSWORD` | Secret | Exportpasswort der .p12 (Schritt 3) |
| `APPLE_ID` | Secret | Apple-ID (E-Mail) des Developer-Kontos |
| `APPLE_PASSWORD` | Secret | App-spezifisches Passwort (Schritt 5) |
| `KEYCHAIN_PASSWORD` | Secret | frei gewähltes, zufälliges Passwort für den temporären Schlüsselbund im Runner |
| `APPLE_SIGNING_IDENTITY` | Variable | vollständiger Name der Identität (Schritt 6) |
| `APPLE_TEAM_ID` | Variable | Team-ID, 10 Zeichen |

Per CLI, ohne dass ein Wert in der Shell-History oder im Log landet:

```sh
base64 -i zertifikat.p12 | gh secret set APPLE_CERTIFICATE
gh secret set APPLE_CERTIFICATE_PASSWORD     # fragt den Wert verdeckt ab
gh secret set APPLE_ID
gh secret set APPLE_PASSWORD
openssl rand -base64 24 | gh secret set KEYCHAIN_PASSWORD
gh variable set APPLE_SIGNING_IDENTITY --body 'Developer ID Application: Vorname Nachname (ABCDE12345)'
gh variable set APPLE_TEAM_ID --body 'ABCDE12345'
```

Fehlt eines dieser Apple-Elemente, endet der Signierjob mit grünem Status, einer Warnung (`::warning`) und einer Zusammenfassung, welche Namen fehlen. Es entstehen dann keine Release-Artefakte. Die mit `run_tests=true` aktivierten Plattformtests laufen trotzdem; deren echte Fehler bleiben rot.

## Updater-Schlüssel und Kanal

| Name | Art | Zweck |
|---|---|---|
| `TAURI_SIGNING_PRIVATE_KEY` | Secret | privater Tauri/minisign-Schlüssel für `.app.tar.gz.sig` |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | Secret | Passwort des privaten Schlüssels; leer bei unverschlüsseltem Schlüssel |
| `OPENSPIDER_UPDATER_PUBKEY` | Variable | öffentlicher Schlüssel, wird beim Build eingebettet |

Fehlt der private oder öffentliche Schlüssel, werden Updater-Artefakte ausdrücklich übersprungen; signierte DMGs entstehen weiterhin. Ein falscher Schlüssel oder ein falsches Passwort ist dagegen ein Buildfehler. Schlüssel werden weder gedruckt noch in Artefakte geschrieben. Die temporäre Build-Konfiguration enthält nur den öffentlichen Schlüssel und den Endpunkt.

Seit 1.0.1 (K3-16, OUT-254) gibt es keinen Build-Kanal mehr: Die temporäre Konfiguration trägt beide festen Endpunkte, `https://github.com/wojcikdominik/openspider/releases/latest/download/latest.json` (stabil) und `https://github.com/wojcikdominik/openspider/releases/download/update-beta/latest-beta.json` (Beta); die App wählt zur Laufzeit nach `updates.channel`. Die Eingabe `channel` und `OPENSPIDER_UPDATE_BETA` entfallen. Der Build-Job erhält `OPENSPIDER_UPDATER_PUBKEY` aus der Repository-Variable. Der Workflow erzeugt keine Update-Manifeste und veröffentlicht nichts. R04 ordnet die architekturspezifischen Archive den Schlüsseln `darwin-aarch64` und `darwin-x86_64` zu; das Universal-Archiv bleibt optionales Zusatzasset. Die URL- und Signaturprüfung zur Laufzeit liegt bei U01/U02.

## Auslösen und Schnittstelle für R04

- `workflow_dispatch`: `arch=all` (Standard), `aarch64`, `x86_64` oder `universal`; `version` optional; `run_tests` (Boolean, Standard `true`).
- `workflow_call`: `version` (String, optional) und `run_tests` (Boolean, Standard `true`); erzeugt immer alle drei Architekturen. Secrets ausdrücklich übergeben oder `secrets: inherit` verwenden.
- `version` muss der bereits vorbereiteten Workspace-Version entsprechen. Version und Lockfile setzt der Aufrufer vor dem Build; der Workflow prüft den Gleichstand, ohne Abhängigkeiten zu verändern.
- Ausgabe `artifact_names`: JSON-Liste der tatsächlich hochgeladenen `openspider-macos-aarch64`, `openspider-macos-x86_64`, `openspider-macos-universal`; bei fehlender Apple-Konfiguration `[]`. Der Aufrufer benötigt `contents: read` und `actions: read`.
- Kein Tag-Auslöser: Release-Tags verarbeitet künftig ausschließlich R04 (`release.yml`).
- Push-Tests liegen in `ci.yml`; dieser Release-Workflow hat keinen Push- oder Zeitplan-Auslöser. Mit `run_tests=true` prüft er Rust-Workspace und Frontend auf `macos-14` (arm64) und `macos-15-intel`. `run_tests=false` überspringt diese Tests für bereits geprüfte R04-Builds; fehlgeschlagene aktivierte Tests blockieren den Build weiterhin. macOS 13 bleibt die Mindestplattform.
- Alle verwendeten Actions sind auf Commit-SHAs fixiert; der Kommentar nennt den aufgelösten Tag bzw. den `stable`-Branch von `dtolnay/rust-toolchain`.

```sh
gh workflow run release-macos.yml --ref m3/r01 -f arch=all
# Mit vorbereiteter Version:
gh workflow run release-macos.yml --ref m3/r01 -f arch=all -f version=1.0.0-beta.1
```

Jedes Artefakt enthält ein DMG, `verify-app.log`, `verify-dmg.log` und bei vorhandenen Updater-Schlüsseln genau ein `OpenSpider_<arch>.app.tar.gz` mit zugehörigem `.sig`. Die Architekturnamen verhindern Namenskollisionen beim Zusammenführen der Release-Assets. Die drei Archive kommen aus getrennten Tauri-Zielverzeichnissen und bleiben zusammen mit ihrer Signatur unverändert.

## Ablauf im Workflow

1. Native Tests (Rust fmt/Clippy/Workspace-Tests, Frontend-Typen/Tests/statische Prüfungen mit Lizenzprüfung, Chromium/WebKit als Testwerkzeuge). Für die Tauri-Testkonfiguration wird zuerst die CLI als Sidecar gebaut. Tests lesen nur den Memory-SecretStore.
2. Apple-Konfiguration prüfen, Checkout, Rust mit beiden Darwin-Zielen, Node 22 und `npm ci`.
3. Workspace-Version prüfen und beide Kanal-Endpunkte samt öffentlichem Updater-Schlüssel über eine temporäre Tauri-Konfiguration setzen; ohne Schlüssel `createUpdaterArtifacts=false`.
4. CLI `openspider` mit `cargo build -p os-cli --bin openspider --release --target … -j 4 --locked` bauen. Sidecar-Dateinamen tragen den Ziel-Triple-Suffix. Für Universal erzeugt `lipo -create` zusätzlich `openspider-universal-apple-darwin` aus beiden CLI-Binaries. Tauri baut die Universal-App selbst aus beiden Zielen.
5. Temporären Schlüsselbund anlegen, .p12 importieren, Codesign-Zugriff erlauben. Kein zweiter Zertifikatsimport durch Tauri.
6. Tauri baut `app,dmg`, signiert die App einschließlich Sidecar mit Hardened Runtime, notarisiert und heftet das App-Ticket. Tauri erzeugt und signiert auch die Updater-Archive, sofern aktiviert. `MACOSX_DEPLOYMENT_TARGET=13.0`; Builds bleiben `--locked`, Parallelität vier.
7. DMG separat signieren, notarisieren und heften; zeitweilige Fehler beim Stapeln werden begrenzt wiederholt.
8. Bundleprüfung und nativer Starttest der App sowie `openspider --version`; danach `verify-macos.sh` für App und DMG einschließlich der App im schreibgeschützt eingehängten DMG.
9. Actions-Artefakte hochladen, Schlüsselbund immer löschen. Der Universal-DMG-Starttest läuft zusätzlich auf beiden Runnern (arm64 und Intel). Fehlende Artefakte werden nur bei unvollständiger Apple-Konfiguration grün übersprungen.

Kein Schritt gibt geheime Werte aus; kein Shell-Tracing. Die verwendeten Werkzeuge bleiben die vorhandenen Rust-/Tauri-/Apple-Werkzeuge; keine zusätzliche Produktabhängigkeit.

Die Tauri-Verfahren für [Sidecars](https://v2.tauri.app/develop/sidecar/), [macOS-Signierung](https://v2.tauri.app/distribute/sign/macos/) und [Updater-Artefakte](https://v2.tauri.app/plugin/updater/) bilden die Grundlage.

## Prüfen

Lokal, nach dem Download und Entpacken des Artefakts (der Download setzt das Quarantäne-Attribut, genau wie bei einem späteren Nutzer):

```sh
scripts/release/verify-macos.sh ~/Downloads/OpenSpider_0.1.0_universal.dmg
scripts/release/verify-macos.sh /Applications/OpenSpider.app
```

Geprüft wird:

- `codesign --verify --deep --strict --verbose=2` (App) bzw. `--strict` (DMG)
- `check-macos-bundle.sh` sucht nach Dateiinhalt jede Mach-O-Datei, unabhängig von Name und Ausführungsrechten. Für jede Architektur: strikte Signaturprüfung, kein Ad-hoc-Signer, Hardened Runtime, sicherer Zeitstempel und leere Entitlements.
- Bei `MACOS_EXPECTED_ARCH=universal` müssen alle Mach-O-Dateien genau `x86_64 arm64` enthalten; bei `aarch64` bzw. `x86_64` genau das jeweilige Ziel.
- `Info.plist`: Bundle-ID, `LSMinimumSystemVersion=13.0`, CLI vorhanden, App-Icon und Editor-Zuordnungen für alle fünf OpenSpider-Dateitypen (OUT-272 [M3]).
- `--smoke` prüft native Runner-Ausführung (`sysctl.proc_translated=0`, Intel ohne diesen Schlüssel), CLI-Version und zehn Sekunden laufende App. Universal wird auf beiden Runnern gestartet. Manuelle Aktivitätsanzeige/Gatekeeper-Erststart und echter Dateidoppelklick bleiben Teil von Q03.
- `spctl -a -t exec -vv` für die App bzw. `spctl -a -t open --context context:primary-signature -vv` für das DMG; erwartet wird `accepted` mit `source=Notarized Developer ID`
- `xcrun stapler validate`
- beim DMG zusätzlich: Einhängen (schreibgeschützt) und dieselbe App-Prüfung für die enthaltene App

Das Skript läuft alle Prüfungen durch, gibt ein Protokoll aus und endet mit Exitcode 0 nur, wenn alles besteht (2 bei falscher Bedienung). Für einen lokalen Build ohne Notarisierung: `--no-notarization` (Gatekeeper-Quelle und Heftung werden dann nur gemeldet).

Von Hand, falls nötig:

```sh
codesign -dvv --entitlements - /Applications/OpenSpider.app
xcrun notarytool history --apple-id "$APPLE_ID" --team-id "$APPLE_TEAM_ID"   # fragt das Passwort ab
xcrun notarytool log <submission-id> --apple-id … --team-id …                 # Begründung bei „Invalid"
```

Die Fixtures laufen auf Linux/macOS über `cargo test -p os-cli --test macos_bundle -j 4 --locked` oder direkt `python3 -m unittest discover -s tests/release -p 'test_macos_*.py' -v`. Die Signierwerkzeuge sind dort kontrollierte Testdoubles; echte Signatur-/Notarisierungsbelege liefert erst der macOS-Lauf. `--plist-only` erlaubt die reine Plist-Prüfung ohne Apple-Werkzeuge.

## Entitlements

`apps/desktop/src-tauri/entitlements.plist` ist bewusst leer (nur ein leeres `<dict/>`); sie wird ausschließlich in der gemeinsamen `tauri.conf.json` unter `bundle.macOS.entitlements` eingebunden, zusammen mit `hardenedRuntime: true` und `minimumSystemVersion: "13.0"` (Mindestplattform laut `docs/ARCHITECTURE.md`). `tauri.macos.conf.json` ergänzt nur die Bundle-Ziele `app` und `dmg`.

Begründung, warum keine Ausnahmen nötig sind:

- **JIT und ausführbarer Speicher** (`com.apple.security.cs.allow-jit`, `allow-unsigned-executable-memory`): Das JavaScript der Oberfläche läuft in WKWebView, im von Apple signierten WebContent-Prozess, nicht im Prozess der App. Die App selbst enthält keinen JS- oder Wasm-Interpreter.
- **Netzwerk** (`com.apple.security.network.client`): betrifft nur die App Sandbox. OpenSpider ist nicht sandboxed; ausgehende Verbindungen sind erlaubt.
- **Bibliotheksvalidierung / DYLD-Variablen:** Es werden keine fremd signierten Bibliotheken geladen. Chromium für das JS-Rendering (`os-render`) ist ein externer Kindprozess der Nutzerinstallation, für den die Hardened Runtime der App nicht gilt.
- **Schlüsselbund** (`keyring`): Zugriff auf den Anmelde-Schlüsselbund braucht keine Entitlements. `keychain-access-groups` (Data-Protection-Keychain) bräuchte ein Provisioning Profile und wird nicht verwendet.

Kommt später eine Funktion dazu, die eine Ausnahme braucht (z. B. ein eingebetteter Interpreter mit JIT), wird sie einzeln in die Plist eingetragen und hier begründet. Stürzt die signierte App beim Start ab, zeigt `log stream --predicate 'process == "OpenSpider"'` die verletzte Regel.

## Fehlersuche

| Meldung | Ursache |
|---|---|
| Lauf endet nach Schritt 1 mit Warnung | ein Secret oder eine Variable fehlt (Namen stehen in der Warnung) |
| „Die Identität aus APPLE_SIGNING_IDENTITY steht nicht im importierten Zertifikat" | falscher Zertifikatstyp (nicht *Developer ID Application*), Variable weicht im Namen ab, oder die .p12 enthält den privaten Schlüssel nicht |
| `security import` scheitert | falsches `APPLE_CERTIFICATE_PASSWORD` oder Base64 mit Zeilenumbruch-/Kopierfehlern |
| `notarytool`: `Invalid credentials` / 401 | `APPLE_PASSWORD` ist nicht das App-spezifische Passwort, oder `APPLE_ID`/`APPLE_TEAM_ID` passen nicht zusammen |
| Notarisierung „Invalid" | `notarytool log` zeigt die Dateien (meist unsignierte Binärdatei oder fehlender Zeitstempel/Hardened Runtime); der Workflow gibt das Log aus |
| `verify-macos.sh`: „Quelle ist nicht Notarized Developer ID" | Tauri hat nicht notarisiert (Variablen am Build-Schritt prüfen) oder das Ticket fehlt |
