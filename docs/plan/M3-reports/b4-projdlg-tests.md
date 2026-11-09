# B4-PROJDLG – unabhängige Abnahmetests

Prüfgegenstand: Branch `m3/b4-projdlg` (53ffd4b3). Grundlage allein die Spec: OUT-228, UI-124, CRAWL-127 (OS-Festlegung 09.11.2026),
UI-Dialogregel (Esc schließt, Enter = Standardaktion, globale Kürzel in modalen Dialogen aus). Der Bauer-Bericht wurde nicht gelesen.
Der Produktcode wurde nur über die vorhandene Harness (`e2e/crawls/harness.tsx`, `fixtures/core.ts`) aufgerufen; für die Selektoren
(Dialog-/Feldnamen, Speicherschlüssel `openspider.recent-crawls.v1`) war ein Blick in `CrawlsHost.tsx`/`model.ts` nötig.

Dateien: `apps/desktop/e2e/acceptance/b4-projdlg.spec.ts` (Playwright, 26 Tests je Browser) und
`apps/desktop/src/features/crawls/acceptance-projdlg.test.tsx` (Vitest, 10 Tests). Bestehende Tests unverändert.
Eingaben in Playwright: `locator.click()`, dann `page.keyboard.type` (ggf. `ControlOrMeta+a`, `Backspace`); kein `fill`, kein
`pressSequentially`, kein `fireEvent.change`. jsdom kennt keine echte Tastatur (kein `user-event` im Projekt): der Vitest-Teil prüft
nur, dass Tastenereignisse im Feld nicht abgebrochen werden und der Fokus im Feld liegt; das eigentliche Tippen deckt Playwright ab.

## Ausführung (Build-Server, Worktree `t-projdlg`, `--workers=1`)

- `npm run -w apps/desktop typecheck`: grün.
- Vitest `acceptance-projdlg.test.tsx`: 9 grün, 1 rot (Produktfehler 1).
- Playwright `b4-projdlg.spec.ts`: Chromium 24 grün / 2 rot, WebKit 24 grün / 2 rot (gleiche Tests rot, Produktfehler 1 und 2).
- Mutationsprobe: gegen den Stand vor dem Paket (`CrawlsHost.tsx`, `crawls.css`, `help/recent.ts`, `contributions/main.ts` von a675fefa)
  sind in Chromium 20 von 26 Playwright-Tests und 9 von 10 Vitest-Tests rot (alle Tipp-, Esc-, Zuletzt-geöffnet-Tests); die Tests
  erkennen den Beta.3-Fehler also. Danach Stand wiederhergestellt.

## Testliste

| Test | Spec-ID | Chromium | WebKit |
|---|---|---|---|
| Suche: Klick, Tippen, filtert über Name/Projekt/Start-URL case-insensitiv, Löschen per Tastatur | OUT-228 | grün | grün |
| Mod+F fokussiert die Suche, danach Tippen ohne Klick | OUT-228, UI-124 | **rot** (Fehler 2) | **rot** (Fehler 2) |
| Projekt anlegen: Übernehmen erst nach Eingabe aktiv (leer: deaktiviert), Umlaute/„&“, Projekt in der Liste | OUT-228 | grün | grün |
| Projekt umbenennen: Tippen ersetzt Namen, Enter übernimmt, Liste und Crawl-Zeilen zeigen neuen Namen | OUT-228 | grün | grün |
| Crawl umbenennen: Übernehmen aktiv, neuer Name in Tabelle, ID unverändert | OUT-228 | grün | grün |
| Crawl verschieben: Tippen im Zielfeld, gesendeter Wert = Eingabe | OUT-228 | grün | grün |
| Leerzeichen/Umlaute, Abbrechen verwirft Eingabe, Wiederöffnen leer, fokus zurück | OUT-228 | grün | grün |
| Esc je Formular (4 Tests): nur Formular zu, Verwaltung offen, Fokus auf Auslöser, nichts übernommen; 2. Esc → Fokus auf „Manage“ | UI-124 | grün | grün |
| Tab/Umschalt+Tab je Formular (4 Tests): Fokus bleibt im Formular, wandert durch mehrere Elemente | UI-124 | grün | grün |
| Globale Kürzel greifen nicht in Formularfelder (Zeichen, Leertaste, Backspace, Entf) | UI-124 | grün | grün |
| Zuletzt geöffnet, Mac- und Windows-Pfad ≥ 150 Zeichen, 640 px: einzeilig, in der Mitte gekürzt, Anfang und Dateiname mit `.ospider` sichtbar (2 Tests) | CRAWL-127 | grün | grün |
| Voller Pfad im Tooltip (`title`) und im zugänglichen Namen; unbekannter Crawl zeigt Dateinamen als Hauptzeile (2 Tests) | CRAWL-127 | grün | grün |
| Kurzer Mac-/Windows-Pfad ungekürzt, nicht abgeschnitten | CRAWL-127 | grün | grün |
| Bekannter Crawl: „Name – Datum Uhrzeit“, Pfad darunter ungekürzt, Tooltip | CRAWL-127 | grün | grün |
| Einträge gleich breit, linksbündig, Reihenfolge | CRAWL-127 | grün | grün |
| Kürzung folgt der Breite (480/640/1280 px), Dateiname bleibt sichtbar | CRAWL-127 | grün | grün |
| Klick auf Eintrag öffnet genau diesen Pfad | CRAWL-127 | grün | grün |
| Echte App-Hülle (`/?transport=mock`): Menü Datei enthält „Zuletzt geöffnet“ genau einmal | CRAWL-127, UI-002 | **rot** (Fehler 1) | **rot** (Fehler 1) |
| Vitest: Tastenereignisse in Suche und 4 Formularen nicht abgebrochen, Fokus im Feld | UI-124, OUT-228 | grün | – |
| Vitest: Verwaltung bekannter/unbekannter Crawl, langer Pfad, Domain-Ersatz bei leerem Namen, Tooltip, zugänglicher Name (4 Tests) | CRAWL-127 | grün | – |
| Vitest: Menü Datei, bekannter Crawl `<Name> – <Datum Uhrzeit>` | CRAWL-127 | grün | – |
| Vitest: Menü Datei, unbekannter langer Pfad ≤ 64 Zeichen, mit „…“, Anfang und Dateiende; ≤ 64 Zeichen unverändert (40/63/64) | CRAWL-127 | grün | – |
| Vitest: `loadRecent` liefert Name/Domain – Datum, Pfad bleibt der volle Pfad | CRAWL-127 | grün | – |
| Vitest: Datei-Menü hat „Zuletzt geöffnet“ nur einmal | CRAWL-127, UI-002 | **rot** (Fehler 1) | – |

## Produktfehler

**1. Menü Datei enthält „Zuletzt geöffnet“ zweimal** (vorbestehend, vor dem Paket vorhanden, nicht Teil des Diffs, aber Ziel (b)).
`installMainMenus()` registriert `file.recent` (Quelle: ui-state), `CrawlsHost` registriert zusätzlich `crawls.recent`
(Quelle: localStorage). Reproduktion: `/?transport=mock` laden, `window.__osMenu.model.menus` → Menü `file`: „New crawl | Open crawl |
Recently opened | Save crawl as… | … | Manage crawls… | Import crawls… | Export crawl… | Recently opened | …“. Im Echtbetrieb zeigen beide
Untermenüs Einträge, ggf. mit unterschiedlichen Beschriftungen (`file.recent` nutzt das Index-Label aus `loadRecent`, `crawls.recent`
`recentMenuLabel`) und je eigener Liste. CRAWL-127 und UI-002 nennen genau einen Eintrag „Zuletzt geöffnet“ (10 Einträge). Tests:
Playwright „echte App-Hülle…“ und Vitest „Untermenü ist genau einmal vorhanden“.

**2. Strg/Cmd+F wirkt in der Crawl-Verwaltung nicht, solange der Fokus auf dem Dialog selbst liegt** (OUT-228 „Suche (Strg/Cmd+F)“;
kleinerer Befund, nicht Teil des Pakets). Reproduktion: Harness `/e2e/crawls/index.html`, „Manage“ klicken, sofort Mod+F: `document.activeElement`
bleibt `section.os-dialog[role=dialog]` (außerhalb von `.os-crawls`, dessen `onKeyDown` das Kürzel behandelt). Nach einem Tab (Fokus im
Suchfeld) ist das Kürzel überflüssig; aus dem Projektbaum funktioniert es (bestehender Test). Test: „Mod+F fokussiert die Suche…“.

Beide Tests bleiben unverändert rot und sind nicht abgeschwächt.

## Spec-Unklarheiten

- „Übernehmen wird aktiv“: Die Spec nennt die Schaltfläche nicht; „aktiv“ bedeutet hier: leer deaktiviert (Anlegen, Umbenennen),
  nach Eingabe aktiviert. „Verschieben“ ist laut Produkt auch leer aktiv (Crawl aus dem Projekt nehmen); die Spec schweigt dazu.
- Esc und Enter in Dialogen: nur die Kürzelregel in UI-Kürzel (Esc schließt, Enter Standardaktion) ist geschrieben; Fokusrückgabe zum
  Auslöser und Tab-Falle ergeben sich aus der Aufgabe und UI-126, nicht aus einer eigenen Dialogregel. Ein Abschnitt „Dialoge
  (Fokus, Tab, Esc, Enter)“ in 05-ui.md würde diese Tests tragen.
- CRAWL-127: „Datum und Uhrzeit des Crawls“ ohne Zeitzone und Format. Tests prüfen nur das Muster `Mon D, YYYY, h:mm` (englische
  Oberfläche). Sprachen, Zeitzone: offen. Kürzung im Menü: „höchstens 64 Zeichen“ ist geprüft (Grenze 64 unverändert, 65+ gekürzt).
- CRAWL-127: Der Tooltip ist als `title`-Attribut an Eintrag oder Elternelement geprüft; die Spec lässt die Technik offen.
- Native Menüs (Tauri) sind in Playwright/Vitest nicht prüfbar; geprüft ist das Menümodell (`label`).
- Nicht abgedeckt: Zusammenspiel mit dem echten Kern (Bridge), WKWebView/WebView2 auf den Zielplattformen, Zeilenumbruch im echten Datei-Menü.

Branch `m3/t-projdlg`, Hash siehe letzte Zeile des Commits.
