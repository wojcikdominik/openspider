import { test, expect } from "@playwright/test";
import type {} from "./harness";

test.beforeEach(async ({ page }) => {
  await page.goto("/e2e/crawls/index.html");
  await page.getByRole("button", { name: "Manage", exact: true }).focus();
  await page.getByRole("button", { name: "Manage", exact: true }).click();
  await expect(page.getByRole("checkbox", { name: "Select: Alpha" })).toBeVisible();
});

// spec: UI-124
test("Escape restores focus, project tree works by keyboard and Mod+F focuses search", async ({ page }) => {
  const tree = page.getByRole("treegrid", { name: "Projects" });
  await tree.focus(); await page.keyboard.press("Home"); await page.keyboard.press("ArrowDown"); await page.keyboard.press("Space");
  await expect(page.getByRole("checkbox", { name: "Select: Alpha" })).toHaveCount(0);
  const mod = process.platform === "darwin" ? "Meta" : "Control";
  await page.keyboard.press(`${mod}+f`);
  await expect(page.getByRole("searchbox")).toBeFocused();
  await page.keyboard.press("Escape");
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Manage", exact: true })).toBeFocused();
});

// spec: CRAWL-143
test("keyboard context menu respects running state; delete always confirms", async ({ page }) => {
  const live = page.getByRole("checkbox", { name: "Select: Live" }); await live.check(); await live.focus();
  await page.keyboard.press("Shift+F10");
  await expect(page.getByRole("menuitem", { name: "Export crawl…", exact: true })).toHaveAttribute("aria-disabled", "true");
  await page.keyboard.press("Escape"); await live.uncheck();
  const alpha = page.getByRole("checkbox", { name: "Select: Alpha" }); await alpha.check(); await alpha.focus();
  await page.keyboard.press("Delete");
  const confirm = page.getByRole("dialog", { name: "Delete", exact: true });
  await expect(confirm).toContainText("Deleted crawls cannot be recovered.");
  expect(await page.evaluate(() => window.crawlsFixture.calls().filter(c => c.cmd === "crawls.delete"))).toEqual([]);
  await confirm.getByRole("button", { name: "Cancel" }).click();
  await expect(confirm).toHaveCount(0);
});

// spec: OUT-191 [M2]
test("project, task and start URL searches combine and sort retains database IDs", async ({ page }) => {
  await page.getByRole("searchbox").fill("kunde a");
  await expect(page.getByRole("checkbox")).toHaveCount(3);
  await page.getByRole("button", { name: "Name", exact: true }).click();
  await page.getByRole("checkbox", { name: "Select all filtered crawls" }).check();
  await page.getByRole("button", { name: "Copy database ID" }).click();
  await expect.poll(() => page.evaluate(() => window.crawlsFixture.calls().filter(c => c.cmd === "crawls.copyIds").length)).toBe(1);
  expect(await page.evaluate(() => window.crawlsFixture.calls().find(c => c.cmd === "crawls.copyIds")?.args)).toEqual({ ids: ["0199f000-0000-7000-8000-000000000001", "0199f000-0000-7000-8000-000000000002"] });
});

// spec: CRAWL-127
test("import cancellation leaves the core untouched, progress persists outside the management dialog", async ({ page }) => {
  await page.evaluate(() => window.crawlsFixture.setPaths(null));
  await page.getByRole("button", { name: "Import crawls…" }).click();
  expect(await page.evaluate(() => window.crawlsFixture.calls().filter(c => c.cmd === "crawls.import"))).toEqual([]);
  await page.evaluate(() => window.crawlsFixture.setPaths(["/fixture/Alpha.ospider"]));
  await page.getByRole("button", { name: "Import crawls…" }).click();
  await expect(page.getByRole("progressbar")).toBeVisible();
  await expect.poll(() => page.getByRole("dialog").evaluate(dialog => dialog.contains(document.activeElement))).toBe(true);
  await page.keyboard.press("Escape"); await expect(page.getByRole("dialog")).toHaveCount(0);
  await expect(page.getByRole("progressbar")).toBeVisible();
  await page.evaluate(() => window.crawlsFixture.complete());
  await expect(page.getByRole("status")).toContainText("Operation completed.");
});

// spec: OUT-230 [M2]
test("a corrupt file offers repair and displays the resulting new file with row loss", async ({ page }) => {
  await page.evaluate(() => window.crawlsFixture.corrupt());
  await page.getByRole("button", { name: "Open crawl…", exact: true }).click();
  await page.getByRole("button", { name: "Attempt repair", exact: true }).click();
  await expect(page.getByRole("progressbar")).toBeVisible();
  await page.evaluate(() => window.crawlsFixture.complete({ path: "/fixture/Alpha.repaired.ospider", integrityOk: true, usable: true, lostRows: 2 }));
  const result = page.getByRole("dialog", { name: "Repair result" });
  await expect(result).toContainText("/fixture/Alpha.repaired.ospider");
  await expect(result).toContainText("Repaired copy is usable.");
});

// spec: OUT-228
test("Shift selection includes the intervening rows and copies IDs in table order", async ({ page }) => {
  await page.getByRole("button", { name: "Name", exact: true }).click();
  await page.getByRole("checkbox", { name: "Select: Alpha" }).check();
  await page.getByRole("checkbox", { name: "Select: Failed" }).click({ modifiers: ["Shift"] });
  await expect(page.getByRole("checkbox", { name: "Select: Beta" })).toBeChecked();
  await page.getByRole("button", { name: "Copy database ID" }).click();
  await expect.poll(() => page.evaluate(() => window.crawlsFixture.calls().filter(c => c.cmd === "crawls.copyIds").length)).toBe(1);
  expect(await page.evaluate(() => window.crawlsFixture.calls().find(c => c.cmd === "crawls.copyIds")?.args)).toEqual({ ids: ["0199f000-0000-7000-8000-000000000001", "0199f000-0000-7000-8000-000000000002", "0199f000-0000-7000-8000-000000000006"] });
});

// B4-PROJDLG: the form dialogs open on top of the management dialog. Real key presses (no `fill`, no `pressSequentially`,
// which focus the field by script) must reach the field, both right after opening and after a click into the field.
// spec: OUT-228
// spec: UI-124
test("typing reaches the create project, rename and move forms and enables Apply", async ({ page }) => {
  const calls = (cmd: string) => page.evaluate(cmd => window.crawlsFixture.calls().filter(c => c.cmd === cmd).map(c => c.args), cmd);
  const typeInto = async (dialogName: string, role: "textbox" | "combobox", fieldName: string, text: string, how: "direct" | "click") => {
    const form = page.getByRole("dialog", { name: dialogName, exact: true });
    await expect(form).toBeVisible();
    const field = form.getByRole(role, { name: fieldName, exact: true });
    await expect(field).toBeFocused();
    if (how === "click") {
      // Like a user: click into the field, clear it and type.
      await field.click();
      await page.keyboard.press("ControlOrMeta+a");
      await page.keyboard.press("Backspace");
    }
    await page.keyboard.type(text, { delay: 20 });
    await expect(field).toBeFocused();
    await expect(field).toHaveValue(text);
    const apply = form.getByRole("button", { name: "Apply", exact: true });
    await expect(apply).toBeEnabled();
    return { form, field, apply };
  };

  // The same applies to the management dialog's own search field.
  await page.getByRole("searchbox").click();
  await page.keyboard.type("Alpha", { delay: 20 });
  await expect(page.getByRole("searchbox")).toHaveValue("Alpha");
  await page.getByRole("searchbox").click();
  await page.keyboard.press("ControlOrMeta+a");
  await page.keyboard.press("Backspace");
  await expect(page.getByRole("searchbox")).toHaveValue("");

  await page.getByRole("button", { name: "Create project", exact: true }).click();
  const create = await typeInto("Create project", "textbox", "Name", "Kunde B", "direct");
  await create.apply.click();
  await expect(create.form).toHaveCount(0);
  await expect.poll(() => calls("projects.create")).toEqual([{ name: "Kunde B" }]);

  // The current name is preselected, typing replaces it; Enter applies.
  await expect(page.getByRole("button", { name: "Rename project", exact: true })).toBeEnabled();
  await page.getByRole("button", { name: "Rename project", exact: true }).click();
  const renameProject = await typeInto("Rename project", "textbox", "Name", "Kunde C", "direct");
  await page.keyboard.press("Enter");
  await expect(renameProject.form).toHaveCount(0);
  await expect.poll(() => calls("projects.rename")).toEqual([{ id: expect.any(String), name: "Kunde C" }]);

  await page.getByRole("treegrid", { name: "Projects" }).getByText("All projects", { exact: true }).click();
  await page.getByRole("checkbox", { name: "Select: Alpha" }).check();
  await page.getByRole("button", { name: "Rename", exact: true }).click();
  const rename = await typeInto("Rename", "textbox", "Name", "Alpha neu", "click");
  await rename.apply.click();
  await expect(rename.form).toHaveCount(0);
  await expect.poll(() => calls("crawls.rename")).toEqual([{ crawlId: "0199f000-0000-7000-8000-000000000001", name: "Alpha neu" }]);

  await page.getByRole("checkbox", { name: "Select: Alpha neu" }).check();
  await page.getByRole("button", { name: "Move to project", exact: true }).click();
  const move = await typeInto("Move to project", "combobox", "Destination project", "Kunde C", "click");
  await move.apply.click();
  await expect(move.form).toHaveCount(0);
  await expect.poll(() => calls("crawls.move")).toEqual([{ ids: ["0199f000-0000-7000-8000-000000000001"], project: "Kunde C" }]);
});

// B4-PROJDLG: Escape closes only the form, focus returns to its trigger inside the still open management dialog.
// spec: UI-124
test("Escape in a form closes only the form and returns focus to its trigger", async ({ page }) => {
  const trigger = page.getByRole("button", { name: "Create project", exact: true });
  await trigger.click();
  const form = page.getByRole("dialog", { name: "Create project", exact: true });
  await form.getByRole("textbox", { name: "Name", exact: true }).click();
  await page.keyboard.type("x");
  await page.keyboard.press("Escape");
  await expect(form).toHaveCount(0);
  await expect(page.getByRole("dialog", { name: "Manage crawls…", exact: true })).toBeVisible();
  await expect(trigger).toBeFocused();
  await page.keyboard.press("Escape");
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Manage", exact: true })).toBeFocused();
});

// CRAWL-127 (OS-Festlegung 09.11.2026): recent entries stay on one path line, shortened in the middle to the available
// width; the file name stays visible, the buttons are equally wide and left-aligned.
// spec: CRAWL-127
test("recent entries show name and date, shorten long paths in the middle and keep the file name visible", async ({ page }) => {
  const long = "/Users/fixture/Library/Application Support/de.openspider.app/some/deeply/nested/folder/structure/crawls/01a18894-1234-7000-8000-bbfe3c06b178.ospider";
  await page.evaluate(paths => localStorage.setItem("openspider.recent-crawls.v1", JSON.stringify(paths)), ["/fixture/Alpha.ospider", long]);
  await page.reload();
  await page.getByRole("button", { name: "Manage", exact: true }).click();
  const nav = page.getByRole("navigation", { name: "Recently opened" });
  const known = nav.getByRole("button", { name: /^Alpha – Oct 1, 2026, .*, \/fixture\/Alpha\.ospider$/ });
  const unknown = nav.getByRole("button", { name: long, exact: true });
  await expect(known).toBeVisible();
  await expect(unknown.locator("..")).toHaveAttribute("title", long);
  await page.setViewportSize({ width: 640, height: 800 });
  const layout = await unknown.evaluate(button => {
    const box = (el: Element) => el.getBoundingClientRect();
    const path = button.querySelector(".os-crawls-recent-path")!;
    const head = path.firstElementChild as HTMLElement;
    const tail = path.lastElementChild as HTMLElement;
    return {
      button: box(button), other: box(button.closest("ul")!.querySelector("button")!), body: box(button.closest(".os-dialog-body")!),
      pathHeight: box(path).height, lineHeight: parseFloat(getComputedStyle(path).lineHeight) || parseFloat(getComputedStyle(path).fontSize) * 1.5,
      headCut: head.scrollWidth > head.clientWidth, tailRight: box(tail).right, tailText: tail.textContent,
      ellipsis: getComputedStyle(head).textOverflow, align: getComputedStyle(button).alignItems,
    };
  });
  expect(layout.button.width).toBeCloseTo(layout.other.width, 0);
  expect(layout.button.left).toBeCloseTo(layout.other.left, 0);
  expect(layout.align).toBe("flex-start");
  expect(layout.pathHeight).toBeLessThan(layout.lineHeight * 1.6);
  expect(layout.headCut).toBe(true);
  expect(layout.ellipsis).toBe("ellipsis");
  expect(layout.tailText).toBe("/crawls/01a18894-1234-7000-8000-bbfe3c06b178.ospider");
  expect(layout.tailRight).toBeLessThanOrEqual(layout.button.right);
  expect(layout.button.right).toBeLessThanOrEqual(layout.body.right);
});
