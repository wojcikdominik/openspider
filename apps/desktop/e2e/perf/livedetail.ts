// Messrezept B4-LIVEDETAIL (M2-N32; UI-145, UI-143): Zeitaufteilung beim Öffnen der Detail-Tabs Inlinks/Outlinks während
// eines Live-Crawls. Läuft in der Hauptoberfläche des Perf-Shells (`e2e/perf/shell.html`): im Browser über die E2E-Bridge
// (`livedetail.measure.ts`) und im nativen Release-Build über WebDriver (`e2e-b/perf/livedetail.mjs`). Nur Diagnose; ändert
// kein Produktverhalten. Sprachunabhängig: Tabs über `data-tab-id`, Tabellen über ihre Container.
import { getClient } from "../../src/ipc/client";
import { config } from "../../src/ipc/config";
import { crawl } from "../../src/ipc/crawl";
import { useSession } from "../../src/state/session";

export interface LiveDetailOptions {
  /** Start-URL des Fixture-Servers (synthetische Site). */
  base: string;
  threads?: number;
  /** URLs je Sekunde; `null` = ungedrosselt. */
  rate?: number | null;
  /** Gecrawlte URLs vor der ersten Messung. */
  warmup?: number;
  /** Messrunden; jede wählt eine neue Zeile der oberen Tabelle (frische Detail-Views). */
  rounds?: number;
  /** Dauer der Leerlaufmessung (nur Live-Aktualisierungen) in ms. */
  idleMs?: number;
}
interface Call { cmd: string; kind?: string; start: number; end?: number; bytes?: number; error?: string }
export interface TabSample {
  tab: string; round: number;
  /** Klick bis Bildschirm (erste Zeile mit der Start-URL sichtbar, nach dem nächsten Frame). */
  totalMs: number;
  /** Wartezeit des Klick-Tasks hinter anderen Tasks des Hauptthreads (Eingabeverzögerung). */
  inputDelayMs: number;
  /** Klick-Handler einschließlich React-Render und -Commit des Tabwechsels (React rendert diskrete Ereignisse in einem
   * Microtask direkt nach dem Ereignis). */
  clickHandlerMs: number;
  /** Ende des Klick-Handlers bis `view.open` abgeschickt (Effekte, Planung). */
  toOpenMs?: number;
  /** Rundreise `view.open` (Kern + Transport + JSON). */
  openMs?: number;
  /** Rundreisen `view.rows` vor der ersten sichtbaren Zeile. */
  rowsMs?: number;
  /** `view.open`-Antwort bis erste Zeile im DOM (React-Render TablePanel/DataGrid). */
  renderMs?: number;
  /** Erste Zeile im DOM bis nächster Frame gemalt. */
  paintMs: number;
  openBytes?: number;
  /** Tab war für diese Auswahl schon geladen (kein `view.open`). */
  retained: boolean;
}
export interface LagReport { durationMs: number; busyMs: number; busyPct: number; maxGapMs: number; p95GapMs: number; gapsOver50: number; longTasks?: number[] }

const now = () => performance.now();
const sleep = (ms: number) => new Promise<void>(resolve => setTimeout(resolve, ms));
const frame = () => new Promise<void>(resolve => requestAnimationFrame(() => setTimeout(resolve, 0)));
/** Einen Task posten (wie ein Eingabeereignis, das hinter laufenden Tasks wartet). */
const task = () => new Promise<number>(resolve => { const channel = new MessageChannel(); channel.port1.onmessage = () => resolve(now()); channel.port2.postMessage(0); });

const calls: Call[] = [];
let instrumented = false;
function instrument() {
  if (instrumented) return;
  instrumented = true;
  const client = getClient(); const invoke = client.invoke.bind(client);
  client.invoke = async <T,>(cmd: string, args?: unknown, signal?: AbortSignal): Promise<T> => {
    if (!cmd.startsWith("view.")) return invoke(cmd, args, signal);
    const spec = (args as { spec?: { kind?: string } } | undefined)?.spec;
    const call: Call = { cmd, kind: spec?.kind, start: now() }; calls.push(call);
    if (calls.length > 5000) calls.splice(0, 1000);
    try {
      const result = await invoke<T>(cmd, args, signal);
      call.end = now(); call.bytes = JSON.stringify(result ?? null).length;
      return result;
    } catch (error) { call.end = now(); call.error = String(error); throw error; }
  };
}

/** Belegung des Hauptthreads: ein 10-ms-Timer misst, wie lange er über seine Frist hinaus warten musste. */
export function lagSampler() {
  const gaps: number[] = []; let stopped = false; let timer: ReturnType<typeof setTimeout>;
  const longTasks: number[] = [];
  const observer = typeof PerformanceObserver !== "undefined" && PerformanceObserver.supportedEntryTypes?.includes("longtask")
    ? new PerformanceObserver(list => { for (const e of list.getEntries()) longTasks.push(Math.round(e.duration)); }) : undefined;
  observer?.observe({ type: "longtask", buffered: false });
  const started = now();
  const schedule = () => { const expected = now() + 10; timer = setTimeout(() => tick(expected), 10); };
  const tick = (expected: number) => {
    if (stopped) return;
    gaps.push(Math.max(0, now() - expected));
    schedule();
  };
  schedule();
  return (): LagReport => {
    stopped = true; clearTimeout(timer); observer?.disconnect();
    const durationMs = now() - started;
    const busyMs = gaps.reduce((n, g) => n + (g > 4 ? g : 0), 0);
    const ordered = [...gaps].sort((a, b) => a - b);
    return { durationMs: Math.round(durationMs), busyMs: Math.round(busyMs), busyPct: Math.round(busyMs / durationMs * 1000) / 10,
      maxGapMs: Math.round(ordered.at(-1) ?? 0), p95GapMs: Math.round(ordered[Math.floor(ordered.length * 0.95)] ?? 0),
      gapsOver50: gaps.filter(g => g > 50).length, ...(observer ? { longTasks } : {}) };
  };
}

function detailTab(id: string): HTMLElement {
  const tab = document.querySelector<HTMLElement>(`section.os-detail [role="tab"][data-tab-id="${id}"]`);
  if (!tab) throw new Error(`detail tab ${id} missing`);
  return tab;
}
/** Sichtbare Detailtabelle (zurückgehaltene Tabs liegen in `[hidden]`-Containern). */
function visibleDetailGrid(): HTMLElement | undefined {
  return [...document.querySelectorAll<HTMLElement>("#detail-panel .os-data-grid")].find(grid => !grid.closest("[hidden]"));
}
function upperGrid(): HTMLElement {
  const grid = document.querySelector<HTMLElement>("#data-panel .os-data-grid");
  if (!grid) throw new Error("upper table missing");
  return grid;
}
async function until<T>(probe: () => T | undefined | false, timeoutMs: number, what: string): Promise<T> {
  const deadline = now() + timeoutMs;
  for (;;) {
    const value = probe();
    if (value) return value;
    if (now() > deadline) throw new Error(`timeout: ${what}`);
    await new Promise(resolve => requestAnimationFrame(resolve));
  }
}
function key(target: HTMLElement, name: string) {
  target.dispatchEvent(new KeyboardEvent("keydown", { key: name, bubbles: true, cancelable: true }));
}

async function measureTab(id: string, round: number, base: string): Promise<TabSample> {
  const from = calls.length;
  const tab = detailTab(id);
  const t0 = now();
  const t1 = await task();
  tab.click();
  // React's sync-lane render was queued as a microtask during the click; this continuation runs after it.
  await Promise.resolve();
  const t2 = now();
  const grid = await until(() => { const g = visibleDetailGrid(); return g && tab.getAttribute("aria-selected") === "true" && g.textContent?.includes(base) ? g : undefined; }, 10_000, `${id} rows`);
  const t3 = now(); void grid;
  await frame();
  const t4 = now();
  // Window for trace analysis (blink.user_timing).
  performance.measure(`livedetail:${id}${calls.slice(from).some(c => c.cmd === "view.open") ? "" : "-retained"}:${round}`, { start: t0, end: t4 });
  const mine = calls.slice(from).filter(c => c.start >= t0 && c.start <= t3);
  const open = mine.find(c => c.cmd === "view.open" && (c.kind === "inlinks" || c.kind === "outlinks"));
  const rows = mine.filter(c => c.cmd === "view.rows" && c.end !== undefined && c.end <= t3);
  const rowsEnd = Math.max(open?.end ?? 0, ...rows.map(c => c.end!));
  return {
    tab: id, round, totalMs: t4 - t0, inputDelayMs: t1 - t0, clickHandlerMs: t2 - t1,
    ...(open ? { toOpenMs: open.start - t2, openMs: (open.end ?? t3) - open.start, openBytes: open.bytes,
      rowsMs: rows.reduce((n, c) => n + (c.end! - c.start), 0), renderMs: t3 - rowsEnd } : {}),
    paintMs: t4 - t3, retained: !open,
  };
}

/** Kompletter Ablauf: Crawl starten, Leerlauf messen, dann je Runde neue Zeile und Inlinks → Outlinks → Inlinks. */
export async function liveDetail(options: LiveDetailOptions) {
  const { base, threads = 4, rate = 150, warmup = 300, rounds = 5, idleMs = 5000 } = options;
  instrument();
  const dto = await config.get();
  const root = dto.config as Record<string, Record<string, unknown>>;
  await config.set({ ...dto, config: { ...root, speed: { ...root.speed, threads, max_urls_per_sec: rate }, analysis: { ...root.analysis, auto_at_end: false } } });
  await crawl.start({ source: { kind: "spider", url: base }, scope: "domain" });
  await until(() => (useSession.getState().crawl?.crawled ?? 0) >= warmup, 120_000, "warm-up")
    .catch(error => { throw new Error(`${String(error)}; crawl ${JSON.stringify(useSession.getState().crawl)}; visibility ${document.visibilityState}`); });
  const grid = await until(() => { const g = upperGrid(); return g.textContent?.includes(base) ? g : undefined; }, 10_000, "upper rows");
  grid.focus(); key(grid, "Home");
  detailTab("urlDetails").click();
  await sleep(1000);
  const idleStart = now(); const idle = lagSampler(); await sleep(idleMs); const idleLag = idle();
  performance.measure("livedetail:idle", { start: idleStart, end: now() });
  const samples: TabSample[] = [];
  const during = lagSampler();
  for (let round = 0; round < rounds; round++) {
    if (round > 0) { detailTab("urlDetails").click(); upperGrid().focus(); key(upperGrid(), "ArrowDown"); }
    await sleep(800);
    for (const id of ["inlinks", "outlinks", "inlinks"]) {
      samples.push(await measureTab(id, round, base));
      await sleep(400);
    }
  }
  const measuringLag = during();
  const state = useSession.getState().crawl;
  await crawl.stop().catch(() => undefined);
  return { userAgent: navigator.userAgent, transport: getClient().transportName, options: { threads, rate, warmup, rounds, idleMs },
    crawled: state?.crawled, phase: state?.phase, idleLag, measuringLag, samples, summary: summarize(samples) };
}

export function summarize(samples: TabSample[]) {
  const fresh = samples.filter(s => !s.retained);
  const pick = (values: (number | undefined)[]) => {
    const v = values.filter((x): x is number => x !== undefined).sort((a, b) => a - b);
    return v.length ? { median: Math.round(v[Math.floor(v.length / 2)] * 10) / 10, max: Math.round(v[v.length - 1] * 10) / 10, n: v.length } : undefined;
  };
  const fields = ["totalMs", "inputDelayMs", "clickHandlerMs", "toOpenMs", "openMs", "rowsMs", "renderMs", "paintMs", "openBytes"] as const;
  const of = (list: TabSample[]) => Object.fromEntries(fields.map(f => [f, pick(list.map(s => s[f]))]));
  return { fresh: of(fresh), inlinks: of(fresh.filter(s => s.tab === "inlinks")), outlinks: of(fresh.filter(s => s.tab === "outlinks")), retained: of(samples.filter(s => s.retained)) };
}

declare global { interface Window { liveDetail: typeof liveDetail } }
window.liveDetail = liveDetail;
