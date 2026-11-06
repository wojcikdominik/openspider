import type { ReactNode } from "react";
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
  isDismissable?: boolean;
}

const slot = (s: Slot | undefined, close: () => void): ReactNode => (typeof s === "function" ? s(close) : s);

/** Modaler In-App-Dialog: Fokusfalle, Esc schließt, Fokus kehrt zum Auslöser zurück (RAC). */
export function Dialog(props: DialogProps) {
  // M2-N32 (UI-145/UI-143): a closed dialog renders nothing, so owners that re-render with every live tick (upper table,
  // header, detail frame) do not run the overlay machinery of their closed dialogs. Focus still returns on close.
  return props.isOpen ? <OpenDialog {...props} /> : null;
}
function OpenDialog({ title, isOpen, onOpenChange, children, footer, description, size = "medium", isDismissable = true }: DialogProps) {
  return (
    <ModalOverlay className="os-overlay" isOpen={isOpen} onOpenChange={onOpenChange} isDismissable={isDismissable}>
      <Modal className={cx("os-modal", `os-modal-${size}`)}>
        <RacDialog className="os-dialog">
          {({ close }) => (
            <>
              <Heading slot="title" className="os-dialog-title">
                {title}
              </Heading>
              {description && <p className="os-dialog-description">{description}</p>}
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

/**
 * Plattformgerechte Fußleiste (5.6): macOS „Abbrechen“ links von „OK“ (OK rechts außen); Windows „OK“, dann „Abbrechen“,
 * beide rechtsbündig.
 */
export function DialogFooter({ okLabel, cancelLabel, onOk, onCancel, okDisabled, extra, platform = currentPlatform(), okVariant = "primary" }: DialogFooterProps) {
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
    <div className="os-dialog-footer" data-platform={platform}>
      {extra && <div className="os-dialog-footer-extra">{extra}</div>}
      <div className="os-dialog-footer-actions">{platform === "mac" ? [cancel, ok] : [ok, cancel]}</div>
    </div>
  );
}
