# M4-K11 – unabhängige Abnahmetests (aktiver Besitzer außerhalb von `Running`)

- Zweig `ai/k11-tests`, Tests auf Bauer-Stand `561603df` (Spec von `main` `2aa3e03a`, nicht gemergt).
- Lauf: Mac mini, Runde 1 (Log `k11t-log/r1.log`, Prüfzeile `commit=ede6b9d5 BAUER=561603df`; der Baum enthielt bereits die Korrektur aus `309f6840`), `CARGO_TARGET_DIR` = Bauordner des Bauers (Anweisung Orchestrator). `cargo fmt --all --check` grün, `cargo clippy -D warnings` der eigenen Testziele grün.
- Grundlage ausschließlich `docs/spec/08-ai.md` AI-020, Nachtrag 26.11.2026 (Unterpunkt „Aktiver Besitzer außerhalb von `Running`“, Kriterien „Löschschutz“ und „Besitzer eines KI-Laufs“), `docs/spec/01-crawl.md` CRAWL-125 (Nachtrag K4-22), AGENT-020, OUT-228/229. Bauer-Bericht, Bauer-Tests und Bauer-Commit-Nachrichten nicht gelesen. Gelesen wurden nur die öffentliche Schnittstelle (`os_store::owner`, `registry`, `resume`, `os_ai::run_owner` ohne Unit-Tests) und für die Durchsicht das Produkt-Diff.
- Keine Wanduhr: `ManualClock`, angehaltene tokio-Zeit, Heartbeat über die Metadaten simuliert. CLI und MCP lesen die Systemuhr. Dort liegt der simulierte Heartbeat 60 s in der Zukunft (nach CRAWL-125 aktuell) bzw. ist älter als 10 s, Grenzfälle entfallen. Kein ai-mock.

## Dateien

| Datei | Inhalt |
|---|---|
| `crates/os-store/tests/acceptance_k11_support/mod.rs` | gemeinsame Hilfen (per `#[path]` in allen Crates), Orakel „CRAWL-125 allein“ |
| `tests/fixtures/crawl-files/k11-acceptance/v1_0_states.json` | 20 Dateien in allen Zuständen, wie 1.0 sie schreibt (selbst aus CRAWL-125 abgeleitet) |
| `crates/os-store/tests/acceptance_k11_rule.rs` | Leseregel, Wirkung (a), Fortsetzbarkeit, 1.0-Dateien, Link-Index |
| `crates/os-store/tests/acceptance_k11_claim.rs` | `claim`/`beat`/`release`, Konstanten |
| `crates/os-ai/tests/acceptance_k11_run_owner.rs` | `RunOwner`: Takt, Ende, fremd/passiv, Absturz, Übernahme, unterbrochener Crawl |
| `crates/os-engine/tests/acceptance_k11_engine.rs` | Fortsetzen/erneut crawlen/entfernen/stoppen, eigene PID, 1.0-Dateien, Konstante |
| `crates/os-app/tests/acceptance_k11_app.rs` | Verwaltung `busy`, Liste, nur lesendes Öffnen, Prüfung beim Auslösen, URLs hinzufügen, 1.0-Dateien |
| `crates/os-cli/tests/acceptance_k11_cli.rs` | `--resume` Exit-Code 1 „läuft noch (Prozess …)“, `--list-crawls` |
| `crates/os-mcp/tests/acceptance_k11_mcp.rs` | `stop_crawl`/`pause_crawl`/`resume_crawl` ohne `busy`, 11 s später, eigene PID, `list_crawls` |

## Testzahlen (Runde 1)

| Crate | Ziel | grün | rot | ignoriert |
|---|---|---|---|---|
| os-store | acceptance_k11_rule | 8 | 1 | 0 |
| os-store | acceptance_k11_claim | 7 | 1 | 0 |
| os-ai | acceptance_k11_run_owner | 7 | 2 | 0 |
| os-engine | acceptance_k11_engine | 7 | 0 | 0 |
| os-mcp | acceptance_k11_mcp | 4 | 0 | 0 |
| os-cli | acceptance_k11_cli | 2 | 2 | 0 |
| os-app | acceptance_k11_app | 6 | 1 | 0 |
| **Summe** | | **41** | **8** | **0** |

## Prüfpunkte je Kriterium

| Kriterium (Spec) | Tests | Ergebnis |
|---|---|---|
| (a) Kriterium 1 Grenzwerte: Completed/Paused/„Unterbrochen“ (gespeichert `Incomplete`), Zukunft, jetzt, 9 999, 10 000 → aktiv; 10 001, 1 h, `owner_pid` leer/fehlend → keiner; Zustand bleibt gespeichert; fremde und eigene PID | rule `boundary_values_…`, `a_stale_heartbeat_never_…` | grün |
| Leseregel ohne Ersatzkette: `heartbeat_at` fehlt, frisches `started_at`/`created_at` → kein aktiver Besitzer | rule `without_heartbeat_…` | grün |
| (b) Kriterium 2 Verwaltung: Löschen, Projekt verschieben, Datei verschieben, Exportieren, Sperren, Entsperren → abgewiesen; Aufbewahrung übergeht; jede PID (Wirkung a); Datei unverändert | rule `management_rejects_…`; app `management_answers_busy_…` (Code `Busy`) | grün |
| Kriterium 2 „11 s später gelingt das Löschen“ (bei 10 s noch belegt) | rule `eleven_seconds_…`, app (gleicher Test) | grün |
| Kriterium 2 Fortsetzen: Engine `resume_crawl` (Completed mit/ohne Warteschlange, Paused, Incomplete) „läuft noch (Prozess <pid>)“ | engine `resume_recrawl_and_stop_…`, `resuming_a_completed_crawl_…` | grün |
| Kriterium 2 Fortsetzen MCP `resume_crawl` „läuft noch“ + PID, ohne `busy` | mcp `resume_on_a_foreign_…` | grün |
| Kriterium 2 Fortsetzen CLI `--resume`: Exit-Code 1 und „läuft noch (Prozess <pid>)“ | cli: Paused grün; Completed mit Limit-Warteschlange **rot**, Completed leer **rot** (B1-1) | teils rot |
| Wirkung (b) erneut crawlen, entfernen, Stoppen ohne Engine → „läuft noch (Prozess <pid>)“; 11 s später Stoppen möglich | engine `resume_recrawl_and_stop_…`, `removing_urls_…`, `stopping_without_an_engine_…` | grün |
| Wirkung (b) MCP `stop_crawl`/`pause_crawl`: Fehler mit Art und PID, Text nicht `busy:`, kein `structuredContent.error.code = busy` (Kriterium f) | mcp `stop_and_pause_…` | grün |
| Mit eigener PID greift (b) nicht: Engine (kein „läuft noch“, Stoppen gelingt), App öffnet schreibbar, Liste bietet Fortsetzen an, MCP-Stopp mit Server-PID gelingt | engine `with_the_own_pid_…`, app `with_the_own_pid_…`, app `the_list_keeps_…`, mcp `stopping_succeeds_…` | grün |
| (d) Completed, `mcp-stdio`, fremde PID, aktueller Heartbeat → App öffnet nur lesend: `controllable=false`, `live`, Art `Mcp`, PID, Phase `completed`, Sitzung nicht schreibbar, Schreibbefehle `Unsupported`, `addUrls` abgewiesen, Datei unverändert | app `a_completed_crawl_with_a_foreign_mcp_owner_opens_read_only` | grün |
| Prüfung beim Auslösen (Sitzung ohne Besitzer geöffnet, danach fremder Besitzer): Fortsetzen, Stoppen, erneut crawlen → `Busy`, Datei unverändert | app `an_open_session_rejects_…` | grün |
| Hinzufügen von URLs in solcher Sitzung schreibt nicht | app `an_open_session_adds_no_urls_…` | **rot** (B1-2, bekannte Nacharbeit `crawl_add.rs`) |
| Kein Index-Schreiben bei fremdem aktivem Besitzer (`link_index::ensure_if_idle`), Kontrolle ohne Besitzer baut ihn | rule `the_deferred_link_index_…` | **rot** (B1-3, bekannte Nacharbeit) |
| Fortsetzbarkeit in der Verwaltung: fremder aktiver Besitzer → nicht fortsetzbar, nach 10 001 ms wieder; eigene PID fortsetzbar | rule `a_foreign_active_owner_makes_the_crawl_not_resumable_…`, app `the_list_keeps_…` | grün |
| (e) Wer schreibt: `claim` schreibt genau `owner_kind`, `owner_pid`, `heartbeat_at`, nie `state`; `beat` nur `heartbeat_at`; `release` `heartbeat_at` = Ende, `owner_pid` leer, `owner_kind` bleibt | claim `claim_writes_…`, `beat_writes_…` | grün |
| (e) `RunOwner` 7 s → ≥ 4 Werte (Start, 2, 4, 6 s), Abstand 2 s, `state` durchgehend gespeichert (Completed, Paused, Incomplete); `finish` → `owner_pid` leer, `heartbeat_at` = Endzeitpunkt; danach kein Schlag | run_owner `a_run_on_a_crawl_without_running_owner_…` | grün |
| (e) fremder aktiver Besitzer (Zukunft, 0, 9 999, 10 000 ms) bzw. fremd laufender Crawl → Start scheitert mit Art und PID, Datei unverändert; fremder Prozess kann nicht `beat`/`release` | run_owner `a_foreign_active_owner_…`, claim `a_foreign_…` (2 Tests) | grün |
| (e) eigener laufender Crawl → passiv, 7 s nichts geschrieben, `finish` leert `owner_pid` nicht, Zustand `Running` | run_owner `during_the_own_running_crawl_…`, claim `during_the_own_…` | grün |
| Absturz: `owner_pid` bleibt, kein Schlag nach dem Verwerfen, ≤ 10 000 ms noch belegt, > 10 s kein aktiver Besitzer, nächster Lauf (anderer Prozess) überschreibt | run_owner `after_a_crash_…`, `a_stale_foreign_owner_…`, claim `after_a_crash_…` | grün |
| Übernahme nach dem Ende des eigenen Crawls (Nachtrag 26.11.): passiv, Engine-Ende leert `owner_pid` → spätestens nach 2 s wieder eigene PID, Heartbeat weiter, Zustand Endzustand, `finish` leert | run_owner `when_the_own_crawl_ends_first_…` | **rot**, erwartet (Nacharbeit des Bauers läuft; B1-4) |
| Übernahme: fremder aktiver Besitzer nach dem Engine-Ende → KI-Lauf schreibt nichts, `finish` gibt ihn nicht frei | run_owner `after_the_own_crawl_ends_the_run_never_overwrites_…` | grün (derzeit trivial, da passiv nichts schreibt; nach der Nacharbeit aussagekräftig) |
| KI-Lauf auf „Unterbrochen“ in 1.0-Form (gespeichertes `Running`, veralteter Heartbeat) | claim `a_claim_on_an_interrupted_crawl_…`, run_owner `a_run_on_an_interrupted_crawl_…` | **rot** (B1-5, B2-1) |
| (c) Kriterium 6, Dateien aus 1.0 (20 Fälle): Zustand, „belegt“, „fremd geführt“, Fortsetzbarkeit, Index-Eintrag, Sperren, Exportieren, Aufbewahrung, Löschen wie CRAWL-125 allein | rule `files_from_1_0_…`; app `files_from_1_0_list_and_delete_…` (Phase, `canResume`, `live`, `busy`); engine `files_from_1_0_…` (Stoppen: „läuft noch (Prozess <pid>)“ genau bei laufend); cli/mcp Listen (ohne Grenzfälle) | grün |
| Abgeleiteter Zustand der Listen unverändert bei aktivem Besitzer (CLI `--list-crawls`, MCP `list_crawls`) | cli/mcp `…list…` | grün |
| Konstanten: `HEARTBEAT_INTERVAL` 2 s in os-store, os-ai, os-engine gleich; Grenze 10 000 ms | claim, run_owner, engine | grün |

## Befunde

| ID | Schwere | Befund |
|---|---|---|
| B1-1 | B1 (bekannt, Nacharbeit `validate_resume`) | Spec AI-020 Kriterium 2: „Fortsetzen (`resume_crawl`, CLI `--resume`) … mit „läuft noch“ abgewiesen“; Wirkung (b): CLI mit „läuft noch (Prozess <pid>)“. Erwartet bei `Completed` (leer und mit Limit-Warteschlange) mit fremdem aktivem Besitzer: Exit 1 und „läuft noch (Prozess 4294967294)“. Beobachtet: Exit 1, „Crawl im Zustand Completed kann nicht fortgesetzt werden“. Bei `Completed` mit Restwarteschlange weist die CLI damit sogar einen ohne Besitzer fortsetzbaren Crawl mit falscher Begründung ab. `Paused` ist korrekt. |
| B1-2 | B1 (bekannt, Nacharbeit `crawl_add.rs`) | Wirkung (b) „wie ein fremd geführter laufender Crawl“. Bei einer zuvor schreibbar geöffneten Sitzung fügt `crawl.addUrls` trotz fremdem aktivem Besitzer eine URL in die Datei ein: `urls` 0 → 1. Ursache: auch `os_engine::lifecycle::add_urls_to_file` prüft nur `Running`; die Funktion liegt nicht in `owns`. |
| B1-3 | B1 (bekannt, Nacharbeit `ensure_if_idle`) | `link_index::ensure_if_idle` baut den Index in die Datei eines fremden aktiven Besitzers (`Completed`). Erwartet wäre, dass er wie bei einem laufenden Crawl nichts schreibt (`false`); beobachtet: Rückgabe `true`, Index gebaut. |
| B1-4 | B1 (erwartet, Nacharbeit) | „Übernahme nach dem Ende des eigenen Crawls“ fehlt. Ein passiver `RunOwner` hat keinen Takt; nach dem Engine-Ende bleibt `owner_pid` leer. Beobachtet: `owner_pid = ""` 2,1 s nach dem Ende. Folge: Der Crawl ist während des restlichen KI-Laufs ungeschützt, Löschen ist möglich. |
| B1-5 | B1 (neu) | KI-Lauf auf einem unterbrochenen Crawl in 1.0-Form (gespeichertes `Running`, fremde PID, Heartbeat 60 s alt). `claim` trägt ein und schreibt `heartbeat_at` = jetzt. Damit gilt der Crawl nach CRAWL-125 als „laufend mit eigener PID“, und `beat`/`release` schreiben deshalb nicht mehr (Prüfung `running`). Beobachtet: nur zwei Heartbeat-Werte in 12 s (Start und der Wert von `finish` bei 12 s). Nach 10 s ist der Crawl nicht mehr belegt (Verwaltungs- und Löschschutz weg, obwohl der Lauf läuft). Hätte `finish` innerhalb von 10 s stattgefunden, bliebe `owner_pid` gesetzt (wie ein Absturz). Verletzt „alle 2 s `heartbeat_at` …, solange der KI-Lauf läuft“, „am Ende … leerem `owner_pid`“ und den Löschschutz. Fundstellen: `crates/os-store/src/owner.rs` `beat`/`release` (Bedingung `occupancy.running`) und `RunOwner::start`. |
| B2-1 | B2 | Spec-Widerspruch für „Unterbrochen“ in 1.0-Form: Der KI-Lauf schreibt `heartbeat_at`, aber nicht `state`. CRAWL-125 wertet ein gespeichertes `Running` über genau diesen `heartbeat_at` aus. Während des KI-Laufs zeigen alle Leser deshalb „Läuft“ statt „Unterbrochen“, entgegen AI-020: „Der Zustand des Crawls in der Verwaltung (CRAWL-128) ändert sich dadurch nicht“ und „Der abgeleitete Zustand bleibt, wie CRAWL-125 ihn bestimmt“. Zu klären: Darf ein KI-Lauf auf einem gespeicherten `Running` überhaupt laufen? Wenn ja, wie sind Zustand und Heartbeat zu trennen? Kein Test dazu, nur dieser Befund. Die Tests zu B1-5 prüfen nur die eindeutigen Teile (Takt, Freigabe, Belegtheit). |
| B3-1 | B3 | `crawls.duplicate` und `crawls.rename` werden bei aktivem Besitzer ebenfalls abgewiesen (gemeinsame Prüfung in `batch` bzw. `guard`). Die Spec nennt nur Löschen, Verschieben, Exportieren, Sperren und Entsperren; das Verhalten entspricht aber dem bei `Running` in 1.0. |
| B3-2 | B3 | `crawls.list` `controllable = own && !foreign_led`: Für einen eigenen, gerade angelegten Crawl (gespeichertes `Running`, frisches `created_at`, noch kein `owner_pid`) gilt „ohne PID = fremd“. Er erscheint bis zum ersten Batch als nicht steuerbar; 1.0 zeigte steuerbar. Kurzes Fenster, nur Anzeige. |
| B3-3 | B3 | Die Verwaltungsliste meldet bei fremden Crawls weiter `OwnerKind::Cli` ohne PID (Hinweis i, wie beauftragt unverändert). |

## Durchsicht des Produkt-Diffs (`12097d2e...561603df`, `crates/*/src`)

| Datei / Stelle | (i) innerhalb `owns` | (ii) Verhaltensänderung ohne aktiven Besitzer außerhalb `Running` | (iii) Anmerkung |
|---|---|---|---|
| os-store `owner.rs` (neu) | ja | – (neu) | B1-5 (Bedingung `running` in `beat`/`release`) |
| os-store `lib.rs` | ja (eine Zeile) | nein | |
| os-store `meta.rs` | ja (zwei Felder samt Lesen) | nein | |
| os-store `registry.rs` | weitgehend: zusätzlich `occupancy` und `foreign_led` (owns nennt nur `active_owner`/`occupied`; B3) | nein (`can_resume` nur bei aktivem fremdem Besitzer anders) | |
| os-store `registry/index.rs` | ja | nein (ältere Einträge werden einmal neu gelesen) | |
| os-store `registry/management.rs` | ja (`guard`, Filter in `retain_with`) | nein | `guard` wirkt auch auf `rename`/`duplicate` (B3-1) |
| os-engine `lifecycle.rs` `ensure_not_running` | ja | nein | `add_urls_to_file` (nicht in owns) prüft nur `Running` → B1-2 |
| os-engine `rescan.rs` `ensure_not_running_with` | ja | nein | |
| os-engine `crawl.rs` `HEARTBEAT_INTERVAL` | ja | nein (Wert 2 s, Test grün) | |
| os-ai `lib.rs`, `run_owner.rs` (neu) | ja | – | passiv ohne Übernahme (B1-4) |
| os-app `commands/crawls.rs` (Liste, `short`, `batch`) | ja | nein; Ausnahme kurzes Fenster B3-2 | |
| os-app `session/feed.rs` (`snapshot`, `refresh_aligned`) | ja | nein (`live` = `Running` ohne aktiven Besitzer) | |
| os-app `session/control.rs` (`ensure_idle`, `stop_checkpoint`) | ja | nein | |
| os-mcp `crawl_service.rs` (`not_live`, `stop`) | ja | nein | |
| **Leser ohne neue Regel** | | | `os-cli` `listing::validate_resume` (B1-1), `os-store` `link_index::ensure_if_idle` (B1-3), `os-app` `commands/crawl_add.rs` und `os-engine` `add_urls_to_file` (B1-2). Nicht verlangt und unverändert: Vergleich (`os-app` `compare.rs`, MCP `compare_crawls`), CLI-/MCP-Listen (Zustand bleibt nach Spec). |

## Nicht geprüfte Kriterien

- Heartbeat des KI-Laufs mit ai-mock `hold`, KI während des eigenen laufenden Crawls mit echter Engine, Löschschutz mit `hold`, erster Satz von „Besitzer eines KI-Laufs“ → M4-D01 (Zuschnitt). Ohne KI-Ausführung sind sie über `RunOwner` mit simuliertem Heartbeat teilweise abgedeckt (siehe oben).
- „ai-mock zählt 0 Requests“ → M4-N01.
- `busy` der KI-Werkzeuge des MCP (AGENT-030) → M4-C02.
- Fortsetzen nach Ablauf der 10 s über CLI/MCP mit echtem Weitercrawlen (braucht eine Fixture-Site); abgedeckt über Stoppen ohne Engine, Verwaltung und Liste.
- Strukturierter Fehler statt Text (bekannte Nacharbeit) – die Tests prüfen Meldungstext bzw. `ErrorCode::Busy`.
