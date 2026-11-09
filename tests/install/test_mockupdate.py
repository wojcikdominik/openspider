"""Tests für den Mock-Update-Server (M3-Q02): Szenarien, Proxy, TLS mit Test-CA, Protokoll."""
import json
import shutil
import socket
import ssl
import tempfile
import time
import unittest
from pathlib import Path

import mockupdate
import tauri_sig

ROOT = Path(__file__).resolve().parents[2]
FIXTURES = ROOT / 'tests' / 'fixtures' / 'update'
REPO = 'wojcikdominik/openspider'
NAMES = {
    'darwin-aarch64': 'OpenSpider_aarch64.app.tar.gz',
    'darwin-x86_64': 'OpenSpider_x86_64.app.tar.gz',
    'windows-x86_64-nsis': 'OpenSpider_{version}_x64-setup.exe',
    'windows-x86_64-msi': 'OpenSpider_{version}_x64_en-US.msi',
}


def release_assets(folder, version='1.0.1', signature=None, platforms=NAMES, installers=()):
    """Nachbau eines ``release-assets``-Ordners; das aarch64-Archiv ist die signierte Fixture."""
    folder.mkdir(parents=True)
    signature = signature or (FIXTURES / 'artifact.sig').read_text().strip()
    entries = {}
    for key, name in platforms.items():
        name = name.format(version=version)
        shutil.copyfile(FIXTURES / 'artifact', folder / name)
        (folder / f'{name}.sig').write_text(signature + '\n')
        entries[key] = dict(signature=signature, url=f'https://github.com/{REPO}/releases/download/v{version}/{name}')
    manifest = dict(version=version, notes='Fixture', pub_date='2026-11-09T10:00:00.000Z', platforms=entries)
    for name in ('latest.json', 'latest-beta.json'):
        (folder / name).write_text(json.dumps(manifest, indent=2))
    for name in installers:
        (folder / name).write_bytes(b'installer')
    return manifest


class Fixture(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.tmp = Path(tempfile.mkdtemp(prefix='q02-test-'))
        cls.certs = mockupdate.make_certs(cls.tmp / 'certs')
        release_assets(cls.tmp / 'n', '1.0.0', installers=['OpenSpider_1.0.0_universal.dmg'])
        (cls.tmp / 'n' / 'latest.json').unlink()
        (cls.tmp / 'n' / 'latest-beta.json').unlink()
        release_assets(cls.tmp / 'n1', '1.0.1')
        cls.info = mockupdate.prepare(cls.tmp / 'n', cls.tmp / 'n1', cls.tmp / 'mock')

    @classmethod
    def tearDownClass(cls):
        shutil.rmtree(cls.tmp, ignore_errors=True)


class Prepare(Fixture):
    def test_versions_repository_and_channel(self):
        self.assertEqual((self.info['current'], self.info['next'], self.info['repository'], self.info['channel']),
                         ('1.0.0', '1.0.1', REPO, 'stable'))
        self.assertEqual(self.info['endpoints'], [f'/{REPO}/releases/latest/download/latest.json',
                                                  f'/{REPO}/releases/latest/download/latest-beta.json'])
        self.assertTrue((self.tmp / 'mock' / 'scenarios.json').is_file())

    # spec: OUT-252 [M3]
    def test_scenarios(self):
        s = self.info['scenarios']
        self.assertEqual(set(s), set(mockupdate.SCENARIOS))
        endpoint = self.info['endpoints'][0]
        self.assertEqual(s['valid'][endpoint]['json']['version'], '1.0.1')
        self.assertEqual(s['downgrade'][endpoint]['json']['version'], '1.0.0')
        self.assertTrue(all(e['url'].startswith('http://github.com/')
                            for e in s['http-artifact'][endpoint]['json']['platforms'].values()))
        self.assertEqual(s['http-redirect'][endpoint]['location'], f'http://github.com{endpoint}')
        artifact = self.info['artifacts']['darwin-aarch64']
        self.assertIsNone(s['valid'][artifact['request_path']]['flip'])
        self.assertEqual(s['tampered'][artifact['request_path']]['flip'], artifact['size'] // 2)

    def test_rejects_unusable_inputs(self):
        cases = {
            'nicht neuer': dict(version='1.0.0'),
            'Plattformen': dict(platforms={k: v for k, v in NAMES.items() if k != 'windows-x86_64-msi'}),
        }
        for label, kwargs in cases.items():
            with self.subTest(label), tempfile.TemporaryDirectory() as tmp:
                release_assets(Path(tmp) / 'n1', **kwargs)
                with self.assertRaisesRegex(ValueError, label):
                    mockupdate.prepare(self.tmp / 'n', Path(tmp) / 'n1', Path(tmp) / 'out')
        with tempfile.TemporaryDirectory() as tmp:
            release_assets(Path(tmp) / 'n1')
            (Path(tmp) / 'n1' / f'{NAMES["darwin-x86_64"]}.sig').write_text('other')  # ohne Version im Namen
            with self.assertRaisesRegex(ValueError, 'Signatur'):
                mockupdate.prepare(self.tmp / 'n', Path(tmp) / 'n1', Path(tmp) / 'out')
        with tempfile.TemporaryDirectory() as tmp:
            release_assets(Path(tmp) / 'n1', version='1.0.1-rc.1')
            with self.assertRaisesRegex(ValueError, 'Stabiler Client'):
                mockupdate.prepare(self.tmp / 'n', Path(tmp) / 'n1', Path(tmp) / 'out')
        with tempfile.TemporaryDirectory() as tmp:
            (Path(tmp) / 'n1').mkdir()
            with self.assertRaisesRegex(ValueError, 'ohne signierte Updater-Artefakte'):
                mockupdate.prepare(self.tmp / 'n', Path(tmp) / 'n1', Path(tmp) / 'out')

    def test_semver_order(self):
        order = ['1.0.0-beta.2', '1.0.0-beta.10', '1.0.0-rc.1', '1.0.0-rc.2', '1.0.0', '1.0.1', '1.1.0']
        self.assertEqual(sorted(reversed(order), key=mockupdate.semver_key), order)
        self.assertTrue(mockupdate.is_prerelease('1.0.0-rc.1'))
        self.assertFalse(mockupdate.is_prerelease('1.0.0'))
        with self.assertRaises(ValueError):
            mockupdate.semver_key('1.0')


class Server(Fixture):
    def setUp(self):
        self.server = mockupdate.MockUpdateServer(self.info, self.tmp / 'certs', log_file=self.tmp / 'log.jsonl').start()
        self.proxy = f'127.0.0.1:{self.server.port}'
        self.artifact = self.info['artifacts']['darwin-aarch64']

    def tearDown(self):
        self.server.stop()

    def fetch(self, url, **kwargs):
        return mockupdate.fetch(url, self.proxy, self.certs['ca.pem'], **kwargs)

    # spec: OUT-250 [M3]
    def test_manifest_request_is_logged_with_query_and_user_agent(self):
        url = f'https://github.com{self.info["endpoints"][1]}?version=1.0.0&os=darwin&arch=aarch64'
        status, body = self.fetch(url)
        self.assertEqual(status, 200)
        self.assertEqual(json.loads(body)['version'], '1.0.1')
        entry = [e for e in self.server.log() if e['kind'] == 'https'][0]
        self.assertEqual((entry['host'], entry['path'], entry['user_agent']),
                         ('github.com', self.info['endpoints'][1], 'OpenSpider-Updater'))
        self.assertEqual(entry['query'], {'version': ['1.0.0'], 'os': ['darwin'], 'arch': ['aarch64']})
        self.assertEqual(len((self.tmp / 'log.jsonl').read_text().splitlines()), len(self.server.log()))

    # spec: OUT-252 [M3]
    def test_valid_artifact_verifies_and_tampered_differs_in_exactly_one_byte(self):
        key = (FIXTURES / 'test.key.pub').read_text()
        signature = (FIXTURES / 'artifact.sig').read_text()
        status, valid = self.fetch(self.artifact['url'])
        self.assertEqual(status, 200)
        tauri_sig.verify(valid, signature, key, '1.0.1')
        self.server.use('tampered')
        status, tampered = self.fetch(self.artifact['url'])
        self.assertEqual((status, len(tampered)), (200, len(valid)))
        self.assertEqual(sum(a != b for a, b in zip(valid, tampered)), 1)
        with self.assertRaises(tauri_sig.SignatureError):
            tauri_sig.verify(tampered, signature, key, '1.0.1')
        self.assertEqual([e['bytes'] for e in self.server.log() if e.get('path') == self.artifact['request_path']],
                         [len(valid)] * 2)

    # spec: OUT-252 [M3]
    def test_redirect_and_downgrade_scenarios(self):
        self.server.use('http-redirect')
        conn_status, _ = self.fetch(f'https://github.com{self.info["endpoints"][0]}')
        self.assertEqual(conn_status, 302)
        self.server.use('downgrade')
        _, body = self.fetch(f'https://github.com{self.info["endpoints"][0]}')
        self.assertEqual(json.loads(body)['version'], '1.0.0')
        self.server.use('http-artifact')
        _, body = self.fetch(f'https://github.com{self.info["endpoints"][0]}')
        self.assertTrue(json.loads(body)['platforms']['darwin-aarch64']['url'].startswith('http://'))
        with self.assertRaises(KeyError):
            self.server.use('unknown')

    def raw(self, request):
        with socket.create_connection(('127.0.0.1', self.server.port), timeout=5) as sock:
            sock.sendall(request)
            return sock.recv(1024)

    # spec: OUT-252 [M3]
    def test_plain_http_and_foreign_hosts_are_refused_and_logged(self):
        self.assertIn(b'403', self.raw(b'GET http://github.com/x HTTP/1.1\r\nHost: github.com\r\n\r\n'))
        self.assertIn(b'403', self.raw(b'CONNECT example.org:443 HTTP/1.1\r\nHost: example.org:443\r\n\r\n'))
        self.assertIn(b'403', self.raw(b'CONNECT github.com:80 HTTP/1.1\r\nHost: github.com:80\r\n\r\n'))
        kinds = [e['kind'] for e in self.server.log()]
        self.assertEqual(kinds, ['http', 'connect-blocked', 'connect-blocked'])

    def test_unknown_path_is_404_and_client_with_other_ca_fails(self):
        status, _ = self.fetch('https://github.com/other/path')
        self.assertEqual(status, 404)
        with tempfile.TemporaryDirectory() as tmp:
            other = mockupdate.make_certs(Path(tmp))
            with self.assertRaises(ssl.SSLCertVerificationError):
                mockupdate.fetch(self.artifact['url'], self.proxy, other['ca.pem'])
        deadline = time.monotonic() + 5
        while 'tls-error' not in [e['kind'] for e in self.server.log()] and time.monotonic() < deadline:
            time.sleep(0.05)
        self.assertIn('tls-error', [e['kind'] for e in self.server.log()])

    def test_rate_limit_throttles_artifacts(self):
        big = self.tmp / 'big.bin'
        big.write_bytes(bytes(256 * 1024))
        self.server.scenarios['big'] = {'/big.bin': dict(status=200, file=str(big), flip=None)}
        self.server.use('big')
        self.server.rate = 512 * 1024
        started = time.monotonic()
        status, size = self.fetch('https://github.com/big.bin', out=self.tmp / 'big.out')
        self.assertEqual((status, size), (200, 256 * 1024))
        # Der letzte Block geht ohne Wartezeit hinaus: mindestens (Größe − Block) / Rate.
        self.assertGreaterEqual(time.monotonic() - started, (192 * 1024) / (512 * 1024) * 0.9)


if __name__ == '__main__':
    unittest.main()
