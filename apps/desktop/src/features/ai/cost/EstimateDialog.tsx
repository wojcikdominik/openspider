import "./../settings/ai.css";
// Gemeinsamer Dialog „Kostenvoranschlag“ (AI-012, Architekten-Entscheidung 13.10.2026): genau ein Dialog für alle bezahlten
// APIs. M4-U01 liefert den Abschnitt „KI“; M5-U01 meldet den Abschnitt „DataForSEO“ über `registerEstimateSection` an, der Kern
// hängt ihn mit `CostEstimateView::with_section` an. Darunter die gemeinsame Summenzeile sowie Crawl-Budget, Verbrauch und
// Rest aus dem `BudgetLedger` (AI-013). „Starten“ gibt alle Bereiche frei; ohne Klick sendet der Lauf keinen KI-Request
// (Einträge `Pending (confirmation)`, Ausführung D01).
import type { FC } from "react";
import { create } from "zustand";
import { useFormat, useT } from "../../../i18n";
import { Dialog, DialogFooter } from "../../../ui/primitives";
import { aiApi, type CostEstimateView, type EstimateRow, type EstimateSection, type Totals } from "../settings/api";
import type { AiRunArgs } from "../../../ipc/gen/AiRunArgs";
import { usd } from "./format";

/** Kanonische Spalten des Abschnitts „KI“ (AI-012; `Reasoning` zwischen `Model` und `Items`, Nachtrag 24.11.2026). */
export const AI_ESTIMATE_COLUMNS = [
  "Task",
  "Model",
  "Reasoning",
  "Items",
  "Cached",
  "Est. Input Tokens",
  "Est. Output Tokens",
  "Est. Cost (low/expected/high)",
  "Batch Savings",
  "Executable",
] as const;

interface EstimateState {
  view: CostEstimateView | null;
  resolve: ((start: boolean) => void) | null;
}

export const useCostEstimateDialog = create<EstimateState>(() => ({ view: null, resolve: null }));

/** Zeigt den Dialog mit `view`; `true` nach „Starten“, `false` nach „Abbrechen“ bzw. Schließen. */
export function showCostEstimate(view: CostEstimateView): Promise<boolean> {
  useCostEstimateDialog.getState().resolve?.(false);
  return new Promise((resolve) => useCostEstimateDialog.setState({ view, resolve }));
}

/**
 * Bestätigungsregel vor einem KI-Lauf (AI-012): holt den Voranschlag (`ai.costEstimate`); verlangt der Kern eine Bestätigung
 * (Schwelle oder 50 % des Restbudgets), erscheint der Dialog, sonst geht es ohne Dialog weiter.
 */
export async function requestCostConfirmation(args?: AiRunArgs): Promise<boolean> {
  const view = await aiApi.estimate(args);
  if (!view.confirmationRequired) return true;
  return showCostEstimate(view);
}

function finish(start: boolean) {
  const { resolve } = useCostEstimateDialog.getState();
  useCostEstimateDialog.setState({ view: null, resolve: null });
  resolve?.(start);
}

export interface SectionProps {
  section: EstimateSection;
}

const SECTIONS = new Map<string, { titleKey: string; Component: FC<SectionProps> }>();

/** Meldet einen weiteren Abschnitt an (M5-U01: `dataforseo`). Unbekannte Abschnitte zeigt der Dialog nicht. */
export function registerEstimateSection(id: string, titleKey: string, Component: FC<SectionProps>) {
  SECTIONS.set(id, { titleKey, Component });
}

function Money({ micro }: { micro: number | null }) {
  const fmt = useFormat();
  return <>{micro === null ? "–" : usd(micro, fmt.lang)}</>;
}

function TotalsCell({ totals }: { totals: Totals }) {
  return <><Money micro={totals.lowMicroUsd} /> / <Money micro={totals.expectedMicroUsd} /> / <Money micro={totals.highMicroUsd} /></>;
}

function AiSection({ section }: SectionProps) {
  const t = useT();
  const fmt = useFormat();
  const rows = section.rows as EstimateRow[];
  if (rows.length === 0) return <p className="os-hint" data-ai-estimate-empty>{t("ai-estimate-no-tasks")}</p>;
  return (
    <div className="os-ai-scroll" role="region" aria-label={t("ai-estimate-section-ai")} tabIndex={0}>
    <table className="os-ai-table" data-ai-estimate-section="ai">
      <thead>
        <tr>{AI_ESTIMATE_COLUMNS.map((c) => <th key={c} scope="col">{c}</th>)}</tr>
      </thead>
      <tbody>
        {rows.map((r) => (
          <tr key={`${r.task}|${r.model}`} data-task={r.task}>
            <td>{r.task}</td>
            <td>{r.model}</td>
            <td data-col="Reasoning">{r.reasoning ?? ""}</td>
            <td>{fmt.integer(r.items)}</td>
            <td>{fmt.integer(r.cached)}</td>
            <td>{fmt.integer(r.estInputTokens)}</td>
            <td>{fmt.integer(r.estOutputTokens)}</td>
            <td><Money micro={r.lowMicroUsd} /> / <Money micro={r.expectedMicroUsd} /> / <Money micro={r.highMicroUsd} /></td>
            <td><Money micro={r.batchSavingsMicroUsd} /></td>
            <td data-col="Executable">{r.executable ? t("ai-estimate-yes") : t("ai-estimate-no", { reason: r.reason })}</td>
          </tr>
        ))}
      </tbody>
      <tfoot>
        <tr>
          <th scope="row" colSpan={7}>{t("ai-estimate-section-sum")}</th>
          <td><TotalsCell totals={section.totals} /></td>
          <td colSpan={2} />
        </tr>
      </tfoot>
    </table>
    </div>
  );
}

registerEstimateSection("ai", "ai-estimate-section-ai", AiSection);

/** Inhalt des Dialogs (ohne Rahmen, auch für Tests und die Bereichsansicht). */
export function EstimateBody({ view }: { view: CostEstimateView }) {
  const t = useT();
  const fmt = useFormat();
  const b = view.budget;
  return (
    <div className="os-ai-estimate" data-ai-estimate>
      {view.sections.map((s) => {
        const entry = SECTIONS.get(s.id);
        if (!entry) return null;
        return (
          <section key={s.id} aria-labelledby={`ai-estimate-${s.id}`} data-section={s.id}>
            <h4 id={`ai-estimate-${s.id}`} className="os-settings-heading">{t(entry.titleKey)}</h4>
            <entry.Component section={s} />
          </section>
        );
      })}
      <dl className="os-ai-estimate-summary">
        <dt>{t("ai-estimate-total")}</dt>
        <dd data-ai-estimate-total><TotalsCell totals={view.totals} /> USD</dd>
        <dt>{t("ai-estimate-budget")}</dt>
        <dd data-ai-estimate-budget>{b.budgetMicroUsd === null ? t("ai-estimate-unlimited") : `${usd(b.budgetMicroUsd, fmt.lang)} USD`}</dd>
        <dt>{t("ai-estimate-consumed")}</dt>
        <dd data-ai-estimate-consumed>{usd(b.consumedMicroUsd, fmt.lang)} USD</dd>
        <dt>{t("ai-estimate-remaining")}</dt>
        <dd data-ai-estimate-remaining>{b.remainingMicroUsd === null ? t("ai-estimate-unlimited") : `${usd(b.remainingMicroUsd, fmt.lang)} USD`}</dd>
      </dl>
      {view.confirmationRequired && (
        <p className="os-hint" data-ai-estimate-confirm>{t("ai-estimate-confirm-hint", { threshold: usd(view.thresholdMicroUsd, fmt.lang) })}</p>
      )}
      {b.halted && <p className="os-hint" data-ai-estimate-halted>{t("ai-estimate-halted")}</p>}
    </div>
  );
}

/** Rahmen des Dialogs; liegt im Host der KI-Dialoge (`./DialogHost`). */
export function CostEstimateDialog() {
  const t = useT();
  const view = useCostEstimateDialog((s) => s.view);
  return (
    <Dialog
      title={t("ai-estimate-title")}
      isOpen={view !== null}
      onOpenChange={(open) => {
        if (!open) finish(false);
      }}
      size="large"
      footer={<DialogFooter okLabel={t("ai-estimate-start")} cancelLabel={t("ai-estimate-cancel")} onOk={() => finish(true)} onCancel={() => finish(false)} />}
    >
      {view && <EstimateBody view={view} />}
    </Dialog>
  );
}
