import { useEffect, useState, type MouseEvent } from "react";
import { useT } from "../../i18n";
import { columnLabel } from "../../i18n/canon";
import type { ColumnState } from "../../grid/DataGrid";
import type { ColumnMeta } from "../../ipc/gen/ColumnMeta";
import { applyTemplateColumns, exportColumns, normalizeColumns, useColumns } from "../../state/columns";
import { Button } from "../../ui/primitives/Button";
import { Dialog, DialogFooter } from "../../ui/primitives/Dialog";
import "./table-advanced.css";

const fixed = (key: string) => key === "Row" || key === "Address";
export type MoveKind = "up" | "down" | "top" | "bottom";
/** Moves all selected keys in one step; `Address` stays first and is never passed. */
export function moveKeys(keys: string[], selected: ReadonlySet<string>, kind: MoveKind): string[] {
  const head = keys.filter(fixed); const body = keys.filter(k => !fixed(k));
  const chosen = body.filter(k => selected.has(k)); const rest = body.filter(k => !selected.has(k));
  if (kind === "top") return [...head, ...chosen, ...rest];
  if (kind === "bottom") return [...head, ...rest, ...chosen];
  const out = [...body];
  const indices = out.map((_, i) => i); if (kind === "down") indices.reverse();
  for (const i of indices) {
    const j = kind === "up" ? i - 1 : i + 1;
    if (j < 0 || j >= out.length || !selected.has(out[i]) || selected.has(out[j])) continue;
    [out[i], out[j]] = [out[j], out[i]];
  }
  return [...head, ...out];
}
/** Puts the visible columns into the new order, keeping hidden columns on their slots (re-adding restores their place). */
export function reorderVisible(columns: ColumnState[], visibleOrder: string[]): ColumnState[] {
  const queue = visibleOrder.map(key => columns.find(c => c.key === key)!);
  return columns.map(c => c.visible && c.key !== "Row" ? queue.shift()! : c);
}
/** Click, Shift+click (range from the anchor) and Ctrl/Cmd+click (toggle) on a list. */
export function nextSelection(keys: string[], selected: ReadonlySet<string>, anchor: string | undefined, key: string, mode: { shift: boolean; toggle: boolean }): Set<string> {
  if (mode.shift && anchor && keys.includes(anchor)) {
    const [a, b] = [keys.indexOf(anchor), keys.indexOf(key)].sort((x, y) => x - y);
    return new Set([...(mode.toggle ? selected : []), ...keys.slice(a, b + 1)]);
  }
  if (mode.toggle) { const next = new Set(selected); if (next.has(key)) next.delete(key); else next.add(key); return next; }
  return new Set([key]);
}

function ColumnList({ label, keys, selected, onSelect, hint }: {
  label: string; keys: string[]; selected: ReadonlySet<string>; onSelect: (key: string, event: MouseEvent | { shiftKey: boolean; ctrlKey: boolean; metaKey: boolean }) => void;
  hint: (key: string) => string | undefined;
}) {
  const t = useT();
  return <div className="u20-list-box">
    <span className="u20-list-label">{label}</span>
    <div role="listbox" aria-label={label} aria-multiselectable className="u20-list">
      {keys.map(key => <div key={key} role="option" tabIndex={0} aria-selected={selected.has(key)} data-selected={selected.has(key) || undefined}
        className="u20-option" onClick={event => onSelect(key, event)}
        onKeyDown={event => { if (event.key === " " || event.key === "Enter") { event.preventDefault(); onSelect(key, { shiftKey: event.shiftKey, ctrlKey: true, metaKey: false }); } }}>
        {columnLabel(t, key)}{hint(key) && <span className="u20-hint"> – {hint(key)}</span>}
      </div>)}
    </div>
  </div>;
}

type ColumnConfiguratorProps = {
  tab: string; tabs: { id: string; label: string }[]; isOpen: boolean; onOpenChange: (open: boolean) => void; label: (tab: string) => string;
};
/** „Spalten konfigurieren“ (UI-009): available/displayed lists, search, multi-select, moves, defaults and named templates.
 * A closed dialog is not rendered (M2-N32): it neither re-renders with the upper table nor subscribes to the column store. */
export function ColumnConfigurator(props: ColumnConfiguratorProps) {
  return props.isOpen ? <ColumnConfiguratorDialog {...props} /> : null;
}
function ColumnConfiguratorDialog({ tab, tabs, isOpen, onOpenChange, label }: ColumnConfiguratorProps) {
  const t = useT(); const store = useColumns();
  const meta: ColumnMeta[] = store.metadata[tab] ?? [];
  const [draft, setDraft] = useState<ColumnState[]>([]);
  const [search, setSearch] = useState("");
  const [left, setLeft] = useState<Set<string>>(new Set()); const [right, setRight] = useState<Set<string>>(new Set());
  const [anchor, setAnchor] = useState<string>();
  const [template, setTemplate] = useState(""); const [name, setName] = useState("");
  const [target, setTarget] = useState(""); const [message, setMessage] = useState<string>();
  useEffect(() => {
    if (!isOpen) return;
    setDraft(useColumns.getState().byTab[tab] ?? []); setSearch(""); setLeft(new Set()); setRight(new Set()); setMessage(undefined);
    void useColumns.getState().loadTemplates().catch(() => undefined);
  }, [isOpen, tab]);
  const matches = (key: string) => `${key} ${columnLabel(t, key)}`.toLowerCase().includes(search.toLowerCase());
  const available = draft.filter(c => !c.visible && c.key !== "Row" && matches(c.key)).map(c => c.key);
  const displayed = draft.filter(c => c.visible && c.key !== "Row" && matches(c.key)).map(c => c.key);
  const hint = (key: string) => {
    const needs = meta.find(c => c.key === key)?.needs;
    return needs?.code === "FEATURE_DISABLED" ? t("u20-columns-disabled", { setting: needs.arg ?? "" }) : undefined;
  };
  const select = (list: "left" | "right") => (key: string, event: { shiftKey: boolean; ctrlKey: boolean; metaKey: boolean }) => {
    const keys = list === "left" ? available : displayed;
    const current = list === "left" ? left : right;
    const next = nextSelection(keys, current, anchor, key, { shift: event.shiftKey, toggle: event.ctrlKey || event.metaKey });
    if (list === "left") { setLeft(next); setRight(new Set()); } else { setRight(new Set([...next].filter(k => k !== "Address"))); setLeft(new Set()); }
    if (!event.shiftKey) setAnchor(key);
  };
  const visibleKeys = () => draft.filter(c => c.visible && c.key !== "Row").map(c => c.key);
  const add = () => {
    const keys = [...left];
    const shown = draft.map(c => keys.includes(c.key) ? { ...c, visible: true } : c);
    // New columns are appended after the last displayed column.
    setDraft(reorderVisible(shown, [...visibleKeys(), ...keys.filter(k => !visibleKeys().includes(k))]));
    setRight(new Set(keys)); setLeft(new Set());
  };
  const remove = () => { setDraft(draft.map(c => right.has(c.key) && !fixed(c.key) ? { ...c, visible: false } : c)); setLeft(new Set(right)); setRight(new Set()); };
  const move = (kind: MoveKind) => setDraft(reorderVisible(draft, moveKeys(visibleKeys(), right, kind)));
  const loadTemplate = () => {
    const chosen = store.templates.find(x => x.name === template); if (!chosen) return;
    const result = applyTemplateColumns(draft, chosen.columns); setDraft(result.columns);
    setMessage(result.missing.length ? t("u20-columns-missing", { count: result.missing.length, columns: result.missing.join(", ") }) : t("u20-columns-loaded", { name: chosen.name }));
  };
  const applyTo = async () => {
    if (!template || !target) return;
    const { missing, saved } = await store.applyTemplate(target, template);
    setMessage(missing.length ? t("u20-columns-missing-tab", { tab: label(target), count: missing.length, columns: missing.join(", ") }) : t("u20-columns-applied", { tab: label(target) }));
    await saved;
  };
  const fail = (error: unknown) => setMessage(String(error instanceof Error ? error.message : error));
  return <Dialog title={t("u20-columns-title")} isOpen={isOpen} onOpenChange={onOpenChange} size="large"
    footer={<DialogFooter okLabel={t("u20-ok")} cancelLabel={t("u20-cancel")}
      onOk={() => { void store.change(tab, draft).catch(fail); onOpenChange(false); }} onCancel={() => onOpenChange(false)}
      extra={<Button onPress={() => { setDraft(normalizeColumns(meta)); setLeft(new Set()); setRight(new Set()); }}>{t("u20-columns-defaults")}</Button>} />}>
    <label className="u20-field">{t("u20-columns-search")}<input aria-label={t("u20-columns-search")} className="os-input" value={search} onChange={e => setSearch(e.target.value)} /></label>
    <div className="u20-columns">
      <ColumnList label={t("u20-columns-available")} keys={available} selected={left} onSelect={select("left")} hint={hint} />
      <div className="u20-column-buttons">
        <Button isDisabled={!left.size} onPress={add}>{t("u20-columns-add")}</Button>
        <Button isDisabled={!right.size} onPress={remove}>{t("u20-columns-remove")}</Button>
        <Button isDisabled={!right.size} onPress={() => move("top")}>{t("u20-columns-top")}</Button>
        <Button isDisabled={!right.size} onPress={() => move("up")}>{t("u20-columns-up")}</Button>
        <Button isDisabled={!right.size} onPress={() => move("down")}>{t("u20-columns-down")}</Button>
        <Button isDisabled={!right.size} onPress={() => move("bottom")}>{t("u20-columns-bottom")}</Button>
      </div>
      <ColumnList label={t("u20-columns-displayed")} keys={displayed} selected={right} onSelect={select("right")} hint={hint} />
    </div>
    <fieldset className="u20-templates">
      <legend>{t("u20-templates")}</legend>
      <div className="u20-row">
        <label>{t("u20-template")}<select aria-label={t("u20-template")} className="os-input" value={template} onChange={e => { setTemplate(e.target.value); setName(e.target.value); }}>
          <option value="">–</option>
          {store.templates.map(x => <option key={x.name} value={x.name}>{x.name}</option>)}
        </select></label>
        <label>{t("u20-template-name")}<input aria-label={t("u20-template-name")} className="os-input" value={name} onChange={e => setName(e.target.value)} /></label>
      </div>
      <div className="u20-row">
        <Button isDisabled={!name.trim()} onPress={() => { const n = name.trim(); setTemplate(n); setMessage(t("u20-template-saved", { name: n })); void store.saveTemplate(n, exportColumns(draft)).catch(fail); }}>{t("u20-template-save")}</Button>
        <Button isDisabled={!template} onPress={loadTemplate}>{t("u20-template-load")}</Button>
        <Button isDisabled={!template || !name.trim() || name.trim() === template} onPress={() => {
          const n = name.trim();
          if (store.templates.some(x => x.name === n)) { setMessage(t("u20-template-exists", { name: n })); return; }
          setTemplate(n); setMessage(t("u20-template-renamed", { name: n })); void store.renameTemplate(template, n).catch(fail);
        }}>{t("u20-template-rename")}</Button>
        <Button isDisabled={!template} onPress={() => { setTemplate(""); setName(""); void store.deleteTemplate(template).catch(fail); }}>{t("u20-template-delete")}</Button>
      </div>
      <div className="u20-row">
        <label>{t("u20-template-target")}<select aria-label={t("u20-template-target")} className="os-input" value={target} onChange={e => setTarget(e.target.value)}>
          <option value="">–</option>
          {tabs.filter(x => x.id !== tab).map(x => <option key={x.id} value={x.id}>{x.label}</option>)}
        </select></label>
        <Button isDisabled={!template || !target} onPress={() => void applyTo().catch(fail)}>{t("u20-template-apply")}</Button>
      </div>
    </fieldset>
    {message && <p role="status" className="u20-message">{message}</p>}
  </Dialog>;
}
