# 11 – Teams: Crawls im Team teilen (ENTWURF)

**Status:** Entwurf Rev. 2 vom 09.11.2026, nach Fable-Prüfung (Nutzeridee, Ausarbeitung Orchestrator; Rev. 1: Score 64, `needs_work`). Keinem Meilenstein zugeordnet, Umsetzung frühestens nach Release 1.0 (Roadmap R10). Vor der Planung klärt der Nutzer die offenen Fragen am Ende; der Paketschnitt für Stufe 1 steht in Abschnitt C.

Ziel: Mehrere Personen eines Teams arbeiten mit denselben Crawls, ohne dass OpenSpider selbst crawlt oder Crawl-Server betreibt. Gecrawlt wird weiterhin auf dem Rechner der Nutzer; das Team teilt die Ergebnisse.

Grundsätze:
- **Crawlen bleibt lokal.** Kein Team-Merkmal startet Crawls auf fremden oder eigenen Servern. Ein Teammitglied kann einen eigenen Rechner (z. B. im Büro) als Crawl-Rechner betreiben; das ist ein normaler OpenSpider mit geplanten Crawls.
- **Geteilt werden unveränderliche Schnappschüsse.** Eine veröffentlichte Version ändert sich nie; eine neue Veröffentlichung ist eine neue Version. Jede Crawl-Datei hat genau einen Besitzer, alle anderen lesen.
- **Keine Geheimnisse im Team.** Zugangsdaten, Cookies, Header-Werte und Tokens verlassen den Rechner nicht (TEAM-003); Proxy- und Formular-Login-Daten liegen ohnehin nicht in der Crawl-Datei.
- **Autark bleibt autark.** Ohne Team-Einrichtung ändert sich an OpenSpider nichts; Team-Funktionen sind ausschaltbar und brauchen in Stufe 1 weder Konto noch Netzwerkdienst von OpenSpider.
- **Zwei Stufen** (Geschäftsmodell Open Core, `docs/plan/ROADMAP-nach-1.0.md`): **Stufe 1 „Team-Ordner“** ist frei (Apache-2.0) und nutzt einen gemeinsamen Ordner (NAS, Netzlaufwerk, Dropbox, Google Drive, OneDrive o. Ä.). **Stufe 2 „OpenSpider Teams“** ist ein bezahlter, gehosteter Dienst über den evnxt-Account (Anmeldung, Rechte, Verlauf, Alarme, Kommentare, Kundenansicht).

## Gemeinsame Begriffe (gelten für alle Einträge)

- **Installations-ID:** zufällige UUID, die jede Installation beim ersten Team-Gebrauch erzeugt und lokal speichert; keine Hardware- oder Systemkennung. Sie ersetzt keine Signatur (siehe „Entschieden“ am Ende).
- **Versionsschlüssel:** `<crawl-id>/<zeitstempel>-<installations-id>`. Der Zeitstempel ist UTC im Format `YYYYMMDDTHHMMSSmmmZ` (Zeitpunkt des Schnappschusses). Die **fortlaufende Nummer** („Version 1, 2, …“) wird nie gespeichert, sondern lokal aus der Sortierung der Versionen einer Crawl-ID berechnet (Zeitstempel, dann Installations-ID als Tiebreak). Zwei Installationen können deshalb nie um dieselbe Nummer konkurrieren.
- **Ordnerlayout:** `<team>/team.json`, `<team>/crawls/<crawl-id>/<zeitstempel>-<installations-id>.<manifest|crawl-datei>`, `<team>/.staging/<installations-id>/` (Zwischenablage, wird nie gelistet), `<team>/.papierkorb/` (TEAM-007). Dateinamen enthalten ausschließlich UUIDs und den Zeitstempel, nie Nutzertexte (Domain, Projekt und Anzeigename stehen nur im Manifest).
- **Zustände einer Version in der Liste:** *verfügbar* (Manifest gültig, Datei vorhanden, Größe passt), *unvollständig synchronisiert* (Prüfsumme beim Laden in den Zwischenspeicher weicht ab), *beschädigt/verändert* (Abweichung bleibt bestehen, TEAM-008), *Konflikt* (Datei ohne passendes Manifest), *benötigt neuere App* (TEAM-006), *zurückgezogen* (nur für bereits lokal zwischengespeicherte Versionen, TEAM-007/008).
- **Listen vs. Laden:** Listen liest nur Manifeste und Datei-Metadaten (Vorhandensein, Größe), nie Dateiinhalte. Die SHA-256-Prüfung erfolgt erst beim Laden in den Zwischenspeicher (TEAM-004).

---

## A. Stufe 1 – Team-Ordner (frei)

### TEAM-001 Team einrichten
- **Priorität:** P1
- **Beschreibung:** In den Einstellungen legt der Nutzer ein Team an: Name und Team-Ordner. Mehrere Teams sind möglich (z. B. je Agentur oder Kunde). Beim ersten Einrichten wird im Ordner eine Team-Kennung `team.json` angelegt: Team-UUID, Name, Formatversion und das eigene Feld `min_version` (Team-Mindestversion von OpenSpider nach TEAM-006, leer = keine). `team.json` ist **additiv** versioniert: Neue Schlüssel dürfen in späteren Versionen hinzukommen, unbekannte Schlüssel werden beim Lesen ignoriert und beim Schreiben unverändert erhalten. Findet die App eine vorhandene `team.json`, tritt sie diesem Team bei. Ein Team lässt sich lokal entfernen, ohne den Ordner zu verändern.
- **Gleichzeitige Erstanlage:** Legen zwei Installationen gleichzeitig ein Team im selben Ordner an, entstehen durch den Sync-Dienst zwei Dateien mit verschiedener UUID (eine als Konfliktkopie). Die App erkennt das generisch (jede JSON-Datei im Ordner-Wurzelverzeichnis mit `team.json`-Schema, die nicht `team.json` heißt), wählt deterministisch die gültige Datei mit der kleinsten UUID (Textvergleich) und meldet den Konflikt einmal mit Hinweis, die übrige Datei zu löschen. Die Entscheidung ist auf allen Installationen dieselbe.
- **Änderung von `min_version`:** Stufe 1 hat keine Rechteverwaltung. Vorläufig darf jede Installation mit Schreibrecht im Ordner `min_version` ändern, nach ausdrücklicher Bestätigung; wer das darf, ist offene Frage (f).
- **Akzeptanzkriterien:**
  - [ ] Leerer Ordner → `team.json` wird angelegt; zweite Installation mit demselben Ordner zeigt dasselbe Team (gleiche UUID).
  - [ ] `team.json` mit zusätzlichem unbekanntem Schlüssel → Team wird normal genutzt; nach einer Änderung von `min_version` steht der unbekannte Schlüssel unverändert in der Datei.
  - [ ] Zwei `team.json` mit verschiedener UUID im Ordner (eine als Konfliktkopie benannt) → beide Installationen wählen dieselbe (kleinste UUID) und melden den Konflikt.
  - [ ] Ordner ohne Schreibrecht → Team nur lesend nutzbar, Hinweis „Veröffentlichen nicht möglich“.
  - [ ] Ordner nicht erreichbar (Netzlaufwerk getrennt) → Team als „nicht verbunden“ markiert, lokale Kopien (TEAM-004) bleiben nutzbar, keine Fehlermeldungsschleife.
  - [ ] `min_version` setzen verlangt eine Bestätigung; ohne Schreibrecht ist die Änderung nicht möglich.
- **Testfixture:** temporäre Verzeichnisse für leer, vorhanden (auch mit unbekanntem Schlüssel), Doppelanlage, nur lesend, nicht vorhanden.

### TEAM-002 Crawl veröffentlichen
- **Priorität:** P1
- **Beschreibung:** Aus der Crawl-Verwaltung oder dem geöffneten Crawl heraus wird ein Crawl im Team veröffentlicht. Die App erzeugt lokal einen konsistenten Schnappschuss, bereinigt ihn nach TEAM-003 und schreibt ihn mit Manifest nach dem Ablauf aus TEAM-008 in den Team-Ordner. Umfang wählbar: **vollständig** (ganze Crawl-Datei nach Bereinigung, mit gespeichertem HTML; Warnung nach TEAM-003) oder **nur Ergebnisse** (alle Tabellen, Befunde, Berichte und Exporte, aber ohne gespeichertes HTML, Quelltexte, Screenshots und gerenderte DOMs).
- **Manifest:** Team-UUID, Crawl-ID, Versionsschlüssel, Start-URL/Domain, Projekt, Crawl-Zeitraum, Zustand und Endgrund, URL-Anzahl, Umfang, `openspider_version`, Schema-Version, Datei (Name, Größe, SHA-256), `snapshot_of` (leer bei abgeschlossenem Crawl), Autor (frei wählbarer Anzeigename plus Installations-ID, keine Hardware- oder Systemkennung), Veröffentlichungszeitpunkt.
- **Schnappschuss eines laufenden Crawls:** Der Zustand wird eingefroren: Die Kopie trägt `snapshot_of` (Crawl-ID, Zustand zum Zeitpunkt, Stand wie URL-Zähler und letzter dauerhafter Commit, Zeitpunkt). Sie gilt **nicht** als „unterbrochen“ und lässt sich nicht fortsetzen; die Ansicht zeigt „Zwischenstand eines laufenden Crawls vom <Zeitpunkt>“. Tabellen, die erst am Crawl-Ende aufgebaut werden, werden in der Kopie aufgebaut, insbesondere `links_target`; das Original bleibt unverändert.
- **„Nur Ergebnisse“ – Inhaltsvoraussetzung:** Alle `q_`-Sichten (OUT-234) und alle Exporte ohne Inhaltsvoraussetzung liefern dieselben Werte wie das Original. Ansichten, die gespeichertes HTML brauchen (N-Gramme, Wortwolke, Re-Analyse der Wortzahl, Inhaltsvergleich), zeigen den bestehenden Hinweis aus UI-016 statt Werten oder Fehlern.
- **Fortschritt und Abbruch:** Die App zeigt Fortschritt je Phase (Schnappschuss, Bereinigung, Prüfsumme, Kopie) und erlaubt Abbruch in jeder Phase; ein Abbruch entfernt die Zwischendateien und lässt keine Teilversion im Ordner zurück. Die Veröffentlichung läuft im Hintergrund und blockiert die Bedienung nicht.
- **Kollision mit einem laufenden Export derselben Crawl-Datei:** Lesende Vorgänge (Export, Berichte) und das Erzeugen des Schnappschusses laufen parallel, ohne einander zu sperren. Ein schreibender Vorgang auf derselben Datei (z. B. Re-Analyse) lässt die Veröffentlichung warten oder liefert „Crawl-Datei ist belegt“; sie hängt nie unbegrenzt.
- **Akzeptanzkriterien:**
  - [ ] Veröffentlichen eines laufenden Crawls stört den Crawl nicht (keine Sperrfreigabe, kein Datenverlust; Prüfung wie CRAWL-125) und liefert eine Datei mit `PRAGMA integrity_check = ok`.
  - [ ] Schnappschuss eines laufenden Crawls → Kopie trägt `snapshot_of` (Stand, Zeitpunkt), ist nicht als „unterbrochen“ gekennzeichnet, nicht fortsetzbar, und `links_target` ist in der Kopie vollständig aufgebaut.
  - [ ] „Nur Ergebnisse“ für einen Crawl mit gespeichertem HTML: Zeilenzahl in `page_html`, `screenshots` und `render_pages` ist 0; Dateigröße ≤ Original minus diese Tabellen + 5 %.
  - [ ] „Nur Ergebnisse“: alle `q_`-Sichten (OUT-234) und Exporte ohne Inhaltsvoraussetzung liefern dieselben Werte wie das Original (Vergleich zeilenweise); N-Gramme, Wortwolke, Re-Analyse Wortzahl und Inhaltsvergleich zeigen den Hinweis aus UI-016.
  - [ ] Zweimaliges Veröffentlichen desselben Crawls ergibt zwei Versionsschlüssel mit lokal berechneten Nummern 1 und 2; Version 1 bleibt byte-gleich.
  - [ ] Abbruch durch den Nutzer in jeder Phase sowie Beenden des Prozesses oder Trennen des Ordners hinterlässt keine als gültig gelistete Teilversion (TEAM-008).
  - [ ] Fortschritt wird je Phase angezeigt; nach Abbruch liegen keine Dateien der Installation in `.staging`.
  - [ ] Veröffentlichen während eines lesenden Exports derselben Crawl-Datei → beide Vorgänge enden erfolgreich; bei einem schreibenden Vorgang kommt „Crawl-Datei ist belegt“ oder ein Warten mit Abbruchmöglichkeit.
- **Testfixture:** `out-basic-site` mit gespeichertem HTML; laufender Fixture-Crawl; Fixture mit gleichzeitigem Export.

### TEAM-003 Keine Geheimnisse im Schnappschuss
- **Priorität:** P0 (innerhalb dieses Kapitels)
- **Beschreibung:** Eine Crawl-Datei enthält heute an mehreren Stellen Geheimnisse. Deshalb wird nie die Original-Datei, sondern ausschließlich die **lokale Kopie** bereinigt, bevor sie in den Team-Ordner kopiert wird (TEAM-008). Betroffene Stellen und Regel:
  - Eigene Request-Header als Klartext in der eingebetteten Konfiguration (`HeaderEntry.value`) → Werte werden durch `<redacted>` ersetzt, Namen bleiben.
  - `auth.basic[].username` in der eingebetteten Konfiguration → wird entfernt (Passwörter liegen ohnehin im SecretStore, nicht in der Datei). Übrig bleibt höchstens der Hinweis, dass eine Authentifizierung genutzt wurde.
  - Tabelle `http_headers` (heute werden nur bekannte sensible Namen geschwärzt) → wird auf eine **Allowlist** von Header-Namen reduziert; alles andere wird entfernt. Startwerte der Allowlist: `Content-Type`, `Content-Length`, `Content-Encoding`, `Content-Language`, `Cache-Control`, `Expires`, `Age`, `Date`, `Last-Modified`, `ETag`, `Vary`, `Server`, `Location`, `Link`, `X-Robots-Tag`, `Retry-After`. Erweiterungen nur ausdrücklich in der Spec.
  - Tabelle `cookies` (volle Werte) → wird geleert.
  - `user:pass@host` in URLs (`urls`, `links`, `discovery_edges`) → die Veröffentlichung wird **verweigert** mit Hinweis auf die betroffene Tabelle und die Anzahl, nicht auf den Wert. Es wird nicht stillschweigend umgeschrieben, weil URLs Schlüssel der Datei sind.
  - Zstd-komprimiertes HTML (`page_html`), Binärdaten, Render-Tabellen mit URLs inklusive Query-Tokens und `crawl_events.message` können Tokens enthalten, die sich nicht automatisch erkennen lassen. Bekannte Werte (siehe Nachweis) werden gesucht; darüber hinaus gilt die Warnung unten.
  - Proxy- und Formular-Login-Daten liegen nicht in der Crawl-Datei; sie bleiben im SecretStore und werden nie kopiert.
- **Umfang „vollständig“ mit gespeichertem HTML:** Veröffentlichen ist nur nach ausdrücklicher Bestätigung der Warnung möglich: „Gespeichertes HTML kann Sitzungsdaten, urheberrechtlich geschützte Inhalte oder personenbezogene Daten enthalten.“ Ob dieser Umfang (und Screenshots) im Team überhaupt geteilt werden darf, ist offene Frage (a).
- **Nachweis (Abschlussprüfung vor dem Kopieren):** Alle bekannten Geheimniswerte (SecretStore, Header-Werte und Basic-Daten der Konfiguration, Cookie-Werte aus der Original-Datei) werden in der bereinigten Kopie gesucht: im Klartext sowie in Base64- und Prozent-kodierten Varianten, per SQL über alle Textspalten aller Tabellen und über die dekomprimierten Inhalte (zstd-HTML, Blobs). Das ersetzt die Byte-Suche über die Datei, die bei komprimierten Inhalten nichts findet. Bei Fund wird nicht veröffentlicht; die Meldung nennt Tabelle und Spalte, nicht den Wert.
- **Akzeptanzkriterien:**
  - [ ] Crawl mit eigenem Request-Header (z. B. `Authorization`/Token), Basic-Auth, Cookies, Proxy mit Passwort und Formular-Login → in der veröffentlichten Kopie ist `cookies` leer, `http_headers` enthält nur Allowlist-Namen, Header-Werte der Konfiguration sind `<redacted>`, `auth.basic[].username` fehlt.
  - [ ] Prüfung nach dem Kopieren: Suche nach jedem Testgeheimnis im Klartext, in Base64 und Prozent-kodiert über alle Textspalten aller Tabellen und alle dekomprimierten Inhalte ergibt 0 Treffer.
  - [ ] Geheimnis in zstd-komprimiertem HTML (nur im Umfang „vollständig“) → Fund über die dekomprimierte Suche, Veröffentlichung verweigert, Meldung nennt `page_html`, nicht den Wert.
  - [ ] Künstlich eingeschleuster Geheimniswert in `crawl_events.message` oder in der Query einer Render-URL → Veröffentlichung verweigert, Meldung nennt die Fundstelle (Tabelle/Spalte), nicht den Wert.
  - [ ] URL mit `user:pass@host` in `urls`, `links` oder `discovery_edges` → Veröffentlichung verweigert mit Hinweis; es wird nichts geschrieben.
  - [ ] Umfang „vollständig“ mit gespeichertem HTML → Veröffentlichen erst nach Bestätigung der Warnung; ohne Bestätigung wird nichts geschrieben. CLI ohne Bestätigungsmöglichkeit verlangt ein ausdrückliches Flag (TEAM-010).
  - [ ] Die Original-Crawl-Datei bleibt byte-gleich (Prüfsumme vor/nach).
- **Testfixture:** Crawl-Fixture mit Testzugangsdaten aus der Projekt-Fixture (Basic-Auth, eigene Header, Cookies, Token in HTML und Query, Userinfo-URL als eigene Variante).

### TEAM-004 Team-Bibliothek
- **Priorität:** P1
- **Beschreibung:** Die Crawl-Verwaltung zeigt je Team einen Bereich „Team“ mit allen veröffentlichten Crawls, gruppiert nach Projekt und Domain, mit den Manifest-Feldern als Spalten, filter- und sortierbar. Die Liste wird **ausschließlich aus Manifesten** (und Datei-Metadaten) aufgebaut, ohne eine Crawl-Datei zu öffnen. Öffnen lädt die Version in einen lokalen Zwischenspeicher (Größenlimit einstellbar, älteste nicht geöffnete zuerst verdrängt), prüft dabei die SHA-256 gegen das Manifest und öffnet sie **nur lesend** (Sperre wie OUT-228). Schlägt die Prüfung fehl, gilt die Version als „unvollständig synchronisiert“ (wird bei Ordneränderungen erneut geprüft) bzw. bei bleibender Abweichung als „beschädigt/verändert“ (TEAM-008); sie wird nicht geöffnet. Bearbeiten erzeugt auf Wunsch eine eigene lokale Kopie. Die Liste aktualisiert sich bei Änderungen im Ordner ohne Neustart.
- **Zwischenspeicher voll:** Eine gerade geöffnete Version wird nie verdrängt. Passt eine weitere Version nur durch Verdrängen einer geöffneten, erscheint die Meldung „Zwischenspeicher voll – eine Version schließen oder das Limit erhöhen“; geöffnet wird erst danach.
- **„Als eigene Kopie öffnen“:** erzeugt eine unabhängige lokale Datei mit **neuer Crawl-ID**, wie das Duplizieren (OUT-228); Team-UUID und Versionsschlüssel der Quelle werden nicht übernommen.
- **Akzeptanzkriterien:**
  - [ ] Eine von Installation A veröffentlichte Version erscheint bei Installation B nach Eintreffen der Ordneränderung ohne Neustart und lässt sich dort öffnen; alle Tabellen, Filter und Berichte funktionieren lesend.
  - [ ] Liste mit 1 000 Versionen: Öffnen der Liste < 500 ms, Sortieren < 200 ms; Zugriffsprotokoll des Tests zeigt Lesezugriffe nur auf Manifeste, keinen auf Crawl-Dateien.
  - [ ] Beim Öffnen wird die SHA-256 geprüft; falsche Prüfsumme → nicht geöffnet, Zustand „unvollständig synchronisiert“; bleibt die Abweichung nach erneuter Prüfung bestehen (Größe und Änderungszeit unverändert über mindestens 10 Minuten), Zustand „beschädigt/verändert“.
  - [ ] Versuch, eine Team-Version zu verändern (Crawl fortsetzen, URLs hinzufügen, Konfiguration ändern) → nicht angeboten.
  - [ ] „Als eigene Kopie öffnen“ erzeugt eine unabhängige lokale Datei mit neuer Crawl-ID; Änderungen daran berühren Team-Ordner und Zwischenspeicher nicht.
  - [ ] Zwischenspeicher voll: Öffnen einer weiteren Version verdrängt nur nicht geöffnete Versionen (älteste zuerst); sind nur geöffnete übrig, erscheint die Meldung und keine geöffnete Version wird geschlossen oder gelöscht.
- **Testfixture:** Team-Ordner mit Manifesten mehrerer Domains und Versionen (auch 1 000 Stück, synthetisch), Ordner mit Teildatei und mit verändertem Byte.

### TEAM-005 Verlauf und Vergleich je Domain
- **Priorität:** P2 (für den ersten Schnitt; Rev. 1: P1)
- **Beschreibung:** Für jede Domain zeigt die Bibliothek den Verlauf aller veröffentlichten Versionen (Datum, URL-Anzahl, Anteil Fehlerseiten, Indexierbarkeit). Zwei Versionen derselben Domain lassen sich direkt mit dem Bericht „Änderungen seit dem letzten Crawl“ vergleichen; beim Eintreffen einer neuen Version bietet die App diesen Vergleich gegen die vorige an. Die Reihenfolge folgt dem Crawl-Zeitraum, nicht den Uhren der Veröffentlichenden. Ein **Konflikt** liegt vor, wenn zwei Versionen derselben Domain einen überlappenden Crawl-Zeitraum haben; beide werden gelistet und gekennzeichnet, der Vergleich bleibt möglich.
- **Akzeptanzkriterien:**
  - [ ] Zwei Versionen einer Domain → Vergleichsbericht mit denselben Zahlen wie beim lokalen Vergleich derselben Dateien.
  - [ ] Verlauf zeigt Versionen verschiedener Teammitglieder chronologisch nach Crawl-Zeitraum, auch wenn Uhren der Rechner abweichen (Veröffentlichungszeitpunkte in falscher Reihenfolge ändern die Reihenfolge nicht).
  - [ ] Zwei Versionen derselben Domain mit überlappendem Crawl-Zeitraum → beide mit „Konflikt“ markiert; zwei Versionen mit getrennten Zeiträumen → nicht markiert.
- **Testfixture:** zwei Fixture-Crawls derselben Site mit bekannten Unterschieden; Manifest-Variante mit überlappendem Zeitraum und verstellter Uhr.

### TEAM-006 Versionen der App im Team
- **Priorität:** P1
- **Beschreibung:** Jede Version trägt Schema-Version und `openspider_version` (Manifest). Eine App, die das Schema nicht lesen kann, listet die Version mit dem Hinweis „veröffentlicht mit OpenSpider <openspider_version aus Manifest> – bitte aktualisieren“ statt eines Fehlers; eine daraus abgeleitete Mindestversion wird nicht berechnet. Die **Team-Mindestversion** ist ein eigenes Feld `min_version` in `team.json` (TEAM-001): Apps darunter können nicht veröffentlichen und zeigen einen Hinweis zum Update, Lesen bleibt möglich. Ältere Versionen (Schema älter als die der App) öffnet die App über eine **migrierte Kopie** im Zwischenspeicher (`migrate_copy`, Migration nach OUT-255), nie durch nur-lesendes Öffnen des Originals; die Datei im Team-Ordner bleibt unberührt.
- **Akzeptanzkriterien:**
  - [ ] Version mit höherem Schema → in älterer App gelistet, nicht geöffnet, Hinweis „veröffentlicht mit OpenSpider <Version aus Manifest> – bitte aktualisieren“.
  - [ ] Version mit älterem Schema → in neuerer App über `migrate_copy` geöffnet; die Datei im Team-Ordner ist danach byte-gleich (Prüfsumme), der Zwischenspeicher enthält die migrierte Kopie.
  - [ ] Scheitert die Migration → Hinweis, nichts im Team-Ordner verändert.
  - [ ] `min_version` in `team.json` größer als die App-Version → Veröffentlichen gesperrt mit Update-Hinweis, Lesen weiter möglich.
- **Testfixture:** Fixture-Archiv aus OUT-255; Manifest mit höherem Schema.

### TEAM-007 Zurückziehen und Löschen
- **Priorität:** P1 für **Zurückziehen**, P2 für **endgültiges Löschen** und automatisches Leeren des Papierkorbs (erster Schnitt: nur Zurückziehen)
- **Beschreibung:** Wer veröffentlicht hat, kann eine Version zurückziehen: Manifest und Datei werden in `.papierkorb/` des Teams verschoben (zuerst das Manifest, damit die Version sofort aus der Liste verschwindet; Wiederherstellen in umgekehrter Reihenfolge). Stufe 1 hat keine Rechteverwaltung: Das Ordnerrecht entscheidet; die App bietet Zurückziehen nur dem veröffentlichenden Anzeigenamen (Installations-ID) an und warnt bei fremden Versionen. Bei allen anderen Installationen verschwindet die Version nach Eintreffen der Ordneränderung aus der Liste. Liegt sie dort bereits im Zwischenspeicher, bleibt sie lesbar und wird als „zurückgezogen“ markiert (TEAM-008). **Aufbewahrung im Papierkorb:** Vorschlag 30 Tage, danach darf das endgültige Löschen (P2) durch jede Installation mit Schreibrecht erfolgen; bis dahin leert der erste Schnitt den Papierkorb nie automatisch. Die Frist ist offene Frage (e). **Endgültiges Löschen** (P2): nach Bestätigung mit Versionsangabe, ohne Wiederherstellung.
- **Akzeptanzkriterien:**
  - [ ] Zurückgezogene Version verschwindet nach Eintreffen der Ordneränderung bei allen Installationen aus der Liste und lässt sich aus dem Papierkorb wiederherstellen (Liste zeigt sie danach wieder).
  - [ ] Im Zwischenspeicher bereits geöffnete zurückgezogene Version bleibt lesbar und ist als „zurückgezogen“ markiert.
  - [ ] Fehlendes Ordnerrecht (Papierkorb oder Crawl-Ordner nicht beschreibbar) → Meldung „Zurückziehen nicht möglich: keine Schreibrechte im Team-Ordner“, nichts verschoben, Version bleibt gelistet.
  - [ ] Zurückziehen einer fremden Version → Warnung vor der Ausführung.
  - [ ] [P2] Endgültiges Löschen verlangt eine ausdrückliche Bestätigung mit Versionsangabe; Einträge im Papierkorb jünger als die Aufbewahrungsfrist werden nicht gelöscht.
- **Testfixture:** Team-Ordner mit mehreren Versionen, nur-lesender Ordner.

### TEAM-008 Sicher mit Sync-Diensten
- **Priorität:** P0 (innerhalb dieses Kapitels)
- **Beschreibung:** Im Team-Ordner liegt nie eine geöffnete Datenbank und nie eine Teildatei in einem gelisteten Pfad. Veröffentlichen läuft in dieser Reihenfolge:
  1. Schnappschuss **lokal** bauen und nach TEAM-003 bereinigen (außerhalb des Team-Ordners).
  2. SHA-256 über die fertige lokale Datei bilden.
  3. **Byte-Kopie** nach `<team>/.staging/<installations-id>/…` (gleiches Volume wie das Ziel).
  4. `fsync` der Datei (und, wo das Betriebssystem es bietet, des Verzeichnisses).
  5. **Umbenennen** in den Zielordner `crawls/<crawl-id>/` ohne Überschreiben (existiert der Name, wird der Zeitstempel erhöht; vorhandene Dateien werden nie ersetzt).
  6. Manifest auf demselben Weg (Staging, `fsync`, Umbenennen) **zuletzt** schreiben. Es nennt Dateiname, Größe und SHA-256 der Crawl-Datei.
- **Verhalten bei anderen Installationen:** Der Sync-Dienst kann das Manifest vor der Datei liefern. Eine Version wird erst **gelistet**, wenn das Manifest gültig ist, die Datei vorhanden ist und die Größe passt. Fehlt die Datei noch, bleibt die Version unsichtbar und wird bei der nächsten Ordneränderung erneut geprüft (kein Fehler, keine Meldungsschleife). Die SHA-256 wird erst beim Laden in den Zwischenspeicher geprüft (TEAM-004): Abweichung → „unvollständig synchronisiert“, bleibt sie bestehen → „beschädigt/verändert“. Gelesen wird ausschließlich aus dem lokalen Zwischenspeicher, nie direkt aus dem Ordner.
- **Windows:** Beim Umbenennen und Öffnen wird bei Sharing-Violation/Lock-Violation (Sync-Client, Virenscanner) mit Backoff wiederholt (Vorschlag: 100 ms, verdoppelt bis 5 s, insgesamt höchstens 30 s); danach Fehlermeldung, Aufräumen der Zwischendateien.
- **Dateinamen:** nur UUID, Zeitstempel und Installations-ID, keine Nutzertexte (keine Domain, kein Projekt, kein Anzeigename), damit Pfadlängen, Sonderzeichen und Groß-/Kleinschreibung der Sync-Dienste keine Rolle spielen.
- **Platzhalterdateien:** OneDrive Files On-Demand, Google Drive Streaming und Dropbox File Provider zeigen Dateien, deren Inhalt noch nicht lokal liegt. **Listen liest nur Manifeste und Metadaten**; der Inhalt der Crawl-Datei wird erst beim Öffnen angefordert (mit Fortschrittsanzeige und Abbruch), nie beim Listen.
- **Konflikte generisch erkennen:** Eine Datei ohne passendes Manifest (oder ein Manifest, das nicht zum Namensschema passt oder auf eine bereits vergebene Datei zeigt) gilt als **Konflikt/unbekannte Datei**, wird markiert, nicht automatisch geöffnet und nie als Version übernommen. Die Erkennung hängt nicht von Namensmustern bestimmter Dienste („(1)“, „conflicted copy“) ab; solche Muster dienen höchstens als zusätzlicher Hinweis.
- **Weitere Fehlerfälle:**
  - *Volume voll:* freier Platz wird vor dem Kopieren geprüft; schlägt das Schreiben dennoch fehl → Meldung „Team-Ordner voll“, Zwischendateien entfernt, nichts gelistet.
  - *Kontingent des Sync-Dienstes erschöpft:* Der Dienst meldet das oft erst nach dem Schreiben. Die App erkennt, was das Dateisystem meldet (Schreibfehler → Meldung wie oben); bleibt die Version bei anderen aus, steht in der Veröffentlichen-Meldung der Hinweis, den Status im Sync-Dienst zu prüfen.
  - *Nachträglich veränderte Datei:* Weicht Größe oder SHA-256 einer bereits gelisteten Version vom Manifest ab, wird sie als „beschädigt/verändert“ markiert und nicht geöffnet; eine bereits im Zwischenspeicher liegende, geprüfte Kopie bleibt lesbar.
  - *Gelöschte Version, die noch im Zwischenspeicher liegt:* bleibt lesbar und wird als „zurückgezogen“ markiert (nicht stillschweigend entfernt).
  - *Verwaiste Zwischendateien:* eigene Dateien in `.staging/<installations-id>/` älter als 24 Stunden werden beim nächsten Start entfernt; `.staging` wird nie gelistet.
- **Akzeptanzkriterien:**
  - [ ] Unvollständige Datei (Größe ≠ Manifest), Manifest ohne Datei oder Datei ohne Manifest → nicht als Version gelistet, Eintrag im Log; Datei ohne Manifest erscheint als „Konflikt/unbekannte Datei“.
  - [ ] Manifest kommt vor der Datei an → Version unsichtbar; sobald die Datei in voller Größe da ist, wird sie ohne Neustart gelistet.
  - [ ] Falsche SHA-256 beim Laden → „unvollständig synchronisiert“; bleibende Abweichung → „beschädigt/verändert“; in beiden Fällen nicht geöffnet.
  - [ ] Zwei Installationen veröffentlichen gleichzeitig denselben Crawl → zwei getrennte, gültige Versionsschlüssel, keine beschädigte oder überschriebene Datei.
  - [ ] Konfliktkopie im Ordner (beliebiger Name, ohne Manifest) → als Konflikt markiert, nicht automatisch geöffnet.
  - [ ] Abbruch (Prozess beendet) zwischen den Schritten 3 bis 6 → keine gelistete Teilversion; verwaiste Zwischendateien werden nach 24 Stunden entfernt.
  - [ ] Windows: Sharing-Violation beim Umbenennen (simuliert durch gehaltenen Handle) wird mit Backoff wiederholt und gelingt nach Freigabe; bei dauerhafter Sperre kommt nach höchstens 30 s eine Meldung.
  - [ ] Platzhalterdatei (Inhalt nicht lokal): Liste öffnet ohne Zugriff auf den Inhalt (Zugriffsprotokoll leer für Dateiinhalte), Inhalt wird erst beim Öffnen gelesen.
  - [ ] Volume voll (simuliert) → Meldung, `.staging` leer, keine gelistete Version.
  - [ ] Nachträglich verändertes Byte in einer gelisteten Version → „beschädigt/verändert“; zuvor geprüfte Zwischenspeicher-Kopie bleibt lesbar.
  - [ ] Dateien im Team-Ordner heißen ausschließlich nach UUIDs und Zeitstempel (Test mit Domain und Anzeigename voller Sonderzeichen).
- **Testfixture:** simulierter Sync-Ordner mit steuerbaren Zuständen (Teildatei, verspätetes Manifest, Konfliktkopie, Platzhalter, voller Datenträger, nachträgliche Änderung, gehaltener Handle unter Windows).

### TEAM-009 Geteilte Konfigurationen
- **Priorität:** P2
- **Beschreibung:** Crawl-Konfigurationen, Presets, Custom-Extraction-Regeln, Filter und Berichtsvorlagen lassen sich im Team teilen und von dort übernehmen; dabei gilt TEAM-003 (keine Geheimnisse; Header-Werte `<redacted>`, kein Basic-Benutzername). Übernommene Konfigurationen sind lokale Kopien.
- **Akzeptanzkriterien:**
  - [ ] Geteiltes Preset mit Login-Konfiguration → beim Übernehmen fehlen die Zugangsdaten, die App fordert sie lokal an.

### TEAM-010 CLI, MCP und geplante Crawls
- **Priorität:** P1
- **Beschreibung:** CLI: `--publish-team <team>` nach einem Crawl (mit `--publish-scope full|results`), `--list-team <team>` als JSON. Für den Umfang `full` mit gespeichertem HTML verlangt die CLI das ausdrückliche Flag `--accept-publish-warning` (Entsprechung der Warnung aus TEAM-003). Geplante Crawls (Aufgaben) können nach Abschluss automatisch veröffentlichen. MCP: Die Team-Bibliothek ist lesend abfragbar (Liste, Verlauf, Vergleich), Veröffentlichen nur mit der Freigabeklasse `write` (AGENT-003).
- **Team-Ordner nicht erreichbar:** Beim geplanten Lauf (oder per CLI) bleibt das Crawl-Ergebnis lokal gültig und unverändert. Die Veröffentlichung wird als **eigener Fehler** gemeldet (Aufgabenprotokoll, JSON-Feld `publish.error`, Hinweis in der Crawl-Verwaltung „Veröffentlichung ausstehend“ mit Aktion „Erneut veröffentlichen“). Der **Exit-Code des Crawls bleibt unverändert**; die Veröffentlichung ändert ihn nicht.
- **Akzeptanzkriterien:**
  - [ ] Geplanter Crawl mit „im Team veröffentlichen“ → nach Abschluss neue Version im Team-Ordner, auch ohne geöffnete App.
  - [ ] Team-Ordner beim geplanten Lauf nicht erreichbar → Crawl erfolgreich abgeschlossen und lokal geöffnet, Exit-Code wie ohne Veröffentlichung, Veröffentlichungsfehler separat protokolliert, „Veröffentlichung ausstehend“ sichtbar; „Erneut veröffentlichen“ gelingt nach Wiederkehr des Ordners.
  - [ ] `--publish-scope full` mit gespeichertem HTML ohne `--accept-publish-warning` → nichts veröffentlicht, Fehlermeldung mit Hinweis.
  - [ ] MCP ohne `write` → Veröffentlichen-Werkzeug nicht gelistet.
- **Testfixture:** geplanter Fixture-Crawl mit erreichbarem und nicht erreichbarem Team-Ordner.

### TEAM-011 Befund-Board (Idee)
- **Priorität:** P2 (Idee, Nutzer 09.11.2026: „vielleicht erstmal zu weit gedacht“)
- **Beschreibung:** Ein gemeinsames Board je Team bzw. Domain, auf dem Befunde aus veröffentlichten Crawls gesammelt und besprochen werden (Befund übernehmen, kommentieren, Status offen/erledigt/ignoriert, zuweisen), direkt in OpenSpider statt in fremden Tools. In Stufe 1 als Dateien im Team-Ordner (je Eintrag eine Datei, nur anhängen, keine gemeinsame Datenbank; TEAM-008 gilt), in Stufe 2 als TEAM-052.

---

## B. Stufe 2 – OpenSpider Teams (bezahlt, evnxt-Account)

Grundsatz auch hier (Nutzer 09.11.2026): **OpenSpider crawlt nie selbst für Kunden.** Stufe 2 hostet nur die Ablage und die Zusammenarbeit; gecrawlt wird immer auf den Rechnern der Nutzer.

Gemeinsame Regeln: Die Ablage ersetzt den Team-Ordner, die Regeln aus Abschnitt A (Schnappschüsse, Versionsschlüssel, TEAM-003, TEAM-006, TEAM-008 soweit auf Ordner bezogen) gelten unverändert. Daten liegen in der EU, verschlüsselt bei der Übertragung und im Speicher; Auftragsverarbeitungsvertrag für Kunden. OpenSpider funktioniert ohne Anmeldung vollständig weiter. Mischbetrieb Ordner plus Ablage und Abrechnung sind offene Frage (c), die rechtliche Prüfung offene Frage (d).

### TEAM-050 Anmeldung, Teams und Rechte
- **Priorität:** P2
- **Beschreibung:** Anmeldung mit dem evnxt-Account; Teams mit Rollen **Besitzer**, **Bearbeiter** (veröffentlichen, kommentieren), **Leser** (öffnen, kommentieren) je Team und optional je Projekt; Einladen per E-Mail; Entfernen eines Mitglieds entzieht sofort den Zugriff auf neue Downloads. Mit Rollen wird auch die Frage nach `min_version` (f) und dem Zurückziehen fremder Versionen entschieden.

### TEAM-051 Gehostete Ablage und Kontingente
- **Priorität:** P2
- **Beschreibung:** Veröffentlichen lädt in die Ablage hoch (fortsetzbarer Upload, Transportkompression; das Dateiformat der Version bleibt unverändert); Kontingente je Team für Speicher und Aufbewahrungsdauer; Hinweis vor Erreichen der Grenze; Export aller Team-Daten jederzeit.

### TEAM-052 Kommentare und Aufgaben
- **Priorität:** P2
- **Beschreibung:** Kommentare und Aufgaben (offen, erledigt, ignoriert, zugewiesen) je URL, Befund oder Bericht einer Version, als eigene Schicht neben dem Schnappschuss; sie wandern bei einer neuen Version derselben Domain mit, solange die URL bzw. der Befund noch existiert.

### TEAM-053 Alarme
- **Priorität:** P2
- **Beschreibung:** Regeln je Domain (z. B. neue 4xx/5xx-Seiten, neue Noindex-Seiten, verlorene Canonicals, Rückgang der indexierbaren Seiten um mehr als x %); beim Eintreffen einer neuen Version werden sie gegen die vorige ausgewertet; Benachrichtigung per E-Mail oder Webhook (z. B. Slack). **Wird mit R6 (Monitoring in evnxt) zusammengeführt**, nicht doppelt gebaut.

### TEAM-054 Kundenansicht
- **Priorität:** P2
- **Beschreibung:** Nur lesender Link oder PDF-Bericht aus einer Version für Externe, mit Ablaufdatum und widerrufbar; zeigt nur die freigegebenen Berichte.

### TEAM-055 Protokoll
- **Priorität:** P2
- **Beschreibung:** Unveränderliches Protokoll je Team: wer hat wann veröffentlicht, zurückgezogen, gelöscht, eingeladen, Rechte geändert.

---

## C. Paketschnitt Stufe 1

Vorschlag der Prüfung (Fable, 09.11.2026). Reihenfolge nach Abhängigkeit; jedes Paket bekommt einen **unabhängigen Testagenten**, der nur aus dieser Spec testet. Neue Crates (`os-team`) nur nach Absprache mit dem Orchestrator (`CLAUDE.md`).

| Paket | Inhalt | Zuständigkeit | Priorität |
|---|---|---|---|
| TM-01 Kern-Schnappschuss | TEAM-002 (Schnappschuss, Umfänge, Manifest-Inhalte), TEAM-003 (Bereinigung, Nachweis) | `os-store`, Modul `team::snapshot`; Manifest-Typen in `os-model` | P0/P1 |
| TM-02 Team-Ordner | TEAM-001, TEAM-006, TEAM-008, TEAM-007 (nur Zurückziehen) | `os-store::team` oder neues Crate `os-team` (nach Absprache) | P0/P1 |
| TM-03 CLI/MCP/Aufgaben | TEAM-010 | CLI-, MCP- und Aufgaben-Crates | P1 |
| TM-04 App | UI zu TEAM-001, TEAM-004 (Bibliothek, Zwischenspeicher, „Als eigene Kopie öffnen“), Veröffentlichen-Dialog mit Warnung, Fortschritt, Abbruch | `apps/desktop` | P1 |
| TM-05 Verlauf (optional) | TEAM-005 | Berichte/UI | P2 |

Abhängigkeiten: TM-01 vor TM-02 (Manifest), TM-02 vor TM-03 und TM-04; TM-05 setzt TM-04 voraus. Nicht im ersten Schnitt: endgültiges Löschen und Papierkorb-Leeren (TEAM-007, P2), TEAM-009, TEAM-011.

**Testfixtures für die Pakete:**
- *Simulierter Sync-Ordner* mit steuerbaren Zuständen: Teildatei, verspätetes Manifest, Konfliktkopie (auch von `team.json`), Platzhalterdatei, voller Datenträger, nachträglich veränderte Datei, gehaltener Handle (Windows).
- *Crawls mit Auth, Headern und Cookies:* Basic-Auth, eigene Request-Header, Cookies, Token in HTML und Query, Userinfo-URL, Formular-Login und Proxy (SecretStore) mit bekannten Testwerten.
- *Bibliothek:* synthetischer Ordner mit 1 000 Manifesten.

---

## Entschieden (Empfehlung, Rev. 2)

1. **Autorangabe:** Anzeigename plus Installations-UUID im Manifest, keine Signatur. Stufe 1 setzt auf Vertrauen im Team; Signaturen kommen frühestens mit den Rollen in Stufe 2.
2. **Keine eigene Kompression** der Crawl-Datei in Stufe 1; Größe wird über den Umfang „nur Ergebnisse“ und den Sync-Dienst gelöst.
3. **Keine Teilübertragung** großer Versionen in Stufe 1 (eine Datei, ein Manifest); der fortsetzbare Upload gehört zur gehosteten Ablage (TEAM-051).
4. **TEAM-053 (Alarme) wird mit R6 (Monitoring in evnxt)** zusammengeführt.
5. **Umfang „nur Ergebnisse“:** welche Ansichten gespeicherte Inhalte brauchen, ist in TEAM-002 geklärt (Hinweis nach UI-016, keine Vorberechnung).

## Offene Fragen (für den Nutzer, vor der Planung zu klären)

- **(a)** Dürfen im Umfang „vollständig“ gespeichertes HTML und Screenshots geteilt werden? Betrifft Datenschutz, Urheberrecht und mögliche Sitzungsdaten (Warnung nach TEAM-003 vorgesehen); Alternative: Umfang „vollständig“ in Stufe 1 nur ohne HTML und Screenshots.
- **(b)** `cookies` und `http_headers` immer entfernen bzw. auf die Allowlist reduzieren, ohne Ausnahme? **Empfehlung: ja** (so in TEAM-003 formuliert).
- **(c)** Abrechnung Stufe 2 (Nutzerplätze, Speicher, beides) und ob Stufe 1 und 2 im selben Team gemischt nutzbar sind (Ordner plus Ablage).
- **(d)** Rechtliche Prüfung, ob das Teilen von Crawl-Inhalten fremder Websites in Stufe 2 (gehostet) zusätzliche Regeln braucht; Stufe 1 überträgt nur zwischen den Nutzern selbst.
- **(e)** Aufbewahrung im Papierkorb (TEAM-007): **Vorschlag 30 Tage**, danach löschbar; oder unbegrenzt bis zum manuellen Leeren?
- **(f)** Wer darf `min_version` in `team.json` ändern? Stufe 1 hat keine Rechte; vorläufig jede Installation mit Schreibrecht nach Bestätigung, eine Einschränkung ist erst mit Stufe 2 (TEAM-050) durchsetzbar.
