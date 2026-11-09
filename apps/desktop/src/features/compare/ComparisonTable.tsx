import type { ColumnState } from "../../grid/DataGrid";
import { ComparisonGrid, useComparisonColumns, useComparisonSource } from "./ComparisonGrid";
import { downloadComparison } from "./export";
import { useRef, useState } from "react";
import { useT } from "../../i18n";
import { describeError, toCmdError } from "../../ipc/errors";
import { ColumnsMenu } from "../table/ColumnsMenu";
import { ELEMENT_FILTERS, CATEGORIES, navigateComparison, selectComparisonUrl, setComparisonSide, useComparison } from "./model";
export function ComparisonTabs() {
  const t=useT(); const state=useComparison();
  const tabs=[...new Set(state.snapshot?.overview.map(row=>row.tab) ?? [])].filter(tab=>tab!=="Change Detection");tabs.push("Change Detection");
  return <div role="tablist" aria-label={t("compare-tabs")} className="os-compare-tabs">{tabs.map(tab=><button type="button" role="tab" aria-selected={state.query.tab===tab} key={tab} onClick={()=>navigateComparison(tab,"All")}>{tab}</button>)}</div>;
}
/**
 * OUT-142 main table of the comparison. It fills the table region like the ordinary table (UI-007, `.u10-table`):
 * tool row on top, one virtualized grid over the complete population (UI-140), row counter below.
 */
export function ComparisonTable() {
  const t=useT(); const state=useComparison(); const [exporting,setExporting]=useState(false);
  const configuration=useRef(new Map<string, ColumnState[]>());
  const { source, error, setError } = useComparisonSource(state.query, state.revision, state.query.tab);
  const { columns, setColumns, reset } = useComparisonColumns(source, configuration.current);
  const changed=state.query.tab==="Change Detection";
  const filters=changed ? ["All",...(state.session?.elements ?? []).map(e=>ELEMENT_FILTERS[e] ?? e)] : [...new Set(state.snapshot?.overview.filter(r=>r.tab===state.query.tab).map(r=>r.filter) ?? ["All"])];
  const exportView=()=>{setExporting(true);void downloadComparison(state.query,"comparison.csv").catch(setError).finally(()=>setExporting(false));};
  return <section className="os-compare-table" aria-label={t("compare-table")}>
    <div className="os-compare-form os-compare-toolbar"><label>{t("compare-filter")}<select aria-label={t("compare-filter")} value={state.query.filter} onChange={e=>{navigateComparison(state.query.tab,e.target.value);if(changed && e.target.value!=="All")useComparison.setState(s=>({query:{...s.query,class:"changed"}}));}}>{filters.map(f=><option key={f}>{f}</option>)}</select></label>
      <label>{t("compare-category")}<select aria-label={t("compare-category")} value={state.query.class ?? ""} onChange={e=>useComparison.setState({query:{...state.query,class:(e.target.value || undefined) as typeof state.query.class},selected:null})}>
        <option value="">{t("compare-all")}</option>{(changed ? ["changed","notChanged"] : CATEGORIES).map(c=><option key={c} value={c}>{c}</option>)}</select></label>
      {!changed && <label>{t("compare-source")}<select aria-label={t("compare-source")} value={state.query.side} onChange={e=>{setComparisonSide(e.target.value as "Current"|"Previous");}}><option>Current</option><option>Previous</option></select></label>}
      {!changed && <label>{t("compare-search")}<input type="search" value={state.query.search ?? ""} onChange={e=>useComparison.setState({query:{...state.query,search:e.target.value},selected:null})} /></label>}
      <button type="button" disabled={!source || exporting} onClick={exportView}>{t("compare-export")}</button>
      {source && <ColumnsMenu columns={columns} onChange={setColumns} onReset={reset} />}
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
