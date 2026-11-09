# Homebrew und winget (M3-P01)

Stand: 31.10.2026. Nur Entwürfe; keine Einreichung, kein Tap, keine Tokens angelegt.
K3-08 ist offen. Vorschlag: Cask-Token `openspider`, Tap
`<Projektkonto>/homebrew-openspider`, winget-ID `OpenSpider.OpenSpider`.
Die endgültige winget-ID muss explizit über `OPENSPIDER_PACKAGE_ID` bzw.
`--package-id` gesetzt werden. `OpenSpider.CITest` dient ausschließlich der CI.

**Nachtrag B3-BREW (Nutzerentscheidung 05.11.2026):** Homebrew-Tap `wojcikdominik/homebrew-openspider`
ab beta.3 über `.github/workflows/homebrew.yml` (Casks `openspider@beta` und ab 1.0 `openspider`,
Generator `scripts/release/homebrew.mjs`, Cask-Text weiterhin aus `packaging/homebrew/generate.mjs`),
Freigabe über das Environment `homebrew`; Einrichtung in `checkliste-release.md`, Abschnitt Homebrew-Tap.
winget erst zur 1.0 mit `publish-packages.yml`. Die Beschreibung unten bleibt für den winget-Teil und
die Smoke-Probe gültig; ihr Homebrew-Teil der Veröffentlichung wird zur 1.0 durch `homebrew.yml` abgelöst.

Cask-Stil (`brew style --cask`, Lauf 42626366623): `depends_on macos: :ventura` (das Symbol allein heißt „mindestens“,
macOS 13; `">= :ventura"` lehnt `Homebrew/OSDependsOn` ab) und mehrzeilige Hashes im `table`-Stil (Werte in einer Spalte,
`Layout/HashAlignment` mit `EnforcedColonStyle: table` in Homebrews RuboCop-Konfiguration).

## Metadaten aus tatsächlichen Assets

R01 kopiert Tauri-DMGs unverändert, R02 die MSI-/NSIS-Dateien, R04 sammelt sie über
`scripts/release/assets.mjs`. Der Generator sucht genau ein Universal-DMG (der Cask
unterscheidet seit der Nutzerentscheidung 04.11.2026 nicht mehr zwischen ARM und Intel;
nur das Universal-DMG wird veröffentlicht, OUT-261) und je einen x64-MSI-/NSIS-Installer.
Die Windows-Muster entsprechen `scripts/release/manifest.mjs`; MSI-Locale und
Dateiversion werden nicht geraten. DMGs je Architektur (nur Actions-Artefakte), die
versionslosen Kopien `OpenSpider-macOS.dmg`/`OpenSpider-Windows-Setup.exe` und separate
CLI-/Updater-Archive werden nicht als Installer gewählt.
Fehlende oder mehrdeutige Assets, eine fehlende Paket-ID und ein nichtleerer
Ausgabeordner führen zum Fehler. SHA-256 stammt immer aus den Asset-Bytes.

```bash
# release-assets enthält bereits heruntergeladene R04-Artefakte.
export OPENSPIDER_PACKAGE_ID=APPROVED.PACKAGE_ID
node packaging/homebrew/generate.mjs --directory release-assets \
  --output package-draft --version 1.0.0 --repository OWNER/REPOSITORY
ruby -c package-draft/openspider.rb
python3 packaging/winget/validate.py package-draft
```

`package-draft` enthält den Cask und drei winget-YAMLs (version, installer,
defaultLocale). Es werden keine Attrappen mit erfundenen Checksummen oder einer
unbestätigten produktiven Paket-ID eingecheckt. Für getrennte Artefaktverzeichnisse
`--mode homebrew` bzw. `--mode winget` benutzen. `--base-url` erlaubt HTTPS und für
CI ausschließlich HTTP auf `127.0.0.1`; URLs mit Zugangsdaten werden abgewiesen.

## Homebrew-Verhalten und Aufgaben

Der Cask installiert `OpenSpider.app` und verknüpft dessen
`Contents/MacOS/openspider` in Homebrews `bin`. Bundle-ID:
`org.openspider.desktop`; Scheduler-Label: `de.openspider.task.`.
`auto_updates true` lässt den In-App-Updater führend. Ein bewusstes Upgrade über
Brew benötigt `brew upgrade --cask --greedy <tap>/openspider`.

Homebrew führt `uninstall launchctl:` **auch während eines Upgrades** aus.
Deshalb betten wir das Ruby-Hilfsskript aus `packaging/homebrew/lifecycle.rb`
mit `generated_script` in den Cask ein. Die strukturierten Homebrew-7-Schritte
`uninstall_preflight_steps` sichern Definition und Aktivierungszustand der Aufgaben
nach `{{temp}}/launchagents.json` (0600 im privaten 0700-Operationsverzeichnis).
`postflight_steps` stellt Definitionen wieder her und lädt nur zuvor aktive Aufgaben
in ihrer gespeicherten GUI-/Benutzerdomain. Vor dem Löschen meldet das Skript
registrierte Aufgaben explizit in beiden Domains ab; Homebrews Wildcard-Suche
allein sieht nur die aktuelle Domain. Die Schritte deklarieren ausschließlich
`~/Library/LaunchAgents` als zusätzlichen Schreibbereich und benötigen kein Netz.
Unabhängig neu angelegte Registrierungen werden nicht überschrieben. Nach einem
Bootstrap-Fehler bleibt die Sicherung zur Wiederherstellung im von Homebrew
gemeldeten Operationsverzeichnis erhalten.

Homebrew teilt `{{temp}}` zwischen den Phasen eines Upgrades bzw. Reinstalls und
entfernt das Verzeichnis nach erfolgreichem Abschluss. Separate Uninstall-/Install-
Befehle teilen es nicht: eine spätere Neuinstallation reaktiviert keine zuvor
deinstallierten Aufgaben. Es entsteht keine dauerhafte Sicherung in Benutzerdaten.
Die CI aktualisiert Homebrew vor der Probe auf die aktuelle Version (7+). Legacy-
Flight-Blocks werden nicht verwendet.
[Homebrew 7: strukturierte Schritte und privates Operationsverzeichnis](https://github.com/Homebrew/brew/blob/7.0.11/docs/Cask-Cookbook.md).

`brew uninstall --cask <tap>/openspider` entfernt die geladenen LaunchAgents,
zugehörige `~/Library/LaunchAgents/de.openspider.task.*.plist`, App und CLI-Link.
Benutzerdaten bleiben erhalten.
Nur der ausdrückliche Aufruf `brew uninstall --cask --zap <tap>/openspider` entfernt
auch die im `zap`-Abschnitt aufgeführten App-Daten, Cache- und Logordner. Explizite
Exportordner unter Dokumente und benutzerdefinierte Speicherorte werden nicht
pauschal gelöscht. Das Löschen der App im Finder löst keinen Cask-Hook aus (K3-09).

Quellen: [Homebrew Cask-Dokumentation](https://docs.brew.sh/Cask-Cookbook#uninstall-launchctl),
[Upgrade-Dispatch, untersuchter Stand](https://github.com/Homebrew/brew/blob/fcdfa0476e180b23cbf4b3a75e7bea76d93ad70d/Library/Homebrew/cask/artifact/abstract_uninstall.rb).

## winget-Verhalten

Schema 1.9.0; MSI mit Scope `machine`, `/qn /norestart`, `ALLUSERS=1 ADDTOPATH=1`,
NSIS (`nullsoft`) mit Scope `user`, `/S`. Beide haben stille Fortschrittsvarianten
und die passenden `InstallLocation`-Schalter. Ohne `REMOVEUSERDATA=1` bleiben
Benutzerdaten erhalten. Die R02-Installer führen Aufgaben-, PATH- und
Startmenü-Bereinigung aus. Die lokale Schema-Prüfung nutzt unveränderte, mit MIT-
Lizenz eingecheckte Microsoft-Schemata; native Abnahme bleibt `winget validate`.
M3-REL-Punkt: aktuell von winget-pkgs empfohlene Schema-Version abgleichen,
bei Bedarf alle drei Schemata byteidentisch vendoren und Generator-/Schema- sowie
native Windows-Proben erneut ausführen. Schema 1.9.0 bleibt bis dahin bestehen.

Die Installationsprobe läuft auf **windows-latest**, ohne Windows Sandbox. Falls
App Installer fehlt, installiert das offizielle MIT-Modul `Microsoft.WinGet.Client`
(gepinnt auf 1.12.440) mit `Repair-WinGetPackageManager -Version 1.12.440 -AllUsers`
den Client. Die Probe aktiviert `LocalManifestFiles`, validiert die drei lokalen
Manifeste und installiert nacheinander mit `--scope machine` und `--scope user`.
Der Loopback-Server liefert die heruntergeladenen Draft-Bytes; die Hashprüfung
bleibt aktiv. Deinstallation erfolgt anhand der tatsächlichen ARP-Registrierung
`--name OpenSpider --exact`, da die CI-ID in keinem öffentlichen Katalog steht.

[Microsoft: lokale Manifeste und Scope](https://learn.microsoft.com/en-us/windows/package-manager/winget/install),
[Microsoft: Client-Reparatur](https://github.com/microsoft/winget-cli/blob/c608376673762a10971e6bf2cbeedb6b6a7cae88/src/PowerShell/Help/Microsoft.WinGet.Client/Repair-WinGetPackageManager.md).

## CI starten (ausschließlich Orchestrator-Testzweig)

Paketbranch `m3/p01` nicht pushen. Der Orchestrator übernimmt die Commits in seinen
Testzweig. Die Workflow-Registrierung folgt dem Verfahren in
`docs/plan/M3-acceptance/smoke/README.md`: bei fehlender Dispatch-Registrierung
nur im Testzweig einen Push-Trigger für **package-manager-smoke.yml** ergänzen und
die drei Inputs dort fest vorbelegen. `publish-packages.yml` dabei nicht ändern.

Benötigt werden zwei vollständige R04-Trockenläufe mit unterschiedlichen Versionen:
beispielsweise ein vorheriger Build `1.0.0-beta.0` und der aktuelle
`1.0.0-beta.1`. Der Checkout des aktuellen Testzweigs muss dieselbe Version wie
der aktuelle Build tragen. Versionierung erfolgt durch den Orchestrator nach
`docs/plan/M3-reports/r04.md`; hier keine Manifest-/Lockfile-Änderungen vornehmen.
Auf dem gegenwärtigen Paketstand ist die Versionsquelle noch **0.1.0**; die
Beispielversionen sind keine Behauptung vorhandener Artefakte.

```bash
export GH_REPO=OWNER/REPOSITORY
export TEST_REF=ORCHESTRATOR_TEST_BRANCH
# Aktuelle Version aus dem Checkout dieses Testzweigs:
export RELEASE_TAG="v$(node --input-type=module -e "import {readVersion} from './scripts/release/check-version.mjs'; console.log(readVersion(process.cwd()))")"
gh workflow run release.yml --repo "$GH_REPO" --ref "$TEST_REF" \
  -f release_tag="$RELEASE_TAG" -f dry_run=true
gh run list --repo "$GH_REPO" --branch "$TEST_REF" --workflow release.yml --limit 5
export RELEASE_RUN_ID=123456789
# Platzhalter durch die tatsächliche ID ersetzen.
gh run watch "$RELEASE_RUN_ID" --repo "$GH_REPO" --exit-status
# Älteren erfolgreichen R04-Lauf samt genauer CLI-Version übernehmen:
export PREVIOUS_RUN_ID=123456788
export PREVIOUS_VERSION=1.0.0-beta.0
gh api "repos/$GH_REPO/actions/runs/$RELEASE_RUN_ID/artifacts" --jq '.artifacts[].name'
gh api "repos/$GH_REPO/actions/runs/$PREVIOUS_RUN_ID/artifacts" --jq '.artifacts[].name'
gh workflow run package-manager-smoke.yml --repo "$GH_REPO" --ref "$TEST_REF" \
  -f run_id="$RELEASE_RUN_ID" -f previous_run_id="$PREVIOUS_RUN_ID" \
  -f previous_version="$PREVIOUS_VERSION"
gh run list --repo "$GH_REPO" --branch "$TEST_REF" --workflow package-manager-smoke.yml --limit 5
export PACKAGE_SMOKE_RUN_ID=123456790
gh run watch "$PACKAGE_SMOKE_RUN_ID" --repo "$GH_REPO" --exit-status
gh run download "$PACKAGE_SMOKE_RUN_ID" --repo "$GH_REPO" \
  --pattern 'package-manager-*' --dir package-manager-evidence
```

Keine Tags, Releases oder Paket-Einreichungen zum Testen erzeugen. Die Suite lädt
`openspider-macos-aarch64`, `openspider-macos-x86_64` (beide Läufe) und
`openspider-windows-x86_64` (aktueller Lauf). Fehlende/abgelaufene Assets sind
Fehler. R01 benötigt weiterhin seine Apple-Zugänge; ein übersprungener DMG-Build
ist kein Installationsnachweis. Windows darf bis zur Signierung am 02.11.
unsigniert sein. Diese Probe belegt keine Authenticode-/SmartScreen-Abnahme.

Erwartet: drei grüne Jobs, auf beiden Macs echte Brew-Installation,
Versionswechsel per Upgrade, ausführbarer CLI-Link, nach Upgrade weiter geladener
Test-LaunchAgent, danach vollständige Deinstallation ohne Registrierung/Link/App.
Windows: `winget validate` grün; MSI und NSIS nacheinander still installiert und
entfernt; CLI, PATH, Startmenü, Test-Aufgabe und Erhalt des Daten-Sentinels geprüft.
Jeder Installerfehler ist rot. Cleanup läuft auch nach Fehlern; primäre Fehler
werden bei zusätzlichem Cleanup-Fehler beibehalten. Job-Summaries und die drei
`package-manager-*`-Artefakte liefern Evidenz. Nach realen Läufen deren IDs und
Ergebnisse in `docs/plan/M3-reports/p01.md` ergänzen.

## Veröffentlichung erst mit M3-REL

`publish-packages.yml` hat ausschließlich `release: types: [published]`; zusätzlich
werden Drafts/Prereleases und Tags außerhalb von `^v[0-9]+\.[0-9]+\.[0-9]+$`
mit Notice und Exit 0 übersprungen. Das gilt auch für normale veröffentlichte
Beta-Releases (K3-04), deren Prerelease-Flag nicht gesetzt ist. Ohne ausdrückliches
Opt-in `RELEASE_PACKAGES_ENABLED=true`, bestätigte `OPENSPIDER_PACKAGE_ID` und
bestehende Repositories `HOMEBREW_TAP_REPOSITORY` / `WINGET_FORK_REPOSITORY`
endet der Gate grün mit Hinweis. Dieser Job hat kein Environment und greift auf
keine Secrets zu.

Die Secrets `HOMEBREW_TAP_TOKEN` und `WINGET_PR_TOKEN` später ausschließlich als
**Environment-Secrets im Environment `release`** einrichten, nicht als Repository-
Secrets. Für `release` Required Reviewers sowie Deployment-Branch-/Tag-Schutz
konfigurieren und den Release-Zweig durch Branch-Schutz absichern. Erst nach
dieser Freigabe prüft der Publish-Job die Secret-Verfügbarkeit; fehlen Secrets,
endet er grün mit Hinweis und überspringt alle weiteren Schritte. `publish.sh`
verlangt beide Tokens zusätzlich mit `${HOMEBREW_TAP_TOKEN:?}` beziehungsweise
`${WINGET_PR_TOKEN:?}` und bricht bei fehlenden Werten vor externen Änderungen ab.
Für den PR gegen `microsoft/winget-pkgs` ist vermutlich ein klassisches PAT mit
`public_repo` nötig; Token-Typ und minimalen Berechtigungsumfang vor M3-REL
gegen GitHubs aktuelle Fork-/PR-Regeln prüfen. Jetzt nichts davon einrichten.

Nach M3-REL lädt er die veröffentlichten stabilen Release-Assets, erzeugt den
Cask und die YAMLs, committet `Casks/openspider.rb` im **bestehenden** Tap und öffnet
über einen Branch im **bestehenden** winget-Fork einen PR gegen
`microsoft/winget-pkgs`. Keine Repository-Erzeugung, kein direkter Push ins
Microsoft-Repository, kein automatischer Merge. Wiederholte Einreichungen eines
bereits existierenden winget-Branches schlagen geschlossen fehl und verlangen
Prüfung. Für die Einreichung genügt das bereits vorhandene `gh` (MIT), ohne
zusätzliches wingetcreate. Tokens werden ausschließlich als Umgebungsvariablen
übergeben, nie in Remote-URLs oder CLI-Argumenten. Keine Debug-/Trace-Ausgabe.

Lizenznachweise: [gh MIT](https://github.com/cli/cli/blob/9490d204a2ba95434153dedce99898ce9f281426/LICENSE),
[winget und Client-Modul MIT](https://github.com/microsoft/winget-cli/blob/c608376673762a10971e6bf2cbeedb6b6a7cae88/LICENSE),
[wingetcreate MIT, geprüft, nicht verwendet](https://github.com/microsoft/winget-create/blob/fb3522c2b5a5556ffe0ac70339b7e45b7c616878/LICENSE).

## Lokale Prüfungen

```bash
nice -n 10 node --test packaging/homebrew/generate.test.mjs
nice -n 10 python3 -m unittest discover -s packaging/winget -p 'test_*.py' -v
ruby packaging/homebrew/lifecycle.test.rb # falls Ruby vorhanden; auch in macOS-CI
bash -n packaging/homebrew/smoke.sh packaging/winget/publish.sh
# actionlint 1.7.7 kennt macos-15-intel noch nicht:
printf 'self-hosted-runner:\n  labels: [macos-15-intel]\n' > /tmp/p01-actionlint.yaml
nice -n 10 actionlint -config-file /tmp/p01-actionlint.yaml \
  .github/workflows/publish-packages.yml .github/workflows/package-manager-smoke.yml
bash scripts/check-neutral.sh
```

Node-Tests tragen `// spec: OUT-166 [M3]` bzw. `// spec: OUT-266 [M3]` und erzeugen
Metadaten gegen die Byte-Fixture `packaging/homebrew/fixtures.json`. Die Python-
Schema-Prüfung wird dabei mit ausgeführt (vorhandene MIT-Werkzeuge PyYAML und
jsonschema). Die Ruby-Skript-Fixture ruft keine OS-Werkzeuge auf. Alle Action-SHAs
wurden per `git ls-remote` aufgelöst; keine neuen Produktionsabhängigkeiten.
