# Bericht M4-K01 – unabhängige Abnahmetests (AI-002, AI-005)

Stand: 24.11.2026, Zweig `ai/k01-tests` (Basis Bauer-Stand `5e03af6a`). Die Erwartungen stammen nur aus
`docs/spec/08-ai.md` (AI-002, AI-005, dazu die dort verwiesenen AI-001, AI-010, AI-016, AI-017, AI-020, AI-021,
INTEG-005 und die Szenariotabelle des ai-mock). Bericht, Tests und Mock-Fragment des Bauers habe ich nicht gelesen. Den
Produktionscode habe ich nur gelesen, um die öffentliche Schnittstelle zu kennen.

## Dateien

- `crates/os-ai/tests/acceptance_k01_anthropic.rs`: 22 Tests (AI-002)
- `crates/os-ai/tests/acceptance_k01_openrouter.rs`: 20 Tests (AI-005)
- `crates/os-ai/tests/acceptance_k01_support/mod.rs`: ai-mock starten und Protokoll lesen, Testuhr (`LimitClock`, Wartezeiten
  vergehen virtuell), Begrenzung mit Testuhr, kleiner lokaler Testserver nur auf tokio-Basis (keine neue Abhängigkeit)

Gegenstelle ist der ai-mock (`AiMock::fixtures()`), Szenarien über `MOCK-SCENARIO:<name>` im Eingabetext oder global über
`POST /__mock/scenario`. Einträge laufen wie im Lauf über `Limiter::run` (INTEG-005, Schutzschalter) und, bei
Schema-Aufgaben, über `schema::run_with_repair` (AI-021). Den lokalen Testserver brauche ich nur für das, was der Mock
nicht liefert: den Klartextwert von `x-api-key`, Cache-Tokens ungleich 0, HTTP 503 zum Vergleich, einmaliges 529, die
`/key`-Varianten, 402 ohne `limit_source` und 403 mit weiteren `limit_source`-Werten.

## Ergebnis des letzten Laufs (Mac mini, `cargo test --locked -p os-ai --no-fail-fast`)

- `acceptance_k01_anthropic`: 22 bestanden, 0 fehlgeschlagen
- `acceptance_k01_openrouter`: 20 bestanden, 0 fehlgeschlagen
- Gesamtlauf `os-ai`: EXIT 0, keine roten Tests (ignoriert sind nur die bestehenden `pending_*`- und `cost_*`-Platzhalter)
- `cargo fmt --all --check`: grün. `cargo clippy --locked -p os-ai --all-targets -- -D warnings`: grün

**Befunde (rote Tests): keine.**

## AI-002 Adapter Anthropic Messages API

| Kriterium | Test(s) | Ergebnis |
|---|---|---|
| K1 `x-api-key`, `anthropic-version`, `content-type: application/json` in jedem Request; Schlüssel in keinem Log | `k1_jeder_request_enthaelt_api_key_version_und_content_type`, `k1_x_api_key_traegt_den_schluessel_des_kontos`, `k1_schluessel_erscheint_in_keinem_log` | grün |
| K2 `output_config.format` mit `type: "json_schema"` und Schema; nie `output_format` | `k2_schema_aufgabe_sendet_output_config_format_json_schema` (dazu `beschreibung_systemtext_steht_im_top_level_feld_system`) | grün |
| K3 `stop_reason: "refusal"` → Refusal-Kennzeichen, kein Parse-Versuch | `k3_refusal_ergibt_kennzeichen_ohne_parse_versuch` | grün |
| K4 usage-Felder → einheitliche Felder | `k4_usage_felder_werden_in_die_einheitlichen_felder_uebertragen`, `k4_fehlende_usage_felder_ergeben_null_nicht_null_wert` | grün (zu `input_tokens` siehe Auslegungsfrage 1) |
| K5 `stop_reason: "max_tokens"` bei Schema-Aufgabe → `Error: Invalid output` | `k5_max_tokens_bei_schema_aufgabe_ergibt_invalid_output` | grün |
| K6 HTTP 529 wird wie 503 nach INTEG-005 wiederholt | `k6_529_dauerhaft_ergibt_1_plus_5_requests_wie_503`, `k6_529_im_mock_wird_wiederholt_und_einmaliges_529_endet_mit_success` | grün |
| K7 Kontingent-Szenarien: 1 Request (bei n gleichzeitigen ≤ n), alle offenen Anthropic-Einträge `Quota exceeded`, andere Anbieter laufen weiter | `k7_kontingent_szenarien_genau_ein_request_und_alle_offenen_eintraege_quota_exceeded` (alle 5 Szenarien), `k7_bei_n_gleichzeitigen_anfragen_hoechstens_n_requests` | grün |
| K8 `429-spend-cap` nicht wiederholt; `429x2` wiederholt → `Success` | `k8_429_spend_cap_wird_nicht_wiederholt`, `k8_429_ohne_spend_cap_wird_wiederholt_und_endet_mit_success` | grün |
| K9 `output_config.effort` nur bei A und B, bei B nie `max`, bei C/D nie; kein Top-Level-`effort` | `k9_effort_nur_bei_a_und_b_bei_b_nie_max_bei_c_und_d_nie` | grün |
| K10 C/D für Schema- und Bildaufgaben nicht wählbar (D „Fähigkeiten unbekannt“); C im Batch synchron; Änderung nach Neuladen | `k10_c_und_d_sind_fuer_schema_und_bildaufgaben_nicht_waehlbar`, `k10_schema_und_bildaufgaben_gehen_nie_an_c_oder_d`, `k10_aufgaben_mit_c_laufen_im_batch_modus_synchron`, `k10_geaenderte_capabilities_gelten_nach_erneutem_laden` | grün (Bibliotheksebene; den Oberflächenteil siehe unten) |
| K11 Bilder als Base64 mit korrektem `media_type` (PNG, JPEG, WebP, GIF) | `k11_bilder_werden_als_base64_mit_korrektem_media_type_gesendet` (Fixture-Bilder, Rückdekodierung byte-genau) | grün |
| AI-017 (verwiesen): Spend-Cap-429 zählt nicht für den Schutzschalter | `ai017_spend_cap_429_zaehlt_nicht_fuer_den_schutzschalter` (Schwelle 1, Gegenprobe mit 401) | grün |

## AI-005 Adapter OpenRouter

| Kriterium | Test(s) | Ergebnis |
|---|---|---|
| K1 8 Modelle; USD/Token → USD/1M (`"0.000003"` → `3.00`); `Per Request $ 0.0010`, `Per Image $ 0.0020` | `k1_verbinden_laedt_8_modelle_und_rechnet_preise_um`, `k1_preis_0_000003_je_token_ergibt_3_00_je_million_fehlende_felder_null` | grün |
| K2 Modelle ohne `structured_outputs`/`response_format` für Schema-Aufgaben nicht wählbar | `k2_modelle_ohne_structured_outputs_oder_response_format_nicht_waehlbar`, `k2_standardliste_embedding_modell_ist_fuer_schema_aufgaben_ausgegraut`, `k2_schema_aufgabe_an_modell_ohne_structured_output_wird_nicht_gesendet` | grün (Bibliotheksebene) |
| K3 `provider.require_parameters = true` nur bei Schema-Aufgaben | `k3_require_parameters_nur_bei_schema_aufgaben` | grün |
| K4 `provider.data_collection = "deny"` in jedem Request (Standard an) | `k4_datensammlung_ausschliessen_ist_standard_und_steht_in_jedem_request` (dazu `beschreibung_routing_praeferenzen_und_app_kennung`: `zdr`, `order` mit `allow_fallbacks: false`, App-Kennung standardmäßig aus) | grün |
| K5 `402-in-flight` → wartet ≥ 3 s (Testuhr), erneut gesendet, `Success`; kein anderer Eintrag `Quota exceeded` | `k5_402_in_flight_wartet_mindestens_3_s_und_endet_mit_success` | grün |
| K6 `402-credits`, `402-key-limit`, `403-guardrail` → alle offenen Einträge `Quota exceeded`, keine Wiederholung, ≤ n Requests, Kontostand neu geladen | `k6_endgueltige_kontingentfehler_beenden_alle_offenen_eintraege`, `k6_bei_n_gleichzeitigen_anfragen_hoechstens_n_requests`, dazu `beschreibung_402_ohne_limit_source_und_403_budgetquellen_sind_endgueltig` | grün |
| K7 `402-weight` → nur dieser Eintrag `Quota exceeded` | `k7_402_weight_betrifft_nur_diesen_eintrag` | grün |
| K8 Kontostand `limit_remaining` direkt (5.80, nicht 10 − 42), `Limit Reset monthly`; `null` = unbegrenzt | `k8_kontostand_zeigt_limit_remaining_direkt`, `k8_kontostand_ohne_limit_ist_unbegrenzt` | grün (Werte; der Anzeigetext „unbegrenzt“ ist Sache der Oberfläche) |
| K9 API-Preise vor Preistabelle; Preisquelle `api` | `k9_api_preise_haben_vorrang_und_preisquelle_ist_api` | grün (Preissatz; die Protokollzeile gehört zu K05) |
| K10 `refusal` (Form a) → Grund = Refusal-Text; `refusal-filter-only` → `content_filter`; kein Parse-Versuch; 1 Request | `k10_refusal_form_a_mit_text_und_nur_filter` | grün |
| K11 `refusal-403` (Form b) → `Refusal`, keine Wiederholung, übrige `Success`, Schutzschalter zählt nicht | `k11_refusal_403_ist_refusal_und_kein_fehler` | grün |
| AI-017 (verwiesen): endgültiges 402 zählt nicht für den Schutzschalter | `ai017_endgueltiges_402_zaehlt_nicht_fuer_den_schutzschalter` | grün |
| AI-001 (verwiesen): usage einheitlich, fehlende Cache-Felder `null` | `ai001_usage_einheitlich_fehlende_cache_felder_sind_null` | grün |

## Hier nicht prüfbar (kein Test, kein Platzhalter)

- **AI-002 K1, „kein Log von OpenSpider“:** Geprüft habe ich nur die Debug- und Display-Ausgaben von Adapter, Konfiguration,
  Ergebnis und Fehler sowie das Mock-Protokoll. Die Volltextsuche über alle Logdateien eines echten App-Laufs braucht die
  App.
- **AI-002 K7 und AI-005 K6, Einträge eines ganzen Laufs:** Geprüft ist die Wirkung über eine Begrenzung je Lauf mit
  `Limiter::run` und der Kontingent-Sperre des Adapters. Die Statusspalten aller Einträge eines echten Laufs braucht die
  Ausführung (D01).
- **AI-002 K10, Oberfläche:** Die Darstellung „ausgegraut“, der Hinweistext „Fähigkeiten unbekannt“ und die Warnung im
  Aufgabendialog gehören zur Oberfläche. Für „`pricing.json` enthält für Anthropic keine Fähigkeitsfelder (CI-Schema,
  AI-010)“ gibt es noch keine `resources/ai/pricing.json`; das prüft K04.
- **AI-002 K10, Batch-Modus im Lauf:** Geprüft ist nur `runs_in_batch()`. Ob C im Lauf wirklich synchron läuft, entscheidet
  AI-016 (Batch-Paket).
- **AI-005 K2, Oberfläche:** Modellauswahl mit Preisspalten, Ausgrauen und Hinweis.
- **AI-005 K6 und K8, Oberfläche:** Ob die Kontostandsanzeige neu gezeichnet wird und den Text „unbegrenzt“ zeigt, ist
  nicht geprüft. Geprüft ist das Neuladen von `GET /key` und der Wert `Remaining::Unlimited`.
- **AI-005 K9, Protokoll:** Die Spalte `Price Source` im Protokoll `ai_requests` gehört zu AI-014 (K05). Geprüft ist die
  Preisquelle `api` der aufgelösten Sätze und ihr Vorrang.
- **AI-019 (EU-Host, `refusal-403` am EU-Endpunkt):** nicht Teil von K01, nicht geprüft.

## Auslegungsfragen (bewusst nicht hart getestet)

1. **`Usage.input_tokens` bei Cache-Tokens (AI-002 K4).** Die Spec sagt nur, die vier Felder „werden übertragen“. Der Plan
   (`docs/plan/M4.md`, Rechenregeln AI-012/AI-014) legt fest: `input_tokens` enthält die aus dem Cache **gelesenen**
   Tokens. Bei Anthropic mit 100/30/40 (Eingabe, Cache-Lesen, Cache-Schreiben) wären das 130. Ob auch die Cache-**Schreib**-
   Tokens dazuzählen (dann 170), regelt weder Spec noch Plan. Der Test prüft nur `≥ 100`. Bitte in der Spec festlegen.
   Laut Schnittstellen-Kommentar rechnet der Code mit 170.
2. **Refusal-Kategorie aus `stop_details` (AI-002).** Die Spec nennt den Aufbau von `stop_details` nicht (Zeichenkette oder
   Objekt mit `category`?). Das Feld ist nicht getestet, und der Mock sendet es nicht.
3. **`max_tokens` und Reparaturversuch (AI-002 K5 und AI-021).** Offen ist, ob ein abgeschnittenes JSON einen
   Reparaturversuch auslöst. Geprüft ist nur der Endstatus `Error: Invalid output`, nicht die Zahl der Requests.
4. **Andere Bildformate (AI-002 K11).** Die Spec nennt nur PNG, JPEG, WebP und GIF. Das Verhalten bei SVG
   (`Skipped (unsupported format)`?) ist hier nicht festgelegt und nicht getestet.
5. **„Datensammlung ausschließen“ aus (AI-005).** Ob dann `data_collection` fehlt oder `"allow"` lautet, ist offen und
   nicht getestet.
6. **Quelle der OpenRouter-Vision-Fähigkeit (AI-005, AI-001).** Die Spec nennt kein Feld. Der Test stützt sich auf die
   Mock-Liste (`architecture.input_modalities`): Das Vision-Modell muss Vision haben, das Textmodell nicht.
7. **`Error Code` für Anthropic-Kontingentfälle.** AI-014 könnte dafür einen Wert verlangen; AI-002 nennt keinen. Nicht
   getestet.

## Abweichungen des ai-mock von der Spec

- **OpenRouter-Modellliste:** Die Spec-Fixture (AI-005, Testfixture) verlangt 3 Modelle ohne Structured Output und 2 mit
  Vision. Der Mock hat nur 1 ohne Structured Output (`mock/embedding`) und 1 mit Vision (`mock/vision`). Die
  Gesamtzahl 8 stimmt. K2 habe ich deshalb zusätzlich mit einer eigenen Liste über `set_models` geprüft.
- **`/key`:** Der Mock liefert immer denselben Stand (`limit 100`, `limit_remaining 90`, `usage 10`, ohne
  `limit_reset`). Die Varianten aus AI-005 K8 (periodisches Limit, `limit = null`) fehlen; ich habe sie über den lokalen
  Testserver geprüft.
- **Preisbeispiel:** AI-005 K1 nennt `"0.000003"` → `3.00`. Die Mock-Preise sind `"0.000001"`/`"0.000005"`; das Beispiel
  habe ich über `set_models` geprüft.
- **Anthropic-usage:** Der Mock sendet `cache_read_input_tokens` und `cache_creation_input_tokens` immer mit `0`. Damit
  lässt sich die Feldzuordnung aus K4 nicht unterscheiden; ich habe sie über den lokalen Testserver geprüft.
- **Anthropic-Refusal:** Der Mock sendet kein `stop_details`.
- **Anthropic-Modell A:** Es meldet die Stufen `low`, `medium`, `high`, `max`, aber nicht `xhigh`. Die Spec nennt die
  Stufen „`low` … `max`“. Für K9 reicht das.
- **Rate-Limit-Header** sendet der Mock nicht (bekannt). Für K01 war das nicht nötig.
- **Zusätzliche Szenarien:** `max-tokens` und `missing-usage` sind Erweiterungen des Mocks (siehe
  `tests/fixtures/ai/README.md`); K5 und der usage-Test nutzen sie.
