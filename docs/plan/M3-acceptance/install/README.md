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
und landet im Protokoll (`mock-log.jsonl`); es gibt keine Weiterleitung ins Internet. Die Test-CA erzeugt
der Treiber in jedem Matrix-Job mit `openssl` in einem temporären Ordner außerhalb des Ergebnisordners
(ECDSA P-256, 7 Tage, CA-Schlüssel nach dem Signieren gelöscht); sie wird nicht hochgeladen und gilt nur im
Testprofil dieses Jobs. Der Updater-Signierschlüssel wird nirgends gebraucht: Das N+1-Paket ist im
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
| Windows x64 NSIS | `windows-latest` | `…_x64-setup.exe /S` (pro Benutzer) | Versuch echte App (**unbelegt, wahrscheinlich nicht erreichbar**), sonst Nachbildung |
| Windows x64 MSI | `windows-latest` | `msiexec /i … /qn` (pro Rechner) | wie NSIS |
| Windows 11 ARM NSIS | `windows-11-arm` | x64-Setup unter Emulation | wie NSIS |

**Windows-Steuerweg (Q-13): unbelegt, wahrscheinlich nicht erreichbar.** Die Matrix versucht, die installierte
Release-App über die DevTools-Schnittstelle von WebView2 zu steuern
(`WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=…`) und dieselben IPC-Befehle aufzurufen wie
der Update-Dialog. Das Paket M3-nightly-b („Ursache 2“, `docs/plan/M3-reports/nightly-b.md`) hat in acht
Diagnoseläufen belegt, dass wry diese Variable mit eigenen Zusatzargumenten überschreibt; der Debug-Port kam
dort nur über `TAURI_CONFIG` in einem **Testbuild** an. Für die signierte Release-App gibt es diesen Weg nicht,
einen anderen tragfähigen Steuerweg ohne Produktänderung kennt das Paket nicht. Q-13 erhält dann den Status
`unbelegt` (kein Fehlschlag des Jobs), **W-02 aus dem Nutzerprotokoll wird Pflicht**, und die Zeile läuft mit
der Nachbildung weiter: signierten N+1-Installer über den Mock laden, Signatur mit dem in N eingebetteten
Schlüssel prüfen, still über N installieren (`/S` bzw. `msiexec /i … /qn`; der Tauri-Updater ruft den
Installer mit anderen Argumenten auf). Erst wenn ein Lauf Q-13 bis Q-17 auf dem echten Weg grün zeigt, gelten
sie unter Windows als automatisch belegt.

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
| Q-13 | Release-App steuerbar · Windows: Versuch über DevTools, **unbelegt, wahrscheinlich nicht erreichbar** · macOS: `Hand` (M-02) | OUT-251 |
| Q-14 | Echter Weg (nur wenn Q-13 grün): App lehnt Manifest mit HTTP-Artefakt ab · Nachbildung: manipuliertes Paket über den Mock geladen und abgelehnt | OUT-252 |
| Q-15 | Echter Weg: App lädt das manipulierte Paket, verwirft es, Installation unverändert · Nachbildung: `unbelegt` (nur Rust-Test gegen Mock) | OUT-252 |
| Q-16 | Echter Weg: N+1 prüfen, herunterladen, „Jetzt installieren und neu starten“ · Nachbildung: Paket laden, Signatur prüfen, Bundle austauschen (macOS) bzw. Installer still über N (Windows) | OUT-251, OUT-252 |
| Q-17 | Echter Weg: Installer schreibt N+1, App startet neu · Nachbildung: N+1 startet am selben Ort, Drosselung bleibt | OUT-251 |
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
| Windows: Update durch den Tauri-Updater der Release-App (Q-13 bis Q-17 echter Weg) | Steuerweg unbelegt, wahrscheinlich nicht erreichbar (wry überschreibt die WebView2-Argumente, M3-nightly-b) | Nachbildung; **W-02 Pflicht** |
| Windows: Klickweg über Menü und Dialog-Schaltflächen | selbst mit Steuerweg nur IPC statt Klick | **W-02** |
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
Fehlt `latest.json` in N+1, läuft keine Zeile: Der Lauf trägt eine `::warning`-Annotation, und
`install-update-protocol` enthält „Übersprungen: keine signierten Updater-Artefakte“ – das ist kein Nachweis.

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
