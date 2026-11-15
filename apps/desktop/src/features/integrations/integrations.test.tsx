import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { createClient, setClient } from "../../ipc/client";
import type { ApiConnectArgs } from "../../ipc/gen/ApiConnectArgs";
import type { ApiKind } from "../../ipc/gen/ApiKind";
import type { ApiRow } from "../../ipc/gen/ApiRow";
import type { ApiSetSettingsArgs } from "../../ipc/gen/ApiSetSettingsArgs";
import type { ApiSourceInfo } from "../../ipc/gen/ApiSourceInfo";
import type { CmdError } from "../../ipc/gen/CmdError";
import type { JsonValue } from "../../ipc/gen/serde_json/JsonValue";
import { MockCore } from "../../ipc/transport/mock";
import { setLanguage } from "../../i18n";
import { useSession } from "../../state/session";
import { runAction } from "../../ui/actions";
import { allMenuContributions } from "../../ui/menu/registry";
import { ApiProgress } from "./ApiProgress";
import { ApiSourceGear, IntegrationsHeaderSlot, IntegrationsHost } from "./IntegrationsHost";
import { registerPropertyCatalog, type Ga4Account, type GscSite } from "./properties";
import { openApiAccess, setApiMode, setCrawlUrl, useIntegrations } from "./store";
import fixture from "./fixtures/integ-ui.json";

const KEY = "sk-TEST-INTEG-002";
let core: MockCore;
let sources: ApiSourceInfo[];
let settings: Record<string, JsonValue>;
let rows: ApiRow[];
let confirmOnce: boolean;
let opSeq = 0;
const calls = (cmd: string) => core.calls.filter((c) => c.cmd === cmd);
const patch = (kind: ApiKind, change: Partial<ApiSourceInfo>) => {
  sources = sources.map((s) => (s.kind === kind ? { ...s, ...change } : s));
};
const connectedAs = (kind: ApiKind, account: string, accounts = [{ kind, account, accountId: `${kind}-1`, scopes: [] }]) =>
  patch(kind, { state: "connected", connected: true, account, accounts, error: null });

function finishOp(opId: string, state: "done" | "failed", error?: CmdError) {
  act(() => {
    useSession.setState((s) => ({ ops: { ...s.ops, [opId]: { opId, labelKey: "op-api-connect", done: 1, total: 1, state, error: error ?? null } } }));
  });
}

function renderHost() {
  return render(
    <>
      <IntegrationsHost />
      <IntegrationsHeaderSlot />
    </>,
  );
}

async function openAt(kind: ApiKind) {
  act(() => openApiAccess(kind));
  return screen.findByRole("dialog", { name: "API access" });
}

const status = (kind: ApiKind) => screen.getByTestId(`integ-status-${kind}`);

beforeEach(async () => {
  core = new MockCore();
  setClient(createClient(core.transport("main")));
  await setLanguage("en");
  sources = structuredClone(fixture.sources) as ApiSourceInfo[];
  settings = structuredClone(fixture.settings) as Record<string, JsonValue>;
  rows = [];
  confirmOnce = false;
  useSession.setState({ ops: {}, crawl: null, sidebarVersion: 0, countsVersion: 0 });
  useIntegrations.setState({ dialog: null, apiMode: false, crawlUrl: null });
  core.handlers.set("api.list", () => structuredClone(sources));
  core.handlers.set("api.status", () => structuredClone(rows));
  core.handlers.set("api.connect", (args) => {
    const a = args as ApiConnectArgs;
    patch(a.kind, { state: "connecting" });
    return { opId: `op-${++opSeq}`, labelKey: "op-api-connect", cancellable: true };
  });
  core.handlers.set("api.disconnect", (args) => {
    patch((args as { kind: ApiKind }).kind, { state: "disconnected", connected: false, account: null, error: null });
    return null;
  });
  core.handlers.set("api.getSettings", (args) => {
    const kind = (args as { kind: ApiKind }).kind;
    return { kind, values: structuredClone(settings[kind]) };
  });
  core.handlers.set("api.setSettings", (args) => {
    const a = args as ApiSetSettingsArgs;
    if (confirmOnce && a.decision === "ask") return { invalidated: false, confirmationRequired: true, dataStale: false };
    settings[a.kind] = a.settings.values;
    return { invalidated: a.decision === "reload", confirmationRequired: false, dataStale: a.decision === "keep" };
  });
  core.handlers.set("crawl.parseList", (args) => {
    const text = (args as { text: string }).text;
    const lines = text.split("\n").filter((l) => l.trim());
    return { valid: new Set(lines).size, invalid: 0, duplicates: lines.length - new Set(lines).size, sample: [] };
  });
});
afterEach(() => {
  cleanup();
  setClient(null);
  registerPropertyCatalog({})();
});

describe("API-Zugriffsdialog", () => {
  // spec: INTEG-001
  it("listet die sechs Integrationen in fester Reihenfolge, jede mit genau einem Status", async () => {
    renderHost();
    const dialog = await openAt("ga4");
    const tabs = within(within(dialog).getByRole("tablist", { name: "Integrations" })).getAllByRole("tab");
    expect(tabs.map((t) => t.textContent)).toEqual([
      "Google Analytics 4Not connected",
      "Search ConsoleNot connected",
      "PageSpeed InsightsNot connected",
      "MajesticNot connected",
      "AhrefsNot connected",
      "MozNot connected",
    ]);
    expect(within(dialog).getByRole("tab", { name: /Google Analytics 4/ })).toHaveAttribute("aria-selected", "true");
  });

  // spec: INTEG-001
  it("Verbinden → Verbinde… → Verbunden; genau ein Test-Request je Klick; Trennen → Nicht verbunden", async () => {
    renderHost();
    await openAt("moz");
    await waitFor(() => expect(status("moz")).toHaveTextContent("Not connected"));
    fireEvent.change(screen.getByLabelText("API key"), { target: { value: KEY } });
    fireEvent.click(screen.getByRole("button", { name: "Connect" }));
    await waitFor(() => expect(status("moz")).toHaveTextContent("Connecting…"));
    expect(calls("api.connect")).toHaveLength(1);
    expect(calls("api.connect")[0]?.args).toEqual({ kind: "moz", apiKey: KEY, remember: true, accountId: null, clientSecret: null });
    expect(screen.queryByRole("button", { name: "Connect" })).not.toBeInTheDocument();
    connectedAs("moz", "moz", [{ kind: "moz", account: "moz", accountId: "default", scopes: [], keySuffix: "abcd" } as never]);
    finishOp(`op-${opSeq}`, "done");
    await waitFor(() => expect(status("moz")).toHaveTextContent("Connected (moz)"));
    expect(screen.getByRole("tab", { name: /Moz/ })).toHaveTextContent("Connected (moz)");
    expect(screen.getByText(/••••••••abcd/)).toBeInTheDocument();
    expect(screen.queryByDisplayValue(KEY)).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Disconnect" }));
    await waitFor(() => expect(status("moz")).toHaveTextContent("Not connected"));
    // Die Schaltfläche verschwindet; der Fokus bleibt im Dialog (WebKit fokussiert angeklickte Schaltflächen nicht).
    expect(status("moz")).toHaveFocus();
    expect(calls("api.disconnect")[0]?.args).toEqual({ kind: "moz" });
    expect(calls("api.connect")).toHaveLength(1);
  });

  // spec: INTEG-001
  it("zeigt den Anbieterfehler als `Fehler: <Code> <Meldung>`; aus Fehler führt nur Trennen heraus", async () => {
    await setLanguage("de");
    renderHost();
    act(() => openApiAccess("majestic"));
    await screen.findByRole("dialog", { name: "API-Zugriff" });
    fireEvent.change(await screen.findByLabelText("API-Schlüssel"), { target: { value: "bad" } });
    fireEvent.click(screen.getByRole("button", { name: "Verbinden" }));
    const error: CmdError = { code: "Permission", messageKey: "err-api-reconnect", args: {}, detail: "HTTP 401: Invalid token", retryable: false };
    patch("majestic", { state: "error", connected: false, error });
    finishOp(`op-${opSeq}`, "failed", error);
    await waitFor(() => expect(status("majestic")).toHaveTextContent("Fehler: 401 Invalid token"));
    expect(screen.queryByRole("button", { name: "Verbinden" })).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Trennen" }));
    await waitFor(() => expect(status("majestic")).toHaveTextContent("Nicht verbunden"));
  });

  // spec: INTEG-001
  it("speichert Integrationsoptionen ohne Schlüssel oder Tokens", async () => {
    renderHost();
    await openAt("ahrefs");
    fireEvent.change(await screen.findByLabelText("API key"), { target: { value: KEY } });
    fireEvent.click(await screen.findByRole("switch", { name: "Fetch data from this source during crawls" }));
    fireEvent.click(screen.getByRole("button", { name: "OK" }));
    await waitFor(() => expect(calls("api.setSettings")).toHaveLength(1));
    const saved = calls("api.setSettings")[0]?.args as ApiSetSettingsArgs;
    expect(saved).toMatchObject({ kind: "ahrefs", decision: "ask", settings: { kind: "ahrefs", values: { enabled: true } } });
    expect(JSON.stringify(core.calls.filter((c) => c.cmd !== "api.connect"))).not.toContain(KEY);
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
  });

  // spec: INTEG-010
  it("fragt bei datenrelevanten Änderungen nach „Löschen und neu laden“ bzw. „Behalten“", async () => {
    confirmOnce = true;
    renderHost();
    await openAt("majestic");
    fireEvent.click(await screen.findByRole("switch", { name: "Fetch data from this source during crawls" }));
    fireEvent.click(screen.getByRole("button", { name: "OK" }));
    fireEvent.click(await screen.findByRole("button", { name: "Delete and reload" }));
    await waitFor(() => expect(calls("api.setSettings").map((c) => (c.args as ApiSetSettingsArgs).decision)).toEqual(["ask", "reload"]));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
  });
});

describe("GA4 und Search Console", () => {
  const ga4 = fixture.ga4Accounts as Ga4Account[];
  const gsc = fixture.gscSites as GscSite[];

  // spec: INTEG-090
  it("Konto → Property → Datenstream sind abhängig; Status nennt Konto, Property und Stream", async () => {
    registerPropertyCatalog({ ga4: async () => ga4 });
    connectedAs("ga4", "analyst@integ.test", [fixture.accounts.google as never]);
    renderHost();
    await openAt("ga4");
    fireEvent.click(await screen.findByRole("button", { name: /Property/ }));
    const options = (await screen.findAllByRole("option")).map((o) => o.textContent);
    expect(options).toEqual(["Shop Web (properties/1001)", "Shop App (properties/1002)"]);
    fireEvent.click(screen.getByRole("option", { name: "Shop Web (properties/1001)" }));
    fireEvent.click(await screen.findByRole("button", { name: /Data stream/ }));
    expect((await screen.findAllByRole("option")).map((o) => o.textContent)).toEqual(["All data streams", "Shop (integ.test)", "Blog (blog.integ.test)"]);
    fireEvent.click(screen.getByRole("option", { name: "Shop (integ.test)" }));
    await waitFor(() => expect(status("ga4")).toHaveTextContent("Connected (analyst@integ.test / Integ Shop / Shop Web / Shop)"));
    fireEvent.click(screen.getByRole("button", { name: "OK" }));
    await waitFor(() => expect(calls("api.setSettings")[0]?.args).toMatchObject({ kind: "ga4", settings: { values: { property: "properties/1001", stream: "streams/1" } } }));
  });

  // spec: INTEG-090
  it("wählt die gespeicherte Property vor und meldet Konten ohne GA4-Property", async () => {
    registerPropertyCatalog({ ga4: async () => ga4 });
    settings.ga4 = { ...(settings.ga4 as object), property: "properties/2001" };
    connectedAs("ga4", "analyst@integ.test", [fixture.accounts.google as never]);
    renderHost();
    await openAt("ga4");
    expect(await screen.findByRole("button", { name: /Account$|Account / })).toHaveTextContent("Agency");
    expect(screen.getByRole("button", { name: /Property/ })).toHaveTextContent("Agency Site (properties/2001)");
    fireEvent.click(screen.getByRole("button", { name: /Account$|Account / }));
    fireEvent.click(await screen.findByRole("option", { name: "Empty" }));
    expect(await screen.findByText("No GA4 properties found for Empty.")).toBeInTheDocument();
  });

  // spec: INTEG-090
  it("bietet ohne Property-Lader die manuelle Eingabe der Property-ID", async () => {
    connectedAs("ga4", "analyst@integ.test", [fixture.accounts.google as never]);
    renderHost();
    await openAt("ga4");
    fireEvent.change(await screen.findByRole("textbox", { name: "GA4 property ID" }), { target: { value: "properties/9" } });
    fireEvent.click(screen.getByRole("button", { name: "OK" }));
    await waitFor(() => expect(calls("api.setSettings")[0]?.args).toMatchObject({ settings: { values: { property: "properties/9" } } }));
  });

  // spec: INTEG-110
  it("beschriftet Domain/URL-Präfix, graut unbestätigte aus und warnt bei fremder Crawl-Domain", async () => {
    registerPropertyCatalog({ gsc: async () => gsc });
    connectedAs("gsc", "analyst@integ.test", [fixture.accounts.google as never]);
    setCrawlUrl("http://other.test/");
    renderHost();
    await openAt("gsc");
    fireEvent.click(await screen.findByRole("button", { name: /Property/ }));
    const options = await screen.findAllByRole("option");
    expect(options.map((o) => o.textContent)).toEqual([
      "integ.test – Domain",
      "other.test – Domain",
      "https://integ.test/ – URL prefix",
      "https://staging.integ.test/ – URL prefix (not verified)",
    ]);
    expect(options[3]).toHaveAttribute("aria-disabled", "true");
    fireEvent.click(options[0] as HTMLElement);
    expect(await screen.findByText("The selected property does not match the crawl domain.")).toBeInTheDocument();
    await waitFor(() => expect(status("gsc")).toHaveTextContent("Connected (analyst@integ.test / integ.test)"));
  });
});

describe("PageSpeed Insights", () => {
  // spec: INTEG-130
  it("steht ohne gespeicherte Auswahl auf Remote mit Schlüsselfeld; Lokal braucht keinen Schlüssel", async () => {
    renderHost();
    await openAt("psi");
    expect(await screen.findByRole("radio", { name: "Remote (PageSpeed Insights API)" })).toBeChecked();
    expect(screen.getByLabelText("API key")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Connect" })).toBeDisabled();
    fireEvent.click(screen.getByRole("radio", { name: "Local (Lighthouse on this computer)" }));
    await waitFor(() => expect(screen.queryByLabelText("API key")).not.toBeInTheDocument());
    expect(screen.getByText(/field data from CrUX is not available/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "OK" }));
    await waitFor(() => expect(calls("api.setSettings")[0]?.args).toMatchObject({ kind: "psi", settings: { values: { local: true } } }));
  });

  // spec: INTEG-132
  it("zeigt den Lighthouse-Pfad nur bei Lokal und verbindet mit dem eingegebenen Pfad", async () => {
    renderHost();
    await openAt("psi");
    expect(await screen.findByRole("radio", { name: "Remote (PageSpeed Insights API)" })).toBeChecked();
    expect(screen.queryByLabelText("Lighthouse path")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("radio", { name: "Local (Lighthouse on this computer)" }));
    const field = await screen.findByLabelText("Lighthouse path");
    expect(screen.getByText(/which lighthouse/)).toBeInTheDocument();
    fireEvent.change(field, { target: { value: "/opt/My Tools/bin/lighthouse " } });
    expect(field).toHaveValue("/opt/My Tools/bin/lighthouse ");
    fireEvent.click(screen.getByRole("button", { name: "Connect" }));
    await waitFor(() => expect(calls("api.connect")).toHaveLength(1));
    expect(calls("api.connect")[0]?.args).toMatchObject({ kind: "psi", apiKey: null, psiLocal: true, lighthousePath: "/opt/My Tools/bin/lighthouse" });
    patch("psi", { state: "error", error: { code: "Permission", messageKey: "err-api-lighthouse-missing", args: {}, detail: "API nicht verbunden", retryable: false } });
    finishOp(`op-${opSeq}`, "failed", { code: "Permission", messageKey: "err-api-lighthouse-missing", args: {}, detail: "API nicht verbunden", retryable: false } as CmdError);
    await waitFor(() => expect(status("psi")).toHaveTextContent("Error: Lighthouse runtime not found"));
    fireEvent.click(screen.getByRole("button", { name: "OK" }));
    await waitFor(() =>
      expect(calls("api.setSettings")[0]?.args).toMatchObject({ kind: "psi", settings: { values: { local: true, lighthouse_path: "/opt/My Tools/bin/lighthouse" } } }),
    );
  });

  // spec: INTEG-130
  it("zeigt bei abgelehntem Schlüssel den Grund und die Prüfliste", async () => {
    patch("psi", { state: "error", error: { code: "Permission", messageKey: "err-api-reconnect", args: {}, detail: "HTTP 403: SERVICE_DISABLED", retryable: false } });
    renderHost();
    await openAt("psi");
    await waitFor(() => expect(status("psi")).toHaveTextContent("Error: PSI access denied (SERVICE_DISABLED)"));
    expect(screen.getByText(/PageSpeed Insights API enabled in the Google Cloud project/)).toBeInTheDocument();
    expect(screen.getByText(/created just now/)).toBeInTheDocument();
    expect(screen.getByText(/restricted to certain APIs or referrers/)).toBeInTheDocument();
  });
});

describe("Linkmetriken", () => {
  // spec: INTEG-150
  it("Ebenen je Anbieter; nicht verfügbare Ebenen deaktiviert; Spaltennamen mit Ebene", async () => {
    renderHost();
    await openAt("moz");
    fireEvent.click(await screen.findByRole("tab", { name: "Metrics" }));
    expect(await screen.findByRole("checkbox", { name: "Exact URL (HTTP + HTTPS)" })).toBeDisabled();
    expect(screen.getByRole("checkbox", { name: "Exact URL" })).toBeChecked();
    fireEvent.click(screen.getByRole("checkbox", { name: "Domain" }));
    fireEvent.click(screen.getByRole("checkbox", { name: "Domain Authority" }));
    const columns = within(screen.getByRole("region", { name: "Resulting columns" }));
    expect(columns.getAllByRole("listitem").map((li) => li.textContent)).toEqual([
      "Moz Page Authority (Exact URL)",
      "Moz Page Authority (Domain)",
      "Moz Domain Authority (Exact URL)",
      "Moz Domain Authority (Domain)",
    ]);
    fireEvent.click(screen.getByRole("button", { name: "OK" }));
    await waitFor(() =>
      expect(calls("api.setSettings")[0]?.args).toMatchObject({ kind: "moz", settings: { values: { levels: ["url", "domain"], metrics: ["page_authority", "domain_authority"] } } }),
    );
  });
});

describe("Einbindung", () => {
  // spec: INTEG-007
  it("API-Fortschritt erscheint nur mit verbundener Integration und folgt den Ticks", async () => {
    rows = [{ ...(fixture.status[2] as ApiRow), connected: false }];
    render(<ApiProgress />);
    await waitFor(() => expect(calls("api.status")).toHaveLength(1));
    expect(screen.queryByTestId("api-progress")).not.toBeInTheDocument();
    rows = fixture.status as ApiRow[];
    act(() => useSession.setState({ sidebarVersion: 1 }));
    const button = await screen.findByRole("button", { name: /^API: 65 %/ });
    expect(button).toHaveAccessibleName("API: 65 %. GA4 done (0 errors), GSC 400/400 (0 errors), PSI 120/400 (3 errors)");
    rows = rows.map((r) => ({ ...r, completed: r.requested }));
    act(() => useSession.setState({ sidebarVersion: 2 }));
    expect(await screen.findByRole("button", { name: /^API: 100 %/ })).toBeInTheDocument();
  });

  // spec: UI-070
  it("das Zahnrad einer Quelle öffnet den Dialog genau dieser Quelle", async () => {
    render(
      <>
        <IntegrationsHost />
        <ApiSourceGear kind="ahrefs" />
      </>,
    );
    fireEvent.click(screen.getByRole("button", { name: "Configure Ahrefs" }));
    const dialog = await screen.findByRole("dialog", { name: "API access" });
    expect(within(dialog).getByRole("tab", { name: /Ahrefs/ })).toHaveAttribute("aria-selected", "true");
    expect(runAction("integrations.open.gsc")).toBe(true);
    await waitFor(() => expect(within(dialog).getByRole("tab", { name: /Search Console/ })).toHaveAttribute("aria-selected", "true"));
  });

  // spec: INTEG-012
  it("APIs-Modus als Kopfleisten-Variante: ohne Verbindung Hinweis statt Lauf, sonst Start mit bereinigter Liste", async () => {
    core.handlers.set("crawl.start", () => ({ crawlId: "c-apis", file: "/fixture/apis.ospider" }));
    renderHost();
    expect(screen.queryByRole("region", { name: "APIs mode" })).not.toBeInTheDocument();
    act(() => void runAction("mode.apis", { source: "menu" }));
    const bar = await screen.findByRole("region", { name: "APIs mode" });
    expect(allMenuContributions().flatMap((c) => c.items).find((i) => i.id === "mode.apis")?.checked?.({})).toBe(true);
    fireEvent.change(within(bar).getByRole("textbox", { name: "URLs (one per line)" }), { target: { value: fixture.apiList } });
    expect(within(bar).getByText("4 URLs, 1 duplicates removed")).toBeInTheDocument();
    await waitFor(() => expect(within(bar).getByText("4 valid, 0 invalid, 1 duplicates")).toBeInTheDocument());
    fireEvent.click(within(bar).getByRole("button", { name: "Fetch API data" }));
    expect(await within(bar).findByText("Connect at least one integration first. Nothing was started.")).toBeInTheDocument();
    expect(calls("crawl.start")).toHaveLength(0);
    connectedAs("psi", "psi");
    fireEvent.click(within(bar).getByRole("button", { name: "Fetch API data" }));
    await waitFor(() => expect(calls("crawl.start")).toHaveLength(1));
    expect(calls("crawl.start")[0]?.args).toEqual({
      source: { kind: "apis", items: ["https://integ.test:8080/a/", "https://integ.test:8080/b/", "https://integ.test:8080/c", "https://integ.test:8080/d?x=1"] },
      scope: "domain",
    });
    act(() => setApiMode(false));
    await waitFor(() => expect(screen.queryByRole("region", { name: "APIs mode" })).not.toBeInTheDocument());
  });
});
