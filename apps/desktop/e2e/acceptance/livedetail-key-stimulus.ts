// acceptance: independent
// Browser-Modul: von Vite aufgelöst, ohne Node-/Playwright-Importe.
import { flushSync } from "react-dom";

// spec: UI-012, UI-145
// Fixture-Nachweis: Auswahlwechsel-Test in livedetail-detail.spec.ts, reale Grid-Key-Handler.
// Diskrete Nutzereingaben haben jeweils einen eigenen React-Commit. Ohne diese Grenze würden
// synthetische Ereignisse im selben JS-Takt dieselbe alte Fokuszeile mehrfach ansteuern.
export function rapidArrows(grid: Element, count: number): number {
  const start = performance.now();
  for (let i = 0; i < count; i++) {
    flushSync(() => {
      grid.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown", code: "ArrowDown", bubbles: true, cancelable: true }));
      grid.dispatchEvent(new KeyboardEvent("keyup", { key: "ArrowDown", code: "ArrowDown", bubbles: true, cancelable: true }));
    });
  }
  return performance.now() - start;
}
