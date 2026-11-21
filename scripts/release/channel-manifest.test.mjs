// spec: OUT-254
// spec: OUT-261
// Vertragstests für das Beta-Manifest am festen Tag update-beta (K3-16): Entscheidungsregel, Eingabeprüfungen,
// Kommandozeile und der Workflow „Kanal-Manifest aktualisieren“.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, writeFileSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { decide, checkSourceRelease, checkHelperRelease, checkManifest, HELPER_TAG } from './channel-manifest.mjs';

const root = resolve(import.meta.dirname, '../..');
const script = join(root, 'scripts/release/channel-manifest.mjs');
const repository = 'wojcikdominik/openspider';

function manifest(version, extra = {}) {
  const url = name => `https://github.com/${repository}/releases/download/v${version}/${name}`;
  return {
    version, notes: 'Fixture', pub_date: '2026-11-21T10:00:00.000Z', ...extra,
    platforms: {
      'darwin-aarch64': { signature: 'c2ln', url: url('OpenSpider_aarch64.app.tar.gz') },
      'windows-x86_64-nsis': { signature: 'c2ln', url: url(`OpenSpider_${version}_x64-setup.exe`) },
    },
  };
}
const release = (tagName, assets, extra = {}) => ({ tagName, isDraft: false, isPrerelease: false, assets: assets.map(name => ({ name })), ...extra });
const helper = (extra = {}) => ({ tagName: HELPER_TAG, isDraft: false, isPrerelease: true, assets: [{ name: 'latest-beta.json' }], ...extra });

// spec: OUT-254
test('Regel: nach einer Beta ersetzen, nach einer stabilen Version nur wenn neuer, nie zurück', () => {
  const cases = [
    // [neu veröffentlicht, bisheriger Inhalt, erwartet]
    ['1.1.0-beta.1', '1.0.1', 'replace'],
    ['1.1.0-beta.2', '1.1.0-beta.1', 'replace'],
    ['1.1.0-rc.1', '1.1.0-beta.9', 'replace'],
    ['1.1.0', '1.1.0-rc.1', 'replace'],
    ['1.0.2', '1.0.1', 'replace'],
    ['1.0.2', '1.1.0-beta.1', 'skip'],
    ['1.0.1', '1.0.1', 'skip'],
  ];
  for (const [version, current, action] of cases) {
    const candidate = manifest(version);
    const before = version === current ? manifest(current) : manifest(current, { notes: 'alt' });
    assert.equal(decide({ candidate, current: before, version }).action, action, `${version} über ${current}`);
  }
  // Ältere Beta: nie zurücksetzen.
  assert.throws(() => decide({ candidate: manifest('1.1.0-beta.1'), current: manifest('1.1.0'), version: '1.1.0-beta.1' }), /nie zurücksetzen/);
  assert.throws(() => decide({ candidate: manifest('1.1.0-beta.1'), current: manifest('1.1.0-beta.2'), version: '1.1.0-beta.1' }), /nie zurücksetzen/);
  // Gleiche Version, anderer Inhalt: kein automatisches Überschreiben.
  assert.throws(() => decide({ candidate: manifest('1.0.1'), current: manifest('1.0.1', { notes: 'anders' }), version: '1.0.1' }), /anderem Inhalt/);
  // Ohne bisheriges Manifest legt der Workflow nichts an.
  assert.throws(() => decide({ candidate: manifest('1.0.1'), current: null, version: '1.0.1' }), /einmalig/);
});

// spec: OUT-261
test('Quelle: veröffentlicht, kein Pre-Release, Latest genau dann wenn stabil, Manifeste nach Kanal', () => {
  assert.equal(checkSourceRelease({ release: release('v1.0.1', ['latest.json', 'latest-beta.json', 'SHA256SUMS']), latestTag: 'v1.0.1' }), '1.0.1');
  assert.equal(checkSourceRelease({ release: release('v1.1.0-beta.1', ['latest-beta.json']), latestTag: 'v1.0.1' }), '1.1.0-beta.1');
  const bad = [
    [release('v1.0.1', ['latest.json', 'latest-beta.json'], { isDraft: true }), 'v1.0.1', /Entwurf/],
    [release('v1.1.0-beta.1', ['latest-beta.json'], { isPrerelease: true }), 'v1.0.1', /Pre-Release/],
    [release('v1.1.0-beta.1', ['latest-beta.json']), 'v1.1.0-beta.1', /Latest/],
    [release('v1.0.1', ['latest.json', 'latest-beta.json']), 'v1.0.0', /nicht Latest/],
    [release('v1.1.0-beta.1', ['latest.json', 'latest-beta.json']), 'v1.0.1', /erwartet latest-beta\.json/],
    [release('v1.0.1', ['latest-beta.json']), 'v1.0.1', /erwartet latest-beta\.json, latest\.json/],
    [release(HELPER_TAG, ['latest-beta.json']), 'v1.0.1', /Ungültiger Release-Tag/],
  ];
  for (const [source, latestTag, message] of bad) assert.throws(() => checkSourceRelease({ release: source, latestTag }), message);
});

// spec: OUT-254
test('Hilfs-Release update-beta: Pre-Release, nie Latest, veröffentlicht, nur latest-beta.json', () => {
  checkHelperRelease({ release: helper(), latestTag: 'v1.0.1' });
  assert.throws(() => checkHelperRelease({ release: helper({ isPrerelease: false }), latestTag: 'v1.0.1' }), /Pre-Release/);
  assert.throws(() => checkHelperRelease({ release: helper({ isDraft: true }), latestTag: 'v1.0.1' }), /Entwurf/);
  assert.throws(() => checkHelperRelease({ release: helper(), latestTag: HELPER_TAG }), /Latest/);
  assert.throws(() => checkHelperRelease({ release: helper({ assets: [{ name: 'latest-beta.json' }, { name: 'latest.json' }] }), latestTag: '' }), /genau latest-beta\.json/);
  assert.throws(() => checkHelperRelease({ release: helper({ tagName: 'v1.0.1' }), latestTag: '' }), /Tag update-beta/);
});

// spec: OUT-254
test('Manifest: Version des Releases, Paket-URLs auf releases/download/v<Version>/', () => {
  checkManifest(manifest('1.0.1'), '1.0.1', repository);
  assert.throws(() => checkManifest(manifest('1.0.0'), '1.0.1', repository), /erwartet 1\.0\.1/);
  const latestUrl = manifest('1.0.1');
  latestUrl.platforms['darwin-aarch64'].url = `https://github.com/${repository}/releases/latest/download/OpenSpider_aarch64.app.tar.gz`;
  assert.throws(() => checkManifest(latestUrl, '1.0.1', repository), /Paket-URL darwin-aarch64/);
  assert.throws(() => checkManifest({ ...manifest('1.0.1'), platforms: {} }, '1.0.1', repository), /keine Plattformen/);
});

function cli(t, { version, latestTag, current, stable = true, stableContent }) {
  const dir = mkdtempSync(join(tmpdir(), 'openspider-channel-'));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  const beta = version.includes('-');
  const files = beta ? ['latest-beta.json'] : ['latest.json', 'latest-beta.json'];
  const text = `${JSON.stringify(manifest(version), null, 2)}\n`;
  writeFileSync(join(dir, 'release.json'), JSON.stringify(release(`v${version}`, files)));
  writeFileSync(join(dir, 'helper.json'), JSON.stringify(helper()));
  writeFileSync(join(dir, 'candidate.json'), text);
  writeFileSync(join(dir, 'stable.json'), stableContent ?? text);
  writeFileSync(join(dir, 'current.json'), `${JSON.stringify(manifest(current), null, 2)}\n`);
  const args = ['--repository', repository, '--release', join(dir, 'release.json'), '--helper', join(dir, 'helper.json'),
    '--latest-tag', latestTag, '--candidate', join(dir, 'candidate.json'), '--current', join(dir, 'current.json')];
  if (!beta && stable) args.push('--stable', join(dir, 'stable.json'));
  return spawnSync(process.execPath, [script, ...args], { encoding: 'utf8' });
}

// spec: OUT-254
test('Kommandozeile: Ausgabe für GITHUB_OUTPUT, Fehler mit Exit 1 und ohne Ausgabe', t => {
  let result = cli(t, { version: '1.1.0-beta.1', latestTag: 'v1.0.1', current: '1.0.1' });
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /^action=replace\nversion=1\.1\.0-beta\.1\nreason=.+\n$/);
  result = cli(t, { version: '1.0.2', latestTag: 'v1.0.2', current: '1.1.0-beta.1' });
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /^action=skip\n/);
  result = cli(t, { version: '1.1.0', latestTag: 'v1.1.0', current: '1.1.0-rc.1' });
  assert.match(result.stdout, /^action=replace\n/);
  // Stabil ohne latest.json bzw. mit abweichendem latest.json: Abbruch.
  result = cli(t, { version: '1.0.2', latestTag: 'v1.0.2', current: '1.0.1', stable: false });
  assert.equal(result.status, 1);
  assert.equal(result.stdout, '');
  assert.match(result.stderr, /--stable fehlt/);
  result = cli(t, { version: '1.0.2', latestTag: 'v1.0.2', current: '1.0.1', stableContent: '{}\n' });
  assert.equal(result.status, 1);
  assert.match(result.stderr, /unterscheiden sich/);
  result = cli(t, { version: '1.1.0-beta.1', latestTag: 'v1.0.1', current: '1.1.0' });
  assert.equal(result.status, 1);
  assert.equal(result.stdout, '');
});

// Workflow .github/workflows/channel-manifest.yml
const workflow = readFileSync(join(root, '.github/workflows/channel-manifest.yml'), 'utf8');
const stepBody = name => {
  const body = workflow.split(`      - name: ${name}\n`)[1];
  assert.ok(body, `Schritt ${name} fehlt`);
  return body.split('\n      - ')[0];
};

// spec: OUT-254
test('Workflow: nur workflow_dispatch, nur main, Environment release, Schreibrecht nur im Job', () => {
  assert.match(workflow, /^name: Kanal-Manifest aktualisieren$/m);
  const on = workflow.split('\non:\n')[1].split(/\n[a-z]/)[0];
  assert.deepEqual([...on.matchAll(/^ {2}([a-z_]+):/gm)].map(m => m[1]), ['workflow_dispatch']);
  assert.match(workflow, /^permissions:\n {2}contents: read$/m);
  assert.equal(workflow.split('contents: write').length - 1, 1);
  assert.match(workflow, /^ {4}environment: release$/m);
  assert.match(workflow, /^ {4}if: github\.ref == 'refs\/heads\/main'$/m);
  for (const line of workflow.split('\n').filter(l => /^\s+(?:- )?uses: /.test(l))) {
    assert.match(line, /uses: [\w.-]+\/[\w.-]+@[0-9a-f]{40} # v\d/, line);
  }
  assert.doesNotMatch(workflow, /secrets\./);
});

// spec: OUT-254
// spec: OUT-261
test('Workflow: ersetzt nur latest-beta.json am Tag update-beta, nur nach der Entscheidung, legt nichts an', () => {
  const uploads = workflow.split('\n').filter(l => /gh release (upload|create|edit|delete)/.test(l));
  assert.deepEqual(uploads.map(l => l.trim()), ['gh release upload update-beta "$RUNNER_TEMP/channel/release/latest-beta.json" --clobber']);
  assert.match(stepBody('latest-beta.json am Tag update-beta ersetzen'), /^ {8}if: steps\.decide\.outputs\.action == 'replace'$/m);
  assert.doesNotMatch(workflow, /gh api [^\n]*(-X|--method) (POST|PATCH|PUT|DELETE)/);
  const decideStep = stepBody('Entscheiden (OUT-254)');
  assert.match(decideStep, /node scripts\/release\/channel-manifest\.mjs/);
  assert.match(decideStep, /GITHUB_OUTPUT/);
  const check = stepBody('Prüfschritt nach der Aktualisierung');
  assert.match(check, /cmp -s/);
  assert.match(check, /releases\/latest/);
});
