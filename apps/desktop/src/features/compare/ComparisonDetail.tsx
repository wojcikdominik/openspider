import { useState } from "react";
import { useT } from "../../i18n";
import { ComparisonLinks } from "./ComparisonLinks";
import { useComparison } from "./model";
export function ComparisonDetail() {
  const t=useT();const selected=useComparison(s=>s.selected);const [direction,setDirection]=useState<"inlinks"|"outlinks">("inlinks");
  return <section className="os-compare-detail" aria-label={t("detail-region")}>
    <div role="tablist" aria-label={t("detail-region")} className="os-compare-detail-tabs">{(["inlinks","outlinks"] as const).map(id=><button type="button" role="tab" aria-selected={direction===id} key={id} onClick={()=>setDirection(id)}>{t(`detail-tab-${id}`)}</button>)}</div>
    {selected==null ? <p className="os-compare-message">{t("compare-select-url")}</p> : <ComparisonLinks direction={direction} urlId={selected} />}
  </section>;
}
