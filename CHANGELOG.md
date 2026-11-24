# Changelog

## [Unreleased]

- Frühe Beta für SEO-Agenturen in Vorbereitung. Plattform-Smoke, Archiv-Kompatibilität und noch offene M2-Parität bleiben vor der Freigabe zu prüfen.

## [1.0.2] - 2026-11-25

OpenSpider 1.0.2 improves how AI assistants see the built-in MCP server. Crawling, analysis and exports work as in 1.0.1.

- Tool hints: every MCP tool now has a title and states whether it only reads data. `crawl_status` is marked as read-only, `stop_crawl` as the one tool that ends a crawl for good. Assistants such as Claude use these hints to decide when to ask before calling a tool. The permission classes in OpenSpider's MCP settings are unchanged: by default only the read tools are available.
- English descriptions: tool descriptions, parameter descriptions and the server's notes to the assistant are now English throughout; five tools had German descriptions before. The tool table under MCP in the app and the downloadable tool description show the same English texts in every interface language.
- The note that content from crawled pages is data and never an instruction stays in place, now in English.

### Deutsch

OpenSpider 1.0.2 verbessert, wie KI-Assistenten den eingebauten MCP-Server sehen. Crawlen, Analyse und Exporte arbeiten wie in 1.0.1.

- Werkzeug-Hinweise: Jedes MCP-Werkzeug hat jetzt einen Titel und gibt an, ob es nur Daten liest. `crawl_status` ist als nur lesend gekennzeichnet, `stop_crawl` als das eine Werkzeug, das einen Crawl endgültig beendet. Assistenten wie Claude nutzen diese Hinweise, um zu entscheiden, wann sie vor einem Aufruf nachfragen. Die Freigabeklassen in den MCP-Einstellungen von OpenSpider sind unverändert: Standardmäßig stehen nur die lesenden Werkzeuge bereit.
- Englische Beschreibungen: Werkzeugbeschreibungen, Parameterbeschreibungen und die Hinweise des Servers an den Assistenten sind jetzt durchgehend englisch; bisher hatten fünf Werkzeuge deutsche Beschreibungen. Die Werkzeugtabelle unter MCP in der App und die herunterladbare Werkzeugbeschreibung zeigen in jeder Oberflächensprache dieselben englischen Texte.
- Der Hinweis, dass Inhalte gecrawlter Seiten Daten sind und nie Anweisungen, bleibt erhalten, jetzt auf Englisch.

## [1.0.1] - 2026-11-25

OpenSpider 1.0.1 adds the choice between a stable and a beta update channel and updates third-party libraries. Crawling, analysis and exports work as in 1.0.0.

- Update channel: Settings → Updates has the new setting "Update channel" with "Stable (recommended)" and "Beta". An installed stable version such as 1.0.1 starts on the stable channel and stays there unless you change it. The stable channel only receives finished versions with fixes.
- Beta channel: beta versions bring new features earlier and may contain bugs. On the beta channel you also receive every newer stable version. The change takes effect with the next update check.
- Back to stable: if you switch from Beta to Stable while a beta version is installed, OpenSpider stays on that version until a newer stable version is released. Nothing is downgraded.
- Libraries: the libraries for the Excel export, for ZIP archives, for e-mail delivery and for URL handling were updated. This closes security advisories that were reported for the previous versions of these libraries.

### Deutsch

OpenSpider 1.0.1 bringt die Wahl zwischen einem stabilen und einem Beta-Update-Kanal und aktualisiert fremde Bibliotheken. Crawlen, Analyse und Exporte arbeiten wie in 1.0.0.

- Update-Kanal: Unter Einstellungen → Updates gibt es die neue Einstellung „Update-Kanal“ mit „Stabil (empfohlen)“ und „Beta“. Eine installierte stabile Version wie 1.0.1 startet im stabilen Kanal und bleibt dort, solange Sie nichts ändern. Der stabile Kanal erhält nur fertige Versionen mit Korrekturen.
- Beta-Kanal: Beta-Versionen bringen neue Funktionen früher und können Fehler enthalten. Im Beta-Kanal erhalten Sie auch jede neuere stabile Version. Die Änderung gilt ab der nächsten Update-Prüfung.
- Zurück zu Stabil: Wer von Beta auf Stabil wechselt, während eine Beta-Version installiert ist, bleibt auf dieser Version, bis eine neuere stabile Version erscheint. Es wird nichts zurückgestuft.
- Bibliotheken: Die Bibliotheken für den Excel-Export, für ZIP-Archive, für den E-Mail-Versand und für die URL-Verarbeitung wurden aktualisiert. Damit sind Sicherheitshinweise geschlossen, die für die bisherigen Versionen dieser Bibliotheken gemeldet waren.

## [1.0.0] - 2026-11-17

OpenSpider 1.0 is the first stable release. Compared with the release candidate it fixes PageSpeed with a local Lighthouse and a wrong crash notice after updates on Windows.

- PageSpeed, source "Local": connecting and crawling now work without an API key. Before, the connection asked for a key even though the local source does not need one, and the crawl did not run Lighthouse on this computer. Choose "Local (Lighthouse on this computer)" under API Access › PageSpeed Insights and enter the full path to the installed Lighthouse command line in the new field "Lighthouse path": on macOS and Linux the output of "which lighthouse", on Windows the file "lighthouse\cli\index.js" in the global npm folder. The path is required; without it the status shows "Lighthouse runtime not found". Field data from CrUX is only available with the source "Remote".
- PageSpeed, source "Local", on Windows: the starters "lighthouse.cmd", "lighthouse.bat" and "lighthouse.ps1" are accepted as path and resolved to the Lighthouse program itself.
- Updates on Windows: after an update had been installed, the next start could report that OpenSpider had not been closed properly and offer a crash report. The update now ends the session in an orderly way. If the installer does not start, for example because the permission prompt was cancelled, a real crash afterwards is still detected.

Known limitations:

- After the update to 1.0 the crash notice can appear once more on Windows, because the update is still installed by the previous version. It can be dismissed; nothing was lost.
- PageSpeed, source "Local": OpenSpider needs an installed Lighthouse and an installed Chrome; neither is included. If Chrome is missing, the row only shows "LIGHTHOUSE_CRASHED" with exit code 1.
- PageSpeed, source "Local": one Lighthouse run at a time with a time limit of 90 seconds per URL; neither can be changed yet. Pages behind HTTP authentication cannot be measured locally, because the login from the crawl configuration does not reach Lighthouse.

### Deutsch

OpenSpider 1.0 ist die erste stabile Version. Gegenüber dem Release-Kandidaten behebt sie PageSpeed mit lokalem Lighthouse und einen falschen Absturzhinweis nach Updates unter Windows.

- PageSpeed, Quelle „Lokal“: Verbinden und Crawlen funktionieren jetzt ohne API-Schlüssel. Bisher verlangte die Verbindung einen Schlüssel, obwohl die lokale Quelle keinen braucht, und der Crawl ließ Lighthouse nicht auf diesem Rechner laufen. Unter API-Zugriff › PageSpeed Insights „Lokal (Lighthouse auf diesem Rechner)“ wählen und im neuen Feld „Pfad zu Lighthouse“ den vollständigen Pfad zur installierten Lighthouse-Kommandozeile eintragen: unter macOS und Linux die Ausgabe von „which lighthouse“, unter Windows die Datei „lighthouse\cli\index.js“ im globalen npm-Ordner. Der Pfad ist nötig; ohne ihn meldet der Status, dass die Lighthouse-Laufzeit nicht gefunden wurde. CrUX-Felddaten gibt es nur mit der Quelle „Remote“.
- PageSpeed, Quelle „Lokal“, unter Windows: Die Starter „lighthouse.cmd“, „lighthouse.bat“ und „lighthouse.ps1“ werden als Pfad angenommen und auf das eigentliche Lighthouse-Programm aufgelöst.
- Updates unter Windows: Nach einem installierten Update konnte der nächste Start melden, OpenSpider sei nicht ordentlich beendet worden, und einen Absturzbericht anbieten. Das Update beendet die Sitzung jetzt ordentlich. Startet der Installer nicht, zum Beispiel weil die Rechteabfrage abgebrochen wurde, wird ein echter Absturz danach weiterhin erkannt.

Bekannte Einschränkungen:

- Nach dem Update auf 1.0 kann der Absturzhinweis unter Windows noch einmal erscheinen, weil das Update noch von der vorherigen Version installiert wird. Er lässt sich schließen; es ist nichts verloren gegangen.
- PageSpeed, Quelle „Lokal“: OpenSpider braucht ein installiertes Lighthouse und ein installiertes Chrome; beides ist nicht enthalten. Fehlt Chrome, zeigt die Zeile nur „LIGHTHOUSE_CRASHED“ mit Exit-Code 1.
- PageSpeed, Quelle „Lokal“: ein Lighthouse-Lauf zur gleichen Zeit mit einem Zeitlimit von 90 Sekunden je URL; beides lässt sich noch nicht ändern. Seiten hinter HTTP-Authentifizierung lassen sich lokal nicht messen, weil die Anmeldung aus der Crawl-Konfiguration Lighthouse nicht erreicht.

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
