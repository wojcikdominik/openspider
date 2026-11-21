import { DatabaseSync } from 'node:sqlite';
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { copyFileSync, existsSync, mkdirSync, readFileSync, renameSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const archive = join(root, 'tests/fixtures/migrations');

/** Schemaversion des Programmstands (`SCHEMA_VERSION` in os-store). */
export function currentSchemaVersion() {
  const code = readFileSync(join(root, 'crates/os-store/src/schema.rs'), 'utf8');
  return Number(code.match(/pub const SCHEMA_VERSION: u32 = (\d+)/)[1]);
}

/** Quelldateien des Archivs für eine Schemaversion; der Crawl kommt aus dem Seed `schema-v<N>`. */
export function seedCopies(schema) {
  return {
    'crawl.ospider': `schema-v${schema}/crawl.ospider`,
    'internal_all.csv': `schema-v${schema}/internal_all.csv`,
    'profile.osconfig': 'config-v2/profile.osconfig',
    'basic.ospreset': 'app-v1/basic.ospreset',
    'nightly.ostask': 'app-v1/nightly.ostask',
    'profile.osauth': 'app-v1/profile.osauth',
    'settings.toml': 'app-v1/settings.toml',
    'ui-state.json': 'app-v1/ui-state.json',
  };
}

// spec: OUT-255
export function archiveFixture({ version, out = join(archive, 'releases'), commit, cli } = {}) {
  if (!/^(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)(?:-beta\.(?:0|[1-9]\d*)|-rc\.[1-9]\d*)?$/.test(version ?? '')) {
    throw new Error('Expected a release version (x.y.z, beta.N or rc.N)');
  }
  commit ??= execFileSync('git', ['rev-parse', 'HEAD'], { cwd: root, encoding: 'utf8' }).trim();
  if (!/^[a-f0-9]{40}$/.test(commit)) throw new Error('Expected full source commit');
  const destination = join(resolve(out), version);
  mkdirSync(dirname(destination), { recursive: true });
  if (existsSync(destination)) throw new Error('Archive already exists; never overwrite');
  const staging = `${destination}.tmp`;
  mkdirSync(staging); // exclusive, including concurrent invocations
  try {
    const schemaVersion = currentSchemaVersion();
    const copies = seedCopies(schemaVersion);
    if (!existsSync(join(archive, copies['crawl.ospider']))) {
      throw new Error('Seed schema is stale; add the new schema fixture first');
    }
    for (const [name, source] of Object.entries(copies)) copyFileSync(join(archive, source), join(staging, name));
    const stamp = text => text.replace(/^generator = .*$/m, `generator = "openspider ${version}"`);
    const configPath = join(staging, 'profile.osconfig');
    const configCode = readFileSync(join(root, 'crates/os-model/src/config.rs'), 'utf8');
    const configVersion = Number(configCode.match(/pub const CONFIG_SCHEMA_VERSION: u32 = (\d+)/)[1]);
    const configText = stamp(readFileSync(configPath, 'utf8'));
    if (Number(configText.match(/^schema_version = (\d+)/m)[1]) !== configVersion) throw new Error('Seed configuration is stale');
    writeFileSync(configPath, configText);
    const db = new DatabaseSync(join(staging, 'crawl.ospider'));
    try {
      const schema = db.prepare('PRAGMA user_version').get().user_version;
      if (schema !== schemaVersion) {
        throw new Error('Seed schema is stale; add the new schema fixture first');
      }
      db.prepare("UPDATE crawl_meta SET value=? WHERE key='generator'").run(`openspider ${version}`);
      const embedded = db.prepare('SELECT osconfig FROM crawl_config WHERE id=1').get().osconfig;
      const stamped = stamp(embedded);
      db.prepare('UPDATE crawl_config SET osconfig=? WHERE id=1').run(stamped);
      const put = db.prepare('INSERT OR REPLACE INTO crawl_meta(key,value) VALUES (?,?)');
      for (const key of ['openspider_version', 'last_writer_version']) put.run(key, version);
      put.run('config_sha256', createHash('sha256').update(stamped).digest('hex'));
    } finally { db.close(); }
    if (cli) {
      execFileSync(resolve(cli), ['--load-crawl', join(staging, 'crawl.ospider'), '--export-tabs', 'Internal:All', '--output-folder', join(staging, 'export')], {
        env: { ...process.env, OPENSPIDER_HOME: join(staging, 'home'), OPENSPIDER_SECRET_STORE: 'memory' }, stdio: 'pipe',
      });
      // The seeded fixture has fixed times and no response durations, so no volatile CSV cells.
      if (readFileSync(join(staging, 'export/internal_all.csv'), 'utf8').replaceAll('\r\n', '\n') !==
          readFileSync(join(staging, 'internal_all.csv'), 'utf8').replaceAll('\r\n', '\n')) throw new Error('Release export differs from golden');
      rmSync(join(staging, 'export'), { recursive: true });
      rmSync(join(staging, 'home'), { recursive: true });
    }
    const files = Object.keys(copies).map(name => ({ name, source: copies[name],
      sha256: createHash('sha256').update(readFileSync(join(staging, name))).digest('hex') }));
    writeFileSync(join(staging, 'manifest.json'), JSON.stringify({ version, commit, schema: schemaVersion, files }, null, 2) + '\n');
    writeFileSync(join(staging, 'README.md'), `# OpenSpider ${version}\n\nQuelle: ${commit}. Synthetische Testdaten aus tests/fixtures/migrations; Herkunft und SHA-256 je Datei in manifest.json. Crawl und Konfiguration mit Release-Version gestempelt. Auth-Schlüssel ausschließlich Testwert (11 × 32 Bytes).\n\nNach Veröffentlichung diesen Ordner unverändert nach tests/fixtures/migrations/releases/${version}/ übernehmen; cargo test --locked -p os-cli --test compat_archive prüft ihn automatisch.\n`);
    renameSync(staging, destination);
    return destination;
  } catch (error) {
    rmSync(staging, { recursive: true, force: true });
    throw error;
  }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const args = process.argv.slice(2);
  if (args.length % 2 || args.some((arg, i) => i % 2 === 0 && !['--version', '--out', '--cli'].includes(arg))) throw new Error('Usage: --version VERSION [--out DIR] [--cli BINARY]');
  const options = Object.fromEntries(Array.from({ length: args.length / 2 }, (_, i) => [args[i * 2].slice(2), args[i * 2 + 1]]));
  console.log(archiveFixture(options));
}
