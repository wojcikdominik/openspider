// Unterbereiche je Integration (INTEG-001): Konto/Verbindung, Metriken (Linkmetrik-Ebenen, INTEG-150), Erweitert.
import { useEffect, useRef, useState } from "react";
import { i18n, currentLanguage, useT } from "../../i18n";
import type { ApiSourceInfo } from "../../ipc/gen/ApiSourceInfo";
import type { JsonValue } from "../../ipc/gen/serde_json/JsonValue";
import { Banner, Button, Checkbox, RadioGroup, Switch, Tabs, TextField, type TabSpec } from "../../ui/primitives";
import { Ga4Selection, GscSelection, useCatalogList } from "./PropertySelection";
import { ga4StatusParts, gscSiteName, usePropertyCatalog } from "./properties";
import { bool, getIn, limitError, str, strings, type Values } from "./settings";
import {
  CONFIG_LEVELS,
  LEVEL_NAME,
  LEVELS,
  LINK_METRICS,
  PROVIDER_LEVELS,
  isLinkMetricKind,
  levelSelectable,
  linkMetricColumns,
  type DialogKind,
  type LinkMetricKind,
} from "./sources";
import { allowedActions, maskedKey, psiDenialReason, statusText } from "./status";
import type { ApiSources } from "./useApiSources";

type Change = (path: readonly string[], value: JsonValue) => void;
const has = (key: string) => i18n.has(currentLanguage(), key);

export interface SourcePanelProps {
  kind: DialogKind;
  info: ApiSourceInfo | undefined;
  conn: ApiSources;
  /** Entwurf der Einstellungen; `null`, solange `api.getSettings` lädt. */
  values: Values | null;
  onChange: Change;
}

export function SourcePanel(props: SourcePanelProps) {
  const t = useT();
  const [tab, setTab] = useState("account");
  useEffect(() => setTab("account"), [props.kind]);
  const tabs: TabSpec[] = [{ id: "account", label: t("integ-tab-account"), content: <AccountTab {...props} /> }];
  if (isLinkMetricKind(props.kind)) tabs.push({ id: "metrics", label: t("integ-tab-metrics"), content: props.values ? <LinkMetricsTab kind={props.kind} values={props.values} onChange={props.onChange} /> : null });
  tabs.push({ id: "advanced", label: t("integ-tab-advanced"), content: props.values ? <AdvancedTab kind={props.kind} values={props.values} onChange={props.onChange} /> : null });
  return <Tabs tabs={tabs} selected={tab} onSelect={setTab} label={t("integ-source-sections", { source: t(`api-${props.kind}`) })} className="os-integ-sections" />;
}

function AccountTab({ kind, info, conn, values, onChange }: SourcePanelProps) {
  const t = useT();
  const state = conn.stateOf(kind);
  const error = conn.errorOf(kind);
  const actions = allowedActions(state);
  const [key, setKey] = useState("");
  const [remember, setRemember] = useState(true);
  const [clientSecret, setClientSecret] = useState("");
  const accounts = info?.accounts ?? [];
  const [accountId, setAccountId] = useState<string | null>(null);
  useEffect(() => {
    setAccountId((cur) => (cur && accounts.some((a) => a.accountId === cur) ? cur : (accounts.find((a) => a.account === info?.account)?.accountId ?? accounts[0]?.accountId ?? null)));
  }, [accounts, info?.account]);
  const catalog = usePropertyCatalog((s) => s.catalog);
  const connected = state === "connected";
  const googleAccount = accounts.find((a) => a.account === info?.account)?.accountId ?? accountId;
  const ga4 = useCatalogList(kind === "ga4" ? catalog.ga4 : undefined, connected, googleAccount);
  const gsc = useCatalogList(kind === "gsc" ? catalog.gsc : undefined, connected, googleAccount);
  const psiLocal = kind === "psi" && values !== null && bool(values.local);
  // INTEG-132: Pfad als Text halten, damit Leerzeichen beim Tippen (z. B. „Program Files“) erhalten bleiben.
  const [lighthousePath, setLighthousePath] = useState(() => (values ? (str(values.lighthouse_path) ?? "") : ""));
  useEffect(() => {
    if (values) setLighthousePath((cur) => (cur.trim() === (str(values.lighthouse_path) ?? "") ? cur : (str(values.lighthouse_path) ?? "")));
  }, [values]);
  const oauth = info?.auth === "oauth";
  const keyNeeded = info?.auth === "apiKey" && !psiLocal;
  const saved = accounts.find((a) => a.accountId === accountId);

  const parts =
    kind === "ga4" && values ? ga4StatusParts(info?.account, ga4.items, str(values.property), str(values.stream))
    : kind === "gsc" && values && str(values.site) ? [info?.account, gscSiteName(str(values.site) as string)]
    : [info?.account];
  const status = statusText(t, kind, { state, parts, error }, has);
  const denial = kind === "psi" && state === "error" ? psiDenialReason(error) : null;
  const busy = state === "connecting";
  // Die gedrückte Schaltfläche verschwindet mit dem Zustandswechsel (und WebKit fokussiert Schaltflächen beim Klick
  // nicht): der Fokus bleibt auf der Statuszeile im Dialog, damit Escape und Tab weiter funktionieren.
  const statusRef = useRef<HTMLParagraphElement>(null);
  const keepFocus = () => statusRef.current?.focus();

  const connect = (withAccount: string | null) => {
    void conn.connect({
      kind,
      apiKey: keyNeeded && !withAccount ? key.trim() : null,
      remember,
      accountId: withAccount,
      clientSecret: oauth && clientSecret ? clientSecret : null,
      ...(kind === "psi" && values !== null ? { psiLocal, lighthousePath: psiLocal ? lighthousePath.trim() : null } : {}),
    });
    setKey("");
    keepFocus();
  };

  return (
    <div className="os-integ-account">
      <p ref={statusRef} tabIndex={-1} className="os-integ-status" data-state={state} data-testid={`integ-status-${kind}`} role="status">
        {status}
      </p>
      {denial && (
        <div className="os-integ-checklist">
          <p>{t("integ-psi-checklist")}</p>
          <ul>
            <li>{t("integ-psi-check-enabled")}</li>
            <li>{t("integ-psi-check-new")}</li>
            <li>{t("integ-psi-check-restricted")}</li>
          </ul>
        </div>
      )}
      {kind === "psi" && values && (
        <RadioGroup
          label={t("integ-psi-source")}
          value={psiLocal ? "local" : "remote"}
          orientation="horizontal"
          onChange={(v) => onChange(["local"], v === "local")}
          options={[
            { value: "remote", label: t("integ-psi-remote") },
            { value: "local", label: t("integ-psi-local") },
          ]}
        />
      )}
      {kind === "psi" && <p className="os-hint">{t(psiLocal ? "integ-psi-local-hint" : "integ-psi-remote-hint")}</p>}
      {psiLocal && (
        <TextField
          label={t("integ-psi-lighthouse-path")}
          value={lighthousePath}
          description={t("integ-psi-lighthouse-path-hint")}
          isDisabled={busy}
          onChange={(v) => {
            setLighthousePath(v);
            onChange(["lighthouse_path"], v.trim() === "" ? null : v.trim());
          }}
        />
      )}
      {accounts.length > 0 && (oauth || keyNeeded) && (
        <RadioGroup
          label={t(oauth ? "integ-google-accounts" : "integ-saved-keys")}
          value={accountId ?? ""}
          onChange={setAccountId}
          options={accounts.map((a) => ({ value: a.accountId, label: a.keySuffix ? `${a.account} ${maskedKey(a.keySuffix)}` : a.account }))}
        />
      )}
      {keyNeeded && actions.connect && (
        <TextField label={t("integ-api-key")} type="password" value={key} onChange={setKey} />
      )}
      {(keyNeeded || oauth) && actions.connect && (
        <Checkbox isSelected={remember} onChange={setRemember}>
          {t("integ-remember")}
        </Checkbox>
      )}
      <div className="os-integ-actions">
        {actions.connect && oauth && (
          <>
            {saved && <Button variant="primary" isDisabled={busy} onPress={() => connect(saved.accountId)}>{t("integ-connect-account", { account: saved.account })}</Button>}
            <Button variant={saved ? "secondary" : "primary"} isDisabled={busy} onPress={() => connect(null)}>{t(kind === "ga4" || kind === "gsc" ? "integ-connect-google" : "integ-connect-oauth", { source: t(`api-${kind}`) })}</Button>
          </>
        )}
        {actions.connect && !oauth && (
          <>
            {keyNeeded && saved && <Button isDisabled={busy} onPress={() => connect(saved.accountId)}>{t("integ-connect-saved")}</Button>}
            <Button variant="primary" isDisabled={busy || (keyNeeded && key.trim() === "")} onPress={() => connect(null)}>{t("integ-connect")}</Button>
          </>
        )}
        {busy && <span className="os-hint">{t(oauth ? "integ-connecting-browser" : "integ-connecting")}</span>}
        {actions.disconnect && <Button onPress={() => { keepFocus(); void conn.disconnect(kind); }}>{t("integ-disconnect")}</Button>}
        {saved && !busy && (oauth || keyNeeded) && (
          <Button variant="danger" onPress={() => { keepFocus(); void conn.removeAccount(kind, saved.accountId); }}>{t("integ-remove-account")}</Button>
        )}
      </div>
      {oauth && actions.connect && (
        <TextField label={t("integ-client-secret")} type="password" value={clientSecret} onChange={setClientSecret} description={t("integ-client-secret-hint")} />
      )}
      {connected && values && kind === "ga4" && <Ga4Selection values={values} onChange={onChange} catalog={ga4} />}
      {connected && values && kind === "gsc" && <GscSelection values={values} onChange={onChange} catalog={gsc} />}
      {values && (
        <Switch isSelected={bool(values.enabled)} onChange={(v) => onChange(["enabled"], v)}>
          {t("integ-enabled")}
        </Switch>
      )}
    </div>
  );
}

function LinkMetricsTab({ kind, values, onChange }: { kind: LinkMetricKind; values: Values; onChange: Change }) {
  const t = useT();
  const levels = strings(values.levels);
  const metrics = strings(values.metrics);
  const toggle = (list: string[], id: string, on: boolean) => (on ? [...list.filter((x) => x !== id), id] : list.filter((x) => x !== id));
  const columns = linkMetricColumns(kind, metrics, levels);
  return (
    <div className="os-integ-metrics">
      <fieldset className="os-integ-group">
        <legend>{t("integ-levels")}</legend>
        {LEVELS.map((level) => {
          const offered = PROVIDER_LEVELS[kind].includes(level);
          const reason = !offered ? t("integ-level-not-offered") : !CONFIG_LEVELS.includes(level) ? t("integ-level-not-available") : null;
          return (
            <span key={level} title={reason ?? undefined} className="os-integ-level">
              <Checkbox isSelected={levelSelectable(kind, level) && levels.includes(level)} isDisabled={!levelSelectable(kind, level)} onChange={(on) => onChange(["levels"], CONFIG_LEVELS.filter((l) => toggle(levels, level, on).includes(l)))}>
                {LEVEL_NAME[level]}
              </Checkbox>
              {reason && <span className="os-hint">{reason}</span>}
            </span>
          );
        })}
      </fieldset>
      <fieldset className="os-integ-group">
        <legend>{t("integ-metrics")}</legend>
        {LINK_METRICS[kind].map((def) => (
          <Checkbox key={def.id} isSelected={metrics.includes(def.id)} onChange={(on) => onChange(["metrics"], LINK_METRICS[kind].map((d) => d.id).filter((id) => toggle(metrics, def.id, on).includes(id)))}>
            {def.name}
          </Checkbox>
        ))}
      </fieldset>
      <section className="os-integ-columns" aria-label={t("integ-columns")}>
        <p>{t("integ-columns-count", { count: columns.length })}</p>
        <ul>
          {columns.map((c) => (
            <li key={c}>{c}</li>
          ))}
        </ul>
      </section>
    </div>
  );
}

function AdvancedTab({ kind, values, onChange }: { kind: DialogKind; values: Values; onChange: Change }) {
  const t = useT();
  const num = (v: JsonValue | undefined, fallback: number) => (typeof v === "number" ? String(v) : String(fallback));
  const [concurrency, setConcurrency] = useState(num(getIn(values, ["limits", "concurrency"]), 5));
  const [retries, setRetries] = useState(num(getIn(values, ["limits", "retries"]), 5));
  const google = kind === "ga4" || kind === "gsc";
  return (
    <div className="os-integ-advanced">
      <TextField
        label={t("integ-concurrency")}
        type="number"
        inputMode="numeric"
        value={concurrency}
        description={t("integ-concurrency-hint")}
        error={limitError("concurrency", concurrency) ? t("integ-range", { min: 1, max: 50 }) : null}
        onChange={(v) => {
          setConcurrency(v);
          if (!limitError("concurrency", v)) onChange(["limits", "concurrency"], Number(v));
        }}
      />
      <TextField
        label={t("integ-retries")}
        type="number"
        inputMode="numeric"
        value={retries}
        error={limitError("retries", retries) ? t("integ-range", { min: 0, max: 20 }) : null}
        onChange={(v) => {
          setRetries(v);
          if (!limitError("retries", v)) onChange(["limits", "retries"], Number(v));
        }}
      />
      {google && (
        <TextField
          label={t("integ-client-id")}
          value={str(values.oauth_client_id) ?? ""}
          description={t("integ-client-id-hint")}
          onChange={(v) => onChange(["oauth_client_id"], v.trim() === "" ? null : v.trim())}
        />
      )}
      {google && <Banner kind="info">{t("integ-client-id-note")}</Banner>}
    </div>
  );
}
