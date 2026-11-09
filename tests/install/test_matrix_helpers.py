"""Tests der plattformunabhängigen Teile von update_matrix.py und crawl_rate.py (M3-Q02)."""
import io
import json
import os
import tempfile
import tomllib
import unittest
from contextlib import redirect_stderr
from pathlib import Path
from unittest import mock

import crawl_rate
import summarize
import update_matrix

INFO = dict(endpoints=['/o/r/releases/latest/download/latest.json', '/o/r/releases/latest/download/latest-beta.json'],
            artifacts={'darwin-aarch64': dict(request_path='/o/r/releases/download/v1.0.1/OpenSpider_aarch64.app.tar.gz')})


def request(query, user_agent='OpenSpider-Updater', path=INFO['endpoints'][1]):
    return dict(kind='https', path=path, query={k: [v] for k, v in query.items()}, user_agent=user_agent)


class Privacy(unittest.TestCase):
    # spec: OUT-250 [M3]
    def test_only_version_os_and_arch(self):
        ok = request(dict(version='1.0.0', os='darwin', arch='aarch64'))
        self.assertEqual(update_matrix.assert_privacy(ok, '1.0.0'), dict(version='1.0.0', os='darwin', arch='aarch64'))
        for bad in (request(dict(version='1.0.0', os='darwin', arch='aarch64', id='x')),
                    request(dict(version='0.9.0', os='darwin', arch='aarch64')),
                    request(dict(version='1.0.0', os='freebsd', arch='aarch64')),
                    request(dict(version='1.0.0', os='windows', arch='x86_64'), user_agent='curl/8')):
            with self.subTest(bad=bad), self.assertRaises(AssertionError):
                update_matrix.assert_privacy(bad, '1.0.0')

    def test_request_filters(self):
        log = [request(dict(version='1')), request({}, path=INFO['artifacts']['darwin-aarch64']['request_path']),
               dict(kind='connect', target='github.com:443'), dict(kind='http', target='http://x/'),
               dict(kind='connect-blocked', target='example.org:443'), request({}, path='/other')]
        self.assertEqual(len(update_matrix.manifest_requests(log, INFO)), 1)
        self.assertEqual(len(update_matrix.artifact_requests(log, INFO)), 1)
        self.assertEqual([e['kind'] for e in update_matrix.plain_or_blocked(log)], ['http', 'connect-blocked'])


class Settings(unittest.TestCase):
    # spec: OUT-250 [M3]
    def test_profile_settings_are_valid_toml_with_app_keys(self):
        for ca in ('/tmp/q02/ca.pem', r'C:\Users\runneradmin\AppData\Local\Temp\q02 ca\ca.pem'):
            data = tomllib.loads(update_matrix.settings_toml(43210, ca))
            self.assertEqual(data['updates'], {'check': True})
            self.assertEqual(data['network']['trusted_certificates'], [ca])
            self.assertEqual(data['network']['proxy'], dict(mode='manual', kind='http', host='127.0.0.1', port=43210,
                                                            bypass=['127.0.0.1', 'localhost']))


class ReportFile(unittest.TestCase):
    def test_failures_are_recorded_and_rendered(self):
        with tempfile.TemporaryDirectory() as tmp:
            report = update_matrix.Report(Path(tmp), dict(label='t', current='1.0.0', next='1.0.1', platform='p',
                                                          target='darwin-aarch64'))
            self.assertTrue(report.check('Q-1', 'ok', 'OUT-250', lambda: {'a': 1}))
            self.assertFalse(report.check('Q-2', 'kaputt', 'OUT-252', lambda: 1 / 0))
            report.note('H-1', 'Hand', 'OUT-251', 'manual', 'M-02')
            data = json.loads((Path(tmp) / 'report.json').read_text())
            self.assertEqual([c['status'] for c in data['checks']], ['passed', 'failed', 'manual'])
            self.assertIn('ZeroDivisionError', (Path(tmp) / 'errors.log').read_text())
            self.assertIn('| Q-2 | kaputt | OUT-252 | failed |', (Path(tmp) / 'report.md').read_text())
            self.assertEqual(len(report.failed), 1)

    def test_refuses_to_touch_a_developer_profile(self):
        env = {k: v for k, v in os.environ.items() if k != 'CI'}
        with mock.patch.dict(os.environ, env, clear=True), redirect_stderr(io.StringIO()) as err, \
                self.assertRaises(SystemExit):
            update_matrix.main(['--installer', 'dmg', '--current', 'a', '--next', 'b', '--certs', 'c',
                                '--testsite', 'd', '--output', 'e'])
        self.assertIn('CI=true', err.getvalue())


class CrawlRate(unittest.TestCase):
    # spec: OUT-250 [M3]
    # spec: CRAWL-142 [M3]
    def test_evaluation_follows_the_measurement_recipe(self):
        passed = crawl_rate.evaluate([100, 101, 99], [97, 98, 96.5])
        self.assertEqual(passed['verdict'], 'bestanden')
        self.assertAlmostEqual(passed['deviation'], 0.03)
        self.assertEqual(crawl_rate.evaluate([100, 100, 100], [94, 94, 94])['verdict'], 'nicht bestanden')
        self.assertEqual(crawl_rate.evaluate([100, 100, 100], [105, 105, 105])['verdict'], 'bestanden')
        noisy = crawl_rate.evaluate([90, 100, 110], [100, 100, 100])
        self.assertEqual(noisy['verdict'], 'nicht auswertbar')


class Summary(unittest.TestCase):
    def test_matrix_table_and_missing_rows(self):
        with tempfile.TemporaryDirectory() as tmp:
            for label, status in (('macOS arm64', 'passed'), ('Windows x64 MSI', 'failed')):
                folder = Path(tmp) / label
                folder.mkdir()
                checks = [dict(id='Q-01', title='N installieren', spec='OUT-266', status='passed', detail=''),
                          dict(id='Q-18', title='Version N+1', spec='OUT-251', status=status, detail='CLI meldet 1.0.0')]
                (folder / 'report.json').write_text(json.dumps(dict(label=label, current='1.0.0-beta.4',
                                                                    next='1.0.0-rc.1', checks=checks)))
            text, ok = summarize.render(summarize.load(tmp), ['macOS arm64', 'Windows x64 MSI', 'Windows 11 ARM'])
        self.assertFalse(ok)
        self.assertIn('| Q-18 | Version N+1 | OUT-251 | ✔ | ✘ |', text)
        self.assertIn('Ohne Ergebnis (Job abgebrochen oder übersprungen): Windows 11 ARM', text)
        self.assertIn('- Windows x64 MSI Q-18 Version N+1: CLI meldet 1.0.0', text)
        self.assertEqual(summarize.render([], [])[1], False)


if __name__ == '__main__':
    unittest.main()
