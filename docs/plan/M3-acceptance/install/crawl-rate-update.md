# Crawl-Rate während eines Update-Downloads (OUT-250, CRAWL-142)

**Stand 09.11.2026: ausstehend.** Messung nur auf dem Mac mini (Referenzhardware, ruhige Maschine nach C11);
Runner-Werte zählen nicht. Werkzeug und Befehl: `README.md` Abschnitt 6.

| Feld | Wert |
|---|---|
| Datum, Uhrzeit | |
| Rechner (`hw.model`, CPU, RAM) | |
| macOS | |
| Commit / CLI-Version | |
| Site | `os-testsite synthetic:n=10000,links=10`, `--max-urls 5000` |
| Download | 70 MB, gedrosselt auf 10 MB/s, HTTPS über Mock-Proxy mit Test-CA |
| Läufe A (URLs/s) | |
| Läufe B (URLs/s) | |
| Median A / Median B | |
| Streuung A (≤ 5 %) | |
| Abweichung B gegen A (≤ 5 %) | |
| Download überdauerte jeden B-Lauf | |
| Ergebnis | bestanden / nicht bestanden / nicht auswertbar |

Grenze der Messung: Der Download läuft in einem eigenen Prozess (gleiches Netz, gleicher TLS-Aufwand,
gleicher Datenträger), nicht im App-Prozess neben der Crawl-Engine. Dass ein Download im App-Prozess den
Crawl-Zustand nicht ändert (kein Neustart, keine Pause), prüft `crates/os-app/tests/update_crawl.rs`.
