# 03 – Analyse-Tabs II: Technisch (ID-Präfix `TECH`)

Clean-Room-Spezifikation. Grundlage: öffentliche Benutzerdokumentation, Issue-Bibliothek, Tutorials und Release-Notes mehrerer Hauptversionen des Referenzprodukts, alles in eigenen Worten beschrieben. Es wurde weder die installierte App noch ihr Code angesehen. Wo die Referenz-Dokumentation keine Zahl oder Logik nennt, legt diese Spec eine **OpenSpider-Festlegung** fest; solche Stellen sind mit „(Festlegung)“ markiert und in Anhang A gesammelt. Sie werden gegen Golden-Exporte geprüft, bevor sie als „verifiziert“ gelten.

Prioritäten: **P0** = MVP-Kern, **P1** = für Parität nötig, **P2** = später/Nische.
Issue-Einstufung: Jeder Filter, der ein Problem anzeigt, trägt eine Art (**Fehler** / **Warnung** / **Chance**, Katalogschlüssel `Issue` / `Warning` / `Opportunity`) und eine Dringlichkeit (**Hoch** / **Mittel** / **Niedrig**, Schlüssel `High` / `Medium` / `Low`). Diese Werte speisen den Issues-Tab. **Führend für die Entscheidungslogik ist UI-063** (eigene Regelcodes `T-*`/`P-*`, Clean-Room, keine 1:1-Abbildung der Referenz). Die Einstufungen dieses Dokuments sind die Katalogeinträge für die TECH-Filter; Anhang B begründet sie bereichsspezifisch und ordnet die Rubrik den UI-063-Codes zu. Die Issue-Bibliothek des Referenzprodukts dient nur als Abgleich im Paritätstest; Abweichungen sind zulässig. Es wird keine Einstufungstabelle aus der Referenz als Datenbestand übernommen.

**Funktionsbezeichnungen:** Tab-, Filter-, Spalten- und Exportnamen (englisch, z. B. „Multiple Conflicting“) werden als **funktionale Bezeichner** verwendet, weil Nutzer und Paritätstests (Golden-Exporte, Spalten-Mapping) sie wiedererkennen müssen. Sie sind kurze Funktionsnamen, keine übernommenen Beschreibungstexte; alle Beschreibungen, Bedingungen und Meldungstexte sind eigene Formulierungen. Reihenfolge und Gruppierung der Filter folgen der Referenz nur, soweit das für die Wiedererkennbarkeit nötig ist; die UI-Spec darf sie ändern. Wo die Referenz für dieselbe Funktion mehrere Namen verwendet (Tab-Doku vs. Report-Menü vs. Issue-Bibliothek), führt die jeweilige Funktion alle Varianten als **Mapping-Aliase** für den Paritätstest auf.

**Paritätsabweichungen** (bewusst anderes Verhalten als die Referenz) sind im Akzeptanzkriterium mit „(Paritätsabweichung)“ gekennzeichnet und in Anhang A gesammelt; Golden-Vergleiche schließen die betroffenen Spalten/Filter aus oder erwarten die dokumentierte Differenz.

**Änderungsstand (Spec v1, Architektenentscheidungen 2026-10-13):** Grundregel „eine Quelle je Funktion“ angewandt. Nicht führende Duplikate sind zu Verweis-Stubs gekürzt (IDs bleiben bestehen, Priorität = die des führenden Eintrags): TECH-009 → CRAWL-089, TECH-024 → CRAWL-092, TECH-042 → CRAWL-093, TECH-067 → CRAWL-091 (Respect-Schalter), TECH-306 → OUT-146 (URL-Mapping), TECH-308 → OUT-152 (Auto Compare, K-2; ohne Speichermodus-Bedingung). Bereichsspezifische Zusatzkriterien der gekürzten Einträge stehen jetzt in TECH-011, TECH-031 und TECH-051. Führend in diesem Dokument und um die Kriterien der Duplikate ergänzt: TECH-008 (HTML-Validierung, aus CRAWL-138), TECH-068 (Self Referencing Meta Refresh, K-12), TECH-114 (Logik JS-Fehler, K-13; Schalter CRAWL-084, Detail-Tab UI-040), TECH-145 (Link Score inkl. formaler Definition aus UI-081), TECH-231 ff. (Mobile-Tab, K-14; gegen die öffentliche Doku geprüft; INTEG-140 ff. liefern nur die Daten), TECH-259 (Inhalt Accessibility Details, K-15; Layout UI-048), TECH-292 (Green Hosting, aus CRAWL-139), TECH-300 ff. (Change Detection, K-3; Elementliste und Standards hier festgelegt, gegen die öffentliche Doku geprüft; OUT-144/OUT-145 und CRAWL-033 verweisen hierher, der Compare-Ablauf bleibt in OUT-140 ff.). TECH-005 ist auf die Filterzustände dieses Dokuments begrenzt (Ablauf CRAWL-141, Teilschritte/Darstellung UI-080). TECH-142 verweist auf den neuen Bulk-Export OUT-068 (G-1). Issue-Einstufung folgt UI-063 (K-10, Anhang B). Nachtrag Fable-Gegenprüfung (R-05, R-16): TECH-230 ist an INTEG-130/INTEG-132 angeglichen (Quelle „Lokal“ = lokaler Lighthouse-Lauf, Standardquelle in INTEG-130, Score-Umrechnung INTEG-140) und führt nur noch Tab, Filter und Spalten. Report- und Exportangaben sind nach der Grundregel zu Verweisen auf 06 gekürzt: TECH-025 → OUT-086/OUT-087, TECH-088 → OUT-091 … OUT-096, TECH-190 → OUT-100/OUT-101 (Verweis-Stubs); TECH-036/037, TECH-219, TECH-231 und TECH-259 verweisen für Namen und Spalten ihrer Exporte auf OUT-088/OUT-089, OUT-057, OUT-108 bzw. OUT-062/OUT-109 und behalten nur die Zeilenauswahl. Die Kettenermittlung aus TECH-025 steht neu in TECH-026.

**Nacharbeit reviews-open-v1 (2026-10-13):** Change Detection nach Architektenentscheidung: Indexability-Status- **und** Grundwechsel zählen als Änderung (TECH-304), strukturierte Daten werden als Typ-Menge + Anzahl verglichen (TECH-303, neue Spalten `Unique Type Names`), Content Change P1 mit plattformübergreifendem Golden-Test (TECH-305). Verweis-Stubs auf das strikte Format „siehe <ID> (führend). <ein Satz>“ normiert (TECH-024, 025, 042, 067, 088, 190, 308). TECH-259-Feldnamen gelten exakt in OUT-021/062/109; `Incomplete`-Export jetzt in OUT-062. TECH-214 legt die Spalte `Found Via` fest. TECH-292 um die CO2.js-Toleranz 0,1 % ergänzt. TECH-145 Priorität P1 bestätigt (OS-Festlegung).

ID-Blöcke (Lücken sind Absicht, damit später eingefügt werden kann):

| Block | Bereich |
|---|---|
| TECH-001 … 009 | Gemeinsame Grundlagen |
| TECH-010 … 029 | Canonicals |
| TECH-030 … 049 | Pagination |
| TECH-050 … 069 | Directives |
| TECH-070 … 099 | Hreflang |
| TECH-100 … 129 | JavaScript |
| TECH-130 … 159 | Links |
| TECH-160 … 179 | AMP |
| TECH-180 … 209 | Structured Data |
| TECH-210 … 229 | Sitemaps |
| TECH-230 … 249 | Mobile |
| TECH-250 … 279 | Accessibility |
| TECH-280 … 299 | Validation |
| TECH-300 … 319 | Change Detection |
| TECH-320 … 339 | Social (Nachtrag 09.11.2026, 1.x) |

## Inhaltsverzeichnis

**Gemeinsame Grundlagen**
- TECH-001 Indexierbarkeitsmodell (P0)
- TECH-002 URL-Vergleich und Auflösung für technische Verweise (P0)
- TECH-003 Head-Zugehörigkeit von Elementen („Outside \<head>“) (P0)
- TECH-004 Mehrwertige Spalten „… 1, … 2, …“ (P0)
- TECH-005 Live-Filter vs. Nachlauf-Analyse („Crawl-Analyse erforderlich“) (P0)
- TECH-006 Rohes vs. gerendertes HTML als Datenquelle (P1)
- TECH-007 Fixture-Konventionen für diesen Bereich (P0)
- TECH-008 Option „Perform HTML Validation“ (Validation- und „Outside \<head>“-Filter) (P1)
- TECH-009 Option „Ignore Non-Indexable URLs for Issues“ (P1, Verweis → CRAWL-089)

**Canonicals**
- TECH-010 Canonical-Extraktion (Link-Element und HTTP-Header) (P0)
- TECH-011 Tab „Canonicals“: Population und Spalten (P0)
- TECH-012 Filter „Contains Canonical“ (P0)
- TECH-013 Filter „Self Referencing“ (P0)
- TECH-014 Filter „Canonicalised“ (P0)
- TECH-015 Filter „Missing“ (P0)
- TECH-016 Filter „Multiple“ (P0)
- TECH-017 Filter „Multiple Conflicting“ (P0)
- TECH-018 Filter „Non-Indexable Canonical“ (P0)
- TECH-019 Filter „Canonical Is Relative“ (P1)
- TECH-020 Filter „Unlinked“ (P1)
- TECH-021 Filter „Invalid Attribute In Annotation“ (P1)
- TECH-022 Filter „Contains Fragment URL“ (P1)
- TECH-023 Filter „Outside \<head>“ (Canonical) (P1)
- TECH-024 Canonicals crawlen/speichern und „Respect Canonical“ (P1, Verweis → CRAWL-092)
- TECH-025 Report „Canonical Chains“ und Export „Non-Indexable Canonicals“ (P1, Verweis → OUT-086/OUT-087)
- TECH-026 Canonical-Ketten: Ermittlung und Zeilenauswahl (P1)

**Pagination**
- TECH-030 Extraktion rel="next"/rel="prev" (P1)
- TECH-031 Tab „Pagination“: Population und Spalten (P1)
- TECH-032 Filter „Contains Pagination“ (P1)
- TECH-033 Filter „First Page“ (P1)
- TECH-034 Filter „Paginated 2+ Pages“ (P1)
- TECH-035 Filter „Pagination URL Not In Anchor Tag“ (P1)
- TECH-036 Filter „Non-200 Pagination URLs“ (P1)
- TECH-037 Filter „Unlinked Pagination URLs“ (P1)
- TECH-038 Filter „Non-Indexable“ (Pagination) (P1)
- TECH-039 Filter „Multiple Pagination URLs“ (P1)
- TECH-040 Filter „Pagination Loop“ (P1)
- TECH-041 Filter „Sequence Error“ (P1)
- TECH-042 Pagination crawlen/speichern, „Respect Next/Prev“ und Reports (P2, Verweis → CRAWL-093)

**Directives**
- TECH-050 Directive-Extraktion (Meta Robots, X-Robots-Tag, Meta Refresh) (P0)
- TECH-051 Tab „Directives“: Population und Spalten (P0)
- TECH-052 Filter „Index“ (P0)
- TECH-053 Filter „Noindex“ (P0)
- TECH-054 Filter „Follow“ (P0)
- TECH-055 Filter „Nofollow“ (P0)
- TECH-056 Filter „None“ (P0)
- TECH-057 Filter „NoArchive“ (P1)
- TECH-058 Filter „NoSnippet“ (P1)
- TECH-059 Filter „Max-Snippet“, „Max-Image-Preview“, „Max-Video-Preview“ (P1)
- TECH-060 Filter „NoODP“, „NoYDIR“ (Altlasten) (P2)
- TECH-061 Filter „NoImageIndex“ (P1)
- TECH-062 Filter „NoTranslate“ (P1)
- TECH-063 Filter „Unavailable_After“ (P1)
- TECH-064 Filter „Refresh“ (P0)
- TECH-065 Filter „Outside \<head>“ (Meta Robots) (P1)
- TECH-066 Konfliktauflösung widersprüchlicher Direktiven (P0)
- TECH-067 „Respect Noindex“ und „Follow Internal/External Nofollow“ (P1, Verweis → CRAWL-091)
- TECH-068 Option „Respect Self Referencing Meta Refresh“ (P1)

**Hreflang**
- TECH-070 Hreflang-Extraktion (HTML, HTTP-Header, XML-Sitemap) (P0)
- TECH-071 Tab „Hreflang“: Population und Spalten (P0)
- TECH-072 Validierung von Sprach-/Regionscodes (P0)
- TECH-073 Filter „Contains Hreflang“ (P0)
- TECH-074 Filter „Non-200 Hreflang URLs“ (P0)
- TECH-075 Filter „Unlinked Hreflang URLs“ (P1)
- TECH-076 Filter „Missing Return Links“ (P0)
- TECH-077 Filter „Inconsistent Language & Region Return Links“ (Alias „… Confirmation Links“) (P1)
- TECH-078 Filter „Non-Canonical Return Links“ (P1)
- TECH-079 Filter „Noindex Return Links“ (P1)
- TECH-080 Filter „Incorrect Language & Region Codes“ (P0)
- TECH-081 Filter „Multiple Entries“ (P1)
- TECH-082 Filter „Missing Self Reference“ (P1)
- TECH-083 Filter „Not Using Canonical“ (P1)
- TECH-084 Filter „Missing X-Default“ (P1)
- TECH-085 Filter „Missing“ (Hreflang) (P0)
- TECH-086 Filter „Outside \<head>“ (Hreflang) (P1)
- TECH-087 Hreflang crawlen, externe Domains als intern (CDN-Liste) (P1)
- TECH-088 Hreflang-Reports (Bulk-Exporte je Problemtyp) (P1, Verweis → OUT-091 … OUT-096)
- TECH-089 Untere Detailansicht „URL Details“ mit Bestätigungsstatus je Annotation (P1)
- TECH-090 Filter „Self Reference Only“ (P1, Nachtrag 09.11.2026, 1.x)
- TECH-091 Filter „Hreflang Language Differs From HTML Lang“ (P1, Nachtrag 09.11.2026, 1.x)

**JavaScript**
- TECH-100 Vergleich Original-HTML vs. gerendertes HTML (Grundlage) (P1)
- TECH-101 Tab „JavaScript“: Population und Spalten (P1)
- TECH-102 Filter „Pages With Blocked Resources“ (P1)
- TECH-103 Filter „Contains JavaScript Links“ (P1)
- TECH-104 Filter „Contains JavaScript Content“ (P1)
- TECH-105 Filter „Noindex Only In Original HTML“ (P1)
- TECH-106 Filter „Nofollow Only In Original HTML“ (P1)
- TECH-107 Filter „Canonical Only In Rendered HTML“ (P1)
- TECH-108 Filter „Canonical Mismatch“ (P1)
- TECH-109 Filter „Page Title Only In Rendered HTML“ / „Page Title Updated By JavaScript“ (P1)
- TECH-110 Filter „Meta Description Only In Rendered HTML“ / „… Updated By JavaScript“ (P1)
- TECH-111 Filter „H1 Only In Rendered HTML“ / „H1 Updated By JavaScript“ (P1)
- TECH-112 Filter „Uses Old AJAX Crawling Scheme URLs“ (P2)
- TECH-113 Filter „Uses Old AJAX Crawling Scheme Meta Fragment Tag“ (P2)
- TECH-114 Filter „Pages With JavaScript Errors“ und Konsolen-Log (P1)

**Links**
- TECH-130 Tab „Links“: Population und Spalten (P0)
- TECH-131 Link-Attribute: Follow, Anchor, Link-Pfad, Crawlability (P0)
- TECH-132 Filter „Pages With High Crawl Depth“ (P0)
- TECH-133 Filter „Pages Without Internal Outlinks“ (P0)
- TECH-134 Filter „Internal Nofollow Outlinks“ (P0)
- TECH-135 Filter „Internal Outlinks With No Anchor Text“ (P0)
- TECH-136 Filter „Non-Descriptive Anchor Text In Internal Outlinks“ (P1)
- TECH-137 Filter „Pages With High External Outlinks“ (P1)
- TECH-138 Filter „Pages With High Internal Outlinks“ (P1)
- TECH-139 Filter „Follow & Nofollow Internal Inlinks To Page“ (P1)
- TECH-140 Filter „Internal Nofollow Inlinks Only“ (P1)
- TECH-141 Filter „Outlinks To Localhost“ (P1)
- TECH-142 Filter „Non-Indexable Page Inlinks Only“ (P1)
- TECH-143 Filter „Pages With Uncrawlable Internal Outlinks“ (P1)
- TECH-144 Link-Präferenzen (Schwellwerte, Anchor-Liste) (P0)
- TECH-145 „% of Total“ und Link Score als Spalten (P1)
- TECH-146 Aggregierte Ankertext-Ansicht („Anchors“) in Inlinks/Outlinks (P1)
- TECH-147 Filter „Pages With Empty Anchor Tags“ (P1, Nachtrag 09.11.2026, 1.x)

**AMP**
- TECH-160 AMP-Erkennung und Crawl-Optionen (P2)
- TECH-161 Tab „AMP“: Population und Spalten (P2)
- TECH-162 AMP-SEO-Filter (Non-200, Return Link, Canonical, Indexierbarkeit) (P2)
- TECH-163 AMP-Validierungsfilter (P2)

**Structured Data**
- TECH-180 Extraktion JSON-LD (P1)
- TECH-181 Extraktion Microdata (P1)
- TECH-182 Extraktion RDFa (P1)
- TECH-183 Tab „Structured Data“: Population und Spalten (P1)
- TECH-184 Filter „Contains Structured Data“ / „Missing Structured Data“ (P1)
- TECH-185 Filter „JSON-LD URLs“, „Microdata URLs“, „RDFa URLs“ (P1)
- TECH-186 Filter „Parse Errors“ (P1)
- TECH-187 Schema.org-Validierung und Filter „Validation Errors“/„Validation Warnings“ (P1)
- TECH-188 Google-Rich-Result-Validierung und Filter „Rich Result Validation Errors“/„… Warnings“ (P1)
- TECH-189 Detailansicht „Structured Data Details“ (P1)
- TECH-190 Reports „Validation Errors & Warnings“ (+ Summary) (P1, Verweis → OUT-100/OUT-101)

**Sitemaps**
- TECH-210 Sitemap-Einbindung in den Crawl (P0)
- TECH-211 Tab „Sitemaps“: Population und Spalten (P0)
- TECH-212 Filter „URLs In Sitemap“ (P0)
- TECH-213 Filter „URLs Not In Sitemap“ (P0)
- TECH-214 Filter „Orphan URLs“ (P0)
- TECH-215 Filter „Non-Indexable URLs In Sitemap“ (P0)
- TECH-216 Filter „URLs In Multiple Sitemaps“ (P1)
- TECH-217 Filter „XML Sitemap With Over 50k URLs“ (P1)
- TECH-218 Filter „XML Sitemap With Over 50MB“ (P1)
- TECH-219 Sitemap-Inlinks und Bulk-Exporte (P1)

**Mobile**
- TECH-230 Mobile-Prüfungen: Ausführungsweg (lokal/PSI) (P1)
- TECH-231 Tab „Mobile“: Population und Spalten (P1)
- TECH-232 Filter „Viewport Not Set“ (P1)
- TECH-233 Filter „Target Size“ (P1)
- TECH-234 Filter „Content Not Sized Correctly“ (P1)
- TECH-235 Filter „Illegible Font Size“ (P1)
- TECH-236 Filter „Unsupported Plugins“ (P1)
- TECH-237 Filter „Mobile Alternate Link“ (P1)

**Accessibility**
- TECH-250 Accessibility-Prüfung mit axe-core (P1)
- TECH-251 Tab „Accessibility“: Population und Spalten (P1)
- TECH-252 Filtergruppe „Best Practice“ (28 Regeln) (P1)
- TECH-253 Filtergruppe „WCAG 2.0 A“ (56 Regeln) (P1)
- TECH-254 Filtergruppe „WCAG 2.0 AA“ (P1)
- TECH-255 Filtergruppe „WCAG 2.0 AAA“ (P1)
- TECH-256 Filtergruppe „WCAG 2.1 AA“ (P1)
- TECH-257 Filtergruppe „WCAG 2.2 AA“ (P1)
- TECH-258 Accessibility-Score und Score-Filter (P2)
- TECH-259 Detailansicht „Accessibility Details“ und Exporte (P1)

**Validation**
- TECH-280 Strukturanalyse auf Token-Ebene (Grundlage) (P1)
- TECH-281 Tab „Validation“: Population und Spalten (P1)
- TECH-282 Filter „Invalid HTML Elements In \<head>“ (P1)
- TECH-283 Filter „\<head> Not First In \<html> Element“ (P1)
- TECH-284 Filter „Missing \<head> Tag“ (P1)
- TECH-285 Filter „Multiple \<head> Tags“ (P1)
- TECH-286 Filter „Missing \<body> Tag“ (P1)
- TECH-287 Filter „Multiple \<body> Tags“ (P1)
- TECH-288 Filter „\<body> Element Preceding \<html>“ (P1)
- TECH-289 Filter „HTML Document Over 2MB“ (P1)
- TECH-290 Filter „Resource Over 2MB“ (P1)
- TECH-291 CO2-Schätzung und Filter „High Carbon Rating“ (P2)
- TECH-292 Option „Green Hosting Carbon Calculation“ (P2)

**Change Detection**
- TECH-300 Change-Detection-Konfiguration (P1)
- TECH-301 Tab „Change Detection“: Population und Spalten (P1)
- TECH-302 Elementfilter für Text-Elemente (Titel, Meta Description, H1) (P1)
- TECH-303 Elementfilter für Zahlen (Wortzahl, Tiefe, Links, Typen) (P1)
- TECH-304 Elementfilter „Indexability“ (P1)
- TECH-305 Elementfilter „Content“ (MinHash-Ähnlichkeit) (P1)
- TECH-306 URL-Mapping zwischen den Crawls (P1, Verweis → OUT-146)
- TECH-307 Umschalter „Changed“/„Not Changed“ und Sortierung nach Änderung (P1)
- TECH-308 Automatischer Crawl-Vergleich („Auto Compare Crawls“) für geplante und CLI-Crawls (P1, Verweis → OUT-152)
- TECH-309 Elementfilter „Canonical“ (P1, Nutzerentscheidung 04.11.2026)
- TECH-310 Elementfilter „Redirect Target“ (P1, Nutzerentscheidung 04.11.2026)

**Social (Nachtrag 09.11.2026, 1.x)**
- TECH-320 Social-Metadaten erfassen, Tab „Social“: Population und Spalten (P1)
- TECH-321 Open-Graph-Filter (P1)
- TECH-322 Card-Filter (X/Twitter) (P1)
- TECH-323 Social-Bilder: Abruf, Status und Maße (P1)

**Anhang A:** Offene Punkte, Festlegungen und Paritätsabweichungen, die gegen Golden-Exporte zu prüfen sind.
**Anhang B:** Rubrik und Begründung der Issue-Einstufungen.
**Anhang C:** Drittquellen, Lizenzen und Attribution.

---

## Gemeinsame Grundlagen

### TECH-001 Indexierbarkeitsmodell
- **Priorität:** P0
- **Beschreibung:** Viele Filter dieses Dokuments hängen davon ab, ob eine URL „Indexable“ oder „Non-Indexable“ ist und warum. Das Modell wird hier verbindlich für diesen Bereich festgelegt; falls die Kern-Spec (Internal/Response Codes) dasselbe Modell definiert, gilt diese ID als Querverweis und darf nicht abweichen.
- **Eingaben/Konfiguration:** Antwortstatus, robots.txt-Entscheidung, Direktiven (TECH-050), Canonical (TECH-010).
- **Ausgabe (Spalten/Filter/Export):** Spalten `Indexability` (Werte `Indexable` / `Non-Indexable`) und `Indexability Status` (leer bei indexierbar, sonst genau ein Grund).
- **Akzeptanzkriterien:**
  - [ ] Gründe und ihre Reihenfolge der Prüfung (der erste zutreffende gewinnt): `Blocked by Robots.txt` → `No Response` (Timeout, DNS-, TLS-, Verbindungsfehler) → `Redirected` (3xx, auch Meta-Refresh-Weiterleitung auf andere URL) → `Client Error` (4xx) → `Server Error` (5xx) → `Self Referencing Meta Refresh` (nur wenn TECH-068 an) → `noindex` (aus Meta Robots, nutzer­agent-spezifischem Meta-Tag oder X-Robots-Tag, inkl. `none`) → `Unavailable After` (Token `unavailable_after` mit parsebarem Datum vor dem Abrufzeitpunkt, TECH-063; K2-86, SQ-05; Position nach `noindex` ist Festlegung) → `Canonicalised` (Canonical zeigt auf andere URL, TECH-014).
  - [ ] Eine 200-Antwort ohne einen dieser Gründe ist `Indexable`. Nicht-HTML-Ressourcen (z. B. PDF) können per X-Robots-Tag oder HTTP-Canonical non-indexable werden.
  - [ ] Eine Seite mit `noindex` **und** fremdem Canonical erhält Status `noindex` (Reihenfolge oben).
  - [ ] Externe URLs werden nur nach Status bewertet (kein HTML geladen); ein externer 200 gilt als `Indexable`.
  - [ ] Meta Refresh (Element oder `Refresh`-Header) mit Ziel ≠ eigener URL (nach TECH-002, jede Verzögerung) → `Redirected`. Ein `Refresh`-Header mit Ziel = eigener URL → ebenfalls `Redirected`, unabhängig von TECH-068 (K2-86, SQ-05; Beleg Referenz `parity-m2-technical` `refresh-self.html`); ein `Refresh`-Header ohne Ziel bleibt ohne Wirkung (CRAWL-015).
  - [ ] Meta Refresh (nur Element) auf die eigene URL oder ohne Ziel (`content="30"`) → bei Standardkonfiguration (TECH-068 aus) unverändert `Indexable` (sofern kein anderer Grund greift). Mit eingeschalteter Option TECH-068 → `Non-Indexable` mit Grund `Self Referencing Meta Refresh` (Statustext ist Festlegung, siehe Anhang A); eingeordnet in der Prüfreihenfolge direkt nach `Server Error` und vor `noindex` (Standard geändert nach Paritätslauf M1, K-57).
- **Testfixture:** `tests/fixtures/tech/indexability/`: je eine Seite pro Grund (robots-gesperrt, 301, 404, 503, Timeout-Route mit 30 s Verzögerung, `noindex`, `none`, X-Robots-Tag `noindex` auf PDF, fremder Canonical, `noindex`+fremder Canonical, `refresh-other.html` mit `5; url=/ziel`, `refresh-self.html` mit `30; url=<eigene URL>`, `refresh-bare.html` mit `content="60"`), Startseite verlinkt alle. Erwartung bei Standard: `refresh-other` → `Redirected`, `refresh-self` und `refresh-bare` → `Indexable`. Mit TECH-068 an: `refresh-self` und `refresh-bare` → `Self Referencing Meta Refresh`. Ergänzung K2-86: `refresh-self-header` (Header `Refresh: 5; url=<eigene URL>`) → in beiden Läufen `Redirected`; `unavailable-past.html` (`unavailable_after` 2000-01-01) → `Unavailable After`.

### TECH-002 URL-Vergleich und Auflösung für technische Verweise
- **Priorität:** P0
- **Beschreibung:** Canonicals, hreflang, Pagination und AMP-Verweise werden relativ zur Basis-URL aufgelöst und für „gleich/ungleich“-Entscheidungen normalisiert verglichen.
- **Eingaben/Konfiguration:** Dokument-URL (nach Redirect die finale Antwort-URL des Dokuments), `<base href>` (erstes gültiges im Head), URL-Umschreibungsregeln aus der Crawl-Konfiguration.
- **Ausgabe (Spalten/Filter/Export):** In Spalten erscheint stets die **aufgelöste absolute URL**; der Rohwert wird zusätzlich gespeichert (für TECH-019).
- **Akzeptanzkriterien:**
  - [ ] Auflösung nach WHATWG-URL-Standard gegen `<base href>` (falls vorhanden), sonst gegen die Dokument-URL.
  - [ ] Für Gleichheitsvergleiche: Schema und Host kleingeschrieben, Default-Port entfernt (`:80` bei http, `:443` bei https), Fragment entfernt, Prozent-Kodierung normalisiert (Großbuchstaben in Hex, keine Dekodierung reservierter Zeichen). Pfad und Query bleiben case-sensitiv; Query-Reihenfolge bleibt erhalten.
  - [ ] `https://a.test/x` und `https://a.test/x/` sind **verschieden**; `http://` vs. `https://` sind verschieden; `https://A.test:443/x#y` ist gleich `https://a.test/x`.
  - [ ] Leerer oder nur aus Leerzeichen bestehender `href` wird nach URL-Standard zur Dokument-URL aufgelöst (Festlegung, Anhang A Punkt 2).
  - [ ] Ungültige URLs (z. B. `http://[bad`) werden als Rohwert gespeichert, nicht aufgelöst, und zählen in keinem Gleichheitsvergleich als Treffer.
- **Testfixture:** `tests/fixtures/tech/url-compare/`: Seiten mit Canonical-Varianten (relativ, `../`, mit `<base>`, Großbuchstaben-Host, `:443`, Fragment, Trailing Slash, leerer href, kaputte URL). Golden prüft aufgelöste Werte und Self-/Canonicalised-Zuordnung.

### TECH-003 Head-Zugehörigkeit von Elementen („Outside \<head>“)
- **Priorität:** P0
- **Beschreibung:** Mehrere Tabs melden Elemente, die außerhalb des `<head>` stehen. Maßgeblich ist, wo das Element nach dem HTML5-Tree-Construction-Algorithmus landet – also so, wie Browser und Google den Head schließen. Ein ungültiges Element im Head (z. B. `<img>`, `<div>`, Text) beendet den Head implizit; alles danach liegt im Body.
- **Eingaben/Konfiguration:** Rohes HTML (für Filter im Rohmodus) bzw. gerendertes DOM (im JS-Modus, siehe TECH-006).
- **Ausgabe (Spalten/Filter/Export):** Flag `in_head` je extrahiertem Canonical, Meta-Robots-, hreflang-, Title- und Meta-Description-Element.
- **Akzeptanzkriterien:**
  - [ ] `<link rel=canonical>` nach `<body>` → außerhalb.
  - [ ] `<link rel=canonical>` innerhalb des Quelltext-`<head>`, aber nach einem `<img>` im Head → außerhalb (Head wurde implizit geschlossen).
  - [ ] `<noscript>` im Head mit erlaubtem Inhalt (`link`, `meta`, `style`) beendet den Head **nicht**.
  - [ ] Element in einem zweiten `<head>`, der vor `<body>` steht, gilt als im Head (Inhalte werden vom Parser in den ersten Head übernommen bzw. dort ignoriert – Festlegung: zählt als im Head); nach Body-Beginn gilt es als außerhalb.
  - [ ] Elemente innerhalb `<template>` werden für SEO-Extraktion ignoriert.
  - [ ] Implementierung: Tree-Builder nach HTML5-Spezifikation (z. B. html5ever) – reines Streaming ohne Tree-Construction-Regeln reicht nicht.
  - [ ] Abhängigkeit von TECH-008: Das Flag `in_head` wird **immer** berechnet (es wird z. B. für die Auswahlregel in TECH-017 gebraucht). Die darauf beruhenden **Filter** „Outside \<head>“ (TECH-023, TECH-065, TECH-086 sowie die gleichnamigen Filter für Page Titles und Meta Description in der On-Page-Spec) werden aber nur befüllt, wenn „Perform HTML Validation“ (TECH-008) aktiv ist; sonst stehen sie in der Übersicht mit 0 und dem Hinweis „HTML-Validierung deaktiviert“.
- **Testfixture:** `tests/fixtures/tech/outside-head/`: Seiten mit Canonical, Meta Robots, hreflang, Title jeweils (a) korrekt im Head, (b) im Body, (c) im Head nach `<img>`, (d) im Head nach `<noscript><link …></noscript>`. Zwei Läufe: TECH-008 an → (b) und (c) stehen in den Outside-Filtern, (a) und (d) nicht; TECH-008 aus → alle Outside-Filter leer; Wertespalten in beiden Läufen gleich: Canonical und hreflang aus (b) und (c) erscheinen nicht in den Wertespalten (TECH-023, TECH-086; K2-86, SQ-04), Meta Robots und Title weiterhin.

### TECH-004 Mehrwertige Spalten „… 1, … 2, …“
- **Priorität:** P0
- **Beschreibung:** Elemente, die mehrfach vorkommen können (Canonical, Meta Robots, X-Robots-Tag, hreflang, Typen usw.), werden als nummerierte Spalten dargestellt.
- **Eingaben/Konfiguration:** keine.
- **Ausgabe (Spalten/Filter/Export):** Spaltenname `<Basis> 1`, `<Basis> 2`, … Anzahl der Spalten = Maximum über alle Zeilen der aktuellen Ansicht/Export.
- **Akzeptanzkriterien:**
  - [ ] Reihenfolge = Reihenfolge des Auftretens: zuerst Quelltextreihenfolge der HTML-Elemente, HTTP-Header-Werte in Header-Reihenfolge.
  - [ ] Exporte enthalten genau so viele nummerierte Spalten wie das Maximum der exportierten Zeilen; leere Zellen bleiben leer (kein „null“).
  - [ ] Hreflang: höchstens 500 Annotationen pro URL werden gespeichert/angezeigt; weitere werden verworfen und in einem Zähler `Hreflang Truncated` (intern, im Crawl-Log und in der Detailansicht TECH-089 sichtbar) vermerkt. Die gespeicherten 500 nehmen an allen hreflang-Filtern teil. **(Paritätsabweichung):** Die Referenz meldet Seiten mit mehr als 500 Annotationen laut Doku überhaupt nicht im Hreflang-Tab. OpenSpider wertet stattdessen die ersten 500 aus, damit große Sprach-Sets nicht unsichtbar werden. Paritätstests schließen Seiten mit > 500 Annotationen aus. Die Konfigurationsoption `hreflang_over_limit = skip | truncate` (Standard `truncate`) stellt bei `skip` das Referenzverhalten her: Die Seite erscheint dann in keinem hreflang-Filter, nur im Log.
- **Testfixture:** `tests/fixtures/tech/multi-columns/`: Seite A mit 3 Canonicals, Seite B mit 1; Export muss `Canonical Link Element 1..3` haben. Seite C mit 510 hreflang-Links → bei `truncate` 500 gespeichert, `Occurrences = 500`; bei `skip` ist Seite C in keinem Hreflang-Filter. Seite D mit genau 500 → in beiden Modi vollständig ausgewertet.

### TECH-005 Live-Filter vs. Nachlauf-Analyse („Crawl-Analyse erforderlich“)
- **Priorität:** P0
- **Beschreibung:** Es gibt zwei Arten von Filtern. **Live-Filter** werden während des Crawls laufend aktualisiert, auch wenn sie Daten anderer URLs brauchen: Sobald das Ziel abgerufen ist, wird die Quelle neu bewertet (inkrementell). **Nachlauf-Filter** brauchen eine Gesamtsicht auf den fertigen Linkgraphen, z. B. „kein einziger Hyperlink zeigt auf X“ oder „nur aus Sitemaps erreichbar“. Sie werden erst in der Nachlauf-Analyse (`os-analyze`) berechnet. Bis dahin steht neben dem Filter der Hinweis „Crawl-Analyse erforderlich“. Dieser Eintrag legt nur fest, **welche Filter dieses Dokuments** live bzw. im Nachlauf berechnet werden und welchem Analyse-Teilschritt sie angehören. Auslösung, automatischer Start am Crawl-Ende, manueller Start, Pause/Abbruch: **CRAWL-141 (führend für den Ablauf)**; Teilschritt-Dialog, Fortschritt und Darstellung des Hinweises: UI-080.
- **Eingaben/Konfiguration:** Teilschritte gemäß UI-080/CRAWL-141. Zuordnung der TECH-Nachlauf-Filter zu den Teilschritten: `Link Score` (TECH-145), `Kanonisierungen` (TECH-018/020), `Paginierung` (TECH-037/040/041), `Hreflang` (TECH-075), `Links` (TECH-139/140/142), `AMP` (TECH-162), `Sitemaps` (TECH-212–216). Die Teilschritte `Kanonisierungen` und `Links` sind OpenSpider-Ergänzungen ohne dokumentierte Referenzentsprechung (Festlegung, Anhang A Punkt 25).
- **Ausgabe (Spalten/Filter/Export):** Pro Filter ein Status `live` / `nachlauf_ausstehend` / `final`.
- **Akzeptanzkriterien:**
  - [ ] Nachlauf-Filter in diesem Dokument (abschließende Liste): TECH-018, TECH-020 (Bereich Canonicals); TECH-037, TECH-040, TECH-041 (Pagination); TECH-075 (Hreflang – laut Referenz-Tutorial der **einzige** hreflang-Filter mit Analysepflicht); TECH-139, TECH-140, TECH-142 (Links); TECH-145 `Link Score` (Link Score); TECH-162 „Missing Non-AMP Return Link“ und „Non-Indexable Canonical“ (AMP); TECH-212, TECH-213, TECH-214, TECH-215, TECH-216 (Sitemaps).
  - [ ] Live (inkrementell) berechnet werden insbesondere: TECH-035, TECH-036 (sobald das Paginierungsziel abgerufen ist), alle hreflang-Filter außer TECH-075 – also auch TECH-074 und TECH-076–TECH-079, die neu bewertet werden, sobald ein Alternate abgerufen ist, sowie TECH-083, das nur die eigene Seite braucht – und TECH-217/TECH-218.
  - [ ] Sitemaps: Von den 7 Sitemap-Filtern sind genau 2 live (TECH-217, TECH-218: Eigenschaften der einzelnen Sitemap-Datei) und 5 Nachlauf (TECH-212–TECH-216). Begründung: Das Referenz-Tutorial nennt das Verhältnis 2 : 5. Die Zuordnung „URL steht in Sitemap“ (TECH-212) wird erst final, wenn alle Sitemap-Dateien eingelesen und die Abgleiche mit den gecrawlten URLs abgeschlossen sind. Vorher wäre die Zahl unvollständig.
  - [ ] Vor Abschluss des zugehörigen Teilschritts tragen die Nachlauf-Filter dieses Dokuments den Zustand `nachlauf_ausstehend` (Darstellung „Crawl-Analyse erforderlich“ in Übersicht und Haupttabelle nach UI-080); nach Abschluss erscheint die Zahl (`final`).
  - [ ] Live-Filter zeigen während des Crawls Zwischenstände. Ein Live-Filter darf eine Quelle wieder entfernen, wenn ein später abgerufenes Ziel die Bedingung aufhebt (z. B. Rücklink gefunden).
  - [ ] Ist ein Teilschritt abgewählt, bleiben dessen Nachlauf-Filter leer und dauerhaft im Zustand `nachlauf_ausstehend`.
  - [ ] Die Ergebnisse der Nachlauf-Filter sind deterministisch (gleiche Eingabe → gleiche Zahlen, unabhängig von der Abrufreihenfolge). Verhalten bei Pause/Fortsetzen: CRAWL-141.
- **Testfixture:** `tests/fixtures/tech/crawl-analysis/`: kleine Site mit fehlendem hreflang-Rücklink (live, muss nach Crawl-Ende **ohne** Analyse gezählt sein), einem nur per hreflang erreichbaren Alternate (TECH-075, erst nach Analyse) und einem Sitemap-Orphan. Test prüft: Missing Return Links = 1 vor der Analyse; Unlinked Hreflang URLs und Orphan URLs „ausstehend“ vor der Analyse, Zahl danach. Zweiter Lauf mit abgewähltem Bereich „Sitemaps“ → TECH-212–216 bleiben markiert.

### TECH-006 Rohes vs. gerendertes HTML als Datenquelle
- **Priorität:** P1
- **Beschreibung:** Im Rendering-Modus „JavaScript“ werden Canonicals, Direktiven, hreflang, Links, strukturierte Daten aus dem **gerenderten** DOM für die Tabs genutzt; das Original-HTML wird zusätzlich ausgewertet und für den JavaScript-Tab (TECH-100) verglichen. Im Modus „Text only“ (Standard) wird nur das Original-HTML ausgewertet.
- **Eingaben/Konfiguration:** Rendering-Modus (`text_only` | `javascript`), AJAX-Timeout (Standard 5 s), Fenstergröße (Mobil 411×731, Desktop 1024×768; Seite wird bis max. 8192 px Höhe verlängert), Shadow DOM abflachen (Standard an), iframes abflachen (Standard an).
- **Ausgabe (Spalten/Filter/Export):** Datensatz enthält beide Fassungen, getrennt gespeichert (Felder `raw_*` und `rendered_*`).
- **Akzeptanzkriterien:**
  - [ ] Eine Seite, deren Canonical nur per JS gesetzt wird, erscheint im JS-Modus in Canonicals > Contains Canonical, im Text-only-Modus in Canonicals > Missing.
  - [ ] X-Robots-Tag und HTTP-Canonical stammen immer aus der HTTP-Antwort, unabhängig vom Modus.
  - [ ] Ein `noindex` im Original-HTML macht die Seite auch im JS-Modus non-indexable, selbst wenn JS ihn entfernt (wie Google, das Seiten mit noindex nicht rendert); siehe TECH-105.
- **Testfixture:** `tests/fixtures/tech/js-vs-raw/`: Seite mit per JS gesetztem Canonical, Title, H1, Meta Robots, Links; Crawl einmal mit `text_only`, einmal mit `javascript`.

### TECH-007 Fixture-Konventionen für diesen Bereich
- **Priorität:** P0
- **Beschreibung:** Einheitliche Ablage der Fixtures für TECH-Funktionen.
- **Eingaben/Konfiguration:** –
- **Ausgabe (Spalten/Filter/Export):** –
- **Akzeptanzkriterien:**
  - [ ] Jede Fixture liegt unter `tests/fixtures/tech/<slug>/`, Status-Codes, Header und Verzögerungen werden im Manifest des Fixture-Servers (`os-testsite`) deklariert, nicht im HTML.
  - [ ] Zu jeder Fixture gibt es Golden-Werte unter `tests/golden/tech/<slug>/` (je Tab ein CSV mit Adresse + relevanten Spalten + Filterzugehörigkeit).
  - [ ] Tests referenzieren die Spec-ID (`// spec: TECH-0xx`).
  - [ ] Fixtures, die Rendering brauchen, sind mit `requires_chromium = true` markiert und laufen in CI nur, wenn Chromium verfügbar ist.
- **Testfixture:** –

### TECH-008 Option „Perform HTML Validation“ (Validation- und „Outside \<head>“-Filter)
- **Priorität:** P1
- **Beschreibung:** Führender Eintrag für die HTML-Validierung (CRAWL-138 verweist hierher). Ein Schalter in der Spider-Konfiguration (Bereich „Advanced“) steuert, ob die Strukturprüfungen ausgeführt werden. Die Prüfung ist keine W3C-Vollvalidierung, sondern erhebt beim Parsen jeder internen HTML-Seite Zusatzdaten auf Token-Ebene (Position und Anzahl von `<html>`, `<head>`, `<body>`, unzulässige Elemente im Head, Elemente, die den Head vorzeitig beenden; Prüflogik TECH-280). Ohne Schalter werden diese Daten nicht erhoben. Davon hängen zwei Filterfamilien ab: alle Filter des Validation-Tabs, die auf der Token-Analyse beruhen, und alle „Outside \<head>“-Filter in den Tabs Canonicals, Directives, Hreflang, Page Titles und Meta Description.
- **Eingaben/Konfiguration:** `perform_html_validation` (bool; Konfigurationsschlüssel `advanced.html_validation`, Spider › Erweitert). Standard in OpenSpider: **an** (Festlegung; Begründung: die Outside-\<head>-Filter sind für die Indexierung von Titel, Meta-Angaben und Canonical relevant, die Mehrkosten der Token-Zählung sind gering). Der Referenzstandard ist in der öffentlichen Doku nicht angegeben (die Beschreibung legt ein Opt-in nahe) und wird gegen eine Standardinstallation geprüft (Anhang A). Die Option wird in der Crawl-Datei gespeichert.
- **Ausgabe (Spalten/Filter/Export):** Betroffene Filter: TECH-282–TECH-288 (Token-Analyse), TECH-023, TECH-065, TECH-086 sowie die Outside-Filter der On-Page-Spec. **Nicht** betroffen: TECH-289/TECH-290 (Größe, Festlegung: nutzen nur die Antwortgröße) und TECH-291 (CO2).
- **Akzeptanzkriterien:**
  - [ ] Option aus → Die Token-Analyse (TECH-280) läuft nicht. TECH-282–288 und alle Outside-Filter sind leer und tragen in der Übersicht den Hinweis „HTML-Validierung deaktiviert (Spider › Erweitert)“ statt einer 0. In der Crawl-Datei sind keine Validierungs-Rohdaten gespeichert (Tabellenzeilen = 0).
  - [ ] Option aus → Spaltenwerte wie `Canonical Link Element n`, `Meta Robots n`, hreflang-Spalten und die Indexierbarkeit bleiben unverändert (die Extraktion hängt nicht von der Option ab).
  - [ ] Option an → Filter werden live während des Crawls befüllt (keine Crawl-Analyse nötig). Die Prüfung läuft immer gegen das Roh-HTML, auch im JS-Modus (TECH-280).
  - [ ] Wird die Option nach dem Crawl eingeschaltet, sind die Filter erst nach einem erneuten Crawl befüllt (die Rohtoken werden nicht gespeichert). Die UI zeigt das mit dem Hinweis „Neu crawlen erforderlich“.
  - [ ] Wird die Option während einer Pause eingeschaltet, haben nur die nach dem Fortsetzen gecrawlten Seiten Validierungsdaten; die Filter tragen dann den Hinweis „unvollständig – n Seiten ohne Prüfung, Neu crawlen empfohlen“.
  - [ ] Die Größenfilter TECH-289/290 und die CO2-Schätzung TECH-291 hängen **nicht** von der Option ab (sie brauchen nur Größenangaben aus dem Abruf).
  - [ ] Mehrkosten: Ein Crawl der Fixture `crawl-site-100` im Textmodus ist mit Option an höchstens 5 % langsamer als ohne (Benchmark).
  - [ ] Die CLI akzeptiert den Schalter über die gespeicherte Konfigurationsdatei.
- **Testfixture:** `tests/fixtures/tech/validation/` und `tests/fixtures/tech/outside-head/`, je ein Lauf mit Option an und aus. Golden „aus“: alle genannten Filter 0 und markiert, Spalten identisch zum Lauf „an“. Zusätzlich die Crawl-Fixture `crawl-validation` (`/ok.html`, `/kein-head.html`, `/zwei-heads.html`, `/canonical-im-body.html`, `/title-nach-div.html`) aus CRAWL-138 und ein Pause-Lauf (Option während der Pause eingeschaltet).

### TECH-009 Option „Ignore Non-Indexable URLs for Issues“
- **Priorität:** P1
- **Verweis:** siehe CRAWL-089 (führend). Welche Filter dieses Dokuments die Option einschränkt, legt der Wirkungsbereich von CRAWL-089 abschließend fest.

---

## Canonicals

### TECH-010 Canonical-Extraktion (Link-Element und HTTP-Header)
- **Priorität:** P0
- **Beschreibung:** Erfasst alle Canonical-Angaben einer Antwort aus zwei Quellen: `<link rel="canonical" href>` im HTML und `Link: <URL>; rel="canonical"` im HTTP-Header.
- **Eingaben/Konfiguration:** Option „Store Canonicals“ (Standard an).
- **Ausgabe (Spalten/Filter/Export):** Je Canonical: Quelle (`html`/`http`), Rohwert, aufgelöste URL (TECH-002), `in_head` (TECH-003), Zusatzattribute (für TECH-021).
- **Akzeptanzkriterien:**
  - [ ] `rel`-Wert wird case-insensitiv und als Token-Liste ausgewertet: `rel="Canonical"` und `rel="canonical alternate"` zählen; `rel="canonicalx"` nicht.
  - [ ] Mehrere `Link`-Header und kommagetrennte Einträge in einem Header werden alle ausgewertet; Parameter-Werte dürfen mit oder ohne Anführungszeichen stehen.
  - [ ] HTTP-Canonical wird auch für Nicht-HTML-Antworten (PDF, Bilder) erfasst.
  - [ ] `<link rel=canonical>` ohne `href`-Attribut wird ignoriert (nicht gezählt); `href=""` siehe TECH-002.
  - [ ] Canonicals in `<template>` und in Kommentaren werden ignoriert; in `<noscript>` im Head werden sie erfasst (Festlegung).
- **Testfixture:** `tests/fixtures/tech/canonical-extract/`: HTML-Canonical, Großschreibung im rel, Mehrfach-rel, zwei Link-Header, ein Header mit zwei Einträgen, PDF mit Link-Header, link ohne href.

### TECH-011 Tab „Canonicals“: Population und Spalten
- **Priorität:** P0
- **Beschreibung:** Übersichtstabelle aller internen URLs, für die Canonical-Aussagen sinnvoll sind.
- **Eingaben/Konfiguration:** –
- **Ausgabe (Spalten/Filter/Export):** Spalten in dieser Reihenfolge: `Address`, `Occurrences` (Anzahl Canonical-Angaben über beide Quellen), `Indexability`, `Indexability Status`, `Canonical Link Element 1..n`, `HTTP Canonical 1..n`, `Meta Robots 1..n`, `X-Robots-Tag 1..n`, `rel="next" 1`, `rel="prev" 1`. Filter: TECH-012 bis TECH-023 plus „All“. Die Referenz exportiert zusätzlich `HTTP rel="next" 1`/`HTTP rel="prev" 1`; OpenSpider führt sie in diesem Tab nicht, der Paritätsvergleich ignoriert sie (K2-88, SQ-13f).
- **Akzeptanzkriterien:**
  - [ ] Population: interne HTML- oder PDF-Seiten mit 2xx, plus interne Nicht-HTML-2xx-URLs mit HTTP-Canonical. URLs ohne 2xx erscheinen nicht. (Paritätsbefund M1 vom 14.10.2026, Befund B-04, K-56: die Referenz führt alle 2xx, Fixture `/s201`; vorher „Status 200“.)
  - [ ] Filter „All“ = Population; jede URL kann in mehreren Filtern zugleich stehen.
  - [ ] Export „Canonicals > All“ und jeder Einzelfilter als CSV/XLSX mit obigen Spalten.
  - [ ] Canonicals speichern aus (CRAWL-046) → Tab Canonicals leer, und es wird kein Indexierbarkeitsstatus `Canonicalised` gesetzt (Festlegung).
  - [ ] Canonicals crawlen aus (CRAWL-046) → Ziele, die nur per Canonical bekannt sind, werden nicht abgerufen; TECH-018 und TECH-020 bleiben für sie leer. Canonical-Ziele, die abgerufen werden, erscheinen in Internal/External mit Inlink-Typ „Canonical“.
  - [ ] „Respect Canonical“ an (CRAWL-092) → canonicalisierte URLs werden weiterhin gecrawlt und ihre Links verfolgt, sind aber in Tabellen/Exporten ausgeblendet – ausgenommen der Filter „Canonicalised“ (TECH-014) dieses Tabs, damit die Kanonisierung nachvollziehbar bleibt (Festlegung, Anhang A Punkt 35).
- **Testfixture:** `tests/fixtures/tech/canonical-basic/` (siehe die folgenden Filter; eine Site deckt alle ab); für die Konfigurationskriterien drei Läufe (Standard, Crawlen aus, Respect Canonical an).

### TECH-012 Filter „Contains Canonical“
- **Priorität:** P0
- **Beschreibung:** URLs mit mindestens einer Canonical-Angabe (HTML oder HTTP).
- **Eingaben/Konfiguration:** –
- **Ausgabe (Spalten/Filter/Export):** Filter „Contains Canonical“ (keine Issue-Einstufung, reine Information).
- **Akzeptanzkriterien:**
  - [ ] Bedingung: `Occurrences ≥ 1`.
  - [ ] Canonical nur per HTTP-Header → enthalten. Canonical außerhalb des Heads → ebenfalls enthalten (zusätzlich in TECH-023).
- **Testfixture:** `canonical-basic/self.html`, `/header-only` (Canonical nur als Header).

### TECH-013 Filter „Self Referencing“
- **Priorität:** P0
- **Beschreibung:** Canonical zeigt auf die eigene URL.
- **Eingaben/Konfiguration:** –
- **Ausgabe (Spalten/Filter/Export):** Filter „Self Referencing“ (Information).
- **Akzeptanzkriterien:**
  - [ ] Bedingung: Es gibt mindestens eine Canonical-Angabe, und die maßgebliche Canonical-URL (TECH-017, Auswahlregel) ist nach TECH-002 gleich der Seiten-URL.
  - [ ] `https://site.test/a` mit Canonical `/a` → Self Referencing. Mit Canonical `/a/` → nicht (sondern Canonicalised).
  - [ ] Canonical `https://site.test/a#top` auf `/a` → Self Referencing (Fragment wird für den Vergleich entfernt) und zusätzlich TECH-022.
- **Testfixture:** `canonical-basic/self.html`, `/slash` (Canonical mit Slash-Abweichung), `/fragment`.

### TECH-014 Filter „Canonicalised“
- **Priorität:** P0
- **Beschreibung:** Canonical zeigt auf eine andere URL; die Seite wird dadurch non-indexable.
- **Eingaben/Konfiguration:** –
- **Ausgabe (Spalten/Filter/Export):** Filter „Canonicalised“; Einstufung: Warnung / Hoch. Setzt `Indexability Status = Canonicalised` (sofern kein vorrangiger Grund, TECH-001).
- **Akzeptanzkriterien:**
  - [ ] Bedingung: maßgebliche Canonical-URL ≠ Seiten-URL (nach TECH-002).
  - [ ] Canonical auf andere Domain (extern) → Canonicalised.
  - [ ] Canonical `http://` auf eine `https://`-Seite → Canonicalised.
  - [ ] Ungültige (nicht auflösbare) Canonical-URL → nicht Canonicalised; Seite bleibt indexierbar (Festlegung).
- **Testfixture:** `canonical-basic/to-other.html`, `/to-external`, `/to-http`, `/invalid-url`.

### TECH-015 Filter „Missing“
- **Priorität:** P0
- **Beschreibung:** Keine Canonical-Angabe in HTML oder Header.
- **Eingaben/Konfiguration:** –
- **Ausgabe (Spalten/Filter/Export):** Filter „Missing“; Einstufung: Warnung / Mittel.
- **Akzeptanzkriterien:**
  - [ ] Bedingung: interne HTML- oder PDF-Seite mit Status 2xx und `Occurrences = 0`.
  - [ ] Bei „Ignore Non-Indexable URLs for Issues“ an (CRAWL-089, Standard) werden nur indexierbare Seiten gelistet (Paritätsbefund M1, Befund B-05, K-56).
  - [ ] PDFs ohne HTTP-Canonical erscheinen in Missing; andere Nicht-HTML-Ressourcen ohne HTTP-Canonical nicht.
  - [ ] Canonical nur per JS gesetzt: im Text-only-Modus Missing, im JS-Modus nicht (TECH-006).
- **Testfixture:** `canonical-basic/none.html`, eine PDF ohne Header.

### TECH-016 Filter „Multiple“
- **Priorität:** P0
- **Beschreibung:** Mehr als eine Canonical-Angabe, egal ob identisch.
- **Eingaben/Konfiguration:** –
- **Ausgabe (Spalten/Filter/Export):** Filter „Multiple“; Einstufung: Warnung / Niedrig.
- **Akzeptanzkriterien:**
  - [ ] Bedingung: `Occurrences ≥ 2` (HTML-Elemente + HTTP-Header-Einträge zusammengezählt).
  - [ ] Zwei identische Link-Elemente → Multiple, aber nicht Multiple Conflicting.
  - [ ] Ein Link-Element + ein identischer Header → Multiple.
- **Testfixture:** `canonical-basic/multi-same.html`, `/multi-mixed-same` (Header + HTML identisch).

### TECH-017 Filter „Multiple Conflicting“
- **Priorität:** P0
- **Beschreibung:** Mehrere Canonical-Angaben, die auf unterschiedliche URLs zeigen.
- **Eingaben/Konfiguration:** –
- **Ausgabe (Spalten/Filter/Export):** Filter „Multiple Conflicting“; Einstufung: Fehler / Hoch.
- **Akzeptanzkriterien:**
  - [ ] Bedingung: Die Menge der nach TECH-002 aufgelösten und normalisierten Canonical-URLs (gültige HTML-Elemente im Head plus HTTP-Header-Einträge; Body-Canonicals zählen nur für TECH-023, K2-86/K2-93) hat ≥ 2 verschiedene Elemente. Rohwerte, die sich nicht auflösen lassen (TECH-002, ungültige URL), werden für diese Menge ignoriert (Festlegung).
  - [ ] Jede Seite in „Multiple Conflicting“ steht auch in „Multiple“ (TECH-016); umgekehrt nicht.
  - [ ] Header zeigt auf `/a`, HTML auf `/b` → Conflicting.
  - [ ] `/a` und `/a#x` → **nicht** conflicting (Fragment entfernt). Relativ `/a` und absolut `https://site.test/a` auf derselben Seite → nicht conflicting.
  - [ ] `/a` und `/A` → conflicting (Pfad case-sensitiv). `/a` und `/a/` → conflicting.
  - [ ] `/a` und `http://[bad` → **nicht** conflicting (ungültiger Wert ignoriert), aber `Occurrences = 2` und damit „Multiple“.
  - [ ] Auswahlregel für die „maßgebliche“ Canonical-URL (für TECH-013/014, TECH-018 und die Indexierbarkeit): erster gültiger HTTP-Header-Eintrag; sonst erstes gültiges HTML-Element mit `in_head = true`. Body-Canonicals zählen nicht für die Auswahl (K2-86). (K2-93)
  - [ ] Beispiele der Auswahlregel auf Seite `/p`: Header `/a` + Head `/b` → maßgeblich `/a`, Status `Canonicalised`. Head `/p` + Body `/b` → maßgeblich `/p`, Self Referencing (TECH-013); die Seite steht ausschließlich zusätzlich in TECH-023, unabhängig von TECH-008. Body `/b` allein → keine maßgebliche Canonical-URL, ausschließlich TECH-023. (K2-93)
  - [ ] Die Spalten `Canonical Link Element n` und `HTTP Canonical n` zeigen alle wirksamen Werte in Quelltext- bzw. Header-Reihenfolge (TECH-004); Body-Canonicals tragen weder Werte noch Occurrences bei (TECH-023/K2-86). (K2-93)
- **Testfixture:** `canonical-basic/multi-conflict.html` (zwei HTML-Canonicals `/a`, `/b`), `/multi-conflict-mixed` (Header `/a` vs. HTML `/b`), `/multi-relabs` (relativ + absolut gleich, Negativtest), `/multi-case` (`/a` vs. `/A`), `/multi-invalid` (`/a` + `http://[bad`), `/head-self-body-other` (Head self, Body fremd). Golden: Filterzugehörigkeit Multiple/Conflicting, maßgebliche URL, `Indexability Status`.

### TECH-018 Filter „Non-Indexable Canonical“
- **Priorität:** P0
- **Beschreibung:** Das Canonical-Ziel ist selbst nicht indexierbar.
- **Eingaben/Konfiguration:** Canonicals crawlen (CRAWL-046) muss aktiv sein, sonst ist der Zielstatus unbekannt und der Filter bleibt leer.
- **Ausgabe (Spalten/Filter/Export):** Filter „Non-Indexable Canonical“; Einstufung: Fehler / Hoch. Nachlauf-Filter (TECH-005).
- **Akzeptanzkriterien:**
  - [ ] Bedingung: Seite ist Canonicalised (TECH-014) und das Ziel ist `Non-Indexable` nach TECH-001 – also robots-gesperrt, ohne Antwort, 3xx, 4xx, 5xx, noindex oder selbst canonicalised.
  - [ ] Externes Canonical-Ziel: nur Statusprüfung; 200 → indexierbar, sonst non-indexable.
  - [ ] Self-Referencing-Canonicals werden nicht hier gelistet (die Seite selbst ist ggf. in anderen Filtern).
  - [ ] Ziel nicht gecrawlt (z. B. durch Exclude ausgeschlossen) → nicht gelistet (Status unbekannt).
- **Testfixture:** `canonical-basic/to-404.html`, `/to-redirect`, `/to-noindex`, `/to-blocked` (robots.txt), `/to-canonicalised` (Ziel hat selbst fremden Canonical), `/to-external-404`.

### TECH-019 Filter „Canonical Is Relative“
- **Priorität:** P1
- **Beschreibung:** Der `href`-Rohwert eines Canonicals ist keine absolute URL.
- **Eingaben/Konfiguration:** –
- **Ausgabe (Spalten/Filter/Export):** Filter „Canonical Is Relative“; Einstufung: Warnung / Niedrig.
- **Akzeptanzkriterien:**
  - [ ] Bedingung: Mindestens ein Rohwert (HTML oder Header) beginnt nicht mit einem Schema (`[a-zA-Z][a-zA-Z0-9+.-]*:`).
  - [ ] `/a`, `a`, `../a`, `?p=2` → relativ. Schema-relativ `//site.test/a` → relativ (Festlegung). `https://site.test/a` → absolut.
- **Testfixture:** `canonical-basic/relative.html` (`href="/relative"`), `/protocol-relative`.

### TECH-020 Filter „Unlinked“
- **Priorität:** P1
- **Beschreibung:** Canonical-Ziele, auf die keine normale Verlinkung im Crawl zeigt; sie wurden nur über das Canonical entdeckt.
- **Eingaben/Konfiguration:** Canonicals crawlen (CRAWL-046). Nachlauf-Filter.
- **Ausgabe (Spalten/Filter/Export):** Filter „Unlinked“; Einstufung: Warnung / Mittel. Gelistet wird das **Canonical-Ziel**, sofern es als interne URL gecrawlt wurde (K2-86, SQ-06; vorher die Quelle; Beleg Referenz `parity-m2-technical` `canonical-unlinked-target.html`); Export „Unlinked Canonical“ listet unverändert Quelle und Ziel.
- **Akzeptanzkriterien:**
  - [ ] Bedingung: URL C ist Canonical-Ziel mindestens einer anderen Seite, und C hat im Crawl keinen eingehenden Hyperlink (`<a href>`, `<area href>`; JS-Links im JS-Modus zählen) von einer internen Seite. Eingänge über Canonical, hreflang, Pagination-Link-Element, Redirect oder Sitemap zählen nicht als Verlinkung.
  - [ ] Self-Referencing-Canonicals werden nicht als Unlinked gemeldet.
  - [ ] Ein Link nur aus einer non-indexable Seite zählt als Verlinkung (Festlegung).
- **Testfixture:** `canonical-basic/to-unlinked.html` mit Canonical auf `/hidden` (nirgends verlinkt; gelistet wird `/hidden`) und `/to-linked` mit Canonical auf eine im Menü verlinkte Seite (nicht gelistet).

### TECH-021 Filter „Invalid Attribute In Annotation“
- **Priorität:** P1
- **Beschreibung:** Canonical-Angaben mit zusätzlichen Attributen, die dazu führen können, dass Suchmaschinen sie ignorieren.
- **Eingaben/Konfiguration:** –
- **Ausgabe (Spalten/Filter/Export):** Filter „Invalid Attribute In Annotation“; Einstufung: Fehler / Hoch.
- **Akzeptanzkriterien:**
  - [ ] Bedingung: Ein Canonical-Link-Element trägt eines der Attribute `hreflang`, `lang`, `media`, `type` (Wert egal, auch leer), **oder** ein Canonical-Eintrag im `Link`-Header hat einen dieser Parameter.
  - [ ] Andere Attribute (`id`, `class`, `data-*`, `title`) lösen den Filter nicht aus.
  - [ ] Ein Canonical mit einem dieser Attribute bzw. Parameter zählt **nur** für diesen Filter: nicht in `Canonical Link Element n`/`HTTP Canonical n`, nicht in `Occurrences`, keine Wirkung auf Indexierbarkeit (`Canonicalised`) und Folgefilter (K2-86, SQ-04; Beleg Referenz `canonical-invalid-attribute.html`).
- **Testfixture:** `canonical-basic/attr-hreflang.html`, `/attr-media`, `/attr-type-header` (Header `Link: <…>; rel="canonical"; type="text/html"`), `/attr-class` (Negativtest).

### TECH-022 Filter „Contains Fragment URL“
- **Priorität:** P1
- **Beschreibung:** Canonical-URL enthält ein `#`-Fragment.
- **Eingaben/Konfiguration:** –
- **Ausgabe (Spalten/Filter/Export):** Filter „Contains Fragment URL“; Einstufung: Fehler / Hoch.
- **Akzeptanzkriterien:**
  - [ ] Bedingung: Ein Rohwert enthält `#` (auch leeres Fragment `…/a#`).
  - [ ] Die Spalte zeigt die URL ohne Fragment; der Rohwert bleibt für diesen Filter erhalten. Vergleiche ignorieren das Fragment (TECH-002). (K2-93)
- **Testfixture:** `canonical-basic/fragment.html` (`#top`), `/fragment-empty` (`#`).

### TECH-023 Filter „Outside \<head>“ (Canonical)
- **Priorität:** P1
- **Beschreibung:** Canonical-Link-Element liegt außerhalb des Heads (TECH-003).
- **Eingaben/Konfiguration:** Nur befüllt bei „Perform HTML Validation“ an (TECH-008).
- **Ausgabe (Spalten/Filter/Export):** Filter „Outside \<head>“; Einstufung: Fehler / Hoch.
- **Akzeptanzkriterien:**
  - [ ] Bedingung: Mindestens ein HTML-Canonical mit `in_head = false`.
  - [ ] Ein Canonical außerhalb des Heads zählt **nur** für diesen Filter: nicht in `Canonical Link Element n`, nicht in `Occurrences`, keine Wirkung auf Indexierbarkeit und Folgefilter; unabhängig von TECH-008, da `in_head` immer berechnet wird (K2-86, SQ-04; vorher „wird angezeigt und zählt in Occurrences“; Beleg Referenz `canonical-outside.html`: Occurrences 0, Indexable).
  - [ ] HTTP-Canonicals lösen den Filter nie aus.
- **Testfixture:** `tests/fixtures/tech/outside-head/canonical-body.html`, `/canonical-after-img.html`.

### TECH-024 Canonicals crawlen/speichern und „Respect Canonical“
- **Priorität:** P1
- **Verweis:** siehe CRAWL-092 (führend). Speichern/Crawlen der Canonicals regelt CRAWL-046, „Canonicals immer folgen“ im Listenmodus CRAWL-030 und die Auswirkung auf den Canonicals-Tab TECH-011.

### TECH-025 Report „Canonical Chains“ und Export „Non-Indexable Canonicals“
- **Priorität:** P1
- **Verweis:** siehe OUT-086 (führend). Für „Non-Indexable Canonicals“ ist OUT-087 führend, hier gilt nur die Zeilenauswahl (Ketten nach TECH-026, Canonical-Ziele nach TECH-018/TECH-020), während Spaltennamen und die Werte von `Chain Type` ausschließlich 06 festlegt (OUT-081/OUT-083).

### TECH-026 Canonical-Ketten: Ermittlung und Zeilenauswahl
- **Priorität:** P1
- **Beschreibung:** Legt fest, welche Folgen von Canonical-Verweisen (auch gemischt mit Weiterleitungen) als Kette gelten. Diese Menge bestimmt die Zeilen der Reports OUT-086 (Canonical Chains) und OUT-083 (Redirect & Canonical Chains). Abgespalten aus TECH-025; Spalten, Report-Namen und die Werte von `Chain Type` stehen ausschließlich in 06.
- **Eingaben/Konfiguration:** Canonicals speichern und crawlen (CRAWL-046); Nachlauf (TECH-005). Sprunglimit: dasselbe Weiterleitungslimit wie für Weiterleitungsketten (Crawl-Limits, OUT-081); ein Canonical-Glied zählt dabei wie ein Weiterleitungssprung (Festlegung).
- **Ausgabe (Spalten/Filter/Export):** keine eigenen Spalten; Zeilenmenge für OUT-083/OUT-086.
- **Akzeptanzkriterien:**
  - [ ] Ein Kettenglied ist ein Canonical-Verweis (HTML-Element oder HTTP-`Link`-Header, maßgebliche Canonical-URL nach TECH-017) auf eine andere URL (Vergleich nach TECH-002) oder ein Weiterleitungssprung; ein selbstreferenzierendes Canonical bildet kein Glied.
  - [ ] Eine Kette entsteht erst ab 2 Gliedern (A→B→C); A→B allein ist keine Kette.
  - [ ] Eine Kette gilt als gemischt, sobald mindestens ein Weiterleitungs- und ein Canonical-Glied vorkommen, sonst als reine Canonical-Kette. Daraus ergeben sich Report-Zuordnung und `Chain Type`-Wert nach OUT-083/OUT-086; dieses Dokument vergibt keine eigenen Typwerte.
  - [ ] Schleifen (A→B→A) werden erkannt; die Kette endet beim ersten wiederholten Knoten und wird als Schleife gekennzeichnet (Spalte nach OUT-081).
  - [ ] Wird das Sprunglimit erreicht, endet die Kette dort; die Kennzeichnung des Abbruchs folgt OUT-081.
  - [ ] Startzeilen: jede gecrawlte URL, die erstes Glied einer Kette ist; Zwischenglieder erhalten nur dann eine eigene Zeile, wenn sie unabhängig verlinkt sind (wie OUT-081/OUT-086).
- **Testfixture:** `tests/fixtures/tech/canonical-chains/`: A→(canonical)B→(canonical)C; D→(canonical)E→(301)F; G↔H Schleife; I→(canonical)J (nur ein Glied, keine Kette).

---

## Pagination

### TECH-030 Extraktion rel="next"/rel="prev"
- **Priorität:** P1
- **Beschreibung:** Erfasst Paginierungsangaben aus `<link rel="next|prev" href>` im HTML und aus `Link`-Headern mit `rel="next"`/`rel="prev"`.
- **Eingaben/Konfiguration:** „Store Pagination“ (Standard an), „Crawl Pagination“ (Standard aus).
- **Ausgabe (Spalten/Filter/Export):** Je Angabe: Richtung (`next`/`prev`), Quelle, Rohwert, aufgelöste URL, `in_head`.
- **Akzeptanzkriterien:**
  - [ ] `rel="previous"` wird wie `prev` behandelt (Festlegung; HTML-Spezifikation kennt nur `prev`, Browser-Praxis akzeptiert beide).
  - [ ] Nur `<link>`-Elemente zählen; `<a rel="next">` ist ein normaler Hyperlink, keine Paginierungsangabe.
  - [ ] Header- und HTML-Angaben werden zusammengeführt; Duplikate mit gleicher URL zählen für TECH-039 einzeln.
- **Testfixture:** `tests/fixtures/tech/pagination/`: Serie `/list?page=1..4` mit Link-Elementen; eine Seite mit Header statt Element; eine mit `rel="previous"`; eine mit `<a rel="next">` ohne Link-Element.

### TECH-031 Tab „Pagination“: Population und Spalten
- **Priorität:** P1
- **Beschreibung:** Tabelle der internen HTML-Seiten mit Paginierungsbezug.
- **Eingaben/Konfiguration:** –
- **Ausgabe (Spalten/Filter/Export):** `Address`, `Occurrences` (Anzahl next+prev-Angaben), `Indexability`, `Indexability Status`, `rel="next" 1..n`, `rel="prev" 1..n`, `Canonical Link Element 1..n`, `HTTP Canonical 1..n`, `Meta Robots 1..n`, `X-Robots-Tag 1..n`. Filter: TECH-032 bis TECH-041 plus „All“. Die Referenz exportiert kein `Occurrences`, dafür `HTTP rel="next" 1`/`HTTP rel="prev" 1`; der Paritätsvergleich ignoriert diese Spalten (K2-88, SQ-13f).
- **Akzeptanzkriterien:**
  - [ ] Population: interne HTML-Seiten mit 2xx (K2-85, SQ-03; vorher zusätzlich gecrawlte Paginierungsziele ohne 2xx). Gecrawlte Ziele ohne 2xx sind keine eigene Zeile; sie erscheinen nur in TECH-036 und dessen Export.
  - [ ] Paginierung crawlen aus (CRAWL-047) → Ziele, die nur per Link-Element bekannt sind, werden nicht abgerufen; TECH-036/037 bleiben für sie leer.
  - [ ] „Respect Next/Prev“ an (CRAWL-093) → Seiten mit `prev` werden gecrawlt und ihre Links verfolgt, sind aber in allen Tabellen **außer dem Pagination-Tab** ausgeblendet (Festlegung, Anhang A Punkt 35).
- **Testfixture:** `tests/fixtures/tech/pagination/`, Läufe mit Paginierung crawlen an/aus und „Respect Next/Prev“ an.

### TECH-032 Filter „Contains Pagination“
- **Priorität:** P1
- **Beschreibung:** Seiten mit mindestens einer next- oder prev-Angabe.
- **Eingaben/Konfiguration:** –
- **Ausgabe (Spalten/Filter/Export):** Filter „Contains Pagination“ (Information).
- **Akzeptanzkriterien:**
  - [ ] Bedingung: `Occurrences ≥ 1`.
- **Testfixture:** `pagination/list?page=1..4` → 4 Treffer, `/about` → 0.

### TECH-033 Filter „First Page“
- **Priorität:** P1
- **Beschreibung:** Erste Seite einer Serie: hat `next`, aber kein `prev`.
- **Eingaben/Konfiguration:** –
- **Ausgabe (Spalten/Filter/Export):** Filter „First Page“ (Information).
- **Akzeptanzkriterien:**
  - [ ] Bedingung: Anzahl `next` ≥ 1 und Anzahl `prev` = 0.
  - [ ] Eine Seite mit nur `prev` (letzte Seite) ist nicht First Page.
- **Testfixture:** `pagination/list?page=1` → First Page; `page=4` (nur prev) → nicht.

### TECH-034 Filter „Paginated 2+ Pages“
- **Priorität:** P1
- **Beschreibung:** Seiten ab Position 2 einer Serie.
- **Eingaben/Konfiguration:** –
- **Ausgabe (Spalten/Filter/Export):** Filter „Paginated 2+ Pages“ (Information).
- **Akzeptanzkriterien:**
  - [ ] Bedingung: Anzahl `prev` ≥ 1 (unabhängig von `next`).
- **Testfixture:** `pagination/list?page=2..4` → 3 Treffer.

### TECH-035 Filter „Pagination URL Not In Anchor Tag“
- **Priorität:** P1
- **Beschreibung:** Eine next/prev-URL wird auf derselben Seite nicht zusätzlich als normaler Hyperlink angeboten.
- **Eingaben/Konfiguration:** –
- **Ausgabe (Spalten/Filter/Export):** Filter „Pagination URL Not In Anchor Tag“; Einstufung: Fehler / Hoch.
- **Akzeptanzkriterien:**
  - [ ] Bedingung: Für mindestens eine next/prev-URL U der Seite gibt es unter den Outlinks derselben Seite keinen `<a href>` mit normalisiert gleicher URL (TECH-002).
  - [ ] `<a href>` mit `rel=nofollow` auf U zählt als vorhanden.
  - [ ] Kann live berechnet werden (nur eigene Seite nötig).
- **Testfixture:** `pagination/list?page=3`: Link-Element auf `page=4`, aber keine sichtbare Paginierungsnavigation.

### TECH-036 Filter „Non-200 Pagination URLs“
- **Priorität:** P1
- **Beschreibung:** Seiten, deren next/prev-Ziele nicht mit 200 antworten.
- **Eingaben/Konfiguration:** Paginierungsziele müssen gecrawlt sein (Crawl Pagination an oder anderweitig entdeckt). Live, inkrementell (TECH-005): Die Quelle wird neu bewertet, sobald ein Ziel abgerufen ist.
- **Ausgabe (Spalten/Filter/Export):** Filter „Non-200 Pagination URLs“; Einstufung: Fehler / Hoch. Export „Pagination > Non-200 Pagination URLs“: OUT-088 (führend für Namen und Spalten); Zeilenauswahl: je (Quelle, Ziel) mit Status ≠ 200 nach der Bedingung unten eine Zeile.
- **Akzeptanzkriterien:**
  - [ ] Bedingung: Mindestens ein Ziel hat Status ≠ 200 – einschließlich robots-gesperrt, keine Antwort, 3xx, 4xx, 5xx.
  - [ ] Nicht abgerufene Ziele (Crawl aus, nicht anderweitig gefunden) werden nicht gemeldet.
- **Testfixture:** `pagination/broken?page=1` mit `next` auf `/broken?page=2` (404) und `/redir?page=1` mit `next` auf 301.

### TECH-037 Filter „Unlinked Pagination URLs“
- **Priorität:** P1
- **Beschreibung:** Paginierungsziele, die nirgends im Crawl per Hyperlink verlinkt sind.
- **Eingaben/Konfiguration:** Nachlauf.
- **Ausgabe (Spalten/Filter/Export):** Filter „Unlinked Pagination URLs“; Einstufung: Fehler / Mittel. Gelistet wird die Quellseite; Export „Pagination > Unlinked Pagination URLs“: OUT-089 (führend für Namen und Spalten); Zeilenauswahl: je (Quelle, unverlinktes Ziel) eine Zeile.
- **Akzeptanzkriterien:**
  - [ ] Bedingung: Ein next/prev-Ziel hat keinen eingehenden Hyperlink (`<a href>`/`<area href>`) von irgendeiner internen Seite (Logik wie TECH-020).
  - [ ] Unterschied zu TECH-035: Hier genügt ein Link von irgendeiner Seite.
- **Testfixture:** `pagination/orphan-series?page=1` mit `next` auf `?page=2`, die nirgends verlinkt ist.

### TECH-038 Filter „Non-Indexable“ (Pagination)
- **Priorität:** P1
- **Beschreibung:** Seiten mit Paginierungsangaben, deren eigener Indexierbarkeitsstatus negativ ist. Typischer Auslöser in der Praxis: Folgeseiten einer Serie verweisen kanonisch auf den Serienanfang oder sind gesperrt. Dadurch werden Inhalte, die nur auf Folgeseiten verlinkt sind, schlechter auffindbar.
- **Eingaben/Konfiguration:** –
- **Ausgabe (Spalten/Filter/Export):** Filter „Non-Indexable“; Einstufung: Warnung / Hoch.
- **Akzeptanzkriterien:**
  - [ ] Bedingung: `Occurrences ≥ 1` und `Indexability = Non-Indexable`.
  - [ ] `page=2` mit Canonical auf `page=1` → gelistet mit Status `Canonicalised`.
- **Testfixture:** `pagination/canon?page=2` (Canonical auf Seite 1), `/noindex?page=3`.

### TECH-039 Filter „Multiple Pagination URLs“
- **Priorität:** P1
- **Beschreibung:** Mehr als eine next- oder mehr als eine prev-Angabe.
- **Eingaben/Konfiguration:** –
- **Ausgabe (Spalten/Filter/Export):** Filter „Multiple Pagination URLs“; Einstufung: Fehler / Niedrig.
- **Akzeptanzkriterien:**
  - [ ] Bedingung: Anzahl `next` ≥ 2 **oder** Anzahl `prev` ≥ 2 (über HTML und Header zusammen, auch identische URLs).
  - [ ] Ein `next` plus ein `prev` → nicht Multiple.
- **Testfixture:** `pagination/multi`: zwei `next`-Elemente auf verschiedene URLs; `/multi-same`: zwei identische `prev`.

### TECH-040 Filter „Pagination Loop“
- **Priorität:** P1
- **Beschreibung:** Die Folge der next-Verweise (oder prev-Verweise) führt zu einer bereits besuchten URL zurück.
- **Eingaben/Konfiguration:** Nachlauf.
- **Ausgabe (Spalten/Filter/Export):** Filter „Pagination Loop“; Einstufung: Fehler / Niedrig.
- **Akzeptanzkriterien:**
  - [ ] Algorithmus: Für jede Seite der Kette wird ausgehend von ihr der erste `next` (bzw. erste `prev`) gefolgt, solange das Ziel gecrawlt ist und eine Angabe hat. Wird eine URL ein zweites Mal erreicht, gehören alle Seiten im Zyklus in den Filter.
  - [ ] `page=3` mit `next` auf `page=1` (bei 1→2→3) → Seiten 1, 2, 3 gelistet.
  - [ ] Seite mit `next` auf sich selbst → gelistet.
  - [ ] Seiten, die nur in den Zyklus hineinführen (Vorlauf), werden nicht gelistet (Festlegung).
- **Testfixture:** `pagination/loop?page=1..3` mit Rücksprung; `/self-next`.

### TECH-041 Filter „Sequence Error“
- **Priorität:** P1
- **Beschreibung:** Die Verweise sind nicht wechselseitig: Wenn A „next = B“ sagt, muss B „prev = A“ sagen (und umgekehrt).
- **Eingaben/Konfiguration:** Nachlauf; Ziele müssen gecrawlt sein.
- **Ausgabe (Spalten/Filter/Export):** Filter „Sequence Error“; Einstufung: Fehler / Niedrig. Gelistet wird die Seite, deren Angabe nicht bestätigt wird.
- **Akzeptanzkriterien:**
  - [ ] Bedingung (a): A hat `next = B`, B ist gecrawlt (HTML, 200) und B hat kein `prev` oder `prev` ≠ A → A gelistet.
  - [ ] Bedingung (b): A hat `prev = Z`, Z ist gecrawlt (HTML, 200) und Z hat kein `next` oder `next` ≠ A → A gelistet.
  - [ ] Nicht gecrawlte Ziele lösen keinen Sequence Error aus. Gecrawlte Ziele ohne 2xx lösen zusätzlich zu TECH-036 einen Sequence Error aus. (K2-93)
  - [ ] Vergleich nach TECH-002.
- **Testfixture:** `pagination/seq?page=1..3`, wobei `page=2` als prev auf `page=0` statt `page=1` zeigt.

### TECH-042 Pagination crawlen/speichern, „Respect Next/Prev“ und Reports
- **Priorität:** P2
- **Verweis:** siehe CRAWL-093 (führend). Speichern/Crawlen der Paginierung regelt CRAWL-047, die Reports „Pagination > Non-200 Pagination URLs“/„Unlinked Pagination URLs“ OUT-088/OUT-089 und die Auswirkung auf den Pagination-Tab TECH-031.

---

## Directives

### TECH-050 Directive-Extraktion (Meta Robots, X-Robots-Tag, Meta Refresh)
- **Priorität:** P0
- **Beschreibung:** Erfasst Robots-Anweisungen aus `<meta name="robots">`, aus nutzeragent-spezifischen Meta-Tags (z. B. `<meta name="googlebot">`), aus `X-Robots-Tag`-Headern sowie Weiterleitungs-Refresh aus `<meta http-equiv="refresh">` und einem `Refresh`-HTTP-Header.
- **Eingaben/Konfiguration:** Robots-User-Agent-Token für spezifische Direktiven (Standard: `googlebot`; konfigurierbar in der Crawl-Konfiguration, gemeinsam mit robots.txt-User-Agent).
- **Ausgabe (Spalten/Filter/Export):** Je Quelle die Liste der normalisierten Direktiven-Token plus Rohwert.
- **Akzeptanzkriterien:**
  - [ ] `name` und Inhalt werden case-insensitiv verarbeitet, Token durch Komma getrennt, Leerzeichen getrimmt: `content="NoIndex , FOLLOW"` → `noindex`, `follow`.
  - [ ] Wertbehaftete Direktiven werden mit Wert gespeichert: `max-snippet:50`, `max-image-preview:large`, `max-video-preview:-1`, `unavailable_after: 2026-12-31`. Das Datum von `unavailable_after` darf Kommas enthalten (z. B. RFC-850 `Thursday, 31-Dec-26 …`); der Parser darf es nicht am Komma zerschneiden.
  - [ ] Meta-Tags mit `name` = generisch (`robots`) oder = konfiguriertem Token werden angewendet; andere (`bingbot`) werden angezeigt, aber nicht für Indexierbarkeit genutzt.
  - [ ] X-Robots-Tag: mehrere Header und mehrere Werte je Header; Präfix `<agent>:` bindet die folgenden Direktiven an diesen Agent (z. B. `googlebot: noindex, otherbot: nofollow`). Nur generische und zum Token passende Teile werden angewendet.
  - [ ] Meta Refresh: `content="5; url=/neu"` → Verzögerung 5, Ziel aufgelöst; `content="0"` → Refresh ohne Ziel (= Selbst-Refresh, Wirkung siehe TECH-068); Varianten `URL=`, Anführungszeichen, `;` oder `,` als Trenner werden akzeptiert.
  - [ ] Meta-Robots im Body werden extrahiert und mit `in_head = false` markiert (TECH-065).
- **Testfixture:** `tests/fixtures/tech/directives/`: je eine Seite pro Direktive; gemischte Groß-/Kleinschreibung; `googlebot`- und `bingbot`-Meta; X-Robots-Tag mit Agent-Präfix; zwei X-Robots-Tag-Header; Meta Refresh in 4 Schreibweisen; `Refresh`-Header.

### TECH-051 Tab „Directives“: Population und Spalten
- **Priorität:** P0
- **Beschreibung:** Übersicht der Robots-Anweisungen je interner URL.
- **Eingaben/Konfiguration:** –
- **Ausgabe (Spalten/Filter/Export):** `Address`, `Occurrences` (Anzahl Meta-Robots + X-Robots-Tag-Werte), `Meta Robots 1..n`, `X-Robots-Tag 1..n`, `Meta Refresh 1..n`, `Canonical Link Element 1..n`, `HTTP Canonical 1..n`. Filter TECH-052 bis TECH-065 plus „All“. Standardspalten am Ende: `Indexability`, `Indexability Status`.
- **Akzeptanzkriterien:**
  - [ ] Population: alle internen URLs mit 2xx-Antwort, jeder Inhaltstyp, da X-Robots-Tag auch an Nicht-HTML hängen kann (K2-85, SQ-03; vorher jeder Status; Beleg Referenz: 3xx und 404 fehlen auf `parity-m2-technical` und `parity-technical`).
  - [ ] Spaltenwert `Meta Robots n` enthält den Rohinhalt des Attributs `content`; bei spezifischem Agent mit Präfix, z. B. `googlebot: noindex`.
  - [ ] Jeder Filter TECH-052–TECH-064 prüft auf das **Vorhandensein des Tokens** in irgendeiner Quelle (Meta oder Header, generisch oder zum Token passend), nicht auf die effektive Wirkung (die regelt TECH-066).
  - [ ] „Respect Noindex“ an (CRAWL-091) → noindex-URLs werden gecrawlt und ihre Links verfolgt, sind aber in Tabellen/Exporten ausgeblendet – ausgenommen die Directives-Filter dieses Tabs (Festlegung, Anhang A Punkt 35). Zusammen mit TECH-068 an werden nur Seiten mit dem Grund `noindex` ausgeblendet, nicht Selbst-Refresh-Seiten.
  - [ ] „Follow Internal Nofollow“ / „Follow External Nofollow“ an (CRAWL-056) → die Links behalten in der Anzeige `Follow = false`; die Filter dieses Tabs ändern sich nicht.
- **Testfixture:** `tests/fixtures/tech/directives/`, Läufe Standard, „Respect Noindex“ an, „Follow Internal Nofollow“ an, „Follow External Nofollow“ an.

### TECH-052 Filter „Index“
- **Priorität:** P0
- **Beschreibung:** Explizites `index`.
- **Eingaben/Konfiguration:** –
- **Ausgabe (Spalten/Filter/Export):** Filter „Index“ (Information).
- **Akzeptanzkriterien:**
  - [ ] Bedingung: Token `index` vorhanden. `all` zählt **nicht** als `index` (Festlegung).
  - [ ] Seite ohne Robots-Angabe erscheint nicht.
- **Testfixture:** `directives/index.html` (`index, follow`), `/all.html`.

### TECH-053 Filter „Noindex“
- **Priorität:** P0
- **Beschreibung:** Anweisung, die Seite nicht zu indexieren.
- **Eingaben/Konfiguration:** –
- **Ausgabe (Spalten/Filter/Export):** Filter „Noindex“; Einstufung: Warnung / Hoch. Seite wird `Non-Indexable` mit Status `noindex`.
- **Akzeptanzkriterien:**
  - [ ] Bedingung: Token `noindex` vorhanden (Meta oder X-Robots-Tag; generisch oder passender Agent).
  - [ ] `none` allein erscheint unter „None“ (TECH-056), **nicht** unter „Noindex“ – die Seite ist aber non-indexable.
  - [ ] `<meta name="bingbot" content="noindex">` bei Token `googlebot` → nicht im Filter, Seite indexierbar.
- **Testfixture:** `directives/noindex-meta.html`, `/noindex-header` (X-Robots-Tag), `/noindex-bingbot.html`, `/noindex-pdf` (PDF mit Header).

### TECH-054 Filter „Follow“
- **Priorität:** P0
- **Beschreibung:** Explizites `follow`.
- **Eingaben/Konfiguration:** –
- **Ausgabe (Spalten/Filter/Export):** Filter „Follow“ (Information).
- **Akzeptanzkriterien:**
  - [ ] Bedingung: Token `follow` vorhanden.
- **Testfixture:** `directives/index.html`.

### TECH-055 Filter „Nofollow“
- **Priorität:** P0
- **Beschreibung:** Anweisung, Links der Seite nicht zu folgen.
- **Eingaben/Konfiguration:** „Follow Internal Nofollow“ / „Follow External Nofollow“ (Standard aus, CRAWL-056).
- **Ausgabe (Spalten/Filter/Export):** Filter „Nofollow“; Einstufung: Warnung / Hoch.
- **Akzeptanzkriterien:**
  - [ ] Bedingung: Token `nofollow` vorhanden.
  - [ ] Bei Standardkonfiguration werden die Links einer solchen Seite nicht in die Frontier aufgenommen; die Outlinks werden aber gespeichert und mit `Follow = false` markiert.
- **Testfixture:** `directives/nofollow.html` verlinkt `/only-via-nofollow-page` (nirgends sonst verlinkt) → wird bei Standard nicht gecrawlt.

### TECH-056 Filter „None“
- **Priorität:** P0
- **Beschreibung:** `none` = Kurzform für `noindex, nofollow`.
- **Eingaben/Konfiguration:** –
- **Ausgabe (Spalten/Filter/Export):** Filter „None“; Einstufung: Warnung / Hoch.
- **Akzeptanzkriterien:**
  - [ ] Bedingung: Token `none` vorhanden.
  - [ ] Wirkung wie noindex + nofollow (Indexierbarkeit `noindex`, Links nicht verfolgt).
- **Testfixture:** `directives/none.html`.

### TECH-057 Filter „NoArchive“
- **Priorität:** P1
- **Beschreibung:** Untersagt eine Cache-Version in Suchergebnissen.
- **Eingaben/Konfiguration:** –
- **Ausgabe (Spalten/Filter/Export):** Filter „NoArchive“ (Information).
- **Akzeptanzkriterien:**
  - [ ] Bedingung: Token `noarchive` vorhanden.
- **Testfixture:** `directives/noarchive.html`.

### TECH-058 Filter „NoSnippet“
- **Priorität:** P1
- **Beschreibung:** Untersagt Text- und Video-Snippets.
- **Eingaben/Konfiguration:** –
- **Ausgabe (Spalten/Filter/Export):** Filter „NoSnippet“; Einstufung: Warnung / Niedrig.
- **Akzeptanzkriterien:**
  - [ ] Bedingung: Token `nosnippet` vorhanden.
  - [ ] `data-nosnippet`-Attribute im Body lösen den Filter nicht aus.
- **Testfixture:** `directives/nosnippet.html`, `/data-nosnippet.html` (Negativtest).

### TECH-059 Filter „Max-Snippet“, „Max-Image-Preview“, „Max-Video-Preview“
- **Priorität:** P1
- **Beschreibung:** Drei Filter für wertbehaftete Vorschau-Begrenzungen.
- **Eingaben/Konfiguration:** –
- **Ausgabe (Spalten/Filter/Export):** Filter „Max-Snippet“, „Max-Image-Preview“, „Max-Video-Preview“ (Information).
- **Akzeptanzkriterien:**
  - [ ] „Max-Snippet“: Token `max-snippet:<n>` vorhanden; gültig sind ganze Zahlen ≥ -1 (`0` = kein Snippet, `-1` = unbegrenzt).
  - [ ] „Max-Image-Preview“: Token `max-image-preview:<v>` vorhanden; gültige Werte `none`, `standard`, `large`.
  - [ ] „Max-Video-Preview“: Token `max-video-preview:<n>`; `0` = nur Standbild, `-1` = unbegrenzt.
  - [ ] Ungültige Werte (`max-snippet:abc`, `max-image-preview:huge`) erscheinen trotzdem im jeweiligen Filter, Rohwert in der Spalte sichtbar.
- **Testfixture:** `directives/max.html` (`max-snippet:50, max-image-preview:large, max-video-preview:-1`), `/max-invalid.html`.

### TECH-060 Filter „NoODP“, „NoYDIR“ (Altlasten)
- **Priorität:** P2
- **Beschreibung:** Veraltete Direktiven ohne heutige Wirkung; werden gemeldet, damit man sie entfernen kann.
- **Eingaben/Konfiguration:** –
- **Ausgabe (Spalten/Filter/Export):** Filter „NoODP“, „NoYDIR“; Einstufung je: Warnung / Niedrig.
- **Akzeptanzkriterien:**
  - [ ] Bedingung: Token `noodp` bzw. `noydir` vorhanden.
- **Testfixture:** `directives/legacy.html` (`noodp, noydir`).

### TECH-061 Filter „NoImageIndex“
- **Priorität:** P1
- **Beschreibung:** Bilder der Seite sollen nicht über diese Seite indexiert werden.
- **Eingaben/Konfiguration:** –
- **Ausgabe (Spalten/Filter/Export):** Filter „NoImageIndex“; Einstufung: Fehler / Niedrig.
- **Akzeptanzkriterien:**
  - [ ] Bedingung: Token `noimageindex` vorhanden.
- **Testfixture:** `directives/noimageindex.html`.

### TECH-062 Filter „NoTranslate“
- **Priorität:** P1
- **Beschreibung:** Suchmaschine soll keine Übersetzung anbieten.
- **Eingaben/Konfiguration:** –
- **Ausgabe (Spalten/Filter/Export):** Filter „NoTranslate“; Einstufung: Warnung / Niedrig.
- **Akzeptanzkriterien:**
  - [ ] Bedingung: Token `notranslate` in Robots-Angaben. `<meta name="google" content="notranslate">` zählt ebenfalls (Festlegung). Das HTML-Attribut `translate="no"` zählt nicht.
- **Testfixture:** `directives/notranslate.html`, `/google-notranslate.html`.

### TECH-063 Filter „Unavailable_After“
- **Priorität:** P1
- **Beschreibung:** Seite soll nach einem Zeitpunkt aus den Ergebnissen verschwinden.
- **Eingaben/Konfiguration:** –
- **Ausgabe (Spalten/Filter/Export):** Filter „Unavailable_After“; Einstufung: Warnung / Mittel.
- **Akzeptanzkriterien:**
  - [ ] Bedingung: Token `unavailable_after` mit beliebigem Wert vorhanden.
  - [ ] Akzeptierte Datumsformate zum Parsen: ISO 8601, RFC 822/1123, RFC 850. Nicht parsebarer Wert → trotzdem im Filter.
  - [ ] Ein parsebares Datum vor dem Abrufzeitpunkt → `Non-Indexable`, Status `Unavailable After` (TECH-001; K2-86, SQ-05; vorher „ändert die Indexierbarkeit nicht“; Beleg Referenz `directive-7.html`). Datum in der Zukunft oder nicht parsebar → Indexierbarkeit unverändert.
- **Testfixture:** `directives/unavailable.html` (Zukunft, ISO), `/unavailable-past.html` (RFC 850 mit Komma).

### TECH-064 Filter „Refresh“
- **Priorität:** P0
- **Beschreibung:** Seiten mit Meta Refresh bzw. `Refresh`-Header (zeitgesteuerte Weiterleitung).
- **Eingaben/Konfiguration:** „Crawl Meta Refresh“ (Standard an): Ziel wird in die Frontier aufgenommen.
- **Ausgabe (Spalten/Filter/Export):** Filter „Refresh“ (Information); Spalte `Meta Refresh n` = Rohwert ausschließlich der Meta-Elemente, nicht der HTTP-Header. (K2-93) Details der Weiterleitung im Response-Codes-Bereich.
- **Akzeptanzkriterien:**
  - [ ] Bedingung: mindestens ein gültiges Meta-Refresh-Element (Head oder Body). HTTP-Refresh wird ausschließlich im Response-Codes-Filter „Redirection (HTTP Refresh)“ (05-ui.md) gelistet, nicht in diesem Filter. (K2-93)
  - [ ] Refresh mit Ziel ≠ eigener URL → Indexierbarkeit `Redirected` (TECH-001); Ziel-URL wird gecrawlt.
  - [ ] Meta-Refresh auf sich selbst oder ohne Ziel → im Filter, kein neuer Abruf. HTTP-Refresh zählt hier nicht, auch nicht bei eigener Ziel-URL. Indexierbarkeit: Header mit eigener Ziel-URL → `Redirected` (SQ-05). Meta-Selbst-Refresh nach TECH-068 bleibt bei Standard (Option aus) indexierbar, sofern kein anderer Grund vorliegt; mit Option an `Non-Indexable` / `Self Referencing Meta Refresh`. Ein Header ohne Ziel ist keine Weiterleitung (K-69). (K2-93)
- **Testfixture:** `directives/refresh-0.html` (`0; url=/target` → Redirected), `/refresh-5-self.html` (→ Self Referencing Meta Refresh), `/refresh-bare.html` (`content="10"` → Self Referencing Meta Refresh), `/refresh-header` (Header `Refresh: 0; url=/target`, Negativfall für diesen Filter). (K2-93)

### TECH-065 Filter „Outside \<head>“ (Meta Robots)
- **Priorität:** P1
- **Beschreibung:** Meta-Robots-Element außerhalb des Heads (TECH-003).
- **Eingaben/Konfiguration:** Nur befüllt bei „Perform HTML Validation“ an (TECH-008).
- **Ausgabe (Spalten/Filter/Export):** Filter „Outside \<head>“; Einstufung: Fehler / Hoch.
- **Akzeptanzkriterien:**
  - [ ] Bedingung: mindestens ein Robots-Meta-Element (generisch oder agent-spezifisch) mit `in_head = false`.
  - [ ] Ein `noindex` im Body wird für die Indexierbarkeit trotzdem angewendet (vorsichtige Annahme, Festlegung).
- **Testfixture:** `outside-head/robots-body.html`, `/robots-after-img.html`.

### TECH-066 Konfliktauflösung widersprüchlicher Direktiven
- **Priorität:** P0
- **Beschreibung:** Bei mehreren oder widersprüchlichen Angaben gilt für die Wirkung die restriktivste.
- **Eingaben/Konfiguration:** –
- **Ausgabe (Spalten/Filter/Export):** Effektive Werte `effective_index` (bool) und `effective_follow` (bool) pro URL, genutzt von TECH-001 und Link-Verfolgung.
- **Akzeptanzkriterien:**
  - [ ] `index` + `noindex` (egal ob in einem Tag, zwei Tags oder Meta + Header) → `noindex` wirkt; die Seite steht in beiden Filtern „Index“ und „Noindex“.
  - [ ] `follow` + `nofollow` → `nofollow` wirkt.
  - [ ] Bei mehreren `max-snippet`-Werten gilt der kleinste nicht-negative Wert (Festlegung, nur für Detailansicht). [M2: Detailansicht; die gespeicherten Rohwerte liefert M1]
- **Testfixture:** `directives/conflict-meta.html` (`index` und `noindex` in zwei Tags), `/conflict-mixed` (Meta `index`, Header `noindex`).

### TECH-067 „Respect Noindex“ und „Follow Internal/External Nofollow“
- **Priorität:** P1
- **Verweis:** siehe CRAWL-091 (führend). „Follow Internal/External Nofollow“ regelt CRAWL-056, „Respect Self Referencing Meta Refresh“ TECH-068, „Ignore Non-Indexable URLs for Issues“ CRAWL-089 und die Auswirkung auf den Directives-Tab TECH-051.

### TECH-068 Option „Respect Self Referencing Meta Refresh“
- **Priorität:** P1
- **Beschreibung:** Führender Eintrag (K-12; CRAWL-094 verweist hierher). Legt fest, ob eine Seite, die sich per Meta Refresh selbst neu lädt, als nicht indexierbar geführt wird. Ein Selbst-Refresh per `Refresh`-Header ist seit K2-86 (SQ-05) unabhängig von dieser Option `Redirected` (TECH-001). Solche Selbst-Refreshes sind verbreitet (z. B. Nachrichtenseiten, die sich periodisch aktualisieren) und schaden der Indexierung meist nicht. Sie sollen aber sichtbar werden, damit man sie bewusst prüft. Die frühere Annahme „die Referenz stuft sie standardmäßig als nicht indexierbar ein“ ist durch den Paritätslauf M1 (14.10.2026, Befund B-03, K-57) widerlegt: Referenzprodukt Free (Standardkonfiguration) führt Selbst-Refresh-Seiten als `Indexable` und zeigt den Refresh als Weiterleitung auf sich selbst (CRAWL-015). Der Standard folgt deshalb der Referenz; die Option erlaubt das Einschalten.
- **Eingaben/Konfiguration:** `respect_self_referencing_meta_refresh` (bool, Konfigurationsschlüssel `advanced.respect_self_ref_meta_refresh`), Standard **aus** (wie Referenz; Konfiguration > Spider > Advanced; bis 14.10.2026 „an“). Selbst-Refresh = Refresh ohne Ziel-URL oder mit Ziel, das nach TECH-002 gleich der eigenen URL ist (Fragment ignoriert).
- **Ausgabe (Spalten/Filter/Export):** `Indexability = Non-Indexable`, `Indexability Status = Self Referencing Meta Refresh` (Statustext Festlegung).
- **Akzeptanzkriterien:**
  - [ ] Option an → Selbst-Refresh-Seite ist `Non-Indexable` mit obigem Status, unabhängig von der Verzögerung (0 s und 300 s gleich behandelt).
  - [ ] Option aus → Selbst-Refresh beeinflusst die Indexierbarkeit nicht. Die Seite bleibt in Directives > Refresh (TECH-064).
  - [ ] Refresh auf `https://site.test/p?x=1` von `https://site.test/p` ist **kein** Selbst-Refresh (Query unterscheidet sich) → `Redirected`.
  - [ ] Refresh auf `https://site.test/p#top` von `https://site.test/p` ist ein Selbst-Refresh.
  - [ ] Folgefilter, die Indexierbarkeit nutzen (TECH-018, TECH-079, TECH-215 usw.), sehen bei Option an die Seite als nicht indexierbar.
  - [ ] Die Option wirkt ohne neuen Crawl (Neuberechnung der Indexierbarkeit und der abhängigen Filter).
- **Testfixture:** `tests/fixtures/tech/directives/refresh-self-*`: `refresh-self-0.html` (`0; url=<self>`), `refresh-self-300.html`, `refresh-bare.html`, `refresh-self-fragment.html` (`url=<self>#top`), `refresh-query.html` (`url=<self>?x=1`), `refresh-self-header` (Header-Variante: in beiden Läufen `Redirected`, K2-86). Außerdem steht `refresh-self-0.html` in `sitemap.xml`, damit bei Option an TECH-215 auslöst. Läufe mit Option an und aus.

---

## Hreflang

Begriffe: Eine **Annotation** ist ein Paar (Code, URL) auf Seite P. Die **Annotationsmenge** von P vereinigt HTML-, HTTP- und Sitemap-Annotationen. Ein **Alternate** von P ist jede URL Q ≠ P aus Ps Annotationsmenge. Ein **Rücklink** ist eine Annotation auf Q, deren URL gleich P ist (TECH-002). Der **Selbstcode** von P ist der Code der Annotation von P, deren URL gleich P ist (bei mehreren: der erste).

### TECH-070 Hreflang-Extraktion (HTML, HTTP-Header, XML-Sitemap)
- **Priorität:** P0
- **Beschreibung:** Erfasst Sprach-/Regionsalternativen aus drei Quellen: `<link rel="alternate" hreflang="…" href="…">`, `Link`-Header mit `rel="alternate"; hreflang="…"` und `<xhtml:link rel="alternate" hreflang href>` innerhalb eines `<url>`-Eintrags einer XML-Sitemap.
- **Eingaben/Konfiguration:** „Store Hreflang“ (Standard an), „Crawl Hreflang“ (Standard aus, TECH-087), Sitemaps müssen geparst werden (TECH-210), damit Sitemap-hreflang vorliegt.
- **Ausgabe (Spalten/Filter/Export):** Je Annotation: Quelle (`html`/`http`/`sitemap`), Code (roh und normalisiert kleingeschrieben), aufgelöste URL, `in_head`, bei Sitemap die Sitemap-URL.
- **Akzeptanzkriterien:**
  - [ ] Nur Link-Elemente mit `rel`-Token `alternate` **und** nicht-leerem `hreflang` zählen. `alternate` ohne hreflang (z. B. RSS, Mobile) ist keine hreflang-Annotation.
  - [ ] Sitemap-Annotationen werden der URL im `<loc>` desselben `<url>`-Blocks zugeordnet; Namespace-Präfix ist beliebig, maßgeblich ist der XHTML-Namespace.
  - [ ] Pro URL höchstens 500 Annotationen. Über 500 wird abgeschnitten, nicht verworfen (Paritätsabweichung, Details und Schalter für das Referenzverhalten in TECH-004).
  - [ ] Relative hreflang-URLs werden aufgelöst (TECH-002) und gespeichert.
  - [ ] Annotationen außerhalb des Heads werden erfasst und markiert (TECH-086).
- **Testfixture:** `tests/fixtures/tech/hreflang/`: Seiten `/en/`, `/de/`, `/fr/` mit HTML-hreflang; `/pdf-en.pdf` mit Header-hreflang; `sitemap-hreflang.xml` mit xhtml:link für `/es/` und `/it/`.

### TECH-071 Tab „Hreflang“: Population und Spalten
- **Priorität:** P0
- **Beschreibung:** Übersicht aller URLs mit hreflang-Bezug.
- **Eingaben/Konfiguration:** „Store Hreflang“ (TECH-070), „Crawl Hreflang“ (TECH-087), für Sitemap-hreflang zusätzlich „Crawl Linked XML Sitemaps“ (TECH-210) oder eine direkt angegebene Sitemap.
- **Ausgabe (Spalten/Filter/Export):** `Address`, `Title 1`, `Occurrences` (Anzahl Annotationen gesamt), `HTML hreflang 1..n` (Code), `HTML hreflang 1..n URL`, `HTTP hreflang 1..n`, `HTTP hreflang 1..n URL`, `Sitemap hreflang 1..n`, `Sitemap hreflang 1..n URL`. Spaltenreihenfolge pro Index: Code, dann URL. Filter TECH-073 bis TECH-086 plus „All“ (14 Filter + All). (Nachtrag 09.11.2026, 1.x: zusätzlich Filter TECH-090/TECH-091 und Spalten `HTML Lang`, `Hreflang Self Code`.) Untere Detailansicht TECH-089. Standardspalten am Ende: `Indexability`, `Indexability Status`.
- **Akzeptanzkriterien:**
  - [ ] Population: interne HTML-2xx-Seiten (K-56, vorher 200) und interne Nicht-HTML-2xx-URLs mit HTTP-hreflang; URLs mit Sitemap-hreflang, soweit sie diese Bedingung erfüllen. Gecrawlte hreflang-Ziele ohne 2xx sind keine eigene Zeile; sie erscheinen nur in TECH-074 und dessen Export (K2-85, SQ-03; vorher Teil der Population).
  - [ ] Abhängigkeit von der Konfiguration:
    - Store aus → Tab leer, alle Filter 0, Hinweis „hreflang wird nicht gespeichert“.
    - Store an, Crawl aus → Tab und alle Filter, die nur die eigene Seite brauchen (TECH-073, TECH-080–TECH-086), sind befüllt. Filter, die den Zustand des Alternates brauchen (TECH-074, TECH-076–TECH-079), werden nur für Alternates befüllt, die anderweitig gecrawlt wurden (z. B. weil sie verlinkt sind). Alternates, die **nur** per hreflang bekannt sind, erscheinen in keinem anderen Tab als URL-Zeile (keine Tabellen-Sichtbarkeit, nicht in Internal/External, nicht in den Zählern des Crawl Overview), und ihre hreflang-Linkzeilen fehlen in `Links:All Inlinks` und den übrigen Link-Exporten. In den hreflang-Spalten der Quellseite stehen sie weiterhin (Paritätsbefund M1, Befund N-08, K-62; Fixture `parity-technical` `/hl/it/`).
    - Store an, Crawl an → alles befüllt.
  - [ ] **(Paritätsabweichung):** Die Referenz verlangt laut Doku **beide** Optionen (Store und Crawl), damit Tab und Filter befüllt werden. OpenSpider befüllt den Tab schon mit Store allein, weil die eigenseitigen Prüfungen ohne Abruf der Alternates möglich sind. Paritätstests laufen mit Store **und** Crawl an. Ein Test „Store an, Crawl aus“ prüft nur das OpenSpider-Verhalten.
  - [ ] `Occurrences` = Summe aller gespeicherten Annotationen über alle drei Quellen; eine Annotation, die in HTML und Sitemap identisch vorkommt, zählt zweimal.
  - [ ] `HTML hreflang n` zeigt den Code in Originalschreibweise (Rohwert), Vergleiche nutzen den normalisierten Code (TECH-072).
  - [ ] Die Sitemap-Spalten werden gefüllt, sobald Sitemap-hreflang vorliegt. **(Paritätsabweichung):** Die Referenz füllt sie laut Doku nur im Listenmodus; Paritätstests im Spider-Modus ignorieren diese Spalten.
- **Testfixture:** `tests/fixtures/tech/hreflang/` mit drei Konfigurationsläufen (Store aus; Store an + Crawl aus; Store an + Crawl an). Golden je Lauf: Filterzahlen. Erwartung „Store an + Crawl aus“: `/en/` → `/nl/` (nur per hreflang bekannt, nicht verlinkt) erzeugt **keinen** Missing-Return-Link-Treffer, `/en/` → `/de/` (verlinkt) schon, wenn `/de/` nicht zurückverweist.

### TECH-072 Validierung von Sprach-/Regionscodes
- **Priorität:** P0
- **Beschreibung:** Prüft, ob ein hreflang-Wert syntaktisch und inhaltlich gültig ist.
- **Eingaben/Konfiguration:** Mitgelieferte Datentabellen: ISO 639-1 (zweistellige Sprachcodes), ISO 3166-1 alpha-2 (Ländercodes), ISO 15924 (Schrift-Codes). Option „Script-Subtag erlauben“ (Standard an).
- **Ausgabe (Spalten/Filter/Export):** Feld `code_valid` je Annotation; Fehlergrund (`invalid_language`, `invalid_region`, `invalid_format`).
- **Akzeptanzkriterien:**
  - [ ] Gültige Formen (case-insensitiv): `x-default`; `ll`; `ll-RR`; mit Script-Option zusätzlich `ll-Ssss` und `ll-Ssss-RR`. `ll` ∈ ISO 639-1, `RR` ∈ ISO 3166-1 alpha-2, `Ssss` ∈ ISO 15924.
  - [ ] Ungültig: `en-UK` (UK ist kein ISO-3166-Code), `en_GB` (Unterstrich), `eng` (dreistellig), `jp` (kein Sprachcode), `es-419` (numerische Region), `EU`, `de-` , leere Zeichenkette, `x-default-de`. **OS-Festlegung 13.10.2026 (K-28):** `eu` ist der ISO-639-1-Code für Baskisch und gültig. Ungültig ist nur ein allein stehendes, vollständig großgeschriebenes `EU` (gelesen als Regionscode ohne Sprache); sonst ist die Prüfung case-insensitiv.
  - [ ] Gültig: `EN-gb`, `de`, `pt-BR`, `zh-Hant`, `zh-Hant-TW`, `X-Default`.
  - [ ] Ein reiner Ländercode, der zufällig auch ein Sprachcode ist (`be` = Belarussisch), gilt als gültige Sprache.
  - [ ] Zurückgezogene ISO-639-1-Codes (`iw`, `in`, `ji`) gelten als ungültig (Festlegung).
- **Testfixture:** `tests/fixtures/tech/hreflang-codes/`: eine Seite mit allen obigen Positiv- und Negativbeispielen als Annotationen; Golden listet `code_valid` je Code.

### TECH-073 Filter „Contains Hreflang“
- **Priorität:** P0
- **Beschreibung:** URLs mit mindestens einer hreflang-Annotation aus beliebiger Quelle.
- **Eingaben/Konfiguration:** –
- **Ausgabe (Spalten/Filter/Export):** Filter „Contains Hreflang“ (Information).
- **Akzeptanzkriterien:**
  - [ ] Bedingung: Annotationsmenge nicht leer.
  - [ ] Eine URL, die nur in der Sitemap Annotationen hat, ist enthalten.
- **Testfixture:** `hreflang/en/`, `/es/` (nur Sitemap).

### TECH-074 Filter „Non-200 Hreflang URLs“
- **Priorität:** P0
- **Beschreibung:** Seiten, deren hreflang-Ziele nicht mit 200 antworten.
- **Eingaben/Konfiguration:** Ziele müssen abgerufen sein (Crawl Hreflang an oder anderweitig entdeckt). Live, inkrementell (TECH-005): keine Crawl-Analyse nötig.
- **Ausgabe (Spalten/Filter/Export):** Filter „Non-200 Hreflang URLs“; Einstufung: Fehler / Hoch. Report OUT-091 (Zeilenauswahl TECH-088).
- **Akzeptanzkriterien:**
  - [ ] Bedingung: mindestens ein Ziel mit robots-Sperre, ohne Antwort, 3xx, 4xx oder 5xx.
  - [ ] Externe Ziele werden nach Status bewertet (Statusprüfung genügt).
  - [ ] Nicht abgerufene Ziele werden nicht gemeldet.
- **Testfixture:** `hreflang/en/` mit Annotation `fr` → `/fr-old/` (301) und `it` → `/it/` (404).

### TECH-075 Filter „Unlinked Hreflang URLs“
- **Priorität:** P1
- **Beschreibung:** hreflang-Ziele, die nur über Annotationen, nicht aber über Hyperlinks gefunden werden.
- **Eingaben/Konfiguration:** Nachlauf, Bereich „Hreflang“ in der Crawl-Analyse (einziger hreflang-Filter, der zwingend die Crawl-Analyse benötigt, TECH-005).
- **Ausgabe (Spalten/Filter/Export):** Filter „Unlinked Hreflang URLs“; Einstufung: Fehler / Mittel (korrigiert; Begründung Anhang B). Gelistet wird die Quellseite; Report listet Quelle + Ziel.
- **Akzeptanzkriterien:**
  - [ ] Bedingung: Ein Alternate Q (intern, bereits gecrawlt) hat keinen eingehenden Hyperlink von einer internen Seite (Logik wie TECH-020). Nur bekannte, nicht gecrawlte Ziele lösen den Filter nicht aus. (K2-93)
  - [ ] Externe Alternates (andere Domain, nicht als intern konfiguriert) lösen den Filter immer aus, weil sie im Crawl nicht verlinkt sein können (Verhalten wie Referenz bei Mehr-Domain-Setups).
- **Testfixture:** `hreflang/en/` annotiert `/nl/`, das nirgends verlinkt ist.

### TECH-076 Filter „Missing Return Links“
- **Priorität:** P0
- **Beschreibung:** Ein Alternate verweist nicht per hreflang zurück; die Beziehung ist dann nicht wechselseitig.
- **Eingaben/Konfiguration:** Live, inkrementell (TECH-005; keine Crawl-Analyse nötig). Alternates müssen gecrawlt und geparst sein (intern oder per TECH-087 als intern behandelt).
- **Ausgabe (Spalten/Filter/Export):** Filter „Missing Return Links“; Einstufung: Fehler / Hoch. Gelistet: Seite P. Report: Zeilen (P, Q, Code auf P für Q).
- **Akzeptanzkriterien:**
  - [ ] Bedingung: Es gibt ein Alternate Q von P, Q wurde mit 200 als HTML (oder mit HTTP-hreflang) geparst, und Qs Annotationsmenge enthält keine Annotation mit URL = P.
  - [ ] Rücklink aus beliebiger Quelle zählt (P per HTML, Q per Sitemap → ok).
  - [ ] Q mit Nicht-200-Status oder nicht gecrawlt → kein Missing Return Link (sondern ggf. TECH-074).
  - [ ] Q zeigt zurück auf `http://`-Variante von P, P ist `https://` → Missing Return Link (Vergleich exakt nach TECH-002).
  - [ ] x-default-Annotationen werden wie jede andere geprüft: Ist die x-default-URL ein anderes Dokument, muss es zurückverweisen.
- **Testfixture:** `hreflang/en/` ↔ `/de/` korrekt; `/en/` → `/fr/`, aber `/fr/` ohne Rückverweis auf `/en/`; `/fr/` mit Rückverweis nur per Sitemap (Positivtest).

### TECH-077 Filter „Inconsistent Language & Region Return Links“
- **Priorität:** P1
- **Beschreibung:** Der Rücklink nennt für P einen anderen Code, als P für sich selbst angibt.
- **Eingaben/Konfiguration:** Live, inkrementell (TECH-005).
- **Ausgabe (Spalten/Filter/Export):** Filter „Inconsistent Language & Region Return Links“; Einstufung: Fehler / Hoch. Report: OUT-094 (führend für Namen und Spalten). Mapping-Aliase für den Paritätstest: Tab-Doku „Inconsistent Language & Region Return Links“ (Hauptname); Report-Menü der Referenz „Inconsistent Language Return Links“ (OpenSpider-Report-Name nach OUT-094); laut Review in der Issue-Bibliothek „Inconsistent Language & Region Confirmation Links“ (in der öffentlichen Tab-Doku nicht bestätigt, vor dem Golden-Test prüfen). In der Detailansicht TECH-089 heißt der zugehörige Bestätigungsstatus `Inconsistent`.
- **Akzeptanzkriterien:**
  - [ ] Bedingung: P hat einen Selbstcode S; ein Alternate Q hat einen Rücklink auf P mit Code C; C ≠ S (case-insensitiv verglichen). `x-default` ist kein Sprachcode und wird bei diesem Vergleich ausgeschlossen. (K2-93)
  - [ ] P ohne Selbstreferenz → Filter für P nicht auswertbar, nicht gelistet.
  - [ ] Beispiel: `/en-gb/` sagt sich selbst `en-gb`, `/de/` verweist auf `/en-gb/` mit `en` → `/en-gb/` gelistet.
- **Testfixture:** `hreflang/inconsistent/en-gb/` und `/de/` wie im Beispiel.

### TECH-078 Filter „Non-Canonical Return Links“
- **Priorität:** P1
- **Beschreibung:** Rücklinks zeigen auf eine nicht-kanonische URL.
- **Eingaben/Konfiguration:** Live, inkrementell (TECH-005).
- **Ausgabe (Spalten/Filter/Export):** Filter „Non-Canonical Return Links“; Einstufung: Fehler / Hoch.
- **Akzeptanzkriterien:**
  - [ ] Bedingung: Eine interne HTML-2xx-Seite P hat ein Alternate Q (gecrawlt, 200, HTML), und Q hat einen Canonical auf eine andere URL. Gelistet wird die Alternate-URL Q; Beziehungsreports behalten die Richtung P → Q. (K2-93)
  - [ ] Q mit Self-Canonical → nicht gelistet.
- **Testfixture:** `hreflang/noncanon/en/` → `de` `/de/?ref=1`, das Canonical `/de/` hat.

### TECH-079 Filter „Noindex Return Links“
- **Priorität:** P1
- **Beschreibung:** Ein Alternate ist per noindex gesperrt; alle Seiten eines hreflang-Sets sollten indexierbar sein.
- **Eingaben/Konfiguration:** Live, inkrementell (TECH-005).
- **Ausgabe (Spalten/Filter/Export):** Filter „Noindex Return Links“; Einstufung: Fehler / Hoch.
- **Akzeptanzkriterien:**
  - [ ] Bedingung: Ein Alternate Q von P (gecrawlt, 200) hat effektives `noindex` (inkl. `none`); P wird gelistet. Die Selbstannotation einer noindex-Seite zählt ebenfalls (Q = P). (K2-93)
- **Testfixture:** `hreflang/noindex/en/` → `/de/` mit `noindex`.

### TECH-080 Filter „Incorrect Language & Region Codes“
- **Priorität:** P0
- **Beschreibung:** Mindestens ein ungültiger Code (TECH-072).
- **Eingaben/Konfiguration:** –
- **Ausgabe (Spalten/Filter/Export):** Filter „Incorrect Language & Region Codes“; Einstufung: Fehler / Hoch.
- **Akzeptanzkriterien:**
  - [ ] Bedingung: Eine Annotation der Seite hat `code_valid = false`.
- **Testfixture:** `hreflang-codes/`.

### TECH-081 Filter „Multiple Entries“
- **Priorität:** P1
- **Beschreibung:** Derselbe Code kommt mehrfach vor – auch über verschiedene Quellen hinweg.
- **Eingaben/Konfiguration:** –
- **Ausgabe (Spalten/Filter/Export):** Filter „Multiple Entries“; Einstufung: Fehler / Hoch.
- **Akzeptanzkriterien:**
  - [ ] Bedingung: In der Annotationsmenge der Seite kommt ein normalisierter Code (kleingeschrieben) ≥ 2-mal vor – egal ob mit gleicher oder unterschiedlicher URL, egal ob in derselben oder verschiedenen Quellen.
  - [ ] `de` und `de-DE` sind verschiedene Codes → kein Treffer.
  - [ ] `x-default` doppelt → Treffer.
- **Testfixture:** `hreflang/multi/a` (zweimal `de` mit unterschiedlichen URLs), `/b` (`de` in HTML und in Sitemap, gleiche URL).

### TECH-082 Filter „Missing Self Reference“
- **Priorität:** P1
- **Beschreibung:** Seite mit hreflang, aber ohne Annotation auf sich selbst.
- **Eingaben/Konfiguration:** –
- **Ausgabe (Spalten/Filter/Export):** Filter „Missing Self Reference“; Einstufung: Warnung / Niedrig.
- **Akzeptanzkriterien:**
  - [ ] Bedingung: Annotationsmenge nicht leer und keine Annotation mit URL = Seiten-URL.
  - [ ] Eine x-default-Annotation auf die eigene URL zählt als Selbstreferenz.
- **Testfixture:** `hreflang/noself/en/` mit Annotationen nur auf `/de/` und `/fr/`.
- **Verweis (Nachtrag 09.11.2026):** Den umgekehrten Fall (nur Selbstreferenz, kein Alternate) meldet TECH-090; den Widerspruch zwischen Selbstcode und `<html lang>` TECH-091.

### TECH-083 Filter „Not Using Canonical“
- **Priorität:** P1
- **Beschreibung:** Die Seite ist canonicalisiert, und ihre eigenen Annotationen verwenden nicht die Canonical-URL.
- **Eingaben/Konfiguration:** –
- **Ausgabe (Spalten/Filter/Export):** Filter „Not Using Canonical“; Einstufung: Fehler / Hoch.
- **Akzeptanzkriterien:**
  - [ ] Bedingung: Annotationsmenge nicht leer, Seite hat Canonical C ≠ eigene URL, und keine Annotation der Seite hat URL = C.
  - [ ] Self-Canonical-Seiten werden hier nie gelistet (dafür TECH-082).
- **Testfixture:** `hreflang/notcanon/en/?utm=x` mit Canonical `/en/` und Selbst-Annotation auf `?utm=x`.

### TECH-084 Filter „Missing X-Default“
- **Priorität:** P1
- **Beschreibung:** Seite mit hreflang, aber ohne `x-default`. Optional, kein Fehler im engeren Sinn.
- **Eingaben/Konfiguration:** –
- **Ausgabe (Spalten/Filter/Export):** Filter „Missing X-Default“; Einstufung: Warnung / Niedrig.
- **Akzeptanzkriterien:**
  - [ ] Bedingung: Annotationsmenge nicht leer und kein Code `x-default` (case-insensitiv).
- **Testfixture:** `hreflang/en/` ohne x-default; `/de/` mit.

### TECH-085 Filter „Missing“ (Hreflang)
- **Priorität:** P0
- **Beschreibung:** Seiten ganz ohne hreflang (kann korrekt sein, wenn es keine Sprachvarianten gibt).
- **Eingaben/Konfiguration:** –
- **Ausgabe (Spalten/Filter/Export):** Filter „Missing“ (Information, keine Issue-Einstufung).
- **Akzeptanzkriterien:**
  - [ ] Bedingung: interne HTML-200-Seite mit leerer Annotationsmenge.
- **Testfixture:** `hreflang/about.html`.

### TECH-086 Filter „Outside \<head>“ (Hreflang)
- **Priorität:** P1
- **Beschreibung:** hreflang-Link-Element außerhalb des Heads (TECH-003).
- **Eingaben/Konfiguration:** Nur befüllt bei „Perform HTML Validation“ an (TECH-008).
- **Ausgabe (Spalten/Filter/Export):** Filter „Outside \<head>“; Einstufung: Fehler / Hoch.
- **Akzeptanzkriterien:**
  - [ ] Bedingung: mindestens eine HTML-Annotation mit `in_head = false`.
  - [ ] Annotationen außerhalb des Heads zählen **nur** für diesen Filter: nicht in den hreflang-Spalten, nicht in `Occurrences` und nicht für die übrigen hreflang-Filter (z. B. Missing Self Reference, Missing x-default); unabhängig von TECH-008 (K2-86, SQ-04; vorher „zählen für alle anderen Filter“; Beleg Referenz `lang-outside.html`).
- **Testfixture:** `outside-head/hreflang-body.html`.

### TECH-087 Hreflang crawlen, externe Domains als intern (CDN-Liste)
- **Priorität:** P1
- **Beschreibung:** Steuert, ob hreflang-Ziele abgerufen werden, und erlaubt, weitere Domains (z. B. Länderdomains) als intern zu behandeln, damit deren HTML geparst und Rücklinks geprüft werden.
- **Eingaben/Konfiguration:** „Crawl Hreflang“ (Standard aus); Liste zusätzlicher als intern geltender Hosts/Präfixe („CDNs“-Konfiguration, gemeinsam mit Crawl-Spec).
- **Ausgabe (Spalten/Filter/Export):** hreflang-Ziele erscheinen als URLs mit Inlink-Typ „Hreflang“.
- **Akzeptanzkriterien:**
  - [ ] Crawl Hreflang aus → Ziele, die nur per hreflang bekannt sind, werden nicht abgerufen; TECH-074/076–079 bleiben für sie leer.
  - [ ] Crawl Hreflang an, Ziel auf fremder Domain → nur Statusprüfung (kein HTML), Rücklinkprüfung entfällt.
  - [ ] Fremde Domain in der Intern-Liste → wird wie intern gecrawlt und geparst; Rücklinks werden geprüft.
- **Testfixture:** `tests/fixtures/tech/hreflang-multidomain/`: Fixture-Server bedient zwei Hosts (`de.site.test`, `fr.site.test`, Auflösung per Host-Header/Resolver-Override); Läufe mit und ohne Intern-Liste.

### TECH-088 Hreflang-Reports (Bulk-Exporte je Problemtyp)
- **Priorität:** P1
- **Verweis:** siehe OUT-091 (führend). Die Zeilenauswahl aller Hreflang-Reports OUT-091 … OUT-096 ist dort festgelegt.

### TECH-089 Untere Detailansicht „URL Details“ mit Bestätigungsstatus je Annotation
- **Priorität:** P1
- **Beschreibung:** Wählt man im Hreflang-Tab eine URL, zeigt die untere Ansicht jede ihrer Annotationen als Zeile mit dem Ergebnis der Rücklinkprüfung. So sieht man auf einen Blick, welches Alternate das Problem verursacht.
- **Eingaben/Konfiguration:** –
- **Ausgabe (Spalten/Filter/Export):** Spalten: `Source` (html/http/sitemap), `Language Code`, `Hreflang URL`, `Status Code` (des Alternates, leer wenn nicht abgerufen), `Indexability`, `Confirmation` (Bestätigungsstatus). Export-Schaltfläche der unteren Ansicht (CSV/XLSX der aktuell gewählten URL(s)).
- **Akzeptanzkriterien:**
  - [ ] Bestätigungsstatus je Annotation (genau einer, erste zutreffende Regel gewinnt): `Self` (URL = eigene Seite) → `Not Crawled` (Alternate nicht abgerufen) → `Non-200` (Status ≠ 200, inkl. robots-gesperrt) → `Noindex` (Alternate effektiv noindex) → `Non-Canonical` (Alternate canonicalisiert) → `Missing` (kein Rücklink) → `Inconsistent` (Rücklink mit anderem Code als dem Selbstcode, TECH-077) → `Confirmed`. Die Statusnamen `Missing` und `Inconsistent` entsprechen der Referenz; die übrigen sind Festlegung (Anhang A).
  - [ ] Konsistenz mit den Filtern: Hat eine Annotation den Status `Missing`, steht die Seite in TECH-076; analog `Inconsistent` → TECH-077, `Non-200` → TECH-074, `Noindex` → TECH-079, `Non-Canonical` → TECH-078. Die Umkehrung gilt nicht zwingend: Eine Annotation, die mehrere Bedingungen erfüllt (z. B. noindex **und** ohne Rücklink), zeigt nur den vorrangigen Status, die Seite steht aber in allen zutreffenden Filtern.
  - [ ] Bei abgeschnittenen Annotationen (TECH-004) zeigt die Ansicht oben den Hinweis „N weitere Annotationen nicht ausgewertet“.
  - [ ] Mehrfachauswahl im oberen Tab → Zeilen aller gewählten URLs, mit zusätzlicher erster Spalte `Address`.
- **Testfixture:** `tests/fixtures/tech/hreflang/`: `/en/` mit Annotationen auf sich selbst (`Self`), `/de/` (`Confirmed`), `/fr/` (`Missing`), `/it/` (404 → `Non-200`), `/nl/` (bei Crawl Hreflang aus → `Not Crawled`), `/es/` (noindex → `Noindex`), `/pt/?ref=1` (canonicalisiert → `Non-Canonical`); Golden-Export der unteren Ansicht.

### TECH-090 Filter „Self Reference Only“ (Nachtrag 09.11.2026)
- **Priorität:** P1
- **Meilenstein:** 1.x (nach 1.0) – OS-Festlegung, Nutzerentscheidung 09.11.2026, ohne Paritätsbezug.
- **Beschreibung:** Seite, deren hreflang-Annotationen nur auf sie selbst verweisen: Es gibt eine Selbstreferenz, aber kein Alternate. Eine solche Gruppe aus einer einzigen Seite hat keine Wirkung; häufig fehlen die Alternates wegen eines Vorlagenfehlers (z. B. Sprachumschalter liefert nur die aktuelle Sprache).
- **Abgrenzung zu TECH-082:** TECH-082 meldet Seiten **mit** Alternates, aber **ohne** Selbstreferenz; dieser Filter meldet Seiten **mit** Selbstreferenz, aber **ohne** Alternate. Beide Mengen sind disjunkt; eine Seite ohne jede Annotation steht in keinem der beiden (sondern in TECH-085).
- **Eingaben/Konfiguration:** –
- **Ausgabe (Spalten/Filter/Export):** Filter „Self Reference Only“ im Tab Hreflang; Einstufung: Warnung / Niedrig (UI-063: `T-W2`, `P-L1`; kann bei einsprachigen Websites mit generierter Annotation gewollt sein und richtet keinen Schaden an).
- **Akzeptanzkriterien:**
  - [ ] Bedingung: Annotationsmenge nicht leer, mindestens eine Annotation mit URL = Seiten-URL (Vergleich nach TECH-002) und kein Alternate (keine Annotation mit URL ≠ Seiten-URL).
  - [ ] `en` → self und `x-default` → self → im Filter (beides zeigt auf die Seite selbst).
  - [ ] `en` → self und `de` → `/de/` → nicht im Filter.
  - [ ] Nur `de` → `/de/` (ohne Selbstreferenz) → nicht hier, sondern in TECH-082.
  - [ ] Quellen werden vereinigt: Selbstreferenz im HTML und Alternate nur in der Sitemap → nicht im Filter.
  - [ ] Abgeschnittene Annotationsmengen (TECH-004) werden mit den gespeicherten Annotationen bewertet.
- **Testfixture:** `tests/fixtures/tech/hreflang/selfonly/`: `a.html` (nur Selbstreferenz), `b.html` (Selbstreferenz + x-default auf sich), `c.html` (Selbstreferenz + `/de/`), `d.html` (nur `/de/`), `e.html` (Selbstreferenz im HTML, Alternate in `sitemap-selfonly.xml`).

### TECH-091 Filter „Hreflang Language Differs From HTML Lang“ (Nachtrag 09.11.2026)
- **Priorität:** P1
- **Meilenstein:** 1.x (nach 1.0) – OS-Festlegung, Nutzerentscheidung 09.11.2026, ohne Paritätsbezug.
- **Beschreibung:** Die Sprache, mit der sich eine Seite in ihrer hreflang-Selbstreferenz ausweist (Selbstcode), widerspricht der Sprachangabe im `lang`-Attribut des `<html>`-Elements. Meist ist eine der beiden Angaben aus einer Vorlage kopiert und nicht angepasst worden.
- **Eingaben/Konfiguration:** –
- **Ausgabe (Spalten/Filter/Export):** Filter „Hreflang Language Differs From HTML Lang“ im Tab Hreflang; zusätzliche Spalten `HTML Lang` (Rohwert) und `Hreflang Self Code`. Einstufung: Warnung / Mittel (UI-063: `T-W2`, `P-M1`; Suchmaschinen werten vorrangig hreflang aus, ein Widerspruch deutet aber auf eine falsch zugeordnete Sprachversion hin).
- **Akzeptanzkriterien:**
  - [ ] Verglichen wird nur der primäre Sprach-Subtag (BCP 47, vor dem ersten `-` bzw. `_`), ohne Groß-/Kleinschreibung; Region und Schrift werden nicht verglichen: Selbstcode `en-GB` und `lang="en-US"` → nicht im Filter; `de-AT` und `lang="en"` → im Filter.
  - [ ] Sprachangabe der Seite: `<html lang>`, bei fehlendem `lang` das Attribut `xml:lang` (gleiche Regel wie ONPAGE-153).
  - [ ] Nicht bewertet (nicht im Filter): Seite ohne Selbstreferenz (dafür TECH-082), Selbstcode nur `x-default`, fehlendes oder leeres `lang`, ungültiger Selbstcode (dafür TECH-080).
  - [ ] Mehrere Selbstreferenzen mit verschiedenen Sprachen: Es gilt der Selbstcode nach der Begriffsdefinition dieses Abschnitts (der erste); die übrigen meldet TECH-081.
  - [ ] Ein ungültiger `lang`-Wert (z. B. `lang="english"`) wird wörtlich als Subtag verglichen und damit gemeldet, sofern er nicht zum Selbstcode passt.
- **Testfixture:** `tests/fixtures/tech/hreflang/lang/`: `en-gb.html` (`en-GB`, `lang="en-US"`), `de-at.html` (`de-AT`, `lang="en"`), `xml-lang.html` (nur `xml:lang="fr"`, Selbstcode `fr`), `no-lang.html`, `english.html` (`lang="english"`, Selbstcode `en`).

---

## JavaScript

Alle Funktionen dieses Abschnitts setzen den Rendering-Modus „JavaScript“ voraus (TECH-006). Im Modus „Text only“ ist der Tab leer, und die Übersicht zeigt den Hinweis „nur im JavaScript-Rendering-Modus“.

### TECH-100 Vergleich Original-HTML vs. gerendertes HTML (Grundlage)
- **Priorität:** P1
- **Beschreibung:** Für jede gerenderte interne HTML-Seite werden SEO-relevante Elemente einmal aus dem Original-HTML und einmal aus dem gerenderten DOM extrahiert und verglichen.
- **Eingaben/Konfiguration:** Rendering-Modus JavaScript; Inhaltsbereich für Wortzählung (Content-Area-Konfiguration der Content-Spec; Standard: `<nav>`, `<footer>`, `<header>` und `<aside>` ausgeschlossen, SQ-09/K2-95).
- **Ausgabe (Spalten/Filter/Export):** Paare `HTML <X>` / `Rendered HTML <X>` für Title, Meta Description, H1 (erstes), Canonical (maßgeblich), Meta Robots (alle Token, kommagetrennt); Wortzahlen; Linkmengen.
- **Akzeptanzkriterien:**
  - [ ] Textvergleich: führende/abschließende Leerzeichen entfernt, interne Whitespace-Folgen zu einem Leerzeichen zusammengefasst, HTML-Entitäten dekodiert; danach exakter, case-sensitiver Vergleich.
  - [ ] „Vorhanden“ heißt: Element existiert **und** sein normalisierter Text ist nicht leer.
  - [ ] Wortzählung: sichtbarer Text im Body innerhalb des Inhaltsbereichs, an Whitespace getrennt; Sichtbarkeit (CSS) wird nicht berücksichtigt.
  - [ ] JS-Link: Hyperlink (Zielmenge normalisiert nach TECH-002), dessen Ziel im gerenderten DOM vorkommt, aber nicht im Original-HTML derselben Seite.
- **Testfixture:** `tests/fixtures/tech/js/`: `static.html` (keine JS-Änderungen), `spa.html` (leerer Body, Inhalt + Links + Title per JS), `mutate.html` (Title/H1/Description per JS geändert).

### TECH-101 Tab „JavaScript“: Population und Spalten
- **Priorität:** P1
- **Beschreibung:** Gegenüberstellung je Seite.
- **Eingaben/Konfiguration:** –
- **Ausgabe (Spalten/Filter/Export):** `Address`, `Status Code`, `Status`, `Indexability`, `Indexability Status`, `HTML Word Count`, `Rendered HTML Word Count`, `Word Count Change`, `JS Word Count %`, `HTML Title`, `Rendered HTML Title`, `HTML Meta Description`, `Rendered HTML Meta Description`, `HTML H1`, `Rendered HTML H1`, `HTML Canonical`, `Rendered HTML Canonical`, `HTML Meta Robots`, `Rendered HTML Meta Robots`, `Unique Inlinks`, `Unique JS Inlinks`, `Unique Outlinks`, `Unique JS Outlinks`, `Unique External Outlinks`, `Unique External JS Outlinks`. Filter TECH-102 bis TECH-114 plus „All“.
- **Akzeptanzkriterien:**
  - [ ] Population: interne HTML-Seiten, die gerendert wurden (Status 200).
  - [ ] `Word Count Change` = `Rendered HTML Word Count − HTML Word Count` (kann negativ sein).
  - [ ] `JS Word Count %` = `max(Word Count Change, 0) / Rendered HTML Word Count × 100`, auf 2 Nachkommastellen; bei gerenderter Wortzahl 0 → 0 (Festlegung).
  - [ ] `Unique JS Inlinks` = Anzahl verschiedener interner Quellseiten, die nur per JS-Link auf die URL verweisen; `Unique JS Outlinks` = Anzahl verschiedener interner Ziele, die nur per JS-Link erreicht werden; `Unique External JS Outlinks` analog extern.
- **Testfixture:** `tests/fixtures/tech/js/`; Golden: `spa.html` mit `HTML Word Count = 0`, `Rendered HTML Word Count = 120`, `JS Word Count % = 100.00`.

### TECH-102 Filter „Pages With Blocked Resources“
- **Priorität:** P1
- **Beschreibung:** Beim Rendern wurden Seitenressourcen (JS, CSS, Bilder, Fonts, XHR) wegen robots.txt nicht geladen; das Rendering kann unvollständig sein.
- **Eingaben/Konfiguration:** robots.txt-Beachtung aktiv; Ressourcenanfragen des Renderers laufen durch dieselbe robots-Prüfung.
- **Ausgabe (Spalten/Filter/Export):** Filter „Pages With Blocked Resources“; Einstufung: Warnung / Hoch. Export „Blocked Resources“ mit Seite, Ressource, Ressourcentyp.
- **Akzeptanzkriterien:**
  - [ ] Bedingung: mindestens eine vom Renderer angefragte Ressource wurde wegen robots.txt blockiert.
  - [ ] Blockierte externe Ressourcen zählen (robots.txt des fremden Hosts), sofern die Option „robots.txt externer Ressourcen beachten“ aktiv ist (Standard an, Festlegung).
  - [ ] Bei „robots.txt ignorieren“ ist der Filter leer.
- **Testfixture:** `js/blocked.html` lädt `/assets-blocked/app.js`; robots.txt sperrt `/assets-blocked/`.

### TECH-103 Filter „Contains JavaScript Links“
- **Priorität:** P1
- **Beschreibung:** Hyperlinks, die erst nach dem Rendern existieren.
- **Eingaben/Konfiguration:** –
- **Ausgabe (Spalten/Filter/Export):** Filter „Contains JavaScript Links“; Einstufung: Warnung / Mittel. Im unteren Outlinks-Bereich Spalte `Link Origin` = `HTML` / `Rendered HTML` / `HTML & Rendered HTML`.
- **Akzeptanzkriterien:**
  - [ ] Bedingung: `Unique JS Outlinks + Unique External JS Outlinks ≥ 1`.
  - [ ] Ein Link, der in beiden Fassungen existiert, ist kein JS-Link.
- **Testfixture:** `js/spa.html` (3 JS-Links), `static.html` (0).

### TECH-104 Filter „Contains JavaScript Content“
- **Priorität:** P1
- **Beschreibung:** Fließtext, der erst durch JavaScript in die Seite kommt.
- **Eingaben/Konfiguration:** –
- **Ausgabe (Spalten/Filter/Export):** Filter „Contains JavaScript Content“; Einstufung: Warnung / Mittel.
- **Akzeptanzkriterien:**
  - [ ] Bedingung: `Rendered HTML Word Count > HTML Word Count` (Festlegung; die Referenz nennt keine Schwelle).
  - [ ] Reine Umformatierung ohne neue Wörter → nicht gelistet.
- **Testfixture:** `js/spa.html` (gelistet), `js/static.html` (nicht).

### TECH-105 Filter „Noindex Only In Original HTML“
- **Priorität:** P1
- **Beschreibung:** Die Robots-Angabe widerspricht sich zwischen den Fassungen: Der Server liefert `noindex`, das Skript nimmt es wieder heraus. Da die Indexierungsentscheidung bereits auf Basis der ausgelieferten Fassung fallen kann, bevor ein Renderdurchgang stattfindet, ist die Absicht „Seite soll indexiert werden“ nicht zuverlässig umgesetzt. OpenSpider behandelt die Seite deshalb vorsichtig als nicht indexierbar.
- **Eingaben/Konfiguration:** –
- **Ausgabe (Spalten/Filter/Export):** Filter „Noindex Only In Original HTML“; Einstufung: Fehler / Hoch.
- **Akzeptanzkriterien:**
  - [ ] Bedingung: Token `noindex` oder `none` in Meta Robots des Originals, aber in keinem Meta Robots des gerenderten DOM.
  - [ ] Indexierbarkeit der Seite: `noindex` (TECH-006).
  - [ ] X-Robots-Tag spielt hier keine Rolle (identisch in beiden Fassungen).
- **Testfixture:** `js/noindex-removed.html` – Skript entfernt das Meta-Element.

### TECH-106 Filter „Nofollow Only In Original HTML“
- **Priorität:** P1
- **Beschreibung:** Analog TECH-105 für `nofollow`.
- **Eingaben/Konfiguration:** –
- **Ausgabe (Spalten/Filter/Export):** Filter „Nofollow Only In Original HTML“; Einstufung: Fehler / Hoch.
- **Akzeptanzkriterien:**
  - [ ] Bedingung: `nofollow` oder `none` im Original, nicht im gerenderten DOM.
  - [ ] Links dieser Seite werden als `Follow = false` behandelt (Original gewinnt, Festlegung).
- **Testfixture:** `js/nofollow-removed.html`.

### TECH-107 Filter „Canonical Only In Rendered HTML“
- **Priorität:** P1
- **Beschreibung:** Canonical existiert erst nach dem Rendern.
- **Eingaben/Konfiguration:** –
- **Ausgabe (Spalten/Filter/Export):** Filter „Canonical Only In Rendered HTML“; Einstufung: Warnung / Niedrig.
- **Akzeptanzkriterien:**
  - [ ] Bedingung: kein HTML-Canonical im Original, mindestens einer im gerenderten DOM. HTTP-Canonical wird nicht berücksichtigt.
- **Testfixture:** `js/canonical-added.html`.

### TECH-108 Filter „Canonical Mismatch“
- **Priorität:** P1
- **Beschreibung:** Canonical im Original und im gerenderten DOM unterscheiden sich.
- **Eingaben/Konfiguration:** –
- **Ausgabe (Spalten/Filter/Export):** Filter „Canonical Mismatch“; Einstufung: Fehler / Hoch.
- **Akzeptanzkriterien:**
  - [ ] Bedingung: beide Fassungen haben einen HTML-Canonical, und die maßgeblichen URLs sind nach TECH-002 verschieden.
  - [ ] JS ändert nur Großschreibung des Hosts → kein Mismatch.
- **Testfixture:** `js/canonical-changed.html` (Original `/a`, gerendert `/b`), `/canonical-host-case.html`.

### TECH-109 Filter „Page Title Only In Rendered HTML“ / „Page Title Updated By JavaScript“
- **Priorität:** P1
- **Beschreibung:** Zwei Filter zum Seitentitel.
- **Eingaben/Konfiguration:** –
- **Ausgabe (Spalten/Filter/Export):** Filter „Page Title Only In Rendered HTML“ und „Page Title Updated By JavaScript“; Einstufung je: Warnung / Mittel.
- **Akzeptanzkriterien:**
  - [ ] Only In Rendered: Original ohne vorhandenen Title (TECH-100 „vorhanden“), gerendert mit.
  - [ ] Updated: beide vorhanden, normalisierte Texte verschieden.
  - [ ] Eine Seite steht höchstens in einem der beiden Filter.
- **Testfixture:** `js/spa.html` (Only), `js/mutate.html` (Updated).

### TECH-110 Filter „Meta Description Only In Rendered HTML“ / „Meta Description Updated By JavaScript“
- **Priorität:** P1
- **Beschreibung:** Zwei Filter zur Meta Description (erste `<meta name="description">`).
- **Eingaben/Konfiguration:** –
- **Ausgabe (Spalten/Filter/Export):** Filter wie benannt; Einstufung je: Warnung / Mittel.
- **Akzeptanzkriterien:**
  - [ ] Logik identisch zu TECH-109, angewendet auf das Attribut `content`.
- **Testfixture:** `js/spa.html`, `js/mutate.html`.

### TECH-111 Filter „H1 Only In Rendered HTML“ / „H1 Updated By JavaScript“
- **Priorität:** P1
- **Beschreibung:** Zwei Filter zur ersten H1.
- **Eingaben/Konfiguration:** –
- **Ausgabe (Spalten/Filter/Export):** Filter wie benannt; Einstufung je: Warnung / Mittel.
- **Akzeptanzkriterien:**
  - [ ] Logik identisch zu TECH-109, angewendet auf den Textinhalt der ersten `<h1>` (inkl. Alt-Text enthaltener Bilder gemäß Heading-Spec).
- **Testfixture:** `js/spa.html`, `js/mutate.html`.

### TECH-112 Filter „Uses Old AJAX Crawling Scheme URLs“
- **Priorität:** P2
- **Beschreibung:** URLs mit Hashbang `#!` aus dem abgekündigten AJAX-Crawling-Schema.
- **Eingaben/Konfiguration:** –
- **Ausgabe (Spalten/Filter/Export):** Filter „Uses Old AJAX Crawling Scheme URLs“; Einstufung: Fehler / Mittel.
- **Akzeptanzkriterien:**
  - [ ] Bedingung: die (vor Fragment-Entfernung erfasste) URL einer gecrawlten Seite enthält `#!`, oder die Seite wurde über einen Link mit `#!` entdeckt.
  - [ ] `#!` muss beim Crawlen erhalten bleiben (Ausnahme von der sonstigen Fragment-Entfernung), damit `/#!/produkte` und `/#!/kontakt` getrennte URLs sind.
  - [ ] Dieser Filter ist auch im Text-only-Modus befüllbar (Festlegung).
- **Testfixture:** `js/hashbang.html` verlinkt `/#!/produkte`.

### TECH-113 Filter „Uses Old AJAX Crawling Scheme Meta Fragment Tag“
- **Priorität:** P2
- **Beschreibung:** Seiten mit `<meta name="fragment" content="!">`.
- **Eingaben/Konfiguration:** –
- **Ausgabe (Spalten/Filter/Export):** Filter wie benannt; Einstufung: Fehler / Mittel.
- **Akzeptanzkriterien:**
  - [ ] Bedingung: Meta-Element mit `name="fragment"` (case-insensitiv) im Original-HTML vorhanden; Wert egal.
- **Testfixture:** `js/meta-fragment.html`.

### TECH-114 Filter „Pages With JavaScript Errors“ und Konsolen-Log
- **Priorität:** P1
- **Beschreibung:** Während des Renderns aufgetretene JavaScript-Fehler, wie sie in der Konsole des eingebauten Chromium erscheinen. Führend für die **Filterlogik** (K-13). Die Erfassung der Konsolenmeldungen (Schalter, Speicherlimit je Seite, Ausfiltern interner Browser-Meldungen) regelt CRAWL-084, den Detail-Tab „Chrome Console Log“ (Layout, Level-Filter) UI-040, Bulk-Export und Summary-Report OUT-058/OUT-107.
- **Eingaben/Konfiguration:** Schalter „JavaScript-Fehlerberichte aktivieren“ (CRAWL-084, Standardwert dort) und Rendering-Modus „JavaScript“ (TECH-006). Ist der Schalter aus, bleibt der Filter leer und trägt in der Übersicht den Hinweis „JavaScript-Fehlererfassung deaktiviert“ statt einer 0.
- **Ausgabe (Spalten/Filter/Export):** Filter „Pages With JavaScript Errors“; Einstufung: Warnung / Niedrig. Datengrundlage je Seite: Konsoleneinträge mit `Level`, `Message`, `Source URL`, `Line`, `Column`, `Timestamp` (Anzeige im Detail-Tab UI-040). Bulk-Export „Bulk Export > JavaScript > Pages With JavaScript Issues“ (Referenzname, korrigiert; Format OUT-058): eine Zeile je Konsoleneintrag der Seiten im Filter, Spalten `Address`, `Level`, `Message`, `Source URL`, `Line`, `Column`. Der frühere Arbeitsname „JavaScript > Console Log“ entfällt.
- **Akzeptanzkriterien:**
  - [ ] Bedingung: mindestens ein Konsoleneintrag mit Level `error` oder eine nicht abgefangene Ausnahme (`Runtime.exceptionThrown`) bis zum Ende des AJAX-Timeouts.
  - [ ] Warnungen und `console.log` erscheinen im Log, lösen den Filter aber nicht aus.
  - [ ] Fehlgeschlagene Netzwerkanfragen (404 einer Ressource) gelten nur dann als Fehler, wenn Chromium sie als Konsolen-Fehler meldet.
  - [ ] Der Bulk-Export enthält nur Seiten, die im Filter stehen, aber für diese **alle** Konsoleneinträge (auch Warnungen), damit der Kontext erhalten bleibt (Festlegung).
- **Testfixture:** `js/error.html` (`throw new Error('x')`), `js/console-error.html` (`console.error`), `js/warn.html` (nur `console.warn`, Negativtest).

---

## Links

### TECH-130 Tab „Links“: Population und Spalten
- **Priorität:** P0
- **Beschreibung:** Interne Linkstruktur je Seite: Tiefe, Ein- und Ausgänge, Link Score.
- **Eingaben/Konfiguration:** Link-Präferenzen (TECH-144).
- **Ausgabe (Spalten/Filter/Export):** `Address`, `Indexability`, `Indexability Status`, `Crawl Depth`, `Link Score`, `Inlinks`, `Unique Inlinks`, `Unique JS Inlinks`, `% of Total`, `Outlinks`, `Unique Outlinks`, `Unique JS Outlinks`, `External Outlinks`, `Unique External Outlinks`, `Unique External JS Outlinks`. Filter TECH-132 bis TECH-143 plus „All“. (Nachtrag 09.11.2026, 1.x: zusätzlich Spalte `Empty Anchor Tags` und Filter TECH-147.)
- **Akzeptanzkriterien:**
  - [ ] Population: interne HTML-Seiten mit Status 2xx (K-56, vorher 200; `% of Total` mit demselben Nenner wie ONPAGE-015).
  - [ ] `Inlinks` = Anzahl aller eingehenden Hyperlinks (inkl. Mehrfachlinks derselben Quelle); `Unique Inlinks` = Anzahl verschiedener Quellseiten. Self-Links zählen nicht (Festlegung, entspricht Standardanzeige).
  - [ ] `Inlinks` ist eine Standardspalte und erscheint in der Standardansicht und im Standardexport.
  - [ ] `Outlinks`/`Unique Outlinks` = interne ausgehende Hyperlinks gesamt/verschiedene Ziele; `External Outlinks`/`Unique External Outlinks` analog extern.
  - [ ] Nur Hyperlinks (`<a href>`, `<area href>`) zählen hier; Ressourcen-Links (img, script, link) nicht.
  - [ ] `Crawl Depth` = Definition der Crawl-Spec (Startseite 0); unerreichbare (z. B. nur Sitemap) Seiten haben leere Tiefe.
- **Testfixture:** `tests/fixtures/tech/links/`: Baum mit 6 Ebenen (Start → /l1 → /l2 → … → /l5), Seiten mit Mehrfachlinks auf dasselbe Ziel, Self-Link, externe Links.

### TECH-131 Link-Attribute: Follow, Anchor, Link-Pfad, Crawlability
- **Priorität:** P0
- **Beschreibung:** Eigenschaften jedes Hyperlinks, auf denen die Filter beruhen.
- **Eingaben/Konfiguration:** –
- **Ausgabe (Spalten/Filter/Export):** In unteren Inlinks/Outlinks-Ansichten: `Type`, `From`, `To`, `Anchor Text`, `Alt Text`, `Follow` (true/false), `Target`, `Rel`, `Status Code`, `Link Path` (XPath), `Link Position` (Head/Navigation/Header/Content/Sidebar/Footer – Spec der Link-Positionen OUT-031; `Head` für Elemente im `<head>`, K-63), `Link Origin` (TECH-103), `Link Crawlability` (TECH-143).
- **Akzeptanzkriterien:**
  - [ ] `Follow = false`, wenn `rel` eines der Token `nofollow`, `ugc`, `sponsored` enthält **oder** die Quellseite effektives Seiten-nofollow hat (TECH-066).
  - [ ] Anchor Text = normalisierter Textinhalt des `<a>`. Ist er leer und enthält der Link ein `<img>` mit nicht-leerem `alt`, wird `Alt Text` gesetzt und gilt als Anchor für TECH-135.
  - [ ] `aria-label` und `title` werden nicht als Anchor gewertet (Festlegung).
  - [ ] Neben der Einzel-Link-Liste gibt es in den unteren Inlinks-/Outlinks-Ansichten die gruppierte Ankertext-Ansicht „Anchors“ (TECH-146). Sie nutzt denselben Anchor-Text wie hier definiert.
- **Testfixture:** `links/attrs.html`: Links mit `nofollow`, `ugc`, `sponsored`, `noopener` (Follow true), leerem Text, Bild mit/ohne Alt, aria-label.

### TECH-132 Filter „Pages With High Crawl Depth“
- **Priorität:** P0
- **Beschreibung:** Seiten, die viele Klicks von der Startseite entfernt sind.
- **Eingaben/Konfiguration:** Schwelle „High Crawl Depth“ (Standard 4).
- **Ausgabe (Spalten/Filter/Export):** Filter „Pages With High Crawl Depth“; Einstufung: Chance / Mittel.
- **Akzeptanzkriterien:**
  - [ ] Bedingung: `Crawl Depth ≥ Schwelle` (Standard: Tiefe 4 und mehr).
  - [ ] Schwelle 6 → Seite mit Tiefe 5 nicht gelistet.
  - [ ] Bei „Ignore Non-Indexable URLs for Issues“ an (TECH-009) werden nur indexierbare Seiten gelistet.
- **Testfixture:** `links/` Kette bis Tiefe 5 → Tiefe 4 und 5 gelistet.

### TECH-133 Filter „Pages Without Internal Outlinks“
- **Priorität:** P0
- **Beschreibung:** Sackgassen-Seiten ohne interne Hyperlinks.
- **Eingaben/Konfiguration:** –
- **Ausgabe (Spalten/Filter/Export):** Filter „Pages Without Internal Outlinks“; Einstufung: Warnung / Hoch.
- **Akzeptanzkriterien:**
  - [ ] Bedingung: `Outlinks = 0` (interne Hyperlinks im ausgewerteten HTML; im JS-Modus das gerenderte).
  - [ ] Eine Seite mit nur einem Self-Link gilt als ohne interne Outlinks (Festlegung K-07; die Referenz zählt den Selbstlink, `parity-technical` `/links/self-only.html`; bestätigt durch K-71, DEV-036).
  - [ ] Eine Seite mit nur nofollow-Links hat Outlinks > 0 und wird nicht gelistet.
  - [ ] Bei „Ignore Non-Indexable URLs for Issues“ an (CRAWL-089, Standard) werden nur indexierbare Seiten gelistet (Paritätsbefund M1, Befund B-05, K-56).
- **Testfixture:** `links/deadend.html`, `links/self-only.html`, dazu eine `noindex`-Sackgasse.

### TECH-134 Filter „Internal Nofollow Outlinks“
- **Priorität:** P0
- **Beschreibung:** Seiten mit mindestens einem internen Link, der nicht verfolgt werden soll.
- **Eingaben/Konfiguration:** –
- **Ausgabe (Spalten/Filter/Export):** Filter „Internal Nofollow Outlinks“; Einstufung: Warnung / Niedrig. Export der betroffenen Links.
- **Akzeptanzkriterien:**
  - [ ] Bedingung: mindestens ein interner Outlink mit `Follow = false` aufgrund von `rel` (nofollow/ugc/sponsored).
  - [ ] Seiten-nofollow allein (Meta) löst diesen Filter nicht aus – dafür Directives > Nofollow (Festlegung).
- **Testfixture:** `links/attrs.html`.

### TECH-135 Filter „Internal Outlinks With No Anchor Text“
- **Priorität:** P0
- **Beschreibung:** Interne Links ohne Ankertext bzw. Bild-Links ohne Alt-Text.
- **Eingaben/Konfiguration:** –
- **Ausgabe (Spalten/Filter/Export):** Filter „Internal Outlinks With No Anchor Text“; Einstufung: Chance / Niedrig.
- **Akzeptanzkriterien:**
  - [ ] Bedingung: mindestens ein interner Outlink, bei dem Anchor Text und Alt Text beide leer sind (nach Trim).
  - [ ] `<a href="/x"><img src="a.png" alt=""></a>` → zählt. `<a href="/x"><img alt="Logo"></a>` → zählt nicht.
  - [ ] Links in `<area>` ohne `alt` zählen.
- **Testfixture:** `links/no-anchor.html`.
- **Verweis (Nachtrag 09.11.2026):** Leere `<a>`-Elemente je Seite, auch extern und nicht verfolgt, meldet TECH-147 (Abgrenzung dort).

### TECH-136 Filter „Non-Descriptive Anchor Text In Internal Outlinks“
- **Priorität:** P1
- **Beschreibung:** Interne Links mit nichtssagendem Ankertext („hier klicken“).
- **Eingaben/Konfiguration:** Liste nichtssagender Ankertexte (TECH-144).
- **Ausgabe (Spalten/Filter/Export):** Filter „Non-Descriptive Anchor Text In Internal Outlinks“; Einstufung: Chance / Niedrig.
- **Akzeptanzkriterien:**
  - [ ] Normalisierung des Ankertexts: Kleinbuchstaben (Unicode), Trim, Whitespace zusammenfassen, abschließende Satzzeichen `.,:;!?…»›→` entfernen.
  - [ ] Bedingung: normalisierter Anchor (bzw. Alt Text bei Bild-Links) ist **exakt gleich** einem normalisierten Listeneintrag. Teilstring-Treffer zählen nicht: „Hier klicken für Preise“ → kein Treffer.
  - [ ] Leere Anchors werden hier nicht gezählt (TECH-135).
- **Testfixture:** `links/nondescriptive.html` mit „Click here“, „Mehr…“, „Read More!“, „Hier klicken für Preise“.

### TECH-137 Filter „Pages With High External Outlinks“
- **Priorität:** P1
- **Beschreibung:** Seiten mit sehr vielen verfolgten externen Links.
- **Eingaben/Konfiguration:** Schwelle „High External Outlinks“ (Standard 100, Festlegung).
- **Ausgabe (Spalten/Filter/Export):** Filter „Pages With High External Outlinks“; Einstufung: Warnung / Niedrig.
- **Akzeptanzkriterien:**
  - [ ] Bedingung: Anzahl externer Outlinks mit `Follow = true` (nicht unique) `≥ Schwelle`.
- **Testfixture:** `links/many-external.html` mit 100 follow- und 20 nofollow-externen Links → gelistet. Der frühere Negativfall `/99-external.html` ist gestrichen: die Referenz listet `parity-m2-links` `external-99.html`; ob sie anders zählt oder eine niedrigere Schwelle nutzt, ist offen, die Schwelle bleibt 100 (K2-86, SQ-13a).

### TECH-138 Filter „Pages With High Internal Outlinks“
- **Priorität:** P1
- **Beschreibung:** Seiten mit sehr vielen verfolgten internen Links.
- **Eingaben/Konfiguration:** Schwelle „High Internal Outlinks“ (Standard 1000).
- **Ausgabe (Spalten/Filter/Export):** Filter „Pages With High Internal Outlinks“; Einstufung: Warnung / Niedrig.
- **Akzeptanzkriterien:**
  - [ ] Bedingung: Anzahl interner Outlinks mit `Follow = true` (nicht unique) `≥ Schwelle`.
- **Testfixture:** `links/many-internal.html` (1000 Links auf generierte Ziele) → gelistet; Schwelle 2000 → nicht.

### TECH-139 Filter „Follow & Nofollow Internal Inlinks To Page“
- **Priorität:** P1
- **Beschreibung:** Seiten, die intern sowohl verfolgte als auch nicht verfolgte Links erhalten.
- **Eingaben/Konfiguration:** Nachlauf.
- **Ausgabe (Spalten/Filter/Export):** Filter „Follow & Nofollow Internal Inlinks To Page“; Einstufung: Warnung / Niedrig.
- **Akzeptanzkriterien:**
  - [ ] Bedingung: mindestens ein interner Inlink mit `Follow = true` und mindestens einer mit `Follow = false`.
- **Testfixture:** `links/mixed-target.html` wird von A normal und von B mit nofollow verlinkt.

### TECH-140 Filter „Internal Nofollow Inlinks Only“
- **Priorität:** P1
- **Beschreibung:** Seiten, die intern ausschließlich über nofollow-Links erreicht werden.
- **Eingaben/Konfiguration:** Nachlauf; Seiten werden nur gecrawlt, wenn „Follow Internal Nofollow“ an ist oder sie anderweitig entdeckt werden (z. B. Sitemap).
- **Ausgabe (Spalten/Filter/Export):** Filter „Internal Nofollow Inlinks Only“; Einstufung: Warnung / Niedrig.
- **Akzeptanzkriterien:**
  - [ ] Bedingung: Anzahl interner Inlinks ≥ 1 und alle haben `Follow = false`.
- **Testfixture:** `links/nofollow-only-target.html` nur von einer Seite per nofollow verlinkt, zusätzlich in Sitemap (damit er gecrawlt wird).

### TECH-141 Filter „Outlinks To Localhost“
- **Priorität:** P1
- **Beschreibung:** Verweise auf Loopback-Adressen (der eigene Rechner des Besuchers). Für Besucher und Suchmaschinen sind solche Ziele nie erreichbar; sie stammen typischerweise aus Entwicklungs- oder Staging-Konfigurationen, die beim Veröffentlichen nicht umgestellt wurden.
- **Eingaben/Konfiguration:** –
- **Ausgabe (Spalten/Filter/Export):** Filter „Outlinks To Localhost“; Einstufung: Fehler / Hoch.
- **Akzeptanzkriterien:**
  - [ ] Bedingung: mindestens ein Outlink (Hyperlink oder Ressource) mit Host `localhost`, `*.localhost`, `127.0.0.0/8`, `::1` oder `0.0.0.0`.
  - [ ] Ausnahme: Ist der Host+Port der Start-URL selbst ein solcher Loopback-Host, werden Links auf exakt denselben Host+Port nicht gemeldet (damit lokale Crawls nicht jede Seite listen).
- **Testfixture:** `links/localhost.html` (Fixture-Server auf `127.0.0.1:<port>`) mit Link auf `http://localhost:9/dev` und Bild von `http://127.0.0.2/x.png`; Links auf `127.0.0.1:<port>` dürfen nicht zählen.

### TECH-142 Filter „Non-Indexable Page Inlinks Only“
- **Priorität:** P1
- **Beschreibung:** Indexierbare Seiten, die nur von nicht indexierbaren Seiten verlinkt werden (deren Links Suchmaschinen schlechter oder gar nicht sehen).
- **Eingaben/Konfiguration:** Nachlauf.
- **Ausgabe (Spalten/Filter/Export):** Filter „Non-Indexable Page Inlinks Only“; Einstufung: Warnung / Hoch. Bulk-Export „Links > Non-Indexable Page Inlinks Only“ (die Inlinks der gelisteten Seiten im Standard-Linkformat): OUT-068.
- **Akzeptanzkriterien:**
  - [ ] Bedingung: Seite ist `Indexable`, hat ≥ 1 internen Hyperlink-Inlink (ohne Self-Links), und jede Quellseite ist `Non-Indexable` (noindex, canonicalised oder robots-gesperrt – Letzteres nur, wenn Links aus gesperrten Seiten bekannt sind, z. B. bei „robots.txt ignorieren, aber melden“).
  - [ ] Die Startseite wird nie gelistet.
- **Testfixture:** `links/via-noindex.html` nur von `/noindex-hub.html` verlinkt; `/via-canonicalised.html` nur von einer canonicalisierten Seite verlinkt.

### TECH-143 Filter „Pages With Uncrawlable Internal Outlinks“
- **Priorität:** P1
- **Beschreibung:** Links, die nicht als `<a href>` umgesetzt sind und daher von Suchmaschinen nicht zuverlässig verfolgt werden.
- **Eingaben/Konfiguration:** Option „Uncrawlable Links speichern“ in der Crawl-Konfiguration (Standard aus; Filter bleibt leer, solange aus).
- **Ausgabe (Spalten/Filter/Export):** Filter „Pages With Uncrawlable Internal Outlinks“; Einstufung: Warnung / Hoch. In Outlinks-Ansicht Spalte `Link Crawlability` = `Crawlable` / `Uncrawlable`. Bulk-Export gleichen Namens.
- **Akzeptanzkriterien:**
  - [ ] Als uncrawlbar erkannt: (a) Nicht-`a`-Element mit `href`-Attribut (z. B. `<span href="/x">`, `<div href>`, `<button href>`); (b) `<a>` ohne `href`, dessen `onclick` eine Navigation mit String-Literal-URL enthält (`location`, `location.href`, `window.location.assign`, `window.open`); (c) `<a href="javascript:…">`, sofern im Ausdruck eine URL als String-Literal steht.
  - [ ] Nur wenn eine URL extrahiert werden kann und sie intern ist, zählt der Link für den Filter.
  - [ ] Uncrawlbare Links werden **nicht** in die Frontier aufgenommen.
  - [ ] `<a href="/x" onclick="track()">` ist crawlbar.
- **Testfixture:** `links/uncrawlable.html` mit je einem Beispiel (a)–(c), einem `javascript:void(0)` ohne URL (nicht zählen), einem Link auf externe Domain per onclick (nicht zählen).

### TECH-144 Link-Präferenzen (Schwellwerte, Anchor-Liste)
- **Priorität:** P0
- **Beschreibung:** Konfigurierbare Grenzen für die Links-Filter.
- **Eingaben/Konfiguration:** `high_crawl_depth` (Standard 4), `high_internal_outlinks` (Standard 1000), `high_external_outlinks` (Standard 100, Festlegung), `non_descriptive_anchors` (Liste, Standard siehe unten, Festlegung).
- **Ausgabe (Spalten/Filter/Export):** –
- **Akzeptanzkriterien:**
  - [ ] Standardliste (Festlegung, englisch + deutsch): `click here`, `click`, `here`, `this`, `this page`, `link`, `more`, `read more`, `learn more`, `more info`, `more information`, `find out more`, `details`, `continue`, `go`, `start`, `hier`, `hier klicken`, `klicken sie hier`, `mehr`, `weiter`, `weiterlesen`, `mehr erfahren`, `mehr lesen`, `mehr info`, `details ansehen`.
  - [ ] Änderung einer Schwelle wirkt nach Neuberechnung der Filter ohne erneuten Crawl.
  - [ ] Werte werden in der Crawl-Datei gespeichert, damit Exporte reproduzierbar sind.
- **Testfixture:** `links/` mit geänderter Konfiguration (Tiefe 6, Liste nur `foo`).

### TECH-145 „% of Total“ und Link Score als Spalten
- **Priorität:** P1
- **Beschreibung:** Zwei abgeleitete Kennzahlen im Links-Tab. **Führend für Link Score** (Definition, Berechnung im Kern, Spaltenwerte); ONPAGE-017 (Spalte im Internal-Tab) und UI-081 verweisen hierher, der Ablauf der Crawl-Analyse steht in CRAWL-141/UI-080. Link Score ist eine relative Gewichtung jeder URL im internen Linkgraphen auf einer Skala 0–100; tatsächlich vergeben werden Werte von 1 (schwächste teilnehmende URL) bis 100 (stärkste). Berechnet wird eine PageRank-Variante mit fester Iterationszahl; das Verfahren ist allgemein bekannt, die folgende formale Definition ist OpenSpiders eigene Festlegung. **OS-Festlegung zur Priorität:** P1 (bestätigt; ONPAGE-017 und UI-081 übernehmen sie). Der Ablauf der Crawl-Analyse (CRAWL-141, P0) funktioniert ohne Link Score; der Teilschritt ist im MVP abschaltbar und blockiert keine P0-Funktion.
- **Eingaben/Konfiguration:** Teilschritt „Link Score“ der Crawl-Analyse (TECH-005). Formale Definition:
  - **Knotenmenge V:** interne, gecrawlte HTML-URLs mit Antwort, die weder selbst weiterleiten noch auf eine andere URL kanonisiert sind; n = |V|.
  - **Auflösung `res(u)`:** von u aus Weiterleitungen (HTTP 3xx, Meta Refresh, JavaScript-Weiterleitung, interne HSTS-Umleitung) und Canonicals (Link-Element oder HTTP-Header) folgen, bis ein Knoten aus V erreicht ist; höchstens 10 Schritte; bei Schleife oder Abbruch undefiniert.
  - **Kanten:** Für jede Quelle s ∈ V ist `T(s)` die *Menge* der aufgelösten Hyperlink-Ziele (`<a href>`) von s ohne s selbst und ohne undefinierte Ziele (Mehrfachlinks zählen einmal). `F(s)` ⊆ `T(s)` sind die Ziele mit mindestens einem folgbaren Link (kein nofollow/ugc/sponsored, Quelle nicht per Meta Robots nofollow). Ausgangsgrad `out(s) = |T(s)|`: nofollow-Ziele zählen im Nenner, erhalten aber keinen Anteil.
  - **Iteration:** Dämpfung d = 0,85, genau k = 10 Durchläufe, Startwert 1/n je Knoten. Je Durchlauf erhält jeder Knoten den Grundwert (1 − d)/n; jede Quelle mit `out(s) > 0` gibt `d · r(s) / out(s)` an jedes Ziel in `F(s)` weiter. Knoten ohne Ausgangskanten geben nichts weiter; es wird nichts umverteilt. Rechnung in f64, Summation in fester Reihenfolge (Knoten-ID aufsteigend).
  - **Normierung:** min/max über die Rohwerte in V; Standard logarithmisch `round(1 + 99 · (ln r − ln min) / (ln max − ln min))`, alternativ linear `round(1 + 99 · (r − min) / (max − min))` als interne Option; kaufmännische Rundung. Welche Variante der Referenz entspricht, entscheidet die Kalibrierung (Anhang A Punkt 36). **Stand M2 (K2-88, SQ-10):** Formel und Normierung sind OS-Festlegung; Abweichungen von der Referenz laufen als DEV „TECH-145 OS-Festlegung, Kalibrierung M3“ (Spalte `Link Score` in Links und Internal im Paritätsvergleich ignoriert). Die Kalibrierung an den vier Referenzgraphen der Paritäts-Sites ist ein M3-Paket.
- **Ausgabe (Spalten/Filter/Export):** `Link Score` (ganzzahlig 1–100; leer für nicht teilnehmende URLs) in den Tabs Links und Internal sowie in den URL-Details; Skalierungsgröße für Visualisierungen (UI). `% of Total`.
- **Akzeptanzkriterien:**
  - [ ] `% of Total` = `Unique Inlinks / Anzahl interner HTML-200-Seiten × 100`, 2 Nachkommastellen (Festlegung; die Referenz nennt die Formel nicht).
  - [ ] `Link Score` ist leer, solange der Teilschritt aussteht; danach hat jede URL aus V einen ganzzahligen Wert 1–100 (0 wird nie vergeben); nicht teilnehmende URLs (Weiterleitungen, kanonisierte, externe, Nicht-HTML) bleiben leer.
  - [ ] Bei genau einer teilnehmenden URL ist ihr Wert 100; sind alle Rohwerte gleich, erhalten alle 100.
  - [ ] Fixture `fx-ui-linkscore` (A Start; A→B, A→R, A→D nofollow, A→A; R = 301 auf C; B→C zweimal, B→A; C→A, C→K; K kanonisiert auf D; D→E; E→A) liefert nach 10 Iterationen die Rohwerte A 0,21601; B 0,09190; C 0,13147; D 0,08661; E 0,10450 (± 1e-5) und normiert logarithmisch A 100, C 46, E 21, B 7, D 1 (linear: A 100, C 35, E 15, B 5, D 1).
  - [ ] Deterministisch: gleiche Eingabe → bitgleiche Rohwerte, unabhängig von der Thread-Anzahl.
  - [ ] Berechnung für 5 Mio. Knoten / 100 Mio. Kanten ≤ 60 s und ≤ 2 GB zusätzlicher Arbeitsspeicher auf Referenzhardware (Performance-Rahmen PERF).
- **Testfixture:** `links/` mit 10 Seiten; Seite im globalen Menü hat `% of Total = 90.00` (9 von 10 Seiten, ohne Self-Link). `fx-ui-linkscore` wie oben (Herleitung: V = {A, B, C, D, E}; R und K werden aufgelöst; T(A) = {B, C, D} mit D nur nofollow, T(B) = {A, C}, T(C) = {A, D}, T(D) = {E}, T(E) = {A}); zusätzlich Zufallsgraphen gegen eine unabhängige Python-Implementierung desselben Verfahrens (nicht den Standard-PageRank einer Bibliothek, der Knoten ohne Ausgangskanten umverteilt und bis zur Konvergenz iteriert).

### TECH-146 Aggregierte Ankertext-Ansicht („Anchors“) in Inlinks/Outlinks
- **Priorität:** P1
- **Beschreibung:** In den unteren Ansichten „Inlinks“ und „Outlinks“ lassen sich die Links statt einzeln auch **gruppiert nach Ankertext** anzeigen. So sieht man, mit welchen Formulierungen eine Seite (oder eine Auswahl von Seiten) intern verlinkt wird, ohne Hunderte Einzelzeilen durchzugehen. Ein Klick auf eine Gruppe zeigt die zugehörigen Einzellinks. Die Ankertext-Wortwolke UI-099 nutzt dieselbe Aggregation (übernommen aus UI-034).
- **Eingaben/Konfiguration:** Ansichtsumschalter `Alle Links` | `Anchors` in der unteren Inlinks-/Outlinks-Ansicht; Kontrollkästchen „Self-Links ausschließen“ (Standard an) und „Nofollow ausschließen“ (Standard aus) (Festlegung der Standardwerte); [P2] Kontrollkästchen „Groß-/Kleinschreibung zusammenfassen“ (Standard aus).
- **Ausgabe (Spalten/Filter/Export):** Spalten der Anchors-Ansicht: `Anchor Text`, `Links` (Anzahl Hyperlinks mit diesem Text), `Unique Sources` (bei Inlinks: verschiedene Quellseiten) bzw. `Unique Targets` (bei Outlinks: verschiedene Ziele), `% of Links`. Export der Ansicht (CSV/XLSX).
- **Akzeptanzkriterien:**
  - [ ] Gruppierungsschlüssel: Anchor Text nach TECH-131, bei Bild-Links ohne Text der Alt Text; Normalisierung nur Trim und Whitespace-Zusammenfassung, **case-sensitiv** („Mehr“ und „mehr“ sind zwei Gruppen, Festlegung). Links ohne Anchor und ohne Alt bilden die Gruppe `(leer)`.
  - [ ] Mehrfachauswahl im oberen Tab (z. B. 3 URLs) → Aggregation über die Inlinks aller gewählten URLs.
  - [ ] „Self-Links ausschließen“ an → Links von einer Seite auf sich selbst zählen nicht. „Nofollow ausschließen“ an → Links mit `Follow = false` zählen nicht.
  - [ ] Sortierung standardmäßig nach `Links` absteigend, bei Gleichstand nach `Anchor Text` aufsteigend (deterministisch).
  - [ ] `% of Links` = `Links / Summe Links aller Gruppen × 100`, 2 Nachkommastellen; die Summe aller Gruppen ergibt 100,00 (± 0,01 Rundung).
  - [ ] Klick auf eine Gruppe filtert die Einzel-Link-Liste auf genau diese Links.
  - [ ] Nur Hyperlinks; Ressourcen-Links (img, script, link) erscheinen nicht.
  - [ ] [P2] „Groß-/Kleinschreibung zusammenfassen“ an → Gruppierungsschlüssel ist der normalisierte Text nach Unicode-Case-Folding (vollständig, sprachunabhängig; „STRASSE“ und „Straße“ fallen zusammen); angezeigt wird die häufigste Schreibweise der Gruppe, bei Gleichstand die in Unicode-Codepunktfolge kleinste **(OS-Festlegung)**. Fixture: „Produkte“ und „produkte“ bilden eine Gruppe „Produkte“ mit `Links = 5`; `% of Links` und Sortierung werden über die zusammengefassten Gruppen berechnet; Klick auf die Gruppe zeigt die Einzellinks beider Schreibweisen. Ausgeschaltet gilt unverändert die case-sensitive Gruppierung oben; der Export enthält die Ansicht im jeweils aktiven Modus.
- **Testfixture:** `tests/fixtures/tech/links/anchors/`: Zielseite `/target` mit Inlinks „Produkte“ (3×, von 3 Seiten), „produkte“ (1×), „Mehr“ (2× von derselben Seite), einem Bild-Link mit Alt „Logo“, einem leeren Link, einem nofollow-Link „Produkte“ und einem Self-Link „Produkte“. Erwartung Standard: „Produkte“ `Links = 4`, `Unique Sources = 4` (inkl. nofollow, ohne Self-Link); mit „Nofollow ausschließen“ `Links = 3`.

### TECH-147 Filter „Pages With Empty Anchor Tags“ (Nachtrag 09.11.2026)
- **Priorität:** P1
- **Meilenstein:** 1.x (nach 1.0) – OS-Festlegung, Nutzerentscheidung 09.11.2026, ohne Paritätsbezug.
- **Beschreibung:** Seiten mit mindestens einem leeren Link-Element: ein `<a href>`, das weder Text noch ein Bild mit Alt-Text enthält. Solche Links (z. B. Symbol-Links, deren Symbol per CSS kommt) geben Suchmaschinen keinen Ankertext und sind ohne zusätzliche Auszeichnung für Screenreader namenlos.
- **Abgrenzung zu TECH-135:** TECH-135 bewertet die gespeicherten **internen Outlinks** (Link-Datensätze), schließt `<area>` ein und richtet sich nach den Crawl-Einstellungen (nur erfasste Links). Dieser Filter bewertet die **Elemente** im HTML der Seite: alle `<a>` mit `href`, intern und extern, auch nofollow und nicht verfolgte Links (z. B. ausgeschlossene Ziele oder Positionsregeln nach INTEG-085), aber kein `<area>`. Eine Seite, die wegen eines leeren `<a>` in TECH-135 steht, steht auch hier; umgekehrt kommen hier Seiten hinzu, deren leere Links nur extern oder nicht erfasst sind. Die Barrierefreiheitsregel `link-name` (TECH-253) bleibt eigenständig: Sie berücksichtigt `aria-label`, `aria-labelledby` und `title`, dieser Filter nicht.
- **Eingaben/Konfiguration:** –
- **Ausgabe (Spalten/Filter/Export):** Filter „Pages With Empty Anchor Tags“ im Tab Links; Spalte `Empty Anchor Tags` (Anzahl der leeren `<a>`-Elemente der Seite); in den unteren Outlinks die Filteroption „Leeres Link-Element“. Einstufung: Chance / Niedrig (wie TECH-135; UI-063: `T-O2`, `P-L1`).
- **Akzeptanzkriterien:**
  - [ ] Leer ist ein `<a href>`, wenn sein Ankertext nach TECH-131 nach Trim leer ist **und** kein enthaltenes `<img>` einen nicht leeren `alt` hat.
  - [ ] `<a href="/x"></a>`, `<a href="/x">   </a>`, `<a href="/x"><img src="i.png" alt=""></a>`, `<a href="/x"><img src="i.png"></a>`, `<a href="/x"><span class="icon"></span></a>` → leer.
  - [ ] `<a href="/x">Mehr</a>`, `<a href="/x"><img alt="Logo"></a>` → nicht leer.
  - [ ] `<a href="/x" aria-label="Warenkorb"></a>` und `<a href="/x" title="Warenkorb"></a>` → leer (Festlegung: Ankertext für Suchmaschinen fehlt; die Barrierefreiheit prüft `link-name`).
  - [ ] Externer Link und `rel="nofollow"`-Link ohne Text → zählen. `<a>` ohne `href` (Sprungmarke) und `<area>` → zählen nicht.
  - [ ] Im Rendering-Modus wird das gerenderte HTML bewertet (Linkherkunft nach CRAWL-079); ein Link, der erst per JavaScript Text erhält, ist nicht leer.
  - [ ] `Empty Anchor Tags` zählt Elemente, nicht Ziele: zwei leere Links auf dasselbe Ziel → 2.
- **Testfixture:** `tests/fixtures/tech/links/empty-anchors.html` mit allen Fällen oben; `empty-anchors-js.html` (Text per Skript ergänzt).

---

## AMP

### TECH-160 AMP-Erkennung und Crawl-Optionen
- **Priorität:** P2
- **Beschreibung:** AMP-Varianten werden über `<link rel="amphtml" href>` auf der regulären Seite gefunden. Eine Seite gilt als AMP-Dokument, wenn das `<html>`-Element das Attribut `amp` oder `⚡` trägt.
- **Eingaben/Konfiguration:** „Store AMP“ (Standard aus), „Crawl AMP“ (Standard aus).
- **Ausgabe (Spalten/Filter/Export):** AMP-URLs mit Inlink-Typ „AMP“; Zuordnung reguläre Seite ↔ AMP-Seite.
- **Akzeptanzkriterien:**
  - [ ] Store aus → `amphtml` wird ignoriert, Tab leer.
  - [ ] Store an, Crawl aus → AMP-URLs werden gespeichert, aber nicht abgerufen.
  - [ ] AMP-Dokumente werden auch erkannt, wenn sie über normale Links gefunden wurden.
- **Testfixture:** `tests/fixtures/tech/amp/`: `/article` mit `amphtml` → `/article/amp`; gültiges AMP-Minimaldokument.

### TECH-161 Tab „AMP“: Population und Spalten
- **Priorität:** P2
- **Beschreibung:** Übersicht der AMP-Dokumente.
- **Eingaben/Konfiguration:** –
- **Ausgabe (Spalten/Filter/Export):** `Address`, `Occurrences`, `Indexability`, `Indexability Status`, `Title 1`, `Title 1 Length`, `Title 1 Pixel Width`, `H1-1`, `H1-1 Length`, `Size`, `Word Count`, `Text Ratio`, `Crawl Depth`, `Response Time`. Filter TECH-162 und TECH-163 plus „All“.
- **Akzeptanzkriterien:**
  - [ ] Population: alle als AMP erkannten oder per `amphtml` referenzierten internen URLs.
  - [ ] Längen/Pixelbreite nach den Regeln der On-Page-Spec (Titel).
- **Testfixture:** `amp/`.

### TECH-162 AMP-SEO-Filter (Non-200, Return Link, Canonical, Indexierbarkeit)
- **Priorität:** P2
- **Beschreibung:** Sechs Filter zur Einbindung der AMP-Seite in die reguläre Site.
- **Eingaben/Konfiguration:** „Missing Non-AMP Return Link“ und „Non-Indexable Canonical“ sind Nachlauf-Filter (Bereich „AMP“ der Crawl-Analyse); die übrigen vier sind live.
- **Ausgabe (Spalten/Filter/Export):** Filter mit Einstufung: „Non-200 Response“ (Fehler/Hoch), „Missing Non-AMP Return Link“ (Fehler/Hoch), „Missing Canonical To Non-AMP“ (Fehler/Hoch), „Non-Indexable Canonical“ (Fehler/Hoch), „Indexable“ (Warnung/Hoch), „Non-Indexable“ (Information).
- **Akzeptanzkriterien:**
  - [ ] Non-200 Response: AMP-URL antwortet nicht mit 200 (robots-gesperrt, keine Antwort, 3xx, 4xx, 5xx).
  - [ ] Missing Non-AMP Return Link: Canonical der AMP-Seite zeigt auf Seite N (≠ AMP-Seite), N wurde gecrawlt, aber N hat kein `amphtml` auf die AMP-URL.
  - [ ] Missing Canonical To Non-AMP: Canonical der AMP-Seite zeigt auf ein anderes AMP-Dokument.
  - [ ] Non-Indexable Canonical: Canonical-Ziel der AMP-Seite ist non-indexable (TECH-001).
  - [ ] Indexable: AMP-Seite ist selbst indexierbar (gilt als Warnung, weil sie normalerweise auf die reguläre Seite canonicalisiert sein soll; eine eigenständige AMP-Seite mit Self-Canonical ist hier ebenfalls gelistet).
  - [ ] Non-Indexable: AMP-Seite ist non-indexable (Normalfall bei korrekter Einbindung).
- **Testfixture:** `amp/` mit Varianten: AMP 404; AMP mit Canonical auf N ohne `amphtml`; AMP mit Canonical auf andere AMP; AMP mit Canonical auf noindex-Seite; eigenständige AMP-Seite.

### TECH-163 AMP-Validierungsfilter
- **Priorität:** P2
- **Beschreibung:** Prüfung der AMP-Pflichtbestandteile durch eigene, deterministische Regeln (unten). Zusätzlich läuft der offizielle AMP-Validator (Apache-2.0) in der gebündelten JS-Engine bzw. Chromium. Er liefert die Meldungen für „Other Validation Errors“ und die Detailansicht.
- **Eingaben/Konfiguration:** „AMP validieren“ (Standard an, wenn Store AMP an). Die Validator-Version ist im Repository fixiert (Datei `assets/amp/validator.js` mit Versionsangabe, Aktualisierung nur per Release). Die Version wird im Crawl gespeichert.
- **Ausgabe (Spalten/Filter/Export):** Filter (alle Fehler/Hoch): „Missing \<html amp> Tag“, „Missing/Invalid Doctype HTML Tag“, „Missing Head Tag“, „Missing Body Tag“, „Missing Canonical“, „Missing/Invalid Meta Charset Tag“, „Missing/Invalid Meta Viewport Tag“, „Missing/Invalid AMP Script“, „Missing/Invalid AMP Boilerplate“, „Contains Disallowed HTML“, „Other Validation Errors“. Untere Ansicht „AMP Validation Details“ mit `Error` (Validator-Code), `Message` (eigener Text), `Line`, `Column`, `Spec URL`, `Filter` (welchem der 11 Filter die Meldung zugeordnet ist).
- **Akzeptanzkriterien:**
  - [ ] Geprüft werden nur Dokumente, die als AMP erkannt sind (TECH-160: `<html amp>`/`<html ⚡>` **oder** über `amphtml` referenziert). Eine per `amphtml` referenzierte Seite ohne AMP-Attribut landet in „Missing \<html amp> Tag“.
  - [ ] Missing \<html amp>: oberstes `<html>`-Start-Tag ohne Attribut `amp` oder `⚡` (Attributwert egal, `amp=""` ist gültig).
  - [ ] Doctype: Das Dokument beginnt nach optionalem BOM, Whitespace und Kommentaren nicht mit `<!doctype html>` (case-insensitiv). `<!DOCTYPE html PUBLIC …>` → Treffer.
  - [ ] Missing Head/Body: Quelltext ohne `<head>`- bzw. `<body>`-Start-Tag (Token-Ebene, TECH-280). Läuft unabhängig von TECH-008.
  - [ ] Missing Canonical: kein `<link rel=canonical>` mit nicht-leerem `href` im Head. Ein Self-Canonical ist gültig; ein Canonical nur als HTTP-Header zählt nicht (Festlegung).
  - [ ] Meta Charset: fehlt `<meta charset="utf-8">` (case-insensitiv) im Head, oder ist es nicht das erste Kind-Element des Heads → Treffer. `<meta charset="iso-8859-1">` → Treffer.
  - [ ] Meta Viewport: Pass nur, wenn im Head ein `<meta name="viewport">` steht, dessen `content` (Komma- oder Semikolon-getrennt, Leerzeichen ignoriert, Schlüssel case-insensitiv) **beide** Paare `width=device-width` **und** `minimum-scale=1` enthält (`minimum-scale=1.0` gilt als gleich). `initial-scale=1` ist empfohlen, aber nicht Pflicht und hat keinen Einfluss. Beispiele: `width=device-width,minimum-scale=1,initial-scale=1` → Pass; `width=device-width,minimum-scale=1` → Pass; `width=device-width,initial-scale=1` → Treffer; `minimum-scale=1` → Treffer; kein Viewport-Meta → Treffer.
  - [ ] AMP Script: kein `<script async src="https://cdn.ampproject.org/v0.js">` im Head (Attribut `async` Pflicht). Zusätzliche Modul-Varianten der Laufzeit sind erlaubt; ob sie korrekt kombiniert sind, entscheidet der Validator und meldet es ggf. unter „Other“.
  - [ ] Boilerplate: im Head fehlt `<style amp-boilerplate>` **oder** `<noscript><style amp-boilerplate>` → Treffer.
  - [ ] Disallowed HTML: mindestens eines von `<script>`, das weder `type="application/ld+json"` hat noch als `application/json`-Konfiguration innerhalb einer AMP-Komponente steht noch eine AMP-Laufzeit-/Komponentenquelle von `cdn.ampproject.org` lädt; `<form>` ohne eingebundene `amp-form`-Komponente; `<iframe>`, `<frame>`, `<frameset>`, `<object>`, `<embed>`, `<img>` (statt `amp-img`), `<video>`/`<audio>` (statt AMP-Komponente); Attribut `style` mit `!important`; Event-Handler-Attribute `on*` außer `on` (AMP-Aktionssyntax).
  - [ ] Other: jede weitere Validator-Meldung der Schwere „Error“, die keiner der oberen Regeln zugeordnet ist. Validator-„Warnings“ werden nur in der Detailansicht gezeigt, lösen keinen Filter aus.
  - [ ] Eine Seite kann in mehreren Validierungsfiltern stehen. Eine durch eigene Regeln erkannte Meldung erscheint nicht zusätzlich unter „Other“ (Zuordnung über Validator-Codes, Mapping-Tabelle `assets/amp/code-map.toml`).
- **Testfixture:** `amp/invalid-*.html`, je eine Datei pro Filter (u. a. `invalid-viewport-no-minscale.html` mit `width=device-width,initial-scale=1`, `invalid-viewport-only-minscale.html`, `invalid-charset-second.html` mit charset an zweiter Stelle, `invalid-doctype-legacy.html`, `invalid-img.html`, `invalid-important.html`), plus `amp/valid.html` (darf in keinem Validierungsfilter erscheinen) und `amp/valid-viewport-variants.html` (`minimum-scale=1.0`, Semikolon-Trenner → Pass).

---

## Structured Data

### TECH-180 Extraktion JSON-LD
- **Priorität:** P1
- **Beschreibung:** Liest alle `<script type="application/ld+json">`-Blöcke und wandelt sie in Items (Typ + Eigenschaften, verschachtelt) um.
- **Eingaben/Konfiguration:** „JSON-LD“ (Standard aus; Extraktion strukturierter Daten ist opt-in).
- **Ausgabe (Spalten/Filter/Export):** Items mit Format `JSON-LD`, Typen (vollständige IRI und Kurzname), Eigenschaftsbaum, Quellposition (Block-Nr., Zeile).
- **Akzeptanzkriterien:**
  - [ ] Unterstützt: einzelnes Objekt, Array von Objekten, `@graph`, verschachtelte Objekte, `@type` als String oder Array, `@id`-Referenzen innerhalb desselben Dokuments (werden für die Validierung aufgelöst).
  - [ ] `@context` als `https://schema.org`, `http://schema.org`, `https://schema.org/` und als Objekt mit `@vocab` wird erkannt; Kurzname = Teil nach dem Vokabular-Präfix.
  - [ ] `type`-Attribut case-insensitiv; Blöcke im Body zählen; HTML-Kommentare `<!-- -->` um den JSON-Inhalt werden toleriert (Festlegung).
  - [ ] Ungültiges JSON (z. B. abschließendes Komma, unmaskierte Zeilenumbrüche in Strings) → Parse-Fehler (TECH-186), übrige Blöcke werden weiter verarbeitet.
- **Testfixture:** `tests/fixtures/tech/sd/jsonld-*.html`: Product mit Offer, `@graph` mit Organization+WebSite, Array mit zwei Items, `@id`-Referenz, kaputtes JSON.

### TECH-181 Extraktion Microdata
- **Priorität:** P1
- **Beschreibung:** Liest Items aus `itemscope`/`itemtype`/`itemprop`/`itemref`/`itemid` nach dem HTML-Microdata-Modell.
- **Eingaben/Konfiguration:** „Microdata“ (Standard aus).
- **Ausgabe (Spalten/Filter/Export):** Items mit Format `Microdata`.
- **Akzeptanzkriterien:**
  - [ ] Wertermittlung je Element wie im Microdata-Standard: `content`-Attribut von `meta`; `src` bei `img`, `audio`, `video`, `source`, `iframe`, `embed`; `href` bei `a`, `area`, `link`; `data` bei `object`; `value` bei `data`/`meter`; `datetime` bei `time` (sonst Text); ansonsten Textinhalt.
  - [ ] Verschachtelte Items (Element mit `itemprop` **und** `itemscope`) werden als Unter-Items geführt; Top-Level-Items sind `itemscope` ohne `itemprop`.
  - [ ] `itemref` wird über IDs aufgelöst; Zyklen werden erkannt und abgebrochen.
  - [ ] Mehrere Typen in `itemtype` (Leerzeichen-getrennt) werden alle gespeichert.
- **Testfixture:** `sd/microdata-*.html`: Recipe mit verschachteltem NutritionInformation, `itemref` auf Element außerhalb, `meta`-basierte Werte, `time datetime`.

### TECH-182 Extraktion RDFa
- **Priorität:** P1
- **Beschreibung:** Liest RDFa-Lite/RDFa-1.1-Annotationen (`vocab`, `typeof`, `property`, `resource`, `prefix`, `about`, `content`, `href`, `src`).
- **Eingaben/Konfiguration:** „RDFa“ (Standard aus).
- **Ausgabe (Spalten/Filter/Export):** Items mit Format `RDFa`.
- **Akzeptanzkriterien:**
  - [ ] `vocab="https://schema.org/"` + `typeof="Product"` → Typ `Product`.
  - [ ] Präfixe (`prefix="schema: https://schema.org/"`, `typeof="schema:Product"`) werden aufgelöst.
  - [ ] Verschachtelung über `property` + `typeof` am selben Element.
  - [ ] Nur schema.org-Vokabular wird typisiert; andere Vokabulare (z. B. Open Graph `og:`) werden ignoriert (Festlegung).
  - **Nachtrag 09.11.2026 (1.x):** Das gilt weiterhin für den Tab Structured Data. Open-Graph- und Card-Angaben werden seitdem im eigenen Tab „Social“ ausgewertet (TECH-320 ff.); „ignoriert“ heißt hier nur noch: nicht als strukturierte Daten typisiert.
- **Testfixture:** `sd/rdfa-*.html`: Organization mit vocab, Product mit Präfix, Event verschachtelt mit Place.

### TECH-183 Tab „Structured Data“: Population und Spalten
- **Priorität:** P1
- **Beschreibung:** Übersicht der gefundenen Typen und Probleme je URL.
- **Eingaben/Konfiguration:** –
- **Ausgabe (Spalten/Filter/Export):** `Address`, `Errors`, `Warnings`, `Rich Result Errors`, `Rich Result Warnings`, `Rich Result Features`, `Total Types`, `Unique Types`, `Type 1..n`. Filter TECH-184 bis TECH-188 plus „All“.
- **Akzeptanzkriterien:**
  - [ ] Population: interne HTML-200-Seiten, sofern mindestens ein Format aktiviert ist; sonst Tab leer mit Hinweis.
  - [ ] `Total Types` = Anzahl Top-Level-Items-Typen inkl. Wiederholungen (zwei Product-Items = 2); `Unique Types` = Anzahl verschiedener Kurznamen über alle Top-Level-Items (Festlegung: verschachtelte Items zählen nicht).
  - [ ] `Type n` = Kurznamen der Top-Level-Typen in Reihenfolge des Auftretens, ohne Duplikate.
  - [ ] `Errors`/`Warnings` = Anzahl Schema.org-Befunde; `Rich Result Errors`/`Rich Result Warnings` = Anzahl Google-Befunde; `Rich Result Features` = kommagetrennte Liste erkannter Rich-Result-Features (z. B. `Product snippet, Review snippet`).
- **Testfixture:** `sd/` gesamt.

### TECH-184 Filter „Contains Structured Data“ / „Missing Structured Data“
- **Priorität:** P1
- **Beschreibung:** Vorhandensein strukturierter Daten.
- **Eingaben/Konfiguration:** –
- **Ausgabe (Spalten/Filter/Export):** „Contains Structured Data“ (Information), „Missing Structured Data“ (Chance / Niedrig).
- **Akzeptanzkriterien:**
  - [ ] Contains: mindestens ein Item in einem aktivierten Format **oder** ein Parse-Fehler (Festlegung: kaputtes JSON-LD zählt als „enthält“).
  - [ ] Missing: kein Item und kein Parse-Fehler in aktivierten Formaten.
  - [ ] Nur Microdata auf der Seite, aber nur JSON-LD aktiviert → Missing.
  - [ ] Bei „Ignore Non-Indexable URLs for Issues“ an (TECH-009) enthält „Missing Structured Data“ nur indexierbare Seiten; „Contains“ ist nicht betroffen.
- **Testfixture:** `sd/none.html`, `sd/microdata-recipe.html` mit Konfiguration nur JSON-LD.

### TECH-185 Filter „JSON-LD URLs“, „Microdata URLs“, „RDFa URLs“
- **Priorität:** P1
- **Beschreibung:** Drei Filter nach Format.
- **Eingaben/Konfiguration:** –
- **Ausgabe (Spalten/Filter/Export):** Filter wie benannt (Information).
- **Akzeptanzkriterien:**
  - [ ] Bedingung jeweils: mindestens ein Item dieses Formats. Eine Seite mit JSON-LD und Microdata steht in beiden.
- **Testfixture:** `sd/mixed.html` (JSON-LD + Microdata).

### TECH-186 Filter „Parse Errors“
- **Priorität:** P1
- **Beschreibung:** Strukturierte Daten, die nicht gelesen werden können.
- **Eingaben/Konfiguration:** –
- **Ausgabe (Spalten/Filter/Export):** Filter „Parse Errors“; Einstufung: Fehler / Hoch. Detailansicht zeigt Fehlermeldung mit Zeile/Spalte im JSON-Block.
- **Akzeptanzkriterien:**
  - [ ] JSON-LD: JSON-Syntaxfehler; JSON gültig, aber Wurzel weder Objekt noch Array; `@type` mit Nicht-String-Wert.
  - [ ] Microdata/RDFa: `itemtype` bzw. `typeof` mit ungültiger (nicht absoluter, nicht auflösbarer) IRI.
  - [ ] Leerer JSON-LD-Block (nur Whitespace) → Parse-Fehler (Festlegung).
  - [ ] Bei „Ignore Non-Indexable URLs for Issues“ an (TECH-009) nur indexierbare Seiten.
- **Testfixture:** `sd/jsonld-broken.html` (abschließendes Komma), `/jsonld-empty.html`, `/microdata-bad-itemtype.html`.

### TECH-187 Schema.org-Validierung und Filter „Validation Errors“/„Validation Warnings“
- **Priorität:** P1
- **Beschreibung:** Prüft Typen und Eigenschaften gegen das schema.org-Vokabular (mitgelieferte, versionierte Vokabular-Datei).
- **Eingaben/Konfiguration:** „Schema.org Validation“ (Standard an, sobald ein Format aktiv), „Case-Sensitive“ (Standard aus), Vokabular-Version (Datei im Repo, Aktualisierung per Release).
- **Ausgabe (Spalten/Filter/Export):** Filter „Validation Errors“ (Fehler / Hoch), „Validation Warnings“ (Chance / Niedrig). Spalten `Errors`, `Warnings` (TECH-183).
- **Akzeptanzkriterien:**
  - [ ] Geprüft wird jedes Item, auch verschachtelte, nach Auflösung von `@id`-Referenzen (TECH-180). Jede Eigenschaft wird einzeln geprüft; ein Array-Wert mit 3 Elementen erzeugt bis zu 3 Befunde.
  - [ ] Befundarten (Fehler), je mit stabiler Befund-ID (TECH-189):
    - `sd.schema.unknown_type`: Typname nicht im Vokabular (`@type: "Prodcut"`).
    - `sd.schema.unknown_property`: Eigenschaftsname nicht im Vokabular (`pricee`).
    - `sd.schema.property_not_allowed`: Eigenschaft existiert, aber nicht für diesen Typ oder seine Obertypen (Domain-Prüfung inkl. Vererbung; `recipeIngredient` an `Product`).
    - `sd.schema.value_type`: Objektwert eines Typs, der in keiner erlaubten Range liegt (`author: {"@type":"Product"}` an `Article`).
    - `sd.schema.value_format`: Literalwert verletzt die Syntax eines Datentyps, wenn die Range **nur** Nicht-Text-Datentypen enthält. Number: `"abc"`. Date: nicht ISO-8601-parsebar. Boolean: weder `true`/`false` noch `https://schema.org/True|False`. Wenn die Range auch `Text` oder `URL` zulässt, ist jeder String gültig.
  - [ ] Für Eigenschaften, die nicht zum schema.org-Vokabular gehören (`@context` mit eigenem Präfix, z. B. `ex:foo`), entsteht kein Befund (Festlegung).
  - [ ] Warnung (einzige Schema.org-Warnung): Items im veralteten Vokabular `data-vocabulary.org` (`sd.schema.deprecated_vocabulary`, ein Befund je Item).
  - [ ] Case-Sensitive aus (Standard): `product` → als `Product` akzeptiert, kein Befund; `Name` als Eigenschaft → als `name` akzeptiert. Case-Sensitive an: `product` → `sd.schema.unknown_type`, `Name` → `sd.schema.unknown_property`.
  - [ ] Abgekündigte (superseded) Typen/Eigenschaften gelten als gültig (Festlegung); Typen/Eigenschaften aus `pending` gelten als gültig (Festlegung).
  - [ ] Zählung: `Errors` = Anzahl der Fehler-Befunde der Seite (nicht der betroffenen Items), `Warnings` analog. Filter-Bedingung: `Errors ≥ 1` bzw. `Warnings ≥ 1`.
  - [ ] Bei „Ignore Non-Indexable URLs for Issues“ an (TECH-009) enthalten beide Filter nur indexierbare Seiten; die Zählspalten bleiben gefüllt.
  - [ ] Lizenzhinweis: Die schema.org-Vokabulardatei wird als separates Datenasset mit Quellenangabe ausgeliefert (Anhang C).
- **Testfixture:** `sd/schema-errors.html` mit genau: 1× unbekannter Typ, 1× unbekannte Eigenschaft, 1× falsche Domain, 1× falscher Objekttyp, 1× `price: "abc"` an `Offer` (Range Number **oder** Text → **kein** Befund), 1× `dateCreated: "gestern"` (Range Date/DateTime → Befund) → Golden `Errors = 5`. `sd/case.html` (`product`, `Name`) in zwei Läufen (Case-Sensitive aus: 0, an: 2). `sd/data-vocabulary.html` → `Warnings = 1`.

### TECH-188 Google-Rich-Result-Validierung und Filter „Rich Result Validation Errors“/„… Warnings“
- **Priorität:** P1
- **Beschreibung:** Prüft Items gegen die Anforderungen der von Google dokumentierten Rich-Result-Features. Die Regeln werden als eigene, versionierte Datei im Repository gepflegt (in eigenen Worten aus Googles öffentlicher Dokumentation abgeleitet).
- **Eingaben/Konfiguration:** „Google Rich Result Feature Validation“ (Standard an, sobald ein Format aktiv).
- **Ausgabe (Spalten/Filter/Export):** Filter „Rich Result Validation Errors“ (Fehler / Hoch), „Rich Result Validation Warnings“ (Chance / Niedrig); Spalte `Rich Result Features`.
- **Akzeptanzkriterien:**
  - [ ] Ein Item löst ein Feature aus, wenn sein Typ (oder ein Untertyp) zu einem Feature der Regeldatei gehört (z. B. `Product` → „Product snippet“ und, mit `offers`, „Merchant listing“; `Recipe` → „Recipe“; `BreadcrumbList` → „Breadcrumb“). Nur Top-Level-Items und die von Google als eigenständig vorgesehenen verschachtelten Items (z. B. `Review` in `Product`) lösen Features aus.
  - [ ] Fehler (`sd.google.missing_required`, `sd.google.invalid_value`): Pflichteigenschaft fehlt; Pflicht- **oder** empfohlene Eigenschaft vorhanden, aber ungültig (falscher Wertetyp, ungültiges Datum, ungültige URL, Wert außerhalb erlaubter Aufzählung).
  - [ ] Warnung (`sd.google.missing_recommended`): empfohlene Eigenschaft fehlt.
  - [ ] Zählung: `Rich Result Errors`/`Rich Result Warnings` = Anzahl der Befunde je Seite; dieselbe fehlende Eigenschaft wird je Feature einmal gezählt (Product ohne `name` → 2 Fehler, wenn „Product snippet“ und „Merchant listing“ ausgelöst sind; Festlegung).
  - [ ] Beispiele: `Product` ohne `name` → Fehler; `Product` mit `name`, ohne `review`/`aggregateRating`/`offers` → 1 Fehler (mindestens eine der drei erforderlich, als ein Befund); `Offer` ohne `priceCurrency` → Fehler; `Product` ohne `brand` → Warnung (wenn in der Regeldatei als empfohlen geführt).
  - [ ] Article-Eigenschaften, die Google nur für AMP-Seiten verlangt, werden auf Nicht-AMP-Seiten nicht als Fehler gemeldet.
  - [ ] Abgrenzung der geprüften Features: Die Regeldatei enthält nur Features, die Google zum Stand der Regeldatei-Version unterstützt. Abgleich mit dem Referenzstand:
    - **FAQ:** wird nicht validiert und erscheint nicht in `Rich Result Features`. Das stimmt mit der Referenz überein, die die FAQ-Validierung in einer jüngeren Version entfernt hat.
    - **Sitelinks Searchbox** und andere von Google eingestellte Features (z. B. HowTo): werden nicht validiert. **(Paritätsabweichung):** Die Referenz-Doku führt die Sitelinks Searchbox weiterhin als unterstütztes Feature. Golden-Vergleiche für Seiten mit `WebSite` + `potentialAction`/`SearchAction` schließen `Rich Result Errors`, `Rich Result Warnings` und `Rich Result Features` aus. Die Liste der betroffenen Features steht in `assets/sd/parity-exclusions.toml` und wird bei jedem Abgleich mit einer neuen Referenzversion aktualisiert.
  - [ ] Die Regeldatei enthält eine Versionsangabe; die aktuelle Version wird im Crawl gespeichert und im Export-Kopf ausgegeben.
  - [ ] Bei „Ignore Non-Indexable URLs for Issues“ an (TECH-009) enthalten beide Filter nur indexierbare Seiten.
- **Testfixture:** `sd/rich-*.html`: Product komplett (0 Befunde), Product ohne Offer-Währung (1 Fehler), Recipe ohne `image` (1 Fehler), Event mit ungültigem `startDate` `"2026-13-40"` (1 Fehler), BreadcrumbList ohne `position` am `ListItem` (1 Fehler je ListItem), Product ohne `brand` (1 Warnung), FAQPage (0 Befunde, kein Feature), WebSite mit SearchAction (0 Befunde, kein Feature; im Paritätstest ausgeschlossen).

### TECH-189 Detailansicht „Structured Data Details“
- **Priorität:** P1
- **Beschreibung:** Untere Detailansicht pro URL mit Item-Baum und Befunden.
- **Eingaben/Konfiguration:** –
- **Ausgabe (Spalten/Filter/Export):** Linke Seite: Baum (Format → Item → Eigenschaft → Wert), Symbole für Fehler/Warnung. Rechte Seite je Befund: `Validation Type` (`Schema.org` oder `Google: <Feature>`), `Severity` (`Error`/`Warning`), `Issue` (eigene Meldungstexte), `Property` (Pfad ab Top-Level-Item, z. B. `offers[0].priceCurrency`), `Befund-ID`, Link auf die schema.org-Seite der Eigenschaft bzw. die Google-Doku. Export der Ansicht als CSV/XLSX und JSON.
- **Akzeptanzkriterien:**
  - [ ] Jeder in TECH-187/188 gezählte Befund erscheint genau einmal; die Anzahl der Zeilen mit `Severity = Error` und `Validation Type = Schema.org` ist gleich `Errors` der Seite, analog für die anderen drei Zählspalten.
  - [ ] Baumreihenfolge: Formate in der Reihenfolge JSON-LD, Microdata, RDFa; Items innerhalb eines Formats in Quelltextreihenfolge; Eigenschaften in Quelltextreihenfolge (JSON: Schlüsselreihenfolge des Dokuments).
  - [ ] Klick auf einen Befund markiert den zugehörigen Knoten im Baum; Klick auf einen Knoten filtert die Befundliste auf diesen Knoten und seine Kinder.
  - [ ] Ein Knoten mit Befund zeigt ein Fehler- bzw. Warnsymbol; ein Elternknoten zeigt das schwerste Symbol seiner Kinder.
  - [ ] Parse-Fehler (TECH-186) erscheinen als eigener Knoten „Parse Error“ mit Zeile/Spalte (1-basiert, bezogen auf den Inhalt des Skriptblocks) und einem Ausschnitt von ± 40 Zeichen um die Fehlerstelle.
  - [ ] Meldungstexte sind eigene Formulierungen (Clean-Room), mit stabiler Befund-ID (`sd.schema.unknown_type`, `sd.google.missing_required` …); Platzhalter für Typ/Eigenschaft werden ersetzt (z. B. „Eigenschaft `priceCurrency` fehlt (Pflicht für Merchant listing)“).
  - [ ] Ohne aktivierte Formate ist die Ansicht leer mit Hinweis auf die Extraktionseinstellungen.
- **Testfixture:** `sd/rich-product-missing-currency.html` (Golden-Detail-JSON mit genau 1 Befund `sd.google.missing_required`, `Property = offers[0].priceCurrency`); `sd/schema-errors.html` (Golden: 5 Schema.org-Zeilen, Summe = `Errors`); `sd/jsonld-broken.html` (Parse-Error-Knoten mit Zeile/Spalte).

### TECH-190 Reports „Validation Errors & Warnings“ (+ Summary)
- **Priorität:** P1
- **Verweis:** siehe OUT-101 (führend). Für die Summary ist OUT-100 führend, hier gilt nur die Zeilenauswahl: eine Zeile je Befund aus TECH-187/TECH-188 mit seiner stabilen Befund-ID (TECH-189).

---

## Sitemaps

### TECH-210 Sitemap-Einbindung in den Crawl
- **Priorität:** P0
- **Beschreibung:** XML-Sitemaps werden entdeckt, abgerufen und geparst; ihre URLs fließen in die Frontier und die Sitemap-Filter ein. (Download-/Parse-Details und Listenmodus liegen in der Crawl-Spec; hier die für den Tab nötigen Regeln.)
- **Eingaben/Konfiguration:** „Crawl Linked XML Sitemaps“ (Standard aus), „Auto Discover XML Sitemaps via robots.txt“ (Standard an, wenn Ersteres an), manuelle Sitemap-URL-Liste.
- **Ausgabe (Spalten/Filter/Export):** Je Sitemap-Datei: URL, Typ (`urlset`/`sitemapindex`), Anzahl Einträge, unkomprimierte Größe in Bytes, Status. Je Seiten-URL: Liste der Sitemaps, in denen sie steht.
- **Akzeptanzkriterien:**
  - [ ] Sitemap-Index-Dateien werden rekursiv aufgelöst (Tiefe max. 3, Festlegung; CRAWL-060 angeglichen, K-27); Zyklen werden erkannt; tiefere Indizes ergeben eine Warnung.
  - [ ] gzip-komprimierte Sitemaps (`.xml.gz` oder `Content-Encoding`/`Content-Type` gzip) werden entpackt; die Größenprüfung nutzt die entpackte Größe.
  - [ ] `<loc>`-Werte werden getrimmt und entity-dekodiert; ungültige URLs werden gezählt, aber nicht übernommen.
  - [ ] Sitemaps werden auch ohne „Crawl Linked XML Sitemaps“ verarbeitet, wenn sie als Start-/Listen-URL angegeben sind.
  - [ ] **OS-Festlegungen zum Sitemap-Ingest (13.10.2026, Welle-2-Sichtung, K-41):** (a) Jede Sitemap-Datei wird je Crawl genau einmal abgerufen, ausschließlich über den Sitemap-Ingest (nicht zusätzlich als Crawl-URL); sie ist eine eigene URL-Zeile mit Statuscode, erscheint aber nicht in den Tabellen. (b) Weiterleitungen beim Abruf werden bis zu 5 Hops verfolgt (wie robots.txt, CRAWL-098); die Sitemap wird unter der angefragten URL geführt. (c) Tiefe: Seed (Start-/Listen-URL, `sitemaps.urls`, robots.txt-Zeile) = 0, Kinder eines Index = Tiefe + 1; abgerufen werden Dateien bis Tiefe 3, die Einträge eines Index auf Tiefe 3 nicht (Warnung). (d) Einträge, deren Schema/Host/Port von der Sitemap-Datei abweichen, werden nicht übernommen (sitemaps.org); die Freigabe über robots.txt (Cross-Submission) ist M2. (e) hreflang-Angaben eines `<url>`-Blocks werden der URL im `<loc>` zugeordnet und mit Quelle `sitemap` und Bezug auf die Sitemap-Datei gespeichert (TECH-070); steht eine URL in mehreren Sitemaps, bleiben alle Angaben erhalten.
- **Testfixture:** `tests/fixtures/tech/sitemaps/`: `robots.txt` mit `Sitemap:`-Zeile auf `sitemap_index.xml` → `sitemap-pages.xml`, `sitemap-posts.xml.gz`.

### TECH-211 Tab „Sitemaps“: Population und Spalten
- **Priorität:** P0
- **Beschreibung:** Abgleich zwischen Crawl und Sitemaps.
- **Eingaben/Konfiguration:** Nachlauf „Sitemaps“ in der Crawl-Analyse.
- **Ausgabe (Spalten/Filter/Export):** `Address`, `Content Type`, `Status Code`, `Status`, `Indexability`, `Indexability Status`, zusätzlich `Sitemaps` (Anzahl Sitemaps, in denen die URL steht). Filter TECH-212 bis TECH-218 plus „All“.
- **Akzeptanzkriterien:**
  - [ ] Population: alle internen URLs aus Crawl ∪ Sitemaps sowie die Sitemap-Dateien selbst (letztere nur in TECH-217/218).
  - [ ] Ohne verarbeitete Sitemap: Tab leer mit Hinweis; TECH-213 meldet dann **nichts** (sonst wäre jede Seite „nicht in Sitemap“).
- **Testfixture:** `sitemaps/`.

### TECH-212 Filter „URLs In Sitemap“
- **Priorität:** P0
- **Beschreibung:** URLs, die in mindestens einer Sitemap stehen.
- **Eingaben/Konfiguration:** –
- **Ausgabe (Spalten/Filter/Export):** Filter „URLs In Sitemap“ (Information). Nachlauf-Filter (Bereich „Sitemaps“, TECH-005; korrigiert, vorher „live“).
- **Akzeptanzkriterien:**
  - [ ] Bedingung: URL (exakter Vergleich nach TECH-002) kommt in ≥ 1 `<loc>` eines `urlset` vor.
  - [ ] `https://site.test/a/` in Sitemap und gecrawlt als `https://site.test/a` → zwei verschiedene URLs.
- **Testfixture:** `sitemaps/sitemap-pages.xml` mit 5 URLs.

### TECH-213 Filter „URLs Not In Sitemap“
- **Priorität:** P0
- **Beschreibung:** Indexierbare Seiten aus dem Crawl, die in keiner Sitemap stehen.
- **Eingaben/Konfiguration:** Nachlauf.
- **Ausgabe (Spalten/Filter/Export):** Filter „URLs Not In Sitemap“; Einstufung: Fehler / Mittel.
- **Akzeptanzkriterien:**
  - [ ] Bedingung: interne URL, im Crawl gefunden, `Indexable`, HTML, nicht in TECH-212.
  - [ ] Non-indexable URLs (3xx, 4xx, noindex, canonicalised …) und Nicht-HTML-Ressourcen (Bilder, CSS, JS, PDF) werden ausgeschlossen (Festlegung für PDF: ausgeschlossen).
- **Testfixture:** `sitemaps/` – `/not-in-sitemap` verlinkt, indexierbar; `/noindex-not-in-sitemap` (nicht gelistet).

### TECH-214 Filter „Orphan URLs“
- **Priorität:** P0
- **Beschreibung:** Seiten, die die normale Erkundung ab der Start-URL nicht erreicht. Dazu zählen (a) URLs, die nur aus Sitemaps bekannt sind, und (b) URLs, die nur über solche Sitemap-URLs entdeckt wurden. Beide sind für einen Besucher bzw. Crawler, der der Seitenstruktur folgt, unsichtbar.
- **Eingaben/Konfiguration:** Nachlauf (Bereich „Sitemaps“); „Crawl Linked XML Sitemaps“ bzw. eine angegebene Sitemap; Sitemap-URLs werden gecrawlt (damit Status/Indexierbarkeit bekannt sind).
- **Ausgabe (Spalten/Filter/Export):** Filter „Orphan URLs“; Einstufung: Fehler / Mittel.
- **Akzeptanzkriterien:**
  - [ ] Algorithmus: Es wird die Menge R der URLs bestimmt, die von der Start-URL (bzw. den Start-URLs) aus über Kanten des Crawls erreichbar sind, **ohne** Kanten des Typs „XML Sitemap“ zu benutzen. Als Kanten zählen Hyperlinks, Redirects (inkl. Meta Refresh), Canonicals, hreflang, Pagination-Link-Elemente und, im JS-Modus, JS-Links (Festlegung zu den Kantentypen, Anhang A Punkt 8). Orphan = interne URL, die in einer Sitemap steht **oder** nur über Ketten erreichbar ist, die bei einer Sitemap-URL beginnen, und die **nicht** in R liegt.
  - [ ] Folge-Orphans: `/orphan-child`, das nur von der Sitemap-URL `/orphan-page` verlinkt ist, ist ebenfalls Orphan (korrigiert gegenüber der früheren Fassung). Ebenso `/orphan-grandchild` (nur von `/orphan-child` verlinkt).
  - [ ] Eine Seite, die in der Sitemap steht **und** von R aus verlinkt ist, ist kein Orphan, auch wenn sie zuerst über die Sitemap abgerufen wurde (Abrufreihenfolge egal).
  - [ ] Eine Folge-URL, die zusätzlich von R aus verlinkt ist, ist kein Orphan.
  - [ ] Ressourcen (Bilder, CSS, JS) werden nie als Orphan gemeldet, nur HTML-Seiten und Dokumente (PDF) (Festlegung).
  - [ ] Bei „Ignore Non-Indexable URLs for Issues“ an (TECH-009) nur indexierbare Orphans.
  - [ ] Im Listenmodus (ohne Spider-Crawl) bleibt der Filter leer.
  - [ ] Die Inlinks-Ansicht einer Folge-Orphan-URL zeigt die Orphan-Quelle als Hyperlink-Inlink, nicht als Sitemap-Inlink. Der Export „Orphan URLs“ (OUT-057, Zeilenauswahl TECH-219) führt für Folge-Orphans in `Sitemap` die Sitemap der Wurzel-Orphan-URL und in der Spalte `Found Via` deren Adresse; bei Wurzel-Orphans ist `Found Via` leer (OS-Festlegung, dieser Eintrag ist für den Spaltennamen führend, OUT-057 übernimmt ihn).
- **Testfixture:** `sitemaps/orphan-page` nur in Sitemap; `/orphan-child` nur von `/orphan-page` verlinkt (Orphan); `/orphan-grandchild` nur von `/orphan-child` verlinkt (Orphan); `/shared-child` von `/orphan-page` **und** von der Startseite verlinkt (kein Orphan); `/in-sitemap-and-linked` in Sitemap und im Menü (kein Orphan). Golden: genau 3 Orphans.

### TECH-215 Filter „Non-Indexable URLs In Sitemap“
- **Priorität:** P0
- **Beschreibung:** Sitemap-Einträge, die nicht indexierbar sind; Sitemaps sollen nur kanonische, indexierbare 200-URLs enthalten.
- **Eingaben/Konfiguration:** Nachlauf.
- **Ausgabe (Spalten/Filter/Export):** Filter „Non-Indexable URLs In Sitemap“; Einstufung: Fehler / Mittel.
- **Akzeptanzkriterien:**
  - [ ] Bedingung: URL in TECH-212 und `Indexability = Non-Indexable` (jeder Grund aus TECH-001).
  - [ ] Nicht abgerufene Sitemap-URLs (z. B. durch Limit) werden nicht gelistet.
- **Testfixture:** `sitemaps/sitemap-pages.xml` enthält `/redirected` (301), `/gone` (410), `/noindex`, `/canonicalised`, `/blocked` (robots).

### TECH-216 Filter „URLs In Multiple Sitemaps“
- **Priorität:** P1
- **Beschreibung:** URL steht in mehr als einer Sitemap-Datei.
- **Eingaben/Konfiguration:** Nachlauf.
- **Ausgabe (Spalten/Filter/Export):** Filter „URLs In Multiple Sitemaps“; Einstufung: Warnung / Niedrig.
- **Akzeptanzkriterien:**
  - [ ] Bedingung: URL kommt in ≥ 2 **verschiedenen** `urlset`-Dateien vor. Zweimal in derselben Datei zählt nicht (Festlegung).
- **Testfixture:** `/both` in `sitemap-pages.xml` und `sitemap-posts.xml.gz`; `/dup-same-file` zweimal in einer Datei.

### TECH-217 Filter „XML Sitemap With Over 50k URLs“
- **Priorität:** P1
- **Beschreibung:** Sitemap-Dateien über der Protokollgrenze von 50.000 Einträgen.
- **Eingaben/Konfiguration:** –
- **Ausgabe (Spalten/Filter/Export):** Filter „XML Sitemap With Over 50k URLs“; Einstufung: Fehler / Hoch. Gelistet: die Sitemap-Datei.
- **Akzeptanzkriterien:**
  - [ ] Bedingung: Anzahl `<url>`- (urlset) bzw. `<sitemap>`-Einträge (index) `> 50000`. Genau 50.000 → nicht gelistet.
- **Testfixture:** generierte `sitemaps/big-50001.xml` und `/big-50000.xml` (Fixture-Server erzeugt sie dynamisch).

### TECH-218 Filter „XML Sitemap With Over 50MB“
- **Priorität:** P1
- **Beschreibung:** Sitemap-Dateien über der Größengrenze.
- **Eingaben/Konfiguration:** –
- **Ausgabe (Spalten/Filter/Export):** Filter „XML Sitemap With Over 50MB“; Einstufung: Fehler / Hoch.
- **Akzeptanzkriterien:**
  - [ ] Bedingung: unkomprimierte Größe `> 52 428 800` Bytes (50 MiB). Bei gzip zählt die entpackte Größe.
  - [ ] Der Parser darf große Dateien nicht vollständig in den Speicher laden (Streaming).
- **Testfixture:** dynamisch erzeugte `sitemaps/huge.xml.gz` (komprimiert klein, entpackt > 50 MiB durch lange `<loc>`-Werte bzw. Leerraum).

### TECH-219 Sitemap-Inlinks und Bulk-Exporte
- **Priorität:** P1
- **Beschreibung:** Nachvollziehbarkeit, aus welcher Sitemap eine URL stammt. Die Bulk-Exporte der Sitemaps-Filter sind führend in OUT-057 spezifiziert (Namen und Spalten); dieser Eintrag legt nur die Inlinks-Darstellung und die Zeilenauswahl fest.
- **Eingaben/Konfiguration:** –
- **Ausgabe (Spalten/Filter/Export):** In der unteren Inlinks-Ansicht Typ `XML Sitemap` mit `From` = Sitemap-URL. Bulk-Exporte: OUT-057.
- **Akzeptanzkriterien:**
  - [ ] Zeilenauswahl der Bulk-Exporte (OUT-057): je (Sitemap, URL)-Paar einer URL im jeweiligen Filter genau eine Zeile; bei Filtern ohne Sitemap-Bezug („URLs Not In Sitemap“) je URL eine Zeile.
  - [ ] Sitemap-Inlinks zählen nicht in `Inlinks`/`Unique Inlinks` des Links-Tabs.
- **Testfixture:** `sitemaps/` Golden-Exporte.

---

## Mobile

### TECH-230 Mobile-Prüfungen: Ausführungsweg (lokal/PSI)
- **Priorität:** P1
- **Beschreibung:** Führend für Tab, Filter und Spalten des Mobile-Bereichs (K-14; TECH-230–237). Die Datenbeschaffung ist führend in 04 spezifiziert: Quellen und **Standardquelle** in INTEG-130, lokaler Lighthouse-Lauf in INTEG-132, Gerätetyp und mobile Emulation in INTEG-131, Auswahl der Metrikgruppe in INTEG-133, Audit-Zuordnung und Score-Umrechnung in INTEG-140, Fehlerbehandlung in INTEG-143. Dieser Eintrag legt keine eigene Quelle, Emulation oder Standardquelle fest. Die beiden Quellen im Sinne dieses Tabs: **Lokal** = Lighthouse läuft auf dem eigenen Rechner (INTEG-132: eigene Chrome-Instanz getrennt vom Render-Browser des Crawls, gebündelte oder vorhandene Node-/Lighthouse-Laufzeit, kein API-Schlüssel, kein Tageskontingent, Authentifizierung aus der Crawl-Konfiguration); **Remote** = Lighthouse über die PageSpeed-Insights-API mit API-Schlüssel (INTEG-130). Beide Quellen liefern Lighthouse-Audits, die nach INTEG-140 auf dieselben Spalten abgebildet werden.
- **Eingaben/Konfiguration:**
  - PSI-/Lighthouse-Anbindung verbunden (INTEG-130) mit Quelle Lokal oder Remote; Standardquelle: siehe INTEG-130.
  - Metrikgruppe „Mobile“ ausgewählt (INTEG-133; Standardauswahl dort).
  - Ohne diese beiden Voraussetzungen (Anbindung an **und** Gruppe gewählt) bleiben alle Lighthouse-basierten Spalten und Filter (TECH-232–236) leer; die Übersicht zeigt bei ihnen den Hinweis „Mobile-Prüfung nicht konfiguriert“ statt 0. Spalte und Filter `Mobile Alternate Link` (TECH-237) stammen aus dem gecrawlten HTML und sind davon unabhängig.
- **Ausgabe (Spalten/Filter/Export):** Spalten `PSI Status` (genau `Success` oder `Error`, wie in der Referenz-Doku; leer, solange die URL noch nicht abgefragt wurde; Wertebereich führend hier, INTEG-143 verweist hierher) und `PSI Error` (Code und Meldung nach INTEG-143; ein erschöpftes Tageskontingent ist ein `Error` mit entsprechender Meldung).
- **Akzeptanzkriterien:**
  - [ ] Anbindung an, Gruppe „Mobile“ abgewählt → keine Mobile-Prüfspalten befüllt, Tab ohne Lighthouse-Werte, Hinweis wie oben. Die übrigen PSI-Metriken (Performance usw.) sind davon unabhängig.
  - [ ] Quelle Lokal: Die Befüllung hängt nicht vom Rendering-Modus des Crawls ab. Der Lighthouse-Lauf nutzt seine eigene Chrome-Instanz (INTEG-132), auch wenn der Crawl im Text-only-Modus läuft.
  - [ ] Fehler beim Abruf (Timeout, Absturz des Lighthouse-Prozesses, API-Fehler, Kontingent erschöpft; Klassen nach INTEG-143) → alle Mobile-Prüfspalten leer, `PSI Status = Error`, `PSI Error` mit Grund, URL in keinem Mobile-Filter.
  - [ ] **Nur mit Quelle Lokal verfügbar:** „Content Not Sized Correctly“ (TECH-234); die Referenz-Doku nennt dieses Audit ausdrücklich als von der Remote-API nicht mehr geliefert. Bei Remote bleibt die Spalte `Content Width` leer, der Filter enthält keine URL und trägt in der Übersicht den Hinweis „nur mit lokaler Quelle“.
  - [ ] Allgemein gilt für jedes Lighthouse-Audit dieses Tabs: Liefert die gewählte Quelle das Audit nicht (welche Audits je Quelle geliefert werden und wie fehlende Audits umgerechnet werden, regelt INTEG-140), bleibt die Spalte leer, die URL steht in keinem zugehörigen Filter, und die Übersicht zeigt „Audit von der Quelle nicht geliefert“.
  - [ ] Für dieselbe Lighthouse-Antwort ergeben Remote- und Lokal-Weg identische Spaltenwerte und Filtermengen (Umrechnung INTEG-140). Golden-Test für `Viewport`, `Tap Targets` und `Plugins` mit einer PSI-Mock-Antwort, deren `lighthouseResult` aus einem lokalen Lauf stammt.
- **Testfixture:** `tests/fixtures/tech/mobile/`: Seiten je Filter (siehe unten) plus `ok.html` (responsiv, keine Befunde). Läufe: (1) Quelle Lokal + Gruppe an (lokaler Lighthouse-Lauf nach INTEG-132); (2) Remote mit gemockter PSI-Antwort ohne die Audits `content-width` und `font-size` → `Content Width`/`Font Display` leer; (3) Anbindung an, Gruppe aus → Lighthouse-Spalten leer, `m-alternate.html` trotzdem im Filter „Mobile Alternate Link“; (4) Anbindung aus → wie (3).

### TECH-231 Tab „Mobile“: Population und Spalten
- **Priorität:** P1
- **Beschreibung:** Mobile-Tauglichkeit je Seite.
- **Eingaben/Konfiguration:** PSI-/Lighthouse-Anbindung mit Metrikgruppe „Mobile“ (TECH-230, INTEG-133) für die Prüfspalten; für `Mobile Alternate Link` die Speichern-/Crawlen-Optionen für mobile Alternate-Links (CRAWL-051).
- **Ausgabe (Spalten/Filter/Export):** `Address`, `PSI Status`, `PSI Error`, `Viewport`, `Tap Targets`, `Content Width`, `Font Display`, `Plugins`, `Mobile Alternate Link`. Filter TECH-232 bis TECH-237 plus „All“. Untere Ansicht „Lighthouse Details“: Inhalt INTEG-144, Layout UI-046, Export der Ansicht OUT-021. Bulk-Export „Reports > Mobile“: OUT-108 (führend für Aufbau und Spalten).
- **Akzeptanzkriterien:**
  - [ ] Population: interne HTML-200-Seiten, für die die Prüfung lief (`PSI Status` gesetzt), **oder** die eine Mobil-Alternative nach TECH-237 haben (dann ggf. mit leeren Prüfspalten).
  - [ ] Spaltennamen und -reihenfolge sind gegen die öffentliche Referenz-Doku geprüft (Referenzstand Oktober 2026): `Address`, `PSI Status`, `PSI Error`, `Viewport`, `Tap Targets`, `Content Width`, `Font Display`, `Plugins`, `Mobile Alternate Link`; die fünf Prüfspalten sind dort als Lighthouse-Score von 100 beschrieben.
  - [ ] **Werte der Prüfspalten** `Viewport`, `Tap Targets`, `Content Width`, `Font Display`, `Plugins`: ganzzahliger Lighthouse-Score 0–100 wie in der Referenz. Die Umrechnung aus dem Audit-Ergebnis ist führend in INTEG-140 und gilt für beide Quellen gleich (Score × 100, gerundet; nicht gelieferte, fehlerhafte oder manuelle Audits leer). Nur die DOM-Erkennung von `Plugins` (TECH-236) liefert binär `100` (bestanden) oder `0` (nicht bestanden). Leer = nicht geprüft. Ein Filter enthält eine URL genau dann, wenn der Score der zugehörigen Spalte gesetzt und `< 100` ist. Das gilt auch für nicht-binäre Scores wie Target Size (Festlegung: jeder Abzug zählt als Befund).
  - [ ] Die Bedingungen in TECH-232 … TECH-235 beschreiben die erwartete Semantik der zugehörigen Lighthouse-Audits und bestimmen die Fixture-Erwartungen; maßgeblich für Spalte und Filter ist der Audit-Score. Weicht die gebündelte Lighthouse-Version (INTEG-132) davon ab, gilt der Audit-Score, und die Fixture-Erwartung wird mit Vermerk in Anhang A angepasst.
  - [ ] `Mobile Alternate Link`: Ziel-URL (Text), nicht Score.
  - [ ] Spaltenname `Font Display` = Referenzname für die Schriftgrößen-Prüfung (TECH-235). Die frühere Arbeitsbezeichnung `Font Size` entfällt, damit das Spalten-Mapping ohne Alias auskommt. Im Tooltip steht „Lesbare Schriftgrößen“.
  - [ ] Zeilenauswahl für „Reports > Mobile“ (OUT-108): Die Tabelle je Mobile-Filter enthält genau die URLs dieses Filters.
- **Testfixture:** `mobile/`; Golden: `ok.html` alle Prüfspalten `100`; `no-viewport.html` `Viewport = 0`; Remote-Lauf: `Content Width`/`Font Display` leer.

### TECH-232 Filter „Viewport Not Set“
- **Priorität:** P1
- **Beschreibung:** Kein brauchbares Viewport-Meta; Mobilgeräte zeigen dann das Desktop-Layout verkleinert.
- **Eingaben/Konfiguration:** –
- **Ausgabe (Spalten/Filter/Export):** Filter „Viewport Not Set“; Einstufung: Fehler / Hoch. Spalte `Viewport`.
- **Akzeptanzkriterien:**
  - [ ] Erwartete Semantik des Audits `viewport` (TECH-231): Fail, wenn kein `<meta name="viewport">` existiert **oder** dessen `content` weder eine `width`- noch eine `initial-scale`-Angabe enthält.
  - [ ] `width=device-width` → Pass; `width=1024` → Pass; `initial-scale=1` allein → Pass (Lighthouse-Semantik; die Referenzbeschreibung nennt nur `width`, siehe Anhang A).
  - [ ] Spaltenwert: Audit-Score nach INTEG-140 (`100` bestanden, `0` nicht bestanden), nicht geprüft = leer (TECH-231).
- **Testfixture:** `mobile/no-viewport.html` (→ `0`), `/viewport-scale-only.html` (nur `initial-scale=1` → `100`), `/viewport-fixed-1024.html` (→ `100`).

### TECH-233 Filter „Target Size“
- **Priorität:** P1
- **Beschreibung:** Bedienelemente (Links, Buttons, Formularfelder) sind zu klein oder liegen zu dicht beieinander.
- **Eingaben/Konfiguration:** Mindestgröße 24×24 CSS-px (fest, entspricht WCAG 2.2 SC 2.5.8 und dem aktuellen Lighthouse-Audit). Gemessen im Lighthouse-Lauf in mobiler Emulation (INTEG-131/INTEG-132), im initialen Viewport **und** unterhalb (ganze Seitenhöhe bis 8192 px, TECH-006).
- **Ausgabe (Spalten/Filter/Export):** Filter „Target Size“; Einstufung: Fehler / Mittel. Spalte `Tap Targets` (Score, TECH-231). Lighthouse Details: je fehlerhaftem Ziel Selektor, Größe `B×H`, nächster Nachbar und Abstand in px.
- **Akzeptanzkriterien:**
  - [ ] Geprüfte Elemente (Ziele): `a[href]`, `button`, `input` (außer `type=hidden`), `select`, `textarea`, `summary`, Elemente mit `role` ∈ {`button`, `link`, `checkbox`, `radio`, `tab`, `menuitem`, `switch`, `option`} und Elemente mit `tabindex ≥ 0`. Nicht geprüft: `disabled`-Elemente, Elemente mit `display:none`, `visibility:hidden`, `opacity:0`, Breite oder Höhe 0, und Elemente vollständig außerhalb der Seite.
  - [ ] Größe = Bounding-Box (`getBoundingClientRect`) in CSS-px, auf 0,1 px gerundet.
  - [ ] Regel A (Größe): Ein Ziel besteht, wenn Breite ≥ 24 **und** Höhe ≥ 24.
  - [ ] Regel B (Abstand), nur für Ziele, die A nicht erfüllen: Um den Mittelpunkt der Box wird ein Kreis mit Durchmesser 24 px gelegt. Das Ziel besteht, wenn dieser Kreis weder die Box eines anderen Ziels noch den 24-px-Kreis eines anderen unterdimensionierten Ziels **schneidet**. Berühren (Abstand der Mittelpunkte genau 24,0 px bzw. Kreisrand genau auf Boxkante) gilt als nicht schneidend.
  - [ ] Regel C (Ausnahme Fließtext): Links, die inline in einem Satz stehen (Elternblock enthält außerhalb des Links nicht-leeren Text in derselben Zeilenbox), bestehen immer.
  - [ ] Verschachtelte Ziele (z. B. `button` in `a`) werden als ein Ziel gewertet (das äußere), um Doppelzählung zu vermeiden (Festlegung).
  - [ ] Erwartetes Ergebnis: Score `0` (Filter), sobald mindestens ein Ziel weder A noch B noch C erfüllt; sonst `100`. Maßgeblich ist für beide Quellen der Audit-Score nach INTEG-140 (TECH-231).
  - [ ] Unabhängig von der Accessibility-Regel `target-size` (TECH-257); beide dürfen für dieselbe Seite auslösen.
- **Testfixture:** `mobile/target-size/`: `adjacent-16.html` (zwei 16×16-Buttons ohne Abstand → 0), `adjacent-23.html` (23×23 nebeneinander → 0), `adjacent-24.html` (24×24 ohne Abstand → 100), `spaced-16-24.html` (16×16, Mittelpunkte genau 24 px auseinander → 100, Berührung), `spaced-16-23.html` (Mittelpunkte 23 px → 0), `spaced-16-30.html` (→ 100), `inline-link.html` (kleiner Link im Satz → 100), `hidden-small.html` (unsichtbarer 10×10-Button → 100), `below-fold.html` (fehlerhafte Ziele erst bei y = 2000 px → 0).

### TECH-234 Filter „Content Not Sized Correctly“
- **Priorität:** P1
- **Beschreibung:** Inhaltsbreite passt nicht zur Viewport-Breite (horizontales Scrollen).
- **Eingaben/Konfiguration:** Nur Quelle Lokal (TECH-230; Audit `content-width`, INTEG-140).
- **Ausgabe (Spalten/Filter/Export):** Filter „Content Not Sized Correctly“; Einstufung: Fehler / Hoch. Spalte `Content Width`.
- **Akzeptanzkriterien:**
  - [ ] Erwartete Semantik des Audits: Fail, wenn die Inhaltsbreite die Viewport-Breite übersteigt (`document.scrollingElement.scrollWidth > window.innerWidth`).
  - [ ] Seite ohne Viewport-Meta: keine eigene Sonderregel; der Spaltenwert folgt dem Audit-Ergebnis nach INTEG-140.
  - [ ] Quelle Remote: Spalte `Content Width` leer, Filter leer (Audit von der API nicht mehr geliefert, TECH-230).
  - [ ] Spaltenwert: Audit-Score nach INTEG-140, nicht geprüft = leer (TECH-231).
- **Testfixture:** `mobile/wide.html` (Element mit fester Breite 900 px), `ok.html`.

### TECH-235 Filter „Illegible Font Size“
- **Priorität:** P1
- **Beschreibung:** Zu großer Anteil zu kleiner Schrift.
- **Eingaben/Konfiguration:** Schwelle 12 px, Anteil 40 % (fest, wie in der Referenz-Doku). Verfügbarkeit: bei beiden Quellen nur, wenn das Lighthouse-Ergebnis das Audit `font-size` enthält (TECH-230, INTEG-140; die Referenz-Doku kennzeichnet nur „Content Not Sized Correctly“ als lokal-exklusiv).
- **Ausgabe (Spalten/Filter/Export):** Filter „Illegible Font Size“; Einstufung: Fehler / Hoch. Spalte `Font Display` (Score, TECH-231); Detail: Anteil in % und die drei häufigsten zu kleinen Schriftgrößen mit Selektor.
- **Akzeptanzkriterien:**
  - [ ] Quelle ohne geliefertes Audit: Spalte `Font Display` leer, Filter leer, Übersicht mit Hinweis „Audit von der Quelle nicht geliefert“ (TECH-230). Liefert die Quelle das Audit, wird dessen Score übernommen (TECH-231).
  - [ ] Spaltenwert: Audit-Score nach INTEG-140.
  - [ ] Erwartete Semantik des Audits: Gemessen wird über alle sichtbaren Textknoten: Gewicht = Anzahl Zeichen ohne Whitespace; Schriftgröße = berechnete `font-size` des Elternelements in CSS-px.
  - [ ] Fail, wenn Anteil der Zeichen mit `font-size < 12` **größer als 40 %** ist. Genau 40 % → Pass.
  - [ ] Seite ohne Viewport-Meta: keine eigene Sonderregel; der Spaltenwert folgt dem Audit-Ergebnis nach INTEG-140.
- **Testfixture:** `mobile/small-font-41.html` (41 % der Zeichen in 11 px → 0), `/small-font-40.html` (genau 40 % → 100), `/small-font-12px.html` (alles in genau 12 px → 100, Grenze exklusiv). Zusätzlich Remote-Lauf mit gemockter PSI-Antwort: `Font Display` leer.

### TECH-236 Filter „Unsupported Plugins“
- **Priorität:** P1
- **Beschreibung:** Seiten mit Browser-Plugins (Flash, Silverlight, Java-Applets), die mobil nicht laufen und von Suchmaschinen nicht indexiert werden.
- **Eingaben/Konfiguration:** Metrikgruppe „Mobile“ (TECH-230, INTEG-133). Liefert die Quelle ein Lighthouse-Audit `plugins`, wird dessen Score übernommen (INTEG-140); sonst ermittelt OpenSpider den Wert selbst aus dem DOM des Crawls (gerendert im JS-Rendering-Modus, sonst Roh-HTML) nach den folgenden Regeln (Festlegung).
- **Ausgabe (Spalten/Filter/Export):** Filter „Unsupported Plugins“; Einstufung: Fehler / Hoch. Spalte `Plugins`.
- **Akzeptanzkriterien:**
  - [ ] Fail bei: `<applet>`; `<embed>` oder `<object>` mit `type` `application/x-shockwave-flash`, `application/futuresplash`, `application/x-silverlight`, `application/x-silverlight-2` bzw. `application/x-silverlight*` oder `application/x-java-applet*`; `<object classid>` mit einer bekannten Flash-, Silverlight- oder Java-ClassID; Datei-Endung `.swf`, `.xap`, `.class`, `.jar` in `src`/`data`; `<param name="movie">` mit `.swf`.
  - [ ] `<object>`/`<embed>` mit Bild-, SVG-, PDF- oder HTML-Inhalt (z. B. `type="image/svg+xml"`, `type="application/pdf"`) → Pass.
  - [ ] Ein per JavaScript eingefügtes Plugin-Element wird nur erkannt, wenn das DOM gerendert geprüft wird (JS-Rendering-Modus oder Lighthouse-Audit `plugins`).
  - [ ] Spaltenwert: Pass = `100`, Fail = `0`, nicht geprüft = leer (TECH-231); Filter enthält die URL genau dann, wenn `Plugins` < 100.
- **Testfixture:** `mobile/flash.html`, `/silverlight.html`, `/applet.html`, `/js-flash.html`, `/svg-object.html` und `/object-pdf.html` (Negativtests).

### TECH-237 Filter „Mobile Alternate Link“
- **Priorität:** P1
- **Beschreibung:** Führend für Spalte und Filter (K-14; INTEG-147 und CRAWL-051 verweisen für die Logik hierher). Seite verweist auf eine separate Mobil-URL (statt responsivem Design). Zulässig, aber meist weniger effizient, weil dieselben Inhalte unter zwei URLs gepflegt werden. Die Daten stammen aus dem gecrawlten Dokument, nicht aus Lighthouse, und werden daher auch ohne PSI-Anbindung befüllt (die Referenz-Doku beschreibt die Spalte ohne PSI-Bezug).
- **Eingaben/Konfiguration:** Speichern/Crawlen von Mobile-Alternate-Links in der Spider-Konfiguration (CRAWL-051; Standardwerte dort). „Speichern“ befüllt Spalte und Filter, „Crawlen“ ruft zusätzlich die Mobil-URLs ab.
- **Ausgabe (Spalten/Filter/Export):** Filter „Mobile Alternate Link“; Einstufung: Warnung / Hoch. Spalte `Mobile Alternate Link` = absolut aufgelöste Ziel-URL (TECH-002).
- **Akzeptanzkriterien:**
  - [ ] Bedingung: `<link>` im Head (TECH-003), dessen `rel` das Token `alternate` enthält, mit einem `media`-Attribut, das eine Bildschirmbreiten-Bedingung (`max-width` oder `max-device-width`) enthält oder `handheld` lautet, ohne `hreflang` und ohne `type` (Festlegung der Erkennungsregel).
  - [ ] Ein gleichwertiger HTTP-`Link`-Header (`rel="alternate"` mit passendem `media`) wird ebenso ausgewertet.
  - [ ] Mehrere passende Angaben: Die erste in Dokumentreihenfolge (HTML vor Header) steht in der Spalte; die Anzahl steht im Tooltip.
  - [ ] Relatives `href` wird gegen die Basis-URL aufgelöst.
  - [ ] `rel=alternate` mit `hreflang` (TECH-070) oder `type="application/rss+xml"` → nicht gelistet.
  - [ ] Filter enthält die URL genau dann, wenn die Spalte nicht leer ist; das gilt auch bei abgeschalteter PSI-Anbindung.
- **Testfixture:** `mobile/m-alternate.html` → `https://m.site.test/page`; `/m-alternate-relative.html` (`href="/m/page"`), `/m-alternate-header` (Link-Header), `/hreflang-only.html` und `/rss-alternate.html` (Negativtests); Lauf mit abgeschalteter PSI-Anbindung.

---

## Accessibility

### TECH-250 Accessibility-Prüfung mit axe-core
- **Priorität:** P1
- **Beschreibung:** Die Regelbibliothek axe-core (Deque, MPL-2.0 – lizenzkompatibel) wird nach dem Rendern in die Seite injiziert und ausgeführt. Ergebnisse werden je Regel und betroffenem Knoten gespeichert und WCAG-Stufen zugeordnet.
- **Eingaben/Konfiguration:** „Accessibility“ in der Extraktionskonfiguration (Standard aus) – erfordert Rendering-Modus JavaScript; axe-core-Version gebündelt (Ziel: 4.11.x, wie im Referenzstand), alle Regeln der unten gelisteten Gruppen explizit aktiviert (auch die standardmäßig deaktivierten AAA- und Best-Practice-Regeln). Es werden **alle vier Ergebnisarten** angefordert: `violations`, `incomplete`, `inapplicable`, `passes` (korrigiert; vorher nur `violations`). Damit die Datenmenge beherrschbar bleibt, werden für `passes` und `inapplicable` nur Regel-ID und Status gespeichert, keine Knoten (Festlegung). Für `incomplete` werden Knoten wie bei Verstößen gespeichert.
- **Ausgabe (Spalten/Filter/Export):** Je URL und Regel: Regel-ID, Ergebnisart (`violation` / `incomplete` / `inapplicable` / `passed`), Impact (`minor`/`moderate`/`serious`/`critical`, bei `inapplicable`/`passed` leer), Anzahl Knoten, je Knoten Selektor, HTML-Ausschnitt, Meldung (eigener Kurztext je Regel; die englische axe-Meldung wird zusätzlich im Feld `axe_message` gespeichert), Gruppe (s. u.).
- **Akzeptanzkriterien:**
  - [ ] Zuordnung der Regel zu genau einer Filtergruppe nach den Tabellen TECH-252–TECH-257 (nicht nach den axe-Tags, die mehrfach sein können).
  - [ ] Nur `violations` zählen als Verstoß und speisen Filter und Zählspalten (TECH-251). `incomplete` (nicht automatisch entscheidbar, manuelle Prüfung nötig), `inapplicable` und `passed` erscheinen ausschließlich in der Detailansicht TECH-259.
  - [ ] Für jede aktivierte Regel und jede geprüfte Seite gibt es genau ein Ergebnis in genau einer Ergebnisart. Die Summe über die vier Arten ist gleich der Zahl der aktivierten Regeln (Vollständigkeitsprüfung im Test).
  - [ ] Lizenz/Attribution: axe-core steht unter MPL-2.0. Das unveränderte axe-Bundle wird als separate Datei ausgeliefert; Lizenztext und Quellenangabe (Deque Systems, Version) stehen in `THIRD_PARTY_NOTICES` (Anhang C). Die Kurzbeschreibungen der Regeln in TECH-252–257 sind eigene Formulierungen, keine Übersetzung der axe-Hilfetexte; Regel-IDs werden als technische Schlüssel unverändert verwendet. `Help URL` verweist auf die Regelseite des Herstellers, statt deren Text zu übernehmen.
  - [ ] Die Prüfung läuft im selben Render-Durchgang wie die Seite (kein zweiter Seitenabruf).
  - [ ] Ohne JS-Rendering ist der Tab leer, Übersicht zeigt Hinweis.
  - [ ] Eine Regel, die in der gebündelten axe-Version nicht existiert, wird übersprungen und beim Start einmalig protokolliert.
- **Testfixture:** `tests/fixtures/tech/a11y/`: je Gruppe eine Seite mit einem gezielten Verstoß pro Regel (soweit statisch provozierbar) und `a11y/clean.html` ohne Verstöße.

### TECH-251 Tab „Accessibility“: Population und Spalten
- **Priorität:** P1
- **Beschreibung:** Übersicht der Verstöße je Seite und Stufe.
- **Eingaben/Konfiguration:** –
- **Ausgabe (Spalten/Filter/Export):** `Address`, `Content Type`, `Status Code`, `Status`, `Indexability`, `Indexability Status`, `All Violations`, `Best Practice Violations`, `WCAG 2.0 A Violations`, `WCAG 2.0 AA Violations`, `WCAG 2.0 AAA Violations`, `WCAG 2.1 AA Violations`, `WCAG 2.2 AA Violations`, `PSI Status`, `PSI Error`, `Accessibility Score`. Filter: „All“, Score-Filter (TECH-258), je Regel ein Filter (TECH-252–257).
- **Akzeptanzkriterien:**
  - [ ] `<Gruppe> Violations` = Anzahl **verletzter Regeln** dieser Gruppe auf der Seite (nicht Knoten), `All Violations` = Summe der Gruppen (Festlegung; Paritätsprüfung nötig).
  - [ ] Population: interne HTML-200-Seiten, die gerendert und geprüft wurden.
  - [ ] Jeder Regel-Filter listet die Seiten mit ≥ 1 Verstoß gegen diese Regel; Filtername in der UI = eigene Kurzbezeichnung, Schlüssel = axe-Regel-ID.
  - [ ] Einstufung aller Accessibility-Filter: Warnung; Dringlichkeit laut Tabelle.
- **Testfixture:** `a11y/`.

### TECH-252 Filtergruppe „Best Practice“ (28 Regeln)
- **Priorität:** P1
- **Beschreibung:** Empfehlungen ohne direkte WCAG-Erfolgskriterium-Zuordnung. Jede Zeile ist ein eigener Filter (Schlüssel = axe-Regel-ID, Dringlichkeit in Klammern). Die Kurzbeschreibungen in TECH-252–TECH-257 sind eigene, knappe Zusammenfassungen der Prüfabsicht; sie wurden nicht aus den axe-Hilfetexten übersetzt. Maßgeblich für das Verhalten ist die gebündelte axe-Version (Attribution: TECH-250, Anhang C).
- **Eingaben/Konfiguration:** –
- **Ausgabe (Spalten/Filter/Export):** 28 Filter, gezählt in `Best Practice Violations`.
- **Akzeptanzkriterien:**
  - [ ] `accesskeys` – accesskey-Werte sind nicht eindeutig (Hoch)
  - [ ] `presentation-role-conflict` – als präsentational markierte Elemente sind dennoch fokussierbar/mit globalen ARIA-Attributen (Niedrig)
  - [ ] `tabindex` – tabindex größer 0 (Hoch)
  - [ ] `skip-link` – Ziel eines Sprunglinks fehlt oder ist nicht fokussierbar (Mittel)
  - [ ] `aria-text` – role=text mit fokussierbaren Nachfahren (Hoch)
  - [ ] `aria-dialog-name` – dialog/alertdialog ohne zugänglichen Namen (Hoch)
  - [ ] `aria-allowed-role` – ARIA-Rolle passt nicht zum Element (Niedrig)
  - [ ] `aria-treeitem-name` – treeitem ohne zugänglichen Namen (Hoch)
  - [ ] `region` – Seiteninhalt außerhalb von Landmarks (Mittel)
  - [ ] `landmark-one-main` – kein main-Landmark (Mittel)
  - [ ] `landmark-no-duplicate-banner` – mehr als ein banner (Mittel)
  - [ ] `landmark-banner-is-top-level` – banner in anderem Landmark verschachtelt (Mittel)
  - [ ] `landmark-no-duplicate-contentinfo` – mehr als ein contentinfo (Mittel)
  - [ ] `landmark-no-duplicate-main` – mehr als ein main (Mittel)
  - [ ] `landmark-complementary-is-top-level` – complementary/aside nicht auf oberster Ebene (Mittel)
  - [ ] `landmark-contentinfo-is-top-level` – contentinfo verschachtelt (Mittel)
  - [ ] `landmark-main-is-top-level` – main verschachtelt (Mittel)
  - [ ] `landmark-unique` – gleichartige Landmarks ohne unterscheidenden Namen (Mittel)
  - [ ] `label-title-only` – Formularfeld nur über title/aria-describedby beschriftet, kein sichtbares Label (Hoch)
  - [ ] `frame-tested` – iframe-Inhalt konnte nicht mit axe geprüft werden (Hoch)
  - [ ] `page-has-heading-one` – keine h1 (Mittel)
  - [ ] `heading-order` – Überschriftenebene springt um mehr als 1 (Mittel)
  - [ ] `empty-heading` – leere Überschrift (Niedrig)
  - [ ] `meta-viewport-large` – Viewport verhindert Zoom bis 500 % (Niedrig)
  - [ ] `image-redundant-alt` – Alt-Text wiederholt umgebenden Text (Niedrig)
  - [ ] `empty-table-header` – Tabellenkopfzelle ohne erkennbaren Text (Niedrig)
  - [ ] `table-duplicate-name` – Tabelle mit identischem summary und caption (Niedrig)
  - [ ] `scope-attr-valid` – scope-Attribut falsch verwendet (Hoch)
- **Testfixture:** `a11y/best-practice.html` (Verstöße gegen mindestens: `tabindex`, `region`, `landmark-one-main`, `page-has-heading-one`, `heading-order`, `empty-heading`, `image-redundant-alt`, `accesskeys`).

### TECH-253 Filtergruppe „WCAG 2.0 A“ (56 Regeln)
- **Priorität:** P1
- **Beschreibung:** Regeln der Konformitätsstufe A. Jede Zeile ist ein eigener Filter; Dringlichkeit Hoch, sofern nicht anders angegeben.
- **Eingaben/Konfiguration:** –
- **Ausgabe (Spalten/Filter/Export):** 56 Filter, gezählt in `WCAG 2.0 A Violations`.
- **Akzeptanzkriterien:**
  - [ ] `scrollable-region-focusable` – scrollbarer Bereich nicht per Tastatur erreichbar
  - [ ] `aria-required-attr` – Pflicht-ARIA-Attribute der Rolle fehlen
  - [ ] `aria-conditional-attr` – ARIA-Attribut entgegen den Regeln der Rolle verwendet
  - [ ] `aria-valid-attr-value` – ungültiger Wert eines ARIA-Attributs
  - [ ] `aria-valid-attr` – ungültiger ARIA-Attributname
  - [ ] `aria-command-name` – button/link/menuitem-Rolle ohne Namen
  - [ ] `aria-input-field-name` – ARIA-Eingabefeld ohne Namen
  - [ ] `aria-meter-name` – meter ohne Namen
  - [ ] `aria-progressbar-name` – progressbar ohne Namen
  - [ ] `aria-required-parent` – Rolle ohne erforderliches Elternelement
  - [ ] `aria-roles` – ungültiger role-Wert
  - [ ] `aria-toggle-field-name` – Umschaltfeld ohne Namen
  - [ ] `aria-tooltip-name` – tooltip ohne Namen
  - [ ] `aria-required-children` – Rolle ohne erforderliche Kindrollen
  - [ ] `aria-deprecated-role` – veraltete ARIA-Rolle (Niedrig)
  - [ ] `aria-braille-equivalent` – Braille-Attribut ohne Nicht-Braille-Gegenstück
  - [ ] `aria-hidden-focus` – aria-hidden-Bereich enthält fokussierbare Elemente
  - [ ] `aria-hidden-body` – aria-hidden="true" am body
  - [ ] `aria-prohibited-attr` – für die Rolle verbotenes ARIA-Attribut
  - [ ] `aria-allowed-attr` – für die Rolle nicht erlaubtes ARIA-Attribut
  - [ ] `duplicate-id-aria` – in ARIA/Labels referenzierte ID doppelt
  - [ ] `bypass` – kein Mechanismus zum Überspringen wiederholter Blöcke
  - [ ] `label` – Formularfeld ohne Beschriftung
  - [ ] `form-field-multiple-labels` – Formularfeld mit mehreren label-Elementen (Mittel)
  - [ ] `frame-title` – iframe/frame ohne title
  - [ ] `frame-title-unique` – iframe-titles nicht eindeutig
  - [ ] `frame-focusable-content` – Frame mit fokussierbarem Inhalt hat tabindex=-1
  - [ ] `document-title` – kein `<title>`
  - [ ] `html-lang-valid` – ungültiger lang-Wert am html-Element
  - [ ] `html-has-lang` – html-Element ohne lang
  - [ ] `html-xml-lang-mismatch` – lang und xml:lang widersprechen sich (Mittel)
  - [ ] `meta-refresh` – zeitgesteuerter Meta Refresh (< 20 h)
  - [ ] `input-image-alt` – Bild-Button ohne Alternativtext
  - [ ] `image-alt` – Bild ohne Alternativtext
  - [ ] `object-alt` – object ohne Alternativtext
  - [ ] `area-alt` – area ohne Alternativtext
  - [ ] `role-img-alt` – role=img ohne Alternativtext
  - [ ] `svg-img-alt` – SVG mit Bildrolle ohne zugänglichen Text
  - [ ] `server-side-image-map` – serverseitige Image-Map (Niedrig)
  - [ ] `video-caption` – video ohne Untertitel-Track
  - [ ] `no-autoplay-audio` – audio/video spielt automatisch mit Ton
  - [ ] `button-name` – Button ohne erkennbaren Text
  - [ ] `input-button-name` – Eingabe-Button ohne erkennbaren Text
  - [ ] `link-in-text-block` – Link im Fließtext nur durch Farbe unterscheidbar
  - [ ] `link-name` – Link ohne erkennbaren Text
  - [ ] `select-name` – select ohne zugänglichen Namen
  - [ ] `summary-name` – summary ohne erkennbaren Text
  - [ ] `marquee` – marquee-Element
  - [ ] `blink` – blink-Element
  - [ ] `nested-interactive` – verschachtelte interaktive Elemente
  - [ ] `listitem` – li außerhalb von ul/ol
  - [ ] `list` – ul/ol mit unzulässigen direkten Kindern
  - [ ] `definition-list` – dl mit unzulässigem Inhalt/Reihenfolge
  - [ ] `dlitem` – dt/dd außerhalb von dl
  - [ ] `th-has-data-cells` – Tabellenkopf ohne zugehörige Datenzellen
  - [ ] `td-headers-attr` – headers-Attribut verweist auf Zelle außerhalb der Tabelle
- **Testfixture:** `a11y/wcag20a.html` (mindestens: `image-alt`, `link-name`, `button-name`, `label`, `html-has-lang`, `document-title` (eigene Seite ohne title), `listitem`, `marquee`, `frame-title`, `meta-refresh` (eigene Seite)).

### TECH-254 Filtergruppe „WCAG 2.0 AA“
- **Priorität:** P1
- **Beschreibung:** Regeln der Stufe AA (2.0).
- **Eingaben/Konfiguration:** –
- **Ausgabe (Spalten/Filter/Export):** 3 Filter, gezählt in `WCAG 2.0 AA Violations`.
- **Akzeptanzkriterien:**
  - [ ] `meta-viewport` – Viewport deaktiviert Zoom/Skalierung (`user-scalable=no` oder `maximum-scale < 2`) (Hoch)
  - [ ] `valid-lang` – ungültiger lang-Wert an einem Element im Body (Hoch)
  - [ ] `color-contrast` – Textkontrast unter 4,5:1 (normal) bzw. 3:1 (groß ≥ 24 px oder ≥ 18,66 px fett) (Hoch)
- **Testfixture:** `a11y/wcag20aa.html` (grauer Text `#999` auf Weiß, `<span lang="xx-zz">`, Viewport `user-scalable=no`).

### TECH-255 Filtergruppe „WCAG 2.0 AAA“
- **Priorität:** P1
- **Beschreibung:** Regeln der Stufe AAA (2.0).
- **Eingaben/Konfiguration:** –
- **Ausgabe (Spalten/Filter/Export):** 3 Filter, gezählt in `WCAG 2.0 AAA Violations`.
- **Akzeptanzkriterien:**
  - [ ] `meta-refresh-no-exceptions` – verzögerter Meta Refresh jeglicher Dauer (Niedrig)
  - [ ] `identical-links-same-purpose` – Links mit gleichem zugänglichen Namen führen zu verschiedenen Zielen (Niedrig)
  - [ ] `color-contrast-enhanced` – Textkontrast unter 7:1 bzw. 4,5:1 für großen Text (Hoch)
- **Testfixture:** `a11y/wcag20aaa.html` (zwei „Mehr“-Links auf verschiedene Ziele, Text `#666` auf Weiß).

### TECH-256 Filtergruppe „WCAG 2.1 AA“
- **Priorität:** P1
- **Beschreibung:** Zusätzliche Regeln der Stufe AA aus WCAG 2.1.
- **Eingaben/Konfiguration:** –
- **Ausgabe (Spalten/Filter/Export):** 2 Filter, gezählt in `WCAG 2.1 AA Violations`.
- **Akzeptanzkriterien:**
  - [ ] `autocomplete-valid` – autocomplete-Wert ungültig oder unpassend für das Feld (Hoch)
  - [ ] `avoid-inline-spacing` – Inline-Textabstände mit `!important` nicht anpassbar (Hoch)
- **Testfixture:** `a11y/wcag21aa.html` (`<input autocomplete="banana">`, `style="letter-spacing:1px !important"`).

### TECH-257 Filtergruppe „WCAG 2.2 AA“
- **Priorität:** P1
- **Beschreibung:** Zusätzliche Regel der Stufe AA aus WCAG 2.2.
- **Eingaben/Konfiguration:** –
- **Ausgabe (Spalten/Filter/Export):** 1 Filter, gezählt in `WCAG 2.2 AA Violations`.
- **Akzeptanzkriterien:**
  - [ ] `target-size` – Touch-Ziele zu klein bzw. zu eng (Hoch)
  - [ ] Hinweis: Diese Regel ist unabhängig von TECH-233 (Mobile); beide dürfen für dieselbe Seite auslösen.
- **Testfixture:** `a11y/wcag22aa.html` (zwei 10×10-Buttons nebeneinander).

### TECH-258 Accessibility-Score und Score-Filter
- **Priorität:** P2
- **Beschreibung:** Gesamtwert 0–100 je Seite, wie ihn Lighthouse liefert (über PSI-API oder lokales Lighthouse, Integrations-Spec).
- **Eingaben/Konfiguration:** PSI-/Lighthouse-Anbindung aktiv.
- **Ausgabe (Spalten/Filter/Export):** Spalte `Accessibility Score`; Filter „Accessibility Score Poor“ (0–49), „Accessibility Score Needs Improvement“ (50–89), „Accessibility Score Good“ (90–100).
- **Akzeptanzkriterien:**
  - [ ] Score wird ganzzahlig gerundet angezeigt; Grenzwerte inklusiv wie angegeben (49 → Poor, 50 → Needs Improvement, 89 → Needs Improvement, 90 → Good).
  - [ ] Ohne PSI/Lighthouse: Spalte leer, Score-Filter leer.
- **Testfixture:** Mock der PSI-Antwort mit Scores 49, 50, 89, 90.

### TECH-259 Detailansicht „Accessibility Details“ und Exporte
- **Priorität:** P1
- **Beschreibung:** Untere Detailansicht und Bulk-Exporte. Führend für den **Inhalt** des Detail-Tabs „Accessibility Details“ (K-15: Datenfelder, Ergebnisarten, Zählregeln); Layout, Ergebnisfilter-Umschalter, Standardauswahl, Sortierung in der Anzeige und Kontextaktionen regelt UI-048 (P1).
- **Eingaben/Konfiguration:** –
- **Ausgabe (Spalten/Filter/Export):**
  - Daten je Regel der gewählten URL (linker Bereich in UI-048): `Issue` (Regel-Kurzname), `Result` (`Violation` / `Incomplete` / `Inapplicable` / `Passed`), `Guideline` (Gruppe), `Impact`, `Nodes` (Anzahl Knoten, bei Inapplicable/Passed leer), `Help URL` (Regelseite des Herstellers).
  - Daten je betroffenem Knoten einer Regel (rechter Bereich in UI-048): `Address` (bei Mehrfachauswahl mehrere URLs), `Location on Page` (CSS-Selektor), `HTML` (Ausschnitt, max. 500 Zeichen).
  - Bulk-Exporte und Report: OUT-062 („All Violations“, je WCAG-Stufe und „Incomplete“) und OUT-109 („Violations Summary“) legen fest, welche Exporte es gibt und welche Zusatzspalten sie haben; die oben definierten Feldnamen übernehmen sie exakt (Detail-Export OUT-021). Hier gilt nur die Zeilenauswahl (Kriterien unten).
- **Akzeptanzkriterien:**
  - [ ] Die Detailansicht zeigt für eine geprüfte URL genau eine Zeile je aktivierter Regel; die Anzahl der Zeilen mit `Result = Violation` ist gleich der Summe der Gruppen-Zählspalten dieser URL (TECH-251).
  - [ ] Eine Regel, die auf der Seite nicht anwendbar ist (z. B. `video-caption` ohne `<video>`), erscheint als `Inapplicable` ohne Impact.
  - [ ] Eine Regel mit unentscheidbarem Ergebnis (typisch `color-contrast` bei Text über Hintergrundbild) erscheint als `Incomplete` mit Knoten und zählt **nicht** in den Filtern.
  - [ ] Je Knoten genau eine Zeile im All-Violations-Export.
  - [ ] Summary-Report (OUT-109): `URLs` je Regel stimmt mit der Zeilenanzahl des jeweiligen Regel-Filters überein.
- **Testfixture:** `a11y/` Golden-Exporte; zusätzlich `a11y/result-types.html`: ein Bild ohne Alt (Violation `image-alt`), Text über einem Hintergrundbild (Incomplete `color-contrast`), keine `<video>` (Inapplicable `video-caption`), korrektes `lang` am html (Passed `html-has-lang`). Golden: Ergebnisart je dieser vier Regeln.

---

## Validation

### TECH-280 Strukturanalyse auf Token-Ebene (Grundlage)
- **Priorität:** P1
- **Beschreibung:** Die Validierungsfilter betrachten den **Quelltext** (Tokenizer-Ebene nach HTML5), nicht den vom Parser reparierten Baum – denn der Baum enthält immer genau ein `head` und ein `body`. Es ist keine W3C-Vollvalidierung, sondern eine Prüfung der Struktur, die Suchmaschinen beim Lesen von Head-Metadaten beeinflusst.
- **Eingaben/Konfiguration:** Rohes HTML (Original-Antwort); im JS-Modus zusätzlich keine Prüfung des gerenderten DOM (Festlegung: Validation prüft immer das Original). Läuft nur, wenn „Perform HTML Validation“ (TECH-008) an ist. Ausnahme: Die AMP-Prüfungen (TECH-163) nutzen dieselbe Token-Analyse immer, unabhängig von TECH-008.
- **Ausgabe (Spalten/Filter/Export):** Je Seite: Zählung der Start-Tags `html`, `head`, `body`; Position (Token-Index) des jeweils ersten; erstes Element-Start-Tag nach `<html>`; Liste unzulässiger Elemente im Head.
- **Akzeptanzkriterien:**
  - [ ] Tags in Kommentaren, `<script>`, `<style>`, `<template>`, `<textarea>`, `<title>` und CDATA werden nicht gezählt (Tokenizer-Zustände beachten).
  - [ ] Tag-Namen case-insensitiv.
- **Testfixture:** `tests/fixtures/tech/validation/`: Varianten unten; zusätzlich `ok.html` und `head-in-script.html` (`document.write('<head>')` in Skript – darf nicht als zweiter Head zählen).

### TECH-281 Tab „Validation“: Population und Spalten
- **Priorität:** P1
- **Beschreibung:** Übersicht der Strukturbefunde.
- **Eingaben/Konfiguration:** „Perform HTML Validation“ (TECH-008) für TECH-282–TECH-288; TECH-289/290 immer; TECH-291 nur im JS-Rendering-Modus.
- **Ausgabe (Spalten/Filter/Export):** `Address`, `Content Type`, `Status Code`, `Status`, `Indexability`, `Indexability Status`. Filter TECH-282 bis TECH-291 plus „All“. Die Referenz exportiert zusätzlich `Size`, `Transferred`, `CO2` und `Carbon Rating`; OpenSpider führt sie in diesem Tab nicht (Werte in TECH-289 … 291), der Paritätsvergleich ignoriert sie (K2-88, SQ-13f).
- **Akzeptanzkriterien:**
  - [ ] Population: alle internen URLs, jeder Status und Inhaltstyp (K2-85, SQ-03; vorher interne HTML-Seiten mit Status 200; Beleg Referenz `validation_all` auf allen Sites). Die Filter prüfen weiter nur ihre Gegenstände: TECH-282 … 289 HTML-Seiten, TECH-290 JS/CSS-Ressourcen.
  - [ ] TECH-008 aus → Filter TECH-282–288 stehen in der Übersicht mit dem Hinweis „HTML-Validierung deaktiviert“; TECH-289–291 werden normal befüllt.
- **Testfixture:** `validation/`.

### TECH-282 Filter „Invalid HTML Elements In \<head>“
- **Priorität:** P1
- **Beschreibung:** Der Head enthält ein Element, das laut HTML-Standard nicht zum Head-Inhalt gehört. Ein standardkonformer Parser deutet ein solches Element als Beginn des Seiteninhalts. Alles, was im Quelltext danach als Head-Angabe gedacht war (Canonical, Robots, hreflang …), liegt dann im Body und wird von Suchmaschinen möglicherweise nicht berücksichtigt.
- **Eingaben/Konfiguration:** –
- **Ausgabe (Spalten/Filter/Export):** Filter „Invalid HTML Elements In \<head>“; Einstufung: Warnung / Hoch. Detail: Liste der Elementnamen.
- **Akzeptanzkriterien:**
  - [ ] Erlaubt im Head: `title`, `meta`, `link`, `script`, `style`, `base`, `noscript`, `template`. Jedes andere Element-Start-Tag zwischen `<head>` und `</head>` (bzw. bis zum ersten `<body>`) → Treffer.
  - [ ] Nicht-Whitespace-Text direkt im Head → Treffer (als `#text`, Festlegung).
  - [ ] Inhalt von `<noscript>` im Head: nur `link`, `meta`, `style` erlaubt; andere (`<img>`) → Treffer.
  - [ ] Benutzerdefinierte Elemente (`<my-widget>`) → Treffer.
- **Testfixture:** `validation/invalid-head-img.html`, `/invalid-head-div.html`, `/invalid-head-text.html`, `/noscript-img.html`, `/noscript-link.html` (Negativtest).

### TECH-283 Filter „\<head> Not First In \<html> Element“
- **Priorität:** P1
- **Beschreibung:** Zwischen `<html>` und `<head>` steht bereits Seiteninhalt. Der Parser legt dann selbst einen (leeren) Head an und wechselt in den Body; der im Quelltext folgende Head wird nicht mehr als Head behandelt.
- **Eingaben/Konfiguration:** –
- **Ausgabe (Spalten/Filter/Export):** Filter „\<head> Not First In \<html> Element“; Einstufung: Warnung / Hoch.
- **Akzeptanzkriterien:**
  - [ ] Bedingung bei vorhandenem `<head>`-Start-Tag (Ausnahme bei fehlendem Head siehe nächster Punkt): Vor ihm (nach `<html>` bzw. ab Dokumentbeginn, falls `<html>` fehlt) steht ein anderes Element-Start-Tag oder Nicht-Whitespace-Text. Doctype, Kommentare und Whitespace zählen nicht. (K2-93)
  - [ ] Fehlt `<head>` ganz → zusätzlich zu TECH-284 auch dieser Filter. (K2-93)
- **Testfixture:** `validation/p-before-head.html`, `/img-before-head.html`.

### TECH-284 Filter „Missing \<head> Tag“
- **Priorität:** P1
- **Beschreibung:** Kein `<head>`-Start-Tag im Quelltext.
- **Eingaben/Konfiguration:** –
- **Ausgabe (Spalten/Filter/Export):** Filter „Missing \<head> Tag“; Einstufung: Fehler / Hoch.
- **Akzeptanzkriterien:**
  - [ ] Bedingung: Anzahl `<head>`-Start-Tags = 0.
  - [ ] Ein Dokument mit `<title>` direkt nach `<html>` ohne `<head>` → Treffer (auch wenn der Parser einen Head erzeugt).
- **Testfixture:** `validation/no-head.html`.

### TECH-285 Filter „Multiple \<head> Tags“
- **Priorität:** P1
- **Beschreibung:** Mehr als ein `<head>`-Start-Tag.
- **Eingaben/Konfiguration:** –
- **Ausgabe (Spalten/Filter/Export):** Filter „Multiple \<head> Tags“; Einstufung: Fehler / Hoch.
- **Akzeptanzkriterien:**
  - [ ] Bedingung: Anzahl `<head>`-Start-Tags ≥ 2.
  - [ ] `<header>` zählt nicht als `<head>`.
- **Testfixture:** `validation/two-heads.html`, `/header-element.html` (Negativtest).

### TECH-286 Filter „Missing \<body> Tag“
- **Priorität:** P1
- **Beschreibung:** Kein `<body>`-Start-Tag im Quelltext.
- **Eingaben/Konfiguration:** –
- **Ausgabe (Spalten/Filter/Export):** Filter „Missing \<body> Tag“; Einstufung: Fehler / Hoch.
- **Akzeptanzkriterien:**
  - [ ] Bedingung: Anzahl `<body>`-Start-Tags = 0.
  - [ ] Frameset-Dokumente (`<frameset>` statt body) → Treffer (Festlegung).
- **Testfixture:** `validation/no-body.html`.

### TECH-287 Filter „Multiple \<body> Tags“
- **Priorität:** P1
- **Beschreibung:** Mehr als ein `<body>`-Start-Tag.
- **Eingaben/Konfiguration:** –
- **Ausgabe (Spalten/Filter/Export):** Filter „Multiple \<body> Tags“; Einstufung: Fehler / Hoch.
- **Akzeptanzkriterien:**
  - [ ] Bedingung: Anzahl `<body>`-Start-Tags ≥ 2.
- **Testfixture:** `validation/two-bodies.html`.

### TECH-288 Filter „\<body> Element Preceding \<html>“
- **Priorität:** P1
- **Beschreibung:** Noch vor dem `<html>`-Start-Tag steht Seiteninhalt (ein Body-Element wie `div`, `p`, `img` oder Text). Der Parser beginnt dann sofort den Body; der später folgende Head mit seinen Metadaten zählt nicht mehr als Head. (Korrigiert: Der Filter erfasst nicht nur ein `<body>`-Tag, sondern jedes Body-Inhaltselement vor `<html>`.)
- **Eingaben/Konfiguration:** „Perform HTML Validation“ (TECH-008).
- **Ausgabe (Spalten/Filter/Export):** Filter „\<body> Element Preceding \<html>“; Einstufung: Warnung / Hoch. Detail: Name des ersten auslösenden Elements bzw. `#text`.
- **Akzeptanzkriterien:**
  - [ ] Bedingung: Das Dokument enthält ein `<html>`-Start-Tag, und **vor** dem ersten davon steht (a) ein Element-Start-Tag, das kein Head-Element ist (also nicht `title`, `meta`, `link`, `script`, `style`, `base`, `noscript`, `template`), oder (b) Nicht-Whitespace-Text. `<body>` vor `<html>` fällt unter (a).
  - [ ] Doctype, Kommentare, Whitespace und BOM vor `<html>` lösen nicht aus.
  - [ ] `<meta charset>` oder `<script>` vor `<html>` lösen **nicht** aus (Festlegung: Der Parser legt dafür einen Head an, die Metadaten bleiben wirksam).
  - [ ] Ohne `<html>`-Start-Tag im Quelltext wird der Filter nicht ausgewertet (Fehlen von `<html>` ist kein Befund dieses Tabs).
  - [ ] Tags in Kommentaren/Skripten zählen nicht (TECH-280).
- **Testfixture:** `validation/body-before-html.html` (`<body>` vor `<html>`), `/div-before-html.html`, `/img-before-html.html`, `/text-before-html.html` (Text „Hallo“ vor `<html>`), `/meta-before-html.html` (Negativtest), `/comment-before-html.html` (Negativtest).

### TECH-289 Filter „HTML Document Over 2MB“
- **Priorität:** P1
- **Beschreibung:** Sehr große HTML-Dokumente; Suchmaschinen verarbeiten nur einen begrenzten Anfang.
- **Eingaben/Konfiguration:** Grenze 2 MB (Referenzstand; frühere Versionen nutzten eine größere Grenze). Konfigurierbar (Festlegung).
- **Ausgabe (Spalten/Filter/Export):** Filter „HTML Document Over 2MB“; Einstufung: Fehler / Hoch.
- **Akzeptanzkriterien:**
  - [ ] Bedingung: unkomprimierte (nach Content-Encoding dekodierte) Größe des HTML-Body der Antwort `> 2 097 152` Bytes (2 MiB, Festlegung zur Einheit).
  - [ ] Separat geladene Ressourcen zählen nicht.
  - [ ] Download-Limit des Crawlers muss größer als die Grenze sein, sonst wird „abgeschnitten“ vermerkt und die Seite trotzdem gelistet.
- **Testfixture:** dynamisch erzeugte `validation/big-2mb-plus.html` (2 097 153 Bytes, gzip ausgeliefert) und `/big-2mb.html` (genau 2 097 152 Bytes).

### TECH-290 Filter „Resource Over 2MB“
- **Priorität:** P1
- **Beschreibung:** JavaScript- oder CSS-Dateien über der Grenze.
- **Eingaben/Konfiguration:** wie TECH-289; Ressourcen müssen gecrawlt werden (Crawl-Spec: JS/CSS prüfen).
- **Ausgabe (Spalten/Filter/Export):** Filter „Resource Over 2MB“; Einstufung: Fehler / Hoch. Gelistet: die Ressource (nicht die einbindende Seite).
- **Akzeptanzkriterien:**
  - [ ] Bedingung: interne Ressource mit Inhaltstyp JavaScript oder CSS, unkomprimierte Größe `> 2 097 152` Bytes.
  - [ ] Bilder/Fonts zählen nicht.
- **Testfixture:** `validation/big.js` (dynamisch 2,5 MiB), `/big.css`, `/big.png` (Negativtest).

### TECH-291 CO2-Schätzung und Filter „High Carbon Rating“
- **Priorität:** P2
- **Beschreibung:** Je Seite wird der CO2-Ausstoß pro Seitenaufruf nach dem Sustainable-Web-Design-Modell aus der übertragenen Datenmenge geschätzt und einer Buchstabenbewertung zugeordnet. Implementierung über eine Portierung bzw. Einbindung der CO2.js-Formeln (Apache-2.0).
- **Eingaben/Konfiguration:** Nur im JS-Rendering-Modus (dann ist die gesamte Übertragungsmenge inkl. Ressourcen bekannt; die Referenz beschränkt den Filter ebenso). Modellversion `swd-v3` (Standard, Festlegung) oder `swd-v4`. Grünes Hosting: über die Option TECH-292 (Standard aus), **nicht** fest verdrahtet (korrigiert). Berechnungsbibliothek: CO2.js in einer im Repository fixierten Version (Apache-2.0), ausgeführt in der gebündelten JS-Engine oder als 1:1-Portierung mit denselben Testvektoren.
- **Ausgabe (Spalten/Filter/Export):** Spalten (im Internal-/Validation-Kontext) `Total Transferred (Bytes)`, `CO2 (mg)`, `Carbon Rating`. Filter „High Carbon Rating“; Einstufung: Chance / Niedrig.
- **Akzeptanzkriterien:**
  - [ ] `Total Transferred (Bytes)` = Summe der über das Netz übertragenen Bytes (komprimierte Größe inkl. HTTP-Header, wie vom Renderer gemeldet) für das Dokument und alle während des Renderns bis zum Ende des AJAX-Timeouts geladenen Ressourcen, einschließlich externer Ressourcen. Aus dem Cache bediente Antworten zählen mit ihrer Originalgröße (Festlegung, damit das Ergebnis nicht von der Crawlreihenfolge abhängt).
  - [ ] `CO2 (mg)` = Ergebnis der CO2.js-Funktion „pro Seitenaufruf“ (inklusive der im Modell vorgesehenen Annahmen zu Erst- und Wiederholungsbesuchen) für die gewählte Modellversion und den Grünhosting-Schalter, umgerechnet in Milligramm, angezeigt mit 2 Nachkommastellen. Die Bewertung nutzt den ungerundeten Grammwert.
  - [ ] Bewertungsstufen (Gramm CO2e je Aufruf, Obergrenze inklusiv), übernommen aus der fixierten CO2.js-Version: `swd-v3`: A+ ≤ 0,095; A ≤ 0,186; B ≤ 0,341; C ≤ 0,493; D ≤ 0,656; E ≤ 0,846; F > 0,846. `swd-v4`: A+ ≤ 0,040; A ≤ 0,079; B ≤ 0,145; C ≤ 0,209; D ≤ 0,278; E ≤ 0,359; F > 0,359. Ein Unit-Test vergleicht diese Tabelle mit den Konstanten der gebündelten CO2.js-Version und schlägt bei Abweichung fehl.
  - [ ] Grenzwerte: Ein Grammwert genau auf der Obergrenze (z. B. 0,186 bei v3) ergibt die bessere Stufe (A).
  - [ ] Filter-Bedingung: `Carbon Rating = F`.
  - [ ] Im Text-only-Modus bleiben Spalten und Filter leer (kein Wert 0).
  - [ ] Deterministisch: gleiche Bytezahl und Optionen → gleiche Werte auf allen Plattformen (Golden mit Toleranz 0,01 mg).
- **Testfixture:** `validation/heavy.html` lädt ein dynamisch erzeugtes 6-MiB-Bild (→ F), `/light.html` (wenige KB → A+). Unit-Test mit festen Bytezahlen (z. B. 1 000 000 Bytes) gegen die Referenzwerte der CO2.js-Testsuite für v3/v4, je mit Grünhosting an und aus.

### TECH-292 Option „Green Hosting Carbon Calculation“
- **Priorität:** P2
- **Beschreibung:** Führender Eintrag für Green Hosting (CRAWL-139 verweist hierher). Läuft eine Website nachweislich mit erneuerbarer Energie, ist der Anteil des Rechenzentrums am Ausstoß kleiner. Die Option sagt der CO2-Schätzung (TECH-291), dass sie für diesen Anteil mit dem Emissionsfaktor für erneuerbare Energie rechnen soll. Ob ein Host grün ist, kann der Nutzer z. B. über die öffentliche Prüfung der Green Web Foundation klären; der Konfigurationsdialog bietet dafür nur einen Link „Hosting-Status prüfen“, der eine öffentliche Prüfseite im Systembrowser öffnet. OpenSpider fragt diesen Dienst **nicht** automatisch ab (keine externen Aufrufe ohne Nutzeraktion, Festlegung).
- **Eingaben/Konfiguration:** `green_hosting` (bool; Konfigurationsschlüssel `advanced.green_hosting`), Standard **aus** (die Referenz nennt keinen Standard, die Beschreibung legt „nur aktivieren, wenn zutreffend“ nahe); Spider-Konfiguration, Bereich „Advanced“ (wie Referenz). Gilt für alle internen URLs des Crawls (Festlegung: keine Unterscheidung je Host; externe Ressourcen werden immer als nicht grün gerechnet).
- **Ausgabe (Spalten/Filter/Export):** wirkt auf `CO2 (mg)` und `Carbon Rating`; die Einstellung wird in der Crawl-Datei, in der Crawl-Übersicht („Grünes Hosting: ja/nein“) und im Export-Kopf vermerkt.
- **Akzeptanzkriterien:**
  - [ ] Option an → Der Rechenzentrumsanteil der Dokument-Bytes und der **internen** Ressourcen-Bytes wird mit dem Grünstrom-Faktor des Modells berechnet (CO2.js-Parameter für grünes Hosting); externe Ressourcen weiter mit dem Standardfaktor. Option aus → alles mit dem Standardfaktor.
  - [ ] Für dieselbe Seite gilt `CO2 (mg)` (an) < `CO2 (mg)` (aus), sofern interne Bytes > 0.
  - [ ] Für feste Bytezahlen (intern/extern) weicht `CO2 (mg)` bei Option an und aus höchstens 0,1 % vom Ergebnis der fixierten CO2.js-Version mit denselben Parametern ab (übernommen aus CRAWL-139).
  - [ ] Umschalten erfordert keinen neuen Crawl (Neuberechnung aus `Total Transferred (Bytes)`, getrennt gespeichert nach intern/extern; Request-Zähler unverändert) und dauert < 2 s bei 10 000 URLs.
  - [ ] Gemischte Seite (1 MiB intern + 1 MiB extern): Bei Option an liegt `CO2 (mg)` zwischen dem Wert „alles grün“ und „alles mit Standardfaktor“.
  - [ ] Ohne Nutzeraktion wird kein Request an einen Green-Hosting-Prüfdienst gesendet (Netzwerkmitschnitt im Test).
  - [ ] Im Text-only-Modus ohne Wirkung (Spalten leer).
- **Testfixture:** `validation/medium.html` (ca. 800 KB intern, 200 KB externes Skript von einem zweiten Fixture-Host), `/mixed-host.html` (1 MiB intern + 1 MiB von `cdn.other.test`). Zwei Läufe (an/aus); Golden: CO2 „an“ kleiner, Rating ggf. eine Stufe besser; Unit-Test mit festen Bytezahlen gegen CO2.js-Referenzwerte mit `green=true`.

---

## Change Detection

Change Detection ist Teil des Vergleichsmodus. Auswahl der Crawls, Rollentausch, Klassifikation Added/New/Removed/Missing für alle anderen Tabs, URL-Mapping, gespeicherte Vergleiche, Vergleichs-Exporte und Auto Compare regelt **OUT-140 ff. (führend für den Compare-Ablauf)**. Hier – **führend für Change Detection (K-3)** – der Tab, der Wertänderungen einzelner Elemente zwischen zwei Crawls zeigt, auch wenn die URL dadurch keinen anderen Filter betritt oder verlässt. OUT-144, OUT-145 und CRAWL-033 verweisen hierher.

### TECH-300 Change-Detection-Konfiguration
- **Priorität:** P1
- **Beschreibung:** Auswahl der zu vergleichenden Elemente und der Schwelle für Inhaltsänderungen (Konfiguration › Compare › Change Detection). Einzige Stelle, an der Elementliste, Reihenfolge und Standardauswahl festgelegt sind.
- **Eingaben/Konfiguration:** Elemente (Mehrfachauswahl) in dieser Reihenfolge, die zugleich die Filterreihenfolge ist (gegen die öffentliche Tab-Doku der Referenz geprüft, Referenzstand Oktober 2026: genau diese 14 Filter):
  1. `Indexability` (TECH-304), 2. `Page Titles`, 3. `Meta Description`, 4. `H1` (TECH-302),
  5. `Word Count`, 6. `Crawl Depth`, 7. `Inlinks`, 8. `Unique Inlinks`, 9. `Internal Outlinks`, 10. `Unique Internal Outlinks`, 11. `External Outlinks`, 12. `Unique External Outlinks`, 13. `Structured Data Unique Types` (TECH-303),
  14. `Content` (TECH-305),
  15. `Canonical` (TECH-309), 16. `Redirect Target` (TECH-310) – OS-Erweiterung (Nutzerentscheidung 04.11.2026, **PA**): Die Elemente 1–14 und ihre Reihenfolge bleiben wie in der Referenz; die beiden Erweiterungen stehen dahinter, damit Filterreihenfolge und Golden-Exporte der Elemente 1–14 unverändert bleiben.
  
  **Standardauswahl:** Elemente 1–13, 15 und 16 an, `Content` aus (Festlegung; die öffentliche Referenz-Doku nennt keine Standardauswahl. Begründung: Elemente 1–13, 15 und 16 werden aus ohnehin gespeicherten Crawl-Daten verglichen und kosten kaum Rechenzeit; `Content` setzt gespeicherte Inhalte voraus, die standardmäßig nicht vorliegen, und ist rechenintensiv). Gespeicherte Konfigurationen mit ausdrücklicher Elementliste (Profile, Crawl-Dateien älterer Versionen) behalten ihre Liste; der Report „Crawl Changes“ (OUT-154) hängt nicht von der Auswahl ab. **Schwelle Inhaltsänderung** in %, ganzzahlig 0–100, Standard **10** (in der Referenz-Doku als Standard des Content-Filters genannt, verifiziert).
- **Ausgabe (Spalten/Filter/Export):** – (Spalten und Filter: TECH-301 ff.)
- **Akzeptanzkriterien:**
  - [ ] Registry-Test: genau 16 Elemente in obiger Reihenfolge; jedes ist einzeln abschaltbar; Standardauswahl wie oben (15 von 16, ohne `Content`).
  - [ ] Abgewählte Elemente erscheinen weder als Filter noch als Spalten. Ist kein Element aktiv, fehlt der Tab „Change Detection“ (auch im rechten Übersichtsbereich).
  - [ ] „Content“ ist nur wählbar, wenn beide Crawls den Text des Inhaltsbereichs bzw. dessen MinHash-Signatur gespeichert haben (die Referenz verlangt dafür gespeichertes HTML); sonst ist es ausgegraut mit Hinweis. Fehlen die Daten nur in einem Crawl eines bereits geöffneten Vergleichs, bleibt das Element mit Hinweis leer.
  - [ ] Konfigurationsänderung (Elemente, Schwelle) erfordert nur Neuberechnung bzw. Neuklassifikation des Vergleichs, keinen neuen Crawl; ein bereits gespeicherter Vergleich bleibt dabei unverändert (OUT-150).
  - [ ] Die Konfiguration ist Teil der Konfigurationsdatei und wird von CLI/Auto Compare (OUT-152) übernommen.
- **Testfixture:** `tests/fixtures/tech/compare/`: Fixture-Server liefert in Version `v1` und `v2` (umschaltbar per Manifest) unterschiedliche Inhalte unter denselben URLs; zwei Crawls werden erzeugt und verglichen.

### TECH-301 Tab „Change Detection“: Population und Spalten
- **Priorität:** P1
- **Beschreibung:** Letzter Tab im Vergleichsmodus.
- **Eingaben/Konfiguration:** Vergleichsmodus mit zwei Crawls (aktuell/vorher, vertauschbar).
- **Ausgabe (Spalten/Filter/Export):** `Address`, dann je gewähltem Element `<Element> (Current)`, `<Element> (Previous)`, bei Zahlen zusätzlich `<Element> Change` (absolut) und `<Element> Change %`. Elementspalten: `Indexability` (dazu `Indexability Status`, TECH-304), `Title 1`, `Meta Description 1`, `H1-1`, `Word Count`, `Crawl Depth`, `Inlinks`, `Unique Inlinks`, `Outlinks`, `Unique Outlinks`, `External Outlinks`, `Unique External Outlinks`, `Unique Types` (dazu `Unique Type Names`, TECH-303), `Content Change %`, `Canonical` (TECH-309), `Redirect Target` (TECH-310). Filter: „All“ + je Element ein Filter (TECH-302–305, TECH-309, TECH-310). Übersicht je Filter: `Current`, `Previous`, `Change` (alle drei = Anzahl geänderter URLs). Export der Ansicht nach OUT-151 mit denselben Spalten.
- **Akzeptanzkriterien:**
  - [ ] Population „All“: URLs, die in **beiden** Crawls vorkommen (nach URL-Mapping OUT-146) und analysiert wurden (interne URLs mit Antwort in beiden Crawls, die in jedem der beiden Crawls eine HTML-Seite **oder eine Weiterleitung** sind). Die Weiterleitung ist eine Erweiterung (Nutzerentscheidung 04.11.2026, TECH-310): Weiterleitungen tragen oft keinen HTML-Content-Type, und eine Seite, die zur Weiterleitung wird, soll im Vergleich bleiben. Ist die URL in einem Crawl keine HTML-Seite, gelten dort alle Elemente außer `Indexability` und `Redirect Target` als nicht erhoben und werden für diese URL nicht verglichen (wie TECH-303).
  - [ ] URLs nur in einem Crawl erscheinen hier nicht (sie sind New/Missing in den anderen Tabs).
  - [ ] `Change %` = `(Current − Previous) / Previous × 100`; bei Previous = 0 und Current > 0 → leer (Division vermeiden), Sortierung behandelt leer als +∞ (Festlegung).
- **Testfixture:** `compare/`.

### TECH-302 Elementfilter für Text-Elemente (Titel, Meta Description, H1)
- **Priorität:** P1
- **Beschreibung:** Filter „Page Titles“, „Meta Description“, „H1“: Seiten, deren erstes Element dieses Typs (Spalten `Title 1`, `Meta Description 1`, `H1-1`) sich geändert hat. Verglichen wird nur das erste Element, damit die angezeigten Spalten die Änderung immer erklären (Festlegung; eine reine Reihenfolgeänderung weiterer H1 ist keine Änderung).
- **Eingaben/Konfiguration:** –
- **Ausgabe (Spalten/Filter/Export):** Drei Filter (Information).
- **Akzeptanzkriterien:**
  - [ ] Vergleich des normalisierten Texts (Trim, Whitespace zusammenfassen), case-sensitiv: „Shop“ → „shop“ ist eine Änderung.
  - [ ] Vorhanden → fehlend und fehlend → vorhanden sind Änderungen.
  - [ ] Nur Whitespace-Unterschiede → keine Änderung.
- **Testfixture:** `compare/`: `/a` Titel geändert, `/b` Titel nur mit zusätzlichem Leerzeichen, `/c` H1 entfernt, `/d` Meta Description neu.

### TECH-303 Elementfilter für Zahlen (Wortzahl, Tiefe, Links, Typen)
- **Priorität:** P1
- **Beschreibung:** Filter „Word Count“, „Crawl Depth“, „Inlinks“, „Unique Inlinks“, „Internal Outlinks“, „Unique Internal Outlinks“, „External Outlinks“, „Unique External Outlinks“, „Structured Data Unique Types“. Zählweise: `Inlinks` = alle internen Hyperlink-Inlinks einschließlich mehrerer Links derselben Quelle, `Unique Inlinks` = Anzahl verschiedener Quellseiten; analog `Internal Outlinks`/`Unique Internal Outlinks` und `External Outlinks`/`Unique External Outlinks` (verschiedene Ziele).
- **Eingaben/Konfiguration:** –
- **Ausgabe (Spalten/Filter/Export):** Neun Filter (Information).
- **Akzeptanzkriterien:**
  - [ ] Bedingung je Filter: Current ≠ Previous (jede Differenz ≥ 1 zählt).
  - [ ] Structured Data Unique Types (Entscheidung des Architekten): verglichen werden die **Menge** der eindeutigen `@type`-Werte und deren **Anzahl**; Änderung, wenn sich eines von beiden unterscheidet. Wechsel von `Product` zu `Article` bei gleicher Anzahl ist eine Änderung; reine Reihenfolge- oder Mehrfachvorkommen-Unterschiede (`Product` zweimal statt einmal) sind keine. Die Referenz-Doku nennt nur die geänderte Anzahl; die Typ-Menge ist eine bewusste OS-Erweiterung (übernommen aus OUT-144).
  - [ ] Zur Erklärung der Änderung führt der Tab neben `Unique Types` (Anzahl) die Spalten `Unique Type Names (Current)`/`Unique Type Names (Previous)` (alphabetisch sortiert, mit `; ` getrennt; OS-Festlegung).
  - [ ] Inlinks vs. Unique Inlinks: Die Startseite verlinkt `/x` jetzt zweimal statt einmal (sonst unverändert) → `Inlinks` geändert (1 → 2), `Unique Inlinks` nicht geändert; analog für die Outlink-Paare.
  - [ ] `Change %` wie TECH-301 (Previous = 0 → leer).
  - [ ] Wert in einem Crawl nicht erhoben (z. B. strukturierte Daten aus) → Element für diese URL nicht verglichen.
- **Testfixture:** `compare/`: `/e` +30 Wörter, `/f` eine Ebene tiefer, `/g` zwei zusätzliche Inlinks, `/h` Product→Product+Breadcrumb, `/h2` Product→Article (gleiche Anzahl, gelistet), `/x` von der Startseite zusätzlich doppelt verlinkt, `/y` mit neuem externem Link.

### TECH-304 Elementfilter „Indexability“
- **Priorität:** P1
- **Beschreibung:** Seiten, deren Indexierbarkeit sich geändert hat: Wechsel zwischen indexierbar und nicht indexierbar **oder** Wechsel des Grundes der Nicht-Indexierbarkeit (Entscheidung des Architekten; die Referenz-Doku beschreibt nur den Wechsel Indexable/Non-Indexable, die Erweiterung um den Grund ist eine bewusste OS-Abweichung, damit z. B. ein neu gesetztes `noindex` auf einer bereits kanonisierten Seite sichtbar wird).
- **Eingaben/Konfiguration:** –
- **Ausgabe (Spalten/Filter/Export):** Filter „Indexability“ (Information).
- **Akzeptanzkriterien:**
  - [ ] Bedingung: `Indexability` (Current) ≠ `Indexability` (Previous) **oder** `Indexability Status` (Current) ≠ `Indexability Status` (Previous).
  - [ ] Wechsel nur des Grundes (noindex → Canonicalised) bei gleichbleibend Non-Indexable → Änderung (übernommen aus OUT-144); beide Gründe stehen in den Spalten `Indexability Status (Current)`/`Indexability Status (Previous)`.
  - [ ] Mehrere Gründe (z. B. `noindex` und `Canonicalised`) werden als Menge verglichen; nur eine geänderte Reihenfolge ist keine Änderung (OS-Festlegung).
- **Testfixture:** `compare/i` erhält in v2 `noindex` (gelistet); `/j` wechselt von noindex zu canonicalised (gelistet); `/j2` behält `noindex` unverändert (nicht gelistet).

### TECH-305 Elementfilter „Content“ (MinHash-Ähnlichkeit)
- **Priorität:** P1
- **Beschreibung:** Seiten, deren Textinhalt sich über die Schwelle hinaus geändert hat.
- **Eingaben/Konfiguration:** Schwelle (TECH-300, Standard 10 %); Inhaltsbereich wie Content-Spec.
- **Ausgabe (Spalten/Filter/Export):** Filter „Content“ (Information); Spalte `Content Change %`.
- **Akzeptanzkriterien:**
  - [ ] Verfahren: Text des Inhaltsbereichs normalisiert (Kleinbuchstaben, Whitespace zusammengefasst), Wort-Shingles der Länge 3, MinHash mit 128 Permutationen und festem Seed (Festlegung); Ähnlichkeit = geschätzte Jaccard-Ähnlichkeit; `Content Change % = (1 − Ähnlichkeit) × 100`, gerundet auf 2 Nachkommastellen. Die Klassifikation nutzt den gerundeten Wert, damit Anzeige und Filter übereinstimmen.
  - [ ] Bedingung: `Content Change % > Schwelle` (strikt; in der Referenz-Doku „mehr als 10 %“). Grenzfalltest über injizierte Ähnlichkeitswerte: 10,00 → nicht gelistet, 10,01 → gelistet.
  - [ ] Identischer Text → 0,00; vollständig anderer Text → 100,00 (± MinHash-Toleranz 3 Prozentpunkte in Tests); Hälfte des Texts ersetzt → zwischen 35 und 65.
  - [ ] Änderung der Schwelle (z. B. 10 → 40) klassifiziert ohne erneutes Crawlen aus den gespeicherten Signaturen neu.
  - [ ] Deterministisch: gleiche Eingaben liefern auf allen Plattformen denselben Wert; Golden-Test `compare/content-change.golden.csv` läuft unverändert auf macOS arm64, macOS x64 und Windows x64 (übernommen aus OUT-145).
  - [ ] Content Change ist P1 wie die übrigen Elemente (Entscheidung des Architekten; OUT-145 ist Verweis-Stub hierher).
- **Testfixture:** `compare/k` (ein Satz von 50 geändert → unter 10 %), `/l` (Hälfte des Texts ersetzt → ca. 50 %), `/m` (identisch), `/n` (ca. 30 % ersetzt → gelistet bei 10, nicht gelistet bei 40), `/o` (nur Footer geändert → 0 bei Inhaltsbereich ohne Footer).

### TECH-306 URL-Mapping zwischen den Crawls
- **Priorität:** P1
- **Verweis:** siehe OUT-146 (führend). Die Spalte `Previous Address` regelt OUT-151.

### TECH-307 Umschalter „Changed“/„Not Changed“ und Sortierung nach Änderung
- **Priorität:** P1
- **Beschreibung:** Jeder Elementfilter kann invertiert werden, und Tabellen lassen sich nach absoluter oder prozentualer Änderung sortieren.
- **Eingaben/Konfiguration:** –
- **Ausgabe (Spalten/Filter/Export):** Umschalter je Filter; Sortierung über `Change`/`Change %`-Spalten.
- **Akzeptanzkriterien:**
  - [ ] „Not Changed“ zeigt genau die URLs aus „All“, die nicht in „Changed“ sind (Summe beider = All).
  - [ ] Sortierung nach `Change` absteigend stellt die größte positive Änderung oben, nach Betrag optional (Umschalter „nach Betrag“).
  - [ ] Exporte berücksichtigen den aktiven Umschalter.
- **Testfixture:** `compare/` – Prüfung der Mengen-Summen.

### TECH-308 Automatischer Crawl-Vergleich („Auto Compare Crawls“) für geplante und CLI-Crawls
- **Priorität:** P1
- **Verweis:** siehe OUT-152 (führend). Eine Abhängigkeit von einem Speichermodus gibt es nicht (CRAWL-124).

### TECH-309 Elementfilter „Canonical“
- **Priorität:** P1
- **Beschreibung (Nutzerentscheidung 04.11.2026):** Seiten, deren maßgebliche Canonical-URL (Auswahlregel TECH-017) sich geändert hat – auch wenn die Indexierbarkeit gleich bleibt (A → B bei unverändert `Canonicalised` ist für TECH-304 unsichtbar). OS-Erweiterung (**PA**, Anhang A Punkt 37).
- **Eingaben/Konfiguration:** Element `Canonical` (TECH-300). Erhoben nur, wenn der Crawl Canonicals speichert (TECH-024); sonst für diesen Crawl nicht erhoben (wie TECH-303).
- **Ausgabe (Spalten/Filter/Export):** Filter „Canonical“ (Information); Spalten `Canonical (Current)`, `Canonical (Previous)` mit der aufgelösten absoluten URL (TECH-002), leer ohne maßgebliches Canonical.
- **Akzeptanzkriterien:**
  - [ ] Bedingung: Canonical (Current) ≠ abgebildetes Canonical (Previous); die Previous-URL wird mit dem URL-Mapping (OUT-146) abgebildet und nach TECH-002 verglichen.
  - [ ] Vorhanden → fehlend und fehlend → vorhanden sind Änderungen; Selbstreferenz → fremdes Ziel ebenso.
  - [ ] Nur Body-Canonical geändert (zählt nicht für die Auswahl, K2-86) → keine Änderung.
  - [ ] Mit Mapping-Regel Staging→Live ist das Staging-Canonical `https://staging.site.test/a` gleich dem Live-Canonical `https://www.site.test/a` → keine Änderung.
  - [ ] Ist die URL in einem der Crawls keine HTML-Seite (TECH-301), wird das Element für sie nicht verglichen.
- **Testfixture:** `tests/fixtures/out-b3-diff`: `/canon.html` zeigt in v1 auf `/canon-a.html`, in v2 auf `/canon-b.html` (gelistet, `Indexability Status` in beiden `Canonicalised`, also nicht in TECH-304); `/keep.html` ohne Canonical (nicht gelistet).

### TECH-310 Elementfilter „Redirect Target“
- **Priorität:** P1
- **Beschreibung (Nutzerentscheidung 04.11.2026):** URLs, deren Weiterleitungsziel sich geändert hat, einschließlich neuer und weggefallener Weiterleitungen. Weiterleitungsziel = aufgelöste Ziel-URL des ersten Sprungs (Spalte `Redirect URL`; HTTP 3xx, Meta-Refresh- oder HTTP-Refresh-Weiterleitung, TECH-001 `Redirected`). OS-Erweiterung (**PA**).
- **Eingaben/Konfiguration:** Element `Redirect Target` (TECH-300).
- **Ausgabe (Spalten/Filter/Export):** Filter „Redirect Target“ (Information); Spalten `Redirect Target (Current)`, `Redirect Target (Previous)`, leer ohne Weiterleitung.
- **Akzeptanzkriterien:**
  - [ ] Bedingung: Redirect Target (Current) ≠ abgebildetes Redirect Target (Previous) (URL-Mapping OUT-146, Vergleich nach TECH-002).
  - [ ] Seite → Weiterleitung und Weiterleitung → Seite sind Änderungen (leer ↔ Ziel).
  - [ ] Nur der Status ändert sich (301 → 308, gleiches Ziel) → keine Änderung (sichtbar über Response Codes und OUT-142).
  - [ ] Längere Kette mit gleichem ersten Ziel → keine Änderung dieses Elements für die Start-URL (Ketten: Report OUT-154).
- **Testfixture:** `tests/fixtures/out-b3-diff`: `/rt` leitet in v1 auf `/end1.html`, in v2 auf `/end2.html` (gelistet); `/moved.html` wird in v2 zur Weiterleitung (gelistet); `/c1` behält das Ziel `/c2` bei längerer Kette (nicht gelistet).

## Social (Nachtrag 09.11.2026)

Neuer Tab für die Vorschau-Metadaten, mit denen soziale Netzwerke und Messenger geteilte Links darstellen: Open Graph (`og:*`) und X-/Twitter-Cards (`twitter:*`). Alle Einträge dieses Abschnitts sind **OpenSpider-Festlegungen (OS-Festlegung, Nutzerentscheidung 09.11.2026)** ohne Paritätsbezug und gehören zum Meilenstein **1.x (nach 1.0)**. Paritätsvergleiche schließen Tab, Spalten und Filter aus. Die Einstufungen folgen UI-063; weil die Wirkung außerhalb der eigenen Website liegt, ist die Dringlichkeit nach `P-L2` durchweg **Niedrig**.

Begriffe: Der **wirksame Wert** einer Card-Angabe ist der `twitter:*`-Wert, ersatzweise der entsprechende Open-Graph-Wert (`twitter:title` ← `og:title`, `twitter:description` ← `og:description`, `twitter:image` ← `og:image`), weil verbreitete Plattformen so zurückfallen. Das **Social-Bild** einer Seite ist das erste `og:image` und, falls abweichend, das wirksame `twitter:image`.

### TECH-320 Social-Metadaten erfassen, Tab „Social“: Population und Spalten
- **Priorität:** P1
- **Meilenstein:** 1.x (nach 1.0)
- **Beschreibung:** Liest je Seite die `<meta>`-Elemente im `<head>` mit `property` oder `name` beginnend mit `og:` bzw. `twitter:` (Attributnamen ohne Beachtung der Groß-/Kleinschreibung; `property` und `name` werden für beide Präfixe akzeptiert, weil beide Schreibweisen verbreitet sind). Bei mehrfach vorhandenen Eigenschaften gilt der erste Wert; Strukturangaben wie `og:image:width` gehören zum unmittelbar davor stehenden `og:image`. Die Social-Bilder werden zusätzlich abgerufen (TECH-323). Meta-Elemente außerhalb des `<head>` werden nicht ausgewertet (Festlegung; Vorschau-Dienste lesen den Kopf).
- **Eingaben/Konfiguration:** Spider › Extraktion: „Social-Metadaten erfassen“ (`extraction.social`, Standard an), „Social-Bilder prüfen“ (`extraction.social_images`, Standard an). Grenzwerte unter Voreinstellungen (`social.*`, TECH-323).
- **Ausgabe (Spalten/Filter/Export):** Tab „Social“ in der Gruppe „Inhalte“ direkt nach „Structured Data“ (UI-004). Spalten: `Address`, `Indexability`, `Indexability Status`, `OG Title`, `OG Description`, `OG Type`, `OG URL`, `OG Image`, `OG Image Width`, `OG Image Height` (deklarierte Werte), `OG Site Name`, `OG Locale`, `Twitter Card`, `Twitter Title`, `Twitter Description`, `Twitter Image`, `Twitter Site`, `Social Image Status Code`, `Social Image Content Type`, `Social Image Size` (Bytes im Export, KB in der Oberfläche wie ONPAGE-171), `Social Image Dimensions (WxH)` (tatsächliche Maße). Filter TECH-321 bis TECH-323 plus „All“. Tab-Export nach OUT-020; Bulk-Export „Social: All Social Images“ (`Address`, `Image URL`, `Source` = `og`/`twitter`, `Status Code`, `Content Type`, `Size`, `Dimensions (WxH)`).
- **Akzeptanzkriterien:**
  - [ ] Population: interne HTML-Seiten mit Status 200; mit ausgeschalteter Erfassung ist der Tab leer und zeigt den Hinweis „Social-Metadaten erfassen ist aus“ (Kennzeichen „Voraussetzung fehlt“, UI-006).
  - [ ] `<meta property="og:title" content="A">` und `<meta name="og:title" content="A">` ergeben beide `OG Title` = A; `<meta name="twitter:card">` und `<meta property="twitter:card">` ebenso.
  - [ ] Zwei `og:title` → `OG Title` = erster Wert. `og:image` A mit `og:image:width` 800 und danach `og:image` B → `OG Image` = A, `OG Image Width` = 800.
  - [ ] Relative Werte in `og:url`/`og:image` werden für Spalten und Abruf nach TECH-002 aufgelöst; der Rohwert bleibt für TECH-321 erhalten.
  - [ ] HTML-Entities werden dekodiert, Leerraum am Rand entfernt; leere `content`-Werte gelten als nicht vorhanden.
  - [ ] Meta-Elemente im `<body>` bleiben unberücksichtigt.
  - [ ] Im Rendering-Modus wird das gerenderte HTML ausgewertet (wie die übrigen Kopfdaten).
  - [ ] Die Strukturdaten-Erfassung (TECH-182) bleibt unverändert: `og:`-Angaben erscheinen dort weiterhin nicht.
- **Testfixture:** `tests/fixtures/tech/social/`: `full.html` (vollständige Angaben beider Arten), `name-attr.html`, `multiple.html`, `relative.html`, `body-meta.html`, `rendered.html` (Angaben per Skript eingefügt).

### TECH-321 Open-Graph-Filter
- **Priorität:** P1
- **Meilenstein:** 1.x (nach 1.0)
- **Beschreibung:** Prüft die vier Pflichtangaben des Open-Graph-Protokolls (`og:title`, `og:type`, `og:image`, `og:url`) und die Form der URL-Angaben.
- **Eingaben/Konfiguration:** –
- **Ausgabe (Spalten/Filter/Export):** Spalte `Missing OG Properties` (kommagetrennt in der Reihenfolge title, type, image, url). Filter:
  - „Open Graph Missing“ – keine einzige `og:`-Angabe. Einstufung: Chance / Niedrig (`T-O2`, `P-L2`).
  - „Open Graph Incomplete“ – mindestens eine, aber nicht alle vier Pflichtangaben. Einstufung: Fehler / Niedrig (`T-I2`, `P-L2`).
  - „Open Graph Relative URL“ – `og:url` oder `og:image` ist nicht absolut (`http`/`https` mit Host); das Protokoll verlangt absolute Adressen. Einstufung: Fehler / Niedrig (`T-I2`, `P-L2`).
- **Akzeptanzkriterien:**
  - [ ] Seite ohne `og:` → nur „Open Graph Missing“; Seite mit allen vier → in keinem der drei Filter (sofern absolut).
  - [ ] Seite mit `og:title` und `og:image` → „Open Graph Incomplete“, `Missing OG Properties` = `type, url`.
  - [ ] `og:image` = `/img/share.jpg` → „Open Graph Relative URL“; für den Bildabruf (TECH-323) wird die aufgelöste URL verwendet. Protokollrelatives `//cdn.test/a.jpg` zählt ebenfalls als nicht absolut.
  - [ ] „Open Graph Missing“ und „Open Graph Incomplete“ schließen sich aus.
- **Testfixture:** `tests/fixtures/tech/social/og-*.html` (keine, vollständig, teilweise, relativ, protokollrelativ).

### TECH-322 Card-Filter (X/Twitter)
- **Priorität:** P1
- **Meilenstein:** 1.x (nach 1.0)
- **Beschreibung:** Prüft Art und Vollständigkeit der Card-Angaben unter Berücksichtigung der wirksamen Werte.
- **Eingaben/Konfiguration:** –
- **Ausgabe (Spalten/Filter/Export):** Spalte `Missing Card Properties`. Pflichtangaben je Card-Art (Festlegung):

  | `twitter:card` | Pflicht (wirksame Werte) |
  |---|---|
  | `summary` | Titel |
  | `summary_large_image` | Titel, Bild |
  | `player` | Titel, `twitter:player`, `twitter:player:width`, `twitter:player:height` |
  | `app` | mindestens eine `twitter:app:id:*`-Angabe |

  Filter:
  - „Twitter Card Missing“ – kein `twitter:card`. Einstufung: Warnung / Niedrig (`T-W2`, `P-L2`; die Darstellung hängt dann vom Rückfallverhalten der Plattform ab).
  - „Twitter Card Invalid Type“ – Wert nicht in {`summary`, `summary_large_image`, `player`, `app`} (Vergleich ohne Groß-/Kleinschreibung). Einstufung: Fehler / Niedrig (`T-I2`, `P-L2`).
  - „Twitter Card Incomplete“ – gültige Art, aber eine Pflichtangabe fehlt. Einstufung: Fehler / Niedrig (`T-I2`, `P-L2`).
- **Akzeptanzkriterien:**
  - [ ] `twitter:card = summary_large_image` mit `og:title` und `og:image`, ohne `twitter:title`/`twitter:image` → in keinem Filter (Rückfall).
  - [ ] `summary_large_image` ohne jedes Bild → „Twitter Card Incomplete“, `Missing Card Properties` = `image`.
  - [ ] `twitter:card = large` → nur „Twitter Card Invalid Type“ (Vollständigkeit wird bei ungültiger Art nicht bewertet).
  - [ ] Seite ohne `twitter:card`, aber mit vollständigem Open Graph → „Twitter Card Missing“.
- **Testfixture:** `tests/fixtures/tech/social/card-*.html` (je Card-Art vollständig und unvollständig, Rückfall auf og, ungültige Art, fehlende Art).

### TECH-323 Social-Bilder: Abruf, Status und Maße
- **Priorität:** P1
- **Meilenstein:** 1.x (nach 1.0)
- **Beschreibung:** Jedes Social-Bild wird abgerufen, um Status, Format, Dateigröße und tatsächliche Maße zu prüfen. Der Abruf ist ein **Hilfsabruf**: Er erscheint nicht als Zeile in Internal/External/Images und zählt nicht zu `Total URLs Encountered` (UI-061), nutzt aber User-Agent, Header, Authentifizierung, Proxy, robots.txt-Regeln und Tempogrenzen des Crawls; Weiterleitungen werden bis zur Grenze aus CRAWL-072 verfolgt. Ist dieselbe URL im Crawl bereits als Bild abgerufen worden, wird die vorhandene Antwort verwendet. Jede Bild-URL wird je Crawl höchstens einmal abgerufen. Externe Bild-URLs werden ebenfalls abgerufen, weil Social-Bilder oft auf CDNs liegen.
- **Eingaben/Konfiguration:** „Social-Bilder prüfen“ (TECH-320). Voreinstellungen (Festlegung mit Begründung):
  - `social.image_min_px` = 200 – kleinste Kantenlänge; darunter zeigen verbreitete Plattformen gar kein Vorschaubild, sondern nur Text.
  - `social.image_recommended` = 1200 × 630 – empfohlene Mindestmaße für große Vorschauen (`og:image` und `summary_large_image`); das entspricht dem verbreiteten Seitenverhältnis 1,91 : 1 und bleibt auf hochauflösenden Bildschirmen scharf. Für `summary`-Cards gilt nur die Mindestgröße.
  - `social.image_max_bytes` = 5 MiB – darüber lehnen verbreitete Plattformen Vorschaubilder ab oder laden sie nicht in angemessener Zeit.
  - Unterstützte Formate: JPEG, PNG, GIF, WebP. SVG und andere Formate werden von den meisten Vorschau-Diensten nicht dargestellt.
- **Ausgabe (Spalten/Filter/Export):** Spalten aus TECH-320. Filter:
  - „Social Image Not 200“ – Endstatus nach Weiterleitungen ≠ 200, keine Antwort oder durch robots.txt gesperrt. Einstufung: Fehler / Niedrig (`T-I1`, `P-L2`).
  - „Social Image Unsupported Format“ – Status 200, aber Inhaltstyp (nach Magic-Bytes, ersatzweise `Content-Type`) kein unterstütztes Format. Einstufung: Warnung / Niedrig (`T-W2`, `P-L2`).
  - „Social Image Below Minimum Size“ – Breite oder Höhe < `image_min_px`. Einstufung: Warnung / Niedrig (`T-W2`, `P-L2`).
  - „Social Image Below Recommended Size“ – Mindestgröße erfüllt, aber für eine große Vorschau Breite < 1200 oder Höhe < 630. Einstufung: Chance / Niedrig (`T-O1`, `P-L2`).
  - „Social Image Over 5 MB“ – Dateigröße > `image_max_bytes` (Filtername folgt dem eingestellten Wert). Einstufung: Warnung / Niedrig (`T-W2`, `P-L2`).
- **Akzeptanzkriterien:**
  - [ ] Bild mit 404 → „Social Image Not 200“; Bild mit 301 auf ein 200-Bild → nicht; durch robots.txt gesperrtes Bild → „Not 200“ mit Statustext `Blocked by Robots.txt`, kein Abruf.
  - [ ] PNG 1200×630 → in keinem Größenfilter; PNG 600×315 → „Below Recommended Size“; PNG 150×150 → nur „Below Minimum Size“.
  - [ ] `summary`-Card mit eigenem `twitter:image` 300×300 und ohne `og:image` → in keinem Größenfilter.
  - [ ] SVG mit Status 200 → „Unsupported Format“, Maße leer, kein Größenfilter.
  - [ ] Bild mit 6 MiB → „Over 5 MB“; die Datei wird höchstens bis `image_max_bytes` + 1 Byte gelesen, wenn `Content-Length` fehlt.
  - [ ] Maße werden aus den Kopfdaten des Bildformats bestimmt, nicht aus `og:image:width/height`; abweichende deklarierte Werte ändern das Ergebnis nicht.
  - [ ] Dasselbe Bild auf 100 Seiten → genau 1 Abruf (Request-Log); ein bereits als Bild gecrawltes Bild → 0 zusätzliche Abrufe.
  - [ ] `Total URLs Encountered` und die Zeilenzahl von Internal/External sind mit und ohne „Social-Bilder prüfen“ gleich.
  - [ ] „Social-Bilder prüfen“ aus → keine Bildabrufe; die fünf Filter zeigen das Kennzeichen „Voraussetzung fehlt“.
- **Testfixture:** `tests/fixtures/tech/social/images/`: `ok-1200x630.png`, `small-600x315.png`, `tiny-150x150.png`, `square-300x300.png`, `logo.svg`, `big-6mb.jpg` (generiert), `/moved.jpg` (301), `/missing.jpg` (404), robots-gesperrtes `/private/share.jpg`, externes Bild auf `cdn.social.test`.

---

## Anhang A: Offene Punkte, Festlegungen und Paritätsabweichungen (gegen Golden-Exporte prüfen)

Legende: **F** = OpenSpider-Festlegung (Referenzverhalten unbekannt, gegen Golden prüfen); **PA** = bewusste Paritätsabweichung (Referenzverhalten bekannt, OpenSpider weicht ab; Golden-Vergleich schließt aus bzw. erwartet Differenz); **K** = in dieser Überarbeitung nach Doku-Prüfung korrigiert.

1. **F – Maßgebliche Canonical-URL bei mehreren Angaben (TECH-017):** HTTP-Header vor gültigem HTML-Element im Head; Body-Canonicals ausschließlich im Problemfilter TECH-023 (K2-86). (K2-93)
2. **F – Leerer Canonical-`href` (TECH-002):** Auflösung zur eigenen URL (→ Self Referencing). Alternativ könnte die Referenz ihn ignorieren.
3. **F – Schwellwert „High External Outlinks“ (TECH-137/144):** Referenz-Standard nicht dokumentiert; Festlegung 100.
4. **F – Standardliste nichtssagender Ankertexte (TECH-144):** Referenzliste nicht öffentlich; eigene Liste (de/en), exakter Abgleich nach Normalisierung.
5. **F – „Contains JavaScript Content“ (TECH-104):** Schwelle „gerenderte Wortzahl > Original-Wortzahl“.
6. **F – `JS Word Count %` und `% of Total` (TECH-101/145):** Formeln nicht dokumentiert.
7. **F – „Non-Canonical Return Links“ (TECH-078):** Alternate Q ist canonicalisiert; im Tab wird Q gelistet. (K2-93)
8. **K – Orphan-Definition (TECH-214):** Korrigiert nach Tab-Doku und Sitemap-Tutorial: Orphans sind URLs nur aus Sitemaps **und** URLs, die nur über solche Sitemap-URLs entdeckt wurden (Folge-Orphans). **F:** Welche Kantentypen (Canonical, hreflang, Redirect, Pagination) als „im Crawl gefunden“ gelten, ist nicht dokumentiert; Festlegung: alle außer „XML Sitemap“.
9. **PA – Sitemap-hreflang-Spalten (TECH-071):** Referenz füllt sie laut Doku nur im Listenmodus; OpenSpider füllt sie immer.
10. **F – Accessibility-Zählweise (TECH-251):** Anzahl verletzter Regeln statt Anzahl Knoten.
11. **K – Mobile-Spaltenname (TECH-231):** Referenzname `Font Display` übernommen; Alias `Font Size` entfällt. Spaltenliste und Score-Darstellung (0–100) am 2026-10-13 gegen die öffentliche Tab-Doku geprüft.
12. **F – Carbon-Modellversion (TECH-291):** Welche SWD-Version die Referenz nutzt, ist nicht dokumentiert (Standard `swd-v3`). **K:** Grünes Hosting ist eine Option (TECH-292), keine feste Annahme. Die Grenzwerte stammen aus der fixierten CO2.js-Version (Anhang C).
13. **F – HTML-Größengrenze 2 MB (TECH-289):** Einheit (MB vs. MiB) nicht dokumentiert; Festlegung MiB.
14. **Lizenz schema.org-Vokabular (TECH-187):** siehe Anhang C.
15. **K/PA – Google-Rich-Result-Features (TECH-188):** FAQ wird wie in der Referenz (in einer jüngeren Version entfernt) nicht validiert → keine Abweichung. Sitelinks Searchbox (und HowTo) validiert OpenSpider nicht, obwohl die Referenz-Doku die Sitelinks Searchbox noch führt → **PA**, Ausschlussliste `assets/sd/parity-exclusions.toml`.
16. **F – Head-Zugehörigkeit bei zweitem `<head>` vor `<body>` (TECH-003):** „zählt als im Head“.
17. **K – Mobile-Prüfung im Text-only-Modus (TECH-230):** Quelle „Lokal“ ist ein lokaler Lighthouse-Lauf in eigener Chrome-Instanz (INTEG-132) und daher unabhängig vom Rendering-Modus des Crawls. Die frühere Festlegung (eigene Messskripte im eingebauten Chromium, Standardquelle `local`) entfällt; die Standardquelle legt INTEG-130 fest.
18. **F – Standardwert „Perform HTML Validation“ (TECH-008):** Referenzstandard nicht dokumentiert; OpenSpider: an. Prüfung an einer frischen Referenzinstallation (nur Beobachtung der Oberfläche, kein Code). Falls die Referenz „aus“ verwendet, ändert sich nur der Standardwert, nicht die Logik.
19. **F – „Ignore Non-Indexable URLs for Issues“ (TECH-009):** Standard (an) und Abgrenzung der betroffenen Filter. Die Referenz-Doku nennt nur eine Beispielliste. Weitere technische Filter sind in OpenSpider nicht betroffen, solange kein Referenzbeleg vorliegt. **Nachtrag Paritätslauf M1 (14.10.2026, K-56):** TECH-015 (Canonicals › Missing) und TECH-133 (Links › Pages Without Internal Outlinks) sind betroffen, weil die Referenz dort nicht indexierbare Seiten ausschließt.
20. **F – Statustext `Self Referencing Meta Refresh` (TECH-001/068):** Die Referenz nennt den genauen Indexierbarkeitsstatus nicht. **K:** Selbst-Refresh ist bei Standard nicht indexierbar (vorher fälschlich „ändert nichts“).
21. **PA – hreflang > 500 Annotationen (TECH-004/070):** Referenz meldet solche Seiten nicht; OpenSpider wertet die ersten 500 aus (`hreflang_over_limit = truncate`), Referenzverhalten über `skip`.
22. **PA – Hreflang-Tab ohne „Crawl Hreflang“ (TECH-071):** Referenz verlangt Store **und** Crawl; OpenSpider befüllt eigenseitige Filter schon mit Store.
23. **F – Bestätigungsstatus der hreflang-Detailansicht (TECH-089):** `Missing`/`Inconsistent` wie Referenz; `Self`, `Not Crawled`, `Non-200`, `Noindex`, `Non-Canonical`, `Confirmed` und die Vorrangregel sind Festlegung.
24. **F – Alias „Inconsistent Language & Region Confirmation Links“ (TECH-077; Report OUT-094):** im Review als Name der Issue-Bibliothek genannt, in der öffentlichen Tab- und Tutorial-Doku nicht gefunden. Vor dem Golden-Test bestätigen; bis dahin nur als Alias im Mapping.
25. **K/F – Live vs. Nachlauf (TECH-005):** **K:** hreflang-Filter außer „Unlinked Hreflang URLs“ sind live (Referenz-Tutorial). Sitemaps: 2 live / 5 Nachlauf (Referenz-Tutorial nennt das Verhältnis; welche zwei live sind, ist aus der Sache abgeleitet: die beiden Dateieigenschaften). **F:** Analysebereiche „Canonicals“ und „Links“ sind OpenSpider-Ergänzungen; ob die Referenz TECH-018/020/139/140/142 live berechnet, ist ungeprüft.
26. **K – Mobile-Scores (TECH-231):** Spaltenwerte sind für beide Quellen der Audit-Score nach INTEG-140 (vorher: lokale Prüfungen binär 0/100). Binär bleibt nur die DOM-Erkennung von `Plugins` (TECH-236). Aufbau des Menüs „Reports > Mobile“ führend in OUT-108 (vorher eigene Aufteilung hier).
27. **F – Accessibility-Ergebnisarten (TECH-250/259):** `passes`/`inapplicable` ohne Knoten gespeichert.
28. **K – CLI-Schalter für Auto-Vergleich (TECH-308):** TECH-308 ist ein Verweis auf OUT-152; CLI-Schalter (einschließlich `--compare-with-previous`) gehören in den CLI-Block von 06. Die frühere Bedingung „Datenbank-Speichermodus“ entfällt (OpenSpider hat nur einen disk-first-Speichermodus).
29. **PA – Spalte `Inlinks` im Links-Tab (TECH-130):** in der Referenz nicht vorhanden; bei OpenSpider standardmäßig ausgeblendet.
30. **K – „\<body> Element Preceding \<html>“ (TECH-288):** laut Issue-Beschreibung jedes Body-Inhaltselement vor `<html>`, nicht nur `<body>`. **F:** Head-Elemente vor `<html>` lösen nicht aus.
31. **K – AMP-Viewport (TECH-163):** `width=device-width` **und** `minimum-scale=1` sind Pflicht (Tab-Doku); `initial-scale=1` nur empfohlen.
32. **F – Anchors-Ansicht (TECH-146):** Standardwerte der Ausschluss-Kästchen, Groß-/Kleinschreibung und Spaltennamen; die Referenz beschreibt nur die Funktion.
33. **K – Issue-Einstufung „Unlinked Hreflang URLs“ (TECH-075):** Fehler / Mittel statt Warnung / Mittel.
34. **K – Bulk-Export-Name JavaScript (TECH-114):** „Pages With JavaScript Issues“.
35. **F – Ausnahmen beim Ausblenden durch Respect-Schalter (TECH-011/031/051):** Ausgeblendete URLs bleiben im jeweils eigenen Fachfilter sichtbar (Canonicalised, Pagination-Tab, Directives-Filter). CRAWL-091/092 beschreiben ein Ausblenden in allen Tabs; die Referenz-Doku nennt keine Ausnahme. Vor dem Golden-Test abgleichen und ggf. in CRAWL-091 ff. oder hier vereinheitlichen.
36. **F – Link-Score-Normierung (TECH-145):** logarithmisch (Standard) vs. linear; Behandlung gemischt follow/nofollow verlinkter Ziele (zählt als folgbar). Entscheidung per Benchmark.
37. **F/K – Change Detection (TECH-300–305):** **K:** Elementliste (14, Reihenfolge), Schwelle 10 % mit „mehr als“, Schwelle und Elementliste gegen die öffentliche Tab-Doku geprüft. **PA (Entscheidung des Architekten):** Indexability zählt zusätzlich den Wechsel des Grundes (TECH-304), Structured Data vergleicht Typ-Menge und Anzahl (TECH-303); die Referenz-Doku nennt nur Indexable ↔ Non-Indexable bzw. die Anzahl. **F:** Standardauswahl (1–13 an, Content aus) – die Referenz-Doku nennt keine Standardauswahl; H1-Vergleich nur des ersten Elements; Rundung `Content Change %` auf 2 Nachkommastellen. **PA (Nutzerentscheidung 04.11.2026):** Elemente 15 `Canonical` (TECH-309) und 16 `Redirect Target` (TECH-310) hinter den 14 Referenzelementen, Standard an; die Population „All“ umfasst zusätzlich Weiterleitungen (TECH-301).
38. **F/K – Mobile (TECH-230–237):** **K:** `PSI Status` nur `Success`/`Error`; nur „Content Not Sized Correctly“ ist laut Referenz-Doku lokal-exklusiv (vorher auch „Illegible Font Size“); `Mobile Alternate Link` wird unabhängig von PSI befüllt. **F:** Erkennungsregel für Mobil-Alternativen (`media` mit Breitenbedingung oder `handheld`, HTTP-Link-Header, erster Treffer); Filter = Score < 100 auch bei nicht-binären Scores. **F:** „Viewport Not Set“ (TECH-232) folgt der Lighthouse-Semantik (`initial-scale` ohne `width` gilt als gesetzt), die Referenzbeschreibung nennt nur `width`; gegen die gebündelte Lighthouse-Version und Golden-Exporte prüfen. Die früheren Sonderregeln „ohne Viewport-Meta übersprungen“ (TECH-234/235) entfallen zugunsten des Audit-Ergebnisses.
39. **K – Report- und Exportangaben (TECH-025, 036, 037, 088, 190, 219, 231, 259):** Namen und Spalten stehen nur noch in 06; hier bleibt die Zeilenauswahl. Die früheren TECH-025-Werte von `Chain Type` (`Canonical`/`Redirect`/`Mixed`) und die Spalte `Loop` entfallen zugunsten OUT-081/OUT-083. **F:** Sprunglimit der Canonical-Ketten (TECH-026) = Weiterleitungslimit statt fester 10 Glieder.

---

## Anhang B: Rubrik und Begründung der Issue-Einstufungen

**Verhältnis zu UI-063 (K-10):** Führend für die Entscheidungslogik ist UI-063 (Typ-Regeln `T-*`, Prioritäts-Regeln `P-*`, Katalog `core/issues/catalog.toml`). Die Werte dieses Anhangs sind die Katalogeinträge für die TECH-Filter; die Rubrik unten ist ihre bereichsspezifische Begründung. Zuordnung der Rubrik zu den UI-063-Codes: Art **Fehler** (a) → `T-I2` bzw. bei widersprüchlichen Signalen `T-I3`, (b) → `T-I1` bzw. `T-I3`, (c) → `T-I2`; **Warnung** → `T-W1` (gewollt mögliches Steuerungssignal) bzw. `T-W2` (abhängig von Ausführung/Interpretation); **Chance** → `T-O1`/`T-O2`. Dringlichkeit **Hoch** → `P-H1`/`P-H2`/`P-H3`, **Mittel** → `P-M1`/`P-M2`, **Niedrig** → `P-L1`/`P-L2`. Beim Anlegen des Katalogs erhält jeder Eintrag die passenden Codes; ergibt der Code eine andere Einstufung als dieser Anhang, gilt UI-063, und die Abweichung braucht ein `override_reason` (z. B. TECH-061).

Die Einstufungen sind eine eigenständige OpenSpider-Bewertung nach den folgenden Kriterien. Sie wurden je Filter aus der Wirkung auf Suchmaschinen und Nutzer abgeleitet und nicht als Tabelle aus einer fremden Quelle übernommen. Für den Paritätstest wird die Einstufung der Referenz je Filter beobachtet und **nur** dann übernommen, wenn sie mit der Rubrik vereinbar ist; sonst wird die Abweichung hier vermerkt. Stand: Paritätsabgleich je Filter ausstehend, ausgenommen TECH-075 (abgeglichen).

**Art**
- **Fehler** – (a) eine Angabe ist ungültig oder widersprüchlich und wird daher wahrscheinlich ignoriert oder falsch ausgelegt, (b) ein Verweisziel ist unbrauchbar (Statusfehler, nicht indexierbar), oder (c) eine harte Grenze eines Protokolls/Formats wird überschritten.
- **Warnung** – die Angabe ist gültig, kann aber eine ungewollte Wirkung haben. Ob ein Problem vorliegt, hängt von der Absicht ab (z. B. bewusst gesetztes noindex).
- **Chance** – kein Fehler, sondern Verbesserungspotenzial (Struktur, Effizienz, Nachhaltigkeit).

**Dringlichkeit**
- **Hoch** – wirkt direkt auf Indexierung, Kanonisierung oder auf das Einlesen der Head-Metadaten, oder macht Seiten bzw. Inhalte für Suchmaschinen unsichtbar.
- **Mittel** – wirkt auf Auffindbarkeit, Signalverteilung oder Darstellung, ohne die Indexierung unmittelbar zu verhindern.
- **Niedrig** – Best Practice, Randfall, veraltetes Merkmal oder geringe messbare Wirkung.

**Ausnahmeregel Pagination:** rel="next"/"prev" wird von großen Suchmaschinen nicht mehr als Indexierungssignal genutzt. Fehler in der Kettenlogik (Multiple, Loop, Sequence) sind deshalb höchstens **Niedrig**. Fehler, die die Auffindbarkeit der Folgeseiten selbst gefährden, sind **Hoch/Mittel**.

**Accessibility (TECH-252–257):** Art immer **Warnung**, weil automatische Prüfungen nur einen Teil der Kriterien abdecken und manuelle Prüfung nötig bleibt. Die Dringlichkeit wird aus der statischen Impact-Angabe der Regel in der gebündelten axe-Version abgeleitet: `critical`/`serious` → Hoch, `moderate` → Mittel, `minor` → Niedrig. Die Werte in den Tabellen TECH-252–257 sind aus dieser Ableitung erzeugt. Ein Test vergleicht sie bei jedem axe-Update mit den axe-Metadaten; bei Abweichung gilt die Ableitung.

| ID | Filter | Einstufung | Begründung (Rubrik) |
|---|---|---|---|
| TECH-014 | Canonicalised | Warnung / Hoch | Gültig und oft gewollt (Warnung); macht die Seite nicht indexierbar (Hoch). |
| TECH-015 | Missing (Canonical) | Warnung / Mittel | Kein Fehler, aber Suchmaschine wählt die kanonische Variante selbst; Duplikat-Signale werden verteilt (Mittel). |
| TECH-016 | Multiple | Warnung / Niedrig | Bei identischen Werten wirkungslos, aber fehleranfällig (Niedrig). |
| TECH-017 | Multiple Conflicting | Fehler / Hoch | Widersprüchlich (a) → Angabe wird wahrscheinlich ignoriert; Kanonisierung betroffen (Hoch). |
| TECH-018 | Non-Indexable Canonical | Fehler / Hoch | Verweisziel unbrauchbar (b); Kanonisierung läuft ins Leere (Hoch). |
| TECH-019 | Canonical Is Relative | Warnung / Niedrig | Gültig, aber anfällig für Fehlauflösung (Niedrig). |
| TECH-020 | Unlinked | Warnung / Mittel | Gültig; Ziel ohne Linksignal, Auffindbarkeit (Mittel). |
| TECH-021 | Invalid Attribute In Annotation | Fehler / Hoch | Angabe kann wegen Zusatzattributen ignoriert werden (a); Kanonisierung (Hoch). |
| TECH-022 | Contains Fragment URL | Fehler / Hoch | Ungültige Form für eine kanonische Adresse (a); Kanonisierung (Hoch). |
| TECH-023 | Outside \<head> (Canonical) | Fehler / Hoch | Außerhalb des Heads wird die Angabe ignoriert (a); Kanonisierung (Hoch). |
| TECH-035 | Pagination URL Not In Anchor Tag | Fehler / Hoch | Folgeseiten ohne Hyperlink sind nicht auffindbar (Ausnahmeregel Pagination: Auffindbarkeit gefährdet → Hoch). |
| TECH-036 | Non-200 Pagination URLs | Fehler / Hoch | Verweisziel unbrauchbar (b); Folgeseiten fehlen (Hoch). |
| TECH-037 | Unlinked Pagination URLs | Fehler / Mittel | Folgeseiten nur über Link-Element erreichbar (b); Auffindbarkeit (Mittel). |
| TECH-038 | Non-Indexable (Pagination) | Warnung / Hoch | Oft gewollt (Warnung); Folgeseiten fallen aus dem Index (Hoch). |
| TECH-039 | Multiple Pagination URLs | Fehler / Niedrig | Widersprüchlich (a); nur Kettenlogik (Ausnahmeregel → Niedrig). |
| TECH-040 | Pagination Loop | Fehler / Niedrig | Widersprüchlich (a); Kettenlogik (Niedrig). |
| TECH-041 | Sequence Error | Fehler / Niedrig | Widersprüchlich (a); Kettenlogik (Niedrig). |
| TECH-053 | Noindex | Warnung / Hoch | Gültig, oft gewollt; schließt die Seite vom Index aus (Hoch). |
| TECH-055 | Nofollow | Warnung / Hoch | Gültig; unterbindet Linksignale der ganzen Seite (Hoch). |
| TECH-056 | None | Warnung / Hoch | Wie noindex + nofollow (Hoch). |
| TECH-058 | NoSnippet | Warnung / Niedrig | Gültig; betrifft nur die Darstellung im Suchergebnis (Niedrig). |
| TECH-060 | NoODP / NoYDIR | Warnung / Niedrig | Veraltet und wirkungslos (Niedrig). |
| TECH-061 | NoImageIndex | Fehler / Niedrig | Häufig versehentlich gesetzt und entzieht Bildern die Indexierung über die Seite (als Fehler markiert, damit es auffällt); Wirkung auf die Seite selbst gering (Niedrig). Abweichung von der Rubrik (eigentlich Warnung) bewusst, Paritätsabgleich ausstehend. |
| TECH-062 | NoTranslate | Warnung / Niedrig | Gültig; betrifft nur Zusatzfunktion im Suchergebnis (Niedrig). |
| TECH-063 | Unavailable_After | Warnung / Mittel | Gültig; Seite verschwindet zeitgesteuert, oft vergessen (Mittel). |
| TECH-065 | Outside \<head> (Meta Robots) | Fehler / Hoch | Angabe außerhalb des Heads, Wirkung unsicher (a); Indexierung (Hoch). |
| TECH-068 | (Status Self Referencing Meta Refresh) | – | Kein eigener Filter; wirkt über die Indexierbarkeit. |
| TECH-074 | Non-200 Hreflang URLs | Fehler / Hoch | Verweisziel unbrauchbar (b); Sprachzuordnung bricht (Hoch). |
| TECH-075 | Unlinked Hreflang URLs | Fehler / Mittel | Alternate nur über Annotation erreichbar: deutet auf eine fehlerhafte Annotation oder Lücke in der Verlinkung (b); Auffindbarkeit (Mittel). Abgeglichen mit der Referenz. |
| TECH-076 | Missing Return Links | Fehler / Hoch | Ohne Gegenseitigkeit wird die Annotation ignoriert (a); Sprachzuordnung (Hoch). |
| TECH-077 | Inconsistent Language & Region Return Links | Fehler / Hoch | Widersprüchliche Codes (a); Sprachzuordnung (Hoch). |
| TECH-078 | Non-Canonical Return Links | Fehler / Hoch | Verweis auf nicht-kanonische Variante wird ignoriert (b); Hoch. |
| TECH-079 | Noindex Return Links | Fehler / Hoch | Verweisziel nicht indexierbar (b); Hoch. |
| TECH-080 | Incorrect Language & Region Codes | Fehler / Hoch | Ungültiger Code wird ignoriert (a); Hoch. |
| TECH-081 | Multiple Entries | Fehler / Hoch | Widersprüchliche Zuordnung (a); Hoch. |
| TECH-082 | Missing Self Reference | Warnung / Niedrig | Empfohlen, aber nicht zwingend (Niedrig). |
| TECH-083 | Not Using Canonical | Fehler / Hoch | Annotation zeigt auf nicht-kanonische Variante (b); Hoch. |
| TECH-084 | Missing X-Default | Warnung / Niedrig | Optionales Merkmal (Niedrig). |
| TECH-086 | Outside \<head> (Hreflang) | Fehler / Hoch | Angabe außerhalb des Heads wird ignoriert (a); Hoch. |
| TECH-090 | Self Reference Only | Warnung / Niedrig | Wirkungslos, aber unschädlich und bei einsprachigen Vorlagen gewollt möglich; Hinweis auf fehlende Alternates (Niedrig). Nachtrag 09.11.2026. |
| TECH-091 | Hreflang Language Differs From HTML Lang | Warnung / Mittel | Beide Angaben gültig, eine davon vermutlich falsch; kann eine falsche Sprachversion im Suchergebnis begünstigen (Mittel). Nachtrag 09.11.2026. |
| TECH-102 | Pages With Blocked Resources | Warnung / Hoch | Kann gewollt sein; unvollständiges Rendering kann Inhalte unsichtbar machen (Hoch). |
| TECH-103 | Contains JavaScript Links | Warnung / Mittel | Gültig; Entdeckung hängt vom Rendern ab (Mittel). |
| TECH-104 | Contains JavaScript Content | Warnung / Mittel | Gültig; Inhalt hängt vom Rendern ab (Mittel). |
| TECH-105 | Noindex Only In Original HTML | Fehler / Hoch | Widerspruch zwischen Fassungen (a); Indexierung (Hoch). |
| TECH-106 | Nofollow Only In Original HTML | Fehler / Hoch | Widerspruch (a); Linksignale der Seite (Hoch). |
| TECH-107 | Canonical Only In Rendered HTML | Warnung / Niedrig | Wird in der Regel nach dem Rendern berücksichtigt; geringes Risiko (Niedrig). |
| TECH-108 | Canonical Mismatch | Fehler / Hoch | Widerspruch (a); Kanonisierung (Hoch). |
| TECH-109–111 | … Only In Rendered HTML / … Updated By JavaScript | Warnung / Mittel | Gültig; Darstellung/Relevanzsignal hängt vom Rendern ab (Mittel). |
| TECH-112 | Uses Old AJAX Crawling Scheme URLs | Fehler / Mittel | Abgekündigtes Schema, Inhalte möglicherweise nicht erreichbar (a); Auffindbarkeit (Mittel). |
| TECH-113 | … Meta Fragment Tag | Fehler / Mittel | Wie TECH-112. |
| TECH-114 | Pages With JavaScript Errors | Warnung / Niedrig | Fehler im Skript, Wirkung auf SEO meist gering (Niedrig). |
| TECH-132 | Pages With High Crawl Depth | Chance / Mittel | Strukturverbesserung (Chance); Signalverteilung (Mittel). |
| TECH-133 | Pages Without Internal Outlinks | Warnung / Hoch | Kann gewollt sein; Sackgasse unterbricht Signalfluss und Entdeckung (Hoch). |
| TECH-134 | Internal Nofollow Outlinks | Warnung / Niedrig | Gültig; geringe Wirkung (Niedrig). |
| TECH-135 | Internal Outlinks With No Anchor Text | Chance / Niedrig | Verbesserungspotenzial (Niedrig). |
| TECH-136 | Non-Descriptive Anchor Text In Internal Outlinks | Chance / Niedrig | Verbesserungspotenzial (Niedrig). |
| TECH-147 | Pages With Empty Anchor Tags | Chance / Niedrig | Wie TECH-135: Verbesserungspotenzial ohne Fehlfunktion (Niedrig). Nachtrag 09.11.2026. |
| TECH-137 | Pages With High External Outlinks | Warnung / Niedrig | Kann gewollt sein; geringe Wirkung (Niedrig). |
| TECH-138 | Pages With High Internal Outlinks | Warnung / Niedrig | Kann gewollt sein; Signalverdünnung gering (Niedrig). |
| TECH-139 | Follow & Nofollow Internal Inlinks To Page | Warnung / Niedrig | Inkonsistente, aber gültige Verlinkung (Niedrig). |
| TECH-140 | Internal Nofollow Inlinks Only | Warnung / Niedrig | Gültig; Seite ohne Linksignal (Niedrig, da meist gewollt). |
| TECH-141 | Outlinks To Localhost | Fehler / Hoch | Verweisziel für jeden Besucher unbrauchbar (b); deutet auf fehlerhafte Veröffentlichung (Hoch). |
| TECH-142 | Non-Indexable Page Inlinks Only | Warnung / Hoch | Gültig; Seite erhält praktisch keine Linksignale (Hoch). |
| TECH-143 | Pages With Uncrawlable Internal Outlinks | Warnung / Hoch | Funktioniert für Nutzer, aber Ziele werden ggf. nicht entdeckt (Hoch). |
| TECH-162 | AMP: Non-200 Response, Missing Non-AMP Return Link, Missing Canonical To Non-AMP, Non-Indexable Canonical | Fehler / Hoch | Ziel unbrauchbar oder Zuordnung AMP↔regulär gebrochen (a/b); Hoch. |
| TECH-162 | AMP: Indexable | Warnung / Hoch | Kann gewollt sein (eigenständige AMP-Seite); sonst Duplikat im Index (Hoch). |
| TECH-163 | AMP-Validierung (11 Filter) | Fehler / Hoch | Ungültiges AMP wird nicht als AMP ausgeliefert (a); Hoch. |
| TECH-184 | Missing Structured Data | Chance / Niedrig | Verbesserungspotenzial (Niedrig). |
| TECH-186 | Parse Errors | Fehler / Hoch | Daten nicht lesbar (a); Rich Results entfallen (Hoch). |
| TECH-187 | Validation Errors | Fehler / Hoch | Ungültige Typen/Eigenschaften werden ignoriert (a); Hoch. |
| TECH-187 | Validation Warnings | Chance / Niedrig | Veraltetes Vokabular, Umstellung empfohlen (Niedrig). |
| TECH-188 | Rich Result Validation Errors | Fehler / Hoch | Feature-Anforderung verletzt, Rich Result entfällt (a); Hoch. |
| TECH-188 | Rich Result Validation Warnings | Chance / Niedrig | Empfohlene Angabe fehlt (Niedrig). |
| TECH-213 | URLs Not In Sitemap | Fehler / Mittel | Sitemap unvollständig (a); Auffindbarkeit (Mittel). |
| TECH-214 | Orphan URLs | Fehler / Mittel | Seiten außerhalb der Linkstruktur (b); Auffindbarkeit (Mittel). |
| TECH-215 | Non-Indexable URLs In Sitemap | Fehler / Mittel | Widerspruch zwischen Sitemap und Seite (a); Signalqualität (Mittel). |
| TECH-216 | URLs In Multiple Sitemaps | Warnung / Niedrig | Gültig, aber unnötig (Niedrig). |
| TECH-217 | XML Sitemap With Over 50k URLs | Fehler / Hoch | Harte Protokollgrenze (c); Datei kann verworfen werden (Hoch). |
| TECH-218 | XML Sitemap With Over 50MB | Fehler / Hoch | Harte Protokollgrenze (c); Hoch. |
| TECH-232 | Viewport Not Set | Fehler / Hoch | Mobile Darstellung grundsätzlich falsch (a); mobile Nutzbarkeit (Hoch). |
| TECH-233 | Target Size | Fehler / Mittel | Bedienbarkeit eingeschränkt (a); Darstellung/Nutzung (Mittel). |
| TECH-234 | Content Not Sized Correctly | Fehler / Hoch | Horizontales Scrollen, Layout falsch (a); Hoch. |
| TECH-235 | Illegible Font Size | Fehler / Hoch | Text praktisch unlesbar (a); Hoch. |
| TECH-236 | Unsupported Plugins | Fehler / Hoch | Inhalt wird gar nicht dargestellt/indexiert (a); Hoch. |
| TECH-237 | Mobile Alternate Link | Warnung / Hoch | Gültig, aber getrennte Mobil-URLs verdoppeln den Pflegeaufwand und riskieren Inkonsistenz (Hoch). |
| TECH-252–257 | Accessibility-Regeln | Warnung / nach axe-Impact | Siehe Accessibility-Regel oben. |
| TECH-282 | Invalid HTML Elements In \<head> | Warnung / Hoch | Seite funktioniert, aber Head endet zu früh (Warnung); Head-Metadaten betroffen (Hoch). |
| TECH-283 | \<head> Not First In \<html> Element | Warnung / Hoch | Wie TECH-282. |
| TECH-284 | Missing \<head> Tag | Fehler / Hoch | Strukturfehler (a); Metadaten-Zuordnung unsicher (Hoch). |
| TECH-285 | Multiple \<head> Tags | Fehler / Hoch | Strukturfehler (a); Hoch. |
| TECH-286 | Missing \<body> Tag | Fehler / Hoch | Strukturfehler (a); Hoch. |
| TECH-287 | Multiple \<body> Tags | Fehler / Hoch | Strukturfehler (a); Hoch. |
| TECH-288 | \<body> Element Preceding \<html> | Warnung / Hoch | Wie TECH-282. |
| TECH-289 | HTML Document Over 2MB | Fehler / Hoch | Grenze der Verarbeitung durch Suchmaschinen (c); Inhalte am Ende gehen verloren (Hoch). |
| TECH-290 | Resource Over 2MB | Fehler / Hoch | Wie TECH-289 für Ressourcen; Rendering unvollständig (Hoch). |
| TECH-291 | High Carbon Rating | Chance / Niedrig | Nachhaltigkeit/Effizienz (Chance); keine direkte SEO-Wirkung (Niedrig). |
| TECH-321 | Open Graph Missing | Chance / Niedrig | Vorschau beim Teilen verbesserbar; Wirkung außerhalb der Website (Niedrig). Nachtrag 09.11.2026. |
| TECH-321 | Open Graph Incomplete, Open Graph Relative URL | Fehler / Niedrig | Pflichtangabe fehlt bzw. Form ungültig (a); Wirkung außerhalb der Website (Niedrig). |
| TECH-322 | Twitter Card Missing | Warnung / Niedrig | Darstellung hängt vom Rückfallverhalten der Plattform ab (Niedrig). |
| TECH-322 | Twitter Card Invalid Type, Twitter Card Incomplete | Fehler / Niedrig | Ungültige Art bzw. Pflichtangabe fehlt (a); Niedrig. |
| TECH-323 | Social Image Not 200 | Fehler / Niedrig | Verweisziel unbrauchbar (b); Vorschau ohne Bild (Niedrig). |
| TECH-323 | Social Image Unsupported Format, Below Minimum Size, Over 5 MB | Warnung / Niedrig | Bild vorhanden, Darstellung hängt von der Plattform ab (Niedrig). |
| TECH-323 | Social Image Below Recommended Size | Chance / Niedrig | Empfehlungsbereich unterschritten (Niedrig). |

---

## Anhang C: Drittquellen, Lizenzen und Attribution

| Quelle | Verwendung | Lizenz | Pflichten / offene Prüfung |
|---|---|---|---|
| axe-core (Deque Systems) | Accessibility-Prüfung TECH-250–259; Regel-IDs, Impact-Metadaten | MPL-2.0 | Unverändertes Bundle als eigene Datei; Lizenztext und Copyright-Hinweis in `THIRD_PARTY_NOTICES`. Änderungen an axe-Dateien müssen als Quelltext unter MPL-2.0 verfügbar sein (OpenSpider ändert sie nicht). Regel-Kurzbeschreibungen in TECH-252–257 sind eigene Texte; axe-Meldungen werden nur zur Laufzeit angezeigt (Feld `axe_message`) und nicht in die Spec oder in eigene Texte übernommen. |
| CO2.js (Green Web Foundation) | CO2-Formeln und Rating-Grenzen TECH-291/292 | Apache-2.0 | Version fixieren; NOTICE-Datei übernehmen; Attribution in `THIRD_PARTY_NOTICES`. Die Rating-Grenzen werden aus CO2.js bezogen (nicht von der Website abgeschrieben); Unit-Test vergleicht sie. **Offen:** Lizenz der Modellbeschreibung auf sustainablewebdesign.org (Quelle der Rating-Idee) prüfen; die Spec zitiert dort keinen Text, nur die über CO2.js bezogenen Zahlenwerte und die Quellenangabe „Sustainable Web Design Model“. |
| schema.org-Vokabular | Validierung TECH-187 | CC BY-SA 3.0 | Separates Datenasset mit Quellenangabe und Lizenzhinweis; Weitergabe des Assets unter gleicher Lizenz. Nicht von `cargo deny` abgedeckt → manuelle Lizenzprüfung im Release-Prozess. |
| Google-Doku zu Rich Results | Regeldatei TECH-188 | Doku-Texte CC BY 4.0, Code-Beispiele Apache-2.0 (Google-Developers-Standard) | Regeldatei in eigenen Worten und als strukturierte Pflicht/Empfohlen-Listen; Quellenangabe je Feature (URL). |
| AMP-Validator (AMP Project) | TECH-163 | Apache-2.0 | Version fixieren; NOTICE übernehmen. |
| ISO 639-1 / 3166-1 / 15924 Codelisten | TECH-072 | Codes frei verwendbar; Bezug über IANA Language Subtag Registry | Datenasset mit Quellen- und Datumsangabe. |
| Referenzprodukt (Doku, Tutorials, Release-Notes) | Funktionsumfang, Namen als Funktionsbezeichner, Paritätsabgleich | urheberrechtlich geschützt | Keine Textübernahme; Namen nur als funktionale Bezeichner (Kopf dieses Dokuments); Einstufungen eigenständig (Anhang B); Beschreibungen abstrahiert. |
