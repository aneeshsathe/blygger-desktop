#!/usr/bin/env bash
# How Burrow web FEELS, measured: the flow pass. It walks whole journeys
# (first open to reading, catching up, quote then publish, draft and revise,
# subscribe, tab switching, sync while reading and typing, phone back and
# keyboard) in headless browsers against REAL local studios with invented
# data, and records per step the time to settle, layout shifts, long tasks,
# requests (and duplicates), focus, kept scroll and selection, flicker, and
# the inputs each journey takes. The same journeys run in the studio's own
# web UI on an identical blyg, as the reference for "nice".
#
#   BLYG_WORKER_DIR=/path/to/built/blygger-studio FLOW_OUT=/some/dir \
#     scripts/browser-web-flow-pass.sh
#
# Writes $FLOW_OUT/flow.json (every step's numbers), $FLOW_OUT/summary.md
# (per journey, Burrow beside the studio) and a screenshot per journey end.
# Exits 1 when a Burrow journey breaks its budget in
# scripts/web-flow-budgets.json, so a regression fails.
#
#   FLOW_TARGETS=burrow,studio   which UIs to walk (default both)
#   FLOW_BROWSERS=chromium,webkit  the phone journeys also run in WebKit
#   FLOW_ONLY=<journey,…>        just these journeys
#   BLYG_WEB_NO_BUILD=1          keep the dist/ that's there
#
# Three studios run under `wrangler dev --local` (scripts/e2e-wrangler.sh):
# `burrow` and `studio` are seeded alike (each walked by one UI, so one's
# read marks don't empty the other's list), and `b` is a blyg both follow.
# scripts/web-flow-feeds.py serves eleven plain RSS sites on loopback ports
# for volume: ten are followed (300+ posts over 11 subscriptions) and the
# last is the one the subscribe journey follows. Only the PIDs started here are
# stopped. Never a real blyg.
set -euo pipefail

die() { printf 'web-flow-pass: %s\n' "$*" >&2; exit 2; }
: "${BLYG_WORKER_DIR:?set BLYG_WORKER_DIR to a built blygger-studio directory}"
: "${FLOW_OUT:?set FLOW_OUT to the directory for the numbers and screenshots}"
[ -d "$BLYG_WORKER_DIR" ] || die "no such directory: $BLYG_WORKER_DIR"
BLYG_WORKER_DIR="$(cd "$BLYG_WORKER_DIR" && pwd)"
export BLYG_WORKER_DIR
mkdir -p "$FLOW_OUT"
FLOW_OUT="$(cd "$FLOW_OUT" && pwd)"

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CTL="$ROOT/scripts/e2e-wrangler.sh"
BLYG_E2E_SCRATCH="$(mktemp -d "${TMPDIR:-/tmp}/burrow-web-flow.XXXXXX")"
export BLYG_E2E_SCRATCH
PROXY_PID=""
FEEDS_PID=""

cleanup() {
  [ -n "$PROXY_PID" ] && kill "$PROXY_PID" 2>/dev/null || true
  [ -n "$FEEDS_PID" ] && kill "$FEEDS_PID" 2>/dev/null || true
  "$CTL" stop-all || true
  if [ -z "${BLYG_E2E_KEEP:-}" ]; then
    rm -rf "$BLYG_E2E_SCRATCH"
  else
    printf 'web-flow-pass: kept %s\n' "$BLYG_E2E_SCRATCH" >&2
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

python3 "$ROOT/scripts/web-flow-feeds.py" --count 11 --entries 30 --out "$BLYG_E2E_SCRATCH/feeds.json" \
  >"$BLYG_E2E_SCRATCH/feeds.log" 2>&1 &
FEEDS_PID=$!

"$CTL" setup
PORT_A="$("$CTL" free-port)"
PORT_S="$("$CTL" free-port)"
PORT_B="$("$CTL" free-port)"
printf 'web-flow-pass: starting studios on :%s (Burrow), :%s (studio UI), :%s (b)…\n' "$PORT_A" "$PORT_S" "$PORT_B" >&2
"$CTL" start burrow "$PORT_A"
"$CTL" start studio "$PORT_S"
"$CTL" start b "$PORT_B"
umask 077
for n in A S B; do
  eval "port=\$PORT_$n"
  # shellcheck disable=SC2154
  "$CTL" mint-token "$port" >"$BLYG_E2E_SCRATCH/token-$n"
  [ -s "$BLYG_E2E_SCRATCH/token-$n" ] || die "this studio mints no tokens (too old)"
done
"$CTL" password >"$BLYG_E2E_SCRATCH/password"
for _ in $(seq 100); do
  [ -s "$BLYG_E2E_SCRATCH/feeds.json" ] && break
  kill -0 "$FEEDS_PID" 2>/dev/null || { cat "$BLYG_E2E_SCRATCH/feeds.log" >&2; die "the feed sites exited"; }
  sleep 0.1
done

PORT_P="$("$CTL" free-port)"
python3 "$ROOT/crates/blyg-web/dev-proxy.py" --port "$PORT_P" --upstream "127.0.0.1:$PORT_A" \
  >"$BLYG_E2E_SCRATCH/proxy.log" 2>&1 &
PROXY_PID=$!
for _ in $(seq 100); do
  curl -fsS -o /dev/null "http://localhost:$PORT_P/burrow/" 2>/dev/null && break
  kill -0 "$PROXY_PID" 2>/dev/null || { cat "$BLYG_E2E_SCRATCH/proxy.log" >&2; die "dev-proxy exited"; }
  sleep 0.1
done

export FLOW_ORIGIN_BURROW="http://localhost:$PORT_P" FLOW_ORIGIN_STUDIO="http://127.0.0.1:$PORT_S" \
  FLOW_URL_BURROW="http://127.0.0.1:$PORT_A" FLOW_URL_STUDIO="http://127.0.0.1:$PORT_S" FLOW_URL_B="http://127.0.0.1:$PORT_B" \
  FLOW_TOKEN_BURROW_FILE="$BLYG_E2E_SCRATCH/token-A" FLOW_TOKEN_STUDIO_FILE="$BLYG_E2E_SCRATCH/token-S" \
  FLOW_TOKEN_B_FILE="$BLYG_E2E_SCRATCH/token-B" FLOW_PASSWORD_FILE="$BLYG_E2E_SCRATCH/password" \
  FLOW_FEEDS_FILE="$BLYG_E2E_SCRATCH/feeds.json" FLOW_BUDGETS="$ROOT/scripts/web-flow-budgets.json" \
  PLAYWRIGHT_DIR="$PW" FLOW_OUT="$FLOW_OUT"
# FLOW_HOLD=<file>: start and seed, write the environment to <file>, and
# keep everything up until <file> is deleted (for re-running the .mjs with
# FLOW_PHASE=journeys while fixing).
if [ -n "${FLOW_HOLD:-}" ]; then
  FLOW_PHASE=seed node "$ROOT/scripts/browser-web-flow-pass.mjs"
  env | grep -E '^(FLOW_|PLAYWRIGHT_DIR=)' | sed 's/^/export /' >"$FLOW_HOLD"
  printf 'web-flow-pass: up; delete %s to stop\n' "$FLOW_HOLD" >&2
  while [ -f "$FLOW_HOLD" ]; do sleep 2; done
  exit 0
fi
node "$ROOT/scripts/browser-web-flow-pass.mjs"
