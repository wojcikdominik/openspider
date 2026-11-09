#!/usr/bin/env python3
"""Crawl-Rate mit und ohne simulierten Update-Download (M3-Q02 Schritt 3, OUT-250/CRAWL-142).

Nur Werte vom Mac mini (Referenzhardware, ruhige Maschine nach C11) zählen. Ablauf je Wiederholung
abwechselnd: Lauf A (ohne Download) und Lauf B (während eines gedrosselten HTTPS-Downloads über den
Mock-Update-Proxy, wie die App ihn ausführt: Proxy, TLS mit Test-CA, User-Agent ``OpenSpider-Updater``).
Bedingung: |Median B − Median A| / Median A ≤ 5 %. Streuen die A-Läufe um mehr als 5 %, ist die
Maschine nicht ruhig genug: Ergebnis „nicht auswertbar“, Messung wiederholen.

Grenze: Der Download läuft in einem eigenen Prozess (Netz, TLS, Datenträger teilen sich die Läufe),
nicht im App-Prozess; die Zustandsseite im App-Prozess prüft ``update_crawl.rs``.

Aufruf:
  python3 tests/install/crawl_rate.py --cli <openspider> --testsite target/release/os-testsite --output DIR
"""
import argparse
import csv
import io
import json
import os
import platform
import re
import statistics
import subprocess
import sys
import tempfile
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
sys.path.insert(0, str(HERE))
import mockupdate  # noqa: E402

DOWNLOAD_PATH = '/wojcikdominik/openspider/releases/download/v0.0.0-q02/payload.bin'


def evaluate(without, with_download, limit=0.05):
    """Bewertung nach dem Messrezept: Ruhe der A-Läufe, dann Abweichung der Mediane."""
    a = statistics.median(without)
    b = statistics.median(with_download)
    spread = (max(without) - min(without)) / a
    deviation = abs(b - a) / a
    if spread > limit:
        verdict = 'nicht auswertbar'
    else:
        verdict = 'bestanden' if deviation <= limit else 'nicht bestanden'
    return dict(median_without=a, median_with=b, spread_without=spread, deviation=deviation,
                limit=limit, verdict=verdict)


def hardware():
    info = dict(platform=platform.platform(), machine=platform.machine(), cpus=os.cpu_count(),
                python=platform.python_version())
    if platform.system() == 'Darwin':
        for key in ('hw.model', 'machdep.cpu.brand_string', 'hw.memsize'):
            info[key] = subprocess.run(['sysctl', '-n', key], capture_output=True, text=True).stdout.strip()
    return info


def crawl(cli, base, max_urls, out, env):
    started = time.monotonic()
    result = subprocess.run([str(cli), '--crawl', base + '/', '--max-urls', str(max_urls), '--no-store',
                             '--export-tabs', 'Internal:All', '--output-folder', str(out)],
                            env=env, capture_output=True, text=True, timeout=3600)
    elapsed = time.monotonic() - started
    if result.returncode:
        raise RuntimeError(f'Crawl Exit {result.returncode}: {result.stderr[-400:]}')
    rows = list(csv.reader(io.StringIO((out / 'internal_all.csv').read_text(encoding='utf-8-sig'))))
    urls = len(rows) - 1
    return dict(seconds=elapsed, urls=urls, urls_per_second=urls / elapsed)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--cli', type=Path, required=True, help='OpenSpider-CLI (Release-Build)')
    parser.add_argument('--testsite', type=Path, required=True, help='os-testsite (Release-Build)')
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--runs', type=int, default=3)
    parser.add_argument('--max-urls', type=int, default=5000)
    parser.add_argument('--download-mb', type=int, default=70)
    parser.add_argument('--rate-mb', type=float, default=10.0, help='Drosselung des Downloads in MB/s')
    args = parser.parse_args(argv)
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    work = Path(tempfile.mkdtemp(prefix='q02-rate-'))
    certs = mockupdate.make_certs(work / 'certs')
    payload = work / 'payload.bin'
    with payload.open('wb') as target:
        for _ in range(args.download_mb):
            target.write(os.urandom(1 << 20))
    scenarios = work / 'scenarios.json'
    scenarios.write_text(json.dumps(dict(scenarios={'valid': {DOWNLOAD_PATH: dict(status=200, file=str(payload))}})),
                         encoding='utf-8')
    port_file = work / 'port'
    rate = args.rate_mb * 1024 * 1024
    proxy = subprocess.Popen([sys.executable, str(HERE / 'mockupdate.py'), 'serve', '--scenarios', str(scenarios),
                              '--certs', str(work / 'certs'), '--rate', str(rate), '--port-file', str(port_file),
                              '--log', str(output / 'mock-log.jsonl')], stdout=subprocess.DEVNULL)
    site_log = output / 'testsite.log'
    # Deterministische Site ohne Dateien (wie crawl-synthetic), größer als die Messung.
    spec = f'synthetic:n={max(args.max_urls * 2, 1000)},links=10'
    site = subprocess.Popen([str(args.testsite.resolve()), spec, '0'], cwd=str(ROOT),
                            stdout=site_log.open('w', encoding='utf-8'), stderr=subprocess.STDOUT)
    env = dict(os.environ, OPENSPIDER_HOME=str(work / 'home'), OPENSPIDER_SECRET_STORE='memory',
               OPENSPIDER_DISABLE_UPDATE_CHECK='1')
    runs = []
    try:
        base = None
        for _ in range(300):
            match = re.search(r'-> (http://127\.0\.0\.1:\d+)', site_log.read_text(encoding='utf-8'))
            if match and port_file.exists():
                base = match.group(1)
                break
            time.sleep(0.1)
        if not base:
            raise RuntimeError('Fixture-Server oder Mock-Proxy nicht gestartet')
        mock = f'127.0.0.1:{port_file.read_text().strip()}'
        crawl(args.cli, base, min(200, args.max_urls), output / 'warmup', env)
        for index in range(args.runs):
            for mode in ('A', 'B'):
                download = None
                if mode == 'B':
                    download = subprocess.Popen(
                        [sys.executable, str(HERE / 'mockupdate.py'), 'fetch', f'https://github.com{DOWNLOAD_PATH}',
                         '--proxy', mock, '--ca', certs['ca.pem'], '--out', str(work / 'download.bin')],
                        stdout=subprocess.DEVNULL)
                    time.sleep(1)
                result = crawl(args.cli, base, args.max_urls, output / f'{mode}-{index}', env)
                if download:
                    result['download_overlapped'] = download.poll() is None
                    download.kill()
                    download.wait()
                result.update(mode=mode, index=index)
                runs.append(result)
                print(json.dumps(result), flush=True)
    finally:
        site.terminate()
        proxy.terminate()
        site.wait(timeout=30)
        proxy.wait(timeout=30)
    without = [r['urls_per_second'] for r in runs if r['mode'] == 'A']
    with_download = [r['urls_per_second'] for r in runs if r['mode'] == 'B']
    summary = evaluate(without, with_download)
    if not all(r.get('download_overlapped') for r in runs if r['mode'] == 'B'):
        summary['verdict'] = 'nicht auswertbar'
        summary['reason'] = 'Download endete vor dem Crawl; --download-mb erhöhen oder --rate-mb senken'
    report = dict(hardware=hardware(), cli=str(args.cli), max_urls=args.max_urls, download_mb=args.download_mb,
                  rate_mb=args.rate_mb, runs=runs, summary=summary)
    (output / 'result.json').write_text(json.dumps(report, indent=2), encoding='utf-8')
    print(json.dumps(summary, indent=2))
    return 0 if summary['verdict'] == 'bestanden' else 1


if __name__ == '__main__':
    sys.exit(main())
