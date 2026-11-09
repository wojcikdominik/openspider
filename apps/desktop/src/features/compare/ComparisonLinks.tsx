import type { ColumnState } from "../../grid/DataGrid";
import type { SortKey } from "../../ipc/gen/SortKey";
import { ComparisonGrid, useComparisonColumns, useComparisonSource } from "./ComparisonGrid";
import { downloadComparison } from "./export";
import { useMemo, useRef, useState } from "react";
import { useT } from "../../i18n";
import { describeError, toCmdError } from "../../ipc/errors";
import { ColumnsMenu } from "../table/ColumnsMenu";
import { linkChangeLabel, sourceLabel } from "./labels";
import { LINK_CHANGES, type ComparisonQuery, type Side, useComparison } from "./model";
/** OUT-149: links of the selected URL in both crawls, one virtualized grid over all links (UI-140, UI-032 detail tables). */
export function ComparisonLinks({direction,urlId}:{direction:"inlinks"|"outlinks";urlId:number}) {
  const t=useT();const side=useComparison(s=>s.query.side);const revision=useComparison(s=>s.revision);
  const [source,setSource]=useState<"Auto"|Side>("Auto");const [change,setChange]=useState("All");const [exporting,setExporting]=useState(false);
  const configuration=useRef(new Map<string, ColumnState[]>());
  const [sort,setSort]=useState<SortKey>();
  const query=useMemo<ComparisonQuery>(()=>({tab:direction,filter:"All",side,urlId,source,change,sort}),[direction,side,urlId,source,change,JSON.stringify(sort ?? null)]);
  const view=useComparisonSource(query, revision);
  const { columns, setColumns, reset } = useComparisonColumns(view.source, configuration.current);
  const exportView=()=>{setExporting(true);void downloadComparison(query,`${direction}-comparison.csv`).catch(view.setError).finally(()=>setExporting(false));};
  const enabled=view.source?.filtersEnabled;
  return <section aria-label={direction} className="os-compare-links"><h3>{t(`detail-tab-${direction}`)}</h3><div className="os-compare-form os-compare-toolbar">
    <label>{t("compare-source")}<select aria-label={t("compare-source")} value={source} onChange={e=>setSource(e.target.value as typeof source)}>{["Auto","Current","Previous"].map(v=><option key={v} value={v}>{sourceLabel(t,v)}</option>)}</select></label>
    <label>{t("compare-link-change")}<select aria-label={t("compare-link-change")} disabled={!enabled} value={enabled===false ? "All" : change} onChange={e=>setChange(e.target.value)}>{LINK_CHANGES.map(v=><option key={v} value={v} disabled={v!=="All" && !enabled}>{linkChangeLabel(t,v)}</option>)}</select></label>
    <button type="button" disabled={!view.source || exporting} onClick={exportView}>{t("compare-export")}</button>
    {view.source && <ColumnsMenu columns={columns} onChange={setColumns} onReset={reset} />}
  </div>{view.error!=null && <p role="alert" className="os-compare-message">{describeError(t, toCmdError(view.error)).text}</p>}{!view.source && view.error==null && <p role="status" className="os-compare-message">{t("compare-loading")}</p>}
    {view.source ? <ComparisonGrid source={view.source} columns={columns} onColumnsChange={setColumns} label={t("compare-links-table")} sort={sort} onSort={setSort}
      onError={view.setError} onExport={exportView} /> : <div className="os-compare-grid-host" />}
    {view.source && <p role="status" className="os-compare-counter">{t("compare-total",{count:view.source.total})}</p>}
  </section>;
}
