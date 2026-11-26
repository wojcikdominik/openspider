// Richtlinie (AI-019: „Keine Einschränkung“ / „EU-Verarbeitung erzwingen“ / „Nur lokal“, Standard keine Einschränkung) und
// Crawl-Budget (AI-013, Standard 5,00 USD nach E-5 a; „Unbegrenzt“ nur über die ausdrückliche Checkbox mit Warnhinweis;
// Teilbudget „KI gesamt“; Warnschwelle 80 %) im Konfigurationsdialog, Seite „KI“ (Entwurf `ai.policy`, `ai.budget`).
import { useEffect, useState } from "react";
import { useFormat, useT } from "../../../i18n";
import { Checkbox, RadioGroup, TextField } from "../../../ui/primitives";
import { useConfig } from "../../config/frame/context";
import { parseUsd, usd } from "../cost/format";
import type { BudgetLimits, Policy } from "./api";

export const POLICIES: readonly Policy[] = ["unrestricted", "eu-only", "local-only"];

export function PolicySetting() {
  const t = useT();
  const { draft } = useConfig();
  const value = (draft.get<string>("ai.policy") ?? "unrestricted") as Policy;
  return (
    <section aria-labelledby="ai-policy-heading" data-ai-policy-setting data-setting-anchor="ai.policy">
      <h4 id="ai-policy-heading" className="os-settings-heading">{t("ai-policy-title")}</h4>
      <RadioGroup
        label={t("ai-policy-label")}
        value={value}
        onChange={(v) => draft.set("ai.policy", v)}
        options={POLICIES.map((p) => ({ value: p, label: t(`ai-policy-${p}`) }))}
      />
      <p className="os-hint" data-ai-policy-hint={value}>{t(`ai-policy-${value}-hint`)}</p>
      <p className="os-hint os-ai-warning" role="note" data-ai-policy-not-enforced>{t("ai-policy-not-enforced")}</p>
    </section>
  );
}

const DEFAULT_BUDGET: BudgetLimits = { crawl: 5_000_000, unlimited: false, areas: {}, tasks: {}, warning_percent: 80 };

function UsdField({ label, micro, onChange, allowEmpty, hint }: { label: string; micro: number | null; onChange: (v: number | null) => void; allowEmpty: boolean; hint?: string }) {
  const t = useT();
  const fmt = useFormat();
  const shown = micro === null ? "" : usd(micro, fmt.lang);
  const [text, setText] = useState(shown);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => setText(shown), [shown]);
  return (
    <TextField
      label={label}
      value={text}
      inputMode="decimal"
      description={hint}
      error={error}
      onChange={(v) => {
        setText(v);
        if (v.trim() === "" && allowEmpty) {
          setError(null);
          onChange(null);
          return;
        }
        const parsed = parseUsd(v, fmt.lang);
        if (parsed === null) setError(t("ai-settings-usd-invalid"));
        else {
          setError(null);
          onChange(parsed);
        }
      }}
    />
  );
}

export function BudgetSetting() {
  const t = useT();
  const { draft } = useConfig();
  const budget = { ...DEFAULT_BUDGET, ...(draft.get<BudgetLimits>("ai.budget") ?? {}) };
  const set = (change: Partial<BudgetLimits>) => draft.set("ai.budget", { ...budget, ...change });
  const aiTotal = budget.areas.ai ?? null;
  const [percentText, setPercentText] = useState(String(budget.warning_percent));
  const [percentError, setPercentError] = useState<string | null>(null);
  return (
    <section aria-labelledby="ai-budget-heading" data-ai-budget-setting data-setting-anchor="ai.budget">
      <h4 id="ai-budget-heading" className="os-settings-heading">{t("ai-budget-title")}</h4>
      <UsdField label={t("ai-budget-crawl")} micro={budget.crawl} allowEmpty={false} hint={t("ai-budget-crawl-hint")} onChange={(v) => v !== null && set({ crawl: v })} />
      <Checkbox isSelected={budget.unlimited} onChange={(v) => set({ unlimited: v })}>{t("ai-budget-unlimited")}</Checkbox>
      {budget.unlimited && <p className="os-hint os-ai-warning" role="note" data-ai-unlimited-warning>{t("ai-budget-unlimited-warning")}</p>}
      <UsdField
        label={t("ai-budget-ai-total")}
        micro={aiTotal}
        allowEmpty
        hint={t("ai-budget-ai-total-hint")}
        onChange={(v) => {
          const areas = { ...budget.areas };
          if (v === null) delete areas.ai;
          else areas.ai = v;
          set({ areas });
        }}
      />
      <TextField
        label={t("ai-budget-warning")}
        value={percentText}
        inputMode="numeric"
        error={percentError}
        onChange={(v) => {
          setPercentText(v);
          const n = Number(v);
          if (/^\d+$/.test(v.trim()) && n >= 0 && n <= 100) {
            setPercentError(null);
            set({ warning_percent: n });
          } else setPercentError(t("ai-budget-warning-invalid"));
        }}
      />
    </section>
  );
}
