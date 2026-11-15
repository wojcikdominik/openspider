import AxeBuilder from "@axe-core/playwright";
import { expect, test, type Page } from "@playwright/test";
import fixture from "../../src/features/integrations/fixtures/integ-ui.json" with { type: "json" };
import type {} from "./harness";

const calls = (page: Page, cmd: string) => page.evaluate((c) => window.integFixture.calls().filter((x) => x.cmd === c), cmd);

test.beforeEach(async ({ page }) => {
  await page.clock.install({ time: new Date("2026-10-19T12:00:00Z") });
  await page.goto("/e2e/integrations/index.html?transport=mock");
  await page.waitForFunction(() => !!window.integFixture);
});

// spec: INTEG-001
test("six integrations, connect with exactly one request, disconnect; Escape returns focus", async ({ page }) => {
  const gear = page.getByRole("button", { name: "Configure Ahrefs" });
  await gear.click();
  const dialog = page.getByRole("dialog", { name: "API access" });
  const list = dialog.getByRole("tablist", { name: "Integrations" });
  await expect(list.getByRole("tab")).toHaveText([
    "Google Analytics 4Not connected",
    "Search ConsoleNot connected",
    "PageSpeed InsightsNot connected",
    "MajesticNot connected",
    "AhrefsNot connected",
    "MozNot connected",
  ]);
  await list.getByRole("tab", { name: /Moz/ }).click();
  await dialog.getByLabel("API key").fill("sk-TEST-INTEG-002");
  await dialog.getByRole("button", { name: "Connect", exact: true }).click();
  await expect(page.getByTestId("integ-status-moz")).toHaveText("Connecting…");
  await page.evaluate(() => window.integFixture.finish("moz", { account: "moz-account" }));
  await expect(page.getByTestId("integ-status-moz")).toHaveText("Connected (moz-account)");
  expect(await calls(page, "api.connect")).toHaveLength(1);
  const axe = await new AxeBuilder({ page }).include('[role="dialog"]').analyze();
  expect(axe.violations.filter((v) => v.impact === "critical")).toEqual([]);
  await dialog.getByRole("button", { name: "Disconnect" }).click();
  await expect(page.getByTestId("integ-status-moz")).toHaveText("Not connected");
  await expect(page.getByTestId("integ-status-moz")).toBeFocused();
  await page.keyboard.press("Escape");
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await expect(gear).toBeFocused();
});

// spec: INTEG-001
test("provider rejection shows code and message; only Disconnect leaves the error state", async ({ page }) => {
  await page.evaluate(() => window.integFixture.open("majestic"));
  const dialog = page.getByRole("dialog", { name: "API access" });
  await dialog.getByLabel("API key").fill("bad");
  await dialog.getByRole("button", { name: "Connect", exact: true }).click();
  await page.evaluate(() => window.integFixture.finish("majestic", { error: { code: "Permission", messageKey: "err-api-reconnect", args: {}, detail: "HTTP 401: Invalid token", retryable: false } }));
  await expect(page.getByTestId("integ-status-majestic")).toHaveText("Error: 401 Invalid token");
  await expect(dialog.getByRole("tab", { name: /Majestic/ })).toContainText("Error: 401 Invalid token");
  await expect(dialog.getByRole("button", { name: "Connect", exact: true })).toHaveCount(0);
  await dialog.getByRole("button", { name: "Disconnect" }).click();
  await expect(page.getByTestId("integ-status-majestic")).toHaveText("Not connected");
});

// spec: INTEG-132
test("PSI local shows the Lighthouse path, connects with it and reports a missing runtime", async ({ page }) => {
  await page.evaluate(() => window.integFixture.open("psi"));
  const dialog = page.getByRole("dialog", { name: "API access" });
  await expect(dialog.getByLabel("Lighthouse path")).toHaveCount(0);
  await dialog.getByRole("radio", { name: "Local (Lighthouse on this computer)" }).click();
  await expect(dialog.getByLabel("API key")).toHaveCount(0);
  await dialog.getByLabel("Lighthouse path").fill("/opt/lh/bin/lighthouse");
  await dialog.getByRole("button", { name: "Connect", exact: true }).click();
  const connect = await calls(page, "api.connect");
  expect(connect).toHaveLength(1);
  expect(connect[0]?.args).toMatchObject({ kind: "psi", psiLocal: true, lighthousePath: "/opt/lh/bin/lighthouse" });
  await page.evaluate(() => window.integFixture.finish("psi", { error: { code: "Permission", messageKey: "err-api-lighthouse-missing", args: {}, detail: "API nicht verbunden", retryable: false } }));
  await expect(page.getByTestId("integ-status-psi")).toHaveText("Error: Lighthouse runtime not found");
});

// spec: INTEG-090
test("GA4 account, property and data stream are dependent and saved on OK", async ({ page }) => {
  await page.evaluate(() => window.integFixture.open("ga4"));
  const dialog = page.getByRole("dialog", { name: "API access" });
  await dialog.getByRole("button", { name: "Connect a new Google account" }).click();
  await page.evaluate(() => window.integFixture.finish("ga4", { account: "analyst@integ.test" }));
  await dialog.getByRole("button", { name: /Account/ }).click();
  await page.getByRole("option", { name: "Agency" }).click();
  await dialog.getByRole("button", { name: /Property/ }).click();
  await expect(page.getByRole("option")).toHaveText(["Agency Site (properties/2001)"]);
  await page.getByRole("option", { name: "Agency Site (properties/2001)" }).click();
  await expect(page.getByTestId("integ-status-ga4")).toHaveText("Connected (analyst@integ.test / Agency / Agency Site)");
  await dialog.getByRole("button", { name: /Account/ }).click();
  await page.getByRole("option", { name: "Empty" }).click();
  await expect(dialog.getByText("No GA4 properties found for Empty.")).toBeVisible();
  await dialog.getByRole("button", { name: /Account/ }).click();
  await page.getByRole("option", { name: "Integ Shop" }).click();
  await dialog.getByRole("button", { name: /Property/ }).click();
  await page.getByRole("option", { name: "Shop Web (properties/1001)" }).click();
  await dialog.getByRole("button", { name: /Data stream/ }).click();
  await page.getByRole("option", { name: "Blog (blog.integ.test)" }).click();
  await dialog.getByRole("button", { name: "OK" }).click();
  await expect(page.getByRole("dialog")).toHaveCount(0);
  const saved = await calls(page, "api.setSettings");
  expect(saved.at(-1)?.args).toMatchObject({ kind: "ga4", settings: { values: { property: "properties/1001", stream: "streams/2" } } });
});

// spec: INTEG-007
test("API progress appears with a connected integration and explains each source in its tooltip", async ({ page }) => {
  await expect(page.getByTestId("api-progress")).toHaveCount(0);
  await page.evaluate((rows) => window.integFixture.setRows(rows), fixture.status as never);
  const button = page.getByRole("button", { name: /^API: 65 %/ });
  await expect(button).toBeVisible();
  // Tastaturfokus zeigt den Tooltip (Zeigermodalität ist im kopflosen Browser nicht verlässlich); feste Uhr: Verzögerung vorspulen.
  await page.keyboard.press("Tab");
  await expect(button).toBeFocused();
  await page.clock.runFor(400);
  await expect(page.getByRole("tooltip")).toHaveText("GA4 done (0 errors), GSC 400/400 (0 errors), PSI 120/400 (3 errors)");
  await page.evaluate(() => window.integFixture.setRows([]));
  await expect(page.getByTestId("api-progress")).toHaveCount(0);
});

// spec: INTEG-012
test("APIs mode replaces nothing without a connected integration and starts the deduplicated list otherwise", async ({ page }) => {
  await page.evaluate(() => window.integFixture.action("mode.apis"));
  const bar = page.getByRole("region", { name: "APIs mode" });
  await bar.getByRole("textbox", { name: "URLs (one per line)" }).fill(fixture.apiList);
  await expect(bar.getByText("4 valid, 0 invalid, 1 duplicates")).toBeVisible();
  await bar.getByRole("button", { name: "Fetch API data" }).click();
  await expect(bar.getByText("Connect at least one integration first. Nothing was started.")).toBeVisible();
  expect(await calls(page, "crawl.start")).toEqual([]);
  await bar.getByRole("button", { name: "API access…" }).click();
  const dialog = page.getByRole("dialog", { name: "API access" });
  await dialog.getByRole("tab", { name: /PageSpeed Insights/ }).click();
  await dialog.getByLabel("API key").fill("psi-key");
  await dialog.getByRole("button", { name: "Connect", exact: true }).click();
  await page.evaluate(() => window.integFixture.finish("psi", { account: "psi" }));
  await expect(page.getByTestId("integ-status-psi")).toHaveText("Connected (psi)");
  await dialog.getByRole("button", { name: "Cancel" }).click();
  await expect(bar.getByText("Data from: PageSpeed Insights")).toBeVisible();
  await bar.getByRole("button", { name: "Fetch API data" }).click();
  await expect.poll(async () => (await calls(page, "crawl.start")).length).toBe(1);
  expect((await calls(page, "crawl.start"))[0]?.args).toEqual({
    source: { kind: "apis", items: ["https://integ.test:8080/a/", "https://integ.test:8080/b/", "https://integ.test:8080/c", "https://integ.test:8080/d?x=1"] },
    scope: "domain",
  });
});
