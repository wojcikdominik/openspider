# Bericht B3-BREW: Homebrew-Tap vorbereiten

Branch `m3/brew` (Basis main `28a0a9b3`), Worktree `c-brew`, Server `m3-brew`. Grundlage: Nutzerentscheidung 05.11.2026
(Tap `wojcikdominik/homebrew-openspider` ab beta.3, winget erst zur 1.0), `docs/release/homebrew-winget.md` (P01),
Spec `docs/spec/06-output-ops.md` (OUT-166, OUT-227, OUT-260, OUT-261), `docs/plan/M3.md` (K3-04, K3-08, K3-09).
Nichts gepusht, kein Workflow gestartet, kein Repository, Token, Secret oder Environment angelegt, kein Website-Deploy.

## Ergebnis in Kürze

| Teil | Datei | Stand |
|---|---|---|
| Generator | `scripts/release/homebrew.mjs` | `generate` (Version + SHA256SUMS → `Casks/*.rb`), `update-tap` (Übernahme in Tap-Checkout mit Schutz gegen Zurücksetzen/Überschreiben) |
| Cask-Text | `packaging/homebrew/generate.mjs` | neue gemeinsame Funktion `caskSource()` für P01 und Tap; livecheck, `conflicts_with`, zap sortiert und um WebKit-Pfade ergänzt |
| Tests | `scripts/release/homebrew.test.mjs` | 11/11 grün, Fixture: unveränderte `SHA256SUMS` von v1.0.0-beta.2 (`tests/release/fixtures/SHA256SUMS-1.0.0-beta.2`) |
| Workflow | `.github/workflows/homebrew.yml` | actionlint 1.7.12 mit shellcheck 0.11.0 ohne Befund |
| Doku | `docs/release/checkliste-release.md`, `docs/release/homebrew-winget.md`, `README.md` | Einrichtung und Ablauf; README „prepared“ |
| Website | Download und Handbuch, de/en | „vorbereitet“ wie bei Docker, kein Installationsbefehl; `check-site` grün |

## Ausgangslage

P01 hatte bereits einen Cask-Generator (`packaging/homebrew/generate.mjs`, liest Asset-Bytes), das LaunchAgent-Hilfsskript,
eine Installationsprobe (`package-manager-smoke.yml`) und `publish-packages.yml`, das **nur stabile** Releases und Homebrew
und winget **gemeinsam** veröffentlicht (beide Tokens Pflicht, Opt-in-Variablen). Für Betas ab beta.3 passt das nicht. Ich
habe den Cask-Text nicht dupliziert, sondern aus `generate()` in `caskSource()` gezogen; beide Wege erzeugen denselben Text.

## Entscheidungen

### Beta-Abbildung: zwei Casks, `openspider@beta` und `openspider`

- `openspider@beta` folgt der jeweils **neuesten veröffentlichten Version**, Beta oder stabil (ab beta.3; zur 1.0 zeigt er
  auf 1.0.0). `openspider` führt **nur stabile** Versionen und entsteht erst mit dem ersten stabilen Release.
- Beide installieren `OpenSpider.app` und schließen sich per `conflicts_with cask:` gegenseitig aus.
- Gründe:
  - `@beta` ist die Homebrew-Konvention für Vorabversionen; der Kanal ist beim Installieren sichtbar.
  - Kein Umbenennen zur 1.0: Beta-Nutzer bleiben auf `openspider@beta` und bekommen 1.0 wie die App selbst (K3-04: das
    Beta-Manifest zeigt mit 1.0 auf 1.0.0). Wer nach 1.0 nur stabile Versionen will, wechselt auf `openspider`.
  - `Casks/openspider.rb` bleibt dem stabilen Kanal vorbehalten, wie es P01 (`publish-packages.yml`) schon vorsieht. Zur
    1.0 schreiben beide Workflows byte-gleiche Dateien (Test prüft das), also kein zweiter Commit.
- Verworfen: ein einziger Cask `openspider`, der der neuesten Vorabversion folgt (stabiler Name mit Beta-Inhalt; nach 1.0
  müsste er für spätere Betas wieder getrennt werden); `openspider-beta` mit Bindestrich (nicht die Konvention, Homebrew
  wertet `-beta` im Token als Versionsbezeichnung).
- Ein stabiles Release hinter einer neueren Beta (z. B. 1.0.1 nach 1.1.0-beta.1) lässt `openspider@beta` stehen
  (`update-tap` meldet „übersprungen“); jede andere ältere Version ist ein Fehler.
- `livecheck` mit `strategy :github_releases` über die Download-URL; Regex stabil `^v?(\d+\.\d+\.\d+)$`, Beta zusätzlich
  `-(beta|rc).N`. `github_releases` überspringt als Pre-Release markierte Releases; das passt zu K3-04 (Betas sind normale
  Releases). Ein als Pre-Release markiertes Release lehnt der Workflow ab, weil `brew audit` solche Versionen zurückweist.

### Kein eigener Cask und keine Formel für die CLI

- Der App-Cask legt die CLI schon in Homebrews `bin` (`binary "#{appdir}/OpenSpider.app/Contents/MacOS/openspider"`, P01).
  Das ist dieselbe Datei, die die LaunchAgents aufrufen (OUT-166, stabiler Pfad).
- Eine zusätzliche Formel aus `openspider_<v>_darwin-universal.tar.gz` würde ebenfalls `bin/openspider` belegen. Konflikte
  zwischen Formel und Cask kann Homebrew nicht ausdrücken; parallele Installation führt zu Link-Konflikten.
- Linux: Das Archiv `openspider_<v>_linux-x86_64.tar.gz` existiert (OUT-273). Eine Linux-Formel mit vorgebauter Binärdatei
  ginge im eigenen Tap, der Serverbetrieb ist aber über das Archiv und das Docker-Image (OUT-274) abgedeckt. Als Folgepunkt
  notiert, nicht gebaut.

### Generator

- Eingabe nur Version + `SHA256SUMS` des Releases; die DMG wird nicht geladen. Dateiname aus `downloads(version).macos.file`
  (`assets.mjs`, OUT-261), URL in derselben Form wie die Update-Manifeste (`manifest.mjs`). Die versionslose Kopie
  `OpenSpider-macOS.dmg` hat dieselbe Prüfsumme, wird aber nie verwendet (Test).
- `SHA256SUMS` wird streng gelesen (Format von `checksums.mjs`, keine doppelten Namen); fehlt das Universal-DMG, bricht er ab.
- Ausgabe in Tap-Struktur (`Casks/openspider@beta.rb`, bei stabiler Version zusätzlich `Casks/openspider.rb`), nur in einen
  leeren Ordner.
- `update-tap`: prüft alle Dateien vor dem ersten Schreiben; nie auf ältere Version zurück, gleiche Version mit anderem
  Inhalt wird nicht still ersetzt (wie bei veröffentlichten Assets), unveränderte Dateien bleiben unberührt.

### Änderungen am gemeinsamen Cask-Text (wirken auch auf P01)

- `livecheck` und `conflicts_with` ergänzt (siehe oben).
- `zap trash` sortiert: Homebrews Cop `Cask/ArrayAlphabetization` vergleicht ohne Groß-/Kleinschreibung; die bisherige
  Reihenfolge (`Caches/org.openspider.desktop` vor `Caches/OpenSpider`) wäre bei `brew style` durchgefallen.
- zap um drei Pfade ergänzt, die macOS/WebKit für die Bundle-ID anlegt: `~/Library/HTTPStorages/org.openspider.desktop`,
  `~/Library/Saved Application State/org.openspider.desktop.savedState`, `~/Library/WebKit/org.openspider.desktop`.
  Die übrigen Pfade stammen aus OUT-227 (Einstellungen/Crawls, Logs, Cache) und der Bundle-ID (`tauri.conf.json`).
- Leere Zeilen im eingebetteten Ruby-Skript ohne Einrückung (sonst Leerzeichen am Zeilenende, RuboCop
  `Layout/TrailingWhitespace` gilt auch im Heredoc).
- Mindest-macOS `depends_on macos: :ventura` (macOS 13, wie `minimumSystemVersion` 13.0, OUT-260; Test). Das Symbol allein
  heißt bei Homebrew „mindestens“; `">= :ventura"` lehnt `Homebrew/OSDependsOn` ab (Lauf 42626366623).
- Mehrzeilige Hashes (`run … args:/writable_paths:`, `uninstall launchctl:/delete:`) im `table`-Stil: Homebrews
  `.rubocop.yml` setzt `Layout/HashAlignment` auf `EnforcedColonStyle: table`, alle Werte beginnen in einer Spalte (Test).

### Workflow `.github/workflows/homebrew.yml`

| Job | Läuft | Tut |
|---|---|---|
| `generate` (ubuntu) | `release: published` und `workflow_dispatch` (`version`) | Release muss existieren, kein Entwurf, nicht als Pre-Release markiert; Universal-DMG muss Asset sein; lädt nur `SHA256SUMS`; erzeugt Casks; Artefakt `homebrew-casks` (30 Tage) |
| `check` (macos-latest) | nach `generate` | `brew update` (strukturierte Schritte brauchen Homebrew 7), lokaler Prüf-Tap per `brew tap-new --no-git`, je Cask `brew style --cask` und `brew audit --cask --strict --online` (Pflicht), `brew audit --cask --new --except=github_repository` (nur Hinweis), echte Installation mit CLI-Link und `--version`, Deinstallation |
| `publish` (ubuntu) | nur `workflow_dispatch` mit `publish=true`, auf `main`, `check` erfolgreich | Environment `homebrew`; bricht ab, wenn das Environment keine Pflichtprüfer hat (wie Docker/ghcr); klont `<Eigentümer>/homebrew-openspider` mit `HOMEBREW_TAP_TOKEN`, übernimmt die geprüften Dateien per `update-tap`, Commit `openspider <Version>`, Push |

- Ohne `publish=true` und Freigabe passiert nichts Öffentliches. Auch das Release-Ereignis schreibt nicht in den Tap.
- `brew audit --new` setzt für fremde Taps als einziger Modus die Signatur-/Notarisierungsprüfung der geladenen App in Gang,
  prüft aber auch die Bekanntheit des GitHub-Repositorys (mindestens 75 Sterne oder 30 Forks/Beobachter, mindestens 30 Tage
  alt), die nur für Einreichungen in homebrew/cask gedacht ist. Deshalb `--except=github_repository` und nur als Hinweis in
  der Zusammenfassung; die Pflichtprüfung ist `--strict --online`. Signatur und Notarisierung prüft die Release-Checkliste
  ohnehin am Entwurf.
- Lesender `GITHUB_TOKEN` (`contents: read`) nur in den zwei Audit-Schritten als `HOMEBREW_GITHUB_API_TOKEN`, gegen das
  Ratenlimit der GitHub-API auf geteilten Runnern; nicht im Installationsschritt.
- Token-Handhabung wie `packaging/winget/publish.sh`: `gh` liest `GH_TOKEN` aus der Umgebung, Push über
  `gh auth git-credential`, kein Token in URL oder Argument.
- `release-scripts.yml` läuft jetzt auch bei Änderungen an `packaging/homebrew/**` und `homebrew.yml`, weil
  `homebrew.test.mjs` beide liest. Der neue Test läuft außerdem im prepare-Job von `release.yml`
  (`node --test scripts/release/*.test.mjs`); er braucht nur Node.

## Geprüft (Server `m3-brew`, Node 22.23.4)

| Prüfung | Ergebnis |
|---|---|
| `node --test scripts/release/*.test.mjs scripts/release/licenses/generate.test.mjs tests/release/orchestration.test.mjs` (mit `~/b3rel-tools` im PATH) | **46/46** grün (35 bestehende + 11 neue) |
| `node --test scripts/release/homebrew.test.mjs` | **11/11** |
| `node --test packaging/homebrew/generate.test.mjs` (P01, unverändert) | **6/6**; Ruby fehlt auf dem Server, dort Syntaxprobe übersprungen |
| `ruby -c` (lokal, Ruby 4.0.7, nur Syntax) für erzeugte `openspider@beta.rb` (beta.2) und `openspider.rb` (1.0.0) | Syntax OK; Unterschied der beiden nur Token, URL, Regex, Gegenstück |
| `actionlint` 1.7.12 + shellcheck 0.11.0 auf `homebrew.yml`, `release-scripts.yml`, `publish-packages.yml` | ohne Befund |
| Gegenprobe Workflow-Tests: `inputs.publish` entfernt, Environment `release` statt `homebrew`, zusätzlicher Auslöser `created`, Prüfer-Prüfung entfernt; zap-Pfad entfernt | je 1 Test rot, nach Rücknahme grün |
| `node website/check-site.mjs` | OK, 20 Seiten |
| `scripts/check-neutral.sh` (lokal, nach Commit) | keine Fundstellen |

## Erst in der CI bzw. nach Go prüfbar

Auf dem Server (Linux) gibt es kein Homebrew; lokal habe ich bewusst kein `brew` ausgeführt. Ungeprüft bis zum ersten Lauf:

- `brew style`/`brew audit` am Cask: insbesondere wie die Cops die P01-Schritte (`generated_script`, `postflight_steps`
  mit `run "/usr/bin/ruby"`) bewerten; der Cop `Cask/InstallSteps` hat eine eigene Meldung für Ruby-Schritte, deren
  Bedingung ich nicht verifizieren konnte. Fällt sie an, betrifft das P01 und den Tap gleichermaßen.
- Aufrufform `brew style --cask <tap>/<token>` und `brew audit --new --except=github_repository` mit einem lokalen Tap.
- Echte Installation des veröffentlichten DMG aus dem Tap, CLI-Link, `/usr/bin/ruby` auf `macos-latest`, Deinstallation.
- Auflösung von `conflicts_with` im Tap (beide Casks im selben Tap). In der P01-Smoke-Probe fehlt der Gegen-Cask im lokalen
  Tap; Homebrew ignoriert dann nicht vorhandene Konflikt-Casks. Beim nächsten Lauf von `package-manager-smoke.yml` beachten.
- `brew livecheck --cask` gegen die echten Releases.
- Job `publish`: Environment-Abfrage, Token-Rechte, Push in den Tap.
- Das Ereignis `release: published` nutzt die Workflow-Datei des getaggten Commits: der beta.3-Tag muss nach dem Merge
  von `main` geschnitten werden.

Vorschlag für die erste Probe nach dem Merge (schreibt nichts, Start durch Orchestrator/Nutzer):
`gh workflow run homebrew.yml --ref main -f version=1.0.0-beta.2` (beta.2 ist veröffentlicht und „Latest“).

## Schritte für den Nutzer

1. Öffentliches Repository `wojcikdominik/homebrew-openspider` anlegen (Zweig `main`; README mit
   `brew install --cask wojcikdominik/openspider/openspider@beta` erst nach der ersten Veröffentlichung).
2. Fine-grained Token nur für dieses Repository, „Contents: Read and write“, kurze Laufzeit.
3. Environment `homebrew` im Hauptrepo: Required reviewers, Deployment-Zweige nur `main`, Environment-Secret
   `HOMEBREW_TAP_TOKEN`.
4. Nach Veröffentlichung von beta.3: Lauf „Homebrew tap“ prüfen (startet von selbst). Nach Go
   `gh workflow run homebrew.yml --ref main -f version=1.0.0-beta.3 -f publish=true` und im Environment freigeben.
5. Danach Website/Handbuch und README auf den Installationsbefehl umstellen (eigenes Go, Website geht donnerstags live).

## Fremde Dateien

| Datei | Änderung | Grund |
|---|---|---|
| `packaging/homebrew/generate.mjs` (P01) | Cask-Text in exportierte `caskSource()` ausgelagert, `caskChannels`, `zapPaths`; livecheck, `conflicts_with`, zap sortiert + 3 WebKit-Pfade, keine Leerzeichen am Zeilenende | eine Quelle für Tap und P01-Veröffentlichung; `brew style` (ArrayAlphabetization, TrailingWhitespace). P01-Tests unverändert 6/6 |
| `.github/workflows/release-scripts.yml` | Pfadfilter `packaging/homebrew/**`, `.github/workflows/homebrew.yml`; Kopfkommentar | neuer Test liest beide |
| `docs/release/checkliste-release.md` | Punkt Homebrew in der Liste, Abschnitt Homebrew-Tap | Einrichtung und Ablauf |
| `docs/release/homebrew-winget.md` | Nachtrag B3-BREW | Abgrenzung zu P01, winget zur 1.0 |
| `README.md` | Satz Homebrew/winget | „prepared“, winget mit 1.0 |
| `website/site/download/index.html`, `website/site/en/download/index.html` | Abschnitt Paketmanager | „vorbereitet“, kein Befehl |
| `website/site/handbuch/index.html`, `website/site/en/manual/index.html` | Abschnitt und Inhaltsverzeichnis-Eintrag Paketmanager | dito |

## Spec-Vorschläge (Spec nicht geändert)

In `docs/spec/06-output-ops.md` gibt es keinen Eintrag zu Homebrew, winget oder Paketmanagern (Suche nach „Homebrew“,
„brew“, „winget“, „Paketmanager“: nur die Erwähnung von winget in OUT-266 und die Zuordnung in `docs/spec/README.md`).
K3-08 (`docs/plan/M3.md`) schlägt dafür schon **OUT-256 „Paketmanager“ (P1)** vor; die Nummer ist frei. Vorschlag:

> **OUT-256 Paketmanager (Homebrew, winget)** – P1. **(OS-Festlegung,** Nutzerentscheidung 05.11.2026) macOS über den
> eigenen Tap `<Projektkonto>/homebrew-openspider` ab beta.3, Windows über winget (`OpenSpider.OpenSpider`) ab 1.0.
> Cask `openspider@beta` folgt der neuesten veröffentlichten Version, `openspider` nur stabilen; beide installieren das
> Universal-DMG (OUT-261), schließen sich gegenseitig aus und legen die CLI in Homebrews `bin`.
> Akzeptanzkriterien: Casks werden aus `SHA256SUMS` des veröffentlichten Releases erzeugt (kein Entwurf, kein Pre-Release);
> `brew style` und `brew audit --strict --online` ohne Befund; Installation, Upgrade (`--greedy`, `auto_updates`) und
> Deinstallation erhalten bzw. entfernen Aufgaben-Registrierungen wie OUT-166; `--zap` entfernt nur die Pfade aus OUT-227
> und die der Bundle-ID; Veröffentlichung nur nach Freigabe; der Tap wird nie auf eine ältere Version gesetzt.
> Testfixture: `scripts/release/homebrew.test.mjs`, `packaging/homebrew/generate.test.mjs`, Workflow `homebrew.yml`.

Offene Fragen an die Spec:

- **zap und Crawls:** Der Standard-Crawl-Ordner liegt nach OUT-227 in `~/Library/Application Support/OpenSpider/crawls/`,
  also im zap-Pfad. `brew uninstall --zap` löscht damit auch Crawl-Dateien (P01-Entscheidung, hier unverändert). Soll zap
  `crawls/` aussparen? Dann müsste zap die Unterordner einzeln aufzählen.
- **Betas nach 1.0:** Gibt es nach 1.0 weitere Betas (z. B. 1.1.0-beta.1) und lesen diese wieder `latest-beta.json`?
  Davon hängt ab, ob `openspider@beta` nach 1.0 bestehen bleibt oder per `deprecate!` zugunsten von `openspider` endet.
- **OUT-166/K3-09:** „Deinstallation entfernt alle OS-Registrierungen“ gilt unter macOS nur für die Cask-Deinstallation,
  nicht für das Löschen im Finder; das Kriterium sollte das (wie in K3-09 vorgeschlagen) präzisieren.

## Folgepunkte

- **winget zur 1.0** (eigenes Paket): `publish-packages.yml`/`packaging/winget/publish.sh` auf winget beschränken; der
  Homebrew-Teil (Commit `Casks/openspider.rb`, Pflicht-Token `HOMEBREW_TAP_TOKEN` im Environment `release`) entfällt, weil
  `homebrew.yml` den Tap führt. Bis dahin ist `publish-packages.yml` ohne Opt-in-Variablen inaktiv. Schema-Abgleich und
  Paket-ID wie in `homebrew-winget.md` beschrieben.
- Optional: Linux-Formel `openspider-cli` aus dem Linux-Archiv, falls Nachfrage besteht.
- Nach erster Veröffentlichung: Installationsbefehl in Handbuch, Download-Seite (de/en) und README; Tap-README.
- Optional: Lesbarer `brew livecheck`-Lauf im Workflow als zusätzliche Prüfung.
