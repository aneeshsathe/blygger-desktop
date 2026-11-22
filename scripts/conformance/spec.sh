# The blygger-spec commit Burrow's conformance scripts run against, and how
# they get it. Sourced by scripts/reader-conformance.sh,
# scripts/schemas-conformance.sh and scripts/feeds-conformance.sh
# (docs/CONFORMANCE.md).
#
# Commit fbc3f62 is branch toolkit-0.69.0 (the toolkit at spec revision 21
# and blygger-studio v0.69.0, open upstream as blygger/blygger-spec#38), one
# commit on top of upstream main 64b272e (2026-11-21). Upstream main's own
# toolkit is still at studio 0.65.0 and revision 18; move the pin to
# upstream once #38 merges.
# Studio target: blygger-studio v0.69.0 (27b5924, contract 6.2.0, Burrow 0.28.0).
SPEC_REPO=https://github.com/aneeshsathe/blygger-spec
SPEC_COMMIT=fbc3f6243f3efe686629e2713f9859715a183973 # branch toolkit-0.69.0

# spec_checkout <scratch dir>: prints the path of a blygger-spec checkout at
# SPEC_COMMIT: $BLYG_SPEC_DIR when set, else a shallow fetch into the
# scratch dir (nothing is written to this repo or to an existing checkout).
spec_checkout() {
  if [ -n "${BLYG_SPEC_DIR:-}" ]; then
    printf '%s\n' "$BLYG_SPEC_DIR"
    return
  fi
  local spec="$1/blygger-spec"
  git init -q "$spec"
  git -C "$spec" fetch -q --depth 1 "$SPEC_REPO" "$SPEC_COMMIT"
  git -C "$spec" -c advice.detachedHead=false checkout -q FETCH_HEAD
  printf '%s\n' "$spec"
}

# conformance_bin <root> <node>: prints the path of the blyg-conformance
# adapter binary: $BLYG_CONFORMANCE_BIN when set, else built.
conformance_bin() {
  if [ -n "${BLYG_CONFORMANCE_BIN:-}" ]; then
    printf '%s\n' "$BLYG_CONFORMANCE_BIN"
    return
  fi
  (cd "$1" && cargo build -q -p blyg-app --example blyg-conformance >&2)
  local target
  target="$(cd "$1" && cargo metadata --format-version 1 --no-deps |
    "$2" -e 'let s="";process.stdin.on("data",d=>s+=d).on("end",()=>console.log(JSON.parse(s).target_directory))')"
  printf '%s\n' "$target/debug/examples/blyg-conformance"
}
