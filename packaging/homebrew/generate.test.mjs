import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, writeFileSync, rmSync, mkdirSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createHash } from 'node:crypto';
import { execFileSync, spawnSync } from 'node:child_process';
import { generate } from './generate.mjs';

const fixture = JSON.parse(readFileSync(new URL('./fixtures.json', import.meta.url)));
function setup(t) {
  const root = mkdtempSync(join(tmpdir(), 'p01-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const directory = join(root, 'assets');
  mkdirSync(directory);
  for (const [name, bytes] of Object.entries(fixture)) writeFileSync(join(directory, name), bytes);
  return { directory, output: join(root, 'out'), version: '1.0.0', repository: 'example/openspider', packageId: 'OpenSpider.CITest' };
}
const parse = (text, loader = 'SafeLoader') => JSON.parse(execFileSync('python3', ['-c',
  'import json,sys,yaml;print(json.dumps(yaml.load(sys.stdin, Loader=getattr(yaml, sys.argv[1]))))', loader], { input: text }));

// spec: OUT-166 [M3]
test('Cask links the CLI at the stable app path and removes registrations without implicit zap', t => {
  const result = generate(setup(t));
  const cask = result['openspider.rb'];
  assert.match(cask, /app "OpenSpider.app"/);
  assert.match(cask, /binary "#\{appdir\}\/OpenSpider.app\/Contents\/MacOS\/openspider"/);
  assert.match(cask, /auto_updates true/);
  assert.match(cask, /uninstall launchctl: "de.openspider.task.\*"/);
  assert.match(cask, /delete:    "~\/Library\/LaunchAgents\/de.openspider.task.\*.plist"/);
  assert.doesNotMatch(cask.split('uninstall launchctl:')[1].split('zap trash:')[0], /Application Support|Preferences|Caches/);
  assert.match(cask, /uninstall_preflight_steps do/);
  assert.match(cask, /postflight_steps do/);
  assert.match(cask, /generated_script "openspider-launchagents.rb"/);
  assert.match(cask, /\{\{temp\}\}\/launchagents.json/);
  assert.doesNotMatch(cask, /\b(?:uninstall_preflight|postflight) do/);
  assert.match(cask, /zap trash:[\s\S]*org.openspider.desktop/);
  assert.match(cask, new RegExp(createHash('sha256').update(fixture['OpenSpider_1.0.0_universal.dmg']).digest('hex')));
  assert.match(cask, /\/v1.0.0\/OpenSpider_1.0.0_universal.dmg/);
  const ruby = spawnSync('ruby', ['-c'], { input: cask, encoding: 'utf8' });
  if (ruby.error?.code === 'ENOENT') t.diagnostic('ruby unavailable; syntax check remains mandatory in macOS CI');
  else assert.equal(ruby.status, 0, ruby.stderr);
});

// spec: OUT-261 [M3]
test('Cask uses exactly the published Universal DMG for Apple Silicon and Intel', t => {
  const opts = setup(t);
  const cask = generate({ ...opts, mode: 'homebrew' })['openspider.rb'];
  const universal = createHash('sha256').update(fixture['OpenSpider_1.0.0_universal.dmg']).digest('hex');
  assert.equal(cask.match(/^  sha256 "([0-9a-f]{64})"$/gm)?.length, 1);
  assert.match(cask, new RegExp(`^  sha256 "${universal}"$`, 'm'));
  assert.equal(cask.match(/^  url "[^"]+"$/gm)?.join('\n'), '  url "https://github.com/example/openspider/releases/download/v1.0.0/OpenSpider_1.0.0_universal.dmg"');
  assert.doesNotMatch(cask, /on_arm|on_intel|_aarch64\.dmg|_x64\.dmg|OpenSpider-macOS\.dmg/);
  for (const name of ['OpenSpider_1.0.0_aarch64.dmg', 'OpenSpider_1.0.0_x64.dmg', 'OpenSpider-macOS.dmg']) {
    assert.doesNotMatch(cask, new RegExp(createHash('sha256').update(fixture[name]).digest('hex')));
  }
  // Release assets without per-architecture DMGs (as published from beta.3 on) still generate the cask.
  for (const name of ['OpenSpider_1.0.0_aarch64.dmg', 'OpenSpider_1.0.0_x64.dmg']) rmSync(join(opts.directory, name));
  rmSync(opts.output, { recursive: true });
  assert.equal(generate({ ...opts, mode: 'homebrew' })['openspider.rb'], cask);
  rmSync(opts.output, { recursive: true });
  writeFileSync(join(opts.directory, 'OpenSpider_0.9.0_universal.dmg'), 'other release');
  assert.throws(() => generate({ ...opts, mode: 'homebrew' }), /genau ein Asset/);
  rmSync(join(opts.directory, 'OpenSpider_0.9.0_universal.dmg'));
  rmSync(join(opts.directory, 'OpenSpider_1.0.0_universal.dmg'));
  assert.throws(() => generate({ ...opts, mode: 'homebrew' }), /genau ein Asset/);
});

// spec: OUT-266 [M3]
test('fixture generates MSI machine and NSIS user with quiet switches and exact SHA256', t => {
  const opts = setup(t);
  const result = generate(opts);
  const installer = parse(result['OpenSpider.CITest.installer.yaml']);
  assert.deepEqual(installer.Installers.map(i => [i.InstallerType, i.Scope]), [['msi', 'machine'], ['nullsoft', 'user']]);
  assert.equal(installer.Installers[0].InstallerSwitches.Silent, '/qn /norestart');
  assert.equal(installer.Installers[1].InstallerSwitches.Silent, '/S');
  assert.equal(installer.Installers[0].InstallerSwitches.Custom, 'ALLUSERS=1 ADDTOPATH=1');
  for (const item of installer.Installers) {
    const name = decodeURIComponent(new URL(item.InstallerUrl).pathname.split('/').at(-1));
    // The versionless NSIS copy only serves latest/download links; winget pins the versioned file.
    assert.notEqual(name, 'OpenSpider-Windows-Setup.exe');
    assert.equal(item.InstallerSha256, createHash('sha256').update(fixture[name]).digest('hex').toUpperCase());
    assert.doesNotMatch(JSON.stringify(item), /REMOVEUSERDATA/);
  }
  rmSync(join(opts.output, 'openspider.rb'));
  assert.match(execFileSync('python3', ['packaging/winget/validate.py', opts.output], { encoding: 'utf8' }), /ok: winget schema/);
});

test('K3-08 has no default ID; missing/ambiguous assets fail closed before writes', t => {
  const opts = setup(t);
  assert.throws(() => generate({ ...opts, packageId: undefined }), /K3-08/);
  assert.throws(() => generate({ ...opts, version: 'bad"\n' }), /Version/);
  assert.throws(() => generate({ ...opts, baseUrl: 'https://token@example.com' }), /Zugangsdaten/);
  writeFileSync(join(opts.directory, 'OpenSpider_2.0.0_x64-setup.exe'), 'other release');
  assert.throws(() => generate(opts), /genau ein Asset/);
  rmSync(join(opts.directory, 'OpenSpider_2.0.0_x64-setup.exe'));
  rmSync(join(opts.directory, 'OpenSpider_1.0.0_x64_en-US.msi'));
  assert.throws(() => generate(opts), /genau ein Asset/);
});

test('stale output rejected; changed bytes change checksum; local drafts allowed only on loopback', t => {
  const opts = setup(t);
  const before = generate(opts);
  assert.throws(() => generate(opts), /leer/);
  rmSync(opts.output, { recursive: true });
  writeFileSync(join(opts.directory, 'OpenSpider_1.0.0_x64-setup.exe'), 'changed');
  const after = generate({ ...opts, baseUrl: 'http://127.0.0.1:12345' });
  assert.notEqual(before['OpenSpider.CITest.installer.yaml'], after['OpenSpider.CITest.installer.yaml']);
  assert.throws(() => generate({ ...opts, baseUrl: 'http://example.com' }), /Loopback/);
});

test('publication has only release published trigger and defaults to no external writes', () => {
  const workflow = readFileSync('.github/workflows/publish-packages.yml', 'utf8');
  assert.match(workflow, /types: \[published\]/);
  assert.doesNotMatch(workflow, /^  (push|workflow_dispatch|workflow_call|pull_request):/m);
  assert.match(workflow, /RELEASE_PACKAGES_ENABLED/);
  assert.match(workflow, /if \[\[ "\$ENABLED" != true/);
  assert.match(workflow, /exit 0/);
  for (const line of workflow.split('\n').filter(l => l.includes('uses: actions/'))) {
    assert.match(line, /@[a-f0-9]{40} /);
  }
  const publish = spawnSync('bash', ['packaging/winget/publish.sh'], { env: { PATH: process.env.PATH }, encoding: 'utf8' });
  assert.notEqual(publish.status, 0);
  assert.match(publish.stderr, /RELEASE_TAG/);
  // BaseLoader preserves GitHub Actions' YAML 1.2 key `on` (SafeLoader uses YAML 1.1).
  const parsed = parse(workflow, 'BaseLoader');
  assert.deepEqual(Object.keys(parsed.on), ['release']);
  assert.deepEqual(parsed.on.release.types, ['published']);
  assert.equal(parsed.jobs.gate.environment, undefined);
  assert.doesNotMatch(JSON.stringify(parsed.jobs.gate), /secrets\.|TAP_READY|WINGET_READY/);
  assert.equal(parsed.jobs.publish.needs, 'gate');
  assert.equal(parsed.jobs.publish.environment, 'release');
  for (const step of parsed.jobs.publish.steps.slice(1)) {
    assert.equal(step.if, "steps.credentials.outputs.ready == 'true'");
  }
  const gate = parsed.jobs.gate.steps[0].run;
  const root = mkdtempSync(join(tmpdir(), 'p01-gate-'));
  try {
    const summary = join(root, 'summary');
    const output = join(root, 'output');
    const env = {
      PATH: process.env.PATH, ENABLED: 'true', DRAFT: 'false', PRERELEASE: 'false', RELEASE_TAG: 'v1.0.0',
      PACKAGE_ID: 'OpenSpider.CITest', TAP_REPOSITORY: 'example/homebrew-openspider',
      WINGET_FORK: 'example/winget-pkgs', GITHUB_OUTPUT: output, GITHUB_STEP_SUMMARY: summary,
    };
    for (const overrides of [
      { ENABLED: 'false' }, { PACKAGE_ID: '' }, { TAP_REPOSITORY: '' }, { WINGET_FORK: '' },
      { DRAFT: 'true' }, { PRERELEASE: 'true' },
      { RELEASE_TAG: 'v1.0.0-beta.1' }, { RELEASE_TAG: 'v1.0.0-rc.1' },
      { RELEASE_TAG: 'v1.0.0+build.1' }, { RELEASE_TAG: '1.0.0' }, { RELEASE_TAG: 'v1.0' },
    ]) {
      const skipped = spawnSync('bash', ['-c', gate], { encoding: 'utf8', env: { ...env, ...overrides } });
      assert.equal(skipped.status, 0, skipped.stderr);
      assert.match(skipped.stdout, /::notice::/);
      assert.equal(existsSync(output), false);
    }
    assert.match(readFileSync(summary, 'utf8'), /übersprungen/);
    const enabled = spawnSync('bash', ['-c', gate], { encoding: 'utf8', env });
    assert.equal(enabled.status, 0, enabled.stderr);
    assert.equal(readFileSync(output, 'utf8'), 'enabled=true\n');
    rmSync(output);
    const credentials = parsed.jobs.publish.steps[0];
    assert.equal(credentials.id, 'credentials');
    for (const [tap, winget] of [['false', 'false'], ['true', 'false'], ['false', 'true']]) {
      const skipped = spawnSync('bash', ['-c', credentials.run], { encoding: 'utf8', env: {
        ...env, TAP_READY: tap, WINGET_READY: winget,
      } });
      assert.equal(skipped.status, 0, skipped.stderr);
      assert.match(skipped.stdout, /::notice::/);
      assert.equal(existsSync(output), false);
    }
    const ready = spawnSync('bash', ['-c', credentials.run], { encoding: 'utf8', env: {
      ...env, TAP_READY: 'true', WINGET_READY: 'true',
    } });
    assert.equal(ready.status, 0, ready.stderr);
    assert.equal(readFileSync(output, 'utf8'), 'ready=true\n');
    // Reach both token guards before any gh/git command, using no actual credentials.
    for (const overrides of [{}, { HOMEBREW_TAP_TOKEN: 'fixture-unused' }]) {
      const blocked = spawnSync('bash', ['packaging/winget/publish.sh'], { encoding: 'utf8', env: {
        ...env, OPENSPIDER_PACKAGE_ID: env.PACKAGE_ID, HOMEBREW_TAP_REPOSITORY: env.TAP_REPOSITORY,
        WINGET_FORK_REPOSITORY: env.WINGET_FORK, ...overrides,
      } });
      assert.notEqual(blocked.status, 0);
      assert.match(blocked.stderr, overrides.HOMEBREW_TAP_TOKEN ? /WINGET_PR_TOKEN/ : /HOMEBREW_TAP_TOKEN/);
      assert.doesNotMatch(blocked.stderr, /fixture-unused/);
    }
  } finally { rmSync(root, { recursive: true, force: true }); }
});
