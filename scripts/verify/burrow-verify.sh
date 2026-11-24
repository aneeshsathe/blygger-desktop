#!/usr/bin/env bash
# shellcheck disable=SC2153 # PID, BIN_STARTED, SCENE come from run.env (load_run)
#
# Run Burrow's real app on sample data, play a demo scene, and capture its
# own window as evidence. macOS only. See skills/verify-burrow/SKILL.md.
#
#   scripts/verify/burrow-verify.sh scenes                 list demo scenes
#   scripts/verify/burrow-verify.sh launch <scene> [VAR=value ...]
#                                                          start; prints the run dir
#   scripts/verify/burrow-verify.sh doctor <run dir>       read-only health check
#   scripts/verify/burrow-verify.sh shot <run dir> [name]  capture the app's window(s)
#   scripts/verify/burrow-verify.sh stop <run dir>         stop it (by PID), drop its data
#   scripts/verify/burrow-verify.sh run <scene> [VAR=value ...]
#                                                          launch, doctor, shot, stop
#   scripts/verify/burrow-verify.sh selftest               the script's own checks
#
# Safety, always: fake mode (sample data, an in-memory token store, never a
# real blyg), a throwaway config and data dir inside the run dir, a debug
# build only (never /Applications/Burrow.app), no activation (the app never
# takes focus), no update checks or downloads. Nothing is ever sent to the
# OS as keystrokes or clicks: a scene drives the app from inside
# (BLYGGER_DEMO). Only the PID this script started is ever signalled, and
# only the windows that PID owns are captured.
#
# Evidence, always: no launch or shot while the screen is locked
# (IOConsoleLocked), and a shot that is one flat colour is renamed
# *.BLANK.png and fails the shot (exit 2).
#
# Environment:
#   BLYGGER_BIN        the binary (default: $CARGO_TARGET_DIR or ./target,
#                      then debug/blygger)
#   BURROW_VERIFY_OUT  where run dirs (the evidence) go
#                      (default: <repo>/target/verify)
#   BURROW_VERIFY_WAIT seconds to wait for the scene's ready line (default 120)
#   BURROW_VERIFY_HOLD seconds to wait after it, for a scene's last
#                      animation (default 1.5)
#   BURROW_VERIFY_CONFIG a config file to start from (copied into the run;
#                      only *.example.com / localhost URLs allowed)
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
HERE="$ROOT/scripts/verify"
OUT="${BURROW_VERIFY_OUT:-$ROOT/target/verify}"
BIN="${BLYGGER_BIN:-${CARGO_TARGET_DIR:-$ROOT/target}/debug/blygger}"
WAIT="${BURROW_VERIFY_WAIT:-120}"
HOLD="${BURROW_VERIFY_HOLD:-1.5}"

die() {
  echo "burrow-verify: $*" >&2
  exit 2
}

# The demo scenes the app knows: its registry (crates/blyg-app/src/scenes.rs),
# printed by the binary itself, so this list and the app can't disagree.
scenes() {
  check_bin
  env -u BLYGGER_DEMO "$BIN" +scenes
}

# Refuse anything but a debug build of this repo's app.
check_bin() {
  [[ -x "$BIN" ]] || die "no binary at $BIN (cargo build -p blyg-app, or set BLYGGER_BIN)"
  case "$BIN" in
    /Applications/* | */release/*) die "refusing $BIN: debug builds only" ;;
    */debug/blygger) ;;
    *) die "refusing $BIN: expected a .../debug/blygger build" ;;
  esac
}

# Variables a caller may not override: they keep the run off real data.
check_assignment() {
  [[ "$1" == *=* ]] || die "expected VAR=value, got '$1'"
  case "${1%%=*}" in
    BLYGGER_FAKE | BLYGGER_DATA_DIR | BLYGGER_CONFIG | BLYGGER_EXTENSIONS_DIR | BLYGGER_TEST_TOKEN | BLYGGER_DEMO | HOME)
      die "refusing to override ${1%%=*}" ;;
  esac
}

# A config for the run may only name example or local hosts.
check_config() {
  [[ -f "$1" ]] || die "no config file $1"
  local host
  while read -r host; do
    case "$host" in
      '' | example.com | *.example.com | localhost | 127.0.0.1) ;;
      *) die "refusing config naming $host: use *.example.com" ;;
    esac
  done < <(grep -oE 'https?://[^/ "]+' "$1" | sed -E 's#^https?://##; s#:[0-9]+$##' | sort -u)
}

# A helper compiled once from scripts/verify/<name>.swift (cached in $OUT).
swift_bin() {
  local cache="$OUT/.$1"
  if [[ ! -x "$cache" || "$HERE/$1.swift" -nt "$cache" ]]; then
    mkdir -p "$OUT"
    swiftc -O -o "$cache" "$HERE/$1.swift" 2>/dev/null || die "swiftc failed on $1.swift"
  fi
  echo "$cache"
}
winid_bin() { swift_bin winid; }

# Is the console locked? Reads `ioreg -n Root -d1 -a` (a plist) on stdin.
# awk reads all of it (no early exit, so pipefail sees no SIGPIPE).
console_locked() {
  awk '/<key>IOConsoleLocked<\/key>/ { getline; if ($0 ~ /<true\/>/) locked = 1 }
    END { exit !locked }'
}

# A locked Mac still hands screencapture a picture of the window, but not of
# the screen a user sees: the 0.28 user-testing pass took its shots that way
# and missed obvious layout bugs. So no shot (and no launch) while locked.
check_unlocked() {
  if ioreg -n Root -d1 -a 2>/dev/null | console_locked; then
    die "refusing: the screen is locked (IOConsoleLocked), so a shot shows nothing a user would see; unlock the Mac and run again"
  fi
}

# A flat, one-colour shot (a window that drew nothing) is no evidence.
check_not_blank() {
  "$(swift_bin shotcheck)" "$1" >/dev/null
}

# Sets PID, BIN_STARTED, SCENE, STARTED, EXTRA (written by launch).
load_run() {
  local run="${1:-}"
  [[ -n "$run" && -f "$run/run.env" ]] || die "not a run dir: '$run'"
  # shellcheck disable=SC1091,SC2153
  source "$run/run.env"
}

alive() { kill -0 "$1" 2>/dev/null; }

# Is $PID still the binary this script started (not a recycled PID)?
ours() {
  alive "$PID" && [[ "$(ps -p "$PID" -o comm= 2>/dev/null)" == "$BIN_STARTED" ]]
}

launch() {
  local scene="${1:-}"
  [[ -n "$scene" ]] || die "usage: launch <scene> [VAR=value ...]"
  shift
  [[ "$(uname)" == Darwin ]] || die "macOS only (window capture uses screencapture)"
  check_bin
  check_unlocked
  scenes | grep -qx -- "$scene" || die "unknown scene '$scene' (see: $0 scenes)"
  local a
  for a in "$@"; do check_assignment "$a"; done
  local run
  run="$OUT/$(date +%Y%m%d-%H%M%S)-$scene"
  mkdir -p "$run/data" "$run/config" "$run/shots"
  if [[ -n "${BURROW_VERIFY_CONFIG:-}" ]]; then
    check_config "$BURROW_VERIFY_CONFIG"
    cp "$BURROW_VERIFY_CONFIG" "$run/config/config"
  else
    : >"$run/config/config"
  fi
  env -u BLYGGER_TEST_TOKEN \
    BLYGGER_FAKE=1 \
    BLYGGER_NO_ACTIVATE=1 \
    BLYGGER_NO_UPDATE=1 \
    BLYGGER_NO_BLOCKLIST_DOWNLOAD=1 \
    BLYGGER_BROWSER_EPHEMERAL=1 \
    BLYGGER_DATA_DIR="$run/data" \
    BLYGGER_CONFIG="$run/config/config" \
    BLYGGER_EXTENSIONS_DIR="$run/config/extensions" \
    BLYGGER_DEMO="$scene" \
    "$@" \
    "$BIN" >"$run/app.log" 2>&1 &
  local pid=$!
  {
    echo "PID=$pid"
    echo "BIN_STARTED=$(printf %q "$BIN")"
    echo "SCENE=$scene"
    echo "STARTED=$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    printf 'EXTRA=%q\n' "$*"
  } >"$run/run.env"
  # Readiness: the scene has played every step (demo.rs prints the line),
  # and the app owns an on-screen window.
  local deadline=$((SECONDS + WAIT))
  until grep -q "blygger: demo $scene ready" "$run/app.log"; do
    alive "$pid" || { tail -20 "$run/app.log" >&2; die "the app exited before '$scene' was ready ($run)"; }
    ((SECONDS < deadline)) || { stop "$run" >/dev/null; die "'$scene' not ready after ${WAIT}s ($run/app.log)"; }
    sleep 0.25
  done
  local w
  w="$(winid_bin)"
  until [[ -n "$("$w" "$pid")" ]]; do
    ((SECONDS < deadline)) || { stop "$run" >/dev/null; die "no window after ${WAIT}s"; }
    sleep 0.25
  done
  sleep "$HOLD"
  echo "$run"
}

doctor() {
  load_run "${1:-}"
  local run="$1" ok=1
  report() { printf '%-8s %s\n' "$1" "$2"; [[ "$1" == ok ]] || ok=0; }
  if ours; then report ok "pid $PID is $BIN_STARTED"; else report FAIL "pid $PID isn't running $BIN_STARTED"; fi
  case "$BIN_STARTED" in
    */debug/blygger) report ok "debug build" ;;
    *) report FAIL "not a debug build" ;;
  esac
  if grep -q "blygger: demo $SCENE ready" "$run/app.log"; then report ok "scene $SCENE played"; else report FAIL "scene $SCENE not ready"; fi
  if grep -q "panicked" "$run/app.log"; then report FAIL "a panic in app.log"; else report ok "no panic in app.log"; fi
  local ids
  ids="$("$(winid_bin)" "$PID" | tr '\n' ' ')"
  if [[ -n "$ids" ]]; then report ok "windows: $ids"; else report FAIL "no on-screen window"; fi
  if [[ -d "$run/data" ]] && [[ "$(cd "$run/data" && pwd)" == "$run/data" ]]; then
    report ok "data dir $run/data (throwaway)"
  else
    report FAIL "data dir missing"
  fi
  ((ok)) || exit 1
}

shot() {
  load_run "${1:-}"
  local run="$1" name="${2:-$SCENE}" id n=0 blank=0 png
  ours || die "pid $PID isn't the app this run started"
  check_unlocked
  for id in $("$(winid_bin)" "$PID"); do
    png="$run/shots/$name-$id.png"
    screencapture -x -o -l "$id" "$png"
    # Locked while capturing: this shot can't be trusted either.
    check_unlocked
    if check_not_blank "$png"; then
      echo "$png"
    else
      # Renamed, so nobody reads it as the screen.
      mv "$png" "${png%.png}.BLANK.png"
      echo "burrow-verify: BLANK SHOT: window $id of '$SCENE' is one flat colour (${png%.png}.BLANK.png); it shows nothing, so this scene FAILS" >&2
      blank=$((blank + 1))
    fi
    n=$((n + 1))
  done
  ((n > 0)) || die "no window to capture"
  ((blank == 0)) || die "$blank of $n shots blank: not evidence of anything"
}

stop() {
  load_run "${1:-}"
  local run="$1"
  if ours; then
    kill "$PID"
    local deadline=$((SECONDS + 10))
    while alive "$PID" && ((SECONDS < deadline)); do sleep 0.2; done
    alive "$PID" && kill -9 "$PID"
  fi
  # The throwaway data goes; the evidence (app.log, shots/, run.env) stays.
  rm -rf "$run/data" "$run/config"
  echo "stopped; evidence in $run"
}

run_all() {
  local run status=0
  run="$(launch "$@")"
  doctor "$run" || status=$?
  shot "$run" || status=$?
  stop "$run"
  return "$status"
}

selftest() {
  local fails=0
  expect_fail() {
    if (eval "$1") >/dev/null 2>&1; then
      echo "FAIL: '$1' was allowed"
      fails=$((fails + 1))
    else echo "ok: refused $1"; fi
  }
  local n
  n="$(scenes | wc -l | tr -d ' ')"
  if ((n > 50)) && scenes | grep -qx rd-stream; then
    echo "ok: $n scenes, rd-stream among them"
  else
    echo "FAIL: scene list ($n)"
    fails=$((fails + 1))
  fi
  expect_fail 'BIN=/Applications/Burrow.app/Contents/MacOS/blygger check_bin'
  expect_fail 'BIN=/bin/sh check_bin'
  expect_fail 'check_assignment BLYGGER_DATA_DIR=/tmp/x'
  expect_fail 'check_assignment BLYGGER_TEST_TOKEN=x'
  expect_fail 'check_assignment HOME=/tmp'
  expect_fail 'check_assignment nonsense'
  expect_fail 'launch no-such-scene'
  expect_fail 'load_run /nonexistent'
  local cfg
  cfg="$(mktemp)"
  echo "blyg-url = https://my.real-blog.net" >"$cfg"
  expect_fail "check_config $cfg"
  echo "blyg = https://a.blyg.example.com" >"$cfg"
  if (check_config "$cfg") 2>/dev/null; then echo "ok: allowed an example.com config"; else
    echo "FAIL: example.com config refused"
    fails=$((fails + 1))
  fi
  rm -f "$cfg"
  # The lock refusal reads ioreg's plist; feed it both answers.
  local plist='<dict>
	<key>IOConsoleLocked</key>
	<%s/>
	<key>IOConsoleUsers</key>
</dict>'
  # shellcheck disable=SC2059 # the format is ours
  if printf "$plist" true | console_locked; then echo "ok: refused a locked console"; else
    echo "FAIL: a locked console was allowed"
    fails=$((fails + 1))
  fi
  # shellcheck disable=SC2059
  if printf "$plist" false | console_locked; then
    echo "FAIL: an unlocked console was refused"
    fails=$((fails + 1))
  else echo "ok: allowed an unlocked console"; fi
  # The blank-shot check: a flat image fails, a drawn one passes.
  local sc dir
  sc="$(swift_bin shotcheck)"
  dir="$(mktemp -d)"
  "$sc" --make flat "$dir/flat.png"
  "$sc" --make drawn "$dir/drawn.png"
  if (check_not_blank "$dir/flat.png") 2>/dev/null; then
    echo "FAIL: a blank shot was allowed"
    fails=$((fails + 1))
  else echo "ok: refused a blank shot"; fi
  if (check_not_blank "$dir/drawn.png") 2>/dev/null; then echo "ok: allowed a drawn shot"; else
    echo "FAIL: a drawn shot was called blank"
    fails=$((fails + 1))
  fi
  rm -rf "$dir"
  if (check_assignment BLYGGER_THEME=paper) 2>/dev/null; then echo "ok: allowed BLYGGER_THEME"; else
    echo "FAIL: BLYGGER_THEME refused"
    fails=$((fails + 1))
  fi
  ((fails == 0)) || exit 1
}

cmd="${1:-}"
shift || true
case "$cmd" in
  scenes) scenes ;;
  launch) launch "$@" ;;
  doctor) doctor "$@" ;;
  shot) shot "$@" ;;
  stop) stop "$@" ;;
  run) run_all "$@" ;;
  selftest) selftest ;;
  *) sed -n '4,16p' "$0" >&2; exit 2 ;;
esac
