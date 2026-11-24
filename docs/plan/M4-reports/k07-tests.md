# M4-K07 – Unabhängige Abnahmetests (AI-018, AI-024)

Stand: 24.11.2026, Zweig `ai/k07-tests` auf Bauer-Stand `e901d35c`. Erwartungen nur aus `docs/spec/08-ai.md`
(AI-018, AI-024; AI-015 Cache-Schlüssel, AI-020 Statuswert, AI-001 Anbieterliste) und den Festlegungen des
Orchestrators. Bericht, Tests und Fixtures des Bauers wurden nicht gelesen; Produktionscode nur für die öffentliche
Schnittstelle.

## Dateien

- `crates/os-ai/tests/acceptance_k07_consent.rs` – AI-018 Einwilligung, Datenflussübersicht, Dialoginhalt (13 Tests)
- `crates/os-ai/tests/acceptance_k07_policies.rs` – AI-018 Datendatei `resources/ai/provider-policies.json` (3 Tests)
- `crates/os-ai/tests/acceptance_k07_redaction.rs` – AI-024 Schwärzung (15 Tests)
- `crates/os-ai/tests/acceptance_k07_support/mod.rs` – Test-Adapter zum ai-mock, Feld-Extraktor für Testseiten
- `tests/fixtures/ai/k07-acceptance/field-markers.html` – ein eindeutiger Marker je Feld (`MARK-TITLE` … `MARK-IMGCTX`,
  Bild-Base64 von `MARK-IMAGE`)
- `tests/fixtures/ai/k07-acceptance/contact-data.html` – erfundene Kontaktdaten in allen sechs Textfeldern und in
  `alt`; IBANs selbst nach ISO 13616 berechnet (gültig: `DE77 1001 0010 0123 4567 89`, `AT16 1234 5000 1234 5678`,
  `NL91ABNA0417164300`; falsche Prüfziffer: `DE78 1001 0010 0123 4567 89`, `XY12 3456 7890 1234 567`)

Aufbau: Die Produktiv-Adapter sind in diesem Stand Stubs (M4-K02). Die Tests senden daher über den Wächter
`os_ai::policy::guard::execute` und einen Test-Adapter, der Anweisungen und Eingabeteile unverändert als JSON an den
ai-mock (`AiMock::fixtures()`, Route OpenAI bzw. Anthropic) schickt. Das Request-Log des Mocks gilt als „was die
Maschine verlassen hat“. Die HTML-Aufbereitung der Felder gehört nicht zu `os-ai`; ein kleiner Extraktor im Test
liefert die Feldwerte (Title, Meta Description, H1, alle H2, `<p>` des Inhalts als Seitentext, erstes Bild mit `alt`,
`<figcaption>` als Bildumfeld).

## Kriterium → Test → Ergebnis

### AI-018

| Kriterium | Test | Ergebnis |
|---|---|---|
| K1 Ohne Bestätigung kein Request, `Pending (consent)`, Mock-Zähler 0 | `ai018_k1_ohne_bestaetigung_kein_request_und_pending_consent` | grün |
| K1 (Schlüssel je Anbieter und Aufgabentyp) | `ai018_k1_einwilligung_gilt_nur_fuer_ihren_anbieter_und_aufgabentyp` | grün |
| K2 Title-Vorschläge + Seitentext → Dialog erneut vor dem nächsten Request | `ai018_k2_title_vorschlaege_mit_seitentext_fragt_vor_dem_naechsten_request_erneut` | grün |
| K2 Festlegung: Teilmenge gedeckt, Modellwechsel fragt nicht neu | `ai018_k2_teilmenge_gedeckt_und_modellwechsel_fragt_nicht_neu` | grün |
| K3 Angezeigte Felder = gesendete (Feldmarker, 14 Einzelfelder + 4 Kombinationen, Bilddaten) | `ai018_k3_angezeigte_felder_entsprechen_exakt_den_gesendeten` | grün |
| K4 Widerruf stoppt sofort alle offenen Aufgaben des Anbieters | `ai018_k4_widerruf_stoppt_sofort_alle_offenen_aufgaben_des_anbieters` | grün |
| K4 (Einwilligung/Widerruf überdauern Neustart) | `ai018_k4_einwilligung_und_widerruf_bleiben_gespeichert` | grün |
| K5 Schwärzung an + `page_text` → `on` ohne Hinweis; aus → `off` mit Hinweis | `ai018_k5_dialog_zeigt_schwaerzung_und_pii_hinweis_nur_bei_aus` | grün |
| K5 Hinweis genau bei `page_text`/`h2`/`image_context` und Schwärzung aus | `ai018_k5_pii_hinweis_genau_bei_page_text_h2_oder_image_context` | grün |
| K6 (Bibliotheksteil) ohne `--ai-consent` keine Aufgaben, Meldung vorhanden | `ai018_k6_cli_ohne_schalter_keine_ki_aufgaben_und_meldung` | grün |
| K6 (Bibliotheksteil) `--ai-consent` ersetzt den Dialog | `ai018_k6_cli_mit_schalter_ersetzt_den_dialog` | grün |
| Ausgabe: Spalten in Spec-Reihenfolge, Host aus Endpunkt, Region, Max Chars, Consent Given At, CSV-Kopf | `ai018_ausgabe_datenflusstabelle_spalten_und_csv_export` | grün |
| Beschreibung „protokolliert“: Schlüssel, Zeit, gezeigte Zeile | `ai018_einwilligung_wird_mit_gezeigten_angaben_protokolliert` | grün |
| Datendatei: alle Anbieter aus AI-001, Trainingsangabe, Zusammenfassung, Prüfdatum | `ai018_provider_policies_jeder_anbieter_hat_trainingsangabe_und_pruefdatum` | grün |
| Datendatei: „mit Quelle“ je Anbieter | `ai018_provider_policies_jeder_anbieter_hat_quelle` | **rot (B1)** |
| Spalte `Training Use` zeigt Angabe, Quelle, Prüfdatum | `ai018_datenfluss_zeigt_trainingsangabe_mit_quelle_und_pruefdatum` | grün |

### AI-024

| Kriterium | Test | Ergebnis |
|---|---|---|
| K1 `max@example.com`, `+49 30 1234567`, gültige IBAN → 3 Platzhalter, `Redactions = 3` | `ai024_k1_drei_werte_werden_zu_drei_platzhaltern_redactions_3` | grün |
| K2 Fixture-Site Kontakt + Impressum, Standard → Volltextsuche im Mock-Log 0 Treffer | `ai024_k2_fixture_site_kontakt_und_impressum_volltextsuche_0_treffer` | grün |
| K3 IBAN-Format mit falscher Prüfziffer wird nicht ersetzt (auch nicht als Telefonnummer) | `ai024_k3_iban_format_mit_falscher_pruefziffer_wird_nicht_ersetzt` | grün |
| K4 schwärzen = aus → Originalwerte, Dialog `off` mit Hinweis | `ai024_k4_schwaerzen_aus_sendet_originale_und_dialog_zeigt_off_mit_hinweis` | grün |
| K5 ungültige eigene Regex beim Speichern abgelehnt (Zeile, Position) | `ai024_k5_ungueltige_eigene_regex_wird_mit_zeile_und_position_abgelehnt` | grün |
| Standard an (OS-Festlegung) | `ai024_schwaerzung_ist_standardmaessig_an` | grün |
| Alle sechs Textfelder, je Art | `ai024_alle_sechs_textfelder_werden_geschwaerzt` | grün |
| Alle sechs Textfelder Ende zu Ende (eigene Kontaktseite, 16 Ersetzungen, `alt` nicht im Feldsatz) | `ai024_alle_sechs_textfelder_ende_zu_ende_auf_eigener_kontaktseite` | grün |
| Mehrere Treffer je Feld, Summe über Felder | `ai024_mehrere_treffer_je_feld` | grün |
| Telefon-Schreibweisen (14 übliche Formen) | `ai024_telefonnummern_in_ueblichen_schreibweisen` | grün |
| IBAN mit/ohne Leerzeichen, mit Buchstaben im BBAN | `ai024_ibans_mit_und_ohne_leerzeichen` | grün |
| Cache-Schlüssel über den geschwärzten Text (AI-015) | `ai024_cache_schluessel_basiert_auf_geschwaerztem_text` | grün |
| Festlegung: eigene Muster → `[REDACTED]`, gezählt; Leerzeilen ignoriert | `ai024_eigene_muster_werden_ersetzt_und_gezaehlt` | grün |
| Festlegung: `alt` wird nicht geschwärzt | `ai024_alt_wird_nicht_geschwaerzt` | grün |
| Schwärzen aus: auch eigene Muster nicht angewandt | `ai024_schwaerzen_aus_laesst_alle_felder_unveraendert` | grün |

## Befunde (rot)

**B1 – `provider-policies.json`: Ollama ohne Quelle.** Spec AI-018, Beschreibung: Angabe zur Trainingsnutzung „aus
`resources/ai/provider-policies.json` mit Quelle und Prüfdatum“. Erwartet: jeder Anbieter aus AI-001 hat eine
Quellen-URL. Beobachtet: `ollama.source_url = null` (Zusammenfassung begründet es mit dem lokalen Server). Die Spalte
`Training Use` zeigt für Ollama dann nur Angabe und Prüfdatum. Schwere gering. Abhilfe: entweder eine Quelle
eintragen (z. B. die Ollama-Dokumentation zu Datenverarbeitung/FAQ) oder in der Spec festhalten, dass lokale Anbieter
ohne Quelle auskommen; dann wird der Test angepasst.

## Hier nicht prüfbar

- AI-018 K6, CLI-Teil: dass die CLI (AI-022) den Schalter `--ai-consent` auswertet und die Meldung tatsächlich in der
  Ausgabe des Laufs erscheint. Geprüft ist nur die Bibliothek (`ConsentGate::cli`, `cli_message`).
- AI-018 Dialog „Datenfluss bestätigen“, Übersicht „KI-Datenfluss“, Widerruf-Bedienung: Oberfläche (M4-U01). Geprüft
  sind nur die Inhalte (`ConsentRequest`, `DataFlowRow`, CSV).
- AI-018 K4 „offene Aufgaben“ im echten Lauf (Warteschlange, Worker): geprüft ist, dass jeder Request nach dem
  Widerruf über ein gemeinsam genutztes Tor `Pending (consent)` erhält; die Warteschlange selbst baut D01.
- AI-024 „Anzahl Ersetzungen je Request im Protokoll (`Redactions`)“: geprüft ist der Wert je Request
  (`GuardedOutcome::redactions`); dass er in die Protokollzeile (AI-014) übernommen wird, entscheidet der Aufrufer (D01).
- AI-024 K2 über einen echten Crawl der Fixture-Site: die Seiten werden hier aus den HTML-Dateien mit einem
  Test-Extraktor aufbereitet, nicht vom Crawler.
- AI-024 K5 „beim Speichern“ in der Oberfläche: geprüft ist die Ablehnung durch `PatternList::parse` und
  `RedactionSettings::from_app_settings`.

## Auslegungsfragen

1. **Position bei ungültiger Regex:** Die Spec verlangt „Zeile und Position“, legt die Zählweise aber nicht fest.
   Getestet wird nur Zeile genau und Position im Bereich 1…Länge+1. Beobachtet: 1-basierte Zeichenposition der
   Fehlerstelle laut Regex-Engine (bei `foo(bar` Position 8 = Zeilenende, bei `*a` Position 1).
2. **Zeilenzählung mit Leerzeilen:** Getestet ist, dass Leerzeilen mitgezählt werden (die Zeile, die der Nutzer
   sieht). Das ist eine Auslegung der Festlegung „Leerzeilen werden ignoriert“.
3. **Quelle bei lokalen Anbietern** (siehe B1).
4. **Persistenz der Einwilligung:** Die Spec sagt nicht ausdrücklich, dass Einwilligungen gespeichert werden; „vor dem
   ersten Lauf“ legt es nahe. Getestet nur mit dem mitgelieferten `MemoryConsentStore`.

## Beobachtungen (nicht hart getestet, Spec legt es nicht fest)

Mit einer einmaligen Sonde (nicht committet) auf `Redactor::default()`:

- **Teilweise Schwärzung bei IBANs in Sonderschreibweise:** `de77 1001 0010 0123 4567 89` (klein),
  `DE77-1001-0010-0123-4567-89` (Bindestriche) und `DE77  1001 …` (doppeltes Leerzeichen) werden nicht als IBAN
  erkannt; der Rest ab `0010` wird als `[PHONE]` ersetzt, `de77 1001` bleibt im Text. Es geht also ein Teil der IBAN
  hinaus. Empfehlung: Kleinbuchstaben und Bindestriche als Trenner mit prüfen.
- **Telefonnummern, die stehen bleiben:** `030 / 1234567` (Schrägstrich mit Leerzeichen, verbreitet),
  `+49.30.1234567` (Punkte), `0 30 1234567`, nackte Ortsnummer `1234567`.
- **Treffer, die keine Telefonnummern sind** (Spec schützt nur IBAN-förmige Zeichenfolgen): `Artikel 0815 4711 22`,
  `Bestellung 012345678`, `GTIN 04012345678901` werden zu `[PHONE]`. Daten (`24.11.2026`, `2026-11-24`), Uhrzeiten,
  Preise, PLZ, ISBN bleiben unverändert.
- **E-Mail:** Großschreibung und Umlaute im lokalen Teil werden ersetzt; `max(at)example.com`, `max@localhost` und
  IDN-Domains (`max@bücher.example`) nicht. Bei `mailto:…?subject=x` wird nur die Adresse ersetzt.
- Ungültige IBAN mit angehängtem Buchstaben (`… 89X`) bleibt vollständig stehen (keine Ersetzung als Telefonnummer).

## Testzahlen des letzten Laufs

Mac mini, `cargo test --locked -p os-ai --no-fail-fast`: 258 grün, 1 rot (`ai018_provider_policies_jeder_anbieter_hat_quelle`),
34 ignoriert (fremde Platzhalter). Davon diese Abnahme: 31 Tests, 30 grün, 1 rot.
`cargo clippy --locked -p os-ai --all-targets -- -D warnings` grün; `cargo fmt --all --check` grün.
