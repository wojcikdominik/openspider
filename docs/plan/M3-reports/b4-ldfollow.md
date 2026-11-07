# B4-LDFOLLOW – Folgepunkte zur Live-Detailprüfung

Branch `m3/ldfollow`, Basis `6d879a261` (origin/main, enthält Merge `25673f6c` von B4-LIVEDETAIL), Linux-Build-Server. Clean-Room: Soll nur aus `docs/spec/*`; keine Referenzproduktprüfung. Tokio bleibt 1.53.3; kein `cargo update`, keine neuen Bibliotheken, kein Push. Die zusätzliche Tauri-Testfunktion verwendet die bereits vorhandene Abhängigkeit.

## Änderungen und Nachweise

| Punkt | Ursache → Änderung | Fixture / Test | Commit |
|---|---|---|---|
| 1: Detailbereiche | `expected` bezeichnet bei Details eine logische View-Version, keine Datenbankgeneration. Getrennte OFFSET-Seiten konnten deshalb unterschiedliche Datenstände mischen. Jeder Bereichslauf erhält jetzt einen eigenen `detail_seq`-Pin: erste Seite pinnt die Datenbankgeneration, Folgeseiten prüfen sie innerhalb ihrer Lesetransaktion. Bei Wechsel `Stale`, vorhandene Wiederholung startet mit frischem Pin. | `commands::view::ldfollow_tests`: fx-ui-basic mit 1.100 Headerzeilen ohne Linkliste; deterministisches Löschen zwischen den echten Bereichsseiten. Direkter Lauf liefert `Stale`; öffentlicher `view.range` wiederholt nach weiterem Schreibzugriff und liefert genau 1.098 Kennungen ohne Lücke/Dublette. | `eb2ee1b37`, `1dc713a91`, `2d5f8aa01` |
| 2: Große Teilbereiche | Die bisherige 20.000er-Grenze galt nur bei vollständiger Überdeckung. Jetzt höchstens 20.000 Kennungen je Antwort. Größere Teilbereiche liefern `exportRequired: true`, keine Kennungen und keine „Alle“-Auswahl; die Oberfläche zeigt einen übersetzten Hinweis auf den vorhandenen Tabellenexport der aktuellen Ansicht. Große vollständige Views behalten `all`. Grenze wie Client-Zeilencache/UI-144: begrenzte Serialisierung, Übertragung und Auswahlmenge statt IPC-Antworten in Millionenhöhe. | `b4_ldfollow_range`: synthetische View mit >30.000 Zeilen, Grenzen 20.000/20.001, umgekehrter Bereich, vollständige Überdeckung. `manifest_contract` prüft Ergebnisfeld und generierten Typ; Grid-Vitest prüft Hinweis ohne irrtümliche „Alle“-Auswahl. | `a2b7d1a84` |
| 3: „Alle“ bei Versionswechsel | Eine jüngere Version während der Antwort ließ die Auswahl still verschwinden. Nach Adoption und Settle bindet sie sich jetzt an `source.version`; neuere Auswahlgesten behalten Vorrang. | `DataGrid.test.tsx`: Antwort aus Version 2 wird von Version 3 überholt; anschließend „Alle“ in Version 3. | `f9f35c39c` |
| 4: Aktualisierungsfehler | Der 500-ms-Refresh setzte Fehler, löschte sie aber nach erfolgreicher Adoption nicht. Erfolg löscht jetzt den Fehler. `DetailError` zeigt übersetzten `messageKey`-Text, Schlüssel und vorhandene technische Details als React-Text; unbekannte Schlüssel erhalten den Fehlerklassen-Fallback. `err-io` für en/de ergänzt. | `detail.test.tsx`: erster Refresh scheitert, zweiter gelingt ohne neue Revision, Alert verschwindet. `Feedback.test.tsx`: Io, Bridge-Schlüssel, unbekannter Schlüssel, Diagnosedetail, en/de. | `977daabc0`, `782644f53` |
| 5: Endpunktdokumentation | „URL“ war bei materialisierten Linklisten falsch. `from`/`to` sind Zeilenkennungen, bei Linklisten Link-IDs; Antwortdokumentation entsprechend präzisiert. | Typen mit `os-app-manifest` neu erzeugt; Linklisten-Nachweis aus `b4_view_range` bleibt unverändert. | `a69c191ee` |
| 6: Speichern beim Schließen | Die Einsekunden-Timer überlebten Panelwechsel, aber keinen Prozessabbruch. `flushDetailState` startet die jüngsten Patches sofort, wartet auf laufende Speicherungen und hält fehlgeschlagene Änderungen zur Wiederholung vor. `beforeunload`/`pagehide`, Menü und Bestätigung des Beendens rufen den Flush auf. Der native Schließ-/OS-/Menüpfad wartet vor dem Quit auf eine adressierte Frontend-Bestätigung (max. 3 s); Fehler/fehlende Antwort lassen die App offen. Gleichzeitige native Quit-Anfragen werden zusammengefasst. | `persistence.test.ts`: letzte Änderung, kein doppelter Timer-Save, beide Browserereignisse, native Bestätigung erst nach Speicherung, Fehler und Wiederholung, bereits laufende Speicherung. Tauri-Mockfixture prüft Erfolg, Ablehnung und fehlende Antwort des nativen Flushs. | `19c2454c0`, `782644f53` |

`commands.json` wurde durch `os-app-manifest` neu erzeugt und ist bytegleich: Name, Argument- und Rückgabetyp bleiben `view.range: RangeArgs → KeyRange`. Der erweiterte Vertrag steht im generierten `KeyRange.ts`, im Vertragstest und in `M2-ui-concept.md` 4.4. Kein bestehender Test, Budget oder `// acceptance: independent`-Test wurde gelockert oder verändert.

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
- Der Export-Hinweis wählt nicht still eine andere Menge aus: die bestehende Auswahl bleibt erhalten, der Tabellenexport exportiert die aktuelle Ansicht. Eine neue serialisierte „Bereich“-Auswahl mit eigener Exportsemantik wurde nicht eingeführt (erlaubte Hinweisvariante des Auftrags).
- Die Antwortmenge ist begrenzt; nicht materialisierte Detailbereiche müssen weiterhin ihre Quelle seitenweise durchsuchen. Dieses Paket baut keinen neuen serverseitigen Index und ändert keine Reihenfolgen.

## Abschluss-Gates

Laufende Prüfung; Endergebnisse werden vor Abschluss ergänzt. Playwright ausschließlich mit `OS_E2E_PORT=14687`, zwei Workern; Lastprozesse unter Timeout. Workspace-Test mit `~/ws-slot.sh`, Debug-Informationen/Incremental aus, Cargo `-j 4 --locked`.
