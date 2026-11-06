import { useEffect, useRef, useState, type KeyboardEvent, type MouseEvent, type ReactNode } from "react";
import { Popover } from "react-aria-components";
import { useT } from "../../i18n";
import { Button } from "../../ui/primitives/Button";
import { Dialog, DialogFooter } from "../../ui/primitives/Dialog";
import { MenuBody, type MenuEntry } from "../../ui/primitives/Menu";
import "./table-advanced.css";

type Translate = ReturnType<typeof useT>;
export interface TabLayout { order: string[]; hidden: string[]; mandatory: string }
/**
 * Entries of a tab's context menu (UI-112). Reusable for every tab bar: the mandatory tab (top: Internal, detail: URL Details,
 * right: Overview) is never closed; closed tabs come back through the bar's dropdown (UI-004).
 */
export function tabContextEntries(t: Translate, tab: string, layout: TabLayout, actions: {
  hide: (ids: string[]) => void; configure: () => void; reset: () => void;
}): MenuEntry[] {
  const visible = layout.order.filter(id => !layout.hidden.includes(id));
  const others = visible.filter(id => id !== tab && id !== layout.mandatory);
  const all = visible.filter(id => id !== layout.mandatory);
  return [
    { id: "tab-close", label: t("u20-tab-close"), isDisabled: tab === layout.mandatory,
      disabledReason: tab === layout.mandatory ? t("u20-tab-mandatory") : undefined, onAction: () => actions.hide([tab]) },
    { id: "tab-close-others", label: t("u20-tab-close-others"), isDisabled: !others.length, onAction: () => actions.hide(others) },
    { id: "tab-close-all", label: t("u20-tab-close-all"), isDisabled: !all.length, onAction: () => actions.hide(all) },
    { id: "tab-configure", label: t("u20-tabs-configure"), separatorBefore: true, onAction: actions.configure },
    { id: "tab-reset", label: t("u20-tabs-reset"), onAction: actions.reset },
  ];
}

/** Opens a context menu for the tab under the pointer, or the focused tab with Shift+F10 / the context menu key. */
export function TabContextMenuHost({ entriesFor, label, children, className }: {
  entriesFor: (tab: string) => MenuEntry[]; label: string; children: ReactNode; className?: string;
}) {
  const [menu, setMenu] = useState<{ tab: string; x: number; y: number }>();
  const anchor = useRef<HTMLSpanElement>(null);
  const tabOf = (target: EventTarget) => (target as HTMLElement).closest?.<HTMLElement>("[data-tab-id]")?.dataset.tabId;
  const onContextMenu = (event: MouseEvent) => {
    const tab = tabOf(event.target); if (!tab) return;
    event.preventDefault(); setMenu({ tab, x: event.clientX, y: event.clientY });
  };
  useEffect(() => {
    if (!menu) return;
    // Keyboard users land in the menu (UI-112 via Shift+F10); the first enabled entry gets the focus.
    const timer = setTimeout(() => document.querySelector<HTMLElement>(".u20-tab-popover [role=menuitem]:not([aria-disabled=true])")?.focus(), 0);
    // The global Esc shortcut (search.clear) stops the event in the capture phase; close the menu from there as well.
    const escape = (event: globalThis.KeyboardEvent) => { if (event.key === "Escape") setMenu(undefined); };
    document.addEventListener("keydown", escape, true);
    return () => { clearTimeout(timer); document.removeEventListener("keydown", escape, true); };
  }, [menu]);
  const onKeyDown = (event: KeyboardEvent) => {
    if (event.key === "Escape" && menu) { event.preventDefault(); setMenu(undefined); return; }
    if (event.key !== "ContextMenu" && !(event.key === "F10" && event.shiftKey)) return;
    const tab = tabOf(event.target); if (!tab) return;
    event.preventDefault(); event.stopPropagation();
    const rect = (event.target as HTMLElement).getBoundingClientRect();
    setMenu({ tab, x: rect.left + 8, y: rect.bottom });
  };
  const entries = menu ? entriesFor(menu.tab).map(e => ({ ...e, onAction: () => { setMenu(undefined); e.onAction?.(); } })) : [];
  return <div className={className} onContextMenu={onContextMenu} onKeyDown={onKeyDown}>
    {children}
    <span ref={anchor} className="u20-menu-anchor" style={{ left: menu?.x ?? 0, top: menu?.y ?? 0 }} />
    <Popover triggerRef={anchor} isOpen={menu !== undefined} onOpenChange={open => { if (!open) setMenu(undefined); }} placement="bottom start" className="os-popover u20-tab-popover">
      <MenuBody entries={entries} label={label} />
    </Popover>
  </div>;
}

type TabsConfigDialogProps = {
  isOpen: boolean; onOpenChange: (open: boolean) => void; layout: TabLayout; label: (id: string) => string;
  onApply: (order: string[], hidden: string[]) => void;
};
/** „Tabs konfigurieren“ (UI-004 [P1]): visibility checkbox and up/down per tab; OK applies, Cancel discards.
 * A closed dialog is not rendered (M2-N32): tab clicks and live ticks of the owning bar do not render it. */
export function TabsConfigDialog(props: TabsConfigDialogProps) {
  return props.isOpen ? <OpenTabsConfigDialog {...props} /> : null;
}
function OpenTabsConfigDialog({ isOpen, onOpenChange, layout, label, onApply }: TabsConfigDialogProps) {
  const t = useT();
  const [order, setOrder] = useState(layout.order); const [hidden, setHidden] = useState(layout.hidden);
  useEffect(() => { if (isOpen) { setOrder(layout.order); setHidden(layout.hidden); } }, [isOpen]);
  const move = (index: number, direction: number) => {
    const next = [...order]; [next[index], next[index + direction]] = [next[index + direction], next[index]]; setOrder(next);
  };
  return <Dialog title={t("u20-tabs-configure-title")} isOpen={isOpen} onOpenChange={onOpenChange}
    footer={<DialogFooter okLabel={t("u20-ok")} cancelLabel={t("u20-cancel")} onOk={() => { onApply(order, hidden); onOpenChange(false); }} onCancel={() => onOpenChange(false)} />}>
    <ul className="u20-tab-list" aria-label={t("u20-tabs-configure-title")}>
      {order.map((id, index) => <li key={id} className="u20-row">
        <label><input type="checkbox" checked={!hidden.includes(id)} disabled={id === layout.mandatory}
          onChange={e => setHidden(e.target.checked ? hidden.filter(h => h !== id) : [...hidden, id])} />{label(id)}</label>
        <Button aria-label={t("u20-tab-up", { tab: label(id) })} isDisabled={index === 0} onPress={() => move(index, -1)}>↑</Button>
        <Button aria-label={t("u20-tab-down", { tab: label(id) })} isDisabled={index === order.length - 1} onPress={() => move(index, 1)}>↓</Button>
      </li>)}
    </ul>
  </Dialog>;
}
