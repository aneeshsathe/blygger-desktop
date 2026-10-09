#!/usr/bin/env bash
# The browser pane's macro runner, end to end, on this Mac (no network):
#
#   scripts/browser-macro-check.sh [snapshot dir]
#
# 1. copies the fixture site (crates/blyg-app/fixtures/browser: a sign-in
#    page, a cookie-gated composer with a ProseMirror-like editor that
#    ignores direct DOM writes, Post appending to #posted; a home feed
#    whose prompt opens a modal composer, Post disabled until there's
#    text) to a mktemp -d folder and serves it on 127.0.0.1 (serve.py:
#    python3, an ephemeral port, /notes redirecting to /home.html);
# 2. runs the app in fake mode with a scratch config and data dir and
#    BLYGGER_DEMO=br-macro: built-in test macros against the fixture, the
#    Preview and Post sheets answering themselves (debug builds only):
#      signed out -> "Sign in …", the fixture's sign-in button, a post
#      (paste: into the editor, read back, Post), the same at once
#      (min-interval), the feed (click the prompt, the modal's editor,
#      Post, the modal gone), /notes redirecting to the feed the pane is
#      already on, a password field (refused), a page that goes to
#      another origin (off-origin); with BLYGGER_MACRO_TRACE=1, so
#      macro.log gets a DOM outline after every step;
# 3. checks the printed macro-* lines and the macro.log (the outlines
#    hold tags, classes and button labels, never the page's text or the
#    post's), then stops the server and the app it started (by PID).
#
# With a snapshot build, it also saves the Preview and Post sheets as
# macro-preview.png and macro-post.png in the snapshot dir:
#   RUSTFLAGS="--cfg blygger_snap" CARGO_TARGET_DIR=target/snap \
#     cargo build -p blyg-app --features gpui-kit/test-support
# Without one (BLYGGER_BIN=target/debug/blygger), no pictures.
#
# The run puts the post's text on the system clipboard for a moment and
# then restores what was there. The web view's data store is ephemeral
# (BLYGGER_BROWSER_EPHEMERAL): the fixture's cookie never reaches the
# pane's persistent store.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN="${BLYGGER_BIN:-$ROOT/target/snap/debug/blygger}"
[ -x "$BIN" ] || { echo "build the binary first (see the header)" >&2; exit 2; }

SCRATCH="$(mktemp -d "${TMPDIR:-/tmp}/blygger-macro.XXXXXX")"
SNAPS="${1:-$SCRATCH/snaps}"
mkdir -p "$SNAPS"
SERVER_PID=""
APP_PID=""
cleanup() {
  [ -n "$APP_PID" ] && kill "$APP_PID" 2>/dev/null || true
  [ -n "$SERVER_PID" ] && kill "$SERVER_PID" 2>/dev/null || true
  rm -rf "$SCRATCH/site" "$SCRATCH/data" "$SCRATCH/config"
}
trap cleanup EXIT

PORT="$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1])')"
mkdir -p "$SCRATCH/site" "$SCRATCH/data" "$SCRATCH/config"
cp "$ROOT"/crates/blyg-app/fixtures/browser/{login.html,notes.html,home.html,pm-lite.js} "$SCRATCH/site/"
# Content blocking off: no lists to compile for a local fixture.
printf 'content-blocking = false\n' > "$SCRATCH/config/config"

python3 -I "$ROOT/crates/blyg-app/fixtures/browser/serve.py" "$PORT" "$SCRATCH/site" >/dev/null 2>&1 &
SERVER_PID=$!
for _ in $(seq 50); do
  curl -fsS "http://127.0.0.1:$PORT/notes.html" >/dev/null 2>&1 && break
  sleep 0.1
done

LOG="$SCRATCH/app.log"
BLYGGER_FAKE=1 BLYGGER_NO_ACTIVATE=1 BLYGGER_NO_UPDATE=1 BLYGGER_BROWSER_EPHEMERAL=1 \
  BLYGGER_CONFIG="$SCRATCH/config/config" BLYGGER_DATA_DIR="$SCRATCH/data" \
  BLYGGER_DEMO=br-macro BLYGGER_DEMO_URL="http://127.0.0.1:$PORT/" BLYGGER_MACRO_TRACE=1 \
  BLYGGER_SNAPSHOT_DIR="$SNAPS" "$BIN" >"$LOG" 2>&1 &
APP_PID=$!
for _ in $(seq 1200); do
  kill -0 "$APP_PID" 2>/dev/null || break
  sleep 0.1
done
if kill -0 "$APP_PID" 2>/dev/null; then
  echo "the app didn't finish in 120 s" >&2
fi
grep -E '^(macro-|snapshot)' "$LOG" || true
MACRO_LOG="$(cat "$SCRATCH/data/extensions/demo/macro.log" 2>/dev/null || true)"
cp "$LOG" "$SNAPS/app.log"

fail=0
check() { # name, pattern (grep -E over the app's output)
  if grep -qE "$2" "$LOG"; then echo "ok: $1"; else echo "FAIL: $1" >&2; fail=1; fi
}
check "signed out stops the run" 'macro-outcome error step=1 do=open reason=signed-out'
check "steps run in order" 'macro-step 4 insert div.pm'
check "the Preview sheet came first" 'macro-sheet preview'
check "the Post sheet read the box back" 'macro-readback differs=false "Tide pools are small oceans'
check "posted" 'macro-posted via=paste "Posted to Fixture Notes"'
check "the fixture holds the post" 'macro-dom-posted .*Tide pools are small oceans.*f\\?/tide-pools'
check "min-interval refuses a second post" 'macro-too-soon wait_s='
check "a password field is refused" 'macro-outcome error step=2 do=insert reason=refused'
check "leaving the origin aborts" 'macro-outcome error step=[0-9]+ do=[a-zA-Z]+ reason=off-origin'
check "the demo finished" 'macro-demo-done'
if [ "$(grep -c 'macro-posted via=' "$LOG")" -ge 3 ] &&
  [ "$(grep -c 'macro-dom-posted .*Tide pools are small oceans' "$LOG")" -ge 3 ]; then
  echo "ok: three posts in the fixture (notes, feed, /notes redirecting to the feed)"
else
  echo "FAIL: expected three posts" >&2; fail=1
fi
case "$MACRO_LOG" in
  *"fixture-feed posted via="*"fixture-redirect posted via="*)
    echo "ok: the feed and the redirect back to it posted" ;;
  *) echo "FAIL: macro.log (feed, redirect): $MACRO_LOG" >&2; fail=1 ;;
esac
case "$MACRO_LOG" in
  *"fixture-feed trace step=3 do=click ok"*'.ProseMirror ce="true"'*'label="Post"'*)
    echo "ok: the trace outlines the modal composer" ;;
  *) echo "FAIL: macro.log has no trace outline: $MACRO_LOG" >&2; fail=1 ;;
esac
case "$MACRO_LOG" in
  *"Nothing here leaves"*|*"Fixture Writer"*|*"A test page"*)
    echo "FAIL: an outline holds the page's text" >&2; fail=1 ;;
  *) echo "ok: the outlines never hold the page's text" ;;
esac
case "$MACRO_LOG" in
  *"fixture-note posted via=paste"*"fixture-password error step=2"*)
    echo "ok: macro.log has the outcomes" ;;
  *) echo "FAIL: macro.log: $MACRO_LOG" >&2; fail=1 ;;
esac
case "$MACRO_LOG" in
  *"Tide pools"*) echo "FAIL: macro.log holds the text" >&2; fail=1 ;;
  *) echo "ok: macro.log never holds the text" ;;
esac
ls "$SNAPS"/macro-*.png 2>/dev/null || echo "(no snapshots: not a snapshot build)"
exit "$fail"
