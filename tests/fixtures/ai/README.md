# KI-Fixtures (M4-T01)

Quelle für Verhalten und Prüfwerte: `docs/spec/08-ai.md`, Konventionen.
Alle Inhalte sind lokale Testdaten; Schlüssel in Tests sind erfundene Platzhalter.
Keine echten Anbieter aufrufen.

## Start und Basis-URLs

`os_testsite::mocks::MockServer::start_all()` registriert ai-mock zusammen mit
fixture-apis. Isoliert: `AiMock::fixtures()?.registrations()` an
`MockServer::start(...)` übergeben. Beide Varianten binden ausschließlich
`127.0.0.1` auf einem ephemeren Port. Die Registry liefert
`api.{anthropic,openai,openrouter,ollama}.base_url`; für die Umgebungsvariablen
`OPENSPIDER_API_BASE_<ANBIETER>` den passenden Präfix einschließlich API-Version
verwenden:

| Anbieter | Präfix auf dem Mock |
|---|---|
| Anthropic | `/anthropic/v1` |
| OpenAI | `/openai/v1` |
| OpenRouter | `/openrouter/api/v1` |
| Ollama | `/ollama` (API-Routen beginnen mit `/api`) |

Clients verwenden die bestehende `os_fetch::resolve::HostResolver`-Zuordnung
`*.mock.test=127.0.0.1` und deaktivieren System-Proxys. Die URL enthält weiter den
Mock-Host und den ephemeren Port; so bleiben globale und EU-Hosts im Log getrennt.
Der Resolver-Vertrag ist in `crates/os-fetch/tests/ai_mock_hosts.rs` geprüft.
Produktive Adapter und die Freigabe von Overrides in Release-Builds besitzen die
jeweiligen Fachpakete. Gemini-Testserver besitzt M4-K03.

## Antworten und Szenarien

Alle `mock-answers.d/*.json` werden in sortierter Reihenfolge geladen:

```json
{"product-1": {"page-type": {"type": "choice", "choice": "product"}, "default": {"ok": true}}}
```

Die äußere Kennung kommt aus `MOCK-ID:product-1`, die innere aus `questions[].id`
(Decisions) oder `task` (Textaufgabe). Ohne passenden Eintrag gilt `default`, dann
`{"ok":true}`; Decisions besitzen außerdem feste Standardwerte je Fragetyp.
Doppelte Paare (MOCK-ID, Frage/Aufgabe) über Fragmentgrenzen sind Ladefehler;
unterschiedliche Fragen derselben MOCK-ID sind erlaubt. Jedes Folgepaket besitzt
sein eigenes Fragment (M4 E5), `base.json` bleibt bei T01.

Szenarien gelten per `X-Scenario` (auch `ok`, `empty`, `4xx`, `throttle` aus
INTEG-019), per `MOCK-SCENARIO:<name>` im Request oder global über
`POST /__mock/scenario {"provider":"openai","scenario":"429x2","count":3}`.
Explizite Header/Marker gehen vor und verbrauchen den globalen Zähler nicht.
Wiederholungszähler sind je Anbieter, Host, Pfad, Szenario und Request-Body
isoliert. Erneutes Setzen des globalen Szenarios setzt dessen Anbieterzähler zurück.
`429x2` liefert zweimal 429, dann Erfolg; `retry-after-7` und `402-in-flight`
liefern einmal den Fehler und dann Erfolg. EU-Fehler wirken nur auf `eu.*`-Hosts.
Zusätzlich zur Spec-Tabelle: `missing-usage` und `max-tokens` für Adapter-Grenzfälle.
`hang` wartet unbegrenzt, `delay=<s>` verwendet Tokio-Zeit; Tests können die
Tokio-Uhr pausieren und fortschalten. `GET /__mock/log` enthält auch Steueraufrufe,
Zeitstempel in Unix-Millisekunden, Methode, Pfad, Host, Header und unveränderten Body.
Authorization, x-api-key und api-key sind immer `[REDACTED]`.

Texttoken zählen Unicode-Zeichen der Inhalts-/System-/Eingabetextteile, keine
Modellnamen, Schemata, Rollen oder Base64-Daten. Ein Bild zählt 85 Token.
Ausgabetoken zählen die serialisierte Antwort. Embeddings sind SHA-256-basiert,
deterministisch und auf Länge 1 normiert. `dimensions` wählt die Dimension
(Standard 768). Fragmentfeld `embedding` kann einen Vektor derselben Dimension
vorgeben. Die 3D-Produktvektoren prüfen Kosinuswerte 0,9500 und 0,9499.

Batchstatus erreicht nach der dritten Statusabfrage das Ende. Anthropic liefert
JSONL über `.../messages/batches/<id>/results`. OpenAI akzeptiert JSONL-Dateien
(auch Multipart-Feld `file`), liefert eine `input_file_id` und nach Abschluss eine
`output_file_id` mit Download über `/openai/v1/files/<id>/content`.
`batch-partial` lässt jeden zehnten Eintrag fehlschlagen (10 % bei 10/100/…);
`batch-expired` markiert alle Einträge als abgelaufen. Decisions als OpenAI-
Batch-Ziel wird mit 400 abgewiesen. Kein Polling benötigt reale Wartezeiten.

## Site und Preise

`site/manifest.json` listet genau 25 verlinkte Seiten-URLs (G), davon 22
indexierbare HTML-200-URLs (S), eine noindex-200-, eine 404- und eine 301-URL.
Alle Seiten verlinken das ganze Inventar. Es gibt sechs Bilder; die Startseite
enthält zwei passende Alt-Texte, einen unpassenden, einen leeren, einen fehlenden
und ein SVG. HTML-Dateien und `scenario.toml`-Bodies stimmen überein; die Bodies
ermöglichen extensionlose URLs ohne Änderung am allgemeinen Fixture-Server.
`TestSite::start("ai/site")` liefert sie unter `Host: ai.test` und freiem Port.

`site-partial/` ist bis auf zwei Produktmarker `500-always` und einen Ratgebermarker
`refusal` identisch. HTTP-Abrufe bleiben erfolgreich; die Marker betreffen erst KI.
Die kleinen Bilder stammen aus vorhandenen Clean-Room-Fixtures; SVG und GIF sind
eigene einfache Testbilder.

`pricing-test.json` enthält USD je Million Token, Batch-Rabatt als Anteil,
Langkontextmultiplikatoren und EU-Zuschlag als Anteil. Es sind ausschließlich die
runden Spec-Testpreise, keine Live-Preise. Die OpenRouter-Modellliste enthält die
zusätzlichen Request-/Bildpreise. Das Laden über `OPENSPIDER_AI_PRICING_FILE` und
dessen Beschränkung auf Test-Builds implementiert M4-K04.
