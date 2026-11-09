# Installations- und Update-Matrix (M3-Q02)

Workflow `.github/workflows/install-test.yml`, Skripte `tests/install/`. Die Matrix installiert Version N aus
einem Lauf von „Release orchestration“ und aktualisiert sie über den Updater auf N+1 aus einem zweiten Lauf.
N+1 kommt ausschließlich von einem lokalen Mock-Update-Server im Runner; kein Request erreicht das
öffentliche Release.

## 1 Wie der Mock die App erreicht

Die App fragt fest `https://github.com/wojcikdominik/openspider/releases/latest/download/latest(-beta).json`
ab; einen Test-Endpunkt gibt es nicht. Das Testprofil (`settings.toml` am Standardort des Runners) setzt
deshalb nur zwei vorhandene App-Einstellungen:

| Einstellung | Wert | Spec |
|---|---|---|
| `network.proxy` | `manual`, `http`, `127.0.0.1:<Port des Mocks>`, Ausnahmen `127.0.0.1`, `localhost` | OUT-269 |
| `network.trusted_certificates` | `ca.pem` der Test-CA dieses Laufs | OUT-270 |

Der Mock (`tests/install/mockupdate.py`) ist ein Proxy, der nur `CONNECT` zu den GitHub-Hosts annimmt und TLS
selbst mit einem Zertifikat der Test-CA beendet. Jedes andere Ziel und jede Klartext-Anfrage bekommt 403
und landet im Protokoll (`mock-log.jsonl`); es gibt keine Weiterleitung ins Internet. Die Test-CA entsteht im
Job `prepare` (ECDSA P-256, 7 Tage, CA-Schlüssel nach dem Signieren gelöscht) und gilt nur in den
Testprofilen dieses Laufs. Der Updater-Signierschlüssel wird nirgends gebraucht: Das N+1-Paket ist im
Release-Lauf signiert worden, die Matrix prüft nur.

Szenarien:

| Szenario | Manifest | Artefakte |
|---|---|---|
| `valid` | N+1 aus `release-assets` | bytegleich |
| `tampered` | N+1 | genau ein Byte (Dateimitte) gekippt |
| `http-artifact` | N+1, Artefakt-URLs `http://` | – |
| `http-redirect` | Endpunkt antwortet 302 auf `http://` | – |
| `downgrade` | Version N | – |

## 2 Matrix

| Zeile | Runner | Installation N | Update-Weg |
|---|---|---|---|
| macOS arm64 (DMG) | `macos-15` | Universal-DMG → `/Applications` | Nachbildung (Abschnitt 4) |
| macOS x64 (DMG) | `macos-15-intel` | Universal-DMG → `/Applications` | Nachbildung |
| Windows x64 NSIS | `windows-latest` | `…_x64-setup.exe /S` (pro Benutzer) | echte App, Tauri-Updater |
| Windows x64 MSI | `windows-latest` | `msiexec /i … /qn` (pro Rechner) | echte App, Tauri-Updater |
| Windows 11 ARM NSIS | `windows-11-arm` | x64-Setup unter Emulation | echte App, Tauri-Updater |

Unter Windows steuert die Matrix die installierte Release-App über die DevTools-Schnittstelle von WebView2
(`WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=…`, derselbe Weg wie `msedgedriver`) und
ruft dieselben IPC-Befehle auf wie der Update-Dialog (`update.check`, `update.download`,
`update.install {when: "now"}`). Download, Signaturprüfung, Installer-Aufruf und Neustart sind der echte
Tauri-Updater der Release-App.

## 3 Prüfungen je Zeile

| ID | Prüfung | Spec |
|---|---|---|
| Q-01 | N still installieren, CLI meldet N | OUT-262, OUT-265, OUT-266 |
| Q-02 | OS-Signatur von N (codesign/spctl/stapler bzw. Authenticode `Valid`) | OUT-265, OUT-266 |
| Q-03 | N+1-Artefakt mit dem **in N eingebetteten** Schlüssel gültig (Version im signierten Kommentar), 1 Byte geändert ungültig; Schlüssel gleich der Variable `OPENSPIDER_UPDATER_PUBKEY` | OUT-252 |
| Q-04 | Mit N einen Crawl in der Crawl-Verwaltung anlegen und exportieren | OUT-255 |
| Q-05 | CLI von N mit `OPENSPIDER_DISABLE_UPDATE_CHECK=1`: kein Request | OUT-250, OUT-253 |
| Q-06 | CLI von N: Exit 10, genau ein Manifest-Request mit nur `version`, `os`, `arch` und User-Agent `OpenSpider-Updater`, kein Download | OUT-250 |
| Q-07 | CLI: Artefakt-URL über HTTP abgelehnt | OUT-252 |
| Q-08 | CLI: Weiterleitung auf HTTP abgelehnt | OUT-252 |
| Q-09 | CLI: Manifest mit eigener Version → Exit 0 | OUT-252 |
| Q-10 | App-Start mit abgeschalteter Prüfung: 20 s kein Request | OUT-250, OUT-253 |
| Q-11 | App-Start: Prüfung über Proxy und Test-CA, nur Version/OS/Arch, `updates.last_check` gespeichert, kein Download ohne Zustimmung | OUT-250, OUT-269, OUT-270 |
| Q-12 | Zweiter App-Start innerhalb von 24 h: keine Prüfung | OUT-250 |
| Q-13 | Windows: Release-App über DevTools steuerbar · macOS: `Hand` | OUT-251 |
| Q-14 | Windows: App lehnt Manifest mit HTTP-Artefakt ab · macOS: manipuliertes Paket über den Mock geladen und abgelehnt (Nachbildung) | OUT-252 |
| Q-15 | Windows: App lädt das manipulierte Paket, verwirft es (Zustand `failed`), Installation unverändert · macOS: `anderswo` | OUT-252 |
| Q-16 | Windows: N+1 prüfen, herunterladen, „Jetzt installieren und neu starten“, alte App beendet sich · macOS: Archiv laden, Signatur prüfen, Bundle austauschen (Nachbildung) | OUT-251, OUT-252 |
| Q-17 | Windows: Installer schreibt N+1, App startet neu · macOS: N+1 startet am selben Ort, Drosselung bleibt | OUT-251 |
| Q-18 | Version N+1 am **ursprünglichen** Installationsort, CLI mit aktualisiert | OUT-251 |
| Q-19 | OS-Signatur nach dem Update gültig | OUT-252, OUT-265, OUT-266 |
| Q-20 | Crawl von N mit N+1 geöffnet, Export bytegleich (normalisiert wie der Installations-Smoke) | OUT-255 |
| Q-21 | Einstellungen (Proxy, Zertifikate, Update-Schalter) erhalten | OUT-255 |
| Q-22 | N+1 lehnt Downgrade auf N ab (Exit 0, kein Download) | OUT-252, OUT-254 |
| Q-23 | Im ganzen Lauf kein Klartext-Request und kein Ziel außerhalb des Mocks | OUT-252 |
| Q-24 | Nur Windows: Deinstallation nach dem Update, Exit 0, Programmdateien entfernt; Benutzerdaten bleiben (NSIS) bzw. werden mit `REMOVEUSERDATA=1` entfernt (MSI) | OUT-266 |
| H-01 … H-05 | Handprüfungen, siehe Abschnitt 4 | – |

Ergebnisse: Artefakt `install-update-<zeile>` (`report.json`, `report.md`, `mock-log.jsonl`,
`commands.log`, App-Logs, Einstellungen) und `install-update-protocol` (`protokoll.md`, Matrix aller Zeilen).
`protokoll.md` wird nach dem Lauf unter `docs/plan/M3-acceptance/install/<datum>-<N>-<N+1>.md` abgelegt.

## 4 Was nicht automatisch geht

| Punkt | Warum | Stattdessen |
|---|---|---|
| macOS: Update-Dialog bedienen und Bundle-Austausch **durch den Tauri-Updater** | Release-Builds haben keinen WebDriver (`e2e-webdriver` nur in Testbuilds), WKWebView keinen DevTools-Port; ein UI-Klick ohne Webdriver ist auf dem Runner nicht verlässlich möglich (Bedienungshilfen-Freigabe) | Nachbildung Q-14/Q-16/Q-17 (gleicher Download-Weg, gleiche Signaturprüfung, gleicher Austausch am selben Ort); echter Klickweg **M-02** |
| Windows: Klickweg über Menü und Dialog-Schaltflächen | IPC statt Klick (gleiche Befehle) | **W-02** |
| Update bei laufendem Crawl („Nach Crawl-Ende installieren“, kein Neustart) | braucht einen Crawl in der App; Zustand deckt `update_install_flow`/`update_crawl` | **M-02, W-02** |
| Neuigkeiten-Dialog nach dem Update (OUT-257) | UI | **M-03, W-03** |
| Zuletzt geöffneter Crawl wieder offen, Aufgaben zeigen auf N+1 | UI bzw. Planer | **M-04, M-05, W-04** |
| SmartScreen-/UAC-Herausgeber beim Installer des Updaters | interaktiv | **W-05, W-16** |
| Crawl-Rate ±5 % während eines Downloads | nur Mac-mini-Werte zählen | Abschnitt 6 |

IDs aus `docs/release/abnahme-1.0-nutzer.md` (Prüfprotokoll des Nutzers).

## 5 Starten (Orchestrator)

Voraussetzung: zwei Läufe von „Release orchestration“ mit dem Artefakt `release-assets`; N+1 muss
signierte Updater-Artefakte haben (`release-reports/updater-status.txt` = „signierte Updater-Artefakte“,
`latest.json` in `release-assets`). N darf ein Tag-Lauf oder ein Trockenlauf sein; für Q-02 braucht N
signierte Installer. Beide Versionen müssen im selben Kanal liegen (Beta → Beta/stabil, stabil → stabil).

```bash
# 1. Lauf-IDs bestimmen (N = zuletzt veröffentlichte Beta, N+1 = Kandidat)
gh run list --repo wojcikdominik/openspider --workflow release.yml --limit 10 \
  --json databaseId,headBranch,displayTitle,conclusion,createdAt
# N+1 als Trockenlauf, falls noch keiner existiert (kein Entwurf, keine Veröffentlichung):
gh workflow run release.yml --repo wojcikdominik/openspider --ref rel/rc1 \
  -f release_tag=v1.0.0-rc.1 -f dry_run=true
gh run download <N1_RUN_ID> --repo wojcikdominik/openspider -n release-reports -D /tmp/n1-reports \
  && cat /tmp/n1-reports/updater-status.txt

# 2. Matrix starten (nach dem Merge von M3-Q02 auf main)
gh workflow run install-test.yml --repo wojcikdominik/openspider --ref main \
  -f current_run_id=<N_RUN_ID> -f next_run_id=<N1_RUN_ID>
gh run list --repo wojcikdominik/openspider --workflow install-test.yml --limit 3
gh run watch <RUN_ID> --repo wojcikdominik/openspider --exit-status

# 3. Protokoll holen und ablegen
gh run download <RUN_ID> --repo wojcikdominik/openspider -n install-update-protocol -D /tmp/q02
gh run download <RUN_ID> --repo wojcikdominik/openspider -p 'install-update-*' -D /tmp/q02/evidence
cp /tmp/q02/protokoll.md docs/plan/M3-acceptance/install/<datum>-<N>-<N+1>.md
```

Vorschlag rc.1: N = Tag-Lauf beta.3 (`42584548577`) bzw. beta.4, N+1 = Trockenlauf `v1.0.0-rc.1` auf `rel/rc1`.
Für 1.0.0 vor der Veröffentlichung: N = rc.1-Tag-Lauf, N+1 = Trockenlauf `v1.0.0`.

Lokal (Server, ohne Installation): `python3 -m unittest discover -s tests/install -v`; mit einer CLI, die mit
`OPENSPIDER_UPDATER_PUBKEY=$(cat tests/fixtures/update/test.key.pub)` gebaut ist, zusätzlich
`Q02_CLI=<pfad>/openspider python3 -m unittest -v test_cli_integration` (echte Update-Prüfung gegen den Mock).

## 6 Crawl-Rate während eines Update-Downloads (Mac mini)

Rezept aus dem Plan (6.2), Werkzeug `tests/install/crawl_rate.py`. Nur auf dem Mac mini (ruhige Maschine
nach C11, nichts anderes laufen lassen):

```bash
cargo build --release --locked -p os-cli -p os-testsite
python3 tests/install/crawl_rate.py --cli target/release/openspider --testsite target/release/os-testsite \
  --output docs/plan/M3-acceptance/install/crawl-rate-$(date +%Y%m%d) --runs 3 --max-urls 5000 \
  --download-mb 70 --rate-mb 10
```

Je Wiederholung abwechselnd Lauf A (ohne Download) und B (während eines gedrosselten HTTPS-Downloads über den
Mock-Proxy mit Test-CA und User-Agent des Updaters). Ergebnis `result.json`: `bestanden` bei
|Median B − Median A| / Median A ≤ 5 %; `nicht auswertbar`, wenn die A-Läufe um mehr als 5 % streuen oder
ein Download vor dem Crawl endet. Werte nach `crawl-rate-update.md` übertragen.
