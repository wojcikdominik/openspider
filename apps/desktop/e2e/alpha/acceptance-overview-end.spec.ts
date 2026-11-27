// Unabhängige Abnahme fix/overview-stale (nur aus der Spec): UI-018 „Nach … Crawl-Ende sind alle Werte exakt und ohne
// Nutzeraktion auf dem … finalen Stand“ mit der Koordinationsentscheidung „exakt, sobald Completed“ (b4-lagfix2.md,
// Nachtrag 09.11.2026, Punkt 2; höchstens 3 s Wartezeit), UI-061 (Overview), UI-062 (Issues).
// Sehr kurzer Crawl der kleinen Fixture fx-ui-basic (echte Bridge). In der Seite hält ein MutationObserver den DOM-Stand in dem
// Augenblick fest, in dem „Completed“ erscheint (ohne jedes Warten), und misst mit performance.now() die Zeit vom Endtick
// (erster Tick mit Phase „completed“) bis dahin. Erwartungswerte wie in detail-sidebar.spec.ts aus der expected.json von
// fx-ui-basic: 27 gefunden und gecrawlt, 25 intern, 2 extern, ein interner 4XX-Fehler.
import type { Page } from "@playwright/test";
import { expect, test, type Alpha } from "./fixture";

const CLIENT_ERROR = "Response Codes: Internal Client Error (4XX)";
const SUMMARY = { "Total URLs Encountered": 27, "Total URLs Crawled": 27, "Total Internal URLs": 25, "Total External URLs": 2 };
const CARDS = { "Total URLs Encountered": 27, "Total URLs Crawled": 27 };
/**
 * „Ohne spürbare Verzögerung“: deutlich unter der Obergrenze von 3 s. Die Schranke wartet auf eine Overview-/Issues-Antwort
 * zur Endversion und deren Zeichnen (eine Anfrage an den Kern, die Zählung der Endversion ist dort beim Endtick bereits
 * angekündigt, plus ein Render); 1500 ms lassen einem belasteten Rechner ein Vielfaches davon und schlagen sicher an, wenn
 * die Schranke erst über die Obergrenze freigegeben wird.
 */
const PROMPT_MS = 1_500;

interface Snapshot {
  overview: Record<string, number> | null;
  issues: Record<string, number> | null;
  cards: Record<string, number> | null;
}
interface Observed { endTickAt: number | null; completedAt: number | null; phaseText: string | null; snapshot: Snapshot | null }

/** Vor dem Start: Endtick-Zeit am Sitzungszustand abgreifen und den DOM-Stand beim Erscheinen von „Completed“ festhalten. */
async function arm(page: Page): Promise<void> {
  await page.evaluate(async () => {
    const sessionPath = "/src/state/session.ts";
    const { useSession } = await import(/* @vite-ignore */ sessionPath);
    const state: Observed = { endTickAt: null, completedAt: null, phaseText: null, snapshot: null };
    (window as unknown as { __ovt: Observed }).__ovt = state;
    type TickLike = { crawl?: { phase?: string } | null };
    const original = useSession.getState().applyTick as (tick: TickLike) => void;
    useSession.setState({
      applyTick: (tick: TickLike) => {
        if (state.endTickAt === null && tick.crawl?.phase === "completed") state.endTickAt = performance.now();
        original(tick);
      },
    });
    const number = (text: string | null | undefined) => Number((text ?? "").replace(/\D/g, ""));
    const read = (): Snapshot => {
      const panel = document.getElementById("sidebar-panel");
      const summary = [...(panel?.querySelectorAll("table") ?? [])].find((t) => t.querySelector("caption")?.textContent?.trim() === "Summary");
      const overview = summary
        ? Object.fromEntries([...summary.querySelectorAll("tr")].filter((r) => r.querySelector("th") && r.querySelector("td")).map((r) => [r.querySelector("th")!.textContent!.trim(), number(r.querySelector("td")!.textContent)]))
        : null;
      const issuesTable = panel?.querySelector("table.os-sidebar-table");
      const issues = issuesTable
        ? Object.fromEntries([...issuesTable.querySelectorAll("tbody tr")].filter((r) => r.querySelector("th button")).map((r) => [r.querySelector("th button")!.textContent!.trim(), number(r.querySelectorAll("td")[2]?.textContent)]))
        : null;
      const dashboard = document.querySelector("[data-dashboard]");
      const cards = dashboard
        ? Object.fromEntries([...dashboard.querySelectorAll("article.os-dashboard-card")].map((c) => [c.querySelector("span")?.textContent?.trim() ?? "", number(c.querySelector("strong")?.textContent)]))
        : null;
      return { overview, issues, cards };
    };
    const observer = new MutationObserver(() => {
      if (state.snapshot) return;
      const text = document.querySelector('[data-testid="crawl-phase"]')?.textContent ?? "";
      if (!text.startsWith("Completed")) return;
      state.completedAt = performance.now();
      state.phaseText = text;
      state.snapshot = read();
      observer.disconnect();
    });
    observer.observe(document.body, { subtree: true, childList: true, characterData: true });
  });
}

async function crawlAndObserve(alpha: Alpha, scenario: string): Promise<{ snapshot: Snapshot; latency: number }> {
  const { page } = alpha;
  await alpha.startFromHeader();
  await expect.poll(() => page.evaluate(() => (window as unknown as { __ovt: Observed }).__ovt.snapshot !== null), { timeout: 30_000 }).toBe(true);
  const observed = await page.evaluate(() => (window as unknown as { __ovt: Observed }).__ovt);
  expect(observed.endTickAt, "end tick seen by the session state").not.toBeNull();
  const latency = observed.completedAt! - observed.endTickAt!;
  console.log(`[ovt] ${scenario} latency=${latency.toFixed(1)}ms phase=${JSON.stringify(observed.phaseText)} snapshot=${JSON.stringify(observed.snapshot)}`);
  test.info().annotations.push({ type: "ovt-latency-ms", description: `${scenario}: ${latency.toFixed(1)}` });
  return { snapshot: observed.snapshot!, latency };
}

const sidebarTab = (page: Page, name: string) => page.getByRole("tab", { name, exact: true });
const pick = (values: Record<string, number> | null, keys: Record<string, number>) => (values ? Object.fromEntries(Object.keys(keys).map((k) => [k, values[k]])) : null);

test.describe("acceptance: exact Overview and Issues as soon as Completed appears", () => {
  test.describe.configure({ timeout: 60_000 });

  // spec: UI-018
  // spec: UI-061
  // acceptance: independent
  test("sidebar Overview active: the summary at the moment of Completed is the final one", async ({ alpha }) => {
    await alpha.open();
    await expect(sidebarTab(alpha.page, "Overview")).toHaveAttribute("aria-selected", "true");
    await arm(alpha.page);
    const { snapshot } = await crawlAndObserve(alpha, "overview-active");
    expect(pick(snapshot.overview, SUMMARY)).toEqual(SUMMARY);
  });

  // spec: UI-018
  // spec: UI-061
  // spec: UI-062
  // acceptance: independent
  test("Dashboard open and sidebar Issues active: dashboard numbers and issue counts at the moment of Completed are final, without delay", async ({ alpha }) => {
    const { page } = alpha;
    await alpha.open();
    await page.locator(".u34-nav-item[data-group=overview] .u34-nav-button").click();
    await expect(page.locator("[data-dashboard]")).toBeVisible();
    await sidebarTab(page, "Issues").click();
    await expect(sidebarTab(page, "Issues")).toHaveAttribute("aria-selected", "true");
    await arm(page);
    const { snapshot, latency } = await crawlAndObserve(alpha, "dashboard+issues-active");
    expect(pick(snapshot.cards, CARDS)).toEqual(CARDS);
    expect(snapshot.issues?.[CLIENT_ERROR]).toBe(1);
    // The sidebar Overview is not drawn (other tab active): no wait for it.
    expect(latency).toBeLessThan(PROMPT_MS);
  });

  // spec: UI-018
  // spec: UI-062
  // acceptance: independent
  test("sidebar Issues active (Overview not drawn): issue counts final at Completed, which appears without delay", async ({ alpha }) => {
    const { page } = alpha;
    await alpha.open();
    await sidebarTab(page, "Issues").click();
    await expect(sidebarTab(page, "Issues")).toHaveAttribute("aria-selected", "true");
    await arm(page);
    const { snapshot, latency } = await crawlAndObserve(alpha, "issues-active");
    expect(snapshot.issues?.[CLIENT_ERROR]).toBe(1);
    expect(latency).toBeLessThan(PROMPT_MS);
  });

  // spec: UI-018
  // spec: UI-061
  // acceptance: independent
  test("sidebar collapsed: Completed appears without delay and the hidden Overview holds the final summary", async ({ alpha }) => {
    const { page } = alpha;
    await alpha.open();
    const sidebar = page.locator(".os-app-sidebar");
    if (await sidebar.isVisible()) {
      await page.evaluate(async () => {
        const path = "/src/ui/actions.ts";
        const { runAction } = await import(/* @vite-ignore */ path);
        runAction("view.toggleSidebar", { source: "menu" });
      });
    }
    await expect(sidebar).toBeHidden();
    await arm(page);
    const { snapshot, latency } = await crawlAndObserve(alpha, "sidebar-collapsed");
    expect(latency).toBeLessThan(PROMPT_MS);
    expect(pick(snapshot.overview, SUMMARY)).toEqual(SUMMARY);
  });
});
