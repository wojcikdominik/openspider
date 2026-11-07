# B4-LAGFIX: F7, Live-Zähler

Arbeitsbranch `m3/lagfix`, Ausgangsstand `13ab61539` (B4-LDFOLLOW). Untersuchung läuft.
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

Noch in Untersuchung. Kette: Writer → Sitzungs-Feed → View-Pflege → gebündelter Tick →
Event-Bus/Store → `ViewDataSource`-Versionswechsel → Tabellenfuß/Chip.

## Änderung und Test

Noch offen.

## Renderlast vorher/nachher

Noch offen. Messrezept aus `b4-livedetail.md`: Produktionsbuild, 5 000 Seiten × 30 Links,
150 URL/s, vier Crawl-Threads, acht Klickrunden, fünf Sekunden Leerlauf.

## Gates

Noch offen. Tokio laut Lockdatei unverändert 1.53.3. Keine Tests oder Budgets gelockert.

## Fremde Dateien

Keine fremden Änderungen übernommen. Temporäre unabhängige Reproduktion und Instrumentierung
wie oben; werden am Ende entfernt. Weitere nötige Pfade werden hier mit Grund ergänzt.
