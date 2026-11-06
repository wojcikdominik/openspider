import { act, cleanup, renderHook, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { createClient, setClient } from "../../../ipc/client";
import { MockCore } from "../../../ipc/transport/mock";
import { useDetailColumns } from "./columns";
import type { ColumnState } from "../../../grid/DataGrid";

const edited: ColumnState[] = [{ key: "Anchor Text", width: 120, visible: false, pinned: false }];
beforeEach(() => { setClient(createClient(new MockCore().transport())); });
afterEach(() => { cleanup(); setClient(null); vi.useRealTimers(); });

// spec: UI-008
// spec: UI-030
it("a table rebuilt before the delayed save sees the column choice; afterwards it comes from the saved state (F1)", async () => {
  const onError = vi.fn();
  const first = renderHook(() => useDetailColumns("inlinks:f1", onError));
  await waitFor(() => expect(first.result.current[0]).toEqual([]));
  act(() => first.result.current[1](edited));
  first.unmount();
  // The selection changed: a new table mounts within the save delay. The choice applies at once and after the load.
  const second = renderHook(() => useDetailColumns("inlinks:f1", onError));
  expect(second.result.current[0]).toEqual(edited);
  await act(async () => { await new Promise(resolve => setTimeout(resolve, 50)); });
  expect(second.result.current[0]).toEqual(edited);
  second.unmount();
  await act(async () => { await new Promise(resolve => setTimeout(resolve, 1200)); });
  const third = renderHook(() => useDetailColumns("inlinks:f1", onError));
  await waitFor(() => expect(third.result.current[0]).toEqual(edited));
  expect(onError).not.toHaveBeenCalled();
});
