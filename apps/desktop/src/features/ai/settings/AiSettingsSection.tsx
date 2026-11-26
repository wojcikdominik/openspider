// Einstellungsabschnitt „KI“ (App-Einstellungen, nur mit Schalter „KI (Vorschau)“): Bestätigungsschwelle des Kostenvoranschlags
// (AI-012), EU-Endpunkte je Anbieter (AI-019), Schwärzung (AI-024), Reiter-Inhalt „Erweitert“ mit den Vorgaben je KI-Anbieter
// (INTEG-005 [M4]: Ollama Standard 1) und die Übersicht „KI-Datenfluss“ (AI-018). Die Konten (Schlüssel, Verbindungstest) folgen
// mit M4-P01 in `./accounts`. Modellwahl je Aufgabe, Richtlinie und Crawl-Budget gehören zum Konfigurationsprofil (Seite „KI“).
import { useCallback, useEffect, useState } from "react";
import { useFormat, useT } from "../../../i18n";
import { toCmdError } from "../../../ipc/errors";
import type { ProviderEndpointDto } from "../../../ipc/gen/ProviderEndpointDto";
import { usePrefs } from "../../../state/prefs";
import { Button, Checkbox, Switch, Tabs, TextField } from "../../../ui/primitives";
import { EmptyState } from "../../../ui/primitives/EmptyState";
import { DataFlowOverview } from "../consent/Overview";
import { parseUsd, usd } from "../cost/format";
import { aiApi, providerName, type AdvancedRow, type SettingsView } from "./api";

function endpointOf(view: SettingsView, provider: string): ProviderEndpointDto {
  const cur = usePrefs.getState().dto?.aiApp?.providerEndpoints?.[provider];
  const adv = view.advanced.find((a) => a.provider === provider);
  const eu = view.eu.find((e) => e.provider === provider);
  return {
    timeoutSeconds: cur?.timeoutSeconds ?? adv?.timeout ?? 120,
    concurrentRequests: cur?.concurrentRequests ?? adv?.concurrent ?? 4,
    baseUrl: cur?.baseUrl ?? null,
    useEuEndpoint: cur?.useEuEndpoint ?? eu?.useEuEndpoint ?? false,
    euAccountConfirmed: cur?.euAccountConfirmed ?? eu?.euAccountConfirmed ?? false,
  };
}

async function patchEndpoint(view: SettingsView, provider: string, change: Partial<ProviderEndpointDto>) {
  await usePrefs.getState().patch({ aiApp: { providerEndpoints: { [provider]: { ...endpointOf(view, provider), ...change } } } });
}

function Threshold({ view, reload }: { view: SettingsView; reload: () => void }) {
  const t = useT();
  const fmt = useFormat();
  const [text, setText] = useState(usd(view.confirmationThresholdMicroUsd, fmt.lang));
  const [error, setError] = useState<string | null>(null);
  const save = async () => {
    const micro = parseUsd(text, fmt.lang);
    if (micro === null) return setError(t("ai-settings-usd-invalid"));
    setError(null);
    await usePrefs.getState().patch({ aiApp: { confirmationThreshold: micro } });
    reload();
  };
  return (
    <div className="os-ai-row" data-setting-anchor="ai_app.confirmation_threshold">
      <TextField label={t("ai-settings-threshold")} value={text} onChange={setText} error={error} inputMode="decimal" description={t("ai-settings-threshold-hint")} />
      <Button onPress={() => void save()}>{t("ai-settings-apply")}</Button>
    </div>
  );
}

function EuEndpoints({ view, reload }: { view: SettingsView; reload: () => void }) {
  const t = useT();
  return (
    <div data-ai-eu>
      <h4 className="os-settings-heading">{t("ai-settings-eu-title")}</h4>
      <p className="os-hint">{t("ai-settings-eu-hint")}</p>
      <p className="os-hint os-ai-warning" role="note">{t("ai-policy-not-enforced")}</p>
      {view.eu.map((e) => (
        <fieldset key={e.provider} className="os-ai-fieldset" data-provider={e.provider}>
          <legend>{providerName(e.provider)}</legend>
          <Switch isSelected={e.useEuEndpoint} onChange={(v) => void patchEndpoint(view, e.provider, { useEuEndpoint: v }).then(reload)}>
            {t("ai-settings-eu-use", { url: e.euBaseUrl })}
          </Switch>
          <Checkbox isSelected={e.euAccountConfirmed} onChange={(v) => void patchEndpoint(view, e.provider, { euAccountConfirmed: v }).then(reload)}>
            {t("ai-settings-eu-confirmed")}
          </Checkbox>
          {!e.allowedUnderEu && <p className="os-hint">{t("ai-settings-eu-not-allowed")}</p>}
        </fieldset>
      ))}
      <p className="os-hint">{t("ai-settings-eu-anthropic")}</p>
    </div>
  );
}

function Redaction({ view, reload }: { view: SettingsView; reload: () => void }) {
  const t = useT();
  const [text, setText] = useState(view.redactionPatterns.join("\n"));
  const [error, setError] = useState<string | null>(null);
  const [saved, setSaved] = useState(false);
  const save = async () => {
    try {
      await aiApi.savePatterns(text);
      setError(null);
      setSaved(true);
      reload();
    } catch (e) {
      const args = toCmdError(e).args as { line?: number; position?: number };
      setSaved(false);
      setError(t("ai-settings-pattern-invalid", { line: args.line ?? 0, position: args.position ?? 0 }));
    }
  };
  return (
    <div data-ai-redaction-settings>
      <h4 className="os-settings-heading">{t("ai-settings-redaction-title")}</h4>
      <Switch isSelected={view.redactPersonalData} anchor="ai_app.redact_personal_data" onChange={(v) => void usePrefs.getState().patch({ aiApp: { redactPersonalData: v } }).then(reload)}>
        {t("ai-settings-redaction-switch")}
      </Switch>
      <label className="os-label" htmlFor="ai-redaction-patterns">{t("ai-settings-patterns")}</label>
      <textarea id="ai-redaction-patterns" className="os-input os-ai-textarea" rows={4} value={text} onChange={(e) => { setText(e.target.value); setSaved(false); }} aria-invalid={error ? true : undefined} aria-describedby={error ? "ai-redaction-patterns-error" : undefined} />
      {error && <p id="ai-redaction-patterns-error" className="os-field-error" role="alert">{error}</p>}
      <Button onPress={() => void save()}>{t("ai-settings-patterns-save")}</Button>
      {saved && <span role="status">{t("ai-settings-saved")}</span>}
    </div>
  );
}

function AdvancedProvider({ view, row, reload }: { view: SettingsView; row: AdvancedRow; reload: () => void }) {
  const t = useT();
  const [text, setText] = useState(String(row.concurrent));
  const [error, setError] = useState<string | null>(null);
  const save = async () => {
    const n = Number(text);
    if (!/^\d+$/.test(text.trim()) || n < row.concurrentMin || n > row.concurrentMax) {
      setError(t("ai-settings-concurrency-invalid", { min: row.concurrentMin, max: row.concurrentMax }));
      return;
    }
    setError(null);
    await patchEndpoint(view, row.provider, { concurrentRequests: n });
    reload();
  };
  return (
    <div className="os-ai-row" data-provider={row.provider} data-ai-advanced-row>
      <TextField
        label={t("ai-settings-concurrency", { provider: providerName(row.provider) })}
        value={text}
        onChange={setText}
        inputMode="numeric"
        error={error}
        description={t("ai-settings-concurrency-hint", { standard: row.concurrentDefault, min: row.concurrentMin, max: row.concurrentMax })}
      />
      <Button onPress={() => void save()}>{t("ai-settings-apply")}</Button>
    </div>
  );
}

function Advanced({ view, reload }: { view: SettingsView; reload: () => void }) {
  const t = useT();
  return (
    <div data-ai-advanced>
      <p className="os-hint">{t("ai-settings-advanced-hint")}</p>
      {view.advanced.map((row) => <AdvancedProvider key={row.provider} view={view} row={row} reload={reload} />)}
    </div>
  );
}

/** Platzhalter der Konten bis M4-P01 (Schlüssel, Verbindungstest; `./accounts`). */
function Accounts() {
  const t = useT();
  return (
    <div data-ai-accounts>
      <EmptyState state={{ code: "NOT_APPLICABLE" }} />
      <p className="os-hint">{t("ai-settings-accounts-later")}</p>
    </div>
  );
}

export function AiSettingsSection() {
  const t = useT();
  const [view, setView] = useState<SettingsView | null>(null);
  const [failed, setFailed] = useState(false);
  const [tab, setTab] = useState("general");
  const reload = useCallback(() => {
    aiApi
      .settings()
      .then((v) => {
        setView(v);
        setFailed(false);
      })
      .catch(() => setFailed(true));
  }, []);
  useEffect(reload, [reload]);

  const general = view ? (
    <>
      <Threshold view={view} reload={reload} />
      <EuEndpoints view={view} reload={reload} />
      <Redaction view={view} reload={reload} />
    </>
  ) : (
    <EmptyState state={{ code: failed ? "NOT_APPLICABLE" : "NO_DATA_YET" }} />
  );
  return (
    <div className="os-ai-settings" data-ai-area="settings">
      <Tabs
        label={t("ai-settings-title")}
        selected={tab}
        onSelect={setTab}
        tabs={[
          { id: "general", label: t("ai-settings-tab-costs"), content: general },
          { id: "dataflow", label: t("ai-settings-tab-dataflow"), content: <DataFlowOverview /> },
          { id: "advanced", label: t("ai-settings-tab-advanced"), content: view ? <Advanced view={view} reload={reload} /> : <EmptyState state={{ code: "NO_DATA_YET" }} /> },
          { id: "accounts", label: t("ai-settings-tab-accounts"), content: <Accounts /> },
        ]}
      />
    </div>
  );
}
