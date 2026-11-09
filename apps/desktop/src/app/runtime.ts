// Start eines Fensters: Transport wählen, Client setzen, Einstellungen laden, Ereigniskanal, Erscheinungsbild und Sprache,
// Kürzel-Dispatcher und Menü. Alles hängt am Fenster-Label; Aufräumen über `dispose()`.
import { installMainMenus } from "../features/help/install";
import { resolveLanguage, setLanguage } from "../i18n";
import { registerAction } from "../ui/actions";
import { installShortcutDispatcher } from "../ui/dispatcher";
import { startMenuController, type MenuController } from "../ui/menu/controller";
import { registerMenuContributions } from "../ui/menu/registry";
import { currentPlatform } from "../ui/platform";
import { ZOOM_STEPS, type Platform } from "../ui/tokens/tokens";
import { createClient, createBridgeClient, createTauriClient, detectTransport, setClient, type IpcClient } from "../ipc/client";
import { bindVisibility, connectWindowEvents, eventBus } from "../ipc/events";
import { localStoragePersistence, MockCore } from "../ipc/transport/mock";
import type { Transport } from "../ipc/transport/types";
import { startPrefsBroadcast, usePrefs } from "../state/prefs";
import { attachSession, useSession } from "../state/session";
import { startAppearanceSync } from "./appearance";
import { BASE_MENU } from "./baseMenu";
import { installGlobalErrorLogging } from "./ErrorBoundary";
import { bindDetailPersistence } from "../features/detail/frame/persistence";
import { openSettingsWindow, quitApp } from "./windows";

export type WindowKind = "main" | "settings" | "viz";

export interface BootOptions {
  kind: WindowKind;
  label?: string;
  /** Vorgegebener Transport (Tests, Harness). */
  transport?: Transport;
  platform?: Platform;
  search?: string;
  /** Standard: nur das Hauptfenster installiert Kürzel und Menü. */
  shortcuts?: boolean;
  menu?: boolean;
}

export interface Runtime {
  client: IpcClient;
  label: string;
  /** Eingestellter Transport: `tauri`, `bridge` oder `mock`. */
  transportName: string;
  /** Erst nach geladenen Einstellungen und gesetzter Sprache ist das Fenster bereit. */
  ready: Promise<void>;
  menu: MenuController | null;
  dispose(): void;
}

declare global {
  interface Window {
    __osMock?: MockCore;
    __osRuntime?: { label: string; transport: string };
  }
}

function windowLabel(kind: WindowKind, search: string): string {
  if (kind !== "viz") return kind;
  return new URLSearchParams(search).get("label") ?? "viz-1";
}

function pickClient(opts: BootOptions, label: string, search: string): IpcClient {
  if (opts.transport) return createClient(opts.transport);
  const choice = detectTransport({
    search,
    bridgeEnv: import.meta.env.VITE_E2E_BRIDGE,
    testBuild: import.meta.env.DEV || import.meta.env.VITE_E2E === "1",
  });
  if (choice.choice === "tauri") return createTauriClient();
  if (choice.choice === "bridge" && choice.bridgeUrl) return createBridgeClient(choice.bridgeUrl);
  const core = (window.__osMock ??= new MockCore(undefined, localStoragePersistence()));
  return createClient(core.transport(label));
}

export function bootWindow(opts: BootOptions): Runtime {
  const search = opts.search ?? (typeof location !== "undefined" ? location.search : "");
  const label = opts.label ?? windowLabel(opts.kind, search);
  const platform = opts.platform ?? currentPlatform();
  const client = pickClient(opts, label, search);
  setClient(client);
  if (typeof window !== "undefined") window.__osRuntime = { label, transport: client.transportName };

  // Bis die Einstellungen da sind (oder falls der Kern fehlt): Systemsprache, Rückfall Englisch (UI-121)
  void setLanguage(resolveLanguage("system"));

  const offs: (() => void)[] = [];
  offs.push(installGlobalErrorLogging());
  offs.push(startPrefsBroadcast(new URLSearchParams(search).get("bridge") ?? client.transportName));
  offs.push(connectWindowEvents(client, label, eventBus));
  offs.push(attachSession(eventBus));
  offs.push(eventBus.on("settingsChanged", () => void usePrefs.getState().load()));
  offs.push(bindVisibility(label));

  const isMain = opts.kind === "main";
  const appearance = startAppearanceSync({ platform, reportLiveInterval: isMain });
  offs.push(appearance.stop);

  let menu: MenuController | null = null;
  if (isMain) {
    offs.push(registerAction("app.settings", () => void openSettingsWindow()));
    offs.push(registerAction("app.quit", () => void quitApp()));
    const zoomBy = (dir: -1 | 1 | 0) => {
      const cur = usePrefs.getState().appearance.zoomPct;
      const i = (ZOOM_STEPS as readonly number[]).indexOf(cur);
      const next = dir === 0 ? 100 : ZOOM_STEPS[Math.min(ZOOM_STEPS.length - 1, Math.max(0, (i < 0 ? 2 : i) + dir))];
      void usePrefs.getState().patch({ appearance: { zoomPct: next } });
    };
    offs.push(registerAction("zoom.in", () => zoomBy(1)));
    offs.push(registerAction("zoom.out", () => zoomBy(-1)));
    offs.push(registerAction("zoom.reset", () => zoomBy(0)));
  }

  if (isMain && (opts.menu ?? true)) {
    offs.push(registerMenuContributions("app/base", BASE_MENU));
    offs.push(installMainMenus());
    menu = startMenuController({
      platform,
      getContext: () => {
        const crawl = useSession.getState().crawl;
        return { hasCrawl: crawl !== null, crawlRunning: crawl?.phase === "running", hasData: (crawl?.crawled ?? 0) > 0, analysisRunning: useSession.getState().analysis?.running === true };
      },
      subscribeContext: (cb) => useSession.subscribe(cb),
    });
    offs.push(() => menu?.stop());
  }
  if (isMain && (opts.shortcuts ?? true)) {
    offs.push(
      installShortcutDispatcher({
        platform,
        nativeAccelerators: () => new Set(menu?.model()?.nativeAccelerators ?? []),
      }),
    );
  }

  let disposed = false;
  const ready = usePrefs
    .getState()
    .load()
    .then(async () => {
      if (isMain) {
        const off = await bindDetailPersistence();
        if (disposed) off(); else offs.push(off);
      }
      await appearance.apply();
    });

  return {
    client,
    label,
    transportName: client.transportName,
    ready,
    menu,
    dispose() {
      disposed = true;
      for (const off of offs.reverse()) off();
      setClient(null);
    },
  };
}
