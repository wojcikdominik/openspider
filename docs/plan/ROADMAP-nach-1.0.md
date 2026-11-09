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
