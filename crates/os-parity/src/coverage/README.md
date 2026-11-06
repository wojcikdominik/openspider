# Spec-Abdeckung

M1 bleibt mit den bisherigen Status- und Ignore-Regeln prüfbar:

```sh
cargo run --locked -p os-parity -- coverage --milestone M1 \
  --test-output target/rust-tests.txt --strict
```

Für M2 werden alle `spec_ids` der kanonischen Paketliste eingelesen, auch die der
Spikes und Messpakete. Ohne `--packages` wird die Liste unter `--root` gewählt.
Testergebnisse stammen aus echten Läufen, nicht aus den eingecheckten Statusdateien:

```sh
cargo test --workspace --exclude openspider-desktop --locked > target/rust-tests.txt 2>&1
npm run -w apps/desktop test -- --reporter=junit --outputFile.junit=../../target/vitest.xml
PLAYWRIGHT_JUNIT_OUTPUT_FILE=../../target/playwright.xml \
  npm run -w apps/desktop e2e -- --reporter=junit
cargo run --locked -p os-parity -- coverage --milestone M2 \
  --test-output target/rust-tests.txt \
  --frontend-junit target/vitest.xml --frontend-junit target/playwright.xml \
  --write docs/plan/M2-status.json
```

`--junit` nimmt weiterhin Nextest-JUnit an. `--frontend-junit` ist wiederholbar,
auch für mehrere Browser. Innerhalb einer JUnit-Datei gewinnt bei Retries der letzte
`testcase` mit identischer Datei, Suite, Browserprojekt (`hostname`) und konkretem
Titel (XML-Reihenfolge, `block`). Verschiedene Template-Instanzen werden danach zusammengeführt.
Browserprojekte bleiben ebenfalls getrennt. Zwischen verschiedenen Ergebnisdateien
überschreibt ein fehlgeschlagener Lauf einen grünen Beleg desselben Tests. Datei
und vollständiger Testtitel müssen eindeutig sein.
Der statische Frontend-Scan unterstützt literale und Template-Titel von `test`/`it`
sowie `describe`/`test.describe`-Gruppen. Nur `.ts` und `.tsx` werden gescannt;
`.js`, `.jsx` und `.mts` zählen nicht zur Abdeckung. Bei Templates müssen Dateipfad und
statische Titelteile eindeutig zu einer Quelle passen; alle ausgegebenen Fälle
werden zusammengeführt (ein roter Fall genügt). `.each` und vollständig berechnete
Titel erhalten keine Gutschrift; dafür einen direkten `test()`-/`it()`-Aufruf nutzen.
Strings, Templates, Blockkommentare und Regex-Literale erzeugen keine Tags.

Entwicklung verlangt standardmäßig nur Welle 0 (einschließlich 0a–0e).
`--merged D12,U10` ergänzt konkret gemergte Besitzerpakete; kurze oder volle
Paket-IDs sind erlaubt. Unbekannte IDs wählen kein Paket aus, erscheinen als
Warnung und in `unknown_merged` der Statusdatei, brechen den Bericht aber nicht ab.
`--merged-log <datei>` liest die Merge-Historie (`git log origin/main --format='%H%x09%s'`)
und ergänzt die daraus erkannten Pakete (`merged.rs`, B4-M2CLOSE): Titel `m2: Merge [m2/][M2-]<Zweig>`,
`m2: Integration Runde N – [merge m2/]<Zweig>` und `merge: m2/<Zweig> … in Integrationsrunde N`; es zählt der
längste Bindestrich-Präfix des Zweignamens, der eine Paket-ID aus `M2-packages.json` ist. Pakete ohne eigenen
Merge-Titel (über einen Integrationszweig gemergt) tragen in `M2-packages.json` das Feld
`merged_via: {"commit", "subject"}` und gelten als gemergt, wenn genau dieser Commit mit diesem Titel in der
Historie steht. Die Statusdatei führt `merged_packages` (ID → Beleg), `other_merge_branches` (Zweige ohne
Planpaket, z. B. `N00-SPEC-TAGS`, `int-16`) und `merged_via_missing`. Die CI nutzt diesen Weg. `--wave 1` wählt alternativ alle
Pakete bis einschließlich Welle 1; das ist nur bei vollständig gemergter Welle
sinnvoll. Für noch nicht gemergte Besitzer bleiben offene Pflicht-IDs und
Anhang-B-Einträge Warnungen. Echte `FAILED`-Ergebnisse (auch ohne Tag) bleiben
in jedem Modus Fehler. M1-IDs aus `M1-packages.json` sind bekannt und erzeugen
keine Tippfehlerwarnungen; sie zählen weiterhin nicht zu M2.

Zulässige C9-Platzhalter werden an Paketbesitz, Ort, Quell- und Zielwelle geprüft
und bleiben im Status `open`. Gültige Zurückstellungen (`M3:`, `M4:`, `M5:`,
`P2:`, `Klärung K2-<Zahl>`) sind `deferred`; Benchmarks sind neutral.
`M2:`-Altgründe in Dateien ausschließlich noch nicht gemergter Besitzer werden
als „Altgrund, Besitzer M2-xx“ in `legacy_ignores` samt Warnung ausgegeben.
Der Vorbau-Besitz aus Welle 0 tritt dabei hinter die Fachbesitzer ab Welle 1
zurück. Unbekannte Besitzer, bereits gemergte Besitzer und andere ungültige Ignores
bleiben Fehler, auch ohne Spec-Tag. Die Altgründe müssen die Besitzer bzw. N01
auflösen; Q01 ändert deren Tests nicht.

`--gate` prüft alle Wellen, fordert jede Pflicht-ID als `implemented`/`verified`
und verbietet sämtliche Platzhalter. Diese Option ist für die Q04-Abnahme und
als manuelle Eingabe des CI-Workflows vorgesehen.

Anhang B wird aus `docs/plan/M2.md` gelesen und je `(ID, Paket)` für die dort
zugewiesenen `extra_spec_ids` berichtet. Die Belegdatei muss zum `owns`-Muster des
Pakets passen; jeder verlangte Marker (`[P1]`, `[M2]`) braucht einen grünen Test.
Marker verschiedener Kommentarzeilen werden nicht zwischen IDs übertragen.
Ausnahmen aus Abschnitt 9, einschließlich AMP (K2-22), zählen nicht.

Die deterministische Statusdatei enthält `specs`, `extra`, `appendix_b`,
`placeholders`, `legacy_ignores`, `required_packages`, `failed_tests`,
`blocking_specs` und `blocking_appendix_b`. Ein fehlender Testlauf
liefert keine Gutschriften. Frische Paritätsbelege können wie in M1 über
`--parity-report`/`--evidence` ergänzt werden; vorhandenes `verified` bleibt nur
bei weiterhin grünem Test und einer M2-Statusdatei erhalten.

Der Push-/PR-Workflow erzeugt Artefakte und eine Step-Summary als Bericht; ein
roter Abdeckungsstatus ist dort kein Gate. `workflow_dispatch` mit `gate=true`
erzwingt Exit 1 bei fehlender Abdeckung oder roten Tests. Bis ci.yml/ui-e2e.yml
JUnit-Artefakte exportieren, läuft E2E separat mit identischer Matrix,
Browserinstallation und Umgebung wie ui-e2e.yml; der Bericht übernimmt die
Artefakte `vitest-junit` und `playwright-junit-<os>` aus diesen Jobs. Concurrency
bricht überholte Läufe ab; Pfadfilter begrenzen die Auslösung.

Planerbedarf (Fable-Befunde 5/6): Anhang B nennt die Marker pro ID, aber keine
Zuordnung einzelner Kriterien zu einzelnen Schichten/Paketen. Bis zur
Präzisierung bleibt die bestehende strenge Auslegung: jedes zugewiesene Paket
belegt jeden genannten Marker mit eigenen grünen Tests. Ein Wechsel zu „ein
beliebiger Marker genügt“ würde ohne Planentscheidung Kriterien fallenlassen.
Eine gemeinsame maschinenlesbare C9-Datei für Q01 und M2-check.py P7 ist als
Planerbedarf im Paketbericht ausgewiesen; bis dahin nennt der Kommentar in
m2.rs die Quelle C9 Rev. 5 / K2-26 (17.10.2026). owns beschreibt allein nicht die erlaubten Zielpakete.
