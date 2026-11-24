# M4-Q00b – Zweite kurze Live-Probe (V-A6 Reasoning-Parameter, V-A8 Speicherung)

Paket M4-Q00b (Welle 4a). Stand: 24.11.2026, **vorbereitet, noch nicht gelaufen**. Startbedingung: neue Testschlüssel
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

## Vergleichstabelle „Spec/Adapter ↔ beobachtet“ (nach dem Lauf füllen)

| Nr. | Schritt | Gesendet (Form) | Spec 08 / Adapter erwartet | beobachtet (Status, Antwort ja/nein, Reasoning-Tokens, Fehlerform) | Abw. |
|---|---|---|---|---|---|
| A0 | `anthropic.models` | – | `claude-haiku-5-5` gelistet mit `thinking.types.disabled/adaptive.supported = true`, `effort.low.supported = true`; mindestens ein Modell ohne `types.disabled` | | |
| A1 | `anthropic.default` | kein `thinking`, kein `effort` | 200, Antwort, Thinking-Tokens bei dieser kleinen Aufgabe 0 (Q00) | | |
| A2 | `anthropic.off` | `thinking: {type: disabled}` | 200, Antwort, `thinking_tokens = 0`, kein `thinking`-Block | | |
| A3 | `anthropic.low` | `thinking: {type: adaptive}` + `effort: low` | 200, Antwort, wenige oder 0 Thinking-Tokens | | |
| A4 | `anthropic.adaptive_only` | `thinking: {type: adaptive}` | 200 (Adapter sendet das nie ohne `effort`) | | |
| A5 | `anthropic.effort_only` | `effort: low` ohne `thinking` | 200 (Form für Modelle ohne `capabilities.thinking`) | | |
| A6 | `anthropic.off_with_effort` | `disabled` + `effort: low` | offen (Adapter sendet das nie) | | |
| A7 | `anthropic.off_not_disableable` | `disabled` an Modell ohne `types.disabled` | 4xx (`Error: <Code> <Meldung>`, V-A6 OS-Festlegung) | | |
| O0 | `openai.models` | – | `gpt-6-luna` gelistet | | |
| O1 | `openai.default` | `store: false`, kein `reasoning` | 200, Antwort, Reasoning-Tokens > 0 (`effort = medium`, Q00), Antwort `store: false` | | |
| O2 | `openai.off` | `reasoning: {effort: none}` | 200, Antwort, `reasoning_tokens = 0`, Echo `reasoning.effort = "none"` | | |
| O3 | `openai.low` | `reasoning: {effort: low}` | 200, Antwort, wenige Reasoning-Tokens | | |
| O4 | `openai.minimal` | `reasoning: {effort: minimal}` | offen (gehört `minimal` zu `reasoning_efforts` von `gpt-6-luna`?) | | |
| O5 | `openai.invalid_effort` | `reasoning: {effort: off}` | 400 mit `error.param`/`code`; Meldung nennt evtl. die zulässigen Werte | | |
| V8a | `openai.default` (Antwort) | `store: false` | Antwort `store: false`; `prompt_cache_retention` = ? (Q00: `"24h"`) | | |
| V8b | `openai.decisions_plain` | ohne `store` | 200, Antwort ohne `store`-Feld (AI-004 Nachtrag) | | |
| V8c | `openai.decisions_store_false` | `store: false` | offen: 200 (ignoriert/gespiegelt) oder 400 (unbekannter Parameter) | | |
| R0 | `openrouter.models` | – | `openai/gpt-6-luna` mit `reasoning.supported_efforts` ∋ `none`, `mandatory = false`, `reasoning` in `supported_parameters` | | |
| R1 | `openrouter.default` | kein `reasoning`, `require_parameters: true` | 200, Antwort, Reasoning-Tokens > 0 (Q00: 52) | | |
| R2 | `openrouter.off` | `reasoning: {effort: none}` + `require_parameters: true` | 200, Antwort, `reasoning_tokens = 0`; kein 404 „no endpoints“ | | |
| R3 | `openrouter.low` | `reasoning: {effort: low}` | 200, Antwort, weniger Reasoning-Tokens als R1 | | |
| R4 | `openrouter.enabled_false` | `reasoning: {enabled: false}` | offen (Adapter sendet das nie) | | |

## Mögliche Ausgänge und Folge

| Ausgang | Folge für `adapters/anthropic/reasoning.rs` bzw. Katalogdaten |
|---|---|
| A2 200, `thinking_tokens = 0` | Form `off` bestätigt; V-A6 (a) mit Datum nachtragen, „sicher aus“ für Anthropic belegt. |
| A2 4xx | `anthropic_thinking(Off)` ist falsch: Fehlerform und Meldung an den Planer; bis zur Klärung ist `off` bei Anthropic nicht verfügbar (niedrigste Stufe), K09 nicht mergen. |
| A2 200, aber `thinking_tokens > 0` oder `thinking`-Block | `disabled` wird angenommen, wirkt aber nicht: `off` nicht als „sicher aus“ werten; Spec-Vorschlag (AI-011), `effort: low` zusätzlich zu prüfen. |
| A3 200 | Form `low` bestätigt. A3 4xx, A5 200: `adaptive` + `effort` verträgt sich nicht – `anthropic_thinking` für `minimal` … `max` weglassen und nur `effort` senden. |
| A4/A5 unterschiedlich zu A3 | Hinweis, welcher der beiden Teile die Wirkung trägt; Abbildung der Stufen in AI-002 ggf. vereinfachen. |
| A6 200 | Konflikt ist harmlos; keine Änderung (der Adapter sendet bei `off` ohnehin kein `effort`). A6 4xx: Regel „bei `off` nie `effort`“ in AI-002 ausdrücklich festhalten und im Mock prüfen. |
| A7 4xx | Erwartung bestätigt; Fehlerform für `Error: <Code> <Meldung>` und ein Mock-Szenario übernehmen. A7 200: `types.disabled.supported = false` bedeutet nicht „abgelehnt“ – Regel in AI-002 (nur senden, wenn unterstützt) bleibt, Befund nachtragen. |
| O2 200, `reasoning_tokens = 0` | `"none"` für `gpt-6-luna` bestätigt; Katalog `reasoning_efforts` für `gpt-6-luna` mit `none` (K04). O2 4xx: `none` aus `reasoning_efforts` streichen, `off` bildet auf die niedrigste angenommene Stufe ab (AI-003 Nachtrag) – K04-Katalog und K09-Test anpassen. |
| O4 200 / 4xx | `minimal` in bzw. nicht in `reasoning_efforts` von `gpt-6-luna` (K04-Katalog). |
| O5 Meldung nennt zulässige Werte | Liste als Quelle für `reasoning_efforts` (mit Datum) im Katalog vermerken. |
| V8a Antwort `store: false` | H2 umgesetzt bestätigt; `prompt_cache_retention`-Wert für V-A8 und AI-018 (Datenflussübersicht) nachtragen; weiterhin nichts senden (Entscheidung 24.11.2026). |
| V8b/V8c | Decisions ohne `store`-Feld und 400 bei `store: false`: AI-004 Nachtrag („nie `store` senden“) bestätigt. 200 bei `store: false`: Decisions kennt den Parameter – Planer entscheidet, ob er gesendet wird (V-A8, AI-018). |
| R2 200, `reasoning_tokens = 0` | Form `off` für OpenRouter bestätigt (`mandatory = false`, `none` in `supported_efforts`). R2 404/400 mit `require_parameters`: AI-005-Regel prüfen (ggf. `off` → niedrigster Wert aus `supported_efforts` ohne `none`). |
| R4 abweichend von R2 | Nur Befund; der Adapter bleibt bei einer Form (`effort`, AI-005 OS-Festlegung). |

## Trockenlauf (24.11.2026)

Siehe `docs/plan/M4-reports/q00b.md` (vollständige Ausgabe). Kurz: 21 Anfragen (Anthropic 8, OpenAI 8, OpenRouter 5),
Obergrenze 0,7480 USD.
