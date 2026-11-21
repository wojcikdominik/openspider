import "./contrast.css";
import { useId, type ReactNode } from "react";
import {
  Checkbox as RacCheckbox,
  FieldError,
  Input,
  Label,
  ListBox,
  ListBoxItem,
  Popover,
  Radio as RacRadio,
  RadioGroup as RacRadioGroup,
  SearchField as RacSearchField,
  Select as RacSelect,
  SelectValue,
  Switch as RacSwitch,
  Text,
  TextField as RacTextField,
  Button as RacButton,
} from "react-aria-components";
import { Check, ChevronDown, Search, X } from "lucide-react";
import { cx } from "./cx";

export interface TextFieldProps {
  label: string;
  value: string;
  onChange: (value: string) => void;
  /** Inline-Fehler (rot + Text), z. B. aus `CmdError.field`. */
  error?: string | null;
  description?: string;
  placeholder?: string;
  isDisabled?: boolean;
  type?: "text" | "url" | "number" | "password";
  className?: string;
  /** Anker für „Einstellung öffnen“ (UI-016): `data-setting-anchor`. */
  anchor?: string;
  inputMode?: "text" | "numeric" | "decimal" | "url";
  autoFocus?: boolean;
}

export function TextField({ label, value, onChange, error, description, placeholder, isDisabled, type = "text", className, anchor, inputMode, autoFocus }: TextFieldProps) {
  return (
    <RacTextField
      className={cx("os-field", className)}
      value={value}
      onChange={onChange}
      isInvalid={!!error}
      isDisabled={isDisabled}
      type={type}
      autoFocus={autoFocus}
      data-setting-anchor={anchor}
    >
      <Label className="os-label">{label}</Label>
      <Input className="os-input" placeholder={placeholder} inputMode={inputMode} />
      {description && <Text slot="description" className="os-hint">{description}</Text>}
      {error ? <FieldError className="os-error">{error}</FieldError> : null}
    </RacTextField>
  );
}

export interface SearchFieldProps {
  label: string;
  value: string;
  onChange: (value: string) => void;
  onSubmit?: (value: string) => void;
  onClear?: () => void;
  /** Suche läuft (Ladezustand). */
  isLoading?: boolean;
  placeholder?: string;
  clearLabel: string;
  className?: string;
}

export function SearchField({ label, value, onChange, onSubmit, onClear, isLoading, placeholder, clearLabel, className }: SearchFieldProps) {
  return (
    <RacSearchField
      className={cx("os-search", className)}
      aria-label={label}
      value={value}
      onChange={onChange}
      onSubmit={onSubmit}
      onClear={onClear}
      data-loading={isLoading ? "true" : undefined}
    >
      <Search size={14} aria-hidden className="os-search-icon" />
      <Input className="os-input" placeholder={placeholder} />
      <RacButton className="os-search-clear" aria-label={clearLabel}>
        <X size={12} aria-hidden />
      </RacButton>
      {isLoading && <span className="os-spinner" role="status" aria-label={label} />}
    </RacSearchField>
  );
}

export interface SwitchProps {
  children: ReactNode;
  isSelected: boolean;
  onChange: (selected: boolean) => void;
  isDisabled?: boolean;
  anchor?: string;
}

export function Switch({ children, isSelected, onChange, isDisabled, anchor }: SwitchProps) {
  return (
    <RacSwitch className="os-switch" isSelected={isSelected} onChange={onChange} isDisabled={isDisabled} data-setting-anchor={anchor}>
      <span className="os-switch-track" aria-hidden>
        <span className="os-switch-thumb" />
      </span>
      <span>{children}</span>
    </RacSwitch>
  );
}

export function Checkbox({ children, isSelected, onChange, isDisabled }: SwitchProps) {
  return (
    <RacCheckbox className="os-checkbox" isSelected={isSelected} onChange={onChange} isDisabled={isDisabled}>
      <span className="os-checkbox-box" aria-hidden>
        {isSelected && <Check size={12} />}
      </span>
      <span>{children}</span>
    </RacCheckbox>
  );
}

export interface RadioOption {
  value: string;
  label: string;
}

export function RadioGroup({ label, value, onChange, options, orientation = "vertical", anchor, isDisabled }: {
  label: string;
  value: string;
  onChange: (value: string) => void;
  options: readonly RadioOption[];
  orientation?: "horizontal" | "vertical";
  anchor?: string;
  isDisabled?: boolean;
}) {
  return (
    <RacRadioGroup className="os-radio-group" value={value} onChange={onChange} orientation={orientation} isDisabled={isDisabled} data-setting-anchor={anchor}>
      <Label className="os-label">{label}</Label>
      <div className="os-radio-items">
        {options.map((o) => (
          <RacRadio key={o.value} value={o.value} className="os-radio">
            <span className="os-radio-dot" aria-hidden />
            <span>{o.label}</span>
          </RacRadio>
        ))}
      </div>
    </RacRadioGroup>
  );
}

export interface SelectProps {
  label: string;
  value: string;
  onChange: (value: string) => void;
  options: readonly RadioOption[];
  isDisabled?: boolean;
  anchor?: string;
  className?: string;
}

/** Auswahlfeld (Dropdown); für Filter mit Zähler und Badges dient die ComboBox des jeweiligen Features. */
export function Select({ label, value, onChange, options, isDisabled, anchor, className }: SelectProps) {
  const id = useId();
  return (
    <RacSelect
      className={cx("os-field", "os-select", className)}
      selectedKey={value}
      onSelectionChange={(key) => {
        if (key !== null) onChange(String(key));
      }}
      isDisabled={isDisabled}
      data-setting-anchor={anchor}
      id={id}
    >
      <Label className="os-label">{label}</Label>
      <RacButton className="os-select-button">
        <SelectValue className="os-select-value" />
        <ChevronDown size={14} aria-hidden />
      </RacButton>
      <Popover className="os-popover">
        <ListBox className="os-listbox">
          {options.map((o) => (
            <ListBoxItem key={o.value} id={o.value} textValue={o.label} className="os-listbox-item">
              {o.label}
            </ListBoxItem>
          ))}
        </ListBox>
      </Popover>
    </RacSelect>
  );
}
