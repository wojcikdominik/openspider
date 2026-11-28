import { create } from "zustand";
import { z } from "zod";
import { uistate } from "../ipc/uistate";
import { toCmdError, type CmdError } from "../ipc/errors";
import type { UiStateJson } from "../ipc/gen/UiStateJson";
import type { JsonValue } from "../ipc/gen/serde_json/JsonValue";
import { defaultLayout, type Layout } from "../features/shell/layout";

export const UI_STATE_VERSION = 2;
export type UiSections = { [key: string]: JsonValue };
export const windowSchema = z.object({
  x: z.number().finite(), y: z.number().finite(),
  width: z.number().finite().positive(), height: z.number().finite().positive(),
  scaleFactor: z.number().finite().positive().default(1),
});
export type WindowBounds = z.infer<typeof windowSchema>;
const object = (value: unknown): UiSections => value !== null && typeof value === "object" && !Array.isArray(value) ? value as UiSections : {};
const paneSchema = z.object({ share: z.number().finite().positive().max(1), visible: z.boolean(), collapsed: z.boolean() });

/** v1 stored pixel dividers with window dimensions; v2 stores proportions. Foreign slices survive saves. */
export function normalizeUiState(input: unknown, width: number, height: number): { sections: UiSections; layout: Layout } {
  const envelope = z.object({ version: z.number().int().min(1).max(UI_STATE_VERSION), state: z.unknown() }).safeParse(input);
  const sections = envelope.success ? { ...object(envelope.data.state) } : {};
  const defaults = defaultLayout(width, height);
  const raw = { ...object(sections.layout) };
  if (envelope.success && envelope.data.version === 1) {
    const legacyWindow = object(sections.window);
    for (const [pane, key, total] of [["sidebar", "sidebarWidth", legacyWindow.width ?? width], ["detail", "detailHeight", legacyWindow.contentHeight ?? height]] as const) {
      const pixels = raw[key];
      if (typeof pixels === "number" && pixels > 0 && typeof total === "number" && total > 0) {
        raw[pane] = { share: Math.min(1, pixels / total), visible: raw[`${pane}Visible`] !== false, collapsed: raw[`${pane}Collapsed`] === true };
      }
    }
  }
  const layout: Layout = {
    sidebar: paneSchema.safeParse(raw.sidebar).data ?? defaults.sidebar,
    detail: paneSchema.safeParse(raw.detail).data ?? defaults.detail,
  };
  return { sections: { ...sections, layout: layout as unknown as JsonValue }, layout };
}

export interface LayoutState {
  loaded: boolean;
  error: CmdError | null;
  sections: UiSections;
  layout: Layout;
  load(width: number, contentHeight: number): Promise<void>;
  setLayout(layout: Layout): void;
  /** Shared versioned document: tabs, filters, history, visualization and named templates are separate slices. */
  setSection(key: string, value: JsonValue): void;
  /** `tables` contains only per-table columns (including pinning and sort), never named templates. */
  resetColumns(): void;
  /** State portion of “reset all UI settings”; appearance/language preferences remain in settings.*. */
  resetAll(width: number, contentHeight: number): void;
  flush(): Promise<void>;
}
type StateApi = Pick<typeof uistate, "load" | "save">;
export function createLayoutStore(api: StateApi = uistate) {
  let loading: Promise<void> | undefined;
  let revision = 0;
  let saved = -1;
  let queue: Promise<void> = Promise.resolve();
  const ownedSections = ["layout", "window", "tables", "columnTemplates"];
  const changedSections = new Map<string, number>();
  let resetRevision: number | undefined;
  return create<LayoutState>((set, get) => ({
    loaded: false, error: null, sections: {}, layout: defaultLayout(1440, 836),
    load(width, height) {
      loading ??= (async () => {
        try {
          const normalized = normalizeUiState(await api.load(), width, height);
          set({ ...normalized, loaded: true, error: null });
        } catch (e) {
          // A transport failure is not an empty file: do not overwrite an unread state.
          set({ error: toCmdError(e) });
          throw e;
        }
      })().catch((e) => { loading = undefined; throw e; });
      return loading;
    },
    setLayout(layout) {
      revision++;
      set({ layout, sections: { ...get().sections, layout: layout as unknown as JsonValue } });
    },
    setSection(key, value) {
      if (key === "layout") throw new Error("Use setLayout for layout changes");
      revision++;
      changedSections.set(key, revision);
      set({ sections: { ...get().sections, [key]: value } });
    },
    resetColumns() {
      revision++;
      const { tables: _tables, ...sections } = get().sections;
      set({ sections });
    },
    resetAll(width, height) {
      const layout = defaultLayout(width, height);
      revision++;
      resetRevision = revision;
      changedSections.clear();
      set({ layout, sections: { layout: layout as unknown as JsonValue } });
    },
    flush() {
      const run = async () => {
        if (loading) await loading;
        if (!get().loaded || saved === revision) return;
        const writing = revision;
        const local = structuredClone(get().sections);
        const changed = [...changedSections.keys()];
        const reset = resetRevision;
        try {
          // The shared file may have changed since startup. Replace only our slices and
          // explicitly edited foreign keys; resetAll deliberately replaces everything.
          // UI-125/UI-004: the save carries the revision it read, so a slice another feature wrote between this read and
          // the write (e.g. the tab bar's „Übersicht“) is not overwritten; on a conflict the merge is redone on the new file.
          for (let attempt = 0; ; attempt++) {
            const fresh = await api.load();
            const sections = reset === undefined ? { ...object(fresh.state) } : {};
            for (const key of new Set([...ownedSections, ...changed])) {
              if (Object.hasOwn(local, key)) sections[key] = local[key]!;
              else delete sections[key];
            }
            const snapshot: UiStateJson = { version: UI_STATE_VERSION, revision: fresh.revision ?? null, state: sections };
            try {
              await api.save(snapshot);
              break;
            } catch (e) {
              if (toCmdError(e).code !== "Stale" || attempt >= 2) throw e;
            }
          }
          saved = writing;
          for (const [key, changedAt] of changedSections) {
            if (changedAt <= writing) changedSections.delete(key);
          }
          if (resetRevision === reset) resetRevision = undefined;
          set({ error: null });
        } catch (e) {
          set({ error: toCmdError(e) });
          throw e;
        }
      };
      const next = queue.then(run, run);
      queue = next.catch(() => undefined);
      return next;
    },
  }));
}
export const useLayout = createLayoutStore();

/** One-second debounce, serialized writes, explicit flush for the controlled quit path. */
export function persistLayout(store: ReturnType<typeof createLayoutStore> = useLayout): () => void {
  let timer: ReturnType<typeof setTimeout> | undefined;
  const flush = () => { clearTimeout(timer); void store.getState().flush().catch(() => undefined); };
  const off = store.subscribe((state, previous) => {
    if (!state.loaded || state.sections === previous.sections) return;
    clearTimeout(timer);
    timer = setTimeout(flush, 1000);
  });
  window.addEventListener("pagehide", flush);
  return () => { off(); clearTimeout(timer); window.removeEventListener("pagehide", flush); };
}
