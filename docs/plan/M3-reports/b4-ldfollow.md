# B4-LDFOLLOW – Folgepunkte zur Live-Detailprüfung

Branch `m3/ldfollow`, Basis `6d879a261` (origin/main, enthält Merge `25673f6c` von B4-LIVEDETAIL), Linux-Build-Server. Clean-Room: Soll nur aus `docs/spec/*`; keine Referenzproduktprüfung. Tokio bleibt 1.53.3; kein `cargo update`, keine neuen Bibliotheken, kein Push. Die zusätzliche Tauri-Testfunktion verwendet die bereits vorhandene Abhängigkeit.

**Status: PAKET OFFEN – alle sechs Änderungen implementiert, verbleibendes WebKit-Abnahme-Gate nicht grün.** Nächster Schritt: Im parallel bearbeiteten Abnahmepaket den Teststimulus von `livedetail-detail.spec.ts:198` prüfen und anschließend das unveränderte bzw. freigegebene Abnahme-Gate erneut ausführen. Der Endlauf erreicht vor der fachlichen Prüfung 834 ms statt <800 ms für acht simulierte Tastenereignisse. Ob Browser-Scheduling, parallele Serverlast oder UI-Laufzeit die Verzögerung verursacht, ist hier nicht abschließend belegt; keine Grenze wurde gelockert.

## Änderungen und Nachweise

| Punkt | Ursache → Änderung | Fixture / Test | Commit |
|---|---|---|---|
| 1: Detailbereiche | `expected` bezeichnet bei Details eine logische View-Version, keine Datenbankgeneration. Getrennte OFFSET-Seiten konnten deshalb unterschiedliche Datenstände mischen. Jeder Bereichslauf erhält jetzt einen eigenen `detail_seq`-Pin: erste Seite pinnt die Datenbankgeneration, Folgeseiten prüfen sie innerhalb ihrer Lesetransaktion. Bei Wechsel `Stale`, vorhandene Wiederholung startet mit frischem Pin. | `commands::view::ldfollow_tests` (`tests/ldfollow/pages.rs`): fx-ui-basic mit 1.100 Headerzeilen ohne Linkliste; deterministisches Löschen zwischen den echten Bereichsseiten. Direkter Lauf liefert `Stale`; öffentlicher `view.range` wiederholt nach weiterem Schreibzugriff und liefert genau 1.098 Kennungen ohne Lücke/Dublette. | `eb2ee1b37`, `1dc713a91`, `2d5f8aa01`, `7ab9a3c01`, `d4497be09` |
| 2: Große Teilbereiche | Die bisherige 20.000er-Grenze galt nur bei vollständiger Überdeckung. Jetzt höchstens 20.000 Kennungen je Antwort. Größere Teilbereiche liefern `exportRequired: true`, keine Kennungen und keine „Alle“-Auswahl; die Oberfläche zeigt einen übersetzten Hinweis auf den vorhandenen Tabellenexport der aktuellen Ansicht. Große vollständige Views behalten `all`. Grenze wie Client-Zeilencache/UI-144: begrenzte Serialisierung, Übertragung und Auswahlmenge statt IPC-Antworten in Millionenhöhe. | `b4_ldfollow_range`: synthetische View mit >30.000 Zeilen, Grenzen 20.000/20.001, umgekehrter Bereich, vollständige Überdeckung. `manifest_contract` prüft Ergebnisfeld und generierten Typ; Grid-Vitest prüft Hinweis ohne irrtümliche „Alle“-Auswahl. | `a2b7d1a84` |
| 3: „Alle“ bei Versionswechsel | Eine jüngere Version während der Antwort ließ die Auswahl still verschwinden. Nach Adoption und Settle bindet sie sich jetzt an `source.version`; neuere Auswahlgesten behalten Vorrang. | `DataGrid.test.tsx`: Antwort aus Version 2 wird von Version 3 überholt; anschließend „Alle“ in Version 3. | `f9f35c39c` |
| 4: Aktualisierungsfehler | Der 500-ms-Refresh setzte Fehler, löschte sie aber nach erfolgreicher Adoption nicht. Erfolg löscht jetzt den Fehler. `DetailError` zeigt übersetzten `messageKey`-Text, Schlüssel und vorhandene technische Details als React-Text; unbekannte Schlüssel erhalten den Fehlerklassen-Fallback. `err-io` für en/de ergänzt. | `detail.test.tsx`: erster Refresh scheitert, zweiter gelingt ohne neue Revision, Alert verschwindet. `Feedback.test.tsx`: Io, Bridge-Schlüssel, unbekannter Schlüssel, Diagnosedetail, en/de. | `977daabc0`, `782644f53` |
| 5: Endpunktdokumentation | „URL“ war bei materialisierten Linklisten falsch. `from`/`to` sind Zeilenkennungen, bei Linklisten Link-IDs; Antwortdokumentation entsprechend präzisiert. | Typen mit `os-app-manifest` neu erzeugt; Linklisten-Nachweis aus `b4_view_range` bleibt unverändert. | `a69c191ee` |
| 6: Speichern beim Schließen | Die Einsekunden-Timer überlebten Panelwechsel, aber keinen Prozessabbruch. `flushDetailState` startet die jüngsten Patches sofort, wartet auf laufende Speicherungen und hält fehlgeschlagene Änderungen zur Wiederholung vor. `beforeunload`/`pagehide`, Menü und Bestätigung des Beendens rufen den Flush auf. Der native Schließ-/OS-/Menüpfad wartet vor dem Quit auf eine adressierte Frontend-Bestätigung (max. 3 s); Fehler/fehlende Antwort lassen die App offen. Gleichzeitige native Quit-Anfragen werden zusammengefasst. | `persistence.test.ts`: letzte Änderung, kein doppelter Timer-Save, beide Browserereignisse, native Bestätigung erst nach Speicherung, Fehler und Wiederholung, bereits laufende Speicherung. Tauri-Mockfixture prüft Erfolg, Ablehnung und fehlende Antwort des nativen Flushs. | `19c2454c0`, `782644f53` |

`commands.json` wurde durch `os-app-manifest` neu erzeugt und ist bytegleich: Name, Argument- und Rückgabetyp bleiben `view.range: RangeArgs → KeyRange`. Der erweiterte Vertrag steht im generierten `KeyRange.ts`, im Vertragstest und in `M2-ui-concept.md` 4.4. Keine bestehende Erwartung und kein Budget wurden gelockert; `// acceptance: independent`-Tests bleiben unverändert. Bestehende Antwort-Doubles erhielten nur das zusätzliche Pflichtfeld.

## Fremde Dateien

- `crates/os-app/src/views/mod.rs`, `commands/compare.rs`, `stub/mod.rs`: Antwortgrenze in allen Implementierungen desselben IPC-Vertrags einhalten.
- `crates/os-app/src/lib.rs`: nur unter `cfg(test)` ein Eigenalias, damit private Tests dieselbe fx-ui-basic-Integrationfixture wiederverwenden können.
- `apps/desktop/src/data/ViewDataSource.test.ts`: bestehendes Antwortdouble um das zusätzliche Vertragsfeld ergänzt, Erwartungen unverändert.
- `apps/desktop/src/app/runtime.ts`, `features/shell/QuitDialog.tsx`, `src-tauri/src/window.rs`: den Flush in echte Start-/Schließpfade integrieren; `src-tauri/Cargo.toml`: vorhandenes Tauri-Testfeature ausschließlich als Dev-Abhängigkeit für den nativen Nachweis.
- Fehlertexte liegen im aktuellen Stand unter `apps/desktop/src/i18n/errors/i18n/{en,de}.ftl`, nicht mehr direkt unter `src/i18n/`.

Die parallelen Dateien `apps/desktop/e2e/acceptance/livedetail-*` wurden nicht geändert.

## Spec-Fragen und Grenzen

- S7 (unsortierte Detailreihenfolge), S9 (Tail-Wiederaufnahme per Ende/Pfeil) und `links_target`-Cache bleiben ausdrücklich unverändert; die Nutzerentscheidungen stehen aus.
- Im Fehlerkatalog existieren nur en/de-Ressourcen. Eine leere Fluent-Nachricht ist kein gültiger Platzhalter für einen Wert; es/fr/it-Dateien mit erfundenem Text oder leerem Schlüssel wurden nicht angelegt. `err-io` und der neue Export-Hinweis fallen für diese Sprachen auf Englisch zurück; Übersetzung bleibt B4-LANG.
- Native kontrollierte Beendigung wartet auf das Speichern. Browser können asynchrone IPC beim endgültigen Verlassen abbrechen; `beforeunload`/`pagehide` starten den Flush sofort, garantieren aber keine Webbrowser-Durabilität nach Zerstörung des Dokuments. Kein zusätzlicher Bestätigungsdialog eingeführt.
- Unveränderlicher Abnahmebefund: `livedetail-dialogs.spec.ts:254` verlangt nach Rückkehr zu „All links“ weiteres Wachstum über den soeben gelesenen Wert. Im ersten Chromium-Lauf war dieser Wert bereits 21 (20 finale Fixture-Inlinks + Kopfzeile); verlangt wurde `> 21`, geliefert korrekt 21. Das widerspricht ab diesem Zeitpunkt dem Fixture-Endstand, nicht der Tabellenaktualität. Kein Testeingriff; unveränderte Dialog-Spec anschließend 14/14 in beiden Engines grün. Erster Gesamtlauf 75/76; zweiter Gesamtlauf 72/76 (alle vier Fehler in WebKit). Der letzte Gesamtlauf nach Ende unseres Workspace-Laufs erreicht 75/76; der Wachstumstest ist darin in beiden Engines grün. Andere Pakete arbeiteten weiterhin auf dem Server. Trace und Fehlerkontext liegen außerhalb des Repos unter `/tmp/b4-ldfollow-evidence/`.
- Weitere unveränderte WebKit-Abnahmebefunde im zweiten Gesamtlauf: `livedetail-detail.spec.ts:142` verlangt für page-1 exakt eine Inlink-Zeile, obwohl nach Entdeckung von `/redirect` korrekt zwei vorliegen (derselbe Test akzeptiert später ausdrücklich 1–2 Inlinks). `:198` erreicht die Teststimulus-Vorgabe nicht: acht auf 5 ms angesetzte Tastenereignisse liegen 949 ms auseinander statt <800 ms. `:305` hat acht Recorder-Samples statt der verlangten >10, weil bis zum Zählerziel nur so viele Proben entstanden. Keine Änderung an Tests, Taktvorgaben oder Sample-Grenzen. Diese Zeit-/Fixture-Vorbedingungen sind vom Nachweis falscher/veralteter Inhalte zu unterscheiden; Traces liegen ebenfalls im externen Evidenzordner.
- UI-145 wurde hier gegen den Entwicklungsserver als bestehende Regression geprüft; kein Nachweis der Release-Budgets auf Referenzhardware/macOS.
- Der Export-Hinweis wählt nicht still eine andere Menge aus: die bestehende Auswahl bleibt erhalten, der Tabellenexport exportiert die aktuelle Ansicht. Eine neue serialisierte „Bereich“-Auswahl mit eigener Exportsemantik wurde nicht eingeführt (erlaubte Hinweisvariante des Auftrags).
- Die Antwortmenge ist begrenzt; nicht materialisierte Detailbereiche müssen weiterhin ihre Quelle seitenweise durchsuchen. Dieses Paket baut keinen neuen serverseitigen Index und ändert keine Reihenfolgen.

## Abschluss-Gates

| Gate auf dem Endstand | Ergebnis |
|---|---|
| `cargo fmt --all --check` | grün |
| `cargo clippy --workspace --all-targets -j 4 --locked -- -D warnings` | grün |
| Workspace-Test, einschließlich Desktop und Doc-Tests | **4.414 bestanden, 0 Fehler, 51 ignoriert** |
| `npm ci` | grün |
| Desktop-Typecheck | grün |
| Desktop-Vitest-Suite | **1.031 bestanden in 121 Dateien** |
| zusätzliche statische Prüfung | **30 Tests bestanden**, 29 Wrapper aktuell, 476 npm-Pakete unter der Lizenz-Positivliste |
| `os-app-manifest --check` | grün; `commands.json` bytegleich |
| `bash scripts/check-neutral.sh` | grün, keine Fundstellen |
| betroffene Playwright-Specs, letzter vollständiger Lauf | **75/76**: Chromium **38/38**, WebKit **37/38**; offen: Teststimulus der schnellen Pfeiltasten **834 ms**, verlangt **<800 ms** |
| unveränderte Dialog-Spec, gezielte Wiederholung | **14/14**, je 7 in Chromium und WebKit |

Playwright-Satz: `e2e/acceptance/livedetail`, `e2e/beta/shift-range-live.spec.ts`, `e2e/beta/detail-during-crawl.spec.ts`, `e2e/detail/`; beide Projekte, `OS_E2E_PORT=14687`, `--workers=2`, kein Retry-/Budget-Eingriff. Drei vollständige Durchgänge: 75/76, 72/76, 75/76; Fehlerkontexte oben. Die 14/14-Dialog-Wiederholung ersetzt keinen vollständigen grünen Lauf. Letzter Fehlerkontext und Trace liegen unter `/tmp/b4-ldfollow-evidence/final/`; frühere Nachweise in den benachbarten Unterordnern.

Die anfänglichen Fehlversuche wurden behoben und Prüfungen wiederholt: privater Test brauchte den Eigenalias zur bestehenden Integrationfixture; native Mockfixture brauchte eine geliehene Ereigniskennung; Clippy verlangte die ausschließlich im neuen Fixturetest geltenden SQL-Ausnahmen und `expect_err`. Ein Zwischenlauf beanstandete zu Recht deren Ort unter `src/`; der private Test liegt deshalb jetzt unter `tests/ldfollow/pages.rs`. Die drei unveränderten Architekturtests sind grün; im Produktionsbaum tragen weiterhin nur Pool und WriteQueue die Ausnahme. Der erste Workspace-Lauf scheiterte an zwei unveränderten Accessibility-Tests wegen fehlendem `OPENSPIDER_CHROMIUM`; der Endlauf verwendet den vorhandenen Playwright-Browser und `OPENSPIDER_SECRET_STORE=memory`. Zur Einhaltung der Freiplatzbedingung von `ws-slot.sh` wurden nach Ende der fehlgeschlagenen Läufe zweimal jeweils rund 30 GiB erzeugte Testprogramme entfernt und im Endlauf bei Bedarf neu gebaut; keine laufenden Produktwerkzeuge, Quellen oder fremden Worktrees angefasst. Playwright ausschließlich mit `OS_E2E_PORT=14687`, zwei Workern; Lastprozesse unter Timeout. Workspace-Test mit `~/ws-slot.sh`, Debug-Informationen/Incremental aus, Cargo `-j 4 --locked`.


Kein Push. `target/`, `node_modules`, `apps/desktop/node_modules` und erzeugte Playwright-Artefakte sind entfernt. Abschließend: Arbeitsbaum sauber, ausschließlich eigene Paketpfade committed.
