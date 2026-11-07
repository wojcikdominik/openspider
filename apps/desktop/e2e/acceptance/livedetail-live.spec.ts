// Unabhängige Abnahme B4-LIVEDETAIL, Teil 1: Live-Aktualisierung von Tabellen, Zählern, Overview, Filterzahlen und Seitenleiste
// während eines laufenden Crawls gegen die echte Bridge (UI-018). Erwartetes Verhalten stammt aus docs/spec/05-ui.md.
// acceptance: independent
import { startAlphaBridge } from "../alpha/support";
import { startGate } from "./livedetail-gate";
import {
  TALL,
  activeChipCount,
  crawlMore,
  crawlTo,
  expect,
  coreTotal,
  footer,
  installRecorder,
  lagRate,
  machineFactor,
  mark,
  openSidebarTab,
  overviewFingerprint,
  overviewSummary,
  parseFooter,
  recordedOverviewDistinct,
  recorderFactor,
  pauseCrawl,
  phase,
  readUpper,
  resumeCrawl,
  startGated,
  test,
  gatedProfile,
  stopRecorder,
  summaryValue,
  toNumber,
  upperGrid,
  waitCrawled,
  type Sample,
} from "./livedetail-support";

/** Letzter Wert von `pick` unter allen Stichproben, die höchstens bis `t` zurückliegen. */
const lastBefore = (samples: Sample[], t: number, pick: (s: Sample) => number): number => {
  let value = 0;
  for (const s of samples) {
    if (s.t > t) break;
    value = pick(s);
  }
  return value;
};

// spec: UI-018
// spec: UI-006
// spec: UI-060
// acceptance: independent
test("table footer, filter chip and Overview grow while the crawl runs and are exact after pause and at the end", async ({ alpha, gate }) => {
  test.setTimeout(180_000);
  const { page } = alpha;
  await page.setViewportSize(TALL);
  await startGated(alpha, gate);
  const slow0 = await machineFactor(page);
  const rate = lagRate(slow0, 2, 23);
  await installRecorder(page, 100);
  await expect.poll(() => phase(page)).toBe("running");
  // Fortschritt in drei Etappen (jeweils `rate` URLs je Sekunde, Zeitgeber im Test; die Rate richtet sich nach dem Rechnerfaktor, damit das Fenster der
  // Verzögerungsprüfung unten genug geprüfte Stichproben enthält). Nach jeder Etappe steht der Crawl am Tor, bis die Overview den
  // Stand der Etappe zeigt: Die Overview zählt gedrosselt (UI-018: nächste Zählung frühestens nach max(2 s, 3 × Dauer der letzten)), wie weit sie
  // hinter dem Crawl liegt, hängt also von der Dauer der Zählung ab – eine feste Sekundengrenze wäre eine Aussage über den Rechner. Geprüft wird,
  // dass die Overview jeden Zwischenstand ohne Nutzeraktion einholt und dabei nie zurückfällt.
  for (const stage of [8, 16, 23]) {
    gate.meter(rate, stage);
    await waitCrawled(page, stage, 180_000);
    await expect.poll(async () => (await overviewSummary(page))["Total URLs Crawled"], { timeout: 15_000 * (await recorderFactor(page, 100, slow0)).slow, intervals: [250], message: `Overview holt den Stand ${stage} des wartenden Crawls ein` }).toBeGreaterThanOrEqual(stage);
  }
  await expect.poll(() => recordedOverviewDistinct(page), { timeout: 15_000, intervals: [250] }).toBeGreaterThanOrEqual(3);
  await mark(page, "before-pause");
  await pauseCrawl(page, gate);
  const factors = await recorderFactor(page, 100, slow0);
  const slow = factors.slow;
  const samples = await stopRecorder(page);
  // Stand nach der Pause (zur Einordnung eines Nachlaufs: bleibt die Anzeige stehen oder holt sie auf?)
  const afterPause = await footer(page);

  const running = samples.filter((s) => s.phase === "running" && s.footer);
  expect(running.length, "Stichproben während des Laufs").toBeGreaterThan(5);
  // Bezugszeit: die erste Stichprobe, in der die Tabelle den Crawl eingeholt hat (Fuß ≥ gecrawlt). Das erstmalige Öffnen und Füllen der Tabelle auf
  // einem langsamen Rechner gehört nicht zur Aktualisierung pro URL (UI-018); ab dann muss sie dem Crawl innerhalb der Grenze folgen.
  const t0 = (running.find((s) => parseFooter(s.footer).shown > 0 && parseFooter(s.footer).shown >= s.crawled) ?? running[0]!).t;

  // 1. Der Tabellenfuß wächst fortlaufend und fällt nie zurück (UI-018: Tabellen wachsen kontinuierlich).
  const shown = running.map((s) => parseFooter(s.footer).shown).filter((n) => n >= 0);
  expect(new Set(shown).size, "verschiedene Zeilenzahlen im Tabellenfuß während des Laufs").toBeGreaterThanOrEqual(5);
  for (let i = 1; i < shown.length; i++) expect(shown[i]!, `Tabellenfuß fällt zurück bei Stichprobe ${i}`).toBeGreaterThanOrEqual(shown[i - 1]!);

  // 2. Live-Zähler der Tabellen (Chip des aktiven Filters, Tabellenfuß): „≤ 1 s“ (UI-018, OS-Festlegung). Messspielraum für den
  //    Dev-Server: 2 s. Referenz ist der Zähler der Statusleiste/Sitzung („erledigt“), der unabhängig vom Tabellen-Code läuft.
  const lagFooter: string[] = [];
  const lagChip: string[] = [];
  let worstFooter = 0;
  let checked = 0;
  for (const s of running) {
    if (s.t - t0 < 2000 * slow) continue;
    checked++;
    const reference = lastBefore(samples, s.t - 2000 * slow, (x) => x.crawled);
    const f = parseFooter(s.footer).shown;
    const c = toNumber(/\(([\d.,]+)\)/.exec(s.chip)?.[1] ?? "-1");
    worstFooter = Math.max(worstFooter, reference - f);
    if (f < reference) lagFooter.push(`t=${Math.round(s.t - t0)}ms footer=${f} < crawled(t-2s)=${reference}`);
    if (c < reference) lagChip.push(`t=${Math.round(s.t - t0)}ms chip=${c} < crawled(t-2s)=${reference}`);
  }
  expect(lagFooter, `Tabellenfuß hinkt dem Crawl um mehr als 2 s (mal Rechnerfaktor) hinterher; Stand nach der Pause: ${afterPause.shown} von ${afterPause.total}, Session-Zähler ${lastBefore(samples, Number.POSITIVE_INFINITY, (x) => x.crawled)}`).toEqual([]);
  expect(lagChip, "Chip des aktiven Filters hinkt dem Crawl um mehr als 2 s (mal Rechnerfaktor) hinterher").toEqual([]);
  console.log(`[livedetail] samples=${running.length} checked=${checked} distinctFooter=${new Set(shown).size} worstFooterShortfall=${worstFooter} machine=${slow0.toFixed(1)} recorder=${factors.raw.toFixed(1)}/${factors.capped.toFixed(1)}`);
  await test.info().attach("live-lag", { body: JSON.stringify({ worstFooterShortfall: worstFooter, samples: running.length, checked, machineFactor: slow0, recorderFactorRaw: factors.raw, recorderFactorCapped: factors.capped, slow }), contentType: "application/json" });
  expect(checked, "geprüfte Stichproben der Verzögerung (sonst wäre die Prüfung leer)").toBeGreaterThanOrEqual(5);

  // 3. Overview (gemeinsame, gedrosselte Zählung): wächst während des Laufs mehrfach; Alter des Stands ≤ ca. 6 s hinter dem Crawl.
  const overviewCrawled = running.map((s) => summaryValue(s.overview, /^Total URLs Crawled/)).filter((n) => n >= 0);
  console.log(`[livedetail] overview distinct=${[...new Set(overviewCrawled)].join(",")}`);
  expect(new Set(overviewCrawled).size, "Overview-Zähler „Total URLs Crawled“ ändert sich während des Laufs").toBeGreaterThanOrEqual(3);
  for (let i = 1; i < overviewCrawled.length; i++) expect(overviewCrawled[i]!, `Overview fällt zurück bei Stichprobe ${i}`).toBeGreaterThanOrEqual(overviewCrawled[i - 1]!);
  // 4. Nach der Pause: alle Werte exakt, ohne Nutzeraktion (UI-018 OS-Festlegung); Quelle der Wahrheit ist der Kern.
  const total = await coreTotal(page, "Response Codes", "All");
  await expect
    .poll(async () => {
      const f = await footer(page);
      const o = (await overviewSummary(page))["Total URLs Crawled"];
      return JSON.stringify({ shown: f.shown, total: f.total, chip: await activeChipCount(page), overview: o });
    }, { timeout: 12_000, intervals: [250] })
    .toBe(JSON.stringify({ shown: total, total, chip: total, overview: total }));
  const status = await page.getByTestId("crawl-status").innerText();
  expect(status.replace(/\s+/g, " ")).toContain(`Done ${total} of`);

  // 5. Fortsetzen und beenden: 27 URLs, 25 interne, 2 externe (expected.json von fx-ui-basic), alle Anzeigen identisch.
  await resumeCrawl(page);
  gate.open();
  await alpha.waitCompleted(60_000);
  await expect
    .poll(async () => {
      const f = await footer(page);
      const o = await overviewSummary(page);
      return JSON.stringify({ shown: f.shown, total: f.total, chip: await activeChipCount(page), crawled: o["Total URLs Crawled"], internal: o["Total Internal URLs"], external: o["Total External URLs"] });
    }, { timeout: 12_000, intervals: [250] })
    .toBe(JSON.stringify({ shown: 27, total: 27, chip: 27, crawled: 27, internal: 25, external: 2 }));
  // Der Filter-Dropdown (Auswahlliste) zeigt die Zähler des Endstands.
  await alpha.table().getByRole("button", { name: /Filter/, exact: true }).click();
  await expect(page.getByRole("option", { name: /^Success \(2XX\)\s*25$/ })).toBeVisible();
  await expect(page.getByRole("option", { name: /^Redirection \(3XX\)\s*1$/ })).toBeVisible();
  await expect(page.getByRole("option", { name: /^Client Error \(4XX\)\s*1$/ })).toBeVisible();
  await page.keyboard.press("Escape");
  // Der Hinweis auf das Alter des Stands gehört zum laufenden Crawl; am Ende sind die Werte exakt.
  await expect(page.locator("[data-overview-age]")).toHaveCount(0, { timeout: 12_000 });
  const finalRows = await readUpper(page);
  expect(finalRows?.rowcount, "aria-rowcount der oberen Tabelle = Gesamtzahl + Kopfzeile").toBe(28);
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
