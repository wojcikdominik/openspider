// B3-BREW: Casks für den eigenen Homebrew-Tap. Homebrew/winget haben noch keine eigene Spec-ID (K3-08,
// Vorschlag OUT-256 im Bericht docs/plan/M3-reports/b3-brew.md); die Tests verweisen auf die berührten Einträge.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, readFileSync, writeFileSync, readdirSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import { casks, parseSums, compareVersions, isStable, updateTap, writeCasks, tapName } from './homebrew.mjs';
import { downloads } from './assets.mjs';
import { generate, caskChannels, zapPaths } from '../../packaging/homebrew/generate.mjs';

const root = resolve(import.meta.dirname, '../..');
// Unveränderte SHA256SUMS des veröffentlichten Releases v1.0.0-beta.2 (gh release download --pattern SHA256SUMS).
const beta2Sums = readFileSync(join(root, 'tests/release/fixtures/SHA256SUMS-1.0.0-beta.2'), 'utf8');
const beta2Universal = '6d0206033cbf3525a50f6a9f60e44ea36f9c25f9046532e9a8466c17d216b0f1';
const repository = 'wojcikdominik/openspider';
const script = join(root, 'scripts/release/homebrew.mjs');

function scratch(t) {
  const directory = mkdtempSync(join(tmpdir(), 'b3-brew-'));
  t.after(() => rmSync(directory, { recursive: true, force: true }));
  return directory;
}
// Ruby-Regex-Literal /…/i aus dem Cask als JavaScript-RegExp (gleiche Syntax für die verwendeten Konstrukte).
function livecheckRegex(cask) {
  const match = /^ {4}regex\(\/(.+)\/i\)$/m.exec(cask);
  assert.ok(match, 'livecheck regex fehlt');
  return new RegExp(match[1], 'i');
}
function rubySyntax(t, cask) {
  const ruby = spawnSync('ruby', ['-c'], { input: cask, encoding: 'utf8' });
  if (ruby.error?.code === 'ENOENT') t.diagnostic('ruby fehlt; brew style/audit laufen im Workflow homebrew.yml (macOS)');
  else assert.equal(ruby.status, 0, ruby.stderr);
}
// Fixture für einen stabilen Release mit denselben Dateinamen-Regeln wie assets.mjs.
function stableSums(version, universalBytes = 'fixture: universal dmg') {
  const hash = text => createHash('sha256').update(text).digest('hex');
  const { macos, windows } = downloads(version);
  return [
    [hash(universalBytes), macos.alias], [hash(universalBytes), macos.file],
    [hash('fixture: nsis'), windows.alias], [hash('fixture: nsis'), windows.file],
    [hash('fixture: cli'), `openspider_${version}_darwin-universal.tar.gz`],
  ].map(([sha, name]) => `${sha}  ${name}\n`).join('');
}

// spec: OUT-261 [M3]
test('Beta-Release: nur openspider@beta, Universal-DMG aus SHA256SUMS, versionierte URL statt versionsloser Kopie', t => {
  const files = casks({ version: '1.0.0-beta.2', sums: parseSums(beta2Sums), repository });
  assert.deepEqual(Object.keys(files), ['Casks/openspider@beta.rb']);
  const cask = files['Casks/openspider@beta.rb'];
  assert.match(cask, /^cask "openspider@beta" do\n {2}version "1\.0\.0-beta\.2"\n/);
  assert.equal(cask.match(/^ {2}sha256 "([0-9a-f]{64})"$/m)[1], beta2Universal);
  assert.equal(cask.match(/^ {2}url "[^"]+"$/gm).join('\n'),
    '  url "https://github.com/wojcikdominik/openspider/releases/download/v#{version}/OpenSpider_#{version}_universal.dmg"');
  // OpenSpider-macOS.dmg hat dieselbe Prüfsumme, ist aber nur der feste latest/download-Link (OUT-261).
  assert.doesNotMatch(cask, /OpenSpider-macOS\.dmg|_aarch64\.dmg|_x64\.dmg|on_arm|on_intel/);
  assert.match(cask, /^ {2}conflicts_with cask: "openspider"$/m);
  assert.match(cask, /^ {2}auto_updates true$/m);
  rubySyntax(t, cask);
});

// spec: OUT-260 [M3]
test('Mindest-macOS 13 (Ventura) wie LSMinimumSystemVersion der App', () => {
  const conf = JSON.parse(readFileSync(join(root, 'apps/desktop/src-tauri/tauri.conf.json'), 'utf8'));
  assert.equal(conf.bundle.macOS.minimumSystemVersion, '13.0');
  for (const cask of Object.values(casks({ version: '1.0.0', sums: parseSums(stableSums('1.0.0')), repository }))) {
    assert.match(cask, /^ {2}depends_on macos: :ventura$/m);
    // Homebrew/OSDependsOn: das Symbol allein heißt „mindestens“; die Form mit ">=" wäre ein brew-style-Fehler.
    assert.doesNotMatch(cask, /depends_on macos: ">=/);
  }
});

// spec: OUT-166 [M3]
// spec: OUT-227 [M3]
test('Deinstallation entfernt Aufgaben-Registrierungen; zap nur App-Daten aus OUT-227 und der Bundle-ID, sortiert', () => {
  const cask = casks({ version: '1.0.0-beta.2', sums: parseSums(beta2Sums), repository })['Casks/openspider@beta.rb'];
  assert.match(cask, /^ {2}uninstall launchctl: "de\.openspider\.task\.\*",\n {12}delete:    "~\/Library\/LaunchAgents\/de\.openspider\.task\.\*\.plist"$/m);
  assert.match(cask, /^ {2}uninstall_preflight_steps do$/m);
  assert.match(cask, /^ {2}postflight_steps do$/m);
  const zap = cask.split('  zap trash: [\n')[1].split('\n  ]\n')[0].split('\n').map(line => /^ {4}"([^"]+)",$/.exec(line)[1]);
  assert.deepEqual(zap, zapPaths);
  // Homebrew Cask/ArrayAlphabetization vergleicht ohne Groß-/Kleinschreibung.
  assert.deepEqual([...zap].sort((a, b) => (a.toLowerCase() < b.toLowerCase() ? -1 : 1)), zap);
  const tauri = JSON.parse(readFileSync(join(root, 'apps/desktop/src-tauri/tauri.conf.json'), 'utf8')).identifier;
  for (const path of zap) assert.ok(path.includes('/OpenSpider') || path.includes(tauri), path);
  // OUT-227 (macOS): Einstellungen und Crawls, Logs, Cache.
  for (const path of ['~/Library/Application Support/OpenSpider', '~/Library/Logs/OpenSpider', '~/Library/Caches/OpenSpider']) {
    assert.ok(zap.includes(path), path);
  }
  // Ohne zap bleiben Benutzerdaten erhalten.
  assert.doesNotMatch(cask.split('uninstall launchctl:')[1].split('zap trash:')[0], /Application Support|Preferences|Caches|Logs/);
  // Keine Zeile mit Leerzeichen am Ende (RuboCop Layout/TrailingWhitespace gilt auch im Heredoc).
  assert.doesNotMatch(cask, /[ \t]$/m);
});

// spec: OUT-166 [M3]
// Layout/HashAlignment in Homebrews RuboCop-Konfiguration (EnforcedColonStyle: table): bei mehrzeiligen Hashes beginnen
// alle Werte in derselben Spalte. brew style läuft erst im Workflow (macOS); hier die erzeugte Form.
test('Mehrzeilige Hashes im Cask sind im table-Stil ausgerichtet (Layout/HashAlignment)', () => {
  const files = casks({ version: '1.0.0', sums: parseSums(stableSums('1.0.0')), repository });
  for (const cask of Object.values(files)) {
    // Eingebettetes Ruby-Skript (Heredoc) ist kein Cask-Code.
    const lines = cask.replace(/<<~'RUBY'\n[\s\S]*?\n {2}RUBY\n/, '').split('\n');
    let groups = 0;
    for (let i = 0; i < lines.length; i++) {
      if (!lines[i].endsWith(',') || !/^ +\w+: /.test(lines[i + 1] ?? '')) continue;
      // Gruppe: aktuelle Zeile bis zur letzten Folgezeile mit `schlüssel:`; erste Zeile darf `befehl "x", schlüssel:` sein.
      const group = [lines[i]];
      while (/^ +\w+: /.test(lines[i + 1] ?? '')) group.push(lines[++i]);
      const columns = group.flatMap(line => {
        const key = /\b\w+: +(?=\S)/.exec(line);
        return key ? [key.index + key[0].length] : [];
      });
      assert.ok(columns.length >= 2, group.join('\n'));
      assert.equal(new Set(columns).size, 1, `Werte nicht ausgerichtet:\n${group.join('\n')}`);
      groups += 1;
    }
    // preflight- und postflight-run plus uninstall (zap trash ist ein Array, kein Hash).
    assert.equal(groups, 3);
    assert.match(cask, /^ {8}args:           \["\{\{staged_path\}\}\/openspider-launchagents\.rb", "restore", "\{\{temp\}\}\/launchagents\.json"\],\n {8}writable_paths: \["~\/Library\/LaunchAgents"\]$/m);
    assert.match(cask, /^ {8}args:           \["\{\{staged_path\}\}\/openspider-launchagents\.rb", "save", "\{\{temp\}\}\/launchagents\.json"\],\n {8}writable_paths: \["~\/Library\/LaunchAgents"\]$/m);
  }
});

// spec: OUT-261 [M3]
test('Stabiler Release: openspider und openspider@beta, gleicher Text wie die P01-Veröffentlichung (generate.mjs)', t => {
  const directory = scratch(t);
  const assets = join(directory, 'assets');
  mkdirSync(assets);
  writeFileSync(join(assets, 'OpenSpider_1.0.0_universal.dmg'), 'fixture: universal dmg');
  writeFileSync(join(assets, 'OpenSpider-macOS.dmg'), 'fixture: universal dmg');
  const p01 = generate({ directory: assets, output: join(directory, 'p01'), version: '1.0.0', repository, mode: 'homebrew' });
  const files = casks({ version: '1.0.0', sums: parseSums(stableSums('1.0.0')), repository });
  assert.deepEqual(Object.keys(files), ['Casks/openspider.rb', 'Casks/openspider@beta.rb']);
  // publish-packages.yml (P01, ab 1.0) schreibt dieselbe Datei; gleicher Text heißt: kein zweiter Commit im Tap.
  assert.equal(files['Casks/openspider.rb'], p01['openspider.rb']);
  const stable = files['Casks/openspider.rb'];
  const beta = files['Casks/openspider@beta.rb'];
  assert.match(stable, /^cask "openspider" do$/m);
  assert.match(stable, /^ {2}conflicts_with cask: "openspider@beta"$/m);
  assert.match(beta, /^ {2}version "1\.0\.0"$/m);
  // Bis auf Token, Gegenstück und Livecheck-Regex sind beide Casks gleich.
  const neutral = text => text.replace(/^cask "[^"]+" do$/m, '').replace(/^ {2}conflicts_with cask: "[^"]+"$/m, '')
    .replace(/^ {4}regex\(.+\)$/m, '');
  assert.equal(neutral(stable), neutral(beta));
  rubySyntax(t, stable);
});

test('livecheck: stabiler Cask nur vX.Y.Z, Beta-Cask auch beta/rc; Strategie github_releases über die Download-URL', () => {
  const files = casks({ version: '1.0.0', sums: parseSums(stableSums('1.0.0')), repository });
  const stable = livecheckRegex(files['Casks/openspider.rb']);
  const beta = livecheckRegex(files['Casks/openspider@beta.rb']);
  for (const tag of ['v1.0.0', 'v1.2.10', '1.0.0']) assert.equal(stable.exec(tag)?.[1], tag.replace(/^v/, ''), tag);
  for (const tag of ['v1.0.0-beta.3', 'v1.0.0-rc.1', 'test-win-7471fcc6', 'v1.0', 'v1.0.0.1', 'v1.0.0+build']) assert.equal(stable.test(tag), false, tag);
  for (const tag of ['v1.0.0-beta.3', 'v1.0.0-rc.1', 'v1.0.0']) assert.equal(beta.exec(tag)?.[1], tag.slice(1), tag);
  for (const tag of ['test-a03-ad198eba', 'v1.0.0-alpha.1', 'v1.0.0-beta.3.1', 'v1.0']) assert.equal(beta.test(tag), false, tag);
  for (const cask of Object.values(files)) {
    assert.match(cask, /^ {2}livecheck do\n {4}url :url\n {4}regex\(.+\)\n {4}strategy :github_releases\n {2}end$/m);
  }
  assert.deepEqual(Object.values(caskChannels).map(c => c.token), ['openspider', 'openspider@beta']);
});

test('Fehler statt Raten: kaputte SHA256SUMS, fehlendes Universal-DMG, ungültige Version oder Repository', () => {
  assert.throws(() => parseSums(''), /leer/);
  assert.throws(() => parseSums(`${beta2Universal} OpenSpider_1.0.0-beta.2_universal.dmg\n`), /Ungültige Zeile/);
  assert.throws(() => parseSums(`${beta2Universal.toUpperCase()}  x.dmg\n`), /Ungültige Zeile/);
  assert.throws(() => parseSums(`${beta2Universal}  ../x.dmg\n`), /Ungültige Zeile/);
  assert.throws(() => parseSums(`${beta2Universal}  a.dmg\n${beta2Universal}  a.dmg\n`), /Doppelter/);
  const sums = parseSums(beta2Sums);
  assert.equal(sums.size, beta2Sums.trim().split('\n').length);
  // SHA256SUMS eines anderen Releases enthält das DMG dieser Version nicht.
  assert.throws(() => casks({ version: '1.0.0-beta.3', sums, repository }), /OpenSpider_1\.0\.0-beta\.3_universal\.dmg/);
  const perArchitecture = new Map([...sums].filter(([name]) => name !== 'OpenSpider_1.0.0-beta.2_universal.dmg'));
  assert.throws(() => casks({ version: '1.0.0-beta.2', sums: perArchitecture, repository }), /Universal-DMG/);
  for (const version of ['1.0.0-beta.3.1', 'v1.0.0', '1.0', '1.0.0-alpha.1', '1.0.0"\n']) {
    assert.throws(() => casks({ version, sums, repository }), /Version/, version);
  }
  for (const bad of ['', 'owner', 'owner/repo/x', 'owner/re"po']) {
    assert.throws(() => casks({ version: '1.0.0-beta.2', sums, repository: bad }), /Repository/, bad);
  }
});

test('Versionsreihenfolge wie das Update-Schema: beta < rc < stabil, numerisch', () => {
  const ordered = ['1.0.0-beta.2', '1.0.0-beta.10', '1.0.0-rc.1', '1.0.0', '1.0.1-beta.1', '1.0.1', '1.10.0'];
  const shuffled = [...ordered].reverse();
  assert.deepEqual(shuffled.sort(compareVersions), ordered);
  assert.equal(compareVersions('1.0.0-beta.3', '1.0.0-beta.3'), 0);
  assert.equal(isStable('1.0.0'), true);
  assert.equal(isStable('1.0.0-rc.1'), false);
  assert.throws(() => compareVersions('1.0.0', 'latest'), /Version/);
});

test('Tap-Aktualisierung: neu, unverändert, nie zurück, nie still überschrieben, @beta bleibt bei neuerer Beta', t => {
  const directory = scratch(t);
  const tap = join(directory, 'tap');
  mkdirSync(tap);
  const write = (name, version, sums) => {
    const out = join(directory, name);
    writeCasks(out, casks({ version, sums: parseSums(sums), repository }));
    return out;
  };
  const beta2 = write('beta2', '1.0.0-beta.2', beta2Sums);
  assert.deepEqual(updateTap({ generated: beta2, tap }), [{ path: 'Casks/openspider@beta.rb', version: '1.0.0-beta.2', action: 'neu' }]);
  assert.deepEqual(readdirSync(join(tap, 'Casks')), ['openspider@beta.rb']);
  assert.equal(updateTap({ generated: beta2, tap })[0].action, 'unverändert');
  // Gleiche Version, andere Bytes: nicht still ersetzen.
  const changed = join(directory, 'changed');
  mkdirSync(join(changed, 'Casks'), { recursive: true });
  writeFileSync(join(changed, 'Casks/openspider@beta.rb'), readFileSync(join(beta2, 'Casks/openspider@beta.rb'), 'utf8').replace(beta2Universal, 'f'.repeat(64)));
  assert.throws(() => updateTap({ generated: changed, tap }), /anderem Inhalt/);
  // Neuere Beta, dann ein älteres Release: kein Zurücksetzen.
  const beta3 = write('beta3', '1.0.0-beta.3', stableSums('1.0.0-beta.3').replace(/^.*darwin-universal.*\n/m, ''));
  assert.equal(updateTap({ generated: beta3, tap })[0].action, 'aktualisiert (vorher 1.0.0-beta.2)');
  const before = readFileSync(join(tap, 'Casks/openspider@beta.rb'), 'utf8');
  assert.throws(() => updateTap({ generated: beta2, tap }), /älter als 1\.0\.0-beta\.3/);
  assert.equal(readFileSync(join(tap, 'Casks/openspider@beta.rb'), 'utf8'), before);
  // Stabil 1.0.0 hinter einer (hypothetischen) 1.1.0-beta.1: openspider neu, openspider@beta bleibt.
  const beta11 = write('beta11', '1.1.0-beta.1', stableSums('1.1.0-beta.1'));
  updateTap({ generated: beta11, tap });
  const stable = write('stable', '1.0.0', stableSums('1.0.0'));
  assert.deepEqual(updateTap({ generated: stable, tap }).map(r => r.action), ['neu', 'übersprungen (Tap hat neuere 1.1.0-beta.1)']);
  assert.match(readFileSync(join(tap, 'Casks/openspider@beta.rb'), 'utf8'), /version "1\.1\.0-beta\.1"/);
  assert.match(readFileSync(join(tap, 'Casks/openspider.rb'), 'utf8'), /version "1\.0\.0"/);
  // Ein älterer stabiler Cask wird ebenfalls nicht zurückgesetzt; geprüft wird vor jedem Schreiben.
  const old = write('old', '1.0.0-rc.1', stableSums('1.0.0-rc.1'));
  assert.throws(() => updateTap({ generated: old, tap }), /älter/);
  // Nur bekannte Cask-Dateien.
  writeFileSync(join(stable, 'Casks/other.rb'), 'cask "other" do\nend\n');
  assert.throws(() => updateTap({ generated: stable, tap }), /Erwartet nur/);
  assert.throws(() => updateTap({ generated: join(directory, 'fehlt'), tap }), /keine/);
});

test('CLI: generate schreibt Tap-Struktur in leeren Ordner; update-tap meldet je Datei; falscher Aufruf bricht ab', t => {
  const directory = scratch(t);
  const sums = join(directory, 'SHA256SUMS');
  writeFileSync(sums, beta2Sums);
  const run = (...args) => spawnSync(process.execPath, [script, ...args], { encoding: 'utf8' });
  const out = join(directory, 'out');
  const ok = run('generate', '--version', '1.0.0-beta.2', '--sums', sums, '--repository', repository, '--output', out);
  assert.equal(ok.status, 0, ok.stderr);
  assert.equal(ok.stdout, 'erzeugt: Casks/openspider@beta.rb\n');
  assert.deepEqual(readdirSync(join(out, 'Casks')), ['openspider@beta.rb']);
  assert.match(run('generate', '--version', '1.0.0-beta.2', '--sums', sums, '--repository', repository, '--output', out).stderr, /leer/);
  const tap = join(directory, 'tap');
  mkdirSync(tap);
  const update = run('update-tap', '--generated', out, '--tap', tap);
  assert.equal(update.status, 0, update.stderr);
  assert.equal(update.stdout, 'Casks/openspider@beta.rb 1.0.0-beta.2: neu\n');
  assert.equal(run().status, 2);
  assert.equal(run('publish').status, 2);
  assert.equal(tapName, 'homebrew-openspider');
});

// Workflow .github/workflows/homebrew.yml: ohne Go nichts Öffentliches.
const workflow = readFileSync(join(root, '.github/workflows/homebrew.yml'), 'utf8');
function job(name) {
  const body = workflow.split(`\n  ${name}:\n`)[1];
  assert.ok(body, `Job ${name} fehlt`);
  return body.split(/\n {2}[a-z][a-z-]*:\n/)[0];
}

test('Workflow: Auslöser nur veröffentlichtes Release und manueller Start, keine Schreibrechte außer im Freigabe-Job', () => {
  const on = workflow.split('\non:\n')[1].split(/\n[a-z]/)[0];
  assert.deepEqual([...on.matchAll(/^ {2}([a-z_]+):/gm)].map(m => m[1]), ['release', 'workflow_dispatch']);
  assert.match(on, /^ {2}release:\n {4}types: \[published\]$/m);
  assert.match(on, /^ {6}publish:\n(?: {8}.+\n)*? {8}type: boolean\n {8}default: false$/m);
  assert.match(workflow, /^permissions:\n {2}contents: read$/m);
  assert.doesNotMatch(workflow, /contents: write|packages: write|id-token: write/);
  for (const line of workflow.split('\n').filter(l => /^\s+(?:- )?uses: /.test(l))) {
    assert.match(line, /uses: [\w.-]+\/[\w.-]+@[0-9a-f]{40} # v\d/, line);
  }
  // Der Tap-Token taucht nur im Freigabe-Job auf; Erzeugen und Prüfen laufen ohne Secrets.
  for (const name of ['generate', 'check']) assert.doesNotMatch(job(name), /secrets\.|environment:/, name);
  assert.match(job('generate'), /persist-credentials: false/);
  // Die Prüfung auf macOS arbeitet nur mit dem Artefakt, ohne Checkout; der lesende GITHUB_TOKEN steht nur in den
  // beiden Audit-Schritten (Ratenlimit der GitHub-API), nicht bei der Installation.
  const check = job('check');
  assert.doesNotMatch(check, /actions\/checkout|GH_TOKEN/);
  assert.deepEqual(check.match(/^.*github\.token.*$/gm), Array(2).fill('          HOMEBREW_GITHUB_API_TOKEN: ${{ github.token }}'));
  assert.doesNotMatch(check.split('name: Installation')[1], /github\.token/);
  assert.match(job('publish'), /persist-credentials: false/);
  assert.equal(workflow.split('secrets.HOMEBREW_TAP_TOKEN').length - 1, 1);
});

test('Workflow: Tap-Schreiben nur mit publish=true, auf main, nach bestandener Prüfung, im Environment homebrew mit Pflichtprüfern', () => {
  const publish = job('publish');
  assert.match(publish, /^ {4}needs: \[generate, check\]$/m);
  const condition = /^ {4}if: >-\n((?: {6}.+\n)+)/m.exec(publish)[1].replace(/\s+/g, ' ');
  for (const part of ["github.event_name == 'workflow_dispatch'", 'inputs.publish', "github.ref == 'refs/heads/main'",
    "needs.check.result == 'success'"]) {
    assert.ok(condition.includes(part), part);
  }
  assert.match(publish, /^ {4}environment: homebrew$/m);
  // Fail closed: ohne Pflichtprüfer im Environment wird nichts geschrieben.
  assert.match(publish, /environments\/homebrew/);
  assert.match(publish, /required_reviewers/);
  // Ziel ist immer <Eigentümer>/homebrew-openspider; kein Token in URL oder Argument.
  assert.match(publish, /homebrew-openspider/);
  assert.doesNotMatch(publish, /https:\/\/[^\s"]*@github\.com|--token|x-access-token/);
  assert.match(publish, /node scripts\/release\/homebrew\.mjs update-tap/);
  // Erzeugen nutzt nur SHA256SUMS des veröffentlichten Releases (kein Entwurf) und den Generator.
  const generateJob = job('generate');
  assert.match(generateJob, /--pattern SHA256SUMS/);
  assert.match(generateJob, /isDraft/);
  assert.match(generateJob, /node scripts\/release\/homebrew\.mjs generate/);
  const check = job('check');
  assert.match(check, /runs-on: macos-/);
  assert.match(check, /brew style/);
  assert.match(check, /brew audit --cask/);
});
