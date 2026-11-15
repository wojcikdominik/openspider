import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { createClient, setClient } from "../../ipc/client";
import type { ApiConnState } from "../../ipc/gen/ApiConnState";
import type { ApiRow } from "../../ipc/gen/ApiRow";
import type { ApiSourceInfo } from "../../ipc/gen/ApiSourceInfo";
import type { CmdError } from "../../ipc/gen/CmdError";
import { MockCore } from "../../ipc/transport/mock";
import { makeT } from "../../i18n";
import { parseUrlLines, startApisMode } from "./apisMode";
import { apiProgress, progressTooltip } from "./progress";
import {
  accountOfProperty,
  ga4StatusParts,
  gscSiteKind,
  gscSiteMatchesCrawl,
  gscSiteUnverified,
  propertiesOf,
  sortGscSites,
  streamHostFilter,
  streamsOf,
  type Ga4Account,
  type GscSite,
} from "./properties";
import { DIALOG_KINDS, LEVELS, levelSelectable, linkMetricColumns } from "./sources";
import { allowedActions, errorMessage, maskedKey, statusText, transition, type ConnEvent } from "./status";
import fixture from "./fixtures/integ-ui.json";

const en = makeT("en");
const de = makeT("de");
const err = (messageKey: string, detail?: string, args: Record<string, unknown> = {}): CmdError =>
  ({ code: "Io", messageKey, args, detail: detail ?? null, retryable: false }) as CmdError;
const ga4 = fixture.ga4Accounts as Ga4Account[];
const sites = fixture.gscSites as GscSite[];

let core: MockCore;
beforeEach(() => {
  core = new MockCore();
  setClient(createClient(core.transport("main")));
});
afterEach(() => setClient(null));

describe("INTEG-001 Statusautomat", () => {
  // spec: INTEG-001
  it("lässt genau die spezifizierten Übergänge zu", () => {
    const states: ApiConnState[] = ["disconnected", "connecting", "connected", "error"];
    const events: ConnEvent[] = ["connect", "succeeded", "failed", "disconnect"];
    const allowed = new Map<string, ApiConnState>([
      ["disconnected:connect", "connecting"],
      ["connecting:succeeded", "connected"],
      ["connecting:failed", "error"],
      ["connected:disconnect", "disconnected"],
      ["error:disconnect", "disconnected"],
    ]);
    for (const s of states) for (const e of events) expect(transition(s, e), `${s}:${e}`).toBe(allowed.get(`${s}:${e}`) ?? null);
    expect(allowedActions("disconnected")).toEqual({ connect: true, disconnect: false });
    expect(allowedActions("connecting")).toEqual({ connect: false, disconnect: false });
    expect(allowedActions("connected")).toEqual({ connect: false, disconnect: true });
    expect(allowedActions("error")).toEqual({ connect: false, disconnect: true });
  });

  // spec: INTEG-001
  it("zeigt je Zustand genau einen der vier Status in beiden Sprachen", () => {
    expect(statusText(de, "moz", { state: "disconnected" })).toBe("Nicht verbunden");
    expect(statusText(de, "moz", { state: "connecting" })).toBe("Verbinde…");
    expect(statusText(de, "moz", { state: "connected", parts: ["moz-konto"] })).toBe("Verbunden (moz-konto)");
    expect(statusText(en, "moz", { state: "connected", parts: [] })).toBe("Connected");
    expect(statusText(de, "moz", { state: "error", error: err("err-api-reconnect", "HTTP 401: Invalid token") })).toBe("Fehler: 401 Invalid token");
    expect(statusText(en, "moz", { state: "error", error: err("err-api-reconnect", "HTTP 403: Forbidden") })).toBe("Error: 403 Forbidden");
  });

  // spec: INTEG-001
  it("meldet die Zeitüberschreitung des Test-Requests nach 30 s", () => {
    const timeout = { ...err("err-api-connect-timeout", undefined, { seconds: 30 }), code: "Timeout" } as CmdError;
    expect(statusText(de, "ga4", { state: "error", error: timeout })).toBe("Fehler: Zeitüberschreitung nach 30 s");
    expect(statusText(en, "ga4", { state: "error", error: timeout })).toBe("Error: timed out after 30 s");
  });

  // spec: INTEG-001
  it("Dialog-Katalog enthält bis Release 1.0 genau sechs Einträge in fester Reihenfolge", () => {
    expect(DIALOG_KINDS).toEqual(["ga4", "gsc", "psi", "majestic", "ahrefs", "moz"]);
  });

  // spec: INTEG-001
  it("zeigt Schlüssel nur maskiert mit den letzten vier Zeichen", () => {
    expect(maskedKey("abcd")).toBe("••••••••abcd");
    expect(maskedKey("xyz-1234")).toBe("••••••••1234");
    expect(maskedKey(null)).toBeNull();
  });
});

describe("INTEG-130 PSI-Ablehnung", () => {
  // spec: INTEG-130
  it("übersetzt SERVICE_DISABLED und API_KEY_INVALID in die Ablehnung mit Grund", () => {
    const disabled = err("err-api-reconnect", "HTTP 403: PERMISSION_DENIED SERVICE_DISABLED");
    expect(statusText(de, "psi", { state: "error", error: disabled })).toBe("Fehler: PSI-Zugriff abgelehnt (SERVICE_DISABLED)");
    const invalid = err("err-api-request", "HTTP 400: API_KEY_INVALID");
    expect(errorMessage(en, "psi", invalid)).toBe("PSI access denied (API_KEY_INVALID)");
    // Andere Quellen behalten die Anbietermeldung.
    expect(errorMessage(en, "moz", invalid)).toBe("400 API_KEY_INVALID");
  });
});

describe("INTEG-132 Lokaler Lighthouse-Lauf", () => {
  // spec: INTEG-132
  it("meldet die fehlende Lighthouse-Laufzeit beim Verbinden von Lokal", () => {
    const missing = { ...err("err-api-lighthouse-missing", "API nicht verbunden"), code: "Permission" } as CmdError;
    expect(statusText(de, "psi", { state: "error", error: missing })).toBe("Fehler: Lighthouse-Laufzeit nicht gefunden");
    expect(statusText(en, "psi", { state: "error", error: missing })).toBe("Error: Lighthouse runtime not found");
  });
});

describe("INTEG-007 API-Fortschritt", () => {
  const row = (kind: ApiRow["kind"], completed: number, requested: number, errors: number, connected = true): ApiRow => ({
    kind, labelKey: `api-${kind}`, connected, requested, received: completed - errors, errors, running: completed < requested, completed,
  });

  // spec: INTEG-007
  it("zählt erledigte durch angeforderte URLs über alle verbundenen Integrationen", () => {
    expect(apiProgress([row("psi", 73, 100, 0)]).percent).toBe(73);
    expect(apiProgress([row("psi", 10, 10, 3)])).toMatchObject({ visible: true, percent: 100, parts: [{ kind: "psi", errors: 3 }] });
    expect(apiProgress([row("psi", 99, 100, 0), row("moz", 100, 100, 0)]).percent).toBe(99);
  });

  // spec: INTEG-007
  it("blendet die Anzeige ohne verbundene Integration aus", () => {
    expect(apiProgress([]).visible).toBe(false);
    expect(apiProgress([row("psi", 3, 10, 1, false)]).visible).toBe(false);
  });

  // spec: INTEG-007
  it("schlüsselt im Tooltip je Integration mit Fehlern auf", () => {
    const progress = apiProgress(fixture.status as ApiRow[]);
    expect(progressTooltip(de, progress)).toBe("GA4 fertig (0 Fehler), GSC 400/400 (0 Fehler), PSI 120/400 (3 Fehler)");
    expect(progressTooltip(en, progress)).toBe("GA4 done (0 errors), GSC 400/400 (0 errors), PSI 120/400 (3 errors)");
    expect(progress.percent).toBe(65);
  });
});

describe("INTEG-090 GA4-Auswahl", () => {
  // spec: INTEG-090
  it("zeigt nur Properties des gewählten Kontos, durchsuchbar", () => {
    expect(propertiesOf(ga4, "accounts/100").map((p) => p.id)).toEqual(["properties/1001", "properties/1002"]);
    expect(propertiesOf(ga4, "accounts/200").map((p) => p.id)).toEqual(["properties/2001"]);
    expect(propertiesOf(ga4, "accounts/300")).toEqual([]);
    expect(propertiesOf(ga4, "accounts/100", "app").map((p) => p.name)).toEqual(["Shop App"]);
  });

  // spec: INTEG-090
  it("filtert auf den Hostnamen des Datenstreams; alle Datenstreams filtern nicht", () => {
    const streams = streamsOf(ga4, "properties/1001");
    expect(streams.map((s) => s.id)).toEqual(["streams/1", "streams/2"]);
    expect(streamHostFilter(streams[0])).toBe("hostName==integ.test");
    expect(streamHostFilter(null)).toBeNull();
  });

  // spec: INTEG-090
  it("wählt das Konto einer gespeicherten Property vor und baut den Status Konto / Property / Stream", () => {
    expect(accountOfProperty(ga4, "properties/2001")).toBe("accounts/200");
    expect(accountOfProperty(ga4, null)).toBe("accounts/100");
    expect(ga4StatusParts("analyst@integ.test", ga4, "properties/1001", "streams/1")).toEqual(["analyst@integ.test", "Integ Shop", "Shop Web", "Shop"]);
    expect(statusText(de, "ga4", { state: "connected", parts: ga4StatusParts("analyst@integ.test", ga4, "properties/1001", "streams/1") }))
      .toBe("Verbunden (analyst@integ.test / Integ Shop / Shop Web / Shop)");
  });
});

describe("INTEG-110 Search-Console-Property", () => {
  // spec: INTEG-110
  it("unterscheidet Domain- und URL-Präfix-Properties und graut unbestätigte aus", () => {
    expect(gscSiteKind("sc-domain:integ.test")).toBe("domain");
    expect(gscSiteKind("https://integ.test/")).toBe("prefix");
    expect(sites.filter(gscSiteUnverified).map((s) => s.siteUrl)).toEqual(["https://staging.integ.test/"]);
    expect(sortGscSites(sites).map((s) => s.siteUrl)).toEqual(["sc-domain:integ.test", "sc-domain:other.test", "https://integ.test/", "https://staging.integ.test/"]);
  });

  // spec: INTEG-110
  it("erkennt eine Property, die nicht zur Crawl-Domain passt", () => {
    expect(gscSiteMatchesCrawl("sc-domain:integ.test", "http://other.test/")).toBe(false);
    expect(gscSiteMatchesCrawl("sc-domain:integ.test", "https://www.integ.test/a")).toBe(true);
    expect(gscSiteMatchesCrawl("https://integ.test/", "https://integ.test/blog/")).toBe(true);
    expect(gscSiteMatchesCrawl("https://integ.test/blog/", "https://integ.test/")).toBe(false);
    expect(gscSiteMatchesCrawl("https://integ.test/", "http://integ.test/")).toBe(false);
    expect(gscSiteMatchesCrawl("sc-domain:integ.test", null)).toBe(true);
  });
});

describe("INTEG-150 Ebenen der Linkmetriken", () => {
  // spec: INTEG-150
  it("benennt Spalten `<Anbieter> <Metrik> (<Ebene>)`", () => {
    expect(linkMetricColumns("ahrefs", ["backlinks"], ["url"])).toEqual(["Ahrefs Backlinks (Exact URL)"]);
    expect(linkMetricColumns("moz", ["domain_authority", "page_authority"], ["domain", "url"])).toEqual([
      "Moz Page Authority (Exact URL)",
      "Moz Page Authority (Domain)",
      "Moz Domain Authority (Exact URL)",
      "Moz Domain Authority (Domain)",
    ]);
  });

  // spec: INTEG-150
  it("deaktiviert Ebenen, die der Anbieter nicht anbietet oder die Konfiguration nicht speichern kann", () => {
    for (const kind of ["majestic", "ahrefs", "moz"] as const) {
      expect(LEVELS.filter((l) => levelSelectable(kind, l))).toEqual(["url", "subdomain", "domain"]);
      expect(levelSelectable(kind, "url_both")).toBe(false);
    }
    expect(linkMetricColumns("majestic", ["TrustFlow"], ["url_both"])).toEqual([]);
  });
});

describe("INTEG-012 APIs-Modus", () => {
  const connected = (fixture.sources as ApiSourceInfo[]).map((s) => (s.kind === "psi" ? { ...s, state: "connected" as const, connected: true } : s));

  // spec: INTEG-012
  it("fragt doppelte URLs der Liste nur einmal ab", () => {
    expect(parseUrlLines(fixture.apiList)).toEqual({
      items: ["https://integ.test:8080/a/", "https://integ.test:8080/b/", "https://integ.test:8080/c", "https://integ.test:8080/d?x=1"],
      duplicates: 1,
    });
  });

  // spec: INTEG-012
  it("startet ohne verbundene Integration keinen Lauf", async () => {
    expect(await startApisMode(fixture.apiList, fixture.sources as ApiSourceInfo[])).toEqual({ started: false, reason: "no-integration" });
    expect(await startApisMode("", connected)).toEqual({ started: false, reason: "empty-list" });
    expect(core.calls.filter((c) => c.cmd === "crawl.start")).toEqual([]);
  });

  // spec: INTEG-012
  it("startet den Lauf als Quelle `apis` mit der bereinigten Liste", async () => {
    core.handlers.set("crawl.start", () => ({ crawlId: "c-apis", file: "/fixture/apis.ospider" }));
    const result = await startApisMode(fixture.apiList, connected);
    expect(result).toEqual({ started: true, ref: { crawlId: "c-apis", file: "/fixture/apis.ospider" } });
    const calls = core.calls.filter((c) => c.cmd === "crawl.start");
    expect(calls).toHaveLength(1);
    expect(calls[0]?.args).toEqual({ source: { kind: "apis", items: parseUrlLines(fixture.apiList).items }, scope: "domain" });
  });
});
