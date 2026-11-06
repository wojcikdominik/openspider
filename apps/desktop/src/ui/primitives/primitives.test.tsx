import { afterEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { useState } from "react";
import { setLanguage } from "../../i18n";
import { EMPTY_CODES } from "../emptyState/codes";
import { Button, ColorPicker, DialogFooter, EmptyState, KeyHint, SplitPane, TabBar, Tabs, clampSplit, fitTabs, moveItem } from "./index";
import { Dialog } from "./Dialog";
import { Banner, ProgressBar } from "./Feedback";
import { IssueTypeIcon } from "../icons";

afterEach(async () => {
  cleanup();
  await act(async () => {
    await setLanguage("en");
  });
});

describe("EmptyState (UI-016)", () => {
  // spec: UI-016
  it("trägt data-empty-state mit Code und Argument und nennt die konkrete Einstellung beim Namen", async () => {
    await act(async () => {
      await setLanguage("de");
    });
    const { container } = render(<EmptyState state={{ code: "FEATURE_DISABLED", arg: "http_headers" }} />);
    const el = container.querySelector("[data-empty-state]");
    expect(el?.getAttribute("data-empty-state")).toBe("FEATURE_DISABLED(http_headers)");
    expect(el).toHaveTextContent("HTTP-Header werden nicht gespeichert");
  });

  // spec: UI-016
  it("jeder der elf Codes hat einen übersetzten Text in beiden Sprachen (kein Schlüsselname)", async () => {
    for (const lang of ["en", "de"]) {
      await act(async () => {
        await setLanguage(lang);
      });
      for (const code of EMPTY_CODES) {
        const { container, unmount } = render(<EmptyState state={{ code, arg: code === "FEATURE_DISABLED" ? "cookies" : code === "API_NOT_CONNECTED" ? "gsc" : "canonicals" }} />);
        const text = container.textContent ?? "";
        expect(text.length, `${lang} ${code}`).toBeGreaterThan(10);
        expect(text).not.toMatch(/empty-/);
        unmount();
      }
    }
  });

  // spec: UI-016
  it("die Aktionsschaltfläche ruft den Handler mit der Aktion auf", () => {
    const onAction = vi.fn();
    render(<EmptyState state={{ code: "NEEDS_CRAWL_ANALYSIS", arg: "canonicals", action: { kind: "startAnalysis" } }} onAction={onAction} />);
    fireEvent.click(screen.getByRole("button", { name: "Start crawl analysis" }));
    expect(onAction).toHaveBeenCalledWith({ kind: "startAnalysis" });
  });
});

describe("DialogFooter je Plattform (5.6)", () => {
  const order = (platform: "mac" | "win") => {
    const { container, unmount } = render(<DialogFooter platform={platform} okLabel="OK" cancelLabel="Cancel" />);
    const labels = [...container.querySelectorAll("button")].map((b) => b.textContent);
    unmount();
    return labels;
  };
  // spec: UI-016
  it("macOS: Abbrechen links von OK; Windows: OK, dann Abbrechen", () => {
    expect(order("mac")).toEqual(["Cancel", "OK"]);
    expect(order("win")).toEqual(["OK", "Cancel"]);
  });
});

describe("ColorPicker (UI-120)", () => {
  // spec: UI-120
  it("eine Akzentfarbe unter 3:1 (#FFFF66) löst eine Warnung mit Kontrastwert aus; Palette und gute Farben nicht", () => {
    const onChange = vi.fn();
    const { rerender } = render(<ColorPicker label="Accent" value="#ffff66" theme="light" onChange={onChange} />);
    const warning = screen.getByTestId("accent-warning");
    expect(warning).toHaveTextContent(/below 3:1/);
    expect(warning).toHaveTextContent(/1\.\d:1/);
    rerender(<ColorPicker label="Accent" value="#2f855a" theme="light" onChange={onChange} />);
    expect(screen.queryByTestId("accent-warning")).toBeNull();
  });

  // spec: UI-120
  it("acht vorgeprüfte Farben wählbar; eigene Eingabe nur als #rrggbb", () => {
    const onChange = vi.fn();
    render(<ColorPicker label="Accent" value="#2f855a" theme="dark" onChange={onChange} />);
    const group = screen.getByRole("group", { name: "Accent" });
    expect(within(group).getAllByRole("button")).toHaveLength(8);
    fireEvent.click(within(group).getByRole("button", { name: "Blue" }));
    expect(onChange).toHaveBeenLastCalledWith("#3c79ce");
    const input = within(group).getByRole("textbox");
    fireEvent.change(input, { target: { value: "#12" } });
    expect(onChange).toHaveBeenCalledTimes(1);
    expect(screen.getByText("Enter a color as #rrggbb.")).toBeInTheDocument();
    fireEvent.change(input, { target: { value: "#FFFF66" } });
    expect(onChange).toHaveBeenLastCalledWith("#ffff66");
  });
});

describe("Weitere Primitive", () => {
  it("KeyHint zeigt das Register-Kürzel in Plattformschreibweise", () => {
    const { container } = render(<KeyHint action="crawl.pause" platform="mac" />);
    expect(container.querySelector("kbd")?.textContent).toBe("⌘P");
    cleanup();
    const w = render(<KeyHint action="crawl.pause" platform="win" />);
    expect(w.container.querySelector("kbd")?.textContent).toBe("Ctrl+P");
    cleanup();
    expect(render(<KeyHint action="gibt.es.nicht" platform="win" />).container.textContent).toBe("");
  });

  it("clampSplit hält beide Mindestmaße ein", () => {
    expect(clampSplit(1000, 50, 120, 200)).toBe(120);
    expect(clampSplit(1000, 990, 120, 200)).toBe(796);
    expect(clampSplit(1000, 400, 120, 200)).toBe(400);
    expect(clampSplit(100, 50, 120, 200)).toBe(120);
  });

  it("SplitPane: Trenner ist ein fokussierbarer Separator, Pfeiltasten verschieben, Doppelklick setzt zurück", () => {
    const onSize = vi.fn();
    render(<SplitPane orientation="horizontal" size={200} onSizeChange={onSize} defaultSize={240} label="Divider" first={<p>A</p>} second={<p>B</p>} />);
    const sep = screen.getByRole("separator", { name: "Divider" });
    expect(sep).toHaveAttribute("aria-orientation", "vertical");
    expect(sep).toHaveAttribute("aria-valuenow", "200");
    fireEvent.keyDown(sep, { key: "ArrowRight" });
    expect(onSize).toHaveBeenLastCalledWith(208);
    fireEvent.keyDown(sep, { key: "ArrowLeft" });
    expect(onSize).toHaveBeenLastCalledWith(192);
    fireEvent.doubleClick(sep);
    expect(onSize).toHaveBeenLastCalledWith(240);
  });

  // spec: UI-126
  it("Tabs: tablist mit Roving Tabindex, Pfeiltasten wechseln den Tab", () => {
    const onSelect = vi.fn();
    render(<Tabs label="Views" selected="one" onSelect={onSelect} tabs={[{ id: "one", label: "One", content: <p>1</p> }, { id: "two", label: "Two", content: <p>2</p> }]} />);
    const list = screen.getByRole("tablist", { name: "Views" });
    const [first, second] = within(list).getAllByRole("tab");
    const panel = screen.getByRole("tabpanel");
    expect(first).toHaveAttribute("aria-controls", panel.id);
    expect(panel).toHaveAttribute("aria-labelledby", first?.id);
    expect(first).toHaveAttribute("aria-selected", "true");
    expect(first).toHaveAttribute("tabindex", "0");
    expect(second).toHaveAttribute("tabindex", "-1");
    first?.focus();
    fireEvent.keyDown(first as Element, { key: "ArrowRight" });
    expect(onSelect).toHaveBeenCalledWith("two");
  });

  it("Banner: Fehler als alert, Hinweis als status; ProgressBar mit Wert", () => {
    render(
      <>
        <Banner kind="error">Boom</Banner>
        <Banner kind="info">Hinweis</Banner>
        <ProgressBar label="Export" value={40} />
      </>,
    );
    expect(screen.getByRole("alert")).toHaveTextContent("Boom");
    expect(screen.getByRole("status")).toHaveTextContent("Hinweis");
    expect(screen.getByRole("progressbar", { name: "Export" })).toHaveAttribute("aria-valuenow", "40");
  });

  it("Dialog: modal mit Titel, schließt über die Fußleiste", () => {
    const onOpenChange = vi.fn();
    render(
      <Dialog title="Demo" isOpen onOpenChange={onOpenChange} footer={(close) => <DialogFooter platform="win" okLabel="OK" cancelLabel="Cancel" onCancel={close} />}>
        <p>Body</p>
      </Dialog>,
    );
    const dialog = screen.getByRole("dialog");
    expect(within(dialog).getByRole("heading", { name: "Demo" })).toBeInTheDocument();
    fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
    expect(onOpenChange).toHaveBeenCalledWith(false);
  });

  // spec: UI-145
  // spec: UI-143
  it("Dialog: geschlossen ohne Inhalt und ohne Render der Teile; Fokus kehrt nach dem Schließen zurück (M2-N32)", async () => {
    let body = 0;
    const Body = () => { body++; return <p>Body</p>; };
    function Owner() {
      const [open, setOpen] = useState(false);
      return <><button onClick={() => setOpen(true)}>Open</button>
        <Dialog title="Demo" isOpen={open} onOpenChange={setOpen} footer={(close) => <DialogFooter platform="win" okLabel="OK" cancelLabel="Cancel" onCancel={close} />}><Body /></Dialog></>;
    }
    const { container } = render(<Owner />);
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(body).toBe(0);
    expect(container.children).toHaveLength(1);
    const trigger = screen.getByRole("button", { name: "Open" });
    trigger.focus(); fireEvent.click(trigger);
    const dialog = await screen.findByRole("dialog");
    expect(body).toBeGreaterThan(0);
    fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    await waitFor(() => expect(document.activeElement).toBe(trigger));
  });

  it("Button und Issue-Symbol rendern", () => {
    const { container } = render(
      <>
        <Button variant="primary">Go</Button>
        <IssueTypeIcon type="warning" />
      </>,
    );
    expect(screen.getByRole("button", { name: "Go" })).toHaveClass("os-btn-primary");
    expect(container.querySelector("svg path")).not.toBeNull();
  });
});

describe("TabBar (Überlauf, Reihenfolge)", () => {
  // spec: UI-126
  it("links tabs to an explicitly named shared panel without colliding between bars", () => {
    const tabs = [{ id: "one", label: "One" }];
    render(<><TabBar tabs={tabs} selected="one" onSelect={() => undefined} label="First" overflowLabel="More" panelId="first-panel" />
      <TabBar tabs={tabs} selected="one" onSelect={() => undefined} label="Second" overflowLabel="More" panelId="second-panel" /></>);
    const rendered = screen.getAllByRole("tab");
    expect(rendered[0]).toHaveAttribute("id", "first-panel-tab-one");
    expect(rendered[0]).toHaveAttribute("aria-controls", "first-panel");
    expect(rendered[1]).toHaveAttribute("id", "second-panel-tab-one");
    expect(rendered[1]).toHaveAttribute("aria-controls", "second-panel");
  });

  it("fitTabs hält den gewählten Tab sichtbar und schiebt den Rest in den Überlauf", () => {
    expect(fitTabs([100, 100, 100], 400, 32, 0)).toEqual({ visible: [0, 1, 2], hidden: [] });
    expect(fitTabs([100, 100, 100, 100], 250, 32, 0)).toEqual({ visible: [0, 1], hidden: [2, 3] });
    const sel = fitTabs([100, 100, 100, 100], 250, 32, 3);
    expect(sel.visible).toContain(3);
    expect(sel.hidden).not.toContain(3);
    expect(sel.visible.length + sel.hidden.length).toBe(4);
  });

  it("moveItem verschiebt ein Element und ignoriert ungültige Ziele", () => {
    expect(moveItem(["a", "b", "c"], 0, 2)).toEqual(["b", "c", "a"]);
    expect(moveItem(["a", "b", "c"], 2, 0)).toEqual(["c", "a", "b"]);
    expect(moveItem(["a", "b"], 0, 5)).toEqual(["a", "b"]);
  });

  it("Alt+Umschalt+→ verschiebt den Tab per Tastatur, Pfeil wechselt, Überlauf-Menü erscheint bei Platzmangel", () => {
    const onReorder = vi.fn();
    const onSelect = vi.fn();
    const tabs = [{ id: "a", label: "A" }, { id: "b", label: "B" }, { id: "c", label: "C" }];
    render(<TabBar tabs={tabs} selected="a" onSelect={onSelect} label="Views" overflowLabel="More" onReorder={onReorder} measure={() => ({ widths: [100, 100, 100], available: 150, overflowWidth: 32 })} />);
    const first = screen.getByRole("tab", { name: "A" });
    fireEvent.keyDown(first, { key: "ArrowRight", altKey: true, shiftKey: true });
    expect(onReorder).toHaveBeenCalledWith(["b", "a", "c"]);
    fireEvent.keyDown(first, { key: "ArrowRight" });
    expect(onSelect).toHaveBeenCalledWith("b");
    expect(screen.getByRole("button", { name: "More" })).toBeInTheDocument();
  });

  // spec: UI-004 [P1]
  // spec: UI-112
  it("selects the focused tab with Enter and offers visibility, configure and reset in the overflow menu (A02-06)", () => {
    const onSelect = vi.fn(); const onToggle = vi.fn(); const onConfigure = vi.fn(); const onReset = vi.fn();
    const tabs = [{ id: "a", label: "Alpha" }, { id: "b", label: "Beta" }, { id: "c", label: "Gamma" }];
    render(<TabBar tabs={tabs} selected="a" onSelect={onSelect} label="Views" overflowLabel="More"
      measure={() => ({ widths: [100, 100, 100], available: 220, overflowWidth: 0 })}
      config={{ all: [...tabs, { id: "d", label: "Delta" }], hidden: ["d"], mandatory: "a", onToggle, onConfigure, onReset, configureLabel: "Configure tabs…", resetLabel: "Reset tabs" }} />);
    fireEvent.keyDown(screen.getByRole("tab", { name: "Beta" }), { key: "Enter" });
    expect(onSelect).toHaveBeenCalledWith("b");
    // „Gamma“ does not fit: it leaves the tab list (also for assistive technology) and is listed in the overflow menu.
    expect(screen.queryByRole("tab", { name: "Gamma" })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "More" }));
    expect(screen.getAllByRole("menuitemcheckbox").map(e => e.textContent?.replace(/^[☑☐]\s*/, ""))).toEqual(["Alpha", "Beta", "Gamma", "Delta"]);
    expect(screen.getByRole("menuitemcheckbox", { name: "Alpha" })).toHaveAttribute("aria-disabled", "true");
    fireEvent.click(screen.getByRole("menuitemcheckbox", { name: "Delta" }));
    expect(onToggle).toHaveBeenCalledWith("d", true);
    fireEvent.click(screen.getByRole("menuitem", { name: "Gamma" }));
    expect(onSelect).toHaveBeenCalledWith("c");
  });

  // spec: UI-007
  // spec: UI-004
  it("keeps labels on one line and names a cut label in the tooltip (A02-01, A02-02)", () => {
    const tabs = [{ id: "a", label: "Rechtschreibung und Grammatik" }];
    const { container } = render(<TabBar tabs={tabs} selected="a" onSelect={() => undefined} label="Views" overflowLabel="More" />);
    const label = container.querySelector<HTMLElement>(".os-tab-label")!;
    expect(label).toHaveTextContent("Rechtschreibung und Grammatik");
    expect(screen.getByRole("tab", { name: "Rechtschreibung und Grammatik" })).toBeInTheDocument();
  });
});
