#!/usr/bin/env bash
# Build Burrow for the browser and serve it on http://127.0.0.1:8787/.
#
#   ./crates/blyg-web/serve.sh               build, then serve
#   ./crates/blyg-web/serve.sh --build-only  build into crates/blyg-web/dist/
#   PORT=9000 ./crates/blyg-web/serve.sh     another port
#
# Needs: rustup's wasm32-unknown-unknown target and python3 (for the static
# server). wasm-bindgen-cli is installed on first run, pinned to the
# wasm-bindgen version in Cargo.lock, under the cargo target dir (nothing
# global). Respects CARGO_TARGET_DIR.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
TARGET="${CARGO_TARGET_DIR:-$ROOT/target}"
DIST="$HERE/dist"
PORT="${PORT:-8787}"
# Must equal the `wasm-bindgen` pin in crates/blyg-web/Cargo.toml.
BINDGEN_VERSION="0.2.128"
TOOLS="$TARGET/blyg-web-tools"
BINDGEN="$TOOLS/bin/wasm-bindgen"

rustup target list --installed 2>/dev/null | grep -qx wasm32-unknown-unknown \
  || rustup target add wasm32-unknown-unknown

if ! "$BINDGEN" --version 2>/dev/null | grep -q "$BINDGEN_VERSION"; then
  echo "Installing wasm-bindgen-cli $BINDGEN_VERSION into $TOOLS"
  cargo install wasm-bindgen-cli --version "$BINDGEN_VERSION" --locked --root "$TOOLS"
fi

echo "Building blyg-web (wasm32, release)"
# blyg-core links SQLite and ring (for its store and HTTP client), whose C
# parts don't build for wasm32 with every toolchain. This crate never calls
# them, so the linker may leave their symbols undefined. The few that a C-ABI
# export still names become `env` imports, stubbed by www/env.js.
(cd "$ROOT" && CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUSTFLAGS="-C link-arg=--allow-undefined" \
  cargo build -p blyg-web --lib --release --target wasm32-unknown-unknown)

rm -rf "$DIST"
mkdir -p "$DIST/pkg" "$DIST/fonts"
"$BINDGEN" --target web --no-typescript --out-dir "$DIST/pkg" \
  "$TARGET/wasm32-unknown-unknown/release/blyg_web.wasm"
# Every `env` import must have a stub in www/env.js, or the page won't start.
if command -v node >/dev/null 2>&1; then
  node -e '
    const fs = require("fs");
    const m = new WebAssembly.Module(fs.readFileSync(process.argv[1]));
    const env = fs.readFileSync(process.argv[2], "utf8");
    const missing = WebAssembly.Module.imports(m)
      .filter((i) => i.module === "env" && !env.includes(`export const ${i.name} =`))
      .map((i) => i.name);
    if (missing.length) {
      console.error("error: add these to www/env.js: " + missing.join(", "));
      process.exit(1);
    }' "$DIST/pkg/blyg_web_bg.wasm" "$HERE/www/env.js"
fi
if command -v wasm-opt >/dev/null 2>&1; then
  wasm-opt -Oz --enable-bulk-memory --enable-nontrapping-float-to-int \
    -o "$DIST/pkg/blyg_web_bg.wasm" "$DIST/pkg/blyg_web_bg.wasm" || true
fi
cp "$HERE"/www/* "$DIST/"
cp "$ROOT"/docs/prototype/*.woff "$DIST/fonts/"
echo "Built $DIST ($(du -sh "$DIST/pkg/blyg_web_bg.wasm" | cut -f1) of wasm)"

if [[ "${1:-}" == "--build-only" ]]; then
  exit 0
fi
echo "Serving on http://127.0.0.1:$PORT/ (Ctrl-C stops it)"
exec python3 -m http.server "$PORT" --bind 127.0.0.1 --directory "$DIST"
