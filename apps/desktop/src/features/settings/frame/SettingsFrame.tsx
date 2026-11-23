import { useEffect, useMemo, useState } from "react";
import { useT } from "../../../i18n";
import { EmptyState } from "../../../ui/primitives/EmptyState";
import { Tabs } from "../../../ui/primitives/Tabs";
import { RegionBoundary } from "../../../app/ErrorBoundary";
import { resolveSections, sectionExtras, sectionOfAnchor, type ResolvedSection } from "./registry";
import { useAiPreview } from "../../ai/preview";
import "./settings.css";

/** Setzt den Fokus auf das Steuerelement mit `data-setting-anchor` (UI-016: Aktionsschaltfläche öffnet die Einstellung). */
export function focusSettingAnchor(anchor: string, doc: Document = document): boolean {
  const host = [...doc.querySelectorAll<HTMLElement>("[data-setting-anchor]")].find((e) => e.dataset.settingAnchor === anchor);
  if (!host) return false;
  const focusable = host.matches("input,button,select,textarea,[tabindex]") ? host : host.querySelector<HTMLElement>("input,button,select,textarea,[tabindex]");
  (focusable ?? host).focus();
  return true;
}

function Placeholder({ section, title }: { section: ResolvedSection; title: string }) {
  const t = useT();
  return (
    <div className="os-settings-placeholder" data-settings-placeholder={section.id}>
      <h2 className="os-settings-heading">{title}</h2>
      <EmptyState state={{ code: "NOT_APPLICABLE" }} />
      <p className="os-hint">{t("settings-placeholder")}</p>
    </div>
  );
}

export interface SettingsFrameProps {
  /** Anker `abschnitt` oder `abschnitt.steuerelement` (aus `settings.html#…` bzw. Fenster-Ereignis). */
  anchor?: string;
  sections?: readonly ResolvedSection[];
}

/** Rahmen des Einstellungsfensters: Seitennavigation links, Abschnitt rechts; Änderungen wirken sofort. */
export function SettingsFrame({ anchor, sections }: SettingsFrameProps) {
  const t = useT();
  // M4-F02: the section „KI“ follows the switch „KI (Vorschau)“ without reopening the window.
  const aiPreview = useAiPreview();
  const all = useMemo(() => sections ?? resolveSections(undefined, aiPreview), [sections, aiPreview]);
  const first = all[0]?.id ?? "general";
  const wanted = anchor ? sectionOfAnchor(anchor) : first;
  const [selected, setSelected] = useState(all.some((s) => s.id === wanted) ? wanted : first);
  useEffect(() => {
    if (!all.some((s) => s.id === selected)) setSelected(first);
  }, [all, selected, first]);

  useEffect(() => {
    if (anchor && all.some((s) => s.id === sectionOfAnchor(anchor))) setSelected(sectionOfAnchor(anchor));
  }, [anchor, all]);

  useEffect(() => {
    if (!anchor || !anchor.includes(".")) return;
    const id = requestAnimationFrame(() => focusSettingAnchor(anchor));
    return () => cancelAnimationFrame(id);
  }, [anchor, selected]);

  return (
    <div className="os-settings">
      <Tabs
        label={t("settings-nav")}
        orientation="vertical"
        selected={selected}
        onSelect={setSelected}
        className="os-settings-tabs"
        tabs={all.map((s) => ({
          id: s.id,
          label: t(s.titleKey),
          content: (
            <RegionBoundary name={`settings-${s.id}`}>
              {s.Component ? (
                <section className="os-settings-section" aria-label={t(s.titleKey)} data-settings-section={s.id}>
                  <h2 className="os-settings-heading">{t(s.titleKey)}</h2>
                  <s.Component />
                  {sectionExtras(s.id).map((Extra, i) => <Extra key={i} />)}
                </section>
              ) : (
                <Placeholder section={s} title={t(s.titleKey)} />
              )}
            </RegionBoundary>
          ),
        }))}
      />
    </div>
  );
}
