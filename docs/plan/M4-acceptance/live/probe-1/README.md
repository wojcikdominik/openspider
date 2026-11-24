# M4-Q00 – Frühe Live-Probe der KI-Anbieter (Probe 1)

Paket M4-Q00 (Welle 3p, E-8 b, E-3 a). Stand des Rahmens: 24.11.2026. Die Spalte „beobachtet“ füllt der Orchestrator
nach dem Live-Lauf aus `probe/results/*.json`.

## Zweck

`docs/spec/08-ai.md` beschreibt die Anbieter-APIs auf dem Stand vom 13.10.2026. Die Probe stellt mit wenigen,
billigen echten Aufrufen fest, wo Anthropic (Messages), OpenAI (Responses und Decisions) und OpenRouter davon
abweichen: Endpunkte, Feldnamen, Fehlerformen, Modell-IDs, Preise, Rate-Limit-Header – und ausdrücklich, ob die
**Decisions API** und das Modell **`gpt-6-luna`** noch wie spezifiziert existieren. Danach legt der Orchestrator nach
E-3 a fest, ob Decisions (AI-004, AI-033) oder der Ersatzweg über strukturierte Ausgabe (AI-034) Standardweg ist;
Spec-Abweichungen gehen als Vorschlag an den Planer (08 „Vor Umsetzung zu verifizieren“).

Jede Anfrage läuft einmal **roh** (eigener `reqwest`-Aufruf nach dem Wortlaut von 08, damit Feldnamen und
Fehlerkörper unverfälscht sichtbar sind) und – wo es einen Adapter gibt – einmal **über den Adapter aus `os-ai`**
(`crates/os-ai/src/adapters/{anthropic,openrouter,openai_responses,openai_decisions}`), um zu sehen, ob der Adapter mit
der echten Antwort zurechtkommt. Die Anfragen sind nicht an die aktuelle Anbieterdokumentation angepasst (Clean-Room;
die Probe prüft den Spec-Stand). EU-Endpunkte werden nicht angesprochen.

## Aufbau

```
probe-1/
  README.md               dieser Rahmen
  probe/                  eigenständiges Rust-Programm (nicht Teil des Haupt-Workspace)
    Cargo.toml            leere [workspace]-Tabelle; Abhängigkeiten wie [workspace.dependencies] der Wurzel
    Cargo.lock            gekürzte Wurzel-Cargo.lock (keine neueren Versionen)
    src/                  cli, plan (Anfrageplan), requests (Körper), runner, budget, record, secret, png
    tests/                Tests gegen einen lokalen Testserver (127.0.0.1), ohne Schlüsselbund
    results/              entsteht im Live-Lauf: <anbieter>.json, summary.json (wird bewusst committet)
```

## Bedienung

Alle Befehle im Ordner `docs/plan/M4-acceptance/live/probe-1/probe`. Rust-Toolchain kommt aus der
`rust-toolchain.toml` der Wurzel.

### 1. Trockenlauf (Standard; kein Netzwerk, kein Schlüsselbund)

```sh
cargo run --locked -- --dry-run
```

Gibt je Anbieter die Liste der Anfragen aus (Weg roh/Adapter, Methode, URL, Kostenobergrenze, Zweck, Schritt-ID).
Ohne `--live` **oder** ohne `OPENSPIDER_LIVE_API_TESTS=1` läuft immer nur der Trockenlauf.

### 2. Schlüssel hinterlegen (Nutzer, einmalig)

Eigene Testschlüssel unter dem Dienst `OpenSpider-Probe` (K4-12, Nachtrag 24.11.2026); ersatzweise liest die Probe die
App-Einträge unter `OpenSpider`. Konto `ai.<anbieter>`, Wert = Schlüssel als Klartext:

```sh
security add-generic-password -s OpenSpider-Probe -a ai.anthropic -w
security add-generic-password -s OpenSpider-Probe -a ai.openai -w
security add-generic-password -s OpenSpider-Probe -a ai.openrouter -w
```

`-w` als letzte Option fragt den Schlüssel verdeckt ab (er landet nicht in der Shell-Historie). Fehlt ein Eintrag,
überspringt die Probe diesen Anbieter und vermerkt das in `results/<anbieter>.json`. Nach der Probe löscht der Nutzer
die Einträge selbst: `security delete-generic-password -s OpenSpider-Probe -a ai.<anbieter>`.

**macOS fragt beim ersten Zugriff** des Programms auf jeden Eintrag nach („… möchte auf vertrauliche Informationen …
im Schlüsselbund zugreifen“), weil der Eintrag mit `security` angelegt wurde und das Probe-Programm nicht in seiner
Zugriffsliste steht. „Erlauben“ genügt für den Lauf; „Immer erlauben“ gilt nur für genau dieses Binärprogramm (nach
einem Neubau fragt macOS erneut). Ist der Schlüsselbund gesperrt, fragt macOS nach dem Anmeldepasswort.

### 3. Live-Lauf

Empfohlen in zwei Schritten – zuerst nur die kostenlosen Abfragen (Modelllisten, OpenRouter-Kontostand), dann der
volle Plan:

```sh
OPENSPIDER_LIVE_API_TESTS=1 cargo run --locked -- --live --models-only
# results/*.json ansehen (Modell-IDs, gpt-6-luna gelistet?), dann:
OPENSPIDER_LIVE_API_TESTS=1 cargo run --locked -- --live
```

Optionen: `--only anthropic|openai|openrouter`, `--max-total-usd <betrag>` (nur nach unten, höchstens 6),
`--model-anthropic <id>`, `--model-openai <id>`, `--model-openrouter <id>` (die ID muss in der jeweiligen Modellliste
stehen; sonst werden die modellabhängigen Anfragen übersprungen). Jeder Lauf überschreibt die Dateien der gelaufenen
Anbieter und `summary.json`.

Bauzeit und Platz (Mac mini M1, 16 GB, kalter Bau des Probe-`target`, gemessen 24.11.2026): siehe Abschnitt
„Messwerte“ unten.

## Kostenbremse

- Konstanten in `src/plan.rs`: je Anbieter höchstens **2 USD** geschätzte Kosten und eine feste Höchstzahl an
  HTTP-Anfragen (Anthropic 14, OpenAI 18, OpenRouter 14; der Plan liegt darunter), gesamt höchstens **6 USD**
  (`--max-total-usd` kann nur senken).
- Vor jeder Anfrage wird ihre **Obergrenze** geprüft (Eingabe ⌈Zeichen/2⌉ + 64 Tokens, je Bild 1.600 Tokens, volle
  Ausgabegrenze); passt sie nicht mehr unter die Grenze, bricht der Anbieter ab, alle übrigen Schritte werden als
  übersprungen aufgezeichnet. Nach jeder Anfrage wird die geschätzte Ist-Kostenzahl aus der gemeldeten `usage`
  verbucht (ohne lesbare `usage`: die Obergrenze); ist eine Grenze erreicht, bricht der Anbieter ab.
- Preise: Anthropic und OpenAI liefern über ihre Modelllisten keine Preise (AI-002, AI-003); die Probe rechnet deshalb
  vorsichtig mit **30 USD je 1 Mio. Eingabe- und 150 USD je 1 Mio. Ausgabe-Tokens**. Die Kostenzahlen der Probe sind
  damit Obergrenzen, keine Abrechnung; maßgeblich ist das Dashboard des Anbieters. OpenRouter: Preise des gewählten
  Modells aus der Liste; gewählt werden nur Modelle mit Tokenpreisen unter der vorsichtigen Annahme und Gebühren je
  Request/Bild ≤ 0,01 USD.
- Kleine Grenzen: `max_tokens`/`max_output_tokens` 128 (Text, Bild) bzw. 256 (strukturiert); Eingaben wenige Sätze;
  ein Testbild 32 × 32 PNG (im Code erzeugt). Keine Wiederholungen (auch nicht bei 429/5xx; der HTTP-Client hat
  `retry::never()`), keine Batch-Aufträge, keine Bildgenerierung, keine Datei-Uploads.
- Summe der Obergrenzen des vollen Plans laut Trockenlauf: siehe „Messwerte“. Tatsächlich zu erwarten: Cent-Beträge.

## Schlüsselschutz

- Schlüssel liest nur der Live-Modus (`KeyringSource`); der Trockenlauf hat keinen Zugang zu einer Schlüsselquelle.
- Typ `ApiSecret`: `Debug` zeigt `ApiSecret([REDACTED])`, kein `Display`, kein `Serialize`, kein `Clone`.
- Aufgezeichnet werden keine Authentifizierungs-Header (nur der Vermerk „x-api-key (nicht aufgezeichnet)“ bzw.
  „authorization …“); von Antwort-Headern nur die Namen, Werte nur bei Rate-Limit-, Request-ID-, `retry-after`-,
  `content-type`- und `openai-processing-ms`/`openai-version`-Headern.
- Jede Konsolenzeile und jede Ergebnisdatei läuft durch einen Schwärzer (`[REDACTED]` für den Schlüssel und seine
  letzten vier Zeichen, wie Anbieter sie in maskierten Fehlermeldungen zeigen); das Feld `data.label` von
  OpenRouter `/key` wird ausgelassen. Fehler des Schlüsselbunds werden nicht im Wortlaut übernommen.
- Belegt durch `tests/probe.rs::full_run_executes_every_planned_step_once_and_never_leaks_the_key`: Der Testserver gibt
  den (erfundenen, nur über eine Testquelle hereingereichten) Schlüssel in einem Header und in Fehlermeldungen zurück;
  weder Konsole noch eine Datei in `results/` enthalten ihn oder seine letzten Zeichen.

## Auslegungen der Probe (wo 08 schweigt)

1. Rohkörper folgen 08; wo 08 den Feldaufbau nicht nennt, wie der Adapter: Anthropic
   `output_config.format = {type, schema}`; OpenAI `input_image.image_url` als Data-URL, `max_output_tokens`;
   OpenRouter `response_format = {type: json_schema, json_schema: {name, strict, schema}}`, Bild als
   `{type: image_url, image_url: {url}}`. Diese Annahmen stehen in der Tabelle als eigene Prüfpunkte.
2. `anthropic-version: 2023-06-01` wie im Adapter (08 nennt den Header, nicht den Wert).
3. Modellwahl (Modell-IDs werden nicht geraten): Anthropic – aus den Modellen, die `structured_outputs` und
   `image_input` als unterstützt melden, das erste mit „haiku“ in der ID, sonst das erste (die Liste hat keine Preise,
   „billigstes“ ist nicht ablesbar). OpenAI Responses – `gpt-6-luna`, falls gelistet (08 AI-010 nennt es mit
   Responses-Preisen), sonst die kürzeste `gpt-`-ID mit „nano“, dann „mini“, ohne Sonderzwecke. OpenAI Decisions –
   immer die Spec-ID `gpt-6-luna` (genau deren Existenz wird geprüft). OpenRouter – je Fähigkeit das billigste Modell
   der Liste (ohne `:free`, ohne Preis 0, weil `data_collection: deny` gesendet wird). Vorgaben per Option.
4. Decisions-Prüfungen über den Mindestumfang hinaus (V-A1): Bildeingabe, 11 Fragen, `instructions` mit 4.000 Zeichen;
   strukturierte Ausgabe an der OS-Grenze (V-A5): 20 Enum-Werte, 2.000 Zeichen Beschreibung.

## Vergleichstabellen „Spec 08 ↔ beobachtet“

Quelle der Beobachtung: `probe/results/<anbieter>.json`, Schritt-ID in Spalte „Schritt“. Spalte „beobachtet“ bleibt
bis zum Live-Lauf leer; „Abw.“ = ja/nein/unklar.

### Gesamtfeststellung (nach dem Lauf ausfüllen)

| Frage | Spec 08 | beobachtet | Folge |
|---|---|---|---|
| Existiert `POST /v1/decisions` wie in AI-004? | öffentliche Beta, Modell `gpt-6-luna` | | E-3 a: Standardweg Decisions oder AI-034 |
| Steht `gpt-6-luna` in `GET /v1/models`? | ja (AI-004, AI-010) | | K4-06, K4-20 |
| Kommt der Decisions-Adapter mit der echten Antwort zurecht? | AI-004-Antwortform | | ggf. N-Paket |
| Kommen die übrigen drei Adapter mit den echten Antworten zurecht? | AI-002/003/005 | | ggf. N-Paket |
| Tatsächliche Kosten laut Dashboards | Cent-Bereich | | – |

### Anthropic Messages (AI-002, AI-017)

| Nr. | Prüfpunkt | Spec 08 (13.10.2026) bzw. Annahme | Schritt | beobachtet | Abw. |
|---|---|---|---|---|---|
| A1 | Endpunkt und Authentifizierung | `POST /v1/messages`, Header `x-api-key`, `anthropic-version`, `content-type: application/json` | `anthropic.raw.text` | | |
| A2 | Modellliste: Felder je Modell | `id`, `display_name`, `max_input_tokens`, `max_tokens`, `capabilities` | `anthropic.raw.models` | | |
| A3 | `capabilities`-Knoten | je Fähigkeit Blatt `supported`: `structured_outputs`, `image_input`, `batch`, `effort` mit `low` … `max` | `anthropic.raw.models` | | |
| A4 | Paginierung der Liste (K01-Annahme) | `has_more`, `last_id`; Parameter `limit`, `after_id` | `anthropic.raw.models`, `anthropic.adapter.models` | | |
| A5 | Preise in der Modellliste | keine (Preistabelle, AI-010) | `anthropic.raw.models` | | |
| A6 | Strukturierte Ausgabe | `output_config.format` mit `type: "json_schema"` und Schema (Feld `schema` = K01-Auslegung); `output_format` wird nicht verwendet | `anthropic.raw.structured` | | |
| A7 | Bildeingabe | Inhaltsblock `image`, Quelle `base64` mit `media_type` | `anthropic.raw.image` | | |
| A8 | `usage` | `input_tokens`, `output_tokens`, `cache_read_input_tokens`, `cache_creation_input_tokens` | `anthropic.raw.text` | | |
| A9 | `stop_reason` | u. a. `end_turn`, `max_tokens`, `refusal` (+ `stop_details`) | `anthropic.raw.*` | | |
| A10 | Fehlerform unbekanntes Modell | HTTP-Status, `error.type`, `error.message` | `anthropic.raw.unknown_model` | | |
| A11 | Rate-Limit-Header | `anthropic-ratelimit-requests-remaining`, `anthropic-ratelimit-input-tokens-remaining` (+ Reset); `anthropic-ratelimit-tokens-remaining` kombiniert, Tausender gerundet | alle `anthropic.raw.*` | | |
| A12 | Request-ID | Header bzw. Antwort-`id` | `anthropic.raw.text` | | |
| A13 | Tokenzählung | `POST /v1/messages/count_tokens` → `input_tokens` (optional, AI-012) | `anthropic.raw.count_tokens` | | |
| V-A3 | 400-Meldungen bei Guthaben/Ausgabengrenze; 402 `billing_error` | Heuristik „You have reached your specified“ / „Your credit balance is too low“ | nicht provoziert (nur sichtbar, falls das Testkonto leer ist) | | |
| V-A5 | Schema an der OS-Grenze | 20 Enum-Werte, 2.000 Zeichen Beschreibung werden angenommen | `anthropic.raw.structured_limits` | | |
| K01-a | Adapter liest die echte Modellliste | `list_models` mit `capabilities`, Modelle ohne `capabilities` = „Fähigkeiten unbekannt“ | `anthropic.adapter.models` | | |
| K01-b | Adapter: Text, Schema (lokale Validierung), Bild | Ergebnis `Ok` mit `usage` | `anthropic.adapter.{text,structured,image}` | | |
| K01-c | Adapter: Fehlerklasse unbekanntes Modell | `Error: <Status> …` (kein Kontingent) | `anthropic.adapter.unknown_model` | | |
| K01-d | Auslegung 9: `max_tokens` Pflichtfeld | Probe sendet immer `max_tokens` | – | | |
| K01-e | Auslegungen 1, 5, 10 (`max_tokens`-Abbruch, Refusal-Grund aus `stop_details`, `effort`) | – | nicht geprüft | – | – |

### OpenAI Responses (AI-003, AI-017)

| Nr. | Prüfpunkt | Spec 08 bzw. Annahme | Schritt | beobachtet | Abw. |
|---|---|---|---|---|---|
| R1 | Endpunkt und Authentifizierung | `POST /v1/responses`, Bearer-Token | `openai.raw.responses_text` | | |
| R2 | Modellliste | `GET /v1/models`, ohne Preise und Fähigkeiten | `openai.raw.models` | | |
| R3 | Strukturierte Ausgabe | `text.format` = `{type: "json_schema", name, schema, strict: true}` | `openai.raw.responses_structured` | | |
| R4 | Bildeingabe | Teil `input_image` mit Data-URL (Feld `image_url` = K02-Auslegung 3) | `openai.raw.responses_image` | | |
| R5 | Ausgabegrenze (K02 offene Frage 6) | Feld `max_output_tokens` | `openai.raw.responses_text` | | |
| R6 | Antwortform | `output[]` → Nachricht → Teile `output_text` bzw. `refusal`; `status` (`incomplete`) | `openai.raw.responses_*` | | |
| R7 | `usage` | `input_tokens`, `output_tokens`, `input_tokens_details.cached_tokens` | `openai.raw.responses_text` | | |
| R8 | Fehlerform unbekanntes Modell | Status, `error.type`, `error.code` | `openai.raw.responses_unknown_model` | | |
| R9 | Rate-Limit-Header, Request-ID | `x-ratelimit-remaining-requests`, `x-ratelimit-remaining-tokens` (+ Reset); `x-request-id` | alle `openai.raw.*` | | |
| V-A5 | Schema an der OS-Grenze | 20 Enum-Werte, 2.000 Zeichen Beschreibung | `openai.raw.responses_structured_limits` | | |
| K02-a | Adapter: Modellliste, Text, Schema, Bild | Ergebnis `Ok`; ohne Katalog keine Fähigkeiten (K02 offene Frage 5) | `openai.adapter.*` | | |
| K02-b | Adapter: Fehlerklasse unbekanntes Modell | `Error: <Status> …` | `openai.adapter.responses_unknown_model` | | |
| K02-c | Auslegung 10: kein `effort`, kein `store` gesendet | wird angenommen | – | | |

### OpenAI Decisions (AI-004, AI-033, V-A1)

| Nr. | Prüfpunkt | Spec 08 bzw. Annahme | Schritt | beobachtet | Abw. |
|---|---|---|---|---|---|
| D1 | Endpunkt existiert | `POST /v1/decisions` (öffentliche Beta) | `openai.raw.decisions_3q` | | |
| D2 | Modell | `gpt-6-luna` in der Liste und vom Endpunkt angenommen | `openai.raw.models` (`facts.gpt_6_luna_listed`), `openai.raw.decisions_3q` | | |
| D3 | Request-Aufbau | `model`, `input` (Text als Zeichenkette = K02-Auslegung 3), `questions[]` mit `name`, `type`, `instructions`, `choices[{value, description}]`, `levels[{label, description}]` | `openai.raw.decisions_3q` | | |
| D4 | Antwort `predicate` | `answers[]` je `name`, `type`; `probability` | `openai.raw.decisions_3q` | | |
| D5 | Antwort `choice` | `choice`, `probabilities[{value, probability}]`, `confidence` | `openai.raw.decisions_3q` | | |
| D6 | Antwort `score` | `score` (gewichteter Mittelwert der Indizes ab 0), `probabilities[{value, label, probability}]`, `confidence` | `openai.raw.decisions_3q` | | |
| D7 | Bedeutung von `confidence` (V-A1) | bis GA = höchste Einzelwahrscheinlichkeit; Abweichung > 0,01 → `confidence_mismatch` | `openai.raw.decisions_3q` | | |
| D8 | `usage` (V-A1) | Format offen; fehlt es → Schätzung | `openai.raw.decisions_3q` | | |
| D9 | Bildeingabe | Nutzer-Nachricht mit `input_text`/`input_image` (Data-URL) | `openai.raw.decisions_image` | | |
| D10 | Grenze Fragen je Request (V-A1) | OS-Grenze 10; was macht die API mit 11? | `openai.raw.decisions_11q` | | |
| D11 | Länge `instructions` (V-A1) | OS-Grenze 2.000; API mit 4.000 Zeichen? | `openai.raw.decisions_long_instructions` | | |
| D12 | Fehlerform, Fehlercodes (V-A1) | offen | `openai.raw.decisions_unknown_model` | | |
| D13 | Rate-Limit-Header (V-A1) | offen | alle `openai.raw.decisions_*` | | |
| D14 | Refusal-Felder über `name`/`type` hinaus (V-A1) | offen | nicht provoziert | | |
| D15 | Preis | nur Eingabe, 0,10 USD je 1 Mio. (über API nicht ablesbar; Dashboard) | – | | |
| K02-d | Adapter wertet die echte Antwort aus | `QuestionResult` je Frage; unbekannte Felder ignoriert; Wertprüfungen (Auslegung 5) | `openai.adapter.decisions_3q` | | |
| K02-e | Auslegung 4: Refusal-Grund aus `refusal`/`reason` | – | nicht geprüft | – | – |

### OpenRouter (AI-005, AI-010)

| Nr. | Prüfpunkt | Spec 08 bzw. Annahme | Schritt | beobachtet | Abw. |
|---|---|---|---|---|---|
| O1 | Basis-URL, Authentifizierung | `https://openrouter.ai/api/v1`, Bearer-Token | `openrouter.raw.text` | | |
| O2 | Modellliste: Felder | `id`, `name`, `context_length`, `pricing`, `supported_parameters` | `openrouter.raw.models` | | |
| O3 | Preisfelder | `pricing.prompt`, `completion` (USD je Token), `request`, `image` als Zeichenketten; fehlend = 0 | `openrouter.raw.models` | | |
| O4 | Fähigkeiten | Structured Output aus `structured_outputs` bzw. `response_format` (K01-Auslegung 4: eines genügt); Vision aus `architecture.input_modalities` (K01-Auslegung) | `openrouter.raw.models` | | |
| O5 | Strukturierte Ausgabe | `response_format` mit `type: "json_schema"` (Verschachtelung `json_schema{name, strict, schema}` = K01) + `provider.require_parameters: true` | `openrouter.raw.structured` | | |
| O6 | Routing-Präferenzen | `provider.data_collection: "deny"` wird angenommen | `openrouter.raw.text` | | |
| O7 | Bildeingabe | `image_url` mit Data-URL | `openrouter.raw.image` | | |
| O8 | `usage` | `prompt_tokens`, `completion_tokens` (Cache-Details, Ist-Kosten?) | `openrouter.raw.text` | | |
| O9 | Kontostand | `GET /key` → `data.limit`, `limit_remaining`, `limit_reset`, `usage`, `usage_daily`, `usage_weekly`, `usage_monthly` | `openrouter.raw.key` | | |
| O10 | Fehlerform unbekanntes Modell | Status, `error.code`, `error.message`, `error.metadata` | `openrouter.raw.unknown_model` | | |
| O11 | Rate-Limit-Header | 08 nennt keine für OpenRouter | alle `openrouter.raw.*` | | |
| V-A4 | Refusal-Formen | `choices[0].message.refusal`/`finish_reason = content_filter`; 403 `error.type = refusal` | nicht provoziert | | |
| V-A5 | Schema an der OS-Grenze | 20 Enum-Werte, 2.000 Zeichen Beschreibung | `openrouter.raw.structured_limits` | | |
| K01-f | Adapter: Modellliste mit Preisen/Fähigkeiten, Kontostand | `OpenRouterModel`, `KeyBalance` | `openrouter.adapter.{models,key}` | | |
| K01-g | Adapter: Text, Schema, Bild, unbekanntes Modell | `Ok` bzw. `Error: <Status> …` | `openrouter.adapter.*` | | |
| K01-h | Auslegung 12: unlesbare Preise (`"-1"`) → „Price unknown“ | Anzahl `prices_unknown` | `openrouter.adapter.models` | | |

## Was die Probe nicht prüft

EU-Endpunkte (AI-019, nur im Mock), Batch (AI-016), Embeddings (AI-008), provozierte Verweigerungen (V-A4, D14,
K01 Auslegung 5), Kontingentmeldungen (V-A3; nur falls ein Testkonto zufällig leer ist), Rate-Limit-Erschöpfung,
Decisions mit 10 und 25 Fragen (V-A1 nennt 1, 10, 11, 25; die Probe deckt 1, 3 und 11 ab), `max_tokens`-Abbruch bei
Schema-Aufgaben, `effort`, Windows-/Linux-Schlüsselbund. Seitenweises Laden der Anthropic-Modellliste zählt der
Adapterschritt als eine Anfrage; ein automatisches `GET /key` des OpenRouter-Adapters nach einem endgültigen
Kontingentfehler wird nicht mitgezählt.

## Messwerte

Gemessen auf dem Mac mini M1 (16 GB), 24.11.2026, Toolchain 1.99.1:

- Kalter Bau (Clippy + Testbau, Debug, eigenes `probe/target`): etwa 4,5 Minuten; danach inkrementell unter 2 s.
  Ein reines `cargo run` (Debug) baut weniger und dürfte darunter liegen (nicht getrennt gemessen).
- Platz `probe/target` nach Clippy, Tests und Trockenlauf: 2,0 GB.
- Trockenlauf: 42 Anfragen (Anthropic 12, OpenAI 17, OpenRouter 13), Summe der Obergrenzen 1,6756 USD
  (0,4582 / 0,6476 / 0,5697 USD). Im Live-Lauf liegen die OpenRouter-Obergrenzen mit den echten Modellpreisen
  deutlich tiefer.
- Tests: 17 (10 in `tests/probe.rs` gegen den lokalen Testserver, 7 in `tests/units.rs`), alle grün; `cargo fmt --check`
  und `cargo clippy --locked --all-targets -- -D warnings` grün.
