import { useDeferredValue, useMemo, useSyncExternalStore } from "react";
import { useT, useLanguageStore } from "../../i18n";
import { useSession } from "../../state/session";
import { currentPlatform } from "../../ui/platform";
import { buildMenuModel, flattenItems } from "../../ui/menu/build";
import { allMenuContributions, onMenuContributionsChanged } from "../../ui/menu/registry";

let revision = 0;
const subscribe = (changed: () => void) => onMenuContributionsChanged(() => { revision++; changed(); });
const snapshot = () => revision;

/** Native menu APIs expose no hover event; the status line provides the reasons by action name. */
export function MenuAvailability() {
  // The list of unavailable actions is informational: rebuilding it after a focus change or a new contribution
  // runs as a deferred, interruptible render and never delays the response to the input itself (UI-145).
  const contributions = useDeferredValue(useSyncExternalStore(subscribe, snapshot));
  const t = useT();
  const lang = useLanguageStore(s => s.lang);
  // Only the menu context, not every live tick (crawl progress), may rebuild the model (UI-018/UI-145).
  const hasCrawl = useSession(s => !!s.crawl);
  const crawlRunning = useSession(s => s.crawl?.phase === "running");
  const hasData = useSession(s => (s.crawl?.crawled ?? 0) > 0);
  const analysisRunning = useSession(s => s.analysis?.running === true);
  const entries = useMemo(() => {
    const model = buildMenuModel(allMenuContributions(), { platform: currentPlatform(), lang, t, native: false, ctx: { hasCrawl, crawlRunning, hasData, analysisRunning } });
    return [...new Map(flattenItems(model.menus.flatMap(menu => menu.items)).filter(item => !item.enabled && item.disabledReason).map(item => [item.id, item])).values()];
    // `contributions` is the revision of the registered menus (actions, focus context, states of their owners).
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [contributions, lang, t, hasCrawl, crawlRunning, hasData, analysisRunning]);
  if (!entries.length) return null;
  return <details className="os-menu-availability">
    <summary>{t("u04-unavailable-actions", { count: entries.length })}</summary>
    {/* Gründe aus dem Export-Katalog nennen ihren Eintrag bereits selbst. */}
    <ul>{entries.map(item => <li key={item.id}>{item.disabledReason!.startsWith(`${item.label}:`) ? item.disabledReason : `${item.label}: ${item.disabledReason}`}</li>)}</ul>
  </details>;
}
