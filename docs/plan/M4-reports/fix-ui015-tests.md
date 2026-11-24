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
