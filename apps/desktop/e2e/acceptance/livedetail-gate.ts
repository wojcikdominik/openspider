// Gesteuerter Fixture-Crawl für die Abnahme B4-LIVEDETAIL.
// acceptance: independent
//
// Problem der festen Rate: Wie lange ein Testschritt dauert, hängt vom Rechner ab; auf langsamen CI-Runnern war der Fixture-Crawl
// (27 URLs) oft fertig, bevor der Test seinen Schritt machte. Dieser Vorsatz-Server (Node, nur Loopback) reicht jede Anfrage an
// `os-testsite` weiter, lässt aber nur so viele Inhalts-Anfragen durch, wie der Test freigibt (`allow`). Alle weiteren Anfragen
// warten, der Crawl bleibt „Running“ und kann nicht vorzeitig enden – unabhängig von der Geschwindigkeit des Rechners. Der Test
// bestimmt, wann der Crawl weiterläuft und wann er endet (`open`). Der Crawl selbst läuft unverändert durch die echte Bridge.
//
// Zählung: `robots.txt` ist frei. Alle anderen Anfragen zählen (fx-ui-basic: Startseite, page-1 … page-20, /redirect, /missing,
// /noindex, /pixel.svg = 25 interne Anfragen; die zwei externen Adressen zeigen auf `localhost:<Port>` und gehen am Vorsatz vorbei,
// werden aber – bei einem Thread und der Reihenfolge der Warteschlange – erst nach den internen URLs geholt).
import { createServer, request, type IncomingMessage, type Server, type ServerResponse } from "node:http";
import type { AddressInfo } from "node:net";

export interface Gate {
  /** `http://127.0.0.1:<port>` ohne Schrägstrich am Ende. */
  base: string;
  startUrl: string;
  /** Anzahl der bisher durchgelassenen Inhalts-Anfragen. */
  admitted(): number;
  /** Bisher freigegebene Gesamtzahl (Budget). */
  budget(): number;
  /** Anzahl der wartenden Anfragen (der Crawl hängt am Tor). */
  waiting(): number;
  /** Lässt `n` weitere Anfragen durch. */
  allow(n: number): void;
  /** Lässt so viele durch, dass insgesamt `total` Anfragen durchgelassen sind. */
  allowTo(total: number): void;
  /** Öffnet das Tor ganz: der Crawl läuft bis zum Ende. */
  open(): void;
  /** Gibt im Takt von `rate` Anfragen pro Sekunde je eine frei, bis insgesamt `limit` erreicht sind (Zeitgeber im Test, nicht im Crawler). */
  meter(rate: number, limit: number): void;
  stopMeter(): void;
  stop(): Promise<void>;
}

export async function startGate(upstreamBase: string, initial = 0): Promise<Gate> {
  const upstream = new URL(upstreamBase);
  let budget = initial;
  let admitted = 0;
  let timer: NodeJS.Timeout | undefined;
  const queue: { go: () => void }[] = [];
  const sockets = new Set<import("node:net").Socket>();

  const pump = () => {
    while (queue.length && admitted < budget) {
      admitted++;
      queue.shift()!.go();
    }
  };
  const forward = (req: IncomingMessage, res: ServerResponse) => {
    const up = request(
      { host: upstream.hostname, port: upstream.port, path: req.url, method: req.method, headers: { ...req.headers, host: upstream.host } },
      (ur) => {
        res.writeHead(ur.statusCode ?? 502, ur.headers);
        ur.pipe(res);
      },
    );
    up.on("error", () => res.destroy());
    req.pipe(up);
  };
  const server: Server = createServer((req, res) => {
    if (req.url === "/robots.txt") return forward(req, res);
    const entry = { go: () => forward(req, res) };
    queue.push(entry);
    // Bricht der Crawler eine wartende Anfrage ab (Pause, Ende), darf sie keinen Platz im Budget belegen.
    res.on("close", () => {
      const i = queue.indexOf(entry);
      if (i >= 0) queue.splice(i, 1);
    });
    pump();
  });
  server.on("connection", (s) => {
    sockets.add(s);
    s.on("close", () => sockets.delete(s));
  });
  await new Promise<void>((ok) => server.listen(0, "127.0.0.1", ok));
  const port = (server.address() as AddressInfo).port;
  const base = `http://127.0.0.1:${port}`;
  return {
    base,
    startUrl: `${base}/`,
    admitted: () => admitted,
    budget: () => budget,
    waiting: () => queue.length,
    allow(n) {
      budget += n;
      pump();
    },
    allowTo(total) {
      budget = Math.max(budget, total);
      pump();
    },
    open() {
      budget = Number.POSITIVE_INFINITY;
      pump();
    },
    meter(rate, limit) {
      clearInterval(timer);
      timer = setInterval(() => {
        if (budget >= limit) return;
        budget += 1;
        pump();
      }, 1000 / rate);
    },
    stopMeter() {
      clearInterval(timer);
    },
    async stop() {
      clearInterval(timer);
      budget = Number.POSITIVE_INFINITY;
      pump();
      for (const s of sockets) s.destroy();
      await new Promise<void>((ok) => server.close(() => ok()));
    },
  };
}
