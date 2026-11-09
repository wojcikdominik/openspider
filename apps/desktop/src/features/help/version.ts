import type { TFn } from "../../i18n";

/**
 * Version with build number, „1.0.0-beta.3 (Build 2578)“ (UI-127, Nutzerentscheidung 09.11.2026); without a build
 * number (developer build, build without Git) only the version. Used by "About", debug info and the update dialog.
 */
export function versionLabel(t: TFn, info: { version: string; buildNumber?: string | null }): string {
  return info.buildNumber ? t("u04-version-build", { version: info.version, build: info.buildNumber }) : info.version;
}
