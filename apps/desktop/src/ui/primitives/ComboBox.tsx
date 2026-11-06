import { useEffect, useId, useRef, useState, type ReactNode, type KeyboardEvent } from "react";
import { Button as RacButton, Popover } from "react-aria-components";
import { ChevronDown } from "lucide-react";
import { cx } from "./cx";

export interface ComboOption {
  id: string;
  label: string;
  /** Zähler hinter dem Namen (Filter-Dropdown, UI-006); `null` = „–“. */
  count?: number | null;
  badge?: ReactNode;
  /** Zusatztext für Screenreader, z. B. „Crawl-Analyse erforderlich“. */
  hint?: string;
  isDisabled?: boolean;
}

/** Editable, nonmodal combobox: DOM focus stays in the input, active option uses aria-activedescendant.
 * RAC's ComboBox hides the rest of the page from AT while leaving its Tab stops enabled (axe aria-hidden-focus).
 * Keep RAC's positioned/dismissable popover, but expose the surrounding page and close on Tab/blur instead.
 */
export function ComboBox({ label, options, selected, onSelect, formatCount = (n) => String(n), className }: {
  label: string;
  options: readonly ComboOption[];
  selected: string | null;
  onSelect: (id: string) => void;
  formatCount?: (n: number) => string;
  className?: string;
}) {
  const id = useId();
  const root = useRef<HTMLDivElement>(null);
  const input = useRef<HTMLInputElement>(null);
  const trigger = useRef<HTMLButtonElement>(null);
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState<string | null>(null);
  const [active, setActive] = useState<string | null>(null);
  const fromTrigger = useRef(false);
  const text = (o: ComboOption) => o.count === undefined ? o.label : `${o.label} (${o.count === null ? "–" : formatCount(o.count)})`;
  const chosen = options.find(o => o.id === selected);
  const filtered = options.filter(o => query === null || text(o).toLowerCase().includes(query.toLowerCase()));
  const enabled = filtered.filter(o => !o.isDisabled);
  const activeId = enabled.some(o => o.id === active) ? active : enabled[0]?.id ?? null;
  const optionId = (key: string) => `${id}-option-${encodeURIComponent(key)}`;
  const close = () => { setOpen(false); setQuery(null); setActive(null); };
  const choose = (option: ComboOption) => {
    if (option.isDisabled) return;
    onSelect(option.id);
    close();
    if (fromTrigger.current) trigger.current?.focus();
    else input.current?.focus();
  };
  useEffect(() => {
    if (open && activeId) document.getElementById(optionId(activeId))?.scrollIntoView({ block: "nearest" });
  }, [open, activeId, id]);

  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    const key = event.key;
    if (key === "Tab") { close(); return; }
    if (key === "Escape" && open) {
      event.preventDefault(); event.stopPropagation(); close();
      if (fromTrigger.current) trigger.current?.focus();
      return;
    }
    if (key === "Enter" && open) {
      event.preventDefault();
      const option = enabled.find(o => o.id === activeId);
      if (option) choose(option);
      return;
    }
    if (key !== "ArrowDown" && key !== "ArrowUp" && !(open && (key === "Home" || key === "End"))) return;
    event.preventDefault();
    if (!open) {
      fromTrigger.current = false; setQuery(null); setOpen(true);
      setActive(key === "ArrowUp" ? enabled.at(-1)?.id ?? null : chosen?.id ?? enabled[0]?.id ?? null);
      return;
    }
    const index = enabled.findIndex(o => o.id === activeId);
    const next = key === "Home" ? 0 : key === "End" ? enabled.length - 1 : Math.max(0, Math.min(enabled.length - 1, index + (key === "ArrowDown" ? 1 : -1)));
    setActive(enabled[next]?.id ?? null);
  };

  return (
    <div ref={root} className={cx("os-field", "os-combobox", className)}>
      <label id={`${id}-label`} htmlFor={`${id}-input`} className="os-label">{label}</label>
      <div className="os-combobox-group">
        <input ref={input} id={`${id}-input`} className="os-input" role="combobox" aria-autocomplete="list"
          aria-expanded={open} aria-controls={open ? `${id}-list` : undefined} aria-activedescendant={open && activeId ? optionId(activeId) : undefined}
          value={query ?? (chosen ? text(chosen) : "")}
          onChange={event => { fromTrigger.current = false; setQuery(event.target.value); setActive(null); setOpen(true); }}
          onKeyDown={onKeyDown} onBlur={event => { if (event.relatedTarget !== trigger.current) close(); }} />
        <RacButton ref={trigger} className="os-combobox-button" aria-label={label} aria-haspopup="listbox" aria-expanded={open}
          aria-controls={open ? `${id}-list` : undefined} onPress={() => {
            if (open) { close(); return; }
            fromTrigger.current = true; setQuery(null); setActive(selected); setOpen(true); input.current?.focus();
          }}>
          <ChevronDown size={14} aria-hidden />
        </RacButton>
      </div>
      {/* M2-N32: the option list exists only while open; a closed filter box costs no per-option work (counts, badges). */}
      {open && <Popover className="os-popover" triggerRef={root} isNonModal isOpen={open} onOpenChange={next => { if (!next) close(); }}>
        <div id={`${id}-list`} role="listbox" aria-labelledby={`${id}-label`} className="os-listbox" onMouseDown={event => event.preventDefault()}>
          {filtered.map(o => (
            <div key={o.id} id={optionId(o.id)} role="option" aria-selected={o.id === selected} aria-disabled={o.isDisabled || undefined}
              aria-description={o.hint} className="os-listbox-item" data-focused={o.id === activeId || undefined}
              data-selected={o.id === selected || undefined} data-disabled={o.isDisabled || undefined}
              onMouseDown={event => event.preventDefault()} onPointerMove={() => { if (!o.isDisabled) setActive(o.id); }} onClick={() => choose(o)}>
              <span className="os-combobox-item">{o.badge}<span>{o.label}</span></span>
              {o.count !== undefined && <span className="os-keyhint">{o.count === null ? "–" : formatCount(o.count)}</span>}
            </div>
          ))}
        </div>
      </Popover>}
    </div>
  );
}
