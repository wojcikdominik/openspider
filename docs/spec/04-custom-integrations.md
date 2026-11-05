# 04 – Custom-Funktionen & API-Integrationen (INTEG)

Bereich: benutzerdefinierte Suche und Extraktion, benutzerdefiniertes JavaScript, Link-Positionen, Google Analytics 4, Google Search Console (Search Analytics + URL Inspection), PageSpeed Insights / Lighthouse / CrUX, Linkmetriken (Majestic, Ahrefs, Moz), KI-Anbieter (OpenAI, Gemini, Anthropic, Ollama, OpenAI-kompatible Endpunkte) für Prompts und Embeddings, API-Authentifizierung und Kontingentverwaltung.

Referenzstand: Oktober 2026, Referenzprodukt (Verhalten laut öffentlicher Doku, FAQ, Tutorials und Release-Notes mehrerer Hauptversionen). Alle Beschreibungen sind eigenständig formuliert; es wurde kein Code des Referenzprodukts betrachtet. Wo die öffentliche Doku keine Details nennt (z. B. GSC-Suchtyp, Dimensionsfilter), leitet die Spezifikation das Verhalten aus den öffentlich dokumentierten Fähigkeiten der jeweiligen Anbieter-API ab und markiert das mit **(abgeleitet)**.

---

## Übersicht

| Abschnitt | IDs | Inhalt | P0 | P1 | P2 |
|---|---|---|---|---|---|
| A Gemeinsame API-Infrastruktur | INTEG-001 – INTEG-019 | Verbindungen, Credentials, OAuth, Kontingente, Fortschritt, APIs-Modus, URL-Abgleich, Testbarkeit | 9 | 9 | 1 |
| B Custom Search | INTEG-020 – INTEG-035 | Enthält/enthält nicht, Text/Regex, Suchbereiche, Ausgabe, Bulk Add | 8 | 7 | 1 |
| C Custom Extraction | INTEG-040 – INTEG-055 | XPath, CSS, Regex, Ausgabearten, Mehrfachwerte, visuelle Extraktion | 11 | 3 | 2 |
| D Custom JavaScript | INTEG-060 – INTEG-072 | Extraktions-/Aktions-Snippets, Laufzeit-API, Bibliothek, Tester | 0 | 6 | 7 |
| E Custom Link Positions | INTEG-080 – INTEG-084 | Klassifikation nach XPath-Teilstrings | 0 | 4 | 1 |
| F Google Analytics 4 | INTEG-090 – INTEG-101 | Property, Zeitraum, Metriken, Dimensionen, Filter, Analytics-Tab, Kompatibilitätsprüfung | 0 | 12 | 0 |
| G Search Console – Search Analytics | INTEG-110 – INTEG-117 | Property, Zeitraum, Suchtyp, Dimensionsfilter, Tab | 0 | 8 | 0 |
| H Search Console – URL Inspection | INTEG-120 – INTEG-128 | Kontingent, Mehrfach-Properties, 17 Felder, 7 Filter, Read-Timeout | 0 | 8 | 1 |
| I PageSpeed Insights / Lighthouse | INTEG-130 – INTEG-147 | Remote/Lokal, Gerät, Overview, CrUX, Lab, Insights, Diagnostics, 19 Filter, Datenbeschaffung für den Mobile-Tab (Tab/Filter führend in TECH-231 ff.), Verweise Plugins / Mobile Alternate Link | 0 | 15 | 3 |
| J Linkmetriken | INTEG-150 – INTEG-157 | Ebenen, Majestic, Ahrefs (inkl. Optionen Land/Volumen), Moz, Link-Metrics-Tab | 0 | 6 | 2 |
| K KI-Anbieter & Prompts | INTEG-160 – INTEG-179 | Konten, Endpunkte, Prompts, Eingaben, Bibliothek, Tester, Limits, KI-Tab, System-Prompt, Tokenverbrauch, Timeout | 0 | 14 | 6 |
| L Embeddings & semantische Analyse | INTEG-180 – INTEG-189 | Ähnlichkeit, Low Relevance, Filterregeln, semantische Suche, Cluster, Reduktions-Presets (führend für Embeddings, ONPAGE-159 … 164 verweisen hierher) | 0 | 10 | 0 |

Summe: 154 Funktionen (P0: 28, P1: 102, P2: 24). Davon sind 6 Einträge Verweis-Stubs auf führende Einträge anderer Bereiche (INTEG-016, 017, 034, 145, 146, 147); sie behalten ihre ID und tragen die Priorität des führenden Eintrags.

## Inhaltsverzeichnis

**A Gemeinsame API-Infrastruktur**
- INTEG-001 API-Zugriffsdialog und Verbindungsstatus
- INTEG-002 Sichere Ablage von Zugangsdaten
- INTEG-003 Google-OAuth-2.0 (Desktop-Loopback mit PKCE)
- INTEG-004 Token-Lebenszyklus und Ablauf
- INTEG-005 Ratenbegrenzung und Wiederholungslogik
- INTEG-006 Kontingentzähler und Tageskontingente
- INTEG-007 API-Warteschlange und Fortschrittsanzeige
- INTEG-008 API-Daten nachträglich bzw. bei pausiertem Crawl anfordern
- INTEG-009 API-Daten für einzelne URLs neu anfordern
- INTEG-010 Invalidierung bei Konfigurationsänderung
- INTEG-011 Automatisch verbinden beim Start
- INTEG-012 APIs-Modus (Datenabruf ohne Crawl)
- INTEG-013 Integrationsspalten im Internal-Tab
- INTEG-014 URL-Abgleich zwischen API-Daten und Crawl
- INTEG-015 Neue URLs aus API-Quellen crawlen (Orphan-Erkennung)
- INTEG-016 Proxy und eigener Truststore für API-Verbindungen
- INTEG-017 API-Integrationen in Zeitplanung und CLI
- INTEG-018 Datenschutz und lokale Datenhaltung
- INTEG-019 Testbarkeit: überschreibbare API-Basis-URLs und Mock-Server

**B Custom Search**
- INTEG-020 Suchfilter definieren
- INTEG-021 Suchbereich „HTML"
- INTEG-022 Suchbereich „Page Text"
- INTEG-023 Suchbereich „Page Text No Anchors"
- INTEG-024 Suchbereich „HTML Head"
- INTEG-025 Suchbereich „HTML Body"
- INTEG-026 Suchbereich „XPath"
- INTEG-027 Suchbereich „Content Area"
- INTEG-028 Regex-Semantik
- INTEG-029 Groß-/Kleinschreibung bei Textsuche
- INTEG-030 Rohes vs. gerendertes HTML
- INTEG-031 Custom-Search-Tab und Ergebnisspalten
- INTEG-032 Bulk Add
- INTEG-033 Reihenfolge der Filter
- INTEG-034 Bulk-Export „Inlinks je Suchfilter"
- INTEG-035 Geltungsbereich der Suche

**C Custom Extraction**
- INTEG-040 Extraktor definieren und Limits
- INTEG-041 XPath 1.0
- INTEG-042 XPath 2.0 / 3.0 / 3.1
- INTEG-043 CSS-Selektor mit optionalem Attribut
- INTEG-044 Regex-Extraktion
- INTEG-045 Ausgabeart „HTML-Element"
- INTEG-046 Ausgabeart „Inner HTML"
- INTEG-047 Ausgabeart „Text"
- INTEG-048 Ausgabeart „Funktionswert"
- INTEG-049 Mehrfachtreffer und nummerierte Spalten
- INTEG-050 Kombinierte Ausdrücke (Union mit `|`)
- INTEG-051 Syntaxprüfung im Dialog
- INTEG-052 Geltungsbereich der Extraktion
- INTEG-053 Custom-Extraction-Tab und Export
- INTEG-054 Visuelle Extraktion
- INTEG-055 Reihenfolge der Extraktoren

**D Custom JavaScript**
- INTEG-060 Snippet-Verwaltung
- INTEG-061 Extraktions-Snippets
- INTEG-062 Aktions-Snippets
- INTEG-063 Laufzeit-API: Rückgabe von Daten und Fehlern
- INTEG-064 Laufzeit-API: Text in Datei speichern
- INTEG-065 Laufzeit-API: URLs herunterladen
- INTEG-066 Laufzeit-API: externe Skripte laden
- INTEG-067 Chrome-Konsolen-Hilfsfunktionen
- INTEG-068 Snippet-Tester und Debugging im externen Browser
- INTEG-069 Mitgelieferte Snippet-Bibliothek
- INTEG-070 Benutzer-Bibliothek, Import/Export
- INTEG-071 Custom-JavaScript-Tab
- INTEG-072 Sicherheitsgrenzen für Snippets

**E Custom Link Positions**
- INTEG-080 Klassifikation der Link-Position
- INTEG-081 Standardregeln
- INTEG-082 Eigene Regeln und Reihenfolge
- INTEG-083 Funktion abschaltbar
- INTEG-084 Ausgabe in Inlinks/Outlinks

**F Google Analytics 4**
- INTEG-090 Verbindung und Auswahl von Konto, Property, Datenstream
- INTEG-091 Zeitraum
- INTEG-092 Metrikauswahl
- INTEG-093 Seitendimension (vollständige URL vs. Landingpage)
- INTEG-094 Dimensionsfilter
- INTEG-095 Abgleichoptionen und Ergebnislimit
- INTEG-096 Paginierung großer Reports
- INTEG-097 Analytics-Tab: Spalten
- INTEG-098 Analytics-Tab: Filter
- INTEG-099 GA4-Kontingente
- INTEG-100 GA4-Daten in Segmenten und Reports
- INTEG-101 Kompatibilitätsprüfung von GA4-Metriken und -Dimensionen

**G Search Console – Search Analytics**
- INTEG-110 Verbindung und Property-Auswahl
- INTEG-111 Zeitraum und Datenstand
- INTEG-112 Suchtyp
- INTEG-113 Dimensionsfilter (Suchanfrage, Land, Gerät)
- INTEG-114 Paginierung und Zeilenlimit
- INTEG-115 Search-Console-Tab: Spalten
- INTEG-116 Search-Console-Tab: Search-Analytics-Filter
- INTEG-117 Aggregation und Doppel-URLs

**H Search Console – URL Inspection**
- INTEG-120 URL Inspection aktivieren und Tageskontingent
- INTEG-121 Nicht-indexierbare URLs auslassen
- INTEG-122 Mehrere Properties verwenden
- INTEG-123 URL-Inspection-Spalten
- INTEG-124 Ableitung der Spalte „Summary"
- INTEG-125 URL-Inspection-Filter
- INTEG-126 Detailansicht URL Inspection
- INTEG-127 Fehlerbehandlung URL Inspection
- INTEG-128 Read-Timeout für Search-Console-Anfragen

**I PageSpeed Insights / Lighthouse**
- INTEG-130 PSI-Verbindung und Quelle (Remote/Lokal)
- INTEG-131 Gerätetyp
- INTEG-132 Lokaler Lighthouse-Lauf
- INTEG-133 Metrikgruppen-Auswahl
- INTEG-134 Overview-Metriken
- INTEG-135 CrUX-Felddaten (URL und Origin)
- INTEG-136 Lighthouse-Labordaten
- INTEG-137 Insights-Spalten
- INTEG-138 Diagnostics-Spalten
- INTEG-139 PageSpeed-Tab: Filter
- INTEG-140 Mobile-Prüfungen aus Lighthouse
- INTEG-141 Accessibility-Score aus Lighthouse
- INTEG-142 PSI-Ratenbegrenzung und Durchsatz
- INTEG-143 PSI-/Lighthouse-Fehlerbehandlung
- INTEG-144 Lighthouse-Details (unterer Tab) und Ressourcen-Export
- INTEG-145 Report „PageSpeed Opportunities Summary"
- INTEG-146 Nicht unterstützte Plugins (Mobile)
- INTEG-147 Mobile Alternate Link

**J Linkmetriken**
- INTEG-150 Ebenen der Linkmetriken
- INTEG-151 Link-Metrics-Tab
- INTEG-152 Majestic
- INTEG-153 Ahrefs (API v3, OAuth)
- INTEG-154 Moz (API v3)
- INTEG-155 Batching und Verbrauchsschätzung
- INTEG-156 Erweiterte Linkmetriken (Topics, Anchor-Texte)
- INTEG-157 Ahrefs-Optionen: Land und Volumenbasis

**K KI-Anbieter & Prompts**
- INTEG-160 KI-Anbieterkonten
- INTEG-161 OpenAI-kompatible und eigene Endpunkte
- INTEG-162 Prompt-Konfiguration
- INTEG-163 Inhaltseingaben für Prompts
- INTEG-164 Erweiterter Prompt mit mehreren Zielen und Platzhaltern
- INTEG-165 Prompt nur auf Segment anwenden
- INTEG-166 Modellkategorien
- INTEG-167 Bild- und Sprachausgabe
- INTEG-168 Prompt-Bibliothek (mitgeliefert)
- INTEG-169 Benutzer-Prompts speichern, Import/Export
- INTEG-170 Prompt-Tester
- INTEG-171 Ratenbegrenzung je Modell
- INTEG-172 Inhaltslänge begrenzen
- INTEG-173 Leere Eingaben nicht senden
- INTEG-174 KI-Tab und Ergebnisspalten
- INTEG-175 Modellparameter
- INTEG-176 Kostenschutz
- INTEG-177 Systemweiter System-Prompt je Anbieter
- INTEG-178 Tokenverbrauch in den Kontoinformationen
- INTEG-179 Request-Timeout für KI-Anbieter

**L Embeddings & semantische Analyse**
- INTEG-180 Embedding-Funktionalität aktivieren
- INTEG-181 Semantische Ähnlichkeit
- INTEG-182 Low Relevance Content
- INTEG-183 Detailansicht „Semantic Similarity"
- INTEG-184 Embedding-Filterregeln
- INTEG-185 Semantische Suche
- INTEG-186 Content-Cluster-Diagramm
- INTEG-187 Dimensionsreduktion der Vektoren
- INTEG-188 Mehrere Embedding-Sätze
- INTEG-189 Voreinstellungen für die Dimensionsreduktion

---

## Konventionen für diesen Bereich

- **Spaltennamen** werden als kanonische, englische Export-Header angegeben (Kompatibilität mit bestehenden Auswerte-Workflows, Looker-Studio-Vorlagen usw.). Die UI darf sie lokalisieren; CSV/XLSX-Exporte verwenden immer die kanonischen Namen.
- **Filternamen, Tab-Namen und Statuswerte** (z. B. `URL is on Google, But Has Issues`, `Non-Indexable with GA Data`, `Does Not Contain`) gelten ebenso als **Schnittstellenbezeichner**: Sie werden bewusst 1:1 übernommen, damit Exporte, Bulk-Export-Namen, Zeitplan-Konfigurationen und bestehende Auswerte-Vorlagen kompatibel bleiben. Übernommen werden nur diese Bezeichner, nicht die erklärenden Texte dazu; alle Beschreibungen, Hinweis- und Fehlermeldungen sind eigenständig formuliert.
- **Bewusste Abweichungen** vom Referenzverhalten sind mit **(OpenSpider-Abweichung)** markiert, Werte, die nicht aus der Referenz-Doku, sondern aus Anbieter-Dokumentation oder eigener Festlegung stammen, mit **(abgeleitet)** bzw. **(Annahme, zu verifizieren)**.
- **Leerwerte:** Fehlt ein Wert, weil die API nichts geliefert hat, ist die Zelle leer (nicht `0`). `0` bedeutet „API hat 0 geliefert".
- **„Crawl-Analyse"** bezeichnet den Nachlauf nach Crawl-Ende; der Ablauf ist in CRAWL-141 (führend) festgelegt, die Ergebnisse je Filter in diesem Dokument. Filter, die ihn brauchen, zeigen bis dahin den Leerzustand aus UI-016 statt Ergebnissen.
- **Eine Quelle je Funktion:** Führend ist die Fachdomäne – externe APIs, KI und Embeddings in diesem Dokument; Crawl-Verhalten in 01, Tab-/Filter-/Spaltenlogik in 02/03, Darstellung und Detail-Tab-Layout in 05 (der Inhalt eines Detail-Tabs bleibt hier), Exporte/Reports/CLI/Scheduling in 06, Performance in 07. Nicht-führende Einträge sind als Verweis-Stub (`- **Verweis:** siehe <ID> (führend)`) gekürzt; ID und Nummerierung bleiben erhalten.
- **Speichermodus:** OpenSpider hat genau einen, festplattenbasierten Speichermodus (CRAWL-124); keine Funktion dieses Bereichs hängt von einem Speichermodus ab.
- **Gemeinsame Testinfrastruktur:** Alle Fixtures liegen unter `fixtures/integ/`. Ein lokaler Fixture-Webserver (`fixture-site`) liefert statische Testseiten unter `http://integ.test:8080/…`. Ein Mock-Server (`fixture-apis`, siehe INTEG-019) emuliert die externen APIs mit aufgezeichneten JSON-Antworten. Kein Akzeptanztest darf echte Fremd-APIs aufrufen; optionale Live-Tests laufen nur mit gesetzter Umgebungsvariable `OPENSPIDER_LIVE_API_TESTS=1`.

---

## A Gemeinsame API-Infrastruktur

### INTEG-001 API-Zugriffsdialog und Verbindungsstatus
- **Priorität:** P0
- **Beschreibung:** Ein zentraler Konfigurationsbereich „API-Zugriff" listet alle Integrationen (GA4, Search Console, PageSpeed Insights, Majestic, Ahrefs, Moz; ab M4 die KI-Anbieter unter „KI"; ab M5 DataForSEO unter „Daten-APIs", D4S-001). Je Integration gibt es die Unterbereiche Konto/Verbindung, Metriken bzw. Prompts, integrationsspezifische Optionen und „Erweitert". Der Verbindungsstatus ist jederzeit sichtbar.
- **Eingaben/Konfiguration:** Je Integration: Verbinden/Trennen, Kontoauswahl (sofern zutreffend), Integrationsoptionen.
- **Ausgabe (Spalten/Filter/Export):** Statusanzeige je Integration: `Nicht verbunden`, `Verbinde…`, `Verbunden (<Kontoname>)`, `Fehler: <Meldung>`. Die Konfiguration (ohne Geheimnisse) ist Teil des Konfigurationsprofils.
- **Akzeptanzkriterien:**
  - [ ] Der Dialog enthält je Ausbaustand genau diese Einträge: bis Release 1.0 (M1–M3) sechs – Google Analytics 4, Search Console, PageSpeed Insights, Majestic, Ahrefs, Moz; [M4] zusätzlich unter „KI" die fünf Anbieter OpenAI, Gemini, Anthropic, Ollama und OpenRouter (OpenRouter nativ nach AI-005/AI-009, übernommen aus 08), also elf; [M5] zusätzlich unter „Daten-APIs" DataForSEO (D4S-001, übernommen aus 09, Änderungsbedarf X1), also zwölf. Jeder Eintrag zeigt zu jedem Zeitpunkt genau einen der vier Status; der Statusautomat gilt für alle Einträge unverändert.
  - [ ] Zustandsübergänge: `Nicht verbunden` → (Klick „Verbinden") → `Verbinde…` → `Verbunden (…)` bei HTTP 2xx des Test-Requests bzw. `Fehler: …` bei jedem anderen Ergebnis; `Verbunden`/`Fehler` → (Klick „Trennen") → `Nicht verbunden`. Kein anderer Übergang ist möglich (Zustandsautomat-Unittest).
  - [ ] „Verbinden" führt genau einen Test-Request aus (GA4/GSC: Property-Liste; PSI: Analyse einer Test-URL bzw. Start des lokalen Lighthouse; Linkmetriken: Konto-/Kontingentabfrage; KI: Modellliste, bei OpenRouter `GET /models`; DataForSEO: kostenloses `GET /v3/appendix/user_data`) und setzt `Verbunden` erst nach dessen Erfolg (Mock-Log: genau 1 Request je Klick).
  - [ ] Antwortet der Test-Request nicht innerhalb von 30 s, wechselt der Status auf `Fehler: Zeitüberschreitung nach 30 s`.
  - [ ] Bei ungültigem Schlüssel (HTTP 401/403 des Anbieters) lautet der Status `Fehler: <HTTP-Code> <Meldung des Anbieters>`; ein parallel laufender Crawl wird nicht unterbrochen (Zahl gecrawlter URLs steigt weiter).
  - [ ] „Trennen" entfernt die aktive Verbindung; bereits geholte Daten bleiben im Crawl erhalten (Spaltenwerte vor/nach Trennen identisch).
  - [ ] Ein gespeichertes Konfigurationsprofil enthält Integrationsoptionen und Metrikauswahl, aber keine Schlüssel/Tokens (Prüfung: Profil-Datei per `grep` auf den Testschlüssel → kein Treffer).
  - [ ] Sind PSI, GA4 und eine KI gleichzeitig verbunden und antwortet jeder Mock mit 2 s Latenz, überlappen sich laut Mock-Zeitstempeln Anfragen verschiedener Integrationen zeitlich (es gibt mindestens ein Intervall, in dem ≥ 2 Integrationen gleichzeitig offene Anfragen haben).
- **Testfixture:** `fixture-apis` mit Endpunkten, die je nach Header `Authorization: Bearer good` → 200 bzw. `bad` → 401 liefern, plus Szenario `?scenario=hang` (antwortet nie); OpenRouter über `ai-mock` (08), DataForSEO über die Kontoprofile `mock-good`/`mock-bad` des DataForSEO-Mocks (Basic-Auth, 09); UI-Test verbindet nacheinander alle Einträge des jeweiligen Ausbaustands (6, 11 bzw. 12).

### INTEG-002 Sichere Ablage von Zugangsdaten
- **Priorität:** P0
- **Beschreibung:** API-Schlüssel, OAuth-Refresh-Tokens und Server-Header mit Geheimnissen werden im Schlüsselbund des Betriebssystems abgelegt (macOS Keychain, Windows Credential Manager), nie im Klartext in Konfigurationsdateien, Crawl-Datenbanken oder Logs. **Ablageschema (führend für alle Bereiche; Entscheidung (Fable-Empfehlung) vom 13.10.2026, aus der Prüfung von 08/09):** Dienst `OpenSpider`, Konto = Integrationskennung `<integration>` in Kleinbuchstaben, je Integration genau ein Eintrag (z. B. `ai.anthropic`, `ai.openai`, `ai.openrouter` aus AI-009, `dataforseo` aus D4S-001; die übrigen Integrationen aus INTEG-001 folgen demselben Muster). Besteht ein Zugang aus mehreren Teilen (z. B. DataForSEO: API-Login und API-Passwort; OAuth: Refresh-Token und Kontokennung), ist das gespeicherte Geheimnis ein JSON-Objekt mit diesen Teilen; Login, E-Mail-Adresse oder Kontokennung sind nie der Kontoname des Schlüsselbund-Eintrags.
- **Eingaben/Konfiguration:** Schlüsselfelder in den Kontodialogen; Option „Schlüssel merken" (Standard: an).
- **Ausgabe:** Maskierte Anzeige (`••••••••abcd`, letzte 4 Zeichen sichtbar).
- **Akzeptanzkriterien:**
  - [ ] Nach Eingabe eines Schlüssels und Neustart ist die Integration ohne erneute Eingabe nutzbar (bei „merken" = an).
  - [ ] Der Schlüssel erscheint in keiner Datei unter dem Datenverzeichnis und in keinem Log (Volltextsuche nach Testschlüssel ergibt 0 Treffer).
  - [ ] Bei „merken" = aus ist der Schlüssel nach Neustart nicht mehr vorhanden.
  - [ ] Jeder Eintrag liegt unter Dienst `OpenSpider` mit der Integrationskennung als Konto (Mock-Keychain-Test über alle Einträge aus INTEG-001 des jeweiligen Ausbaustands); kein Eintrag verwendet einen anderen Dienstnamen oder einen Login bzw. eine E-Mail-Adresse als Konto; ein Login-Wechsel überschreibt den bestehenden Eintrag.
  - [ ] Ist der Schlüsselbund gesperrt oder nicht verfügbar, erscheint eine verständliche Meldung, und der Schlüssel kann für die laufende Sitzung im Speicher verwendet werden.
  - [ ] Ein Export der Konfiguration für CLI/Zeitplanung (OUT-201, OUT-164) enthält Schlüssel nur, wenn der Nutzer dies ausdrücklich bestätigt; dann wird die Datei mit einem Passwort verschlüsselt (AES-256-GCM).
- **Testfixture:** Integrationstest mit Mock-Keychain-Backend; Dateisystem-Scan nach dem Teststring `sk-TEST-INTEG-002`.

### INTEG-003 Google-OAuth-2.0 (Desktop-Loopback mit PKCE)
- **Priorität:** P0
- **Beschreibung:** GA4 und Search Console werden über OAuth 2.0 für installierte Anwendungen verbunden: Systembrowser öffnet die Google-Zustimmung, Rückleitung an einen kurzlebigen Loopback-Listener (`http://127.0.0.1:<zufälliger Port>`), Code-Austausch mit PKCE (S256). Bereits autorisierte Google-Konten werden gemerkt und können ohne erneute Zustimmung gewählt werden. Weil OpenSpider Open Source ist, kann der Nutzer statt der mitgelieferten OAuth-Client-ID eine eigene Google-Cloud-Client-ID hinterlegen (wichtig, falls das Projekt die Google-Verifizierung für sensible Scopes nicht hat).
- **Eingaben/Konfiguration:** „Neues Google-Konto verbinden", Liste gemerkter Konten, „Konto entfernen", Erweitert: eigene Client-ID / Client-Secret.
- **Ausgabe:** Kontoname/E-Mail in der Statusanzeige.
- **Akzeptanzkriterien:**
  - [ ] Angefragte Scopes sind minimal: `analytics.readonly` für GA4, `webmasters.readonly` für Search Console; keine Schreib-Scopes.
  - [ ] Der Loopback-Listener lauscht nur auf 127.0.0.1, akzeptiert genau eine Rückleitung mit passendem `state` und schließt sich danach bzw. nach 5 Minuten Timeout.
  - [ ] Ein falscher `state`-Parameter führt zum Abbruch mit Meldung; kein Token wird gespeichert.
  - [ ] Nach erfolgreicher Anmeldung erscheint das Konto in der Liste gemerkter Konten; ein zweites Konto kann ergänzt werden, beide sind auswählbar.
  - [ ] „Konto entfernen" löscht das Refresh-Token aus dem Schlüsselbund und widerruft es beim Anbieter (Revoke-Endpunkt); scheitert der Widerruf, wird lokal trotzdem gelöscht und ein Hinweis gezeigt.
  - [ ] GA4- und GSC-Verbindungen derselben Instanz beeinflussen sich nicht gegenseitig (Konto A für GA4, Konto B für GSC gleichzeitig möglich).
  - [ ] Mit hinterlegter eigener Client-ID wird ausschließlich diese verwendet.
- **Testfixture:** `fixture-apis/oauth` emuliert Authorization- und Token-Endpunkt; Test simuliert Browser-Redirect per HTTP-Aufruf an den Loopback-Port, inkl. Negativfall mit manipuliertem `state`.

### INTEG-004 Token-Lebenszyklus und Ablauf
- **Priorität:** P1
- **Beschreibung:** Access-Tokens werden vor Ablauf automatisch per Refresh-Token erneuert. Anbieter mit begrenzter Token-Gültigkeit (Ahrefs: Token ca. ein Jahr gültig) zeigen das Ablaufdatum und warnen rechtzeitig.
- **Eingaben/Konfiguration:** keine (automatisch); Anzeige „gültig bis" im Kontodialog.
- **Ausgabe:** Statusmeldung bei Ablauf: `Fehler: Anmeldung abgelaufen – bitte neu verbinden`.
- **Akzeptanzkriterien:**
  - [ ] Läuft ein Access-Token während eines Crawls ab, wird es transparent erneuert; kein API-Request geht verloren (Mock: Token läuft nach 3 Requests ab → alle 10 URLs erhalten Daten).
  - [ ] Ist der Refresh-Token ungültig (`invalid_grant`), pausiert die betroffene Integration, der Crawl läuft weiter, und eine Meldung fordert zur Neuanmeldung auf.
  - [ ] Für Ahrefs wird 30 Tage vor Ablauf beim Programmstart ein Hinweis angezeigt.
- **Testfixture:** `fixture-apis/oauth` mit kurzer Token-Lebensdauer (`expires_in: 2`) und einem Modus, der beim Refresh `invalid_grant` liefert.

### INTEG-005 Ratenbegrenzung und Wiederholungslogik
- **Priorität:** P0
- **Beschreibung:** Jede Integration hat eine eigene Drosselung (Token-Bucket je Anbieter, je Modell bzw. je Property), damit dokumentierte Anbieterlimits eingehalten werden. Bei HTTP 429 und vorübergehenden Serverfehlern (500, 502, 503, 504) wird mit exponentiellem Backoff und Jitter wiederholt; ein `Retry-After`-Header hat Vorrang.
- **Eingaben/Konfiguration:** Je Integration „Erweitert": maximale Anfragen pro Zeitfenster (Vorgaben siehe jeweilige Integration), maximale Wiederholungen (Standard 5), maximale gleichzeitige Anfragen (Standard 5, Bereich 1–50). **Integrationen können abweichende Vorgaben für gleichzeitige Anfragen und Anfragen je Zeitfenster festlegen**; sie stehen dann im Eintrag der Integration und gelten dort vorrangig (übernommen aus 08/09): DataForSEO Standard 10, Bereich 1–30, höchstens 1.000 Anfragen je Minute und Endpunkt (D4S-005, Änderungsbedarf X3); Ollama Standard 1, da lokal rechenbegrenzt (AI-006). Ebenso legt der jeweilige Eintrag fest, welche anbieterspezifischen Antworten als Drosselung gelten (DataForSEO: HTTP 200 mit Statuscode 40202/40209, D4S-004/005; OpenRouter: 402 `openrouter_in_flight_budget` wie 429, AI-005) oder trotz HTTP 429 **nicht** wiederholt werden (Anthropic `enforced_spend_limit_reached`, AI-002). Backoff-Parameter (fest, nicht in der UI): Basis 1 s, Faktor 2, Obergrenze 60 s, Jitter ±20 % → Wartezeiten ca. 1, 2, 4, 8, 16 s.
- **Ausgabe:** Fehler, die nach allen Wiederholungen bestehen bleiben, werden je URL in den Statusspalten der Integration gespeichert (z. B. `PSI Status` = `Error`, `PSI Error` = `500 Unable to process request`).
- **Akzeptanzkriterien:**
  - [ ] Bei einer Vorgabe von 60 Anfragen / 100 s werden in einem beliebigen 100-s-Fenster nie mehr als 60 Anfragen gesendet (Messung am Mock-Server).
  - [ ] Antwortet der Mock zweimal mit 429 und dann mit 200, enthält die URL am Ende gültige Daten und keinen Fehler.
  - [ ] Ohne `Retry-After` liegen zwischen Versuch n und n+1 (n = 1…5) jeweils 0,8·2^(n−1) bis min(60, 1,2·2^(n−1)) s (Mock-Zeitstempel).
  - [ ] `Retry-After: 7` (Sekunden) bzw. ein HTTP-Datum 7 s in der Zukunft führt zu einer Wartezeit von mindestens 7 s vor der nächsten Anfrage an diesen Anbieter; der Header-Wert hat Vorrang vor dem Backoff.
  - [ ] 400/401/403/404 werden nicht wiederholt (Mock-Log: genau 1 Request).
  - [ ] Nach 5 erfolglosen Wiederholungen (insgesamt 6 Requests) wird der Fehler gespeichert und die URL als „fehlgeschlagen" markiert; sie kann über INTEG-008/INTEG-009 erneut angefordert werden.
  - [ ] Bei Einstellung „gleichzeitige Anfragen = 2" hat der Mock zu keinem Zeitpunkt mehr als 2 offene Anfragen dieser Integration.
  - [ ] Integrationen mit eigener Vorgabe zeigen im Reiter „Erweitert" deren Standard und Bereich ([M5] DataForSEO: Standard 10, Eingabe 31 wird abgelehnt; [M4] Ollama: Standard 1); alle anderen Integrationen zeigen Standard 5 und Bereich 1–50 (Konfigurationstest über alle Einträge aus INTEG-001).
  - [ ] Isolation: Crawl von `site-10-pages` mit PSI-Mock im Szenario `500-always` dauert höchstens 10 % länger als derselbe Crawl ohne Integration; eine gleichzeitig verbundene GA4-Integration erhält ihre Daten in derselben Zeit (±10 %) wie ohne gestörte PSI.
- **Testfixture:** `fixture-apis` mit Szenario-Schaltern `?scenario=429x2`, `?scenario=retry-after-7`, `?scenario=retry-after-date`, `?scenario=500-always`, `?scenario=slow-2s`; Zeitstempel-Log des Mock-Servers wird ausgewertet.

### INTEG-006 Kontingentzähler und Tageskontingente
- **Priorität:** P1
- **Beschreibung:** Für Anbieter mit Tageskontingent (PSI 25.000 Anfragen/Tag, GSC URL Inspection 2.000 URLs/Tag/Property, GA4-Token-Kontingente, Ahrefs-API-Einheiten pro Monat) führt OpenSpider einen lokalen Zähler, zeigt den Verbrauch an und stoppt vor Überschreitung. Google-Tageskontingente werden um Mitternacht US-Pazifikzeit zurückgesetzt.
- **Eingaben/Konfiguration:** Optionale eigene Obergrenze je Integration (z. B. „max. 500 URL-Inspections pro Crawl").
- **Ausgabe:** Kontingentanzeige im Kontodialog und in der Fortschrittsanzeige: `URL Inspection: 1.234 / 2.000 heute (sc-domain:example.com)`.
- **Akzeptanzkriterien:**
  - [ ] Der Zähler ist pro Google-Property und Kalendertag (America/Los_Angeles) getrennt und überlebt einen Neustart.
  - [ ] Ist das lokale Tageskontingent erschöpft, werden weitere Anfragen nicht gesendet; betroffene URLs erhalten den Status `Quota exceeded` (nicht leer).
  - [ ] Meldet der Anbieter Kontingentüberschreitung (z. B. HTTP 429 mit Grund `quotaExceeded`/`RESOURCE_EXHAUSTED`), wird der lokale Zähler auf das Limit gesetzt und keine weitere Anfrage mehr an diese Property gestellt.
  - [ ] Eine benutzerdefinierte Obergrenze unter dem Anbieterlimit wird eingehalten.
  - [ ] GA4: Liefert die API Kontingentinformationen (`propertyQuota`), werden „tokensPerDay" und „tokensPerHour" (verbraucht/verbleibend) angezeigt.
- **Testfixture:** Mock-URL-Inspection-Endpunkt mit konfigurierbarem Restkontingent 5; Crawl von `fixtures/integ/site-10-pages/` → genau 5 URLs mit Daten, 5 mit `Quota exceeded`.

### INTEG-007 API-Warteschlange und Fortschrittsanzeige
- **Priorität:** P0
- **Beschreibung:** API-Abfragen laufen in einer eigenen Warteschlange neben dem Crawl. Eine separate Fortschrittsanzeige „API" zeigt den Prozentsatz der erledigten API-Anfragen über alle verbundenen Integrationen. Zusätzlich gibt es in der rechten Seitenleiste eine Übersicht „API", die je Integration Fortschritt **und Fehleranzahl** ausweist. Daten erscheinen live an den URLs, sobald sie vorliegen.
- **Eingaben/Konfiguration:** keine.
- **Ausgabe:** Fortschrittsanzeige `API: 73 %`; Tooltip mit Aufschlüsselung je Integration inkl. Fehlern (`PSI 120/400 (3 Fehler), GSC 400/400 (0 Fehler), GA4 fertig (0 Fehler)`); Seitenleisten-Übersicht „API" (Layout und Spaltensatz führend in UI-070); dieser Eintrag legt fest, wie `Progress` (Prozent) und `Errors` (Anzahl URLs mit endgültigem Fehlerstatus) gezählt werden.
- **Akzeptanzkriterien:**
  - [ ] Liefert der PSI-Mock für 3 von 10 URLs einen endgültigen Fehler, zeigt die Übersicht „API" für PSI `Progress = 100 %` und `Errors = 3`; ein Doppelklick auf die Fehlerzahl öffnet den PageSpeed-Tab mit Filter auf die fehlerhaften URLs.
  - [ ] Die Fehleranzahl zählt nur endgültige Fehler (nach allen Wiederholungen gemäß INTEG-005); eine URL, die nach 2 × 429 erfolgreich war, zählt nicht. `Quota exceeded` zählt als Fehler.
  - [ ] Nach erfolgreicher Neuanforderung (INTEG-008/009) sinkt die Fehleranzahl entsprechend.
  - [ ] Bei Crawl-Ende kann die API-Anzeige < 100 % sein; der Crawl gilt erst als vollständig (für Auto-Analyse, Exporte in Zeitplänen), wenn auch die API-Warteschlange leer ist.
  - [ ] Die Anzeige erreicht 100 %, wenn alle URLs entweder Daten, „keine Daten" oder einen endgültigen Fehlerstatus haben.
  - [ ] Pausieren des Crawls pausiert auch die API-Warteschlange; Fortsetzen setzt beide fort.
  - [ ] Ohne verbundene Integration ist die API-Anzeige ausgeblendet.
- **Testfixture:** `fixtures/integ/site-10-pages/` + Mock-PSI mit 2 s Latenz; Szenario `errors-3of10` (3 feste URLs → `lh-errored-document`); UI-Test prüft Zwischenstände, Endstand 100 % und Fehlerzahl 3.

### INTEG-008 API-Daten nachträglich bzw. bei pausiertem Crawl anfordern
- **Priorität:** P1
- **Beschreibung:** Wird eine Integration erst nach dem Crawl oder während einer Pause verbunden, kann über die Schaltfläche „API-Daten anfordern" der Abruf für alle passenden URLs gestartet werden, ohne den Crawl fortzusetzen. Fehlgeschlagene URLs werden dabei erneut versucht.
- **Eingaben/Konfiguration:** Schaltfläche im API-Dialog bzw. in der Fortschrittsleiste; aktiv, sobald ein Crawl pausiert oder beendet ist und mindestens eine Integration verbunden ist.
- **Ausgabe:** wie Live-Abruf.
- **Akzeptanzkriterien:**
  - [ ] Nach einem abgeschlossenen Crawl ohne PSI → PSI verbinden → „API-Daten anfordern" füllt die PSI-Spalten aller passenden URLs; die Crawl-Daten bleiben unverändert.
  - [ ] Bei pausiertem Crawl werden nur API-Anfragen ausgeführt; die Zahl gecrawlter URLs ändert sich nicht.
  - [ ] Bereits erfolgreich abgefragte URLs werden nicht erneut angefragt (kein Doppelverbrauch von Kontingent); URLs mit Fehlerstatus werden erneut angefragt.
- **Testfixture:** gespeicherter Crawl `fixtures/integ/crawls/site-10-pages.osc` + Mock-PSI.

### INTEG-009 API-Daten für einzelne URLs neu anfordern
- **Priorität:** P1
- **Beschreibung:** Per Kontextmenü auf eine oder mehrere markierte URLs können Daten der verbundenen Integrationen neu abgerufen werden. Vorhandene Daten dieser URLs werden ersetzt. GA4 und Search Analytics sind ausgenommen, da sie als Gesamtreport geladen werden.
- **Eingaben/Konfiguration:** Kontextmenü „API → Daten neu anfordern → <Integration | Alle>".
- **Ausgabe:** Aktualisierte Zellen der markierten URLs.
- **Akzeptanzkriterien:**
  - [ ] Für PSI, URL Inspection, Linkmetriken und KI-Prompts ist der Eintrag aktiv; für GA4 und Search Analytics nicht vorhanden bzw. deaktiviert.
  - [ ] Nach Neuanforderung enthält die Zelle den neuen Wert (Mock liefert bei zweitem Aufruf einen anderen Performance-Score → neuer Wert sichtbar).
  - [ ] Mehrfachauswahl von 3 URLs erzeugt genau 3 Anfragen pro gewählter Integration.
- **Testfixture:** Mock-PSI mit Zähler-abhängiger Antwort (Aufruf 1: Score 0.42, Aufruf 2: Score 0.91).

### INTEG-010 Invalidierung bei Konfigurationsänderung
- **Priorität:** P1
- **Beschreibung:** Werden GA4- oder Search-Console-Einstellungen geändert, die die Daten beeinflussen (Property, Zeitraum, Metriken, Dimension, Filter, Suchtyp), fragt OpenSpider, ob vorhandene Daten dieser Integration gelöscht und neu geladen werden sollen.
- **Eingaben/Konfiguration:** Dialog mit „Löschen und neu laden" / „Behalten".
- **Ausgabe:** Bei „Behalten" wird im Tab ein Hinweis angezeigt, dass die Daten nicht der aktuellen Konfiguration entsprechen.
- **OS-Festlegung (Entscheidung (Fable-Empfehlung) vom 01.11.2026):** Die geänderte Konfiguration und die Invalidierung der betroffenen API-Daten („Löschen und neu laden“: Entfernen der Werte; „Behalten“: Hinweis „Daten stammen aus einer früheren Konfiguration“) werden in **einem** Commit der Crawl-Datei gespeichert. Es gibt keinen dauerhaften Zustand, in dem die neue Konfiguration ohne Hinweis bzw. ohne Löschung vorliegt. Scheitert der Commit (etwa an einer Sperre), bleibt die alte Konfiguration vollständig erhalten und der Dialog meldet den Fehler.
- **Akzeptanzkriterien:**
  - [ ] Änderung des GA4-Zeitraums nach einem Crawl löst den Dialog aus; „Löschen und neu laden" entfernt alle GA4-Werte und lädt sie mit dem neuen Zeitraum.
  - [ ] Reine UI-Änderungen (z. B. Spaltenreihenfolge) lösen keinen Dialog aus.
  - [ ] Bei „Behalten" bleiben die Werte unverändert und der Hinweis „Daten stammen aus einer früheren Konfiguration" ist sichtbar.
  - [ ] Eine Einstellungsänderung, die während der WAL-Rückführung (CRAWL-126) oder einer kurzen fremden Schreibsperre gespeichert wird, wartet im Busy-Timeout statt zu scheitern; danach sind Konfiguration und Hinweis bzw. Löschung gemeinsam gespeichert.
- **Testfixture:** gespeicherter Crawl mit GA4-Mockdaten; Mock liefert pro Zeitraum unterschiedliche Sessions.

### INTEG-011 Automatisch verbinden beim Start
- **Priorität:** P2
- **Beschreibung:** Je Integration kann „Beim Start automatisch verbinden" aktiviert werden; gespeicherte Zugangsdaten werden dann beim Programmstart bzw. Crawlstart ohne Klick verwendet.
- **Eingaben/Konfiguration:** Checkbox je Integration (Standard: aus).
- **Ausgabe:** Status `Verbunden` nach Start.
- **Akzeptanzkriterien:**
  - [ ] Mit aktivierter Option ist nach Neustart der Status `Verbunden`, ohne Benutzeraktion.
  - [ ] PSI mit Quelle „Lokal" verbindet sich ebenfalls automatisch (kein API-Schlüssel nötig).
  - [ ] Schlägt die automatische Verbindung fehl, startet der Crawl trotzdem und eine nicht-blockierende Meldung erscheint.
- **Testfixture:** Mock-APIs; Start der App mit vorbereitetem Profil.

### INTEG-012 APIs-Modus (Datenabruf ohne Crawl)
- **Priorität:** P1
- **Beschreibung:** Eigener Modus, in dem eine URL-Liste (Eingabe, Datei, Sitemap) hochgeladen wird und nur API-Daten abgerufen werden; die URLs werden nicht gecrawlt (kein HTTP-Request an die Zielseiten). Führender Eintrag für den APIs-Modus; CRAWL-037 verweist hierher. Aufruf per Kommandozeile: siehe CLI-Block in 06 (OUT-208).
- **Eingaben/Konfiguration:** Modus „APIs", URL-Liste, verbundene Integrationen.
- **Ausgabe:** Tabs der Integrationen und Internal-Tab mit Adresse und API-Spalten; Crawl-Spalten (Status Code, Title …) bleiben leer.
- **Akzeptanzkriterien:**
  - [ ] Im APIs-Modus registriert der Fixture-Webserver 0 Zugriffe auf die hochgeladenen URLs.
  - [ ] Für jede hochgeladene URL werden PSI, URL Inspection, Linkmetriken und GA4/GSC-Werte (sofern verbunden) zugeordnet.
  - [ ] KI-Prompts mit Seiteninhalt als Eingabe werden im APIs-Modus übersprungen (Status `No content – APIs mode`); Prompts ohne Seiteninhalt (nur URL) werden ausgeführt.
  - [ ] Doppelte URLs in der Liste werden nur einmal abgefragt.
  - [ ] Ist beim Start im APIs-Modus keine Integration verbunden, wird kein Lauf gestartet; stattdessen erscheint ein Hinweis, dass mindestens eine Integration verbunden sein muss (kein leerer Lauf, kein Eintrag in der Crawl-Verwaltung).
- **Testfixture:** `fixtures/integ/apis-mode-list.txt` (5 URLs, davon 1 Duplikat) + Request-Log des Fixture-Servers.

### INTEG-013 Integrationsspalten im Internal-Tab
- **Priorität:** P0
- **Beschreibung:** Alle Werte aus Custom Search, Custom Extraction, Custom JavaScript, GA4, GSC, PSI, Linkmetriken und KI-Prompts werden zusätzlich als Spalten im Internal-Tab geführt, damit ein Gesamtexport alle Daten enthält.
- **Eingaben/Konfiguration:** Spaltenauswahl (ein-/ausblenden) wie bei allen Tabs.
- **Ausgabe:** Internal-Tab-Spalten mit identischen Namen wie im Integrations-Tab.
- **Akzeptanzkriterien:**
  - [ ] Für jede konfigurierte Custom-Search-Regel, jeden Extraktor, jedes Extraktions-Snippet und jeden Prompt existiert im Internal-Tab genau eine (bzw. bei Mehrfachwerten n nummerierte) Spalte mit gleichem Namen und gleichen Werten wie im Fach-Tab.
  - [ ] Der CSV-Export des Internal-Tabs enthält diese Spalten.
  - [ ] Ohne konfigurierte Integration erscheinen keine leeren Integrationsspalten.
- **Testfixture:** `fixtures/integ/site-10-pages/` mit 1 Suchfilter, 1 Extraktor, Mock-PSI; Vergleich der Spalten in beiden Tabs.

### INTEG-014 URL-Abgleich zwischen API-Daten und Crawl
- **Priorität:** P0
- **Beschreibung:** API-Quellen, die Reports über viele URLs liefern (GA4, Search Analytics), müssen gecrawlten URLs zugeordnet werden. Das Protokoll (http/https) wird immer automatisch abgeglichen. Optional werden Varianten mit/ohne abschließenden Schrägstrich und mit abweichender Groß-/Kleinschreibung zusammengeführt. Gibt es mehrere Varianten in den API-Daten, wird die Variante mit dem höchsten Hauptwert (GA4: Sessions; GSC: Clicks, bei Gleichstand Impressions) zugeordnet; Werte werden **nicht** summiert.
- **Eingaben/Konfiguration:** „Slash- und Nicht-Slash-URLs abgleichen" (Standard an), „Groß-/Kleinschreibung abgleichen" (Standard an).
- **Ausgabe:** zugeordnete Werte in den Integrationsspalten.
- **Akzeptanzkriterien:**
  - [ ] Gecrawlte `https://integ.test:8080/a/` erhält Daten der API-Zeile `http://integ.test:8080/a/`.
  - [ ] API-Zeilen `/b` (Sessions 10) und `/b/` (Sessions 30) → gecrawlte `/b/` erhält 30, nicht 40.
  - [ ] Mit deaktiviertem Slash-Abgleich erhält gecrawlte `/c` keine Daten der API-Zeile `/c/`.
  - [ ] `/Produkt` (API, 5 Sessions) wird gecrawltem `/produkt` zugeordnet, wenn Groß-/Kleinschreibungs-Abgleich an ist; sonst nicht.
  - [ ] Query-Strings und Fragmente werden nicht verworfen: `/d?x=1` und `/d` sind verschiedene URLs.
  - [ ] Prozentkodierung wird vor dem Vergleich normalisiert (`/%C3%BC` = `/ü`).
- **Testfixture:** `fixtures/integ/matching/` (Seiten `/a/`, `/b/`, `/c`, `/produkt`, `/d`, `/ü`) + Mock-GA4-Report mit den Varianten.

### INTEG-015 Neue URLs aus API-Quellen crawlen (Orphan-Erkennung)
- **Priorität:** P1
- **Beschreibung:** Optional werden URLs, die nur in GA4 oder Search Analytics vorkommen (nicht über interne Links gefunden), in die Crawl-Warteschlange aufgenommen. Diese URLs gelten als Orphan-URLs ihrer Quelle. Ist die Option aus, erscheinen solche URLs nur im Orphan-Pages-Report.
- **Eingaben/Konfiguration:** GA4: „In GA4 entdeckte neue URLs crawlen"; GSC: „In Search Console entdeckte neue URLs crawlen" (beide Standard aus).
- **Ausgabe:** Filter `Orphan URLs` im Analytics- bzw. Search-Console-Tab (nach Crawl-Analyse); Report „Orphan Pages" mit Spalte `Source` (Report, Spalten und Quellwerte führend in OUT-099).
- **Akzeptanzkriterien:**
  - [ ] Mit aktivierter Option wird eine nur in der API vorkommende URL des gleichen Hosts gecrawlt und ist nach der Crawl-Analyse im Filter `Orphan URLs` enthalten.
  - [ ] URLs fremder Hosts aus den API-Daten werden nie gecrawlt.
  - [ ] Ohne aktivierte Option wird die URL nicht gecrawlt, erscheint aber im Orphan-Pages-Report (OUT-099) mit ihrer Quelle.
  - [ ] Ohne Crawl-Analyse zeigt der Filter `Orphan URLs` einen Hinweis „Crawl-Analyse erforderlich".
  - [ ] Include/Exclude-Regeln und robots.txt gelten auch für so entdeckte URLs.
- **Testfixture:** `fixtures/integ/orphans/` (Startseite verlinkt `/linked`, `/orphan` existiert ohne Link) + Mock-GA4/GSC mit Zeilen für `/linked`, `/orphan`, `https://fremd.test/x`.

### INTEG-016 Proxy und eigener Truststore für API-Verbindungen
- **Priorität:** P1
- **Verweis:** siehe CRAWL-122 (führend). Eine eigene Proxy-Einstellung je Integration gibt es nicht.

### INTEG-017 API-Integrationen in Zeitplanung und CLI
- **Priorität:** P1
- **Verweis:** siehe OUT-201 (führend). Geplante Aufgaben wählen ihre Integrationen nach OUT-164.

### INTEG-018 Datenschutz und lokale Datenhaltung
- **Priorität:** P0
- **Beschreibung:** Abgerufene API-Daten und OAuth-Daten verbleiben lokal auf dem Rechner des Nutzers. OpenSpider sendet keine API-Daten, Schlüssel oder Seiteninhalte an Dritte außer an den jeweils vom Nutzer konfigurierten Anbieter. Vor dem ersten KI-Prompt bzw. der ersten Snippet-Ausführung mit externem Netzwerkzugriff wird angezeigt, dass Seiteninhalte an den Anbieter übertragen werden.
- **Eingaben/Konfiguration:** einmalige Bestätigung je KI-Anbieter.
- **Ausgabe:** keine.
- **Akzeptanzkriterien:**
  - [ ] Netzwerkmitschnitt eines Crawls mit Mock-Integrationen zeigt nur Verbindungen zu Fixture-Site, Mock-APIs und (falls aktiviert) Update-Server.
  - [ ] Vor dem ersten KI-Request an einen Anbieter erscheint der Hinweisdialog; ohne Bestätigung wird kein Request gesendet.
- **Testfixture:** Crawl unter Netzwerk-Sandbox mit Allowlist; Testlauf schlägt bei unerwarteten Zielen fehl.

### INTEG-019 Testbarkeit: überschreibbare API-Basis-URLs und Mock-Server
- **Priorität:** P0
- **Beschreibung:** Jede Integration hat eine intern überschreibbare Basis-URL (Umgebungsvariable `OPENSPIDER_API_BASE_<ANBIETER>`), damit Akzeptanztests gegen den Mock-Server `fixture-apis` laufen. Der Mock-Server liefert aufgezeichnete, anonymisierte Antworten für GA4 Data API, Search Analytics, URL Inspection, PSI v5, Majestic, Ahrefs v3, Moz v3, [M4] OpenAI, Gemini, Anthropic, Ollama und OpenRouter (Modul `ai-mock`, Pfade und Szenarien führend in 08, Konventionen) sowie [M5] DataForSEO (Routen unter `/__mock/dataforseo/v3/…`, Kontoprofile und Szenarien führend in D4S-009, übernommen aus 09, Änderungsbedarf X2). Alle Mocks sind Module des Crates `os-testsite` (`docs/ARCHITECTURE.md`); `fixture-apis` ist der Sammelname dieser Module.
- **Eingaben/Konfiguration:** Umgebungsvariablen; nur in Debug-/Testbuilds oder mit explizitem Flag `--allow-api-override` wirksam.
- **Ausgabe:** keine.
- **Akzeptanzkriterien:**
  - [ ] In Release-Builds ohne Flag werden die Variablen ignoriert.
  - [ ] Für jede Integration existiert mindestens ein Erfolgs-, ein Leer-, ein Fehler- (4xx) und ein Drossel-Szenario im Mock; das Drossel-Szenario verwendet die dokumentierte Drosselform des Anbieters (in der Regel HTTP 429, bei DataForSEO HTTP 200 mit Statuscode 40202).
  - [ ] Alle vier Pflichtszenarien sind je Integration über den Header `X-Scenario` erreichbar (zusätzlich zum Query-Parameter, wo vorhanden). [M5] Für DataForSEO gilt die Abbildung Erfolg → `ok`, Leer → `empty`, Fehler (4xx) → `invalid-field-one`, Drossel → `ratelimit-x2`; weitere DFS-Szenarien und die Steuerung über `POST /__control/dfs/scenario` sind in D4S-009 festgelegt.
  - [ ] Alle Akzeptanztests dieses Dokuments laufen offline.
- **Testfixture:** `fixtures/integ/fixture-apis/` (Szenariodaten der `os-testsite`-Module, Szenario-Routing über Query-Parameter bzw. Header `X-Scenario`); Ein Test iteriert über alle Einträge aus INTEG-001 des jeweiligen Ausbaustands und ruft die vier Pflichtszenarien per `X-Scenario` ab.

---

## B Custom Search

> Umsetzungshinweis (Rust): Es gilt der einheitliche Regex-Dialekt aus den Konventionen von 01-crawl.md (`fancy-regex` mit Schrittlimit; Lookaround und Rückverweise, wie sie die Referenz-Doku mit Beispielen wie `(?=.*a)(?=.*b)` vorsieht). Ziel ist Kompatibilität zur verbreiteten Java/PCRE-Syntax.

### INTEG-020 Suchfilter definieren
- **Priorität:** P0
- **Beschreibung:** Der Nutzer legt bis zu 100 Suchfilter an. Jeder Filter hat Name, Bedingung (enthält / enthält nicht), Abfragetyp (Text / Regex), Suchausdruck und Suchbereich. Der Suchausdruck darf mehrzeilig sein (aufklappbares Textfeld), etwa für komplette Tracking-Snippets.
- **Eingaben/Konfiguration:** Tabelle mit Spalten Name, Bedingung, Typ, Ausdruck, Suchbereich (+ XPath-Feld bei Bereich XPath, + Option „Groß-/Kleinschreibung beachten" bei Typ Text); Schaltflächen Hinzufügen, Entfernen, Bulk Add (INTEG-032), Nach oben/Nach unten (INTEG-033).
- **Ausgabe:** je Filter eine Spalte und ein Filter im Custom-Search-Tab (INTEG-031).
- **Akzeptanzkriterien:**
  - [ ] Es lassen sich genau 100 Filter anlegen; der 101. wird mit Meldung „Maximal 100 Suchfilter" abgelehnt.
  - [ ] Ohne Name erhält ein Filter automatisch den Namen `Filter <n>` (n = Position, beginnend bei 1).
  - [ ] Doppelte Namen werden beim Speichern abgelehnt (Spaltennamen müssen eindeutig sein).
  - [ ] Leere Suchausdrücke werden beim Speichern abgelehnt.
  - [ ] Ein ungültiger Regex (z. B. `(abc`) wird im Dialog rot markiert und kann nicht gespeichert werden.
  - [ ] Die Filter werden im Konfigurationsprofil gespeichert und nach Laden identisch wiederhergestellt.
- **Testfixture:** UI-Test mit Skript, das 101 Filter anlegt; `fixtures/integ/search/` für die folgenden Funktionen.

### INTEG-021 Suchbereich „HTML"
- **Priorität:** P0
- **Beschreibung:** Durchsucht das vollständige HTML-Dokument (inkl. Kommentare, Skripte, Attribute) als Zeichenkette.
- **Eingaben/Konfiguration:** Suchbereich = HTML.
- **Ausgabe:** Trefferzahl bzw. Contains/Does Not Contain.
- **Akzeptanzkriterien:**
  - [ ] Ein Text, der nur in einem HTML-Kommentar `<!-- GTM-ABC123 -->` vorkommt, wird gefunden (Treffer 1).
  - [ ] Ein Text, der nur in einem Attribut (`data-sku="X-99"`) vorkommt, wird gefunden.
  - [ ] Die Suche erfolgt auf dem dekodierten Dokument (Zeichensatz gemäß HTTP-Header/Meta), nicht auf Rohbytes: `für` wird in einer ISO-8859-1-Seite gefunden.
  - [ ] HTML-Entities werden im Bereich HTML nicht aufgelöst: Suche nach `&amp;` findet die Entity, Suche nach `&` findet sowohl `&amp;` als auch ein rohes `&`.
- **Testfixture:** `fixtures/integ/search/html.html` (Kommentar, Attribut, Entities) und `latin1.html` (Content-Type `text/html; charset=iso-8859-1`).

### INTEG-022 Suchbereich „Page Text"
- **Priorität:** P0
- **Beschreibung:** Durchsucht nur den sichtbaren Text der Seite ohne Markup, ohne Inhalte von `<script>`, `<style>`, `<noscript>`, `<template>` und ohne Kommentare. Entities sind aufgelöst, Leerraum ist zu einzelnen Leerzeichen normalisiert.
- **Eingaben/Konfiguration:** Suchbereich = Page Text.
- **Ausgabe:** Trefferzahl.
- **Akzeptanzkriterien:**
  - [ ] Text nur im Kommentar oder nur in einem `<script>` wird nicht gefunden (Treffer 0, bei „enthält nicht" → `Does Not Contain`).
  - [ ] `Out&nbsp;of&nbsp;stock` wird mit der Suche `Out of stock` gefunden.
  - [ ] Text, der über Inline-Elemente verteilt ist (`Out <b>of</b> stock`), wird gefunden.
  - [ ] Text in `<title>` und `alt`-Attributen zählt nicht zum Seitentext.
- **Testfixture:** `fixtures/integ/search/pagetext.html`.

### INTEG-023 Suchbereich „Page Text No Anchors"
- **Priorität:** P1
- **Beschreibung:** Wie Page Text, aber Text innerhalb von `<a>`-Elementen wird ausgeschlossen. Typischer Zweck: Seiten ermitteln, auf denen ein Begriff als unverlinkter Fließtext steht – etwa als Kandidaten für neue interne Links. Weil Linktexte (auch aus Menü und Footer) nicht mitzählen, beeinflusst ein seitenweit verlinkter Menüpunkt mit demselben Begriff das Ergebnis nicht.
- **Eingaben/Konfiguration:** Suchbereich = Page Text No Anchors.
- **Ausgabe:** Trefferzahl.
- **Akzeptanzkriterien:**
  - [ ] Seite mit `<a>Laufschuhe</a>` im Menü und ohne weiteres Vorkommen → Treffer 0.
  - [ ] Seite mit Menülink und einmal „Laufschuhe" im Absatz → Treffer 1.
  - [ ] Verschachtelte Elemente innerhalb von `<a>` (`<a><span>Laufschuhe</span></a>`) werden ebenfalls ausgeschlossen.
- **Testfixture:** `fixtures/integ/search/noanchors-a.html`, `noanchors-b.html`.

### INTEG-024 Suchbereich „HTML Head"
- **Priorität:** P1
- **Beschreibung:** Durchsucht nur den Quelltext innerhalb des `<head>`-Elements (so wie der Parser ihn bildet).
- **Eingaben/Konfiguration:** Suchbereich = HTML Head.
- **Ausgabe:** Trefferzahl bzw. Contains/Does Not Contain.
- **Akzeptanzkriterien:**
  - [ ] Ein GTM-Snippet im Head wird gefunden; dasselbe Snippet nur im Body wird nicht gefunden.
  - [ ] Bei Dokumenten ohne explizites `<head>` wird der vom HTML5-Parser implizit erzeugte Head verwendet (Elemente vor dem ersten Body-Inhalt).
  - [ ] Typischer Prüffall: „enthält nicht" + GTM-ID auf 10 Seiten, von denen 3 das Snippet im Head haben → 7 × `Does Not Contain`, 3 × `Contains`.
- **Testfixture:** `fixtures/integ/search/head-gtm/` (10 Seiten).

### INTEG-025 Suchbereich „HTML Body"
- **Priorität:** P1
- **Beschreibung:** Durchsucht den Quelltext (Markup und Text) innerhalb des `<body>`-Elements.
- **Eingaben/Konfiguration:** Suchbereich = HTML Body.
- **Ausgabe:** Trefferzahl.
- **Akzeptanzkriterien:**
  - [ ] Ein `<noscript>`-GTM-iframe im Body wird gefunden; ein Treffer nur im Head nicht.
  - [ ] Markup im Body ist durchsuchbar (Suche nach `class="price"` findet Treffer).
- **Testfixture:** `fixtures/integ/search/body.html`.

### INTEG-026 Suchbereich „XPath"
- **Priorität:** P1
- **Beschreibung:** Ein zusätzlicher XPath-Ausdruck begrenzt die Suche auf die Textinhalte der getroffenen Elemente; alle Treffer werden summiert.
- **Eingaben/Konfiguration:** Suchbereich = XPath, Feld „XPath" (z. B. `//h3`).
- **Ausgabe:** Trefferzahl über alle passenden Elemente.
- **Akzeptanzkriterien:**
  - [ ] XPath `//h3`, Suche `Preis` auf Seite mit `Preis` in zwei h3 und einem p → Treffer 2.
  - [ ] Ein XPath ohne Treffer auf der Seite ergibt 0 (bzw. `Does Not Contain`), kein Fehler.
  - [ ] Ein syntaktisch ungültiger XPath wird im Dialog abgelehnt.
  - [ ] XPath auf Attributknoten (`//img/@alt`) durchsucht die Attributwerte.
- **Testfixture:** `fixtures/integ/search/xpath.html`.

### INTEG-027 Suchbereich „Content Area"
- **Priorität:** P1
- **Beschreibung:** Durchsucht den Text des konfigurierten Inhaltsbereichs, der auch für Wortanzahl und Duplicate-Content genutzt wird (Standard: Body ohne `<nav>`, `<footer>`, `<header>` und `<aside>` (SQ-09/K2-95); konfigurierbar per Ein-/Ausschluss von Elementen, Klassen und IDs; Definition führend in ONPAGE-140).
- **Eingaben/Konfiguration:** Suchbereich = Content Area; Inhaltsbereich-Einstellungen.
- **Ausgabe:** Trefferzahl.
- **Akzeptanzkriterien:**
  - [ ] Text im `<footer>` wird mit Standardeinstellung nicht gefunden.
  - [ ] Nach Ausschluss der ID `#comments` sinkt die Trefferzahl einer Seite, deren einziges Vorkommen in `#comments` liegt, von 1 auf 0.
- **Testfixture:** `fixtures/integ/search/contentarea.html` (Treffer in nav, footer, `#comments`, main).

### INTEG-028 Regex-Semantik
- **Priorität:** P0
- **Beschreibung:** Regex-Filter zählen nicht-überlappende Treffer. Standardmäßig wird Groß-/Kleinschreibung beachtet; `(?i)` schaltet sie ab. Wortgrenzen `\b`, Alternativen `|`, Lookahead/Lookbehind und mehrzeilige Muster werden unterstützt. Ein Zeit-/Schrittlimit pro Seite verhindert katastrophales Backtracking.
- **Eingaben/Konfiguration:** Typ = Regex; globales Limit „Regex-Timeout je Seite" (Standard 2 s).
- **Ausgabe:** Trefferzahl; bei Timeout Zellwert `Regex timeout`.
- **Akzeptanzkriterien:**
  - [ ] Muster `pr` auf Text „pr promotion pre-rendering PR" → 3 Treffer; `\bpr\b` → 1; `(?i)\bpr\b` → 2.
  - [ ] `\bnatural\b|\borganic\b|\bfree\b` zählt jedes Vorkommen jedes Worts.
  - [ ] `(?=.*rot)(?=.*blau)` (mit `(?s)`) liefert für eine Seite mit beiden Wörtern in beliebiger Reihenfolge ≥ 1 Treffer, für eine Seite mit nur einem Wort 0.
  - [ ] Der Punkt `.` matcht standardmäßig keinen Zeilenumbruch, mit `(?s)` schon.
  - [ ] Muster `(a+)+$` auf 50 × `a` + `b` endet nach höchstens 2 s mit `Regex timeout`, der Crawl läuft weiter.
  - [ ] Treffer leerer Länge (z. B. `x*`) werden nicht gezählt bzw. die Zählung terminiert.
- **Testfixture:** `fixtures/integ/search/regex.html`, `regex-redos.html`.

### INTEG-029 Groß-/Kleinschreibung bei Textsuche
- **Priorität:** P0
- **Beschreibung:** Textsuchen ignorieren standardmäßig Groß-/Kleinschreibung; über „Groß-/Kleinschreibung beachten" wird exakt gesucht (z. B. für falsch geschriebene Markennamen oder Akronyme). Der Vergleich ist Unicode-bewusst.
- **Eingaben/Konfiguration:** Checkbox je Textfilter (Standard: aus).
- **Ausgabe:** Trefferzahl.
- **Akzeptanzkriterien:**
  - [ ] Suche `openspider` findet `OpenSpider` ohne Option; mit Option nicht.
  - [ ] Suche `straße` findet `STRASSE` nicht (keine Sonderfaltung), aber `Straße`/`STRAßE` bei ausgeschalteter Option.
  - [ ] Textsuche behandelt Regex-Sonderzeichen wörtlich: `a.b` findet nicht `axb`.
- **Testfixture:** `fixtures/integ/search/case.html`.

### INTEG-030 Rohes vs. gerendertes HTML
- **Priorität:** P0
- **Beschreibung:** Im Textmodus wird das ausgelieferte HTML durchsucht. Im JavaScript-Rendering-Modus wird das gerenderte DOM (serialisiert) durchsucht.
- **Eingaben/Konfiguration:** Rendering-Modus (Bereich Crawl/Rendering).
- **Ausgabe:** Trefferzahl.
- **Akzeptanzkriterien:**
  - [ ] Text, der erst per JS eingefügt wird, ergibt im Textmodus 0 und im Rendering-Modus 1 Treffer.
  - [ ] Text, der per JS entfernt wird, ergibt im Textmodus 1 und im Rendering-Modus 0 Treffer.
- **Testfixture:** `fixtures/integ/search/js-inject.html` (fügt „Injected-Marker" per `DOMContentLoaded` ein, entfernt „Removed-Marker").

### INTEG-031 Custom-Search-Tab und Ergebnisspalten
- **Priorität:** P0
- **Beschreibung:** Tab „Custom Search" mit Basisspalten und je Filter einer Ergebnisspalte. Jeder Filter erscheint als gleichnamiger Filter im Tab und in der rechten Übersicht mit Live-Zählern.
- **Eingaben/Konfiguration:** keine.
- **Ausgabe:** Spalten `Address`, `Content`, `Status Code`, `Status`, danach je Filter `<Filtername>`. Die Spalte mit dem Inhaltstyp heißt in diesem Tab (wie in Custom Extraction, INTEG-053) `Content`; im Custom-JavaScript-Tab (INTEG-071) und im KI-Tab heißt sie `Content Type`. Diese uneinheitliche Benennung wird zur Export-Kompatibilität mit der Referenz beibehalten; die UI zeigt in allen Tabs die gleiche lokalisierte Beschriftung „Inhaltstyp".
- **Akzeptanzkriterien:**
  - [ ] Der CSV-Export des Tabs beginnt mit den Headern `Address,Content,Status Code,Status`; die Spalte `Content` enthält den Content-Type-Wert der Antwort (z. B. `text/html; charset=utf-8`).
  - [ ] „Enthält"-Filter: Zellwert = Anzahl der Treffer (Ganzzahl ≥ 0).
  - [ ] Filter „<Name>" (enthält) zeigt alle URLs mit Trefferzahl ≥ 1.
  - [ ] „Enthält nicht"-Filter: Zellwert = `Contains` oder `Does Not Contain`.
  - [ ] Filter „<Name>" (enthält nicht) zeigt alle URLs mit Wert `Does Not Contain`.
  - [ ] Die Übersicht rechts zeigt je Filter die Anzahl der URLs, die den Filter erfüllen, und aktualisiert sich während des Crawls.
  - [ ] Der Export des Tabs berücksichtigt den aktiven Filter (nur die gefilterten Zeilen).
  - [ ] Nicht durchsuchte URLs (z. B. Bilder, externe URLs) erscheinen nicht im Tab.
- **Testfixture:** `fixtures/integ/search/` komplett (≈ 15 Seiten) mit erwarteter Ergebnistabelle `expected.csv`.

### INTEG-032 Bulk Add
- **Priorität:** P1
- **Beschreibung:** In einem separaten Fenster werden viele Suchausdrücke gleichen Typs zeilenweise eingefügt (z. B. 100 Keywords); jede Zeile wird ein Filter mit gemeinsamer Bedingung, Typ und Suchbereich. Der Name wird aus dem Ausdruck gebildet.
- **Eingaben/Konfiguration:** Mehrzeiliges Textfeld; Bedingung, Typ, Suchbereich, Groß-/Kleinschreibung einmalig wählbar.
- **Ausgabe:** neue Filter in der Tabelle.
- **Akzeptanzkriterien:**
  - [ ] 10 eingefügte Zeilen erzeugen 10 Filter mit Namen = Ausdruck.
  - [ ] Leere Zeilen und Duplikate werden übersprungen (Hinweis mit Anzahl).
  - [ ] Wird das Limit 100 überschritten, werden nur so viele Zeilen übernommen, bis 100 Filter erreicht sind; der Rest wird gemeldet.
  - [ ] CSV/TXT-Datei-Import mit einer Spalte ist möglich.
- **Testfixture:** `fixtures/integ/search/bulk-keywords.txt` (105 Zeilen inkl. 3 leere, 2 Duplikate).

### INTEG-033 Reihenfolge der Filter
- **Priorität:** P2
- **Beschreibung:** Filter können nach oben/unten verschoben werden, ohne sie neu anzulegen. Die Reihenfolge bestimmt die Spaltenreihenfolge in Tab und Export.
- **Eingaben/Konfiguration:** Schaltflächen „Nach oben", „Nach unten".
- **Ausgabe:** Spaltenreihenfolge.
- **Akzeptanzkriterien:**
  - [ ] Nach Verschieben von Filter 3 an Position 1 ist dessen Spalte die erste Ergebnisspalte im Tab und im CSV.
  - [ ] Verschieben ändert keine bereits erhobenen Werte.
- **Testfixture:** UI-Test mit 3 Filtern.

### INTEG-034 Bulk-Export „Inlinks je Suchfilter"
- **Priorität:** P1
- **Verweis:** siehe OUT-060 (führend). Für „Does Not Contain"-Filter umfasst der Inlinks-Export die Inlinks der URLs mit Status `Does Not Contain` (Filtersemantik INTEG-020/031).

### INTEG-035 Geltungsbereich der Suche
- **Priorität:** P0
- **Beschreibung:** Custom Search wird auf alle gecrawlten internen Dokumente mit HTML-Inhalt angewandt, die einen Antwortkörper haben, unabhängig vom Statuscode (also auch 404-Seiten mit Inhalt). Externe URLs, Bilder, CSS, JS und andere Ressourcen werden nicht durchsucht. Suchtreffer werden auch für Seiten erhoben, deren Inhalt durch „Assume pages are HTML" als HTML behandelt wird.
- **Eingaben/Konfiguration:** keine.
- **Ausgabe:** —
- **Akzeptanzkriterien:**
  - [ ] Eine interne 404-Seite mit dem Suchtext liefert Trefferzahl 1.
  - [ ] Eine verlinkte externe Seite, die den Suchtext enthält, erscheint nicht im Custom-Search-Tab.
  - [ ] Eine `.js`-Datei mit dem Suchtext wird nicht durchsucht.
  - [ ] Eine Weiterleitung (301) ohne Body erscheint nicht als Treffer.
- **Testfixture:** `fixtures/integ/search/scope/` (404 mit Body, externe Seite auf `extern.test`, JS-Datei, 301).

---

## C Custom Extraction

> Umsetzungshinweis (Rust): XPath 1.0 z. B. über libxml2-Bindings; XPath 2.0–3.1 erfordert eine eigene oder externe Engine (z. B. die Rust-Crate `xee`). CSS-Selektoren z. B. über `scraper`/`selectors`. Für HTML wird zuerst ein HTML5-konformer Parser (html5ever) verwendet und das Ergebnis als DOM für XPath bereitgestellt; Elementnamen sind dabei klein geschrieben und ohne Namensraum-Präfix adressierbar.

### INTEG-040 Extraktor definieren und Limits
- **Priorität:** P0
- **Beschreibung:** Bis zu 100 Extraktoren mit Name, Modus (XPath / CSS-Selektor / Regex), Ausdruck, ggf. Attribut (nur CSS) und Ausgabeart (nur XPath/CSS). Je URL werden über alle Extraktoren höchstens 1.000 Einzelwerte gespeichert.
- **Eingaben/Konfiguration:** Tabelle Name, Modus, Ausdruck, Attribut, Ausgabeart; Schaltflächen Hinzufügen, Entfernen, Visuell (INTEG-054), Nach oben/unten.
- **Ausgabe:** Spalten im Custom-Extraction-Tab und Internal-Tab.
- **Akzeptanzkriterien:**
  - [ ] Der 101. Extraktor wird abgelehnt mit Meldung „Maximal 100 Extraktoren".
  - [ ] Ohne Name heißt ein Extraktor `Extractor <n>`.
  - [ ] Liefert eine Seite 1.200 Treffer über alle Extraktoren, werden die ersten 1.000 (in Extraktor-Reihenfolge, dann Dokumentreihenfolge) gespeichert, und die URL erhält den Hinweis `Extraction limit reached` in der Spalte `Extraction Warnings`.
  - [ ] Der Name bestimmt den Spaltenkopf im Export.
- **Testfixture:** `fixtures/integ/extract/many-matches.html` (1.200 `<li>`).

### INTEG-041 XPath 1.0
- **Priorität:** P0
- **Beschreibung:** XPath-Ausdrücke wählen Knoten im (rohen oder gerenderten) DOM. Elementknoten werden gemäß Ausgabeart ausgegeben; Attributknoten (`/@attr`) und Textknoten (`/text()`) liefern ihren Wert.
- **Eingaben/Konfiguration:** Modus XPath, Ausdruck, Ausgabeart.
- **Ausgabe:** ein Wert je Treffer.
- **Akzeptanzkriterien:**
  - [ ] `//h1` mit Ausgabe Text auf Seite mit `<h1>Hallo <b>Welt</b></h1>` → `Hallo Welt`.
  - [ ] `//meta[@property='og:title']/@content` → Wert des Attributs.
  - [ ] `//*[@itemtype]/@itemtype` liefert alle Microdata-Typen als einzelne Werte.
  - [ ] `(//iframe/@src)[1]` liefert nur den ersten Treffer.
  - [ ] `//*[@hreflang]/@hreflang` auf einer Seite mit 3 hreflang-Links → 3 Werte.
  - [ ] `//a[starts-with(@href,'mailto:')]/@href` liefert alle Mailto-Links.
  - [ ] Elementnamen werden unabhängig von der Schreibweise im Quelltext klein adressiert (`<H1>` wird von `//h1` gefunden).
  - [ ] Ein Ausdruck ohne Treffer liefert eine leere Zelle, keinen Fehler.
- **Testfixture:** `fixtures/integ/extract/xpath.html` (h1 mit Inline-Element, OG-Tags, Microdata, 3 hreflang, 2 iframes, 2 mailto-Links, Großbuchstaben-Tags).

### INTEG-042 XPath 2.0 / 3.0 / 3.1
- **Priorität:** P1
- **Beschreibung:** Neben XPath 1.0 werden Funktionen und Ausdrücke der XPath-Versionen 2.0 bis 3.1 unterstützt (u. a. `string-join`, `distinct-values`, `matches`, `replace`, `lower-case`, `tokenize`, Sequenzen, `for`-Ausdrücke).
- **Eingaben/Konfiguration:** wie INTEG-041.
- **Ausgabe:** wie INTEG-041 bzw. INTEG-048.
- **Akzeptanzkriterien:**
  - [ ] `string-join(distinct-values(//a[starts-with(@href,'mailto:')]/substring-after(@href,'mailto:')),'; ')` mit Funktionswert liefert eindeutige Adressen, getrennt durch `; `, in einer Zelle.
  - [ ] `lower-case(//h1)` liefert den kleingeschriebenen H1-Text.
  - [ ] `//a[matches(@href,'\.pdf$','i')]/@href` liefert alle PDF-Links unabhängig von der Schreibweise.
  - [ ] `translate(//title,'ABCDEFGHIJKLMNOPQRSTUVWXYZ','abcdefghijklmnopqrstuvwxyz')` funktioniert ebenfalls (XPath-1.0-Weg).
- **Testfixture:** `fixtures/integ/extract/xpath3.html` (Mailto-Duplikate, PDF-Links `.pdf`/`.PDF`).

### INTEG-043 CSS-Selektor mit optionalem Attribut
- **Priorität:** P0
- **Beschreibung:** CSS-Selektoren (Level 3 + gängige Level-4-Pseudoklassen wie `:not()`, `:is()`, `:has()`) wählen Elemente. Ist ein Attributname angegeben, wird dessen Wert statt des Elements ausgegeben.
- **Eingaben/Konfiguration:** Modus CSS, Selektor, optional Attribut, Ausgabeart (bei gesetztem Attribut ignoriert).
- **Ausgabe:** ein Wert je Treffer.
- **Akzeptanzkriterien:**
  - [ ] `span.price` mit Ausgabe Text → alle Preistexte.
  - [ ] `link[rel=canonical]` mit Attribut `href` → Canonical-URL (wie im Quelltext, nicht absolut aufgelöst).
  - [ ] `article:has(> h2)` wird unterstützt.
  - [ ] Attribut, das am Element fehlt → kein Wert für diesen Treffer (keine leere Pseudo-Zelle).
  - [ ] Ungültiger Selektor (`div[`) wird im Dialog abgelehnt.
- **Testfixture:** `fixtures/integ/extract/css.html`.

### INTEG-044 Regex-Extraktion
- **Priorität:** P0
- **Beschreibung:** Regex wird auf dem unveränderten HTML-Quelltext (roh bzw. serialisiert gerendert) ausgeführt, ohne Vorverarbeitung. Der Punkt matcht dabei auch Zeilenumbrüche (DOTALL standardmäßig an). Enthält das Muster Gruppen, wird der Inhalt der ersten Gruppe ausgegeben; ohne Gruppe der Gesamttreffer. Jeder Treffer ist ein eigener Wert. Einsatzfälle: Inhalte außerhalb von Elementen wie Kommentare, Inline-JS, JSON-LD.
- **Eingaben/Konfiguration:** Modus Regex, Ausdruck.
- **Ausgabe:** ein Wert je Treffer.
- **Akzeptanzkriterien:**
  - [ ] `["'](GTM-[A-Z0-9]+)["']` liefert die GTM-ID ohne Anführungszeichen.
  - [ ] `"datePublished":\s*"(.*?)"` liefert das Datum aus JSON-LD.
  - [ ] `<!--(.*?)-->` liefert mehrzeilige Kommentare vollständig (DOTALL).
  - [ ] Muster mit zwei Gruppen `(a)(b)` liefert den Inhalt der Gruppe 1.
  - [ ] Ein Muster ohne Gruppe liefert den Gesamttreffer.
  - [ ] Regex-Timeout je Seite wie INTEG-028 → Zellwert `Regex timeout`.
- **Testfixture:** `fixtures/integ/extract/regex.html` (GTM-Snippet, JSON-LD, mehrzeiliger Kommentar).

### INTEG-045 Ausgabeart „HTML-Element"
- **Priorität:** P0
- **Beschreibung:** Gibt das getroffene Element samt eigenem Tag, Attributen und innerem HTML aus (äußeres HTML).
- **Eingaben/Konfiguration:** Ausgabeart = HTML-Element.
- **Ausgabe:** HTML-Zeichenkette.
- **Akzeptanzkriterien:**
  - [ ] `//div[@id='box']` → `<div id="box"><p>A</p></div>` (Serialisierung des Parsers, Attribute in Quellreihenfolge).
  - [ ] Bei Attribut- oder Textknoten wird der Wert ausgegeben (keine Fehlermeldung).
- **Testfixture:** `fixtures/integ/extract/outputs.html`.

### INTEG-046 Ausgabeart „Inner HTML"
- **Priorität:** P0
- **Beschreibung:** Gibt nur den Inhalt des Elements inkl. Kind-Elementen aus, ohne das Element selbst.
- **Eingaben/Konfiguration:** Ausgabeart = Inner HTML.
- **Ausgabe:** HTML-Zeichenkette.
- **Akzeptanzkriterien:**
  - [ ] `//div[@id='box']` → `<p>A</p>`.
  - [ ] Leeres Element → leerer String (Zelle leer, Treffer zählt aber für die Nummerierung, INTEG-049).
- **Testfixture:** `fixtures/integ/extract/outputs.html`.

### INTEG-047 Ausgabeart „Text"
- **Priorität:** P0
- **Beschreibung:** Gibt den Textinhalt des Elements und aller Nachfahren aus, mit aufgelösten Entities, Leerraum normalisiert (Folgen von Leerraum → ein Leerzeichen, Trim). Inhalte von `<script>`/`<style>` innerhalb des Elements werden ausgelassen.
- **Eingaben/Konfiguration:** Ausgabeart = Text.
- **Ausgabe:** Text.
- **Akzeptanzkriterien:**
  - [ ] `<p>  A&amp;B\n   <i>C</i> </p>` → `A&B C`.
  - [ ] Element mit eingebettetem `<script>` → Skriptinhalt nicht im Ergebnis.
  - [ ] `<br>` erzeugt ein Leerzeichen zwischen den Wörtern (keine Verschmelzung `ZeileEinsZeileZwei`).
- **Testfixture:** `fixtures/integ/extract/outputs.html`.

### INTEG-048 Ausgabeart „Funktionswert"
- **Priorität:** P1
- **Beschreibung:** Der Ausdruck wird als XPath-Ausdruck ausgewertet, dessen Ergebnis ein atomarer Wert ist (Zahl, Zeichenkette, Boolean), und dieser Wert wird ausgegeben. Nur im Modus XPath verfügbar.
- **Eingaben/Konfiguration:** Ausgabeart = Funktionswert.
- **Ausgabe:** ein Wert.
- **Akzeptanzkriterien:**
  - [ ] `count(//h1)` auf einer Seite mit 2 H1 → `2`.
  - [ ] `string-length(//h3[1])` → Länge des ersten h3-Texts.
  - [ ] `boolean(//meta[@name='robots'])` → `true`/`false`.
  - [ ] Ganzzahlige Ergebnisse werden ohne Nachkommastellen ausgegeben (`2`, nicht `2.0`).
  - [ ] Wählt der Nutzer Funktionswert bei einem Ausdruck, der eine Knotenmenge liefert, wird der String-Wert des ersten Knotens ausgegeben und ein Hinweis im Dialog gezeigt.
  - [ ] Im Modus CSS oder Regex ist Funktionswert nicht wählbar.
- **Testfixture:** `fixtures/integ/extract/function.html`.

### INTEG-049 Mehrfachtreffer und nummerierte Spalten
- **Priorität:** P0
- **Beschreibung:** Liefert ein Extraktor auf einer URL mehrere Werte, werden sie in nummerierten Spalten `<Name> 1`, `<Name> 2`, … in Dokumentreihenfolge abgelegt. Die Spaltenzahl im Tab richtet sich nach dem Maximum über alle URLs. Liefert ein Extraktor auf allen URLs höchstens einen Wert, heißt die Spalte `<Name> 1`.
- **Eingaben/Konfiguration:** keine.
- **Ausgabe:** nummerierte Spalten.
- **Akzeptanzkriterien:**
  - [ ] Seite A mit 3 h2, Seite B mit 1 h2, Extraktor „h2" → Spalten `h2 1`, `h2 2`, `h2 3`; Seite B hat nur `h2 1` befüllt.
  - [ ] Reihenfolge der Werte = Reihenfolge im Dokument.
  - [ ] Während des Crawls neu entstehende Spalten (Seite C mit 5 h2) erscheinen live im Tab.
  - [ ] Im Export sind leere Zellen leer, nicht `null`.
- **Testfixture:** `fixtures/integ/extract/multi/` (Seiten mit 1, 3, 5 h2).

### INTEG-050 Kombinierte Ausdrücke (Union mit `|`)
- **Priorität:** P1
- **Beschreibung:** XPath-Unions (`//h2 | //span[@class='comments']`) liefern Werte beider Teilausdrücke in Dokumentreihenfolge, sodass zusammengehörige Werte (z. B. Artikeltitel und Kommentarzahl) nebeneinander in den nummerierten Spalten liegen.
- **Eingaben/Konfiguration:** XPath mit `|`.
- **Ausgabe:** nummerierte Spalten in Dokumentreihenfolge.
- **Akzeptanzkriterien:**
  - [ ] Blogliste mit Titel/Kommentarzahl-Paaren → Spalten wechseln Titel, Zahl, Titel, Zahl … in Dokumentreihenfolge.
  - [ ] Ein Knoten, der von beiden Teilausdrücken getroffen wird, erscheint nur einmal.
- **Testfixture:** `fixtures/integ/extract/blog-list.html`.

### INTEG-051 Syntaxprüfung im Dialog
- **Priorität:** P0
- **Beschreibung:** Jeder Ausdruck wird bei Eingabe geprüft und mit Häkchen (gültig) bzw. rotem Kreuz (ungültig) markiert; der Tooltip nennt die Fehlerstelle. Ungültige Extraktoren können nicht gespeichert werden.
- **Eingaben/Konfiguration:** —
- **Ausgabe:** Validierungsanzeige.
- **Akzeptanzkriterien:**
  - [ ] `//div[` → Kreuz; `//div` → Häkchen; `div > ` (CSS) → Kreuz; `(` (Regex) → Kreuz.
  - [ ] „OK" ist deaktiviert, solange mindestens ein Extraktor ungültig ist.
  - [ ] Die Prüfung erkennt XPath-3.1-Syntax als gültig (`for $x in //a return $x/@href`).
- **Testfixture:** UI-Komponententest.

### INTEG-052 Geltungsbereich der Extraktion
- **Priorität:** P0
- **Beschreibung:** Extraktion läuft nur für interne URLs mit 2xx-Antwort und HTML-Inhalt. Im Textmodus wird das ausgelieferte HTML genutzt, im Rendering-Modus das gerenderte DOM. Ein Element, das nur im gerenderten DOM existiert, wird im Textmodus nicht gefunden.
- **Eingaben/Konfiguration:** Rendering-Modus.
- **Ausgabe:** —
- **Akzeptanzkriterien:**
  - [ ] Interne 404-Seite mit passendem Element → keine Extraktion (Zelle leer, URL nicht im Custom-Extraction-Tab bzw. ohne Werte).
  - [ ] Externe 200-Seite → keine Extraktion.
  - [ ] JS-injiziertes `<div id="late">` wird nur im Rendering-Modus extrahiert.
  - [ ] Interne 200-Seite nach Weiterleitung (301 → 200) wird unter der Ziel-URL extrahiert.
- **Testfixture:** `fixtures/integ/extract/scope/` (404, externe Seite, JS-Inject, Redirect).

### INTEG-053 Custom-Extraction-Tab und Export
- **Priorität:** P0
- **Beschreibung:** Tab „Custom Extraction" mit Basisspalten und je Extraktor dessen Spalte(n). Je Extraktor existiert ein gleichnamiger Filter, der die URLs mit mindestens einem Wert dieses Extraktors zeigt. Werte erscheinen in Echtzeit während des Crawls.
- **Eingaben/Konfiguration:** —
- **Ausgabe:** Spalten `Address`, `Content`, `Status Code`, `Status`, `<Extraktor> 1..n` (Benennung `Content` statt `Content Type` siehe INTEG-031); Filter je Extraktor. Export der Tab-Ansicht über die Export-Engine (OUT-020, führend), Bulk-Exporte der Extraktionen in OUT-061 (führend); dieser Eintrag legt nur Spalten und Spaltenreihenfolge fest, die diese Exporte übernehmen.
- **Akzeptanzkriterien:**
  - [ ] Der CSV-Export beginnt mit den Headern `Address,Content,Status Code,Status`.
  - [ ] Filter „<Extraktor>" zeigt genau die URLs mit mindestens einem nicht-leeren Wert dieses Extraktors (und blendet die Spalten anderer Extraktoren aus).
  - [ ] Ein Export bei aktivem Filter enthält nur die gefilterten URLs.
  - [ ] Werte mit Zeilenumbrüchen werden im CSV korrekt gequotet.
  - [ ] Nach Laden eines gespeicherten Crawls (CRAWL-127) sind alle Extraktionsspalten mit unveränderten Werten vorhanden.
  - [ ] Werte über 32.767 Zeichen werden im XLSX-Export auf 32.767 Zeichen gekürzt (Excel-Zellgrenze), im CSV nicht.
- **Testfixture:** `fixtures/integ/extract/` komplett + Referenzexport `expected-extraction.csv`.

### INTEG-054 Visuelle Extraktion
- **Priorität:** P2
- **Beschreibung:** Ein eingebetteter Browser öffnet eine URL; der Nutzer klickt ein Element an, das hervorgehoben wird. OpenSpider schlägt einen robusten Ausdruck (XPath oder CSS) vor, bietet Alternativen an und zeigt eine Vorschau des extrahierten Werts – getrennt für rohes und gerendertes HTML. Alternativ kann eine Zeile in der Quelltextansicht gewählt und ein Attribut aus einer Auswahlliste bestimmt werden (z. B. `content` eines Meta-Tags). Navigation im eingebetteten Browser per Strg/Cmd+Klick. Der Browser verwendet den User-Agent der aktuell bearbeiteten Konfiguration.
- **Eingaben/Konfiguration:** Browser-Symbol je Extraktor; URL-Feld; Ansicht „Browser / Quelltext / gerendert"; Vorschlagsliste; Ausgabeart; „Übernehmen", „Übernehmen und weiteren Extraktor anlegen".
- **Ausgabe:** befüllter Extraktor.
- **Akzeptanzkriterien:**
  - [ ] Klick auf eine Autorenangabe erzeugt einen Ausdruck, der auf der Testseite genau dieses Element trifft (Trefferzahl im Dialog = 1); die Vorschau zeigt den Autorennamen.
  - [ ] Der Vorschlag wird nach fester Rangfolge gebildet; es gewinnt die erste Stufe, deren Ausdruck auf der Seite **genau ein** Element trifft: (1) eindeutige `id` → `//*[@id='…']`; (2) `itemprop` → `//*[@itemprop='…']`; (3) Element + Klasse(n) → `//span[@class='author']` bzw. `contains(concat(' ',normalize-space(@class),' '),' author ')`; (4) Element + anderes Attribut (`data-*`, `rel`, `name`); (5) kürzester relativer Pfad ab dem nächsten Vorfahren mit eindeutiger ID; (6) absoluter positionsbasierter Pfad als letzte Wahl.
  - [ ] Die Vorschlagsliste zeigt bis zu 5 Alternativen aus unterschiedlichen Stufen, jeweils mit Trefferzahl auf der aktuellen Seite; Ausdrücke mit dynamisch wirkenden IDs/Klassen (≥ 6 aufeinanderfolgende Hex-Ziffern oder Ziffernfolgen ≥ 5, z. B. `css-1a2b3c4d`) werden in Stufe 1/3 übersprungen.
  - [ ] Ist ein Element nur in der gerenderten Ansicht vorhanden (im rohen HTML liefert der vorgeschlagene Ausdruck 0 Treffer), erscheint der Hinweis „benötigt JavaScript-Rendering".
  - [ ] Auswahl einer `<meta property="article:published_time" content="…">`-Zeile und Attribut `content` erzeugt `//meta[@property='article:published_time']/@content`.
  - [ ] `file://`-URLs werden abgelehnt.
  - [ ] Der verwendete User-Agent entspricht dem der gerade bearbeiteten (nicht der zuletzt gespeicherten) Konfiguration.
- **Testfixture:** `fixtures/integ/extract/visual.html` (Autor mit Klasse `author` und ohne ID, ein Element mit `id="price"`, ein Element mit `itemprop="datePublished"`, ein Element mit generierter Klasse `css-9f8e7d6c`, Meta-Zeitstempel, JS-only-Element) + UI-Test über WebDriver; erwartete Vorschläge in `visual-expected.json`.

### INTEG-055 Reihenfolge der Extraktoren
- **Priorität:** P2
- **Beschreibung:** Extraktoren lassen sich verschieben; die Reihenfolge bestimmt die Spaltenreihenfolge in Tab und Export und die Reihenfolge beim Anwenden des 1.000-Werte-Limits.
- **Eingaben/Konfiguration:** „Nach oben", „Nach unten".
- **Ausgabe:** Spaltenreihenfolge.
- **Akzeptanzkriterien:**
  - [ ] Nach Verschieben ist die Spaltenreihenfolge im Tab und CSV entsprechend geändert.
- **Testfixture:** UI-Test mit 3 Extraktoren.

---

## D Custom JavaScript

> Kompatibilitätshinweis: Damit bestehende Snippets der Community ohne Änderung laufen, stellt OpenSpider seine Snippet-Laufzeit-API unter dem eigenen Namen `openspider` **und** zusätzlich unter dem Alias `seoSpider` bereit. Die Funktionsnamen sind Schnittstellenbezeichner (funktionale Kompatibilität), keine übernommenen Texte.

### INTEG-060 Snippet-Verwaltung
- **Priorität:** P1
- **Beschreibung:** Benutzerdefinierte JavaScript-Snippets werden während des Renderns jeder passenden Seite im Browser-Kontext ausgeführt. Je Snippet: Name, Typ (Extraktion / Aktion), JavaScript-Code, Timeout (bei Aktion Pflicht), Inhaltstyp-Filter (Standard: `text/html`). Snippets laufen nur auf internen URLs mit Status 200 und nicht auf PDFs. Voraussetzung ist der JavaScript-Rendering-Modus.
- **Eingaben/Konfiguration:** Tabelle Name, Typ, Inhaltstypen, Timeout, Editor-Schaltfläche „JS"; Hinzufügen, Aus Bibliothek hinzufügen (INTEG-069), Entfernen, Nach oben/unten.
- **Ausgabe:** Spalten im Custom-JavaScript-Tab (nur Extraktions-Snippets).
- **Akzeptanzkriterien:**
  - [ ] Ist Rendering nicht aktiv, zeigt der Dialog eine Warnung, und beim Crawlstart erscheint ein Hinweis; Snippets werden im Textmodus nicht ausgeführt (Zähler „ausgeführte Snippets" = 0, Spalten leer).
  - [ ] Ein Snippet mit Inhaltstyp-Filter `text/html` wird für eine 200-HTML-Seite ausgeführt, nicht für ein Bild, nicht für ein PDF, nicht für eine 404-Seite und nicht für externe URLs (Ausführungsprotokoll je URL).
  - [ ] Der Inhaltstyp-Filter vergleicht den MIME-Typ ohne Parameter und ohne Beachtung der Groß-/Kleinschreibung (`text/html; charset=UTF-8` passt zu `text/html`); mehrere Typen sind kommagetrennt möglich; `*` passt auf jeden Typ außer PDF.
  - [ ] Mehrere Snippets laufen pro Seite sequenziell in Tabellenreihenfolge (Protokoll-Zeitstempel: Ende Snippet 1 < Start Snippet 2); wirft ein Snippet eine Exception, laufen die nachfolgenden Snippets dieser Seite trotzdem.
  - [ ] Reihenfolge der Snippets bestimmt die Spaltenreihenfolge.
  - [ ] Es lassen sich genau 100 Snippets anlegen; das 101. wird mit der Meldung „Maximal 100 Snippets" abgelehnt.
  - [ ] Ohne Name erhält ein Snippet den Namen `Snippet <n>`; doppelte Namen werden beim Speichern abgelehnt.
- **Testfixture:** `fixtures/integ/js/` mit Seite `page.html`, `missing.html` (404), `doc.pdf`, Bild, Seite mit `Content-Type: text/html; charset=UTF-8`; Snippets `first.js`, `throws.js`, `third.js`.

### INTEG-061 Extraktions-Snippets
- **Priorität:** P1
- **Beschreibung:** Extraktions-Snippets geben Daten an OpenSpider zurück. Der Code wird in eine asynchrone, sofort ausgeführte Funktion eingebettet; er muss das Ergebnis mit `return` zurückgeben (auch ein Promise). Ein einzelner Wert ergibt eine Spalte `<Name> 1`; ein Array ergibt nummerierte Spalten `<Name> 1..n`. Während ein Extraktions-Snippet läuft, werden keine weiteren Seitenressourcen mehr geladen. Überschreitet ein Snippet das Zeitlimit (Standard: Rendering-Timeout), wird abgebrochen.
- **Eingaben/Konfiguration:** Typ = Extraktion; Code.
- **Ausgabe:** Werte in Spalten; Fehler in Spalte `<Name> Error`.
- **Akzeptanzkriterien:**
  - [ ] `return openspider.data(document.querySelectorAll('h2').length);` → Zahl in `<Name> 1`.
  - [ ] `return seoSpider.data([...document.querySelectorAll('h2')].map(e=>e.textContent));` → 3 Spalten bei 3 h2.
  - [ ] Snippet ohne `return` → Zelle leer und `<Name> Error` = `Snippet returned no value`.
  - [ ] Asynchrones Snippet (`await new Promise(r=>setTimeout(r,500))`) liefert seinen Wert.
  - [ ] Snippet mit Endlosschleife/Hängen wird nach dem Timeout abgebrochen; `<Name> Error` = `Timeout`; die nächste URL wird normal verarbeitet.
  - [ ] Eine ungefangene Exception erscheint als Fehlermeldung in `<Name> Error`.
  - [ ] Objekte werden als JSON-String ausgegeben.
- **Testfixture:** `fixtures/integ/js/h2.html` + Snippets `count.js`, `list.js`, `noreturn.js`, `async.js`, `hang.js`, `throw.js`.

### INTEG-062 Aktions-Snippets
- **Priorität:** P1
- **Beschreibung:** Aktions-Snippets geben keine Daten zurück, sondern verändern die Seite (Scrollen für Infinite Scroll, Mouseover auslösen, Cookie-Banner wegklicken). Sie haben ein Pflicht-Timeout in Sekunden, während dessen Ressourcen weiter laden. Alle Aktions-Snippets laufen vor allen Extraktions-Snippets; bei mehreren Aktions-Snippets gilt das größte Timeout als Wartezeit, bevor das DOM übernommen wird. Die Auswirkungen fließen in Link-Erkennung und gerendertes HTML ein.
- **Eingaben/Konfiguration:** Typ = Aktion; Timeout (1–120 s).
- **Ausgabe:** keine Spalte; Auswirkungen auf gerendertes HTML, gefundene Links, Extraktion.
- **Akzeptanzkriterien:**
  - [ ] Ein Scroll-Snippet auf einer Infinite-Scroll-Testseite führt dazu, dass Links zu Seite 2 und 3 entdeckt werden; ohne Snippet nur Seite 1.
  - [ ] Aktions-Snippets erscheinen nicht im Custom-JavaScript-Tab.
  - [ ] Bei zwei Aktions-Snippets mit 3 s und 8 s Timeout wird mindestens 8 s gewartet; nicht 11 s.
  - [ ] Ein Extraktions-Snippet, das nach einem Mouseover-Aktions-Snippet läuft, sieht den per Mouseover eingeblendeten Inhalt.
  - [ ] Ohne Timeout kann ein Aktions-Snippet nicht gespeichert werden.
- **Testfixture:** `fixtures/integ/js/infinite-scroll.html` (lädt beim Scrollen Links zu `/p2`, `/p3`), `hover.html` (Tooltip-Inhalt bei `mouseover`).

### INTEG-063 Laufzeit-API: Rückgabe von Daten und Fehlern
- **Priorität:** P1
- **Beschreibung:** Das globale Objekt bietet `data(wert | array)` zur Rückgabe von Ergebnissen und `error(meldung)` zur Rückgabe einer Fehlermeldung, die in der Fehlerspalte erscheint.
- **Eingaben/Konfiguration:** —
- **Ausgabe:** Wert- bzw. Fehlerspalte.
- **Akzeptanzkriterien:**
  - [ ] `return openspider.error('kein Preis');` → Wertspalte leer, `<Name> Error` = `kein Preis`.
  - [ ] `data(null)`/`data(undefined)` → leere Zelle ohne Fehler.
  - [ ] `data([])` → keine Spalte befüllt, kein Fehler.
  - [ ] Beide Namensräume `openspider` und `seoSpider` verhalten sich identisch.
- **Testfixture:** Snippets `error.js`, `null.js`, `empty.js`.

### INTEG-064 Laufzeit-API: Text in Datei speichern
- **Priorität:** P2
- **Beschreibung:** `saveText(text, pfad, anhängen)` schreibt Text in eine Datei; fehlende Verzeichnisse werden angelegt; mit `anhängen = true` wird angehängt statt überschrieben. Pfade sind auf ein vom Nutzer freigegebenes Ausgabeverzeichnis beschränkt (INTEG-072).
- **Eingaben/Konfiguration:** Ausgabeverzeichnis für Snippets (Einstellung).
- **Ausgabe:** Datei im Ausgabeverzeichnis; Rückgabe-Promise.
- **Akzeptanzkriterien:**
  - [ ] Zwei Seiten rufen `saveText(location.href+'\n','urls.txt',true)` auf → Datei enthält 2 Zeilen.
  - [ ] Mit `anhängen = false` enthält die Datei nur den letzten Inhalt.
  - [ ] Pfad `../../etc/x` oder ein absoluter Pfad außerhalb des Ausgabeverzeichnisses wird abgelehnt (Fehler in `<Name> Error`).
  - [ ] Nicht existierende Unterverzeichnisse (`a/b/c.txt`) werden angelegt.
- **Testfixture:** Snippet `savetext.js` auf 2 Seiten; temporäres Ausgabeverzeichnis.

### INTEG-065 Laufzeit-API: URLs herunterladen
- **Priorität:** P2
- **Beschreibung:** `saveUrls(urls, zielordner)` lädt die angegebenen URLs (z. B. Bilder, PDFs) herunter und legt sie in einer Verzeichnisstruktur ab, die dem URL-Pfad entspricht (`<zielordner>/<host>/<pfad>`).
- **Eingaben/Konfiguration:** Ausgabeverzeichnis.
- **Ausgabe:** Dateien.
- **Akzeptanzkriterien:**
  - [ ] `saveUrls(['http://integ.test:8080/img/a/b.png'],'imgs')` → Datei `imgs/integ.test/img/a/b.png` mit identischem Inhalt (SHA-256-Vergleich).
  - [ ] Downloads respektieren Crawl-Geschwindigkeitslimit und User-Agent.
  - [ ] Eine 404-URL erzeugt keine Datei und einen Fehlereintrag.
- **Testfixture:** `fixtures/integ/js/images.html` mit 3 Bildern, eins davon 404.

### INTEG-066 Laufzeit-API: externe Skripte laden
- **Priorität:** P2
- **Beschreibung:** `loadScript(url)` lädt eine externe JavaScript-Bibliothek asynchron in den Seitenkontext und gibt ein Promise zurück, das nach dem Laden erfüllt wird.
- **Eingaben/Konfiguration:** —
- **Ausgabe:** —
- **Akzeptanzkriterien:**
  - [ ] `await openspider.loadScript('http://integ.test:8080/lib/x.js'); return openspider.data(window.X_LOADED);` → `true`.
  - [ ] Ein nicht erreichbares Skript führt zu einem abgelehnten Promise und einer Fehlermeldung.
- **Testfixture:** `fixtures/integ/js/lib/x.js`.

### INTEG-067 Chrome-Konsolen-Hilfsfunktionen
- **Priorität:** P2
- **Beschreibung:** Snippets können die Konsolen-Hilfsfunktionen der Chrome DevTools nutzen, insbesondere `getEventListeners(element)` (z. B. um Elemente mit Klick-Handlern ohne echten Link zu finden).
- **Eingaben/Konfiguration:** —
- **Ausgabe:** —
- **Akzeptanzkriterien:**
  - [ ] Snippet, das die Zahl der Elemente mit `click`-Listenern zählt, liefert auf der Testseite `2`.
- **Testfixture:** `fixtures/integ/js/listeners.html` (2 Elemente mit `addEventListener('click',…)`).

### INTEG-068 Snippet-Tester und Debugging im externen Browser
- **Priorität:** P2
- **Beschreibung:** Ein Editor mit Syntaxhervorhebung erlaubt das Testen eines Snippets gegen eine eingegebene URL; Ergebnis bzw. Fehler erscheinen in einem Testbereich. Über „Externer Browser" wird die Seite in einem sichtbaren Chrome mit geöffneten DevTools und injiziertem Snippet geöffnet, um es zu debuggen.
- **Eingaben/Konfiguration:** URL-Feld, „Testen", „Externer Browser".
- **Ausgabe:** Testergebnis (Werte, Fehler inkl. Stacktrace, Laufzeit in ms).
- **Akzeptanzkriterien:**
  - [ ] Test eines Snippets mit `fetch` auf eine nicht erreichbare URL zeigt eine `TypeError`-Meldung im Testbereich.
  - [ ] Testergebnis entspricht dem Wert, den der Crawl für dieselbe URL liefert.
  - [ ] „Externer Browser" öffnet die Seite mit DevTools; der Snippet-Code ist in den Quellen als eigene Datei sichtbar.
- **Testfixture:** `fixtures/integ/js/h2.html`; Snippet mit absichtlichem Fehler.

### INTEG-069 Mitgelieferte Snippet-Bibliothek
- **Priorität:** P2
- **Beschreibung:** OpenSpider liefert eine eigene, unter Apache-2.0 stehende Bibliothek mit kommentierten Vorlagen. Die Referenz enthält neben eigenen Vorlagen auch Community-Snippets, die an bestimmte Drittdienste gebunden sind (u. a. Passagen-Embeddings, ein Index-Monitoring-Dienst, ein Dienst für „verwandte Fragen", Cookie-Pop-ups, Gemini-Aufrufe). OpenSpider deckt diese Anwendungsfälle mit **anbieterneutralen** Vorlagen ab; markengebundene Drittdienst-Snippets werden bewusst nicht mitgeliefert **(OpenSpider-Abweichung)**, können aber über die Benutzerbibliothek (INTEG-070) importiert werden. Vorlagen mit API-Aufrufen enthalten einen Platzhalter `YOUR_API_KEY`.
- **Eingaben/Konfiguration:** „Aus Bibliothek hinzufügen" mit Reitern „System" und „Benutzer".
- **Ausgabe:** neues Snippet.
- **Akzeptanzkriterien:**
  - [ ] Der Reiter „System" enthält mindestens diese 14 Vorlagen (ID → Typ): `infinite-scroll` (Aktion), `trigger-mouseover` (Aktion), `accept-cookie-banner` (Aktion, klickt Schaltflächen gängiger Consent-Frameworks anhand konfigurierbarer Selektorliste), `download-images` (Extraktion, nutzt `saveUrls`), `click-elements-without-link` (Extraktion, nutzt `getEventListeners`), `llm-alt-text-openai`, `llm-alt-text-gemini`, `llm-sentiment-intent-language-openai`, `llm-sentiment-intent-language-gemini` (Extraktion), `page-embedding-openai`, `page-embedding-gemini` (Extraktion, ein Vektor je Seite), `passage-embeddings` (Extraktion, siehe nächstes Kriterium), `rest-api-template` (Extraktion, generische GET/POST-Abfrage eines beliebigen Dienstes je URL), `index-status-service-template` (Extraktion, generische Abfrage eines externen Index-Monitoring-Dienstes mit konfigurierbarer Endpunkt-URL und Antwortfeld).
  - [ ] `passage-embeddings` zerlegt den Hauptinhalt in Passagen (Absätze bzw. Abschnitte unter Überschriften, max. 1.000 Zeichen je Passage, konfigurierbar oben im Code) und liefert ein Array von Objekten `{passage, heading, vector}`; auf `fixtures/integ/js/passages.html` (3 Abschnitte) entstehen 3 Werte.
  - [ ] Jede Vorlage beginnt mit einem Kommentarblock mit den Feldern `Zweck`, `Typ`, `Anzupassen`, `Benötigt` (z. B. Rendering, API-Schlüssel) – automatisch per Regex geprüft.
  - [ ] Die Scroll-Vorlage besteht den Test aus INTEG-062 ohne Änderung; die Cookie-Banner-Vorlage schließt das Banner der Fixture `cookie-banner.html` (Element danach `display:none` bzw. entfernt).
  - [ ] Vorlagen mit Platzhalter `YOUR_API_KEY` warnen beim Crawlstart, wenn der Platzhalter nicht ersetzt wurde (Warnung nennt Snippet-Namen).
- **Testfixture:** automatischer Test jeder Systemvorlage gegen `fixtures/integ/js/` (inkl. `passages.html`, `cookie-banner.html`); LLM-, Embedding- und REST-Vorlagen gegen Mock-Endpunkte von `fixture-apis`.

### INTEG-070 Benutzer-Bibliothek, Import/Export
- **Priorität:** P2
- **Beschreibung:** Eigene Snippets können in einer Benutzerbibliothek gespeichert (Speichern-Symbol) und als JSON exportiert/importiert werden. Beim Export wird gewarnt, dass Schlüssel im Code mitexportiert werden.
- **Eingaben/Konfiguration:** Speichern, Exportieren, Importieren.
- **Ausgabe:** JSON-Datei (Felder: name, type, contentTypes, timeout, code, version).
- **Akzeptanzkriterien:**
  - [ ] Export → Import auf anderer Installation ergibt identische Snippets (Roundtrip-Vergleich).
  - [ ] Enthält ein Snippet eine Zeichenkette, die wie ein Schlüssel aussieht (z. B. `sk-…`, `AIza…`), zeigt der Export eine Warnung.
  - [ ] Ungültiges JSON beim Import → Fehlermeldung, keine Teilübernahme.
- **Testfixture:** `fixtures/integ/js/library-export.json`, `library-broken.json`.

### INTEG-071 Custom-JavaScript-Tab
- **Priorität:** P1
- **Beschreibung:** Tab „Custom JavaScript" mit Basisspalten und je Extraktions-Snippet dessen Wert- und Fehlerspalten. Je Snippet ein gleichnamiger Filter.
- **Eingaben/Konfiguration:** —
- **Ausgabe:** `Address`, `Content Type`, `Status Code`, `Status`, `<Snippet> 1..n`, `<Snippet> Error`.
- **Akzeptanzkriterien:**
  - [ ] Filter „<Snippet>" zeigt alle URLs, auf denen das Snippet ausgeführt wurde, mit dessen Spalten.
  - [ ] Aktions-Snippets haben weder Spalte noch Filter.
  - [ ] Spalten erscheinen zusätzlich im Internal-Tab (INTEG-013).
- **Testfixture:** `fixtures/integ/js/` mit 1 Extraktions- und 1 Aktions-Snippet.

### INTEG-072 Sicherheitsgrenzen für Snippets
- **Priorität:** P1
- **Beschreibung:** Snippets laufen ausschließlich im isolierten Render-Browser-Kontext ohne Zugriff auf das Dateisystem außer über `saveText`/`saveUrls` im freigegebenen Ausgabeverzeichnis. Snippets dürfen Netzwerkanfragen stellen (z. B. an LLM-APIs); beim ersten Crawl mit Snippets, die `fetch`/`XMLHttpRequest` enthalten, wird darauf hingewiesen (INTEG-018). Importierte Snippets aus Dateien werden vor der ersten Ausführung angezeigt und müssen bestätigt werden.
- **Eingaben/Konfiguration:** Ausgabeverzeichnis; Bestätigungsdialog.
- **Ausgabe:** —
- **Akzeptanzkriterien:**
  - [ ] Pfad-Traversal (`saveText(x,'../../x')`) wird abgelehnt.
  - [ ] Ein importiertes Snippet wird vor Bestätigung nicht ausgeführt.
  - [ ] Snippets haben keinen Zugriff auf Node-/Tauri-APIs (`window.__TAURI__` ist im Render-Kontext `undefined`).
- **Testfixture:** Snippets `traversal.js`, `tauri-probe.js`.

---

## E Custom Link Positions

### INTEG-080 Klassifikation der Link-Position
- **Priorität:** P1
- **Beschreibung:** Für jeden gefundenen Link wird der XPath-Pfad des `<a>`-Elements gespeichert (z. B. `/html/body/div[2]/nav/ul/li[3]/a`) und anhand einer geordneten Regelliste einer Position zugeordnet (z. B. Navigation, Header, Sidebar, Footer, Content). Eine Regel besteht aus Positionsname und einem oder mehreren Teilstrings; die erste Regel (von oben), deren Teilstring im Pfad vorkommt, gewinnt. Der Pfad enthält zur besseren Unterscheidung auch IDs und Klassen der Vorfahren (z. B. `div[@id='sidebar']`).
- **Eingaben/Konfiguration:** Regelliste (siehe INTEG-081/082).
- **Ausgabe:** Spalten `Link Path` und `Link Position` in Inlinks/Outlinks (INTEG-084).
- **Akzeptanzkriterien:**
  - [ ] Link innerhalb `<nav>` → `Navigation`.
  - [ ] Link innerhalb `<footer>` → `Footer`.
  - [ ] Link innerhalb `<aside>` oder eines Elements mit Klasse/ID, die `sidebar` enthält → `Sidebar`.
  - [ ] Link innerhalb `<header>` (aber nicht in `<nav>`) → `Header`.
  - [ ] Link im `<main>`-Fließtext → `Content`.
  - [ ] Ein Link in `<footer><nav>…</nav></footer>` wird gemäß Regelreihenfolge zugeordnet (Standard: Navigation steht vor Footer → `Navigation`).
  - [ ] Teilstring-Vergleich ist case-sensitiv auf dem gespeicherten Pfad (Elementnamen klein).
- **Testfixture:** `fixtures/integ/linkpos/page.html` mit Links in nav, header, aside, `div#sidebar`, footer, footer>nav, main.

### INTEG-081 Standardregeln
- **Priorität:** P1
- **Beschreibung:** OpenSpider liefert eine Standard-Regelliste in folgender Reihenfolge: `Navigation` (`nav`, `menu`), `Header` (`header`), `Aside` (`aside`), `Sidebar` (`sidebar`), `Footer` (`footer`), `Content` (leerer Teilstring = passt immer). `aside` ergibt wie in der Referenz die Position `Aside` (K2-87, SQ-13d; vorher `Sidebar`; Beleg Referenz `parity-m2-links` `positions.html`). `Content` muss als Auffangregel immer letzte Position sein.
- **Eingaben/Konfiguration:** „Auf Standard zurücksetzen".
- **Ausgabe:** —
- **Akzeptanzkriterien:**
  - [ ] Frische Installation zeigt genau diese Regeln in dieser Reihenfolge.
  - [ ] `Content` kann nicht über eine andere Regel verschoben werden (Schaltfläche deaktiviert bzw. Hinweis).
  - [ ] „Zurücksetzen" stellt die Standardliste wieder her.
  - [ ] Link in `<aside>` → Position `Aside` (Werteliste OUT-031).
- **Testfixture:** UI-Test; Fixture aus INTEG-080.

### INTEG-082 Eigene Regeln und Reihenfolge
- **Priorität:** P1
- **Beschreibung:** Nutzer können Regeln hinzufügen (z. B. Position `Mega Menu` mit Teilstring `mega-menu`), Teilstrings ergänzen und Regeln umsortieren, um Fehlklassifikationen zu korrigieren.
- **Eingaben/Konfiguration:** Hinzufügen, Entfernen, Nach oben/unten.
- **Ausgabe:** —
- **Akzeptanzkriterien:**
  - [ ] Neue Regel `Breadcrumb` mit Teilstring `breadcrumb` oberhalb von `Content` → Links in `<ol class="breadcrumb">` erhalten `Breadcrumb`.
  - [ ] Änderungen der Regeln nach dem Crawl können ohne Neucrawl angewendet werden (Positionen werden aus gespeicherten Pfaden neu berechnet).
- **Testfixture:** `fixtures/integ/linkpos/breadcrumb.html`.

### INTEG-083 Funktion abschaltbar
- **Priorität:** P2
- **Beschreibung:** Die Speicherung der Link-Pfade und die Positionsermittlung können abgeschaltet werden, um Speicher zu sparen und den Crawl zu beschleunigen.
- **Eingaben/Konfiguration:** Checkbox „Link-Positionen speichern" (Standard: an). Konfigurationsschlüssel `link_positions.store` (bool, Standard `true`; CLI `--set link_positions.store=false`) **(OS-Festlegung)**.
- **Ausgabe:** Spalten `Link Path`/`Link Position` leer.
- **Umfang (OS-Festlegung):** betrifft die Linkzeilen (Inlinks/Outlinks und Link-Exporte), nicht die Bild-Instanzen (ONPAGE-172). Für einen ohne Pfade gespeicherten Crawl hat die Neuzuordnung nach INTEG-082 keine Pfade und ändert nichts; Einschalten nach dem Crawl erfordert einen Neucrawl.
- **Akzeptanzkriterien:**
  - [ ] Bei deaktivierter Option sind beide Spalten leer und der Speicherumfang der Crawl-Datei kleiner als mit Option (Messung auf Fixture mit 1.000 Links).
  - [ ] Filter, die von Link-Positionen abhängen (z. B. „nur Content-Links"), zeigen einen Hinweis.
- **Testfixture:** `fixtures/integ/linkpos/many-links.html` (1.000 Links).

### INTEG-084 Ausgabe in Inlinks/Outlinks
- **Priorität:** P1
- **Beschreibung:** In den unteren Tabs Inlinks und Outlinks sowie in den zugehörigen Bulk-Exporten erscheinen die Spalten `Link Path` und `Link Position`. Nach Position kann gefiltert werden.
- **Eingaben/Konfiguration:** —
- **Ausgabe:** Spalten, Filter-Dropdown „Link Position".
- **Akzeptanzkriterien:**
  - [ ] Bulk-Export „All Inlinks" enthält beide Spalten.
  - [ ] Der Filter „Link Position = Content" in Inlinks zeigt nur Content-Links.
- **Testfixture:** Fixture aus INTEG-080.

---

## F Google Analytics 4

> API: Google Analytics Data API (GA4) `runReport`; Kontenliste über die Admin API (`accountSummaries`). Umsetzung über OAuth gemäß INTEG-003.

### INTEG-090 Verbindung und Auswahl von Konto, Property, Datenstream
- **Priorität:** P1
- **Beschreibung:** Nach der Google-Anmeldung werden Konto, Property und Web-Datenstream ausgewählt. Die Auswahl des Datenstreams schränkt die Daten auf dessen Hostnamen ein (Filter auf `hostName`), damit Daten anderer Streams/Domains nicht vermischt werden. Option „Alle Datenstreams".
- **Eingaben/Konfiguration:** Dropdowns Konto → Property → Datenstream; Suchfeld bei vielen Properties.
- **Ausgabe:** Status `Verbunden (<Konto> / <Property> / <Stream>)`.
- **Akzeptanzkriterien:**
  - [ ] Die Dropdowns sind abhängig: Property-Liste zeigt nur Properties des gewählten Kontos.
  - [ ] Bei Auswahl eines Datenstreams mit Domain `integ.test` enthalten Anfragen einen Hostnamen-Filter; Zeilen anderer Hosts werden nicht zugeordnet.
  - [ ] Ein Konto ohne GA4-Property zeigt „Keine GA4-Properties gefunden".
  - [ ] Die Auswahl wird im Profil gespeichert (Property-ID, Stream-ID) und bei erneuter Verbindung vorausgewählt.
- **Testfixture:** Mock `accountSummaries` (2 Konten, 3 Properties, 2 Streams) + Mock `runReport`.

### INTEG-091 Zeitraum
- **Priorität:** P1
- **Beschreibung:** Der Abfragezeitraum ist standardmäßig „letzte 30 Tage" (bis einschließlich gestern) und kann über Vorgaben (7, 14, 28, 30, 90 Tage, 12 Monate) oder ein freies Start-/Enddatum gewählt werden. Optional ein Vergleichszeitraum ist nicht Teil der Parität (P2-Erweiterung, nicht spezifiziert).
- **Eingaben/Konfiguration:** Zeitraum-Auswahl.
- **Ausgabe:** Zeitraum wird im Tab-Hinweis und im Export-Dateikopf (Metadaten-Zeile in XLSX) angezeigt.
- **Akzeptanzkriterien:**
  - [ ] Standardanfrage enthält `startDate = heute−30`, `endDate = heute−1` (Zeitzone der Property) **(abgeleitet)**.
  - [ ] Ein Enddatum vor dem Startdatum wird vor dem Senden abgelehnt; ein Enddatum in der Zukunft wird auf „heute" begrenzt.
  - [ ] Es gibt keine clientseitige Untergrenze für das Startdatum; lehnt die API einen Zeitraum ab (HTTP 400), wird deren Meldung im Status angezeigt, und es werden keine GA4-Werte gesetzt. (Eine feste Untergrenze ist in der Data-API-Doku nicht belegt – **Annahme, zu verifizieren**.)
- **Testfixture:** Mock `runReport` protokolliert Request-Bodies; Testuhr auf festes Datum gesetzt; Szenario `400-invalid-date-range`.

### INTEG-092 Metrikauswahl
- **Priorität:** P1
- **Beschreibung:** Standardmäßig werden 7 Metriken abgefragt: Sessions, Engaged Sessions, Engagement Rate, Views, Conversions, Event Count, Total Revenue. Google hat „Conversions" in der GA4-Oberfläche in „Key Events" umbenannt (API-Name `keyEvents`; `conversions` ist abgekündigt). OpenSpider fragt den aktuellen API-Namen ab, behält aber den **kanonischen Export-Header `GA4 Conversions`** zur Kompatibilität mit bestehenden Vorlagen; die UI darf die Spalte als „Key Events" beschriften. Aus einem Katalog von mindestens 65 GA4-Metriken (Nutzer, Sitzungen, Engagement, E-Commerce, Werbung) können weitere gewählt werden. Die Data API erlaubt max. 10 Metriken je Anfrage; bei mehr Metriken werden mehrere Anfragen gestellt und über den Dimensionswert (Seiten-URL bzw. Landingpage) zusammengeführt. Inkompatible Kombinationen werden bereits bei der Auswahl verhindert (INTEG-101).
- **Eingaben/Konfiguration:** Metrik-Baum mit Kategorien und Suchfeld; „Standard wiederherstellen".
- **Ausgabe:** je Metrik eine Spalte `GA4 <Metrikname>` im Analytics- und Internal-Tab; Metrikname = englischer Anzeigename aus den GA4-Metadaten (z. B. `GA4 Engaged Sessions`), Ausnahme `GA4 Conversions` für `keyEvents`.
- **Akzeptanzkriterien:**
  - [ ] Ohne Änderung erscheinen genau die 7 Standardspalten in dieser Reihenfolge: `GA4 Sessions`, `GA4 Engaged Sessions`, `GA4 Engagement Rate`, `GA4 Views`, `GA4 Conversions`, `GA4 Event Count`, `GA4 Total Revenue`; der Request enthält dafür die API-Namen `sessions`, `engagedSessions`, `engagementRate`, `screenPageViews`, `keyEvents`, `eventCount`, `totalRevenue`.
  - [ ] Bei Auswahl von 15 Metriken werden pro Seitenbereich genau 2 Anfragen gestellt (10 + 5). Zusammenführung: Für jede URL stammen alle 15 Werte aus den Zeilen mit identischem Dimensionswert; eine URL, die nur in einer der beiden Antworten vorkommt, erhält nur die Werte dieser Antwort, die übrigen Zellen bleiben leer (nicht `0`).
  - [ ] Quotienten-Metriken (Engagement Rate, Bounce Rate) werden in der UI als Prozent mit 2 Nachkommastellen angezeigt (`0.6234` → `62,34 %`) und im Export standardmäßig als Dezimalzahl (`0.6234`) ausgegeben; mit Exportoption „Prozentwerte als Prozent" als `62.34`.
  - [ ] Währungsmetriken werden in der Property-Währung (Feld `currencyCode` der Property) mit 2 Nachkommastellen ausgegeben; die Währung steht im Spaltentooltip.
  - [ ] Eine von Google abgekündigte Metrik (API-Fehler 400 mit Hinweis auf unbekannte Metrik) wird im Status gemeldet und aus der Anfrage entfernt; die Anfrage wird ohne sie wiederholt, die übrigen Metriken werden geladen.
  - [ ] Der Metrik-Baum wird aus dem Metadaten-Endpunkt der Property (`getMetadata`) befüllt; benutzerdefinierte Metriken der Property erscheinen in einer eigenen Kategorie „Custom".
- **Testfixture:** Mock `runReport` mit Metadaten-Endpunkt (inkl. 1 Custom-Metrik); Szenario mit unbekannter Metrik; Szenario „15 Metriken" mit einer URL, die nur in Antwort 2 vorkommt.

### INTEG-093 Seitendimension (vollständige URL vs. Landingpage)
- **Priorität:** P1
- **Beschreibung:** Die Metriken werden nach einer Seitendimension gruppiert: „Seiten-URL" (vollständige URL inkl. Protokoll, Host und Query, GA4-Dimension `pageLocation`) oder „Landingpage" (`landingPagePlusQueryString`, Pfad + Query, kombiniert mit dem Host des Datenstreams). Die Wahl kann je Metrik getroffen werden (z. B. Sessions nach Landingpage, Views nach Seiten-URL).
- **Eingaben/Konfiguration:** Dimension je Metrik bzw. global.
- **Ausgabe:** Spaltenname mit Suffix bei Landingpage: `GA4 Sessions (Landing Page)`.
- **Akzeptanzkriterien:**
  - [ ] Bei Landingpage-Dimension werden Pfade mit dem Stream-Host zu absoluten URLs ergänzt und gemäß INTEG-014 abgeglichen.
  - [ ] Der GA4-Sonderwert `(not set)` wird keiner URL zugeordnet.
  - [ ] Bei `pageLocation` werden Zeilen mit fremdem Host nicht zugeordnet.
  - [ ] Werden beide Dimensionen gemischt, erscheinen beide Spaltensätze unterscheidbar.
- **Testfixture:** Mock-Report mit `(not set)`, fremdem Host, Pfad-Zeilen.

### INTEG-094 Dimensionsfilter
- **Priorität:** P1
- **Beschreibung:** Zusätzliche Filter auf GA4-Dimensionen schränken die Daten ein, etwa „Session default channel group = Organic Search" oder „First user source = google". Operatoren: ist gleich, enthält, beginnt mit, endet mit, Regex, ist nicht (Negation). Mehrere Filter werden mit UND verknüpft.
- **Eingaben/Konfiguration:** Filterliste: Dimension (Dropdown, durchsuchbar), Operator, Wert(e).
- **Ausgabe:** Hinweis im Analytics-Tab „Gefiltert: <Filterbeschreibung>".
- **Akzeptanzkriterien:**
  - [ ] Filter „sessionDefaultChannelGroup = Organic Search" erscheint als `dimensionFilter` im Request.
  - [ ] Zwei Filter erzeugen eine `andGroup`.
  - [ ] Ungültiger Regex wird vor dem Senden abgelehnt.
- **Testfixture:** Mock `runReport` mit Prüfung des Request-Bodys.

### INTEG-095 Abgleichoptionen und Ergebnislimit
- **Priorität:** P1
- **Beschreibung:** Optionen „Slash/Nicht-Slash abgleichen" und „Groß-/Kleinschreibung abgleichen" (INTEG-014). „Maximale Ergebnisse begrenzen" lädt nur die Top-N-URLs nach Sessions (Standard 100.000), um Laufzeit und Kontingent zu begrenzen.
- **Eingaben/Konfiguration:** zwei Checkboxen (Standard an); Zahl „Max. Ergebnisse" (Standard 100.000, Bereich 1–10.000.000).
- **Ausgabe:** —
- **Akzeptanzkriterien:**
  - [ ] Mit Limit 1.000 werden höchstens 1.000 Zeilen geladen, sortiert absteigend nach Sessions (`orderBys`).
  - [ ] URLs außerhalb der Top-N erhalten keine Daten und fallen in den Filter `No GA Data`.
- **Testfixture:** Mock-Report mit 2.500 Zeilen.

### INTEG-096 Paginierung großer Reports
- **Priorität:** P1
- **Beschreibung:** Reports werden seitenweise geladen (`limit` und `offset`), bis `rowCount` bzw. das Ergebnislimit erreicht ist. Die Data API liefert ohne Angabe 10.000 Zeilen je Anfrage und akzeptiert höchstens 250.000; OpenSpider verwendet standardmäßig 10.000 (einstellbar unter „Erweitert" bis 250.000, größere Eingaben werden auf 250.000 begrenzt) **(abgeleitet aus der Data-API-Doku)**. Erhält die Antwort Sampling- oder Schwellen-Hinweise (`dataLossFromOtherRow`, `subjectToThresholding`), wird ein Hinweis angezeigt.
- **Eingaben/Konfiguration:** —
- **Ausgabe:** Hinweis bei `(other)`-Zeile / Thresholding.
- **Akzeptanzkriterien:**
  - [ ] Mock mit `rowCount = 25.000` und Seitengröße 10.000 (Standard) → 3 Anfragen mit `offset` 0, 10.000, 20.000; alle 25.000 Zeilen werden verarbeitet.
  - [ ] Seitengröße 250.000 und `rowCount = 300.000` → 2 Anfragen (`limit: 250000`, `offset` 0 und 250.000); eine eingestellte Seitengröße von 400.000 wird als `limit: 250000` gesendet.
  - [ ] Das Ergebnislimit aus INTEG-095 begrenzt die Paginierung: Limit 15.000 bei Seitengröße 10.000 → 2 Anfragen, die zweite mit `limit: 5000`.
  - [ ] Bei `dataLossFromOtherRow = true` erscheint der Hinweis „GA4 hat Zeilen zu (other) zusammengefasst – Daten unvollständig".
- **Testfixture:** Mock mit paginierter Antwort und Metadaten-Flags.

### INTEG-097 Analytics-Tab: Spalten
- **Priorität:** P1
- **Beschreibung:** Tab „Analytics" mit Basisspalten und den gewählten GA4-Metriken.
- **Eingaben/Konfiguration:** —
- **Ausgabe:** `Address`, `Status Code`, `Indexability`, `Indexability Status`, `GA4 <Metrik>`… (Standard: `GA4 Sessions`, `GA4 Engaged Sessions`, `GA4 Engagement Rate`, `GA4 Views`, `GA4 Conversions`, `GA4 Event Count`, `GA4 Total Revenue`; UI-Beschriftung von `GA4 Conversions` darf „Key Events" lauten, siehe INTEG-092).
- **Akzeptanzkriterien:**
  - [ ] Für jede gecrawlte HTML-URL existiert eine Zeile; URLs ohne Daten haben leere Metrikzellen.
  - [ ] Liefert GA4 für eine URL `0` Sessions, steht `0` in der Zelle (nicht leer).
- **Testfixture:** `fixtures/integ/ga4/site/` (8 Seiten) + Mock-Report.

### INTEG-098 Analytics-Tab: Filter
- **Priorität:** P1
- **Beschreibung:** Fünf Filter im Analytics-Tab.
- **Eingaben/Konfiguration:** —
- **Ausgabe:** Filter mit URL-Zählern in der Übersicht.
- **Akzeptanzkriterien:**
  - [ ] **Sessions Above 0:** URLs mit `GA4 Sessions` ≥ 1.
  - [ ] **Bounce Rate Above 70%:** URLs mit Bounce Rate > 0,70 (streng größer; 0,70 exakt ist nicht enthalten). Ist Bounce Rate nicht als Metrik gewählt, wird sie für diesen Filter automatisch mit abgefragt (Bounce Rate = 1 − Engagement Rate) und nur bei Sessions ≥ 1 gewertet.
  - [ ] **No GA Data:** gecrawlte, interne, indexierbare oder nicht-indexierbare HTML-URLs (Status 200), für die die API keinerlei Zeile geliefert hat (alle GA4-Zellen leer). Ein geliefertes `0` zählt als Daten.
  - [ ] **Non-Indexable with GA Data:** URLs mit `Indexability = Non-Indexable` und mindestens einem GA4-Wert > 0.
  - [ ] **Orphan URLs:** URLs, die in den GA4-Daten vorkommen, aber über keinen internen Link gefunden wurden; nur befüllt nach Crawl-Analyse und bei aktivierter Option INTEG-015 (sonst Hinweis).
- **Testfixture:** `fixtures/integ/ga4/site/` mit: Seite mit 0 Sessions, mit 5 Sessions/Bounce 0,70, mit 3 Sessions/Bounce 0,71, noindex-Seite mit Sessions, Seite ohne Zeile, verwaiste Seite; erwartete Filterzählung in `expected-filters.json`.

### INTEG-099 GA4-Kontingente
- **Priorität:** P1
- **Beschreibung:** Die GA4 Data API begrenzt den Verbrauch über Token-Kontingente je Property (pro Tag, pro Stunde, gleichzeitige Anfragen). OpenSpider fordert `returnPropertyQuota` an, zeigt die Restmengen an (INTEG-006) und hält Anfragen zurück, wenn das Stundenkontingent knapp wird (< 5 % verbleibend), bis zur nächsten vollen Stunde.
- **Eingaben/Konfiguration:** —
- **Ausgabe:** Kontingentanzeige.
- **Akzeptanzkriterien:**
  - [ ] Antwort mit `tokensPerHour.remaining` < 5 % → nächste Anfrage erst nach Stundenwechsel (Testuhr).
  - [ ] Höchstens 10 gleichzeitige Anfragen je Property.
  - [ ] `RESOURCE_EXHAUSTED` → Integration pausiert mit Meldung; Crawl läuft weiter.
- **Testfixture:** Mock mit konfigurierbaren `propertyQuota`-Werten.

### INTEG-100 GA4-Daten in Segmenten und Reports
- **Priorität:** P1
- **Beschreibung:** GA4-Spalten sind wie alle Spalten in Segment-Regeln, Spaltenfiltern und Sortierung nutzbar (z. B. Segment „Seiten mit > 100 Sessions"). Segmentdefinition führend in CRAWL-134, Darstellung in UI-020/067/068. Der Orphan-Pages-Report (OUT-099, führend) enthält GA4 als Quelle.
- **Eingaben/Konfiguration:** Segment-Editor (CRAWL-134).
- **Ausgabe:** —
- **Akzeptanzkriterien:**
  - [ ] Segment „GA4 Sessions > 100" ordnet genau die URLs mit mehr als 100 Sessions zu (Operator „größer als" ist streng).
  - [ ] Orphan-Pages-Report listet GA4-only-URLs mit dem GA-Quellwert aus OUT-099.
- **Testfixture:** Fixture aus INTEG-098.

### INTEG-101 Kompatibilitätsprüfung von GA4-Metriken und -Dimensionen
- **Priorität:** P1
- **Beschreibung:** Die GA4 Data API lehnt Anfragen ab, deren Metriken und Dimensionen (Seitendimension aus INTEG-093 plus Filterdimensionen aus INTEG-094) nicht miteinander kombinierbar sind. Die Referenz macht inkompatible Metriken seit einer früheren Hauptversion in der Auswahl nicht mehr wählbar; Einzelheiten dazu beschreibt die öffentliche Doku nicht (Release-Zuordnung **zu verifizieren**). OpenSpider fragt dazu den Kompatibilitätsendpunkt der Data API (`checkCompatibility`) mit der aktuellen Auswahl ab und deaktiviert inkompatible Einträge im Metrik-Baum bzw. in der Dimensionsliste **(abgeleitet)**.
- **Eingaben/Konfiguration:** Metrik-Baum, Seitendimension je Metrik, Dimensionsfilter.
- **Ausgabe:** Ausgegraute Einträge mit Tooltip „Nicht kombinierbar mit: <Dimension/Metrik>"; Warnsymbol an bereits gewählten, inzwischen inkompatiblen Metriken.
- **Akzeptanzkriterien:**
  - [ ] Nach jeder Änderung der Auswahl wird höchstens 500 ms später (Entprellung) genau ein `checkCompatibility`-Request mit allen gewählten Metriken und Dimensionen gesendet (Mock-Log).
  - [ ] Metriken, die der Mock als `INCOMPATIBLE` meldet, sind im Baum ausgegraut und nicht anklickbar; die übrigen bleiben wählbar.
  - [ ] Wird durch Hinzufügen eines Dimensionsfilters eine bereits gewählte Metrik inkompatibel, wird sie nicht stillschweigend entfernt, sondern markiert; „OK" ist deaktiviert, bis der Konflikt aufgelöst ist.
  - [ ] Ist der Kompatibilitätsendpunkt nicht erreichbar, bleibt die Auswahl frei; lehnt dann `runReport` die Kombination ab (HTTP 400), erscheint die API-Meldung im Status, und es werden keine Teilwerte gesetzt.
  - [ ] Kein Request an `runReport` enthält eine Kombination, die `checkCompatibility` zuvor als inkompatibel gemeldet hat.
- **Testfixture:** Mock `checkCompatibility` mit Regel „Metrik `advertiserAdCost` inkompatibel zu Dimension `pageLocation`" und Regel „Filterdimension `firstUserSource` macht `itemRevenue` inkompatibel"; Mock `runReport` liefert für dieselbe Kombination 400.

---

## G Search Console – Search Analytics

> API: Search Console API `searchanalytics.query` und `sites.list`. Umsetzung über OAuth gemäß INTEG-003.

### INTEG-110 Verbindung und Property-Auswahl
- **Priorität:** P1
- **Beschreibung:** Nach Google-Anmeldung werden alle Properties des Kontos (Domain-Properties `sc-domain:` und URL-Präfix-Properties) mit Berechtigungsstufe gelistet. Die Property sollte zur Crawl-Domain passen; bei Abweichung erscheint ein Hinweis.
- **Eingaben/Konfiguration:** Property-Dropdown mit Suchfeld.
- **Ausgabe:** Status `Verbunden (<Konto> / <Property>)`.
- **Akzeptanzkriterien:**
  - [ ] `sc-domain:integ.test` und `https://integ.test/` erscheinen beide und sind unterscheidbar beschriftet („Domain" / „URL-Präfix").
  - [ ] Properties mit Berechtigung `siteUnverifiedUser` werden ausgegraut.
  - [ ] Crawl von `http://other.test/` mit gewählter Property `sc-domain:integ.test` → Warnhinweis „Property passt nicht zur Crawl-Domain".
- **Testfixture:** Mock `sites.list` mit 4 Properties (inkl. unverifiziert).

### INTEG-111 Zeitraum und Datenstand
- **Priorität:** P1
- **Beschreibung:** Standardzeitraum: letzte 30 Tage bis einschließlich des letzten vollständig verfügbaren Tages. Vorgaben 7, 28, 30, 90 Tage, 3, 6, 12, 16 Monate sowie frei wählbare Daten; Search Console stellt höchstens 16 Monate bereit **(abgeleitet)**. Option „Frische (vorläufige) Daten einbeziehen" (`dataState = all`) **(abgeleitet)**.
- **Eingaben/Konfiguration:** Zeitraum, Checkbox „Vorläufige Daten" (Standard aus).
- **Ausgabe:** Zeitraumhinweis im Tab.
- **Akzeptanzkriterien:**
  - [ ] Standardanfrage: `endDate` = heute−2 (Zeitzone America/Los_Angeles), `startDate` = `endDate`−29 **(abgeleitet: die Referenz-Doku nennt den Datenstand nicht; Search Console stellt finale Daten typischerweise mit 2–3 Tagen Verzug bereit)**.
  - [ ] Ein Startdatum älter als 16 Monate wird auf das früheste mögliche Datum begrenzt (mit Hinweis) **(abgeleitet aus der Datenaufbewahrung der Search Console API, nicht aus der Referenz-Doku)**.
  - [ ] Mit „Vorläufige Daten" enthält der Request `dataState: "all"`.
- **Testfixture:** Mock `searchanalytics.query` mit Request-Protokoll; Testuhr.

### INTEG-112 Suchtyp
- **Priorität:** P1
- **Beschreibung:** **(abgeleitet)** Auswahl des Suchtyps: Web (Standard), Bilder, Video, News (Tab „News" der Google-Suche), Google News, Discover.
- **Eingaben/Konfiguration:** Dropdown Suchtyp.
- **Ausgabe:** Suchtyp im Tab-Hinweis und im Spaltentooltip.
- **Akzeptanzkriterien:**
  - [ ] Standard-Request enthält `type: "web"`.
  - [ ] Bei `discover` und `googleNews` werden Positions-Werte nicht angezeigt (die API liefert dort keine Position; Spalte `Position` leer) und Dimensionsfilter auf `query`/`device` sind deaktiviert.
- **Testfixture:** Mock mit typabhängigen Antworten.

### INTEG-113 Dimensionsfilter (Suchanfrage, Land, Gerät)
- **Priorität:** P1
- **Beschreibung:** **(abgeleitet)** Filter auf Suchanfrage (enthält, enthält nicht, ist gleich, ist nicht gleich, Regex enthält, Regex enthält nicht), Land (ISO-3166-1-alpha-3) und Gerät (Desktop, Mobil, Tablet). Typischer Einsatz: Marken-Suchanfragen ausschließen (Regex „enthält nicht" mit Markenvarianten). Mehrere Filter werden mit UND verknüpft.
- **Eingaben/Konfiguration:** Filterliste Dimension/Operator/Wert.
- **Ausgabe:** Hinweis „Gefiltert: …" im Tab.
- **Akzeptanzkriterien:**
  - [ ] Filter „Query excludingRegex `(?i)openspider|open spider`" erscheint als `dimensionFilterGroups` im Request.
  - [ ] Land `deu` und Gerät `MOBILE` werden als zusätzliche Filter gesendet.
  - [ ] Hinweis im Dialog: Mit Suchanfrage-Filter enthalten die Daten keine anonymisierten Suchanfragen; Summen können daher niedriger als ohne Filter sein.
  - [ ] Ein ungültiger RE2-Regex wird vor dem Senden abgelehnt (Search Console verwendet RE2-Syntax: kein Lookaround).
- **Testfixture:** Mock mit Request-Prüfung.

### INTEG-114 Paginierung und Zeilenlimit
- **Priorität:** P1
- **Beschreibung:** Abfrage mit Dimension `page`; Seiten zu je 25.000 Zeilen (`rowLimit`, `startRow`) bis eine Seite weniger Zeilen liefert. Ein optionales Gesamtlimit begrenzt die Anzahl.
- **Eingaben/Konfiguration:** „Max. Zeilen" (Standard: unbegrenzt).
- **Ausgabe:** —
- **Akzeptanzkriterien:**
  - [ ] Mock mit 60.000 Seiten-Zeilen → 3 Anfragen (`startRow` 0, 25.000, 50.000).
  - [ ] Eine leere erste Antwort (keine `rows`) führt zu keinem Fehler; alle URLs fallen in `No Search Analytics Data`.
- **Testfixture:** Mock mit großer, paginierter Antwort.

### INTEG-115 Search-Console-Tab: Spalten
- **Priorität:** P1
- **Beschreibung:** Tab „Search Console" mit Search-Analytics-Spalten und (optional, H) URL-Inspection-Spalten.
- **Eingaben/Konfiguration:** —
- **Ausgabe:** `Address`, `Status Code`, `Indexability`, `Indexability Status`, `Clicks`, `Impressions`, `CTR`, `Position`, danach URL-Inspection-Spalten (INTEG-123).
- **Akzeptanzkriterien:**
  - [ ] CTR wird als Prozent mit 2 Nachkommastellen angezeigt; Export als Dezimalwert (0–1).
  - [ ] Position wird mit 1 Nachkommastelle angezeigt (ungerundet im Export).
  - [ ] Clicks/Impressions sind Ganzzahlen.
- **Testfixture:** `fixtures/integ/gsc/site/` (8 Seiten) + Mock.

### INTEG-116 Search-Console-Tab: Search-Analytics-Filter
- **Priorität:** P1
- **Beschreibung:** Vier Filter auf Basis der Search-Analytics-Daten (die sieben URL-Inspection-Filter folgen in INTEG-125).
- **Eingaben/Konfiguration:** —
- **Ausgabe:** Filter mit Zählern.
- **Akzeptanzkriterien:**
  - [ ] **Clicks Above 0:** URLs mit `Clicks` ≥ 1.
  - [ ] **No Search Analytics Data:** gecrawlte interne HTML-URLs, für die die API keine Zeile geliefert hat (Impressions-Zelle leer).
  - [ ] **Non-Indexable with Search Analytics Data:** `Indexability = Non-Indexable` und `Impressions` ≥ 1.
  - [ ] **Orphan URLs:** URLs mit Search-Analytics-Daten, die nicht über interne Links gefunden wurden; nur nach Crawl-Analyse und mit Option INTEG-015.
- **Testfixture:** `fixtures/integ/gsc/site/` mit Seite mit 0 Clicks/120 Impressions, noindex-Seite mit Impressions, Seite ohne Zeile, verwaiste URL; `expected-filters.json`.

### INTEG-117 Aggregation und Doppel-URLs
- **Priorität:** P1
- **Beschreibung:** Daten werden je Seite (`aggregationType = byPage`) abgefragt. Varianten einer URL (Protokoll, Slash, Groß-/Kleinschreibung) werden nach INTEG-014 zugeordnet, ohne Summierung; Fragmente (`#`) in Seiten-URLs der API (z. B. aus Sprungmarken in Suchergebnissen) werden für den Abgleich entfernt, und deren Werte werden der fragmentlosen URL nur zugeordnet, wenn keine fragmentlose Zeile existiert.
- **Eingaben/Konfiguration:** wie INTEG-014.
- **Ausgabe:** —
- **Akzeptanzkriterien:**
  - [ ] Request enthält `aggregationType: "byPage"` und Dimension `page`.
  - [ ] API-Zeilen `/x` (10 Clicks) und `/x#abschnitt` (4 Clicks) → gecrawltes `/x` erhält 10.
  - [ ] Nur `/y#abschnitt` (4 Clicks) vorhanden → gecrawltes `/y` erhält 4.
- **Testfixture:** Mock mit Fragment-Zeilen.

---

## H Search Console – URL Inspection

> API: Search Console URL Inspection API (`urlInspection/index:inspect`). Öffentliche Limits: 2.000 Abfragen pro Tag und 600 pro Minute je Property.

### INTEG-120 URL Inspection aktivieren und Tageskontingent
- **Priorität:** P1
- **Beschreibung:** Optional wird für gecrawlte HTML-URLs der Indexierungsstatus abgefragt. Je Property sind 2.000 Abfragen pro Tag möglich (Zähler INTEG-006), maximal 600 pro Minute. Abgefragt werden die ersten 2.000 HTML-URLs in Entdeckungsreihenfolge (Crawl ist Breitensuche nach Crawl-Tiefe); wer andere URLs prüfen will, nutzt Include/Exclude oder den Listenmodus.
- **Eingaben/Konfiguration:** Checkbox „URL Inspection aktivieren" (Standard aus); Sprache der Meldungen (`languageCode`, Standard: UI-Sprache).
- **Ausgabe:** URL-Inspection-Spalten (INTEG-123).
- **Akzeptanzkriterien:**
  - [ ] Bei 2.500 gecrawlten HTML-URLs werden genau 2.000 abgefragt, und zwar die mit kleinster Crawl-Tiefe/frühester Entdeckung; die übrigen 500 erhalten `Summary` = `Quota exceeded`.
  - [ ] Nie mehr als 600 Anfragen je Minute und Property (Messung am Mock).
  - [ ] Nicht-HTML-URLs (Bilder, PDFs, CSS) werden nicht abgefragt.
  - [ ] Im Listenmodus werden die hochgeladenen URLs in Listenreihenfolge abgefragt.
- **Testfixture:** generierte Fixture-Site `fixtures/integ/gsc/deep-site/` (2.500 Seiten, 5 Ebenen) + Mock mit Zähler.

### INTEG-121 Nicht-indexierbare URLs auslassen
- **Priorität:** P1
- **Beschreibung:** Option, nicht-indexierbare URLs (noindex, Canonical auf andere URL, Nicht-200, robots-blockiert) nicht abzufragen, um Kontingent zu sparen.
- **Eingaben/Konfiguration:** Checkbox „Nicht-indexierbare URLs bei URL Inspection auslassen" (Standard aus).
- **Ausgabe:** Ausgelassene URLs: `Summary` leer.
- **Akzeptanzkriterien:**
  - [ ] Mit Option werden noindex- und kanonisierte URLs nicht abgefragt (Mock-Log), indexierbare schon.
  - [ ] Das Kontingent wird dadurch nicht verbraucht (Zähler steigt nur um die indexierbaren URLs).
- **Testfixture:** `fixtures/integ/gsc/site/` (2 indexierbare, 1 noindex, 1 kanonisiert).

### INTEG-122 Mehrere Properties verwenden
- **Priorität:** P1
- **Beschreibung:** Sind für eine Domain mehrere Properties verifiziert (z. B. `sc-domain:integ.test`, `https://integ.test/`, `https://integ.test/blog/`), erkennt OpenSpider alle und verwendet je URL die spezifischste passende Property (längster passender URL-Präfix, Domain-Property als Rückfall). Weil das Kontingent pro Property gilt, sind so mehr als 2.000 Abfragen je Crawl möglich.
- **Eingaben/Konfiguration:** Checkbox „Mehrere Properties verwenden" (Standard aus).
- **Ausgabe:** Spalte `Inspection Property` (welche Property genutzt wurde).
- **Akzeptanzkriterien:**
  - [ ] `https://integ.test/blog/a` → Property `https://integ.test/blog/`; `https://integ.test/shop/b` → `https://integ.test/`; `https://sub.integ.test/c` → `sc-domain:integ.test`.
  - [ ] Ist das Kontingent der spezifischsten Property erschöpft, wird **keine** andere Property als Ausweichweg genutzt (Ergebnisse wären sonst uneinheitlich), sondern `Quota exceeded` gesetzt.
  - [ ] Mit 2 passenden Properties und 3.000 URLs (je 1.500 pro Property) werden alle 3.000 abgefragt.
- **Testfixture:** Mock `sites.list` mit 3 Properties; Fixture-Site mit `/blog/`, `/shop/`, Subdomain.

### INTEG-123 URL-Inspection-Spalten
- **Priorität:** P1
- **Beschreibung:** 17 Spalten aus der API-Antwort, im Search-Console-Tab und Internal-Tab.
- **Eingaben/Konfiguration:** —
- **Ausgabe (Spalte ← API-Feld):**
  - `Summary` ← abgeleitet (INTEG-124)
  - `Coverage` ← `indexStatusResult.coverageState` (Klartext der API, z. B. „Submitted and indexed", „Crawled - currently not indexed", „Discovered - currently not indexed", „URL is unknown to Google")
  - `Last Crawl` ← `indexStatusResult.lastCrawlTime` (lokale Zeit, ISO-8601 im Export)
  - `Crawled As` ← `crawledAs` (`Desktop`/`Mobile`)
  - `Crawl Allowed` ← `robotsTxtState` (`Yes` bei ALLOWED, `No` bei DISALLOWED)
  - `Page Fetch` ← `pageFetchState` (z. B. `Successful`, `Soft 404`, `Blocked by robots.txt`, `Not found (404)`, `Server error (5xx)`, `Redirect error`, `Access denied (403)`, `Blocked (4xx)`, `Internal crawl error`, `Invalid URL`)
  - `Indexing Allowed` ← `indexingState` (`Yes`, `No: 'noindex' detected in 'robots' meta tag`, `No: 'noindex' detected in 'X-Robots-Tag' http header`, …)
  - `User-Declared Canonical` ← `userCanonical`
  - `Google-Selected Canonical` ← `googleCanonical`
  - `Mobile Usability` ← `mobileUsabilityResult.verdict`
  - `Mobile Usability Issues` ← Liste der Issue-Typen, mit `; ` getrennt
  - `AMP Results` ← `ampResult.verdict`
  - `AMP Issues` ← Liste der AMP-Issue-Meldungen
  - `Rich Results` ← `richResultsResult.verdict`
  - `Rich Results Types` ← erkannte Typen (`detectedItems[].richResultType`), `; ` getrennt
  - `Rich Results Types Errors` ← Typen mit mindestens einem Issue der Schwere ERROR
  - `Rich Results Warnings` ← Typen mit Issues der Schwere WARNING
- **Akzeptanzkriterien:**
  - [ ] Jede der 17 Spalten wird aus einer Mock-Antwort korrekt befüllt (Vergleich mit `expected-inspection.csv`).
  - [ ] Fehlt ein Teilergebnis in der Antwort (z. B. kein `ampResult`, weil keine AMP-Seite; kein `mobileUsabilityResult`, weil von Google eingestellt), bleiben die zugehörigen Spalten leer, ohne Fehler.
  - [ ] Verdikte werden lesbar abgebildet: PASS → `Pass`, PARTIAL → `Partial`, FAIL → `Fail`, NEUTRAL → `Neutral`.
- **Testfixture:** Mock-Antworten `inspect-indexed.json`, `inspect-not-indexed.json`, `inspect-canonical-mismatch.json`, `inspect-rich-error.json`, `inspect-amp-fail.json`, `inspect-mobile-fail.json`, `inspect-minimal.json`.

### INTEG-124 Ableitung der Spalte „Summary"
- **Priorität:** P1
- **Beschreibung:** `Summary` fasst den Status in drei Werte zusammen, analog zur Search-Console-Oberfläche.
- **Eingaben/Konfiguration:** —
- **Ausgabe:** `URL is on Google` | `URL is on Google, but has issues` | `URL is not on Google` (bzw. `Quota exceeded`, `Error: <Meldung>`).
- **Akzeptanzkriterien:**
  - [ ] `indexStatusResult.verdict = PASS` und keines der Teilergebnisse Mobile/AMP/Rich Results = FAIL bzw. PARTIAL → `URL is on Google`.
  - [ ] `indexStatusResult.verdict = PASS` und mindestens ein Teilergebnis FAIL/PARTIAL → `URL is on Google, but has issues`.
  - [ ] `indexStatusResult.verdict` ≠ PASS (FAIL, NEUTRAL, PARTIAL) → `URL is not on Google`.
  - [ ] Ein Fehler der API für diese URL (z. B. 403 „URL gehört nicht zur Property") → `Error: …`.
- **Testfixture:** Mock-Antworten aus INTEG-123.

### INTEG-125 URL-Inspection-Filter
- **Priorität:** P1
- **Beschreibung:** Sieben Filter im Search-Console-Tab auf Basis der URL-Inspection-Daten.
- **Eingaben/Konfiguration:** —
- **Ausgabe:** Filter mit Zählern.
- **Akzeptanzkriterien:**
  - [ ] **URL Is Not on Google:** alle URLs mit `Summary = URL is not on Google` – unabhängig von OpenSpiders eigener Indexierbarkeitsbewertung (Sammelfilter).
  - [ ] **Indexable URL Not Indexed:** URLs mit `Indexability = Indexable` (OpenSpider) und `Summary = URL is not on Google`.
  - [ ] **URL is on Google, But Has Issues:** `Summary = URL is on Google, but has issues`.
  - [ ] **User-Declared Canonical Not Selected:** `User-Declared Canonical` und `Google-Selected Canonical` beide nicht leer und nach URL-Normalisierung verschieden. Ist kein User-Canonical gesetzt, greift der Filter nicht.
  - [ ] **Page Is Not Mobile Friendly:** `Mobile Usability = Fail`.
  - [ ] **AMP URL Is Invalid:** `AMP Results = Fail`.
  - [ ] **Rich Result Invalid:** `Rich Results = Fail` oder `Rich Results Types Errors` nicht leer.
  - [ ] Abgefragte URLs ohne Daten zu einem Teilbereich fallen nicht in dessen Filter.
- **Testfixture:** `fixtures/integ/gsc/inspection-site/` (7 Seiten, jede provoziert genau einen Filter) + passende Mock-Antworten; `expected-filters.json`.

### INTEG-126 Detailansicht URL Inspection
- **Priorität:** P2
- **Beschreibung:** Unterer Detail-Tab „URL Inspection" mit allen Feldern der Rohantwort, inkl. `referringUrls`, `sitemap`, Rich-Result-Elementen mit Einzel-Issues und einem Link zur Prüfung der URL in der Search-Console-Oberfläche (`inspectionResultLink`). Dieser Eintrag legt den Inhalt fest; Layout und Grundverhalten des Detailfensters: UI-030 (führend). Bulk-Exporte „URL Inspection → Rich Results / Referring Pages / Sitemaps": OUT-064 (führend).
- **Eingaben/Konfiguration:** —
- **Ausgabe:** Detailtabelle mit einer Zeile je Rich-Result-Issue (`Rich Result Type`, `Item Name`, `Severity`, `Issue`).
- **Akzeptanzkriterien:**
  - [ ] Für eine URL mit 2 Rich-Result-Fehlern zeigt die Detailansicht beide Meldungen; der Bulk-Export nach OUT-064 enthält für diese URL 2 Zeilen.
  - [ ] Der Link „In Search Console öffnen" verwendet `inspectionResultLink` aus der Antwort.
- **Testfixture:** `inspect-rich-error.json`.

### INTEG-127 Fehlerbehandlung URL Inspection
- **Priorität:** P1
- **Beschreibung:** Antwortfehler werden je URL abgelegt: 403 (keine Berechtigung bzw. URL nicht in Property), 429 (Kontingent), 5xx (vorübergehend, Wiederholung nach INTEG-005).
- **Eingaben/Konfiguration:** —
- **Ausgabe:** `Summary = Error: <Meldung>`.
- **Akzeptanzkriterien:**
  - [ ] URL eines fremden Hosts → `Error: URL not in property` und kein Verbrauch beim lokalen Zähler.
  - [ ] Antwort 429 mit Tageskontingent-Grund → alle verbleibenden URLs `Quota exceeded`, keine weiteren Anfragen.
  - [ ] Eine Inspection-Antwort, die erst nach 90 s eintrifft, wird mit dem Standard-Read-Timeout (INTEG-128) noch verarbeitet; überschreitet sie den Timeout, gilt sie als vorübergehender Fehler und wird nach INTEG-005 wiederholt; nach Ausschöpfen der Wiederholungen → `Summary = Error: Read timeout`.
- **Testfixture:** Mock-Szenarien `403-not-in-property`, `429-daily`, `delay-90s`, `delay-180s`.

### INTEG-128 Read-Timeout für Search-Console-Anfragen
- **Priorität:** P1
- **Beschreibung:** Search-Console-Antworten (Search Analytics bei großen Properties, URL Inspection) können sehr lange dauern. Die Referenz hat den Lese-Timeout für Search-Console-Anfragen in einer jüngeren Version deutlich angehoben (von 20 s auf 2 Minuten). OpenSpider verwendet für alle Search-Console-Endpunkte (`sites.list`, `searchanalytics.query`, `urlInspection/index:inspect`) standardmäßig einen Lese-Timeout von 120 s, im Reiter „Erweitert" einstellbar. Der Verbindungsaufbau-Timeout ist davon getrennt (fest 30 s).
- **Eingaben/Konfiguration:** „Erweitert → Read-Timeout (Sekunden)": Standard 120, Bereich 10–600.
- **Ausgabe:** Bei Überschreitung Fehlerstatus `Read timeout` (Search Analytics: Statusmeldung der Integration; URL Inspection: `Summary = Error: Read timeout`).
- **Akzeptanzkriterien:**
  - [ ] Mit Standardwert wird eine `searchanalytics.query`-Antwort mit 100 s Verzögerung vollständig verarbeitet (alle Zeilen zugeordnet).
  - [ ] Mit Einstellung 30 s führt dieselbe Antwort zu einem Timeout nach 30 s (±2 s) und einer Wiederholung gemäß INTEG-005; bleibt es beim Timeout, zeigt der Status `Fehler: Read timeout nach 30 s`, bereits geladene Seiten der Paginierung bleiben erhalten.
  - [ ] Werte außerhalb 10–600 werden im Dialog abgelehnt.
  - [ ] Der Timeout gilt je Anfrage, nicht für die gesamte Paginierung (3 Seiten à 100 s mit Timeout 120 s → Erfolg).
  - [ ] Die Einstellung wirkt nur auf Search-Console-Endpunkte: Bei GSC-Read-Timeout 300 s und GA4-Mock mit 100 s Verzögerung wird der GA4-Request weiterhin nach dem GA4-eigenen Timeout (Standard 60 s) abgebrochen.
- **Testfixture:** Mock `searchanalytics.query` mit Szenarien `delay-100s` (1 Seite) und `paged-delay-100s` (3 Seiten); Testuhr für beschleunigte Zeitmessung.

---

## I PageSpeed Insights / Lighthouse

> API: PageSpeed Insights API v5 (`runPagespeed`, Parameter `strategy`, `category`, `key`). Die Feld- und Labordaten werden aus `loadingExperience`, `originLoadingExperience` und `lighthouseResult` gelesen. Lighthouse-Audit-IDs beziehen sich auf Lighthouse 13 (öffentliche Lighthouse-Dokumentation); bei Versionswechseln wird die Zuordnungstabelle gepflegt.

### INTEG-130 PSI-Verbindung und Quelle (Remote/Lokal)
- **Priorität:** P1
- **Beschreibung:** Zwei Quellen: **Remote** – Lighthouse läuft über die PSI-API auf Google-Servern; liefert zusätzlich CrUX-Felddaten; benötigt einen (kostenlosen) API-Schlüssel aus der Google Cloud Console mit aktivierter PSI-API. **Lokal** – Lighthouse läuft auf dem eigenen Rechner (INTEG-132); kein Schlüssel, kein Tageslimit, auch für Staging-/Login-Seiten, aber keine CrUX-Daten.
- **Eingaben/Konfiguration:** Quelle (Remote/Lokal; Schlüssel `psi.source`, **Standard `remote`**), API-Schlüssel (Remote), Verbinden. **OS-Festlegung** (die Referenz-Doku nennt keinen Standard): Remote ist Standard, weil nur diese Quelle CrUX-Felddaten liefert und unter den Messbedingungen der Google-Server mit Referenz-Exporten vergleichbar ist; Lokal ist eine Auswahl ohne Schlüssel und für Staging-Seiten. Dieser Eintrag ist führend für Quellen und Standardquelle (TECH-230 verweist hierher).
- **Ausgabe:** Status; PageSpeed-Tab.
- **Akzeptanzkriterien:**
  - [ ] „Verbinden" (Remote) prüft den Schlüssel mit einer Testanfrage. Antwortet Google mit 403 `SERVICE_DISABLED` bzw. `API_KEY_INVALID`, lautet der Status `Fehler: PSI-Zugriff abgelehnt (<Grund>)`, und darunter erscheint eine Prüfliste: (a) Ist im Google-Cloud-Projekt des Schlüssels die „PageSpeed Insights API" freigeschaltet? (b) Wurde der Schlüssel eben erst angelegt? Dann später erneut verbinden. (c) Ist der Schlüssel auf bestimmte APIs/Referrer beschränkt?
  - [ ] Ein neues Profil ohne gespeicherte Auswahl steht auf Quelle Remote; ohne API-Schlüssel ist die Integration nicht verbunden (keine stille Umstellung auf Lokal).
  - [ ] Bei Quelle Lokal sind alle CrUX-Spalten leer und die CrUX-Metrikgruppe ist im Dialog deaktiviert.
  - [ ] Abgefragt werden nur interne HTML-URLs mit Status 200 (keine Weiterleitungen, keine Ressourcen).
- **Testfixture:** Mock-PSI mit Szenarien `key-ok`, `key-not-enabled` (403 `SERVICE_DISABLED`), `key-invalid` (400 `API_KEY_INVALID`).

### INTEG-131 Gerätetyp
- **Priorität:** P1
- **Beschreibung:** Auswahl, ob die Analyse für Mobil (Standard) oder Desktop erfolgt (`strategy = mobile | desktop`); lokal mit entsprechender Lighthouse-Emulation (Viewport, Drosselung).
- **Eingaben/Konfiguration:** Dropdown Gerät.
- **Ausgabe:** Gerätetyp im Tab-Hinweis.
- **Akzeptanzkriterien:**
  - [ ] Standard-Request enthält `strategy=mobile`.
  - [ ] Bei Desktop enthalten CrUX-Werte die Desktop-Felddaten (Request `strategy=desktop`).
- **Testfixture:** Mock-PSI mit strategieabhängiger Antwort.

### INTEG-132 Lokaler Lighthouse-Lauf
- **Priorität:** P1
- **Beschreibung:** Lighthouse wird lokal gegen die URLs ausgeführt (eigene Chrome-Instanz, getrennt vom Render-Browser des Crawls). Authentifizierung (Cookies/Header der Crawl-Konfiguration) wird übernommen, sodass auch geschützte Staging-Seiten geprüft werden können. Läufe sind ressourcenintensiv; die Parallelität ist begrenzt (Standard 1, max. CPU-Kerne/2). Umsetzungshinweis: Lighthouse steht unter Apache-2.0 und kann mit einer Node-Laufzeit gebündelt werden; alternativ wird eine vorhandene Node-/Lighthouse-Installation genutzt.
- **Eingaben/Konfiguration:** Quelle = Lokal; „Parallele Lighthouse-Läufe" (1 … ⌊CPU-Kerne/2⌋, mindestens 1); „Timeout je URL" (Standard 90 s, Bereich 30–600 s); optional Pfad zu einer vorhandenen Node-/Lighthouse-Installation.
- **Ausgabe:** wie Remote, ohne CrUX.
- **Akzeptanzkriterien:**
  - [ ] Eine hinter HTTP-Basic-Auth liegende Fixture-Seite liefert mit konfigurierten Zugangsdaten einen Performance Score; ohne Zugangsdaten `PSI Status = Error`, `PSI Error` beginnt mit `ERRORED_DOCUMENT_REQUEST`.
  - [ ] Bei Parallelität 1 läuft nie mehr als ein Lighthouse-Prozess gleichzeitig; bei Parallelität 3 nie mehr als 3 (Prozessliste wird während des Laufs alle 200 ms abgetastet).
  - [ ] Auf einem Rechner mit 4 Kernen lässt der Dialog maximal den Wert 2 zu.
  - [ ] Die verwendete Lighthouse-Version wird im Kontodialog und in der Spalte `Lighthouse Version` angezeigt (identisch mit `lighthouseResult.lighthouseVersion` des Laufs).
  - [ ] Eine Seite, die nie fertig lädt, wird nach dem Timeout je URL abgebrochen (`PSI Error = Timeout`); der Chrome-Prozess dieses Laufs ist danach beendet, und die nächste URL wird normal analysiert.
  - [ ] Stürzt der Lighthouse-Chrome ab (Prozess wird im Test per Signal beendet), erhält nur die betroffene URL einen Fehler; der Crawl und die übrigen Läufe gehen weiter.
  - [ ] Ohne gebündelte oder konfigurierte Lighthouse-Laufzeit ist „Lokal" mit dem Status `Fehler: Lighthouse-Laufzeit nicht gefunden` nicht verbindbar.
- **Testfixture:** `fixtures/integ/psi/auth/` (Basic Auth), `fixtures/integ/psi/heavy.html` (große Bilder, render-blockierendes CSS, unbenutztes JS, Layout-Shift), `fixtures/integ/psi/never-loads.html` (hält eine Ressource-Anfrage endlos offen).

### INTEG-133 Metrikgruppen-Auswahl
- **Priorität:** P1
- **Beschreibung:** Im Reiter „Metriken" werden die Gruppen Overview, CrUX (Felddaten), Lighthouse (Labordaten), Insights, Diagnostics und Mobile ein-/ausgeschaltet bzw. einzelne Metriken gewählt. Nur gewählte Metriken erscheinen als Spalten. Die Gruppenzuordnung folgt der Metrikauswahl der Referenz-Konfigurationsdoku: **Optimize DOM Size** (`DOM Size`) und **Font Display** (`Font Display Savings (ms)`) gehören zu Diagnostics, **Avoid Enormous Network Payloads** (`Network Payload Size (Bytes)`) gehört zu Insights. (Die Tab-Doku der Referenz listet diese Spalten teilweise in der jeweils anderen Gruppe; maßgeblich für OpenSpider ist die Metrikauswahl, weil sie bestimmt, welche Spalten und Filter erscheinen.)
- **Eingaben/Konfiguration:** Metrik-Baum.
- **Ausgabe:** Spaltenumfang des PageSpeed-Tabs; Spaltenreihenfolge im Tab: Overview, CrUX, Lighthouse, Insights, Diagnostics (Mobile-Spalten erscheinen im Mobile-Tab, TECH-231; Datenbeschaffung INTEG-140).
- **Akzeptanzkriterien:**
  - [ ] Abwahl der Gruppe Diagnostics entfernt genau die Spalten aus INTEG-138 (inkl. `DOM Size`, `Font Display Savings (ms)`, `Efficient Cache Policy Savings (Bytes)`) sowie die Filter Minify CSS, Minify JavaScript, Reduce Unused CSS, Reduce Unused JavaScript, Reduce JavaScript Execution Time, Minimize Main-Thread Work, Optimize DOM Size und Font Display; der Filter Avoid Enormous Network Payloads bleibt.
  - [ ] Abwahl der Gruppe Insights entfernt die Spalten aus INTEG-137 (inkl. `Network Payload Size (Bytes)`) und die Filter Document Request Latency bis Duplicated JavaScript sowie Avoid Enormous Network Payloads.
  - [ ] Filter, deren Metrik nicht gewählt ist, erscheinen weder im Tab noch in der Übersicht rechts.
  - [ ] Abwahl der Gruppe Mobile entfernt die Lighthouse-basierten Spalten und Filter des Mobile-Tabs (TECH-231 ff., Daten aus INTEG-140); `Mobile Alternate Link` (TECH-237) bleibt, da es aus dem Crawl stammt.
  - [ ] Eine Abwahl verändert keine gespeicherten Werte: erneutes Anwählen nach dem Crawl zeigt die Spalten mit ihren Werten wieder an, sofern sie abgefragt wurden.
- **Testfixture:** UI-Test mit `psi-heavy.json`; erwartete Spalten-/Filterlisten je Gruppenkombination in `expected-groups.json`.

### INTEG-134 Overview-Metriken
- **Priorität:** P1
- **Beschreibung:** Zusammenfassende Ressourcen-Kennzahlen aus Lighthouse (Netzwerk-Requests).
- **Eingaben/Konfiguration:** Gruppe Overview.
- **Ausgabe:** `Total Size Savings (Bytes)`, `Total Time Savings (ms)`, `Total Requests`, `Total Page Size (Bytes)`, je Ressourcentyp `HTML Size`, `HTML Count`, `Image Size`, `Image Count`, `CSS Size`, `CSS Count`, `JavaScript Size`, `JavaScript Count`, `Font Size`, `Font Count`, `Media Size`, `Media Count`, `Other Size`, `Other Count`, `Third Party Size`, `Third Party Count`.
- **Akzeptanzkriterien:**
  - [ ] Größen sind übertragene Bytes (`transferSize`), ganzzahlig.
  - [ ] Summe der Größen der Typen HTML…Other = `Total Page Size` (Third Party ist eine Querschnittsmenge und nicht Teil der Summe).
  - [ ] `Total Size Savings` und `Total Time Savings` sind die Summen der Einsparpotenziale aller Insights/Diagnostics-Audits mit Einsparangabe; liefert ein Lauf sowohl `cache-insight` als auch das ältere `uses-long-cache-ttl`, wird nur `cache-insight` gezählt (keine Doppelzählung desselben Sachverhalts).
- **Testfixture:** Mock-PSI-Antwort `psi-heavy.json` mit bekannten Werten; `expected-overview.csv`.

### INTEG-135 CrUX-Felddaten (URL und Origin)
- **Priorität:** P1
- **Beschreibung:** Felddaten der echten Nutzer (75. Perzentil, 28 Tage) für die URL und für den gesamten Origin, mit Kategorie je Metrik und Gesamtbewertung der Core Web Vitals.
- **Eingaben/Konfiguration:** Gruppe CrUX (nur Remote).
- **Ausgabe:** `Core Web Vitals Assessment`, `CrUX First Contentful Paint (sec)`, `… Category`, `CrUX Largest Contentful Paint (sec)`, `… Category`, `CrUX Cumulative Layout Shift`, `… Category`, `CrUX Interaction to Next Paint (ms)`, `… Category`, `CrUX Time to First Byte (ms)`, `… Category`; dieselben Spalten mit Präfix `CrUX Origin`.
- **Akzeptanzkriterien:**
  - [ ] Kategorien: API-Werte FAST → `Good`, AVERAGE → `Needs Improvement`, SLOW → `Poor`.
  - [ ] Schwellen (zur Kontrolle der Zuordnung): LCP Good ≤ 2,5 s, Poor > 4,0 s; INP Good ≤ 200 ms, Poor > 500 ms; CLS Good ≤ 0,1, Poor > 0,25; FCP Good ≤ 1,8 s, Poor > 3,0 s; TTFB Good ≤ 0,8 s, Poor > 1,8 s.
  - [ ] CLS: Die API liefert das Perzentil mit 100 multipliziert (z. B. `12`); die Spalte zeigt `0.12`.
  - [ ] LCP/FCP werden von ms in Sekunden mit 2 Nachkommastellen umgerechnet.
  - [ ] `Core Web Vitals Assessment` = `Pass`, wenn LCP, CLS und (falls vorhanden) INP `Good` sind; `Fail`, wenn eine davon nicht `Good` ist; leer, wenn LCP oder CLS fehlen.
  - [ ] Liefert die API für die URL keine Felddaten (zu wenig Traffic), bleiben URL-Spalten leer; Origin-Spalten werden trotzdem befüllt, falls vorhanden.
  - [ ] Liefert die API in `loadingExperience` stattdessen Origin-Daten (Rückfall der API, erkennbar an `origin_fallback: true`), werden diese **nicht** als URL-Daten ausgewiesen.
- **Testfixture:** Mock-Antworten `crux-full.json`, `crux-origin-only.json`, `crux-fallback.json`, `crux-none.json`.

### INTEG-136 Lighthouse-Labordaten
- **Priorität:** P1
- **Beschreibung:** Labormesswerte und Scores aus dem Lighthouse-Lauf.
- **Eingaben/Konfiguration:** Gruppe Lighthouse.
- **Ausgabe:** `Performance Score`, `First Contentful Paint Time (sec)`, `First Contentful Paint Score`, `Speed Index Time (sec)`, `Speed Index Score`, `Largest Contentful Paint Time (sec)`, `Largest Contentful Paint Score`, `Time to Interactive (sec)`, `Time to Interactive Score`, `Max Potential First Input Delay (ms)`, `Max Potential First Input Delay Score`, `Total Blocking Time (ms)`, `Total Blocking Time Score`, `Cumulative Layout Shift`, `Cumulative Layout Shift Score`, `Lighthouse Version`.
- **Akzeptanzkriterien:**
  - [ ] `Performance Score` = `categories.performance.score × 100`, ganzzahlig gerundet (0–100).
  - [ ] Einzelscores ebenfalls 0–100.
  - [ ] Zeitwerte aus `audits[<id>].numericValue` (ms) → Sekunden mit 2 Nachkommastellen bzw. ms ganzzahlig, wie im Spaltennamen angegeben.
  - [ ] Wird ein Audit in der genutzten Lighthouse-Version nicht mehr geliefert (z. B. Time to Interactive in neueren Versionen nicht Teil des Scores), bleibt die Spalte leer.
  - [ ] Score-Farbkennzeichnung: 90–100 grün, 50–89 orange, 0–49 rot.
- **Testfixture:** `psi-heavy.json`, `psi-fast.json`.

### INTEG-137 Insights-Spalten
- **Priorität:** P1
- **Beschreibung:** Spalten aus den Lighthouse-Insight-Audits (seit Lighthouse 13 Ersatz für frühere „Opportunities").
- **Eingaben/Konfiguration:** Gruppe Insights.
- **Ausgabe (Spalte ← Audit-ID):**
  - `Document Request Latency` ← `document-latency-insight` (Teilprüfungen: `Number of Redirects`, `Server Responds Quickly`, `Applies Text Compression` als eigene Spalten Yes/No bzw. Anzahl)
  - `LCP Request Discovery` ← `lcp-discovery-insight`
  - `LCP Breakdown` ← `lcp-breakdown-insight` (Phasen TTFB, Load Delay, Load Duration, Render Delay in ms)
  - `Render Blocking Requests Savings (ms)` ← `render-blocking-insight`
  - `Preconnect Candidates Savings (ms)` und `Maximum Critical Path Latency (ms)` ← `network-dependency-tree-insight`
  - `Use Efficient Cache Lifetimes Savings (Bytes)` ← `cache-insight`
  - `Layout Shift Culprits` ← `cls-culprits-insight` (Anzahl Verursacher)
  - `Improve Image Delivery Savings (Bytes)` ← `image-delivery-insight`
  - `Forced Reflow Savings (ms)` ← `forced-reflow-insight`
  - `Legacy JavaScript Savings (Bytes)` ← `legacy-javascript-insight`
  - `Duplicated JavaScript Savings (Bytes)` ← `duplicated-javascript-insight`
  - `3rd Parties` ← `third-parties-insight` (Anzahl Drittanbieter)
  - `Network Payload Size (Bytes)` ← `total-byte-weight` (Gesamtgröße; gehört gemäß INTEG-133 zur Gruppe Insights, weil der Filter Avoid Enormous Network Payloads dort einsortiert ist)
  - (`DOM Size` und `Font Display Savings (ms)` stehen gemäß INTEG-133 bei den Diagnostics, INTEG-138.)
- **Akzeptanzkriterien:**
  - [ ] Jede Spalte wird aus `psi-heavy.json` mit dem erwarteten Wert befüllt (`expected-insights.csv`).
  - [ ] Einsparwerte stammen aus `metricSavings`/`details.overallSavingsMs` bzw. `overallSavingsBytes`; fehlen sie, ist die Zelle `0`, wenn das Audit bestanden ist, sonst leer.
  - [ ] Einheiten stehen im Spaltennamen; Bytes ganzzahlig, ms ganzzahlig.
- **Testfixture:** `psi-heavy.json` (Lighthouse-13-Struktur), Testseite `fixtures/integ/psi/heavy.html` für den lokalen Lauf.

### INTEG-138 Diagnostics-Spalten
- **Priorität:** P1
- **Beschreibung:** Spalten aus diagnostischen Lighthouse-Audits.
- **Eingaben/Konfiguration:** Gruppe Diagnostics.
- **Ausgabe (Spalte ← Audit-ID):** `Minify CSS Savings (ms)`, `Minify CSS Savings (Bytes)` ← `unminified-css`; `Minify JavaScript Savings (ms)`, `Minify JavaScript Savings (Bytes)` ← `unminified-javascript`; `Reduce Unused CSS Savings (ms)`, `Reduce Unused CSS Savings (Bytes)` ← `unused-css-rules`; `Reduce Unused JavaScript Savings (ms)`, `Reduce Unused JavaScript Savings (Bytes)` ← `unused-javascript`; `JavaScript Execution Time (sec)`, `JavaScript Execution Time Category` ← `bootup-time`; `Efficient Cache Policy Savings (Bytes)` ← `uses-long-cache-ttl` (älteres Cache-Audit; in Lighthouse-Versionen, die es nicht mehr liefern, bleibt die Spalte leer – der Nachfolger `cache-insight` füllt nur die Insights-Spalte `Use Efficient Cache Lifetimes Savings (Bytes)`); `Minimize Main-Thread Work (sec)`, `Minimize Main-Thread Work Category` ← `mainthread-work-breakdown`; `DOM Size` ← `dom-size-insight` (Anzahl Elemente); `Font Display Savings (ms)` ← `font-display-insight`; `User Timing Marks and Measures` ← `user-timings` (Anzahl). (`Network Payload Size (Bytes)` steht gemäß INTEG-133 bei den Insights.)
- **Akzeptanzkriterien:**
  - [ ] Kategorie-Spalten: Audit-Score ≥ 0,9 → `Good`, 0,5 ≤ Score < 0,9 → `Average`, < 0,5 → `Slow`; Grenzfälle: 0,9 → `Good`, 0,5 → `Average`, 0,4999 → `Slow`.
  - [ ] Alle Werte entsprechen `expected-diagnostics.csv` für `psi-heavy.json`.
  - [ ] `psi-legacy-cache.json` (enthält `uses-long-cache-ttl` mit 120.000 Bytes Einsparung) → `Efficient Cache Policy Savings (Bytes) = 120000`; `psi-heavy.json` (Lighthouse 13, ohne dieses Audit) → Zelle leer, nicht `0`.
  - [ ] `DOM Size` ist eine Ganzzahl (Anzahl DOM-Elemente aus den Audit-Details); `Font Display Savings (ms)` ganzzahlig.
- **Testfixture:** `psi-heavy.json`, `psi-fast.json`, `psi-legacy-cache.json` (Antwort einer älteren Lighthouse-Version).

### INTEG-139 PageSpeed-Tab: Filter
- **Priorität:** P1
- **Beschreibung:** 19 Filter im PageSpeed-Tab. Grundregel: Ein Audit gilt als „nicht bestanden", wenn sein Score < 0,9 ist bzw. (bei Insight-Audits ohne numerischen Score) ein Einsparpotenzial > 0 oder mindestens ein beanstandetes Element vorliegt. Audits mit `scoreDisplayMode` `notApplicable` oder `manual` gelten als bestanden.
- **Eingaben/Konfiguration:** —
- **Ausgabe:** Filter mit Zählern; je Filter Export der beanstandeten Ressourcen (INTEG-144).
- **Akzeptanzkriterien:**
  - [ ] **Document Request Latency:** `document-latency-insight` nicht bestanden (Weiterleitungen beim Dokument, langsame Serverantwort oder fehlende Textkompression).
  - [ ] **LCP Request Discovery:** `lcp-discovery-insight` nicht bestanden (LCP-Bild nicht sofort auffindbar, Lazy-Loading auf LCP-Bild, fehlende `fetchpriority=high`).
  - [ ] **Render Blocking Requests:** `render-blocking-insight` mit Einsparung > 0 ms.
  - [ ] **Network Dependency Tree:** `network-dependency-tree-insight` nicht bestanden (lange kritische Anforderungsketten).
  - [ ] **Use Efficient Cache Lifetimes:** `cache-insight` mit Einsparung > 0 Bytes.
  - [ ] **Layout Shift Culprits:** `cls-culprits-insight` mit mindestens einem Verursacher.
  - [ ] **Improve Image Delivery:** `image-delivery-insight` mit Einsparung > 0 Bytes.
  - [ ] **Forced Reflow:** `forced-reflow-insight` nicht bestanden.
  - [ ] **Legacy JavaScript:** `legacy-javascript-insight` mit Einsparung > 0 Bytes.
  - [ ] **Duplicated JavaScript:** `duplicated-javascript-insight` mit Einsparung > 0 Bytes.
  - [ ] **Avoid Enormous Network Payloads:** `total-byte-weight` nicht bestanden (Score < 0,9).
  - [ ] **Minify CSS:** `unminified-css` mit Einsparung > 0.
  - [ ] **Minify JavaScript:** `unminified-javascript` mit Einsparung > 0.
  - [ ] **Reduce Unused CSS:** `unused-css-rules` mit Einsparung > 0.
  - [ ] **Reduce Unused JavaScript:** `unused-javascript` mit Einsparung > 0.
  - [ ] **Reduce JavaScript Execution Time:** `JavaScript Execution Time Category` ist nicht `Good` (also `Average` oder `Slow`, d. h. Audit-Score < 0,9).
  - [ ] **Minimize Main-Thread Work:** `Minimize Main-Thread Work Category` ist nicht `Good` (Score < 0,9).
  - [ ] **Optimize DOM Size:** `dom-size-insight` nicht bestanden. Lighthouse wertet das DOM dann als problematisch, wenn auf einem umfangreichen DOM eine einzelne Stil- bzw. Layout-Berechnung 40 ms oder mehr benötigt (Schwelle aus der Lighthouse-Dokumentation; OpenSpider übernimmt das Urteil des Audits und rechnet nicht selbst nach).
  - [ ] **Font Display:** `font-display-insight` mit Einsparung > 0 ms (Webfonts, deren `font-display`-Wert Text während des Ladens unsichtbar lässt).
  - [ ] Eine URL ohne PSI-Daten (Fehler) erscheint in keinem dieser Filter.
- **Testfixture:** `psi-heavy.json` (löst alle 19 aus), `psi-fast.json` (löst keinen aus), lokale Seiten `fixtures/integ/psi/` je Problem (`redirect-chain`, `lazy-lcp.html`, `render-block.html`, `no-cache`, `cls.html`, `big-image.html`, `forced-reflow.html`, `legacy-js.html`, `dup-js.html`, `huge-payload.html`, `unminified.html`, `unused.html`, `long-tasks.html`, `huge-dom.html`, `font-swap-missing.html`).

### INTEG-140 Mobile-Prüfungen aus Lighthouse
- **Priorität:** P1
- **Beschreibung:** Datenbeschaffung für den Mobile-Tab: Die Mobile-Prüfergebnisse werden aus Lighthouse gewonnen – remote über die PSI-API oder lokal (Quelle gemäß INTEG-130/132) – und als Spaltenwerte bereitgestellt. **Tab-Population, Spaltennamen, Filterbedingungen (Pass/Fail) und Einstufung sind führend in TECH-230 … TECH-237 spezifiziert**; dieser Eintrag legt nur fest, welche Lighthouse-Audits abgefragt werden, wie deren Ergebnisse in Spaltenwerte umgerechnet werden und was bei fehlenden Audits gilt. Gegen die öffentliche Referenz-Doku verifiziert: Die Prüfspalten sind Lighthouse-Scores 0–100 (kein Pass/Fail-Text), die Spalte für die Schriftgrößen-Prüfung heißt `Font Display`, und die Prüfung der Inhaltsbreite ist nur mit Quelle Lokal verfügbar, weil die Remote-API dieses Audit nicht mehr liefert. Voraussetzung: PSI ist verbunden und die Metrikgruppe Mobile ist gewählt (INTEG-133).
- **Eingaben/Konfiguration:** Gruppe Mobile; Quelle Remote/Lokal (INTEG-130).
- **Ausgabe:** Werte für die Spalten des Mobile-Tabs (Namen und Reihenfolge: TECH-231). Zuordnung Spalte ← Lighthouse-Audit: `Viewport` ← `viewport`; `Tap Targets` ← `target-size` (ersatzweise das ältere `tap-targets`, falls nur dieses geliefert wird); `Content Width` ← `content-width` (nur Quelle Lokal); `Font Display` ← `font-size` (Schriftgrößen-Lesbarkeit, nicht das Font-Display-Insight des PageSpeed-Tabs; der Name bleibt aus Export-Kompatibilität, der Tooltip erklärt den Inhalt); `Plugins` ← `plugins`, sofern die genutzte Lighthouse-Version es liefert, sonst DOM-Erkennung nach TECH-236. `PSI Status`/`PSI Error` gemäß INTEG-143. `Mobile Alternate Link` stammt nicht aus Lighthouse, sondern aus dem gecrawlten HTML (TECH-237).
- **Akzeptanzkriterien:**
  - [ ] Spaltenwert = Audit-Score × 100, kaufmännisch auf eine ganze Zahl gerundet (0–100); ein Remote-Score von 0,5 für `target-size` ergibt `Tap Targets = 50`.
  - [ ] Audits mit `scoreDisplayMode = notApplicable` ergeben `100`; Audits, die die genutzte Lighthouse-Version nicht liefert oder die mit Fehler bzw. `manual` zurückkommen, ergeben eine leere Zelle (nicht `0`).
  - [ ] Mit Quelle Remote und der Antwort `psi-mobile-remote.json` (ohne `content-width`) ist `Content Width` leer; mit Quelle Lokal wird es für `wide-content.html` als `0` und für `ok.html` als `100` befüllt.
  - [ ] Liefert eine Antwort nur `tap-targets` (ältere Lighthouse-Version), wird dessen Score in `Tap Targets` übernommen; liefert sie beide, gilt `target-size`.
  - [ ] Liefert die Antwort kein `plugins`-Audit, wird `Plugins` aus der DOM-Erkennung nach TECH-236 befüllt; liefert sie es, wird dessen Score übernommen.
  - [ ] URLs mit `PSI Status ≠ Success` haben leere Prüfspalten.
  - [ ] Bei abgewählter Gruppe Mobile werden für die Mobile-Audits keine zusätzlichen Kategorien angefragt (Mock-Log) und die Prüfspalten bleiben leer.
- **Testfixture:** `fixtures/integ/psi/mobile/` (Seiten `no-viewport.html`, `viewport-no-width.html`, `small-targets.html`, `big-targets.html`, `wide-content.html`, `tiny-font.html`, `small-footnote.html`, `ok.html`) + Mock-PSI-Antworten `psi-mobile-remote.json` (ohne `content-width`), `psi-mobile-legacy.json` (nur `tap-targets`, mit `plugins`) und lokaler Lauf; die Filtererwartungen je Seite stehen in den Fixtures von TECH-232 … TECH-236.

### INTEG-141 Accessibility-Score aus Lighthouse
- **Priorität:** P2
- **Beschreibung:** Optional wird zusätzlich die Lighthouse-Kategorie Accessibility abgefragt und als Score 0–100 ausgegeben (die detaillierte Barrierefreiheitsprüfung ist in TECH-250 ff. spezifiziert).
- **Eingaben/Konfiguration:** Checkbox „Accessibility-Score abfragen".
- **Ausgabe:** `Accessibility Score`.
- **Akzeptanzkriterien:**
  - [ ] Mit Option enthält der Request `category=accessibility` zusätzlich zu `performance`.
  - [ ] Score = `categories.accessibility.score × 100`.
- **Testfixture:** Mock-Antwort mit beiden Kategorien.

### INTEG-142 PSI-Ratenbegrenzung und Durchsatz
- **Priorität:** P1
- **Beschreibung:** Remote-PSI ist auf 25.000 Anfragen pro Tag und 60 Anfragen pro 100 Sekunden je Schlüssel begrenzt. OpenSpider drosselt automatisch (INTEG-005) und zeigt eine Restzeit-Schätzung. Diese ergibt sich aus der eingestellten Rate: Restzeit = offene URLs ÷ (Anfragen je Fenster ÷ Fensterlänge), mindestens jedoch offene URLs × gemessene mittlere Antwortzeit ÷ gleichzeitige Anfragen. Bei Quelle Lokal gilt kein Tageslimit; die Schätzung nutzt dort nur die gemessene Laufzeit je URL und die Parallelität.
- **Eingaben/Konfiguration:** Erweitert: Anfragen/100 s (Standard 60), gleichzeitige Anfragen (Standard 5).
- **Ausgabe:** Restzeit-Schätzung in der Fortschrittsanzeige.
- **Akzeptanzkriterien:**
  - [ ] Nie mehr als 60 Anfragen in 100 s (Mock-Messung).
  - [ ] Nach 25.000 Anfragen am Tag (lokaler Zähler, Tageswechsel America/Los_Angeles) werden für weitere URLs keine Anfragen mehr gesendet; diese URLs erhalten `PSI Status = Error` und `PSI Error = Quota exceeded: <Meldung>` (INTEG-143); bei Quelle Lokal greift diese Grenze nicht (25.001. lokaler Lauf wird ausgeführt).
  - [ ] Rechenbeispiel ratenbegrenzt: 3.000 offene URLs, 60 Anfragen/100 s, mittlere Antwortzeit 4 s, 5 gleichzeitige Anfragen → max(3.000 ÷ 0,6/s; 3.000 × 4 s ÷ 5) = max(5.000 s; 2.400 s) = 5.000 s; Anzeige „ca. 1 h 23 min" (±5 %).
  - [ ] Rechenbeispiel antwortzeitbegrenzt: 3.000 URLs, mittlere Antwortzeit 20 s, 5 gleichzeitige Anfragen → max(5.000 s; 12.000 s) = 12.000 s; Anzeige „ca. 3 h 20 min" (±5 %).
- **Testfixture:** Mock-PSI mit Zeitmessung, Testuhr.

### INTEG-143 PSI-/Lighthouse-Fehlerbehandlung
- **Priorität:** P1
- **Beschreibung:** „500 Unable to process request" ist häufig vorübergehend und wird mit Backoff wiederholt; bei gehäuftem Auftreten pausiert die Integration 10 Minuten. Lighthouse-Fehler wie `ERRORED_DOCUMENT_REQUEST`, `FAILED_DOCUMENT_REQUEST`, `NO_FCP` sind seitenbedingt und werden nicht automatisch wiederholt.
- **Eingaben/Konfiguration:** —
- **Ausgabe:** Spalten `PSI Status` (genau `Success` | `Error`; leer, solange die URL noch nicht abgefragt wurde – Wertebereich führend TECH-230) und `PSI Error` (Code und Meldung, sonst leer) im PageSpeed- und Mobile-Tab; Filter `PSI Errors` im PageSpeed-Tab. Ein erschöpftes Kontingent (lokaler Tageszähler nach INTEG-142 oder API-Antwort 429 mit Tageskontingent-Grund) ist kein eigener Statuswert, sondern `PSI Status = Error` mit `PSI Error = Quota exceeded: <Meldung>`.
- **Akzeptanzkriterien:**
  - [ ] 500 einmalig → nach Wiederholung Daten vorhanden, `PSI Status = Success`, `PSI Error` leer.
  - [ ] Mehr als 10 aufeinanderfolgende 500er → Integration pausiert 10 Minuten (Testuhr), Hinweis in der UI.
  - [ ] `lighthouseResult.runtimeError.code = ERRORED_DOCUMENT_REQUEST` → `PSI Status = Error`, `PSI Error = ERRORED_DOCUMENT_REQUEST <Meldung>`, keine Wiederholung (Mock-Log: 1 Request).
  - [ ] Filter **PSI Errors** zeigt genau die URLs mit `PSI Status = Error`, einschließlich der URLs mit erschöpftem Kontingent (`PSI Error` beginnt mit `Quota exceeded:`); URLs mit leerem `PSI Status` (noch nicht abgefragt) erscheinen nicht.
  - [ ] Kontingent erschöpft (Mock-Restkontingent 0) → `PSI Status = Error`, `PSI Error` beginnt mit `Quota exceeded:`; der Wert `Quota exceeded` kommt in `PSI Status` nie vor.
  - [ ] URLs mit Fehler sind über INTEG-008/INTEG-009 erneut anforderbar.
- **Testfixture:** Mock-Szenarien `500-once`, `500-burst`, `lh-errored-document`, `quota-exhausted` (Restkontingent 0).

### INTEG-144 Lighthouse-Details (unterer Tab) und Ressourcen-Export
- **Priorität:** P2
- **Beschreibung:** Unterer Tab „Lighthouse Details" zeigt je ausgewählter URL alle Audits mit Score und Anzeigewert und – nach Auswahl eines Audits – dessen beanstandete Ressourcen. Dieser Eintrag legt den Inhalt fest; Layout des Detail-Tabs: UI-046 (führend), Export des Detail-Tabs einer URL: OUT-021 (führend). Der Export der Ressourcenliste je Audit über alle URLs (Menü „Reports → PageSpeed → <Audit>") ist in OUT-114 führend spezifiziert.
- **Eingaben/Konfiguration:** —
- **Ausgabe:** Ressourcentabelle im unteren Tab mit den kanonischen Spalten `Source Page`, `URL`, `Size (Bytes)`, `Potential Savings` (Spaltensatz der Referenz-Oberfläche; in der öffentlichen Doku nicht aufgeführt – **zu verifizieren**). Feldzuordnung aus `details.items`: `URL` ← `url`; `Size (Bytes)` ← `totalBytes` (ersatzweise `transferSize`/`resourceSize`); `Potential Savings` ← `wastedBytes` in Bytes, bei Audits ohne Byte-Angabe `wastedMs` in ms (Einheit im Spaltentooltip). Spalten und Format des Ressourcen-Exports je Audit: OUT-114 (führend).
- **Akzeptanzkriterien:**
  - [ ] Für `psi-heavy.json` listet der Detail-Tab beim Audit „Reduce Unused JavaScript" alle Skripte aus `details.items` mit `Size (Bytes)` und `Potential Savings`.
  - [ ] Bei „Render Blocking Requests" (nur ms-Einsparung je Ressource) zeigt der Detail-Tab den ms-Wert in `Potential Savings` mit Einheit „ms“ im Tooltip.
  - [ ] Items ohne `url` (z. B. Elemente bei Layout-Shift-Verursachern) erscheinen mit dem Element-Selektor bzw. Snippet in `URL` und leerer Größe.
- **Testfixture:** `psi-heavy.json`.

### INTEG-145 Report „PageSpeed Opportunities Summary"
- **Priorität:** P2
- **Verweis:** siehe OUT-104 (führend). Die Einsparwerte stammen aus denselben Audit-Feldern wie die Spalten in INTEG-137/138.

### INTEG-146 Nicht unterstützte Plugins (Mobile)
- **Priorität:** P1
- **Verweis:** siehe TECH-236 (führend). Liefert die genutzte Lighthouse-Version ein `plugins`-Audit, wird dessen Score in die Spalte `Plugins` übernommen, sonst greift die DOM-Erkennung von TECH-236 (Datenbeschaffung: INTEG-140).

### INTEG-147 Mobile Alternate Link
- **Priorität:** P1
- **Verweis:** siehe TECH-237 (führend). Die Spalte wird aus dem gecrawlten HTML befüllt, unabhängig von PSI und der Metrikgruppen-Auswahl (INTEG-133).

---

## J Linkmetriken

### INTEG-150 Ebenen der Linkmetriken
- **Priorität:** P1
- **Beschreibung:** Linkmetriken können je Anbieter auf vier Ebenen abgefragt werden: **Exact URL** (genau diese URL inkl. Protokoll), **Exact URL (HTTP + HTTPS)** (beide Protokollvarianten summiert, sofern der Anbieter das unterstützt), **Subdomain** (Host der URL) und **Domain** (registrierbare Root-Domain nach Public Suffix List). Subdomain- und Domain-Werte werden je Host/Domain nur einmal abgefragt und allen URLs zugeordnet.
- **Eingaben/Konfiguration:** Je Anbieter und Metrik Checkboxen für die Ebenen.
- **Ausgabe:** Spaltenname `<Anbieter> <Metrik> (<Ebene>)`, z. B. `Ahrefs Backlinks (Exact URL)`, `Moz Domain Authority (Domain)`.
- **Akzeptanzkriterien:**
  - [ ] Für 100 URLs auf `www.integ.test` und Domain-Ebene wird genau 1 Domain-Abfrage gestellt; alle 100 URLs erhalten denselben Wert.
  - [ ] `blog.example.co.uk/x` → Domain-Ebene `example.co.uk` (Public Suffix List).
  - [ ] Ebenen, die ein Anbieter nicht anbietet, sind im Dialog deaktiviert.
- **Testfixture:** Mock-Anbieter-APIs mit Aufrufzähler; URL-Liste mit gemischten Hosts.

### INTEG-151 Link-Metrics-Tab
- **Priorität:** P1
- **Beschreibung:** Tab „Link Metrics" mit Basisspalten und den gewählten Metriken aller verbundenen Linkdaten-Anbieter. Linkmetriken werden für interne HTML-URLs abgefragt; optional auch für externe Ziel-URLs (Option „Linkmetriken für externe URLs abfragen", Standard aus).
- **Eingaben/Konfiguration:** —
- **Ausgabe:** `Address`, `Status Code`, `Title 1`, dann Anbieter-Spalten; zusätzlich im Internal-Tab.
- **Akzeptanzkriterien:**
  - [ ] Mit Ahrefs und Moz verbunden erscheinen beide Spaltensätze nebeneinander.
  - [ ] Externe URLs erhalten nur Daten bei aktivierter Option.
  - [ ] Zahlen größer als 2³¹−1 (z. B. Backlinks großer Domains) werden korrekt als 64-Bit-Ganzzahl gespeichert und angezeigt.
- **Testfixture:** Mock-Antwort mit `backlinks: 5000000000`.

### INTEG-152 Majestic
- **Priorität:** P1
- **Beschreibung:** Anbindung über ein Majestic-Zugriffstoken (OpenApps-Freigabe) eines Kontos mit API-Zugang. Wahl des Index: „Fresh" oder „Historic". Metriken: External Backlinks, Referring Domains, Trust Flow, Citation Flow, Referring IPs, Referring Subnets, Indexed URLs, External Backlinks EDU, External Backlinks GOV, Referring Domains EDU, Referring Domains GOV.
- **Eingaben/Konfiguration:** Token, Index (Fresh/Historic), Metriken × Ebenen.
- **Ausgabe:** Spalten `Majestic <Metrik> (<Ebene>)`.
- **Akzeptanzkriterien:**
  - [ ] Die Wahl des Index wird an die API übergeben (Mock prüft Parameter `datasource=fresh|historic`).
  - [ ] Trust Flow und Citation Flow sind Ganzzahlen 0–100.
  - [ ] Abfragen werden gebündelt (mehrere URLs je Anfrage, Standard 100) (INTEG-155).
  - [ ] Ein Konto ohne API-Berechtigung erzeugt beim Verbinden die Meldung des Anbieters.
- **Testfixture:** Mock-Majestic-Endpunkt mit Bündelantwort.

### INTEG-153 Ahrefs (API v3, OAuth)
- **Priorität:** P1
- **Beschreibung:** Anbindung über „Mit Ahrefs anmelden" (OAuth; Schlüsseleingabe nicht vorgesehen), Auswahl des Ahrefs-Workspaces. Nutzbar mit jedem bezahlten Ahrefs-Tarif, der API-Integrationseinheiten enthält; der Verbrauch zählt gegen das monatliche Einheiten-Kontingent. Das Token ist etwa ein Jahr gültig (INTEG-004). Metriken (API v3): Domain Rating, URL Rating, Backlinks, Referring Domains, Referring Pages, Dofollow/Nofollow-Backlinks, Backlinks nach Typ (Text, Bild, Redirect, Canonical), Gov-/Edu-Backlinks und -Domains, Referring IPs, Referring Class-C-Subnetze, Linked Root Domains, Outgoing Links intern/extern, Organic Keywords (gesamt, Top 3, Top 10), Organic Traffic (gesamt, Top 3, Top 10), Traffic Value.
- **Eingaben/Konfiguration:** OAuth-Anmeldung, Workspace, Metriken × Ebenen; Reiter „Optionen" mit Land und Volumenbasis für Traffic-/Keyword-Metriken (INTEG-157).
- **Ausgabe:** Spalten `Ahrefs <Metrik> (<Ebene>)`; verbrauchte Einheiten in der Kontingentanzeige.
- **Akzeptanzkriterien:**
  - [ ] OAuth-Flow über Loopback analog INTEG-003; nach Anmeldung ist der Workspace wählbar.
  - [ ] Liefert die API die verbleibenden Einheiten, werden sie angezeigt; vor einem Crawl mit geschätzt mehr Einheiten als verfügbar erscheint eine Warnung (INTEG-155).
  - [ ] Werte jenseits 32-Bit werden korrekt verarbeitet.
  - [ ] Ein abgelaufenes Token pausiert nur die Ahrefs-Integration.
- **Testfixture:** Mock-Ahrefs-OAuth + Metrik-Endpunkte mit Einheitenzähler.

### INTEG-154 Moz (API v3)
- **Priorität:** P1
- **Beschreibung:** Anbindung mit Moz-API-Zugriffstoken; Auswahl Kontotyp „Free" (begrenzte Metriken, niedrige Rate) oder „Paid" (separates API-Abo). Metriken: Page Authority, Domain Authority, Spam Score, Brand Authority, Link Propensity, Root Domains Linking, External Pages Linking, Pages Linking (gesamt), Nofollow-Root-Domains, Last Crawled.
- **Eingaben/Konfiguration:** Token, Kontotyp, Metriken × Ebenen.
- **Ausgabe:** Spalten `Moz <Metrik> (<Ebene>)`.
- **Akzeptanzkriterien:**
  - [ ] Bei Kontotyp Free sind nur die im freien Kontingent verfügbaren Metriken wählbar; die Rate ist auf einen Free-Grenzwert gedrosselt, Standard 1 Anfrage / 10 s und unter „Erweitert" änderbar **(Annahme, zu verifizieren: Wert stammt aus dem Free-Tarif der älteren Moz-Links-API v2; für v3 ist er weder in der Referenz-Doku noch sicher in der Moz-Doku belegt)**. Prüfung: Mit Standardwert liegen zwischen zwei Free-Anfragen ≥ 10 s (Mock-Zeitstempel); meldet der Mock 429, greift INTEG-005.
  - [ ] Bei Paid werden bis zu 50 URLs je Anfrage gebündelt.
  - [ ] Page Authority und Domain Authority sind Ganzzahlen 1–100; Spam Score in Prozent 0–100 (bzw. leer, wenn nicht ermittelt).
- **Testfixture:** Mock-Moz-v3-Endpunkt (JSON-RPC-artig) mit Free/Paid-Szenarien.

### INTEG-155 Batching und Verbrauchsschätzung
- **Priorität:** P2
- **Beschreibung:** Vor dem Abruf schätzt OpenSpider die Zahl der Anfragen bzw. Einheiten aus URL-Anzahl, Ebenen und Metriken und zeigt sie an. Anfragen werden anbieterabhängig gebündelt.
- **Eingaben/Konfiguration:** —
- **Ausgabe:** Hinweis „Geschätzter Verbrauch: <n> Anfragen / <m> Einheiten".
- **Akzeptanzkriterien:**
  - [ ] Für 250 URLs, Ebene Exact URL, Batchgröße 100 → Schätzung 3 Anfragen; tatsächlich werden 3 Anfragen gestellt.
  - [ ] Übersteigt die Schätzung das bekannte Restkontingent, erscheint eine Bestätigungsabfrage.
- **Testfixture:** Mock-APIs mit Zählern.

### INTEG-156 Erweiterte Linkmetriken (Topics, Anchor-Texte)
- **Priorität:** P2
- **Beschreibung:** Zusätzliche, nicht-numerische Metriken: Majestic Trust-Flow-Topics (Themenkategorien mit Wert) und häufigste Anchor-Texte.
- **Eingaben/Konfiguration:** Metrikauswahl Majestic.
- **Ausgabe:** `Majestic Topical Trust Flow Topic 1..n`, `… Value 1..n`, `Majestic Anchor Text 1..n`.
- **Akzeptanzkriterien:**
  - [ ] Für eine Domain mit 3 Topics werden 3 Topic/Wert-Paare in Rangfolge ausgegeben.
- **Testfixture:** Mock-Majestic-Antwort mit Topics.

### INTEG-157 Ahrefs-Optionen: Land und Volumenbasis
- **Priorität:** P1
- **Beschreibung:** Ein eigener Reiter „Optionen" der Ahrefs-Integration (im Referenzstand) steuert, wie die Traffic- und Keyword-Metriken (Organic Keywords gesamt/Top 3/Top 10, Organic Traffic gesamt/Top 3/Top 10, Traffic Value) berechnet werden: **Land** (alle Länder oder ein bestimmtes Land) und **Volumenbasis** (monatliches Suchvolumen des aktuellsten Monats oder Durchschnittsvolumen über die letzten 12 Monate). Linkmetriken (Backlinks, Domain Rating usw.) sind davon unabhängig. Die Abbildung auf API-Parameter erfolgt gemäß Ahrefs-API-v3-Doku (z. B. Länderparameter als ISO-3166-1-alpha-2-Code, Volumenmodus `monthly`/`average`) **(abgeleitet)**.
- **Eingaben/Konfiguration:** Dropdown „Land" (Standard: „Alle Länder"; durchsuchbare Liste aller von der API unterstützten Länder); Optionsfeld „Suchvolumen" (`Monatlich` | `Durchschnitt`, Standard `Monatlich` **(Annahme, zu verifizieren)**).
- **Ausgabe:** unveränderte Spaltennamen; Spaltentooltip und Tab-Hinweis nennen Land und Volumenbasis (z. B. „Ahrefs: DE, Durchschnittsvolumen").
- **Akzeptanzkriterien:**
  - [ ] Mit Land „Deutschland" enthalten alle Requests für Traffic-/Keyword-Metriken den Länderparameter `de`; Requests für reine Linkmetriken enthalten ihn nicht (Mock-Log).
  - [ ] Mit „Alle Länder" wird kein Länderparameter gesendet.
  - [ ] Volumenbasis „Durchschnitt" setzt den Volumenmodus `average`, „Monatlich" `monthly` (Mock-Log); der Mock liefert je Modus unterschiedliche Traffic-Werte, die korrekt in den Spalten erscheinen.
  - [ ] Eine Änderung von Land oder Volumenbasis nach dem Abruf löst den Dialog aus INTEG-010 aus („Löschen und neu laden" / „Behalten").
  - [ ] Land und Volumenbasis werden im Konfigurationsprofil gespeichert und in CLI-/Zeitplan-Crawls verwendet.
- **Testfixture:** Mock-Ahrefs-v3-Endpunkte, die Länder- und Volumenparameter protokollieren und modusabhängige Werte liefern (`organic_traffic`: monthly 1.200, average 950 für `de`).

---

## K KI-Anbieter & Prompts

### INTEG-160 KI-Anbieterkonten
- **Priorität:** P1
- **Beschreibung:** Vier Anbieter: **OpenAI** (API-Schlüssel eines aufgeladenen API-Kontos, getrennt von ChatGPT-Abos), **Gemini** (API-Schlüssel aus Google AI Studio), **Anthropic** (API-Schlüssel eines bezahlten API-Kontos), **Ollama** (lokal, kein Schlüssel; Server-URL Standard `http://localhost:11434`). Beim Verbinden wird die Modellliste des Anbieters geladen und in den Prompt-Dropdowns angeboten; eine freie Modelleingabe ist möglich.
- **Eingaben/Konfiguration:** Je Anbieter Reiter „Konto" (Schlüssel bzw. Server-URL, Verbinden, „Kontoinformationen" mit Tokenverbrauch, INTEG-178), „Prompts", „Erweitert" (Endpunkte INTEG-161, systemweiter System-Prompt INTEG-177, Ratenlimits INTEG-171, Request-Timeout INTEG-179).
- **Ausgabe:** Status; Modellliste.
- **Akzeptanzkriterien:**
  - [ ] Verbinden lädt die Modellliste (Mock: 5 Modelle) und zeigt sie im Dropdown.
  - [ ] Modellvalidierung: Ist ein in einem gespeicherten Prompt gewähltes Modell nicht (mehr) in der beim Verbinden geladenen Modellliste enthalten, erscheint je betroffenem Prompt eine Warnung „Modell <name> ist beim Anbieter nicht mehr verfügbar" mit Auswahlliste der verfügbaren Modelle; ohne Auswahl bleibt der Prompt unverändert, wird beim Crawlstart aber erneut gemeldet. Frei eingegebene Modelle, die nicht in der Liste stehen, werden genauso gekennzeichnet.
  - [ ] Ollama ohne laufenden Server → `Fehler: Server nicht erreichbar (http://localhost:11434)`.
  - [ ] Mehrere Anbieter können gleichzeitig verbunden und in unterschiedlichen Prompts verwendet werden.
  - [ ] Schlüssel werden gemäß INTEG-002 gespeichert.
- **Hinweis (übernommen aus 08):** OpenRouter ist ab M4 als fünfter, nativer Anbieter verfügbar (AI-005; Schlüssel gemeinsam mit 08 nach AI-009, Eintrag in INTEG-001). Für die Paritäts-Prompts dieses Abschnitts bleibt OpenRouter zusätzlich als OpenAI-kompatibler Endpunkt über INTEG-161 nutzbar; beide Wege teilen sich denselben Schlüssel.
- **Testfixture:** Mock-Endpunkte für Modelllisten aller vier Anbieter; Profil mit einem Prompt auf Modell `retired-model-1`, das in der Mock-Liste fehlt.

### INTEG-161 OpenAI-kompatible und eigene Endpunkte
- **Priorität:** P1
- **Beschreibung:** Im Reiter „Erweitert" lassen sich Server-URL, Pfade (Modellliste, Chat/Completions, Embeddings, bei Ollama zusätzlich Bildgenerierung), zusätzliche HTTP-Header und Modellparameter anpassen; derselbe Reiter enthält den systemweiten System-Prompt (INTEG-177) und den Request-Timeout (INTEG-179). So werden OpenAI-kompatible Anbieter (Azure OpenAI, OpenRouter, DeepSeek, xAI, LM Studio unter `http://localhost:1234`) und private LLM-Gateways nutzbar. Auch Gemini- und Ollama-Endpunkte sind anpassbar.
- **Eingaben/Konfiguration:** Server-URL, Pfad Modellliste, Pfad Completions, Pfad Embeddings, Header-Liste (Name/Wert, Werte als geheim markierbar), API-Schlüssel (darf bei Header-Authentifizierung ein Platzhalter sein).
- **Ausgabe:** —
- **Akzeptanzkriterien:**
  - [ ] Azure-Szenario: Server-URL + Pfade + Header `api-key: <wert>` → Anfragen gehen an den konfigurierten Pfad mit diesem Header (Mock prüft).
  - [ ] LM-Studio-Szenario ohne Schlüssel funktioniert (beliebiger Platzhalter im Schlüsselfeld).
  - [ ] Als geheim markierte Header-Werte werden im Schlüsselbund gespeichert, nicht im Profil.
  - [ ] „Auf Standard zurücksetzen" stellt die offiziellen Endpunkte wieder her.
- **Testfixture:** Mock-OpenAI-kompatibler Server unter abweichendem Pfad.

### INTEG-162 Prompt-Konfiguration
- **Priorität:** P1
- **Beschreibung:** Bis zu 100 Prompts über alle Anbieter. Je Prompt: Name, Anbieter, Modellkategorie, Modell, Inhaltseingabe, Prompt-Text. Die Prompts werden während des Crawls je passender URL ausgeführt; JavaScript-Rendering ist nicht erforderlich. Unbenannte Prompts heißen `<Anbieter>: <n>`.
- **Eingaben/Konfiguration:** Prompt-Tabelle; Schaltflächen Hinzufügen, Aus Bibliothek, Speichern, Testen (▶), Erweitert (Zahnrad), Nach oben/unten, Entfernen.
- **Ausgabe:** je Prompt Spalte + Filter im KI-Tab (INTEG-174).
- **Akzeptanzkriterien:**
  - [ ] Der 101. Prompt wird abgelehnt.
  - [ ] Ein unbenannter OpenAI-Prompt an Position 1 heißt `OpenAI: 1`.
  - [ ] Wird eine Eingabe gewählt, die gespeichertes HTML erfordert (Page Text, HTML), während „HTML speichern" aus ist, erscheint eine Warnung mit Direktlink zur Einstellung.
  - [ ] Prompts laufen nur für interne HTML-URLs mit Status 200 (bzw. für Bild-URLs, wenn die Eingabe Bilder sind).
  - [ ] Pro URL und Prompt wird höchstens eine Anfrage gestellt (außer bei Wiederholung nach Fehler).
- **Testfixture:** `fixtures/integ/ai/site/` (5 Seiten, 3 Bilder) + Mock-Chat-Endpunkte, die den empfangenen Prompt spiegeln.

### INTEG-163 Inhaltseingaben für Prompts
- **Priorität:** P1
- **Beschreibung:** Auswählbare Eingaben: **Page Text** (Text des Inhaltsbereichs, ohne Navigation/Footer gemäß Content-Area-Einstellung), **HTML** (rohes HTML bzw. gerendertes HTML im Rendering-Modus), **Custom Extraction** (Wert eines gewählten Extraktors), **URL Details** (Felder wie Address, Title 1, Meta Description 1, H1-1, Word Count), **Bild** (bei Bild-URLs: das Bild selbst wird übertragen, z. B. für Alt-Text-Generierung), **Keine** (nur Prompt-Text, z. B. mit URL-Platzhalter).
- **Eingaben/Konfiguration:** Dropdown Inhaltseingabe; bei Custom Extraction Extraktor-Auswahl; bei URL Details Feldauswahl.
- **Ausgabe:** —
- **Akzeptanzkriterien:**
  - [ ] Page Text enthält keinen Text aus `<nav>` und `<footer>` (Mock spiegelt Eingabe → Prüfung).
  - [ ] HTML im Rendering-Modus enthält per JS eingefügte Inhalte.
  - [ ] Custom-Extraction-Eingabe übergibt alle Werte des Extraktors, durch Zeilenumbruch getrennt.
  - [ ] Bei Bild-Eingabe werden Bilder als Base64-Inhalt mit korrektem MIME-Typ gesendet (nicht nur als URL, damit auch lokale/Staging-Bilder funktionieren); Bilder > 20 MB werden übersprungen (`Image too large`).
  - [ ] Bei Bild-Eingabe werden im Rendering-Modus auch per JS geladene Bilder berücksichtigt.
- **Testfixture:** `fixtures/integ/ai/site/` mit nav/footer, JS-Inhalt, Bildern (1 × 25 MB).

### INTEG-164 Erweiterter Prompt mit mehreren Zielen und Platzhaltern
- **Priorität:** P2
- **Beschreibung:** Im erweiterten Editor kann ein Prompt mehrere Eingaben kombinieren (z. B. Page Text + Extraktor „Preis" + Title). Platzhalter wie `{{url}}`, `{{title}}`, `{{page_text}}`, `{{extraction:<Name>}}` werden pro URL ersetzt. Optional ein **prompt-spezifischer** System-Prompt; er ergänzt den systemweiten System-Prompt des Anbieters (INTEG-177), Rangfolge dort.
- **Eingaben/Konfiguration:** Editor mit Platzhalter-Einfügen, Feld „System-Prompt (nur dieser Prompt)".
- **Ausgabe:** —
- **Akzeptanzkriterien:**
  - [ ] `Titel: {{title}} / URL: {{url}}` wird pro URL korrekt ersetzt (Mock-Echo).
  - [ ] Unbekannte Platzhalter werden beim Speichern markiert.
  - [ ] Leere Platzhalterwerte werden durch leere Zeichenkette ersetzt; besteht die gesamte variable Eingabe nur aus Leerwerten, greift INTEG-173.
- **Testfixture:** Mock-Echo-Endpunkt.

### INTEG-165 Prompt nur auf Segment anwenden
- **Priorität:** P2
- **Beschreibung:** Ein Prompt kann auf URLs eines Segments beschränkt werden („Match on Segment"; Segmentdefinition führend in CRAWL-134). Segmente können auch auf Issues basieren (z. B. „Bilder ohne Alt-Text"), sodass kostenpflichtige Anfragen nur dort gestellt werden, wo sie nötig sind.
- **Eingaben/Konfiguration:** Dropdown „Segment" (Standard: kein Segment = alle URLs).
- **Ausgabe:** Nicht passende URLs: Zelle leer, Status `Skipped (segment)`.
- **Akzeptanzkriterien:**
  - [ ] Mit Segment „Bilder ohne Alt-Text" werden nur die 2 von 3 Bildern ohne Alt abgefragt (Mock-Zähler = 2).
- **Testfixture:** `fixtures/integ/ai/site/` (3 Bilder, 2 ohne Alt).

### INTEG-166 Modellkategorien
- **Priorität:** P1
- **Beschreibung:** Je Anbieter verfügbare Kategorien: OpenAI – Chat, Moderation, Embeddings, Bildgenerierung, Text-to-Speech; Gemini – Chat, Embeddings, Bildgenerierung, Text-to-Speech; Anthropic – Chat (keine Embeddings); Ollama – Chat, Embeddings, Bildgenerierung (im Referenzstand; nutzt lokal installierte Bildmodelle, Laufzeit stark hardwareabhängig, daher mit einstellbarem Request-Timeout, INTEG-179). Die Kategorie bestimmt Endpunkt und Ergebnisformat.
- **Eingaben/Konfiguration:** Dropdown Kategorie je Prompt.
- **Ausgabe:** Chat → Text; Moderation → `flagged` (true/false) + markierte Kategorien; Embeddings → Vektor (INTEG-180); Bild/TTS → Datei (INTEG-167).
- **Akzeptanzkriterien:**
  - [ ] Das Kategorie-Dropdown enthält je Anbieter genau die oben genannten Kategorien: OpenAI 5, Gemini 4, Anthropic 1, Ollama 3.
  - [ ] Anthropic bietet keine Kategorie Embeddings an; Ollama bietet keine Kategorien Moderation und Text-to-Speech an.
  - [ ] Ein Ollama-Prompt der Kategorie Bildgenerierung sendet die Anfrage an den konfigurierten Ollama-Endpunkt für Bildgenerierung (Pfad aus INTEG-161) und erzeugt eine Bilddatei gemäß INTEG-167.
  - [ ] Moderation-Ergebnis erscheint als `true`/`false` in `<Prompt>` und die Kategorien in `<Prompt> Categories`.
  - [ ] Embedding-Prompts zeigen in der Tabelle nur `Vector (<Dimensionen>)`, nicht den Rohvektor; der Vektor ist im Export „AI:Embeddings" (OUT-069, führend für das Format) als JSON-Array verfügbar.
- **Testfixture:** Mock-Endpunkte je Kategorie.

### INTEG-167 Bild- und Sprachausgabe
- **Priorität:** P2
- **Beschreibung:** Prompts der Kategorien Bildgenerierung und Text-to-Speech erzeugen Dateien, die im Crawl-Ordner gespeichert werden; die Tabelle zeigt den Dateipfad, die Detailansicht eine Vorschau (Bild bzw. Audio-Player).
- **Eingaben/Konfiguration:** Kategorie, Modell, ggf. Bildgröße/Stimme als Modellparameter (INTEG-175).
- **Ausgabe:** Spalte mit Dateipfad; Vorschau.
- **Akzeptanzkriterien:**
  - [ ] Mock-Bildantwort (Base64-PNG) → Datei existiert, Pfad in Zelle, Vorschau zeigt Bild; gilt für OpenAI, Gemini und Ollama gleichermaßen.
  - [ ] Mock-Audioantwort → MP3-Datei, Player in Detailansicht.
  - [ ] Ollama-Bildgenerierung mit 45 s Antwortzeit gelingt bei Request-Timeout 60 s; bei Timeout 30 s → `<Prompt> Status = Error: Timeout nach 30 s`, keine Datei (INTEG-179).
  - [ ] Dateinamen sind eindeutig je (URL, Prompt) und enthalten keine unzulässigen Zeichen (Muster `<prompt-slug>/<sha1(url)>.<ext>`).
- **Testfixture:** Mock-Endpunkte mit Beispiel-PNG und -MP3; Mock-Ollama-Bildendpunkt mit konfigurierbarer Verzögerung (`delay=45s`).

### INTEG-168 Prompt-Bibliothek (mitgeliefert)
- **Priorität:** P1
- **Beschreibung:** OpenSpider liefert eigene Vorlagen je Anbieter. Die Bibliothek ist eine versionierte Datei im Programmpaket (`resources/ai/prompt-library.json`, Felder wie INTEG-169 plus `id`, `minAppVersion`); Standardmodelle je Vorlage werden mit Releases gepflegt und sind nur dort hinterlegt (kein Hartkodieren im Code).
- **Eingaben/Konfiguration:** „Aus Bibliothek hinzufügen" (Reiter „System" / „Benutzer").
- **Ausgabe:** neuer Prompt.
- **Akzeptanzkriterien:**
  - [ ] Der Reiter „System" enthält mindestens diese Vorlagen (ID → Anbieter, Kategorie, Eingabe): `alt-text` → OpenAI, Gemini, Anthropic, Ollama je Chat mit Eingabe Bild (4 Vorlagen); `page-language` → alle 4, Chat, Page Text; `search-intent` → alle 4, Chat, Page Text (Antwort genau eines der Wörter `informational`, `navigational`, `commercial`, `transactional`); `meta-description` → alle 4, Chat, URL Details (Title 1, H1-1) + Page Text, Antwort ≤ 155 Zeichen; `moderation` → OpenAI, Moderation, Page Text; `page-embedding` → OpenAI, Gemini, Ollama, Embeddings, Page Text. Insgesamt also mindestens 20 Vorlagen.
  - [ ] Die Gemini-Embedding-Vorlage setzt den Aufgabentyp `SEMANTIC_SIMILARITY`.
  - [ ] Jede Vorlage läuft gegen den Mock fehlerfrei (Status `Success`); die Antwort der `search-intent`-Vorlagen wird gegen die erlaubte Wortliste geprüft.
  - [ ] Jede Vorlage enthält ein Modell; beim Hinzufügen wird es gegen die Live-Modellliste des verbundenen Anbieters geprüft (INTEG-160). Fehlt es dort, erscheint der Hinweis „Vorlagenmodell nicht verfügbar" mit Auswahl eines verfügbaren Modells; ohne Verbindung wird ohne Prüfung übernommen und beim Verbinden nachgeprüft.
  - [ ] Ein CI-Test validiert `prompt-library.json` gegen ein JSON-Schema (Pflichtfelder, eindeutige IDs je Anbieter, gültige Kategorie-/Eingabe-Kombinationen gemäß INTEG-163/166).
- **Testfixture:** automatischer Test aller Systemvorlagen gegen Mock; Mock-Modellliste ohne das Modell der `alt-text`-Vorlage für den Hinweisfall.

### INTEG-169 Benutzer-Prompts speichern, Import/Export
- **Priorität:** P2
- **Beschreibung:** Eigene Prompts können in der Benutzerbibliothek gespeichert, als JSON exportiert/importiert und als Teil eines Konfigurationsprofils (auch als Standardprofil) gespeichert werden. Schlüssel werden nie mitexportiert.
- **Eingaben/Konfiguration:** Speichern-Symbol; Export/Import.
- **Ausgabe:** JSON (name, provider, category, model, input, prompt, systemPrompt, ignoreProviderSystemPrompt, parameters, segment, version; auf oberster Ebene je Anbieter `providerSystemPrompt`, INTEG-177).
- **Akzeptanzkriterien:**
  - [ ] Export → Import ergibt identische Prompts.
  - [ ] Die Exportdatei enthält keinen API-Schlüssel und keine geheimen Header.
- **Testfixture:** Roundtrip-Test.

### INTEG-170 Prompt-Tester
- **Priorität:** P1
- **Beschreibung:** Über ▶ öffnet sich ein Tester: URL eingeben, „Testen" zeigt die tatsächlich gesendete Eingabe (nach Platzhalterersetzung, Kürzung) und die Antwort des Modells sowie Tokenverbrauch und Laufzeit, sofern der Anbieter sie liefert.
- **Eingaben/Konfiguration:** URL-Feld.
- **Ausgabe:** Eingabe, Antwort, Tokens, ms.
- **Akzeptanzkriterien:**
  - [ ] Die angezeigte Eingabe entspricht byte-genau dem Inhalt, den der Mock empfängt.
  - [ ] Fehler des Anbieters (z. B. 400 „context length exceeded") werden im Tester im Klartext angezeigt.
- **Testfixture:** Mock-Echo-Endpunkt mit Usage-Feldern.

### INTEG-171 Ratenbegrenzung je Modell
- **Priorität:** P1
- **Beschreibung:** Für jeden Anbieter und jedes Modell (getrennt nach Chat und Embeddings) ist die maximale Zahl Anfragen pro Minute (RPM) einstellbar. Vorgaben orientieren sich an den Einstiegs-Bezahlstufen; für Gemini wird darauf hingewiesen, dass die kostenlose Stufe sehr niedrige Limits hat (ca. 5 RPM für Chat-Modelle; Embeddings praktisch ~20 RPM). Kontingentfehler (429) führen zu Backoff (INTEG-005) und einem Hinweis, die RPM zu senken.
- **Eingaben/Konfiguration:** Reiter „Erweitert": RPM je Modell; gleichzeitige Anfragen.
- **Ausgabe:** —
- **Akzeptanzkriterien:**
  - [ ] Bei RPM 20 werden in keinem 60-s-Fenster mehr als 20 Anfragen an dieses Modell gesendet.
  - [ ] Unterschiedliche Modelle desselben Anbieters werden getrennt gedrosselt.
  - [ ] Nach 3 aufeinanderfolgenden 429 für ein Modell erscheint der Hinweis „RPM für <Modell> reduzieren".
- **Testfixture:** Mock mit Zeitprotokoll und 429-Szenario.

### INTEG-172 Inhaltslänge begrenzen
- **Priorität:** P1
- **Beschreibung:** Option „Seiteninhalt begrenzen" kürzt die Inhaltseingabe auf eine Höchstzahl Zeichen (Standard 5.000, je Modell einstellbar), um Token-Limits und Kosten einzuhalten. Überschreitet eine Anfrage dennoch das Kontextlimit, wird der Fehler gespeichert; betroffene URLs lassen sich nach Anpassung gesammelt neu anfordern.
- **Eingaben/Konfiguration:** Checkbox + Zahl (Zeichen) je Modell.
- **Ausgabe:** Fehlerstatus `Error: context length exceeded`.
- **Akzeptanzkriterien:**
  - [ ] Mit Limit 5.000 empfängt der Mock bei einer Seite mit 20.000 Zeichen Text genau 5.000 Zeichen Inhalt (zzgl. Prompt-Text).
  - [ ] Gekürzt wird an einer Unicode-Zeichengrenze (keine zerbrochenen Multibyte-Zeichen).
  - [ ] Kontextfehler des Mocks → Fehlerstatus; Filter „Errors" im KI-Tab listet die URL; Kontextmenü „Fehlgeschlagene neu anfordern" stellt die Anfragen erneut.
- **Testfixture:** `fixtures/integ/ai/long.html` (20.000 Zeichen, Umlaute an Position 4.999/5.000).

### INTEG-173 Leere Eingaben nicht senden
- **Priorität:** P1
- **Beschreibung:** Ist die Inhaltseingabe für eine URL leer (z. B. Extraktor ohne Wert, Seite ohne Text), wird keine Anfrage gesendet, um Kosten zu sparen.
- **Eingaben/Konfiguration:** —
- **Ausgabe:** Status `Skipped (empty input)`.
- **Akzeptanzkriterien:**
  - [ ] Prompt mit Eingabe Custom Extraction auf 3 Seiten, von denen 1 keinen Extraktionswert hat → Mock-Zähler = 2.
- **Testfixture:** `fixtures/integ/ai/site/`.

### INTEG-174 KI-Tab und Ergebnisspalten
- **Priorität:** P1
- **Beschreibung:** Tab „AI" mit Basisspalten und je Prompt einer Ergebnisspalte und einem gleichnamigen Filter. Zusätzlich Filter „Errors" (alle URLs mit Fehlerstatus irgendeines Prompts). Ergebnisse erscheinen auch im Internal-Tab.
- **Eingaben/Konfiguration:** —
- **Ausgabe:** `Address`, `Content Type`, `Status Code`, `Status`, `Indexability`, `Indexability Status`, `<Prompt>`, `<Prompt> Status`.
- **Akzeptanzkriterien:**
  - [ ] Filter „<Prompt>" zeigt alle URLs, für die der Prompt ausgeführt wurde.
  - [ ] Mehrzeilige Antworten bleiben im Export erhalten (korrekt gequotet).
  - [ ] Bild-URLs erscheinen im KI-Tab, wenn ein Prompt mit Bild-Eingabe konfiguriert ist (Content Type `image/*`).
  - [ ] `<Prompt> Status` ∈ {`Success`, `Skipped (empty input)`, `Skipped (segment)`, `Error: …`, `Quota exceeded`}.
- **Testfixture:** `fixtures/integ/ai/site/` + Mock.

### INTEG-175 Modellparameter
- **Priorität:** P2
- **Beschreibung:** Über das Zahnrad eines Modells können beliebige zusätzliche Parameter (Name, Typ Text/Zahl/Boolean/JSON, Wert) in den Request aufgenommen werden, z. B. `temperature`, `max_tokens`, `dimensions` (OpenAI-Embeddings), `outputDimensionality` (Gemini), `reasoning_effort`.
- **Eingaben/Konfiguration:** Parameterliste je Modell.
- **Ausgabe:** —
- **Akzeptanzkriterien:**
  - [ ] Parameter `temperature` (Zahl, 0.2) erscheint als JSON-Zahl im Request-Body (nicht als String).
  - [ ] Parameter vom Typ JSON wird als Objekt eingefügt.
- **Testfixture:** Mock mit Request-Protokoll.

### INTEG-176 Kostenschutz
- **Priorität:** P2
- **Beschreibung:** OpenSpider-eigene Ergänzung: Vor dem Crawlstart wird die Zahl der geplanten KI-Anfragen geschätzt (URLs × Prompts, abzüglich Segment-/Leerausschlüssen soweit bekannt) und angezeigt. Optional eine harte Obergrenze an Anfragen bzw. Tokens je Crawl; bei Erreichen werden weitere KI-Anfragen gestoppt.
- **Eingaben/Konfiguration:** „Max. KI-Anfragen je Crawl", „Max. Tokens je Crawl" (leer = unbegrenzt).
- **Ausgabe:** Hinweis; Status `Quota exceeded` bei Erreichen.
- **Akzeptanzkriterien:**
  - [ ] Mit Obergrenze 3 Anfragen und 5 URLs werden genau 3 Anfragen gesendet; die 2 übrigen URLs erhalten `<Prompt> Status = Quota exceeded`.
  - [ ] Mit Obergrenze 1.000 Tokens je Crawl und Mock-Antworten à 400 Tokens (`usage.total_tokens`) werden 3 Anfragen gesendet (nach der 3. ist die Grenze erreicht bzw. überschritten); danach keine weitere. Grundlage ist dieselbe Zählung wie in INTEG-178 (Crawl-Summe).
- **Testfixture:** Mock mit `usage`-Feldern.

### INTEG-177 Systemweiter System-Prompt je Anbieter
- **Priorität:** P1
- **Beschreibung:** Im Reiter „Erweitert" jedes KI-Anbieters kann ein System-Prompt hinterlegt werden, der für **alle** Chat-Prompts dieses Anbieters gilt (Rolle, Tonfall, Ausgabeformat, Einschränkungen) – im Referenzstand. Er wird im anbieterspezifischen Feld übertragen: OpenAI und OpenAI-kompatibel als Nachricht mit Rolle `system` (bzw. `developer`, wenn der Endpunkt das verlangt), Anthropic als Top-Level-Feld `system`, Gemini als `systemInstruction`, Ollama als Nachricht mit Rolle `system` **(abgeleitet aus den Anbieter-APIs)**. Rangfolge mit dem prompt-spezifischen System-Prompt (INTEG-164): Sind beide gesetzt, wird zuerst der systemweite, dann – durch eine Leerzeile getrennt – der prompt-spezifische Text gesendet **(OpenSpider-Festlegung)**. Für die Kategorien Embeddings, Moderation, Bildgenerierung und TTS wird kein System-Prompt gesendet.
- **Eingaben/Konfiguration:** Mehrzeiliges Feld „System-Prompt (für alle Prompts dieses Anbieters)", Standard leer; je Prompt im erweiterten Editor Checkbox „Systemweiten System-Prompt ignorieren" (Standard aus).
- **Ausgabe:** — (Wirkung im Request; der Prompt-Tester INTEG-170 zeigt den gesendeten System-Prompt an).
- **Akzeptanzkriterien:**
  - [ ] Systemweiter Text „Antworte auf Deutsch." bei OpenAI → jeder Chat-Request enthält als erste Nachricht `{"role":"system","content":"Antworte auf Deutsch."}` (Mock-Log).
  - [ ] Derselbe Text bei Anthropic → Request-Feld `system` = „Antworte auf Deutsch."; bei Gemini → `systemInstruction.parts[0].text`; bei Ollama → erste Nachricht mit Rolle `system`.
  - [ ] Sind systemweiter („A") und prompt-spezifischer („B") System-Prompt gesetzt, lautet der gesendete System-Text `A\n\nB`; mit „Systemweiten System-Prompt ignorieren" nur `B`.
  - [ ] Ein leeres Feld erzeugt keine System-Nachricht (kein leeres `system`-Feld im Request).
  - [ ] Embedding-Requests desselben Anbieters enthalten keinen System-Prompt.
  - [ ] Der systemweite System-Prompt ist Teil des Konfigurationsprofils und des Prompt-Exports (INTEG-169, Feld `providerSystemPrompt`).
- **Testfixture:** Mock-Chat-Endpunkte aller vier Anbieter mit Request-Protokoll; Profil mit systemweitem und prompt-spezifischem System-Prompt.

### INTEG-178 Tokenverbrauch in den Kontoinformationen
- **Priorität:** P1
- **Beschreibung:** Der Dialog „Kontoinformationen" jedes verbundenen KI-Anbieters zeigt den Tokenverbrauch (im Referenzstand). OpenSpider summiert dazu die `usage`-Angaben der Antworten (Eingabe-, Ausgabe- und Gesamt-Tokens; bei Anthropic `input_tokens`/`output_tokens`, bei Gemini `usageMetadata`, bei Ollama `prompt_eval_count`/`eval_count`) **(abgeleitet)** und weist sie getrennt aus für: aktuellen Crawl, aktuelle Sitzung und kumuliert seit Installation bzw. letztem Zurücksetzen – jeweils je Modell. Es handelt sich um lokal gezählte Werte, nicht um die Abrechnung des Anbieters.
- **Eingaben/Konfiguration:** Schaltfläche „Kontoinformationen" im Reiter „Konto"; „Zähler zurücksetzen".
- **Ausgabe:** Tabelle `Model`, `Requests`, `Input Tokens`, `Output Tokens`, `Total Tokens` je Zeitraum (Crawl/Sitzung/Gesamt); Hinweis „lokal gezählt".
- **Akzeptanzkriterien:**
  - [ ] Nach 3 Mock-Antworten mit `usage = {prompt_tokens: 100, completion_tokens: 20}` für Modell `m1` zeigt die Crawl-Zeile für `m1`: `Requests 3`, `Input Tokens 300`, `Output Tokens 60`, `Total Tokens 360`.
  - [ ] Liefert eine Antwort keine `usage`-Angaben, wird der Request gezählt, die Tokens nicht; die Zeile erhält den Hinweis „unvollständig".
  - [ ] Embedding-Requests werden mit ihren Eingabe-Tokens gezählt.
  - [ ] Die kumulierten Zähler überleben einen Programmneustart; „Zähler zurücksetzen" setzt nur „Gesamt" zurück.
  - [ ] Prompt-Tester-Aufrufe (INTEG-170) zählen zur Sitzung und zu „Gesamt", nicht zum Crawl.
- **Testfixture:** Mock-Endpunkte aller vier Anbieter mit festen `usage`-Werten im jeweiligen Anbieterformat, ein Szenario ohne `usage`.

### INTEG-179 Request-Timeout für KI-Anbieter
- **Priorität:** P1
- **Beschreibung:** Je KI-Anbieter ist im Reiter „Erweitert" der Request-Timeout einstellbar. Die Referenz bietet das in einer jüngeren Version ausdrücklich für Ollama an, weil lokale Modelle (insbesondere Bildgenerierung) je nach Hardware sehr lange rechnen; OpenSpider bietet die Einstellung für alle vier Anbieter. Ein Timeout gilt als vorübergehender Fehler und wird nach INTEG-005 wiederholt, bei Bildgenerierung höchstens einmal (teure Läufe).
- **Eingaben/Konfiguration:** „Request-Timeout (Sekunden)": Standard 120 für OpenAI/Gemini/Anthropic, 300 für Ollama **(OpenSpider-Festlegung)**; Bereich 10–3.600.
- **Ausgabe:** Bei Überschreitung `<Prompt> Status = Error: Timeout nach <n> s`.
- **Akzeptanzkriterien:**
  - [ ] Ollama-Chat-Mock mit 200 s Antwortzeit gelingt mit Standardwert (300 s); mit Einstellung 60 s wird nach 60 s (±2 s) abgebrochen.
  - [ ] Nach Timeout eines Chat-Requests folgen bis zu 5 Wiederholungen (INTEG-005); bei Bildgenerierung genau 1 Wiederholung, danach Fehlerstatus.
  - [ ] Werte außerhalb 10–3.600 werden im Dialog abgelehnt.
  - [ ] Der Timeout eines Anbieters beeinflusst andere Anbieter nicht (OpenAI-Mock mit 100 s Antwortzeit gelingt bei OpenAI-Timeout 120 s, auch wenn Ollama auf 30 s steht).
  - [ ] Der Prompt-Tester (INTEG-170) verwendet denselben Timeout und zeigt bei Überschreitung die Timeout-Meldung.
- **Testfixture:** Mock-Endpunkte mit Parameter `delay=<s>` für Chat und Bildgenerierung; Testuhr.

---

## L Embeddings & semantische Analyse

> Dieser Abschnitt ist **führend** für Embeddings: Anbieter und Konfiguration, Berechnung, Logik der Content-Filter „Semantically Similar" und „Low Relevance Content", Regeln, semantische Suche und Cluster. ONPAGE-159 … ONPAGE-164 sind Verweise hierher. Darstellung: UI-044 (Duplicate Details), UI-072 (Seitenleiste Semantic Search), UI-101 (Cluster-Diagramm); Exporte: OUT-052 (Bulk-Exporte Content), OUT-025 (Seitenleiste), OUT-026 (Visualisierungen). **Vergleichsregel einheitlich für alle Einträge:** Ein Ähnlichkeitswert erfüllt eine Schwelle, wenn er **≥** der Schwelle ist; verglichen wird immer der ungerundete Wert.

### INTEG-180 Embedding-Funktionalität aktivieren
- **Priorität:** P1
- **Beschreibung:** Unter den Content-Einstellungen wird „Embedding-Funktionalität aktivieren" eingeschaltet und gewählt, welcher Embedding-Prompt (INTEG-166) als Quelle dient. Anbieter, Modell, Vektordimension (INTEG-187) und Inhaltsbegrenzung (INTEG-172) sind Eigenschaften dieses Prompts. Eingabe ist die Inhaltseingabe des Prompts (INTEG-163); Standard ist der Seitentext des Inhaltsbereichs (ONPAGE-140), gewonnen aus dem rohen HTML im Textmodus bzw. dem gerenderten HTML im Rendering-Modus. Vorbedingungen in den Speicher-Einstellungen: Im Textmodus muss „HTML speichern" aktiv sein; im JavaScript-Rendering-Modus müssen **sowohl** „HTML speichern" **als auch** „Gerendertes HTML speichern" aktiv sein. Teilfunktionen „Semantische Ähnlichkeit" und „Low Relevance" werden einzeln aktiviert. Berechnungen erfolgen in der Crawl-Analyse (Ablauf: CRAWL-141). Vektoren werden je URL in der Crawl-Datei gespeichert (float32).
  - **Inhaltsbegrenzung:** Es gilt die Begrenzung des Prompts (INTEG-172). Für Embedding-Prompts ist sie standardmäßig **aus** (Annahme: der Standardzustand der Referenz ist nicht dokumentiert; ihre Anleitung empfiehlt das Einschalten bei großen Seiten). Ist sie aus, wird der Text ungekürzt gesendet; eine automatische Kürzung auf das Token-Limit des Modells findet nicht statt.
  - **URLs ohne Vektor:** Scheitert die Anfrage (z. B. Kontextlänge überschritten, Kontingent erschöpft, Zeitüberschreitung) oder wird sie übersprungen (leere Eingabe, INTEG-173; Segment, INTEG-165), erhält die URL keinen Vektor. Sie ist dann weder Quelle noch Ziel in INTEG-181, erhält keinen `Semantic Relevance Score` und trägt nicht zum Zentroid bei. Der Grund steht in `<Prompt> Status` (Werte gemäß INTEG-174); eine eigene Spalte `Embedding Status` gibt es nicht. Neuanforderung ohne Recrawl über INTEG-009 bzw. „Fehlgeschlagene neu anfordern" (INTEG-172).
- **Eingaben/Konfiguration:** Checkbox Hauptschalter, Embedding-Quelle (Prompt), Checkboxen Semantic Similarity / Low Relevance, Schwellen (INTEG-181/182), Regeln (INTEG-184). Konfigurationsschlüssel (Konfigurationsprofil und Headless-Konfigurationsdatei): `embeddings.enabled`, `embeddings.source_prompt`, `embeddings.similarity_enabled`, `embeddings.similarity_threshold`, `embeddings.low_relevance_enabled`, `embeddings.low_relevance_threshold`, `embeddings.only_indexable`, `embeddings.rules`, `embeddings.compare_within_rule`, `embeddings.pair_rules`. Anbieter, Modell, Dimension und Inhaltsbegrenzung sind Prompt-Einstellungen und haben keine `embeddings.*`-Schlüssel.
- **Ausgabe:** Spalten und Filter im Content-Tab nach Crawl-Analyse; gespeicherte Vektoren (Export gemäß INTEG-166).
- **Akzeptanzkriterien:**
  - [ ] Textmodus ohne „HTML speichern" → beim Aktivieren erscheint die Warnung „Embeddings benötigt: HTML speichern" mit Direktlink zur Einstellung; mit „HTML speichern" keine Warnung.
  - [ ] Rendering-Modus mit „HTML speichern", aber ohne „Gerendertes HTML speichern" → Warnung „Embeddings benötigt: Gerendertes HTML speichern"; fehlen beide, nennt die Warnung beide Einstellungen.
  - [ ] Wird der Crawl trotz Warnung gestartet, wird keine Embedding-Anfrage gesendet (Mock-Zähler 0), und die Embedding-Filter zeigen den Hinweis „Vorbedingung nicht erfüllt".
  - [ ] Ohne Crawl-Analyse zeigen die Filter „Semantically Similar" und „Low Relevance Content" den Leerzustand `NEEDS_CRAWL_ANALYSIS` (UI-016); ist die jeweilige Teilfunktion aus, zeigen sie `FEATURE_DISABLED`.
  - [ ] Jede URL der Vergleichsmenge erhält genau einen Vektor fester Dimension; eine erneute Crawl-Analyse ruft die API nicht erneut auf (Mock-Zähler unverändert). Ist der gesendete Text einer URL unverändert (Hash des gesendeten Texts), wird auch bei einer Neuanforderung keine Anfrage gestellt.
  - [ ] Eine Seite ohne Text im Inhaltsbereich wird nicht gesendet (`<Prompt> Status = Skipped (empty input)`).
  - [ ] Inhaltsbegrenzung an mit 5.000 Zeichen: Für `long-12000.html` empfängt der Mock genau die ersten 5.000 Codepoints des Texts; `short-3000.html` wird unverändert gesendet. Inhaltsbegrenzung aus: Der Mock empfängt alle 12.000 Zeichen; lehnt er Texte über 8.000 Zeichen mit HTTP 400 ab, lautet der Status `Error: 400 …`, und die URL hat keinen Vektor.
  - [ ] Eine URL ohne Vektor erscheint weder im Filter „Semantically Similar" noch als `Closest Semantically Similar Address` einer anderen URL, hat einen leeren `Semantic Relevance Score`, und der Zentroid ist mit und ohne diese URL identisch.
  - [ ] Der API-Fehler einer URL bricht die Analyse der übrigen URLs nicht ab.
  - [ ] Nach Einschalten der Begrenzung und „Fehlgeschlagene neu anfordern" erhält die zuvor fehlerhafte URL einen Vektor; an die gecrawlte Website geht dabei keine HTTP-Anfrage.
- **Testfixture:** `fixtures/integ/embed/site/` (10 Seiten: 3 nahezu gleiche Texte über Laufschuhe, 1 Seite über Steuerrecht, 6 thematisch passende Seiten) + `fixtures/integ/embed/limits/` (`long-12000.html`, `short-3000.html`, `empty.html`) + Mock-Embedding-Endpunkt mit deterministischen Vektoren (Hash des Texts → fester Vektor; vordefinierte Vektoren für die Fixture-Seiten), konfigurierbarer Längengrenze und Request-Log.

### INTEG-181 Semantische Ähnlichkeit
- **Priorität:** P1
- **Beschreibung:** Für jede URL der Vergleichsmenge wird die Kosinus-Ähnlichkeit (über L2-normalisierte Vektoren, 0–1) zu allen URLs berechnet, mit denen sie nach INTEG-184 verglichen werden darf. Ein Paar gilt als semantisch ähnlich, wenn seine Ähnlichkeit **≥** der Schwelle ist. Standardschwelle 0,95, einstellbar von 0,50 bis 1,00. **(OpenSpider-Festlegung:** Die Referenz beschreibt ähnliche Seiten als „über" der Schwelle und lässt den Grenzfall offen; OpenSpider schließt die Grenze einheitlich ein.)
- **Eingaben/Konfiguration:** Schwelle „Semantische Ähnlichkeit" (`embeddings.similarity_threshold`).
- **Ausgabe:** Content-Tab-Spalten `Closest Semantically Similar Address`, `Semantic Similarity Score`, `No. Semantically Similar`; Filter `Semantically Similar`. Detailansicht: INTEG-183; Bulk-Export „Content → Semantically Similar": OUT-052 (führend).
- **Akzeptanzkriterien:**
  - [ ] Filter **Semantically Similar** zeigt URLs mit mindestens einem Partner, dessen Ähnlichkeit ≥ Schwelle ist. Ein Paar mit exakt 0,9500 (Mock-Vektoren bis auf 1e-9) zählt bei Schwelle 0,95; ein Paar mit 0,9499 zählt nicht.
  - [ ] Eine URL wird nie mit sich selbst verglichen; URLs mit identischer Adresse nach Normalisierung (z. B. nur abweichendes Fragment) gelten als dieselbe URL.
  - [ ] Bei den 3 Laufschuh-Seiten (Fixture-Vektoren mit paarweisen Scores 0,9700 / 0,9650 / 0,9600) zeigt jede Seite `No. Semantically Similar = 2`; `Closest Semantically Similar Address` ist der Partner mit dem höchsten Score, `Semantic Similarity Score` dessen Wert (z. B. Seite A → Partner B mit 0,970).
  - [ ] Gleichstand beim höchsten Score: Es wird die lexikografisch kleinste Adresse gewählt (deterministisch).
  - [ ] URLs ohne Partner ≥ Schwelle haben `No. Semantically Similar = 0`; `Closest Semantically Similar Address` und `Semantic Similarity Score` zeigen trotzdem den nächsten zulässigen Partner und dessen Score (auch unter der Schwelle), damit Grenzfälle sichtbar sind. Hat eine URL nach INTEG-184 gar keinen zulässigen Partner, bleiben beide Zellen leer.
  - [ ] Identische Vektoren ergeben den Score `1.000`.
  - [ ] Bei Schwelle 0,98 ist der Filter für die Laufschuh-Seiten leer; eine Schwellenänderung wirkt nach erneuter Crawl-Analyse ohne neue API-Anfragen.
  - [ ] Scores werden in der Oberfläche mit 3 Nachkommastellen angezeigt (Darstellung gemäß UI-044) und im Export ungerundet (mindestens 6 Nachkommastellen) ausgegeben; Werte außerhalb 0,50–1,00 werden im Schwellenfeld mit Fehlermeldung abgelehnt.
  - [ ] Leistung: Bis 50.000 URLs wird exakt (paarweise) gerechnet und auf alle CPU-Kerne verteilt; 10.000 URLs à 768 Dimensionen benötigen auf der Referenzmaschine (8 Kerne) ≤ 60 s. Über 50.000 URLs wird ein approximativer Nachbarschaftsindex (z. B. HNSW) verwendet; auf dem Benchmark-Datensatz `embed-bench-60k` erreicht er für `Closest Semantically Similar Address` eine Übereinstimmung von ≥ 99 % mit der exakten Rechnung, von den 1.000 eingebetteten Paaren ≥ Schwelle werden mindestens 990 gefunden, und auf der 10-Seiten-Fixture sind die Ergebnisse identisch.
- **Testfixture:** `fixtures/integ/embed/site/` mit vordefinierten Vektoren und erwarteten Scores (`expected-similarity.csv`, inkl. Gleichstand-Paar, Grenzwertpaaren 0,9500 und 0,9499 sowie identischem Vektorpaar); synthetischer Datensatz `fixtures/integ/embed/embed-bench-60k.bin` (60.000 Zufallsvektoren mit eingebetteten Clustern und 1.000 eingebetteten Paaren ≥ 0,95, fester Seed).

### INTEG-182 Low Relevance Content
- **Priorität:** P1
- **Beschreibung:** Aus den L2-normalisierten Vektoren aller URLs der Vergleichsmenge (INTEG-184; Ein-/Ausschlussregeln und „nur indexierbare Seiten" wirken, Paarregeln nicht) wird der Mittelwert (Zentroid) gebildet; für jede URL wird ihre Kosinus-Ähnlichkeit zum Zentroid als „Semantic Relevance Score" ausgegeben (Wertebereich −1 bis 1). URLs unter der Schwelle (Standard 0,4, laut Referenz-Anleitung ebenfalls 0,4) gelten als thematische Ausreißer.
- **Eingaben/Konfiguration:** Schwelle „Low Relevance" (`embeddings.low_relevance_threshold`, 0–1, Standard 0,4).
- **Ausgabe:** Spalte `Semantic Relevance Score`; Filter `Low Relevance Content`; Bulk-Export: OUT-052.
- **Akzeptanzkriterien:**
  - [ ] Filter **Low Relevance Content** zeigt URLs mit `Semantic Relevance Score` < Schwelle; ein Score genau gleich der Schwelle gilt als relevant und steht nicht im Filter (konsistent zur ≥-Regel dieses Abschnitts).
  - [ ] Die Steuerrecht-Seite der Fixture hat einen Score < 0,4 und ist im Filter; alle anderen nicht.
  - [ ] Mock mit 5 nahezu gleichen und 1 orthogonalen Vektor → nur die orthogonale URL im Filter; jeder Score liegt in [−1, 1] und wird mit 3 Nachkommastellen angezeigt.
  - [ ] Der Zentroid wird nur aus URLs der Vergleichsmenge gebildet; URLs außerhalb (ausgeschlossen, nicht indexierbar bei aktivem `embeddings.only_indexable`, ohne Vektor) erhalten keinen Score.
- **Testfixture:** wie INTEG-181, ergänzt um `fixtures/integ/embed/orthogonal/` (5 + 1 Mock-Vektoren).

### INTEG-183 Detailansicht „Semantic Similarity"
- **Priorität:** P1
- **Beschreibung:** Inhalt des Filters „Semantic Similarity" im unteren Tab „Duplicate Details": Für die gewählte URL werden alle Partner mit Ähnlichkeit ≥ Schwelle mit Score absteigend gelistet; daneben steht der an das Modell gesendete Text beider Seiten. Layout und Umschaltung zwischen Near Duplicates und Semantic Similarity: UI-044 (führend). Bulk-Export „Content → Semantically Similar": OUT-052 (führend).
- **Eingaben/Konfiguration:** —
- **Ausgabe:** `Address`, `Semantic Similarity Score`; Textvergleich.
- **Akzeptanzkriterien:**
  - [ ] Für eine Laufschuh-Seite erscheinen die 2 Partnerseiten absteigend nach Score; die Zeilenzahl entspricht `No. Semantically Similar` der Fokus-URL.
  - [ ] Der angezeigte Text ist byte-gleich mit dem gespeicherten Eingabetext des Embeddings; wurde nach INTEG-172 gekürzt, ist das über der Textspalte vermerkt.
  - [ ] Der Bulk-Export nach OUT-052 enthält je Paar und Richtung eine Zeile mit denselben Paaren wie die Detailansicht.
- **Testfixture:** wie INTEG-181.

### INTEG-184 Embedding-Filterregeln
- **Priorität:** P1
- **Beschreibung:** Regeln legen fest, welche URLs in die Analyse eingehen und welche Paare verglichen werden. Zwei Regelarten sind kombinierbar:
  - **Einschluss-/Ausschlussregeln** (Regex auf `Address`) bestimmen die Vergleichsmenge für Ähnlichkeit, Zentroid und Low Relevance. Gibt es mindestens eine Einschlussregel, gehören nur passende URLs dazu; eine Ausschlussregel hat Vorrang vor einer Einschlussregel. Option „nur innerhalb gleicher Regel vergleichen": Ein Paar wird nur verglichen, wenn beide URLs dieselbe Einschlussregel erfüllen (z. B. nur `/blog/` mit `/blog/`).
  - **Quell-/Zielmuster-Paare** (je Regel ein Regex für die Quelle und eines für das Ziel): Gibt es mindestens eine Paarregel, werden nur gerichtete Paare (A, B) verglichen, bei denen A das Quell- und B das Zielmuster derselben Regel erfüllt. Anwendungsfälle: Weiterleitungs-Mapping „alte Struktur → neue Struktur", Staging nur gegen Live. Paarregeln wirken nur auf INTEG-181/183, nicht auf Zentroid und Low Relevance.
  - **Nur indexierbare Seiten** (Standard an) **(OpenSpider-Erweiterung):** Nicht indexierbare URLs gehören nicht zur Vergleichsmenge und werden nicht an den Anbieter gesendet. Die Referenz-Doku beschreibt eine solche Option nur für Duplikate, nicht für Embeddings (Annahme: die Referenz analysiert alle Seiten mit Embedding; der Paritäts-Standard wäre dann „aus").
  - Auswertung: Ein Paar (A, B) wird verglichen, wenn beide URLs zur Vergleichsmenge gehören, eine vorhandene Paarregel für (A, B) zutrifft und – bei aktiver Option – A und B eine gemeinsame Einschlussregel erfüllen.
- **Eingaben/Konfiguration:** `embeddings.rules` (Liste aus `{mode: include|exclude, regex}`), `embeddings.compare_within_rule` (Standard aus), `embeddings.pair_rules` (Liste aus `{source_regex, target_regex}`), `embeddings.only_indexable` (Standard an).
- **Ausgabe:** wirkt auf INTEG-181 … INTEG-186.
- **Akzeptanzkriterien:**
  - [ ] Ausschluss `/tag/` → Tag-Seiten haben keine Scores und erscheinen in keinem Filter.
  - [ ] Einschluss `/blog/` und Ausschluss `/blog/tag/` → `/blog/tag/x` gehört nicht zur Vergleichsmenge (Ausschluss hat Vorrang).
  - [ ] Mit „nur innerhalb gleicher Regel vergleichen" und Einschlussregeln `/blog/`, `/shop/` wird ein Blog-Artikel nie als ähnlich zu einer Shop-Seite gemeldet, auch bei Score 0,99.
  - [ ] Paarregel `^.*/alt/` → `^.*/neu/`: `/alt/x` erhält nur Partner unter `/neu/`; `/neu/y` ist keine Quelle einer Regel und hat daher keinen Closest-Eintrag und `No. Semantically Similar = 0`.
  - [ ] Eine Paarregel ändert keinen `Semantic Relevance Score` (Werte mit und ohne Paarregel identisch).
  - [ ] Ein ungültiger Regex wird beim Speichern der Konfiguration mit Meldung abgelehnt.
  - [ ] Eine `noindex`-Seite erhält mit Standardeinstellung weder eine Embedding-Anfrage noch einen Score; mit `embeddings.only_indexable = false` erhält sie beides.
- **Testfixture:** `fixtures/integ/embed/rules/` (Blog-/Shop-/Tag-Seiten mit vorgegebenen Vektoren, `alt/*.html`, `neu/*.html`, `noindex.html`).

### INTEG-185 Semantische Suche
- **Priorität:** P1
- **Beschreibung:** Ein rechter Seitenbereich „Semantic Search" nimmt eine Freitext-Anfrage entgegen, erzeugt deren Embedding mit demselben Modell wie die Seiten und listet die URLs nach Kosinus-Ähnlichkeit absteigend. Ein Mindestscore blendet schwache Treffer aus; angezeigt werden Treffer mit Score ≥ Mindestscore. Eine Ansicht „Zentroid" zeigt die Seiten nach Nähe zum Website-Mittelpunkt (zentralste und fernste Seiten). Dieser Eintrag ist führend für Logik und Standardwert; Layout der Seitenleiste: UI-072, Export: OUT-025.
- **Eingaben/Konfiguration:** Suchfeld, Mindestscore (0,0–1,0, Standard **0,5** – OpenSpider-Festlegung: die öffentliche Referenz-Doku nennt für die semantische Suche keinen Standardwert; geprüft gegen User Guide, Anleitungen und Release History), Ansicht „Abfrage"/„Zentroid", Embedding-Satz (INTEG-188).
- **Ausgabe:** `Address`, `Score`.
- **Akzeptanzkriterien:**
  - [ ] Frische Installation: Mindestscore 0,5.
  - [ ] Anfrage „Laufschuhe für Marathon" listet die 3 Laufschuh-Seiten oben (Mock-Vektor der Anfrage vordefiniert).
  - [ ] Mindestscore 0,5 blendet alle Treffer < 0,5 aus; ein Treffer mit genau 0,5 bleibt sichtbar. Mindestscore 0,0 zeigt alle URLs mit Vektor.
  - [ ] Ansicht „Zentroid" sortiert nach `Semantic Relevance Score`.
  - [ ] Die Anfrage verbraucht genau 1 Embedding-Request.
- **Testfixture:** wie INTEG-181 + vordefinierter Anfrage-Vektor.

### INTEG-186 Content-Cluster-Diagramm
- **Priorität:** P1
- **Beschreibung:** Visualisierung der Embeddings als 2D-Streudiagramm nach Dimensionsreduktion (t-SNE oder UMAP; Voreinstellungen siehe INTEG-189) mit Clusterfärbung (k-means oder HDBSCAN). Einstellbar: Stichprobengröße, Reduktionsverfahren, Clusterverfahren, Farbschema; Einschränkung auf ein Segment. Per Rechtsklick auf einen Punkt lassen sich die internen Links dieser URL einblenden: **Show Inlinks** (alle eingehenden Links als Linien von den Quell-Punkten), **Show Outlinks** (alle ausgehenden Links zu Ziel-Punkten), **Show Inlinks Within Cluster** (nur eingehende Links von Seiten desselben Clusters – zeigt, ob thematisch verwandte Seiten untereinander verlinken). Eine Legende listet die Cluster mit Farbe und Anzahl; ein Klick auf einen Legendeneintrag blendet den Cluster aus bzw. wieder ein. Ausreißer liegen isoliert am Rand. Dieser Eintrag ist führend für Berechnung, Stichprobe, Clusterbildung, Linkdaten und die diagrammspezifische Interaktion (UI-101 verweist hierher); gemeinsamer Visualisierungs-Rahmen (Fenster, Zoom, Suche, Browser-Modus): UI-090, Export der Visualisierung: OUT-026.
- **Eingaben/Konfiguration:** Zahnrad-Dialog (Stichprobengröße Standard 5.000 URLs, Bereich 100–50.000; Reduktionsverfahren; Clusterverfahren mit k bzw. Mindest-Clustergröße; Farbschema; Zufalls-Seed); Segment-Auswahl; Kontextmenü je Punkt; klickbare Legende.
- **Ausgabe:** interaktives Diagramm; Export PNG/SVG und CSV (`Address`, `x`, `y`, `Cluster`); HDBSCAN-Rauschpunkte erhalten `Cluster = -1` und die Legende „Ausreißer".
- **Akzeptanzkriterien:**
  - [ ] Die 3 Laufschuh-Seiten landen im selben Cluster; die Steuerrecht-Seite in keinem gemeinsamen Cluster mit ihnen.
  - [ ] Mit festem Zufalls-Seed sind Layout und Clusterzuordnung reproduzierbar (byte-identische CSV bei zwei Läufen mit gleichen Einstellungen).
  - [ ] Bei mehr URLs als der Stichprobengröße werden genau so viele URLs zufällig (Seed-gesteuert) gezogen; die CSV hat dann genau Stichprobengröße Zeilen.
  - [ ] Mit Segment „Blog" enthält das Diagramm nur URLs dieses Segments.
  - [ ] **Show Inlinks** auf Seite A zeichnet genau so viele Linien, wie A eindeutige interne Quellseiten im Diagramm hat (Fixture: 4); Quellseiten außerhalb der Stichprobe bzw. des Segments werden nicht gezeichnet, ihre Anzahl erscheint als Hinweis.
  - [ ] **Show Outlinks** auf Seite A zeichnet je eindeutigem internem Ziel im Diagramm eine Linie (Fixture: 3).
  - [ ] **Show Inlinks Within Cluster** auf Seite A zeichnet nur Linien von Quellseiten mit gleicher Cluster-ID (Fixture: 2 von 4); hat A keine solchen Inlinks, erscheint der Hinweis „Keine Inlinks aus demselben Cluster".
  - [ ] Ein Klick auf einen Legendeneintrag blendet alle Punkte dieses Clusters (und dazugehörige Linklinien) aus; ein zweiter Klick blendet sie ein; der Zustand wirkt nicht auf den CSV-Export (der immer alle Punkte enthält).
  - [ ] `[OS-Erweiterung]` Alt-Klick (macOS: Option-Klick) auf einen Legendeneintrag blendet alle übrigen Cluster aus, sodass nur dieser Cluster sichtbar bleibt; ein erneuter Alt-Klick auf denselben Eintrag blendet wieder alle Cluster ein. Die Koordinaten der Punkte ändern sich dabei nicht (keine Neuberechnung).
  - [ ] `[OS-Erweiterung]` Der SVG-/PNG-Export enthält nur die aktuell sichtbaren Cluster (Punkte, Linklinien, Legendeneinträge); Fixture mit 4 Clustern, einer ausgeblendet → Export zeigt 3 Cluster. Der CSV-Export bleibt davon unberührt (siehe vorheriges Kriterium).
  - [ ] `[OS-Erweiterung]` Ausgeblendete Cluster erscheinen in der Legende ausgegraut und werden von der Diagrammsuche und den Tooltips ausgenommen (übernommen aus UI-101 Rev. 2).
  - [ ] OS-Festlegung: Der CSV-Export hat genau die Spalten `Address`, `x`, `y`, `Cluster` (keine Spalte `Visible`, damit das Format und die byte-identische CSV unabhängig vom Legendenzustand bleiben).
  - [ ] `[OS-Erweiterung]` Eine Neuberechnung (Wechsel des Reduktionsverfahrens, eines Parameters oder einer Voreinstellung nach INTEG-189) läuft mit Fortschrittsanzeige im Hintergrund und dauert bei einer Stichprobe von 10.000 Punkten höchstens 30 s auf Referenzhardware (07-performance.md); die Oberfläche bleibt währenddessen bedienbar.
- **Testfixture:** wie INTEG-181, ergänzt um einen Linkgraphen: Laufschuh-Seite A mit 4 internen Inlinks (2 aus dem Laufschuh-Cluster, 2 von anderen Seiten) und 3 internen Outlinks; Segment „Blog" über URL-Präfix `/blog/`.

### INTEG-187 Dimensionsreduktion der Vektoren
- **Priorität:** P1
- **Beschreibung:** Über Modellparameter (`dimensions` bei OpenAI, `outputDimensionality` bei Gemini) können kürzere Vektoren angefordert werden (z. B. 256 statt 1.024 Dimensionen), um Speicher und Analysezeit zu reduzieren. Vektoren werden vor der Kosinus-Berechnung normalisiert, da verkürzte Vektoren nicht zwingend Einheitslänge haben. Nicht zu verwechseln mit der 2D-Projektion für das Cluster-Diagramm und deren Voreinstellungen (INTEG-189).
- **Eingaben/Konfiguration:** Modellparameter (INTEG-175).
- **Ausgabe:** `Vector (256)` in der Prompt-Spalte.
- **Akzeptanzkriterien:**
  - [ ] Parameter `outputDimensionality = 256` erscheint im Request; gespeicherte Vektoren haben 256 Einträge.
  - [ ] Nicht normalisierte Mock-Vektoren ergeben dieselben Scores wie ihre normalisierten Varianten.
- **Testfixture:** Mock mit nicht normalisierten 256-d-Vektoren.

### INTEG-188 Mehrere Embedding-Sätze
- **Priorität:** P1
- **Beschreibung:** Sind mehrere Embedding-Prompts konfiguriert (z. B. Gemini und lokales Ollama-Modell), werden alle gespeichert; in Content-Tab, semantischer Suche und Cluster-Diagramm kann zwischen den Sätzen gewechselt werden. Vektoren verschiedener Modelle werden nie miteinander verglichen.
- **Eingaben/Konfiguration:** Dropdown „Embedding-Satz".
- **Ausgabe:** Spalten/Ansichten je gewähltem Satz.
- **Akzeptanzkriterien:**
  - [ ] Wechsel des Satzes ändert die Scores entsprechend den Vektoren des anderen Modells ohne neue API-Anfragen.
  - [ ] Eine semantische Suchanfrage wird mit dem Modell des gewählten Satzes eingebettet.
- **Testfixture:** zwei Mock-Embedding-Endpunkte mit unterschiedlichen Vektoren.

### INTEG-189 Voreinstellungen für die Dimensionsreduktion
- **Priorität:** P1
- **Beschreibung:** Für das Cluster-Diagramm gibt es Voreinstellungen (im Referenzstand), damit Nutzer ohne Kenntnis der t-SNE-/UMAP-Parameter brauchbare Darstellungen erhalten. OpenSpider definiert drei Presets **(OpenSpider-Festlegung der Parameterwerte)**: „Lokal" (betont kleine, dichte Gruppen), „Ausgewogen" (Standard), „Global" (betont die Gesamtstruktur). Werte: UMAP – Lokal `n_neighbors=5, min_dist=0.0`, Ausgewogen `n_neighbors=15, min_dist=0.1`, Global `n_neighbors=50, min_dist=0.5`; t-SNE – Lokal `perplexity=5`, Ausgewogen `perplexity=30`, Global `perplexity=50`; jeweils Distanzmaß Kosinus. Zusätzlich „Benutzerdefiniert" mit freien Parametern. Das Preset ist unabhängig von der Verkürzung der Vektoren über Modellparameter (INTEG-187).
- **Eingaben/Konfiguration:** Dropdown „Voreinstellung" (Lokal / Ausgewogen / Global / Benutzerdefiniert) im Zahnrad-Dialog; bei „Benutzerdefiniert" Felder `n_neighbors` (2–200), `min_dist` (0,0–0,99), `perplexity` (2–100).
- **Ausgabe:** Diagramm; CSV-Export enthält in einer Kopfzeile Verfahren, Preset und Parameter.
- **Akzeptanzkriterien:**
  - [ ] Frische Installation: Preset „Ausgewogen", Verfahren UMAP.
  - [ ] Auswahl eines Presets setzt die oben genannten Parameter (Unittest auf die an die Reduktion übergebenen Werte).
  - [ ] Eine manuelle Parameteränderung schaltet das Dropdown auf „Benutzerdefiniert"; Rückwahl eines Presets stellt dessen Werte wieder her.
  - [ ] Ist `perplexity` ≥ Anzahl der Punkte, wird sie automatisch auf ⌊(n−1)/3⌋ begrenzt und ein Hinweis gezeigt (10-Seiten-Fixture mit Global → perplexity 3).
  - [ ] Mit festem Seed liefern zwei Läufe mit gleichem Preset identische Koordinaten; verschiedene Presets liefern verschiedene Koordinaten.
- **Testfixture:** `fixtures/integ/embed/site/` (10 Punkte) und `embed-bench-60k` (Stichprobe 5.000) mit festem Seed.

---

## Zählung und offene Punkte

| Priorität | Anzahl | IDs |
|---|---|---|
| P0 | 28 | INTEG-001, 002, 003, 005, 007, 013, 014, 018, 019, 020, 021, 022, 028, 029, 030, 031, 035, 040, 041, 043, 044, 045, 046, 047, 049, 051, 052, 053 |
| P1 | 101 | alle übrigen, sofern nicht P2 |
| P2 | 25 | INTEG-011, 017, 033, 054, 055, 064–070, 083, 126, 141, 144, 145, 155, 156, 164, 165, 167, 169, 175, 176 |

Gesamt: 154 Funktionen mit 603 Akzeptanzkriterien (Verweis-Stubs haben keine eigenen Kriterien). Maßgeblich ist jeweils das Feld **Priorität** des Eintrags; jede ID hat genau eine Priorität.

**Änderungen gegenüber Revision 2 (Architektenentscheidungen zur Spec v1)**
- K-1: Abschnitt L ist führend für Embeddings, alle Einträge INTEG-180 … 189 sind P1. Schwellenvergleich einheitlich „≥" (INTEG-181/183/185). Die Zusatzkriterien aus ONPAGE-159 … 164 (Inhaltsbegrenzung, URLs ohne Vektor, Konfigurationsschlüssel `embeddings.*`, Zentroid aus normalisierten Vektoren, Quell-/Zielmuster-Paare, „nur indexierbare Seiten", Detailansicht) sind in INTEG-180 … 184 übernommen; INTEG-184 enthält beide Regelmodelle. INTEG-189 steht jetzt hinter INTEG-188.
- K-7: INTEG-012 führend für den APIs-Modus (Kriterium aus CRAWL-037 übernommen).
- K-14: INTEG-140 beschreibt nur noch die Datenbeschaffung aus PSI/Lighthouse; Tab, Spalten und Filter führend in TECH-230 … 237. INTEG-146 → Verweis auf TECH-236 (P1), INTEG-147 → Verweis auf TECH-237 (P1).
- K-16: Mindestscore der semantischen Suche 0,5 (INTEG-185); die öffentliche Referenz-Doku nennt keinen Standardwert.
- Grundregel: INTEG-016 → CRAWL-122/OUT-269/OUT-270, INTEG-017 → OUT-201/OUT-164, INTEG-034 → OUT-060, INTEG-145 → OUT-104 als Verweis-Stubs; Querverweise auf führende Einträge in INTEG-012, 015, 027, 053, 100, 126, 133, 141, 144, 165, 186 und in den Konventionen (Crawl-Analyse CRAWL-141, Speichermodus CRAWL-124).

- Nacharbeit reviews-open-v1: INTEG-017 (Stub) P2 → P1 wie OUT-201; INTEG-130 legt die Standardquelle `remote` fest (OS-Festlegung); INTEG-144 verweist für den Ressourcen-Export je Audit auf OUT-114; INTEG-186 um ausgeblendete Cluster (Legende, Suche, Tooltips) und die OS-Festlegung „keine Spalte `Visible`“ ergänzt; Vektor-Export führend in OUT-069; Regex-Dialekt nach Konvention 01 (`fancy-regex` mit Schrittlimit); Auth-Profile führend OUT-223.

**Offene Punkte / bewusst abgeleitete Annahmen**
- GSC-Suchtyp, Dimensionsfilter und „vorläufige Daten" (INTEG-111–113) sind in der Referenz-Doku nicht im Detail beschrieben und wurden aus den Fähigkeiten der Search Console API abgeleitet.
- Die Standard-Linkpositionsregeln (INTEG-081) sind eine OpenSpider-Festlegung; die Referenz nennt nur das Prinzip (Teilstring-Regeln, Content als Auffangregel).
- Lighthouse-Audit-IDs (INTEG-137–139) beziehen sich auf Lighthouse 13; vor Implementierung gegen die gebündelte Lighthouse-Version prüfen.
- Die Ahrefs- und Moz-Metrikkataloge (INTEG-153/154) folgen den aktuellen API-Versionen v3; die exakte Liste wird aus der Anbieter-Doku zum Implementierungszeitpunkt übernommen.
- Mobile-Usability-Daten der URL Inspection API sind seit Einstellung des Google-Berichts möglicherweise leer; Spalten/Filter bleiben für Parität erhalten (INTEG-123/125).
- Website-Authentifizierung (Basic/Digest, Formular-Login, Auth-Profile) ist in CRAWL-117 … CRAWL-120 und OUT-223 (Auth-Profile) spezifiziert; hier nur referenziert (INTEG-132).
- PSI-Gruppenzuordnung (INTEG-133/137/138): Die Referenz-Doku ordnet DOM Size, Font Display und Network Payload je nach Kapitel unterschiedlichen Gruppen zu; OpenSpider folgt der Metrikauswahl-Doku (DOM Size/Font Display → Diagnostics, Avoid Enormous Network Payloads → Insights).
- Mobile-Tab (INTEG-140): gegen die öffentliche Referenz-Doku verifiziert – Prüfspalten sind Lighthouse-Scores 0–100, Spaltenname `Font Display`, `Content Width` nur mit lokaler Quelle. Die Referenz-Doku nennt nur „Content Not Sized Correctly" als lokal-exklusiv; ob das Schriftgrößen-Audit remote noch geliefert wird, ist bei Implementierung gegen die gebündelte Lighthouse-Version zu prüfen (INTEG-140 lässt die Spalte leer, wenn das Audit fehlt).
- Embeddings (INTEG-181): Die Referenz-Anleitung beschreibt ähnliche Seiten als „über" der Schwelle; OpenSpider schließt den Grenzwert bewusst ein (Architektenentscheidung K-1). Die Anzeige des nächsten Partners auch unter der Schwelle stützt sich auf die Referenz-Anleitung (Zähler 0, bester Kandidat weiterhin sichtbar).
- „Nur indexierbare Seiten" für Embeddings (INTEG-184) ist eine OpenSpider-Erweiterung; Paritätsstandard per Black-Box-Test klären.
- Export der Ressourcenliste je PageSpeed-Audit ist in OUT-114 übernommen (INTEG-144 verweist dorthin).
- Zu verifizieren: Spaltensatz des Lighthouse-Details-Tabs (INTEG-144), Standard-Volumenbasis der Ahrefs-Optionen (INTEG-157), Moz-Free-Rate (INTEG-154), Release-Zuordnung der GA4-Kompatibilitätsprüfung (INTEG-101), fehlende Untergrenze des GA4-Startdatums (INTEG-091).
- OpenSpider-Festlegungen ohne Referenzvorgabe: Rangfolge systemweiter/prompt-spezifischer System-Prompt (INTEG-177), Standard-Timeouts der KI-Anbieter (INTEG-179), Parameterwerte der Reduktions-Presets (INTEG-189), anbieterneutrale statt markengebundener Snippet-Vorlagen (INTEG-069).
