# M4-F02 – Unabhängige Abnahmetests

Zweig `ai/f02-tests` ab `f4fe100f` (Bau-Zweig M4-F02 + main); Lauf 2 auf dem integrierten Stand `4262f276` (Bauer-Nachträge + main mit den Klarstellungen vom 23.11.2026) plus eigene Testcommits. Erwartungen nur aus `docs/spec/05-ui.md` (UI-004 [M4], UI-013 [M4], UI-016), `docs/spec/10-agent-interface.md` AGENT-030 (Schalter „KI (Vorschau)“), `docs/spec/schemas/settings.schema.json` (`ai_app.preview_enabled`, Standard aus), `docs/plan/M4.md` (Abschnitt 2 E1/E-2 b, 3.3, Nachweis-Tabelle „Schalter“), `docs/plan/M4-packages.json` (M4-F02 `prompt_notes`) und `docs/spec/06-output-ops.md` OUT-203 (OS-Festlegung Schalter „KI (Vorschau)“ vom 23.11.2026, in Lauf 1 als Wortlaut des Orchestrators, seit `4262f276` in der Spec) und den Nachträgen 23.11.2026 in `docs/plan/M4.md` 3.3 (Command-Namen mit einem Punkt, Umschalten ohne Neuladen, KI-Seite im Menü „Konfiguration ▾“). Bericht, Commit-Texte, Diffs und Tests des Bauers (`apps/desktop/e2e/ai/`) wurden nicht gelesen. Produktcode nur für Aufrufwege (Selektoren, Command-Namen, CLI-Optionen, Test-Harness). Jeder Test trägt `// spec: <ID>` und `// acceptance: independent`.

Ausgeführt auf dem Mac mini (Baum `/Volumes/osbuild/openspider/wt/rel-1.0`, rsync aus dem Worktree, jeder schwere Befehl unter `lockf heavy.lock`, Playwright mit `--workers=1`, `OS_E2E_TRANSPORT=bridge`, Chromium und WebKit). Logs: Lauf 1 unter `/Volumes/osbuild/openspider/wt/rel-1.0/f02-tests-log/run1/` (erster Durchgang) und `f02-tests-log/*.log` (zweiter Durchgang von Lauf 1); Lauf 2 unter `f02-tests-log/run2/`, Lauf 3 (Stand `e52898b4` + eigene Testcommits, Nachträge 7 und 8 in M4.md 3.3) unter `f02-tests-log/run3/` (`APP.log`, `MCP.log`, `CLI.log`, `BRIDGE.log`, `PW.log`, `FMT.log`, `CLIPPY.log`, `TC.log`, `steps.txt`).

## 1. Ergebnis

| Kriterium (Spec-ID / Planstelle) | Datei → Test | Lauf 1 | Lauf 2 (`run2/`) |
|---|---|---|---|
| M4.md 3.3: alle 15 Bereiche des Command-Katalogs haben einen Command | `crates/os-app/tests/acceptance_f02_ai_ipc.rs` → `every_area_of_the_ai_catalog_has_a_command` | grün | grün (APP.log) |
| M4.md 3.3 + Nachtrag (1): `ai.costEstimate`, `ai.costBudget`, `ai.costLog`, `ai.testerRun` | … → `named_actions_cost_estimate_budget_log_and_tester_run_exist` | grün (damals beide Schreibweisen zugelassen) | grün (nur Ein-Punkt-Form) |
| Nachtrag (1): jeder KI-Command hat genau einen Punkt | … → `every_ai_command_name_has_exactly_one_dot` | – (neu) | grün |
| M4.md 3.3 „Stubs, ts-rs“: KI-Commands in `commands.json`, Typen mit erzeugter Datei | … → `ai_commands_are_in_the_manifest_with_generated_types` | grün | grün |
| M4.md 3.3: Stubs antworten `Unsupported`/`err-not-implemented` mit Command-Namen (Schalter aus und an) | … → `ai_stubs_answer_not_implemented_with_switch_off_and_on` | grün | grün |
| settings.schema.json / E-2 b: Schalter Standard aus (IPC `settings.get`) | … → `preview_switch_is_off_by_default_over_ipc` | grün | grün |
| Schalter über `settings.set` an/aus, übersteht Neustart | … → `preview_switch_roundtrips_through_settings_set_and_restart` | grün | grün |
| AGENT-030: Schalter aus (Standard) → `tools/list` ohne die vier KI-Werkzeuge, auch mit `--allow cost` (`openspider-mcp`) | `crates/os-mcp/tests/acceptance_f02_ai_switch.rs` → `switch_off_by_default_hides_the_four_ai_tools_even_with_cost_allowed` | grün | grün (MCP.log) |
| AGENT-030: Schalter an → alle vier; aus → übrige Werkzeuge unverändert | … → `switch_on_lists_all_four_and_off_keeps_every_other_tool` | grün | grün |
| AGENT-030: Aufruf bei Schalter aus = Antwort für unbekanntes Werkzeug (auch `confirm: true`) | … → `switch_off_rejects_ai_tool_calls_like_an_unknown_tool` | grün | grün |
| AGENT-030 Gegenprobe: bei Schalter an kein „unbekanntes Werkzeug“ | … → `switch_on_ai_tools_are_known_tools` | grün | grün |
| AGENT-030 für `openspider mcp` (os-cli): `tools/list` folgt dem Schalter | `crates/os-cli/tests/acceptance_f02_ai_switch.rs` → `openspider_mcp_follows_the_switch_in_tools_list` | grün | grün (CLI.log) |
| AGENT-030 für `openspider mcp`: Aufruf bei aus wie unbekanntes Werkzeug | … → `openspider_mcp_rejects_ai_tools_like_unknown_ones_when_off` | grün | grün |
| OUT-203 / E-2 b: Schalter aus → `--list-exports`, `--list-reports`, `--help export-tabs|bulk-export|save-report|export-custom-summary` ohne KI-Namen | … → `switch_off_lists_contain_no_ai_entries` | grün | grün |
| UI-004 [M4] / OUT-203: Schalter an → jeder der vier KI-Tabs als Export-Tab gelistet | … → `switch_on_lists_offer_every_ai_tab` | grün | grün |
| E-2 b: Schalter aus → `--help` nennt keine KI-Option (OUT-201/AI-022) | … → `switch_off_general_help_names_no_ai_option` | grün | grün |
| Schalter an → `--help` nennt alle neun KI-Optionen | … → `switch_on_general_help_names_the_ai_options` | grün | grün |
| OUT-203: Schalter aus, ausdrücklicher KI-Name (`--export-tabs`, `--bulk-export`, `--save-report`) → Exit 2 vor dem Crawl, Meldung „unknown …“, kein KI-Name in Vorschlag/Liste | … → `switch_off_explicit_ai_names_are_unknown_and_rejected_before_the_crawl` | **rot** (Befund 1) | grün |
| OUT-203: Schalter an, dieselben Namen gültig | … → `switch_on_explicit_ai_names_are_valid` | grün | grün |
| OUT-203 (Klarstellung): Schalter aus, `--export-custom-summary` mit KI-Kennzahl → Exit 2 mit Meldung für unbekannten Namen | … → `switch_off_explicit_ai_summary_metric_is_unknown` | **rot** (Befund 2) | grün |
| OUT-203 (Klarstellung): Schalter an, KI-Kennzahl → Exit 2 mit „noch nicht unterstützt“, nicht „unknown“, 0 Requests | … → `switch_on_ai_summary_metric_ends_with_not_supported` (Lauf 1: `switch_on_explicit_ai_summary_metric_is_valid`, vor der Klarstellung) | **rot** (Befund 2) | grün |
| E-2 b / UI-004 [M4] / UI-013 [M4]: Schalter aus → keine KI-Tabs (Pillen und ▾-Listen aller Gruppen), kein Feld „Kosten“/„Cost“ (de/en), keine KI-Seite in „Konfiguration ▾“ und im Konfigurationsdialog, kein `ai.*`-Command über die Bridge | `apps/desktop/e2e/acceptance/f02-ai-switch.spec.ts` → „switch off (default): no AI tabs, no cost field, no AI configuration pages, no AI commands sent“ | grün (Chromium, WebKit) | grün (Chromium, WebKit; PW.log) |
| E-2 b / settings.schema.json: Schalter aus → kein Einstellungsabschnitt „KI“/„AI“; Schalter „KI (Vorschau)“ auf Allgemein, aus; keine `ai.*`-Commands | … → „switch off (default): settings have no AI section, …“ | grün (beide) | grün (beide) |
| UI-004 [M4], UI-016, UI-013 [M4], 3.3 + Nachtrag (6): Schalter an → vier KI-Tabs (übrige unverändert), je Leerzustand mit erlaubtem UI-016-Code, Abschnitt „KI“, KI-Seite im Konfigurationsdialog und im Menü „Konfiguration ▾“, Feld „Cost:“/„Kosten:“; Ausschalten entfernt alles wieder (nach Neuladen) | … → „switch on: four AI tabs with an empty state, …; off again removes them“ | **rot** (beide; nur „Konfiguration ▾“, Befund 3) | grün (beide) |
| Nachtrag (5): Umschalten im Einstellungsfenster wirkt im offenen Hauptfenster ohne Neuladen (KI-Tabs, Feld „Kosten“) | … → „toggling the switch in the settings window updates the open main window without reload“ | **rot** (beide; Befund 4) | grün (beide) |
| Nachtrag (5)/(6): Umschalten ohne Neuladen für Einstellungsabschnitt „KI“ und KI-Konfigurationsseiten (Dialog und Menü) | … → „toggling the switch adds and removes the AI settings section and configuration pages without reload“ | – (neu nach Nachtrag 5) | grün (beide) |

**Lauf 1:** 28 Testfälle, 21 grün, 7 rot. **Lauf 2:** 31 Testfälle (21 Rust: os-app 7, os-mcp 4, os-cli 10; 10 Playwright = 5 Tests × Chromium/WebKit), **alle grün**. Ebenfalls in Lauf 2 grün: `cargo fmt --all --check` (FMT.log), `cargo clippy --locked -p os-app -p os-mcp -p os-cli --tests -- -D warnings` (CLIPPY.log), `npm run -w apps/desktop typecheck` (TC.log), Bridge-Bau (BRIDGE.log). Playwright mit `OS_E2E_TRANSPORT=bridge`, `--workers=1`.

## 1a. Lauf 3 (Nachträge 7 und 8, `f02-tests-log/run3/`)

Geändert vor Lauf 3 (Spec-Entscheidung, kein Lockern): Feld „Kosten“ nur mit Schalter an **und** aktiven KI-Aufgaben (Nachtrag 7). KI-Aufgaben werden von außen über die Bridge gesetzt (`config.get`, `config.ai.enabled = true`, `config.ai.suggestions.title = true`, `config.set`, ohne geöffneten Crawl auf den Entwurf des Fensters). Neu geprüft sind außerdem die Namen nach Nachtrag 8.

| Kriterium | Test | Lauf 3 |
|---|---|---|
| alle Rust-Abnahmetests (os-app 7, os-mcp 4, os-cli 10) | wie oben | grün (APP.log, MCP.log, CLI.log) |
| Nachtrag 7: Schalter aus, auch mit aktiven KI-Aufgaben → kein Feld „Kosten“ | „switch off (default): no AI tabs, no cost field, …“ | grün (Chromium, WebKit) |
| Schalter aus: Einstellungen ohne Abschnitt „KI“/„AI“, Schalter „KI (Vorschau)“ aus | „switch off (default): settings have no AI section, …“ | grün (beide) |
| Nachtrag 7: Schalter an ohne KI-Aufgaben → kein Feld; mit aktiven KI-Aufgaben → „Cost:“ (en) / „Kosten:“ (de). Nachtrag 8: Abschnitt heißt „KI“ (de) und „AI“ (en). Alle übrigen Prüfungen wie in Lauf 2 | „switch on: four AI tabs with an empty state, …; off again removes them“ | grün in allen genannten Teilen (beide Engines) |
| Nachtrag 8: die vier KI-Tab-Namen bleiben auf Deutsch englisch | ebenda (weiche Prüfung) | **rot** (beide Engines, Befund 5) |
| Nachtrag 5/7: ohne KI-Aufgaben kein Feld; nach dem Aktivieren erscheint es ohne Neuladen; Ausschalten entfernt KI-Tabs und Feld sofort | „toggling the switch in the settings window updates the open main window without reload“ | grün (beide) |
| Nachtrag 5/6: Abschnitt und KI-Konfigurationsseiten kommen und gehen ohne Neuladen | „toggling the switch adds and removes the AI settings section and configuration pages without reload“ | grün (beide) |

**Lauf 3:** 31 Testfälle, 29 grün und 2 rot (derselbe Test auf Chromium und WebKit, nur das Teilkriterium „KI-Tab-Namen auf Deutsch englisch“). `cargo fmt --all --check`, clippy (`-D warnings`, os-app/os-mcp/os-cli `--tests`), Typecheck und Bridge-Bau sind grün.

**Befund 5 – KI-Tab-Namen werden auf Deutsch übersetzt (Nachtrag 8).** Einordnung: **Spec-Unklarheit, Entscheidung nötig.** Nachtrag 8 verlangt englische KI-Tab-Namen „wie die übrigen Tab-Namen“; im Produkt sind aber auch die übrigen Tab-Namen auf Deutsch übersetzt („Intern“, „Antwortcodes“, „Eigene Suche“ …). Die Begründung trägt also nicht. Entweder sollen die KI-Tabs ausnahmsweise englisch bleiben (dann Produktfehler), oder sie werden wie alle Tabs übersetzt (dann Nachtrag 8 anpassen). Der Test bleibt spec-treu rot. Wörtlich (Chromium, gleich in WebKit):
`Error: AI auf Deutsch: Intern,Extern,Sicherheit,Antwortcodes,URL,Seitentitel,Meta-Beschreibung,Meta-Keywords,H1,H2,Inhalt,Bilder,Strukturierte Daten,Canonicals,Paginierung,Direktiven,Hreflang,JavaScript,Sitemaps,Mobil,Barrierefreiheit,Validierung,Links,Link-Metriken,PageSpeed,Analytics,Search Console,Eigene Suche,Eigene Extraktion,Eigenes JavaScript,KI,KI-Entscheidungen,KI-Vorschläge,KI-Suche` – `Expected: true Received: false`; ebenso für `AI Decisions`, `AI Suggestions`, `AI Search`.

## 2. Befunde

Lauf 2: keine roten Tests. Lauf 3: Befund 5 (oben, offen). Die Befunde aus Lauf 1 und ihre Erledigung:

1. **OUT-203: ausdrücklich genannter KI-Name wurde bei Schalter aus angenommen** – Produktfehler; Bauer-Nachtrag, in Lauf 2 grün. Lauf 1 wörtlich (gekürzt): `--export-tabs "AI:All": exit 0 (erwartet 2), Meldung „unknown … 'AI:All'“: false, Requests: 23` … `openspider: Fertig: 22 URLs gecrawlt, 1 Exportdateien, 0 Warnungen (Exit-Code 0)`; ebenso `AI Decisions:All`, `--bulk-export "AI Decisions:All Answers"`, `--save-report "AI:AI Crawler Accessibility"`.
2. **`--export-custom-summary` allgemein nicht unterstützt** – Spec-Unklarheit; durch die Klarstellung in OUT-203 entschieden (immer Exit 2; bei aus + KI-Kennzahl Meldung für unbekannten Namen, sonst „noch nicht unterstützt“). Beide Tests danach angepasst (Spec-Entscheidung, kein Lockern), in Lauf 2 grün. Lauf 1 wörtlich: `Fehler: --export-custom-summary wird in dieser Version noch nicht unterstützt` (Exit 2, auch bei Schalter aus).
3. **Menü „Konfiguration ▾“ ohne KI-Seite** – Produktfehler (Nachtrag 6); Bauer-Nachtrag, in Lauf 2 grün. Lauf 1 wörtlich: `Konfiguration ▾ an: Spider: Crawl,… Expected: > 22 Received: 22`.
4. **Ausschalten entfernte die KI-Tabs im offenen Hauptfenster nicht** – Produktfehler (Nachtrag 5); Bauer-Nachtrag, in Lauf 2 grün. Lauf 1 wörtlich: `Expected: 0 Received: 4 … Timeout 15000ms exceeded while waiting on the predicate`.

## 3. Spec-Unklarheiten und Lücken

Entschieden (23.11.2026): Feld „Kosten“ nur mit Schalter an und aktiven KI-Aufgaben (Nachtrag 7); Namen nach Nachtrag 8 (siehe aber Befund 5); Command-Namen mit genau einem Punkt (Nachtrag 1); Gruppe der KI-Tabs vorerst „Benutzerdefiniert“ (Nachtrag 2; die Tests suchen weiterhin gruppenunabhängig); Umschalten ohne Neuladen (Nachtrag 5); KI-Seite auch im Menü „Konfiguration ▾“ (Nachtrag 6); `--export-custom-summary` (OUT-203).

Weiter offen:
- **Zahlenformat des Felds „Kosten“** (M4-U01): nicht geprüft; geprüft ist nur der Feldname.
- **Leerzustand der KI-Tabs ohne Crawl:** Code nicht festgelegt; geprüft ist genau ein UI-016-Code mit Rang ≤ 8 (nicht `NO_MATCH`, `NOT_APPLICABLE`, `NO_SELECTION`).
- **Name der KI-Konfigurationsseite** („KI“/„AI“, Nachtrag 8): nur indirekt geprüft (eine zusätzliche Seite in Dialog und Menü), nicht über den Namen.
- **`--api-budget`** (gemeinsames Budget KI + DataForSEO) bei Schalter aus in `--help`: offen, nicht geprüft.
- **KI-Optionen bei Schalter aus ausdrücklich gesetzt** (`--use-ai`, `--ai-tasks …`): Verhalten nicht festgelegt (OUT-203 regelt nur Namen), nicht geprüft.
- **Mock-Zähler 0 (AGENT-030):** nicht direkt gemessen; geprüft ist das Scheitern schon an der Werkzeugauswahl (identische Antwort wie für einen unbekannten Namen) und im UI-Test, dass bei Schalter aus kein `ai.*`-Command über die Bridge geht.
- **MCP-Server der Desktop-App** (AGENT-030 „Server der App“): nicht geprüft, nur `openspider-mcp` und `openspider mcp`.

## 4. Dateien

| Datei | Art |
|---|---|
| `crates/os-app/tests/acceptance_f02_ai_ipc.rs` | neu |
| `crates/os-mcp/tests/acceptance_f02_ai_switch.rs` | neu |
| `crates/os-cli/tests/acceptance_f02_ai_switch.rs` | neu |
| `apps/desktop/e2e/acceptance/f02-ai-switch.spec.ts` | neu |
| `docs/plan/M4-reports/f02-tests.md` | neu (dieser Bericht) |
