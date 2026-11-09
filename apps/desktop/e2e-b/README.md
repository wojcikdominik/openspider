# Native platform checks (M2-S02)

From the repository root, after `npm ci`:

```sh
cargo build --locked -p openspider-desktop --features e2e-webdriver
swiftc apps/desktop/spikes/macos.swift -o "$TMPDIR/s02-macos"
S02_MAC_HELPER="$TMPDIR/s02-macos" S02_GUI=1 \
  node apps/desktop/e2e-b/run.mjs "${CARGO_TARGET_DIR:-target}/debug/openspider-desktop" smoke
```

The binary must retain the `e2e-webdriver` feature throughout the run. A concurrent
default-feature Cargo build can replace it; finish builds before starting or copy
the feature binary to a temporary path. The runner starts its own Vite instance
on the app's configured dev port 1420 and fails if that port is occupied.
All fixture traffic, IPC and WebDriver requests use loopback. Each app gets a
temporary `OPENSPIDER_HOME` and `OPENSPIDER_SECRET_STORE=memory`.

Modes: `smoke` (main window, active desktop, separate settings window, close
confirmation while crawling), `s3` (native editing and table-dispatch fixture),
`s6` (running fixture crawl, five 30-second visibility windows). Reports, timings,
screenshots and logs go to ignored `artifacts/s02-<mode>/`, or the fourth argument.
Errors produce a nonzero exit, never `skip`/`continue-on-error` or a mock success.

On macOS use an unlocked GUI session. The default launches a temporary app bundle
through LaunchServices (`S02_GUI=0` explicitly selects the diagnostic child-process
path). Both paths record Tauri show/focus and `NSRunningApplication.activate`
results. The helper tries AXMain/AXRaise/AXFrontmost when Accessibility access
exists, then a title-bar click on the target PID's onscreen window if activation
has not succeeded. It yields activation only if it is itself active; a background
helper cannot yield Terminal's activation. It records the foreground PID, helper
activation, session lock and event-posting availability without requesting or
changing system permissions. For S3 it posts native Cmd keys: embedded-driver
keyboard actions use JavaScript and cannot verify NSMenu accelerator routing.
If native posting is unavailable, S3 fails explicitly. The table is a fixture
endpoint for the production dispatcher, not an implementation of the later grid.

On Windows build without `e2e-webdriver`, but with the test-only `TAURI_CONFIG` that
`ui-e2e-b.yml` sets in the step "Build Windows app": msedgedriver hands
`--remote-debugging-port=0` to WebView2 through `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS`,
which the app ignores because wry passes its own additional browser arguments through the
API ("DevToolsActivePort file doesn't exist"; a WebView2 policy value is ignored as well).
The override adds the flag (plus wry's default `--disable-features=...`) to both windows.
WebViews of one data directory need identical arguments, so in this build the settings
window exists hidden from startup and `openSettingsWindow` shows it; the product
configuration is unchanged. Put a WebView2-compatible `msedgedriver` on PATH
(`setup-windows.ps1` in CI). Then run
`node apps/desktop/e2e-b/run.mjs target/debug/openspider-desktop.exe s3`. The runner
starts `msedgedriver` itself (`--verbose`, log `msedgedriver.log` in the output folder)
and connects WebdriverIO with the capabilities `tauri-driver` would build
(`browserName: webview2`, `ms:edgeOptions.binary`); there is no retry, a failed session
records the app/WebView2 command lines in `report.processes`. `S02_WINDOWS_DRIVER=tauri`
selects `tauri-driver` 2.1.0 (`cargo install tauri-driver --version 2.1.0 --locked`) instead.
The runner invokes `focus-windows.ps1` without showing a console window. It
identifies the target by its exact executable path, restores it, requests foreground
permission and temporarily attaches the input threads around `SetForegroundWindow`.
Every attached thread is detached in `finally`; a timeout bounds the helper.

Before **each** shortcut on either platform, the runner reactivates the app,
focuses the intended fixture element, waits for two consecutive visible, focused
WebView samples, inspects the native foreground app and reads `document.hasFocus()`
again. The macOS key helper also checks the foreground PID immediately before
posting. A failed prerequisite prevents injection. Reports contain each key's
activation result, focus samples, target and injection status (`keyboardFocus`).
The early `beforeActivation`/`afterTauriActivation` snapshots are diagnostics,
not the keyboard readiness decision. `focus.test.mjs` covers delayed activation,
focus stolen after a WebDriver action, and refusing input to hidden/minimized or
incorrectly focused fixtures.

S6 records frontend visibility and the shell's visible/minimized inputs separately.
Private core source state and native macOS occlusion are not exposed on the base
branch. A missing running crawl or a zero-tick interval cannot pass S6. Even valid
rate observations alone will not pass the native-source requirement until the
shell provides that measurement seam. See the precise handoff in `S-6.md`.

`ui-e2e-b.yml` runs nightly, manually, and as a reusable workflow;
`spike-platform.yml` manually calls it for S3 (both platforms) and S6 (macOS).
The first branch CI runs on `6e31faa` reached Vite but timed out before app startup:
ANSI colors split the readiness strings. The runner now normalizes that output;
regression fixtures retain the five actual CI logs and reports. Run both suites:

```sh
node --test apps/desktop/e2e-b/measurement.test.mjs apps/desktop/e2e-b/readiness.test.mjs apps/desktop/e2e-b/focus.test.mjs
```

A Windows run of the corrected runner and an unlocked macOS GUI run are still
required. Actual evidence and outstanding gates live in `docs/plan/M2-spikes/`.

`app.requestQuit` with a `confirm` decision broadcasts the `app-quit-confirmation`
notice, which opens the quit dialog (same kernel entry as the native close). The smoke
therefore closes the window first, while no dialog is open, and reads the quit decision
afterwards from the same running crawl (cause of the red nightlies since 22.10.2026:
`docs/plan/M3-reports/nightly-b.md`).

## M2-U32

Mode `u32` adds real OS clipboard, file-dialog cancellation and positive file selection,
singleton/independent settings close, notification permission/plugin acceptance,
OS/DOM/canvas HiDPI, and the existing running-crawl quit smoke plus cancellation.
`U32_EXPECT_DSF` is mandatory and must match the actual display scale. Mode `menu`
uses the macOS AX helper in `e2e-c/menu.swift` for native menu snapshots and Help smoke.
These modes have not been run on Linux; exact platform commands and remaining
OS observations are in `docs/plan/M2-acceptance/e2e/ausfuehrung.md`.

```sh
npx tsc -p apps/desktop/e2e-b/tsconfig.u32.json
node --check apps/desktop/e2e-b/run.mjs
```
