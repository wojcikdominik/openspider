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

## Beobachtung außerhalb des Auftrags (nicht geändert)

Laut UI-113 heißt die Zeile „Crawl starten / fortsetzen: Mod+Enter (im URL-Feld auch Enter)“.
`requestStart()` setzt einen pausierten oder unterbrochenen Crawl aber nicht fort, weil `canStart` bei
`resumable` falsch ist. Das galt schon vorher für Mod+Enter und für die implizite Übermittlung. Ob Enter bzw.
Mod+Enter im Zustand „pausiert“ fortsetzen soll, entscheidet der Orchestrator.

## Fremde Dateien (K2-92)

| Datei | Bereich | Änderung |
|---|---|---|
| – | – | keine; geändert wurden nur `apps/desktop/src/features/header/{Header.tsx,model.ts,Header.test.tsx}`, `apps/desktop/e2e/header/enter-restart.spec.ts` und dieser Bericht |

## Aufräumen

Auf dem Server sind `target/` im Worktree `b4-enter` und die temporäre Diagnose-Spec gelöscht. Kein `pkill`
wurde ausgeführt. Ein Workspace-`cargo test` war nicht nötig, weil kein Rust geändert wurde, also auch kein
ws-slot.

Branch `m3/b4-enter`, Hash: siehe Abschluss des Auftrags (letzter Commit dieses Berichts).
