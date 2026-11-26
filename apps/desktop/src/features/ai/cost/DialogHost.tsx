import { ConsentDialog } from "../consent/ConsentDialog";
import { CostEstimateDialog } from "./EstimateDialog";

/**
 * Host der Dialoge der KI-Vorschau im Hauptfenster: „Kostenvoranschlag“ (AI-012) und „Datenfluss bestätigen“ (AI-018). Er wird
 * mit dem Statusleistenfeld `Kosten` nur bei eingeschaltetem Schalter „KI (Vorschau)“ eingehängt; geöffnet werden die Dialoge
 * über `requestCostConfirmation`/`showCostEstimate` bzw. `requestConsent`/`showConsentDialog` (Einstieg eines KI-Laufs).
 */
export function AiDialogHost() {
  return <>
    <CostEstimateDialog />
    <ConsentDialog />
  </>;
}
