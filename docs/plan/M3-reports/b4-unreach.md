# B4-UNREACH – Dialog bei nicht erreichbarer Start-URL – Bericht

Stand 09.11.2026. Branch `m3/unreach`, Mac-Worktree `openspider-wt/c-unreach`, Server-Worktree `m3-unreach` (per `srv-sync.sh`
gespiegelt). Grundlage: UI-019, UI-126, UI-121 (05), CRAWL-128 (01), OUT-182 (06).

**Basis:** `main` 1a09dbe3 (Merge B4-RESUME), nicht `origin/main`. `origin/main` stand bei bb0b27e1 und enthielt B4-RESUME noch
nicht; ohne den Zustand „Fehlgeschlagen“ für die nicht erreichbare Start-URL gäbe es nichts anzuzeigen. Der Server-Worktree
wurde wie vorgegeben ab `origin/main` angelegt und per `srv-sync.sh` auf den Stand des Mac-Worktrees gebracht.

## Ausgangslage

- Seit B4-RESUME endet ein Spider-Crawl, dessen einzige abgerufene Seite die Start-URL ohne Antwort ist, als `Failed`
  (CRAWL-128). Die Engine liefert dafür `Ok(summary)` mit `state = Failed`.
- `os-app` `session::control::settle` sendete nur bei `Err` eine Meldung (`crawl-failed`). Für `Ok` + `Failed` gab es
  keine Meldung; die Oberfläche zeigte nur die Phase in der Statusleiste.
- Die Oberfläche hatte bereits Texte und einen Dialog für `start-dns`, `start-connection` und `start-tls`
  (`features/notifications`), aber der Kern sendete diese Codes nie.

## Umsetzung

### Kern

- `os-engine` (`crawl.rs`, Export in `lib.rs`): Die Prüfung aus B4-RESUME ist jetzt die öffentliche Funktion
  `start_failure(&CrawlFile, start_url) -> Option<StartFailure { reason, url, robots }>`. Sie liest den Grund der
  Start-URL-Zeile. Sperrt die Regel „robots.txt nicht erreichbar“ die Start-URL, liest sie den Grund der
  robots.txt-Hilfszeile. Es zählen nur Netzfehler (DNS, Verbindung verweigert, Verbindungs- und Antwort-Timeout,
  Verbindungsfehler, TLS). Die Engine-Entscheidung `start_url_unreachable` ruft dieselbe Funktion auf. Das Verhalten ist
  unverändert, die B4-RESUME-Tests bleiben grün.
- `os-app` (`session/control.rs`): Endet der Lauf mit `Ok` und `Failed`, liest `notify_start_unreachable` die Ursache über
  den Lese-Pool aus der Datei und sendet eine Fehlermeldung:
  - Code nach Fehlerklasse: `start-dns` (dns_lookup_failed), `start-connection` (connection_refused, connection_error),
    `start-timeout` (connection_timeout, response_timeout), `start-tls` (tls_error).
  - Argumente: `startUrl`, `url` (die gescheiterte URL: Start-URL oder robots.txt), `reason` (Fehlerklasse unverändert
    aus der Zeile), `robots` (bool).
  - Ist keine Ursache lesbar, sendet die App die allgemeine Meldung `crawl-failed`. Ein stilles Ende gibt es damit nicht.
  - `start_unreachable_notice` ist öffentlich, damit die Zuordnung testbar ist.

### Oberfläche (`features/notifications`)

- Meldungen mit diesen Codes **und** einer Ursache `reason` vom Kern zeigen den Dialog „Startadresse nicht erreichbar“
  (Titel ist der zugängliche Name). Inhalt:
  - Haupttext je Klasse; `notice-start-timeout` ist neu.
  - „Ursache: <Fehlerklasse>“ mit übersetzter Klasse (`notify-reason-<reason>`). Ist eine Klasse unbekannt, steht der
    Kernwert da, kein geratener Text.
  - Die gescheiterte Adresse. Für die robots.txt steht ausdrücklich, dass die robots.txt nicht abrufbar war und die
    Startadresse deshalb nicht gecrawlt wurde.
  - Der Hinweis, dass der Crawl als „Fehlgeschlagen“ endete (kein stilles „0 URLs“).
- Handlungsoptionen bleiben die des bestehenden Fehlerdialogs: „Log öffnen“, „Diagnose kopieren“, „Schließen“. Bei einer
  HTTPS-Start-URL mit DNS- oder Verbindungsfehler gibt es zusätzlich „HTTP-Adresse übernehmen“. Das trägt nur die Adresse
  ein, der Nutzer startet selbst. Siehe Spec-Frage S1.
- Tastatur und Fokus (UI-126) über das bestehende `Dialog`-Primitiv: Der Fokus geht in den Dialog, Tab bleibt in der
  Fokusfalle, der Fokusrahmen ist sichtbar, Esc schließt, und der Fokus kehrt zum Auslöser zurück (beim Start per Enter
  das Feld „Start-URL“).
- Sprachrückfall: `noticeMessage` prüft einen Schlüssel jetzt in der aktiven Sprache **oder** in Englisch. Vorher fiel
  eine Sprache ohne `notice-…`-Schlüssel (es/fr/it vor B4-LANG) auf die Rohanzeige „Ursache: start-timeout“ zurück. Die
  übrigen neuen Schlüssel fallen über `translate` auf Englisch zurück.
- Neue Texte nur in `en.ftl`/`de.ftl` (13 Schlüssel, darunter `notice-crawl-failed` für die bisher unübersetzte
  allgemeine Meldung).

## Kriterium → Test → Ergebnis

| Kriterium | Test | Ergebnis |
|---|---|---|
| UI-019: Start-URL nicht erreichbar → Dialog mit Ursache (Fehlerklasse); geschlossener Port | `crates/os-app/tests/b4_unreach.rs` `a_closed_port_reports_the_refused_connection_of_the_start_url_within_two_seconds` (robots.txt ignoriert, Ursache aus der Start-URL-Zeile) | grün: `start-connection`, `connection_refused`, `robots=false`; Phase `Failed` im Tick |
| UI-019 „innerhalb von 2 s“ | dieselbe Datei: alle Fälle prüfen „Ende des Abrufs → Meldung < 2 s“; geschlossener Port zusätzlich „Start → Meldung < 2 s“ | grün; gemessen 20–250 ms nach dem Abrufende |
| UI-019 „2 s nach dem Timeout“ | `a_timeout_is_reported_within_two_seconds_after_the_timeout` (Port nimmt an, antwortet nie, Antwort-Timeout 1 s) | grün: `start-timeout`; letzter Timeout bei 3,7 s (der Abrufer wiederholt), Meldung 35 ms danach |
| CRAWL-128: unbekannte Domain | `an_unknown_domain_reports_the_failed_dns_lookup` (`.invalid`, robots.txt beachtet und ignoriert) | grün: `start-dns`, `dns_lookup_failed`, URL = robots.txt bzw. Start-URL |
| CRAWL-128: robots.txt per Netzfehler nicht erreichbar | `an_unreachable_robots_txt_reports_its_own_row_and_url` | grün: Ursache aus der Hilfszeile, `robots=true`, `url=…/robots.txt` |
| CRAWL-128: robots.txt mit 500 ist kein Fehlschlag | `a_robots_txt_answering_500_is_no_failure_and_shows_no_start_dialog` (os-testsite-Szenario) | grün: `Completed`, keine Startfehler-Meldung |
| Zuordnung aller sechs Netzklassen | `every_network_class_maps_to_its_dialog_code` | grün |
| Engine-Verhalten unverändert | `os-engine` `crawl::unreachable_tests`, `tests/b4_resume.rs` (8) | grün |
| UI-019 Darstellung je Fall (Fixture `features/notifications/fixtures/start-unreachable.json`) | `StartUnreachable.test.tsx` „names the cause … (closedPort, unknownDomain, robotsNetwork, timeout, tls)“ | grün |
| Deutsche Texte | „shows the German cause and state“ | grün |
| UI-126 Fokus, Esc, Fokusrückgabe | „moves focus into the dialog, closes with Escape and returns focus …“ | grün |
| Handlungsoptionen | „offers the HTTP address only for an HTTPS start URL and never starts a crawl itself“ | grün |
| Ohne Kern-Ursache bleibt der bisherige Dialog | „keeps the generic dialog for start codes without a core cause“, bestehende `Notifications.test.tsx` (12) | grün |
| UI-121 Englisch als Rückfall (es/fr/it) | „falls back to English for a language without these texts“ (Katalog mit `fr`-Datei ohne diese Schlüssel) | grün |
| Ende-zu-Ende, beide Engines: Start per Enter, Dialog, Ursache, Statusleiste „Failed“, Tab-Fokusfalle, sichtbarer Fokus, axe (critical/serious) im Dialog, Esc, Fokus zurück im Feld | `apps/desktop/e2e/beta/unreach.spec.ts` (robots.txt beachtet und ignoriert) × Chromium, WebKit | grün, 4/4 |

## Gates (Server, Worktree `m3-unreach`)

Stand der Commits f260374b…9e014c3b (Code unverändert seitdem; dieser Bericht ist der letzte Commit).

| Gate | Ergebnis |
|---|---|
| `cargo fmt --all --check` | grün |
| `cargo clippy --workspace --all-targets -- -D warnings` | grün |
| `cargo test --workspace` (nach `~/ws-slot.sh`, `CARGO_PROFILE_*_DEBUG=0`, `CARGO_INCREMENTAL=0`) | grün: 617 Testbinaries, 4479 bestanden, 0 fehlgeschlagen, 51 ignoriert (darin `b4_unreach` 6/6) |
| `cargo test -p os-app --test b4_unreach` | 6/6 grün |
| `cargo test -p os-engine --test b4_resume`, `--lib unreachable` | 8/8, 3/3 grün |
| `npm run -w apps/desktop typecheck` | grün |
| `npm run -w apps/desktop test` (CSS-Lint + Vitest) | 123 Dateien, 1048 Tests grün (darin neu `StartUnreachable.test.tsx` 10) |
| Playwright Chromium + WebKit: `e2e/beta/unreach.spec.ts`, betroffen `e2e/header`, `e2e/a11y` (Dialog in der App-Shell) | 48/48 grün (unreach 4/4) |

Hinweis zum Ablauf: Der erste Gate-Lauf brach ab. Die Platte des Servers lief voll (0 GB frei, ENOSPC bei Playwright), und
Tailscale war kurz getrennt. Daraufhin wurde das eigene `target/` (36 GB) gelöscht und die Gates per `nohup` neu
gestartet. Fremde Worktrees wurden nicht angefasst.

## Fremde Dateien (K2-92)

Geändert, obwohl sie nicht aus diesem Paket stammen:

- `crates/os-engine/src/crawl.rs`, `crates/os-engine/src/lib.rs` (B4-RESUME): Die Prüfung `start_url_unreachable` wurde
  in die öffentliche Funktion `start_failure` ausgelagert. Das Verhalten ist unverändert.
- `crates/os-app/src/session/control.rs` (Sitzungs-Lebenszyklus): Meldung in `settle` und zwei neue Funktionen. Am
  bestehenden Ablauf ist nichts geändert.
- `apps/desktop/src/features/notifications/Notifications.tsx` (Zuständigkeit UI-019): Dialog erweitert, Textauswahl mit
  Englisch als Rückfall.

Neu: `crates/os-app/tests/b4_unreach.rs`, `apps/desktop/src/features/notifications/StartUnreachable.test.tsx`,
`apps/desktop/src/features/notifications/fixtures/start-unreachable.json`, `apps/desktop/e2e/beta/unreach.spec.ts`.

K2-63: Nichts wurde gelockert. Keine Prüfung wurde entfernt oder abgeschwächt, keine bestehende Erwartung geändert.

## Spec-Fragen

- **S1 Handlungsoptionen:** UI-019 nennt für die nicht erreichbare Start-URL keine eigenen Optionen. „robots.txt
  ignorieren“ gilt nur für die Sperre per `Disallow`, „Log öffnen“ und „Diagnose kopieren“ nur für einen unerwarteten
  Kernfehler. Umgesetzt sind die bestehenden Optionen des Fehlerdialogs (Log, Diagnose, Schließen, bei HTTPS „HTTP-Adresse
  übernehmen“). Offen ist, ob es „Erneut versuchen“ oder einen Verweis auf Timeout bzw. die robots.txt-Regel
  (`robots.unreachable_policy`) geben soll.
- **S2 „Statuscode bzw. Fehlerklasse“:** Bei einem Netzfehler gibt es keinen Statuscode (die Zeile hat Status 0). Der
  Dialog nennt nur die Fehlerklasse. Ist das so gemeint?
- **S3 Zeitbezug „2 s nach dem Timeout“:** Der Abrufer wiederholt einen Antwort-Timeout (Antwort-Timeout 1 s → Ende des
  Abrufs nach ≈3,3–3,7 s). Gemessen wird ab dem letzten Timeout, also dem Ende des Abrufs. Soll ab dem ersten Timeout
  gemessen werden, bräuchte es eine Entscheidung zu den Wiederholungen der Start-URL.
- **S4 Geltungsbereich:** Der Dialog erscheint am Ende eines Laufs, den diese App besitzt. Er erscheint nicht, wenn ein
  bereits fehlgeschlagener Crawl später geöffnet wird, und nicht für Crawls aus CLI, MCP-STDIO oder Planer. Die Spec
  sagt dazu nichts.
- **S5 Statusleiste:** CRAWL-128 nennt den Zustand „Fehlgeschlagen“. Die deutsche Statusleiste zeigt `status-failed =
  Fehler` (fremde Datei `features/statusbar/i18n/de.ftl`, nicht geändert); die Crawl-Verwaltung zeigt „Fehlgeschlagen“.
  Angleichen?

## Weitere Befunde (nicht in diesem Paket)

- Die übrigen Startfehler-Dialoge aus UI-019 (Start-URL per robots.txt gesperrt, Start-URL leitet auf fremden Host um)
  haben Texte und Tests in der Oberfläche. Der Kern sendet `start-robots` und `start-redirect-host` aber weiterhin nicht.
- `mail::on_failed` läuft nur bei `Err` der Engine. Ein als `Failed` beendeter Lauf mit nicht erreichbarer Start-URL
  löst `CrawlEvent::Completed` aus, nicht den Fehlerweg der Abschlussmail. Ob das so gewollt ist, regelt die Spec der
  Mail (OUT-282), nicht dieses Paket.
