// Unabhängige Abnahme B4-LIVEDETAIL, Teil 1: Live-Aktualisierung von Tabellen, Zählern, Overview, Filterzahlen und Seitenleiste
// während eines laufenden Crawls gegen die echte Bridge (UI-018). Erwartetes Verhalten stammt aus docs/spec/05-ui.md.
// acceptance: independent
import { startAlphaBridge } from "../alpha/support";
import { startGate } from "./livedetail-gate";
import {
  TALL,
  readDetail,
  openDetailTab,
  clickUpperRow,
  activeChipCount,
  crawlMore,
  crawlTo,
  expect,
  coreTotal,
  footer,
  installRecorder,
  RATE,
  INTERVAL,
  lagWindow,
  startLoadFixture,
  machineFactor,
  mark,
  openSidebarTab,
  overviewFingerprint,
  overviewSummary,
  parseFooter,
  recorderFactor,
  pauseCrawl,
  readUpper,
  resumeCrawl,
  startGated,
  test,
  gatedProfile,
  stopRecorder,
  summaryValue,
  upperGrid,
  waitCrawled,
  type Sample,
} from "./livedetail-support";

// spec: UI-018
// spec: UI-006
// spec: UI-060
// acceptance: independent
test("table footer, filter chip and Overview grow at 3 URL/s with bounded startup and final observation", async ({ alpha }) => {
    const kind = "counters" as "counters" | "inlinks";
    test.setTimeout(180_000);
    const { page } = alpha;
    await page.setViewportSize(TALL);
    await alpha.open();
    const machine = await machineFactor(page);
    const boundMs = kind === "counters" ? 1000 : 3000;
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
      // UI-018: harte Grenze 1 s für Tabellenzähler, ohne Dev- oder Rechnerzuschlag.
      const limitMs = kind === "counters" ? boundMs : boundMs * factors.slow;
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
      const values = running.map(value);
      expect(new Set(values).size, "mindestens fünf sichtbare Zwischenstände").toBeGreaterThanOrEqual(5);
      for (let i = 1; i < values.length; i++) expect(values[i]!, "Anzeige fällt nie zurück").toBeGreaterThanOrEqual(values[i - 1]!);
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

      // Pause/Fortsetzen und Endstand bleiben Bestandteil der ursprünglichen Abnahme.
      await pauseCrawl(page, gate);
      const overview = running.map((s) => summaryValue(s.overview, /^Total URLs Crawled/)).filter((n) => n >= 0);
      expect(new Set(overview).size, "Overview wächst mehrfach ohne Nutzeraktion").toBeGreaterThanOrEqual(3);
      for (let i = 1; i < overview.length; i++) expect(overview[i]!).toBeGreaterThanOrEqual(overview[i - 1]!);
      const total = await coreTotal(page, "Response Codes", "All");
      await expect.poll(async () => JSON.stringify({ footer: await footer(page), chip: await activeChipCount(page), overview: (await overviewSummary(page))["Total URLs Crawled"] }), { timeout: 12_000 }).toBe(JSON.stringify({ footer: { shown: total, total, selected: 0 }, chip: total, overview: total }));
      await resumeCrawl(page);
      gate.open();
      await alpha.waitCompleted(60_000);
      const endTotal = plan.limit + 2; // Root, plan.limit Rückverlinker, Hold.
      await expect.poll(async () => JSON.stringify({ footer: await footer(page), chip: await activeChipCount(page), overview: (await overviewSummary(page))["Total URLs Crawled"] }), { timeout: 12_000 }).toBe(JSON.stringify({ footer: { shown: endTotal, total: endTotal, selected: 0 }, chip: endTotal, overview: endTotal }));
      await alpha.table().getByRole("button", { name: /Filter/, exact: true }).click();
      await expect(page.getByRole("option", { name: new RegExp(`^Success \\(2XX\\)\\s*${endTotal}$`) })).toBeVisible();
      await page.keyboard.press("Escape");
      await expect(page.locator("[data-overview-age]")).toHaveCount(0, { timeout: 12_000 });
      expect((await readUpper(page))?.rowcount).toBe(endTotal + 1);
      const endOverview = await overviewSummary(page);
      expect(endOverview["Total Internal URLs"]).toBe(endTotal);
      expect(endOverview["Total External URLs"]).toBe(0);
    } finally {
      await gate.stop();
    }
});

// spec: UI-018
// spec: UI-061
// spec: UI-062
// acceptance: independent
test("after pause, resume and completion Overview and Issues equal an uninterrupted crawl", async ({ alpha, gate }) => {
  test.setTimeout(180_000);
  const { page } = alpha;
  // Lauf A: ununterbrochen (Tor offen, nur der Endstand zählt)
  await startGated(alpha, gate, "Internal");
  gate.open();
  await alpha.waitCompleted(60_000);
  await openSidebarTab(page, "Issues");
  await expect(page.getByRole("tabpanel", { name: "Issues" })).toContainText("Client Error (4XX)", { timeout: 15_000 });
  const issuesA = await issueRows(page);
  await openSidebarTab(page, "Overview");
  const overviewA = await settledFingerprint(page);

  // Lauf B in einer zweiten App-Instanz: Pause nach der Hälfte, Fortsetzen, Abschluss (UI-018 Kriterium 3)
  const second = await startAlphaBridge();
  const gateB = await startGate(alpha.site.base);
  const pageB = await page.context().newPage();
  alpha.track(pageB);
  try {
    await pageB.goto(`/?${new URLSearchParams({ bridge: second.url })}`);
    await expect(pageB.locator("[data-shell-ready]")).toHaveAttribute("data-shell-ready", "true");
    await gatedProfile(pageB);
    const field = pageB.getByLabel("Start URL");
    await field.fill(gateB.startUrl);
    await field.press("Enter");
    // die Hälfte der Fixture-Seiten, dann Pause: das Tor hält den Crawl, bis der Test ihn weiterlaufen lässt
    await crawlTo(pageB, gateB, 13);
    await pauseCrawl(pageB, gateB);
    await expect
      .poll(async () => (await overviewSummary(pageB))["Total URLs Crawled"], { timeout: 12_000, intervals: [250] })
      .toBe(await coreTotal(pageB, "Response Codes", "All"));
    expect((await overviewSummary(pageB))["Total URLs Crawled"], "nach der Pause zeigt die Overview weniger als den Endstand").toBeLessThan(27);
    await resumeCrawl(pageB);
    gateB.open();
    await expect(pageB.getByTestId("crawl-phase")).toHaveText("Completed", { timeout: 60_000 });
    await openSidebarTab(pageB, "Issues");
    await expect(pageB.getByRole("tabpanel", { name: "Issues" })).toContainText("Client Error (4XX)", { timeout: 15_000 });
    const issuesB = await issueRows(pageB);
    await openSidebarTab(pageB, "Overview");
    const overviewB = await settledFingerprint(pageB);
    expect(overviewB, "Overview nach Pause/Fortsetzen = Overview des ununterbrochenen Crawls").toEqual(overviewA);
    expect(issuesB, "Issues nach Pause/Fortsetzen = Issues des ununterbrochenen Crawls").toEqual(issuesA);
  } finally {
    await pageB.close();
    await gateB.stop();
    await second.stop();
  }
});

// spec: UI-018
// spec: UI-060
// spec: UI-061
// spec: UI-004
// acceptance: independent
test("the Overview dashboard follows the running crawl without user action and shows exact numbers after pause and at the end", async ({ alpha, gate }) => {
  test.setTimeout(120_000);
  const { page } = alpha;
  await startGated(alpha, gate, "Internal");
  // erst wenn der Crawl läuft: ein neu gestarteter Crawl setzt die Ansicht auf die Tabellen zurück
  await crawlTo(page, gate, 1);
  await page.locator(".u34-nav-item[data-group=overview] .u34-nav-button").click();
  const dashboard = page.locator("[data-dashboard]");
  await expect(dashboard).toBeVisible();
  const cards = () =>
    dashboard.evaluate((el) => Object.fromEntries([...el.querySelectorAll(".os-dashboard-card")].filter((c) => c.querySelector("strong") && c.querySelector("span")).map((c) => [c.querySelector("span")!.textContent!.trim(), Number((c.querySelector("strong")!.textContent ?? "").replace(/[^\d]/g, ""))])));
  const crawledKey = async () => Object.keys(await cards()).find((k) => /crawled/i.test(k)) ?? "";
  const slow0 = await machineFactor(page);
  const series: number[] = [];
  const cardCrawled = async () => {
    const all = await cards();
    const key = Object.keys(all).find((k) => /crawled/i.test(k));
    const value = key ? all[key]! : -1;
    if (value >= 0) series.push(value);
    return value;
  };
  // Etappen wie bei der Tabelle: nach jeder Etappe hält das Tor den Crawl, bis die (gedrosselte) Übersicht den Stand zeigt
  for (const stage of [7, 14, 20]) {
    gate.meter(3, stage);
    await expect.poll(cardCrawled, { timeout: 15_000 * slow0, intervals: [250], message: `Karte „gecrawlt“ holt den Stand ${stage} ein` }).toBeGreaterThanOrEqual(stage);
  }
  console.log(`[livedetail] dashboard crawled series distinct=${[...new Set(series)].join(",")}`);
  expect(new Set(series).size, "Karte „gecrawlt“ ändert sich während des Laufs").toBeGreaterThanOrEqual(3);
  for (let i = 1; i < series.length; i++) expect(series[i]!, `Karte fällt zurück bei Stichprobe ${i}`).toBeGreaterThanOrEqual(series[i - 1]!);
  await pauseCrawl(page, gate);
  const truth = await coreTotal(page, "Response Codes", "All");
  await expect.poll(async () => (await cards())[await crawledKey()], { timeout: 12_000, intervals: [250] }).toBe(truth);
  await resumeCrawl(page);
  gate.open();
  await alpha.waitCompleted(60_000);
  await expect
    .poll(async () => {
      const all = await cards();
      return JSON.stringify([all["Total URLs Encountered"], all["Total URLs Crawled"], all["Remaining URLs"]]);
    }, { timeout: 12_000, intervals: [300] })
    .toBe(JSON.stringify([27, 27, 0]));
  const finalCards = await cards();
  console.log(`[livedetail] dashboard cards at the end: ${JSON.stringify(finalCards)}`);
  const values = Object.values(finalCards);
  expect(values.filter((v) => v === 27).length, "gefunden und gecrawlt = 27").toBeGreaterThanOrEqual(2);
  // Statusverteilung (Response Codes): 25 × 2XX, 1 × 3XX, 1 × 4XX
  await expect(dashboard.locator(".os-dashboard-row", { hasText: /^Success \(2XX\)/ }).first()).toContainText("25", { timeout: 12_000 });
  await expect(dashboard.locator(".os-dashboard-row", { hasText: /^Client Error \(4XX\)/ }).first()).toContainText("1");
  // wichtigste Probleme: der Client-Error-Filter steht dort
  await expect(dashboard).toContainText("Client Error (4XX)");
});

// spec: UI-018
// spec: UI-062
// acceptance: independent
test("the sidebar Issues list fills during the crawl without user action and is exact at the end", async ({ alpha, gate }) => {
  test.setTimeout(120_000);
  const { page } = alpha;
  await startGated(alpha, gate, "Internal");
  await openSidebarTab(page, "Issues");
  const panel = page.getByRole("tabpanel", { name: "Issues" });
  // der 404-Fund (/missing) entsteht erst nach den 20 Seiten: vorher gibt es das Problem nicht
  await crawlTo(page, gate, 8);
  await expect(panel).not.toContainText("Client Error (4XX)");
  gate.open();
  await alpha.waitCompleted(60_000);
  // … nachher erscheint es ohne weiteres Zutun, mit genau einer URL (expected.json: clientErrors = 1)
  await expect(panel).toContainText("Client Error (4XX)", { timeout: 15_000 });
  const row = panel.locator("tr, [role=row]").filter({ hasText: /Client Error \(4XX\)/ }).first();
  await expect.poll(async () => (await row.locator("td").allInnerTexts()).map((c) => c.trim())[2] ?? "", { timeout: 10_000 }).toBe("1");
});

// spec: UI-018
// spec: UI-140
// acceptance: independent
test("an unsorted table scrolled to the bottom follows new rows (tail mode) until the user scrolls up", async ({ alpha, gate }) => {
  test.setTimeout(120_000);
  const { page } = alpha;
  await startGated(alpha, gate);
  await crawlTo(page, gate, 8);
  const grid = upperGrid(page);
  const atBottom = async () => {
    const g = await readUpper(page);
    if (!g || !g.rows.length) return null;
    const last = Math.max(...g.rows.map((r) => r.index));
    return { total: g.rowcount - 1, last: last - 1, first: Math.min(...g.rows.map((r) => r.index)) - 2, hidden: g.rowcount - 1 - (last - 1) };
  };
  await grid.hover({ position: { x: 160, y: 80 } });
  await page.mouse.wheel(0, 4000);
  await expect.poll(async () => (await atBottom())?.hidden, { timeout: 15_000 }).toBe(0);
  // Folgt neuen Zeilen: Der Test lässt zehnmal je eine URL durch (das Tor, nicht die Uhr, bestimmt den Takt); nach jeder neuen Zeile
  // ist die letzte sichtbare Zeile wieder die letzte der Tabelle.
  const follow: string[] = [];
  const totals = new Set<number>();
  for (let i = 0; i < 10; i++) {
    const before = (await atBottom())!.total;
    gate.allow(1);
    await expect.poll(async () => (await atBottom())?.total ?? 0, { timeout: 15_000, intervals: [100] }).toBeGreaterThan(before);
    let state = await atBottom();
    await expect.poll(async () => ((state = await atBottom())?.hidden ?? 99) <= 1, { timeout: 15_000, intervals: [100] }).toBe(true).catch(() => undefined);
    totals.add(state!.total);
    if (state!.hidden > 1) follow.push(`Schritt ${i}: total=${state!.total} last=${state!.last}`);
  }
  console.log(`[livedetail] tail mode totals=${[...totals].join(",")} misses=${follow.length}`);
  expect(totals.size, "die Tabelle wuchs während der Beobachtung").toBeGreaterThanOrEqual(4);
  expect(follow, "Tail-Modus: die Tabelle bleibt am Ende, wenn neue Zeilen eintreffen").toEqual([]);

  // manuelles Hochscrollen beendet den Tail-Modus (UI-018): Die Ansicht springt nach oben und bleibt dort, auch wenn weitere Zeilen eintreffen.
  // Es gibt keine Wiederholung des Scrollens: Bleibt das Hochscrollen wirkungslos (Beobachtung F5, tritt unter Last auf), ist der Test rot.
  // Der Verlauf der ersten sichtbaren Zeile steht in der Meldung.
  await page.mouse.wheel(0, -3000);
  const trail: string[] = [];
  for (let i = 0; i < 25; i++) {
    const state = await atBottom();
    trail.push(`${state?.first}/${state?.total}`);
    if (state?.first === 0) break;
    await page.waitForTimeout(200);
  }
  console.log(`[livedetail] tail mode scroll-up trail: ${trail.join(" ")}`);
  await test.info().attach("tail-scroll-trail", { body: trail.join(" "), contentType: "text/plain" });
  expect((await atBottom())?.first, `erste sichtbare Zeile nach dem Hochscrollen (Verlauf je 200 ms: ${trail.join(" ")})`).toBe(0);
  // zwei weitere URLs: die Zeilenzahl wächst, die Ansicht bleibt oben
  const totalBefore = (await atBottom())!.total;
  await crawlMore(page, gate, 2);
  await expect.poll(async () => (await atBottom())?.total ?? 0, { timeout: 15_000 }).toBeGreaterThan(totalBefore);
  const after = (await atBottom())!;
  expect(after.first, "nach dem Hochscrollen folgt die Tabelle den neuen Zeilen nicht mehr").toBe(0);
});

async function issueRows(page: import("@playwright/test").Page): Promise<string[]> {
  return page.getByRole("tabpanel", { name: "Issues" }).evaluate((panel) =>
    [...panel.querySelectorAll("tbody tr, [role=row]")].map((r) => (r.textContent ?? "").replace(/\s+/g, " ").trim()).filter(Boolean).sort());
}

/** Overview, bis sich zwei aufeinanderfolgende Lesungen nicht mehr unterscheiden (exakter Stand ohne Nutzeraktion). */
async function settledFingerprint(page: import("@playwright/test").Page): Promise<string[]> {
  let previous = "";
  let current: string[] = [];
  await expect
    .poll(async () => {
      current = await overviewFingerprint(page);
      const text = JSON.stringify(current);
      const same = text === previous && current.length > 0;
      previous = text;
      return same;
    }, { timeout: 15_000, intervals: [600] })
    .toBe(true);
  return current;
}
