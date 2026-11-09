# OpenSpider – Funktionsspezifikation (Clean-Room)

Quelle: öffentliche Dokumentation, Release-Notes und beobachtbares Verhalten des Referenzprodukts (Referenzstand Oktober 2026).
Regeln:
- Verhalten in eigenen Worten beschreiben, keine Textpassagen kopieren, kein Code/Bytecode des Referenzprodukts.
- Jede Funktion hat eine ID `<BEREICH>-<NNN>` (z. B. `CRAWL-001`), Priorität P0 (MVP) / P1 (Parität) / P2 (später) und prüfbare Akzeptanzkriterien. Meilensteine (M1…M5, `docs/ARCHITECTURE.md`) werden unabhängig davon geschnitten: P beschreibt die Bedeutung einer Funktion, M den Zeitplan – ein P0-Eintrag aus 08 kommt z. B. erst mit M4, ein P1-Tab bereits mit M2 (Zuordnung siehe „Meilensteinzuordnung“ unten).

## Format je Funktion

### CRAWL-001 Titel
- **Priorität:** P0
- **Beschreibung:** …
- **Eingaben/Konfiguration:** …
- **Ausgabe (Spalten/Filter/Export):** …
- **Akzeptanzkriterien:** 
  - [ ] …
- **Testfixture:** welche Testseite das Verhalten prüft

## Meilensteinzuordnung

Quelle der Meilensteine: `docs/ARCHITECTURE.md` (Version 1.0 = M1 + M2 + M3; KI = M4; DataForSEO = M5). Die Tabelle ordnet Bereiche und ID-Gruppen zu; für die AGENT-IDs ist die Liste in `10-agent-interface.md` maßgeblich.

| M | Release | Bereiche / ID-Gruppen | Ausdrücklich nicht enthalten |
|---|---|---|---|
| M1 | intern | 01, 02, 03: alle P0-Einträge; 06: alle P0-Einträge außer OUT-250 … 266 (Crawl-Kern, Speicher, CLI-Kern aus 06 Abschnitt 8, Export-Engine, Basis-Tabs und -Filter); 07: PERF-001, -003, -005 (Kleinvarianten); 10: AGENT-001, -003, -004, -005, -010 … -013 | Desktop-UI (05), API-Integrationen (04), alles aus 08/09 |
| M2 | intern | 05: alle P0- und P1-Einträge ohne KI-Teile (u. a. UI-129 MCP-Menü, UI-013 ohne Feld `Kosten`); 01, 02, 03, 06: alle P1-Einträge und [P1]-Kriterien (u. a. JS-Rendering 01 G, Reports, Bulk-Exporte, Scheduling, Crawl-Vergleich, OUT-285, Manifest und Leseschnittstelle der Crawl-Datei OUT-233/234, Linux-Serverbetrieb OUT-273); 04 ohne KI: Abschnitt A (INTEG-001 … 019 ohne [M4]/[M5]-Kriterien), B – J (INTEG-020 … 157 ohne INTEG-069, das P2 ist); 07 vollständig; 10: AGENT-002, -014, -015, -020, -021, -040 … -043 | KI-Anbieter, Prompts, Embeddings, DataForSEO |
| M3 | **1.0** | P0/P1-Einträge aus OUT-250 … 266: Update-Mechanismus (OUT-250, -251, -252, -255, -257 Neuigkeiten nach einem Update), Plattformen, Signatur und Installer (OUT-260 … 263, -265, -266; OUT-263 in der Emulationsphase); Homebrew/winget. Die P2-Einträge OUT-253, -254, -264 gehören nicht dazu. **Visualisierung als Schwerpunkt (Nutzerentscheid 30.10.2026):** 05: UI-102 (neu, P1) sowie die vorgezogenen P2-Einträge UI-094, UI-097, UI-098, UI-100 (Priorität bleibt P2 als Bedeutung, Zuordnung M3) | keine weiteren neuen Fachfunktionen |
| M4 | 1.x | **08 vollständig** (AI-001 … AI-127, inkl. `BudgetLedger` und gemeinsamem Kostenvoranschlag, AI-012/013); **KI-Teile aus 04:** INTEG-160 … 179 (Anbieter, Prompts, KI-Tab „AI“), INTEG-180 … 189 (Embeddings, semantische Analysen), die KI-Prompt- und Embedding-Vorlagen der Custom-JS-Bibliothek in INTEG-069 (`llm-*`, `page-embedding-*`, `passage-embeddings`) sowie die [M4]-Kriterien in INTEG-001, -005, -019 und die KI-Flags aus INTEG-017/OUT-201; 02: ONPAGE-159 … 164 (Verweise auf Embeddings); 05: Tabs „AI“, „AI Decisions“, „AI Suggestions“, „AI Search“ (UI-004), UI-072, UI-101, Issue-Kategorien „AI“/„AI Search“ (UI-064), Statusleistenfeld `Kosten` (UI-013); 06: OUT-065, OUT-069, KI-Teile von OUT-025, -052, -194, -201; 01: [M4]-Kriterien in CRAWL-114 (KI-Crawler aus AI-120) und CRAWL-123 (API-Clients im Testbetrieb, soweit `os-ai`); **10: AGENT-030** (KI-Hüllen) | DataForSEO |
| M5 | 1.y | **09 vollständig** (D4S-001 … D4S-076); [M5]-Kriterien in INTEG-001, -005, -019, UI-004, UI-005, UI-064, OUT-194, OUT-201, AI-012, AI-013; DataForSEO-Hüllen in AGENT-030 | – |

Regeln:
- Kennzeichen **[M4]**/**[M5]** an einem Kriterium, Listeneintrag oder Absatz gehen der Zuordnung des umgebenden Eintrags vor (z. B. ist INTEG-001 ein M2-Eintrag, seine KI-Einträge gehören zu M4).
- Innerhalb eines Meilensteins bestimmt die Priorität die Reihenfolge; P2-Einträge aus 01 – 07 und 10 gehören zu keinem der Meilensteine M1 – M3 und werden nach Release 1.0 gesondert eingeplant; ausgenommen sind die in der M3-Zeile ausdrücklich genannten Einträge UI-094, UI-097, UI-098, UI-100. In 08 und 09 bezeichnet P0 die erste Ausbaustufe von M4 bzw. M5, P2 eine spätere Ausbaustufe.
- Für M5 gilt zusätzlich das Release-Gate am Ende von `09-dataforseo.md` (Golden-Test der SERP-Zuschläge, V-D3): Es ist vor Freigabe von M5 abzuhaken, aber kein Teil der CI-Akzeptanz.
- Eine Funktion ist erst abgenommen, wenn alle Kriterien ihres Meilensteins erfüllt sind; Kriterien späterer Meilensteine bleiben bis dahin offen und blockieren die Abnahme nicht.

**Nachtrag 09.11.2026 – Funktionsideen 1.x (Nutzerentscheidung 09.11.2026):** Die an diesem Tag aufgenommenen Einträge gehören **unabhängig von ihrer Priorität** zum Meilenstein **„1.x (nach 1.0)“** und zu keinem der Meilensteine M1 – M3; die Regel „P1-Einträge aus 01, 02, 03, 06 gehören zu M2“ gilt für sie nicht. Es sind: CRAWL-148, CRAWL-149, CRAWL-150; ONPAGE-182; TECH-090, TECH-091, TECH-147, TECH-320 … TECH-323; INTEG-085; UI-149, UI-150, UI-151, UI-152; OUT-286, OUT-287, OUT-288, OUT-289; AGENT-022 (dazu das Format `markdown` in AGENT-017). Ausnahme: **INTEG-190 gehört zu M4** (1.1, KI). Die Einträge tragen das Feld „Meilenstein“; eingeplant werden sie erst bei der Planung des jeweiligen 1.x-Releases (`docs/plan/ROADMAP-nach-1.0.md`, Abschnitt „Nachtrag 09.11.2026“). Neue Tabs, Filter und Spalten daraus sind OS-Erweiterungen: Paritätsvergleiche schließen sie aus, betroffene Golden-Dateien (z. B. Crawl Overview, Issues) werden mit der Umsetzung neu erzeugt.
