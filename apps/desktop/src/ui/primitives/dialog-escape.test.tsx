import { afterEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { useRef, useState, type ReactNode } from "react";
import { Dialog as RacDialog, Modal, ModalOverlay } from "react-aria-components";
import { Dialog, dialogEscapeCloses, escapeTargetBelow, useTopmostEscape } from "./Dialog";

/** An overlay built on ModalOverlay directly (like the settings dialog), wired to the shared Escape logic. */
function PlainOverlay({ onClose, children }: { onClose: () => void; children: ReactNode }) {
  const ref = useRef<HTMLElement>(null);
  useTopmostEscape(ref, onClose);
  return <ModalOverlay className="os-overlay" isOpen onOpenChange={open => { if (!open) onClose(); }} isKeyboardDismissDisabled>
    <Modal className="os-modal"><RacDialog ref={ref} aria-label="Settings">{children}</RacDialog></Modal>
  </ModalOverlay>;
}

afterEach(cleanup);

const flush = () => act(async () => { await Promise.resolve(); await Promise.resolve(); });

describe("Dialog: Escape schließt immer den obersten Dialog (UI-015)", () => {
  // spec: UI-015
  it("Escape on the trigger outside the freshly opened dialog closes it once", () => {
    const onOpenChange = vi.fn();
    render(<>
      <button>Trigger</button>
      <Dialog title="Demo" isOpen onOpenChange={onOpenChange}><p>Body</p></Dialog>
    </>);
    const trigger = screen.getByRole("button", { name: "Trigger", hidden: true });
    expect(fireEvent.keyDown(trigger, { key: "Escape" })).toBe(false);
    expect(onOpenChange).toHaveBeenCalledTimes(1);
    expect(onOpenChange).toHaveBeenCalledWith(false);
  });

  // spec: UI-015
  it("Escape with the focus inside the dialog closes it once", () => {
    const onOpenChange = vi.fn();
    render(<Dialog title="Demo" isOpen onOpenChange={onOpenChange}><p>Body</p></Dialog>);
    fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });
    expect(onOpenChange).toHaveBeenCalledTimes(1);
    expect(onOpenChange).toHaveBeenCalledWith(false);
  });

  // spec: UI-015
  it("leaves other keys, modified, repeated and composing Escapes and Escapes a handler took alone", () => {
    render(<Dialog title="Demo" isOpen onOpenChange={() => undefined}><p>Body</p></Dialog>);
    const dialog = screen.getByRole("dialog");
    const base = { key: "Escape", repeat: false, isComposing: false, defaultPrevented: false, altKey: false, ctrlKey: false, metaKey: false, shiftKey: false };
    expect(dialogEscapeCloses(base, dialog)).toBe(true);
    expect(dialogEscapeCloses({ ...base, key: "Enter" }, dialog)).toBe(false);
    expect(dialogEscapeCloses({ ...base, shiftKey: true }, dialog)).toBe(false);
    expect(dialogEscapeCloses({ ...base, repeat: true }, dialog)).toBe(false);
    expect(dialogEscapeCloses({ ...base, isComposing: true }, dialog)).toBe(false);
    expect(dialogEscapeCloses({ ...base, defaultPrevented: true }, dialog)).toBe(false);
  });

  // spec: UI-015
  it("only the topmost of two open dialogs reacts, also when the focus is still in the outer one (B1-1)", () => {
    const outer = vi.fn();
    const inner = vi.fn();
    render(<>
      <button>Trigger</button>
      <Dialog title="Outer" isOpen onOpenChange={outer}><button>Delete</button></Dialog>
      <Dialog title="Inner" isOpen onOpenChange={inner}><p>Inner body</p></Dialog>
    </>);
    fireEvent.keyDown(screen.getByRole("button", { name: "Trigger", hidden: true }), { key: "Escape" });
    expect(inner).toHaveBeenCalledTimes(1);
    // The outer dialog's own button is the target: its overlay must not swallow the key.
    fireEvent.keyDown(screen.getByRole("button", { name: "Delete", hidden: true }), { key: "Escape" });
    expect(inner).toHaveBeenCalledTimes(2);
    expect(outer).not.toHaveBeenCalled();
  });

  // spec: UI-015
  it("isKeyboardDismissDisabled keeps the dialog open on Escape inside and outside; isDismissable=false does not", () => {
    const mandatory = vi.fn();
    const { unmount } = render(<>
      <button>Trigger</button>
      <Dialog title="Mandatory" isOpen isKeyboardDismissDisabled onOpenChange={mandatory}><p>Body</p></Dialog>
    </>);
    fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });
    fireEvent.keyDown(screen.getByRole("button", { name: "Trigger", hidden: true }), { key: "Escape" });
    expect(mandatory).not.toHaveBeenCalled();
    unmount();
    const pointerOnly = vi.fn();
    render(<Dialog title="Pointer" isOpen isDismissable={false} onOpenChange={pointerOnly}><p>Body</p></Dialog>);
    fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });
    expect(pointerOnly).toHaveBeenCalledWith(false);
  });

  // spec: UI-015
  it("returns the focus to the trigger after Escape; without the trigger it closes without error and the focus is on body (B2-1)", async () => {
    function Host({ keepTrigger }: { keepTrigger: boolean }) {
      const [open, setOpen] = useState(false);
      const [shown, setShown] = useState(true);
      return <>
        {shown && <button onClick={() => { setOpen(true); if (!keepTrigger) setShown(false); }}>Open</button>}
        <Dialog title="Demo" isOpen={open} onOpenChange={setOpen}><p>Body</p></Dialog>
      </>;
    }
    const kept = render(<Host keepTrigger />);
    const trigger = screen.getByRole("button", { name: "Open" });
    trigger.focus();
    fireEvent.click(trigger);
    fireEvent.keyDown(trigger, { key: "Escape" });
    await flush();
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(document.activeElement).toBe(trigger);
    kept.unmount();

    render(<Host keepTrigger={false} />);
    const gone = screen.getByRole("button", { name: "Open" });
    gone.focus();
    fireEvent.click(gone);
    expect(gone.isConnected).toBe(false);
    fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });
    await flush();
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(document.activeElement).toBe(document.body);
  });

  // spec: UI-015
  it("a primitive dialog above an overlay without the primitive closes on Escape whose target is still in that overlay", () => {
    const settings = vi.fn();
    const email = vi.fn();
    const own = vi.fn();
    const { rerender } = render(<>
      <button>Page</button>
      <PlainOverlay onClose={settings}><button onKeyDown={e => { if (e.key === "Escape") { e.preventDefault(); own(); } }}>Email</button></PlainOverlay>
      <Dialog title="Add SMTP account" isOpen onOpenChange={email}><p>Body</p></Dialog>
    </>);
    const lower = screen.getByRole("button", { name: "Email", hidden: true });
    const upper = screen.getByRole("dialog", { name: "Add SMTP account", hidden: true });
    const page = screen.getByText("Page");
    expect(escapeTargetBelow(lower, upper)).toBe(true);
    expect(escapeTargetBelow(upper, upper)).toBe(false);
    // The page is not a lower dialog: its handlers keep their turn (bubble phase).
    expect(escapeTargetBelow(page, upper)).toBe(false);
    expect(escapeTargetBelow(document.body, upper)).toBe(false);
    // The lower overlay's own handlers (here one that would consume Escape) do not get the key.
    fireEvent.keyDown(lower, { key: "Escape" });
    expect(email).toHaveBeenCalledTimes(1);
    expect(email).toHaveBeenCalledWith(false);
    expect(settings).not.toHaveBeenCalled();
    expect(own).not.toHaveBeenCalled();
    // Alone, the overlay without the primitive is the topmost and closes on Escape as before.
    rerender(<PlainOverlay onClose={settings}><button>Email</button></PlainOverlay>);
    fireEvent.keyDown(screen.getByRole("button", { name: "Email" }), { key: "Escape" });
    expect(settings).toHaveBeenCalledTimes(1);
  });
});
