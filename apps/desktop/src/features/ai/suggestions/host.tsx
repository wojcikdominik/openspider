// Prüfbereich der KI-Vorschläge (M4-V01, AI-076) neben der Tabelle des Tabs „AI Suggestions“. Die Tabellenregion hat keinen
// Anschluss für tab-eigene Bereiche; der Bereich läuft deshalb als eigene React-Wurzel am Dokument (eingehängt aus
// `features/ai/tabs.ts`, additive Registrierung) und erscheint nur mit Schalter „KI (Vorschau)“ und aktivem Tab.
import { useEffect, useState } from "react";
import { createRoot, type Root } from "react-dom/client";
import { useT } from "../../../i18n";
import { useTabs } from "../../../state/tabs";
import { useDetailSelection } from "../../detail/frame/selection";
import { useAiPreview } from "../preview";
import { SUGGESTIONS_TAB } from "./model";
import { ReviewPanel } from "./ReviewPanel";

/** Der Bereich selbst: sichtbar nur mit Schalter an und aktivem Tab „AI Suggestions“; einklappbar. */
export function SuggestionsReviewHost() {
  const t = useT();
  const on = useAiPreview();
  const active = useTabs((s) => s.active);
  const selection = useDetailSelection((s) => s.selection);
  const [open, setOpen] = useState(true);
  if (!on || active !== SUGGESTIONS_TAB) return null;
  const focus = selection.focus ?? selection.urlIds[0];
  return <aside className="os-ai-sugg" aria-label={t("ai-suggestions-review-region")} data-open={open ? "true" : "false"}>
    <div className="os-ai-sugg-head">
      <h2>{t("ai-suggestions-review-title")}</h2>
      <button type="button" className="os-btn" aria-expanded={open} onClick={() => setOpen(!open)}>
        {open ? t("ai-suggestions-collapse") : t("ai-suggestions-expand")}
      </button>
    </div>
    {open && <ReviewPanel urlId={focus} selected={selection.urlIds} />}
  </aside>;
}

let mounted: { root: Root; el: HTMLElement; users: number } | undefined;

/** Hängt den Bereich einmal je Fenster ein (aus dem Tab-Abgleich der Tab-Leiste aufgerufen). */
export function useSuggestionsReviewHost(): void {
  useEffect(() => {
    if (!mounted) {
      const el = document.createElement("div");
      el.dataset.aiSuggestionsHost = "true";
      document.body.appendChild(el);
      const root = createRoot(el);
      root.render(<SuggestionsReviewHost />);
      mounted = { root, el, users: 0 };
    }
    mounted.users++;
    return () => {
      const current = mounted;
      if (!current) return;
      current.users--;
      if (current.users > 0) return;
      mounted = undefined;
      // Nicht während eines laufenden Renderns der anderen Wurzel aushängen.
      queueMicrotask(() => { current.root.unmount(); current.el.remove(); });
    };
  }, []);
}
