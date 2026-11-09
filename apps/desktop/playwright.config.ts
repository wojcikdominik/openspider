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
// Die beiden Lag-Prüfungen der Abnahme B4-LIVEDETAIL (Tabellenzähler/Chip und Inlinks bei 3 URL/s, UI-018) laufen je Engine über eine
// Minute Zufluss plus Schlussfenster (zusammen rund 3,5 bis 4 Minuten Worker-Zeit, auf zwei Kernen etwa 2 Minuten der Wanduhrzeit der
// livedetail-Specs; Messung Runde 3 in docs/plan/M3-reports/b4-livedetail-tests.md). Mit OS_E2E_LAG=split (nur `ui-e2e.yml`) liegen
// sie in einem eigenen Projekt `<engine>-lag` und fehlen im Hauptprojekt; die CI-Matrix führt dieses Projekt als eigenen Job aus, damit
// die Schritt-Laufzeit der übrigen Jobs unter dem Limit bleibt. Ohne OS_E2E_LAG (lokal, `spec-coverage-m2.yml`) bleiben beide Tests im
// Hauptprojekt und es gibt kein `-lag`-Projekt: Es entfällt keine Prüfung, und kein Test läuft doppelt.
const LAG = /grow at 3 URL\/s with bounded startup and final observation/;
const LAG_FILES = /acceptance[\\/]livedetail-(live|detail)\.spec\.ts$/;
const splitLag = process.env.OS_E2E_LAG === "split";
const engines = [
  { name: "chromium", use: { ...devices["Desktop Chrome"], viewport: { width: 1280, height: 800 } } },
  { name: "webkit", use: { ...devices["Desktop Safari"], viewport: { width: 1280, height: 800 } } },
];
const all = engines.flatMap((engine) => [
  { ...engine, testIgnore: MAIN_IGNORE, ...(splitLag ? { grepInvert: LAG } : {}) },
  { name: `${engine.name}-perf`, use: engine.use, testMatch: PERF, dependencies: [engine.name] },
  { name: `${engine.name}-nightly`, use: engine.use, testMatch: NIGHTLY },
  { name: `${engine.name}-lag`, use: engine.use, testMatch: LAG_FILES, grep: LAG },
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
  projects: all.filter(
    (p) =>
      wanted.includes(p.name.replace(/-(perf|nightly|lag)$/, "")) &&
      (!p.name.endsWith("-nightly") || process.env.OS_E2E_NIGHTLY === "1") &&
      (!p.name.endsWith("-lag") || splitLag),
  ),
  webServer: {
    command: `npm run dev -- --port ${port}`,
    url: `http://localhost:${port}`,
    reuseExistingServer: !process.env.CI,
    timeout: 60_000,
    stdout: "ignore",
  },
});
