import { ComparisonDetail } from "../../src/features/compare/ComparisonDetail";
import ReactDOM from "react-dom/client";
import { createBridgeClient, setClient } from "../../src/ipc/client";
import { setLanguage } from "../../src/i18n";
import { ComparisonControls } from "../../src/features/compare/ComparisonControls";
import { ComparisonTable, ComparisonTabs } from "../../src/features/compare/ComparisonTable";
import { ComparisonNumbers } from "../../src/features/compare/ComparisonNumbers";
import { useComparison } from "../../src/features/compare/model";
// B4-CMPUI: the comparison panels fill their region (like the shell's table and detail areas); the harness gives them one.
import "../../src/ui/ui.css";
const client=createBridgeClient(new URLSearchParams(location.search).get("bridge")!);setClient(client);
declare global {interface Window { compareHarness: { invoke: typeof client.invoke; state: typeof useComparison.getState } }}
window.compareHarness={invoke:client.invoke,state:useComparison.getState};
function Harness(){const active=useComparison(s=>!!s.session);return <><ComparisonControls />{active && <><ComparisonTabs /><div style={{height:360,display:"flex"}}><ComparisonTable /></div><div style={{height:420,display:"flex"}}><ComparisonDetail /></div><ComparisonNumbers /><ComparisonNumbers issues /></>}</>;}
void setLanguage("en").then(()=>ReactDOM.createRoot(document.getElementById("root")!).render(<Harness />));
