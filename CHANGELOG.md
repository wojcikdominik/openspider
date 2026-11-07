# Changelog

## [Unreleased]

- Frühe Beta für SEO-Agenturen in Vorbereitung. Plattform-Smoke, Archiv-Kompatibilität und noch offene M2-Parität bleiben vor der Freigabe zu prüfen.

## [1.0.0-beta.3] - 2026-11-06

Faster crawls, Chromium on demand, one-click agent connections and a report for checking deployments.

- Faster large crawls: crawl data is written in bigger batches, about 30 % less time for 100,000 and 1 million URLs in our benchmarks. The link score analysis now runs in memory (1 million URLs: 113 s → 23 s).
- JavaScript rendering no longer needs an installed Chrome: OpenSpider downloads a tested Chromium version on request and can remove it again.
- Connect Claude Desktop, Claude Code, Codex or Cursor to the MCP server in one step. Agents can only read by default; starting crawls needs the "Agent may start crawls" option. Existing configuration files keep their format, a backup is created, and read-only files are never changed.
- New report "Crawl Changes" compares a crawl with the previous crawl of the same project: removed pages, new redirects, changed canonicals and indexability. Useful after a deployment; the sub-filters can be exported.
- Crash reports are stored locally and only sent if you agree.
- Visualisations: 3D crawl and directory graphs, word cloud, and a chart for the selected entry of the overview.
- New "About OpenSpider" dialog with developer, website and source code links.
- Docker: a Dockerfile for running the CLI, exports and the MCP server on Linux servers (build the image yourself, see the user guide).
- Live crawl: the detail window stays responsive while a crawl runs; Shift+click and Shift+arrow selections are no longer lost during a running crawl; the Crawl Path dialog shows its rows again; a column choice in a detail table is kept when you select another URL; scrolling up in tail mode takes effect at once.
- Fixed possible crawl file damage: listing crawls from a second process (CLI, MCP server or a second app instance) while a crawl was running could damage that crawl's file. Listing now only reads such files and never changes them; locking a running crawl is no longer possible.
- Fixes: a stopped crawl stays "Stopped" after a restart; the status card shows "Finished (limit reached)" like the status bar; the findings sidebar follows the UI language; repairing a crawl file no longer counts write errors as lost rows; more reliable "Check for updates"; updated spreadsheet import library (security advisory RUSTSEC-2026-0436).

### Deutsch

Schnellere Crawls, Chromium auf Abruf, Agenten mit einem Klick verbinden und ein Bericht zum Prüfen von Deployments.

- Große Crawls schneller: Crawl-Daten werden in größeren Paketen geschrieben, in unseren Messungen rund 30 % weniger Zeit bei 100.000 und 1 Million URLs. Die Link-Score-Analyse läuft jetzt im Arbeitsspeicher (1 Million URLs: 113 s → 23 s).
- JavaScript-Rendering braucht kein installiertes Chrome mehr: OpenSpider lädt auf Wunsch eine geprüfte Chromium-Version herunter und kann sie auch wieder entfernen.
- Claude Desktop, Claude Code, Codex oder Cursor mit einem Schritt an den MCP-Server anbinden. Agenten dürfen standardmäßig nur lesen; Crawls starten sie erst mit der Option „Agent darf Crawls starten“. Vorhandene Konfigurationsdateien behalten ihr Format, es wird eine Sicherung angelegt, und schreibgeschützte Dateien werden nie verändert.
- Neuer Bericht „Änderungen seit dem letzten Crawl“ vergleicht einen Crawl mit dem vorigen Crawl desselben Projekts: entfernte Seiten, neue Weiterleitungen, geänderte Canonicals und Indexierbarkeit. Praktisch nach einem Deployment; die Unterfilter lassen sich exportieren.
- Absturzberichte werden lokal gespeichert und nur mit Zustimmung gesendet.
- Visualisierungen: 3D-Crawl- und Verzeichnisgraph, Wortwolke und ein Diagramm zum gewählten Eintrag der Übersicht.
- Neuer Dialog „Über OpenSpider“ mit Entwickler, Website und Link zum Quellcode.
- Docker: ein Dockerfile, um CLI, Exporte und den MCP-Server auf Linux-Servern zu betreiben (Image selbst bauen, siehe Handbuch).
- Live-Crawl: Das Detailfenster reagiert während eines Crawls flüssiger; Auswahlen mit Umschalt+Klick und Umschalt+Pfeil gehen im laufenden Crawl nicht mehr verloren; der Crawl-Pfad-Dialog zeigt wieder seine Zeilen; die Spaltenwahl einer Detailtabelle bleibt beim Wechsel zu einer anderen URL erhalten; Hochscrollen im Tail-Modus wirkt sofort.
- Mögliche Beschädigung von Crawl-Dateien behoben: Wurden Crawls aus einem zweiten Prozess aufgelistet (CLI, MCP-Server oder eine zweite App-Instanz), während ein Crawl lief, konnte dessen Datei beschädigt werden. Das Auflisten liest solche Dateien jetzt nur noch und ändert sie nie; laufende Crawls lassen sich nicht mehr sperren.
- Korrekturen: Ein gestoppter Crawl bleibt nach einem Neustart „Gestoppt“; die Statuskarte zeigt wie die Statusleiste „Fertig (Grenze erreicht)“; die Befunde-Seitenleiste folgt der Oberflächensprache; die Reparatur einer Crawl-Datei zählt Schreibfehler nicht mehr als verlorene Zeilen; „Nach Updates suchen“ ist zuverlässiger; die Bibliothek für den Tabellen-Import ist aktualisiert (Sicherheitshinweis RUSTSEC-2026-0436).

## [1.0.0-beta.2] - 2026-11-03

Wartungsversion ohne neue Funktionen: Sie prüft die automatische Aktualisierung von beta.1 auf eine neuere Version.

- App-Funktionen unverändert gegenüber beta.1.
- Bekannt: Der Chrome-Download auf Klick für JavaScript-Rendering folgt mit beta.3 (statt wie angekündigt mit beta.2).

## [1.0.0-beta.1] - 2026-11-03

Erste offene Beta. Rückmeldungen bitte über Hilfe → „Feedback geben…“ oder https://openspider.de/beta/.

- Desktop-App für macOS (Apple Silicon und Intel, signiert und notarisiert) und Windows x64 (signiert über Azure Artifact Signing).
- Automatische Update-Prüfung (abschaltbar) mit Download im Hintergrund; Installation sofort oder beim Beenden, nie während eines laufenden Crawls.
- Live-Crawl flüssiger: gemeinsame, gedrosselte Zählung für Übersicht, Probleme und Filter; Übersicht zeichnet nur geänderte Zeilen neu.
- „Top-Probleme“ und „Befunde“ laden nach Crawl-Ende schnell; Pause, Fortsetzen und Beenden halten die Crawl-Datei sauber.
- Alte Crawls und Einstellungen werden beim Öffnen übernommen.
- Bekannt: JavaScript-Rendering nutzt in dieser Beta ein installiertes Chrome; der Download auf Klick folgt mit beta.2.

## [1.0.0-beta.0] - 2026-10-30

- Technischer Trockenlauf der Release-Orchestrierung: Desktop-Installer, CLI-Archive, Update-Manifeste und SHA-256-Prüfsummen.
- Dieser Stand dient der Prüfung des Release-Prozesses. Kein freigegebener Produktionsstand.
