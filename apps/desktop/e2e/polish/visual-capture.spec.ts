import { test, expect, type Alpha } from '../alpha/fixture';
import { crawledWithSelection, showPanes } from './support';
import { copyFileSync, mkdirSync } from 'node:fs';
import { resolve } from 'node:path';
test.use({ locale: "de-DE" });
test.beforeEach(async ({ page }) => {
  await page.clock.setFixedTime(new Date("2026-10-28T12:00:00Z"));
});

// The capture needs loaded view counts and details after each theme/navigation change.
async function readyTable(alpha: Alpha, theme: string) {
  await expect(alpha.page.locator('html')).toHaveAttribute('data-theme', theme);
  await alpha.settledGrid();
  await expect.poll(() => alpha.counter()).toMatch(/^Angezeigt: 25 von 25(?: · 1 markiert)?$/);
  // UI-018 tail mode: a table that showed its last row while the crawl was still running ends scrolled to the bottom
  // (slow runners). A wheel gesture up returns to the first row before the click at its position.
  const area = await alpha.table().locator('canvas').boundingBox();
  expect(area).not.toBeNull();
  await alpha.page.mouse.move(area!.x + area!.width / 2, area!.y + area!.height / 2);
  await alpha.page.mouse.wheel(0, -100_000);
  await alpha.settledGrid();
  await alpha.table().locator('canvas').click({ position: { x: 60, y: 46 } });
  await expect(alpha.page.locator('.os-proptable').getByRole('cell', { name: alpha.startUrl, exact: true }).first()).toBeVisible();
}

// spec: UI-001
test('own fx-ui-basic screenshots', async ({ alpha }, testInfo) => {
  const capture = async (name: string) => {
    const filename = `${name}-${testInfo.project.name}.png`;
    const path = testInfo.outputPath(filename);
    await alpha.page.screenshot({ scale: 'css', path });
    if (process.env.OS_CAPTURE_REPORT === '1') {
      const dir = resolve('../../docs/plan/M2-reports/u35');
      mkdirSync(dir, { recursive: true });
      copyFileSync(path, resolve(dir, filename));
    }
  };
  await alpha.page.setViewportSize({ width: 1024, height: 640 });
  await crawledWithSelection(alpha);
  for (const theme of ['light', 'dark']) {
    await alpha.page.evaluate(async theme => {
      const path = '/src/state/prefs.ts';
      const { usePrefs } = await import(/* @vite-ignore */ path);
      await usePrefs.getState().patch({ appearance: { theme } });
    }, theme);
    await readyTable(alpha, theme);
    await capture(`${process.env.U35_CAPTURE ?? 'nachher'}-${theme}-1024x640`);
  }
  if (!process.env.U35_CAPTURE) {
    await alpha.page.setViewportSize({ width: 1440, height: 900 });
    await showPanes(alpha.page, "default");
    for (const theme of ['light', 'dark']) {
      await alpha.page.evaluate(async theme => {
        const path = '/src/state/prefs.ts';
        const { usePrefs } = await import(/* @vite-ignore */ path);
        await usePrefs.getState().patch({ appearance: { theme } });
      }, theme);
      await readyTable(alpha, theme);
      await capture(`nachher-${theme}-1440x900`);
      await alpha.page.locator('[data-group="overview"] button').click();
      await alpha.page.locator('[data-dashboard] h2').first().waitFor();
      await alpha.page.locator('[data-dashboard] .os-dashboard-sections > section').last().getByRole('button').first().waitFor();
      await capture(`dashboard-${theme}-1440x900`);
      await alpha.page.locator('[data-group="urls"] button').first().click();
    }
  }
});
