import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { Notifications, isStartUnreachable, noticeMessage } from "./Notifications";
import { setupHeader } from "../header/fixtures/setup";
import { useHeader } from "../header/model";
import { useSession } from "../../state/session";
import { createI18n, loadFtlFiles, setLanguage } from "../../i18n";
import type { Notice } from "../../ipc/gen/Notice";
import fixture from "./fixtures/start-unreachable.json";

// B4-UNREACH: Dialog zur nicht erreichbaren Start-URL. Die Meldungen der Fixture entsprechen dem, was os-app nach einem als
// „Fehlgeschlagen“ beendeten Lauf sendet (Ursache aus der Start-URL-Zeile bzw. der robots.txt-Hilfszeile).
const cases = fixture as unknown as Record<"closedPort" | "unknownDomain" | "robotsNetwork" | "timeout" | "tls", Notice>;

beforeEach(async () => { setupHeader(); await setLanguage("en"); });
afterEach(() => { cleanup(); vi.restoreAllMocks(); });
function push(n: Notice) { act(() => useSession.getState().pushNotice(structuredClone(n))); }

// spec: UI-019
// spec: CRAWL-128
it.each([
  ["closedPort", /connection to the start address failed/, "Cause: Connection refused", "Address: http://127.0.0.1:9/"],
  ["unknownDomain", /could not be resolved \(DNS\)/, "Cause: DNS lookup failed", "robots.txt of the start address could not be retrieved"],
  ["robotsNetwork", /connection to the start address failed/, "Cause: Connection refused", "robots.txt of the start address could not be retrieved"],
  ["timeout", /did not respond in time/, "Cause: Response timeout", "Address: http://site.test/"],
  ["tls", /TLS certificate/, "Cause: TLS error", "Address: https://site.test/"],
] as const)("names the cause of the unreachable start URL from the core (%s)", async (name, message, cause, where) => {
  render(<Notifications />);
  push(cases[name]);
  const dialog = await screen.findByRole("dialog", { name: "Start address not reachable" });
  expect(within(dialog).getByText(message)).toBeVisible();
  expect(within(dialog).getByTestId("notify-cause")).toHaveTextContent(cause);
  expect(within(dialog).getByText(new RegExp(where.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")))).toBeVisible();
  if (cases[name].args.robots === true) expect(within(dialog).getByText(new RegExp(String(cases[name].args.url).replace(/[.]/g, "\\.")))).toBeVisible();
  // Kein stilles „0 URLs“: der Dialog sagt, dass der Crawl als fehlgeschlagen endete.
  expect(within(dialog).getByText(/ended as “Failed”/)).toBeVisible();
  // Kein Rohcode als Ursache.
  expect(dialog.textContent).not.toContain(String(cases[name].args.reason));
});

// spec: UI-019
it("shows the German cause and state", async () => {
  await setLanguage("de");
  render(<Notifications />);
  push(cases.robotsNetwork);
  const dialog = await screen.findByRole("dialog", { name: "Startadresse nicht erreichbar" });
  expect(within(dialog).getByTestId("notify-cause")).toHaveTextContent("Ursache: Verbindung abgelehnt");
  expect(within(dialog).getByText(/robots.txt der Startadresse war nicht abrufbar.*http:\/\/site\.test\/robots\.txt/)).toBeVisible();
  expect(within(dialog).getByText(/endete als „Fehlgeschlagen“/)).toBeVisible();
});

// spec: UI-019
// spec: UI-126
it("moves focus into the dialog, closes with Escape and returns focus to the previously focused control", async () => {
  render(<><button type="button">Before</button><Notifications /></>);
  const before = screen.getByRole("button", { name: "Before" });
  before.focus();
  push(cases.closedPort);
  const dialog = await screen.findByRole("dialog", { name: "Start address not reachable" });
  await waitFor(() => expect(dialog.contains(document.activeElement)).toBe(true));
  for (const name of ["Open logs", "Copy diagnostics", "Close"]) expect(within(dialog).getByRole("button", { name })).toBeEnabled();
  fireEvent.keyDown(document.activeElement!, { key: "Escape" });
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  expect(useSession.getState().notices).toHaveLength(0);
  await waitFor(() => expect(document.activeElement).toBe(before));
});

// spec: UI-019
it("offers the HTTP address only for an HTTPS start URL and never starts a crawl itself", async () => {
  const core = setupHeader();
  render(<Notifications />);
  push(cases.unknownDomain);
  fireEvent.click(await screen.findByRole("button", { name: /Use HTTP address/ }));
  expect(useHeader.getState().url).toBe("http://unbekannt.invalid/");
  expect(core.calls.some((c) => c.cmd === "crawl.start")).toBe(false);
  push(cases.closedPort);
  await screen.findByRole("dialog", { name: "Start address not reachable" });
  expect(screen.queryByRole("button", { name: /Use HTTP address/ })).toBeNull();
});

// spec: UI-019
it("keeps the generic dialog for start codes without a core cause", async () => {
  render(<Notifications />);
  push({ level: "error", code: "start-connection", args: {} });
  expect(isStartUnreachable({ level: "error", code: "start-connection", args: {} })).toBe(false);
  const dialog = await screen.findByRole("dialog", { name: "Crawl notice" });
  expect(within(dialog).queryByTestId("notify-cause")).toBeNull();
});

// spec: UI-019
// spec: UI-121
it("falls back to English for a language without these texts (es/fr/it before B4-LANG)", () => {
  const catalog = createI18n([...loadFtlFiles(), { path: "/src/features/fixture/i18n/fr.ftl", lang: "fr", feature: "features/fixture", source: "notify-close = Fermer\n" }]);
  expect(catalog.languages()).toContain("fr");
  expect(noticeMessage(catalog, "fr", cases.timeout)).toBe("The start address did not respond in time.");
  expect(catalog.translate("fr", "notify-unreachable-title")).toBe("Start address not reachable");
  expect(catalog.translate("fr", "notify-reason-connection_refused")).toBe("Connection refused");
  expect(catalog.translate("fr", "notify-cause", { cause: "x" })).toBe("Cause: x");
  // Unbekannte Codes bleiben die allgemeine Ursache, auch in einer unvollständigen Sprache.
  expect(noticeMessage(catalog, "fr", { level: "error", code: "fixture-unknown", args: {} })).toBe("Cause: fixture-unknown");
});
