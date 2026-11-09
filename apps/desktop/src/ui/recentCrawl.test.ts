import { describe, expect, it } from "vitest";
import { fileName, middleEllipsis, recentCrawlTitle, RECENT_PATH_MAX, splitPath } from "./recentCrawl";

const mac = "/Users/dominikwojcik/Library/Application Support/de.openspider.app/crawls/01a18894-1234-7000-8000-bbfe3c06b178.ospider";
const win = "C:\\Users\\Dominik\\AppData\\Roaming\\de.openspider.app\\crawls\\Kunde A\\shop.example.com 2026-11-09.ospider";

// spec: CRAWL-127
describe("middleEllipsis (OS-Festlegung 09.11.2026)", () => {
  it("shortens a long macOS path in the middle and keeps start, parent directory and file name", () => {
    const short = middleEllipsis(mac);
    expect(short.length).toBeLessThanOrEqual(RECENT_PATH_MAX);
    expect(short).toMatch(/^\/Users\/domi/);
    expect(short.endsWith("…/crawls/01a18894-1234-7000-8000-bbfe3c06b178.ospider")).toBe(true);
    expect(fileName(short)).toBe(fileName(mac));
  });

  it("treats backslashes as separators in Windows paths", () => {
    const short = middleEllipsis(win);
    expect(short.length).toBeLessThanOrEqual(RECENT_PATH_MAX);
    expect(short).toMatch(/^C:\\Users\\/);
    expect(short.endsWith("…\\Kunde A\\shop.example.com 2026-11-09.ospider")).toBe(true);
    expect(fileName(short)).toBe("shop.example.com 2026-11-09.ospider");
    expect(splitPath(win)).toEqual({ head: "C:\\Users\\Dominik\\AppData\\Roaming\\de.openspider.app\\crawls", tail: "\\Kunde A\\shop.example.com 2026-11-09.ospider" });
  });

  it("returns short paths unchanged", () => {
    for (const path of ["/fixture/Alpha.ospider", "C:\\crawls\\a.ospider", "a.ospider"]) expect(middleEllipsis(path)).toBe(path);
  });

  it("keeps the file name when the parent directory is long and shortens only an overlong file name itself", () => {
    const longParent = `/Volumes/Daten/${"x".repeat(40)}/report.ospider`;
    expect(middleEllipsis(longParent, 40)).toBe(`/Volumes/Daten/${"x".repeat(9)}…/report.ospider`);
    const longFile = `/tmp/${"n".repeat(80)}.ospider`;
    const short = middleEllipsis(longFile);
    expect(short).toHaveLength(RECENT_PATH_MAX);
    expect(short.startsWith("/tmp/")).toBe(true);
    expect(short.endsWith(".ospider")).toBe(true);
    expect(middleEllipsis("n".repeat(80))).toHaveLength(RECENT_PATH_MAX);
  });
});

// spec: CRAWL-127
describe("recentCrawlTitle", () => {
  // Local time without zone: the expected wall-clock time does not depend on the test machine's time zone.
  const meta = { name: "caloone.de", startUrl: "https://www.caloone.de/", startedAt: "2026-11-09T14:32:00" };

  it("shows the crawl name with date and time of the crawl", () => {
    expect(recentCrawlTitle(meta, "de")).toBe("caloone.de – 09.11.2026, 14:32");
    expect(recentCrawlTitle(meta, "en")).toMatch(/^caloone\.de – Nov 9, 2026, 2:32\sPM$/);
  });

  it("falls back to the host of the start URL and leaves unknown crawls to the path", () => {
    expect(recentCrawlTitle({ ...meta, name: " " }, "de")).toBe("www.caloone.de – 09.11.2026, 14:32");
    expect(recentCrawlTitle({ name: "Shop", startedAt: null }, "de")).toBe("Shop");
    expect(recentCrawlTitle(undefined, "de")).toBeUndefined();
  });
});
