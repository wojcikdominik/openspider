# Release-Fundament M3

Die maßgebliche Version steht in `[workspace.package] version`. Plattformneutrale
Node-Skripte (ab Node 22), aus jedem Arbeitsverzeichnis aufrufbar:

```
node scripts/release/set-version.mjs 1.0.0-beta.1
node scripts/release/check-version.mjs
```

`set-version` aktualisiert die npm-Workspace-/Lockversionen, Tauri und die lokalen
Workspace-Paketversionen im Cargo-Lockfile. Der Abhängigkeitsbaum bleibt erhalten;
kein `cargo update`. M3-R04 prüft den Versionsgleichstand in CI. Die MSI-Abbildung
steht in M3-R02 (K3-07).

Bundle-ID: `org.openspider.desktop` (K3-02). Vorläufige Orchestrator-Entscheidungen
vom 30.10.2026: GitHub Releases (K3-03), Beta-Datei per Build-Variable (K3-04,
abgelöst durch K3-16 vom 21.11.2026, siehe unten), Azure Artifact Signing für
Windows (K3-05; Umsetzung M3-R02).

Build-Variablen:

- `OPENSPIDER_UPDATER_PUBKEY`: öffentlicher minisign-Schlüssel; wird von build.rs
  gezielt als `plugins.updater.pubkey` übernommen. Ohne gesetzte Schlüssel-/Beta-Variable
  bleibt die eingehende Updater-Konfiguration (auch `--config`/`TAURI_CONFIG`) erhalten.
  Ein Release-Build mit leerem effektivem Schlüssel gibt eine Cargo-Warnung aus.
  Kein Produkt- oder Testschlüssel im Repo-Config.
- `OPENSPIDER_UPDATE_BETA=1`: `latest-beta.json`, bei `0` `latest.json`. Die Variable
  setzt ausschließlich den Endpunkt; ein eingehender Schlüssel bleibt erhalten.
  Bei allein gesetztem Schlüssel ergänzt der Build `latest.json`, sofern kein
  Endpunkt-Override vorliegt. Ohne Build-Variablen bleibt der Endpunkt-Platzhalter leer.
  Einzige Kanalvariable für Update-Kern, CLI und Tauri-Adapter; Windows wählt
  sein NSIS-/MSI-Target über den Tauri-Updater selbst. **Gilt bis einschließlich
  1.0.0.** Ab 1.0.1 legt nicht mehr der Build den Kanal fest, sondern die Einstellung
  `updates.channel` (K3-16, OUT-254); was mit der Variable geschieht, entscheidet das
  Paket Release-Tooling 1.0.1.
- `OPENSPIDER_UPDATE_REPO`: GitHub-Repository als `owner/repo`; der Standard steht
  einmalig in `apps/desktop/src-tauri/build_update.rs` und wird von `build.rs` verwendet.
  Endpunkte entstehen als `https://github.com/<repo>/releases/latest/download/<manifest>`.
  Diese Variable allein überschreibt keine eingehende Updater-Konfiguration.
- Der feste HTTP-User-Agent des Update-Kerns ist `OpenSpider-Updater`, ohne Kennung.

~~K3-04, Orchestrator-Entscheid vom 30.10.2026: Bis 1.0 werden Betas als **normale
GitHub-Releases**, ohne Pre-Release-Markierung, veröffentlicht. Jeder Release trägt
beide Manifeste `latest.json` und `latest-beta.json`. Beta-Builds verwenden
`releases/latest/download/latest-beta.json`.~~ (abgelöst durch K3-16)

**Update-Kanäle ab 1.0 (K3-16, Nutzerentscheidung 21.11.2026, Spec OUT-254/OUT-261):**

- **„Latest“ ist auf GitHub immer die neueste stabile Version.** Stabile Releases
  werden als Latest veröffentlicht. Betas werden mit `--latest=false` veröffentlicht
  und weiterhin **nicht** als Pre-Release markiert (Homebrew lehnt Pre-Releases ab).
  Damit liefern `releases/latest/download/latest.json` und die versionslosen
  Download-Links (`OpenSpider-macOS.dmg`, `OpenSpider-Windows-Setup.exe`) immer die
  stabile Version.
- **Manifeste je Release:** Ein stabiles Release trägt `latest.json` und
  `latest-beta.json` mit identischem Inhalt (Clients bis 1.0.0 haben ihren Kanal im
  Build und lesen beide Dateien über `releases/latest/download/…`). Ein Beta-Release
  trägt **kein** `latest.json`, nur `latest-beta.json`.
- **Beta-Manifest am festen Tag:** Der Beta-Kanal ab 1.0.1 liest
  `https://github.com/wojcikdominik/openspider/releases/download/update-beta/latest-beta.json`.
  Das Release mit dem festen Tag `update-beta` ist als Pre-Release markiert, wird nie
  Latest und enthält nur diese eine Datei. Sie nennt immer die **neueste Version
  insgesamt** (Beta oder stabil, Maximum nach SemVer). Angelegt wird das Release
  einmalig vom Eigentümer; aktualisiert wird die Datei nur nach dessen Go
  (`--clobber`). Ablauf: [release-process.md](release-process.md), Abschnitt
  „Update-Kanäle“.
- Der stabile Kanal ab 1.0.1 liest weiter `releases/latest/download/latest.json`.

Die Veröffentlichung bleibt an das Go des Nutzers gebunden.

U01/U02 implementieren Prüfung, Hintergrund-Download und Installation über den
Tauri-Updater. Fehlende oder ungültige öffentliche Schlüssel deaktivieren den
Updater sicher; es entstehen keine Update-Requests. Die App bietet manuelle
Prüfung und automatische Prüfungen gemäß Einstellung und Umgebungsvariable.
Installation berücksichtigt laufende Crawls und bietet Jetzt/Beim Beenden/Nach
Crawl-Ende; während eines laufenden Crawls ausschließlich die letzte Wahl.

Release-Builds müssen vor dem Desktop-Build die CLI nach
`apps/desktop/src-tauri/binaries/openspider-<target-triple>[.exe]` legen. Debug-Checks
ohne gestagte CLI entfernen nur für diesen Build den externalBin-Eintrag; Release
verlangt das echte Binary. M3-R01/R02 erstellen die nativen Bundles. M3-L01 erzeugt
vollständige Lizenztexte unter `resources/third-party`. Nur die nativen
Release-Build-Schritte setzen `OPENSPIDER_REQUIRE_NOTICES=1`. M3-R04 ruft
`licenses.yml` als Gate auf und übernimmt `third-party-notices/THIRD_PARTY_NOTICES.txt`
in die CLI-Archive für Linux, Windows und macOS (arm64/x64/universal).

Mock: `os_testsite::mocks::update`, Routen unter `/update`: `valid/latest.json`,
`tampered/latest.json`, `older/latest.json`, `http/latest.json`, `artifact`,
`interrupted` (Download-Abbruch). M3-U01 stellt Testschlüssel und kryptographisch
signierte Daten ausschließlich in `tests/fixtures/update` bereit. M3-D01 füllt
das Migrationsarchiv.

Veröffentlichen, Signieren mit Produktzugängen und Push sind kein Teil von F01.
