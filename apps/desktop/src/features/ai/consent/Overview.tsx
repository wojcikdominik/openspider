import "./../settings/ai.css";
// Übersicht „KI-Datenfluss“ (AI-018, jederzeit aufrufbar): alle konfigurierten Aufgaben mit `Consent Given At`, Widerruf je
// Anbieter, Export als CSV (Inhalt vom Kern, `DataFlowOverview::to_csv`), Richtlinie (AI-019).
import { useCallback, useEffect, useState } from "react";
import { useT } from "../../../i18n";
import { Button } from "../../../ui/primitives";
import { EmptyState } from "../../../ui/primitives/EmptyState";
import { AI_019_ENABLED, aiApi, providerName, type ConsentView } from "../settings/api";
import { DataFlowTable } from "./DataFlowTable";

export const CSV_FILE_NAME = "ai-data-flow.csv";

export function DataFlowOverview() {
  const t = useT();
  const [view, setView] = useState<ConsentView | null>(null);
  const [failed, setFailed] = useState(false);
  const [copied, setCopied] = useState(false);
  const load = useCallback(() => {
    aiApi
      .consent()
      .then((v) => {
        setView(v);
        setFailed(false);
      })
      .catch(() => setFailed(true));
  }, []);
  useEffect(load, [load]);

  if (failed || !view) {
    return (
      <div data-ai-dataflow-overview>
        <h4 className="os-settings-heading">{t("ai-consent-overview-title")}</h4>
        <EmptyState state={{ code: "NO_DATA_YET" }} />
      </div>
    );
  }
  const revoke = async (provider: string) => {
    setView(await aiApi.revoke(provider));
  };
  const copy = async () => {
    await navigator.clipboard?.writeText(view.csv).catch(() => undefined);
    setCopied(true);
  };
  const href = `data:text/csv;charset=utf-8,${encodeURIComponent(view.csv)}`;
  return (
    <div data-ai-dataflow-overview>
      <h4 className="os-settings-heading">{t("ai-consent-overview-title")}</h4>
      {AI_019_ENABLED && <p data-ai-policy={view.policy}>{t("ai-consent-policy", { policy: t(`ai-policy-${view.policy}`) })}</p>}
      <p data-ai-redaction={view.redaction ? "on" : "off"}>{t(view.redaction ? "ai-consent-redaction-on" : "ai-consent-redaction-off")}</p>
      {view.rows.length === 0 ? (
        <EmptyState state={{ code: "NO_DATA_YET" }} />
      ) : (
        <DataFlowTable columns={view.columns} rows={view.rows} caption={t("ai-consent-overview-caption")} label={t("ai-consent-overview-title")} />
      )}
      <div className="os-ai-actions">
        <Button onPress={() => void copy()} isDisabled={view.rows.length === 0}>{t("ai-consent-copy-csv")}</Button>
        <a className="os-btn os-btn-secondary" href={href} download={CSV_FILE_NAME} data-ai-csv-download>{t("ai-consent-save-csv")}</a>
        {copied && <span role="status">{t("ai-consent-copied")}</span>}
      </div>
      {view.consentedProviders.length > 0 && (
        <div className="os-ai-actions" data-ai-revoke>
          {view.consentedProviders.map((p) => (
            <Button key={p} onPress={() => void revoke(p)}>{t("ai-consent-revoke", { provider: providerName(p) })}</Button>
          ))}
        </div>
      )}
    </div>
  );
}
