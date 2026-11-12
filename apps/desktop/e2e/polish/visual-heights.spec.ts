import { test, expect } from "../alpha/fixture";
import { crawledWithSelection, showPanes } from "./support";

test.beforeEach(async ({ page }) => {
  await page.clock.setFixedTime(new Date("2026-10-28T12:00:00Z"));
});

for (const language of ["en", "de"] as const) for (const theme of ["light", "dark"] as const) {
  test.describe(`${language}/${theme}`, () => {
    test.use({ locale: language === "de" ? "de-DE" : "en-US" });
    // spec: UI-120
    // spec: UI-001
    // spec: UI-003
    // spec: UI-013
    // spec: UI-016
    test("K2-77 measured heights at 100% zoom, both densities, no overlap at 1024 × 640", async ({ alpha }, info) => {
      const { page } = alpha;
      await page.addInitScript(() => {
        const stroke = CanvasRenderingContext2D.prototype.strokeRect;
        CanvasRenderingContext2D.prototype.strokeRect = function(x, y, w, h) {
          if (this.canvas.classList.contains("os-grid-canvas") && this.lineWidth === 1 && w > 20) {
            const physicalHeight = h * this.getTransform().d * this.canvas.getBoundingClientRect().height / this.canvas.height;
            this.canvas.dataset[y < 0 ? "u35Head" : "u35Row"] = String(physicalHeight);
          }
          return stroke.call(this, x, y, w, h);
        };
      });
      await page.setViewportSize({ width: 1024, height: 640 });
      await crawledWithSelection(alpha);
      const measurements = [];
      for (const density of ["compact", "normal"] as const) {
        await page.evaluate(async ({ theme, density }) => {
          const path = "/src/state/prefs.ts";
          const { usePrefs } = await import(/* @vite-ignore */ path);
          await usePrefs.getState().patch({ appearance: { theme, rowDensity: density, zoomPct: 100 } });
        }, { theme, density });
        const canvas = page.locator(".u10-grid-host canvas");
        await expect.poll(() => canvas.getAttribute("data-u35-row").then(Number)).toBe(density === "compact" ? 28 : 44);
        const mcp = await page.getByTestId("crawl-status").getByRole("button", { name: /MCP:/ }).boundingBox();
        expect(mcp).not.toBeNull();
        expect(mcp!.x).toBeGreaterThanOrEqual(-1);
        expect(mcp!.x + mcp!.width).toBeLessThanOrEqual(1025);
        expect(mcp!.y + mcp!.height).toBeLessThanOrEqual(641);
        const measured = await page.evaluate(() => {
          const height = (s: string) => document.querySelector(s)!.getBoundingClientRect().height;
          const canvas = document.querySelector<HTMLCanvasElement>(".u10-grid-host canvas")!;
          const clipped = (e: HTMLElement) => {
            const r = e.getBoundingClientRect();
            let left = r.left, right = r.right, top = r.top, bottom = r.bottom;
            for (let p = e.parentElement; p; p = p.parentElement) {
              const css = getComputedStyle(p), b = p.getBoundingClientRect();
              if (css.overflowX !== "visible") { left = Math.max(left, b.left); right = Math.min(right, b.right); }
              if (css.overflowY !== "visible") { top = Math.max(top, b.top); bottom = Math.min(bottom, b.bottom); }
            }
            return { left, right, top, bottom };
          };
          const controls = [...document.querySelectorAll<HTMLElement>('button, a[href], input:not([type=hidden]), select, textarea, summary, [role=tab], [role=combobox], [tabindex]:not([tabindex="-1"])')].filter(e => {
            const r = clipped(e), css = getComputedStyle(e);
            return r.right > r.left && r.bottom > r.top && css.visibility !== "hidden" && css.display !== "none";
          });
          const overlaps = controls.flatMap((a, i) => controls.slice(i + 1).filter(b => {
            if (a.contains(b) || b.contains(a)) return false;
            const x = clipped(a), y = clipped(b);
            return Math.min(x.right, y.right) - Math.max(x.left, y.left) > 1 && Math.min(x.bottom, y.bottom) - Math.max(x.top, y.top) > 1;
          }).map(b => `${a.getAttribute('aria-label') ?? a.textContent}/${b.getAttribute('aria-label') ?? b.textContent}`));
          return { row: Number(canvas.dataset.u35Row), head: Number(canvas.dataset.u35Head), tab: height('.os-tabbar [role=tab]:not([hidden])'), toolbar: height('.u10-toolbar'), control: height('.os-crawl-header form button'), status: height('footer'), overlaps, overflow: document.documentElement.scrollWidth > innerWidth };
        });
        for (const [key, expected] of Object.entries({ row: density === "compact" ? 28 : 44, head: density === "compact" ? 32 : 44, tab: 28, toolbar: 32, control: 24, status: 24 })) expect(Math.abs(measured[key as keyof typeof measured] as number - expected), `${density}: ${key}`).toBeLessThanOrEqual(1);
        expect(measured.overlaps).toEqual([]);
        expect(measured.overflow).toBe(false);
        measurements.push({ density, ...measured });
      }
      await info.attach('measured-heights.json', { body: JSON.stringify(measurements, null, 2), contentType: 'application/json' });
    });
  });
}

// spec: UI-061
// spec: UI-062
// spec: UI-004 [P1]
test("overview shows real fixture counts and problems navigate to their filter", async ({ alpha }) => {
  await alpha.open(); await alpha.startFromHeader(); await alpha.waitCompleted();
  await alpha.page.locator('[data-group="overview"] button').click();
  const dashboard = alpha.page.locator('[data-dashboard]');
  await expect(dashboard).toContainText('Status distribution');
  await expect(alpha.page.getByRole('region', { name: 'Details', exact: true })).toBeHidden();
  await expect(dashboard.locator('.os-dashboard-metrics')).toContainText('27');
  // UI-061 (OS-Festlegung 02.11.2026): robots.txt/sitemap fetches are not encountered URLs; table rows are 27.
  await expect(dashboard.locator('.os-dashboard-metrics strong')).toHaveText(['27', '27', '0', '21', '2']);
  const problem = dashboard.locator('.os-dashboard-sections > section').last().getByRole('button').first();
  await expect(problem).toBeVisible();
  await problem.click();
  await expect(dashboard).toHaveCount(0);
  await expect(alpha.table().getByRole('grid')).toBeVisible();
  await expect(alpha.page.getByRole('region', { name: 'Details', exact: true })).toBeVisible();
});

// spec: UI-129 [P1]
test('MCP tool columns and empty state use the shared design system', async ({ alpha }) => {
  await alpha.page.setViewportSize({ width: 1024, height: 640 });
  await alpha.open();
  await alpha.page.locator('[data-group="overview"] button').click();
  await expect(alpha.page.locator('.os-dashboard [data-empty-state="NO_DATA_YET"]')).toBeVisible();
  await alpha.page.evaluate(async () => {
    const path = '/src/features/mcp/model.ts';
    const { openMcp } = await import(/* @vite-ignore */ path);
    openMcp('tools');
  });
  const table = alpha.page.locator('.os-mcp-table');
  await expect(table.getByRole('checkbox', { name: 'query_urls', exact: true })).toBeChecked();
  const columns = await table.locator('th').evaluateAll(nodes => nodes.map(n => n.getBoundingClientRect().width));
  expect(columns[0]).toBeGreaterThanOrEqual(47);
  expect(columns[1]).toBeGreaterThanOrEqual(120);
  expect(columns[3]).toBeGreaterThanOrEqual(180);
});

// spec: UI-007
// spec: UI-110 [P1]
test('selection boxes and row ellipsis use the existing selection and context menu', async ({ alpha }) => {
  await alpha.open(); await alpha.startFromHeader(); await alpha.waitCompleted();
  await alpha.settledGrid();
  const canvas = alpha.table().locator('canvas');
  // UI-018 tail mode: a table that showed its last row while the crawl was still running follows new rows and ends
  // scrolled to the bottom (slow runners). A wheel gesture up returns to the first row before the clicks at its position.
  const area = await canvas.boundingBox();
  expect(area).not.toBeNull();
  await alpha.page.mouse.move(area!.x + area!.width / 2, area!.y + area!.height / 2);
  await alpha.page.mouse.wheel(0, -100_000);
  const row = alpha.table().locator('.os-grid-a11y [aria-rowindex="2"]');
  await expect(row).toHaveAttribute('aria-selected', 'false');
  await canvas.click({ position: { x: 14, y: 46 } });
  await expect(row).toHaveAttribute('aria-selected', 'true');
  await canvas.click({ position: { x: 14, y: 46 } });
  await expect(row).toHaveAttribute('aria-selected', 'false');
  const actionX = await alpha.page.evaluate(async () => {
    const path = '/src/state/columns.ts';
    const { useColumns } = await import(/* @vite-ignore */ path);
    return useColumns.getState().byTab.Internal.find((c: { key: string }) => c.key === 'Address').width - 12;
  });
  await canvas.click({ position: { x: actionX, y: 46 } });
  await expect(alpha.page.getByRole('menuitem', { name: 'Copy URLs', exact: true })).toBeEnabled();
  await expect(row).toHaveAttribute('aria-selected', 'true');
  await alpha.page.keyboard.press('Escape');
  await expect(alpha.page.getByRole('dialog')).toHaveCount(0);
});

// spec: UI-001
// spec: UI-113
test('narrow table toolbar keeps every enabled action reachable by scrolling and focus', async ({ alpha }) => {
  const { page } = alpha;
  await page.setViewportSize({ width: 1024, height: 640 });
  await crawledWithSelection(alpha);
  await showPanes(page, 360);
  await alpha.settledGrid();
  const toolbar = page.locator('.u10-toolbar');
  // Loaded filter chips force overflow; the original shell collision check stays unchanged.
  expect(await toolbar.evaluate(el => el.scrollWidth > el.clientWidth)).toBe(true);
  const controls = toolbar.locator('button:not(:disabled), input:not(:disabled), select:not(:disabled)');
  for (const control of await controls.all()) {
    if (!await control.isVisible()) continue;
    await control.scrollIntoViewIfNeeded();
    await control.focus();
    await expect(control).toBeFocused();
    await expect.poll(() => control.evaluate(el => {
      const row = el.closest('.u10-toolbar')!.getBoundingClientRect();
      const rect = el.getBoundingClientRect();
      return { name: el.getAttribute('aria-label') ?? el.textContent,
        row: { left: row.left, right: row.right, top: row.top, bottom: row.bottom },
        rect: { left: rect.left, right: rect.right, top: rect.top, bottom: rect.bottom },
        within: rect.left >= row.left - .5 && rect.right <= row.right + .5
          && rect.top >= row.top - .5 && rect.bottom <= row.bottom + .5 };
    })).toMatchObject({ within: true });
  }
  await toolbar.getByRole('button', { name: 'Configure columns…', exact: true }).click();
  await expect(page.getByRole('dialog')).toBeVisible();
});
