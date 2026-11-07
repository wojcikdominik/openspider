// Unabhängige Abnahme B4-LIVEDETAIL, Teil 2: Detailfenster Inlinks/Outlinks im laufenden Crawl (UI-030 bis UI-033, UI-145, UI-018).
// Fixture fx-ui-basic: Startseite „/“ verlinkt page-1 … page-20 (und weitere), jede page-N verlinkt nur zurück auf „/“. Die Inlinks
// der Startseite wachsen deshalb mit jeder gecrawlten Seite; die Inlinks von page-N sind genau eine Zeile (From „/“).
// acceptance: independent
import type { Alpha } from "../alpha/fixture";
import {
  TALL,
  upperGrid,
  parseFooter,
  crawlMore,
  crawlTo,
  expect,
  test,
  type Gate,
  clickUpperRow,
  coreDetailTotal,
  detailGridLocator,
  installRecorder,
  RATE,
  INTERVAL,
  lagWindow,
  startLoadFixture,
  machineFactor,
  mark,
  openDetailTab,
  pauseCrawl,
  pressUpper,
  readDetail,
  recorderFactor,
  resumeCrawl,
  selectedUpper,
  sortByKeyboard,
  startGated,
  stopRecorder,
  urlIdOf,
  urlIdsOf,
  waitCrawled,
  type Sample,
} from "./livedetail-support";



/** Der sichtbare Inhalt des Detailfensters zeigt Daten einer anderen URL als erwartet. */
interface Switch {
  firstNewMs: number | null;
  staleAfterNew: string[];
}
function analyse(samples: Sample[], markT: number, endT: number, name: string, expected: (d: NonNullable<Sample["detail"]>) => boolean, wrong: (d: NonNullable<Sample["detail"]>) => boolean): Switch {
  const after = samples.filter((s) => s.t >= markT && s.t < endT);
  const first = after.find((s) => s.detail?.name === name && expected(s.detail));
  if (!first) return { firstNewMs: null, staleAfterNew: [] };
  const staleAfterNew = after
    .filter((s) => s.t > first.t && s.detail?.name === name && wrong(s.detail))
    .map((s) => `+${Math.round(s.t - markT)}ms rowcount=${s.detail!.rowcount} first=${s.detail!.rows[0] ?? ""}`);
  return { firstNewMs: first.t - markT, staleAfterNew };
}

async function selectHomeWithInlinks(alpha: Alpha, gate: Gate, minCrawled: number): Promise<{ home: string; base: string }> {
  const { page } = alpha;
  await page.setViewportSize(TALL);
  await startGated(alpha, gate);
  await crawlTo(page, gate, minCrawled);
  await clickUpperRow(page, gate.startUrl);
  await openDetailTab(page, "Inlinks");
  await expect.poll(async () => (await readDetail(page))?.rowcount ?? 0, { timeout: 10_000 }).toBeGreaterThan(2);
  return { home: gate.startUrl, base: gate.base };
}

// spec: UI-032
// spec: UI-018
// spec: UI-145
// acceptance: independent
test("Inlinks of the selected URL grow at 3 URL/s with bounded startup and final observation", async ({ alpha }) => {
    const kind = "inlinks" as "counters" | "inlinks";
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
      expect.soft(lag, `${kind}: Lag-Grenze während Anlauf, Last und ereignisfreiem Schlussfenster`).toEqual([]);
      expect.soft(chipLag, "aktiver Chip hält dieselbe Lag-Grenze ein").toEqual([]);
      const final = running.at(-1)!;
      expect(value(final), "letzter Stand wird ohne Pause exakt").toBe(plan.limit - (kind === "inlinks" ? 1 : 0));

      // Pause/Fortsetzen und Endstand bleiben Bestandteil der ursprünglichen Abnahme.
      await pauseCrawl(page, gate);
      const homeId = await urlIdOf(page, gate.startUrl);
      await expect.poll(async () => ((await readDetail(page))?.rowcount ?? 0) - 1, { timeout: 10_000 }).toBe(await coreDetailTotal(page, "inlinks", homeId));
      const paused = await readDetail(page);
      for (const row of paused!.rows) {
        expect(row.cells[2], "To jeder Inlink-Zeile ist die ausgewählte Startseite").toBe(gate.startUrl);
        expect(row.cells[1]).toMatch(/\/page-\d+\.html$/);
      }
      await resumeCrawl(page);
      gate.open();
      await alpha.waitCompleted(60_000);
      await expect.poll(async () => ((await readDetail(page))?.rowcount ?? 0) - 1, { timeout: 10_000 }).toBe(plan.limit);
      const froms = (await readDetail(page))!.rows.map((r) => r.cells[1]);
      expect(new Set(froms).size, "keine doppelten sichtbaren Inlinks").toBe(froms.length);
    } finally {
      await gate.stop();
    }
});

// spec: UI-032
// spec: UI-033
// spec: UI-145
// spec: UI-142
// acceptance: independent
test("switching the selection shows the new URL's Inlinks and Outlinks and never rows of the previous URL", async ({ alpha, gate }) => {
  test.setTimeout(120_000);
  const { page } = alpha;
  const { home, base } = await selectHomeWithInlinks(alpha, gate, 9);
  const homeOutlinks = await (async () => {
    await openDetailTab(page, "Outlinks");
    await expect.poll(async () => (await readDetail(page))?.rowcount ?? 0, { timeout: 10_000 }).toBeGreaterThan(20);
    return (await readDetail(page))!.rowcount;
  })();
  expect(homeOutlinks, "Outlinks der Startseite: 20 Seiten + /redirect, /missing, /noindex + 2 externe = 25 Zeilen").toBe(26);
  await openDetailTab(page, "Inlinks");
  await expect.poll(async () => (await readDetail(page))?.name).toBe("Inlinks");
  const target1 = `${base}/page-1.html`;
  const target3 = `${base}/page-3.html`;

  const slow0 = await machineFactor(page);
  await installRecorder(page, 50);
  // Der Crawl läuft während der Wechsel weiter (3 URLs je Sekunde bis zur 20. URL, /redirect – die 22. – bleibt aus), damit Änderungen der Ansichten dazwischenfallen.
  gate.meter(3, 20);
  // Start → page-1 (eine Zeile tiefer)
  const m1 = await mark(page, "to-page-1");
  await pressUpper(page, "ArrowDown");
  await expect.poll(() => selectedUpper(page)).toEqual([target1]);
  await expect.poll(async () => (await readDetail(page))?.rowcount, { timeout: 15_000 }).toBe(2);
  await page.waitForTimeout(1500);
  // page-1 → Start zurück
  const m2 = await mark(page, "back-to-home");
  await pressUpper(page, "ArrowUp");
  await expect.poll(() => selectedUpper(page)).toEqual([home]);
  await expect.poll(async () => (await readDetail(page))?.rowcount ?? 0, { timeout: 15_000 }).toBeGreaterThan(8);
  await page.waitForTimeout(1500);
  // Start → page-3 (drei Zeilen tiefer, synchroner Tastatur-Stimulus)
  await upperGrid(page).focus();
  const m3 = await mark(page, "to-page-3");
  // spec: UI-012, UI-145 — echte Key-Handler ohne Timer, IPC oder Zwischenassertions.
  const stimulusMs = await page.evaluate(async () => {
    const grid = document.activeElement!;
    const start = performance.now();
    for (let i = 0; i < 3; i++) {
      grid.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown", code: "ArrowDown", bubbles: true }));
      grid.dispatchEvent(new KeyboardEvent("keyup", { key: "ArrowDown", code: "ArrowDown", bubbles: true }));
      // React übernimmt den diskreten Tastendruck vor dem nächsten Ereignis.
      await Promise.resolve();
    }
    return performance.now() - start;
  });
  await test.info().attach("arrow-stimulus", { body: JSON.stringify({ stimulusMs, count: 3 }), contentType: "application/json" });
  expect(stimulusMs, "Testvorbedingung: nur der synchrone Stimulus, ohne Fokus/IPC/Warten").toBeLessThan(800);
  await expect.poll(() => selectedUpper(page)).toEqual([target3]);
  await expect.poll(async () => (await readDetail(page))?.rowcount, { timeout: 15_000 }).toBe(2);
  await page.waitForTimeout(1500);
  const factors = await recorderFactor(page, 50, slow0);
  const slow = factors.slow;
  const samples = await stopRecorder(page);

  {
    const timeline: string[] = [];
    let last = "";
    for (const s of samples) {
      const key = s.detail ? `${s.detail.name}:${s.detail.rowcount}:${s.detail.rows[0]?.split("|")[2]?.replace(base, "") ?? ""}` : "none";
      if (key !== last) timeline.push(`${Math.round(s.t - m1)}ms ${s.label ?? ""} ${key} sel=${s.upperSelected.map((x) => x.replace(base, "")).join(",")}`);
      last = key;
    }
    console.log("[livedetail] timeline\n" + timeline.join("\n"));
  }
  // Inlinks einer Seite: jede Zeile hat die Seite als Ziel (To). Die Startseite verlinkt page-N; /redirect leitet auf page-1 weiter,
  // wodurch page-1 später eine zweite Inlink-Zeile bekommt – das ist neuer Inhalt derselben URL, keine veralteten Zeilen.
  const onlyTo = (address: string) => (d: NonNullable<Sample["detail"]>) => d.rows.length > 0 && d.rows.every((r) => r.split("|")[2] === address);
  const toPage = (address: string) => (d: NonNullable<Sample["detail"]>) => onlyTo(address)(d) && d.rowcount >= 2 && d.rowcount <= 3 && d.rows.some((r) => r.split("|")[1] === home);
  const notTo = (address: string) => (d: NonNullable<Sample["detail"]>) => d.rows.some((r) => r.split("|")[2] !== address);
  const a = analyse(samples, m1, m2, "Inlinks", toPage(target1), notTo(target1));
  const b = analyse(samples, m2, m3, "Inlinks", (d) => d.rowcount > 8 && onlyTo(home)(d), notTo(home));
  const c = analyse(samples, m3, Number.POSITIVE_INFINITY, "Inlinks", toPage(target3), notTo(target3));
  console.log(`[livedetail] switch first-new-data ms: page-1=${a.firstNewMs?.toFixed(0)} home=${b.firstNewMs?.toFixed(0)} page-3=${c.firstNewMs?.toFixed(0)}`);
  for (const [label, r] of [["page-1", a], ["home", b], ["page-3", c]] as const) {
    expect(r.firstNewMs, `${label}: Inlinks der neuen Auswahl erscheinen`).not.toBeNull();
    // Die Spec nennt Zeitbudgets nur für den Release-Build (UI-145: 150 ms; Spec-Fragen S1/S2). Für den Dev-Server gilt hier eine grobe
    // Obergrenze von 3 s mal Rechnerfaktor (max. Faktor aus Rechnertakt vor dem Lauf und gekapptem Takt während des Laufs): Ein Rückschritt
    // von 0,3 s auf 10 s fällt damit auch auf einem ruhigen Rechner auf. Den Median misst `livedetail-budget.spec.ts` (nächtlich).
    expect(r.firstNewMs!, `${label}: Inlinks der neuen Auswahl erscheinen spätestens nach 3 s mal Rechnerfaktor ${slow.toFixed(1)} (Rechnertakt ${slow0.toFixed(1)}, Takt im Lauf roh ${factors.raw.toFixed(1)} / gekappt ${factors.capped.toFixed(1)})`).toBeLessThan(3000 * slow);
    expect(r.staleAfterNew, `${label}: nach dem Erscheinen der neuen Zeilen kehren Zeilen der vorigen URL nicht zurück`).toEqual([]);
  }

  // Outlinks der gewählten Seite: genau die Rückverlinkung auf die Startseite.
  await openDetailTab(page, "Outlinks");
  await expect.poll(async () => {
    const d = await readDetail(page);
    return d ? `${d.name}:${d.rowcount}:${d.rows[0]?.cells[2]}` : "";
  }, { timeout: 15_000 }).toBe(`Outlinks:2:${home}`);
  // URL Details nennt die Fokus-URL (UI-031)
  const details = await openDetailTab(page, "URL Details");
  await expect(details).toContainText(target3);
  await expect(details).not.toContainText(target1);
});

// spec: UI-145
// spec: UI-012
// acceptance: independent
test("holding an arrow key leads to few detail requests and ends on the focused URL", async ({ alpha, gate }) => {
  test.setTimeout(120_000);
  const { page } = alpha;
  const { home, base } = await selectHomeWithInlinks(alpha, gate, 12);
  const homeId = await urlIdOf(page, home);
  await page.evaluate(async () => {
    const { getClient } = await import(/* @vite-ignore */ String("/src/ipc/client.ts"));
    const client = getClient();
    const original = client.invoke.bind(client);
    const calls: { cmd: string; ids: number[] | undefined; kind: string | undefined; t: number }[] = [];
    (window as unknown as { __ldCalls: typeof calls }).__ldCalls = calls;
    client.invoke = (cmd: string, args?: { spec?: { kind?: string; urlIds?: number[] } }, signal?: AbortSignal) => {
      calls.push({ cmd, ids: args?.spec?.urlIds, kind: args?.spec?.kind, t: performance.now() });
      return original(cmd, args, signal);
    };
  });
  gate.meter(3, 20);
  // Schnelle Auswahlwechsel (Pfeiltaste gedrückt halten): acht Wechsel der Fokuszeile unmittelbar hintereinander im selben Takt des
  // Seitenkontexts – unabhängig von der Geschwindigkeit des Rechners liegen sie damit innerhalb der Entprellung von 100 ms (UI-145).
  // Die Auswahl wird wie von der oberen Tabelle über die Übergabeschnittstelle des Detailfensters veröffentlicht.
  const ids = await urlIdsOf(page, Array.from({ length: 8 }, (_, i) => `${base}/page-${i + 1}.html`));
  await page.evaluate(async (ids) => {
    const { setDetailSelection } = await import(/* @vite-ignore */ String("/src/features/detail/frame/selection.ts"));
    for (const id of ids) setDetailSelection({ urlIds: [id], focus: id, topTab: "Response Codes" });
  }, ids);
  const target = `${base}/page-8.html`;
  await expect.poll(async () => {
    const d = await readDetail(page);
    return d ? `${d.rowcount}:${d.rows[0]?.cells[2]}` : "";
  }, { timeout: 15_000 }).toBe(`2:${target}`);
  const calls = await page.evaluate(() => (window as unknown as { __ldCalls: { cmd: string; ids?: number[]; kind?: string }[] }).__ldCalls);
  const inlinkOpens = calls.filter((c) => c.cmd === "view.open" && c.kind === "inlinks" && c.ids && c.ids[0] !== homeId);
  console.log(`[livedetail] inlinks view.open during 8 quick selection changes: ${inlinkOpens.length}`);
  expect(inlinkOpens.length, "schnelle Auswahlwechsel werden entprellt: weniger Detailabfragen als Wechsel").toBeLessThan(8);
  // Anzeige = Inlinks der Fokuszeile
  const d = await readDetail(page);
  expect(d!.rows[0]!.cells[2]).toBe(target);
});

// spec: UI-145
// spec: UI-030
// spec: UI-018
// acceptance: independent
test("a detail tab that was hidden while the crawl ran shows the current data when it is shown again", async ({ alpha, gate }) => {
  test.setTimeout(120_000);
  const { page } = alpha;
  const { home } = await selectHomeWithInlinks(alpha, gate, 4);
  const homeId = await urlIdOf(page, home);
  const before = (await readDetail(page))!.rowcount - 1;
  await openDetailTab(page, "Outlinks");
  await expect.poll(async () => (await readDetail(page))?.name).toBe("Outlinks");
  // Während „Inlinks“ verborgen ist, kommen sieben URLs dazu (das Tor lässt genau sieben durch).
  await crawlMore(page, gate, 7);
  await openDetailTab(page, "Inlinks");
  // Nach dem Zurückwechseln zeigt das Detailfenster den aktuellen Stand, nicht den der letzten Sichtbarkeit.
  await expect.poll(async () => ((await readDetail(page))?.rowcount ?? 0) - 1, { timeout: 15_000 }).toBeGreaterThanOrEqual(before + 5);
  await pauseCrawl(page, gate);
  await expect
    .poll(async () => ((await readDetail(page))?.rowcount ?? 0) - 1, { timeout: 10_000, intervals: [250] })
    .toBe(await coreDetailTotal(page, "inlinks", homeId));
});

// spec: UI-010
// spec: UI-032
// spec: UI-018
// acceptance: independent
test("a sorted Inlinks table keeps its order while links arrive and reports the correct total", async ({ alpha, gate }) => {
  test.setTimeout(180_000);
  const { page } = alpha;
  const { home } = await selectHomeWithInlinks(alpha, gate, 4);
  const homeId = await urlIdOf(page, home);
  // Sortieren per Tastatur: Kopfzeile, Spalte „From“ (Type, From, To, …), Enter (aufsteigend)
  const grid = detailGridLocator(page);
  await sortByKeyboard(page, grid, 1);
  await expect.poll(async () => (await readDetail(page))?.sort[1], { timeout: 15_000 }).toBe("ascending");
  await installRecorder(page, 100);
  gate.meter(3, 18);
  await waitCrawled(page, 18);
  const samples = (await stopRecorder(page)).filter((s) => s.detail?.name === "Inlinks" && s.detail.rows.length > 1);
  expect(samples.length).toBeGreaterThan(5);
  const unsorted: string[] = [];
  for (const s of samples) {
    const froms = s.detail!.rows.map((r) => r.split("|")[1]!);
    const sorted = [...froms].sort();
    if (JSON.stringify(froms) !== JSON.stringify(sorted)) unsorted.push(`rowcount=${s.detail!.rowcount} ${froms.slice(0, 4).join(",")}`);
  }
  expect(unsorted, "sichtbare Zeilen bleiben nach From aufsteigend sortiert, während neue Links eintreffen").toEqual([]);
  const counts = samples.map((s) => s.detail!.rowcount);
  expect(new Set(counts).size, "Gesamtzahl wächst während der Beobachtung").toBeGreaterThanOrEqual(3);

  // zweiter Klick: absteigend; die größte Adresse (page-9.html) steht oben
  await sortByKeyboard(page, detailGridLocator(page), 1);
  await expect.poll(async () => (await readDetail(page))?.sort[1], { timeout: 15_000 }).toBe("descending");
  await expect.poll(async () => (await readDetail(page))?.rows[0]?.cells[1], { timeout: 15_000 }).toMatch(/\/page-9\.html$/);
  await pauseCrawl(page, gate);
  await expect
    .poll(async () => ((await readDetail(page))?.rowcount ?? 0) - 1, { timeout: 10_000, intervals: [250] })
    .toBe(await coreDetailTotal(page, "inlinks", homeId));
});

// spec: UI-033
// spec: UI-018
// acceptance: independent
test("Outlinks of the start page stay complete and correct while the crawl runs", async ({ alpha, gate }) => {
  test.setTimeout(120_000);
  const { page } = alpha;
  const { home, base } = await selectHomeWithInlinks(alpha, gate, 4);
  await openDetailTab(page, "Outlinks");
  await installRecorder(page, 100);
  gate.meter(3, 14);
  await waitCrawled(page, 14);
  const samples = (await stopRecorder(page)).filter((s) => s.detail?.name === "Outlinks" && s.detail.rowcount > 1);
  expect(samples.length).toBeGreaterThan(5);
  // Die Startseite ist die erste gecrawlte URL: ihre 25 Outlinks stehen von Anfang an vollständig da und bleiben es.
  for (const s of samples) expect(s.detail!.rowcount - 1, `Outlinks der Startseite bei +${Math.round(s.t - samples[0]!.t)}ms`).toBe(25);
  const d = (await readDetail(page))!;
  for (const row of d.rows) expect(row.cells[1], "From jeder Outlink-Zeile = ausgewählte URL").toBe(home);
  expect(d.rows.some((r) => r.cells[2] === `${base}/page-1.html`)).toBe(true);
});

// spec: UI-030
// spec: UI-012
// spec: UI-018
// acceptance: independent
test("a multi-row selection keeps its rows while rows arrive and Inlinks show the union", async ({ alpha, gate }) => {
  test.setTimeout(120_000);
  const { page } = alpha;
  const { base } = await selectHomeWithInlinks(alpha, gate, 5);
  // Klick auf page-2, Umschalt+Klick auf page-3 (UI-012: Mehrfachauswahl per Shift+Klick)
  await clickUpperRow(page, `${base}/page-2.html`);
  // (Im laufenden Crawl geht ein Umschalt+Klick häufig verloren – eigener Befund, siehe den Test „every Shift+click …“. Hier wird
  // der Crawl für die Auswahl angehalten, damit dieser Test die Vereinigung der Inlinks und ihre Stabilität prüft.)
  await pauseCrawl(page, gate);
  await clickUpperRow(page, `${base}/page-3.html`, ["Shift"]);
  await expect.poll(() => selectedUpper(page)).toEqual([`${base}/page-2.html`, `${base}/page-3.html`]);
  await resumeCrawl(page);
  await expect(page.locator("#data-panel .u10-counter")).toContainText("2 selected");
  // Vereinigung der Inlinks: je ein Link von „/“ auf page-2 und page-3
  await expect.poll(async () => {
    const d = await readDetail(page);
    return d ? JSON.stringify([d.rowcount, d.rows.map((r) => r.cells[2]).sort()]) : "";
  }, { timeout: 15_000 }).toBe(JSON.stringify([3, [`${base}/page-2.html`, `${base}/page-3.html`]]));
  // Weiter crawlen: Markierung und Detailansicht bleiben stehen
  await crawlMore(page, gate, 4);
  expect(await selectedUpper(page)).toEqual([`${base}/page-2.html`, `${base}/page-3.html`]);
  await expect(page.locator("#data-panel .u10-counter")).toContainText("2 selected");
  const d = await readDetail(page);
  expect(d!.rowcount).toBe(3);
});

// spec: UI-032
// spec: UI-018
// spec: UI-142
// acceptance: independent
test("a link discovered later (redirect target) appears in the Inlinks of an already selected URL", async ({ alpha, gate }) => {
  test.setTimeout(120_000);
  const { page } = alpha;
  const { home, base } = await selectHomeWithInlinks(alpha, gate, 4);
  const target = `${base}/page-1.html`;
  await pressUpper(page, "ArrowDown");
  await expect.poll(() => selectedUpper(page)).toEqual([target]);
  await expect.poll(async () => (await readDetail(page))?.rowcount, { timeout: 15_000 }).toBe(2);
  // /redirect (HTTP-Weiterleitung auf page-1) wird nach den 20 Seiten gecrawlt; seine Kante ist ein weiterer Inlink von page-1.
  await crawlTo(page, gate, 24);
  await expect
    .poll(async () => {
      const d = await readDetail(page);
      return d ? JSON.stringify([d.rowcount, d.rows.map((r) => r.cells[1].replace(base, "")).sort()]) : "";
    }, { timeout: 15_000, intervals: [250] })
    .toBe(JSON.stringify([3, ["/", "/redirect"]]));
  void home;
});

// spec: UI-012
// spec: UI-018
// acceptance: independent
test("every Shift+click extends the selection while the crawl runs", async ({ alpha, gate }) => {
  test.setTimeout(120_000);
  const { page } = alpha;
  const { base } = await selectHomeWithInlinks(alpha, gate, 7);
  await clickUpperRow(page, `${base}/page-2.html`);
  // Der Crawl liefert während der Klicks laufend Änderungen (2 URLs je Sekunde bis zur 24. URL, dazu eine URL unmittelbar vor jedem Klick).
  gate.meter(2, 24);
  const lost: string[] = [];
  for (let k = 3; k <= 8; k++) {
    // Umschalt+Klick auf page-k: markiert page-2 … page-k (UI-012: Mehrfachauswahl per Shift+Klick)
    gate.allow(1);
    await clickUpperRow(page, `${base}/page-${k}.html`, ["Shift"]);
    const expected = Array.from({ length: k - 1 }, (_, i) => `${base}/page-${i + 2}.html`);
    await expect.poll(() => selectedUpper(page), { timeout: 15_000, intervals: [100] }).toEqual(expected).catch(() => undefined);
    const got = await selectedUpper(page);
    if (JSON.stringify(got) !== JSON.stringify(expected)) lost.push(`Shift+Klick auf page-${k}: ${got.length} statt ${expected.length} Zeilen markiert`);
  }
  expect(lost, "Umschalt+Klicks im laufenden Crawl werden nicht verworfen (UI-012, UI-018: Auswahl bleibt stabil)").toEqual([]);
});
