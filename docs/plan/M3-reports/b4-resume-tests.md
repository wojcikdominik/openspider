# B4-RESUME – unabhängige Abnahmetests

Branch `m3/resume-tests` (ab Stand `m3/resume` 45b043ce), nur Testdateien:

- `crates/os-mcp/tests/acceptance_resume.rs` (19 Tests, MCP-Prozess `openspider-mcp` über STDIO)
- `crates/os-cli/tests/acceptance_resume.rs` (19 Tests, Binary `openspider`, `openspider mcp`, SIGINT/SIGKILL)

Grundlage ausschließlich `docs/spec/*` (AGENT-020, AGENT-001, AGENT-003, CRAWL-125/126/128/130, UI-003/013, OUT-204/228/233).
Code nur gelesen, um Aufrufe zu kennen. Nicht gelesen: Bauer-Bericht, `beta3.md`, Commit-Nachrichten, `m2_process.rs`.

## Methode

- Fixture: synthetische `os-testsite`-Site mit 40 Seiten (`synthetic:n=40,links=4,seed=7`). Die langsame Variante antwortet je Seite
  250 ms verzögert und braucht mit einem Thread mindestens 10 s bis zum natürlichen Ende; Pause, Stopp und Abbruch greifen nach
  5 bis 10 Seitenabrufen, also sicher davor. Die schnelle Variante ohne Verzögerung liefert den ununterbrochenen Referenzlauf
  (Menge aus Pfad, Statuscode, Tiefe; bei mehreren Threads nur Pfad und Statuscode, weil die Tiefe von der Abrufreihenfolge abhängt).
- „Keine doppelten oder verlorenen URLs“: Menge der gecrawlten URLs (`q_urls`) gleich Referenz, und kein Pfad zweimal im
  Request-Log der Testsite. Nach SIGKILL dürfen nur nicht gespeicherte URLs erneut abgerufen werden (CRAWL-125).
- Zustand und Endgrund aus `q_manifest` (OUT-233), Besitzer aus `crawl_meta` (`owner_kind`, `owner_pid`), Status der Verwaltung aus
  `--list-crawls --json`. „Datei unverändert“ heißt: `q_manifest`, `crawl_meta` und gecrawlte URL-Menge vor und nach dem Aufruf gleich
  (nachdem die automatische Crawl-Analyse eines beendeten Crawls durch ist).
- MCP-Crawls laufen gegen `127.0.0.1`; dafür setzt der Test `allow_private_targets = true` in der `mcp.toml` seines Heimatordners.
- Alle Tests tragen `// spec: <ID>` und `// acceptance: independent`.

## Ergebnis

Lauf auf dem Build-Server, je 5 Wiederholungen mit `--test-threads=3`:

| Datei | Tests | grün | rot |
|---|---|---|---|
| `os-mcp/tests/acceptance_resume.rs` | 19 | 19 | 0 |
| `os-cli/tests/acceptance_resume.rs` | 19 | 16 | 3 (in allen Wiederholungen rot, nicht wackelig) |

Gesamt: 38 Tests, 35 grün, 3 rot. Die drei roten Tests sind Befunde (siehe unten); die 35 grünen waren in allen Läufen stabil.

## Kriterium → Test → Ergebnis

Dateikürzel: **M** = `os-mcp/tests/acceptance_resume.rs`, **C** = `os-cli/tests/acceptance_resume.rs`.

### Freigaben (AGENT-001, AGENT-003)

| Kriterium | Test | Ergebnis |
|---|---|---|
| Ohne Schalter und mit `--read-only` kein Crawl-Werkzeug in `tools/list`; mit `--allow crawl` alle fünf (`start_crawl`, `crawl_status`, `pause_crawl`, `stop_crawl`, `resume_crawl`), aber kein `write`-Werkzeug; `--allow read` ändert nichts | M `crawl_tools_are_listed_only_with_the_crawl_class` | grün |
| Gespeicherte App-Freigabe (`classes` mit `crawl`) gilt ohne Schalter, `--read-only` überschreibt sie | M `app_configuration_enables_crawl_tools_and_read_only_overrides_it` | grün |
| Aufruf eines nicht freigegebenen Crawl-Werkzeugs: MCP-Fehler (`isError`) mit Freigabehinweis (ohne Schalter und mit `--read-only`) | M `calling_a_crawl_tool_without_approval_is_an_mcp_error_with_a_hint` | grün |
| Dasselbe über das echte Binary `openspider mcp` | C `openspider_mcp_lists_crawl_tools_only_with_the_crawl_class_and_refuses_calls_otherwise` | grün |
| `--read-only --allow crawl` startet nicht (Exit 2, stderr nennt beide Schalter); `--read-only --allow read` startet | C `read_only_together_with_allow_crawl_does_not_start` | grün |
| Unbekannte oder fehlende Crawl-ID bei `crawl_status`, `pause_crawl`, `stop_crawl`, `resume_crawl` ist ein MCP-Fehler | M `control_tools_report_unknown_crawl_ids_as_mcp_errors` | grün |

### Pause (AGENT-020 `pause_crawl`, CRAWL-126, CRAWL-130)

| Kriterium | Test | Ergebnis |
|---|---|---|
| Nach `pause_crawl` startet binnen 1 s keine neue Anfrage (höchstens Threads-viele laufende); Zustand `Paused` (Manifest `completeness = paused`) bleibt stabil, kein natürliches Ende; laufende Anfragen sind gespeichert (gespeicherte Seiten = abgerufene Seiten) | M `pause_crawl_halts_new_requests_and_the_crawl_stays_paused` | grün |
| `crawl_status` liefert Zustand, Fehlerzähler, Fortschrittsobjekt; Engine schreibt `owner_kind = mcp-stdio` und `owner_pid` = PID des MCP-Prozesses | M `crawl_status_reports_state_progress_and_errors_while_running` | grün |
| CLI: erstes SIGINT pausiert, Exit 130, letzte stderr-Zeile nennt den Exit-Code, Verwaltung zeigt `Paused`, Warteschlange nicht leer | C `sigint_pauses_with_exit_130_and_resume_equals_an_uninterrupted_crawl` | grün |
| CLI: nach SIGINT keine weiteren Anfragen, abgerufene Seiten sind gespeichert | C `sigint_stops_new_requests_promptly` | grün |

### Fortsetzen: gleiches Ergebnis ohne doppelte oder verlorene URLs (CRAWL-126, CRAWL-128, AGENT-020)

| Kriterium | Test | Ergebnis |
|---|---|---|
| MCP: Pause, `resume_crawl` im selben Prozess; Zustand Läuft → Pausiert → Läuft → Abgeschlossen; gleiche Kennung, nur eine Crawl-Datei; Menge, Statuscodes und Tiefen gleich Referenz; keine URL doppelt | M `pause_and_resume_in_one_session_equals_an_uninterrupted_crawl` | grün |
| MCP: Pause, Server beenden, neuer Prozess setzt fort („nach Neustart“) | M `a_paused_crawl_resumes_in_a_new_server_process_with_the_same_result` | grün |
| MCP: drei Pause-/Fortsetzen-Zyklen mit Standard-Threadzahl (parallele Anfragen); keine Anfragen während der Pause | M `repeated_pause_and_resume_cycles_with_parallel_threads_lose_and_duplicate_nothing` | grün |
| MCP: Listenmodus (`url_list`) mit Pause und Fortsetzen | M `a_list_crawl_survives_pause_and_resume` | grün |
| CLI: `--load-crawl <ID> --resume` nach SIGINT führt den Crawl zu Ende (Exit 0, Status `Completed`, gleiche ID, Menge gleich Referenz, jede Seite genau einmal) | C `sigint_pauses_with_exit_130_and_resume_equals_an_uninterrupted_crawl` | grün |
| CLI: zwei SIGINT-Zyklen mit 5 Threads, danach Fortsetzen; keine Anfragen nach dem Pausieren, kein zweiter Crawl angelegt | C `two_sigint_cycles_with_parallel_threads_lose_and_duplicate_nothing` | grün |
| Zwischen den Oberflächen: CLI pausiert, MCP `resume_crawl` setzt fort | C `a_cli_paused_crawl_is_resumed_by_mcp_with_the_same_result` | grün |
| Zwischen den Oberflächen: MCP pausiert, CLI `--resume` setzt fort | C `an_mcp_paused_crawl_is_resumed_by_the_cli_with_the_same_result` | grün |

### Unterbrechung per Prozessabbruch (CRAWL-125, CRAWL-128, OUT-228)

| Kriterium | Test | Ergebnis |
|---|---|---|
| MCP-Server per SIGKILL: Datei konsistent (`integrity_check = ok`, an einer Kopie geprüft), nach Ausbleiben des Heartbeats `Incomplete` (Manifest `interrupted`), `resume_crawl` im neuen Prozess führt zur Referenzmenge, bereits gespeicherte URLs werden nicht erneut abgerufen | M `a_killed_server_leaves_an_incomplete_crawl_that_resumes_without_duplicates` | grün |
| CLI per SIGKILL: Verwaltung zeigt `Incomplete`, `--resume` setzt fort, Menge gleich Referenz, gespeicherte URLs nicht erneut abgerufen | C `a_killed_cli_crawl_is_listed_as_incomplete_and_resumes_without_duplicates` | grün |

### Stopp ist endgültig (AGENT-020 `stop_crawl`, UI-003, CRAWL-128)

| Kriterium | Test | Ergebnis |
|---|---|---|
| `stop_crawl` auf laufendem Crawl: Endzustand `Completed` mit Endgrund `stopped` (`completeness = stopped`), Warteschlange nicht abgearbeitet; sobald der Endzustand gemeldet ist, ist `owner_pid` leer; automatische Crawl-Analyse läuft durch; `resume_crawl` danach MCP-Fehler mit „gestoppt – endgültig beendet“, Datei unverändert, keine Anfragen; gilt auch nach Neustart des Servers | M `stop_crawl_ends_a_running_crawl_for_good` | grün |
| `stop_crawl` nach einer Pause, im selben Prozess: wie oben | M `stop_crawl_after_a_pause_is_final_in_the_same_session` | grün |
| `stop_crawl` auf einem Pausierten Crawl eines früheren Prozesses: wie oben | M `stop_crawl_on_a_paused_crawl_of_an_earlier_process_is_final` | grün |
| `stop_crawl` nach Pause und Fortsetzen: wie oben | M `stop_crawl_after_a_resume_is_final` | grün |
| CLI-pausierter Crawl, per MCP gestoppt: `owner_pid` leer, Analyse läuft, Verwaltung `Completed`; MCP `resume_crawl` Fehler mit Hinweis; CLI `--resume` Exit 1 mit „gestoppt – endgültig beendet“, Datei und Site unverändert | C `a_crawl_stopped_via_mcp_is_final_for_cli_and_mcp` | grün |
| MCP-gestarteter Crawl, per MCP gestoppt, danach CLI `--resume`: Exit 1 mit Hinweis | C `a_running_mcp_crawl_stopped_via_mcp_is_final_for_the_cli` | grün |

### Zustandsmatrix `resume_crawl` / `--resume` (CRAWL-128)

| Zustand | MCP | CLI | Ergebnis |
|---|---|---|---|
| Pausiert | fortgesetzt (M `pause_and_resume…`, `a_paused_crawl_resumes…`) | fortgesetzt (C `sigint_pauses…`) | grün |
| Unterbrochen | fortgesetzt (M `a_killed_server…`) | fortgesetzt (C `a_killed_cli_crawl…`) | grün |
| Limit erreicht | `resume_crawl` erlaubt, endet ohne Erhöhung sofort wieder am Limit, nichts zusätzlich abgerufen (M `a_crawl_ended_by_the_url_limit…`) | gleiches Verhalten, mit `--max-urls 1000` setzt es fort und erreicht die Referenzmenge (C `a_limit_ended_crawl_resumes_with_a_raised_limit…`) | grün |
| Gestoppt | Fehler mit Hinweis (M `stop_crawl_*`) | Exit 1 mit „gestoppt – endgültig beendet“ (C `a_crawl_stopped_via_mcp…`) | grün |
| Regulär fertig (`queue_empty`) | Fehler, Datei unverändert (M `resume_crawl_rejects_a_regularly_completed_crawl…`) | Exit 1, Meldung nennt den Zustand, Datei unverändert (C `resume_of_a_regularly_completed_crawl…`) | grün |
| Fehlgeschlagen (generierte Datei, Warteschlange nicht leer) | Fehler, Datei unverändert (C `mcp_resume_crawl_rejects_a_failed_crawl…`) | Exit 1, Meldung nennt den Zustand (C `resume_of_a_failed_crawl_exits_1…`) | grün |
| Läuft (eigener Prozess) | Fehler, Crawl läuft unbeeinflusst weiter (M `resume_crawl_rejects_a_running_crawl_and_the_crawl_carries_on`) | – | grün |
| Läuft (fremder Prozess) | – | abgelehnt (Exit ≠ 0), Crawl läuft weiter und endet regulär (C `resume_of_a_running_crawl_of_another_process_is_refused…`) | grün |

### Fremdprozess-Crawls (AGENT-020)

| Kriterium | Test | Ergebnis |
|---|---|---|
| Crawl eines anderen MCP-Prozesses: lesend als `Running` sichtbar, `pause_crawl`, `stop_crawl`, `resume_crawl` liefern MCP-Fehler, der Crawl endet unbeeinflusst regulär und vollständig; Metadaten `owner_kind = mcp-stdio`, `owner_pid` = PID des Besitzers | M `a_second_server_cannot_control_a_crawl_run_by_another_process` | grün |
| Crawl der CLI: Metadaten `owner_kind = cli`, `owner_pid` = PID der CLI; MCP kann ihn nicht steuern | C `mcp_cannot_control_a_crawl_run_by_the_cli` und `resume_of_a_running_crawl_of_another_process…` | grün |

### Rote Tests (Befunde)

| Kriterium | Test | Ergebnis |
|---|---|---|
| CRAWL-128: Crawl mit nicht erreichbarer Start-URL ist „Fehlgeschlagen“ (OUT-204: mit `--fail-on-start-error` Exit 3) | C `a_crawl_whose_start_url_is_unreachable_is_listed_as_failed` | **rot** (Befund 2) |
| OUT-228/CRAWL-125/AGENT-020: `--list-crawls` neben einem regulär endenden MCP-Crawl lässt die Datei konsistent | C `listing_crawls_while_an_mcp_crawl_ends_regularly_keeps_the_file_intact` | **rot** (Befund 1) |
| dasselbe neben einem `stop_crawl`, das gerade abschließt | C `listing_crawls_while_an_mcp_stop_is_finishing_keeps_the_file_intact` | **rot** (Befund 1) |

## Befunde

1. **Schwer, vorbestehend (nicht von B4 eingeführt): `openspider --list-crawls` neben einem endenden MCP-Crawl zerstört die Crawl-Datei.**
   Läuft `--list-crawls --json` wiederholt, während ein vom MCP-Server (`start_crawl`) gestarteter Crawl endet und seine automatische
   Crawl-Analyse schreibt, bricht die Analyse im Server mit `database disk image is malformed` oder `crawl_meta: Schlüssel "crawl_id" fehlt`
   ab, weitere Werkzeugaufrufe melden `disk I/O error`, und die Datei ist hinterher beschädigt (`PRAGMA integrity_check`:
   `wrong # of entries in index url_ext_data_change_seq` bzw. `btreeInitPage() returns error code 11`; Analyse bleibt auf `pending`;
   die Datei fehlt anschließend in `--list-crawls`, das dann `[]` ausgibt). Reproduzierbar in 5 von 6 Läufen, sowohl nach regulärem
   Ende (ohne Pause, Stopp oder Fortsetzen) als auch nach `stop_crawl`. Nachgeprüft auf dem Stand vor dem Paket (`26d7ceb4`):
   dort ebenso 6 von 6 rot, das Paket hat den Fehler also nicht eingeführt. Nicht betroffen: Auflisten während der laufenden
   Abrufe und während einer Pause (6 von 6 grün), reines `crawl_status`-Abfragen ohne Auflisten (8 von 8 grün).
   Die Spec verlangt diese Verträglichkeit (AGENT-020: Fremdprozess-Crawls erscheinen live in der Crawl-Verwaltung; CRAWL-125: Datei konsistent).
   Vermutete Ursache (nicht bestätigt): `--list-crawls` öffnet jede Datei mit einer Lese-/Schreibverbindung (`open_raw_metadata`,
   Kommentar „als letzte Verbindung entfernt SQLite beim Schließen `-wal`/`-shm`“); das stimmt nicht, solange ein anderer Prozess
   die Datei noch beschreibt, oder die Sperren des Besitzerprozesses gehen verloren (Kopfprüfung per `std::fs` auf derselben Datei im
   Prozess mit offener Verbindung). Kein B4-Mangel, aber ein Fehler, der mit dem Anhalten und Stoppen per MCP (Agent fragt Status, Nutzer hat
   die App offen) praktisch sichtbar wird. Gehört in ein eigenes Paket.
2. **Mittel, vorbestehend: Start-URL nicht erreichbar → Zustand `Completed` statt `Failed`.** `openspider --crawl http://127.0.0.1:1/
   --fail-on-start-error` endet mit Exit 3 und der Meldung „Start-URL nicht erreichbar“, die Verwaltung führt den Crawl aber als
   `Completed` (Log: „Crawl beendet … crawled=1 state=Completed“). CRAWL-128 nennt genau diesen Fall für „Fehlgeschlagen“. Folge für B4: Ein
   so beendeter Crawl wäre dank `Completed` mit leerer Warteschlange abgelehnt, aber der Zustand `Failed` entsteht im normalen Betrieb nie;
   die Zeilen „Fehlgeschlagen“ der Zustandsmatrix sind deshalb nur mit generierten Dateien geprüft (Testfixture-Vorgabe von CRAWL-128).
3. **Niedrig, Beobachtung: Verzögerung von `crawl_status` nach `resume_crawl`.** Unmittelbar nach erfolgreichem `resume_crawl` meldet
   `crawl_status` noch bis rund 180 ms lang `Paused`, obwohl die Antwort `Running` nennt (CRAWL-130: „Läuft → Pausiert → Läuft“). Der Test
   erlaubt bis 5 s und ist grün; ein Client, der direkt nach dem Fortsetzen auf „nicht mehr Paused“ prüft, kann das sehen.
4. **Niedrig, Beobachtung: `resume_crawl` direkt nach dem gemeldeten Endzustand eines gestoppten Crawls antwortet zunächst mit `busy`
   („still saving its final state; retry …“) statt mit dem Hinweis „gestoppt – endgültig beendet“.** Das Fenster dauert so lange, bis der Lauf
   inklusive Crawl-Analyse abgeschlossen ist. Für ein gestopptes Crawl ist „später erneut versuchen“ irreführend. Die Tests wiederholen
   deshalb bei `busy` (siehe Spec-Fragen) und prüfen den endgültigen Hinweis.

## Spec-Fragen (keine Tests erfunden)

1. **Fehlercode bei belegtem Crawl:** Die Spec nennt keinen `busy`-Code für Steuerbefehle auf einem Crawl, der gerade pausiert, stoppt oder
   abschließt (Verhalten des Bauers: wiederholbarer Fehler `busy` mit `structuredContent`). Offen: Soll `busy` Teil des Vertrags sein, und
   gilt er auch für `resume_crawl` auf einem gestoppten Crawl in der Abschlussphase (Befund 4)?
2. **Doppelte Steuerbefehle:** `pause_crawl` auf einem bereits pausierten oder beendeten Crawl, `stop_crawl` auf einem beendeten oder bereits
   gestoppten Crawl: Fehler oder idempotenter Erfolg? Die Spec schweigt.
3. **`stop_crawl` auf einem unterbrochenen Crawl (`Incomplete`):** UI-003 spricht von „laufendem oder pausiertem“ Crawl; der Bauer lässt auch
   unterbrochene Crawls stoppen. Gewollt?
4. **Grenzart (`limit_kind`, UI-013 Nachtrag beta.4):** Die Spec verlangt die Angabe „max. URLs“/„max. Tiefe“/„Zeit“ in der Oberfläche, sagt aber nicht,
   wo sie für CLI und MCP erscheint (Manifest? `crawl_status`?). Deshalb nicht geprüft.
5. **Rückgabe von `crawl_status`:** AGENT-020 nennt „Fortschritt, URLs/s, Fehler“, aber keine Feldnamen. Der Test prüft nur, dass `state`,
   `errors` (Zahl) und `progress` (Objekt) vorhanden sind.
6. **CLI `--resume` mit angepasster Konfiguration:** Die Spec sagt, das Limit lasse sich vor dem Fortsetzen erhöhen (CRAWL-126/128), legt für die
   CLI aber nicht fest, wie. Der Test benutzt `--max-urls <n>` zusammen mit `--load-crawl … --resume` (Kurzform für `--set limits.max_urls`, OUT-186); das funktioniert,
   sollte aber festgeschrieben werden. Ebenso offen: Exit-Code von `--resume` auf einem Limit-Crawl ohne Erhöhung (der Test verlangt nur „nicht 2“ und keine
   zusätzlichen Abrufe).
7. **Besitzer-Metadaten nach Pause:** AGENT-020 legt nur fest, dass `owner_kind` und `owner_pid` geschrieben werden. Ob `owner_pid` auch nach einer
   Pause geleert wird (die App würde sonst einen pausierten Crawl als „fremd gesteuert“ zeigen), steht nicht in der Spec. Geprüft ist nur der Stopp.
8. **Robots-Abruf beim Fortsetzen:** CRAWL-126 verlangt „keine URL im Request-Log doppelt“. Die Tests zählen nur Seitenabrufe, nicht `robots.txt` und
   Sitemaps; ob `robots.txt` beim Fortsetzen erneut gelesen werden darf, ist offen.

## Hinweise zum Lauf

- Server: `cargo test -j 6 --locked`, `CARGO_PROFILE_*_DEBUG=0`, `CARGO_INCREMENTAL=0`, `nice -n 10`; `cargo fmt --all --check` und
  `cargo clippy -p os-mcp -p os-cli --test acceptance_resume -- -D warnings` sind sauber.
- Die Testprozesse (MCP-Server, CLI-Kinder) werden bei Abbruch des Tests über `Drop` beendet; die Tests benutzen keine `pkill`-Muster.

## Nachtrag 07.11.2026: Kill-Tests spec-treu (Branch `m3/resume-tests-ci`)

**Problem.** `a_killed_server_leaves_an_incomplete_crawl_that_resumes_without_duplicates` (MCP) und `a_killed_cli_crawl_is_listed_as_incomplete_and_resumes_without_duplicates`
(CLI) verlangten nach SIGKILL pauschal `>= 8` gespeicherte URLs bei 10 verlangten Abrufen. Die Schwelle stammte nicht aus der Spec:
CRAWL-125 verlangt nur, dass „alle bis ≤ 2 s vor dem Abbruch gecrawlten URLs“ vorhanden sind; bei 250 ms je Seite sind das nur
die ersten rund zwei. Folge: rot bei 7 gespeicherten URLs (Wackler, abhängig vom Commit-Zeitpunkt des Writers).

**Korrektur.**
- Der Test hält im Request-Log der Testsite fest, welche Seiten bei einem Schnappschuss schon angefragt waren (mindestens 10), wartet
  `KILL_AFTER_SNAPSHOT` = 2,5 s weiter und beendet den Prozess erst dann. Jede Seite des Schnappschusses ist nach der Antwortverzögerung der
  Fixture (250 ms) abgerufen, also beim Abbruch mehr als 2 s fertig (Reserve 250 ms plus der Rest bis 2,5 s). Genau diese Menge muss nach dem Abbruch in
  der Datei stehen (`must_be_stored ⊆ stored_before`); die Fehlermeldung nennt die fehlenden Pfade.
- Voraussetzungen werden geprüft statt angenommen: Schnappschuss nicht kleiner als 10 Seiten (die Prüfung ist nie leer) und der Crawl darf vor dem Abbruch
  nicht enden (`page_request_count < reference.len()`); nach dem Abbruch muss `stored_before.len() < reference.len()` bleiben.
- Gegenprobe (nicht eingecheckt): `BATCH_MAX_WAIT` und `BATCH_MAX_WAIT_BACKLOG` im Writer des Servers (`os-engine/src/writer_thread.rs`) von
  100/500 ms auf 5 s gesetzt, damit der Writer „zu spät sichert“. Ergebnis: beide Tests 3 von 3 rot, mit „länger als 2 s vor dem Abbruch abgerufene URLs fehlen …:
  ["/", "/s1/p1.html", …]“. Danach zurückgesetzt. Der Test bleibt also scharf.
- Die übrigen Schwellen beider Dateien auf Tempoabhängigkeit gesichtet: Unverändert bleiben die aus der Spec stammenden Prüfungen (CRAWL-126:
  höchstens Threads-viele Anfragen nach der Pause, gespeichert = abgerufen; Limit 12 → 12 URLs) und reine Vorbedingungen, die bei langsamerem Rechner nur
  sicherer werden (Pause oder Stopp vor dem natürlichen Ende bei 250 ms je Seite und einem Thread). Geändert (Spec-Begründung CRAWL-126:
  „Tests, die ‚Pausiert‘ erwarten, nutzen eine Fixture, die vor der Pause nicht enden kann“):
  - `repeated_pause_and_resume_cycles_with_parallel_threads_…` (MCP) und `two_sigint_cycles_with_parallel_threads_…` (CLI) laufen mit mehreren Threads gegen
    die neue Fixture `SLOW_PARALLEL` (1 s je Antwort). Mit 5 Threads und 250 ms hätte der Crawl bei einem schnellen Rechner enden können, bevor
    der dritte Zyklus pausiert.
  - `sigint_pauses_with_exit_130_and_resume_equals_an_uninterrupted_crawl` (CLI): `stored.len() >= 8` ersetzt durch `stored.len() == page_request_count`
    (CRAWL-126: laufende Anfragen werden zu Ende geführt und gespeichert) – strenger und unabhängig vom Tempo.

**Nachweis (Server, `m3-resume-ci`, Stand `m3/resume` 922fd64f plus Teständerungen).**
- Beide Kill-Tests je 40 Läufe, davon je 20 unter Fremdlast (14 Busy-Loops mit `timeout 900` auf 16 Kernen, Lastmittel rund 14): 80 von 80 grün.
  Die Auswertung verlangt „1 passed“ (ein erster Versuch mit unvollständigem `--exact`-Namen hatte 0 Tests ausgeführt und wurde verworfen).
- Ganze Datei `acceptance_resume.rs` je einmal: MCP 19 von 19, CLI 19 von 19 grün (die drei früher roten CLI-Tests zu Befund 1 und 2 sind auf diesem Stand grün).
- `cargo fmt --all --check` und `cargo clippy -p os-mcp -p os-cli --test acceptance_resume -- -D warnings` sauber.
- Kein Produktfehler gefunden.
