// Generische Feldsteuerelemente der Konfigurationsseiten. Typ, Grenzen und Aufzählungen kommen aus dem Schema des Kerns
// (`config.schema`); ob der Wert gültig ist, entscheidet `config.validate` – die Oberfläche zeigt nur dessen Meldungen je Pfad.
import { useEffect, useId, useState, type ReactNode } from "react";
import { Label, Radio, RadioGroup as RacRadioGroup, TextArea, TextField as RacTextField } from "react-aria-components";
import { useT } from "../../../i18n";
import { Checkbox, Select, Switch, TextField } from "../../../ui/primitives";
import { issueText, useConfig, useConfigValue } from "./context";
import { firstPerPath, lineIndexOf } from "./model";
import type { FieldMeta } from "./schema";
import type { FieldDef } from "./types";

export const slug = (path: string): string => path.replace(/[._[\]]/g, "-");
export const fieldLabelKey = (path: string): string => `config-f-${slug(path)}`;
export const enumLabelKey = (path: string, value: string): string => `config-e-${slug(path)}-${value.replace(/_/g, "-")}`;

/** Zahl aus der Eingabe: ganze Zahl bzw. Dezimalzahl (Komma oder Punkt); `undefined`, wenn keine Zahl. */
export function parseNumber(text: string, integer: boolean, unsigned = false): number | undefined {
  const s = text.trim().replace(",", ".");
  if (s === "") return undefined;
  const sign = unsigned ? "" : "-?";
  if (integer ? !new RegExp(`^${sign}\\d+$`).test(s) : !new RegExp(`^${sign}(\\d+\\.?\\d*|\\.\\d+)$`).test(s)) return undefined;
  const n = Number(s);
  return Number.isFinite(n) ? n : undefined;
}

/** Zeilen einer Listeneingabe: leere Zeilen entfallen, sonst bleibt der Text unverändert. */
export const parseLines = (text: string): string[] => text.split(/\r?\n/).filter((l) => l.trim() !== "");

function useLocalText<T>(value: T, parse: (text: string) => T | undefined, format: (v: T) => string): [string, (s: string) => void] {
  const [text, setText] = useState(() => format(value));
  useEffect(() => {
    const parsed = parse(text);
    if (parsed === undefined || JSON.stringify(parsed) !== JSON.stringify(value)) setText(format(value));
    // eslint-disable-next-line react-hooks/exhaustive-deps -- nur bei externer Änderung des Wertes abgleichen
  }, [JSON.stringify(value)]);
  return [text, setText];
}

/** Rahmen eines Feldes: Anker für Sprungziele, Sperre mit Tooltip, Hinweis und Meldungen des Kerns. */
export function FieldFrame({ path, hint, children, showIssues = true }: { path: string; hint?: string; children: ReactNode; showIssues?: boolean }) {
  const t = useT();
  const { lock, issues } = useConfigValue(path);
  const hintId = useId();
  return (
    <div className="os-cfg-field" data-setting-anchor={path} data-lock={lock ?? undefined} title={lock ? t(`config-lock-${lock}`) : undefined}>
      {children}
      {hint && (
        <p className="os-hint" id={hintId}>
          {hint}
        </p>
      )}
      {showIssues &&
        firstPerPath(issues).map((i, n) => (
          <p key={n} className={i.warning ? "os-cfg-warning" : "os-error"} role="alert" data-issue-path={i.path}>
            {issueText(t, i)}
          </p>
        ))}
    </div>
  );
}

function rangeHint(t: ReturnType<typeof useT>, meta: FieldMeta | null): string | undefined {
  if (!meta) return undefined;
  if (meta.minimum !== undefined && meta.maximum !== undefined && meta.maximum < 1e9) return t("config-range", { min: meta.minimum, max: meta.maximum });
  if (meta.minimum !== undefined) return t("config-range-min", { min: meta.minimum });
  return undefined;
}

export function NumberInput({ def, meta }: { def: FieldDef; meta: FieldMeta }) {
  const t = useT();
  const { value, set, lock, issues } = useConfigValue<number | null>(def.path);
  const integer = meta.type === "integer";
  const unsigned = meta.unsigned === true;
  const [text, setText] = useLocalText<number | null>(value ?? null, (s) => (s.trim() === "" ? undefined : (parseNumber(s, integer, unsigned) ?? undefined)), (v) => (v === null ? "" : String(v)));
  const [localError, setLocalError] = useState<string | null>(null);
  const nullable = meta.nullable;
  const unset = value === null || value === undefined;
  const label = t(def.labelKey ?? fieldLabelKey(def.path));
  const apiError = issues.find((i) => !i.warning);
  const onText = (s: string) => {
    setText(s);
    const n = parseNumber(s, integer, unsigned);
    if (s.trim() === "") {
      setLocalError(nullable ? null : t("config-required"));
      if (nullable) set(null);
      return;
    }
    if (n === undefined) {
      setLocalError(t(integer ? (unsigned ? "config-not-unsigned" : "config-not-integer") : "config-not-number"));
      return;
    }
    if (meta.maximum === undefined && meta.typeMaximum !== undefined && n > meta.typeMaximum) {
      setLocalError(t("config-range", { min: meta.minimum ?? 0, max: meta.typeMaximum }));
      return;
    }
    setLocalError(null);
    set(n);
  };
  const fallback = (): number => {
    const d = meta.default;
    if (typeof d === "number") return d;
    return meta.minimum ?? 0;
  };
  return (
    <div className="os-cfg-number">
      <TextField
        label={label}
        value={text}
        onChange={onText}
        isDisabled={lock !== null || (nullable && unset)}
        inputMode={integer ? "numeric" : "decimal"}
        error={localError ?? (apiError ? issueText(t, apiError) : null)}
        description={def.hintKey ? t(def.hintKey) : rangeHint(t, meta)}
        className="os-cfg-input"
      />
      {def.unitKey && <span className="os-cfg-unit">{t(def.unitKey)}</span>}
      {nullable && (
        <Checkbox
          isSelected={unset}
          isDisabled={lock !== null}
          onChange={(on) => {
            setLocalError(null);
            if (on) set(null);
            else set(fallback());
          }}
        >
          {t(def.nullLabelKey ?? "config-unlimited")}
        </Checkbox>
      )}
    </div>
  );
}

export function TextInput({ def, meta }: { def: FieldDef; meta: FieldMeta }) {
  const t = useT();
  const { value, set, lock, issues } = useConfigValue<string | null>(def.path);
  const apiError = issues.find((i) => !i.warning);
  return (
    <TextField
      label={t(def.labelKey ?? fieldLabelKey(def.path))}
      value={value ?? ""}
      onChange={(s) => set(s === "" && meta.nullable ? null : s)}
      isDisabled={lock !== null}
      error={apiError ? issueText(t, apiError) : null}
      description={def.hintKey ? t(def.hintKey) : undefined}
      className="os-cfg-input os-cfg-wide"
    />
  );
}

/** Mehrzeiliges Feld mit einer Liste je Zeile (Include/Exclude, Parameter, CDNs …); Fehler werden zeilengenau gemeldet. */
export function LinesInput({ def, rows = 6, mono = false }: { def: FieldDef; rows?: number; mono?: boolean }) {
  const t = useT();
  const { value, set, lock, issues } = useConfigValue<string[] | null>(def.path);
  const list = value ?? [];
  const [text, setText] = useLocalText<string[]>(list, (s) => parseLines(s), (v) => v.join("\n"));
  const ownIssues = firstPerPath(issues.filter((i) => !i.warning));
  const lines = ownIssues.map((i) => ({ idx: lineIndexOf(i.path), issue: i })).sort((a, b) => (a.idx ?? -1) - (b.idx ?? -1));
  return (
    <RacTextField className="os-field os-cfg-lines" isDisabled={lock !== null} isInvalid={ownIssues.length > 0} value={text} onChange={(s) => {
      setText(s);
      set(parseLines(s));
    }}>
      <Label className="os-label">{t(def.labelKey ?? fieldLabelKey(def.path))}</Label>
      <TextArea className={`os-input os-cfg-textarea${mono ? " os-mono" : ""}`} rows={rows} spellCheck={false} />
      {def.hintKey && <p className="os-hint">{t(def.hintKey)}</p>}
      {lines.length > 0 && (
        <ul className="os-cfg-line-errors" role="alert">
          {lines.map(({ idx, issue }, n) => {
            const msg = issueText(t, issue);
            return (
              <li key={n} data-issue-path={issue.path} data-line={idx === null ? undefined : idx + 1}>
                {idx === null ? msg : t("config-line-error", { line: idx + 1, text: list[idx] ?? "", reason: msg })}
              </li>
            );
          })}
        </ul>
      )}
    </RacTextField>
  );
}

export function EnumInput({ def, meta, radio }: { def: FieldDef; meta: FieldMeta; radio: boolean }) {
  const t = useT();
  const { value, set, lock } = useConfigValue<string>(def.path);
  const options = (meta.enum ?? []).map((v) => ({ value: v, label: t(enumLabelKey(def.path, v)) }));
  const label = t(def.labelKey ?? fieldLabelKey(def.path));
  if (radio)
    return (
      <RacRadioGroup className="os-radio-group" value={value ?? ""} onChange={set} isDisabled={lock !== null}>
        <Label className="os-label">{label}</Label>
        <div className="os-radio-items">
          {options.map((o) => (
            <Radio key={o.value} value={o.value} className="os-radio">
              <span className="os-radio-dot" aria-hidden />
              <span>{o.label}</span>
            </Radio>
          ))}
        </div>
      </RacRadioGroup>
    );
  return <Select label={label} value={value ?? ""} onChange={set} options={options} isDisabled={lock !== null} />;
}

/** Schlüsselwort oder Ganzzahl im Bereich als Auswahlliste (`auto`, 1–6; CRAWL-151). */
export function ChoiceIntInput({ def, meta }: { def: FieldDef; meta: FieldMeta }) {
  const t = useT();
  const { value, set, lock } = useConfigValue<string | number>(def.path);
  const words = meta.choices ?? [];
  const numbers: number[] = [];
  if (meta.minimum !== undefined && meta.maximum !== undefined) for (let n = meta.minimum; n <= meta.maximum; n++) numbers.push(n);
  const options = [...words.map((w) => ({ value: w, label: t(enumLabelKey(def.path, w)) })), ...numbers.map((n) => ({ value: String(n), label: String(n) }))];
  const current = value === undefined || value === null ? "" : String(value);
  return (
    <Select
      label={t(def.labelKey ?? fieldLabelKey(def.path))}
      value={current}
      onChange={(v) => set(words.includes(v) ? v : Number(v))}
      options={options}
      isDisabled={lock !== null}
    />
  );
}

export function BoolInput({ def }: { def: FieldDef }) {
  const t = useT();
  const { draft } = useConfig();
  const { value, set, lock } = useConfigValue<boolean>(def.path);
  const disabled = lock !== null || (def.enabledWhen ? draft.get(def.enabledWhen.path) !== def.enabledWhen.equals : false);
  return (
    <Switch isSelected={value === true} onChange={set} isDisabled={disabled}>
      {t(def.labelKey ?? fieldLabelKey(def.path))}
    </Switch>
  );
}

interface StoreCrawlValue {
  crawl: boolean;
  store: boolean;
}

/** `spider.*`-Linktypen: abrufen und/oder speichern. */
export function StoreCrawlInput({ def }: { def: FieldDef }) {
  const t = useT();
  const { value, set, lock } = useConfigValue<StoreCrawlValue>(def.path);
  const v = value ?? { crawl: false, store: false };
  return (
    <fieldset className="os-cfg-storecrawl" disabled={lock !== null}>
      <legend className="os-label">{t(def.labelKey ?? fieldLabelKey(def.path))}</legend>
      <Checkbox isSelected={v.crawl} onChange={(crawl) => set({ ...v, crawl })} isDisabled={lock !== null}>
        {t("config-crawl")}
      </Checkbox>
      <Checkbox isSelected={v.store} onChange={(store) => set({ ...v, store })} isDisabled={lock !== null}>
        {t("config-store")}
      </Checkbox>
    </fieldset>
  );
}

/** Wählt das Steuerelement nach `kind` bzw. nach dem Typ im Schema. */
export function ConfigField({ def }: { def: FieldDef }) {
  const { draft } = useConfig();
  const t = useT();
  const meta = draft.meta(def.path);
  const kind = def.kind ?? "auto";
  let control: ReactNode;
  let ownIssues = false;
  if (!meta && draft.status === "ready" && kind === "auto") {
    control = <p className="os-hint">{t("config-field-unknown", { path: def.path })}</p>;
  } else if (kind === "storeCrawl") {
    control = <StoreCrawlInput def={def} />;
  } else if (kind === "lines" || meta?.type === "array") {
    control = <LinesInput def={def} />;
    ownIssues = true;
  } else if (meta?.choices) {
    control = <ChoiceIntInput def={def} meta={meta} />;
  } else if (kind === "enum" || kind === "radio" || meta?.enum) {
    control = meta ? <EnumInput def={def} meta={meta} radio={kind === "radio"} /> : null;
  } else if (kind === "bool" || meta?.type === "boolean") {
    control = <BoolInput def={def} />;
  } else if (meta && (kind === "int" || kind === "number" || meta.type === "integer" || meta.type === "number")) {
    control = <NumberInput def={def} meta={meta} />;
    ownIssues = true;
  } else if (meta) {
    control = <TextInput def={def} meta={meta} />;
    ownIssues = true;
  } else {
    control = null;
  }
  return (
    <FieldFrame path={def.path} showIssues={!ownIssues} hint={kind === "bool" || kind === "enum" || kind === "radio" || kind === "storeCrawl" || meta?.choices ? (def.hintKey ? t(def.hintKey) : undefined) : undefined}>
      {control}
    </FieldFrame>
  );
}

/** Liste von Feldern mit Abschnittsüberschriften. */
export function FieldList({ fields }: { fields: readonly FieldDef[] }) {
  const t = useT();
  return (
    <div className="os-cfg-fields">
      {fields.filter((def) => !def.hidden).map((def) => (
        <div key={def.path} className="os-cfg-row">
          {def.sectionKey && <h3 className="os-cfg-section">{t(def.sectionKey)}</h3>}
          <ConfigField def={def} />
        </div>
      ))}
    </div>
  );
}

export function PageHeading({ titleKey, descriptionKey }: { titleKey: string; descriptionKey?: string }) {
  const t = useT();
  return (
    <header className="os-cfg-page-head">
      <h2 className="os-cfg-page-title">{t(titleKey)}</h2>
      {descriptionKey && <p className="os-hint">{t(descriptionKey)}</p>}
    </header>
  );
}
