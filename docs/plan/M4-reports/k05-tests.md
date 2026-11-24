# M4-K05 – unabhängige Abnahmetests (Kostenkern)

Testagent, Zweig `ai/k05-tests` (Basis `fd002cb0`). Erwartungen nur aus `docs/spec/08-ai.md` (AI-012, AI-013,
AI-014 mit AI-010 und „Konventionen“), `tests/fixtures/ai/README.md`, `tests/fixtures/ai/pricing-test.json`, dem
Paketauftrag M4-K05 (`prompt_notes`) und den Orchestrator-Auslegungen (a) Sperre nach Überschreitung der
Reservierung und (b) Korrekturfaktor = Median Ist/Schätzung der Eingabe-Tokens, letzte 200, ab 20, ohne Gebühren.
Bericht, Commits und Tests des Bauers wurden nicht gelesen; Produktcode nur für Signaturen.

Ausgeführt auf dem Mac mini (`/Volumes/osbuild/openspider/wt/ai-k05`, unter `heavy.lock`), Logs in
`/Volumes/osbuild/openspider/wt/ai-k05/k05-tests-log/`:

- `run1.log`: alle fünf Testprogramme (Stand `35028900`) – 58 grün.
- `run2.log`: `acceptance_k05_budget_store` mit dem zusätzlichen Test zur Sperre über Neuöffnen – 6 grün, 1 rot;
  Clippy rot (`type_complexity` in einem meiner Tests).
- `run3.log`: nach Korrektur Clippy `-p os-model -p os-store -p os-ai --all-targets -D warnings` grün,
  `acceptance_k05_reserve` erneut 11 grün.

Gesamt: **65 Tests, 64 grün, 1 rot.** Kein SIGBUS, keine Wiederholung nötig.

## 1. Kriterien → Tests

| Kriterium | Testdatei › Test | Ergebnis (Log) |
|---|---|---|
| AI-013 Standardwerte 5,00 USD / 80 % / nicht unbegrenzt | os-model `acceptance_k05_ledger` › `defaults_are_five_usd_and_eighty_percent`, `micro_usd_unit` | grün (run1) |
| AI-013 Budget 0,010, je 0,004 → genau 2 | os-model › `crawl_budget_ten_milli_allows_exactly_two_of_four_milli`, `open_reservations_count_against_budget`; os-ai `acceptance_k05_reserve` › `ten_milli_budget_with_formula_sends_exactly_two` | grün (run1) |
| AI-013 „KI gesamt“ 0,008 bei Crawl 1,00 → genau 2 | os-model › `ai_area_budget_allows_exactly_two` | grün (run1) |
| AI-013 Aufgaben-Teilbudget stoppt nur diese Aufgabe | os-model › `task_budget_stops_only_that_task` | grün (run1) |
| AI-013 Budget 0 → nur Beträge 0 | os-model › `zero_budget_allows_only_zero_amounts`; os-ai › `zero_budget_runs_only_free_rates` | grün (run1) |
| AI-013 „Unbegrenzt“ nur ausdrücklich, Teilbudgets wirken weiter | os-model › `unlimited_ignores_crawl_budget_but_keeps_sub_budgets`; os-store › `unlimited_and_zero_roundtrip` | grün (run1) |
| AI-013 Gebühren reserviert: 0,005 Budget, 0,001 + 0,002 → genau 1 | os-ai › `fees_reserved_budget_five_milli_allows_one_image_request`, `reservation_includes_request_and_image_fees` | grün (run1) |
| AI-013 Überschreitung: Mehrbetrag verbucht, `over_reservation`, danach crawl-weit keine bezahlte Reservierung, Beträge 0 weiter, Budgeterhöhung hebt auf (Auslegung a) | os-model › `over_reservation_books_excess_and_halts_paid_calls_until_raised`; os-ai `acceptance_k05_log` › `over_reservation_flag_in_log` | grün (run1) |
| AI-013 Sperre nach Überschreitung gilt nach Neuöffnen weiter (Auslegung a) | os-store `acceptance_k05_budget_store` › `halt_after_over_reservation_survives_reopen` | **rot** (run2), siehe Befund B1 |
| AI-013 Nebenläufigkeit: 4 Threads, 200 Einträge, Σ Ist ≤ Budget (8 Budgets × 6 Startwerte × 2 Größen; Teilbudgets; mit Formel 5 Budgets × 5 Startwerte) | os-model › `parallel_workers_never_exceed_crawl_budget`, `parallel_workers_never_exceed_sub_budgets`; os-ai › `parallel_workers_with_formula_never_exceed_budget` | grün (run1) |
| AI-013 Budget wird bei korrektem Katalog nie überschritten (Ist mit ≤ reservierten Tokens ≤ Reservierung; Property-Test 2.000 Fälle inkl. Rundung, Gebühren, Zuschlag, Batch, Langkontext) | os-ai › `actual_within_reserved_tokens_never_exceeds_reservation` | grün (run1, run3) |
| AI-013 Grenzwerte und Verbrauch überstehen Schließen/Neuöffnen | os-store › `limits_survive_close_and_reopen`, `spend_survives_reopen_and_limits_ledger`; os-model › `reopened_ledger_counts_previous_spend_of_all_areas` | grün (run1) |
| AI-013 Freigeben gibt Betrag zurück; Ist ersetzt Reservierung | os-model › `release_returns_amount`, `settle_replaces_reservation_with_actual` | grün (run1) |
| AI-013 Warnschwelle 80 % | os-model › `warning_at_eighty_percent` | grün (run1) |
| AI-013 Reservierungsformel (× 1,25, `max_tokens`, Batch-Rabatt, EU-Zuschlag, Langkontext ab × 1,25 ≥ Schwelle) mit `pricing-test.json` | os-ai › `fixture_prices_match_spec_conventions`, `reservation_basic_formula_mock_claude`, `reservation_with_batch_discount`, `reservation_with_regional_uplift`, `reservation_long_context_threshold_uses_inflated_estimate` | grün (run1) |
| AI-014 Summenzeile 3 × 1.000/200 mock-claude → 3000 / 600 / 6.000 µUSD | os-ai `acceptance_k05_log` › `totals_row_three_mock_claude_answers`, `totals_are_grouped_by_task_and_model` | grün (run1) |
| AI-014 `Cost = Token Cost + Fixed Fees + Regional Uplift` je Zeile (Property-Test 3.000 Fälle) | os-ai › `every_row_cost_is_sum_of_parts`; os-store › `stored_rows_keep_cost_sum_invariant` | grün (run1) |
| AI-014 OpenRouter-Bildrequest 1 Bild → `Fixed Fees` 3.000 µUSD | os-ai › `openrouter_image_request_fixed_fees` | grün (run1) |
| AI-014 Regionalzuschlag als Spalte, Langkontext (AI-010-Prüfwerte 200/100/60.750 µUSD, EU +10 %) | os-ai › `regional_uplift_and_long_context_columns` | grün (run1) |
| AI-014 Cache-Treffer-Zeile `Cache Hit = true`, `Cost = 0`, kein HTTP-Status | os-ai › `cache_hit_row_costs_zero` | grün (run1) |
| AI-014 Wiederholungen als eine Zeile `Retries = n`; Fehlversuche ohne `usage` kosten 0 | os-ai › `retries_one_row_and_failed_attempts_free` | grün (run1) |
| AI-014 ohne `usage`: Tokens `null`, `Price Source = estimate` | os-ai › `missing_usage_is_estimate_with_null_tokens` | grün (run1) |
| AI-014 `Refusal Reason` ≤ 500 Zeichen | os-ai › `refusal_reason_truncated_in_log`; os-store › `refusal_reason_is_truncated_to_500_chars` | grün (run1) |
| AI-014 Protokollsumme = KI-Verbrauch des Ledgers auf 1 µUSD | os-ai › `log_sum_matches_ledger_to_the_micro_usd`; os-store › `log_sum_equals_ledger_ai_share` | grün (run1) |
| AI-012 Token-Schätzung ⌈Zeichen / 3,5⌉ (Unicode), + Bild-Schätzwert + Anweisungen | os-ai `acceptance_k05_estimate` › `text_token_estimate_is_ceiling_of_chars_over_3_5`, `input_estimate_adds_images_and_instructions` | grün (run1) |
| AI-012 niedrig/hoch × 0,75/1,25 bzw. × 0,9/1,1 | os-ai › `uncalibrated_spread_is_075_125`, `calibrated_spread_is_09_11_and_factor_scales_input` | grün (run1) |
| AI-012 Cache-Treffer in `Cached`, Kosten 0 | os-ai › `cache_hits_counted_and_free` | grün (run1) |
| AI-012 Bildaufgabe 3 × (0,001 + 0,002) = 9.000 µUSD + Tokenkosten; Gebühren nicht im Korrekturfaktor | os-ai › `image_task_fees_are_nine_thousand_plus_tokens`, `calibration_factor_does_not_scale_fees` | grün (run1) |
| AI-012 `Executable = nein` mit `Price unknown` / `Missing API key`; „Preis unbekannt“ ohne Beträge | os-ai › `not_executable_reasons` | grün (run1) |
| AI-012 Bestätigung über Schwelle (Standard 1,00 USD) oder über 50 % des Restbudgets | os-ai › `default_confirmation_threshold_is_one_usd`, `confirmation_rule_threshold_or_half_of_remaining` | grün (run1) |
| AI-012 Kalibrierung ab 20, Median, letzte 200, je Modell, aus dem Protokoll, ohne Gebühren (Auslegung b) | os-ai › `calibration_needs_twenty_samples`, `calibration_is_median_not_mean`, `calibration_uses_last_two_hundred`, `calibration_from_log_per_model_ignores_fees` | grün (run1) |
| AI-012 Batch-Ersparnis | os-ai › `batch_savings_column` | grün (run1) |

Fixture: alle Prüfwerte in `acceptance_k05_reserve`, `acceptance_k05_log` und `acceptance_k05_estimate` stammen aus
`tests/fixtures/ai/pricing-test.json`, gelesen über einen eigenen Loader in
`crates/os-ai/tests/acceptance_k05_support/mod.rs` (nicht über den Produktcode). Das OpenRouter-Vision-Modell ist
mit den Spec-Werten (0,001 je Request, 0,002 je Bild) im Helfer angelegt; die Tokenpreise (1/5 USD je 1 Mio.)
entsprechen der Mock-Modellliste. Neue Fixtures unter `tests/fixtures/ai/k05-acceptance/` waren nicht nötig.

## 2. Befunde

**B1 – Sperre nach Überschreitung geht beim Neuöffnen verloren** (rot:
`os-store/tests/acceptance_k05_budget_store.rs` › `halt_after_over_reservation_survives_reopen`, `run2.log`).
Einordnung: **Spec-Unklarheit, vermutlich Lücke im Produkt.** Nach einer Ist-Antwort über der Reservierung
sperrt der Ledger im Speicher korrekt. Wird die Crawl-Datei geschlossen und der Ledger aus `api_budget` und
`ai_requests` neu aufgebaut (`CrawlBudgetLedger::open` mit `AiRequestsSpend`), ist die Sperre weg, obwohl die
Zeile mit `over_reservation = true` im Protokoll steht und das Budget nicht erhöht wurde. Nach Auslegung (a)
(„… bis das Budget erhöht wird“) wäre ein Neustart ein Umgehungsweg. Die Spec sagt nur „nach Neustart und
‚Fortsetzen‘ gilt der bisherige Verbrauch weiter“, nichts zur Sperre. Wörtliche Meldung:

```
thread 'halt_after_over_reservation_survives_reopen' panicked at crates/os-store/tests/acceptance_k05_budget_store.rs:316:5:
nach Neuöffnen ohne Budgeterhöhung darf keine bezahlte Reservierung durchgehen: Ok(Reservation { id: ReservationId(1), scope: BudgetScope { area: "ai", task: Some("decision") }, maximum: MicroUsd(1) })
```

Zu entscheiden: Soll die Sperre einen Neustart überdauern (z. B. aus `over_reservation`-Zeilen nach der letzten
Grenzwertänderung abgeleitet), oder hebt „Fortsetzen“ sie auf? Wird Letzteres entschieden, ist der Test zu
streichen, nicht zu lockern.

Sonst keine Abweichung zwischen Spec und Code gefunden.

## 3. Spec-Unklarheiten, Lücken, nicht prüfbar

Unklarheiten (nicht geraten; Tests meiden die Grenzfälle oder halten sich an die eindeutige Lesart):

1. **Rundung unter 1 µUSD** ist nirgends festgelegt (Ist-Kosten, Reservierung, ⌈geschätzt × 1,25⌉, niedrig/hoch).
   Die Tests nutzen Prüfwerte ohne Bruchteile; der Property-Test prüft nur „Ist ≤ Reservierung“.
2. **Median bei gerader Anzahl** (z. B. genau 20 Requests) ist nicht definiert; die Tests verwenden ungerade
   Anzahlen oder gleiche Mittelwerte.
3. **„bei Batch mit Rabatt“ (AI-013):** ob der Rabatt auch Gebühren je Request/Bild betrifft, bleibt offen
   (AI-010 nennt den Rabatt nur bei Tokenpreisen). Batch-Tests ohne Gebühren.
4. **Warnschwelle:** „Warnung bei Schwelle“ – erreicht (≥) oder überschritten (>)? Getestet: 78 % keine Warnung,
   genau 80 % Warnung.
5. **Vorrang bei fehlendem Schlüssel und fehlendem Preis zugleich** (`Executable`-Grund) ist offen; nicht getestet.
6. **Kalibrierung und Cache-Treffer/Zeilen ohne `usage`:** ob sie als „protokollierte Requests“ im Fenster der
   letzten 200 zählen, sagt die Spec nicht; nicht getestet.
7. **Langkontext und Cache-Lesen:** AI-010 nennt einen eigenen Faktor für Cache-Lesen, `pricing-test.json` enthält
   für `gpt-6-luna/responses` keinen. Der Test-Loader setzt 1,0; kein Test hängt davon ab.
8. **Fixture-Lücke (AI-010, nicht K05):** Die Testfixture-Zeile von AI-010 verlangt einen Eintrag mit
   `verified_at` vor 200 Tagen; `pricing-test.json` hat keinen.
9. **Sperre über Neustart** – siehe B1.

In dieser Etappe nicht prüfbar (brauchen Lauf, Adapter, Mock-Server oder Oberfläche):

- AI-013: Mock-Zähler = 2 und Status `Budget exceeded` an allen offenen Einträgen im Lauf; „Budget erhöhen und
  fortsetzen“ sendet nur `Budget exceeded`-Einträge; Statusleistenfeld `Kosten` (Format UI-013, Tooltip KI /
  DataForSEO / Reservierungen), Warnanzeige; Lasttest gegen `ai-mock` mit großer `usage.output_tokens`;
  Budget 0 mit echten Ollama-Aufgaben und Cache-Treffern im Lauf.
- AI-012: Dialog „Kostenvoranschlag“, `Pending (confirmation)` und „Starten“; `Items = 22` an der Fixture-Site
  und ±25 % / ±5 % gegen protokollierte Ist-Kosten des Mocks; keine Inhalte an Anbieter ohne Stichprobenoption
  (Mock-Log); Hochrechnung je 100 Seiten im laufenden Crawl; Stichprobe über `count_tokens`; Hinweis „Preis
  möglicherweise veraltet“ (AI-010); Lauf mit Budget startet bei unbekanntem Preis erst nach Preiseingabe.
- AI-014: Ansicht „KI-Protokoll“ mit Filtern, Export CSV/XLSX „AI Request Log“ und Textform `Cost 0.006000`
  (in dieser Etappe gibt es keine Formatierungs- oder Exportfunktion; geprüft ist der Betrag 6.000 µUSD);
  Volltextsuche nach Fixture-Text in `ai_requests`; Mock-Log unverändert bei Cache-Treffer; `429x2` im Lauf.
- Nicht in dieser Etappe: alle `[M5]`-Kriterien und INTEG-176. Zusätzlich geprüft, nur als Schnittstelle laut
  `prompt_notes`: ein zweiter Bereich meldet seinen Verbrauch ohne `os-ai` an und zählt ins gemeinsame
  Crawl-Budget (`reopened_ledger_counts_previous_spend_of_all_areas`).

## 4. Angelegte Dateien

| Datei | Inhalt |
|---|---|
| `crates/os-model/tests/acceptance_k05_ledger.rs` | 15 Tests `BudgetLedger` (AI-013) |
| `crates/os-store/tests/acceptance_k05_budget_store.rs` | 7 Tests `api_budget`, `ai_requests`, Neuöffnen (AI-013, AI-014) |
| `crates/os-ai/tests/acceptance_k05_support/mod.rs` | Helfer: eigener Loader für `pricing-test.json`, Crawl-Datei, LCG |
| `crates/os-ai/tests/acceptance_k05_reserve.rs` | 11 Tests Reservierung und Lasttest (AI-013) |
| `crates/os-ai/tests/acceptance_k05_estimate.rs` | 15 Tests Kostenvoranschlag und Kalibrierung (AI-012) |
| `crates/os-ai/tests/acceptance_k05_log.rs` | 11 Tests Protokoll (AI-014, AI-013 `over_reservation`) |
| `docs/plan/M4-reports/k05-tests.md` | dieser Bericht |
