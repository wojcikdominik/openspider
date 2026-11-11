// Unabhängige Abnahme rc.1, Crawl-delay-Hinweise in der Statusleiste (Vitest, jsdom).
// acceptance: independent
// Erwartungen allein aus der Spec: CRAWL-145 „Anzeige (Nachtrag 11.11.2026, Fehlerbehebung rc.1)“ und UI-013.
// - Jeder Crawl-delay-Hinweis mit gleichem Wortlaut (gleiche Ursache, gleiche Sekundenzahl) steht genau einmal in der
//   Statusleiste, auch wenn er für mehrere Hosts gilt.
// - Die betroffenen Hosts stehen im Tooltip des Hinweises.
// - `http` und `https` desselben Hosts zählen als ein Host.
// - Wortlaut nach CRAWL-145 (OS-Festlegung K2-76): nicht angewandt „Crawl-delay n s in robots.txt – nicht angewandt“.
// Die Meldungen kommen wie im Betrieb als Notice `crawl-delay` je robots.txt-Origin (öffentliche IPC-Schnittstelle).
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { act, cleanup, render } from "@testing-library/react";
import { Statusbar } from "./Statusbar";
import { setupHeader, tick } from "../header/fixtures/setup";
import { attachSession, useSession } from "../../state/session";
import { createEventBus } from "../../ipc/events";
import { setLanguage } from "../../i18n";
import type { Notice } from "../../ipc/gen/Notice";

beforeEach(async () => { setupHeader(); await setLanguage("en"); vi.useFakeTimers(); });
afterEach(() => { cleanup(); vi.useRealTimers(); });

const delay = (origin: string, seconds: number, applied = false): Notice => ({
  level: "info", code: "crawl-delay",
  args: { origin, seconds, applied, effectiveSeconds: applied ? Math.min(60, seconds) : 0, capped: applied && seconds > 60 },
});

/** Laufender Crawl aus der Fixture `tick-running.json`; die Hinweise kommen über den Event-Bus wie vom Kern. */
function running(notices: Notice[], viaTick = false) {
  const bus = createEventBus(); const off = attachSession(bus);
  const view = render(<Statusbar />);
  act(() => {
    if (viaTick) useSession.getState().applyTick({ ...tick, notices });
    else {
      useSession.getState().applyTick(tick);
      for (const notice of notices) bus.emit({ kind: "notice", notice });
    }
    vi.advanceTimersByTime(1000);
  });
  return { ...view, bus, off };
}

/** Innerste Elemente, deren Text „Crawl-delay“ enthält – also die sichtbaren Hinweise. */
function hints(root: HTMLElement): HTMLElement[] {
  const all = [...root.querySelectorAll<HTMLElement>("*")].filter(el => /Crawl-delay/.test(el.textContent ?? ""));
  return all.filter(el => ![...el.children].some(child => /Crawl-delay/.test(child.textContent ?? "")));
}
function hintsMatching(root: HTMLElement, pattern: RegExp): HTMLElement[] {
  return hints(root).filter(el => pattern.test(el.textContent ?? ""));
}
/** Tooltip des Hinweises: `title` am Element oder einem Vorfahren innerhalb der Statusleiste, sonst `aria-describedby`. */
function tooltip(el: HTMLElement): string {
  for (let node: HTMLElement | null = el; node; node = node.parentElement) {
    const title = node.getAttribute("title");
    if (title) return title;
    const described = node.getAttribute("aria-describedby");
    if (described) return described.split(/\s+/).map(id => document.getElementById(id)?.textContent ?? "").join(" ");
    if (node.getAttribute("data-testid") === "crawl-status") break;
  }
  return "";
}
const occurrences = (text: string, host: string) =>
  (text.match(new RegExp(`(^|[^\\w.-])${host.replace(/[.]/g, "\\.")}(?![\\w.-])`, "g")) ?? []).length;

// spec: CRAWL-145
// spec: UI-013
it("Fixture tick-running: drei Hosts mit gleichem Crawl-delay ergeben genau einen Hinweis, alle drei Hosts im Tooltip", () => {
  const { container } = running([
    delay("https://alpha.test", 2), delay("https://beta.test", 2), delay("https://gamma.test", 2),
  ]);
  const found = hintsMatching(container, /Crawl-delay 2(?:[.,]0+)? s in robots\.txt/);
  expect(found).toHaveLength(1);
  expect(hints(container)).toHaveLength(1);
  expect(found[0]!.textContent).toMatch(/Crawl-delay 2(?:[.,]0+)? s in robots\.txt – not applied/);
  const tip = tooltip(found[0]!);
  for (const host of ["alpha.test", "beta.test", "gamma.test"]) expect(occurrences(tip, host)).toBe(1);
});

// spec: CRAWL-145
it("http und https desselben Hosts zählen als ein Host: ein Hinweis, Host genau einmal im Tooltip", () => {
  const { container } = running([delay("http://shop.test", 3), delay("https://shop.test", 3)]);
  const found = hintsMatching(container, /Crawl-delay 3(?:[.,]0+)? s/);
  expect(found).toHaveLength(1);
  expect(occurrences(tooltip(found[0]!), "shop.test")).toBe(1);
});

// spec: CRAWL-145
it("http/https desselben Hosts plus weitere Hosts: ein Hinweis, jeder Host genau einmal im Tooltip", () => {
  const { container } = running([
    delay("http://alpha.test", 4), delay("https://alpha.test", 4), delay("https://beta.test", 4), delay("http://gamma.test", 4),
  ], true);
  const found = hintsMatching(container, /Crawl-delay 4(?:[.,]0+)? s/);
  expect(found).toHaveLength(1);
  const tip = tooltip(found[0]!);
  expect(occurrences(tip, "alpha.test")).toBe(1);
  expect(occurrences(tip, "beta.test")).toBe(1);
  expect(occurrences(tip, "gamma.test")).toBe(1);
});

// spec: CRAWL-145
it("zwei unterschiedliche Sekundenzahlen ergeben zwei Hinweise, je genau einmal und mit den richtigen Hosts", () => {
  const { container } = running([
    delay("https://alpha.test", 2), delay("https://beta.test", 5), delay("https://gamma.test", 2), delay("http://beta.test", 5),
  ]);
  expect(hints(container)).toHaveLength(2);
  const two = hintsMatching(container, /Crawl-delay 2(?:[.,]0+)? s/);
  const five = hintsMatching(container, /Crawl-delay 5(?:[.,]0+)? s/);
  expect(two).toHaveLength(1);
  expect(five).toHaveLength(1);
  expect(occurrences(tooltip(two[0]!), "alpha.test")).toBe(1);
  expect(occurrences(tooltip(two[0]!), "gamma.test")).toBe(1);
  expect(occurrences(tooltip(two[0]!), "beta.test")).toBe(0);
  expect(occurrences(tooltip(five[0]!), "beta.test")).toBe(1);
  expect(occurrences(tooltip(five[0]!), "alpha.test")).toBe(0);
});

// spec: CRAWL-145
it("gleiche Sekundenzahl, aber andere Ursache (angewandt / nicht angewandt) ergibt zwei verschiedene Hinweise", () => {
  const { container } = running([delay("https://alpha.test", 2, true), delay("https://beta.test", 2, false)]);
  const found = hintsMatching(container, /Crawl-delay 2(?:[.,]0+)? s/);
  expect(found).toHaveLength(2);
  expect(new Set(found.map(el => el.textContent)).size).toBe(2);
  const notApplied = found.find(el => /not applied/.test(el.textContent ?? ""))!;
  const applied = found.find(el => !/not applied/.test(el.textContent ?? ""))!;
  expect(occurrences(tooltip(notApplied), "beta.test")).toBe(1);
  expect(occurrences(tooltip(notApplied), "alpha.test")).toBe(0);
  expect(occurrences(tooltip(applied), "alpha.test")).toBe(1);
});

// spec: CRAWL-145
it("wiederholt gemeldete Hinweise (dieselben Origins erneut) bleiben ein Hinweis ohne doppelte Hosts", () => {
  const { container, bus } = running([delay("https://alpha.test", 2), delay("https://beta.test", 2)]);
  act(() => {
    for (const origin of ["https://alpha.test", "http://alpha.test", "https://beta.test", "https://alpha.test"]) bus.emit({ kind: "notice", notice: delay(origin, 2) });
    vi.advanceTimersByTime(1000);
  });
  const found = hintsMatching(container, /Crawl-delay 2(?:[.,]0+)? s/);
  expect(found).toHaveLength(1);
  expect(occurrences(tooltip(found[0]!), "alpha.test")).toBe(1);
  expect(occurrences(tooltip(found[0]!), "beta.test")).toBe(1);
});

// spec: CRAWL-145
// spec: UI-013
it("Deutsch: drei Hosts mit Crawl-delay 1,5 s ergeben genau einen Hinweis im Wortlaut der Spec", async () => {
  await act(async () => { await setLanguage("de"); });
  const { container } = running([delay("https://alpha.test", 1.5), delay("http://beta.test", 1.5), delay("https://gamma.test", 1.5)]);
  const found = hintsMatching(container, /Crawl-delay 1,5 s in robots\.txt/);
  expect(found).toHaveLength(1);
  expect(hints(container)).toHaveLength(1);
  expect(found[0]!.textContent).toMatch(/Crawl-delay 1,5 s in robots\.txt – nicht angewandt/);
  const tip = tooltip(found[0]!);
  for (const host of ["alpha.test", "beta.test", "gamma.test"]) expect(occurrences(tip, host)).toBe(1);
});
