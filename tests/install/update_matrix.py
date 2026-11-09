#!/usr/bin/env python3
"""Installations- und Update-Matrix (M3-Q02): Version N installieren, auf N+1 aktualisieren.

Läuft in ``.github/workflows/install-test.yml`` auf einem frischen Runner. Verändert das echte
Benutzerprofil (Standardorte, ``/Applications``, Registry) und startet deshalb nur mit ``CI=true``
oder ``--allow-profile-changes``. N+1 kommt ausschließlich vom lokalen Mock-Update-Server
(``mockupdate.py``); die Einstellung ``network.proxy`` lenkt die App dorthin, ``trusted_certificates``
vertraut der laufzeiterzeugten Test-CA. Kein Request erreicht GitHub.

Ablauf und Prüf-IDs: ``docs/plan/M3-acceptance/install/README.md``.
"""
import argparse
import datetime as dt
import hashlib
import json
import os
import platform
import plistlib
import re
import shutil
import signal
import socket
import subprocess
import sys
import tempfile
import time
import tomllib
import traceback
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
sys.path.insert(0, str(HERE))
sys.path.insert(0, str(ROOT / 'scripts' / 'smoke'))
import cdp  # noqa: E402
import mockupdate  # noqa: E402
import tauri_sig  # noqa: E402
from installed import normalize  # noqa: E402  (Spec 06 §0, wie der Installations-Smoke)

SYSTEM = platform.system()
PROJECT = 'Q02 Update'
CHECKLIST = 'docs/release/abnahme-1.0-nutzer.md'


# --- Protokoll ----------------------------------------------------------------------------------
class Report:
    def __init__(self, output, meta):
        self.output = output
        self.meta = meta
        self.checks = []

    def check(self, ident, title, spec, fn, on_error='failed', hint=''):
        """``on_error``: Status bei einer Ausnahme; ``unbelegt`` für erwartbar unerreichbare Wege (kein Fehlschlag)."""
        started = time.monotonic()
        try:
            detail = fn()
            status = 'passed'
        except Exception as error:  # noqa: BLE001 – jeder Fehler wird protokolliert, der Lauf geht weiter
            detail = f'{type(error).__name__}: {error}' + (f' – {hint}' if hint else '')
            status = on_error
            with (self.output / 'errors.log').open('a', encoding='utf-8') as log:
                log.write(f'## {ident}\n{traceback.format_exc()}\n')
        self.checks.append(dict(id=ident, title=title, spec=spec, status=status, detail=_short(detail),
                                seconds=round(time.monotonic() - started, 1)))
        print(f'[{status}] {ident} {title}: {_short(detail)}', flush=True)
        self.write()
        return status == 'passed'

    def note(self, ident, title, spec, status, detail):
        self.checks.append(dict(id=ident, title=title, spec=spec, status=status, detail=detail, seconds=0))
        print(f'[{status}] {ident} {title}: {detail}', flush=True)
        self.write()

    @property
    def failed(self):
        return [c for c in self.checks if c['status'] == 'failed']

    def write(self):
        data = dict(self.meta, checks=self.checks, finished=dt.datetime.now(dt.timezone.utc).isoformat())
        (self.output / 'report.json').write_text(json.dumps(data, indent=2, ensure_ascii=False), encoding='utf-8')
        lines = [f'# Update-Matrix {self.meta["label"]}', '',
                 f'N = `{self.meta["current"]}`, N+1 = `{self.meta["next"]}`, Plattform `{self.meta["platform"]}`, '
                 f'Updater-Ziel `{self.meta["target"]}`', '',
                 '| ID | Prüfung | Spec | Ergebnis | Detail |', '|---|---|---|---|---|']
        for c in self.checks:
            detail = str(c['detail']).replace('|', '\\|').replace('\n', ' ')
            lines.append(f'| {c["id"]} | {c["title"]} | {c["spec"]} | {c["status"]} | {detail} |')
        (self.output / 'report.md').write_text('\n'.join(lines) + '\n', encoding='utf-8')


def _short(value, limit=600):
    text = value if isinstance(value, str) else json.dumps(value, ensure_ascii=False, default=str)
    return text if len(text) <= limit else text[:limit] + ' …'


# --- Hilfsfunktionen ----------------------------------------------------------------------------
class Context:
    def __init__(self, output):
        self.output = output
        self.commands = (output / 'commands.log').open('a', encoding='utf-8')

    def run(self, args, timeout=600, check=True, env=None):
        args = [str(a) for a in args]
        started = time.monotonic()
        result = subprocess.run(args, text=True, capture_output=True, timeout=timeout, env=env,
                                encoding='utf-8', errors='replace')
        self.commands.write(f'$ {" ".join(args)}\n# exit {result.returncode} '
                            f'({time.monotonic() - started:.1f}s)\n{result.stdout}{result.stderr}\n')
        self.commands.flush()
        if check and result.returncode:
            raise RuntimeError(f'{Path(args[0]).name}: Exit {result.returncode}: {(result.stderr or result.stdout)[-400:]}')
        return result

    def powershell(self, script, timeout=300, check=True):
        return self.run(['powershell.exe', '-NoProfile', '-NonInteractive', '-Command', script],
                        timeout=timeout, check=check)


def until(fn, label, timeout, interval=1.0):
    deadline = time.monotonic() + timeout
    last = None
    while time.monotonic() < deadline:
        try:
            value = fn()
            if value:
                return value
        except Exception as error:  # noqa: BLE001 – bis zur Frist wiederholen
            last = error
        time.sleep(interval)
    raise TimeoutError(f'{label}: keine Bedingung nach {timeout}s ({last})')


def sha256(path):
    digest = hashlib.sha256()
    with Path(path).open('rb') as source:
        while chunk := source.read(1 << 20):
            digest.update(chunk)
    return digest.hexdigest()


def free_port():
    with socket.socket() as sock:
        sock.bind(('127.0.0.1', 0))
        return sock.getsockname()[1]


def ps_quote(value):
    return "'" + str(value).replace("'", "''") + "'"


def config_dir():
    if SYSTEM == 'Darwin':
        return Path.home() / 'Library' / 'Application Support' / 'OpenSpider'
    return Path(os.environ['APPDATA']) / 'OpenSpider'


def logs_dir():
    if SYSTEM == 'Darwin':
        return Path.home() / 'Library' / 'Logs' / 'OpenSpider'
    return Path(os.environ['LOCALAPPDATA']) / 'OpenSpider' / 'logs'


def updater_target():
    """Plattformschlüssel, den der Update-Kern anfragt (``UpdateContext::target``)."""
    if SYSTEM == 'Darwin':
        arm = subprocess.run(['sysctl', '-n', 'hw.optional.arm64'], capture_output=True, text=True).stdout.strip()
        return 'darwin-aarch64' if arm == '1' else 'darwin-x86_64'
    # Der Kern setzt unter Windows immer `nsis` (policy.rs), auch bei einer MSI-Installation.
    return 'windows-x86_64-nsis'


def clean_env():
    env = {k: v for k, v in os.environ.items()
           if k not in ('OPENSPIDER_DISABLE_UPDATE_CHECK', 'OPENSPIDER_HOME', 'WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS')}
    env['OPENSPIDER_SECRET_STORE'] = 'memory'
    return env


# --- Installation -------------------------------------------------------------------------------
class Installed:
    def __init__(self, root, gui, cli):
        self.root, self.gui, self.cli = Path(root), Path(gui), Path(cli)

    def describe(self):
        return dict(root=str(self.root), gui=str(self.gui), cli=str(self.cli))


def macos_app():
    return Path('/Applications/OpenSpider.app')


def discover_macos(app):
    info = plistlib.loads((app / 'Contents' / 'Info.plist').read_bytes())
    gui = app / 'Contents' / 'MacOS' / info['CFBundleExecutable']
    clis = [p for p in (app / 'Contents' / 'MacOS').iterdir()
            if re.fullmatch(r'openspider(?:-(?:aarch64|x86_64|universal)-apple-darwin)?', p.name) and p != gui]
    if len(clis) != 1:
        raise AssertionError(f'Erwartet ein CLI im Bundle, gefunden {clis}')
    return Installed(app, gui, clis[0])


def discover_windows(folder):
    folder = Path(folder)
    gui = [p for p in folder.glob('*.exe') if p.name.lower() != 'openspider.exe' and 'uninstall' not in p.name.lower()]
    if len(gui) != 1 or not (folder / 'openspider.exe').is_file():
        raise AssertionError(f'Installation in {folder} unvollständig: {[p.name for p in folder.glob("*.exe")]}')
    return Installed(folder, gui[0], folder / 'openspider.exe')


def windows_entries(ctx):
    script = (r"$k=@('HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*',"
              r"'HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*',"
              r"'HKLM:\Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\*');"
              r"@(Get-ItemProperty $k -ErrorAction SilentlyContinue | Where-Object { $_.DisplayName -like 'OpenSpider*' } |"
              r" Select-Object DisplayName,DisplayVersion,InstallLocation,PSPath) | ConvertTo-Json -Depth 3")
    text = ctx.powershell(script).stdout.strip() or '[]'
    value = json.loads(text)
    return value if isinstance(value, list) else [value]


def windows_candidates(ctx, kind):
    local = Path(os.environ['LOCALAPPDATA'])
    program_files = Path(os.environ.get('ProgramFiles', r'C:\Program Files'))
    defaults = ([local / 'OpenSpider', local / 'Programs' / 'OpenSpider'] if kind == 'nsis'
                else [program_files / 'OpenSpider'])
    registry = [Path(e['InstallLocation'].strip('"')) for e in windows_entries(ctx) if e.get('InstallLocation')]
    return [p for p in dict.fromkeys(defaults + registry)]


def find_asset(folder, pattern):
    matches = [p for p in Path(folder).iterdir() if re.fullmatch(pattern, p.name)]
    if len(matches) != 1:
        raise AssertionError(f'Erwartet genau eine Datei {pattern} in {folder}, gefunden {[p.name for p in matches]}')
    return matches[0]


def install_current(ctx, kind, assets, version):
    if kind == 'dmg':
        dmg = find_asset(assets, re.escape(f'OpenSpider_{version}_universal.dmg'))
        mount = Path(tempfile.mkdtemp(prefix='q02-dmg-'))
        ctx.run(['hdiutil', 'attach', dmg, '-nobrowse', '-readonly', '-mountpoint', mount])
        try:
            apps = list(mount.glob('*.app'))
            if len(apps) != 1:
                raise AssertionError(f'DMG enthält {apps}')
            target = macos_app()
            if target.exists():
                shutil.rmtree(target)
            ctx.run(['ditto', apps[0], target])
        finally:
            ctx.run(['hdiutil', 'detach', mount], check=False)
        return discover_macos(macos_app())
    if kind == 'nsis':
        setup = find_asset(assets, re.escape(f'OpenSpider_{version}_x64-setup.exe'))
        ctx.run([setup, '/S'], timeout=900)
    else:
        msi = find_asset(assets, re.escape('OpenSpider_') + r'.*_x64_[A-Za-z-]+\.msi')
        result = ctx.run(['msiexec.exe', '/i', msi, '/qn', '/norestart', '/l*v', ctx.output / 'msi-install.log'],
                         timeout=900, check=False)
        if result.returncode not in (0, 3010):
            raise RuntimeError(f'msiexec Exit {result.returncode}')
    for folder in windows_candidates(ctx, kind):
        if (folder / 'openspider.exe').is_file():
            return discover_windows(folder)
    raise AssertionError(f'Installationsordner nicht gefunden; Registry: {windows_entries(ctx)}')


def cli_version(ctx, installed, env):
    return ctx.run([installed.cli, '--version'], env=env).stdout.strip()


def gui_version(ctx, installed):
    if SYSTEM == 'Darwin':
        return plistlib.loads((installed.root / 'Contents' / 'Info.plist').read_bytes())['CFBundleShortVersionString']
    return ctx.powershell(f'(Get-Item -LiteralPath {ps_quote(installed.gui)}).VersionInfo.ProductVersion').stdout.strip()


def os_signature(ctx, installed):
    if SYSTEM == 'Darwin':
        ctx.run(['codesign', '--verify', '--deep', '--strict', '--verbose=2', installed.root])
        spctl = ctx.run(['spctl', '-a', '-t', 'exec', '-vv', installed.root], check=False)
        text = spctl.stdout + spctl.stderr
        if spctl.returncode or 'accepted' not in text:
            raise AssertionError(f'spctl: {text.strip()}')
        ctx.run(['xcrun', 'stapler', 'validate', installed.root])
        return text.strip().splitlines()[-1]
    states = {}
    for path in (installed.gui, installed.cli):
        states[path.name] = ctx.powershell(f'(Get-AuthenticodeSignature -LiteralPath {ps_quote(path)}).Status').stdout.strip()
    if any(v != 'Valid' for v in states.values()):
        raise AssertionError(f'Authenticode: {states}')
    return states


# --- App starten und beenden --------------------------------------------------------------------
class App:
    def __init__(self, ctx, installed, env, name):
        self.ctx = ctx
        self.out = (ctx.output / name).open('w', encoding='utf-8')
        self.proc = subprocess.Popen([str(installed.gui)], env=env, stdout=self.out, stderr=subprocess.STDOUT,
                                     cwd=str(ctx.output))

    def alive(self):
        return self.proc.poll() is None

    def stop(self, grace=20):
        try:
            stop_pid(self.proc.pid, grace, self.proc)
        finally:
            self.out.close()


def pid_alive(pid):
    if SYSTEM == 'Windows':
        result = subprocess.run(['tasklist', '/FI', f'PID eq {pid}', '/NH'], capture_output=True, text=True)
        return str(pid) in result.stdout
    try:
        os.kill(pid, 0)
        return True
    except OSError:
        return False


def stop_pid(pid, grace=20, proc=None):
    alive = (lambda: proc.poll() is None) if proc else (lambda: pid_alive(pid))
    if not alive():
        return
    if SYSTEM == 'Windows':
        subprocess.run(['taskkill', '/PID', str(pid), '/T'], capture_output=True)
    else:
        os.kill(pid, signal.SIGTERM)
    deadline = time.monotonic() + grace
    while alive() and time.monotonic() < deadline:
        time.sleep(0.5)
    if alive():
        if SYSTEM == 'Windows':
            subprocess.run(['taskkill', '/PID', str(pid), '/T', '/F'], capture_output=True)
        else:
            os.kill(pid, signal.SIGKILL)
    if proc:
        proc.wait(timeout=30)


def processes_in(ctx, folder):
    """(PID, Pfad, Startzeit) aller Prozesse, deren Programm in ``folder`` liegt."""
    if SYSTEM == 'Windows':
        script = (f"$d={ps_quote(str(folder).rstrip(os.sep) + os.sep)};"
                  "@(Get-CimInstance Win32_Process | Where-Object { $_.ExecutablePath -and "
                  "$_.ExecutablePath.StartsWith($d, [System.StringComparison]::OrdinalIgnoreCase) } | "
                  "Select-Object ProcessId,ExecutablePath,@{n='Created';e={$_.CreationDate.ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ss')}})"
                  " | ConvertTo-Json")
        text = ctx.powershell(script).stdout.strip() or '[]'
        value = json.loads(text)
        return [(v['ProcessId'], v['ExecutablePath'], v['Created']) for v in (value if isinstance(value, list) else [value])]
    out = subprocess.run(['ps', '-A', '-o', 'pid=,lstart=,command='], capture_output=True, text=True).stdout
    prefix = str(folder).rstrip('/') + '/'
    result = []
    for line in out.splitlines():
        match = re.match(r'\s*(\d+)\s+(\w+\s+\w+\s+\d+\s+[\d:]+\s+\d+)\s+(.*)$', line)
        if match and match.group(3).startswith(prefix):
            result.append((int(match.group(1)), match.group(3), match.group(2)))
    return result


# --- Mock-Abfragen ------------------------------------------------------------------------------
def manifest_requests(entries, info):
    return [e for e in entries if e['kind'] == 'https' and e['path'] in info['endpoints']]


def artifact_requests(entries, info):
    paths = {a['request_path'] for a in info['artifacts'].values()}
    return [e for e in entries if e['kind'] == 'https' and e['path'] in paths]


def plain_or_blocked(entries):
    return [e for e in entries if e['kind'] in ('http', 'connect-blocked', 'tls-error')]


def settings_toml(port, ca):
    """App-Einstellungen des Testprofils (OUT-222): Proxy auf den Mock, Test-CA, lokale Ziele direkt."""
    # JSON-Zeichenketten sind gültige TOML-Basic-Strings (auch Windows-Pfade mit Backslash).
    return ('[updates]\ncheck = true\n\n'
            f'[network]\ntrusted_certificates = [{json.dumps(str(ca))}]\n\n'
            '[network.proxy]\nmode = "manual"\nkind = "http"\nhost = "127.0.0.1"\n'
            f'port = {port}\nbypass = ["127.0.0.1", "localhost"]\n')


def assert_privacy(request, version):
    """OUT-250: nur Version, Betriebssystem und Architektur; fester User-Agent."""
    if set(request['query']) != {'version', 'os', 'arch'}:
        raise AssertionError(f'Unerwartete Parameter: {request["query"]}')
    if request['query']['version'] != [version]:
        raise AssertionError(f'Version im Request {request["query"]["version"]} statt {version}')
    if request['query']['os'][0] not in ('darwin', 'windows', 'linux') or request['query']['arch'][0] not in ('aarch64', 'x86_64'):
        raise AssertionError(f'OS/Arch: {request["query"]}')
    if request['user_agent'] != mockupdate.USER_AGENT:
        raise AssertionError(f'User-Agent {request["user_agent"]!r}')
    return {k: v[0] for k, v in request['query'].items()}


# --- Ablauf -------------------------------------------------------------------------------------
def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--installer', choices=('dmg', 'nsis', 'msi'), required=True)
    parser.add_argument('--current', type=Path, required=True, help='release-assets von N')
    parser.add_argument('--next', type=Path, required=True, help='release-assets von N+1')
    parser.add_argument('--certs', type=Path, help='ca.pem, server.pem, server.key (Standard: im Job neu erzeugen)')
    parser.add_argument('--testsite', type=Path, required=True, help='os-testsite-Binary')
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--label', default='')
    parser.add_argument('--pubkey-file', type=Path, help='Erwarteter Updater-Schlüssel (Variable OPENSPIDER_UPDATER_PUBKEY)')
    parser.add_argument('--allow-profile-changes', action='store_true')
    args = parser.parse_args(argv)
    if os.environ.get('CI') != 'true' and not args.allow_profile_changes:
        parser.error('Ändert Benutzerprofil, /Applications und Registry: nur mit CI=true oder --allow-profile-changes')
    if (args.installer == 'dmg') != (SYSTEM == 'Darwin'):
        parser.error(f'Installer {args.installer} passt nicht zu {SYSTEM}')
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    ctx = Context(output)
    info = mockupdate.prepare(args.current, args.next, output / 'mock')
    target = updater_target()
    vn, vn1 = info['current'], info['next']
    report = Report(output, dict(label=args.label or f'{SYSTEM} {args.installer}', current=vn, next=vn1,
                                 platform=platform.platform(), machine=platform.machine(), target=target,
                                 installer=args.installer, repository=info['repository'],
                                 runner=os.environ.get('RUNNER_NAME'), run=os.environ.get('GITHUB_RUN_ID')))
    env = clean_env()
    state = {}
    if args.certs is None:
        # Test-CA nur in diesem Job, außerhalb des Ergebnisordners (der als Artefakt hochgeladen wird).
        args.certs = Path(tempfile.mkdtemp(prefix='q02-certs-'))
        mockupdate.make_certs(args.certs)
    with mockupdate.MockUpdateServer(info, args.certs, log_file=output / 'mock-log.jsonl') as mock:
        run_matrix(ctx, report, mock, info, args, env, target, state)
    for name, source in (('app-logs', logs_dir()), ('settings', config_dir())):
        if source.is_dir():
            shutil.copytree(source, output / name, dirs_exist_ok=True,
                            ignore=shutil.ignore_patterns('*.ospider', '*.ospider-*'))
    report.write()
    print(f'\n{len(report.failed)} von {len(report.checks)} Prüfungen fehlgeschlagen; Protokoll {output / "report.md"}')
    return 1 if report.failed else 0


def run_matrix(ctx, report, mock, info, args, env, target, state):
    vn, vn1 = info['current'], info['next']
    artifact = info['artifacts'][target]
    proxy = f'127.0.0.1:{mock.port}'
    ca = (args.certs / 'ca.pem').resolve()
    settings_path = config_dir() / 'settings.toml'

    # Q-01 Installation von N
    def install():
        state['n'] = install_current(ctx, args.installer, args.current, vn)
        version = cli_version(ctx, state['n'], env)
        if vn not in version:
            raise AssertionError(f'CLI meldet {version!r}, erwartet {vn}')
        state['gui_hash'] = sha256(state['n'].gui)
        return dict(state['n'].describe(), cli_version=version, gui_version=gui_version(ctx, state['n']))
    if not report.check('Q-01', f'N installieren ({args.installer})', 'OUT-262, OUT-265, OUT-266', install):
        return
    installed = state['n']
    report.check('Q-02', 'OS-Signatur von N gültig', 'OUT-265, OUT-266', lambda: os_signature(ctx, installed))

    # Q-03 Schlüssel in N, Signatur des N+1-Artefakts (OUT-252, ohne App)
    def keys():
        gui_keys = tauri_sig.embedded_public_keys(installed.gui.read_bytes())
        cli_keys = tauri_sig.embedded_public_keys(installed.cli.read_bytes())
        if len(gui_keys) != 1 or cli_keys != gui_keys:
            raise AssertionError(f'Eingebettete Schlüssel App {len(gui_keys)}, CLI {len(cli_keys)}, gleich={cli_keys == gui_keys}')
        state['pubkey'] = gui_keys[0]
        if args.pubkey_file and args.pubkey_file.is_file() and args.pubkey_file.read_text().strip():
            if args.pubkey_file.read_text().strip() != state['pubkey']:
                raise AssertionError('Eingebetteter Schlüssel weicht von OPENSPIDER_UPDATER_PUBKEY ab')
        key_id = tauri_sig.public_key(state['pubkey'])[0][::-1].hex().upper()
        payload = Path(artifact['path']).read_bytes()
        comment = tauri_sig.verify(payload, Path(artifact['path'] + '.sig').read_text(), state['pubkey'], vn1)
        flipped = bytearray(payload)
        flipped[artifact['flip']] ^= 0x01
        try:
            tauri_sig.verify(bytes(flipped), Path(artifact['path'] + '.sig').read_text(), state['pubkey'], vn1)
        except tauri_sig.SignatureError:
            pass
        else:
            raise AssertionError('Manipuliertes Artefakt wurde akzeptiert')
        return dict(key_id=key_id, trusted_comment=comment, artifact=Path(artifact['path']).name)
    report.check('Q-03', 'N+1-Artefakt mit dem in N eingebetteten Schlüssel gültig, 1 Byte geändert ungültig',
                 'OUT-252', keys)

    # Testprofil: Proxy auf den Mock, Test-CA vertrauen, lokale Crawl-Ziele direkt.
    settings_path.parent.mkdir(parents=True, exist_ok=True)
    if settings_path.exists():
        shutil.copyfile(settings_path, ctx.output / 'settings.before-q02.toml')
    settings_text = settings_toml(mock.port, ca)
    settings_path.write_text(settings_text, encoding='utf-8')
    written = tomllib.loads(settings_text)

    # Q-04 Daten mit N anlegen
    fixture = Path(tempfile.mkdtemp(prefix='q02-site-')) / 'out-basic-site'
    shutil.copytree(ROOT / 'tests' / 'fixtures' / 'out-basic-site', fixture)
    site_log = ctx.output / 'testsite.log'
    site = subprocess.Popen([str(args.testsite.resolve()), str(fixture), '8900'], cwd=str(ROOT),
                            stdout=site_log.open('w', encoding='utf-8'), stderr=subprocess.STDOUT)
    try:
        def seed():
            base = until(lambda: (m := re.search(r'-> (http://127\.0\.0\.1:\d+)', site_log.read_text(encoding='utf-8')))
                         and m.group(1), 'Fixture-Server', 30, 0.2)
            state['base'] = base
            before = ctx.output / 'before'
            ctx.run([installed.cli, '--crawl', base + '/', '--project-name', PROJECT, '--set', 'threads=1',
                     '--set', 'robots.retries=0', '--export-tabs', 'Internal:All', '--output-folder', before], env=env)
            state['before'] = normalize(before / 'internal_all.csv', base)
            rows = json.loads(ctx.run([installed.cli, '--list-crawls', '--json', '--project-name', PROJECT],
                                      env=env).stdout)
            if len(rows) != 1:
                raise AssertionError(f'Crawl-Verwaltung: {rows}')
            state['crawl'] = rows[0]
            return dict(crawl=rows[0].get('id'), urls=rows[0].get('urls'), status=rows[0].get('status'))
        report.check('Q-04', 'Mit N einen Crawl in der Crawl-Verwaltung anlegen und exportieren', 'OUT-255', seed)
    finally:
        site.terminate()
        site.wait(timeout=30)

    # Q-05 … Q-09 CLI von N gegen den Mock
    def cli_check(scenario, extra_env=None):
        mock.use(scenario)
        mark = mock.mark()
        result = ctx.run([installed.cli, '--check-update'], env=dict(env, **(extra_env or {})), check=False, timeout=120)
        time.sleep(0.5)
        return result, mock.log(mark)

    def disabled():
        result, log = cli_check('valid', {'OPENSPIDER_DISABLE_UPDATE_CHECK': '1'})
        if result.returncode in (0, 10) or log:
            raise AssertionError(f'Exit {result.returncode}, {len(log)} Requests')
        return dict(exit=result.returncode, requests=0)
    report.check('Q-05', 'CLI: Prüfung per Umgebungsvariable aus → kein Request', 'OUT-250, OUT-253', disabled)

    def available():
        result, log = cli_check('valid')
        manifests = manifest_requests(log, info)
        if result.returncode != 10 or vn1 not in result.stdout:
            raise AssertionError(f'Exit {result.returncode}: {result.stdout.strip()} {result.stderr.strip()}')
        if len(manifests) != 1 or artifact_requests(log, info) or plain_or_blocked(log):
            raise AssertionError(f'Requests: {log}')
        return dict(exit=10, stdout=result.stdout.strip(), request=assert_privacy(manifests[0], vn),
                    endpoint=manifests[0]['path'])
    report.check('Q-06', 'CLI: N+1 verfügbar (Exit 10), Request nur mit Version/OS/Arch', 'OUT-250', available)

    def rejected(scenario):
        def check():
            result, log = cli_check(scenario)
            if result.returncode in (0, 10) or artifact_requests(log, info) or plain_or_blocked(log):
                raise AssertionError(f'Exit {result.returncode}, Requests {log}')
            return dict(exit=result.returncode, stderr=result.stderr.strip()[-200:], manifest_requests=len(log))
        return check
    report.check('Q-07', 'CLI: Artefakt-URL über HTTP abgelehnt', 'OUT-252', rejected('http-artifact'))
    report.check('Q-08', 'CLI: Weiterleitung des Manifests auf HTTP abgelehnt', 'OUT-252', rejected('http-redirect'))

    def same():
        result, log = cli_check('downgrade')
        if result.returncode != 0 or artifact_requests(log, info):
            raise AssertionError(f'Exit {result.returncode}: {result.stdout.strip()}')
        return dict(exit=0, stdout=result.stdout.strip())
    report.check('Q-09', 'CLI: Manifest mit eigener Version → aktuell (Exit 0)', 'OUT-252', same)

    # Q-10 … Q-12 Startprüfung der installierten App
    mock.use('valid')

    def app_session(name, extra_env=None, wait=20):
        mark = mock.mark()
        app = App(ctx, installed, dict(env, **(extra_env or {})), name)
        try:
            time.sleep(wait)
            if not app.alive():
                raise AssertionError(f'App beendet mit {app.proc.returncode}')
            return mock.log(mark)
        finally:
            app.stop()

    def app_disabled():
        log = app_session('app-disabled.log', {'OPENSPIDER_DISABLE_UPDATE_CHECK': '1'})
        if log:
            raise AssertionError(f'{len(log)} Requests trotz Abschaltung')
        return dict(requests=0, seconds=20)
    report.check('Q-10', 'App-Start mit abgeschalteter Prüfung → kein Request', 'OUT-250, OUT-253', app_disabled)

    def app_startup():
        mark = mock.mark()
        app = App(ctx, installed, env, 'app-startup.log')
        try:
            request = until(lambda: manifest_requests(mock.log(mark), info), 'Startprüfung der App', 90)[0]
            time.sleep(3)
        finally:
            app.stop()
        stored = tomllib.loads(settings_path.read_text(encoding='utf-8'))
        if not stored.get('updates', {}).get('last_check'):
            raise AssertionError('updates.last_check nicht gespeichert')
        if artifact_requests(mock.log(mark), info):
            raise AssertionError('Download ohne Zustimmung gestartet')
        return dict(request=assert_privacy(request, vn), last_check=stored['updates']['last_check'])
    report.check('Q-11', 'App-Start: Prüfung über Proxy und Test-CA, nur Version/OS/Arch, kein Download',
                 'OUT-250, OUT-269, OUT-270', app_startup)

    def app_throttled():
        log = app_session('app-throttled.log')
        if manifest_requests(log, info):
            raise AssertionError('Zweite Startprüfung innerhalb von 24 h')
        return dict(requests=len(log))
    report.check('Q-12', 'Zweiter App-Start innerhalb von 24 h → keine Prüfung', 'OUT-250', app_throttled)

    # Q-13 … Q-17 Update in der App
    if SYSTEM == 'Darwin' or not update_windows(ctx, report, mock, info, installed, env, state):
        update_replica(ctx, report, mock, info, installed, env, state, proxy, ca, args.installer)

    # Q-18 … Q-23 Nach dem Update (am ursprünglichen Ort; ein zweiter Installationsort wäre ein Befund)
    try:
        after = discover_macos(macos_app()) if SYSTEM == 'Darwin' else discover_windows(installed.root)
    except (AssertionError, OSError, KeyError) as error:
        report.note('Q-18', 'Installation am ursprünglichen Ort nach dem Update', 'OUT-251', 'failed', str(error))
        return

    def versions():
        cli = cli_version(ctx, after, env)
        gui = gui_version(ctx, after)
        if vn1 not in cli:
            raise AssertionError(f'CLI am Installationsort meldet {cli!r} statt {vn1}')
        if SYSTEM == 'Darwin' and gui != vn1:
            raise AssertionError(f'CFBundleShortVersionString {gui!r} statt {vn1}')
        detail = dict(cli=cli, gui=gui, root=str(after.root))
        if SYSTEM == 'Windows':
            detail['registry'] = windows_entries(ctx)
        return detail
    report.check('Q-18', 'Version N+1 am ursprünglichen Installationsort, CLI mit aktualisiert', 'OUT-251', versions)
    report.check('Q-19', 'OS-Signatur nach dem Update gültig', 'OUT-252, OUT-265, OUT-266',
                 lambda: os_signature(ctx, after))

    def data():
        rows = json.loads(ctx.run([after.cli, '--list-crawls', '--json', '--project-name', PROJECT], env=env).stdout)
        if [r.get('id') for r in rows] != [state['crawl'].get('id')]:
            raise AssertionError(f'Crawl-Verwaltung nach dem Update: {rows}')
        out = ctx.output / 'after'
        ctx.run([after.cli, '--load-crawl', state['crawl']['id'], '--export-tabs', 'Internal:All',
                 '--output-folder', out], env=env)
        exported = normalize(out / 'internal_all.csv', state['base'])
        if exported != state['before']:
            (ctx.output / 'before.normalized.csv').write_bytes(state['before'])
            (ctx.output / 'after.normalized.csv').write_bytes(exported)
            raise AssertionError('Export nach dem Update weicht ab (siehe *.normalized.csv)')
        return dict(crawl=rows[0].get('id'), rows=exported.count(b'\n') - 1)
    report.check('Q-20', 'Crawl von N mit N+1 geöffnet, Export bytegleich (normalisiert)', 'OUT-255', data)

    def settings_kept():
        stored = tomllib.loads(settings_path.read_text(encoding='utf-8'))
        if stored.get('network') != written['network'] or stored.get('updates', {}).get('check') is not True:
            raise AssertionError(f'settings.toml verändert: network={stored.get("network")}')
        return dict(keys=sorted(stored))
    report.check('Q-21', 'Einstellungen (Proxy, Zertifikate, Update-Schalter) erhalten', 'OUT-255', settings_kept)

    def downgrade():
        mock.use('downgrade')
        mark = mock.mark()
        result = ctx.run([after.cli, '--check-update'], env=env, check=False, timeout=120)
        log = mock.log(mark)
        if result.returncode != 0 or artifact_requests(log, info):
            raise AssertionError(f'Exit {result.returncode}: {result.stdout.strip()} {result.stderr.strip()}')
        mock.use('valid')
        result2 = ctx.run([after.cli, '--check-update'], env=env, check=False, timeout=120)
        if result2.returncode != 0:
            raise AssertionError(f'Gleiche Version: Exit {result2.returncode}')
        return dict(downgrade=result.stdout.strip(), same=result2.stdout.strip())
    report.check('Q-22', f'N+1 lehnt Downgrade auf {vn} ab (Exit 0, kein Download)', 'OUT-252, OUT-254', downgrade)

    def no_leak():
        entries = mock.log()
        leaks = plain_or_blocked(entries)
        hosts = sorted({e.get('host') for e in entries if e['kind'] == 'https'})
        if leaks:
            raise AssertionError(f'Klartext oder fremde Ziele: {leaks[:5]}')
        return dict(requests=len(entries), hosts=hosts)
    report.check('Q-23', 'Kein Klartext-Request und kein Ziel außerhalb des Mocks im gesamten Lauf', 'OUT-252', no_leak)

    if SYSTEM == 'Windows':
        report.check('Q-24', 'Deinstallation nach dem Update: Exit 0, Programm entfernt; Benutzerdaten bleiben (NSIS) '
                     'bzw. werden mit REMOVEUSERDATA=1 entfernt (MSI)', 'OUT-266',
                     lambda: uninstall_windows(ctx, args, info, after, settings_path))

    for ident, title, spec, ref in MANUAL[SYSTEM]:
        required = ' – Pflicht, weil Q-13 nicht erreichbar war' if state.get('w02_required') and 'W-02' in ref else ''
        report.note(ident, title, spec, 'manual', f'Prüfprotokoll {CHECKLIST}: {ref}{required}')


def uninstall_windows(ctx, args, info, after, settings_path):
    crawls = Path(os.environ['LOCALAPPDATA']) / 'OpenSpider' / 'crawls'
    for pid, _, _ in processes_in(ctx, after.root):
        stop_pid(pid)
    if args.installer == 'nsis':
        uninstallers = [p for p in after.root.glob('*.exe') if 'uninstall' in p.name.lower()]
        if len(uninstallers) != 1:
            raise AssertionError(f'Deinstallationsprogramm: {uninstallers}')
        ctx.run([uninstallers[0], '/S'], timeout=600)
        until(lambda: not after.cli.exists() and not after.gui.exists(), 'Programmdateien entfernt', 120)
        if not settings_path.is_file() or not crawls.is_dir():
            raise AssertionError('Benutzerdaten nach der Deinstallation entfernt (ohne Auswahl)')
        return dict(uninstaller=uninstallers[0].name, user_data='erhalten')
    codes = {}
    for folder in (args.next, args.current):  # nach einem MSI-Update ist N+1 installiert, sonst noch N
        msi = find_asset(folder, re.escape('OpenSpider_') + r'.*_x64_[A-Za-z-]+\.msi')
        result = ctx.run(['msiexec.exe', '/x', msi, '/qn', '/norestart', 'REMOVEUSERDATA=1', '/l*v',
                          ctx.output / f'msi-uninstall-{folder.name}.log'], timeout=900, check=False)
        codes[msi.name] = result.returncode
        if result.returncode in (0, 3010):
            break
    else:
        raise AssertionError(f'msiexec /x: {codes}')
    if after.cli.exists() or after.gui.exists():
        raise AssertionError('Programmdateien nach der Deinstallation vorhanden')
    if settings_path.exists() or crawls.exists():
        raise AssertionError('Benutzerdaten trotz REMOVEUSERDATA=1 vorhanden')
    return dict(msiexec=codes, user_data='entfernt')


def update_windows(ctx, report, mock, info, installed, env, state):
    """Update in der echten Release-App über die WebView2-DevTools. Gibt False zurück, wenn der Steuerweg nicht
    erreichbar ist (dann übernimmt die Nachbildung). Unbelegt: Laut M3-nightly-b („Ursache 2“) überschreibt wry die
    Variable ``WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS``; der Debug-Port kam dort nur über ``TAURI_CONFIG`` in einem
    Testbuild an, was bei der signierten Release-App nicht geht. Q-13 ist daher voraussichtlich ``unbelegt``."""
    vn, vn1 = info['current'], info['next']
    port = free_port()
    app = App(ctx, installed, dict(env, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=f'--remote-debugging-port={port}'),
              'app-update.log')
    conn = None

    def connect():
        nonlocal conn
        conn = cdp.connect(port, wait=120)
        until(lambda: conn.evaluate("typeof window.__TAURI_INTERNALS__?.invoke === 'function'"), 'Tauri-IPC', 60)
        return dict(devtools_port=port, status=conn.evaluate(cdp.invoke_expression('update.status')))
    if not report.check('Q-13', 'Release-App über WebView2-DevTools steuerbar (unbelegt, wahrscheinlich nicht erreichbar)',
                        'OUT-251', connect, on_error='unbelegt',
                        hint=f'kein Steuerweg; Klickweg {CHECKLIST}: W-02 ist Pflicht, weiter mit Nachbildung'):
        app.stop()
        state['w02_required'] = True
        return False

    def invoke(name, args=None, timeout=60):
        return conn.evaluate(cdp.invoke_expression(name, args), timeout=timeout)

    def wait_state(predicate, timeout=900):
        return until(lambda: (s := invoke('update.status')) and predicate(s) and s, 'Update-Zustand', timeout)

    def http_rejected():
        mock.use('http-artifact')
        mark = mock.mark()
        result = invoke('update.check')
        if not (isinstance(result, dict) and 'failed' in result):
            raise AssertionError(f'Zustand {result}')
        if artifact_requests(mock.log(mark), info):
            raise AssertionError('Artefakt trotz HTTP-URL geladen')
        return result
    report.check('Q-14', 'App: Manifest mit HTTP-Artefakt abgelehnt', 'OUT-252', http_rejected)

    def tampered():
        mock.use('tampered')
        mark = mock.mark()
        result = invoke('update.check')
        if not (isinstance(result, dict) and result.get('available', {}).get('version') == vn1):
            raise AssertionError(f'Prüfung: {result}')
        invoke('update.download')
        final = wait_state(lambda s: isinstance(s, dict) and ('failed' in s or 'ready' in s))
        downloads = artifact_requests(mock.log(mark), info)
        if 'failed' not in final:
            raise AssertionError(f'Manipuliertes Paket angenommen: {final}')
        if not downloads:
            raise AssertionError('Kein Download-Request; Ablehnung nicht durch die Signatur belegt')
        if sha256(installed.gui) != state['gui_hash'] or vn not in cli_version(ctx, installed, env):
            raise AssertionError('Installation verändert')
        return dict(state=final, downloaded=downloads[-1].get('bytes'))
    report.check('Q-15', 'App: 1 Byte geändertes Paket verworfen, Installation unverändert', 'OUT-252', tampered)

    def update():
        mock.use('valid')
        result = invoke('update.check')
        if not (isinstance(result, dict) and result.get('available', {}).get('version') == vn1):
            raise AssertionError(f'Prüfung: {result}')
        invoke('update.download')
        ready = wait_state(lambda s: isinstance(s, dict) and ('ready' in s or 'failed' in s))
        if 'ready' not in ready:
            raise AssertionError(f'Download: {ready}')
        triggered = time.time()
        try:
            installing = invoke('update.install', {'when': 'now'}, timeout=120)
        except (cdp.CdpError, OSError) as error:
            installing = f'Verbindung beendet ({type(error).__name__}), erwartet beim Installieren'
        state['install_triggered'] = triggered
        until(lambda: not app.alive(), 'Alte App beendet sich', 180)
        return dict(ready=ready, install=installing)
    ok = report.check('Q-16', 'App: N+1 prüfen, herunterladen (signiert), „Jetzt installieren und neu starten“',
                      'OUT-251, OUT-252', update)
    try:
        conn.close()
    except OSError:
        pass
    app.stop()
    if not ok:
        return True  # Steuerweg vorhanden, Update gescheitert: Befund, keine Nachbildung

    def restarted():
        version = until(lambda: vn1 in (cli_version(ctx, installed, env) if installed.cli.is_file() else ''),
                        'Installer schreibt N+1', 900, 5)
        procs = until(lambda: [p for p in processes_in(ctx, installed.root)
                               if dt.datetime.fromisoformat(p[2] + '+00:00').timestamp()
                               >= state['install_triggered'] - 5], 'Neustart der App', 300, 2)
        for pid, _, _ in procs:
            stop_pid(pid)
        return dict(cli=version, processes=[p[1] for p in procs])
    report.check('Q-17', 'Installer ersetzt N durch N+1, App startet neu', 'OUT-251', restarted)
    return True


def update_replica(ctx, report, mock, info, installed, env, state, proxy, ca, kind):
    """Nachbildung, wenn die Release-App nicht steuerbar ist (macOS immer, Windows ohne DevTools-Zugang).
    Geprüft wird alles außer dem Klick und dem Updater-Code selbst: Download wie die App (Proxy, Test-CA,
    User-Agent), Signatur mit dem in N eingebetteten Schlüssel, dann Einspielen am selben Ort – macOS wie der
    Tauri-Updater (entpacken, altes Bundle beiseite, neues an dieselbe Stelle), Windows mit dem signierten
    N+1-Installer still über die vorhandene Installation (``/S`` bzw. ``msiexec /i … /qn``; die Argumente des
    Updaters sind andere). Der echte Klickweg ist Handprüfung M-02 bzw. W-02."""
    vn1 = info['next']
    key = 'windows-x86_64-msi' if kind == 'msi' else updater_target()
    artifact = info['artifacts'][key]
    work = Path(tempfile.mkdtemp(prefix='q02-update-'))
    if SYSTEM == 'Darwin':
        report.note('Q-13', 'Release-App steuerbar', 'OUT-251', 'manual',
                    f'macOS-Release-Builds haben keinen WebDriver/DevTools-Zugang; Klickweg {CHECKLIST}: M-02')

    def tampered():
        mock.use('tampered')
        status, size = mockupdate.fetch(artifact['url'], proxy, str(ca), work / 'tampered.tar.gz')
        if status != 200:
            raise AssertionError(f'HTTP {status}')
        try:
            tauri_sig.verify((work / 'tampered.tar.gz').read_bytes(), Path(artifact['path'] + '.sig').read_text(),
                             state['pubkey'], vn1)
        except tauri_sig.SignatureError as error:
            return dict(rejected=str(error), bytes=size)
        raise AssertionError('Manipuliertes Paket als gültig erkannt')
    report.check('Q-14', 'Manipuliertes Paket über den Mock geladen und abgelehnt (Nachbildung)', 'OUT-252', tampered)
    report.note('Q-15', 'App: manipuliertes Paket in der App verworfen', 'OUT-252', 'unbelegt',
                'in der Release-App nicht automatisierbar; Updater-Prüfung abgedeckt durch '
                'update_security::tauri_signature_rejects_one_modified_byte_and_wrong_keys (Mock, kein installiertes Paket)')

    def replace():
        mock.use('valid')
        archive = work / Path(artifact['path']).name
        status, _ = mockupdate.fetch(artifact['url'], proxy, str(ca), archive)
        if status != 200:
            raise AssertionError(f'HTTP {status}')
        comment = tauri_sig.verify(archive.read_bytes(), Path(artifact['path'] + '.sig').read_text(),
                                   state['pubkey'], vn1)
        if SYSTEM == 'Windows':
            if kind == 'msi':
                result = ctx.run(['msiexec.exe', '/i', archive, '/qn', '/norestart', '/l*v',
                                  ctx.output / 'msi-update.log'], timeout=900, check=False)
                if result.returncode not in (0, 3010):
                    raise RuntimeError(f'msiexec Exit {result.returncode}')
            else:
                ctx.run([archive, '/S'], timeout=900)
            until(lambda: vn1 in cli_version(ctx, installed, env), 'Installer schreibt N+1', 300, 5)
            return dict(trusted_comment=comment, installer=archive.name, key=key)
        extracted = work / 'extracted'
        extracted.mkdir()
        ctx.run(['/usr/bin/tar', '-xzf', archive, '-C', extracted])
        apps = list(extracted.glob('*.app'))
        if len(apps) != 1 or apps[0].name != macos_app().name:
            raise AssertionError(f'Archiv enthält {apps}')
        backup = macos_app().with_name('.OpenSpider.q02-old.app')
        os.rename(macos_app(), backup)
        shutil.move(str(apps[0]), str(macos_app()))
        shutil.rmtree(backup)
        return dict(trusted_comment=comment, archive=archive.name)
    title = ('Updater-Archiv laden, Signatur prüfen, Bundle austauschen (Nachbildung des Tauri-Updaters)'
             if SYSTEM == 'Darwin' else
             'N+1-Installer laden, Signatur prüfen, still über N installieren (Nachbildung, nicht der Tauri-Updater)')
    if not report.check('Q-16', title, 'OUT-251, OUT-252', replace):
        return

    def restarted():
        after = discover_macos(macos_app()) if SYSTEM == 'Darwin' else discover_windows(installed.root)
        mark = mock.mark()
        app = App(ctx, after, env, 'app-after-update.log')
        try:
            time.sleep(20)
            if not app.alive():
                raise AssertionError(f'N+1 beendet mit {app.proc.returncode}')
        finally:
            app.stop()
        if manifest_requests(mock.log(mark), info):
            raise AssertionError('Startprüfung trotz 24-h-Drosselung')
        return dict(gui=gui_version(ctx, after), pid=app.proc.pid)
    report.check('Q-17', 'N+1 startet am selben Ort, Drosselung über das Update erhalten (Nachbildung)',
                 'OUT-251, OUT-250', restarted)


MANUAL = {
    'Darwin': [
        ('H-01', 'Hilfe → „Nach Updates suchen“, Dialog, Austausch durch den Tauri-Updater und Neustart', 'OUT-251', 'M-02'),
        ('H-02', 'Update bei laufendem Crawl: kein Neustart, „Nach Crawl-Ende installieren“', 'OUT-250, OUT-251', 'M-02'),
        ('H-03', 'Neuigkeiten-Dialog nach dem Update', 'OUT-257', 'M-03'),
        ('H-04', 'Zuletzt geöffneter Crawl wieder offen, Aufgabe zeigt auf N+1 und läuft', 'OUT-251, OUT-166', 'M-04, M-05'),
    ],
    'Windows': [
        ('H-01', 'Update über die Oberfläche (Menü, Dialog-Schaltflächen) statt IPC', 'OUT-251', 'W-02'),
        ('H-02', 'Update bei laufendem Crawl: kein Neustart, „Nach Crawl-Ende installieren“', 'OUT-250, OUT-251', 'W-02'),
        ('H-03', 'Neuigkeiten-Dialog nach dem Update', 'OUT-257', 'W-03'),
        ('H-04', 'Zuletzt geöffneter Crawl wieder offen, Aufgabe zeigt auf N+1', 'OUT-251, OUT-166', 'W-04'),
        ('H-05', 'SmartScreen/UAC-Herausgeber beim Updater-Installer', 'OUT-266', 'W-05, W-16'),
    ],
}


if __name__ == '__main__':
    sys.exit(main())
