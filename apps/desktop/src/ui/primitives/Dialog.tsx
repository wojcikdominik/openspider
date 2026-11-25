import { useEffect, useRef, type ReactNode } from "react";
import { Dialog as RacDialog, Heading, Modal, ModalOverlay } from "react-aria-components";
import { currentPlatform, type Platform } from "../platform";
import { Button } from "./Button";
import { cx } from "./cx";

type Slot = ReactNode | ((close: () => void) => ReactNode);

export interface DialogProps {
  title: string;
  isOpen: boolean;
  onOpenChange: (open: boolean) => void;
  children: Slot;
  footer?: Slot;
  /** Beschreibender Text unter dem Titel. */
  description?: string;
  size?: "small" | "medium" | "large";
  /** Klick neben den Dialog schließt ihn (nur Zeiger; Escape steuert `isKeyboardDismissDisabled`). */
  isDismissable?: boolean;
  /** Escape schließt den Dialog nicht (Pflichtdialog). Standard: Escape schließt den obersten offenen Dialog. */
  isKeyboardDismissDisabled?: boolean;
  /** Optional symbol left of title and description (e.g. the app mark in the update dialog, OUT-251). */
  icon?: ReactNode;
}

const slot = (s: Slot | undefined, close: () => void): ReactNode => (typeof s === "function" ? s(close) : s);

/** Modaler In-App-Dialog: Fokusfalle, Esc schließt, Fokus kehrt zum Auslöser zurück (RAC). */
export function Dialog(props: DialogProps) {
  // M2-N32 (UI-145/UI-143): a closed dialog renders nothing, so owners that re-render with every live tick (upper table,
  // header, detail frame) do not run the overlay machinery of their closed dialogs. Focus still returns on close.
  return props.isOpen ? <OpenDialog {...props} /> : null;
}
/** Offene modale Dialoge in Stapelreihenfolge: React Aria hängt jedes Modal als eigenes Portal an das Dokumentende. */
const OPEN_DIALOGS = ".os-overlay [role='dialog'], .os-overlay [role='alertdialog']";

/**
 * UI-015/UI-113: does this Escape `keydown` close `dialog`?
 *
 * Escape closes the topmost open dialog, wherever the focus is. React Aria handles Escape only from inside a modal and
 * consumes it there even when that modal is not the topmost (a nested confirmation was left open). When the focus was last
 * moved without a key or pointer event (screen reader, programmatic focus), React Aria moves the focus into a freshly
 * opened dialog only after an animation frame; an Escape in that window landed outside the dialog and was lost. The
 * dialog therefore handles Escape itself on the document (bubble phase): yes for a plain Escape that no handler took
 * (`preventDefault`, e.g. an open list, popover or a search field clearing its text), not repeated, without IME
 * composition, when `dialog` is the topmost open dialog.
 */
export function dialogEscapeCloses(
  e: Pick<KeyboardEvent, "key" | "repeat" | "isComposing" | "defaultPrevented" | "altKey" | "ctrlKey" | "metaKey" | "shiftKey">,
  dialog: Element,
): boolean {
  if (e.key !== "Escape" || e.defaultPrevented || e.repeat || e.isComposing) return false;
  if (e.altKey || e.ctrlKey || e.metaKey || e.shiftKey) return false;
  const open = dialog.ownerDocument.querySelectorAll(OPEN_DIALOGS);
  return open[open.length - 1] === dialog;
}

function OpenDialog({ title, isOpen, onOpenChange, children, footer, description, size = "medium", isDismissable = true, isKeyboardDismissDisabled = false, icon }: DialogProps) {
  const ref = useRef<HTMLElement>(null);
  const latest = useRef({ onOpenChange, isKeyboardDismissDisabled });
  latest.current = { onOpenChange, isKeyboardDismissDisabled };
  // The trigger, read while rendering (before the modal makes the rest of the page inert and the trigger loses focus).
  const trigger = useRef<Element | null>(typeof document !== "undefined" ? document.activeElement : null);
  useEffect(() => {
    const dialog = ref.current;
    if (!dialog) return;
    let escaped = false;
    const onKey = (e: KeyboardEvent) => {
      if (!dialogEscapeCloses(e, dialog)) return;
      // The topmost dialog owns Escape; a mandatory dialog keeps it without closing.
      e.preventDefault();
      e.stopPropagation();
      if (latest.current.isKeyboardDismissDisabled) return;
      escaped = true;
      latest.current.onOpenChange(false);
    };
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("keydown", onKey);
      // Return the focus to the trigger once the page is no longer inert (after the unmount cleanups). React Aria does so
      // only from a scope the focus has entered, and only a frame later. A trigger that no longer exists is skipped (the
      // focus stays on body); focus that something else took meanwhile stays where it is.
      const el = trigger.current;
      if (!escaped || !(el instanceof HTMLElement) || el === el.ownerDocument.body) return;
      queueMicrotask(() => {
        const active = el.ownerDocument.activeElement;
        if (el.isConnected && (!active || active === el.ownerDocument.body)) el.focus({ preventScroll: true });
      });
    };
  }, []);
  return (
    <ModalOverlay className="os-overlay" isOpen={isOpen} onOpenChange={onOpenChange} isDismissable={isDismissable} isKeyboardDismissDisabled>
      <Modal className={cx("os-modal", `os-modal-${size}`)}>
        <RacDialog ref={ref} className="os-dialog">
          {({ close }) => (
            <>
              {icon ? (
                <div className="os-dialog-head">
                  <span className="os-dialog-icon" aria-hidden>{icon}</span>
                  <div className="os-dialog-head-text">
                    <Heading slot="title" className="os-dialog-title">
                      {title}
                    </Heading>
                    {description && <p className="os-dialog-description">{description}</p>}
                  </div>
                </div>
              ) : (
                <>
                  <Heading slot="title" className="os-dialog-title">
                    {title}
                  </Heading>
                  {description && <p className="os-dialog-description">{description}</p>}
                </>
              )}
              <div className="os-dialog-body">{slot(children, close)}</div>
              {slot(footer, close)}
            </>
          )}
        </RacDialog>
      </Modal>
    </ModalOverlay>
  );
}

export interface DialogFooterProps {
  okLabel: string;
  cancelLabel: string;
  onOk?: () => void;
  onCancel?: () => void;
  okDisabled?: boolean;
  /** Zusätzliche Aktion links (z. B. „Zurücksetzen“). */
  extra?: ReactNode;
  /** Zielplattform; Standard: erkannte Plattform. */
  platform?: Platform;
  okVariant?: "primary" | "danger";
}

/** Elements with their own meaning for Enter: Enter there never triggers the dialog's OK action. */
const ENTER_OWNERS = [
  "button", "a[href]", "textarea", "select", "[contenteditable]:not([contenteditable='false'])",
  "[role='button']", "[role='link']", "[role='combobox']", "[role='listbox']", "[role='option']", "[role='menu']",
  "[role='menuitem']", "[role='tab']", "[role='tree']", "[role='treeitem']", "[role='grid']", "[role='gridcell']",
].join(", ");

/**
 * UI-113 (OS-Festlegung, Orchestrator 09.11.2026): does this Enter `keydown` trigger the OK action of `dialog`?
 *
 * Yes, unless the focus is on another button, a link, a multi-line text area or a selection control (or another widget
 * with its own Enter handling), OK is disabled, an IME composition is open, the key carries a modifier or repeats, or an
 * element inside the dialog already handled Enter (`preventDefault`). A text field inside a `<form>` keeps the form's own
 * submission. For a destructive OK (`danger`) only the focused OK button itself reacts (native button activation), so
 * this returns `false`.
 */
export function dialogEnterConfirms(
  e: Pick<KeyboardEvent, "key" | "keyCode" | "isComposing" | "repeat" | "defaultPrevented" | "altKey" | "ctrlKey" | "metaKey" | "shiftKey" | "target">,
  dialog: Element,
  ok: { disabled: boolean; danger: boolean },
): boolean {
  if (e.key !== "Enter" || e.defaultPrevented || e.repeat || e.isComposing || e.keyCode === 229) return false;
  if (e.altKey || e.ctrlKey || e.metaKey || e.shiftKey) return false;
  if (ok.disabled || ok.danger) return false;
  const target = e.target;
  if (!(target instanceof Element) || !dialog.contains(target)) return false;
  const owner = target.closest(ENTER_OWNERS);
  if (owner && dialog.contains(owner)) return false;
  if (target instanceof HTMLInputElement && target.form) return false;
  return true;
}

/**
 * Plattformgerechte Fußleiste (5.6): macOS „Abbrechen“ links von „OK“ (OK rechts außen); Windows „OK“, dann „Abbrechen“,
 * beide rechtsbündig.
 */
export function DialogFooter({ okLabel, cancelLabel, onOk, onCancel, okDisabled, extra, platform = currentPlatform(), okVariant = "primary" }: DialogFooterProps) {
  const root = useRef<HTMLDivElement>(null);
  const latest = useRef({ onOk, okDisabled, okVariant });
  latest.current = { onOk, okDisabled, okVariant };
  // UI-113: Enter is the dialog's default action. Listening on the document (bubble phase) runs after every handler
  // inside the dialog, so a field that handles Enter itself (preventDefault/stopPropagation) keeps it.
  useEffect(() => {
    const dialog = root.current?.closest("[role='dialog'], [role='alertdialog']");
    if (!dialog) return;
    const onKey = (e: KeyboardEvent) => {
      const { onOk: action, okDisabled: disabled, okVariant: variant } = latest.current;
      if (!action || !dialogEnterConfirms(e, dialog, { disabled: !!disabled, danger: variant === "danger" })) return;
      e.preventDefault();
      action();
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, []);
  const ok = (
    <Button key="ok" variant={okVariant} onPress={onOk} isDisabled={okDisabled} data-role="ok">
      {okLabel}
    </Button>
  );
  const cancel = (
    <Button key="cancel" variant="secondary" onPress={onCancel} data-role="cancel">
      {cancelLabel}
    </Button>
  );
  return (
    <div ref={root} className="os-dialog-footer" data-platform={platform}>
      {extra && <div className="os-dialog-footer-extra">{extra}</div>}
      <div className="os-dialog-footer-actions">{platform === "mac" ? [cancel, ok] : [ok, cancel]}</div>
    </div>
  );
}
