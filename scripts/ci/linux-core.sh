#!/usr/bin/env bash
# Linux-Serverbetrieb von Kern, CLI und MCP-STDIO (OUT-273, Etappe E0, Paket M2-V01).
#
# Aufruf:  scripts/ci/linux-core.sh [schritt ...]
#   Schritte: fmt | clippy | chromium | disk | test | release | golden | all (Standard: all)
#
#   fmt       cargo fmt --all --check
#   clippy    cargo clippy --workspace --exclude openspider-desktop --all-targets --locked -- -D warnings
#   chromium  sucht Chromium (OPENSPIDER_CHROMIUM, sonst vorinstalliertes Chrome, sonst Playwright-Chromium) und prüft es
#             per `--headless=new --dump-dom` auf eine lokale Datei, ohne DISPLAY und ohne virtuellen Framebuffer;
#             schreibt OPENSPIDER_CHROMIUM nach $GITHUB_ENV, damit spätere Schritte (Rendering-Tests) es sehen
#   disk      freier Speicher auf den Laufwerken von Zielordner und TMPDIR; unter LINUX_CORE_MIN_FREE_GIB (Standard 4)
#             bricht der Schritt mit klarer Meldung ab (CRAWL-124: die Engine pausiert Crawls unter 1 GiB)
#   test      cargo test --workspace --exclude openspider-desktop --locked, ohne D-Bus/Secret Service,
#             mit der Skip-Liste scripts/ci/linux-core-skips.txt (Ziel: leer); prüft nach dem Kompilieren `disk`
#   release   cargo build --release -p os-cli --locked; Binärdatei nach target/linux-core/openspider
#   golden    os-cli-Test tests/linux_server.rs gegen die Release-Binärdatei (Crawl von out-basic-site, Golden
#             Internal:All, MCP-STDIO, Fehler statt Hänger bei fehlendem Geheimnis)
#
# Die Desktop-App (openspider-desktop) wird weder gebaut noch geprüft; es werden keine Linux-Pakete installiert.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

SKIPS_FILE="scripts/ci/linux-core-skips.txt"
CARGO_TARGET="${CARGO_TARGET_DIR:-target}"
OUT_DIR="target/linux-core"
SUMMARY="${GITHUB_STEP_SUMMARY:-/dev/null}"
NOTES="$(mktemp)"   # Anmerkungen (Skip-Liste, Chromium-Pfad); kommen hinter die Tabelle der Schritte
trap 'cat "$NOTES" >> "$SUMMARY" 2> /dev/null || true; rm -f "$NOTES"' EXIT

# --- Umgebung (M2.md C5, C13): ressourcenschonend, kein Schlüsselbund, kein D-Bus, keine Anzeige --------------------
export CARGO_TERM_COLOR="${CARGO_TERM_COLOR:-always}"
export CARGO_INCREMENTAL=0
export CARGO_PROFILE_DEV_DEBUG="${CARGO_PROFILE_DEV_DEBUG:-line-tables-only}"
export CARGO_PROFILE_TEST_DEBUG="${CARGO_PROFILE_TEST_DEBUG:-line-tables-only}"
export OPENSPIDER_SECRET_STORE=memory
unset DBUS_SESSION_BUS_ADDRESS DISPLAY WAYLAND_DISPLAY
# Ohne Sitzungsbus unter $XDG_RUNTIME_DIR/bus findet auch ein Secret-Service-Client keinen Dienst.
XDG_RUNTIME_DIR="$(mktemp -d)"
export XDG_RUNTIME_DIR

log() { printf '\n==> %s\n' "$*"; }
die() { printf '::error::%s\n' "$*" >&2; exit 1; }

# Führt einen Schritt aus und hält Name, Ergebnis und Dauer in der Job-Zusammenfassung fest.
timed() {
  local name="$1"
  shift
  local start end status=0
  start=$(date +%s)
  # Eigene Subshell mit `set -e`: in einer `||`-Liste wäre errexit sonst abgeschaltet und ein Fehler mitten im Schritt unbemerkt.
  set +e
  ( set -e; "$@" )
  status=$?
  set -e
  end=$(date +%s)
  printf '| %s | %s | %ss |\n' "$name" "$([ "$status" -eq 0 ] && echo ok || echo "FEHLER ($status)")" "$((end - start))" >> "$SUMMARY"
  return "$status"
}

# --- Skip-Liste ------------------------------------------------------------------------------------------------------
SKIP_ARGS=()
SKIP_COUNT=0
load_skips() {
  SKIP_ARGS=()
  SKIP_COUNT=0
  [ -f "$SKIPS_FILE" ] || die "$SKIPS_FILE fehlt"
  local line name reason
  while IFS= read -r line || [ -n "$line" ]; do
    case "$line" in ''|'#'*) continue ;; esac
    line="${line%$'\r'}"
    case "$line" in *'|'*) ;; *) die "$SKIPS_FILE: Eintrag ohne Grund (erwartet '<Test> | <Grund>'): $line" ;; esac
    name="$(printf '%s' "${line%%|*}" | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')"
    reason="$(printf '%s' "${line#*|}" | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')"
    [ -n "$name" ] || die "$SKIPS_FILE: Eintrag ohne Testnamen: $line"
    [ -n "$reason" ] || die "$SKIPS_FILE: Eintrag ohne Grund: $line"
    SKIP_ARGS+=(--skip "$name")
    SKIP_COUNT=$((SKIP_COUNT + 1))
    printf '::warning title=Linux-Skip::%s (%s)\n' "$name" "$reason"
    printf -- '- Skip `%s`: %s\n' "$name" "$reason" >> "$NOTES"
  done < "$SKIPS_FILE"
  log "Skip-Liste: $SKIP_COUNT Einträge (Ziel: 0)"
}

# --- Schritte --------------------------------------------------------------------------------------------------------
step_fmt() {
  log "cargo fmt --all --check"
  cargo fmt --all --check
}

step_clippy() {
  log "cargo clippy (ohne openspider-desktop)"
  cargo clippy --workspace --exclude openspider-desktop --all-targets --locked -- -D warnings
}

# Findet ein Chromium: OPENSPIDER_CHROMIUM, vorinstalliertes Chrome auf dem Runner, sonst Playwright-Chromium.
find_chromium() {
  if [ -n "${OPENSPIDER_CHROMIUM:-}" ]; then
    [ -x "$OPENSPIDER_CHROMIUM" ] || die "OPENSPIDER_CHROMIUM ist nicht ausführbar: $OPENSPIDER_CHROMIUM"
    printf '%s' "$OPENSPIDER_CHROMIUM"
    return
  fi
  local name path
  for name in google-chrome google-chrome-stable chromium chromium-browser; do
    if path="$(command -v "$name" 2> /dev/null)"; then
      printf '%s' "$path"
      return
    fi
  done
  # Fallback: Playwright-Chromium (Version aus dem Projekt, M2.md C3; ohne Linux-Pakete, nur der Browser-Download).
  local pw
  if [ -x node_modules/.bin/playwright ]; then
    pw=(node_modules/.bin/playwright)
  else
    command -v npx > /dev/null 2>&1 || die "Kein Chrome/Chromium gefunden und kein npx für den Playwright-Fallback"
    pw=(npx --yes "playwright@${PLAYWRIGHT_VERSION:?PLAYWRIGHT_VERSION fehlt für den Playwright-Fallback}")
  fi
  log "Kein vorinstalliertes Chrome: installiere Playwright-Chromium" >&2
  "${pw[@]}" install chromium >&2
  path="$(find "${PLAYWRIGHT_BROWSERS_PATH:-$HOME/.cache/ms-playwright}" -type f -path '*chromium-*/chrome-linux*/chrome' 2> /dev/null | sort | tail -n 1)"
  [ -n "$path" ] || die "Playwright-Chromium nach der Installation nicht gefunden"
  printf '%s' "$path"
}

step_chromium() {
  log "Chromium headless (ohne DISPLAY, ohne xvfb)"
  local chrome work page out
  chrome="$(find_chromium)"
  echo "Chromium: $chrome"
  "$chrome" --version || true
  work="$(mktemp -d)"
  page="$work/smoke.html"
  cat > "$page" << 'HTML'
<!doctype html>
<html><head><meta charset="utf-8"><title>smoke</title></head>
<body><p id="t">vor-js</p>
<script>document.getElementById('t').textContent = 'gerendert-' + (6 * 7);</script>
</body></html>
HTML
  local cmd=("$chrome" --headless=new --no-sandbox --disable-gpu --disable-dev-shm-usage --no-first-run
    --user-data-dir="$work/profile" --dump-dom "file://$page")
  if command -v timeout > /dev/null 2>&1; then cmd=(timeout 60 "${cmd[@]}"); fi
  out="$(env -u DISPLAY -u WAYLAND_DISPLAY "${cmd[@]}" 2> "$work/stderr.txt")" || {
    cat "$work/stderr.txt" >&2
    die "Chromium-Smoke fehlgeschlagen: $chrome"
  }
  case "$out" in
    *gerendert-42*) echo "Chromium-Smoke ok: JavaScript lief headless ohne Anzeige" ;;
    *) printf '%s\n' "$out" >&2; die "Chromium-Smoke: DOM enthält das JavaScript-Ergebnis nicht" ;;
  esac
  export OPENSPIDER_CHROMIUM="$chrome"
  if [ -n "${GITHUB_ENV:-}" ]; then
    echo "OPENSPIDER_CHROMIUM=$chrome" >> "$GITHUB_ENV"
  fi
  printf -- '- Chromium: `%s`\n' "$chrome" >> "$NOTES"
  mkdir -p "$OUT_DIR"
  printf '%s' "$chrome" > "$OUT_DIR/chromium.path"
}

# Die Engine warnt unter 2 GiB freiem Speicher und pausiert Crawls unter 1 GiB (CRAWL-124). Läuft die Platte des Runners
# beim Testbau so weit voll, scheitern Dutzende Tests mit irreführenden Befunden (fehlende Zeilen in der Crawl-Datei,
# Zustand Paused statt Completed; Läufe 42646892161, 42677181224). Deshalb vor dem Testlauf messen und klar abbrechen.
step_disk() {
  local min_gib="${LINUX_CORE_MIN_FREE_GIB:-4}"
  case "$min_gib" in '' | *[!0-9]*) die "LINUX_CORE_MIN_FREE_GIB muss eine ganze Zahl (GiB) sein: $min_gib" ;; esac
  log "Freier Speicher (verlangt: mindestens $min_gib GiB vor dem Testlauf, CRAWL-124)"
  local min_kib=$((min_gib * 1024 * 1024)) path kib low=""
  mkdir -p "$CARGO_TARGET"
  for path in "$CARGO_TARGET" "${TMPDIR:-/tmp}"; do
    df -h "$path" | tail -n 1
    kib="$(df -Pk "$path" | awk 'NR == 2 { print $4 }')"
    [ "$kib" -ge "$min_kib" ] || low="$low $path ($((kib / 1024)) MiB frei)"
  done
  du -sh "$CARGO_TARGET" 2> /dev/null || true
  [ -z "$low" ] || die "Zu wenig freier Speicher vor dem Testlauf:$low; verlangt sind mindestens $min_gib GiB. Darunter \
warnt (< 2 GiB) bzw. pausiert (< 1 GiB) der Ressourcenschutz der Engine die Crawls der Tests (CRAWL-124)."
}

step_test() {
  log "cargo test (ohne openspider-desktop), OPENSPIDER_SECRET_STORE=$OPENSPIDER_SECRET_STORE, ohne D-Bus"
  [ -z "${DBUS_SESSION_BUS_ADDRESS:-}" ] || die "DBUS_SESSION_BUS_ADDRESS darf im Testlauf nicht gesetzt sein"
  load_skips
  # Kompilieren getrennt vom Lauf: Laufzeit der reinen Testausführung bleibt sichtbar (Dauer hängt am Cache).
  cargo test --workspace --exclude openspider-desktop --locked --no-run
  step_disk
  cargo test --workspace --exclude openspider-desktop --locked --no-fail-fast -- ${SKIP_ARGS[@]+"${SKIP_ARGS[@]}"}
}

step_release() {
  log "cargo build --release -p os-cli"
  cargo build --release -p os-cli --locked
  mkdir -p "$OUT_DIR"
  cp "$CARGO_TARGET/release/openspider" "$OUT_DIR/openspider"
  chmod +x "$OUT_DIR/openspider"
  "$OUT_DIR/openspider" --version
  (cd "$OUT_DIR" && { sha256sum openspider 2> /dev/null || shasum -a 256 openspider; } | tee openspider.sha256)
  if command -v ldd > /dev/null 2>&1; then
    ldd "$OUT_DIR/openspider" || true
    if ldd "$OUT_DIR/openspider" | grep -Eiq 'libgtk|libwebkit|libX11'; then
      die "Die Release-Binärdatei hängt an Desktop-/Anzeigebibliotheken (Serverbetrieb ohne Anzeige und ohne D-Bus verlangt)"
    fi
  fi
}

step_golden() {
  log "CLI-Golden-Smoke mit der Release-Binärdatei"
  local bin="$ROOT/$OUT_DIR/openspider"
  [ -x "$bin" ] || die "Release-Binärdatei fehlt: $bin (Schritt release zuerst)"
  OPENSPIDER_BIN="$bin" cargo test -p os-cli --test linux_server --locked -- --nocapture
}

run_all() {
  rm -f "$OUT_DIR/chromium.path"
  timed fmt step_fmt
  timed clippy step_clippy
  timed chromium step_chromium
  # Die Subshell des Schritts kann die Umgebung nicht zurückgeben: Pfad aus der Datei übernehmen.
  if [ -f "$OUT_DIR/chromium.path" ]; then OPENSPIDER_CHROMIUM="$(cat "$OUT_DIR/chromium.path")"; export OPENSPIDER_CHROMIUM; fi
  timed test step_test
  timed release step_release
  timed golden step_golden
}

main() {
  if [ "$(uname -s)" != "Linux" ]; then
    echo "Hinweis: Dieses Skript ist für Linux (ubuntu-latest) gedacht; Lauf auf $(uname -s) nur zur Probe." >&2
  fi
  printf '### linux-core\n\n| Schritt | Ergebnis | Dauer |\n|---|---|---|\n' >> "$SUMMARY"
  local steps=("$@")
  [ "${#steps[@]}" -gt 0 ] || steps=(all)
  local s
  for s in "${steps[@]}"; do
    case "$s" in
      all) run_all ;;
      fmt | clippy | chromium | disk | test | release | golden) timed "$s" "step_$s" ;;
      *) die "Unbekannter Schritt: $s (fmt | clippy | chromium | disk | test | release | golden | all)" ;;
    esac
  done
}

main "$@"
