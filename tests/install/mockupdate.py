#!/usr/bin/env python3
"""Lokaler Mock-Update-Server für die Installations- und Update-Matrix (M3-Q02).

Die App fragt fest ``https://github.com/<repo>/releases/latest/download/latest(-beta).json`` ab.
Damit kein Request das öffentliche Release erreicht, läuft der Test über die app-weite
Proxy-Einstellung (OUT-269) und eine vertrauenswürdige Test-CA (OUT-270):

* Der Proxy nimmt nur ``CONNECT`` zu den GitHub-Hosts an und beendet TLS selbst mit einem
  Zertifikat der laufzeiterzeugten Test-CA. Alle anderen Ziele und jede Klartext-Anfrage werden
  mit 403 abgewiesen und protokolliert; es gibt keine Weiterleitung ins Internet.
* Hinter dem TLS-Endpunkt liefert ein Szenario Manifest und Artefakte von Version N+1 aus einem
  Release-Trockenlauf (``release-assets``) aus, unverändert oder mit Fehlern (OUT-252).

Szenarien (``prepare``):
  valid          Manifest N+1, Artefakte bytegleich
  tampered       Manifest N+1, im ausgelieferten Artefakt ist genau ein Byte gekippt
  http-artifact  Manifest N+1, Artefakt-URLs mit ``http://``
  http-redirect  Endpunkt antwortet 302 auf eine ``http://``-Adresse
  downgrade      Manifest mit Version N (für N gleich, für N+1 älter)

Aufrufe:
  mockupdate.py certs --out DIR
  mockupdate.py prepare --current DIR --next DIR --out DIR
  mockupdate.py serve --scenarios FILE --certs DIR [--scenario valid] [--port 0] [--rate BYTES/S] [--log FILE]
  mockupdate.py fetch URL --proxy HOST:PORT --ca FILE --out FILE
"""
import argparse
import copy
import http.client
import http.server
import json
import os
import re
import shutil
import socketserver
import ssl
import subprocess
import sys
import tempfile
import threading
import time
from pathlib import Path
from urllib.parse import parse_qs, urlsplit

ALLOWED_HOSTS = ('github.com', 'objects.githubusercontent.com', 'release-assets.githubusercontent.com')
USER_AGENT = 'OpenSpider-Updater'
SCENARIOS = ('valid', 'tampered', 'http-artifact', 'http-redirect', 'downgrade')
PLATFORM_KEYS = ('darwin-aarch64', 'darwin-x86_64', 'windows-x86_64-nsis', 'windows-x86_64-msi')
_SEMVER = re.compile(r'^v?(\d+)\.(\d+)\.(\d+)(?:-([0-9A-Za-z.-]+))?(?:\+[0-9A-Za-z.-]+)?$')


# --- SemVer -------------------------------------------------------------------------------------
def semver_key(version):
    """Sortierschlüssel nach SemVer 2.0 (Vorabversionen vor der Release-Version)."""
    match = _SEMVER.match(version.strip())
    if not match:
        raise ValueError(f'Keine SemVer-Version: {version!r}')
    core = tuple(int(x) for x in match.group(1, 2, 3))
    pre = match.group(4)
    if pre is None:
        return core + ((1,),)
    parts = tuple((0, int(p), '') if p.isdigit() else (1, 0, p) for p in pre.split('.'))
    return core + ((0,) + parts,)


def is_prerelease(version):
    return '-' in version.split('+', 1)[0]


# --- Test-CA ------------------------------------------------------------------------------------
def make_certs(out, openssl=None, days=7):
    """Ephemere Test-CA (ECDSA P-256) und Serverzertifikat für die GitHub-Hosts erzeugen.

    Der CA-Schlüssel wird nach dem Signieren gelöscht. Nur ``ca.pem``, ``server.pem`` und
    ``server.key`` bleiben; die CA gilt ausschließlich über ``trusted_certificates`` der Testprofile.
    """
    out = Path(out)
    out.mkdir(parents=True, exist_ok=True)
    git_openssl = Path(os.environ.get('ProgramFiles', r'C:\Program Files')) / 'Git' / 'usr' / 'bin' / 'openssl.exe'
    openssl = openssl or shutil.which('openssl') or (str(git_openssl) if git_openssl.is_file() else None)
    if not openssl:
        raise RuntimeError('openssl nicht gefunden')
    with tempfile.TemporaryDirectory(prefix='q02-ca-') as tmp:
        tmp = Path(tmp)
        (tmp / 'ca.cnf').write_text(
            '[req]\ndistinguished_name = dn\nprompt = no\n[dn]\nCN = OpenSpider Q02 Test CA\n'
            '[v3_ca]\nbasicConstraints = critical,CA:TRUE\nkeyUsage = critical,keyCertSign,cRLSign\n'
            'subjectKeyIdentifier = hash\n', encoding='ascii')
        (tmp / 'leaf.cnf').write_text(
            '[req]\ndistinguished_name = dn\nprompt = no\n[dn]\nCN = github.com\n', encoding='ascii')
        (tmp / 'leaf.ext').write_text(
            'basicConstraints = critical,CA:FALSE\nkeyUsage = critical,digitalSignature\n'
            'extendedKeyUsage = serverAuth\nsubjectKeyIdentifier = hash\nauthorityKeyIdentifier = keyid\n'
            'subjectAltName = ' + ','.join(f'DNS:{h}' for h in ALLOWED_HOSTS) + '\n', encoding='ascii')

        def run(*args):
            subprocess.run([openssl, *args], check=True, capture_output=True, cwd=tmp)

        run('ecparam', '-name', 'prime256v1', '-genkey', '-noout', '-out', 'ca.key')
        run('req', '-x509', '-new', '-key', 'ca.key', '-sha256', '-days', str(days), '-config', 'ca.cnf',
            '-extensions', 'v3_ca', '-out', 'ca.pem')
        run('ecparam', '-name', 'prime256v1', '-genkey', '-noout', '-out', 'server.key')
        run('req', '-new', '-key', 'server.key', '-config', 'leaf.cnf', '-out', 'server.csr')
        run('x509', '-req', '-in', 'server.csr', '-CA', 'ca.pem', '-CAkey', 'ca.key', '-sha256',
            '-days', str(days), '-set_serial', str(int.from_bytes(os.urandom(8), 'big')),
            '-extfile', 'leaf.ext', '-out', 'server.pem')
        for name in ('ca.pem', 'server.pem', 'server.key'):
            shutil.copyfile(tmp / name, out / name)
    return {name: str(out / name) for name in ('ca.pem', 'server.pem', 'server.key')}


# --- Szenarien ----------------------------------------------------------------------------------
def read_manifest(assets):
    """Kanal-Manifest eines ``release-assets``-Ordners; beide Dateien müssen gleich sein (K3-04)."""
    assets = Path(assets)
    found = {n: json.loads((assets / n).read_text(encoding='utf-8'))
             for n in ('latest.json', 'latest-beta.json') if (assets / n).is_file()}
    if not found:
        return None
    values = list(found.values())
    if any(v != values[0] for v in values[1:]):
        raise ValueError(f'latest.json und latest-beta.json unterscheiden sich in {assets}')
    return values[0]


_DOWNLOAD = re.compile(r'^/([A-Za-z0-9_.-]+)/([A-Za-z0-9_.-]+)/releases/download/v([^/]+)/([^/]+)$')


def prepare(current, nxt, out, current_version=None):
    """Szenarien aus den ``release-assets`` zweier Trockenläufe (N und N+1) erzeugen."""
    out = Path(out)
    out.mkdir(parents=True, exist_ok=True)
    manifest = read_manifest(nxt)
    if manifest is None:
        raise ValueError('N+1 enthält kein latest.json: Trockenlauf ohne signierte Updater-Artefakte')
    version_next = manifest['version']
    version_current = current_version or detect_version(current)
    if semver_key(version_next) <= semver_key(version_current):
        raise ValueError(f'N+1 ({version_next}) ist nicht neuer als N ({version_current})')
    if not is_prerelease(version_current) and is_prerelease(version_next):
        raise ValueError('Stabiler Client und Vorabversion als N+1: die App lehnt das ab (OUT-254)')
    missing = [k for k in PLATFORM_KEYS if k not in manifest.get('platforms', {})]
    if missing:
        raise ValueError(f'Manifest N+1 ohne Plattformen: {", ".join(missing)}')
    repo = None
    artifacts = {}
    for key, entry in manifest['platforms'].items():
        url = urlsplit(entry['url'])
        match = _DOWNLOAD.match(url.path)
        if url.scheme != 'https' or url.hostname != 'github.com' or not match:
            raise ValueError(f'Unerwartete Artefakt-URL für {key}: {entry["url"]}')
        owner, name, tag_version, filename = match.groups()
        if tag_version != version_next:
            raise ValueError(f'Artefakt-URL {key} zeigt auf v{tag_version}, Manifest auf {version_next}')
        if repo not in (None, f'{owner}/{name}'):
            raise ValueError('Artefakte aus verschiedenen Repositorys')
        repo = f'{owner}/{name}'
        path = Path(nxt) / filename
        sig = Path(nxt) / (filename + '.sig')
        if not path.is_file() or not sig.is_file():
            raise ValueError(f'Artefakt fehlt im Trockenlauf: {filename}')
        if sig.read_text(encoding='utf-8').strip() != entry['signature'].strip():
            raise ValueError(f'Signatur im Manifest weicht von {filename}.sig ab')
        artifacts[key] = dict(url=entry['url'], path=str(path.resolve()), size=path.stat().st_size,
                              request_path=url.path, flip=path.stat().st_size // 2)
    endpoints = [f'/{repo}/releases/latest/download/{n}' for n in ('latest.json', 'latest-beta.json')]

    def with_manifest(value, flip=False):
        routes = {p: dict(status=200, json=value) for p in endpoints}
        for a in artifacts.values():
            routes[a['request_path']] = dict(status=200, file=a['path'], flip=a['flip'] if flip else None)
        return routes

    http_manifest = copy.deepcopy(manifest)
    for entry in http_manifest['platforms'].values():
        entry['url'] = 'http://' + entry['url'][len('https://'):]
    older = copy.deepcopy(manifest)
    older['version'] = version_current
    redirect = {p: dict(status=302, location=f'http://github.com{p}') for p in endpoints}
    scenarios = {'valid': with_manifest(manifest), 'tampered': with_manifest(manifest, flip=True),
                 'http-artifact': with_manifest(http_manifest), 'http-redirect': redirect,
                 'downgrade': with_manifest(older)}
    result = dict(repository=repo, current=version_current, next=version_next,
                  channel='beta' if is_prerelease(version_current) else 'stable',
                  endpoints=endpoints, artifacts=artifacts, scenarios=scenarios)
    (out / 'scenarios.json').write_text(json.dumps(result, indent=2), encoding='utf-8')
    return result


def detect_version(assets):
    """Version N aus einem ``release-assets``-Ordner (Manifest, sonst Dateinamen der Installer)."""
    manifest = read_manifest(assets)
    if manifest:
        return manifest['version']
    versions = set()
    for path in Path(assets).iterdir():
        m = re.match(r'^OpenSpider_(\d+\.\d+\.\d+(?:-[0-9A-Za-z.]+)?)_(?:universal\.dmg|x64-setup\.exe)$', path.name)
        if m:
            versions.add(m.group(1))
    if len(versions) != 1:
        raise ValueError(f'Version N in {assets} nicht eindeutig: {sorted(versions)}')
    return versions.pop()


# --- Server -------------------------------------------------------------------------------------
class _Inner(http.server.BaseHTTPRequestHandler):
    """HTTP/1.1 hinter dem TLS-Endpunkt; liefert die Routen des aktiven Szenarios."""
    protocol_version = 'HTTP/1.1'
    timeout = 30

    def log_message(self, *args):
        pass

    def do_HEAD(self):
        self._serve(body=False)

    def do_GET(self):
        self._serve(body=True)

    def _serve(self, body):
        mock = self.server
        split = urlsplit(self.path)
        scenario = mock.scenario
        route = mock.scenarios[scenario].get(split.path)
        entry = dict(kind='https', host=(self.headers.get('Host') or '').split(':')[0], method=self.command,
                     path=split.path, query=parse_qs(split.query, keep_blank_values=True),
                     user_agent=self.headers.get('User-Agent'), scenario=scenario)
        try:
            if route is None:
                entry['status'] = 404
                self._bytes(404, b'not found', 'text/plain', body)
            elif 'location' in route:
                entry['status'] = route['status']
                self.send_response(route['status'])
                self.send_header('Location', route['location'])
                self.send_header('Content-Length', '0')
                self.end_headers()
            elif 'json' in route:
                entry['status'] = 200
                self._bytes(200, json.dumps(route['json']).encode('utf-8'), 'application/json', body)
            else:
                entry['status'] = 200
                entry['bytes'] = self._file(Path(route['file']), route.get('flip'), body)
        finally:
            mock.record(entry)

    def _bytes(self, status, data, ctype, body):
        self.send_response(status)
        self.send_header('Content-Type', ctype)
        self.send_header('Content-Length', str(len(data)))
        self.end_headers()
        if body:
            self.wfile.write(data)

    def _file(self, path, flip, body):
        size = path.stat().st_size
        self.send_response(200)
        self.send_header('Content-Type', 'application/octet-stream')
        self.send_header('Content-Length', str(size))
        self.end_headers()
        if not body:
            return 0
        sent = 0
        rate = self.server.rate
        started = time.monotonic()
        with path.open('rb') as source:
            while chunk := source.read(64 * 1024):
                if flip is not None and sent <= flip < sent + len(chunk):
                    chunk = bytearray(chunk)
                    chunk[flip - sent] ^= 0x01
                    chunk = bytes(chunk)
                self.wfile.write(chunk)
                sent += len(chunk)
                if rate:
                    ahead = sent / rate - (time.monotonic() - started)
                    if ahead > 0:
                        time.sleep(ahead)
        return sent


class _Proxy(socketserver.BaseRequestHandler):
    def handle(self):
        mock = self.server.mock
        sock = self.request
        sock.settimeout(30)
        reader = sock.makefile('rb', buffering=0)
        try:
            request_line = reader.readline(8192).decode('latin-1').strip()
            headers = []
            while True:
                line = reader.readline(8192)
                if line in (b'\r\n', b'\n', b''):
                    break
                headers.append(line.decode('latin-1').strip())
        except OSError:
            return
        parts = request_line.split()
        if len(parts) != 3:
            return
        method, target, _ = parts
        if method != 'CONNECT':
            # Klartext über den Proxy wird nie bedient (OUT-252: kein HTTP).
            mock.record(dict(kind='http', method=method, target=target, status=403, scenario=mock.scenario))
            sock.sendall(b'HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n')
            return
        host, _, port = target.rpartition(':')
        if host not in ALLOWED_HOSTS or port != '443':
            mock.record(dict(kind='connect-blocked', target=target, status=403, scenario=mock.scenario))
            sock.sendall(b'HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n')
            return
        mock.record(dict(kind='connect', target=target, scenario=mock.scenario))
        sock.sendall(b'HTTP/1.1 200 Connection Established\r\n\r\n')
        try:
            tls = mock.context.wrap_socket(sock, server_side=True)
        except (ssl.SSLError, OSError) as error:
            mock.record(dict(kind='tls-error', target=target, error=type(error).__name__, scenario=mock.scenario))
            return
        try:
            _Inner(tls, self.client_address, mock)
        except (OSError, ssl.SSLError):
            pass
        finally:
            try:
                tls.close()
            except OSError:
                pass


class _ThreadingServer(socketserver.ThreadingTCPServer):
    daemon_threads = True
    allow_reuse_address = True


class MockUpdateServer:
    """Proxy mit TLS-Endpunkt für die GitHub-Hosts; Szenario umschaltbar, Requests protokolliert."""

    def __init__(self, scenarios, certs, scenario='valid', host='127.0.0.1', port=0, rate=None, log_file=None):
        data = scenarios if isinstance(scenarios, dict) else json.loads(Path(scenarios).read_text(encoding='utf-8'))
        self.info = data
        self.scenarios = data['scenarios']
        self.scenario = scenario
        self.rate = rate
        self.log_file = Path(log_file) if log_file else None
        self._log = []
        self._lock = threading.Lock()
        self.context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
        self.context.minimum_version = ssl.TLSVersion.TLSv1_2
        self.context.set_alpn_protocols(['http/1.1'])
        self.context.load_cert_chain(Path(certs) / 'server.pem', Path(certs) / 'server.key')
        self._server = _ThreadingServer((host, port), _Proxy)
        self._server.mock = self
        self._thread = None

    @property
    def port(self):
        return self._server.server_address[1]

    def start(self):
        self._thread = threading.Thread(target=self._server.serve_forever, daemon=True)
        self._thread.start()
        return self

    def stop(self):
        self._server.shutdown()
        self._server.server_close()

    def __enter__(self):
        return self.start()

    def __exit__(self, *exc):
        self.stop()

    def use(self, scenario):
        if scenario not in self.scenarios:
            raise KeyError(scenario)
        self.scenario = scenario

    def record(self, entry):
        entry = dict(entry, at=time.time())
        with self._lock:
            self._log.append(entry)
            if self.log_file:
                with self.log_file.open('a', encoding='utf-8') as out:
                    out.write(json.dumps(entry) + '\n')

    def log(self, since=0):
        with self._lock:
            return list(self._log[since:])

    def mark(self):
        with self._lock:
            return len(self._log)


# --- Client wie die App (für Nachbildung und simulierten Download) ------------------------------
def fetch(url, proxy, cafile, out=None, user_agent=USER_AGENT, timeout=60):
    """GET über ``CONNECT`` am Proxy mit Prüfung gegen die Test-CA; liefert (Status, Bytes)."""
    parts = urlsplit(url)
    if parts.scheme != 'https':
        raise ValueError('Nur HTTPS')
    proxy_host, _, proxy_port = proxy.rpartition(':')
    context = ssl.create_default_context(cafile=cafile)
    conn = http.client.HTTPSConnection(proxy_host, int(proxy_port), context=context, timeout=timeout)
    conn.set_tunnel(parts.hostname, parts.port or 443)
    try:
        conn.request('GET', parts.path + (f'?{parts.query}' if parts.query else ''),
                     headers={'User-Agent': user_agent, 'Accept': 'application/octet-stream'})
        response = conn.getresponse()
        if out is None:
            return response.status, response.read()
        size = 0
        with Path(out).open('wb') as target:
            while chunk := response.read(256 * 1024):
                target.write(chunk)
                size += len(chunk)
        return response.status, size
    finally:
        conn.close()


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest='command', required=True)
    c = sub.add_parser('certs')
    c.add_argument('--out', type=Path, required=True)
    p = sub.add_parser('prepare')
    p.add_argument('--current', type=Path, required=True, help='release-assets von Version N')
    p.add_argument('--next', type=Path, required=True, help='release-assets von Version N+1')
    p.add_argument('--out', type=Path, required=True)
    s = sub.add_parser('serve')
    s.add_argument('--scenarios', type=Path, required=True)
    s.add_argument('--certs', type=Path, required=True)
    s.add_argument('--scenario', default='valid', choices=SCENARIOS)
    s.add_argument('--port', type=int, default=0)
    s.add_argument('--rate', type=float, help='Bytes je Sekunde für Artefakte (gedrosselter Download)')
    s.add_argument('--log', type=Path)
    s.add_argument('--port-file', type=Path)
    f = sub.add_parser('fetch')
    f.add_argument('url')
    f.add_argument('--proxy', required=True)
    f.add_argument('--ca', required=True)
    f.add_argument('--out', type=Path, required=True)
    args = parser.parse_args(argv)
    if args.command == 'certs':
        make_certs(args.out)
        print(f'Test-CA unter {args.out} (ca.pem, server.pem, server.key)')
    elif args.command == 'prepare':
        info = prepare(args.current, args.next, args.out)
        print(json.dumps({k: info[k] for k in ('repository', 'current', 'next', 'channel')}))
    elif args.command == 'serve':
        server = MockUpdateServer(args.scenarios, args.certs, args.scenario, port=args.port, rate=args.rate,
                                  log_file=args.log).start()
        if args.port_file:
            args.port_file.write_text(str(server.port), encoding='ascii')
        print(f'Mock-Update-Proxy auf 127.0.0.1:{server.port}, Szenario {args.scenario}', flush=True)
        try:
            while True:
                time.sleep(3600)
        except KeyboardInterrupt:
            server.stop()
    elif args.command == 'fetch':
        status, size = fetch(args.url, args.proxy, args.ca, args.out)
        print(f'{status} {size} Bytes')
        return 0 if status == 200 else 1
    return 0


if __name__ == '__main__':
    sys.exit(main())
