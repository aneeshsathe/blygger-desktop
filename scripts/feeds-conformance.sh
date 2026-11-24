#!/usr/bin/env bash
# Burrow's pace-layered order against the blygger-spec feeds fixtures
# (conformance/feeds/ in blygger-spec, RFC-3; docs/CONFORMANCE.md).
#
#   scripts/feeds-conformance.sh
#
#   BLYG_SPEC_DIR=/path/to/blygger-spec   use this checkout, don't fetch
#
# The fixtures are vendored verbatim in crates/blyg-core/tests/fixtures/feeds/,
# and blyg-core's test `the_specs_feed_fixtures_give_their_expected_order`
# (src/pace_tests.rs) runs them in every `cargo test`. This script is the
# identity check (as blygger-studio's `check:fixtures` is for its copy): it
# fetches the spec at the pinned commit (scripts/conformance/spec.sh), fails
# if the vendored copy differs, then runs the test. Not in CI: it needs the
# network. To take a new fixture set: copy the spec's conformance/feeds/*.json
# over the vendored ones and move the pin.
set -euo pipefail

die() { printf 'feeds-conformance: %s\n' "$*" >&2; exit 2; }
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
. "$ROOT/scripts/conformance/spec.sh"
VENDORED="$ROOT/crates/blyg-core/tests/fixtures/feeds"

SCRATCH="$(mktemp -d "${TMPDIR:-/tmp}/burrow-feeds.XXXXXX")"
trap 'rm -rf "$SCRATCH"' EXIT INT TERM

SPEC="$(spec_checkout "$SCRATCH")"
FEEDS="$SPEC/conformance/feeds"
[ -d "$FEEDS" ] || die "no feeds fixtures at $FEEDS"

drift=0
for f in "$FEEDS"/*.json; do
  name="$(basename "$f")"
  if ! cmp -s "$f" "$VENDORED/$name"; then
    printf 'feeds-conformance: %s differs from the spec (or is missing)\n' "$name"
    drift=1
  fi
done
for f in "$VENDORED"/*.json; do
  name="$(basename "$f")"
  [ -f "$FEEDS/$name" ] || { printf 'feeds-conformance: %s is not in the spec\n' "$name"; drift=1; }
done
[ "$drift" = 0 ] || die "the vendored fixtures differ from blygger-spec ${SPEC_COMMIT:0:7}"
n="$(ls "$FEEDS"/*.json | wc -l | tr -d ' ')"
printf 'feeds-conformance: %s fixtures match blygger-spec %s\n' "$n" "${SPEC_COMMIT:0:7}"

cd "$ROOT"
cargo test -q -p blyg-core --lib pace::tests::the_specs_feed_fixtures_give_their_expected_order
