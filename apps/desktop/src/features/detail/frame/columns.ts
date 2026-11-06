import { useEffect, useRef, useState } from "react";
import { scheduleDetailState } from "./persistence";
import { z } from "zod";
import { uistate } from "../../../ipc/uistate";
import type { ColumnState } from "../../../grid/DataGrid";
const columnSchema = z.array(z.object({ key: z.string(), width: z.number().min(32).max(4000), visible: z.boolean(), pinned: z.boolean(), sort: z.enum(["asc", "desc"]).optional() }));
/**
 * Edits on their way to the UI state (saved after a delay). UI-008/UI-030: a table rebuilt by a selection change within
 * that delay reads the choice from here instead of the older saved state; an entry leaves only once its save succeeded.
 */
const unsaved = new Map<string, ColumnState[]>();
/** Each detail table owns a layout, including separate layouts for Links and Anchors. */
export function useDetailColumns(id: string, onError: (error: unknown) => void): readonly [ColumnState[], (columns: ColumnState[]) => void] {
  const [state, setState] = useState<{ id: string; columns: ColumnState[]; edited: boolean }>(() => ({ id, columns: unsaved.get(id) ?? [], edited: false }));
  const edited = useRef(false);
  useEffect(() => {
    let active = true; edited.current = false;
    // The answer may overtake the save of an edit that is pending now: that edit stays newer than the answer.
    const pendingAtStart = unsaved.get(id);
    void uistate.load().then(dto => {
      const newer = unsaved.get(id) ?? pendingAtStart;
      if (newer) { if (active && !edited.current) setState({ id, columns: newer, edited: false }); return; }
      const root = dto.state && typeof dto.state === "object" && !Array.isArray(dto.state) ? dto.state : {};
      const layouts = root.detailColumns && typeof root.detailColumns === "object" && !Array.isArray(root.detailColumns) ? root.detailColumns : {};
      const result = columnSchema.safeParse(layouts[id]);
      if (active && !edited.current) setState({ id, columns: result.success ? result.data : [], edited: false });
    }).catch(onError);
    return () => { active = false; };
  }, [id]);
  return [state.id === id ? state.columns : [], columns => {
    edited.current = true; setState({ id, columns, edited: true });
    unsaved.set(id, columns);
    scheduleDetailState(`columns:${id}`, root => {
      const layouts = root.detailColumns && typeof root.detailColumns === "object" && !Array.isArray(root.detailColumns) ? root.detailColumns : {};
      return { ...root, detailColumns: { ...layouts, [id]: columns.map(column => ({ ...column })) } };
    }, onError, () => { if (unsaved.get(id) === columns) unsaved.delete(id); });
  }];
}
