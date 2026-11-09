// Unabhängige Abnahme B4-UPDDLG (Testagent; Grundlage allein die Spec, Bauer-Bericht nicht gelesen).
// Spec: docs/spec/06-output-ops.md OUT-251 (OS-Festlegung 09.11.2026: kurze Update-Notes), OUT-261 (Release-Text),
// docs/spec/05-ui.md UI-127 (Build-Nummer im Release-Workflow), docs/release/release-rhythmus.md (Format, Regeln),
// docs/release/notes-template.md. Aufruf nur über die Kommandozeilen der Skripte (Schwarzkasten) in temporären Projekten.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync, readdirSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';

const root = resolve(import.meta.dirname, '../..');
const notesScript = join(root, 'scripts/release/notes.mjs');
const manifestScript = join(root, 'scripts/release/manifest.mjs');

const EN = ['- **Faster** large crawls', '- Skip a version in the update dialog', '- See the [user guide](https://openspider.de/en/manual/)'];
const DE = ['- **Schnellere** große Crawls', '- Eine Version im Update-Dialog überspringen', '- Siehe das [Handbuch](https://openspider.de/handbuch/)'];
const DETAILED = [
  'DETAILED-SENTENCE One sentence summarising the release.',
  '',
  '- DETAILED-EN-POINT-A a long explanation that belongs to the release text and the changelog only',
  '- DETAILED-EN-POINT-B another long explanation',
  '',
  '### Deutsch',
  '',
  'DETAILED-SATZ Ein Satz als Zusammenfassung.',
  '',
  '- DETAILED-DE-PUNKT-A eine lange Erklärung, die nur in Release-Text und CHANGELOG gehört',
  '- DETAILED-DE-PUNKT-B noch eine lange Erklärung',
].join('\n');

/** Dokument im Format der Vorlage: kurzer Abschnitt oben, danach die ausführlichen Notes. */
const documentOf = ({ en = EN, de = DE, detailed = DETAILED, open = '<!-- update -->', close = '<!-- /update -->', marker = '### Deutsch' } = {}) =>
  `${open}\n${en.join('\n')}\n\n${marker}\n\n${de.join('\n')}\n${close}\n\n${detailed}\n`;

function project(t, version, text) {
  const dir = mkdtempSync(join(tmpdir(), 'openspider-upddlg-'));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  mkdirSync(join(dir, 'docs/release/notes'), { recursive: true });
  if (text !== null) writeFileSync(join(dir, 'docs/release/notes', `${version}.md`), text);
  // CHANGELOG-Eintrag wie im Wochenablauf aus den ausführlichen Notes (Rest des Dokuments) — unabhängig vom --check geschrieben.
  writeFileSync(join(dir, 'CHANGELOG.md'), `# Changelog\n\n## [Unreleased]\n\n## [${version}] - 2026-11-12\n\n${DETAILED.trim()}\n`);
  return dir;
}
const run = (dir, ...args) => spawnSync(process.execPath, [notesScript, ...args], { cwd: dir, encoding: 'utf8' });
/** Nur die Prüfung vor dem Bauen (prepare-Job) gegen den vorbereiteten CHANGELOG-Eintrag. */
const checkOnly = (dir, version) => run(dir, '--check', version);
/** Wochenablauf Mittwoch: CHANGELOG-Eintrag aus der Notes-Datei erzeugen, danach prüfen. */
function weekly(dir, version) {
  const changelog = run(dir, '--changelog', version, '--date', '2026-11-12');
  assert.equal(changelog.status, 0, changelog.stderr);
  return run(dir, '--check', version);
}

// ---------------------------------------------------------------------------------------------------------------------

// spec: OUT-251
test('gültiges Dokument ab beta.4: --check ist grün', t => {
  const dir = project(t, '1.0.0-beta.4', documentOf());
  const check = weekly(dir, '1.0.0-beta.4');
  assert.equal(check.status, 0, check.stderr);
});

// spec: OUT-251
test('--check ist rot, wenn der kurze Abschnitt ab 1.0.0-beta.4 fehlt, und nennt Datei und Abhilfe', t => {
  for (const version of ['1.0.0-beta.4', '1.0.0-beta.5', '1.0.0-rc.1', '1.0.0', '1.0.1']) {
    const dir = project(t, version, `${DETAILED}\n`);
    const check = checkOnly(dir, version);
    assert.notEqual(check.status, 0, `${version}: --check muss rot sein`);
    assert.match(check.stderr, new RegExp(`docs[\\\\/]release[\\\\/]notes[\\\\/]${version.replaceAll('.', '\\.')}\\.md`), `${version}: nennt die Datei`);
    assert.match(check.stderr, /update/i, `${version}: nennt den fehlenden Abschnitt`);
  }
});

// spec: OUT-251
test('bis 1.0.0-beta.3 (veröffentlicht) bleibt ein Dokument ohne kurzen Abschnitt grün; das Manifest trägt die ausführlichen Notes', t => {
  for (const version of ['1.0.0-beta.0', '1.0.0-beta.1', '1.0.0-beta.2', '1.0.0-beta.3']) {
    const dir = project(t, version, `${DETAILED}\n`);
    const check = checkOnly(dir, version);
    assert.equal(check.status, 0, `${version}: ${check.stderr}`);
    const out = join(dir, 'update-notes.md');
    const made = run(dir, version, out);
    assert.equal(made.status, 0, made.stderr);
    const text = readFileSync(out, 'utf8');
    assert.match(text, /DETAILED-EN-POINT-A/);
    assert.match(text, /DETAILED-DE-PUNKT-A/);
  }
});

// spec: OUT-251
test('nur der kurze Abschnitt landet in der Update-Notes-Datei (= Feld notes der Manifeste); die ausführlichen Notes nicht', t => {
  const dir = project(t, '1.0.0-beta.4', documentOf());
  assert.equal(weekly(dir, '1.0.0-beta.4').status, 0);
  const out = join(dir, 'update-notes.md');
  const made = run(dir, '1.0.0-beta.4', out);
  assert.equal(made.status, 0, made.stderr);
  const text = readFileSync(out, 'utf8');
  for (const line of [...EN, ...DE]) assert.ok(text.includes(line), `enthält ${line}`);
  assert.ok(text.includes('### Deutsch'), 'enthält die Trennzeile für die App');
  assert.ok(!text.includes('DETAILED'), 'keine ausführlichen Notes');
  assert.ok(!text.includes('<!--'), 'keine Markierungszeilen, die der Update-Dialog anzeigen würde');
  // Reihenfolge: englisch zuerst, dann Deutsch
  assert.ok(text.indexOf(EN[0]) < text.indexOf('### Deutsch') && text.indexOf('### Deutsch') < text.indexOf(DE[0]));
});

// spec: OUT-251
test('der kurze Abschnitt gelangt als notes in beide Manifeste (manifest.mjs), die ausführlichen Notes nicht', t => {
  const version = '1.0.0-beta.4';
  const dir = project(t, version, documentOf());
  assert.equal(weekly(dir, version).status, 0);
  const out = join(dir, 'update-notes.md');
  assert.equal(run(dir, version, out).status, 0);
  const assets = mkdtempSync(join(tmpdir(), 'openspider-upddlg-assets-'));
  t.after(() => rmSync(assets, { recursive: true, force: true }));
  const signature = Buffer.from('untrusted comment: fixture\nRWRmaXh0dXJl\ntrusted comment: fixture\nZml4dHVyZQ==\n').toString('base64');
  for (const name of ['OpenSpider_aarch64.app.tar.gz', 'OpenSpider_x86_64.app.tar.gz', `OpenSpider_${version}_x64-setup.exe`, `OpenSpider_${version}_x64_en-US.msi`]) {
    writeFileSync(join(assets, name), `fixture:${name}`);
    writeFileSync(join(assets, `${name}.sig`), signature);
  }
  const made = spawnSync(process.execPath, [manifestScript, assets, version, 'example/openspider', out, '2026-11-12T08:00:00Z'], { cwd: dir, encoding: 'utf8' });
  assert.equal(made.status, 0, made.stderr);
  for (const file of ['latest.json', 'latest-beta.json']) {
    const manifest = JSON.parse(readFileSync(join(assets, file), 'utf8'));
    assert.equal(manifest.version, version);
    assert.ok(manifest.notes.includes(EN[0]) && manifest.notes.includes(DE[0]), `${file}: kurze Notes beider Sprachen`);
    assert.ok(!manifest.notes.includes('DETAILED'), `${file}: keine ausführlichen Notes`);
  }
  assert.ok(readdirSync(assets).includes('latest-beta.json'));
});

// spec: OUT-251
// spec: OUT-261
test('Release-Text und CHANGELOG-Eintrag bestehen aus den ausführlichen Notes, ohne den kurzen Abschnitt', t => {
  const version = '1.0.0-beta.4';
  const dir = project(t, version, documentOf());
  assert.equal(weekly(dir, version).status, 0);
  const changelog = readFileSync(join(dir, 'CHANGELOG.md'), 'utf8');
  assert.match(changelog, /DETAILED-EN-POINT-A/);
  assert.match(changelog, /DETAILED-DE-PUNKT-B/);
  assert.ok(!changelog.includes('<!-- update'), 'CHANGELOG ohne Markierungen');
  assert.ok(!changelog.includes(EN[0]) && !changelog.includes(DE[0]), 'CHANGELOG ohne die kurzen Punkte');
  const out = join(dir, 'update-notes.md');
  const body = join(dir, 'release-notes.md');
  const made = run(dir, version, out, '--body', body, '--repository', 'example/openspider');
  assert.equal(made.status, 0, made.stderr);
  const text = readFileSync(body, 'utf8');
  assert.match(text, /DETAILED-EN-POINT-A/);
  assert.match(text, /DETAILED-DE-PUNKT-A/);
  assert.ok(!text.includes('<!--'), 'Release-Text ohne Markierungen');
  assert.ok(!text.includes(EN[0]) && !text.includes(DE[0]), 'Release-Text ohne die kurzen Punkte');
  // OUT-261: der Download-Kasten steht vor den Notes; die kurzen Notes gehören nicht in den Release-Text.
  assert.ok(text.indexOf('Download') >= 0 && text.indexOf('Download') < text.indexOf('DETAILED-EN-POINT-A'));
});

// spec: OUT-251
test('--check ist rot, wenn der CHANGELOG-Eintrag vom Rest des Dokuments (den ausführlichen Notes) abweicht', t => {
  const version = '1.0.0-beta.4';
  const dir = project(t, version, documentOf());
  assert.equal(weekly(dir, version).status, 0);
  // Ein Wort im Dokument ändern, CHANGELOG bleibt: das Dokument ist die einzige Quelle.
  const path = join(dir, 'docs/release/notes', `${version}.md`);
  writeFileSync(path, readFileSync(path, 'utf8').replace('DETAILED-EN-POINT-A', 'DETAILED-EN-POINT-CHANGED'));
  const check = run(dir, '--check', version);
  assert.notEqual(check.status, 0);
  assert.match(check.stderr, /CHANGELOG/);
  // Eine Änderung nur im kurzen Abschnitt berührt den CHANGELOG-Eintrag dagegen nicht.
  writeFileSync(path, documentOf({ en: ['- **Faster** crawls', '- Another short point', '- Third short point'] }));
  assert.equal(run(dir, '--check', version).status, 0);
});

// ---------------------------------------------------------------------------------------------------------------------
// Regeln des kurzen Abschnitts (docs/release/release-rhythmus.md, OUT-251): je Sprache 3 bis 6 einzeilige Punkte ≤ 160 Zeichen
// ---------------------------------------------------------------------------------------------------------------------
const points = n => Array.from({ length: n }, (_, i) => `- Point ${i + 1} with **bold** and a [link](https://openspider.de/)`);

// spec: OUT-251
test('Punktzahl je Sprache: 3 und 6 sind grün, 0, 1, 2 und 7 sind rot — englisch wie deutsch', t => {
  for (const n of [3, 4, 5, 6]) {
    const dir = project(t, '1.0.0-beta.4', documentOf({ en: points(n), de: points(n) }));
    assert.equal(checkOnly(dir, '1.0.0-beta.4').status, 0, `${n} Punkte`);
  }
  for (const n of [0, 1, 2, 7, 10]) {
    for (const [label, doc] of [
      ['englisch', documentOf({ en: points(n), de: points(4) })],
      ['deutsch', documentOf({ en: points(4), de: points(n) })],
    ]) {
      const dir = project(t, '1.0.0-beta.4', doc);
      const check = checkOnly(dir, '1.0.0-beta.4');
      assert.notEqual(check.status, 0, `${n} ${label}e Punkte müssen rot sein`);
      assert.match(check.stderr, /docs[\\/]release[\\/]notes[\\/]1\.0\.0-beta\.4\.md/);
    }
  }
});

// spec: OUT-251
test('ein Punkt hat höchstens 160 Zeichen: 160 grün, 161 rot', t => {
  const line = length => `- ${'x'.repeat(length - 2)}`;
  assert.equal(line(160).length, 160);
  const ok = project(t, '1.0.0-beta.4', documentOf({ en: [line(160), ...EN.slice(1)] }));
  assert.equal(checkOnly(ok, '1.0.0-beta.4').status, 0);
  for (const doc of [documentOf({ en: [line(161), ...EN.slice(1)] }), documentOf({ de: [line(161), ...DE.slice(1)] })]) {
    const dir = project(t, '1.0.0-beta.4', doc);
    assert.notEqual(checkOnly(dir, '1.0.0-beta.4').status, 0);
  }
});

// spec: OUT-251
test('jeder Punkt ist eine Zeile „- …“: Fortsetzungszeilen, Absätze und Überschriften im kurzen Abschnitt sind rot', t => {
  for (const en of [
    [EN[0], `${EN[1]}\n  continued on a second line`, EN[2]],
    [EN[0], 'A paragraph that is not a list point', EN[1], EN[2]],
    [EN[0], '#### A heading', EN[1], EN[2]],
  ]) {
    const dir = project(t, '1.0.0-beta.4', documentOf({ en }));
    assert.notEqual(checkOnly(dir, '1.0.0-beta.4').status, 0, JSON.stringify(en));
  }
});

// spec: OUT-251
test('Markierungen: genau einmal, je allein auf ihrer Zeile; ### Deutsch zwischen den Listen ist Pflicht', t => {
  const cases = {
    'öffnende Markierung doppelt': `${documentOf()}\n<!-- update -->\n- a\n- b\n- c\n<!-- /update -->\n`,
    'schließende Markierung fehlt': documentOf({ close: '' }),
    'öffnende Markierung fehlt': documentOf({ open: '' }),
    'Markierung nicht allein auf der Zeile': documentOf({ open: '<!-- update --> text' }),
    'schließende Markierung vor der öffnenden': `<!-- /update -->\n${documentOf()}`,
    'Trennzeile ### Deutsch fehlt': documentOf({ marker: '' }),
    'Trennzeile doppelt': documentOf({ de: ['### Deutsch', ...DE] }),
  };
  for (const [label, doc] of Object.entries(cases)) {
    const dir = project(t, '1.0.0-beta.4', doc);
    assert.notEqual(checkOnly(dir, '1.0.0-beta.4').status, 0, label);
  }
});

// spec: OUT-251
test('Entwurfsmarkierungen im kurzen Abschnitt (TODO/STUB) sind rot', t => {
  for (const word of ['TODO', 'STUB']) {
    const dir = project(t, '1.0.0-beta.4', documentOf({ en: [`- ${word} write this`, EN[1], EN[2]] }));
    assert.notEqual(checkOnly(dir, '1.0.0-beta.4').status, 0, word);
  }
});

// spec: OUT-251
test('Fettdruck und Links im kurzen Abschnitt sind erlaubt und bleiben im Manifest-Text erhalten', t => {
  const dir = project(t, '1.0.0-beta.4', documentOf());
  assert.equal(weekly(dir, '1.0.0-beta.4').status, 0);
  const out = join(dir, 'update-notes.md');
  assert.equal(run(dir, '1.0.0-beta.4', out).status, 0);
  const text = readFileSync(out, 'utf8');
  assert.ok(text.includes('**Faster**'));
  assert.ok(text.includes('[user guide](https://openspider.de/en/manual/)'));
});

// spec: OUT-251
test('die Vorlage docs/release/notes-template.md ist mit ausgefüllten Platzhaltern ein gültiges Dokument', t => {
  const template = readFileSync(join(root, 'docs/release/notes-template.md'), 'utf8');
  assert.ok(template.includes('<!-- update -->') && template.includes('<!-- /update -->'));
  const filled = template.replace(/<[^>\n]*>/g, match => (match.startsWith('<!--') ? match : 'filled'));
  const dir = project(t, '1.0.0-beta.4', filled);
  writeFileSync(join(dir, 'CHANGELOG.md'), '# Changelog\n\n## [Unreleased]\n\n');
  const check = weekly(dir, '1.0.0-beta.4');
  assert.equal(check.status, 0, `${check.stderr}\n${filled}`);
});

// ---------------------------------------------------------------------------------------------------------------------
// Build-Nummer im Release-Workflow (UI-127): einmal im vollständigen Checkout ermittelt, an alle Bau-Jobs weitergegeben
// ---------------------------------------------------------------------------------------------------------------------
const workflow = name => readFileSync(join(root, '.github/workflows', name), 'utf8');
/** Text eines Jobs auf oberster Ebene von `jobs:` (zwei Leerzeichen eingerückter Schlüssel bis zum nächsten). */
function job(text, name) {
  const jobs = text.split(/^jobs:\n/m)[1];
  const parts = jobs.split(/^  (?=[A-Za-z0-9_-]+:\s*$)/m);
  const found = parts.find(part => part.startsWith(`${name}:`));
  assert.ok(found, `Job ${name} vorhanden`);
  return found;
}

// spec: UI-127
test('Release-Workflow: Build-Nummer = git rev-list --count HEAD, einmal im vollständigen Checkout (fetch-depth: 0) des prepare-Jobs', () => {
  const prepare = job(workflow('release.yml'), 'prepare');
  assert.match(prepare, /fetch-depth:\s*0/);
  assert.match(prepare, /git rev-list --count HEAD/);
  // genau eine Stelle ermittelt die Nummer
  assert.equal([...workflow('release.yml').matchAll(/rev-list --count/g)].length, 1);
  for (const file of ['release-macos.yml', 'release-windows.yml']) assert.ok(!/rev-list --count/.test(workflow(file)), `${file} zählt nicht selbst`);
  assert.match(prepare, /outputs:[\s\S]*build_number/);
});

// spec: UI-127
test('Release-Workflow: alle Bau-Jobs (macOS, Windows, Linux-Kern) erhalten dieselbe Nummer als OPENSPIDER_BUILD_NUMBER', () => {
  const release = workflow('release.yml');
  for (const name of ['macos', 'windows']) assert.match(job(release, name), /build_number:\s*\$\{\{\s*needs\.prepare\.outputs\.build_number\s*\}\}/, name);
  assert.match(job(release, 'linux-core'), /OPENSPIDER_BUILD_NUMBER:\s*\$\{\{\s*needs\.prepare\.outputs\.build_number\s*\}\}/);
  for (const file of ['release-macos.yml', 'release-windows.yml']) assert.match(workflow(file), /OPENSPIDER_BUILD_NUMBER=.*GITHUB_ENV/, `${file} exportiert die Nummer für den Bau`);
});

// spec: UI-127
test('Release-Workflow: ein Lauf ohne gültige Nummer bricht ab (flacher Checkout, nicht numerisch, 0)', () => {
  const prepare = job(workflow('release.yml'), 'prepare');
  assert.match(prepare, /is-shallow-repository/);
  assert.match(prepare, /\[1-9\]\[0-9\]\*/, 'positive Zahl ohne führende Null');
  assert.match(prepare, /exit 1/);
});
