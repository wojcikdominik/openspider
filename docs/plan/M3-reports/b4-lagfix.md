# B4-LAGFIX: F7, Live-Zähler

Arbeitsbranch `m3/lagfix`, Ausgangsstand `13ab61539` (B4-LDFOLLOW). Produktänderung umgesetzt; Endnachweise laufen.
Soll ausschließlich `docs/spec/*`; keine Referenzprodukt-Sichtung, keine Abhängigkeitsänderung.

## Anforderung und Reproduktion

UI-018 verlangt für aktiven Chip und Tabellenfuß während des Crawls ≤ 1 s.
Die unabhängige F7-Prüfung erlaubt 2 s × Rechner-/Recorderfaktor und ist insoweit großzügiger.
Die gemeinsame Zählung für Overview und Auswahllisten hat ausdrücklich ihren eigenen Takt
(max(2 s, 3 × vorige Zähldauer)); sie darf nicht als Ersatz für die Live-Zähler dienen.
UI-145/PERF-004 verlangen begrenzte Zeilenfenster und geringe Renderlast.

Unveränderte Kopien aus `m3/livedetail-tests-ci`: `livedetail-lag-window.spec.ts`,
`livedetail-load-fixture.ts`, `livedetail-gate.ts`, `livedetail-support.ts`, temporär unter
`apps/desktop/e2e/lagfix-repro/`. Alle vier Dateien sind byteidentisch zu
`2c5c9b5b3` (SHA-256-Nachweis `/tmp/b4-lagfix-repro-hashes.json`). Der Testbranch
wurde während der Arbeit parallel weitergeführt; spätere Änderungen wurden nicht in
die Reproduktion übernommen. Das vermeidet das Überschreiben der bereits versionierten,
abweichenden Support-Datei. Diese Kopien werden nicht committet. Eigene temporäre
Instrumentierung in `diagnostic.spec.ts`, ebenfalls nicht zum Commit vorgesehen.
Playwright: eigener Port 14791, zwei Worker, F7 mit `taskset -c 0,1`.

## Ursache mit Messwerten

Kette: Writer (100-ms-Sammelfenster, bei Rückstau 500 ms) → Sitzungs-Feed (100-ms-Poll,
View-Publikation nach konfiguriertem Intervall) → asynchrone View-Pflege → gebündelter Tick
(Standard 500 ms) → synchroner Event-Bus/Store → `ViewDataSource`-Versionswechsel → Fuß/Chip.
Der Fehler liegt im letzten Übergang: Die Zähler lasen `source.total`; das wird erst nach
erfolgreichem Laden des sichtbaren Zeilenfensters übernommen. Ein bereits angekündigter
Zählerstand blieb somit hinter einer Zeilenantwort hängen. Bei `Stale` bleibt die alte
Zeilenversion bewusst sichtbar; damit blieb fälschlich auch der Zähler stehen.

Unveränderter F7-Baseline-Lauf in Chromium: Rechnerfaktor 1,01; Recorderfaktor 1,033;
Grenze 2066 ms; 523 Lag-Stichproben, davon 397 während Zufluss und 104 im ereignisfreien
Schlussfenster. Acht verletzte Stichproben (Fuß und Chip); unter anderem 89 statt ≥90,
sowie 125 statt ≥127. Die beiden Fixture-Prüfungen und die Inlinks-Prüfung bestanden.

Temporäre Diagnose mit 180 URLs, 3 URL/s, Chromium auf CPU 0/1:
122 eingehende Ticks, 98 begonnene/beendete Versionswechsel, 18 fehlgeschlagene IPC-Antworten.
Erfolgreiche `view.rows`: Median 44,45 ms, Maximum 344,2 ms (84 Antworten).
`catalog.filterCounts`: Median 767,4 ms, Maximum 2293,3 ms (73 Antworten), eigener
hintergrundgebündelter Pfad; nicht die Quelle des aktiven Live-Zählers.
Konkrete Sequenz (Browserzeit in ms):

| Zeitpunkt | Crawl / Ereignis | Fuß |
|---|---|---|
| 46752 | Crawl 128, View 127 | 126, danach 127 |
| 47981 | View 128 / Version 144 angekommen | 127 |
| 48062 | Zeilenanforderung Version 144 verworfen | 127 |
| 48558–48935 | Versionen 146 und 149 ebenfalls überholt/verworfen | 127 |
| 49376 | View 135 / Version 153 angekommen | 127 |
| 49457 | Zeilenfenster erfolgreich übernommen | 135 |

Für URL 128: Crawlstatus bis View-Metadaten 1229 ms, bis DOM 2705 ms;
der bereits vorhandene Zähler wurde weitere 1476 ms durch Zeilenwechsel zurückgehalten.
Die Datenkette vor dem UI hatte in diesem Dev-Lauf ebenfalls messbare Latenz; die Änderung
beseitigt die belegte zusätzliche Wartezeit der Zähler. Kein neuer Timer, kein neues IPC,
keine höhere Ereignisrate. Diagnosedaten `/tmp/b4-lagfix-diag-chromium.json`,
Baseline-Trace und Attachment `/tmp/b4-lagfix-before-results/`, extrahierter F7-Befund
`/tmp/b4-lagfix-baseline-f7.json`.

## Änderung und Test

`LiveViewTotal` liest die neueste Gesamtzahl der zugehörigen View direkt aus dem
Sitzungsstore; eine Ankündigung vor der bereits übernommenen Zeilenversion wird ignoriert.
Fuß, aktiver Chip und aktiver Listeneintrag warten nicht mehr auf `view.rows`. Die Zeilen,
Version, Auswahl und Scrollanker werden weiterhin gemeinsam übernommen. Die gemeinsame
Overview-/Auswahllisten-Zählung behält ihren Takt. Nur die kleinen Zählerbereiche abonnieren
den Zahlenwert, nicht die umgebende Tabelle oder die Canvas.

Neue eigene Fixture-Regression `b4-live-counts.test.tsx`: ausstehende Zeilenantwort,
anschließendes `Stale`, korrekter Fuß/Chip/Listeneintrag, kein zusätzlicher Grid-Render;
außerdem fremde View und ältere Ankündigung. Gegen den unveränderten Ausgangsstand
1/2 rot, gegen den Fix 2/2 grün. Zusammen mit `table.test.tsx` und `n27-live-chip.test.tsx`
31/31 grün (erste Fassung). Ein weiterer Fixture-Test erhält den bekannten „All“-Nenner,
solange die gemeinsame Zählung noch `null` ist; Endfassung 3/3 neue Tests grün.
Kein bestehender Test geändert; unabhängige Abnahme unverändert.

### Zwischenlauf und Störungen

Ein erster Nachher-Versuch überlappte die eigenen Clippy-/Workspace-/Frontend-Gates.
Erste Zählerprüfung: eine verletzte Probe (10 statt ≥11 nach 2592 ms), Inlinks grün.
Der Recorder zeigt zwischen zwei tatsächlichen Takten **3889,4 ms** Abstand: bei +5009 ms
Crawl 11 / Fuß 10, erst bei +8899 ms die nächste Probe, bei +10445 ms Crawl 24 / Fuß 23.
Das ist ein zusätzlicher langer Browser-/Scheduling-Stillstand; die Beobachtung allein
belegt dessen Ursprung nicht. Keine Testgrenze geändert. Serie für die Untersuchung
unterbrochen: 5 bestanden, 1 fehlgeschlagen, 2 unterbrochen, 32 nicht ausgeführt.
Protokoll `/tmp/b4-lagfix-f7.log`, Attachment/Trace `/tmp/b4-lagfix-f7-results/`,
extrahiert `/tmp/b4-lagfix-first-fix-f7.json`. Die vollständige Serie wird ohne eigene
parallel laufende Gates wiederholt; das parallele fremde Paket ist nicht angehalten worden.

Eigene Instrumentierungsprobleme: Die erste Diagnose wurde vor dem erneuten `alpha.open()`
installiert und dadurch verworfen; korrigierter Lauf lieferte das oben ausgewertete Protokoll.
Eine spätere, unbenutzte Diagnose-Erweiterung verletzte TypeScript `unknown` im Catch;
der temporäre Diagnose-Test wurde aus dem Worktree entfernt, Typecheck anschließend grün.
Ein früh gestarteter bestehender Playwright-Lauf wurde für den „All“-Nenner-Randfall
unterbrochen und auf dem endgültigen Produktstand vollständig neu gestartet.

## Renderlast vorher/nachher

Noch offen. Messrezept aus `b4-livedetail.md`: Produktionsbuild, 5 000 Seiten × 30 Links,
150 URL/s, vier Crawl-Threads, acht Klickrunden, fünf Sekunden Leerlauf.

## Gates

| Prüfung | Stand |
|---|---|
| `npm ci` | Exit 0, Lockdatei unverändert |
| Frontend-Typecheck | Exit 0 |
| Frontend-Vitest | **1034/1034**, 122 Dateien, 179,17 s, Exit 0 |
| `cargo fmt --all --check` | Exit 0 |
| Clippy, Workspace/all-targets, `-j 4 --locked`, `-D warnings` | Exit 0 |
| Workspace-Test | **4414 bestanden, 0 fehlgeschlagen, 51 vorhandene Ignorierungen**, 612 Ergebnisgruppen, Exit 0 |
| `scripts/check-neutral.sh` | Exit 0, keine Fundstellen |
| bestehende Playwright-Specs | erster vollständiger Lauf **59/60**, Chromium **30/30**, WebKit **29/30**; Wiederholung folgt |
| F7, fünf Wiederholungen je Engine auf zwei Kernen | folgt |

Einziger Fehler im ersten vollständigen Browser-Lauf: geschützter Test
`livedetail-detail.spec.ts:198`, Selbstkontrolle des Teststimulus an Zeile 227:
acht Tastenereignisse in **936 ms**, verlangt **<800 ms**. Die fachliche Prüfung danach
wird nicht erreicht. Das ist bereits in `b4-ldfollow.md` dokumentiert. UI-145 definiert
100 ms Entprellung für schnelle Auswahlwechsel; die 800-ms-Prüfung stellt sicher,
dass der Test überhaupt schnelle Eingaben liefert. Für diese Vorbedingung wurde weder
das Produkt noch der unabhängige Test geändert. Protokoll `/tmp/b4-lagfix-suite-final.log`,
Trace `/tmp/b4-lagfix-suite-final-results/`. Die komplette Suite wird nach Ende der
eigenen Hintergrund-Gates nochmals ausgeführt.

Workspace-Aufruf nach `~/ws-slot.sh`, mit `CARGO_PROFILE_DEV_DEBUG=0`,
`CARGO_PROFILE_TEST_DEBUG=0`, `CARGO_INCREMENTAL=0`, `-j 4 --locked` und
`OPENSPIDER_CHROMIUM=/home/build/.cache/ms-playwright/chromium-1248/chrome-linux64/chrome`.
Schwere Kommandos mit `timeout 3600s nice -n 10`, Playwright eigener Port 14791 und
`--workers=2`. Ein zurückgebliebenes eigenes Fixture-Paar aus dem bewusst unterbrochenen
Browser-Zwischenlauf wurde beendet, bevor die Kontrollmessungen beginnen.

Tokio laut unveränderter Lockdatei **1.53.3**; keine Abhängigkeit aktualisiert.
Keine bestehenden Tests, Budgets, Ignorierungen oder Wiederholungsregeln geändert.
Logs `/tmp/b4-lagfix-{npm-ci-final,typecheck-final,vitest-final,fmt-final,clippy-final,workspace,neutral-final}.log`.

## Fremde Dateien

Keine fremden Änderungen übernommen. Temporäre unabhängige Reproduktion und Instrumentierung
wie oben; werden am Ende entfernt. Kurzzeitig vorbereitete Diagnose-Marker in
`crates/os-app/src/session/feed.rs` und `crates/os-app/src/views/mod.rs` wurden vor einem
Diagnoselauf byteidentisch zurückgenommen; keine Kernänderung übernommen. Eine separate
Bridge-Kompilation bestätigte den wiederhergestellten Stand. Keine fremden Quellen oder
Tests committet. Mess-Builds und Logs bleiben ausschließlich unter `/tmp/b4-lagfix-*`.
