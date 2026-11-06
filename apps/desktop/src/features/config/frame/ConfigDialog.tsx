import { pageSaveBlocked } from "./registry";
// Rahmen des Konfigurationsdialogs (M2-U13, M2-ui-concept 1.5 / W3): Seitennavigation mit Suche, Profilwahl, Hinweise zum
// Crawl-Zustand (CRAWL-126, F-13/F-14), Abweichungen und die Fußleiste. Die Seiten kommen aus `features/config/pages/*`.
import "./config.css";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Dialog as RacDialog, Heading, Modal, ModalOverlay } from "react-aria-components";
import { ChevronRight } from "lucide-react";
import { useT } from "../../../i18n";
import { crawl as crawlApi } from "../../../ipc/crawl";
import { toCmdError } from "../../../ipc/errors";
import { profile as profileApi } from "../../../ipc/profile";
import { useSession } from "../../../state/session";
import { Banner, Button, DialogFooter, SearchField, Select, type BannerKind } from "../../../ui/primitives";
import { ConfigProvider, messageText, useConfig as useConfigContext, type ConfigContextValue } from "./context";
import { DiffPanel } from "./DiffPanel";
import { useConfigDraft } from "./draft";
import { focusAnchor } from "./focus";
import { deviations, pageForAnchor, runModeOf, type ConfigJson } from "./model";
import { checkProfileLoad, useProfileList } from "./profiles";
import { navigation, resolvePages, type ResolvedPage } from "./registry";
import { searchPages } from "./search";
import { closeConfig, useConfigDialog } from "./store";
import { groupTitleKey } from "./types";

const pageAnchors = (p: ResolvedPage): string[] => [...p.anchors, ...p.fields.map((f) => f.path)];

interface Notice {
  kind: BannerKind;
  text: string;
}

export function ConfigDialog() {
  const t = useT();
  const { anchor, diff, revision } = useConfigDialog();
  const crawl = useSession((s) => s.crawl);
  const runMode = runModeOf(crawl);
  const draft = useConfigDraft(true);
  const pages = useMemo(() => resolvePages(), []);
  const nav = useMemo(() => navigation(pages), [pages]);
  const anchorPages = useMemo(() => pages.map((p) => ({ id: p.id, anchors: pageAnchors(p) })), [pages]);
  const firstReal = pages.find((p) => p.Component)?.id ?? pages[0]?.id ?? "";
  const [pageId, setPageId] = useState<string>(() => (anchor ? (pageForAnchor(anchorPages, anchor)?.id ?? firstReal) : firstReal));
  const [pendingFocus, setPendingFocus] = useState<string | null>(anchor);
  const [showDiff, setShowDiff] = useState(diff);
  const [query, setQuery] = useState("");
  const [collapsed, setCollapsed] = useState<Record<string, boolean>>({});
  const [notice, setNotice] = useState<Notice | null>(null);
  const [profileName, setProfileName] = useState<string | null>(null);
  const [pausing, setPausing] = useState(false);
  const [profilesRevision, setProfilesRevision] = useState(0);
  const contentRef = useRef<HTMLDivElement>(null);
  const profiles = useProfileList(true);
  const reloadProfiles = profiles.reload;
  useEffect(() => {
    if (profilesRevision > 0) void reloadProfiles();
  }, [profilesRevision, reloadProfiles]);

  const page = pages.find((p) => p.id === pageId) ?? pages[0];

  const goto = useCallback(
    (target: string) => {
      const byId = pages.find((p) => p.id === target);
      const hit = byId ?? pages.find((p) => p.id === pageForAnchor(anchorPages, target)?.id);
      if (hit) setPageId(hit.id);
      setShowDiff(false);
      setPendingFocus(byId ? null : target);
    },
    [pages, anchorPages],
  );

  // Sprungziel von außen (UI-016 / openConfig): Seite wählen und das Ziel-Steuerelement fokussieren.
  useEffect(() => {
    if (!anchor) return;
    goto(anchor);
  }, [anchor, revision, goto]);
  useEffect(() => {
    if (diff) setShowDiff(true);
  }, [diff, revision]);

  useEffect(() => {
    if (!pendingFocus || draft.status !== "ready" || showDiff) return;
    const raf = requestAnimationFrame(() => {
      // A late frame (hidden or busy WebKit) must not pull the focus out of a field the user has already moved to:
      // the next keystrokes would land in the jump target.
      const active = document.activeElement;
      const editing = active instanceof HTMLElement && !!contentRef.current?.contains(active) && active.matches("input, textarea, select");
      if (!editing) focusAnchor(contentRef.current, pendingFocus);
      setPendingFocus(null);
    });
    return () => cancelAnimationFrame(raf);
  }, [pendingFocus, pageId, draft.status, showDiff]);

  useEffect(() => {
    if (runMode !== "running") setPausing(false);
  }, [runMode]);

  const pauseAndEdit = useCallback(async () => {
    setPausing(true);
    try {
      await crawlApi.pause();
    } catch (e) {
      setPausing(false);
      const err = toCmdError(e);
      setNotice({ kind: "error", text: messageText(t, err.messageKey, err.args, err.detail) });
    }
  }, [t]);

  const loadProfile = useCallback(
    async (name: string): Promise<boolean> => {
      try {
        const loaded = (await profileApi.load({ name })).config as ConfigJson;
        const check = checkProfileLoad(runMode, draft.cfg, loaded ?? {});
        if (!check.ok) {
          const text =
            check.reason === "locked"
              ? t("config-profile-blocked-fields", { name, fields: check.fields.join(", ") })
              : t(check.reason === "running" ? "config-profile-blocked-running" : "config-profile-blocked-foreign");
          setNotice({ kind: "warning", text });
          return false;
        }
        draft.replace(loaded ?? {});
        setProfileName(name);
        setNotice({ kind: "info", text: t("config-profile-loaded", { name }) });
        return true;
      } catch (e) {
        const err = toCmdError(e);
        setNotice({ kind: "error", text: messageText(t, err.messageKey, err.args, err.detail) });
        return false;
      }
    },
    [draft, runMode, t],
  );

  const value: ConfigContextValue = {
    draft,
    runMode,
    pauseAndEdit,
    pausing,
    goto,
    profileName,
    loadProfile,
    profilesRevision,
    bumpProfiles: () => setProfilesRevision((n) => n + 1),
  };

  const onOk = async () => {
    if (runMode === "foreign" || !draft.dirty) {
      closeConfig();
      return;
    }
    const result = await draft.save();
    if (!result) return;
    if (result.deferred.length > 0) {
      setNotice({ kind: "info", text: t("config-deferred", { fields: result.deferred.join(", ") }) });
      return;
    }
    closeConfig();
  };

  const hits = useMemo(() => searchPages(pages, query, t), [pages, query, t]);
  const deviationCount = useMemo(() => (draft.status === "ready" ? deviations(draft.cfg, draft.saved, draft.defaultRows).length : 0), [draft.status, draft.cfg, draft.saved, draft.defaultRows]);

  const Content = page?.Component ?? null;
  return (
    <ModalOverlay className="os-overlay" isOpen onOpenChange={(open) => !open && closeConfig()} isDismissable={false}>
      <Modal className="os-modal os-config-modal">
        <RacDialog className="os-dialog os-config-dialog" aria-label={t("config-title")}>
          <header className="os-config-head">
            <Heading slot="title" className="os-dialog-title">
              {t("config-title")}
            </Heading>
            <Select
              className="os-config-profile"
              label={t("config-profile")}
              value={profileName ?? ""}
              onChange={(name) => name !== "" && void loadProfile(name)}
              isDisabled={runMode === "running" || runMode === "foreign"}
              options={[{ value: "", label: t("config-profile-session") }, ...profiles.profiles.map((p) => ({ value: p.name, label: p.isDefault ? t("config-profile-default-mark", { name: p.name }) : p.name }))]}
            />
          </header>
          <div className="os-config-body">
            <nav className="os-config-nav" aria-label={t("config-nav")}>
              <SearchField label={t("config-search")} placeholder={t("config-search-placeholder")} clearLabel={t("config-search-clear")} value={query} onChange={setQuery} onClear={() => setQuery("")} />
              {query.trim() !== "" ? (
                <ul className="os-config-results" aria-label={t("config-search-results")}>
                  {hits.length === 0 && <li className="os-hint">{t("config-search-none")}</li>}
                  {hits.map((h) => (
                    <li key={`${h.pageId}|${h.path ?? ""}`}>
                      <button
                        type="button"
                        className="os-config-result"
                        data-path={h.path ?? undefined}
                        onClick={() => {
                          goto(h.path ?? h.pageId);
                          setQuery("");
                        }}
                      >
                        <span>{h.label}</span>
                        {h.path && <span className="os-hint">{h.pageTitle}</span>}
                      </button>
                    </li>
                  ))}
                </ul>
              ) : (
                <ul className="os-config-tree">
                  {nav.map((g) =>
                    g.group === null ? (
                      <li key={g.id}>
                        <NavButton page={g.pages[0] as ResolvedPage} current={pageId === g.id && !showDiff} onSelect={(id) => { setPageId(id); setShowDiff(false); }} />
                      </li>
                    ) : (
                      <li key={g.id}>
                        <button type="button" className="os-config-group" aria-expanded={!collapsed[g.group]} onClick={() => setCollapsed((c) => ({ ...c, [g.group as string]: !c[g.group as string] }))}>
                          <ChevronRight size={12} aria-hidden className="os-config-chevron" />
                          {t(groupTitleKey(g.group))}
                        </button>
                        {!collapsed[g.group] && (
                          <ul>
                            {g.pages.map((p) => (
                              <li key={p.id}>
                                <NavButton page={p} current={pageId === p.id && !showDiff} onSelect={(id) => { setPageId(id); setShowDiff(false); }} />
                              </li>
                            ))}
                          </ul>
                        )}
                      </li>
                    ),
                  )}
                </ul>
              )}
            </nav>
            <div className="os-config-content" ref={contentRef} data-page={pageId}>
              <ConfigProvider value={value}>
                <ModeBanners />
                {notice && (
                  <Banner kind={notice.kind} onDismiss={() => setNotice(null)} dismissLabel={t("config-dismiss")}>
                    {notice.text}
                  </Banner>
                )}
                {draft.saveError && (
                  <Banner kind="error">
                    {messageText(t, draft.saveError.messageKey, draft.saveError.args, draft.saveError.detail)}
                    {draft.saveError.field ? ` (${draft.saveError.field})` : ""}
                  </Banner>
                )}
                {draft.status === "loading" && <p className="os-hint" role="status">{t("config-loading")}</p>}
                {draft.status === "error" && draft.loadError && (
                  <Banner kind="error">{messageText(t, draft.loadError.messageKey, draft.loadError.args, draft.loadError.detail)}</Banner>
                )}
                {draft.status === "ready" && (showDiff ? <DiffPanel onBack={() => setShowDiff(false)} /> : Content ? <Content key={page?.id} /> : <Placeholder page={page} />)}
              </ConfigProvider>
            </div>
          </div>
          <DialogFooter
            okLabel={runMode === "foreign" ? t("config-close") : t("config-ok")}
            cancelLabel={t("config-cancel")}
            onOk={() => void onOk()}
            onCancel={closeConfig}
            okDisabled={draft.status !== "ready" || draft.saving}
            {...(pageSaveBlocked(pages, draft) ? { okDisabled: true } : {})}
            extra={
              <Button variant="secondary" onPress={() => setShowDiff((v) => !v)} isDisabled={draft.status !== "ready"} data-testid="config-deviations">
                {t("config-deviations", { count: deviationCount })}
              </Button>
            }
          />
        </RacDialog>
      </Modal>
    </ModalOverlay>
  );
}

function Placeholder({ page }: { page: ResolvedPage | undefined }) {
  const t = useT();
  return (
    <section className="os-cfg-placeholder" data-testid="config-placeholder">
      <h2 className="os-cfg-page-title">{page ? t(page.titleKey) : ""}</h2>
      <p className="os-hint">{t("config-page-pending")}</p>
    </section>
  );
}

function NavButton({ page, current, onSelect }: { page: ResolvedPage; current: boolean; onSelect: (id: string) => void }) {
  const t = useT();
  return (
    <button type="button" className="os-config-item" aria-current={current ? "page" : undefined} data-page-id={page.id} data-pending={page.Component ? undefined : "true"} onClick={() => onSelect(page.id)}>
      {t(page.titleKey)}
    </button>
  );
}

/** Hinweise zum Crawl-Zustand: läuft („Pausieren und bearbeiten“), pausiert/geöffnet, fremd gestartet (schreibgeschützt). */
function ModeBanners() {
  const t = useT();
  const { runMode, pauseAndEdit, pausing } = useConfigContext();
  if (runMode === "free") return null;
  if (runMode === "foreign") return <Banner kind="warning">{t("config-foreign-note")}</Banner>;
  if (runMode === "running")
    return (
      <Banner
        kind="info"
        action={
          <Button variant="secondary" onPress={() => void pauseAndEdit()} isDisabled={pausing} data-testid="config-pause-edit">
            {t("config-pause-and-edit")}
          </Button>
        }
      >
        {t("config-running-note")}
      </Banner>
    );
  return <Banner kind="info">{t("config-paused-note")}</Banner>;
}
