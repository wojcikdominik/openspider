# M4-K04 – Unabhängige Abnahmetests (Schlüssel, Katalog, Modellwahl)

Testagent ohne Bauer-Kontext; Erwartungen nur aus `docs/spec/08-ai.md` (AI-009, AI-010, AI-011 samt Nachträgen „24.11.2026, Q00“, AI-012-Nachtrag nur Datenwerte), `docs/spec/04-custom-integrations.md` (INTEG-002, Nachtrag INTEG-162), `docs/plan/M4.md` (K4-12, E-4 b, E-7 a, Auftragskern K04) und `M4-packages.json` (`prompt_notes` K04). Gelesen vom Produkt nur die öffentliche Schnittstelle (`os-ai::keys`, `os-ai::catalog`, Adapter-Modelltypen) und `resources/ai/*`.

## Stand und Läufe

- Zweig `ai/k04-tests`, Bauer-Stand `ai/k04` = `f43096a6` (gemergt), letzter Testcommit siehe Abschlussnachricht.
- Mac mini, unter `heavy.lock`, eigenes `CARGO_TARGET_DIR`; Log `acc-log/run3.log`, Prüfzeile `TESTER=f1834fa3 BAUER=f43096a6`.
- `cargo fmt --all --check` grün; `cargo clippy --locked -p os-ai --all-targets -- -D warnings` grün; `cargo check --locked --workspace --all-targets` grün.

| Testziel | grün | rot | ignoriert |
|---|---|---|---|
| `acceptance_k04_keys` (AI-009, INTEG-002 [M4]) | 19 | 0 | 0 |
| `acceptance_k04_catalog` (AI-010) | 19 | 0 | 0 |
| `acceptance_k04_choice` (AI-011) | 24 | 0 | 0 |
| `acceptance_k04_data` (AI-011, AI-012 [M4], INTEG-162 [M4]) | 11 | 1 | 0 |
| **Summe** | **73** | **1** | **0** |

Fixtures: `tests/fixtures/ai/k04-acceptance/` (Anthropic-Modelle A–D und OpenRouter M-R1/M-R2/M-R3/Grenze 4000 nach den Mock-Erweiterungen in 08, Abnahme-Preisdatei mit Eintrag `verified_at` vor 200 Tagen, gefüllte Standardliste `task-defaults-filled.json`, ungültige Preisdatei). Der Schlüsselspeicher ist eine Test-Implementierung von `SecretStoreMut` (`RecordingStore`: protokolliert Zugriffe, sperrbar); kein echter Schlüsselbund, keine echten Anbieter.

## Prüfpunkte je Spec-ID

### AI-009 / INTEG-002 (19 grün)
- Dienst `OpenSpider`, Konten `ai.anthropic`/`ai.openai`/`ai.openrouter`, Variablen `OPENSPIDER_AI_KEY_*`; Ollama ohne Schlüssel.
- „Merken“ an: genau ein Eintrag je Anbieter, Wert Klartext (kein JSON); erneutes Speichern überschreibt; ein von anderer Stelle (04) abgelegter Eintrag wird ohne Eingabe genutzt; nach „Neustart“ (neue Instanz) vorhanden.
- Umgebungsvariable hat Vorrang, Herkunft `Environment` (Anzeige „aus Umgebungsvariable“), wird bei Lesen/Status/Speichern/Löschen nie in den Speicher geschrieben; Löschen lässt sie unberührt.
- Ohne Schlüsselbund (E-7 a): Schlüssel aus Umgebung nutzbar, kein Schreibversuch; ohne Variable `Error: Missing API key`.
- „Merken“ aus: nur Sitzung, kein Schreiben; nach Neustart weg, auch wenn vorher gemerkt.
- Gesperrter Schlüsselbund: beim Speichern verständliche Meldung + Sitzungsschlüssel; beim Lesen Status mit Meldung, Statuswert `Error: Missing API key`; nach Entsperren wieder nutzbar; Löschfehler ohne Schlüssel im Text.
- Löschen aus Sitzung und Schlüsselbund; Löschen eines fehlenden Eintrags ist kein Fehler.
- Länge 128, 129, 200 Zeichen unverändert (Speicher, Neustart, Umgebung), Maske `••••••••` + letzte 4.
- Leerraum am Rand wird entfernt (Auslegung, s. B2-2); leerer Schlüssel abgelehnt, nichts geschrieben.
- Kein Schlüssel (`sk-TEST-AI-009…`, auch nicht dessen Anfang) in `Debug`/`Display` von `AiKeyStore`, `ResolvedKey`, `ApiKeySecret`, `KeyStatus`, `SaveOutcome`, `KeyError`, Adapter-`ApiKey`; nicht im Profil (TOML und JSON von `CrawlConfig`).

### AI-010 (19 grün)
- Ausgelieferte `pricing.json`: gültig gegen `validate_pricing` und gegen `resources/ai/schemas/pricing.schema.json`; jeder Eintrag mit `source_url` und gültigem `verified_at`; `gpt-6-luna` über `responses` (0,10/0,50, Cache-Lesen 0,01, Batch 50 %, Langkontext ab 272.000 × 2 / × 1,5, Cache-Lesen × 2, EU +10 %) und über `decisions` (nur Eingabe) wie in der Spec; `reasoning_efforts` aus der Menge ohne Dubletten; Anthropic-Einträge ohne Fähigkeitsfelder.
- Prüfwerte mit `pricing-test.json`: 200 µUSD (`responses`) / 100 µUSD (`decisions`); Langkontext 60.750 µUSD, EU 66.825 µUSD.
- Kein Rückgriff auf einen anderen Endpunkt, unbekanntes Modell und fremder Anbieter → Preis unbekannt.
- Preisquelle `table`/`api`/`user`; Nutzerüberschreibung vor Tabelle und API, nicht überschriebene Felder bleiben; Nutzerpreis auch für unbekanntes Modell; „Preise zurücksetzen“ stellt Tabelle/API wieder her; ungültige Überschreibung (negativ, Text, leer) abgelehnt.
- „Preis möglicherweise veraltet“: 200 Tage ja, Grenze 120 Tage nein, 121 Tage ja.
- Schema-Negativfälle: fehlende Pflichtfelder (inkl. `endpoint`, `source_url`, `verified_at`), negative Preise und Gebühren, falscher Endpunkt, Preis als Text, falsches/ungültiges Datum, leere Quelle, doppeltes Tripel, kein JSON; zwei Endpunkte eines Modells gültig; Anthropic mit `effort`/`vision`/`capabilities`/`reasoning_efforts`/`max_output_tokens` ungültig; `reasoning_efforts` nur aus der Menge, ohne Dubletten (`off`, `model-default`, `turbo` ungültig); `max_output_tokens` nur positive Ganzzahl.
- `OPENSPIDER_AI_PRICING_FILE` lädt die Testdatei; ungültige oder fehlende Datei → Fehler statt stillem Rückfall.
- Nachtrag: `reasoning_efforts = ["none","low","medium","high"]` → Dropdown genau `off`, `low`, `medium`, `high`, `model-default`; ohne Feld (oder unbekanntes Modell) nur `model-default`; `max_output_tokens` aus dem Katalog; Fähigkeiten je Endpunkt, keine für Anthropic.

### AI-011 (24 grün in `choice`, 7 grün + 1 rot in `data`)
- Standardmodell bei verbundenem Anbieter; nur Anthropic → erstes Anthropic-Modell der Ersatzliste, Ersatzweg (nicht Decisions); nur OpenRouter → OpenRouter-Ersatz; ohne Schlüssel `Error: Missing API key` für alle 12 Aufgaben; Schlüssel da, aber kein geeignetes Modell → `Skipped (no capable model)`; Modell fehlt in Live-Liste → „nicht mehr verfügbar“ und nächster Kandidat; eigene Wahl und eigene Ersatzliste ersetzen den Standard.
- Fähigkeiten: Vision für `suggest-alt`/`decision-vision`, strukturierte Ausgabe für Schema-Aufgaben (C und D nicht zuweisbar), Decisions-Modell nur für Entscheidungsaufgaben, Embedding-Modell für `embedding`; ungeeignete Kandidaten werden in Reihenfolge übersprungen.
- Prüfwerte des Nachtrags: `suggest-title` A → `off`, 1024; B → `low`, 2048, Warnung „aus nicht verfügbar“; W = 14 → A = 288, 576; W = 16 → 640; `score` zählt Levels; `redirect-confirm` gleiche Formel; `ask-sql` auf OpenAI-Katalogmodell → `medium`, 6144 (auch im Request); `audit-summary` auf B → 4000 (Kappung), A → 8192, OpenRouter-Grenze 4000, OpenRouter `null` → keine Kappung (20.480), OpenAI-Katalog `max_output_tokens` 3000.
- Reserve-Tabelle für alle Stufen inkl. `model-default` 4.096; Fallback auf nächstniedrigere bzw. niedrigste Stufe; ohne Stufen `model-default`; Standardstufe je Aufgabentyp laut Tabelle.
- Decisions und Embeddings: keine Stufe, kein `max_tokens` (auch bei gespeicherter Stufe und Nutzerwert).
- Dropdown: A mit `off` … `max`; B ohne `off` und ohne `max`; C und D nur `model-default`; M-R1 `off`/`low`/`medium`/`high`; M-R2 (`mandatory`) ohne `off`; M-R3 nur `model-default`.
- Gespeicherte Stufe nach Modellwechsel: effektive Stufe + Warnung, gespeicherter Wert bleibt; Nutzer-`max_tokens` ersetzt Standard, „knapp“ nur bei kleinerem Wert; „Auf Standard zurücksetzen“ setzt alles zurück.
- Profil-Export/-Import (TOML und JSON von `CrawlConfig`): Modell, Ersatzliste, Stufe, Ausgabegrenze, Temperatur, Eingabekürzung bleiben erhalten; unbekannte Aufgabe oder Stufe beim Import abgelehnt.
- `task-defaults.json`: genau die 12 Aufgabentypen; gültig gegen `validate_task_defaults` und `resources/ai/schemas/task-defaults.schema.json`; Stufen und Antwortanteile entsprechen der Tabelle (`decision-w` für die drei Entscheidungsaufgaben, `embedding` ohne beides); Negativfälle (fehlende/unbekannte Stufe, `answer_tokens` 0/negativ/Text/Bruch, feste Zahl oder unbekannte Formel bei Entscheidungsaufgaben, fehlende/unbekannte Aufgabe, unbekannter Endpunkt) abgelehnt.
- Prüfung „Standardmodell in `pricing.json` für den Endpunkt der Aufgabe“: gefüllte Liste ohne Befund; falscher Endpunkt und ungepreistes Ersatzmodell gemeldet; OpenRouter/Ollama ohne Tabelleneintrag zulässig; ausgelieferte Dateien konsistent.
- Platzhalterzustand: `models_status = pending-confirmation`, alle `model = null`, Ersatzlisten leer; mit selbst gewähltem Modell funktioniert die Aufgabe (Stufe/Grenze aus den ausgelieferten Werten). **Rot:** siehe B1-1.

### AI-012 [M4] (nur Datenwerte, 2 grün)
`decisions_question_overhead_tokens` = 100, Quelle „Live-Probe Q00…“, `verified_at` 2026-11-24; Schema-Zuschlag Anthropic 350, OpenAI Responses 80, OpenRouter `openai/*` 80, sonst 0 (OpenRouter anderes Präfix, Ollama); gültig gegen `resources/ai/schemas/provider-limits.schema.json`; ohne `source_url`/`verified_at`, negativer Wert, ungültiges Datum abgelehnt.

### INTEG-162 [M4] (1 grün)
`chat_prompt_max_tokens`: ohne Grenze 6144, Grenze 8192 → 6144, 6144 → 6144, 4000 → 4000 (Anthropic B, OpenRouter), OpenAI-Katalog 3000 → 3000.

## Befunde

**B1-1 (Abweichung): Platzhalterzustand meldet `Error: Missing API key`, obwohl alle Schlüssel vorhanden sind.** Test `acceptance_k04_data::bundled_placeholder_without_model_and_all_keys_is_not_missing_api_key`.
- Spec AI-011: „Ist kein Modell der Ersatzliste verfügbar, erhalten alle Einträge der Aufgabe `Error: Missing API key` (fehlender Schlüssel) bzw. `Skipped (no capable model)` (Schlüssel da, aber kein geeignetes Modell).“
- Erwartet: Mit der ausgelieferten `task-defaults.json` (keine Standardmodelle) und Schlüsseln für Anthropic, OpenAI und OpenRouter fehlt kein Schlüssel; der Statuswert darf nicht `Error: Missing API key` sein.
- Beobachtet: `plan_task` liefert für alle 12 Aufgaben `MissingApiKey`, weil es ohne Kandidaten keinen Anbieter mit Schlüssel „sieht“.
- Wirkung: Bis zur Bestätigung der Standardmodelle (K4-20) sieht ein Nutzer mit hinterlegten Schlüsseln, der kein eigenes Modell wählt, die irreführende Meldung „Missing API key“ (und der Voranschlag würde die Aufgabe so begründen). Die Spec regelt den Fall „gar kein Kandidat“ nicht ausdrücklich (siehe B2-1); Vorschlag: `Skipped (no capable model)` oder eigener Hinweis „kein Modell gewählt“.

**B2 (Unklarheiten der Spec)**
1. Aufgabe ohne jeden Kandidaten (Standardmodell `null`, leere Ersatzliste, kein Nutzermodell): welcher Statuswert? (Hintergrund von B1-1.)
2. Leerraum am Rand eines eingegebenen Schlüssels bzw. einer Umgebungsvariable: Die Spec sagt nichts; gebaut ist Trimmen, geprüft als Auslegung. Ebenso offen, ob eine **leere** Umgebungsvariable als „gesetzt“ gilt (gebaut: nein, Rückfall auf den Schlüsselbund; nicht geprüft).
3. AI-010 nennt `valid_from` in der Feldliste „je Eintrag“; Schema und ausgelieferte Einträge führen es optional bzw. gar nicht. Pflichtfeld oder optional?
4. Langkontext: „getrennte Faktoren für Eingabe, Ausgabe und Cache-Lesen“ – im Schema ist `cache_read_factor` optional (fehlt → Eingabefaktor), `pricing-test.json` hat keinen. Für Cache-Schreiben im Langkontext (OpenRouter-Hinweis 0,25 USD) gibt es kein Feld.
5. Reihenfolge Sitzungsschlüssel vor Schlüsselbund (gebaut: Umgebung > Sitzung > Schlüsselbund) steht nicht in der Spec; nicht geprüft.

**B3 (Hinweise)**
1. `resources/ai/pricing.json` enthält keine Anthropic-Einträge; jede Kostenschätzung mit Anthropic zeigt damit „Preis unbekannt“ (Budget-Läufe starten nach AI-010 erst nach Preiseingabe). K4-06 verlangt Werte aus der öffentlichen Doku vor dem Release.
2. Herkunft einiger Werte: Laut Notiz der Datei stammen Cache-Schreiben 0,125, `reasoning_efforts` und `max_output_tokens` von `gpt-6-luna` aus der OpenRouter-Liste der Live-Probe; der Nachtrag zu AI-010 verlangt für Cache-Schreiben die öffentliche OpenAI-Preisseite. `source_url` der Einträge verweist auf die Spec, nicht auf eine Anbieterseite. Nicht automatisiert prüfbar; vor dem Release abgleichen (AI-025, Q02).
3. `provider-limits.json` führt nur die Q00-Werte; Batch-Grenzen (AI-016) und TPM-Standards (AI-017) fehlen – nicht K04-Umfang, Besitz klären.
4. `pricing-test.json` (T01) hat weiterhin keinen Eintrag mit `verified_at` vor 200 Tagen; die Abnahme nutzt eine eigene Fixture.

## Nicht geprüft (mit Grund)
- INTEG-001-Dialog und Regressionstestsatz für OpenRouter, Anzeige „aus Umgebungsvariable“, Hinweis „Fähigkeiten unbekannt“, Warntexte im Aufgabendialog, Drag & Drop der Ersatzliste: Oberfläche (U01).
- „Verbinden“ (ein Test-Request, 30 s, 401/403), Kontostand OpenRouter: Adapter/U01.
- Kostenvoranschlag „Preis unbekannt“/„nicht ausführbar“, Budget-Start erst nach Preiseingabe, Kostenspalte und `Endpoint` im Protokoll, `<Q> Source = self-reported`, Crawlstart trotz fehlendem Schlüssel: K10/D01/U01.
- Request-Bodies (`thinking`, `output_config.effort`, `reasoning`) gegen den ai-mock: K09; hier nur effektive Stufe und `max_tokens` im gemeinsamen Request.
- Dateisystem-Scan nach `sk-TEST-AI-009` über Crawl-Datei, Exporte, Logs, Absturzberichte: keine Lauf-/Exportpfade in K04; geprüft sind Debug/Display/Fehlertexte und das Profil. `os-ai::keys` schreibt keine Logs.
- Gemeinsame Nutzung mit 04 (INTEG-160) im Code von `os-integrations`: dort noch keine OpenAI-Prompts; geprüft nur, dass ein unter `ai.openai` abgelegter Eintrag genutzt wird.
- Echter Schlüsselbund und echtes Linux/Docker-Verhalten: nur über Test-Speicher (gesperrt/nicht verfügbar) nachgestellt.
- Export mit Schlüsseln (AES-256-GCM, OUT-201/164): nicht K04.
