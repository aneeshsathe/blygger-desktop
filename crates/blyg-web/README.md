# blyg-web: Burrow in a browser, and on a phone

Try Burrow's main window in a browser tab, or Burrow as a phone app (any
window 700px wide or less, or the installed web app). The rules come from Burrow's own
Rust, compiled to WebAssembly: what the omnibar matches, what ⏎ does, the
counter, the labels, when a post may be published, deleted or discarded, and
the Markdown preview. A small page of plain ES modules draws the window.

Served from a blyg at `/burrow/`, it runs **live** on your own posts and
reading list (see [Live mode](#live-mode)). Anywhere else it's the
**demo**: the invented sample posts that `BLYGGER_FAKE=1` uses, stored in
this browser (IndexedDB), with nothing sent anywhere. The header says
which.

```sh
./crates/blyg-web/serve.sh            # build, then serve on http://127.0.0.1:8787/
./crates/blyg-web/serve.sh --build-only   # just build crates/blyg-web/dist/
WASM_OPT=1 ./crates/blyg-web/serve.sh     # also run wasm-opt -Oz (see Size)
```

To try the phone app on a phone, serve on your network (for example
`python3 -m http.server -d crates/blyg-web/dist 8787 --bind 0.0.0.0`) and
open it on the phone. Installing it and the offline copy need https, or
localhost.

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
| Reading view-model: order, visibility, titles, hosts, badges, dates, reading search, reply stubs, partial quotes, quote labels | `crates/blyg-app/src/reading/vm.rs` | compiled unchanged (`#[path]`), with small shims in `src/reading/mod.rs` |
| Sample posts | `crates/blyg-app/src/fake.rs` | ported to `src/seed.rs` |
| Sample reading (subscriptions, posts, hoppers) | `crates/blyg-app/src/fake_reading.rs` | compiled unchanged (`#[path]`) |
| Shelf (📚, 50 at most, shelved first), read state, hoppers, reply stubs | the `shelf` branch's behaviour | `src/store.rs`, `src/read_api.rs` |
| Store (search, create, save, kind, scratch notes, publish, delete, discard) | `fake.rs`'s behaviour | `src/store.rs` |
| Live mode: owner-API requests, answers, paging, error mapping; the outbox and conflicts | `blyg_core::api` (wire types, `contract`, `imported_row`) | `src/api.rs`, `src/live.rs` (the page's `live.js` only fetches) |

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

- Posts | Reading (⌃1 / ⌃2). Reading lists what you read, newest first,
  with unread dots and lineage glyphs. Open a post to read it with its
  lineage (what it replies to, quotes or forks, and who drew on it). From
  there: ❝ Quote, ↩ Reply, ⊕ Hopper, [[ Copy link, mark unread, A− / A+.
  Hold a sentence to quote, reply to or link just that passage.
- The shelf: 📚 marks shelved posts, which come first, with a line under
  them. ⌥⌘B shelves or unshelves the open post, ⌘B lists the shelf. It
  holds 50 at most.
- ⌘K links a post at the caret, ⇧⌘K quotes one (threads only).
- The editor bar under the editor is the phone's, described below. It works
  with a mouse: drag or scroll it sideways, or use the ‹ › arrows.

Browsers keep ⌘N, ⌘T and ⌘L for themselves, so the page also takes ⌃ or ⌥
wherever Burrow uses ⌘.

## On a phone

Under 700px wide the page is a phone app, after the approved prototype:

- **Bottom tabs** Write · Read · Shelf, and a floating **＋** for quick
  capture. Capture is a bottom sheet with the 1000-character counter,
  **Keep as note** (a scratch note) and **Publish** (offline, it's kept).
- **Write**: a search box ("Search or start a post"; no match offers
  "＋ Start …"), the shelf strip, then Posts. Hold a post to shelve it.
- **The editor bar** rides above the on-screen keyboard. It's paged, like
  Apple Notes' tool screens: swipe it sideways and it snaps a page at a
  time, with dots under it, and Publish stays at the right.
  Page 1: [[ link · ❝ quote · ◦⇄≡ · TK · 📚 · 👁.
  Page 2: bold, italic, heading, bullets, numbers, block quote, code.
  Page 3: code block, web link, image, rule, strikethrough, hide keyboard.
  Tools don't take focus, so the keyboard stays up.
- **Read**: the stream. Swipe a card right to mark it read; swipe it left
  for ❝ Quote · ↩ Reply · ⊕ Hopper; tap to read. "● N" shows only unread.
  In a post, **hold a sentence** for ❝ Quote · ↩ Reply to this · [[ Link.
  A reply is a stub thread quoting just that passage.
- Text is sized in rem, so the system text size (Dynamic Type on iOS) and
  A− / A+ reflow the reader. Safe-area insets, 44px tap targets and
  `prefers-reduced-motion` are respected. The system back gesture closes
  the editor and the post.

The app pins itself to the `visualViewport`, which shrinks when the
keyboard opens on iOS Safari and Android Chrome. That keeps the editor bar
above the keyboard and hides the tabs while you type.

### Install and offline

It's an installable web app: `manifest.webmanifest` (standalone, the repo's
app icon at 192, 512 and SVG), Apple touch icon and status-bar tags, and a
service worker (`sw.js`). `serve.sh` stamps the worker with a hash of the
build and the list of files, so it caches the whole shell, the wasm, fonts
and icons, and each build replaces the last. Once opened, Burrow opens with
no network. Your posts stay in IndexedDB, not in the worker's cache.
Installing needs https, or `localhost` / `127.0.0.1`.

The worker caches only this app's files under its scope. It passes every
other request, `/api/` and `/studio/` above all, straight to the network
(a worker sees every fetch its pages make, wherever it goes, so scope alone
wouldn't keep the API out).

## Live mode

Served from a blyg at `/burrow/`, the page shares an origin with the
Studio (`/studio`) and the owner API (`/api`, at the host root), so it uses
the Studio's session cookie (`HttpOnly`, `SameSite=Lax`, `Path=/`): there's
no CORS, no token and nothing to configure, and the page never sees the
cookie.

- **Which mode.** `GET /api/capabilities` decides (`api::detect`): a
  contract 5 answer is a blyg; a 401 with the blyg's `WWW-Authenticate`
  challenge is a blyg you aren't signed in to ("Sign in at Studio" opens
  `/studio/login` in a new tab; come back and the page catches up); any
  other answer (a static server's 404) is the demo. No answer at all, with
  a live store already in this browser, opens that store offline.
- **Only `/api/`.** Every request goes through `live.js`'s `call`, which
  refuses anything but a same-origin `/api/…` path, with
  `credentials: 'same-origin'`. The Worker refuses a cookie request whose
  `Origin` isn't its own or whose `Sec-Fetch-Site` is `cross-site` (403);
  the page's own requests pass. Nothing logs a header.
- **Contract 5.7.0.** `src/api.rs` builds the requests and reads the
  answers on blyg-core's wire types (`WireItem`, `CursorPage`,
  `imported_row`, `contract`): errors are `{code, message, details}`
  (401 → sign in, 409 `revision-conflict`, 409 `cursor-expired`, 429
  with `Retry-After`, validation `details.issues`); lists are read page by
  page to `nextCursor: null` (`Pager`), and an expired cursor drops what
  was read and starts over (at most `CURSOR_RESTARTS` times), so a list is
  never partial.
- **What it reads.** `GET /api/settings` (`siteUrl`, for permalinks),
  `/api/items`, `/api/reading/imported` and `/api/hoppers`, on start,
  after reconnecting, and when the tab comes back after a minute.
- **What it writes.** `POST /api/items` (a new draft, or a reply stub
  with `stubOf`), `PATCH /api/items/{id}` (`expectedRevision`,
  `contentMd`, and `kind` until first published), `DELETE` a draft,
  `POST …/publish` (`expectedRevision`, `note`; only from the publish
  sheet or capture's Publish), `POST …/restore` (Discard changes),
  `PUT`/`DELETE /api/reading/{sub}/{id}/read` and
  `PUT`/`DELETE /api/hoppers/{id}/items/{sub}/{id}`. Scratch notes stay
  in the browser until ⌘D makes them drafts, as natively. The shelf stays
  in the browser (the studio has none yet, blygger-studio#135).
- **The outbox** (`src/live.rs`). Every keystroke is in the store at once
  and persisted (IndexedDB key `live`; the demo's is `store`, so they
  never mix). About 800 ms after typing stops, the outbox sends what waits:
  each edited item once, with the revision its edit was based on; then
  draft deletes, read marks and hopper changes. Typing while a save is in
  flight keeps the item pending for the next send. Offline (no answer, or
  the browser says so), the status bar says "offline · N changes waiting";
  the outbox survives a reload and replays when the blyg answers (the
  `online` event, the tab coming back, or every 20 s).
- **Conflicts.** A 409 `revision-conflict` stops that item, fetches the
  blyg's copy and keeps it beside yours: a banner over the editor shows
  its revision and opening words, with **Keep mine** (yours is sent, based
  on their revision) and **Use theirs**. Nothing is overwritten without
  that choice. A fresh list never overwrites an item with unsent edits; an
  item deleted elsewhere under an edit keeps your text as a new draft; a
  draft deleted here but changed elsewhere isn't deleted.

### Developing against a local blyg

Never against a real one. Run the Worker locally with throwaway state
(`wrangler dev` on port 8790), build, then:

```sh
./crates/blyg-web/serve.sh --build-only
python3 crates/blyg-web/dev-proxy.py      # http://localhost:8791/burrow/
```

`dev-proxy.py` serves `dist/` at `/burrow/` and forwards everything else
to the local Worker with the browser's own `Host`, `Origin` and
`Sec-Fetch-*` headers, so the same-origin guard behaves as on a blyg (a
cross-origin request is still refused). It refuses a non-loopback
upstream. Sign in at `http://localhost:8791/studio/login`. Publishing on
the local Worker still fetches the URLs a post links to (webmention
discovery), so publish test posts without links.

## What's missing

Profiles, mentions, subscriptions, version history (⌘Y), AI/TK
generation, image paste and upload, the in-app browser, extensions,
settings, withdrawing a post and the full editor's scroll sync aren't
here. Conflicts offer mine-or-theirs, not a merge.

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

## Size

`serve.sh` builds with the workspace's `wasm-release` profile
(`opt-level = "z"`, fat LTO, one codegen unit, `panic = "abort"`,
stripped). Measured on the module after `wasm-bindgen`:

| Build | raw | gzip -9 | brotli -11 |
|---|---:|---:|---:|
| `release` profile | 3,649,271 | 1,327,435 | 958,684 |
| `release` + `wasm-opt -Oz` | 3,361,353 | 1,341,089 | 980,660 |
| `wasm-release` (what `serve.sh` ships) | 3,056,642 | 1,104,967 | 832,204 |
| `wasm-release` with live mode (`api`, `live`) | 3,251,424 | 1,168,940 | 875,655 |
| `wasm-release` + `wasm-opt -Oz` | 2,762,061 | 1,153,243 | 866,324 |

`wasm-opt` (Binaryen 116, from the pinned `wasm-opt` crate) makes the raw
module about 10% smaller but the compressed one about 4% larger, at -Oz, -Os
and -O2 alike. That's why it is opt-in (`WASM_OPT=1`).

What dominates, from `twiggy` on a build that keeps names: one data segment
of 1.25 MB (41%) and 1.8 MB of code. The data is lookup tables: HTML
entities (`html5ever`, `markdown-it`), Unicode classes (`regex-syntax`),
the legacy text encodings (`encoding_rs`, through `lol_html`) and IDNA
(`url`). By crate, the code is `regex-automata` 12%, `regex-syntax` 5%,
`fancy-regex` 5%, `aho-corasick` 4%, `html5ever` 5%, `lol_html` 4%,
`encoding_rs` 4%, `markdown-it` 4%, `serde_json` 3%, and Burrow's own
crates about 8%, with `core`/`alloc`/`hashbrown` generics making up most of
the rest. SQLite, ureq, rustls and ring don't appear: LTO drops them.

Ways to shrink it further, none of them done here: one HTML parser instead
of two (`ammonia`'s `html5ever` for the sanitiser, `lol_html` in the
renderer); `regex` without its Unicode tables where patterns are ASCII;
dropping `fancy-regex` for plain `regex`; and moving the sample data out of
the module.

The build is reproducible: two clean builds in different target dirs gave
byte-identical `dist/` trees (the wasm, the service worker's stamp and the
icons), and `wasm-opt` is deterministic too. Source paths are remapped, so
the module names no home directory or checkout. It stays reproducible with
the same Rust toolchain, `Cargo.lock`, `wasm-bindgen` and `wasm-opt`
versions and `sips` (the icons). Another toolchain, a lock-file update, or a
`CARGO_HOME` / checkout path containing spaces (which the flags don't
quote) would change the output or break the build.
