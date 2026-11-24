# The blygger-spec commit Burrow's conformance scripts run against, and how
# they get it. Sourced by scripts/reader-conformance.sh,
# scripts/schemas-conformance.sh and scripts/feeds-conformance.sh
# (docs/CONFORMANCE.md).
#
# Commit f5817bd is upstream main (2026-11-23): blygger/blygger-spec#38 (the
# toolkit at revision 21 and studio 0.69.0) merged, then b53599c, the reader
# suite at the twenty-third revision (decision #118, spec §2 text content:
# fixtures 111 and 112). Its feeds and schemas suites are fbc3f62's.
# Render target: blygger-studio v0.71.0 (Burrow 0.28.0, docs/RENDER.md).
SPEC_REPO=https://github.com/blygger/blygger-spec
SPEC_COMMIT=f5817bdcdf6b6d36c8f587de44fb09c7c9f75b37 # upstream main

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
