# B4-RESUME – Crawls per MCP und CLI anhalten und fortsetzen – Bericht

Stand 07.11.2026 (Nacharbeit, Abschnitte 5 und 6). Branch `m3/resume` (Basis `main` 816e51d2, zuletzt `main` 6d879a26 und `m3/resume-tests` df8a9435 eingemischt), Mac-Worktree `openspider-wt/c-resume`, Server-Worktree
`m3-resume` (git-Worktree auf 816e51d2, per `srv-sync.sh` gespiegelt). Grundlage: AGENT-001/003/020 (10), CRAWL-126/128/130
(01), UI-003/UI-013 (05), OUT-233 (06), `docs/plan/beta3.md` „Für beta.4 aus B3-N00“, `M3-reports/b3-n00.md` (S2, Punkt 4),
`M3-reports/b3-flakes.md` (Pause-Fixtures ohne natürliches Ende).

## Umfang

### 1 „Gestoppt ist endgültig“ in Store, Engine, CLI und MCP (CRAWL-128, AGENT-020)

- `os-store` `CrawlListing::can_resume`: `false` für jeden Crawl mit Endgrund `stopped`, unabhängig von Zustand und
  Warteschlange. Die App-Verwaltung (`commands/crawls.rs`) nutzt damit wieder ausschließlich die gemeinsame Kernregel.
- `os-engine` `lifecycle::prepare` (jedes `resume_crawl`) lehnt `stopped` ab, bevor irgendetwas geschrieben wird.
  Gemeinsamer Hinweis `lifecycle::STOPPED_IS_FINAL` = „gestoppt – endgültig beendet“.
- `lifecycle::add_urls_to_file` lehnt `stopped` ebenfalls ab (sonst stünden zusätzliche URLs in der Warteschlange eines
  Crawls, den das anschließende Fortsetzen abweist). Siehe Spec-Frage S3.
- CLI `--load-crawl <ID> --resume`: Exit 1, Meldung „Crawl <id> kann nicht fortgesetzt werden: gestoppt – endgültig beendet“,
  kein Abruf (`listing::validate_resume`).
- MCP `resume_crawl` (STDIO und In-App-HTTP): MCP-Fehler „Crawl <id> cannot be resumed: gestoppt – endgültig beendet
  (stopped crawls are final; start a new crawl)“ – vor jedem Abruferaufbau. Pausierte, unterbrochene und durch ein Limit
  beendete Crawls (Rest in der Warteschlange) bleiben fortsetzbar.
- Werkzeugtexte: `stop_crawl` „Stop a running or paused crawl for good … can never be resumed (use pause_crawl …)“,
  `resume_crawl` „Resume a paused, interrupted or limit-ended crawl … Stopped crawls are final and are rejected“.

### 2 `pause_crawl` und Fehlercode `busy` (AGENT-020, AGENT-003)

- Neues Werkzeug `pause_crawl(crawl_id | id)`, Klasse `crawl` (erscheint nur mit Freigabe `crawl`, wie `start_crawl`).
  - STDIO: signalisiert die Pause und antwortet sofort mit `{"crawl_id","pausing":true}` (wie `stop_crawl` mit
    `stopping`); `crawl_status` meldet danach `Paused`.
  - In-App-HTTP: derselbe Weg wie die Kopfleiste (`control::settle(.., Some(false))`), antwortet nach dem dauerhaften
    „Pausiert“.
- `stop_crawl` beendet in STDIO jetzt auch einen pausierten oder unterbrochenen Crawl ohne laufende Engine endgültig
  (UI-003 „Stoppen“ aus „Pausiert“): neues `lifecycle::stop_unowned` schreibt in einem Commit mit `synchronous = FULL`
  Zustand `Completed`, Endgrund `stopped`, `ended_at`/`heartbeat_at` und gibt `owner_pid` frei; die Warteschlange bleibt
  gesichert; die Crawl-Analyse startet wie nach einem regulären Ende (CRAWL-141), ohne die Antwort aufzuhalten.
  Vorher war nach einer Pause kein endgültiges Beenden per MCP möglich.
- Fehlercode `busy` (`ToolError::Busy`, `SessionError::Busy`): wiederholbar. Antwort `isError: true`, Text
  „busy: … (retryable: call again with the same arguments in a few seconds)“ und maschinenlesbar
  `structuredContent = {"error":{"code":"busy","retryable":true,"message":…}}`. Andere Fehler bleiben unverändert reiner
  Text (kein `structuredContent`).
  - STDIO: zweite Pause/Stopp/Fortsetzen-Anforderung, solange eine Pause oder ein Stopp noch läuft; Fortsetzen/Stoppen,
    während ein gemeldeter Endzustand noch abgeschlossen wird (Checkpoint, Writer; neu `CrawlHandle::is_finished`).
  - In-App-HTTP: „App is busy or shutting down“ (andere Operation läuft, B3-N00 Punkt 4) und App-Fehler der Klasse
    `ErrorCode::Busy` (`ensure_idle`) werden `busy` statt „interner Fehler“.
  - Weitere Abbildung der Sitzungsfehler (K-38, korrigierbar statt „intern“): `NotFound` → nicht gefunden,
    `NotControllable`/`Invalid` → ungültige Argumente; `Internal` unverändert.
- Fremdprozess-Crawls (AGENT-020 OS-Festlegung): Pause/Stopp eines in einem anderen Prozess laufenden Crawls → MCP-Fehler
  „runs in another process (<owner_kind>, PID <n>); it can only be controlled there“.
- Skill-/Plugin-Werkzeugtabelle (`integrations/**/references/tools.md`) um `pause_crawl` und `busy` ergänzt; der Validator
  vergleicht sie mit `tools::all()`.

### 3 `m2_process` umgebaut, owner_pid-Wettlauf an der Ursache behoben

- **Ursache:** Die Engine veröffentlichte den Endzustand im Fortschritt (`publish`) **vor** dem letzten Batch mit
  `state`, `end_reason` und `owner_pid = ""`. Wer auf „nicht mehr Running“ wartete (MCP `crawl_status`, App), konnte die
  Datei noch mit `owner_pid = <PID>` lesen. Das widersprach auch der OS-Festlegung in CRAWL-126 („Pausiert“ wird gemeldet,
  sobald Warteschlange und Metadaten dauerhaft geschrieben sind).
- **Behebung** (`os-engine/src/crawl.rs`): `publish` des Endzustands erst nach `durable()` + letztem Batch + `flush()`
  (dauerhafter Commit), vor dem Checkpoint. Auch auf dem Fehlerweg der Fragment-Materialisierung wird der Endzustand
  veröffentlicht, damit Beobachter nie auf „Running“ stehen bleiben.
- **Nachweis an der Ursache:** `os-engine/tests/b4_resume.rs::the_reported_end_state_is_already_durable_with_the_owner_released`
  wartet über den Fortschrittskanal (ohne Abfrageschleife) auf den Endzustand und liest im selben Augenblick die Datei
  (Pause und Stopp). Gegenprobe mit dem alten `crawl.rs`: **5/5 rot** (Zeile 140, `owner_pid` noch gesetzt); mit Fix grün.
- **Test** `crates/os-mcp/tests/m2_process.rs::headless_crawl_start_status_stop_resume_permissions_and_safe_overrides`
  (Name und Spec-Tags unverändert, um Konflikte mit B4-M2CLOSE zu vermeiden): statt „gestoppten Crawl fortsetzen“ jetzt
  start → `resume_crawl` auf laufenden Crawl abgewiesen (kein `busy`) → `pause_crawl` während `/held` (3 s) läuft →
  `pause_crawl`/`resume_crawl`/`stop_crawl` je `busy` mit `structuredContent` → `Paused`; **direkt danach** Datei
  `Paused`, `owner_kind = mcp-stdio`, `owner_pid = ""` (kein Warten, keine Lockerung) → zweite Pause „already paused“ →
  `resume_crawl` → `stop_crawl` (laufend) → `Completed`/`stopped`, `owner_pid = ""`, Warteschlange > 0 →
  `resume_crawl` abgewiesen mit „gestoppt – endgültig beendet“, kein weiterer Abruf → zweiter Crawl: pausieren, dann
  `stop_crawl` aus „Pausiert“ → `stopped`, Fortsetzen abgewiesen → unbekannte ID für alle vier Steuerwerkzeuge Fehler.
  Alle übrigen Prüfungen (Freigaben, Zielschutz, Overrides, Profil, „already finished“, Gleichheit mit dem CLI-Kern)
  unverändert. Wiederholte Aufrufe bei `busy` nutzen den dokumentierten Vertrag (`call_retrying_busy`, max. 10 s).
- **Fixture** `tests/fixtures/agent-m2-crawl/scenario.toml`: `/held` (3 s, hält die Pause „busy“) und die Kette
  `/chain/1 … /chain/12` (je 100 ms; bei 2 URLs/s ≥ 6 s), die vor Pause oder Stopp nicht von selbst enden kann
  (B3-FLAKES). `/` verlinkt sie nicht; bestehende Tests sind unberührt.

### 4 Verwaltung: „Abgeschlossen (gestoppt)“ – umgesetzt; `limit_kind` – nicht umgesetzt (Spec-Frage S1)

- CRAWL-128 legt den Anzeigenamen eindeutig fest. `crawls.list` liefert additiv `endReason` (`CrawlListing.end_reason`,
  `#[serde(default)]`, TS `endReason?: string | null`); Export-Schlüssel und Sortierung bleiben `Completed`.
  Die Verwaltung zeigt bei `completed` + `stopped` „Abgeschlossen (gestoppt)“ / „Completed (stopped)“
  (`crawls-phase-completed-stopped`, DE/EN), „Fortsetzen“ ist aus (über `canResume`).
- `limit_kind` in „Fertig (Grenze erreicht: …)“ steht nicht eindeutig in der Spec → weggelassen, siehe S1.

### 5 Nacharbeit: Abnahmetests eingebunden, Befunde behoben

`main` (mit Crawl-Datei-Fix, B4-LIVEDETAIL, Playwright-Port) und `m3/resume-tests` sind eingemischt. **Die 38 Abnahmetests sind unverändert und jetzt alle grün** (19 in `os-mcp`, 19 in `os-cli`),
auch die vorher drei roten. Keinen halte ich für falsch.

| Befund (`b4-resume-tests.md`) | Ursache | Fix | Test |
|---|---|---|---|
| 1, schwer: `--list-crawls` zerstört die Datei | siehe `b4-listfix.md` | auf `main`, per Merge hier | `listing_crawls_while_…` (Abnahme) |
| 2, mittel: nicht erreichbare Start-URL wird `Completed` statt `Failed` (CRAWL-128) | Die Engine kannte nur `QueueEmpty`; nur die CLI prüfte die Start-URL, und nur für den Exit-Code | `crawl.rs`: Endet ein Spider-Crawl mit genau einer abgerufenen Seite und ist das die Start-URL ohne Antwort wegen eines Netzfehlers (auch nicht erreichbare robots.txt), dann Zustand `Failed` ohne Endgrund. **OUT-182 bleibt erfüllt:** eine Zeile, Analyse-Hook, Exporte, Exit 0 mit Warnung, Exit 3 nur mit `--fail-on-start-error`. Der CLI-Zweig „`Failed` ist immer Exit 3“ entfällt, denn alle anderen Fehlschläge kommen als `Err` | `a_crawl_whose_start_url_is_unreachable_is_listed_as_failed` (Abnahme), `an_unreachable_start_url_fails_keeps_its_row_and_cannot_be_resumed`, `a_start_url_that_answers_is_completed_even_with_an_error_or_a_robots_block`, `an_unreachable_url_of_a_list_crawl_is_no_failed_crawl` (Engine, gegen Fixtures bzw. geschlossenen Port), `an_unreachable_start_url_is_listed_as_failed_and_stays_a_valid_result` (CLI) |
| 3, niedrig: `crawl_status` nennt nach `resume_crawl` bis rund 180 ms `Paused` | `state` kam aus der Datei, der erste Batch des Laufs ist da noch nicht geschrieben | `crawl_status` nimmt den Zustand des eigenen Laufs, nur ohne Lauf die Datei. Ein gemeldeter Endzustand ist dauerhaft (Abschnitt 3) | `m2_process`: `crawl_status` direkt nach `resume_crawl` ist `Running` |
| 4, niedrig: `resume_crawl` auf gestopptem Crawl antwortet `busy`, solange der Lauf abschließt | `busy` wurde vor dem Endgrund geprüft | Erst Endgrund `stopped` prüfen (dauerhaft, sobald gemeldet), dann `busy` | `m2_process`: der Aufruf direkt nach dem Stopp liefert den Hinweis, ohne Wiederholung |

**Spec:** `busy` steht jetzt in AGENT-020 (nur Code und Form, kein neues Verhalten). S1–S5 unten und die 8 Fragen aus `b4-resume-tests.md` bleiben offen und gehen an Fable; `limit_kind` bleibt offen.

**Bei den Gates gefunden, im ersten Bericht übersehen:** `e2e/beta/n04-mcp.spec.ts` zählte 16 Werkzeuge. Mit `pause_crawl` sind es 17; die Erwartung ist angepasst (kein Lockern, das Werkzeug ist gewollt).

**Merge-Fehler von mir:** Der Konflikt in `registry_management.rs` war im Merge-Commit `d29c6ddf` falsch gelöst (fehlende Klammer, Datei übersetzt dort nicht), Korrektur in `62faa845`. Beim Merge nach `main` zählt nur der Endstand.

### 6 Auflagen der Fable-Prüfung (Score 80)

| Punkt | Ursache | Fix | Test und Gegenprobe |
|---|---|---|---|
| **A1** `acceptance_resume.rs` übersetzt unter Windows nicht | `libc::kill` ohne `cfg(unix)` | `send_sigint` wie `cli_load.rs` gekapselt, `spawn_cli` setzt unter Windows `CREATE_NEW_PROCESS_GROUP`. **Einzige Änderung an den Abnahmetests**, vom Orchestrator freigegeben; Unix-Verhalten unverändert | Windows nicht ausgeführt, nur die `cfg`-Stellen geprüft. Anders als `cli_load` überspringt der Test nicht, wenn Strg+Untbr nicht zustellbar ist, denn die Aufrufer dürfen sich nicht ändern. Die letzte Windows-CI (Lauf 42309492699) ließ den SIGINT-Test von `cli_load` rund 9 s laufen, das spricht für eine zustellbare Konsole; sicher zeigt das erst die CI nach dem Merge |
| **A2** `crawl_status` meldet nach fremdem Weiterführen weiter `Paused` | `CrawlService::state()` nutzte auch den Handle eines beendeten Laufs | Handle-Zustand nur, solange `!is_finished()`, sonst gilt die Datei. Zusätzlich entfällt `progress`, wenn der letzte Lauf nicht mehr zum Zustand passt, sonst widerspräche sich die Antwort | `crawl_status_follows_the_file_after_another_process_finished_the_crawl` (zwei echte Prozesse); Gegenprobe ohne Fix: rot (`Paused` statt `Completed`) |
| **B3** robots.txt mit 5xx/429 macht den Crawl `Failed` | Mein Fix aus Abschnitt 5 zählte `blocked_by_robots_unreachable` und `malformed` mit | `is_network_failure`: nur DNS, Verbindung verweigert, Timeout, Verbindungsfehler, TLS. Bei `blocked_by_robots_unreachable` entscheidet die Hilfszeile der robots.txt | `a_robots_txt_answering_500_is_no_failed_crawl_although_it_blocks_the_start_url` (Gegenprobe ohne Fix: rot, `Failed`), `an_unreachable_start_url_fails_…` (geschlossener Port), `only_network_failures_count_as_unreachable` |

**Spec-Nachträge** (nur dokumentiert, kein neues Verhalten): S2 Fälle von `busy`, S5 und Testbericht (2), (3), (5), (7) in AGENT-020; S3 in CRAWL-131; S4 und S7 in CRAWL-128; S6 in OUT-182; Testbericht (3) auch in UI-003; Testbericht (6) in CRAWL-126 (mit neuem Test `resume_without_raising_the_limit_exits_0_with_a_hint_and_fetches_nothing`).
Zwei Stellen weichen bewusst vom Wortlaut der Vorgabe ab, weil sie sonst nicht stimmen:
- **(5):** `progress` gibt es nur, solange der Server den letzten Lauf kennt und er zum Zustand passt.
- **(7):** Nach einem harten Abbruch lässt sich `owner_pid` nicht mehr leeren; dort gilt der Heartbeat.

**Testbericht (8): kein Nachtrag, der Code macht es anders.** Beim Fortsetzen wird die robots.txt bekannter Hosts **nicht** erneut gelesen: `PrimedFetch` bedient sie aus `robots_files` (`os-engine/src/resume.rs`), und `lifecycle_pause.rs:235` prüft ausdrücklich „keine URL – auch nicht robots.txt – doppelt im Request-Log“. Der vorgesehene Nachtrag „erneut lesen ist erlaubt“ widerspräche dem. Entscheidung bei Fable.

**Neu gefunden, wackeliger Abnahmetest (nicht geändert, K2-63):** `a_killed_server_leaves_an_incomplete_crawl_that_resumes_without_duplicates` (`os-mcp/tests/acceptance_resume.rs:984`) verlangt nach SIGKILL mindestens 8 gespeicherte URLs bei 10 begonnenen Abrufen. Einzeln und allein gelaufen, bei Serverlast um 12:
| Stand | grün | rot |
|---|---|---|
| `m3/resume-tests` (df8a9435, vor meinen Änderungen dieser Runde) | 15 | 5 |
| Stand dieser Runde | 11 | 9 |

Jeder Fehlschlag meldet genau 7 gespeicherte URLs. Der Writer ist seit der Paketbasis unverändert (`writer_thread.rs`, `os-store/src/writer.rs`). Die Differenz der beiden Zeilen ist bei je 20 Läufen und wechselnder Last nicht belastbar; einen Rückschritt schließt sie nicht sicher aus, einen Zusammenhang mit meinen Änderungen sehe ich aber nicht. CRAWL-125 verlangt nur alle bis 2 s vor dem Abbruch gecrawlten URLs. Bei 250 ms je Seite dürften demnach bis zu 8 der 10 Abrufe fehlen, die Schwelle 8 ist also strenger als die Spec. **Vorschlag an Orchestrator/Fable:** Schwelle aus der Spec ableiten (Abrufe bis 2 s vor dem Abbruch). Bis dahin kann der Test CI und Workspace-Läufe rot färben.

## Commits (Branch `m3/resume`)

| Commit | Inhalt |
|---|---|
| `967f6b68` | store: `can_resume` lehnt `stopped` ab + Test |
| `e81299ef` | engine: gestoppt ist endgültig, `stop_unowned`, Endzustand erst nach dauerhafter Freigabe von `owner_pid`, `CrawlHandle::is_finished` + `tests/b4_resume.rs` |
| `bce814f5` | cli: `--resume` gestoppt → Exit 1 + Test |
| `0d787fc6` | mcp: `pause_crawl`, `busy`, Werkzeugtexte, `m2_process` umgebaut, Fixture, Skill-Tabelle |
| `494515b7` | app: HTTP-MCP `pause_crawl`/`busy`/Fehlerabbildung, Listing `endReason` + Tests |
| `60eeacba` | ui: Verwaltung „Abgeschlossen (gestoppt)“ + Vitest |
| `d29c6ddf`, `ba1d9edd` | merge: `main`, `m3/resume-tests` |
| `62faa845` | test: Merge-Konflikt `registry_management` vervollständigt |
| `824a01a9` | engine: nicht erreichbare Start-URL endet als `Failed` (Befund 2) |
| `7e42db74` | mcp: `crawl_status` und `resume_crawl` (Befunde 3 und 4) |
| `e6c4f3ea` | spec: AGENT-020 Fehlercode `busy` |
| `d78808ce`, `556d36af` | test: clippy in `b4_resume`, `n04-mcp` erwartet 17 Werkzeuge |
| `d1f06fcc`, `a5f8e5c2`, `7d34c501` | Auflagen A1, A2, B3 |
| `e32139ff`, `d1b8a0f0` | test: `--resume` am Limit; spec: Nachträge |
| (letzter Commit) | docs: Bericht |

## Spec-Fragen

**Stand nach Fable-Entscheidung:** S2–S7 und die Testbericht-Fragen (2), (3), (5), (6), (7) sind entschieden und in der Spec nachgetragen. Offen: S1 `limit_kind` (entscheidet der Nutzer) und Testbericht (8), siehe Abschnitt 6. Der Text unten ist der ursprüngliche Stand der Fragen.

- **S1 (UI-013 Nachtrag beta.4 ↔ OUT-233/CRAWL-130): `limit_kind`.** UI-013 nennt „max. URLs“, „max. Tiefe“, „Zeit“,
  „sobald der Endgrund sie führt (`limit_kind`)“. Es fehlen: der Speicherort/Schlüssel (OUT-233 `q_manifest` kennt nur
  `end_reason`), die Werte, die englischen Texte und die Regel, welche Grenzen überhaupt `limit_reached` ergeben – heute
  endet nur das URL-Limit mit `limit_reached`; eine Tiefengrenze lässt die Warteschlange leer laufen (`queue_empty`), eine
  Zeitgrenze gibt es in der Konfiguration nicht. **Vorschlag:** Nachtrag OUT-233/CRAWL-130: Metadatum `limit_kind` ∈
  {`max_urls`} (weitere Werte erst mit einer Zeitgrenze), im Manifest optional; Anzeige UI-013 „Fertig (Grenze erreicht:
  max. URLs)“ / „Completed (Limit reached: max URLs)“; ausdrücklich festhalten, dass die Tiefengrenze `queue_empty` bleibt
  (oder, falls gewünscht, als `limit_reached`/`max_depth` zählt – dann Engine-Änderung).
- **S2 (AGENT-020): Fehlercode `busy`.** Code und Form stehen seit Abschnitt 5 in AGENT-020; offen bleibt, wann genau er gilt. Vorher nur in `docs/plan/beta3.md`/B3-N00 beschrieben.
  Umgesetzt auf Anweisung des Orchestrators. **Vorschlag Nachtrag AGENT-020:** „Steuerwerkzeuge (`pause_crawl`,
  `stop_crawl`, `resume_crawl`, `start_crawl`) melden vorübergehende Belegung (Pause/Stopp läuft noch, Endzustand wird
  gespeichert, App führt eine andere Operation aus) als MCP-Fehler mit `structuredContent.error = {code: "busy",
  retryable: true, message}`; ein späterer Aufruf mit denselben Argumenten kann gelingen. Alle übrigen Fehler sind nicht
  wiederholbar.“
- **S3 (CRAWL-131 ↔ CRAWL-128): URLs hinzufügen zu einem gestoppten Crawl.** CRAWL-131 [P1] setzt einen
  „abgeschlossenen“ Crawl für zusätzliche URLs fort; ein gestoppter Crawl ist „Abgeschlossen (gestoppt)“, aber „Gestoppt
  ist endgültig gilt für alle Oberflächen“. Umgesetzt: abgewiesen, ohne etwas zu schreiben (sonst inkonsistente
  Warteschlange). **Vorschlag:** CRAWL-131 ergänzen „gilt nicht für Endgrund `stopped`“. Bei gegenteiliger Entscheidung
  entfällt die Prüfung in `add_urls_to_file` (4 Zeilen), und `prepare` bräuchte eine Ausnahme für diesen Weg.
- **S4 (Übersetzung):** CRAWL-128 nennt nur „Abgeschlossen (gestoppt)“. Englisch umgesetzt als „Completed (stopped)“,
  analog zu „Completed (Limit reached)“ (UI-013). Bitte bestätigen.
- **S5 (AGENT-020): Rückkehr von `pause_crawl`.** Die Spec legt nicht fest, ob `pause_crawl` auf „Pausiert“ wartet.
  STDIO antwortet sofort (`pausing: true`, wie `stop_crawl`), In-App-HTTP nach dem dauerhaften „Pausiert“ (Weg der
  Kopfleiste). Beides ist für Agenten über `crawl_status` gleich zu behandeln; ggf. vereinheitlichen.

- **S6 (OUT-182 ↔ CRAWL-128): nicht erreichbare Start-URL.** OUT-182 sagt „endet regulär … kein Fehler“, CRAWL-128 nennt sie als Beispiel für „Fehlgeschlagen“. Umgesetzt: beides zusammen (Ergebnis und Exit-Code nach OUT-182,
  Zustand nach CRAWL-128). **Vorschlag:** OUT-182 ergänzen „Zustand in der Verwaltung: Fehlgeschlagen“.
- **S7 (CRAWL-128): Was zählt als „nicht erreichbar“?** Umgesetzt: Netzfehler und eine deshalb nicht erreichbare robots.txt. **Nicht** als Fehlschlag: Sperre per robots.txt, zu viele Weiterleitungen, Zielschutz
  (`blocked_private_target`), außerdem Listen-Crawls und Crawls mit weiteren abgerufenen Seiten. Bitte bestätigen, vor allem den Zielschutz.
- **Folge von S6 für die Analyse, nicht getrennt festgelegt:** CLI und MCP lassen den Analyse-Hook für diesen Fall laufen, die Analyse gilt dort aber als unvollständig, weil der Crawl nicht `Completed` ist. Die App startet für `Failed` keine automatische Analyse.
- **Die 8 Fragen aus `b4-resume-tests.md`:** unverändert offen, nicht von mir entschieden.

## Testergebnisse (Server `openspider-build`, Worktree `m3-resume`)

Endstand `d1b8a0f0` (mit allen Auflagen), danach nur dieser Bericht committet. Playwright-Port `OS_E2E_PORT=14829`.

- `cargo fmt --all --check`: ok. `cargo clippy --workspace --all-targets -- -D warnings`: ok.
- `cargo test --workspace --exclude openspider-desktop -j 6 --locked --no-fail-fast` (über `ws-slot.sh`): **4 443 bestanden, 0 fehlgeschlagen, 50 ignoriert**, 609 Testprogramme, Exit 0;
  gegenüber `main` keine `#[ignore]` hinzugekommen. Der wackelige Abnahmetest (Abschnitt 6) war in diesem Lauf grün.
- **Abnahmetests:** in diesem Lauf `os-mcp` 19/19, `os-cli` 19/19; einzige Änderung ist A1.
- **`m2_process`-Test 20× unter Last:** Testlauf auf CPU 0–1 mit zwei `yes`-Prozessen je Lauf (mit `timeout`, danach beendet, 0 Reste): **20/20**.
- **Gegenproben:** A2 ohne Fix rot, B3 ohne Fix rot (Abschnitt 6), owner_pid-Wettlauf ohne Engine-Fix 5/5 rot (erster Stand).
- `npm run -w apps/desktop typecheck`: ok. `npm run -w apps/desktop test`: CSS-Lint ok, **Vitest 119 Dateien, 1 023 Tests bestanden**.
- Playwright mit echter Bridge, `--workers=1 --retries=0`:
  - `crawls/management.spec.ts`, `crawls/bridge.spec.ts`: Chromium 7/7, WebKit 7/7.
  - `beta/resume`, `beta/control`, `beta/n04-mcp`, `beta/services`, `header/header`, `statusbar/statusbar`, `alpha/crawl-control`, `mcp/dialog`, `polish/visual-heights`, `mcp/bridge` (mit dem echten 10-Minuten-Test): **Chromium 34/34, WebKit 34/34**.
  - Der erste Chromium-Lauf wurde nach 20 grünen Tests von außen beendet (Exit 143, SIGTERM, nicht von mir, Absender unbekannt). Die 34/34 stammen aus der vollständigen Wiederholung.
- `os-parity run --all`: **„Paritätslauf: bestanden“, 218/218 Vergleiche ok**, Exit 0.
- `bash scripts/check-neutral.sh`: keine Fundstellen.
- Nicht ausgeführt: macOS und Windows; die CI läuft nach dem Merge.
- `target/` des Server-Worktrees am Ende gelöscht.

## Fremde Dateien (K2-92)

| Datei | Änderung | Grund |
|---|---|---|
| `crates/os-engine/src/crawl.rs` | `publish` des Endzustands nach dem dauerhaften Commit; `CrawlHandle::is_finished` | Ursache des owner_pid-Wettlaufs liegt am Crawl-Ende, nicht in `lifecycle.rs` (Auftrag Punkt 3) |
| `crates/os-app/src/mcp/session.rs` | `pause_crawl`, `busy`-Abbildung, Stopp-Prüfung vor dem Fortsetzen | In-App-HTTP-MCP (AGENT-020); B4-LIVEDETAIL berührt `os-app/src/mcp` nicht |
| `crates/os-app/src/dto/crawls.rs`, `crates/os-app/src/commands/crawls.rs`, `apps/desktop/src/ipc/gen/CrawlListing.ts` | additives `endReason`; `can_resume` nur noch Kernregel | Verwaltung „Abgeschlossen (gestoppt)“ (CRAWL-128); generierte Datei per `dto_exports` geprüft |
| `crates/os-app/tests/cmd_mcp_service.rs` | neuer Test | Nachweis In-App-HTTP pause/resume/stop |
| `crates/os-app/tests/n00_stopped_state.rs` | eine Zusicherung `endReason` | Nachweis Listing mit echtem Stopp (Datei aus B3-N00) |
| `integrations/skills/openspider/references/tools.md`, `integrations/claude-plugin/skills/openspider/references/tools.md` | `pause_crawl`, `busy`, Texte zu Stopp/Fortsetzen | Validator vergleicht die Tabelle mit `tools::all()` (AGENT-041/042); beide Kopien byte-gleich (`sync-skill`) |
| `tests/fixtures/agent-m2-crawl/scenario.toml` | Routen `/held`, `/chain/1…12` | Fixture ohne natürliches Ende vor der Pause (B3-FLAKES) |
| `docs/spec/10-agent-interface.md`, `docs/spec/01-crawl.md`, `docs/spec/05-ui.md`, `docs/spec/06-output-ops.md` | Nachträge nach Fable-Entscheidungen (Abschnitt 6) | Auftrag, kein neues Verhalten |
| `crates/os-cli/tests/listing_live_owner.rs` | Test zu A2 | braucht `openspider` und `openspider mcp` als echte Prozesse; Datei kam über `main` aus B4-LISTFIX |
| `crates/os-cli/tests/acceptance_resume.rs` | `send_sigint`/`spawn_cli` portabel (A1) | vom Orchestrator freigegeben, sonst unverändert |
| `apps/desktop/e2e/beta/n04-mcp.spec.ts` | 17 statt 16 Werkzeuge | `pause_crawl` (AGENT-020) |
| `crates/os-mcp/tests/acceptance_resume.rs`, `docs/plan/M3-reports/b4-resume-tests.md` | per Merge aus `m3/resume-tests`, unverändert | Abnahmetests des Testagenten |
| `crates/os-cli/src/exec.rs`, `crates/os-cli/tests/cli_basics.rs` | `Failed` ohne Fehler bleibt gültiges Ergebnis; neuer Test | Befund 2, OUT-182 |
| `apps/desktop/src/features/crawls/i18n/{de,en}.ftl` | Schlüssel `crawls-phase-completed-stopped` | neue UI-Texte DE/EN; nicht unter `apps/desktop/src/i18n/**` (B4-LANG). Neue Sprachen von B4-LANG fallen für diesen Schlüssel auf Englisch zurück, bis sie ihn übersetzen |

Eigene Pfade: `crates/os-store/src/registry.rs`, `crates/os-store/tests/registry_management.rs`,
`crates/os-engine/src/lifecycle.rs`, `crates/os-engine/tests/b4_resume.rs`, `crates/os-cli/src/listing.rs`,
`crates/os-cli/tests/cli_load.rs`, `crates/os-mcp/**`, `apps/desktop/src/features/crawls/**`, dieser Bericht.

## Offene Punkte

- Offen: S1 `limit_kind` (Nutzer), Testbericht (8) (Fable), Schwelle des wackeligen Abnahmetests (Abschnitt 6).
- **Folgepunkte, nicht in diesem Paket:** B4 `stop_unowned` nicht atomar (vorbestehend), B5 Dialog UI-019 (eigenes Paket), `owner_kind`.
- Die App-Kopfleiste/Statuskarte bleibt unverändert (Statuskarte „Analyse läuft“ / „Gestoppt – Datei wird
  zusammengeführt“ ist ein eigener beta.4-Punkt aus B3-N00).
