# Beta 3 – Arbeitsliste

Stand 04.11.2026. Sammelt alles, was nach beta.1/beta.2 für die nächste Beta beschlossen oder gefunden wurde. Reihenfolge innerhalb eines Abschnitts = Priorität. Pakete bekommen beim Start eine ID und einen Bericht unter `docs/plan/M3-reports/`.

## 1 MCP mit einem Klick verbinden (Nutzerentscheidung 04.11.2026)

Anlass: Ein Nutzer bat Codex, über OpenSpider einen Crawl zu starten. Codex kannte den Server nicht, und selbst nach dem Eintragen wäre nur Lesen freigegeben gewesen. Heute braucht die Einrichtung fünf Schritte in zwei Reitern (MCP → „Mit … verbinden…“ → Vorschau → Bestätigen → Schreiben, dann Reiter „Zugang“ → „Crawlen“).

- **Ein Dialog je Programm:** „Mit Codex verbinden“ (ebenso Claude Code, Claude Desktop) zeigt einen Haken „Agent darf Crawls starten“ (Klasse `crawl`, standardmäßig an in diesem Dialog, Lesen immer an) und einen Button „Verbinden“. Sicherung, Eintrag und Freigabe passieren in einem Schritt; die Vorschau bleibt aufklappbar. Abschluss mit klarer Anweisung: „Starte in <Programm> eine neue Sitzung.“ Bestehende Einträge werden erkannt („Bereits verbunden – aktualisieren?“).
- **Nur anhängen statt neu schreiben:** `os_mcp::client_config::merged` parst die Client-Datei und schreibt sie komplett neu (Codex-TOML: 130 von 176 Zeilen geändert, Reihenfolge und Format anders, Inhalt laut Prüfung vom 04.11. identisch). Künftig formatbewahrend ergänzen (z. B. `toml_edit`, JSON mit erhaltener Reihenfolge), sodass die Vorschau nur die neuen Zeilen zeigt.
- **Vorschau ohne Geheimnisse:** Die Vorschau zeigt die komplette Konfiguration fremder Programme. Werte, die nach Schlüssel/Token/Passwort aussehen (`env`-Einträge, `headers`, `*_KEY`, `*_TOKEN` …), maskieren; nur der eigene Eintrag erscheint vollständig.
- **Cursor:** Ein-Klick-Installation über Cursors Installationslink (App und Website).
- **Claude Desktop:** Installationspaket (Erweiterung) prüfen und, falls tragfähig, anbieten.
- **Website:** Abschnitt „MCP in 2 Minuten“ für Claude Code, Codex und Cursor (DE/EN), verlinkt von Start- und Funktionsseite.
- **Hilfetext korrigieren:** `mcp-classes-help` verspricht Kostenschätzung und Bestätigung für kostenpflichtige Aufrufe; das kommt erst mit AGENT-030 (M4). Bis dahin nicht zusagen.
- **Handbuch:** Abschnitt „MCP einrichten“ auf den neuen Ablauf umstellen, Hinweis auf die Freigabe `crawl`.
- Spec: AGENT-003 (Freigaben) und das zuständige UI-Kapitel um den Verbindungsdialog ergänzen, bevor gebaut wird.

## 2 Leistung

- **Rückschritt beim Crawl (Diagnose 04.11.2026, Build-Server, gleiche Methode, 100k URLs):** Stand 18.10. (31e2ddd3) 48,0 s, beta.2 85,9 s (+78 %). Crawl-Phase +25,6 s, neue Analyse +10,9 s (fast nur Link Score), Export/Speichern +1,2 s. Ursachen per Bisect: Merges M2-D10 (+5,6 s), D11 (+4,3 s), D02 (+15,8 s, Crawl-Datei verdoppelt), D08 (Link Score +17 s Analyse; E07 hat später ~5,5 s zurückgeholt). Bei 1 Mio.: Link Score 113 s. Funktionen sind gewollt (Spec-IDs), die Kosten liegen in der Umsetzung. Fix-Paket mit vier Teilen:
  1. Link Score im Speicher statt 10 SQL-Durchläufe über alle Kanten (`os-analyze` `link_score.rs:157-178`), erwartet −9 s/100k, −100 s/1 Mio.
  2. `write_ext` (`os-store/src/writer_m2.rs`) mit zwischengespeicherten Schlüssel-IDs und ohne `clear_text` bei neuen Seiten: 1 statt 3 Statements je Zeile, erwartet −5…−8 s.
  3. `content.soft404_text` nicht speichern, in der Analyse aus Titel/H1/Text bilden: −110 MB je 100k, größere Writer-Batches (`BATCH_BYTES` 8 MiB begrenzt sonst auf ~477 Seiten).
  4. Parser: Text des Inhaltsbereichs einmal je Seite, Selektoren statisch, Security- mit Link-Durchlauf zusammenlegen (`os-parse` `content.rs`, `readability.rs`, `security.rs`, `validation.rs`), erwartet −4…−8 s.
  **Nutzerentscheidung 04.11.2026:** Nur Teil 1 (Link Score im Speicher) und die Ursache der 1-Mio.-Skalierung (Mac: 100k 48 s, 1 Mio. 771 s = Faktor 16 statt 10) werden jetzt bearbeitet; Teile 2–4 ruhen zugunsten von Funktionen. Ursprüngliches Ziel war ~35 s für 100k auf dem Mac mini (heute 48 s). Messwerkzeuge und Logs: `~/perf` auf dem Build-Server. Zusätzlich prüfen: Durchsatzbudget nach PERF-002 für neue Analysen; warum 1 Mio. auf dem Mac relativ schlechter skaliert als 100k.
- **B3-PERF erledigt (gemergt 04.11.2026, Fable 91):** Link Score im Speicher, bitgleich. Server: 100k 85,6 → 77,5 s, 1 Mio. 1 082 → 975 s (Analyse 113 → 23,5 s); 5 Mio. Knoten/100 Mio. Kanten 47,8 s, 1,34 GB. Bericht `docs/plan/M3-reports/b3-perf.md`.
- **Ursache 1-Mio.-Skalierung (B3-PERF):** fast nur Crawl-Phase. (a) Writer-Kanal 256 Seiten < Batch ~464 → Writer und Worker warten abwechselnd (1 Mio.: Writer 235 s untätig, Worker 41 % blockiert); (b) zufällige `url_counters`-Updates mit Index `url_counters_change_seq` verteuern jeden Commit mit wachsendem N. Vorschläge: **V1** `CHANNEL_CAPACITY` 1024 + `IN_FLIGHT_BYTES` 32 MiB (`os-engine/src/writer_thread.rs`; Server 100k 77,8 → 53,3 s, 1 Mio. 1 082 → 703 s) – eigenes Paket, PERF-001/002/003/005 und K-49-Grenze neu messen bzw. herleiten, nicht lockern; V2 `BATCH_BYTES` 16 MiB nur zusammen mit V1; V3 Änderungsfeed ohne zufälligen Index (−146 s, Schema + Spec); V5 `--save-crawl` per Dateikopie statt `VACUUM INTO` (Spec OUT-230/CRAWL-127). Entscheidung Nutzer offen.
- **B3-QUEUE erledigt (gemergt 05.11.2026, Fable 86):** V1 umgesetzt mit `CHANNEL_CAPACITY` 1 024 (aus der Batchgröße abgeleitet) und `IN_FLIGHT_BYTES` 24 MiB (3 Batches). Server: 100k 78,0 → 54,0 s, 1 Mio. 986 → 699/712 s; PERF-001/002/003/005 grün. K-49 (Plan-Entscheidung, Orchestrator nach Fable-Empfehlung): Wiederholungsgrenze 5 032 statt 4 264 Seiten je Abbruch bei 8 Threads. **Auflage vor dem beta.3-Tag:** PERF-003 auf dem Mac mini (`N20_MAX_LOAD=2.5 bash crates/os-loadtest/scripts/n20-mac.sh <out>`, erwartet ~475 MB < 500 MB; Rückfall `IN_FLIGHT_BYTES = 5 * BATCH_BYTES / 2`). **Auflage erfüllt 05.11.2026:** Mac mini (M-Chip, 16 GB, 8 Threads, balanced) PERF-003 large 2/2 bestanden, Spitzen-RSS 454 MB und 472 MB (< 500 MB), PERF-002 1 467 bzw. 1 514 URLs/s; kein Rückfall nötig (Berichte lokal `codex-local/perf-mac-b3queue/`). V2 nicht übernommen. Folgeauftrag: Render-/M2-Nutzdaten in `estimate_bytes` zählen (Bericht `b3-queue.md`).
- Fable-Kleinigkeiten B3-PERF: `existing` bei Re-Analyse streamen statt 2·n Tupel im RAM (`link_score.rs:510`); Spec-Verweis `TECH-139` in `link_score_b3_perf.rs:323` und `link_score_m2.rs:258` korrigieren; Abbruchtest `link_score_m2.rs:192` trifft jetzt die Veröffentlichung statt der Iterationen.
- Mac-mini-Messung nach B3-PERF (100k, 1 Mio. mit `OS_SCALE_DIAG=1`, wenn der Mac frei ist); README-Abschnitt „Performance“ nachziehen.
- Aussage „5 Mio. URLs bleiben in der App flüssig“ (Werte aus Entwicklungsständen im Oktober) mit beta.2/beta.3 nachmessen.

## 3 Funde aus Beta-Tests (UI)

- Befunde-Seitenleiste: Namen auf Englisch in der deutschen Oberfläche.
- „Kein Crawl geöffnet.“ ohne Innenabstand.
- Start-URL leer, nachdem ein Crawl wieder geöffnet wurde.
- Fenstertitel der Visualisierung in der falschen Sprache.
- Update-Dialog: Buttons „Beim Beenden installieren“ / „Jetzt installieren und neu starten“ brechen zweizeilig um.
- „Hilfe → Nach Updates suchen“ öffnete in einer seit über 17 Stunden laufenden Instanz keinen Dialog; nach Neustart sofort. Ursache klären.
- Funde aus dem Windows-Test des Nutzers (stehen noch aus).
- V02 (Graphen/Wortwolke): offene Low-Punkte; V01 (Übersicht nach Teilmengen): Backlog aus dem Bericht.

## 4 Funktionen

- Chrome-Download auf Klick für JavaScript-Rendering (in beta.1/beta.2 als „folgt mit einer der nächsten Betas“ angekündigt).

## 4a Crawl-Vergleich für Deployments (Nutzerfrage 04.11.2026)

Geprüft mit beta.2 an `tests/fixtures/out-m2-compare` (v1 → v2): Vergleich per CLI (`--crawl-comparison`) markiert New/Added/Unchanged und Change Detection korrekt; ein SQL-Diff über zwei `.ospider`-Dateien (`ATTACH`) findet verschwundene Seiten, geänderte Canonicals und neue Weiterleitungen.

- **CLI-Export mit Unterfiltern:** Vergleichsexporte enthalten nur Zeilen des Current-Crawls; `Missing`/`Removed` (z. B. gelöschte Seiten in `Internal:HTML`, behobene 404 in `Response Codes:Client Error (4xx)`) lassen sich nicht exportieren. OUT-151 sieht das vor – Syntax für den Unterfilter in `--export-tabs` festlegen (Spec-Klärung) und umsetzen.
- **Canonical-Ziel in Change Detection:** Heute wird nur Indexability verglichen; ein Wechsel des Canonical-Ziels A → B bei gleichbleibendem „Canonicalised“ bleibt unsichtbar. Neues Element (Spec: TECH-300/OUT-144 erweitern), ebenso Weiterleitungsziel.
- **Report „Änderungen seit dem letzten Crawl“:** fertiger Report (App, CLI `--save-report`, MCP) mit verschwundenen Seiten, neuen Weiterleitungen und -ketten, geänderten Canonicals, Indexierbarkeitswechseln. Handbuch: Abschnitt „Deployment prüfen“ mit SQL-Beispiel.

## 4b Docker-Image (Nutzerentscheidung 04.11.2026)

Anlass: LinkedIn-Frage nach einer Self-Hosted-Docker-Lösung. Grundlage besteht (OUT-273 Linux-Serverbetrieb: CLI, Exporte, geplante Läufe, MCP über STDIO; Release-Archiv `openspider_<v>_linux-x86_64.tar.gz`).

- Dockerfile auf Basis des Linux-Release-Archivs (glibc, x86_64) mit Chromium für JS-Rendering (`OPENSPIDER_CHROMIUM`), nicht als root, Arbeitsordner `/data`, `OPENSPIDER_HOME` als Volume.
- Beispiele: Crawl mit Export (`docker run --rm -v "$PWD:/data" … --headless --crawl … --output-folder /data`), Vergleich mit dem vorherigen Crawl, MCP über `docker run -i … mcp`.
- Release-Workflow: Image bauen, Smoke-Test (Crawl gegen `os-testsite`, Export, MCP-Handshake), signieren/SBOM prüfen; Veröffentlichung in der GitHub Container Registry (`ghcr.io/wojcikdominik/openspider`) **nur nach ausdrücklichem Go des Nutzers**.
- Spec: OUT-273 bzw. neuer Eintrag im Release-Abschnitt (06) um das Image ergänzen; Handbuch-Abschnitt „Docker“; README-Installation.
- Klar sagen: keine Weboberfläche im Container (Desktop-App bleibt die UI).

## 5 Release-Ablauf

- Entwurfshinweise („unsigniert“, „Installations-Smoke“, „TODO M2-Parität“) nur in die Zusammenfassung des Actions-Laufs, nicht in den Release-Text (Fable-Empfehlung zu m3/rel-assets).
- Feedback-Link im Download-Kasten: bei stabilen Releases auf GitHub Discussions statt `/beta/`.
- Beta-Seiten der Website (`/beta/`, `/en/beta/`) direkt auf die versionslosen Downloads verlinken.
- Release-Notes stehen vor dem Bauen fest: Die Update-Notes in `latest.json` entstehen beim Bauen; eine spätere Änderung am Release-Text erreicht den Update-Dialog nicht.
- `scripts/release/licenses/generate.mjs`: Wiederholung bzw. Zwischenspeicher für nachgeladene Lizenztexte (beta.2-Windows-Job brach an einem Netz-Timeout ab).
- CHANGELOG-Einträge von `rel/beta1` und `rel/beta2` nach main; `compat_1.0.0-beta.1` nach `tests/fixtures/migrations/releases/` (K3-11).

## 6 Tests und CI

- Pause-vs.-Abschluss-Wettlauf in Fixtures (header.spec:69 WebKit, n04-live:74, `beta_resume.rs` a_new_paused_crawl… auf Windows): Testserver hält Antworten bis zur Pause zurück. Dazu Spec-Frage CRAWL-001: Was gilt, wenn Pause und Crawl-Ende zusammenfallen?
- macOS-Zeitgrenzen in `render_chromium.rs` (Zeile ~213 pixel.png, ~635 late_paint 1500 ms vs. 1533 ms).
- Pfadfilter: Commits, die nur `website/`, `README.md` oder `docs/` ändern, sollen laufende main-CI nicht abbrechen bzw. keine volle CI auslösen.
- Weitere bekannte Flakes: Intel, SMTP, `robots_step_m2`.
- Windows: `cmd_profile_runtime.rs:270` `secret_references_and_plain_exports_never_persist_credentials` liest beim Durchsuchen eine Datei, die noch eine SQLite-Verbindung sperrt (Os code 33, „locked a portion of the file“; main-CI f725eeeb, 04.11.2026). Test erst scannen lassen, wenn alle Verbindungen geschlossen sind – nicht lockern.
- `os-store` `crawl_files::damaged_file_is_detected_and_repaired_into_a_new_copy`: unter Serverlast 1362 statt ≥ 1900 gerettete URLs (B3-REL, 05.11.2026), einzeln 3/3 grün. Ursache laut Fable: `files.rs:487–516` zählt Schreibfehler am Ziel (SQLITE_BUSY/IOERR bei `commit` eines Bisektionsblocks) als verlorene Zeilen – echter Robustheitsmangel der Reparatur: Zielfehler wiederholen oder abbrechen statt als Datenverlust zählen.
- Windows `cmd_profile_runtime.rs:270` (Datei-Sperre beim Durchsuchen) zum zweiten Mal (main-CI 8b7cbd5e, 05.11.2026) → im Flake-Paket beheben.
- linux-core `os-engine/tests/memory_process.rs:89` `renderer_child_rss_triggers_pause_when_engine_alone_is_below_limit`: `renderer_bytes` 183 MB statt ≥ 190 MB (main-CI c51534b0, 05.11.2026); Test seit M2-E02 unverändert, Kindprozess-RSS auf dem Runner knapp unter der Schwelle. Ursache klären (Allokation vollständig berühren, bevor E02_MEMORY_READY gemeldet wird), Schwelle nicht senken.
- Aus B3-CURSOR (Fable): Scheitert `staged.persist()` nach der Sicherung (`os-app/src/mcp/clients.rs:254–295`), bleibt `*.<uuid>.bak` mit fremden Geheimnissen liegen → Sicherung best-effort löschen, eigener Fehler. JSONC-Meldung um „(z. B. Kommentare)“ ergänzen. Handbuch: Projekt-`.cursor/mcp.json` überdeckt den globalen Eintrag gleichen Namens.
- macOS-arm64-Runner bei GitHub zeitweise nicht verfügbar („not acquired by Runner … capacity constraints“, 04.11.2026): solche Abbrüche erneut starten, nicht als Testfehler werten.
- **macOS-WebKit-E2E in Spec-coverage-M2 (05.11.2026, korrigiert):** Die Vermutung „neues Runner-Image 20261104“ ist widerlegt (B3-MACIMG). Dieselben 19 Fehlschläge gab es schon auf Image 20261028 (Lauf 41989485643, als grün gemeldet, weil der Playwright-Exit nur im Gate zählt). Ursache: `spec-coverage-m2.yml` fehlten gegenüber `ui-e2e.yml` der Bau von `os-testsite`, macOS `AppleKeyboardUIMode` und die Linux-AppArmor-Freigabe; der Abbruch kam vom 60-min-Job-Limit (ein macOS-Job 47–54 min Testzeit). Fix auf `m3/macimg` (zwei macOS-Shards, Artefakte immer hochladen), Nachweis Lauf 42037881032 grün; Merge nach Fable-Freigabe. Diagnose-Zweige `diag/macos-image`, `diag/macos-image-2` auf origin.
- main-CI f6d98ccf (05.11.2026): linux-core `os-mcp/tests/m2_process.rs:509` `headless_crawl_start_status_stop_resume…`: nach `wait_stopped` steht `owner_pid` noch auf der PID statt leer (Wettlauf Zustand „gestoppt“ vs. Freigabe der Besitzer-Metadaten); derselbe Test setzt danach einen gestoppten Crawl fort – mit dem Paket „Fortsetzen/MCP“ (beta.4, AGENT-020) ohnehin umzubauen. macOS `os-store/tests/scale_storage.rs:43` „disk I/O error“ (SQLITE_IOERR_FSYNC 1034) auf dem Runner. Beide erneut gestartet.
- **B3-FLAKES erledigt (gemergt 05.11.2026, Fable 86 + Nacharbeit):** Pause-Fixtures ohne natürliches Ende (header.spec, beta_resume 20/20 unter Last), Windows-Dateisperre (Scan nach Schließen aller Verbindungen), RSS-Messung nach Ablauf des Messspeichers stabil (Grenze 190 MB unverändert), Reparatur OUT-230 wiederholt bzw. bricht bei Zielfehlern ab, MCP-Sicherung nach gescheitertem Ersetzen entfernt, `lifecycle_crash` 180 s seriell. Spec-Nachträge CRAWL-126, OUT-230, AGENT-043. Nachweis für Windows/macOS: main-CI nach dem Merge.
- Folgepunkte aus B3-FLAKES (nicht beta.3):
  - `n04-live:74` WebKit ist Latenz, kein Pause-Wettlauf (Tab-Klick 1,6 s) → Paket M2-N32.
  - Referenz-Crawl in `lifecycle_crash` 25–67 s statt ≈4 s für 1 500 URLs auf dem Server; Verdacht Crawl-Ende (5xx-Backoff oder Abschluss-Checkpoint). Klären, bevor die 180 s wieder knapp werden.
  - `render_chromium` late_paint: Wartezeit erst ab verarbeitetem `resize` zählen (Produktvorschlag, eigenes Paket); `pixel.png` braucht ein Fehlerprotokoll.
  - SMTP „Connection refused“, `robots_step_m2`, Intel weiter offen.
- **B3-BREW erledigt (gemergt 05.11.2026, Fable 90):** Homebrew-Tap vorbereitet, Veröffentlichung nach Go. Nutzer legt Tap-Repo, Token und Environment `homebrew` an. Folgepunkte: Spec-Nachtrag OUT-256 „Paketmanager“ (inkl. `--zap` entfernt Standard-Crawl-Ordner); `@beta` auslassen, wenn eine stabile Version hinter einer neueren Beta erscheint (nach 1.0); vor 1.0 den Homebrew-Teil aus `packaging/winget/publish.sh` entfernen (P01 schreibt sonst ohne eigenes Go); erste Probe `gh workflow run homebrew.yml --ref main -f version=1.0.0-beta.2` (schreibt nichts).
- **B3-N00 erledigt (gemergt 05.11.2026, Fable 90):** „Gestoppt“ war seit M2-N03 behoben, jetzt mit Tests (Statuskarte, Neustart); Statuskarte zeigt „Fertig (Grenze erreicht)“. Spec-Entscheidungen S1–S4 (Fable-Empfehlung) eingetragen: Gestoppt ist Endgrund, kein Verwaltungszustand; „Gestoppt ist endgültig“ für App, CLI und MCP; Wortlaut „Fertig (Grenze erreicht)“; Definition „gecrawlt“ (CRAWL-130). Das Fable-Gate M2 (M2-Q04) hat nie als eigenes Gate stattgefunden, die A03-Befundliste ist leer.
- **B3-MACIMG erledigt (gemergt 05.11.2026, Fable 86):** Spec-coverage-M2-E2E wie ui-e2e (os-testsite, macOS-Tastaturnavigation, Linux-Sandbox), macOS in zwei Shards, Artefakte auch bei Abbruch.
- **B3-COV erledigt (gemergt 05.11.2026, Fable 88):** Spec-coverage-M2 meldet Testfehler immer rot; Abdeckung läuft wieder (124 Pakete erkannt, unbekannte IDs als Warnung `unknown_merged`); AppArmor im `evidence`-Job (59 Rust-Tests waren still rot); 15-min-Limit für die Browserinstallation. `M2-status.json`: 588/590 umgesetzt, `passed: false` (im Entwicklungsmodus nur Warnung).
- Folgepunkte aus B3-MACIMG/B3-COV (nicht beta.3):
  - Paket „M2-Nachweise“ (nur Tags/Marker/Begründungen, keine Testlogik): Test-Tags AGENT-041/042 (M2-M02), 25 Anhang-B-Einträge ohne `[M2]`, 13 Ignores ohne gültigen Grund. Vorbedingung für das M2-Gate (Q04: `M2-status.json` ohne open).
  - Merge-Erkennung: Titelform „m2: Integration Runde N – merge m2/<id>“ aufnehmen (15 Merges, 13 weitere Pakete); Pakete über Integrationszweige (N06, Q02, N15F, FIXE-PORT) per Alias-Tabelle oder Planfeld `merged_via` – Planerentscheidung.
  - `unknown_merged` im Gate rot machen (Hygiene); `ui-e2e.yml` Browserinstallation ebenfalls mit 15-min-Schritt-Limit; Budget-Schritt nach E2E-Timeout kann das 60-min-Job-Limit reißen.
  - M3-Q01: Basis ist `m2.rs` nach B3-COV inkl. `unknown_merged`; Regressionstest `unknown_merged_ids_warn_and_select_no_package` einschließen.
  - Spec-coverage-M2 Shard-Balance macOS (21 vs. 34 min); flaky `sidebar-chart/live:40` (config.get im IPC-freien Fenster), `compare/bridge:44` (Spaltenkopf > 5 s).
- **M2-N29 entfällt (Nutzerentscheidung 06.11.2026):** Kennzeichnung noch nicht gebauter Funktionen als „Kommt später“ wird nicht umgesetzt (kein Spec-Eintrag, kein Paket in `M2-packages.json`). Der technische Rest (Aufteilung von `Manager::retain_with`) bleibt Aufräumpunkt ohne Termin. M2-Abschluss damit: B4-LIVEDETAIL (M2-N32), B4-M2CLOSE (Nachweise, M2-N34, Merge-Erkennung), danach Fable-Gate M2-Q04.
- Für beta.4 aus B3-N00:
  - Paket „Fortsetzen/MCP“: CLI `--resume` und MCP `resume_crawl` lehnen gestoppte Crawls ab (`os-store` `can_resume`, `os-engine` `lifecycle::prepare`, Werkzeugtexte `os-mcp`), dazu MCP-Werkzeug `pause_crawl(id)` und wiederholbarer Fehlercode `busy` (AGENT-020).
  - Verwaltung zeigt „Abgeschlossen (gestoppt)“ (additives `endReason` im Listing), Grenzart `limit_kind` in „Fertig (Grenze erreicht: …)“.
  - `exclude_self_links`: Entscheidung durch Fable; TECH-146 sieht „Self-Links ausschließen“ standardmäßig an, `viz.rs` nutzt `unwrap_or(false)` (Besitz V02).
  - Statuskarte kennt „Analyse läuft“ und „Gestoppt – Datei wird zusammengeführt“ noch nicht (UI-013-Nachtrag Kurzform/Vollform).
