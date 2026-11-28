# Changelog

## [Unreleased]

- Frühe Beta für SEO-Agenturen in Vorbereitung. Plattform-Smoke, Archiv-Kompatibilität und noch offene M2-Parität bleiben vor der Freigabe zu prüfen.

## [1.0.4] - 2026-12-03

OpenSpider 1.0.4 fixes errors in the display at the end of a crawl, in the saved choice of "Overview" and in the automatic crawl retention. Crawling, analysis and exports otherwise work as in 1.0.3.

- Overview at the end of a crawl: after a crawl ended, the Overview and Issues tabs in the sidebar could stay at an earlier count, and the final status such as "Completed" could appear before the final figures. The final status now appears once Overview and Issues show the final state, at most 3 seconds after the end of the crawl. If neither tab is open, nothing waits.
- Very short crawls and crawls from elsewhere: if a window sees a crawl for the first time when it has already ended, for example a very short crawl or one started in another window, via MCP or on the command line, the final status appears at once. Overview and Issues then load the final figures without any action and show no figures until then, never those of a previous crawl.
- No avoidable wait: OpenSpider now counts the final state, including the changes made by the automatic crawl analysis, before it reports the end of the crawl. The result of the automatic analysis appears in the status bar together with the final status, not before. After a quiet phase the status bar shows a change at once instead of a quarter of a second later; it still updates at most four times per second.
- "Overview" at crawl start: if you chose "Overview" in the navigation above the tabs shortly after OpenSpider had started, starting a crawl could switch back to the tabs. This happened mainly on macOS. The choice now stays.
- Saved interface settings: saving the window layout, for example after you resized the sidebar, could overwrite an interface setting saved at the same moment, such as the choice of "Overview". OpenSpider now keeps both.
- Crawl retention: after a change under Settings → Crawls, a pending cleanup could still delete by the setting that applied when it was triggered, for example after you had switched back to "Never delete automatically". Two quick changes in a row could also leave the earlier one in effect for the hourly cleanup. Each cleanup now reads the saved settings when it starts. With "Never delete automatically", the default, no cleanup runs in the background.
- Quitting during the crawl retention: if you quit OpenSpider while the automatic retention is running, OpenSpider now waits for the step in progress, at most 5 seconds, and skips steps that have not started yet.

### Deutsch

OpenSpider 1.0.4 behebt Fehler in der Anzeige am Ende eines Crawls, bei der gespeicherten Wahl „Übersicht“ und in der automatischen Crawl-Aufbewahrung. Crawlen, Analyse und Exporte arbeiten sonst wie in 1.0.3.

- Übersicht am Ende eines Crawls: Nach dem Ende eines Crawls konnten die Tabs „Übersicht“ und „Befunde“ in der Seitenleiste auf einem älteren Stand stehen bleiben, und der Endstatus wie „Fertig“ konnte vor den Endzahlen erscheinen. Der Endstatus erscheint jetzt, sobald „Übersicht“ und „Befunde“ den Endstand zeigen, längstens 3 Sekunden nach dem Ende des Crawls. Ist keiner der beiden Tabs geöffnet, wird nicht gewartet.
- Sehr kurze Crawls und Crawls von anderswo: Sieht ein Fenster einen Crawl zum ersten Mal, wenn er schon beendet ist, zum Beispiel einen sehr kurzen Crawl oder einen, der in einem anderen Fenster, über MCP oder auf der Kommandozeile gestartet wurde, erscheint der Endstatus sofort. „Übersicht“ und „Befunde“ laden die Endzahlen dann ohne Ihr Zutun nach und zeigen bis dahin keine Zahlen, nie die eines vorherigen Crawls.
- Keine vermeidbare Wartezeit: OpenSpider zählt den Endstand jetzt einschließlich der Änderungen durch die automatische Crawl-Analyse, bevor es das Ende des Crawls meldet. Das Ergebnis der automatischen Analyse erscheint in der Statusleiste zusammen mit dem Endstatus, nicht davor. Nach einer Ruhephase zeigt die Statusleiste eine Änderung sofort statt eine Viertelsekunde später; sie aktualisiert weiterhin höchstens viermal pro Sekunde.
- „Übersicht“ beim Crawl-Start: Wenn Sie kurz nach dem Start von OpenSpider „Übersicht“ in der Navigation über den Tabs gewählt hatten, konnte ein Crawl-Start wieder zu den Tabs wechseln. Das trat vor allem unter macOS auf. Die Wahl bleibt jetzt erhalten.
- Gespeicherte Oberflächeneinstellungen: Das Speichern des Fensterlayouts, zum Beispiel nachdem Sie die Seitenleiste verbreitert hatten, konnte eine im selben Moment gespeicherte Oberflächeneinstellung überschreiben, etwa die Wahl „Übersicht“. OpenSpider behält jetzt beide.
- Crawl-Aufbewahrung: Nach einer Änderung unter Einstellungen → Crawls konnte eine anstehende Aufräumung noch nach der Einstellung löschen, die beim Auslösen galt, zum Beispiel nachdem Sie wieder auf „Nie automatisch löschen“ gestellt hatten. Zwei schnelle Änderungen hintereinander konnten außerdem die ältere für die stündliche Aufräumung gelten lassen. Jede Aufräumung liest jetzt beim Beginn die gespeicherten Einstellungen. Mit „Nie automatisch löschen“, der Voreinstellung, läuft im Hintergrund keine Aufräumung.
- Beenden während der Crawl-Aufbewahrung: Beenden Sie OpenSpider, während die automatische Aufbewahrung läuft, wartet OpenSpider jetzt auf den laufenden Schritt, höchstens 5 Sekunden, und lässt noch nicht begonnene Schritte aus.

## [1.0.3] - 2026-11-27

OpenSpider 1.0.3 fixes two errors that have existed since 1.0.0, one in the main table and one in dialogs, and improves how AI assistants see the built-in MCP server. Crawling, analysis and exports otherwise work as in 1.0.1. A version 1.0.2 was not released; its changes are part of this version.

- The error in the main table: right after you started a crawl, opened a saved crawl or cleared the current one, the table still showed the rows of the previous crawl for a moment, and they could be selected. If you then chose "Remove" or "Re-crawl", the action could hit a different URL in the new crawl. This needed a click within that short moment and a new crawl that was already finished, for example a small or a saved one.
- The table: the rows of the previous crawl now disappear as soon as you start, open or clear a crawl. A selection made in the new crawl stays when its first data arrives.
- Protection in the core: "Remove", "Re-crawl", API requests for selected URLs and the export of detail data now state which crawl the selection comes from. If that crawl is no longer the open one, OpenSpider changes nothing and shows the message "The selected rows belong to a crawl that is no longer open. Nothing was changed."
- Dialogs: if you pressed Escape right after a dialog opened, for example in the export dialog after its keyboard shortcut, the key press could be lost and the dialog stayed open. Escape now closes the topmost open dialog, no matter where the keyboard focus is. If a question lies on top of a dialog, each Escape closes one level. A search field or an open selection list inside a dialog still handles Escape first.
- MCP tool hints: every MCP tool now has a title and states whether it only reads data. `crawl_status` is marked as read-only, `stop_crawl` as the one tool that ends a crawl for good. Assistants such as Claude use these hints to decide when to ask before calling a tool. The permission classes in OpenSpider's MCP settings are unchanged: by default only the read tools are available.
- English MCP descriptions: tool descriptions, parameter descriptions and the server's notes to the assistant are now English throughout; five tools had German descriptions before. The tool table under MCP in the app and the downloadable tool description show the same English texts in every interface language. The note that content from crawled pages is data and never an instruction stays in place, now in English.

### Deutsch

OpenSpider 1.0.3 behebt zwei Fehler, die seit 1.0.0 bestehen, einen in der Haupttabelle und einen in Dialogen, und verbessert, wie KI-Assistenten den eingebauten MCP-Server sehen. Crawlen, Analyse und Exporte arbeiten sonst wie in 1.0.1. Eine Version 1.0.2 ist nicht erschienen; ihre Änderungen sind in dieser Version enthalten.

- Der Fehler in der Haupttabelle: Direkt nachdem Sie einen Crawl gestartet, einen gespeicherten Crawl geöffnet oder den aktuellen geleert hatten, zeigte die Tabelle kurz noch die Zeilen des vorherigen Crawls, und sie ließen sich markieren. Wählten Sie dann „Entfernen“ oder „Erneut crawlen“, konnte die Aktion eine andere URL im neuen Crawl treffen. Dafür brauchte es einen Klick in diesem kurzen Moment und einen neuen Crawl, der schon fertig war, zum Beispiel einen kleinen oder einen gespeicherten.
- Die Tabelle: Die Zeilen des vorherigen Crawls verschwinden jetzt, sobald Sie einen Crawl starten, öffnen oder leeren. Eine Auswahl im neuen Crawl bleibt erhalten, wenn dessen erste Daten eintreffen.
- Absicherung im Kern: „Entfernen“, „Erneut crawlen“, API-Abrufe für markierte URLs und der Export von Detaildaten nennen jetzt den Crawl, aus dem die Auswahl stammt. Ist dieser Crawl nicht mehr der geöffnete, ändert OpenSpider nichts und zeigt die Meldung „Die markierten Zeilen gehören zu einem Crawl, der nicht mehr geöffnet ist. Es wurde nichts geändert.“
- Dialoge: Wenn Sie direkt nach dem Öffnen eines Dialogs Escape drückten, zum Beispiel im Export-Dialog nach dessen Tastenkürzel, konnte der Tastendruck verloren gehen und der Dialog blieb offen. Escape schließt jetzt den obersten offenen Dialog, egal wo der Tastaturfokus liegt. Liegt eine Rückfrage über einem Dialog, schließt jedes Escape eine Ebene. Ein Suchfeld oder eine geöffnete Auswahlliste im Dialog verarbeitet Escape weiterhin zuerst.
- MCP-Werkzeug-Hinweise: Jedes MCP-Werkzeug hat jetzt einen Titel und gibt an, ob es nur Daten liest. `crawl_status` ist als nur lesend gekennzeichnet, `stop_crawl` als das eine Werkzeug, das einen Crawl endgültig beendet. Assistenten wie Claude nutzen diese Hinweise, um zu entscheiden, wann sie vor einem Aufruf nachfragen. Die Freigabeklassen in den MCP-Einstellungen von OpenSpider sind unverändert: Standardmäßig stehen nur die lesenden Werkzeuge bereit.
- Englische MCP-Beschreibungen: Werkzeugbeschreibungen, Parameterbeschreibungen und die Hinweise des Servers an den Assistenten sind jetzt durchgehend englisch; bisher hatten fünf Werkzeuge deutsche Beschreibungen. Die Werkzeugtabelle unter MCP in der App und die herunterladbare Werkzeugbeschreibung zeigen in jeder Oberflächensprache dieselben englischen Texte. Der Hinweis, dass Inhalte gecrawlter Seiten Daten sind und nie Anweisungen, bleibt erhalten, jetzt auf Englisch.

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
