# M4-Q00b – Zweite kurze Live-Probe (V-A6 Reasoning-Parameter, V-A8 Speicherung)

Paket M4-Q00b (Welle 4a). Vorbereitet 24.11.2026, **live gelaufen am 25.11.2026** (21 Anfragen, Kostenobergrenze der Probe 0,2887 USD; Auswertung in `docs/plan/M4-reports/q00b.md`). Startbedingung: neue Testschlüssel
des Nutzers (die aus Q00 sind widerrufen) und ein eigenes, ausdrückliches Ja des Nutzers zu diesem Live-Lauf
(Obergrenze 1 USD). Das Ergebnis ist Merge-Voraussetzung für M4-K09.

## Zweck

Die Live-Probe Q00 hat gezeigt, dass alle getesteten Modelle standardmäßig nachdenken (Q00-H1) und OpenAI Responses
standardmäßig speichert (Q00-H2). 08 legt seitdem die Parameter fest (Nachträge vom 24.11.2026 an AI-002, AI-003,
AI-004, AI-005, AI-011), die genaue Form ist aber nicht live belegt (V-A6, V-A8). Q00b prüft mit 21 kleinen Anfragen:

- **(a) Anthropic:** Nimmt `claude-haiku-5-5` `thinking: {"type": "disabled"}` und `{"type": "adaptive"}` an – allein
  und zusammen mit `output_config.effort`? Was meldet `usage.output_tokens_details.thinking_tokens` (0 bei
  `disabled`)? Was passiert im Konfliktfall `disabled` + `effort`, und was antwortet ein Modell ohne
  `thinking.types.disabled` auf `disabled` (Fehlerform)?
- **(b) OpenAI Responses:** Nimmt `gpt-6-luna` `reasoning: {"effort": "none"}` an (Reasoning-Tokens 0)? Wird
  `minimal` angenommen? Wie sieht die Ablehnung eines unbekannten Werts aus?
- **(c) OpenRouter:** `reasoning: {"effort": "none"}` für `openai/gpt-6-luna` zusammen mit
  `provider.require_parameters: true` (Reasoning-Tokens 0?); Alternative `reasoning: {"enabled": false}`.
- **(d) V-A8:** Spiegelt Responses `store: false` in der Antwort, und was steht bei `prompt_cache_retention`? Kennt
  die Decisions-Antwort ein `store`-Feld, und wie reagiert `/v1/decisions` auf `store: false`?

Alle Schema-Anfragen nutzen dieselbe kleine Aufgabe wie Probe 1 (Seitentyp mit AI-034-Schema, Antwort rund 40 Tokens)
mit `max_tokens`/`max_output_tokens` = 200. Die Formen „Adapter“ sind genau die, die
`crates/os-ai/src/adapters/anthropic/reasoning.rs` (Zweig `ai/k09`, Stand `edca3073`) nach 08 sendet: Stufe `off` →
Anthropic `thinking: {"type": "disabled"}` (ohne `effort`), OpenAI/OpenRouter `reasoning: {"effort": "none"}`;
Stufe `low` → Anthropic `thinking: {"type": "adaptive"}` + `output_config.effort = "low"`, sonst
`reasoning: {"effort": "low"}`; `model-default` → kein Parameter. „Alternative“ sind die naheliegenden Nachbarformen
und Grenzfälle. Die Probe passt keine Form an die öffentliche Anbieterdoku an (Clean-Room; die Formen stammen aus 08
und dem Adapter).

## Aufbau

```
probe-2/
  README.md          dieser Rahmen (Vergleichstabelle unten, Spalte „beobachtet“ leer)
  probe/             Kopie von probe-1/probe mit eigenem Plan (src/plan.rs, src/requests.rs, src/runner.rs);
                     Kostenbremse, Schlüsselschutz und record::sanitize unverändert übernommen; ohne os-ai
    Cargo.lock       gekürzte Wurzel-Cargo.lock von main (keine neueren Versionen)
    tests/           18 Tests gegen einen lokalen Testserver (127.0.0.1), ohne Schlüsselbund
    results/         entsteht im Live-Lauf: <anbieter>.json, summary.json
```

## Bedienung

Alle Befehle im Ordner `docs/plan/M4-acceptance/live/probe-2/probe`.

### 1. Trockenlauf (Standard; kein Netzwerk, kein Schlüsselbund)

```sh
cargo run --locked -- --dry-run
```

### 2. Neue Testschlüssel hinterlegen (Nutzer)

Nur unter dem Dienst `OpenSpider-Probe`; nötig sind `ai.anthropic` und `ai.openai`, `ai.openrouter` ist optional
(fehlt der Eintrag, überspringt die Probe OpenRouter). In der zsh, je Konto:

```sh
read -s "K?Schlüssel für ai.anthropic: " && security add-generic-password -U -s OpenSpider-Probe -a ai.anthropic -w "$K"; unset K
read -s "K?Schlüssel für ai.openai: " && security add-generic-password -U -s OpenSpider-Probe -a ai.openai -w "$K"; unset K
read -s "K?Schlüssel für ai.openrouter: " && security add-generic-password -U -s OpenSpider-Probe -a ai.openrouter -w "$K"; unset K
```

Nicht `security add-generic-password … -w` ohne Wert verwenden: die verdeckte Abfrage schneidet nach 128 Zeichen ab
(OpenAI-Projektschlüssel sind länger). `-U` überschreibt die alten, widerrufenen Einträge aus Q00. macOS fragt beim
ersten Zugriff des Programms nach („Erlauben“). Die Probe liest ersatzweise auch `OpenSpider/ai.<anbieter>`; für Q00b
sollen dort keine Schlüssel liegen. Nach der Probe: `security delete-generic-password -s OpenSpider-Probe -a
ai.<anbieter>`.

### 3. Live-Lauf (erst nach dem Ja des Nutzers)

```sh
OPENSPIDER_LIVE_API_TESTS=1 cargo run --locked -- --live --models-only   # nur die drei Modelllisten (kostenlos)
OPENSPIDER_LIVE_API_TESTS=1 cargo run --locked -- --live                 # voller Plan, 21 Anfragen
```

Optionen: `--only anthropic|openai|openrouter` (getrennte Läufe sind zulässig; `summary.json` wird aus allen
vorhandenen Anbieterdateien zusammengesetzt), `--max-total-usd <betrag>` (nur nach unten, höchstens 1). Danach
optional ohne Netz: `cargo run --locked -- --rebuild-results` (Ergebnisdateien erneut bereinigen, Summe neu bilden).

## Kostenbremse

- Je Anbieter höchstens **0,40 USD** (Paketeintrag M4-Q00b; der Auftrag nannte höchstens 0,50 USD – die engere Grenze
  gilt), gesamt höchstens **1 USD**, feste Höchstzahl an HTTP-Anfragen (Anthropic 9, OpenAI 9, OpenRouter 6; der Plan
  liegt mit 8/8/5 darunter). Keine Wiederholungen (`retry::never()`), jede Anfrage höchstens einmal.
- Obergrenze je Anfrage: Eingabe ⌈Zeichen/2⌉ + 64 Tokens, volle Ausgabegrenze 200; Preise wie Probe 1 vorsichtig
  30/150 USD je 1 Mio. Tokens (Anthropic, OpenAI; Listen ohne Preise), OpenRouter Listenpreise plus 0,01 USD Gebühr
  (Planung: vorsichtige Preise). Summe der Obergrenzen laut Trockenlauf: **0,7480 USD** (0,2992 / 0,2305 / 0,2182);
  jede Anbietersumme liegt unter 0,40 USD, die Bremse schneidet also keinen geplanten Schritt ab. Tatsächlich zu
  erwarten: Bruchteile eines Cents (Q00: ≈ 0,0006 USD für 17 OpenAI-Anfragen).

## Schlüsselschutz und Bereinigung

Unverändert aus Probe 1: Schlüssel nur im Live-Lauf aus dem Schlüsselbund, Typ mit geschwärzter `Debug`-Ausgabe, keine
Authentifizierungs-Header in den Aufzeichnungen, Antwort-Header nur mit Namen (Werte nur bei Rate-Limit-, Request-ID-
und Formatheadern), Schwärzer für Schlüssel und deren letzte Zeichen, und `record::sanitize` vor jedem Schreiben
(Konto-/Organisations-/Projekt-/Nutzerkennungen, Anbieter-Request-IDs nur mit Präfix, signierte bzw. verschlüsselte
Reasoning-Blobs, kundeneigene Modelle). Belegt durch
`tests/probe.rs::full_run_sends_each_planned_form_once_and_records_observations` (Testserver gibt Schlüssel,
`openai-organization`, `user_id`, Request-IDs, Signatur- und verschlüsselte Blobs und ein Fine-Tune zurück).

## Aufgezeichnet je Anfrage

`results/<anbieter>.json` → `requests[]`: HTTP-Status, Antwortkörper (gekürzt, bereinigt), `usage`, und
`observation` mit `answer_present`, `answer_is_json`, `reasoning_tokens` (Anthropic
`usage.output_tokens_details.thinking_tokens`, OpenAI `usage.output_tokens_details.reasoning_tokens`, OpenRouter
`usage.completion_tokens_details.reasoning_tokens`), `output_tokens`, `stop` (`stop_reason`/`status`/
`finish_reason`), `incomplete_reason`, `thinking_blocks`, `store`, `prompt_cache_retention`, `reasoning_echo`,
`error_type`/`error_code`/`error_message`, `decisions_answer_types`. Dazu `facts`: Thinking-Fähigkeiten aller
Anthropic-Modelle (`thinking`, `types.disabled`, `types.adaptive`, `effort`, `effort.low`), der Listeneintrag von
`gpt-6-luna` und für `openai/gpt-6-luna` bei OpenRouter `reasoning` (`supported_efforts`, `mandatory`,
`default_enabled`), `supported_parameters`, `pricing`.

## Vergleichstabelle „Spec/Adapter ↔ beobachtet“

Live-Lauf am 25.11.2026 (Haupt-Mac, Ja des Nutzers, ein voller Lauf über alle drei Anbieter; zwei vorherige
kostenlose `--models-only`-Läufe sind überschrieben). Quelle: `probe/results/<anbieter>.json`, Feld `observation` und
`usage` je Schritt. „Abw.“: **nein** = wie 08/Adapter; **ja** = weicht ab; **belegt** = vorher offen, jetzt geklärt;
**Grenze** = Befund schränkt die Aussage ein.

| Nr. | Schritt | Gesendet (Form) | Spec 08 / Adapter erwartet | beobachtet (Status, Antwort, Reasoning-Tokens, Fehlerform) | Abw. |
|---|---|---|---|---|---|
| A0 | `anthropic.models` | – | `claude-haiku-5-5` mit `thinking.types.disabled/adaptive` und `effort.low`; ein Modell ohne `types.disabled` | 15 Modelle. `claude-haiku-5-5`: `thinking`, `adaptive`, `disabled`, `effort`, `effort.low` alle `true`. **Ohne `types.disabled`:** `claude-fable-5-2`, `claude-sonnet-5-5`, `claude-opus-5-5`, `claude-fable-5-1`, `claude-fable-5`. **Ohne `types.adaptive`:** `claude-opus-4-5-20251101`, `claude-haiku-4-5-20251001`, `claude-sonnet-4-5-20250929` (die beiden letzten auch ohne `effort`) | belegt |
| A1 | `anthropic.default` | kein `thinking`, kein `effort` | 200, Antwort, Thinking-Tokens bei dieser kleinen Aufgabe 0 (wie Q00) | 200, JSON-Antwort, `usage.output_tokens = 42`, `output_tokens_details.thinking_tokens = 0`, kein `thinking`-Block, `end_turn` | nein |
| A2 | `anthropic.off` | `thinking: {type: disabled}` | 200, Antwort, `thinking_tokens = 0` | 200, JSON-Antwort, 41 Ausgabe-Tokens, `thinking_tokens = 0`, kein Block | nein (Grenze, s. u.) |
| A3 | `anthropic.low` | `thinking: {type: adaptive}` + `effort: low` | 200, Antwort, wenige oder 0 Thinking-Tokens | 200, JSON-Antwort, 42, `thinking_tokens = 0` | nein |
| A4 | `anthropic.adaptive_only` | `thinking: {type: adaptive}` | 200 | 200, JSON-Antwort, 42, `thinking_tokens = 0` | nein |
| A5 | `anthropic.effort_only` | `effort: low` ohne `thinking` | 200 | 200, JSON-Antwort, 42, `thinking_tokens = 0` | nein |
| A6 | `anthropic.off_with_effort` | `disabled` + `effort: low` | offen | 200, JSON-Antwort, 41, `thinking_tokens = 0` – **Konflikt wird angenommen** | belegt |
| A7 | `anthropic.off_not_disableable` | `disabled` an `claude-fable-5-2` (`types.disabled = false`) | 4xx | **400** `invalid_request_error`: „"thinking.type.disabled" is not supported for this model. Use "thinking.type.adaptive" and "output_config.effort" to control thinking behavior.“ (kein `error.code`) | nein |
| O0 | `openai.models` | – | `gpt-6-luna` gelistet | gelistet (`owned_by: system`, `shutdown_date: null`); 108 Modelle, 3 kundeneigene ausgelassen | nein |
| O1 | `openai.default` | `store: false`, kein `reasoning` | 200, Reasoning-Tokens > 0 (`medium`), Antwort `store: false` | 200, JSON-Antwort, 76 Ausgabe-Tokens, **36 Reasoning-Tokens**, Echo `reasoning.effort = "medium"`, `store: false` | nein |
| O2 | `openai.off` | `reasoning: {effort: none}` | 200, `reasoning_tokens = 0`, Echo `none` | 200, JSON-Antwort, 38, **`reasoning_tokens = 0`**, kein `reasoning`-Element in `output[]`, Echo `effort: "none"` | belegt |
| O3 | `openai.low` | `reasoning: {effort: low}` | 200, wenige Reasoning-Tokens | 200, JSON-Antwort, 61, `reasoning_tokens = 21`, Echo `low` | nein |
| O4 | `openai.minimal` | `reasoning: {effort: minimal}` | offen | **400** `invalid_request_error`, `code = unsupported_value`, `param = reasoning.effort`: „'minimal' is not supported with the 'gpt-6-luna' model. Supported values are: 'none', 'low', 'medium', 'high', 'xhigh', and 'max'.“ | belegt |
| O5 | `openai.invalid_effort` | `reasoning: {effort: off}` | 400 mit `param`/`code` | **400**, `code = invalid_value`, `param = reasoning.effort`: „Supported values are: 'none', 'minimal', 'low', 'medium', 'high', 'xhigh', and 'max'.“ (allgemeine Werteliste der API) | nein |
| V8a | `openai.default` (Antwort) | `store: false` | Antwort `store: false`; `prompt_cache_retention` = ? | in allen drei 200-Antworten `store: false`; **`prompt_cache_retention: "24h"`** unverändert (ohne gesendetes Feld) | belegt |
| V8b | `openai.decisions_plain` | ohne `store` | 200, kein `store`-Feld | 200, `answers[0]` = `soft_404`/`predicate`/`probability 0.0`, Antwort ohne `store`, `usage.input_tokens = 212`, `output_tokens = 0`, `reasoning_tokens = 0`. In der Konsole „Antwort false“: Auswertungsfehler der Probe (zählte `answers[]` nicht), in `results/openai.json` korrigiert und vermerkt | nein |
| V8c | `openai.decisions_store_false` | `store: false` | offen | **400** `invalid_request_error`, `code = unknown_parameter`, `param = store`: „Unknown parameter: 'store'.“ | belegt |
| R0 | `openrouter.models` | – | `openai/gpt-6-luna` mit `supported_efforts` ∋ `none`, `mandatory = false`, `reasoning` in `supported_parameters` | `supported_efforts = [max, xhigh, high, medium, low, none]`, `mandatory = false`, `default_enabled = true`, `default_effort = medium`; `supported_parameters` enthält `reasoning` (und `reasoning_effort`, `include_reasoning`) | nein |
| R1 | `openrouter.default` | kein `reasoning`, `require_parameters: true` | 200, Reasoning-Tokens > 0 | 200, JSON-Antwort, 65 Ausgabe-, **25 Reasoning-Tokens**, `usage.cost = 0,0000435` | nein |
| R2 | `openrouter.off` | `reasoning: {effort: none}` + `require_parameters: true` | 200, `reasoning_tokens = 0` | 200, JSON-Antwort, 38, **`reasoning_tokens = 0`**, keine `reasoning_details`, `usage.cost = 0,00003` | belegt |
| R3 | `openrouter.low` | `reasoning: {effort: low}` | 200, weniger als R1 | 200, JSON-Antwort, 59, `reasoning_tokens = 19`, `message.reasoning` mit Zusammenfassung | nein |
| R4 | `openrouter.enabled_false` | `reasoning: {enabled: false}` | offen | 200, JSON-Antwort, 38, `reasoning_tokens = 0` – gleiche Wirkung wie R2 | belegt |

**Zu Anthropic „0 Thinking-Tokens auch ohne Parameter“:** `usage.output_tokens_details.thinking_tokens` ist in allen
sechs 200-Antworten 0, es gibt keinen `thinking`-Block, und die Ausgabe-Tokens (41–42) entsprechen nur dem JSON. Das
widerspricht Q00 nicht: Dort dachte `claude-haiku-5-5` ebenfalls nur bei der Grenzwert-Anfrage mit 20 Enum-Werten und
2.000 Zeichen Beschreibung (256 Thinking-Tokens), bei der kleinen Schema-Aufgabe waren es auch damals 0 (Q00-H1:
„bei einfachen Anfragen 0“). Folge: Q00b belegt, dass `disabled`, `adaptive` + `effort` und alle Alternativen
**angenommen** werden; ob `disabled` Thinking bei einer Aufgabe, die sonst denkt, wirklich abschaltet, belegt Q00b
nicht (Grenze; Vorschlag: in Q02 mit dem Grenzwert-Schema wiederholen).

## Ausgänge und Folgen

| Ausgang (eingetreten) | Folge |
|---|---|
| A2 200, `thinking_tokens = 0` (ohne Vergleichswert, s. o.) | Form `off` angenommen. V-A6 (a) nachtragen: Form belegt, Wirkung bei dieser Aufgabe nicht messbar. |
| A3/A4/A5 200 | `adaptive` + `effort`, `adaptive` allein und `effort` allein werden angenommen; Abbildung in AI-002 bleibt. |
| A6 200 | Konflikt `disabled` + `effort` ist harmlos; keine Änderung (der Adapter sendet bei `off` ohnehin kein `effort`). |
| A7 400 mit Hinweis auf `adaptive` + `effort` | Regel „`off` nur bei `types.disabled.supported`, sonst niedrigste Stufe“ bestätigt; die Meldung empfiehlt genau die Ersatzform des Adapters. Fünf aktuelle Modelle (u. a. `claude-sonnet-5-5`, `claude-opus-5-5`, `claude-fable-5-2`) haben **kein** `off`; die Fehlerform gehört als Mock-Szenario in os-testsite. |
| O2 200, `reasoning_tokens = 0` | `"none"` für `gpt-6-luna` bestätigt, „sicher aus“ für OpenAI belegt. |
| O4 400 `unsupported_value` | `minimal` gehört **nicht** in `reasoning_efforts` von `gpt-6-luna`; die Meldung nennt die Liste `none, low, medium, high, xhigh, max` – identisch mit dem K04-Katalog. |
| O5 400 `invalid_value` | Allgemeine Werteliste der API: `none, minimal, low, medium, high, xhigh, max` (= die Stufen aus AI-011 außer `off`). |
| V8a `store: false` gespiegelt, `prompt_cache_retention: "24h"` | H2-Umsetzung bestätigt; ob `"24h"` bei `store: false` noch etwas speichert, bleibt offen (V-A8, AI-018-Hinweis). |
| V8c 400 `unknown_parameter` | Decisions kennt `store` nicht: „nie `store` an `/v1/decisions`“ (AI-004 Nachtrag) ist zwingend, nicht nur vorsichtig. Ob Decisions speichert, bleibt offen. |
| R2 200, `reasoning_tokens = 0`, mit `require_parameters` | Form `off` für OpenRouter bestätigt; kein Konflikt mit `require_parameters`. |
| R4 200, gleiche Wirkung | Nur Befund; der Adapter bleibt bei `effort` (AI-005 OS-Festlegung). |

## Trockenlauf (24.11.2026)

Siehe `docs/plan/M4-reports/q00b.md` (vollständige Ausgabe). Kurz: 21 Anfragen (Anthropic 8, OpenAI 8, OpenRouter 5),
Obergrenze 0,7480 USD.
