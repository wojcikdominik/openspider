// B4-LAGFIX3 (UI-090): Ein Visualisierungsfenster abonniert seine Ereignisse vor dem ersten Rendern. Meldet der Kern
// „Daten geändert“ in dieser Lücke (z. B. weil die automatische Crawl-Analyse nach dem Schnappschuss schreibt), darf die
// einmalige Meldung nicht verloren gehen: Der Hinweis erscheint nach dem Einhängen trotzdem.
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { createClient, getClient, setClient } from "../../ipc/client";
import { connectWindowEvents, eventBus } from "../../ipc/events";
import { attachSession, useSession } from "../../state/session";
import { FakeVizCore } from "./fixtures/fakeCore";
import { windowLabelFor } from "./open";
import { VizWindow } from "./VizWindow";

let core: FakeVizCore;
let offs: Array<() => void> = [];

async function subscribeWindow(): Promise<{ snapshotId: string; label: string }> {
  const { snapshotId } = (await core.invoke("viz.open", { kind: "crawlTree", rootUrlId: null })) as { snapshotId: string };
  const label = windowLabelFor(snapshotId);
  setClient(createClient(core.transport(label)));
  // Wie `bootWindow`: Ereigniskanal und Sitzungszustand hängen vor dem ersten Rendern am Bus.
  offs = [connectWindowEvents(getClient(), label), attachSession(eventBus)];
  return { snapshotId, label };
}

beforeEach(() => {
  core = new FakeVizCore();
  useSession.setState({ notices: [] });
});

afterEach(() => {
  offs.forEach((off) => off());
  cleanup();
  setClient(null);
  useSession.setState({ notices: [] });
});

describe("Visualisierungsfenster: „Daten geändert“ vor dem Einhängen", () => {
  // spec: UI-090
  it("zeigt den Hinweis, wenn die Meldung zwischen Abonnement und erstem Rendern ankam", async () => {
    const { snapshotId, label } = await subscribeWindow();
    act(() => core.notifyChanged(snapshotId));
    await waitFor(() => expect(useSession.getState().notices.some((n) => n.code === "viz-data-changed")).toBe(true));
    render(<VizWindow params={{ snapshotId, kind: "crawlTree", rootUrlId: null, label }} exportDir={null} />);
    await screen.findByText("25 nodes");
    expect(screen.getByTestId("viz-data-changed")).toHaveTextContent("Data changed");
  });

  // spec: UI-090
  it("nach „Aktualisieren“ taucht die alte Meldung beim erneuten Einhängen nicht wieder auf", async () => {
    const { snapshotId, label } = await subscribeWindow();
    act(() => core.notifyChanged(snapshotId));
    await waitFor(() => expect(useSession.getState().notices.some((n) => n.code === "viz-data-changed")).toBe(true));
    const params = { snapshotId, kind: "crawlTree" as const, rootUrlId: null, label };
    const first = render(<VizWindow params={params} exportDir={null} />);
    const hint = await screen.findByTestId("viz-data-changed");
    fireEvent.click(within(hint).getByRole("button", { name: "Refresh" }));
    await waitFor(() => expect(screen.queryByTestId("viz-data-changed")).toBeNull());
    first.unmount();
    render(<VizWindow params={params} exportDir={null} />);
    await screen.findByText("25 nodes");
    expect(screen.queryByTestId("viz-data-changed")).toBeNull();
  });

  // spec: UI-090
  it("eine Meldung zu einem anderen Schnappschuss löst keinen Hinweis aus", async () => {
    const { snapshotId, label } = await subscribeWindow();
    act(() => eventBus.emit({ kind: "notice", notice: { level: "info", code: "viz-data-changed", args: { snapshotId: "other" } } }));
    render(<VizWindow params={{ snapshotId, kind: "crawlTree", rootUrlId: null, label }} exportDir={null} />);
    await screen.findByText("25 nodes");
    expect(screen.queryByTestId("viz-data-changed")).toBeNull();
  });
});
