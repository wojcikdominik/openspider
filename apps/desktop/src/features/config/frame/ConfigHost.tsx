// Einmal je Hauptfenster: registriert die Aktionen (Kürzel Mod+K, Mod+Umschalt+C), die Menübeiträge und hält den Dialog.
import { useEffect } from "react";
import { registerAction } from "../../../ui/actions";
import { registerMenuContributions } from "../../../ui/menu/registry";
import { ConfigDialog } from "./ConfigDialog";
import { configMenu } from "./menu";
import { openConfig, useConfigDialog } from "./store";

/** Menüeinträge des Fachmenüs „Konfiguration“ (UI-002) und ihr Sprungziel im Dialog (UI-016). */
export const CONFIG_SECTION_ANCHORS: Readonly<Record<string, string>> = {
  "config.content": "content",
  "config.robots": "robots",
  "config.rewrite": "rewrite",
  "config.includeExclude": "scope.include",
  "config.speed": "speed",
  "config.userAgent": "user_agent",
  "config.headers": "http.headers",
  "config.api": "api",
  "config.segments": "segments",
  "config.analysis": "analysis",
  "config.search": "custom_search",
  "config.extraction": "custom_extraction",
  "config.js": "custom_js",
};

export function ConfigHost() {
  const open = useConfigDialog((s) => s.open);
  useEffect(() => {
    const offs = [
      registerAction("config.spider", () => openConfig()),
      registerAction("config.profiles", () => openConfig({ anchor: "profiles" })),
      registerAction("config.diff", () => openConfig({ diff: true })),
      ...Object.entries(CONFIG_SECTION_ANCHORS).map(([id, anchor]) => registerAction(id, () => openConfig({ anchor }))),
    ];
    const offMenu = registerMenuContributions("features/config", configMenu());
    return () => {
      for (const off of offs.reverse()) off();
      offMenu();
    };
  }, []);
  return open ? <ConfigDialog /> : null;
}
