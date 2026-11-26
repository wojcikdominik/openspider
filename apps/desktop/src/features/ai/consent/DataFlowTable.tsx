// Datenflusstabelle (AI-018): Spalten in der Reihenfolge des Kerns (`os_ai::policy::COLUMNS`, kanonische Namen wie im
// CSV-Export), `Stored by Provider` nach `Training Use (per provider policy)`.
import type { TFn } from "../../../i18n";
import type { ConsentRowView } from "../settings/api";

/** Beschriftung eines Feldes (Kennungen aus AI-030/AI-032). */
export function fieldLabel(t: TFn, id: string): string {
  return t(`ai-field-${id.replace(/_/g, "-")}`);
}

/** Die Tabelle liegt in einem fokussierbaren, scrollbaren Bereich (breite Tabelle; axe `scrollable-region-focusable`). */
export function DataFlowTable({ columns, rows, caption, label }: { columns: string[]; rows: ConsentRowView[]; caption?: string; label: string }) {
  return (
    <div className="os-ai-scroll" role="region" aria-label={label} tabIndex={0}>
    <table className="os-ai-table" data-ai-dataflow>
      {caption && <caption>{caption}</caption>}
      <thead>
        <tr>{columns.map((c) => <th key={c} scope="col">{c}</th>)}</tr>
      </thead>
      <tbody>
        {rows.map((r) => (
          <tr key={`${r.key.provider}|${r.key.task}|${r.key.fields.join(",")}`} data-task={r.key.task} data-provider={r.key.provider}>
            {r.cells.map((cell, i) => <td key={columns[i] ?? i} data-col={columns[i]}>{cell}</td>)}
          </tr>
        ))}
      </tbody>
    </table>
    </div>
  );
}
