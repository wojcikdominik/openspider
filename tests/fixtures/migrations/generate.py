"""Synthetic historical fixtures. Run from any directory; no network or customer data."""
import argparse
import ctypes
import ctypes.util
import json
from pathlib import Path
import sqlite3
import subprocess

SOURCE = 'f8bdb23a6d6e60bad0e80de80309013e146144bc'
ROOT = Path(__file__).resolve().parents[3]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--out", type=Path, default=Path(__file__).resolve().parent)
parser.add_argument("--only-schema", type=int, choices=range(1, 7),
                    help="only write schema-vN (other files untouched; no libsodium needed)")
ARGS = parser.parse_args()
OUT = ARGS.out.resolve()
# Schema v6 (M4-F01): migrations 0001-0005 from SOURCE, 0006_ai.sql from the checked-out tree.
V6_MIGRATION = 'crates/os-store/migrations/0006_ai.sql'

def source(path):
    return subprocess.check_output(['git', 'show', f'{SOURCE}:{path}'], cwd=ROOT)

def write(path, data):
    target = OUT / path
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_bytes(data)

if ARGS.only_schema is None:
    for version in (1, 2):
        path = 'tests/fixtures/config/' + ('v1/' if version == 1 else '') + 'default.osconfig'
        write(f'config-v{version}/profile.osconfig', source(path))
    write('app-v1/basic.ospreset', source('tests/fixtures/out-m2-export/basic.ospreset'))
    task = source('tests/fixtures/out-m2-tasks/nightly.ostask').decode()
    task = '\n'.join(line for line in task.splitlines() if 'password' not in line) + '\n'
    write('app-v1/nightly.ostask', task.encode())
    write('app-v1/settings.toml', b'[general]\nlanguage = "en"\n')
    write('app-v1/ui-state.json', b'{"version":1,"state":{"activeTab":"Internal","recentFiles":[]}}\n')
    # Deterministic TEST ONLY machine key and nonce. Never use these values for real profiles.
    header = dict(format='osauth', version=1, mode='keyring', keyRef='compat-test',
                  salt='00' * 16, nonce='00' * 24)
    aad = json.dumps(header, sort_keys=True, separators=(',', ':')).encode()
    plain = b'{"name":"compat-test","username":"fixture","password":"test-only"}'
    sodium = ctypes.CDLL(ctypes.util.find_library('sodium'))
    sodium.sodium_init()
    encrypted = ctypes.create_string_buffer(len(plain) + 16)
    length = ctypes.c_ulonglong()
    assert sodium.crypto_aead_xchacha20poly1305_ietf_encrypt(
        encrypted, ctypes.byref(length), plain, ctypes.c_ulonglong(len(plain)),
        aad, ctypes.c_ulonglong(len(aad)), None, bytes(24), bytes([17]) * 32) == 0
    write('app-v1/profile.osauth', json.dumps(dict(header=header, ciphertext=encrypted.raw.hex()),
                                           sort_keys=True, separators=(',', ':')).encode() + b'\n')
for version in ([ARGS.only_schema] if ARGS.only_schema else range(1, 7)):
    folder = OUT / f'schema-v{version}'
    folder.mkdir(exist_ok=True)
    path = folder / 'crawl.ospider'
    path.unlink(missing_ok=True)
    db = sqlite3.connect(path)
    db.execute('PRAGMA application_id = 1330860100')
    migrations = subprocess.check_output(['git', 'ls-tree', '-r', '--name-only', SOURCE, 'crates/os-store/migrations'], cwd=ROOT).decode().splitlines()
    for migration in sorted(migrations)[:version]:
        db.executescript(source(migration).decode())
    if version >= 6:
        db.executescript((ROOT / V6_MIGRATION).read_text())
    db.execute(f'PRAGMA user_version = {version}')
    meta = dict(crawl_id='0199aaaa-0000-7000-8000-000000000001', schema_version=str(version),
                generator='openspider 0.1.0', created_at='2026-10-17T00:00:00.000Z',
                started_at='2026-10-17T00:00:00.000Z', ended_at='2026-10-17T00:00:01.000Z',
                mode='spider', start_url='https://compat.test/', project='Compatibility',
                name='Archived crawl', state='Completed', next_url_id='3', next_seq='3',
                change_seq='1', reset_seq='0', analysis_stale='0')
    db.executemany('INSERT INTO crawl_meta VALUES (?,?)', meta.items())
    config = (OUT / f'config-v{1 if version < 3 else 2}/profile.osconfig').read_text()
    db.execute('INSERT INTO crawl_config VALUES (1,?,?)', (config, meta['created_at']))
    for index, (url, title, status) in enumerate([
        ('https://compat.test/', 'Archiv – Start', 200),
        ('https://compat.test/missing', 'Nicht gefunden', 404)], 1):
        db.execute('''INSERT INTO urls(id,url,address,origin,is_internal,state,discovered_via,
          crawl_depth,folder_depth,url_length,status_code,status,content_type,content_class,
          title_count,title_1,title_1_len,indexability,indexability_status)
          VALUES (?,?,?,?,1,'done','start',0,0,?,?,?,'text/html','HTML',1,?,?,?,?)''',
          (index, url, url, 'https://compat.test', len(url), status,
           'OK' if status == 200 else 'Not Found', title, len(title),
           'Indexable' if status == 200 else 'Non-Indexable', '' if status == 200 else 'Client Error'))
        if version >= 2:
            db.execute('INSERT OR IGNORE INTO url_counters(url_id) VALUES (?)', (index,))
    if version >= 3:
        db.execute("INSERT INTO url_ext(url_id,key,v_int,change_seq) VALUES (1,'head.title_outside',1,1)")
    db.commit()
    db.execute('VACUUM')
    db.close()
    assert path.stat().st_size <= 1024 * 1024
