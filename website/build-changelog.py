#!/usr/bin/env python3
"""Erzeugt die Changelog-Seiten der Website aus docs/release/notes/<Version>.md.

Aufruf: python3 website/build-changelog.py [--check]
Quellen: docs/release/notes/ (eine Quelle für App, GitHub-Release und Website),
website/changelog/releases.json (veröffentlichte Versionen mit Datum, neueste zuerst) und
website/changelog/en/<Version>.md für Versionen, deren Notes nur deutsch vorliegen.
--check schreibt nichts und endet mit 1, wenn die Seiten nicht zum Stand der Notes passen.
"""
import html
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
NOTES = ROOT / "docs" / "release" / "notes"
EXTRA = ROOT / "website" / "changelog"
SITE = ROOT / "website" / "site"
REPO = "https://github.com/wojcikdominik/openspider"
MONTHS = {
    "de": ["Januar", "Februar", "März", "April", "Mai", "Juni", "Juli", "August", "September", "Oktober", "November", "Dezember"],
    "en": ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"],
}
PAGES = {
    "de": {
        "path": "neuigkeiten", "other": "en/changelog", "title": "Neuigkeiten und Changelog | OpenSpider",
        "description": "Was sich in OpenSpider geändert hat: alle veröffentlichten Versionen mit neuen Funktionen und Korrekturen, die neueste zuerst.",
        "h1": "Neuigkeiten", "lead": "Alle veröffentlichten Versionen, die neueste zuerst. Wer OpenSpider installiert hat, bekommt neue Versionen in der App angeboten.",
        "download": 'Zum <a href="/download/">Download</a>', "release": "Dateien auf GitHub ↗", "locale": "de_DE", "alt_locale": "en_GB",
        "og_alt": "OpenSpider – Der Crawler für die Agentic Ära.", "skip": "Zum Inhalt", "home": "OpenSpider · Startseite", "nav_label": "Hauptnavigation",
        "nav": '<a href="/funktionen/">Funktionen</a><a href="/download/">Download</a><a href="/faq/">FAQ</a><a class="language" href="/en/changelog/" lang="en" hreflang="en" aria-label="Read in English">EN</a>',
        "footer": '<footer><div><strong>OpenSpider</strong><p>Für Menschen und KI-Agenten.</p><small>Code: Apache-2.0</small></div><nav aria-label="Weitere Links"><a href="/handbuch/">Handbuch</a><a href="/neuigkeiten/" aria-current="page">Neuigkeiten</a><a href="/impressum/">Impressum</a><a href="/datenschutz/">Datenschutz</a><a href="https://github.com/wojcikdominik/openspider">GitHub ↗</a></nav></footer>',
    },
    "en": {
        "path": "en/changelog", "other": "neuigkeiten", "title": "What's new and changelog | OpenSpider",
        "description": "What has changed in OpenSpider: every published version with new features and fixes, newest first.",
        "h1": "What's new", "lead": "Every published version, newest first. If you have OpenSpider installed, the app offers new versions to you.",
        "download": 'Go to <a href="/en/download/">Download</a>', "release": "Files on GitHub ↗", "locale": "en_GB", "alt_locale": "de_DE",
        "og_alt": "OpenSpider – The crawler for the agentic era.", "skip": "Skip to content", "home": "OpenSpider · Home", "nav_label": "Main navigation",
        "nav": '<a href="/en/features/">Features</a><a href="/en/download/">Download</a><a href="/en/faq/">FAQ</a><a class="language" href="/neuigkeiten/" lang="de" hreflang="de" aria-label="Auf Deutsch lesen">DE</a>',
        "footer": '<footer><div><strong>OpenSpider</strong><p>For people and AI agents.</p><small>Code: Apache-2.0</small></div><nav aria-label="More links"><a href="/en/manual/">User guide</a><a href="/en/changelog/" aria-current="page">What\'s new</a><a href="/en/legal/">Legal notice</a><a href="/en/privacy/">Privacy</a><a href="https://github.com/wojcikdominik/openspider">GitHub ↗</a></nav></footer>',
    },
}


def split_languages(version):
    """Liefert {'de': text, 'en': text}. Der Kurzblock für den Update-Dialog entfällt."""
    text = (NOTES / f"{version}.md").read_text(encoding="utf-8")
    text = re.sub(r"<!-- update -->.*?<!-- /update -->", "", text, flags=re.S)
    parts = re.split(r"^### Deutsch\s*$", text, maxsplit=1, flags=re.M)
    if len(parts) == 2:
        return {"en": parts[0], "de": parts[1]}
    extra = EXTRA / "en" / f"{version}.md"
    if not extra.exists():
        sys.exit(f"{version}: Notes nur deutsch und {extra.relative_to(ROOT)} fehlt")
    return {"de": text, "en": extra.read_text(encoding="utf-8")}


def inline(text):
    out = html.escape(text, quote=False)
    out = re.sub(r"`([^`]+)`", r"<code>\1</code>", out)
    return re.sub(r"https://openspider\.de(/[^\s<]*?)(?=[.,;)]?(\s|$))", r'<a href="\1">openspider.de\1</a>', out)


def item(text):
    # Kurzes Stichwort vor dem ersten Doppelpunkt hervorheben, damit sich die Liste überfliegen lässt.
    head, sep, rest = text.partition(": ")
    if sep and len(head) <= 48 and "`" not in head:
        return f"<li><strong>{inline(head)}:</strong> {inline(rest)}</li>"
    return f"<li>{inline(text)}</li>"


def body(text):
    out, items = [], []
    for line in [l.strip() for l in text.splitlines()] + [""]:
        if line.startswith("- "):
            items.append(item(line[2:]))
            continue
        if items:
            out.append("<ul>" + "".join(items) + "</ul>")
            items = []
        if line:
            out.append(f"<p>{inline(line)}</p>")
    return "".join(out)


def date_text(iso, lang):
    y, m, d = (int(p) for p in iso.split("-"))
    return f"{d}. {MONTHS['de'][m - 1]} {y}" if lang == "de" else f"{d} {MONTHS['en'][m - 1]} {y}"


def page(lang, releases):
    p = PAGES[lang]
    url = f"https://openspider.de/{p['path']}/"
    alt = {"de": "https://openspider.de/neuigkeiten/", "en": "https://openspider.de/en/changelog/"}
    sections = []
    for r in releases:
        v = r["version"]
        sections.append(
            f'<section id="v{v.replace(".", "-")}"><h2>{v}</h2>'
            f'<p class="release-meta"><time datetime="{r["date"]}">{date_text(r["date"], lang)}</time> · '
            f'<a href="{REPO}/releases/tag/v{v}">{p["release"]}</a></p>{body(r[lang])}</section>'
        )
    meta = html.escape(p["description"])
    title = html.escape(p["title"])
    return f"""<!doctype html>
<html lang="{lang}">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title}</title>
<meta name="description" content="{meta}">
<link rel="canonical" href="{url}">
<link rel="alternate" hreflang="de" href="{alt['de']}">
<link rel="alternate" hreflang="en" href="{alt['en']}">
<link rel="alternate" hreflang="x-default" href="{alt['de']}">
<link rel="stylesheet" href="/assets/site.css?v=20261104">
<meta property="og:type" content="website">
<meta property="og:site_name" content="OpenSpider">
<meta property="og:title" content="{title}">
<meta property="og:description" content="{meta}">
<meta property="og:url" content="{url}">
<meta property="og:locale" content="{p['locale']}">
<meta property="og:locale:alternate" content="{p['alt_locale']}">
<meta property="og:image" content="https://openspider.de/assets/og.png">
<meta property="og:image:width" content="1200">
<meta property="og:image:height" content="630">
<meta property="og:image:alt" content="{p['og_alt']}">
<meta name="twitter:card" content="summary_large_image">
<meta name="twitter:title" content="{title}">
<meta name="twitter:description" content="{meta}">
<meta name="twitter:image" content="https://openspider.de/assets/og.png">
<meta name="twitter:image:alt" content="OpenSpider logo">
<link rel="icon" type="image/svg+xml" href="/assets/favicon.svg">
<link rel="icon" type="image/png" sizes="32x32" href="/assets/favicon-32.png">
<link rel="apple-touch-icon" sizes="180x180" href="/assets/apple-touch-icon.png">
<style>main code{{overflow-wrap:anywhere}}.changelog ul{{color:var(--muted);padding-left:1.2em;margin:0}}.changelog li{{margin-bottom:8px}}.changelog li strong{{color:var(--text)}}.changelog p{{margin-bottom:16px}}.changelog h2{{margin-bottom:6px}}.changelog .release-meta{{font-size:.875rem}}</style>
</head>
<body>
<a class="skip" href="#main">{p['skip']}</a>
<div class="wrap">
<header class="site-header">
<a class="brand" href="/{'' if lang == 'de' else 'en/'}" aria-label="{p['home']}"><img src="/assets/brand/openspider-mark.svg" width="40" height="40" alt="">OpenSpider</a>
<nav aria-label="{p['nav_label']}">{p['nav']}</nav>
</header>
<main id="main" tabindex="-1">
<section class="page-intro"><p class="eyebrow">OpenSpider</p><h1>{p['h1']}</h1><p class="lead">{p['lead']} {p['download']}.</p></section><div class="details changelog">{''.join(sections)}</div>
</main>
{p['footer']}
</div>
</body>
</html>
"""


def main():
    check = "--check" in sys.argv[1:]
    releases = json.loads((EXTRA / "releases.json").read_text(encoding="utf-8"))["releases"]
    for r in releases:
        r.update(split_languages(r["version"]))
    stale = []
    for lang, p in PAGES.items():
        target = SITE.joinpath(*p["path"].split("/"), "index.html")
        text = page(lang, releases)
        if check:
            if not target.exists() or target.read_text(encoding="utf-8") != text:
                stale.append(str(target.relative_to(ROOT)))
            continue
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(text, encoding="utf-8")
        print(f"geschrieben: {target.relative_to(ROOT)}")
    if stale:
        sys.exit("veraltet (python3 website/build-changelog.py ausführen): " + ", ".join(stale))


if __name__ == "__main__":
    main()
