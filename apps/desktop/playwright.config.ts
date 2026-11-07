import { defineConfig, devices } from "@playwright/test";

// E2E-A „Frontend + Kern“ (M2-ui-concept 7.3): Playwright gegen das Vite-Frontend, IPC über die E2E-Bridge `os-app-bridge`
// (Umgebung OS_E2E_TRANSPORT=bridge|mock|auto, siehe e2e/support/bridge.ts). Engines: WebKit (≈ WKWebView) und Chromium
// (≈ WebView2); auf macOS-CI nur WebKit, auf Windows-CI nur Chromium (OS_E2E_PROJECTS), in der Cloud (Linux) beide.
const wanted = (process.env.OS_E2E_PROJECTS ?? "chromium,webkit").split(",").map((s) => s.trim());

// Laufzeitbudgets (UI-146) werden erst nach allen übrigen Tests und ohne parallele Last gemessen: je Engine ein eigenes
// Projekt `<engine>-perf`, das vom Hauptprojekt abhängt. `OS_E2E_PROJECTS` nennt weiterhin nur die Engines.
const PERF = /viz[\\/]perf\.spec\.ts$/;
// Die Zeitmessung B4-LIVEDETAIL (Median erster Klick, UI-145) liefert in der PR-CI ohne Basis kein Pass/Fail-Signal (der Test prüft nur mit
// OS_LD_BASELINE_*) und braucht je Engine mehrere Minuten. Sie läuft nächtlich (`ui-nightly.yml`) im Projekt `<engine>-nightly`
// und ist in den Haupt- und Perf-Projekten ausgenommen. Die Nightly-Projekte gibt es nur mit OS_E2E_NIGHTLY=1, sonst liefe die Messung bei
// jedem `npm run e2e` ohne `--project` (ui-e2e.yml, spec-coverage-m2.yml) mit.
const NIGHTLY = /acceptance[\\/]livedetail-budget\.spec\.ts$/;
// Visual snapshots require the dedicated platform/scale configuration. Node tests of the U31 perf harness
// (`e2e/perf/**/*.test.mjs`, `*.nodetest.mts`) run under `node --test`, not in `npm run e2e`.
const PERF_NODE_TESTS = [/perf[\\/].*\.test\.mjs$/, /\.nodetest\.mts$/];
const MAIN_IGNORE = [PERF, NIGHTLY, /visual[\\/]/, ...PERF_NODE_TESTS];
const engines = [
  { name: "chromium", use: { ...devices["Desktop Chrome"], viewport: { width: 1280, height: 800 } } },
  { name: "webkit", use: { ...devices["Desktop Safari"], viewport: { width: 1280, height: 800 } } },
];
const all = engines.flatMap((engine) => [
  { ...engine, testIgnore: MAIN_IGNORE },
  { name: `${engine.name}-perf`, use: engine.use, testMatch: PERF, dependencies: [engine.name] },
  { name: `${engine.name}-nightly`, use: engine.use, testMatch: NIGHTLY },
]);

// Eigener Port je Worktree (OS_E2E_PORT): Auf dem Build-Server laufen Pakete parallel; mit dem festen Port 1420 und
// reuseExistingServer lief Playwright sonst gegen den Vite-Server eines fremden Worktrees (B4-LANG-Tests, 06.11.2026).
const port = Number(process.env.OS_E2E_PORT ?? 1420);

export default defineConfig({
  testDir: "./e2e",
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 1 : 0,
  reporter: process.env.CI ? [["list"], ["html", { open: "never" }]] : "list",
  timeout: 30_000,
  expect: { timeout: 5_000 },
  use: {
    baseURL: `http://localhost:${port}`,
    trace: "retain-on-failure",
    reducedMotion: "reduce",
  },
  projects: all.filter((p) => wanted.includes(p.name.replace(/-(perf|nightly)$/, "")) && (!p.name.endsWith("-nightly") || process.env.OS_E2E_NIGHTLY === "1")),
  webServer: {
    command: `npm run dev -- --port ${port}`,
    url: `http://localhost:${port}`,
    reuseExistingServer: !process.env.CI,
    timeout: 60_000,
    stdout: "ignore",
  },
});
