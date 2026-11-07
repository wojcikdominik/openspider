import { currentLanguage, i18n, useT } from "../../../i18n";
import { describeError, toCmdError } from "../../../ipc/errors";
import { Banner, ProgressBar } from "../../../ui/primitives/Feedback";
import { EmptyState } from "../../../ui/primitives/EmptyState";
import { openConfig } from "../../config/frame/store";
import type { EmptyState as EmptyStateDto } from "../../../ipc/gen/EmptyState";

export function DetailLoading() { const t = useT(); return <ProgressBar label={t("detail-loading")} />; }
export function DetailError({ error }: { error: unknown }) {
  const t = useT(); const e = toCmdError(error);
  const known = i18n.has(currentLanguage(), e.messageKey);
  const message = describeError(t, e, () => known);
  return <Banner kind="error"><span>{message.text}</span><p><code>{e.messageKey}</code></p>{message.detail && <p>{message.detail}</p>}</Banner>;
}
export function DetailEmpty({ state }: { state: EmptyStateDto }) {
  return <EmptyState state={state} onAction={action => { if (action.kind === "openSetting") openConfig({ anchor: action.anchor }); }} />;
}
