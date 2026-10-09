#!/usr/bin/env bash
# A real extension's browser macro, end to end, on this Mac (no network):
#
#   scripts/browser-macro-ext-check.sh [snapshot dir]
#
# Like browser-macro-check.sh, but the macro comes from an installed
# extension, through the ⇧⌘P palette:
#
# 1. serves the fixture site (crates/blyg-app/fixtures/browser) from a
#    mktemp -d folder on 127.0.0.1 (serve.py: python3, an ephemeral port);
# 2. writes a fixture extension into a temporary extensions folder: its
#    manifest declares browser.automate:http://127.0.0.1:<port>, one site
#    (the fixture) and one macro shaped like the bundled cross-post's (the
#    home feed's prompt opens a modal composer; Post is disabled until
#    there's text; the modal closes), and its program is this binary's bundled
#    cross-post (`blygger +ext cross-post`), so `extension/macro.prepare`
#    is the real one (with a template setting, to show it ran);
# 3. runs the app in fake mode with a scratch config enabling and granting
#    it, and BLYGGER_DEMO=br-macro-ext (debug builds only): sign in with
#    the fixture's button, open a published post, choose the macro's ⇧⌘P
#    row; macro.prepare shapes the text; the Preview and Post sheets
#    answer themselves; Post;
# 4. checks the printed lines (#posted holds the prepared text) and the
#    macro.log, then stops the server and the app it started (by PID).
#
# Build first (debug): cargo build -p blyg-app   (BLYGGER_BIN to override).
# With a snapshot build (see browser-macro-check.sh) it also saves the
# Preview and Post sheets in the snapshot dir.
#
# The run puts the post's text on the system clipboard for a moment and
# then restores what was there. The web view's data store is ephemeral
# (BLYGGER_BROWSER_EPHEMERAL).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TARGET="${CARGO_TARGET_DIR:-$ROOT/target}"
BIN="${BLYGGER_BIN:-$TARGET/debug/blygger}"
[ -x "$BIN" ] || { echo "build the binary first (see the header)" >&2; exit 2; }

SCRATCH="$(mktemp -d "${TMPDIR:-/tmp}/blygger-macro-ext.XXXXXX")"
SNAPS="${1:-$SCRATCH/snaps}"
mkdir -p "$SNAPS"
SERVER_PID=""
APP_PID=""
cleanup() {
  [ -n "$APP_PID" ] && kill "$APP_PID" 2>/dev/null || true
  [ -n "$SERVER_PID" ] && kill "$SERVER_PID" 2>/dev/null || true
  rm -rf "$SCRATCH/site" "$SCRATCH/data" "$SCRATCH/config" "$SCRATCH/extensions"
}
trap cleanup EXIT

PORT="$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1])')"
BASE="http://127.0.0.1:$PORT"
mkdir -p "$SCRATCH/site" "$SCRATCH/data" "$SCRATCH/config" "$SCRATCH/extensions/fixture-notes"
cp "$ROOT"/crates/blyg-app/fixtures/browser/{login.html,notes.html,home.html,pm-lite.js} "$SCRATCH/site/"

# The fixture extension. Its macro id is cross-post's, so the bundled
# program's macro.prepare answers it.
PM="[role='dialog'] div.ProseMirror[contenteditable='true'], [role='dialog'] [contenteditable='true'], [aria-modal='true'] [contenteditable='true']"
PROMPT="[role='button'], button, div"
cat > "$SCRATCH/extensions/fixture-notes/extension.toml" <<EOF
name = "fixture-notes"
version = "0.0.1"
protocol = 1
description = "Test only: cross-posts to the local fixture site."
command = ["$BIN", "+ext", "cross-post"]
capabilities = ["items.read", "ui", "browser.automate:$BASE"]

[[sites]]
id = "fixture"
title = "Fixture Notes"
origin = "$BASE"
home = "$BASE/login.html"
signed-out = "a.sign-in"
content-blocking = false

[[macros]]
id = "cross-post-note"
title = "Cross-post to Fixture Notes…"
site = "fixture"
tested = "the fixture, every run"
steps = [
  { do = "open", url = "$BASE/home.html" },
  { do = "waitFor", selector = "$PROMPT", text = "What's on your mind?", timeout = "10s" },
  { do = "click", selector = "$PROMPT", text = "What's on your mind?" },
  { do = "waitFor", selector = "$PM", timeout = "10s" },
  { do = "focus", selector = "$PM" },
  { do = "insert", selector = "$PM" },
  { do = "submit", selector = "[role='dialog'] button, [aria-modal='true'] button", text = "Post" },
  { do = "waitFor", selector = "$PM", absent = true, timeout = "10s" },
  { do = "done", text = "Posted to Fixture Notes" },
]
EOF

# Content blocking off: no lists to compile for a local fixture.
cat > "$SCRATCH/config/config" <<EOF
content-blocking = false
extension = fixture-notes
extension-allow = fixture-notes items.read
extension-allow = fixture-notes ui
extension-allow = fixture-notes browser.automate:$BASE
extension-setting = fixture-notes template={{excerpt}} (via {{permalink}})
EOF

python3 -I "$ROOT/crates/blyg-app/fixtures/browser/serve.py" "$PORT" "$SCRATCH/site" >/dev/null 2>&1 &
SERVER_PID=$!
for _ in $(seq 50); do
  curl -fsS "$BASE/notes.html" >/dev/null 2>&1 && break
  sleep 0.1
done

LOG="$SCRATCH/app.log"
BLYGGER_FAKE=1 BLYGGER_NO_ACTIVATE=1 BLYGGER_NO_UPDATE=1 BLYGGER_BROWSER_EPHEMERAL=1 \
  BLYGGER_CONFIG="$SCRATCH/config/config" BLYGGER_DATA_DIR="$SCRATCH/data" \
  BLYGGER_EXTENSIONS_DIR="$SCRATCH/extensions" \
  BLYGGER_DEMO=br-macro-ext BLYGGER_DEMO_URL="$BASE/" \
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
MACRO_LOG="$(cat "$SCRATCH/data/extensions/fixture-notes/macro.log" 2>/dev/null || true)"
cp "$LOG" "$SNAPS/app.log"

fail=0
check() { # name, pattern (grep -E over the app's output)
  if grep -qE "$2" "$LOG"; then echo "ok: $1"; else echo "FAIL: $1" >&2; fail=1; fi
}
check "the palette offered the extension's macro" '^macro-ext-row "Cross-post to Fixture Notes… ↗ Fixture Notes"'
check "the Preview sheet came first" 'macro-sheet preview'
check "the Post sheet read the box back" 'macro-readback differs=false ".*\(via https://blyg\.example\.com/'
check "posted" 'macro-posted via=[a-z-]+ "Posted to Fixture Notes"'
check "the fixture holds the prepared text" 'macro-dom-posted .*\(via https:\\?/\\?/blyg\.example\.com'
check "the demo finished" 'macro-demo-done'
case "$MACRO_LOG" in
  *"cross-post-note posted via="*) echo "ok: macro.log has the outcome" ;;
  *) echo "FAIL: macro.log: $MACRO_LOG" >&2; fail=1 ;;
esac
# +list-extensions shows the last run.
LIST="$(BLYGGER_CONFIG="$SCRATCH/config/config" BLYGGER_DATA_DIR="$SCRATCH/data" \
  BLYGGER_EXTENSIONS_DIR="$SCRATCH/extensions" "$BIN" +list-extensions)"
case "$LIST" in
  *"last run: "*" posted via="*) echo "ok: +list-extensions shows the last run" ;;
  *) echo "FAIL: +list-extensions: $LIST" >&2; fail=1 ;;
esac
ls "$SNAPS"/macro-*.png 2>/dev/null || echo "(no snapshots: not a snapshot build)"
exit "$fail"
