import { act, cleanup, render, screen } from "@testing-library/react";
import { afterEach, expect, it } from "vitest";
import { setLanguage } from "../../../i18n";
import { IpcError, localError } from "../../../ipc/errors";
import { DetailError } from "./Feedback";

afterEach(cleanup);
// spec: UI-030
// spec: UI-126
it("shows the command message and diagnostic detail, with a readable fallback for unknown keys", async () => {
  await setLanguage("en");
  const error = { ...localError("Io", "err-io"), detail: "fixture: read connection closed" };
  const mounted = render(<DetailError error={new IpcError(error)} />);
  expect(screen.getByRole("alert")).toHaveTextContent("A file operation failed.");
  expect(screen.getByRole("alert")).toHaveTextContent(error.detail);
  mounted.rerender(<DetailError error={{ ...error, messageKey: "err-bridge-closed" }} />);
  expect(screen.getByRole("alert")).toHaveTextContent("err-bridge-closed");
  expect(screen.getByRole("alert")).toHaveTextContent(error.detail);
  mounted.rerender(<DetailError error={{ ...error, messageKey: "err-fixture-untranslated" }} />);
  expect(screen.getByRole("alert")).toHaveTextContent("A file operation failed.");
  expect(screen.getByRole("alert")).toHaveTextContent("err-fixture-untranslated");
  await act(async () => { await setLanguage("de"); });
  mounted.rerender(<DetailError error={error} />);
  expect(screen.getByRole("alert")).toHaveTextContent("Ein Dateivorgang ist fehlgeschlagen.");
  await act(async () => { await setLanguage("en"); });
});
