# B4-RESUME – Crawls per MCP und CLI anhalten und fortsetzen – Bericht

Stand 06.11.2026. Branch `m3/resume` (Basis `main` 816e51d2), Mac-Worktree `openspider-wt/c-resume`, Server-Worktree
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

## Commits (Branch `m3/resume`)

| Commit | Inhalt |
|---|---|
| `967f6b68` | store: `can_resume` lehnt `stopped` ab + Test |
| `e81299ef` | engine: gestoppt ist endgültig, `stop_unowned`, Endzustand erst nach dauerhafter Freigabe von `owner_pid`, `CrawlHandle::is_finished` + `tests/b4_resume.rs` |
| `bce814f5` | cli: `--resume` gestoppt → Exit 1 + Test |
| `0d787fc6` | mcp: `pause_crawl`, `busy`, Werkzeugtexte, `m2_process` umgebaut, Fixture, Skill-Tabelle |
| `494515b7` | app: HTTP-MCP `pause_crawl`/`busy`/Fehlerabbildung, Listing `endReason` + Tests |
| `60eeacba` | ui: Verwaltung „Abgeschlossen (gestoppt)“ + Vitest |
| (letzter Commit) | docs: Bericht |

## Spec-Fragen

- **S1 (UI-013 Nachtrag beta.4 ↔ OUT-233/CRAWL-130): `limit_kind`.** UI-013 nennt „max. URLs“, „max. Tiefe“, „Zeit“,
  „sobald der Endgrund sie führt (`limit_kind`)“. Es fehlen: der Speicherort/Schlüssel (OUT-233 `q_manifest` kennt nur
  `end_reason`), die Werte, die englischen Texte und die Regel, welche Grenzen überhaupt `limit_reached` ergeben – heute
  endet nur das URL-Limit mit `limit_reached`; eine Tiefengrenze lässt die Warteschlange leer laufen (`queue_empty`), eine
  Zeitgrenze gibt es in der Konfiguration nicht. **Vorschlag:** Nachtrag OUT-233/CRAWL-130: Metadatum `limit_kind` ∈
  {`max_urls`} (weitere Werte erst mit einer Zeitgrenze), im Manifest optional; Anzeige UI-013 „Fertig (Grenze erreicht:
  max. URLs)“ / „Completed (Limit reached: max URLs)“; ausdrücklich festhalten, dass die Tiefengrenze `queue_empty` bleibt
  (oder, falls gewünscht, als `limit_reached`/`max_depth` zählt – dann Engine-Änderung).
- **S2 (AGENT-020): Fehlercode `busy`.** Nur in `docs/plan/beta3.md`/B3-N00 beschrieben, nicht in `docs/spec`.
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

## Testergebnisse (Server `openspider-build`, Worktree `m3-resume`)

- `cargo fmt --all --check`: ok. `cargo clippy --workspace --all-targets -- -D warnings`: ok.
- `cargo test --workspace --exclude openspider-desktop -j 6 --locked --no-fail-fast` (über `ws-slot.sh`): **4 357 bestanden,
  0 fehlgeschlagen, 50 ignoriert**, 602 Testprogramme, Exit 0 (keine neuen Ignores).
- Gezielt (vor dem Gesamtlauf):
  - `os-engine`: `b4_resume` 4/4 (neu), `lifecycle_pause` 13 + 1 ignoriert (unverändert), `core_stop`, `lifecycle_hook`,
    `add_urls_m2` grün.
  - `os-store` `registry_management` 14/14 (darin neu `a_stopped_crawl_is_never_resumable_whatever_its_queue_holds`).
  - `os-mcp` Unit 85/85 (darin `busy_is_a_retryable_error_code_and_other_errors_stay_plain_text`, Werkzeugliste),
    `m2_process` 4/4; die übrigen Integrations-Tests über den Gesamtlauf.
  - `os-cli` `cli_load` 10/10 (darin neu `resume_refuses_a_stopped_crawl_with_exit_1_whatever_its_queue_holds`).
  - `os-app`: `cmd_mcp_service` 6/6 (neu `http_pause_resume_and_stop_follow_the_app_session_and_stopped_is_final`),
    `n00_stopped_state` 2/2 (+ `endReason`), `dto_exports` 7/7 (TS-Vertrag), `beta_stop` 2/2.
- **`m2_process`-Test 20× unter Last:** Testbinärdatei, MCP-Kind und Testsite auf CPU 0–1 (`taskset`), dort je Lauf zwei
  `yes`-Lastprozesse (mit `timeout`, danach beendet, 0 Reste): **20/20 grün**, 4,7–5,8 s je Lauf.
- Gegenprobe owner_pid-Wettlauf ohne Engine-Fix: 5/5 rot (siehe oben).
- `npm run -w apps/desktop typecheck`: ok. `npm run -w apps/desktop test`: CSS-Lint ok, **Vitest 117 Dateien,
  1 012 Tests bestanden** (neu: Verwaltung „Completed (stopped)“/„Abgeschlossen (gestoppt)“).
- Playwright mit echter Bridge (`os-app-bridge`, `--workers=1 --retries=0`, Engines nacheinander; Hauptspecs über eine
  nur im Server-Worktree angelegte Konfiguration mit Vite-Port 1447, danach gelöscht):
  `crawls/management.spec.ts` + `crawls/bridge.spec.ts` (eigene Konfiguration) und `beta/resume.spec.ts`,
  `beta/control.spec.ts`, `header/header.spec.ts`, `statusbar/statusbar.spec.ts`, `alpha/crawl-control.spec.ts`:
  **Chromium 18/18, WebKit 18/18**.
- `bash scripts/check-neutral.sh`: keine Fundstellen.
- `os-parity run --all` (`--openspider target/debug/openspider`): **„Paritätslauf: bestanden“, 218/218 Vergleiche ok**, 10 Sites, Exit 0.
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
| `apps/desktop/src/features/crawls/i18n/{de,en}.ftl` | Schlüssel `crawls-phase-completed-stopped` | neue UI-Texte DE/EN; nicht unter `apps/desktop/src/i18n/**` (B4-LANG). Neue Sprachen von B4-LANG fallen für diesen Schlüssel auf Englisch zurück, bis sie ihn übersetzen |

Eigene Pfade: `crates/os-store/src/registry.rs`, `crates/os-store/tests/registry_management.rs`,
`crates/os-engine/src/lifecycle.rs`, `crates/os-engine/tests/b4_resume.rs`, `crates/os-cli/src/listing.rs`,
`crates/os-cli/tests/cli_load.rs`, `crates/os-mcp/**`, `apps/desktop/src/features/crawls/**`, dieser Bericht.

## Offene Punkte

- Spec-Nachträge S1–S5 (Planer/Fable). `limit_kind` folgt nach S1.
- **Serverplatte:** Ein vollständiger `cargo test --workspace` belegt je Worktree rund 120 GB `target/`. Am Vormittag
  liefen B4-M2CLOSE und mein erster Gesamtlauf gleichzeitig, die Platte lief voll (0 B frei; mein Lauf brach beim Linken ab,
  der gleichzeitige Lauf von B4-M2CLOSE sollte auf Folgefehler geprüft werden). `ws-slot.sh` prüft nur 25 GB frei; für zwei
  gleichzeitige Gesamtläufe reicht das nicht. Mein `target/` wurde sofort gelöscht, der wiederholte Gesamtlauf startete
  erst bei ≥ 135 GB frei (trotzdem zeitweise nur 12 GB frei, weil danach zwei weitere Läufe begannen). Vorschlag: Schwelle
  in `ws-slot.sh` auf rund 130 GB.
- Die App-Kopfleiste/Statuskarte bleibt unverändert (Statuskarte „Analyse läuft“ / „Gestoppt – Datei wird
  zusammengeführt“ ist ein eigener beta.4-Punkt aus B3-N00).
