#!/usr/bin/env bash
# Burrow web's live updates, end to end, in headless Chromium against a REAL
# local studio (never a mock, never a real blyg):
#
#   BLYG_WORKER_DIR=/path/to/blygger-studio scripts/browser-web-live-check.sh
#
# BLYG_WORKER_DIR is a built studio (`npm ci && npm run build` there; work
# in a scratch copy). Playwright comes from $PLAYWRIGHT_DIR (a node_modules
# dir holding `playwright`), else the studio's own node_modules (Studio has
# it as a dev dependency), else it's installed into this run's scratch dir.
# Chromium must be installed for it (`npx playwright install chromium`).
#
# What it does:
#   1. builds crates/blyg-web/dist (serve.sh --build-only; BLYG_WEB_NO_BUILD=1
#      uses the dist that's there);
#   2. starts two `wrangler dev --local` studios with scratch state through
#      scripts/e2e-wrangler.sh: `a`, the blyg under test, and `b`, a blyg
#      it subscribes to; mints a token on each (kept in files, never shown);
#   3. serves dist through crates/blyg-web/dev-proxy.py in front of `a`, the
#      way a blyg serves it (same origin as /api and /studio);
#   4. runs scripts/browser-web-live-check.mjs: it seeds both, opens Burrow
#      web signed in with a Studio session, checks that idle polling stays
#      within 5 requests a minute, then changes things through the API and
#      checks each change is on screen within 20 s with no interaction;
#   5. stops only the processes it started (by PID) and deletes its scratch
#      dir (BLYG_E2E_KEEP=1 keeps it, with the wrangler and proxy logs).
set -euo pipefail

die() { printf 'web-live-check: %s\n' "$*" >&2; exit 1; }
: "${BLYG_WORKER_DIR:?set BLYG_WORKER_DIR to a built blygger-studio directory}"
[ -d "$BLYG_WORKER_DIR" ] || die "no such directory: $BLYG_WORKER_DIR"
BLYG_WORKER_DIR="$(cd "$BLYG_WORKER_DIR" && pwd)"
export BLYG_WORKER_DIR

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CTL="$ROOT/scripts/e2e-wrangler.sh"
BLYG_E2E_SCRATCH="$(mktemp -d "${TMPDIR:-/tmp}/burrow-web-live.XXXXXX")"
export BLYG_E2E_SCRATCH
PROXY_PID=""

cleanup() {
  [ -n "$PROXY_PID" ] && kill "$PROXY_PID" 2>/dev/null || true
  "$CTL" stop-all || true
  if [ -z "${BLYG_E2E_KEEP:-}" ]; then
    rm -rf "$BLYG_E2E_SCRATCH"
  else
    printf 'web-live-check: kept %s\n' "$BLYG_E2E_SCRATCH" >&2
  fi
}
trap cleanup EXIT INT TERM

# Playwright: given, the studio's, or a scratch install.
PW="${PLAYWRIGHT_DIR:-}"
if [ -z "$PW" ] && [ -d "$BLYG_WORKER_DIR/node_modules/playwright" ]; then
  PW="$BLYG_WORKER_DIR/node_modules"
fi
if [ -z "$PW" ]; then
  mkdir -p "$BLYG_E2E_SCRATCH/pw"
  (cd "$BLYG_E2E_SCRATCH/pw" && npm init -y >/dev/null && npm install --no-audit --no-fund playwright@1.56.1 >/dev/null) \
    || die "installing playwright failed"
  PW="$BLYG_E2E_SCRATCH/pw/node_modules"
fi
[ -d "$PW/playwright" ] || die "no playwright in $PW"

if [ -z "${BLYG_WEB_NO_BUILD:-}" ]; then
  "$ROOT/crates/blyg-web/serve.sh" --build-only
fi
[ -f "$ROOT/crates/blyg-web/dist/index.html" ] || die "no crates/blyg-web/dist (build it first)"

"$CTL" setup
PORT_A="$("$CTL" free-port)"
PORT_B="$("$CTL" free-port)"
[ "$PORT_A" != "$PORT_B" ] || PORT_B="$("$CTL" free-port)"
printf 'web-live-check: starting the studio on :%s and :%s…\n' "$PORT_A" "$PORT_B" >&2
"$CTL" start a "$PORT_A"
"$CTL" start b "$PORT_B"
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

LIVE_ORIGIN="http://localhost:$PORT_P" \
LIVE_URL_A="http://127.0.0.1:$PORT_A" LIVE_URL_B="http://127.0.0.1:$PORT_B" \
LIVE_TOKEN_A_FILE="$BLYG_E2E_SCRATCH/token-a" LIVE_TOKEN_B_FILE="$BLYG_E2E_SCRATCH/token-b" \
LIVE_PASSWORD_FILE="$BLYG_E2E_SCRATCH/password" PLAYWRIGHT_DIR="$PW" \
  node "$ROOT/scripts/browser-web-live-check.mjs"
