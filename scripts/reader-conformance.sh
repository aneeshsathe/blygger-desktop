#!/usr/bin/env bash
# Burrow's renderer against the blygger-spec reader conformance suite
# (conformance/reader/ in blygger-spec; docs/CONFORMANCE-READER.md).
#
#   scripts/reader-conformance.sh [extra run.mjs args, e.g. --only label]
#
#   BLYG_SPEC_DIR=/path/to/blygger-spec   use this checkout, don't fetch
#   BLYG_CONFORMANCE_BIN=/path/to/bin     use this adapter binary, don't build
#   BLYG_READER_KEEP=1                    keep the scratch dir (report.html)
#   NODE=/path/to/node                    Node 20+ (default: node on PATH)
#
# What it does:
#   1. builds the adapter (`cargo build -p blyg-app --example blyg-conformance`);
#   2. shallow-fetches blygger-spec at the pinned commit below into a scratch
#      dir (nothing is written to this repo or to an existing checkout);
#   3. runs the suite in batch mode, writing summary.json, results.json and
#      report.html to the scratch dir (never the suite's own out/);
#   4. compares every case with scripts/reader-conformance.expected.
#
# Exit status: 0 when no case regressed. A regression is a case that passes
# in the baseline and now warns or fails, or warns and now fails; a case
# not in the suite any more is one too. Known fails are reported, not
# errors, and a case that got better is reported as fixed (update the
# baseline). The suite's own exit code (1 on any fail) is ignored for that
# reason. It is not wired into CI: it needs the network and Node 20.
set -euo pipefail

SPEC_REPO=https://github.com/aneeshsathe/blygger-spec
SPEC_COMMIT=510c76c953f7caab3207ed443e4edf8294ffd559 # branch conformance-0.4
# The baseline was last checked (2026-11-13) with BLYG_SPEC_DIR at blygger-spec
# d248bc4 (protocol 0.4 r15, decision #102's fixtures), which is not on
# SPEC_REPO yet: move the pin once it is.

die() { printf 'reader-conformance: %s\n' "$*" >&2; exit 2; }
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
EXPECTED="$ROOT/scripts/reader-conformance.expected"
NODE="${NODE:-node}"
command -v "$NODE" >/dev/null || die "Node 20+ is needed (set NODE)"

SCRATCH="$(mktemp -d "${TMPDIR:-/tmp}/burrow-reader.XXXXXX")"
cleanup() {
  if [ -z "${BLYG_READER_KEEP:-}" ]; then
    rm -rf "$SCRATCH"
  else
    printf 'reader-conformance: kept %s\n' "$SCRATCH" >&2
  fi
}
trap cleanup EXIT INT TERM

if [ -n "${BLYG_CONFORMANCE_BIN:-}" ]; then
  BIN="$BLYG_CONFORMANCE_BIN"
else
  (cd "$ROOT" && cargo build -q -p blyg-app --example blyg-conformance)
  TARGET="$(cd "$ROOT" && cargo metadata --format-version 1 --no-deps |
    "$NODE" -e 'let s="";process.stdin.on("data",d=>s+=d).on("end",()=>console.log(JSON.parse(s).target_directory))')"
  BIN="$TARGET/debug/examples/blyg-conformance"
fi
[ -x "$BIN" ] || die "no adapter binary at $BIN"

if [ -n "${BLYG_SPEC_DIR:-}" ]; then
  SPEC="$BLYG_SPEC_DIR"
else
  SPEC="$SCRATCH/blygger-spec"
  git init -q "$SPEC"
  git -C "$SPEC" fetch -q --depth 1 "$SPEC_REPO" "$SPEC_COMMIT"
  git -C "$SPEC" -c advice.detachedHead=false checkout -q FETCH_HEAD
fi
SUITE="$SPEC/conformance/reader"
[ -f "$SUITE/run.mjs" ] || die "no reader suite at $SUITE"

OUT="$SCRATCH/out"
(cd "$SUITE" && "$NODE" run.mjs --cmd "$BIN --batch" --batch --out "$OUT" --name burrow ${@+"$@"}) || true
[ -f "$OUT/summary.json" ] || die "the suite wrote no summary"

"$NODE" - "$OUT/summary.json" "$EXPECTED" "$#" <<'JS'
const fs = require("fs");
const [summaryPath, expectedPath, extraArgs] = process.argv.slice(2);
const rank = { pass: 0, info: 0, warn: 1, fail: 2 };
const now = new Map();
for (const c of JSON.parse(fs.readFileSync(summaryPath, "utf8")).checks) {
  if (c.id.endsWith(".census")) continue;
  now.set(c.id.split(".").slice(2).join("."), c);
}
const known = new Map();
for (const line of fs.readFileSync(expectedPath, "utf8").split("\n")) {
  const t = line.trim();
  if (!t || t.startsWith("#")) continue;
  const [status, id] = t.split(/\s+/);
  known.set(id, status);
}
const regressed = [], fixed = [], still = [];
for (const [id, c] of now) {
  const was = known.get(id) ?? "pass";
  if (rank[c.status] > rank[was]) regressed.push(`${id}: ${was} -> ${c.status}: ${c.detail}`);
  else if (rank[c.status] < rank[was]) fixed.push(`${id}: ${was} -> ${c.status}`);
  else if (c.status !== "pass") still.push(`${c.status} ${id}`);
}
// A subset run (--only) can't tell a removed case from one not selected.
if (extraArgs === "0") {
  for (const id of known.keys()) if (!now.has(id)) regressed.push(`${id}: no longer in the suite`);
}
const n = { pass: 0, warn: 0, fail: 0 };
for (const c of now.values()) n[c.status] = (n[c.status] ?? 0) + 1;
console.log(`reader-conformance: ${n.pass} pass, ${n.warn} warn, ${n.fail} fail of ${now.size}`);
if (still.length) console.log(`known (${still.length}):\n  ${still.join("\n  ")}`);
if (fixed.length) console.log(`fixed, take out of the baseline (${fixed.length}):\n  ${fixed.join("\n  ")}`);
if (regressed.length) {
  console.log(`REGRESSED (${regressed.length}):\n  ${regressed.join("\n  ")}`);
  process.exit(1);
}
JS
