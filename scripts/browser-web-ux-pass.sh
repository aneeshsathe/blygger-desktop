#!/usr/bin/env bash
# A usability pass over Burrow web (blyg-web) in headless Chromium, against a
# REAL local studio with invented data (never a mock, never a real blyg):
#
#   BLYG_WORKER_DIR=/path/to/blygger-studio UX_SHOTS=/some/dir \
#     scripts/browser-web-ux-pass.sh
#
# It walks the personas of skills/user-testing-pass (first week, daily
# reader, writer, keyboard-only, phone thumb user) at phone (390×844) and
# desktop (1440×900) sizes, takes a screenshot at every step, and writes
# what a script can notice by itself to $UX_SHOTS/observations.json:
# failed or refused requests, page and console errors, text wider than its
# box, controls that do nothing, small tap targets on the phone and slow
# steps. A person (or an agent) still looks at every screenshot: the
# script finds where to look, not what's pleasant.
#
# BLYG_WORKER_DIR is a built studio (`npm ci && npm run build` in a scratch
# copy). Playwright comes from $PLAYWRIGHT_DIR, else the studio's
# node_modules; Chromium must be installed for it. Steps 1–3 and 5 are
# scripts/browser-web-live-check.sh's: build dist (BLYG_WEB_NO_BUILD=1
# keeps the one there), two `wrangler dev --local` studios through
# scripts/e2e-wrangler.sh, dev-proxy.py in front of `a`, and only the PIDs
# started here are stopped. Re-run it after fixes and compare the shots.
#
# A third studio, `m`, runs path-mounted (MOUNT=/notes, as a blyg on a
# shared domain is), behind its own dev-proxy serving the app at
# /notes/burrow/: the "mount" personas sign in, seed and use it there.
set -euo pipefail

die() { printf 'web-ux-pass: %s\n' "$*" >&2; exit 1; }
: "${BLYG_WORKER_DIR:?set BLYG_WORKER_DIR to a built blygger-studio directory}"
: "${UX_SHOTS:?set UX_SHOTS to the directory for screenshots and observations}"
[ -d "$BLYG_WORKER_DIR" ] || die "no such directory: $BLYG_WORKER_DIR"
BLYG_WORKER_DIR="$(cd "$BLYG_WORKER_DIR" && pwd)"
export BLYG_WORKER_DIR
mkdir -p "$UX_SHOTS"
UX_SHOTS="$(cd "$UX_SHOTS" && pwd)"

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CTL="$ROOT/scripts/e2e-wrangler.sh"
BLYG_E2E_SCRATCH="$(mktemp -d "${TMPDIR:-/tmp}/burrow-web-ux.XXXXXX")"
export BLYG_E2E_SCRATCH
PROXY_PID=""
PROXY_M_PID=""
MOUNT=/notes

cleanup() {
  [ -n "$PROXY_PID" ] && kill "$PROXY_PID" 2>/dev/null || true
  [ -n "$PROXY_M_PID" ] && kill "$PROXY_M_PID" 2>/dev/null || true
  "$CTL" stop-all || true
  if [ -z "${BLYG_E2E_KEEP:-}" ]; then
    rm -rf "$BLYG_E2E_SCRATCH"
  else
    printf 'web-ux-pass: kept %s\n' "$BLYG_E2E_SCRATCH" >&2
  fi
}
trap cleanup EXIT INT TERM

PW="${PLAYWRIGHT_DIR:-}"
if [ -z "$PW" ] && [ -d "$BLYG_WORKER_DIR/node_modules/playwright" ]; then
  PW="$BLYG_WORKER_DIR/node_modules"
fi
[ -n "$PW" ] && [ -d "$PW/playwright" ] || die "no playwright (set PLAYWRIGHT_DIR)"

if [ -z "${BLYG_WEB_NO_BUILD:-}" ]; then
  "$ROOT/crates/blyg-web/serve.sh" --build-only
fi
[ -f "$ROOT/crates/blyg-web/dist/index.html" ] || die "no crates/blyg-web/dist (build it first)"

"$CTL" setup
PORT_A="$("$CTL" free-port)"
PORT_B="$("$CTL" free-port)"
[ "$PORT_A" != "$PORT_B" ] || PORT_B="$("$CTL" free-port)"
printf 'web-ux-pass: starting the studio on :%s and :%s…\n' "$PORT_A" "$PORT_B" >&2
PORT_M="$("$CTL" free-port)"
"$CTL" start a "$PORT_A"
"$CTL" start b "$PORT_B"
"$CTL" start m "$PORT_M" "MOUNT=$MOUNT"
umask 077
"$CTL" mint-token "$PORT_A" >"$BLYG_E2E_SCRATCH/token-a"
"$CTL" mint-token "$PORT_B" >"$BLYG_E2E_SCRATCH/token-b"
"$CTL" password >"$BLYG_E2E_SCRATCH/password"
[ -s "$BLYG_E2E_SCRATCH/token-a" ] && [ -s "$BLYG_E2E_SCRATCH/token-b" ] || die "this studio mints no tokens (too old)"

PORT_P="$("$CTL" free-port)"
python3 "$ROOT/crates/blyg-web/dev-proxy.py" --port "$PORT_P" --upstream "127.0.0.1:$PORT_A" \
  >"$BLYG_E2E_SCRATCH/proxy.log" 2>&1 &
PROXY_PID=$!
for _ in $(seq 100); do
  curl -fsS -o /dev/null "http://localhost:$PORT_P/burrow/" 2>/dev/null && break
  kill -0 "$PROXY_PID" 2>/dev/null || { cat "$BLYG_E2E_SCRATCH/proxy.log" >&2; die "dev-proxy exited"; }
  sleep 0.1
done

PORT_PM="$("$CTL" free-port)"
python3 "$ROOT/crates/blyg-web/dev-proxy.py" --port "$PORT_PM" --upstream "127.0.0.1:$PORT_M" --mount "$MOUNT" \
  >"$BLYG_E2E_SCRATCH/proxy-m.log" 2>&1 &
PROXY_M_PID=$!
for _ in $(seq 100); do
  curl -fsS -o /dev/null "http://localhost:$PORT_PM$MOUNT/burrow/" 2>/dev/null && break
  kill -0 "$PROXY_M_PID" 2>/dev/null || { cat "$BLYG_E2E_SCRATCH/proxy-m.log" >&2; die "dev-proxy (mount) exited"; }
  sleep 0.1
done

LIVE_ORIGIN="http://localhost:$PORT_P" \
LIVE_ORIGIN_M="http://localhost:$PORT_PM" LIVE_MOUNT="$MOUNT" \
LIVE_URL_A="http://127.0.0.1:$PORT_A" LIVE_URL_B="http://127.0.0.1:$PORT_B" \
LIVE_TOKEN_A_FILE="$BLYG_E2E_SCRATCH/token-a" LIVE_TOKEN_B_FILE="$BLYG_E2E_SCRATCH/token-b" \
LIVE_PASSWORD_FILE="$BLYG_E2E_SCRATCH/password" PLAYWRIGHT_DIR="$PW" UX_SHOTS="$UX_SHOTS" \
  node "$ROOT/scripts/browser-web-ux-pass.mjs"
