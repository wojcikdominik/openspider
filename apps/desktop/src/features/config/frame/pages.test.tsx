// Seiten Spider/Limits/Rendering/Erweitert/robots/Umschreibung/Include-Exclude/Geschwindigkeit/User-Agent/Header/Auth/Profile:
// jedes Feld muss im Schema des Kerns stehen und in beiden Sprachen beschriftet sein. Die Fachlogik des Kerns (Geschwindigkeit,
// User-Agent, Header, Authentifizierung) gehört anderen Paketen; hier steht nur die Bedienung der Oberfläche und deshalb
// kein Spec-Tag dieser Fachfunktionen (nur Tags aus M2-U13: OUT-220 für Konfigurationsdialog und Profile).
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { getT, i18n, setLanguage } from "../../../i18n";
import { createClient, setClient } from "../../../ipc/client";
import { MockCore } from "../../../ipc/transport/mock";
import { useSession } from "../../../state/session";
import uaToml from "../../../../../../crates/os-fetch/resources/user-agents.toml?raw";
import catalogJson from "../../../../../../resources/ai/ai-crawlers.json?raw";
import { ConfigHost } from "./ConfigHost";
import { fieldLabelKey } from "./fields";
import { fieldMeta } from "./schema";
import { resolvePages } from "./registry";
import { closeConfig, openConfig } from "./store";
import { installConfigCore, SCHEMA, type FakeConfigCore } from "./testing";
import { resolveUa } from "../pages/user-agent";
import { UA_PRESETS } from "../pages/user-agent/presets";
import { derivePresets } from "../pages/user-agent/derivePresets";
import { secretRefFor, hostOfPrefix } from "../pages/auth";
import { hostOfUrl, normalizeHost } from "../pages/robots";
import { HIGH_THREADS } from "../pages/speed";

let core: MockCore;
let fake: FakeConfigCore;

async function setup() {
  core = new MockCore();
  fake = installConfigCore(core);
  setClient(createClient(core.transport("main")));
  await setLanguage("en");
  useSession.setState({ crawl: null });
  render(<ConfigHost />);
}
async function open(anchor: string) {
  act(() => openConfig({ anchor }));
  return screen.findByRole("dialog", { name: "Crawl configuration" });
}
const type = (el: HTMLElement, value: string) => fireEvent.change(el, { target: { value } });

afterEach(() => {
  act(() => closeConfig());
  cleanup();
});

describe("Seitenregister", () => {
  // spec: OUT-220
  it("jedes Feld jeder Seite steht im Schema des Kerns und hat Beschriftungen in Deutsch und Englisch", () => {
    const pages = resolvePages().filter((p) => p.Component);
    expect(pages.map((p) => p.id)).toEqual(
      expect.arrayContaining(["spider-crawl", "spider-extraction", "spider-limits", "spider-rendering", "spider-advanced", "spider-presets", "robots", "rewrite", "include-exclude", "speed", "user-agent", "headers", "auth", "profiles"]),
    );
    const seen = new Set<string>();
    for (const page of pages) {
      for (const f of page.fields) {
        expect(fieldMeta(SCHEMA, f.path), `${page.id}: ${f.path} fehlt im Schema`).not.toBeNull();
        expect(seen.has(f.path), `${f.path} doppelt`).toBe(false);
        seen.add(f.path);
        const key = f.labelKey ?? fieldLabelKey(f.path);
        for (const lang of ["de", "en"]) expect(i18n.has(lang, key), `${lang}: ${key}`).toBe(true);
        const meta = fieldMeta(SCHEMA, f.path);
        for (const value of meta?.enum ?? []) for (const lang of ["de", "en"]) expect(i18n.has(lang, `config-e-${f.path.replace(/[._]/g, "-")}-${value.replace(/_/g, "-")}`), `${lang}: ${f.path}=${value}`).toBe(true);
      }
      for (const lang of ["de", "en"]) expect(i18n.has(lang, page.titleKey), page.titleKey).toBe(true);
    }
  });

  // spec: OUT-220
  it("jede Seite rendert alle ihre Felder ohne Hinweis auf unbekannte Einstellungen", async () => {
    await setup();
    const dialog = await open("limits");
    const pages = resolvePages().filter((p) => p.Component && p.fields.length > 0);
    for (const page of pages) {
      fireEvent.click(within(dialog).getByRole("button", { name: getT()(page.titleKey) }));
      await waitFor(() => expect(dialog.querySelector(`[data-page-id="${page.id}"]`)).not.toBeNull());
      expect(dialog.textContent, page.id).not.toContain("is not known to this version");
      for (const f of page.fields.filter((x) => !x.hidden)) {
        expect(dialog.querySelector(`[data-setting-anchor="${f.path}"]`), `${page.id}: ${f.path}`).not.toBeNull();
      }
    }
  });
});

describe("User-Agent", () => {
  // Kein Spec-Tag: Fachfunktion eines anderen Pakets, hier nur Bedienung der Oberfläche.
  it("die Voreinstellungsliste entspricht den Datendateien des Kerns (Kennung, Anzeigename, Token, User-Agent)", () => {
    // user-agents.toml plus KI-Crawler aus resources/ai/ai-crawlers.json (CRAWL-114 [M4], M4-S01).
    expect(UA_PRESETS.map((p) => ({ ...p }))).toEqual(derivePresets(uaToml, catalogJson));
  });

  // Kein Spec-Tag: Fachfunktion eines anderen Pakets, hier nur Bedienung der Oberfläche.
  it("wirksamer User-Agent und Token folgen der Auflösung des Kerns", () => {
    expect(resolveUa({ preset: "googlebot-desktop" }, "1.0")).toMatchObject({ token: "Googlebot", missingToken: false });
    expect(resolveUa({ preset: "openspider" }, "1.2.3").http).toBe("Mozilla/5.0 (compatible; OpenSpider/1.2.3; +https://openspider.de)");
    expect(resolveUa({ preset: "chrome-desktop" }, null).token).toBe("*");
    expect(resolveUa({ preset: "custom", http: "MyBot/1" }, null)).toMatchObject({ http: "MyBot/1", token: "*", missingToken: true });
    expect(resolveUa({ preset: "custom", http: "MyBot/1", robots_token: "MyBot" }, null)).toMatchObject({ token: "MyBot", missingToken: false });
  });

  // spec: OUT-220
  it("eine eigene Angabe schaltet auf „Benutzerdefiniert“ und warnt ohne robots.txt-Token", async () => {
    await setup();
    const dialog = await open("user_agent");
    expect(within(dialog).getByTestId("ua-effective")).toHaveTextContent("OpenSpider");
    type(within(dialog).getByRole("textbox", { name: "HTTP user agent" }), "MyBot/1.0");
    await waitFor(() => expect(within(dialog).getByTestId("ua-missing-token")).toBeInTheDocument());
    type(within(dialog).getByRole("textbox", { name: "robots.txt user agent (token)" }), "MyBot");
    await waitFor(() => expect(within(dialog).queryByTestId("ua-missing-token")).toBeNull());
    fireEvent.click(within(dialog).getByRole("button", { name: "OK" }));
    await waitFor(() => expect(fake.setCalls).toHaveLength(1));
    expect(fake.config.user_agent).toMatchObject({ preset: "custom", http: "MyBot/1.0", robots_token: "MyBot" });
  });
});

describe("Geschwindigkeit, Header, Authentifizierung, Voreinstellungen", () => {
  beforeEach(() => setup());

  // Kein Spec-Tag: Fachfunktion eines anderen Pakets, hier nur Bedienung der Oberfläche.
  it("ab der Warnschwelle erscheint ein Hinweis auf die Serverlast", async () => {
    const dialog = await open("speed.threads");
    expect(within(dialog).queryByTestId("threads-warning")).toBeNull();
    type(within(dialog).getByRole("textbox", { name: "Max. threads" }), String(HIGH_THREADS));
    await waitFor(() => expect(within(dialog).getByTestId("threads-warning")).toHaveTextContent("20 threads"));
    type(within(dialog).getByRole("textbox", { name: "Max. threads" }), "999");
    await waitFor(() => expect(within(dialog).getByText(/darf höchstens 200 sein/)).toBeInTheDocument());
  });

  // spec: OUT-220
  it("optionale Rate: ohne Haken kein Limit, mit Wert eine Dezimalzahl (0,5 → 0.5)", async () => {
    const dialog = await open("speed.max_urls_per_sec");
    const rate = within(dialog).getByRole("textbox", { name: "Max. URLs per second" });
    expect(rate).toBeDisabled();
    fireEvent.click(within(dialog).getByRole("checkbox", { name: "Not limited" }));
    await waitFor(() => expect(rate).toBeEnabled());
    type(rate, "0,5");
    fireEvent.click(within(dialog).getByRole("button", { name: "OK" }));
    await waitFor(() => expect(fake.setCalls).toHaveLength(1));
    expect((fake.config.speed as Record<string, unknown>).max_urls_per_sec).toBe(0.5);
  });

  // spec: OUT-220
  it("Header-Tabelle und Accept-Language werden in die Konfiguration geschrieben", async () => {
    const dialog = await open("http.headers");
    fireEvent.click(await within(dialog).findByRole("button", { name: "Add header" }));
    type(within(dialog).getByRole("textbox", { name: "Header name, row 1" }), "X-Test");
    type(within(dialog).getByRole("textbox", { name: "Header value, row 1" }), "1");
    type(within(dialog).getByRole("textbox", { name: "Host, row 1" }), "stage.example.test");
    type(within(dialog).getByRole("textbox", { name: "Accept-Language value" }), "de-DE,de;q=0.9");
    fireEvent.click(within(dialog).getByRole("button", { name: "OK" }));
    await waitFor(() => expect(fake.setCalls).toHaveLength(1));
    const http = fake.config.http as { headers: unknown[]; accept_language: string };
    expect(http.headers).toEqual([{ name: "X-Test", value: "1", host: "stage.example.test" }]);
    expect(http.accept_language).toBe("de-DE,de;q=0.9");
  });

  // spec: OUT-220
  it("Passwörter gehen nie in die Konfiguration: nur an auth.saveProfile, in der Konfiguration bleibt der Verweis", async () => {
    core.handlers.set("auth.saveProfile", (args) => ({ ...(args as { profile: Record<string, unknown> }).profile, secret: undefined, createdAt: "2026-10-20", hasSecret: true }));
    const dialog = await open("auth.basic");
    fireEvent.click(within(dialog).getByRole("button", { name: "Add row" }));
    type(within(dialog).getByRole("textbox", { name: "URL prefix, row 1" }), "https://stage.example.test/app/");
    type(within(dialog).getByRole("textbox", { name: "User name, row 1" }), "tester");
    const pw = within(dialog).getByLabelText("Password, row 1") as HTMLInputElement;
    expect(pw.type).toBe("password");
    fireEvent.change(pw, { target: { value: "s3cret-TEST" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "Save password" }));
    await waitFor(() => expect(within(dialog).getByText("Password saved in the keychain.")).toBeInTheDocument());
    const call = core.calls.find((c) => c.cmd === "auth.saveProfile")!.args as { profile: Record<string, unknown> };
    expect(call.profile).toMatchObject({ name: "basic-stage-example-test-app", kind: "basic", host: "stage.example.test", username: "tester", secret: "s3cret-TEST" });
    expect(pw.value).toBe("");
    fireEvent.click(within(dialog).getByRole("button", { name: "OK" }));
    await waitFor(() => expect(fake.setCalls).toHaveLength(1));
    expect(JSON.stringify(fake.setCalls)).not.toContain("s3cret-TEST");
    expect(JSON.stringify(core.calls.filter((c) => c.cmd !== "auth.saveProfile"))).not.toContain("s3cret-TEST");
    expect((fake.config.auth as { basic: unknown[] }).basic).toEqual([{ prefix: "https://stage.example.test/app/", username: "tester", secret_ref: "basic-stage-example-test-app" }]);
  });

  // Kein Spec-Tag: Fachfunktion eines anderen Pakets, hier nur Bedienung der Oberfläche.
  it("nicht verfügbare Authentifizierungsprofile (Stub des Kerns) erscheinen als Meldung statt als Absturz", async () => {
    const dialog = await open("auth.basic");
    fireEvent.click(within(dialog).getByRole("button", { name: "Add row" }));
    type(within(dialog).getByRole("textbox", { name: "URL prefix, row 1" }), "https://stage.example.test/");
    fireEvent.change(within(dialog).getByLabelText("Password, row 1"), { target: { value: "x" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "Save password" }));
    await waitFor(() => expect(within(dialog).getByText(/not available yet|Error:/)).toBeInTheDocument());
  });

  // spec: OUT-220
  it("Voreinstellungen: „Zurücksetzen“ stellt nur die Werte der Gruppe wieder her", async () => {
    const dialog = await open("title.max_chars");
    const group = dialog.querySelector('[data-group="title"]') as HTMLElement;
    const max = within(group).getByRole("textbox", { name: "Max. characters" }) as HTMLInputElement;
    const min = within(group).getByRole("textbox", { name: "Min. characters" }) as HTMLInputElement;
    type(max, "70");
    type(min, "10");
    const other = dialog.querySelector('[data-group="h1"]') as HTMLElement;
    type(within(other).getByRole("textbox", { name: "H1: max. characters" }), "80");
    fireEvent.click(within(group).getByRole("button", { name: /Reset Page titles to factory settings/ }));
    await waitFor(() => expect(max.value).toBe("60"));
    expect(min.value).toBe("30");
    expect((within(other).getByRole("textbox", { name: "H1: max. characters" }) as HTMLInputElement).value).toBe("80");
  });
});

describe("Hilfsfunktionen", () => {
  // spec: CRAWL-102
  it("Hostnamen für eigene robots.txt-Dateien werden ohne Schema und Pfad geführt", () => {
    expect(normalizeHost("https://Blog.Example.test/pfad?x#y")).toBe("blog.example.test");
    expect(normalizeHost("  shop.example.test ")).toBe("shop.example.test");
    expect(hostOfUrl("https://blog.example.test:8080/a")).toBe("blog.example.test:8080");
    expect(hostOfUrl("blog.example.test/x")).toBe("blog.example.test");
    expect(hostOfUrl("http://")).toBeNull();
  });

  // Kein Spec-Tag: Fachfunktion eines anderen Pakets, hier nur Bedienung der Oberfläche.
  it("Schlüsselbund-Verweise sind stabil aus dem URL-Präfix abgeleitet", () => {
    expect(secretRefFor("https://Stage.Example.test/App/", 0)).toBe("basic-stage-example-test-app");
    expect(secretRefFor("", 2)).toBe("basic-3");
    expect(hostOfPrefix("https://stage.example.test/app/")).toBe("stage.example.test");
  });
});
