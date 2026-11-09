// B4-LAGFIX2: Ein neu geladenes Fenster meldet seine Sichtbarkeit sofort. Beim Neuladen meldet das alte Dokument zuletzt
// „verdeckt“; ohne eigene Meldung des neuen Dokuments bliebe das Fenster im Kern verdeckt (Ticks ≤ 1 Hz statt im
// Live-Intervall), und Tabellenfuß und Zähler folgten dem Crawl erst nach bis zu zwei Sekunden.
import { afterEach, describe, expect, it } from "vitest";
import { createClient, setClient } from "./client";
import { bindVisibility } from "./events";
import { MockCore } from "./transport/mock";

afterEach(() => setClient(null));

function fakeDocument(state: DocumentVisibilityState) {
  const target = new EventTarget();
  return Object.assign(target, { visibilityState: state }) as unknown as Document & { visibilityState: DocumentVisibilityState };
}

const reports = (core: MockCore) => core.calls.filter((c) => c.cmd === "events.setVisibility").map((c) => c.args);

describe("bindVisibility", () => {
  // spec: UI-018
  // spec: UI-143
  it("reports the current visibility at once, so a reload does not inherit the old document's hidden report", async () => {
    const core = new MockCore();
    setClient(createClient(core.transport("main")));
    const old = fakeDocument("visible");
    const offOld = bindVisibility("main", old);
    (old as { visibilityState: DocumentVisibilityState }).visibilityState = "hidden";
    old.dispatchEvent(new Event("visibilitychange"));
    offOld();
    const fresh = fakeDocument("visible");
    const off = bindVisibility("main", fresh);
    await expect.poll(() => reports(core)).toEqual([
      { window: "main", visible: true },
      { window: "main", visible: false },
      { window: "main", visible: true },
    ]);
    off();
  });

  // spec: UI-143
  it("a document that loads hidden reports hidden and later changes as before", async () => {
    const core = new MockCore();
    setClient(createClient(core.transport("main")));
    const doc = fakeDocument("hidden");
    const off = bindVisibility("main", doc);
    (doc as { visibilityState: DocumentVisibilityState }).visibilityState = "visible";
    doc.dispatchEvent(new Event("visibilitychange"));
    off();
    doc.dispatchEvent(new Event("visibilitychange"));
    await expect.poll(() => reports(core)).toEqual([
      { window: "main", visible: false },
      { window: "main", visible: true },
    ]);
  });
});
