#!/usr/bin/env python3
"""Run one local CLI scale stage. Usage: python3 -I run-stage.py N.

Run from the repository root after the release build; temporary crawl files are removed
only after measurement succeeds or fails. Requires macOS ps and caffeinate externally.
"""
import datetime
import importlib.util
import json
import os
import re
import resource
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[4]
SCALE = Path(__file__).resolve().parent


def main():
    n = int(sys.argv[1])
    metadata = json.loads(subprocess.check_output(['cargo', 'metadata', '--no-deps', '--format-version', '1'], cwd=ROOT))
    binaries = Path(metadata['target_directory']) / 'release'
    date = datetime.date.today().isoformat()
    dest = SCALE / os.environ.get('OS_SCALE_DEST', date)
    dest.mkdir(exist_ok=True)
    max_load = float(os.environ.get('OS_SCALE_MAX_LOAD', '2'))
    min_idle = float(os.environ.get('OS_SCALE_MIN_IDLE', '0'))
    idle = None
    # A preceding release build leaves the 1-minute load high; wait (bounded) for it to settle.
    wait_s = float(os.environ.get('OS_SCALE_LOAD_WAIT_S', '600'))
    poll_s = float(os.environ.get('OS_SCALE_LOAD_POLL_S', '15'))
    deadline = time.monotonic() + wait_s
    while True:
        uptime = subprocess.check_output(['uptime'], text=True).strip()
        load = float(uptime.split('load averages:')[-1].strip().split()[0].replace(',', '.'))
        if load < max_load or time.monotonic() + poll_s > deadline:
            break
        print(f'load {load} >= {max_load}, waiting {poll_s:.0f} s: {uptime}', file=sys.stderr, flush=True)
        time.sleep(poll_s)
    if load >= max_load:
        raise RuntimeError(f'load >= {max_load} after waiting {wait_s:.0f} s: {uptime}')
    if min_idle:
        top = subprocess.check_output(['top', '-l', '2', '-s', '1', '-n', '0'], text=True)
        idle = float(re.findall(r'([0-9.]+)% idle', top)[-1])
        if idle < min_idle:
            raise RuntimeError(f'CPU idle {idle}% < {min_idle}%')
    spec = importlib.util.spec_from_file_location('dbstat', SCALE / 'dbstat.py')
    dbstat = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(dbstat)
    with tempfile.TemporaryDirectory(prefix='openspider-scale-') as tmp:
        tmp = Path(tmp)
        site = subprocess.Popen([str(binaries/'os-testsite'), f'synthetic:n={n},links=30'], stdout=subprocess.PIPE, text=True)
        try:
            url = site.stdout.readline().strip().split(' -> ')[1]
            env = os.environ.copy()
            env.update(OPENSPIDER_HOME=str(tmp/'home'), OPENSPIDER_SECRET_STORE='memory', OS_SCALE_DIAG='1')
            with (tmp/'stderr').open('w') as log:
                start = time.monotonic()
                proc = subprocess.Popen([str(binaries/'openspider'), '--crawl', url, '--headless', '--output-folder', str(tmp/'out'), '--json-progress'], stdout=subprocess.DEVNULL, stderr=log, env=env)
                samples = []
                while proc.poll() is None:
                    if time.monotonic() - start > 5400:
                        proc.kill()
                        proc.wait()
                        raise RuntimeError('CLI exceeded 5400 seconds')
                    files = list((tmp/'home'/'crawls').glob('*.ospider'))
                    db = files[0] if files else None
                    def size(path):
                        try: return path.stat().st_size
                        except FileNotFoundError: return 0
                    ps = subprocess.run(['ps', '-o', 'rss=', '-p', str(proc.pid)], capture_output=True, text=True)
                    rss = int(ps.stdout.strip() or 0) * 1024
                    samples.append({'elapsed_s': time.monotonic()-start, 'db_bytes': size(db) if db else 0, 'wal_bytes': size(Path(str(db)+'-wal')) if db else 0, 'rss_bytes': rss})
                    time.sleep(5)
                elapsed = time.monotonic()-start
            # poll() has reaped the CLI; the other reaped children (ps/cargo metadata) are smaller.
            peak_rss = resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss
            if sys.platform != 'darwin': peak_rss *= 1024
            lines = (tmp/'stderr').read_text().splitlines()
            if proc.returncode:
                raise RuntimeError(f'CLI exit {proc.returncode}: {lines[-20:]}')
            progress = []
            diag = {}
            phases = []
            for line in lines:
                if line.startswith('OS_SCALE_DIAG '):
                    parts = line.split(); diag[parts[1]] = {k:int(v) for k,v in (p.split('=',1) for p in parts[2:])}
                    if 'duration_ms' in diag[parts[1]]:
                        phases.append({'phase': parts[1], **diag[parts[1]]})
                elif line.startswith('{'):
                    try: p=json.loads(line)
                    except ValueError: continue
                    if p.get('phase') in ('crawl','analysis') and 'crawled' in p: progress.append(p)
            final = max((p.get('crawled',0) for p in progress), default=0)
            if final != n: raise RuntimeError(f'expected {n} URLs, got {final}')
            files = list((tmp/'home'/'crawls').glob('*.ospider'))
            if len(files) != 1: raise RuntimeError(f'expected one crawl file, got {files}')
            storage = dbstat.measure(str(files[0]))
            export_dir = os.environ.get('OS_SCALE_EXPORT_DIR')
            if export_dir:
                subprocess.run([str(binaries/'openspider'), '--load-crawl', str(files[0]),
                                '--headless', '--output-folder', export_dir,
                                '--export-tabs', 'Internal:All', '--bulk-export', 'Links:All Inlinks'],
                               env=env, check=True, timeout=5400)
            points = [(0,0)] + [(p['crawled'],p['elapsed_s']) for p in progress if p.get('crawled',0)>0]
            def at(count):
                for (a,ta),(b,tb) in zip(points,points[1:]):
                    if a <= count <= b and b>a: return ta+(tb-ta)*(count-a)/(b-a)
                return None
            windows=[]
            for begin in range(0,n,100000):
                end=min(begin+100000,n); a=at(begin); b=at(end)
                windows.append({'from':begin,'to':end,'seconds':b-a if a is not None and b is not None else None,'urls_per_s':(end-begin)/(b-a) if a is not None and b is not None and b>a else None})
            result={'rss_peak_rusage_bytes':int(peak_rss),'quiet':{'max_load':max_load,'min_idle_pct':min_idle,'idle_pct':idle},'n':n,'commit':subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),'uptime':uptime,'cli_elapsed_s_sampled':elapsed,'progress':progress,'windows':windows,'samples':samples,'diag':diag,'phases':phases,'storage':storage,'measurement_notes':['Progress elapsed_s is integer seconds; window boundaries are linearly interpolated.','Sampling every 5 s can miss short peaks; process completion time can be overestimated by up to 5 s.','Parse is included in engine analyze_ns; worker sums overlap and are not wall-clock shares.']}
            (dest/f'stage-{n}.json').write_text(json.dumps(result,indent=2,ensure_ascii=False)+'\n')
            top=sorted(storage['objects'],key=lambda o:o['bytes'],reverse=True)[:10]
            md=[f'# Stufe {n}', '',f'- Load: {uptime}',f'- URLs: {final}; Datenbank: {storage["file_bytes"]} Bytes ({storage["file_bytes"]/final:.1f} Bytes/URL)',f'- WAL-Spitze (5-s-Abtastung): {max(s["wal_bytes"] for s in samples)} Bytes; RSS-Spitze: {max(int(peak_rss),max(s["rss_bytes"] for s in samples))} Bytes',f'- Freiliste: {storage["freelist_count"]} Seiten', '', '## Durchsatzfenster', '', '| URLs | URLs/s |','|---|---:|']
            md += [f'| {w["from"]}–{w["to"]} | {w["urls_per_s"] if w["urls_per_s"] is not None else "unbekannt"} |' for w in windows]
            md += ['', '## Größte Objekte', '', '| Objekt | Bytes | Bytes/URL | ungenutzte Bytes |','|---|---:|---:|---:|']
            md += [f'| {o["name"]} | {o["bytes"]} | {o["bytes"]/final:.1f} | {o["unused"]} |' for o in top]
            (dest/f'stage-{n}.md').write_text('\n'.join(md)+'\n')
            if storage['analysis_state'] != 'completed' or 'pending' in storage['analysis_steps'].values():
                raise RuntimeError(f'incomplete analysis: {storage["analysis_steps"]}')
            print(f'{n}: {storage["file_bytes"]} bytes, windows={windows}, results={dest}',flush=True)
        finally:
            site.send_signal(signal.SIGINT)
            site.wait(timeout=30)


if __name__ == '__main__':
    main()
