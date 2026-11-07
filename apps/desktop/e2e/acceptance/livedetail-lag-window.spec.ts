// acceptance: independent
// Fixture-Gegenprüfungen der in live/detail integrierten Lastmessung.
// Soll ausschließlich docs/spec/05-ui.md.
import { test, expect, INTERVAL, RATE, lagWindow, startLoadFixture } from "./livedetail-support";

// spec: UI-018, UI-032, UI-145
// Rechenprobe gegen die tatsächlich verwendete Fixture-Größe, auch für machine=8/slow=16.
test("lag fixture preserves 3 URL/s and leaves ten recorder beats after the bound", async () => {
  for (const machine of [1, 4, 8]) for (const bound of [1000, 3000]) for (const initial of [0, 4]) {
    const plan = lagWindow(machine, bound, initial);
    expect(plan.limit - initial).toBe(plan.remaining);
    expect(plan.remaining / RATE * 1000 - bound * plan.maxFactor).toBeGreaterThanOrEqual(plan.reserveMs);
    expect(plan.reserveMs / (INTERVAL * plan.maxFactor)).toBeGreaterThanOrEqual(10);
    expect(plan.remaining / RATE).toBeGreaterThanOrEqual(60);
  }
});

// spec: UI-018, UI-032
test("extended fixture has exactly one backlink per content page and a held final request", async () => {
  const gate = await startLoadFixture(3);
  try {
    gate.allowTo(4);
    const home = await (await fetch(gate.startUrl)).text();
    expect([...home.matchAll(/href="\/page-\d+\.html"/g)]).toHaveLength(3);
    expect(home).toContain('href="/hold.html"');
    for (let n = 1; n <= 3; n++) {
      const page = await (await fetch(`${gate.base}/page-${n}.html`)).text();
      expect([...page.matchAll(/href="\/"/g)]).toHaveLength(1);
    }
    expect(gate.admitted()).toBe(4);
    expect(gate.budget()).toBe(4);
  } finally { await gate.stop(); }
});

