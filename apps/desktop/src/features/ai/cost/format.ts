// Beträge der KI-Vorschau (UI-013 [M4], führend für 08 und 09): Mikro-USD aus dem `BudgetLedger`, angezeigt mit 4
// Nachkommastellen und dem Dezimaltrenner der UI-Sprache (UI-121), z. B. `Kosten: 3,2100 / 5,0000 USD` bzw.
// `Cost: 3.2100 / 5.0000 USD`. Exporte, Logs und `--json-progress` schreiben immer mit Punkt (Kern).
import type { TFn } from "../../../i18n";
import { getFormatters } from "../../../i18n";
import type { AiCostSnapshot } from "../../../ipc/gen/AiCostSnapshot";

/** Mikro-USD → Betrag mit 4 Nachkommastellen in der UI-Sprache (ohne Währung). */
export function usd(micro: number, lang: string): string {
  return getFormatters(lang).decimal(micro / 1_000_000, 4);
}

/** USD-Eingabe der UI-Sprache (`5,00` bzw. `5.00`) → Mikro-USD; `null` bei ungültiger Eingabe. */
export function parseUsd(text: string, lang: string): number | null {
  const sep = usd(1.5e6, lang).includes(",") ? "," : ".";
  let t = text.trim().replace(/\s| | /g, "");
  if (t === "") return null;
  if (sep === ",") t = t.replace(/\./g, "").replace(",", ".");
  else t = t.replace(/,/g, "");
  if (!/^\d+(\.\d{0,6})?$/.test(t)) return null;
  const [whole, frac = ""] = t.split(".");
  return Number(whole) * 1_000_000 + Number(frac.padEnd(6, "0"));
}

/** Warnschwelle erreicht: Verbrauch (inkl. offener Reservierungen) ≥ `percent` % des Crawl-Budgets (AI-013). */
export function warningReached(s: AiCostSnapshot, percent: number): boolean {
  const budget = s.budgetMicroUsd;
  if (budget === null || budget === undefined || budget === 0) return false;
  return s.spentMicroUsd * 100 >= budget * percent;
}

/** Text des Statusleistenfelds. */
export function fieldText(t: TFn, s: AiCostSnapshot, lang: string): string {
  const spent = usd(s.spentMicroUsd, lang);
  if (s.budgetMicroUsd === null || s.budgetMicroUsd === undefined) return t("ai-cost-field-unlimited", { spent });
  return t("ai-cost-field", { spent, budget: usd(s.budgetMicroUsd, lang) });
}

/** Zeilen des Tooltips (Aufteilung KI / DataForSEO / offene Reservierungen, gleiches Zahlenformat). */
export function tooltipLines(t: TFn, s: AiCostSnapshot, lang: string, warning: boolean, percent: number): string[] {
  const lines = [
    t("ai-cost-tip-ai", { amount: usd(s.aiMicroUsd, lang) }),
    // Produktname in beiden Sprachen gleich (UI-013: Zeile `DataForSEO: …`).
    `DataForSEO: ${usd(s.dataForSeoMicroUsd, lang)} USD`,
    t("ai-cost-tip-reserved", { amount: usd(s.reservedMicroUsd, lang) }),
  ];
  if (warning) lines.push(t("ai-cost-tip-warning", { percent }));
  return lines;
}
