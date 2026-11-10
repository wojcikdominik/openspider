// Unabhängige Abnahme B4-ENTER, Komponentenebene (jsdom). Ergänzt e2e/acceptance/b4-enter.spec.ts um die Fälle, die
// Playwright nicht erzeugen kann (IME-Ereignisfolgen, Vorschlagsliste ohne native Auswahl) und um den Zustand
// „unterbrochen“. Erwartungen stammen allein aus der Spec: UI-003 (Enter im URL-Feld entspricht „Start“, Rückfrage vor dem
// erneuten Crawlen) und UI-113 (Mod+Enter, Kontextregeln, OS-Festlegung 09.11.2026: pausiert/unterbrochen + unveränderte
// Adresse = fortsetzen ohne Rückfrage und ohne neuen Crawl; leeres Feld bzw. unbekannte Start-URL gilt als unverändert).
import { afterEach, beforeEach, expect, it } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { Header } from "./Header";
import { useHeader } from "./model";
import { setupHeader, tick } from "./fixtures/setup";
import { useSession } from "../../state/session";
import { setClient } from "../../ipc/client";
import { setLanguage } from "../../i18n";
import { runAction } from "../../ui/actions";
import { installShortcutDispatcher } from "../../ui/dispatcher";

let core: ReturnType<typeof setupHeader>;
beforeEach(async () => { core = setupHeader(); await setLanguage("en"); });
afterEach(() => { cleanup(); setClient(null); });

const calls = (cmd: string) => core.calls.filter((c) => c.cmd === cmd);
const field = () => screen.getByLabelText("Start URL");
const typeUrl = (value: string) => fireEvent.change(field(), { target: { value } });
// The session ignores ticks that do not advance `seq`; every call is a new tick.
let seq = 100;
const phase = (p: "paused" | "incomplete" | "completed" | "running", startUrl?: string | null) =>
  act(() => useSession.getState().applyTick({ ...tick, seq: ++seq, crawl: { ...tick.crawl!, phase: p, ...(startUrl === undefined ? {} : { startUrl }) } }));
/** Lets pending promises (validate, start, uistate) settle so that a call that must NOT happen has had its chance. */
const settle = () => act(async () => { await new Promise((r) => setTimeout(r, 30)); });
/** `fireEvent` returns false when a handler called preventDefault. */
const enter = (init: KeyboardEventInit = {}) => fireEvent.keyDown(field(), { key: "Enter", code: "Enter", keyCode: 13, ...init });

// spec: UI-113
it("does not start while an IME composition is open or being committed, and starts with the next plain Enter", async () => {
  render(<Header />);
  typeUrl("example.test");
  fireEvent.compositionStart(field());
  expect(enter({ isComposing: true })).toBe(true);
  expect(enter({ isComposing: true, keyCode: 229 })).toBe(true);
  fireEvent.compositionEnd(field(), { data: "x" });
  // Safari: the committing Enter arrives after compositionend with keyCode 229 and isComposing false.
  expect(enter({ keyCode: 229, isComposing: false })).toBe(true);
  await settle();
  expect(calls("crawl.validateStartUrl")).toHaveLength(0);
  expect(calls("crawl.start")).toHaveLength(0);
  expect(enter()).toBe(false);
  await waitFor(() => expect(calls("crawl.start")).toHaveLength(1));
  expect(calls("crawl.start")[0]?.args).toEqual({ source: { kind: "spider", url: "https://example.test/" }, scope: "domain" });
});

// spec: UI-113
it("does not resume a paused crawl or ask the Replace question on an IME Enter", async () => {
  phase("paused", "https://site.test/");
  render(<Header />);
  typeUrl("other.test");
  expect(enter({ isComposing: true })).toBe(true);
  expect(enter({ keyCode: 229 })).toBe(true);
  await settle();
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(calls("crawl.resume")).toHaveLength(0);
});

// spec: UI-003
// spec: UI-113
it("asks before replacing results: Enter, Cancel, Enter again, Replace", async () => {
  phase("completed", "https://site.test/");
  render(<Header />);
  typeUrl("next.test");
  enter();
  const first = await screen.findByRole("dialog");
  expect(first).toBeVisible();
  expect(calls("crawl.start")).toHaveLength(0);
  fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  expect(calls("crawl.start")).toHaveLength(0);
  // The validated (normalized) address stays in the field.
  expect(field()).toHaveValue("https://next.test/");
  enter();
  expect(await screen.findByRole("dialog")).toBeVisible();
  fireEvent.click(screen.getByRole("button", { name: "Replace" }));
  await waitFor(() => expect(calls("crawl.start")).toHaveLength(1));
  expect(calls("crawl.start")[0]?.args).toEqual({ source: { kind: "spider", url: "https://next.test/" }, scope: "domain" });
});

// spec: UI-003
// spec: UI-113
it("with suggestions present, Enter still starts the second and third crawl; arrow keys hand exactly one Enter to the list", async () => {
  core.uiState.state = { header: { history: ["https://old.test/", "https://older.test/"] } };
  render(<Header />);
  await waitFor(() => expect(document.querySelectorAll("datalist option")).toHaveLength(2));
  for (const url of ["one.test", "two.test", "three.test"]) {
    typeUrl(url);
    enter();
    await waitFor(() => expect(calls("crawl.start").at(-1)?.args).toMatchObject({ source: { url: `https://${url}/` } }));
  }
  expect(calls("crawl.start")).toHaveLength(3);
  expect([...document.querySelectorAll("datalist option")].map((o) => o.getAttribute("value")).slice(0, 3))
    .toEqual(["https://three.test/", "https://two.test/", "https://one.test/"]);

  // Arrow key, then Enter: the picker adopts an entry (the browser then fires `input`), nothing starts with that Enter.
  for (const arrow of ["ArrowDown", "ArrowUp"]) {
    const before = calls("crawl.start").length;
    typeUrl("");
    fireEvent.keyDown(field(), { key: arrow, keyCode: arrow === "ArrowDown" ? 40 : 38 });
    expect(enter()).toBe(true);
    typeUrl("https://old.test/");
    await settle();
    expect(calls("crawl.start")).toHaveLength(before);
    // The following Enter is the start.
    expect(enter()).toBe(false);
    await waitFor(() => expect(calls("crawl.start")).toHaveLength(before + 1));
    expect(calls("crawl.start").at(-1)?.args).toMatchObject({ source: { url: "https://old.test/" } });
  }
});

// spec: UI-113
it("after the arrow keys, Escape or typing releases Enter again", async () => {
  core.uiState.state = { header: { history: ["https://old.test/"] } };
  render(<Header />);
  await waitFor(() => expect(document.querySelectorAll("datalist option")).toHaveLength(1));
  typeUrl("next.test");
  fireEvent.keyDown(field(), { key: "ArrowDown", keyCode: 40 });
  fireEvent.keyDown(field(), { key: "Escape", keyCode: 27 });
  expect(enter()).toBe(false);
  await waitFor(() => expect(calls("crawl.start")).toHaveLength(1));
  fireEvent.keyDown(field(), { key: "ArrowDown", keyCode: 40 });
  typeUrl("again.test");
  expect(enter()).toBe(false);
  await waitFor(() => expect(calls("crawl.start")).toHaveLength(2));
});

// spec: UI-113
it("Escape in the URL field starts nothing", async () => {
  render(<Header />);
  typeUrl("next.test");
  expect(fireEvent.keyDown(field(), { key: "Escape", keyCode: 27 })).toBe(true);
  await settle();
  expect(calls("crawl.validateStartUrl")).toHaveLength(0);
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(field()).toHaveValue("next.test");
});

for (const state of ["paused", "incomplete"] as const) {
  // spec: UI-113
  // spec: UI-003
  it(`${state} crawl, unchanged address: Enter resumes without a question and without a new crawl`, async () => {
    phase(state, "https://site.test/blog/");
    render(<Header />);
    // The field carries the start URL of the crawl (UI-003 OS-Festlegung B3-UI).
    await waitFor(() => expect(field()).toHaveValue("https://site.test/blog/"));
    enter();
    await waitFor(() => expect(calls("crawl.resume")).toHaveLength(1));
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(calls("crawl.start")).toHaveLength(0);
    expect(calls("crawl.validateStartUrl")).toHaveLength(0);
    // Same normalized address in another spelling counts as unchanged.
    typeUrl("HTTPS://Site.Test/blog/");
    enter();
    await waitFor(() => expect(calls("crawl.resume")).toHaveLength(2));
    expect(calls("crawl.start")).toHaveLength(0);
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  // spec: UI-113
  it(`${state} crawl: an empty field or a crawl without known start URL counts as unchanged`, async () => {
    phase(state, "https://site.test/");
    render(<Header />);
    await waitFor(() => expect(field()).toHaveValue("https://site.test/"));
    typeUrl("");
    enter();
    await waitFor(() => expect(calls("crawl.resume")).toHaveLength(1));
    cleanup();
    useHeader.setState({ url: "", mode: "spider" });
    phase(state, null);
    render(<Header />);
    typeUrl("whatever.test");
    enter();
    await waitFor(() => expect(calls("crawl.resume")).toHaveLength(2));
    expect(calls("crawl.start")).toHaveLength(0);
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  // spec: UI-113
  // spec: UI-003
  it(`${state} crawl, changed address: Enter asks first (nothing resumed), then starts a new crawl`, async () => {
    phase(state, "https://site.test/blog/");
    render(<Header />);
    await waitFor(() => expect(field()).toHaveValue("https://site.test/blog/"));
    typeUrl("https://elsewhere.test/");
    enter();
    expect(await screen.findByRole("dialog")).toBeVisible();
    await settle();
    expect(calls("crawl.resume")).toHaveLength(0);
    expect(calls("crawl.start")).toHaveLength(0);
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(calls("crawl.start")).toHaveLength(0);
    expect(calls("crawl.resume")).toHaveLength(0);
    enter();
    fireEvent.click(await screen.findByRole("button", { name: "Replace" }));
    await waitFor(() => expect(calls("crawl.start")).toHaveLength(1));
    expect(calls("crawl.start")[0]?.args).toEqual({ source: { kind: "spider", url: "https://elsewhere.test/" }, scope: "domain" });
    expect(calls("crawl.resume")).toHaveLength(0);
  });
}

// spec: UI-113
it("Mod+Enter outside a text field resumes a paused crawl with an unchanged address, asks with a changed one", async () => {
  const off = installShortcutDispatcher({ platform: "win" });
  try {
    phase("paused", "https://site.test/blog/");
    render(<Header />);
    await waitFor(() => expect(field()).toHaveValue("https://site.test/blog/"));
    expect(document.activeElement).toBe(document.body);
    fireEvent.keyDown(document.body, { key: "Enter", code: "Enter", keyCode: 13, ctrlKey: true });
    await waitFor(() => expect(calls("crawl.resume")).toHaveLength(1));
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(calls("crawl.start")).toHaveLength(0);
    typeUrl("https://elsewhere.test/");
    fireEvent.keyDown(document.body, { key: "Enter", code: "Enter", keyCode: 13, ctrlKey: true });
    expect(await screen.findByRole("dialog")).toBeVisible();
    expect(calls("crawl.resume")).toHaveLength(1);
    expect(calls("crawl.start")).toHaveLength(0);
  } finally { off(); }
});

// spec: UI-113
it("the start action (Mod+Enter) follows the state: nothing while running, Replace question with results", async () => {
  render(<Header />);
  phase("running", "https://site.test/");
  act(() => { runAction("crawl.start"); });
  await settle();
  expect(calls("crawl.start")).toHaveLength(0);
  expect(calls("crawl.resume")).toHaveLength(0);
  expect(screen.queryByRole("dialog")).toBeNull();
  phase("completed", "https://site.test/");
  typeUrl("next.test");
  act(() => { runAction("crawl.start"); });
  expect(await screen.findByRole("dialog")).toBeVisible();
  expect(calls("crawl.start")).toHaveLength(0);
});

// spec: UI-003
it("Enter while a crawl is running starts nothing", async () => {
  phase("running", "https://site.test/");
  render(<Header />);
  typeUrl("next.test");
  enter();
  await settle();
  expect(calls("crawl.start")).toHaveLength(0);
  expect(calls("crawl.resume")).toHaveLength(0);
  expect(screen.queryByRole("dialog")).toBeNull();
});
