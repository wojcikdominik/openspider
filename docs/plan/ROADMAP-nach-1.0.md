# OpenSpider – Roadmap nach Release 1.0

Sammelstelle für Funktionen, die bewusst **nach 1.0** kommen. Ausspezifiziert wird jeweils erst bei der Planung des Meilensteins (Opus), Abnahme durch Fable.

| # | Thema | Quelle | Spec / Stand |
|---|---|---|---|
| R1 | KI-Funktionen (Klassifizierung, Optimierungsvorschläge, Frag den Crawl, AI-Search-Sichtbarkeit) | Nutzer 13.10. | `docs/spec/08-ai.md` – M4 |
| R2 | DataForSEO (Keywords, Backlinks, SERP, LLM Mentions) | Nutzer 13.10. | `docs/spec/09-dataforseo.md` – M5 |
| R3 | Fokussierter Crawl nach Seitentyp („nur Produktseiten“) | Nutzer 16.10. | `docs/spec/10-agent-interface.md` AGENT-060–062 – mit M4 |
| R4a | Server-Betrieb: OpenSpider als Crawl-Engine von Autonomo SEO (Cloud-Variante unter evnxt), Etappen E0–E3 | Nutzer 14./16.10. | `docs/plan/autonomo-abgleich.md` – **parallel zu M2/M3** (Go 16.10.) |
| R4b | Verbindung Desktop ↔ Cloud (E4) | Nutzer 14.10. | nach 1.0 |
| R5 | Mobile App als Viewer/Steuerung für Crawls auf Desktop/Server (Tauri iOS/Android) | Nutzer 15.10. | setzt R4a/R4b voraus |
| R6 | Monitoring-Modus (siehe Abschnitt Audisto) | Abgleich Audisto 16.10. | in evnxt/Autonomo (E5), setzt R4a voraus |
| R8 | Logfile-Analyse lokal (Bot-Verifizierung, Abgleich mit Crawl) | Abgleich Autonomo 16.10. | Kandidat für 1.x; Server-Zulauf bleibt Cloud |
| R9 | Crawl-Qualität aus Autonomo: Indexierungsregeln mit „gewollt“-Ausnahmen, Fallenwächter, Soft-404-Probe | Abgleich Autonomo 16.10. | Kandidat für 1.x |
| R7 | Erweiterte Struktur- und Linkanalysen (siehe Abschnitt Audisto) | Abgleich Audisto 16.10. | Kandidat für 1.x |
| R10 | Teams: Crawls im Team teilen – Stufe 1 Team-Ordner (frei), Stufe 2 OpenSpider Teams über evnxt-Account (bezahlt); gecrawlt wird weiter lokal | Nutzer 09.11. | `docs/spec/11-teams.md` (Entwurf) |
| R11 | Funktionsideen 1.x (JSON-Profile, Crawl-Diagnose per MCP, eigene Dashboards, konfigurierbare Issues, Social-Tab, Markdown u. a.) | Nutzer 09.11. | ausspezifiziert, siehe Abschnitt „Nachtrag 09.11.2026“ unten |

## Nachtrag 09.11.2026: Funktionsideen 1.x

Nutzerentscheidung 09.11.2026: 16 Funktionsideen werden als OpenSpider-Festlegungen in die Spec aufgenommen (`docs/spec/README.md`, Meilensteinzuordnung). Meilenstein ist „1.x (nach 1.0)“, nur INTEG-190 gehört zu M4. Nicht aufgenommen: Dateinamen-Muster für Exporte (Nutzerentscheidung). Aufwand grob: klein ≈ ein Paket von wenigen Tagen, mittel ≈ ein bis zwei Wochen, groß ≈ mehrere Pakete.

| ID | Titel | Priorität | Aufwand |
|---|---|---|---|
| OUT-288 | Konfigurationsprofile auch als JSON | P1 | klein |
| AGENT-022 | Crawl-Diagnose (MCP-Werkzeug, Klasse `read`) | P1 | mittel |
| UI-150 | Eigene Dashboards | P2 | groß |
| UI-151 | Konfigurierbare Issues (ersetzt den P2-Teil von UI-063) | P1 | mittel |
| INTEG-085 | Links nach Position nicht verfolgen | P1 | mittel |
| CRAWL-149 | Host-Pause bei Nichtantwort | P1 | mittel |
| TECH-320 | Social-Metadaten erfassen, Tab „Social“ | P1 | mittel |
| TECH-321 | Open-Graph-Filter | P1 | klein |
| TECH-322 | Card-Filter (X/Twitter) | P1 | klein |
| TECH-323 | Social-Bilder: Abruf, Status und Maße | P1 | mittel |
| ONPAGE-182 | Image-Filter Missing Title Attribute | P2 | klein |
| TECH-090 | Filter „Self Reference Only“ (hreflang) | P1 | klein |
| TECH-091 | Filter „Hreflang Language Differs From HTML Lang“ | P1 | klein |
| TECH-147 | Filter „Pages With Empty Anchor Tags“ | P1 | klein |
| OUT-286 | Geplante und per CLI gestartete Läufe pausieren und fortsetzen | P1 | groß |
| OUT-287 | CLI-Verwaltung von Crawls und Projekten | P1 | mittel |
| CRAWL-148 | Relative Sitemap-Angaben bei manueller Eingabe | P1 | klein |
| OUT-289 | Seiten als Markdown: Ansicht, Bulk-Export und MCP | P1 | mittel |
| UI-149 | Suche in der Overview | P1 | klein |
| CRAWL-150 | Rendering: Warten auf Netzwerkruhe | P1 | mittel |
| INTEG-190 | Prompts auch bei leerer Eingabe senden (M4) | P2 | klein |
| UI-152 | Weitere Sprachen: Türkisch und Niederländisch (Community) | P2 | klein (Übersetzung extern) |

Entscheidungen des Nutzers vom 09.11.2026 (alle Vorschläge bestätigt):
1. **CRAWL-150:** „Netzwerkruhe“ vorerst nur Option; Standard bleibt die feste Wartezeit bis zu einem Vergleich auf realen Websites.
2. **OUT-286 / AGENT-020:** Die MCP-Werkzeuge `pause_crawl`/`resume_crawl` dürfen fremde Crawls über den neuen Steuerkanal steuern, nur mit freigegebener Klasse `crawl`.
3. **OUT-286:** Höchstdauer einer Pause bei geplanten und CLI-Läufen 24 h; danach beendet sich der Prozess, der Crawl bleibt pausiert.
4. **INTEG-190:** „Auch bei leerer Eingabe senden“ gilt nur für die Prompts aus 04, nicht für die Aufgaben aus 08.
5. **ONPAGE-182:** „Bild ohne title“ erscheint ab Werk nicht im Issues-Tab (Katalogfeld `default_enabled = false`, einschaltbar über UI-151).
6. **TECH-323:** Grenzwerte für Social-Bilder bestätigt: mindestens 200 × 200 px, empfohlen 1200 × 630 px, höchstens 5 MiB.
7. **CRAWL-149:** Standardwerte bestätigt (60 s ohne Antwort und 5 Fehlschläge, Höchstdauer 30 min); Abrufe aus dem Erkennungsfenster wieder eingereiht statt als `No Response` gespeichert.
8. **OUT-287:** Die App-Verwaltung erhält dabei die bisher fehlende Aktion „Projekt löschen“ (nur leere Projekte bzw. mit ausdrücklicher Bestätigung samt Crawls) – bestätigt.
9. **AGENT-022:** Die Crawl-Diagnose wird später auch in der App sichtbar (z. B. Hilfe → Crawl-Diagnose), gleiche Kernfunktion.

## Geschäftsmodell (Entscheidung Nutzer 16.10.2026): Open Core

- **OpenSpider bleibt vollständig autark und frei** (Apache-2.0): kein Account, kein Online-Zwang außer zum Crawlen, keine Pflicht-Telemetrie, keine künstlichen Sperren. Alle Crawl-, Analyse-, Export-, CLI- und MCP-Funktionen sind ohne evnxt nutzbar.
- **KI und externe Daten über zwei gleichwertige Bezugswege:**
  1. *Eigene Schlüssel / lokal* – Claude, OpenAI, OpenRouter mit eigenem API-Schlüssel oder Ollama lokal; DataForSEO mit eigenem Konto. Kostenlos in OpenSpider, Abrechnung direkt beim Anbieter.
  2. *evnxt-Account* – Anmeldung in OpenSpider, KI und DataForSEO ohne eigene Schlüssel, Abrechnung über evnxt-Credits oder Abo (Marge auf Modell- und Datenkosten). Budget und Kostenschätzung von OpenSpider gelten unverändert.
- **evnxt verdient an Komfort und Cloud, nicht an Sperren:** Cloud-Crawls über Autonomo (auch bei ausgeschaltetem Rechner), Monitoring und Alarme (R6), Archiv/Historie, Team-Zugang, Laien-Audit mit KI-Maßnahmenplan, Desktop↔Cloud-Abgleich (R4b).
- Folge für die Spec: evnxt ist ein weiterer Anbieter in `os-ai` bzw. Datenquelle in `os-integrations` (08-ai.md Abschnitt „Bezugsweg evnxt“, 09-dataforseo.md D4S-200), kein Sonderpfad im Kern.

## Abgleich mit Audisto (16.10.2026)

Audisto ist ein Cloud-Crawler mit Schwerpunkt Monitoring und Linkgraph-Analyse. Was OpenSpider bereits abdeckt: Indexierbarkeit/Direktiven-Konflikte, hreflang-Gruppen, Canonical-Analyse, Sitemap-Validierung, Redirect-Ketten, Rendering-Vergleich (Original vs. gerendert), Web Vitals (PSI), Link Score, Segmente, Duplikate/Thin Content, inkonsistente nofollow-Verlinkung (TECH-139), Crawl-Vergleich, geplante Crawls, Custom Search/Extraction (Regex/XPath), URL-Umschreibung, API über MCP.

### Lücken → R6 Monitoring-Modus (größter Unterschied)
- Regelbasierte Tests je Seitentyp/Bereich (UND/ODER, Regex/XPath/Header/Inhalt), auch für neu entstehende Seiten; No-Code-Editor plus Konfigurationsdatei.
- Alarme mit Schwellen und Routing je Test: E-Mail, Slack, Microsoft Teams, Jira, Webhook; kritisch sofort, sonst Zusammenfassung.
- KPI-Verlauf über viele Crawls (nicht nur Zweier-Vergleich) mit Vorfalls-Dashboard und Drilldown.
- Datei-Fingerprints im Zeitverlauf (robots.txt, Sitemaps, JS, Bilder) und Archiv von HTML/Ressourcen je Crawl zur Fehlersuche im Nachhinein.
- Läufe nach Deployments (CI/CD-Auslöser) – die CLI kann das bereits, es fehlen Auswertung/Alarm.
- Sinnvoll nur mit Server-Betrieb (R4); lokal als „Monitoring light“ über geplante Crawls + Regeln + Benachrichtigung denkbar.

### Lücken → R7 Analysen (lokal umsetzbar, Kandidaten für 1.x)
- CheiRank (Hub-Wert) zusätzlich zum PageRank-artigen Link Score, kombinierter 2D-Rank und „Link-Potenziale“ (wo interne Links fehlen).
- Navigationsebenen Bot vs. Nutzer (über welche Links Menschen eine Seite erreichen vs. Crawler), Inhaltsverteilung je Klicktiefe.
- Isolationsrisiko: Seiten, die nur über langfristig noindex/canonicalisierte Seiten erreichbar sind.
- Ähnliche URLs / Crawl-Pfad-Duplikate durch Facetten, unsortierte Parameter, fehlertolerante URLs (Cluster statt Einzelfilter).
- Sitemap-`lastmod`-Plausibilität (lastmod gegen Last-Modified/Inhaltsänderung).
- Mustergestützte Seitentyp-Segmentierung mit Strukturvergleich je Typ (überschneidet sich mit R3).
- Web Vitals segmentiert (je Seitentyp/Segment).

## Idee: Mobile App (Nutzer 02.11.2026, Backlog, nicht eingeplant)

Brainstorming vom 02.11.2026. Ein vollständiger Crawler auf dem Telefon ist nicht sinnvoll (Laufzeit, Speicher, Hintergrundbeschränkungen von iOS/Android).
Empfohlene Reihenfolge, frühestens nach 1.0:
- **D – Lite-Inspector:** einzelne URL prüfen (Status, Weiterleitungen, Title/Description, Canonical, hreflang, robots, strukturierte Daten), ohne Konto und ohne Server.
- **B – Companion-App:** Status laufender Crawls auf dem eigenen Rechner ansehen, Benachrichtigung bei Crawl-Ende, Overview/Top-Probleme lesen. Setzt eine Verbindung Rechner ↔ Telefon voraus (lokal oder über einen Relay-/Push-Dienst).
Offene Fragen an den Nutzer: Zielgruppe (Agentur unterwegs, Inhouse, Kunde), ob ein eigener Relay-/Push-Dienst betrieben werden soll, Plattformen (iOS, Android, beide).
