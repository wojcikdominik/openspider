import { readdirSync, readFileSync, writeFileSync, existsSync, rmSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { releaseNotes } from './notes.mjs';

const targets = [
  ['darwin-aarch64', /^OpenSpider_aarch64\.app\.tar\.gz$/],
  ['darwin-x86_64', /^OpenSpider_x86_64\.app\.tar\.gz$/],
  ['windows-x86_64-nsis', /_x64-setup\.exe$/],
  ['windows-x86_64-msi', /_x64_[A-Za-z-]+\.msi$/],
];

// OUT-254/OUT-261 (K3-16): A stable release carries latest.json and latest-beta.json with identical content
// (clients up to 1.0.0 chose their file at build time and read releases/latest/download/…). A beta release
// carries only latest-beta.json and never latest.json, so releases/latest/download/latest.json stays stable.
export function manifestFiles(version) {
  return isPrerelease(version) ? ['latest-beta.json'] : ['latest.json', 'latest-beta.json'];
}

export function isPrerelease(version) {
  return String(version).includes('-');
}

export function manifests({ directory, version, repository, notes, date, allowUnsigned = false }) {
  // Validate version independently of changelog contents.
  releaseNotes(`## ${version}\nvalidation`, version);
  if (!/^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/.test(repository)) throw new Error('Ungültiges GitHub-Repository');
  if (!date || Number.isNaN(Date.parse(date))) throw new Error('Release-Datum fehlt oder ist ungültig');
  // Never leave stale channel metadata behind on a repeated invocation.
  for (const file of ['latest.json', 'latest-beta.json']) rmSync(join(directory, file), { force: true });
  const assets = readdirSync(directory);
  const platforms = {};
  const missing = [];
  for (const [key, pattern] of targets) {
    const matches = assets.filter(a => pattern.test(a));
    if (matches.length > 1) throw new Error(`Mehrdeutiges Paket für ${key}`);
    const name = matches[0];
    if (!name || !existsSync(join(directory, `${name}.sig`))) { missing.push(key); continue; }
    const signature = readFileSync(join(directory, `${name}.sig`), 'utf8').trim();
    const decoded = Buffer.from(signature, 'base64').toString('utf8');
    if (!/^[A-Za-z0-9+/]+={0,2}$/.test(signature) || !decoded.startsWith('untrusted comment:') || decoded.split('\n').length < 4) {
      throw new Error(`Ungültige Tauri/Minisign-Signatur: ${name}`);
    }
    platforms[key] = { signature, url: `https://github.com/${repository}/releases/download/v${version}/${encodeURIComponent(name)}` };
  }
  if (missing.length) {
    if (!allowUnsigned) throw new Error(`Signierte Updater-Artefakte fehlen: ${missing.join(', ')}`);
    return { status: 'unsigniert', missing, files: [] };
  }
  const manifest = { version, notes, pub_date: new Date(date).toISOString(), platforms };
  const files = manifestFiles(version);
  for (const name of files) writeFileSync(join(directory, name), `${JSON.stringify(manifest, null, 2)}\n`);
  return { status: 'signierte Updater-Artefakte', files, platforms: Object.keys(platforms) };
}
if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const [directory, version, repository, notesFile, date, mode] = process.argv.slice(2);
  const result = manifests({ directory, version, repository, notes: readFileSync(notesFile, 'utf8'), date, allowUnsigned: mode === '--allow-unsigned' });
  console.log(JSON.stringify(result));
}
