import { MockCore } from "../../../ipc/transport/mock";
import type { CrawlListing } from "../../../ipc/gen/CrawlListing";
import type { OpHandle } from "../../../ipc/gen/OpHandle";
import type { CrawlDesktop } from "../desktop";
import data from "./management.json";
export const fixtureRows: CrawlListing[] = data.rows.map((r, i) => ({
  crawl: { crawlId: r.id, name: r.name, file: `/fixture/${r.name}.ospider`, phase: r.phase as CrawlListing["crawl"]["phase"], mode: r.name === "list.csv" ? "list" : "spider",
    startedAt: `2026-10-${String(i + 1).padStart(2, "0")}T12:00:00Z`, urls: i + 1, fileBytes: r.size, schemaVersion: 3, owner: { kind: "app", live: r.phase === "running", controllable: true } },
  project: r.project, locked: r.locked, startUrl: r.startUrl, durationMs: 1000 * (i + 1), canResume: r.canResume, endReason: (r as { endReason?: string }).endReason ?? null,
}));
export function managementFixture() {
  const core = new MockCore();
  let rows = structuredClone(fixtureRows);
  let ps = structuredClone(data.projects);
  let nextOp = 0;
  let lastHandle: OpHandle | null = null;
  const operation = (name: string) => (lastHandle = { opId: `fixture-${++nextOp}`, labelKey: name, cancellable: name !== "repair" });
  core.handlers.set("crawls.list", () => structuredClone(rows));
  core.handlers.set("projects.list", () => structuredClone(ps));
  core.handlers.set("crawls.rename", args => { const a = args as { crawlId: string; name: string }; rows.find(r => r.crawl.crawlId === a.crawlId)!.crawl.name = a.name; });
  core.handlers.set("projects.create", args => { const p = { id: `project-${ps.length}`, name: (args as { name: string }).name, locked: false, crawls: 0 }; ps.push(p); return p; });
  core.handlers.set("projects.rename", args => { const a = args as { id: string; name: string }; const p = ps.find(p => p.id === a.id)!; for (const row of rows) if (row.project === p.name) row.project = a.name; p.name = a.name; });
  core.handlers.set("projects.lock", args => { const a = args as { id: string; locked: boolean }; ps.find(p => p.id === a.id)!.locked = a.locked; });
  for (const name of ["lock", "unlock"]) core.handlers.set(`crawls.${name}`, args => { for (const row of rows) if ((args as { ids: string[] }).ids.includes(row.crawl.crawlId)) row.locked = name === "lock"; });
  for (const name of ["move", "delete", "duplicate", "import", "export"]) core.handlers.set(`crawls.${name}`, () => operation(name));
  for (const name of ["crawls.copyIds", "crawls.reveal", "crawl.pause", "crawl.resume", "op.cancel", "config.set"]) core.handlers.set(name, () => undefined);
  core.handlers.set("crawl.open", args => { const a = args as { crawlId?: string; path?: string }; return rows.find(r => r.crawl.crawlId === a.crawlId || r.crawl.file === a.path)?.crawl ?? rows[0]!.crawl; });
  core.handlers.set("crawl.saveAs", () => operation("save"));
  let destination: string | null = "/fixture/output.ospider";
  let paths: string[] | null = ["/fixture/Alpha.ospider"];
  const desktop: CrawlDesktop = {
    chooseOpen: async () => paths, chooseSave: async () => destination, chooseDirectory: async () => destination,
    check: async () => undefined, repair: async () => operation("repair"), configuration: async () => ({ schemaVersion: 2, generator: "fixture", config: { mode: "spider", fixture: true } }),
  };
  return { core, desktop, rows: () => rows, projects: () => ps, handle: () => lastHandle,
    setRows(value: CrawlListing[]) { rows = value; }, setDestination(value: string | null) { destination = value; }, setPaths(value: string[] | null) { paths = value; } };
}
