# B4-ENTER – unabhängige Abnahmetests

Prüfling: Branch `m3/b4-enter` (30f9b70b), Kopfzeile der Desktop-App, Start-URL-Feld. Testbranch: `m3/t-enter`.
Grundlage ist allein die Spec (UI-003, UI-113 samt OS-Festlegung vom 09.11.2026). Der Bauer-Bericht wurde nicht gelesen.
Die Produktdateien wurden nur gelesen, um Bedienelemente und Teststellen zu finden (Beschriftungen, Test-IDs).

## Dateien

- `apps/desktop/e2e/acceptance/b4-enter.spec.ts` (Playwright, 14 Tests, echte Crawls über die Bridge `os-app-bridge`)
- `apps/desktop/src/features/header/acceptance-enter.test.tsx` (Vitest, 15 Tests)

Bestehende Tests sind unverändert. Eingaben sind echte Tastatur (`page.keyboard`), kein `fill`, kein `requestSubmit`.
Einzige Ausnahme sind synthetische IME-Ereignisse (Test 13), mit zurückgelesenen Ereignisfeldern gegen Leerlauf.
Die Crawls laufen gegen einen eigenen Loopback-Server der Spec-Datei (`/fast/<n>`: fertig in Millisekunden,
`/slow/<n>`: nie endend, zum Pausieren). Jeder Test hat eine frische Bridge (`freshState`); Mock-Transport wird abgelehnt.

## Testliste Playwright

Ergebnis je Engine, jeweils headless, `--workers=1`, Stand 3c5355b6 (Spalte „Basis“ = gleiche Tests gegen `bc8e2f65`, Mutationsprobe).

| # | Test | Spec | Chromium | WebKit | Basis Chromium | Basis WebKit |
|---|------|------|----------|--------|----------------|--------------|
| 1 | Drei Starts nacheinander per Enter, Rückfrage ab dem zweiten, Vorschlagsliste neueste zuerst, je Enter genau ein Crawl | UI-003, UI-113 | grün | grün | grün | rot |
| 2 | Wiederholter Start derselben URL, keine Doppel in der Vorschlagsliste | UI-003, UI-113 | grün | grün | grün | rot |
| 3 | Enter in der Rückfrage = Standardaktion | UI-113 | rot | rot | rot | rot |
| 4 | Escape und Abbrechen in der Rückfrage: nichts startet, Ergebnisse und Eingabe bleiben, Rückfrage erneut möglich | UI-003, UI-113 | grün | grün | grün | rot |
| 5 | Enter während Crawl läuft startet nichts | UI-003 | grün | grün | grün | grün |
| 6 | Pausiert, URL unverändert: Enter setzt denselben Crawl fort (ohne Rückfrage, kein neuer Crawl, Start-URL nicht erneut abgerufen); andere Schreibweise derselben Adresse gilt als unverändert | UI-113, UI-003 | grün | grün | rot | rot |
| 7 | Pausiert: Mod+Enter außerhalb eines Textfelds setzt fort | UI-113 | grün | grün | rot | rot |
| 8 | Pausiert, leeres Feld gilt als unverändert: Enter setzt fort | UI-113 | grün | grün | rot | rot |
| 9 | Pausiert, geänderte URL: Enter und Mod+Enter fragen, nichts wird fortgesetzt; Escape behält die Pause; Ersetzen startet neuen Crawl | UI-113, UI-003 | grün | grün | rot | rot |
| 10 | Pfeiltasten in den Vorschlägen, dann Enter übernimmt den Eintrag und startet nicht (↓ und ↑); nächstes Enter startet | UI-003, UI-113 | grün (1) | grün (1) | grün | rot |
| 11 | Escape im URL-Feld startet nichts und ändert nichts | UI-003, UI-113 | grün | grün | grün | rot |
| 12 | Nach ↓ und Escape startet das nächste Enter | UI-003, UI-113 | grün | rot (2) | grün | rot |
| 13 | Kein Start während IME (synthetisch: `isComposing`, `keyCode` 229, Safari-Folge nach `compositionend`); danach startet echtes Enter | UI-113 | grün | grün | grün | grün |
| 14 | Kein Start während echter IME-Komposition (CDP `imeSetComposition` plus Taste „Process“/229); Festschreiben und Enter startet | UI-113 | grün | übersprungen | grün | übersprungen |

(1) Playwrights Chromium und headless WebKit können die native `<datalist>`-Auswahl nicht per Tastatur bedienen. Der Test prüft dort
nur „Enter nach Pfeiltaste startet nicht und fragt nicht“ und vermerkt `engine-limit`. Die Übernahme des Eintrags ist nur bei
WebKit mit Anzeige prüfbar (Probe im Test): dort grün (siehe unten, `xvfb-run --headed`).
(2) Befund 2.

Zusatzlauf WebKit mit Anzeige (`xvfb-run -a … --headed`, Stand vor Aufspaltung von Test 11/12): 12 grün, Test 3 rot, Test 14 übersprungen.
Dort wird die Übernahme per Pfeil und Enter tatsächlich geprüft und ist grün; Test 12 ist dort ebenfalls grün.

## Testliste Vitest (15 Tests, alle grün auf `m3/b4-enter`)

IME-Ereignisfolgen (offen/Festschreiben, Safari-`keyCode` 229, auch bei pausiertem Crawl); Rückfrage Enter, Abbrechen, Enter, Ersetzen (UI-003);
drei Starts mit gefüllter Vorschlagsliste (neueste zuerst); Pfeil + Enter reicht genau ein Enter an die Liste (↓ und ↑);
Escape/Tippen nach Pfeil gibt Enter frei; Escape im Feld; je für `paused` und `incomplete` (unterbrochen): unverändert setzt fort (auch normalisierte
Schreibweise), leeres Feld und unbekannte Start-URL gelten als unverändert, geänderte Adresse fragt und startet erst nach „Ersetzen“;
Mod+Enter über den echten Tastatur-Dispatcher (fortsetzen bzw. fragen); Start-Aktion während „läuft“ und mit Ergebnissen; Enter während „läuft“.
Alle mit `// spec:` (UI-003, UI-113).

## Mutationsprobe gegen `bc8e2f65` (Stand vor dem Paket)

Dieselben Testdateien, Produktcode von `bc8e2f65`, gleiche Bridge-Binärdatei (keine Rust-Änderung im Paket).

- WebKit headless: 11 rot, 2 grün (Test 5 und 13, die nichts Neues prüfen), 1 übersprungen. Zweiter, dritter Start und alles mit Vorschlagsliste rot, wie im Nutzerbefund.
- Chromium: 5 rot (Test 3, 6, 7, 8, 9), 9 grün. In Chromium war der Mehrfachstart per Enter auch vorher nicht kaputt, es fehlte das Fortsetzen.
- Vitest: 11 rot, 4 grün.

## Läufe auf der Stand-3c5355b6-Basis

- `npm run -w apps/desktop typecheck`: grün. `npm run -w apps/desktop test`: 129 Dateien, 1147 Tests grün.
- Playwright Chromium: 13 grün, 1 rot (Befund 1). WebKit: 11 grün, 2 rot (Befund 1, 2), 1 übersprungen.
- Auf dem Mac wurde nichts ausgeführt; `target/` auf dem Server ist gelöscht.

## Befunde

### Befund 1: Enter in der Rückfrage ist keine Standardaktion (Test 3, beide Engines, auch auf `bc8e2f65`)
UI-113 (Kontextregeln): „In modalen Dialogen sind globale Kürzel außer Esc (Dialog schließen) und Enter (Standardaktion) deaktiviert.“
Reproduktion: Crawl abschließen, andere URL tippen, Enter (Rückfrage „Replace the current view?“ erscheint), Enter. Die Rückfrage bleibt offen, es startet nichts.
Ursache vermutlich: Der Fokus liegt beim Öffnen auf dem Dialogcontainer (`SECTION`, nicht auf einer Schaltfläche) und der Dialog hat keine Enter-Behandlung.
Nicht Teil des Pakets (Dialogbaustein, bereits vor B4-ENTER so). Gehört in die Merge-Entscheidung nur, wenn der Orchestrator Enter in der Rückfrage zum Umfang zählt.
Siehe auch Spec-Unklarheit 2.

### Befund 2: Nach ↓ und Escape geht das nächste Enter verloren (Test 12, WebKit headless, nicht Chromium, nicht WebKit mit Anzeige)
Reproduktion: Feld mit Vorschlagsliste (mindestens ein Eintrag), Crawl abgeschlossen, URL tippen, ArrowDown, Escape, Enter. Es kommt keine Rückfrage; erst das zweite Enter startet.
Beleg: Mitgeloggte `keydown`-Ereignisse zeigen `Escape` bereits als `defaultPrevented` auf `document` im Capture. Der globale Dispatcher (`ui/dispatcher.ts`) behandelt Escape im Feld
(Kürzel `search.clear`, Bereich „field“) und ruft `preventDefault` und `stopPropagation`. Dadurch erreicht Escape den `onKeyDown` des URL-Felds nie, und das Zurücksetzen
von `browsingSuggestions` (Header.tsx, Zweig `Escape`) läuft nicht. Das nächste Enter wird dann der nativen Liste überlassen. Mit echter WebKit-Auswahl (Anzeige) schließt Escape die Liste,
und das Enter wird nativ ausgelöst (Test grün). Ob das im echten WKWebView genauso ist, kann ohne Mac-Lauf nicht belegt werden.
Schwere gering bis mittel; Behebung läge im Paket (Zurücksetzen auch über Eingabe-/Blur-Ereignisse oder Escape im Capture vor dem Dispatcher).

### Hinweis (kein Fehler): Enter während IME-Komposition über `page.keyboard`
Wird bei offener CDP-Komposition ein normales Enter mit Zeichenereignis gesendet (`page.keyboard.press("Enter")`), startet der Crawl über die implizite Formularübermittlung
(`onSubmit` prüft keine Komposition). Echte IMEs senden keine Zeichenereignisse für dieses Enter (Taste „Process“/229), deshalb prüft Test 14 diese Form und ist grün.
Eine zusätzliche Absicherung im Submit-Zweig wäre billig.

## Spec-Unklarheiten

1. **Mod+Enter im Textfeld:** UI-113 nennt „Mod+Enter (im URL-Feld auch Enter)“, die Kontextregel lässt im Textfeld aber nur Esc, Enter, Mod+F, F6/Shift+F6 global zu.
   Ob Mod+Enter im URL-Feld wirken soll, ist offen. Getestet ist nur Mod+Enter außerhalb von Textfeldern (so im Auftrag).
2. **„Enter (Standardaktion)“ im Dialog „Ersetzen“:** Es steht nicht, welches die Standardaktion einer Rückfrage mit zerstörerischer Wirkung ist (Ersetzen oder Abbrechen).
   Test 3 nimmt „Ersetzen“ an (primäre Schaltfläche).
3. **Pfeil, Escape und Enter in der Vorschlagsliste:** UI-003 beschreibt die Liste nur als „Dropdown-Vorschläge“. Das Verhalten von Enter nach Pfeil/Escape ist nur aus dem Auftrag („Escape verhält sich wie bisher“) abgeleitet.
4. **Unterbrochen („incomplete“) in E2E:** Der Zustand ist per Playwright nur über Neustart der Bridge herstellbar (`e2e/beta/persistent-bridge.ts`); er ist deshalb nur in Vitest abgedeckt.
5. **Leeres Feld bei fertigem Crawl:** UI-003 sagt nur, dass ein leeres Feld „Start“ deaktiviert. Wie Enter dann reagiert, ist nicht festgelegt und nicht getestet.

## Branch und Hash

Branch `m3/t-enter`; Testcommit `3c5355b6` (Tests), Berichtscommit ist die Spitze des Branches (der Hash steht in der Abgabemeldung). Nicht gepusht.
