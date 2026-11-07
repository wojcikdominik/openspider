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
`apps/desktop/e2e/lagfix-repro/`. Das vermeidet das Überschreiben der bereits versionierten,
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
31/31 grün. Kein bestehender Test geändert; unabhängige Abnahme unverändert.

## Renderlast vorher/nachher

Noch offen. Messrezept aus `b4-livedetail.md`: Produktionsbuild, 5 000 Seiten × 30 Links,
150 URL/s, vier Crawl-Threads, acht Klickrunden, fünf Sekunden Leerlauf.

## Gates

Noch offen. Tokio laut Lockdatei unverändert 1.53.3. Keine Tests oder Budgets gelockert.

## Fremde Dateien

Keine fremden Änderungen übernommen. Temporäre unabhängige Reproduktion und Instrumentierung
wie oben; werden am Ende entfernt. Weitere nötige Pfade werden hier mit Grund ergänzt.
