# Kompatibilitätsarchiv (OUT-255)

K3-11: Ab `1.0.0-beta.1` gehört jede veröffentlichte Version hierher. Vorherige
Schemastände sind synthetische, kleine Dateien aus dem eigenen Code. Schema blieb
nach K2-99 bis 1.0 bei **v5**; **v6** kommt mit M4-F01 (`0006_ai.sql`, KI-Tabellen).
Keine Kunden- oder Referenzproduktdaten.

Alle folgenden Quellen stammen aus Commit
`f8bdb23a6d6e60bad0e80de80309013e146144bc` (Basis `m3/int-b`).

| Datei | Herkunft / Erzeugung |
| --- | --- |
| `schema-v1/crawl.ospider` | `crates/os-store/migrations/0001_init.sql`, zwei künstliche interne HTML-Seiten (200/404), Metadaten und eingebettete Konfiguration v1 |
| `schema-v2/crawl.ospider` | Migrationen 0001–0002, Konfiguration v1, URL-Zähler |
| `schema-v3/crawl.ospider` | Migrationen 0001–0003, Konfiguration v2, Zusatzfakt `head.title_outside=1` |
| `schema-v4/crawl.ospider` | Migrationen 0001–0004, Konfiguration v2, gleicher Zusatzfakt |
| `schema-v5/crawl.ospider` | Migrationen 0001–0005, Konfiguration v2, gleicher Zusatzfakt in kompakter Speicherung |
| `schema-v6/crawl.ospider` | M4-F01: Migrationen 0001–0005 aus obigem Commit plus `0006_ai.sql` aus dem ausgecheckten Stand (leere KI-Tabellen), sonst wie v5; erzeugt mit `generate.py --only-schema 6`, v1–v5 dabei unverändert |
| `schema-v6/internal_all.csv` | Kopie von `schema-v5/internal_all.csv` (gleiche Nutzdaten, KI-Tabellen leer) |
| `schema-v*/internal_all.csv` | Einmalig mit aktueller CLI aus obigen Dateien exportiert; Erwartung nach M2-Integration (int-e), K2-63; eingecheckte Sollwerte, Normalisierung gemäß Spec 06 Abschnitt 0 durch `os_testsite::golden::normalize` |
| `config-v1/profile.osconfig` | Unverändert aus `tests/fixtures/config/v1/default.osconfig` |
| `config-v2/profile.osconfig` | Unverändert aus `tests/fixtures/config/default.osconfig` |
| `app-v1/basic.ospreset` | Unverändert aus `tests/fixtures/out-m2-export/basic.ospreset` |
| `app-v1/nightly.ostask` | `tests/fixtures/out-m2-tasks/nightly.ostask`, Zeile mit künstlichem Proxy-Passwort entfernt |
| `app-v1/profile.osauth` | Format v1 nach `crates/os-store/src/auth_profiles.rs`; Generator verschlüsselt künstliches Profil mittels libsodium XChaCha20-Poly1305; `keyRef=compat-test`, Schlüssel `11` hex × 32; Salt/Nonce null, ausschließlich reproduzierbare Testdaten |
| `app-v1/settings.toml` | Synthetischer früher OUT-222-Stand nach `crates/os-model/src/settings.rs`: Sprache Englisch, spätere Abschnitte fehlen und erhalten Defaults |
| `app-v1/ui-state.json` | Synthetischer v1-Stand nach `crates/os-app/src/uistate.rs`, noch ohne Revision; Tab `Internal`, leerer Dateiverlauf |

Reproduktion im ausgecheckten Repository (Python 3, System-SQLite und libsodium;
keine neuen Projektabhängigkeiten; `--only-schema N` schreibt nur `schema-vN` und braucht kein libsodium):

```sh
python3 tests/fixtures/migrations/generate.py
CARGO_INCREMENTAL=0 timeout 3600s nice -n 10 cargo test --locked -j 4 -p os-store -p os-model --test compat_archive
CARGO_INCREMENTAL=0 UPDATE_GOLDEN=1 timeout 3600s nice -n 10 cargo test --locked -j 4 -p os-cli --test compat_archive
```

Die CSV-Erwartung nach M2-Integration (int-e), K2-63 ergänzt gemäß ONPAGE-019
vier leere Inhaltsspalten und setzt gemäß ONPAGE-110 `Meta Keywords 2 Length`
für die HTML-200-Seite von leer auf `0`. Die Archivdatenbanken bleiben unverändert.

Goldens nur mit begründeter Prüfung neu erzeugen. Normaler Testlauf schreibt keine
Sollwerte. SQLite-Dateien sind jeweils ≤1 MiB; die Erzeugung prüft diese Grenze.
Der Generator liest die SQL-/Quellfixtures mit `git show` vom genannten Commit,
nicht aus möglicherweise inzwischen veränderten Arbeitsdateien. SQLite-Versionen
können physisch andere Bytes erzeugen; Schema und Nutzdaten bleiben identisch.

Die Integrationstests `compat_archive` in os-store, os-model, os-cli und os-app
prüfen Öffnen, Migration/Sicherung, Exporte, Profile, Aufgaben, Presets und Zustand.
Vor dem App-Export wird dieselbe ausstehende Analyse wie automatisch in der CLI
ausgeführt; so werden auch abgeleitete Spalten wie Link Score verglichen.
Die Tests arbeiten für Schreibzugriffe auf temporären Kopien. Die App öffnet alte
Crawls auf migrierten Kopien; das Original bleibt erhalten.

## Release-Archiv und Übernahme ab beta.1

Bei jeder Version, einschließlich Trockenläufen, entsteht das Archiv.
Die Grenze ab `1.0.0-beta.1` betrifft ausschließlich die Übernahme veröffentlichter
Versionen ins Repository.

`release.yml` ruft `scripts/release/archive-compat-fixture.mjs --version <version>
--out release-reports/compat --cli <release-cli>` auf. Das Skript verwendet die
synthetische Probe `schema-v<SCHEMA_VERSION>` (seit M4-F01 v6), stempelt Erzeugerversion in Crawl und Konfiguration,
prüft den Export mit dem gerade gebauten Linux-Release-Binary und schreibt je Datei
Quellpfad und SHA-256 sowie den vollständigen Release-Commit in `manifest.json`.
Ein veraltetes Seed-Schema, eine Exportabweichung oder vorhandenes Ziel bricht ab.
Mit neuer Schema-/Konfigurationsversion sind zuerst Seed und Generator zu ergänzen.

Das Release enthält `compat_<version>.tar.gz` mit Prüfsumme; der Actions-Bericht
enthält denselben Versionsordner. Ab `1.0.0-beta.1` nach tatsächlicher Veröffentlichung den Ordner
unverändert nach `tests/fixtures/migrations/releases/<version>/` übernehmen und
committen. CLI-Tests entdecken alle Release-Ordner automatisch. Das Skript darf
auch direkt mit dem Standardziel im Repository verwendet werden; es committet,
pusht und veröffentlicht nichts. Ein Entwurfsrelease ist noch keine veröffentlichte
Version. Frühere Trockenläufe (z. B. beta.0 aus R04) werden als temporäre
Artefakte unterstützt; daraus folgt keine Pflicht, sie als veröffentlichten Stand
ins Repository zu übernehmen. Keine erfundenen Archive vor der Veröffentlichung. `releases/1.0.0-beta.1/` und `releases/1.0.0-beta.2/`
sind die unveränderten, gegen `SHA256SUMS` geprüften Inhalte der veröffentlichten `compat_<version>.tar.gz` (B3-REL).

Node-Vertragstests: `node --test tests/fixtures/migrations/node/archive.test.mjs`.

## Datenpfad und K3-02

`org.openspider.desktop` bleibt unverändert. Einstellungen und Crawl-Verwaltung
verwenden schon vorher `OpenSpider` über `os_store::paths`; sie hängen nicht von
der Bundle-ID ab. Ein alter Bundle-ID-Ordner existiert in diesem Produktpfad nicht.
Deshalb keine einmalige Ordnerübernahme. Der App-Test prüft Einstellungen,
UI-Zustand und Crawl-Verwaltung über zwei App-Starts und pinnt die Bundle-ID.
LaunchAgent-Präfix bleibt `de.openspider.task.`. OUT-232: der echte
Dateiweiterleitungsadapter wird mit Archivdateien ohne WebView getestet; Fokus und
Rückfragedialog des Betriebssystems bleiben der manuellen Prüfung M3-Q03 vorbehalten.

Befund D01-01: Wiederholtes Öffnen derselben Altdatei nach Neustart darf nicht
an einer vorhandenen Migrationskopie scheitern. Die App wählt inzwischen einen
freien Namen (`.migrated-2.ospider` usw.); der Regressionstest bewahrt Original,
bisherige Kopie und zusätzlich belegte Namen bytegleich.
