# M4-F02 – Unabhängige Abnahmetests

Zweig `ai/f02-tests` ab `f4fe100f` (Bau-Zweig M4-F02 + main). Erwartungen nur aus `docs/spec/05-ui.md` (UI-004 [M4], UI-013 [M4], UI-016), `docs/spec/10-agent-interface.md` AGENT-030 (Schalter „KI (Vorschau)“), `docs/spec/schemas/settings.schema.json` (`ai_app.preview_enabled`, Standard aus), `docs/plan/M4.md` (Abschnitt 2 E1/E-2 b, 3.3, Nachweis-Tabelle „Schalter“), `docs/plan/M4-packages.json` (M4-F02 `prompt_notes`) und der vom Orchestrator übermittelten OUT-203-Festlegung vom 23.11.2026 (Schalter und ausdrücklich genannte KI-Namen; in diesem Zweig noch nicht in `docs/spec`). Bericht, Commit-Texte, Diffs und Tests des Bauers (`apps/desktop/e2e/ai/`) wurden nicht gelesen. Produktcode nur für Aufrufwege (Selektoren, Command-Namen, CLI-Optionen, Test-Harness). Jeder Test trägt `// spec: <ID>` und `// acceptance: independent`.

Ausgeführt auf dem Mac mini (Baum `/Volumes/osbuild/openspider/wt/rel-1.0`, rsync aus dem Worktree, jeder schwere Befehl unter `lockf heavy.lock`, Playwright mit `--workers=1`, `OS_E2E_TRANSPORT=bridge`, Chromium und WebKit). Logs: `/Volumes/osbuild/openspider/wt/rel-1.0/f02-tests-log/` (erster Lauf unter `run1/`).

## 1. Ergebnis

Abschlusslauf (zweiter Lauf, Stand dieses Berichts): `f02-tests-log/{APP,MCP,CLI,PW,FMT,CLIPPY,TC}.log`; APP und MCP stammen aus dem ersten Lauf (Dateien danach unverändert), Logs des ersten Laufs unter `f02-tests-log/run1/`.

| Kriterium (Spec-ID / Planstelle) | Datei → Test | Ergebnis |
|---|---|---|
| M4.md 3.3: alle 15 Bereiche des Command-Katalogs haben einen Command | `crates/os-app/tests/acceptance_f02_ai_ipc.rs` → `every_area_of_the_ai_catalog_has_a_command` | grün (APP.log) |
| M4.md 3.3: `ai.cost.estimate|budget|log`, `ai.tester.run` vorhanden | … → `named_actions_cost_estimate_budget_log_and_tester_run_exist` | grün |
| M4.md 3.3 „Stubs, ts-rs“: KI-Commands in `commands.json`, Typen mit erzeugter Datei | … → `ai_commands_are_in_the_manifest_with_generated_types` | grün |
| M4.md 3.3: Stubs antworten `Unsupported`/`err-not-implemented` mit Command-Namen (Schalter aus und an) | … → `ai_stubs_answer_not_implemented_with_switch_off_and_on` | grün |
| settings.schema.json / E-2 b: Schalter Standard aus (IPC `settings.get`) | … → `preview_switch_is_off_by_default_over_ipc` | grün |
| Schalter über `settings.set` an/aus, übersteht Neustart | … → `preview_switch_roundtrips_through_settings_set_and_restart` | grün |
| AGENT-030: Schalter aus (Standard) → `tools/list` ohne die vier KI-Werkzeuge, auch mit `--allow cost` (`openspider-mcp`) | `crates/os-mcp/tests/acceptance_f02_ai_switch.rs` → `switch_off_by_default_hides_the_four_ai_tools_even_with_cost_allowed` | grün (MCP.log) |
| AGENT-030: Schalter an → alle vier; aus → übrige Werkzeuge unverändert | … → `switch_on_lists_all_four_and_off_keeps_every_other_tool` | grün |
| AGENT-030: Aufruf bei Schalter aus = Antwort für unbekanntes Werkzeug (auch `confirm: true`) | … → `switch_off_rejects_ai_tool_calls_like_an_unknown_tool` | grün |
| AGENT-030 Gegenprobe: bei Schalter an kein „unbekanntes Werkzeug“ | … → `switch_on_ai_tools_are_known_tools` | grün |
| AGENT-030 für `openspider mcp` (os-cli): `tools/list` folgt dem Schalter | `crates/os-cli/tests/acceptance_f02_ai_switch.rs` → `openspider_mcp_follows_the_switch_in_tools_list` | grün (CLI.log) |
| AGENT-030 für `openspider mcp`: Aufruf bei aus wie unbekanntes Werkzeug | … → `openspider_mcp_rejects_ai_tools_like_unknown_ones_when_off` | grün |
| E-2 b / OUT-203: Schalter aus → `--list-exports`, `--list-reports`, `--help export-tabs|bulk-export|save-report|export-custom-summary` ohne KI-Namen | … → `switch_off_lists_contain_no_ai_entries` | grün |
| UI-004 [M4] / OUT-203: Schalter an → jeder der vier KI-Tabs als Export-Tab gelistet | … → `switch_on_lists_offer_every_ai_tab` | grün |
| E-2 b: Schalter aus → `--help` nennt keine KI-Option (OUT-201/AI-022) | … → `switch_off_general_help_names_no_ai_option` | grün |
| Schalter an → `--help` nennt alle neun KI-Optionen | … → `switch_on_general_help_names_the_ai_options` | grün |
| OUT-203 (Festlegung 23.11.): Schalter aus, ausdrücklicher KI-Name (`--export-tabs`, `--bulk-export`, `--save-report`) → Exit 2 vor dem Crawl, Meldung „unknown …“, kein KI-Name in Vorschlag/Liste | … → `switch_off_explicit_ai_names_are_unknown_and_rejected_before_the_crawl` | **rot** – Produktnachtrag ausstehend (Befund 1) |
| OUT-203: Schalter an, dieselben Namen gültig | … → `switch_on_explicit_ai_names_are_valid` | grün |
| OUT-203: Schalter aus, `--export-custom-summary` mit KI-Kennzahl → Meldung „unknown …“ | … → `switch_off_explicit_ai_summary_metric_is_unknown` | **rot** (Befund 2) |
| OUT-203: Schalter an, KI-Kennzahl für `--export-custom-summary` gültig | … → `switch_on_explicit_ai_summary_metric_is_valid` | **rot** (Befund 2) |
| E-2 b / UI-004 [M4] / UI-013 [M4]: Schalter aus → keine KI-Tabs (Pillen und ▾-Listen aller Gruppen), kein Feld „Kosten“/„Cost“ (de/en), keine KI-Seite in „Konfiguration ▾“ und im Konfigurationsdialog, kein `ai.*`-Command über die Bridge | `apps/desktop/e2e/acceptance/f02-ai-switch.spec.ts` → „switch off (default): no AI tabs, no cost field, no AI configuration pages, no AI commands sent“ | grün (Chromium, WebKit; PW.log) |
| E-2 b / settings.schema.json: Schalter aus → kein Einstellungsabschnitt „KI“/„AI“; Schalter „KI (Vorschau)“ auf Allgemein, aus; keine `ai.*`-Commands | … → „switch off (default): settings have no AI section, …“ | grün (beide Engines) |
| UI-004 [M4], UI-016, UI-013 [M4], 3.3: Schalter an → vier KI-Tabs (übrige unverändert), je Leerzustand mit erlaubtem UI-016-Code, Abschnitt „KI“, KI-Seite im Konfigurationsdialog **und** im Einstieg „Konfiguration ▾“, Feld „Cost:“/„Kosten:“; Ausschalten entfernt alles wieder (nach Neuladen) | … → „switch on: four AI tabs with an empty state, …; off again removes them“ | **rot** (beide Engines) – nur das Teilkriterium „Konfiguration ▾“ (Befund 3); alle übrigen weichen Prüfungen dieses Tests grün |
| E-2 b: Umschalten im Einstellungsfenster wirkt im offenen Hauptfenster ohne Neuladen | … → „toggling the switch in the settings window updates the open main window without reload“ | **rot** (beide Engines) – Einschalten wirkt sofort, Ausschalten entfernt die KI-Tabs nicht (Befund 4) |

Summe: 30 Testfälle (22 Rust, 8 Playwright = 4 Tests × 2 Engines); 22 grün, 8 rot (3 Rust, 4 Playwright, davon je 2 gleiche Ursache je Engine). Zusätzlich auf dem Mini: `cargo fmt --all --check` grün (FMT.log), `cargo clippy --locked -p os-app -p os-mcp -p os-cli --tests -- -D warnings` grün (CLIPPY.log, erster Lauf 437 s), `npm run -w apps/desktop typecheck` grün (TC.log).

## 2. Befunde

1. **OUT-203: ausdrücklich genannter KI-Name wird bei Schalter aus angenommen** – Einordnung: Produktfehler, **Produktnachtrag ausstehend** (laut Orchestrator noch nicht umgesetzt). Alle drei Kataloge betroffen; der Crawl läuft (23 Requests an die Testsite) und schreibt die KI-Exportdatei mit 0 Zeilen. Wörtlich (gekürzt):
   `--export-tabs "AI:All": exit 0 (erwartet 2), Meldung „unknown … 'AI:All'“: false, Requests: 23, KI-Namen in Vorschlag/Liste: []` … `Export: …/out/ai_all.csv (0 Zeilen)` … `openspider: Fertig: 22 URLs gecrawlt, 1 Exportdateien, 0 Warnungen (Exit-Code 0)`; ebenso `--export-tabs "AI Decisions:All"`, `--bulk-export "AI Decisions:All Answers"`, `--save-report "AI:AI Crawler Accessibility"`.
2. **`--export-custom-summary` ist allgemein nicht unterstützt** – Einordnung: unsicher (vorbestehender Produktstand, kein F02-Fehler im engeren Sinn; die OUT-203-Festlegung nennt die Option aber ausdrücklich). Bei Schalter aus und an dieselbe Meldung, Exit 2 ohne Request: `Fehler: --export-custom-summary wird in dieser Version noch nicht unterstützt` / `openspider: --export-custom-summary wird in dieser Version noch nicht unterstützt (Exit-Code 2)`. Damit ist weder „unbekannter Name bei aus“ (Meldung fehlt) noch „gültig bei an“ erfüllt. Die Kennzahl `AI Decisions:All` steht bei Schalter an in `--help export-custom-summary`. Entscheidung nötig, ob die Option in 1.1 kommt oder die Festlegung sie bis dahin ausnimmt.
3. **Einstieg „Konfiguration ▾“ listet die KI-Konfigurationsseite nicht** – Einordnung: vermutlich Produktfehler (UI-004: „Konfiguration ▾“ ist der Einstieg in die Seiten der Crawl-Konfiguration; 3.3: Konfigurationsseiten registriert und mit Schalter sichtbar). Im Konfigurationsdialog selbst erscheint die Seite mit Schalter an (grün), im Menü nicht: `Error: Konfiguration ▾ an: Spider: Crawl,Spider: Extraction,Spider: Limits,Spider: Rendering,Spider: Advanced,Spider: Presets,robots.txt,URL rewriting,Include / Exclude,Speed,User-Agent,HTTP headers,Custom: Custom search,Custom: Custom extraction,… expect(received).toBeGreaterThan(expected) Expected: > 22 Received: 22`.
4. **Ausschalten im Einstellungsfenster entfernt die KI-Tabs im offenen Hauptfenster nicht** – Einordnung: unsicher (Spec/Plan verlangen nicht ausdrücklich „ohne Neustart“; Einschalten wirkt aber sofort, und das Feld „Kosten“ verschwindet sofort, nur die vier Tabs bleiben bis zum Neuladen). Nach Neuladen ist alles weg (Test 3 grün in diesem Teil). Wörtlich: `Error: expect(received).toBe(expected) // Object.is equality Expected: 0 Received: 4 … Timeout 15000ms exceeded while waiting on the predicate`.

## 3. Spec-Unklarheiten und Lücken

- **Command-Namen:** M4.md 3.3 schreibt `ai.cost.estimate`, `ai.tester.run`; der bestehende Command-Vertrag (M2, `manifest_contract`) verlangt genau einen Punkt (`<domäne>.<command>`). Die Tests akzeptieren beide Schreibweisen (`ai.cost.estimate` oder `ai.costEstimate`). Der Plan sollte die Form festlegen.
- **UI-013 vs. Platzhalter:** UI-013 zeigt `Kosten` „nur wenn für den Crawl KI-Aufgaben … aktiv sind“; M4.md 3.3/F02 verlangen einen Platzhalter, sichtbar mit Schalter. Ob der Platzhalter bei Schalter an ohne KI-Aufgaben erscheinen darf, ist offen; getestet ist nur der Feldname („Kosten:“/„Cost:“) mit Schalter an und das Fehlen mit Schalter aus. Das endgültige Zahlenformat (M4-U01) ist nicht geprüft.
- **Gruppe der KI-Tabs:** UI-004 sagt, spätere Tabs „erhalten ihre Gruppe im jeweiligen Meilenstein“; M4 legt keine fest. Die Tests suchen gruppenunabhängig (derzeit unter „Custom“).
- **Leerzustand der KI-Tabs ohne Crawl:** Der Code ist nicht festgelegt (z. B. `NO_DATA_YET` oder `API_NOT_CONNECTED(<anbieter>)`). Geprüft ist genau ein Code aus UI-016 mit Rang ≤ 8 (also nicht `NO_MATCH`, `NOT_APPLICABLE`, `NO_SELECTION`).
- **Namen von Abschnitt/Seite:** „KI“ (de) steht nur im Plan; der englische Name ist nicht festgelegt (Test zählt nur einen zusätzlichen Abschnitt). Ob die KI-Tab-Namen auf Deutsch übersetzt werden, ist offen; Tabs werden nur auf Englisch geprüft.
- **Umschalten ohne Neustart:** nicht festgelegt (siehe Befund 4).
- **`--api-budget`:** gemeinsames Budget für KI und DataForSEO (OUT-201); ob es bei Schalter aus in `--help` erscheinen darf, ist offen. Nicht geprüft.
- **KI-Optionen bei Schalter aus ausdrücklich gesetzt** (`--use-ai`, `--ai-tasks …`): Verhalten (ablehnen wie unbekannte Option oder ignorieren) nicht festgelegt; die OUT-203-Festlegung regelt nur Namen. Nicht geprüft.
- **Mock-Zähler 0 (AGENT-030):** nicht direkt gemessen; geprüft ist, dass der Aufruf bei Schalter aus schon an der Werkzeugauswahl scheitert (identische Antwort wie für einen unbekannten Namen). Im UI-Test ist geprüft, dass bei Schalter aus kein `ai.*`-Command über die Bridge geht.
- **Server der App (AGENT-030 „Server der App“):** der HTTP-MCP-Server der Desktop-App wurde nicht geprüft, nur `openspider-mcp` und `openspider mcp`.
- **OUT-203-Festlegung** steht in diesem Zweig noch nicht in `docs/spec/06-output-ops.md`; Grundlage war der Wortlaut des Orchestrators.

## 4. Dateien

| Datei | Art |
|---|---|
| `crates/os-app/tests/acceptance_f02_ai_ipc.rs` | neu |
| `crates/os-mcp/tests/acceptance_f02_ai_switch.rs` | neu |
| `crates/os-cli/tests/acceptance_f02_ai_switch.rs` | neu |
| `apps/desktop/e2e/acceptance/f02-ai-switch.spec.ts` | neu |
| `docs/plan/M4-reports/f02-tests.md` | neu (dieser Bericht) |
