# Unabhängige Abnahme Fix UI-015 (Escape im Dialog-Baustein)

Gegenstand: `apps/desktop/src/ui/primitives/Dialog.tsx`, Stand `9ad53e0e` (Zweig `fix/ui146-svg-export`).
Testzweig: `fix/ui146-svg-export-tests`, letzter Test-Commit `38d3df1e`. Die Tests stammen nur aus dem Sollverhalten im
Auftrag und aus der Spec (UI-113 Kontextregeln „In modalen Dialogen … Esc (Dialog schließen)“, Enter-Festlegung vom
09.11.2026). Tests und Bericht des Bauers habe ich nicht gelesen.

## Dateien

- `apps/desktop/src/ui/primitives/acceptance-dialog-escape.test.tsx` (Vitest, 14 Fälle)
- `apps/desktop/e2e/export/acceptance-dialog-escape.spec.ts` (Export options, 8 Fälle)
- `apps/desktop/e2e/tasks/acceptance-dialog-escape.spec.ts` (Scheduling mit Rückfrage „Delete task“, Settings → Email, 7 Fälle)
- `apps/desktop/e2e/crawls/acceptance-dialog-escape.spec.ts` (Manage crawls mit Rückfrage „Delete“ und Kontextmenü, 5 Fälle)

Alle Tests tragen `// spec: UI-015` und `// acceptance: independent`. Es gibt keine festen Wartezeiten und keine Retries.
Die Lücke vor dem Fokuswechsel wird mit angehaltener Seitenuhr (`page.clock.pauseAt`, das hält auch
`requestAnimationFrame` an) und einem programmatischen Klick erzeugt oder durch `blur()` auf `body` nachgebildet.
Synchronisiert wird über zwei Animationsframes.

## Läufe (Mac mini, unter `heavy.lock`, Logs in `/Volumes/osbuild/openspider/wt/fix-ui146-tests-log/`)

| Lauf | Stand | Ergebnis |
|---|---|---|
| run1: `npm ci`, Typecheck, Vitest | 220a0172 | TC 0, Vitest 14/14 grün |
| tc2: Typecheck | 3510c59d | 0 |
| pw3: Playwright chromium+webkit ×1 | 38d3df1e | 40/40 grün |
| pw10: Playwright chromium+webkit `--repeat-each=10` | 38d3df1e | 399 grün, 1 rot (B1-1, WebKit) |
| Gegenprobe: `Dialog.tsx` aus `6c995146` eingesetzt | 38d3df1e | Vitest 7 von 14 rot, Playwright 14 von 40 rot; danach Datei zurückgesetzt (sha 6e2fcadad0e1 = Fix-Stand) |

Zwischenstände pw1 und pw2 hatten Fehler in den Tests selbst, nicht im Produkt. Sie sind behoben:
- Bei angehaltener Uhr kehrt der Fokus erst einen Frame später zurück, wenn er schon im Dialog war. Die Fokusprüfung
  läuft jetzt nach `resume`.
- WebKit fokussiert einen nackten `<button>` der Testseite beim Mausklick nicht. Er wird jetzt per Tastatur geöffnet.
- „Manage crawls“ übernimmt den Fokus synchron, dort gibt es keine Lücke.

## Gegenprobe (Stand vor dem Fix)

Rot gegen `6c995146`, grün gegen `9ad53e0e`:
- Vitest: Escape mit Fokus auf `body` oder auf dem Auslöser, Escape nach Tastenkürzel, verschachtelt mit Fokus außerhalb,
  Gleichverhalten bei `isDismissable=false`, zwei Escapes, verschwundener Auslöser.
- Playwright: Export-Lücke und Export mit Fokus auf `body`, Scheduling-Lücke, Rückfrage mit Fokus auf `body`, Restfall
  verschachtelt, E-Mail-Dialog in der Lücke. Dazu kommen „zwei schnelle Escapes“ bei verschachtelten Dialogen (Tasks,
  Crawls): Vor dem Fix ging das zweite Escape verloren.

Der Fix behebt damit nachweislich den gemeldeten Fehler.

## Befunde

### B1 – Abweichung vom Sollverhalten

**B1-1 Verschachtelter Dialog, Fokus noch im äußeren (Restfall des Bauers): Escape geht verloren (WebKit, selten).**
- Erwartet: Escape schließt nur die innere Rückfrage „Delete task“, „Scheduling“ bleibt offen.
- Beobachtet: In 1 von 20 Läufen (WebKit, Lauf 217 in pw10) lag der Fokus nach dem Öffnen noch auf der Schaltfläche
  „Delete task“ im äußeren Dialog. Das Escape bewirkte nichts, die Rückfrage blieb offen.
  - In den übrigen 19 Läufen lag der Fokus auf `body`, und der Fall war grün.
  - In Chromium trat der Fall nie auf.
- Auslösung:
  1. Scheduling öffnen und „Nightly“ markieren.
  2. Fokus auf „Delete task“ setzen.
  3. Seitenuhr anhalten.
  4. „Delete task“ programmatisch klicken.
  5. Escape drücken.
- Ursache (aus dem Produktcode gelesen): React Aria verarbeitet das Escape im äußeren Overlay, weil das Ziel darin liegt
  (`preventDefault` und `stopPropagation`). Der äußere Dialog ist nicht der oberste und schließt nicht. Der
  Dokument-Listener des Fixes sieht das Ereignis nicht mehr bzw. nur als `defaultPrevented`.
- Wirkung für Nutzer: gering. Das Fenster ist etwa ein Frame lang und tritt nur bei programmatischer Öffnung oder
  Öffnung per Screenreader auf, nicht nach Maus oder Tastatur. Danach schließt ein zweites Escape. Datenverlust oder eine
  falsche Aktion ist nicht möglich, die Rückfrage bleibt nur offen.
- Test: `e2e/tasks/acceptance-dialog-escape.spec.ts` „… (Restfall)“. Er bleibt stehen und ist zeitabhängig rot.

### B2 – Unklarheit

**B2-1 Fokus, wenn der Auslöser nicht mehr existiert.** Die Spec legt kein Ziel fest. Beobachtet (Vitest): Der Dialog
schließt, es gibt keinen Fehler, und der Fokus bleibt nicht auf einem gelösten Element. Er liegt dann auf `body`. Ob ein
Ersatzziel gelten soll (etwa der Container des früheren Auslösers), sollte der Orchestrator entscheiden.

**B2-2 Wirkung von `isDismissable`.** Mehrere Verwender setzen `isDismissable={!busy}`. In React Aria wirkt das nur auf
den Klick außerhalb, nicht auf Escape. Dass Escape bei laufenden Vorgängen nicht schließt, sichern allein die
`onOpenChange`-Wächter der Verwender (Export, Tasks, Email). Mit dem Fix verhalten sich Escape innerhalb und außerhalb
des Dialogs gleich (Vitest-Test „4/5“). Ein Verwender ohne Wächter, aber mit `isDismissable={false}`, würde also per
Escape schließen. Ich empfehle, für Pflichtdialoge eine eindeutige Regel festzulegen.

### B3 – Hinweise

- **B3-1** „Manage crawls“ und seine Rückfrage übernehmen den Fokus synchron, auch beim programmatischen Öffnen. Dort
  gibt es keine Lücke. Der Gap-Fall ist an Export options, Scheduling und dem E-Mail-Konto belegt.
- **B3-2** Ist der Fokus schon im Dialog, kommt er nach dem Escape erst nach einem Animationsframe zum Auslöser zurück
  (Verhalten von React Aria, unverändert). In der echten Lücke gibt der Fix den Fokus sofort per Microtask zurück.
- **B3-3** Ein Escape in einem Suchfeld mit Text leert das Feld und lässt den Dialog offen. Im leeren Feld schließt es
  den Dialog (Vitest am `SearchField`-Baustein). In den geprüften App-Dialogen gibt es kein `SearchField`-Primitive. Das
  Suchfeld in „Manage crawls“ ist ein natives `input type=search`, dort schließt Escape wie bisher den Dialog.

## Bewertung Restfall „verschachtelte Dialoge“

In 19 von 20 Läufen geht der Fokus beim Öffnen der inneren Rückfrage auf `body`, weil der äußere Dialog inert wird. Dann
greift der Fix und schließt korrekt nur die Rückfrage. Nur wenn der Fokus noch auf dem auslösenden Element im äußeren
Dialog steht (in WebKit beobachtet), verschluckt das Overlay des äußeren Dialogs das Escape. Das ist ein echter, aber
seltener Fehler mit geringer Wirkung (B1-1). Er ist nicht neu: Vor dem Fix ging das Escape in diesem Fall immer
verloren. Der Fix ist für die Freigabe tragfähig; B1-1 kann als Folgearbeit laufen.

## Geprüfte Dialoge

1. Export options (Export, `ExportDialog`): Lücke, `body`-Fokus, Mod+E, Mausklick, Auswahlliste „File format“,
   laufender Exportstart (bleibt offen), Enter-Standardaktion, Fokusfalle, Abbrechen.
2. Scheduling (Planung, `TasksHost`) mit verschachtelter Rückfrage „Delete task“: Lücke, verschachtelt, Restfall, zwei
   schnelle Escapes, Enter bei der zerstörerischen Rückfrage, Abbrechen.
3. Add SMTP account (Einstellungen → Email): Escape im Textfeld, Lücke.
4. Manage crawls (Crawl-Verwaltung, `CrawlsHost`) mit Rückfrage „Delete“: Öffnen per Entf-Taste und sofortiges Escape
   (auch bei angehaltener Uhr), Fokus auf `body`, Kontextmenü (Shift+F10), zwei schnelle Escapes.
5. Baustein (Vitest): eigene Testdialoge mit `ComboBox`, `SearchField` und `DialogFooter`.

## Folgeauftrag 25.11.2026 (Bauer-Stand `23094ab6`, Endstand ausstehend)

Festlegungen des Orchestrators:
- B1-1 ist behoben: Es schließt immer der oberste Dialog.
- B2-2: Allein `isKeyboardDismissDisabled` steuert, ob Escape schließt; `isDismissable` betrifft nur den Klick daneben.
- B2-1: Ist der Auslöser verschwunden, schließt der Dialog ohne Fehler, und der Fokus darf auf `body` liegen.

`fix/ui146-svg-export` ist in den Abnahmezweig gemergt (`89921a79`).

### Wackler in der Vorbereitung beseitigt

`export/acceptance-dialog-escape.spec.ts` „Escape while the focus lies on body …“ (im Lauf des Bauers 1 von 1 120 rot)
hatte eine Wettlaufbedingung: Das `blur()` kam, bevor React Aria den Fokus einen Frame nach dem Öffnen in den Dialog
legt. Neue Reihenfolge:
1. warten, bis der Fokus im Dialog ist (`expect.poll`),
2. Seitenuhr anhalten (kein weiterer Frame),
3. `blur()`,
4. Vorbedingung „Fokus auf body“ hart prüfen,
5. Escape drücken.

Die Zusicherung bleibt unverändert, zusätzlich wird nach dem Weiterlaufen der Uhr geprüft, dass der Fokus beim Auslöser
bleibt.

Dieselbe Schwäche hatten weitere Fälle; sie sind ebenso festgezogen:
- Export: der laufende Vorgang.
- Tasks: die verschachtelte Rückfrage mit Fokus auf `body`.
- Crawls: die Rückfrage nach Mausklick. Dieser Dialog holt den Fokus beim `blur()` synchron zurück, „Fokus auf body“
  lässt sich dort also nicht herstellen. Der Fall liefert jetzt das Escape mit Ziel `body` aus, so wie die Tastatur es
  bei Fokus auf `body` täte. Bisher war die Vorbedingung dort nur scheinbar erfüllt.

Die Lückenfälle (Uhr angehalten vor dem programmatischen Öffnen) waren bereits deterministisch.

### Neue Fälle

- Vitest „4 (Festlegung 25.11.)“: Mit `isKeyboardDismissDisabled` bleibt der Dialog offen, wenn Escape mit Fokus im
  Dialog oder auf `body` kommt, und `onOpenChange` wird nicht aufgerufen.
- Vitest „3+4“: Ist der oberste Dialog ein Pflichtdialog, schließt Escape auch den Dialog darunter nicht, egal ob das
  Ziel im Pflichtdialog, auf `body` oder im unteren Dialog liegt.
- Vitest „3 (B1-1)“: Liegt das Escape-Ziel noch im äußeren Dialog, schließt nur der innere, und der Fokus geht an
  dessen Auslöser.
- Playwright Tasks „deterministic Restfall“: Uhr angehalten, Rückfrage programmatisch geöffnet, Escape mit Ziel auf
  „Delete task“ im äußeren Dialog. Es schließt nur die Rückfrage, und der Fokus kehrt zurück. Der bisherige
  Tastaturfall mit Fokus auf `body` oder im äußeren Dialog erwartet jetzt ebenfalls Grün.
- Playwright `settings-sections/acceptance-dialog-escape.spec.ts`: Einstellungsrahmen → Notifications → „Add SMTP
  account“, geöffnet, während der Fokus noch im Einstellungsrahmen liegt. Geprüft werden drei Wege: Escape über die
  Tastatur, Escape mit Ziel auf der Schaltfläche „Add“ der Einstellungen und Escape im Textfeld. Jedes Mal schließt nur
  der Kontodialog, die Einstellungen bleiben, und der Fokus kehrt zu „Add“ zurück.

Hinweis: Auf der Testseite ist der Einstellungsrahmen (`SettingsFrame`) das Fenster selbst, kein Overlay. Sobald der
Einstellungsdialog als eigenes Overlay vorliegt, prüfe ich den Fall dort erneut.

### Vorläufe gegen `23094ab6`

| Lauf | Stand | Ergebnis |
|---|---|---|
| a1: Typecheck, Vitest, Playwright ×1 | f3154021 | TC 0, Vitest 17/17, Playwright 46/48 (Fehler im Test: Crawls-Vorbedingung, siehe oben) |
| a2: Typecheck, Vitest, Playwright `--repeat-each=3` | c21f748d | TC 0, Vitest 17/17, Playwright 144/144 |
