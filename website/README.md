# OpenSpider Projektwebsite · M3-W01

`site/` enthält die direkt auslieferbare Website: Deutsch unter `/`, Englisch unter
`/en/`, jeweils Funktionen, Download/Installation, FAQ, Impressum und Datenschutz und
404-Seite. Gemeinsames CSS, Bilder und Icons liegen unter `site/assets/`.
Kein Build-Schritt, keine Laufzeitabhängigkeiten. Einziges Browser-JavaScript ist das optionale
`assets/beta-feedback.js` (Versionsvorbelegung im Beta-Formular); alle Seiten funktionieren ohne.
Das Formular sendet an `POST /api/feedback`, siehe `api/README.md` (PHP, nicht Teil von `site/`).
Die Danke-Seiten `/beta/danke/` und `/en/beta/thanks/` sind `noindex` und stehen nicht in der Sitemap.
Das bestehende Verzeichnis `placeholder/` bleibt unverändert.

## Auslieferung (erst nach Nutzer-Go, D12)

Dieses Paket veröffentlicht nichts und ändert weder DNS noch Hosting. Die folgenden
Schritte sind ausschließlich eine Anleitung für die spätere Veröffentlichung nach
ausdrücklichem Go des Nutzers. Echte Download-Links ergänzt M3-W02 für Version 1.0
oder eine öffentliche Beta.

Den **Inhalt** von `website/site/` einschließlich Unterordnern nach
`/var/www/openspider.de/public/` kopieren, zum Beispiel vom Repository-Verzeichnis:

```sh
cp -R website/site/. /var/www/openspider.de/public/
```

Caddy muss dieses Verzeichnis als Root mit `file_server` ausliefern.
Canonical-URLs setzen HTTPS und die Domain `openspider.de` voraus.
Die Verzeichnis-URLs verwenden abschließende Schrägstriche. Für unbekannte URLs
muss Caddy die mitgelieferte `/404.html` beziehungsweise `/en/404.html` als
Fehlerdokument mit HTTP-Status 404 ausliefern; `file_server` wählt diese Dateien
nicht automatisch aus. Diese Serverkonfiguration ist beim Betreiber vorzunehmen.
Lokale Vorschau: `python3 -m http.server 8000 --directory website/site`.

## Inhalt und Quellen

Ausschließlich Repo-Inhalte: `README.md`, `docs/ARCHITECTURE.md`, `docs/spec/*`
und die Paketvorgaben aus `docs/plan/M3.md` / `M3-packages.json` (M3-W01). Crawl und Analyse-Tabs: Spec 01–03 und 05;
Exporte, Vergleich und Aufgabenplanung: Spec 06 (OUT-001 ff., OUT-140 ff.,
OUT-200 ff.); Change Detection: Spec 03 (TECH-300 ff.); MCP: Spec 10
(AGENT-001 ff.). Plattformen und Voraussetzungen: Architektur sowie Spec 06,
OUT-260–267 und OUT-273. Linux wird ausdrücklich auf Kern/CLI beschränkt;
Eine Linux-Desktop-App ist eine bewusste Lücke (OUT-264); Linux-CLI nach OUT-273
bleibt vorgesehen. Die Download-Seiten nennen nur Mindestplattformen und
„Version 1.0 folgt“, einschließlich Windows on ARM unter x64-Emulation.
Keine Leistungszahlen oder Veröffentlichungstermine. Der dokumentierte
Funktionsumfang wird auf der Funktionsseite vom Umfang einzelner Beta-Builds abgegrenzt.

## Changelog-Seiten

`/neuigkeiten/` und `/en/changelog/` werden erzeugt, nicht von Hand gepflegt:
`python3 website/build-changelog.py` liest `docs/release/notes/<Version>.md` (dieselbe Quelle wie
Update-Dialog und GitHub-Release) und `website/changelog/releases.json` (veröffentlichte Versionen
mit Datum, neueste zuerst). Bei einem Release: Version und Datum in `releases.json` eintragen,
Skript ausführen, beide Seiten mit committen. Notes, die nur deutsch vorliegen, brauchen
`website/changelog/en/<Version>.md`.

## Prüfung

```sh
python3 website/build-changelog.py --check
node website/check-site.mjs
bash scripts/check-neutral.sh
```

Der Node-Check benötigt keine Abhängigkeiten. Er prüft alle 22 HTML-Seiten,
interne Links einschließlich Sprungmarken, Assets, erlaubte externe Links,
eindeutige Titel/Beschreibungen, Canonicals, gegenseitige hreflang-Paare,
Sprachwechsel, Überschriften, alt-Attribute, Social-Metadaten, JSON-LD,
Sitemap (ohne noindex-Seiten), robots.txt, OG-Bildmaße, das Formularziel `/api/feedback`,
das einzige erlaubte Skript und das Gewicht beider Startseiten.
Unkomprimiert einschließlich CSS, Logo und aller referenzierten Icons:
DE **77.865 Byte**, EN **77.643 Byte**; jeweils unter 200.000 Byte.
Das nur in Metadaten referenzierte OG-Bild wird beim Seitenaufruf nicht geladen.
Im ursprünglichen Website-Lauf wurde der Check zusätzlich mit absichtlich beschädigten Links, Bildern,
alt-Attributen, hreflang und h1 geprüft; alle Fehler wurden erkannt.

Im ursprünglichen Website-Lauf zusätzlich lokal in Chrome geprüft: alle 14 Seiten bei 320 und 1280 px,
jeweils hell/dunkel (56 Kombinationen). Kein horizontaler Überlauf, keine
fehlenden Bilder, geprüfte Textkontraste auf WCAG-AA-Niveau, Skip-Link bei
Fokus sichtbar. Die beiden Startseiten und das OG-Bild wurden visuell gesichtet.
Am 31.10.2026 erneut mit Chromium geprüft: alle 56 Kombinationen ohne
horizontalen Überlauf, fehlende Bilder, HTTP- oder JavaScript-Fehler. Lange
Überschriften umbrechen jetzt auch bei 320 px.
Dies ersetzt keine vollständige manuelle Prüfung mit Screenreader.

## Offene Punkte vor Veröffentlichung

- Impressum und Datenschutzerklärung sind aus evnxt.de/wojcik.de übernommen und an openspider.de angepasst
  (keine Cookies, kein Tracking, Zugriffsprotokolle bis neun Wochen, Hoster Hetzner). Brand-Dateien sind eingesetzt;
  das OG-Bild ist aus dem neuen Logo erzeugt.
- Echte Screenshots ergänzen; die Website zeigt derzeit ausschließlich das Repo-Logo.
- M3-W02: echte Download-Links für Version 1.0 oder eine veröffentlichte Beta ergänzen.
- Caddy-Fehlerseiten wie oben anbinden und HTTPS/404 nach dem Kopieren prüfen.
