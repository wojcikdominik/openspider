// B4-LIVEDETAIL: eigene Konfiguration für das Messrezept `livedetail.measure.ts` (nicht Teil der Perf-Suite `*.bench.ts`
// und nicht der normalen E2E-Suite). Vom Repository-Wurzelverzeichnis:
//   npm exec -w apps/desktop -- vite build --config e2e/perf/vite.config.ts
//   OS_APP_BRIDGE_BIN=$PWD/target/release/os-app-bridge \
//     npm exec -w apps/desktop -- playwright test --config e2e/perf/livedetail.config.ts
// Umgebung: OS_E2E_PROJECTS (chromium,webkit), OS_LIVEDETAIL_PAGES/RATE/THREADS/WARMUP/ROUNDS/IDLE_MS/LABEL,
// OS_LIVEDETAIL_PROFILE=1 (Chromium: CPU-Profil der Messphase), OS_LIVEDETAIL_TRACE=1 (Chromium: Performance-Trace), OS_LIVEDETAIL_DEV=1 (Bedingungen der E2E-Suite:
// Vite-Entwicklungsserver statt Produktionsbuild; ohne OS_APP_BRIDGE_BIN die Debug-Bridge).
import { defineConfig } from "@playwright/test";
import base from "./playwright.config";

const dev = process.env.OS_LIVEDETAIL_DEV === "1";
export default defineConfig({
  ...base,
  testMatch: "livedetail.measure.ts",
  timeout: 600_000,
  reporter: [["list"]],
  ...(dev ? {
    use: { ...base.use, baseURL: "http://localhost:1420" },
    webServer: { command: "npm run dev", url: "http://localhost:1420", reuseExistingServer: false, timeout: 60_000, stdout: "ignore", cwd: "../.." },
  } : {}),
});
