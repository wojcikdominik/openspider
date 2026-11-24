// OpenSpider MCP launcher for the Claude plugin.
//
// Finds the OpenSpider command-line program that the OpenSpider installer put on this
// computer and runs `openspider mcp` (MCP over STDIO). It starts no shell, downloads
// nothing, writes no files and opens no network connection. stdout belongs to MCP;
// diagnostics go to stderr only.
import { spawn } from 'node:child_process';
import { accessSync, constants as fsConstants, statSync } from 'node:fs';
import { constants } from 'node:os';
import { delimiter, join, parse } from 'node:path';

const windows = process.platform === 'win32';
const executable = windows ? 'openspider.exe' : 'openspider';
const download = 'https://github.com/wojcikdominik/openspider/releases/latest';

function isExecutableFile(path) {
  try {
    if (!statSync(path).isFile()) return false;
    if (!windows) accessSync(path, fsConstants.X_OK);
    return true;
  } catch {
    return false;
  }
}

// Locations in search order: PATH first, then the default install locations of the
// official installers (macOS DMG and Homebrew cask, Windows per-user setup and MSI).
function candidates(env) {
  const list = [];
  for (const entry of (env.PATH ?? '').split(delimiter)) {
    const dir = entry.trim().replace(/^"(.*)"$/, '$1');
    if (dir) list.push(join(dir, executable));
  }
  const root = parse(process.cwd()).root;
  const appBinary = ['OpenSpider.app', 'Contents', 'MacOS', 'openspider'];
  if (process.platform === 'darwin') {
    list.push(join(root, 'Applications', ...appBinary));
    if (env.HOME) list.push(join(env.HOME, 'Applications', ...appBinary));
    list.push(join(root, 'opt', 'homebrew', 'bin', 'openspider'));
    list.push(join(root, 'usr', 'local', 'bin', 'openspider'));
  } else if (windows) {
    if (env.LOCALAPPDATA) list.push(join(env.LOCALAPPDATA, 'OpenSpider', executable));
    for (const name of ['ProgramFiles', 'ProgramW6432']) {
      if (env[name]) list.push(join(env[name], 'OpenSpider', executable));
    }
  }
  return [...new Set(list)];
}

function fail(lines) {
  for (const line of lines) console.error(line);
  process.exitCode = 1;
}

const override = process.env.OPENSPIDER_BINARY;
const searched = override ? [override] : candidates(process.env);
const binary = searched.find(isExecutableFile);

if (!binary) {
  fail([
    override
      ? `OpenSpider could not start: OPENSPIDER_BINARY does not point to an executable file (${override}).`
      : 'OpenSpider could not start: the OpenSpider program was not found on this computer.',
    `Install OpenSpider 1.0 or later from ${download} (macOS DMG or Homebrew cask, Windows setup or MSI) and restart Claude.`,
    'Already installed elsewhere? Set OPENSPIDER_BINARY to the absolute path of the openspider executable',
    '(on macOS inside OpenSpider.app/Contents/MacOS, on Windows openspider.exe in the install folder).',
    'Building from source instead: in an OpenSpider checkout run cargo install --path crates/os-cli --locked',
    'and put Cargo\'s bin directory on PATH.',
    `Searched: ${searched.join(', ') || '(empty PATH)'}`,
  ]);
} else {
  const child = spawn(binary, ['mcp'], { stdio: 'inherit', shell: false });
  child.on('error', (error) => {
    fail([`OpenSpider could not start ${binary} (${error.code}). Check the installation; see the plugin README.`]);
  });
  for (const signal of ['SIGINT', 'SIGTERM']) {
    process.on(signal, () => child.kill(signal));
  }
  child.on('exit', (code, signal) => {
    process.exitCode = code ?? 128 + (constants.signals[signal] ?? 1);
  });
}
