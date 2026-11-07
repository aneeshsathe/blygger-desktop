# blyg-web: Burrow in a browser

Try Burrow's main window in a browser tab. The rules come from Burrow's own
Rust, compiled to WebAssembly: what the omnibar matches, what ⏎ does, the
counter, the labels, when a post may be published, deleted or discarded, and
the Markdown preview. A small page of plain ES modules draws the window. It
runs on the invented sample posts that `BLYGGER_FAKE=1` uses and stores
them in this browser (IndexedDB). Nothing is sent anywhere.

```sh
./crates/blyg-web/serve.sh            # build, then serve on http://127.0.0.1:8787/
./crates/blyg-web/serve.sh --build-only   # just build crates/blyg-web/dist/
```

You need the `wasm32-unknown-unknown` target (the script adds it with
rustup) and `python3` for the static server. The first run installs
`wasm-bindgen-cli` 0.2.128, pinned to the crate's `wasm-bindgen`, under the
cargo target dir. It installs nothing globally. The page loads a
WebAssembly module, so serve `dist/` over http. Opening it as `file://`
won't work.

## What runs in Rust

| Piece | Source | How it gets here |
|---|---|---|
| Model: `Item`, `Kind`, `Status`, `SyncStatus`, `plain_title`, `promotion_kind` | `blyg-core` | dependency, without the `keychain` feature |
| View-model: `ListModel` (omnibar/list), `counter`, `banner`, `pill`, `sync_label`, `version_label`, `highlight_ranges`, `relative_time`, `publish_decision`, `discard`, `discard_changes`, … | `crates/blyg-app/src/vm.rs` | compiled unchanged (`#[path]`), with a three-function `keymap` shim |
| Preview: `render_preview`, `article_html`, CSP, studio CSS, preview script | `blyg-render` | dependency |
| Quote resolver and snapshot sanitiser | `crates/blyg-app/src/studio/{resolver,sanitize}.rs` | compiled unchanged (`#[path]`) |
| Themes (all ten built-ins, with inheritance and derived colours) | `blyg_core::config::theme` | dependency |
| Sample posts | `crates/blyg-app/src/fake.rs` | ported to `src/seed.rs` |
| Store (search, create, save, kind, scratch notes, publish, delete, discard) | `fake.rs`'s behaviour | `src/store.rs` |

`src/lib.rs` exports one class, `Burrow`, whose methods take and return
plain strings and numbers (JSON for views), so `cargo test -p blyg-web`
runs the bindings natively. Those tests and the included `vm.rs`,
`resolver.rs` and `sanitize.rs` tests are part of `cargo test --workspace`.

## What works

- The NV omnibar: live, case-insensitive filtering with matches highlighted;
  ↑/↓ move the selection and the editor follows; ⏎ opens, or creates a draft
  seeded with the query when nothing matches; esc clears.
- The editor saves on every keystroke. About 800 ms after typing stops, the
  store is written to IndexedDB. That write stands in for the push, so the
  status bar goes "saved in this browser", then "syncing…", then "synced".
- The status bar: `◦ fragment` / `≡ thread` (click it or press ⌘T), the
  1000-character counter (amber over 900, red over 1000), the "too long"
  banner, sync state and version.
- ⌘N opens a new post. It exists only after its first keystroke, as a
  scratch note. If you leave it empty, it's deleted. ⌘D makes it a draft.
- ⌘⏎ opens the publish sheet with a version note. An over-limit fragment
  shakes the status bar instead. Publishing makes vN on a pretend
  `blyg.example.com`.
- ⇧⌘⌫ deletes a draft after a confirmation sheet. Published posts are never
  deleted.
- ⌥⌘Z discards the unpublished changes on a published post.
- ⌘E shows the live preview beside the editor. It is `blyg-render`'s
  output in a sandboxed iframe under the preview CSP. Quotes of your own
  published posts and of the sample imported post resolve, and TK spans,
  YouTube facades and unresolved quotes show as they do in the app. While
  the preview is open, the status bar shows the preview's counts.
- ⌘L focuses the omnibar. A theme menu offers Burrow's ten built-in themes.
  The page also has stacked or side-by-side layouts, an offline switch
  ("offline · N changes waiting") and a reset to the sample data.

Browsers keep ⌘N, ⌘T and ⌘L for themselves, so the page also takes ⌃ or ⌥
wherever Burrow uses ⌘.

## What's missing

The Reading view, profiles, version history (⌘Y), quick capture, AI/TK
generation, image paste and upload, the in-app browser, extensions,
settings, conflict resolution and the full editor's scroll sync aren't
here. There's no connection to a blyg. A "connect to a local `wrangler dev`
studio" mode would need the owner API from the browser, which means CORS on
the Worker and a token held by the page. Neither exists, so it isn't built.

## Why not the real GPUI window?

GPUI does have a browser platform. `gpui-kit` 0.6.6 pulls in `gpui-pre-web`
0.3.6, which draws to a single canvas with WebGPU, or WebGL2 as a fallback.
The native window still can't be compiled for the web:

- `gpui-pre-web`'s default `multithreaded` feature uses `wasm_thread`, which
  needs nightly Rust (`#![feature]`, E0554 on stable). It also needs atomics,
  `build-std`, and COOP/COEP headers on the server.
- `blyg-app` depends on crates that have no wasm build: `wry` (the preview
  and browser WebViews), `global-hotkey`, `ureq` (updates), `rusqlite` and
  `ureq` through `blyg-core`'s live backend, the Keychain, and the
  filesystem config. Separating them would need `cfg` gates through
  `app.rs`, the store and the live backend.
- `blyg-render`'s `page_shell` seeds its nonce from `SystemTime::now()`,
  which panics in `wasm32-unknown-unknown`. This crate builds the same
  document with a nonce from `crypto.getRandomValues`.

Of the GPUI-free crates, `blyg-render` builds for wasm32 unchanged.
`blyg-core` compiles once this crate turns on the browser backends of
`getrandom`, `ring` and `rustls-pki-types`. It doesn't link everywhere,
though: the C parts of SQLite and ring don't build for wasm32 with Apple's
toolchain. Nothing here opens the store or the network, so `serve.sh` links
with `--allow-undefined`. The five C functions that unused C-ABI exports
still name become `env` imports. `www/env.js` stubs them so they throw if
they're ever called. When `node` is installed, the build fails if a new
import appears without a stub.

The module is about 3.3 MB, before compression and without `wasm-opt`.
Most of it is the renderer's and sanitiser's HTML parsers and regex
engines. `serve.sh` runs
`wasm-opt -Oz` if it finds one on the PATH.
