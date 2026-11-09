import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, it } from "vitest";
import { Providers } from "../../app/Providers";
import { setLanguage } from "../../i18n";
import { createClient, setClient } from "../../ipc/client";
import { MockCore } from "../../ipc/transport/mock";
import { useSession } from "../../state/session";
import { useSidebarNavigation } from "../sidebar/frame/state";
import fixture from "../../../e2e/sidebar/sidebar-dto.json";
import tick from "../../ipc/__fixtures__/Tick.json";
import type { CrawlStatus } from "../../ipc/gen/CrawlStatus";
import { Dashboard } from "./Dashboard";
import { UpperTable } from "../table/UpperTable";
import { useTabs } from "../../state/tabs";
afterEach(() => { cleanup(); setClient(null); });
// spec: UI-061
// spec: UI-062
// spec: UI-004 [P1]
it("renders fixture counts and navigates from top problems using the existing filter contract", async () => {
  await setLanguage("en");
  const core = new MockCore();
  core.handlers.set("sidebar.overview", () => fixture.overview);
  core.handlers.set("sidebar.issues", () => fixture.issues);
  setClient(createClient(core.transport()));
  useSession.setState({ crawl: tick.crawl as CrawlStatus });
  render(<Providers><Dashboard /></Providers>);
  await screen.findByRole("heading", { name: "Status distribution" });
  const issue = fixture.issues.filter(r => r.count > 0)[0]!;
  const button = await screen.findByRole("button", { name: new RegExp(`${issue.tab}: ${issue.filter}`) });
  fireEvent.click(button);
  expect(useSidebarNavigation.getState().target?.spec).toMatchObject({ tab: issue.tab, filter: issue.filter });
  expect(screen.queryByText("Total URLs Encountered")).toBeVisible();
});

// spec: UI-004
// B4-CMPUI2 (A8): the data tab bar points at `data-panel` (`aria-controls`); the overview branch carries that id as well.
it("keeps the data panel id in the overview branch of the table region", async () => {
  await setLanguage("en");
  const core = new MockCore();
  core.handlers.set("sidebar.overview", () => fixture.overview);
  core.handlers.set("sidebar.issues", () => fixture.issues);
  setClient(createClient(core.transport()));
  useSession.setState({ crawl: tick.crawl as CrawlStatus });
  useTabs.setState({ view: "overview" });
  render(<Providers><UpperTable /></Providers>);
  await screen.findByRole("heading", { name: "Status distribution" });
  const panel = document.getElementById("data-panel");
  expect(panel).not.toBeNull();
  expect(panel!.querySelector("[data-dashboard]")).not.toBeNull();
  useTabs.setState({ view: "tabs" });
});
