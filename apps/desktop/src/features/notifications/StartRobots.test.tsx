import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { Notifications } from "./Notifications";
import { setupHeader } from "../header/fixtures/setup";
import { useSession } from "../../state/session";
import { setLanguage } from "../../i18n";
import type { Notice } from "../../ipc/gen/Notice";
import { closeConfig, useConfigDialog } from "../config/frame/store";

// B4-STARTFIX: Meldung `start-robots`, wie os-app sie nach einem Lauf sendet, dessen Start-URL robots.txt per `Disallow` sperrt
// (Fixture `ui/fx-ui-robots-start`). Der Lauf endet als „Fertig“ (CRAWL-128); der Dialog nennt die Sperre und verweist nur auf
// die Einstellung „robots.txt ignorieren“.
const blocked: Notice = {
  level: "warning",
  code: "start-robots",
  args: { startUrl: "http://127.0.0.1:9/", url: "http://127.0.0.1:9/robots.txt", lineNumber: 2, line: "Disallow: /" },
};

let core: ReturnType<typeof setupHeader>;
beforeEach(async () => { core = setupHeader(); await setLanguage("en"); });
afterEach(async () => { cleanup(); vi.restoreAllMocks(); closeConfig(); await setLanguage("en"); });

// spec: UI-019
// spec: CRAWL-128
it("names the robots.txt block and only points to the Ignore robots.txt setting", async () => {
  render(<Notifications />);
  act(() => useSession.getState().pushNotice(structuredClone(blocked)));
  const dialog = await screen.findByRole("dialog", { name: "Crawl notice" });
  expect(within(dialog).getByText("robots.txt blocks the start address. The setting remains unchanged.")).toBeVisible();
  expect(within(dialog).getByTestId("notify-robots-file")).toHaveTextContent("Blocking robots.txt: http://127.0.0.1:9/robots.txt");
  // Die blockierende Zeile formuliert die Oberfläche selbst (Kern liefert Nummer und Regeltext getrennt).
  expect(within(dialog).getByTestId("notify-robots-line")).toHaveTextContent("Blocking rule (line 2): Disallow: /");
  // Kein Fehlschlag: weder der Titel noch der Zustand „Failed“ der nicht erreichbaren Start-URL.
  expect(dialog.textContent).not.toContain("Failed");
  fireEvent.click(within(dialog).getByRole("button", { name: "Open the “Ignore robots.txt” setting" }));
  expect(useConfigDialog.getState()).toMatchObject({ open: true, anchor: "robots.mode" });
  expect(core.calls.some((c) => c.cmd === "config.set")).toBe(false);
});

// spec: UI-019
it("shows the German texts", async () => {
  await setLanguage("de");
  render(<Notifications />);
  act(() => useSession.getState().pushNotice(structuredClone(blocked)));
  const dialog = await screen.findByRole("dialog", { name: "Crawl-Meldung" });
  expect(within(dialog).getByText(/robots\.txt sperrt die Startadresse/)).toBeVisible();
  expect(within(dialog).getByTestId("notify-robots-file")).toHaveTextContent("Sperrende robots.txt: http://127.0.0.1:9/robots.txt");
  expect(within(dialog).getByTestId("notify-robots-line")).toHaveTextContent("Sperrende Regel (Zeile 2): Disallow: /");
  expect(within(dialog).getByRole("button", { name: "Einstellung „robots.txt ignorieren“ öffnen" })).toBeVisible();
});

// spec: UI-019
it("shows the rule without a line number when the core has none", async () => {
  render(<Notifications />);
  act(() => useSession.getState().pushNotice({ ...structuredClone(blocked), args: { ...blocked.args, lineNumber: null } }));
  const dialog = await screen.findByRole("dialog", { name: "Crawl notice" });
  expect(within(dialog).getByTestId("notify-robots-line")).toHaveTextContent("Blocking rule: Disallow: /");
});
