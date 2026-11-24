// fix-sel (UI-018): Beobachter des Crawl-Beginns dürfen den IPC-Aufruf nie beeinflussen – weder vor der Anfrage noch nach
// der Antwort. Ihr Fehler geht den üblichen Weg der Oberfläche (Ereignis `error`, das installGlobalErrorLogging protokolliert).
import { afterEach, beforeEach, expect, it } from "vitest";
import { client, createClient, setClient } from "./client";
import { crawlBegin } from "./crawlBegin";
import { IpcError, localError } from "./errors";
import { MockCore } from "./transport/mock";

let core: MockCore;
let reported: string[];
const onError = (event: ErrorEvent) => { reported.push(event.message); };
beforeEach(() => {
  core = new MockCore(); reported = [];
  setClient(createClient(core.transport()));
  crawlBegin.reset();
  window.addEventListener("error", onError);
});
afterEach(() => { window.removeEventListener("error", onError); setClient(null); crawlBegin.reset(); });

// spec: UI-018
it("a listener that throws before the request and after the answer leaves crawl.start untouched and is reported", async () => {
  core.handlers.set("crawl.start", () => ({ crawlId: "crawl-b", file: "/b" }));
  const failures = [new Error("before"), new Error("after")];
  let calls = 0;
  const off = crawlBegin.subscribe(() => { throw failures[calls++] ?? new Error("more"); });
  const rejections: unknown[] = [];
  const onRejection = (event: PromiseRejectionEvent) => { rejections.push(event.reason); };
  window.addEventListener("unhandledrejection", onRejection);
  try {
    await expect(client.invoke("crawl.start", { probe: true })).resolves.toEqual({ crawlId: "crawl-b", file: "/b" });
    await new Promise(resolve => setTimeout(resolve, 0));
  } finally { off(); window.removeEventListener("unhandledrejection", onRejection); }
  expect(core.calls.map(c => c.cmd)).toEqual(["crawl.start"]);
  expect(crawlBegin.get()).toEqual({ revision: 1, crawlId: "crawl-b" });
  expect(crawlBegin.settled()).toBe(true);
  expect(reported).toEqual(["crawl begin listener: before", "crawl begin listener: after"]);
  expect(rejections).toEqual([]);
});

// spec: UI-018
it("a failed begin is settled and notified once; its error reaches the caller unchanged", async () => {
  core.handlers.set("crawl.open", () => { throw new IpcError(localError("NotFound", "err-crawl-not-found")); });
  let notified = 0;
  const off = crawlBegin.subscribe(() => { notified++; });
  try {
    const error = await client.invoke("crawl.open", { kind: "id", crawlId: "x" }).then(() => undefined, (e: unknown) => e);
    expect((error as IpcError).cmdError.messageKey).toBe("err-crawl-not-found");
  } finally { off(); }
  // Sent, then failed.
  expect(notified).toBe(2);
  expect(crawlBegin.get()).toEqual({ revision: 1, crawlId: undefined });
  expect(crawlBegin.settled()).toBe(true);
});
