# B4-LIVEDETAIL – Messung auf der Referenzhardware (Mac mini)

Gemessen am 08.11.2026 mit `apps/desktop/e2e-b/perf/livedetail-mac.sh` auf `m3/livedetail` 99fd41ea (Release-Build), Nutzer-Freigabe 06.11.2026 für diesen einen Lauf. 5000 Seiten, 150 URL/s, 4 Threads, 10 Runden.

| Messung (UI-145, Klick bis gezeichnete Tabelle) | Median | Max | Budget |
|---|---:|---:|---:|
| Inlinks während des Live-Crawls | 34 ms | 50 ms | 150 ms |
| Outlinks während des Live-Crawls | 34 ms | 52 ms | 150 ms |
| Neue Auswahl (frisch) | 34 ms | 52 ms | 150 ms |
| Beibehaltene Auswahl | 33 ms | 52 ms | 150 ms |

Hauptthread im Leerlauf 0,3 % belegt (größte Lücke 6 ms), während der Messung 1,4 % (größte Lücke 20 ms). Kernmessung ohne Oberfläche (`B4_LIVEDETAIL_CORE`, Release-Test): Öffnen eines Detail-Views im Median 4,6–15,7 ms, Maximum 30 ms bei bis zu 148 273 Links.

Der erste Versuch des nativen Teils brach mit „TimeoutError“ ab, während der Mac noch vom Release-Build ausgelastet war (Last 13); die Wiederholung mit `--measure-only` bei Last unter 2,5 lief vollständig. Hardwaredaten ohne Seriennummern in `livedetail-hardware.txt`.
