// Visualisierungsfenster (UI-090): nicht modal, Schnappschuss des Crawl-Stands beim Öffnen. Ändern sich die Daten,
// meldet der Kern `viz-data-changed`; die Kopfzeile zeigt dann „Daten geändert – Aktualisieren“, erst ein Klick lädt
// neu (Wurzel und Zoom bleiben erhalten).
import { useEffect, useState } from "react";
import { useWindowTitle } from "../../app/windowTitle";
import { useT } from "../../i18n";
import { describeError, toCmdError } from "../../ipc/errors";
import { eventBus } from "../../ipc/events";
import type { Notice } from "../../ipc/gen/Notice";
import { viz } from "../../ipc/viz";
import { useSession } from "../../state/session";
import { Banner, Button, EmptyState, Select } from "../../ui/primitives";
import { Graph3DView } from "./Graph3DView";
import { GraphView } from "./GraphView";
import { coreKind, isWindowKind, isWordKind, layoutOf, waitForKind, WINDOW_KINDS, type VizWindowKind, type VizWindowParams } from "./open";
import { WordCloudView } from "./WordCloudView";
import "./viz.css";

export function VizWindow({ params, exportDir }: { params: VizWindowParams; exportDir?: string | null }) {
  const t = useT();
  const [kind, setKind] = useState<VizWindowKind | null>(params.kind);
  const [waiting, setWaiting] = useState(params.kind === null && params.snapshotId !== null);
  const [changed, setChanged] = useState(false);
  const [provenance, setProvenance] = useState(false);
  const [generation, setGeneration] = useState(0);
  const [refreshing, setRefreshing] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const snapshotId = params.snapshotId;

  // Die App-Hülle übergibt nur die Schnappschuss-Kennung; die Art schreibt das öffnende Fenster in den gemeinsamen Speicher.
  useEffect(() => {
    if (kind !== null || !snapshotId) return;
    let live = true;
    void waitForKind(snapshotId).then((k) => {
      if (!live) return;
      setWaiting(false);
      if (k) setKind(k);
    });
    return () => {
      live = false;
    };
  }, [kind, snapshotId]);

  // UI-121 (B3-UI): document and native window title follow the UI language, also after a language change.
  useWindowTitle(`${kind ? t(`viz-kind-${kind}`) : t("app-viz-title")} – ${t("app-title")}`);

  useEffect(() => {
    const handle = (n: Notice) => {
      if (n.args.snapshotId !== snapshotId) return;
      if (n.code === "viz-data-changed") setChanged(true);
      if (n.code === "viz-provenance-missing") setProvenance(true);
    };
    // Meldungen kommen einzeln oder gebündelt im Tick.
    const offs = [eventBus.on("notice", handle), eventBus.on("tick", (tick) => tick.notices.forEach(handle))];
    // UI-090 (B4-LAGFIX3): Das Fenster abonniert seine Ereignisse vor dem ersten Rendern; eine Meldung aus dieser Lücke
    // liegt nur in den Sitzungsmeldungen. Der Kern meldet „Daten geändert“ je Schnappschuss nur einmal.
    useSession.getState().notices.forEach(handle);
    return () => offs.forEach((off) => off());
  }, [snapshotId]);

  const refresh = async () => {
    if (!snapshotId || !kind) return;
    setRefreshing(true);
    try {
      await viz.open({ kind: coreKind(kind), rootUrlId: isWordKind(kind) ? params.rootUrlId : null, refreshSnapshotId: snapshotId });
      // Die alte Meldung gilt dem alten Stand; sie darf nach einem erneuten Einhängen nicht wieder auftauchen.
      const session = useSession.getState();
      session.notices.filter((n) => n.code === "viz-data-changed" && n.args.snapshotId === snapshotId).forEach((n) => session.dismissNotice(n));
      setChanged(false);
      setGeneration((g) => g + 1);
    } catch (e) {
      setError(describeError(t, toCmdError(e)).text);
    } finally {
      setRefreshing(false);
    }
  };

  if (!snapshotId) return <EmptyState state={{ code: "NO_DATA_YET" }} />;

  return (
    <div className="os-viz">
      <header className="os-viz-header" role="banner">
        <h1 className="os-viz-title">{kind ? t(`viz-kind-${kind}`) : t("app-viz-title")}</h1>
        {changed && (
          <div className="os-viz-changed" role="status" data-testid="viz-data-changed">
            <span>{t("viz-data-changed")}</span>
            <Button variant="primary" isDisabled={refreshing} onPress={() => void refresh()}>
              {t("viz-refresh")}
            </Button>
          </div>
        )}
      </header>
      {provenance && (
        <Banner kind="info" onDismiss={() => setProvenance(false)} dismissLabel={t("ui-dialog-close")}>
          {t("viz-provenance-missing")}
        </Banner>
      )}
      {error && (
        <Banner kind="error" onDismiss={() => setError(null)} dismissLabel={t("ui-dialog-close")}>
          {error}
        </Banner>
      )}
      {!kind && !waiting && (
        <div className="os-viz-pick">
          <Select
            label={t("viz-pick-kind")}
            value=""
            onChange={(v) => isWindowKind(v) && setKind(v)}
            options={WINDOW_KINDS.map((k) => ({ value: k, label: t(`viz-kind-${k}`) }))}
          />
        </div>
      )}
      {kind && isWordKind(kind) && <WordCloudView snapshotId={snapshotId} urlId={params.rootUrlId} generation={generation} source={kind === "bodyWords" ? "body" : "anchors"} />}
      {kind && layoutOf(kind) === "force3d" && <Graph3DView kind={kind} snapshotId={snapshotId} generation={generation} initialRoot={params.rootUrlId} />}
      {kind && (layoutOf(kind) === "tree" || layoutOf(kind) === "force") && <GraphView kind={kind} snapshotId={snapshotId} generation={generation} initialRoot={params.rootUrlId} exportDir={exportDir} />}
    </div>
  );
}
