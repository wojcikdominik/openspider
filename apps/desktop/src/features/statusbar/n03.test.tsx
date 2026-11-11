import { afterEach, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { Statusbar } from "./Statusbar";
import { setupHeader, tick } from "../header/fixtures/setup";
import { useSession } from "../../state/session";
import { setLanguage } from "../../i18n";

afterEach(cleanup);
// spec: CRAWL-145
// spec: UI-013
it.each([false, true])("keeps the Crawl-delay explanation visible after notices are consumed (applied=%s)", async applied => {
  setupHeader(); await setLanguage("en");
  useSession.setState({ crawlDelays: {} });
  useSession.getState().applyTick({ ...tick, notices: [{ level: "info", code: "crawl-delay", args: { origin: "https://site.test", seconds: 1, applied } }] });
  useSession.getState().clearNotices();
  render(<Statusbar />);
  expect(screen.getByText(applied ? /Rate limit: Crawl-delay 1.0 s.*applied/ : /Crawl-delay 1.0 s.*not applied/)).toBeVisible();
});

// spec: UI-080 [M2]
// spec: UI-082 [M2]
it.each([
  ["analysis-completed", "Analysis completed"],
  ["analysis-cancelled", "Analysis cancelled"],
  ["analysis-failed", "Analysis failed"],
])("retains terminal analysis feedback for %s after its notice is dismissed", async (code, text) => {
  setupHeader(); await setLanguage("en");
  useSession.getState().applyTick({ ...tick, notices: [{ level: "info", code, args: {} }] });
  useSession.getState().clearNotices();
  render(<Statusbar />);
  expect(screen.getByTestId("analysis-feedback")).toHaveTextContent(text);
});

// spec: CRAWL-145
// spec: UI-013
it.each(["en", "de"])("shows the original and capped effective delay in %s", async language => {
  setupHeader(); await setLanguage(language);
  useSession.setState({ crawlDelays: {} });
  useSession.getState().applyTick({ ...tick, notices: [{ level: "info", code: "crawl-delay", args: {
    origin: "https://site.test", seconds: 120, effectiveSeconds: 60, applied: true, capped: true,
  } }] });
  useSession.getState().clearNotices();
  render(<Statusbar />);
  expect(screen.getByText(language === "en" ? /Crawl-delay 120.0 s.*capped to 60.0 s.*applied/ : /Crawl-delay 120,0 s.*auf 60,0 s gekappt.*angewandt/)).toBeVisible();
});

// spec: CRAWL-145
// spec: UI-013
it("shows each Crawl-delay cause once, even when several robots.txt origins report it", async () => {
  setupHeader(); await setLanguage("de");
  useSession.setState({ crawlDelays: {} });
  const delay = (origin: string, seconds: number) => ({ level: "info" as const, code: "crawl-delay", args: { origin, seconds, applied: true, effectiveSeconds: seconds } });
  useSession.getState().applyTick({ ...tick, notices: [delay("https://site.test", 1), delay("http://site.test", 1), delay("https://social.test", 1), delay("https://slow.test", 5)] });
  // The same notice again (feed refresh) must not add an entry either.
  useSession.getState().pushNotice(delay("https://site.test", 1));
  useSession.getState().clearNotices();
  render(<Statusbar />);
  const entries = screen.getAllByText(/Crawl-delay/);
  expect(entries.map(e => e.textContent?.replace(/[⁦-⁩]/g, ""))).toEqual([
    "Begrenzung: Crawl-delay 1,0 s in robots.txt – angewandt",
    "Begrenzung: Crawl-delay 5,0 s in robots.txt – angewandt",
  ]);
  expect(entries[0]).toHaveAttribute("title", "site.test, social.test");
  expect(entries[1]).toHaveAttribute("title", "slow.test");
});
