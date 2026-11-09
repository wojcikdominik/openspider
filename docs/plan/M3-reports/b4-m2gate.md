# Bericht B4-M2GATE: offene Gate-Punkte M2-Q04 aus der Fable-Prüfung von B4-M2CLOSE

Branch `m3/m2gate` (Basis main `bb0b27e1`, nach Fable A1 mit dem lokalen main `c8a0d85f` zusammengeführt), Worktree `~/Projects/openspider-wt/c-m2gate`, Server-Worktree `m3-m2gate`.
Nicht nach origin gepusht, nichts gemergt. Ausgeführt von Claude Opus 5.5 (Planer- und Bauarbeit in einem Paket).

## Ergebnis in Kürze

Alle sechs Punkte sind umgesetzt. Die Gate-Prüfung `os-parity coverage --milestone M2 --gate --strict` endet auf dem
Server mit **Exit 0, `passed: true`**: 590/590 IDs implemented, **Anhang B 0 offen** (123 Einträge), **0 ungültige
Ignores**, `failed_tests: []`. Die Evidenz ist frisch und vollständig von diesem Stand, aber **nur Linux** (Chromium und
WebKit); macOS-WebKit und Windows-Chromium liefert erst der echte Gate-Lauf in der CI (Abschnitt 6). `docs/plan/M2-status.json`
habe ich deshalb nicht überschrieben.

**Nachbesserung (Fable `needs_work` 70):** lokaler `main` mit M3-Q01 eingemergt und `mx.rs` angepasst (A1), Beleg
CRAWL-124 korrigiert (A2), beide Prüfregeln als K2-102 in `M2.md` (A3), B4-LANG als M3-Paket eingetragen (Planer-Punkte
a–d). Nach dem Merge: clippy grün, `os-parity` 208/208, M1 242/242, M2-Gate weiter Exit 0; M3-Zahlen im Abschnitt
„Nachbesserung“.

| Prüfgröße | B4-M2CLOSE (CI-Lauf 42263770213, main 06.11.) | B4-M2GATE (Server, `--gate`) |
|---|---|---|
| IDs implemented / open | 590 / 0 | 590 / 0 |
| Anhang B (ID, Paket) gesamt / offen | 137 / 14 | **123 / 0** |
| Ignores ohne gültigen Grund | 4 (+ `render_provision.rs` mit überdehntem Grund) | **0** (4 bedingte Skips nach neuer Regel belegt, D7-Grund ehrlich) |
| Gemergte Pakete (Merge-Historie) | 147 | 147, `unknown_merged: []`, `merged_via_missing: []` |
| `--milestone M1 --strict` | 242/242 | 242/242, Exit 0 |
| `passed` | false | **true** (Entwicklung und Gate) |

## 1 Anhang B: 11 geerbte Einträge und OUT-015/N06 (Planerarbeit, K2-101)

**Begründung.** `m2.rs` verlangt nach B2 den Marker von *jedem* Paket, das die ID in `extra_spec_ids` führt. Elf Pakete
führen eine Anhang-B-ID nur aus anderem Anlass (Nachbesserungen der Wellen 4n/4q–4s, Skalierung, Paritätswerkzeug), die
Tabelle in Anhang B nennt sie nie. Ihre Tests prüfen das zurückgestellte bzw. [P1]-Kriterium nicht; Marker wären
Gefälligkeitsbelege. Die in der Tabelle zuständigen Pakete sind erfüllt. Die Planregel (M2.md 10: Planer ändert den Plan in
eigenen Commits; `M2-check.py` vor jedem Commit an `M2-packages.json`) ist eingehalten: Klärung **K2-101** nennt jeden
Eintrag mit Grund, nichts ist still gestrichen.

**Änderung** (Commit `58e6e8de`, `plan:`):

| Eintrag | Grund (Kurzform, voll in K2-101) | Belegt weiterhin durch |
|---|---|---|
| CRAWL-112 / N30 | Windows-Zeitabhängigkeit der Threads-Änderung; Kriterium ist der UI-Hinweis ab 20 Threads | U03 |
| CRAWL-124 / E07 | Skalierung Engine/Store; Kriterium Dialog und Umzug | O02, U14 |
| CRAWL-126 / FIX-WAL | WAL-Rückführung; Kriterium API-Daten während Pause | K04, K07, U13 |
| CRAWL-130 / N25 | Hreflang-Fix; Kriterium Aktualisierungsrate | U03 |
| CRAWL-130 / N28 | „gefunden“-Zählung; Kriterium Aktualisierungsrate | U03 |
| ONPAGE-011 / N06 | nur `os-parity`, kein Spaltentest | K06, U20 |
| ONPAGE-090 / N07 | Pixelbreite; Kriterium Titel aus dem DOM | R02 |
| ONPAGE-140 / N08 | Wortgrenzen; Kriterium Recrawl-Hinweis | K03, U12 |
| OUT-031 / N07 | Link Position; Kriterium Rohwert `mailto:`/`tel:` | D08 |
| OUT-081 / N06 | kein HSTS-Fall in `os-parity` | N01 |
| UI-121 / N03 | Laufzeit-Umschaltung (P0); [P1] Community-Ressource | U33, U32 |
| OUT-015 / N06 | Paritätsmessung, kein Produktexport (Grenzfall aus B4-M2CLOSE) | X01, L01 |

Zusätzlich entfällt der in B4-M2CLOSE gesetzte Marker `// spec: OUT-015 [M2]` in `crates/os-parity/src/diff.rs`
(Commit `49f073de`); der vorherige Tag `OUT-015, OUT-085` bleibt.

Die übrigen 26 geerbten Paarungen (z. B. TECH-008 bei D01/D04/D06/D07/K06, CRAWL-126 bei N03/N04/FIXE-PORT/N25) haben
eigene grüne Marker und bleiben unverändert; K2-101 sagt das ausdrücklich.

**Nachweis.** `python3 -I docs/plan/M2-check.py` → `OK` (150 Pakete, 590 `spec_ids`, P5/P6 unverändert erfüllt, weil
jede gestrichene ID bei den Tabellenpaketen bleibt). Gate-Ausgabe unten: Anhang B 123 Einträge, 0 blockierend offen (vorher 137/14).

## 2 Q03: CRAWL-124 und PERF-008

**Begründung.** M2-Q03 ist ein Messpaket ohne Testdateien (`owns`: nur Dokumente und Aufrufhilfen); ein getaggter Test
kann dort nicht entstehen. **Korrigiert nach Fable A2:** `q03.md` enthält **keine** CRAWL-124-Profilergebnisse (die
Profilläufe gehören zur noch offenen Stufe 2 auf dem 16-GB-Mac). Den Profilbenchmark bei 1 Mio. belegt M1-N14/N21 (A4,
`M1-acceptance/checklist.md` Zeilen 17 und 333, `M1.md` Welle 7b); die M2-Kriterien von CRAWL-124 (Dialog, Umzug des
Speichermodus) belegen O02 (`os-app/tests/cmd_crawls_management.rs:194/334`, `os-store/tests/registry_management.rs:254`)
und U14 (`e2e/settings-sections/sections.spec.ts:23`), alle mit `[M2]`. `q03.md` belegt nur PERF-008, und zwar Stufe 1
(1 Mio. CSV-Zeilen 1,97 s, XLSX-Limitwarnung und Blattteilung bestanden); Stufe 2 ist offen.

**Änderung** (Commit `58e6e8de`): CRAWL-124 und PERF-008 aus den `extra_spec_ids` von M2-Q03 gestrichen; in Anhang B
lauten die Zeilen nach der Korrektur (Commit `828f6f3b`) `O02⁺ (W1), U14⁺ (W2); Q03 gestrichen (K2-101):
Profilbenchmark bei 1 Mio. belegt M1-N14/N21 (A4, M1-acceptance/checklist.md), Stufe 2 von Q03 offen` bzw. `X01 (W1); Q03
gestrichen (K2-101): Großlauf ohne Testmarker im Bericht M2-reports/q03.md (Stufe 1; Stufe 2 auf der Referenzhardware
offen)`. K2-101 ist entsprechend berichtigt. Q03 behält PERF-006/007 (nicht in Anhang B).

**Nachweis.** `m2.rs` liest Anhang B weiter fehlerfrei (Testlauf `coverage_m2`, Gate-Ausgabe ohne Q03-Eintrag).

## 3 UI-121 / U32: Community-Prüfung der visuellen Suite ohne Ergebnis

**Befund.** `apps/desktop/e2e/visual/language.spec.ts` enthält drei Tests mit `UI-121 [P1]`: Zeile 4 (es/fr/it
vollständig) ist nach K2-79 mit `M3:` zurückgestellt, die beiden Community-Tests sind die nach K2-79 in M2 verlangte
Prüfung. Die Datei lag aber unter `visual/`, das `playwright.config.ts` für E2E-A ausschließt, und die eigene
U32-Konfiguration läuft in keinem Workflow. Die Prüfung existierte, lieferte aber nie ein Ergebnis.

**Änderung** (Commit `24260f18`): `MAIN_IGNORE` schließt `visual/` weiter aus, **außer** `visual/language.spec.ts`.
Die Datei macht keine Screenshots und braucht keinen Skalierungsfaktor; sie nutzt nur die Harness-Seiten des Vite-Servers.
Damit läuft sie in E2E-A auf allen CI-Plattformen (`ui-e2e.yml` und `spec-coverage-m2.yml`, Linux Chromium/WebKit,
macOS WebKit, Windows Chromium), ohne Workflow-Änderung. Kein Test wurde geändert, gelöscht oder umdefiniert; die
Zeile-4-Zurückstellung bleibt. `e2e/visual/README.md` nennt die Ausnahme.

**Nachweis (Server, Port aus `OS_E2E_PORT`=14636).**

```
npx playwright test visual/language.spec.ts --project=chromium --project=webkit --workers=2 --reporter=list,junit
→ je Engine: 2 bestanden, 1 übersprungen (fixme „M3: vollständige es/fr/it-Kataloge nach K2-79 …“), Exit 0
```

Im vollen E2E-A-Lauf (unten) steht UI-121 / M2-U32 auf `implemented`: `visual/language.spec.ts` liefert je Engine 2 × `ok` und 1 × `ignored` (Zeile 4, `M3:`).

## 4 Bedingte Skips: Regel in `m2.rs`

**Regel** (Commit `0077b674`): Ein Frontend-Test mit `test.skip(bedingung, grund)` und ungültigem Grund ist keine
Zurückstellung, wenn ein JUnit-`skipped` durch ein `ok` **desselben Tests in einem anderen Browserprojekt** (Attribut
`hostname` der Suite) belegt ist. Lief der Test überall `ok`, wurde nie übersprungen. Ungültig bleibt der Ignore bei:
keinem Laufzeitergebnis; `skipped` ohne `ok` anderswo; `ok` nur im eigenen Projekt (andere Template-Instanz oder andere
JUnit-Datei desselben Projekts); einem Fehlschlag. `M2:`-Gründe, C9-Platzhalter und leere Gründe nimmt die Regel nie aus
(B2, K2-63). Die anerkannten Fälle stehen mit ihren Läufen in der neuen Statusdatei-Liste `conditional_skips`; die
Konsole meldet ihre Anzahl. Dafür liefert `frontend::read_junit` je Ergebnis das Browserprojekt mit.

**Tests gegen Fixtures** (`crates/os-parity/tests/coverage_m2.rs`):

- `conditional_skip_counts_only_with_ok_in_another_project`: Template-Test (Muster wie `polish/tabbars.spec.ts:73`),
  WebGL-Test (Muster wie `viz/v02.spec.ts`), `M2:`-Test; fünf Fälle: Chromium ok/WebKit skipped → anerkannt, `M2:`
  bleibt ungültig; nur WebKit skipped → alle ungültig; ok nur im eigenen Projekt (Template-Instanz und zweite Datei) →
  ungültig; Fehlschlag in anderem Projekt → ungültig und `failed_tests`; kein Ergebnis → ungültig.
- Gegenprobe: Mit „ok in irgendeinem Projekt“ statt „in einem anderen Projekt“ scheitert der Test (Mutation auf dem
  Server geprüft, danach zurückgesetzt).

**Wirkung.** `tabbars.spec.ts:73` (Chromium ok, WebKit skipped) und `v02.spec.ts:133/222/237` (überall ok) sind keine
ungültigen Ignores mehr, sobald die JUnit-Belege beider Engines vorliegen. Ohne E2E-Belege bleiben sie rot.

## 5 `render_provision.rs:391` (`real_archive_from_versions_file`)

**Kann der Test laufen?** Nein, nicht in einer Suite: Er lädt die echten Chrome-for-Testing-Archive (ca. 200 MB je
Plattform) von `storage.googleapis.com`; M2.md 7 verbietet externes Netz in Tests, und die Plattform kommt aus
`OPENSPIDER_K01_PLATFORM`. Dieselbe Logik (Download, SHA-256 vor dem Entpacken, Fortsetzen, Abbruch) prüfen die übrigen
Tests dieser Datei und `os-render/tests/provision_download.rs` gegen den lokalen Mock. Der echte Lauf ist eine manuelle
Prüfung (Erstlauf 03.11.2026, `M3-reports/k01.md`).

**Ehrlicher Grund nach D7** (Commit `72d9eb3e`): `Manuell: OUT-260 – Protokoll M3-Q03 (echte Chrome-for-Testing-Archive
über externes Netz, ca. 200 MB je Plattform)`. Das ist die Form, die M3-K01 ursprünglich gesetzt hatte; „Klärung K3-01“
war überdehnt, weil K3-01 die Bündelungsfrage entscheidet, nicht diesen Test zurückstellt.

Damit der Grund stimmt, zwei Folgeänderungen:

- `m2.rs` erkennt die D7-Form, aber nur exakt `Manuell: <Spec-ID> – Protokoll M3-Q03`, optional mit Zusatz in Klammern
  (Test `d7_manual_reason_is_valid_only_in_its_exact_form` mit neun Negativfällen). `Klärung K3-` aus D7 erkennt `m2.rs`
  schon seit `21fd8758`.
- `M3-packages.json`, M3-Q03 `prompt_notes`: Prüfpunkt **(7) OUT-260** mit Befehl (Commit `58e6e8de`). Vorher nannte Q03
  diesen Test nicht; „Protokoll M3-Q03“ wäre sonst leer gewesen.

## 6 Gate-Lauf `workflow_dispatch` mit `gate`

Der Workflow braucht keine Änderung: `M2_GATE` ist bei `workflow_dispatch` mit `gate=true` gesetzt, der Abdeckungsschritt
übergibt `--gate --strict`, verlangt 5 `playwright-m2.xml` und 1 `playwright-m2-perf.xml` und endet sonst mit Exit 1.
`visual/language.spec.ts` kommt über die Playwright-Konfiguration automatisch in alle fünf E2E-Jobs.

**Befehle für den Orchestrator nach dem Merge von `m3/m2gate` und dem Push von `main`:**

```sh
cd ~/Projects/openspider
# 1. Gate-Lauf starten (erst, wenn der Push-Lauf von main gestartet ist; danach bis zum Start des Gate-Laufs nicht erneut
#    nach main pushen: Die Concurrency-Gruppe ersetzt sonst den wartenden Gate-Lauf durch den neueren Push-Lauf).
gh workflow run spec-coverage-m2.yml --ref main -f gate=true
# 2. Lauf-ID holen und abwarten (Exit ≠ 0 = Gate rot)
RUN=$(gh run list --workflow spec-coverage-m2.yml --event workflow_dispatch --branch main --limit 1 --json databaseId -q '.[0].databaseId')
gh run watch "$RUN" --exit-status
# 3. Statusdatei aus dem Artefakt übernehmen (Pfad im Artefakt: target/M2-status.json)
gh run download "$RUN" -n spec-coverage-m2 -D "/tmp/m2-gate-$RUN"
jq '{mode, passed, blocking_specs, blocking_appendix_b, failed_tests, n_bad: (.ignored_without_valid_reason|length)}' \
  "/tmp/m2-gate-$RUN/target/M2-status.json"
cp "/tmp/m2-gate-$RUN/target/M2-status.json" docs/plan/M2-status.json
git add docs/plan/M2-status.json
git commit -m "docs: M2-status.json aus dem Gate-Lauf $RUN (spec-coverage-m2, workflow_dispatch gate=true)"
```

Erwartung: `mode: "gate"`, `passed: true`, `blocking_specs: []`, `blocking_appendix_b: 0`, `failed_tests: []`, keine
ungültigen Ignores. Ist der Lauf rot, stehen die Gründe in `coverage-m2.txt` im selben Artefakt und in der Step-Summary.
Hinweis: Am 09.11. um 08:48 stand bereits ein Push-Lauf auf main in der Warteschlange (42588573478).

## Server-Ergebnisse (erster Lauf, vor dem Merge von `main`)

Alle Läufe im Server-Worktree `m3-m2gate`, Stand `24260f18` (plus dieser Bericht), mit `~/ws-slot.sh` und
`CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0`; Skript `~/m2gate-run.sh`, Belege unter
`ev/` im Server-Worktree (13 MB).

| Prüfung | Ergebnis |
|---|---|
| `cargo fmt --all --check` | grün |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` (inkl. `openspider-desktop`) | grün |
| `cargo test --workspace --exclude openspider-desktop --locked --no-fail-fast` | **4399 bestanden, 0 fehlgeschlagen, 50 ignoriert**, Exit 0 |
| `cargo test -p os-parity --test coverage_m2` | 32/32 (2 neu) |
| `npm run -w apps/desktop typecheck` | grün |
| `npm run -w apps/desktop test` (vitest, JUnit) | 122 Dateien, **1037/1037** |
| Integrations-Vertragstests (node:test, JUnit) | 7/7 |
| E2E-A Chromium (`CI=true`, `OS_E2E_PROJECTS=chromium`, inkl. `chromium-perf`, `--workers=4`) | **558 bestanden, 11 übersprungen**, Exit 0 |
| E2E-A WebKit (dito) | **554 bestanden, 14 übersprungen, 1 flaky**, Exit 0 |
| `visual/language.spec.ts` allein, Chromium + WebKit, Port `OS_E2E_PORT`=14636 | 4 bestanden, 2 übersprungen (fixme `M3:`), Exit 0 |
| `os-parity coverage --milestone M2 --strict` (Entwicklung) | Exit 0, `passed: true` |
| `os-parity coverage --milestone M2 --gate --strict` | **Exit 0, `passed: true`** (Ausgabe unten) |
| `os-parity coverage --milestone M1 --strict` | 242/242, Exit 0 |
| `python3 -I docs/plan/M2-check.py` | OK |

Flaky unter WebKit (im Retry grün, JUnit wertet den letzten Versuch): `e2e/acceptance/livedetail-memo.spec.ts:103` „a
theme change repaints the open tables, the detail window and the sidebar at once while the crawl runs“ (B4-LIVEDETAIL,
nicht von diesem Paket berührt). Bitte in der CI beobachten.

Gate-Ausgabe (Warnungen gekürzt; 73 davon sind vorhandene „nicht eindeutig zuordenbar“-Hinweise zu vitest-Titeln ohne
Gutschrift, keine aus diesem Paket):

```
Gemergte M2-Pakete laut Merge-Historie: 147 (davon über merged_via: 4); andere Merge-Zweige: 24
Spec-Abdeckung: 590 IDs – implemented 590, verified 0, deferred 0, open 0
Zusatz-IDs (extra, nicht gewertet): 75 – implemented 75, verified 0, deferred 0, open 0
Bedingte Skips mit ok in einem anderen Browserprojekt (keine Zurückstellung): 4
Anhang B: 123 (ID, Paket)-Einträge, 0 blockierend offen
M2 Gate: 0 blockierende IDs, 0 ungültige Ignores
```

`conditional_skips` der Gate-Statusdatei: `polish/tabbars.spec.ts:73` (Chromium 3 × ok, WebKit 3 × skipped),
`viz/v02.spec.ts:133/222/237` (Chromium und WebKit ok). Merge-Historie aus dem lokalen `main` (`bb0b27e1`), weil der
Server-Klon von `origin/main` älter ist; seit `6e08d7f2` kamen nur `merge:`-Titel von M3-Paketen dazu.

Platte: Der Lauf wartete von 09:36 bis 11:08 UTC in `ws-slot` (zeitweise nur 33 GB frei durch andere Worktrees). `target/`
dieses Worktrees ist am Ende gelöscht.

## Nachbesserung nach Fable `needs_work` 70 (09.11.2026)

**A1 (Blocker), Basis veraltet.** Lokaler `main` (`c8a0d85f`, 74 Commits weiter, mit M3-Q01 `coverage/mx.rs`) in
`m3/m2gate` gemergt (Merge `594826c7`, konfliktfrei). `mx.rs` ruft `frontend::read_junit` jetzt mit der neuen Rückgabe
auf und nimmt nur die Ergebnisse (`.into_iter().map(|(result, _project)| result)`, Commit `3d35a04b`). Ob die Skip-Regel
auch in `mx.rs` gilt, bleibt Folgepunkt für M3-Q01 (siehe M3-Zahlen unten: `v02.spec.ts:133/222/237` sind dort noch
ungültige Ignores).

**A2, falscher Beleg CRAWL-124.** Anhang B und K2-101 korrigiert (Abschnitt 2, Commit `828f6f3b`).

**A3, Regeländerung in den Plan.** `M2.md` Regel 5 trägt die Ergänzung K2-102 (a) bedingte Skips, (b) D7-Form im
M2-Check; neue Klärung **K2-102** mit der vollständigen Regel, den Tests und dem Satz: „Ein bedingter Skip, dessen
Bedingung keine Engine-, Plattform- oder Fähigkeitsprüfung ist, verstößt gegen K2-63.“ Fables Entscheidungen sind
eingetragen (andere Plattform desselben Projekts zählt nicht; D7-Form eng bestätigt). Die Crate-README verweist auf
K2-102 (Commit `828f6f3b`). `M2-check.py` OK.

**Planer-Punkte (Commit `6ee7c160`):**

- (a) `M3-packages.json`: Eintrag **B4-LANG** (Branch `m3/lang`, `extra_spec_ids` UI-121 mit dem [P1]-Kriterium
  es/fr/it, `depends_on` B4-M2GATE, Welle 4, `server+ci`). `owns`: `apps/desktop/src/**/i18n/**`, `e2e/i18n/**`,
  `e2e/visual/language*.spec.ts`, dazu aus dem Bericht `b4-lang.md` die neuen Dateien `e2e/acceptance/lang/**`,
  `core/issues/texts/{es,fr,it}.toml` und `{es,fr,it}.d/**`, `scripts/i18n-missing.mjs`, `M3-reports/b4-lang*.md`. Die
  übrigen B4-LANG-Änderungen (Kern-Sprachzuordnung, `UiLanguage`/Schema, Exportoption, Spaltenköpfe, `ui/shortcuts.ts`)
  stehen in dessen K2-92-Tabelle; die `prompt_notes` verweisen darauf. Zusätzlich vermerkt: Nach D8 braucht der
  es/fr/it-Test `// spec: UI-121 [M3]`, sonst bleibt das Teilkriterium in der M3-Abdeckung offen (auf `m3/lang` trägt er
  nur `[P1]`).
- (b) `M3.md` 1.3: Zeile „K2-79 Punkt (2) → B4-LANG“, Merge nach B4-M2GATE und dem Gate-Lauf M2.
- (c) Vermerk in `M3.md` 1.3 und in den `prompt_notes` von B4-LANG: Der `M3:`-Grund in `language.spec.ts:5` erscheint
  bis zum Merge von B4-LANG in der M3-Abdeckung als Altgrund und entfällt mit dem Merge.
- (d) `e2e/visual/README.md`: „ausschließlich das benannte M3-Kriterium verwendet `test.fixme`“ gilt jetzt ausdrücklich
  nur bis zum Merge von B4-LANG.

Hinweise für den Integrator: `m3/lang` ändert ebenfalls `e2e/visual/README.md`, `e2e/visual/language.spec.ts` und
`apps/desktop/playwright.config.ts` (Cherry-Pick `e3c10e72`); beim Merge von B4-LANG nach B4-M2GATE sind dort Konflikte
möglich. `merged.rs` erkennt `merge: B4-LANG …` als Reihe B, nicht als Paket der M3-Liste (Folgepunkt M3-Q01; im Gate ist
B4-LANG ohnehin Pflicht).

**Server nach dem Merge** (Stand `6ee7c160`, gleiche Evidenz wie oben: `rust-tests.txt`, vitest-, Playwright- und
Integrations-JUnit vom ersten Lauf; Merge-Historie des lokalen `main` `c8a0d85f`):

| Prüfung | Ergebnis |
|---|---|
| `cargo fmt --all --check` | grün |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | grün |
| `cargo test -p os-parity --locked` | **208 bestanden, 0 fehlgeschlagen** (inkl. `coverage_mx`) |
| `coverage --milestone M1 --strict` | 242/242, Exit 0 |
| `coverage --milestone M2 --strict` (Entwicklung) | Exit 0 |
| `coverage --milestone M2 --gate --strict` | **Exit 0**: 590/590, Anhang B 123/0 offen, 0 ungültige Ignores, 4 bedingte Skips |
| `coverage --milestone M3` (Entwicklung) | Exit 1 (erwartet, M3 läuft): 18 IDs, 16 implemented, 2 open (OUT-262 ohne Ergebnis – Windows, OUT-263 ohne Tag); Teilkriterien 58: 24 implemented, 7 manual, 27 open, 24 blockierend; 3 ungültige Ignores (`v02.spec.ts:133/222/237`, M3-Prüfer ohne Skip-Regel); Altgrund `visual/language.spec.ts:4` (`M3:`, bis B4-LANG) |
| `coverage --milestone M3 --gate` | Exit 1 (erwartet): 2 blockierende IDs, 27 blockierende Teilkriterien (darunter `B4-LANG: UI-121`), 4 ungültige Ignores (zusätzlich der `M3:`-Altgrund) |
| `python3 -I docs/plan/M2-check.py` | OK |

Die M3-Zahlen beruhen auf Linux-Evidenz ohne Release-, macOS- und Windows-JUnit; viele offene M3-Teilkriterien (Q02,
R04, S01, W02) brauchen genau diese Belege oder stehen noch aus. Für dieses Paket zählt: Die M3-Abdeckung läuft mit der
neuen `read_junit`-Signatur, und der Merge bricht weder clippy noch die Tests.

## Antworten auf die Fragen (Fable, 09.11.2026)

1. Andere Plattform desselben Projekts zählt **nicht** – so umgesetzt und in K2-102 festgehalten.
2. D7-Form im M2-Check bestätigt, eng gefasst – K2-102 (b).
3. es/fr/it: B4-LANG eingetragen (Planer-Punkte a–d).
4. Weitere Grenzfälle aus B4-M2CLOSE (TECH-008/K06, CRAWL-127/U15) bleiben außerhalb dieses Pakets.

**Folgepunkte M3-Q01:** Skip-Regel (K2-102 a) in `mx.rs` übernehmen; Reihe-B-Pakete mit Eintrag in der Paketliste
(B4-LANG) als gemergt erkennen.

## Fremde Dateien (K2-92)

| Datei | Besitz laut Plan | Änderung |
|---|---|---|
| `crates/os-parity/src/coverage/m2.rs` | M2-Q01 / M3-Q01 | Regel bedingte Skips, `conditional_skips`, D7-Grund `Manuell:` (Verhaltensänderung, siehe 4 und 5) |
| `crates/os-parity/src/coverage/frontend.rs` | M2-Q01 | `read_junit` liefert zusätzlich das Browserprojekt (`m2.rs` nutzt es, `mx.rs` verwirft es) |
| `crates/os-parity/src/coverage/mx.rs` | M3-Q01 | Aufruf an die neue `read_junit`-Rückgabe angepasst, Verhalten unverändert (A1) |
| `crates/os-parity/src/coverage/README.md` | M2-Q01 / M3-Q01 | Doku beider Regeln, Verweis K2-102 |
| `crates/os-parity/tests/coverage_m2.rs` | M2-Q01 | 2 neue Tests; bestehende Tests unverändert |
| `crates/os-parity/src/diff.rs` | M2-N06 | Marker `OUT-015 [M2]` entfernt (K2-101) |
| `crates/os-app/tests/render_provision.rs` | M3-K01 | Ignore-Grund und Doc-Kommentar, Testlogik unverändert |
| `apps/desktop/playwright.config.ts` | M2-F04 | `visual/language.spec.ts` aus dem Ausschluss genommen |
| `apps/desktop/e2e/visual/README.md` | M2-U32 (künftig B4-LANG) | ein Satz zur Ausnahme; `test.fixme` nur bis B4-LANG |
| `docs/plan/M2-packages.json` | Plan | 14 IDs aus `extra_spec_ids` von 10 Paketen (K2-101) |
| `docs/plan/M2.md` | Plan | Anhang B (CRAWL-124, PERF-008), Regel 5 (Ergänzung K2-102), Klärungen K2-101 und K2-102 |
| `docs/plan/M3-packages.json` | Plan | M3-Q03 `prompt_notes` Punkt (7) OUT-260; neuer Eintrag B4-LANG |
| `docs/plan/M3.md` | Plan | 1.3: Zeile K2-79 Punkt (2) → B4-LANG |
| `docs/plan/M3-reports/b4-m2gate.md` | neu | dieser Bericht |

K2-63: Keine Assertion, kein Budget, kein Zeitlimit geändert; kein Test gelöscht oder aktiviert. Die Skip-Regel lockert
keine Prüfung für Tests, die nie liefen; die D7-Form ist eng gefasst.

## Commits

- `6ee7c160` plan: B4-LANG als M3-Paket (UI-121 [P1] es/fr/it, K2-79 Punkt 2), M3.md 1.3, Vermerk M3:-Grund bis zum Merge (B4-M2GATE)
- `828f6f3b` plan: K2-102 Prüfregeln bedingte Skips und D7-Form Manuell in Regel 5; CRAWL-124-Beleg korrigiert (Fable A2/A3, B4-M2GATE)
- `3d35a04b` parity: mx.rs auf die neue read_junit-Signatur (Ergebnis, Browserprojekt) anpassen (Fable A1, B4-M2GATE)
- `594826c7` merge: lokaler main (c8a0d85f, u. a. M3-Q01 mx.rs) in m3/m2gate (Fable A1, B4-M2GATE)
- `f403600b` docs: Bericht B4-M2GATE (Gate-Prüfung M2 auf dem Server: 590/590, Anhang B 0 offen, 0 ungültige Ignores, passed)
- `24260f18` test: visual/language.spec.ts läuft in E2E-A (UI-121 [P1], U32, B4-M2GATE)
- `49f073de` test: Marker OUT-015 [M2] am Paritätstest entfernt (K2-101, B4-M2GATE)
- `58e6e8de` plan: K2-101 geerbte Anhang-B-Paarungen und Q03 aus extra_spec_ids, Q03-Prüfpunkt OUT-260 (B4-M2GATE)
- `72d9eb3e` test: ehrlicher Ignore-Grund nach D7 für den manuellen Archivtest (B4-M2GATE)
- `0077b674` parity: bedingte Skips nur mit ok in einem anderen Browserprojekt, D7-Grund Manuell (B4-M2GATE)
- (Berichtsergänzung, `docs:`)
