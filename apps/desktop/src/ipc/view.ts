// Generiert von scripts/gen-ipc.mjs aus src/ipc/commands.json – nicht von Hand ändern (npm run gen:ipc).
import type { CopyArgs } from "./gen/CopyArgs";
import type { CopyResult } from "./gen/CopyResult";
import type { KeyRange } from "./gen/KeyRange";
import type { LocateArgs } from "./gen/LocateArgs";
import type { RangeArgs } from "./gen/RangeArgs";
import type { RowWindow } from "./gen/RowWindow";
import type { RowsArgs } from "./gen/RowsArgs";
import type { ViewCloseArgs } from "./gen/ViewCloseArgs";
import type { ViewHandle } from "./gen/ViewHandle";
import type { ViewOpenArgs } from "./gen/ViewOpenArgs";
import type { ViewUpdateArgs } from "./gen/ViewUpdateArgs";
import { client } from "./client";

export const view = {
  /** `view.open` */
  open: (args: ViewOpenArgs, signal?: AbortSignal) => client.invoke<ViewHandle>("view.open", args, signal),
  /** `view.update` */
  update: (args: ViewUpdateArgs, signal?: AbortSignal) => client.invoke<ViewHandle>("view.update", args, signal),
  /** `view.rows` */
  rows: (args: RowsArgs, signal?: AbortSignal) => client.invoke<RowWindow>("view.rows", args, signal),
  /** `view.locate` */
  locate: (args: LocateArgs, signal?: AbortSignal) => client.invoke<number | null>("view.locate", args, signal),
  /** `view.range` */
  range: (args: RangeArgs, signal?: AbortSignal) => client.invoke<KeyRange>("view.range", args, signal),
  /** `view.copy` */
  copy: (args: CopyArgs, signal?: AbortSignal) => client.invoke<CopyResult>("view.copy", args, signal),
  /** `view.close` */
  close: (args: ViewCloseArgs, signal?: AbortSignal) => client.invoke<void>("view.close", args, signal),
} as const;
