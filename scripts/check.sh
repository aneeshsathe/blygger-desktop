#!/usr/bin/env bash
# The checks every commit must pass: the same commands CI runs, in one place.
# CI calls this script step by step, so a check added here runs in CI too.
#
#   scripts/check.sh --fix        the gate before a commit: format the code,
#                                 then fmt, clippy, test and wording
#   scripts/check.sh              the same, failing on unformatted code
#   scripts/check.sh STEP...      only those steps, in that order
#   scripts/check.sh all          every step, docs and deny included
#
# Steps:
#   fmt         cargo fmt --check (with --fix, formats first)
#   clippy      clippy on every crate and target, warnings are errors
#   test        the workspace's tests
#   wording     blyg-app's tests with UI text spelled as Windows shows it
#               (Ctrl+B, not ⌘B), so a test expecting Mac glyphs fails on a
#               Mac too (skills/burrow-cross-platform, "Testing wording").
#               Skipped by default on Windows, where it is the normal spelling.
#   extensions  format check of extensions/community.toml (no network)
#   docs        the docs site with the pinned mdBook (scripts/build-site.sh)
#   ci-skip     no code reads a path CI may skip (scripts/ci-skip.py check)
#   deny        cargo deny --locked check (deny.toml): advisories, licences, sources;
#               needs cargo-deny (`cargo install cargo-deny --locked`)
#
# docs and deny aren't in the default run: docs downloads mdBook on first use
# and checks only site/; deny needs a tool most machines lack and fetches the
# RustSec advisory database. CI runs both, in their own jobs. Run
# `scripts/check.sh docs` when you change site/, and `deny` when you change
# dependencies.
#
# On Windows, run it from Git Bash (`bash scripts/check.sh`); CI does the same.
# Cargo runs with --locked, so a stale Cargo.lock fails here as it does in CI.
set -euo pipefail
cd "$(dirname "$0")/.."

fix=0
steps=()
for arg in "$@"; do
  case "$arg" in
    --fix) fix=1 ;;
    -h|--help) sed -n '2,/^set -euo/p' "$0" | sed '$d; s/^# \{0,1\}//'; exit 0 ;;
    all) steps+=(fmt clippy test wording extensions ci-skip docs deny) ;;
    fmt|clippy|test|wording|extensions|ci-skip|docs|deny) steps+=("$arg") ;;
    *) echo "check.sh: unknown step '$arg' (try --help)" >&2; exit 2 ;;
  esac
done
if [ ${#steps[@]} -eq 0 ]; then
  steps=(fmt clippy test)
  case "$(uname -s)" in
    MINGW*|MSYS*|CYGWIN*) ;;
    *) steps+=(wording) ;;
  esac
fi

run() {
  echo "==> $*" >&2
  "$@"
}

for step in "${steps[@]}"; do
  case "$step" in
    fmt)
      if [ "$fix" = 1 ]; then run cargo fmt --all; fi
      run cargo fmt --all -- --check ;;
    clippy) run cargo clippy --workspace --all-targets --locked -- -D warnings ;;
    test) run cargo test --workspace --locked ;;
    wording) run env BLYGGER_TEST_SPELLING=windows cargo test -p blyg-app --locked ;;
    extensions) run python3 scripts/community-extensions.py check ;;
    ci-skip) run python3 scripts/ci-skip.py check ;;
    docs) run scripts/build-site.sh ;;
    deny) run cargo deny --locked check ;;
  esac
done
echo "check.sh: passed (${steps[*]})" >&2
