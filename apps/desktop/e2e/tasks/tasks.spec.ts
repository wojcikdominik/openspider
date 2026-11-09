import { test, expect } from "@playwright/test";
import type {} from "./harness";

test.use({ timezoneId: "Europe/Berlin" });
test.beforeEach(async ({ page }) => {
  await page.clock.setFixedTime(new Date("2026-10-20T12:00:00Z"));
  await page.goto("/e2e/tasks/index.html"); await page.waitForFunction(() => !!window.taskFixture);
});

// spec: OUT-161 [P1]
test("an imported New York instant displays and edits in Berlin time", async ({ page }) => {
  await page.evaluate(() => window.taskFixture.prepareOnceImport());
  await page.getByRole("button", { name: "Scheduling", exact: true }).click();
  await page.getByRole("button", { name: "Import tasks…", exact: true }).click();
  await expect.poll(() => page.evaluate(() => window.taskFixture.calls().some(c => c.cmd === "tasks.import"))).toBe(true);
  await page.evaluate(() => window.taskFixture.complete());
  await page.getByRole("checkbox", { name: "Select Nightly" }).check();
  await page.getByRole("button", { name: "Edit task", exact: true }).click();
  const input = page.getByLabel("Start date and time", { exact: true });
  await expect(input).toHaveValue("2026-11-01T15:00");
  await input.fill("2026-11-01T15:01");
  await page.getByRole("button", { name: "Save", exact: true }).click();
  const args = await page.evaluate(() => window.taskFixture.calls().filter(c => c.cmd === "tasks.save").at(-1)!.args);
  expect(args).toMatchObject({ task: { schedule: { kind: "once", at: "2026-11-01T15:01:00+01:00" } } });
});

// spec: OUT-160
test("an OS-scheduled run can be aborted without a manual start", async ({ page }) => {
  await page.evaluate(() => window.taskFixture.scheduledRun());
  await page.getByRole("button", { name: "Scheduling", exact: true }).click();
  await page.getByRole("button", { name: "Abort run", exact: true }).click();
  const calls = await page.evaluate(() => window.taskFixture.calls());
  expect(calls.find(c => c.cmd === "op.cancel")?.args).toEqual({ opId: "planned-operation" });
  expect(calls.some(c => c.cmd === "tasks.run")).toBe(false);
});

// spec: OUT-160
// spec: OUT-161 [P1]
test("task editor tabs support keyboard navigation and persist edits", async ({ page }) => {
  await page.getByRole("button", { name: "Scheduling", exact: true }).click();
  await page.getByRole("checkbox", { name: "Select Nightly" }).check();
  await page.getByRole("button", { name: "Edit task", exact: true }).click();
  const editor = page.getByRole("dialog", { name: "Edit task", exact: true });
  await editor.getByLabel("Task name", { exact: true }).fill("Nightly edited");
  const tabs = editor.getByRole("tablist", { name: "Task settings", exact: true });
  await expect(tabs.getByRole("tab")).toHaveText(["General", "Spider", "APIs", "Exports"]);
  await tabs.getByRole("tab", { name: "General", exact: true }).focus();
  await page.keyboard.press("ArrowRight");
  await expect(tabs.getByRole("tab", { name: "Spider", exact: true })).toBeFocused();
  await page.keyboard.press("Enter");
  await expect(editor.getByLabel(/Auth profile/)).toHaveValue("shop-login");
  await editor.getByLabel(/Auth profile/).fill("profiles/imported.osauth");
  await tabs.getByRole("tab", { name: "Exports", exact: true }).click();
  await expect(editor.getByRole("tablist", { name: "Export settings", exact: true }).getByRole("tab")).toHaveText(["Output settings", "Tables", "Local files", "Looker Studio", "Templates / presets", "Notifications"]);
  await editor.getByRole("button", { name: "Save", exact: true }).click();
  await expect(page.getByRole("cell", { name: "Nightly edited", exact: true })).toBeVisible();
  const saves = await page.evaluate(() => window.taskFixture.calls().filter(c => c.cmd === "tasks.save"));
  expect(saves).toHaveLength(1);
  expect(saves[0].args).toMatchObject({ task: { crawl: { profile: "shop", authProfile: "profiles/imported.osauth" } } });
});

// spec: OUT-210 [M2]
// spec: OUT-211 [M2]
test("Custom Crawl Overview preserves a canonical ordered metric list", async ({ page }) => {
  await page.getByRole("button", { name: "Scheduling", exact: true }).click();
  await page.getByRole("checkbox", { name: "Select Nightly" }).check();
  await page.getByRole("button", { name: "Edit task", exact: true }).click();
  await page.getByRole("tab", { name: "Exports", exact: true }).click();
  await page.getByRole("tab", { name: "Looker Studio", exact: true }).click();
  await page.getByText("Custom Crawl Overview", { exact: true }).click();
  await page.getByRole("button", { name: "Internal:HTML", exact: true }).click();
  await page.getByRole("button", { name: "Response Codes:Client Error (4xx)", exact: true }).click();
  await page.getByRole("button", { name: "Move Response Codes:Client Error (4xx) up", exact: true }).click();
  await page.getByRole("button", { name: "Show selection as text", exact: true }).click();
  await expect(page.getByLabel(/Metric names/)).toHaveValue("Response Codes:Client Error (4xx)\nInternal:HTML");
});

// spec: OUT-169 [P1]
// spec: OUT-171 [P1]
test("run history and abort are available without a pause action", async ({ page }) => {
  await page.getByRole("button", { name: "Scheduling", exact: true }).click();
  await page.getByRole("checkbox", { name: "Select Nightly" }).check();
  await page.getByRole("button", { name: "Run now", exact: true }).click();
  await page.getByRole("button", { name: "Abort run", exact: true }).click();
  expect(await page.evaluate(() => window.taskFixture.calls().some(c => c.cmd === "op.cancel"))).toBe(true);
  await page.getByRole("button", { name: "Run history", exact: true }).click();
  await expect(page.getByRole("cell", { name: "Completed with warnings", exact: true })).toBeVisible();
  await expect(page.getByRole("cell", { name: "Aborted", exact: true })).toBeVisible();
  await expect(page.getByRole("columnheader", { name: "URLs crawled", exact: true })).toBeVisible();
  await expect(page.getByRole("cell", { name: "1,234", exact: true })).toBeVisible();
  const warning = page.getByRole("row").filter({ has: page.getByRole("cell", { name: "crawl-list", exact: true }) });
  await expect(warning.getByRole("cell", { name: "output/list-run", exact: true })).toBeVisible();
  await warning.getByRole("button", { name: "Open run log", exact: true }).click();
  const calls = await page.evaluate(() => window.taskFixture.calls());
  expect(calls.find(c => c.cmd === "tasks.openLog")?.args).toEqual({ id: "run-list" });
  expect(calls.some(c => c.cmd === "diag.openLogs")).toBe(false);
  await expect(page.getByRole("button", { name: /pause/i })).toHaveCount(0);
});

// spec: OUT-280 [P1]
test("plaintext SMTP cannot be saved without explicit confirmation", async ({ page }) => {
  await page.getByRole("button", { name: "Add SMTP account", exact: true }).click();
  await page.getByLabel("Account label", { exact: true }).fill("Test SMTP");
  await page.getByLabel("SMTP host", { exact: true }).fill("smtp.example.test");
  await page.getByLabel("Username", { exact: true }).fill("test");
  await page.getByLabel("Sender address", { exact: true }).fill("test@example.test");
  await page.getByRole("combobox", { name: "Encryption", exact: true }).selectOption("none");
  await expect(page.getByRole("button", { name: "Save account", exact: true })).toBeDisabled();
  await page.getByText(/I confirm sending credentials/).click();
  await expect(page.getByRole("button", { name: "Save account", exact: true })).toBeEnabled();
});

// B4-PROJDLG: the task editor opens on top of the scheduling dialog (as its React sibling). Real key presses after a
// click into a field (no `fill`) must reach the field.
// spec: OUT-161 [P1]
test("typing reaches the task editor opened from the scheduling dialog", async ({ page }) => {
  await page.getByRole("button", { name: "Scheduling", exact: true }).click();
  await page.getByRole("checkbox", { name: "Select Nightly" }).check();
  await page.getByRole("button", { name: "Edit task", exact: true }).click();
  const editor = page.getByRole("dialog", { name: "Edit task", exact: true });
  for (const [label, text] of [["Task name", "Nightly typed"], ["Project name (defaults to task name)", "Kunde B"]] as const) {
    const field = editor.getByLabel(label, { exact: true });
    await field.click();
    await page.keyboard.press("ControlOrMeta+a");
    await page.keyboard.press("Backspace");
    await page.keyboard.type(text, { delay: 20 });
    await expect(field).toBeFocused();
    await expect(field).toHaveValue(text);
  }
  await editor.getByRole("button", { name: "Save", exact: true }).click();
  await expect(page.getByRole("cell", { name: "Nightly typed", exact: true })).toBeVisible();
  const saves = await page.evaluate(() => window.taskFixture.calls().filter(c => c.cmd === "tasks.save"));
  expect(saves.at(-1)?.args).toMatchObject({ task: { name: "Nightly typed", project: "Kunde B" } });
});
