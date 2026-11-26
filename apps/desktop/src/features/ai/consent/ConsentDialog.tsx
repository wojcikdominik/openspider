// Dialog „Datenfluss bestätigen“ (AI-018): vor dem ersten Lauf einer KI-Aufgabe bzw. wenn ein Feld hinzukommt. Je Aufgabe Anbieter,
// Modell, Host, Region, gesendete Felder, Höchstzahl Zeichen, Bilder, Schwärzung, Trainingsnutzung laut Anbieterrichtlinie und
// `Stored by Provider` (Nachtrag 24.11.2026). Der PII-Hinweis erscheint nur bei Schwärzung aus und `page_text`/`h2`/
// `image_context` (Kern: `DataFlowRow::pii_notice`). Bestätigen speichert je Installation (`ai.consentGrant`, ConsentStore).
import { create } from "zustand";
import { useT } from "../../../i18n";
import { Dialog, DialogFooter } from "../../../ui/primitives";
import { aiApi, providerName, type ConsentRequestView, type ConsentView } from "../settings/api";
import { DataFlowTable, fieldLabel } from "./DataFlowTable";

interface ConsentState {
  view: ConsentView | null;
  resolve: ((granted: boolean) => void) | null;
  error: string | null;
}

export const useConsentDialog = create<ConsentState>(() => ({ view: null, resolve: null, error: null }));

/** Zeigt den Dialog für die offenen Anfragen in `view`; `true` nach „Bestätigen“ (gespeichert), sonst `false`. */
export function showConsentDialog(view: ConsentView): Promise<boolean> {
  useConsentDialog.getState().resolve?.(false);
  return new Promise((resolve) => useConsentDialog.setState({ view, resolve, error: null }));
}

/**
 * Einwilligungsregel vor einem KI-Lauf (AI-018): ohne offene Anfrage (alle Aufgaben eingewilligt) ohne Dialog weiter, sonst der
 * Dialog. Ohne Bestätigung sendet der Lauf keinen Request (Einträge `Pending (consent)`, Kern `policy::guard::execute`).
 */
export async function requestConsent(): Promise<boolean> {
  const view = await aiApi.consent();
  if (view.requests.length === 0) return true;
  return showConsentDialog(view);
}

function close(granted: boolean) {
  const { resolve } = useConsentDialog.getState();
  useConsentDialog.setState({ view: null, resolve: null, error: null });
  resolve?.(granted);
}

async function confirm(requests: ConsentRequestView[]) {
  try {
    await aiApi.grant(requests.map((r) => r.row.key));
    close(true);
  } catch (e) {
    useConsentDialog.setState({ error: e instanceof Error ? e.message : String(e) });
  }
}

/** Inhalt des Dialogs (ohne Rahmen). */
export function ConsentBody({ view }: { view: ConsentView }) {
  const t = useT();
  const pii = view.requests.some((r) => r.piiNotice);
  return (
    <div className="os-ai-consent" data-ai-consent-dialog>
      <p data-ai-redaction={view.redaction ? "on" : "off"}>{t(view.redaction ? "ai-consent-redaction-on" : "ai-consent-redaction-off")}</p>
      <DataFlowTable columns={view.columns} rows={view.requests.map((r) => r.row)} label={t("ai-consent-dialog-title")} />
      <ul className="os-ai-consent-added">
        {view.requests.map((r) => (
          <li key={`${r.row.key.provider}|${r.row.key.task}`} data-task={r.row.key.task}>
            {r.previousFields === null
              ? t("ai-consent-first", { task: r.row.key.task, provider: providerName(r.row.key.provider) })
              : t("ai-consent-added", { task: r.row.key.task, fields: r.addedFields.map((f) => fieldLabel(t, f)).join(", ") })}
          </li>
        ))}
      </ul>
      {pii && <p className="os-hint" role="note" data-ai-pii-notice>{t("ai-consent-pii-notice")}</p>}
    </div>
  );
}

export function ConsentDialog() {
  const t = useT();
  const view = useConsentDialog((s) => s.view);
  const error = useConsentDialog((s) => s.error);
  return (
    <Dialog
      title={t("ai-consent-dialog-title")}
      isOpen={view !== null}
      onOpenChange={(open) => {
        if (!open) close(false);
      }}
      size="large"
      footer={<DialogFooter okLabel={t("ai-consent-confirm")} cancelLabel={t("ai-consent-cancel")} onOk={() => view && void confirm(view.requests)} onCancel={() => close(false)} />}
    >
      {view && <ConsentBody view={view} />}
      {error && <p className="os-field-error" role="alert">{t("ai-consent-error", { detail: error })}</p>}
    </Dialog>
  );
}
