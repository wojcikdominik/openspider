// Messrezept B4-LIVEDETAIL (M2-N32; UI-145, UI-143) im nativen Release-Build (macOS WKWebView / Windows WebView2):
// startet die mit `--features e2e-webdriver` gebaute App (Perf-Shell aus `prepare.mjs`), einen synthetischen Fixture-Server
// und misst in der Hauptoberfläche `window.liveDetail` (e2e/perf/livedetail.ts): Live-Crawl, dann je Runde Klick auf
// Inlinks → Outlinks → Inlinks mit Zeitaufteilung. Aufruf vom Repository-Wurzelverzeichnis, Schritte siehe
// `livedetail-mac.sh`. Ergebnis: target/loadtest/livedetail-native.json. Budget UI-145: erste Seite ≤ 150 ms.
import { spawn } from "node:child_process";
import { mkdtemp, rm, mkdir, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { resolve, join } from "node:path";
import { createServer } from "node:net";
import { setTimeout as delay } from "node:timers/promises";

const exe = process.platform === "win32" ? ".exe" : "";
const binary = resolve(process.argv[2] ?? `target/release/openspider-desktop${exe}`);
const testsite = resolve(process.env.OS_TESTSITE_BIN ?? `target/release/os-testsite${exe}`);
const num = (name, fallback) => (process.env[name] ? Number(process.env[name]) : fallback);
const pages = num("OS_LIVEDETAIL_PAGES", 5000);
const rate = num("OS_LIVEDETAIL_RATE", 150);
const options = {
  threads: num("OS_LIVEDETAIL_THREADS", 4),
  rate: rate > 0 ? rate : null,
  warmup: num("OS_LIVEDETAIL_WARMUP", 300),
  rounds: num("OS_LIVEDETAIL_ROUNDS", 10),
  idleMs: num("OS_LIVEDETAIL_IDLE_MS", 5000),
};
const BUDGET_MS = 150;
const freePort = () =>
  new Promise((ok) => {
    const s = createServer();
    s.listen(0, "127.0.0.1", () => {
      const n = s.address().port;
      s.close(() => ok(n));
    });
  });
const home = await mkdtemp(join(tmpdir(), "os-livedetail-native-"));
const port = await freePort();
const site = spawn(testsite, [`synthetic:n=${pages},links=30,seed=7`], { stdio: ["ignore", "pipe", "inherit"] });
const base = await new Promise((ok, fail) => {
  let out = "";
  const timer = setTimeout(() => fail(new Error("fixture startup timeout")), 20000);
  site.stdout.on("data", (d) => {
    out += String(d);
    const m = /-> (http:\/\/127\.0\.0\.1:\d+)/.exec(out);
    if (m) {
      clearTimeout(timer);
      ok(m[1]);
    }
  });
  site.once("error", fail);
});
const child = spawn(binary, [], {
  env: { ...process.env, OPENSPIDER_HOME: home, OPENSPIDER_SECRET_STORE: "memory", TAURI_WEBDRIVER_PORT: String(port) },
  stdio: ["ignore", "pipe", "pipe"],
});
let log = "";
child.stdout.on("data", (d) => (log += d));
child.stderr.on("data", (d) => (log += d));
const request = async (path, body, method = "POST") => {
  const r = await fetch(`http://127.0.0.1:${port}${path}`, {
    method,
    headers: { "Content-Type": "application/json" },
    signal: AbortSignal.timeout(30000),
    ...(body === undefined ? {} : { body: JSON.stringify(body) }),
  });
  const v = await r.json();
  if (!r.ok || v.value?.error) throw new Error(JSON.stringify(v));
  return v.value;
};
let session;
const execute = (script) => request(`/session/${session}/execute/sync`, { script, args: [] });
async function until(fn, ms, step, interval = 50) {
  const deadline = performance.now() + ms;
  while (performance.now() < deadline) {
    const r = await fn();
    if (r) return r;
    await delay(interval);
  }
  throw new Error(`Native readiness timeout: ${step}`);
}
const report = { scenario: "livedetail-native", started_at: new Date().toISOString(), pages, options, errors: [] };
try {
  child.on("error", (e) => report.errors.push(String(e)));
  await until(
    async () => {
      try {
        session = (await request("/session", { capabilities: { alwaysMatch: {} } })).sessionId;
        return session;
      } catch {
        if (child.exitCode !== null) throw new Error(`App exited: ${log}`);
        return false;
      }
    },
    30000,
    "webdriver session",
  );
  await until(() => execute('return document.querySelector("[data-shell-ready]")?.getAttribute("data-shell-ready")==="true"'), 30000, "shell ready");
  if (process.platform === "darwin")
    await until(() => execute('return document.visibilityState==="visible"'), 5000, "window visible").catch(() => {
      throw new Error("Native window not visible: unlocked GUI session with visible app window required");
    });
  report.userAgent = await execute("return navigator.userAgent");
  await execute(
    `window.__ld=null; window.liveDetail(${JSON.stringify({ base: `${base}/`, ...options })}).then(v=>window.__ld={value:v},e=>window.__ld={error:String(e)+' '+String(e&&e.stack||'')});return true;`,
  );
  // Rarely poll while measuring: each WebDriver script runs on the page's main thread.
  const r = await until(() => execute("return window.__ld"), 600000, "liveDetail", 1000);
  if (r.error) throw new Error(r.error);
  Object.assign(report, r.value);
  const fresh = r.value.samples.filter((s) => !s.retained);
  report.budget = { ms: BUDGET_MS, maxTotalMs: Math.max(...fresh.map((s) => s.totalMs)), passed: fresh.every((s) => s.totalMs <= BUDGET_MS) };
  if (!report.budget.passed) process.exitCode = 1;
} catch (error) {
  report.errors.push(String(error));
  process.exitCode = 1;
} finally {
  if (session) await request(`/session/${session}`, undefined, "DELETE").catch(() => {});
  for (const p of [child, site])
    if (p.exitCode === null && p.signalCode === null) {
      p.kill();
      await Promise.race([new Promise((ok) => p.once("exit", ok)), delay(5000)]);
      if (p.exitCode === null && p.signalCode === null) p.kill("SIGKILL");
    }
  await rm(home, { recursive: true, force: true, maxRetries: 10, retryDelay: 100 }).catch(() => {});
  await mkdir("target/loadtest", { recursive: true });
  await writeFile("target/loadtest/livedetail-native.json", JSON.stringify(report, null, 2));
  await writeFile("target/loadtest/livedetail-native.log", log);
  console.log(JSON.stringify({ errors: report.errors, idleLag: report.idleLag, measuringLag: report.measuringLag, summary: report.summary, budget: report.budget }, null, 2));
}
