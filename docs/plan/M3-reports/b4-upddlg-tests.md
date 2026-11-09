# B4-UPDDLG – unabhängige Abnahmetests

Testagent (Sonnet), Branch `m3/t-upddlg` auf `m3/upddlg` (49a6967c). Grundlage allein die Spec (`docs/spec/06-output-ops.md` OUT-250 bis OUT-251, OUT-181, `docs/spec/05-ui.md` UI-127/UI-126, `docs/release/release-rhythmus.md`, `docs/release/notes-template.md`). Der Bauer-Bericht und sein Ordner wurden nicht gelesen. Der Produktcode wurde nur zum Aufrufen benutzt (Kommandonamen, Einhängepunkte). Bestehende Tests sind unverändert.

## Ergebnis in einem Satz

Alle Abnahmetests sind grün (Playwright 48/48 in Chromium und in WebKit, Vitest 21/21, node:test 17/17, Rust os-cli 3/3 mit und ohne Build-Nummer) bis auf **einen roten Rust-Test**: ein Randfall der Überspringen-Logik (Produktabweichung vom Wortlaut, siehe unten). Gegen den Stand vor dem Paket (`17689124`) sind die Tests überwiegend rot (Mutationsprobe unten).

## Testdateien und Spec-Zuordnung

| Datei | Tests | Spec-IDs |
|---|---|---|
| `apps/desktop/e2e/acceptance/b4-upddlg.spec.ts` (Playwright, echte Klicks, Tastatur, Mausrad) | 48 | OUT-251, OUT-250, OUT-257 (Sprachregel), UI-126, UI-127 |
| `apps/desktop/src/features/update/acceptance-upddlg.test.tsx` (Vitest, `UpdateHost` gegen Mock-Kern) | 21 | OUT-251, OUT-250, UI-127 |
| `scripts/release/acceptance-upddlg.test.mjs` (node:test, Schwarzkasten über die Skript-Kommandozeilen) | 17 | OUT-251, OUT-261, UI-127 |
| `crates/os-cli/tests/acceptance_version.rs` | 3 | OUT-181, UI-127 |
| `crates/os-app/tests/acceptance_upddlg.rs` (nicht im Auftrag genannt, aber nötig: die Überspringen-Logik liegt im Kern) | 8 | OUT-251, OUT-250, OUT-222, UI-127, OUT-181 |

### Playwright (je Browser 48 Tests)

- **Aufbau (de, en; 1024×640 und 800×640)**: Symbol links vom Titel, Titel fett, Text darunter, Überschrift „OpenSpider <Version>“ im eigenen scrollbaren Kasten (Mausrad scrollt, Knopfreihe bleibt stehen), darunter Kontrollkästchen, darunter die Knopfreihe; Dialog passt in 1024×640; axe ohne critical/serious (OUT-251, UI-126). Deutscher Kopftext wörtlich aus der Spec.
- **Knopfreihe (macOS/Windows × de/en)**: eine Reihe; „Diese Version überspringen“ links; rechts macOS: Später, Beim Beenden, Update installieren (Primärknopf ganz rechts); Windows: Primärknopf zuerst (OUT-251).
- **Einzeilig (de/en/es/fr/it × 800×640/1024×640, 10 Tests)**: in vier Zuständen (Download fehlgeschlagen = „Im Hintergrund herunterladen“, bereit, Crawl läuft = „Nach Crawl-Ende installieren“, vorgemerkt = „Schließen“) je Knopf: ein Zeilenkasten, nicht abgeschnitten, nicht zweizeilig hoch, im Fenster, kein horizontaler Überlauf von Seite und Dialog (OUT-251).
- **Notes (de/en/es/fr/it)**: nur die Oberflächensprache (de → deutscher Teil, sonst englischer), keine Rohüberschrift „###“/„Deutsch“, Liste und Fettdruck gerendert (OUT-251, OUT-257).
- **HTML**: `script`, `u`, `img`, `iframe` usw. werden nicht eingehängt, `window.__pwned` bleibt unset, `javascript:`-Adressen sind keine Links (OUT-251).
- **Links (de, en)**: echter Klick öffnet einen neuen Tab (Browser-Transport), das App-Fenster navigiert nicht, der Dialog bleibt offen (OUT-251).
- **Aktionen (de, en)**: Überspringen schreibt `updates.skippedVersion`, die manuelle Prüfung zeigt die Version trotzdem, eine neuere ebenso; Später erinnern ändert nichts; „Beim Beenden installieren“ → `{when:"onQuit"}`, „Update installieren“ → `{when:"now"}`; Installationsknöpfe bis Download-Ende gesperrt, Fortschrittsbalken (value 42), danach aktiv; laufender Crawl → nur „Nach Crawl-Ende installieren“ (`{when:"afterCrawl"}`) (OUT-251).
- **Kontrollkästchen (de/en × Einstellung an/aus)**: zeigt den Wert der Einstellung, Klick schreibt `updates.check`, beim erneuten Öffnen gilt der gespeicherte Wert, es gibt nur dieses eine Kontrollkästchen (OUT-250/OUT-251).
- **Tastatur (de, en)**: Tab bleibt im Dialog und erreicht alle Knöpfe und das Kontrollkästchen, sichtbarer Fokus; Esc wie „Später erinnern“ (keine Einstellung, keine Installation), Fokus kehrt zum Auslöser zurück; Leertaste schaltet das Kontrollkästchen, Enter auf „Beim Beenden installieren“ (UI-126, OUT-251).
- **Versionsanzeige (de, en)**: „Über“, Debug-Info und Update-Dialog zeigen „1.0.0-beta.3 (Build 2578)“, die neue Version steht ohne „(Build …)“; ohne Nummer nur die Version, nie „(Build“, „null“ oder „undefined“ (UI-127).

### Vitest (21)

Notes-Sprache und Marker (de/en/es), Liste/Fettdruck, HTML/`javascript:`, Link öffnet extern und navigiert nicht, Kopftext mit/ohne Build-Nummer, Download-Zustand und Aktivierung der Installationsknöpfe, Crawl läuft, `onQuit`/`now`, Überspringen speichert und schließt, manuelle Prüfung zeigt übersprungene Version, Später/Esc ohne Nebenwirkung, Kontrollkästchen an/aus/Umschalten/Wiedereinblenden/kein Auto-Install-Schalter.

### node:test (17)

`notes.mjs --check` grün/rot (fehlender Abschnitt ab beta.4, beta.5, rc.1, 1.0.0, 1.0.1 rot; bis beta.3 weiter grün mit ausführlichen Notes im Manifest); nur der kurze Abschnitt in der Update-Notes-Datei und per `manifest.mjs` in beiden Manifesten, ohne Markierungszeilen; Release-Text und CHANGELOG nur aus dem Rest; Punktzahl 0/1/2/7/10 rot, 3 bis 6 grün (en und de getrennt); 160/161 Zeichen; einzeilige Punkte; Markierungen (doppelt, fehlend, nicht allein, falsche Reihenfolge, `### Deutsch` fehlt/doppelt); TODO/STUB; Fettdruck/Links erlaubt; Vorlage `notes-template.md` ist ausgefüllt ein gültiges Dokument; Release-Workflow: Build-Nummer einmal im `prepare`-Job mit `fetch-depth: 0` und `git rev-list --count HEAD`, an macOS/Windows/Linux-Kern als `OPENSPIDER_BUILD_NUMBER` weitergereicht, Abbruch ohne gültige Nummer.

### Rust

- `os-cli/tests/acceptance_version.rs`: `--version` einzeilig, Version = `workspace.package.version`, Hash und `<os>-<arch>`; mit `OPENSPIDER_BUILD_NUMBER=n` genau `(Build n, <hash>, <os>-<arch>)`, ohne Variable im Debug-Build keine Nummer; Nummer ist positive Zahl ohne führende Null.
- `os-app/tests/acceptance_upddlg.rs`: automatische Prüfung meldet die übersprungene Version nicht mehr, manuelle zeigt sie; ältere/gleiche übersprungene Versionen (1.0.0, 0.9.0, 1.0.1-beta.9, 1.0.0-rc.1) melden 1.0.1 wieder; `skipped_version` steht in `settings.toml` unter `[updates]` und übersteht einen Neustart; Später erinnern schreibt nichts; `app_info.buildNumber` und `diag::system_info()` (`buildNumber`, `versionDisplay`) mit und ohne Nummer.

## Ergebnisse (Build-Server, `--workers=1`)

| Suite | Chromium | WebKit |
|---|---|---|
| Playwright `b4-upddlg.spec.ts` | 48/48 grün (2,1 min) | 48/48 grün (2,2 min) |

Vitest 21/21, node:test 17/17, `tsc --noEmit` sauber, `cargo fmt --all --check` sauber, `cargo clippy` für beide Rust-Testziele mit `-D warnings` sauber.
Rust: `acceptance_version` 3/3 ohne und mit `OPENSPIDER_BUILD_NUMBER=2578`; `acceptance_upddlg` 7/8 grün (der eine rote siehe unten; `app_info`-Test auch mit Nummer 2578 grün). Ein ungültiges `OPENSPIDER_BUILD_NUMBER=abc` bricht den Bau von `os-model` ab (von Hand geprüft, kein Test: ein Test müsste Cargo in Cargo starten).

Zwischenstände, die Testfehler waren (nicht das Produkt): Die Tick-Fixture bringt eine Warnung „Speicher knapp“ mit, deren Dialog den Update-Dialog überdeckte (Test emittiert jetzt `notices: []`); das Symbol (`img`) zählte als „eingehängtes HTML“ (Prüfung jetzt nur im Notes-Kasten); `es/fr/it` setzen die Menüsprache auf `en` (Warten auf `^(<Sprache>|en)$`).

## Produktfehler und Abweichungen

1. **Rot: `a_version_older_than_the_skipped_one_stays_silent_for_automatic_checks`** (`cargo test -p os-app --test acceptance_upddlg`). Skipped = `1.0.2`, das Manifest bietet `1.0.1` an, automatische Prüfung → `Available(1.0.1)`. Spec (OUT-251): automatische Prüfungen melden die übersprungene Version nicht mehr, „erst eine neuere Version“. In `crates/os-app/src/commands/update.rs` vergleicht der Kern die übersprungene Version auf Gleichheit; wörtlich gelesen müsste alles bis einschließlich der übersprungenen still bleiben. Randfall (zurückgezogenes Release, älteres Manifest), geringe Schwere. Entscheidung Fable: Vergleich auf „nicht neuer als übersprungen“ ändern (SemVer-Vergleich) oder den Test streichen und die Spec auf „gleiche Version“ präzisieren.

Sonst keine Produktfehler gefunden.

## Spec-Unklarheiten

- Die englischen Beschriftungen und die von es/fr/it stehen nicht in der Spec (nur die deutschen). Die Tests nehmen für Englisch die Oberflächentexte („Skip this version“, „Remind me later“, „Install on quit“, „Install update“, „Install after crawl ends“, „Download in background“, „Close“), für es/fr/it prüfen sie nur die Geometrie.
- „Auf Windows zuerst“ wird als „Primärknopf zuerst in der rechten Gruppe“ gelesen; „Diese Version überspringen“ steht auf beiden Systemen links.
- Welche Knöpfe bei fehlgeschlagenem Download stehen, ist offen (Spec nennt nur „Im Hintergrund herunterladen“); Tests prüfen dort nur Geometrie und diesen Namen.
- Ob die Markierungszeilen `<!-- update -->` im Manifest-`notes` stehen, sagt die Spec nicht; die Tests verlangen „nein“, weil der Dialog sie sonst als Text zeigen würde.
- Was die automatische Prüfung für eine übersprungene Version zurückgibt, ist offen; die Tests verlangen nur „nicht Available“ (die App liefert `UpToDate`, also „aktuell“, obwohl eine neuere Version existiert; im Dialog nur bei manueller Prüfung sichtbar).
- Beschriftungskonflikt (Hinweis, kein Verstoß): In der englischen Debug-Info heißt die Zeile mit dem Commit-Hash weiter „Build“, direkt neben „Version 1.0.0-beta.3 (Build 2578)“. „Über“ wurde auf „Commit“ umbenannt, die Debug-Info nicht. UI-127 nennt beides „Build“.

## Mutationsprobe gegen `17689124` (Stand vor dem Paket)

Eigene Testdateien in ein frisches Worktree auf `17689124` kopiert, selbe Befehle:

| Suite | Ergebnis vor dem Paket |
|---|---|
| Playwright Chromium | 48/48 rot |
| Vitest | 21/21 rot |
| node:test | 13 rot, 4 grün (Nr. 3 ist der gewollte Kompatibilitätstest bis beta.3; Nr. 10, 11, 12 erwarten „rot“ und sind dort aus anderem Grund rot, also nicht aussagekräftig) |
| `acceptance_version` ohne Nummer | 3/3 grün (Entwickler-Build, keine Nummer erwartet, unverändert richtig) |
| `acceptance_version` mit Nummer 2578 | 1 rot (Ausgabe `(unknown, linux-x86_64)` ohne Nummer) |
| `acceptance_upddlg` (os-app) | übersetzt nicht (`skipped_version`, `build_number` fehlen) = rot |

## Aufräumen

`target/` und das Worktree `t-upddlg-base` auf dem Server gelöscht, kein breites `pkill`. Der lokale Worktree `t-upddlg-base` ist entfernt.
