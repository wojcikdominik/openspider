import { afterEach, expect, it } from "vitest";
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { HelpDialogs } from "./HelpDialogs";
import { licenseDocuments, searchLicenseText } from "./licenses";
import { useHelp } from "./state";
import { setLanguage } from "../../i18n";
import { createClient, setClient } from "../../ipc/client";
import { MockCore } from "../../ipc/transport/mock";
import { debugFixture } from "./__fixtures__/fx-ui-basic";

afterEach(() => { cleanup(); useHelp.setState({ page: null }); setClient(null); });

// spec: UI-002 [M3]
it("About shows NOTICE and the bundled notices verbatim and searches both", async () => {
  await setLanguage("en");
  const core = new MockCore();
  core.handlers.set("app.info", () => debugFixture.app);
  setClient(createClient(core.transport()));
  useHelp.setState({ page: "about" });
  render(<HelpDialogs />);
  await screen.findByText(/Commit fixture/);
  fireEvent.click(screen.getByText("Third-party notices"));
  for (const document of licenseDocuments) {
    const region = await screen.findByRole("region", { name: document.name });
    expect(region.querySelector("pre")?.textContent).toBe(document.text);
    expect(region).toBeVisible();
  }
  const search = screen.getByRole("searchbox", { name: "Search licenses" });
  fireEvent.change(search, { target: { value: "axe-core" } });
  expect(within(screen.getByRole("region", { name: "NOTICE" })).getByText(/axe-core 4.11.0/)).toBeVisible();
  fireEvent.change(search, { target: { value: "not-a-real-dependency-12345" } });
  expect(screen.getAllByText("No matches.")).toHaveLength(2);
  fireEvent.change(search, { target: { value: "" } });
  expect(screen.getByText(/Liberation Sans \(Regular/)).toBeVisible();
});

// spec: UI-002 [M3]
it("search preserves license paragraph context and handles case and whitespace", () => {
  const fixture = "Copyright Someone\nPermission is hereby granted\n\nOther license";
  expect(searchLicenseText(fixture, "  PERMISSION ")).toBe("Copyright Someone\nPermission is hereby granted");
  expect(searchLicenseText(fixture, " ")).toBe(fixture);
  expect(searchLicenseText(fixture, "missing")).toBe("");
});

// spec: UI-002 [M3]
it("a package-name search resolves shared license text IDs to full terms", () => {
  const fixture = "npm: fixture-package\nVersion: 1\nLicense: MIT\nLicense texts: L0001\n\n" + "=".repeat(80) + "\nLicense text L0001\n\nCopyright fixture\n\nPermission and full conditions";
  expect(searchLicenseText(fixture, "fixture-package")).toContain("Permission and full conditions");
});
