// Unabhängige Tests (nur aus der Spezifikation) zum PageSpeed-Kontodialog mit Quelle Lokal (INTEG-130, INTEG-132):
// Feld „Pfad zu Lighthouse“ nur bei Lokal, Verbinden ohne Schlüssel, Status bei fehlender Laufzeit, Remote unverändert.
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { setLanguage } from "../../i18n";
import type { ApiConnectArgs } from "../../ipc/gen/ApiConnectArgs";
import type { ApiConnState } from "../../ipc/gen/ApiConnState";
import type { ApiSourceInfo } from "../../ipc/gen/ApiSourceInfo";
import type { CmdError } from "../../ipc/gen/CmdError";
import type { JsonValue } from "../../ipc/gen/serde_json/JsonValue";
import { SourcePanel } from "./SourcePanel";
import type { ApiSources } from "./useApiSources";

const PATH_WITH_SPACES = "C:\\Program Files\\nodejs\\node_modules\\lighthouse\\cli\\index.js";

function psiInfo(state: ApiConnState, extra: Partial<ApiSourceInfo> = {}): ApiSourceInfo {
  return { kind: "psi", labelKey: "api-psi", auth: "apiKey", connected: state === "connected", autoConnect: false, state, accounts: [], ...extra };
}

function fakeConn(state: ApiConnState, error: CmdError | null = null) {
  const connect = vi.fn<(args: ApiConnectArgs) => Promise<void>>(() => Promise.resolve());
  const conn: ApiSources = {
    sources: null,
    error: null,
    stateOf: () => state,
    errorOf: () => error,
    reload: () => Promise.resolve(),
    connect,
    disconnect: () => Promise.resolve(),
    removeAccount: () => Promise.resolve(),
  };
  return { conn, connect };
}

function settings(local: boolean, lighthousePath: string | null = null): Record<string, JsonValue> {
  return { enabled: true, device: "mobile", local, lighthouse_path: lighthousePath, metric_groups: [], limits: { retries: 5, concurrency: 5, window_s: 60 } };
}

function renderPanel(opts: { state?: ApiConnState; error?: CmdError | null; values: Record<string, JsonValue>; info?: ApiSourceInfo }) {
  const state = opts.state ?? "disconnected";
  const { conn, connect } = fakeConn(state, opts.error ?? null);
  const onChange = vi.fn<(path: readonly string[], value: JsonValue) => void>();
  render(<SourcePanel kind="psi" info={opts.info ?? psiInfo(state)} conn={conn} values={opts.values} onChange={onChange} />);
  return { connect, onChange };
}

const lighthouseMissing: CmdError = { code: "Permission", messageKey: "err-api-lighthouse-missing", args: {}, detail: "Fehler: Lighthouse-Laufzeit nicht gefunden", retryable: false } as CmdError;

afterEach(() => {
  cleanup();
});

describe("PageSpeed-Kontodialog, Quelle Lokal", () => {
  // spec: INTEG-132
  it("zeigt „Pfad zu Lighthouse“ nur bei Lokal und kein Schlüsselfeld", async () => {
    await setLanguage("de");
    renderPanel({ values: settings(true, PATH_WITH_SPACES) });
    const field = screen.getByLabelText("Pfad zu Lighthouse");
    expect(field).toHaveValue(PATH_WITH_SPACES);
    expect(screen.queryByLabelText("API-Schlüssel")).toBeNull();
    expect(screen.getByRole("button", { name: "Verbinden" })).toBeEnabled();
  });

  // spec: INTEG-130
  it("Remote zeigt kein Lighthouse-Feld, verlangt einen Schlüssel und schickt keinen Pfad", async () => {
    await setLanguage("en");
    const { connect } = renderPanel({ values: settings(false, "/usr/local/bin/lighthouse") });
    expect(screen.queryByLabelText("Lighthouse path")).toBeNull();
    const key = screen.getByLabelText("API key");
    const button = screen.getByRole("button", { name: "Connect" });
    expect(button).toBeDisabled();
    fireEvent.change(key, { target: { value: "  sk-REMOTE-1  " } });
    expect(button).toBeEnabled();
    act(() => fireEvent.click(button));
    expect(connect).toHaveBeenCalledTimes(1);
    const args = connect.mock.calls[0]![0];
    expect(args.kind).toBe("psi");
    expect(args.apiKey).toBe("sk-REMOTE-1");
    expect(args.psiLocal ?? false).toBe(false);
    expect(args.lighthousePath ?? null).toBeNull();
  });

  // spec: INTEG-130
  // spec: INTEG-132
  it("Verbinden bei Lokal sendet Quelle und getrimmten Pfad, ohne Schlüssel", async () => {
    await setLanguage("en");
    const { connect, onChange } = renderPanel({ values: settings(true) });
    const field = screen.getByLabelText("Lighthouse path");
    fireEvent.change(field, { target: { value: `  ${PATH_WITH_SPACES}  ` } });
    // Leerzeichen innerhalb des Pfads bleiben erhalten (z. B. „Program Files“).
    expect(field).toHaveValue(`  ${PATH_WITH_SPACES}  `);
    expect(onChange).toHaveBeenLastCalledWith(["lighthouse_path"], PATH_WITH_SPACES);
    act(() => fireEvent.click(screen.getByRole("button", { name: "Connect" })));
    expect(connect).toHaveBeenCalledTimes(1);
    const args = connect.mock.calls[0]![0];
    expect(args).toMatchObject({ kind: "psi", psiLocal: true, lighthousePath: PATH_WITH_SPACES });
    expect(args.apiKey ?? null).toBeNull();
  });

  // spec: INTEG-132
  it("ein leerer oder nur aus Leerzeichen bestehender Pfad gilt als nicht konfiguriert", async () => {
    await setLanguage("en");
    const { onChange } = renderPanel({ values: settings(true, "/opt/lh") });
    const field = screen.getByLabelText("Lighthouse path");
    fireEvent.change(field, { target: { value: "   " } });
    expect(onChange).toHaveBeenLastCalledWith(["lighthouse_path"], null);
    fireEvent.change(field, { target: { value: "" } });
    expect(onChange).toHaveBeenLastCalledWith(["lighthouse_path"], null);
  });

  // spec: INTEG-132
  it("fehlende Laufzeit: Status „Fehler: Lighthouse-Laufzeit nicht gefunden“", async () => {
    await setLanguage("de");
    renderPanel({ state: "error", error: lighthouseMissing, values: settings(true) });
    expect(screen.getByTestId("integ-status-psi")).toHaveTextContent(/^Fehler: Lighthouse-Laufzeit nicht gefunden$/);
  });

  // spec: INTEG-132
  it("fehlende Laufzeit auf Englisch", async () => {
    await setLanguage("en");
    renderPanel({ state: "error", error: lighthouseMissing, values: settings(true) });
    expect(screen.getByTestId("integ-status-psi")).toHaveTextContent(/^Error: Lighthouse runtime not found$/);
  });

  // spec: INTEG-132
  it("verbunden: Status nennt die Lighthouse-Version", async () => {
    await setLanguage("de");
    renderPanel({ state: "connected", values: settings(true, "/opt/lh/cli/index.js"), info: psiInfo("connected", { account: "Lighthouse 12.4.1" }) });
    expect(screen.getByTestId("integ-status-psi")).toHaveTextContent("12.4.1");
  });

  // spec: INTEG-130
  it("Umschalten auf Lokal ändert nur die Quelle", async () => {
    await setLanguage("en");
    const { onChange } = renderPanel({ values: settings(false) });
    act(() => fireEvent.click(screen.getByRole("radio", { name: /^Local/ })));
    expect(onChange).toHaveBeenCalledWith(["local"], true);
    expect(onChange.mock.calls.every(([path]) => path[0] === "local")).toBe(true);
  });
});
