import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, writeFileSync, readFileSync, readdirSync, mkdirSync, rmSync, copyFileSync, cpSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { manifests } from '../../scripts/release/manifest.mjs';
import { checksums } from '../../scripts/release/checksums.mjs';
import { releaseNotes, releaseBody, checkNotes, versionNotes, withChangelogSection, feedbackLink, notesPath, changelogSection, updateNotes, splitNotesDocument, compareVersions, FIRST_SHORT_NOTES } from '../../scripts/release/notes.mjs';
import { downloads, isPublished } from '../../scripts/release/assets.mjs';

const root = resolve(import.meta.dirname, '../..');
const releaseWorkflow = () => readFileSync(join(root, '.github/workflows/release.yml'), 'utf8');
// Shell body of a named workflow step, dedented like GitHub Actions runs it.
const step = name => releaseWorkflow().split(`      - name: ${name}\n`)[1]
  .split('        run: |\n')[1].split('      - ')[0]
  .split('\n').map(line => line.startsWith('          ') ? line.slice(10) : line).join('\n');
const fixtures = ['OpenSpider_aarch64.app.tar.gz', 'OpenSpider_x86_64.app.tar.gz',
  'OpenSpider_1.0.0-beta.0_x64-setup.exe', 'OpenSpider_1.0.0-beta.0_x64_en-US.msi'];
// Public test-only, format-valid signature envelope; no claim of cryptographic verification.
const signature = Buffer.from('untrusted comment: fixture\nRWRmaXh0dXJl\ntrusted comment: fixture\nZml4dHVyZQ==\n').toString('base64');
function fixture(t) {
  const directory = mkdtempSync(join(tmpdir(), 'openspider-release-'));
  t.after(() => rmSync(directory, { recursive: true, force: true }));
  for (const name of fixtures) {
    writeFileSync(join(directory, name), `fixture:${name}`);
    writeFileSync(join(directory, `${name}.sig`), signature);
  }
  return { directory, version: '1.0.0-beta.0', repository: 'example/openspider', notes: 'Fixture notes', date: '2026-10-30T00:00:00Z' };
}

// spec: OUT-261 [M3]
test('Manifest liefert pro Architektur und Windows-Installer genau das passende signierte Asset', t => {
  const options = fixture(t);
  manifests(options);
  const manifest = JSON.parse(readFileSync(join(options.directory, 'latest-beta.json')));
  assert.deepEqual(Object.keys(manifest.platforms), ['darwin-aarch64', 'darwin-x86_64', 'windows-x86_64-nsis', 'windows-x86_64-msi']);
  Object.values(manifest.platforms).forEach((entry, i) => {
    assert.equal(entry.signature, signature);
    assert.equal(entry.url, `https://github.com/example/openspider/releases/download/v1.0.0-beta.0/${fixtures[i]}`);
  });
  assert.equal(readFileSync(join(options.directory, 'latest.json'), 'utf8'), readFileSync(join(options.directory, 'latest-beta.json'), 'utf8'));
});

test('Stabiler Release aktualisiert auch den Beta-Endpunkt', t => {
  const options = fixture(t);
  manifests({ ...options, version: '1.0.0' });
  for (const file of ['latest.json', 'latest-beta.json']) {
    assert.equal(JSON.parse(readFileSync(join(options.directory, file))).version, '1.0.0');
  }
});

test('Ohne Secrets grün und unsigniert, ohne leere oder veraltete Manifeste', t => {
  const options = fixture(t);
  manifests(options);
  rmSync(join(options.directory, `${fixtures[0]}.sig`));
  assert.throws(() => manifests(options), /fehlen/);
  assert.deepEqual(manifests({ ...options, allowUnsigned: true }), { status: 'unsigniert', missing: ['darwin-aarch64'], files: [] });
  assert.ok(!readdirSync(options.directory).some(n => n.startsWith('latest')));
});

test('Mehrdeutige Installer und ungültige Signaturen werden abgelehnt', t => {
  const options = fixture(t);
  writeFileSync(join(options.directory, 'Other_x64-setup.exe'), 'duplicate');
  assert.throws(() => manifests(options), /Mehrdeutig/);
  rmSync(join(options.directory, 'Other_x64-setup.exe'));
  writeFileSync(join(options.directory, `${fixtures[0]}.sig`), 'invalid');
  assert.throws(() => manifests({ ...options, allowUnsigned: true }), /Signatur/);
});

test('SHA256SUMS erfasst jedes Asset einschließlich Manifesten und Signaturen reproduzierbar', async t => {
  const options = fixture(t);
  manifests(options);
  const result = await checksums(options.directory);
  assert.equal(await checksums(options.directory), result);
  const lines = result.trim().split('\n');
  assert.equal(lines.length, readdirSync(options.directory).length - 1);
  for (const line of lines) {
    const [hash, name] = line.split('  ');
    assert.equal(hash, createHash('sha256').update(readFileSync(join(options.directory, name))).digest('hex'));
  }
});

test('Release-Notes wählen ausschließlich die angeforderte Changelog-Sektion', () => {
  assert.equal(releaseNotes('# Log\n## [Unreleased]\nfuture\n## [1.0.0-beta.0] - today\nBeta\n## [0.1.0]\nold', '1.0.0-beta.0'), 'Beta\n');
  assert.throws(() => releaseNotes('## [Unreleased]\nfuture', '1.0.0'), /fehlt/);
});

// spec: OUT-273 [M3]
test('Linux-CLI-Archiv aus linux-core bleibt unverändert im gesammelten Release und in SHA256SUMS', async t => {
  const options = fixture(t);
  const incoming = join(options.directory, 'incoming');
  const output = join(options.directory, 'out');
  mkdirSync(incoming);
  const archive = 'openspider_1.0.0-beta.0_linux-x86_64.tar.gz';
  // Real tar fixture; the workflow separately executes the built/extracted Linux binary.
  writeFileSync(join(incoming, 'openspider'), '#!/bin/sh\necho openspider 1.0.0-beta.0\n', { mode: 0o755 });
  assert.equal(spawnSync('tar', ['-czf', join(incoming, archive), '-C', incoming, 'openspider']).status, 0);
  writeFileSync(join(incoming, 'verify-app.log'), 'not distributable');
  const result = spawnSync(process.execPath, ['scripts/release/assets.mjs', incoming, output, options.version], { cwd: root, encoding: 'utf8' });
  assert.equal(result.status, 0, result.stderr);
  assert.deepEqual(readdirSync(output), [archive]);
  assert.deepEqual(readFileSync(join(output, archive)), readFileSync(join(incoming, archive)));
  assert.match(await checksums(output), new RegExp(archive.replaceAll('.', '\\.')));
  const extracted = join(options.directory, 'extracted');
  mkdirSync(extracted);
  assert.equal(spawnSync('tar', ['-xzf', join(output, archive), '-C', extracted]).status, 0);
  assert.equal(spawnSync(join(extracted, 'openspider'), ['--version'], { encoding: 'utf8' }).stdout.trim(), 'openspider 1.0.0-beta.0');
});

test('Asset-Kollisionen werden abgelehnt statt Signaturen oder Installer zu überschreiben', t => {
  const options = fixture(t);
  const nested = join(options.directory, 'nested');
  mkdirSync(nested);
  writeFileSync(join(nested, fixtures[0]), 'collision');
  const output = join(options.directory, 'out');
  // Output must not be nested inside input in production; use a separate sibling here.
  const result = spawnSync(process.execPath, ['scripts/release/assets.mjs', options.directory, `${output}-external`, options.version], { cwd: root, encoding: 'utf8' });
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /Kollision/);
});

// spec: OUT-255 [M3]
test('D01 archiviert mit dem Release-Binary vor den Prüfsummen und ohne Fallback', () => {
  const workflow = releaseWorkflow();
  const hook = workflow.indexOf('      - name: Archiv-Kompatibilitäts-Fixture (D01)');
  assert.ok(hook > 0);
  assert.ok(hook < workflow.indexOf('      - name: SHA256SUMS über alle Release-Assets'));
  const body = workflow.slice(hook, workflow.indexOf('      - name: SHA256SUMS', hook));
  assert.match(body, /node scripts\/release\/archive-compat-fixture\.mjs --version "\$VERSION" --out release-reports\/compat --cli/);
  assert.match(body, /release-assets\/compat_\$\{VERSION\}\.tar\.gz/);
  assert.doesNotMatch(body, /STUB|if \[\[ -f/);
  // The open-parity hint goes to the Actions summary, never into the release text.
  assert.doesNotMatch(body, /release-notes\.md/);
  assert.match(body, /TODO: Offene M2-Paritätspakete[^\n]*>> "\$GITHUB_STEP_SUMMARY"/);
});

test('Release-Erstellung bleibt Draft, normale Beta, kein Latest und Trockenlauf standardmäßig an', () => {
  const workflow = readFileSync(join(root, '.github/workflows/release.yml'), 'utf8');
  assert.match(workflow, /--verify-tag --draft --prerelease=false --latest=false/);
  assert.match(workflow, /if: github.event_name == 'push' \|\| !inputs.dry_run/);
  assert.match(workflow, /dry_run:[\s\S]*?default: true/);
  for (const line of workflow.split('\n').filter(line => line.includes('uses:') && !line.includes('uses: ./'))) {
    assert.match(line, /@[0-9a-f]{40} # v\d/);
  }
});

test('Trockenlauf v1.0.0-beta.0: Skript-CLIs erzeugen beide Manifeste, Notes und vollständig prüfbare Assets', async t => {
  const options = fixture(t);
  const notes = join(options.directory, 'notes.md');
  function run(script, ...args) {
    const result = spawnSync(process.execPath, [`scripts/release/${script}.mjs`, ...args], { cwd: root, encoding: 'utf8' });
    assert.equal(result.status, 0, result.stderr);
    return result;
  }
  run('notes', options.version, notes);
  const expected = readFileSync(notes, 'utf8');
  rmSync(notes);
  const outsideNotes = join(options.directory, '..', `${options.directory.split('/').at(-1)}-notes.md`);
  t.after(() => rmSync(outsideNotes, { force: true }));
  writeFileSync(outsideNotes, expected);
  const status = run('manifest', options.directory, options.version, options.repository, outsideNotes, options.date);
  assert.match(status.stdout, /signierte Updater-Artefakte/);
  assert.ok(!readdirSync(options.directory).includes('updater-status.txt'));
  run('checksums', options.directory);
  assert.equal(JSON.parse(readFileSync(join(options.directory, 'latest-beta.json'))).notes, expected);
  assert.equal(JSON.parse(readFileSync(join(options.directory, 'latest.json'))).version, '1.0.0-beta.0');
  const verification = spawnSync('sha256sum', ['-c', 'SHA256SUMS'], { cwd: options.directory, encoding: 'utf8' });
  assert.equal(verification.status, 0, verification.stderr);
  assert.equal(verification.stdout.trim().split('\n').length, 10);
});

// spec: OUT-273 [M3]
// spec: OUT-261 [M3]
test('Workflow packt L01-Hinweise in Linux-, Windows- und das veröffentlichte Universal-macOS-CLI-Archiv', t => {
  const { directory, version } = fixture(t);
  const notices = 'fixture: vollständige Drittlizenztexte';
  for (const path of ['incoming', 'third-party-notices', 'target/linux-core', 'scripts/release', 'temp']) {
    mkdirSync(join(directory, path), { recursive: true });
  }
  for (const name of ['LICENSE', 'NOTICE']) writeFileSync(join(directory, name), `fixture:${name}`);
  writeFileSync(join(directory, 'third-party-notices/THIRD_PARTY_NOTICES.txt'), notices);
  writeFileSync(join(directory, 'target/linux-core/openspider'), '#!/bin/sh\necho openspider\n', { mode: 0o755 });
  writeFileSync(join(directory, 'incoming/openspider.exe'), 'fixture:Windows CLI');
  copyFileSync(join(root, 'scripts/release/assets.mjs'), join(directory, 'scripts/release/assets.mjs'));
  for (const arch of ['aarch64', 'x86_64', 'universal']) {
    assert.equal(spawnSync('tar', ['-czf', join(directory, `incoming/openspider_${version}_darwin-${arch}.tar.gz`),
      '-C', join(directory, 'target/linux-core'), 'openspider']).status, 0);
  }
  writeFileSync(join(directory, 'incoming/OpenSpider_aarch64.app.tar.gz'), 'signed updater bytes');
  const run = name => spawnSync('bash', ['-c', step(name)], { cwd: directory, encoding: 'utf8',
    env: { ...process.env, VERSION: version, GITHUB_WORKSPACE: directory, RUNNER_TEMP: join(directory, 'temp') } });
  let result = run('CLI-Archiv packen und extrahiert prüfen');
  assert.equal(result.status, 0, result.stderr);
  const linuxArchive = `openspider_${version}_linux-x86_64.tar.gz`;
  copyFileSync(join(directory, 'release-artifacts', linuxArchive), join(directory, 'incoming', linuxArchive));
  result = run('Distributable Assets sammeln und CLI-Archive vervollständigen');
  assert.equal(result.status, 0, result.stderr);
  const out = join(directory, 'release-assets');
  // OUT-261: CLI archives per macOS architecture are built but no longer published.
  for (const platform of ['darwin-aarch64', 'darwin-x86_64']) {
    assert.ok(!readdirSync(out).includes(`openspider_${version}_${platform}.tar.gz`), platform);
  }
  for (const platform of ['linux-x86_64', 'darwin-universal']) {
    const archive = join(out, `openspider_${version}_${platform}.tar.gz`);
    const contents = spawnSync('tar', ['-tzf', archive], { encoding: 'utf8' });
    assert.equal(contents.status, 0, contents.stderr);
    assert.deepEqual(contents.stdout.trim().split('\n').sort(), ['LICENSE', 'NOTICE', 'THIRD_PARTY_NOTICES.txt', 'openspider']);
    assert.equal(spawnSync('tar', ['-xOzf', archive, 'THIRD_PARTY_NOTICES.txt'], { encoding: 'utf8' }).stdout, notices);
  }
  const zip = join(out, `openspider_${version}_windows-x86_64.zip`);
  assert.equal(spawnSync('unzip', ['-p', zip, 'THIRD_PARTY_NOTICES.txt'], { encoding: 'utf8' }).stdout, notices);
  assert.equal(readFileSync(join(out, 'OpenSpider_aarch64.app.tar.gz'), 'utf8'), 'signed updater bytes');
});

test('Orchestrierung übergibt Beta-Kanal an R01/R02 mit einheitlicher Build-Variable', () => {
  const workflow = readFileSync(join(root, '.github/workflows/release.yml'), 'utf8');
  for (const platform of ['macos', 'windows']) {
    const job = workflow.split(`  ${platform}:\n`)[1].split(/\n  [a-z][a-z-]*:/)[0];
    assert.match(job, /channel: \$\{\{ needs.prepare.outputs.channel \}\}/);
    const child = readFileSync(join(root, `.github/workflows/release-${platform}.yml`), 'utf8');
    assert.match(child, /OPENSPIDER_UPDATE_BETA: \$\{\{ (?:inputs.channel|\(inputs.channel \|\| 'beta'\)) == 'beta' && '1' \|\| '0' \}\}/);
  }
});


test('L01-Gate blockiert CLI-Build und Assemble; native Signierung verwendet release', () => {
  const workflow = readFileSync(join(root, '.github/workflows/release.yml'), 'utf8');
  assert.match(workflow, /  licenses:\n    needs: prepare\n    uses: \.\/\.github\/workflows\/licenses.yml/);
  assert.match(workflow, /  linux-core:\n    needs: \[prepare, licenses\]/);
  assert.match(workflow, /  assemble:\n    needs: \[prepare, licenses, macos, windows, linux-core\]/);
  for (const platform of ['macos', 'windows']) {
    const child = readFileSync(join(root, `.github/workflows/release-${platform}.yml`), 'utf8');
    assert.match(child, /    environment: release/);
    if (platform === 'macos') {
      const smoke = child.split('  universal-smoke:\n')[1].split('  artifacts:\n')[0];
      assert.match(smoke, /    environment: release/);
    }
  }
});

test('Alle externen Actions in Release- und Lizenzworkflows sind SHA-gepinnt', () => {
  for (const name of readdirSync(join(root, '.github/workflows')).filter(name => /^release.*\.yml$/.test(name) || name === 'licenses.yml')) {
    const workflow = readFileSync(join(root, '.github/workflows', name), 'utf8');
    for (const line of workflow.split('\n').filter(line => line.includes('uses:') && !line.includes('uses: ./'))) {
      assert.match(line, /@[0-9a-f]{40}(?:\s|$)/, `${name}: ${line}`);
    }
  }
});

// Every distributable file the R01/R02/linux-core artifacts deliver to assemble (Actions artifact layout).
function incomingArtifacts(directory, version) {
  const files = {
    'openspider-macos-aarch64': [`OpenSpider_${version}_aarch64.dmg`, `openspider_${version}_darwin-aarch64.tar.gz`,
      'OpenSpider_aarch64.app.tar.gz', 'OpenSpider_aarch64.app.tar.gz.sig', 'verify-app.log', 'verify-dmg.log'],
    'openspider-macos-x86_64': [`OpenSpider_${version}_x64.dmg`, `openspider_${version}_darwin-x86_64.tar.gz`,
      'OpenSpider_x86_64.app.tar.gz', 'OpenSpider_x86_64.app.tar.gz.sig', 'verify-app.log'],
    'openspider-macos-universal': [`OpenSpider_${version}_universal.dmg`, `openspider_${version}_darwin-universal.tar.gz`,
      'OpenSpider_universal.app.tar.gz', 'OpenSpider_universal.app.tar.gz.sig', 'verify-app.log'],
    'openspider-windows-x86_64': [`OpenSpider_${version}_x64-setup.exe`, `OpenSpider_${version}_x64-setup.exe.sig`,
      `OpenSpider_${version}_x64_en-US.msi`, `OpenSpider_${version}_x64_en-US.msi.sig`, 'openspider.exe'],
    'openspider-linux-x86_64': [`openspider_${version}_linux-x86_64.tar.gz`],
  };
  for (const [artifact, names] of Object.entries(files)) {
    mkdirSync(join(directory, artifact), { recursive: true });
    for (const name of names) writeFileSync(join(directory, artifact, name), name.endsWith('.sig') ? signature : `fixture:${artifact}/${name}`);
  }
}

// spec: OUT-261 [M3]
// spec: OUT-266 [M3]
test('Veröffentlicht nur Universal-DMG und -CLI für macOS, dazu bytegleiche versionslose Kopien in SHA256SUMS', async t => {
  const { directory, version, repository, date } = fixture(t);
  const incoming = join(directory, 'incoming');
  const out = join(directory, 'release-assets');
  incomingArtifacts(incoming, version);
  const result = spawnSync(process.execPath, ['scripts/release/assets.mjs', incoming, out, version], { cwd: root, encoding: 'utf8' });
  assert.equal(result.status, 0, result.stderr);
  assert.deepEqual(readdirSync(out).sort(), [
    'OpenSpider-Windows-Setup.exe',
    'OpenSpider-macOS.dmg',
    'OpenSpider_1.0.0-beta.0_universal.dmg',
    'OpenSpider_1.0.0-beta.0_x64-setup.exe',
    'OpenSpider_1.0.0-beta.0_x64-setup.exe.sig',
    'OpenSpider_1.0.0-beta.0_x64_en-US.msi',
    'OpenSpider_1.0.0-beta.0_x64_en-US.msi.sig',
    'OpenSpider_aarch64.app.tar.gz',
    'OpenSpider_aarch64.app.tar.gz.sig',
    'OpenSpider_x86_64.app.tar.gz',
    'OpenSpider_x86_64.app.tar.gz.sig',
    'openspider.exe',
    'openspider_1.0.0-beta.0_darwin-universal.tar.gz',
    'openspider_1.0.0-beta.0_linux-x86_64.tar.gz',
  ].sort());
  for (const name of [`OpenSpider_${version}_aarch64.dmg`, `OpenSpider_${version}_x64.dmg`, 'OpenSpider_universal.app.tar.gz']) {
    assert.match(result.stdout, new RegExp(`nicht veröffentlicht \\(OUT-261\\): ${name.replaceAll('.', '\\.')}`));
  }
  const { macos, windows } = downloads(version);
  assert.deepEqual(macos, { file: 'OpenSpider_1.0.0-beta.0_universal.dmg', alias: 'OpenSpider-macOS.dmg' });
  assert.deepEqual(windows, { file: 'OpenSpider_1.0.0-beta.0_x64-setup.exe', alias: 'OpenSpider-Windows-Setup.exe' });
  for (const { file, alias } of [macos, windows]) {
    assert.deepEqual(readFileSync(join(out, alias)), readFileSync(join(incoming, file.endsWith('.dmg') ? 'openspider-macos-universal' : 'openspider-windows-x86_64', file)));
  }
  // The copies must never be picked as updater packages; the manifest still has exactly four targets.
  rmSync(join(out, 'openspider.exe'));
  assert.deepEqual(manifests({ directory: out, version, repository, notes: 'Fixture notes', date }).platforms,
    ['darwin-aarch64', 'darwin-x86_64', 'windows-x86_64-nsis', 'windows-x86_64-msi']);
  const sums = Object.fromEntries((await checksums(out)).trim().split('\n').map(line => line.split('  ').reverse()));
  for (const { file, alias } of [macos, windows]) {
    assert.match(sums[alias], /^[0-9a-f]{64}$/);
    assert.equal(sums[alias], sums[file]);
  }
  assert.equal(Object.keys(sums).length, readdirSync(out).length - 1);
  const verification = spawnSync('sha256sum', ['-c', 'SHA256SUMS'], { cwd: out, encoding: 'utf8' });
  assert.equal(verification.status, 0, verification.stderr);

  // Draft guard of the workflow accepts exactly this set ...
  mkdirSync(join(directory, 'release-reports'));
  writeFileSync(join(directory, 'release-reports/release-notes.md'), releaseBody({ version, repository, notes: 'Fixture notes' }));
  const guard = () => spawnSync('bash', ['-c', step('Veröffentlichte Dateien und versionslose Downloads prüfen (OUT-261)')],
    { cwd: directory, encoding: 'utf8', env: { ...process.env, VERSION: version } });
  let check = guard();
  assert.equal(check.status, 0, check.stdout + check.stderr);
  // ... and rejects a differing copy, a copy missing from SHA256SUMS and every unpublished macOS file.
  writeFileSync(join(out, macos.alias), 'tampered');
  assert.match(guard().stdout, /OpenSpider-macOS\.dmg fehlt oder ist nicht bytegleich/);
  copyFileSync(join(out, macos.file), join(out, macos.alias));
  writeFileSync(join(out, 'SHA256SUMS'), readFileSync(join(out, 'SHA256SUMS'), 'utf8').split('\n').filter(l => !l.endsWith(`  ${windows.alias}`)).join('\n'));
  assert.match(guard().stdout, /OpenSpider-Windows-Setup\.exe fehlt in SHA256SUMS/);
  await checksums(out);
  assert.equal(guard().status, 0);
  for (const name of [`OpenSpider_${version}_aarch64.dmg`, `openspider_${version}_darwin-x86_64.tar.gz`, 'OpenSpider_universal.app.tar.gz.sig']) {
    writeFileSync(join(out, name), 'unpublished');
    check = guard();
    assert.notEqual(check.status, 0);
    assert.match(check.stdout, /darf nicht veröffentlicht werden/);
    rmSync(join(out, name));
  }
  // A release text with draft hints never becomes a draft (OUT-261, hints belong in the summary).
  const released = readFileSync(join(directory, 'release-reports/release-notes.md'), 'utf8');
  writeFileSync(join(directory, 'release-reports/release-notes.md'), `${released}\n---\n\n**Entwurfshinweise:**\n\n- TODO: Parität\n`);
  assert.match(guard().stdout, /Release-Text fehlt oder enthält Entwurfshinweise/);
  rmSync(join(directory, 'release-reports/release-notes.md'));
  assert.match(guard().stdout, /Release-Text fehlt/);
  writeFileSync(join(directory, 'release-reports/release-notes.md'), released);
  assert.equal(guard().status, 0);
  rmSync(join(out, windows.alias));
  assert.notEqual(guard().status, 0);
  // The guard runs before the draft is created.
  const workflow = releaseWorkflow();
  assert.ok(workflow.indexOf('versionslose Downloads prüfen (OUT-261)') < workflow.indexOf('gh release create'));
});

// spec: OUT-261 [M3]
test('Unbekannte macOS-Dateien, vorhandene Kopie-Namen und Kopien ohne Quelle werden nicht still veröffentlicht', t => {
  const { directory, version } = fixture(t);
  for (const name of ['OpenSpider_9.9.9_universal.dmg', 'OpenSpider_1.0.0-beta.0_arm64.dmg', 'openspider_1.0.0-beta.0_darwin-arm64.tar.gz', 'OpenSpider_arm64.app.tar.gz']) {
    assert.throws(() => isPublished(name, version), /Unerwartet/, name);
  }
  assert.equal(isPublished('OpenSpider_1.0.0-beta.0_universal.dmg', version), true);
  assert.equal(isPublished('OpenSpider_aarch64.app.tar.gz.sig', version), true);
  assert.equal(isPublished('OpenSpider_universal.app.tar.gz.sig', version), false);
  const incoming = join(directory, 'incoming');
  mkdirSync(incoming);
  writeFileSync(join(incoming, 'OpenSpider-macOS.dmg'), 'foreign bytes');
  let result = spawnSync(process.execPath, ['scripts/release/assets.mjs', incoming, join(directory, 'out-a'), version], { cwd: root, encoding: 'utf8' });
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /Kollision: OpenSpider-macOS\.dmg/);
  rmSync(join(incoming, 'OpenSpider-macOS.dmg'));
  writeFileSync(join(incoming, `OpenSpider_${version}_x64-setup.exe`), 'nsis');
  result = spawnSync(process.execPath, ['scripts/release/assets.mjs', incoming, join(directory, 'out-b'), version], { cwd: root, encoding: 'utf8' });
  assert.equal(result.status, 0, result.stderr);
  // Without an Apple build (no secrets) there is no Universal DMG and therefore no macOS copy.
  assert.deepEqual(readdirSync(join(directory, 'out-b')).sort(), ['OpenSpider-Windows-Setup.exe', `OpenSpider_${version}_x64-setup.exe`]);
});

// spec: OUT-261 [M3]
// spec: OUT-251 [M3]
test('Release-Text beginnt mit dem Download-Kasten; Update-Manifeste enthalten nur die CHANGELOG-Notes', t => {
  const expected = readFileSync(join(root, 'tests/release/fixtures/release-body-1.0.0-beta.2.md'), 'utf8');
  const notes = `${expected.split('## Neu in dieser Version\n\n')[1].trim()}\n`;
  // Fixture: the hand-written download box of the published v1.0.0-beta.2 release (template, user 04.11.2026).
  assert.equal(releaseBody({ version: '1.0.0-beta.2', repository: 'wojcikdominik/openspider', notes }), expected);
  const body = releaseBody({ version: '1.0.0-beta.3', repository: 'example/openspider', notes: 'Neu\n' });
  assert.ok(body.startsWith('## Download\n\n| System | Datei / File |\n'));
  assert.match(body, /\| \*\*macOS 13\+\*\* \(Apple Silicon und Intel\) \| \[OpenSpider_1\.0\.0-beta\.3_universal\.dmg\]\(https:\/\/github\.com\/example\/openspider\/releases\/download\/v1\.0\.0-beta\.3\/OpenSpider_1\.0\.0-beta\.3_universal\.dmg\) \|/);
  assert.match(body, /\| \*\*Windows 10\/11\*\* \(x64, auch Windows on ARM\) \| \[OpenSpider_1\.0\.0-beta\.3_x64-setup\.exe\]\(https:\/\/github\.com\/example\/openspider\/releases\/download\/v1\.0\.0-beta\.3\/OpenSpider_1\.0\.0-beta\.3_x64-setup\.exe\) \|/);
  assert.ok(body.endsWith('\n## Neu in dieser Version\n\nNeu\n'));
  assert.throws(() => releaseBody({ version: '1.0.0-beta.3', repository: 'bad repo', notes: 'x' }), /Repository/);
  assert.throws(() => releaseBody({ version: '1.0.0-beta.3', repository: 'example/openspider', notes: ' \n' }), /fehlen/);

  // Workflow step end to end: separate files for manifest and release body.
  const { directory, version, date } = fixture(t);
  cpSync(join(root, 'scripts/release'), join(directory, 'scripts/release'), { recursive: true });
  cpSync(join(root, 'docs/release/notes'), join(directory, 'docs/release/notes'), { recursive: true });
  copyFileSync(join(root, 'CHANGELOG.md'), join(directory, 'CHANGELOG.md'));
  mkdirSync(join(directory, 'release-assets'));
  for (const name of fixtures) {
    copyFileSync(join(directory, name), join(directory, 'release-assets', name));
    copyFileSync(join(directory, `${name}.sig`), join(directory, 'release-assets', `${name}.sig`));
  }
  const result = spawnSync('bash', ['-c', step('Release-Notes und Update-Manifeste')], { cwd: directory, encoding: 'utf8',
    env: { ...process.env, VERSION: version, RELEASE_DATE: date, GITHUB_REPOSITORY: 'example/openspider', GITHUB_STEP_SUMMARY: join(directory, 'summary') } });
  assert.equal(result.status, 0, result.stderr);
  const changelog = releaseNotes(readFileSync(join(root, 'CHANGELOG.md'), 'utf8'), version);
  assert.equal(readFileSync(join(directory, 'release-reports/update-notes.md'), 'utf8'), changelog);
  for (const file of ['latest.json', 'latest-beta.json']) {
    const manifestNotes = JSON.parse(readFileSync(join(directory, 'release-assets', file))).notes;
    assert.equal(manifestNotes, changelog);
    assert.doesNotMatch(manifestNotes, /## Download|Datei \/ File|Neu in dieser Version|Entwurfshinweise|Installations-Smoke/);
  }
  // OUT-261 / beta3 §5: the release text is the download box plus the notes, nothing else;
  // draft hints only appear in the Actions summary (and the release-reports artifact).
  const released = readFileSync(join(directory, 'release-reports/release-notes.md'), 'utf8');
  assert.equal(released, releaseBody({ version, repository: 'example/openspider', notes: changelog }));
  assert.doesNotMatch(released, /Entwurfshinweis|Installations-Smoke|unsigniert|TODO/);
  const summary = readFileSync(join(directory, 'summary'), 'utf8');
  assert.match(summary, /\*\*Entwurfshinweise \(nicht Teil des Release-Texts\):\*\*/);
  assert.match(summary, /^- Installations-Smoke:/m);
  assert.match(readFileSync(join(directory, 'release-reports/entwurfshinweise.md'), 'utf8'), /^- Installations-Smoke:/m);
  // Without complete signed updater artifacts the "unsigniert" hint also lands in the summary only.
  rmSync(join(directory, 'release-assets', `${fixtures[0]}.sig`));
  rmSync(join(directory, 'release-reports'), { recursive: true });
  const unsigned = spawnSync('bash', ['-c', step('Release-Notes und Update-Manifeste')], { cwd: directory, encoding: 'utf8',
    env: { ...process.env, VERSION: version, RELEASE_DATE: date, GITHUB_REPOSITORY: 'example/openspider', GITHUB_STEP_SUMMARY: join(directory, 'summary-unsigned') } });
  assert.equal(unsigned.status, 0, unsigned.stderr);
  assert.match(readFileSync(join(directory, 'summary-unsigned'), 'utf8'), /^- unsigniert: /m);
  assert.doesNotMatch(readFileSync(join(directory, 'release-reports/release-notes.md'), 'utf8'), /unsigniert/);
  const workflow = releaseWorkflow();
  assert.match(workflow, /manifest\.mjs release-assets "\$VERSION" "\$GITHUB_REPOSITORY" release-reports\/update-notes\.md /);
  assert.match(workflow, /--notes-file release-reports\/release-notes\.md/);
});

// spec: OUT-251 [M3]
test('Notes-Dokument je Version ist einzige Quelle; eingecheckte Notes sind gültig und gleich dem CHANGELOG', () => {
  const changelog = readFileSync(join(root, 'CHANGELOG.md'), 'utf8');
  // Only version-named documents; a README or similar in the folder is not a release.
  const documents = readdirSync(join(root, 'docs/release/notes')).filter(name => /^\d+\.\d+\.\d+(-[0-9A-Za-z.]+)?\.md$/.test(name));
  assert.ok(documents.length >= 3, documents.join());
  for (const name of documents) {
    const version = name.slice(0, -3);
    const notes = versionNotes(readFileSync(join(root, 'docs/release/notes', name), 'utf8'), version);
    // On main a notes document may precede its CHANGELOG section (Wednesday); if both exist they are equal.
    const section = changelogSection(changelog, version);
    if (section !== null) assert.equal(`${section}\n`, notes, name);
    assert.equal(checkNotes({ root, version, requireChangelog: false }), notes);
  }
  // Every published version (CHANGELOG sections from rel/beta1, rel/beta2) has its notes document.
  for (const version of ['1.0.0-beta.0', '1.0.0-beta.1', '1.0.0-beta.2']) {
    assert.equal(checkNotes({ root, version }), releaseNotes(changelog, version));
  }
  assert.equal(notesPath('1.0.0-beta.3'), join('docs', 'release', 'notes', '1.0.0-beta.3.md'));
  for (const text of ['', ' \n', '## Neu\n- x', '# Titel\n- x', '- TODO: später', '- STUB(R03)', 'Entwurfshinweise: unsigniert']) {
    assert.throws(() => versionNotes(text, '1.0.0-beta.3'), Error, JSON.stringify(text));
  }
  assert.equal(versionNotes('### Neu\r\n\r\n- Punkt\r\n', '1.0.0-beta.3'), '### Neu\n\n- Punkt\n');
  assert.throws(() => notesPath('../1.0.0'), /Ungültige/);
});

// spec: OUT-251 [M3]
test('prepare bricht vor dem Bauen ab, wenn die Notes fehlen, leer sind oder vom CHANGELOG abweichen', t => {
  const { directory } = fixture(t);
  cpSync(join(root, 'scripts/release'), join(directory, 'scripts/release'), { recursive: true });
  const version = '1.0.0-beta.3';
  const run = env => spawnSync('bash', ['-c', step('Release-Notes vor dem Bauen prüfen (OUT-251)')], { cwd: directory, encoding: 'utf8',
    env: { ...process.env, VERSION: version, GITHUB_STEP_SUMMARY: join(directory, 'summary'), ...env } });
  const notes = join(directory, 'docs/release/notes', `${version}.md`);
  writeFileSync(join(directory, 'CHANGELOG.md'), '# Changelog\n\n## [Unreleased]\n\n## [1.0.0-beta.2] - 2026-11-03\n\n- alt\n');
  let result = run({ GITHUB_ACTIONS: 'true' });
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /Release-Notes für 1\.0\.0-beta\.3 fehlen: docs[/\\]release[/\\]notes[/\\]1\.0\.0-beta\.3\.md anlegen/);
  assert.doesNotMatch(result.stderr, /    at /, 'no stack trace');
  assert.match(result.stdout, /^::error title=Release-Notes::Release-Notes für 1\.0\.0-beta\.3 fehlen/m);
  assert.match(readFileSync(join(directory, 'summary'), 'utf8'), /\*\*Release-Notes:\*\* Release-Notes für 1\.0\.0-beta\.3 fehlen/);
  mkdirSync(dirname(notes), { recursive: true });
  writeFileSync(notes, '\n  \n');
  assert.match(run().stderr, /Release-Notes 1\.0\.0-beta\.3 sind leer/);
  writeFileSync(notes, '- Chrome-Download auf Klick\n');
  assert.match(run().stderr, /CHANGELOG-Eintrag 1\.0\.0-beta\.3 fehlt: node scripts\/release\/notes\.mjs --changelog 1\.0\.0-beta\.3/);
  // The documented one-liner writes the CHANGELOG section from the notes document; the gate then passes.
  const changelog = spawnSync(process.execPath, ['scripts/release/notes.mjs', '--changelog', version, '--date', '2026-11-12'], { cwd: directory, encoding: 'utf8' });
  assert.equal(changelog.status, 0, changelog.stderr);
  result = run();
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /vorhanden und gleich dem CHANGELOG-Eintrag/);
  // A later edit of only one of both is caught before building.
  writeFileSync(notes, '- Chrome-Download auf Klick (geändert)\n');
  assert.match(run().stderr, /weicht von docs[/\\]release[/\\]notes[/\\]1\.0\.0-beta\.3\.md ab/);
  // The gate is part of prepare, which every build job needs.
  const workflow = releaseWorkflow();
  const prepare = workflow.split('\n  prepare:\n')[1].split('\n  macos:\n')[0];
  assert.ok(prepare.indexOf('Release-Notes vor dem Bauen prüfen (OUT-251)') > prepare.indexOf('Versionsgleichstand und Tag prüfen'));
  assert.match(prepare, /VERSION: \$\{\{ steps\.version\.outputs\.version \}\}\n        run: \|\n          set -euo pipefail\n          node scripts\/release\/notes\.mjs --check "\$VERSION"\n/);
  for (const job of ['licenses', 'macos', 'windows']) {
    assert.match(workflow, new RegExp(`\\n  ${job}:\\n(?:    [^\\n]*\\n)*?    needs: prepare\\n`), job);
  }
});

// spec: OUT-251 [M3]
test('CHANGELOG-Eintrag entsteht aus dem Notes-Dokument unter [Unreleased] und wird nie still überschrieben', () => {
  const changelog = '# Changelog\n\n## [Unreleased]\n\n- künftig\n\n## [1.0.0-beta.2] - 2026-11-03\n\n- alt\n';
  const updated = withChangelogSection(changelog, '1.0.0-beta.3', '- neu\n', '2026-11-12');
  assert.equal(updated, '# Changelog\n\n## [Unreleased]\n\n- künftig\n\n## [1.0.0-beta.3] - 2026-11-12\n\n- neu\n\n## [1.0.0-beta.2] - 2026-11-03\n\n- alt\n');
  assert.equal(withChangelogSection(updated, '1.0.0-beta.3', '- neu', '2026-11-13'), updated);
  assert.throws(() => withChangelogSection(updated, '1.0.0-beta.3', '- anders', '2026-11-12'), /abweichenden Eintrag/);
  assert.throws(() => withChangelogSection(changelog, '1.0.0-beta.3', '- neu', '12.11.2026'), /Datum/);
  assert.equal(withChangelogSection('# Changelog\n\n## [1.0.0] - x\n\n- a\n', '1.0.1', '- b', '2026-12-01'),
    '# Changelog\n\n## [1.0.1] - 2026-12-01\n\n- b\n\n## [1.0.0] - x\n\n- a\n');
  assert.equal(releaseNotes(updated, '1.0.0-beta.3'), '- neu\n');
});

// spec: OUT-261 [M3]
test('Feedback-Link im Download-Kasten: Vorabversion zur Beta-Seite, stabile Version zu GitHub Discussions', () => {
  for (const version of ['1.0.0-beta.3', '1.0.0-rc.1']) {
    assert.equal(feedbackLink(version, 'wojcikdominik/openspider'), '[openspider.de/beta](https://openspider.de/beta/)');
    assert.match(releaseBody({ version, repository: 'wojcikdominik/openspider', notes: 'x' }), /· Feedback: \[openspider\.de\/beta\]\(https:\/\/openspider\.de\/beta\/\)\n/);
  }
  const stable = releaseBody({ version: '1.0.0', repository: 'wojcikdominik/openspider', notes: 'x' });
  assert.match(stable, /· Feedback: \[GitHub Discussions\]\(https:\/\/github\.com\/wojcikdominik\/openspider\/discussions\)\n/);
  assert.doesNotMatch(stable, /openspider\.de\/beta/);
});

// GitHub Actions path filters: patterns in order, the last matching one wins ("!" excludes); `*` stays within one
// path segment, `**` crosses segments, `**/` may also match nothing (root files).
function pathFilter(workflow, event = 'push') {
  const text = readFileSync(join(root, '.github/workflows', workflow), 'utf8');
  const block = text.split(`\n  ${event}:\n`)[1].split(/\n  [a-z_]+:/)[0];
  const alias = /\n    paths: \*([\w-]+)/.exec(`\n${block}`)?.[1];
  const source = alias ? text.split(`paths: &${alias}\n`)[1] : block.split('    paths:')[1].replace(/^[^\n]*\n/, '');
  const patterns = source.split('\n').filter(line => /^      - /.test(line)).map(line => line.slice(8).replace(/^["']|["']$/g, ''));
  const tail = source.split('\n').findIndex(line => !/^      - /.test(line));
  const list = patterns.slice(0, tail < 0 ? undefined : tail);
  const regex = glob => new RegExp(`^${glob.replace(/[.+^${}()|[\]\\]/g, '\\$&').replace(/\*\*\/|\*\*|\*|\?/g,
    token => ({ '**/': '(?:.*/)?', '**': '.*', '*': '[^/]*', '?': '[^/]' })[token])}$`);
  const rules = list.map(pattern => ({ exclude: pattern.startsWith('!'), regex: regex(pattern.replace(/^!/, '')) }));
  // A push triggers the workflow if at least one changed file is included after all rules.
  return files => files.some(file => rules.reduce((included, rule) => (rule.regex.test(file) ? !rule.exclude : included), false));
}

// B3-REL (K2-67, docs/plan/beta3.md §6): docs/website/learnings-only commits on main start no heavy CI and thus never
// queue behind or replace a running main run; files that tests or jobs read keep triggering.
test('Pfadfilter: reine Doku-, Website- und Learnings-Commits starten keine schwere CI, gelesene Dateien schon', () => {
  const quiet = ['README.md', 'CHANGELOG.md', 'website/site/beta/index.html', 'website/site/assets/site.css', 'website/check-site.mjs',
    'website/api/feedback.php', 'docs/plan/M3-reports/b3-rel.md', 'docs/release/notes/1.0.0-beta.3.md', 'docs/release/release-rhythmus.md',
    'docs/plan/M1-acceptance/loadtest/probe-p33/s0-baseline/summary.json', 'docs/plan/M3-reports/v02/polish/a.png', 'docs/handbook/book.toml',
    'learnings/learnings.md', 'learnings/raw/notes.txt', 'assets/logo.png'];
  const read = ['docs/spec/06-output-ops.md', 'docs/spec/schemas/osconfig.schema.json', 'docs/issues/README.md', 'docs/plan/M2-packages.json',
    'docs/plan/M1-status.json', 'docs/plan/M1-filters.toml', 'tests/fixtures/migrations/releases/1.0.0-beta.1/README.md',
    'crates/os-app/src/lib.rs', 'Cargo.lock', '.github/workflows/ci.yml'];
  for (const workflow of ['ci.yml', 'linux-core.yml', 'parity.yml']) {
    for (const event of ['push', 'pull_request']) {
      const runs = pathFilter(workflow, event);
      for (const file of quiet) assert.equal(runs([file]), false, `${workflow} ${event}: ${file}`);
      for (const file of [...read, 'docs/plan/M2.md']) assert.equal(runs([file]), true, `${workflow} ${event}: ${file}`);
      assert.equal(runs(['website/site/index.html', 'crates/os-cli/src/main.rs']), true, `${workflow}: mixed push`);
    }
  }
  const licenses = pathFilter('licenses.yml');
  for (const file of quiet) assert.equal(licenses([file]), false, `licenses.yml: ${file}`);
  for (const file of ['Cargo.lock', 'package-lock.json', 'deny.toml', 'NOTICE', 'scripts/release/licenses/npm-sources.json']) assert.equal(licenses([file]), true, file);
  // Spec coverage keeps docs/spec and never runs for website or notes.
  const coverage = pathFilter('spec-coverage-m2.yml');
  assert.equal(coverage(['docs/spec/06-output-ops.md']), true);
  for (const file of ['website/site/beta/index.html', 'docs/release/notes/1.0.0-beta.3.md', 'README.md']) assert.equal(coverage([file]), false, file);
  assert.equal(pathFilter('ui-e2e.yml')(['website/site/beta/index.html']), false);
  // Light replacements: website check for website/**, release contract tests for notes and CHANGELOG.
  const site = pathFilter('website.yml');
  assert.equal(site(['website/site/beta/index.html']), true);
  assert.equal(site(['website/api/tests/crash_test.php']), true);
  assert.equal(site(['crates/os-app/src/lib.rs']), false);
  const scripts = pathFilter('release-scripts.yml');
  for (const file of ['docs/release/notes/1.0.0-beta.3.md', 'CHANGELOG.md', 'scripts/release/notes.mjs', 'tests/release/orchestration.test.mjs', '.github/workflows/release.yml']) {
    assert.equal(scripts([file]), true, file);
  }
  assert.equal(scripts(['website/site/index.html']), false);
  // Tag pushes ignore path filters on GitHub; the release workflow itself has none.
  assert.doesNotMatch(releaseWorkflow(), /paths/);
});

test('Leichte Ersatzläufe sind billig, SHA-gepinnt und rufen die vorhandenen Prüfungen auf', () => {
  const site = readFileSync(join(root, '.github/workflows/website.yml'), 'utf8');
  assert.match(site, /runs-on: ubuntu-latest\n    timeout-minutes: 10/);
  assert.match(site, /run: node website\/check-site\.mjs/);
  const scripts = readFileSync(join(root, '.github/workflows/release-scripts.yml'), 'utf8');
  assert.match(scripts, /node --test scripts\/release\/\*\.test\.mjs scripts\/release\/licenses\/generate\.test\.mjs tests\/release\/orchestration\.test\.mjs/);
  for (const workflow of [site, scripts]) {
    assert.match(workflow, /permissions:\n  contents: read/);
    for (const line of workflow.split('\n').filter(line => line.includes('uses:'))) assert.match(line, /@[0-9a-f]{40} # v\d/);
  }
  // The license text cache is wired wherever the generator runs.
  for (const workflow of ['licenses.yml', 'release-macos.yml', 'release-windows.yml']) {
    const text = readFileSync(join(root, '.github/workflows', workflow), 'utf8');
    const generate = text.split(/      - name: (?:Generate complete release notices|Vollständige Drittlizenzen erzeugen)[^\n]*\n/)[1].split('      - ')[0];
    assert.match(generate, /OPENSPIDER_LICENSE_CACHE: \$\{\{ runner\.temp \}\}\/license-texts/, workflow);
    assert.match(text, /path: \$\{\{ runner\.temp \}\}\/license-texts\n          key: license-texts-v1-\$\{\{ hashFiles\('scripts\/release\/licenses\/\*-sources\.json'\) \}\}/, workflow);
  }
});

// spec: OUT-251 [M3]
test('Kurze Update-Notes: nur der Abschnitt <!-- update --> geht ins Manifest, Release-Text und CHANGELOG bleiben ausführlich', t => {
  const short = '- Short one\n- Short **two**\n- Short [three](https://openspider.de/)\n\n### Deutsch\n\n- Kurz eins\n- Kurz **zwei**\n- Kurz [drei](https://openspider.de/)';
  const detailed = 'Long summary.\n\n- Long point with details\n\n### Deutsch\n\nLange Zusammenfassung.\n\n- Langer Punkt mit Details';
  const document = `<!-- update -->\n${short}\n<!-- /update -->\n\n${detailed}\n`;
  const version = '1.0.0-beta.4';
  assert.equal(FIRST_SHORT_NOTES, version);
  assert.deepEqual(splitNotesDocument(document.replaceAll('\n', '\r\n')).update, short);
  assert.equal(updateNotes(document, version), `${short}\n`);
  assert.equal(versionNotes(document, version), `${detailed}\n`);
  // Published versions up to beta.3 have no short section: their detailed notes stay the update notes.
  const beta3 = readFileSync(join(root, 'docs/release/notes/1.0.0-beta.3.md'), 'utf8');
  assert.equal(updateNotes(beta3, '1.0.0-beta.3'), versionNotes(beta3, '1.0.0-beta.3'));
  assert.equal(compareVersions('1.0.0-beta.3', '1.0.0-beta.4'), -1);
  assert.equal(compareVersions('1.0.0-beta.10', '1.0.0-beta.4'), 1);
  assert.equal(compareVersions('1.0.0-rc.1', '1.0.0-beta.9'), 1);
  assert.equal(compareVersions('1.0.0', '1.0.0-rc.1'), 1);
  assert.equal(compareVersions('1.0.1', '1.0.1'), 0);
  // From beta.4 on the short section is required and checked.
  assert.throws(() => updateNotes(`${detailed}\n`, version), /Kurze Update-Notes 1\.0\.0-beta\.4 fehlen: .*<!-- update -->/);
  assert.throws(() => updateNotes(`${detailed}\n`, '1.0.0'), /fehlen/);
  const broken = [
    [short.replace('- Short one\n', ''), /Englisch: 2 Punkte/],
    [short.replace('- Kurz eins', '- Kurz eins\n- 4\n- 5\n- 6\n- 7'), /Deutsch: 7 Punkte/],
    [short.replace('\n### Deutsch\n', '\n'), /genau eine Zeile ### Deutsch/],
    [short.replace('- Short one', 'Short one'), /nur einzeilige Listenpunkte/],
    [short.replace('- Short one', `- ${'x'.repeat(170)}`), /länger als 160 Zeichen/],
    [short.replace('- Short one', '- TODO'), /Entwurfsmarkierungen/],
  ];
  for (const [section, pattern] of broken) {
    assert.throws(() => updateNotes(`<!-- update -->\n${section}\n<!-- /update -->\n\n${detailed}\n`, version), pattern, String(pattern));
  }
  assert.throws(() => updateNotes(`<!-- update -->\n${short}\n\n${detailed}\n`, version), /genau ein Abschnitt/);
  // CLI end to end: --changelog writes the detailed notes, --check passes, the manifest file gets the short notes,
  // the release body the detailed ones.
  const directory = mkdtempSync(join(tmpdir(), 'openspider-short-notes-'));
  t.after(() => rmSync(directory, { recursive: true, force: true }));
  cpSync(join(root, 'scripts/release'), join(directory, 'scripts/release'), { recursive: true });
  mkdirSync(join(directory, 'docs/release/notes'), { recursive: true });
  writeFileSync(join(directory, 'docs/release/notes', `${version}.md`), document);
  writeFileSync(join(directory, 'CHANGELOG.md'), '# Changelog\n\n## [Unreleased]\n\n## [1.0.0-beta.3] - 2026-11-09\n\n- alt\n');
  const cli = (...args) => spawnSync(process.execPath, ['scripts/release/notes.mjs', ...args], { cwd: directory, encoding: 'utf8' });
  assert.equal(cli('--changelog', version, '--date', '2026-11-12').status, 0);
  assert.equal(releaseNotes(readFileSync(join(directory, 'CHANGELOG.md'), 'utf8'), version), `${detailed}\n`);
  const check = cli('--check', version);
  assert.equal(check.status, 0, check.stderr);
  const built = cli(version, 'update-notes.md', '--body', 'release-notes.md', '--repository', 'example/openspider');
  assert.equal(built.status, 0, built.stderr);
  assert.equal(readFileSync(join(directory, 'update-notes.md'), 'utf8'), `${short}\n`);
  const body = readFileSync(join(directory, 'release-notes.md'), 'utf8');
  assert.equal(body, releaseBody({ version, repository: 'example/openspider', notes: `${detailed}\n` }));
  assert.doesNotMatch(body, /<!-- update -->|Short one/);
  // A document without the short section fails the gate before the build.
  writeFileSync(join(directory, 'docs/release/notes', `${version}.md`), `${detailed}\n`);
  const missing = cli('--check', version);
  assert.notEqual(missing.status, 0);
  assert.match(missing.stderr, /Kurze Update-Notes 1\.0\.0-beta\.4 fehlen/);
});

// spec: OUT-251 [M3]
test('Vorlage für die nächste Version enthält den kurzen Abschnitt im erwarteten Format', () => {
  const template = readFileSync(join(root, 'docs/release/notes-template.md'), 'utf8');
  const { update, notes } = splitNotesDocument(template);
  assert.match(update, /^- .*\n[\s\S]*\n### Deutsch\n\n- /);
  assert.match(notes, /### Deutsch/);
  // Filled in (placeholders replaced by text), the template passes the checks.
  const filled = template.replace(/<(?!!)[^>\n]*>/g, 'Text');
  assert.ok(updateNotes(filled, '1.0.0-beta.4').startsWith('- Text'));
  assert.ok(versionNotes(filled, '1.0.0-beta.4').startsWith('Text'));
});
