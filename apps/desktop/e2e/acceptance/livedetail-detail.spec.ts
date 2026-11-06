// Unabhängige Abnahme B4-LIVEDETAIL, Teil 2: Detailfenster Inlinks/Outlinks im laufenden Crawl (UI-030 bis UI-033, UI-145, UI-018).
// Fixture fx-ui-basic: Startseite „/“ verlinkt page-1 … page-20 (und weitere), jede page-N verlinkt nur zurück auf „/“. Die Inlinks
// der Startseite wachsen deshalb mit jeder gecrawlten Seite; die Inlinks von page-N sind genau eine Zeile (From „/“).
// acceptance: independent
import { expect, test, type Alpha } from "../alpha/fixture";
import {
  TALL,
  clickUpperRow,
  coreDetailTotal,
  crawledCount,
  detailGridLocator,
  installRecorder,
  mark,
  openDetailTab,
  pauseCrawl,
  pressUpper,
  readDetail,
  resumeCrawl,
  selectedUpper,
  sortByKeyboard,
  startSlow,
  stopRecorder,
  upperGrid,
  urlIdOf,
  waitCrawled,
  type Sample,
} from "./livedetail-support";

const lastBefore = (samples: Sample[], t: number): number => {
  let value = 0;
  for (const s of samples) {
    if (s.t > t) break;
    value = s.crawled;
  }
  return value;
};

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

async function selectHomeWithInlinks(alpha: Alpha, rate: number, minCrawled: number): Promise<{ home: string; base: string }> {
  const { page } = alpha;
  await page.setViewportSize(TALL);
  await startSlow(alpha, rate);
  await waitCrawled(page, minCrawled);
  await clickUpperRow(page, alpha.startUrl);
  await openDetailTab(page, "Inlinks");
  await expect.poll(async () => (await readDetail(page))?.rowcount ?? 0, { timeout: 10_000 }).toBeGreaterThan(2);
  return { home: alpha.startUrl, base: alpha.site.base };
}

// spec: UI-032
// spec: UI-018
// spec: UI-145
// acceptance: independent
test("Inlinks of the selected URL grow with the running crawl and equal the core after pause and at the end", async ({ alpha }) => {
  test.setTimeout(120_000);
  const { page } = alpha;
  const { home, base } = await selectHomeWithInlinks(alpha, 3, 4);
  const homeId = await urlIdOf(page, home);
  await installRecorder(page, 100);
  await waitCrawled(page, 20);
  await page.getByRole("button", { name: "Pause", exact: true }).click();
  await expect(page.getByTestId("crawl-phase")).toHaveText("Paused");
  const samples = (await stopRecorder(page)).filter((s) => s.phase === "running" && s.detail?.name === "Inlinks");
  expect(samples.length).toBeGreaterThan(20);
  const counts = samples.map((s) => s.detail!.rowcount - 1);
  console.log(`[livedetail] inlinks distinct=${[...new Set(counts)].join(",")}`);

  // Die Zeilenzahl wächst fortlaufend (mindestens fünf verschiedene Stände) und fällt nie zurück.
  expect(new Set(counts).size).toBeGreaterThanOrEqual(5);
  for (let i = 1; i < counts.length; i++) expect(counts[i]!, `Inlinks fällt zurück bei Stichprobe ${i}`).toBeGreaterThanOrEqual(counts[i - 1]!);

  // Solange nur die Startseite und page-1 … page-20 gecrawlt sind, verlinkt jede gecrawlte Seite zurück: Inlinks(Start) = gecrawlt − 1.
  // Nachlauf des Detailfensters höchstens 3 s (Messspielraum Dev-Server; UI-142: „die UI lädt sichtbare Fenster nach“).
  const t0 = samples[0]!.t;
  const lag: string[] = [];
  for (const s of samples) {
    if (s.t - t0 < 3500) continue;
    const reference = lastBefore(samples, s.t - 3000);
    if (reference <= 21 && s.detail!.rowcount - 1 < reference - 1) lag.push(`+${Math.round(s.t - t0)}ms inlinks=${s.detail!.rowcount - 1} < crawled(t-3s)-1=${reference - 1}`);
  }
  expect(lag, "Inlinks der Startseite hinken dem Crawl um mehr als 3 s hinterher").toEqual([]);

  // Nach der Pause: genau die Zahl des Kerns, ohne Nutzeraktion.
  await expect
    .poll(async () => ((await readDetail(page))?.rowcount ?? 0) - 1, { timeout: 10_000, intervals: [250] })
    .toBe(await coreDetailTotal(page, "inlinks", homeId));
  const paused = await readDetail(page);
  for (const row of paused!.rows) {
    expect(row.cells[2], "To jeder Inlink-Zeile ist die ausgewählte Startseite").toBe(home);
    expect(row.cells[1]).toMatch(new RegExp(`^${base.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}/page-\\d+\\.html$`));
  }

  // Fortsetzen und Ende: 20 Inlinks (page-1 … page-20), jede From-Adresse einmal.
  await page.getByRole("button", { name: "Resume", exact: true }).click();
  await alpha.waitCompleted(60_000);
  await expect.poll(async () => ((await readDetail(page))?.rowcount ?? 0) - 1, { timeout: 10_000 }).toBe(20);
  // Sortiert nach From aufsteigend sind die ersten Zeilen page-1, page-10, page-11 … (Bytefolge der URL, UI-010).
  const end = await readDetail(page);
  const froms = end!.rows.map((r) => r.cells[1]);
  expect(new Set(froms).size, "keine doppelten Zeilen im sichtbaren Ausschnitt").toBe(froms.length);
});

// spec: UI-032
// spec: UI-033
// spec: UI-145
// spec: UI-142
// acceptance: independent
test("switching the selection shows the new URL's Inlinks and Outlinks and never rows of the previous URL", async ({ alpha }) => {
  test.setTimeout(120_000);
  const { page } = alpha;
  const { home, base } = await selectHomeWithInlinks(alpha, 3, 9);
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

  await installRecorder(page, 50);
  // Start → page-1 (eine Zeile tiefer)
  const m1 = await mark(page, "to-page-1");
  await pressUpper(page, "ArrowDown");
  await expect.poll(() => selectedUpper(page)).toEqual([target1]);
  await expect.poll(async () => (await readDetail(page))?.rowcount, { timeout: 5000 }).toBe(2);
  await page.waitForTimeout(1500);
  // page-1 → Start zurück
  const m2 = await mark(page, "back-to-home");
  await pressUpper(page, "ArrowUp");
  await expect.poll(() => selectedUpper(page)).toEqual([home]);
  await expect.poll(async () => (await readDetail(page))?.rowcount ?? 0, { timeout: 5000 }).toBeGreaterThan(8);
  await page.waitForTimeout(1500);
  // Start → page-3 (zwei Zeilen tiefer, schnell hintereinander)
  const m3 = await mark(page, "to-page-3");
  await pressUpper(page, "ArrowDown", 3);
  await expect.poll(() => selectedUpper(page)).toEqual([target3]);
  await expect.poll(async () => (await readDetail(page))?.rowcount, { timeout: 5000 }).toBe(2);
  await page.waitForTimeout(1500);
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
    expect(r.firstNewMs!, `${label}: Inlinks der neuen Auswahl erscheinen spätestens nach 3 s (UI-012/UI-145: 150 ms + 150 ms im Release-Build)`).toBeLessThan(3000);
    expect(r.staleAfterNew, `${label}: nach dem Erscheinen der neuen Zeilen kehren Zeilen der vorigen URL nicht zurück`).toEqual([]);
  }

  // Outlinks der gewählten Seite: genau die Rückverlinkung auf die Startseite.
  await openDetailTab(page, "Outlinks");
  await expect.poll(async () => {
    const d = await readDetail(page);
    return d ? `${d.name}:${d.rowcount}:${d.rows[0]?.cells[2]}` : "";
  }, { timeout: 8000 }).toBe(`Outlinks:2:${home}`);
  // URL Details nennt die Fokus-URL (UI-031)
  const details = await openDetailTab(page, "URL Details");
  await expect(details).toContainText(target3);
  await expect(details).not.toContainText(target1);
});

// spec: UI-145
// spec: UI-012
// acceptance: independent
test("holding an arrow key leads to few detail requests and ends on the focused URL", async ({ alpha }) => {
  test.setTimeout(120_000);
  const { page } = alpha;
  const { home, base } = await selectHomeWithInlinks(alpha, 3, 12);
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
  await upperGrid(page).focus();
  // Pfeiltaste gedrückt halten: acht Wechsel im Abstand von 5 ms, im Seitenkontext ausgelöst (die Latenz der Testwerkzeuge
  // soll die Abstände nicht über die Entprellung von 100 ms hinaus dehnen, UI-145)
  const spacing = await page.evaluate(async () => {
    const grid = document.querySelector<HTMLElement>("#data-panel .os-data-grid")!;
    const times: number[] = [];
    for (let i = 0; i < 8; i++) {
      grid.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown", code: "ArrowDown", bubbles: true, cancelable: true }));
      times.push(performance.now());
      await new Promise((r) => setTimeout(r, 5));
    }
    return times[times.length - 1]! - times[0]!;
  });
  expect(spacing, "acht Tastendrücke innerhalb von weniger als 100 ms × 8").toBeLessThan(800);
  const target = `${base}/page-8.html`;
  await expect.poll(() => selectedUpper(page)).toEqual([target]);
  await expect.poll(async () => (await readDetail(page))?.rowcount, { timeout: 5000 }).toBe(2);
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
test("a detail tab that was hidden while the crawl ran shows the current data when it is shown again", async ({ alpha }) => {
  test.setTimeout(120_000);
  const { page } = alpha;
  const { home } = await selectHomeWithInlinks(alpha, 3, 4);
  const homeId = await urlIdOf(page, home);
  const before = (await readDetail(page))!.rowcount - 1;
  await openDetailTab(page, "Outlinks");
  await expect.poll(async () => (await readDetail(page))?.name).toBe("Outlinks");
  const crawledThen = await crawledCount(page);
  await waitCrawled(page, crawledThen + 7);
  await openDetailTab(page, "Inlinks");
  // Nach dem Zurückwechseln zeigt das Detailfenster den aktuellen Stand, nicht den der letzten Sichtbarkeit.
  await expect.poll(async () => ((await readDetail(page))?.rowcount ?? 0) - 1, { timeout: 4000 }).toBeGreaterThanOrEqual(before + 5);
  await page.getByRole("button", { name: "Pause", exact: true }).click();
  await expect(page.getByTestId("crawl-phase")).toHaveText("Paused");
  await expect
    .poll(async () => ((await readDetail(page))?.rowcount ?? 0) - 1, { timeout: 10_000, intervals: [250] })
    .toBe(await coreDetailTotal(page, "inlinks", homeId));
});

// spec: UI-010
// spec: UI-032
// spec: UI-018
// acceptance: independent
test("a sorted Inlinks table keeps its order while links arrive and reports the correct total", async ({ alpha }) => {
  test.setTimeout(120_000);
  const { page } = alpha;
  const { home } = await selectHomeWithInlinks(alpha, 2, 4);
  const homeId = await urlIdOf(page, home);
  // Sortieren per Tastatur: Kopfzeile, Spalte „From“ (Type, From, To, …), Enter (aufsteigend)
  const grid = detailGridLocator(page);
  await sortByKeyboard(page, grid, 1);
  await expect.poll(async () => (await readDetail(page))?.sort[1], { timeout: 5000 }).toBe("ascending");
  await installRecorder(page, 100);
  await waitCrawled(page, 18);
  const samples = (await stopRecorder(page)).filter((s) => s.detail?.name === "Inlinks" && s.detail.rows.length > 1);
  expect(samples.length).toBeGreaterThan(20);
  const unsorted: string[] = [];
  for (const s of samples) {
    const froms = s.detail!.rows.map((r) => r.split("|")[1]!);
    const sorted = [...froms].sort();
    if (JSON.stringify(froms) !== JSON.stringify(sorted)) unsorted.push(`rowcount=${s.detail!.rowcount} ${froms.slice(0, 4).join(",")}`);
  }
  expect(unsorted, "sichtbare Zeilen bleiben nach From aufsteigend sortiert, während neue Links eintreffen").toEqual([]);
  const counts = samples.map((s) => s.detail!.rowcount);
  expect(new Set(counts).size, "Gesamtzahl wächst während der Beobachtung").toBeGreaterThanOrEqual(4);

  // zweiter Klick: absteigend; die größte Adresse (page-9.html) steht oben
  await sortByKeyboard(page, detailGridLocator(page), 1);
  await expect.poll(async () => (await readDetail(page))?.sort[1], { timeout: 5000 }).toBe("descending");
  await expect.poll(async () => (await readDetail(page))?.rows[0]?.cells[1], { timeout: 5000 }).toMatch(/\/page-9\.html$/);
  await page.getByRole("button", { name: "Pause", exact: true }).click();
  await expect(page.getByTestId("crawl-phase")).toHaveText("Paused");
  await expect
    .poll(async () => ((await readDetail(page))?.rowcount ?? 0) - 1, { timeout: 10_000, intervals: [250] })
    .toBe(await coreDetailTotal(page, "inlinks", homeId));
});

// spec: UI-033
// spec: UI-018
// acceptance: independent
test("Outlinks of the start page stay complete and correct while the crawl runs", async ({ alpha }) => {
  test.setTimeout(120_000);
  const { page } = alpha;
  const { home, base } = await selectHomeWithInlinks(alpha, 3, 4);
  await openDetailTab(page, "Outlinks");
  await installRecorder(page, 100);
  await waitCrawled(page, 14);
  const samples = (await stopRecorder(page)).filter((s) => s.detail?.name === "Outlinks" && s.detail.rowcount > 1);
  expect(samples.length).toBeGreaterThan(10);
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
test("a multi-row selection keeps its rows while rows arrive and Inlinks show the union", async ({ alpha }) => {
  test.setTimeout(120_000);
  const { page } = alpha;
  const { base } = await selectHomeWithInlinks(alpha, 2, 5);
  // Klick auf page-2, Umschalt+Klick auf page-3 (UI-012: Mehrfachauswahl per Shift+Klick)
  await clickUpperRow(page, `${base}/page-2.html`);
  // (Im laufenden Crawl geht ein Umschalt+Klick häufig verloren – eigener Befund, siehe den Test „every Shift+click …“. Hier wird
  // der Crawl für die Auswahl angehalten, damit dieser Test die Vereinigung der Inlinks und ihre Stabilität prüft.)
  await pauseCrawl(page);
  await clickUpperRow(page, `${base}/page-3.html`, ["Shift"]);
  await expect.poll(() => selectedUpper(page)).toEqual([`${base}/page-2.html`, `${base}/page-3.html`]);
  await resumeCrawl(page);
  await expect(page.locator("#data-panel .u10-counter")).toContainText("2 selected");
  // Vereinigung der Inlinks: je ein Link von „/“ auf page-2 und page-3
  await expect.poll(async () => {
    const d = await readDetail(page);
    return d ? JSON.stringify([d.rowcount, d.rows.map((r) => r.cells[2]).sort()]) : "";
  }, { timeout: 6000 }).toBe(JSON.stringify([3, [`${base}/page-2.html`, `${base}/page-3.html`]]));
  // Weiter crawlen: Markierung und Detailansicht bleiben stehen
  const before = await crawledCount(page);
  await waitCrawled(page, Math.min(26, before + 4));
  expect(await selectedUpper(page)).toEqual([`${base}/page-2.html`, `${base}/page-3.html`]);
  await expect(page.locator("#data-panel .u10-counter")).toContainText("2 selected");
  const d = await readDetail(page);
  expect(d!.rowcount).toBe(3);
});

// spec: UI-032
// spec: UI-018
// spec: UI-142
// acceptance: independent
test("a link discovered later (redirect target) appears in the Inlinks of an already selected URL", async ({ alpha }) => {
  test.setTimeout(120_000);
  const { page } = alpha;
  const { home, base } = await selectHomeWithInlinks(alpha, 3, 4);
  const target = `${base}/page-1.html`;
  await pressUpper(page, "ArrowDown");
  await expect.poll(() => selectedUpper(page)).toEqual([target]);
  await expect.poll(async () => (await readDetail(page))?.rowcount, { timeout: 5000 }).toBe(2);
  // /redirect (HTTP-Weiterleitung auf page-1) wird nach den 20 Seiten gecrawlt; seine Kante ist ein weiterer Inlink von page-1.
  await waitCrawled(page, 24);
  await expect
    .poll(async () => {
      const d = await readDetail(page);
      return d ? JSON.stringify([d.rowcount, d.rows.map((r) => r.cells[1].replace(base, "")).sort()]) : "";
    }, { timeout: 8000, intervals: [250] })
    .toBe(JSON.stringify([3, ["/", "/redirect"]]));
  void home;
});

// spec: UI-012
// spec: UI-018
// acceptance: independent
test("every Shift+click extends the selection while the crawl runs", async ({ alpha }) => {
  test.setTimeout(120_000);
  const { page } = alpha;
  const { base } = await selectHomeWithInlinks(alpha, 3, 7);
  await clickUpperRow(page, `${base}/page-2.html`);
  const lost: string[] = [];
  for (let k = 3; k <= 8; k++) {
    // Umschalt+Klick auf page-k: markiert page-2 … page-k (UI-012: Mehrfachauswahl per Shift+Klick)
    await clickUpperRow(page, `${base}/page-${k}.html`, ["Shift"]);
    const expected = Array.from({ length: k - 1 }, (_, i) => `${base}/page-${i + 2}.html`);
    await page.waitForTimeout(1200);
    const got = await selectedUpper(page);
    if (JSON.stringify(got) !== JSON.stringify(expected)) lost.push(`Shift+Klick auf page-${k}: ${got.length} statt ${expected.length} Zeilen markiert`);
  }
  expect(lost, "Umschalt+Klicks im laufenden Crawl werden nicht verworfen (UI-012, UI-018: Auswahl bleibt stabil)").toEqual([]);
});
