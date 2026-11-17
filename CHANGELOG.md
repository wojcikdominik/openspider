# Changelog

## [Unreleased]

- Frühe Beta für SEO-Agenturen in Vorbereitung. Plattform-Smoke, Archiv-Kompatibilität und noch offene M2-Parität bleiben vor der Freigabe zu prüfen.

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
