// Prüfansicht der KI-Vorschläge (08 AI-076, M4-V01): je URL Kandidaten mit Länge, Pixelbreite (aus dem Kern, UI-038),
// Gültigkeit und Verstößen; „Übernehmen“, „Bearbeiten“ (Live-Prüfung im Kern), „Ablehnen“, „Zurücksetzen“; Massenaktion
// „Kandidat 1 übernehmen“ für markierte URLs; SERP-Vorschau des gewählten Kandidaten (UI-037/ONPAGE-194) über
// `detail.serpMeasure` – dieselbe Kürzung wie für einen gleichlautenden echten Title.
import { useEffect, useMemo, useState } from "react";
import { detail as detailApi } from "../../../ipc/detail";
import { isSilentError, toCmdError } from "../../../ipc/errors";
import { eventBus } from "../../../ipc/events";
import type { SerpMeasureRow } from "../../../ipc/gen/SerpMeasureRow";
import { useT } from "../../../i18n";
import { Button } from "../../../ui/primitives/Button";
import { Badge, Banner } from "../../../ui/primitives/Feedback";
import { TextField } from "../../../ui/primitives/fields";
import { displayText } from "../../detail/serp/model";
import {
  ELEMENTS,
  initialCandidate,
  serpTexts,
  statusHint,
  suggestApi,
  violationText,
  type CheckResult,
  type ElementId,
  type SuggestDetail,
} from "./model";
import "../../detail/serp/serp.css";
import "./suggestions.css";

export interface ReviewPanelProps {
  /** Fokus-URL der oberen Tabelle. */
  urlId?: number;
  /** Markierte URLs (Massenaktion). */
  selected: readonly number[];
}

export function ReviewPanel({ urlId, selected }: ReviewPanelProps) {
  const t = useT();
  const [element, setElement] = useState<ElementId>("title");
  const [data, setData] = useState<SuggestDetail>();
  const [error, setError] = useState<string>();
  const [version, setVersion] = useState(0);
  const [chosen, setChosen] = useState<number>();
  const [editing, setEditing] = useState<string | null>(null);
  const [check, setCheck] = useState<CheckResult>();
  const [rows, setRows] = useState<SerpMeasureRow[]>([]);
  const [message, setMessage] = useState<string>();
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (urlId === undefined) { setData(undefined); return; }
    const controller = new AbortController();
    void suggestApi.detail(urlId, controller.signal)
      .then((value) => { if (!controller.signal.aborted) { setData(value); setError(undefined); } })
      .catch((e) => { if (!controller.signal.aborted && !isSilentError(e)) setError(t(toCmdError(e).messageKey)); });
    return () => controller.abort();
  }, [urlId, version]);

  // Grenzwertänderung in den Preferences: Kandidaten ohne Request neu prüfen (AI-075), danach neu laden.
  useEffect(() => eventBus.on("settingsChanged", () => {
    void suggestApi.recheck().then(() => setVersion((v) => v + 1)).catch(() => undefined);
  }), []);

  const el = data?.elements.find((e) => e.element === element);
  useEffect(() => { setChosen(initialCandidate(el)); setEditing(null); setCheck(undefined); }, [data, element]);
  const candidate = el?.candidates.find((c) => c.index === chosen);
  const previewText = editing ?? candidate?.value ?? el?.current ?? "";

  // Live-Prüfung des bearbeiteten Texts im Kern (Länge, Pixelbreite, Dubletten, Sprache).
  useEffect(() => {
    if (editing === null || urlId === undefined) return;
    const controller = new AbortController();
    void suggestApi.check({ urlId, element, value: editing }, controller.signal)
      .then((value) => { if (!controller.signal.aborted) setCheck(value); })
      .catch(() => undefined);
    return () => controller.abort();
  }, [editing, element, urlId]);

  // SERP-Vorschau des gewählten Kandidaten (Messung und Kürzung im Kern).
  const serp = useMemo(() => serpTexts(data, element, previewText), [data, element, previewText]);
  useEffect(() => {
    if (!data) return;
    const controller = new AbortController();
    void detailApi.serpMeasure({ title: serp.title, description: serp.description, device: "desktop", boldTerms: [], prefix: "" }, controller.signal)
      .then((value) => { if (!controller.signal.aborted) setRows(value.rows); })
      .catch(() => undefined);
    return () => controller.abort();
  }, [serp.title, serp.description, data]);

  const act = async (action: "accept" | "edit" | "reject" | "reset" | "acceptFirst", extra: { candidate?: number; value?: string } = {}) => {
    if (urlId === undefined) return;
    setBusy(true); setMessage(undefined);
    try {
      const urlIds = action === "acceptFirst" ? [...selected] : [urlId];
      const result = await suggestApi.review({ urlIds, element, action, ...extra });
      if (action === "acceptFirst") setMessage(t("ai-suggestions-bulk-result", { accepted: result.accepted ?? 0, skipped: result.skipped ?? 0 }));
      else if (result.errors.length > 0) setMessage(t("ai-suggestions-not-accepted"));
      setEditing(null);
      setVersion((v) => v + 1);
    } catch (e) {
      setMessage(t(toCmdError(e).messageKey));
    } finally {
      setBusy(false);
    }
  };

  if (urlId === undefined) return <p className="os-hint">{t("ai-suggestions-select-row")}</p>;
  if (error) return <Banner kind="error">{error}</Banner>;
  if (!data) return <p className="os-hint">{t("ai-suggestions-loading")}</p>;
  const hint = statusHint(t, el?.status);
  const titleRow = rows[0];
  const descriptionRow = rows[1];
  const shown = element === "title" ? displayText(serp.title, titleRow) : displayText(serp.description, descriptionRow);
  const metrics = element === "title" ? titleRow : descriptionRow;
  return <div className="os-ai-sugg-body" data-url-id={urlId}>
    <p className="os-ai-sugg-address">{data.address}</p>
    <div className="os-ai-sugg-elements" role="group" aria-label={t("ai-suggestions-element")}>
      {ELEMENTS.map((id) => <button key={id} type="button" className="os-btn" aria-pressed={element === id} onClick={() => setElement(id)}>
        {t(`ai-suggestions-element-${id}`)}
      </button>)}
    </div>
    {!el || el.status === null ? <p className="os-hint">{t("ai-suggestions-none")}</p> : <>
      <dl className="os-ai-sugg-status">
        <dt>{t("ai-suggestions-status")}</dt>
        <dd data-testid="ai-sugg-status" title={hint}>{el.status}{el.provisional ? ` (${t("ai-suggestions-provisional")})` : ""}</dd>
        <dt>{t("ai-suggestions-review")}</dt>
        <dd data-testid="ai-sugg-review">{t(`ai-suggestions-review-${(el.review ?? "Open").toLowerCase()}`)}</dd>
        <dt>{t("ai-suggestions-current")}</dt>
        <dd>{el.current ?? t("ai-suggestions-empty")}{el.currentPixelWidth ? ` · ${el.currentPixelWidth} px` : ""}</dd>
        {el.accepted && <><dt>{t("ai-suggestions-accepted-value")}</dt><dd data-testid="ai-sugg-accepted">{el.accepted.value}</dd></>}
      </dl>
      {hint && <p className="os-hint" role="note">{hint}</p>}
      {el.accepted && el.accepted.violations.length > 0 && <Banner kind="warning">
        <span data-testid="ai-sugg-accepted-violations">{t("ai-suggestions-edited-violations")}: {el.accepted.violations.map((v) => violationText(t, v)).join("; ")}</span>
      </Banner>}
      {el.candidates.length > 0 && <fieldset className="os-ai-sugg-candidates">
        <legend>{t("ai-suggestions-candidates")}</legend>
        {el.candidates.map((c, i) => <label key={c.index} className="os-ai-sugg-candidate" data-valid={c.valid ? "true" : "false"}>
          <input type="radio" name={`ai-sugg-${urlId}-${element}`} checked={chosen === c.index} onChange={() => { setChosen(c.index); setEditing(null); }} />
          <span className="os-ai-sugg-value">{t("ai-suggestions-candidate-n", { n: i + 1 })}: {c.value}</span>
          <span className="os-ai-sugg-metrics">{t("ai-suggestions-metrics", { length: c.length, px: c.pixelWidth ?? 0 })}</span>
          <Badge tone={c.valid ? "accent" : "danger"}>{c.valid ? t("ai-suggestions-valid") : t("ai-suggestions-invalid")}</Badge>
          {c.violations.length > 0 && <span className="os-ai-sugg-violations">{c.violations.map((v) => violationText(t, v)).join("; ")}</span>}
        </label>)}
      </fieldset>}
      {editing !== null && <div className="os-ai-sugg-edit">
        <TextField label={t("ai-suggestions-edit-field")} value={editing} onChange={setEditing} autoFocus />
        {check && <p className="os-hint" data-testid="ai-sugg-live" data-valid={check.valid ? "true" : "false"}>
          {t("ai-suggestions-metrics", { length: check.length, px: check.pixelWidth })}
          {check.violations.length > 0 ? ` · ${check.violations.map((v) => violationText(t, v)).join("; ")}` : ` · ${t("ai-suggestions-valid")}`}
        </p>}
      </div>}
      <div className="os-fach-actions">
        {editing === null
          ? <Button variant="primary" isDisabled={busy || !candidate?.valid} onPress={() => void act("accept", { candidate: chosen })}>{t("ai-suggestions-accept")}</Button>
          : <Button variant="primary" isDisabled={busy} onPress={() => void act("edit", { value: editing })}>{t("ai-suggestions-save-edit")}</Button>}
        {editing === null
          ? <Button isDisabled={busy} onPress={() => setEditing(candidate?.value ?? el.accepted?.value ?? el.current ?? "")}>{t("ai-suggestions-edit")}</Button>
          : <Button isDisabled={busy} onPress={() => { setEditing(null); setCheck(undefined); }}>{t("ai-suggestions-cancel-edit")}</Button>}
        <Button isDisabled={busy} onPress={() => void act("reject")}>{t("ai-suggestions-reject")}</Button>
        <Button isDisabled={busy} onPress={() => void act("reset")}>{t("ai-suggestions-reset")}</Button>
      </div>
      <section className="os-serp-preview" data-device="desktop" aria-label={t("ai-suggestions-serp")} data-testid="ai-sugg-serp">
        <p className="os-serp-title" data-truncated={titleRow?.truncated ? "true" : "false"}>{element === "title" ? shown : displayText(serp.title, titleRow)}</p>
        <p className="os-serp-description" data-truncated={descriptionRow?.truncated ? "true" : "false"}>{element === "title" ? displayText(serp.description, descriptionRow) : shown}</p>
      </section>
      {metrics && <p className="os-hint" data-testid="ai-sugg-serp-metrics">{t("ai-suggestions-serp-metrics", { px: metrics.px, max: metrics.maxPx, truncated: metrics.truncated ? t("ai-suggestions-yes") : t("ai-suggestions-no") })}</p>}
    </>}
    {selected.length > 1 && <div className="os-fach-actions">
      <Button isDisabled={busy} onPress={() => void act("acceptFirst")}>{t("ai-suggestions-bulk", { count: selected.length })}</Button>
    </div>}
    {message && <p role="status" className="os-hint" data-testid="ai-sugg-message">{message}</p>}
  </div>;
}
