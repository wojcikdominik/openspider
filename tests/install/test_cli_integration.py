"""Echte CLI gegen den Mock-Update-Server (M3-Q02), ohne Installation.

Läuft nur mit ``Q02_CLI=<pfad zu openspider>``; das Binary muss mit
``OPENSPIDER_UPDATER_PUBKEY=$(cat tests/fixtures/update/test.key.pub)`` gebaut sein (stabiler Kanal). Belegt vor dem
ersten CI-Lauf, dass Testprofil (Proxy, Test-CA), Mock und Prüfungen der Matrix zum Update-Kern passen.
"""
import os
import platform
import re
import shutil
import subprocess
import tempfile
import time
import unittest
from pathlib import Path

import mockupdate
import update_matrix
from test_mockupdate import NAMES, release_assets

CLI = os.environ.get('Q02_CLI')


@unittest.skipUnless(CLI, 'Q02_CLI nicht gesetzt')
class RealCli(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.tmp = Path(tempfile.mkdtemp(prefix='q02-cli-'))
        version = subprocess.run([CLI, '--version'], capture_output=True, text=True, check=True).stdout
        cls.current = re.search(r'\d+\.\d+\.\d+(?:-[0-9A-Za-z.]+)?', version).group(0)
        certs = mockupdate.make_certs(cls.tmp / 'certs')
        (cls.tmp / 'n').mkdir()
        # Der Kern fragt den Schlüssel des Hosts an (``UpdateContext::target``); Linux steht nicht im Release-Manifest.
        system = {'Linux': 'linux', 'Darwin': 'darwin', 'Windows': 'windows'}[platform.system()]
        arch = {'amd64': 'x86_64', 'arm64': 'aarch64'}.get(platform.machine().lower(), platform.machine().lower())
        host = f'{system}-{arch}' + ('-nsis' if system == 'windows' else '')
        release_assets(cls.tmp / 'n1', '1.0.1', platforms=dict(NAMES, **{host: f'q02-{host}.bin'}))
        cls.info = mockupdate.prepare(cls.tmp / 'n', cls.tmp / 'n1', cls.tmp / 'mock', current_version=cls.current)
        cls.server = mockupdate.MockUpdateServer(cls.info, cls.tmp / 'certs').start()
        home = cls.tmp / 'home'
        home.mkdir()
        (home / 'settings.toml').write_text(update_matrix.settings_toml(cls.server.port, certs['ca.pem']), encoding='utf-8')
        cls.env = {k: v for k, v in os.environ.items() if k != 'OPENSPIDER_DISABLE_UPDATE_CHECK'}
        cls.env.update(OPENSPIDER_HOME=str(home), OPENSPIDER_SECRET_STORE='memory')

    @classmethod
    def tearDownClass(cls):
        cls.server.stop()
        shutil.rmtree(cls.tmp, ignore_errors=True)

    def check(self, scenario, **env):
        self.server.use(scenario)
        mark = self.server.mark()
        result = subprocess.run([CLI, '--check-update'], env=dict(self.env, **env), capture_output=True, text=True,
                                timeout=60)
        time.sleep(0.3)
        return result, self.server.log(mark)

    # spec: OUT-250 [M3]
    def test_newer_version_exit_10_with_minimal_request(self):
        result, log = self.check('valid')
        self.assertEqual(result.returncode, 10, result.stderr)
        self.assertIn('1.0.1', result.stdout)
        manifests = update_matrix.manifest_requests(log, self.info)
        self.assertEqual(len(manifests), 1)
        self.assertEqual(manifests[0]['path'], self.info['endpoints'][0])  # stabiler Kanal
        update_matrix.assert_privacy(manifests[0], self.current)
        self.assertFalse(update_matrix.artifact_requests(log, self.info) or update_matrix.plain_or_blocked(log))

    # spec: OUT-250 [M3]
    def test_disabled_by_environment_makes_no_request(self):
        result, log = self.check('valid', OPENSPIDER_DISABLE_UPDATE_CHECK='1')
        self.assertNotIn(result.returncode, (0, 10))
        self.assertEqual(log, [])

    # spec: OUT-252 [M3]
    def test_http_artifact_and_http_redirect_are_rejected(self):
        for scenario in ('http-artifact', 'http-redirect'):
            with self.subTest(scenario):
                result, log = self.check(scenario)
                self.assertNotIn(result.returncode, (0, 10), result.stdout)
                self.assertFalse(update_matrix.artifact_requests(log, self.info))
                self.assertFalse(update_matrix.plain_or_blocked(log), log)

    # spec: OUT-252 [M3]
    def test_same_version_is_current(self):
        result, log = self.check('downgrade')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(len(update_matrix.manifest_requests(log, self.info)), 1)


if __name__ == '__main__':
    unittest.main()
