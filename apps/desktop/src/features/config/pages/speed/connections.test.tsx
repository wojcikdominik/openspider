// Konfiguration › Geschwindigkeit › „Verbindungen pro Host“ (CRAWL-151): Auswahl `auto` oder 1–6 aus dem Schema des Kerns,
// beschriftet in Deutsch und Englisch, schreibt `"auto"` als Text und Zahlen als Ganzzahl in die Konfiguration.
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { i18n, setLanguage } from "../../../../i18n";
import { createClient, setClient } from "../../../../ipc/client";
import { MockCore } from "../../../../ipc/transport/mock";
import { useSession } from "../../../../state/session";
import { ConfigHost } from "../../frame/ConfigHost";
import { fieldMeta } from "../../frame/schema";
import { closeConfig, openConfig } from "../../frame/store";
import { installConfigCore, SCHEMA, type FakeConfigCore } from "../../frame/testing";

let fake: FakeConfigCore;

async function setup(lang: "en" | "de") {
  const core = new MockCore();
  fake = installConfigCore(core);
  setClient(createClient(core.transport("main")));
  await setLanguage(lang);
  useSession.setState({ crawl: null });
  render(<ConfigHost />);
}

async function open(name: string) {
  act(() => openConfig({ anchor: "speed.connections_per_host" }));
  return screen.findByRole("dialog", { name });
}

const speed = () => fake.config.speed as Record<string, unknown>;

afterEach(() => {
  act(() => closeConfig());
  cleanup();
});

describe("Verbindungen pro Host", () => {
  // spec: CRAWL-151
  it("liest Auswahl und Grenzen aus dem Schema und ist in beiden Sprachen beschriftet", () => {
    expect(fieldMeta(SCHEMA, "speed.connections_per_host")).toMatchObject({ type: "integer", minimum: 1, maximum: 6, choices: ["auto"], default: "auto" });
    for (const lang of ["de", "en"])
      for (const key of ["config-f-speed-connections-per-host", "config-e-speed-connections-per-host-auto", "config-hint-connections-per-host"]) expect(i18n.has(lang, key), `${lang}: ${key}`).toBe(true);
  });

  describe("English", () => {
    beforeEach(() => setup("en"));

    // spec: CRAWL-151
    it("defaults to automatic and stores a fixed number as an integer", async () => {
      const dialog = await open("Crawl configuration");
      const field = dialog.querySelector('[data-setting-anchor="speed.connections_per_host"]') as HTMLElement;
      expect(field).not.toBeNull();
      const button = within(field).getByRole("button", { name: /Connections per host/ });
      expect(button).toHaveTextContent("Automatic (as many as threads, at most 6)");
      expect(field).toHaveTextContent("Separate TCP connections per host");
      fireEvent.click(button);
      const options = await screen.findAllByRole("option");
      expect(options.map((o) => o.textContent)).toEqual(["Automatic (as many as threads, at most 6)", "1", "2", "3", "4", "5", "6"]);
      fireEvent.click(screen.getByRole("option", { name: "2" }));
      fireEvent.click(within(dialog).getByRole("button", { name: "OK" }));
      await waitFor(() => expect(fake.setCalls).toHaveLength(1));
      expect(speed().connections_per_host).toBe(2);
    });

    // spec: CRAWL-151
    it("switching back to automatic writes the keyword", async () => {
      fake.config = { ...fake.config, speed: { ...speed(), connections_per_host: 4 } };
      const dialog = await open("Crawl configuration");
      const field = dialog.querySelector('[data-setting-anchor="speed.connections_per_host"]') as HTMLElement;
      const button = within(field).getByRole("button", { name: /Connections per host/ });
      await waitFor(() => expect(button).toHaveTextContent("4"));
      fireEvent.click(button);
      fireEvent.click(await screen.findByRole("option", { name: /Automatic/ }));
      fireEvent.click(within(dialog).getByRole("button", { name: "OK" }));
      await waitFor(() => expect(fake.setCalls).toHaveLength(1));
      expect(speed().connections_per_host).toBe("auto");
    });
  });

  describe("Deutsch", () => {
    beforeEach(() => setup("de"));

    // spec: CRAWL-151
    it("zeigt Beschriftung, Standard und Hinweis auf Deutsch", async () => {
      const dialog = await open("Crawl-Konfiguration");
      const field = dialog.querySelector('[data-setting-anchor="speed.connections_per_host"]') as HTMLElement;
      expect(within(field).getByRole("button", { name: /Verbindungen pro Host/ })).toHaveTextContent("Automatisch (so viele wie Threads, höchstens 6)");
      expect(field).toHaveTextContent("Unter HTTP/1.1 trägt eine Verbindung eine Anfrage zur Zeit.");
    });
  });
});
