// Package metadata uses the bytes and filenames produced by R01/R02/R04.
import { createHash } from 'node:crypto';
import { readdirSync, readFileSync, writeFileSync, mkdirSync, lstatSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';

export const schemaVersion = '1.9.0';

// B3-BREW (Nutzerentscheidung 05.11.2026): eigener Tap ab beta.3. `openspider` führt nur stabile Versionen,
// `openspider@beta` die jeweils neueste veröffentlichte Version (Beta oder stabil). Beide installieren dieselbe
// App und schließen sich gegenseitig aus. Einzige Quelle des Cask-Texts für generate() (P01, winget/Smoke) und
// scripts/release/homebrew.mjs (Tap-Workflow), damit beide Wege byte-gleiche Casks schreiben.
export const caskChannels = {
  stable: { token: 'openspider', other: 'openspider@beta', tag: String.raw`/^v?(\d+\.\d+\.\d+)$/i` },
  beta: { token: 'openspider@beta', other: 'openspider', tag: String.raw`/^v?(\d+\.\d+\.\d+(?:-(?:beta|rc)\.\d+)?)$/i` },
};
// Homebrew's RuboCop config enforces Layout/HashAlignment in `table` style: the values of a multi-line hash
// start in one column (`args:           [...]` above `writable_paths: [...]`). `depends_on macos: :ventura` is the
// minimum release (Homebrew/OSDependsOn rejects the older `">= :ventura"`); Ventura = macOS 13 (OUT-260).
// App data removed only by an explicit `brew uninstall --zap`: OUT-227 paths (settings, default crawls, logs,
// cache) and what macOS/WebKit store for the bundle id org.openspider.desktop (tauri.conf.json). Sorted
// case-insensitively, as Homebrew's Cask/ArrayAlphabetization cop requires.
export const zapPaths = [
  '~/Library/Application Support/OpenSpider',
  '~/Library/Application Support/org.openspider.desktop',
  '~/Library/Caches/OpenSpider',
  '~/Library/Caches/org.openspider.desktop',
  '~/Library/HTTPStorages/org.openspider.desktop',
  '~/Library/Logs/OpenSpider',
  '~/Library/Preferences/org.openspider.desktop.plist',
  '~/Library/Saved Application State/org.openspider.desktop.savedState',
  '~/Library/WebKit/org.openspider.desktop',
];

export function caskSource({ channel = 'stable', version, sha256, url }) {
  const { token, other, tag } = caskChannels[channel] ?? {};
  if (!token) throw Error('Ungültiger Cask-Kanal');
  if (!/^\d+\.\d+\.\d+(?:-(?:beta|rc)\.\d+)?$/.test(version ?? '')) throw Error('Ungültige Version');
  if (!/^[0-9a-f]{64}$/.test(sha256 ?? '')) throw Error('Ungültige SHA-256');
  // Only characters of an encoded URL: nothing that could end the Ruby string or interpolate (#{…}).
  if (!/^https?:\/\/[A-Za-z0-9.:-]+\/[A-Za-z0-9._~%+\/-]+$/.test(url ?? '')) throw Error('Ungültige URL');
  return `cask "${token}" do
  version "${version}"
  sha256 "${sha256}"

  url "${url}"
  name "OpenSpider"
  desc "Website crawler and analysis desktop application"
  homepage "https://openspider.de/"

  livecheck do
    url :url
    regex(${tag})
    strategy :github_releases
  end

  auto_updates true
  conflicts_with cask: "${other}"
  depends_on macos: :ventura

  app "OpenSpider.app"
  generated_script "openspider-launchagents.rb", content: <<~'RUBY'
${readFileSync(new URL('./lifecycle.rb', import.meta.url), 'utf8').trimEnd().split('\n').map(line => (line ? `    ${line}` : '')).join('\n')}
  RUBY
  binary "#{appdir}/OpenSpider.app/Contents/MacOS/openspider"

  postflight_steps do
    run "/usr/bin/ruby",
        args:           ["{{staged_path}}/openspider-launchagents.rb", "restore", "{{temp}}/launchagents.json"],
        writable_paths: ["~/Library/LaunchAgents"]
  end

  uninstall_preflight_steps do
    run "/usr/bin/ruby",
        args:           ["{{staged_path}}/openspider-launchagents.rb", "save", "{{temp}}/launchagents.json"],
        writable_paths: ["~/Library/LaunchAgents"]
  end

  uninstall launchctl: "de.openspider.task.*",
            delete:    "~/Library/LaunchAgents/de.openspider.task.*.plist"

  zap trash: [
${zapPaths.map(path => `    "${path}",`).join('\n')}
  ]
end
`;
}

export function generate({ directory, output, version, repository, packageId, mode = 'all', baseUrl }) {
  if (!/^\d+\.\d+\.\d+(?:-(?:beta|rc)\.\d+)?$/.test(version)) throw Error('Ungültige Version');
  if (!/^[\w.-]+\/[\w.-]+$/.test(repository)) throw Error('Ungültiges Repository');
  if (!['all', 'homebrew', 'winget'].includes(mode)) throw Error('Ungültiger Modus');
  // K3-08 remains open: no default package identifier may escape into publication.
  if (mode !== 'homebrew' && !/^[A-Za-z0-9]+(?:\.[A-Za-z0-9]+)+$/.test(packageId ?? '')) {
    throw Error('OPENSPIDER_PACKAGE_ID / --package-id fehlt (K3-08)');
  }
  const base = baseUrl ?? `https://github.com/${repository}/releases/download/v${version}`;
  const parsed = new URL(base);
  if (parsed.username || parsed.password || parsed.search || parsed.hash ||
      !(parsed.protocol === 'https:' || (parsed.protocol === 'http:' && parsed.hostname === '127.0.0.1'))) {
    throw Error('Nur HTTPS oder Loopback für CI; keine Zugangsdaten in URLs');
  }
  const assets = readdirSync(directory).filter(n => lstatSync(join(directory, n)).isFile());
  const pick = pattern => {
    const found = assets.filter(n => pattern.test(n));
    if (found.length !== 1) throw Error(`Erwartet genau ein Asset für ${pattern}: ${found.length}`);
    const name = found[0];
    return { name, url: `${parsed.href.replace(/\/$/, '')}/${encodeURIComponent(name)}`,
      hash: createHash('sha256').update(readFileSync(join(directory, name))).digest('hex') };
  };
  const result = {};
  if (mode !== 'winget') {
    // OUT-261 (Nutzerentscheidung 04.11.2026): only the Universal DMG is published, for Apple Silicon
    // and Intel alike. R01 copies the Tauri DMG name unchanged; per-architecture DMGs (Actions artifacts
    // only) and the versionless copy OpenSpider-macOS.dmg are never selected.
    const dmg = pick(/^OpenSpider_[A-Za-z0-9.+-]+_universal\.dmg$/);
    result['openspider.rb'] = caskSource({ version, sha256: dmg.hash, url: dmg.url });
  }
  if (mode !== 'homebrew') {
    // The installer patterns match scripts/release/manifest.mjs, without guessing versions/locales.
    const msi = pick(/^OpenSpider_[A-Za-z0-9.+-]+_x64_[A-Za-z-]+\.msi$/);
    const nsis = pick(/^OpenSpider_[A-Za-z0-9.+-]+_x64-setup\.exe$/);
    const common = { PackageIdentifier: packageId, PackageVersion: version };
    const docs = {
      version: { ...common, DefaultLocale: 'en-US', ManifestType: 'version', ManifestVersion: schemaVersion },
      installer: { ...common, MinimumOSVersion: '10.0.17763.0', Installers: [
        { Architecture: 'x64', InstallerType: 'msi', Scope: 'machine', InstallerUrl: msi.url,
          InstallerSha256: msi.hash.toUpperCase(), InstallerSwitches: {
            Silent: '/qn /norestart', SilentWithProgress: '/qb /norestart', Custom: 'ALLUSERS=1 ADDTOPATH=1', InstallLocation: 'INSTALLDIR="<INSTALLPATH>"' } },
        { Architecture: 'x64', InstallerType: 'nullsoft', Scope: 'user', InstallerUrl: nsis.url,
          InstallerSha256: nsis.hash.toUpperCase(), InstallerSwitches: { Silent: '/S', SilentWithProgress: '/S', InstallLocation: '/D=<INSTALLPATH>' } },
      ], ManifestType: 'installer', ManifestVersion: schemaVersion },
      'locale.en-US': { ...common, PackageLocale: 'en-US', Publisher: 'OpenSpider',
        PackageName: 'OpenSpider', License: 'Apache-2.0', ShortDescription: 'Website crawler and analysis desktop application',
        PackageUrl: 'https://openspider.de/', ManifestType: 'defaultLocale', ManifestVersion: schemaVersion },
    };
    for (const [suffix, doc] of Object.entries(docs)) {
      const type = doc.ManifestType;
      // JSON scalar quoting is a subset of YAML; all documents remain ordinary YAML mappings.
      result[`${packageId}.${suffix === 'version' ? '' : `${suffix}.`}yaml`] =
        `# yaml-language-server: $schema=https://aka.ms/winget-manifest.${type}.${schemaVersion}.schema.json\n` + yaml(doc);
    }
  }
  mkdirSync(output, { recursive: true });
  // Fail closed on stale/mixed manifests; callers must use an empty output directory.
  if (readdirSync(output).length) throw Error('Ausgabeordner muss leer sein');
  for (const [name, content] of Object.entries(result)) writeFileSync(join(output, name), content);
  return result;
}

function yaml(value, indent = '') {
  return Object.entries(value).map(([key, item]) => {
    if (Array.isArray(item)) return `${indent}${key}:\n` + item.map(v => `${indent}  -\n${yaml(v, `${indent}    `)}`).join('');
    if (typeof item === 'object') return `${indent}${key}:\n${yaml(item, `${indent}  `)}`;
    return `${indent}${key}: ${JSON.stringify(item)}\n`;
  }).join('');
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const { values } = parseArgs({ options: Object.fromEntries(
    ['directory', 'output', 'version', 'repository', 'package-id', 'mode', 'base-url'].map(k => [k, { type: 'string' }])) });
  generate({ ...values, packageId: values['package-id'] ?? process.env.OPENSPIDER_PACKAGE_ID, baseUrl: values['base-url'] });
}
