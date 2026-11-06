#!/usr/bin/env bash
# Prüft eine signierte und notarisierte macOS-App (.app) oder ein DMG (M2-Z01).
#
# Aufruf:  scripts/release/verify-macos.sh [--no-notarization] <OpenSpider.app | OpenSpider.dmg>
#
# App:  codesign --verify --deep --strict, Hardened-Runtime-Flag, spctl -a -t exec (Quelle "Notarized Developer ID"),
#       xcrun stapler validate.
# DMG:  codesign --verify --strict, spctl -a -t open --context context:primary-signature,
#       xcrun stapler validate; zusätzlich wird das DMG schreibgeschützt eingehängt und die darin liegende App
#       wie oben geprüft.
#
# --no-notarization  für lokale Builds ohne Notarisierung: Gatekeeper-Quelle und Heftung werden nur gemeldet,
#                    nicht als Fehler gewertet (Signatur und Hardened Runtime bleiben Pflicht).
#
# Alle Prüfungen laufen durch; die Ausgabe ist das Prüfprotokoll (im Workflow per tee ins Job-Log und Artefakt).
# Exitcode 0 nur, wenn jede Pflichtprüfung bestanden ist. Gibt keine Geheimnisse aus, liest keine.

set -uo pipefail

require_notarized=1
if [[ "${1:-}" == "--no-notarization" ]]; then
  require_notarized=0
  shift
fi
if [[ $# -ne 1 ]]; then
  echo "Aufruf: $0 [--no-notarization] <App oder DMG>" >&2
  exit 2
fi
target="${1%/}"
if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "Dieses Skript läuft nur unter macOS." >&2
  exit 2
fi
if [[ ! -e "$target" ]]; then
  echo "Nicht gefunden: $target" >&2
  exit 2
fi

failures=0
mount_point=""

cleanup() {
  if [[ -n "$mount_point" ]]; then
    hdiutil detach -quiet "$mount_point" >/dev/null 2>&1 || hdiutil detach -quiet -force "$mount_point" >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT

# run <Titel> <Pflicht 1|0> <Befehl ...>: führt aus, gibt die Ausgabe ins Protokoll, zählt Pflichtfehler.
# Die Ausgabe des letzten Befehls bleibt in $last_output (für Folgeprüfungen).
last_output=""
# stapler validate fragt Apples Ticket-Dienst ab; kurze Netzwerkaussetzer auf CI-Runnern (Error 68,
# "network connection was lost", "CloudKit's response is inconsistent") sollen den Lauf nicht rot machen –
# bis zu 5 Versuche im Abstand von 60 s wie beim Anheften (Trockenlauf beta.3 42207909586).
stapler_validate() {
  local target="$1" i
  for i in 1 2 3 4 5; do
    if xcrun stapler validate "$target"; then return 0; fi
    [ "$i" -lt 5 ] && { echo "stapler validate fehlgeschlagen (Versuch $i/5), neuer Versuch in 60 s" >&2; sleep 60; }
  done
  return 1
}

run() {
  local title="$1" required="$2"
  shift 2
  echo
  echo "### $title"
  echo "\$ $*"
  last_output="$("$@" 2>&1)"
  local rc=$?
  [[ -n "$last_output" ]] && echo "$last_output"
  if [[ $rc -eq 0 ]]; then
    echo "-> OK"
  elif [[ "$required" == "1" ]]; then
    echo "-> FEHLER (Exitcode $rc)"
    failures=$((failures + 1))
  else
    echo "-> nicht bestanden (Exitcode $rc), nicht als Fehler gewertet"
  fi
  return $rc
}

check_flags() {
  # Hardened Runtime steht als "runtime" in den Flags der Signatur (flags=0x10000(runtime)).
  local path="$1"
  echo
  echo "### Hardened Runtime"
  echo "\$ codesign -dvv $path"
  local info
  info="$(codesign -dvv "$path" 2>&1)"
  echo "$info" | grep -E '^(Identifier|Authority|TeamIdentifier|Timestamp|Runtime Version|CodeDirectory)' || true
  if echo "$info" | grep -Eq 'flags=0x[0-9a-f]+\([^)]*runtime'; then
    echo "-> OK (Flag runtime gesetzt)"
  else
    echo "-> FEHLER: Hardened Runtime nicht aktiv"
    failures=$((failures + 1))
  fi
}

verify_app() {
  local app="$1"
  echo
  echo "=== App: $app ==="
  if [[ -f "$app/Contents/MacOS/$(basename "$app" .app)" ]]; then
    echo "Architekturen: $(lipo -archs "$app/Contents/MacOS/$(basename "$app" .app)" 2>&1)"
  fi
  run "Signatur (codesign --verify --deep --strict)" 1 codesign --verify --deep --strict --verbose=2 "$app"
  check_flags "$app"
  run "Bundle: alle Mach-O-Dateien, Mindestplattform, Dateizuordnungen" 1 bash "$(dirname "${BASH_SOURCE[0]}")/check-macos-bundle.sh" "$app"
  if run "Gatekeeper (spctl -a -t exec)" "$require_notarized" spctl -a -t exec -vv "$app"; then
    if [[ "$require_notarized" == "1" ]] && ! grep -q 'source=Notarized Developer ID' <<<"$last_output"; then
      echo "-> FEHLER: Quelle ist nicht 'Notarized Developer ID'"
      failures=$((failures + 1))
    fi
  fi
  run "Notarisierungsticket (stapler validate)" "$require_notarized" stapler_validate "$app"
}

verify_dmg() {
  local dmg="$1"
  echo
  echo "=== DMG: $dmg ==="
  run "Signatur (codesign --verify --strict)" 1 codesign --verify --strict --verbose=2 "$dmg"
  # Hardened Runtime gilt für ausführbaren Code, nicht für das DMG selbst; geprüft wird sie an der App darin.
  echo
  echo "### Signaturdetails"
  codesign -dvv "$dmg" 2>&1 | grep -E '^(Identifier|Authority|TeamIdentifier|Timestamp)' || true
  if run "Gatekeeper (spctl -a -t open)" "$require_notarized" spctl -a -t open --context context:primary-signature -vv "$dmg"; then
    if [[ "$require_notarized" == "1" ]] && ! grep -q 'source=Notarized Developer ID' <<<"$last_output"; then
      echo "-> FEHLER: Quelle ist nicht 'Notarized Developer ID'"
      failures=$((failures + 1))
    fi
  fi
  run "Notarisierungsticket (stapler validate)" "$require_notarized" stapler_validate "$dmg"

  mount_point="$(mktemp -d "${TMPDIR:-/tmp}/openspider-dmg.XXXXXX")"
  echo
  echo "### DMG einhängen (schreibgeschützt)"
  if hdiutil attach -quiet -nobrowse -readonly -mountpoint "$mount_point" "$dmg" >/dev/null 2>&1; then
    local app
    app="$(find "$mount_point" -maxdepth 2 -name '*.app' -print -quit)"
    if [[ -n "$app" ]]; then
      verify_app "$app"
    else
      echo "-> FEHLER: keine .app im DMG gefunden"
      failures=$((failures + 1))
    fi
  else
    echo "-> FEHLER: DMG lässt sich nicht einhängen"
    failures=$((failures + 1))
    mount_point=""
  fi
}

echo "OpenSpider macOS-Prüfprotokoll"
echo "Datum:  $(date -u +%Y-%m-%dT%H:%M:%SZ)"
echo "Ziel:   $target"
echo "Modus:  $([[ "$require_notarized" == "1" ]] && echo 'signiert und notarisiert' || echo 'nur signiert (--no-notarization)')"

case "$target" in
  *.app) verify_app "$target" ;;
  *.dmg) verify_dmg "$target" ;;
  *)
    echo "Unbekannter Typ (erwartet .app oder .dmg): $target" >&2
    exit 2
    ;;
esac

echo
if [[ $failures -eq 0 ]]; then
  echo "ERGEBNIS: BESTANDEN"
  exit 0
fi
echo "ERGEBNIS: DURCHGEFALLEN ($failures Pflichtprüfung(en) fehlgeschlagen)"
exit 1
