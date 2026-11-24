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
App-Einträge unter `OpenSpider`. Konto `ai.<anbieter>`, Wert = Schlüssel als Klartext. In der zsh, je Anbieter:

```sh
read -s "K?Schlüssel für ai.anthropic: " && security add-generic-password -U -s OpenSpider-Probe -a ai.anthropic -w "$K"; unset K
read -s "K?Schlüssel für ai.openai: " && security add-generic-password -U -s OpenSpider-Probe -a ai.openai -w "$K"; unset K
read -s "K?Schlüssel für ai.openrouter: " && security add-generic-password -U -s OpenSpider-Probe -a ai.openrouter -w "$K"; unset K
```

- **Nicht** `security add-generic-password … -w` ohne Wert verwenden: die verdeckte Abfrage von `security` schneidet
  nach **128 Zeichen** ab, OpenAI-Projektschlüssel sind länger (beim Lauf am 24.11.2026 war der OpenAI-Schlüssel
  dadurch zunächst ungültig). `read -s` hat diese Grenze nicht; der Schlüssel landet weder in der Shell-Historie noch
  auf dem Bildschirm (`unset K` entfernt ihn danach aus der Shell).
- `-U` überschreibt einen vorhandenen Eintrag; ohne `-U` bleibt ein alter (z. B. abgeschnittener) Eintrag stehen.
- Fehlt ein Eintrag, überspringt die Probe diesen Anbieter und vermerkt das in `results/<anbieter>.json`. Nach der
  Probe löscht der Nutzer die Einträge selbst: `security delete-generic-password -s OpenSpider-Probe -a ai.<anbieter>`.

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
Anbieter. `summary.json` setzt die Probe seit der Auswertung (24.11.2026) aus **allen** vorhandenen Anbieterdateien
zusammen (`last_run` beschreibt den letzten Lauf); bis dahin überschrieb ein `--only`-Lauf die Summe mit nur seinem
Anbieter. Getrennte `--only`-Läufe sind damit zulässig; Dateien älterer Läufe anderer Anbieter fließen mit ein.

Ohne Netz und ohne Schlüsselbund bereinigt `cargo run --locked -- --rebuild-results` vorhandene Ergebnisdateien erneut
(siehe „Schlüsselschutz“) und setzt `summary.json` neu zusammen.

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
- Vor dem Schreiben entfernt `record::sanitize` Kennungen (Auswertung 24.11.2026): Konto-, Organisations-, Projekt-
  und Nutzerkennungen (`*user_id`, `workspace_id`, `organization_id`, `project_id`, …, auch `expires_at` des
  Schlüssels) → „(ausgelassen: Kontokennung)“; Anbieter-Request- und Antwort-IDs (Header `request-id`,
  `x-request-id`, `x-generation-id`, Felder `id`/`request_id` mit Präfix `msg_`, `req_`, `resp_`, `rs_`, `gen-` …) →
  nur das Präfix bleibt; signierte bzw. verschlüsselte Reasoning-Blobs (`signature`, `encrypted_content`,
  `reasoning.encrypted.data`); IDs kundeneigener Modelle (Fine-Tunes, `…:ft-…`, `ft:…`), die Organisation oder Zweck
  verraten. Header-Werte von `openai-organization`, `openai-project`, `anthropic-organization-id`,
  `anthropic-workspace-id` werden ohnehin nie aufgezeichnet.
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
   immer die Spec-ID `gpt-6-luna` (genau deren Existenz wird geprüft). OpenRouter – seit der Auswertung
   `openai/gpt-6-luna`, falls wählbar (Vergleichbarkeit mit dem direkten Weg; die billigsten Modelle der Liste,
   `inclusionai/ling-3.0-tiny` bzw. für Bilder `scaledown/extract`, waren untypische Spezialmodelle), sonst je
   Fähigkeit das billigste Modell der Liste (ohne `:free`, ohne Preis 0, weil `data_collection: deny` gesendet wird).
   Vorgaben per Option.
4. Decisions-Prüfungen über den Mindestumfang hinaus (V-A1): Bildeingabe, 11 Fragen, `instructions` mit 4.000 Zeichen;
   strukturierte Ausgabe an der OS-Grenze (V-A5): 20 Enum-Werte, 2.000 Zeichen Beschreibung.

## Vergleichstabellen „Spec 08 ↔ beobachtet“

Live-Lauf am 24.11.2026 (Haupt-Mac, drei `--only`-Läufe, 42 Anfragen). Quelle: `probe/results/<anbieter>.json`,
Schritt-ID in Spalte „Schritt“. „Abw.“: **nein** = wie 08; **ja** = weicht ab (Folge und Vorschlag im Bericht
`docs/plan/M4-reports/q00.md`, Nummer in Klammern); **neu** = zusätzliche Felder/Header, die 08 nicht nennt (werden
ignoriert, kein Bruch); **offen** = durch die Probe nicht entschieden.

### Gesamtfeststellung

| Frage | Spec 08 | beobachtet | Folge |
|---|---|---|---|
| Existiert `POST /v1/decisions` wie in AI-004? | öffentliche Beta, Modell `gpt-6-luna` | **ja**: 200 mit `answers[]` exakt in der Form von AI-004 (predicate/choice/score), `usage` vorhanden, Latenz 134–464 ms | E-3 a: Decisions als Standardweg möglich (Empfehlung im Bericht) |
| Steht `gpt-6-luna` in `GET /v1/models`? | ja (AI-004, AI-010) | **ja** (außerdem `gpt-5.6-luna`, `gpt-6-sol`, `gpt-6-astra` u. a.); über OpenRouter als `openai/gpt-6-luna` | K4-06, K4-20 |
| Kommt der Decisions-Adapter mit der echten Antwort zurecht? | AI-004-Antwortform | **ja**: alle drei Fragen `Success`, `level` aus `round(score)`, `confidence_mismatch` bei score (0,7 roh vs. 0,8) | – |
| Kommen die übrigen drei Adapter mit den echten Antworten zurecht? | AI-002/003/005 | **ja** für Text, Schema, Bild, Fehler; Lücken bei Feldern (Q00-N1, -M3, -M4) und ohne Schutz gegen Reasoning-Abbruch (Q00-H1) | N-Paket bzw. K04/D01 |
| Tatsächliche Kosten | Cent-Bereich | Tokens: Anthropic 1.992 ein / 356 aus, OpenAI 3.860 / 438, OpenRouter 896 / 498; OpenRouter meldet Ist-Kosten 0,00026 USD (nur Rohschritte); OpenAI zu Spec-Preisen ≈ 0,0006 USD; Anthropic laut Dashboard (keine Preistabelle); Obergrenze der Probe 0,4964 USD | – |

### Anthropic Messages (AI-002, AI-017)

| Nr. | Prüfpunkt | Spec 08 (13.10.2026) bzw. Annahme | Schritt | beobachtet | Abw. |
|---|---|---|---|---|---|
| A1 | Endpunkt und Authentifizierung | `POST /v1/messages`, Header `x-api-key`, `anthropic-version`, `content-type: application/json` | `anthropic.raw.text` | 200 mit `anthropic-version: 2023-06-01` | nein |
| A2 | Modellliste: Felder je Modell | `id`, `display_name`, `max_input_tokens`, `max_tokens`, `capabilities` | `anthropic.raw.models` | alle vorhanden (15 Modelle); zusätzlich `type`, `created_at`, `lifecycle`, `line`, `deprecated_at`, `retires_at` | neu (N4) |
| A3 | `capabilities`-Knoten | je Fähigkeit Blatt `supported`: `structured_outputs`, `image_input`, `batch`, `effort` mit `low` … `max` | `anthropic.raw.models` | alle wie beschrieben, `effort` mit `low`, `medium`, `high`, `xhigh`, `max`; zusätzlich `thinking` (mit `types.adaptive/disabled/enabled/between_tools`), `pdf_input`, `citations`, `code_execution`, `context_management`, `server_tools` | neu (N4, H1) |
| A4 | Paginierung der Liste (K01-Annahme) | `has_more`, `last_id`; Parameter `limit`, `after_id` | `anthropic.raw.models`, `anthropic.adapter.models` | `has_more = false`, `first_id`, `last_id`; Adapteraufruf mit `?limit=1000` angenommen (1 Seite) | nein |
| A5 | Preise in der Modellliste | keine (Preistabelle, AI-010) | `anthropic.raw.models` | keine Preisfelder | nein |
| A6 | Strukturierte Ausgabe | `output_config.format = {type: "json_schema", schema}`; kein `output_format` | `anthropic.raw.structured` | angenommen, gültiges JSON nach Schema; **rund 350 Eingabe-Tokens Aufschlag** gegenüber Text (399 statt 49) | nein (N7) |
| A7 | Bildeingabe | Block `image`, Quelle `base64` mit `media_type` | `anthropic.raw.image` | angenommen, Antwort „Red“; 63 Eingabe-Tokens | nein |
| A8 | `usage` | `input_tokens`, `output_tokens`, `cache_read_input_tokens`, `cache_creation_input_tokens` | `anthropic.raw.text` | alle vorhanden; zusätzlich `cache_creation.{ephemeral_5m,ephemeral_1h}_input_tokens`, `output_tokens_details.thinking_tokens`, `inference_geo` (`global`), `service_tier`, `speed` | neu |
| A9 | `stop_reason` | u. a. `end_turn`, `max_tokens`, `refusal` (+ `stop_details`) | `anthropic.raw.*` | `end_turn`; bei `structured_limits` **`max_tokens` mit nur einem `thinking`-Block** (256 von 256 Ausgabe-Tokens Thinking, kein Text); `stop_details: null` | ja (H1) |
| A10 | Fehlerform unbekanntes Modell | HTTP-Status, `error.type`, `error.message` | `anthropic.raw.unknown_model` | 404, `type: "error"`, `error.type = "not_found_error"`, Meldung `model: <id>`, zusätzlich `request_id` im Körper | nein |
| A11 | Rate-Limit-Header | `anthropic-ratelimit-requests-remaining`, `-input-tokens-remaining` (+ Reset); `-tokens-remaining` kombiniert, Tausender gerundet | alle `anthropic.raw.*` | alle vorhanden (je `-limit`, `-remaining`, `-reset` als RFC-3339); zusätzlich `-output-tokens-*`; Werte im Lauf ohne sichtbare Rundung (2000000, Limit nicht angebrochen) | neu; Rundung offen |
| A12 | Request-ID | Header bzw. Antwort-`id` | `anthropic.raw.text` | Header `request-id` (`req_…`), Antwort-`id` (`msg_…`); Adapter nimmt `msg_…` | ja (N2) |
| A13 | Tokenzählung | `POST /v1/messages/count_tokens` → `input_tokens` | `anthropic.raw.count_tokens` | 200, `{"input_tokens": 49}` (= Ist-Wert der Textanfrage) | nein |
| V-A3 | 400-Meldungen Guthaben/Ausgabengrenze; 402 | Heuristik | nicht provoziert | – | offen |
| V-A5 | Schema an der OS-Grenze | 20 Enum-Werte, 2.000 Zeichen Beschreibung | `anthropic.raw.structured_limits` | Schema **angenommen** (kein 4xx); Antwort fehlt, weil Thinking die 256 Ausgabe-Tokens verbrauchte | offen (H1) |
| K01-a | Adapter liest die echte Modellliste | `list_models` mit `capabilities` | `anthropic.adapter.models` | 15 Modelle, alle mit Fähigkeiten, Aufwandsstufen `low` … `max` korrekt | nein |
| K01-b | Adapter: Text, Schema, Bild | `Ok` mit `usage` | `anthropic.adapter.{text,structured,image}` | alle `Ok`, `usage` vollständig; Thinking-Blöcke würden übersprungen (nur `text`-Blöcke gelesen) | nein |
| K01-c | Adapter: Fehlerklasse unbekanntes Modell | `Error: <Status> …` | `anthropic.adapter.unknown_model` | `Error: 404 model: …` | nein |
| K01-d | Auslegung 9: `max_tokens` Pflichtfeld | Probe sendet immer `max_tokens` | – | ohne Befund | – |
| K01-e | Auslegungen 1, 5, 10 | – | nicht geprüft | – | – |

### OpenAI Responses (AI-003, AI-017)

| Nr. | Prüfpunkt | Spec 08 bzw. Annahme | Schritt | beobachtet | Abw. |
|---|---|---|---|---|---|
| R1 | Endpunkt und Authentifizierung | `POST /v1/responses`, Bearer-Token | `openai.raw.responses_text` | 200 | nein |
| R2 | Modellliste | `GET /v1/models`, ohne Preise und Fähigkeiten | `openai.raw.models` | 104 Einträge mit `id`, `object`, `created`, `owned_by`, neu `shutdown_date`; keine Preise/Fähigkeiten; 3 kundeneigene Fine-Tunes ausgelassen | neu (N4) |
| R3 | Strukturierte Ausgabe | `text.format = {type: "json_schema", name, schema, strict: true}` | `openai.raw.responses_structured` | angenommen, gültiges JSON; Antwort spiegelt `text.format` (mit `description: null`) | nein |
| R4 | Bildeingabe | `input_image` mit Data-URL (Feld `image_url`) | `openai.raw.responses_image` | angenommen, „Red“; 42 Eingabe-Tokens | nein |
| R5 | Ausgabegrenze (K02 Frage 6) | `max_output_tokens` | `openai.raw.responses_text` | angenommen und in der Antwort gespiegelt | nein |
| R6 | Antwortform | `output[]` → Nachricht → `output_text`/`refusal`; `status` | `openai.raw.responses_*` | wie beschrieben; zusätzlich vorangestelltes `output[]`-Element `type: "reasoning"` (verschlüsselt), Nachricht mit `phase: "final_answer"`; bei `structured_limits` `status: "incomplete"`, `incomplete_details.reason = "max_output_tokens"`, nur Reasoning ausgegeben | ja (H1) |
| R6a | Speicherung / Reasoning-Standard | 08 nennt weder `store` noch `reasoning` | alle | **`store: true`** (Antworten werden gespeichert), `reasoning.effort = "medium"`, `prompt_cache_retention: "24h"` als Standard | ja (H2, H1) |
| R7 | `usage` | `input_tokens`, `output_tokens`, `input_tokens_details.cached_tokens` | `openai.raw.responses_text` | alle vorhanden; zusätzlich `input_tokens_details.cache_write_tokens` (+ TTL-Aufteilung), `output_tokens_details.reasoning_tokens`, `total_tokens` | neu (N1) |
| R8 | Fehlerform unbekanntes Modell | Status, `error.type`, `error.code` | `openai.raw.responses_unknown_model` | 404, `error.type = "invalid_request_error"`, `error.code = "model_not_found"`, `param: null` | nein |
| R9 | Rate-Limit-Header, Request-ID | `x-ratelimit-remaining-requests`, `-tokens` (+ Reset); `x-request-id` | alle `openai.raw.*` | vorhanden (`x-ratelimit-limit/remaining/reset-{requests,tokens}`, Reset als Go-Dauer `2ms`/`0s`); bei Bildern zusätzlich `-input-images`; `x-request-id` | neu |
| V-A5 | Schema an der OS-Grenze | 20 Enum-Werte, 2.000 Zeichen | `openai.raw.responses_structured_limits` | Schema **angenommen**; keine Antwort (Reasoning verbrauchte 256 Tokens) | offen (H1) |
| K02-a | Adapter: Modellliste, Text, Schema, Bild | `Ok`; ohne Katalog keine Fähigkeiten | `openai.adapter.*` | alle `Ok`; Modelle ohne Fähigkeiten (erwartet bis K04); `usage.cache_write_tokens = null`, obwohl geliefert | ja (N1) |
| K02-b | Adapter: unbekanntes Modell | `Error: <Status> …` | `openai.adapter.responses_unknown_model` | `Error: 404 The model … does not exist …` | nein |
| K02-c | Auslegung 10: kein `effort`, kein `store` | – | – | Folge: Standard `store: true`, Reasoning `medium` | ja (H1, H2) |

### OpenAI Decisions (AI-004, AI-033, V-A1)

| Nr. | Prüfpunkt | Spec 08 bzw. Annahme | Schritt | beobachtet | Abw. |
|---|---|---|---|---|---|
| D1 | Endpunkt existiert | `POST /v1/decisions` (Beta) | `openai.raw.decisions_3q` | 200 | nein |
| D2 | Modell | `gpt-6-luna` gelistet und angenommen | `openai.raw.models`, `…decisions_3q` | gelistet (`facts.gpt_6_luna_listed = true`), angenommen | nein |
| D3 | Request-Aufbau | `model`, `input` (Text als Zeichenkette), `questions[]` mit `name`, `type`, `instructions`, `choices[{value, description}]`, `levels[{label, description}]` | `openai.raw.decisions_3q` | angenommen | nein |
| D4 | Antwort `predicate` | `answers[]` je `name`, `type`; `probability` | `…decisions_3q` | `{name, type, probability}` (2 Nachkommastellen) | nein |
| D5 | Antwort `choice` | `choice`, `probabilities[{value, probability}]`, `confidence` | `…decisions_3q` | genau so, alle Werte aufgeführt (auch 0,0) | nein |
| D6 | Antwort `score` | `score`, `probabilities[{value, label, probability}]`, `confidence` | `…decisions_3q` | genau so; `value` = Index ab 0; `score = 0,82 = Σ i·p` | nein |
| D7 | Bedeutung von `confidence` (V-A1) | bis GA = höchste Einzelwahrscheinlichkeit | `…decisions_3q` | choice: 1,0 = Maximum; **score: 0,7 bei Maximum 0,8** – nicht die höchste Einzelwahrscheinlichkeit | ja (N5; Spec-Regel trägt) |
| D8 | `usage` (V-A1) | Format offen | `…decisions_3q` | wie Responses: `input_tokens`, `input_tokens_details.{cached_tokens, cache_write_tokens}`, `output_tokens = 0`, `output_tokens_details.reasoning_tokens = 0`, `total_tokens`; **Eingabe-Tokens je Frage** (3 Fragen 495, 11 Fragen 1.761, 1 Frage mit Bild 165) | ja (M1) |
| D9 | Bildeingabe | Nutzer-Nachricht mit `input_text`/`input_image` (Data-URL) | `openai.raw.decisions_image` | angenommen, `probability 0,95` für „rot“ | nein |
| D10 | Fragen je Request (V-A1) | OS-Grenze 10 bis GA | `openai.raw.decisions_11q` | **11 Fragen angenommen**, alle beantwortet | ja (OS-Grenze bleibt gültig) |
| D11 | Länge `instructions` (V-A1) | OS-Grenze 2.000 Zeichen | `openai.raw.decisions_long_instructions` | 4.000 Zeichen: **HTTP 200, Antwort `type: "refusal"`** ohne weitere Felder, nur 45 Eingabe-Tokens | ja (M2) |
| D12 | Fehlerform (V-A1) | offen | `openai.raw.decisions_unknown_model` | 404, `invalid_request_error`/`model_not_found` wie Responses | nein |
| D13 | Rate-Limit-Header (V-A1) | offen | alle `openai.raw.decisions_*` | `x-ratelimit-{limit,remaining,reset}-{requests,tokens}`, `x-request-id`; kein `openai-version`, Antwort ohne `id` | neu (N6) |
| D14 | Refusal-Felder (V-A1) | offen | `…decisions_long_instructions` | nur `name` und `type: "refusal"` – keine Begründung | neu |
| D15 | Preis | nur Eingabe, 0,10 USD/1M | – | `output_tokens = 0` in allen Antworten; Preis über OpenRouter-Liste für `openai/gpt-6-luna` bestätigt (Responses-Weg), für Decisions über API nicht ablesbar | offen (Dashboard) |
| K02-d | Adapter wertet die echte Antwort aus | `QuestionResult` je Frage | `openai.adapter.decisions_3q` | alle `Success`; score `level = fair` (Index 1), `confidence 0,8`, `confidence_raw 0,7` | nein |
| K02-e | Auslegung 4: Refusal-Grund | – | `…long_instructions` (nur roh) | es gibt kein Grundfeld | – |

### OpenRouter (AI-005, AI-010)

Lauf mit `--model-openrouter openai/gpt-6-luna` (Vorgabe des Orchestrators).

| Nr. | Prüfpunkt | Spec 08 bzw. Annahme | Schritt | beobachtet | Abw. |
|---|---|---|---|---|---|
| O1 | Basis-URL, Authentifizierung | `https://openrouter.ai/api/v1`, Bearer | `openrouter.raw.text` | 200 | nein |
| O2 | Modellliste: Felder | `id`, `name`, `context_length`, `pricing`, `supported_parameters` | `openrouter.raw.models` | alle vorhanden (438 Modelle); zusätzlich `architecture`, `reasoning` (`default_enabled`, `default_effort`, `supported_efforts`), `endpoints[]`, `top_provider`, `default_parameters`, `benchmarks`, `canonical_slug`, `expiration_date`, `links`; oben `total_count`, `links.next` | neu (N4) |
| O3 | Preisfelder | `prompt`, `completion`, `request`, `image` als Zeichenketten; fehlend = 0 | `openrouter.raw.models` | `prompt`/`completion` überall; **`request` in keinem Eintrag**; `image` in 35; neu `input_cache_read`, `input_cache_write(_1h)`, `internal_reasoning`, `web_search`, `audio*`, `image_output` und **`overrides[]` (Langkontext ab `min_prompt_tokens`)**; 8 Modelle mit unlesbaren Preisen | ja (M4) |
| O3a | Preise `gpt-6-luna` | AI-010: 0,10/0,50 USD je 1M, Cache-Lesen 0,01, Langkontext ab 272.000: 0,20/0,75 | `openrouter.raw.models` (`facts.selected_model_entries`) | **identisch**; dazu Cache-Schreiben 0,125, Cache-Lesen im Langkontext 0,02 | nein (neu: Cache-Schreiben) |
| O4 | Fähigkeiten | Structured Output aus `structured_outputs`/`response_format`; Vision aus `architecture.input_modalities` | `openrouter.raw.models` | beide Quellen vorhanden und belastbar | nein |
| O5 | Strukturierte Ausgabe | `response_format {type: json_schema, json_schema{name, strict, schema}}` + `provider.require_parameters` | `openrouter.raw.structured` | angenommen, gültiges JSON | nein |
| O6 | Routing-Präferenzen | `provider.data_collection: "deny"` | `openrouter.raw.text` | angenommen (Anbieter `OpenAI`) | nein |
| O7 | Bildeingabe | `image_url` mit Data-URL | `openrouter.raw.image` | angenommen, „Red“ | nein |
| O8 | `usage` | `prompt_tokens`, `completion_tokens` | `openrouter.raw.text` | vorhanden; zusätzlich **`cost` (Ist-Kosten USD) und `cost_details`**, `prompt_tokens_details.{cached_tokens, cache_write_tokens}`, `completion_tokens_details.reasoning_tokens`, `is_byok` | ja (M3) |
| O9 | Kontostand | `GET /key` → `data.limit`, `limit_remaining`, `limit_reset`, `usage`, `usage_daily/weekly/monthly` | `openrouter.raw.key` | alle vorhanden (`limit 50`, `limit_remaining 50`, `limit_reset null`); zusätzlich `budgets[]`, `effective_budget`, `allowed_data_regions`, `byok_*`, `free_model_daily_requests`, `is_*_key`, `rate_limit` (veraltet), Konto-/Arbeitsbereichskennungen (ausgelassen) | neu |
| O10 | Fehlerform unbekanntes Modell | Status, `error.code`, `error.message`, `error.metadata` | `openrouter.raw.unknown_model` | **400** (nicht 404), `error.code = 400`, Meldung `… is not a valid model ID`, **kein `error.type`, kein `metadata`**, Top-Level `user_id` (ausgelassen) | ja (N3) |
| O11 | Rate-Limit-Header | 08 nennt keine | alle `openrouter.raw.*` | keine; Header `x-generation-id`, `x-openrouter-release`, bei Fehler `x-openrouter-error-origin` | nein |
| O12 | Reasoning | 08 nennt keines | `openrouter.raw.structured*` | bei `gpt-6-luna` standardmäßig an: `message.reasoning` (Zusammenfassung), `reasoning_details[]` (verschlüsselt, ausgelassen); `structured_limits`: `finish_reason = "length"`, `content: null` | ja (H1) |
| V-A4 | Refusal-Formen | `message.refusal` / `content_filter`; 403 `refusal` | nicht provoziert | Feld `message.refusal: null` existiert | offen |
| V-A5 | Schema an der OS-Grenze | 20 Enum-Werte, 2.000 Zeichen | `openrouter.raw.structured_limits` | Schema **angenommen**; keine Antwort (Reasoning 256 von 256) | offen (H1) |
| K01-f | Adapter: Modellliste, Kontostand | `OpenRouterModel`, `KeyBalance` | `openrouter.adapter.{models,key}` | `Ok`; Preise inkl. Cache-Lesen/-Schreiben korrekt; `KeyBalance` korrekt (50 USD); `overrides` und `usage.cost` nicht gelesen | ja (M3, M4) |
| K01-g | Adapter: Text, Schema, Bild, unbekanntes Modell | `Ok` bzw. `Error: <Status> …` | `openrouter.adapter.*` | alle `Ok`; `Error: 400 … is not a valid model ID` | nein |
| K01-h | Auslegung 12: unlesbare Preise → „Price unknown“ | `prices_unknown` | `openrouter.adapter.models` | 8 Modelle | nein |

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
- Tests: 21 (12 in `tests/probe.rs` gegen den lokalen Testserver, 9 in `tests/units.rs`), alle grün; `cargo fmt --check`
  und `cargo clippy --locked --all-targets -- -D warnings` grün (Stand nach der Auswertung, 24.11.2026).
- Live-Lauf 24.11.2026 (Haupt-Mac): Anthropic 12, OpenAI 17, OpenRouter 13 Anfragen; Kostenobergrenze der Probe
  0,1739 / 0,2453 / 0,0772 USD = 0,4964 USD; tatsächliche Kosten im Cent-Bereich (siehe Bericht).
