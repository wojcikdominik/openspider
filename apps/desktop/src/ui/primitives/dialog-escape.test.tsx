import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { Dialog, dialogEscapeCloses } from "./Dialog";

afterEach(cleanup);

describe("Dialog: Escape schließt auch vor dem Fokuswechsel in den Dialog (UI-015)", () => {
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
  it("leaves Escape alone inside the dialog, with a modifier, during IME composition and when a handler took it", () => {
    const onOpenChange = vi.fn();
    render(<>
      <button>Trigger</button>
      <Dialog title="Demo" isOpen onOpenChange={onOpenChange}><p>Body</p></Dialog>
    </>);
    const dialog = screen.getByRole("dialog");
    const trigger = screen.getByRole("button", { name: "Trigger", hidden: true });
    const base = { key: "Escape", isComposing: false, defaultPrevented: false, altKey: false, ctrlKey: false, metaKey: false, shiftKey: false };
    expect(dialogEscapeCloses({ ...base, target: trigger }, dialog)).toBe(true);
    expect(dialogEscapeCloses({ ...base, target: dialog }, dialog)).toBe(false);
    expect(dialogEscapeCloses({ ...base, target: trigger, key: "Enter" }, dialog)).toBe(false);
    expect(dialogEscapeCloses({ ...base, target: trigger, shiftKey: true }, dialog)).toBe(false);
    expect(dialogEscapeCloses({ ...base, target: trigger, isComposing: true }, dialog)).toBe(false);
    expect(dialogEscapeCloses({ ...base, target: trigger, defaultPrevented: true }, dialog)).toBe(false);
  });

  // spec: UI-015
  it("only the topmost of two open dialogs reacts", () => {
    const outer = vi.fn();
    const inner = vi.fn();
    render(<>
      <button>Trigger</button>
      <Dialog title="Outer" isOpen onOpenChange={outer}><p>Outer body</p></Dialog>
      <Dialog title="Inner" isOpen onOpenChange={inner}><p>Inner body</p></Dialog>
    </>);
    fireEvent.keyDown(screen.getByRole("button", { name: "Trigger", hidden: true }), { key: "Escape" });
    expect(inner).toHaveBeenCalledTimes(1);
    expect(outer).not.toHaveBeenCalled();
  });
});
