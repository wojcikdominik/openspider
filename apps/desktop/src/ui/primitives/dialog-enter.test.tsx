import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import type { FormEvent, ReactNode } from "react";
import { Dialog, DialogFooter } from "./Dialog";

afterEach(cleanup);

function renderDialog(footer: Partial<Parameters<typeof DialogFooter>[0]> = {}, body: ReactNode = null) {
  const onOk = vi.fn();
  const onCancel = vi.fn();
  render(<>
    <input aria-label="Outside" />
    <Dialog title="Demo" isOpen onOpenChange={() => undefined}
      footer={<DialogFooter platform="win" okLabel="OK" cancelLabel="Cancel" onOk={onOk} onCancel={onCancel} {...footer} />}>
      <p>Body</p>{body}
    </Dialog>
  </>);
  const dialog = screen.getByRole("dialog");
  return { onOk, onCancel, dialog };
}

describe("Dialog: Enter als Standardaktion (UI-113, OS-Festlegung 09.11.2026)", () => {
  // spec: UI-113
  it("Enter on the dialog itself and in a single-line text field triggers OK once", () => {
    const { onOk, dialog } = renderDialog({}, <label>Name<input /></label>);
    expect(fireEvent.keyDown(dialog, { key: "Enter", keyCode: 13 })).toBe(false);
    expect(onOk).toHaveBeenCalledTimes(1);
    fireEvent.keyDown(within(dialog).getByLabelText("Name"), { key: "Enter", keyCode: 13 });
    expect(onOk).toHaveBeenCalledTimes(2);
    // Other keys do nothing.
    fireEvent.keyDown(dialog, { key: " ", keyCode: 32 });
    expect(onOk).toHaveBeenCalledTimes(2);
  });

  // spec: UI-113
  it("Enter leaves buttons, links, multi-line text areas and selection controls alone", () => {
    const { onOk, onCancel, dialog } = renderDialog({}, <>
      <a href="#x">Link</a>
      <textarea aria-label="Notes" />
      <select aria-label="Choice"><option>a</option></select>
      <div role="listbox" aria-label="List"><div role="option" aria-selected="false" tabIndex={0}>Item</div></div>
      <input role="combobox" aria-label="Combo" aria-expanded="false" />
    </>);
    const targets = [
      within(dialog).getByRole("button", { name: "Cancel" }),
      within(dialog).getByRole("button", { name: "OK" }),
      within(dialog).getByRole("link", { name: "Link" }),
      within(dialog).getByLabelText("Notes"),
      within(dialog).getByLabelText("Choice"),
      within(dialog).getByRole("option", { name: "Item" }),
      within(dialog).getByLabelText("Combo"),
    ];
    for (const target of targets) fireEvent.keyDown(target, { key: "Enter", keyCode: 13 });
    expect(onOk).not.toHaveBeenCalled();
    expect(onCancel).not.toHaveBeenCalled();
  });

  // spec: UI-113
  it("no OK while it is disabled, during an IME composition, with a modifier, on key repeat or outside the dialog", () => {
    const disabled = renderDialog({ okDisabled: true });
    fireEvent.keyDown(disabled.dialog, { key: "Enter", keyCode: 13 });
    expect(disabled.onOk).not.toHaveBeenCalled();
    cleanup();
    const { onOk, dialog } = renderDialog();
    fireEvent.keyDown(dialog, { key: "Enter", keyCode: 13, isComposing: true });
    fireEvent.keyDown(dialog, { key: "Enter", keyCode: 229 });
    for (const mod of ["shiftKey", "ctrlKey", "metaKey", "altKey"]) fireEvent.keyDown(dialog, { key: "Enter", keyCode: 13, [mod]: true });
    fireEvent.keyDown(dialog, { key: "Enter", keyCode: 13, repeat: true });
    fireEvent.keyDown(screen.getByLabelText("Outside"), { key: "Enter", keyCode: 13 });
    expect(onOk).not.toHaveBeenCalled();
  });

  // spec: UI-113
  it("a field that handles Enter itself, or a text field of a form, keeps it", () => {
    const own = vi.fn();
    const submit = vi.fn((e: FormEvent) => e.preventDefault());
    const { onOk, dialog } = renderDialog({}, <>
      <input aria-label="Own" onKeyDown={(e) => { if (e.key === "Enter") { e.preventDefault(); own(); } }} />
      <input aria-label="Stopped" onKeyDown={(e) => { if (e.key === "Enter") e.stopPropagation(); }} />
      <form onSubmit={submit}><input aria-label="In form" /></form>
    </>);
    fireEvent.keyDown(within(dialog).getByLabelText("Own"), { key: "Enter", keyCode: 13 });
    fireEvent.keyDown(within(dialog).getByLabelText("Stopped"), { key: "Enter", keyCode: 13 });
    fireEvent.keyDown(within(dialog).getByLabelText("In form"), { key: "Enter", keyCode: 13 });
    expect(own).toHaveBeenCalledTimes(1);
    expect(onOk).not.toHaveBeenCalled();
  });

  // spec: UI-113
  it("a destructive OK (danger) reacts only to its own focused button, never to Enter elsewhere", () => {
    const { onOk, dialog } = renderDialog({ okVariant: "danger" }, <label>Name<input /></label>);
    fireEvent.keyDown(dialog, { key: "Enter", keyCode: 13 });
    fireEvent.keyDown(within(dialog).getByLabelText("Name"), { key: "Enter", keyCode: 13 });
    expect(onOk).not.toHaveBeenCalled();
    // The focused OK button itself is activated natively (here: click as the browser's result of Enter on a button).
    fireEvent.click(within(dialog).getByRole("button", { name: "OK" }));
    expect(onOk).toHaveBeenCalledTimes(1);
  });

  // spec: UI-113
  it("uses the current OK action and state after a re-render", () => {
    const first = vi.fn();
    const second = vi.fn();
    const footer = (onOk: () => void, okDisabled: boolean) => <DialogFooter platform="win" okLabel="OK" cancelLabel="Cancel" onOk={onOk} okDisabled={okDisabled} />;
    const { rerender } = render(<Dialog title="Demo" isOpen onOpenChange={() => undefined} footer={footer(first, true)}><p>Body</p></Dialog>);
    const dialog = screen.getByRole("dialog");
    fireEvent.keyDown(dialog, { key: "Enter", keyCode: 13 });
    rerender(<Dialog title="Demo" isOpen onOpenChange={() => undefined} footer={footer(second, false)}><p>Body</p></Dialog>);
    fireEvent.keyDown(screen.getByRole("dialog"), { key: "Enter", keyCode: 13 });
    expect(first).not.toHaveBeenCalled();
    expect(second).toHaveBeenCalledTimes(1);
  });
});
