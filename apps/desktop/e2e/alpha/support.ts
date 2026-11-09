// Hilfen der Alpha-Abnahme (M2-A01): Fixture-Site fx-ui-basic (os-testsite), echte Bridge mit bekanntem Home und die
// Kommandozeile `openspider` für den Vergleich „UI = CLI“. Alles läuft auf Loopback, ohne externes Netz.
import { spawn, spawnSync, type ChildProcess } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, readdirSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { bridgeBinary } from "../support/bridge";

const EXE = process.platform === "win32" ? ".exe" : "";

/** Zielordner von Cargo: wie `bridgeBinary()` (CARGO_TARGET_DIR, sonst `<repo>/target`). */
function debugDir(): string {
  const target = process.env.CARGO_TARGET_DIR ?? resolve(import.meta.dirname, "..", "..", "..", "..", "target");
  return join(target, "debug");
}

export type Tool = "openspider" | "os-testsite" | "os-parity";
const ENV_OF: Record<Tool, string> = { openspider: "OS_CLI_BIN", "os-testsite": "OS_TESTSITE_BIN", "os-parity": "OS_PARITY_BIN" };

export function toolBinary(tool: Tool): string {
  return process.env[ENV_OF[tool]] ?? join(debugDir(), `${tool}${EXE}`);
}

const sleep = (ms: number) => new Promise<void>((r) => setTimeout(r, ms));

/**
 * Baut CLI, Testserver und Paritätswerkzeug, falls sie fehlen. Die CI baut sonst nur die Bridge; mehrere Worker einigen
 * sich über ein Sperrverzeichnis (mkdir ist atomar), damit Cargo nur einmal läuft.
 */
export async function ensureTools(): Promise<void> {
  const missing = () => (["openspider", "os-testsite", "os-parity"] as Tool[]).filter((t) => !existsSync(toolBinary(t)));
  if (missing().length === 0) return;
  const lock = join(tmpdir(), "os-alpha-build.lock");
  const deadline = Date.now() + 25 * 60_000;
  for (;;) {
    if (missing().length === 0) return;
    let mine = false;
    try {
      mkdirSync(lock);
      mine = true;
    } catch {
      /* ein anderer Worker baut */
    }
    if (mine) {
      try {
        const r = spawnSync("cargo", ["build", "--locked", "-p", "os-cli", "-p", "os-testsite", "-p", "os-parity"], { stdio: "inherit", cwd: resolve(import.meta.dirname, "..", "..", "..", "..") });
        if (r.status !== 0) throw new Error(`cargo build für die Alpha-Werkzeuge schlug fehl (Status ${r.status})`);
      } finally {
        rmSync(lock, { recursive: true, force: true });
      }
      return;
    }
    if (Date.now() > deadline) throw new Error("Zeitüberschreitung beim Warten auf den Build der Alpha-Werkzeuge");
    await sleep(1000);
  }
}

export interface Site {
  /** `http://127.0.0.1:<port>` ohne Schrägstrich am Ende. */
  base: string;
  port: number;
  stop(): Promise<void>;
}

/** Startet den Fixture-Server `os-testsite ui/fx-ui-basic` (25 interne URLs, 2 externe, Redirect, 404, noindex) bzw. das genannte Szenario. */
export async function startSite(scenario = "ui/fx-ui-basic"): Promise<Site> {
  const child = spawn(toolBinary("os-testsite"), [scenario], { stdio: ["ignore", "pipe", "inherit"] });
  const line = await new Promise<string>((ok, fail) => {
    let buf = "";
    const timer = setTimeout(() => fail(new Error("os-testsite meldet keine Adresse")), 20_000);
    child.stdout?.on("data", (d: Buffer) => {
      buf += d.toString();
      const m = /-> (http:\/\/127\.0\.0\.1:\d+)/.exec(buf);
      if (m) {
        clearTimeout(timer);
        ok(m[1]!);
      }
    });
    child.on("exit", () => {
      clearTimeout(timer);
      fail(new Error("os-testsite beendet sich vorzeitig"));
    });
  });
  const port = Number(new URL(line).port);
  return { base: line, port, async stop() { child.kill(); await sleep(50); } };
}

export interface AlphaBridge {
  /** `ws://127.0.0.1:<port>/?token=<hex>` */
  url: string;
  home: string;
  crawlsDir: string;
  stop(): Promise<void>;
}

/** Wie `startBridge`, aber mit sichtbarem Home: die CLI verwendet es als `OPENSPIDER_HOME`, um dieselbe Crawl-Verwaltung zu sehen. */
export async function startAlphaBridge(): Promise<AlphaBridge> {
  const bin = bridgeBinary();
  if (!existsSync(bin)) throw new Error(`os-app-bridge fehlt (${bin}); cargo build -p os-app --features e2e-bridge --bin os-app-bridge`);
  const home = mkdtempSync(join(tmpdir(), "os-alpha-"));
  const child: ChildProcess = spawn(bin, ["--home", home], { stdio: ["ignore", "pipe", "inherit"] });
  const cleanup = () => {
    try {
      rmSync(home, { recursive: true, force: true, maxRetries: 10, retryDelay: 100 });
    } catch {
      /* Temp-Verzeichnis bleibt liegen (Windows hält Dateien kurz gesperrt) */
    }
  };
  const info = await new Promise<{ port: number; token: string }>((ok, fail) => {
    let buf = "";
    const timer = setTimeout(() => fail(new Error("os-app-bridge meldet keinen Port")), 20_000);
    child.stdout?.on("data", (d: Buffer) => {
      buf += d.toString();
      const nl = buf.indexOf("\n");
      if (nl >= 0) {
        clearTimeout(timer);
        try {
          ok(JSON.parse(buf.slice(0, nl)));
        } catch (e) {
          fail(e as Error);
        }
      }
    });
    child.on("exit", () => {
      clearTimeout(timer);
      fail(new Error("os-app-bridge beendet sich vorzeitig"));
    });
  });
  return {
    url: `ws://127.0.0.1:${info.port}/?token=${info.token}`,
    home,
    crawlsDir: join(home, "crawls"),
    async stop() {
      child.kill();
      await sleep(50);
      cleanup();
    },
  };
}

export interface CliResult {
  code: number | null;
  stdout: string;
  stderr: string;
}

/** Führt `openspider` mit Argumenten aus; `home` setzt `OPENSPIDER_HOME`. */
export function runCli(args: string[], opts: { home: string; cwd?: string }): Promise<CliResult> {
  return new Promise((ok, fail) => {
    const child = spawn(toolBinary("openspider"), args, { cwd: opts.cwd, env: { ...process.env, OPENSPIDER_HOME: opts.home, OPENSPIDER_SECRET_STORE: "memory" }, stdio: ["ignore", "pipe", "pipe"] });
    let stdout = "";
    let stderr = "";
    child.stdout.on("data", (d: Buffer) => (stdout += d.toString()));
    child.stderr.on("data", (d: Buffer) => (stderr += d.toString()));
    child.on("error", fail);
    child.on("exit", (code) => ok({ code, stdout, stderr }));
  });
}

/** Startet `openspider` im Hintergrund (für den Fremdprozess-Crawl). */
export function spawnCli(args: string[], opts: { home: string }): { child: ChildProcess; done: Promise<CliResult> } {
  const child = spawn(toolBinary("openspider"), args, { env: { ...process.env, OPENSPIDER_HOME: opts.home, OPENSPIDER_SECRET_STORE: "memory" }, stdio: ["ignore", "pipe", "pipe"] });
  let stdout = "";
  let stderr = "";
  child.stdout?.on("data", (d: Buffer) => (stdout += d.toString()));
  child.stderr?.on("data", (d: Buffer) => (stderr += d.toString()));
  const done = new Promise<CliResult>((ok) => child.on("exit", (code) => ok({ code, stdout, stderr })));
  return { child, done };
}

export function tempDir(prefix: string): string {
  return mkdtempSync(join(tmpdir(), prefix));
}

export function removeDir(dir: string): void {
  try {
    rmSync(dir, { recursive: true, force: true, maxRetries: 10, retryDelay: 100 });
  } catch {
    /* best effort */
  }
}

export function listFiles(dir: string): string[] {
  return existsSync(dir) ? readdirSync(dir).sort() : [];
}
