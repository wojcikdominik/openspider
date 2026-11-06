import "../../src/main";
import { eventBus } from "../../src/ipc/events";
import { config } from "../../src/ipc/config";
import { crawls } from "../../src/ipc/crawls";
import { sidebar } from "../../src/ipc/sidebar";
import { crawl } from "../../src/ipc/crawl";
import { useSession } from "../../src/state/session";
import { recordFrames, paint } from "../../src/grid/recorder";
// B4-LIVEDETAIL (additiv): Messrezept `window.liveDetail` für Browser und nativen Release-Build.
import "./livedetail";
const ticks: { at: number; phase?: string }[] = [];
const clicks: number[] = [];
// B4-LIVEDETAIL: read ticks from the window's bus. There is exactly one event channel per window (`events.subscribe`);
// a second subscription replaced the app's channel in the native build, and the session then received no ticks.
eventBus.on("tick", (tick) => {
  ticks.push({ at: performance.now(), phase: tick.crawl?.phase });
});
document.addEventListener(
  "click",
  (event) => {
    const now = performance.now();
    const start =
      event.timeStamp > now + 60000
        ? event.timeStamp - performance.timeOrigin
        : event.timeStamp;
    void paint().then(() => {
      const latency = performance.now() - start;
      clicks.push(latency >= 0 ? latency : NaN);
    });
  },
  true,
);
window.perfShell = {
  ticks,
  clicks,
  state: () => useSession.getState().crawl,
  async start(base: string) {
    const dto = await config.get();
    const root = dto.config as Record<string, Record<string, unknown>>;
    await config.set({
      ...dto,
      config: {
        ...root,
        speed: { ...root.speed, threads: 20, max_urls_per_sec: null },
        analysis: { ...root.analysis, auto_at_end: false },
      },
    });
    await crawl.start({
      source: { kind: "spider", url: base },
      scope: "domain",
    });
  },
  open: (path: string) => crawl.open({ kind: "path", path }),
  completeSidebars: async () => {
    await Promise.all([sidebar.overview({}), sidebar.issues({})]);
  },
  async importFixture(path: string) {
    const operation = await crawls.import({ paths: [path] });
    const progress: {
      done: number;
      total?: number | null;
      indicator: boolean;
    }[] = [];
    const deadline = performance.now() + 120_000;
    while (performance.now() < deadline) {
      const status = useSession.getState().ops[operation.opId];
      if (status) {
        progress.push({
          done: status.done,
          total: status.total,
          indicator: !!document.querySelector('[role="progressbar"]'),
        });
        if (status.state === "done") return { result: status.result, progress };
        if (status.state === "failed" || status.state === "cancelled")
          throw new Error(JSON.stringify(status));
      }
      await new Promise((resolve) => setTimeout(resolve, 50));
    }
    throw new Error("Import timed out");
  },
  async scrollFrames(axis: "vertical" | "horizontal") {
    const element = document.querySelector<HTMLElement>(
      ".os-data-grid .os-grid-scroll",
    );
    if (!element) throw new Error("Native grid scroll element missing");
    element.scrollTop = 0;
    element.scrollLeft = 0;
    await paint();
    return recordFrames((elapsed) => {
      if (axis === "vertical") element.scrollTop = elapsed * 1.5;
      else
        element.scrollLeft =
          ((element.scrollWidth - element.clientWidth) * elapsed) / 10000;
    });
  },
  stop: () => crawl.stop(),
};
declare global {
  interface Window {
    perfShell: {
      ticks: { at: number; phase?: string }[];
      clicks: number[];
      open(path: string): ReturnType<typeof crawl.open>;
      completeSidebars(): Promise<void>;
      importFixture(path: string): Promise<{
        result: unknown;
        progress: { done: number; total?: number | null; indicator: boolean }[];
      }>;
      state(): ReturnType<typeof useSession.getState>["crawl"];
      start(base: string): Promise<void>;
      scrollFrames(
        axis: "vertical" | "horizontal",
      ): ReturnType<typeof recordFrames>;
      stop(): Promise<void>;
    };
  }
}
