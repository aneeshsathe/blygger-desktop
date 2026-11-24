#!/usr/bin/env bash
# Burrow against the blygger-spec schemas suite's §10.1 grammar corpus
# (conformance/schemas/ in blygger-spec; docs/CONFORMANCE.md).
#
#   scripts/schemas-conformance.sh
#
#   BLYG_SPEC_DIR=/path/to/blygger-spec   use this checkout, don't fetch
#   BLYG_CONFORMANCE_BIN=/path/to/bin     use this adapter binary, don't build
#   STUDIO_DIR=/path/to/blygger-studio    a built studio checkout (npm ci &&
#                                         npm run build) at STUDIO_TAG; without
#                                         it one is cloned and built in scratch
#   BLYG_SCHEMAS_KEEP=1                   keep the scratch dir (report.html)
#   NODE=/path/to/node                    Node 20+ (default: node on PATH)
#
# The suite always runs its built-in studio adapter first (its samples are
# the corpus every other adapter is checked against), so it needs a studio.
# Burrow's adapter, scripts/conformance/schemas-adapter.mjs, implements
# grammar() only: Burrow publishes through the studio, so its documents are
# the studio's, and it stores no item documents itself. The suite runs with
# --offline (no live snapshot) and --out in the scratch dir. Burrow's grammar
# results are compared with scripts/schemas-conformance.expected, the same
# way scripts/reader-conformance.sh compares its baseline: exit 1 only when a
# case that was expected to pass fails, or a baseline case left the corpus.
# Not in CI: it needs the network, Node 20 and a studio build.
set -euo pipefail

STUDIO_REPO=https://github.com/blygger/blygger-studio
STUDIO_TAG=v0.69.0

die() { printf 'schemas-conformance: %s\n' "$*" >&2; exit 2; }
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
EXPECTED="$ROOT/scripts/schemas-conformance.expected"
. "$ROOT/scripts/conformance/spec.sh"
NODE="${NODE:-node}"
command -v "$NODE" >/dev/null || die "Node 20+ is needed (set NODE)"
NPM="$(dirname "$(command -v "$NODE")")/npm"

SCRATCH="$(mktemp -d "${TMPDIR:-/tmp}/burrow-schemas.XXXXXX")"
cleanup() {
  if [ -z "${BLYG_SCHEMAS_KEEP:-}" ]; then
    rm -rf "$SCRATCH"
  else
    printf 'schemas-conformance: kept %s\n' "$SCRATCH" >&2
  fi
}
trap cleanup EXIT INT TERM

BIN="$(conformance_bin "$ROOT" "$NODE")"
[ -x "$BIN" ] || die "no adapter binary at $BIN"
SPEC="$(spec_checkout "$SCRATCH")"
SUITE="$SPEC/conformance/schemas"
[ -f "$SUITE/run.mjs" ] || die "no schemas suite at $SUITE"

if [ -z "${STUDIO_DIR:-}" ]; then
  STUDIO_DIR="$SCRATCH/blygger-studio"
  git clone -q --depth 1 --branch "$STUDIO_TAG" -c advice.detachedHead=false "$STUDIO_REPO" "$STUDIO_DIR"
  (cd "$STUDIO_DIR" && "$NPM" ci --no-audit --no-fund >/dev/null && "$NPM" run -s build >/dev/null)
fi
export STUDIO_DIR

OUT="$SCRATCH/out"
(cd "$SUITE" && "$NPM" ci --no-audit --no-fund >/dev/null)
(cd "$SUITE" && BLYG_CONFORMANCE_BIN="$BIN" "$NODE" run.mjs --offline \
  --impl "$ROOT/scripts/conformance/schemas-adapter.mjs" --out "$OUT") || true
[ -f "$OUT/grammar-burrow.json" ] || die "the suite wrote no grammar results for Burrow"

"$NODE" - "$OUT/grammar-burrow.json" "$OUT/grammar-studio.json" "$EXPECTED" <<'JS'
const fs = require("fs");
const [burrowPath, studioPath, expectedPath] = process.argv.slice(2);
const read = (p) => (fs.existsSync(p) ? JSON.parse(fs.readFileSync(p, "utf8")) : null);
const burrow = read(burrowPath), studio = read(studioPath);
const known = new Set();
for (const line of fs.readFileSync(expectedPath, "utf8").split("\n")) {
  const t = line.trim();
  if (!t || t.startsWith("#")) continue;
  const [status, id] = t.split(/\s+/);
  if (status === "fail") known.add(id);
}
const studioOk = new Map((studio?.results ?? []).map((r) => [r.case, r.ok]));
const now = new Map(burrow.results.map((r) => [r.case, r]));
const regressed = [], fixed = [], still = [];
for (const [id, r] of now) {
  if (!r.ok && !known.has(id)) regressed.push(`${id}: ${r.diffs.join("; ")}`);
  else if (r.ok && known.has(id)) fixed.push(id);
  else if (!r.ok) still.push(`fail ${id}${studioOk.get(id) === false ? " (the studio fails it too)" : ""}`);
}
for (const id of known) if (!now.has(id)) regressed.push(`${id}: no longer in the corpus`);
const pass = [...now.values()].filter((r) => r.ok).length;
const sPass = [...studioOk.values()].filter(Boolean).length;
console.log(`schemas-conformance: grammar ${pass} pass, ${now.size - pass} fail of ${now.size} (${burrow.implementation}); studio ${studio?.version ?? "?"}: ${sPass} of ${studioOk.size}`);
if (still.length) console.log(`known (${still.length}):\n  ${still.join("\n  ")}`);
if (fixed.length) console.log(`fixed, take out of the baseline (${fixed.length}):\n  ${fixed.join("\n  ")}`);
if (regressed.length) {
  console.log(`REGRESSED (${regressed.length}):\n  ${regressed.join("\n  ")}`);
  process.exit(1);
}
JS
