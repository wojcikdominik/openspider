// Native E2E-B + platform spikes. Run from the repository root; see S-3/S-5b/S-6.
import assert from "node:assert/strict";
import { spawn, execFileSync } from "node:child_process";
import { once } from "node:events";
import { mkdtemp, mkdir, rm, writeFile, copyFile, readFile } from "node:fs/promises";
import { createHash } from "node:crypto";
import { tmpdir, release, arch } from "node:os";
import { resolve, join, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { createServer } from "node:net";
import { createServer as httpServer } from "node:http";
import { setTimeout as delay } from "node:timers/promises";
import { remote } from "webdriverio";
import { nativeCases } from "./u32-cases.mjs";
import { menuCases } from "../e2e-c/menu.mjs";
import { measure, validateRate } from "./measurement.mjs";
import { viteReady } from "./readiness.mjs";
import { prepareKeyboard } from "./focus.mjs";

const repo = resolve(dirname(fileURLToPath(import.meta.url)), "../../..");
const binary = process.argv[2] && resolve(process.argv[2]);
const mode = process.argv[3] ?? "smoke";
const output = resolve(process.argv[4] ?? `apps/desktop/e2e-b/artifacts/s02-${mode}`);
if (!binary || !["smoke", "s3", "s6", "u32", "menu"].includes(mode)) throw new Error("Usage: node apps/desktop/e2e-b/run.mjs <debug binary> [smoke|s3|s6|u32|menu] [output]");
if (!["darwin", "win32"].includes(process.platform)) throw new Error("Native spike requires macOS or Windows");
const started = Date.now();
const guiLaunch = process.platform === "darwin" && process.env.S02_GUI !== "0";
await mkdir(output, { recursive: true });
const home = await mkdtemp(join(tmpdir(), "openspider-s02-"));
const children = [];
let browser, appPid, fixture;
let logs = "";
const report = { mode, platform: process.platform, release: release(), arch: arch(), node: process.version,
  startedAt: new Date(started).toISOString(),
  revision: execFileSync("git", ["rev-parse", "HEAD"], { cwd: repo, encoding: "utf8" }).trim(),
  runnerSha256: createHash("sha256").update(await readFile(fileURLToPath(import.meta.url))).digest("hex"),
  readinessSha256: createHash("sha256").update(await readFile(new URL("./readiness.mjs", import.meta.url))).digest("hex"),
  focusSha256: createHash("sha256").update(await readFile(new URL("./focus.mjs", import.meta.url))).digest("hex"),
  nativeFocusSha256: createHash("sha256").update(await readFile(join(repo, process.platform === "darwin" ?
    "apps/desktop/spikes/macos.swift" : "apps/desktop/e2e-b/focus-windows.ps1"))).digest("hex"),
  probeSha256: createHash("sha256").update(await readFile(join(repo, "apps/desktop/spikes/probe.ts"))).digest("hex"),
  launch: guiLaunch ? "LaunchServices" : "child-process", checks: [], errors: [], keyboardFocus: [] };

async function freePort() {
  const server = createServer();
  server.listen(0, "127.0.0.1");
  await once(server, "listening");
  const port = server.address().port;
  await new Promise(r => server.close(r));
  return port;
}
function child(command, args, env = {}) {
  const p = spawn(command, args, { cwd: repo, env: { ...process.env, ...env }, windowsHide: true, stdio: ["ignore", "pipe", "pipe"] });
  p.on("error", e => { logs += `${command}: ${e}\n`; });
  p.stdout.on("data", d => { logs += d; });
  p.stderr.on("data", d => { logs += d; });
  children.push(p);
  return p;
}
async function until(fn, label, ms = 30_000) {
  const deadline = Date.now() + ms;
  let last;
  while (Date.now() < deadline) {
    try { const value = await fn(); if (value) return value; } catch (e) { last = e; }
    await delay(150);
  }
  throw new Error(`${label}: timed out${last ? ` (${last})` : ""}`);
}
// execute/async may block the WKWebView evaluation queue needed by IPC. Poll an independent promise instead.
async function evaluate(expression) {
  await browser.execute(`window.__s02Result = null; Promise.resolve().then(() => (${expression})).then(value => { window.__s02Result = {ok:true, value:value ?? null}; }, error => { window.__s02Result = {ok:false, failure:JSON.stringify(error, Object.getOwnPropertyNames(error))}; }); return true;`);
  const result = await until(() => browser.execute("return window.__s02Result"), expression);
  if (!result.ok) throw new Error(result.failure);
  return result.value;
}
async function check(name, fn) {
  try { const value = await fn(); report.checks.push({ name, status: "passed", value }); }
  catch (e) { report.checks.push({ name, status: "failed", error: String(e) }); report.errors.push(`${name}: ${e}`); }
}
function native(action, key) {
  const helper = process.env.S02_MAC_HELPER;
  assert.ok(helper, "Set S02_MAC_HELPER to compiled spikes/macos.swift");
  return execFileSync(helper, [action, String(appPid), ...(key ? [key] : [])], { encoding: "utf8", timeout: 10_000 });
}
function nativeFocus(action) {
  if (process.platform === "darwin") return JSON.parse(native(action));
  return JSON.parse(execFileSync("powershell.exe", ["-NoLogo", "-NoProfile", "-NonInteractive",
    "-ExecutionPolicy", "Bypass",
    "-File", join(repo, "apps/desktop/e2e-b/focus-windows.ps1"), "-Action", action, "-Binary", binary],
  { encoding: "utf8", windowsHide: true, timeout: 15_000 }));
}
async function key(value, target) {
  const trace = { key: value, target, at: new Date().toISOString() };
  report.keyboardFocus.push(trace);
  await prepareKeyboard({
    activate: async () => {
      await evaluate("window.s02.activate()");
      return nativeFocus("activate");
    },
    snapshot: () => evaluate("window.s02.keyboardFocus()"),
    focusTarget: () => browser.execute((id) => document.getElementById(id).focus(), target),
    inspect: async () => nativeFocus("inspect"),
    trace,
  });
  if (process.platform === "darwin") native("key", value);
  else await browser.performActions([{ type: "key", id: "keyboard", actions: [
    { type: "keyDown", value: "\uE009" }, { type: "keyDown", value },
    { type: "keyUp", value }, { type: "keyUp", value: "\uE009" },
  ] }]);
  trace.injected = true;
  await delay(250);
  trace.afterKey = await evaluate("window.s02.keyboardFocus()");
}
async function startFixture() {
  let requests = 0;
  fixture = httpServer((req, res) => {
    requests++;
    res.setHeader("Content-Type", "text/html");
    const i = Number(req.url.slice(1)) || 0;
    const timer = setTimeout(() => res.end(`<html><title>S02 fixture ${i}</title><body><a href="/${i + 1}">Next</a></body></html>`), 100);
    res.on("close", () => clearTimeout(timer));
  });
  fixture.listen(0, "127.0.0.1");
  await once(fixture, "listening");
  const url = `http://127.0.0.1:${fixture.address().port}/`;
  const result = await evaluate(`window.s02.start(${JSON.stringify(url)})`);
  report.crawlStart = result;
  await until(() => requests > 1, "Crawl must request the fixture");
  await until(async () => (await evaluate("window.s02.snapshot()")).ticks.some(t => t.phase === "running"), "Crawl must emit running ticks");
  return result;
}

try {
  // Always start our own Vite; an occupied fixed Tauri dev port is an error, never another worktree's app.
  const vite = child(process.execPath, [join(repo, "node_modules/vite/bin/vite.js"), join(repo, "apps/desktop"), "--host", "localhost"], {});
  await until(async () => {
    if (vite.exitCode !== null) throw new Error(`Vite exited ${vite.exitCode}: ${logs}`);
    return viteReady(logs);
  }, "Vite ready");
  const port = await freePort();
  const env = { OPENSPIDER_HOME: home, OPENSPIDER_SECRET_STORE: "memory", TAURI_WEBDRIVER_PORT: String(port) };
  if (process.platform === "win32") {
    const capabilities = { browserName: "webview2", "ms:edgeOptions": { binary, webviewOptions: {} } };
    // The runner talks to msedgedriver itself with the capabilities tauri-driver would build (`browserName: webview2`,
    // `ms:edgeOptions.binary`): tauri-driver only maps these and hides the native driver's output, while the verbose
    // msedgedriver log is the evidence when the session cannot be created. S02_WINDOWS_DRIVER=tauri selects tauri-driver.
    // The WebView2 remote-debugging flag comes from the test build (TAURI_CONFIG in ui-e2e-b.yml), see README.
    const useTauriDriver = process.env.S02_WINDOWS_DRIVER === "tauri";
    if (useTauriDriver) child(process.env.TAURI_DRIVER ?? "tauri-driver", ["--port", String(port), "--native-port", String(await freePort())], env);
    else child(process.env.EDGE_DRIVER ?? "msedgedriver", [`--port=${port}`, "--verbose", `--log-path=${join(output, "msedgedriver.log")}`], env);
    await until(async () => (await fetch(`http://127.0.0.1:${port}/status`, { signal: AbortSignal.timeout(1000) })).ok, "windows webdriver");
    try {
      browser = await remote({ hostname: "127.0.0.1", port, logLevel: "error", connectionRetryCount: 0,
        capabilities: useTauriDriver ? { "tauri:options": { application: binary } } : capabilities });
    } catch (e) {
      try { report.processes = execFileSync("powershell.exe", ["-NoLogo", "-NoProfile", "-NonInteractive", "-Command",
        "Get-CimInstance Win32_Process | Where-Object { $_.Name -match 'openspider|msedgewebview2' -and $_.CommandLine -notmatch 'type=(renderer|gpu-process|utility|crashpad)' } | Select-Object Name,ProcessId,ParentProcessId,CommandLine | ConvertTo-Json -Depth 3"],
        { encoding: "utf8", windowsHide: true, timeout: 20_000 }).slice(0, 5000); } catch (err) { report.processes = String(err); }
      throw e;
    }
  } else {
    if (guiLaunch) {
      const bundle = join(home, "S02.app");
      await mkdir(join(bundle, "Contents/MacOS"), { recursive: true });
      await copyFile(binary, join(bundle, "Contents/MacOS/OpenSpider"));
      await writeFile(join(bundle, "Contents/Info.plist"), '<?xml version="1.0"?><plist version="1.0"><dict><key>CFBundleExecutable</key><string>OpenSpider</string><key>CFBundleIdentifier</key><string>org.openspider.s02</string><key>CFBundleName</key><string>OpenSpider S02</string><key>CFBundlePackageType</key><string>APPL</string></dict></plist>');
      execFileSync("/usr/bin/open", ["-n", "-a", bundle, "--stdout", join(output, "app.stdout.log"), "--stderr", join(output, "app.stderr.log"), ...Object.entries(env).flatMap(([k,v]) => ["--env", `${k}=${v}`])]);
      appPid = await until(() => {
        const lines = execFileSync("/bin/ps", ["-A", "-o", "pid=,command="], { encoding: "utf8" }).split("\n");
        const line = lines.find(l => l.includes(join(bundle, "Contents/MacOS/OpenSpider")));
        return line && Number(line.trim().split(/\s+/)[0]);
      }, "GUI app process");
    } else appPid = child(binary, [], env).pid;
    await until(async () => (await fetch(`http://127.0.0.1:${port}/status`, { signal: AbortSignal.timeout(1000) })).ok, "embedded WebDriver");
    browser = await remote({ hostname: "127.0.0.1", port, logLevel: "error", connectionRetryCount: 0, capabilities: {} });
  }
  // Bound each probe so a lost startup callback cannot consume the entire readiness deadline.
  await browser.setTimeout({ script: 5000 });
  report.startup = { attempts: 0, transientErrors: [] };
  await until(async () => {
    report.startup.attempts++;
    try { return await browser.execute("return window.__osRuntime?.transport === 'tauri' && !!window.__osMenu"); }
    catch (e) { report.startup.transientErrors.push(String(e)); throw e; }
  }, "native app + menu");
  await browser.setTimeout({ script: 30_000 });
  await evaluate("import('/spikes/probe.ts').then(m => { window.s02 = m; })");
  report.beforeActivation = await evaluate("window.s02.snapshot()");
  report.afterTauriActivation = await evaluate("window.s02.activate()");
  if (process.platform === "darwin" && process.env.S02_MAC_HELPER) {
    report.nativeActivation = JSON.parse(native("activate"));
    report.afterNativeActivation = await evaluate("window.s02.snapshot()");
  } else if (process.platform === "win32") {
    report.nativeActivation = nativeFocus("activate");
    report.afterNativeActivation = await evaluate("window.s02.snapshot()");
  }
  // spec: UI-001, OUT-232
  await check("main window and real IPC", async () => {
    const state = await evaluate("window.s02.snapshot()");
    assert.equal(state.transport, "tauri");
    assert.equal(state.shell.visible, true);
    return state;
  });
  // spec: UI-113
  if (mode === "s3") {
    await evaluate("window.s02.installKeyboardFixture()");
    await check("native editing shortcuts in a text field", async () => {
      await key("a", "s02-input");
      let result = await evaluate("window.s02.keyboardSnapshot()");
      assert.deepEqual(result.selection, [0, 13]);
      await key("c", "s02-input");
      await key("v", "s02-paste");
      result = await evaluate("window.s02.keyboardSnapshot()");
      assert.equal(result.paste, "fixture alpha");
      await key("a", "s02-input"); await key("x", "s02-input");
      assert.equal((await evaluate("window.s02.keyboardSnapshot()")).value, "");
      await key("z", "s02-input");
      result = await evaluate("window.s02.keyboardSnapshot()");
      assert.equal(result.value, "fixture alpha");
      assert.deepEqual(result.actions, [], "Editing must not dispatch table/global commands");
      return result;
    });
    await check("table fixture C/A dispatch once; V/X/Z do not mutate it", async () => {
      const before = await evaluate("window.s02.keyboardSnapshot()");
      for (const k of ["c", "a", "v", "x", "z"]) await key(k, "s02-grid");
      const after = await evaluate("window.s02.keyboardSnapshot()");
      assert.deepEqual(after.actions.slice(before.actions.length), ["table.copy", "table.selectAll"]);
      assert.equal(after.value, before.value);
      assert.equal(after.paste, before.paste);
      return after;
    });
  }
  // spec: UI-143
  if (mode === "s6") {
    await check("running fixture crawl prerequisite", startFixture);
    if (report.errors.length === 0) {
      for (const state of ["visible", "minimized", "restored", "covered", "uncovered"]) {
        let cover;
        await check(`frontend-rate-${state}`, async () => {
          try {
            await check(`window-state-${state}`, async () => {
              if (state === "minimized") await evaluate("window.s02.minimize()");
              if (["restored", "uncovered"].includes(state)) {
                native("restore"); await evaluate("window.s02.restore()");
              }
              if (state === "covered") cover = child(process.env.S02_MAC_HELPER, ["cover", String(appPid)]);
              return await evaluate("window.s02.snapshot()");
            });
            await delay(2000);
            const before = await evaluate("window.s02.snapshot()");
            await delay(30_000);
            const after = await evaluate("window.s02.snapshot()");
            const measurement = measure(state, before, after);
            report.checks.push(measurement);
            validateRate(measurement);
            return { seconds: measurement.seconds, hz: measurement.hz };
          } finally {
            if (cover && cover.exitCode === null) {
              cover.kill();
              await Promise.race([once(cover, "exit"), delay(3000)]);
            }
          }
        });
      }
    }
    await check("native-occlusion-source", async () => {
      assert.fail("window.rs lacks NSWindowOcclusionState observation and separate telemetry for frontend/shell sources (N01 interface requirement)");
    });
  }
  if (mode === "u32") await nativeCases({ browser, evaluate, check, until, output, binary, startFixture, appPid });
  if (mode === "menu") await menuCases({ browser, evaluate, check, output, appPid });
  if (mode === "smoke" || mode === "u32") {
    // spec: UI-001
    await check("active native desktop", async () => {
      const state = await evaluate("window.s02.snapshot()");
      assert.equal(state.frontend, "visible");
      assert.equal(state.focus, true);
      return { frontend: state.frontend, focus: state.focus };
    });
    // spec: OUT-222, UI-016
    await check("separate settings window", async () => {
      const before = await browser.getWindowHandles();
      await evaluate("window.s02.settings()");
      // The Windows test build creates the (hidden) settings window at startup (see ui-e2e-b.yml), so it may already be among
      // the handles; find it by its label instead of by a new handle.
      let settings;
      await until(async () => {
        for (const handle of await browser.getWindowHandles()) {
          if (handle === before[0]) continue;
          await browser.switchToWindow(handle);
          report.settingsProbe = [...(report.settingsProbe ?? []).slice(-8), await browser.execute("return {label: window.__osRuntime?.label ?? null, href: location.href, text: document.body.innerText.length, ready: document.readyState}").catch(e => String(e))];
          if (await browser.execute("return window.__osRuntime?.label === 'settings' && document.body.innerText.length > 0").catch(() => false)) { settings = handle; return true; }
        }
        await browser.switchToWindow(before[0]);
        return false;
      }, "settings window");
      // Outside the Windows test build the settings window is created at runtime: it must be a new window.
      if (process.platform !== "win32") assert.ok(!before.includes(settings), "settings window must be a new window");
      await evaluate("import('/spikes/probe.ts').then(m => { window.s02 = m; })");
      const value = await evaluate("window.s02.snapshot()");
      assert.equal(value.transport, "tauri");
      assert.equal(value.shell.visible, true);
      assert.equal(value.shell.minimized, false);
      await browser.switchToWindow(before[0]);
      return { ...value, handle: settings };
    });
    // spec: UI-113, OUT-232
    // N01b: app.requestQuit itself opens the "Quit OpenSpider?" confirmation (the kernel broadcasts the
    // app-quit-confirmation notice for every confirm decision; CI screenshot of the failed run), so the window close is
    // judged first, while no dialog is open; the quit decision is read afterwards from the same running crawl.
    const dialogOpen = () => browser.execute("return !!document.querySelector('[role=alertdialog], [role=dialog], dialog[open]')");
    await check("closing during a running fixture crawl asks before quitting", async () => {
      if (!report.crawlStart) await startFixture();
      assert.equal(await dialogOpen(), false);
      await evaluate("window.s02.close()");
      await until(dialogOpen, "quit confirmation");
      return await browser.execute("return document.querySelector('[role=alertdialog], [role=dialog], dialog[open]').textContent");
    });
    await check("app.requestQuit confirms a running fixture crawl", async () => {
      report.quitDecision = await evaluate("window.s02.requestQuit()");
      assert.equal(report.quitDecision.kind, "confirm");
      assert.ok(report.quitDecision.reasons.includes("crawl"));
      return report.quitDecision;
    });
    if (mode === "u32") {
      // spec: UI-001
      await check("U32 cancel quit confirmation keeps the main window and crawl", async () => {
        const before = await browser.getWindowHandles();
        const clicked = await browser.execute(() => {
          const dialog = document.querySelector('[role=alertdialog], [role=dialog], dialog[open]');
          const cancel = [...(dialog?.querySelectorAll("button") ?? [])].find(button => /^(Cancel|Abbrechen)$/.test(button.textContent.trim()));
          cancel?.click();
          return !!cancel;
        });
        assert.equal(clicked, true, "Quit confirmation must offer Cancel");
        await until(() => browser.execute("return !document.querySelector('[role=alertdialog], [role=dialog], dialog[open]')"), "cancelled quit dialog");
        assert.deepEqual(await browser.getWindowHandles(), before);
        const state = await evaluate("window.s02.snapshot()");
        assert.equal(state.shell.visible, true);
        assert.ok(state.ticks.some(t => t.phase === "running"));
        return { cancelled: true, mainAlive: true };
      });
    }
  }
} catch (e) { report.errors.push(String(e)); }
finally {
  if (browser) {
    report.final = await evaluate("window.s02.snapshot()").catch(e => ({ error: String(e) }));
    await browser.saveScreenshot(join(output, "window.png")).catch(e => { logs += `Screenshot: ${e}\n`; });
    await browser.deleteSession().catch(() => {});
  }
  for (const p of children.reverse()) {
    if (p.exitCode === null) { p.kill(); await Promise.race([once(p, "exit"), delay(3000)]); if (p.exitCode === null) p.kill("SIGKILL"); }
  }
  if (guiLaunch && appPid) { try { process.kill(appPid, "SIGTERM"); } catch {} }
  if (fixture) { fixture.closeAllConnections(); fixture.close(); }
  report.seconds = (Date.now() - started) / 1000;
  report.status = report.errors.length ? "failed" : "passed";
  await writeFile(join(output, "report.json"), JSON.stringify(report, null, 2) + "\n");
  await writeFile(join(output, "native.log"), logs);
  await rm(home, { recursive: true, force: true });
}
console.log(JSON.stringify({ status: report.status, seconds: report.seconds, errors: report.errors }, null, 2));
if (report.status !== "passed") process.exitCode = 1;
