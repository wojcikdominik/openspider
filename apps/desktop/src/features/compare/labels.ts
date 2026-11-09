import type { TFn } from "../../i18n/core";
import { columnLabel, filterLabel, tabLabel } from "../../i18n/canon";
import type { FilterComparison } from "./model";

/** UI-121: display text of a canonical comparison value; the value itself stays canonical (export, CLI, IPC). */
const slug = (value: string) => value.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "");
/** OUT-142 categories and the Change Detection classes (`changed`/`notChanged`). */
export const classLabel = (t: TFn, value: string) => t(`compare-class-${slug(value)}`, undefined, { fallback: value });
/** OUT-149 link change filter: Alle / Hinzugefügt / Entfernt / Ankertext geändert / Unverändert. */
export const linkChangeLabel = (t: TFn, value: string) => t(`compare-link-${slug(value)}`, undefined, { fallback: value });
/** Source of the link table; Current/Previous are the role names of the two crawls (as in the comparison dialog). */
export const sourceLabel = (t: TFn, value: string) => value === "Auto" ? t("compare-source-auto") : value;
/** Filter of a comparison row: the core sends the template as `filter` and the resolved name as `label`. */
export const rowFilterLabel = (t: TFn, row: Pick<FilterComparison, "filter" | "label">) => filterLabel(t, { id: row.label || row.filter, labelTemplate: row.filter });
export const rowLabel = (t: TFn, row: Pick<FilterComparison, "tab" | "filter" | "label">) => `${tabLabel(t, row.tab)}: ${rowFilterLabel(t, row)}`;
/** TECH-300 element of the Change Detection tab: named like its tab (Page Titles, H1 …) or its column (Word Count …). */
export function elementLabel(t: TFn, element: string): string {
  if (element === "All") return filterLabel(t, { id: "All" });
  const tab = tabLabel(t, element);
  return tab !== element ? tab : columnLabel(t, element);
}
