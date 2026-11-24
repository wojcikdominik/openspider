// spec: UI-015
// acceptance: independent
//
// Unabhängige Abnahme des Escape-Verhaltens des Dialog-Bausteins (Sollverhalten aus dem Auftrag zu UI-015 und den
// Kontextregeln von UI-113: „In modalen Dialogen ... Esc (Dialog schließen)“). Geschrieben nur aus Sollverhalten und Spec.
import { afterEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { useState, type ReactNode } from "react";
import { Dialog, DialogFooter } from "./Dialog";
import { ComboBox } from "./ComboBox";
import { SearchField } from "./fields";

afterEach(cleanup);

const esc = (target: EventTarget, init: KeyboardEventInit = {}) => {
  const event = new KeyboardEvent("keydown", { key: "Escape", code: "Escape", keyCode: 27, bubbles: true, cancelable: true, ...init });
  act(() => { target.dispatchEvent(event); });
  return event;
};

/** Moves the focus out of every dialog onto `body` (the gap before the focus has entered a freshly opened dialog). */
const focusBody = () => act(() => { (document.activeElement as HTMLElement | null)?.blur(); });

function Host({ title = "Demo", opener = "Open", refuse = false, onChange, children, isDismissable, footer }: {
  title?: string; opener?: string; refuse?: boolean; onChange?: (open: boolean) => void; children?: ReactNode; isDismissable?: boolean; footer?: ReactNode;
}) {
  const [open, setOpen] = useState(false);
  return <>
    <button type="button" onClick={() => setOpen(true)}>{opener}</button>
    <Dialog title={title} isOpen={open} isDismissable={isDismissable} footer={footer}
      onOpenChange={value => { onChange?.(value); if (!value && refuse) return; setOpen(value); }}>
      <button type="button">Inside</button>
      {children}
    </Dialog>
  </>;
}

function open(name = "Open") {
  const trigger = screen.getByRole("button", { name });
  act(() => trigger.focus());
  fireEvent.click(trigger);
  return trigger;
}

describe("UI-015 Abnahme: Escape am Dialog-Baustein", () => {
  // spec: UI-015
  // acceptance: independent
  it("1+2: Escape closes a visible dialog while the focus is still on body and returns the focus to the trigger", async () => {
    const onChange = vi.fn();
    render(<Host onChange={onChange} />);
    const trigger = open();
    expect(screen.getByRole("dialog", { name: "Demo" })).toBeVisible();
    focusBody();
    expect(document.activeElement).toBe(document.body);
    const event = esc(document.body);
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(onChange).toHaveBeenCalledTimes(1);
    expect(onChange).toHaveBeenCalledWith(false);
    expect(event.defaultPrevented).toBe(true);
    await waitFor(() => expect(trigger).toHaveFocus());
  });

  // spec: UI-015
  // acceptance: independent
  it("1+2: Escape whose target is still the (outside) trigger closes the dialog and the focus returns to it", async () => {
    render(<Host />);
    const trigger = open();
    focusBody();
    esc(trigger);
    expect(screen.queryByRole("dialog")).toBeNull();
    await waitFor(() => expect(trigger).toHaveFocus());
  });

  // spec: UI-015
  // acceptance: independent
  it("1+2: a dialog opened by keyboard shortcut from a focused field closes on an early Escape; focus goes back to that field", async () => {
    function ShortcutHost() {
      const [open, setOpen] = useState(false);
      return <>
        <input aria-label="Table" onKeyDown={e => { if (e.key === "e" && (e.metaKey || e.ctrlKey)) { e.preventDefault(); setOpen(true); } }} />
        <Dialog title="Export options" isOpen={open} onOpenChange={setOpen}><button type="button">Inside</button></Dialog>
      </>;
    }
    render(<ShortcutHost />);
    const field = screen.getByRole("textbox", { name: "Table" });
    act(() => field.focus());
    fireEvent.keyDown(field, { key: "e", ctrlKey: true, metaKey: true });
    expect(screen.getByRole("dialog", { name: "Export options" })).toBeVisible();
    focusBody();
    esc(document.body);
    expect(screen.queryByRole("dialog")).toBeNull();
    await waitFor(() => expect(field).toHaveFocus());
  });

  // spec: UI-015
  // acceptance: independent
  it("5: Escape with the focus inside the dialog still closes it once and returns the focus to the trigger", async () => {
    const onChange = vi.fn();
    render(<Host onChange={onChange} />);
    const trigger = open();
    const inside = within(screen.getByRole("dialog")).getByRole("button", { name: "Inside" });
    act(() => inside.focus());
    esc(inside);
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(onChange).toHaveBeenCalledTimes(1);
    await waitFor(() => expect(trigger).toHaveFocus());
  });

  // spec: UI-015
  // acceptance: independent
  it("3: with nested dialogs only the topmost closes per Escape (focus outside both), then the outer one", async () => {
    function Nested() {
      const [outer, setOuter] = useState(false);
      const [inner, setInner] = useState(false);
      return <>
        <button type="button" onClick={() => setOuter(true)}>Open outer</button>
        <Dialog title="Outer" isOpen={outer} onOpenChange={setOuter}>
          <button type="button" onClick={() => setInner(true)}>Open inner</button>
          <Dialog title="Inner" isOpen={inner} onOpenChange={setInner}><button type="button">Inner button</button></Dialog>
        </Dialog>
      </>;
    }
    render(<Nested />);
    const outerTrigger = open("Open outer");
    const innerTrigger = within(screen.getByRole("dialog", { name: "Outer" })).getByRole("button", { name: "Open inner" });
    act(() => innerTrigger.focus());
    fireEvent.click(innerTrigger);
    expect(screen.getByRole("dialog", { name: "Inner" })).toBeVisible();
    focusBody();
    esc(document.body);
    expect(screen.queryByRole("dialog", { name: "Inner" })).toBeNull();
    expect(screen.getByRole("dialog", { name: "Outer" })).toBeVisible();
    await waitFor(() => expect(innerTrigger).toHaveFocus());
    focusBody();
    esc(document.body);
    expect(screen.queryByRole("dialog")).toBeNull();
    await waitFor(() => expect(outerTrigger).toHaveFocus());
  });

  // spec: UI-015
  // acceptance: independent
  it("3: with nested dialogs and the focus inside the inner one, Escape closes only the inner one", async () => {
    function Nested() {
      const [outer, setOuter] = useState(true);
      const [inner, setInner] = useState(true);
      return <>
        <Dialog title="Outer" isOpen={outer} onOpenChange={setOuter}><button type="button">Outer button</button></Dialog>
        <Dialog title="Inner" isOpen={inner} onOpenChange={setInner}><button type="button">Inner button</button></Dialog>
      </>;
    }
    render(<Nested />);
    const button = screen.getByRole("button", { name: "Inner button", hidden: true });
    act(() => button.focus());
    esc(button);
    expect(screen.queryByRole("dialog", { name: "Inner", hidden: true })).toBeNull();
    expect(screen.getByRole("dialog", { name: "Outer" })).toBeVisible();
  });

  // spec: UI-015
  // acceptance: independent
  it("3: an Escape the open combobox list handles closes only the list; the next Escape closes the dialog", () => {
    function ComboHost() {
      const [open, setOpen] = useState(true);
      const [value, setValue] = useState<string | null>("a");
      return <Dialog title="Demo" isOpen={open} onOpenChange={setOpen}>
        <ComboBox label="Filter" options={[{ id: "a", label: "Alpha" }, { id: "b", label: "Beta" }]} selected={value} onSelect={setValue} />
      </Dialog>;
    }
    render(<ComboHost />);
    const input = screen.getByRole("combobox", { name: "Filter" });
    act(() => input.focus());
    fireEvent.keyDown(input, { key: "ArrowDown" });
    expect(input).toHaveAttribute("aria-expanded", "true");
    esc(input);
    expect(input).toHaveAttribute("aria-expanded", "false");
    expect(screen.getByRole("dialog", { name: "Demo" })).toBeVisible();
    esc(input);
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  // spec: UI-015
  // spec: UI-113
  // acceptance: independent
  it("3: Escape in a search field with text clears the field and keeps the dialog; in the empty field it closes the dialog", () => {
    function SearchHost() {
      const [open, setOpen] = useState(true);
      const [query, setQuery] = useState("abc");
      return <Dialog title="Demo" isOpen={open} onOpenChange={setOpen}>
        <SearchField label="Search" value={query} onChange={setQuery} clearLabel="Clear" />
      </Dialog>;
    }
    render(<SearchHost />);
    const field = screen.getByRole("searchbox", { name: "Search" });
    act(() => field.focus());
    esc(field);
    expect(field).toHaveValue("");
    expect(screen.getByRole("dialog", { name: "Demo" })).toBeVisible();
    esc(field);
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  // spec: UI-015
  // acceptance: independent
  it("3: an Escape another element outside the dialog already handled (preventDefault) does not close the dialog", () => {
    render(<Host />);
    open();
    focusBody();
    const consume = (e: KeyboardEvent) => { if (e.key === "Escape") e.preventDefault(); };
    document.body.addEventListener("keydown", consume);
    try {
      esc(document.body);
      expect(screen.getByRole("dialog", { name: "Demo" })).toBeVisible();
    } finally { document.body.removeEventListener("keydown", consume); }
  });

  // spec: UI-015
  // acceptance: independent
  it("4: a dialog whose owner refuses to close (running operation) stays open on Escape with the focus inside or outside", () => {
    const onChange = vi.fn();
    render(<Host refuse onChange={onChange} />);
    const trigger = open();
    const inside = within(screen.getByRole("dialog")).getByRole("button", { name: "Inside" });
    act(() => inside.focus());
    esc(inside);
    expect(screen.getByRole("dialog", { name: "Demo" })).toBeVisible();
    focusBody();
    esc(document.body);
    expect(screen.getByRole("dialog", { name: "Demo" })).toBeVisible();
    expect(trigger).not.toHaveFocus();
  });

  // spec: UI-015
  // acceptance: independent
  it("4/5: isDismissable=false gives the same Escape result with the focus outside as with the focus inside", () => {
    const inside = vi.fn();
    const { unmount } = render(<Host isDismissable={false} onChange={inside} />);
    open();
    const button = within(screen.getByRole("dialog")).getByRole("button", { name: "Inside" });
    act(() => button.focus());
    esc(button);
    const closedFromInside = screen.queryByRole("dialog") === null;
    unmount();
    const outside = vi.fn();
    render(<Host isDismissable={false} onChange={outside} />);
    open();
    focusBody();
    esc(document.body);
    const closedFromOutside = screen.queryByRole("dialog") === null;
    expect(closedFromOutside).toBe(closedFromInside);
    expect(outside.mock.calls).toEqual(inside.mock.calls);
  });

  // spec: UI-015
  // acceptance: independent
  it("two Escapes in a row close the dialog once; the second does nothing else", async () => {
    const onChange = vi.fn();
    render(<Host onChange={onChange} />);
    const trigger = open();
    focusBody();
    esc(document.body);
    esc(document.activeElement ?? document.body);
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(onChange.mock.calls).toEqual([[false]]);
    await waitFor(() => expect(trigger).toHaveFocus());
  });

  // spec: UI-015
  // acceptance: independent
  it("2: when the trigger no longer exists, Escape still closes the dialog and the focus does not stay on a detached element", async () => {
    function Vanishing() {
      const [open, setOpen] = useState(false);
      const [trigger, setTrigger] = useState(true);
      return <>
        {trigger && <button type="button" onClick={() => setOpen(true)}>Open</button>}
        <button type="button" data-testid="drop" onClick={() => setTrigger(false)} hidden />
        <Dialog title="Demo" isOpen={open} onOpenChange={setOpen}><button type="button">Inside</button></Dialog>
      </>;
    }
    render(<Vanishing />);
    open();
    fireEvent.click(screen.getByTestId("drop"));
    expect(screen.queryByRole("button", { name: "Open", hidden: true })).toBeNull();
    focusBody();
    expect(() => esc(document.body)).not.toThrow();
    expect(screen.queryByRole("dialog")).toBeNull();
    await act(async () => { await Promise.resolve(); });
    const active = document.activeElement;
    expect(active === null || active.isConnected).toBe(true);
  });

  // spec: UI-015
  // spec: UI-113
  // acceptance: independent
  it("5: Enter default action, focus trap and closing by button are unchanged", async () => {
    const onOk = vi.fn();
    function Full() {
      const [open, setOpen] = useState(false);
      return <>
        <button type="button" onClick={() => setOpen(true)}>Open</button>
        <Dialog title="Demo" isOpen={open} onOpenChange={setOpen}
          footer={<DialogFooter platform="win" okLabel="OK" cancelLabel="Cancel" onOk={onOk} onCancel={() => setOpen(false)} />}>
          <label>Name<input /></label>
        </Dialog>
      </>;
    }
    render(<Full />);
    const trigger = open();
    const dialog = screen.getByRole("dialog", { name: "Demo" });
    const field = within(dialog).getByLabelText("Name");
    act(() => field.focus());
    fireEvent.keyDown(field, { key: "Enter", keyCode: 13 });
    expect(onOk).toHaveBeenCalledTimes(1);
    expect(dialog).toBeVisible();
    // Focus trap: Tab from the last control wraps to a control inside the dialog.
    const cancel = within(dialog).getByRole("button", { name: "Cancel" });
    act(() => cancel.focus());
    fireEvent.keyDown(cancel, { key: "Tab", code: "Tab", keyCode: 9 });
    expect(dialog.contains(document.activeElement)).toBe(true);
    fireEvent.click(cancel);
    expect(screen.queryByRole("dialog")).toBeNull();
    await waitFor(() => expect(trigger).toHaveFocus());
  });
});
