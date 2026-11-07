#!/usr/bin/env bash
# Build Burrow for the browser and serve it on http://127.0.0.1:8787/.
#
#   ./crates/blyg-web/serve.sh               build, then serve
#   ./crates/blyg-web/serve.sh --build-only  build into crates/blyg-web/dist/
#   PORT=9000 ./crates/blyg-web/serve.sh     another port
#   WASM_OPT=1 ./crates/blyg-web/serve.sh    also run wasm-opt -Oz (smaller raw,
#                                            but a little larger gzipped; see README)
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
# Binaryen's optimiser, from the `wasm-opt` crate on crates.io (it builds
# Binaryen from source; no prebuilt binaries). Pinned so builds repeat.
WASM_OPT_VERSION="0.116.1"
WASM_OPT_BIN="$TOOLS/bin/wasm-opt"
BUILD_ONLY=0
[[ "${1:-}" == "--build-only" ]] && BUILD_ONLY=1

rustup target list --installed 2>/dev/null | grep -qx wasm32-unknown-unknown \
  || rustup target add wasm32-unknown-unknown

if ! "$BINDGEN" --version 2>/dev/null | grep -q "$BINDGEN_VERSION"; then
  echo "Installing wasm-bindgen-cli $BINDGEN_VERSION into $TOOLS"
  cargo install wasm-bindgen-cli --version "$BINDGEN_VERSION" --locked --root "$TOOLS"
fi
if [[ "${WASM_OPT:-0}" == 1 ]] && [[ "$(cat "$TOOLS/wasm-opt.version" 2>/dev/null)" != "$WASM_OPT_VERSION" || ! -x "$WASM_OPT_BIN" ]]; then
  echo "Installing wasm-opt $WASM_OPT_VERSION into $TOOLS (compiles Binaryen; a few minutes, once)"
  cargo install wasm-opt --version "$WASM_OPT_VERSION" --locked --root "$TOOLS"
  echo "$WASM_OPT_VERSION" > "$TOOLS/wasm-opt.version"
fi

echo "Building blyg-web (wasm32, profile wasm-release)"
# blyg-core links SQLite and ring (for its store and HTTP client), whose C
# parts don't build for wasm32 with every toolchain. This crate never calls
# them, so the linker may leave their symbols undefined. The few that a C-ABI
# export still names become `env` imports, stubbed by www/env.js.
# Source paths in panic messages are remapped, so the module names no home
# directory or checkout path and is the same wherever it is built.
CARGO_HOME_DIR="${CARGO_HOME:-$HOME/.cargo}"
RUSTUP_HOME_DIR="${RUSTUP_HOME:-$HOME/.rustup}"
(cd "$ROOT" && CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUSTFLAGS="-C link-arg=--allow-undefined \
--remap-path-prefix=$ROOT=/burrow --remap-path-prefix=$CARGO_HOME_DIR=/cargo --remap-path-prefix=$RUSTUP_HOME_DIR=/rustup" \
  cargo build -p blyg-web --lib --profile wasm-release --target wasm32-unknown-unknown)

rm -rf "$DIST"
mkdir -p "$DIST/pkg" "$DIST/fonts"
"$BINDGEN" --target web --no-typescript --out-dir "$DIST/pkg" \
  "$TARGET/wasm32-unknown-unknown/wasm-release/blyg_web.wasm"
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
if [[ "${WASM_OPT:-0}" == 1 ]]; then
  "$WASM_OPT_BIN" -Oz --enable-bulk-memory --enable-nontrapping-float-to-int --enable-sign-ext --enable-mutable-globals \
    -o "$DIST/pkg/blyg_web_bg.wasm" "$DIST/pkg/blyg_web_bg.wasm"
fi
cp "$HERE"/www/* "$DIST/"
cp "$ROOT"/docs/prototype/*.woff "$DIST/fonts/"

# Icons for the installed app (the repo's app icon). The home-screen icon
# on iOS is the tile alone, full bleed; iOS rounds it itself.
mkdir -p "$DIST/icons"
cp "$ROOT/packaging/icon.svg" "$ROOT/packaging/icon-512.png" "$DIST/icons/"
if command -v sips >/dev/null 2>&1; then
  sips -z 192 192 "$ROOT/packaging/icon-512.png" --out "$DIST/icons/icon-192.png" >/dev/null
  sips -c 412 412 "$ROOT/packaging/icon-512.png" --out "$DIST/icons/tile.png" >/dev/null
  sips -z 180 180 "$DIST/icons/tile.png" --out "$DIST/icons/icon-180.png" >/dev/null
  rm "$DIST/icons/tile.png"
else
  cp "$ROOT/packaging/icon-512.png" "$DIST/icons/icon-192.png"
  cp "$ROOT/packaging/icon-512.png" "$DIST/icons/icon-180.png"
fi

# The service worker caches every file of this build under a name that
# changes when any file does, so an update replaces the offline copy.
(cd "$DIST" && {
  FILES="$(find . -type f ! -name sw.js | sed 's|^\./||' | LC_ALL=C sort)"
  BUILD="$(printf '%s\n' "$FILES" | while read -r f; do cat "$f"; done | shasum | cut -c1-12)"
  LIST="[$(printf '%s\n' "$FILES" | sed 's/.*/"&"/' | paste -sd, -)]"
  sed -e "s|__BUILD__|$BUILD|" -e "s|__FILES__|$LIST|" sw.js > sw.js.tmp && mv sw.js.tmp sw.js
})
W="$DIST/pkg/blyg_web_bg.wasm"
echo "Built $DIST: wasm $(wc -c <"$W" | tr -d ' ') bytes, $(gzip -9 -n -c "$W" | wc -c | tr -d ' ') gzipped, sha256 $(shasum -a 256 "$W" | cut -c1-16)"

if [[ $BUILD_ONLY == 1 ]]; then
  exit 0
fi
echo "Serving on http://127.0.0.1:$PORT/ (Ctrl-C stops it)"
exec python3 -m http.server "$PORT" --bind 127.0.0.1 --directory "$DIST"
