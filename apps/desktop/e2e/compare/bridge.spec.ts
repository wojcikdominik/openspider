import { readFileSync } from "node:fs";
import { createServer } from "node:http";
import { test, expect, type Locator, type Page } from "@playwright/test";
import { startBridge } from "../support/bridge";
import type { CrawlListing } from "../../src/ipc/gen/CrawlListing";
import type {} from "./harness";

async function selectRow(grid: Locator, muster: RegExp) {
  await expect(grid).toHaveAttribute("aria-rowcount", /^[2-9]$|^[1-9]\d+$/);
  const rowCount = Number(await grid.getAttribute("aria-rowcount"));
  await grid.focus();
  await grid.press("Home");
  const selected = grid.getByRole("row", { selected: true });
  for (let row = 1; row < rowCount; row++) {
    await expect(selected).toHaveCount(1);
    if (await selected.getByRole("gridcell").filter({ hasText: muster }).count()) return;
    if (row + 1 < rowCount) await grid.press("ArrowDown");
  }
  await expect(selected).toContainText(muster);
}

async function prepare(page:Page,version:()=>void) {
  const ids:string[]=[];
  for (let i=0;i<2;i++) {
    if(i)version();
    await page.evaluate(async()=>{await window.compareHarness.invoke("crawl.start",{source:{kind:"spider",url:new URLSearchParams(location.search).get("site")},scope:"domain"});});
    await expect.poll(async()=>{
      const rows=await page.evaluate(()=>window.compareHarness.invoke<CrawlListing[]>("crawls.list",{show:"crawls"}));
      return rows.length===i+1 && rows.every(r=>r.crawl.phase==="completed");
    },{timeout:30_000}).toBe(true);
    const rows=await page.evaluate(()=>window.compareHarness.invoke<CrawlListing[]>("crawls.list",{show:"crawls"}));ids.push(rows.find(r=>!ids.includes(r.crawl.crawlId))!.crawl.crawlId);
    await expect.poll(()=>page.evaluate(async()=>{try{await window.compareHarness.invoke("crawl.clear");return true;}catch(e){if((e as {code:string}).code==="Busy")return false;throw e;}}),{timeout:30_000}).toBe(true);
  }
  return ids;
}
// spec: CRAWL-144
// spec: OUT-140
// spec: OUT-141
// spec: OUT-142
// spec: OUT-143
// spec: OUT-146
// spec: OUT-149
// spec: TECH-301
test("real crawl pair: clickable classes, swap, mappings, link changes, source and Change Detection",async({page})=>{
  test.setTimeout(180_000);
  await page.setViewportSize({ width: 1600, height: 800 });
  let version=1;
  const server=createServer((req,res)=>{
    const url=req.url??"/";res.setHeader("content-type",url==="/robots.txt"?"text/plain":"text/html");
    if(url==="/robots.txt"){res.end("User-agent: *\nAllow: /\n");return;}
    const links=url==="/start" ? (version===1 ? '<a href="/b">B</a><a href="/c">Alt</a><a href="/same">Same</a>' : '<a href="/a">A</a><a href="/c">Neu</a><a href="/same">Same</a>') : (url==="/a" || url==="/b" ? '<a href="/same">Same</a>' : "");
    res.end(`<!doctype html><title>${url} ${url==="/c"?version:""}</title><h1>Fixture</h1>${links}`);
  });
  await new Promise<void>(resolve=>server.listen(0,"127.0.0.1",resolve));
  const bridge=await startBridge();expect(bridge,"compiled real core bridge required").not.toBeNull();
  try {
    const port=(server.address() as {port:number}).port;
    await page.goto(`/e2e/compare/index.html?bridge=${encodeURIComponent(bridge!.url)}&site=${encodeURIComponent(`http://127.0.0.1:${port}/start`)}`);
    await page.waitForFunction(()=>!!window.compareHarness);
    const ids=await prepare(page,()=>{version=2;});
    await page.getByRole("button",{name:"Compare crawls",exact:true}).click();
    // Deliberately supply reverse chronology; core selects the later crawl.
    await page.getByLabel("Current",{exact:true}).selectOption(ids[0]!);await page.getByLabel("Previous",{exact:true}).selectOption(ids[1]!);
    await expect(page.getByRole("button",{name:"Compare",exact:true})).toBeEnabled();await page.getByRole("button",{name:"Compare",exact:true}).click();
    await expect.poll(()=>page.evaluate(()=>window.compareHarness.state().busy),{timeout:30_000}).toBe(false);
    await expect(page.getByRole("grid",{name:"Comparison table",exact:true})).toBeVisible();
    expect(await page.evaluate(()=>window.compareHarness.state().session?.current.crawlId)).toBe(ids[1]);
    const snapshot=await page.evaluate(()=>window.compareHarness.state().snapshot);
    expect(snapshot!.overview.length).toBeGreaterThan(20);
    for(const row of snapshot!.overview){if(row.current!=null && row.previous!=null && row.classes)expect(row.current-row.previous).toBe(row.classes[0]+row.classes[1]-row.classes[2]-row.classes[3]);}
    // A count click must produce exactly the displayed population.
    const html=page.getByRole("table",{name:"Comparison overview"}).getByRole("row").filter({hasText:"Internal: HTML"});
    await html.getByRole("button",{name:"1",exact:true}).nth(0).click();
    await expect(page.getByRole("grid",{name:"Comparison table",exact:true}).getByRole("row")).toHaveCount(2);
    // Select /start from Current.
    await page.getByLabel("Comparison status",{exact:true}).selectOption("");await page.getByLabel("Filter",{exact:true}).selectOption("All");
    await selectRow(page.getByRole("grid",{name:"Comparison table",exact:true}), /\/start$/);
    await page.getByRole("tab",{name:"Outlinks",exact:true}).click();
    const out=page.getByRole("region",{name:"outlinks"});
    await expect(out.getByRole("row")).toHaveCount(4);
    await out.getByLabel("Link change").selectOption("Removed");await expect(out.getByRole("gridcell",{name:/\/b$/})).toHaveCount(1);
    await out.getByLabel("Link change").selectOption("Added");await expect(out.getByRole("gridcell",{name:/\/a$/})).toHaveCount(1);
    await out.getByLabel("Link change").selectOption("Anchor Changed");await expect(out.getByRole("gridcell",{name:"Neu",exact:true})).toHaveCount(1);
    // Previous Anchor follows all 15 standard link columns; reveal it in the ARIA mirror.
    const linkScroll = out.getByRole("grid",{name:"Link comparison table",exact:true}).locator(".os-grid-scroll");
    await linkScroll.evaluate(el => { el.scrollLeft = el.scrollWidth; });
    await expect(out.getByRole("gridcell",{name:"Alt",exact:true})).toHaveCount(1);
    await linkScroll.evaluate(el => { el.scrollLeft = 0; });
    const [download]=await Promise.all([page.waitForEvent("download"),out.getByRole("button",{name:"Export view (CSV)"}).click()]);
    const csv=readFileSync((await download.path())!,"utf8");expect(csv.split("\r\n")[0]).toContain("Link Change,Crawl,Previous Anchor");expect(csv).toContain("Neu");expect(csv).toContain("Alt");
    await out.getByLabel("Link change").selectOption("All");await out.getByLabel("Source",{exact:true}).selectOption("Previous");await expect(out.getByRole("gridcell",{name:/\/b$/})).toHaveCount(1);
    // All = Added + Anchor Changed + Unchanged for Current; Previous includes Removed instead.
    await out.getByLabel("Source",{exact:true}).selectOption("Current");
    const sizes:Record<string,number>={};
    for(const change of ["All","Added","Removed","Anchor Changed","Unchanged"]){
      await out.getByLabel("Link change").selectOption(change);
      await expect.poll(()=>page.evaluate(()=>window.compareHarness.state().busy)).toBe(false);
      await expect(out.getByRole("row")).toHaveCount(change==="All"?4:2);
      sizes[change]=(await out.getByRole("row").count())-1;
    }
    expect(sizes.All).toBe(sizes.Added!+sizes["Anchor Changed"]!+sizes.Unchanged!);
    await out.getByLabel("Link change").selectOption("All");
    await selectRow(page.getByRole("grid",{name:"Comparison table",exact:true}), /\/a$/);
    for(const change of ["Added","Removed","Anchor Changed","Unchanged"])await expect(out.getByRole("option",{name:change,exact:true})).toHaveJSProperty("disabled",true);
    await expect(out.getByRole("gridcell",{name:/\/same$/})).toHaveCount(1);
    // K2-77: Change Detection is its own entry of the grouped navigation during a comparison.
    await page.getByRole("button",{name:"Change Detection",exact:true}).click();await expect(page.getByRole("tab",{name:"Change Detection",exact:true})).toHaveAttribute("aria-selected","true");await expect(page.getByRole("columnheader",{name:"Title 1 (Previous)",exact:true})).toBeVisible();
    await expect(page.getByRole("grid",{name:"Comparison table",exact:true}).getByRole("row")).toHaveCount(4);
    await page.getByRole("button",{name:"Swap Current / Previous"}).click();
    await expect.poll(()=>page.evaluate(()=>window.compareHarness.state().session?.current.crawlId)).toBe(ids[0]);
    await expect.poll(()=>page.evaluate(()=>window.compareHarness.state().busy),{timeout:30_000}).toBe(false);
    const after=await page.evaluate(()=>window.compareHarness.state().snapshot!);
    for(const before of snapshot!.overview){const row=after.overview.find(r=>r.tab===before.tab && r.filter===before.filter);if(row?.classes && before.classes)expect(row.classes).toEqual([before.classes[2],before.classes[3],before.classes[0],before.classes[1]]);}
    await page.getByRole("button",{name:"Compare crawls",exact:true}).click();await page.getByRole("button",{name:"Add mapping rule"}).click();
    await page.getByLabel("Pattern 1",{exact:true}).fill("[");await expect(page.getByRole("button",{name:"Compare",exact:true})).toBeDisabled();await expect(page.getByRole("alert")).toContainText("invalid regular expression");
    await page.getByLabel("Pattern 1",{exact:true}).fill("/amp/");await page.getByLabel("Replacement 1",{exact:true}).fill("/");await page.getByLabel("Input URL",{exact:true}).fill("https://site.test/de/amp/a");await expect(page.getByLabel("Final URL")).toHaveText("https://site.test/de/a");
    // Verify the same components in the actual main-window composition, including sidebar and lower details.
    await page.goto(`/?bridge=${encodeURIComponent(bridge!.url)}`);
    await page.getByRole("button",{name:"Compare crawls",exact:true}).click();
    await expect(page.getByRole("button",{name:"Compare",exact:true})).toBeEnabled();
    await page.getByRole("button",{name:"Compare",exact:true}).click();
    const mainTable=page.locator('[data-region="table"]').getByRole("grid",{name:"Comparison table",exact:true});
    await expect(mainTable).toBeVisible({timeout:30_000});
    await expect(page.locator(".os-app-sidebar").getByRole("table",{name:"Comparison overview"})).toBeVisible();
    await selectRow(mainTable, /\/start$/);
    await expect(page.getByRole("region",{name:"inlinks",exact:true})).toBeVisible();
  } finally {await bridge?.stop();await new Promise<void>(resolve=>server.close(()=>resolve()));}
});
