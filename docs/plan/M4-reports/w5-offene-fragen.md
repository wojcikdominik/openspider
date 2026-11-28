# Offene Spec-Fragen nach Welle 5 (K11, U01, D01, V01)

Stand 27.11.2026 (Planer). Quellen: Bauer- und Abnahmeberichte `k11.md`, `k11-tests.md`, `u01.md`, `u01-tests.md`,
`d01.md`, `d01-tests.md`, `v01.md`, `v01-tests.md` (gelesen aus `int/w5d` = `866242fe`), `docs/plan/M4.md`,
`docs/plan/M4-packages.json`, `docs/plan/M4-reports/n01-auflagen.md`, `docs/spec/08-ai.md`, `10-agent-interface.md`,
`04-custom-integrations.md` (Stand `main` = `fda6f861`; Spec und Plan sind auf `int/w5d` identisch) sowie die
Platzhalter `git grep -n "wartet auf M4-" int/w5d -- crates apps` (103 Treffer). Stichproben im Code von `int/w5d`
sind mit Datei und Zeile genannt.

Kennungen K4-26 … K4-53 setzen Abschnitt 7 von `M4.md` fort (letzte vergebene Nummer dort: K4-25). Sie sind
Vorschläge; in Abschnitt 7 trägt sie der Orchestrator mit seiner Entscheidung ein. Auflagen, die keine Entscheidung
brauchen, stehen in `n01-auflagen.md` (B11 ff., C7 ff., D5, E8 ff.).

Die Spec ist in diesem Auftrag **nicht** geändert worden: Ein reiner Widerspruch innerhalb der Spec, der sich ohne
Entscheidung aus dem Text auflösen ließe, ist nicht gefunden worden. Die beiden Kandidaten (K4-26 Spec gegen Bestand der
Crawl-Datei, K4-40 Exportnamen gegen Filternamen) brauchen beide eine Wahl.

## Übersicht

Status: **offen** = braucht eine Entscheidung; **geklärt (Spec)** = die Spec beantwortet die Frage eindeutig, es fehlt nur
die Umsetzung; **entschieden, Nachtrag fehlt** = der Orchestrator hat am 27.11.2026 entschieden (`v01-tests.md`,
„Entscheidungen“), die Spec trägt es noch nicht.

| Kennung | Thema | Status | Paket | blockiert D05 |
|---|---|---|---|---|
| K4-26 | Quelle von `html_lang` und `structured_data_types` nach dem Crawl | **entschieden 27.11.: (a)** | D05 | nein mehr (war ja) |
| K4-27 | Ort der erteilten Kostenbestätigung über einen Neustart | **entschieden 27.11.: (a)** | D05 | nein mehr (war ja) |
| K4-28 | D05 bricht `acceptance_f02_ai_ipc.rs` (Datei nicht in `owns`) | **entschieden 27.11.: (a)** | D05 | nein mehr (war ja) |
| K4-29 | Kontingentsperre je KI-Lauf: Adapter-Instanzen | geklärt (Spec) | D05 | nein (Auflage E8) |
| K4-30 | Platzhalter für 1.1-Kriterien, die auf Welle-6-Pakete warten | **entschieden 27.11.: (a)** | D03, D02, Planer | nein |
| K4-31 | Bedeutung des Filters `Open` (AI Suggestions) | offen | N01 (C2) | nein |
| K4-32 | Kennzeichen „vorläufig“ nach Stoppen und anderen Laufenden | offen | N01 (C2) | nein |
| K4-33 | Umfang des Nachlaufs (Stufe 2): Lauf oder ganze Datei | offen | N01 (C2) | nein |
| K4-34 | Fehlende Spalten in `0006` für Vorschläge | offen | N01 (nach D05) | nein |
| K4-35 | Optionen von `SuggestionConfig` (Profil oder Laufargument) | offen | N01 | nein |
| K4-36 | `usage` wird über `guard::execute` bei Ausgabegrenze nicht verbucht | geklärt (Spec) | N01 | nein (Auflage B11) |
| K4-37 | `Refusal Reason` leer über `guard::execute` | geklärt (Spec) | N01 | nein (Auflage B12) |
| K4-38 | Zell-Tooltip der Statuszelle in der generischen Tabelle | offen | N01 oder D02 | nein |
| K4-39 | Prüfbereich der Vorschläge als eigene React-Wurzel (Tabellenbereich) | offen | N01 | nein |
| K4-40 | Exportnamen „AI Suggestions: Accepted/All“ kollidieren mit Tab-Exporten | offen | Spec | nein |
| K4-41 | Pfad der Golden-Dateien von KI-Paketen | offen | N01, D02, D03 | nein |
| K4-42 | Formelschutz der Vorschlagsexporte immer an | entschieden, Nachtrag eingetragen 27.11. | Spec | nein |
| K4-43 | AI-075: wie zählen Cache-Treffer der Reparaturrunden | entschieden, Nachtrag eingetragen 27.11. | Spec | nein |
| K4-44 | Format von `duplicate_of:` (volle Adresse) | entschieden, Nachtrag eingetragen 27.11. | Spec | nein |
| K4-45 | AI-030: sind `scope` und `thresholds` optional | offen | D03, D04 | nein |
| K4-46 | Teilbudgets je Aufgabentyp (AI-013) ohne Oberfläche | offen | N01 | nein |
| K4-47 | Antwortschema der Vorschläge liegt im Code statt unter `resources/ai/schemas/` | geklärt (Spec) | N01 | nein (Auflage B13) |
| K4-48 | Geltungsbereich der Vorschläge nur innerhalb von S | geklärt (Spec) | – | nein |
| K4-49 | Heartbeat in der Zukunft gilt unbegrenzt als aktuell | geklärt (Spec) | – | nein |
| K4-50 | Kostenvoranschlag der Vorschläge ohne Reparaturrunden | offen (gering) | N01 (C2) | nein |
| K4-51 | Antwort mit mehr als n Kandidaten | offen (gering) | – | nein |
| K4-52 | „Übernehmen“ eines ungültigen Kandidaten | offen (gering) | – | nein |
| K4-53 | Sammelbestätigung kleiner Bauauslegungen (V01 1, 2, 5, 6; U01 Fragen 4, 5) | offen (gering) | Spec | nein |
| K4-54 | Ungültiger Schlüssel (HTTP 401) beendet den KI-Lauf nicht (nach der Abnahme D05, 28.11.2026) | offen | Spec, N01 (B20) | nein (nach D05) |

Zahl (Erststand): 28 Fragen – 19 offen (davon 2 Planfragen: K4-28, K4-30), 6 aus der Spec geklärt, 3 entschieden ohne
Spec-Nachtrag. D05 blockierten K4-26, K4-27, K4-28.

**Stand nach den Entscheidungen des Orchestrators vom 27.11.2026:** K4-26, K4-27, K4-28 und K4-30 entschieden (je
Möglichkeit a, eingetragen in 08 AI-032/AI-020, `M4-packages.json`, `M4.md` Abschnitt 7); Nachträge zu K4-42, K4-43,
K4-44 in 08 AI-075/AI-076 geschrieben. Offen bleiben 15 Fragen (K4-31 … K4-35, K4-38 … K4-41, K4-45, K4-46, K4-50 …
K4-53); keine davon blockiert D05.

**Ergänzt 28.11.2026 (Planer, nach der Abnahme von D05 auf `int/w5m`):** K4-54 (Abschnitt F) aus `d05.md` (Nacharbeit
R3) und `d05-tests.md` (R3, Runde 2). Offen damit 16 Fragen.

---

## A Fragen, die D05 blockieren

### K4-26 Quelle von `html_lang` und `structured_data_types` für einen KI-Lauf nach dem Crawl
- **Fundstelle:** `v01.md`, Auslegung 6 („`html lang` steht in 1.0 nicht in der Crawl-Datei“); 08 AI-032, Nachtrag K4-24
  Punkt (1): „Titel, Meta Description, H1, URL, Status, Wortzahl, `html_lang`, `hreflang_self` und strukturierte
  Datentypen kommen aus den vorhandenen Spalten“; 03 TECH-091 (1.x) führt die Spalte `HTML Lang` erst neu ein.
  Prüfung des Planers in `int/w5d`: Keine Migration `0001` … `0006` hat eine Spalte für `<html lang>`; der Parser liest
  das Attribut nur für die Rechtschreibprüfung (`crates/os-parse/src/spelling/lang.rs:138`). Strukturierte Datentypen
  stehen in `structured_data` nur, wenn die Extraktion eingeschaltet ist (`StructuredDataConfig`; Standardwert nicht
  geprüft). `hreflang_self` ist aus der Tabelle `hreflang` ableitbar.
- **Unklar:** Die Annahme in K4-24 trifft für `html_lang` nicht zu und für `structured_data_types` nur bedingt. D05
  baut `ai_input.rs` und die Tabelle der gespeicherten KI-Eingaben; Kriterium (b) von D05 verlangt `html_lang` und
  strukturierte Datentypen „während des Crawls und nach dem Crawl“ im Mock-Log.
- **Möglichkeiten:**
  - (a) Die gespeicherten KI-Eingaben (K4-24, Punkt 1) enthalten zusätzlich `html_lang` und `structured_data_types`
    (wenige Bytes je Seite); `hreflang_self` wird aus `hreflang` gelesen. Folge: alles in D05-`owns` (neue Tabelle),
    keine Änderung an 1.0-Tabellen; Spec-Nachtrag zu AI-032 nötig.
  - (b) Neue Spalte `html_lang` in `urls` (wie später TECH-091). Folge: Änderung an 1.0-Schema und Writer, nicht in
    D05-`owns`, mehr Risiko so kurz vor 1.1; TECH-091 würde vorgezogen.
  - (c) Nur aus `page_html` ableiten (K4-24, Punkt 2), sonst `Skipped (input unavailable)`. Folge: Fragen mit
    `html_lang` (AI-057, 1.2) laufen nach dem Crawl ohne gespeichertes HTML nie; widerspricht dem Zweck von K4-24.
- **Empfehlung:** (a). Kleinster Eingriff, bleibt in D05, erfüllt K4-24 („gleiche Eingabe in Pause und nach Neustart“).
  Den Satz in AI-032 entsprechend berichtigen (Planer, nach Entscheidung).
- **Paket:** D05 (Spec-Nachtrag: Planer). **Blockiert D05:** ja (Kriterium b, Tabellenaufbau in `0006`).
- **Entscheidung (Orchestrator, 27.11.2026): (a).** Präzisierung: `structured_data_types` nur bei eingeschalteter
  Extraktion; sonst NULL (unterscheidbar von leerer Liste) und `Skipped (input unavailable)`. Standardwert nachgesehen:
  JSON-LD, Microdata und RDFa sind aus (Spec 01 CRAWL-065 „OS-Default: aus“, 03 TECH-180/181/182 „Standard aus“; Code
  `StructuredDataConfig::default` in `crates/os-model/src/config/v2.rs:817-826` ebenso). Eingetragen: 08 AI-032
  (berichtigt), D05-Kriterium (k).

### K4-27 Wo liegt die erteilte Kostenbestätigung eines Laufs über einen Neustart?
- **Fundstelle:** `d01-tests.md` B3-1, `d01.md` Auslegung 16; 08 AI-020, Nachtrag K4-13, „Kostenbestätigung beim
  Fortsetzen“: „gilt nach dem Fortsetzen weiter, auch nach App-Neustart“; D05-Kriterium (f) „Fortsetzen ohne neuen
  Voranschlag“. `ai_runs` (`crates/os-store/migrations/0006_ai.sql:2-7`) hat kein Feld dafür; D01 übergibt nur
  `Confirmation::Granted`.
- **Unklar:** Das Verhalten ist festgelegt, der Speicherort nicht. D05-`owns` erlaubt in `0006` nur die neue Tabelle der
  KI-Eingaben und keine Änderung an `os_store::ai::answers` (`begin_run`/`finish_run`, D01). Außerdem: Gilt ein
  Fortsetzen als derselbe Lauf (dieselbe `ai_runs`-Zeile)?
- **Möglichkeiten:**
  - (a) Spalte `confirmed_at TEXT` in `ai_runs` (0006 ist in keinem Release), Fortsetzen = dieselbe Zeile; D05-`owns`
    um diese Spalte und die Schreibfunktion erweitern. Folge: abfragbar, auch für CLI `--resume` und MCP
    `resume_crawl`; `schema-v6`-Fixture neu erzeugen (macht D05 ohnehin).
  - (b) Schlüssel in `ai_runs.config_json` ohne Schemaänderung. Folge: keine Migration, aber Bedeutung in einem
    JSON-Feld versteckt; Schreibfunktion liegt trotzdem außerhalb von D05-`owns`.
  - (c) Speicher der App (wie `ai-consent.json`). Folge: CLI und MCP sehen die Bestätigung nicht; widerspricht
    „Fortsetzen (App, CLI, MCP)“.
- **Empfehlung:** (a), mit „Fortsetzen = dieselbe `ai_runs`-Zeile; ein neuer Lauf („KI-Analyse starten“) = neue Zeile“.
  Klar und für alle drei Einstiege gleich.
- **Paket:** D05. **Blockiert D05:** ja (`owns`).
- **Entscheidung (Orchestrator, 27.11.2026): (a).** Eingetragen: 08 AI-020 (Nachtrag 27.11.2026), D05-`owns` und
  Kriterium (l).

### K4-28 `ai.decisionsRun` bricht `acceptance_f02_ai_ipc.rs`; die Datei liegt nicht in D05-`owns`
- **Fundstelle:** `v01.md`, „Benötigte Änderungen“ 1; `u01.md`, Fremdänderung `acceptance_f02_ai_ipc.rs`;
  `crates/os-app/tests/acceptance_f02_ai_ipc.rs:201-245` (Listen `U01_IMPLEMENTED`, `IMPLEMENTED`);
  `crates/os-app/src/commands/ai_catalog.rs:56,101,130` (`ai.decisionsRun` ist Stub). D05-`owns` nennt die Testdatei nicht.
- **Unklar:** Der F02-Test verlangt, dass jeder `ai.*`-Command ein Stub ist. Sobald D05 den Handler setzt, wird er rot;
  nach den Paketregeln müsste D05 abbrechen. U01 und V01 haben je eine eigene Ausnahmeliste eingefügt (beim Merge
  Konflikt an derselben Stelle).
- **Möglichkeiten:**
  - (a) D05-`owns` um die Datei erweitern, nur Umbau auf „prüfe nur Commands mit Stub-Handler“ (Liste der Stubs aus
    `ai_catalog`, Vorschlag V01); danach braucht kein Paket mehr eine Ausnahme.
  - (b) D05-`owns` nur um eine dritte Ausnahme `ai.decisionsRun` erweitern; den Umbau macht N01 (Auflage C7).
  - (c) D05 lässt den Test rot und meldet; N01 korrigiert. Folge: Gate von D05 nicht grün.
- **Empfehlung:** (a). Einmalige Änderung, beseitigt die Konfliktquelle für D02, C02 und alle Folgepakete.
- **Paket:** D05 (sonst N01). **Blockiert D05:** ja (`owns`).
- **Entscheidung (Orchestrator, 27.11.2026): (a).** Eingetragen: D05-`owns` und Kriterium (m); Auflage N01 C7 damit
  erledigt.

---

## B Fragen und Klärungen, die D05 betreffen, aber nicht blockieren

### K4-29 Kontingentsperre „je KI-Lauf“: wo sie liegt — geklärt (Spec)
- **Fundstelle:** `d01-tests.md` B2-2, Frage 2; `d01.md`, Nacharbeit „Fundstelle zu B2-2“; 08 AI-020, Nachtrag
  26.11.2026 (Auslegung 15 aus D01, bestätigt): „Die Sperre … gilt je KI-Lauf … ein neuer Lauf versucht es wieder.“
- **Geklärt:** Die Spec legt die Wirkung eindeutig fest. Der Stand von D01 setzt die Sperre am Laufbeginn in den
  Adapter-Instanzen zurück; teilen zwei gleichzeitige Läufe (z. B. Decision-Fragen und Vorschläge) dieselben Instanzen,
  hebt der zweite Start die Sperre des ersten auf – das widerspricht „je KI-Lauf“.
- **Folge:** Auflage an D05 (`n01-auflagen.md` E8): Der Laufrahmen `os_ai::run` legt je KI-Lauf eigene
  Adapter-Instanzen an (Vorschlag aus `d01.md`). **Paket:** D05. **Blockiert D05:** nein.

### K4-30 Platzhalter für 1.1-Kriterien, die auf Pakete der Welle 6 warten
- **Fundstelle:** `crates/os-ai/tests/pending_m4_v01.rs:64` `default_scope_includes_ai_mismatch_filters`
  (`wartet auf M4-N01: Antwortformat der Fragen title-matches-content …`) – die Fragen AI-054/AI-055 baut D03
  (Welle 6, hängt an N01); ebenso `e2e/ai-cost/u01.spec.ts` (D02). `M4.md` E4: Auflösung durch N01 (Wellen 1–5) bzw.
  **N02 (Wellen 6–13, Release 1.2)**. AI-070 ist eine 1.1-ID (P0).
- **Unklar:** Niemand löst vor Q11 die Platzhalter auf, die auf D02, D03 oder S02 warten; N01 kann es nicht (läuft
  vorher), N02 kommt erst mit 1.2. Q11 misst die 1.1-IDs.
- **Möglichkeiten:**
  - (a) Jedes Welle-6-Paket löst die Platzhalter, die auf es warten, selbst auf (`owns` je Paket: nur das `#[ignore]` der
    genannten Tests in `pending_m4_*.rs`); der Grund wird vorher auf das richtige Paket gesetzt (N01, Auflage B14; `owns` E10).
  - (b) Kurzes Nacharbeitspaket „N01b“ nach Welle 6, vor Q11.
  - (c) Q11 führt sie als bekannte Lücke; Abdeckung der 1.1-IDs dann unvollständig.
- **Empfehlung:** (a). Kein neues Paket, das wartende Paket kennt den Test, Q11 misst vollständig.
- **Paket:** Planer (`M4-packages.json`: D02, D03, S02), N01 (Gründe umsetzen). **Blockiert D05:** nein.
- **Entscheidung (Orchestrator, 27.11.2026): (a).** Eingetragen: `owns` von D02, D03, S02; N01 Auflage B14.

---

## C Vorschläge (V01) – Anbindung, Datenhaltung, Oberfläche

### K4-31 Bedeutung des Filters `Open` im Tab „AI Suggestions“
- **Fundstelle:** `v01-tests.md` B2-2 (an Orchestrator/Planer verwiesen); `v01.md` Auslegung 9; 08 AI-070 (Filter
  `Open`), AI-076 (`Review Status` ∈ {`Open`, `Accepted`, `Edited`, `Rejected`}; „URLs mit `No valid suggestion` bleiben
  `Open`“).
- **Unklar:** Einträge ohne Kandidaten (`Pending`, `Error: …`, `Budget exceeded`) tragen `Review Status = Open`, stehen
  im Bau aber nicht im Filter `Open`. Die Spec sagt nicht, ob `Open` den Prüfstatus allein meint.
- **Möglichkeiten:**
  - (a) `Open` = `Review Status = Open`, gleich welcher Status. Folge: Filter enthält auch Fehler und wartende
    Einträge, die man nicht prüfen kann.
  - (b) `Open` = `Review Status = Open` und mindestens ein Kandidat (`Valid` oder `No valid suggestion`) – „prüfbar und
    offen“ (Stand des Baus). Folge: Fehler stehen in `Errors`, wartende nur in den Elementfiltern.
  - (c) `Open` = `Review Status = Open` und Status `Valid`. Folge: `No valid suggestion` fiele heraus, obwohl AI-076 sie
    ausdrücklich `Open` nennt.
- **Empfehlung:** (b). Der Filter ist die Arbeitsliste der Prüfung; deckt sich mit AI-076 und dem gebauten Stand. Dazu
  festhalten: `Accepted` umfasst `Accepted` und `Edited` (wie der Export „Accepted“).
- **Paket:** Spec-Nachtrag; Code unverändert (V01). **Blockiert D05:** nein.

### K4-32 Kennzeichen „vorläufig“ nach Stoppen, Budgetende, Abbruch und Pause
- **Fundstelle:** `v01-tests.md` B2-5; 08 AI-071 („Bis der Nachlauf abgeschlossen ist, sind Status und Kandidat 1 als
  vorläufig gekennzeichnet“, Nachlauf „nach Abschluss des Laufs“); AI-020 Nachtrag K4-13 (Stoppen, Pause, Beenden).
  V01 kennt nur `RunControl::stop` für Pause und Stopp (`v01.md`, Schnittstelle); die Anbindung an den Laufrahmen ist
  Auflage N01 C2.
- **Unklar:** Läuft der Nachlauf (ohne Request) nach einem Stopp, nach erreichtem Budget, nach Abbruch durch Beenden?
  Heute bleiben fertige Einträge nach einem Stopp dauerhaft vorläufig.
- **Möglichkeiten:**
  - (a) Nachlauf bei jedem Laufende mit übernommenen Ergebnissen (fertig, Stopp, Budget, Kontingent), nicht bei Pause
    und nicht bei Abbruch/Absturz; danach kein Kennzeichen. Ein späterer Lauf wiederholt den Nachlauf (siehe K4-33).
  - (b) Nachlauf nur, wenn für das Element keine offenen Einträge mehr im Geltungsbereich sind; sonst bleibt „vorläufig“
    sichtbar, auch dauerhaft.
  - (c) Nachlauf auch bei Pause.
- **Empfehlung:** (a). Stopp ist endgültig, der Nutzer erwartet einen festen Stand; der Nachlauf kostet nichts; Pause
  und Abbruch setzen fort bzw. werden neu gesendet, dort bleibt das Kennzeichen zu Recht.
- **Paket:** N01 (C2), V02 übernimmt. **Blockiert D05:** nein (D05 liefert nur die Ende-Pfade).

### K4-33 Umfang des Nachlaufs (Stufe 2 der Dublettenprüfung): Lauf oder ganze Datei
- **Fundstelle:** `v01.md` Auslegung 4 („Nachlauf über alle Einträge des Elements in der Datei (nicht nur des Laufs) –
  strenger als ‚desselben Laufs‘“); 08 AI-071: „gegen die Kandidaten anderer URLs **desselben Laufs**“.
- **Unklar:** Der Bau weicht von der Spec ab. Folgen beider Lesarten: Nach Spec kann ein zweiter Lauf auf andere URLs
  denselben Kandidaten erneut vorschlagen (Dublette über Läufe). Nach dem Bau kann ein späterer Lauf mit einer
  kleineren Adresse Kandidat 1 einer früher fertigen URL verschieben (ohne Request, aber überraschend).
- **Möglichkeiten:**
  - (a) Wie Spec: nur Kandidaten desselben Laufs. V01 ändern.
  - (b) Wie Bau: alle Einträge des Elements in der Datei, Adressreihenfolge entscheidet.
  - (c) Lauf plus Bestand: Kandidaten des Laufs in Adressreihenfolge; Kandidat 1 früherer Läufe und übernommene Werte
    (`Accepted`/`Edited`) gelten als bereits reserviert und ändern sich nie.
- **Empfehlung:** (c). Keine Dubletten über Läufe und keine nachträgliche Änderung früherer Ergebnisse; bleibt
  deterministisch (AI-071, 20 Wiederholungen).
- **Paket:** N01 (mit C2), Spec-Nachtrag. **Blockiert D05:** nein.

### K4-34 Fehlende Spalten in `0006_ai.sql` für Vorschläge
- **Fundstelle:** `v01.md`, „Datenhaltung im vorhandenen Schema“ und „Benötigte Änderungen“ 2;
  `crates/os-store/migrations/0006_ai.sql:87-93` (`ai_suggestions`: nur `value`, `status`, `violations_json`,
  `review_status DEFAULT 'unreviewed'`, `accepted_value`, `model`, `request_id`); 08 AI-071 (Spalten je Kandidat
  `… Length`, `… Pixel Width`), AI-076 (Export mit `Length`, `Pixel Width`, `Origin`, `Generated At`).
- **Unklar:** V01 legt Länge, Pixelbreite, Runde, „vorläufig“, Zeitpunkt, Fokus-Begriff, Lauf, Herkunft und die
  übernommenen Werte als JSON in `violations_json` bzw. als Pseudo-Element `focus` ab. 0006 steckt in keinem Release;
  nach 1.1 ginge es nur noch über `0007`.
- **Möglichkeiten:**
  - (a) Spalten jetzt in 0006 ergänzen (Liste aus `v01.md`), `review_status` mit `CHECK` auf die Werte aus AI-076 und
    Standard `open`, V01-Speicher umstellen, `schema-v6`-Fixture neu erzeugen. Folge: einmal Arbeit, sauberes Schema,
    abfragbar über SQL/MCP (AI-100).
  - (b) JSON lassen. Folge: funktioniert, aber Exporte und Abfragen hängen an einem internen JSON-Format; spätere
    Änderung nur mit Migration.
  - (c) eigene Tabelle `ai_suggestion_meta`. Folge: wie (a), aber zwei Tabellen für einen Eintrag.
- **Empfehlung:** (a), vor dem Release 1.1. Die Gelegenheit ist einmalig (wie B7 `normalized`). Reihenfolge: nach D05,
  das 0006 ebenfalls ändert.
- **Paket:** N01 (Auflage B15, Entscheidung offen). **Blockiert D05:** nein (nur Reihenfolge an 0006).

### K4-35 Welche Optionen hat `SuggestionConfig`?
- **Fundstelle:** `v01.md`, „Benötigte Änderungen“ 4; `crates/os-model/src/config/ai.rs:46-53` (`title`, `meta`, `h1`,
  `alt`, `style`, `segment_styles`); 08 AI-070 (Kandidatenzahl 1–5, Standard 3; Geltungsbereich; Fokus-Datei), AI-075
  (Reparaturrunden 0–3, Standard 2; Regeln (1)–(6) einzeln abschaltbar außer (2)), AI-074 (Option „H1 darf Title
  gleichen“, V02), AI-077 („Regelset je Profil“).
- **Unklar:** Die Spec nennt die Optionen unter „Eingaben/Konfiguration“, sagt aber nur für AI-077, dass sie im Profil
  liegen. Heute kommen Kandidatenzahl, Runden und Regeln nur als Command-Argumente; CLI und Zeitplanung (AI-022) hätten
  keinen Weg.
- **Möglichkeiten:**
  - (a) Im Profil: `candidates`, `repair_rounds`, `rules` (je Regel an/aus; Regel (2) nicht abschaltbar), später
    `h1_may_equal_title`; Geltungsbereich und Fokus-Datei je Lauf. Folge: Teil von Profil, Export/Import, CLI.
  - (b) Nur je Lauf (Stand). Folge: jede Ausführung muss alles mitgeben; CLI braucht eigene Optionen.
  - (c) Profil als Standard, Lauf darf überschreiben.
- **Empfehlung:** (a); (c) nur, wenn eine Oberfläche das Überschreiben braucht. Begründung: gleiche Behandlung wie die
  übrigen KI-Einstellungen, AI-022 „Aufgaben … kommen aus dem Konfigurationsprofil“.
- **Paket:** N01 (Auflage B16, Entscheidung offen; `config/ai.rs` additiv nach D05). **Blockiert D05:** nein.

### K4-36 `usage` wird nicht verbucht, wenn die Ausgabegrenze über `guard::execute` greift — geklärt (Spec)
- **Fundstelle:** `v01.md` Auslegung 11; `v01-tests.md` B1-1 (Entscheidung 27.11.2026: Lücke im Kern, Auflage N01);
  Test `crates/os-ai/tests/acceptance_v01_mock.rs:206`. Code: `crates/os-ai/src/provider.rs:249`
  (`ProviderError::OutputLimitReached { reason }` ohne `usage`).
- **Geklärt:** 08 AI-020 (Nachtrag 24.11.2026) „die gemeldeten Tokens werden verbucht“; AI-014 Kriterium „Szenario
  `output-limit` … `Output Tokens` = gesendete Grenze, `Cost` > 0“. Folge der Lücke: Ein Vorschlagslauf verbucht für
  solche Antworten 0 USD, der Budgetverbrauch liegt unter dem, was der Anbieter berechnet.
- **Folge:** Auflage B11 (fehlte in der Auflagenliste). **Paket:** N01. **Blockiert D05:** nein (D01-Weg verbucht korrekt).

### K4-37 Spalte `Refusal Reason` — geklärt (Spec)
- **Fundstelle:** `v01.md` Auslegung 11, letzter Satz („beim Refusal über diesen Weg fehlt der Refusal-Text (Spalte
  `Refusal Reason` leer)“); 08 AI-014 Kriterium (Refusal-Text gekürzt ≤ 500 Zeichen in `Refusal Reason`) und Nachtrag
  26.11.2026 (Spalte nach `Error Code`, vor `Duration ms`).
- **Geklärt:** Die Spalte fehlt nicht: `0006_ai.sql:44` hat `refusal_reason` mit Längenprüfung. Es fehlt die Befüllung auf
  dem Weg `AiProvider::execute` → `ProviderError::Refusal` (ohne Text, `provider.rs`).
- **Folge:** Auflage B12. **Paket:** N01. **Blockiert D05:** nein.

### K4-38 Zell-Tooltip der Statuszelle in der generischen Tabelle
- **Fundstelle:** `v01.md`, „Benötigte Änderungen“ 3; Platzhalter `pending_m4_v01.rs:74`
  `status_cell_tooltip_in_the_table` (`wartet auf M4-N01`); 08 AI-020 (Tooltip der Statuszelle bei `Error: Output limit
  reached`), AI-020 Nachtrag 26.11.2026 und D02-`prompt_notes` (Hinweis zu `Skipped (input unavailable)` „Tooltip bzw.
  Detailbereich“).
- **Unklar:** Die generische Tabelle (`DataGrid`) hat keinen Anschluss für Zell-Tooltips; kein M4-Paket besitzt
  `apps/desktop/src/features/table/**`. D02 braucht denselben Anschluss.
- **Möglichkeiten:**
  - (a) N01 baut einen Tooltip-Anschluss je Spalte (z. B. Tooltip-Funktion in der Spaltenbeschreibung) als Auflage mit
    benannten Pfaden; V01-Platzhalter auflösen, D02 nutzt ihn.
  - (b) D02 baut ihn, weil es ihn zuerst für den eigenen Tab braucht; V01-Platzhalter dann „wartet auf M4-D02“.
  - (c) Kein Zell-Tooltip; Hinweis nur im Prüf- bzw. Detailbereich, Spec-Nachtrag zu AI-020.
- **Empfehlung:** (a). N01 liegt vor D02, ein gemeinsamer Anschluss statt zwei Lösungen.
- **Paket:** N01 (Auflage C9, Entscheidung offen), D02. **Blockiert D05:** nein.

### K4-39 Prüfbereich der Vorschläge als eigene React-Wurzel
- **Fundstelle:** `v01.md` Auslegung 13 und „Benötigte Änderungen“ 3 (`registerTabPanel(tabId, Component)` in
  `features/table/UpperTable.tsx` bzw. `app/mainRegions.ts`); `v01-tests.md`, „Nicht geprüft“ (axe des Prüfbereichs,
  de/en).
- **Unklar:** Die Spec legt den Ort des Prüfbereichs nicht fest (AI-076 „im Tab“). Der Bau hängt ihn als eigene
  React-Wurzel rechts unten ans Dokument; das wird Vorbild für weitere KI-Tabs.
- **Möglichkeiten:**
  - (a) Anschluss für tab-eigene Bereiche in der Tabellenregion (N01), Prüfbereich dorthin verschieben.
  - (b) Prüfbereich als Abschnitt im unteren Detailbereich über die vorhandene Registrierung des Detailrahmens (wie D02
    für den KI-Abschnitt).
  - (c) Für 1.1 belassen; Q11 prüft axe und Tastaturbedienung des Bereichs, Umbau in 1.2.
- **Empfehlung:** (a). Kleiner Anschluss, vermeidet eine zweite Wurzel als Muster; axe-Prüfung des Bereichs in N01 mit
  aufnehmen.
- **Paket:** N01 (Auflage C10, Entscheidung offen). **Blockiert D05:** nein.

### K4-40 Exportnamen „AI Suggestions: Accepted“ und „AI Suggestions: All“
- **Fundstelle:** `v01.md` Auslegung 12; 08 AI-076 (Ausgabe), AI-070 (Tab-Filter `Accepted`, dazu `All` je Tab);
  06 OUT-192 (Leerraum um `:` wird ignoriert), OUT-008 (Testfixture: keine Kollision zweier Exporte auf denselben
  Dateinamen).
- **Unklar:** Die Spec-Namen kollidieren mit den Tab-Exporten `AI Suggestions:Accepted` und `AI Suggestions:All`
  (gleiche Auflösung, gleicher Dateiname). Gebaut: `AI Suggestions:Accepted Suggestions`, `AI Suggestions:All
  Suggestions`.
- **Möglichkeiten:**
  - (a) Namen wie gebaut in die Spec übernehmen (Dateien `ai_suggestions_accepted_suggestions.csv`,
    `ai_suggestions_all_suggestions.csv`).
  - (b) Eigener Präfix ohne Tab-Bezug, z. B. `AI Suggestions Export:Accepted`. Folge: Umbenennen im Code.
  - (c) Die Exporte entfallen zugunsten der Tab-Exporte der Filter. Folge: Spalten aus AI-076 (`Current Value`,
    `New Value`, `Origin` …) passen nicht zu den Tabspalten.
- **Empfehlung:** (a). Keine Codeänderung, eindeutig, folgt dem Muster `<Bereich>:<Name>` aus OUT-008 (`Links:All
  Inlinks`). Dieselbe Prüfung vor D02 für Exporte des Tabs „AI Decisions“ (AI-039).
- **Paket:** Spec-Nachtrag (Planer); Hinweis an D02. **Blockiert D05:** nein.

### K4-41 Pfad der Golden-Dateien von KI-Paketen
- **Fundstelle:** `v01-tests.md` B3-1; 08 AI-076 (`tests/golden/ai/suggestions-accepted.csv`); gebaut
  `tests/fixtures/ai/suggest/golden/suggestions-accepted.csv` (V01-`owns` enthält `tests/golden/**` nicht). 08 nennt
  `tests/golden/ai/…` noch für AI-039, AI-041, AI-050, AI-051, AI-092, AI-101, AI-107, AI-124; D02- und D03-`owns`
  enthalten ihre Golden-Pfade ebenfalls nicht.
- **Unklar:** Spec an den Bau anpassen (wie bei D01, „Pfad an den Bau angepasst“) oder den Bau an die Spec.
- **Möglichkeiten:**
  - (a) Spec-Konvention `tests/golden/ai/` bleibt; N01 verschiebt die V01-Datei (Testpfad mit), Planer ergänzt die
    Golden-Pfade in den `owns` von D02, D03 und späteren Paketen.
  - (b) Golden-Dateien im Fixture-Ordner des jeweiligen Pakets; alle neun Stellen in 08 anpassen.
  - (c) Nur AI-076 anpassen, übrige bleiben. Folge: zwei Konventionen.
- **Empfehlung:** (a). Eine Konvention, wie in M1–M3 (`tests/golden/*`); eine Datei verschieben statt neun Spec-Stellen
  ändern.
- **Paket:** N01 (Auflage B17, Entscheidung offen), Planer (`owns` D02/D03, E9). **Blockiert D05:** nein.

### K4-42 Formelschutz der Vorschlagsexporte — entschieden, Nachtrag eingetragen (08 AI-076, 27.11.2026)
- **Fundstelle:** `v01-tests.md` B1-2, B2-4 und „Entscheidungen 27.11.2026“ („B2-4 entfällt: Der CSV-Formelschutz ist
  für beide Vorschlags-Exporte immer an“); 08 AI-076 („CSV-Injection-Schutz nach OUT-016“); 06 OUT-016 (Option, P2,
  Standard aus), OUT-003 (in XLSX nie als Formel, Wert als Text).
- **Entschieden:** In CSV immer maskiert (`= + - @` mit `'`), auch ohne die Option aus OUT-016; in XLSX unverändert als
  Textzelle (nach OUT-003). Gebaut und abgenommen.
- **Vorschlag Nachtrag AI-076:** „Der Schutz nach OUT-016 ist für beide Exporte immer an, unabhängig von der Option
  (KI-Text ist nicht vertrauenswürdig); in XLSX gilt OUT-003 (Textzelle ohne `'`).“ **Paket:** Spec (Planer). **D05:** nein.

### K4-43 AI-075: wie zählen Cache-Treffer der Reparaturrunden — entschieden, Nachtrag eingetragen (08 AI-075, 27.11.2026)
- **Fundstelle:** `v01-tests.md` B2-1 und „Entscheidungen 27.11.2026“; `v01.md` Auslegung 7; 08 AI-075 („nach 1 + 2
  Requests“), AI-015 („Gleiche Eingabe … erzeugt keinen neuen Request“), AI-014 (Cache-Treffer = Zeile mit `Cache Hit`).
- **Entschieden:** Gezählt werden HTTP-Requests. Ist eine Reparaturrunde identisch mit der vorigen, ist sie ein
  Cache-Treffer: eine Protokollzeile mit `Cache Hit = true` und Kosten 0, kein HTTP-Request; die Runde zählt als Runde.
- **Vorschlag Nachtrag AI-075:** zweites Kriterium ergänzen um „liefert der Mock in jeder Runde dieselben Kandidaten, ist
  die dritte Runde ein Cache-Treffer (2 HTTP-Requests, 3 Protokollzeilen)“. **Paket:** Spec. **D05:** nein.

### K4-44 Format von `duplicate_of:` — entschieden, Nachtrag eingetragen (08 AI-075, 27.11.2026)
- **Fundstelle:** `v01-tests.md` B2-3 und „Entscheidungen 27.11.2026“; 08 AI-075 (Beispiel `duplicate_of:/b/`), AI-071
  (`duplicate_candidate:http://ai.test:8080/a/`).
- **Entschieden:** `duplicate_of` nennt die volle Adresse wie `duplicate_candidate`. **Vorschlag:** Beispiel in AI-075 auf
  `duplicate_of:http://ai.test:8080/b/` ändern. **Paket:** Spec. **D05:** nein.

### K4-47 Antwortschema der Vorschläge im Code — geklärt (Spec)
- **Fundstelle:** `v01.md`, „Benötigte Änderungen“ 5 (Schema in `prompt::schema`); 08 AI-021 (CI-Test „für alle Schemas
  unter `resources/ai/schemas/`“), Konventionen („Datendateien statt Code“).
- **Geklärt:** Das Schema gehört als Datei unter `resources/ai/schemas/`, damit der CI-Test aus AI-021 es erfasst.
- **Folge:** Auflage B13. **Paket:** N01. **D05:** nein.

### K4-48 Geltungsbereich der Vorschläge nur innerhalb von S — geklärt (Spec)
- **Fundstelle:** `v01.md` Auslegung 3 (Problemfilter ∩ S; noindex-Seite ohne Title erhält keinen Vorschlag).
- **Geklärt:** 08 Konventionen, „Grundmenge und Geltungsbereich“: S gilt für seitenbezogene KI-Aufgaben; „Abweichungen
  sind je Frage bzw. Aufgabe einstellbar“. Der Bau entspricht dem. Keine Änderung. **D05:** nein.

### K4-50 Kostenvoranschlag der Vorschläge ohne Reparaturrunden (gering)
- **Fundstelle:** `v01.md` Auslegung 10; 08 AI-012 (Voranschlag je Aufgabe).
- **Unklar:** Der Voranschlag zählt nur Erstrequests; bei vielen ungültigen Kandidaten liegen die Kosten bis zum
  (1 + R)-fachen darüber.
- **Möglichkeiten:** (a) wie gebaut, Budget schützt über die Reservierung (AI-013); (b) erwartete Requests = Erstrequests
  × (1 + gelernte Reparaturquote je Aufgabe); (c) obere Schätzung mit 1 + R Requests (widerspricht „hoch = erwartet ×
  1,25“).
- **Empfehlung:** (a) für 1.1, (b) als Kandidat für die Kalibrierung in Q02. **Paket:** N01 (C2). **D05:** nein.

### K4-51 Antwort mit mehr als n Kandidaten (gering)
- **Fundstelle:** `v01-tests.md` B3-2; 08 AI-070 (Kandidatenzahl n).
- **Möglichkeiten:** (a) die ersten n speichern, Rest verwerfen (Stand); (b) Schemaverstoß mit Reparaturversuch
  (kostet einen Request); (c) alle speichern.
- **Empfehlung:** (a). Kein Mehrpreis, deterministisch; als Satz in AI-070 festhalten. **D05:** nein.

### K4-52 „Übernehmen“ eines ungültigen Kandidaten (gering)
- **Fundstelle:** `v01.md` Auslegung 8; 08 AI-076 („je URL einen Kandidaten übernehmen“, Massenaktion nur gültige).
- **Möglichkeiten:** (a) nur gültige übernehmbar, ungültige nur über „Bearbeiten“ (dann `Edited` mit Verstoßliste) –
  Stand; (b) jeder Kandidat übernehmbar, ungültige als `Accepted` mit Verstoßliste.
- **Empfehlung:** (a). Verstöße bleiben sichtbar (AI-076 „keine stille Korrektur“). **D05:** nein.

---

## D Decision-Engine und Einstellungen

### K4-45 AI-030: sind `scope` und `thresholds` optional?
- **Fundstelle:** `d01-tests.md` B2-1 und Frage 1; 08 AI-030 („`scope` (…), `thresholds` (AI-037), optional `issues`
  und `segment`“). Hinweise in der Spec: AI-030 Nachtrag 26.11.2026 („Ohne Angabe in `scope` gilt eine Bildfrage für
  …“), Konventionen (Standard-Geltungsbereich S), AI-037 (Standardwerte aller Schwellwerte). Schema und Prüfung von D01
  behandeln beide als optional.
- **Möglichkeiten:**
  - (a) Optional mit Standard: fehlendes `scope` = S (Seitenfragen) bzw. interne Bilder mit 200 (Bildfragen), fehlende
    `thresholds` = Standards aus AI-037. Folge: Stand des Baus; Bibliotheksdateien kürzer.
  - (b) Pflicht; Schema und Bibliothek (D03) ergänzen. Folge: jede Frage nennt Werte, die meist den Standards gleichen.
- **Empfehlung:** (a), als Nachtrag in AI-030. **Paket:** D03 (Bibliothek), D04 (Editor). **D05:** nein.

### K4-46 Teilbudgets je Aufgabentyp (AI-013) ohne Oberfläche
- **Fundstelle:** `u01.md` Frage 6; `n01-auflagen.md` E5 („Entscheidung offen“); 08 AI-013 (Eingabe „Teilbudgets je
  Aufgabentyp (optional)“, P0-Kriterium „Teilbudget `suggest-title = 0,50 USD` stoppt nur Title-Vorschläge“). Kern
  vorhanden: `crates/os-model/src/budget.rs:64` (`BudgetLimits::tasks`).
- **Unklar:** Wer baut die Eingabe auf der Konfigurationsseite KI (und den Abgleich in den `prompt_notes` von C01 für
  CLI)? Ohne sie ist das Kriterium nur über Profil-Import prüfbar.
- **Möglichkeiten:** (a) N01 als Auflage in `features/ai/settings/PolicyBudget.tsx` (U01-Pfad) plus Test; (b) D02;
  (c) auf 1.2 verschieben, Kriterium als 1.2 markieren (Änderung des 1.1-Umfangs, Nutzer).
- **Empfehlung:** (a). Kleine Oberfläche, Kern fertig, P0-ID bleibt in 1.1. D05 muss beim Laufstart die Teilbudgets aus
  `ai.budget` mitführen (`save_limits`), das gehört zu D05 Schritt (3).
- **Paket:** N01 (Auflage C11, Entscheidung offen). **D05:** nein.

### K4-49 Heartbeat in der Zukunft gilt unbegrenzt als aktuell — geklärt (Spec)
- **Fundstelle:** `k11.md` Hinweis (iv); `n01-auflagen.md` E6.
- **Geklärt:** 01 CRAWL-125, Nachtrag K4-22: „Eine Bezugszeit in der Zukunft gilt als aktuell“ (Kriterium „1 ms in der
  Zukunft → laufend“); 08 AI-020: „Grenze, Uhr und Zukunftsregel wie CRAWL-125“. Das Risiko (vorgehende Uhr eines
  anderen Rechners hält den Crawl belegt) ist Verhalten von 1.0 und bleibt; keine Änderung in 1.1. **D05:** nein.

### K4-53 Sammelbestätigung kleiner Bauauslegungen (gering)
- **Fundstelle und Inhalt:**
  - `v01.md` Auslegung 1: `Suggestion 1` ist der gewählte Kandidat (erster gültiger nach dem Nachlauf), gespeichert wird in
    Antwortreihenfolge.
  - `v01.md` Auslegung 2: Spaltennamen Meta nach AI-072 „analog“, zusätzliche Spalten `… Suggestion k Valid` und
    `… Suggestion Provisional` (nicht in der Spaltenliste von AI-071).
  - `v01.md` Auslegung 5: Zeilenumbruch/Anführungszeichen bei Titles sind Verstoß, bei Meta Descriptions werden sie
    entfernt (AI-072); `empty` immer Verstoß.
  - `v01.md` Auslegung 6: Seitensprache aus `<html lang>` (nur aus gespeichertem HTML), sonst whatlang ≥ 0,95 – hängt an
    K4-26.
  - `u01.md` Frage 4: Export der Datenflusstabelle über „CSV kopieren“/„Als CSV speichern“ statt Exportdialog (AI-018
    „Export als CSV“).
  - `u01.md` Frage 5: deutscher Spaltenname „Reasoning-Stufe“ statt Glossareintrag „Reasoning“.
  Die Nacharbeit U01 vom 26.11. sagt dazu nur „die übrigen Punkte bleiben wie berichtet“ – keine ausdrückliche
  Entscheidung.
- **Möglichkeiten:** (a) alle wie gebaut bestätigen, Nachtrag „bestätigte Auslegungen aus V01/U01“ in 08 (Muster wie bei
  D01); (b) einzeln entscheiden.
- **Empfehlung:** (a); Auslegung 6 erst nach K4-26. **Paket:** Spec (Planer). **D05:** nein.

---

## E Hinweise zur Planpflege (keine Fragen)

- Erledigt 27.11.2026: `M4.md` Abschnitt 7, Zeile K4-24, nannte die Speichergrenze 20.000 Zeichen noch „Vorschlag des Planers, zu bestätigen“;
  08 AI-032 trägt sie seit `cb882d40` als „bestätigt, Orchestrator 26.11.2026“. Zeile nachziehen.
- Erledigt 27.11.2026: Kopf von `n01-auflagen.md` auf „K11, U01, D01, V01 abgenommen, in `int/w5d`“.
- Platzhalter mit Gründen auf bereits gebaute Pakete (`wartet auf M4-D01/U01/K11` in `pending_m4_k0*.rs`,
  `pending_m4_v01.rs`, `e2e/ai-suggestions/suggestions.spec.ts:206`) hängen tatsächlich an D05 bzw. N01 C2; Gründe in
  N01 berichtigen (Auflage B14) bzw. mit C2 auflösen (Auflage C8).

---

## F Nach der Abnahme von D05 (28.11.2026)

### K4-54 Ungültiger Schlüssel (HTTP 401) beendet den KI-Lauf nicht
- **Fundstelle:** `d05-tests.md` R3 (Runde 1: ein Lauf mit Mock-Szenario `401` endete nicht in 60 s) und Runde 2
  („passt zu INTEG-005/AI-017 … kein Befund“); `d05.md` Nacharbeit R3 mit Test `run_queue_lifecycle.rs`
  `status_401_trips_the_breaker_after_ten_requests_without_retries`. Spec: 04 INTEG-005 („400/401/403/404 werden nicht
  wiederholt“); 08 AI-017 (Schutzschalter: nach 10 endgültigen Fehlern 5 Minuten keine Requests an den Anbieter;
  Ausnahmen nur Spend-Cap-429, endgültiges OpenRouter-402, OpenRouter-403 `refusal`, Ausgabegrenze); 08 AI-020
  (Kontingentsperre: weitere Einträge des Anbieters im selben Lauf `Quota exceeded` ohne Request).
- **Unklar:** Das Verhalten ist spec-gemäß, aber für den Nutzer ungünstig: Ein ungültiger Schlüssel gilt für jeden Eintrag
  als eigener endgültiger Fehler. Bei 22 Einträgen und einer gleichzeitigen Anfrage: 10 Requests, 300 s Pause, 10 Requests,
  300 s, 2 Requests – der Lauf endet nach gut 600 s mit 22 Requests, die alle mit derselben Ursache scheitern (`d05.md`,
  R3).
- **Möglichkeiten:**
  - (a) Eine Kontoablehnung (401/403 des Anbieters, ohne den Refusal-Fall von AI-005) beendet wie die Kontingentsperre alle
    offenen Einträge dieses Anbieters im selben KI-Lauf sofort (ohne Request, ohne Protokollzeile; Status
    `Error: <HTTP-Code> <Meldung>` bzw. nach Festlegung); mitzuentscheiden ist, ob der Fall dann wie Spend-Cap-429 und
    OpenRouter-402 („beenden bereits alle offenen Einträge“) nicht für den Schutzschalter zählt. Vorschlag des Orchestrators. Folge: Nachtrag in 08 AI-017/AI-020, Umsetzung N01
    (Auflage B20), Anpassung des D05-Tests.
  - (b) Wie die Spec belassen (Schutzschalter begrenzt die Zahl der Requests, der Lauf dauert entsprechend).
- **Empfehlung:** keine eigene; die Entscheidung liegt beim Orchestrator. Bei (a) zu klären: ob 403 ohne `refusal` bei
  allen Anbietern eine Kontoablehnung ist (Fehlerfälle je Anbieter in AI-002, AI-003, AI-005).
- **Paket:** Spec (Planer nach Entscheidung), N01 (B20). **D05:** nein (gebaut und abgenommen).
