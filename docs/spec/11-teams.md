# 11 – Teams: Crawls im Team teilen (ENTWURF)

**Status:** Entwurf vom 09.11.2026 (Nutzeridee, Ausarbeitung Orchestrator). Keinem Meilenstein zugeordnet, Umsetzung frühestens nach Release 1.0 (Roadmap R10). Vor der Planung prüft Fable den Entwurf; offene Punkte stehen am Ende.

Ziel: Mehrere Personen eines Teams arbeiten mit denselben Crawls, ohne dass OpenSpider selbst crawlt oder Crawl-Server betreibt. Gecrawlt wird weiterhin auf dem Rechner der Nutzer; das Team teilt die Ergebnisse.

Grundsätze:
- **Crawlen bleibt lokal.** Kein Team-Merkmal startet Crawls auf fremden oder eigenen Servern. Ein Teammitglied kann einen eigenen Rechner (z. B. im Büro) als Crawl-Rechner betreiben; das ist ein normaler OpenSpider mit geplanten Crawls.
- **Geteilt werden unveränderliche Schnappschüsse.** Eine veröffentlichte Version ändert sich nie; eine neue Veröffentlichung ist eine neue Version. Jede Crawl-Datei hat genau einen Besitzer, alle anderen lesen.
- **Keine Geheimnisse im Team.** Zugangsdaten, Cookies, Tokens und Formular-Logins verlassen den Rechner nie (TEAM-003).
- **Autark bleibt autark.** Ohne Team-Einrichtung ändert sich an OpenSpider nichts; Team-Funktionen sind ausschaltbar und brauchen in Stufe 1 weder Konto noch Netzwerkdienst von OpenSpider.
- **Zwei Stufen** (Geschäftsmodell Open Core, `docs/plan/ROADMAP-nach-1.0.md`): **Stufe 1 „Team-Ordner“** ist frei (Apache-2.0) und nutzt einen gemeinsamen Ordner (NAS, Netzlaufwerk, Dropbox, Google Drive, OneDrive o. Ä.). **Stufe 2 „OpenSpider Teams“** ist ein bezahlter, gehosteter Dienst über den evnxt-Account (Anmeldung, Rechte, Verlauf, Alarme, Kommentare, Kundenansicht).

---

## A. Stufe 1 – Team-Ordner (frei)

### TEAM-001 Team einrichten
- **Priorität:** P1
- **Beschreibung:** In den Einstellungen legt der Nutzer ein Team an: Name und Team-Ordner. Mehrere Teams sind möglich (z. B. je Agentur oder Kunde). Beim ersten Einrichten wird im Ordner eine Team-Kennung (`team.json`: UUID, Name, Mindestversion nach TEAM-006, Formatversion) angelegt; findet die App eine vorhandene `team.json`, tritt sie diesem Team bei. Ein Team lässt sich lokal entfernen, ohne den Ordner zu verändern.
- **Akzeptanzkriterien:**
  - [ ] Leerer Ordner → `team.json` wird angelegt; zweite Installation mit demselben Ordner zeigt dasselbe Team (gleiche UUID).
  - [ ] Ordner ohne Schreibrecht → Team nur lesend nutzbar, Hinweis „Veröffentlichen nicht möglich“.
  - [ ] Ordner nicht erreichbar (Netzlaufwerk getrennt) → Team als „nicht verbunden“ markiert, lokale Kopien (TEAM-004) bleiben nutzbar, keine Fehlermeldungsschleife.
- **Testfixture:** temporäre Verzeichnisse für leer, vorhanden, nur lesend, nicht vorhanden.

### TEAM-002 Crawl veröffentlichen
- **Priorität:** P1
- **Beschreibung:** Aus der Crawl-Verwaltung oder dem geöffneten Crawl heraus wird ein Crawl im Team veröffentlicht. Die App erzeugt einen konsistenten Schnappschuss (auch während eines laufenden Crawls, Stand des letzten dauerhaften Commits) und schreibt ihn mit Manifest in den Team-Ordner. Umfang wählbar: **vollständig** (ganze Crawl-Datei) oder **nur Ergebnisse** (alle Tabellen, Befunde, Berichte und Exporte, aber ohne gespeichertes HTML, Quelltexte, Screenshots und gerenderte DOMs). Manifest: Team-UUID, Crawl-ID, Version (fortlaufend je Crawl), Start-URL/Domain, Projekt, Crawl-Zeitraum, Zustand und Endgrund, URL-Anzahl, Umfang, OpenSpider-Version, Schema-Version, Größe, SHA-256, veröffentlichender Nutzer (frei wählbarer Anzeigename, keine Systemkennung), Zeitpunkt.
- **Akzeptanzkriterien:**
  - [ ] Veröffentlichen eines laufenden Crawls stört den Crawl nicht (keine Sperrfreigabe, kein Datenverlust; Prüfung wie CRAWL-125) und liefert eine Datei mit `PRAGMA integrity_check = ok`.
  - [ ] „Nur Ergebnisse“ ist für einen Crawl mit gespeichertem HTML deutlich kleiner und enthält keine HTML-/Quelltext-/Screenshot-Inhalte; alle Tabellen und Berichte zeigen dieselben Werte wie das Original.
  - [ ] Zweimaliges Veröffentlichen desselben Crawls ergibt Version 1 und 2; Version 1 bleibt unverändert.
  - [ ] Abbruch während des Schreibens (Prozess beendet, Ordner getrennt) hinterlässt keine als gültig gelistete Teilversion (TEAM-008).
- **Testfixture:** `out-basic-site` mit gespeichertem HTML; laufender Fixture-Crawl.

### TEAM-003 Keine Geheimnisse im Schnappschuss
- **Priorität:** P0 (innerhalb dieses Kapitels)
- **Beschreibung:** Vor dem Schreiben entfernt die App aus dem Schnappschuss alles, was Zugangsdaten enthalten kann: Authentifizierungs- und Formular-Login-Konfiguration, Cookies und Header mit Werten, Proxy-Zugangsdaten, API-Schlüssel und Tokens von Integrationen. Übrig bleibt höchstens der Hinweis, dass eine Authentifizierung genutzt wurde. Eine abschließende Prüfung sucht im Schnappschuss nach bekannten Geheimnis-Mustern und Werten aus dem SecretStore; bei Fund wird nicht veröffentlicht.
- **Akzeptanzkriterien:**
  - [ ] Crawl mit Formular-Login, eigenem Cookie-Header, Proxy mit Passwort und API-Schlüssel einer Integration → keiner der Werte steht irgendwo im Schnappschuss (Byte-Suche über die ganze Datei).
  - [ ] Künstlich eingeschleuster Geheimniswert in einer Tabelle → Veröffentlichung verweigert, Meldung nennt die Fundstelle (Tabelle/Spalte), nicht den Wert.
  - [ ] Die Original-Crawl-Datei bleibt unverändert.
- **Testfixture:** Crawl-Fixture mit Testzugangsdaten aus der Projekt-Fixture.

### TEAM-004 Team-Bibliothek
- **Priorität:** P1
- **Beschreibung:** Die Crawl-Verwaltung zeigt je Team einen Bereich „Team“ mit allen veröffentlichten Crawls, gruppiert nach Projekt und Domain, mit den Manifest-Feldern als Spalten, filter- und sortierbar. Öffnen lädt die Version in einen lokalen Zwischenspeicher (Größenlimit einstellbar, älteste zuerst verdrängt) und öffnet sie **nur lesend** (Sperre wie OUT-228); Bearbeiten erzeugt auf Wunsch eine eigene lokale Kopie. Die Liste aktualisiert sich bei Änderungen im Ordner ohne Neustart.
- **Akzeptanzkriterien:**
  - [ ] Eine von Installation A veröffentlichte Version erscheint bei Installation B ohne Neustart und lässt sich dort öffnen; alle Tabellen, Filter und Berichte funktionieren lesend.
  - [ ] Versuch, eine Team-Version zu verändern (Crawl fortsetzen, URLs hinzufügen, Konfiguration ändern) → nicht angeboten; „Als eigene Kopie öffnen“ erzeugt eine unabhängige lokale Datei.
  - [ ] Liste mit 1 000 Versionen bleibt bedienbar (Öffnen der Ansicht < 1 s ab lokalem Ordner).
- **Testfixture:** Team-Ordner mit Manifesten mehrerer Domains und Versionen.

### TEAM-005 Verlauf und Vergleich je Domain
- **Priorität:** P1
- **Beschreibung:** Für jede Domain zeigt die Bibliothek den Verlauf aller veröffentlichten Versionen (Datum, URL-Anzahl, Anteil Fehlerseiten, Indexierbarkeit). Zwei Versionen derselben Domain lassen sich direkt mit dem Bericht „Änderungen seit dem letzten Crawl“ vergleichen; beim Eintreffen einer neuen Version bietet die App diesen Vergleich gegen die vorige an.
- **Akzeptanzkriterien:**
  - [ ] Zwei Versionen einer Domain → Vergleichsbericht mit denselben Zahlen wie beim lokalen Vergleich derselben Dateien.
  - [ ] Verlauf zeigt Versionen verschiedener Teammitglieder chronologisch, auch wenn Uhren der Rechner abweichen (Sortierung nach Crawl-Zeitraum, Konflikte markiert).
- **Testfixture:** zwei Fixture-Crawls derselben Site mit bekannten Unterschieden.

### TEAM-006 Versionen der App im Team
- **Priorität:** P1
- **Beschreibung:** Jede Version trägt Schema- und App-Version. Eine App, die das Schema nicht lesen kann, listet die Version mit Hinweis „benötigt OpenSpider ≥ x.y“ statt eines Fehlers. Im Team lässt sich eine Mindestversion setzen; ältere Apps können dann nicht veröffentlichen und zeigen einen Hinweis zum Update. Ältere Versionen werden von neueren Apps nach OUT-255 gelesen (Migration in der lokalen Kopie, nie im Team-Ordner).
- **Akzeptanzkriterien:**
  - [ ] Version mit höherem Schema → in älterer App gelistet, nicht geöffnet, Hinweis mit Mindestversion.
  - [ ] Version mit älterem Schema → in neuerer App geöffnet; die Datei im Team-Ordner bleibt byte-gleich.
- **Testfixture:** Fixture-Archiv aus OUT-255.

### TEAM-007 Zurückziehen und Löschen
- **Priorität:** P1
- **Beschreibung:** Wer veröffentlicht hat, kann eine Version zurückziehen (aus der Liste ausgeblendet, Datei in einen Papierkorb-Unterordner des Teams) oder endgültig löschen (nach Bestätigung, ohne Wiederherstellung). Stufe 1 hat keine Rechteverwaltung: Das Ordnerrecht entscheidet; die App bietet Zurückziehen nur dem veröffentlichenden Anzeigenamen an und warnt bei fremden Versionen.
- **Akzeptanzkriterien:**
  - [ ] Zurückgezogene Version verschwindet bei allen Installationen aus der Liste und lässt sich aus dem Papierkorb wiederherstellen.
  - [ ] Endgültiges Löschen verlangt eine ausdrückliche Bestätigung mit Versionsangabe.

### TEAM-008 Sicher mit Sync-Diensten
- **Priorität:** P0 (innerhalb dieses Kapitels)
- **Beschreibung:** Im Team-Ordner liegt nie eine geöffnete Datenbank: Geschrieben wird in eine temporäre Datei mit Präfix `.partial-`, nach Abschluss Prüfsumme gebildet, dann atomar umbenannt; das Manifest wird zuletzt geschrieben. Gelistet wird nur, was ein gültiges Manifest mit passender Prüfsumme hat. Konfliktkopien von Sync-Diensten (z. B. „(1)“, „conflicted copy“) werden erkannt und als solche angezeigt, nie stillschweigend als Version übernommen. Gelesen wird ausschließlich aus dem lokalen Zwischenspeicher (TEAM-004), nie direkt aus dem Ordner.
- **Akzeptanzkriterien:**
  - [ ] Unvollständige Datei, falsche Prüfsumme oder fehlendes Manifest → nicht gelistet, Eintrag im Log.
  - [ ] Zwei Installationen veröffentlichen gleichzeitig denselben Crawl → zwei getrennte, gültige Versionen, keine beschädigte Datei.
  - [ ] Konfliktkopie im Ordner → als Konflikt markiert, nicht automatisch geöffnet.
- **Testfixture:** simulierte Sync-Zustände (Teildatei, Konfliktkopie, verspätetes Manifest).

### TEAM-009 Geteilte Konfigurationen
- **Priorität:** P2
- **Beschreibung:** Crawl-Konfigurationen, Presets, Custom-Extraction-Regeln, Filter und Berichtsvorlagen lassen sich im Team teilen und von dort übernehmen; dabei gilt TEAM-003 (keine Geheimnisse). Übernommene Konfigurationen sind lokale Kopien.
- **Akzeptanzkriterien:**
  - [ ] Geteiltes Preset mit Login-Konfiguration → beim Übernehmen fehlen die Zugangsdaten, die App fordert sie lokal an.

### TEAM-010 CLI, MCP und geplante Crawls
- **Priorität:** P1
- **Beschreibung:** CLI: `--publish-team <team>` nach einem Crawl (mit `--publish-scope full|results`), `--list-team <team>` als JSON. Geplante Crawls (Aufgaben) können nach Abschluss automatisch veröffentlichen. MCP: Die Team-Bibliothek ist lesend abfragbar (Liste, Verlauf, Vergleich), Veröffentlichen nur mit der Freigabeklasse `write` (AGENT-003).
- **Akzeptanzkriterien:**
  - [ ] Geplanter Crawl mit „im Team veröffentlichen“ → nach Abschluss neue Version im Team-Ordner, auch ohne geöffnete App.
  - [ ] MCP ohne `write` → Veröffentlichen-Werkzeug nicht gelistet.

### TEAM-011 Befund-Board (Idee)
- **Priorität:** P2 (Idee, Nutzer 09.11.2026: „vielleicht erstmal zu weit gedacht“)
- **Beschreibung:** Ein gemeinsames Board je Team bzw. Domain, auf dem Befunde aus veröffentlichten Crawls gesammelt und besprochen werden (Befund übernehmen, kommentieren, Status offen/erledigt/ignoriert, zuweisen), direkt in OpenSpider statt in fremden Tools. In Stufe 1 als Dateien im Team-Ordner (je Eintrag eine Datei, nur anhängen, keine gemeinsame Datenbank; TEAM-008 gilt), in Stufe 2 als TEAM-052.

---

## B. Stufe 2 – OpenSpider Teams (bezahlt, evnxt-Account)

Grundsatz auch hier (Nutzer 09.11.2026): **OpenSpider crawlt nie selbst für Kunden.** Stufe 2 hostet nur die Ablage und die Zusammenarbeit; gecrawlt wird immer auf den Rechnern der Nutzer.

Gemeinsame Regeln: Die Ablage ersetzt den Team-Ordner, die Regeln aus Abschnitt A (Schnappschüsse, TEAM-003, TEAM-006, TEAM-008) gelten unverändert. Daten liegen in der EU, verschlüsselt bei der Übertragung und im Speicher; Auftragsverarbeitungsvertrag für Kunden. OpenSpider funktioniert ohne Anmeldung vollständig weiter.

### TEAM-050 Anmeldung, Teams und Rechte
- **Priorität:** P2
- **Beschreibung:** Anmeldung mit dem evnxt-Account; Teams mit Rollen **Besitzer**, **Bearbeiter** (veröffentlichen, kommentieren), **Leser** (öffnen, kommentieren) je Team und optional je Projekt; Einladen per E-Mail; Entfernen eines Mitglieds entzieht sofort den Zugriff auf neue Downloads.

### TEAM-051 Gehostete Ablage und Kontingente
- **Priorität:** P2
- **Beschreibung:** Veröffentlichen lädt in die Ablage hoch (fortsetzbar, komprimiert); Kontingente je Team für Speicher und Aufbewahrungsdauer; Hinweis vor Erreichen der Grenze; Export aller Team-Daten jederzeit.

### TEAM-052 Kommentare und Aufgaben
- **Priorität:** P2
- **Beschreibung:** Kommentare und Aufgaben (offen, erledigt, ignoriert, zugewiesen) je URL, Befund oder Bericht einer Version, als eigene Schicht neben dem Schnappschuss; sie wandern bei einer neuen Version derselben Domain mit, solange die URL bzw. der Befund noch existiert.

### TEAM-053 Alarme
- **Priorität:** P2
- **Beschreibung:** Regeln je Domain (z. B. neue 4xx/5xx-Seiten, neue Noindex-Seiten, verlorene Canonicals, Rückgang der indexierbaren Seiten um mehr als x %); beim Eintreffen einer neuen Version werden sie gegen die vorige ausgewertet; Benachrichtigung per E-Mail oder Webhook (z. B. Slack).

### TEAM-054 Kundenansicht
- **Priorität:** P2
- **Beschreibung:** Nur lesender Link oder PDF-Bericht aus einer Version für Externe, mit Ablaufdatum und widerrufbar; zeigt nur die freigegebenen Berichte.

### TEAM-055 Protokoll
- **Priorität:** P2
- **Beschreibung:** Unveränderliches Protokoll je Team: wer hat wann veröffentlicht, zurückgezogen, gelöscht, eingeladen, Rechte geändert.

---

## Offene Fragen (vor der Planung zu klären)

1. Stufe 1: Reicht der Anzeigename als Autorangabe, oder braucht es eine einfache Signatur je Installation, damit Versionen nicht unter fremdem Namen veröffentlicht werden können?
2. Umfang „nur Ergebnisse“: Welche Tabellen und Berichte brauchen gespeicherte Inhalte (z. B. Rechtschreibung, Ähnlichkeit, Custom Search) und fallen dann weg oder werden vorab berechnet?
3. Größe: Kompression im Schnappschuss (Format, Zielgröße für 1 Mio. URLs) und ob große Versionen in Teilen übertragen werden.
4. Stufe 2: Abrechnungseinheit (Nutzerplätze, Speicher, beides) und ob Stufe 1 und 2 im selben Team gemischt nutzbar sind (Ordner plus Ablage).
5. Rechtliches: Prüfung, ob das Teilen von Crawl-Inhalten fremder Websites in Stufe 2 (gehostet) zusätzliche Regeln braucht; Stufe 1 überträgt nur zwischen den Nutzern selbst.
6. Verhältnis zu R6 (Monitoring in evnxt/Autonomo): Alarme aus TEAM-053 und Monitoring dort zusammenführen oder trennen.
