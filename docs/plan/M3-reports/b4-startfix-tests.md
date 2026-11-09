# B4-STARTFIX – unabhängige Abnahmetests

Prüfgegenstand: Branch `m3/startfix` (d173923d), Basis vor dem Paket `c8a0d85f`. Grundlage allein die Spec: UI-019, CRAWL-128, CRAWL-131,
OUT-152, OUT-169, OUT-182, OUT-204, OUT-281, OUT-282 samt den OS-Festlegungen vom 09.11.2026. Der Bauer-Bericht wurde nicht gelesen. Der
Produktcode wurde nur gelesen, um Schnittstellen (Testharness, Selektoren) zu kennen; erwartet wird, was die Spec bzw. der Auftrag sagt.

Dateien (bestehende Tests unverändert):
- `crates/os-app/tests/acceptance_startfix.rs` (14 Tests, App-Kern über Start/Abfrage/Ende des Laufs, SMTP-Mock),
- `crates/os-cli/tests/acceptance_startfix.rs` (21 Tests: Binary `openspider`, `--run-task` als Prozess, Planerlauf im Prozess mit SMTP-Mock),
- `crates/os-app/tests/acceptance_startfix/{mime,fixtures}.rs` (Hilfen: MIME-Zerlegung der mitgeschnittenen Mails, robots.txt-Fixtures als
  temporäre Szenario-Ordner; die CLI-Tests binden sie per `#[path]` ein),
- `apps/desktop/e2e/acceptance/b4-startfix.spec.ts` (Playwright, 4 Tests je Engine, deutsche Oberfläche, echte Bridge).

Fixtures: robots.txt mit mehreren Gruppen, sperrende Zeile 7 (`Disallow: /secret/`, bewusst nicht Zeile 2 wie in `fx-ui-robots-start`), `Allow`
für `/secret/open.html`; robots.txt mit 500/503/429; nicht belegter Port; Server, der nie antwortet (hält das Ende jeder Verbindung fest, damit
„2 s ab dem Ende des letzten Abrufversuchs“ gemessen werden kann); `.invalid`-Host für DNS.
Playwright: Start-URL per Klick ins Feld und `page.keyboard.type`, Absenden mit Enter bzw. Klick auf „Starten“, Dialog mit Esc geschlossen. Nur
„URLs hinzufügen“ und „Crawl öffnen“ laufen über dieselben IPC-Aufrufe wie die übrigen E2E-Tests (die Oberfläche hat dafür keinen Dialog). Die
Sprache „de“ steht in der `settings.toml` des Bridge-Home.

## Ausführung (Build-Server, Worktree `t-startfix`, `--workers=1`, `--test-threads=1`)

- `os-app --test acceptance_startfix`: 14 grün.
- `os-cli --test acceptance_startfix`: 16 grün, 5 rot (Produktfehler 1–4, siehe unten).
- Playwright `b4-startfix.spec.ts`: Chromium 4 grün, WebKit 4 grün.
- rustfmt, `clippy -D warnings` (beide Test-Targets) und `npm run -w apps/desktop typecheck` sauber.
- Mutationsprobe: die 13 Produktdateien des Pakets (Kern `session/control.rs`, `commands/mail.rs`, `mail_hooks.rs`, `exec.rs`, `mail.rs`,
  `run_task.rs`, `crawl.rs`, `lib.rs`, `robots/parser.rs`, `Notifications.tsx`, drei `.ftl`) auf den Stand `c8a0d85f` gesetzt, Tests unverändert:
  App-Kern 6 von 14 rot, CLI/Planer 16 von 21 rot, Playwright Chromium 4 von 4 rot. Die grün gebliebenen sind die Gegenproben („keine Mail“,
  „keine Meldung“, Erfolg bei Sperre), die vor dem Paket ebenfalls gelten mussten. Danach Stand wiederhergestellt und erneut geprüft (14 bzw. 16/5).

## Testliste

| Test | Spec-ID | Ergebnis |
|---|---|---|
| App: Fehlschlag (Port zu) → genau eine Mail `[FAILED]` mit Adresse und Klasse in Klartext und HTML, auch mit „bei Erfolg“ an | OUT-281, OUT-282, CRAWL-128 | grün |
| App: „bei Fehlschlag“ aus → keine Mail, auch keine Erfolgsmail; Kontrolle: Erfolgslauf sendet seine Mail | OUT-281, CRAWL-128 | grün |
| App: ohne Empfänger keine Mail | OUT-281 | grün |
| App: robots.txt nicht erreichbar (Standardmodus) → genau eine Fehler-Mail mit Adresse | OUT-281, CRAWL-128 | grün |
| App: DNS-Fehler → Mail nennt Klasse und Host | OUT-281, CRAWL-128 | grün |
| App: robots-Sperre und robots.txt mit 500 → Lauf „Abgeschlossen“, Erfolgsmail ohne `[FAILED]` | OUT-281, CRAWL-128 | grün |
| App: nicht erreichbare Start-URL → genau eine Meldung mit Fehlerklasse, ≤ 2 s | UI-019, CRAWL-128 | grün |
| App: Server ohne Antwort (Timeout 1 s) → Meldung ≤ 2 s nach dem Ende des letzten Abrufversuchs (Verbindungsende gemessen) | UI-019, CRAWL-128 | grün |
| App: gesperrte Start-URL → „Abgeschlossen“, genau eine Meldung mit robots.txt-URL, Zeilennummer 7 als Zahl, Regeltext als eigener Wert, ohne „Zeile“-/„line“-Präfix; Konfiguration bleibt „respektieren“ | UI-019, CRAWL-128 | grün |
| App: erlaubte Start-URL (`/`, `/secret/open.html` per Allow) → keine Sperr-Meldung | UI-019, CRAWL-128 | grün |
| App: „robots.txt ignorieren“ → keine Meldung | UI-019 | grün |
| App: Meldung höchstens einmal je Crawl – nicht nach „URLs hinzufügen“ (URL wirklich abgerufen, Status 200 in der Crawl-Datei) und nicht nach Neustart der App + Crawl öffnen + „URLs hinzufügen“ | UI-019, CRAWL-131 | grün |
| App: zwei Crawls mit derselben gesperrten Start-URL → je Crawl genau eine Meldung | UI-019 | grün |
| App: robots.txt mit 500 → „Abgeschlossen“, keine Dialoge zur nicht erreichbaren Start-URL | CRAWL-128, UI-019 | grün |
| CLI: nicht erreichbar → Exit 0, Warnung mit Adresse, Schlusszeile nennt „nicht erreichbar“ statt „Fertig“, Zustand Failed; mit `--fail-on-start-error` Exit 3 | OUT-182, OUT-204, CRAWL-128 | grün |
| CLI: gesperrt → Exit 0 auch mit `--fail-on-start-error`, Zustand Completed, Warnung mit robots.txt-URL, Zeile 7 und Regel, kein „nicht erreichbar“ | OUT-182, OUT-204, CRAWL-128 | grün |
| CLI: erlaubte Start-URL → keine Sperr-Warnung | OUT-182 | grün |
| CLI: robots.txt 500/503/429 → Exit 0 mit `--fail-on-start-error`, Zustand Completed, Warnung zur Sperre | OUT-182, OUT-204, CRAWL-128 | grün |
| CLI: Warnung zur Regel „robots.txt nicht erreichbar“ nennt robots.txt-URL und Statuscode | OUT-182, CRAWL-128 | **rot** (Fehler 4) |
| CLI-Mail: Fehlschlag → genau eine Mail `[FAILED]` mit Adresse (mit „bei Erfolg“ an und aus) | OUT-281, OUT-282, CRAWL-128 | grün |
| CLI-Mail: Exit 3 (`--fail-on-start-error`) → genau eine Mail mit Adresse | OUT-281, OUT-182 | **rot** (Fehler 2) |
| CLI-Mail: Fehlerklasse genannt, robots.txt ignoriert | OUT-281, CRAWL-128 | grün |
| CLI-Mail: Fehlerklasse genannt, Standard (robots.txt beachten) | OUT-281, CRAWL-128 | **rot** (Fehler 1) |
| CLI-Mail: „bei Fehlschlag“ aus → keine Mail; Kontrolle Erfolgslauf sendet | OUT-281, CRAWL-128 | grün |
| CLI-Mail: robots-Sperre → Erfolgsmail ohne `[FAILED]`; bei „Erfolg“ aus keine Mail | OUT-281, CRAWL-128 | grün |
| CLI Auto-Vergleich: Failed, mit und ohne vorherigen Crawl → Vermerk „Auto-Vergleich übersprungen (Crawl fehlgeschlagen)“, Exit 0, kein „Auto-Vergleich fehlgeschlagen“/„Crawl erfolgreich“, Schlusszeile nennt Fehler, kein gespeicherter Vergleich | OUT-152, CRAWL-128 | grün |
| CLI-Mail mit Auto-Vergleich: Vermerk statt Änderungstabelle | OUT-152, OUT-281 | **rot** (Fehler 3) |
| Planer (`--run-task`): Historie `Failed`, Exit-Code 0, Message = Fehler des Crawls (ohne „erfolgreich“/„Auto-Vergleich“), Lauf-Log `result=Failed` | OUT-169, OUT-182, CRAWL-128 | grün |
| Planer mit Auto-Vergleich, erster Lauf und nach erfolgreichem Vorlauf: `Failed`, Vermerk in Historie und Lauf-Log, kein gespeicherter Vergleich | OUT-169, OUT-152, CRAWL-128 | grün |
| Planer: robots-Sperre → nicht `Failed`, Exit 0, mit und ohne Auto-Vergleich | OUT-169, CRAWL-128 | grün |
| Planer-Mail (4 Kombinationen Erfolg/Fehlschlag): Fehlschlag → genau 1 bzw. 0 Mails, `[FAILED]`, Adresse | OUT-281, OUT-169, CRAWL-128 | grün |
| Planer-Mail: Fehlerklasse, robots.txt ignoriert | OUT-281, CRAWL-128 | grün |
| Planer-Mail: Fehlerklasse, Standard (robots.txt beachten) | OUT-281, CRAWL-128 | **rot** (Fehler 1) |
| Planer-Mail mit Auto-Vergleich (vorheriger Crawl vorhanden): bei Fehlschlag „aus“ keine Mail; sonst genau eine Mail mit Vermerk statt Änderungstabelle, Historie `Failed` mit Vermerk | OUT-281, OUT-152, OUT-169, CRAWL-128 | grün |
| Planer: robots-Sperre → Erfolgsmail ohne `[FAILED]` | OUT-281, OUT-169, CRAWL-128 | grün |
| E2E (de): nicht erreichbare Start-URL → Dialog mit „Ursache: …abgelehnt“ und Adresse, Statusleiste „Fehlgeschlagen“ (nicht „Fertig“), Esc schließt, kein Wiedererscheinen | UI-019, CRAWL-128 | Chromium grün, WebKit grün |
| E2E (de): gesperrte Start-URL → Dialog mit robots.txt-URL, „Zeile 7“, `Disallow: /secret/`; Statusleiste „Fertig“; Button „robots.txt ignorieren“ führt zur Einstellung, die unverändert „respektieren“ bleibt | UI-019, CRAWL-128 | Chromium grün, WebKit grün |
| E2E (de): Dialog höchstens einmal – nicht nach „URLs hinzufügen“, nicht nach Neustart der Bridge mit demselben Home + Crawl öffnen + „URLs hinzufügen“ (Zähler steigt, Phase „Fertig“, 2 s ohne Dialog) | UI-019, CRAWL-131 | Chromium grün, WebKit grün |
| E2E (de): zweiter Crawl mit derselben gesperrten Start-URL (Rückfrage „Ersetzen“ bestätigt) → Dialog wieder, einmal | UI-019, CRAWL-128 | Chromium grün, WebKit grün |

Vitest: nicht ergänzt. Die Übersetzungen und Dialoglogik laufen vollständig in den Playwright-Tests gegen den echten Kern; ein jsdom-Test
würde dieselben Aussagen mit Mock-Daten wiederholen.

## Produktfehler

**1. Fehler-Mail der Kommandozeile und des Planers nennt im Standardmodus keine Fehlerklasse.** Auftrag: „mit Ursache, Fehlerklasse und
Adresse“. Mit „robots.txt beachten“ (Standard) und nicht belegtem Port lautet die Fehlermeldung (und damit der Mail-Inhalt, Klartext und HTML):
`Start-URL http://127.0.0.1:<port>/ nicht erreichbar: Blocked by Robots.txt (robots.txt unreachable)`. Die eigentliche Klasse (Verbindung
abgelehnt) fehlt; die App-Mail nennt sie (`connection_refused`), die CLI/Planer-Mail mit „robots.txt ignorieren“ ebenfalls („Connection
Refused“). Die Meldung ist zudem Englisch in einem sonst deutschen Satz. Reproduktion: Mail-Konto in den Einstellungen, `openspider --crawl
http://127.0.0.1:<zu>/ --set robots.retries=0`. Tests: `cli_failure_mail_names_the_error_class_with_robots_respected`,
`planner_failure_mail_names_the_error_class_with_robots_respected`.

**2. Mail bei Exit 3 (`--fail-on-start-error`) ohne Adresse und ohne Kopfdaten.** Die Fehler-Mail enthält nur `Fehler: Start-URL nicht erreichbar
(Blocked by Robots.txt (robots.txt unreachable)) – Abbruch wegen --fail-on-start-error`; `Start-URL:`, `Projekt:`, `Start:`, `Ende:` sind leer
bzw. fehlen, die Adresse kommt nirgends vor (im Fall ohne `--fail-on-start-error` steht sie im Text). Test:
`cli_a_failed_crawl_with_exit_3_also_sends_exactly_one_failure_mail`.

**3. Fehler-Mail der Kommandozeile mit `--compare-with-previous` enthält den Vermerk „Auto-Vergleich übersprungen (Crawl fehlgeschlagen)“ nicht.**
OUT-152: „Die Fehler-Mail (OUT-281) enthält statt der Änderungstabelle denselben Vermerk“ (Kriterium steht unter „Auto Compare für Scheduling und
CLI“). Die Warnung steht auf stderr, die globale CLI-Mail (`os-cli/src/mail.rs`) enthält weder Tabelle noch Vermerk. Beim Planer ist es richtig
(grüner Test). Möglicherweise nur Auslegung (siehe Spec-Unklarheiten), Test: `cli_the_failure_mail_of_a_skipped_auto_comparison_carries_the_note_instead_of_the_change_table`.

**4. Warnung der CLI zur Regel „robots.txt nicht erreichbar“ (robots.txt antwortet 500/503/429) ohne robots.txt-URL und ohne Grund.** OUT-182:
Warnung „mit robots.txt-URL und sperrender Zeile (… bei der Regel ‚robots.txt nicht erreichbar‘ mit dem Grund)“. Ausgabe:
`Warnung: Start-URL http://127.0.0.1:<port>/ durch robots.txt gesperrt: Blocked by Robots.txt (robots.txt unreachable)`. Weder die URL
`…/robots.txt` noch der Statuscode (500) kommt vor. Exit 0 und Zustand Completed sind richtig. Test:
`the_warning_for_an_unreachable_robots_txt_names_its_url_and_the_reason` (rot für 500, 503, 429).

Die roten Tests sind nicht abgeschwächt; Fehler 1, 2 und 4 prüfen je eine eigene Eigenschaft, damit ein Fehler die anderen Aussagen nicht verdeckt.

## Beobachtung (kein Test, nicht abgesichert)

In WebKit startete beim zweiten Crawl (Feld bereits mit derselben URL gefüllt, markiert und überschrieben) Enter den Crawl nicht, in Chromium
schon; der Klick auf „Starten“ funktionierte in beiden. Vermutlich öffnet das Kombinationsfeld Start-URL einen Vorschlag, der Enter verbraucht.
Nicht untersucht, nicht Teil des Pakets.

## Spec-Unklarheiten

- OUT-281 nennt für die Fehler-Mail nur „Betreff-Präfix `[FAILED]` und Fehlermeldung“. Dass sie Fehlerklasse und Adresse trägt, stammt aus dem
  Auftrag, nicht aus der Spec (Fehler 1 und 2 beruhen darauf). Die Spec sollte den Mindestinhalt festlegen.
- OUT-152/OUT-281: Ob die globale CLI-Mail (ohne Aufgabe) überhaupt eine Änderungstabelle bzw. „kein vorheriger Crawl“ tragen soll, ist offen;
  im Erfolgsfall enthält sie beides nicht. Fehler 3 gilt nur, wenn der Vermerk-Satz auch für die CLI-Mail gilt.
- OUT-182: „mit dem Grund“ bei der Regel „robots.txt nicht erreichbar“ ist nicht definiert (Statuscode? Fehlerklasse?). Der Test prüft den Statuscode.
- UI-019: Der deutsche Wortlaut des Dialogs ist nicht festgelegt; geprüft sind robots.txt, „sperr…“, URL, „Zeile 7“, Regeltext. Der Auftrag nennt
  „durch robots.txt gesperrt“, der Dialog sagt „robots.txt sperrt die Startadresse.“.
- OUT-169: Ergebnis des Planerlaufs bei Sperre per robots.txt: `Completed` oder `Completed with warnings` ist offen (der Lauf hat eine Warnung); geprüft ist nur „nicht Failed“.
- OUT-152 mit `--strict`: Zählt der Vermerk „übersprungen“ als Warnung (Exit 4)? Nicht getestet.
- UI-019 „2 s nach dem Timeout“: gemessen als Zeit vom Schließen der letzten Verbindung durch den Client bis zur Meldung in der Abfrage der App.
- Ein zweiter Crawl in derselben Sitzung fragt „Bisherige Ansicht ersetzen?“ (nicht in der Spec geregelt); der Test bestätigt sie.
- Nicht abgedeckt: native Tauri-Fenster (WKWebView/WebView2), Mail über das `--run-task`-Binary (der Prozess-Schlüsselbund ist im Test leer und
  die Aufgabenprüfung OUT-168 verlangt dort das Konto-Kennwort; daher läuft die Planer-Mail im Prozess), echte Zeitüberschreitung ohne Wiederholungen der Start-URL.

Branch `m3/t-startfix` (Basis `m3/startfix` d173923d), Tests und Bericht in zwei Commits `test:`; nicht gepusht.
