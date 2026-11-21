// OUT-254/OUT-261 (K3-16): Beta-Manifest am festen Hilfs-Release `update-beta` pflegen.
// Ablauf und Regeln: docs/release/release-process.md, Abschnitt „Update-Kanäle“.
//
// - Beta-Release veröffentlicht → `latest-beta.json` am Tag `update-beta` ersetzen.
// - Stabiles Release veröffentlicht → nur ersetzen, wenn die Version nach SemVer neuer ist als der bisherige Inhalt.
// - Nie eine ältere Version hochladen (kein Zurücksetzen); eine Rücknahme entscheidet der Eigentümer.
//
// Das Skript entscheidet nur und prüft Eingaben; Lesen und Hochladen macht der Workflow
// .github/workflows/channel-manifest.yml mit gh. Es schreibt nichts außer seiner Ausgabe.
import { readFileSync, existsSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { isDeepStrictEqual } from 'node:util';
import { compareVersions } from './notes.mjs';
import { isPrerelease, manifestFiles } from './manifest.mjs';

export const HELPER_TAG = 'update-beta';
export const BETA_MANIFEST = 'latest-beta.json';

function versionOfTag(tag) {
  const match = /^v(\d+\.\d+\.\d+(?:-(?:beta|rc)\.\d+)?)$/.exec(tag ?? '');
  if (!match) throw new Error(`Ungültiger Release-Tag: ${tag} (erwartet v<Version>, nicht ${HELPER_TAG})`);
  return match[1];
}

function assetNames(release) {
  return (release.assets ?? []).map(asset => (typeof asset === 'string' ? asset : asset.name)).sort();
}

// Das gerade veröffentlichte versionierte Release: veröffentlicht, kein Pre-Release, Latest genau dann, wenn stabil.
export function checkSourceRelease({ release, latestTag }) {
  const version = versionOfTag(release.tagName);
  if (release.isDraft) throw new Error(`${release.tagName} ist ein Entwurf; das Beta-Manifest folgt erst nach der Veröffentlichung.`);
  if (release.isPrerelease) throw new Error(`${release.tagName} ist als Pre-Release markiert; Betas und stabile Releases sind keine Pre-Releases (OUT-261).`);
  const beta = isPrerelease(version);
  if (beta && latestTag === release.tagName) {
    throw new Error(`Beta ${release.tagName} ist als Latest markiert; Latest ist immer die neueste stabile Version (OUT-261).`);
  }
  if (!beta && latestTag !== release.tagName) {
    throw new Error(`Stabiles Release ${release.tagName} ist nicht Latest (Latest: ${latestTag || 'keins'}); erst als Latest veröffentlichen (OUT-261).`);
  }
  const names = assetNames(release).filter(name => name.startsWith('latest'));
  const expected = [...manifestFiles(version)].sort();
  if (!isDeepStrictEqual(names, expected)) {
    throw new Error(`${release.tagName} trägt ${names.join(', ') || 'keine Manifeste'}, erwartet ${expected.join(', ')} (OUT-254, OUT-261).`);
  }
  return version;
}

// Das Hilfs-Release: fester Tag, Pre-Release (wird so nie Latest), veröffentlicht, einziges Asset latest-beta.json.
export function checkHelperRelease({ release, latestTag }) {
  if (release.tagName !== HELPER_TAG) throw new Error(`Hilfs-Release muss den Tag ${HELPER_TAG} haben, nicht ${release.tagName}.`);
  if (release.isDraft) throw new Error(`${HELPER_TAG} ist ein Entwurf; die Beta-Manifest-URL wäre öffentlich nicht erreichbar.`);
  if (!release.isPrerelease) throw new Error(`${HELPER_TAG} muss als Pre-Release markiert sein, damit es nie Latest wird.`);
  if (latestTag === HELPER_TAG) throw new Error(`${HELPER_TAG} ist als Latest markiert; das darf nie sein.`);
  const names = assetNames(release);
  if (!isDeepStrictEqual(names, [BETA_MANIFEST])) {
    throw new Error(`${HELPER_TAG} muss genau ${BETA_MANIFEST} tragen, trägt aber: ${names.join(', ') || 'nichts'}.`);
  }
}

// Inhalt eines Kanal-Manifests: Version des Releases und Paket-URLs auf releases/download/v<Version>/.
export function checkManifest(manifest, version, repository) {
  if (manifest?.version !== version) throw new Error(`${BETA_MANIFEST} nennt ${manifest?.version}, erwartet ${version}.`);
  const entries = Object.entries(manifest.platforms ?? {});
  if (!entries.length) throw new Error(`${BETA_MANIFEST} von ${version} hat keine Plattformen.`);
  const prefix = `https://github.com/${repository}/releases/download/v${version}/`;
  for (const [key, entry] of entries) {
    if (typeof entry?.url !== 'string' || !entry.url.startsWith(prefix)) {
      throw new Error(`Paket-URL ${key} zeigt nicht auf ${prefix} (OUT-254).`);
    }
    if (typeof entry.signature !== 'string' || !entry.signature.trim()) throw new Error(`Signatur ${key} fehlt.`);
  }
}

// Regel aus OUT-254: nach einer Beta ersetzen, nach einer stabilen Version nur, wenn sie neuer ist; nie zurück.
export function decide({ candidate, current, version }) {
  if (!current) throw new Error(`${HELPER_TAG} hat kein lesbares ${BETA_MANIFEST}; das Hilfs-Release legt der Eigentümer einmalig an.`);
  const order = compareVersions(version, current.version);
  if (order > 0) return { action: 'replace', reason: `${version} ist neuer als ${current.version}` };
  if (order === 0) {
    if (isDeepStrictEqual(candidate, current)) return { action: 'skip', reason: `${version} steht bereits am Tag ${HELPER_TAG}` };
    throw new Error(`${HELPER_TAG} nennt bereits ${version}, aber mit anderem Inhalt; Eigentümer entscheidet (kein automatisches Überschreiben).`);
  }
  if (!isPrerelease(version)) {
    return { action: 'skip', reason: `stabile Version ${version} ist nicht neuer als ${current.version}; Beta-Manifest bleibt` };
  }
  throw new Error(`Beta ${version} ist älter als ${current.version} am Tag ${HELPER_TAG}; nie zurücksetzen, Eigentümer entscheidet.`);
}

function readJson(path, label) {
  if (!path || !existsSync(path)) throw new Error(`${label} fehlt: ${path}`);
  try {
    return JSON.parse(readFileSync(path, 'utf8'));
  } catch {
    throw new Error(`${label} ist kein gültiges JSON: ${path}`);
  }
}

export function run(args) {
  const options = {};
  for (let i = 0; i < args.length; i += 2) {
    if (!args[i]?.startsWith('--') || args[i + 1] === undefined) throw new Error(`Unerwartetes Argument: ${args[i]}`);
    options[args[i].slice(2)] = args[i + 1];
  }
  for (const name of ['release', 'helper', 'candidate', 'current', 'repository']) {
    if (!options[name]) throw new Error(`--${name} fehlt`);
  }
  const latestTag = options['latest-tag'] ?? '';
  const release = readJson(options.release, 'Release-Metadaten');
  const version = checkSourceRelease({ release, latestTag });
  checkHelperRelease({ release: readJson(options.helper, 'Metadaten von update-beta'), latestTag });
  const candidate = readJson(options.candidate, `${BETA_MANIFEST} des Releases`);
  checkManifest(candidate, version, options.repository);
  if (!isPrerelease(version)) {
    // Stabil: latest.json und latest-beta.json müssen bytegleich sein (Clients bis 1.0.0).
    if (!options.stable) throw new Error('--stable fehlt (latest.json des stabilen Releases)');
    if (readFileSync(options.stable, 'utf8') !== readFileSync(options.candidate, 'utf8')) {
      throw new Error(`latest.json und ${BETA_MANIFEST} von ${version} unterscheiden sich (OUT-254).`);
    }
  }
  const current = readJson(options.current, `bisheriges ${BETA_MANIFEST} am Tag ${HELPER_TAG}`);
  return { version, ...decide({ candidate, current, version }) };
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    const result = run(process.argv.slice(2));
    process.stdout.write(`action=${result.action}\nversion=${result.version}\nreason=${result.reason}\n`);
  } catch (error) {
    process.stderr.write(`${error.message}\n`);
    process.exit(1);
  }
}
