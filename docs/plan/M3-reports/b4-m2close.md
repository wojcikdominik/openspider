# Bericht B4-M2CLOSE: M2 formal abschließen (Eingabe für das Fable-Gate M2-Q04)

Branch `m3/m2close` (Basis main `816e51d2`), Worktree `~/Projects/openspider-wt/c-m2close`, Server-Worktree `m3-m2close`.
Nicht nach origin gepusht, nichts gemergt.

## Ergebnis in Kürze

`M2-status.json` hat **keine offene ID mehr: 590/590 implemented, `blocking_specs: []`, `unknown_merged: []`,
`failed_tests: []`** (Entwicklung und `--gate` gleich). `passed` bleibt aber **false**. Offen sind **14 Anhang-B-Einträge**
und **5 Ignores** (Abschnitt „Offen für das Gate“). Beide lassen sich nicht mit einem Tag oder einer ehrlichen Begründung
schließen; sie brauchen eine Entscheidung von Planer bzw. Fable.

| Prüfgröße | vorher (Lauf 42069070215) | nachher |
|---|---|---|
| IDs implemented / open | 588 / 2 | **590 / 0** |
| Anhang B offen, gesamt | 34 von 137 | **14** |
| Anhang B offen, blockierend | 25 | 14 (jetzt alle blockierend, weil alle Besitzer als gemergt erkannt sind) |
| Ignores ohne gültigen Grund | 13 | **5** |
| Pflichtpakete in Entwicklung | 127 | 149 von 150 (es fehlt nur M2-Q04, das Gate selbst) |
| `--milestone M1 --strict` | 242/242 | 242/242, Exit 0 |

Zwei Abweichungen vom Auftrag:

- **M2-N34 ist schon auf main** (`229b304b`, Merge `5bd171af` vom 02.11.). Die Spec-Datei reagiert bereits auf Stale wie
  beauftragt. Ich habe sie nicht geändert und nur den 20×-Lauf gefahren. In `M2-backlog.md` steht N34 noch als Folgepaket.
- **AGENT-041/042 brauchten keine Tags.** `integrations/tests/contracts.test.mjs` trägt sie seit M2-M02, lag aber
  außerhalb der Scan-Wurzeln und der Evidenz. Nötig war eine kleine Erweiterung von Scanner und Workflow statt reiner Tags.

## 1 M2-Nachweise

### AGENT-041/042: 2 von 2 geschlossen

- `frontend.rs` scannt zusätzlich `integrations/tests/*.test.mjs`, nur dort `.mjs`.
- Der `evidence`-Job führt die 7 Vertragstests mit JUnit aus. `node:test` schreibt keine `<testsuite>`, der Schritt hüllt
  die Fälle deshalb in eine Suite mit dem Dateipfad.
- **Einschränkung für das Gate:** Die Vertragstests belegen Format, Schemata und Launcher. Marketplace-Installation und
  Slash-Befehl auf macOS/Windows (AGENT-042) prüft nur `integrations.yml`, und das fließt nicht in die Abdeckung ein.

### Anhang B: 20 von 34 Einträgen geschlossen

Nur Kommentar-Tags (Commit `01d1f082`). Ein Marker steht nur dort, wo der Test das zurückgestellte Kriterium aus M1.md 7.3
tatsächlich prüft. Die Prüfung je Test lief über drei Teilagenten; ich habe den Diff gesichtet und CRAWL-039 sowie OUT-015
stichprobenartig gegen Spec und Test gelesen, nicht alle 20.

| Eintrag | Test mit Marker | Geprüftes Kriterium |
|---|---|---|
| AGENT-004 / D15 | `custom_extract_parse.rs` `extracted_instructions_remain_inert_data`, `tab_custom_extraction.rs` `sim_crawl_transports_…` | Custom-Extraction-Wert bleibt Daten, kein Abruf |
| CRAWL-022 / E05 | `render_integration.rs` `raw_and_dom_are_compared_…`, `render_chromium.rs` `disabled_images_…` | Gesamt übertragen mit Ressourcen |
| CRAWL-039 / N07 | `n07_onpage.rs` `css_discovery_and_image_analysis_are_independent` | `style`-Hintergründe ohne Rendering nur mit Option, im JavaScript-Modus immer |
| CRAWL-077 / E05 | `render_integration.rs` `text_mode_never_calls_renderer_…`, `a_started_crawl_cannot_change_…` | [P1] Chromium fehlt, Moduswechsel |
| CRAWL-077 / R01 | `provision_select.rs` `explicit_missing_path_never_falls_back`, `without_any_chromium_…` | kein stiller Rückfall, Meldung |
| CRAWL-077 / U13 | `e2e/config/dialog.spec.ts` (Sperre; Chromium-Fehler) | [P1] in der Oberfläche |
| CRAWL-127 / U15 | `crawls.test.tsx` (3 Tests) | Zuletzt-geöffnet, Vordergrund-Fall **nur UI-Seite**; Doppelklick nicht |
| CRAWL-131 / E02 | `add_urls_m2.rs` (2), `cmd_crawl_add_m2.rs` | [P1] URLs hinzufügen, `analysis_stale` |
| CRAWL-140 / D10 | `tab_validation.rs` `decoded_size_thresholds_…` | [P1] Neuklassifikation ohne Abruf |
| CRAWL-140 / E03 | `http_m2.rs` (2) | [P1] Größe bei abgeschnittenem Body |
| ONPAGE-012 / E05 | `render_integration.rs` `raw_and_dom_…` | Total Transferred mit Rendering |
| ONPAGE-142 / D02, D19, N08 | `tab_content.rs` (2), `tab_content_spelling.rs` (2), `n08_content.rs` | [P1] PDF-Zeile, Sprachsummen, Hash-Spalte |
| OUT-015 / N06 | `os-parity/src/diff.rs` `upload_format_tests::…` | **Grenzfall**, siehe unten |
| OUT-031 / D08 | `bulk_links_m2.rs` `raw_mailto_and_tel_…` | Rohwert `mailto:`/`tel:` |
| OUT-203 / L01 | `cli_basics.rs` `list_exports_and_list_reports_…` (`[P1] [M2]`), `registry.rs` `without_a_similar_name_…` (`[P1]`) | [P1] Listen und Vorschlag |
| TECH-008 / D01, D04, K06 | `tab_meta_m2.rs`, `tab_canonicals_m2.rs`, `commands/catalog.rs` | Schalter/Abdeckungszustände |

**Grenzfälle, bitte im Gate bewerten:**

- **OUT-015 / N06:** N06 besitzt nur `os-parity`. Der Test prüft die Paritätsmessung des Kriteriums (`Original Url`,
  Duplikate, Reihenfolge), nicht den Produktexport. Bei den anderen N06-Einträgen (unten) gibt es nicht einmal das.
- **TECH-008 / K06:** Der Test belegt die DTO-Zustände der Befehlsebene, nicht den Hinweis in der Oberfläche.
- **CRAWL-127 / U15:** teilweise, siehe Tabelle.

### Ignores: 8 von 13 geschlossen

Nur Begründungstexte (Commit `3abb003c`), Bedingungen und Testlogik unverändert. Alle neuen Gründe gelten auch im
M1-Strict-Lauf aus `ci.yml`; ein blankes `P2:` gälte dort nicht. M2.md 5.1 nennt deshalb zwar `P2: CRAWL-054`, ich setze
`Klärung K2-32: P2: CRAWL-054 …`, und K2-32 hält genau das fest.

| Test | neuer Grund |
|---|---|
| `perf/n23-soak-diagnostics.bench.ts` (3) | `Benchmark: …` |
| `perf/perf.bench.ts:202` | `Benchmark: soak runs once per suite; …` |
| `viz/perf3d.spec.ts:26` | `Benchmark: Messrezept, nur mit OS_VIZ_PERF3D=1`; zusätzlich liest `results.rs` den Grund aus `<property name="skip">`, weil Playwright ihn nicht in `<skipped>` schreibt (Commit `1424b443`) |
| `tab_pagination_m2.rs:305` | `Klärung K2-32: P2 CRAWL-093 (Respect Next/Prev)` |
| `tab_response_codes.rs:622` | `Klärung K2-32: P2: CRAWL-054 – …` |
| `os-app/tests/render_provision.rs:392` | `Klärung K3-01: manuell, OUT-260 – Protokoll M3-Q03 …` |

## 2 M2-N34

Keine Änderung nötig, siehe oben. `detail-during-crawl.spec.ts` auf dem Stand dieses Branches, `--repeat-each=20
--retries=0 --workers=1`: **40 von 40 bestanden** (20 Chromium, 20 WebKit), Budget 300 ms unverändert. B4-LIVEDETAIL arbeitet
parallel an `view.open`; diese Datei habe ich nicht berührt, eine Berührung besteht also nicht. Ob der Test nach dem
Merge von B4-LIVEDETAIL noch grün ist, deckt dieser Lauf nicht ab.

## 3 Merge-Erkennung

Variante: **Titelregeln plus `merged_via`** (Commits `74719c46`, `76152d02`).

- Die Regeln liegen jetzt in `crates/os-parity/src/coverage/merged.rs` statt als Python im Workflow, damit sie testbar sind.
  Neuer Schalter `--merged-log <git log --format=%H%x09%s>`.
- Titelformen: `m2: Merge …` (wie B3-COV), `m2: Integration Runde N – [merge m2/]<Zweig>` (Runden 6/7),
  `merge: m2/<Zweig> … in Integrationsrunde N` (Runde 8, im Auftrag nicht genannt: N02a, A01, E05).
- `merged_via {commit, subject}` in `M2-packages.json` für die vier Sonderfälle. Gezählt wird nur, wenn Commit **und** Titel
  in der Historie stehen; sonst Eintrag in `merged_via_missing`.

| Paket | Beleg |
|---|---|
| M2-N06, M2-Q02 | `5c70de74` m2: Merge base-4r (Fable N06/Q02 freigegeben) |
| M2-N15F | `62164954` m2: Merge N20 (Fable N15F approved, N20 approved mit Auflagen) |
| M2-FIXE-PORT | `77a22061` engine: FIX-E-Port an die M2-Linie anpassen (Cherry-Pick in int-23, kein Merge-Titel; `int-23.md`) |

Ergebnis auf main `816e51d2`: **147 Pakete** statt 124 (+19 über Titelformen, +4 über `merged_via`), `unknown_merged: []`,
`merged_via_missing: []`, 24 andere Zweige (`other_merge_branches`; die 23 aus B3-COV und `ci-slim`). G0 und V02 zählen über
Welle 0, Q04 ist bewusst nicht gemergt.

Tests: `merged::tests::branch_of_each_merge_title_form`, `merged::tests::longest_package_prefix_and_merged_via_with_subject_check`,
`coverage_m2::merge_log_selects_packages_by_title_form_and_merged_via`. Keine Spec-ID, weil Infrastruktur (wie B3-COV).
Nicht umgesetzt, weil nicht beauftragt: `unknown_merged` im Gate rot (Folgepunkt in `beta3.md`).

## 4 Abdeckungslauf

**Evidenz, gemischt und deshalb kein Ersatz für den Gate-Lauf:**

- Rust, vitest und Integrations-JUnit frisch vom Server auf diesem Stand.
- E2E-JUnit der fünf Matrix-Jobs aus dem **CI-Artefakt von diag/cov (05.11.)**, weil macOS und Windows auf dem Server nicht
  laufen. Die Testnamen sind unverändert; die CI-Dateien tragen aber noch die alten Skip-Texte.
- Deshalb zusätzlich ein Linux-Lauf von `viz/perf3d.spec.ts` und `viz/v02.spec.ts` (16 bestanden, 2 übersprungen), als erste
  JUnit übergeben, damit der neue perf3d-Grund zählt.
- Merge-Historie von `origin/main` (`816e51d2`).

Der Workflow selbst lief nicht (kein Push). `actionlint`/`shellcheck` sind grün, den Abdeckungsschritt habe ich von Hand
nachgestellt. **Der erste echte Beleg ist der Workflow-Lauf nach dem Merge bzw. `workflow_dispatch` mit `gate`.**
`docs/plan/M2-status.json` habe ich wegen der gemischten Evidenz nicht aktualisiert (Stand weiter Oktober); das gehört zum
Gate-Lauf.

```
Spec-Abdeckung: 590 IDs – implemented 590, verified 0, deferred 0, open 0
Zusatz-IDs: 75 – implemented 75, open 0
Ignorierte Tests ohne gültigen Grund: 5
Anhang B: 137 (ID, Paket)-Einträge, 14 blockierend offen
M2 Gate: 0 blockierende IDs, 5 ungültige Ignores
```

Die Zeile ist die aus `--gate`; `--strict` endet mit Exit 1, solange die Punkte unten offen sind.

## Offen für das Gate

### A Anhang B: 14 Einträge

**11 Einträge sind „geerbt“.** Das Paket steht in der Anhang-B-Tabelle von M2.md für diese ID **nicht**. Es führt die ID nur als
`extra_spec_id` aus späteren Wellen, aus anderem Anlass. `m2.rs` verlangt den Marker von jedem Paket mit dieser `extra_spec_id`
(so auch der Wortlaut von B2). Alle in der Tabelle genannten Pakete dieser IDs sind erfüllt.

| Eintrag | In der Tabelle zuständig | Warum kein Marker |
|---|---|---|
| CRAWL-112 / N30 | U03 | Kriterium ist der Hinweis ab 20 Threads in der UI; N30 besitzt nur einen Thread-/Rücksetztest |
| CRAWL-124 / E07 | O02, U14, Q03 | `scale_*.rs` prüft weder Dialog noch Umzug |
| CRAWL-126 / FIX-WAL | K04, K07, U13 | kein Test „API-Daten während Pause“ |
| CRAWL-130 / N25 | U03 | prüft keine Aktualisierungsrate; `export_catalog_prerequisites_…` ist höchstens Teilbeleg |
| CRAWL-130 / N28 | U03 | prüft die Encountered-Zählung, nicht die Rate |
| ONPAGE-011 / N06 | K06, U20 | Paritätswerkzeug, kein Spaltentest |
| ONPAGE-090 / N07 | R02 | prüft Pixelbreite, nicht den Titel aus dem DOM |
| ONPAGE-140 / N08 | K03, U12 | prüft Wortgrenzen, nicht den Recrawl-Hinweis |
| OUT-031 / N07 | D08 | prüft Link Position; das dortige `[P1]` wirkt selbst unpassend |
| OUT-081 / N06 | N01 | kein HSTS-Fall in os-parity |
| UI-121 / N03 | F04, U32, U33 | prüft das P0-Kriterium, nicht das [P1]-Kriterium Community-Ressource |

**Vorschlag:** Entweder die IDs aus den `extra_spec_ids` dieser Pakete streichen oder die Prüfung in `m2.rs` auf die Pakete der
Anhang-B-Tabelle begrenzen. Beides ändert Plan bzw. Regel und ist darum nicht von mir umgesetzt (K2-63). Inhaltlich ginge
kein Nachweis verloren. Tags wären hier Gefälligkeitsbelege.

**3 Einträge stehen wirklich in der Tabelle:**

| Eintrag | Befund | Vorschlag |
|---|---|---|
| CRAWL-124 / Q03 | Q03 besitzt keine Testdateien, nur Dokumente und Skripte | Plan: Beleg über Q03-Bericht zulassen oder Eintrag streichen; O02 und U14 sind erfüllt |
| PERF-008 / Q03 | wie oben; PERF-008-Tests von X01 sind teils Benchmarks, die für Anhang B nicht zählen | wie oben; `xlsx_m2.rs::combined_workbook_splits_…` (X01) ist ein Nicht-Benchmark-Beleg |
| UI-121 / U32 | `e2e/visual/language.spec.ts` trägt `[P1]` bereits, aber `visual/` ist in `playwright.config.ts` ausgeschlossen und kein Workflow nennt die Suite: **es gibt kein Ergebnis** | `language.spec.ts` in den Abdeckungslauf aufnehmen (eigenes kleines Paket) |

### B Ignores: 5

| Test | Befund | Vorschlag |
|---|---|---|
| `e2e/viz/v02.spec.ts:133, 222, 237` | Bedingter Skip „WebGL 2 nicht verfügbar“. **In keinem der fünf CI-Jobs tatsächlich übersprungen**, alle `ok`. Der Scanner wertet jedes `test.skip(bedingung, grund)` statisch als Ignore | siehe unten |
| `e2e/polish/tabbars.spec.ts:73` (3 Instanzen) | Skip nur unter WebKit („Skalierungsfaktoren prüft die Abnahme mit Chromium“); auf Chromium und Windows `ok` | siehe unten |
| `os-analyze/tests/tab_internal.rs:588` | **Nicht aktivierbarer Platzhalter**: Körper ist `unreachable!("Rendering ist M2")`, Grund `M2:`. M2.md 5.1 nennt ihn „alter Test; neuer Test in M2-E05“; `render_integration.rs` trägt jetzt `ONPAGE-012 [P1] [M2]`. Nicht gelöscht und nicht umbenannt (Auftrag) | Besitzer löscht den Platzhalter nach Fable-Freigabe. B2 verlangt ohnehin „0 Tests mit Ignore-Grund `M2:`“; das ist der letzte produktive |

Für die vier bedingten Skips passt keiner der erlaubten Präfixe ehrlich: `Benchmark` nicht, `M3:` wäre eine falsche
Zurückstellung und bräche später in M3, K2-12 regelt Messungen. **Vorschlag, eins von beiden:**

1. Regel in `m2.rs`: Ein bedingter Skip mit mindestens einem `ok` im selben Lauf ist keine Zurückstellung. Das ändert eine
   Prüfung, deshalb Fable-Entscheidung; ein immer übersprungener Test bliebe rot.
2. Neue Klärung K3-xx „umgebungsbedingte Skips“, auf die sich die Gründe berufen.

Ich empfehle (1), weil es die Ursache trifft und künftige Fälle mit abdeckt.

### C Weitere Hinweise

- `M2-backlog.md` führt N34 noch als offenes Folgepaket; `b3-n00.md` nennt es erledigt.
- Andere B-Kriterien aus M2.md 8 (B1, B3 ff.) habe ich nicht geprüft; dieses Paket betrifft B2.

## Server-Ergebnisse

| Prüfung | Ergebnis |
|---|---|
| `cargo fmt --all --check` | grün |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | grün |
| `cargo test --workspace --exclude openspider-desktop --locked --no-fail-fast` (ws-slot, 2. Lauf, maßgeblich) | **4354 bestanden, 0 fehlgeschlagen, 50 ignoriert**, Exit 0 |
| 1. Lauf desselben Stands | 4345 / 9 / 50: alle 9 in `os-analyze` (`graph_dirs`, `graph_limits`, `link_score_b3_perf`) mit `SqliteFailure DiskFull`. Ursache Platzmangel, nicht genau belegt: gemeinsames 31-GB-tmpfs `/tmp` oder die Platte (siehe unten). Im 2. Lauf alle grün |
| `npm run -w apps/desktop typecheck`, `test` | grün |
| Integrations-Vertragstests | 7 / 0 |
| `detail-during-crawl.spec.ts` 20× Chromium + WebKit | 40 / 0 |
| `os-parity run --all` | bestanden: 218 Vergleiche ok, 3 vorgesehene Auslassungen, 10 Sites `present` (Referenzdateien des Worktrees, ohne `parity/ref-m2`) |
| `os-parity coverage --milestone M1 --strict` | 242/242, Exit 0 |
| `docs/plan/M2-check.py` | OK |
| `actionlint` 1.7.12 + shellcheck 0.11.0 auf `spec-coverage-m2.yml` | 0 Befunde |

Platte: Mein `target/` wuchs im 1. Lauf auf 114 GB, die Platte lief zwischenzeitlich voll, andere Pakete mussten Läufe
wiederholen (Hinweis des Orchestrators). Gelöscht; alle weiteren Läufe mit Debug 0 (`target/` 56 GB), am Ende wieder
gelöscht, Worktree 849 MB.

## Fremde Dateien (K2-92)

Parallelpakete: **B4-RESUME** berührt `os-engine` und `os-cli` (Zeilen unten mit *), B4-LIVEDETAIL und B4-LANG keine.

| Datei | Besitz laut Plan | Änderung |
|---|---|---|
| `apps/desktop/e2e/config/dialog.spec.ts` | U13 | Tag |
| `apps/desktop/e2e/perf/n23-soak-diagnostics.bench.ts` | N23 | Skip-Grund |
| `apps/desktop/e2e/perf/perf.bench.ts` | U31 | Skip-Grund |
| `apps/desktop/e2e/viz/perf3d.spec.ts` | M3-V02 | Skip-Grund |
| `apps/desktop/src/features/crawls/crawls.test.tsx` | U15 | Tag |
| `crates/os-analyze/tests/{n08_content,tab_canonicals_m2,tab_content,tab_content_spelling,tab_custom_extraction,tab_meta_m2,tab_validation}.rs` | N08, D04, D02, D19, D15, D01, D10 | Tag |
| `crates/os-analyze/tests/{tab_pagination_m2,tab_response_codes}.rs` | D05, D12 | Ignore-Grund |
| `crates/os-app/src/commands/catalog.rs` | K06 | Tag (Kommentar im Testmodul) |
| `crates/os-app/tests/cmd_crawl_add_m2.rs` | E02 | Tag |
| `crates/os-app/tests/render_provision.rs` | F05b/M3-K01 | Ignore-Grund |
| `crates/os-cli/src/registry.rs` * | L01 | Tag (Kommentar im Testmodul) |
| `crates/os-cli/tests/cli_basics.rs` * | L01 | Tag |
| `crates/os-engine/tests/{add_urls_m2,render_chromium,render_integration}.rs` * | E02, E05 | Tag |
| `crates/os-export/tests/bulk_links_m2.rs` | D08 | Tag |
| `crates/os-fetch/tests/http_m2.rs` | E03 | Tag |
| `crates/os-parse/tests/{custom_extract_parse,n07_onpage}.rs` | D15, N07 | Tag |
| `crates/os-render/tests/provision_select.rs` | R01 | Tag |
| `crates/os-parity/src/diff.rs` | N06 | Tag |
| `crates/os-parity/src/coverage.rs`, `coverage/{m2,frontend,results,merged}.rs`, `coverage/README.md` | Q01; M3-Q01 „ohne m2.rs-Verhalten zu ändern“ | **Verhaltensänderung in `m2.rs`** nur zusätzlich über `--merged-log`; Scanner + Skip-Grund wie oben; bei der Integration mit M3-Q01 abgleichen |
| `crates/os-parity/tests/{coverage_m2,coverage_bench,coverage_status}.rs` | Q01 | 2 neue Tests; sonst nur neues Feld `merged_log: None` in den Struct-Literalen. Keine Assertion geändert |
| `.github/workflows/spec-coverage-m2.yml` | Q01 | siehe Abschnitte 1 und 3; Pfad `integrations/**` als Auslöser |
| `docs/plan/M2-packages.json` | Plan | Feld `merged_via` an 4 Paketen |
| `docs/plan/M3-reports/b4-m2close.md` | neu | dieser Bericht |

Testlogik, Assertions, Budgets und Zeitlimits sind unverändert, kein Test gelöscht oder aktiviert (K2-63).
