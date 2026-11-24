// Abnahme fix-sel (unabhängiger Testagent, nur aus Anforderung und Spec): `client.invoke` bleibt für alle Befehle
// unverändert (Rückgabewert, Fehlerweitergabe, AbortSignal); nur `crawl.start`, `crawl.open`, `crawl.clear` gelten als
// Beginn neuen Inhalts in diesem Fenster; ein Fehler in einem Beobachter des Beginns darf den Aufruf nicht scheitern lassen.
import { afterEach, beforeEach, expect, it } from "vitest";
import commands from "../../ipc/commands.json";
import { client, createClient, setClient } from "../../ipc/client";
import { crawlBegin } from "../../ipc/crawlBegin";
import { IpcError, localError } from "../../ipc/errors";
import { MockCore } from "../../ipc/transport/mock";

const BEGINS = ["crawl.start", "crawl.open", "crawl.clear"];
let core: MockCore;
beforeEach(() => {
  core = new MockCore();
  for (const { name } of commands as { name: string }[]) core.handlers.set(name, args => ({ echo: name, args }));
  setClient(createClient(core.transport()));
  crawlBegin.reset();
});
afterEach(() => { setClient(null); crawlBegin.reset(); });

// spec: UI-018
// spec: UI-142
it("returns every non-begin command's answer unchanged and does not note a begin for it", async () => {
  let notified = 0;
  const off = crawlBegin.subscribe(() => { notified++; });
  try {
    for (const { name } of commands as { name: string }[]) {
      if (BEGINS.includes(name)) continue;
      const before = crawlBegin.get();
      await expect(client.invoke(name, { probe: name }), name).resolves.toEqual({ echo: name, args: { probe: name } });
      expect(crawlBegin.get(), name).toBe(before);
    }
    expect(notified).toBe(0);
    expect(crawlBegin.get()).toEqual({ revision: 0, crawlId: undefined });
  } finally { off(); }
});

// spec: UI-018
it("returns the answers of crawl.start, crawl.open and crawl.clear unchanged and notes their crawl id", async () => {
  core.handlers.set("crawl.start", () => ({ crawlId: "crawl-b", file: "/b.ospider" }));
  core.handlers.set("crawl.open", () => ({ crawlId: "crawl-c", file: "/c.ospider", name: "c" }));
  core.handlers.set("crawl.clear", () => null);
  await expect(client.invoke("crawl.start", { x: 1 })).resolves.toEqual({ crawlId: "crawl-b", file: "/b.ospider" });
  expect(crawlBegin.get()).toEqual({ revision: 1, crawlId: "crawl-b" });
  await expect(client.invoke("crawl.open", { kind: "id", crawlId: "crawl-c" })).resolves.toEqual({ crawlId: "crawl-c", file: "/c.ospider", name: "c" });
  expect(crawlBegin.get()).toEqual({ revision: 2, crawlId: "crawl-c" });
  await expect(client.invoke("crawl.clear")).resolves.toBeNull();
  expect(crawlBegin.get()).toEqual({ revision: 3, crawlId: null });
  expect(core.calls.map(c => c.cmd)).toEqual(["crawl.start", "crawl.open", "crawl.clear"]);
  expect(core.calls[0]!.args).toEqual({ x: 1 });
});

// spec: UI-018
it("passes errors through unchanged for ordinary and begin commands; a failed begin keeps no crawl id", async () => {
  for (const name of ["view.rows", "crawl.recrawl", "crawl.start", "crawl.open", "crawl.clear"]) {
    core.handlers.set(name, () => { throw new IpcError(localError("Io", "err-probe", { name })); });
    const error = await client.invoke(name, {}).then(() => undefined, (e: unknown) => e);
    expect(error, name).toBeInstanceOf(IpcError);
    expect((error as IpcError).cmdError, name).toMatchObject({ code: "Io", messageKey: "err-probe", args: { name } });
  }
  expect(crawlBegin.get()).toEqual({ revision: 3, crawlId: undefined });
});

// spec: UI-018
it("honours the AbortSignal as before: an aborted call is cancelled, a long-running operation is cancelled in the core", async () => {
  const aborted = new AbortController(); aborted.abort();
  for (const name of ["view.rows", "crawl.start"]) {
    const error = await client.invoke(name, {}, aborted.signal).then(() => undefined, (e: unknown) => e);
    expect((error as IpcError).cmdError.code, name).toBe("Cancelled");
  }
  // Nothing reached the core for the aborted calls.
  expect(core.calls).toEqual([]);
  core.handlers.set("crawl.recrawl", () => ({ opId: "op-7", labelKey: "x", cancellable: true }));
  core.handlers.set("op.cancel", () => undefined);
  const later = new AbortController();
  await expect(client.invoke("crawl.recrawl", { selection: { kind: "ids", ids: [1] } }, later.signal)).resolves.toMatchObject({ opId: "op-7" });
  later.abort();
  await new Promise(resolve => setTimeout(resolve, 0));
  expect(core.calls.filter(c => c.cmd === "op.cancel").map(c => c.args)).toEqual([{ opId: "op-7" }]);
});

// spec: UI-018
it("a throwing begin listener does not make crawl.start fail and does not keep the request from the core", async () => {
  core.handlers.set("crawl.start", () => ({ crawlId: "crawl-b", file: "/b.ospider" }));
  const off = crawlBegin.subscribe(() => { if (crawlBegin.get().crawlId === undefined) throw new Error("listener failure"); });
  let result: unknown; let error: unknown;
  try { result = await client.invoke("crawl.start", { probe: true }); } catch (e) { error = e; } finally { off(); }
  expect(error).toBeUndefined();
  expect(result).toEqual({ crawlId: "crawl-b", file: "/b.ospider" });
  expect(core.calls.map(c => c.cmd)).toEqual(["crawl.start"]);
});
