# 06 – Exporte, Reports & Betrieb (ID-Präfix `OUT`)

Clean-Room-Spezifikation. Grundlage: öffentliche Bedienungsanleitung, FAQ, Tutorials und Release-Notes mehrerer Hauptversionen des Referenzprodukts. Alle Beschreibungen sind eigene Formulierungen. Wo das Referenzprodukt Verhalten nicht öffentlich festlegt oder wir bewusst abweichen, ist das als **(OS-Festlegung)** markiert – diese Punkte sind verbindlich für OpenSpider, aber keine Paritätsbehauptung.

Zuständige Crates (siehe `docs/ARCHITECTURE.md`): `os-export` (Formate, Bulk-Exporte, Reports, Sitemaps), `os-analyze` (Ketten, Orphans, hreflang-Rücklinks, Vergleich), `os-cli` (Kommandozeile, Scheduling-Ziel), `os-integrations` (Google Drive/Sheets, E-Mail-OAuth), `apps/desktop` (Dialoge).

## Übersicht / Inhaltsverzeichnis

| Abschnitt | IDs | Inhalt |
|---|---|---|
| [0 Konventionen](#0-konventionen) | – | Spaltennamen, Fixtures, Golden-Dateien, Begriffe |
| [1 Export-Engine & Formate](#1-export-engine--formate) | OUT-001 – OUT-016 | CSV, XLSX, XLS, Google Sheets, Dateinamen, Limits, Streaming, Segment-Filter, leere Exporte |
| [2 Tab- und Detail-Exporte](#2-tab--und-detail-exporte) | OUT-020 – OUT-027 | Export aktueller Tab/Filter, untere Detail-Tabs (inkl. Lighthouse, Accessibility, Header, Cookies, Konsole, N-grams), rechte Panels (inkl. Spelling & Grammar, Semantic Search), Mehrfachauswahl, Crawl-Pfad |
| [3 Bulk-Exporte](#3-bulk-exporte) | OUT-030 – OUT-075 | Queued URLs, Links, Web, Path Type, filterbasierte Exporte je Tab, All Error Inlinks, Issues, URL Inspection, KI, Multi-Export, Non-Indexable Page Inlinks Only (OUT-068), Embedding-Vektoren (OUT-069) |
| [4 Reports](#4-reports) | OUT-080 – OUT-114 | Crawl Overview, Redirects, Canonicals, Pagination, Hreflang, Insecure Content, SERP Summary, Orphan Pages, Structured Data, PageSpeed (inkl. Ressourcen je Audit, OUT-114), JavaScript, Mobile, Accessibility, HTTP-Header, Cookies |
| [5 XML- und Bild-Sitemaps](#5-xml--und-bild-sitemaps) | OUT-120 – OUT-133 | Seitenauswahl, lastmod, priority, changefreq, Bilder, hreflang, Aufteilung, Segmente |
| [6 Crawl-Vergleich](#6-crawl-vergleich) | OUT-140 – OUT-154 | Compare-Modus, Added/New/Removed/Missing, Change Detection, URL-Mapping, Site Structure, Auto-Compare, Report „Crawl Changes“ |
| [7 Geplante Crawls](#7-geplante-crawls-scheduling) | OUT-160 – OUT-171 | Aufgaben, Intervalle, OS-Integration, Exporte, Historie, Benachrichtigungen, Import/Export |
| [8 Kommandozeile / Headless](#8-kommandozeile--headless) | OUT-180 – OUT-209 | alle Optionen, Exit-Codes, Fehlerverhalten |
| [9 Looker-Studio-Export](#9-looker-studio-export-custom-crawl-overview) | OUT-210 – OUT-214 | Zeitreihen-Zeile je Crawl, Metrikkatalog, Segmente |
| [10 Konfigurationsdateien & Crawl-Dateien](#10-konfigurationsdateien--crawl-dateien) | OUT-220 – OUT-234 | Profile, Standardkonfiguration, Auth-Profile, Einstellungen, Crawl-Verwaltung, Aufbewahrung, Manifest und Leseschnittstelle der Crawl-Datei |
| [11 Logging & Diagnose](#11-logging--diagnose) | OUT-240 – OUT-247 | Logdateien, Level, Rotation, Log-Paket, Nutzungsstatistik, Absturzberichte |
| [12 Update-Mechanismus](#12-update-mechanismus) | OUT-250 – OUT-255 | Prüfen, Herunterladen, Signatur, Abschalten |
| [13 Plattformanforderungen](#13-plattformanforderungen) | OUT-260 – OUT-274 | macOS arm64/x64, Windows x64, Installer, Signatur, Pfade, Ressourcen, Zertifikate, Linux-Serverbetrieb (Kern und CLI), Docker-Image |
| [14 Benachrichtigungen & Automatisierungs-Schnittstellen](#14-benachrichtigungen--automatisierungs-schnittstellen) | OUT-280 – OUT-285 | E-Mail-Konten, Abschlussmail, Anhänge, Desktop-Benachrichtigung, MCP-Server |
| [15 Nachtrag 09.11.2026](#15-nachtrag-09112026-funktionen-für-1x-nach-10) | OUT-286 – OUT-289 | 1.x: fremde Läufe pausieren, CLI-Verwaltung, Profile als JSON, Markdown |

Prioritäten: **P0** = MVP-Kern, **P1** = für Parität nötig, **P2** = später/Nische.

---

## 0 Konventionen

- **Kanonische Spaltennamen:** Alle Spalten-, Tab-, Filter-, Bulk-Export- und Report-Namen haben einen festen englischen Bezeichner (z. B. `Address`, `Status Code`, `Indexability`). Diese Bezeichner sind die Schlüssel für CLI, Presets und Looker-Export und bleiben sprachunabhängig. Die UI darf übersetzte Anzeigenamen zeigen; Exporte nutzen standardmäßig die kanonischen englischen Namen **(OS-Festlegung,** Option „Spaltenüberschriften in UI-Sprache“ unter Einstellungen → Export). Hintergrund: Die Referenz exportiert in UI-Sprache, wodurch Vorlagen in anderen Sprachen brechen – das vermeiden wir.
- **Spaltenkatalog:** Die vollständigen Spalten der Haupt-Tabs (Internal, External, Response Codes …) definieren die Spec-Dateien der jeweiligen Bereiche. Dieses Dokument definiert die Spalten von Bulk-Exporten und Reports, die nicht 1:1 einer Tab-Ansicht entsprechen.
- **Fixtures:** Jede Testfixture liegt unter `tests/fixtures/out-<name>/` und wird vom `os-testsite`-Server auf `http://127.0.0.1:<port>/` ausgeliefert. Erwartete Exporte liegen als Golden-Dateien unter `tests/golden/out-<name>/`. Platzhalter `{BASE}` steht für die Fixture-Basis-URL.
- **Vergleich von Golden-Dateien:** Zeitstempel, Antwortzeiten und Crawl-IDs werden vor dem Vergleich normalisiert (durch `<TS>`, `<MS>`, `<ID>` ersetzt).
- **Zahlenformat in Exporten:** Dezimalpunkt `.`, kein Tausendertrennzeichen, Prozentwerte als Zahl 0–100 mit max. 2 Nachkommastellen (CSV) bzw. als numerische Zelle (XLSX). Datum/Zeit ISO 8601 mit Zeitzonenoffset (`2026-10-13T09:35:00+02:00`).
- **Leere Werte:** Fehlende Werte sind leere Zellen, nicht `null`, `-` oder `0`. Eine echte `0` wird als `0` ausgegeben.
- **„Inlinks-Export zu Filter X“** bezeichnet in diesem Dokument immer: alle Links (Hyperlinks und Ressourcen-Referenzen) im Crawl, deren Ziel-URL im Filter X enthalten ist, im Inlink-Spaltenformat aus OUT-031.
- **Eine Quelle je Funktion (Spec v1):** Dieses Dokument ist führend für Exporte, Reports, CLI, Scheduling, Crawl-Vergleich (ohne Change Detection, führend TECH-300), Update und Plattform. Ist eine Funktion in einem anderen Bereich führend, steht hier nur ein **Verweis-Eintrag** (Priorität = die des führenden Eintrags, Zeile „Verweis: siehe <ID> (führend)“); IDs werden dabei weder gelöscht noch umnummeriert. Umgekehrt sind Zusatzkriterien aus anderen Bereichen in die hier führenden Einträge übernommen und als „(übernommen aus <ID>)“ gekennzeichnet.
- **Priorität je ID:** Jede ID hat genau eine Priorität (die ihres Kerns). Teilumfänge mit abweichender Priorität tragen am Akzeptanzkriterium bzw. an der Option das Präfix **[P1]** oder **[P2]**.

---

## 1 Export-Engine & Formate

### OUT-001 Gemeinsame Export-Pipeline
- **Priorität:** P0
- **Beschreibung:** Alle tabellarischen Exporte (Tab, Detail-Tab, Bulk, Report, Multi-Export, CLI, Scheduling) laufen über eine gemeinsame Pipeline: Datenquelle (Abfrage auf `os-store`) → Zeilen-Stream → Formatschreiber (CSV/XLSX/XLS/Google Sheets). UI, CLI und Scheduling liefern für dieselbe Auswahl byte-identische Dateien (abgesehen von Zeitstempeln im Dateinamen).
- **Eingaben/Konfiguration:** Exportziel (Datei/Ordner/Google Drive), Format, optional Segment, optional Spaltenauswahl.
- **Ausgabe (Spalten/Filter/Export):** Eine Tabelle mit Kopfzeile (kanonische Spaltennamen) und einer Zeile pro Datensatz.
- **Akzeptanzkriterien:**
  - [ ] Ein Export von „Internal:All“ über die UI und über `openspider --load-crawl <id> --export-tabs "Internal:All"` ergibt nach Normalisierung identische CSV-Dateien.
  - [ ] Die Kopfzeile ist immer vorhanden, auch wenn keine Datenzeilen existieren (außer OUT-012 „Leere Exporte überspringen“ greift).
  - [ ] Die Pipeline arbeitet streamend: Ein Export mit 1 000 000 Zeilen und 60 Spalten erhöht den Speicherbedarf des Prozesses um höchstens 300 MB gegenüber dem Leerlauf (Messung in CI mit synthetischem Crawl).
  - [ ] Sortierung im Export entspricht der Standardsortierung der Datenquelle (für Tab-Exporte: aktuelle UI-Sortierung, siehe OUT-020; sonst Crawl-Reihenfolge = aufsteigende interne URL-ID).
- **Testfixture:** `out-basic-site`: 12 HTML-Seiten, 2 Bilder, 1 CSS, 1 JS, ein 404, ein 301, ein externer Link; Golden-Exporte für Internal:All in allen Formaten.

### OUT-002 CSV-Format
- **Priorität:** P0
- **Beschreibung:** CSV nach RFC 4180.
- **Eingaben/Konfiguration:** Einstellungen → Export: „UTF-8-BOM schreiben“ (Standard: an), „Trennzeichen“ (Komma Standard; **[P2]** Semikolon und Tab wählbar), „Zeilenende“ (CRLF Standard **(OS-Festlegung)**).
- **Ausgabe:** Datei `*.csv`, Kodierung UTF-8.
- **Akzeptanzkriterien:**
  - [ ] Felder, die Trennzeichen, `"`, CR oder LF enthalten, werden in `"` eingeschlossen; enthaltene `"` werden verdoppelt.
  - [ ] Zeilenumbrüche innerhalb eines Feldes (z. B. mehrzeiliger Title) bleiben erhalten und brechen das Parsen mit einem RFC-konformen Parser nicht.
  - [ ] Mit aktivem BOM beginnt die Datei mit `EF BB BF`; ohne BOM nicht.
  - [ ] Nicht-ASCII-Zeichen (z. B. `ü`, `日本`, Emoji) werden unverändert UTF-8-kodiert ausgegeben.
  - [ ] Zahlen werden ohne Anführungszeichen und mit Dezimalpunkt geschrieben, unabhängig von der Systemsprache (Test unter `LANG=de_DE`).
  - [ ] Ungültige UTF-8-Sequenzen aus Quelldaten werden durch U+FFFD ersetzt; der Export bricht nicht ab.
  - [ ] [P2] Mit Trennzeichen Semikolon bzw. Tab gelten die Quoting-Regeln für das gewählte Trennzeichen; Zahlen behalten den Dezimalpunkt.
- **Testfixture:** `out-csv-edge`: Seiten mit Title `a,b`, Title mit `"Zitat"`, Title mit Zeilenumbruch (`<title>Zeile1\nZeile2</title>`), Title `日本語 🕷`, Seite in ISO-8859-1 deklariert mit ungültigen Bytes.

### OUT-003 Excel-Workbook (XLSX)
- **Priorität:** P0
- **Beschreibung:** Export als Office-Open-XML-Arbeitsmappe.
- **Eingaben/Konfiguration:** Format „Excel Workbook (.xlsx)“.
- **Ausgabe:** Datei `*.xlsx`, ein Arbeitsblatt je Export (bzw. mehrere bei Konsolidierung, OUT-007).
- **Akzeptanzkriterien:**
  - [ ] Numerische Spalten (Status Code, Größen, Längen, Pixelbreiten, Antwortzeiten, Zählwerte) sind als Zahlenzellen gespeichert, Texte als Inline-/Shared-Strings, Datumswerte als Datumszellen mit Format `yyyy-mm-dd hh:mm:ss`.
  - [ ] Kein Zellwert wird je als Formel geschrieben; ein Title `=SUM(A1)` erscheint in Excel als Text `=SUM(A1)`.
  - [ ] Erste Zeile ist fixiert und hat einen Autofilter über alle Spalten **(OS-Festlegung)**.
  - [ ] Texte über 32 767 Zeichen werden auf 32 767 Zeichen gekürzt; die letzten Zeichen werden durch `…[truncated]` ersetzt und ein Hinweis im Export-Log vermerkt.
  - [ ] Mehr als 1 048 575 Datenzeilen: Fortsetzung auf weiteren Blättern `<Name> (2)`, `<Name> (3)` … jeweils mit Kopfzeile; Warnung an den Nutzer.
  - [ ] Blattnamen sind max. 31 Zeichen lang, enthalten keine der Zeichen `[]:*?/\` und sind innerhalb der Datei eindeutig (Suffix ` (2)` bei Kollision).
  - [ ] Datei öffnet ohne Reparaturdialog in Excel (Win/Mac), LibreOffice und Numbers (manueller Test, einmal pro Release).
- **Testfixture:** `out-csv-edge` (wie OUT-002) + synthetischer Crawl mit 1,1 Mio. URLs (generiert, nicht gecrawlt) für den Zeilenüberlauf.

### OUT-004 Excel 97–2004 (XLS)
- **Priorität:** P2
- **Beschreibung:** Legacy-Binärformat für alte Excel-Versionen.
- **Eingaben/Konfiguration:** Format „Excel 97–2004 (.xls)“; CLI `--export-format xls`.
- **Ausgabe:** Datei `*.xls` (BIFF8).
- **Akzeptanzkriterien:**
  - [ ] Max. 65 535 Datenzeilen und 256 Spalten pro Blatt; Überlauf wie bei OUT-003 auf Folgeblätter, Spalten über 256 werden abgeschnitten und gewarnt.
  - [ ] Texte max. 32 767 Zeichen.
  - [ ] Ist kein lizenzkompatibler BIFF8-Schreiber verfügbar, darf das Format in der UI ausgeblendet sein; die CLI meldet dann `export format 'xls' not supported` mit Exit-Code 2.
- **Testfixture:** `out-basic-site`.

### OUT-005 Google-Sheets-Export
- **Priorität:** P1
- **Beschreibung:** Export direkt in eine Google-Tabelle im Google Drive des Nutzers.
- **Eingaben/Konfiguration:** Google-Konto (OAuth über System-Browser mit PKCE, Scope nur `drive.file`); Ordnerstruktur in Drive: `OpenSpider/<Projektname>/<Aufgaben- bzw. Crawlname>/` **(OS-Festlegung;** Referenz nutzt einen festen Stammordner mit Projekt-/Crawl-Unterordnern).
- **Ausgabe:** Eine Google-Tabelle pro Export (bzw. eine mit mehreren Blättern bei Konsolidierung); Rückgabe des Links an UI/CLI-Log.
- **Akzeptanzkriterien:**
  - [ ] Werte werden im Modus „roh“ geschrieben (keine Interpretation als Formel, keine automatische Datumserkennung); `=1+1` bleibt Text.
  - [ ] Zellenlimit: Bevor geschrieben wird, wird `Zeilen × Spalten` gegen das konfigurierbare Limit geprüft (Standard 10 000 000 Zellen, entspricht dem aktuellen Google-Limit; Referenz dokumentiert 5 Mio.). Überschreitung → Kürzung auf die maximal mögliche Zeilenzahl, deutliche Warnung in UI/Log mit Angabe der abgeschnittenen Zeilen, Empfehlung CSV/XLSX.
  - [ ] Schreibvorgänge erfolgen in Blöcken (≤ 10 000 Zeilen je Request); bei Timeout/HTTP 429/5xx bis zu 5 Wiederholungen mit exponentiellem Backoff (1 s, 2 s, 4 s, 8 s, 16 s).
  - [ ] „Überschreiben“: existiert die Tabelle gleichen Namens im Zielordner, wird ihr Inhalt ersetzt (ID bleibt gleich, damit Verknüpfungen erhalten bleiben). Wurde sie zwischenzeitlich vom Nutzer gelöscht oder in den Papierkorb gelegt, wird sie neu angelegt statt abzubrechen.
  - [ ] Abgelaufenes/entzogenes Token → Exporte nach Drive schlagen mit klarer Meldung „Google-Konto erneut verbinden“ fehl; lokale Exporte desselben Laufs werden trotzdem geschrieben.
  - [ ] Tokens werden im OS-Schlüsselbund (macOS Keychain / Windows Credential Manager) gespeichert, nie in Konfigurationsdateien.
- **Testfixture:** Mock-Server für die Sheets-/Drive-API im `os-testsite` (Endpunkte simulieren 429, Timeout, gelöschte Datei); `out-basic-site` als Datenquelle.

### OUT-006 Exportdialog und Formatwahl
- **Priorität:** P0
- **Beschreibung:** Jeder Export in der UI öffnet einen Speichern-Dialog mit Formatwahl; das zuletzt gewählte Format und Verzeichnis werden pro Nutzer gemerkt.
- **Eingaben/Konfiguration:** Format: CSV, Excel Workbook, Excel 97–2004 (falls verfügbar), Google Sheets (falls Konto verbunden).
- **Ausgabe:** Datei bzw. Google-Tabelle.
- **Akzeptanzkriterien:**
  - [ ] Vorgeschlagener Dateiname gemäß OUT-008.
  - [ ] Nach erfolgreichem Export bietet eine Meldung „Ordner öffnen“ bzw. „In Google Sheets öffnen“ an.
  - [ ] Existiert die Zieldatei, fragt das Betriebssystem-Dialogfenster nach Überschreiben; bei Ablehnung wird nichts geschrieben.
  - [ ] Format und Verzeichnis des letzten Exports werden beim nächsten Export vorbelegt (auch nach Neustart).
- **Testfixture:** UI-E2E-Test mit `out-basic-site`.

### OUT-007 Konsolidierung mehrerer Exporte in einer Arbeitsmappe
- **Priorität:** P1
- **Beschreibung:** Werden mehrere Exporte gleichzeitig erzeugt (Multi-Export, Scheduling, CLI), können sie statt in Einzeldateien als Blätter einer einzigen XLSX-Datei bzw. Google-Tabelle abgelegt werden.
- **Eingaben/Konfiguration:** Option „In einer Arbeitsmappe zusammenfassen“ (Standard aus); CLI `--single-workbook` **(OS-Festlegung,** Flagname).
- **Ausgabe:** Eine Datei `<crawlname>_export.xlsx` bzw. eine Google-Tabelle mit je einem Blatt pro Export; Blattreihenfolge = Auswahlreihenfolge.
- **Akzeptanzkriterien:**
  - [ ] Blattnamen werden aus „Tab Filter“ bzw. „Bulk-Export-Name“ gebildet und nach den Regeln aus OUT-003 gekürzt/eindeutig gemacht.
  - [ ] Bei CSV ist die Option deaktiviert (ausgegraut) bzw. wird in der CLI mit Warnung ignoriert.
  - [ ] Exporte, die keine Tabellen sind (Screenshots, Quelltext, Sitemaps), werden nicht in die Arbeitsmappe aufgenommen, sondern weiterhin als Dateien geschrieben.
- **Testfixture:** `out-basic-site`, Multi-Export von Internal:All, Response Codes:All, Links:All Inlinks.

### OUT-008 Dateinamen-Konvention
- **Priorität:** P0
- **Beschreibung:** Einheitliche, vorhersagbare Dateinamen, die CLI-Nutzer per Skript weiterverarbeiten können.
- **Eingaben/Konfiguration:** keine.
- **Ausgabe:** Dateiname = kanonischer Name in Kleinbuchstaben, Leer- und Sonderzeichen durch `_` ersetzt, Bestandteile mit `_` verbunden. Beispiele **(OS-Festlegung)**: `internal_all.csv`, `response_codes_client_error_(4xx).csv` → normalisiert `response_codes_client_error_4xx.csv`, `links_all_inlinks.csv` (Bulk-Export `Links:All Inlinks`, Menü und Name verbunden; M1-Plan K-50), `redirect_chains.csv`, `crawl_overview.csv`.
- **Akzeptanzkriterien:**
  - [ ] Erlaubte Zeichen im Dateinamen: `a-z 0-9 _ - .`; mehrere `_` hintereinander werden zu einem zusammengefasst; führende/abschließende `_` entfallen.
  - [ ] Dateinamen sind ≤ 120 Zeichen (Kürzung vor der Erweiterung) und auf Windows gültig (keine reservierten Namen wie `con`, `nul`).
  - [ ] Bei Segment-Exporten wird `_segment_<segmentname>` angehängt.
  - [ ] Der Dateinamensvorschlag des UI-Exportdialogs hängt an den Basisnamen dieses Eintrags `_<YYYY-MM-DD>` an; Regel und Datumsquelle sind in OUT-020 führend festgelegt. CLI, Scheduling und Multi-Export verwenden den Basisnamen ohne Datum; dort trennen Zeitstempel-Ordner die Läufe (OUT-013, OUT-190).
  - [ ] Die Zuordnung kanonischer Name → Dateiname ist in einer Tabelle in `os-export` hinterlegt und per `openspider --help export-tabs --verbose` einsehbar.
- **Testfixture:** Unit-Test über alle registrierten Exportnamen (keine Kollision zweier Exporte auf denselben Dateinamen).

### OUT-009 Spaltenauswahl und -reihenfolge
- **Priorität:** P1
- **Beschreibung:** Tab-Exporte übernehmen die in der UI eingestellte Spaltenauswahl und -reihenfolge (sichtbare Spalten in sichtbarer Reihenfolge). Ein „Spalten-Konfigurator“ erlaubt, Spalten in Masse ein-/auszublenden und neu zu ordnen; „Spalten zurücksetzen“ stellt den Standard wieder her.
- **Eingaben/Konfiguration:** Spaltenkonfiguration pro Tab (wird mit dem Konfigurationsprofil gespeichert, OUT-220).
- **Ausgabe:** Export-Spalten = sichtbare Spalten.
- **Akzeptanzkriterien:**
  - [ ] Wird in „Internal“ die Spalte `Meta Keywords 1` ausgeblendet und `Title 1` vor `Status Code` verschoben, enthält der Export diese Reihenfolge und keine Meta-Keywords-Spalte.
  - [ ] CLI/Scheduling verwenden die Spaltenkonfiguration aus der übergebenen Konfigurationsdatei; ohne Konfigurationsdatei die Standardspalten.
  - [ ] Bulk-Exporte und Reports haben feste Spalten (nicht konfigurierbar) **(OS-Festlegung)**.
  - [ ] `Address` bleibt immer erste Spalte und kann nicht ausgeblendet werden.
- **Testfixture:** `out-basic-site` mit gespeicherter Spaltenkonfiguration `tests/fixtures/out-basic-site/columns.toml`.

### OUT-010 Exportfortschritt und Abbruch
- **Priorität:** P1
- **Beschreibung:** Lang laufende Exporte zeigen Fortschritt und sind abbrechbar, ohne die UI zu blockieren.
- **Eingaben/Konfiguration:** keine.
- **Ausgabe:** Fortschrittsanzeige (Zeilen geschrieben / gesamt, Prozent).
- **Akzeptanzkriterien:**
  - [ ] Die UI bleibt während eines Exports bedienbar (Tabellen scrollbar).
  - [ ] „Abbrechen“ stoppt innerhalb von 2 s; die unvollständige Datei wird gelöscht (lokal) bzw. die halbfertige Google-Tabelle bleibt mit Hinweisblatt „unvollständig“ bestehen.
  - [ ] Ein Export während eines laufenden Crawls exportiert den Stand zum Startzeitpunkt des Exports (konsistenter Snapshot über SQLite-Lesetransaktion).
- **Testfixture:** synthetischer Crawl mit 500 000 URLs.

### OUT-011 Segment-Einschränkung von Exporten
- **Priorität:** P1
- **Beschreibung:** Ist ein Segment aktiv (rechts „Segments“ bzw. Segment-Auswahl), beziehen sich Tab-Exporte, Bulk-Exporte und Reports nur auf URLs dieses Segments. In Scheduling/CLI kann pro Export ein Segment gewählt werden oder „alle Segmente einzeln“.
- **Eingaben/Konfiguration:** Segmentauswahl; CLI `--segment <name>` bzw. `--segment all` **(OS-Festlegung,** Flagname).
- **Ausgabe:** gefilterte Exporte, Dateiname mit Segmentsuffix (OUT-008).
- **Akzeptanzkriterien:**
  - [ ] Bei Inlink-Exporten gilt das Segment für die **Ziel**-URL (Segment „/blog/“ → alle Links, die auf Blog-URLs zeigen) **(OS-Festlegung)**.
  - [ ] Mit `--segment all` entsteht pro konfiguriertem Segment eine Datei sowie eine ungefilterte Datei.
  - [ ] Bulk-Exporte funktionieren auch bei aktivem Segment (bekannter Fehler der Referenz, der hier nicht auftreten darf).
  - [ ] Segment-Definition und -Zuordnung kommen ausschließlich aus CRAWL-134 (Darstellung UI-020/UI-067/UI-068): Ein Segment-Export enthält genau die URLs, deren Tabellenspalte `Segment` dieses Segment zeigt; der Export berechnet die Zuordnung nicht neu (übernommen aus UI-068).
- **Testfixture:** `out-segments`: Seiten unter `/blog/`, `/shop/`, `/info/`; Segmentdefinitionen „Blog“ (Pfad enthält `/blog/`), „Shop“.

### OUT-012 Leere Exporte überspringen
- **Priorität:** P1
- **Beschreibung:** Option, Exporte ohne Datenzeilen gar nicht erst zu schreiben (Multi-Export, Scheduling, CLI). Leere Exporte werden dann auch nicht an E-Mails angehängt.
- **Eingaben/Konfiguration:** Checkbox „Leere Exporte überspringen“ (Standard aus); CLI `--skip-empty-exports` **(OS-Festlegung,** Flagname).
- **Ausgabe:** nur nicht-leere Dateien.
- **Akzeptanzkriterien:**
  - [ ] Bei aktivierter Option und einem Crawl ohne 4xx-URLs wird `response_codes_client_error_4xx.csv` nicht erzeugt; ohne Option wird eine Datei nur mit Kopfzeile erzeugt.
  - [ ] Die Zusammenfassung (Log/CLI-Ausgabe) listet übersprungene Exporte namentlich.
- **Testfixture:** `out-basic-site` ohne 5xx-Seite.

### OUT-013 Ausgabeordner: Überschreiben vs. Zeitstempel-Ordner
- **Priorität:** P0
- **Beschreibung:** Für automatisierte Exporte (CLI, Scheduling) wird festgelegt, ob Dateien im Zielordner überschrieben werden oder je Lauf ein eigener Unterordner mit Zeitstempel angelegt wird. Gilt analog für Google Drive.
- **Eingaben/Konfiguration:** „Dateien überschreiben“ / „Zeitstempel-Ordner anlegen“.
- **Ausgabe:** Zeitstempel-Ordnername `YYYY-MM-DD_HH-MM-SS` in lokaler Zeit **(OS-Festlegung)**.
- **Akzeptanzkriterien:**
  - [ ] Zeitstempel-Modus: zwei Läufe in derselben Minute erzeugen zwei unterschiedliche Ordner; fällt die Sekunde zusammen, wird `_2` angehängt.
  - [ ] Überschreiben-Modus: vorhandene gleichnamige Dateien werden ersetzt; andere Dateien im Ordner bleiben unangetastet.
  - [ ] Keiner der beiden Modi und Zielordner nicht leer → Abbruch **vor** dem Crawl mit verständlicher Fehlermeldung (CLI: Exit-Code 2). (Abweichung: Die Referenz scheitert hier still.)
  - [ ] Zielordner existiert nicht → er wird angelegt; ist das nicht möglich (Rechte), Abbruch vor dem Crawl mit Exit-Code 2.
- **Testfixture:** CLI-Integrationstest mit temporären Verzeichnissen.

### OUT-014 Kopieren aus Tabellen in die Zwischenablage
- **Priorität:** P2
- **Beschreibung:** Markierte Zellen/Zeilen jeder Tabelle können als tabulatorgetrennter Text kopiert werden (zum Einfügen in Tabellenkalkulationen).
- **Eingaben/Konfiguration:** Strg/Cmd+C, Kontextmenü „Kopieren“, „URL kopieren“.
- **Ausgabe:** TSV in der Zwischenablage, optional mit Kopfzeile („Mit Spaltenköpfen kopieren“).
- **Akzeptanzkriterien:**
  - [ ] Drei markierte Zeilen ergeben drei TSV-Zeilen; Tabs/Zeilenumbrüche in Zellen werden durch Leerzeichen ersetzt.
  - [ ] Kopieren von mehr als 100 000 Zeilen fragt vorher nach.
- **Testfixture:** UI-Test mit `out-basic-site`.

### OUT-015 Exporte in Listenmodus-Reihenfolge
- **Priorität:** P1
- **Beschreibung:** Im Listenmodus kann der Export die URLs exakt in der Reihenfolge der hochgeladenen Liste ausgeben – einschließlich Duplikaten und nachträglich korrigierter URLs (z. B. ergänztes Schema).
- **Eingaben/Konfiguration:** automatisch im Listenmodus für Tab-Exporte des „Internal“/„All“-Bereichs; Option im Exportdialog „Upload-Reihenfolge beibehalten“ (Standard an).
- **Ausgabe:** Zusätzliche erste Spalte `Original Url` (wie hochgeladen) vor `Address` **(OS-Festlegung)**.
- **Akzeptanzkriterien:**
  - [ ] Liste `B, A, B, example.com/c` → Export enthält 4 Zeilen in dieser Reihenfolge; Zeile 4 hat `Original Url` = `example.com/c`, `Address` = `http://example.com/c` (bzw. die normalisierte Form).
  - [ ] Nicht gecrawlte Listeneinträge (ungültige URL, durch Exclude ausgeschlossen) erscheinen mit leerer `Address` und `Status` = `Invalid`/`Excluded`.
  - [ ] Duplikate werden nur einmal gecrawlt, aber mehrfach exportiert.
- **Testfixture:** `out-list-order`: Listendatei `list.txt` mit Duplikaten, einer URL ohne Schema, einer ungültigen Zeile und einer ausgeschlossenen URL.

### OUT-016 Formel- und Injection-Schutz für CSV
- **Priorität:** P2
- **Beschreibung:** Optionaler Schutz gegen CSV-Injection, da gecrawlte Inhalte (Titles, Anker) fremdkontrolliert sind.
- **Eingaben/Konfiguration:** Einstellungen → Export: „Zellen, die mit `= + - @` beginnen, in CSV mit `'` maskieren“ (Standard aus, um Daten unverändert zu lassen) **(OS-Festlegung)**.
- **Ausgabe:** maskierte Zellen.
- **Akzeptanzkriterien:**
  - [ ] Mit Option: Title `=HYPERLINK("x")` wird als `'=HYPERLINK(""x"")` geschrieben; Zahlen wie `-5` (numerische Spalten) werden nicht maskiert.
  - [ ] Ohne Option bleibt der Wert unverändert.
- **Testfixture:** `out-csv-edge` (Seite mit Title `=HYPERLINK("http://evil")`).

---

## 2 Tab- und Detail-Exporte

### OUT-020 Export des aktuellen Haupt-Tabs inkl. Filter
- **Priorität:** P0
- **Beschreibung:** Die Schaltfläche „Export“ über der Haupttabelle exportiert genau die aktuell sichtbare Ansicht: gewählter Tab, gewählter Filter, aktive Suche (inkl. Regex/Spaltensuche), aktive Segment-Einschränkung und aktuelle Sortierung. Führender Eintrag für „Export der aktuellen Ansicht“; ONPAGE-191 und UI-015 verweisen hierher (Lage der Schaltfläche: UI-015).
- **Eingaben/Konfiguration:** aktueller UI-Zustand; Format (OUT-006); Option „Alle Spalten exportieren (auch ausgeblendete)“ im Exportdialog, Standard aus (übernommen aus UI-015/ONPAGE-191).
- **Ausgabe:** Datei mit sichtbaren Spalten (OUT-009), Zeilen in sichtbarer Reihenfolge; mit Option „Alle Spalten“: alle Spalten des Tabs in Standardreihenfolge der Fachspec. Dateiname nach OUT-008, Kodierung/Quoting nach OUT-002/OUT-003.
- **Akzeptanzkriterien:**
  - [ ] Mit Option „Alle Spalten“ enthält der Export von „Page Titles:Missing“ auch die ausgeblendete Spalte `Title 1 Pixel Width`; ohne Option nicht (übernommen aus ONPAGE-191).
  - [ ] Golden-Test: Export „Page Titles:Missing“ der Fixture `onpage-titles` (Fachspec 02) mit Option „Alle Spalten“ ist nach Normalisierung byte-identisch zur dort abgelegten Golden-CSV (übernommen aus ONPAGE-191).
  - [ ] Bei deutscher Systemsprache steht `Response Time` als `0.512`, nicht `0,512`; in XLSX ist `Status Code` eine Zahlenzelle, `Hash` eine Textzelle (übernommen aus ONPAGE-191).
  - [ ] Tab „Response Codes“, Filter „Client Error (4xx)“ → Export enthält genau die Zeilen dieses Filters (Anzahl = Zähler in der Filterliste).
  - [ ] Mit Suchbegriff `/blog/` in der Suchleiste enthält der Export nur passende Zeilen.
  - [ ] Nach Sortierung nach `Word Count` absteigend ist die erste Exportzeile die mit der höchsten Wortzahl.
  - [ ] Für jeden Tab und jeden Filter aller Spec-Bereiche existiert ein Export (automatisierter Test iteriert über die Filter-Registry und prüft, dass ein Export ohne Fehler entsteht).
  - [ ] Während eines laufenden Crawls ist der Export möglich (Snapshot, OUT-010).
  - [ ] Dateinamensvorschlag (führend hier, Entscheidung des Architekten; übernommen aus UI-015): `<tab>_<filter>_<YYYY-MM-DD>` aus dem Basisnamen nach OUT-008 plus Datum; Datum = **Startdatum des Crawls in lokaler Zeitzone** (im Vergleichsmodus das des Current-Crawls), nicht das Exportdatum, damit Exporte älterer Crawls richtig datiert sind; ein Segmentsuffix steht vor dem Datum. Beispiel: „Response Codes:Client Error (4xx)“ eines am 2026-10-13 um 23:30 Ortszeit (21:30 UTC) gestarteten und am 2026-10-14 exportierten Crawls → `response_codes_client_error_4xx_2026-10-13.csv`. Hintergrundausführung und Abbruch: OUT-010; Mehrfachauswahl: OUT-022.
- **Testfixture:** `out-basic-site`.

### OUT-021 Export der unteren Detail-Tabs
- **Priorität:** P0
- **Beschreibung:** Kern (P0) ist der Export der Detail-Tabs Inlinks und Outlinks; alle übrigen Detail-Tabs sind Teilumfang **[P1]**, N-grams **[P2]** wegen der Priorität des Detail-Tabs UI-049 (Kennzeichnung an den Akzeptanzkriterien). Spaltennamen der Detail-Exporte sind die kanonischen Schlüssel des jeweils führenden Eintrags; weichen sie hier ab, gilt der führende Eintrag. Inhalt der Detail-Tabs: jeweilige Fachspec; Layout: UI-031 ff. Für die in der Haupttabelle markierte(n) URL(s) lassen sich die Detailtabellen des unteren Fensters exportieren: URL Details, Inlinks, Outlinks, Image Details, Resources, Duplicate Details, Structured Data Details, Spelling & Grammar Details, Lighthouse Details, Accessibility Details, HTTP Headers, Cookies, Chrome Console Log, N-grams. Zugriff über die Export-Schaltfläche des unteren Fensters oder Rechtsklick → Export → <Detailtab>. Detail-Tabs, deren Daten für die gewählte URL nicht erhoben wurden (z. B. Lighthouse ohne PSI/Lighthouse-Integration), sind im Export-Menü ausgegraut; der Tooltip nennt die nötige Einstellung.
- **Eingaben/Konfiguration:** Auswahl in der Haupttabelle; Filter des Detail-Tabs (z. B. Inlinks-Filter „Auto/Current/Previous“ im Vergleichsmodus; N-grams-Auswahl n = 1…6 und Inhaltsbereich).
- **Ausgabe:** Spalten wie im jeweiligen Detail-Tab; Inlinks/Outlinks im Format aus OUT-031. Kanonische Exportpfade **(OS-Festlegung)**: `Detail:<Detailtab>` (z. B. `Detail:Lighthouse Details`), damit Detail-Exporte auch per CLI für eine URL-Liste (**[P2]** `--detail-export "<Detailtab>" --urls <datei>`) ansprechbar sind.
  - Lighthouse Details: Langformat `Address`, `Category` (Performance/Accessibility/Best Practices/SEO), `Audit ID`, `Audit Title`, `Score` (0–1, leer bei informativen Audits), `Display Value`, `Numeric Value`, `Numeric Unit`, `Savings (ms)`, `Savings (Bytes)`, `Item` (je Detailzeile eines Audits, z. B. Ressourcen-URL), `Item Value`.
  - Accessibility Details: zwei Teiltabellen, im XLSX als zwei Blätter, im CSV als zwei Dateien `…_rules.csv` und `…_locations.csv` **(OS-Festlegung)**; Spaltennamen und Ergebniswerte führend TECH-259: (a) Regeln: `Address`, `Issue`, `Result` (`Violation`/`Incomplete`/`Inapplicable`/`Passed`), `Guideline`, `Impact`, `Nodes`, `Help URL`; (b) Knoten: `Address`, `Issue`, `Location on Page` (CSS-Selektor), `HTML`.
  - HTTP Headers: `Address`, `Direction` (`Request`/`Response`), `Header Name`, `Header Value` (Redaktion wie OUT-048).
  - Cookies: Spalten und Werte wie UI-043 (führend; identisch mit OUT-049).
  - Chrome Console Log: `Address`, `Level`, `Message`, `Source URL`, `Line`, `Column`, `Timestamp` (kanonische Schlüssel aus TECH-114, führend).
  - N-grams: `N-Gram`, `Occurrences`, `Pages`, `Linked`, `Unlinked` (kanonische Schlüssel und Bedeutung aus UI-049, führend; dort nicht definierte Zusatzspalten gibt es im Export nicht).
- **Akzeptanzkriterien:**
  - [ ] Inlinks-Export einer URL mit 3 eingehenden Verweisen (2× Hyperlink, 1× Canonical) liefert 3 Zeilen mit `Type` = `Hyperlink`, `Hyperlink`, `Canonical`.
  - [ ] [P1] URL Details: zwei Spalten `Name`, `Value` mit allen Eigenschaften der URL; Anzahl Zeilen = Anzahl Spalten der Haupttabelle „Internal:All“ (alle, auch ausgeblendete).
  - [ ] [P1] Structured Data Details: eine Zeile pro Eigenschaft mit Spalten `Subject`, `Predicate`, `Object`, `Format` (JSON-LD/Microdata/RDFa), `Validation Type`, `Severity`, `Issue`.
  - [ ] [P1] Duplicate Details: Spalten `Address`, `Near Duplicate Address`, `Similarity` (Prozent).
  - [ ] [P1] Spelling & Grammar Details: Spalten führend ONPAGE-157, Kopfzeile genau `Address,Error,Type,Detail,Suggestion,Page Section,Rule ID` (Paritätsspalten, dann die OpenSpider-Erweiterungen; `Page Section` ∈ `Page Title`/`Meta Description`/`Body`).
  - [ ] [P1] Lighthouse Details: Mock-PSI-Antwort mit 3 Audits, davon eines mit 4 Detailzeilen → 6 Zeilen (1 + 1 + 4); `Score` des informativen Audits ist leer, nicht 0.
  - [ ] [P1] Accessibility Details: Seite mit 2 Bildern ohne alt → Regeltabelle enthält die Zeile `image-alt` mit `Result`=`Violation`, `Nodes`=2; Knotentabelle 2 Zeilen mit unterschiedlichen `Location on Page`.
  - [ ] [P1] HTTP Headers: Seite mit 2 `Set-Cookie`-Headern → 2 Zeilen `Set-Cookie`, `Direction`=Response; `Authorization` im Request erscheint als `<redacted>`.
  - [ ] [P1] Chrome Console Log: Seite mit `console.warn("w")` und `console.error("e")` → 2 Zeilen mit `Level` warning bzw. error; die Kopfzeile enthält `Source URL`, nicht `Source`; ohne JS-Rendering ist der Export deaktiviert.
  - [ ] [P2] N-grams (Priorität des Detail-Tabs UI-049): Text „rot blau rot blau rot“ (unverlinkt, ein Absatz) mit n = 2 → Zeilen `rot blau` (`Occurrences` 2) und `blau rot` (`Occurrences` 2), je `Pages`=1, `Linked`=0, `Unlinked`=2; n = 1 → `rot` (3), `blau` (2); Sortierung `Occurrences` absteigend, bei Gleichstand alphabetisch. Kopfzeile genau `N-Gram,Occurrences,Pages,Linked,Unlinked`.
  - [ ] [P1] Image Details und Resources: Export enthält je eingebundener Ressource der gewählten URL eine Zeile im Standard-Linkformat (OUT-031).
  - [ ] [P2] `--detail-export "Inlinks" --urls <datei>` mit 3 URLs erzeugt eine Datei mit den Inlinks aller 3 URLs (Spalte `Address` wie OUT-022).
- **Testfixture:** `out-detail-tabs`: Seite `/target` mit 2 Hyperlinks und einem Canonical darauf, JSON-LD-Block, Rechtschreibfehler „Beispeil“, Nah-Duplikat `/target-copy`, Seite `/a11y` mit 2 Bildern ohne alt, Seite `/console` mit Konsolenmeldungen, Seite `/ngram` mit Text „rot blau rot blau rot“, Header-Seite mit 2 `Set-Cookie`; Mock-PSI-Antwort.

### OUT-022 Mehrfachauswahl-Export der Detail-Tabs
- **Priorität:** P1
- **Beschreibung:** Werden in der Haupttabelle mehrere URLs (Strg/Cmd- oder Umschalt-Klick, „Alle markieren“) gewählt, exportiert der Detail-Export die Daten aller gewählten URLs in einer Datei.
- **Eingaben/Konfiguration:** Mehrfachauswahl.
- **Ausgabe:** Wie OUT-021; bei Tabellen ohne natürliche Quell-/Zielspalte wird eine erste Spalte `Address` (die gewählte URL) ergänzt.
- **Akzeptanzkriterien:**
  - [ ] Auswahl von 3 URLs mit je 2 Inlinks → 6 Zeilen.
  - [ ] Auswahl von 50 000 URLs funktioniert ohne UI-Blockade (streamend).
- **Testfixture:** `out-detail-tabs`.

### OUT-023 Crawl-Path-Report einer URL
- **Priorität:** P1
- **Beschreibung:** Für eine gewählte URL wird der kürzeste Pfad ab Start-URL ausgegeben, über den der Crawler sie gefunden hat. Erreichbar über Rechtsklick → Export → Crawl Path Report (nicht im Reports-Menü) und per CLI über `--export-crawl-path "<URL>[,<URL>…]"` (OS-Erweiterung, K2-88, SQ-14; Namensprüfung nach OUT-203, Format nach OUT-195, Ausgabeordner nach OUT-190; Dateiname `crawl_path_report.csv` bzw. Endung nach Format).
- **Eingaben/Konfiguration:** gewählte URL(s).
- **Ausgabe:** Spalten `Target Address` (die gewählte URL, wichtig bei Mehrfachauswahl), `Step` (0 = Zielseite), `Address`, `Status Code`, `Link Type` (Typ des Verweises, über den `Address` vom nächsthöheren Schritt aus erreicht wurde: Werte wie `Type` in OUT-031), `Anchor`, `Crawl Depth`. Reihenfolge: Zielseite zuerst, Start-URL zuletzt (von unten nach oben lesen). `Link Type`/`Anchor` der letzten Zeile (Start-URL bzw. Sitemap) sind leer.
- **Akzeptanzkriterien:**
  - [ ] Für `/a/b/c` (Start → `/a` → `/a/b` → `/a/b/c`) entstehen 4 Zeilen, `Step` 0–3, letzte Zeile = Start-URL; es gilt immer: Anzahl Zeilen − 1 = `Crawl Depth` der Zielseite.
  - [ ] Weiterleitungen sind eigene Schritte: Start → `/r` (301) → `/z` ergibt 3 Zeilen, die Zeile `/z` hat `Link Type` = `HTTP Redirect`, `/r` hat `Status Code` 301.
  - [ ] Mehrere gleich kurze Pfade: verwendet wird der Pfad über den zuerst entdeckten Verweis (Crawl-Reihenfolge); dasselbe Ergebnis bei wiederholtem Export (deterministisch) **(OS-Festlegung)**.
  - [ ] Für eine URL, die nur aus der XML-Sitemap stammt (Orphan), endet der Pfad mit der Sitemap-URL und `Link Type` = `XML Sitemap`.
  - [ ] Für eine URL, die nur über GA/GSC bekannt ist (nicht gecrawlt), ist der Report leer (nur Kopfzeile) mit Hinweis „nicht über Links erreicht“.
  - [ ] Im Listenmodus: Pfad besteht nur aus der Listen-URL selbst und ggf. Weiterleitungen.
  - [ ] Mehrfachauswahl von 3 URLs → eine Datei, Blöcke je `Target Address` in Auswahlreihenfolge.
  - [ ] CLI: `--load-crawl <datei> --export-crawl-path "<BASE>/a/b/c"` erzeugt `crawl_path_report.csv` mit denselben Zeilen wie der Export aus der Oberfläche; mehrere URLs → eine Datei, Blöcke in Argumentreihenfolge; eine URL, die nicht im Crawl ist, ergibt nur die Kopfzeile und eine Warnung auf stderr (Exit-Code 0) (K2-88, SQ-14).
- **Testfixture:** `out-crawl-path`: lineare Linkkette über 3 Ebenen, eine 301-Zwischenstation, zwei gleich lange Pfade zu `/diamond` (über `/l` und `/r`, `/l` zuerst verlinkt) + Seite nur in `sitemap.xml`.

### OUT-024 Exporte im unteren Fenster: SERP Snippet, Rendered Page, View Source
- **Priorität:** P2
- **Beschreibung:** Speichern einzelner Inhalte des unteren Fensters: gerenderter Screenshot (PNG), gespeichertes HTML (Original/gerendert) als `.html`, HTTP-Header als `.txt`.
- **Eingaben/Konfiguration:** gewählte URL; die Daten müssen gespeichert worden sein (HTML speichern, Rendering, Header-Extraktion).
- **Ausgabe:** einzelne Dateien.
- **Akzeptanzkriterien:**
  - [ ] Speichern ist deaktiviert, wenn die Daten nicht vorhanden sind (Tooltip nennt die nötige Einstellung).
  - [ ] Gespeichertes HTML ist byte-identisch mit dem empfangenen Body (nach Dekompression).
- **Testfixture:** `out-detail-tabs` mit aktivierter HTML-Speicherung.

### OUT-025 Export der rechten Übersichtsfenster
- **Priorität:** P1
- **Beschreibung:** Die rechten Panels (Overview, Issues, Site Structure, Segments, Response Times, Spelling & Grammar, Semantic Search, API) sind exportierbar.
- **Eingaben/Konfiguration:** Export-Schaltfläche im jeweiligen Panel.
- **Ausgabe:**
  - Overview: identisch mit OUT-080 Crawl Overview.
  - Issues: Spalten `Issue Name`, `Issue Type` (Issue/Warning/Opportunity), `Issue Priority` (High/Medium/Low), `URLs`, `% of Total`, `Description`, `How To Fix`, dahinter `[OS-Erweiterung]` `Why It Matters` (UI-065; Entscheidung (Fable-Empfehlung) vom 13.10.2026). `Description` = Text *what*, `How To Fix` = *fix*, `Why It Matters` = *why* aus UI-065. Die Texte sind eigenständig verfasste OpenSpider-Texte (gepflegt im einzigen Issue-Katalog `core/issues/catalog.toml` mit den Textdateien `core/issues/texts/{de,en}.toml` nach UI-063/UI-065, de/en); Issue-Beschreibungen der Referenz werden weder übernommen noch paraphrasierend nachgebaut (Clean-Room-Regel).
  - Site Structure: zwei Tabellen (XLSX: zwei Blätter; CSV: `site_structure_directories.csv` und `site_structure_depth.csv`) **(OS-Festlegung** der Aufteilung):
    - Verzeichnisse: `Path` (Verzeichnis), `URLs` (gesamt), `Indexable`, `Non-Indexable`.
    - Tiefe: `Crawl Depth` (Zeilen `1` … `10`, `10+`), je Antwortcode-Klasse eine Spalte (`2xx`, `3xx`, `4xx`, `5xx`, `No Response`, `Blocked`) und `Total`. Wie die Referenz beginnt die Tiefe bei Zeile `0` für die Start-URL(s). Die frühere Annahme „die Referenz zeigt Tiefen 1 bis 10+“ ist durch den Paritätslauf M1 widerlegt (14.10.2026, K-58). Die Grundmenge dieses Panel-Exports (alle internen URLs mit bekannter Tiefe, je Antwortcode-Klasse) bleibt OS-Festlegung (P1); der Abschnitt `Crawl Depth` des Crawl Overview (OUT-080) zählt nach Referenz nur interne HTML-Seiten mit 2xx.
  - Response Times: Spalten `Response Time Range` (Buckets 0–1 s, 1–2 s, 2–3 s, 3–4 s, 4–5 s, 5–6 s, 6–7 s, 7–8 s, 8–9 s, 9–10 s, über 10 s), `URLs`, `% of Total`.
  - Segments: je Segment eine Zeile mit `Segment`, `URLs`, `Indexable`, `Non-Indexable`, `Issues`, `Warnings`, `Opportunities`.
  - Spelling & Grammar: die häufigsten eindeutigen Fehler (max. 100, wie im Panel) mit den Spalten aus ONPAGE-158 (führend): `Error`, `Type` (Spelling/Grammar), `URLs` (Anzahl betroffener URLs), `Instances` (Anzahl Vorkommen). Derselbe Inhalt ist der Report „Spelling & Grammar Errors Summary“ (OUT-052).
  - Semantic Search: Ergebnisliste der zuletzt ausgeführten Abfrage; Spaltennamen führend INTEG-185: `Address`, `Score` (Kosinus, 0–1, ungerundet wie INTEG-181), davor die Export-Zusatzspalten `Query`, `Rank` und danach `Embedding Model` **(OS-Festlegung)**; bei Anzeige „Zentroid“ stattdessen `Address`, `Semantic Relevance Score` (INTEG-182).
- **Akzeptanzkriterien:**
  - [ ] Issues-Export enthält genau die Zeilen, die im Issues-Panel angezeigt werden (gleiche Zähler).
  - [ ] Für jedes registrierte Issue existieren nicht-leere `Description`-, `Why It Matters`- und `How To Fix`-Texte in Deutsch und Englisch (Registry-Test; Längen und Platzhalter nach UI-065).
  - [ ] Response-Times-Buckets sind halboffene Intervalle `[a, b)`; eine Antwortzeit von exakt 1,000 s fällt in 1–2 s.
  - [ ] Site Structure Tiefe: Fixture mit Start-URL, 2 Seiten in Tiefe 1 (eine davon 404) und 11 Seiten in Tiefe 11 → Zeile `1`: `2xx`=1, `4xx`=1; Zeile `10+`: `2xx`=11; Zeile `0`: 1. Summe `Total` über alle Zeilen = Anzahl interner URLs mit bekannter Tiefe.
  - [ ] Spelling & Grammar: Fehler „Beispeil“ auf 3 Seiten (insgesamt 5-mal) → eine Zeile `URLs`=3, `Instances`=5; bei mehr als 100 eindeutigen Fehlern enthält der Export genau 100 Zeilen, sortiert nach `URLs` absteigend, dann `Error` alphabetisch.
  - [ ] Semantic Search: Export ohne vorherige Abfrage ist deaktiviert; mit Mock-Embeddings und Abfrage „rote Schuhe“ stimmen Reihenfolge und Werte mit der Panel-Anzeige überein.
- **Testfixture:** `out-basic-site` mit Fixture-Server-Option `delay_ms` je Seite (100, 1000, 2500, 11000 ms); `out-site-structure` (Tiefenkette bis 11, eine 404 in Tiefe 1); `out-detail-tabs` (Rechtschreibfehler); Mock-Embedding-Endpunkt mit festen Vektoren.

### OUT-026 Export der Visualisierungen
- **Priorität:** P2
- **Beschreibung:** Crawl-Baum, Verzeichnisbaum, Kraftdiagramme und Inhalts-Cluster lassen sich als SVG/PNG und als HTML-Datei (öffnet im externen Browser) speichern.
- **Eingaben/Konfiguration:** Visualisierungsfenster → Speichern.
- **Ausgabe:** `.svg`, `.png` (Auflösung wählbar 1×/2×), `.html` (eigenständig, ohne Netzwerkzugriff lauffähig).
- **Akzeptanzkriterien:**
  - [ ] HTML-Export enthält Daten und Skripte inline und funktioniert offline.
  - [ ] Visualisierungs-Exporte sind nur lokal speicherbar (nicht nach Google Drive).
- **Testfixture:** `out-basic-site`.

### OUT-027 Export der Suchergebnisse aus „View Source“/HTML-Suche
- **Priorität:** P2
- **Beschreibung:** Treffer einer Suche über gespeichertes HTML können als Liste (`Address`, `Line`, `Match`) exportiert werden.
- **Eingaben/Konfiguration:** Suchbegriff/Regex.
- **Ausgabe:** Tabelle.
- **Akzeptanzkriterien:**
  - [ ] Suche nach `gtag\(` (Regex) auf 3 von 10 Seiten → 3 Adressen mit Zeilennummer.
- **Testfixture:** `out-detail-tabs`.

---

## 3 Bulk-Exporte

Menü „Bulk Export“. Alle Bulk-Exporte beziehen sich auf den gesamten Crawl (bzw. das aktive Segment, OUT-011), unabhängig von der aktuellen Tabellenauswahl. Kanonische Pfade für CLI/Presets: `"<Untermenü>:<Eintrag>"`, verschachtelt mit weiterem `:` (z. B. `"Response Codes:Internal:Client Error (4xx) Inlinks"`).

### OUT-030 Queued URLs
- **Priorität:** P1
- **Beschreibung:** Exportiert alle entdeckten, aber noch nicht gecrawlten URLs (Warteschlange) – typischerweise bei pausiertem oder durch Limits beendetem Crawl.
- **Eingaben/Konfiguration:** keine.
- **Ausgabe:** Spalten `Address`, `Crawl Depth` (geplante Tiefe), `Found On` (erste Quell-URL), `Link Type`.
- **Akzeptanzkriterien:**
  - [ ] Crawl mit Limit „max. 5 URLs“ auf einer Seite mit 20 Links → Export enthält die übrigen entdeckten, nicht gecrawlten internen URLs, keine bereits gecrawlte URL.
  - [ ] Nach vollständigem Crawl ist der Export leer (nur Kopfzeile).
  - [ ] URLs, die durch Exclude/robots.txt nie eingereiht werden, erscheinen nicht.
- **Testfixture:** `out-queue`: Startseite mit 20 internen Links, Crawl-Limit 5.

### OUT-031 Links: All Inlinks (Standard-Linkformat)
- **Priorität:** P0
- **Beschreibung:** Jeder Verweis im Crawl (Hyperlinks und Ressourcen-Referenzen, intern und extern) als eine Zeile. Das Spaltenformat ist das Standard-Linkformat aller Inlink-/Outlink-Exporte dieses Dokuments.
- **Eingaben/Konfiguration:** Welche Linktypen gespeichert werden, steuert die Crawl-Konfiguration (Bilder, CSS, JS, Canonicals usw. „speichern“).
- **Ausgabe (Standard-Linkformat):**
  `Type`, `Source`, `Destination`, `Size (Bytes)`, `Alt Text`, `Anchor`, `Status Code`, `Status`, `Follow`, `Target`, `Rel`, `Path Type`, `Link Path`, `Link Position`, `Link Origin`.
  - `Type` ∈ {`Hyperlink`, `Image`, `CSS`, `JavaScript`, `Media`, `Iframe`, `Canonical`, `HTTP Canonical`, `Rel Next`, `Rel Prev`, `Hreflang`, `HTTP Hreflang`, `AMP`, `Mobile Alternate`, `Meta Refresh`, `HTTP Refresh`, `HTTP Redirect`, `HSTS Policy`, `JavaScript Redirect`, `XML Sitemap`, `PDF Link`}.
  - `Status` = Statustext bzw. Ergebnis (`OK`, `Not Found`, `Moved Permanently`, `HSTS Policy`, `DNS Lookup Failed`, `Connection Timeout`, `Blocked by Robots.txt`, `Not Crawled` …).
  - `Rel` = alle `rel`-Werte des Elements in Originalreihenfolge, kleingeschrieben, mit einem Leerzeichen getrennt; `Target` = Wert des `target`-Attributs oder leer.
  - `Size (Bytes)` eines bekannten, noch nicht gecrawlten internen Ziels ist im Linkexport 0 (K2-96); unbekannte externe Größen bleiben leer.
  - `Alt Text` nur bei `Type`=Image (bzw. verlinktem Bild bei Hyperlink); sonst leer.
  - `Follow` = `True`/`False` (False bei `rel=nofollow`/`ugc`/`sponsored` am Link oder Meta/X-Robots nofollow der Quelle).
  - `Path Type` ∈ {`Absolute`, `Protocol-Relative`, `Root-Relative`, `Path-Relative`}.
  - `Link Path` = XPath des Link-Elements im (gerenderten bzw. rohen) DOM.
  - `Link Position` ∈ {`Head`, `Navigation`, `Header`, `Content`, `Sidebar`, `Aside`, `Footer`} oder benutzerdefinierte Position. `Head` gilt für jedes Element im `<head>` (`<link>` jeder Art, `<meta http-equiv="refresh">`, `<script src>`, `<style>`-Importe). Das betrifft auch die Zeilen aus Canonical, Hreflang, Rel Next/Prev, Meta Refresh, CSS und JavaScript im Kopf (Paritätsbefund M1 vom 14.10.2026, Befund N-09, K-63).
  - `Link Path` für Elemente im `<head>`: Pfad in der Form der Referenz, `//head/<element>[n]` (n = Position unter den gleichnamigen Kindelementen von `<head>`, z. B. `//head/link[3]`, `//head/meta[2]`; die genaue Form legt M1-P31 per `os-parity` fest). Links aus HTTP-Headern (HTTP Canonical, HTTP Hreflang, HTTP Refresh, HTTP Redirect) haben weiterhin leere `Link Path`/`Link Position`.
  - `Link Origin` ∈ {`HTML`, `Rendered HTML`, `HTML & Rendered HTML`}.
- **Akzeptanzkriterien:**
  - [ ] Seite mit `<a href="/x" rel="nofollow" target="_blank">Text</a>` → Zeile mit `Type`=Hyperlink, `Anchor`=Text, `Follow`=False, `Target`=_blank, `Rel`=nofollow, `Path Type`=Root-Relative.
  - [ ] `<img src="pic.jpg" alt="Hund">` → `Type`=Image, `Alt Text`=Hund, `Path Type`=Path-Relative, `Size (Bytes)` = Größe des Bildes (wenn gecrawlt).
  - [ ] Zwei identische Links auf derselben Seite → zwei Zeilen (keine Deduplizierung).
  - [ ] `Status Code` ist leer, wenn das Ziel nicht gecrawlt wurde (z. B. externe Links bei deaktivierter Prüfung); `Status` dann `Not Crawled` **(OS-Festlegung)**.
  - [ ] `Destination` = Adresse der Ziel-URL (aufgelöst, wie `Address` der Ziel-Zeile); nur für Links ohne auflösbares Ziel (z. B. `mailto:`, `tel:`, ungültige URL) steht der Rohwert des Attributs. [P1] Solche Links werden erst mit CRAWL-053 („nicht crawlbare Links speichern“) gespeichert; bis dahin verwirft die Engine sie (K-35), und der Fall ist nur auf Bibliotheksebene geprüft (OS-Festlegung 14.10.2026, Welle-3-Sichtung, K-42). Die Crawl-Datei hält je Link den Rohwert wie im Markup (`links.target_raw`) und die Kennung der Ziel-URL; jeder gespeicherte Link mit auflösbarem Ziel hat eine Ziel-URL-Zeile (OS-Festlegung 13.10.2026, Welle-2-Sichtung, K-37).
  - [ ] Bei JS-Rendering: Link nur im gerenderten DOM → `Link Origin`=Rendered HTML; in beiden → `HTML & Rendered HTML`.
  - [ ] Link im `<nav>` → `Link Position`=Navigation; im `<footer>` → Footer; im `<main>` → Content; `<link rel="canonical">` und `<link rel="stylesheet">` im `<head>` → Head mit Pfad `//head/link[n]` (K-63).
  - [ ] 1 Mio. Links werden in < 60 s als CSV geschrieben (Referenz-Hardware: Apple M1, SSD).
  - [ ] Kopfzeile ist exakt `Type,Source,Destination,Size (Bytes),Alt Text,Anchor,Status Code,Status,Follow,Target,Rel,Path Type,Link Path,Link Position,Link Origin` (Golden-Vergleich).
  - [ ] Typzuordnung je Element (je eine Zeile, Golden): `<link rel="canonical">` → Canonical; `Link: <…>; rel="canonical"` (Header) → HTTP Canonical; `<link rel="next">`/`prev` → Rel Next/Rel Prev; `<link rel="alternate" hreflang>` → Hreflang; hreflang im Header → HTTP Hreflang; `<link rel="amphtml">` → AMP; `<link rel="alternate" media="only screen…">` → Mobile Alternate; `<iframe src>` → Iframe; `<video>/<audio>/<source>` → Media; `<link rel="stylesheet">` → CSS; `<script src>` → JavaScript; `<meta http-equiv="refresh">` → Meta Refresh; Antwortheader `Refresh: 0; url=/x` → HTTP Refresh; 3xx-`Location` → HTTP Redirect; Link in PDF-Datei → PDF Link.
  - [ ] `<a href="/x" rel="Nofollow  UGC">` → `Rel` = `nofollow ugc`, `Follow`=False.
  - [ ] `Link Path` ist ein absoluter XPath mit Positionsindizes, z. B. `/html[1]/body[1]/main[1]/p[2]/a[1]` (jeder Schritt mit `[n]`, siehe ONPAGE-172); derselbe Link liefert bei wiederholtem Crawl denselben Pfad.
  - [ ] Sortierung: nach `Destination`, dann `Source`, dann Dokumentreihenfolge **(OS-Festlegung)**.
- **Testfixture:** `out-links`: Seiten mit allen Linktypen (inkl. Canonical-Header, hreflang-Header, `Refresh`-Header, iframe, video, PDF mit Link), Pfadtypen, nofollow/ugc/sponsored (auch gemischte Groß-/Kleinschreibung), `target`, Links in nav/header/main/aside/footer, ein nur per JS eingefügter Link.

### OUT-032 Links: All Outlinks
- **Priorität:** P0
- **Beschreibung:** Inhaltlich gleiche Menge wie OUT-031, aber aus Sicht der Quellseiten und mit Fokus auf Quellen innerhalb des Crawls; sortiert nach `Source`, dann Linkreihenfolge im Dokument.
- **Eingaben/Konfiguration:** wie OUT-031.
- **Ausgabe:** Standard-Linkformat (OUT-031).
- **Akzeptanzkriterien:**
  - [ ] Für jede gecrawlte interne HTML-Seite stimmen die Zeilen mit `Source` = Seite mit dem Outlinks-Detailtab dieser Seite überein.
  - [ ] Reihenfolge innerhalb einer Quelle = Reihenfolge im Quelltext.
- **Testfixture:** `out-links`.

### OUT-033 Links: All Anchor Text
- **Priorität:** P1
- **Beschreibung:** Alle Hyperlinks mit ihrem Ankertext (bzw. Alt-Text verlinkter Bilder) für Ankertext-Analysen.
- **Eingaben/Konfiguration:** keine.
- **Ausgabe:** `Source`, `Destination`, `Anchor`, `Alt Text`, `Link Position`, `Follow`, `Status Code` **(OS-Festlegung** der Spaltenauswahl).
- **Akzeptanzkriterien:**
  - [ ] Nur `Type`=Hyperlink (keine Ressourcen).
  - [ ] Verlinktes Bild ohne Text → `Anchor` leer, `Alt Text` gefüllt.
  - [ ] Ankertext ist whitespace-normalisiert (mehrfache Leerzeichen/Zeilenumbrüche → ein Leerzeichen, getrimmt).
- **Testfixture:** `out-links` (Link mit `<a>\n  Mehr   lesen\n</a>`, verlinktes Bild).

### OUT-034 Links: External Links
- **Priorität:** P1
- **Beschreibung:** Alle Verweise von internen Seiten auf externe Ziele.
- **Ausgabe:** Standard-Linkformat.
- **Eingaben/Konfiguration:** Definition „intern“ gemäß Crawl-Konfiguration (Subdomains, Startordner).
- **Akzeptanzkriterien:**
  - [ ] Bei deaktivierter Option „alle Subdomains crawlen“ gilt `blog.example.test` als extern und erscheint hier.
  - [ ] Externe Bilder/Skripte (CDN) erscheinen mit ihrem `Type`.
- **Testfixture:** `out-links` mit Hosts `example.test`, `blog.example.test`, `cdn.other.test` (über Host-Mapping im Fixture-Server).

### OUT-035 Links: Internal Nofollow Outlinks
- **Priorität:** P1
- **Beschreibung:** Interne Hyperlinks mit `Follow`=False.
- **Ausgabe:** Standard-Linkformat.
- **Akzeptanzkriterien:**
  - [ ] Enthält Links mit `rel` `nofollow`, `ugc`, `sponsored` (einzeln oder kombiniert) sowie Links von Seiten mit Meta-Robots `nofollow`.
  - [ ] Externe nofollow-Links sind nicht enthalten.
- **Testfixture:** `out-links`.

### OUT-036 Links: Internal Outlinks With No Anchor Text
- **Priorität:** P1
- **Beschreibung:** Interne Hyperlinks ohne zugänglichen Ankertext.
- **Ausgabe:** Standard-Linkformat.
- **Akzeptanzkriterien:**
  - [ ] Enthalten: `<a href="/x"></a>`, `<a href="/x">   </a>`, `<a href="/x"><img src="i.png"></a>` (Bild ohne alt).
  - [ ] Nicht enthalten: `<a href="/x"><img src="i.png" alt="Logo"></a>` und `<a href="/x" aria-label="Start"></a>` **(OS-Festlegung:** aria-label zählt als Text).
- **Testfixture:** `out-links-anchor`.

### OUT-037 Links: Non-Descriptive Anchor Text In Internal Outlinks
- **Priorität:** P1
- **Beschreibung:** Interne Hyperlinks, deren Ankertext (case-insensitiv, getrimmt, Satzzeichen am Ende ignoriert) in einer konfigurierbaren Liste nichtssagender Phrasen steht.
- **Eingaben/Konfiguration:** Liste in Konfiguration → Links (Standard, englisch + deutsch **(OS-Festlegung)**: „click here“, „here“, „read more“, „more“, „learn more“, „this“, „link“, „continue“, „go“, „start“, „hier“, „hier klicken“, „mehr“, „weiterlesen“, „mehr erfahren“, „weiter“).
- **Ausgabe:** Standard-Linkformat.
- **Akzeptanzkriterien:**
  - [ ] `Click Here!` → enthalten; `Click here for pricing` → nicht enthalten (exakter Phrasenvergleich, nicht Teilstring).
  - [ ] Nach Entfernen von „more“ aus der Liste ist `More` nicht mehr enthalten.
- **Testfixture:** `out-links-anchor`.

### OUT-038 Links: Follow & Nofollow Internal Inlinks To Page
- **Priorität:** P1
- **Beschreibung:** Alle internen Inlinks zu Seiten, die sowohl follow- als auch nofollow-Inlinks erhalten (inkonsistente Verlinkung).
- **Ausgabe:** Standard-Linkformat (alle Inlinks dieser Zielseiten, beide Arten).
- **Akzeptanzkriterien:**
  - [ ] Ziel `/p` mit 2 follow- und 1 nofollow-Link → 3 Zeilen; Ziel `/q` nur mit follow-Links → keine Zeile.
- **Testfixture:** `out-links-follow`.

### OUT-039 Links: Internal Nofollow Inlinks Only
- **Priorität:** P1
- **Beschreibung:** Inlinks zu Seiten, die ausschließlich über nofollow-Links intern verlinkt sind.
- **Ausgabe:** Standard-Linkformat.
- **Akzeptanzkriterien:**
  - [ ] Ziel `/r` mit nur 2 nofollow-Inlinks → 2 Zeilen; `/p` aus OUT-038 → keine Zeile.
  - [ ] Canonical/hreflang-Verweise zählen nicht als follow-Inlink.
- **Testfixture:** `out-links-follow`.

### OUT-040 Links: Outlinks to Localhost
- **Priorität:** P1
- **Beschreibung:** Verweise auf lokale Adressen, die auf Live-Seiten nicht vorkommen sollten.
- **Ausgabe:** Standard-Linkformat.
- **Akzeptanzkriterien:**
  - [ ] Enthalten: Ziele mit Host `localhost`, `127.0.0.0/8`, `[::1]`, `0.0.0.0`, Hosts mit Endung `.localhost`.
  - [ ] Nicht enthalten: private Netze `192.168.x.x`/`10.x.x.x` **(OS-Festlegung;** ein eigener Filter für private Netze ist nicht Teil dieses Eintrags).
  - [ ] Wenn die Fixture selbst auf 127.0.0.1 läuft, werden Links auf den **eigenen** Crawl-Host nicht gemeldet (sonst wäre jede Fixture positiv).
- **Testfixture:** `out-links` gemappt auf Host `site.test`, mit Links auf `http://localhost:3000/x` und `http://127.0.0.1/y`.

### OUT-041 Links: Pages With Uncrawlable Internal Outlinks
- **Priorität:** P1
- **Beschreibung:** Exportiert „Links“, die für Suchmaschinen nicht als normale Links crawlbar sind (Referenzstand), sofern das Speichern solcher Links aktiviert ist.
- **Eingaben/Konfiguration:** Konfiguration → Crawl → „Nicht crawlbare Links speichern“.
- **Ausgabe:** Standard-Linkformat + Spalte `Link Crawlability` mit Grund: `href on non-anchor element`, `onclick navigation`, `javascript: URL`, `missing href`, `empty href`, `fragment only` **(OS-Festlegung** der Werte).
- **Akzeptanzkriterien:**
  - [ ] `<span href="/a">`, `<div onclick="location.href='/b'">`, `<a href="javascript:go('/c')">`, `<a onclick="...">` ohne href werden je eine Zeile.
  - [ ] Normale `<a href="/d">` erscheinen nicht.
  - [ ] Ohne aktivierte Option ist der Export leer und im Menü mit Hinweis versehen.
- **Testfixture:** `out-uncrawlable-links`.

### OUT-042 Path Type: Absolute / Protocol-Relative / Root-Relative / Path-Relative Links
- **Priorität:** P2
- **Beschreibung:** Vier Exporte, je einer pro Pfadtyp der `href`-Notation, mit Quellseiten.
- **Ausgabe:** Standard-Linkformat, gefiltert nach `Path Type`.
- **Akzeptanzkriterien:**
  - [ ] `https://x.test/a` → Absolute; `//x.test/a` → Protocol-Relative; `/a` → Root-Relative; `a`, `./a`, `../a`, `?q=1` → Path-Relative.
  - [ ] Summe der vier Exporte = Anzahl aller Zeilen in All Outlinks mit nicht-leerem `href`-Attribut.
- **Testfixture:** `out-links`.

### OUT-043 Web: Screenshots
- **Priorität:** P1
- **Beschreibung:** Speichert alle beim JS-Rendering erzeugten Seiten-Screenshots als Dateien.
- **Eingaben/Konfiguration:** Rendering = JavaScript, „Screenshots speichern“ aktiv; Fenstergröße gemäß Rendering-Konfiguration.
- **Ausgabe:** Ordner `screenshots/` mit PNG je URL; Dateiname = bereinigte URL (OUT-008-Regeln) + `_<8-stelliger Hash>.png`; zusätzlich `screenshots/index.csv` mit `Address`, `File`.
- **Akzeptanzkriterien:**
  - [ ] Anzahl PNG-Dateien = Anzahl gerenderter Seiten mit gespeichertem Screenshot.
  - [ ] Ohne Rendering ist der Eintrag deaktiviert mit Hinweis.
  - [ ] Nur lokal speicherbar (kein Drive).
- **Testfixture:** `out-render`: 3 Seiten mit JS-generiertem Inhalt.

### OUT-044 Web: Archived Website
- **Priorität:** P2
- **Beschreibung:** Speichert die gerenderte Website als Offline-Kopie (Ordnerstruktur, umgeschriebene Links) und optional im WARC-Format für gängige WARC-Betrachter.
- **Eingaben/Konfiguration:** Rendering-Konfiguration „Website archivieren“; Format: Ordner / WARC / beides.
- **Ausgabe:** Ordner `archive/<host>/<pfad>/index.html` + Ressourcen; bzw. `archive.warc.gz` (WARC 1.1).
- **Akzeptanzkriterien:**
  - [ ] Offline-Kopie lässt sich per `file://` öffnen; interne Links zeigen auf lokale Dateien.
  - [ ] WARC enthält je URL einen `request`- und einen `response`-Record; Validierung mit einem WARC-Validator (z. B. `warcio check`) ohne Fehler.
- **Testfixture:** `out-render`.

### OUT-045 Web: All Page Source
- **Priorität:** P1
- **Beschreibung:** Speichert den HTML-Quelltext aller internen HTML-Seiten – wahlweise roh (wie empfangen) oder gerendert.
- **Eingaben/Konfiguration:** „HTML speichern“ bzw. „gerendertes HTML speichern“ in der Extraktions-Konfiguration; Auswahl Original/Rendered beim Export.
- **Ausgabe:** Ordner `page_source/` mit einer `.html` pro URL + `index.csv` (`Address`, `File`, `Source Type`).
- **Akzeptanzkriterien:**
  - [ ] Rohes HTML ist byte-identisch zum (dekomprimierten) Antwortkörper.
  - [ ] Ist nur rohes HTML gespeichert, ist die Option „Rendered“ deaktiviert.
- **Testfixture:** `out-render`.

### OUT-046 Web: All Page Text
- **Priorität:** P1
- **Beschreibung:** Extrahierter sichtbarer Text aller internen HTML-Seiten mit Status 200 als Textdateien.
- **Eingaben/Konfiguration:** „HTML speichern“ aktiv; Inhaltsbereich-Konfiguration (Ein-/Ausschluss von Elementen) wird beachtet.
- **Ausgabe:** Ordner `page_text/` mit `.txt` (UTF-8) pro URL + `index.csv`.
- **Akzeptanzkriterien:**
  - [ ] Seiten mit Status ≠ 200 und Nicht-HTML werden nicht exportiert.
  - [ ] Text enthält keinen Inhalt aus `<script>`, `<style>`, `<noscript>`-Tags **(OS-Festlegung** für noscript) und keine HTML-Tags.
  - [ ] Ist `<nav>` per Inhaltsbereich ausgeschlossen, fehlt der Navigationstext.
- **Testfixture:** `out-render` + Seite mit nav/footer.

### OUT-047 Web: All PDF Documents / All PDF Content
- **Priorität:** P2
- **Beschreibung:** Speichert alle gecrawlten PDF-Dateien bzw. deren extrahierten Text.
- **Eingaben/Konfiguration:** PDFs speichern / PDF-Inhalt extrahieren.
- **Ausgabe:** `pdf_documents/*.pdf` bzw. `pdf_content/*.txt`, jeweils mit `index.csv`.
- **Akzeptanzkriterien:**
  - [ ] PDF-Dateien sind byte-identisch zum Download.
  - [ ] Passwortgeschützte/defekte PDFs werden übersprungen und im `index.csv` mit `Error` vermerkt.
- **Testfixture:** `out-pdf`: 2 gültige PDFs, 1 defektes PDF.

### OUT-048 Web: All HTTP Request Headers / All HTTP Response Headers
- **Priorität:** P1
- **Beschreibung:** Alle gesendeten Request- bzw. empfangenen Response-Header je URL.
- **Eingaben/Konfiguration:** Extraktion „HTTP-Header speichern“.
- **Ausgabe:** Langformat `Address`, `Header Name`, `Header Value` (eine Zeile je Header; mehrfach vorkommende Header wie `Set-Cookie` als mehrere Zeilen).
- **Akzeptanzkriterien:**
  - [ ] Header-Namen in Originalschreibweise (HTTP/1.1) bzw. Kleinbuchstaben (HTTP/2) wie empfangen.
  - [ ] Request-Header mit Zugangsdaten (`Authorization`, `Proxy-Authorization`, `Cookie`) werden als `<redacted>` ausgegeben, außer Option „Zugangsdaten in Exporten zeigen“ ist aktiv **(OS-Festlegung)**.
- **Testfixture:** `out-headers`: Seiten mit mehreren `Set-Cookie`, `Link`-Header, Basic-Auth-Bereich.

### OUT-049 Web: All Cookies
- **Priorität:** P1
- **Beschreibung:** Alle gesetzten Cookies je URL (aus HTTP-Headern und – bei Rendering – per JavaScript gesetzt).
- **Eingaben/Konfiguration:** Extraktion „Cookies speichern“.
- **Ausgabe:** Spalten und Wertelisten führend UI-043, in dieser Reihenfolge: `Address`, `Cookie Name`, `Cookie Value`, `Domain`, `Path`, `Expires`, `Max-Age`, `Secure`, `HttpOnly`, `SameSite`, `Type` (`HTTP`/`JavaScript`), `Session` (`True` wenn weder Expires noch Max-Age), `First-Party` (`True`/`False`). Alle Spalten erscheinen unabhängig von ihrer Sichtbarkeit im Detail-Tab.
- **Akzeptanzkriterien:**
  - [ ] `Set-Cookie: a=1; Secure; HttpOnly; SameSite=Lax` → eine Zeile mit `Secure`=True, `HttpOnly`=True, `SameSite`=Lax, `Session`=True.
  - [ ] Per `document.cookie` gesetztes Cookie erscheint nur mit Rendering, `Type`=JavaScript.
  - [ ] Kopfzeile ist exakt `Address,Cookie Name,Cookie Value,Domain,Path,Expires,Max-Age,Secure,HttpOnly,SameSite,Type,Session,First-Party` (identisch mit dem Cookies-Detail-Export OUT-021); ein Cookie aus einem Drittanbieter-iframe hat `First-Party`=False.
- **Testfixture:** `out-headers`.

### OUT-050 Response Codes: Inlinks-Exporte je Filter
- **Priorität:** P0
- **Beschreibung:** Für die Bereiche **Internal & External**, **Internal** und **External** je ein Export pro Response-Code-Filter mit allen Verweisen auf URLs dieses Filters.
- **Eingaben/Konfiguration:** keine.
- **Ausgabe:** Standard-Linkformat. Einträge je Bereich:
  - `All` (alle Inlinks), `Blocked by Robots.txt Inlinks`, `Blocked Resource Inlinks`, `No Response Inlinks`, `Success (2xx) Inlinks`, `Redirection (3xx) Inlinks`, `Redirection (JavaScript) Inlinks`, `Redirection (Meta Refresh) Inlinks`, `Redirection (HTTP Refresh) Inlinks` (Filter im Referenzstand), `Client Error (4xx) Inlinks`, `Server Error (5xx) Inlinks`, `All Error Inlinks` (Referenzstand, Details OUT-067).
- **Akzeptanzkriterien:**
  - [ ] `Internal:Client Error (4xx) Inlinks` enthält genau alle Verweise mit internem Ziel, dessen Status 400–499 ist; Anzahl Ziele = Zähler „Client Error (4xx)“ im Tab Internal-Bereich.
  - [ ] `No Response Inlinks` umfasst Ziele mit DNS-Fehler, Verbindungsabbau, Timeout, ungültiger URL.
  - [ ] `Redirection (HTTP Refresh) Inlinks` enthält Verweise auf Ziele, die mit Status 200 und Antwortheader `Refresh: <n>; url=<ziel>` antworten; diese Ziele erscheinen **nicht** in `Redirection (Meta Refresh) Inlinks` (und umgekehrt: `<meta http-equiv="refresh">` nur in Meta Refresh).
  - [ ] `Success (2xx) Inlinks` enthält die HTTP-Refresh-Ziele **nicht**, obwohl ihr Statuscode 200 ist (Refresh-Klassifikation hat Vorrang, analog Meta Refresh) **(OS-Festlegung)**.
  - [ ] Quelle eines Verweises kann selbst eine Weiterleitung sein (`Type`=HTTP Redirect), wenn die 404 nur über eine Weiterleitung erreicht wurde.
  - [ ] Jeder der 12 Einträge × 3 Bereiche ist per CLI `--bulk-export` ansprechbar (z. B. `"Response Codes:Internal:Redirection (HTTP Refresh) Inlinks"`).
- **Testfixture:** `out-response-codes`: Ziele mit 200, 301→404, 302, Meta-Refresh, `Refresh`-Header (`Refresh: 0; url=/refresh-target`), JS-Redirect, 403, 404, 410, 500, 503, Timeout (Fixture verzögert 60 s), DNS-Fehler (`nonexistent.invalid`), robots-gesperrte URL, extern 404, extern 500.

### OUT-051 Security: Exporte je Filter
- **Priorität:** P1
- **Beschreibung:** Für jeden Security-Filter, bei dem ein Verweis die Ursache ist, ein Export der betroffenen Verweise.
- **Ausgabe:** Standard-Linkformat. Einträge: `HTTP URLs Inlinks`, `Mixed Content` (Quelle = HTTPS-Seite, Ziel = HTTP-Ressource), `Form URL Insecure` (Formular-`action` auf HTTP), `Form On HTTP URL`, `Unsafe Cross-Origin Links` (externe Links mit `target=_blank` ohne `noopener`/`noreferrer`), `Protocol-Relative Resource Links`.
- **Akzeptanzkriterien:**
  - [ ] HTTPS-Seite mit `<img src="http://…">` → Zeile in Mixed Content, `Type`=Image.
  - [ ] `<a href="https://ext.test" target="_blank">` ohne rel → Unsafe Cross-Origin Links; mit `rel="noopener"` nicht.
  - [ ] `<form action="http://…/login">` auf HTTPS-Seite → Form URL Insecure.
- **Testfixture:** `out-security` (Fixture-Server mit TLS über selbstsigniertes Zertifikat, Vertrauen per Test-Konfiguration).

### OUT-052 Content: Duplikat-, Ähnlichkeits- und Rechtschreib-Exporte
- **Priorität:** P1
- **Beschreibung:** Paarweise bzw. detaillierte Exporte der Content-Filter.
- **Ausgabe:** Spaltennamen bestimmt der jeweils führende Fach-Eintrag (in Klammern); dieser Export übernimmt sie exakt. Die Zeilenauswahl je Export steht in ONPAGE-165.
  - `Exact Duplicates` (ONPAGE-165): `Address`, `Exact Duplicate Address`, `Hash`.
  - `Near Duplicates` (ONPAGE-145): `Address`, `Near Duplicate Address`, `Similarity` (%), nur Paare ≥ eingestellter Schwelle (Standard 90 %).
  - `Semantically Similar` (INTEG-181): `Address`, `Closest Semantically Similar Address`, `Semantic Similarity Score` (0–1, ungerundet); je URL des Filters eine Zeile je Partner ≥ Schwelle, sodass die nächste Adresse immer enthalten ist (Referenz-Anleitung: nächste Adresse plus alle Partner über der Schwelle); die Spalte heißt in jeder Zeile `Closest Semantically Similar Address`, sortiert je URL nach Score absteigend **(OS-Festlegung)**.
  - `Low Relevance Content` (INTEG-182): `Address`, `Semantic Relevance Score`.
  - `Spelling & Grammar Errors` (ONPAGE-157): `Address`, `Language`, `Type`, `Error`, `Detail`, `Suggestion`, `Page Section`, danach die Erweiterung `Rule ID` (nur Grammatik). `Spelling Errors` und `Grammar Errors` sind dieselbe Tabelle, gefiltert auf `Type` = Spelling bzw. Grammar.
  - `Spelling & Grammar Errors Summary` (ONPAGE-158): `Error`, `Type`, `URLs`, `Instances` (höchstens 100 Zeilen, Inhalt wie OUT-025).
  - `Soft 404 Inlinks` (ONPAGE-165/ONPAGE-147): Inlinks-Export zu Filter „Soft 404 Pages“ im Format aus OUT-031.
  - `All Page Text`: siehe OUT-046.
  - Hinweis zur Benennung: Die öffentliche Tutorial-Doku der Referenz beschreibt einen kombinierten Export; der Spec-Review gegen den Referenzstand nennt getrennte Einträge für Rechtschreib- und Grammatikfehler (vor Umsetzung per Golden-Export der Free-Version bestätigen, Anhang B). OpenSpider registriert alle drei kanonischen Pfade `Content:Spelling Errors`, `Content:Grammar Errors`, `Content:Spelling & Grammar Errors`, damit Skripte beider Varianten funktionieren.
- **Akzeptanzkriterien:**
  - [ ] Zwei Seiten mit identischem Inhaltsbereich → je Seite eine Zeile (Paar symmetrisch in beide Richtungen).
  - [ ] Bei Schwelle 90 % erscheinen Paare mit 89,9 % nicht, mit 90,0 % schon.
  - [ ] Exporte erfordern abgeschlossene Crawl-Analyse; sonst Hinweis „Crawl-Analyse ausführen“ und leerer Export.
  - [ ] Seite mit 2 Rechtschreib- und 1 Grammatikfehler → `Spelling Errors` 2 Zeilen, `Grammar Errors` 1 Zeile, `Spelling & Grammar Errors` 3 Zeilen; Zeilenzahl kombiniert = Summe der beiden Einzelexporte (Invariante, Registry-Test).
  - [ ] `--bulk-export "Content:Spelling Errors,Content:Grammar Errors"` erzeugt `content_spelling_errors.csv` und `content_grammar_errors.csv`.
  - [ ] Fehler im Title werden mit `Page Section`=`Page Title` ausgegeben, Fehler im Fließtext mit `Page Section`=`Body`.
  - [ ] Semantically Similar: URL A mit Partnern B (0,970) und C (0,965) über der Schwelle 0,95 → zwei Zeilen für A in dieser Reihenfolge; eine URL ohne Partner ≥ Schwelle erscheint nicht.
  - [ ] Soft 404 Inlinks: Fixture `onpage-content/` (Fachspec 02) → genau 4 Zeilen (Kriterium aus ONPAGE-165).
- **Testfixture:** `out-content-dupes`: 2 exakte Duplikate, 2 Seiten mit ~92 % Überschneidung, 1 Seite mit ~85 %; `out-detail-tabs`: Seite mit „Beispeil“ im Title und „Fehlerr“ im Text sowie einem Grammatikfehler („die Haus“).

### OUT-053 Images: Exporte je Filter
- **Priorität:** P1
- **Beschreibung:** Verweise auf Bilder der jeweiligen Image-Filter (Quelle = einbindende Seite).
- **Ausgabe:** Instanzspalten führend ONPAGE-181/ONPAGE-172 (Kopfzeile genau `From,To,Alt Text,Size,Real Dimensions (WxH),Dimensions in Attributes (WxH),Display Dimensions (WxH),Potential Savings,Path Type,Link Path,Link Position,Link Origin`; Werte wie ONPAGE-172), eine Zeile je Bildinstanz; dieser Eintrag legt Exportnamen, Dateinamen und Mechanik fest. Einträge: `All Image Inlinks`, `Images Missing Alt Text Inlinks`, `Images Missing Alt Attribute Inlinks`, `Images With Alt Text Over X Characters Inlinks` (X aus Präferenzen, Standard 100), `Images Over X KB Inlinks` (X Standard 100), `Images Missing Size Attributes Inlinks`, `Incorrectly Sized Images Inlinks`, `Background Images`.
- **Akzeptanzkriterien:**
  - [ ] `<img src="a.png" alt="">` → in „Missing Alt Text“, nicht in „Missing Alt Attribute“; `<img src="b.png">` → nur in „Missing Alt Attribute“ (beide Filter sind disjunkt).
  - [ ] Alt-Text mit 101 Zeichen → enthalten; mit 100 Zeichen → nicht.
  - [ ] Bild 100 KB (102 400 Bytes) → nicht enthalten; 102 401 Bytes → enthalten (Grenze: „über“).
  - [ ] Der Name im Menü zeigt den aktuell konfigurierten Wert statt „X“; CLI akzeptiert beide Schreibweisen.
- **Testfixture:** `out-images`: Bilder mit/ohne alt, langer alt, 100 KB und 100 KB + 1 Byte, ohne width/height, CSS-Hintergrundbild.

### OUT-054 Canonicals: Exporte je Filter
- **Priorität:** P1
- **Beschreibung:** Je Canonicals-Filter ein Inlinks-Export auf die betroffenen URLs.
- **Ausgabe:** Standard-Linkformat. Einträge: `Contains Canonical Inlinks`, `Self Referencing Inlinks`, `Canonicalised Inlinks`, `Missing Inlinks`, `Multiple Inlinks`, `Multiple Conflicting Inlinks`, `Non-Indexable Canonical Inlinks`, `Canonical Is Relative Inlinks`, `Unlinked Inlinks`, `Invalid Attribute In Annotation Inlinks`, `Outside <head> Inlinks`.
- **Akzeptanzkriterien:**
  - [ ] Seite `/a` mit Canonical auf `/b`, verlinkt von `/home` und `/x` → `Canonicalised Inlinks` enthält 2 Zeilen mit `Destination`=`/a`.
  - [ ] Für jeden Filter des Canonicals-Tabs (Spec-Bereich Indexierung) existiert genau ein Export; Registry-Test schlägt fehl, wenn ein Filter ohne Export hinzukommt.
- **Testfixture:** `out-canonicals`: je eine Seite pro Filterfall.

### OUT-055 Directives: Exporte je Filter
- **Priorität:** P1
- **Beschreibung:** Je Directives-Filter ein Inlinks-Export.
- **Ausgabe:** Standard-Linkformat. Einträge (je `… Inlinks`): `Index`, `Noindex`, `Follow`, `Nofollow`, `None`, `NoArchive`, `NoSnippet`, `Max-Snippet`, `Max-Image-Preview`, `Max-Video-Preview`, `NoODP`, `NoYDIR`, `NoImageIndex`, `NoTranslate`, `Unavailable_After`, `Refresh`, `Outside <head>`.
- **Akzeptanzkriterien:**
  - [ ] Seite mit `X-Robots-Tag: noindex` (Header) erscheint im Noindex-Ziel genauso wie mit Meta-Tag.
  - [ ] Registry-Test: jeder Directives-Filter hat einen Export.
- **Testfixture:** `out-directives`.

### OUT-056 Structured Data: Exporte (RDF-Tripel)
- **Priorität:** P1
- **Beschreibung:** Strukturierte Daten aller URLs eines Filters als Tripel (Subjekt–Prädikat–Objekt), formatübergreifend einheitlich (JSON-LD, Microdata, RDFa).
- **Ausgabe:** `Address`, `Format`, `Subject`, `Predicate`, `Object`, `Validation Type`, `Severity`, `Issue` (die letzten drei nur bei Validierungsfunden). Einträge: `Contains Structured Data`, `Validation Errors`, `Validation Warnings`, `Parse Errors`, `Microdata URLs`, `JSON-LD URLs`, `RDFa URLs`, `Rich Result Feature Detected` **(OS-Festlegung** zur Benennung des letzten).
- **Akzeptanzkriterien:**
  - [ ] JSON-LD `{"@type":"Product","name":"X","offers":{"@type":"Offer","price":"5"}}` ergibt mindestens die Tripel `_:b0 rdf:type schema:Product`, `_:b0 schema:name "X"`, `_:b0 schema:offers _:b1`, `_:b1 schema:price "5"`. Blank-Node-IDs sind je URL deterministisch.
  - [ ] Parse-Error-URL (ungültiges JSON) erscheint im Export `Parse Errors` mit `Issue` = Parser-Meldung und Zeile/Spalte.
- **Testfixture:** `out-structured-data`: gültiges Product, Product ohne Pflichtfeld, kaputtes JSON-LD, Microdata-Breadcrumb, RDFa-Organisation.

### OUT-057 Sitemaps: Exporte je Filter
- **Priorität:** P1
- **Beschreibung:** Für Filter des Sitemaps-Tabs: welche URL steht in welcher Sitemap.
- **Ausgabe:** `Sitemap`, `Address`, `Status Code`, `Indexability`, `Indexability Status`; bei `Orphan URLs` zusätzlich `Found Via` (Spaltenname und Bedeutung führend TECH-214). Einträge: `URLs In Sitemap`, `URLs Not In Sitemap` (nur `Address`), `Non-Indexable URLs In Sitemap`, `URLs In Multiple Sitemaps`, `XML Sitemap With Over 50k URLs`, `XML Sitemap Over 50MB`, `Orphan URLs`. Zeilenauswahl: TECH-219. **OS-Festlegung (R-16):** Die Exportnamen tragen keinen Zusatz „Inlinks“, die URL-Spalte heißt `Address` (nicht `URL`); die früheren Namen aus TECH-219 („… Inlinks“) werden von CLI und Presets als Mapping-Alias akzeptiert.
- **Akzeptanzkriterien:**
  - [ ] URL in `sitemap-a.xml` und `sitemap-b.xml` → 2 Zeilen in „URLs In Multiple Sitemaps“.
  - [ ] Noindex-URL in Sitemap → „Non-Indexable URLs In Sitemap“.
  - [ ] Folge-Orphan `/o2`, gefunden über die Orphan-URL `/o1` aus `sitemap-a.xml` → Zeile mit `Sitemap`=`sitemap-a.xml`, `Found Via`=`/o1`; die Zeile von `/o1` hat leeres `Found Via`.
  - [ ] Erfordert, dass Sitemaps gecrawlt wurden; sonst Hinweis.
- **Testfixture:** `out-sitemaps-in`: Sitemap-Index mit 2 Sitemaps, Überschneidung, noindex-URL, 404-URL, verwaiste URL.

### OUT-058 JavaScript: Exporte je Filter
- **Priorität:** P2
- **Beschreibung:** Inlinks-Exporte je JavaScript-Filter sowie Konsolen-Log aller Seiten.
- **Ausgabe:** Standard-Linkformat je Filter (`Blocked Resource`, `Contains JavaScript Links`, `Contains JavaScript Content`, `Noindex Only In Original HTML`, `Nofollow Only In Original HTML`, `Canonical Only In Rendered HTML`, `Canonical Mismatch`, `Page Title Only In Rendered HTML`, `Pages With JavaScript Errors`, …); `All Console Messages`: `Address`, `Level` (error/warning/info/log), `Message`, `Source URL`, `Line`, `Column` (kanonische Schlüssel wie TECH-114, führend; gleiche Spalten wie der Bulk-Export „JavaScript:Pages With JavaScript Issues“ dort).
- **Akzeptanzkriterien:**
  - [ ] `Contains JavaScript Links` liefert die Links, die nur im gerenderten DOM vorhanden sind (nicht leer, bekannter Fehler der Referenz).
  - [ ] `console.error("x")` auf einer Seite → eine Zeile `Level`=error.
- **Testfixture:** `out-render` + Seite mit Konsolenfehler.

### OUT-059 AMP: Exporte je Filter
- **Priorität:** P2
- **Beschreibung:** Je AMP-Filter ein Inlinks-Export auf die betroffenen AMP-URLs (Quelle = Seite, die per `rel="amphtml"` oder Hyperlink auf die AMP-Seite verweist).
- **Ausgabe:** Standard-Linkformat. Einträge (je `… Inlinks`): `Non-200 Response`, `Missing Non-AMP Return Link`, `Missing Canonical to Non-AMP`, `Non-Indexable Canonical`, `Indexable`, `Non-Indexable`, `Missing <html amp> Tag`, `Missing/Invalid <!doctype html> Tag`, `Missing <head> Tag`, `Missing <body> Tag`, `Missing Canonical`, `Missing/Invalid <meta charset> Tag`, `Missing/Invalid <meta viewport> Tag`, `Missing/Invalid AMP Script`, `Missing/Invalid AMP Boilerplate`, `Contains Disallowed HTML`, `Other Validation Errors`. Die maßgebliche Filterliste definiert der Spec-Bereich des AMP-Tabs; Registry-Test erzwingt 1:1-Abdeckung.
- **Akzeptanzkriterien:**
  - [ ] `amphtml`-Link auf 404 → Eintrag in „Non-200 Response“ mit `Type`=AMP, `Status Code`=404.
  - [ ] AMP-Seite `/p/amp` ohne `<link rel="canonical">` → „Missing Canonical“; mit Canonical auf `/p`, aber `/p` verweist nicht per `amphtml` zurück → „Missing Non-AMP Return Link“.
  - [ ] AMP-Seite mit Canonical auf eine noindex-Seite → „Non-Indexable Canonical“.
  - [ ] AMP-Seite ohne `⚡`/`amp`-Attribut an `<html>` → „Missing <html amp> Tag“; `<html amp>` und `<html ⚡>` gelten beide als vorhanden.
  - [ ] AMP-Seite mit `<img>` statt `<amp-img>` → „Contains Disallowed HTML“.
  - [ ] Ohne aktivierte AMP-Extraktion sind alle AMP-Exporte im Menü ausgegraut mit Hinweis.
  - [ ] Registry-Test: Anzahl AMP-Exporte = Anzahl AMP-Filter.
- **Testfixture:** `out-amp`: Normalseite mit `amphtml`-Link auf gültige AMP-Seite, auf 404-AMP-Seite, AMP-Seite ohne Canonical, ohne Rückverweis, mit noindex-Canonical, ohne `amp`-Attribut, mit `<img>`, ohne Boilerplate.

### OUT-060 Custom Search: Exporte je Suchfilter
- **Priorität:** P1
- **Beschreibung:** Für jeden benutzerdefinierten Suchfilter ein Export der URLs mit Treffern (bzw. ohne Treffer bei „Does Not Contain“), inklusive Trefferanzahl.
- **Ausgabe:** `Address`, `Filter`, `Occurrences`, `Status Code`, `Indexability` **(OS-Festlegung;** Referenz liefert Inlinks – zusätzlich `… Inlinks`-Variante im Standard-Linkformat).
- **Akzeptanzkriterien:**
  - [ ] Filter „Contains `Ausverkauft`“ auf 2 Seiten (eine mit 3 Treffern) → 2 Zeilen, Occurrences 3 und 1.
  - [ ] Pro Filter eine Datei, Dateiname enthält Filternamen.
- **Testfixture:** `out-custom-search`.

### OUT-061 Custom Extraction: Exporte
- **Priorität:** P1
- **Beschreibung:** Alle extrahierten Werte im Langformat sowie Inlinks-Exporte je Extraktor.
- **Ausgabe:** `All Extractions`: `Address`, `Extractor`, `Index` (1-basiert bei mehreren Treffern), `Value`. Je Extraktor `… Inlinks` im Standard-Linkformat.
- **Akzeptanzkriterien:**
  - [ ] XPath-Extraktor `//span[@class='price']` auf Seite mit 2 Preisen → 2 Zeilen mit Index 1 und 2.
  - [ ] Langformat hat keine Begrenzung auf die in der Tabelle angezeigte Anzahl Spalten.
- **Testfixture:** `out-custom-extraction`.

### OUT-062 Accessibility: All Violations und je WCAG-Stufe
- **Priorität:** P1
- **Beschreibung:** Alle Barrierefreiheitsverstöße mit Fundstelle; zusätzlich gefiltert nach WCAG-Stufe.
- **Eingaben/Konfiguration:** Accessibility-Prüfung + JS-Rendering aktiv.
- **Ausgabe:** `Address`, `Issue`, `Rule ID`, `Guideline` (z. B. `WCAG 2.0 A, 1.1.1`), `Impact` (Critical/Serious/Moderate/Minor), `Priority` (High/Medium/Low), `Location on Page` (CSS-Selektor), `HTML`, `Help URL`. Die mit dem Detail-Tab gemeinsamen Spalten (`Issue`, `Guideline`, `Impact`, `Location on Page`, `HTML`, `Help URL`) heißen exakt wie in TECH-259 (führend); `Rule ID` und `Priority` (UI-063) sind Export-Zusatzspalten. Einträge: `All Violations`, `WCAG 2.0 A`, `WCAG 2.0 AA`, `WCAG 2.0 AAA`, `WCAG 2.1 AA`, `WCAG 2.2 AA`, `Best Practice` sowie `Incomplete` (alle Knoten mit Ergebnisart `Incomplete` zur manuellen Prüfung, gleiche Spalten; übernommen aus TECH-259).
- **Akzeptanzkriterien:**
  - [ ] `<img src="x.png">` ohne alt → Zeile mit Regel „image-alt“, Stufe WCAG 2.0 A, Impact Critical.
  - [ ] Summe der Stufen-Exporte = All Violations (jede Verletzung genau einer Stufe zugeordnet; maßgeblich ist die niedrigste zutreffende Stufe).
  - [ ] `Incomplete`: Text über einem Hintergrundbild (`color-contrast` unentscheidbar) → eine Zeile je Knoten; diese Knoten erscheinen in keinem Violations-Export.
- **Testfixture:** `out-a11y`: fehlender alt, zu geringer Kontrast, fehlendes Label, doppelte IDs.

### OUT-063 Issues: Alle Issues als Dateisammlung
- **Priorität:** P0
- **Beschreibung:** Exportiert jede Zeile des Issues-Panels als eigene Datei mit den betroffenen URLs; bei linkbezogenen Issues zusätzlich die Inlinks-Variante.
- **Eingaben/Konfiguration:** Zielordner; Format.
- **Ausgabe:** Ordner `issues/` mit Dateien `<issue_type>_<issue_name>.csv` (z. B. `issue_response_codes_internal_client_error_4xx.csv`, `issue_..._inlinks.csv`) und einer `issues_overview.csv` (Format wie OUT-025 Issues inkl. Erweiterungsspalte `Why It Matters`; die Texte in `Description`/`How To Fix`/`Why It Matters` sind eigenständig verfasste OpenSpider-Texte, keine Übernahme oder Nachbildung der Issue-Beschreibungen der Referenz).
- **Akzeptanzkriterien:**
  - [ ] Anzahl Dateien (ohne Inlinks-Varianten und Übersicht) = Anzahl Zeilen im Issues-Panel.
  - [ ] Jede Datei enthält genau die URLs, die der Issue-Zähler nennt.
  - [ ] Die Option „Nicht indexierbare URLs bei Issues ignorieren“ wirkt identisch auf Panel und Export.
  - [ ] Bei Google Sheets: eine Tabelle mit einem Blatt je Issue (Konsolidierung erzwungen), da sonst Hunderte Dateien entstünden **(OS-Festlegung)**.
- **Testfixture:** `out-issues`: Seite ohne Title, doppelte Titles, 404-Link, fehlendes H1, langer Title.

### OUT-064 URL Inspection: Rich Results / Referring Pages / Sitemaps
- **Priorität:** P2
- **Beschreibung:** Detaildaten aus der Search-Console-URL-Inspection-Schnittstelle.
- **Eingaben/Konfiguration:** GSC-Verbindung mit aktivierter URL-Inspektion (Tageslimit 2 000 je Property beachten).
- **Ausgabe:**
  - `Rich Results`: `Address`, `Rich Result Type`, `Validity`, `Severity`, `Issue Type`, `Item Name`.
  - `Referring Pages`: `Address`, `Referring Page` (max. 5 je URL laut Schnittstelle).
  - `Sitemaps`: `Address`, `Sitemap`.
- **Akzeptanzkriterien:**
  - [ ] Mit Mock-GSC-Antwort mit 7 Referring Pages werden nur die gelieferten (≤ 5) ausgegeben.
  - [ ] Auch im APIs-Modus (ohne Crawl) verfügbar.
- **Testfixture:** Mock-GSC-Endpunkt im `os-testsite`.

### OUT-065 AI: generierte Bilder und Audiodateien
- **Priorität:** P2
- **Beschreibung:** Speichert durch KI-Integrationen (Prompts mit Ausgabetyp Bild bzw. Sprache) erzeugte Bilder bzw. Audiodateien als Dateien. Zwei Einträge: `AI:All Generated Images`, `AI:All Generated Audio`.
- **Eingaben/Konfiguration:** KI-Prompt mit Ausgabetyp Bild/Audio; Zielordner.
- **Ausgabe:** Ordner `ai_images/<prompt_name>/` bzw. `ai_audio/<prompt_name>/` mit einer Datei je URL; Dateiname = bereinigte URL (OUT-008) + `_<8-stelliger Hash>.<ext>`; je Ordner `index.csv` mit `Address`, `Prompt`, `File`, `MIME Type`, `Bytes`, `Error`.
- **Akzeptanzkriterien:**
  - [ ] Nur lokal speicherbar (Drive-Ziel deaktiviert); Erweiterung aus dem MIME-Typ der Anbieterantwort: `image/png`→`.png`, `image/jpeg`→`.jpg`, `image/webp`→`.webp`, `audio/mpeg`→`.mp3`, `audio/wav`→`.wav`; unbekannter Typ → `.bin` und Warnung.
  - [ ] Zwei Bild-Prompts über 3 URLs → 2 Unterordner mit je 3 Dateien und je einem `index.csv` mit 3 Zeilen.
  - [ ] URL, für die der Anbieter einen Fehler lieferte → keine Datei, aber Zeile in `index.csv` mit leerem `File` und gefülltem `Error`.
  - [ ] Dateien sind byte-identisch mit der (ggf. base64-dekodierten) Anbieterantwort.
  - [ ] Ohne Prompt mit Ausgabetyp Bild/Audio sind die Einträge ausgegraut.
- **Testfixture:** Mock-KI-Endpunkt mit festen PNG-/MP3-Antworten und einer Fehlerantwort (HTTP 500) für eine URL; `out-basic-site` (3 Seiten).

### OUT-066 Weitere Tab-Bulk-Exporte (generische Regel)
- **Priorität:** P1
- **Beschreibung:** Damit kein Filter ohne Massenexport bleibt, gilt: Für jeden Filter jedes Haupt-Tabs, dessen Befund an der Ziel-URL hängt (Page Titles, Meta Description, H1, H2, Hreflang, Pagination, Mobile, Validation, Link Metrics, Analytics, Search Console, PageSpeed), steht ein Export `<Tab>:<Filter> Inlinks` im Standard-Linkformat bereit, sofern der Bereich „Inlinks-Exporte für alle Filter“ aktiviert ist (Standard: an) **(OS-Festlegung,** Erweiterung gegenüber Referenz).
- **Ausgabe:** Standard-Linkformat.
- **Akzeptanzkriterien:**
  - [ ] Registry-Test: Anzahl generierter Inlinks-Exporte = Anzahl registrierter Filter dieser Tabs.
  - [ ] `Page Titles:Missing Inlinks` enthält alle Inlinks auf Seiten ohne Title.
- **Testfixture:** `out-issues`.

### OUT-067 Response Codes: All Error Inlinks
- **Priorität:** P1
- **Beschreibung:** Ein gebündelter Export aller Verweise auf fehlerhafte Ziele – Ziele ohne Antwort, mit Client-Fehler (4xx) und mit Server-Fehler (5xx) – in einer Datei (im Referenzstand unter Bulk Export → Response Codes). Erspart das Zusammenführen dreier Einzelexporte.
- **Eingaben/Konfiguration:** keine; verfügbar in den Bereichen `Internal & External`, `Internal`, `External` (kanonische Pfade `Response Codes:All Error Inlinks`, `Response Codes:Internal:All Error Inlinks`, `Response Codes:External:All Error Inlinks`) **(OS-Festlegung** für die Verfügbarkeit in allen drei Bereichen).
- **Ausgabe:** Standard-Linkformat (OUT-031); `Status Code` leer und `Status` mit Fehlergrund (z. B. `DNS Lookup Failed`, `Connection Timeout`, `Connection Refused`) bei Zielen ohne Antwort.
- **Akzeptanzkriterien:**
  - [ ] Mengengleichheit: Zeilen von `All Error Inlinks` = Vereinigung der Zeilen von `No Response Inlinks`, `Client Error (4xx) Inlinks` und `Server Error (5xx) Inlinks` desselben Bereichs (gleiche Zeilenzahl, keine Duplikate, Test über Multimengen-Vergleich).
  - [ ] Nicht enthalten: Verweise auf 2xx, 3xx (auch wenn die Kette auf 404 endet – die 404 selbst ist enthalten, die Weiterleitungs-URL nicht), robots-gesperrte Ziele, Blocked Resource.
  - [ ] Ein externes Ziel mit 500 erscheint in `Internal & External` und `External`, nicht in `Internal`.
  - [ ] Sortierung: `Status Code` aufsteigend (leere Werte = No Response zuerst), dann `Destination`, dann `Source` **(OS-Festlegung)**.
  - [ ] Erscheint im Crawl Overview (OUT-080) und im Multi-Export-Baum (OUT-075).
  - [ ] Die Zeilenauswahl stützt sich ausschließlich auf die Filterzuordnung der Fachspec (No Response ONPAGE-054, Client Error (4xx) ONPAGE-062, Server Error (5xx) ONPAGE-063); ein Ziel, das dort in Success (2xx), Redirection (3xx) oder Blocked by Robots.txt steht, erscheint nie (übernommen aus ONPAGE-067).
  - [ ] Fixture `onpage-rc/` (Fachspec 02): `Internal:All Error Inlinks` enthält die Verweise auf `/e400`, `/e403`, `/e404`, `/e410`, `/e429`, `/e500`, `/e503`, `/timeout`, `/drop` und keine auf `/ok`, `/r301`, `/blocked/a`, `/s999`; `External:All Error Inlinks` enthält die Verweise auf das externe `/e404`, `http://nohost.invalid/` und `http://127.0.0.1:1/` und keine internen Ziele (übernommen aus ONPAGE-067).
- **Testfixture:** `out-response-codes` (enthält No Response, 403, 404, 410, 500, 503, extern 404/500, 301→404); `onpage-rc/` aus Fachspec 02.

### OUT-075 Multi-Export (Mehrfachexport mit Vorlagen)
- **Priorität:** P1
- **Beschreibung:** Ein Dialog (Bulk Export → Multi Export) zeigt einen Baum aller Tabs/Filter, Bulk-Exporte und Reports mit Checkboxen. Mit einem Klick werden alle gewählten Exporte erzeugt. Auswahlen lassen sich als benannte Vorlagen (Presets) speichern und in UI, Scheduling und CLI wiederverwenden. Optional wird zusätzlich der Looker-Studio-Export (OUT-210) für einen manuellen Crawl ausgeführt.
- **Eingaben/Konfiguration:** Auswahl, Format, Zielordner/Drive, Optionen Konsolidierung (OUT-007), Leere überspringen (OUT-012), Segment (OUT-011), Überschreiben/Zeitstempel (OUT-013).
- **Ausgabe:** Dateien bzw. Arbeitsmappe; Preset-Datei `<name>.ospreset` (TOML, Liste kanonischer Exportpfade + Optionen) im Einstellungsordner **(OS-Festlegung)**.
- **Akzeptanzkriterien:**
  - [ ] Auswahl „Internal:All“, „Links:All Inlinks“, „Reports:Crawl Overview“ → drei Dateien in einem Lauf.
  - [ ] Suche im Baum filtert Einträge nach Teilstring.
  - [ ] Preset speichern → nach Neustart wieder auswählbar; Preset per CLI `--export-preset <name|pfad>` nutzbar.
  - [ ] Preset mit unbekanntem Exportnamen (z. B. aus neuerer Version) → Warnung, übrige Exporte laufen.
  - [ ] Einträge, deren Voraussetzung fehlt (z. B. Screenshots ohne Rendering), sind markiert und werden übersprungen mit Logeintrag.
- **Testfixture:** `out-basic-site`.

### OUT-068 Links: Non-Indexable Page Inlinks Only
- **Priorität:** P1
- **Beschreibung:** Bulk-Export (Menü Links) aller internen Verweise auf indexierbare Seiten, die **ausschließlich** von nicht indexierbaren Seiten aus verlinkt werden – also Seiten, die Suchmaschinen nur über nicht indexierbare Quellen finden können. Die Auswahl der Zielseiten folgt dem gleichnamigen Filter der Fachspec (TECH-142, führend für die Filterlogik); dieser Eintrag legt nur den Export fest. Kanonischer Pfad `Links:Non-Indexable Page Inlinks Only`. (Lücke G-1 aus PARITY.md.)
- **Eingaben/Konfiguration:** keine.
- **Ausgabe:** Standard-Linkformat (OUT-031); eine Zeile je Verweis auf eine betroffene Zielseite.
- **Akzeptanzkriterien:**
  - [ ] Ziel `/x`, verlinkt nur von `/noindex-a` (meta noindex) und `/canon-b` (kanonisiert auf eine andere URL) → 2 Zeilen mit `Destination`=`/x`.
  - [ ] Ziel `/y`, verlinkt von `/noindex-a` und von der indexierbaren Startseite → keine Zeile für `/y`.
  - [ ] Menge der Ziel-URLs im Export = Menge der URLs im Filter aus TECH-142 (Registry-/Konsistenztest).
  - [ ] Maßgeblich sind interne Hyperlink-Inlinks ohne Self-Links (wie TECH-142); Canonical-, hreflang- und Ressourcen-Verweise werden weder als Quelle gezählt noch exportiert **(OS-Festlegung** für den Export). Die Startseite erscheint nie als Ziel.
  - [ ] Per CLI `--bulk-export "Links:Non-Indexable Page Inlinks Only"` ansprechbar und im Multi-Export-Baum (OUT-075) enthalten; Dateiname `links_non_indexable_page_inlinks_only.csv` (OUT-008).
  - [ ] Erfordert die abgeschlossene Crawl-Analyse (der Filter entsteht im Nachlauf); vorher Hinweis und leerer Export wie OUT-112.
- **Testfixture:** `out-links-follow` erweitert um `/noindex-a`, `/canon-b`, `/x`, `/y`.

### OUT-069 AI: Embeddings (Vektor-Export)
- **Priorität:** P2
- **Beschreibung:** Exportiert die gespeicherten Embedding-Vektoren (INTEG-166/INTEG-180) je URL, z. B. zur Weiterverarbeitung in eigenen Werkzeugen. Führend für Name und Format dieses Exports; INTEG-166 verweist hierher. Kanonischer Pfad `AI:Embeddings` **(OS-Festlegung,** Entscheidung zu der offenen Formatfrage JSON-Array aus INTEG-166 gegenüber CSV aus dem früheren ONPAGE-159).
- **Eingaben/Konfiguration:** Embedding-Funktionalität aktiv (INTEG-180); Auswahl des Embedding-Prompts, falls mehrere existieren.
- **Ausgabe:** CSV/XLSX mit den Spalten `Address`, `Prompt`, `Embedding Model`, `Dimensions`, `Embedding`; `Embedding` enthält den Vektor als JSON-Array von Gleitkommazahlen in kürzester verlustfreier Dezimaldarstellung (Dezimalpunkt, kein Leerzeichen), z. B. `[0.0123,-0.4567,…]`. Eine Zeile je URL mit Vektor; URLs ohne Vektor fehlen. XLSX: Zellen über 32 767 Zeichen sind nicht zulässig – der Export bietet dann nur CSV an (Hinweis im Dialog).
- **Akzeptanzkriterien:**
  - [ ] Für die Fixture `fixtures/integ/embed/site/` (Fachspec 04) enthält der Export 10 Zeilen; `Dimensions` stimmt mit der Länge des JSON-Arrays überein.
  - [ ] Einlesen des JSON-Arrays ergibt bitgleich dieselben Gleitkommawerte wie der gespeicherte Vektor (im gespeicherten Zahlentyp) (Round-Trip-Test).
  - [ ] Eine URL, deren Embedding-Anfrage fehlschlug, erscheint nicht.
  - [ ] Ohne aktive Embedding-Funktionalität ist der Eintrag ausgegraut.
- **Testfixture:** Mock-Embedding-Endpunkt aus 04 (deterministische Vektoren).

---

## 4 Reports

Menü „Reports“. Reports sind aufbereitete Auswertungen, die nicht 1:1 einem Tab entsprechen. Reports, deren Daten erst in der Nachlauf-Analyse („Crawl Analysis“) entstehen, sind vor deren Abschluss mit Hinweis gekennzeichnet; der Export liefert dann nur die Kopfzeile und meldet „Crawl-Analyse erforderlich“. Kanonische CLI-Pfade: `"<Untermenü>:<Report>"`, z. B. `"Redirects:Redirect Chains"`; Reports ohne Untermenü nur `"Crawl Overview"`.

### OUT-080 Crawl Overview
- **Priorität:** P0
- **Beschreibung:** Gesamtübersicht des Crawls in einer Tabelle: Eckdaten, Inhaltstypen, Antwortcodes und für jeden Tab jeden Filter mit Anzahl und Anteil – identisch mit den Zählern im rechten „Overview“-Panel.
- **Eingaben/Konfiguration:** optional Segment.
- **Ausgabe:** Langformat mit Spalten `Section`, `Item`, `URLs`, `% of Total`. Abschnitte in dieser Reihenfolge:
  1. `Summary`: `Start URL`/`List File`, `Mode`, `Crawl Start`, `Crawl End`, `Elapsed Time`, `Total URLs Encountered`, `Total URLs Crawled`, `Total Internal Blocked by robots.txt`, `Total External Blocked by robots.txt`, `URLs Remaining In Queue`, `Average Response Time (ms)`, `Average URLs/s`, `OpenSpider Version`.
  2. `Crawl Data – Internal`: `All`, `HTML`, `JavaScript`, `CSS`, `Images`, `PDF`, `Plugins`, `Media`, `Fonts`, `XML`, `Other`, `Unknown`.
  3. `Crawl Data – External`: wie 2.
  4. `Response Codes` (intern+extern getrennt): `Blocked by Robots.txt`, `Blocked Resource`, `No Response`, `Success (2xx)`, `Redirection (3xx)`, `Redirection (JavaScript)`, `Redirection (Meta Refresh)`, `Redirection (HTTP Refresh)`, `Redirect Chain`, `Redirect Loop`, `Client Error (4xx)`, `Server Error (5xx)`, `All Errors` (Summe No Response + 4xx + 5xx, Gegenstück zu OUT-067) **(OS-Festlegung** für die Zeile `All Errors`).
  5. Je Tab (Reihenfolge wie UI) alle Filter.
  6. `Crawl Depth`: `0` … `10`, `10+` (Zeile `0` = Start-URL(s), wie die Referenz). Gezählt werden **interne HTML-Seiten mit Status 2xx** je Tiefe, nicht alle internen URLs (Paritätsbefund M1 vom 14.10.2026, K-58; vorher „alle internen URLs mit bekannter Tiefe“ und „die Referenz beginnt bei 1“, beides widerlegt).
  7. `Issues`: Anzahl Issues/Warnings/Opportunities nach Priorität.
- **Akzeptanzkriterien:**
  - [ ] Jeder Zähler stimmt mit dem entsprechenden Zähler im UI-Overview überein (automatisierter Abgleich über alle Filter).
  - [ ] `% of Total` bezieht sich auf die Grundgesamtheit des Abschnitts (z. B. Page-Titles-Filter auf „interne HTML-URLs“); Rundung auf 2 Nachkommastellen; Division durch 0 → leer.
  - [ ] Filter, die mangels Konfiguration nicht befüllt werden (z. B. PageSpeed ohne API), erscheinen mit leerem `URLs` statt 0.
  - [ ] Im Vergleichsmodus zusätzliche Spalten (siehe OUT-147).
  - [ ] `Total URLs Encountered` ≥ `Total URLs Crawled`; Differenz = blockierte + nicht gecrawlte + Queue.
  - [ ] `Redirection (HTTP Refresh)` zählt im Fixture genau die Ziel-URL mit `Refresh`-Header (1), `Redirection (Meta Refresh)` genau die Meta-Refresh-Seite (1).
  - [ ] `All Errors` = `No Response` + `Client Error (4xx)` + `Server Error (5xx)` je Bereich (Invariante).
- **Testfixture:** `out-basic-site` + `out-response-codes`; Golden `crawl_overview.csv`.

### OUT-081 Redirects: All Redirects
- **Priorität:** P0
- **Beschreibung:** Alle internen (und, falls externe gefolgt werden, externen) Weiterleitungen – Einzelsprünge und Ketten – je Start-URL eine Zeile, mit allen Sprüngen bis zum Endziel.
- **Eingaben/Konfiguration:** Max. Anzahl zu folgender Weiterleitungen (Crawl-Limits); Weiterleitungsarten: HTTP 3xx, HSTS-Policy (interne 307-Umleitung auf HTTPS, wenn „HSTS respektieren“ aktiv), Meta Refresh, HTTP-`Refresh`-Header, JavaScript-Redirect (bei Rendering).
- **Ausgabe:** `Chain Type` (`HTTP Redirect`, `HSTS Policy`, `JavaScript Redirect`, `Meta Refresh Redirect`, `HTTP Refresh`, `Mixed` – `Mixed` sobald eine Kette Sprünge unterschiedlicher Art enthält **(OS-Festlegung)**), `Number of Redirects`, `Redirect Loop` (True/False), `Temp Redirect in Chain` (True/False), `Address` (Start-URL), `Final Address`, `Final Status Code`, `Final Status`, `Final Content Type`, `Final Indexability`, `Final Indexability Status`, `Inlinks` (Anzahl Inlinks auf die Start-URL), `Sample Source` (eine verlinkende Seite) **(OS-Festlegung** für die letzten beiden), danach je Sprung n (1…max): `Redirect Type n` (∈ {`HTTP Redirect`, `HSTS Policy`, `JavaScript Redirect`, `Meta Refresh Redirect`, `HTTP Refresh`}), `Redirect URL n`, `Status Code n`, `Status n`.
- **Akzeptanzkriterien:**
  - [ ] HSTS: `http://site.test/h` auf einem Host, der zuvor `Strict-Transport-Security` geliefert hat, und aktivem „HSTS respektieren“ → erster Sprung `Redirect Type 1`=`HSTS Policy`, `Status Code 1`=307, `Status 1`=`HSTS Policy`; ohne die Option erscheint stattdessen der echte Server-Sprung (z. B. 301, `HTTP Redirect`).
  - [ ] HTTP Refresh: Antwort 200 mit Header `Refresh: 0; url=/r2` → `Redirect Type 1`=`HTTP Refresh`, `Status Code 1`=200, `Chain Type`=`HTTP Refresh`.
  - [ ] Meta Refresh: `<meta http-equiv="refresh" content="0;url=/m2">` → `Redirect Type 1`=`Meta Refresh Redirect`.
  - [ ] Kette 301 → Meta Refresh → 200 → `Chain Type`=`Mixed`, `Redirect Type 1`=`HTTP Redirect`, `Redirect Type 2`=`Meta Refresh Redirect`.
  - [ ] `/a` 301→ `/b` 200 → eine Zeile, `Number of Redirects`=1, `Final Status Code`=200, `Temp Redirect in Chain`=False.
  - [ ] `/c` 302→ `/d` 301→ `/e` 200 → `Number of Redirects`=2, `Temp Redirect in Chain`=True (302, 303, 307 sowie Meta Refresh und HTTP Refresh mit Verzögerung > 0 gelten als temporär **(OS-Festlegung** für Meta/HTTP Refresh); die interne 307 der HSTS Policy zählt **nicht** als temporär, da sie keine Server-Weiterleitung ist **(OS-Festlegung)**).
  - [ ] Zwischenglieder einer Kette (`/d`) erzeugen keine eigene Zeile, wenn sie nur über die Kette erreicht wurden; sind sie zusätzlich direkt verlinkt, erhalten sie eine eigene Zeile **(OS-Festlegung)**.
  - [ ] Schleife `/f`→`/g`→`/f` → `Redirect Loop`=True, `Final Address` leer, `Final Status` = `Redirect Loop`.
  - [ ] Selbst-Refresh (Meta Refresh ohne URL oder auf die eigene URL, `Refresh`-Header auf die eigene URL; CRAWL-015, K-69) → eine Zeile mit einem Sprung auf die eigene Adresse, `Redirect Loop`=True; die Seite steht auch in Redirect Chains (Schleifen sind dort immer enthalten, OUT-082) (Paritätsbefund M1, Befund N-07, K-57).
  - [ ] Abbruch bei Erreichen des Weiterleitungslimits → `Final Status` = `Max Redirects Reached`.
  - [ ] Spaltenzahl richtet sich nach der längsten Kette im Crawl.
- **Testfixture:** `out-redirects`: einfache 301, 302-301-Kette, 5er-Kette, Schleife, Kette auf 404, Meta Refresh, `Refresh`-Header, JS-Redirect, 308, gemischte Kette 301→Meta Refresh, HSTS-Host (TLS-Fixture liefert `Strict-Transport-Security: max-age=31536000`, HTTP-Variante 301 auf HTTPS), Kette über Host-Wechsel http→https→www.

### OUT-082 Redirects: Redirect Chains
- **Priorität:** P0
- **Beschreibung:** Wie OUT-081, aber nur Ketten mit **mindestens 2** Weiterleitungen.
- **Ausgabe:** Spalten wie OUT-081.
- **Akzeptanzkriterien:**
  - [ ] `/a` (1 Sprung) fehlt; `/c` (2 Sprünge) und die 5er-Kette sind enthalten; die Schleife ist enthalten.
  - [ ] Filterbar über die `Redirect Loop`-Spalte (Excel-Autofilter vorhanden, OUT-003).
- **Testfixture:** `out-redirects`.

### OUT-083 Redirects: Redirect & Canonical Chains
- **Priorität:** P1
- **Beschreibung:** Ketten aus Weiterleitungen **und/oder** Canonicals mit mindestens 2 Gliedern (z. B. 301 auf eine Seite, deren Canonical woandershin zeigt).
- **Ausgabe:** wie OUT-081; `Redirect Type n` kann zusätzlich `Canonical` (Link-Element) bzw. `HTTP Canonical` (HTTP-`Link`-Header) sein. **Werteliste `Chain Type` für Ketten mit Canonical-Gliedern (führend hier, OS-Festlegung zu R-16; die Kettenermittlung steht in TECH-026):** `Canonical` = nur Canonical-Glieder; `Redirect & Canonical` = mindestens ein Weiterleitungs- und ein Canonical-Glied. Reine Weiterleitungsketten behalten ihre Werte aus OUT-081 (`Mixed` bezeichnet nur Weiterleitungsketten mit unterschiedlichen Weiterleitungsarten). Die früheren Werte `Canonical`/`Redirect`/`Mixed` aus TECH-025 entfallen. Das Sprunglimit ist dasselbe Weiterleitungslimit wie in OUT-081 (CRAWL-072); ein Canonical-Glied zählt wie ein Sprung. Die Referenz schreibt für Ketten aus Weiterleitungs- und Canonical-Gliedern `Mixed`; OpenSpider bleibt bei `Redirect & Canonical`, die Abweichung ist als DEV geführt (K2-88, SQ-13e).
- **Akzeptanzkriterien:**
  - [ ] `/h` 301→ `/i` (Canonical → `/j`, 200, self-canonical) → 2 Glieder, `Final Address`=`/j`, `Chain Type`=`Redirect & Canonical`, `Redirect Type 1`=`HTTP Redirect`, `Redirect Type 2`=`Canonical`.
  - [ ] Reine Canonical-Kette A→B→C erscheint hier mit `Chain Type`=`Canonical` (gleiche Zeile wie in OUT-086).
  - [ ] Seite mit selbstreferenzierendem Canonical bildet kein Kettenglied.
  - [ ] Canonical-Schleife `/k`↔`/l` → `Redirect Loop`=True.
- **Testfixture:** `out-redirects` + `out-canonicals`.

### OUT-084 Redirects: Redirects to Errors
- **Priorität:** P1
- **Beschreibung:** Weiterleitungen/Ketten, deren Endziel ein Fehler ist: robots.txt-gesperrt, keine Antwort, 4xx oder 5xx.
- **Ausgabe:** wie OUT-081.
- **Akzeptanzkriterien:**
  - [ ] Kette `/m` 301→ `/n` 404 enthalten; `/a`→200 nicht.
  - [ ] Weiterleitung auf robots-gesperrte URL enthalten mit `Final Status` = `Blocked by Robots.txt`.
  - [ ] Weiterleitung auf DNS-Fehler-Host enthalten mit `Final Status` = `No Response` / `DNS Lookup Failed`.
  - [ ] Alle Weiterleitungsarten zählen (HTTP 3xx, Meta Refresh, HTTP Refresh, JavaScript, HSTS Policy); auch Ketten mit nur einem Sprung sind enthalten; eine Schleife ohne fehlerhaftes Endziel ist nicht enthalten (übernommen aus ONPAGE-067).
  - [ ] Das Endziel wird ausschließlich über die Filterzuordnung der Fachspec bestimmt: Blocked by Robots.txt (ONPAGE-052), No Response (ONPAGE-054), Client Error (4xx) (ONPAGE-062), Server Error (5xx) (ONPAGE-063) (übernommen aus ONPAGE-067).
  - [ ] Fixture `onpage-rc/` (Fachspec 02): `/r-to-404` → `Number of Redirects`=1, `Final Status Code`=404; `/r-to-blocked` → `Final Status`=`Blocked by Robots.txt`; `/r-chain-500` → `Number of Redirects`=2; `/r301` (→ 200) und `/loop1` fehlen (übernommen aus ONPAGE-067).
- **Testfixture:** `out-redirects`; `onpage-rc/` aus Fachspec 02.

### OUT-085 Ketten-Reports im Listenmodus
- **Priorität:** P1
- **Beschreibung:** Im Listenmodus werden für Ketten-Reports alle Sprünge nur dann vollständig verfolgt, wenn „Weiterleitungen immer folgen“ bzw. „Canonicals immer folgen“ aktiviert sind; jede Listen-URL ist Startpunkt einer Zeile.
- **Eingaben/Konfiguration:** Erweiterte Optionen „Always Follow Redirects“, „Always Follow Canonicals“.
- **Ausgabe:** wie OUT-081, Reihenfolge = Upload-Reihenfolge.
- **Akzeptanzkriterien:**
  - [ ] Liste mit `/c` und aktivem „immer folgen“ → vollständige Kette `/c`→`/d`→`/e` im Report, obwohl Crawl-Tiefe 0 ist.
  - [ ] Ohne die Option → nur erster Sprung, `Final Status` = `Not Followed` **(OS-Festlegung)**.
  - [ ] Gleichzeitig mehrere Listen-URLs, die in dieselbe Kette münden → jede eigene Zeile.
- **Testfixture:** `out-redirects` + `out-list-order/redirects.txt`.

### OUT-086 Canonicals: Canonical Chains
- **Priorität:** P1
- **Beschreibung:** Canonical-Ketten mit mindestens 2 Gliedern (A → B, B → C).
- **Ausgabe:** wie OUT-081 mit `Redirect Type n` = `Canonical`/`HTTP Canonical` und `Chain Type` = `Canonical` (Werteliste OUT-083). Zeilenmenge: reine Canonical-Ketten nach TECH-026; Sprunglimit wie OUT-081 (Abbruch → `Final Status` = `Max Redirects Reached`).
- **Akzeptanzkriterien:**
  - [ ] A (canonical B), B (canonical C), C (self) → eine Zeile für A mit 2 Gliedern; B erscheint nicht als eigene Startzeile, sofern nicht unabhängig verlinkt.
  - [ ] Canonical per HTTP-`Link`-Header wird gleich behandelt.
- **Testfixture:** `out-canonicals`.

### OUT-087 Canonicals: Non-Indexable Canonicals
- **Priorität:** P1
- **Beschreibung:** Canonical-Ziele, die nicht indexierbar sind (Status ≠ 200, noindex, selbst kanonisiert, blockiert); indexierbare ausschließlich über Canonicals gefundene Ziele gehören nur zum eigenen Unlinked-Filter/Report (K2-96).
- **Ausgabe:** `Source` (Seite mit Canonical), `Canonical`, `Status Code`, `Status`, `Indexability`, `Indexability Status`, `Unlinked` (True/False).
- **Akzeptanzkriterien:**
  - [ ] Canonical auf 404 → enthalten, `Indexability Status`=Client Error.
  - [ ] Canonical auf noindex-Seite → enthalten.
  - [ ] Canonical auf indexierbare 200-Seite, die nirgends verlinkt ist → hier nicht enthalten; nur im eigenen Unlinked-Filter/Report (K2-96).
- **Testfixture:** `out-canonicals`.

### OUT-088 Pagination: Non-200 Pagination URLs
- **Priorität:** P2
- **Beschreibung:** `rel="next"`/`rel="prev"`-Ziele, die nicht mit 200 antworten.
- **Ausgabe:** `Source`, `Pagination Type` (Next/Prev), `Pagination URL`, `Status Code`, `Status`.
- **Akzeptanzkriterien:**
  - [ ] `rel=next` auf `/page/3` (404) → enthalten.
- **Testfixture:** `out-pagination`.

### OUT-089 Pagination: Unlinked Pagination URLs
- **Priorität:** P2
- **Beschreibung:** Paginierungs-URLs, die nur über `rel=next/prev` und nicht über Hyperlinks entdeckt wurden.
- **Ausgabe:** wie OUT-088 + `Hyperlink Inlinks` (=0).
- **Akzeptanzkriterien:**
  - [ ] `/page/2` nur per `<link rel=next>` erreichbar → enthalten; per `<a>` verlinkt → nicht enthalten.
- **Testfixture:** `out-pagination`.

### OUT-090 Hreflang: All Hreflang URLs
- **Priorität:** P1
- **Beschreibung:** Alle hreflang-Annotationen als 1:1-Zuordnung (Seite → Sprachcode → Ziel-URL), aus HTML, HTTP-Header und XML-Sitemap.
- **Eingaben/Konfiguration:** hreflang speichern/crawlen; Crawl-Analyse.
- **Ausgabe:** `Address`, `Hreflang Language`, `Hreflang URL`, `Annotation Source` (`HTML`/`HTTP Header`/`XML Sitemap`), `Status Code`, `Indexability`, `Is Self Reference` (True/False).
- **Akzeptanzkriterien:**
  - [ ] Seite mit 3 hreflang-Links (de, en, x-default) → 3 Zeilen.
  - [ ] Dieselbe Annotation in HTML und Sitemap → 2 Zeilen mit unterschiedlicher `Annotation Source`.
- **Testfixture:** `out-hreflang`: Seiten de/en/fr mit korrekten Rücklinks, eine ohne Rücklink, eine mit falschem Regionscode, eine mit Rücklink auf kanonisierte Variante, eine mit Rücklink auf noindex-Seite, ein hreflang-Ziel 404, ein unverlinktes Ziel.

### OUT-091 Hreflang: Non-200 Hreflang URLs
- **Priorität:** P1
- **Beschreibung:** hreflang-Ziele, die nicht mit 200 antworten (inkl. Weiterleitung, Blockierung, keine Antwort). Führend für die Zeilenauswahl aller Hreflang-Reports OUT-091 … OUT-096 (TECH-088 verweist hierher): Jeder Report enthält genau die Beziehungen der Seiten im entsprechenden Filter TECH-074 … TECH-079; „Unlinked Hreflang URLs“ (OUT-092) ist erst nach der Crawl-Analyse befüllt (TECH-005); Mapping-Aliase stehen beim jeweiligen Filter (z. B. TECH-077) (übernommen aus TECH-088).
- **Ausgabe:** wie OUT-090 + `Status`.
- **Akzeptanzkriterien:**
  - [ ] Ziel 404 → enthalten; Ziel 301 → enthalten; Ziel 200 → nicht.
- **Testfixture:** `out-hreflang`.

### OUT-092 Hreflang: Unlinked Hreflang URLs
- **Priorität:** P1
- **Beschreibung:** hreflang-Ziele, die nicht über Hyperlinks im Crawl gefunden wurden.
- **Ausgabe:** wie OUT-090.
- **Akzeptanzkriterien:**
  - [ ] `/fr/` nur per hreflang referenziert → enthalten (eine Zeile pro referenzierender Seite).
- **Testfixture:** `out-hreflang`.

### OUT-093 Hreflang: Missing Return Links
- **Priorität:** P1
- **Beschreibung:** Seite A verweist per hreflang auf B, aber B verweist nicht per hreflang zurück auf A.
- **Ausgabe:** `Address` (A), `Hreflang Language` (Code des fehlenden Rücklinks, daher leer; K2-96), `Hreflang URL` (B), `Hreflang URL Status Code`, `Hreflang URL Indexability`, `Missing Return Link To` (A).
- **Akzeptanzkriterien:**
  - [ ] A→B ohne B→A → eine Zeile.
  - [ ] Ist B nicht 200 oder nicht gecrawlt, wird die Zeile trotzdem ausgegeben, mit Statuscode von B (Referenz ergänzte Statuscodes in einer früheren Version).
  - [ ] Rücklink über eine andere Quelle (z. B. Sitemap statt HTML) zählt als vorhanden.
- **Testfixture:** `out-hreflang`.

### OUT-094 Hreflang: Inconsistent Language & Region Return Links
- **Priorität:** P1
- **Beschreibung:** Rücklinks sind vorhanden, aber der Sprach-/Regionscode, mit dem A die Seite B annotiert, unterscheidet sich vom Code, mit dem B sich selbst annotiert.
- **Ausgabe:** `Address` (A), `Hreflang URL` (B), `Expected Language` (Code laut A), `Actual Language` (Selbstannotation von B).
- **Akzeptanzkriterien:**
  - [ ] A annotiert B als `de-AT`, B annotiert sich selbst als `de-DE` → enthalten.
  - [ ] Vergleich ist case-insensitiv (`de-at` = `de-AT`).
  - [ ] Der frühere Referenz-Reportname „Inconsistent Language Return Links“ wird von CLI (`--save-report`) und Presets als Mapping-Alias akzeptiert und erzeugt denselben Report (übernommen aus TECH-088; Alias auch beim Filter TECH-077).
- **Testfixture:** `out-hreflang`.

### OUT-095 Hreflang: Non Canonical Return Links
- **Priorität:** P1
- **Beschreibung:** Rücklinks zeigen auf eine kanonisierte (nicht-kanonische) URL statt auf die kanonische.
- **Ausgabe:** `Address`, `Hreflang Language`, `Hreflang URL`, `Canonical Of Hreflang URL`. Richtung nach K2-98 analog OUT-094: `Address` ist die Quelle A, `Hreflang URL` das Ziel B des Hreflang-Verweises A → B; B verweist auf eine nicht-kanonische Variante von A zurück.
- **Akzeptanzkriterien:**
  - [ ] B verweist zurück auf `/de/?ref=x`, die per Canonical auf `/de/` zeigt → enthalten.
- **Testfixture:** `out-hreflang`.

### OUT-096 Hreflang: Noindex Return Links
- **Priorität:** P1
- **Beschreibung:** Rücklinks zeigen auf Seiten mit noindex.
- **Ausgabe:** `Address`, `Hreflang Language`, `Hreflang URL`, `Indexability Status`.
- **Akzeptanzkriterien:**
  - [ ] Rücklink auf Seite mit `<meta name="robots" content="noindex">` → enthalten.
- **Testfixture:** `out-hreflang`.

### OUT-097 Insecure Content
- **Priorität:** P1
- **Beschreibung:** Alle unsicheren (HTTP-)Verweise auf HTTPS-Seiten, wie in der Referenz: Hyperlinks auf HTTP-URLs – intern **und extern** (z. B. Links auf Social-Media-Profile per `http://`) –, HTTP-Canonicals, HTTP-Paginierungs- und hreflang-Verweise sowie alle per HTTP eingebundenen Ressourcen (Bilder, JS, CSS, Medien, iframes), intern wie extern (z. B. CDN-Bilder).
- **Ausgabe:** Standard-Linkformat (OUT-031), nur Zeilen mit HTTPS-`Source` und HTTP-`Destination`. Zusätzliche Spalte `Internal` (True/False) zur Filterung **(OS-Festlegung)**.
- **Akzeptanzkriterien:**
  - [ ] HTTPS-Seite mit `<a href="http://site.test/x">` (intern) → enthalten, `Internal`=True.
  - [ ] HTTPS-Seite mit `<a href="http://social.test/profil">` (externer Hyperlink) → enthalten, `Internal`=False.
  - [ ] HTTPS-Seite mit `<img src="http://cdn.test/p.jpg">` (externe Ressource) → enthalten, `Type`=Image.
  - [ ] HTTPS-Seite mit `<script src="http://cdn.test/a.js">` → enthalten (`Type`=JavaScript).
  - [ ] HTTPS-Seite mit `<link rel="canonical" href="http://site.test/x">` → enthalten (`Type`=Canonical); mit `rel="next"` auf HTTP → enthalten (`Type`=Rel Next).
  - [ ] HTTPS-Seite mit protokollrelativem Verweis `//cdn.test/a.js` → nicht enthalten (wird über HTTPS aufgelöst).
  - [ ] HTTP-Seite mit HTTP-Ressourcen → nicht enthalten.
  - [ ] Externe HTTP-Ziele werden aufgenommen, auch wenn „externe Links prüfen“ deaktiviert ist (Bewertung allein anhand des Schemas; `Status Code` dann leer).
- **Testfixture:** `out-security`: HTTPS-Seiten mit internem HTTP-Link, externem HTTP-Social-Link, HTTP-CDN-Bild, HTTP-Skript, HTTP-Canonical, HTTP-`rel=next`, protokollrelativem Skript; HTTP-Seite mit HTTP-Bild.

### OUT-098 SERP Summary
- **Priorität:** P1
- **Beschreibung:** Titles und Meta Descriptions aller internen HTML-Seiten mit Status 2xx (keine Fehlerseiten; K2-96) mit Zeichenlänge und Pixelbreite; die Datei kann im SERP-Modus wieder geladen, bearbeitet und erneut berechnet werden.
- **Ausgabe:** `Address`, `Title 1`, `Title 1 Length`, `Title 1 Pixel Width`, `Meta Description 1`, `Meta Description 1 Length`, `Meta Description 1 Pixel Width`.
- **Akzeptanzkriterien:**
  - [ ] Längen und Pixelbreiten sind identisch mit den Werten in Page Titles/Meta Description (gleiche Berechnung, Schrift und Pixelregeln aus dem On-Page-Bereich).
  - [ ] Der SERP-Modus akzeptiert diese Datei direkt (Spaltenzuordnung `Address`→URL, `Title 1`→Title, `Meta Description 1`→Description) sowie Dateien mit Spalten `URL`, `Title`, `Description`.
  - [ ] Nicht-ASCII-Titel bleiben nach Export/Import unverändert (Rundreise-Test).
- **Testfixture:** `out-serp`: Titles mit 30, 60, 61 Zeichen, mit breiten Zeichen (`WWWW`), Umlauten, CJK.

### OUT-099 Orphan Pages
- **Priorität:** P1
- **Beschreibung:** URLs, die aus Google Analytics, der Search Console oder XML-Sitemaps bekannt sind, aber im Crawl nicht über interne Links gefunden wurden.
- **Eingaben/Konfiguration:** Mindestens eine Quelle: GA4-Verbindung, GSC-Verbindung, Sitemap-Crawl; Crawl-Analyse abgeschlossen.
- **Ausgabe:** `Address`, `Source` ∈ {`GA`, `GSC`, `Sitemap`, `GA & GSC`, `GA & Sitemap`, `GSC & Sitemap`, `GA & GSC & Sitemap`} (Quellwert `GA` auch für GA4-Daten, nicht `GA4`; OS-Festlegung zur offenen Frage aus 04, Kompatibilität mit bestehenden Auswertungen), `Status Code` (falls die URL über die Quelle gecrawlt wurde), `Indexability`.
- **Akzeptanzkriterien:**
  - [ ] URL nur in Sitemap, nicht verlinkt → `Source`=Sitemap.
  - [ ] URL in GA4 (Mock) und Sitemap → `Source`=`GA & Sitemap`.
  - [ ] Verlinkte URLs erscheinen nie, auch wenn sie in allen Quellen stehen.
  - [ ] Ohne jede Quelle ist der Report deaktiviert mit Hinweis auf die nötigen Integrationen.
  - [ ] URL-Abgleich nach Normalisierung (Schema/Host-Kleinschreibung, Standardport, Fragment entfernt); Trailing-Slash-Unterschiede gelten als verschiedene URLs.
- **Testfixture:** `out-sitemaps-in` + Mock-GA4/GSC mit Orphan-Pfaden.

### OUT-100 Structured Data: Validation Errors & Warnings Summary
- **Priorität:** P1
- **Beschreibung:** Aggregierte Validierungsfunde über alle URLs mit Beispiel-URL.
- **Ausgabe:** `Validation Type` (`Schema.org`/`Google Rich Results`), `Severity` (`Error`/`Warning`), `Type` (Schema-Typ), `Feature` (Rich-Result-Feature), `Issue`, `URLs`, `% of Total`, `Sample URL`. Zeilenauswahl: TECH-190 (Befunde aus TECH-187/188 mit Befund-ID TECH-189).
- **Akzeptanzkriterien:**
  - [ ] Drei Produktseiten ohne `offers` → eine Zeile mit `URLs`=3.
  - [ ] `Sample URL` ist die lexikographisch kleinste betroffene URL (deterministisch) (übernommen aus TECH-190).
  - [ ] Die Summe von `URLs` über alle Zeilen ist größer oder gleich der Anzahl distinct Paare (`Address`, Befund-ID) im Detailreport OUT-101 (übernommen aus TECH-190).
  - [ ] Sortierung: Errors vor Warnings, dann `URLs` absteigend.
- **Testfixture:** `out-structured-data`.

### OUT-101 Structured Data: Validation Errors & Warnings
- **Priorität:** P1
- **Beschreibung:** Dieselben Funde je URL.
- **Ausgabe:** `Address`, `Validation Type`, `Severity`, `Type`, `Feature`, `Issue`, `Property Path`.
- **Akzeptanzkriterien:**
  - [ ] Summe der Zeilen je Issue = `URLs` × Vorkommen im Summary (bei mehrfachem Vorkommen je Seite mehrere Zeilen).
- **Testfixture:** `out-structured-data`.

### OUT-102 Structured Data: Google Rich Results Features Summary
- **Priorität:** P1
- **Beschreibung:** Je erkanntem Rich-Result-Feature Anzahl URLs, mit/ohne Fehler.
- **Ausgabe:** `Feature`, `URLs`, `% of Total`, `Valid`, `With Warnings`, `With Errors`, `Sample URL`.
- **Akzeptanzkriterien:**
  - [ ] Die Liste gültiger Features ist versioniert in `os-analyze` hinterlegt (`rich_result_features.toml` mit Feld `deprecated_since`); ein dort als abgekündigt markiertes Feature wird nicht mehr als Feature gezählt (Test: Fixture-Seite mit Markup eines als abgekündigt markierten Typs → keine Zeile für dieses Feature, Schema.org-Validierung des Markups bleibt aber bestehen). Welche Features abgekündigt sind, richtet sich nach Googles öffentlicher Dokumentation, nicht nach einer Referenzversion.
  - [ ] Seite mit gültigem Breadcrumb und Product mit Warnung → `Breadcrumb`: `Valid`=1; `Product`: `With Warnings`=1; `Valid` + `With Warnings` + `With Errors` = `URLs` je Feature.
- **Testfixture:** `out-structured-data`.

### OUT-103 Structured Data: Google Rich Results Features
- **Priorität:** P1
- **Beschreibung:** Matrix URL × Feature.
- **Ausgabe:** `Address`, danach eine Spalte je Feature mit Wert `Valid`/`Warnings`/`Errors`/leer.
- **Akzeptanzkriterien:**
  - [ ] Seite mit gültigem Breadcrumb und fehlerhaftem Product → `Breadcrumb`=Valid, `Product`=Errors.
- **Testfixture:** `out-structured-data`.

### OUT-104 PageSpeed: Opportunities Summary
- **Priorität:** P2
- **Beschreibung:** Aggregierte Lighthouse/PSI-Verbesserungsvorschläge (Insight- und Diagnose-Audits mit Einsparangabe) über alle URLs. Führend; INTEG-145 verweist hierher. Die Einsparwerte stammen aus denselben Audit-Feldern wie die Spalten des PageSpeed-Tabs (INTEG-137/138).
- **Eingaben/Konfiguration:** PSI-API oder lokale Lighthouse-Integration.
- **Ausgabe:** `Audit`, `URLs`, `% of Total`, `Total Savings (ms)`, `Average Savings (ms)`, `Total Savings (Bytes)`, `Average Savings (Bytes)`. Kanonischer Schlüssel der ersten Spalte ist `Audit` (nicht `Opportunity`) **(OS-Festlegung,** Entscheidung zu R-13): Lighthouse führt die früheren „Opportunities“ inzwischen als „Insights“, und `Audit` ist derselbe Schlüssel wie im Ressourcen-Export OUT-114 und im Detail-Export OUT-021; der Report-Name bleibt „PageSpeed Opportunities Summary“. Je Audit eine Zeile; Werte, die ein Audit nicht liefert (z. B. keine Byte-Einsparung), bleiben leer.
- **Akzeptanzkriterien:**
  - [ ] Zählregel (übernommen aus INTEG-145): `URLs` zählt nur URLs mit Einsparung > 0; URLs mit Einsparung 0 oder ohne PSI-Daten zählen nicht und gehen auch nicht in den Mittelwert ein. Beispiel: 3 URLs mit 100, 200 und 0 ms beim Audit „Render Blocking Requests“ → `URLs`=2, `Total Savings (ms)`=300, `Average Savings (ms)`=150.
  - [ ] `% of Total` = `URLs` / Anzahl URLs mit PSI-Daten × 100.
  - [ ] Audit ohne Byte-Angabe → `Total Savings (Bytes)` und `Average Savings (Bytes)` leer, nicht 0.
  - [ ] Werte = Summe/Mittel der Einzelwerte aus dem PageSpeed-Tab (Golden aus Mock-PSI-Antworten).
- **Testfixture:** Mock-PSI-Endpunkt mit 3 festen Antworten (Einsparungen 100/200/0 ms für Render-Blocking, eine Antwort mit Byte-Einsparung für „Reduce Unused JavaScript“).

### OUT-105 PageSpeed: CSS Coverage Summary
- **Priorität:** P2
- **Beschreibung:** Je CSS-Datei ungenutzte Bytes über alle Seiten.
- **Ausgabe:** `Resource`, `Pages`, `Total Bytes`, `Average Unused Bytes`, `Average Unused %`.
- **Akzeptanzkriterien:**
  - [ ] Eine CSS-Datei auf 3 Seiten mit 40/50/60 % ungenutzt → `Pages`=3, `Average Unused %`=50.
- **Testfixture:** Mock-PSI.

### OUT-106 PageSpeed: JavaScript Coverage Summary
- **Priorität:** P2
- **Beschreibung:** Wie OUT-105 für JavaScript-Dateien (ungenutzter JS-Code je Datei über alle Seiten, auf denen sie geladen wird).
- **Ausgabe:** `Resource`, `Pages`, `Total Bytes` (übertragene Größe der Datei), `Average Unused Bytes`, `Average Unused %`, `Sample URL`.
- **Akzeptanzkriterien:**
  - [ ] `app.js` (100 000 Bytes) auf 4 Seiten mit 20/40/60/80 % ungenutzt → `Pages`=4, `Average Unused %`=50, `Average Unused Bytes`=50000.
  - [ ] Inline-Skripte (ohne eigene URL) werden nicht aufgeführt; Skripte mit Query-String-Varianten (`app.js?v=1`, `app.js?v=2`) sind zwei getrennte Zeilen **(OS-Festlegung)**.
  - [ ] Seiten ohne Coverage-Daten (PSI-Fehler) fließen nicht in `Pages` und Mittelwerte ein.
  - [ ] Sortierung: `Average Unused Bytes` × `Pages` absteigend (größtes Einsparpotenzial zuerst) **(OS-Festlegung)**.
- **Testfixture:** Mock-PSI mit 4 Antworten für `app.js` und einer Fehlerantwort; zusätzlich `vendor.js?v=1`/`?v=2`.

### OUT-107 JavaScript: Chrome Console Log Summary
- **Priorität:** P2
- **Beschreibung:** Aggregation der Browser-Konsolenmeldungen beim Rendern.
- **Eingaben/Konfiguration:** JS-Rendering + JS-Fehlerberichte aktiv.
- **Ausgabe:** `Level`, `Message` (URL-spezifische Teile wie Query-Strings nicht normalisiert), `Pages`, `% of Total`, `Sample URL`.
- **Akzeptanzkriterien:**
  - [ ] Dieselbe Fehlermeldung auf 4 Seiten → eine Zeile `Pages`=4.
- **Testfixture:** `out-render`.

### OUT-108 Mobile
- **Priorität:** P2
- **Beschreibung:** Gesammelter Export aller Mobile-Filter und der Lighthouse-Detaildaten.
- **Ausgabe:** Ordner/Arbeitsmappe mit je einer Tabelle pro Mobile-Filter (Spalten wie Tab, führend TECH-231) sowie `lighthouse_details` im Langformat `Address`, `Audit`, `Score`, `Display Value`, `Item`, `Snippet`: eine Zeile je Detailzeile (`details.items`) eines Mobile-Audits mit CSS-Selektor bzw. Ressourcen-URL in `Item` und HTML-Ausschnitt in `Snippet`; Audits ohne Detailzeilen ergeben eine Zeile mit leerem `Item`/`Snippet` (OS-Festlegung zu R-16; ersetzt die frühere Aufteilung je Audit aus TECH-231). Priorität P2 bleibt (OS-Festlegung): Der Mobile-Tab (TECH-231, P1) und sein Tab-Export (OUT-020) decken die Parität ab; der gesammelte Report ist eine Zusatzfunktion.
- **Akzeptanzkriterien:**
  - [ ] Anzahl Tabellen = Anzahl Mobile-Filter + 1.
  - [ ] `tap-targets`/`target-size`-Audit mit 3 zu kleinen Zielen → 3 Zeilen in `lighthouse_details` mit unterschiedlichem `Item`.
- **Testfixture:** `out-mobile` mit lokalem Lighthouse-Mock.

### OUT-109 Accessibility: Violations Summary
- **Priorität:** P1
- **Beschreibung:** Aggregierte Barrierefreiheitsverstöße.
- **Ausgabe:** `Issue`, `Rule ID`, `Guideline`, `Impact`, `Priority`, `URLs`, `% of Total`, `Sample URL` (gemeinsame Spaltennamen wie TECH-259).
- **Akzeptanzkriterien:**
  - [ ] `URLs` zählt Seiten, nicht Vorkommen (Seite mit 5 Bildern ohne alt zählt 1).
  - [ ] Sortierung: Priority (High→Low), dann `URLs` absteigend.
- **Testfixture:** `out-a11y`.

### OUT-110 HTTP Header Summary
- **Priorität:** P1
- **Beschreibung:** Alle vorkommenden Response-Header-Namen mit Anzahl der URLs, die sie liefern.
- **Eingaben/Konfiguration:** HTTP-Header-Extraktion aktiv.
- **Ausgabe:** `Header Name` (Kleinbuchstaben, normalisiert), `URLs`, `% of Total`, `Distinct Values`, `Most Common Value`.
- **Akzeptanzkriterien:**
  - [ ] Header `Cache-Control` auf 10 von 12 URLs → `URLs`=10, `% of Total`=83.33.
  - [ ] `Content-Type` und `content-type` werden zusammengeführt.
- **Testfixture:** `out-headers`.

### OUT-111 Cookie Summary
- **Priorität:** P1
- **Beschreibung:** Eindeutige Cookies über den Crawl – nützlich für Datenschutz-/Consent-Prüfungen.
- **Eingaben/Konfiguration:** Cookie-Extraktion aktiv (mit Rendering für JS-Cookies).
- **Ausgabe:** `Cookie Name`, `Domain`, `Path`, `Persistence` (`Session` oder Laufzeit in Tagen), `Secure`, `HttpOnly`, `SameSite`, `Type`, `First-Party`, `URLs`, `Sample URL`. Namen und Wertelisten der mit UI-043 gemeinsamen Spalten (`Secure`, `HttpOnly`, `SameSite`, `Type` `HTTP`/`JavaScript`, `First-Party` `True`/`False`) sind führend in UI-043; `Persistence`, `URLs` und `Sample URL` sind Report-eigene Aggregatspalten.
- **Akzeptanzkriterien:**
  - [ ] Eindeutigkeit über (Name, Domain, Path, Secure, HttpOnly, SameSite, Type): derselbe Cookie auf 12 Seiten → eine Zeile `URLs`=12.
  - [ ] Cookie mit `Max-Age=86400` → `Persistence`=1.
  - [ ] Drittanbieter-Cookie aus iframe/Skript einer anderen Domain → `First-Party`=False.
- **Testfixture:** `out-headers`.

### OUT-112 Report-Verfügbarkeit und Voraussetzungen
- **Priorität:** P0
- **Beschreibung:** Das Reports-Menü zeigt jeden Report; nicht erfüllte Voraussetzungen (Integration fehlt, Extraktion nicht aktiviert, Crawl-Analyse fehlt, kein Rendering) werden im Menü und im Export-Log benannt, statt kommentarlos leere Dateien zu erzeugen.
- **Eingaben/Konfiguration:** Crawl-Konfiguration.
- **Ausgabe:** Tooltip/Hinweistext; CLI: Warnung je betroffenem Report.
- **Akzeptanzkriterien:**
  - [ ] Cookie Summary ohne Cookie-Extraktion → Menüeintrag mit Hinweis „Cookies-Extraktion aktivieren“; CLI-Warnung `report 'Cookie Summary' requires cookie extraction`.
  - [ ] Hreflang-Reports ohne abgeschlossene Crawl-Analyse → Hinweis + Angebot „Analyse jetzt starten“.
- **Testfixture:** `out-basic-site` mit minimaler Konfiguration.

### OUT-113 Reports im Vergleichs- und Segmentkontext
- **Priorität:** P1
- **Beschreibung:** Reports berücksichtigen das aktive Segment (OUT-011). Im Vergleichsmodus beziehen sich Reports auf den aktuellen Crawl; der Crawl Overview zeigt zusätzlich die Vergleichsspalten (OUT-147). Der Report „Crawl Changes“ (OUT-154) gibt es nur im Vergleichsmodus; er liest beide Crawls.
- **Akzeptanzkriterien:**
  - [ ] Redirect Chains mit Segment „Shop“ → nur Ketten, deren Start-URL im Segment liegt.
  - [ ] Im Vergleichsmodus ist das Reports-Menü bedienbar.
- **Testfixture:** `out-segments`.

### OUT-114 PageSpeed: Ressourcen je Audit
- **Priorität:** P2
- **Beschreibung:** Je Insight- bzw. Diagnose-Audit lässt sich die Liste der beanstandeten Ressourcen über alle URLs exportieren (Menü Reports → PageSpeed → <Audit>; CLI `--save-report "PageSpeed:<Audit>"`). Führend für diesen Export; INTEG-144 verweist hierher (Inhalt des Detail-Tabs einer URL bleibt INTEG-144, dessen Export OUT-021). Datengrundlage sind die Detailzeilen (`details.items`) des Audits in der gespeicherten PSI/Lighthouse-Antwort.
- **Eingaben/Konfiguration:** PSI-API oder lokale Lighthouse-Integration; Audit-Auswahl im Menü bzw. per CLI-Namen (Namensliste über `--list-reports`, OUT-203).
- **Ausgabe:** Spalten in dieser Reihenfolge: `Source Page`, `URL`, `Size (Bytes)`, `Potential Savings`, danach angehängt `Audit`, `Potential Savings (ms)` **(OS-Festlegung;** die ersten vier Spalten entsprechen der Ressourcentabelle des Detail-Tabs nach INTEG-144, die beiden angehängten sind OpenSpider-Zusätze). Eine Zeile je (Seite, Ressource). Feldzuordnung: `URL` aus `url`; `Size (Bytes)` aus `totalBytes`, ersatzweise `transferSize` bzw. `resourceSize`; `Potential Savings` aus `wastedBytes` in Bytes; liefert ein Audit nur eine Zeiteinsparung (`wastedMs`), steht diese in `Potential Savings (ms)` und `Potential Savings` bleibt leer. Sortierung: `Source Page`, dann Reihenfolge der Detailzeilen.
- **Akzeptanzkriterien:**
  - [ ] Export „Reduce Unused JavaScript“ für `psi-heavy.json` hat die Kopfzeile `Source Page,URL,Size (Bytes),Potential Savings,Audit,Potential Savings (ms)` und je Seite eine Zeile pro Skript aus `details.items`.
  - [ ] „Render Blocking Requests“ (nur Zeiteinsparung je Ressource) → Wert in `Potential Savings (ms)`, `Potential Savings` leer.
  - [ ] Detailzeilen ohne `url` (z. B. Elemente bei Layout-Verschiebungen) erscheinen mit Selektor bzw. Snippet in `URL` und leerer `Size (Bytes)`.
  - [ ] Seiten ohne PSI-Daten oder ohne Befund für das Audit erzeugen keine Zeile; gibt es für das Audit keine einzige Zeile, greift OUT-012.
  - [ ] Die Zeilenzahl je Seite stimmt mit der Ressourcentabelle des Detail-Tabs „Lighthouse Details“ für dasselbe Audit überein.
- **Testfixture:** `psi-heavy.json` (Fachspec 04) im PSI-Mock des `os-testsite`.

---

## 5 XML- und Bild-Sitemaps

Menü „Sitemaps → XML-Sitemap erstellen / Bild-Sitemap erstellen“. Ein Assistent mit Reitern (Seiten, Last Modified, Priority, Change Frequency, Images, Hreflang) erzeugt die Dateien aus dem aktuellen Crawl. Die Einstellungen werden mit dem Konfigurationsprofil gespeichert (OUT-220), damit CLI/Scheduling sie verwenden.

### OUT-120 XML-Sitemap-Erzeugung (Grundfunktion)
- **Priorität:** P0
- **Beschreibung:** Erzeugt aus den gecrawlten internen URLs eine XML-Sitemap nach sitemaps.org-Protokoll 0.9.
- **Eingaben/Konfiguration:** Zielordner, Dateiname (Standard `sitemap.xml`), Optionen OUT-121 – OUT-130.
- **Ausgabe:** `<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">` mit je `<url><loc>…</loc>[<lastmod>][<changefreq>][<priority>]</url>`.
- **Akzeptanzkriterien:**
  - [ ] Standardinhalt: nur interne URLs mit Inhaltstyp HTML, Status 200, indexierbar (kein noindex, nicht kanonisiert auf andere URL, nicht paginiert mit `rel=prev`).
  - [ ] Jede URL erscheint höchstens einmal; Reihenfolge = Crawl-Reihenfolge (Start-URL zuerst).
  - [ ] Robots-gesperrte und nicht gecrawlte URLs (Queue) sind nie enthalten.
  - [ ] Die generierte Datei validiert gegen das offizielle XSD `sitemap.xsd`.
  - [ ] Externe URLs sind nie enthalten.
- **Testfixture:** `out-sitemap-gen`: 20 Seiten inkl. noindex, kanonisierter Seite, paginierter Seite `/list?page=2` mit `rel=prev`, PDF, 301, 404, 500, robots-gesperrtem Pfad, Seite mit Umlaut-Pfad `/über-uns`, Seite mit `&` im Query.

### OUT-121 Sitemap: Einschlussoptionen für Seiten
- **Priorität:** P1
- **Beschreibung:** Optional können zusätzliche Kategorien aufgenommen werden.
- **Eingaben/Konfiguration:** Checkboxen (Standard jeweils aus): „Noindex-Seiten einschließen“, „Kanonisierte Seiten einschließen“, „Paginierte URLs (rel=prev) einschließen“, „PDFs einschließen“, „3xx einschließen“, „4xx einschließen“, „5xx einschließen“, „Keine Antwort einschließen“.
- **Ausgabe:** erweiterte `<url>`-Liste.
- **Akzeptanzkriterien:**
  - [ ] Je Checkbox einzeln aktiviert → genau die betreffenden URLs kommen hinzu (6 Einzeltests + 1 Test „alle an“).
  - [ ] „PDFs einschließen“ nimmt interne `application/pdf`-URLs mit Status 200 auf (sofern nicht zugleich eine andere Ausschlussregel greift).
  - [ ] Eine Seite, die sowohl noindex als auch kanonisiert ist, erscheint nur, wenn beide Optionen aktiv sind.
- **Testfixture:** `out-sitemap-gen`.

### OUT-122 Sitemap: Ausschluss einzelner URLs
- **Priorität:** P1
- **Beschreibung:** Vor der Erzeugung entfernte URLs (Rechtsklick → Entfernen im Internal-Tab) sowie per Exclude/robots.txt nie gecrawlte URLs erscheinen nicht. Zusätzlich: Regex-Ausschlussliste im Assistenten **(OS-Festlegung)**.
- **Eingaben/Konfiguration:** Regex-Liste (eine pro Zeile).
- **Akzeptanzkriterien:**
  - [ ] Nach Entfernen von `/impressum` fehlt sie in der Sitemap.
  - [ ] Regex `\?sessionid=` schließt URLs mit Session-Parameter aus.
- **Testfixture:** `out-sitemap-gen`.

### OUT-123 Sitemap: lastmod
- **Priorität:** P1
- **Beschreibung:** Optionales `<lastmod>`-Element.
- **Eingaben/Konfiguration:** „lastmod aufnehmen“ (Standard an); Quelle: „Server-Antwort (Last-Modified)“ (Standard; ohne gültigen Header ersatzweise das Crawl-Datum der URL) oder „Benutzerdefiniertes Datum“ (Datumsauswahl, gilt für alle URLs).
- **Ausgabe:** W3C-Datetime als Datum ohne Uhrzeit, `YYYY-MM-DD` (UTC), für alle Quellen (K2-88, SQ-11, wie die Referenz; vorher Server-Datum mit Uhrzeit).
- **Akzeptanzkriterien:**
  - [ ] Server-Quelle: URL mit `Last-Modified: Tue, 01 Oct 2026 10:00:00 GMT` → `<lastmod>2026-10-01</lastmod>`.
  - [ ] Server-Quelle, kein/ungültiger Header → `<lastmod>` = Datum des Abrufs dieser URL (Crawl-Datum, UTC; K2-88, SQ-11; vorher kein `<lastmod>`). Tests verwenden eine feste Uhr; der Paritätsvergleich normalisiert das Crawl-Datum.
  - [ ] Benutzerdefiniert `2026-10-13` → alle URLs `<lastmod>2026-10-13</lastmod>`.
  - [ ] Abgeschaltet → kein `<lastmod>` in der Datei.
- **Testfixture:** `out-sitemap-gen` (Fixture-Server setzt `Last-Modified` für ausgewählte Seiten, einmal ungültig).

### OUT-124 Sitemap: priority
- **Priorität:** P1
- **Beschreibung:** Optionales `<priority>` abhängig von der Crawl-Tiefe.
- **Eingaben/Konfiguration:** „priority aufnehmen“ (Standard an); Tabelle Tiefe → Wert, Standard: Tiefe 0 = 1.0, 1 = 0.9, 2 = 0.8, 3 = 0.7, 4 = 0.6, ≥ 5 = 0.5; alle Werte editierbar (0.0–1.0, Schritt 0.1).
- **Ausgabe:** `<priority>` mit einer Nachkommastelle.
- **Akzeptanzkriterien:**
  - [ ] Start-URL → `1.0`; Seite in Tiefe 7 → `0.5`.
  - [ ] Eingabe 1.5 wird abgelehnt (Validierungsfehler im Dialog).
  - [ ] Im Listenmodus (alle Tiefe 0) → alle `1.0`, mit Hinweis im Dialog.
  - [ ] Abgeschaltet → kein `<priority>`.
- **Testfixture:** `out-sitemap-gen` (lineare Kette bis Tiefe 7).

### OUT-125 Sitemap: changefreq
- **Priorität:** P1
- **Beschreibung:** Optionales `<changefreq>`.
- **Eingaben/Konfiguration:** „changefreq aufnehmen“ (Standard an); Basis: „Last-Modified-Header“ oder „Crawl-Tiefe“. Bei Tiefe: Tabelle Tiefe → Wert (`always`, `hourly`, `daily`, `weekly`, `monthly`, `yearly`, `never`), Standard **(OS-Festlegung)**: 0 = daily, 1 = daily, 2 = weekly, 3 = weekly, ≥ 4 = monthly.
- **Ausgabe:** `<changefreq>`.
- **Akzeptanzkriterien:**
  - [ ] Basis Last-Modified: Header jünger als 24 h relativ zum Crawlzeitpunkt der URL → `daily`; Header fehlt oder ungültig → `daily` (Ersatz Crawl-Datum wie OUT-123; K2-88, SQ-11; vorher `monthly`); Header älter → `monthly` (prüft M2-N11 am Referenzexport `sitemap_default.xml`, Abweichung an den Planer).
  - [ ] Grenzfall: genau 24 h → `monthly` (strikt „innerhalb der letzten 24 h“).
  - [ ] Basis Tiefe: Werte gemäß Tabelle.
- **Testfixture:** `out-sitemap-gen` (Fixture-Server setzt `Last-Modified` dynamisch auf jetzt − 1 h bzw. jetzt − 48 h).

### OUT-126 Sitemap: Bilder in der XML-Sitemap
- **Priorität:** P1
- **Beschreibung:** Optional werden Bilder je Seite als `<image:image><image:loc>` aufgenommen (Namespace `http://www.google.com/schemas/sitemap-image/1.1`).
- **Eingaben/Konfiguration:** „Bilder aufnehmen“ (Standard aus); „Externe Bilder (CDN) einschließen per Regex“ (leer = keine externen); „Nur relevante Bilder mit bis zu N IMG-Inlinks aufnehmen“ (Standard deaktiviert; dient dem Ausschluss seitenweiter Logos/Icons). Die Schwelle ist – wie in der Referenz – eine inklusive Obergrenze der Anzahl einbindender Seiten (`IMG Inlinks`); OpenSpider übernimmt das Referenzverhalten unverändert. **[P2]** Option „noindex-Bilder ausschließen“.
- **Ausgabe:** `<url>` mit 0..n `<image:image>`.
- **Akzeptanzkriterien:**
  - [ ] Interne Bilder mit Status 200 werden der einbindenden Seite zugeordnet; Bilder mit Status ≠ 200 nie.
  - [ ] Externes Bild `https://cdn.test/p.jpg` erscheint nur, wenn es auf die Regex `cdn\.test` passt.
  - [ ] Schwelle N = 5: Logo auf 20 Seiten (20 IMG-Inlinks) fehlt, Produktbild mit 1 Inlink ist enthalten; Grenzfälle: Bild mit genau 5 IMG-Inlinks enthalten, mit 6 nicht.
  - [ ] Max. 1 000 Bilder je `<url>`; weitere werden ausgelassen und gewarnt.
  - [ ] [P2] Mit „noindex-Bilder ausschließen“ fehlen Bilder, deren Antwort `X-Robots-Tag: noindex` trägt.
- **Testfixture:** `out-sitemap-images`: Logo auf allen 20 Seiten, je Produktseite ein Bild, ein Banner auf genau 5 und eines auf genau 6 Seiten, ein CDN-Bild, ein 404-Bild.

### OUT-127 Bild-Sitemap (separat)
- **Priorität:** P1
- **Beschreibung:** Eigene Sitemap, die nur Seiten enthält, die mindestens ein aufzunehmendes Bild einbinden, mit den Bildern.
- **Eingaben/Konfiguration:** wie OUT-126 (Regex, Inlink-Schwelle); Seitenregeln wie OUT-120/121.
- **Ausgabe:** `image-sitemap.xml` **(OS-Festlegung** Standardname) im gleichen Format wie OUT-126.
- **Akzeptanzkriterien:**
  - [ ] Seiten ohne (verbleibende) Bilder fehlen.
  - [ ] Nur Bilder mit Status 200.
  - [ ] Aufteilung wie OUT-129.
- **Testfixture:** `out-sitemap-images`.

### OUT-128 Sitemap: hreflang
- **Priorität:** P1
- **Beschreibung:** Optional werden die gecrawlten hreflang-Annotationen jeder URL als `<xhtml:link rel="alternate" hreflang="…" href="…"/>` (Namespace `http://www.w3.org/1999/xhtml`) ausgegeben. OpenSpider erzeugt keine hreflang-Beziehungen, sondern übernimmt vorhandene aus HTML/HTTP-Header.
- **Eingaben/Konfiguration:** „hreflang aufnehmen“ (Standard aus); Option „nur hreflang-Ziele mit Status 200 und indexierbar“ (Standard an) **(OS-Festlegung)**.
- **Ausgabe:** `<xhtml:link>`-Elemente je `<url>`.
- **Akzeptanzkriterien:**
  - [ ] Seite `/de/` mit Annotationen de, en, x-default → 3 `xhtml:link`-Elemente inkl. Selbstverweis.
  - [ ] hreflang-Ziel 404 wird bei aktiver Filteroption nicht aufgenommen.
  - [ ] Ohne hreflang-Extraktion im Crawl ist die Option deaktiviert.
- **Testfixture:** `out-hreflang`.

### OUT-129 Sitemap: Aufteilung und Sitemap-Index
- **Priorität:** P0
- **Beschreibung:** Überschreitet die Sitemap die Protokollgrenzen, werden mehrere Dateien plus ein Sitemap-Index erzeugt.
- **Eingaben/Konfiguration:** max. URLs je Datei (Standard 49 999 wie Referenz, max. 50 000); Basis-URL, unter der die Dateien später liegen (für `<loc>` im Index; Standard = Start-URL-Origin + `/`).
- **Ausgabe:** `sitemap_1.xml`, `sitemap_2.xml`, … und `sitemap_index.xml` mit `<sitemapindex>`/`<sitemap><loc>` (+ `<lastmod>` = Erzeugungsdatum) **(OS-Festlegung** Dateinamen).
- **Akzeptanzkriterien:**
  - [ ] 49 999 URLs → genau eine Datei `sitemap.xml`, kein Index.
  - [ ] 50 000 URLs → zwei Dateien (49 999 + 1) und Index.
  - [ ] Zusätzlich wird geteilt, wenn eine Datei unkomprimiert 50 MiB (52 428 800 Bytes) überschreiten würde.
  - [ ] Index validiert gegen `siteindex.xsd`; `<loc>` sind absolute URLs.
  - [ ] Bild-Sitemaps zählen `<url>`-Einträge, nicht Bilder, für die Grenze.
- **Testfixture:** synthetischer Crawl mit 50 000 bzw. 120 000 URLs (generiert in `os-store`), plus Crawl mit sehr langen URLs für die Bytegrenze.

### OUT-130 Sitemap je Segment
- **Priorität:** P2
- **Beschreibung:** Bei konfigurierten Segmenten kann je Segment eine eigene Sitemap erzeugt werden, plus ein Index über alle.
- **Eingaben/Konfiguration:** „Nach Segment erstellen“ (nur sichtbar, wenn Segmente existieren).
- **Ausgabe:** `sitemap_<segment>.xml` (+ ggf. `_1`, `_2` bei Teilung), `sitemap_index.xml`.
- **Akzeptanzkriterien:**
  - [ ] Segmente Blog/Shop → zwei Sitemaps + Index; URLs ohne Segment landen in `sitemap_unsegmented.xml` **(OS-Festlegung)**.
  - [ ] Segmentnamen werden für Dateinamen bereinigt (OUT-008); Bindestriche bleiben erlaubt.
  - [ ] Die URL-Menge jeder Segment-Sitemap ist identisch mit der Segmentzuordnung aus CRAWL-134 (Spalte `Segment`), auch bei Segmenten mit der Bedingung „ist in keinem Segment“ (übernommen aus UI-068).
- **Testfixture:** `out-segments`.

### OUT-131 Sitemap: XML-Korrektheit und Kodierung
- **Priorität:** P0
- **Beschreibung:** Korrekte Kodierung von URLs und Sonderzeichen.
- **Akzeptanzkriterien:**
  - [ ] Datei ist UTF-8 mit Deklaration `<?xml version="1.0" encoding="UTF-8"?>`, ohne BOM.
  - [ ] `&` in URLs wird als `&amp;` maskiert; `'`, `"`, `<`, `>` ebenfalls entitätskodiert.
  - [ ] Nicht-ASCII-Pfade werden prozentkodiert ausgegeben (`/über-uns` → `/%C3%BCber-uns`), IDN-Hosts in Punycode.
  - [ ] URLs über 2 048 Zeichen werden ausgelassen und im Erzeugungsbericht aufgeführt.
  - [ ] Erzeugungsbericht nach Abschluss: Anzahl URLs, Dateien, ausgelassene URLs mit Grund.
- **Testfixture:** `out-sitemap-gen`.

### OUT-132 Sitemap: gzip-Ausgabe
- **Priorität:** P2
- **Beschreibung:** Optional werden Sitemaps zusätzlich bzw. ausschließlich als `.xml.gz` geschrieben **(OS-Festlegung)**.
- **Akzeptanzkriterien:**
  - [ ] Entpackte `.gz`-Datei ist byte-identisch zur unkomprimierten Variante.
  - [ ] Im Index verweisen `<loc>` auf `.xml.gz`, wenn nur komprimiert geschrieben wird.
- **Testfixture:** `out-sitemap-gen`.

### OUT-133 Sitemaps in CLI und Scheduling
- **Priorität:** P1
- **Beschreibung:** Sitemap- und Bild-Sitemap-Erzeugung nach Crawl-Ende per CLI (`--create-sitemap`, `--create-images-sitemap`) und als „lokale Datei“ in geplanten Aufgaben; Einstellungen kommen aus der Konfigurationsdatei.
- **Akzeptanzkriterien:**
  - [ ] `openspider --crawl {BASE} --headless --output-folder out --create-sitemap` erzeugt `out/sitemap.xml`, identisch zur UI-Erzeugung mit derselben Konfiguration.
  - [ ] Ohne Sitemap-Einstellungen in der Konfigurationsdatei gelten die Standardwerte aus OUT-120–OUT-129.
- **Testfixture:** `out-sitemap-gen`.

---

## 6 Crawl-Vergleich

Voraussetzung: beide Crawls liegen als Crawl-Dateien (`*.ospider`) in der Crawl-Verwaltung vor (OUT-228). Da OpenSpider immer auf Disk speichert (ein einziger disk-first-Modus, siehe CRAWL-124 und `docs/ARCHITECTURE.md`), entfällt die Einschränkung der Referenz auf einen bestimmten Speichermodus; keine Vergleichsfunktion (auch nicht Auto Compare, OUT-152) hängt von einem Speichermodus oder Leistungsprofil ab.

Führung: Dieser Abschnitt ist führend für den Vergleichsablauf (Auswahl, Klassifikation, URL-Mapping, Site Structure, Detailansichten, gespeicherte Vergleiche, Exporte, Auto Compare). CRAWL-032/034/035/036/144 und TECH-306/308 verweisen hierher. **Change Detection** (Elementauswahl, Standardwerte, Spalten, Elementfilter) ist führend in TECH-300 ff.; OUT-144 ist nur ein Verweis.

### OUT-140 Compare-Modus: Crawls auswählen
- **Priorität:** P1
- **Beschreibung:** Zwei Wege: (a) in der Crawl-Verwaltung zwei Crawls markieren → „Zum Vergleich wählen“; (b) Modus → Compare, dann je eine Auswahl für „Current“ und „Previous“. Standardmäßig ist der neuere Crawl „Current“.
- **Eingaben/Konfiguration:** zwei Crawl-IDs.
- **Ausgabe:** Vergleichsansicht (alle Tabs/Filter mit Vergleichsspalten).
- **Akzeptanzkriterien:**
  - [ ] Auswahl in beliebiger Reihenfolge → der Crawl mit späterem Startzeitpunkt ist Current.
  - [ ] Mehr oder weniger als zwei markierte Crawls → Schaltfläche deaktiviert.
  - [ ] Vergleich von Crawls unterschiedlicher Modi (Spider vs. Liste) ist erlaubt; ein Hinweis weist darauf hin.
  - [ ] Crawls aus älteren Dateiversionen werden vor dem Vergleich migriert (Schema-Migration aus `os-store`), ohne das Original zu verändern.
  - [ ] Der Vergleichsmodus ist bei jedem Leistungsprofil (CRAWL-124) wählbar; es gibt keinen Zustand, in dem er wegen der Speicherart deaktiviert ist (übernommen aus CRAWL-032).
- **Testfixture:** `out-compare`: Fixture-Server mit Umschalter `?version=1|2` (Version 2: 3 Seiten neu, 2 gelöscht, 1 Title geändert, 1 Seite neu noindex, 1 Link entfernt → Tiefe ändert sich, 1 404 behoben).

### OUT-141 Current/Previous tauschen
- **Priorität:** P1
- **Beschreibung:** Eine Schaltfläche vertauscht die Rollen der beiden Crawls; alle Zähler werden neu berechnet.
- **Akzeptanzkriterien:**
  - [ ] Nach Tausch gilt: neues Added = altes Removed, neues New = altes Missing (und umgekehrt).
- **Testfixture:** `out-compare`.

### OUT-142 Vergleichsklassifikation je Filter: Added / New / Removed / Missing
- **Priorität:** P1
- **Beschreibung:** Für jeden Filter F jedes Tabs wird jede URL u klassifiziert (nach URL-Mapping, OUT-146):
  - **Added:** u existiert in beiden Crawls, war in Previous **nicht** in F, ist in Current in F.
  - **New:** u existiert nur in Current und ist dort in F.
  - **Removed:** u existiert in beiden Crawls, war in Previous in F, ist in Current nicht mehr in F.
  - **Missing:** u existiert nur in Previous und war dort in F.
- **Ausgabe:** Im Overview je Filter die Spalten `Current`, `Previous`, `Change`, `Added`, `New`, `Removed`, `Missing`; in der Haupttabelle Unterfilter mit diesen vier Kategorien.
  - **OS-Festlegung (Orchestrator, 09.11.2026):** Die Haupttabelle zeigt die Spalten des Tabs wie im normalen Modus und dazu die Vergleichsspalten aus OUT-151; `Comparison Status` und (bei aktivem URL-Mapping) `Previous Address` stehen direkt nach `Address`. Anzeigetitel in der deutschen Oberfläche „Vergleichsstatus“ und „Vorherige Adresse“, in der englischen wie der kanonische Name.
  - **OS-Festlegung (Orchestrator, 09.11.2026):** Die Werte der Spalte `Comparison Status` erscheinen in der Tabellenanzeige in der Oberflächensprache, mit denselben Begriffen wie die Kategorien der Übersicht und des Unterfilters (de: „Hinzugefügt“, „Neu“, „Entfernt“, „Fehlend“, „Unverändert“). Filter, Sortierung, Kopieren und Exporte arbeiten weiter mit den kanonischen englischen Werten (`Added`/`New`/`Removed`/`Missing`/`Unchanged`, OUT-151).
- **Akzeptanzkriterien:**
  - [ ] Invariante für jeden Filter: `Current − Previous = Added + New − Removed − Missing` (automatisierter Test über alle Filter).
  - [ ] Im Fixture: Title-geänderte Seite, die vorher „Duplicate“ war und jetzt nicht mehr → Removed in „Page Titles:Duplicate“.
  - [ ] Behobene 404 → Removed in „Response Codes:Client Error (4xx)“ und Added in „Success (2xx)“.
  - [ ] Gelöschte Seite (nicht mehr verlinkt, nicht gecrawlt) → Missing in „Internal:HTML“.
  - [ ] Neue Seite → New in „Internal:HTML“.
  - [ ] „Existiert in Crawl“ heißt: URL wurde gecrawlt (nicht nur entdeckt/blockiert) **(OS-Festlegung)**.
  - [ ] Ein Klick auf eine Zahl im Overview listet genau diese URLs in der Haupttabelle (Zeilenzahl = angezeigte Zahl) (übernommen aus CRAWL-032).
- **Testfixture:** `out-compare`.

### OUT-143 Vergleich im Issues-Panel
- **Priorität:** P1
- **Beschreibung:** Das Issues-Panel zeigt je Issue Current, Previous, Change sowie die vier Kategorien; Diagramme reagieren auf Klicks auf Tabs/Filter/Zahlen.
- **Akzeptanzkriterien:**
  - [ ] Klick auf die Zahl in „Added“ eines Issues filtert die Haupttabelle auf genau diese URLs.
  - [ ] Issues, die nur in einem Crawl existieren, erscheinen mit 0 im anderen.
  - [ ] Je Issue gilt die Invariante aus OUT-142 (`Current − Previous = Added + New − Removed − Missing`), wobei der Filter „URL hat dieses Issue“ ist; die Zahlen stimmen mit dem Tab-Vergleich desselben Filters überein (übernommen aus CRAWL-144).
  - [ ] Die Issues-Ansicht ist nach `Change` sortierbar (übernommen aus CRAWL-144).
- **Testfixture:** `out-compare`.

### OUT-144 Change Detection (Elementänderungen)
- **Priorität:** P1
- **Verweis:** siehe TECH-300 (führend). Die Population (nur URLs, die nach URL-Mapping OUT-146 in beiden Crawls existieren) regelt TECH-301.

### OUT-145 Content Change Detection
- **Priorität:** P1
- **Verweis:** siehe TECH-305 (führend). Die gespeicherten Inhalts-Signaturen erlauben Vergleich, Auto Compare (OUT-152) und gespeicherte Vergleiche (OUT-150) ohne erneutes Crawlen.

### OUT-146 URL-Mapping zwischen Crawls
- **Priorität:** P1
- **Beschreibung:** Regex-Ersetzungsregeln bilden URLs des Previous-Crawls auf URLs des Current-Crawls ab, damit z. B. Staging gegen Live, Mobil-Unterordner gegen Desktop oder URLs mit/ohne Trailing Slash verglichen werden können. Gilt für Overview, Issues, Site Structure, Inlinks/Outlinks-Vergleich (OUT-149) und Change Detection. Führend; CRAWL-034 und TECH-306 verweisen hierher.
- **Eingaben/Konfiguration:** Liste von Regeln `Suchmuster (Regex)` → `Ersetzung` (mit `$1`-Gruppen), beliebig viele, nacheinander angewendet; Regex-Dialekt und Abgleichsemantik sind die gemeinsamen aus den Konventionen von 01-crawl.md (Lookahead/Lookbehind, Rückreferenzen `$1` im Ersatztext, Inline-Flags `(?i)`, nicht-gierige Quantoren; Backtracking-Engine `fancy-regex` mit Schrittlimit, Q23) und **Suche**, nicht Vollübereinstimmung: ersetzt wird jeder Treffer irgendwo in der URL (Entscheidung des Architekten zu R-02; die frühere Einschränkung auf die Crate `regex` ohne Lookaround aus TECH-306 entfällt). Reiter „Test“ mit Eingabe-URL und Ergebnisanzeige je Regel.
- **Akzeptanzkriterien:**
  - [ ] Regel `^https://staging\.site\.test` → `https://www.site.test`: Staging-URL `/a` wird als dieselbe URL wie Live-`/a` behandelt (keine New/Missing-Einträge für gemappte Paare).
  - [ ] Regel `/$` → `` (Trailing Slash entfernen) mappt `/a/` auf `/a`.
  - [ ] Mehrere Regeln werden in Listenreihenfolge auf das Ergebnis der vorigen angewendet.
  - [ ] Kollision (Entscheidung des Architekten): Bilden mehrere Previous-URLs auf dieselbe Current-URL ab, gewinnt die Previous-URL, deren erste passende Regel in der vom Nutzer festgelegten Listenreihenfolge am weitesten vorn steht. **OS-Festlegung** für Gleichstände: Eine Previous-URL, auf die keine Regel passt (unverändert identisch), geht jeder gemappten vor; bei gleicher erster Regel gewinnt die lexikographisch kleinste Previous-URL. Die unterlegenen Previous-URLs bleiben ohne Partner (wie nicht zugeordnete Previous-URLs); die Kollision wird als Warnung angezeigt und geloggt. Beispiel: Previous enthält `/a/` und `/a`, Regel `/$` → `` → `/a` (unverändert) wird `/a` zugeordnet, `/a/` bleibt ohne Partner.
  - [ ] Ungültige Regex → Fehlermeldung im Dialog mit Fehlerposition, Speichern nicht möglich.
  - [ ] Lookaround wird unterstützt: Regel `^https://staging\.site\.test(?=/shop/)` → `https://www.site.test` bildet `https://staging.site.test/shop/a` auf `https://www.site.test/shop/a` ab und lässt `https://staging.site.test/blog/b` unverändert.
  - [ ] Suchsemantik: Regel `/amp/` → `/` (ohne Anker) bildet `https://site.test/de/amp/a` auf `https://site.test/de/a` ab.
  - [ ] Überschreitet eine Regel beim Abgleich das Schrittlimit der Engine, bleibt die URL für diese Regel unverändert, und die Regel wird einmal je Vergleich als Warnung gemeldet (kein Absturz, kein Hängen).
  - [ ] Test-Reiter zeigt für eine Eingabe die URL nach jeder einzelnen Regel und das Endergebnis sofort an (übernommen aus CRAWL-034).
  - [ ] Regel `^https://staging\.site\.test/(.*)$` → `https://www.site.test/$1` ordnet `https://staging.site.test/a` der URL `https://www.site.test/a` zu (Rückverweis-Test, übernommen aus TECH-306).
- **Testfixture:** `out-compare-mapping`: zwei Hosts (`staging.site.test`, `www.site.test`) mit gleicher Struktur, einer mit Trailing Slashes.

### OUT-147 Crawl-Overview-Export im Vergleichsmodus
- **Priorität:** P1
- **Beschreibung:** Reports → Crawl Overview im Vergleichsmodus enthält je Zeile die Vergleichswerte.
- **Ausgabe:** `Section`, `Item`, `Current`, `Previous`, `Change`, `Change %`, `Added`, `New`, `Removed`, `Missing`, zusätzlich Kopfblock mit Name/Startzeit beider Crawls.
- **Akzeptanzkriterien:**
  - [ ] Werte identisch mit dem Overview-Panel im Vergleichsmodus.
  - [ ] Change-Detection-Elemente erscheinen als eigener Abschnitt.
- **Testfixture:** `out-compare`.

### OUT-148 Site Structure im Vergleich
- **Priorität:** P2
- **Beschreibung:** Verzeichnisbaum mit Änderungen je Verzeichnis: Anzahl hinzugekommener (grüner Pfeil nach oben) und weggefallener URLs (roter Pfeil nach unten), aufklappbar bis zur URL; Darstellung der Crawl-Tiefen-Verschiebung.
- **Ausgabe:** Export: `Path`, `Current URLs`, `Previous URLs`, `New`, `Missing`, `Average Depth Current`, `Average Depth Previous`.
- **Akzeptanzkriterien:**
  - [ ] Verzeichnis `/blog/` mit 2 neuen und 1 fehlenden URL → `New`=2, `Missing`=1.
  - [ ] Die Werte eines Elternverzeichnisses sind die Summe seiner Unterverzeichnisse plus seiner eigenen URLs (übernommen aus CRAWL-035).
- **Testfixture:** `out-compare`.

### OUT-149 Inlinks/Outlinks-Vergleich im unteren Fenster
- **Priorität:** P1
- **Beschreibung:** Die Detail-Tabs Inlinks und Outlinks zeigen im Vergleichsmodus die Links der markierten URL aus beiden Crawls und welche Links zwischen den Crawls hinzugekommen, weggefallen oder gleich geblieben sind. Führend; CRAWL-144 verweist hierher (Layout des Detail-Tabs: UI-Bereich).
- **Eingaben/Konfiguration:**
  - Auswahl „Quelle“: `Auto` (Standard; folgt der Auswahl Current/Previous des oberen Fensters), `Current`, `Previous`.
  - Auswahl „Änderung“ mit den Filterwerten **Alle / Hinzugefügt / Entfernt / Ankertext geändert / Unverändert** (kanonische Werte für Export/CLI: `All` / `Added` / `Removed` / `Anchor Changed` / `Unchanged`); Standard „Alle“.
- **Definitionen (OS-Festlegung):** Für eine URL u, die nach URL-Mapping (OUT-146) in beiden Crawls existiert, seien L_C(u) und L_P(u) die Mengen ihrer Outlinks (bzw. Inlinks) im Current- bzw. Previous-Crawl. **Link-Identität (Entscheidung des Architekten):** das Tripel (Quelle, Ziel, `Type`), Quelle und Ziel jeweils nach URL-Mapping; der Ankertext gehört nicht zur Identität (bei Outlinks ist die Quelle u, bei Inlinks das Ziel u). Hinzugefügt = L_C(u) \ L_P(u); Entfernt = L_P(u) \ L_C(u). Für Links in L_C(u) ∩ L_P(u) wird der Ankertext verglichen (Menge der Ankertexte aller Link-Instanzen mit dieser Identität, Whitespace normalisiert, case-sensitiv): unterscheidet er sich, ist die Änderungsart **„Ankertext geändert“**, sonst **„Unverändert“**. Ein Ankertextwechsel ist also nie „Entfernt + Hinzugefügt“. Änderungen nur von Position, `Rel` oder Pfad lassen einen Link „Unverändert“ (OS-Festlegung).
- **Ausgabe (Spalten/Filter/Export):** Spalten des Standard-Linkformats (OUT-031) plus `Link Change` (`Added`/`Removed`/`Anchor Changed`/`Unchanged`), `Crawl` (`Current`/`Previous`; bei „Entfernt“ stets `Previous`) und `Previous Anchor` (genau bei `Anchor Changed` gefüllt) **(OS-Festlegung** der Spaltennamen). Der Export (OUT-021) trägt dieselben Zusatzspalten.
- **Akzeptanzkriterien:**
  - [ ] Seite, deren Tiefe von 1 auf 2 stieg, zeigt unter Inlinks → „Entfernt“ den entfernten Link von der Startseite.
  - [ ] Quelle „Auto“: Steht das obere Fenster auf „Previous“, zeigt der Inlinks-Tab die Inlinks des Previous-Crawls; Umschalten oben auf „Current“ wechselt den Tab ohne weitere Aktion (übernommen aus CRAWL-144).
  - [ ] Fixture v1→v2: `/start` verliert den Link auf `/b` und erhält einen auf `/a` → Outlinks von `/start` mit „Hinzugefügt“ = {`/a`}, „Entfernt“ = {`/b`}; mit „Alle“ und Quelle „Current“ fehlt `/b` (übernommen aus CRAWL-144).
  - [ ] Nur geänderter Ankertext (`/c` „Alt“ → „Neu“) erscheint unter „Ankertext geändert“ mit `Link Change`=`Anchor Changed`, `Anchor`=Neu und `Previous Anchor`=Alt, nicht unter „Hinzugefügt“/„Entfernt“/„Unverändert“.
  - [ ] Derselbe Ziel-URL-Wechsel bei gleichem Ankertext (Link von `/start` auf `/b` → `/a`) ist „Entfernt“ + „Hinzugefügt“; ein Wechsel nur des `Type` (Hyperlink → Canonical auf dasselbe Ziel) ebenfalls.
  - [ ] Invariante je URL: |Alle mit Quelle Current| = |Hinzugefügt| + |Ankertext geändert| + |Unverändert|; |Alle mit Quelle Previous| = |Entfernt| + |Ankertext geändert| + |Unverändert|.
  - [ ] Existiert u nur in einem Crawl, sind „Hinzugefügt“/„Entfernt“/„Ankertext geändert“/„Unverändert“ ausgegraut und die Tabelle zeigt die Links des Crawls, in dem u existiert (übernommen aus CRAWL-144).
  - [ ] Export der Ansicht enthält die Spalten `Link Change`, `Crawl` und `Previous Anchor` (OUT-151).
- **Testfixture:** `out-compare` erweitert um `/start` (Linkwechsel `/b` → `/a`) und `/c` (geänderter Ankertext).

### OUT-150 Gespeicherte Vergleiche
- **Priorität:** P2
- **Beschreibung:** Ein abgeschlossener Vergleich wird als kleiner Eintrag (Modus „Compare“) in der Crawl-Verwaltung gespeichert und kann ohne Neuberechnung geöffnet werden. Gespeichert werden: IDs von Current/Previous, die beim Vergleich wirksame Compare-Konfiguration (Change-Detection-Elemente, Content-Schwelle, URL-Mapping) und die berechneten Klassifikationen (Added/New/Removed/Missing je Filter, Change-Detection-Ergebnisse) **(OS-Festlegung** des Umfangs).
- **Eingaben/Konfiguration:** automatisch nach jedem Vergleich (UI, Auto-Compare, CLI); Name Standard `<Projekt> – <Name Current> vs <Name Previous>`, umbenennbar.
- **Ausgabe:** Zeile in der Crawl-Verwaltung mit `Mode`=Compare, `Name`, `Current`, `Previous`, `Created`, `Size on Disk`, `Status` (`Available`/`Unavailable`).
- **Akzeptanzkriterien:**
  - [ ] Öffnen eines gespeicherten Vergleichs dauert < 2 s bei 100 000 URLs je Crawl (keine Neuberechnung; Messung: es wird keine Klassifikationsabfrage über beide Crawls ausgeführt).
  - [ ] Zähler des geöffneten gespeicherten Vergleichs sind identisch mit denen unmittelbar nach der Berechnung (Golden-Vergleich des Crawl Overview, OUT-147).
  - [ ] Nach Änderung der Compare-Konfiguration (z. B. URL-Mapping) bleibt der gespeicherte Vergleich unverändert; „Neu berechnen“ erzeugt einen neuen Eintrag, der alte bleibt erhalten.
  - [ ] Wird einer der beiden Crawls gelöscht, ist der Vergleich als `Unavailable` markiert und nicht öffenbar; der Löschdialog des Crawls nennt die betroffenen Vergleiche namentlich.
  - [ ] Filter „Anzeigen: Vergleiche“ in der Crawl-Verwaltung listet nur Vergleiche; „Anzeigen: Crawls“ keine.
  - [ ] Speicherbedarf eines gespeicherten Vergleichs ≤ 10 % der Summe beider Crawl-Dateien (Fixture mit 100 000 URLs).
- **Testfixture:** `out-compare`; synthetisches Crawlpaar mit je 100 000 URLs für Lade- und Größenmessung.

### OUT-151 Tab-Exporte im Vergleichsmodus
- **Priorität:** P1
- **Beschreibung:** Exporte aus der Vergleichsansicht enthalten die Vergleichsinformation.
- **Ausgabe:** Zusätzlich zu den Spalten des Current-Crawls die Spalte `Comparison Status` (`Added`/`New`/`Removed`/`Missing`/`Unchanged`), bei aktivem URL-Mapping die Spalte `Previous Address` (übernommen aus TECH-306) und für Change-Detection-Ansichten die `Previous …`-Spalten (Spaltennamen gemäß TECH-301) **(OS-Festlegung)**. Detail-Exporte von Inlinks/Outlinks tragen `Link Change`, `Crawl`, `Previous Anchor` (OUT-149).
  - **OS-Festlegung (Orchestrator, 09.11.2026):** Die Spaltennamen in Exportdateien bleiben kanonisch (`Comparison Status`, `Previous Address`); nur die Anzeige in der Oberfläche ist übersetzt (de: „Vergleichsstatus“, „Vorherige Adresse“; en: wie kanonisch, OUT-142). In Exportdateien stehen `Comparison Status` und `Previous Address` nach den Spalten des Current-Crawls (anders als in der Tabellenanzeige, wo sie direkt nach `Address` folgen, OUT-142).
- **Akzeptanzkriterien:**
  - [ ] Export „Response Codes:Client Error (4xx)“ mit Unterfilter „Removed“ enthält die behobene 404 mit `Comparison Status`=Removed.
  - [ ] Bulk-Exporte und Reports beziehen sich auf den Current-Crawl.
  - [ ] Change-Detection-Exporte berücksichtigen den aktiven Umschalter „Changed“/„Not Changed“ (übernommen aus TECH-307).
  - [ ] Mit Mapping-Regel Staging→Live enthält ein Export für die Live-URL `/a` die Spalte `Previous Address` = Staging-URL `/a`.
- **Unterfilter im CLI-Export (Nutzerentscheidung 04.11.2026):** `--export-tabs` nimmt den Unterfilter als **dritten Namensteil** `"<Tab>:<Filter>:<Unterfilter>"` mit `<Unterfilter>` ∈ {`Added`, `New`, `Removed`, `Missing`} (ohne Beachtung der Groß-/Kleinschreibung, Leerräume um `:` wie OUT-192). Begründung der Form: Sie folgt der Hierarchie Tab › Filter › Unterfilter der Vergleichsansicht (OUT-142) und der verschachtelten Schreibweise mit weiterem `:` aus OUT-193; ein eigenes Flag gälte dagegen für alle Einträge einer Liste und könnte verschiedene Unterfilter in einem Aufruf nicht ausdrücken. Kein Tab-Exportname enthält heute einen dritten Namensteil (Golden `tests/golden/out-cli/list_exports.txt`), die Form ist also eindeutig: Ein Name wird zuerst vollständig als `Tab:Filter` aufgelöst; nur wenn das scheitert und der letzte Teil einer der vier Unterfilter ist, gilt er als Unterfilter des davor stehenden `Tab:Filter`. Ohne dritten Teil bleibt der Export wie oben (Zeilen des Filters im Current-Crawl mit `Added`/`New`/`Unchanged`). Dateiname nach OUT-008 aus dem vollständigen Namen (`internal_html_missing.csv`).
  - **Herkunft der Werte (Festlegung):** `Missing`-Zeilen tragen die Werte des Previous-Crawls (die URL existiert nur dort), `Address` ist die Previous-Adresse. `Removed`-Zeilen tragen die Werte der Current-Zeile des Partners, wie der Unterfilter „Removed“ der Haupttabelle (OUT-142: ein Klick listet genau diese Zeilen) – sie zeigen also, warum die URL den Filter verlassen hat (z. B. die behobene 404 jetzt mit 200). **Abweichung vom Auftrag:** Der Auftrag nannte für `Removed` die Previous-Werte; das widerspräche der Vergleichsansicht, deren Export OUT-151 beschreibt. Den Vorher-Zustand je URL liefert der Report „Crawl Changes“ (OUT-154). Orchestrator-Entscheid 04.11.2026 (Fable-Empfehlung): Current-Werte bleiben.
  - **Validierung (OUT-203):** Ein Unterfilter ohne `--crawl-comparison`, `--project-crawl-comparison`/`--compare-with-previous` → Exit-Code 2 vor dem Crawl mit Hinweis auf diese Flags. Ein Unterfilter am Tab „Change Detection“ → Exit-Code 2 (dort gibt es nur „Changed“/„Not Changed“, TECH-307). Unbekannter dritter Teil (z. B. `Internal:HTML:Gone`) → unbekannter Name nach OUT-203.
  - **Auto Compare ohne Vorlauf:** Ist beim ersten Crawl eines Projekts (OUT-152) kein Vergleich vorhanden, wird ein Export mit Unterfilter mit Warnung übersprungen (keine Datei, Exit-Code 0).
- **Akzeptanzkriterien (Nutzerentscheidung 04.11.2026):**
  - [ ] `--crawl-comparison <v1> <v2> --export-tabs "Internal:HTML:Missing"` erzeugt `internal_html_missing.csv` mit genau den gelöschten Seiten, `Comparison Status`=`Missing` und Werten des Previous-Crawls (z. B. `Title 1` aus v1).
  - [ ] `--export-tabs "Response Codes:Client Error (4xx):Removed"` enthält die behobene 404 mit `Comparison Status`=`Removed` (CLI-Pendant zum ersten Kriterium oben).
  - [ ] Invariante über die Exportdateien eines Filters: Zeilen ohne Unterfilter = `Current`; `Added` + `New` + `Unchanged` (Spalte `Comparison Status` ohne Unterfilter) = `Current`; und `Current − Previous = Added + New − Removed − Missing` mit den Zeilenzahlen der vier Unterfilter-Exporte (wie OUT-142).
  - [ ] `--export-tabs "Internal:HTML:Missing"` ohne Vergleich → Exit-Code 2, der Fixture-Server zählt 0 Requests; `"Change Detection:Page Titles:Added"` → Exit-Code 2.
  - [ ] `"internal : html : MISSING"` und `"Internal:HTML:Missing"` sind gleichwertig.
- **Testfixture:** `out-compare`; für die CLI-Kriterien `tests/fixtures/out-b3-diff` (`v1` → `v2`, README dort).

### OUT-152 Automatischer Vergleich (Auto Compare) für Scheduling und CLI
- **Priorität:** P1
- **Beschreibung:** Nach Abschluss eines geplanten oder CLI-Crawls, der einem Projekt zugeordnet ist, wird automatisch mit dem vorherigen Crawl desselben Projekts verglichen (Vergleich inkl. Change Detection gemäß TECH-300); der Vergleich wird gespeichert (OUT-150) und fließt in die Abschlussmail ein (Änderungstabelle, OUT-282). Führender Eintrag; CRAWL-036 und TECH-308 verweisen hierher. Keine Abhängigkeit von einem Speichermodus oder Leistungsprofil: OpenSpider hat nur den disk-first-Modus (CRAWL-124, `docs/ARCHITECTURE.md`), die Option ist daher nie wegen der Speicherart gesperrt.
- **Eingaben/Konfiguration:** Aufgaben-Option „Mit vorherigem Crawl vergleichen“ im Aufgabendialog (Reiter Spider, OUT-160/162), Standard aus, nur aktivierbar, wenn ein Projektname gesetzt ist (übernommen aus TECH-308); CLI `--project-crawl-comparison ["true"|"false"]` bzw. das gleichwertige OS-Alias `--compare-with-previous` (vergleicht mit dem vorherigen Crawl des Projekts; Syntax und Validierung in OUT-202) bzw. `--crawl-comparison <idA> <idB>`. Change-Detection-Einstellungen (TECH-300) und URL-Mapping (OUT-146) kommen aus der Aufgaben- bzw. übergebenen Konfigurationsdatei.
- **Ausgabe:** gespeicherter Vergleich „<Projekt> – <Name Current> vs <Name Previous>“ (OUT-150); Änderungstabelle in der Abschlussmail (OUT-282); optional Exporte im Vergleichskontext (OUT-151).
- **Akzeptanzkriterien:**
  - [ ] Erster Crawl eines Projekts → kein Vergleich, Hinweis im Log, kein Fehler (Exit-Code 0); die Mail enthält nur die Issue-Tabelle und den Hinweis „kein vorheriger Crawl“ (übernommen aus TECH-308).
  - [ ] Zweiter Crawl → genau ein gespeicherter Vergleich mit Current = Crawl 2, Previous = Crawl 1.
  - [ ] Verglichen wird immer mit dem **unmittelbar vorherigen, erfolgreich abgeschlossenen** Crawl desselben Projekts (nach Startzeit); abgebrochene, fehlgeschlagene und unvollständige Crawls werden übersprungen: Lauf 3 nach abgebrochenem Lauf vergleicht mit Lauf 2 (übernommen aus TECH-308).
  - [ ] Fehlt beim aktuellen Crawl die Crawl-Analyse (TECH-005), wird sie vor dem Vergleich automatisch ausgeführt (übernommen aus TECH-308).
  - [ ] Die Zahlen des automatisch erzeugten Vergleichs (Overview und Mail-Änderungstabelle) stimmen mit einem manuell geöffneten Vergleich derselben zwei Crawls überein (Golden-Vergleich des Crawl Overview, OUT-147) (übernommen aus TECH-308).
  - [ ] Change-Detection-Einstellungen und URL-Mapping kommen aus der Konfigurationsdatei.
  - [ ] Exporte (`--export-tabs` usw.) können im Vergleichskontext ausgeführt werden und tragen dann die Vergleichsspalten (OUT-151).
  - [ ] Die Option ist bei jedem Leistungsprofil aktivierbar; es gibt keine Fehlermeldung oder ausgegraute Option wegen eines Speichermodus.
  - [ ] `--project-crawl-comparison "true"` und `--compare-with-previous` erzeugen bei sonst gleichen Argumenten identische gespeicherte Vergleiche.
- **Testfixture:** `out-compare` via CLI-Integrationstest (zwei Läufe mit Projektname; dritter Lauf nach absichtlich abgebrochenem Lauf); SMTP-Mock für die Mail.

### OUT-153 Paritätsprüfungen über Vergleich
- **Priorität:** P2
- **Beschreibung:** Der Vergleich dient auch für Paritätschecks zwischen Konfigurationen: Mobil- vs. Desktop-User-Agent, rohes vs. gerendertes HTML, verschiedene User-Agents. Im Detail-Tab „Rendered Page“ lassen sich die Screenshots beider Crawls nebeneinander zeigen.
- **Akzeptanzkriterien:**
  - [ ] Zwei Crawls derselben Fixture mit unterschiedlichem User-Agent (Fixture liefert für Mobil-UA einen anderen Title) → Change Detection meldet die Title-Änderung.
  - [ ] Nebeneinander-Ansicht nur verfügbar, wenn beide Crawls Screenshots enthalten; fehlt in einem Crawl der Screenshot, zeigt die betreffende Hälfte „Kein Screenshot in diesem Crawl“ (übernommen aus CRAWL-144).
  - [ ] Beide Screenshots werden in gleicher Skalierung gezeigt; unter jedem Bild stehen Crawl-Name, Datum, User-Agent und Viewport (Anwendungsfall Desktop- vs. Mobil-Crawl) (übernommen aus CRAWL-144).
- **Testfixture:** `out-compare-ua`: Fixture-Server variiert Title nach User-Agent.

### OUT-154 Report „Crawl Changes“ (Änderungen seit dem letzten Crawl)
- **Priorität:** P1
- **Beschreibung (Nutzerentscheidung 04.11.2026):** Ein fertiger Report beantwortet die Frage „Was hat dieses Deployment geändert?“ in einer Tabelle: verschwundene Seiten, neue Fehlerseiten, neue Weiterleitungen und -ketten, geänderte Canonicals und Wechsel der Indexierbarkeit, je mit dem Wert vorher und nachher. Er vergleicht Current gegen Previous nach URL-Mapping (OUT-146) und braucht keine Change-Detection-Auswahl (TECH-300): Er rechnet seine Abschnitte immer vollständig. Name für `--save-report`, Export und MCP: `Crawl Changes` (Datei `crawl_changes.csv`, OUT-008); Name in der deutschen Oberfläche „Änderungen seit dem letzten Crawl“.
- **Eingaben/Konfiguration:** ein geöffneter Vergleich (App: Vergleichsmodus, Menü „Reports“; CLI: `--save-report "Crawl Changes"` zusammen mit `--crawl-comparison <id1> <id2>` oder `--project-crawl-comparison`/`--compare-with-previous`, OUT-202; MCP: `compare_crawls` mit `changes`, AGENT-015). Ohne Vergleich ist die Voraussetzung unerfüllt (OUT-112): CLI → Warnung `report 'Crawl Changes' requires a crawl comparison (--crawl-comparison or --compare-with-previous)`, Datei nur mit Kopfzeile, Exit-Code 0 (mit `--strict` 4, OUT-194); App → Menüeintrag ausgegraut mit Hinweis „Nur im Vergleichsmodus“.
- **Begriffe (OS-Festlegung):** *Seite* = interne URL mit Inhaltsklasse HTML; *existiert* wie OUT-142 (gecrawlt); *Fehler* = Status 4xx oder 5xx; *Weiterleitung* = URL mit Weiterleitungsziel (HTTP 3xx, Meta-Refresh- oder HTTP-Refresh-Weiterleitung auf eine andere URL, TECH-001 `Redirected`); *Weiterleitungsziel* = die aufgelöste Ziel-URL des ersten Sprungs; *Kettenlänge* = Zahl der Sprünge ab der URL bis zum ersten Ziel ohne Weiterleitung, bei einer Schleife die Zahl der Glieder im Umlauf (wie `Number of Redirects` in OUT-081/082), 0 ohne Weiterleitung; *Kette* = Kettenlänge ≥ 2 (wie OUT-082); *Canonical* = die maßgebliche Canonical-URL nach TECH-017, nur für Seiten erhoben, die in **beiden** Crawls HTML sind und deren Crawl Canonicals speichert (TECH-024); *Indexierbarkeit* = `Indexability` und die Menge der Gründe in `Indexability Status` wie TECH-304. Ziel-URLs des Previous-Crawls (Canonical, Weiterleitungsziel) werden vor dem Vergleich mit dem URL-Mapping abgebildet und nach TECH-002 verglichen.
- **Ausgabe:** Eine Zeile je URL und Änderungsart; eine URL kann in mehreren Abschnitten stehen (z. B. eine indexierbare Seite, die jetzt 404 liefert: `Page Gone`, `New Error` und `Indexability Changed`). Spalten in dieser Reihenfolge: `Change`, `Address`, `Previous Address`, `Status Code (Previous)`, `Status Code (Current)`, `Indexability (Previous)`, `Indexability (Current)`, `Indexability Status (Previous)`, `Indexability Status (Current)`, `Canonical (Previous)`, `Canonical (Current)`, `Redirect Target (Previous)`, `Redirect Target (Current)`, `Redirect Chain Length (Previous)`, `Redirect Chain Length (Current)`. `Address` ist die Current-Adresse, bei URLs ohne Partner die Previous-Adresse; `Previous Address` ist die Adresse im Previous-Crawl (leer bei URLs nur in Current). Werte eines Crawls, in dem die URL nicht existiert, bleiben leer. Sortierung: Abschnitte in der Reihenfolge der Tabelle, darin `Address` aufsteigend (Bytefolge). Die Spalte `Change` trägt die kanonischen englischen Werte; die Oberfläche zeigt die deutschen Namen.

  | `Change` (kanonisch) | Oberfläche (DE) | Bedingung |
  |---|---|---|
  | `Page Gone` | Seite verschwunden | Seite, im Previous-Crawl indexierbar (damit Status 2xx, TECH-001); im Current-Crawl existiert sie nicht (Missing, auch nur entdeckt oder gesperrt) oder antwortet mit Fehler oder ohne Antwort (`No Response`). |
  | `New Error` | Neue Fehlerseite | Interne URL mit Fehler im Current-Crawl, die im Previous-Crawl nicht existierte oder ohne Fehler antwortete (neue URL mit 404 ebenso wie eine früher erreichbare Seite). Hatte sie im Previous-Crawl keine Antwort (`No Response`, Status 0 oder leer), hat sie dort nicht „ohne Fehler geantwortet“ und ist nicht neu (wie `All Errors`, OUT-080). Keine Antwort im Current-Crawl ist kein `New Error` (Fehler = 4xx/5xx). |
  | `New Redirect` | Neue Weiterleitung | Interne URL mit Weiterleitung im Current-Crawl, die im Previous-Crawl nicht existierte oder nicht weiterleitete. |
  | `Redirect Target Changed` | Weiterleitungsziel geändert | Interne URL, die in beiden Crawls weiterleitet, mit verschiedenem Weiterleitungsziel. |
  | `New Redirect Chain` | Neue Weiterleitungskette | Interne URL mit Kette im Current-Crawl, deren Kettenlänge im Previous-Crawl kleiner als 2 war oder die dort nicht existierte. |
  | `Longer Redirect Chain` | Weiterleitungskette verlängert | Interne URL mit Kette in beiden Crawls, deren Kettenlänge gewachsen ist. |
  | `Canonical Changed` | Canonical geändert | Seite in beiden Crawls, deren Canonical sich unterscheidet; vorhanden → fehlend und fehlend → vorhanden sind Änderungen. |
  | `Indexability Changed` | Indexierbarkeit geändert | Interne URL in beiden Crawls, deren Indexierbarkeit sich nach TECH-304 geändert hat (Wechsel indexierbar ↔ nicht indexierbar oder der Menge der Gründe). Eine URL ohne Antwort im Current-Crawl existiert dort (gecrawlt, OUT-142) und wird mit ihrem Grund `No Response` (TECH-001) verglichen: vorher indexierbar → jetzt keine Antwort steht hier und unter `Page Gone`. |

- **Akzeptanzkriterien:**
  - [ ] Fixture `out-b3-diff` v1 → v2 ergibt genau diese Zeilen (Golden `tests/golden/out-b3-diff/crawl_changes.csv`): `Page Gone` = {`/broken-now.html`, `/gone.html`}; `New Error` = {`/broken-now.html`, `/broken.html`}; `New Redirect` = {`/c3`, `/moved.html`, `/r1b`}; `Redirect Target Changed` = {`/c2`, `/r1`, `/rt`}; `New Redirect Chain` = {`/c2`, `/r1`}; `Longer Redirect Chain` = {`/c1`} mit Kettenlänge 2 → 3; `Canonical Changed` = {`/canon.html`} mit `/canon-a.html` → `/canon-b.html` bei unverändert `Canonicalised`; `Indexability Changed` = {`/broken-now.html`, `/fixed.html`, `/idx.html`, `/moved.html`}.
  - [ ] Unveränderte Seiten (`/keep.html`) und neue Seiten ohne Fehler und ohne Weiterleitung (`/new.html`) erscheinen nicht.
  - [ ] Für jede URL der Change-Detection-Population (TECH-301) stimmen `Canonical Changed` und `Indexability Changed` mit den Change-Detection-Filtern `Canonical` (TECH-309) und `Indexability` (TECH-304) überein; `Redirect Target Changed` ist genau der Teil des Filters `Redirect Target` (TECH-310), der in beiden Crawls weiterleitet.
  - [ ] Abschnitt `Page Gone` ohne Mapping: jede Zeile ohne `Status Code (Current)` gehört zu „Internal:HTML“ › Missing (OUT-142).
  - [ ] Mit Mapping-Regel Staging→Live erscheinen gleich gebliebene Canonical- und Weiterleitungsziele nicht als Änderung (Ziele des Previous-Crawls werden abgebildet).
  - [ ] Nach Tausch von Current und Previous (OUT-141) beziehen sich alle Abschnitte auf die neue Richtung: v2 → v1 liefert `/new.html` als `Page Gone`, `/gone.html` erscheint dort nicht.
  - [ ] CLI: `--crawl-comparison <v1> <v2> --save-report "Crawl Changes"` erzeugt `crawl_changes.csv` gleich der Golden-Datei; mit `--compare-with-previous` liefert der zweite Projektcrawl dieselben Zeilen. Ohne Vergleich: Warnung wie oben, Datei mit Kopfzeile, Exit-Code 0.
  - [ ] App: Im Vergleichsmodus ist „Reports › Änderungen seit dem letzten Crawl“ bedienbar und schreibt dieselbe Datei wie die CLI; ohne Vergleich ist der Eintrag ausgegraut.
  - [ ] MCP: `compare_crawls(a, b, changes=true)` liefert dieselben Zeilen paginiert (AGENT-005) und die Anzahl je Abschnitt.
  - [ ] Segment (OUT-011): Zeilen mit Current-Partner zählen, wenn die Current-URL im Segment liegt; Zeilen ohne Current-Partner, wenn die Previous-URL im Segment des Previous-Crawls liegt (OS-Festlegung).
  - [ ] Pause des Ressourcenwächters hält die Berechnung an, Abbruch liefert keinen Teil-Report (CRAWL-129).
- **Testfixture:** `tests/fixtures/out-b3-diff` (`v1` Previous, `v2` Current; Inhalt im README dort).

---

## 7 Geplante Crawls (Scheduling)

Menü „Datei → Zeitplanung“. Eine geplante Aufgabe ist eine Beschreibung von Crawl + Exporten, die zu festgelegten Zeiten über das CLI-Binary (Abschnitt 8) ausgeführt wird. Aufgaben werden als Dateien im Einstellungsordner gespeichert (`schedules/<task-id>.ostask`, TOML) **(OS-Festlegung)**. Dieser Abschnitt (OUT-160 … OUT-171) ist führend für geplante Crawls; CRAWL-136 verweist hierher. Die Priorität P1 aus der Architektenentscheidung K-5 ist eine **Einzelpriorität**: Jede ID dieses Abschnitts hat P1; weitergehende Teilumfänge tragen am Kriterium **[P2]** (Entscheidung zu R-08).

### OUT-160 Aufgabenverwaltung
- **Priorität:** P1
- **Beschreibung:** Liste aller Aufgaben mit Spalten `Name`, `Project`, `Next Run`, `Interval`, `Mode`, `Start URL / Source`, `Status` (Valid/Invalid), `Last Run Result`. Aktionen: Hinzufügen, Bearbeiten, Duplizieren, Löschen, Jetzt ausführen, Historie, Export/Import (OUT-170), Abbrechen eines laufenden Laufs (OUT-171); ab 1.x zusätzlich Pausieren/Fortsetzen eines laufenden Laufs (OUT-286, Nachtrag 09.11.2026).
- **Eingaben/Konfiguration:** Der Aufgabendialog folgt der Reiterstruktur der Referenz, damit Umsteiger Felder an gewohnter Stelle finden (übernommen aus CRAWL-136): **Allgemein** (OUT-161), **Spider** (Modus, Start-URL/Listenquelle, Konfiguration, Auth-Profil, „Mit vorherigem Crawl vergleichen“ – OUT-162/163, OUT-152), **APIs** (OUT-164), **Exporte** mit den Unterreitern in dieser Reihenfolge: Ausgabe-Einstellungen, Tabellen, Lokale Dateien, Looker Studio (OUT-165/210), Vorlagen/Presets (OUT-075), Benachrichtigungen (OUT-281). Ein Schalter „Headless“ existiert nicht (OUT-163).
- **Ausgabe:** Aufgabendateien + Registrierung beim Betriebssystem (OUT-166).
- **Akzeptanzkriterien:**
  - [ ] Hinzufügen → Aufgabe erscheint in der Liste und ist im OS-Planer registriert (macOS: LaunchAgent-plist vorhanden; Windows: Eintrag in der Aufgabenplanung unter Ordner `\OpenSpider\`).
  - [ ] Löschen → Aufgabendatei und OS-Registrierung entfernt; bereits erzeugte Crawls und Exporte bleiben erhalten.
  - [ ] „Jetzt ausführen“ startet sofort einen Lauf, unabhängig vom Zeitplan, und verschiebt den nächsten geplanten Lauf nicht.
  - [ ] Bearbeiten einer Aufgabe während sie läuft wirkt erst auf den nächsten Lauf.
  - [ ] Der Aufgabendialog zeigt genau die Reiter Allgemein, Spider, APIs, Exporte; der Reiter Exporte genau die sechs Unterreiter in der oben genannten Reihenfolge (übernommen aus CRAWL-136).
- **Testfixture:** Integrationstest mit gemocktem OS-Planer-Adapter (Trait in `os-cli`), plus manueller Test je OS pro Release.

### OUT-161 Aufgabe: Allgemein (Name, Projekt, Zeitplan)
- **Priorität:** P1
- **Beschreibung:** Grunddaten einer Aufgabe.
- **Eingaben/Konfiguration:** `Task Name` (Pflicht, eindeutig), `Project Name` (Projektordner in der Crawl-Verwaltung und Drive, Standard = Taskname), `Description`, `Start Date/Time`, `Interval` ∈ {Einmalig, Täglich, Wöchentlich, Monatlich}; **[P2]** erweitert **(OS-Festlegung)**: Stündlich, alle N Tage, Wochentage-Auswahl.
- **Ausgabe:** Zeitplan im OS-Planer.
- **Akzeptanzkriterien:**
  - [ ] Startzeitpunkt in der Vergangenheit → Validierungsfehler bei „Einmalig“; bei wiederkehrenden Intervallen wird der nächste zukünftige Termin berechnet und angezeigt.
  - [ ] Monatlich mit Starttag 31 → in Monaten mit weniger Tagen am letzten Tag des Monats.
  - [ ] Wöchentlich → gleicher Wochentag und gleiche Uhrzeit wie Startzeitpunkt.
  - [ ] Zeitangaben gelten in lokaler Zeit; bei Sommerzeitumstellung läuft ein täglicher 02:30-Lauf am Umstellungstag genau einmal (in der fehlenden Stunde: 03:30; in der doppelten: beim ersten Auftreten).
  - [ ] Taskname mit für Dateinamen unzulässigen Zeichen wird für Ordnernamen bereinigt (OUT-008), Anzeige bleibt unverändert.
  - [ ] [P2] Intervall „Stündlich“, „alle N Tage“ und Wochentage-Auswahl erzeugen korrekte nächste Termine (Unit-Tests von `next_run` inkl. DST).
- **Testfixture:** Unit-Tests der Terminberechnung (`next_run(start, interval, now, tz)`) mit Zeitzone `Europe/Berlin` inkl. DST-Daten 2026-03-29 und 2026-10-25.

### OUT-162 Aufgabe: Crawl-Einstellungen
- **Priorität:** P1
- **Beschreibung:** Was und wie gecrawlt wird.
- **Eingaben/Konfiguration:** `Mode` ∈ {Spider, List, **[P2]** APIs}; Spider: Start-URL (Eingabefeld bietet die zuletzt gecrawlten URLs an, dieselbe Liste wie CRAWL-001; übernommen aus CRAWL-136); List: Quelle = lokale Datei (`.txt`, `.csv`, `.xlsx`), XML-Sitemap-URL (auch Sitemap-Index; mehrere Sitemap-URLs zulässig) oder Google-Sheet-URL (mit Drive-Konto für private Tabellen); `Config File` (`.osconfig`, optional – sonst Standardkonfiguration); `Auth Profile` (`.osauth`, optional).
- **Akzeptanzkriterien:**
  - [ ] List-Quelle Sitemap-URL → alle `<loc>` der Sitemap (rekursiv über Index) werden gecrawlt.
  - [ ] List-Quelle Google-Sheet → URLs der ersten Spalte des ersten Blatts werden gecrawlt (Kopfzeile wird erkannt, wenn die erste Zelle keine URL ist).
  - [ ] Fehlt die Konfigurationsdatei zur Laufzeit → Lauf schlägt mit Status „Failed: config not found“ fehl, Historie-Eintrag, keine Teil-Exporte.
  - [ ] Auth-Profil mit Formular-Login → der geplante Crawl erreicht die geschützte Seite (Status 200 statt Weiterleitung zum Login).
  - [ ] Das Start-URL-Feld zeigt nach zwei manuellen Crawls beide URLs als Vorschläge (jüngste zuerst) (übernommen aus CRAWL-136).
  - [ ] [P2] Modus APIs: der Lauf ruft nur API-Daten für die Liste ab und sendet keine Requests an die gelisteten URLs (wie OUT-208).
- **Testfixture:** `out-schedule`: Fixture mit Sitemap-Index, Login-Formular-Bereich (Cookie-Session) und Basic-Auth-Bereich.

### OUT-163 Aufgabe: Ausführungsart (headless)
- **Priorität:** P1
- **Beschreibung:** Geplante Läufe laufen immer ohne Benutzeroberfläche über das CLI-Binary **(OS-Festlegung;** in der Referenz muss „Headless“ für Exporte aktiviert sein – bei OpenSpider entfällt die Wahl). **[P2]** Optional „Nach Abschluss in der App öffnen“: Ist die Desktop-App geöffnet, wird der Crawl dort als Benachrichtigung angeboten.
- **Akzeptanzkriterien:**
  - [ ] Ein geplanter Lauf öffnet kein Fenster und stiehlt keinen Fokus.
  - [ ] Läuft die Desktop-App, beeinflusst der geplante Lauf deren Crawl nicht (getrennte Prozesse, getrennte Crawl-Dateien).
  - [ ] [P2] Mit „Nach Abschluss in der App öffnen“ und geöffneter App erscheint nach Laufende genau ein In-App-Hinweis mit „Öffnen“.
- **Testfixture:** manueller Test je OS.

### OUT-164 Aufgabe: API-Datenquellen
- **Priorität:** P1
- **Beschreibung:** Pro Aufgabe wählbar, welche Integrationen während des Crawls abgefragt werden: GA4, Search Console, PageSpeed Insights, Majestic, Ahrefs, Moz. Zugangsdaten stammen aus den globalen Integrationseinstellungen (Schlüsselbund).
- **Akzeptanzkriterien:**
  - [ ] Aktivierte Integration ohne gültige Zugangsdaten → Aufgabe „Invalid“ mit Grund.
- **Testfixture:** Mock-Endpunkte.

### OUT-165 Aufgabe: Exporte
- **Priorität:** P1
- **Beschreibung:** Exporte nach Crawl-Ende.
- **Eingaben/Konfiguration:**
  - *Ausgabe:* Ziel lokal (Ordner) und/oder Google Drive (Konto); Überschreiben vs. Zeitstempel-Ordner (OUT-013); Format CSV/XLSX/XLS/Google Sheets; Konsolidierung in eine Arbeitsmappe (OUT-007); Leere überspringen (OUT-012); Segment (OUT-011).
  - *Tabellen:* Auswahlbaum Tabs/Filter, Bulk-Exporte, Reports (wie Multi-Export, OUT-075) oder Auswahl eines gespeicherten Presets.
  - *Lokale Dateien:* Crawl-Datei zusätzlich als `.ospider` in den Exportordner kopieren (die Crawl-Verwaltung speichert ohnehin, OUT-228), XML-Sitemap, Bild-Sitemap.
  - *Looker Studio:* OUT-210.
- **Ausgabe:** Ordner `<Output>/<Project>/<Task>/[<Zeitstempel>/]` bzw. Drive `OpenSpider/<Project>/<Task>/[<Zeitstempel>/]`.
- **Akzeptanzkriterien:**
  - [ ] Ein Lauf mit Internal:All (CSV), Redirect Chains und XML-Sitemap erzeugt drei Dateien im Zielordner.
  - [ ] Drive-Fehler (z. B. Quota) lässt lokale Exporte unberührt; Lauf-Status „Completed with warnings“.
  - [ ] Ist eine Auswahl-Position nicht verfügbar (z. B. PSI-Report ohne PSI), wird sie übersprungen und in der Historie aufgeführt.
- **Testfixture:** `out-basic-site` + Drive-Mock.

### OUT-166 Ausführung über den Betriebssystem-Planer
- **Priorität:** P1
- **Beschreibung:** Damit geplante Crawls ohne geöffnete App laufen, registriert OpenSpider jede Aufgabe beim OS-Planer, der `openspider --run-task <task-id>` aufruft **(OS-Festlegung)**.
  - macOS: LaunchAgent (`~/Library/LaunchAgents/de.openspider.task.<id>.plist`) mit `StartCalendarInterval`; läuft im Benutzerkontext, wenn der Benutzer angemeldet ist.
  - Windows: Aufgabe in der Aufgabenplanung (Ordner `\OpenSpider\`), Trigger gemäß Intervall, „Nur ausführen, wenn Benutzer angemeldet ist“ (Standard); **[P2]** Option „Unabhängig von der Anmeldung ausführen“ (verlangt Kennworteingabe im Windows-Dialog – OpenSpider speichert kein Windows-Kennwort).
- **Eingaben/Konfiguration:** Optionen: „Auch im Akkubetrieb ausführen“ (Standard an, wie im Referenzstand), „Verpasste Läufe nach dem Aufwachen nachholen“ (Standard an: höchstens **ein** Nachhol-Lauf), **[P2]** „Rechner zum Ausführen aufwecken“ (Windows).
- **Akzeptanzkriterien:**
  - [ ] Bei geschlossener App startet der Lauf zur geplanten Zeit (± 60 s).
  - [ ] Nach Ruhezustand über 3 verpasste tägliche Termine → genau ein Nachhol-Lauf.
  - [ ] Keine Begrenzung der Laufzeit durch den Planer (Windows: Option „Aufgabe beenden, falls länger als“ deaktiviert) – lange Crawls werden nicht abgebrochen.
  - [ ] Nach einem App-Update zeigen registrierte Aufgaben auf das neue Binary (stabiler Pfad oder Neuregistrierung beim ersten Start nach Update).
  - [ ] Deinstallation entfernt alle OS-Registrierungen.
  - [ ] Aufgabe „täglich 03:00“ erzeugt plist bzw. Task-XML identisch zu den Golden-Dateien `daily-0300.plist` / `daily-0300.xml`; die Task-XML enthält `DisallowStartIfOnBatteries=false`, `StopIfGoingOnBatteries=false` und `ExecutionTimeLimit=PT0S` (übernommen aus CRAWL-136).
  - [ ] [P2] „Unabhängig von der Anmeldung ausführen“ und „Rechner zum Ausführen aufwecken“ setzen die entsprechenden Felder der Task-XML (Golden-Varianten).
- **Testfixture:** Adapter-Unit-Tests (erzeugtes plist/Task-XML gegen Golden unter `tests/golden/out-schedule/`), manueller Release-Test.

### OUT-167 Parallele und überlappende Läufe
- **Priorität:** P1
- **Beschreibung:** Jeder Lauf ist ein eigener Prozess. Verschiedene Aufgaben dürfen gleichzeitig laufen. Für dieselbe Aufgabe gilt die Option „Überlappung“: `Parallel starten` (Referenzverhalten, Standard) oder `Überspringen, wenn noch aktiv` **(OS-Festlegung)**.
- **Akzeptanzkriterien:**
  - [ ] Zwei Aufgaben zur selben Minute → zwei Prozesse, zwei Crawl-Dateien, keine Sperrkonflikte.
  - [ ] „Überspringen“ aktiv und Vorlauf läuft noch → Historie-Eintrag „Skipped: previous run still active“.
  - [ ] Gleichzeitige Läufe schreiben nie in dieselbe Exportdatei (Zeitstempel-Ordner bzw. Dateisperre mit Wartezeit ≤ 60 s, danach Fehler).
- **Testfixture:** Integrationstest mit zwei CLI-Prozessen auf `out-basic-site` (verlangsamt).

### OUT-168 Gültigkeitsprüfung von Aufgaben
- **Priorität:** P1
- **Beschreibung:** Jede Aufgabe zeigt den Status `Valid` oder `Invalid` mit Grund, geprüft beim Speichern, beim App-Start und vor jedem Lauf.
- **Akzeptanzkriterien:**
  - [ ] Gründe mindestens: Konfigurationsdatei fehlt, Auth-Profil fehlt, Listen-Datei fehlt, ungültige Start-URL, Ausgabeordner nicht beschreibbar, Google-Konto nicht verbunden/abgelaufen, E-Mail-Konto fehlt für Benachrichtigung, OS-Registrierung fehlt.
  - [ ] Ungültige Aufgabe wird vom Planer trotzdem aufgerufen, beendet sich aber sofort mit Historie-Eintrag und (falls möglich) Fehlermail.
- **Testfixture:** Unit-Tests je Grund.

### OUT-169 Laufhistorie
- **Priorität:** P1
- **Beschreibung:** Je Aufgabe eine Historie der Läufe.
- **Ausgabe:** Spalten `Started`, `Finished`, `Duration`, `Result` (`Completed`, `Completed with warnings`, `Failed`, `Skipped`, `Aborted`), `URLs Crawled`, `Crawl ID`, `Output Folder`, `Message`; Aktion „Log öffnen“ (Lauf-Logdatei, OUT-241).
- **Akzeptanzkriterien:**
  - [ ] Abgestürzter Lauf (Prozess beendet ohne Abschluss) erscheint beim nächsten App-Start als `Failed` mit „unexpected termination“.
  - [ ] Historie hält mindestens die letzten 100 Läufe je Aufgabe; ältere werden verworfen.
  - [ ] Listenmodus-Läufe erscheinen ebenso in der Historie (Referenzfehler in einer früheren Version behoben).
- **Testfixture:** CLI-Integrationstest mit absichtlich abgebrochenem Prozess.

### OUT-170 Aufgaben exportieren/importieren
- **Priorität:** P1
- **Beschreibung:** Aufgaben können als Datei exportiert und auf einem anderen Rechner importiert werden.
- **Ausgabe:** `.ostask`-Datei (TOML) ohne Geheimnisse; referenzierte Konfigurations-/Auth-Dateien optional als `.zip` mitgepackt.
- **Akzeptanzkriterien:**
  - [ ] Import auf anderem Rechner → Aufgabe erscheint; fehlende Abhängigkeiten (Google-Konto, E-Mail-Konto, Pfade) führen zu `Invalid` mit konkretem Grund.
  - [ ] Exportdatei enthält keine Tokens/Kennwörter (Test sucht nach bekannten Testgeheimnissen).
- **Testfixture:** Roundtrip-Test.

### OUT-171 Laufende geplante Crawls abbrechen
- **Priorität:** P1
- **Beschreibung:** Aus der Aufgabenliste kann ein laufender Lauf abgebrochen werden; **[P2]** optional maximale Laufzeit je Aufgabe **(OS-Festlegung;** die Referenz unterstützt das laut Hersteller noch nicht).
- **Eingaben/Konfiguration:** **[P2]** „Max. Laufzeit (h)“, Standard leer.
- **Akzeptanzkriterien:**
  - [ ] Abbruch → Prozess beendet sich innerhalb 10 s, Crawl wird gespeichert, Status `Aborted`, Exporte werden nicht erzeugt.
  - [ ] [P2] Max. Laufzeit erreicht → Crawl wird gestoppt, Exporte des bisherigen Stands werden erzeugt, Status `Completed with warnings`.
  - [ ] Ein laufender geplanter Crawl bietet keine Aktion „Pausieren“ (Parität: ein geplanter Lauf ist ein Prozess ohne Oberfläche), nur „Abbrechen“ (übernommen aus CRAWL-136).
  - **Nachtrag 09.11.2026 (1.x):** Mit OUT-286 erhält ein laufender geplanter Lauf zusätzlich „Pausieren“ und „Fortsetzen“, sofern sein Prozess erreichbar ist (Steuerkanal dort). Das Kriterium oben gilt bis zur Umsetzung von OUT-286 und danach nur für nicht erreichbare Besitzerprozesse; „Abbrechen“ bleibt unverändert.
- **Testfixture:** `out-basic-site` mit großer Verzögerung.

---

## 8 Kommandozeile / Headless

Ein eigenes Konsolen-Binary `openspider` (Crate `os-cli`) bietet alle Funktionen ohne Benutzeroberfläche. Optionsnamen orientieren sich an der Referenz, damit vorhandene Skripte leicht übertragbar sind; abweichende/zusätzliche Optionen sind markiert. Alle Exporte nutzen die kanonischen Namen aus Abschnitt 0. Dieser Abschnitt ist führend für die Kommandozeile (K-6: OUT-180 … OUT-195 tragen als **Einzelpriorität** je ID P0 – einzige begründete Ausnahme ist OUT-187 mit P1, weil `--auth-config` Authentifizierungsprofile voraussetzt (OUT-223, P1) und ohne sie nicht umsetzbar ist; OUT-196 ff. haben eigene Prioritäten; Entscheidung zu R-08); CRAWL-135 verweist hierher, die crawl-seitige Wirkung einzelner Optionen (z. B. `--resolve`, `--resume`) beschreibt der jeweils genannte CRAWL-Eintrag. Exit-Codes gelten ausschließlich nach OUT-204.

### OUT-180 CLI-Binary und Aufruf je Plattform
- **Priorität:** P0
- **Beschreibung:** Bereitstellung eines echten Konsolenprogramms je Plattform.
  - macOS: `OpenSpider.app/Contents/MacOS/openspider` (Konsolenprogramm, blockiert bis zum Ende, Ausgabe im Terminal); Menüpunkt „Kommandozeilenwerkzeug installieren“ legt einen Symlink in `/usr/local/bin` (mit Admin-Rückfrage) oder `~/.local/bin` an.
  - Windows: `openspider.exe` (Subsystem Console) im Installationsordner neben der GUI; Installer bietet „Zu PATH hinzufügen“ an.
- **Akzeptanzkriterien:**
  - [ ] Aufruf aus Terminal/PowerShell blockiert bis zum Ende und liefert einen Exit-Code (OUT-204).
  - [ ] Das Binary startet ohne grafische Sitzung (SSH auf macOS; Windows-Dienstkonto) für Crawls ohne JS-Rendering.
  - [ ] Keine Abhängigkeit von installierter Java- oder Node-Laufzeit.
- **Testfixture:** CI-Smoke-Test `openspider --version` auf allen drei Zielplattformen.

### OUT-181 Hilfe und Version
- **Priorität:** P0
- **Beschreibung:** Selbstdokumentation.
- **Eingaben/Konfiguration:** `--help`; `--help export-tabs`, `--help bulk-export`, `--help save-report`, `--help export-custom-summary` listen alle gültigen Namen der jeweiligen Kategorie (aus derselben Registry wie die UI; Syntax wie Referenz); `--version`. Kurzformen `--list-exports` / `--list-reports` siehe OUT-203.
- **Akzeptanzkriterien:**
  - [ ] `--help export-tabs` listet jeden `Tab:Filter`, eine Zeile je Eintrag, maschinenlesbar mit `--json`.
  - [ ] `--version` gibt `openspider <semver> (<git-hash>, <os>-<arch>)` aus; mit Build-Nummer (UI-127, Nutzerentscheidung 09.11.2026) `openspider <semver> (Build <n>, <git-hash>, <os>-<arch>)`.
  - [ ] Liste ist vollständig: Registry-Test vergleicht Anzahl mit UI-Filterregistry.
  - [ ] `--help` listet alle Flags gruppiert (Quelle, Konfiguration, Vergleich, APIs, Exporte, OS-Erweiterungen); OS-Erweiterungen sind als solche markiert (übernommen aus CRAWL-135).
- **Testfixture:** Golden-Ausgabe `tests/golden/out-cli/help_export_tabs.txt`.

### OUT-182 `--crawl <url>`
- **Priorität:** P0
- **Beschreibung:** Startet einen Crawl im Spider-Modus ab der URL.
- **Akzeptanzkriterien:**
  - [ ] URL ohne Schema → `http://` wird ergänzt (wie UI), Hinweis auf stderr.
  - [ ] Ungültige URL → Exit-Code 2 vor Crawlbeginn.
  - [ ] Start-URL nicht erreichbar → Crawl endet regulär mit 1 URL (No Response), Exit-Code 0, Warnung **(OS-Festlegung:** kein Fehler, weil das ein gültiges Crawlergebnis ist); mit `--fail-on-start-error` Exit-Code 3. Der Zustand in der Verwaltung ist Fehlgeschlagen (CRAWL-128); Exporte und der Analyse-Hook laufen, die Analyse gilt als unvollständig, die App startet keine automatische Analyse (CRAWL-141; Entscheidung Fable, B4-RESUME).
  - [ ] `openspider --crawl https://site.test/ --save-crawl --output-folder X --resolve '*.site.test=127.0.0.1'` mit demselben Profil wie ein UI-Crawl derselben Fixture ergibt dieselbe URL-Menge, dieselben Statuscodes und Tiefen (übernommen aus CRAWL-135; `--resolve` ist OS-Erweiterung, Wirkung CRAWL-123).
- **Testfixture:** `out-basic-site`.

### OUT-183 `--crawl-list <quelle>`
- **Priorität:** P0
- **Beschreibung:** Crawl im Listenmodus. Quelle: lokale Datei (`.txt`, `.csv`, `.xlsx`; URLs werden aus allen Zellen extrahiert), XML-Sitemap-URL oder -Datei (inkl. Index; mehrfach angebbar), Google-Sheet-URL (mit `--google-drive-account`). Zusätzlich akzeptiert (übernommen aus CRAWL-135): `--crawl-google-sheet <Google-Konto> <Tabellen-URL>` (Referenz-Schreibweise laut Release-Notes; exakte Referenz-Syntax noch offen, Q31 in 01) als gleichwertige Form; OS-Erweiterung `--crawl-sitemap <URL>` als Kurzform für `--crawl-list <Sitemap-URL>`.
- **Akzeptanzkriterien:**
  - [ ] Datei mit 10 URLs, davon 2 Duplikate → 8 gecrawlte URLs; Export in Upload-Reihenfolge (OUT-015) mit 10 Zeilen.
  - [ ] `--crawl-list https://site.test/sitemap_index.xml` crawlt alle URLs aller untergeordneten Sitemaps.
  - [ ] [P2] Mehrere `--crawl-list`-Angaben werden zusammengeführt.
  - [ ] `--crawl-list https://site.test/sitemap_index.xml` und `--crawl-sitemap https://site.test/sitemap_index.xml` erzeugen identische Crawls (gleiche URL-Menge, gleiche Reihenfolge im Request-Log) (übernommen aus CRAWL-135).
  - [ ] Bei `--crawl-list <datei>` gibt die CLI auf stderr dieselben Zähler (gelesen, gültig, Duplikate, ungültig) aus wie der Zusammenfassungsdialog des Listenmodus (CRAWL-027) (übernommen aus CRAWL-135).
  - [ ] Leere Liste → Exit-Code 2 mit Meldung.
- **Testfixture:** `out-list-order`, `out-sitemaps-in`.

### OUT-184 `--load-crawl <datei|id>`
- **Priorität:** P0
- **Beschreibung:** Öffnet einen gespeicherten Crawl (Pfad zu `.ospider` oder Crawl-ID aus der Crawl-Verwaltung), um ohne erneutes Crawlen zu exportieren.
- **Akzeptanzkriterien:**
  - [ ] `--load-crawl <id> --export-tabs "Internal:All" --output-folder out` erzeugt den Export ohne Netzwerkzugriff.
  - [ ] Unbekannte ID → Exit-Code 2 mit Hinweis auf `--list-crawls`.
  - [ ] Ein noch laufender oder unvollständiger Crawl kann geladen werden; Exporte zeigen den gespeicherten Stand; Warnung „crawl incomplete“.
  - [ ] Crawl-Dateien älterer Schemaversion werden migriert (Kopie, Original bleibt unverändert) **(OS-Festlegung)**.
  - [ ] OS-Erweiterung `--resume` zusammen mit `--load-crawl` setzt einen pausierten, unterbrochenen oder durch ein Limit beendeten Crawl fort (Fortsetzungssemantik: CRAWL-126/CRAWL-128); ein gestoppter Crawl (UI-003) wird nicht fortgesetzt (Exit 1, Entscheidung (Fable-Empfehlung) vom 05.11.2026); ohne `--resume` wird nicht weitergecrawlt (übernommen aus CRAWL-135).
- **Testfixture:** gespeicherter Crawl `tests/fixtures/out-basic-site/crawl.ospider`; pausierter Crawl aus `out-queue`.

### OUT-185 `--list-crawls`
- **Priorität:** P0
- **Beschreibung:** Listet alle Crawls der Crawl-Verwaltung.
- **Ausgabe:** Tabelle `ID`, `Project`, `Name`, `Mode`, `Start URL`, `Started`, `URLs`, `Status`, `Locked`; mit `--json` als JSON-Array.
- **Akzeptanzkriterien:**
  - [ ] Filter `--project-name X` beschränkt auf ein Projekt.
  - [ ] IDs sind stabil und identisch mit „ID kopieren“ in der UI.
- **Testfixture:** Crawl-Verwaltung mit 3 Crawls in 2 Projekten.

### OUT-186 `--config <datei>`
- **Priorität:** P0
- **Beschreibung:** Verwendet eine gespeicherte Crawl-Konfiguration (OUT-221). Alle Optionen ohne eigene CLI-Flags (Rendering, Include/Exclude, Custom Extraction, Spalten, Sitemap-Optionen, Change Detection, Segmente …) werden so gesetzt. OS-Erweiterungen (übernommen aus CRAWL-135): `--set <schlüssel>=<wert>` überschreibt einzelne Konfigurationswerte (mehrfach angebbar, wirkt nach dem Laden von `--config`); `--max-urls <n>` ist Kurzform für `--set limits.max_urls=<n>`.
- **Akzeptanzkriterien:**
  - [ ] Konfiguration mit Exclude `/privat/` → keine URL unter `/privat/` im Crawl.
  - [ ] Nicht lesbare/ungültige Datei → Exit-Code 2 mit Zeilen-/Feldangabe des Fehlers.
  - [ ] Konfiguration einer neueren Schemaversion mit unbekannten Feldern → Warnung, unbekannte Felder werden ignoriert.
  - [ ] `--set threads=abc` → Exit-Code 2 mit Meldung „ungültiger Wert für threads: Ganzzahl 1–200 erwartet“; `--set unbekannt=1` → Exit-Code 2 mit Vorschlag des ähnlichsten Schlüssels (übernommen aus CRAWL-135, Exit-Code nach OUT-204). **OS-Festlegung 13.10.2026 (K-30):** Vorgeschlagen wird nur bei Levenshtein ≤ 3 (dieselbe Schwelle wie OUT-203); ohne ähnlichen Schlüssel lautet die Meldung „unbekannter Schlüssel“ mit Verweis auf die Liste der setzbaren Schlüssel.
  - [ ] `--config a.osconfig --set threads=10` → effektiver Wert 10, auch wenn `a.osconfig` 5 setzt.
- **Testfixture:** `tests/fixtures/out-basic-site/exclude.osconfig`.

### OUT-187 `--auth-config <datei>`
- **Priorität:** P1
- **Beschreibung:** Verwendet ein Authentifizierungsprofil (Basic/Digest/NTLM-Zugangsdaten und/oder gespeicherte Formular-Login-Cookies), damit automatisierte Crawls geschützte Bereiche erreichen. Bewusste Ausnahme von der P0-Einzelpriorität des CLI-Kerns (siehe Abschnittseinleitung): P1, weil das Profilformat OUT-223 (P1) Voraussetzung ist.
- **Akzeptanzkriterien:**
  - [ ] Basic-Auth-Bereich der Fixture liefert mit Profil 200, ohne 401.
  - [ ] Formular-Login: gespeicherte Session-Cookies werden gesendet; abgelaufene Session → Warnung „authentication cookies expired“ im Log.
  - [ ] Das Profil ist verschlüsselt (OUT-223); falsches/fehlendes Schlüsselmaterial → Exit-Code 2.
- **Testfixture:** `out-schedule` (Login-Bereich).

### OUT-188 `--headless`
- **Priorität:** P0
- **Beschreibung:** Das CLI-Binary läuft immer ohne Oberfläche; `--headless` wird aus Kompatibilitätsgründen akzeptiert und ist wirkungslos **(OS-Festlegung)**. **[P2]** Mit `--gui` wird stattdessen die Desktop-App mit dem gestarteten Crawl geöffnet.
- **Akzeptanzkriterien:**
  - [ ] Mit und ohne `--headless` identisches Verhalten des CLI-Binaries.
  - [ ] [P2] `--gui` öffnet die Desktop-App bzw. übergibt den Crawl an eine laufende Instanz (OUT-232).
- **Testfixture:** CLI-Test.

### OUT-189 `--save-crawl`
- **Priorität:** P0
- **Beschreibung:** Legt den abgeschlossenen Crawl zusätzlich als `.ospider`-Datei im Ausgabeordner ab (in der Crawl-Verwaltung wird ohnehin gespeichert).
- **Akzeptanzkriterien:**
  - [ ] Datei `crawl.ospider` im Ausgabeordner; per `--load-crawl <pfad>` wieder ladbar mit identischen Exporten.
  - [ ] Ohne `--save-crawl` und mit `--no-store` **(OS-Festlegung)** wird der Crawl nach Ende verworfen (temporäre Datei gelöscht).
- **Testfixture:** `out-basic-site`.

### OUT-190 `--output-folder`, `--overwrite`, `--timestamped-output`
- **Priorität:** P0
- **Beschreibung:** Ziel der Ausgaben; Standard = aktuelles Arbeitsverzeichnis. Verhalten gemäß OUT-013.
- **Akzeptanzkriterien:**
  - [ ] `--timestamped-output` legt `<output>/<YYYY-MM-DD_HH-MM-SS>/` an, auch in Google Drive.
  - [ ] `--overwrite` ersetzt gleichnamige Dateien.
  - [ ] Beide angegeben → `--timestamped-output` hat Vorrang, Warnung.
  - [ ] Pfade mit Leerzeichen und Nicht-ASCII-Zeichen funktionieren (Windows: `C:\Users\Jürgen\Desktop\SEO Reports`).
  - [ ] Ein Pfad, der mit `\` endet und in Anführungszeichen steht (Windows-Fallstrick `"C:\out\"`), wird erkannt und korrigiert statt die Folgeargumente zu verschlucken **(OS-Festlegung)**.
- **Testfixture:** CLI-Integrationstests mit temporären Ordnern.

### OUT-191 `--project-name`, `--task-name`
- **Priorität:** P0
- **Beschreibung:** Ordnet den Crawl einem Projekt und Namen in der Crawl-Verwaltung zu; bestimmt auch die Ordnerstruktur in Google Drive und lokal (OUT-165).
- **Akzeptanzkriterien:**
  - [ ] Crawl erscheint in der UI-Crawl-Verwaltung unter dem Projekt mit dem Namen.
  - [ ] Ohne Angabe: Projekt „Default“, Name = Host der Start-URL bzw. Listen-Dateiname.
- **Testfixture:** CLI-Test + Abfrage via `--list-crawls`.

### OUT-192 `--export-tabs "<Tab>:<Filter>,…"`
- **Priorität:** P0
- **Beschreibung:** Exportiert Tabs mit Filtern nach Crawl-Ende bzw. aus dem geladenen Crawl. Namen entsprechen den kanonischen UI-Namen; konfigurierbare Schwellen im Namen dürfen als `X` oder als aktueller Wert geschrieben werden (z. B. `Page Titles:Over X Characters` oder `Page Titles:Over 60 Characters`).
- **Akzeptanzkriterien:**
  - [ ] `--export-tabs "Internal:All,Response Codes:Client Error (4xx)"` erzeugt `internal_all.csv` und `response_codes_client_error_4xx.csv`.
  - [ ] Namensvergleich ist case-insensitiv und toleriert zusätzliche Leerzeichen um `:` und `,`.
  - [ ] Kommas innerhalb eines Namens sind per `\,` maskierbar.
  - [ ] Unbekannter Name → Abbruch vor dem Crawl (OUT-203).
  - [ ] Im Vergleich (OUT-202) darf ein dritter Namensteil den Unterfilter wählen: `"Internal:HTML:Missing"` (Syntax, Werte und Validierung in OUT-151, Nutzerentscheidung 04.11.2026).
- **Testfixture:** `out-basic-site`.

### OUT-193 `--bulk-export "<Menü>:<Eintrag>,…"`
- **Priorität:** P0
- **Beschreibung:** Führt Bulk-Exporte aus Abschnitt 3 aus; verschachtelte Menüs mit weiterem `:`.
- **Akzeptanzkriterien:**
  - [ ] `--bulk-export "Links:All Inlinks,Response Codes:Internal:Client Error (4xx) Inlinks"` erzeugt beide Dateien.
  - [ ] Nicht-tabellarische Bulk-Exporte (Screenshots, Page Source …) landen in Unterordnern des Ausgabeordners.
- **Testfixture:** `out-links`, `out-response-codes`.

### OUT-194 `--save-report "<Menü>:<Report>,…"`
- **Priorität:** P0
- **Beschreibung:** Speichert Reports aus Abschnitt 4. Zusätzlich registrierte Report-Namen anderer Bereiche (übernommen aus 08/09; Inhalt und Format führend dort, Namensprüfung nach OUT-203):
  - [M4] `AI:Action Plan` – Maßnahmenplan (AI-106/AI-107); Format über `--export-format` bzw. die Report-Option `md|html|pdf|docx` (Standard `md`); `AI:AI Crawler Accessibility` (AI-125); `AI:AI Search Readiness` (AI-127).
  - [M5] `DataForSEO:Keyword Cannibalization`, `DataForSEO:DataForSEO Relevant Pages`, `DataForSEO:LLM Share of Voice`, `DataForSEO:LLM Top Domains per Keyword`, `DataForSEO:LLM Top Pages per Keyword`, `DataForSEO:LLM Co-Cited Pages`, `DataForSEO:LLM Mentions History`, `DataForSEO:DataForSEO Cost Log` (D4S-072/073).
- **Akzeptanzkriterien:**
  - [ ] `--save-report "Crawl Overview,Redirects:Redirect Chains"` erzeugt `crawl_overview.csv` und `redirect_chains.csv`.
  - [ ] Report mit unerfüllter Voraussetzung → Warnung, kein Abbruch, Exit-Code 0 (bzw. 4 mit `--strict`).
  - [ ] [M4] `--save-report "AI:Action Plan"` nach einem Lauf mit `--ai-tasks audit-summary` erzeugt `ai_action_plan.md`, inhaltsgleich mit `tests/golden/ai/action-plan.md` (AI-107); ohne KI-Lauf im Crawl gilt die Voraussetzungsregel oben (Warnung, kein Abbruch).
  - [ ] [M5] `--save-report "DataForSEO:Keyword Cannibalization"` gegen den DataForSEO-Mock erzeugt die Datei aus D4S-072.
  - [ ] `--save-report "Crawl Changes"` ist nur mit Vergleich (OUT-202) befüllt; ohne Vergleich gilt die Voraussetzungsregel oben (OUT-154).
- **Testfixture:** `out-redirects`; [M4] `tests/fixtures/ai/site/` + `ai-mock` (08); [M5] `tests/fixtures/dfs-site/` + DataForSEO-Mock (09).

### OUT-195 `--export-format csv|xls|xlsx|gsheet`
- **Priorität:** P0
- **Beschreibung:** Format für alle Exporte des Aufrufs; Standard `csv`. `gsheet` erfordert `--google-drive-account`.
- **Akzeptanzkriterien:**
  - [ ] `gsheet` ohne Konto → Exit-Code 2 vor dem Crawl.
  - [ ] Unbekanntes Format → Exit-Code 2 mit Liste gültiger Werte.
- **Testfixture:** CLI-Test.

### OUT-196 Zusätzliche Export-Optionen
- **Priorität:** P1
- **Beschreibung:** **(OS-Festlegung,** Flagnamen) `--export-preset <name|pfad>` (Multi-Export-Preset, OUT-075), `--single-workbook` (OUT-007), `--skip-empty-exports` (OUT-012), `--segment <name|all>` (OUT-011), `--column-headers ui|canonical` (Abschnitt 0).
- **Akzeptanzkriterien:**
  - [ ] Preset und `--export-tabs` kombinierbar; Duplikate werden nur einmal exportiert.
  - [ ] Jede Option hat einen Integrationstest.
- **Testfixture:** `out-basic-site`, `out-segments`.

### OUT-197 `--create-sitemap`, `--create-images-sitemap`
- **Priorität:** P1
- **Beschreibung:** Siehe OUT-133.
- **Akzeptanzkriterien:**
  - [ ] Dateien liegen im (ggf. zeitgestempelten) Ausgabeordner; Aufteilung gemäß OUT-129.
- **Testfixture:** `out-sitemap-gen`.

### OUT-198 `--google-drive-account <konto>`
- **Priorität:** P1
- **Beschreibung:** Wählt ein in der App verbundenes Google-Konto (E-Mail-Adresse) für Drive/Sheets-Exporte und Google-Sheet-Listenquellen.
- **Akzeptanzkriterien:**
  - [ ] Nicht verbundenes Konto → Exit-Code 2 mit Hinweis, das Konto in der App zu verbinden (OAuth erfordert einmalig einen Browser).
  - [ ] [P2] Auf Rechnern ohne Browser kann ein in der App exportiertes Konto-Token-Paket importiert werden: `openspider --import-google-credentials <datei>` (verschlüsselt, OUT-223).
- **Testfixture:** Drive-Mock.

### OUT-199 `--email-on-complete <adresse,…>`
- **Priorität:** P2
- **Beschreibung:** Sendet nach Abschluss eine Abschlussmail (OUT-282) über das konfigurierte E-Mail-Konto.
- **Akzeptanzkriterien:**
  - [ ] Ohne konfiguriertes E-Mail-Konto → Warnung, Crawl läuft trotzdem, Exit-Code 0.
  - [ ] `--email-attach-exports` **(OS-Festlegung)** hängt die Exporte gezippt an (OUT-283).
- **Testfixture:** SMTP-Mock (z. B. lokaler Test-SMTP im `os-testsite`).

### OUT-200 `--export-custom-summary "<Metrik>,…"`
- **Priorität:** P2
- **Beschreibung:** Führt den Looker-Studio-Export (OUT-210) mit den genannten Metriken aus; erfordert Google-Konto.
- **Akzeptanzkriterien:**
  - [ ] Hängt genau eine Zeile an die Tabelle `<task>_custom_summary_report` an.
  - [ ] Unbekannte Metrik → Abbruch vor dem Crawl (OUT-203).
- **Testfixture:** Sheets-Mock.

### OUT-201 API-Optionen
- **Priorität:** P1
- **Beschreibung:** Aktiviert Integrationen für einen CLI- oder geplanten Crawl; führend für die CLI-Anbindung der Integrationen, INTEG-017 (Integrationen in Zeitplanung und CLI) und AI-022 (KI-Aufgaben in CLI) verweisen hierher bzw. legen die fachliche Semantik fest. Konto- **und** Property-Auswahl werden vollständig berücksichtigt (übernommen aus INTEG-017).
  - Analytics/Search Console/PageSpeed: `--use-google-analytics-4 "<konto>" "<account>" "<property>" "<datenstrom>"`, `--use-google-search-console "<konto>" "<website>"`, `--use-pagespeed`.
  - Linkmetriken: `--use-ahrefs`, `--use-majestic`, `--use-mozscape`.
  - KI und Embeddings **(OS-Festlegung,** Flagnamen; übernommen aus INTEG-017): `--use-ai` führt die im Konfigurationsprofil aktivierten KI-Prompts aus (INTEG-160 … INTEG-179); `--use-embeddings` erzeugt die im Profil konfigurierten Embeddings und berechnet die semantischen Auswertungen (INTEG-180 … INTEG-189). KI-Aufgaben aus 08 über `--ai-tasks`, `--ai-questions`, `--ai-budget`, `--ai-consent`, `--ai-batch`, `--ai-reuse-cache` (Semantik führend in AI-022) sowie `--export-query "<Name>"` (Export einer gespeicherten „Frag den Crawl“-Abfrage ohne KI-Request, AI-105, P2; Dateiname nach OUT-020) **[M4]**.
  - DataForSEO **[M5]** (Semantik führend in D4S-073): `--use-dataforseo "<modul,…>"`, `--dfs-location`, `--dfs-language`, `--dfs-device`, `--dfs-serp-mode`, `--dfs-keywords`, `--dfs-budget`, `--dfs-max-cache-age`, `--dfs-cache-from`, `--dfs-sandbox`, `--dfs-query-strings`, `--dfs-allowed-params`, `--dfs-allow-operators`, `--dfs-estimate-only`, `--dfs-accept-data-transfer`, `--dfs-allow-partial`; Bulk-Exporte und Reports unter `DataForSEO:<Eintrag>` (D4S-072).
  - Gemeinsames Budget **[M4]** (OS-Festlegung, BudgetLedger nach AI-013): `--api-budget <usd>` setzt die Obergrenze aller bezahlten APIs je Crawl (KI und DataForSEO zusammen); `--ai-budget` bzw. `--dfs-budget` setzen nur das jeweilige Teilbudget. Für angeforderte KI-Aufgaben genügt eines von `--api-budget`, `--ai-budget` oder einem Budget im Profil (AI-022).
  - `--require-apis` **(OS-Festlegung,** übernommen aus INTEG-017): macht jede angeforderte Integration verpflichtend.
  - Schlüssel kommen aus dem Schlüsselbund der App oder aus Umgebungsvariablen `OPENSPIDER_PSI_KEY`, `OPENSPIDER_AHREFS_KEY`, `OPENSPIDER_MAJESTIC_KEY`, `OPENSPIDER_MOZ_KEY` bzw. für KI-Anbieter `OPENSPIDER_AI_KEY_<ANBIETER>` (AI-009) **(OS-Festlegung)**. `--use-google-analytics` (Universal Analytics) wird mit Fehlermeldung „nicht mehr unterstützt“ abgelehnt.
- **Ausgabe:** wie ein UI-Crawl mit denselben Integrationen; Exporte enthalten die Integrationsspalten.
- **Akzeptanzkriterien:**
  - [ ] `--use-google-search-console "kontoA" "sc-domain:integ.test"` fragt genau diese Property ab (Mock-Log prüft den Property-Parameter); analog fragt `--use-google-analytics-4` genau die angegebene Property und den angegebenen Datenstrom ab (übernommen aus INTEG-017).
  - [ ] Ohne `--require-apis`: Fehlt für eine angeforderte Integration der Schlüssel bzw. das Token des genannten Kontos, läuft der Crawl ohne diese Integration, stderr und Log enthalten je Integration eine Warnung, Exit-Code 0 (mit `--strict` 4, OUT-204) (übernommen aus INTEG-017).
  - [ ] Mit `--require-apis`: derselbe Fall endet vor dem Crawl mit Exit-Code 2 und einer Meldung, die Integration und Konto nennt; der Fixture-Server zählt 0 Requests (Prüfung im Rahmen von OUT-203) (übernommen aus INTEG-017).
  - [ ] `--use-ai` bzw. `--use-embeddings` ohne aktivierte Prompts bzw. ohne Embedding-Konfiguration im Profil → Warnung „keine KI-Prompts konfiguriert“ bzw. „keine Embeddings konfiguriert“, Exit-Code 0; mit `--require-apis` Exit-Code 2 vor dem Crawl.
  - [ ] Mit `--use-embeddings` enthält ein anschließender Export der semantischen Filter (INTEG-181/182) dieselben Zeilen wie ein UI-Crawl mit gleichem Profil gegen den Embedding-Mock.
  - [ ] Exporte starten erst, wenn die API-, KI- und Embedding-Warteschlangen leer sind (INTEG-007, AI-022) (übernommen aus INTEG-017).
  - [ ] Schlüssel aus Umgebungsvariablen erscheinen nie im Log.
  - [ ] [M4] `--api-budget 0.01 --ai-tasks decisions` gegen `ai-mock` (Kosten 0,004 USD je Request) → genau 2 KI-Requests, danach `Budget exceeded`; Crawl und Exporte laufen vollständig (wie AI-022, nun über das Crawl-Budget). [M5] Mit `--api-budget 1.00` und gleichzeitig `--use-dataforseo labs` zählen KI- und DataForSEO-Kosten gegen dieselbe Obergrenze (D4S-013).
  - [ ] [M4] `--export-query "Seiten ohne H1"` mit einer im Profil gespeicherten Abfrage erzeugt die Exportdatei ohne KI-Request (Mock-Zähler 0); ein unbekannter Name → Exit-Code 2 vor dem Crawl (OUT-203).
- **Testfixture:** Mock-Endpunkte im `os-testsite` (GA4, GSC, PSI, Linkmetriken, KI-Text, Embeddings; [M4] `ai-mock`; [M5] DataForSEO-Mock); CLI-Integrationstest mit Request-Zähler.

### OUT-202 `--crawl-comparison <id1> <id2>`, `--project-crawl-comparison ["true"|"false"]`, `--compare-with-previous`
- **Priorität:** P1
- **Beschreibung:** Vergleich zweier Crawls bzw. des neuen Crawls mit dem vorherigen erfolgreich abgeschlossenen Crawl desselben Projekts (Auto Compare, OUT-152); Current/Previous nach Crawl-Datum. Die Referenz erwartet `--project-crawl-comparison "true"` (Syntax gegen die öffentliche Referenz-Doku geprüft am 2026-10-13); OpenSpider akzeptiert das Flag mit und ohne Argument, damit bestehende Skripte unverändert laufen. `--compare-with-previous` ist ein gleichwertiges OS-Alias ohne Argument **(OS-Festlegung;** übernommen aus TECH-308). Die IDs für `--crawl-comparison` liefert `--list-crawls` (OUT-185).
- **Akzeptanzkriterien:**
  - [ ] `--crawl-comparison A B --save-report "Crawl Overview"` erzeugt den Overview mit Vergleichsspalten (OUT-147); die Reihenfolge von A und B ist egal (Current = späteres Crawl-Datum).
  - [ ] Vergleich mit sich selbst → Exit-Code 2.
  - [ ] `--project-crawl-comparison`, `--project-crawl-comparison true` und `--project-crawl-comparison "TRUE"` aktivieren den Vergleich identisch; `--project-crawl-comparison false` deaktiviert ihn (gleichbedeutend mit Weglassen).
  - [ ] Jeder andere Wert (z. B. `yes`, `1`) → Exit-Code 2 mit Meldung der gültigen Werte.
  - [ ] Ein auf das Flag folgendes Argument, das mit `--` beginnt, wird nicht als Wert verbraucht (`--project-crawl-comparison --headless` funktioniert).
  - [ ] `--project-crawl-comparison` bzw. `--compare-with-previous` ohne `--project-name` → Exit-Code 2 mit Meldung „Projektname fehlt“.
  - [ ] `--project-crawl-comparison MeinProjekt` (Projektname als Wert) → Exit-Code 2 mit Hinweis auf die richtige Syntax `--project-name "MeinProjekt" --project-crawl-comparison "true"` (übernommen aus CRAWL-036/CRAWL-135).
  - [ ] `--compare-with-previous` zusammen mit `--project-crawl-comparison false` → Exit-Code 2 (widersprüchliche Angaben).
  - [ ] `--crawl-comparison A B --export-tabs "Internal:HTML:Missing" --save-report "Crawl Changes"` exportiert die Missing-Zeilen (OUT-151) und den Report OUT-154.
- **Testfixture:** `out-compare`; `out-b3-diff` (OUT-151, OUT-154).

### OUT-203 Validierung aller Namen vor dem Crawl
- **Priorität:** P0
- **Beschreibung:** Alle Argumente (Optionsnamen, Tab-, Bulk-, Report-, Metrik-Namen, Formate, Pfade, Konten) werden geprüft, **bevor** gecrawlt wird. Bei unbekannten Namen werden ähnliche gültige Namen vorgeschlagen (Levenshtein ≤ 3) **(OS-Festlegung;** bewusst strenger als die Referenz, deren öffentlich dokumentiertes Verhalten keine Vorab-Validierung beschreibt). **[P1]** Gibt es keinen ähnlichen Treffer, wird die vollständige Liste gültiger Namen des betroffenen Katalogs ausgegeben (Lücke G-5); dazu die Optionen `--list-exports` (alle Namen für `--export-tabs` und `--bulk-export`, nach Katalog gruppiert) und `--list-reports` (alle Namen für `--save-report`), jeweils mit `--json` maschinenlesbar.
- **Akzeptanzkriterien:**
  - [ ] `--export-tabs "Internal:Al"` → Exit-Code 2 in < 1 s, Meldung `unknown export 'Internal:Al' – did you mean 'Internal:All'?`.
  - [ ] Kein Netzwerkzugriff erfolgt vor erfolgreicher Validierung (Fixture-Server zählt 0 Requests).
  - [ ] Unbekannte Option → Exit-Code 2 mit Vorschlag der nächstliegenden Option (z. B. `--crawl-lst` → „meinten Sie `--crawl-list`?“) (übernommen aus CRAWL-135).
  - [ ] Ein Vergleichs-Unterfilter (`"<Tab>:<Filter>:Missing"`, OUT-151) ohne Vergleichsflag → Exit-Code 2 vor dem Crawl; `--list-exports` und `--help export-tabs` führen die Unterfilter nicht einzeln auf (sie gelten für jeden Tab-Export außer Change Detection; die Listen bleiben identisch mit der Registry), die Kurzhilfe der Option `--export-tabs` nennt die Syntax.
  - [ ] [P1] `--save-report "Gibtsnicht"` (kein Treffer mit Levenshtein ≤ 3) → Exit-Code 2; stderr enthält nach der Fehlermeldung die vollständige Liste aller gültigen Report-Namen (eine Zeile je Name), identisch mit `--list-reports`. Analog für `--export-tabs`, `--bulk-export` (Liste wie `--list-exports`, nur der betroffene Katalog) und `--export-custom-summary` (Liste wie `--help export-custom-summary`).
  - [ ] [P1] `--list-exports` und `--list-reports` geben ihre Listen auf stdout aus, Exit-Code 0, ohne Crawl und ohne Netzwerkzugriff; Inhalt identisch mit `--help export-tabs` + `--help bulk-export` bzw. `--help save-report` (gemeinsame Registry, Golden `tests/golden/out-cli/list_exports.txt`, `list_reports.txt`).
- **Testfixture:** CLI-Test mit Request-Zähler im `os-testsite`.

### OUT-204 Exit-Codes
- **Priorität:** P0
- **Beschreibung:** Eindeutige Rückgabewerte für Automatisierung **(OS-Festlegung)**.
- **Ausgabe:** `0` Erfolg (auch mit Warnungen, außer `--strict`); `1` unerwarteter interner Fehler; `2` Aufruf-/Konfigurationsfehler (vor dem Crawl erkannt); `3` Crawl konnte nicht durchgeführt werden (z. B. Crawl-Datei nicht schreibbar, Datenträger voll; mit `--fail-on-start-error` auch Start-URL nicht erreichbar); `4` Crawl erfolgreich, aber mindestens ein Export fehlgeschlagen (bzw. Warnungen bei `--strict`); `130` durch Benutzer (SIGINT/Strg+C) abgebrochen.
- **Akzeptanzkriterien:**
  - [ ] Je Exit-Code ein Integrationstest, der ihn provoziert.
  - [ ] Die letzte Zeile auf stderr fasst Ergebnis und Exit-Code zusammen.
- **Testfixture:** CLI-Tests (Drive-Mock mit Fehler für Code 4; schreibgeschützter Ordner für Code 3).

### OUT-205 Fortschritt und maschinenlesbare Ausgabe
- **Priorität:** P1
- **Beschreibung:** stdout bleibt für Nutzdaten (z. B. `--list-crawls`, `--help`) reserviert; Fortschritt und Logs gehen nach stderr. Mit `--progress json` (Alias `--json-progress`, übernommen aus CRAWL-135) wird je Sekunde (max.) eine JSON-Zeile mit `{"phase","crawled","queued","errors","elapsed_s","eta_s"}` auf stderr ausgegeben **(OS-Festlegung;** Felder des Crawl-Fortschritts siehe CRAWL-130).
- **Akzeptanzkriterien:**
  - [ ] Ohne TTY keine Fortschrittsbalken/ANSI-Sequenzen.
  - [ ] Phasen: `validate`, `crawl`, `analysis`, `export`, `notify`, `done`.
  - [ ] `--quiet` unterdrückt alles außer Fehlern; `--log-level` siehe OUT-241.
- **Testfixture:** CLI-Test mit Pipe statt TTY.

### OUT-206 Abbruch per Signal
- **Priorität:** P1
- **Beschreibung:** SIGINT/SIGTERM (macOS) bzw. Strg+C/Konsolen-Schließen (Windows) beenden den Crawl geordnet.
- **Akzeptanzkriterien:**
  - [ ] Erstes Strg+C: Crawl stoppt, Crawl-Datei wird konsistent geschlossen, keine Exporte, Exit-Code 130, innerhalb 10 s.
  - [ ] Zweites Strg+C: sofortiger Abbruch; die Crawl-Datei bleibt dank WAL öffnenbar.
  - [ ] [P2] Option `--export-on-interrupt` erzeugt die Exporte des bisherigen Stands vor dem Beenden.
  - [ ] Nach dem ersten Strg+C zeigt die Crawl-Verwaltung den Crawl als `Paused` und er ist per `--load-crawl <id> --resume` fortsetzbar (Status-Semantik CRAWL-128).
- **Testfixture:** CLI-Test, der nach 2 s SIGINT sendet.

### OUT-207 JS-Rendering im Headless-Betrieb
- **Priorität:** P1
- **Beschreibung:** JavaScript-Rendering funktioniert im CLI ohne sichtbares Fenster (Chromium headless) auf macOS und Windows, auch bei gesperrtem Bildschirm.
- **Akzeptanzkriterien:**
  - [ ] Geplanter Lauf mit Rendering bei gesperrtem Bildschirm liefert Screenshots und gerendertes HTML.
  - [ ] Fehlt die Chromium-Komponente, meldet die CLI das vor dem Crawl (Exit-Code 2) und nennt den Befehl zum Nachinstallieren (`openspider --install-browser`).
- **Testfixture:** `out-render` in CI (macOS/Windows Runner).

### OUT-208 Weitere Modi per CLI
- **Priorität:** P2
- **Beschreibung:** `--mode apis --crawl-list <datei>` ruft nur API-Daten für die Liste ab (Funktion des APIs-Modus: INTEG-012, führend; hier nur die CLI-Anbindung); `--mode serp --serp-file <datei>` berechnet Längen/Pixelbreiten für hochgeladene Titles/Descriptions und exportiert das Ergebnis **(OS-Festlegung,** Flagnamen).
- **Akzeptanzkriterien:**
  - [ ] SERP-Modus mit Datei aus OUT-098 → Export identisch zur Eingabe (Rundreise).
  - [ ] APIs-Modus führt keine HTTP-Requests gegen die gelisteten URLs aus.
- **Testfixture:** `out-serp`.

### OUT-209 Aufgabenausführung `--run-task <id>`
- **Priorität:** P1
- **Beschreibung:** Einstieg für den OS-Planer (OUT-166): lädt die Aufgabendatei und führt sie aus; schreibt Historie und Lauf-Log.
- **Akzeptanzkriterien:**
  - [ ] `--run-task` mit unbekannter ID → Exit-Code 2; da keine Aufgabe zuordenbar ist, erfolgt der Eintrag im globalen Log (OUT-240).
  - [ ] Ergebnis identisch mit dem gleichwertigen manuellen CLI-Aufruf (Test vergleicht Exporte).
- **Testfixture:** `out-schedule`.

---

## 9 Looker-Studio-Export (Custom Crawl Overview)

### OUT-210 Zeitreihen-Export je Crawl
- **Priorität:** P1
- **Beschreibung:** Nach jedem Crawl einer Aufgabe (oder manuell über Multi-Export) wird **eine Zeile** mit Kennzahlen des Crawls an eine Google-Tabelle angehängt. Über mehrere Läufe entsteht eine Zeitreihe, die z. B. Looker Studio als Datenquelle nutzt. URL-Einzeldaten werden nicht exportiert.
- **Eingaben/Konfiguration:** Aktivierung „Custom Crawl Overview“; Google-Konto; Auswahl der Metriken (Liste „Verfügbar“ → „Ausgewählt“, Reihenfolge = Spaltenreihenfolge).
- **Ausgabe:** Google-Tabelle `OpenSpider/<Project>/<Task>_custom_summary_report` (Blatt `data`); Spalte 1 `Date` (Datum + Uhrzeit des Crawlstarts, als Datumswert), Spalte 2 `Start URL` (bzw. Name der Listendatei im Listenmodus), danach die gewählten Metriken in Auswahlreihenfolge. Belegt durch die Referenz-Doku sind: eine Zeile je Lauf, ein Datums- und ein URL-Feld neben lauter Zahlenfeldern sowie die frei wählbare Metrikreihenfolge; die feste Position von `Date` vorn und `Start URL` an zweiter Stelle, der Blattname `data` und das Datumsformat sind **(OS-Festlegung)**.
- **Akzeptanzkriterien:**
  - [ ] Drei Läufe → drei Datenzeilen unter einer Kopfzeile, chronologisch.
  - [ ] Kopfzeile bei Auswahl `Internal:HTML`, `Response Codes:Client Error (4xx)` = `Date`, `Start URL`, `Internal:HTML`, `Response Codes:Client Error (4xx)` (genau diese Reihenfolge).
  - [ ] `Date` wird als Datumswert geschrieben (Sheets-Zellformat `yyyy-mm-dd hh:mm:ss`, lokale Zeit des Rechners), nicht als Text.
  - [ ] Umbenennen der Aufgabe erzeugt eine neue Tabelle (bestehende bleibt unverändert).
  - [ ] Werte sind Zahlen (nicht Text), damit Looker Studio sie als Number erkennt.
- **Testfixture:** Sheets-Mock, `out-basic-site` dreimal gecrawlt.

### OUT-211 Metrikkatalog
- **Priorität:** P1
- **Beschreibung:** Verfügbare Metriken: alle Summary-Werte aus OUT-080 Abschnitt 1, jeder Filter jedes Tabs als URL-Anzahl (`<Tab>:<Filter>`), Issues nach Typ/Priorität, Durchschnittswerte (Antwortzeit, Wortanzahl, Crawl-Tiefe), sowie URL-Inspection-Kennzahlen (z. B. `Search Console:URL Is Not on Google`, `Search Console:Indexable URL Not Indexed`, `Search Console:Rich Result Invalid`) bei aktiver Integration.
- **Akzeptanzkriterien:**
  - [ ] Katalog wird aus der Filter-Registry erzeugt; neue Filter erscheinen automatisch.
  - [ ] Nicht befüllte Metriken (Integration inaktiv) werden als leere Zelle geschrieben, nicht als 0.
  - [ ] Metriken lassen sich per Kopieren/Einfügen als Textliste übernehmen (für CLI `--export-custom-summary`).
  - [ ] [P2] Enthält eine Aufgabe eine gewählte Metrik, die im aktuellen Katalog nicht mehr existiert oder nicht mehr befüllt wird (z. B. nach Umbau einer Integration durch ein Update), zeigt der Aufgabendialog beim Öffnen/Bearbeiten eine Warnung, die jede betroffene Metrik namentlich nennt und „Entfernen“ anbietet; die Aufgabe bleibt `Valid` (Lücke G-4).
- **Testfixture:** Registry-Unit-Test.

### OUT-212 Spaltenstabilität bei Änderungen
- **Priorität:** P1
- **Beschreibung:** Spaltenzuordnung bleibt über Läufe stabil.
- **Akzeptanzkriterien:**
  - [ ] Neu hinzugefügte Metrik wird als neue Spalte **rechts** angefügt, und die Kopfzeile wird automatisch ergänzt **(Abweichung:** in der Referenz manuell nötig).
  - [ ] Entfernte Metrik: Spalte bleibt bestehen, neue Zeilen erhalten dort leere Werte.
  - [ ] Umsortierung der Auswahl nach dem ersten Lauf ändert die Spaltenreihenfolge der bestehenden Tabelle nicht (Zuordnung über Kopfzeilennamen, nicht Position).
  - [ ] Kopfzeilen nutzen immer kanonische englische Namen (sprachunabhängige Vorlage).
  - [ ] [P2] Läuft eine Aufgabe mit einer nicht mehr befüllten bzw. nicht mehr existierenden Metrik, schreibt der Lauf eine leere Zelle in deren Spalte, einen Log-Hinweis `metric '<Name>' is no longer populated` im Lauf-Log und einen Eintrag in der Laufhistorie (`Completed with warnings`, OUT-169) (Lücke G-4).
- **Testfixture:** Sheets-Mock mit vorbelegter Tabelle.

### OUT-213 Segmentierte Zeitreihen
- **Priorität:** P2
- **Beschreibung:** Bei konfigurierten Segmenten wird je Segment eine eigene Tabelle `<Task>_custom_summary_report_<Segment>` geführt, zusätzlich zur Gesamt-Tabelle.
- **Akzeptanzkriterien:**
  - [ ] Zwei Segmente → drei Tabellen, je eine Zeile pro Lauf.
- **Testfixture:** `out-segments`.

### OUT-214 Looker-Studio-Vorlage und Datentypen
- **Priorität:** P2
- **Beschreibung:** OpenSpider liefert eine eigene, offen lizenzierte Looker-Studio-Berichtsvorlage (Seiten: Zusammenfassung, Antwortcodes, URL-Typen, Indexierbarkeit, Struktur, On-Page, Inhalt, PageSpeed, strukturierte Daten, Sicherheit, hreflang, Sitemaps, JavaScript, URL Inspection) und eine Datentyp-Liste (`Column`, `Type`) zur korrekten Feldtyp-Zuordnung in Looker Studio.
- **Eingaben/Konfiguration:** Option „Datentyp-Blatt in die Zeitreihen-Tabelle schreiben“ (Standard an).
- **Ausgabe:**
  - Datentyp-Liste als eigenständige Datei im Repository (`docs/looker/datatypes.csv`, generiert aus der Metrik-Registry) und in der Doku verlinkt – entspricht dem Referenzvorgehen (dort eine separat verlinkte Tabelle, nicht Teil des Exports).
  - Zusätzlich, abweichend von der Referenz, ein zweites Blatt `datatypes` in jeder Zeitreihen-Tabelle, das genau die Spalten dieser Tabelle beschreibt **(OS-Festlegung, Abweichung)**.
  - Typwerte: `Date` (nur Spalte `Date`), `Text` (nur `Start URL`), `Percent` (Metriken, die einen Anteil ausdrücken, z. B. `% of Total`-Kennzahlen), `Number` (alle übrigen).
- **Akzeptanzkriterien:**
  - [ ] `docs/looker/datatypes.csv` enthält für jede Metrik des Katalogs (OUT-211) genau eine Zeile; CI schlägt fehl, wenn die Datei nicht zur Registry passt (Generator-Diff leer).
  - [ ] Blatt `datatypes` enthält genau eine Zeile je Spalte des Blatts `data`, in derselben Reihenfolge; nach Hinzufügen einer Metrik (OUT-212) wird auch das Blatt `datatypes` um eine Zeile ergänzt.
  - [ ] Mit deaktivierter Option entsteht kein zweites Blatt; die Zeitreihe ist unverändert.
  - [ ] Typzuordnung: `Date`→Date, `Start URL`→Text, `Internal:HTML`→Number; keine Spalte hat einen leeren Typ.
  - [ ] Die Vorlage ist eigenständig erstellt (kein Kopieren fremder Vorlagen), steht unter einer offenen Lizenz (CC BY 4.0) und ist in der Doku verlinkt; jede Seite der Vorlage nutzt nur Felder, die in `datatypes.csv` vorkommen (Prüfskript über die exportierte Vorlagendefinition).
- **Testfixture:** Sheets-Mock; Registry-Snapshot mit einer hinzugefügten Metrik.

---

## 10 Konfigurationsdateien & Crawl-Dateien

### OUT-220 Konfigurationsprofile speichern, laden, als Standard setzen
- **Priorität:** P0
- **Beschreibung:** Die gesamte Crawl-Konfiguration (alle Bereiche des Konfigurationsdialogs inkl. Spaltenkonfiguration, Segmente, Sitemap-Optionen, Change Detection, URL-Mapping) kann als Datei gespeichert und geladen werden. Eine Konfiguration kann als Standard gesetzt werden; sie gilt dann für jeden neuen Crawl. „Standard zurücksetzen“ stellt die Werkseinstellungen wieder her.
- **Eingaben/Konfiguration:** Menü Konfiguration → Profile: „Speichern unter…“, „Laden…“, „Zuletzt geladen“ (letzte 10), „Als Standard speichern“, „Standard zurücksetzen“.
- **Ausgabe:** `.osconfig`-Datei (OUT-221); Standardkonfiguration als `default.osconfig` im Einstellungsordner (OUT-227).
- **Akzeptanzkriterien:**
  - [ ] Speichern und erneutes Laden ergibt eine Konfiguration, die bei Serialisierung byte-identisch ist (Rundreise).
  - [ ] Nach „Als Standard speichern“ und Neustart ist die Konfiguration aktiv.
  - [ ] „Standard zurücksetzen“ fragt nach Bestätigung und löscht `default.osconfig`.
  - [ ] Laden einer Konfiguration während eines laufenden Crawls ist gesperrt (Hinweis); im pausierten Zustand nur für Optionen, die sich zur Laufzeit ändern lassen.
  - [ ] Der Konfigurationsdialog verwirft ungespeicherte Änderungen bei „Abbrechen“.
- **Testfixture:** Unit-Rundreise über alle Konfigurationsfelder mit Zufallswerten (Property-Test).

### OUT-221 Format der Konfigurationsdatei
- **Priorität:** P0
- **Beschreibung:** Menschenlesbares, versioniertes Textformat **(OS-Festlegung)**: TOML, UTF-8, mit Pflichtfeld `schema_version` (Ganzzahl) und `generator = "openspider <version>"`. Es werden **alle** Werte geschrieben (nicht nur Abweichungen), stabil sortiert nach Abschnitten, damit Dateien diff- und versionskontrollierbar sind. Kommentare sind erlaubt und werden beim Laden ignoriert. Konfigurationsdateien der Referenz können nicht importiert werden (proprietäres Format). **Nachtrag 09.11.2026 (1.x):** Dasselbe Schema lässt sich auch als JSON lesen und schreiben (OUT-288); TOML bleibt Standard- und Einbettungsformat, die Regeln dieses Eintrags gelten für beide Formate.
- **Akzeptanzkriterien:**
  - [ ] Ein JSON-Schema (`docs/spec/schemas/osconfig.schema.json`) beschreibt das Format; CI validiert alle Fixture-Konfigurationen dagegen.
  - [ ] Fehlende Felder → Standardwert; unbekannte Felder → Warnung mit Feldpfad, Laden wird fortgesetzt.
  - [ ] Typfehler (z. B. `max_depth = "abc"`) → Fehlermeldung mit Zeile/Spalte, Datei wird nicht geladen.
  - [ ] Handbearbeitete Datei mit Kommentaren lädt korrekt.
  - **OS-Festlegung 13.10.2026 (K-31):** „Pflichtfeld“ gilt für das Schreiben: jede geschriebene Datei trägt `schema_version` und `generator` (beim Schreiben immer auf die eigene Version gesetzt). Eine Datei ohne `schema_version` lädt als Version 1 ohne Warnung (Regel „fehlende Felder → Standardwert“). „Alle Werte“ heißt alle gesetzten Werte: nicht gesetzte optionale Werte (z. B. `limits.max_depth`) fehlen in der Datei, weil TOML kein `null` kennt. Wertebereiche, die die Spec nicht nennt (z. B. `response_timeout_s` 1–3600, `robots.retries` 0–10), legt die Validierung fest; maßgeblich ist das JSON-Schema.
- **Testfixture:** `tests/fixtures/config/` mit gültigen, unvollständigen, fehlerhaften und zukünftigen (`schema_version` + 1) Dateien.

### OUT-222 Trennung von Crawl-Konfiguration und App-Einstellungen
- **Priorität:** P1
- **Beschreibung:** Rechner-/benutzerbezogene Einstellungen liegen nicht in der Crawl-Konfiguration, sondern unter „Einstellungen“ (macOS: App-Menü → Einstellungen; Windows: Datei → Einstellungen): Sprache, Theme, Speicherort der Crawls, Speicher-/Ressourcenlimits, Proxy, vertrauenswürdige Zertifikate, Benachrichtigungen/E-Mail-Konten, Aufbewahrung, Updates, Export-Standards, verbundene Konten. Datei: `settings.toml`.
  - **Exportordner (OS-Festlegung, führend):** Schlüssel `export.directory`, Standard `<Dokumente>/OpenSpider/Exports`. `<Dokumente>` ist der Dokumente-Ordner des Benutzers laut Betriebssystem (macOS: `~/Documents`; Windows: bekannter Ordner „Dokumente“ (`FOLDERID_Documents`), auch wenn er umgeleitet ist, z. B. nach OneDrive). Er ist das Standardziel aller Exporte ohne ausdrückliche Pfadangabe außerhalb des UI-Exportdialogs, u. a. der MCP-Exporte (AGENT-021, dort zusätzlich die Liste erlaubter Verzeichnisse); der UI-Exportdialog schlägt beim ersten Export ebenfalls diesen Ordner vor.
- **Akzeptanzkriterien:**
  - [ ] Eine `.osconfig` enthält keine Werte aus `settings.toml`; Austausch einer Konfiguration zwischen Rechnern ändert keine lokalen Einstellungen.
  - [ ] Ohne Eintrag `export.directory` ist der wirksame Exportordner `<Dokumente>/OpenSpider/Exports`; er wird beim ersten Export angelegt (fehlende Elternordner eingeschlossen). Ein in den Einstellungen geänderter Ordner gilt ohne Neustart für den nächsten Export, auch für AGENT-021.
  - [ ] Ist der Ordner nicht beschreibbar, scheitert der Export mit einer Meldung, die den Pfad nennt; es wird nicht stillschweigend in einen anderen Ordner geschrieben.
  - [ ] Änderungen an Einstellungen, die einen Neustart erfordern, werden als solche gekennzeichnet.
- **Testfixture:** Unit-Test der Schlüsselmengen beider Formate (disjunkt); Pfadauflösung `<Dokumente>` mit gemocktem OS-Ordnerdienst (macOS, Windows mit umgeleitetem Dokumente-Ordner); schreibgeschützter Exportordner.

### OUT-223 Authentifizierungsprofile
- **Priorität:** P1
- **Beschreibung:** Zugangsdaten für standardbasierte Authentifizierung (Basic, Digest, NTLM) und beim Formular-Login gespeicherte Cookies können als Profil exportiert, wieder importiert und in CLI/Scheduling verwendet werden. Führend für Funktion, Priorität und Dateiformat; CRAWL-121 verweist hierher (dessen frühere Festlegung `.ospiderauth`/AES-256-GCM/P2 entfällt, OS-Festlegung zur Auflösung der Doppelspezifikation).
- **Eingaben/Konfiguration:** Konfiguration → Authentifizierung → Profile → Exportieren bzw. Importieren; CLI `--auth-config <Datei>` (OUT-187); Passphrase (für portable Dateien) oder „an diesen Rechner binden“ (Schlüssel im OS-Schlüsselbund).
- **Ausgabe:** `.osauth`-Datei, verschlüsselt mit XChaCha20-Poly1305 (AEAD); Schlüsselableitung Argon2id bei Passphrase, bei Rechnerbindung Schlüssel aus dem OS-Schlüsselbund **(OS-Festlegung,** einziges Auth-Profilformat).
- **Akzeptanzkriterien:**
  - [ ] Datei enthält keine Klartext-Kennwörter/Cookies (Test sucht nach Testgeheimnis).
  - [ ] Falsche Passphrase → eindeutiger Fehler, kein Absturz.
  - [ ] An Rechner gebundenes Profil lässt sich auf anderem Rechner nicht entschlüsseln (klarer Hinweis).
  - [ ] CLI: Passphrase über `OPENSPIDER_AUTH_PASSPHRASE` oder interaktiv (TTY); nie als Argument **(OS-Festlegung)**.
  - [ ] Export, Import und Headless-Crawl mit dem importierten Profil erreichen den geschützten Bereich (Fixture `crawl-auth` `/mitglieder/` → 200, solange die Formular-Cookies gültig sind) (übernommen aus CRAWL-121).
- **Testfixture:** `out-schedule` (Basic-Auth + Formular-Login); `crawl-auth` (Fachspec 01).

### OUT-224 Konfigurations-Unterschiede anzeigen
- **Priorität:** P2
- **Beschreibung:** Ein Fenster zeigt alle Abweichungen der aktuellen Konfiguration von einer Vergleichsbasis (Tastenkürzel Strg/Cmd+Umschalt+C oder Menü Konfiguration → „Abweichungen anzeigen“). Führender Eintrag; CRAWL-133 und UI-116 verweisen hierher (Fensterlayout: UI-116).
- **Eingaben/Konfiguration:** Vergleichsbasis: Werkseinstellungen (Standard) / gespeichertes Profil (`.osconfig`) / Konfiguration eines geöffneten Crawls (übernommen aus UI-116).
- **Ausgabe:** Tabelle `Setting`, `Current`, `Default` (bei anderer Basis: Wert der Basis); `Setting` nennt für Optionen des Konfigurationsprofils (CRAWL-132) den Anzeigenamen und den stabilen Schlüssel `bereich.option` (übernommen aus CRAWL-133); kopierbar als Text und exportierbar (OUT-006).
- **Akzeptanzkriterien:**
  - [ ] Nach Änderung von 3 Werten zeigt das Fenster genau 3 Zeilen; nach Änderung nur von Threads 5 → 10 genau diese eine Zeile (übernommen aus CRAWL-133).
  - [ ] Bei unveränderter Standardkonfiguration ist die Tabelle leer mit Hinweis „Keine Abweichungen“ (übernommen aus UI-116).
  - [ ] Jede Zeile ist anklickbar und öffnet die betreffende Einstellung im Konfigurationsdialog (übernommen aus UI-116).
  - [ ] App-Einstellungen (`settings.toml`, OUT-222) wie Update-Prüfung, Sprache oder Fokus-Modus erscheinen nie im Diff (übernommen aus CRAWL-142).
  - [ ] Kopieren als Text ergibt eine Zeile je Abweichung im Format `<Setting>\t<Current>\t<Default>`.
- **Testfixture:** Unit-Test; Profil mit 3 geänderten Werten.

### OUT-225 Geheimnisse außerhalb von Konfigurationsdateien
- **Priorität:** P0
- **Beschreibung:** API-Schlüssel, OAuth-Tokens, Kennwörter und E-Mail-Zugangsdaten werden ausschließlich im OS-Schlüsselbund (macOS Keychain, Windows Credential Manager) gespeichert; Konfigurationen verweisen nur per Kennung darauf.
- **Akzeptanzkriterien:**
  - [ ] Volltextsuche nach einem eingegebenen Test-API-Schlüssel in Einstellungsordner, Crawl-Dateien, Logs und Exporten findet nichts.
  - [ ] Fehlt der Schlüsselbund-Eintrag (anderer Rechner), meldet die Integration „nicht verbunden“ statt eines kryptischen Fehlers.
- **Testfixture:** Integrationstest mit Mock-Keychain-Backend.

### OUT-226 Versionierung und Migration
- **Priorität:** P1
- **Beschreibung:** Konfigurationen, Aufgaben, Presets und Crawl-Dateien älterer OpenSpider-Versionen werden beim Laden migriert; Dateien neuerer Versionen werden mit Warnung geladen (Konfiguration) bzw. abgewiesen, wenn das Crawl-Schema inkompatibel ist.
- **Akzeptanzkriterien:**
  - [ ] Für jede Schemaänderung existiert ein Migrationstest mit einer Fixture-Datei der Vorversion.
  - [ ] Migration einer Crawl-Datei erzeugt vorher eine Sicherung `<name>.ospider.bak-v<alt>` oder arbeitet auf einer Kopie.
  - [ ] Crawl-Datei mit höherer Hauptschemaversion → Meldung „Bitte OpenSpider aktualisieren“, keine Veränderung der Datei.
- **Testfixture:** `tests/fixtures/migrations/`.

### OUT-227 Speicherorte je Betriebssystem
- **Priorität:** P0
- **Beschreibung:** Feste, dokumentierte Pfade (ermittelt über die `dirs`-Crate) **(OS-Festlegung)**:
  - Einstellungen: macOS `~/Library/Application Support/OpenSpider/`, Windows `%APPDATA%\OpenSpider\`.
  - Crawls (Standard): macOS `~/Library/Application Support/OpenSpider/crawls/`, Windows `%LOCALAPPDATA%\OpenSpider\crawls\`.
  - Logs: macOS `~/Library/Logs/OpenSpider/`, Windows `%LOCALAPPDATA%\OpenSpider\logs\`.
  - Cache (Chromium, temporäre Exporte): macOS `~/Library/Caches/OpenSpider/`, Windows `%LOCALAPPDATA%\OpenSpider\cache\`.
  - Umgebungsvariable `OPENSPIDER_HOME` überschreibt alle Basispfade (für portable Installationen und Tests).
- **Akzeptanzkriterien:**
  - [ ] Hilfe → Debug zeigt alle effektiven Pfade.
  - [ ] Mit `OPENSPIDER_HOME=/tmp/x` schreibt die App nichts außerhalb von `/tmp/x` (außer OS-Schlüsselbund und OS-Planer).
- **Testfixture:** Integrationstest mit `OPENSPIDER_HOME`.

### OUT-228 Crawl-Verwaltung
- **Priorität:** P1
- **Beschreibung:** Jeder Crawl wird automatisch fortlaufend gespeichert (auch bei Absturz/Stromausfall bis zum letzten Commit) und erscheint in der Crawl-Verwaltung (Datei → Crawls). Führend für Datei- und Projektverwaltung (CRAWL-128, CRAWL-143 und UI-124 verweisen hierher); führend für die Pause-/Fortsetzungs-Semantik und die Status-Zuordnung bei Abbruch sind CRAWL-128/CRAWL-143 (bzw. CRAWL-125/126).
- **Eingaben/Konfiguration:** Aktionen (Kontextmenü und Schaltflächen, per Tastatur erreichbar): Öffnen, Umbenennen, in Projekt verschieben, Projekt anlegen/umbenennen/sperren, Duplizieren, Exportieren (`.ospider`), Importieren, Löschen, ID kopieren, Sperren/Entsperren, Zum Vergleich wählen, Im Dateimanager zeigen (OS-Erweiterung), **[P2]** Projekt exportieren (alle Crawls eines Projekts als ZIP, OS-Erweiterung); Mehrfachauswahl für Löschen, Sperren, Verschieben, Exportieren (übernommen aus CRAWL-128/143); Filter „Anzeigen: Crawls / Vergleiche / alle“; Suche (Strg/Cmd+F).
- **Ausgabe:** Tabelle `Project`, `Name`, `Mode`, `Start URL`, `Started`, `Duration`, `URLs`, `Size on Disk`, `Status` (Running/Paused/Completed/Incomplete/Failed), `Locked`, `ID`.
- **Akzeptanzkriterien:**
  - [ ] Nach erzwungenem Beenden (`kill -9`) während des Crawls erscheint der Crawl als `Incomplete`, ist öffenbar und fortsetzbar; nach Fortsetzen und Abschluss ist die Menge gecrawlter URLs identisch mit einem ununterbrochenen Crawl derselben Fixture.
  - [ ] Datenverlust bei `kill -9` höchstens die URLs der letzten 5 s bzw. der letzten 1 000 URLs (Commit-Intervall, je nachdem was früher eintritt) **(OS-Festlegung)**.
  - [ ] Löschen fragt nach Bestätigung, entfernt die Datei und gibt Speicherplatz frei (`Size on Disk` der Summe sinkt um die Dateigröße); gesperrte Crawls sind nicht löschbar (Schaltfläche deaktiviert, CLI-Löschversuch → Exit-Code 2).
  - [ ] Duplizieren erzeugt eine unabhängige Kopie mit neuer ID und Name `<Name> (Kopie)`; Änderungen an der Kopie (Umbenennen, Löschen) betreffen das Original nicht.
  - [ ] Umbenennen und Verschieben in ein anderes Projekt ändern die ID nicht; `--list-crawls` zeigt danach den neuen Namen/Projekt.
  - [ ] Suche filtert über `Name`, `Project` und `Start URL` (Teilstring, case-insensitiv); „Anzeigen“-Filter und Suche wirken kombiniert.
  - [ ] `Status`: laufender Crawl `Running`, pausierter (auch per Pause oder SIGINT beendeter) `Paused`, regulär beendeter `Completed`, abgestürzter bzw. hart beendeter (SIGKILL) `Incomplete`, mit Fehler beendeter `Failed` (Zuordnung gemäß CRAWL-128; UI-Anzeigenamen dort).
  - [ ] Import einer `.ospider`-Datei kopiert sie in den Crawl-Ordner und vergibt eine neue ID, wenn die ID schon existiert; Import einer Nicht-Crawl-Datei mit Endung `.ospider` → Fehlermeldung, nichts wird kopiert.
  - [ ] Tabelle bleibt bei 1 000 gespeicherten Crawls flüssig (< 500 ms Öffnen, Sortieren je Spalte < 200 ms, Suche < 100 ms); Metadaten stammen aus einem separaten Index, nicht aus dem Öffnen jeder Datei (übernommen aus CRAWL-128).
  - [ ] `ID` ist eine stabile, eindeutige Kennung (UUID v7) **(OS-Festlegung,** übernommen aus CRAWL-128); „ID kopieren“ legt genau die ID ohne Leerzeichen/Zeilenumbruch in die Zwischenablage, bei Mehrfachauswahl eine ID je Zeile in Tabellenreihenfolge; die kopierte ID wird von `--load-crawl` akzeptiert (End-to-End-Test Zwischenablage → CLI).
  - [ ] Mehrfachauswahl von bis zu 1 000 Crawls: Löschen, Sperren, Verschieben und Exportieren in einem Schritt (übernommen aus CRAWL-128); der Löschdialog nennt Anzahl und freiwerdenden Speicher und weist darauf hin, dass gelöschte Crawls nicht wiederherstellbar sind (übernommen aus UI-124).
  - [ ] „Crawl exportieren…“ funktioniert für Crawls ohne Projekt und für Crawls in einem Projektordner (Referenzfehler nicht übernehmen); Export eines laufenden Crawls ist deaktiviert (Tooltip „Crawl läuft – erst pausieren“), ein pausierter Crawl ist exportierbar und bleibt fortsetzbar (übernommen aus CRAWL-143).
  - [ ] „Zum Vergleich wählen“ ist nur bei genau zwei markierten Crawls aktiv; „Löschen“ und „Umbenennen“ sind für gesperrte Crawls ausgegraut (übernommen aus CRAWL-143).
  - [ ] Löschen eines Crawls, der in einem gespeicherten Vergleich verwendet wird, nennt vorher den Vergleich namentlich (OUT-150).
  - [ ] [P2] „Projekt exportieren“ erzeugt ein ZIP mit allen `.ospider`-Dateien des Projekts; Import des ZIP auf einem anderen Rechner legt das Projekt mit allen Crawls an.
- **Testfixture:** Integrationstest mit 3 Projekten und 1 000 synthetischen Crawl-Metadaten; `out-basic-site` für Absturz-/Fortsetzungstest; umbenannte Textdatei `fake.ospider`.
- **Verweis (Nachtrag 09.11.2026):** Löschen, Umbenennen, Verschieben und Projektverwaltung per CLI sowie die neue Aktion „Projekt löschen“: OUT-287 (1.x); Pausieren/Fortsetzen fremder Crawls: OUT-286.

### OUT-229 Automatische Crawl-Aufbewahrung (Retention)
- **Priorität:** P1
- **Beschreibung:** Crawls werden nach einer einstellbaren Dauer automatisch gelöscht; gesperrte Crawls und Crawls in gesperrten Projekten sind ausgenommen. Sperren eines Projekts gilt auch für künftige Crawls darin (inkl. geplanter).
- **Eingaben/Konfiguration:** Einstellungen → Crawl-Aufbewahrung: `Nie` (Standard), `1 Woche`, `1 Monat`, `3 Monate`, `6 Monate`, `1 Jahr`, `Benutzerdefiniert (Tage)` **(OS-Festlegung** der Auswahlwerte); Bezugszeitpunkt = Crawl-Ende (bei unvollständigen: letzte Änderung). **[P2]** Zusätzlich „Höchstens N Crawls behalten“ (OS-Erweiterung, übernommen aus UI-124; Standard leer): überzählige ungesperrte Crawls werden, älteste zuerst, gelöscht. Führend; CRAWL-128 und UI-124 verweisen für die Aufbewahrung hierher.
- **Akzeptanzkriterien:**
  - [ ] Einstellung „1 Woche“ → Crawl mit Ende vor 8 Tagen wird beim nächsten App-Start bzw. spätestens 1 h später gelöscht; Crawl vor 6 Tagen bleibt.
  - [ ] Gesperrter Crawl bleibt unabhängig vom Alter.
  - [ ] Neuer Crawl in gesperrtem Projekt ist automatisch geschützt.
  - [ ] Crawls, die von einem gespeicherten Vergleich referenziert werden, werden trotzdem gelöscht; der Vergleich wird als „nicht verfügbar“ markiert (OUT-150), vorher erscheint ein Hinweis bei Aktivierung der Aufbewahrung.
  - [ ] Ein gerade geöffneter oder laufender Crawl wird nie gelöscht.
  - [ ] Jede automatische Löschung wird geloggt (Name, ID, Alter).
  - [ ] [P2] 5 Crawls (einer gesperrt), „Höchstens 2 Crawls“ → die 2 jüngsten ungesperrten und der gesperrte bleiben (übernommen aus UI-124).
- **Testfixture:** Unit-Test mit manipulierbarer Uhr.

### OUT-230 Crawl-Datei speichern und öffnen
- **Priorität:** P0
- **Beschreibung:** Ein Crawl kann als einzelne portable Datei `*.ospider` (SQLite, WAL zusammengeführt) gespeichert und auf jedem unterstützten System geöffnet werden; Doppelklick öffnet die App (OUT-272). Pausierte Crawls lassen sich nach dem Öffnen fortsetzen (Fortsetzungssemantik: CRAWL-126/CRAWL-128, führend).
- **Eingaben/Konfiguration:** Datei → Crawl speichern unter… / Crawl öffnen…; CLI `--save-crawl` (OUT-189), `--load-crawl` (OUT-184).
- **Ausgabe:** `*.ospider`: SQLite-Datei mit `application_id` = feste OpenSpider-Kennung und `user_version` = Schemaversion **(OS-Festlegung)**; enthält Crawl-Daten, die beim Crawl wirksame Konfiguration (als eingebettete `.osconfig`), Queue-Zustand und Metadaten (Version, Start-URL, Zeiten). Keine Geheimnisse (OUT-225).
- **Akzeptanzkriterien:**
  - [ ] Auf macOS arm64 gespeicherte Datei öffnet auf Windows x64 (und umgekehrt) mit byte-identischen CSV-Exporten von `Internal:All` und `Links:All Inlinks`.
  - [ ] Export erzeugt eine in sich geschlossene Datei (keine `-wal`/`-shm`-Begleitdateien nötig; Test öffnet die Datei nach Kopie ohne Begleitdateien).
  - [ ] Speichern eines 1-Mio.-URL-Crawls dauert < 60 s (SSD).
  - [ ] Pausierter Crawl (Queue mit 15 URLs) → speichern, App neu starten, öffnen, fortsetzen → genau diese 15 URLs werden gecrawlt; Ergebnis entspricht einem ununterbrochenen Crawl.
  - [ ] Die eingebettete Konfiguration lässt sich aus der Crawl-Datei als `.osconfig` extrahieren („Konfiguration aus Crawl übernehmen“) und ist identisch mit der beim Crawl verwendeten.
  - [ ] Datei ohne passende `application_id` (z. B. beliebige SQLite-Datei) → Meldung „keine OpenSpider-Crawl-Datei“, kein Absturz, keine Veränderung der Datei.
  - [ ] Beschädigte Datei (zufällige Bytes in der Mitte überschrieben) → Fehlermeldung mit Option „Reparaturversuch“ (SQLite-Integrity-Check; Reparatur schreibt in eine neue Datei `<name>.repaired.ospider`), kein Absturz. Vorübergehende Schreibfehler am Ziel (Datei belegt oder gesperrt, kurzzeitiger E/A-Fehler) werden einige Male wiederholt; bleibende Schreibfehler am Ziel brechen die Reparatur mit Meldung ab, die halbe Kopie wird entfernt. Als verloren zählen nur Zeilen, die aus der Quelle nicht lesbar sind oder vom Schema abgewiesen werden (OS-Festlegung, Nachtrag 05.11.2026, B3-FLAKES).
  - [ ] Volltextsuche nach Testgeheimnissen (API-Schlüssel, Auth-Kennwort) in der Datei → 0 Treffer.
- **Testfixture:** gespeicherter Crawl aus `out-basic-site`, pausierter Crawl aus `out-queue`, absichtlich beschädigte Kopie, fremde SQLite-Datei.

### OUT-231 Speicherort der Crawl-Daten
- **Priorität:** P0
- **Verweis:** siehe CRAWL-124 (führend). Hier gilt nur, dass der Standardpfad des Crawl-Ordners je Betriebssystem in OUT-227 festgelegt ist.

### OUT-232 Einzelinstanz und Dateizuordnung beim Öffnen
- **Priorität:** P1
- **Beschreibung:** Doppelklick auf `.ospider`/`.osconfig`/`.ospreset`/`.ostask` öffnet die laufende App-Instanz (keine zweite) und lädt die Datei bzw. fragt, ob die Konfiguration geladen werden soll.
- **Akzeptanzkriterien:**
  - [ ] Zweiter Start der GUI fokussiert die vorhandene Instanz und übergibt die Datei.
  - [ ] Öffnen eines Crawls bei laufendem Crawl fragt, ob der aktuelle pausiert werden soll.
- **Testfixture:** manueller Test je OS.

### OUT-233 Manifest der Crawl-Datei
- **Priorität:** P1
- **Beschreibung:** **(OS-Festlegung,** Austausch mit fremden Lesern wie Skripten, BI-Werkzeugen und der Cloud-Variante; Grundlage `docs/plan/autonomo-abgleich.md` 5.2) Jede Crawl-Datei (`*.ospider`, OUT-230) beschreibt sich selbst über ein Manifest. Ein Leser erkennt daran ohne Kenntnis interner Tabellen, welches Programm die Datei geschrieben hat, wie weit der Crawl gekommen ist, mit welcher Konfiguration und welche Version der Leseschnittstelle (OUT-234) gilt. Das Manifest ist Teil der Leseschnittstelle und wird als Sicht `q_manifest(key TEXT, value TEXT)` bereitgestellt, eine Zeile je Schlüssel. Jeder Schlüssel der Tabelle ist immer vorhanden; ein unbekannter oder nicht zutreffender Wert ist `NULL`. Neue Schlüssel kommen nur additiv hinzu.
- **Eingaben/Konfiguration:** keine; die Werte stammen aus den Crawl-Metadaten, der eingebetteten Konfiguration und den Zählern der Datei.
- **Ausgabe:** Schlüssel des Manifests, Version 1:

  | Schlüssel | Wert |
  |---|---|
  | `format` | immer `ospider` |
  | `manifest_version` | `1` |
  | `read_api_version` | Hauptversion der Leseschnittstelle (OUT-234), zunächst `1` |
  | `schema_version` | interne Schemaversion (`user_version`), nur informativ; Leser werten sie nicht aus |
  | `openspider_version` | Version des Programms, das die Datei angelegt hat (SemVer, z. B. `0.2.0`) |
  | `last_writer_version` | Version des Programms, das zuletzt geschrieben hat |
  | `writer_kind` | `app`, `cli` oder `mcp-stdio` (Prozess, der den Crawl führt bzw. zuletzt geführt hat) |
  | `crawl_id`, `name`, `project` | Kennung, Name und Projekt laut Crawl-Verwaltung (OUT-228) |
  | `mode` | `spider` oder `list` |
  | `start_url` | Start-URL im Spider-Modus, sonst `NULL` |
  | `list_source` | Quelle im Listenmodus (Dateiname bzw. `paste`), sonst `NULL` |
  | `started_at`, `ended_at` | ISO 8601 mit Zeitzonenoffset; `ended_at` ist `NULL`, solange der Crawl läuft oder pausiert |
  | `state` | Crawl-Zustand wie OUT-228: `Running`, `Paused`, `Completed`, `Incomplete`, `Failed` |
  | `end_reason` | `queue_empty`, `limit_reached`, `stopped` oder `NULL` |
  | `completeness` | abgeleitete Vollständigkeit, Regel unten |
  | `user_agent` | wirksamer User-Agent-String des Crawls |
  | `config_sha256` | SHA-256 (hex, Kleinbuchstaben) der eingebetteten `.osconfig` (`q_config.osconfig`) |
  | `urls_total`, `urls_crawled` | Zeilen in `q_urls` bzw. davon abgerufene URLs |
  | `analysis_state` | Zustand der Crawl-Analyse (CRAWL-141): `done`, `stale`, `pending` oder `off` |

  Regel `completeness`: `Completed` mit `queue_empty` → `complete`; `Completed` mit `limit_reached` → `limited`; Ende mit `stopped` → `stopped`; `Paused` → `paused`; `Running` mit aktuellem Heartbeat → `running`; `Running` ohne aktuellen Heartbeat (Absturz, Regel der Crawl-Verwaltung OUT-228) und `Incomplete` → `interrupted`; `Failed` → `failed`.
- **Akzeptanzkriterien:**
  - [ ] Für einen abgeschlossenen Crawl von `out-basic-site` liefert `SELECT key, value FROM q_manifest ORDER BY key` genau die Schlüssel der Tabelle mit den erwarteten Werten (Golden nach Normalisierung von Zeiten, IDs und Versionen).
  - [ ] Jede Zeile der Regel `completeness` ist durch einen Test belegt (abgeschlossen, Limit erreicht, gestoppt, pausiert, laufend, verwaister Crawl ohne Heartbeat, fehlgeschlagen).
  - [ ] `config_sha256` stimmt mit dem SHA-256 der über „Konfiguration aus Crawl übernehmen“ (OUT-230) extrahierten `.osconfig` überein.
  - [ ] `user_agent` entspricht dem User-Agent, den `os-testsite` im Request-Log für die Start-URL protokolliert.
  - [ ] Eine aus Schema v1 oder v2 migrierte Datei hat ein vollständiges Manifest; Werte, die die alte Datei nicht kennt (z. B. `openspider_version`), sind `NULL`.
  - [ ] Das Manifest enthält keine Geheimnisse (Volltextsuche wie OUT-230 → 0 Treffer).
- **Testfixture:** `out-basic-site`; pausierter Crawl aus `out-queue`; M1-Dateien (Schema v1/v2) aus `tests/fixtures/crawl-files`.

### OUT-234 Stabile Leseschnittstelle der Crawl-Datei (`q_`-Sichten)
- **Priorität:** P1
- **Beschreibung:** **(OS-Festlegung)** Fremde Leser greifen auf eine Crawl-Datei nur über dokumentierte, nur-lesende Sichten mit Präfix `q_` zu, nie auf interne Tabellen. Interne Tabellen und Migrationen dürfen sich ändern; die Sichten bleiben innerhalb einer Hauptversion (`read_api_version`, OUT-233) stabil. Dieser Eintrag ist führend für die Grundsichten; AI-100 (M4) ergänzt weitere `q_`-Sichten und Spalten nach denselben Regeln, das MCP-Werkzeug `sql_query` (AGENT-014) sieht die Sichten wie jede andere Abfrage.
  - **Sichten, Version 1:** `q_manifest` (OUT-233); `q_config(osconfig, updated_at)` – eingebettete Konfiguration ohne Geheimnisse; `q_urls` – eine Zeile je URL, die in den Tabellen erscheint (intern und extern); `q_links` – eine Zeile je Link-Instanz (Hyperlinks und Ressourcen-Referenzen); `q_sitemap_urls` – eine Zeile je Eintrag einer gelesenen XML-Sitemap; `q_columns(view, column, type, description)` – Selbstbeschreibung aller `q_`-Sichten.
  - **`q_urls`, Pflichtspalten:** `url_id` (Schlüssel, stabil innerhalb der Datei, nicht über Crawls hinweg), `address`, `is_internal`, `crawled` (1 = abgerufen), `content_type`, `status_code`, `status`, `indexability`, `indexability_status`, `title_1`, `meta_description_1`, `h1_1`, `h2_1`, `canonical_link_element_1`, `word_count`, `size_bytes`, `transferred_bytes`, `response_time`, `crawl_depth`, `folder_depth`, `inlinks`, `unique_inlinks`, `outlinks`, `unique_outlinks`, `external_outlinks`, `redirect_url`, `redirect_type`, `http_version`, `hash`, `last_modified`, `crawl_timestamp`.
  - **`q_links`, Pflichtspalten:** `source_url_id`, `target_url_id` (`NULL`, wenn das Ziel keine URL der Datei ist), `source_address`, `target_address`, `type`, `anchor`, `alt_text`, `follow`, `rel`, `target`, `path_type`, `link_path`, `link_position`, `link_origin`, `target_status_code`.
  - **`q_sitemap_urls`, Pflichtspalten:** `sitemap_url`, `url_id`, `address`, `lastmod`.
  - **Benennung und Werte:** Spaltennamen sind snake_case-Ableitungen der kanonischen Spaltennamen (Kleinbuchstaben, Einheiten als Suffix: `Size (bytes)` → `size_bytes`). Werte entsprechen dem CSV-Export derselben Datei: `q_urls` dem Export `Internal:All` bzw. `External:All`, `q_links` dem Export `All Inlinks` (OUT-031), jeweils mit denselben Einheiten (z. B. `response_time` in Sekunden) und Aufzählungswerten; Zahlen sind `INTEGER`/`REAL`, Wahrheitswerte `0`/`1`, Zeitpunkte ISO 8601 mit Offset, fehlende Werte `NULL`.
  - **Stabilität:** Innerhalb einer Hauptversion sind nur additive Änderungen erlaubt (neue Sichten, neue Spalten am Ende); Bedeutung, Typ und Einheit bestehender Spalten ändern sich nicht. Eine inkompatible Änderung erhöht `read_api_version` und steht in den Release-Notes. Ein Leser prüft `format` und `read_api_version` und bricht bei unbekannter Hauptversion mit einer klaren Meldung ab.
  - **Zugriff:** Die Sichten sind in der Datei gespeichert und brauchen weder OpenSpider-Code noch eigene Kollationen oder Funktionen; jede Abfrage einschließlich `WHERE` und `ORDER BY` läuft mit Standard-SQLite. Abgeschlossene Dateien lassen sich mit `file:<pfad>?mode=ro` öffnen; liegt die Datei in einem schreibgeschützten Verzeichnis, öffnet der Leser sie mit `immutable=1`. Während eines laufenden Crawls liest ein Leser einen konsistenten WAL-Schnappschuss; dafür braucht er nach den SQLite-Regeln Schreibrecht im Verzeichnis der Datei (für `-shm`).
- **Akzeptanzkriterien:**
  - [ ] Jede Pflichtspalte der drei Datensichten ist vorhanden und in `q_columns` mit Typ und nicht leerer Beschreibung geführt; `q_columns` beschreibt jede Spalte jeder `q_`-Sicht (CI-Test, keine Lücken).
  - [ ] Für `out-basic-site` stimmen `q_urls` (interne Zeilen) mit dem CSV-Export `Internal:All` und `q_links` mit `All Inlinks` derselben Datei in allen Pflichtspalten überein (Abgleich nach kanonischem Spaltennamen, gleiche Zeilenmenge).
  - [ ] Ein Golden aller `q_`-Sichten für `out-basic-site` ist für eine neu erzeugte Datei und für eine aus Schema v2 migrierte Datei identisch (Migrationen brechen die Leseschnittstelle nicht).
  - [ ] Ein Leser ohne OpenSpider-Code (Python `sqlite3`, Verbindung `mode=ro`) liest alle `q_`-Sichten einer abgeschlossenen Datei, filtert und sortiert auf jeder Textspalte ohne Fehler; derselbe Leser liest eine Datei während eines laufenden Crawls ohne Sperrfehler für den Crawl.
  - [ ] Die Sichten enthalten keine Geheimnisse (Volltextsuche wie OUT-230 über alle Sichtausgaben → 0 Treffer).
  - [ ] `read_api_version` in `q_manifest` ist `1`; eine Testdatei mit `read_api_version = 99` lässt der mitgelieferte Beispielleser mit der Meldung „unbekannte Version der Leseschnittstelle“ fallen.
- **Testfixture:** `out-basic-site`; M1-Datei (Schema v2) aus `tests/fixtures/crawl-files`; Beispielleser `scripts/ci/read_api_smoke.py`.

---

## 11 Logging & Diagnose

### OUT-240 Logdateien
- **Priorität:** P1
- **Beschreibung:** App und CLI schreiben strukturierte Logs in den Log-Ordner (OUT-227): `openspider.log` (App), `cli-<YYYYMMDD-HHMMSS>-<pid>.log` je CLI-Lauf, `task-<id>-<zeitstempel>.log` je geplantem Lauf.
- **Ausgabe:** Zeilenformat `<ISO-Zeitstempel> <LEVEL> <modul>: <nachricht> [key=value …]`; optional JSON-Zeilen (`--log-format json`).
- **Akzeptanzkriterien:**
  - [ ] Jede Zeile hat Zeitstempel mit Millisekunden und Zeitzonenoffset.
  - [ ] Crawl-Start und -Ende werden mit Konfigurations-Zusammenfassung (Modus, Start-URL, Limits, Rendering an/aus) geloggt.
  - [ ] Panics/unerwartete Fehler werden mit Backtrace geloggt, bevor der Prozess endet.
- **Testfixture:** CLI-Lauf auf `out-basic-site`, Prüfung der Logzeilen per Regex.

### OUT-241 Log-Level
- **Priorität:** P1
- **Beschreibung:** Stufen `error`, `warn`, `info` (Standard), `debug`, `trace`; einstellbar in Hilfe → Debug (App, wirkt sofort) bzw. `--log-level` und `OPENSPIDER_LOG` (CLI, Syntax wie `RUST_LOG` mit Modulfiltern).
- **Akzeptanzkriterien:**
  - [ ] `--log-level debug` protokolliert jede angefragte URL mit Status und Dauer; `info` nicht.
  - [ ] `OPENSPIDER_LOG=os_fetch=trace` erhöht nur das Level des Fetch-Moduls.
- **Testfixture:** CLI-Test.

### OUT-242 Rotation und Größenbegrenzung
- **Priorität:** P1
- **Beschreibung:** Logs wachsen nicht unbegrenzt.
- **Akzeptanzkriterien:**
  - [ ] `openspider.log` rotiert bei 10 MB; max. 5 Archive (`openspider.1.log` …), ältere werden gelöscht.
  - [ ] Lauf-Logs älter als 30 Tage werden beim App-Start gelöscht (Einstellung, Standard 30).
  - [ ] Gesamtgröße des Log-Ordners ≤ 500 MB; bei Überschreitung werden die ältesten Dateien zuerst entfernt.
- **Testfixture:** Unit-Test mit kleinen Grenzwerten.

### OUT-243 Schutz sensibler Daten in Logs
- **Priorität:** P0
- **Beschreibung:** Logs dürfen keine Geheimnisse enthalten.
- **Akzeptanzkriterien:**
  - [ ] Werte von `Authorization`, `Proxy-Authorization`, `Cookie`, `Set-Cookie`, API-Schlüsseln, OAuth-Tokens, Kennwörtern und Passphrasen werden als `<redacted>` geloggt.
  - [ ] Zugangsdaten in URLs (`https://user:pass@host/`) werden als `https://<redacted>@host/` geloggt.
  - [ ] Query-Parameter mit Namen `key`, `api_key`, `token`, `access_token`, `password`, `secret` werden maskiert.
  - [ ] Automatisierter Test: Crawl mit allen genannten Geheimnistypen, anschließend Suche nach den Klartextwerten in allen Logs → 0 Treffer.
- **Testfixture:** `out-schedule` + Mock-APIs mit Testschlüsseln.

### OUT-244 Diagnosepaket („Logs speichern“) und Debug-Informationen
- **Priorität:** P1
- **Beschreibung:** Hilfe → Debug zeigt Version, Build, OS/Architektur, CPU-Kerne, RAM gesamt/verfügbar, eingestellte Ressourcenlimits, effektive Pfade, Status der Chromium-Komponente, verbundene Integrationen (nur ja/nein). „Logs speichern“ erstellt ein ZIP für den Support.
- **Ausgabe:** `openspider-diagnostics-<zeitstempel>.zip` mit Logs (letzte 7 Tage), `system.txt`, Konfigurations-Diff gegen Standard (OUT-224, ohne Geheimnisse), Liste der Aufgaben (ohne Geheimnisse).
- **Akzeptanzkriterien:**
  - [ ] ZIP enthält keine Crawl-Daten und keine Geheimnisse (Test wie OUT-243).
  - [ ] Vor dem Speichern zeigt ein Dialog, welche Dateien enthalten sind.
  - [ ] CLI-Äquivalent: `openspider --diagnostics <zielordner>` **(OS-Festlegung)**.
- **Testfixture:** Integrationstest.

### OUT-245 Lokale Nutzungsstatistik
- **Priorität:** P2
- **Beschreibung:** Hilfe → Nutzungsstatistik zeigt eigene Nutzungsdaten über die Zeit, kumulativ und nicht-kumulativ (Parität zum Referenzstand). Daten bleiben ausschließlich lokal; es gibt keine Telemetrie ohne ausdrückliche Zustimmung (Opt-in, Standard aus). Führend; UI-128 verweist hierher (Dialoglayout und Diagramm: UI-128).
- **Eingaben/Konfiguration:** Zeitraum (letzte 12 Monate, Kalenderjahr, gesamt); Darstellung „Kumulativ“ / „Je Zeitraum“; „Statistik zurücksetzen“ mit Rückfrage (übernommen aus UI-128).
- **Ausgabe:** Kennzahlen je Monat: `Crawls`, `URLs Crawled`, `Crawl Time` (Stunden, Laufzeit ohne Pausen), `Exports`, `Reports`; Tabelle kopier-/exportierbar als CSV (übernommen aus UI-128). Speicherung lokal im Einstellungsordner (OUT-227).
- **Akzeptanzkriterien:**
  - [ ] Nach 3 Crawls zeigt die Statistik 3 Crawls im aktuellen Monat.
  - [ ] Ohne Opt-in erfolgt kein Netzwerkzugriff außer Update-Prüfung (Netzwerkmitschnitt im Test); Öffnen des Dialogs und jede Erfassung erzeugen keine ausgehende Verbindung.
  - [ ] „Kumulativ“: jeder Monatswert = Summe aller Monate bis einschließlich dieses Monats; „Je Zeitraum“: Wert des Monats allein; letzter kumulativer Wert = Summe aller nicht-kumulativen Werte (Fixture 2/0/5 → kumulativ 2/2/7) (übernommen aus UI-128).
  - [ ] Ein Crawl mit 25 URLs und 30 s Laufzeit, davon 10 s pausiert, erhöht `Crawls` um 1, `URLs Crawled` um 25 und `Crawl Time` um 20 s (übernommen aus UI-128).
  - [ ] Abgebrochene Crawls zählen mit ihren tatsächlich gecrawlten URLs; CLI- und geplante Läufe zählen ebenfalls (gemeinsamer lokaler Speicher) (übernommen aus UI-128).
  - [ ] „Statistik zurücksetzen“ löscht ausschließlich die Nutzungsdaten, keine Crawls oder Einstellungen.
- **Testfixture:** Unit-Test; vorbefüllte Statistikdatei mit 3 Monaten (Crawls 2/0/5).

### OUT-246 Absturzerkennung und lokaler Absturzbericht
- **Priorität:** P1
- **Beschreibung:** **OS-Festlegung (Nutzer 03.11.2026):** App und CLI halten unerwartete Programmenden lokal als Absturzbericht fest. Es wird dabei nichts gesendet; das Senden regelt OUT-247. Drei Arten:
  - `panic`: ein Rust-Panic in einem beliebigen Thread (auch wenn der Prozess weiterläuft). **Ausnahme (Klärung N33/Fable, 03.11.2026):** Panics in Parsern für Fremddaten, die gezielt abgefangen und dem Nutzer als saubere Fehlermeldung gezeigt werden (z. B. Tabellenimport CRAWL-027/031, PDF-Analyse), erzeugen keinen Bericht und keinen Hinweis nach OUT-247; die Logzeile mit Backtrace (OUT-240) bleibt. Technisch markiert der abfangende Code den Abschnitt (thread-lokal) als „eingedämmt“, der Hook fragt die Markierung ab. Weil die Markierung thread-lokal ist, darf ein so geschützter Parser seine Arbeit nicht auf andere Threads verteilen (z. B. PDF-Bibliothek ohne Thread-Pool-Funktion); ein Panic in einem fremden Worker-Thread gilt sonst als intern **(Klärung C01/Fable, 03.11.2026)**. Interne Panics (Pools, Schreibwarteschlange, Laufzeit) erzeugen weiterhin Berichte. Der Bericht wird im Panic-Hook geschrieben, bevor der Prozess endet; der Backtrace-Eintrag im Log (OUT-240) bleibt unverändert bestehen.
  - `frontend`: ein Fehler, der in der Oberfläche eine Fehlergrenze auslöst (eine Region zeigt ihre Ersatzfläche „Bereich neu laden“ oder die ganze Ansicht fällt aus). Unbehandelte Promise-Ablehnungen und Fensterfehler werden wie bisher nur geloggt und erzeugen keinen Bericht.
  - `unclean_exit`: der vorige App-Prozess endete ohne geordnetes Beenden und ohne `panic`-Bericht (z. B. Speicherzugriffsfehler, Stapelüberlauf, Abbruch durch das Betriebssystem, Stromausfall). Erkennung über eine Sitzungsmarke: Die App legt beim Start im Log-Ordner (OUT-227) `crashes/session-<pid>.json` mit PID, Prozess-Startzeit und Version an und löscht sie beim geordneten Beenden. Findet ein Start eine Marke, deren Prozess (gleiche PID **und** gleiche Startzeit) nicht mehr läuft, entsteht der Bericht nachträglich mit den letzten Logzeilen jenes Laufs; die Marke wird danach entfernt. SIGKILL durch den Nutzer (Task-Manager) ist davon nicht unterscheidbar und zählt ebenfalls als `unclean_exit`.
  - Native Speicherabbilder (Minidumps) sind ausdrücklich nicht Teil dieses Eintrags.
- **Eingaben/Konfiguration:** keine; Aufbewahrung fest (siehe Kriterien).
- **Ausgabe:** `crashes/crash-<YYYYMMDD-HHMMSS>-<art>.json` im Log-Ordner, UTF-8-JSON mit genau diesen Feldern: `format` (= 1), `id` (zufällige UUID v4), `kind`, `time` (ISO mit Offset), `app_version`, `build` (Commit-Kurzhash), `os` (Name und Version), `arch`, `process` (`app` | `cli`), `uptime_s`, `thread` (Name oder leer), `message` (erste Zeile, max. 500 Zeichen), `location` (`<crate-relativer Pfad>:<Zeile>` oder leer), `backtrace` (Liste von Funktionsnamen, max. 64 Einträge, ohne absolute Pfade), `crawl_running` (Boolean), `rendering_active` (Boolean), `log_tail` (letzte max. 200 Logzeilen des betroffenen Laufs; nur Zeilen von OpenSpider selbst – Module `os_*`/`openspider*`, Log der Oberfläche, Zeile des Panic-Hooks – in jedem Level; Zeilen fremder Bibliotheken nur bei `warn`/`error` und dann ohne Nachrichtentext und Felder als `<zeit> <LEVEL> <ziel>: <omitted>`; gefiltert wird vor dem Kürzen, **Klärung C01, 03.11.2026**), `sent` (Zeitstempel oder `null`), `server_ref` (Referenz des Servers oder `null`, OUT-247).
- **Datenschutz (verbindlich für alle Felder):** Vor dem Schreiben werden alle Texte (`message`, `backtrace`, `log_tail`) bereinigt: Geheimnisse wie in OUT-243; jede URL (`http`, `https`, `ftp`, `file`) → `<url>`; E-Mail-Adressen → `<email>`; IPv4/IPv6-Adressen → `<ip>`; der Benutzer-Heimatordner und `OPENSPIDER_HOME` → `~`. Hostnamen, die in einer URL desselben Berichts vorkamen, werden auch ohne Schema überall im Bericht ersetzt (Bibliotheks-Logs nennen sie einzeln). Der Bericht enthält keine Crawl-Daten, keine Konfigurationswerte, keine Dateinamen von Crawls oder Exporten und keine Projektnamen; einzige Ausnahme sind die Zeilen der Konfigurations-Zusammenfassung aus OUT-240 (Modus, Limits, Rendering an/aus) im `log_tail`, deren URLs ebenfalls bereinigt sind **(Klärung C01, 03.11.2026)**. **Klärung C01/Fable (03.11.2026):** Auch in eigenen Logzeilen werden die Werte von Feldern, die Namen oder Orte des Nutzers tragen (`name`, `project`, `path`, `file`, `dir`, `title`, im Text- wie im JSON-Format), durch `<omitted>` ersetzt. Absolute Pfade außerhalb von `~` (Unix `/…`, Windows `X:\…`, `\\?\…`, UNC) werden zu `<path>` bzw. `<path>/<file>` gekürzt; erhalten bleiben nur die crate-relative Form von `location` und Pfade unter `~`. Von der Panic-Logzeile (OUT-240) übernimmt `log_tail` nur die erste Zeile, die Rückverfolgung steht ausschließlich in `backtrace`.
- **Akzeptanzkriterien:**
  - [ ] Ein ausgelöster Panic (Testbefehl, nur in Test-Builds) erzeugt genau einen Bericht `kind = panic` mit gefüllten Feldern `message`, `location` und `backtrace`; der Bericht ist geschrieben, bevor der Prozess endet.
  - [ ] Ein ausgelöster Fehler in einer Fehlergrenze der Oberfläche erzeugt genau einen Bericht `kind = frontend` mit Fehlermeldung und Komponenten-Stack als `backtrace`; je App-Lauf höchstens 5 `frontend`-Berichte, gleiche Meldung und Stelle nur einmal.
  - [ ] Hartes Beenden des App-Prozesses, anschließender Neustart → genau ein Bericht `kind = unclean_exit` mit `log_tail` aus dem beendeten Lauf; ein zweiter Neustart erzeugt keinen weiteren. Geordnetes Beenden (auch über das Betriebssystem beim Abmelden) erzeugt keinen Bericht.
  - [ ] Laufen zwei App-Prozesse gleichzeitig, meldet keiner den anderen als abgestürzt (Marke mit laufender PID und gleicher Startzeit wird nicht ausgewertet); eine Marke mit wiederverwendeter PID, aber anderer Startzeit, gilt als Absturz.
  - [ ] CLI (`process = cli`): ein Panic schreibt denselben Bericht und gibt dessen Pfad auf stderr aus; die CLI zeigt keinen Dialog und sendet nie. `unclean_exit` wird nur für die App erkannt.
  - [ ] Datenschutz-Test: Crawl auf eine Fixture mit dem Hostnamen `crash-secret.test`, einer URL mit `?token=SEKRET-246`, einem Basic-Auth-Kennwort `pw-SEKRET-246`, einer E-Mail-Adresse im Log und `OPENSPIDER_HOME` als Basispfad; dann Panic. Suche im Bericht nach `crash-secret.test`, `SEKRET-246`, der E-Mail-Adresse, dem absoluten `OPENSPIDER_HOME`-Pfad und nach Seiteninhalten der Fixture (Meta-Description, relative Pfade) – auch mit `--log-level debug` → 0 Treffer; `<url>` und `~` kommen vor.
  - [ ] Aufbewahrung: höchstens 20 Berichte und höchstens 5 MB im Ordner `crashes/`; ältere werden beim App-Start zuerst gelöscht, ebenso Berichte älter als 30 Tage. Ein einzelner Bericht ist höchstens 64 KB groß (zuerst wird `log_tail` von vorn gekürzt, dann `backtrace`).
  - [ ] Fehler beim Schreiben des Berichts (Ordner schreibgeschützt, Platte voll) lösen keinen zweiten Panic aus; der Prozess endet wie ohne Bericht.
  - [ ] Das Diagnosepaket (OUT-244) enthält die gespeicherten Berichte; ohne einen Klick auf „Senden“ (OUT-247) erfolgt kein Netzwerkzugriff (Netzwerkmitschnitt über Panic, Neustart und Öffnen des Hinweises).
- **Testfixture:** Integrationstests mit `OPENSPIDER_HOME` und Testbefehl für Panic bzw. Frontend-Fehler; Fixture-Site `crash-secret.test` (lokaler Mock-Server); Unit-Tests für die Bereinigung und die Sitzungsmarke (gemockte Prozessliste).

### OUT-247 Absturzbericht ansehen und senden (nur mit Zustimmung)
- **Priorität:** P1
- **Beschreibung:** **OS-Festlegung (Nutzer 03.11.2026):** Gibt es beim App-Start ungesendete Berichte aus OUT-246, die der Nutzer noch nicht verworfen hat, zeigt das Hauptfenster einen nicht-modalen Hinweis: „OpenSpider hatte beim letzten Lauf ein Problem.“ (en: „OpenSpider ran into a problem last time.“) mit den Schaltflächen „Bericht ansehen…“, „Senden“ und „Nicht senden“. Der Hinweis betrifft nur Berichte früherer Läufe; Berichte des laufenden Prozesses erscheinen erst beim nächsten Start. Gesendet wird nur nach einem Klick auf „Senden“ – nie automatisch, nie im Hintergrund. Zusätzlich listet Hilfe → „Absturzberichte…“ alle gespeicherten Berichte (Zeit, Art, gesendet ja/nein, Server-Referenz) mit „Ansehen“, „Senden“ und „Löschen“.
  - „Bericht ansehen…“ zeigt genau den JSON-Text, der gesendet würde (schreibgeschützt, kopierbar), dazu zwei optionale Felder: „Was haben Sie zuletzt gemacht?“ (max. 2 000 Zeichen) und „E-Mail für Rückfragen“ (max. 254 Zeichen, gültige Adresse oder leer). Diese Felder werden nur gesendet, nicht im Bericht gespeichert.
  - Senden: `POST https://openspider.de/api/crash`, `Content-Type: application/json`, Körper `{"report": <Bericht>, "note": "<Text oder leer>", "email": "<Adresse oder leer>"}`, über die app-weite Proxy-Einstellung (CRAWL-122), Zeitlimit 15 s, kein automatischer Wiederholversuch. Bei mehreren ungesendeten Berichten sendet „Senden“ die neuesten 5 einzeln nacheinander; ältere bleiben ungesendet in der Liste.
  - Antwort `200` mit `{"ok": true, "ref": "<Referenz>"}` → Bericht erhält `sent` und `server_ref`; der Hinweis zeigt „Danke! Referenz: <Referenz>“ – die Referenz kann der Nutzer in einem GitHub-Issue angeben. Jede andere Antwort, Zeitüberschreitung oder fehlende Verbindung → Meldung „Senden fehlgeschlagen“ mit Ursache und „Erneut versuchen“; der Bericht bleibt ungesendet.
  - „Nicht senden“ blendet den Hinweis für die betroffenen Berichte dauerhaft aus; sie bleiben bis zur Aufbewahrungsgrenze (OUT-246) in Hilfe → „Absturzberichte…“.
  - Einstellung (Allgemein, `settings.toml` OUT-222, Schlüssel `crash_reports.prompt`): „Nach einem Absturz fragen, ob ein Bericht gesendet werden soll“ (Standard: an). Aus → kein Hinweis beim Start; Berichte werden weiter lokal gespeichert und sind über das Hilfe-Menü sendbar.
  - Verwaltete Umgebungen: Richtlinienwert `DisableCrashReports` (gleiche Quellen und Rangfolge wie `DisableUpdateCheck` in OUT-253; Umgebungsvariable `OPENSPIDER_DISABLE_CRASH_REPORTS`). Gesetzt → kein Hinweis, „Senden“ ausgegraut mit „Durch Administrator deaktiviert“; lokales Speichern bleibt.
- **Server (`website/api/crash.php`, gleiche Betriebsregeln wie das Feedback-Skript):** nimmt nur `POST` mit `application/json` bis 80 KB an; prüft `format = 1`, die Feldnamen und -typen aus OUT-246 und die Längen von `note`/`email`; speichert keine IP-Adresse; Ratenbegrenzung 10 Berichte je Stunde je (gesalzener, täglich wechselnder) IP-Kennung; legt den Bericht als JSON-Datei im Datenordner ab und sendet eine Mail an die konfigurierte Adresse mit Betreff `[OpenSpider Absturz] <app_version> <os>/<arch> <kind>: <message gekürzt>` und dem Bericht im Text. Die Referenz ist eine kurze, zufällige Kennung (8 Zeichen, Base32). Antworten: `200 {"ok":true,"ref":…}`, `400 {"ok":false,"error":"invalid"}`, `413 … "too_large"`, `429 … "rate"`, `500 … "server"`.
- **Akzeptanzkriterien:**
  - [ ] Nach einem Absturz (OUT-246) zeigt der nächste Start den Hinweis; ohne ungesendete, nicht verworfene Berichte erscheint er nicht. Beide Sprachen übersetzt.
  - [ ] „Bericht ansehen…“ zeigt Byte für Byte den Bericht, der im Körper als `report` gesendet wird (Test vergleicht angezeigten Text und Mock-Server-Eingang).
  - [ ] Netzwerkmitschnitt: Start mit Hinweis, Ansehen, „Nicht senden“, erneuter Start → 0 Verbindungen zu `openspider.de`. Erst „Senden“ erzeugt genau eine Anfrage je Bericht.
  - [ ] Mock-Server mit Antwort `200` → Bericht hat danach `sent` und `server_ref`, Hinweis zeigt die Referenz; mit `500`, `429` und Zeitüberschreitung → Fehlermeldung, Bericht bleibt ungesendet, „Erneut versuchen“ sendet erneut.
  - [ ] Einstellung aus → kein Hinweis; Richtlinie `DisableCrashReports` gesetzt → kein Hinweis und keine Anfrage, auch nicht über Hilfe → „Absturzberichte…“ (Rangfolgetest wie OUT-253 über den gemeinsamen Richtlinien-Resolver).
  - [ ] Senden läuft nicht im UI-Thread und beeinflusst einen laufenden Crawl nicht (keine Pause, kein Neustart).
  - [ ] Server-Tests (`website/api/tests/`): gültiger Bericht → 200 mit 8-stelliger Referenz, Datei gespeichert, Mail mit erwartetem Betreff; falsches `format`, fehlendes Feld, falscher Typ, ungültige E-Mail → 400; 81 KB → 413; 11. Bericht derselben IP-Kennung in einer Stunde → 429; kein Klartext der IP in Datei, Mail oder Fehlerlog.
  - [ ] Die Datenschutzerklärung der Website (de und en) beschreibt Absturzberichte: Inhalt, Zweck, Speicherdauer auf dem Server und dass nur nach Klick gesendet wird.
- **Testfixture:** Mock-Server für `/api/crash` (Antworten 200/400/429/500/Zeitüberschreitung); Bericht-Fixture `crash-panic.json` (aus OUT-246); PHP-Testfälle analog `feedback_test.php`.

---

## 12 Update-Mechanismus

### OUT-250 Update-Prüfung
- **Priorität:** P1
- **Beschreibung:** Die Desktop-App prüft beim Start (höchstens einmal je 24 h) und manuell über Hilfe → „Nach Updates suchen“ ein signiertes Update-Manifest (z. B. auf GitHub Releases). Führend für Update-Prüfung; CRAWL-142 verweist hierher.
- **Eingaben/Konfiguration:** Einstellungen → Updates: „Automatisch nach Updates suchen“ (Standard an); Schlüssel `updates.check = false` in `settings.toml`; Umgebungsvariable `OPENSPIDER_DISABLE_UPDATE_CHECK=1` (für verwaltete Umgebungen) **(OS-Festlegung)**.
- **Akzeptanzkriterien:**
  - [ ] Die Anfrage überträgt nur Version, Betriebssystem und Architektur (keine Nutzerkennung).
  - [ ] Offline/Timeout (5 s) → keine Fehlermeldung beim Start, nur Log-Eintrag.
  - [ ] Abgeschaltet → kein Netzwerkzugriff auf den Update-Server (Netzwerkmitschnitt).
  - [ ] Das CLI prüft nie automatisch; `openspider --check-update` gibt verfügbare Version aus (Exit-Code 0 = aktuell, 10 = Update verfügbar). Ein geplanter Lauf erzeugt keinen Request an den Update-Server (Netzwerkmitschnitt) (übernommen aus CRAWL-142).
  - [ ] Eine Update-Prüfung oder ein Update-Download während eines laufenden Crawls verursacht keinen Neustart, keine Pause und keine Änderung der Crawl-Rate um mehr als 5 % (Messung während simuliertem Download) (übernommen aus CRAWL-142).
  - [ ] Die Update-Einstellung gehört zu den App-Einstellungen (`settings.toml`, OUT-222), nicht zum Crawl-Profil, und erscheint nicht im Konfigurations-Diff (OUT-224).
- **Testfixture:** Mock-Update-Server im `os-testsite`.

### OUT-251 Herunterladen und Installieren
- **Priorität:** P1
- **Beschreibung:** Ein verfügbares Update wird im Hintergrund heruntergeladen; danach bietet die App „Jetzt installieren und neu starten“ oder „Beim Beenden installieren“ an. Release-Notes werden angezeigt.
- **Akzeptanzkriterien:**
  - [ ] Während eines laufenden Crawls wird kein Neustart erzwungen; Installieren ist erst nach Pause/Ende möglich (ein pausierter Crawl wird vorher konsistent geschlossen und bleibt fortsetzbar); während eines laufenden Crawls bietet der Dialog „Nach Crawl-Ende installieren“ (übernommen aus CRAWL-142).
  - [ ] Abgebrochener Download wird beim nächsten Start fortgesetzt oder verworfen, nie halb installiert.
  - [ ] Nach dem Update startet die App neu und öffnet den zuletzt geöffneten Crawl.
  - [ ] Update aktualisiert auch das CLI-Binary und registrierte Aufgaben (OUT-166).
  - [ ] **OS-Festlegung (Nutzerentscheidung 04.11.2026):** Die angezeigten Release-Notes (Feld `notes` in `latest.json`/`latest-beta.json`) sind ausschließlich die CHANGELOG-Notes der Version, ohne den Download-Kasten des GitHub-Release-Texts (OUT-261) und ohne Entwurfshinweise.
  - [ ] **OS-Festlegung (Notes vor dem Bau, `docs/plan/beta3.md` §5, B3-REL 05.11.2026):** Die Notes jeder Version stehen vor dem Bauen als eingechecktes Dokument `docs/release/notes/<Version>.md` fest. Es ist die einzige Quelle für `notes` in beiden Manifesten und für den Release-Text (OUT-261); der CHANGELOG-Eintrag der Version muss wortgleich sein. Fehlt das Dokument, ist es leer, enthält es Überschriften der Ebene 1/2 oder Entwurfsmarkierungen (TODO/STUB) oder weicht der CHANGELOG-Eintrag ab, bricht der Release-Lauf vor dem Bauen mit einer Meldung ab, die Datei und Abhilfe nennt.
  - [ ] Die Aktionsschaltflächen des Update-Dialogs („Update installieren“, „Beim Beenden installieren“, „Nach Crawl-Ende installieren“, „Diese Version überspringen“, „Später erinnern“, „Im Hintergrund herunterladen“, „Schließen“) sind in allen Oberflächensprachen einzeilig, ohne abgeschnittenen Text, bei der minimalen Fenstergröße 1024×640 (UI-001) und bei 800 px Fensterbreite ohne horizontalen Überlauf; Prüfung per E2E (B3-UI, B4-UPDDLG).
  - [ ] **OS-Festlegung (Nutzerentscheidung 09.11.2026, B4-UPDDLG):** Aufbau des Dialogs bei einem verfügbaren Update wie ein klassischer macOS-Update-Dialog, von oben nach unten: (1) Kopf mit App-Symbol links, rechts fett „Eine neue Version von OpenSpider ist verfügbar!“ und darunter „OpenSpider <neu> ist verfügbar – Sie haben <installiert>. Möchten Sie die neue Version jetzt installieren?“; (2) scrollbarer Notes-Kasten mit Überschrift „OpenSpider <Version>“ und den gerenderten Notes in der Oberflächensprache (Regel wie OUT-257: Deutsch → Teil unter `### Deutsch`, sonst englischer Teil; Listen, Fettdruck, Links im Standardbrowser, kein HTML ausgeführt); (3) Kontrollkästchen mit der vorhandenen Einstellung „Automatisch nach Updates suchen“ (OUT-250; eine Einstellung zum automatischen Installieren gibt es nicht); (4) eine Knopfreihe nach dem Fußleisten-Muster der übrigen Dialoge: links „Diese Version überspringen“, rechts „Später erinnern“, „Beim Beenden installieren“ und als Primärknopf „Update installieren“ (= Jetzt installieren und neu starten; auf macOS rechts außen, auf Windows zuerst). Während eines laufenden Crawls ersetzt „Nach Crawl-Ende installieren“ beide Installationsknöpfe. Die Installationsknöpfe sind aktiv, sobald der Hintergrund-Download fertig ist; bis dahin zeigt der Dialog den Fortschritt. Esc schließt wie „Später erinnern“, der Fokus kehrt zurück (UI-126).
  - [ ] **OS-Festlegung (Nutzerentscheidung 09.11.2026):** „Diese Version überspringen“ merkt sich die Version in den App-Einstellungen (`updates.skipped_version`, OUT-222); automatische Prüfungen melden danach nur Versionen, die nach SemVer größer als die übersprungene sind (Vorabversionen eingeschlossen: `1.0.0-beta.4` < `1.0.0-rc.1` < `1.0.0`; Entscheidung Orchestrator 09.11.2026). Die manuelle Prüfung (Hilfe → „Nach Updates suchen“) zeigt auch die übersprungene Version. „Später erinnern“ schließt nur; das Update wird bei der nächsten regulären Prüfung (OUT-250, höchstens einmal je 24 h beim Start) wieder gemeldet.
  - [ ] **OS-Festlegung (Nutzerentscheidung 09.11.2026):** Das Feld `notes` der Update-Manifeste enthält nur die kurzen Update-Notes der Version (je Sprache 3 bis 6 knappe Punkte, Abschnitt `<!-- update -->` … `<!-- /update -->` in `docs/release/notes/<Version>.md`, Format in `docs/release/release-rhythmus.md`); die ausführlichen Notes bleiben für Release-Text (OUT-261) und CHANGELOG. `notes.mjs --check` bricht ab, wenn der kurze Abschnitt ab 1.0.0-beta.4 fehlt oder nicht je Sprache 3 bis 6 Punkte hat. Bereits veröffentlichte Versionen bis 1.0.0-beta.3 bleiben unverändert (ihr Manifest trägt die ausführlichen Notes).
- **Testfixture:** Mock-Update-Server, manueller Release-Test je OS.

### OUT-252 Integrität und Signatur von Updates
- **Priorität:** P0
- **Beschreibung:** Updates werden nur installiert, wenn die Paketsignatur (Ed25519, Tauri-Updater-Signatur) mit dem in der App eingebetteten öffentlichen Schlüssel übereinstimmt; zusätzlich muss die OS-Codesignatur (Apple Developer ID / Authenticode) gültig sein.
- **Akzeptanzkriterien:**
  - [ ] Manipuliertes Paket (1 Byte geändert) → Installation verweigert, Warnung, Log-Eintrag.
  - [ ] Manifest über HTTP (statt HTTPS) wird abgelehnt.
  - [ ] Downgrade über den Updater ist nicht möglich (Versionsvergleich nach SemVer).
- **Testfixture:** Mock-Update-Server mit gültigem und manipuliertem Paket.

### OUT-253 Updates in verwalteten Umgebungen
- **Priorität:** P2
- **Beschreibung:** Administratoren können Updates zentral steuern: MSI-Installationen per Softwareverteilung, Update-Prüfung per Richtlinie deaktiviert **(OS-Festlegung)**:
  - Windows: Registry `HKLM\Software\Policies\OpenSpider` bzw. `HKCU\Software\Policies\OpenSpider`, Wert `DisableUpdateCheck` (REG_DWORD); MSI-Eigenschaft `DISABLEUPDATECHECK=1` schreibt den HKLM-Wert bei der Installation.
  - macOS: verwaltete Einstellungen (Konfigurationsprofil) in der Domain `org.openspider.desktop` (= Bundle-ID), Schlüssel `DisableUpdateCheck` (Boolean).
  - Rangfolge: HKLM/Profil > HKCU > Umgebungsvariable `OPENSPIDER_DISABLE_UPDATE_CHECK` > `settings.toml` (OUT-250).
- **Akzeptanzkriterien:**
  - [ ] `DisableUpdateCheck`=1 (bzw. `true`) → kein Netzwerkzugriff auf den Update-Server (Netzwerkmitschnitt über 2 App-Starts und manuelles „Nach Updates suchen“); der Menüpunkt zeigt „Durch Administrator deaktiviert“.
  - [ ] Wert `0`/`false` oder nicht vorhanden → Benutzereinstellung gilt.
  - [ ] Rangfolgetest mit allen Kombinationen von HKLM ∈ {fehlt, 0, 1} × HKCU ∈ {fehlt, 0, 1}: ein explizit gesetzter HKLM-Wert entscheidet immer (HKLM=0, HKCU=1 → Prüfung aktiv); nur wenn HKLM fehlt, entscheidet HKCU; fehlen beide, gilt die Benutzereinstellung.
  - [ ] Gesetzte Richtlinie überschreibt die Benutzereinstellung; die Option ist in der UI ausgegraut mit Hinweis „durch Richtlinie festgelegt“.
  - [ ] Ungültiger Typ (z. B. REG_SZ „ja“) → wird ignoriert, Warnung im Log.
  - [ ] `msiexec /i OpenSpider.msi /qn DISABLEUPDATECHECK=1` → Registry-Wert gesetzt; Deinstallation entfernt ihn.
- **Testfixture:** Unit-Tests des Richtlinien-Resolvers mit gemocktem Registry-/Defaults-Backend; manueller Test in Windows-VM und mit macOS-Konfigurationsprofil.

### OUT-254 Update-Kanäle
- **Priorität:** P2
- **Beschreibung:** Kanäle `stable` (Standard) und `beta`; Wechsel in den Einstellungen.
- **Akzeptanzkriterien:**
  - [ ] Wechsel von beta zu stable bietet erst bei einer neueren stable-Version ein Update an (kein Downgrade).
- **Testfixture:** Mock-Update-Server.

### OUT-255 Datenkompatibilität über Versionen
- **Priorität:** P0
- **Beschreibung:** Nach einem Update bleiben alle Crawls, Konfigurationen, Presets, Aufgaben und Einstellungen nutzbar (Migration gemäß OUT-226).
- **Akzeptanzkriterien:**
  - [ ] CI-Test: Crawl-Dateien und Konfigurationen jeder veröffentlichten Version (Fixture-Archiv) werden mit der aktuellen Version geöffnet und exportiert.
- **Testfixture:** `tests/fixtures/migrations/`.

### OUT-257 Neuigkeiten nach einem Update
- **Priorität:** P1
- **Beschreibung:** **OS-Festlegung (Nutzerentscheidung 09.11.2026):** Beim ersten Start einer neueren Version nach einem Update zeigt die App einmal einen modalen Dialog „Neu in OpenSpider <Version>“ mit den Release-Notes. Quelle sind die Notes unter `docs/release/notes/<Version>.md` (englischer Teil, darunter `### Deutsch` vollständig auf Deutsch, siehe `docs/release/release-rhythmus.md`); sie werden beim Build in die App eingebettet, zum Anzeigen gibt es keinen Netzwerkzugriff. Sprache: Oberflächensprache Deutsch → deutscher Teil, jede andere Oberflächensprache → englischer Teil. Wurden Versionen übersprungen, zeigt der Dialog die Notes aller eingebetteten Versionen zwischen der zuletzt gestarteten (ausschließlich) und der aktuellen Version (einschließlich), neueste zuerst, je mit Versionsüberschrift. Die zuletzt gestartete Version wird in den Einstellungen gespeichert (bleibt nach OUT-255 über Updates erhalten). Hilfe-Menü → „Neuigkeiten“ öffnet den Dialog jederzeit mit den Notes der aktuellen Version. Die CLI zeigt nichts an.
- **Wann nicht:** Erstinstallation (weder gespeicherte Version noch Daten einer früheren Installation); gleiche oder ältere Version (Downgrade); fehlende eingebettete Notes für die aktuelle Version (dann kein Dialog, Log-Eintrag).
- **Übergang:** Fehlt die gespeicherte Version, sind aber Einstellungen oder Crawl-Verwaltungsdaten einer früheren Installation vorhanden (Update von einer Version ohne diese Funktion, z. B. 1.0.0-beta.3 → 1.0.0-beta.4), zeigt der Dialog die Notes der aktuellen Version.
- **Bedienung:** Fokus im Dialog, Schließen mit „OK“ oder Esc, danach Fokusrückgabe (UI-126); Inhalt scrollbar; Listen und Hervorhebungen der Notes werden dargestellt, eingebettetes HTML nicht ausgeführt; Links öffnen im Standardbrowser. Kein Schalter „Nicht mehr anzeigen“. Erscheint erst, nachdem die App bereit ist, und nie über einem Dialog, der eine Entscheidung verlangt (z. B. Wiederherstellung nach Absturz); dann danach.
- **Akzeptanzkriterien:**
  - [ ] Gespeicherte Version 1.0.0-beta.3, aktuelle 1.0.0-beta.4 → Dialog einmal mit den Notes von beta.4; nach „OK“ und Neustart kein Dialog mehr.
  - [ ] Gespeicherte Version 1.0.0-beta.2, aktuelle 1.0.0-beta.4 → Notes von beta.4 und beta.3, neueste zuerst.
  - [ ] Erstinstallation (leeres Profil) → kein Dialog; gespeicherte Version danach = aktuelle.
  - [ ] Keine gespeicherte Version, aber vorhandene Einstellungen einer früheren Installation → Dialog mit den Notes der aktuellen Version.
  - [ ] Gleiche oder höhere gespeicherte Version → kein Dialog.
  - [ ] Oberflächensprache Deutsch → deutscher Teil; Englisch, Spanisch, Französisch, Italienisch → englischer Teil.
  - [ ] Hilfe → „Neuigkeiten“ zeigt die Notes der aktuellen Version, auch ohne Update.
  - [ ] Bedienung nur per Tastatur möglich, Esc schließt, Fokus kehrt zurück; axe ohne kritische oder schwerwiegende Verstöße (UI-126).
  - [ ] Kein Netzwerkzugriff beim Anzeigen (Netzwerkmitschnitt bzw. Test mit blockiertem Netz).
- **Testfixture:** Profilverzeichnisse mit gespeicherter Version beta.2/beta.3/beta.4, leeres Profil, Profil mit Einstellungen ohne gespeicherte Version; eingebettete Test-Notes für mehrere Versionen.

---

## 13 Plattformanforderungen

### OUT-260 macOS Apple Silicon (arm64)
- **Priorität:** P0
- **Beschreibung:** Native arm64-Builds von App, CLI und gebündelten Komponenten. **OS-Festlegung (Nutzer 03.11.2026, K3-01):** Chromium für das Rendering wird nicht gebündelt, sondern auf Anfrage als feste, per SHA-256 geprüfte Version heruntergeladen (System-Chrome als Rückfall); es läuft nativ in der jeweiligen Architektur.
- **Akzeptanzkriterien:**
  - [ ] Unterstützt macOS 13 (Ventura) und neuer **(OS-Festlegung;** Entscheidung (Fable-Empfehlung) vom 13.10.2026: WKWebView ab Safari 16.2 wird für `color-mix(in oklch)`, `forced-colors` und das Canvas-Verhalten der Tabelle vorausgesetzt; vorher macOS 12**)**. `LSMinimumSystemVersion` = 13.0.
  - [ ] Kein Prozess läuft unter Rosetta 2 (Prüfung über Aktivitätsanzeige/`sysctl.proc_translated`).
  - [ ] Vollständige Testsuite läuft in CI auf `macos-14` (arm64).
- **Testfixture:** CI-Matrix.

### OUT-261 macOS Intel (x86_64)
- **Priorität:** P0
- **Beschreibung:** Native x86_64-Builds; Auslieferung **[P1]** als Universal-Binary. **OS-Festlegung (Nutzerentscheidung 04.11.2026):** Für macOS wird nur das Universal-DMG veröffentlicht (eine Datei für Apple Silicon und Intel, weniger Dateien auf der Release-Seite). DMGs und CLI-Archive je Architektur werden weiter gebaut und geprüft, soweit Installations-Smoke und Updater sie brauchen, aber nicht als Release-Dateien hochgeladen; das Universal-Updater-Archiv ebenfalls nicht. Automatische Updates nutzen weiterhin die Updater-Pakete je Architektur.
- **Akzeptanzkriterien:**
  - [ ] Unterstützt macOS 13 und neuer (wie OUT-260).
  - [ ] Vollständige Testsuite in CI auf `macos-13` (x64). **OS-Festlegung (Nutzer 03.11.2026):** Intel-Macs sind kaum noch verbreitet; im Release-Lauf läuft die Intel-Testsuite weiter, ein Fehlschlag ist aber nur eine Warnung und hält den Entwurf nicht auf. Bau, Signatur, Notarisierung und Start-Smoke des x86_64- und des Universal-DMG bleiben blockierend.
  - [ ] Das Update-Manifest liefert je Architektur das passende Paket.
  - [ ] [P1] Das Universal-DMG enthält App und CLI mit beiden Architekturen (`lipo -archs` meldet `x86_64 arm64`) und startet auf beiden nativ.
  - [ ] **OS-Festlegung (Nutzerentscheidung 04.11.2026):** Ein Release enthält für macOS genau `OpenSpider_<v>_universal.dmg`, `openspider_<v>_darwin-universal.tar.gz` und die Updater-Pakete `OpenSpider_aarch64.app.tar.gz`/`OpenSpider_x86_64.app.tar.gz` samt `.sig`; `OpenSpider_<v>_aarch64.dmg`, `OpenSpider_<v>_x64.dmg`, `openspider_<v>_darwin-{aarch64,x86_64}.tar.gz` und `OpenSpider_universal.app.tar.gz(.sig)` fehlen. Zusätzlich liegt eine versionslose, bytegleiche Kopie `OpenSpider-macOS.dmg` des Universal-DMG bei, sodass `https://github.com/<repo>/releases/latest/download/OpenSpider-macOS.dmg` immer das neueste Release lädt; beide stehen in `SHA256SUMS`. Fehlt die Kopie oder weicht sie ab, entsteht kein Release-Entwurf.
  - [ ] **OS-Festlegung (Nutzerentscheidung 04.11.2026):** Der Text des GitHub-Releases beginnt mit einem Download-Kasten: Tabelle „System | Datei“ mit macOS 13+ (Apple Silicon und Intel) → Universal-DMG und Windows 10/11 (x64, auch Windows on ARM) → NSIS-Setup, jeweils als Link auf die versionierte Datei dieses Releases; danach ein Satz (de, darunter kursiv en), wofür die übrigen Dateien sind (automatische Updates, CLI, Firmen-Installationen per MSI, Prüfsummen), Links auf Benutzerhandbuch (de/en) und Feedback, dann „## Neu in dieser Version“ mit den Notes der Version (OUT-251). **(B3-REL, `docs/plan/beta3.md` §5:)** Der Feedback-Link zeigt bei Vorabversionen (Version mit `-`, z. B. `-beta.3`, `-rc.1`) auf `https://openspider.de/beta/`, bei stabilen Versionen auf die GitHub Discussions des Repositorys. Der Release-Text enthält nichts weiter; Entwurfshinweise (unsigniert, Installations-Smoke, offene Parität) stehen nur in der Zusammenfassung des Actions-Laufs, ein Release-Text mit solchen Hinweisen wird nicht als Entwurf angelegt.
- **Testfixture:** CI-Matrix.

### OUT-262 Windows x64
- **Priorität:** P0
- **Beschreibung:** Windows 10 ab Version 1809 und Windows 11, 64 Bit, mit WebView2-Evergreen-Runtime (Entscheidung (Fable-Empfehlung) vom 13.10.2026; vorher „Windows 10 (22H2)“). Die Oberfläche setzt die jeweils aktuelle Evergreen-Version voraus; eine fest gebündelte WebView2-Version wird nicht unterstützt.
- **Akzeptanzkriterien:**
  - [ ] Benötigte WebView2-Runtime wird vom Installer geprüft und bei Bedarf installiert (Bootstrapper, offline-Variante im MSI-Paket optional).
  - [ ] Pfade > 260 Zeichen funktionieren für Exporte und Crawl-Dateien (Long-Path-fähige APIs).
  - [ ] Funktioniert in einer Remote-Desktop-Sitzung (RDP): App-Start, Crawl mit JS-Rendering, Trennen und Wiederverbinden der Sitzung während des Crawls sowie Wechsel der Bildschirmauflösung führen nicht zum Absturz; der Crawl läuft weiter.
  - [ ] Vollständige Testsuite in CI auf `windows-latest`.
- **Testfixture:** CI-Matrix + manueller RDP-Test pro Release.

### OUT-263 Windows on ARM (arm64)
- **Priorität:** P1
- **Beschreibung:** Kern (P1, ab erstem Release): Der x64-Build wird unter der Windows-11-Emulation auf arm64 unterstützt. **[P2]** Native arm64-Builds für Windows 11 on ARM (Phase 2).
- **Akzeptanzkriterien:**
  - Phase 1 (Emulation):
    - [ ] x64-Installer (MSI und NSIS) installiert auf Windows 11 arm64 ohne Fehler.
    - [ ] Smoke-Suite unter Emulation besteht: Crawl von `out-basic-site` mit und ohne JS-Rendering, Export `Internal:All` byte-identisch zur x64-Referenz, XML-Sitemap-Erzeugung, geplante Aufgabe läuft zur geplanten Zeit.
    - [ ] Crawl-Geschwindigkeit unter Emulation ≥ 50 % der nativen x64-Leistung auf vergleichbarer Hardware (Richtwert, Messung dokumentiert).
  - Phase 2 (nativ):
    - [ ] [P2] Eigene arm64-Pakete; der Installer wählt automatisch die passende Architektur, das Update-Manifest liefert je Architektur das passende Paket.
    - [ ] [P2] Kein Prozess (App, CLI, Chromium) läuft emuliert (Prüfung über Task-Manager-Spalte „Architektur“).
    - [ ] [P2] Vollständige Testsuite in CI auf einem Windows-arm64-Runner.
- **Testfixture:** Smoke-Suite `tests/smoke/` auf Windows-11-arm64-VM bzw. Runner.

### OUT-264 Linux
- **Priorität:** P2
- **Beschreibung:** Nicht Zielplattform des Projekts (Zielplattformen: macOS arm64/x64, Windows x64). **Bewusste Paritätslücke:** Die Referenz liefert Linux-Pakete für Ubuntu (`.deb`) und Fedora (`.rpm`) und seit einer früheren Referenzversion zusätzlich Arm64-Linux-Pakete (Ubuntu arm64, Fedora aarch64). OpenSpider bietet bis auf Weiteres keine Linux-Pakete und keine Linux-Desktop-App an; diese Lücke wird in der Paritätsmatrix (README/Website) ausdrücklich als „nicht unterstützt“ geführt. Kern und CLI unter Linux x86_64 sind seit dem Abgleich mit der Cloud-Variante (16.10.2026) als OUT-273 (P1) geprüft; dieser Eintrag behält aarch64, Pakete und die Desktop-App. Pakete `.deb`/`.rpm` (x86_64 und arm64) sind Kandidaten für eine spätere Version.
- **Akzeptanzkriterien:**
  - [ ] `cargo build -p os-cli` gelingt in CI auf einem aarch64-Linux-Runner bzw. per Cross-Build (Build-Check, keine Release-Pflicht). x86_64 einschließlich Golden-Vergleich und headless Chromium: siehe OUT-273.
  - [ ] Die Paritätsmatrix in der Doku listet Linux-Desktop und Linux-Pakete (x64/arm64) als bewusste Lücke mit Verweis auf diesen Eintrag.
- **Testfixture:** CI-Build-Job; `out-basic-site`.

### OUT-265 macOS: Paketierung, Signatur, Notarisierung
- **Priorität:** P0
- **Beschreibung:** Auslieferung als signiertes, notarisiertes DMG mit Hardened Runtime; App startet ohne Gatekeeper-Warnung.
- **Akzeptanzkriterien:**
  - [ ] `spctl --assess --type execute OpenSpider.app` → accepted; `stapler validate` → ok.
  - [ ] Alle eingebetteten Binärdateien (CLI, Hilfsprozesse) sind signiert. Chromium ist nicht eingebettet (K3-01, Nutzer 03.11.2026); das auf Anfrage geladene Chromium trägt die Signatur seines Herstellers und wird vor dem Entpacken per SHA-256 geprüft.
  - [ ] Benötigte Berechtigungen sind minimal (Netzwerk-Client; keine Kamera/Mikrofon; Zugriff auf vom Nutzer gewählte Dateien).
- **Testfixture:** Release-Pipeline-Check.

### OUT-266 Windows: Installer und Signatur
- **Priorität:** P0
- **Beschreibung:** Zwei Installer-Varianten: MSI (für Softwareverteilung, Per-Machine) und EXE/NSIS (Per-User, ohne Admin-Rechte). Alle Binärdateien Authenticode-signiert.
- **Eingaben/Konfiguration:** Stille Installation: `msiexec /i OpenSpider.msi /qn INSTALLDIR="…" ADDTOPATH=1`; NSIS: `OpenSpider-setup.exe /S /D=<pfad>`; stille Deinstallation entsprechend.
- **Akzeptanzkriterien:**
  - [ ] SmartScreen zeigt den verifizierten Herausgeber.
  - [ ] Stille Installation und Deinstallation ohne Dialoge (Exit-Code 0).
  - [ ] Deinstallation entfernt Programmdateien, Startmenü-Einträge, PATH-Eintrag und geplante Aufgaben; Benutzerdaten (Crawls, Einstellungen) bleiben, außer der Nutzer wählt „Benutzerdaten entfernen“ (interaktiv) bzw. `REMOVEUSERDATA=1`.
  - [ ] **OS-Festlegung (Nutzerentscheidung 04.11.2026):** Neben `OpenSpider_<v>_x64-setup.exe` (samt `.sig`) und dem MSI (samt `.sig`) enthält jedes Release eine versionslose, bytegleiche Kopie `OpenSpider-Windows-Setup.exe` des NSIS-Setups für den festen Link `https://github.com/<repo>/releases/latest/download/OpenSpider-Windows-Setup.exe`; sie steht in `SHA256SUMS` und wird weder vom Update-Manifest noch von winget verwendet (beide verweisen auf die versionierten Dateien).
- **Testfixture:** Release-Pipeline-Test in Windows-VM.

### OUT-267 Hardware-Anforderungen und Ressourcenlimits
- **Priorität:** P1
- **Beschreibung:** Mindestens 4 GB RAM, empfohlen 8 GB (bis ~1 Mio. URLs) bzw. 16 GB (darüber), SSD empfohlen. Statt einer JVM-Speicherzuteilung gibt es einstellbare Obergrenzen.
- **Eingaben/Konfiguration:** Einstellungen → Ressourcen: „Max. Arbeitsspeicher für Caches“ (Standard: 25 % des RAM, min. 512 MB), „Max. parallele Render-Prozesse“ (Standard: CPU-Kerne / 2, max. 8).
- **Akzeptanzkriterien:**
  - [ ] Crawl von 1 Mio. URLs (synthetische Fixture, ohne Rendering) bleibt auf einem 8-GB-Rechner unter 2 GB RSS.
  - [ ] Bei Speicherknappheit (verfügbarer RAM < 10 %) drosselt der Crawler und warnt, statt abzustürzen.
  - [ ] Es gibt kein festes URL-Limit; ab 5 Mio. URLs erscheint ein Hinweis zur Laufzeit/Datenträgergröße.
- **Testfixture:** synthetischer Großcrawl im nächtlichen CI-Job.

### OUT-268 Dateisystem, Pfade, Zeichenkodierung
- **Priorität:** P0
- **Beschreibung:** Plattformneutrale Behandlung von Pfaden und Dateinamen.
- **Akzeptanzkriterien:**
  - [ ] Unicode-Pfade (`Jürgen`, `日本`) funktionieren für alle Ein-/Ausgaben auf beiden Plattformen.
  - [ ] Dateinamen aus URLs werden für beide Dateisysteme gültig gemacht (OUT-008), case-insensitive Kollisionen auf macOS/Windows werden mit Hash-Suffix aufgelöst.
  - [ ] Keine hartkodierten Pfadtrenner (Lint-Regel im Code-Review, siehe `CLAUDE.md`).
- **Testfixture:** Unit-Tests mit Pfad-Edge-Cases.

### OUT-269 Proxy
- **Priorität:** P1
- **Verweis:** siehe CRAWL-122 (führend). Hier gilt zusätzlich, dass dieselbe app-weite Proxy-Einstellung (`settings.toml`, OUT-222) auch für Integrationen, Update-Prüfung (OUT-250), Google-Drive/Sheets- und E-Mail-Verbindungen (OUT-005, OUT-280) gilt.

### OUT-270 Vertrauenswürdige Zertifikate
- **Priorität:** P1
- **Beschreibung:** Zusätzlich zum OS-Zertifikatsspeicher können eigene CA-Zertifikate hinterlegt werden (z. B. für TLS-inspizierende Unternehmensproxys). „Ermitteln“ zeigt den Aussteller des Zertifikats einer Test-URL; „Hinzufügen“ übernimmt es.
- **Eingaben/Konfiguration:** Dateien `.crt`, `.pem`, `.cer`, `.der`.
- **Akzeptanzkriterien:**
  - [ ] Fixture mit selbstsigniertem Zertifikat: ohne Eintrag → TLS-Fehler (No Response); nach Hinzufügen der CA → 200.
  - [ ] Ungültige Datei → Fehlermeldung, keine Änderung.
  - [ ] Standard: OS-Zertifikatsspeicher wird genutzt (rustls mit plattformeigenem Verifier).
- **Testfixture:** `out-security` (eigene Test-CA).

### OUT-271 Oberflächensprache und Theme
- **Priorität:** P0
- **Verweis:** siehe UI-121 (führend). Das Theme regelt UI-120.

### OUT-272 Dateizuordnungen und Desktop-Integration
- **Priorität:** P1
- **Beschreibung:** Registrierung der Dateitypen `.ospider`, `.osconfig`, `.osauth`, `.ospreset`, `.ostask` mit Icon; Startmenü/Launchpad-Eintrag; **[P2]** Anzeige des Crawl-Fortschritts im Dock-/Taskleisten-Symbol.
- **Akzeptanzkriterien:**
  - [ ] Doppelklick auf `.ospider` öffnet die App mit dem Crawl (OUT-232).
  - [ ] [P2] Während eines Crawls zeigt das Dock-/Taskleisten-Symbol den Fortschritt (Prozent der bekannten URLs) und entfernt ihn bei Crawl-Ende.
- **Testfixture:** manueller Test je OS.

### OUT-273 Linux-Serverbetrieb: Kern und CLI (x86_64)
- **Priorität:** P1
- **Beschreibung:** **(OS-Festlegung,** Grundlage `docs/plan/autonomo-abgleich.md` 5.4, Etappe E0) OpenSpider läuft als Crawl-Arbeiter auf Linux-Servern: Kern-Crates, das CLI `openspider` und der MCP-Server über STDIO werden unter Linux x86_64 (glibc, `ubuntu-latest`) gebaut und mit der vollständigen Testsuite geprüft. Die Desktop-App (`openspider-desktop`) und Linux-Pakete sind nicht Teil dieses Eintrags (bleiben OUT-264, P2). Linux ist damit eine geprüfte Zusage für Kommandozeile und Kern, keine Zielplattform der App.
- **Eingaben/Konfiguration:** keine neuen Optionen; Chromium für JS-Rendering über den konfigurierten Pfad bzw. `OPENSPIDER_CHROMIUM` (K2-10 in `docs/plan/M2.md`).
- **Akzeptanzkriterien:**
  - [ ] CI-Job `linux-core` auf `ubuntu-latest` bei jedem Push und Pull Request: `cargo fmt --check`, `cargo clippy` mit `-D warnings` und `cargo test` für den Workspace ohne `openspider-desktop`, jeweils mit `--locked`, grün.
  - [ ] Die CLI-Golden-Tests (`tests/golden/out-cli`, `tests/golden/out-basic-site`) laufen im Job und sind byte-gleich mit den Ergebnissen auf macOS und Windows (Normalisierung nach Abschnitt 0).
  - [ ] Ein Release-Build von `openspider` (Binärdatei als Job-Artefakt) crawlt `out-basic-site` über `os-testsite` headless und liefert für `--export-tabs "Internal:All"` den Golden-Export.
  - [ ] Ohne D-Bus-Sitzung und ohne Secret Service starten CLI und MCP-STDIO; Befehle ohne Geheimnisse laufen uneingeschränkt, Befehle mit Geheimnis melden einen klaren Fehler statt zu hängen.
  - [ ] JS-Rendering (CRAWL-078 ff.) läuft im Job headless ohne Display und ohne virtuellen Framebuffer mit dem über `OPENSPIDER_CHROMIUM` angegebenen Chromium; nachgewiesen durch die Rendering-Tests, die im Job unverändert mitlaufen.
  - [ ] Unter Linux übersprungene Tests stehen mit Grund in einer Liste im Repo; Ziel der Liste ist leer.
- **Testfixture:** CI-Job `linux-core`; `out-basic-site`.

### OUT-274 Docker-Image für den Serverbetrieb (x86_64)
- **Priorität:** P1
- **Beschreibung:** **(OS-Festlegung,** Nutzerentscheidung 04.11.2026, `docs/plan/beta3.md` 4b) Ein Container-Image macht den Linux-Serverbetrieb aus OUT-273 ohne Installation nutzbar: CLI und Exporte, geplante Läufe über einen Planer des Hosts (z. B. cron) und der MCP-Server über STDIO (`docker run -i … mcp`, Freigaben nach AGENT-003). Das Image enthält **keine Weboberfläche**; Oberfläche bleibt die Desktop-App, die die im Container erzeugten Crawl-Dateien öffnet. Inhalt: die CLI aus dem Linux-Release-Archiv `openspider_<version>_linux-x86_64.tar.gz` (unverändert, mit `LICENSE`, `NOTICE`, `THIRD_PARTY_NOTICES.txt`), Chromium der Basisdistribution für JS-Rendering über `OPENSPIDER_CHROMIUM`, Schriften und CA-Zertifikate. Quelle: `packaging/docker/Dockerfile`, Workflow `.github/workflows/docker.yml`.
- **Eingaben/Konfiguration:** Build-Argumente `OPENSPIDER_VERSION` und `OPENSPIDER_SHA256` (ohne Standardwert), optional `OPENSPIDER_ARCHIVE_URL` (nur HTTPS) bzw. ein lokales Archiv als Build-Kontext `archive`. Zur Laufzeit: Arbeitsordner `/data` (Standard-Ausgabeordner), `OPENSPIDER_HOME=/config` als Volume (Crawl-Verwaltung, Einstellungen, Logs; Voraussetzung für den Vergleich mit dem vorherigen Crawl), `OPENSPIDER_SECRET_STORE=memory`; Geheimnisse nur über Umgebungsvariablen zur Laufzeit (OUT-201, OUT-187). `OPENSPIDER_CHROMIUM_SANDBOX=1` startet Chromium mit Sandbox, wenn das Sicherheitsprofil des Containers sie zulässt (Standard ohne, weil das Standardprofil von Docker sie verhindert).
- **Ausgabe:** Image mit Einstiegspunkt `openspider` (Standardbefehl `--help`), OCI-Labels für Quelle, Lizenz `Apache-2.0`, Version, Revision und Beschreibung; Tags `<version>` und Kanal (`beta` bzw. `latest`).
- **Akzeptanzkriterien:**
  - [ ] Das Image läuft als eigener Benutzer mit fester UID/GID 10001 (nicht root); `docker run --user <uid>:<gid>` mit gemountetem `/config` funktioniert ebenso.
  - [ ] Die SHA-256-Prüfsumme des Archivs wird vor dem Entpacken geprüft; falsche oder fehlende Prüfsumme, unerwarteter Archivinhalt oder eine andere Version in `openspider --version` brechen den Build ab.
  - [ ] Basis-Image mit Tag und Digest gepinnt, kein `latest`; glibc der Basis mindestens die des Linux-Release-Builds (OUT-273).
  - [ ] Kein Geheimnis im Image (keine Schlüssel in `ENV`, `ARG` oder Dateien); ohne D-Bus und Schlüsselbund melden Befehle mit fehlendem Geheimnis einen klaren Fehler statt zu hängen (wie OUT-273).
  - [ ] Smoke-Test im Workflow gegen `os-testsite`: `--version`, Crawl mit CSV-Export (Zeilenzahl wie Golden `out-basic-site`), zwei Projektcrawls mit `--compare-with-previous` und `--save-report "Crawl Changes"` (Golden `out-b3-diff`), JS-Rendering mit dem Chromium des Images (gerenderter Titel), MCP über STDIO (`initialize` → `tools/list`, ohne Schalter nur lesende Werkzeuge, mit `--allow read,crawl` zusätzlich `start_crawl`).
  - [ ] SBOM (SPDX) und Provenance werden mit dem Image erzeugt und geprüft; veröffentlicht wird genau das geprüfte Image.
  - [ ] Kein Push in eine Registry ohne ausdrückliche Freigabe: nur per manuellem Workflow-Start mit `publish=true` aus einem veröffentlichten Release und nach Freigabe im Environment `ghcr`; kein automatischer Push bei Tags; eine vorhandene Version wird nicht überschrieben.
  - [ ] Lizenzhinweise im Image: `/usr/share/doc/openspider/` (aus dem Release-Archiv) und die Copyright-Dateien der Systempakete.
- **Testfixture:** Vertragstest `packaging/docker/docker.test.mjs`; Smoke-Test `packaging/docker/smoke.mjs` im Workflow `docker.yml` mit `out-basic-site`, `out-b3-diff` (v1, v2) und `crawl-m2-render`.

---

## 14 Benachrichtigungen & Automatisierungs-Schnittstellen

### OUT-280 E-Mail-Konten
- **Priorität:** P1
- **Beschreibung:** Konten für den Versand von Benachrichtigungen: SMTP (Host, Port, STARTTLS/TLS, Benutzer, Kennwort im Schlüsselbund), Gmail (OAuth, Versand-Berechtigung) und Microsoft 365/Outlook (OAuth). Testmail-Schaltfläche.
- **Eingaben/Konfiguration:** Einstellungen → Benachrichtigungen.
- **Akzeptanzkriterien:**
  - [ ] Testmail an Test-SMTP (`os-testsite`) kommt an; falsches Kennwort → verständliche Fehlermeldung mit SMTP-Antwortcode.
  - [ ] Unverschlüsseltes SMTP nur nach expliziter Bestätigung.
- **Testfixture:** Test-SMTP-Server.

### OUT-281 E-Mail bei Crawl-Ende (global und je Aufgabe)
- **Priorität:** P1
- **Beschreibung:** Global „E-Mail bei Crawl-Ende“ an festgelegte Empfänger für jeden Crawl; für geplante Aufgaben Unterreiter „Benachrichtigungen“ im Reiter Exporte des Aufgabendialogs (OUT-160) mit aufgabenspezifischen Empfängern. Optional auch bei Fehlschlag (Standard an) **(OS-Festlegung)**.
- **Akzeptanzkriterien:**
  - [ ] Crawl in der UI mit globaler Option → eine Mail nach Ende.
  - [ ] Fehlgeschlagener geplanter Lauf → Mail mit Betreff-Präfix `[FAILED]` und Fehlermeldung.
  - [ ] Entfernen eines Empfängers macht die Aufgabe nicht ungültig (Referenzfehler nicht übernehmen); ohne Empfänger wird keine Mail gesendet.
- **Testfixture:** Test-SMTP-Server.

### OUT-282 Inhalt der Abschlussmail
- **Priorität:** P1
- **Beschreibung:** Die Mail (HTML + Klartext-Alternative) enthält: Aufgabe/Projekt, Start-URL, Start/Ende/Dauer, gecrawlte URLs, Verteilung der Antwortcodes, indexierbar/nicht indexierbar, eine Tabelle aller Issues (Name, Typ, Priorität, URLs). Ist Auto-Compare aktiv (OUT-152), zusätzlich eine Änderungstabelle (übernommen aus TECH-308): Spalten `Issue / Filter`, `Previous`, `Current`, `Change` (absolut), `Change %` (Previous = 0 → leer), sortiert nach Betrag von `Change` absteigend; dazu je eine Zeile für neue und fehlende URLs (New/Missing, OUT-142) und je Change-Detection-Element (TECH-300 ff.) eine Zeile mit der Anzahl geänderter URLs; starke Veränderungen (z. B. Anstieg nicht indexierbarer Seiten, verschwundene Titles, neue/fehlende Seiten) werden hervorgehoben. Ohne vorherigen Crawl enthält die Mail statt der Änderungstabelle den Hinweis „kein vorheriger Crawl“.
- **Eingaben/Konfiguration:** Schwelle „starke Veränderung“: Änderung ≥ 10 % und ≥ 10 URLs **(OS-Festlegung)**.
- **Akzeptanzkriterien:**
  - [ ] Zahlen in der Mail stimmen mit dem Crawl Overview überein.
  - [ ] Mit Vergleich: Fixture `out-compare` → Mail nennt 3 neue und 2 fehlende Seiten.
  - [ ] Die Zahlen der Änderungstabelle stimmen mit dem manuell geöffneten Vergleich derselben zwei Crawls überein (Golden-Vergleich) (übernommen aus TECH-308).
  - [ ] Zeilen mit `Change` = 0 werden weggelassen, wenn „Leere Exporte überspringen“ (OUT-012) aktiv ist, sonst angezeigt **(OS-Festlegung,** übernommen aus TECH-308).
  - [ ] Fixture mit Seite `/i`, die im zweiten Lauf `noindex` erhält, und geändertem Title auf `/a` → Änderungstabelle zeigt `Indexability` +1 und `Page Titles` +1 (übernommen aus TECH-308).
  - [ ] Mail enthält keine Geheimnisse und keine vollständigen Exporte im Text (nur als Anhang, OUT-283).
- **Testfixture:** Test-SMTP-Server, Golden-Text der Klartext-Variante.

### OUT-283 Exporte als E-Mail-Anhang
- **Priorität:** P2
- **Beschreibung:** Gewählte Exporte werden gezippt an die Abschlussmail angehängt; „Leere Exporte überspringen“ (OUT-012) gilt auch für Anhänge.
- **Eingaben/Konfiguration:** Auswahl der anzuhängenden Exporte (aus der Exportauswahl der Aufgabe); max. Anhanggröße (Standard 20 MB).
- **Akzeptanzkriterien:**
  - [ ] ZIP größer als Limit → Mail ohne Anhang mit Warnhinweis und Pfad/Link zum Exportordner.
  - [ ] ZIP-Dateinamen entsprechen OUT-008.
- **Testfixture:** Test-SMTP-Server.

### OUT-284 Desktop-Benachrichtigung
- **Priorität:** P2
- **Beschreibung:** Systembenachrichtigung (macOS Notification Center, Windows-Info-Center/Toast) bei Crawl-Ende, Crawl-Fehler/-Abbruch, Abschluss der Crawl-Analyse und Abschluss eines Exports, der länger als 10 s gedauert hat **(OS-Festlegung** der Schwelle), sofern die App nicht im Vordergrund ist.
- **Eingaben/Konfiguration:** Einstellungen → Benachrichtigungen → Desktop: Hauptschalter (Standard an) und je Ereignis ein Schalter.
- **Ausgabe:** Titel `OpenSpider`; Text z. B. „Crawl abgeschlossen: site.test – 1 234 URLs, 12 Fehler (4xx/5xx)“, „Crawl fehlgeschlagen: <Grund>“, „Export fertig: internal_all.csv“.
- **Akzeptanzkriterien:**
  - [ ] App im Hintergrund, Crawl endet → genau eine Benachrichtigung mit Host, Anzahl gecrawlter URLs und Anzahl 4xx+5xx (Zahlen = Crawl Overview).
  - [ ] App im Vordergrund → keine Systembenachrichtigung (nur In-App-Hinweis).
  - [ ] Export von 8 s Dauer → keine Benachrichtigung; von 12 s → eine.
  - [ ] Klick auf die Benachrichtigung bringt die App mit dem betreffenden Crawl (bzw. bei Export mit geöffnetem Zielordner-Hinweis) in den Vordergrund.
  - [ ] Hauptschalter aus → keine Benachrichtigungen; einzelnes Ereignis aus → nur dieses unterdrückt.
  - [ ] Hat der Nutzer Benachrichtigungen auf OS-Ebene verweigert (macOS-Berechtigung), zeigt die Einstellungsseite einen Hinweis mit Link zu den Systemeinstellungen; kein Fehler, kein erneutes Nachfragen bei jedem Crawl.
  - [ ] CLI- und geplante Läufe (OUT-163) erzeugen keine Desktop-Benachrichtigungen (dafür OUT-281).
- **Testfixture:** Benachrichtigungs-Adapter mit Mock-Backend (Unit-Test der Auslöselogik inkl. Vordergrund-Status und Dauer-Schwelle); manueller Test je OS.

### OUT-285 MCP-Server (Steuerung durch KI-Assistenten)
- **Priorität:** P1
- **Beschreibung:** Optionaler, lokal laufender MCP-Server, über den KI-Assistenten Crawls starten, Fortschritt abfragen (inkl. Crawl-ID), Daten abfragen und Exporte/Reports auslösen können – technisch eine Hülle um dieselben Funktionen wie das CLI (Abschnitt 8). Transporte: STDIO (headless) und Streamable HTTP ausschließlich auf Loopback (`127.0.0.1` und `::1`, Port konfigurierbar). Der HTTP-Transport ist nur zusammen mit dem Zugriffsschutz aus AGENT-002 (Bearer-Token + `Origin`-/`Host`-Prüfung) aktivierbar; ohne ihn wäre er für jeden lokalen Prozess und per DNS-Rebinding aus dem Browser erreichbar (OS-Festlegung, AGENT-002 ist Voraussetzung des HTTP-Teils). Standardmäßig deaktiviert. Führender Eintrag (Hauptneuerung einer jüngeren Referenzversion, daher P1; der STDIO-Kern mit lesenden Werkzeugen ist als AGENT-001 mit P0 vorgezogen, Erweiterungen in 10-agent-interface.md); UI-129 beschreibt nur Menü, Dialoge und Statusanzeige.
- **Eingaben/Konfiguration:** Transport STDIO/Streamable HTTP; Port (Vorschlag zufällig aus 49152–65535); „Beim App-Start automatisch starten“ (Standard aus); Werkzeug-Freigaben je Werkzeug an/aus (übernommen aus UI-129); Standardfreigabe nach Werkzeugklassen gemäß AGENT-003 (führend): nur lesende Werkzeuge an, crawlstartende, schreibende und kostenpflichtige erst nach ausdrücklicher Freigabe (OS-Festlegung, bewusste Abweichung zugunsten der Sicherheit).
- **Ausgabe:** Werkzeugliste über `tools/list`; herunterladbare API-Beschreibung `openspider-mcp-tools.md` (Markdown, je Werkzeug Überschrift, Beschreibung, Parametertabelle) (übernommen aus UI-129).
  - **Serverstatus (führend; Anzeige im Statusleistenfeld `MCP` nach UI-013, übernommen aus UI-129):** genau einer der Texte `MCP: aus` (HTTP-Server der App gestoppt oder Start gescheitert), `MCP: bereit (Port <p>)` (lauscht, keine aktive Sitzung) oder `MCP: verbunden (<n> Clients)` (n ≥ 1, Singular `1 Client`). n zählt die aktiven Sitzungen des Streamable-HTTP-Transports (Sitzungs-ID nach erfolgreichem `initialize`); eine Sitzung endet mit HTTP `DELETE`, beim Verbindungsabbruch des Clients oder nach 10 Minuten ohne Anfrage **(OS-Festlegung)**. STDIO-Server, die ein Client selbst startet (`openspider mcp`, AGENT-001), laufen als eigener Prozess und zählen nicht. Klick auf das Feld öffnet das Menü „MCP“ (UI-129).
- **Akzeptanzkriterien:**
  - [ ] HTTP-Transport bindet ausschließlich an Loopback (`127.0.0.1`, `::1`); Zugriff von anderen Hosts ist nicht möglich.
  - [ ] HTTP-Transport ist nur mit dem Schutz aus AGENT-002 startbar: Anfragen ohne gültigen Token → 401, mit fremdem `Origin`/`Host` → 403; es gibt keine Einstellung, die Token oder Origin-Prüfung abschaltet.
  - [ ] Exporte über MCP verwenden dieselben kanonischen Namen und erzeugen dieselben Dateien wie das CLI.
  - [ ] MCP-Logausgaben gehen nie auf stdout im STDIO-Modus (würde das Protokoll stören).
  - [ ] Große Ergebnisse werden seitenweise bzw. als Datei geliefert statt in einer übergroßen Antwort.
  - [ ] Die Werkzeugliste im UI (UI-129) und die Markdown-Beschreibung stimmen exakt mit der Antwort auf `tools/list` überein (Name und Parameterschema; Anzahl Überschriften = Anzahl Werkzeuge) (übernommen aus UI-129).
  - [ ] Abgeschaltete Werkzeuge erscheinen nicht in `tools/list` und werden bei direktem Aufruf mit einem MCP-Fehler abgelehnt (übernommen aus UI-129).
  - [ ] Mit Auto-Start ist der Server ≤ 2 s nach Erscheinen des Hauptfensters erreichbar; ist der Port belegt, erhält der Nutzer eine Benachrichtigung statt eines stillen Fehlschlags (übernommen aus UI-129).
  - [ ] Statusanzeige live: Jeder Zustandswechsel erscheint ≤ 1 s danach im Feld `MCP` der Statusleiste. Ablauf im Test: Server aus → `MCP: aus`; Start auf Port 51234 → `MCP: bereit (Port 51234)`; erster Client sendet `initialize` → `MCP: verbunden (1 Client)`; zweiter Client → `MCP: verbunden (2 Clients)`; ein Client sendet `DELETE` → `MCP: verbunden (1 Client)`; zweiter Client bricht die Verbindung ab → `MCP: bereit (Port 51234)`; Server stoppen → `MCP: aus`. Bei belegtem Port bleibt die Anzeige `MCP: aus`. Anfragen mit ungültigem Token (401) oder fremdem `Origin` (403) ändern den Zähler nicht.
  - [ ] Ein Crawl-Start über MCP erzeugt denselben Crawl (URL-Menge, Statuscodes) wie der gleichwertige CLI-Aufruf und erscheint in der Crawl-Verwaltung (OUT-228).
- **Testfixture:** MCP-Client-Test (STDIO und HTTP) gegen `out-basic-site`: `tools/list` vs. UI-Liste und Markdown, Auto-Start nach Neustart, belegter Port, abgeschaltetes Werkzeug; UI-E2E-Test der Statusanzeige mit zwei HTTP-Test-Clients und Testuhr (10-Minuten-Ablauf).

---

## Anhang A – Fixture-Übersicht dieses Bereichs

| Fixture | Zweck | Genutzt von |
|---|---|---|
| `out-basic-site` | kleine Website mit allen Inhaltstypen und je einem Fehlerfall | Engine, Formate, CLI, Multi-Export |
| `out-csv-edge` | Sonderzeichen, Zeilenumbrüche, Formeln, Kodierungsfehler | OUT-002, -003, -016 |
| `out-list-order` | Listen mit Duplikaten, ungültigen Zeilen, Schema-losen URLs | OUT-015, -085, -183 |
| `out-segments` | Pfad-Segmente Blog/Shop/Info | OUT-011, -113, -130, -213 |
| `out-detail-tabs` | Inlinks/Canonical, JSON-LD, Rechtschreibung/Grammatik, Nah-Duplikat, Accessibility, Konsole, N-grams, Header | OUT-021 – -027, -052 |
| `out-site-structure` | Tiefenkette bis 11, 404 in Tiefe 1 | OUT-025, -080 |
| `out-crawl-path` | lineare Tiefe, Sitemap-Orphan | OUT-023 |
| `out-queue` | Crawl-Limit mit Restwarteschlange | OUT-030 |
| `out-links`, `out-links-anchor`, `out-links-follow`, `out-uncrawlable-links` | Linktypen, Anker, follow/nofollow, nicht crawlbare Links, nur von nicht indexierbaren Seiten verlinkte Ziele | OUT-031 – -042, -068 |
| `out-render`, `out-pdf`, `out-headers` | Rendering, PDFs, Header, Cookies | OUT-043 – -049, -107, -110, -111 |
| `out-response-codes`, `out-security` | alle Statuscodes inkl. HTTP-Refresh-Header, TLS, Mixed Content, externe HTTP-Ziele | OUT-050, -051, -067, -080, -097, -270 |
| `out-content-dupes`, `out-images`, `out-canonicals`, `out-directives`, `out-structured-data`, `out-sitemaps-in`, `out-amp`, `out-custom-search`, `out-custom-extraction`, `out-a11y`, `out-issues` | filterbasierte Bulk-Exporte | OUT-052 – -066 |
| `out-redirects`, `out-pagination`, `out-hreflang`, `out-serp`, `out-mobile` | Reports (inkl. HSTS-Host, `Refresh`-Header, gemischte Ketten) | OUT-081 – -109 |
| `out-sitemap-gen`, `out-sitemap-images` | Sitemap-Generator | OUT-120 – -133 |
| `out-compare`, `out-compare-mapping`, `out-compare-ua` | Crawl-Vergleich inkl. Linkwechsel `/start`, Ankertextwechsel `/c`, Auto-Compare-Läufe | OUT-140 – -153, -282 |
| `onpage-rc/` (Fachspec 02) | Antwortcode-Fixture für Fehler-Inlinks und Redirects to Errors | OUT-067, -084 |
| `out-schedule` | Sitemap-Index, Formular-/Basic-Auth, Golden-plist/Task-XML | OUT-162, -166, -187, -209, -223 |
| `out-markdown` (Nachtrag 09.11.2026) | HTML-Seiten mit allen Markdown-Fällen und erwarteten `.md`-Dateien | OUT-289 |
| Mocks im `os-testsite` | Google Sheets/Drive, GA4/GSC, PSI, KI (Text/Bild/Audio/Embeddings), SMTP, Update-Server, Proxy | OUT-005, -021, -025, -064, -065, -099, -104 – -106, -114, -201, -210 – -214, -250, -269, -280 |

## Anhang B – Offene Punkte / Annahmen

- Die vollständigen Spaltenlisten der Haupt-Tabs und die exakte Filterliste je Tab stammen aus den anderen Spec-Bereichen; dieses Dokument verweist über die Filter-Registry darauf (Registry-Tests in OUT-020, -054, -055, -066, -181, -211).
- Spaltennamen einzelner Bulk-Exporte und Reports sind dort, wo die öffentliche Doku sie nicht nennt, als **(OS-Festlegung)** gekennzeichnet; Paritätsabgleich erfolgt über manuell erzeugte Golden-Exporte der Referenz (Free-Version, ≤ 500 URLs, siehe `docs/ARCHITECTURE.md`).
- Die Referenz nennt für Google Sheets ein Limit von 5 Mio. Zellen; Google erlaubt inzwischen 10 Mio. – OUT-005 macht das Limit konfigurierbar.
- Die Referenz teilt Sitemaps ab 49 999 URLs; das Protokoll erlaubt 50 000. OpenSpider übernimmt 49 999 als Standard (OUT-129).
- Revision nach Spec-Review (2026-10-13, Score 86, „needs_work“) – Prüfstand gegen öffentliche Quellen:
  - Belegt in der Referenz-Doku bzw. den Release-Notes: Response-Code-Filter `Redirection (HTTP Refresh)` und Bulk-Export `All Error Inlinks` (OUT-050/-067/-080); Change-Detection-Filterliste mit 14 Elementen beginnend mit `Indexability` und Content-Schwelle „mehr als 10 %“ (OUT-144/-145); Site-Structure-Tiefen 1 bis 10+ nach Antwortcode (OUT-025); Spelling-&-Grammar-Panel mit den 100 häufigsten Fehlern und URL-Anzahl (OUT-025); CLI-Syntax `--project-crawl-comparison "true"` (OUT-152/-202); Looker-Studio-Export mit Datums- und URL-Feld, frei wählbarer Metrikreihenfolge und separat verlinkter Datentyp-Tabelle (OUT-210/-214); HSTS-Policy als interne 307 (OUT-081); Insecure-Content-Report umfasst HTTP-Links, Canonicals, Paginierung und Mixed Content ohne Einschränkung auf interne Ziele (OUT-097); Arm64-Linux-Pakete seit einer früheren Referenzversion (OUT-264).
  - Entfernte, nicht belegte Versionsangaben: FAQ-Rich-Result-Entfernung (OUT-102), CLI-Namensliste (OUT-203), RDP-Absturz als Referenzproblem (OUT-262; laut Review in einer früheren Version behoben, nun eigenes Kriterium). Die Release-History führt die jüngeren Zwischenversionen nur als Bugfix-Versionen.
  - Noch per Golden-Export zu bestätigen: getrennte Bulk-Exporte `Content:Spelling Errors`/`Content:Grammar Errors` (OUT-052, OpenSpider registriert vorsorglich beide Varianten plus den kombinierten Export); exakte Werteliste `Chain Type` inkl. `HTTP Refresh` (OUT-081); Verfügbarkeit von `All Error Inlinks` im Bereich „Internal & External“ (OUT-067); Spaltennamen der Detail-Exporte Lighthouse/Accessibility/N-grams (OUT-021).
  - Clean-Room-Hinweis: `Description`/`How To Fix`/`Why It Matters` (OUT-025, OUT-063) und die Kurzbeschreibungen in `Grammar Errors:Detail` (OUT-052) werden eigenständig verfasst; Issue-Texte der Referenz dürfen weder kopiert noch als Vorlage für Umformulierungen genutzt werden.
- Revision Spec v1 (2026-10-13, Entscheidungen des Architekten zu PARITY.md):
  - Verweis-Einträge (eine Quelle je Funktion): OUT-144 → TECH-300; OUT-145 → TECH-305; OUT-231 → CRAWL-124; OUT-269 → CRAWL-122; OUT-271 → UI-121. Ihre bisherigen Detailkriterien gehören in die führenden Einträge der jeweiligen Bereiche.
  - Hier führend und um Kriterien anderer Bereiche ergänzt: OUT-020 (ONPAGE-191, UI-015), OUT-011/-130 (CRAWL-134, UI-068), OUT-067/-084 (ONPAGE-067), OUT-140 … -153 (CRAWL-032/034/035/036/144, TECH-305/306/307/308), OUT-160 … -171 (CRAWL-136), OUT-180 … -209 (CRAWL-135, CRAWL-036, TECH-308), OUT-224 (CRAWL-133, UI-116), OUT-228/-229 (CRAWL-128/143, UI-124), OUT-245 (UI-128), OUT-250/-251 (CRAWL-142), OUT-282 (TECH-308), OUT-285 (UI-129).
  - Prioritäten: OUT-149 P2 → P1; OUT-285 P2 → P1; OUT-021 eine Priorität (P0, übrige Detail-Tabs [P1]); OUT-263 P1 (Emulation) mit [P2]-Phase nativ; Teilumfänge in OUT-002, -126, -161 … -166, -183, -188, -198, -206, -261, -272 als [P1]/[P2] gekennzeichnet.
  - Lücken geschlossen: G-1 (OUT-068), G-4 (OUT-211/-212), G-5 (OUT-203, `--list-exports`/`--list-reports`). Gegen die öffentliche Referenz-Doku geprüft: CLI-Syntax `--crawl-comparison <id> <id>`, `--project-crawl-comparison "true"`, `--list-crawls` und die `--help <kategorie>`-Namenslisten; das Verhalten bei ungültigen Namen ist dort nicht dokumentiert (OUT-203 bleibt OS-Festlegung).
- Nacharbeit nach der Fable-Gegenprüfung (2026-10-13, Entscheidungen des Architekten):
  - R-02: URL-Mapping (OUT-146) nutzt den gemeinsamen Regex-Dialekt aus 01 (fancy-regex, Lookaround, Suchsemantik); die Einschränkung auf die Crate `regex` entfällt.
  - R-03: OUT-145 ist Verweis auf TECH-305 (P1); Filtername und Toleranzen gelten nur noch dort.
  - R-08: K-5/K-6 sind Einzelprioritäten. OUT-164, -170, -171 → P1 (maximale Laufzeit in OUT-171 [P2]); OUT-184, -185, -191 → P0; OUT-187 bleibt als begründete Ausnahme P1 (setzt OUT-223 voraus).
  - R-11: OUT-201 → P1 mit KI-/Embedding-Flags und `--require-apis` (übernommen aus INTEG-017).
  - R-13: OUT-104 mit Zählbeispiel aus INTEG-145 und kanonischem Spaltenschlüssel `Audit`; neuer Ressourcen-Export je Audit OUT-114 (aus INTEG-144).
  - R-17: Kanonische Spaltenschlüssel der Detail- und Bulk-Exporte aus den führenden Einträgen: Konsole `Source URL` (TECH-114) in OUT-021/-058, N-grams `N-Gram`, `Occurrences`, `Pages`, `Linked`, `Unlinked` (UI-049) in OUT-021 (dort jetzt [P2] wie UI-049).
  - R-19: Dateinamensvorschlag `<tab>_<filter>_<YYYY-MM-DD>` des UI-Exportdialogs in OUT-008/OUT-020 (aus UI-015); CLI und Scheduling bleiben ohne Datum.
- Nacharbeit reviews-open-v1, Runde 3 (Architekten-/Planerentscheidungen):
  - Inhaltsübersicht: Bereichsgrenzen OUT-114 und OUT-133 korrigiert (R-18).
  - OUT-146: Kollision – erste passende Regel in Nutzerreihenfolge gewinnt; Gleichstände als OS-Festlegung.
  - OUT-149: Link-Identität (Quelle, Ziel, Linktyp); neue Änderungsart „Ankertext geändert“ (`Anchor Changed`).
  - OUT-020 führend für den Dateinamensvorschlag (Startdatum des Crawls, lokale Zeitzone); OUT-008 verweist hierher.
  - Spaltennamen nach dem führenden Fach-Eintrag: Cookies (UI-043) in OUT-021/-049/-111 inkl. `First-Party`; Accessibility (TECH-259) in OUT-021/-062/-109; Spelling & Grammar (ONPAGE-157/158) in OUT-021/-025/-052; Semantik (INTEG-181/182/185) in OUT-025/-052; Images (ONPAGE-181) in OUT-053; `Found Via` (TECH-214) in OUT-057.
  - OUT-052 um „Spelling & Grammar Errors Summary“, „Soft 404 Inlinks“ und die Partnerzeilen von „Semantically Similar“ ergänzt; OUT-062 um den Export `Incomplete`; OUT-083/-086 legen die `Chain Type`-Werte für Canonical-Ketten fest; OUT-100 übernimmt die TECH-190-Kriterien; OUT-094 den Alias aus TECH-088; OUT-108 Element-Zeilen; OUT-099 Quellwert `GA`.
  - Neu: OUT-069 Vektor-Export „AI:Embeddings“ (P2).
  - OUT-223 führend für Auth-Profile (`.osauth`, XChaCha20-Poly1305, P1), CRAWL-121 verweist hierher.
  - OUT-285: Standardfreigabe der MCP-Werkzeuge nach AGENT-003 (nur lesend); STDIO-Kern als AGENT-001 (P0) vorgezogen.
  - Stubs OUT-144, OUT-231, OUT-271 auf das strikte Format normiert.
- Nachtrag Abgleich Cloud-Variante, Etappe E0 (16.10.2026, Planer; `docs/plan/autonomo-abgleich.md` 5.2/5.4): neu OUT-233 (Manifest der Crawl-Datei), OUT-234 (stabile Leseschnittstelle, `q_`-Sichten, führend für die Grundsichten von AI-100) und OUT-273 (Linux-Serverbetrieb von Kern und CLI, P1). OUT-264 behält aarch64, Pakete und Desktop (P2). Befund-Zuordnungstabelle, gemeinsame Datendateien (Bot-Katalog, Lastprofile) und ein Secret-Store für Server sind noch nicht spezifiziert.
- Nachtrag B3-DOCKER (05.11.2026, Nutzerentscheidung 04.11.2026, `docs/plan/beta3.md` 4b): neu OUT-274 (Docker-Image für den Serverbetrieb, P1) im Abschnitt 13. Ein Konfigurationsschlüssel für die Chromium-Sandbox (`LaunchOpts.no_sandbox` in `os-render`) ist nicht spezifiziert; das Image setzt `--no-sandbox` über seinen Chromium-Starter.

---

## 15 Nachtrag 09.11.2026: Funktionen für 1.x (nach 1.0)

Die folgenden Einträge sind **OpenSpider-Festlegungen (OS-Festlegung, Nutzerentscheidung 09.11.2026)** ohne Paritätsbezug. Sie gehören unabhängig von ihrer Priorität zum Meilenstein **1.x (nach 1.0)** (Zuordnung `README.md`). Neue CLI-Optionen sind OS-Erweiterungen im Sinn von OUT-181 (in `--help` markiert); Exit-Codes gelten nach OUT-204.

### OUT-286 Geplante und per CLI gestartete Läufe pausieren und fortsetzen
- **Priorität:** P1
- **Meilenstein:** 1.x (nach 1.0)
- **Beschreibung:** Ein Crawl, den ein anderer Prozess führt – ein geplanter Lauf (`--run-task`, OUT-209), ein manueller CLI-Lauf oder ein MCP-Server über STDIO (`owner_kind` = `cli` bzw. `mcp-stdio`, AGENT-020) –, lässt sich aus der App pausieren und fortsetzen, sofern der Besitzerprozess erreichbar ist. Bisher war das ausgeschlossen (OUT-171, CRAWL-136, AGENT-020). Der Besitzerprozess bleibt dabei Besitzer: Er hält an, wartet und setzt auf Anforderung fort, sodass Exporte, Benachrichtigungen und Laufhistorie eines geplanten Laufs nach dem Fortsetzen wie gewohnt entstehen.
- **Steuerkanal (OS-Festlegung):**
  - Die App schreibt eine Steueranforderung als Datei `<crawl-id>.control` (JSON: `request` ∈ {`pause`, `resume`}, `request_id`, `valid_until` = Zeitpunkt + 10 s) in den Ordner der Crawl-Datei, atomar (temporäre Datei, dann Umbenennen). Kein Netzwerk, kein Shell-Aufruf; Zugriff nur für denselben Betriebssystem-Benutzer.
  - Der Besitzerprozess prüft die Datei mindestens einmal je Sekunde (zusammen mit dem Heartbeat, CRAWL-125), führt eine gültige Anforderung aus, quittiert sie in den Crawl-Metadaten (`control_ack` = `request_id`) und löscht die Datei. Abgelaufene Anforderungen werden verworfen, nicht nachträglich ausgeführt.
  - **Erreichbar** ist ein Besitzerprozess, wenn sein Heartbeat höchstens 5 s alt ist und er in den Crawl-Metadaten `control = 1` meldet (Fähigkeit; ältere Versionen melden nichts und bleiben wie bisher nur lesend).
- **Verhalten:**
  - **Pausieren:** Der Besitzerprozess verhält sich wie bei CRAWL-126 (keine neue Anfrage nach 1 s, Warteschlange dauerhaft gespeichert), beendet sich aber nicht, sondern wartet. `owner_pid` bleibt gesetzt und der Heartbeat läuft weiter (Abweichung von der Regel „`owner_pid` leer bei Pause“ in AGENT-020, nur für diesen Fall). Die Crawl-Verwaltung zeigt `Paused` mit dem Zusatz „wartet auf Fortsetzen (<cli|mcp-stdio>, PID <n>)“.
  - **Fortsetzen:** Der Besitzerprozess crawlt weiter; danach laufen Analyse, Exporte, Benachrichtigungen und Historie wie bei einem ununterbrochenen Lauf. Die Laufzeit-Konfiguration ändert sich nicht (Konfigurationsänderungen an fremden Crawls bleiben ausgeschlossen).
  - **Höchstdauer der Pause:** Option „Pausierte Läufe beenden nach (h)“ je Aufgabe (OUT-161) bzw. `--max-pause-h <n>` für CLI-Läufe, Standard 24 h, 0 = unbegrenzt. Danach beendet sich der Besitzerprozess wie nach dem ersten SIGINT (OUT-206): Crawl bleibt `Paused`, Exit-Code 130, keine Exporte; Historie `Aborted` mit „Pause-Höchstdauer erreicht“.
  - **Besitzer endet während der Pause** (Abbrechen nach OUT-171, Signal, Neustart des Rechners): Der Crawl bleibt `Paused` und ist wie jeder pausierte Crawl in der App oder per `--load-crawl <id> --resume` fortsetzbar (CRAWL-126); die Exporte der Aufgabe entstehen dann nicht (Hinweis beim Fortsetzen in der App).
  - **Geplante Läufe:** Die Aufgabenliste (OUT-160) bietet für einen laufenden Lauf „Pausieren“, „Fortsetzen“ und „Abbrechen“. Ein pausierter Lauf gilt für die Überlappungsregel (OUT-167) als aktiv; die Historie (OUT-169) zeigt während der Pause den Zustand `Paused`.
  - **Weiterhin nicht steuerbar** aus der App: Stoppen (für geplante Läufe bleibt „Abbrechen“), Threads, Konfiguration.
  - Das erste SIGINT eines CLI-Laufs pausiert und beendet weiterhin den Prozess (CRAWL-126, OUT-206); der Steuerkanal ändert daran nichts.
- **Ausgabe:** Schaltflächen „Pausieren“/„Fortsetzen“ für fremde Crawls in der Crawl-Verwaltung (OUT-228), in der Kopfleiste eines geöffneten fremden Crawls (UI-003) und in der Aufgabenliste; Tooltip bei nicht erreichbarem Besitzer „Besitzerprozess nicht erreichbar – Steuerung nicht möglich“. CLI-Lauf: Zeile auf stderr bei Pause und Fortsetzen; `--progress json` (OUT-205) meldet die Phase `paused`.
- **Akzeptanzkriterien:**
  - [ ] CLI-Crawl von `out-basic-site` (verlangsamt), App „Pausieren“: Innerhalb von 3 s startet keine neue Anfrage (Request-Log), die Verwaltung zeigt `Paused` mit Zusatz, der Prozess läuft weiter, `owner_pid` ist gesetzt.
  - [ ] „Fortsetzen“: Der Prozess crawlt zu Ende, erzeugt die angeforderten Exporte und endet mit Exit-Code 0; URL-Menge, Statuscodes und Tiefen sind identisch mit einem ununterbrochenen Lauf.
  - [ ] Geplanter Lauf: Historie zeigt während der Pause `Paused`, nach Fortsetzen und Ende `Completed`; Exporte und Abschlussmail (Mock) entstehen genau einmal.
  - [ ] Prozess wird während der Pause beendet (SIGKILL): Die Verwaltung zeigt nach Ablauf des Heartbeats `Paused` ohne Zusatz; „Fortsetzen“ in der App setzt den Crawl im App-Prozess fort.
  - [ ] Besitzer einer älteren Version (ohne `control = 1`) bzw. mit Heartbeat älter als 5 s: Schaltflächen deaktiviert mit Tooltip; es wird keine Steuerdatei geschrieben.
  - [ ] Eine abgelaufene Steuerdatei (`valid_until` überschritten) wird vom Besitzer gelöscht und nicht ausgeführt; die App meldet nach 3 s ohne Quittung „Keine Antwort vom Besitzerprozess“.
  - [ ] `--max-pause-h` (Testwert über simulierte Uhr): Nach Ablauf endet der Prozess mit Exit-Code 130, Crawl `Paused`, Historie `Aborted` mit Meldung.
  - [ ] Überlappung „Überspringen, wenn noch aktiv“ (OUT-167): Ein pausierter Lauf verhindert den nächsten Lauf derselben Aufgabe (Historie „Skipped: previous run still active“).
- **Testfixture:** `out-basic-site` mit großer Verzögerung (wie OUT-171); `out-schedule` für geplante Läufe mit SMTP-Mock; simulierte Uhr für Heartbeat und Höchstdauer.
- **Entscheidung (Nutzer 09.11.2026):** `pause_crawl`/`resume_crawl` (AGENT-020) dürfen über denselben Steuerkanal auch fremde Crawls steuern – für den In-App-HTTP-Server (er ist die App) und für STDIO-Server, beides nur mit freigegebener Werkzeugklasse `crawl`. Ohne diese Freigabe antworten die Werkzeuge bei fremden Crawls wie bisher mit Fehler.

### OUT-287 CLI-Verwaltung von Crawls und Projekten
- **Priorität:** P1
- **Meilenstein:** 1.x (nach 1.0)
- **Beschreibung:** Die Aktionen der Crawl-Verwaltung (OUT-228) zum Löschen, Umbenennen und Verschieben von Crawls sowie zum Anlegen, Umbenennen und Löschen von Projekten sind ohne Oberfläche per CLI verfügbar, z. B. für Aufräumskripte auf einem Server (OUT-273) oder nach geplanten Läufen. Es gelten dieselben Regeln wie in der App (gesperrte Crawls und Projekte, laufende Crawls, Vergleiche); beide Wege nutzen dieselbe Kernfunktion.
- **Eingaben/Konfiguration (OS-Erweiterung):**
  - `--delete-crawl <id>[,<id>…]` – löscht Crawls; ohne `--yes` nur interaktiv (TTY-Rückfrage mit Anzahl und freiwerdendem Speicher), ohne TTY und ohne `--yes` Exit-Code 2.
  - `--rename-crawl <id> <name>`
  - `--move-crawl <id>[,<id>…] --to-project <name>` – `--to-project ""` entfernt die Projektzuordnung; das Zielprojekt muss existieren.
  - `--create-project <name>`, `--rename-project <alt> <neu>`
  - `--delete-project <name> [--with-crawls]` – ein leeres Projekt wird gelöscht; ein Projekt mit Crawls nur mit `--with-crawls` (löscht die Crawls mit, Rückfrage bzw. `--yes` wie oben).
  - `--list-projects` – Name, Anzahl Crawls, Größe, gesperrt.
  - `--json` – Ergebnis als JSON auf stdout (Fortschritt und Meldungen auf stderr, OUT-205).
  - Die Befehle schließen sich gegenseitig und mit Crawl-Optionen (`--crawl`, `--crawl-list`, `--load-crawl`, `--run-task`) aus (Exit-Code 2).
- **Regeln (wie OUT-228, OS-Festlegung wo die App keine hat):**
  - Gesperrte Crawls sind nicht lösch- und nicht umbenennbar; gesperrte Projekte sind nicht lösch- und nicht umbenennbar.
  - Ein laufender Crawl (Status `Running` mit aktuellem Heartbeat) und ein pausierter Crawl mit wartendem Besitzer (OUT-286) sind nicht lösch- und nicht verschiebbar; Umbenennen ist erlaubt (betrifft nur die Metadaten).
  - Projektnamen sind eindeutig (Vergleich ohne Groß-/Kleinschreibung); `--create-project` und `--rename-project` auf einen vorhandenen Namen → abgelehnt.
  - Wird ein zu löschender Crawl in einem gespeicherten Vergleich verwendet, nennt die Rückfrage bzw. die Ausgabe den Vergleich namentlich (OUT-150); der Vergleich wird als „nicht verfügbar“ markiert.
  - **Alles oder nichts bei der Prüfung:** Bei mehreren IDs werden zuerst alle geprüft; ist eine abgelehnt, wird nichts geändert. Scheitert danach die Ausführung an einem Dateisystemfehler, bleiben bereits erledigte Änderungen bestehen und die Ausgabe nennt sie.
  - Ein geöffnetes App-Fenster zeigt die Änderungen innerhalb von 2 s in der Crawl-Verwaltung (gemeinsamer Index).
  - Die App-Verwaltung erhält dieselbe Aktion „Projekt löschen“ (bisher nicht vorhanden) mit denselben Regeln.
- **Ausgabe:** Ohne `--json` eine Zeile je Objekt auf stdout (`deleted <id> <name> (<größe>)`, `refused <id>: <grund>` …). Mit `--json` ein Objekt `{"action", "ok", "results": [{"id"|"project", "result": "done"|"refused"|"failed", "reason", "freed_bytes"?, "comparisons"?}]}`; Gründe als stabile Schlüssel: `not_found`, `locked`, `project_locked`, `running`, `owner_waiting`, `name_exists`, `project_not_found`, `project_not_empty`, `confirmation_required`, `io_error`.
- **Exit-Codes (nach OUT-204):** `0` alles ausgeführt; `2` Aufruf- oder Prüffehler, nichts geändert (unbekannte ID, gesperrt, laufend, Name vergeben, fehlende Bestätigung); `3` Dateisystemfehler während der Ausführung (Teilausführung möglich, siehe Ausgabe); `1` interner Fehler.
- **Akzeptanzkriterien:**
  - [ ] `--delete-crawl <id> --yes --json` löscht die Datei, gibt `freed_bytes` = Dateigröße aus und Exit-Code 0; `--list-crawls` enthält die ID danach nicht mehr.
  - [ ] `--delete-crawl <gesperrt> --yes` → Exit-Code 2, `reason = locked`, Datei unverändert (übernimmt das Kriterium aus OUT-228).
  - [ ] `--delete-crawl a,b --yes` mit gesperrtem `b` → Exit-Code 2, auch `a` bleibt erhalten.
  - [ ] Ohne `--yes` und ohne TTY → Exit-Code 2, `reason = confirmation_required`.
  - [ ] `--rename-crawl` und `--move-crawl` ändern die ID nicht; `--list-crawls --json` zeigt danach Name bzw. Projekt (wie OUT-228).
  - [ ] `--move-crawl` eines laufenden Crawls → Exit-Code 2, `reason = running`; `--rename-crawl` desselben Crawls gelingt.
  - [ ] `--delete-project leer` gelingt; `--delete-project voll` ohne `--with-crawls` → Exit-Code 2, `project_not_empty`; mit `--with-crawls --yes` werden Projekt und Crawls gelöscht, sofern keiner gesperrt oder laufend ist.
  - [ ] `--create-project Kunde` bei vorhandenem `kunde` → Exit-Code 2, `name_exists`.
  - [ ] Löschen eines in einem Vergleich verwendeten Crawls nennt den Vergleich in stdout bzw. in `comparisons`.
  - [ ] Schreibgeschützter Crawl-Ordner: `--delete-crawl` → Exit-Code 3, `reason = io_error`.
  - [ ] Bei geöffneter App erscheint die Änderung innerhalb von 2 s in der Crawl-Verwaltung (E2E-Test).
- **Testfixture:** Crawl-Verwaltung mit 3 Projekten (eines gesperrt, eines leer) und 6 Crawls (einer gesperrt, einer laufend mit simuliertem Heartbeat, einer in einem gespeicherten Vergleich); schreibgeschützter Ordner.

### OUT-288 Konfigurationsprofile auch als JSON
- **Priorität:** P1
- **Meilenstein:** 1.x (nach 1.0)
- **Beschreibung:** Konfigurationsprofile (OUT-220) lassen sich neben TOML (OUT-221) auch als JSON lesen und schreiben, mit demselben Schema (`docs/spec/schemas/osconfig.schema.json`) und denselben Pflichtfeldern `schema_version` und `generator`. Das erleichtert die Erzeugung und Prüfung von Profilen in Skripten, CI-Pipelines und Agenten, die JSON nativ verarbeiten. TOML bleibt das Standardformat (Standardkonfiguration `default.osconfig`, Einbettung in die Crawl-Datei OUT-230).
- **Eingaben/Konfiguration:**
  - Dateiendungen: `.osconfig` (TOML) und `.osconfig.json` (JSON). Beim Laden entscheidet der Inhalt, nicht die Endung: Beginnt die Datei nach optionalem BOM und Leerraum mit `{`, wird sie als JSON gelesen, sonst als TOML.
  - App: „Laden…“ akzeptiert beide; „Speichern unter…“ bietet beide Formate an (Standard TOML); „Exportieren als JSON“ im Menü Konfiguration → Profile.
  - CLI: `--config` (OUT-186) akzeptiert beide Formate; neu `--convert-config <eingabe> <ausgabe>` (OS-Erweiterung) – Format der Ausgabe nach Endung (`.json` → JSON, sonst TOML); kein Crawl, Exit-Code 0 bzw. 2 bei ungültiger Eingabe.
- **Abbildung (OS-Festlegung):** gleiche Feldnamen und Verschachtelung wie TOML (Tabellen → Objekte, Tabellen-Arrays → Arrays von Objekten); Ganzzahlen ohne, Gleitkommazahlen mit Dezimalpunkt; geschrieben wird wie in OUT-221 stabil sortiert, mit zwei Leerzeichen Einrückung, UTF-8 ohne BOM, LF. Ein Feld `$schema` (Verweis auf das JSON-Schema, für Editoren) wird beim Schreiben gesetzt und beim Laden ohne Warnung ignoriert. `null` wird wie ein fehlendes Feld behandelt (Standardwert); geschrieben wird `null` nie. JSON kennt keine Kommentare; Kommentare einer TOML-Datei gehen bei der Umwandlung verloren (Hinweis im Dialog und auf stderr).
- **Ausgabe:** `.osconfig.json`-Datei.
- **Akzeptanzkriterien:**
  - [ ] **Rundreise:** Für jede Konfiguration (Property-Test über alle Felder wie OUT-220) gilt: TOML → JSON → TOML ergibt eine byte-identische TOML-Datei, JSON → TOML → JSON eine byte-identische JSON-Datei; die geladenen Konfigurationen sind gleich.
  - [ ] Jede geschriebene JSON-Datei ist gültig gegen `osconfig.schema.json` (CI); alle Fixture-Konfigurationen aus `tests/fixtures/config/` liegen zusätzlich als JSON vor und werden ebenfalls validiert.
  - [ ] Fehlerregeln wie OUT-221: fehlende Felder → Standardwert; unbekannte Felder → Warnung mit Feldpfad; Typfehler → Meldung mit Zeile/Spalte, Datei wird nicht geladen; Datei ohne `schema_version` lädt als Version 1; zukünftige `schema_version` lädt mit Warnung (OUT-226).
  - [ ] `--config profil.osconfig.json` ergibt denselben Crawl wie `--config profil.osconfig` mit gleichem Inhalt (gleiche URL-Menge und Exporte).
  - [ ] Eine JSON-Datei mit Endung `.osconfig` und eine TOML-Datei mit Endung `.json` werden anhand des Inhalts korrekt gelesen.
  - [ ] `--convert-config a.osconfig b.osconfig.json` schreibt JSON, meldet verlorene Kommentare auf stderr und endet mit Exit-Code 0; ungültige Eingabe → Exit-Code 2 mit Zeile/Spalte.
  - [ ] Keine Geheimnisse in JSON-Dateien (OUT-225, Volltextsuche nach Testgeheimnissen).
- **Testfixture:** `tests/fixtures/config/` um JSON-Gegenstücke ergänzt; Datei mit Kommentaren; Datei mit `null`-Werten und `$schema`.

### OUT-289 Seiten als Markdown: Ansicht, Bulk-Export und MCP
- **Priorität:** P1
- **Meilenstein:** 1.x (nach 1.0)
- **Beschreibung:** Gespeichertes Original-HTML bzw. gespeichertes gerendertes HTML einer Seite wird in Markdown umgewandelt: als dritte Ansicht im Quelltext-Fenster (UI-041), als Bulk-Export „alle Seiten als Markdown“ und als Format `markdown` des MCP-Werkzeugs `url_content` (AGENT-017). Markdown ist kompakter als HTML und erhält anders als reiner Text (OUT-046) die Struktur; es eignet sich für Redaktionsprüfungen, Content-Migrationen und als Eingabe für Agenten und KI. Voraussetzung ist gespeichertes HTML („HTML speichern“ bzw. „Gerendertes HTML speichern“); ohne gespeichertes HTML sind Ansicht und Export deaktiviert mit Hinweis auf die Einstellung (UI-016). Die Umwandlung liest nur die Crawl-Datei, nie das Netz.
- **Eingaben/Konfiguration:**
  - Quelle: Original oder gerendert (wie OUT-045; „gerendert“ nur, wenn gespeichert).
  - Umfang: „Nur Inhaltsbereich“ (Standard an) verwendet den Inhaltsbereich nach ONPAGE-140 und entfernt damit Navigation, Kopf- und Fußbereich nach den Einstellungen des Inhaltsbereichs; aus = ganzer `<body>`.
  - „Kopfdaten voranstellen“ (Standard: im Export an, in der Ansicht aus): YAML-Block mit `url`, `title`, `status_code`, `crawled_at`, `source` (`original`/`rendered`).
  - Schlüssel `export.markdown.content_area`, `export.markdown.front_matter` im Profil; die Ansicht merkt sich ihre Schalter im UI-Zustand (UI-125).
- **Umwandlungsregeln (OS-Festlegung, Ausgabe CommonMark mit GFM-Tabellen):**
  - **Entfernt** werden `script`, `style`, `noscript`, `template`, `iframe`, `object`, `embed`, `canvas`, `svg`, Formularelemente (`form`-Inhalte bleiben als Text, `input`/`select`/`button`/`textarea` entfallen), Kommentare und Elemente mit Attribut `hidden`.
  - **Überschriften** `h1`–`h6` → `#` … `######` (ATX), Inhalt einzeilig; leere Überschriften entfallen.
  - **Absätze** und Blockelemente → durch eine Leerzeile getrennt; `br` → harter Umbruch mit `\` am Zeilenende; Leerraum nach HTML-Regeln zusammengefasst; höchstens eine Leerzeile in Folge.
  - **Hervorhebung:** `strong`/`b` → `**…**`, `em`/`i` → `*…*`, `del`/`s` → `~~…~~`; `code` → Backticks (bei enthaltenem Backtick mit längerer Begrenzung); `pre` → umzäunter Codeblock, Sprache aus einer Klasse `language-<x>` bzw. `lang-<x>`.
  - **Listen:** `ul` → `- `, `ol` → `1. ` mit fortlaufender Nummer ab `start`; Verschachtelung durch Einrückung um die Breite des Listenzeichens; `dl` → Begriff fett in eigener Zeile, Beschreibung als Absatz darunter.
  - **Links:** `[Text](URL "title")` mit absoluter URL (aufgelöst nach TECH-002); `title` nur, wenn vorhanden. Leerer Linktext → Bild-Alt, sonst die URL als Autolink `<URL>`. `javascript:`-Links und Links ohne `href` → nur Text. Fragment-Links auf dieselbe Seite bleiben relativ (`#abschnitt`).
  - **Bilder:** `![Alt](URL)` mit absoluter URL; ohne Alt `![](URL)`; `data:`-Bilder entfallen (Alt-Text bleibt als Text, falls vorhanden).
  - **Tabellen:** GFM-Tabelle; Kopfzeile = erste Zeile mit `th` bzw. die erste Zeile; `colspan`/`rowspan` → zusätzliche leere Zellen; `|` in Zellen als `\|`, Zeilenumbrüche in Zellen als Leerzeichen. Tabellen mit nur einer Spalte oder mit verschachtelten Tabellen (Layout-Tabellen) werden als Absätze ausgegeben.
  - **Sonstiges:** `blockquote` → `> `, `hr` → `---`; `figure`/`figcaption` → Bild, Bildunterschrift als eigener Absatz. Zeichen mit Markdown-Bedeutung im Text (`\`, `*`, `_`, `[`, `]`, `` ` ``, `#`/`>`/`-`/`+`/Ziffer+`.` am Zeilenanfang) werden maskiert.
  - Ausgabe UTF-8 ohne BOM, LF, abschließender Zeilenumbruch; dasselbe HTML ergibt auf allen Plattformen byte-identisches Markdown.
- **Ausgabe:**
  - **Ansicht (UI-041):** Der Umschalter „HTML | Sichtbarer Text“ erhält „Markdown“; Diff zwischen Original und gerendert zeilenbasiert; Schaltflächen „Kopieren“ und „Als .md speichern“.
  - **Bulk-Export „Web: All Page Markdown“** (Menü Bulk-Export → Web, CLI `--bulk-export "Web:All Page Markdown"`): Ordner `page_markdown/` mit einer `.md`-Datei je interner HTML-URL mit Status 200 und gespeichertem HTML, Dateinamen nach den Regeln von OUT-045, dazu `index.csv` (`Address`, `File`, `Source Type`). Seiten ohne gespeichertes HTML fehlen und werden in `index.csv` nicht aufgeführt; ist für keine Seite HTML gespeichert, ist der Export deaktiviert (CLI: Exit-Code 2 vor dem Crawl nach OUT-203, wenn „HTML speichern“ im Profil aus ist).
  - **MCP:** `url_content(crawl_id, url, format = "markdown")` (AGENT-017) liefert dieselbe Umwandlung (Quelle wie dort: gerendert, falls gespeichert, sonst Original; Umfang Inhaltsbereich; ohne Kopfdaten) im Untrusted-Datenblock (AGENT-004) mit Kürzung nach `max_chars`.
- **Akzeptanzkriterien:**
  - [ ] Golden-Test: Fixture-Seiten mit Überschriften, verschachtelten Listen, Links (relativ, absolut, `javascript:`, leer mit Bild-Alt), Bildern (mit/ohne Alt, `data:`), Tabellen (mit `th`, `colspan`, einspaltig, verschachtelt), Codeblock mit Sprache, Zitat, Definitionsliste und Sonderzeichen ergeben byte-gleich die erwarteten `.md`-Dateien (macOS, Windows, Linux).
  - [ ] Ein CommonMark/GFM-Parser liest jede erzeugte Datei ohne Fehler; Anzahl und Reihenfolge der Überschriften, Links und Bilder entsprechen dem HTML (Rundreise-Prüfung).
  - [ ] „Nur Inhaltsbereich“ an: kein Text aus `<nav>` und `<footer>` (bei Standard-Inhaltsbereich); aus: beide enthalten.
  - [ ] Bulk-Export auf `out-render`: genau eine `.md` je interner HTML-200-Seite mit gespeichertem HTML; `index.csv` hat dieselbe Zeilenzahl; Kopfdaten-Block mit `url` und `title` am Dateianfang.
  - [ ] Crawl ohne gespeichertes HTML: Ansicht zeigt Hinweis, Export ist deaktiviert, `url_content(format="markdown")` liefert die Meldung aus AGENT-017.
  - [ ] Die MCP-Antwort ist für dieselbe Seite inhaltsgleich mit der Ansicht bei gleichen Schaltern (ohne Kopfdaten) und steht im markierten Datenblock; eine Seite mit eingebetteter Anweisung (AGENT-004) erscheint nur dort.
  - [ ] Umwandlung einer 5-MB-Seite dauert höchstens 1 s (Referenzhardware A) und blockiert die Oberfläche nicht.
- **Testfixture:** `tests/fixtures/out-markdown/` (neu) mit den genannten Seiten und erwarteten `.md`-Dateien; `out-render`; `agent-injection` (10) für die MCP-Prüfung.
