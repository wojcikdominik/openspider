# M4-K4-20 – Unabhängige Abnahmetests (bestätigte Standardmodelle, Preise)

Testagent ohne Bauer-Kontext. Grundlage nur `docs/spec/08-ai.md` auf dem Stand von `ai/k4-20` (`df6cdebf`): AI-011 „Nachtrag 25.11.2026 – bestätigte Standardmodelle“ (Tabelle, Punkte, Kriterien, „Berichtigung 25.11.2026“), AI-011 „Nachtrag 25.11.2026 – Aufgabe ohne Kandidaten“, AI-010 „Nachtrag 25.11.2026 – Preise der Decisions API“ und Nachtrag nach K04 (`valid_from`, Schemaort), AI-025 (Prüfdatum). Nicht gelesen: Bauer-Bericht `k4-20.md`, `model_choice_confirmed.rs` und weitere Tests des Bauers.

## Stand und Läufe

- Zweig `ai/k4-20-tests` von `ai/k4-20` (`df6cdebf`); Tests `b6e8ed95`, Bericht im Folgecommit. Nicht gepusht.
- Mac mini, `heavy.lock`, eigenes `CARGO_TARGET_DIR`, Log `acc-log/run1.log`, Prüfzeile `TESTER=b6e8ed95 BAUER=df6cdebf`.
- `cargo fmt --all --check` grün; `cargo clippy --locked --workspace --all-targets -- -D warnings` grün.
- `cargo test --locked -p os-ai -p os-app -p os-parity --no-fail-fast`: **1.602 bestanden, 0 rot, 63 ignoriert** (inkl. der K04-Abnahmetests; der dort rote Test zum Platzhalterzustand ist jetzt grün).

| Testziel | grün | rot | ignoriert |
|---|---|---|---|
| `acceptance_k4_20_defaults` (AI-011) | 12 | 0 | 0 |
| `acceptance_k4_20_pricing` (AI-010, AI-025) | 9 | 0 | 0 |
| **Summe** | **21** | **0** | **0** |

Fixtures `tests/fixtures/ai/k4-20-acceptance/`: Anthropic-Liste mit `claude-haiku-5-5`/`claude-sonnet-5-5` (Form AI-002), OpenRouter-Liste mit `openai/gpt-6-luna` (Form AI-005), `task-defaults-no-models.json` (ausgelieferte Stufen/Antwortanteile, alle Modelle `null`, Ersatzlisten leer). OpenAI-Modelle und ihre Fähigkeiten aus der ausgelieferten `pricing.json` (AI-010).

## Prüfpunkte

**AI-011, bestätigte Liste**
- Ausgelieferte `task-defaults.json` = Tabelle für alle 12 Aufgaben (Standard und Reihenfolge der Ersatzliste), `models_status = confirmed`.
- Jedes genannte Nicht-OpenRouter-Modell hat in `pricing.json` einen Eintrag für den genannten Endpunkt mit `source_url` (Anbieterseite `https://…`) und gültigem `verified_at`; `check_against_pricing` ohne Befund.
- Jedes Standard- und Ersatzmodell ist seiner Aufgabe zuweisbar (Fähigkeiten aus Katalog bzw. Liste).
- Alle Schlüssel → Standardmodell je Aufgabe. Nur OpenAI → `decision` Decisions `gpt-6-luna`, `suggest-title` Responses `gpt-6-luna`, `ask-sql` Responses `gpt-6-sol` (Ersatz 1), `embedding` `text-embedding-3-small`. Nur Anthropic → `decision`/`suggest-title` `claude-haiku-5-5` (Ersatzweg), `ask-sql` `claude-sonnet-5-5`, `embedding` `Error: Missing API key`. Nur OpenRouter → `decision` `openai/gpt-6-luna`; `suggest-title`, `ask-sql`, `embedding` `Error: Missing API key` (Berichtigung). Kein Schlüssel → alle `Error: Missing API key`.

**AI-011, Aufgabe ohne Kandidaten** (gegen `task-defaults-no-models.json`)
- Schlüssel für alle drei bzw. für jeweils nur einen Anbieter → alle 12 Aufgaben `Skipped (no capable model)`; ohne jeden Schlüssel → `Error: Missing API key`.
- Mit Modellwahl im Profil gilt das Grundkriterium (Plan mit Schlüssel; ohne Schlüssel des gewählten Anbieters `Error: Missing API key`, auch wenn ein anderer Schlüssel da ist).

**AI-010, Decisions-Preise**
- Eintrag `openai/gpt-6-luna/decisions`: Eingabe 0,10 wie `responses`, Ausgabe 0, Cache-Lesen und Cache-Schreiben gleich `responses` (beide > 0), Langkontext identisch (Schwelle 272.000), kein `batch_discount`, Quelle `https://…`, Datum gültig.
- Kriterium 1: 1.000.000 Eingabe, davon 400.000 gecacht → 64.000 µUSD vor Langkontext-Faktor; nicht 100.000 µUSD pauschal; mit Faktor (Eingabe und Cache-Lesen × 2) 128.000 µUSD (siehe B2-1).
- Kriterium 2: 300.000 Eingabe → 60.000 µUSD (Faktor 2), Ausgabe 0 auch bei 1.000 Ausgabe-Tokens; unter der Schwelle 200.000 → 20.000 µUSD; gleicher Eingabeanteil wie `responses`.
- Cache-Schreiben: 100.000 Eingabe (20.000 gelesen, 30.000 geschrieben) kostet über `decisions` dasselbe wie über `responses` (5.000 + 200 + 3.750 µUSD).
- Grundkriterium mit `pricing-test.json` (1.000 Eingabe über `decisions` → 100 µUSD) gilt weiter.

**AI-010, Datei und Schema**: `pricing.json` gültig gegen `validate_pricing` und `resources/ai/schemas/pricing.schema.json`; jeder Eintrag mit `source_url`/`verified_at`; Eintrag ohne `valid_from` gültig, ohne `source_url` oder `verified_at` ungültig; die drei Datenschemas liegen im Repository je genau einmal unter `resources/ai/schemas/`.

**AI-025**: Alle Preiseinträge und `task-defaults.json` haben am 25.11.2026 ein Prüfdatum von höchstens 120 Tagen und keines in der Zukunft.

## Befunde

Keine B1 (keine Abweichung von der Spec gefunden).

**B2 (Unklarheiten/Widersprüche der Spec)**
1. AI-010-Nachtrag, Kriterium 1: Der Request liegt mit 1.000.000 Eingabe-Tokens über der Langkontext-Schwelle; das Kriterium nennt nur den Betrag „vor Langkontext-Faktor“ und keinen Endbetrag. Getestet: 64.000 µUSD ohne Faktor, 128.000 µUSD mit Faktor 2 auf Eingabe und Cache-Lesen. Offen ist auch, ob die Schwelle an allen Eingabe-Tokens (inkl. gecachter) gemessen wird.
2. AI-004 (Beschreibung) sagt weiter „keine Kosten für Ausgabe, Cache-Lesen oder Cache-Schreiben“, AI-010 (Beschreibung) „über `decisions` nur Eingabe 0,10, alle anderen Positionen 0“. Der Nachtrag an AI-010 ersetzt das sinngemäß, AI-004 ist aber nicht nachgetragen; Widerspruch zwischen den Einträgen.
3. `redirect-confirm` steht in der Tabelle bei den Vorschlagsaufgaben (Standard Responses `gpt-6-luna`, Ersatz nur `claude-haiku-5-5`), nutzt aber die Entscheidungsformel `decision-w` und ist eine Decision-Vorlage (AI-082). Damit läuft es nie über die Decisions API und hat keinen OpenRouter-Ersatz. Gewollt? (Getestet wie die Tabelle.)
4. Kriterium „jeweils 0 Requests“ (Aufgabe ohne Kandidaten): Die Modellwahl ist eine reine Funktion ohne Netz; die Requestzahl lässt sich auf dieser Ebene nicht messen (erst D01 mit ai-mock).

**B3 (Hinweise)**
1. Preiswerte gegen die Anbieterseiten nicht geprüft (kein Netzzugriff in der Abnahme). Auffällig zum Abgleich: `claude-haiku-5-5` mit denselben Grundpreisen wie `gpt-6-luna` (0,10/0,50) und Langkontext ab 100.001 Tokens mit Faktor 5 auf Eingabe, Ausgabe und Cache-Lesen.
2. Der Regionalzuschlag EU 10 % steht auch beim Decisions-Eintrag; der Nachtrag nennt ihn nicht, AI-004 verweist auf die Preistabelle – konsistent, nur nicht ausdrücklich.
3. Die OpenRouter-Fixture nimmt Bild-Eingabe für `openai/gpt-6-luna` an (für `decision-vision`); in der echten Liste vor dem Release prüfen.

## Nicht geprüft
- Request-Zählung und Statuswerte im Lauf gegen ai-mock (D01), Kostenvoranschlag „Preis unbekannt“ (K10/U01).
- Wöchentlicher CI-Check und Release-Check aus AI-025 als Werkzeug (K08); hier nur die Prüfdaten der Datendateien.
- Preiswerte gegen die öffentliche Preisdokumentation der Anbieter (B3-1).
