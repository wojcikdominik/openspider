# 02 – Analyse-Tabs I: Onpage (ID-Präfix `ONPAGE`)

Clean-Room-Funktionsspezifikation für OpenSpider. Referenz: Referenzprodukt (Referenzstand Oktober 2026), beschrieben ausschließlich anhand öffentlicher Dokumentation (User Guide „Tabs“ und „Configuration“, SEO-Issues-Bibliothek, Tutorials zu Duplicate Content, Lesbarkeit, Rechtschreibung/Grammatik, Release-Notes mehrerer Hauptversionen). Alle Texte sind eigene Formulierungen. Wo die öffentliche Doku ein Verhalten nicht festlegt, trifft diese Spec eine **Annahme (A-n)**; alle Annahmen sind in Abschnitt 15 gesammelt und müssen per Paritätstest gegen die Referenz (Free-Version, ≤500 URLs) verifiziert und ggf. korrigiert werden.

## Inhaltsverzeichnis

| Abschnitt | IDs | Inhalt |
|---|---|---|
| 0 Grundlagen | – | Begriffe, Normalisierung, Schwellwerte, Fixture-Konventionen |
| 1 Gemeinsame Mechanik | ONPAGE-001 … 009 | Tab-Gerüst, Präferenzen, Textnormalisierung, Pixelbreite, Duplikat-Gruppierung, Issue-Klassifizierung, Crawl-Analyse-Abhängigkeit, Mehrfachvorkommen |
| 2 Internal | ONPAGE-010 … 024 | Spalten, Inhaltstyp-Filter |
| 3 External | ONPAGE-025 … 029 | Spalten, Inhaltstyp-Filter |
| 4 Security | ONPAGE-030 … 043 | HTTP/HTTPS, Mixed Content, Formulare, Cross-Origin, Security-Header, Bad Content Type |
| 5 Response Codes | ONPAGE-050 … 067 | Statusklassen, Blockierungen, Weiterleitungsarten, Ketten, Schleifen, Fehler-Exporte |
| 6 URL | ONPAGE-070 … 081 | URL-Hygiene-Filter |
| 7 Page Titles | ONPAGE-090 … 099 | Titel-Extraktion und 9 Filter |
| 8 Meta Description | ONPAGE-100 … 108 | Extraktion und 8 Filter |
| 9 Meta Keywords | ONPAGE-110 … 113 | Extraktion und 3 Filter |
| 10 H1 | ONPAGE-120 … 126 | Extraktion und 6 Filter |
| 11 H2 | ONPAGE-130 … 135 | Extraktion und 5 Filter |
| 12 Content | ONPAGE-140 … 166 | Inhaltsbereich, Wortzählung, exakte/Near-Duplicates, Low Content, Soft 404, Lorem Ipsum, Lesbarkeit, Rechtschreibung/Grammatik, Embeddings (nur Verweise, führend INTEG-180 ff.), PDF-Inhalte |
| 13 Images | ONPAGE-170 … 181 | Bildquellen, Spalten, Image Details, 7 Filter |
| 14 Querschnitt | ONPAGE-190 … 194 | Übersichtszähler, Export je Filter (Verweis), Lower-Tab-Verknüpfung, Live-Aktualisierung (Verweis), SERP-Snippet |
| 15 Annahmen & Verifikation | – | Liste aller Annahmen mit Prüfanweisung, Verifikationsaufträge V-1 … V-7, Paritäts-Erwartungswerte Issue-Klassifizierung (zu V-4) |

### Funktionsübersicht (alle IDs)

| ID | Titel | Prio |
|---|---|---|
| ONPAGE-001 | Tab-Gerüst: Spalten, Filter, Zähler | P0 |
| ONPAGE-002 | Präferenzen für Schwellwerte | P0 |
| ONPAGE-003 | Textnormalisierung und Zeichenlänge | P0 |
| ONPAGE-004 | Pixelbreiten-Berechnung (Verweis → UI-038) | P0 |
| ONPAGE-005 | Duplikat-Gruppierung (Wertgleichheit über Seiten) | P0 |
| ONPAGE-006 | Issue-Klassifizierung (Verweis → UI-063) | P0 |
| ONPAGE-007 | Abhängigkeit „Crawl-Analyse erforderlich“ | P0 |
| ONPAGE-008 | Mehrfachvorkommen und Spalten „…1/…2“ | P0 |
| ONPAGE-009 | Grundmenge der Element-Tabs | P0 |
| ONPAGE-010 | Internal: Basisspalten | P0 |
| ONPAGE-011 | Internal: Onpage-Element-Spalten | P0 |
| ONPAGE-012 | Internal: Size, Transferred, Total Transferred | P0 |
| ONPAGE-013 | Internal: Word Count, Text Ratio | P0 |
| ONPAGE-014 | Internal: Crawl Depth, Folder Depth | P0 |
| ONPAGE-015 | Internal: Link-Zählspalten | P0 |
| ONPAGE-016 | Internal: JS-Link-Zählspalten | P1 |
| ONPAGE-017 | Internal: Link Score (Verweis → TECH-145) | P1 |
| ONPAGE-018 | Internal: Technische Spalten (Hash, Response Time, Last-Modified, Redirect, HTTP-Version, URL Encoded Address) | P0 |
| ONPAGE-019 | Internal: Content-Analyse-Spalten | P1 |
| ONPAGE-020 | Inhaltstyp-Klassifizierung (gemeinsam für Internal/External) | P0 |
| ONPAGE-021 | Internal-Filter HTML | P0 |
| ONPAGE-022 | Internal-Filter JavaScript, CSS, Images, PDF | P0 |
| ONPAGE-023 | Internal-Filter Flash, Other (+ Erweiterung Media, Fonts, XML) | P1 |
| ONPAGE-024 | Internal-Filter Unknown | P0 |
| ONPAGE-025 | External: Definition und Spalten | P0 |
| ONPAGE-026 | External: Inhaltstyp-Filter | P0 |
| ONPAGE-030 | Security: Spalten | P1 |
| ONPAGE-031 | Security-Filter HTTP URLs | P0 |
| ONPAGE-032 | Security-Filter HTTPS URLs | P0 |
| ONPAGE-033 | Security-Filter Mixed Content | P1 |
| ONPAGE-034 | Security-Filter Form URL Insecure | P1 |
| ONPAGE-035 | Security-Filter Form on HTTP URL | P1 |
| ONPAGE-036 | Security-Filter Unsafe Cross-Origin Links | P1 |
| ONPAGE-037 | Security-Filter Protocol-Relative Resource Links | P1 |
| ONPAGE-038 | Security-Filter Missing HSTS Header | P1 |
| ONPAGE-039 | Security-Filter Missing Content-Security-Policy Header | P1 |
| ONPAGE-040 | Security-Filter Missing X-Content-Type-Options Header | P1 |
| ONPAGE-041 | Security-Filter Missing X-Frame-Options Header | P1 |
| ONPAGE-042 | Security-Filter Missing Secure Referrer-Policy Header | P1 |
| ONPAGE-043 | Security-Filter Bad Content Type | P1 |
| ONPAGE-050 | Response Codes: Spalten | P0 |
| ONPAGE-051 | Response Codes: Aufteilung Internal/External und „All“ | P0 |
| ONPAGE-052 | Filter Blocked by Robots.txt | P0 |
| ONPAGE-053 | Filter Blocked Resource | P1 |
| ONPAGE-054 | Filter No Response | P0 |
| ONPAGE-055 | Filter Success (2XX) | P0 |
| ONPAGE-056 | Filter Redirection (3XX) | P0 |
| ONPAGE-057 | Filter Redirection (JavaScript) | P1 |
| ONPAGE-058 | Filter Redirection (Meta Refresh) | P0 |
| ONPAGE-059 | Filter Redirection (HTTP Refresh) | P1 |
| ONPAGE-060 | Filter Redirect Chain | P1 |
| ONPAGE-061 | Filter Redirect Loop | P1 |
| ONPAGE-062 | Filter Client Error (4XX) | P0 |
| ONPAGE-063 | Filter Server Error (5XX) | P0 |
| ONPAGE-064 | Redirect Type „HSTS Policy“ | P1 |
| ONPAGE-065 | Status-Texte für Fehlerfälle ohne Antwort | P0 |
| ONPAGE-066 | Statuscodes außerhalb 2xx–5xx (1xx, ≥600) | P2 |
| ONPAGE-067 | Bulk-Export „All Error Inlinks“ und Report „Redirects to Error“ (Verweis → OUT-067) | P1 |
| ONPAGE-070 | URL: Spalten | P0 |
| ONPAGE-071 | URL-Filter Non ASCII Characters | P0 |
| ONPAGE-072 | URL-Filter Underscores | P0 |
| ONPAGE-073 | URL-Filter Uppercase | P0 |
| ONPAGE-074 | URL-Filter Multiple Slashes | P0 |
| ONPAGE-075 | URL-Filter Repetitive Path | P1 |
| ONPAGE-076 | URL-Filter Contains A Space | P0 |
| ONPAGE-077 | URL-Filter Internal Search | P1 |
| ONPAGE-078 | URL-Filter Parameters | P0 |
| ONPAGE-079 | URL-Filter Broken Bookmark | P1 |
| ONPAGE-080 | URL-Filter GA Tracking Parameters | P1 |
| ONPAGE-081 | URL-Filter Over 115 Characters | P0 |
| ONPAGE-090 | Page Titles: Extraktion und Spalten | P0 |
| ONPAGE-091 | Titel-Filter Missing | P0 |
| ONPAGE-092 | Titel-Filter Duplicate | P0 |
| ONPAGE-093 | Titel-Filter Over 60 Characters | P0 |
| ONPAGE-094 | Titel-Filter Below 30 Characters | P0 |
| ONPAGE-095 | Titel-Filter Over 561 Pixels | P1 |
| ONPAGE-096 | Titel-Filter Below 200 Pixels | P1 |
| ONPAGE-097 | Titel-Filter Same as H1 | P1 |
| ONPAGE-098 | Titel-Filter Multiple | P0 |
| ONPAGE-099 | Titel-Filter Outside <head> | P1 |
| ONPAGE-100 | Meta Description: Extraktion und Spalten | P0 |
| ONPAGE-101 | MD-Filter Missing | P0 |
| ONPAGE-102 | MD-Filter Duplicate | P0 |
| ONPAGE-103 | MD-Filter Over 155 Characters | P0 |
| ONPAGE-104 | MD-Filter Below 70 Characters | P0 |
| ONPAGE-105 | MD-Filter Over 985 Pixels | P1 |
| ONPAGE-106 | MD-Filter Below 400 Pixels | P1 |
| ONPAGE-107 | MD-Filter Multiple | P0 |
| ONPAGE-108 | MD-Filter Outside <head> | P1 |
| ONPAGE-110 | Meta Keywords: Extraktion und Spalten | P1 |
| ONPAGE-111 | MK-Filter Missing | P1 |
| ONPAGE-112 | MK-Filter Duplicate | P1 |
| ONPAGE-113 | MK-Filter Multiple | P1 |
| ONPAGE-120 | H1: Extraktion und Spalten | P0 |
| ONPAGE-121 | H1-Filter Missing | P0 |
| ONPAGE-122 | H1-Filter Duplicate | P0 |
| ONPAGE-123 | H1-Filter Over 70 Characters | P0 |
| ONPAGE-124 | H1-Filter Multiple | P0 |
| ONPAGE-125 | H1-Filter Alt Text in h1 | P1 |
| ONPAGE-126 | H1-Filter Non-sequential | P1 |
| ONPAGE-130 | H2: Extraktion und Spalten | P0 |
| ONPAGE-131 | H2-Filter Missing | P0 |
| ONPAGE-132 | H2-Filter Duplicate | P0 |
| ONPAGE-133 | H2-Filter Over 70 Characters | P0 |
| ONPAGE-134 | H2-Filter Multiple | P0 |
| ONPAGE-135 | H2-Filter Non-sequential | P1 |
| ONPAGE-140 | Inhaltsbereich (Content Area) | P0 |
| ONPAGE-141 | Wortzählung | P0 |
| ONPAGE-142 | Content: Spalten | P0 |
| ONPAGE-143 | Content-Filter Exact Duplicates | P0 |
| ONPAGE-144 | Content-Filter Near Duplicates (MinHash) | P1 |
| ONPAGE-145 | Lower-Tab Duplicate Details | P1 |
| ONPAGE-146 | Content-Filter Low Content Pages | P0 |
| ONPAGE-147 | Content-Filter Soft 404 Pages | P1 |
| ONPAGE-148 | Content-Filter Lorem Ipsum Placeholder | P1 |
| ONPAGE-149 | Lesbarkeit: Flesch-Berechnung und Spalten | P1 |
| ONPAGE-150 | Content-Filter Readability Difficult | P1 |
| ONPAGE-151 | Content-Filter Readability Very Difficult | P1 |
| ONPAGE-152 | Lesbarkeit für weitere Sprachen (Erweiterung) | P2 |
| ONPAGE-153 | Rechtschreibprüfung: Engine und Sprachwahl | P1 |
| ONPAGE-154 | Grammatikprüfung: Engine und Regeln | P1 |
| ONPAGE-155 | Content-Filter Spelling Errors | P1 |
| ONPAGE-156 | Content-Filter Grammar Errors | P1 |
| ONPAGE-157 | Lower-Tab Spelling & Grammar Details, Ignorieren, Wörterbuch | P1 |
| ONPAGE-158 | Neu-Prüfung ohne Recrawl und Top-100-Fehlerübersicht | P2 |
| ONPAGE-159 | Embeddings: Anbieter und Konfiguration (Verweis → INTEG-180) | P1 |
| ONPAGE-160 | Content-Filter Semantically Similar (Verweis → INTEG-181) | P1 |
| ONPAGE-161 | Content-Filter Low Relevance Content (Verweis → INTEG-182) | P1 |
| ONPAGE-162 | Embedding-Regeln (URL-Muster) (Verweis → INTEG-184) | P1 |
| ONPAGE-163 | Lower-Tab Duplicate Details: Semantic Similarity (Verweis → INTEG-183) | P1 |
| ONPAGE-164 | Embeddings nur für indexierbare Seiten (Verweis → INTEG-180) | P1 |
| ONPAGE-165 | Bulk-Exporte Content | P1 |
| ONPAGE-166 | PDF-Dokumente: Titel, Keywords, Wortzahl, Lesbarkeit, Rechtschreibung | P1 |
| ONPAGE-170 | Bildquellen-Erkennung | P0 |
| ONPAGE-171 | Images: Spalten | P0 |
| ONPAGE-172 | Lower-Tab Image Details | P1 |
| ONPAGE-173 | Bildgröße bestimmen | P0 |
| ONPAGE-174 | Image-Filter Over 100 KB | P0 |
| ONPAGE-175 | Image-Filter Missing Alt Text | P0 |
| ONPAGE-176 | Image-Filter Missing Alt Attribute | P0 |
| ONPAGE-177 | Image-Filter Alt Text Over 100 Characters | P0 |
| ONPAGE-178 | Image-Filter Background Images | P1 |
| ONPAGE-179 | Image-Filter Missing Size Attributes | P1 |
| ONPAGE-180 | Image-Filter Incorrectly Sized Images | P1 |
| ONPAGE-181 | Bulk-Exporte Images | P1 |
| ONPAGE-182 | Image-Filter Missing Title Attribute (Nachtrag 09.11.2026, 1.x) | P2 |
| ONPAGE-190 | Übersichtszähler je Tab/Filter | P0 |
| ONPAGE-191 | Export der aktuellen Filteransicht (Verweis → OUT-020) | P0 |
| ONPAGE-192 | Verknüpfung mit Lower-Tabs (Inlinks/Outlinks/Details) | P0 |
| ONPAGE-193 | Live-Aktualisierung während des Crawls (Verweis → UI-018) | P0 |
| ONPAGE-194 | Lower-Tab SERP Snippet: Geräteumschaltung und Bearbeitung | P1 |

Summe: 150 Funktionen (P0: 80, P1: 67, P2: 3), davon 12 Verweis-Stubs (ONPAGE-004, -006, -017, -067, -159 … -164, -191, -193). Insgesamt 521 prüfbare Akzeptanzkriterien, 57 dokumentierte Annahmen (A-1 … A-56 inkl. A-5b) und 7 Verifikationsaufträge (V-1 … V-7).

**Änderungsstand (Spec v1, Architektenentscheidungen 2026-10-13):** Grundregel „eine Quelle je Funktion“ angewandt; nicht führende Duplikate sind zu Verweis-Stubs gekürzt (IDs bleiben bestehen): ONPAGE-004 → UI-038 (K-9, P0), ONPAGE-006 → UI-063 (K-10, P0; frühere Zuordnungstabelle nur noch als Paritäts-Erwartungswerte in Abschnitt 15), ONPAGE-017 → TECH-145 (Link Score), ONPAGE-067 → OUT-067/OUT-084 (All Error Inlinks), ONPAGE-159 … 164 → INTEG-180 … 184 (K-1, jetzt P1), ONPAGE-191 → OUT-020 (Export aktuelle Ansicht), ONPAGE-193 → UI-018 (K-11, P0). Jede ID trägt genau eine Priorität: ONPAGE-012 und ONPAGE-142 auf P0 vereinheitlicht, abweichende Teilumfänge als [P1]-Kriterien markiert. ONPAGE-007 auf die Filterzustände dieses Dokuments begrenzt (Ablauf CRAWL-141/UI-080). ONPAGE-157 als führend für den Inhalt des Detail-Tabs gekennzeichnet (Layout UI-047, K-15), gegen die öffentliche Doku geprüft. ONPAGE-165/181 verweisen für Format und Mechanik auf OUT-052/053/066. Gegenprüfung: Default `issues.ignore_non_indexable` auf `true` nach CRAWL-089 nachgezogen (0.3, ONPAGE-009, A-51); ONPAGE-017-Stub auf Skala 1–100 nach TECH-145 korrigiert.

**Nacharbeit reviews-open-v1 (2026-10-13):** ONPAGE-165 legt die Spaltennamen der Content-Bulk-Exporte als führender Fach-Eintrag fest (`Exact Duplicate Address`; OUT-052 übernimmt sie exakt, ebenso `Page Section`/`Type` aus ONPAGE-157 und `Error`/`Type`/`URLs`/`Instances` aus ONPAGE-158); Soft-404-Inlinks im Format OUT-031.

**Änderungsstand (Review-Runde 2):** Ergänzt: Schalter für paginierte URLs bei Duplikaten (ONPAGE-005), Schalter für nicht indexierbare URLs in Problemfiltern (ONPAGE-009), PDF-Unterstützung (ONPAGE-009/011/013/142/149/153, neu ONPAGE-166), Fehler-Exporte im Response-Codes-Bereich (neu ONPAGE-067), Inhaltsbegrenzung und Fehlerfall bei Embeddings (ONPAGE-159), Image-Details-Spalten Path Type/Link Path/Link Position/Link Origin und srcset-Verhalten (ONPAGE-170/172/181), Soft-404-Inlinks-Export (ONPAGE-165), SERP-Snippet je Gerät (neu ONPAGE-194), Sprachliste der Referenz (ONPAGE-153). Korrigiert: Filtername „Flash“ (ONPAGE-023), nicht belegte Spalten als Erweiterung markiert (ONPAGE-025/142/157/172), Real Dimensions nur mit Rendering (ONPAGE-172), Readability-Labels als Annahme (ONPAGE-149), Pixel-Defaults als zu verifizieren markiert (0.3, V-3). Geschärft: ONPAGE-001/004/144/145/153/154/160/191.

---

## 0 Grundlagen

### 0.1 Begriffe

- **Interne URL:** URL, deren Host dem Host der Start-URL entspricht (Groß-/Kleinschreibung egal; Port und Schema dürfen abweichen: `http://` und `https://` desselben Hosts sowie ein anderer Port desselben Hosts gelten als intern – CRAWL-006 führt, OS-Festlegung 14.10.2026, Welle-3-Sichtung, K-43). Subdomains gelten als extern, außer die Crawl-Option „alle Subdomains crawlen“ ist aktiv (Definition liegt im Crawl-Bereich; hier nur referenziert). `www.` ist eine eigene Subdomain.
- **Externe URL:** jede nicht interne URL.
- **HTML-Seite:** URL, deren Inhaltstyp nach ONPAGE-020 als „HTML“ klassifiziert ist.
- **Indexierbar:** gemäß Indexierbarkeits-Logik (eigener Spec-Bereich). In diesem Dokument wird nur das Ergebnis genutzt: `Indexability ∈ {Indexable, Non-Indexable}` und `Indexability Status` als Begründung (z. B. `Canonicalised`, `noindex`, `Blocked by robots.txt`, `Redirected`, `Client Error`, `Server Error`, `No Response`).
- **Element-Tabs:** Page Titles, Meta Description, Meta Keywords, H1, H2.
- **Inhaltsbereich:** konfigurierbarer Teil des `<body>` (ONPAGE-140), Basis für Wortzählung, Near-Duplicates, Lesbarkeit, Rechtschreibung/Grammatik, Embeddings, Low Content.
- **Crawl-Analyse:** Nachlaufschritt nach Crawl-Ende (Crate `os-analyze`), der seitenübergreifende Kennzahlen berechnet.
- **Parse-Baum:** Das HTML wird für alle strukturabhängigen Prüfungen (Outside `<head>`, Heading-Reihenfolge, Mehrfachvorkommen) nach dem HTML5-Parsing-Algorithmus (WHATWG) interpretiert, so wie ein Browser es täte. Ein Element „liegt im `<head>`“, wenn es im resultierenden Baum Nachfahre des `head`-Elements ist. Konsequenz: Ein `<div>` oder `<img>` im Head-Bereich beendet den `<head>` implizit; alle danach folgenden `<title>`/`<meta>` landen im `<body>`.

### 0.2 Textnormalisierung N(x)

Gilt für Titel, Meta-Description, Meta-Keywords, Überschriften, Alt-Texte:
1. HTML-Entities dekodieren (`&amp;` → `&`, `&nbsp;` → U+00A0, numerische Entities).
2. Jede Folge von Leerraum (Unicode `White_Space`, inkl. U+00A0, Tab, CR, LF) durch genau ein Leerzeichen U+0020 ersetzen.
3. Führenden und abschließenden Leerraum entfernen.
4. Keine Änderung der Groß-/Kleinschreibung, keine Unicode-Normalisierung (NFC bleibt wie geliefert) – **Annahme A-1**.

**Zeichenlänge** `len(x)` = Anzahl Unicode-Codepoints von N(x) (**Annahme A-2**; Referenz basiert vermutlich auf UTF-16-Einheiten, relevant nur bei Emoji/Astral-Zeichen).

### 0.3 Standard-Schwellwerte (Präferenzen)

| Schlüssel | Default | Wirkt auf |
|---|---|---|
| `title.max_chars` | 60 | ONPAGE-093 |
| `title.min_chars` | 30 | ONPAGE-094 |
| `title.max_px` | 561 (zu verifizieren, V-3) | ONPAGE-095 |
| `title.min_px` | 200 (zu verifizieren, V-3) | ONPAGE-096 |
| `meta_description.max_chars` | 155 | ONPAGE-103 |
| `meta_description.min_chars` | 70 | ONPAGE-104 |
| `meta_description.max_px` | 985 (zu verifizieren, V-3) | ONPAGE-105 |
| `meta_description.min_px` | 400 (zu verifizieren, V-3; kein öffentlicher Beleg gefunden) | ONPAGE-106 |
| `url.max_chars` | 115 | ONPAGE-081 |
| `h1.max_chars` | 70 | ONPAGE-123 |
| `h2.max_chars` | 70 | ONPAGE-133 |
| `image.max_kb` | 100 (1 KB = 1024 Byte) | ONPAGE-174 |
| `image.alt_max_chars` | 100 | ONPAGE-177 |
| `content.low_word_count` | 200 | ONPAGE-146 |
| `content.soft404_phrases` | siehe ONPAGE-147 | ONPAGE-147 |
| `duplicates.near_threshold_pct` | 90 | ONPAGE-144 |
| `duplicates.only_indexable` | true | ONPAGE-005, -143, -144 |
| `duplicates.near_enabled` | false | ONPAGE-144 |
| `duplicates.ignore_paginated` | true (**Annahme A-50**) | ONPAGE-005, -092, -102, -112, -122, -132 |
| `issues.ignore_non_indexable` | true (Festlegung CRAWL-089, führend; ersetzt frühere Annahme A-51) | ONPAGE-009 und alle Problemfilter der Element-Tabs, ONPAGE-146 |
| Embedding-Schwellen und -Optionen | siehe INTEG-180 … 189 (führend; Vergleich „≥ Schwelle“) | ONPAGE-159 … 164 (Verweise) |
| `pdf.store` | false | ONPAGE-166 |
| `serp.<device>.title_max_px`, `serp.<device>.description_max_px` | siehe ONPAGE-194 | ONPAGE-194 |
| `images.incorrect_size_min_saving_kb` | 4 | ONPAGE-180 |

**Belegstand der Pixel-Defaults (V-3):** Die Werte 561, 985 und 200 sind in der Referenz nur als Teil von Filter- bzw. Issue-Namen („Over 561 Pixels“, „Over 985 Pixels“, „Below 200 Pixels“) sichtbar; ausdrücklich als einstellbarer Default beschrieben ist nur die Untergrenze von 200 px für Titel. Für 400 px (Meta Description, Untergrenze) wurde kein öffentlicher Beleg gefunden. Alle vier Werte sind daher vorläufige Defaults, die im Paritätstest anhand der Filternamen einer frisch installierten Referenz (Default-Konfiguration) bestätigt werden müssen. Die Referenz passt diese Grenzen gelegentlich an veränderte Suchergebnis-Darstellungen an; OpenSpider übernimmt den beim Paritätstest beobachteten Stand.

### 0.4 Fixture-Konventionen

- Fixtures liegen unter `tests/fixtures/<szenario>/` und werden von `os-testsite` ausgeliefert; Statuscodes, Header, Weiterleitungen, Content-Types und Verzögerungen werden in `scenario.toml` als `[[route]]` definiert, absolute Links nutzen `{{BASE_URL}}`.
- Szenario-Namen dieses Dokuments beginnen mit `onpage-` (z. B. `onpage-titles`).
- Jede Fixture hat eine Golden-Datei `tests/golden/<szenario>/<tab>__<filter>.csv` mit der erwarteten Adressliste je Filter (sortiert), damit ein Test pro Akzeptanzkriterium ohne UI läuft.
- **Infrastruktur-Bedarf für dieses Dokument** (an `os-testsite`, Absprache mit Orchestrator):
  - **F-TLS:** HTTPS-Modus mit selbstsigniertem Zertifikat (Crawler im Test mit „ungültige Zertifikate akzeptieren“) – für Security-Fixtures.
  - **F-HOST2:** „Externer Host“: Der Crawl startet auf `http://127.0.0.1:<port>/`; Links auf `http://localhost:<port>/` gelten dann als extern (anderer Host, gleicher Server). Alternativ zweite `TestSite`-Instanz.
  - **F-RAW:** Route-Option `raw_body_bytes`/`gzip = true`, um Transfer-Größe ≠ Größe zu testen.
  - Nicht erreichbare Ziele ohne Netz: geschlossener Port (Connection Refused), Hostname unter `.invalid` (DNS-Fehler, RFC 2606), `delay_ms` > Crawl-Timeout (Timeout).

---

## 1 Gemeinsame Mechanik

### ONPAGE-001 Tab-Gerüst: Spalten, Filter, Zähler
- **Priorität:** P0
- **Beschreibung:** Jeder Analyse-Tab ist eine Tabellenansicht auf den Crawl-Speicher mit fester Spaltenliste und einer Filterliste. Der erste Filter heißt immer „All“ und zeigt die Grundmenge des Tabs; jeder weitere Filter ist eine Teilmenge. Filter werden in `os-analyze` als benannte Prädikate implementiert und in `os-store` als abfragbare Sicht (Paging, Sortierung, Volltextsuche über Adresse) bereitgestellt.
- **Eingaben/Konfiguration:** Tab-ID, Filter-ID, Sortierung, Seitengröße, Suchbegriff.
- **Ausgabe:** Zeilen mit den Spalten des Tabs; je Filter die Trefferzahl und der Anteil an der Grundmenge in Prozent (eine Nachkommastelle).
- **Akzeptanzkriterien:**
  - [ ] Jeder Tab dieses Dokuments liefert als ersten Filter „All“; dessen Zeilenzahl ist gleich der in der Tab-Einleitung bzw. ONPAGE-009 definierten Grundmenge (Test: für `onpage-titles` liefert Page Titles → All genau die 2xx-HTML-Seiten der Golden-Liste `page_titles__all.csv`).
  - [ ] Für jede Fixture und jeden Filter F gilt `F ⊆ All` (automatischer Test über alle `onpage-*`-Szenarien; ein Verstoß lässt den Test mit Tab-, Filter- und URL-Angabe fehlschlagen).
  - [ ] Eine URL darf in mehreren Filtern desselben Tabs stehen: `onpage-titles/pixel-W70.html` (70× „W“) erscheint sowohl in „Over 60 Characters“ als auch in „Over 561 Pixels“.
  - [ ] Prozentangabe = Treffer ÷ Größe(All) × 100, kaufmännisch auf eine Nachkommastelle gerundet: 3 von 7 → „42,9 %“, 1 von 3 → „33,3 %“, 2 von 3 → „66,7 %“; bei leerer Grundmenge „0,0 %“ (keine Division durch 0, kein „NaN“).
  - [ ] Die Spaltenreihenfolge im Default-Layout entspricht der Reihenfolge in dieser Spec; Ausblenden oder Verschieben einer Spalte ändert weder Trefferzahl noch Zeilenmenge eines Filters (Test: Golden-Liste vor/nach Ausblenden aller Spalten außer Address identisch).
  - [ ] Sortierung ist stabil: bei gleichem Spaltenwert entscheidet Address aufsteigend (Bytevergleich der Anzeigeform), bei identischer Address zuletzt die Entdeckungsreihenfolge (interne URL-ID); ohne Sortierung gilt die Entdeckungsreihenfolge. Leere Werte stehen bei auf- und absteigender Sortierung am Ende (UI-010). Textspalten sortieren über die sprachunabhängige Unicode-Root-Kollation (Groß/Klein gleichwertig), URL-Spalten bytewise. Diese Regel gilt identisch für Oberfläche, Exporte und MCP (Entscheidung (Fable-Empfehlung) vom 13.10.2026).
  - [ ] Die Volltextsuche filtert innerhalb des aktiven Filters über Address (Teilstring, case-insensitiv): Suche „len-6“ in Page Titles → All liefert genau `len-60.html` und `len-61.html`.
  - [ ] Filter mit nicht erfüllter Voraussetzung (Rendering, Crawl-Analyse, optionale Prüfung, abgeschalteter Speicher-Schalter) melden einen Zustand aus ONPAGE-007 und keine Trefferzahl 0.
- **Testfixture:** `onpage-titles` (siehe ONPAGE-090) plus `pixel-W70.html`; der Test iteriert alle Filter aller Tabs und prüft Teilmengen-Eigenschaft, Zähler, Prozentwerte und Sortierstabilität.

### ONPAGE-002 Präferenzen für Schwellwerte
- **Priorität:** P0
- **Beschreibung:** Alle Schwellwerte aus 0.3 sind in der Crawl-Konfiguration (Abschnitt „Preferences“) einstellbar. Änderungen nach dem Crawl wirken ohne Recrawl auf die Filter, weil Filter aus gespeicherten Rohwerten (Länge, Pixelbreite, Wortzahl, Bytes) berechnet werden.
- **Eingaben/Konfiguration:** `CrawlConfig.preferences` (serde, mit Defaults aus 0.3).
- **Ausgabe:** Filter-Namen enthalten den aktuellen Wert dynamisch, z. B. wird „Over 60 Characters“ im Page-Titles-Tab zu „Over 70 Characters“, sobald `title.max_chars = 70` gesetzt ist.
- **Akzeptanzkriterien:**
  - [ ] Ohne Konfiguration gelten exakt die Defaults aus 0.3.
  - [ ] Ändern von `title.max_chars` von 60 auf 50 nach Crawl-Ende ändert die Treffermenge von ONPAGE-093 sofort, ohne erneute HTTP-Anfragen (Test: Request-Zähler des Fixture-Servers bleibt gleich).
  - [ ] Filterbezeichnung zeigt den eingestellten Wert (z. B. „Over 50 Characters“, „Over 600 Pixels“, „Over 200 KB“).
  - [ ] Ungültige Werte (negativ, min > max) werden beim Laden der Konfiguration mit Fehlermeldung abgelehnt.
  - [ ] Konfiguration wird in der Crawl-Datei gespeichert; beim Öffnen eines alten Crawls gelten dessen Werte.
- **Testfixture:** `onpage-titles` mit zwei Konfigurationen (Default / `title.max_chars=50`); Golden-Listen für beide.

### ONPAGE-003 Textnormalisierung und Zeichenlänge
- **Priorität:** P0
- **Beschreibung:** Einheitliche Funktion `normalize_text()` und `char_len()` gemäß 0.2 im Crate `os-parse`, genutzt von allen Element-Tabs, Alt-Text-Prüfungen und Duplikatvergleichen.
- **Eingaben/Konfiguration:** keine.
- **Ausgabe:** normalisierter String, Länge in Codepoints.
- **Akzeptanzkriterien:**
  - [ ] `"  Hallo\n\t Welt  "` → `"Hallo Welt"`, Länge 10.
  - [ ] `"A&amp;B"` → `"A&B"`, Länge 3.
  - [ ] `"&nbsp;Test&nbsp;"` → `"Test"` (NBSP zählt als Leerraum).
  - [ ] `"Über"` → Länge 4 (nicht 5 Bytes).
  - [ ] Leerer oder nur aus Leerraum bestehender Wert → `""`, Länge 0.
  - [ ] Ungültige Entities (`&foo;`) bleiben unverändert erhalten.
- **Testfixture:** Unit-Tests in `os-parse`; zusätzlich `onpage-titles/entities.html` mit `<title>  Shop &amp; Mehr&nbsp; </title>` → Title 1 = „Shop & Mehr“, Länge 11.

### ONPAGE-004 Pixelbreiten-Berechnung
- **Priorität:** P0
- **Verweis:** siehe UI-038 (führend). Die Spalten `Title 1/2 Pixel Width` und `Meta Description 1/2 Pixel Width` sowie die Filter ONPAGE-095, -096, -105, -106 und die SERP-Vorschau ONPAGE-194 nutzen ausschließlich die UI-038-Berechnung, die im Kern (`os-analyze`/`os-parse`) läuft und nicht im Frontend.

### ONPAGE-005 Duplikat-Gruppierung (Wertgleichheit über Seiten)
- **Priorität:** P0
- **Beschreibung:** Gemeinsame Logik für die Filter „Duplicate“ in Page Titles, Meta Description, Meta Keywords, H1, H2: Eine Seite ist Duplikat, wenn ihr erster Wert (…1) nach Normalisierung N(x) nicht leer ist und mindestens eine andere Seite der Vergleichsmenge denselben Wert hat. Vergleichsmenge = Grundmenge des Tabs, eingeschränkt auf indexierbare Seiten, solange `duplicates.only_indexable = true` (Default). Vergleich nach N(x) **ohne Beachtung der Groß-/Kleinschreibung** (**Annahme A-4, revidiert nach Paritätslauf M1 vom 14.10.2026, K-58**: die Referenz zählt Werte, die sich nur in der Schreibweise unterscheiden, als Duplikat; Fixture `parity-onpage` `dup-case.html`). Verglichen wird mit der Kollation `OS_TEXT` (ICU-Root, Stärke Secondary), mit der OpenSpider auch alle Textspalten sortiert: Unterschiede der Groß-/Kleinschreibung und andere tertiäre Unterschiede zählen nicht, Akzente zählen, kanonisch äquivalente Schreibungen (NFC/NFD) sind gleich (**Annahme A-4b**, Entscheidung K-70 vom 15.10.2026; ersetzt `str::to_lowercase` aus K-58). Gegenüber `str::to_lowercase` unterscheidet sich das Ergebnis nur bei kanonischer Äquivalenz, ß/ss und Sigma-Formen; belegt ist gegen die Referenz nur der Fall reiner Groß-/Kleinschreibung. Die Randfälle prüft ein Verifikationsauftrag in M2 (`docs/plan/M2-backlog.md`, M2-S04).
  - **Paginierte Seiten:** Ist `duplicates.ignore_paginated = true` (Referenz: Option unter Spider-Konfiguration → Advanced), werden Seiten, die ein `<link rel="prev">` (im `<head>`, Wert nicht leer) oder einen HTTP-`Link`-Header mit `rel="prev"` tragen, aus der Vergleichsmenge aller fünf Duplicate-Filter (Page Titles, Meta Description, Meta Keywords, H1, H2) entfernt. Die erste Seite einer Serie (nur `rel="next"`) bleibt in der Vergleichsmenge. Der Schalter wirkt nicht auf Content → Exact/Near Duplicates. Die Default-Stellung ist in der öffentlichen Doku nicht angegeben (**Annahme A-50**: an).
- **Eingaben/Konfiguration:** `duplicates.only_indexable`, `duplicates.ignore_paginated`.
- **Ausgabe:** Filter „Duplicate“ je Element-Tab; zusätzlich Gruppen-ID (intern) zur Anzeige „gleicher Wert wie …“.
- **Akzeptanzkriterien:**
  - [ ] Zwei indexierbare Seiten mit Titel „Startseite“ → beide im Filter.
  - [ ] Eine indexierbare und eine canonicalisierte (non-indexable) Seite mit gleichem Titel → keine im Filter (Default); bei `only_indexable=false` → beide.
  - [ ] „Startseite“ vs. „startseite“ → Duplikat, beide im Filter (A-4, revidiert).
  - [ ] „Startseite “ (mit Leerzeichen) vs. „Startseite“ → Duplikat (Normalisierung).
  - [ ] Leere Werte bilden nie eine Duplikatgruppe (die gehören zu „Missing“).
  - [ ] Nur der erste Wert (…1) wird verglichen; ein identischer zweiter Titel (…2) erzeugt kein Duplikat.
  - [ ] Inkrementell: Duplikate werden bereits während des Crawls aktualisiert (beide Seiten erscheinen, sobald die zweite gecrawlt ist).
  - [ ] Paginierung, `ignore_paginated = true`: `/blog/` (nur `rel="next"`), `/blog/page/2/`, `/blog/page/3/` (je mit `rel="prev"`) und `/archiv` (ohne Paginierungs-Links) haben alle den Titel „Blog“ → im Filter Page Titles → Duplicate stehen genau `/blog/` und `/archiv`; `/blog/page/2/` und `/blog/page/3/` fehlen. Dasselbe gilt für Meta Description, Meta Keywords, H1 und H2.
  - [ ] Paginierung, `ignore_paginated = false`: alle vier Seiten stehen in allen fünf Duplicate-Filtern.
  - [ ] `rel="prev"` nur als HTTP-Header (`/blog/page/4/` mit `Link: </blog/page/3/>; rel="prev"`) wirkt wie das HTML-Element: Seite fehlt bei `ignore_paginated = true` im Duplicate-Filter.
  - [ ] Umschalten von `ignore_paginated` nach Crawl-Ende ändert die Filter ohne HTTP-Anfragen.
- **Testfixture:** `onpage-duplicates/`: `a.html`, `b.html` (gleicher Titel/MD/H1/H2/Keywords), `c.html` (gleiche Werte, aber `<link rel=canonical href=a.html>`), `d.html` (nur Groß-/Kleinschreibung anders), `e.html` (gleicher Wert mit Zusatz-Leerzeichen); Unterordner `pagination/` mit `blog/index.html`, `blog/page/2/index.html`, `blog/page/3/index.html`, `archiv.html` (alle Titel/MD/Keywords/H1/H2 identisch) und Route `/blog/page/4/` mit `rel="prev"` nur als `Link`-Header.

### ONPAGE-006 Issue-Klassifizierung (Typ/Priorität)
- **Priorität:** P0
- **Verweis:** siehe UI-063 (führend). Die Tabelle „Paritäts-Erwartungswerte Issue-Klassifizierung“ in Abschnitt 15 dieses Dokuments dient nur als Erwartungswert-Liste für den Paritätstest (V-4).

### ONPAGE-007 Abhängigkeit „Crawl-Analyse erforderlich“
- **Priorität:** P0
- **Beschreibung:** Manche Filter benötigen seitenübergreifende Nachberechnung (Near Duplicates, Semantically Similar, Low Relevance Content, Background Images, Incorrectly Sized Images, Redirect Chain/Loop-Vervollständigung, Link Score) oder eine optionale Funktion (JS-Rendering, Spell-/Grammar-Check, Embeddings). Solange die Voraussetzung fehlt, zeigt der Filter einen Zustandshinweis. Dieser Eintrag legt nur fest, **welche Filter dieses Dokuments** von welcher Voraussetzung abhängen und welchen Zustand sie zeigen. Auslösung, Ablauf und Abbruch der Crawl-Analyse sind in CRAWL-141 (führend für den Ablauf) bzw. UI-080 (Teilschritte, Fortschritt) spezifiziert, die Darstellung des Hinweises in UI-016/UI-080.
- **Eingaben/Konfiguration:** Status der Crawl-Analyse (nicht gestartet / läuft / fertig / veraltet nach Konfig-Änderung); Konfigurationsschalter.
- **Ausgabe:** Filterzustand `ready | needs_crawl_analysis | needs_config(<Schalter>) | needs_rendering`.
- **Akzeptanzkriterien:**
  - [ ] Nach Crawl-Ende ohne Analyse zeigt „Near Duplicates“ den Zustand `needs_crawl_analysis` und keine Zeilen.
  - [ ] Ist Near-Duplicates deaktiviert, zeigt der Filter `needs_config(duplicates.near_enabled)`.
  - [ ] Nach Änderung von Schwellwert oder Inhaltsbereich wechselt der Zustand auf „veraltet“; erneute Analyse läuft ohne HTTP-Anfragen.
  - [ ] Nach einer abgebrochenen Crawl-Analyse (Ablauf CRAWL-141/UI-080) zeigen die Filter dieses Dokuments den letzten konsistenten Stand bzw. weiterhin `needs_crawl_analysis`, nie Teilergebnisse.
- **Testfixture:** `onpage-content-dupes` (ONPAGE-144) mit Zustandsprüfung vor/nach Analyse.

### ONPAGE-008 Mehrfachvorkommen und Spalten „…1/…2“
- **Priorität:** P0
- **Beschreibung:** Für Titel, Meta-Description, Meta-Keywords, H1, H2 werden die ersten zwei Vorkommen in Dokumentreihenfolge als eigene Spalten gespeichert (`Title 1`, `Title 2`, `Title 1 Length`, `Title 2 Length` usw.). Die Spalte „Occurrences“ zählt Vorkommen: Bei Titel, Meta-Description und Meta-Keywords gedeckelt auf 2; bei H1 und H2 die tatsächliche Gesamtzahl (ungedeckelt).
- **Eingaben/Konfiguration:** keine (Erweiterung auf alle Vorkommen: über Custom Extraction, anderer Bereich).
- **Ausgabe:** Spalten je Element-Tab, siehe ONPAGE-090/100/110/120/130.
- **Akzeptanzkriterien:**
  - [ ] Seite mit 3 `<title>` → Occurrences = 2, Title 1 und Title 2 gefüllt, dritter ignoriert.
  - [ ] Seite mit 5 `<h1>` → Occurrences = 5, h1-1 und h1-2 gefüllt.
  - [ ] Leeres Element zählt als Vorkommen (Occurrences +1) mit leerem Wert und Länge 0.
  - [ ] Seite ohne Element → Occurrences = 0, alle Wertspalten leer.
- **Testfixture:** `onpage-titles/multi-3.html` (3 Titel), `onpage-h1/five-h1.html`.

### ONPAGE-009 Grundmenge der Element-Tabs
- **Priorität:** P0
- **Beschreibung:**
  - **Element-Tabs** (Page Titles, Meta Description, Meta Keywords, H1, H2): Grundmenge („All“) sind alle **internen HTML-Seiten mit Statuscode 2xx** (**Annahme A-5**; Weiterleitungen, Fehlerseiten und blockierte URLs fehlen, weil deren Inhalt nicht ausgewertet wird).
  - **PDF-Dokumente** (seit einer früheren Referenzversion): Interne PDFs mit Status 2xx werden geparst; ihr Dokumenttitel und ihre Keywords aus den Dokumenteigenschaften erscheinen ohne weitere Konfiguration in den Spalten `Title 1` und `Meta Keywords 1` des Internal-Tabs (ONPAGE-011, ONPAGE-166). Ob PDFs zusätzlich in der Grundmenge der Tabs Page Titles und Meta Keywords geführt werden, legt die öffentliche Doku nicht fest; das wird im Paritätstest V-2 geklärt (**Annahme A-5b**: nein, PDFs erscheinen nur im Internal-Tab). In Meta Description, H1 und H2 erscheinen PDFs nie.
  - **Content-Tab:** Grundmenge sind interne HTML-Seiten mit Status 2xx **und** – sofern `pdf.store = true` – interne PDFs mit Status 2xx (Wortzahl, Lesbarkeit, Rechtschreibung/Grammatik; ONPAGE-166).
  - **Indexierbarkeit:** schränkt die Grundmenge nicht ein; sie wird als Spalte gezeigt. Ist `issues.ignore_non_indexable = true` (Referenz: Option unter Spider-Konfiguration → Advanced, empfohlen abzuschalten bei komplett `noindex`-geschalteten Staging-Umgebungen), werden nicht indexierbare Seiten aus allen **Problemfiltern** (alle Filter außer „All“) der Element-Tabs sowie aus Low Content Pages entfernt; „All“ bleibt unverändert. Default-Stellung in der öffentlichen Doku nicht angegeben; OpenSpider-Standard ist **an** (`true`), festgelegt in CRAWL-089 (führend für den Schalter, A-51).
- **Eingaben/Konfiguration:** `pdf.store`, `issues.ignore_non_indexable`.
- **Ausgabe:** Grundmenge je Element-Tab und Content-Tab.
- **Akzeptanzkriterien:**
  - [ ] Eine interne 404-HTML-Seite mit `<title>` erscheint nicht im Page-Titles-Tab.
  - [ ] Eine interne `noindex`-Seite erscheint im Page-Titles-Tab → All (Indexability = Non-Indexable).
  - [ ] Externe HTML-Seiten erscheinen in keinem Element-Tab.
  - [ ] Interne `doc.pdf` mit Dokumenttitel „Preisliste 2026“ und Keywords „preise, liste“: Internal-Tab zeigt `Title 1` = „Preisliste 2026“ und `Meta Keywords 1` = „preise, liste“ (ohne `pdf.store`).
  - [ ] `doc.pdf` erscheint nicht in Meta Description, H1, H2 (immer) und gemäß A-5b nicht in Page Titles / Meta Keywords (Paritätstest V-2 entscheidet; bei Abweichung wird dieses Kriterium angepasst).
  - [ ] Content-Tab → All enthält `doc.pdf` genau dann, wenn `pdf.store = true`.
  - [ ] `issues.ignore_non_indexable = true`: `noindex.html` ohne `<title>` steht in Page Titles → All, aber nicht in Page Titles → Missing; mit `false` steht sie in Missing.
  - [ ] Default-Konfiguration (ohne Änderung, Standard nach CRAWL-089 = an): `noindex.html` ohne `<title>` steht nicht in Page Titles → Missing.
- **Testfixture:** `onpage-titles/` mit `noindex.html` (ohne Titel), Route `/gone` (404, HTML-Body mit Titel), Link auf `{{BASE_URL}}` des Fremdhosts (F-HOST2), `doc.pdf` (mit gesetzten Dokumenteigenschaften Title/Keywords, erzeugt per Skript im Fixture-Generator).

---

## 2 Internal

Grundmenge („All“): alle internen URLs, die gecrawlt oder entdeckt wurden – inklusive Weiterleitungen, Fehlerseiten, durch robots.txt blockierter URLs und Ressourcen (Bilder, CSS, JS, PDF …), sofern diese gemäß Crawl-Konfiguration gespeichert werden.

### ONPAGE-010 Internal: Basisspalten
- **Priorität:** P0
- **Beschreibung:** Grundspalten, die in nahezu allen Tabs wiederkehren.
- **Eingaben/Konfiguration:** keine.
- **Ausgabe (Spalten, in dieser Reihenfolge):** `Address` (absolute URL in Anzeigeform, Non-ASCII dekodiert), `Content` (Content-Type-Header ohne Änderung, z. B. `text/html; charset=utf-8`), `Status Code` (Ganzzahl; 0 bei keiner Antwort/Blockierung), `Status` (Reason-Phrase der Antwort, z. B. `OK`, `Not Found`; bei HTTP/2 ohne Reason-Phrase der Standardtext zum Code; bei Fehlern der Status-Text aus ONPAGE-065), `Indexability`, `Indexability Status`.
- **Akzeptanzkriterien:**
  - [ ] 200-Seite: Status Code 200, Status „OK“, Content wie gesendet.
  - [ ] Server sendet `HTTP/1.1 200 Alles gut` → Status „Alles gut“ (Reason-Phrase wird übernommen).
  - [ ] HTTP/2-Antwort 404 → Status „Not Found“.
  - [ ] robots.txt-blockierte URL: Status Code 0, Status „Blocked by Robots.txt“, Indexability „Non-Indexable“.
  - [ ] Antwort ohne Content-Type-Header: Spalte Content leer.
  - [ ] Address einer URL `/caf%C3%A9` wird als `/café` angezeigt; die angefragte Form steht in `URL Encoded Address` (ONPAGE-018).
- **Testfixture:** `onpage-internal/` mit `index.html` (verlinkt alle Fälle), Routen: `/custom-reason` (Status 200, eigener Reason – benötigt Route-Option `reason`, F-Erweiterung), `/blocked/x.html` (robots.txt `Disallow: /blocked/`), `/no-ct` (ohne Content-Type), `/caf%C3%A9`.

### ONPAGE-011 Internal: Onpage-Element-Spalten
- **Priorität:** P0
- **Beschreibung:** Der Internal-Tab zeigt für HTML-Seiten die Kernwerte der Element-Tabs und Direktiven als Spalten.
- **Eingaben/Konfiguration:** Speicher-Schalter der Extraktion (anderer Bereich); abgeschaltete Elemente erscheinen nicht als Spalten.
- **Ausgabe (Spalten):** `Title 1`, `Title 1 Length`, `Title 1 Pixel Width`, `Meta Description 1`, `Meta Description 1 Length`, `Meta Description 1 Pixel Width`, `Meta Keywords 1`, `Meta Keywords 1 Length`, `H1-1`, `H1-1 Length`, `H2-1`, `H2-1 Length`, `Meta Robots 1`, `X-Robots-Tag 1`, `Meta Refresh 1`, `Canonical Link Element 1`, `rel="next" 1`, `rel="prev" 1`, `HTTP rel="next" 1`, `HTTP rel="prev" 1`; danach (am Ende der Tabelle) die zweiten Vorkommen `Title 2`, `Meta Description 2`, `H1-2`, `H2-2` usw. inkl. Längen.
- **Akzeptanzkriterien:**
  - [ ] Werte sind identisch mit den Werten der jeweiligen Element-Tabs (gleiche Normalisierung, gleiche Länge).
  - [ ] Bilder, CSS, JavaScript und andere Nicht-HTML-URLs haben leere Element-Spalten.
  - [ ] Interne HTML-Seiten ohne Status 2xx (Weiterleitungen, 4xx, 5xx, ohne Antwort) haben leere Element-Spalten einschließlich Längen und Pixelbreiten, auch wenn ihr Körper `<title>`/`<h1>` enthält. Ihr Inhalt wird nicht ausgewertet (A-5, ONPAGE-009). Beispiel: `/e410` (410, `<title>Error 410</title>`) → `Title 1` und `H1-1` leer (Paritätsbefund M1, Befund N-01, K-60; die Referenz schreibt für die Längen 0, OpenSpider lässt sie leer wie bei Nicht-HTML, DEV-003).
  - [ ] PDFs: `Title 1` und `Meta Keywords 1` stammen aus den Dokumenteigenschaften (ONPAGE-166), alle übrigen Element-Spalten (Meta Description, H1, H2, Meta Robots, Canonical …) bleiben leer; `X-Robots-Tag 1` wird wie bei jeder URL aus dem Header gefüllt.
  - [ ] `Meta Robots 1` enthält den `content` des ersten `<meta name="robots">` (Name case-insensitiv), z. B. `noindex, follow`.
  - [ ] `X-Robots-Tag 1` enthält den Wert des ersten `X-Robots-Tag`-Headers.
  - [ ] `Meta Refresh 1` enthält den `content` des ersten `<meta http-equiv="refresh">`, z. B. `5; url=/ziel`.
  - [ ] `Canonical Link Element 1` enthält die absolut aufgelöste URL aus `<link rel="canonical">`, in der Anzeige ohne Fragment wie im Canonicals-Tab (K2-96); die gespeicherte Originalannotation bleibt als TECH-022-Beleg erhalten.
  - [ ] `HTTP rel="next" 1` stammt aus einem `Link`-Header mit `rel="next"`.
- **Testfixture:** `onpage-internal/elements.html` mit allen Elementen; Route `/elements-header` mit `X-Robots-Tag: noindex` und `Link: <{{BASE_URL}}/p2>; rel="next"`.

### ONPAGE-012 Internal: Size, Transferred, Total Transferred
- **Priorität:** P0
- **Beschreibung:** `Size` = Größe der Ressource in Bytes. Für HTML-Seiten die Größe des **unkomprimierten** HTML. Für andere Ressourcen der Wert des `Content-Length`-Headers, 0 wenn er fehlt (**Annahme A-6**: für heruntergeladene Bilder wird stattdessen die tatsächliche Körpergröße genutzt, siehe ONPAGE-173). `Transferred` = tatsächlich über das Netz übertragene Bytes des Körpers, ohne Statuszeile, Header und Chunk-Rahmen (bei gzip/br kleiner als Size; K-02, CRAWL-022 angeglichen). `Total Transferred` = Transferred der Seite plus aller beim Rendern geladenen Ressourcen (nur im JS-Rendering-Modus, sonst = Transferred).
- **Eingaben/Konfiguration:** Rendering-Modus.
- **Ausgabe:** Spalten `Size (bytes)`, `Transferred (bytes)`, `Total Transferred (bytes)`; UI zeigt KB mit einer Nachkommastelle, Exporte in Bytes.
- **Akzeptanzkriterien:**
  - [ ] Unkomprimierte 10 000-Byte-HTML-Seite: Size = Transferred = 10 000.
  - [ ] Gleiche Seite gzip-komprimiert ausgeliefert: Size = 10 000, Transferred = komprimierte Länge (< 10 000).
  - [ ] PDF ohne Content-Length (chunked): Size = 0 (A-6).
  - [ ] Ohne Rendering: Total Transferred = Transferred.
  - [ ] [P1] Mit Rendering und 2 Bildern à 5 000 Byte: Total Transferred ≥ Transferred + 10 000 (Total Transferred mit Ressourcen setzt JS-Rendering voraus).
- **Testfixture:** `onpage-internal/size-10k.html` (exakt 10 000 Byte), Route `/size-10k-gz` (F-RAW gzip), Route `/chunked.pdf`.

### ONPAGE-013 Internal: Word Count, Text Ratio
- **Priorität:** P0
- **Präzisierung (K2-96):** In den Body reparierte `title`-Elemente bilden für die Text Ratio eine Textgrenze. Normale Inline-Elemente bleiben zusammenhängend; die Wortzählungsregeln bleiben ONPAGE-141.
- **Beschreibung:** `Word Count` gemäß ONPAGE-141 (nur Inhaltsbereich). `Text Ratio` = (Anzahl Zeichen des sichtbaren Textes im `<body>` ohne Markup, ohne `script`/`style`/`noscript`/`template`) ÷ (Anzahl Zeichen des gesamten HTML-Quelltextes) × 100, drei Nachkommastellen (K2-87, SQ-08; vorher eine) (**Annahme A-7**: Text Ratio nutzt den gesamten Body, nicht nur den Inhaltsbereich).
- **Eingaben/Konfiguration:** Inhaltsbereich (ONPAGE-140).
- **Ausgabe:** Spalten `Word Count`, `Text Ratio`.
- **Akzeptanzkriterien:**
  - [ ] Seite mit `<body><p>eins zwei drei</p></body>` (ohne nav/footer) → Word Count 3.
  - [ ] Text in `<nav>` und `<footer>` zählt standardmäßig nicht zum Word Count.
  - [ ] Inhalt von `<script>`/`<style>` zählt weder zum Word Count noch zum Text Ratio.
  - [ ] Text Ratio ∈ [0, 100]; Seite ohne Body-Text → 0.
  - [ ] Nicht-HTML-URLs → beide Spalten leer; Ausnahme: interne PDF mit `pdf.store = true` → `Word Count` gefüllt (ONPAGE-166), `Text Ratio` leer.
  - [ ] HTML-Seiten ohne Status 2xx → beide Spalten leer (A-5, K-60).
- **Testfixture:** `onpage-content/wordcount-3.html`, `onpage-content/nav-footer.html` (10 Wörter im main, 50 im nav, 20 im footer → 10).

### ONPAGE-014 Internal: Crawl Depth, Folder Depth
- **Priorität:** P0
- **Beschreibung:** `Crawl Depth` = Anzahl Klicks (Link-Hops) von der Start-URL auf kürzestem gefundenen Weg; Start-URL = 0. Eine Weiterleitung zählt als zusätzliche Ebene. `Folder Depth` = Anzahl Verzeichnisebenen im Pfad: Pfadsegmente, auf die ein `/` folgt (**Annahme A-8**): `/` → 0, `/a/` → 1, `/a/b.html` → 1, `/a/b/` → 2, `/a/b` → 1.
- **Eingaben/Konfiguration:** keine.
- **Ausgabe:** Spalten `Crawl Depth`, `Folder Depth`.
- **Akzeptanzkriterien:**
  - [ ] Start → A → B: Tiefe 0, 1, 2.
  - [ ] Start → `/r` (301) → `/ziel`: `/r` Tiefe 1, `/ziel` Tiefe 2.
  - [ ] Wird B später über einen kürzeren Weg gefunden, gilt die kleinere Tiefe (Breitensuche-Semantik).
  - [ ] In Listenmodus hochgeladene URLs haben Tiefe 0 (Verweis auf Modus-Spec).
  - [ ] Folder-Depth-Beispiele aus der Beschreibung liefern exakt die genannten Werte; Query-Strings werden ignoriert (`/a/?x=1` → 1).
  - [ ] Leere Pfadsegmente (doppelter Schrägstrich) zählen nicht: `/urls/a//b.html` → 2 (Paritätsbefund M1, Befund N-05, K-60).
- **Testfixture:** `onpage-internal/depth/` mit Kette `index → a.html → b.html`, Route `/r` → `/ziel` (301).

### ONPAGE-015 Internal: Link-Zählspalten
- **Priorität:** P0
- **Beschreibung:** Zählungen aus dem Link-Graphen (Hyperlinks `<a href>` und gleichwertige Linkelemente gemäß Link-Spec):
  - `Inlinks`: Anzahl interner Hyperlinks auf die URL (jede Link-Instanz zählt, auch mehrfach von derselben Seite).
  - `Unique Inlinks`: Anzahl verschiedener interner Quellseiten.
  - `% of Total`: Unique Inlinks von internen HTML-Seiten mit Status 2xx ÷ Anzahl aller gecrawlten internen HTML-Seiten (Status 2xx) × 100 (Paritätsbefund M1, Befund B-04, K-56: die Referenz zählt alle 2xx, Fixture `/s201`).
  - `Outlinks`: Anzahl interner Hyperlinks von der URL; `Unique Outlinks`: Anzahl verschiedener interner Ziel-URLs.
  - `External Outlinks`: Anzahl Hyperlinks auf externe URLs; `Unique External Outlinks`: verschiedene externe Ziele.
- **Eingaben/Konfiguration:** keine.
- **Ausgabe:** Spalten wie beschrieben (Ganzzahlen; % mit zwei Nachkommastellen).
- **Akzeptanzkriterien:**
  - [ ] Seite A verlinkt Z dreimal, Seite B einmal → Z: Inlinks 4, Unique Inlinks 2.
  - [ ] A verlinkt Z dreimal und Y einmal → A: Outlinks 4, Unique Outlinks 2.
  - [ ] Selbstverweise (A → A) zählen als Inlink und Outlink (**Annahme A-9**).
  - [ ] Bei 4 gecrawlten internen 2xx-HTML-Seiten (davon eine mit Status 201) und 2 eindeutigen verlinkenden Seiten → % of Total = 50,00.
  - [ ] Links mit `rel="nofollow"` werden gezählt (Nofollow beeinflusst nur das Folgen, nicht die Zählung) – Verweis Link-Spec.
  - [ ] Ressourcen-Referenzen (img, script, link stylesheet) zählen nicht als Hyperlink-Inlinks (sie erscheinen in eigenen Spalten/Lower-Tabs).
- **Testfixture:** `onpage-internal/links/` mit A (3× Z, 1× Y, 1× externer Link zweimal), B (1× Z), Z, Y.

### ONPAGE-016 Internal: JS-Link-Zählspalten
- **Priorität:** P1
- **Beschreibung:** Im JS-Rendering-Modus: `Unique JS Inlinks`, `Unique JS Outlinks`, `Unique External JS Outlinks` = Anzahl eindeutiger Links, die nur im gerenderten DOM vorkommen, nicht im Roh-HTML.
- **Eingaben/Konfiguration:** Rendering = JavaScript.
- **Ausgabe:** drei Spalten; ohne Rendering leer.
- **Akzeptanzkriterien:**
  - [ ] Ein per `document.body.appendChild` eingefügter Link auf `/js-only` erhöht `Unique JS Outlinks` der Quelle um 1 und `Unique JS Inlinks` von `/js-only` um 1.
  - [ ] Ein Link, der sowohl im Roh-HTML als auch gerendert vorkommt, zählt nicht als JS-Link.
  - [ ] Ohne Rendering sind die Spalten leer (nicht 0).
- **Testfixture:** `onpage-internal/js-links.html` mit Inline-Script, das einen internen und einen externen Link einfügt.

### ONPAGE-017 Internal: Link Score (Spaltenanbindung)
- **Priorität:** P1
- **Verweis:** siehe TECH-145 (führend). Die Spalte `Link Score` erscheint zusätzlich im Internal-Tab.

### ONPAGE-018 Internal: Technische Spalten
- **Priorität:** P0
- **Beschreibung:**
  - `Hash`: MD5 (hex, 32 Zeichen, Kleinbuchstaben) über den vollständigen, dekomprimierten Antwortkörper der HTML-Seite (Bytes wie empfangen). **Nur für HTML-Antworten** (Inhaltsklasse HTML, ONPAGE-020); Bilder, CSS, JavaScript, Text, JSON, SVG und Antworten ohne Inhaltstyp (z. B. 301 mit leerem Körper) haben einen leeren `Hash` (Paritätsbefund M1, Befund N-06, K-60). Ist der Speicher-Schalter „Hash“ aus, entfällt auch die Erkennung exakter Duplikate (ONPAGE-143).
  - `Response Time`: Sekunden vom Senden der Anfrage bis zum vollständigen Empfang des Körpers, drei Nachkommastellen.
  - `Last-Modified`: Rohwert des Headers, sonst leer.
  - `Redirect URL`: absolut aufgelöstes Ziel einer Weiterleitung (HTTP-Location, Meta-Refresh-URL, HTTP-Refresh-URL, JS-Ziel), sonst leer. Bei einem Selbst-Refresh (Meta Refresh ohne URL oder auf die eigene URL, `Refresh`-Header auf die eigene URL; CRAWL-015) steht hier die eigene Adresse und in `Redirect Type` `Meta Refresh Redirect` bzw. `HTTP Refresh Redirect` (Paritätsbefund M1, Befund N-07, K-57). Ein `Refresh`-Header **ohne** URL ist keine Weiterleitung: `Redirect URL` und `Redirect Type` bleiben leer (Paritätsbefund M1-P31, Fixture `parity-response` `/http-refresh-nourl.html`, K-69).
  - `Redirect Type`: `HTTP Redirect`, `HSTS Policy`, `JavaScript Redirect`, `Meta Refresh Redirect`, `HTTP Refresh Redirect` (Letzteres als eigene Ausprägung, **Annahme A-10**).
  - `HTTP Version`: z. B. `1.1` oder `2`, wie ausgehandelt.
  - `URL Encoded Address`: die tatsächlich angefragte URL, Non-ASCII-Zeichen prozentkodiert (UTF-8, Großbuchstaben-Hex).
  - `Crawl Timestamp`: Zeitpunkt der Anfrage (ISO 8601, lokale Zeitzone mit Offset).
- **Eingaben/Konfiguration:** Speicher-Schalter „Hash“, „Response Time“, „Last-Modified“ (bei Abschaltung Spalte ausgeblendet).
- **Ausgabe:** Spalten wie beschrieben.
- **Akzeptanzkriterien:**
  - [ ] Zwei byte-identische Seiten haben gleichen Hash; ein zusätzliches Leerzeichen ändert ihn.
  - [ ] Hash einer gzip-ausgelieferten Seite = Hash derselben unkomprimierten Seite.
  - [ ] Route mit `delay_ms = 500` → Response Time ≥ 0,500.
  - [ ] Route mit `Location: /ziel` relativ → Redirect URL absolut (`{{BASE_URL}}/ziel`).
  - [ ] Pfad `/straße` → URL Encoded Address endet auf `/stra%C3%9Fe`.
  - [ ] Fehlt `Last-Modified`, ist die Spalte leer (nicht „0“ oder Datum 1970).
  - [ ] `Hash` ist für `/assets/logo.png` (Bild) und eine 301-Antwort mit leerem Körper leer, für jede 200-HTML-Seite gefüllt (K-60).
  - [ ] `content="30"` ohne URL auf `/m.html` → `Redirect URL` = `{{BASE_URL}}/m.html`, `Redirect Type` = `Meta Refresh Redirect` (K-57).
- **Testfixture:** `onpage-internal/tech/` mit `same1.html`/`same2.html` (identisch), `same3.html` (+1 Leerzeichen), Routen `/slow` (delay 500), `/rel-redirect` (302 → `/ziel`), `/straße`, `/lm` (`Last-Modified`-Header).

### ONPAGE-019 Internal: Content-Analyse-Spalten
- **Priorität:** P1
- **Beschreibung:** Der Internal-Tab spiegelt Werte aus dem Content-Tab: `Closest Similarity Match`, `No. Near Duplicates`, `Spelling Errors`, `Grammar Errors`, `Language`, `Flesch Reading Ease Score`, `Readability`; mit Embeddings zusätzlich `Closest Semantically Similar Address`, `Semantic Similarity Score`, `No. Semantically Similar`, `Semantic Relevance Score`.
- **Eingaben/Konfiguration:** wie in den jeweiligen Content-Funktionen.
- **Ausgabe:** Spalten; leer, wenn die Funktion deaktiviert ist oder die Crawl-Analyse fehlt.
- **Akzeptanzkriterien:**
  - [ ] Werte im Internal-Tab sind für jede URL identisch mit dem Content-Tab.
  - [ ] Deaktivierte Funktionen → Spalten leer, keine 0.
- **Testfixture:** `onpage-content-dupes` (ONPAGE-144).

### ONPAGE-020 Inhaltstyp-Klassifizierung
- **Priorität:** P0
- **Beschreibung:** Jede URL wird anhand des MIME-Typs aus dem `Content-Type`-Header (Parameter wie `charset` ignoriert, case-insensitiv, Leerraum getrimmt) genau einer Klasse zugeordnet:

| Klasse | MIME-Typen |
|---|---|
| HTML | `text/html`, `application/xhtml+xml` |
| JavaScript | `application/javascript`, `text/javascript`, `application/x-javascript`, `application/ecmascript`, `text/ecmascript`, `application/x-ecmascript`, `text/x-javascript` |
| CSS | `text/css` |
| Images | `image/*` (inkl. `image/svg+xml`, `image/webp`, `image/avif`) |
| PDF | `application/pdf` |
| Plugins | `application/x-shockwave-flash`, `application/futuresplash` |
| Media | `video/*`, `audio/*` |
| Fonts | `font/*`, `application/font-woff`, `application/font-woff2`, `application/x-font-ttf`, `application/x-font-otf`, `application/vnd.ms-fontobject` |
| XML | `application/xml`, `text/xml`, `application/rss+xml`, `application/atom+xml` |
| Other | jeder andere syntaktisch gültige MIME-Typ (z. B. `text/plain`, `application/json`, `application/zip`, `application/msword`) |
| Unknown | kein Content-Type, syntaktisch ungültiger Wert, keine Antwort, durch robots.txt blockiert |

  Ist „Assume pages are HTML“ (Crawl-Spec) aktiv, wird eine Antwort ohne Content-Type als HTML klassifiziert.
- **Eingaben/Konfiguration:** „Assume pages are HTML“.
- **Ausgabe:** Klassenwert pro URL; Basis der Filter in ONPAGE-021 … 024 und ONPAGE-026.
- **Akzeptanzkriterien:**
  - [ ] `text/html; charset=UTF-8` → HTML; `TEXT/HTML` → HTML.
  - [ ] `image/svg+xml` → Images (nicht XML).
  - [ ] `application/json` → Other.
  - [ ] Fehlender Header → Unknown; mit „Assume pages are HTML“ → HTML.
  - [ ] Wert `foo` (ohne `/`) → Unknown.
  - [ ] Jede URL ist genau einer Klasse zugeordnet (Summe aller Klassenfilter = All).
- **Testfixture:** `onpage-ctype/` mit Routen pro Zeile der Tabelle (z. B. `/t-html`, `/t-xhtml`, `/t-js`, `/t-css`, `/t-png`, `/t-svg`, `/t-pdf`, `/t-swf`, `/t-mp4`, `/t-woff2`, `/t-xml`, `/t-json`, `/t-none`, `/t-invalid`) und `/blocked/x` (robots.txt).

### ONPAGE-021 Internal-Filter HTML
- **Priorität:** P0
- **Beschreibung:** Filter „HTML“ = interne URLs der Klasse HTML (ONPAGE-020), unabhängig vom Statuscode.
- **Eingaben/Konfiguration:** keine.
- **Ausgabe:** Filter `HTML`.
- **Akzeptanzkriterien:**
  - [ ] Enthält `/t-html` und `/t-xhtml`, nicht `/t-xml`.
  - [ ] Enthält eine interne 404-Seite mit `text/html` (Statuscode egal).
  - [ ] Eine 301-Antwort mit `text/html`-Body ist enthalten.
- **Testfixture:** `onpage-ctype/`.

### ONPAGE-022 Internal-Filter JavaScript, CSS, Images, PDF
- **Priorität:** P0
- **Beschreibung:** Je ein Filter pro Klasse `JavaScript`, `CSS`, `Images`, `PDF` gemäß ONPAGE-020, eingeschränkt auf interne URLs.
- **Eingaben/Konfiguration:** Ressourcen werden nur aufgeführt, wenn ihr Crawl laut Konfiguration aktiv ist (Crawl-Spec: „Check Images/CSS/JavaScript/…“).
- **Ausgabe:** vier Filter.
- **Akzeptanzkriterien:**
  - [ ] JavaScript enthält `/t-js` und eine `text/javascript`-Ressource.
  - [ ] CSS enthält `/t-css`.
  - [ ] Images enthält `/t-png` und `/t-svg`.
  - [ ] PDF enthält `/t-pdf`.
  - [ ] Ist „Check Images“ abgeschaltet, ist der Images-Filter leer, obwohl `<img>` vorkommen.
- **Testfixture:** `onpage-ctype/`.

### ONPAGE-023 Internal-Filter Flash, Other (+ Erweiterung Media, Fonts, XML)
- **Priorität:** P1
- **Beschreibung:** Filter `Plugins` (interne Klasse `Plugins`, d. h. `.swf`-Inhalte gemäß ONPAGE-020; bis K2-87 „Flash“), `Media`, `Fonts`, `XML` und `Other`. Die Referenz führt diese Klassen als eigene Filter (Overview `parity-m2-onpage`: Media, Fonts, XML, Plugins, Other je 1); **Annahme A-49 ist widerlegt** (K2-87, SQ-07). `Other` enthält nur URLs ohne eine der übrigen Klassen (z. B. `application/octet-stream`, `application/json`, `text/plain`). Die Filter sind unabhängig von `ui.parity_filters` sichtbar; die Klassenzuordnung in ONPAGE-020 (Speicherwert) bleibt gleich.
- **Eingaben/Konfiguration:** Medien-Links (`<video>`, `<audio>`, `<source>`) werden nur entdeckt, wenn die entsprechende Crawl-Option aktiv ist. `ui.parity_filters` wirkt nicht mehr auf diese Filter (K2-87).
- **Ausgabe:** Filter `Plugins`, `Media`, `Fonts`, `XML`, `Other`.
- **Akzeptanzkriterien:**
  - [ ] Der Filter heißt in UI und Export „Plugins“ (vorher „Flash“) und enthält `/t-swf`; die Filter-Kennung in `M1-filters.toml`/`M2-filters.toml` stellt M2-N07 um.
  - [ ] Media enthält `/t-mp4`; Fonts `/t-woff2`; XML `/t-xml` und `/sitemap.xml`; Other enthält nur `/t-json` und `text/plain`-Dokumente – bei `ui.parity_filters` an und aus gleich.
  - [ ] Summe aller Inhaltstyp-Filter = All.
- **Testfixture:** `onpage-ctype/` (Läufe mit `ui.parity_filters` an und aus, Ergebnis gleich).

### ONPAGE-024 Internal-Filter Unknown
- **Priorität:** P0
- **Beschreibung:** Filter „Unknown“ = interne URLs der Klasse Unknown: fehlender oder ungültiger Content-Type, keine Antwort (Timeout, Verbindungsfehler), durch robots.txt blockiert.
- **Eingaben/Konfiguration:** keine.
- **Ausgabe:** Filter `Unknown`.
- **Akzeptanzkriterien:**
  - [ ] Enthält `/t-none`, `/t-invalid`, `/blocked/x` und eine Timeout-URL (`/slow` mit delay > Timeout).
  - [ ] Enthält keine URL mit gültigem Content-Type.
- **Testfixture:** `onpage-ctype/` + Route `/timeout` (delay_ms 3000, Crawl-Timeout im Test 1 s).

---

## 3 External

### ONPAGE-025 External: Definition und Spalten
- **Priorität:** P0
- **Beschreibung:** Der External-Tab listet alle externen URLs (0.1), die über Hyperlinks oder Ressourcen-Referenzen interner Seiten gefunden wurden. Externe URLs werden nur geprüft (Statuscode abfragen), nicht weiter gecrawlt; ihre Links werden nicht verfolgt.
- **Eingaben/Konfiguration:** „Check external links“ (Crawl-Spec). Abgeschaltet → externe URLs werden gelistet, aber nicht angefragt (Status leer/0, Status „Not Checked“ – **Annahme A-11**) oder gar nicht gelistet (Referenzverhalten verifizieren).
- **Ausgabe (Spalten):**
  - Paritätsspalten (laut öffentlicher Tab-Doku), in dieser Reihenfolge: `Address`, `Content`, `Status Code`, `Status`, `Crawl Depth`, `Inlinks` (Anzahl **Hyperlink**-Instanzen (`<a href>`, `<area href>`) von internen Seiten wie ONPAGE-015; Weiterleitungen, Bild-, CSS-, JS- und andere Ressourcen-Referenzen zählen nicht – Paritätsbefund M1, Befund N-10, K-61).
  - **OpenSpider-Erweiterung** (in der Referenz-Doku nicht belegt; standardmäßig ausgeblendet, über die Spaltenauswahl einblendbar): `Response Time`, `Redirect URL`, `Redirect Type`.
- **Akzeptanzkriterien:**
  - [ ] Default-Layout zeigt genau die sechs Paritätsspalten in der genannten Reihenfolge; die drei Erweiterungsspalten sind ausgeblendet, werden aber im Export mit der Option „alle Spalten“ (UI-015, OUT-020) mit ausgegeben.
  - [ ] Link von `127.0.0.1` auf `localhost` (F-HOST2) erscheint im External-Tab, nicht im Internal-Tab.
  - [ ] Externe Seite wird genau einmal angefragt, ihre ausgehenden Links erscheinen nirgends.
  - [ ] Crawl Depth der externen URL = Tiefe der verlinkenden Seite + 1.
  - [ ] Inlinks = Anzahl Hyperlink-Instanzen von internen Seiten (2 Seiten × 2 Links → 4).
  - [ ] Eine externe URL, die per Hyperlink und zusätzlich als Ziel einer internen 301 erreicht wird, hat Inlinks 1. Ein externes Bild, das nur per `<img src>` eingebunden ist, hat Inlinks 0 (K-61).
  - [ ] Externe Subdomain gilt als extern, solange „alle Subdomains crawlen“ aus ist.
- **Testfixture:** `onpage-external/` mit `index.html` → zwei Links auf `http://localhost:<port>/ext.html`, `ext.html` enthält Link auf `/never-seen.html`; zusätzlich Route `/ext-404` am externen Host.

### ONPAGE-026 External: Inhaltstyp-Filter
- **Priorität:** P0
- **Beschreibung:** Gleiche Klassenfilter wie Internal (HTML, JavaScript, CSS, Images, PDF, Plugins, Media, Fonts, XML, Other, Unknown – ONPAGE-023, K2-87) über externe URLs, Logik ONPAGE-020.
- **Eingaben/Konfiguration:** keine.
- **Ausgabe:** Filter je Klasse.
- **Akzeptanzkriterien:**
  - [ ] Externes Bild (via `<img src>`) erscheint in External → Images.
  - [ ] Externe URL ohne Antwort (Port geschlossen) erscheint in External → Unknown.
  - [ ] Externe URL, die durch die robots.txt des externen Hosts blockiert ist (wenn robots.txt für externe beachtet wird), erscheint in Unknown.
  - [ ] Summe aller Klassenfilter = External All.
- **Testfixture:** `onpage-external/` mit `<img src="http://localhost:<port>/ext.png">`, Link auf `http://localhost:1/` (anderer Host, Port 1, Connection Refused; ein anderer Port am Start-Host wäre nach 0.1/CRAWL-006 intern, K-43).

---

## 4 Security

Grundmenge („All“): alle internen URLs (alle Inhaltstypen). Die seitenbasierten Filter (Mixed Content, Form …, Unsafe Cross-Origin, Protocol-Relative) betreffen nur interne HTML-Seiten mit Status 2xx; die Header-Filter ONPAGE-039 … 042 alle internen URLs mit 2xx-Antwort, unabhängig vom Inhaltstyp (K2-85, SQ-03; **Annahme A-12** in der Fassung „jede HTTP-Antwort“ ist widerlegt: die Referenz zählt 3xx/4xx nicht, Nicht-HTML mit 2xx schon); ONPAGE-038 prüft alle internen HTTPS-Antworten (A-14, kein Gegenbeleg).

### ONPAGE-030 Security: Spalten
- **Priorität:** P1
- **Beschreibung:** Spalten des Security-Tabs.
- **Eingaben/Konfiguration:** keine.
- **Ausgabe (Spalten):** `Address`, `Content`, `Status Code`, `Status`, `Indexability`, `Indexability Status`, `Canonical Link Element 1…n`, `Meta Robots 1…n`, `X-Robots-Tag 1…n` (alle gefundenen Vorkommen als fortlaufende Spalten).
- **Akzeptanzkriterien:**
  - [ ] Seite mit 2 Canonicals → Spalten `Canonical Link Element 1` und `2` gefüllt.
  - [ ] Spaltenanzahl wächst dynamisch mit dem Maximum an Vorkommen im Crawl.
- **Testfixture:** `onpage-security/two-canonicals.html`.

### ONPAGE-031 Security-Filter HTTP URLs
- **Priorität:** P0
- **Beschreibung:** Interne URLs mit Schema `http`.
- **Eingaben/Konfiguration:** keine.
- **Ausgabe:** Filter `HTTP URLs`.
- **Akzeptanzkriterien:**
  - [ ] Crawl auf `http://127.0.0.1:<port>/` → alle internen URLs im Filter.
  - [ ] Bei HTTPS-Start (F-TLS) erscheint ein interner Link `http://127.0.0.1:<tls-port>/alt` im Filter (gleicher Host, anderes Schema = intern).
  - [ ] Externe HTTP-URLs erscheinen nicht.
- **Testfixture:** `onpage-security/` (F-TLS) mit `index.html` → Link auf `http://`-Variante.

### ONPAGE-032 Security-Filter HTTPS URLs
- **Priorität:** P0
- **Beschreibung:** Interne URLs mit Schema `https`.
- **Eingaben/Konfiguration:** keine.
- **Ausgabe:** Filter `HTTPS URLs`.
- **Akzeptanzkriterien:**
  - [ ] HTTPS-Crawl: alle internen `https`-URLs im Filter; HTTP URLs ∪ HTTPS URLs = All.
- **Testfixture:** `onpage-security/` (F-TLS).

### ONPAGE-033 Security-Filter Mixed Content
- **Priorität:** P1
- **Beschreibung:** Interne HTML-Seiten, die per HTTPS ausgeliefert werden und mindestens eine Ressource über `http://` einbinden. Als Ressource zählen: `<img src|srcset>`, `<picture><source srcset>`, `<script src>`, `<link rel="stylesheet" href>`, `<iframe src>`, `<video src|poster>`, `<audio src>`, `<source src>`, `<track src>`, `<embed src>`, `<object data>`. Ein normaler Hyperlink `<a href="http://…">` ist **kein** Mixed Content.
- **Eingaben/Konfiguration:** keine.
- **Ausgabe:** Filter `Mixed Content`; die betroffenen HTTP-Ressourcen sind im Lower-Tab Outlinks sichtbar; Bulk-Export „Security > Mixed Content“ (Seite + Ressource).
- **Akzeptanzkriterien:**
  - [ ] HTTPS-Seite mit `<img src="http://…/a.png">` → im Filter.
  - [ ] HTTPS-Seite mit `<script src="http://…">` → im Filter.
  - [ ] HTTPS-Seite mit `<a href="http://…">` → nicht im Filter.
  - [ ] HTTP-Seite mit HTTP-Bild → nicht im Filter.
  - [ ] HTTPS-Seite mit relativem `<img src="/a.png">` → nicht im Filter.
  - [ ] Bulk-Export enthält je Zeile Quelle (Seite) und Ziel (HTTP-Ressource).
- **Testfixture:** `onpage-security/` (F-TLS): `mixed-img.html`, `mixed-js.html`, `mixed-css.html`, `mixed-iframe.html`, `link-only.html`, `relative.html`.

### ONPAGE-034 Security-Filter Form URL Insecure
- **Priorität:** P1
- **Beschreibung:** Interne HTML-Seiten mit mindestens einem `<form>`, dessen aufgelöste Action-URL das Schema `http` hat. Fehlt `action` oder ist sie leer, ist die Action die Seiten-URL selbst.
- **Eingaben/Konfiguration:** keine.
- **Ausgabe:** Filter `Form URL Insecure`.
- **Akzeptanzkriterien:**
  - [ ] HTTPS-Seite mit `<form action="http://…/send">` → im Filter.
  - [ ] HTTPS-Seite mit `<form action="/send">` → nicht im Filter.
  - [ ] HTTP-Seite mit `<form action="/send">` → im Filter (Action löst zu http auf) und zusätzlich in ONPAGE-035.
  - [ ] `<form>` ohne action auf HTTPS-Seite → nicht im Filter.
  - [ ] `action="javascript:void(0)"` oder `mailto:` → ignoriert.
- **Testfixture:** `onpage-security/form-*.html` (F-TLS) und `onpage-security-http/form-relative.html` (HTTP-Crawl).

### ONPAGE-035 Security-Filter Form on HTTP URL
- **Priorität:** P1
- **Beschreibung:** Interne HTML-Seiten mit Schema `http`, die mindestens ein `<form>`-Element enthalten (unabhängig von dessen Action).
- **Eingaben/Konfiguration:** keine.
- **Ausgabe:** Filter `Form on HTTP URL`.
- **Akzeptanzkriterien:**
  - [ ] HTTP-Seite mit Formular (Action https) → im Filter.
  - [ ] HTTP-Seite ohne Formular → nicht im Filter.
  - [ ] HTTPS-Seite mit Formular → nicht im Filter.
- **Testfixture:** `onpage-security-http/form-https-action.html`, `no-form.html`.

### ONPAGE-036 Security-Filter Unsafe Cross-Origin Links
- **Priorität:** P1
- **Beschreibung:** Interne HTML-Seiten mit mindestens einem Hyperlink (`<a>`, `<area>`) auf eine **externe** URL, der `target="_blank"` (Wert case-insensitiv) trägt und dessen `rel`-Attribut weder das Token `noopener` noch `noreferrer` enthält (Tokens per Leerraum getrennt, case-insensitiv). Hinweis in der UI: moderne Browser setzen `noopener` bei `_blank` implizit; das Issue ist daher niedrig priorisiert.
- **Eingaben/Konfiguration:** keine.
- **Ausgabe:** Filter `Unsafe Cross-Origin Links`; Lower-Tab Outlinks zeigt Spalte `Target`.
- **Akzeptanzkriterien:**
  - [ ] `<a href="http://localhost:…/x" target="_blank">` → im Filter.
  - [ ] Gleicher Link mit `rel="noopener"` → nicht; mit `rel="nofollow noreferrer"` → nicht; mit `rel="NOOPENER"` → nicht.
  - [ ] Interner Link mit `target="_blank"` ohne rel → nicht im Filter.
  - [ ] `target="_self"` / `target="foo"` → nicht im Filter.
- **Testfixture:** `onpage-security/xo-*.html` (F-HOST2).

### ONPAGE-037 Security-Filter Protocol-Relative Resource Links
- **Priorität:** P1
- **Beschreibung:** Interne HTML-Seiten, die mindestens eine Ressource (Elementliste wie ONPAGE-033) über eine schemalose URL einbinden, d. h. der Attributwert beginnt (nach Trim) mit `//`.
- **Eingaben/Konfiguration:** keine.
- **Ausgabe:** Filter `Protocol-Relative Resource Links`.
- **Akzeptanzkriterien:**
  - [ ] `<script src="//cdn.example.invalid/x.js">` → im Filter.
  - [ ] `<img src="//localhost:…/a.png">` → im Filter.
  - [ ] `<a href="//localhost/x">` (Hyperlink) → nicht im Filter (**Annahme A-13**).
  - [ ] `<img src="/a.png">` → nicht.
- **Testfixture:** `onpage-security-http/protorel-*.html`.

### ONPAGE-038 Security-Filter Missing HSTS Header
- **Priorität:** P1
- **Beschreibung:** Interne URLs, die per HTTPS geantwortet haben und keinen `Strict-Transport-Security`-Header senden (Headername case-insensitiv). HTTP-Antworten werden nicht geprüft, da Browser HSTS auf HTTP ignorieren (**Annahme A-14**). Nur Vorhandensein wird geprüft; `max-age=0` gilt als vorhanden.
- **Eingaben/Konfiguration:** keine.
- **Ausgabe:** Filter `Missing HSTS Header`.
- **Akzeptanzkriterien:**
  - [ ] HTTPS-Seite ohne Header → im Filter; mit `Strict-Transport-Security: max-age=31536000` → nicht.
  - [ ] HTTP-Seite ohne Header → nicht im Filter.
  - [ ] URL ohne Antwort → nicht im Filter.
- **Testfixture:** `onpage-security/` (F-TLS) Routen `/hsts-yes`, `/hsts-no`.

### ONPAGE-039 Security-Filter Missing Content-Security-Policy Header
- **Priorität:** P1
- **Beschreibung:** Interne URLs ohne `Content-Security-Policy`-Header. Nur Vorhandensein zählt, nicht Qualität. `Content-Security-Policy-Report-Only` und `<meta http-equiv="Content-Security-Policy">` zählen **nicht** als vorhanden (**Annahme A-15**).
- **Eingaben/Konfiguration:** keine.
- **Ausgabe:** Filter `Missing Content-Security-Policy Header`.
- **Akzeptanzkriterien:**
  - [ ] Header fehlt → im Filter; Header mit beliebigem Wert → nicht.
  - [ ] Nur Report-Only-Header → im Filter.
  - [ ] Nur Meta-CSP → im Filter.
  - [ ] Grundmenge (gilt ebenso für ONPAGE-040 … 042): 404- oder 301-Antwort ohne Header → nicht im Filter; Bild mit 200 ohne Header → im Filter (K2-85, SQ-03).
- **Testfixture:** `onpage-security-http/` Routen `/csp-yes`, `/csp-no`, `/csp-ro`, `csp-meta.html`.

### ONPAGE-040 Security-Filter Missing X-Content-Type-Options Header
- **Priorität:** P1
- **Beschreibung:** Interne URLs, deren `X-Content-Type-Options`-Header fehlt oder nicht den Wert `nosniff` hat (Vergleich nach Trim, case-insensitiv).
- **Eingaben/Konfiguration:** keine.
- **Ausgabe:** Filter `Missing X-Content-Type-Options Header`.
- **Akzeptanzkriterien:**
  - [ ] Fehlend → im Filter; `nosniff` → nicht; `NoSniff` → nicht; `sniff` → im Filter.
- **Testfixture:** `onpage-security-http/` Routen `/xcto-*`.

### ONPAGE-041 Security-Filter Missing X-Frame-Options Header
- **Priorität:** P1
- **Beschreibung:** Interne URLs, deren `X-Frame-Options`-Header fehlt oder weder `DENY` noch `SAMEORIGIN` ist (case-insensitiv, getrimmt). Eine CSP-Direktive `frame-ancestors` ersetzt den Header für diesen Filter **nicht** (**Annahme A-16**).
- **Eingaben/Konfiguration:** keine.
- **Ausgabe:** Filter `Missing X-Frame-Options Header`.
- **Akzeptanzkriterien:**
  - [ ] Fehlend → im Filter; `DENY` → nicht; `sameorigin` → nicht; `ALLOW-FROM https://x` → im Filter.
- **Testfixture:** `onpage-security-http/` Routen `/xfo-*`.

### ONPAGE-042 Security-Filter Missing Secure Referrer-Policy Header
- **Priorität:** P1
- **Beschreibung:** Interne URLs ohne `Referrer-Policy`-Header oder mit einem effektiven Wert außerhalb von {`no-referrer-when-downgrade`, `strict-origin-when-cross-origin`, `no-referrer`, `strict-origin`}. Enthält der Header eine kommagetrennte Liste, gilt (wie im Browser) das letzte gültige Token als effektiver Wert.
- **Eingaben/Konfiguration:** keine.
- **Ausgabe:** Filter `Missing Secure Referrer-Policy Header`.
- **Akzeptanzkriterien:**
  - [ ] Fehlend → im Filter.
  - [ ] `strict-origin-when-cross-origin` → nicht; `no-referrer` → nicht.
  - [ ] `unsafe-url` → im Filter; `origin` → im Filter; `same-origin` → im Filter.
  - [ ] `unsafe-url, strict-origin` → nicht (letztes gültiges Token).
  - [ ] `strict-origin, foo` → nicht (`foo` ungültig, letztes gültiges = strict-origin).
- **Testfixture:** `onpage-security-http/` Routen `/rp-*`.

### ONPAGE-043 Security-Filter Bad Content Type
- **Priorität:** P1
- **Beschreibung:** Interne URLs mit 2xx-Antwort, deren deklarierter Content-Type nicht zum tatsächlichen Inhalt passt oder veraltet ist. Ein fehlender oder explizit leerer `Content-Type`-Header ist kein Treffer; die MIME-Syntax allein wird nicht geprüft. Die bisherigen Regeln 2–4 gelten ausschließlich für 2xx-Antworten (K2-94). Regeln:
  2. Die URL hat eine **bekannte** Dateiendung (`.html/.htm`, `.css`, `.js/.mjs`, `.png`, `.jpg/.jpeg`, `.gif`, `.webp`, `.avif`, `.svg`, `.ico`, `.pdf`, `.xml`, `.json`, `.woff/.woff2/.ttf/.otf`, `.mp4/.webm/.mp3`) und der deklarierte Typ gehört nicht zur erwarteten Klasse (ONPAGE-020) → Treffer. URLs ohne oder mit unbekannter Endung werden nach Regel 2 nicht geprüft.
  3. Inhalts-Sniffing: Magic Bytes widersprechen dem deklarierten Typ (z. B. PNG-Signatur bei `text/html`, HTML-Markup bei `image/jpeg`) → Treffer. Beim generischen Typ `application/octet-stream` findet kein Sniffing statt (K2-87, SQ-13c; Beleg Referenz `parity-m2-security` `/submit`).
  4. Veraltete Typen aus einer konfigurierbaren Liste → Treffer. Default-Liste für Parität: `text/javascript`, `application/x-javascript` (Referenz empfiehlt `application/javascript`). Hinweis: RFC 9239 hat `text/javascript` wieder zum Standard erklärt; die Liste ist daher abschaltbar.
- **Eingaben/Konfiguration:** `security.obsolete_mime_types` (Liste).
- **Ausgabe:** Filter `Bad Content Type`; zusätzliche Spalte im Filter `Expected Content Type` (Erweiterung OpenSpider).
- **Akzeptanzkriterien:**
  - [ ] `/image.jpg` mit `application/javascript` → Treffer.
  - [ ] `/script.js` mit `text/javascript` → Treffer (Default-Liste); nach Leeren der Liste → kein Treffer.
  - [ ] `/script.js` mit `application/javascript` → kein Treffer.
  - [ ] `/page` (ohne Endung) mit `text/plain` und Text-Body → kein Treffer.
  - [ ] `/logo` (ohne Endung) mit `text/html`, Body beginnt mit PNG-Signatur → Treffer (Regel 3).
  - [ ] Content-Type `html` ohne Endungs- oder Inhaltswiderspruch → kein Treffer; explizit leerer Header → kein Treffer; 404 mit PNG-Endung und `text/plain` → kein Treffer (K2-94).
  - [ ] `/submit` (ohne Endung) mit `application/octet-stream` und HTML-Body → kein Treffer; 301 ohne `Content-Type` → kein Treffer (K2-87).
- **Testfixture:** `onpage-security-http/` Routen `/image.jpg` (JS-Typ), `/script.js` (zwei Varianten), `/page`, `/logo` (PNG-Bytes, html-Typ), `/bad-syntax`.

---

## 5 Response Codes

Grundmenge („All“): alle internen und externen URLs mit Antwort- oder Blockierungsstatus. Jeder Filter existiert in drei Sichten: **All** (intern + extern), **Internal**, **External** (Filtername mit Präfix, z. B. „Internal Client Error (4XX)“). Zugehörige Bulk-Exporte und Reports mit Fehlerbezug: OUT-067 und OUT-084 (führend; ONPAGE-067 ist Verweis).

### ONPAGE-050 Response Codes: Spalten
- **Priorität:** P0
- **Beschreibung:** Spalten des Response-Codes-Tabs.
- **Eingaben/Konfiguration:** keine.
- **Ausgabe (Spalten):** `Address`, `Content`, `Status Code`, `Status`, `Indexability`, `Indexability Status`, `Inlinks`, `Response Time`, `Redirect URL`, `Redirect Type`.
- **Akzeptanzkriterien:**
  - [ ] Für eine 301-URL sind Redirect URL (absolut) und Redirect Type „HTTP Redirect“ gefüllt.
  - [ ] Für eine 200-URL sind Redirect URL und Redirect Type leer.
  - [ ] Inlinks = Anzahl interner Link-Instanzen auf die URL (wie ONPAGE-015).
- **Testfixture:** `onpage-rc/` (siehe ONPAGE-051).

### ONPAGE-051 Response Codes: Aufteilung Internal/External und „All“
- **Priorität:** P0
- **Beschreibung:** Jeder Statusfilter (052–063) ist in den Sichten All/Internal/External verfügbar. Die Sicht wird über eine Umschaltung (Dropdown) oder getrennte Filtereinträge gewählt; die Issue-Klassifizierung (UI-063) unterscheidet intern/extern.
- **Eingaben/Konfiguration:** Sicht.
- **Ausgabe:** Filtersicht.
- **Akzeptanzkriterien:**
  - [ ] Für jeden Filter gilt: Treffer(All) = Treffer(Internal) ∪ Treffer(External), disjunkt.
  - [ ] Ein externer 404 erscheint in „External Client Error (4XX)“, nicht in „Internal …“.
- **Testfixture:** `onpage-rc/`: `index.html` verlinkt Routen `/ok`, `/r301`→`/ok`, `/r302`→`/ok`, `/r307`→`/ok`, `/r308`→`/ok`, `/e400`, `/e403`, `/e404`, `/e410`, `/e429`, `/e500`, `/e503`, `/blocked/a`, `/timeout`, `/meta-refresh.html`, `/http-refresh`, `/js-redirect.html`, `/chain1`→`/chain2`→`/ok`, `/loop1`→`/loop2`→`/loop1`; plus externer Host (F-HOST2) mit `/e404`, sowie `http://nohost.invalid/` und `http://127.0.0.1:1/`.

### ONPAGE-052 Filter Blocked by Robots.txt
- **Priorität:** P0
- **Beschreibung:** URLs, die nicht angefragt wurden, weil die robots.txt ihres Hosts sie für den konfigurierten User-Agent verbietet (nur wenn „robots.txt beachten“ aktiv ist). Status Code 0, Status „Blocked by Robots.txt“.
- **Eingaben/Konfiguration:** robots.txt-Modus (Crawl-Spec): beachten / ignorieren / ignorieren aber melden.
- **Ausgabe:** Filter `Blocked by Robots.txt`; zusätzliche Spalte `Matched Robots.txt Line` (Zeilennummer und Regeltext) – Verweis Robots-Spec.
- **Akzeptanzkriterien:**
  - [ ] `/blocked/a` erscheint mit Status Code 0 und wird nie angefragt (Fixture-Server-Log).
  - [ ] Im Modus „ignorieren“ wird `/blocked/a` angefragt und erscheint nicht im Filter.
  - [ ] Im Modus „ignorieren aber melden“ wird angefragt, erscheint aber zusätzlich im Filter (**Annahme A-17**).
  - [ ] Im Text-Modus (kein Rendering) erscheinen blockierte Bilder/CSS/JS hier statt unter Blocked Resource.
- **Testfixture:** `onpage-rc/robots.txt` mit `Disallow: /blocked/`.

### ONPAGE-053 Filter Blocked Resource
- **Priorität:** P1
- **Beschreibung:** Nur im JS-Rendering-Modus: Seitenressourcen (Bilder, JavaScript, CSS), die beim Rendern nicht geladen werden konnten – wegen robots.txt-Sperre oder Ladefehler (Timeout, Verbindungsfehler, 4xx/5xx der Ressource).
- **Eingaben/Konfiguration:** Rendering = JavaScript.
- **Ausgabe:** Filter `Blocked Resource`; Lower-Tab „Rendered Page“/Ressourcenliste (Rendering-Spec) zeigt die betroffene Seite.
- **Akzeptanzkriterien:**
  - [ ] Seite bindet `/blocked/app.js` ein, robots.txt sperrt `/blocked/` → mit Rendering: `/blocked/app.js` in Blocked Resource, nicht in Blocked by Robots.txt.
  - [ ] Ohne Rendering ist der Filter leer und zeigt `needs_rendering` (ONPAGE-007).
  - [ ] Ressource mit Status 500 während des Renderns → in Blocked Resource (**Annahme A-18**: Ladefehler inkl. HTTP-Fehler).
- **Testfixture:** `onpage-rc/render-blocked.html` mit `<script src="/blocked/app.js">`, `<link rel=stylesheet href="/e500.css">` (Route 500).

### ONPAGE-054 Filter No Response
- **Priorität:** P0
- **Beschreibung:** URLs ohne HTTP-Antwort: fehlerhafte URL (nicht parsebar), DNS-Fehler, Verbindung abgelehnt, Verbindungsabbruch, Timeout, TLS-Fehler. Status Code 0, Status-Text gemäß ONPAGE-065. Mit Crawl-Option „Crawl Invalid Links“ (frühere Referenzversion) werden auch syntaktisch ungültige Links (z. B. `htps://`) hier als „Malformed“ gemeldet.
- **Eingaben/Konfiguration:** Timeout, Retry-Anzahl (Crawl-Spec), „Crawl Invalid Links“.
- **Ausgabe:** Filter `No Response`.
- **Akzeptanzkriterien:**
  - [ ] `/timeout` (delay > Timeout) → Status „Connection Timeout“.
  - [ ] `http://127.0.0.1:1/` → Status „Connection Refused“.
  - [ ] `http://nohost.invalid/` → Status „DNS Lookup Failed“.
  - [ ] Link `htps://x` mit „Crawl Invalid Links“ an → Eintrag mit Status „Malformed“; aus → kein Eintrag.
  - [ ] Robots-blockierte URLs erscheinen **nicht** in No Response (früheren Referenzfehler nicht nachbauen).
  - [ ] Hosts ohne erreichbare robots.txt folgen CRAWL-100: mit der Voreinstellung „Host sperren“ stehen sie als „Blocked by Robots.txt (robots.txt unreachable)“ in Blocked by Robots.txt, nicht in No Response; die Fehlerstatus oben gelten mit „alles erlauben“ (OS-Festlegung 14.10.2026, K-44).
- **Testfixture:** `onpage-rc/` wie ONPAGE-051, plus `invalid-links.html` mit `<a href="htps://x">`.

### ONPAGE-055 Filter Success (2XX)
- **Priorität:** P0
- **Beschreibung:** URLs mit Status 200–299.
- **Eingaben/Konfiguration:** keine.
- **Ausgabe:** Filter `Success (2XX)`.
- **Akzeptanzkriterien:**
  - [ ] `/ok` (200) und Route `/s204` (204) erscheinen; `/r301` nicht.
- **Testfixture:** `onpage-rc/` + Route `/s204`.

### ONPAGE-056 Filter Redirection (3XX)
- **Priorität:** P0
- **Beschreibung:** URLs mit Status 300–399, unabhängig davon, ob ein `Location`-Header vorhanden ist. Redirect Type „HTTP Redirect“ (bzw. „HSTS Policy“, ONPAGE-064).
- **Eingaben/Konfiguration:** keine.
- **Ausgabe:** Filter `Redirection (3XX)`.
- **Akzeptanzkriterien:**
  - [ ] `/r301`, `/r302`, `/r307`, `/r308` erscheinen, jeweils mit Redirect URL `…/ok`.
  - [ ] Route `/r304` (304 ohne Location) erscheint mit leerer Redirect URL.
  - [ ] Route `/r301-noloc` (301 ohne Location) erscheint mit leerer Redirect URL.
- **Testfixture:** `onpage-rc/` + Routen `/r304`, `/r301-noloc`.

### ONPAGE-057 Filter Redirection (JavaScript)
- **Priorität:** P1
- **Beschreibung:** Nur im Rendering-Modus: interne HTML-Seiten (Status 2xx), deren Script innerhalb der Render-Wartezeit eine Navigation auf eine andere URL auslöst (`location.href = …`, `location.replace(…)`, `location.assign(…)`). Redirect Type „JavaScript Redirect“, Redirect URL = Navigationsziel. Das Ziel wird als neue URL gecrawlt.
- **Eingaben/Konfiguration:** Rendering = JavaScript, Render-Wartezeit.
- **Ausgabe:** Filter `Redirection (JavaScript)`.
- **Akzeptanzkriterien:**
  - [ ] `js-redirect.html` mit `location.replace('/ok')` → im Filter, Redirect URL `…/ok`.
  - [ ] Ohne Rendering → nicht im Filter, Filterzustand `needs_rendering`.
  - [ ] Navigation nur auf einen Hash (`location.hash='x'`) → kein Redirect.
  - [ ] Navigation nach Wartezeit (setTimeout 10 s bei 5 s Wartezeit) → kein Redirect.
- **Testfixture:** `onpage-rc/js-redirect.html`, `js-hash.html`, `js-late.html`.

### ONPAGE-058 Filter Redirection (Meta Refresh)
- **Priorität:** P0
- **Beschreibung:** Interne/externe HTML-Seiten mit `<meta http-equiv="refresh" content="<n>; url=<ziel>">` (Attributname/-wert case-insensitiv, `url=` optional mit Anführungszeichen, beliebige Verzögerung n ≥ 0), deren aufgelöstes Ziel ≠ eigene URL ist. Redirect Type „Meta Refresh Redirect“. Das Ziel wird gecrawlt.
- **Eingaben/Konfiguration:** keine.
- **Ausgabe:** Filter `Redirection (Meta Refresh)`.
- **Akzeptanzkriterien:**
  - [ ] `content="0; url=/ok"` → im Filter, Redirect URL `…/ok`.
  - [ ] `content="5;URL='/ok'"` → im Filter.
  - [ ] `content="30"` (ohne URL, Selbst-Reload) → im Filter, Redirect URL = eigene Adresse (Paritätsbefund M1, Befund N-07, K-57; ersetzt „nicht im Filter“).
  - [ ] `content="0; url=meta-refresh.html"` (auf sich selbst) → im Filter, außerdem im Filter Redirect Loop (ONPAGE-061).
  - [ ] Meta Refresh außerhalb `<head>` wird ebenfalls erkannt (Browser tun das auch).
- **Testfixture:** `onpage-rc/meta-refresh*.html` (4 Varianten).

### ONPAGE-059 Filter Redirection (HTTP Refresh)
- **Priorität:** P1
- **Beschreibung:** URLs, deren Antwort einen `Refresh`-HTTP-Header mit Ziel-URL enthält (Syntax wie Meta Refresh), auch mit der eigenen URL als Ziel (K2-86, SQ-05; vorher „Ziel ≠ eigene URL“; Beleg Referenz `refresh-self.html`). Redirect Type „HTTP Refresh Redirect“.
- **Eingaben/Konfiguration:** keine.
- **Ausgabe:** Filter `Redirection (HTTP Refresh)`.
- **Akzeptanzkriterien:**
  - [ ] Route `/http-refresh` (200, Header `Refresh: 0; url=/ok`) → im Filter, Ziel wird gecrawlt.
  - [ ] Header `Refresh: 10` ohne URL → nicht im Filter.
  - [ ] Header `Refresh: 0; url=<eigene URL>` → im Filter, Indexability `Redirected` (TECH-001).
- **Testfixture:** `onpage-rc/` Routen `/http-refresh`, `/http-refresh-nourl`.

### ONPAGE-060 Filter Redirect Chain
- **Priorität:** P1
- **Beschreibung:** Interne URLs, die auf eine URL weiterleiten, die selbst wieder weiterleitet (≥ 2 Hops). Jede Weiterleitungsart (HTTP, Meta Refresh, HTTP Refresh, JS, HSTS) zählt als Hop. Der Filter listet die **Start-URL** der Kette. Maßgeblich ist die Hop-Zahl `Number of Redirects` ≥ 2 (jede URL der Kette einmal gezählt). Ketten, die in einer Schleife enden, zählen ebenfalls, sobald sie mindestens 2 Hops haben, und stehen zusätzlich in ONPAGE-061 (**Annahme A-19 revidiert**, Paritätsbefund M1: die Referenz zählt die Mitglieder der Schleife `/loop1` ↔ `/loop2` auch als Redirect Chain, `parity-response` Internal Redirect Chain 9; Entscheidung K-66 vom 15.10.2026, ersetzt „Ketten, die in einer Schleife enden, gehören nur zu ONPAGE-061“). Eine Selbst-Weiterleitung (Ziel = eigene URL, auch Selbst-Refresh) hat 1 Hop und steht nur in ONPAGE-061. Vollständige Ketten werden im Report „Redirects > Redirect Chains“ (Export-Spec) mit Spalten `Number of Redirects`, Zwischen-URLs, `Final Address`, `Final Status Code`, `Loop` ausgegeben.
- **Eingaben/Konfiguration:** Maximale Anzahl verfolgter Redirects (Crawl-Spec, Default 5).
- **Ausgabe:** Filter `Redirect Chain`; Spalte `Number of Redirects` (Erweiterung im Filter).
- **Akzeptanzkriterien:**
  - [ ] `/chain1` → `/chain2` → `/ok`: `/chain1` im Filter mit 2 Hops; `/chain2` selbst nicht (eigene Kette hat nur 1 Hop) (**Annahme A-19**).
  - [ ] `/r301` → `/ok` (1 Hop) → nicht im Filter.
  - [ ] Meta-Refresh-Seite → 301 → `/ok` → im Filter (gemischte Arten).
  - [ ] `/loop1` → `/loop2` → `/loop1`: beide URLs im Filter (je 2 Hops) und zusätzlich im Filter Redirect Loop (A-19 revidiert, K-66; Beleg `parity-response`).
  - [ ] Selbst-Weiterleitung `/self` → `/self` und Selbst-Refresh (je 1 Hop) → nicht im Filter, nur in Redirect Loop (Beleg `parity-response` `/meta-refresh-self.html`).
  - [ ] Kette, die in eine Schleife führt (`/tail` → `/loop1` → …), und `/qloop` → `/qloop?x=1` → `/qloop?x=1` → im Filter (3 bzw. 2 Hops; **Annahme A-19b**, in den Paritäts-Sites nicht belegt).
  - [ ] Kette länger als Redirect-Limit: Start-URL im Filter, Final Status Code „Max Redirects“ im Report.
  - [ ] Externe Ketten erscheinen nicht im Filter (nur „interne URLs“). Die Sicht `External Redirect Chain` (ONPAGE-051) ist deshalb immer leer; dasselbe gilt für `External Redirect Loop` (ONPAGE-061; OS-Festlegung 14.10.2026, K-44, Prüfung im Paritätslauf).
- **Testfixture:** `onpage-rc/` Routen `/chain1`, `/chain2`, `/mixed-chain.html` (meta refresh → `/r301`), `/long1`…`/long7`, `/loop1`, `/loop2`, `/tail`, `/qloop`, `/self`.

### ONPAGE-061 Filter Redirect Loop
- **Priorität:** P1
- **Beschreibung:** Interne URLs, deren Weiterleitungskette zu einer bereits in der Kette enthaltenen URL zurückführt. Der Filter ist leer, wenn keine Schleife existiert.
- **Eingaben/Konfiguration:** keine.
- **Ausgabe:** Filter `Redirect Loop`; im Redirect-Chains-Report `Loop = true`.
- **Akzeptanzkriterien:**
  - [ ] `/loop1` → `/loop2` → `/loop1` → beide URLs im Filter (**Annahme A-20**: alle Kettenmitglieder, die in die Schleife führen).
  - [ ] Selbst-Weiterleitung `/self` → `/self` → im Filter.
  - [ ] Selbst-Refresh (Meta Refresh ohne URL oder auf die eigene URL, `Refresh`-Header auf die eigene URL) → im Filter (K-57, K-69).
  - [ ] `Refresh`-Header ohne URL (`parity-response` `/http-refresh-nourl.html`) → keine Weiterleitung, nicht im Filter (Parität, K-69).
  - [ ] `/loop1` und `/loop2` stehen zusätzlich in Redirect Chain (ONPAGE-060, K-66); `/self` nicht.
  - [ ] Weiterleitung `/a?x=1` → `/a` → `/a?x=1` wird als Schleife erkannt (URL-Vergleich nach Normalisierung).
  - [ ] Crawler terminiert (keine Endlosschleife).
- **Testfixture:** `onpage-rc/` Routen `/loop1`, `/loop2`, `/self`, `/a` mit Query-Weiche.

### ONPAGE-062 Filter Client Error (4XX)
- **Priorität:** P0
- **Beschreibung:** URLs mit Status 400–499.
- **Eingaben/Konfiguration:** keine.
- **Ausgabe:** Filter `Client Error (4XX)`.
- **Akzeptanzkriterien:**
  - [ ] `/e400`, `/e403`, `/e404`, `/e410`, `/e429` erscheinen; Status-Texte „Bad Request“, „Forbidden“, „Not Found“, „Gone“, „Too Many Requests“.
  - [ ] Externer 404 nur in External-Sicht.
- **Testfixture:** `onpage-rc/`.

### ONPAGE-063 Filter Server Error (5XX)
- **Priorität:** P0
- **Beschreibung:** URLs mit Status 500–599.
- **Eingaben/Konfiguration:** Retry bei 5xx (Crawl-Spec).
- **Ausgabe:** Filter `Server Error (5XX)`.
- **Akzeptanzkriterien:**
  - [ ] `/e500`, `/e503` erscheinen.
  - [ ] Wird nach Retry eine 200 geliefert, zählt das letzte Ergebnis (URL nicht im Filter).
- **Testfixture:** `onpage-rc/` + Route `/flaky` (erst 503, dann 200 – benötigt zustandsbehaftete Route, F-Erweiterung `sequence`).

### ONPAGE-064 Redirect Type „HSTS Policy“
- **Priorität:** P1
- **Beschreibung:** Hat ein Host per HTTPS einen gültigen `Strict-Transport-Security`-Header gesendet, werden spätere `http://`-URLs desselben Hosts (bzw. Subdomains bei `includeSubDomains`) nicht angefragt, sondern intern als Weiterleitung mit Status 307 auf die `https://`-Variante behandelt. Redirect Type „HSTS Policy“, Status „HSTS Policy“ (**Annahme A-21**). Die Option „HSTS respektieren“ (Crawl-Spec, Default an) schaltet das ab.
- **Eingaben/Konfiguration:** HSTS respektieren.
- **Ausgabe:** Eintrag im Filter Redirection (3XX) mit Redirect Type „HSTS Policy“.
- **Akzeptanzkriterien:**
  - [ ] HTTPS-Start mit HSTS-Header, interner Link auf `http://…/x` → `http://…/x` erscheint mit Status 307, Typ „HSTS Policy“, ohne HTTP-Anfrage auf Port 80/HTTP (Server-Log).
  - [ ] Ohne HSTS-Header → echte HTTP-Anfrage.
  - [ ] Option aus → echte HTTP-Anfrage trotz Header.
- **Testfixture:** `onpage-security/` (F-TLS mit HTTP-Parallel-Listener auf demselben Host – F-Erweiterung) Route `/hsts-yes` + Link auf `http://`-Variante.

### ONPAGE-065 Status-Texte für Fehlerfälle ohne Antwort
- **Priorität:** P0
- **Beschreibung:** Feste Status-Texte bei Status Code 0: `Blocked by Robots.txt`, `Connection Timeout`, `Connection Refused`, `Connection Error` (Abbruch/Reset), `DNS Lookup Failed`, `Malformed` (ungültige URL), `TLS Error` (Zertifikat/Handshake; Erweiterung), `Blocked by Exclude` nicht hier (ausgeschlossene URLs werden nicht gelistet – Crawl-Spec). Die Trennung von `Connection Refused` (Verbindung aktiv abgelehnt, z. B. geschlossener Port) und `Connection Timeout` (keine Antwort innerhalb der Frist) ist eine **OS-Festlegung** (Paritätsbefund M1, Entscheidung K-67 vom 15.10.2026, DEV-005): Die Referenz meldet für den geschlossenen Port `localhost:1` `Connection Timeout`. OpenSpider meldet die tatsächliche Ursache, weil beide Fälle verschiedene Abhilfen verlangen (Dienst/Port/Firewall prüfen bzw. Last oder Timeout-Einstellung) und ein abgelehnter Verbindungsaufbau sofort feststeht.
- **Eingaben/Konfiguration:** keine.
- **Ausgabe:** Wert der Spalte `Status`.
- **Akzeptanzkriterien:**
  - [ ] Jeder Fall aus ONPAGE-054 liefert exakt den genannten Text.
  - [ ] Server schließt Verbindung nach Header-Versand → „Connection Error“.
  - [ ] Ungültiges Zertifikat ohne „ungültige Zertifikate akzeptieren“ → „TLS Error“.
- **Testfixture:** `onpage-rc/` + Route `/drop` (F-Erweiterung `abort_after_headers`), F-TLS ohne Akzeptanz.

### ONPAGE-066 Statuscodes außerhalb 2xx–5xx
- **Priorität:** P2
- **Beschreibung:** Antworten mit 1xx-Endstatus (praktisch nur bei defekten Servern) oder Codes ≥ 600 werden mit ihrem Code angezeigt und fallen in keinen der Statusfilter außer All.
- **Eingaben/Konfiguration:** keine.
- **Ausgabe:** Zeile in All.
- **Akzeptanzkriterien:**
  - [ ] Route mit Status 999 (LinkedIn-typisch) erscheint in All mit Code 999, in keinem anderen Statusfilter.
- **Testfixture:** `onpage-rc/` Route `/s999`.

### ONPAGE-067 Bulk-Export „All Error Inlinks“ und Report „Redirects to Error“
- **Priorität:** P1
- **Verweis:** siehe OUT-067 (führend). Den Report „Redirects to Error“ regelt OUT-084.

---

## 6 URL

Grundmenge („All“): alle internen URLs. Prüfungen arbeiten auf der Anzeigeform der Adresse (Prozentkodierung von Non-ASCII aufgelöst), Teile gemäß WHATWG-URL-Parser (`path`, `query`, `fragment`).

### ONPAGE-070 URL: Spalten
- **Priorität:** P0
- **Beschreibung:** Spalten des URL-Tabs.
- **Eingaben/Konfiguration:** keine.
- **Ausgabe (Spalten):** `Address`, `Content`, `Status Code`, `Status`, `Indexability`, `Indexability Status`, `Hash`, `Length` (Zeichenlänge der Address, Codepoints), `Canonical Link Element 1`, `URL Encoded Address`.
- **Akzeptanzkriterien:**
  - [ ] `Length` von `http://127.0.0.1:8080/abc` = 25.
  - [ ] `Length` einer URL mit `ä` zählt `ä` als 1 Zeichen (Address-Form, nicht kodierte Form).
- **Testfixture:** `onpage-url/` (siehe ONPAGE-071 ff.).

### ONPAGE-071 URL-Filter Non ASCII Characters
- **Priorität:** P0
- **Beschreibung:** Interne URLs, deren Address (dekodierte Anzeigeform von Pfad, Query, Fragment; Host in Unicode-Form bei IDN) mindestens ein Zeichen > U+007F enthält. Prozentkodiert gelieferte UTF-8-Sequenzen zählen ebenfalls, da sie dekodiert angezeigt werden (**Annahme A-22**).
- **Eingaben/Konfiguration:** keine.
- **Ausgabe:** Filter `Non ASCII Characters`.
- **Akzeptanzkriterien:**
  - [ ] Link `href="/über-uns"` → im Filter.
  - [ ] Link `href="/%C3%BCber-uns"` → im Filter (A-22).
  - [ ] `/ueber-uns` → nicht.
  - [ ] `/a%20b` → nicht (Leerzeichen ist ASCII).
- **Testfixture:** `onpage-url/index.html` mit allen URL-Varianten als Links; Routen liefern jeweils 200 HTML.

### ONPAGE-072 URL-Filter Underscores
- **Priorität:** P0
- **Beschreibung:** Interne URLs mit mindestens einem `_` in Pfad **oder Query** (**Annahme A-23, revidiert nach Paritätslauf M1 vom 14.10.2026, K-58**: die Referenz zählt `?sort_by=x` mit; die frühere Annahme „nur Pfad“ ist widerlegt). Das Fragment zählt nicht.
- **Eingaben/Konfiguration:** keine.
- **Ausgabe:** Filter `Underscores`.
- **Akzeptanzkriterien:**
  - [ ] `/mein_artikel` → im Filter; `/mein-artikel` → nicht.
  - [ ] `/seite?sort_by=x` → im Filter (A-23, revidiert); `/seite#a_b` → nicht (Fragment).
- **Testfixture:** `onpage-url/`.

### ONPAGE-073 URL-Filter Uppercase
- **Priorität:** P0
- **Beschreibung:** Interne URLs mit mindestens einem Großbuchstaben (Unicode-Kategorie Lu) in Pfad, Query oder Fragment. Schema und Host sind nach Normalisierung immer klein und werden nicht geprüft. Hex-Ziffern von Prozentkodierungen (`%C3`) zählen nicht.
- **Eingaben/Konfiguration:** keine.
- **Ausgabe:** Filter `Uppercase`.
- **Akzeptanzkriterien:**
  - [ ] `/Produkte` → im Filter; `/produkte?Farbe=rot` → im Filter.
  - [ ] Link `http://127.0.0.1:<port>/x` auf `HTTP://127.0.0.1…/x` geschrieben → nicht (Host/Schema normalisiert).
  - [ ] `/caf%C3%A9` → nicht (Hex zählt nicht; Anzeige `/café`).
  - [ ] `/ÜBER` → im Filter.
- **Testfixture:** `onpage-url/`.

### ONPAGE-074 URL-Filter Multiple Slashes
- **Priorität:** P0
- **Beschreibung:** Interne URLs, deren Pfad zwei oder mehr aufeinanderfolgende `/` enthält. Das `//` nach dem Schema und Schrägstriche in Query/Fragment zählen nicht.
- **Eingaben/Konfiguration:** keine.
- **Ausgabe:** Filter `Multiple Slashes`.
- **Akzeptanzkriterien:**
  - [ ] `/a//b` → im Filter; `//` am Pfadanfang einer absoluten URL (`http://h:p//a`) → im Filter.
  - [ ] `/a/b?next=http://x` → nicht.
  - [ ] URL-Normalisierung darf `//` im Pfad **nicht** zusammenfassen (sonst wäre der Filter immer leer).
- **Testfixture:** `onpage-url/`.

### ONPAGE-075 URL-Filter Repetitive Path
- **Priorität:** P1
- **Beschreibung:** Interne URLs, deren Pfad ein nicht-leeres Segment mehrfach enthält (beliebige Position, exakter Vergleich, case-sensitiv) (**Annahme A-24**: ein beliebiges Segment, nicht nur Verzeichnisse; Dateiname zählt mit).
- **Eingaben/Konfiguration:** keine.
- **Ausgabe:** Filter `Repetitive Path`.
- **Akzeptanzkriterien:**
  - [ ] `/services/seo/services/` → im Filter.
  - [ ] `/a/b/a/b/` (typisch für fehlerhafte relative Links) → im Filter.
  - [ ] `/blog/2024/blog-post` → nicht (Segmente verschieden).
  - [ ] `/Shop/shop/` → nicht (case-sensitiv).
- **Testfixture:** `onpage-url/` mit einer Seite `/a/b/` die einen relativen Link `a/b/` enthält (erzeugt `/a/b/a/b/`, Route liefert 200 → realistische Spider-Falle; Crawl-Tiefe im Test begrenzen).

### ONPAGE-076 URL-Filter Contains A Space
- **Priorität:** P0
- **Beschreibung:** Interne URLs, deren Address ein Leerzeichen enthält. Es zählen ein literales Leerzeichen im Link-Attribut und die Kodierung `%20` (Anzeigeform dekodiert beide zu einem Leerzeichen); ein `+` in der Query zählt nicht (**Annahme A-25**).
- **Eingaben/Konfiguration:** keine.
- **Ausgabe:** Filter `Contains A Space`.
- **Akzeptanzkriterien:**
  - [ ] `href="/mein artikel"` → im Filter (angefragt als `%20`).
  - [ ] `href="/mein%20artikel"` → im Filter.
  - [ ] `href="/suche?q=a+b"` → nicht.
- **Testfixture:** `onpage-url/`.

### ONPAGE-077 URL-Filter Internal Search
- **Priorität:** P1
- **Beschreibung:** Interne URLs, die heuristisch zur seiteninternen Suche gehören. Treffer, wenn mindestens eine Bedingung gilt (case-insensitiv) (**Annahme A-26**, Heuristik ist OpenSpider-eigen):
  1. Ein Pfadsegment ist exakt einer der Werte `search`, `suche`, `recherche`, `buscar`, `busca`, `cerca`, `zoeken`, `szukaj`, `sok`, `haku`, `catalogsearch`.
  2. Ein Query-Parameter-Name ist einer von `q`, `query`, `search`, `search_query`, `searchterm`, `keyword`, `keywords`, `k`, `term` – und der Wert ist nicht leer.
  3. Die Seite enthält strukturierte Daten `SearchAction` mit einem `urlTemplate`, dessen Muster auf die URL passt (Erweiterung).
- **Eingaben/Konfiguration:** `url.internal_search_segments`, `url.internal_search_params` (Listen, Defaults wie oben).
- **Ausgabe:** Filter `Internal Search`.
- **Akzeptanzkriterien:**
  - [ ] `/search?q=schuhe` → im Filter; `/suche/schuhe` → im Filter; `/?s=test` → nicht im Filter (Parameter `s` gestrichen, Beleg Referenz `parity-m2-onpage` `/?s=stone`; K2-87, SQ-13b).
  - [ ] `/research/` → nicht (Segment-Gleichheit, nicht Teilstring).
  - [ ] `/?q=` (leer) → nicht.
  - [ ] Nach Entfernen von `q` aus der Liste → `/search?q=…` bleibt wegen Segment `search` im Filter.
- **Testfixture:** `onpage-url/` mit Suchformular `<form action="/search"><input name="q"></form>` und Links auf die Varianten (Formulare werden nicht abgeschickt; URLs kommen aus Links).

### ONPAGE-078 URL-Filter Parameters
- **Priorität:** P0
- **Beschreibung:** Interne URLs, deren Address ein `?` oder `&` enthält (außerhalb des Fragments).
- **Eingaben/Konfiguration:** keine.
- **Ausgabe:** Filter `Parameters`.
- **Akzeptanzkriterien:**
  - [ ] `/p?id=1` → im Filter; `/p?` (leere Query) → im Filter (**Annahme A-27**).
  - [ ] `/a&b` (Ampersand im Pfad) → im Filter.
  - [ ] `/p#x?y` → nicht (nur im Fragment).
- **Testfixture:** `onpage-url/`.

### ONPAGE-079 URL-Filter Broken Bookmark
- **Priorität:** P1
- **Beschreibung:** Nur wenn die Crawl-Option „Crawl Fragment Identifiers“ aktiv ist (sonst werden Fragmente entfernt): interne HTML-URLs mit Fragment `#x`, bei denen die Zielseite (Status 2xx, Roh-HTML bzw. gerendertes DOM im Rendering-Modus) kein Element mit `id="x"` (exakter, case-sensitiver Vergleich, Fragment prozent-dekodiert) enthält. Ausgenommen sind: leeres Fragment (`#`), `#top` (laut HTML-Standard gültig, wenn kein Element `top` existiert), Hashbang `#!…`, Text-Fragmente `#:~:text=…`. `<a name="x">` gilt als gültiges Ziel (**Annahme A-28**; HTML-Standard erlaubt es).
- **Eingaben/Konfiguration:** „Crawl Fragment Identifiers“ (Crawl-Spec).
- **Ausgabe:** Filter `Broken Bookmark`; Lower-Tab Inlinks zeigt die verlinkenden Seiten.
- **Akzeptanzkriterien:**
  - [ ] `/page#kontakt` mit `<h2 id="kontakt">` → nicht im Filter.
  - [ ] `/page#preise` ohne passendes id → im Filter.
  - [ ] `/page#Kontakt` (Großschreibung) → im Filter.
  - [ ] `/page#top`, `/page#`, `/page#!/route`, `/page#:~:text=abc` → nicht im Filter.
  - [ ] `/page#anker-alt` mit `<a name="anker-alt">` → nicht im Filter (A-28).
  - [ ] Option aus → Filter leer und Zustand `needs_config`.
  - [ ] Die Zielseite wird nur einmal angefragt, auch bei 10 verschiedenen Fragmenten.
- **Testfixture:** `onpage-url/bookmarks.html` (Links mit allen Fragment-Varianten auf `target.html`), `target.html` mit `id="kontakt"` und `<a name="anker-alt">`.

### ONPAGE-080 URL-Filter GA Tracking Parameters
- **Priorität:** P1
- **Beschreibung:** Interne URLs mit mindestens einem Query-Parameter, dessen Name (case-insensitiv) mit `utm_` beginnt oder exakt `_ga` oder `_gl` ist.
- **Eingaben/Konfiguration:** keine.
- **Ausgabe:** Filter `GA Tracking Parameters`.
- **Akzeptanzkriterien:**
  - [ ] `/?utm_source=nl` → im Filter; `/?UTM_Medium=x` → im Filter.
  - [ ] `/?_ga=2.1234` und `/?_gl=1*abc` → im Filter.
  - [ ] `/?gclid=x` → nicht (kein GA-Parameter im Sinne dieses Filters).
  - [ ] `/?utmx=1` → nicht; `/#utm_source=x` (im Fragment) → nicht.
- **Testfixture:** `onpage-url/`.

### ONPAGE-081 URL-Filter Over 115 Characters
- **Priorität:** P0
- **Beschreibung:** Interne URLs mit `Length` (ONPAGE-070) > `url.max_chars` (Default 115).
- **Eingaben/Konfiguration:** `url.max_chars`.
- **Ausgabe:** Filter `Over 115 Characters` (Name dynamisch).
- **Akzeptanzkriterien:**
  - [ ] URL mit exakt 115 Zeichen → nicht im Filter; 116 Zeichen → im Filter.
  - [ ] Nach Setzen von `url.max_chars = 50` ändern sich Treffer und Filtername ohne Recrawl.
- **Testfixture:** `onpage-url/` mit zwei Links, deren Pfade exakt 40 und 41 Zeichen lang sind; da die Basis-URL (Port) variiert, setzt der Test `url.max_chars = len(base_url) + 40` und erwartet nur den 41er-Pfad im Filter. Zusätzlich ein Unit-Test der Filterfunktion mit festen 115/116-Zeichen-URLs bei Default-Schwellwert.

---

## 7 Page Titles

Grundmenge: ONPAGE-009. Extraktion aus dem Parse-Baum (0.1). `<title>`-Elemente im SVG-Namensraum (`<svg><title>…</title></svg>`) zählen **nicht**.

### ONPAGE-090 Page Titles: Extraktion und Spalten
- **Priorität:** P0
- **Beschreibung:** Alle `<title>`-Elemente im HTML-Namensraum werden in Dokumentreihenfolge gesammelt (im `<head>` und außerhalb). Wert = N(Textinhalt). Für jedes Vorkommen wird zusätzlich gespeichert, ob es im `<head>` liegt.
- **Eingaben/Konfiguration:** Speicher-Schalter „Page Titles“ (aus → Tab leer, Spalten ausgeblendet).
- **Ausgabe (Spalten):** `Address`, `Occurrences` (max. 2), `Title 1`, `Title 1 Length`, `Title 1 Pixel Width`, `Title 2`, `Title 2 Length`, `Title 2 Pixel Width`, `Indexability`, `Indexability Status`. Alle Spalten, auch die Pixelbreiten, sind standardmäßig sichtbar und stehen im Standardexport `Page Titles:<Filter>` (Paritätsbefund M1, Befund N-02, K-60).
- **Akzeptanzkriterien:**
  - [ ] `<title>Start</title>` → Title 1 „Start“, Length 5, Occurrences 1.
  - [ ] `<title>A <b>B</b></title>` → Wert literal `A <b>B</b>` (Inhalt von `<title>` ist RCDATA, Tags werden nicht interpretiert).
  - [ ] SVG-`<title>` im Body wird ignoriert (Occurrences zählt sie nicht).
  - [ ] Im Rendering-Modus wird der Titel aus dem gerenderten DOM genommen (per JS gesetzter `document.title` wird erkannt).
- **Testfixture:** `onpage-titles/` mit `basic.html`, `rcdata.html`, `svg-title.html` (Titel + Inline-SVG mit `<title>`), `js-title.html` (setzt Titel per Script).

### ONPAGE-091 Titel-Filter Missing
- **Priorität:** P0
- **Beschreibung:** Seiten, bei denen kein `<title>` existiert oder alle Vorkommen nach N(x) leer sind.
- **Eingaben/Konfiguration:** keine.
- **Ausgabe:** Filter `Missing`.
- **Akzeptanzkriterien:**
  - [ ] Seite ohne `<title>` → im Filter.
  - [ ] `<title></title>` und `<title>   </title>` und `<title>&nbsp;</title>` → im Filter.
  - [ ] `<title></title>` gefolgt von `<title>X</title>` → nicht im Filter (aber in Multiple).
  - [ ] Seite mit Titel nur außerhalb `<head>` → nicht in Missing (stattdessen Outside <head>).
- **Testfixture:** `onpage-titles/missing-*.html` (4 Varianten), `empty-then-filled.html`.

### ONPAGE-092 Titel-Filter Duplicate
- **Priorität:** P0
- **Beschreibung:** Duplikat-Logik ONPAGE-005 auf `Title 1`.
- **Eingaben/Konfiguration:** `duplicates.only_indexable`, `duplicates.ignore_paginated` (ONPAGE-005).
- **Ausgabe:** Filter `Duplicate`; Sortierung standardmäßig nach Title 1, damit Gruppen zusammenstehen.
- **Akzeptanzkriterien:** siehe ONPAGE-005, angewendet auf Titel; zusätzlich:
  - [ ] Standard-Sortierung im Filter gruppiert gleiche Titel direkt untereinander.
- **Testfixture:** `onpage-duplicates/`.

### ONPAGE-093 Titel-Filter Over 60 Characters
- **Priorität:** P0
- **Beschreibung:** Seiten mit `Title 1 Length` > `title.max_chars` (Default 60).
- **Eingaben/Konfiguration:** `title.max_chars`.
- **Ausgabe:** Filter `Over 60 Characters`.
- **Akzeptanzkriterien:**
  - [ ] Länge 60 → nicht; 61 → im Filter.
  - [ ] Länge wird nach Normalisierung gemessen (Titel mit 58 Zeichen + 5 Leerzeichen am Rand → 58 → nicht).
  - [ ] Nur Title 1 zählt (Title 2 mit 80 Zeichen bei Title 1 mit 20 → nicht).
- **Testfixture:** `onpage-titles/len-60.html`, `len-61.html`, `len-58-padded.html`, `multi-short-long.html`.

### ONPAGE-094 Titel-Filter Below 30 Characters
- **Priorität:** P0
- **Beschreibung:** Seiten mit 0 < `Title 1 Length` < `title.min_chars` (Default 30). Fehlende Titel gehören nicht dazu.
- **Eingaben/Konfiguration:** `title.min_chars`.
- **Ausgabe:** Filter `Below 30 Characters`.
- **Akzeptanzkriterien:**
  - [ ] Länge 29 → im Filter; 30 → nicht.
  - [ ] Fehlender Titel (Länge 0) → nicht im Filter.
- **Testfixture:** `onpage-titles/len-29.html`, `len-30.html`, `missing-none.html`.

### ONPAGE-095 Titel-Filter Over 561 Pixels
- **Priorität:** P1
- **Beschreibung:** Seiten mit `Title 1 Pixel Width` > `title.max_px` (Default 561). Die Breite ist eine Annäherung an die Abschneidegrenze in Suchergebnissen, die sich ändern kann.
- **Eingaben/Konfiguration:** `title.max_px`.
- **Ausgabe:** Filter `Over 561 Pixels`.
- **Akzeptanzkriterien:**
  - [ ] Titel aus 60× „W“ → im Filter (breit, obwohl nur 60 Zeichen → nicht in Over 60 Characters).
  - [ ] Titel aus 70× „i“ → nicht im Filter (schmal, aber in Over 60 Characters).
  - [ ] Grenzfall: Titel mit berechneter Breite genau 561 → nicht im Filter.
- **Testfixture:** `onpage-titles/pixel-W60.html`, `pixel-i70.html`; Grenzfall-Titel wird im Unit-Test über die Metriktabelle konstruiert.

### ONPAGE-096 Titel-Filter Below 200 Pixels
- **Priorität:** P1
- **Beschreibung:** Seiten mit 0 < `Title 1 Pixel Width` < `title.min_px` (Default 200). Fehlende Titel gehören nicht dazu.
- **Eingaben/Konfiguration:** `title.min_px`.
- **Ausgabe:** Filter `Below 200 Pixels`.
- **Akzeptanzkriterien:**
  - [ ] Titel „Start“ → im Filter.
  - [ ] Fehlender Titel → nicht.
  - [ ] Änderung von `title.min_px` wirkt nur auf den Filter, nicht auf die gespeicherte Breite.
- **Testfixture:** `onpage-titles/basic.html`, `missing-none.html`.

### ONPAGE-097 Titel-Filter Same as H1
- **Priorität:** P1
- **Beschreibung:** Seiten, bei denen `Title 1` (nicht leer) exakt gleich `H1-1` ist (beide nach N(x), case-sensitiv, **Annahme A-29**). Hinweischarakter: kein Fehler, nur Prüfanlass.
- **Eingaben/Konfiguration:** keine.
- **Ausgabe:** Filter `Same as H1`; zusätzliche Spalte `H1-1` im Filter.
- **Akzeptanzkriterien:**
  - [ ] `<title>Kontakt</title>` + `<h1>Kontakt</h1>` → im Filter.
  - [ ] `<h1> Kontakt </h1>` → im Filter (Normalisierung).
  - [ ] `<h1>kontakt</h1>` → nicht (A-29).
  - [ ] Gleichheit nur mit H1-2 → nicht.
  - [ ] Titel und H1 beide leer → nicht.
- **Testfixture:** `onpage-titles/same-h1-*.html` (5 Varianten).

### ONPAGE-098 Titel-Filter Multiple
- **Priorität:** P0
- **Beschreibung:** Seiten mit mehr als einem `<title>`-Element (HTML-Namensraum), unabhängig von Position und Inhalt.
- **Eingaben/Konfiguration:** keine.
- **Ausgabe:** Filter `Multiple`.
- **Akzeptanzkriterien:**
  - [ ] Zwei Titel im Head → im Filter.
  - [ ] Ein Titel im Head, einer im Body → im Filter (und in Outside <head>).
  - [ ] Titel + SVG-Titel → nicht.
  - [ ] Zwei leere Titel → im Filter (und in Missing).
- **Testfixture:** `onpage-titles/multi-*.html`.

### ONPAGE-099 Titel-Filter Outside <head>
- **Priorität:** P1
- **Beschreibung:** Seiten mit mindestens einem `<title>` (HTML-Namensraum), das im Parse-Baum nicht im `<head>` liegt.
- **Eingaben/Konfiguration:** keine.
- **Ausgabe:** Filter `Outside <head>`.
- **Akzeptanzkriterien:**
  - [ ] `<body><title>X</title>` → im Filter.
  - [ ] `<head><div></div><title>X</title></head>` → im Filter (Div beendet den Head implizit, Titel landet im Body).
  - [ ] `<head><img src=a.png><title>X</title>` → im Filter.
  - [ ] Dokument ohne explizites `<head>`-Tag, Titel vor allen Body-Inhalten (`<!doctype html><title>X</title><p>…`) → nicht im Filter (Parser legt implizites head an).
  - [ ] `<head><noscript><link …></noscript><title>X</title>` → nicht im Filter (erlaubter Head-Inhalt).
- **Testfixture:** `onpage-titles/outside-*.html` (5 Varianten).

---

## 8 Meta Description

Grundmenge: ONPAGE-009. Vorkommen = `<meta>`-Elemente, deren `name`-Attribut (getrimmt, case-insensitiv) `description` ist. Wert = N(`content`); fehlt `content`, ist der Wert leer.

### ONPAGE-100 Meta Description: Extraktion und Spalten
- **Priorität:** P0
- **Beschreibung:** Sammelt alle Meta-Descriptions in Dokumentreihenfolge mit Head-Zugehörigkeit. `<meta property="og:description">` und `<meta name="twitter:description">` zählen nicht.
- **Eingaben/Konfiguration:** Speicher-Schalter „Meta Description“.
- **Ausgabe (Spalten):** `Address`, `Occurrences` (max. 2), `Meta Description 1`, `Meta Description 1 Length`, `Meta Description 1 Pixel Width`, `Meta Description 2`, `Meta Description 2 Length`, `Meta Description 2 Pixel Width`, `Indexability`, `Indexability Status`. Alle Spalten, auch die Pixelbreiten, sind standardmäßig sichtbar und stehen im Standardexport (K-60).
- **Akzeptanzkriterien:**
  - [ ] `<meta name="Description" content="Text">` → erkannt (Name case-insensitiv).
  - [ ] `<meta property="og:description" content="X">` allein → Occurrences 0.
  - [ ] `<meta name="description">` ohne content → Occurrences 1, Wert leer.
  - [ ] Entities in content werden dekodiert (`&quot;` → `"`).
- **Testfixture:** `onpage-meta/` mit `basic.html`, `case.html`, `og-only.html`, `no-content.html`, `entities.html`.

### ONPAGE-101 MD-Filter Missing
- **Priorität:** P0
- **Beschreibung:** Seiten ohne Meta-Description oder bei denen alle Vorkommen nach N(x) leer sind.
- **Eingaben/Konfiguration:** keine.
- **Ausgabe:** Filter `Missing`.
- **Akzeptanzkriterien:**
  - [ ] Kein Element → im Filter; `content=""` → im Filter; `content="  "` → im Filter.
  - [ ] Leeres erstes + gefülltes zweites Vorkommen → nicht im Filter.
- **Testfixture:** `onpage-meta/missing-*.html`.

### ONPAGE-102 MD-Filter Duplicate
- **Priorität:** P0
- **Beschreibung:** Duplikat-Logik ONPAGE-005 auf `Meta Description 1`.
- **Eingaben/Konfiguration:** `duplicates.only_indexable`, `duplicates.ignore_paginated` (ONPAGE-005).
- **Ausgabe:** Filter `Duplicate`.
- **Akzeptanzkriterien:** wie ONPAGE-005, angewendet auf Meta Description.
- **Testfixture:** `onpage-duplicates/`.

### ONPAGE-103 MD-Filter Over 155 Characters
- **Priorität:** P0
- **Beschreibung:** Seiten mit `Meta Description 1 Length` > `meta_description.max_chars` (Default 155).
- **Eingaben/Konfiguration:** `meta_description.max_chars`.
- **Ausgabe:** Filter `Over 155 Characters`.
- **Akzeptanzkriterien:**
  - [ ] 155 → nicht; 156 → im Filter.
- **Testfixture:** `onpage-meta/len-155.html`, `len-156.html`.

### ONPAGE-104 MD-Filter Below 70 Characters
- **Priorität:** P0
- **Beschreibung:** Seiten mit 0 < `Meta Description 1 Length` < `meta_description.min_chars` (Default 70).
- **Eingaben/Konfiguration:** `meta_description.min_chars`.
- **Ausgabe:** Filter `Below 70 Characters`.
- **Akzeptanzkriterien:**
  - [ ] 69 → im Filter; 70 → nicht; fehlend → nicht.
- **Testfixture:** `onpage-meta/len-69.html`, `len-70.html`.

### ONPAGE-105 MD-Filter Over 985 Pixels
- **Priorität:** P1
- **Beschreibung:** Seiten mit `Meta Description 1 Pixel Width` > `meta_description.max_px` (Default 985), Berechnung UI-038.
- **Eingaben/Konfiguration:** `meta_description.max_px`.
- **Ausgabe:** Filter `Over 985 Pixels`.
- **Akzeptanzkriterien:**
  - [ ] 150× „W“ → im Filter (obwohl ≤ 155 Zeichen).
  - [ ] 160× „i“ → nicht im Filter (aber in Over 155 Characters).
- **Testfixture:** `onpage-meta/pixel-W150.html`, `pixel-i160.html`.

### ONPAGE-106 MD-Filter Below 400 Pixels
- **Priorität:** P1
- **Beschreibung:** Seiten mit 0 < `Meta Description 1 Pixel Width` < `meta_description.min_px` (Default 400).
- **Eingaben/Konfiguration:** `meta_description.min_px`.
- **Ausgabe:** Filter `Below 400 Pixels`.
- **Akzeptanzkriterien:**
  - [ ] Kurze Beschreibung („Kurz.“) → im Filter; fehlend → nicht.
- **Testfixture:** `onpage-meta/short.html`.

### ONPAGE-107 MD-Filter Multiple
- **Priorität:** P0
- **Beschreibung:** Seiten mit mehr als einem Meta-Description-Element (unabhängig von Inhalt und Position).
- **Eingaben/Konfiguration:** keine.
- **Ausgabe:** Filter `Multiple`.
- **Akzeptanzkriterien:**
  - [ ] Zwei Elemente im Head → im Filter, Occurrences 2.
  - [ ] Ein `name="description"` + ein `property="og:description"` → nicht.
- **Testfixture:** `onpage-meta/multi.html`, `with-og.html`.

### ONPAGE-108 MD-Filter Outside <head>
- **Priorität:** P1
- **Beschreibung:** Seiten mit mindestens einer Meta-Description, die im Parse-Baum nicht im `<head>` liegt (Logik wie ONPAGE-099).
- **Eingaben/Konfiguration:** keine.
- **Ausgabe:** Filter `Outside <head>`.
- **Akzeptanzkriterien:**
  - [ ] `<head><span></span><meta name=description …>` → im Filter.
  - [ ] Meta-Description im `<body>` → im Filter.
  - [ ] Meta-Description regulär im Head → nicht.
- **Testfixture:** `onpage-meta/outside-*.html`.

---

## 9 Meta Keywords

Grundmenge: ONPAGE-009. Vorkommen = `<meta name="keywords">` (Name getrimmt, case-insensitiv). Hinweis in der UI: Die großen westlichen Suchmaschinen werten das Element nicht aus; relevant ggf. für Baidu/Yandex.

### ONPAGE-110 Meta Keywords: Extraktion und Spalten
- **Priorität:** P1
- **Beschreibung:** Extraktion analog ONPAGE-100.
- **Eingaben/Konfiguration:** Speicher-Schalter „Meta Keywords“.
- **Ausgabe (Spalten):** `Address`, `Occurrences` (max. 2), `Meta Keywords 1`, `Meta Keywords 1 Length`, `Meta Keywords 2`, `Meta Keywords 2 Length`, `Indexability`, `Indexability Status`.
- **Akzeptanzkriterien:**
  - [ ] `<meta name="KEYWORDS" content="a, b">` → Meta Keywords 1 „a, b“, Length 4.
  - [ ] Keine Pixelbreiten-Spalte.
- **Testfixture:** `onpage-meta/keywords.html`.

### ONPAGE-111 MK-Filter Missing
- **Priorität:** P1
- **Beschreibung:** Seiten ohne Meta-Keywords oder mit nur leeren Vorkommen.
- **Eingaben/Konfiguration:** keine.
- **Ausgabe:** Filter `Missing`.
- **Akzeptanzkriterien:**
  - [ ] Ohne Element → im Filter; `content=""` → im Filter; mit Wert → nicht.
- **Testfixture:** `onpage-meta/basic.html` (ohne Keywords), `keywords-empty.html`.

### ONPAGE-112 MK-Filter Duplicate
- **Priorität:** P1
- **Beschreibung:** Duplikat-Logik ONPAGE-005 auf `Meta Keywords 1`.
- **Eingaben/Konfiguration:** `duplicates.only_indexable`, `duplicates.ignore_paginated` (ONPAGE-005).
- **Ausgabe:** Filter `Duplicate`.
- **Akzeptanzkriterien:** wie ONPAGE-005; `"a, b"` vs. `"a,b"` → kein Duplikat (kein Token-Vergleich).
- **Testfixture:** `onpage-duplicates/` + `kw-spacing.html`.

### ONPAGE-113 MK-Filter Multiple
- **Priorität:** P1
- **Beschreibung:** Seiten mit mehr als einem Meta-Keywords-Element.
- **Eingaben/Konfiguration:** keine.
- **Ausgabe:** Filter `Multiple`.
- **Akzeptanzkriterien:**
  - [ ] Zwei Elemente → im Filter; eines → nicht.
- **Testfixture:** `onpage-meta/keywords-multi.html`.

---

## 10 H1

Grundmenge: ONPAGE-009. Vorkommen = alle `<h1>`-Elemente in Dokumentreihenfolge (Parse-Baum bzw. gerendertes DOM im Rendering-Modus). Wert = N(Textinhalt inkl. Nachfahren), wobei `<img alt>` innerhalb der Überschrift als Text (Alt-Wert) eingesetzt wird (**Annahme A-30**); `<script>`/`<style>`-Inhalt wird ignoriert; CSS-generierter Inhalt (`::before`) zählt nicht.

### ONPAGE-120 H1: Extraktion und Spalten
- **Priorität:** P0
- **Beschreibung:** Speichert Gesamtzahl der H1 (ungedeckelt) und die ersten zwei Werte. Zusätzlich je Seite die geordnete Heading-Sequenz (Level 1–6) für Non-sequential-Prüfungen und ein Flag, ob ein H1 Alt-Text enthält.
- **Eingaben/Konfiguration:** Speicher-Schalter „H1“.
- **Ausgabe (Spalten):** `Address`, `Occurrences` (ungedeckelt), `H1-1`, `H1-1 Length`, `H1-2`, `H1-2 Length`, `Indexability`, `Indexability Status`.
- **Akzeptanzkriterien:**
  - [ ] `<h1>Hallo <span>Welt</span></h1>` → „Hallo Welt“.
  - [ ] `<h1><img src=l.png alt="Logo"> Firma</h1>` → „Logo Firma“ (A-30).
  - [ ] `<h1>A<br>B</h1>` → „A B“ (`<br>` gilt als Leerraum, **Annahme A-31**).
  - [ ] `h1::before { content: "X" }` beeinflusst den Wert nicht.
  - [ ] Versteckte H1 (`style="display:none"`) wird gezählt (keine Sichtbarkeitsprüfung).
- **Testfixture:** `onpage-h1/` mit `basic.html`, `nested.html`, `img-alt.html`, `br.html`, `pseudo.html`, `hidden.html`.

### ONPAGE-121 H1-Filter Missing
- **Priorität:** P0
- **Beschreibung:** Seiten ohne H1 oder bei denen alle H1 nach N(x) leer sind.
- **Eingaben/Konfiguration:** keine.
- **Ausgabe:** Filter `Missing`.
- **Akzeptanzkriterien:**
  - [ ] Ohne `<h1>` → im Filter; `<h1></h1>` → im Filter; `<h1><img src=a.png></h1>` (Bild ohne alt) → im Filter.
  - [ ] `<h1><img src=a.png alt="Logo"></h1>` → nicht (Wert „Logo“), aber in Alt Text in h1.
- **Testfixture:** `onpage-h1/missing-*.html`.

### ONPAGE-122 H1-Filter Duplicate
- **Priorität:** P0
- **Beschreibung:** Duplikat-Logik ONPAGE-005 auf `H1-1`.
- **Eingaben/Konfiguration:** `duplicates.only_indexable`, `duplicates.ignore_paginated` (ONPAGE-005).
- **Ausgabe:** Filter `Duplicate`.
- **Akzeptanzkriterien:** wie ONPAGE-005, angewendet auf H1-1.
- **Testfixture:** `onpage-duplicates/`.

### ONPAGE-123 H1-Filter Over 70 Characters
- **Priorität:** P0
- **Beschreibung:** Seiten mit `H1-1 Length` > `h1.max_chars` (Default 70). Hinweischarakter (Überschriften haben kein technisches Limit).
- **Eingaben/Konfiguration:** `h1.max_chars`.
- **Ausgabe:** Filter `Over 70 Characters`.
- **Akzeptanzkriterien:**
  - [ ] 70 → nicht; 71 → im Filter.
  - [ ] Nur H1-1 zählt (**Annahme A-32**).
- **Testfixture:** `onpage-h1/len-70.html`, `len-71.html`.

### ONPAGE-124 H1-Filter Multiple
- **Priorität:** P0
- **Beschreibung:** Seiten mit `Occurrences` > 1 (zählt auch leere H1).
- **Eingaben/Konfiguration:** keine.
- **Ausgabe:** Filter `Multiple`.
- **Akzeptanzkriterien:**
  - [ ] Zwei H1 → im Filter; eine → nicht; fünf → im Filter mit Occurrences 5.
- **Testfixture:** `onpage-h1/five-h1.html`, `basic.html`.

### ONPAGE-125 H1-Filter Alt Text in h1
- **Priorität:** P1
- **Beschreibung:** Seiten, bei denen mindestens ein `<h1>` ein `<img>` mit nicht-leerem `alt` als Nachfahren enthält (typisch: Logo in H1 durch CMS-Template).
- **Eingaben/Konfiguration:** keine.
- **Ausgabe:** Filter `Alt Text in h1`.
- **Akzeptanzkriterien:**
  - [ ] `<h1><img src=l.png alt="Logo"></h1>` → im Filter.
  - [ ] `<h1><a href="/"><img alt="Firma"></a></h1>` → im Filter (verschachtelt).
  - [ ] `<h1><img src=l.png alt=""></h1>` → nicht.
  - [ ] Alt-Text in zweiter H1 → im Filter.
- **Testfixture:** `onpage-h1/alt-*.html`.

### ONPAGE-126 H1-Filter Non-sequential
- **Priorität:** P1
- **Beschreibung:** Seiten mit mindestens einem H1, deren erste Überschrift (h1–h6) in Dokumentreihenfolge **kein** H1 ist.
- **Eingaben/Konfiguration:** keine.
- **Ausgabe:** Filter `Non-sequential`.
- **Akzeptanzkriterien:**
  - [ ] `<h2>…<h1>…` → im Filter.
  - [ ] `<h1>…<h2>…` → nicht.
  - [ ] Seite ohne H1 (`<h2>…<h3>…`) → nicht (gehört zu Missing).
  - [ ] Leere `<h3></h3>` vor dem H1 → im Filter (leere Überschriften zählen in der Sequenz, **Annahme A-33**).
- **Testfixture:** `onpage-h1/seq-*.html`.

---

## 11 H2

Grundmenge, Extraktion und Normalisierung wie H1 (Level 2).

### ONPAGE-130 H2: Extraktion und Spalten
- **Priorität:** P0
- **Beschreibung:** wie ONPAGE-120 für `<h2>`.
- **Eingaben/Konfiguration:** Speicher-Schalter „H2“.
- **Ausgabe (Spalten):** `Address`, `Occurrences` (ungedeckelt), `H2-1`, `H2-1 Length`, `H2-2`, `H2-2 Length`, `Indexability`, `Indexability Status`.
- **Akzeptanzkriterien:**
  - [ ] Seite mit 12 H2 → Occurrences 12, H2-1/H2-2 gefüllt.
  - [ ] Verschachtelter Text wird wie bei H1 zusammengesetzt.
- **Testfixture:** `onpage-h2/twelve.html`.

### ONPAGE-131 H2-Filter Missing
- **Priorität:** P0
- **Beschreibung:** Seiten ohne H2 oder nur mit leeren H2.
- **Eingaben/Konfiguration:** keine.
- **Ausgabe:** Filter `Missing`.
- **Akzeptanzkriterien:**
  - [ ] Ohne H2 → im Filter; `<h2> </h2>` → im Filter; mit Text → nicht.
- **Testfixture:** `onpage-h2/missing-*.html`.

### ONPAGE-132 H2-Filter Duplicate
- **Priorität:** P0
- **Beschreibung:** Duplikat-Logik ONPAGE-005 auf `H2-1`.
- **Eingaben/Konfiguration:** `duplicates.only_indexable`, `duplicates.ignore_paginated` (ONPAGE-005).
- **Ausgabe:** Filter `Duplicate`.
- **Akzeptanzkriterien:** wie ONPAGE-005, angewendet auf H2-1.
- **Testfixture:** `onpage-duplicates/`.

### ONPAGE-133 H2-Filter Over 70 Characters
- **Priorität:** P0
- **Beschreibung:** Seiten mit `H2-1 Length` > `h2.max_chars` (Default 70) (**Annahme A-32** analog).
- **Eingaben/Konfiguration:** `h2.max_chars`.
- **Ausgabe:** Filter `Over 70 Characters`.
- **Akzeptanzkriterien:**
  - [ ] 70 → nicht; 71 → im Filter.
- **Testfixture:** `onpage-h2/len-70.html`, `len-71.html`.

### ONPAGE-134 H2-Filter Multiple
- **Priorität:** P0
- **Beschreibung:** Seiten mit mehr als einer H2. Hinweischarakter: mehrere H2 sind in einer logischen Gliederung normal; der Filter dient der schnellen Durchsicht.
- **Eingaben/Konfiguration:** keine.
- **Ausgabe:** Filter `Multiple`.
- **Akzeptanzkriterien:**
  - [ ] Zwei H2 → im Filter; eine → nicht.
- **Testfixture:** `onpage-h2/twelve.html`, `one.html`.

### ONPAGE-135 H2-Filter Non-sequential
- **Priorität:** P1
- **Beschreibung:** Seiten mit mindestens einem H2, bei denen vor dem ersten H2 (in Dokumentreihenfolge) eine Überschrift der Ebenen 3–6 steht. Damit gilt: `h1, h3, h2` → Treffer; `h1, h2, h3, h2` → kein Treffer (Rücksprung von h3 auf h2 ist korrekt) (**Annahme A-34**).
- **Eingaben/Konfiguration:** keine.
- **Ausgabe:** Filter `Non-sequential`.
- **Akzeptanzkriterien:**
  - [ ] `h1, h3, h2` → im Filter.
  - [ ] `h1, h2, h3` → nicht.
  - [ ] `h1, h2, h3, h2` → nicht.
  - [ ] `h4, h1, h2` → im Filter (und H1 Non-sequential).
  - [ ] Seite ohne H2 → nicht.
  - [ ] `h2` als erste Überschrift ohne H1 → nicht in H2 Non-sequential (A-34).
- **Testfixture:** `onpage-h2/seq-*.html` (6 Varianten).

---

## 12 Content

Grundmenge: ONPAGE-009 (interne HTML-Seiten mit Status 2xx; zusätzlich interne PDFs mit Status 2xx, wenn `pdf.store = true` – ONPAGE-166). Für PDFs ersetzt der extrahierte Dokumenttext den Inhaltsbereich (ONPAGE-140 gilt nur für HTML).

### ONPAGE-140 Inhaltsbereich (Content Area)
- **Priorität:** P0
- **Beschreibung:** Legt fest, welcher Text einer Seite in Wortzählung, Near-Duplicates, Lesbarkeit, Rechtschreibung/Grammatik (Body-Teil), Low Content, Soft 404, Lorem Ipsum und Embeddings eingeht. Basis ist der Textinhalt des `<body>`. Standardmäßig ausgeschlossen: Elemente `nav`, `footer`, `header` und `aside` samt Nachfahren (K2-87/SQ-09, bestätigt durch N08 und K2-95: M1 38/38, Word Count ohne Zellfehler). Immer ausgeschlossen: `script`, `style`, `noscript`, `template`, `svg`, Kommentare. Konfigurierbar sind Listen „Include“ und „Exclude“ für Elementnamen, Klassen und IDs.
  - Ist mindestens eine Include-Regel gesetzt, wird nur Text innerhalb passender Elemente verwendet; Exclude-Regeln wirken danach (Exclude gewinnt bei Verschachtelung).
  - Block-Grenzen (`p`, `div`, `li`, `td`, `th`, `h1`–`h6`, `br`, `section`, `article` …) erzeugen einen Leerraum, damit `<p>a</p><p>b</p>` zu „a b“ wird, nicht „ab“.
  - Versteckter Text (CSS `display:none`, `hidden`-Attribut) wird **nicht** ausgeschlossen.
  - Im Rendering-Modus wird das gerenderte DOM verwendet.
- **Eingaben/Konfiguration:** `content.area.include_elements|classes|ids`, `content.area.exclude_elements|classes|ids` (Default exclude_elements = [`nav`, `footer`, `header`, `aside`], K2-95).
- **Ausgabe:** gespeicherter Seitentext je URL (komprimiert in `os-store`, Voraussetzung für Neuanalyse ohne Recrawl).
- **Akzeptanzkriterien:**
  - [ ] Default: Text in `<nav>` und `<footer>` fehlt; Text in `<main>` ist enthalten; Text in `<header>` und `<aside>` fehlt (SQ-09/K2-95) (Beleg Referenz `parity-m2-links` `positions.html`: Word Count 1).
  - [ ] Exclude-Klasse `mobile-menu` entfernt `<div class="menu mobile-menu">…</div>` (Klassenvergleich tokenweise).
  - [ ] Exclude-ID `comments` entfernt `<section id="comments">`.
  - [ ] Include-Element `main` → nur Text aus `<main>`.
  - [ ] `<p>a</p><p>b</p>` ergibt zwei Wörter.
  - [ ] Text in `<div hidden>` ist enthalten.
  - [ ] Änderung des Inhaltsbereichs nach dem Crawl wirkt auf Word Count ohne HTTP-Anfragen (Re-Analyse aus gespeichertem HTML; setzt Speichern des HTML voraus – ohne gespeichertes HTML zeigt die UI „Recrawl nötig“).
- **Testfixture:** `onpage-content/area.html` mit `<header>` (5 Wörter), `<nav>` (20), `<main>` (100), `<div class="menu mobile-menu">` (15), `<section id="comments">` (30), `<aside>` (10), `<footer>` (25), `<div hidden>` (3).

### ONPAGE-141 Wortzählung
- **Priorität:** P0
- **Beschreibung:** `Word Count` = Anzahl Token des Inhaltsbereich-Textes, getrennt an Unicode-Leerraum. Jedes nicht-leere Token zählt, auch reine Satzzeichen-Token wie „–“ (**Annahme A-35**). **Dazu kommt je Listenelement `<li>` im Inhaltsbereich ein Token** (**Annahme A-35b**, Paritätsbefund M1 vom 14.10.2026, Befund N-03, K-59, präzisiert durch K-68 vom 15.10.2026). M1-P31 hat die Regel zellgenau geprüft: Token + Anzahl `<li>` im Inhaltsbereich trifft `Word Count` auf allen 211 HTML-Seiten der Paritäts-Sites (0 Abweichungen). Die vorherige Fassung „+ 1 je `<a href>`“ ist widerlegt: Links in `<p>` zählen in der Referenz nicht (`out-basic-site`, `parity-technical` `/links/*`). `<li>` in ausgeschlossenen Bereichen (`nav`, `footer`, Exclude-Regeln) zählen nicht, Include-Regeln gelten. Auf allen Fixture-Seiten enthält jedes `<li>` genau einen Link; ein `<li>` ohne Link ist gegen die Referenz nicht belegt und zählt nach der Regel ebenfalls (Annahme, M2-S06). **K2-87/SQ-09, bestätigt durch N08 (K2-95):** Jede Grenze eines `<a>`-Elements trennt wie Leerraum, aneinandergrenzende Anker werden nicht verkettet (Beleg Referenz `parity-m2-links` `anchors.html`, K-68 grün). Keine Sprachsegmentierung (CJK-Text ohne Leerzeichen zählt als ein Wort pro Block – bekannte Einschränkung, dokumentieren).
- **Eingaben/Konfiguration:** Inhaltsbereich.
- **Ausgabe:** Spalte `Word Count` (Internal, Content).
- **Akzeptanzkriterien:**
  - [ ] „Das ist ein Test.“ → 4.
  - [ ] „Preis: 10 € – jetzt!“ → 5 (`Preis:`, `10`, `€`, `–`, `jetzt!`).
  - [ ] „E-Mail-Adresse“ → 1.
  - [ ] `a&nbsp;b` → 2 (NBSP trennt).
  - [ ] Leerer Body → 0.
  - [ ] `area.html` aus ONPAGE-140 → 100 + 15 + 30 + 3 = 148 (`nav`, `footer`, `header` und `aside` ausgeschlossen; keine Listenelemente `<li>` im Inhaltsbereich).
  - [ ] [vorbehaltlich N08, SQ-09] `<main><p><a href="/a">click here</a><a href="/b">read more</a><a href="/c"></a><a href="/d">River guide</a></p></main>` → 6.
  - [ ] `<main><p>eins zwei</p><ul><li><a href="/a">Seite 1</a></li><li><a href="/b">Seite 2</a></li></ul></main><nav><a href="/c">Menü</a></nav>` → 2 + 4 + 2 = 8 (A-35b: zwei `<li>`; `nav` ist ausgeschlossen).
  - [ ] `<main><p>Mehr <a href="/x">hier lesen</a></p></main>` → 3 (ein Link im Absatz zählt nicht zusätzlich; K-68).
  - [ ] `<main><ul><li>eins</li><li>zwei</li></ul></main>` → 4 (`<li>` ohne Link zählt ebenfalls; Annahme A-35b, unbelegt).
- **Testfixture:** `onpage-content/wordcount-*.html`, `area.html`.

### ONPAGE-142 Content: Spalten
- **Priorität:** P0
- **Beschreibung:** Spalten des Content-Tabs.
- **Eingaben/Konfiguration:** Aktivierungsschalter der Teilfunktionen.
- **Ausgabe (Spalten):** `Address`, `Word Count`, `Sentence Count`¹, `Average Words Per Sentence`, `Flesch Reading Ease Score`, `Readability`, `Closest Similarity Match`, `No. Near Duplicates`, `Closest Semantically Similar Address`, `Semantic Similarity Score`, `No. Semantically Similar`, `Semantic Relevance Score`, `Total Language Errors`, `Spelling Errors`, `Grammar Errors`, `Language`, `Hash`, `Indexability`, `Indexability Status`.
  - Spalten von P1-Funktionen (Near Duplicates ONPAGE-144, Lesbarkeit ONPAGE-149, Rechtschreibung/Grammatik ONPAGE-153 ff., Embeddings INTEG-181/182) erscheinen erst mit diesen Funktionen; Berechnung und Logik der Embedding-Spalten `Closest Semantically Similar Address`, `Semantic Similarity Score`, `No. Semantically Similar`, `Semantic Relevance Score` sind in INTEG-181/182 spezifiziert (führend).
  - ¹ `Sentence Count` ist in der Spaltenliste der Referenz-Tab-Doku **nicht** aufgeführt, wird aber im Lesbarkeits-Tutorial der Referenz als angezeigter Wert genannt. Status: „laut Tutorial“, im Paritätstest V-5 zu bestätigen; bis dahin wird die Spalte geführt (sie wird für `Average Words Per Sentence` ohnehin berechnet).
- **Akzeptanzkriterien:**
  - [ ] Spalten deaktivierter Funktionen sind leer und standardmäßig ausgeblendet.
  - [ ] [P1] Eine interne PDF (mit `pdf.store = true`) hat eine Zeile mit gefülltem `Word Count`, `Sentence Count`, `Average Words Per Sentence`, `Flesch Reading Ease Score`, `Readability` und – bei aktivierter Prüfung – `Spelling Errors`/`Grammar Errors`; `Hash`, `Closest Similarity Match` und Embedding-Spalten bleiben für PDFs leer (**Annahme A-53**).
  - [ ] [P1] `Total Language Errors` = `Spelling Errors` + `Grammar Errors` (leer, wenn beide Prüfungen aus; wenn nur eine aktiv, gleich dieser).
  - [ ] `Hash` identisch mit Internal-Tab.
- **Testfixture:** `onpage-content/`.

### ONPAGE-143 Content-Filter Exact Duplicates
- **Priorität:** P0
- **Beschreibung:** Seiten, deren `Hash` (MD5 über das vollständige HTML, ONPAGE-018) mit mindestens einer anderen Seite der Vergleichsmenge übereinstimmt. Vergleichsmenge = Grundmenge, bei `duplicates.only_indexable = true` (Default) nur indexierbare Seiten. Wird bereits während des Crawls befüllt (keine Crawl-Analyse nötig). Jedes Byte zählt: abweichendes Markup (z. B. anderer Canonical, Session-ID im HTML) verhindert die Erkennung.
- **Eingaben/Konfiguration:** Speicher-Schalter „Hash“ (Extraktion, Default an), `duplicates.only_indexable`. Referenzverhalten: Wird „Hash“ abgeschaltet, wird kein Hash gespeichert und die Erkennung exakter Duplikate findet nicht statt. `duplicates.exact_enabled` (Default true) ist eine zusätzliche OpenSpider-Option, um nur den Filter abzuschalten, den Hash aber zu behalten.
- **Ausgabe:** Filter `Exact Duplicates`; Lower-Tab „Duplicate Details“ listet die übrigen Seiten mit gleichem Hash.
- **Akzeptanzkriterien:**
  - [ ] Speicher-Schalter „Hash“ aus: Spalte `Hash` in Internal/Content ausgeblendet und leer, Filter `Exact Duplicates` zeigt den Zustand `needs_config(extraction.hash)` statt Treffern; nach Wiedereinschalten ist ein Recrawl nötig (Hash wird beim Abruf berechnet), sofern kein HTML gespeichert ist – mit gespeichertem HTML wird der Hash ohne HTTP-Anfragen nachberechnet.
  - [ ] `/a` und `/b` mit byte-identischem HTML → beide im Filter.
  - [ ] `/c` identisch bis auf ein Leerzeichen → nicht im Filter.
  - [ ] `/a?ref=x` liefert dasselbe HTML wie `/a` → beide im Filter (typischer Parameter-Duplicate).
  - [ ] Identisches HTML, eine Seite mit `X-Robots-Tag: noindex` → nicht im Filter (Default); mit `only_indexable=false` → beide.
  - [ ] Identische 404-Seiten erscheinen nie (nicht in der Grundmenge).
- **Testfixture:** `onpage-content-dupes/` mit `a.html`, `b.html` (Kopie), `c.html` (+1 Leerzeichen), Route `/a?ref=x` → gleiche Datei (Route ohne Query-Matching liefert statische Datei), Route `/noidx` (Kopie + Header).

### ONPAGE-144 Content-Filter Near Duplicates (MinHash)
- **Priorität:** P1
- **Beschreibung:** Seiten, deren Inhaltsbereich-Text mit mindestens einer anderen Seite eine Ähnlichkeit ≥ Schwellwert (Default 90 %) hat. Verfahren:
  1. Text normalisieren: Kleinschreibung (Unicode), Satzzeichen entfernen, Leerraum zusammenfassen.
  2. Shingles: überlappende Wort-n-Gramme mit n = 3 (**Annahme A-36**; Seiten mit < 3 Wörtern: ein Shingle aus allen Wörtern).
  3. MinHash-Signatur (128 Hashfunktionen, deterministischer Seed) je Seite; Kandidatenpaare über LSH-Banding. Die Band-Aufteilung (b Bänder × r Zeilen, b × r = 128) wird aus dem Schwellwert so gewählt, dass die Kandidatenwahrscheinlichkeit für ein Paar mit Jaccard = Schwellwert ≥ 0,999 ist (z. B. 90 % → 32 × 4); bei Schwellwertänderung wird neu gebandet.
  4. Für Kandidaten wird die Jaccard-Ähnlichkeit der Shingle-Mengen exakt berechnet (deterministisch, keine Schätzungs-Streuung); Ergebnis in Prozent. Ein Paar gilt als Near Duplicate, wenn die exakte Jaccard-Ähnlichkeit ≥ Schwellwert ist (Grenze inklusive).
  5. Paare, die bereits **exakte Duplikate** sind (gleicher Hash), werden nicht als Near Duplicates gezählt. Seiten mit gleichem Text aber unterschiedlichem HTML können 100 % erreichen und erscheinen hier.
  - `Closest Similarity Match` = höchste Ähnlichkeit zu irgendeiner anderen Seite, nur gefüllt wenn ≥ Schwellwert; Werte ≥ 99,5 % werden als 100 % angezeigt; Anzeige ganzzahlig gerundet.
  - `No. Near Duplicates` = Anzahl anderer Seiten mit Ähnlichkeit ≥ Schwellwert.
  - Vergleichsmenge wie ONPAGE-143 (`only_indexable`).
  - Erfordert `duplicates.near_enabled = true` (Text wird gespeichert) und Crawl-Analyse.
- **Eingaben/Konfiguration:** `duplicates.near_enabled` (Default false), `duplicates.near_threshold_pct` (Default 90, Bereich 50–100), `duplicates.only_indexable`, Inhaltsbereich.
- **Ausgabe:** Filter `Near Duplicates`; Spalten `Closest Similarity Match`, `No. Near Duplicates`; Lower-Tab ONPAGE-145.
- **Akzeptanzkriterien:**
  - [ ] Unit-Test Shingling: „Der Hund bellt laut.“ → normalisiert „der hund bellt laut“ → Shingles {„der hund bellt“, „hund bellt laut“}; „Hund bellt“ (2 Wörter) → ein Shingle {„hund bellt“}.
  - [ ] Unit-Test Jaccard: Mengen mit |A ∩ B| = 9 und |A ∪ B| = 10 → 90,0 % → bei Schwellwert 90 ein Near Duplicate (Grenze inklusive); 8/9 = 88,9 % → kein Near Duplicate.
  - [ ] Fixture (Varianten sind so konstruiert, dass sie untereinander < 90 % ähnlich sind): `near-95` hat zur Basis 95,0 %, `near-91` 91,0 %, `near-89` 89,0 %, `near-80` 80,0 %. Erwartung bei Schwellwert 90: `near-base` → Closest 95, `No. Near Duplicates` 2; `near-95` → Closest 95, Anzahl 1; `near-91` → Closest 91, Anzahl 1.
  - [ ] `near-89` und `near-80` stehen nicht im Filter und haben leere Spalten `Closest Similarity Match` / `No. Near Duplicates`.
  - [ ] Schwellwert auf 75 senken und Crawl-Analyse erneut ausführen → `near-80` im Filter, ohne HTTP-Anfragen (Request-Zähler unverändert).
  - [ ] Anzeige-Rundung: exakte Ähnlichkeit 94,6 % → „95“; 99,6 % → „100“; 99,4 % → „99“.
  - [ ] Exaktes Duplikat-Paar (`a`/`b` aus ONPAGE-143, gleicher Hash) → zählt nicht als Near Duplicate zueinander (`No. Near Duplicates` von `a` enthält `b` nicht).
  - [ ] `nav-variant-1/2.html` (gleicher `<main>`, andere `<nav>`, daher verschiedener Hash) → beide im Filter mit 100.
  - [ ] `only_indexable = true`: `near-95-noindex` (Text wie `near-95`, aber `noindex`) steht nicht im Filter und erhöht `No. Near Duplicates` von `near-base` nicht; mit `false` schon.
  - [ ] Ohne `duplicates.near_enabled`: Filterzustand `needs_config(duplicates.near_enabled)`; aktiviert, aber ohne Crawl-Analyse: `needs_crawl_analysis`.
  - [ ] Determinismus: zwei Läufe auf derselben Crawl-Datei liefern byte-identische Exporte.
  - [ ] Recall des Kandidatenverfahrens: Auf einem synthetischen Korpus von 2 000 Seiten mit 300 eingebauten Paaren zwischen 90 % und 100 % findet das Verfahren ≥ 299 Paare (Vergleich mit Brute-Force-Jaccard); falsch-positive Paare < Schwellwert gibt es durch Schritt 4 nie.
  - [ ] Performance: 100 000 Seiten à 500 Wörter in < 60 s auf Referenzhardware (M1, 8 Kerne), Speicher < 2 GB.
- **Testfixture:** `onpage-content-dupes/near-*.html`: Basistext `near-base` mit 300 Wörtern (generierter Fließtext, fest eingecheckt) und Varianten `near-95`, `near-91`, `near-89`, `near-80` mit ersetzten Wortanteilen, deren Jaccard-Werte zur Basis (und paarweise untereinander) im Generator-Skript vorberechnet und in `expected_jaccard.json` eingecheckt sind; `near-95-noindex.html`; `nav-variant-1/2.html` (gleicher main, andere nav). Synthetischer Recall-Korpus wird im Test generiert (fester Seed).

### ONPAGE-145 Lower-Tab Duplicate Details
- **Priorität:** P1
- **Beschreibung:** Für eine ausgewählte URL listet der Lower-Tab „Duplicate Details“ alle exakten Duplikate und Near Duplicates mit den Spalten `Address`, `Similarity` (ganzzahlige Prozent, Rundung wie ONPAGE-144) und `Match Type` (`Exact` | `Near`; OpenSpider-Erweiterung zur Unterscheidung). Exakte Duplikate stehen mit Similarity 100 oben. Sortierung: Similarity absteigend, bei Gleichstand Address aufsteigend. Klick auf eine Zeile öffnet eine Textansicht mit dem analysierten Text (Inhaltsbereich) der Ausgangsseite links und der gewählten Vergleichsseite rechts; Unterschiede werden auf Wortebene hervorgehoben (Diff nach Myers über die Token aus ONPAGE-141; gelöschte Wörter links, eingefügte rechts markiert).
- **Eingaben/Konfiguration:** Filter im Lower-Tab: „Exact“, „Near“, „Semantic“ (Inhalt des Filters „Semantic“: INTEG-183, führend).
- **Ausgabe:** Lower-Tab; Bulk-Export „Content > Near Duplicates“ mit Spalten `Address`, `Near Duplicate Address`, `Similarity`.
- **Akzeptanzkriterien:**
  - [ ] Auswahl von `near-base` zeigt genau zwei Zeilen: `near-95` (95, Near) vor `near-91` (91, Near); `near-89`/`near-80` erscheinen nicht.
  - [ ] Auswahl von `a.html` (ONPAGE-143) zeigt `b.html` mit Similarity 100 und Match Type `Exact`, und `a?ref=x` ebenso; Sortierung bei Gleichstand nach Address.
  - [ ] Auswahl einer Seite ohne Duplikate zeigt eine leere Liste mit Hinweistext; ohne Crawl-Analyse zeigt der Filter „Near“ den Zustand `needs_crawl_analysis`, „Exact“ ist trotzdem befüllt.
  - [ ] Diff: Für `near-base` vs. `near-95` sind exakt die Wortpositionen markiert, die in `expected_diff.json` des Generators stehen (keine zusätzlichen, keine fehlenden Markierungen).
  - [ ] Bulk-Export enthält je Paar zwei Zeilen (A→B und B→A) (**Annahme A-37**): für die Fixture bei Schwellwert 90 genau 4 Zeilen aus den Paaren (base, 95) und (base, 91) plus 2 Zeilen für das `nav-variant`-Paar = 6 Zeilen.
- **Testfixture:** `onpage-content-dupes/` (inkl. `expected_diff.json`).

### ONPAGE-146 Content-Filter Low Content Pages
- **Priorität:** P0
- **Beschreibung:** Seiten der Grundmenge mit `Word Count` < `content.low_word_count` (Default 200). Indexierbarkeit spielt keine Rolle (**Annahme A-38**), außer `issues.ignore_non_indexable = true` (ONPAGE-009) – dann fallen nicht indexierbare Seiten heraus. PDFs (nur mit `pdf.store`) werden nicht geprüft (**Annahme A-53**).
- **Eingaben/Konfiguration:** `content.low_word_count`, `issues.ignore_non_indexable`.
- **Ausgabe:** Filter `Low Content Pages`.
- **Akzeptanzkriterien:**
  - [ ] 199 Wörter → im Filter; 200 → nicht.
  - [ ] Seite mit 150 Wörtern im main und 300 im footer → im Filter (footer zählt nicht).
  - [ ] Bilder erscheinen nie (nicht in Grundmenge); eine PDF mit 50 Wörtern erscheint auch mit `pdf.store = true` nicht (A-53).
  - [ ] `noindex`-Seite mit 50 Wörtern → im Filter bei `issues.ignore_non_indexable = false`, nicht im Filter bei `true`.
- **Testfixture:** `onpage-content/words-199.html`, `words-200.html`, `footer-heavy.html`, `noindex-50.html`, `short.pdf` (50 Wörter).

### ONPAGE-147 Content-Filter Soft 404 Pages
- **Priorität:** P1
- **Beschreibung:** Seiten mit Status 200, deren Titel, H1 oder Inhaltsbereich-Text eine der konfigurierten Phrasen enthält (Teilstring, case-insensitiv, nach Normalisierung N(x)) (**Annahme A-39** zu Prüforten). Default-Phrasenliste (OpenSpider-eigen, mehrsprachig): `page not found`, `404 not found`, `error 404`, `404 error`, `page cannot be found`, `page can't be found`, `page could not be found`, `seite nicht gefunden`, `seite wurde nicht gefunden`, `fehler 404`, `404 fehler`, `page introuvable`, `página no encontrada`, `pagina non trovata`, `pagina niet gevonden`. Seiten mit Statuscode ≠ 200 werden nicht geprüft.
- **Eingaben/Konfiguration:** `content.soft404_phrases` (Liste, editierbar; leere Liste deaktiviert den Filter).
- **Ausgabe:** Filter `Soft 404 Pages`; Spalte `Matched Phrase` im Filter (Erweiterung).
- **Akzeptanzkriterien:**
  - [ ] 200-Seite mit `<h1>Seite nicht gefunden</h1>` → im Filter, Matched Phrase „seite nicht gefunden“.
  - [ ] 200-Seite mit Titel „Page Not Found – Shop“ → im Filter.
  - [ ] Echte 404-Seite mit gleichem Text → nicht im Filter.
  - [ ] Phrase nur im `<footer>` (Default-Inhaltsbereich) → nicht im Filter.
  - [ ] Eigene Phrase „Produkt nicht mehr verfügbar“ ergänzen → passende Seite erscheint nach Neu-Analyse ohne Recrawl.
  - [ ] Blogartikel mit Satz „Was tun bei Error 404?“ im Text → im Filter (bekannte Falsch-Positive sind akzeptiert; Dokumentation weist darauf hin).
- **Testfixture:** `onpage-content/soft404-*.html` und Route `/real-404` (404, gleicher Body).

### ONPAGE-148 Content-Filter Lorem Ipsum Placeholder
- **Priorität:** P1
- **Beschreibung:** Seiten, deren Titel, Meta-Description oder `<body>`-Text (gesamter Body, nicht nur Inhaltsbereich, **Annahme A-40**) die Zeichenfolge „lorem ipsum“ enthält (case-insensitiv, beliebiger Leerraum zwischen den Wörtern).
- **Eingaben/Konfiguration:** keine.
- **Ausgabe:** Filter `Lorem Ipsum Placeholder`.
- **Akzeptanzkriterien:**
  - [ ] „Lorem ipsum dolor sit amet“ im main → im Filter.
  - [ ] „LOREM\n  IPSUM“ im Footer → im Filter (A-40).
  - [ ] „lorem“ allein → nicht.
  - [ ] „Lorem Ipsum“ nur in einem HTML-Kommentar oder `<script>` → nicht.
- **Testfixture:** `onpage-content/lorem-*.html`.

### ONPAGE-149 Lesbarkeit: Flesch-Berechnung und Spalten
- **Priorität:** P1
- **Beschreibung:** Berechnet für jede Seite der Grundmenge mit ≥ 1 Wort den Flesch Reading Ease Score (englische Formel) über den Inhaltsbereich-Text:
  - **Kalibrierung K2-87/SQ-08, K2-95:**
    1. W aus der Wortzahl des Inhaltsbereichs einschließlich Zahlentoken und A-35b. Auf der Startseite ergeben 55 Linktexte plus 55 Listeneinträge W=110. Der Satztext enthält keine künstlichen Listenpunktzeichen.
    2. Erst nichtleere Satzfragmente an `. ! ? … :` sowie Blockgrenzen bilden; Dezimalpunkte zwischen ASCII-Ziffern und Initialismen wie `z.B.` bleiben erhalten. Auch Punkte/Fragezeichen innerhalb von URL-Texten trennen. Anschließend jedes Fragment an Wortgrenzen umbrechen, bevor die Zeile 80 Unicode-Zeichen erreicht (höchstens 79). Lange einzelne Token werden nicht zerschnitten. Satzzeichen belegen keine Umbruchbreite. Ankergrenzen allein sind keine Satzgrenzen. Gegenprobe: auf allen 379 synthetischen HTML-Seiten stimmt S; Breite 80 Zeichen ergibt 28 Abweichungen, 80 Bytes 20, 79 Zeichen vor Abtrennung des Endpunkts noch 3. Die Trennung vor dem Umbruch beseitigt auch diese drei.
    3. Kleinschreibung; Trennzeichen innerhalb und am Ende eines Tokens bleiben erhalten, damit Vokalgruppen nicht über Bindestriche oder `&` zusammenfallen und `notice.` anders als `notice` gezählt wird. Stummes `ed`/`es` am Tokenende entfernen; `es` nach c/g/s/x/z bzw. ch/sh bleibt. ASCII-Vokalgruppen aeiouy zählen. `ia`/`io` zählen eine zusätzliche Gruppe, außer nach c/s/t. Für abschließendes e eine Gruppe abziehen, außer bei ee oder Konsonant+le. Die Formel verwendet den rohen Gruppenzähler einschließlich **0 für „the“**; die öffentliche Einzelwort-Hilfe `syllables` behält ihr spezifiziertes Minimum 1. Damit ergeben sich beim Absatz mit vier „the“ Syl=28, bei `kategorie` und `variante` jeweils 3, bei `tree` 1 und `images` 3. Die Mitnahme von äöü erklärt zusätzliche Silben und zu niedrige Scores auf der alten synthetischen Site.
    4. Score nach der unveränderten Flesch-Formel, Begrenzung 0…100, Rundung erst am Ende auf drei Nachkommastellen. Labels nach der Banddatei.
  - **Score** = 206,835 − 1,015 × (W/S) − 84,6 × (Syl/W), auf [0, 100] begrenzt, drei Nachkommastellen (K2-87, SQ-08; vorher eine).
  - **Readability**-Label nach der Werteliste der Referenz (V-5 durch Q02 belegt, K2-87, SQ-08; ersetzt A-54): ≥ 90 „Very Easy“; 80 bis < 90 „Easy“; 70 bis < 80 „Fairly Easy“; 60 bis < 70 „Normal“; 50 bis < 60 „Fairly Hard“; 30 bis < 50 „Hard“; < 30 „Very Hard“. Die Filter ONPAGE-150/151 behalten ihre Namen „Readability Difficult“/„Readability Very Difficult“ und entsprechen den Labels „Hard“/„Very Hard“. Die Labels liegen als Daten vor (`readability_bands.toml`).
  - `Sentence Count` = S (Spalte laut Referenz-Tutorial, siehe ONPAGE-142 Fußnote ¹); `Average Words Per Sentence` = W/S (drei Nachkommastellen, K2-87, SQ-08).
  - Die Formel ist für Englisch ausgelegt; sie wird (wie in der Referenz) auf alle Seiten angewandt. UI-Hinweis bei Seiten mit `lang` ≠ `en*`.
- **Eingaben/Konfiguration:** Speicher-Schalter „Readability“ (Default an).
- **Ausgabe:** Spalten `Sentence Count`, `Average Words Per Sentence`, `Flesch Reading Ease Score`, `Readability`.
- **Akzeptanzkriterien:**
  - [ ] Silben: `the`→1, `cake`→1, `table`→2, `beautiful`→3, `readability`→5, `rhythm`→1 (y als Vokal), `a`→1.
  - [ ] „The cat sat on the mat.“ → W=6, S=1, Syl=4 (kalibrierter Rohzähler), Rohscore 144,345 → 100,000, „Very Easy“.
  - [ ] „Internationalization considerations necessitate comprehensive organizational restructuring.“ → Score 0,000, „Very Hard“.
  - [ ] `<p>Ohne Punkt</p><p>Noch ein Satz</p>` → S = 2.
  - [ ] „Es kostet 3.5 Euro.“ → S = 1.
  - [ ] Seite ohne Wörter → alle vier Spalten leer.
  - [ ] Schalter aus → Spalten und Filter ONPAGE-150/151 ausgeblendet.
  - [ ] Label-Zuordnung an den Filtergrenzen: 49,999 → „Hard“, 30,000 → „Hard“, 29,999 → „Very Hard“, 0,000 → „Very Hard“, 50,000 → „Fairly Hard“; die übrigen Grenzen (60, 70, 80, 90) werden gegen `readability_bands.toml` getestet, nicht gegen fest codierte Texte.
  - [ ] Interne PDF mit `pdf.store = true` erhält einen Score aus dem extrahierten Dokumenttext (ONPAGE-166); Satzgrenzen zusätzlich an Absatzwechseln des PDF-Textlayers.
- **Testfixture:** `onpage-content/flesch-*.html` mit den obigen Texten; Unit-Tests für Silbenzählung; zusätzlich `flesch-difficult.html` mit einem Text, dessen Score im Unit-Test auf 30–49,9 vorberechnet ist.

### ONPAGE-150 Content-Filter Readability Difficult
- **Priorität:** P1
- **Beschreibung:** Seiten mit 30 ≤ Flesch Score < 50 (Label „Hard“, K2-87: anspruchsvoller Text mit langen Sätzen und vielen mehrsilbigen Wörtern).
- **Eingaben/Konfiguration:** keine (Bänder fest).
- **Ausgabe:** Filter `Readability Difficult`.
- **Akzeptanzkriterien:**
  - [ ] Score 30,0 → im Filter; 49,9 → im Filter; 50,0 → nicht; 29,9 → nicht.
  - [ ] `flesch-difficult.html` → im Filter.
- **Testfixture:** `onpage-content/flesch-difficult.html`; Grenzwerte als Unit-Test auf der Label-Funktion.

### ONPAGE-151 Content-Filter Readability Very Difficult
- **Priorität:** P1
- **Beschreibung:** Seiten mit Flesch Score < 30 (Label „Very Hard“, K2-87: sehr schwer zugänglicher Text, typischerweise Fach- oder Wissenschaftssprache).
- **Eingaben/Konfiguration:** keine.
- **Ausgabe:** Filter `Readability Very Difficult`.
- **Akzeptanzkriterien:**
  - [ ] Score 0,0 und 29,9 → im Filter; 30,0 → nicht.
  - [ ] Seiten ohne Score (0 Wörter) → nicht.
- **Testfixture:** `onpage-content/flesch-very-difficult.html`.

### ONPAGE-152 Lesbarkeit für weitere Sprachen (Erweiterung)
- **Priorität:** P2
- **Beschreibung:** Optionale, über die Referenz hinausgehende Erweiterung: sprachabhängige Lesbarkeitsformeln, gewählt anhand `Language` (ONPAGE-153): Deutsch nach Amstad (180 − W/S − 58,5 × Syl/W) mit deutscher Silbenheuristik, weitere Sprachen später. Bänder und Filter bleiben gleich. Default aus (Parität).
- **Eingaben/Konfiguration:** `content.readability.language_aware` (Default false).
- **Ausgabe:** zusätzliche Spalte `Readability Formula` (`Flesch (EN)`, `Amstad (DE)`).
- **Akzeptanzkriterien:**
  - [ ] Aus: identische Werte wie ONPAGE-149 für alle Seiten.
  - [ ] An: Seite mit `lang="de"` nutzt Amstad; Seite ohne lang nutzt Flesch.
- **Testfixture:** `onpage-content/flesch-de.html` (`lang="de"`).

### ONPAGE-153 Rechtschreibprüfung: Engine und Sprachwahl
- **Priorität:** P1
- **Beschreibung:** Optionale Prüfung (Default aus) für interne HTML-Seiten der Grundmenge und – wie in der Referenz – für interne PDFs, sofern `pdf.store = true` (ONPAGE-166). Geprüft werden bei HTML drei Bereiche: Titel, Meta-Description und Inhaltsbereich-Text; bei PDFs Dokumenttitel und Dokumenttext. Sprache:
  - Modus „Auto“: aus `<html lang>` (BCP 47, case-insensitiv; `xml:lang` nur, wenn `lang` fehlt). Fehlt die Region (z. B. `en`), gilt die konfigurierte Default-Variante je Sprache (Defaults: `en` → `en-GB`, `de` → `de-DE`, `pt` → `pt-PT`, `fa` → `fa-IR`). Fehlt `lang` ganz oder ist es leer, gilt die globale Default-Sprache (Default `en-GB`). PDFs nutzen die Sprachangabe des Dokumentkatalogs, sonst die globale Default-Sprache.
  - Modus „Manuell“: eine feste Sprache für alle Seiten, `lang` wird ignoriert.
  - Nicht unterstützte Sprache → Seite wird nicht geprüft, `Language` zeigt den Code mit Zusatz „(unsupported)“, `Spelling Errors` bleibt leer.
  - Ausgenommen werden Token mit Ziffern, URLs, E-Mail-Adressen, reine Großbuchstaben-Akronyme bis 5 Zeichen, Wörter aus persönlichem Wörterbuch und Ignorierliste; geprüft werden nur ganze Token.
  - Zählweise: jede Fundstelle zählt einzeln (dasselbe falsche Wort dreimal → 3).
  - **Zielmenge der Sprachen (Parität):** die Sprachliste der Referenz (dort als „40 Sprachen“ beworben; im Tutorial ist von 39 die Rede): Arabisch, Asturisch, Belarussisch, Bretonisch, Katalanisch, Chinesisch, Dänisch, Niederländisch, Englisch (AU, CA, NZ, ZA, US, GB), Französisch, Galicisch, Deutsch (AT, DE, CH), Griechisch, Irisch, Italienisch, Japanisch, Khmer, Persisch (AF, IR), Polnisch, Portugiesisch (AO, BR, MZ, PT), Rumänisch, Russisch, Slowakisch, Slowenisch, Spanisch, Schwedisch, Tagalog, Tamil, Ukrainisch – zusammen 40 Sprach-/Regionsvarianten. Die exakte Liste wird im Paritätstest V-6 am Sprachauswahlfeld der Referenz abgeglichen.
  - **Engine und Ausbaustufen:** Hunspell-kompatible Wörterbücher (Rust-Implementierung mit zulässiger Lizenz; Wörterbuchdateien je Sprache separat ladbar, Lizenz je Wörterbuch prüfen). Stufe 1 (P1, MVP der Prüfung): de-DE, de-AT, de-CH, en-GB, en-US, en-AU, en-CA, en-NZ, en-ZA, fr, es, it, nl, pt-PT, pt-BR, pl, sv, da, ru, ga. Stufe 2 (P1, Parität): alle übrigen Sprachen der Zielmenge mit alphabetischer Schrift (ar, ast, be, br, ca, gl, el, fa-AF, fa-IR, pt-AO, pt-MZ, ro, sk, sl, tl, ta, uk). Stufe 3 (P2): Sprachen ohne Leerzeichen-Wortgrenzen (ja, zh, km) – benötigen eine Wortsegmentierung vor der Prüfung; bis dahin gelten sie als „unsupported“. Über die Referenz hinaus dürfen weitere Sprachen ergänzt werden (z. B. nb, fi, cs).
- **Eingaben/Konfiguration:** `spelling.enabled`, `spelling.mode` (auto/manual), `spelling.manual_language`, `spelling.default_variants` (Abbildung Sprache → Variante), `spelling.default_language`; PDF-Prüfung über `pdf.store` (der frühere Schalter `spelling.check_pdf` entfällt).
- **Ausgabe:** Spalten `Language`, `Spelling Errors` (Anzahl Fehlerinstanzen über alle Bereiche).
- **Akzeptanzkriterien:**
  - [ ] `de-errors.html` (`lang="de"`) mit „Rechtschreibfeler“ im main, „Kontackt“ im Titel, „Beschreibng“ in der Meta-Description → `Language` = `de-DE`, `Spelling Errors` = 3; Lower-Tab-Zeilen mit Page Section „Body“, „Page Title“, „Meta Description“ (Bulk-Export, ONPAGE-157).
  - [ ] Dasselbe Fehlerwort dreimal im main → +3 Fehler.
  - [ ] `lang="en"` → `Language` = `en-GB`; „colour“ kein Fehler, „color“ ein Fehler; nach Setzen der Default-Variante `en` → `en-US` umgekehrt.
  - [ ] `lang="DE-at"` → `Language` = `de-AT` (Groß-/Kleinschreibung egal).
  - [ ] `lang="pt"` → `pt-PT`; `lang="pt-BR"` → `pt-BR`.
  - [ ] `lang="xx"` → `Language` = „xx (unsupported)“, `Spelling Errors` leer, Seite nicht im Filter Spelling Errors.
  - [ ] Ohne `lang` → globale Default-Sprache `en-GB`; Modus „Manuell“ mit `de-DE` prüft auch `en-colour.html` deutsch.
  - [ ] „ISO9001“, „https://example.org“, „info@example.org“, „NASA“ → keine Fehler; „NASAKURS“ (8 Großbuchstaben) wird geprüft.
  - [ ] Fehlerwort nur im `<footer>` → nicht gemeldet (Inhaltsbereich).
  - [ ] PDF `fehler.pdf` mit einem Fehlerwort im Text: mit `pdf.store = true` → `Spelling Errors` = 1; ohne → keine Zeile im Content-Tab.
  - [ ] Für jede Sprache der Stufen 1 und 2 existiert ein Smoke-Test (ein korrektes und ein falsches Wort je Sprache, in `onpage-spelling/langs/<code>.html`).
  - [ ] Prüfung aus → Spalten leer, Filter im Zustand `needs_config(spelling.enabled)`.
- **Testfixture:** `onpage-spelling/` mit `de-errors.html`, `de-repeat.html`, `en-colour.html` (`lang="en"`), `de-at.html`, `pt.html`, `pt-br.html`, `xx.html`, `tokens.html`, `no-lang.html`, `footer-only.html`, `fehler.pdf`, `langs/*.html`.

### ONPAGE-154 Grammatikprüfung: Engine und Regeln
- **Priorität:** P1
- **Beschreibung:** Optionale Prüfung (Default aus) auf denselben Bereichen wie ONPAGE-153. Jede Regel hat eine ID, Kategorie und Beschreibung; Regeln sind einzeln abschaltbar. Engine-Entscheidung offen (Lizenzvorgabe: kein LGPL im Bundle): Option a) regelbasierte Engine mit Apache/MIT-Lizenz (für Englisch z. B. eine Rust-Bibliothek), Option b) optionale Anbindung eines lokal vom Nutzer betriebenen externen Grammatik-Servers über HTTP (nicht gebündelt). Die Regeln laufen auf dem bereits normalisierten Text (Leerraum zusammengefasst, ONPAGE-140), Satzgrenzen wie ONPAGE-149.

  Mindestumfang (sprachunabhängig, sofern nicht anders angegeben):

  | Regel-ID | Kategorie | Prüft | Ausnahmen |
  |---|---|---|---|
  | `WORD_REPEAT` | Wiederholung | zwei identische aufeinanderfolgende Wort-Token (case-insensitiv, Satzzeichen ignoriert) | de: `der der`, `die die`, `das das`, `den den`, `dem dem` (Artikel + Relativpronomen); en: `had had`, `that that` |
  | `SENTENCE_START_LOWER` | Großschreibung | erstes Wort eines Satzes beginnt mit Kleinbuchstaben | Satz beginnt mit Ziffer, URL, E-Mail; Sprachen ohne Groß-/Kleinschreibung |
  | `DOUBLE_PUNCT` | Zeichensetzung | `..`, `,,`, `!!`, `??`, `;;`, `::` (genau zwei gleiche Zeichen) | `...` (Auslassung), `?!`/`!?` |
  | `SPACE_BEFORE_PUNCT` | Zeichensetzung | Leerzeichen direkt vor `,` `.` `;` `:` `!` `?` | fr: Leerzeichen vor `; : ! ?` ist korrekt |
  | `EN_COULD_OF` | Verwechslung (en) | `could of`, `should of`, `would of`, `must of` | – |
  | `DE_SEIT_SEID` | Verwechslung (de) | nur eindeutige Zeitangaben nach „seid“: `seid langem`, `seid kurzem`, `seid gestern`, `seid heute` (Vorschlag „seit …“); die umgekehrte Richtung ist ohne Satzanalyse nicht eindeutig und entfällt | – |

- **Eingaben/Konfiguration:** `grammar.enabled`, `grammar.disabled_rules` (Liste von Regel-IDs).
- **Ausgabe:** Spalte `Grammar Errors` (Anzahl Fundstellen); je Fundstelle Regel-ID, Kategorie, Beschreibung, Vorschlag (für ONPAGE-157).
- **Akzeptanzkriterien:**
  - [ ] „Das ist ist ein Test.“ → genau 1 Fehler `WORD_REPEAT`, Vorschlag „ist“.
  - [ ] „Die Frau, die die Zeitung liest, lacht.“ → 0 Fehler (Ausnahme).
  - [ ] „das ist gut. heute regnet es.“ → genau 2 Fehler `SENTENCE_START_LOWER`; „2026 war gut.“ → 0.
  - [ ] „Wirklich?? Ja.. Na gut...“ → genau 2 Fehler `DOUBLE_PUNCT` (`??`, `..`), die Auslassung `...` nicht.
  - [ ] „Hallo , Welt“ (de) → 1 Fehler `SPACE_BEFORE_PUNCT`; „Bonjour !“ mit `lang="fr"` → 0.
  - [ ] „You could of asked.“ (en) → 1 Fehler `EN_COULD_OF`, Vorschlag „could have“.
  - [ ] „Ich warte seid langem.“ (de) → 1 Fehler `DE_SEIT_SEID`, Vorschlag „seit langem“.
  - [ ] `grammar.disabled_rules = ["WORD_REPEAT"]` → „Das ist ist ein Test.“ liefert nach Neu-Prüfung (ONPAGE-158) 0 Fehler, ohne HTTP-Anfragen.
  - [ ] Grammatikfehler zählen nicht in `Spelling Errors` und umgekehrt; ein falsch geschriebenes Wort, das zugleich doppelt vorkommt („Tesst Tesst“), zählt 2 Rechtschreib- und 1 Grammatikfehler.
- **Testfixture:** `onpage-spelling/grammar-de.html` (alle deutschen Beispielsätze in je eigenem `<p>`), `grammar-en.html`, `grammar-fr.html`; Unit-Tests je Regel.

### ONPAGE-155 Content-Filter Spelling Errors
- **Priorität:** P1
- **Beschreibung:** Seiten mit `Spelling Errors` ≥ 1.
- **Eingaben/Konfiguration:** `spelling.enabled`.
- **Ausgabe:** Filter `Spelling Errors`.
- **Akzeptanzkriterien:**
  - [ ] `de-errors.html` → im Filter; fehlerfreie Seite → nicht.
  - [ ] Nach „Zum Wörterbuch hinzufügen“ für alle 3 Fehlerwörter und Neu-Prüfung → nicht mehr im Filter.
- **Testfixture:** `onpage-spelling/`.

### ONPAGE-156 Content-Filter Grammar Errors
- **Priorität:** P1
- **Beschreibung:** Seiten mit `Grammar Errors` ≥ 1.
- **Eingaben/Konfiguration:** `grammar.enabled`.
- **Ausgabe:** Filter `Grammar Errors`.
- **Akzeptanzkriterien:**
  - [ ] `grammar-de.html` → im Filter; Seite ohne Grammatikfehler → nicht.
- **Testfixture:** `onpage-spelling/`.

### ONPAGE-157 Lower-Tab Spelling & Grammar Details, Ignorieren, Wörterbuch
- **Priorität:** P1
- **Beschreibung:** Lower-Tab „Spelling & Grammar Details“ für die gewählte URL: eine Zeile je Fehlerinstanz. Paritätsspalten (wie in der Referenz), in dieser Reihenfolge: `Error` (Wort/Fundstelle), `Type` (Spelling/Grammar), `Detail` (Beschreibung des Fehlers bzw. der Regel), `Suggestion` (bis zu 3 Vorschläge, durch „, “ getrennt). **OpenSpider-Erweiterungen** (standardmäßig ausgeblendet, einblendbar): `Page Section` (Page Title / Meta Description / Body – in der Referenz nur Spalte des Bulk-Exports, nicht des Lower-Tabs) und `Rule ID` (nur Grammar; in der Referenz nicht belegt). Daneben Textansicht mit hervorgehobenen Fundstellen. Kontextaktionen:
  - „Ignorieren (alle)“: Wort für diesen Crawl ignorieren (Ignorierliste in der Crawl-Konfiguration).
  - „Regel ignorieren“: Grammatikregel für diesen Crawl abschalten.
  - „Zum Wörterbuch hinzufügen“: Wort sprachbezogen im persönlichen Wörterbuch des Benutzers speichern (gilt für alle künftigen Crawls, unabhängig von gespeicherter Konfiguration; Speicherort Benutzer-Konfigurationsverzeichnis via `dirs`).
- **Eingaben/Konfiguration:** persönliches Wörterbuch (Datei je Sprache), Ignorierliste (je Crawl).
- **Ausgabe:** Lower-Tab; Bulk-Export „Content > Spelling & Grammar Errors“ mit Spalten `Address`, `Language`, `Type`, `Error`, `Detail`, `Suggestion`, `Page Section` (Paritätsspalte des Exports).
- **Akzeptanzkriterien:**
  - [ ] Default-Layout des Lower-Tabs zeigt genau `Error`, `Type`, `Detail`, `Suggestion`; `Page Section` und `Rule ID` erscheinen erst nach Einblenden.
  - [ ] `de-errors.html` zeigt 3 Zeilen; im Bulk-Export tragen sie die Page Sections „Page Title“, „Meta Description“, „Body“.
  - [ ] „Ignorieren (alle)“ auf „Rechtschreibfeler“ entfernt das Wort in allen Seiten dieses Crawls nach Neu-Prüfung; ein neuer Crawl meldet es wieder.
  - [ ] „Zum Wörterbuch hinzufügen“ wirkt auch im nächsten Crawl.
  - [ ] Export filterbar nach Page Section.
- **Abgrenzung:** Dieser Eintrag ist führend für den **Inhalt** des Detail-Tabs (Spalten, Kontextaktionen, Wirkung); Layout und Interaktion (Markierung im Textausschnitt, Platzierung) regelt UI-047 (P1). Spalten und Kontextaktionen am 2026-10-13 gegen das öffentliche Rechtschreib-/Grammatik-Tutorial der Referenz geprüft: Fehlertext, Typ, Detail, Vorschlag; Aktionen „Regel ignorieren“, „Ignorieren (alle)“, „Zum Wörterbuch hinzufügen“; eine Spalte `Count` und eine Aktion „nur auf dieser Seite ignorieren“ sind dort nicht beschrieben.
- **Testfixture:** `onpage-spelling/de-errors.html`.

### ONPAGE-158 Neu-Prüfung ohne Recrawl und Top-100-Fehlerübersicht
- **Priorität:** P2
- **Beschreibung:** Nach Änderungen an Sprache, Wörterbuch, Ignorierliste oder Regeln kann die Prüfung auf den gespeicherten Texten erneut ausgeführt werden (Fortschrittsanzeige, abbrechbar). Eine Übersicht (rechte Seitenleiste bzw. Report „Spelling & Grammar Errors Summary“) zeigt die 100 häufigsten Fehler mit Anzahl betroffener URLs.
- **Eingaben/Konfiguration:** keine.
- **Ausgabe:** Report mit Spalten `Error`, `Type`, `URLs`, `Instances`.
- **Akzeptanzkriterien:**
  - [ ] Neu-Prüfung erzeugt keine HTTP-Anfragen.
  - [ ] Bei 150 verschiedenen Fehlern zeigt die Übersicht genau 100, sortiert nach URL-Anzahl absteigend.
- **Testfixture:** `onpage-spelling/many-errors/` (generierte Seiten mit 150 verschiedenen Fehlerwörtern).

### ONPAGE-159 Embeddings: Anbieter und Konfiguration
- **Priorität:** P1
- **Verweis:** siehe INTEG-180 (führend). Eingabetext ist standardmäßig der Inhaltsbereich nach ONPAGE-140.

### ONPAGE-160 Content-Filter Semantically Similar
- **Priorität:** P1
- **Verweis:** siehe INTEG-181 (führend). Der Filter erscheint im Content-Tab (Spalten siehe ONPAGE-142).

### ONPAGE-161 Content-Filter Low Relevance Content
- **Priorität:** P1
- **Verweis:** siehe INTEG-182 (führend). Der Filter erscheint im Content-Tab (Spalte `Semantic Relevance Score`, ONPAGE-142).

### ONPAGE-162 Embedding-Regeln (URL-Muster)
- **Priorität:** P1
- **Verweis:** siehe INTEG-184 (führend). Dort sind sowohl Quell-/Zielmuster-Paare als auch Einschluss-/Ausschlussregeln spezifiziert.

### ONPAGE-163 Lower-Tab Duplicate Details: Semantic Similarity
- **Priorität:** P1
- **Verweis:** siehe INTEG-183 (führend). Der Filter „Semantic“ ergänzt die Filter „Exact“ und „Near“ des Lower-Tabs Duplicate Details (ONPAGE-145).

### ONPAGE-164 Embeddings nur für indexierbare Seiten
- **Priorität:** P1
- **Verweis:** siehe INTEG-180 (führend). Die Einschränkung der Vergleichsmenge auf indexierbare Seiten (Annahme A-55) wird dort entschieden.

### ONPAGE-165 Bulk-Exporte Content
- **Priorität:** P1
- **Beschreibung:** Bulk-Exporte (Format, Dateinamen und Mechanik führend in OUT-052 bzw. OUT-066; Spaltennamen bestimmt der führende Fach-Eintrag des jeweiligen Inhalts, OUT-052 übernimmt sie exakt; hier die Zeilenauswahl aus den Filtern dieses Tabs): „Exact Duplicates“ (Spalten `Address`, `Exact Duplicate Address`, `Hash`; dieser Eintrag ist für die Spaltennamen führend, OUT-052 übernimmt sie, OS-Festlegung analog `Near Duplicate Address` aus ONPAGE-145), „Near Duplicates“ (ONPAGE-145), „Semantically Similar“ (INTEG-181/183), „Spelling & Grammar Errors“ (ONPAGE-157), „Spelling & Grammar Errors Summary“ (ONPAGE-158), „Low Relevance Content“, „Soft 404 Inlinks“, „All Page Text“ (`Address`, Inhaltsbereich-Text).
  - **„Content > Soft 404 Inlinks“:** alle Link-Instanzen (Quelle → Ziel), deren Ziel im Filter Soft 404 Pages (ONPAGE-147) steht; Spalten wie alle Inlink-Exporte nach OUT-031.
- **Eingaben/Konfiguration:** keine.
- **Ausgabe:** CSV/XLSX.
- **Akzeptanzkriterien:**
  - [ ] Jeder Export hat für die Fixture die erwartete Zeilenzahl (Golden-CSV).
  - [ ] Near-Duplicate-Export enthält nur Paare ≥ Schwellwert.
  - [ ] Soft 404 Inlinks: `onpage-content/index.html` verlinkt `soft404-h1.html` zweimal und `soft404-title.html` einmal, `other.html` verlinkt `soft404-h1.html` einmal → Export enthält genau 4 Zeilen; Links auf `/real-404` (echter 404) erscheinen nicht.
- **Testfixture:** `onpage-content-dupes/`, `onpage-spelling/`, `onpage-content/` (Soft-404-Seiten und Verlinkung wie im Kriterium).

### ONPAGE-166 PDF-Dokumente: Titel, Keywords, Wortzahl, Lesbarkeit, Rechtschreibung
- **Priorität:** P1
- **Beschreibung:** Seit einer früheren Referenzversion werden interne PDFs (Klasse PDF nach ONPAGE-020, Status 2xx) geparst. Zwei Stufen:
  1. **Immer** (sofern PDFs gecrawlt werden): Aus den Dokumenteigenschaften werden Titel und Keywords gelesen und im Internal-Tab in `Title 1` / `Title 1 Length` / `Title 1 Pixel Width` bzw. `Meta Keywords 1` / `Meta Keywords 1 Length` angezeigt (Normalisierung N(x) wie bei HTML). Fehlt der Dokumenttitel, bleibt `Title 1` leer (kein Ersatz durch Dateinamen oder erste Textzeile). Links im PDF werden extrahiert (Link-Spec).
  2. **Mit `pdf.store = true`** (Referenz: Option „Store PDF“ unter Spider-Konfiguration → Extraction): Der Textlayer des PDFs wird extrahiert und gespeichert. Daraus werden `Word Count` (Tokenisierung ONPAGE-141), Lesbarkeit (ONPAGE-149, Satzgrenzen zusätzlich an Absatzwechseln) und – bei aktivierter Prüfung – Rechtschreibung/Grammatik (ONPAGE-153/154, Bereiche „Page Title“ = Dokumenttitel und „Body“ = Dokumenttext) berechnet; die PDF erscheint dann als Zeile im Content-Tab (ONPAGE-009) und in den Filtern Readability Difficult/Very Difficult, Spelling Errors, Grammar Errors. Nicht für PDFs berechnet: Exact/Near Duplicates, Embeddings, Low Content, Soft 404, Lorem Ipsum (**Annahme A-53**).
  - PDFs ohne Textlayer (gescannte Bilder) haben `Word Count` = 0 und leere Lesbarkeitsspalten; keine OCR.
  - Verschlüsselte PDFs ohne Leserechte: Eigenschaften leer, `Word Count` leer, kein Abbruch des Crawls.
  - Weitere PDF-Eigenschaften (Subject, Author, Erstellungs-/Änderungsdatum, Seitenzahl) über die Option „Extract PDF Properties“ gehören zur Extraktions-Spec und werden hier nur referenziert.
- **Eingaben/Konfiguration:** `pdf.store` (Default false), Speicher-Schalter „Readability“, `spelling.enabled`, `grammar.enabled`.
- **Ausgabe:** Spalten im Internal-Tab (Title 1, Meta Keywords 1, Word Count) und Zeilen im Content-Tab.
- **Akzeptanzkriterien:**
  - [ ] `doc.pdf` (Titel „Preisliste 2026“, Keywords „preise, liste“) → Internal: `Title 1` = „Preisliste 2026“, `Title 1 Length` = 15, `Meta Keywords 1` = „preise, liste“; unabhängig von `pdf.store`.
  - [ ] `untitled.pdf` (ohne Titel-Eigenschaft, erste Textzeile „Einleitung“) → `Title 1` leer.
  - [ ] `pdf.store = false` → `doc.pdf` hat leeren `Word Count` und erscheint nicht im Content-Tab.
  - [ ] `pdf.store = true`, `text-120.pdf` mit genau 120 Wörtern im Textlayer → Word Count 120; Content-Tab-Zeile mit Flesch-Score (Wert im Unit-Test aus demselben Text vorberechnet).
  - [ ] `pdf.store = true`, Spelling an, `fehler.pdf` mit „Rechtschreibfeler“ → `Spelling Errors` = 1, Seite im Filter Spelling Errors.
  - [ ] `scan.pdf` (nur Bild, kein Textlayer) → Word Count 0, Readability leer, kein Fehler im Crawl-Log außer Hinweis „kein Textlayer“.
  - [ ] `encrypted.pdf` (Leseschutz) → Crawl läuft weiter; Eigenschaften leer.
  - [ ] Externe PDFs werden nicht geparst (keine Title-/Word-Count-Werte im External-Tab).
  - [ ] Ob PDFs in den Tabs Page Titles / Meta Keywords erscheinen, wird gemäß V-2 entschieden (A-5b, ONPAGE-009).
- **Testfixture:** `onpage-pdf/` mit `index.html` (verlinkt alle PDFs), `doc.pdf`, `untitled.pdf`, `text-120.pdf`, `fehler.pdf`, `scan.pdf`, `encrypted.pdf`; PDFs werden vom Fixture-Generator (Rust, `printpdf` o. ä. mit freier Lizenz) deterministisch erzeugt und eingecheckt. Externe PDF über F-HOST2.

---

## 13 Images

Grundmenge („All“): alle internen **und** externen Bild-URLs (Inhaltsklasse Images nach ONPAGE-020 oder per Bild-Referenz entdeckt), sofern „Check Images“ aktiv ist. Eine Bild-URL ist eine Zeile; Prüfungen auf Instanzebene (`<img>`-Vorkommen auf Seiten) werden je Bild-URL aggregiert: Ein Bild steht in einem Filter, wenn **mindestens eine** Instanz die Bedingung erfüllt.

### ONPAGE-170 Bildquellen-Erkennung
- **Priorität:** P0
- **Beschreibung:** Bild-URLs werden entdeckt aus:
  1. `<img src>` (P0),
  2. `<a href>`, wenn das Ziel ein Bild ist (Endung `.jpg/.jpeg/.png/.gif/.webp/.avif/.svg/.bmp/.ico` oder Content-Type `image/*`) (P0),
  3. `<img srcset>` und `<picture><source srcset>` – als eigene Bild-URLs im Images-Tab (und Internal/External → Images) nur mit Option „Extract images from srcset“ (Default aus, **Annahme A-42**) (P1). **Unabhängig von dieser Option** (Referenzverhalten) werden srcset-Kandidaten als Instanzen im Lower-Tab Image Details der einbindenden Seite angezeigt (Quelle `srcset`); bei abgeschalteter Option werden sie dafür nicht angefragt (Size/Dimensionen leer),
  4. CSS-Hintergrundbilder und dynamisch geladene Bilder – nur im Rendering-Modus, über die beim Rendern geladenen Ressourcen (P1, siehe ONPAGE-178),
  5. `<link rel="icon">`, `<meta property="og:image">` – nicht als Bild-Instanz (eigene Bereiche).
  Lazy-Load-Attribute (`data-src`) werden ohne Rendering nicht ausgewertet.
- **Eingaben/Konfiguration:** „Check Images“, „Extract images from srcset“, Rendering-Modus.
- **Ausgabe:** Bild-URLs in Images-Tab und Internal/External → Images; Instanzen (Seite, Bild, Alt-Attribut vorhanden?, Alt-Wert, width/height-Attribute, Quelle `img|a|srcset|css`).
- **Akzeptanzkriterien:**
  - [ ] `<img src="a.png">` → `a.png` im Tab; Instanz mit Quelle `img`.
  - [ ] `<a href="big.jpg">` → `big.jpg` im Tab; Instanz ohne Alt-Information (zählt nicht für Alt-Filter).
  - [ ] `<img srcset="s.png 1x, l.png 2x">` → `s.png`/`l.png` im Images-Tab nur mit aktivierter Option.
  - [ ] Option aus: Lower-Tab Image Details der Seite `sources.html` listet trotzdem Instanzen für `s.png` und `l.png` (Quelle `srcset`); der Fixture-Server erhält keine Anfrage auf `s.png`/`l.png`.
  - [ ] `<picture><source srcset="p.webp" type="image/webp"><img src="p.jpg" alt="P"></picture>` → `p.jpg` immer im Tab; `p.webp` im Tab nur mit Option, im Lower-Tab immer.
  - [ ] `<img data-src="lazy.png">` ohne Rendering → nicht entdeckt; mit Rendering und Lazy-Load-Script → entdeckt.
  - [ ] `<img src="data:image/png;base64,…">` → nicht als URL gelistet (**Annahme A-43**).
- **Testfixture:** `onpage-images/sources.html` mit allen Varianten, Bilddateien in `onpage-images/img/`.

### ONPAGE-171 Images: Spalten
- **Priorität:** P0
- **Beschreibung:** Spalten des Images-Tabs.
- **Eingaben/Konfiguration:** keine.
- **Ausgabe (Spalten):** `Address`, `Content`, `Size` (UI in KB mit einer Nachkommastelle, 1 KB = 1024 Byte; Export in Bytes), `IMG Inlinks` (Anzahl verschiedener Seiten mit Instanz; OpenSpider-Erweiterung), `Indexability`, `Indexability Status`.
- **Akzeptanzkriterien:**
  - [ ] Bild mit 102 400 Byte → UI „100,0 KB“, Export `102400`.
  - [ ] Bild auf 3 Seiten eingebunden (davon 2× auf einer Seite) → IMG Inlinks 3.
- **Testfixture:** `onpage-images/`.

### ONPAGE-172 Lower-Tab Image Details
- **Priorität:** P1
- **Beschreibung:** Für eine gewählte Bild-URL (Images-Tab) oder Seite (andere Tabs): Liste der Instanzen, plus Bildvorschau. Paritätsspalten in dieser Reihenfolge:
  - `From` (Seite), `To` (Bild-URL), `Alt Text`,
  - `Real Dimensions (WxH)`: tatsächliche Pixelmaße der Bilddatei. **Referenzverhalten:** nur im JavaScript-Rendering-Modus und nur für Bilder, die im Render-Viewport tatsächlich gerendert wurden; sonst leer.
  - `Dimensions in Attributes (WxH)`: aus `width`/`height`-Attributen; leer, wenn eines fehlt,
  - `Display Dimensions (WxH)`: gerenderte CSS-Größe (nur Rendering, nur im Viewport gerenderte Bilder),
  - `Potential Savings` (Bytes, ONPAGE-180),
  - `Path Type`: Schreibweise der Bildreferenz im Attribut – `Absolute` (`https://h/a.png`), `Protocol-Relative` (`//h/a.png`), `Root-Relative` (`/a.png`), `Path-Relative` (`a.png`, `../a.png`),
  - `Link Path`: XPath des referenzierenden Elements im Dokument (z. B. `/html[1]/body[1]/main[1]/div[2]/img[1]`; OS-Festlegung: jeder Schritt trägt einen Positionsindex `[n]`),
  - `Link Position`: Seitenbereich der Referenz gemäß Link-Positions-Konfiguration (Link-Spec; laut Referenz-Doku u. a. `Head`, `Nav`, `Footer` und als Auffangbereich `Content`; vollständige Default-Liste und Benutzerdefinition in der Link-Spec),
  - `Link Origin`: `HTML` (nur im Roh-HTML), `Rendered HTML` (nur im gerenderten DOM), `HTML & Rendered HTML` (in beiden), `Dynamically Loaded` (ohne Markup-Referenz, zur Laufzeit per Script geladen).
  - **OpenSpider-Erweiterungen** (standardmäßig ausgeblendet): `Alt Text Length` (Codepoints nach N(x)); `Source` (`img` | `a` | `srcset` | `css`); optional `images.real_dimensions_from_header` (Default false): liest Real Dimensions ohne Rendering aus dem Dateikopf (PNG/JPEG/GIF/WebP/AVIF) – in Parität aus.
- **Eingaben/Konfiguration:** Rendering-Modus (für Real/Display Dimensions), Link-Positions-Konfiguration, `images.real_dimensions_from_header`.
- **Ausgabe:** Lower-Tab; dieselben Spalten in den Bulk-Exporten ONPAGE-181.
- **Akzeptanzkriterien:**
  - [ ] Default-Layout zeigt genau die Paritätsspalten in der genannten Reihenfolge; `Alt Text Length` und `Source` sind ausgeblendet.
  - [ ] Mit Rendering: PNG 800×600 mit `width="400" height="300"` im sichtbaren Bereich → Real „800x600“, Attributes „400x300“, Display „400x300“.
  - [ ] Ohne Rendering (Parität): dasselbe Bild → Attributes „400x300“, Real **leer**, Display leer.
  - [ ] Ohne Rendering mit `images.real_dimensions_from_header = true` → Real „800x600“, Display leer.
  - [ ] Mit Rendering, Bild 3 000 px unterhalb des Viewports ohne Lazy-Load-Auslösung → Real und Display leer (nicht im Viewport gerendert).
  - [ ] `width` ohne `height` → Attributes leer.
  - [ ] Real Dimensions (mit Rendering) für PNG, JPEG, GIF, WebP, AVIF; SVG → leer (**Annahme A-44**).
  - [ ] Path Type: `src="https://…/a.png"` → `Absolute`; `src="//…/a.png"` → `Protocol-Relative`; `src="/img/a.png"` → `Root-Relative`; `src="img/a.png"` → `Path-Relative`.
  - [ ] Link Path für `<body><main><div></div><div><img></div></main>` → `/html[1]/body[1]/main[1]/div[2]/img[1]`.
  - [ ] Link Position: Bild in `<footer>` → `Footer`; Bild in `<nav>` → `Nav`; Bild in `<main>` → `Content`.
  - [ ] Link Origin: Bild im Roh-HTML ohne Rendering → `HTML`; mit Rendering und unverändertem DOM → `HTML & Rendered HTML`; per `appendChild` eingefügtes `<img>` → `Rendered HTML`; per `new Image().src=…` ohne DOM-Einfügung geladenes Bild → `Dynamically Loaded`.
- **Testfixture:** `onpage-images/dimensions.html` (inkl. Bild weit unterhalb des Viewports), `img/800x600.png`, `.jpg`, `.gif`, `.webp`, `.avif`, `.svg`; `onpage-images/paths.html` (vier Path-Type-Varianten, Bilder in nav/footer/main, verschachtelte divs); `onpage-images/origin.html` (Inline-Script fügt ein `<img>` ein und lädt ein Bild per `new Image()`).

### ONPAGE-173 Bildgröße bestimmen
- **Priorität:** P0
- **Beschreibung:** `Size` eines Bildes = Anzahl Bytes des (dekomprimierten) Antwortkörpers, wenn das Bild heruntergeladen wurde; sonst `Content-Length`; fehlt beides, 0. Bilder werden vollständig geladen, sofern „Check Images“ aktiv ist (Download auf Header-Bytes zu kürzen ist eine zulässige Optimierung, dann muss Size aus `Content-Length` stammen). Real Dimensions stammen in Parität aus dem Rendering (ONPAGE-172), nicht aus diesem Download.
- **Eingaben/Konfiguration:** keine.
- **Ausgabe:** Spalte `Size`.
- **Akzeptanzkriterien:**
  - [ ] Bild mit korrektem Content-Length → Size = Länge.
  - [ ] Bild chunked ohne Content-Length → Size = tatsächliche Bytes (nicht 0).
  - [ ] Bild-URL mit Status 404 → Size 0, erscheint in keinem Größenfilter, auch wenn die Fehlerseite groß ist (**Annahme A-45**).
- **Testfixture:** `onpage-images/` Routen `/img/chunked.png`, `/img/missing.png` (404).

### ONPAGE-174 Image-Filter Over 100 KB
- **Priorität:** P0
- **Beschreibung:** Bilder mit `Size` > `image.max_kb` × 1024 Byte (Default 100 KB = 102 400 Byte).
- **Eingaben/Konfiguration:** `image.max_kb`.
- **Ausgabe:** Filter `Over 100 KB` (Name dynamisch).
- **Akzeptanzkriterien:**
  - [ ] 102 400 Byte → nicht; 102 401 Byte → im Filter.
  - [ ] Externes Bild mit 200 KB → im Filter (intern und extern).
  - [ ] Nach `image.max_kb = 200` → 200-KB-Bild (204 800 Byte) nicht mehr im Filter.
- **Testfixture:** `onpage-images/img/size-102400.png`, `size-102401.png` (Padding-Chunk in PNG), externes Bild über F-HOST2.

### ONPAGE-175 Image-Filter Missing Alt Text
- **Priorität:** P0
- **Beschreibung:** Bilder mit mindestens einer `<img>`-Instanz, deren `alt`-Attribut vorhanden, aber nach N(x) leer ist. Hinweis: Für dekorative Bilder ist `alt=""` korrekt; der Filter dient der Prüfung.
- **Eingaben/Konfiguration:** keine.
- **Ausgabe:** Filter `Missing Alt Text`.
- **Akzeptanzkriterien:**
  - [ ] `<img src=a.png alt="">` → im Filter; `alt="   "` → im Filter.
  - [ ] `<img src=a.png>` (kein Attribut) → nicht hier (sondern ONPAGE-176).
  - [ ] Gleiches Bild auf Seite 1 mit `alt="Logo"` und Seite 2 mit `alt=""` → im Filter.
  - [ ] Bild nur über `<a href>` verlinkt → nicht.
  - [ ] `role="presentation"` oder `aria-hidden="true"` befreit nicht (**Annahme A-46**).
- **Testfixture:** `onpage-images/alt-*.html`.

### ONPAGE-176 Image-Filter Missing Alt Attribute
- **Priorität:** P0
- **Beschreibung:** Bilder mit mindestens einer `<img>`-Instanz ohne `alt`-Attribut.
- **Eingaben/Konfiguration:** keine.
- **Ausgabe:** Filter `Missing Alt Attribute`.
- **Akzeptanzkriterien:**
  - [ ] `<img src=b.png>` → im Filter.
  - [ ] `<img src=b.png alt>` (Attribut ohne Wert) → nicht hier, sondern in Missing Alt Text.
  - [ ] Ein Bild kann gleichzeitig in Missing Alt Text und Missing Alt Attribute stehen (verschiedene Instanzen).
- **Testfixture:** `onpage-images/alt-*.html`.

### ONPAGE-177 Image-Filter Alt Text Over 100 Characters
- **Priorität:** P0
- **Beschreibung:** Bilder mit mindestens einer Instanz, deren Alt-Text-Länge (N(x), Codepoints) > `image.alt_max_chars` (Default 100).
- **Eingaben/Konfiguration:** `image.alt_max_chars`.
- **Ausgabe:** Filter `Alt Text Over 100 Characters`.
- **Akzeptanzkriterien:**
  - [ ] 100 Zeichen → nicht; 101 → im Filter.
  - [ ] Alt-Text mit Entities (`&amp;`) wird dekodiert gemessen.
- **Testfixture:** `onpage-images/alt-100.html`, `alt-101.html`.

### ONPAGE-178 Image-Filter Background Images
- **Priorität:** P1
- **Beschreibung:** Nur Rendering-Modus + Crawl-Analyse: Bilder, die beim Rendern geladen wurden und auf keiner Seite als `<img>`/`<picture>`/`srcset`-Instanz vorkommen, sondern über CSS (`background-image`, `background`, `content: url()`, Inline-Style oder Stylesheet; ermittelt über berechnete Styles der Elemente) oder dynamisch per Script geladen werden. Hinweis: Hintergrundbilder werden von Suchmaschinen meist nicht indexiert und haben keinen Alt-Text.
- **Eingaben/Konfiguration:** Rendering = JavaScript, Crawl-Analyse. Optionale Erweiterung ohne Rendering: `images.parse_inline_css_backgrounds` (Default aus, Parität) wertet `style`-Attribute und `<style>`-Blöcke aus.
- **Ausgabe:** Filter `Background Images`; Lower-Tab Inlinks zeigt die einbindenden Seiten.
- **Akzeptanzkriterien:**
  - [ ] `<div style="background-image:url(/img/bg.jpg)">` mit Rendering → `bg.jpg` im Filter nach Crawl-Analyse.
  - [ ] `.hero{background:url(/img/hero.jpg)}` in externem Stylesheet → im Filter.
  - [ ] Bild, das zusätzlich irgendwo als `<img>` vorkommt → nicht im Filter.
  - [ ] Ohne Rendering → `needs_rendering`; mit Inline-CSS-Option → `bg.jpg` auch ohne Rendering.
- **Testfixture:** `onpage-images/background.html`, `css/hero.css`.

### ONPAGE-179 Image-Filter Missing Size Attributes
- **Priorität:** P1
- **Beschreibung:** Bilder mit mindestens einer `<img>`-Instanz, bei der `width` oder `height` als HTML-Attribut fehlt (eines genügt) (**Annahme A-47**). CSS-Größen (`style="width:…"`) zählen nicht als Attribut. Hintergrund: fehlende Abmessungen können Layout-Verschiebungen (CLS) verursachen.
- **Eingaben/Konfiguration:** keine.
- **Ausgabe:** Filter `Missing Size Attributes`.
- **Akzeptanzkriterien:**
  - [ ] `<img src=a.png>` → im Filter.
  - [ ] `<img src=a.png width=400>` → im Filter (A-47).
  - [ ] `<img src=a.png width=400 height=300>` → nicht.
  - [ ] `<img src=a.png style="width:400px;height:300px">` → im Filter.
  - [ ] `<img width="" height="">` (leere Werte) → im Filter.
- **Testfixture:** `onpage-images/size-attrs-*.html`.

### ONPAGE-180 Image-Filter Incorrectly Sized Images
- **Priorität:** P1
- **Beschreibung:** Nur Rendering-Modus + Crawl-Analyse: Bilder mit mindestens einer Instanz, deren reale Pixelmaße größer sind als die gerenderte Anzeigegröße (Viewport des Render-Profils, Device-Pixel-Ratio 1) und deren geschätzte Einsparung ≥ `images.incorrect_size_min_saving_kb` (Default 4 KB = 4096 Byte) ist. Schätzung (**Annahme A-48**): `Potential Savings = Size × (1 − (dispW × dispH) / (realW × realH))`, nur wenn realW > dispW oder realH > dispH; SVG ausgenommen; Instanzen mit Anzeigegröße 0 (unsichtbar) ausgenommen.
- **Eingaben/Konfiguration:** Rendering, `images.incorrect_size_min_saving_kb`.
- **Ausgabe:** Filter `Incorrectly Sized Images`; Spalten in Image Details (ONPAGE-172).
- **Akzeptanzkriterien:**
  - [ ] JPEG 2000×1000 (200 KB) angezeigt in 400×200 → Savings ≈ 196 KB → im Filter.
  - [ ] PNG 410×300 (20 KB) angezeigt in 400×300 → Savings ≈ 0,5 KB → nicht.
  - [ ] Bild kleiner als Anzeige (hochskaliert) → nicht.
  - [ ] `display:none`-Bild → nicht.
  - [ ] Ohne Rendering → `needs_rendering`.
- **Testfixture:** `onpage-images/sizing.html` mit `img/2000x1000.jpg` (CSS-Breite 400px), `img/410x300.png` (CSS 400×300), `img/100x100.png` (CSS 200×200), verstecktes Bild.

### ONPAGE-181 Bulk-Exporte Images
- **Priorität:** P1
- **Beschreibung:** Bulk-Exporte je Filter mit Instanzebene (Format, Exportnamen und Mechanik führend in OUT-053; hier nur Zeilenauswahl und Instanzspalten aus ONPAGE-172): „Images Missing Alt Text“, „Images Missing Alt Attribute“, „Images Over X KB“, „Alt Text Over X Characters“, „Missing Size Attributes“, „Incorrectly Sized Images“, „Background Images“, „All Image Inlinks“. Spalten: `From`, `To`, `Alt Text`, `Size`, `Real Dimensions (WxH)`, `Dimensions in Attributes (WxH)`, `Display Dimensions (WxH)`, `Potential Savings`, `Path Type`, `Link Path`, `Link Position`, `Link Origin` (Werte wie ONPAGE-172).
- **Eingaben/Konfiguration:** keine.
- **Ausgabe:** CSV/XLSX.
- **Akzeptanzkriterien:**
  - [ ] „Images Missing Alt Text“ listet nur die betroffenen Instanzen (Bild mit 1 guter und 1 leerer Instanz → 1 Zeile).
  - [ ] Zeilenzahlen entsprechen den Golden-CSVs.
- **Testfixture:** `onpage-images/`.

### ONPAGE-182 Image-Filter Missing Title Attribute (Nachtrag 09.11.2026)
- **Priorität:** P2
- **Meilenstein:** 1.x (nach 1.0) – OS-Festlegung, Nutzerentscheidung 09.11.2026, ohne Paritätsbezug.
- **Beschreibung:** Bilder mit mindestens einer `<img>`-Instanz ohne `title`-Attribut oder mit leerem `title` (nach N(x)). Der Filter ist ein **Hinweis**: Das `title`-Attribut ist weder für Suchmaschinen noch für die Barrierefreiheit erforderlich (dafür ist `alt` maßgeblich, ONPAGE-175/176) und wird von Hilfstechnologien uneinheitlich vorgelesen. Er dient Teams, deren Redaktionsrichtlinien ein Bild-`title` vorschreiben (z. B. Tooltip mit Bildquelle).
- **Eingaben/Konfiguration:** keine eigene; Einblendung im Issues-Tab über UI-151.
- **Ausgabe:** Filter `Missing Title Attribute` im Images-Tab; Spalte `Title Attribute` in Image Details (ONPAGE-172, Wert der Instanz, leer wenn fehlend); Bulk-Export „Images Missing Title Attribute“ mit den Instanzspalten aus ONPAGE-181. Katalogeintrag (UI-063): Typ `Opportunity`, Priorität `Low` (`T-O2`, `P-L1`) mit `default_enabled = false` (UI-151): Er erscheint im Issues-Tab erst, wenn der Nutzer ihn einschaltet; der Filter im Images-Tab ist immer verfügbar.
- **Akzeptanzkriterien:**
  - [ ] `<img src=a.png alt="A">` → im Filter; `<img src=a.png alt="A" title="Quelle: X">` → nicht.
  - [ ] `title=""` und `title="   "` → im Filter.
  - [ ] Gleiches Bild auf Seite 1 mit `title` und auf Seite 2 ohne → im Filter (Aggregationsregel des Abschnitts); der Bulk-Export listet nur die Instanz auf Seite 2.
  - [ ] Bild nur über `<a href>` verlinkt oder nur aus `srcset`/CSS bekannt → nicht im Filter (keine `<img>`-Instanz). `<picture>`: maßgeblich ist das `<img>`-Element.
  - [ ] Ein `title` am umschließenden Link (`<a title="…"><img></a>`) zählt nicht als Bild-`title`.
  - [ ] Im Standard erscheint der Befund nicht im Issues-Tab und nicht im Issues-Export; nach Einschalten über UI-151 erscheint er dort als `Opportunity`/`Low`.
- **Testfixture:** `onpage-images/title-*.html` (mit, ohne, leer, nur am Link, gemischt über zwei Seiten).

---

## 14 Querschnitt

### ONPAGE-190 Übersichtszähler je Tab/Filter
- **Priorität:** P0
- **Beschreibung:** Eine Übersichtsleiste (rechte Seitenleiste) zeigt je Tab dieses Dokuments alle Filter mit Trefferzahl und Prozent; Klick aktiviert Tab und Filter.
- **Eingaben/Konfiguration:** keine.
- **Ausgabe:** Baumstruktur Tab → Filter → (Anzahl, %).
- **Akzeptanzkriterien:**
  - [ ] Zahlen in der Übersicht = Zeilenzahlen der Filteransicht (Test über alle Fixtures).
  - [ ] Filter mit Abhängigkeit zeigen den Zustand (z. B. „Crawl-Analyse erforderlich“).
- **Testfixture:** alle `onpage-*`-Szenarien.

### ONPAGE-191 Export der aktuellen Filteransicht
- **Priorität:** P0
- **Verweis:** siehe OUT-020 (führend). Der Golden-Vergleich `tests/golden/onpage-titles/page_titles__missing.csv` (inkl. `quote.html`) ist dort als Kriterium geführt.

### ONPAGE-192 Verknüpfung mit Lower-Tabs
- **Priorität:** P0
- **Beschreibung:** Für jede Zeile eines Tabs dieses Dokuments sind die Lower-Tabs `URL Details`, `Inlinks`, `Outlinks`, `Image Details`, `Duplicate Details`, `Spelling & Grammar Details`, `SERP Snippet` (ONPAGE-194), `Rendered Page`, `View Source`, `HTTP Headers` verfügbar (Inhalt der Detail-Tabs in der jeweiligen Fachspec, z. B. ONPAGE-145/157/172/194; Layout und Interaktion in 05, UI-031 ff.).
- **Eingaben/Konfiguration:** keine.
- **Ausgabe:** Lower-Tab-Inhalte für die Auswahl.
- **Akzeptanzkriterien:**
  - [ ] Auswahl einer 404-URL im Response-Codes-Tab zeigt im Inlinks-Tab alle verlinkenden Seiten mit Ankertext.
  - [ ] Auswahl einer Seite im Page-Titles-Tab zeigt im SERP-Snippet deren Title 1 und Meta Description 1 (Details ONPAGE-194).
- **Testfixture:** `onpage-rc/`, `onpage-titles/`.

### ONPAGE-193 Live-Aktualisierung während des Crawls
- **Priorität:** P0
- **Verweis:** siehe UI-018 (führend). Welche Filter dieses Dokuments live und welche erst nach der Crawl-Analyse aktualisiert werden, legt ONPAGE-007 fest.

### ONPAGE-194 Lower-Tab SERP Snippet: Geräteumschaltung und Bearbeitung
- **Priorität:** P1
- **Beschreibung:** Der Lower-Tab „SERP Snippet“ zeigt für die gewählte HTML-Seite eine Vorschau eines Suchergebnisses aus Address (als Anzeige-URL), `Title 1` und `Meta Description 1`.
  - **Geräteumschaltung:** Auswahl `Desktop` (Default), `Mobile`, `Tablet`. Je Gerät gelten eigene Abschneidegrenzen in Pixeln für Titel und Beschreibung und eigene Schriftgrößen für die Pixelberechnung (UI-038). Die Grenzen sind in den Preferences je Gerät einstellbar (`serp.desktop.*`, `serp.mobile.*`, `serp.tablet.*`). Defaults: Desktop = `title.max_px` / `meta_description.max_px` aus 0.3; Mobile/Tablet: **Annahme A-56** (vorläufig Titel 561 px wie Desktop, Beschreibung 680 px), Paritätstest V-7. Die Referenz-Doku ist hier uneinheitlich: Der Tab-Teil nennt einstellbare Mobile/Tablet-Beschreibungslängen in den Preferences, der Konfigurations-Teil sagt, dass die Preferences-Breiten die Snippet-Vorschau (noch) nicht beeinflussen. OpenSpider koppelt die Desktop-Vorschau an die Filter-Schwellwerte und hält Mobile/Tablet getrennt.
  - **Abschneiden:** Überschreitet ein Text die Grenze des gewählten Geräts, wird er am letzten ganzen Wort, dessen Ende einschließlich „ …“ noch in die Grenze passt, gekürzt und mit „ …“ angezeigt. Neben der Vorschau stehen je Feld Zeichenzahl und Pixelbreite sowie die verbleibenden bzw. überschüssigen Pixel.
  - **Bearbeitung:** Titel und Beschreibung sind in der Vorschau editierbar; Zeichen-/Pixelwerte und Kürzung aktualisieren sich bei jeder Eingabe. Änderungen werden je URL in der Crawl-Datei gemerkt (nur Vorschau, keine Änderung an der Website und an den Filter-Werten) und über „Zurücksetzen“ verworfen.
- **Eingaben/Konfiguration:** Gerät, `serp.<device>.title_max_px`, `serp.<device>.description_max_px`, `serp.<device>.title_font_px`, `serp.<device>.description_font_px`.
- **Ausgabe:** Vorschau, Kennzahlen je Feld, gespeicherte Bearbeitungen.
- **Akzeptanzkriterien:**
  - [ ] Desktop: Titel aus 70× „W“ (Breite 1 322 px bei 20 px) wird gekürzt; der angezeigte Text inklusive „ …“ ist ≤ 561 px; Titel „Start“ wird ungekürzt angezeigt.
  - [ ] Umschalten auf Mobile verwendet die Mobile-Grenzen: Eine Beschreibung mit 900 px (Desktop-Schrift) wird auf Desktop (985) ungekürzt, auf Mobile (680) gekürzt angezeigt.
  - [ ] Ändern von `serp.mobile.description_max_px` auf 900 → dieselbe Beschreibung ist auf Mobile ungekürzt; Desktop-Filter „Over 985 Pixels“ bleibt unverändert.
  - [ ] Ändern von `meta_description.max_px` ändert Filter **und** Desktop-Vorschau.
  - [ ] Bearbeiten des Titels in der Vorschau aktualisiert Zeichenzahl und Pixelbreite sofort; Spalten `Title 1` / `Title 1 Length` im Page-Titles-Tab bleiben unverändert; nach Schließen und erneutem Öffnen der Crawl-Datei ist die Bearbeitung noch sichtbar; „Zurücksetzen“ stellt `Title 1` wieder her.
  - [ ] Seite ohne Meta Description → Beschreibungsfeld leer mit Hinweis „keine Meta Description“ (keine automatisch generierte Beschreibung).
- **Testfixture:** `onpage-titles/pixel-W70.html`, `basic.html`; `onpage-meta/desc-900px.html` (Beschreibung, deren Breite im Unit-Test auf 900 ± 5 px vorberechnet ist).

---

## 15 Annahmen & Verifikation

Jede Annahme wird mit einer Paritätsfixture gegen die Referenz (Free-Version, ≤ 500 URLs, Export der jeweiligen Filter) geprüft. Ergebnis in `docs/spec/PARITY.md` festhalten; bei Abweichung Spec korrigieren (nicht Code an Spec vorbei ändern).

| Nr. | Annahme | Betrifft | Prüfung gegen Referenz |
|---|---|---|---|
| A-1 | Keine Case-/Unicode-Normalisierung bei Elementwerten | 003, 005 | Titel mit NFD-Umlaut vs. NFC → Duplicate? |
| A-2 | Länge in Codepoints | 003 | Titel mit 3 Emoji: Length-Spalte vergleichen |
| A-3 | Pixelbreite: 20 px Titel / 14 px Description, Arial-Metrik | 004 → UI-038 (führend; dort weiterzuführen) | 20 Golden-Titel, Abweichung ≤ 3 % |
| A-4 | ~~Duplikatvergleich case-sensitiv~~ **widerlegt (Paritätslauf M1, 14.10.2026):** Vergleich ohne Groß-/Kleinschreibung (K-58) | 005 | „Kontakt“ vs. „kontakt“ |
| A-4b | Duplikatvergleich mit Kollation `OS_TEXT` (ICU-Root, Secondary): NFC/NFD gleich, ß/ss und Sigma-Formen nach ICU (K-70) | 005 | „Straße“ vs. „STRASSE“, NFC- vs. NFD-„é“ (M2-S04) |
| A-5 | Element-Tabs nur interne HTML 2xx | 009 | 404-Seite mit Titel im Page-Titles-All? |
| A-5b | PDFs erscheinen nicht in den Tabs Page Titles / Meta Keywords (nur Title-/Keywords-Spalte im Internal-Tab) | 009, 166 | `doc.pdf` mit Titel crawlen, Page Titles → All prüfen (V-2) |
| A-6 | Size ohne Content-Length = 0 (außer Bilder) | 012, 173 | chunked PDF |
| A-7 | Text Ratio über gesamten Body | 013 | Seite mit großem Footer |
| A-8 | Folder Depth zählt Segmente vor `/` | 014 | `/a/b`, `/a/b/`, `/a/b.html` |
| A-9 | Selbstlinks zählen als In-/Outlink | 015 | Seite mit Link auf sich selbst |
| A-10 | Eigener Redirect Type für HTTP Refresh | 018, 059 | Refresh-Header-Route |
| A-11 | Externe URLs bei „Check external“ aus | 025 | Option aus, External-Tab prüfen |
| A-12 | Widerlegt (Q02, K2-85, SQ-03): Header-Filter 039–042 nur für interne URLs mit 2xx (jeder Inhaltstyp); vorher „alle internen URLs mit Antwort“ | 038–042 | erledigt |
| A-13 | Protocol-relative nur für Ressourcen, nicht `<a>` | 037 | `<a href="//…">` |
| A-14 | HSTS-Filter nur für HTTPS-Antworten | 038 | HTTP-Crawl, Filter leer? |
| A-15 | CSP Report-Only/Meta zählt nicht | 039 | Routen `/csp-ro`, `csp-meta.html` |
| A-16 | frame-ancestors ersetzt XFO nicht | 041 | CSP mit frame-ancestors, ohne XFO |
| A-17 | „Ignore robots but report“ erscheint in Blocked by Robots.txt | 052 | Modus umschalten |
| A-18 | Blocked Resource umfasst HTTP-Fehler beim Rendern | 053 | CSS mit 500 |
| A-19 | Redirect Chain listet nur Start-URL; revidiert (K-66): Schleifenmitglieder mit ≥ 2 Hops zählen auch als Redirect Chain | 060 | `/chain1`, `/chain2`; `parity-response` `/loop1`, `/loop2` (belegt) |
| A-19b | Vorlauf einer Schleife (`/tail` → `/loop1` …) zählt ab 2 Hops als Redirect Chain | 060 | Paritäts-Site mit Vorlauf vor einer Schleife (M2-S06) |
| A-20 | Redirect Loop listet alle Mitglieder | 061 | `/loop1`, `/loop2` |
| A-21 | HSTS-Weiterleitung als 307 mit Status „HSTS Policy“ | 064 | HTTPS-Fixture mit HSTS |
| A-22 | Prozentkodiertes UTF-8 zählt als Non-ASCII | 071 | `/%C3%BCber` |
| A-23 | ~~Underscores nur im Pfad~~ **widerlegt (Paritätslauf M1):** Pfad und Query (K-58) | 072 | `/?sort_by=x` |
| A-24 | Repetitive Path: beliebiges Segment, case-sensitiv | 075 | `/Shop/shop/`, `/a/x/a.html` |
| A-25 | Space: literal und `%20`, nicht `+` | 076 | drei Varianten |
| A-26 | Internal-Search-Heuristik (Segmente/Parameter) | 077 | Listen gegen Referenz-Treffer abgleichen |
| A-27 | Leere Query `?` zählt als Parameter | 078 | `/p?` |
| A-28 | `<a name>` gilt als gültiger Bookmark | 079 | `#anker-alt` |
| A-29 | Same as H1 case-sensitiv | 097 | „Kontakt“/„kontakt“ |
| A-30 | Alt-Text in H1 ist Teil des H1-Werts | 120 | `<h1><img alt="Logo"></h1>` → H1-1? |
| A-31 | `<br>` in Überschriften = Leerzeichen | 120 | `<h1>A<br>B</h1>` |
| A-32 | Over-X-Filter für H1/H2 prüfen nur das erste Vorkommen | 123, 133 | H1-2 mit 80 Zeichen |
| A-33 | Leere Überschriften zählen in der Reihenfolge | 126 | leeres `<h3>` vor H1 |
| A-34 | H2 Non-sequential: h3–h6 vor erstem H2; ohne H1 kein Treffer | 135 | 6 Sequenz-Fixtures |
| A-35 | Wortzählung zählt Satzzeichen-Token | 141 | „Preis: 10 € – jetzt!“ |
| A-35b | Je Listenelement `<li>` im Inhaltsbereich ein zusätzliches Token (Paritätsbefund M1, K-59, präzisiert K-68; vorher „je `<a href>`“, widerlegt) | 141 | 211 HTML-Seiten der Paritäts-Sites, 0 Abweichungen (M1-P31); `<li>` ohne Link offen (M2-S06) |
| A-36 | Near-Duplicates: Wort-3-Gramme, exakte Jaccard | 144 | Golden-Paare 95/91/89/80 %; Kalibrierung von n bei Abweichung > 2 Prozentpunkte |
| A-37 | Near-Duplicate-Export enthält beide Richtungen | 145 | Bulk-Export zählen |
| A-38 | Low Content unabhängig von Indexierbarkeit (solange `issues.ignore_non_indexable` aus) | 146 | `noindex`-Seite mit 50 Wörtern, Option an/aus |
| A-39 | Soft 404 prüft Titel, H1, Inhaltsbereich; eigene Default-Phrasen | 147 | Phrase nur im Titel / nur im Footer |
| A-40 | Lorem Ipsum über gesamten Body + Titel + Meta-Description | 148 | Lorem nur im Footer |
| A-41 | Satzenden auch an Blockgrenzen (vorläufig) | 149 | Kalibrierung an den Q02-Referenzwerten in M2-N08 (K2-87, SQ-08) |
| A-42 | srcset-Extraktion standardmäßig aus (Lower-Tab zeigt srcset-Instanzen trotzdem) | 170 | Default-Konfiguration der Referenz; Image Details einer srcset-Seite |
| A-43 | `data:`-Bilder werden nicht gelistet | 170 | Inline-Base64-Bild |
| A-44 | SVG ohne Real Dimensions | 172 | SVG mit viewBox |
| A-45 | Bilder mit Status ≠ 2xx: Size 0, in keinem Größenfilter | 173 | 404-Bild |
| A-46 | `role="presentation"` befreit nicht von Alt-Filtern | 175 | Bild mit role |
| A-47 | Missing Size Attributes: ein fehlendes Attribut genügt | 179 | nur `width` |
| A-48 | Savings-Formel flächenproportional | 180 | 3 Sizing-Fixtures, Potential-Savings-Spalte vergleichen |
| A-49 | Widerlegt (Q02, K2-87, SQ-07): die Referenz führt Media/Fonts/XML/Plugins als eigene Filter, Other ohne diese Klassen | 023, 026 | erledigt |
| A-50 | „Ignore Paginated URLs for Duplicate Filters“ standardmäßig an | 005 | Default-Konfiguration der Referenz; `pagination/`-Fixture ohne Konfigurationsänderung |
| A-51 | Keine Annahme mehr: „Ignore Non-Indexable URLs for Issues“ standardmäßig **an**, festgelegt in CRAWL-089 (führend) | 009, 146 | Nur noch Paritätsbeobachtung: Default-Konfiguration der Referenz; `noindex.html` ohne Titel in Missing? (Abweichung ändert den OS-Standard nicht) |
| A-52 | „Limit Page Content“ (Embeddings) standardmäßig aus | 159 → INTEG-180 (führend; dort weiterzuführen) | Default der erweiterten KI-Anbieter-Einstellungen |
| A-53 | PDFs im Content-Tab nur mit Wortzahl, Lesbarkeit, Sprache, Rechtschreibung/Grammatik; keine Duplikat-/Embedding-/Low-Content-/Soft-404-/Lorem-Auswertung | 142, 146, 166 | zwei identische PDFs, PDF mit 50 Wörtern |
| A-54 | Erledigt (V-5 durch Q02, K2-87, SQ-08): Labels der Referenz Very Easy … Very Hard | 149 | – |
| A-55 | Referenz schränkt Embedding-Analysen nicht auf indexierbare Seiten ein | 164 → INTEG-180 (führend; dort weiterzuführen) | `noindex`-Seite mit Embedding: erscheint sie in Semantically Similar? |
| A-56 | SERP-Snippet-Grenzen Mobile/Tablet (vorläufig Titel 561 px, Beschreibung 680 px) | 194 | Preferences/Snippet der Referenz für Mobile und Tablet ablesen (V-7) |

### Verifikationsaufträge

Bündeln mehrere Annahmen bzw. unbelegte Werte zu einem Prüflauf gegen die Referenz (Ergebnis in `docs/spec/PARITY.md`):

| Nr. | Gegenstand | Betrifft | Vorgehen |
|---|---|---|---|
| V-1 | Filterliste Internal/External (Flash vs. Plugins; Media/Fonts/XML) | 023, 026, A-49 | Erledigt durch Q02 (K2-87, SQ-07) |
| V-2 | PDF-Behandlung in Element-Tabs und Content-Tab | 009, 166, A-5b, A-53 | `onpage-pdf/` mit „Store PDF“ an/aus crawlen; Page Titles, Meta Keywords, Content exportieren |
| V-3 | Pixel-Defaults (561/200/985/400) und Pixel-Metrik (Schrift, Größe) | 0.3, UI-038, 095, 096, 105, 106, A-3 | Filternamen nach Neuinstallation ablesen; 20 Golden-Titel und 20 Golden-Beschreibungen exportieren, Pixel-Spalten vergleichen (≤ 3 % je Wert) |
| V-4 | Issue-Typ und -Priorität je Filter | UI-063 (führend), Erwartungswerte unten | Issues-Export der Referenz über alle Fixtures, Abgleich mit den Erwartungswerten unten und mit dem Ergebnis der UI-063-Logik; Abweichungen in `PARITY.md` protokollieren (kein Zwang zur Angleichung) |
| V-5 | Readability-Labels und Spalte Sentence Count | 142, 149, A-54 | Labels erledigt durch Q02 (K2-87, SQ-08); Spalte Sentence Count prüft M2-N08 |
| V-6 | Sprachliste der Rechtschreibprüfung | 153 | Sprachauswahlfeld der Referenz abschreiben (nur Namen/Codes), mit Zielmenge abgleichen |
| V-7 | SERP-Snippet je Gerät | 194, A-56 | Snippet-Vorschau einer Seite mit langem Titel/Beschreibung auf Desktop/Mobile/Tablet, Abschneidepunkte notieren |

**Bekannte Lücken der öffentlichen Doku** (bewusst offen gelassen, bei Paritätstests beobachten): exakte Default-Liste der Soft-404-Phrasen der Referenz, exakte Shingle-Parameter der Near-Duplicate-Erkennung, Silbenzähl-Algorithmus der Referenz, Schriftart/-größe für Pixelbreiten. Für diese gilt: OpenSpider-Ergebnis muss die Golden-Fixtures erfüllen; Abweichungen zur Referenz ≤ 5 % der Treffer je Filter auf einem realen Test-Crawl (500 URLs) sind akzeptiert und in `PARITY.md` zu dokumentieren.

### Paritäts-Erwartungswerte Issue-Klassifizierung (zu V-4)

Herkunft: früherer Inhalt von ONPAGE-006. Sachliche Einordnung, wie sie die öffentliche Issue-Bibliothek der Referenz (Referenzstand Oktober 2026) je Issue angibt; keine übernommenen Beschreibungstexte. **Keine Vorgabe für OpenSpider:** Maßgeblich ist die eigene Entscheidungslogik in UI-063. Die Tabelle dient ausschließlich als Vergleichsbasis im Paritätstest V-4.

| Tab | Filter | Typ | Priorität |
|---|---|---|---|
| Response Codes | Internal No Response / Client Error (4XX) / Server Error (5XX) / Redirect Loop | Issue | High |
| Response Codes | Internal Blocked by Robots.txt / Internal Blocked Resource | Warning | High |
| Response Codes | Internal Redirect Chain / External Blocked Resource | Warning | Medium |
| Response Codes | Internal Redirection (3XX/Meta Refresh/HTTP Refresh/JavaScript); External No Response / 4XX / 5XX | Warning | Low |
| Security | HTTP URLs, Mixed Content, Form URL Insecure, Form on HTTP URL | Issue | High |
| Security | Missing HSTS, Unsafe Cross-Origin Links, Protocol-Relative Resource Links, Missing CSP, Missing X-Content-Type-Options, Missing X-Frame-Options, Missing Secure Referrer-Policy, Bad Content Type | Warning | Low |
| URL | Multiple Slashes, Contains A Space, Broken Bookmark | Issue | Low |
| URL | Non ASCII Characters, Uppercase, Repetitive Path, Internal Search, Parameters, GA Tracking Parameters | Warning | Low |
| URL | Underscores, Over 115 Characters | Opportunity | Low |
| Page Titles | Missing, Multiple, Outside <head> | Issue | High |
| Page Titles | Duplicate, Over/Below X Characters, Over/Below X Pixels | Opportunity | Medium |
| Page Titles | Same as H1 | Opportunity | Low |
| Meta Description | Multiple, Outside <head> | Issue | Medium |
| Meta Description | Missing, Duplicate, Over/Below X Characters, Over/Below X Pixels | Opportunity | Low |
| Meta Keywords | (keine Issues; reine Filter) | – | – |
| H1 | Missing | Issue | Medium |
| H1 | Multiple | Warning | Medium |
| H1 | Alt Text in h1, Non-sequential | Warning | Low |
| H1 | Duplicate, Over X Characters | Opportunity | Low |
| H2 | Missing, Multiple, Non-sequential | Warning | Low |
| H2 | Duplicate, Over X Characters | Opportunity | Low |
| Content | Exact Duplicates | Issue | High |
| Content | Spelling Errors, Grammar Errors | Issue | Medium |
| Content | Soft 404 Pages, Lorem Ipsum Placeholder | Warning | High |
| Content | Near Duplicates, Semantically Similar | Warning | Medium |
| Content | Low Relevance Content | Warning | Low |
| Content | Low Content Pages | Opportunity | Medium |
| Content | Readability Difficult, Readability Very Difficult | Opportunity | Low |
| Images | Missing Alt Text, Missing Alt Attribute | Issue | Low |
| Images | Background Images | Warning | Low |
| Images | Over X KB | Opportunity | Medium |
| Images | Alt Text Over X Characters, Incorrectly Sized Images, Missing Size Attributes | Opportunity | Low |
