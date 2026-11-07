// Ergänzt die unveränderten, als acceptance: independent gesperrten Altprüfungen.
// Soll ausschließlich docs/spec/05-ui.md; Messspielraum wie bisher 2 s Zähler / 3 s Inlinks.
import { test, expect, TALL, clickUpperRow, crawlTo, installRecorder, machineFactor, mark,
  openDetailTab, parseFooter, readDetail, recorderFactor, startGated, stopRecorder,
  waitCrawled, type Sample } from "./livedetail-support";
import { INTERVAL, RATE, lagWindow, startLoadFixture } from "./livedetail-load-fixture";

// spec: UI-018, UI-032, UI-145
// Rechenprobe gegen die tatsächlich verwendete Fixture-Größe, auch für machine=8/slow=16.
test("lag fixture preserves 3 URL/s and leaves ten recorder beats after the bound", async () => {
  for (const machine of [1, 4, 8]) for (const bound of [2000, 3000]) for (const initial of [0, 4]) {
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

for (const kind of ["counters", "inlinks"] as const) {
  // spec: UI-018, UI-032, UI-126, UI-145
  test(`${kind}: bounded startup and sustained 3 URL/s including an event-free final lag window`, async ({ alpha }) => {
    test.setTimeout(180_000);
    const { page } = alpha;
    await page.setViewportSize(TALL);
    await alpha.open();
    const machine = await machineFactor(page);
    const boundMs = kind === "counters" ? 2000 : 3000;
    const initial = kind === "counters" ? 0 : 4;
    const plan = lagWindow(machine, boundMs, initial);
    // Eine weitere URL wartet hinter der Messgrenze: der Crawl bleibt auch im Schlussfenster Running.
    const gate = await startLoadFixture(plan.limit);
    try {
      await startGated(alpha, gate);
      if (initial) {
        await crawlTo(page, gate, initial);
        await clickUpperRow(page, gate.startUrl);
        await openDetailTab(page, "Inlinks");
        await expect.poll(async () => (await readDetail(page))?.rowcount ?? 0).toBe(initial);
      }
      await installRecorder(page, INTERVAL);
      const start = await mark(page, "fixed-measurement-start");
      gate.meter(RATE, plan.limit);
      await waitCrawled(page, plan.limit, 120_000);
      gate.stopMeter();
      const finalStart = await mark(page, "last-url-no-further-event");
      // Keine Pause, Auswahl, Tab-Änderung oder weitere URL. F6 darf nicht durch Pause repariert werden.
      await page.waitForTimeout(boundMs * plan.maxFactor + plan.reserveMs);
      const factors = await recorderFactor(page, INTERVAL, machine);
      const beats = await page.evaluate(() => (window as unknown as { __ld: { beats: number[] } }).__ld.beats);
      const samples = await stopRecorder(page);
      const running = samples.filter((s) => s.t >= start && s.phase === "running");
      const limitMs = boundMs * factors.slow;
      const value = (s: Sample) => kind === "counters" ? parseFooter(s.footer).shown : (s.detail?.rowcount ?? 0) - 1;
      const target = (s: Sample) => s.crawled - (kind === "inlinks" ? 1 : 0);
      const chip = (s: Sample) => Number(/\(([\d.,]+)\)/.exec(s.chip)?.[1]?.replace(/[^\d]/g, "") ?? -1);
      const first = running.find((s) => s.crawled > initial);
      const startup = first && running.find((s) => s.t >= first.t + limitMs);
      const lag: object[] = [];
      const chipLag: object[] = [];
      let checked = 0;
      let flowingChecked = 0;
      let finalChecked = 0;
      for (const s of running) {
        if (s.t < start + limitMs) continue;
        const reference = [...running].reverse().find((r) => r.t <= s.t - limitMs);
        if (!reference) continue;
        checked++;
        if (s.t < finalStart) flowingChecked++;
        if (s.t >= finalStart + limitMs) finalChecked++;
        if (value(s) < target(reference)) lag.push({ t: s.t - start, actual: value(s), expected: target(reference) });
        if (kind === "counters" && chip(s) < reference.crawled) chipLag.push({ t: s.t - start, actual: chip(s), expected: reference.crawled });
      }
      const flowingBeats = beats.filter((t) => t >= start + limitMs && t < finalStart).length;
      const finalBeats = beats.filter((t) => t >= finalStart + limitMs).length;
      // Diagnose VOR jeder inhaltlichen Assertion, einschließlich Anlauf und Stichprobenwächtern.
      await test.info().attach("live-lag", { body: JSON.stringify({ kind, rate: RATE, initial, plan, start, finalStart,
        machineFactor: machine, recorderFactorRaw: factors.raw, recorderFactorCapped: factors.capped,
        slow: factors.slow, limitMs, checked, flowingChecked, finalChecked, flowingBeats, finalBeats, beats, first, startup, lag, chipLag, samples }), contentType: "application/json" });
      expect(first, "erste URL nach festem Messbeginn aufgezeichnet").toBeDefined();
      expect(startup, "eigene, feste Anlaufgrenze ab erster URL statt erstem Einholen").toBeDefined();
      expect(value(startup!), "Anlauf: erste URL innerhalb der Lag-Grenze angezeigt").toBeGreaterThanOrEqual(target(first!));
      if (kind === "counters") expect(chip(startup!), "Anlauf: aktiver Chip").toBeGreaterThanOrEqual(first!.crawled);
      expect(flowingBeats, "echte Recorder-Takte während des Zuflusses").toBeGreaterThanOrEqual(5);
      expect(finalBeats, "echte Recorder-Takte im ereignisfreien Schlussfenster").toBeGreaterThanOrEqual(5);
      expect(flowingChecked, "Lag-Stichproben noch während des Zuflusses").toBeGreaterThanOrEqual(5);
      expect(finalChecked, "Lag-Stichproben nach letzter URL ohne weiteres Ereignis").toBeGreaterThanOrEqual(5);
      expect(lag, `${kind}: Lag-Grenze während Anlauf, Last und ereignisfreiem Schlussfenster`).toEqual([]);
      expect(chipLag, "aktiver Chip hält dieselbe Lag-Grenze ein").toEqual([]);
      const final = running.at(-1)!;
      expect(value(final), "letzter Stand wird ohne Pause exakt").toBe(plan.limit - (kind === "inlinks" ? 1 : 0));
    } finally {
      await gate.stop();
    }
  });
}
