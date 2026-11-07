// acceptance: independent
// Ergänzende, unabhängige Last-Fixture: wie fx-ui-basic verlinkt jede Inhaltsseite zurück auf die Startseite.
// spec: UI-018, UI-032
import { createServer } from "node:http";
import type { AddressInfo } from "node:net";
import { startGate, type Gate } from "./livedetail-gate";

export const RATE = 3;
export const INTERVAL = 100;

// spec: UI-018, UI-145
// Kein vom Prüfling bestimmter Start und keine reduzierte Last. Mindestens 60 s decken auch einen
// anwachsenden Rückstand bei 2,5 statt 3 URL/s ab. Zehn Recorder-Takte Reserve beim maximalen Faktor.
export function lagWindow(machine: number, boundMs: number, alreadyCrawled: number) {
  const maxFactor = Math.max(2 * machine, 4);
  const reserveMs = 10 * INTERVAL * maxFactor;
  const durationMs = Math.max(60_000, boundMs * maxFactor + reserveMs);
  const remaining = Math.ceil(RATE * durationMs / 1000);
  return { maxFactor, reserveMs, durationMs, remaining, limit: alreadyCrawled + remaining };
}

// spec: UI-018, UI-032
// Root + `pages` Rückverlinker + eine am Tor wartende Schlussseite. Alle Antworten 50 ms verzögert;
// kein externes Netz, kein Weiterleitungslink, keine zusätzliche Linkquelle zur Startseite.
export async function startLoadFixture(pages: number): Promise<Gate> {
  const server = createServer((req, res) => {
    if (req.url === "/robots.txt") {
      res.writeHead(200, { "content-type": "text/plain" });
      res.end("User-agent: *\nAllow: /\n");
      return;
    }
    const path = new URL(req.url ?? "/", "http://fixture.invalid").pathname;
    const n = /^\/page-(\d+)\.html$/.exec(path);
    const valid = path === "/" || path === "/hold.html" || (n && Number(n[1]) >= 1 && Number(n[1]) <= pages);
    const links = path === "/"
      ? Array.from({ length: pages }, (_, i) => `<a href="/page-${i + 1}.html">Page ${i + 1}</a>`).join("") + '<a href="/hold.html">Hold</a>'
      : path === "/hold.html" ? "" : '<a href="/">Home</a>';
    const timer = setTimeout(() => {
      res.writeHead(valid ? 200 : 404, { "content-type": "text/html; charset=utf-8" });
      res.end(`<!doctype html><html lang="en"><head><title>${path}</title></head><body><h1>${path}</h1>${links}</body></html>`);
    }, 50);
    res.on("close", () => clearTimeout(timer));
  });
  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
  const gate = await startGate(`http://127.0.0.1:${(server.address() as AddressInfo).port}`);
  const stop = gate.stop.bind(gate);
  return { ...gate, async stop() {
    await stop();
    server.closeAllConnections();
    await new Promise<void>((resolve) => server.close(() => resolve()));
  } };
}
