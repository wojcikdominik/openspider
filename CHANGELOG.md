# Changelog

## [Unreleased]

- Frühe Beta für SEO-Agenturen in Vorbereitung. Plattform-Smoke, Archiv-Kompatibilität und noch offene M2-Parität bleiben vor der Freigabe zu prüfen.

## [1.0.0-beta.4] - 2026-11-12

A new update dialog, live counters that keep up with the crawl, clear messages when the start URL cannot be reached, and a repaired comparison view.

- New update dialog: it shows a short summary of the new version in the UI language and offers "Skip this version", "Remind me later" and "Install on quit". A skipped version is only reported again when you check manually; newer versions are still announced. Switching off the automatic check keeps an already found release available.
- The version now carries a build number, for example "1.0.0-beta.4 (Build 2790)", in the About dialog, the debug info, the update dialog and `openspider --version`.
- Live crawl: the counters and the table footer follow the running crawl within one second instead of jumping with the next batch of rows. "Completed" only appears once the final counts are exact and the automatic crawl analysis has finished, so the overview and the issues are complete when the status changes.
- Unreachable start URL: if the start URL or its robots.txt cannot be fetched, the crawl ends as "Failed" and a dialog names the cause (DNS, connection refused, timeout, TLS) and the address. Before, the crawl simply ended with 0 URLs.
- Start URL blocked by robots.txt: the dialog names the robots.txt and the blocking line. It appears once per crawl and start URL.
- Failure mail: scheduled and command-line crawls that fail at the start URL send a mail marked `[FAILED]` with the error class, the address and the cause. The scheduler history lists the run as "Failed", and the automatic comparison is skipped for it.
- Comparison view repaired: it fills the table, detail and sidebar areas, uses the normal grouped tab bar and the same columns as the normal view with the comparison status next to the address, scrolls through all rows without pages, and shows "Comparison" as the mode. The values of the comparison status follow the UI language; copies and exports keep the canonical values.
- Crawl management: typing into the form fields and the search works again. "Recently opened" is now one list with the name or domain, date and time, and a path shortened in the middle. A crawl file that no longer exists is reported in the status bar with its path instead of a dialog with a raw error code. Ctrl/Cmd+F focuses the search in the whole dialog.
- Enter in the start URL field starts a crawl and resumes a paused one; with a changed URL OpenSpider asks before starting a new crawl. In dialogs with OK and Cancel, Enter confirms; destructive actions still need the focused button.
- Menu "Configuration": every entry now opens the configuration dialog on its own page; before, most entries stayed disabled. The list of unavailable menu actions in the status bar opens at its trigger, wraps long lines and names the actual reason.
- Sidebar overview: after a finished crawl the overview shows "Up to date" instead of a growing age.
- MCP: agents can pause, resume and stop crawls (`pause_crawl`, `resume_crawl`, `stop_crawl`) when the option "Agent may start crawls" is on. `crawl_status` reports the state of the run, including crawls started by another process.
- Detail window in a live crawl: range selections with Shift stay bound to one consistent state of the data; selecting "all" in very large tables is limited to 20,000 rows with a hint to use the export; errors while refreshing the detail window are shown and recover on their own; a column choice is saved before the window closes.
- Windows: the updater detects whether OpenSpider was installed with the MSI or the EXE installer and downloads the matching update.
- Form login: the login is confirmed before the login browser closes.
- Crawl files: the backup made before a file migration no longer drops the file lock; crawl management checks all locks before opening a file for writing.
- Security updates: SQLite 3.54.1 (RUSTSEC-2026-0459) and the MCP library rmcp 3.9.0 (RUSTSEC-2026-0457, -0461, -0462).

Known issue on macOS: if the crawl storage location is reached through a symbolic link (for example `/tmp`), "Recently opened" can list the same crawl file twice. A fix follows in the next version.

### Deutsch

Ein neuer Update-Dialog, Live-Zähler, die mit dem Crawl Schritt halten, klare Meldungen bei nicht erreichbarer Start-URL und eine reparierte Vergleichsansicht.

- Neuer Update-Dialog: Er zeigt eine kurze Zusammenfassung der neuen Version in der Oberflächensprache und bietet „Diese Version überspringen“, „Später erinnern“ und „Beim Beenden installieren“. Eine übersprungene Version wird nur noch bei der manuellen Prüfung gemeldet; neuere Versionen werden weiterhin angekündigt. Wer die automatische Prüfung abschaltet, behält ein bereits gefundenes Release.
- Die Version trägt jetzt eine Build-Nummer, zum Beispiel „1.0.0-beta.4 (Build 2790)“, im Dialog „Über OpenSpider“, in der Debug-Info, im Update-Dialog und in `openspider --version`.
- Live-Crawl: Zähler und Tabellenfuß folgen dem laufenden Crawl innerhalb einer Sekunde, statt mit dem nächsten Zeilenpaket zu springen. „Abgeschlossen“ erscheint erst, wenn die Endzahlen exakt sind und die automatische Crawl-Analyse fertig ist; Übersicht und Befunde sind beim Statuswechsel also vollständig.
- Start-URL nicht erreichbar: Lässt sich die Start-URL oder ihre robots.txt nicht abrufen, endet der Crawl als „Fehlgeschlagen“, und ein Dialog nennt Ursache (DNS, Verbindung verweigert, Timeout, TLS) und Adresse. Bisher endete der Crawl einfach mit 0 URLs.
- Start-URL per robots.txt gesperrt: Der Dialog nennt die robots.txt und die sperrende Zeile. Er erscheint je Crawl und Start-URL einmal.
- Fehler-Mail: Geplante Crawls und Crawls von der Kommandozeile, die an der Start-URL scheitern, senden eine Mail mit `[FAILED]`, Fehlerklasse, Adresse und Ursache. Die Historie des Planers führt den Lauf als „Fehlgeschlagen“, der automatische Vergleich wird dafür übersprungen.
- Vergleichsansicht repariert: Sie füllt Tabellen-, Detail- und Seitenleistenbereich, nutzt die normale gruppierte Tab-Leiste und dieselben Spalten wie die normale Ansicht mit dem Vergleichsstatus neben der Adresse, scrollt ohne Seiten durch alle Zeilen und zeigt als Modus „Vergleich“. Die Werte des Vergleichsstatus folgen der Oberflächensprache; Kopien und Exporte behalten die kanonischen Werte.
- Crawl-Verwaltung: Tippen in die Formularfelder und in die Suche funktioniert wieder. „Zuletzt geöffnet“ ist jetzt eine Liste mit Name oder Domain, Datum und Uhrzeit und einem in der Mitte gekürzten Pfad. Eine Crawl-Datei, die es nicht mehr gibt, wird mit ihrem Pfad in der Statusleiste gemeldet statt in einem Dialog mit rohem Fehlercode. Strg/Cmd+F setzt den Fokus im ganzen Dialog in die Suche.
- Enter im Start-URL-Feld startet einen Crawl und setzt einen pausierten fort; bei geänderter URL fragt OpenSpider nach, bevor ein neuer Crawl startet. In Dialogen mit OK und Abbrechen bestätigt Enter; zerstörerische Aktionen brauchen weiterhin den fokussierten Knopf.
- Menü „Konfiguration“: Jeder Eintrag öffnet jetzt den Konfigurationsdialog auf seiner Seite; bisher blieben die meisten Einträge ausgegraut. Die Liste der nicht verfügbaren Menüaktionen in der Statusleiste öffnet an ihrem Auslöser, bricht lange Zeilen um und nennt den tatsächlichen Grund.
- Übersicht in der Seitenleiste: Nach einem fertigen Crawl steht dort „Stand aktuell“ statt eines wachsenden Alters.
- MCP: Agenten können Crawls pausieren, fortsetzen und stoppen (`pause_crawl`, `resume_crawl`, `stop_crawl`), wenn die Option „Agent darf Crawls starten“ aktiv ist. `crawl_status` nennt den Zustand des Laufs, auch bei Crawls aus einem anderen Prozess.
- Detailfenster im Live-Crawl: Bereichsauswahlen mit Umschalt bleiben an einen einheitlichen Datenstand gebunden; „Alle auswählen“ ist in sehr großen Tabellen auf 20.000 Zeilen begrenzt und verweist auf den Export; Fehler beim Aktualisieren des Detailfensters werden angezeigt und erholen sich von selbst; eine Spaltenwahl wird gespeichert, bevor das Fenster schließt.
- Windows: Der Updater erkennt, ob OpenSpider mit dem MSI- oder dem EXE-Installer installiert wurde, und lädt das passende Update.
- Formular-Login: Die Anmeldung wird bestätigt, bevor der Login-Browser schließt.
- Crawl-Dateien: Die Sicherung vor einer Dateimigration verliert die Dateisperre nicht mehr; die Crawl-Verwaltung prüft alle Sperren, bevor sie eine Datei zum Schreiben öffnet.
- Sicherheits-Updates: SQLite 3.54.1 (RUSTSEC-2026-0459) und die MCP-Bibliothek rmcp 3.9.0 (RUSTSEC-2026-0457, -0461, -0462).

Bekannter Fehler unter macOS: Liegt der Crawl-Speicherort hinter einer symbolischen Verknüpfung (zum Beispiel `/tmp`), kann „Zuletzt geöffnet“ dieselbe Crawl-Datei zweimal zeigen. Die Korrektur folgt mit der nächsten Version.

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
