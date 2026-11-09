import { createRoot } from "react-dom/client";
import { bootWindow } from "../../src/app/runtime";
import { setLanguage } from "../../src/i18n";
import { useSession } from "../../src/state/session";
import { CrawlsHost, openCrawls } from "../../src/features/crawls";
import { managementFixture } from "../../src/features/crawls/fixtures/core";
import { IpcError, localError } from "../../src/ipc/errors";
import { updateRecent } from "../../src/features/help/recent";
import "../../src/ui/ui.css";
const fixture = managementFixture();
const runtime = bootWindow({ kind: "main", transport: fixture.core.transport("main"), menu: false });
await runtime.ready;
await setLanguage("en");
const api = {
  calls: () => fixture.core.calls,
  setPaths: fixture.setPaths,
  setDestination: fixture.setDestination,
  /** CRAWL-127: seeds the one recent list (UI state), as opening crawls would. */
  seedRecent: (paths: string[]) => updateRecent(() => paths.map(path => ({ path }))),
  corrupt() { fixture.core.handlers.set("crawl.open", () => { throw new IpcError(localError("Store", "err-crawl-corrupt")); }); },
  complete(result: { path: string; integrityOk: boolean; usable: boolean; lostRows: number } | null = null) {
    const h = fixture.handle()!;
    useSession.setState({ ops: { [h.opId]: { ...h, state: "done", done: 1, result } } });
  },
};
declare global { interface Window { crawlsFixture: typeof api } }
window.crawlsFixture = api;
createRoot(document.getElementById("root")!).render(<main><button onClick={e => openCrawls(e.currentTarget)}>Manage</button><CrawlsHost desktop={fixture.desktop} /></main>);
