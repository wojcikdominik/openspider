import { createRoot } from "react-dom/client";
import { bootWindow } from "../../src/app/runtime";
import { setLanguage } from "../../src/i18n";
import { settings as settingsApi } from "../../src/ipc/settings";
import { client } from "../../src/ipc/client";
import { usePrefs } from "../../src/state/prefs";
import { useSession } from "../../src/state/session";
import { SettingsFrame } from "../../src/features/settings/frame/SettingsFrame";
import resources from "../../src/features/settings/sections/storage/fixture.json";
import pem from "../../src/features/settings/sections/network/fixtures/ca.pem.txt?raw";
import "../../src/ui/ui.css";

const runtime = bootWindow({ kind: "settings", menu: false });
await runtime.ready;
await setLanguage(new URLSearchParams(location.search).get("lang") ?? "en");
const core = window.__osMock;
if (core) {
  core.restartKeys.clear();
  core.handlers.set("settings.resources", () => resources);
  core.handlers.set("crawls.checkLocation", () => ["warn-location-sync"]);
  core.handlers.set("crawls.list", () => [{ crawl: { crawlId: "fixture-old", phase: "completed" } }]);
  core.handlers.set("crawls.move", () => ({ opId: "fixture-move" }));
  core.handlers.set("settings.probeCertificates", () => ({ certificates: [pem], trusted: false }));
  core.handlers.set("settings.importCertificates", () => ["/fixture/certificate.pem"]);
  core.handlers.set("settings.setProxyPassword", () => null);
  core.handlers.set("api.list", () => []);
  // UI-131 / OUT-254: injected running version (`?version=`), environment lock (`?updateEnv=1`) and a failing save
  // (`?channelFails=1`). The channel is stored through `settings.set`, so it survives a reload like settings.toml.
  const query = new URLSearchParams(location.search);
  const version = query.get("version") ?? "1.0.1";
  const channelState = () => {
    const raw = core.settings.updates.channel;
    const prerelease = /^[^+]*-/.test(version);
    return { channel: raw === "stable" || raw === "beta" ? raw : prerelease ? "beta" : "stable", version: version.split("+")[0], prerelease };
  };
  core.handlers.set("update.status", () => query.get("updateEnv") === "1" ? { disabled: "environment" } : "idle");
  core.handlers.set("update.channel", () => channelState());
  core.handlers.set("update.set_channel", async (args, ctx) => {
    if (query.get("channelFails") === "1") throw new Error("settings.toml not writable");
    await ctx.core.transport(ctx.window).invoke("settings.set", { patch: { updates: { channel: (args as { channel: string }).channel } } });
    return channelState();
  });
  core.handlers.set("update.set_auto", async (args, ctx) => {
    await ctx.core.transport(ctx.window).invoke("settings.set", { patch: { updates: { check: (args as { check: boolean }).check } } });
  });
}
const api = {
  invoke: (command: string, args?: unknown) => client.invoke(command, args),
  calls: () => core?.calls ?? [],
  importCertificate: () => settingsApi.importCertificates({ certificates: [pem] }),
  settings: () => usePrefs.getState().dto,
  completeMove: () => useSession.setState({ ops: { "fixture-move": { opId: "fixture-move", labelKey: "", state: "done", done: 1, total: 1 } } }),
};
declare global { interface Window { settingsFixture: typeof api } }
window.settingsFixture = api;
createRoot(document.getElementById("root")!).render(<main style={{ height: "100vh" }}><SettingsFrame anchor="storage" /></main>);
