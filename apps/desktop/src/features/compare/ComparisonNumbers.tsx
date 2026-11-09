import { useLayoutEffect, useRef, useState } from "react";
import { useFormat, useT } from "../../i18n";
import { classLabel, rowLabel } from "./labels";
import { CATEGORIES, navigateComparison, type FilterComparison, useComparison } from "./model";
/**
 * OUT-142/OUT-143 comparison numbers of the sidebar. Titles and numbers follow the UI language (UI-121, UI-007); when the
 * seven count columns do not fit the sidebar, the table scrolls horizontally with a visible hint and a fixed title column.
 */
export function ComparisonNumbers({ issues = false }: { issues?: boolean }) {
  const t = useT(); const f = useFormat(); const state = useComparison(); const [descending, setDescending] = useState(true);
  const scroller = useRef<HTMLDivElement>(null); const [clipped, setClipped] = useState(false);
  const data = state.snapshot?.[issues ? "issues" : "overview"] ?? [];
  const rows = issues ? [...data].sort((a,b) => {
    const missing = Number(a.change==null)-Number(b.change==null);
    const delta = a.change!=null && b.change!=null ? (descending ? b.change-a.change : a.change-b.change) : 0;
    return missing || delta || `${a.tab}/${a.filter}`.localeCompare(`${b.tab}/${b.filter}`,"en");
  }) : data;
  useLayoutEffect(() => {
    const element = scroller.current; if (!element) return;
    const measure = () => setClipped(element.scrollWidth > element.clientWidth + 1);
    measure();
    const observer = typeof ResizeObserver === "undefined" ? undefined : new ResizeObserver(measure);
    observer?.observe(element);
    return () => observer?.disconnect();
  }, [rows.length, t]);
  const click = (r: FilterComparison, index: number) => {
    navigateComparison(r.tab,r.filter,index >= 3 ? CATEGORIES[index-3] : undefined,index === 1 ? "Previous" : "Current");
    if(r.tab==="Change Detection")useComparison.setState(s=>({query:{...s.query,class:"changed",side:"Current"}}));
  };
  const heading = (name: string) => name === "Current" || name === "Previous" ? name : name === "Change" ? t("compare-column-change") : classLabel(t, name);
  return <div className="os-compare-numbers">
    {state.snapshot?.notes.map((note,i) => <p role="status" key={i}>{t(`compare-note-${note.kind}`, { address: note.address ?? "", winner: note.winner ?? "", rule: note.rule ?? 0 })}</p>)}
    {clipped && <p className="os-compare-scroll-hint" data-testid="compare-scroll-hint">{t("compare-scroll-hint")}</p>}
    <div className="os-compare-numbers-scroll" ref={scroller} tabIndex={clipped ? 0 : undefined}>
    <table aria-label={t(issues ? "compare-issues" : "compare-overview")}><thead><tr><th>{t("compare-filter")}</th>
      {["Current","Previous","Change",...CATEGORIES].map(name => <th key={name}>{issues && name === "Change" ? <button type="button" onClick={() => setDescending(v => !v)}>{heading(name)} {descending ? "↓" : "↑"}</button> : heading(name)}</th>)}
    </tr></thead><tbody>{rows.map(row => <tr key={`${row.tab}/${row.filter}`}><th scope="row">{rowLabel(t, row)}</th>
      {[row.current,row.previous,row.change,...(row.classes ?? [null,null,null,null])].map((count,i) => <td key={i}>{count == null ? "–" : i === 2 ? f.integer(count) : <button type="button" onClick={() => click(row,i)}>{f.integer(count)}</button>}</td>)}
    </tr>)}</tbody></table>
    </div>
  </div>;
}
