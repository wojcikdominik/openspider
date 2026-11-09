# B4-ENTER – Enter im Start-URL-Feld startet in WebKit keinen zweiten Crawl

Stand: 09.11.2026 · Branch `m3/b4-enter` (Basis `main` `bc8e2f65`) · Tests: Linux-Build-Server (Worktree `b4-enter`)
Spec: UI-003 („Enter im URL-Feld entspricht ‚Start‘“, Vorschlagsliste der letzten 20 Start-URLs), UI-113
(Tabelle „Crawl starten / fortsetzen: Mod+Enter, im URL-Feld auch Enter“, Kontextregel für Textfelder)
Ergebnis: PAKET FERTIG (nicht gepusht)

## Ursache

Auslöser ist die Vorschlagsliste am Feld (`<input list="os-start-history">` mit `<datalist>`). Fokus,
Composition-State, ein fremder `keydown`-Handler und der Submit-Button sind es nicht.

- Beim ersten Start ist die Liste leer, Enter funktioniert. Mit dem Start landet die URL in der Liste (UI-003).
- Ab dann leitet WebKit Enter im Feld an seinen Vorschlags-Picker weiter. Das `keydown` gilt intern als
  behandelt (`defaultHandled`, für JavaScript unsichtbar, `defaultPrevented` bleibt `false`). Deshalb folgt kein
  `keypress`, und ohne `keypress` gibt es keine implizite Formularübermittlung. `onSubmit` in `Header.tsx` läuft
  nie, auch nicht beim zweiten oder dritten Druck. Der Picker bleibt bestehen, auch wenn gerade kein Eintrag
  passt und nichts sichtbar aufgeklappt ist. Das erklärt die Beobachtung „auch ohne History-Dropdown“.
- Chromium leitet Enter nur dann an sein Popup, wenn dort ein Eintrag markiert ist. Sonst feuert es `keypress`
  und schickt das Formular ab.

Diagnose (temporärer Spec, nicht eingecheckt). Gemessen wurden die Ereignisse am Feld beim Enter nach
Crawl 1, Linux-WebKit, Bridge:

| Variante | Ereignisse bei Enter | Dialog „Replace …“ |
|---|---|---|
| Liste mit Eintrag (Normalfall), neue URL passt zum Eintrag | `keydown`, `keyup` – kein `keypress`, kein `submit` | nein |
| Liste mit Eintrag, getippte URL passt nicht (`localhost:…/x/`) | wie oben | nein |
| nach Reload (Liste beim Laden schon gefüllt) | wie oben | nein |
| `list`-Attribut entfernt, aber erst nach Crawl 1 | wie oben (Picker existierte schon) | nein |
| `list`-Attribut **vor** Crawl 1 entfernt | `keydown`, `keypress`, `submit`, `keyup` | ja |
| Chromium, alle Varianten | `keydown`, `keypress`, `submit`, `keyup` | ja |

`isComposing` war immer `false`, `keyCode` 13, `defaultPrevented` `false`.

## Fix

`apps/desktop/src/features/header/Header.tsx` und `model.ts`:

- Das Start-URL-Feld behandelt Enter selbst im `keydown`. Es ruft `requestStart()` auf, denselben Weg wie
  „Start“ (Prüfung, Rückfrage bei vorhandenen Ergebnissen, Login-Warnung), und danach `preventDefault()`. So
  schickt keine Engine das Formular zusätzlich ab, und es gibt keinen Doppelstart in Chromium. `onSubmit` bleibt
  für den Klick auf „Start“.
- Ob ein `keydown` startet, entscheidet `enterStartsCrawl()` in `model.ts`. Diese Fälle startet die Funktion
  nicht:
  - IME-Composition: `isComposing` oder `keyCode === 229`. Safari meldet das bestätigende Enter nach
    `compositionend` mit 229.
  - Ein Modifikator ist gedrückt. Mod+Enter bleibt beim globalen Dispatcher (UI-113), Shift/Alt+Enter bleiben
    nativ.
  - Das Ereignis ist schon `defaultPrevented`.
  - Enter direkt nach ↑/↓ in die Vorschläge (nur, wenn die Liste Einträge hat). Dieses Enter bleibt nativ und
    übernimmt den markierten Vorschlag. Der Merker fällt bei Eingabe, Escape, Blur und nach jedem Enter, also
    startet spätestens das nächste Enter. Ein Enter kann so nie dauerhaft verschluckt werden.
- Escape und die Pfeiltasten bekommen kein `preventDefault`. Das native Verhalten (Vorschläge öffnen und
  schließen) bleibt unverändert. Die Vorschlagsliste selbst ist unverändert.

## Tests

- **E2E (neu):** `apps/desktop/e2e/header/enter-restart.spec.ts` (`// spec: UI-003`, `// spec: UI-113`). Der
  Test läuft mit echter Tastatur (`page.keyboard`), echten Crawls über `os-app-bridge` und einem lokalen
  Node-Fixture-Server. Er nutzt `freshState`, die Liste ist also zu Beginn leer.
  1. Crawl 1 per Tippen und Enter, warten auf `completed`. Die URL steht jetzt in der Liste.
  2. Feld leeren (Mod+A, Backspace), neue URL tippen, Enter. Die Rückfrage „Replace“ erscheint (UI-003:
     „Start = neu crawlen nach Rückfrage“). Nach dem Bestätigen holt der Kern `/second/`, und Crawl 2 wird fertig.
  3. Enter auf dieselbe URL fragt erneut. Escape schließt ohne Start, das Feld bleibt.

  Im Mock-Transport wird der Test übersprungen, weil der Mock keinen Crawl-Lebenszyklus hat. Auch
  `header.spec.ts › CRAWL-001` ist im Mock rot (auf `main` geprüft).
- **Unit (neu, `Header.test.tsx`):** zwei Tests, beide mit `// spec: UI-003` und `// spec: UI-113`.
  - Enter startet und ruft `preventDefault` auf. Keinen Start lösen aus: Enter während einer Composition, mit
    `keyCode` 229, Shift+Enter und Escape. ↑ ohne Vorschläge hält Enter nicht zurück. Abgeschlossene Daten führen
    zur Rückfrage.
  - ↓ und dann Enter bleibt nativ (übernimmt den Vorschlag). Das nächste Enter startet mit dem übernommenen Wert.
    ↑, Enter, Enter: Das zweite Enter startet wieder.

### Läufe (Server, `srv-run.sh b4-enter`, Bridge `cargo build --locked -p os-app --features e2e-bridge --bin os-app-bridge`)

| Befehl | Ergebnis |
|---|---|
| `npm run -w apps/desktop typecheck` | grün |
| `npm run -w apps/desktop test` | CSS-Lint OK, 128 Dateien / 1 130 Tests grün |
| `OS_E2E_TRANSPORT=bridge npx playwright test -c e2e/header/playwright.config.ts --workers=1 --project=chromium --project=webkit` (Header, Statusleiste, neuer Test) | 10 passed |
| `npx playwright test --workers=1 --project=chromium --project=webkit e2e/beta/unreach.spec.ts e2e/keyboard/shortcuts.spec.ts` (drücken ebenfalls Enter im Feld bzw. prüfen UI-113) | 178 passed |

### Gegenprobe ohne Fix

Auf dem Server nur `Header.tsx` auf `bc8e2f65` zurückgesetzt (`git checkout -- …/Header.tsx`). Test und
`model.ts` blieben gleich.

| Test | Ergebnis ohne Fix |
|---|---|
| `enter-restart.spec.ts` WebKit | **rot**: Nach dem zweiten Enter erscheint kein Dialog (`expect(dialog).toBeVisible()`, Zeile 59) |
| `enter-restart.spec.ts` Chromium | grün (erwartet, Chromium war nicht betroffen) |
| die zwei neuen Unit-Tests | **rot** (2 failed, 15 passed) |

Danach wurde der Fix wieder eingespielt und der WebKit-Test erneut ausgeführt: grün.

**Hinweis macOS:** Der Server belegt den Fix für Linux-WebKit (WPE). Der Picker-Pfad
(`TextFieldInputType::handleKeydownEvent`) ist plattformübergreifend WebKit-Code, also betrifft der Fehler auch
WKWebView. Der Fix greift dort, bevor WebKit das Ereignis bearbeitet, nämlich im DOM-`keydown`. Ein Lauf auf
macOS-WebKit, über macOS-CI oder im Zieltest der App, steht noch aus.

## Nachtrag: Fortsetzen per Enter / Mod+Enter (Orchestrator, 09.11.2026)

Auslöser war meine Rückfrage: UI-113 nennt „Crawl starten / fortsetzen: Mod+Enter (im URL-Feld auch Enter)“.
`requestStart()` setzte einen pausierten Crawl aber nicht fort. Der Orchestrator hat entschieden: Im Zustand
„pausiert“ setzen Enter im URL-Feld und Mod+Enter den Crawl fort, solange die URL unverändert ist. Bei geänderter
URL läuft der normale Startweg mit Rückfrage. Diese Entscheidung steht jetzt als **OS-Festlegung (Orchestrator,
09.11.2026)** unter UI-113 in `docs/spec/05-ui.md`.

**Umsetzung** (`Header.tsx`, `model.ts`):

- `requestStart()` ist der gemeinsame Weg für Enter im Feld, Mod+Enter (Aktion `crawl.start`) und „Start“.
  Bei einem pausierten oder unterbrochenen Crawl (`resumable`) mit unveränderter URL ruft er jetzt
  `pauseResume()` auf: keine Rückfrage, kein neuer Crawl.
- Ob die URL geändert ist, entscheidet `startUrlDiffers()` in `model.ts`. Verglichen wird die normalisierte
  Adresse im Feld mit der normalisierten Start-URL des Crawls. `site.test/blog/` gilt also als gleich zu
  `https://site.test/blog/`.
- Nur bei geänderter URL erlaubt `canStart` den Start auch im Zustand `resumable`. Dann laufen Prüfung,
  Rückfrage „Replace“ und `crawl.start`. Der Kern nimmt den Start bei pausiertem Crawl an (E2E unten).
- Die Schaltflächen bleiben unverändert: Im Zustand „pausiert“ zeigt die Kopfleiste weiter „Fortsetzen“ statt
  „Start“.

**Auslegung, bitte bestätigen:** Ein leeres oder ungültiges Feld gilt als **unverändert**, ebenso ein Crawl ohne
bekannte Start-URL (z. B. ein unterbrochener Crawl ohne `startUrl` im Tick). Enter setzt dann fort und ersetzt
nichts. Das ist die nicht-destruktive Wahl, und ich habe sie so in die Festlegung geschrieben.

**Mod+Enter im URL-Feld:** Nach der Kontextregel von UI-113 wirken in einem Textfeld nur Esc, Enter, Mod+F und
F6 global. Mod+Enter wirkt deshalb weiterhin nur außerhalb von Textfeldern, und im URL-Feld ist Enter der Weg.
Unverändert.

**Tests:**

- **Unit** (`Header.test.tsx`, `// spec: UI-113`, `// spec: UI-003`):
  - Pausiert: Enter im Feld setzt fort. `runAction("crawl.start")` (Mod+Enter) mit anders geschriebener, aber
    gleicher URL setzt ebenfalls fort. Ein unterbrochener Crawl ohne Start-URL setzt trotz anderer Eingabe
    fort. Kein Dialog, kein `crawl.start`, kein `validateStartUrl`.
  - Pausiert mit geänderter URL: Enter, Rückfrage, „Replace“, dann `crawl.start` mit der neuen URL und kein
    `resume`.
- **E2E** (`e2e/header/enter-restart.spec.ts`, zweiter Test, echte Tastatur, Bridge, endlose Fixture wie
  `header.spec.ts › CRAWL-001`):
  1. Start per Enter, dann Pause.
  2. Enter im Feld: Der Crawl läuft weiter, kein Dialog, dieselbe `crawlId`.
  3. Pause, Fokus raus aus dem Feld, Ctrl+Enter (Plattform per Init-Skript auf Windows gestellt, damit Mod
     unabhängig vom Host ist): läuft weiter, kein Dialog, dieselbe `crawlId`. Die Start-URL wurde genau einmal
     geladen.
  4. Pause, URL geändert, Enter: Rückfrage, „Replace“. Der Kern lädt `/other/`, und es gibt eine neue `crawlId`.

| Lauf (Server, `--workers=1`) | Ergebnis |
|---|---|
| `npm run -w apps/desktop typecheck` | grün |
| `npm run -w apps/desktop test` | CSS-Lint OK, 128 Dateien / 1 132 Tests grün |
| `e2e/header/playwright.config.ts` Chromium + WebKit (Header, Statusleiste, beide neuen Tests) | 12 passed |
| `e2e/beta/unreach.spec.ts`, `e2e/keyboard/shortcuts.spec.ts` Chromium + WebKit | 178 passed |
| **Gegenprobe:** `Header.tsx` auf dem Stand `4953d3a7` (B4-ENTER-Fix ohne Fortsetzen) | Fortsetzen-E2E in Chromium und WebKit **rot** („Expected running, Received paused“ nach Enter), die zwei neuen Unit-Tests **rot** |

Nach der Gegenprobe wurde der Fix wieder eingespielt und der Test erneut ausgeführt: grün. Danach wurde `target/`
auf dem Server gelöscht.

## Fremde Dateien (K2-92)

| Datei | Bereich | Änderung |
|---|---|---|
| `docs/spec/05-ui.md` | Spec (UI-113) | OS-Festlegung (Orchestrator, 09.11.2026) zum Fortsetzen per Enter/Mod+Enter, im Auftrag des Orchestrators |

Sonst nur eigene Pfade: `apps/desktop/src/features/header/{Header.tsx,model.ts,Header.test.tsx}`, `apps/desktop/e2e/header/enter-restart.spec.ts`, dieser Bericht.

## Aufräumen

Auf dem Server sind `target/` im Worktree `b4-enter` und die temporären Diagnose-Specs gelöscht. Kein `pkill`
wurde ausgeführt. Ein Workspace-`cargo test` war nicht nötig, weil kein Rust geändert wurde, also auch kein
ws-slot.

Branch `m3/b4-enter`, Hash: letzter Commit dieses Berichts (wird bei der Abgabe gemeldet).
