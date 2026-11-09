import type { ColumnState } from "../../grid/DataGrid";
import { ComparisonGrid, mergeOrdinaryColumns, splitOrdinaryColumns, useComparisonColumns, useComparisonSource } from "./ComparisonGrid";
import { downloadComparison } from "./export";
import { useEffect, useMemo, useRef, useState } from "react";
import { useT } from "../../i18n";
import { describeError, toCmdError } from "../../ipc/errors";
import { exportColumns, useColumns } from "../../state/columns";
import { ColumnsMenu } from "../table/ColumnsMenu";
import { useTabs } from "../../state/tabs";
import { tabId } from "../../ui/primitives/TabBar";
import { classLabel, elementLabel, rowFilterLabel } from "./labels";
import { ELEMENT_FILTERS, CATEGORIES, navigateComparison, selectComparisonUrl, setComparisonSide, useComparison } from "./model";

const CHANGE_DETECTION = "Change Detection";
/**
 * OUT-142 main table of the comparison. It fills the table region like the ordinary table (UI-007, `.u10-table`):
 * tool row on top, one virtualized grid over the complete population (UI-140), row counter below. The data tab comes from
 * the ordinary tab bar (K2-77, model.ts), the columns from the ordinary column configuration of that tab (UI-009).
 */
export function ComparisonTable() {
  const t=useT(); const state=useComparison(); const [exporting,setExporting]=useState(false);
  const configuration=useRef(new Map<string, ColumnState[]>());
  const [local,setLocal]=useState(()=>new Map<string, ColumnState>());
  const changed=state.query.tab===CHANGE_DETECTION;
  const ordinary=useColumns(s=>changed ? undefined : s.byTab[state.query.tab]);
  const meta=useColumns(s=>changed ? undefined : s.metadata[state.query.tab]);
  useEffect(()=>{ if(!changed) void useColumns.getState().load(state.query.tab).catch(()=>undefined); },[state.query.tab,changed]);
  const projection=useMemo(()=>ordinary ? exportColumns(ordinary) : [],[ordinary]);
  const { source, error, setError } = useComparisonSource(state.query, state.revision, changed || !meta ? { catalogTab: state.query.tab } : { columns: projection, meta });
  const own = useComparisonColumns(source, configuration.current);
  const columns = !changed && ordinary ? mergeOrdinaryColumns(ordinary, source, local) : own.columns;
  const setColumns = (next: ColumnState[]) => {
    if (changed || !ordinary) { own.setColumns(next); return; }
    const parts = splitOrdinaryColumns(next);
    setLocal(new Map(parts.comparison.map(c => [c.key, c])));
    void useColumns.getState().change(state.query.tab, parts.ordinary).catch(setError);
  };
  const reset = () => {
    if (changed || !ordinary) { own.reset(); return; }
    setLocal(new Map()); void useColumns.getState().reset(state.query.tab).catch(setError);
  };
  const filters=changed ? ["All",...(state.session?.elements ?? []).map(e=>ELEMENT_FILTERS[e] ?? e)].map(f=>({id:f,label:elementLabel(t,f)}))
    : (state.snapshot?.overview.filter(r=>r.tab===state.query.tab) ?? []).map(r=>({id:r.filter,label:rowFilterLabel(t,r)}));
  const options=filters.length ? filters : [{id:"All",label:rowFilterLabel(t,{filter:"All",label:"All"})}];
  const exportView=()=>{setExporting(true);void downloadComparison(state.query,"comparison.csv",changed ? [] : projection).catch(setError).finally(()=>setExporting(false));};
  // UI-004: the ordinary tab bar controls this panel (`aria-controls="data-panel"`), as it controls the ordinary table.
  const panelTab=useTabs(s=>s.view==="tabs" && s.active===state.query.tab && s.catalog.some(tab=>tab.id===s.active) ? s.active : null);
  return <section className="os-compare-table" role="tabpanel" id="data-panel" aria-labelledby={panelTab ? tabId("data-panel", panelTab) : undefined} aria-label={panelTab ? undefined : t("compare-table")}>
    <div className="os-compare-form os-compare-toolbar"><label>{t("compare-filter")}<select aria-label={t("compare-filter")} value={state.query.filter} onChange={e=>{navigateComparison(state.query.tab,e.target.value);if(changed && e.target.value!=="All")useComparison.setState(s=>({query:{...s.query,class:"changed"}}));}}>{[...new Map(options.map(o=>[o.id,o])).values()].map(f=><option key={f.id} value={f.id}>{f.label}</option>)}</select></label>
      <label>{t("compare-category")}<select aria-label={t("compare-category")} value={state.query.class ?? ""} onChange={e=>useComparison.setState({query:{...state.query,class:(e.target.value || undefined) as typeof state.query.class},selected:null})}>
        <option value="">{t("compare-all")}</option>{(changed ? ["changed","notChanged"] : CATEGORIES).map(c=><option key={c} value={c}>{classLabel(t,c)}</option>)}</select></label>
      {!changed && <label>{t("compare-source")}<select aria-label={t("compare-source")} value={state.query.side} onChange={e=>{setComparisonSide(e.target.value as "Current"|"Previous");}}><option value="Current">Current</option><option value="Previous">Previous</option></select></label>}
      {!changed && <label>{t("compare-search")}<input type="search" value={state.query.search ?? ""} onChange={e=>useComparison.setState({query:{...state.query,search:e.target.value},selected:null})} /></label>}
      <button type="button" disabled={!source || exporting} onClick={exportView}>{t("compare-export")}</button>
      {/* Stays mounted while a changed projection reloads: the dialog previews each change live. */}
      {columns.length > 0 && <ColumnsMenu columns={columns} onChange={setColumns} onReset={reset} />}
    </div>
    {error != null && <p role="alert" className="os-compare-message">{describeError(t, toCmdError(error)).text}</p>}
    {!source && error == null && <p role="status" className="os-compare-message">{t("compare-loading")}</p>}
    {source?.contentAvailable===false && <p role="status" className="os-compare-message">{t("compare-content-unavailable")}</p>}
    {source ? <ComparisonGrid source={source} columns={columns} onColumnsChange={setColumns} selectedKey={state.selected} label={t("compare-table")} sort={state.query.sort}
      onSort={sort=>useComparison.setState({query:{...state.query,sort},selected:null,selectedPartner:null})}
      onSelect={key=>selectComparisonUrl(key,source.side,source.partner(key))}
      onError={setError} onExport={exportView} /> : <div className="os-compare-grid-host" />}
    {source && <p role="status" className="os-compare-counter">{t("compare-total",{count:source.total})}{source.total===0 && <> · {t("compare-empty")}</>}</p>}
  </section>;
}
/** „Übersicht“ of the grouped navigation during a comparison (UI-150: „Im Vergleichsmodus nicht verfügbar“). */
export function ComparisonOverviewUnavailable() {
  const t=useT();
  return <section className="os-compare-table" aria-label={t("compare-table")}><p role="status" className="os-compare-message">{t("compare-overview-unavailable")}</p></section>;
}
