#!/usr/bin/env python3
"""Fasst die ``report.json`` aller Matrixzeilen zu einem Protokoll zusammen (M3-Q02).

Aufruf: python3 tests/install/summarize.py <ordner mit */report.json> [--out protokoll.md] [--summary $GITHUB_STEP_SUMMARY]
Exit 1, wenn eine Zeile fehlt oder eine Prüfung fehlgeschlagen ist.
"""
import argparse
import json
import sys
from pathlib import Path

SYMBOL = {'passed': '✔', 'failed': '✘', 'manual': 'Hand', 'covered': 'anderswo', 'skipped': '–'}


def load(folder):
    return sorted((json.loads(p.read_text(encoding='utf-8')) for p in Path(folder).rglob('report.json')),
                  key=lambda r: r['label'].lower())


def render(reports, expected=None, run_url=None):
    lines = ['# Installations- und Update-Matrix (M3-Q02)', '']
    if run_url:
        lines += [f'Lauf: {run_url}', '']
    if not reports:
        return '\n'.join(lines + ['Keine Ergebnisse.']) + '\n', False
    versions = sorted({(r['current'], r['next']) for r in reports})
    lines += ['Versionen: ' + ', '.join(f'N = `{a}` → N+1 = `{b}`' for a, b in versions), '']
    ids = []
    titles = {}
    for r in reports:
        for c in r['checks']:
            if c['id'] not in titles:
                ids.append(c['id'])
                titles[c['id']] = (c['title'], c['spec'])
    labels = [r['label'] for r in reports]
    lines += ['| ID | Prüfung | Spec | ' + ' | '.join(labels) + ' |', '|---|---|---|' + '---|' * len(labels)]
    ok = True
    for ident in ids:
        cells = []
        for r in reports:
            status = next((c['status'] for c in r['checks'] if c['id'] == ident), 'skipped')
            ok &= status != 'failed'
            cells.append(SYMBOL.get(status, status))
        title, spec = titles[ident]
        lines.append(f'| {ident} | {title} | {spec} | ' + ' | '.join(cells) + ' |')
    missing = sorted(set(expected or []) - set(labels))
    if missing:
        ok = False
        lines += ['', 'Ohne Ergebnis (Job abgebrochen oder übersprungen): ' + ', '.join(missing)]
    failures = [(r['label'], c) for r in reports for c in r['checks'] if c['status'] == 'failed']
    if failures:
        lines += ['', '## Fehlgeschlagen', '']
        lines += [f'- {label} {c["id"]} {c["title"]}: {c["detail"]}' for label, c in failures]
    lines += ['', 'Hand = Prüfprotokoll `docs/release/abnahme-1.0-nutzer.md` (ID in der Detailspalte der Zeile).']
    return '\n'.join(lines) + '\n', ok


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('folder', type=Path)
    parser.add_argument('--out', type=Path)
    parser.add_argument('--summary', type=Path)
    parser.add_argument('--expect', action='append', default=[], help='Erwartete Zeilen (Label)')
    parser.add_argument('--run-url')
    args = parser.parse_args(argv)
    text, ok = render(load(args.folder), args.expect, args.run_url)
    if args.out:
        args.out.write_text(text, encoding='utf-8')
    if args.summary:
        with args.summary.open('a', encoding='utf-8') as out:
            out.write(text)
    print(text)
    return 0 if ok else 1


if __name__ == '__main__':
    sys.exit(main())
