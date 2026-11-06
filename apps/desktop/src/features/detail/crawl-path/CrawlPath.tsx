import { useState } from "react";
import { useT } from "../../../i18n";
import { Banner } from "../../../ui/primitives/Feedback";
import type { SortKey } from "../../../ipc/gen/SortKey";
import { TablePanel } from "../frame/TablePanel";
import type { DetailPanelProps } from "../frame/types";
export const CRAWL_PATH_COLUMNS = ["Crawl Depth", "Address", "Anchor", "Link Type", "Target Address", "Step", "Status Code"];
const HIDDEN_COLUMNS = ["Target Address", "Step", "Status Code"];
export function CrawlPath(props: DetailPanelProps) {
  const t = useT(); const [sort, setSort] = useState<SortKey[]>([]);
  // UI-050: the grid fills its parent; a dialog body has no height of its own, so this container provides it.
  return <div className="os-crawl-path">{props.listMode && <Banner>{t("detail-direct-upload")}</Banner>}
    <TablePanel {...props} title={t("detail-tab-crawlPath")} pinAddress={false} columns={CRAWL_PATH_COLUMNS} hiddenColumns={HIDDEN_COLUMNS} columnTitles={{ "Crawl Depth": "Depth", Anchor: "Anchor Text" }}
      spec={{ kind: "detail", detail: "crawlPath", urlIds: props.selection.urlIds, params: {}, sort }} onSort={setSort} /></div>;
}
