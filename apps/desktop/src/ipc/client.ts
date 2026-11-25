// Ein Client, drei Transporte (Tauri, Bridge, Mock) hinter einer Schnittstelle (docs/plan/M2.md 3.8).
// Aufgaben über den Transport hinaus: Fehler → IpcError, AbortSignal → `op.cancel`, Verwerfen veralteter Antworten.
import commands from "./commands.json";
import { trackBegin } from "./begin/crawlBegin";
import { IpcError, localError, toIpcError } from "./errors";
import type { OpHandle } from "./gen/OpHandle";
import { createBridgeTransport } from "./transport/bridge";
import { createTauriTransport, isTauri } from "./transport/tauri";
import type { Transport, UiEvent } from "./transport/types";

export type { Transport, UiEvent };

export interface IpcClient {
  readonly transportName: Transport["name"];
  invoke<T>(cmd: string, args?: unknown, signal?: AbortSignal): Promise<T>;
  subscribe(window: string, onEvent: (e: UiEvent) => void): () => void;
}

const LONG_RUNNING: ReadonlySet<string> = new Set(
  (commands as { name: string; longRunning: boolean }[]).filter((c) => c.longRunning).map((c) => c.name),
);

function isOpHandle(v: unknown): v is OpHandle {
  return typeof v === "object" && v !== null && typeof (v as OpHandle).opId === "string";
}

/** Merkt je `viewId` die höchste Version; ältere Antworten gelten als veraltet (UI-142, kein Flackern). */
export class VersionGuard {
  private readonly latest = new Map<string, number>();
  /** Wirft `Stale`, wenn die Antwort älter ist als die zuletzt gesehene. */
  accept<T>(value: T): T {
    if (typeof value === "object" && value !== null) {
      // Building is an operation acknowledgement, not a published data version.
      // Keep the watermark so late rows from the previous generation still fail.
      if ("state" in value && value.state === "building") return value;
      const v = value as { viewId?: unknown; version?: unknown };
      if (typeof v.viewId === "string" && typeof v.version === "number") {
        const seen = this.latest.get(v.viewId);
        if (seen !== undefined && v.version < seen) throw new IpcError(localError("Stale", "err-stale"));
        this.latest.set(v.viewId, v.version);
      }
    }
    return value;
  }
  /** Eine Ansicht wurde geschlossen. */
  forget(viewId: string): void {
    this.latest.delete(viewId);
  }
}

export function createClient(t: Transport): IpcClient {
  const guard = new VersionGuard();
  return {
    transportName: t.name,
    async invoke<T>(cmd: string, args?: unknown, signal?: AbortSignal): Promise<T> {
      if (signal?.aborted) throw new IpcError(localError("Cancelled", "err-cancelled"));
      let result: T;
      try {
        result = await t.invoke<T>(cmd, args ?? {}, signal);
      } catch (e) {
        throw toIpcError(e);
      }
      if (signal?.aborted && !LONG_RUNNING.has(cmd)) throw new IpcError(localError("Cancelled", "err-cancelled"));
      if (LONG_RUNNING.has(cmd) && signal && isOpHandle(result)) {
        const opId = result.opId;
        const cancel = () => void t.invoke("op.cancel", { opId }).catch(() => undefined);
        if (signal.aborted) cancel();
        else signal.addEventListener("abort", cancel, { once: true });
      }
      return guard.accept(result);
    },
    subscribe: (window, onEvent) => t.subscribe(window, onEvent),
  };
}

// --- aktiver Client (Singleton), von den generierten Domänen-Wrappern benutzt -------------------------------------

let active: IpcClient | null = null;

export function setClient(c: IpcClient | null): void {
  active = c;
}

export function getClient(): IpcClient {
  if (!active) throw new Error("IPC-Client nicht initialisiert (setClient)");
  return active;
}

/** Stellvertreter für den aktiven Client: `client.invoke(...)` in den Wrappern. */
export const client: IpcClient = {
  get transportName() {
    return getClient().transportName;
  },
  // fix-sel (UI-018): begins of new content from this window are noted for the upper table.
  invoke: <T>(cmd: string, args?: unknown, signal?: AbortSignal) => trackBegin(cmd, () => getClient().invoke<T>(cmd, args, signal)),
  subscribe: (window, onEvent) => getClient().subscribe(window, onEvent),
};

export type TransportChoice = "tauri" | "bridge" | "mock";

/** Wählt den Transport: Tauri (Fenster-Internals), Bridge (`VITE_E2E_BRIDGE` bzw. `?bridge=`), sonst Mock. */
export function detectTransport(env: {
  win?: object;
  search?: string;
  bridgeEnv?: string;
  testBuild?: boolean;
}): { choice: TransportChoice; bridgeUrl?: string } {
  if (isTauri(env.win ?? globalThis)) return { choice: "tauri" };
  const params = new URLSearchParams(env.search ?? "");
  const testBuild = env.testBuild ?? false;
  const fromQuery = testBuild ? params.get("bridge") : null;
  const bridgeUrl = fromQuery ?? (env.bridgeEnv || undefined);
  if (testBuild && params.get("transport") === "mock") return { choice: "mock" };
  if (bridgeUrl) return { choice: "bridge", bridgeUrl };
  return { choice: "mock" };
}

export function createBridgeClient(url: string): IpcClient {
  return createClient(createBridgeTransport({ url }));
}

export function createTauriClient(): IpcClient {
  return createClient(createTauriTransport());
}
