import { appendFileSync, existsSync, readFileSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';
import { downloads } from './assets.mjs';

const versionPattern = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-beta\.(?:0|[1-9]\d*)|-rc\.[1-9]\d*)?$/;
function checkVersion(version) {
  if (!versionPattern.test(version ?? '')) throw new Error('Ungültige Release-Version');
}

// OUT-251 (Notes vor dem Bau): one checked-in notes document per version is the only source for the
// update dialog (latest.json/latest-beta.json, short section) and the release text (OUT-261). It is fixed before the build.
export function notesPath(version) {
  checkVersion(version);
  return join('docs', 'release', 'notes', `${version}.md`);
}

// OUT-251 (Nutzerentscheidung 09.11.2026): a notes document starts with the short update notes for the update
// dialog, fenced by `<!-- update -->` and `<!-- /update -->` on lines of their own (English list, `### Deutsch`,
// German list; 3 to 6 points each). Only this part goes into the update manifests; the rest of the document (the
// detailed notes) is the release text and the CHANGELOG entry. Versions before FIRST_SHORT_NOTES were published
// without it; their manifests carry the detailed notes.
export const FIRST_SHORT_NOTES = '1.0.0-beta.4';
export const UPDATE_OPEN = '<!-- update -->';
export const UPDATE_CLOSE = '<!-- /update -->';
const SHORT_MIN = 3;
const SHORT_MAX = 6;
const SHORT_LINE_MAX = 160;

// SemVer precedence for the release versions accepted by checkVersion (x.y.z, -beta.N, -rc.N).
export function compareVersions(a, b) {
  checkVersion(a);
  checkVersion(b);
  const parse = v => {
    const [core, pre] = v.split('-');
    const [kind, n] = pre ? pre.split('.') : ['~', '0'];
    return [...core.split('.').map(Number), kind, Number(n)];
  };
  const [x, y] = [parse(a), parse(b)];
  for (let i = 0; i < x.length; i++) {
    if (x[i] === y[i]) continue;
    return typeof x[i] === 'number' ? Math.sign(x[i] - y[i]) : (x[i] < y[i] ? -1 : 1);
  }
  return 0;
}

// Splits a notes document into the short update notes (null without the fenced section) and the detailed notes.
export function splitNotesDocument(text) {
  const doc = String(text ?? '').replaceAll('\r\n', '\n');
  const lines = doc.split('\n');
  const opens = lines.flatMap((line, i) => line.trim() === UPDATE_OPEN ? [i] : []);
  const closes = lines.flatMap((line, i) => line.trim() === UPDATE_CLOSE ? [i] : []);
  if (!opens.length && !closes.length) return { update: null, notes: doc };
  if (opens.length !== 1 || closes.length !== 1 || closes[0] < opens[0]) {
    throw new Error(`Kurze Update-Notes: genau ein Abschnitt ${UPDATE_OPEN} … ${UPDATE_CLOSE} erwartet`);
  }
  const update = lines.slice(opens[0] + 1, closes[0]).join('\n').trim();
  const notes = [...lines.slice(0, opens[0]), ...lines.slice(closes[0] + 1)].join('\n');
  return { update, notes };
}

// Short update notes of a version for the update manifests, validated: English and German list of 3 to 6 short
// single-line points each, separated by `### Deutsch`. Before FIRST_SHORT_NOTES: the detailed notes (legacy).
export function updateNotes(text, version) {
  checkVersion(version);
  const { update } = splitNotesDocument(text);
  if (update === null) {
    if (compareVersions(version, FIRST_SHORT_NOTES) < 0) return versionNotes(text, version);
    throw new Error(`Kurze Update-Notes ${version} fehlen: in ${notesPath(version)} oben einen Abschnitt ${UPDATE_OPEN} … ${UPDATE_CLOSE} mit je ${SHORT_MIN} bis ${SHORT_MAX} Punkten auf Englisch und unter ### Deutsch ergänzen (docs/release/release-rhythmus.md)`);
  }
  const fail = message => { throw new Error(`Kurze Update-Notes ${version} (${notesPath(version)}): ${message}`); };
  if (/\b(?:TODO|STUB)\b|Entwurfshinweis/.test(update)) fail('Entwurfsmarkierungen (TODO/STUB/Entwurfshinweis)');
  // Raw HTML and inline comments never belong into the manifest: older clients show the text unrendered.
  const html = update.split('\n').find(line => /<!--|<[a-z]/i.test(line));
  if (html) fail(`kein HTML und keine Kommentare im kurzen Abschnitt, gefunden: ${JSON.stringify(html)}`);
  const lines = update.split('\n');
  const marker = lines.flatMap((line, i) => line.trim() === '### Deutsch' ? [i] : []);
  if (marker.length !== 1) fail('genau eine Zeile ### Deutsch zwischen englischer und deutscher Liste erwartet');
  for (const [language, part] of [['Englisch', lines.slice(0, marker[0])], ['Deutsch', lines.slice(marker[0] + 1)]]) {
    const points = part.filter(line => line.trim());
    const wrong = points.find(line => !/^- \S/.test(line));
    if (wrong) fail(`${language}: nur einzeilige Listenpunkte „- …“ erlaubt, gefunden: ${JSON.stringify(wrong)}`);
    if (points.length < SHORT_MIN || points.length > SHORT_MAX) fail(`${language}: ${points.length} Punkte, erlaubt sind ${SHORT_MIN} bis ${SHORT_MAX}`);
    const long = points.find(line => line.length > SHORT_LINE_MAX);
    if (long) fail(`${language}: Punkt länger als ${SHORT_LINE_MAX} Zeichen, bitte kürzen: ${JSON.stringify(long)}`);
  }
  return `${update}\n`;
}

// Warnings for the short update notes: clients up to 1.0.0-beta.3 show the manifest text unrendered, so Markdown
// emphasis and links appear there as `**…**` and `[…](…)`. Allowed, but reported (B4-UPDDLG A4).
export function updateNotesWarnings(text) {
  const { update } = splitNotesDocument(text);
  if (update === null) return [];
  const warnings = [];
  if (/\*\*|__/.test(update)) warnings.push('Fettdruck (**…**) im kurzen Abschnitt: Clients bis 1.0.0-beta.3 zeigen die Sternchen roh');
  if (/\]\(/.test(update)) warnings.push('Link [Text](URL) im kurzen Abschnitt: Clients bis 1.0.0-beta.3 zeigen ihn als rohen Markdown-Text');
  return warnings;
}

function reportWarnings(version) {
  for (const warning of updateNotesWarnings(readFileSync(notesPath(version), 'utf8'))) {
    console.warn(`Warnung Update-Notes ${version}: ${warning}`);
    if (process.env.GITHUB_ACTIONS === 'true') console.log(`::warning title=Update-Notes::${warning}`);
    if (process.env.GITHUB_STEP_SUMMARY) appendFileSync(process.env.GITHUB_STEP_SUMMARY, `\n**Update-Notes (Warnung):** ${warning}\n`);
  }
}

// Validates the detailed notes of a document (the short update section removed): not empty, no level-1/2 headings
// (they would split the CHANGELOG section and the release text), no draft markers.
export function versionNotes(text, version) {
  checkVersion(version);
  const notes = splitNotesDocument(text).notes.trim();
  if (!notes) throw new Error(`Release-Notes ${version} sind leer: ${notesPath(version)} vor dem Bauen ausfüllen`);
  if (/^#{1,2}\s/m.test(notes)) throw new Error(`Release-Notes ${version}: keine Überschriften der Ebene 1/2 (# oder ##), höchstens ###`);
  if (/\b(?:TODO|STUB)\b|Entwurfshinweis/.test(notes)) throw new Error(`Release-Notes ${version} enthalten Entwurfsmarkierungen (TODO/STUB/Entwurfshinweis)`);
  return `${notes}\n`;
}

// CHANGELOG section of a version (without the heading line); null if absent.
export function changelogSection(changelog, version) {
  checkVersion(version);
  const sections = changelog.replaceAll('\r\n', '\n').split(/^## /m).slice(1);
  const section = sections.find(s => s.split('\n')[0].match(/^\[?([^\]\s]+)\]?/)?.[1] === version);
  return section === undefined ? null : section.slice(section.indexOf('\n') + 1).trim();
}

// CHANGELOG notes of a version; fails if the section is missing or empty.
export function releaseNotes(changelog, version) {
  const notes = changelogSection(changelog, version);
  if (!notes) throw new Error(`CHANGELOG-Eintrag ${version} fehlt oder ist leer`);
  return `${notes}\n`;
}

// Gate before the build (prepare job): notes document present and valid; the CHANGELOG section,
// if `requireChangelog` (release build) must exist and match it word for word.
export function checkNotes({ root = process.cwd(), version, requireChangelog = true }) {
  const path = join(root, notesPath(version));
  if (!existsSync(path)) {
    throw new Error(`Release-Notes für ${version} fehlen: ${notesPath(version)} anlegen und committen, bevor gebaut wird (docs/release/release-rhythmus.md)`);
  }
  const text = readFileSync(path, 'utf8');
  const notes = versionNotes(text, version);
  updateNotes(text, version);
  const section = changelogSection(readFileSync(join(root, 'CHANGELOG.md'), 'utf8'), version);
  if (section === null && !requireChangelog) return notes;
  if (!section) throw new Error(`CHANGELOG-Eintrag ${version} fehlt: node scripts/release/notes.mjs --changelog ${version} --date <JJJJ-MM-TT>`);
  if (`${section}\n` !== notes) {
    throw new Error(`CHANGELOG-Eintrag ${version} weicht von ${notesPath(version)} ab; die Notes-Datei ist die einzige Quelle`);
  }
  return notes;
}

// Inserts the CHANGELOG section for a version from its notes document (directly below [Unreleased]).
// An existing identical section is kept; a differing one is never overwritten.
export function withChangelogSection(changelog, version, notes, date) {
  if (!/^\d{4}-\d{2}-\d{2}$/.test(date ?? '') || Number.isNaN(Date.parse(date))) throw new Error('Datum erwartet: JJJJ-MM-TT');
  const body = versionNotes(notes, version);
  const existing = changelogSection(changelog, version);
  if (existing !== null) {
    if (`${existing}\n` === body) return changelog;
    throw new Error(`CHANGELOG enthält bereits einen abweichenden Eintrag ${version}; nicht überschrieben`);
  }
  const text = changelog.replaceAll('\r\n', '\n');
  const section = `## [${version}] - ${date}\n\n${body}\n`;
  const unreleased = /^## \[Unreleased\][^\n]*\n(?:(?!^## )[^\n]*\n?)*/m.exec(text);
  if (unreleased) {
    const end = unreleased.index + unreleased[0].length;
    const before = text.slice(0, end).replace(/\n*$/, '\n\n');
    return before + section + text.slice(end).replace(/^\n+/, '');
  }
  const first = text.search(/^## /m);
  return first < 0 ? `${text.replace(/\n*$/, '\n\n')}${section}` : text.slice(0, first) + section + text.slice(first);
}

// Feedback link in the download box: prereleases → beta page, stable → GitHub Discussions.
export function feedbackLink(version, repository) {
  checkVersion(version);
  return version.includes('-')
    ? '[openspider.de/beta](https://openspider.de/beta/)'
    : `[GitHub Discussions](https://github.com/${repository}/discussions)`;
}

// GitHub release body: download box, then the notes. Nothing else (OUT-261; draft hints go to the
// Actions summary only). Never used for the update manifests.
export function releaseBody({ version, repository, notes }) {
  checkVersion(version);
  if (!/^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/.test(repository ?? '')) throw new Error('Ungültiges GitHub-Repository');
  if (!notes?.trim()) throw new Error('Release-Notes fehlen');
  const { macos, windows } = downloads(version);
  const link = file => `[${file}](https://github.com/${repository}/releases/download/v${version}/${encodeURIComponent(file)})`;
  return [
    '## Download',
    '',
    '| System | Datei / File |',
    '|---|---|',
    `| **macOS 13+** (Apple Silicon und Intel) | ${link(macos.file)} |`,
    `| **Windows 10/11** (x64, auch Windows on ARM) | ${link(windows.file)} |`,
    '',
    'Mehr brauchst du nicht. Die übrigen Dateien unten sind für automatische Updates, die Kommandozeile (CLI), Firmen-Installationen (MSI) und Prüfsummen (`SHA256SUMS`).',
    '*That\'s all you need. The other files below are for automatic updates, the command line (CLI), managed installs (MSI) and checksums.*',
    '',
    `Anleitung: [Benutzerhandbuch](https://openspider.de/handbuch/) · [User guide](https://openspider.de/en/manual/) · Feedback: ${feedbackLink(version, repository)}`,
    '',
    '## Neu in dieser Version',
    '',
    notes.trim(),
    '',
  ].join('\n');
}

function main() {
  // notes.mjs <version> <update-notes> [--body <release-body> --repository <owner/name>]
  //   <update-notes>: short update notes for the manifests (OUT-251); --body: download box + detailed notes
  // notes.mjs --check <version>                      (prepare job, before the build)
  // notes.mjs --changelog <version> --date YYYY-MM-DD (writes the CHANGELOG section from the notes document)
  const { values, positionals } = parseArgs({ allowPositionals: true, options: {
    body: { type: 'string' }, repository: { type: 'string' }, check: { type: 'boolean' },
    changelog: { type: 'boolean' }, date: { type: 'string' } } });
  const usage = 'Aufruf: notes.mjs <Version> <Update-Notes> [--body <Datei> --repository <owner/name>] | --check <Version> | --changelog <Version> --date <JJJJ-MM-TT>';
  if (values.check || values.changelog) {
    if (positionals.length !== 1 || (values.check && values.changelog) || values.body || values.repository) throw new Error(usage);
    const [version] = positionals;
    if (values.check) {
      checkNotes({ version });
      reportWarnings(version);
      console.log(`Release-Notes ${version}: ${notesPath(version)} vorhanden und gleich dem CHANGELOG-Eintrag, kurze Update-Notes gültig`);
    } else {
      const notes = readFileSync(notesPath(version), 'utf8');
      writeFileSync('CHANGELOG.md', withChangelogSection(readFileSync('CHANGELOG.md', 'utf8'), version, notes, values.date));
    }
  } else {
    const [version, output] = positionals;
    if (!output || positionals.length !== 2 || values.date) throw new Error(usage);
    if (values.repository && !values.body) throw new Error('--repository nur zusammen mit --body');
    const notes = checkNotes({ version });
    reportWarnings(version);
    // OUT-251: the update manifests get only the short update notes; the release text the detailed ones.
    writeFileSync(output, updateNotes(readFileSync(notesPath(version), 'utf8'), version));
    if (values.body) writeFileSync(values.body, releaseBody({ version, repository: values.repository, notes }));
  }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    main();
  } catch (error) {
    // Clear message instead of a stack trace; annotated in GitHub Actions and in the run summary.
    console.error(error.message);
    if (process.env.GITHUB_ACTIONS === 'true') console.log(`::error title=Release-Notes::${error.message}`);
    if (process.env.GITHUB_STEP_SUMMARY) appendFileSync(process.env.GITHUB_STEP_SUMMARY, `\n**Release-Notes:** ${error.message}\n`);
    process.exitCode = 1;
  }
}
