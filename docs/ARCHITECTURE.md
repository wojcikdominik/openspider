# OpenSpider – Architektur

Ziel: das beste Crawling-Werkzeug für die Agentic Ära – gebaut für AI Agents und Menschen gleichermaßen. Mindestens Funktionsparität mit den führenden Desktop-Crawlern, dabei schneller, sparsamer im Speicher und vollständig über MCP steuerbar. Plattformen: macOS (arm64, x64) und Windows (x64). Lizenz: Apache-2.0.

**Mindestplattformen** (Entscheidung (Fable-Empfehlung) vom 13.10.2026): macOS 13.0 oder neuer (arm64/x64; WKWebView ab Safari 16.2 für `color-mix(in oklch)`, `forced-colors`, Canvas-DPR), Windows 10 ab 1809 bzw. Windows 11, x64, mit WebView2-Evergreen-Runtime. Festgeschrieben in `apps/desktop/src-tauri/tauri.conf.json` (`bundle.macOS.minimumSystemVersion = "13.0"`, M2-F03) und im M3-Installerplan (OUT-260 … 262).

## Grundsätze

1. **Kern ohne UI.** Die gesamte Crawl- und Analyse-Logik lebt in Rust-Crates und ist über CLI und Tauri gleichermaßen nutzbar. Die UI ist eine reine Ansicht auf den Crawl-Speicher.
2. **Disk-first.** Jeder Crawl ist eine SQLite-Datei (`*.ospider`, WAL-Modus). Kein „Memory vs. Database“-Modus nötig: SQLite + Page-Cache deckt beides ab. Millionen URLs ohne RAM-Explosion.
3. **Baum je Seite, Speicher begrenzt.** HTML wird je Seite mit html5ever (über `scraper`) in einen Baum geparst, weil die HTML5-Tree-Builder-Semantik (z. B. „liegt im `<head>`“, TECH-003) einen Baum verlangt (Planentscheidung K-09). Der Baum lebt nur während der Verarbeitung einer Seite; sein Speicher ist durch `limits.max_page_kb` begrenzt. Ergebnisse fließen sofort in die Crawl-Datei – es gibt nie einen crawlweiten DOM oder Seiteninhalt im RAM.
4. **Deterministisch testbar.** Jede Funktion aus `docs/spec` hat eine lokale Testfixture und Golden-Outputs.

## Crates (Cargo-Workspace)

| Crate | Aufgabe | Wichtige Abhängigkeiten |
|---|---|---|
| `os-model` | Gemeinsame Typen: `CrawlConfig`, Schreib-/Parse-Modell, IDs, `Clock`/`IdSource`, Indexierbarkeitsmodell (`indexability::evaluate`, reine Funktion) | serde, schemars |
| `os-url` | URL-Normalisierung, Umschreibung, Include/Exclude, Segmente | url, fancy-regex, psl |
| `os-fetch` | HTTP-Client, robots.txt, Rate-Limit, Auth, Proxy, Cookies, HTTP/2 | reqwest (rustls), cookie_store; **eigener RFC-9309-Parser** für robots.txt, **eigener Token-Bucket** für das Rate-Limit (gilt für alle Abrufe) |
| `os-parse` | HTML-Extraktion: Links, Meta, Headings, Canonicals, hreflang, Bilder, Directives, Sitemaps, PDF-Metadaten; ab M2 strukturierte Daten, Custom Search/Extraction | scraper/html5ever (**Baum je Seite**, K-09), encoding_rs, quick-xml, lopdf, serde_json |
| `os-render` | JS-Rendering über Chromium (CDP), Screenshots, gerendertes DOM | chromiumoxide |
| `os-store` | Crawl-Datei (SQLite), Schema-Migrationen, Abfragen für UI-Tabellen (Paging, Filter, Sortierung), Änderungsfeed (`urls.change_seq`/`crawl_meta.change_seq`, vom Writer je Batch gestempelt), Text-Kollation `OS_TEXT` (Unicode-Root, sprachunabhängig – eine Sortierregel für UI, Export und MCP) | rusqlite (bundled, collation), icu_collator |
| `os-engine` | Frontier, Scheduler, Worker-Pool, Pause/Resume, Modi (Spider/Liste/SERP/Vergleich); **ohne Abhängigkeit auf `os-analyze`** – Indexierbarkeit aus `os-model`, die Crawl-Analyse am Ende löst der Aufrufer über einen Hook aus | tokio |
| `os-analyze` | Filterlogik je Tab (SQL-Plan `views::ViewPlan`, Grundlage auch für die View-Caches der UI ab M2), Issues inkl. Erklärungstexten (what/why/fix, eine Quelle für UI, Export, MCP), Nachlauf-Analyse (Link Score, Duplikate per SimHash/MinHash, Orphans, Ketten, hreflang-Rücklinks, Sitemaps) | ab M2: roaring (Bitmaps und Sortierpermutationen der View-Sitzungen, serialisiert in den View-Cache der Crawl-Datei) |
| `os-export` | CSV, XLSX, Bulk-Exporte, Reports, Sitemap-Generator | csv, rust_xlsxwriter |
| `os-ai` | KI-Anbieter (Claude, OpenAI Responses + Decisions API, OpenRouter, Ollama), Embeddings, Decision-Engine (predicate/choice/score), Batch, Cache, Kostenschätzung/Budget, „Frag den Crawl“ (read-only SQL) | reqwest, keyring |
| `os-integrations` | GSC, GA4, PSI, Majestic/Ahrefs/Moz, DataForSEO (Labs, Backlinks, SERP, LLM Mentions), OAuth | reqwest, oauth2, keyring |
| `os-mcp` | MCP-Server (STDIO + Streamable HTTP auf Loopback mit Token), Werkzeugklassen read/crawl/write/cost, Prompts; Skill + Claude-Code-Plugin unter `integrations/` | rmcp |
| `os-app` (ab M2) | Tauri-freie Service-Schicht der Desktop-App (Entscheidung (Fable-Empfehlung) vom 13.10.2026): Sitzungszustand, View-Register und View-Caches, Ereignis-Coalescer, DTOs (ts-rs), Operationen mit Abbruch; transportunabhängiger Command-Dispatcher (Name + JSON) für Tauri-Commands, E2E-Bridge und In-App-HTTP-MCP; Lese-Pool für alle Datenbankzugriffe der App. **Keine Tauri-Abhängigkeit**; hängt nur an `os-model`, `os-store`, `os-analyze`, `os-engine`, `os-export`, `os-mcp`, `os-integrations` (API-Commands) und `os-render` (Login-Fenster) sowie – falls bestätigt – `os-sched` (Planentscheidungen K2-16, K2-07 in `docs/plan/M2.md`) – `os-mcp` und `os-cli` hängen **nie** an `os-app` | ts-rs, roaring, tokio |
| `os-sched` (ab M2, vorbehaltlich Klärung K2-07) | Aufgabenplanung für App und CLI: Aufgaben, Laufhistorie, OS-Planer-Backends (launchd, Windows-Aufgabenplanung) mit Dry-Run, E-Mail-Benachrichtigung; Rückfall ohne eigenes Crate: `os-store::tasks` | os-model, os-store, lettre |
| `os-cli` | `openspider` Binary: headless Crawl, Export, `openspider mcp`, Scheduling-Ziel | clap |
| `os-testsite` | Lokaler Fixture-Server (axum) mit Szenarien aus `tests/fixtures`; synthetische Sites ohne Dateien (`synthetic:n=1000000,links=30` oder `[synthetic]` in `scenario.toml`), optionales Request-Log | axum |
| `os-parity` | Paritätsabgleich gegen Referenz-Exporte (`diff`, `run`) und Spec-Abdeckung aus Tags und Testergebnissen (`coverage`) | csv, toml, serde |
| `os-loadtest` | Lasttest-Runner (Binary, PERF-009): `os-loadtest run --scenario perf-001 --size small` bzw. `--size large`, schreibt `target/loadtest/report.json`; Trait `CrawlerUnderTest`, den `os-engine` später implementiert | os-testsite, clap |

## Crawl-Datei und Speicher

Schreibpfad großer Crawls (M1-P32, K-74 bis K-77; Messwerte in `docs/plan/M1-acceptance/loadtest/probe-p32/README.md`). Ziel: Der Durchsatz bleibt über den ganzen Crawl konstant (PERF-002) und kein Schreibaufruf des Writers blockiert länger als einige Sekunden (PERF-003).

**Schema v2.** `url_counters` (eine Zeile je URL, `WITHOUT ROWID`, rund 30 Byte) ist die einzige Quelle aller Link-Zähler: `inlinks`, `unique_inlinks`, `self_inlinks`, `inlinks_all`, `img_inlinks`, `outlinks`, `unique_outlinks`, `self_outlinks`, `external_outlinks`, `unique_external_outlinks` und die zwei Zähler, die Live-Abfragen ohne Linkindex exakt machen: `html_2xx_sources` (verschiedene interne HTML-Quellseiten mit Status 2xx und mindestens einem Hyperlink auf die URL, Grundlage von `% of Total`) und `image_link_inlinks` (Link-Instanzen vom Typ `Image` mit interner Quelle, Images-Grundmenge). Die Zählerspalten in `urls` bleiben im Schema, werden aber nicht mehr gepflegt und nicht gelesen. Ein Trigger legt die Zeile mit der `urls`-Zeile an und löscht sie mit ihr; während eines Schreiblaufs fehlt der Einfüge-Trigger (er zwingt SQLite je eingefügter `urls`-Zeile zu einem Statement-Journal, 50 µs statt 7 µs), der Writer legt die Zeile selbst an, und Pause, Ende und Schließen der Senke stellen den Trigger wieder her. Der Writer schreibt die Differenzen je Batch aufsteigend nach Kennung in dieselbe Transaktion und stempelt `url_counters.change_seq`; `queries::changed_since` vereinigt `urls.change_seq` und `url_counters.change_seq`. Grund: Die Zählerpflege ändert je Batch rund 60 000 zufällige Zeilen; in der breiten `urls`-Zeile passt die Tabelle bei 1 Mio. URLs nicht in den Cache, die schmale Zeile bleibt darin.

**Index `links_target`.** Er fehlt während eines laufenden oder pausierten Crawls und nach einem Abbruch. Der Writer (`SqliteSink::open`) entfernt ihn beim Öffnen; beim regulären Ende (Zustand `Completed` im letzten Batch: Ende, Stopp, Limit) baut er ihn in der Transaktion dieses Batches sortiert auf (`os_store::link_index`, Ereignis `links_index_built`, Messzähler `link_index_build_ns`); der Aufbau läuft mit 4 MiB Seitencache, weil der Sortierer sonst bis zur Cachegröße (256 MiB) zusätzlichen Speicher belegt, und mit zwei Hilfsthreads des Sortierers (`PRAGMA threads = 2`, N22; 1 Mio. Seiten x 30 Links: 22 statt 33 s; Kaltprobe N27 mit verdrängtem Dateicache 20 bis 22 s, der Aufbau liest die Tabelle sequenziell; vier Threads sparen im Crawl nur 1,5 s, kosten aber bis zu 130 MB Spitzen-RSS bei `perf-003` und sind verworfen, ebenso größere Sortierpuffer; Mikrobenchmark `crates/os-store/tests/bench_links_build.rs`). `link_index::ensure_if_idle` baut ihn bei Bedarf nach; die Crawl-Analyse (`os-analyze`) und die Exporte (`os-export`) einer nicht laufenden Datei rufen es auf. Ohne Index bleibt jede Abfrage korrekt, kostet aber höchstens einen sequenziellen Lauf über `links` (nie einen je Ergebniszeile; Inlinks einer URL bei 250 000 Seiten x 30 Links: 0,24 s ohne Index gegen 0,3 ms mit Index, hochgerechnet auf 1 Mio. Seiten rund 0,9 s):

| Abfrage (Stelle) | mit Index | ohne Index (laufender, pausierter, abgebrochener Crawl) |
|---|---|---|
| Zähler je URL, `Inlinks` & Co. (`os-analyze` Tabs, `os-mcp`, `os-export`) | `url_counters` | `url_counters` (unabhängig vom Index) |
| `% of Total` (Internal) und Images-Grundmenge | `url_counters.html_2xx_sources` / `image_link_inlinks` | dasselbe |
| Inlinks einer URL (`queries::inlinks`, MCP `url_details`, Detail-Export) | Suche im Index, Sortierung nur der Treffer | ein Lauf über `links`: Treffer, Sortierung und Gesamtzahl in einer Abfrage |
| Bulk-Export „All Inlinks“, Response-Codes-Inlinks (`links_export`) | Suche im Index je Zielzeile | ein Lauf über `links`, Zielmenge als Hashtabelle |
| Redirect-Reports (`reports/redirects.rs`) | Suche je Start-URL | Menge der verlinkten Ziele und Beispielquellen je ein Lauf für alle Start-URLs |
| Zähler neu berechnen (`counters::recompute_many`, `verify_all`) | Suche im Index | ein Lauf je Zählergruppe für die ganze Menge |
| URLs entfernen (`os-engine` `rescan::remove_urls`) | Suche im Index | je Abfrage ein Lauf für alle entfernten URLs |
| Fortsetzen (`os-engine` `resume.rs`, Ressourcen-Links der Warteschlange) | kein Index nötig: gehört zu M1-P33 | |

**Indizes von `urls` mit zufälligen Schlüsseln (M1-N22, K-80; M1-N27, K-83).** Ursache des Durchsatzverfalls im 1-Mio.-Crawl (PERF-002; Belege: `docs/plan/M1-acceptance/loadtest/probe-n22/README.md`): `urls` trägt 13 Sekundärindizes, fünf davon über Textspalten, deren Werte mit der Einfügereihenfolge nichts zu tun haben (`urls_title1`, `urls_md1`, `urls_h1`, `urls_h2`, `urls_hash`). Jeder Batch aus 2 000 Seiten ändert in jedem dieser Indizes 2 000 verschiedene Blätter; jedes davon wandert ins WAL und beim Checkpoint einzeln in die Hauptdatei. Die Seiten je Batch (`cache_write`) fielen ohne sie von 14 000 auf 9 000, der Commit blieb flach statt von 95 auf 196 ms je Batch zu wachsen, und die Rückführung des WAL, die zuvor mit der Dateigröße wuchs, folgte ihm (sie war ein Folgeeffekt, keine eigene Ursache). Lebenszyklus (K-80, geändert durch K-83): Ab `url_index::DROP_FROM_URLS` (250 000 URLs; kleinere Crawls behalten alle Indizes, sie passen mit allen Blättern in den Seitencache und ihre Pflege kostet dort nichts) entfernt der Writer die fünf Indizes vor dem Batch, der die Schwelle erreicht (beim Fortsetzen einer großen Datei mit dem ersten Schreiben). **Sie entstehen nicht wieder** (M1-N27, K-83): weder beim regulären Ende (`Completed` im letzten Batch) noch über `link_index::ensure_if_idle`, und das Vorwärmen des Dateicaches (`url_index::Prefetch`) läuft nur noch, wenn in einer kleinen Datei ausnahmsweise etwas aufzubauen ist. Grund: Kein Leser der Inventur unten braucht sie, ihr Aufbau kostete im Endbatch bei 1 Mio. URLs 50 bis 58 s (davon Vorwärmen 10 bis 15 s; mit kaltem Dateicache bis 82 s im Endbatch), und der Endbatch soll nach K-80/K-84 bei 1 Mio. höchstens 45 s dauern. Der Endbatch besteht seitdem aus den Datenanweisungen des letzten Batches (unter 1 s), dem Aufbau von `links_target` (rund 25 s) und dem `COMMIT` mit dem WAL-Checkpoint (3 bis 5 s); Messzähler `end_data_ns`, `link_index_build_ns`, `end_commit_ns` (`os_store::sink::diag`) und `w_end_batch_ns` (`os_engine::diag`, der Schreibaufruf mit `Completed`), im Lasttest `crawler_end_batch_s` und `crawler_tail_s`; ab 1 Mio. Seiten wertet `os-loadtest` K-80 selbst aus (Eintrag „K-80: Endbatch … s > 60 s“ in `crawler_violations`). Dateien ab der Schwelle (größte `urls.id` mindestens `DROP_FROM_URLS`, `url_index::is_large`) bekommen die fünf Indizes also nie zurück; `url_index::ensure` bleibt öffentlich: Ein künftiger Leser, der eine Suche über `title_1`, `meta_desc_1`, `h1_1`, `h2_1` oder `hash_md5` braucht (M2), ruft es selbst auf und zahlt den Aufbau (rund 10 s je Index bei 1 Mio. URLs) dann dort. Diagnoseschalter `OS_STORE_DROP_URL_INDEXES` (`1` = immer entfernen, `0` = nie). Nicht betroffen sind `UNIQUE(url)` (Dublettenschutz des Writers), `urls_change_seq` (Änderungsfeed), `urls_status`, `urls_class`, `urls_state`, `urls_depth`, `urls_index` (wenige Werte, die Änderungen liegen nahe beieinander) sowie `urls_redirect` und `urls_canon` (fast nur NULL). Inventur der Leser auf einer Datei **über** der Schwelle (jede Zeile: ohne die fünf Indizes belegt durch `EXPLAIN QUERY PLAN` aller Filter aller Tabs, `crates/os-analyze/tests/k83_duplicate_plans.rs`; kein Plan benutzt einen der fünf Indizes, auch wenn sie in der Datei stehen):

| Leser (Stelle) | braucht | ohne die fünf Indizes |
|---|---|---|
| Engine, Warteschlange nachladen (`queue_disk.rs`, `queue JOIN urls`), Fortsetzen (`resume.rs`: `urls ORDER BY id`, `list_input JOIN urls`) | Primärschlüssel, `queue_order` | unverändert |
| Engine, Neuabruf/Entfernen (`rescan.rs`: `redirect_target_id`/`canonical_target_id IN (…)`) | `urls_redirect`, `urls_canon` | unverändert (bleiben stehen) |
| `queries::changed_since` (Änderungsfeed, `os-app`/UI), `queries::url_by_address` | `urls_change_seq`, `UNIQUE(url)` | unverändert |
| Tabellen-Filter nach Status, Klasse, Zustand, Tiefe, Indexierbarkeit (`os-analyze`, `os-mcp`, `os-export`) | `urls_status` … `urls_index` | unverändert |
| Filter „Duplicate“ der Tabs Page Titles, Meta Description, H1, H2 (`os-analyze`, `tabs/duplicates.rs`: unkorrelierte Gruppierung `GROUP BY … COLLATE OS_TEXT HAVING COUNT(*) > 1` seit bf521c1; die BINARY-Indizes hälfen ihr nicht) und „Exact Duplicates“ (Content-Tab, ONPAGE-143, `tabs/content.rs`: seit M1-N27 ebenfalls `hash_md5 IN (SELECT … GROUP BY hash_md5 HAVING COUNT(*) > 1)`, früher korreliertes `EXISTS` auf `urls_hash`) | keinen der fünf Indizes | **ein Lauf über `urls`** mit Sortierer je Filter, O(n log n), live auch in einer laufenden Datei; Treffermenge unverändert (auch für Seiten außerhalb der Vergleichsmenge: dann genügt ein Treffer in ihr, sonst braucht es zwei). Benchmark `crates/os-analyze/tests/bench_duplicates.rs` (1 Mio. Zeilen ohne Indizes, Zeiten je Filter im Bericht `probe-n27`) |
| Exporte und Detailansicht (`os-export`) einer nicht laufenden Datei | `links_target`, keinen der fünf Indizes | `ensure_if_idle` baut `links_target` vorher, die fünf Indizes nur in einer kleinen Datei |

**WAL.** Standard ist der Checkpoint im `COMMIT` (`wal_autocheckpoint`, Schwelle 256 MiB, wächst mit der Datei bis 512 MiB; ein von außen gesetzter Wert bleibt unangetastet); Abschluss und Pause führen den kurzen `TRUNCATE`-Checkpoint aus. Nach K-74/K-75 kostet er bei 1 Mio. Seiten höchstens 1 s (größter Commit 0,79 s). Der Hintergrund-Checkpoint (K-76: `SqliteSink::with_background_checkpoint` oder `OS_STORE_BACKGROUND_CHECKPOINT=1`; Thread mit eigener Verbindung, `PRAGMA wal_checkpoint(PASSIVE)` nach WAL-Größe oder fünf Sekunden, `journal_size_limit`, Messzähler `bg_checkpoint_*`, `wait_checkpoint_*`) ist **nicht** der Standard: SQLite setzt das WAL nur zurück, wenn der nächste Schreibvorgang nach dem Ende eines Durchlaufs beginnt, der Writer beginnt aber unmittelbar nach dem Commit. Das WAL wächst dann auf 1 GiB, der Store-Benchmark mit 1 Mio. Seiten wird um 27 % langsamer (4 170 gegen 5 740 Seiten/s, größter Commit 2,8 gegen 1,0 s). N22: Mit den Zufallsindizes wuchs die Rückführung im Commit mit der Dateigröße (500 000 Seiten, Messmodus `OS_STORE_WAL_EXPLICIT=1`, der den `PASSIVE`-Checkpoint nach dem Commit getrennt zählt: 39, 108, 121 ms je Batch in den drei Eimern, 26 Rückführungen mit 133 000 Frames); ohne sie bleibt sie flach (30, 29, 40 ms) und der Commit auch. Die Schwelle (`OS_STORE_WAL_BOUNDS=min:max`, 512 MiB bis 1 GiB) brachte +5 %, der Hintergrund-Checkpoint -3 % bei 2,2 GiB WAL: beides bleibt, wie es war.

**Seitengröße und mmap.** Unverändert (4 KiB, kein `mmap`). A/B nach K-77 (Store-Benchmark und Kurzprobe mit 250 000 Seiten): 16 KiB bringt +8 % bzw. +18 % Durchsatz, aber 65 MB mehr Spitzen-RSS; `mmap_size` 1 GiB liegt im Rauschen (+3 % / +8 %) bei 15 MB mehr. Übernommen wird nur ein Gewinn von mindestens 10 % ohne höheren Spitzen-RSS (Bericht: `docs/plan/M1-acceptance/loadtest/probe-p32/README.md`).

**Diagnosezähler des Schreibpfads (M1-N22).** `os_store::sink::diag` zählt je 100 Batches (16 Eimer, der letzte nimmt den Rest) Aufbau- und Commitdauer (Summe, Maximum), die Aufbauzeit je Anweisungsgruppe (`urls` einfügen, `urls` aktualisieren, `links`, `url_counters`, `queue`, Rest), die Länge der `-wal`-Datei und die Seiten, die der Cache nachlud oder ins WAL schrieb (`sqlite3_db_status`): `commit_ns_h00` … `h15`, `g_links_ns_h00` … (Tabelle im Kopf von `sink.rs`). `os_engine::diag` zählt je Nachladevorgang der Warteschlange Dauer, Zeilen und Wartezeit auf `writer.flush()` (Summe, Maximum, Verlauf je 10 Nachladevorgänge `q_refill_*_r00` … `r15`) sowie die Zeit, in der ein Pop wegen fehlendem Block leer ausging (`q_starved_*`). `os-loadtest` gibt beide ohne Änderung im `diag`-Block aus; `docs/plan/M1-acceptance/loadtest/probe-n22/summarize.py` liest sie. Diagnoseschalter (Standard aus): `OS_STORE_WAL_EXPLICIT=1`, `OS_STORE_WAL_BOUNDS=min:max`, `OS_STORE_BACKGROUND_CHECKPOINT=1` (K-76), `OS_ENGINE_REFILL_NOFLUSH=1` (das Nachladen wartet nicht auf `writer.flush()`; die Messung zeigt keinen Unterschied: 0 Starvation, Wartezeit 0,3 bis 0,5 s je Nachladevorgang laufen im Hintergrund).

**Near-Duplicates (M2-N16, PERF-007/ONPAGE-144).** Bis einschließlich
1.048.576 gespeicherten Signaturen und 64 MiB HTML-Hashtext nutzt die Analyse
kompakte Page-Records und höchstens `min(8, available_parallelism())` LSH-Worker.
`OPENSPIDER_NEAR_DUP_THREADS` ist ausschließlich ein Test-/Mess-Override nach
unten; `RAYON_NUM_THREADS` hat keinen Einfluss. Überschreitet bereits die Anzahl
aller `minhash`-Zeilen die Grenze (auch vor dem HTML-/Indexierbarkeitsfilter),
oder überschreiten die geladenen Hashtexte 64 MiB, wird auf SQLite-TEMP-Banding
zurückgefallen. Bei 90 % Schwelle entstehen dort 16 Bandzeilen je Seite samt
Lookup-Index: allein Bandbytes plus 32-Byte-Hash ergeben 1.024 Byte je Seite,
noch ohne IDs, SQLite-Records, B-Baum-Seiten und die Kopie im Lookup-Index.
Daher mehrere GB freien Platz im SQLite-Tempbereich für Millionenbestände
vorsehen; hinzu kommen temporäre Ergebnispaare und die Publikation im Crawl-WAL.
Auch der schnelle Pfad hält Ergebnispaare in SQLite TEMP. Der tatsächliche Bedarf
hängt von Schwelle, Hashlängen und Paarzahl ab und wurde nicht als Plattenbudget
vermessen. Beide Pfade publizieren atomar mit `ORDER BY url_id, other_id`.
Der Disk-Fallback erhält die Ergebnisse, garantiert aber weder <600 s noch
<2 GB für beliebige Bestände oberhalb der Cachegrenze: Kandidaten je Seite und
Ergebnispaare können stark wachsen, letztere bis quadratisch. Backlog-Vorschlag:
blockweises externes Sortieren der Bänder mit begrenzten Merge-Puffern und
blockweiser Kandidatenprüfung; dabei die exakte Paarmenge und f64-Bits weiterhin
gegen das N14-Orakel prüfen. Siehe `docs/plan/M2-reports/n16.md`.

## Desktop-App

`apps/desktop`: Tauri v2 + React + TypeScript + Vite; Konzept und Entscheidungen: `docs/plan/M2-ui-concept.md`.
- Schichten: React-Oberfläche → typisierte IPC-Wrapper → dünne Tauri-Command-Hüllen (`src-tauri`, je Domäne ein internes Plugin) → `os-app` → Kern-Crates. `openspider-desktop` ist der einzige Ort mit Tauri-Abhängigkeiten.
- Tabellen: Glide Data Grid (Canvas, MIT; `@glideapps/glide-data-grid@6.0.4-alpha24` exakt gepinnt, hinter eigenem Adapter; Rückfall: Fork/Vendoring → Fenster-Rebasing → eigenes Canvas-Grid) mit serverseitigem Paging (Offset auf versionierten Views) über `os-app`.
- Datenbankzugriffe der App ausschließlich über den Lese-Pool bzw. `spawn_blocking` in `os-app`, nie auf dem UI-Thread oder einem Tokio-Worker.
- Fortschritt/Ereignisse: ein Ereigniskanal je Fenster, im Kern gebündelt (≤ 10 Nachrichten/s über alle Ereignisarten, ≤ 1 Hz bei verdecktem/minimiertem Fenster); Quelle ist der Änderungsfeed der Crawl-Datei, damit auch Crawls fremder Prozesse live erscheinen.
- UI-Bausteine: React Aria Components, Zustand, TanStack Query, Fluent (i18n), Lucide, CodeMirror 6.
- Visualisierungen: D3 (Bäume über d3-hierarchy + Canvas, Wortwolken über d3-cloud), sigma.js/graphology (kraftgerichtete Graphen, WebGL), three.js (3D).

## KI & externe Daten (nach Release 1.0, M4/M5)

- Alle KI-Aufrufe laufen über `os-ai`; Fachlogik kennt nur „Frage“ bzw. „Aufgabe“, nie einen konkreten Anbieter.
- Decision-Engine: Fragen einmal anbieterneutral definiert; OpenAI Decisions API bevorzugt, sonst strukturierte Ausgabe über Claude/OpenAI/OpenRouter (Wahrscheinlichkeit dann als Selbsteinschätzung gekennzeichnet).
- Ergebnis-Cache je (Inhalts-Hash, Fragen-Version, Modell) in der Crawl-Datei; Kostenschätzung vor jedem Lauf, hartes Budget je Crawl.
- API-Schlüssel ausschließlich im OS-Schlüsselbund. In CI nur Mock-Server (`os-testsite`), keine echten API-Aufrufe.
- Spezifikation: `docs/spec/08-ai.md`, `docs/spec/09-dataforseo.md`.

## Meilensteine

**Version 1.0 (stabil)** = M1 + M2 + M3. KI-API-Funktionen und DataForSEO kommen erst danach als eigene Feature-Releases.

| M | Inhalt | Abnahme durch Fable |
|---|---|---|
| M1 | Crawl-Kern, Speicher, CLI, Basis-Tabs (P0 aus 01/02/03/06), MCP headless lesend (10: AGENT P0), Lasttests klein | Paritätstests gegen Free-Referenzexporte, PERF-001/003/005 |
| M2 | Desktop-UI, alle P0/P1-Tabs und Filter, Reports/Exporte, JS-Rendering, Google-APIs (GSC, GA4, PSI) und Linkmetriken (04 ohne KI), MCP-Menü, Skill + Plugin | volle Paritätsmatrix P0+P1 (ohne KI), PERF komplett |
| M3 | Signierte Installer (dmg/exe/msi), Auto-Update, Homebrew/winget → **Release 1.0** | Installationstest macOS arm64/x64, Windows |
| M4 | KI-Funktionen: 08-ai, KI-Teile aus 04 (Anbieter, Prompts, Embeddings, semantische Ähnlichkeit), AGENT-030 → Release 1.x | Mock-Tests + Stichprobe mit echten Schlüsseln |
| M5 | DataForSEO: 09-dataforseo → Release 1.y | Mock-Tests + Stichprobe mit echtem Konto |

## Datenfluss

```
Config → os-engine ─┬→ os-fetch ──→ os-parse ──→ os-store (urls, links, resources, issues…)
                    └→ os-render (optional)            │
                                                       ├→ os-analyze (live + Nachlauf)
                                                       ├→ os-export / os-cli
                                                       └→ os-app → Tauri-Commands → UI
```

Abhängigkeitsrichtung: `os-engine` kennt `os-analyze` nicht; `os-app` (ab M2) sitzt über allen Kern-Crates, und kein Kern-Crate (auch nicht `os-mcp`) hängt an `os-app` – Sitzungszugriff für In-App-MCP-Werkzeuge kommt über ein optionales Trait-Objekt im `os_mcp::ToolContext`, das `os-app` bereitstellt. Nach Crawl-Ende ruft die Engine nur einen Hook (`EngineDeps.on_finished`); CLI bzw. App starten darüber die Crawl-Analyse (`os_analyze::analysis::run_after_crawl`). Verbindliche Crate-Abhängigkeiten für M1: `docs/plan/M1.md` Abschnitt 3.2, ab M2: `docs/plan/M2.md` Abschnitt 3.2.

**`os-ai` (ab M4, Nachtrag 21.11.2026, `docs/plan/M4.md` E10):** `os-ai` hängt an `os-model`, `os-store` und `os-fetch` (gemeinsame Geheimnisablage `SecretStoreMut` und Host-Überschreibung `HostResolver` für die Testhosts, CRAWL-123). An `os-ai` hängen nur `os-integrations`, `os-app`, `os-cli` und `os-mcp`; `os-analyze` nutzt KI-Ergebnisse nur über Traits und hat keine Manifest-Abhängigkeit auf `os-ai`; `os-engine`, `os-fetch`, `os-parse`, `os-render`, `os-store` und `os-export` hängen nicht an `os-ai`. Bibliotheken für Embedding-Analysen (ANN-Index, Clustering, Dimensionsreduktion) gehören zu `os-analyze::semantic`, DOCX/Markdown für den Maßnahmenplan zu `os-export`.

## Qualitätssicherung

- `cargo test --workspace`, `cargo clippy -- -D warnings`, `cargo fmt --check`, `cargo deny check licenses bans sources` (Lizenzpolitik in `deny.toml`: nur permissive Lizenzen und MPL-2.0, kein GPL/LGPL/AGPL).
- Paritätstests: Fixtures in `tests/fixtures/<szenario>/` + Golden-Exporte in `tests/golden/`. Referenzwerte aus der Free-Version des Referenzprodukts werden manuell exportiert und abgelegt.
- CI: GitHub Actions Matrix `macos-14` (arm64), `macos-13` (x64), `windows-latest`. Zusätzlich die Jobs `rustfmt` und `cargo-deny` (ubuntu).
- Desktop-UI (ab M2): vitest (Logik, Komponenten), Playwright gegen Frontend + Kern-Bridge in WebKit und Chromium (Hauptlast der E2E-Tests), WebDriver gegen die echte App auf beiden Plattformen (Windows `tauri-driver`, macOS WebdriverIO-Service mit eingebettetem Server `tauri-plugin-wdio-webdriver`), Bedienungshilfen-Smoke nur für native macOS-Menüs; Stufenmodell in `docs/plan/M2-ui-concept.md` 7.3.
- Fortschritt: `docs/spec/PARITY.md` – Status je Funktions-ID (offen / in Arbeit / implementiert / verifiziert).
