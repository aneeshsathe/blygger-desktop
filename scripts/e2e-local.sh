#!/usr/bin/env bash
# End-to-end tests of blyg-core against a REAL blyg Worker running locally.
#
#   BLYG_WORKER_DIR=/path/to/worker scripts/e2e-local.sh [extra test args]
#
# BLYG_WORKER_DIR is the Worker's directory (the one with wrangler.jsonc and
# node_modules; run `npm install && npm run build` there first): a
# blygger-studio 0.49.12 or later (API contract 5.x, the only one Burrow
# speaks).
#
#   BLYG_E2E_AUTH=password   sign in with the studio password, not the token
#   BLYG_E2E_STOCK=1         kept for older invocations; every studio Burrow
#                            supports is stock, and gets a minted manual token
#   BLYG_E2E_SKIP_NOTES=1    skip the markdown-notes extension's e2e
#   BLYG_E2E_SKIP_SWITCHER=1 skip the blyg switcher's e2e (the app's, with
#                            `a` and `b` signed in at once)
#
# What it does:
#   1. derives a local-only wrangler config and random dev secrets in a
#      scratch dir (nothing is written to the Worker directory);
#   2. starts two `wrangler dev --local` instances on free ports, each with
#      its own scratch D1/R2 state (--persist-to): `a` is the blyg under test,
#      `b` a second blyg to subscribe to;
#   3. runs `cargo test -p blyg-core --test e2e -- --ignored --test-threads=1`,
#      then the markdown-notes extension's e2e against the same blyg
#      (`cargo test -p blyg-ext-notes --test e2e -- --ignored --test-threads=1`);
#      extra test args go to the core suite only; and the blyg switcher's
#      e2e in the app (`cargo test -p blyg-app --bin blygger switcher::e2e
#      -- --ignored`), `a` and `b` (as localhost) signed in side by side;
#   4. stops only the wrangler processes it started, and deletes the scratch
#      dir (set BLYG_E2E_KEEP=1 to keep it, e.g. to read the wrangler logs).
#
# It never uses --remote, never deploys, and never talks to a real blyg.
set -euo pipefail

die() { printf 'e2e-local: %s\n' "$*" >&2; exit 1; }
: "${BLYG_WORKER_DIR:?set BLYG_WORKER_DIR to the blyg Worker directory}"
[ -d "$BLYG_WORKER_DIR" ] || die "no such directory: $BLYG_WORKER_DIR"
BLYG_WORKER_DIR="$(cd "$BLYG_WORKER_DIR" && pwd)"
export BLYG_WORKER_DIR

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CTL="$ROOT/scripts/e2e-wrangler.sh"
BLYG_E2E_SCRATCH="$(mktemp -d "${TMPDIR:-/tmp}/blygger-e2e.XXXXXX")"
export BLYG_E2E_SCRATCH

cleanup() {
  "$CTL" stop-all || true
  if [ -z "${BLYG_E2E_KEEP:-}" ]; then
    rm -rf "$BLYG_E2E_SCRATCH"
  else
    printf 'e2e-local: kept %s\n' "$BLYG_E2E_SCRATCH" >&2
  fi
}
trap cleanup EXIT INT TERM

"$CTL" setup
PORT_A="$("$CTL" free-port)"
PORT_B="$("$CTL" free-port)"
[ "$PORT_A" != "$PORT_B" ] || PORT_B="$("$CTL" free-port)"
printf 'e2e-local: starting the Worker on :%s and :%s…\n' "$PORT_A" "$PORT_B" >&2
"$CTL" start a "$PORT_A"
"$CTL" start b "$PORT_B"

export BLYG_E2E_URL="http://127.0.0.1:$PORT_A"
export BLYG_E2E_URL_B="http://127.0.0.1:$PORT_B"
# A studio 0.28+ mints manual tokens (one per instance); an older Worker
# fork uses its BLYG_OWNER_TOKEN; an older stock studio, the password.
BLYG_E2E_TOKEN="$("$CTL" mint-token "$PORT_A")"
if [ -n "$BLYG_E2E_TOKEN" ]; then
  BLYG_E2E_TOKEN_B="$("$CTL" mint-token "$PORT_B")"
  export BLYG_E2E_TOKEN_B
  # The blyg switcher's second blyg: `b` again, as localhost, so its host
  # (a blyg's identity: Keychain account, data folder) differs from `a`'s.
  # Studio tokens are bound to their origin, so it gets its own.
  BLYG_E2E_SWITCH_URL="http://localhost:$PORT_B"
  BLYG_E2E_SWITCH_TOKEN="$("$CTL" mint-token "$PORT_B" localhost)"
  export BLYG_E2E_SWITCH_URL BLYG_E2E_SWITCH_TOKEN
  [ "${BLYG_E2E_AUTH:-}" != password ] || BLYG_E2E_PASSWORD="$("$CTL" password)"
else
  BLYG_E2E_TOKEN="$("$CTL" token)"
  if [ -n "${BLYG_E2E_STOCK:-}" ] || [ "${BLYG_E2E_AUTH:-}" = password ]; then
    BLYG_E2E_PASSWORD="$("$CTL" password)"
  fi
fi
export BLYG_E2E_TOKEN
[ -z "${BLYG_E2E_PASSWORD:-}" ] || export BLYG_E2E_PASSWORD
# The browser sign-in tests act as the owner in a browser (sign in to the
# studio, then allow Burrow), whatever the rest of the suite signs in with.
BLYG_E2E_OWNER_PASSWORD="$("$CTL" password)"
export BLYG_E2E_OWNER_PASSWORD
export BLYG_E2E_CTL="$CTL"

cd "$ROOT"
cargo test -p blyg-core --test e2e -- --ignored --test-threads=1 "$@"
if [ -z "${BLYG_E2E_SKIP_SWITCHER:-}" ]; then
  printf 'e2e-local: the blyg switcher, two blygs at once…\n' >&2
  cargo test -p blyg-app --bin blygger switcher::e2e -- --ignored --test-threads=1
fi
[ -z "${BLYG_E2E_SKIP_NOTES:-}" ] || exit 0
printf 'e2e-local: the markdown-notes extension…\n' >&2
cargo test -p blyg-ext-notes --test e2e -- --ignored --test-threads=1
