# Release-Prozess (M3-R04)

Stand 03.11.2026: offene Beta (Nutzerentscheid 02.11.2026, Feedback über openspider.de/beta und GitHub-Issues). Bundle-ID `org.openspider.desktop`, GitHub Releases als Update-Host, kein Chromium im Bundle. R03, D01 und offene M2-Parität sind ausstehende Abnahmen. Diese Orchestrierung ersetzt sie nicht.

## Version und Start

Die vorbereitete `[workspace.package] version` ist maßgeblich. Der Workflow setzt keine Version und ändert keine Abhängigkeiten. Vor dem Lauf im separaten Release-Vorbereitungsbranch:

```bash
node scripts/release/set-version.mjs 1.0.0-beta.0
cargo metadata --locked --format-version 1 > /dev/null
node scripts/release/check-version.mjs
```

`set-version.mjs` synchronisiert Rust, npm, Tauri und die lokalen Workspace-Pakete in Cargo.lock. Registry-Pakete bleiben unverändert; kein `cargo update`. Versionsänderungen separat prüfen und committen. `beta.0` ist für den technischen Trockenlauf reserviert; die Windows-MSI-Version ist `1.0.0` und liegt vor beta.1 (`1.0.1`), RC.1 (`1.0.500`) und stable (`1.0.999`). Der Produktversionsstring bleibt `1.0.0-beta.0`.

Notes vor dem Bau (OUT-251, B3-REL): Einzige Quelle der Notes ist das eingecheckte Dokument `docs/release/notes/<Version>.md` (nur Text, Überschriften höchstens `###`, keine TODO/STUB-Markierungen). Es wird am Mittwochvormittag festgelegt und kann vorab als reiner Doku-Commit auf `main` landen (startet keine schwere CI, nur den leichten Lauf `release-scripts.yml`). `CHANGELOG.md` braucht im Release-Stand eine wortgleiche Sektion `## [<Version>] - <Datum>`; `node scripts/release/notes.mjs --changelog <Version> --date <JJJJ-MM-TT>` schreibt sie aus dem Dokument unter `[Unreleased]` und überschreibt nie einen abweichenden Eintrag. `node scripts/release/notes.mjs --check <Version>` prüft beides; derselbe Aufruf läuft im `prepare`-Job vor jedem Bau-Job und bricht mit Datei und Abhilfe ab. Was danach am Release-Text geändert wird, erreicht `latest.json`/`latest-beta.json` nicht mehr; deshalb den Release-Text nach dem Bau nicht von Hand ändern.

`release.yml` startet bei Tags `v*` oder manuell über `workflow_dispatch`. `release_tag` muss exakt `v<Workspace-Version>` sein. Bei `dry_run=true` ist kein echter Tag erforderlich. Ein Entwurf verlangt einen vorhandenen Tag auf genau demselben Commit. Tags nur nach Nutzer-Go anlegen/pushen (D12).

## Jobs und Artefakte

1. `prepare`: Versionsgleichstand, Tag, Notes-Dokument samt CHANGELOG-Eintrag (OUT-251) und Release-Vertragstests. Alle Bau-Jobs hängen von `prepare` ab.
2. `macos`/`windows`: lokale wiederverwendbare R01/R02-Workflows, Kanal aus der Version, Secrets geerbt. R02 verwendet Azure Artifact Signing mit OIDC im Environment `release`. Native macOS-Tests laufen vor dem Signierjob.
3. `linux-core`: Ubuntu 22.04 x86_64/glibc, bestehendes `scripts/ci/linux-core.sh release` und `golden`, höchstens vier Cargo-Jobs. Archiv extrahieren und CLI starten. Kein Linux-Desktop, keine deb/rpm, kein Linux-arm64-Versprechen.
4. `assemble`: Assets einsammeln, Kollisionsprüfung, Windows-CLI zippen, Notes, Manifeste, verbindliches D01-Kompatibilitätsarchiv vor den Prüfsummen. Entwurfshinweise (unsigniert, Installations-Smoke/STUB R03, TODO M2-Parität) stehen in der Actions-Zusammenfassung und in `release-reports/entwurfshinweise.md`, nie im Release-Text. Actions-Artefakte `release-assets` und `release-reports` sichern.
5. `draft`: nur bei Tag-Push oder `dry_run=false`; Environment `release`, `contents: write`. Bricht ab, wenn der Release-Text Entwurfshinweise enthält. `gh release create --verify-tag --draft --prerelease=false --latest=false`. Bestehende Releases werden nie überschrieben, auch keine bestehenden Entwürfe. Bei Wiederholung mit vorhandenem Entwurf zuerst manuell entscheiden.

Assets (Nutzerentscheidung 04.11.2026, OUT-261/OUT-266): Universal-DMG `OpenSpider_<Version>_universal.dmg`, Windows NSIS/MSI, Updater-Archive `OpenSpider_{aarch64,x86_64}.app.tar.gz` und die Installer jeweils plus `.sig`, CLI-Archive `openspider_<Version>_darwin-universal.tar.gz`, `openspider_<Version>_windows-x86_64.zip` und `openspider_<Version>_linux-x86_64.tar.gz`, `compat_<Version>.tar.gz`, Manifeste, `SHA256SUMS`. Dazu zwei versionslose, bytegleiche Kopien `OpenSpider-macOS.dmg` (Universal-DMG) und `OpenSpider-Windows-Setup.exe` (NSIS) für feste Links `releases/latest/download/…`. `scripts/release/assets.mjs` lässt DMGs und CLI-Archive je macOS-Architektur sowie `OpenSpider_universal.app.tar.gz(.sig)` aus (sie bleiben Actions-Artefakte für R03 und Paketmanager-Smoke), lehnt unbekannte macOS-Dateinamen ab und legt die Kopien an. Der `draft`-Job prüft vor `gh release create`, dass beide Kopien bytegleich vorhanden und in `SHA256SUMS` sind und keine der ausgelassenen Dateien vorliegt. CLI-Archive enthalten `openspider` bzw. `openspider.exe`, LICENSE, NOTICE und die vollständige `THIRD_PARTY_NOTICES.txt` aus dem L01-Artefakt `third-party-notices`. Assemble ergänzt diese Datei in den eigenständigen macOS-CLI-Archiven aus R01; signierte Updater-Archive bleiben bytegleich. macOS-CLI stammt aus der signierten App. Signierte Updater-Bytes werden beim Sammeln nicht verändert. Prüfprotokolle bleiben in den Actions-Artefakten.

## Signaturen und Update-Endpunkte

Updater: Secrets `TAURI_SIGNING_PRIVATE_KEY`, optional `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`; Variable `OPENSPIDER_UPDATER_PUBKEY`. Apple-Konfiguration siehe [macos-signing.md](macos-signing.md), Azure siehe [windows-signing.md](windows-signing.md). Werte nie protokollieren. Environment `release` und dessen Freigaben/Branch-Regeln vorab konfigurieren: `v*`-Tags und den Release-Branch (`release/1.0`, beim Trockenlauf auch `m3/r04-beta0`) als Deployment-Ziele zulassen. Apple- und Updater-Zugangsdaten für R01 müssen als Repository-Secrets verfügbar sein; R01 bindet derzeit kein Environment. R02 liest die Konfiguration im Environment `release`.

Ohne Apple-Konfiguration überspringt R01 seine Distributions-Builds grün. R02 baut ohne Azure-Konfiguration unsigniert und ohne Updater-Schlüssel ohne Updater-Signaturen. R04 meldet bei fehlenden signierten Zielpaketen ausdrücklich **unsigniert** und erzeugt keine unvollständigen Manifeste. Beide Manifeste entstehen ausschließlich bei vier vollständigen signierten Zielen: macOS arm64, macOS x64, Windows x64 NSIS und Windows x64 MSI. CLI-/Installer-Assets und Berichte bleiben verfügbar. Ungültige Signaturhüllen und mehrdeutige Installer sind Fehler. Der Generator prüft das Tauri/Minisign-Hüllenformat; kryptografische Prüfung und echte Installation bleiben Aufgabe des Updaters und der Installations-/Update-Abnahme.

Plattformschlüssel: `darwin-aarch64`, `darwin-x86_64`, `windows-x86_64-nsis`, `windows-x86_64-msi`. Universal-DMG/CLI sind die einzigen macOS-Downloads; Updates wählen das native Architekturpaket. Kein erfundener Windows-ARM-Schlüssel: x64 unter Emulation gehört zu R03.

~~K3-04 (vorläufige Nutzerentscheidung): bis 1.0 sind Betas **normale GitHub Releases**, keine Pre-Releases. Jeder vollständige Release trägt identische `latest.json` und `latest-beta.json`; stable aktualisiert ebenfalls beide Endpunkte. Betas lesen `https://github.com/wojcikdominik/openspider/releases/latest/download/latest-beta.json`. Stable-Clients lehnen Beta-Versionen ab. Beim späteren manuellen Veröffentlichen muss der freigegebene Release ausdrücklich als **Latest** gesetzt werden, sonst funktioniert `releases/latest/download/…` nicht zuverlässig. Der Entwurf setzt Latest ausdrücklich nicht. Ein Entwurf ist für den öffentlichen Updater noch nicht erreichbar.~~ **Abgelöst durch K3-16 (Nutzerentscheidung 21.11.2026), siehe Abschnitt „Update-Kanäle“.**

`SHA256SUMS` erfasst alle Dateien in `release-assets` außer sich selbst, einschließlich `.sig`, Manifesten und der versionslosen Kopien. `updater-status.txt` bleibt ausschließlich in `release-reports` (auch in der Summary), außerhalb von Release-Assets und SHA256SUMS. Die Notes und Prüfberichte sind Release-Body bzw. Actions-Artefakte und keine separaten Release-Assets.

Notes: `notes.mjs <Version> release-reports/update-notes.md --body release-reports/release-notes.md --repository <owner/name>`. `update-notes.md` enthält nur das Notes-Dokument der Version und speist `latest.json`/`latest-beta.json` (Update-Dialog, OUT-251). `release-notes.md` ist der Release-Text und besteht nur aus dem Download-Kasten (Universal-DMG und NSIS-Setup dieses Releases, Hinweis zu den übrigen Dateien de/en, Links Handbuch/User guide/Feedback) und „## Neu in dieser Version“ mit denselben Notes. Feedback-Link (OUT-261): Vorabversionen `https://openspider.de/beta/`, stabile Versionen `https://github.com/<owner/name>/discussions`. Der Release-Text wird unverändert veröffentlicht; Entwurfshinweise stehen nur in der Actions-Zusammenfassung.

## Update-Kanäle (K3-16, Nutzerentscheidung 21.11.2026)

Spec: OUT-254 (Kanäle, P1 ab 1.0.1), OUT-261 (Veröffentlichungsregel), UI-131 (Einstellung). Plan: `docs/plan/M3.md` K3-16.

**Regel:** „Latest“ ist auf GitHub immer die neueste stabile Version.

| | stabiles Release `vX.Y.Z` | Beta-Release `vX.Y.Z-beta.N` / `-rc.N` |
|---|---|---|
| Entwurf (`release.yml`) | `--draft --prerelease=false --latest=false` (unverändert) | ebenso |
| Veröffentlichen | als **Latest**: `gh release edit vX.Y.Z --draft=false --latest` | **nicht** Latest, **nicht** Pre-Release: `gh release edit vX.Y.Z-beta.N --draft=false --latest=false` |
| Manifeste im Release | `latest.json` und `latest-beta.json`, identisch (Clients bis 1.0.0) | nur `latest-beta.json`, **kein** `latest.json` |
| Beta-Manifest am Tag `update-beta` | ersetzen, wenn die Version nach SemVer neuer ist als der bisherige Inhalt | immer ersetzen |

Beim Veröffentlichen über die Weboberfläche ist „Set as the latest release“ möglicherweise vorausgewählt (nicht geprüft); deshalb über `gh release edit` mit ausdrücklichem `--latest` bzw. `--latest=false` veröffentlichen. Bis das Paket Release-Tooling 1.0.1 `manifest.mjs` umgestellt hat (Betas ohne `latest.json`) und der Tag `update-beta` existiert, wird **keine Beta veröffentlicht**.

**Beta-Manifest am festen Tag `update-beta`** (`https://github.com/wojcikdominik/openspider/releases/download/update-beta/latest-beta.json`):

- *Anlegen, einmalig durch den Eigentümer*, nach dem Veröffentlichen von 1.0.1 und erst, wenn `homebrew.yml` den Tag `update-beta` überspringt (sonst scheitert dessen Lauf am Versionsformat): Release mit Tag `update-beta` am aktuellen `main`, als **Pre-Release** markiert (wird damit nie Latest), Titel und Text sagen, dass es nur das Beta-Manifest trägt. Einziges Asset ist `latest-beta.json` aus dem Release 1.0.1 (bytegleich zu dessen `latest-beta.json`). Kein weiteres Asset, keine Notes außer diesem Hinweis.
- *Aktualisieren, nur nach Go des Eigentümers*: nach jeder Veröffentlichung nach der Tabelle oben `latest-beta.json` des gerade veröffentlichten Releases mit `--clobber` hochladen. Das geschieht über den Workflow „Kanal-Manifest aktualisieren“ aus dem Paket Release-Tooling 1.0.1 (Freigabe im Environment `release`); bis es ihn gibt, nur von Hand durch den Eigentümer. Ältere Inhalte werden nie hochgeladen (kein Zurücksetzen auf eine ältere Version); eine Rücknahme entscheidet der Eigentümer im Einzelfall.
- Das Release `update-beta` ist eine bewusste Ausnahme von „bestehende Releases werden nie überschrieben“ (D12): Nur diese eine Datei in diesem einen Release wird ersetzt. Versionierte Releases bleiben unverändert.
- *Prüfschritt nach jeder Veröffentlichung*: `releases/latest/download/latest.json` abrufen → `version` ist die neueste stabile Version; die Beta-Manifest-URL abrufen → `version` ist die neueste Version insgesamt; in beiden zeigen alle Paket-URLs auf `releases/download/v<diese Version>/…`. Außerdem `releases/latest/download/OpenSpider-macOS.dmg` → stabile Version.

**Rückweg beta → stabil:** Nur über die Einstellung in der App (UI-131). Wer wechselt, bleibt auf seiner Vorabversion, bis eine neuere stabile Version erscheint; eine Anleitung zur Neuinstallation einer älteren stabilen Version gibt es nicht.

**Offen:** Eine Richtlinie für Firmen-/MSI-Installationen, die den Kanal festlegt oder Beta verbietet (OUT-253), ist nicht entschieden.

## Noch fehlende Paket-Schnittstellen

- **Abhängigkeit L01 (Gate):** `licenses.yml` fehlt auf diesem Branch. Der lokale `workflow_call` ist als kommentierter Job `licenses` vorbereitet. Nach Integration diesen Job aktivieren und `licenses` in `needs` von `linux-core` und `assemble` aufnehmen. Schnittstelle: Artefakt `third-party-notices` mit nicht leerer `THIRD_PARTY_NOTICES.txt` in der Wurzel, aus demselben Actions-Lauf. Beide Jobs laden es bereits verpflichtend; fehlt es, scheitert der Lauf vor der CLI-Packung. Keine Platzhalter-Lizenztexte und keine Archive ohne Drittlizenzen. Der vollständige Trockenlauf setzt daher die L01-Integration voraus.

- **STUB/TODO(R03):** Die Actions-Zusammenfassung (nicht der Release-Text) zeigt den fehlenden installierten Plattform-Smoke. Nach Integration `smoke-installed.yml` mit Artefakten desselben Runs verdrahten; Schnittstelle gemeinsam mit R03 festlegen. Keine vorgetäuschte erfolgreiche Smoke-Abnahme.
- **D01 (integriert):** `scripts/release/archive-compat-fixture.mjs --version <Version> --out release-reports/compat --cli <release-cli>` wird verpflichtend mit der gebauten Linux-Release-CLI ausgeführt. Fehler blockieren den Lauf. Jede Version erzeugt `compat_<version>.tar.gz`, auch bei Trockenläufen; das Archiv wird vor SHA256SUMS gepackt und als Release-Asset sowie Actions-Bericht gesichert. Ab `1.0.0-beta.1` jede veröffentlichte Version prüfen und unverändert als `tests/fixtures/migrations/releases/<version>/` ins Repo übernehmen (entpackter Ordner, kein Archiv; beta.1 und beta.2 übernommen mit B3-REL). Diese Grenze betrifft nur die Übernahme ins Repo; der Workflow committet nichts zurück.
- **TODO(M2):** Offene Parität und Lizenz-Abnahme L01 vor externer Freigabe prüfen. Keine neue Abhängigkeit in R04.

Veröffentlichung, Update-Verteilung, Website und Paketmanager-Einreichungen folgen erst nach ausdrücklichem Nutzer-Go über M3-A01/M3-REL. Die [Release-Checkliste](checkliste-release.md) hält die manuellen Schritte fest.
