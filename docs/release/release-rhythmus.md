# Wöchentlicher Release-Rhythmus

Beschlossen am 04.11.2026 (Nutzerwunsch, Termine vom Nutzer bestätigt): Ab der Woche vom 09.11.2026 erscheinen Updates gesammelt einmal pro Woche am **Donnerstag** statt einzeln zwischendurch. Veröffentlicht wird weiterhin nur nach ausdrücklichem Go des Nutzers (siehe `checkliste-release.md`).

## Wochenablauf

| Tag | Was passiert |
|---|---|
| Freitag – Dienstag | Pakete werden gebaut, von Fable abgenommen und nach `main` gemergt. `main` bleibt jederzeit veröffentlichbar; halbfertige Funktionen bleiben im Paket-Branch. |
| **Dienstag 18:00** | **Merge-Schluss.** Was danach fertig wird, geht in die Woche darauf. Ausnahme: Korrekturen am Release-Kandidaten selbst. |
| Mittwoch Vormittag | Release-Kandidat: Notes als `docs/release/notes/<Version>.md` festlegen (einzige Quelle für Update-Dialog und Release-Text; darf als reiner Doku-Commit vorab auf `main`), auf dem Release-Branch `set-version`, CHANGELOG-Eintrag mit `notes.mjs --changelog <Version> --date <JJJJ-MM-TT>`, `notes.mjs --check <Version>`. Die Notes landen beim Bauen in `latest.json` und lassen sich danach nicht mehr ändern; ohne gültiges Notes-Dokument bricht der Release-Workflow vor dem Bauen ab. Release-Workflow erzeugt den Entwurf. |
| Mittwoch | Prüfungen am Entwurf: Update von der Vorwoche auf den Kandidaten (Mac mini, Windows), Installations-Smoke, kurzer 100k-Benchmark gegen die Vorwoche, Website-Änderungen der Woche mit `check-site` vorbereitet. |
| Mittwoch Abend | Zusammenfassung an den Nutzer: Inhalt, Testergebnisse, Text der Release-Notes, Entwurfshinweise aus der Actions-Zusammenfassung, offene Risiken. |
| **Donnerstag 10:00** | Nach Go: Entwurf als Latest veröffentlichen, Download-Links und Manifeste prüfen, Website (Handbuch, Funktionsseiten) zur neuen Version deployen, Ankündigung in GitHub Discussions (eigenes Go). |
| Donnerstag – Freitag | Absturzberichte, Issues und Discussions beobachten; Freitag ist Puffer für einen Hotfix. Kein Release am Wochenende. |

## Sprache der Release-Notes

Nutzerentscheidung 05.11.2026, ergänzt 06.11.2026: Die Notes (`docs/release/notes/<Version>.md`) sind auf Englisch, danach folgt unter `### Deutsch` dieselbe Liste vollständig auf Deutsch (nicht gekürzt; der Nutzer verwendet sie für Ankündigungen, z. B. LinkedIn). Erlaubt sind höchstens Überschriften der Ebene 3 (`notes.mjs --check`). Der Download-Kasten des Release-Texts ist unverändert zweisprachig, die Überschrift davor legt OUT-261 fest.

### Kurze Update-Notes (ab 1.0.0-beta.4)

Nutzerentscheidung 09.11.2026 (OUT-251, B4-UPDDLG): Der Update-Dialog der App zeigt nicht die ausführlichen Notes, sondern einen kurzen Abschnitt am Anfang derselben Datei. Vorlage: `docs/release/notes-template.md` nach `docs/release/notes/<Version>.md` kopieren und ausfüllen.

```markdown
<!-- update -->
- Short point in English (one line)
- …

### Deutsch

- Kurzer Punkt auf Deutsch (eine Zeile)
- …
<!-- /update -->

<ausführliche Notes wie bisher: englische Liste, darunter ### Deutsch mit der vollständigen deutschen Liste>
```

Regeln (`notes.mjs --check` bricht sonst vor dem Bauen ab):

- Die Zeilen `<!-- update -->` und `<!-- /update -->` stehen allein auf ihrer Zeile, genau einmal je Datei, am besten ganz oben.
- Dazwischen: englische Liste, eine Zeile `### Deutsch`, deutsche Liste. Je Sprache 3 bis 6 Punkte, jeder Punkt eine Zeile „- …“ mit höchstens 160 Zeichen. Fettdruck und Links (`[Text](https://…)`) sind erlaubt; Links öffnen im Standardbrowser.
- Nur dieser Abschnitt landet als `notes` in `latest.json`/`latest-beta.json` (`notes.mjs <Version> <Update-Notes>` schreibt ihn in die Datei, die `manifest.mjs` liest). Release-Text (OUT-261) und CHANGELOG-Eintrag bestehen aus dem Rest der Datei, den ausführlichen Notes; `notes.mjs --changelog` übernimmt den kurzen Abschnitt nicht.
- Fehlt der Abschnitt ab 1.0.0-beta.4, ist der Release-Lauf rot. Bis 1.0.0-beta.3 (bereits veröffentlicht) bleiben die Dateien unverändert; ihr Manifest trug die ausführlichen Notes.
- Die App wählt die Sprache wie beim Neuigkeiten-Dialog (OUT-257): deutsche Oberfläche → deutscher Teil, jede andere → englischer Teil.

## Versionen

- Eine Beta pro Woche bis 1.0. Ausnahme (Nutzerentscheidungen 06.11. und 08.11.2026): beta.3 einmalig am Montag, 09.11.2026 – ursprünglich für Freitag, 06.11., geplant, verschoben für den Fix der Crawl-Datei-Beschädigung und ergänzt um B4-LIVEDETAIL; danach beta.4 am 12.11.2026, beta.5 am 19.11.2026 usw. im normalen Rhythmus.
- Fällt eine Woche ohne nennenswerte Änderungen aus, entfällt das Release; der Rhythmus bleibt.

## Hotfixes außerhalb des Rhythmus

Nur bei Absturz beim Start oder Crawl, Datenverlust in Crawl-Dateien, Sicherheitslücke oder kaputtem Updater/Installer. Alles andere wartet bis Donnerstag.

**Versionsschema (Nutzerentscheidung 05.11.2026):** Ein Hotfix bekommt die nächste Beta-Nummer (z. B. beta.4 als Hotfix nach beta.3), das folgende Donnerstags-Release die übernächste. Hintergrund (B3-REL, `docs/plan/M3-reports/b3-rel.md`, Abschnitt Hotfix-Schema): `1.0.0-beta.3.1` ordnet der Update-Kern zwar richtig ein, aber `set-version`, die Release-Skripte und die MSI-Versionsabbildung lehnen das Schema ab, und die MSI-Abbildung hat zwischen beta.3 und beta.4 keinen freien Wert.

## Website

Website-Änderungen, die neue Funktionen beschreiben, gehen mit dem Release am Donnerstag live. Reine Text- oder Fehlerkorrekturen dürfen jederzeit raus.

## Voraussetzungen (aus `docs/plan/beta3.md` §5/§6, umgesetzt mit B3-REL)

- Pfadfilter in der CI: Commits, die nur Markdown, `docs/` (außer `docs/spec`, `docs/issues`, `docs/plan/M2.md`, `docs/plan/*.json`/`*.toml`), `website/` oder `learnings/` ändern, starten `ci.yml`, `linux-core.yml`, `parity.yml` und `licenses.yml` nicht. Website-Commits prüft der leichte Lauf `website.yml` (`check-site`, PHP-Tests), Notes-/CHANGELOG-Commits der leichte Lauf `release-scripts.yml`. Keine Pflicht-Checks auf `main` (Stand 05.11.2026), daher bleibt nichts hängen.
- Release-Notes stehen vor dem Bauen fest (Mittwoch Vormittag, `docs/release/notes/<Version>.md`).
- Entwurfshinweise nur in die Actions-Zusammenfassung, nicht in den Release-Text.
- Wiederholung mit Backoff (3 Versuche) und SHA-256-gepinnter Zwischenspeicher in `scripts/release/licenses/generate.mjs`, damit der Mittwochslauf nicht an einem Netz-Timeout hängt.
