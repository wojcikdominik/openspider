#!/usr/bin/env bash
# B4-LIVEDETAIL (M2-N32, UI-145): verbindliche Messung „Detail-Tab Inlinks/Outlinks während des Live-Crawls“ im
# Release-Build auf der Referenzhardware (Mac mini, aktive, entsperrte GUI-Sitzung). Vom Repository-Wurzelverzeichnis:
#   apps/desktop/e2e-b/perf/livedetail-mac.sh            # bauen + messen (10 Runden, 5000 Seiten, 150 URL/s)
#   apps/desktop/e2e-b/perf/livedetail-mac.sh --measure-only
# Ergebnis: target/loadtest/livedetail-native.json (Zeitaufteilung je Klick), Exit 1 bei Fehler oder Budgetverletzung.
# Optional zusätzlich: OS_LIVEDETAIL_CORE=1 (Kernmessung ohne UI, Release-Test), OS_LIVEDETAIL_WEBKIT=1 (Playwright-WebKit
# mit Release-Bridge, Anhaltswert). Größen: OS_LIVEDETAIL_PAGES/RATE/THREADS/WARMUP/ROUNDS/IDLE_MS.
set -euo pipefail
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$script_dir/../../../.."
[[ "$(uname -s)" == Darwin ]] || { echo "requires macOS" >&2; exit 1; }
if [[ "$(ioreg -n Root -d1 -a | plutil -extract IOConsoleLocked raw -o - - 2>/dev/null)" == true ]]; then
  echo "Screen is locked: native measurement requires an unlocked GUI session" >&2; exit 1
fi
if [[ "${1:-}" != --awake ]]; then exec caffeinate -dimsu "$script_dir/livedetail-mac.sh" --awake "$@"; fi
shift
export OS_PERF_LOAD_LIMIT="${OS_PERF_LOAD_LIMIT:-2.5}" OPENSPIDER_SECRET_STORE=memory CARGO_INCREMENTAL=0
quiet() { python3 -c 'import os,sys; sys.exit(0 if os.getloadavg()[0] < float(sys.argv[1]) else 1)' "$OS_PERF_LOAD_LIMIT"; }
wait_quiet() { while ! quiet; do echo "Waiting for load < $OS_PERF_LOAD_LIMIT"; sleep 15; done; }
mkdir -p target/loadtest
if [[ "${1:-}" != --measure-only ]]; then
  npm ci
  cargo build --release --locked -j 6 -p os-testsite -p os-cli --bins
  node apps/desktop/e2e-b/perf/prepare.mjs
  # tauri-build verlangt im Release-Profil die Sidecar-Datei (bundle.externalBin); sie bleibt nicht im Arbeitsverzeichnis.
  triple="$(rustc -vV | sed -n 's/^host: //p')"
  sidecar="apps/desktop/src-tauri/binaries/openspider-$triple"
  staged=""
  if [[ ! -e "$sidecar" ]]; then
    mkdir -p apps/desktop/src-tauri/binaries
    cp target/release/openspider "$sidecar"
    staged=1
  fi
  build_status=0
  npm exec -w apps/desktop -- tauri build --no-bundle --features e2e-webdriver --config ../../target/ui-perf-native.conf.json || build_status=$?
  if [[ -n "$staged" ]]; then rm -f "$sidecar"; rmdir apps/desktop/src-tauri/binaries 2>/dev/null || true; fi
  (( build_status == 0 )) || exit "$build_status"
  if [[ "${OS_LIVEDETAIL_CORE:-}" == 1 ]]; then cargo test --release --locked -j 6 -p os-app --test b4_livedetail_core --no-run; fi
  if [[ "${OS_LIVEDETAIL_WEBKIT:-}" == 1 ]]; then
    cargo build --release --locked -j 6 -p os-app --bin os-app-bridge --features e2e-bridge
    npm exec -w apps/desktop -- playwright install webkit
  fi
fi
{ system_profiler SPHardwareDataType SPSoftwareDataType; sw_vers; node --version; rustc --version; git rev-parse HEAD; } > target/loadtest/livedetail-hardware.txt 2>&1 || true
status=0
wait_quiet
node apps/desktop/e2e-b/perf/livedetail.mjs target/release/openspider-desktop || status=$?
if [[ "${OS_LIVEDETAIL_CORE:-}" == 1 ]]; then
  wait_quiet
  cargo test --release --locked -j 6 -p os-app --test b4_livedetail_core -- --ignored --nocapture 2>&1 | grep -E "B4_LIVEDETAIL_CORE|test result" | tee target/loadtest/livedetail-core.txt || status=1
fi
if [[ "${OS_LIVEDETAIL_WEBKIT:-}" == 1 ]]; then
  wait_quiet
  OS_APP_BRIDGE_BIN="$PWD/target/release/os-app-bridge" OS_TESTSITE_BIN="$PWD/target/release/os-testsite" OS_E2E_PROJECTS=webkit OS_LIVEDETAIL_LABEL=mac \
    npm exec -w apps/desktop -- playwright test --config e2e/perf/livedetail.config.ts || status=1
fi
exit $status
