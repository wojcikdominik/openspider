// Geschwindigkeit (CRAWL-112/113/151): Threads (wirken live, auch während „Läuft“), Verbindungen je Host, Rate, Drosselung.
import { useT } from "../../../../i18n";
import { Banner } from "../../../../ui/primitives";
import { useConfig } from "../../frame/context";
import { definePage, f } from "../../frame/definePage";
import type { FieldDef } from "../../frame/types";

/** Ab so vielen Threads erscheint ein Hinweis auf die Last für den Zielserver (CRAWL-112). */
export const HIGH_THREADS = 20;

const fields: FieldDef[] = [
  f("speed.threads", { sectionKey: "config-sec-speed-parallel" }),
  f("speed.max_per_host"),
  f("speed.connections_per_host", { hintKey: "config-hint-connections-per-host" }),
  f("speed.auto"),
  f("speed.max_urls_per_sec", { sectionKey: "config-sec-speed-rate", nullLabelKey: "config-unlimited-rate", hintKey: "config-hint-rate" }),
  f("speed.adaptive_throttle"),
];

function ThreadsNote() {
  const t = useT();
  const { draft } = useConfig();
  const threads = draft.get<number | undefined>("speed.threads");
  if (typeof threads !== "number" || threads < HIGH_THREADS) return null;
  return (
    <Banner kind="warning">
      <span data-testid="threads-warning">{t("config-threads-high", { threads })}</span>
    </Banner>
  );
}

export const page = definePage({
  id: "speed",
  order: 95,
  fields,
  anchors: ["speed"],
  descriptionKey: "config-speed-desc",
  After: ThreadsNote,
});
