# blyg-web (preview)

`crates/blyg-web` is Burrow in a browser and on a phone: Burrow's own Rust
(the omnibar and list rules, the counter, the Markdown preview and the
reading view-model) compiled to WebAssembly, drawn by a small page of plain
ES modules. At phone widths it has bottom tabs (Write · Read · Shelf), a ＋
for quick capture and a paged bar above the keyboard; it installs as an
offline web app.

It runs in one of two modes, and says which:

- **Live**, when it's served from a blyg at `/burrow/`, beside `/studio`
  (proposed upstream in blygger-studio#139). It works on your own posts and
  reading list through the owner API at `{mount}/api` (beside `{mount}/burrow/`; `/api` for a blyg at the host root) on the same origin, signed in
  by your Studio session cookie. There's no token and nothing to configure.
  Signed out, it asks you to sign in at Studio.
- **Demo** everywhere else: invented sample posts kept in the browser
  (IndexedDB). Nothing is sent anywhere.

```sh
./crates/blyg-web/serve.sh               # build, then serve the demo on http://127.0.0.1:8787/
./crates/blyg-web/serve.sh --build-only  # just build crates/blyg-web/dist/
```

## How live mode works

- **Detection.** `GET /api/capabilities`: a contract 5 answer is a blyg; a
  401 with the blyg's `WWW-Authenticate` challenge is a blyg you aren't
  signed in to; anything else (a 404 from a static server) is the demo.
- **Requests.** Only same-origin `{mount}/api/…` paths, with
  `credentials: 'same-origin'`. The Worker refuses a cookie request whose
  `Origin` or `Sec-Fetch-Site` is cross-site; the page's own requests pass.
- **Contract 5.7.0.** Lists are read page by page to `nextCursor: null`
  and restarted on 409 `cursor-expired`, so a list is never partial. Edits
  are `PATCH /api/items/{id}` with `expectedRevision`; publishing is
  `POST /api/items/{id}/publish`, and only ever because you chose it. The
  request shapes and the error mapping are in `src/api.rs`, on blyg-core's
  wire types; the store and outbox in `src/live.rs`.
- **Offline and the outbox.** Every keystroke is in the store at once and
  persisted (IndexedDB key `live`, apart from the demo's `store`). After a
  pause the outbox sends what waits: edits (each with the revision it was
  based on), new drafts, draft deletes, read marks and hopper changes.
  Offline, they wait, survive a reload, and replay when the blyg answers.
- **Refreshing.** On start, and when the page comes back after a minute,
  it asks `GET /api/changes` first and reads again only the lists whose
  revisions moved since they were last read (`changes_plan` in
  `src/live.rs`, on the native sync's dependency lists), and everything
  every 10 minutes as the native sync does. Without a usable answer it
  reads everything, as before.
- **Conflicts.** A 409 `revision-conflict` fetches the blyg's copy and
  shows it beside yours: "Keep mine" sends yours over it, "Use theirs"
  takes it. Nothing is overwritten without that choice.
- **The service worker** caches only the app's own files under its scope
  and passes every other request, `/api/` and `/studio/` above all,
  straight to the network.
- The shelf stays in the browser until the studio has one
  (blygger-studio#135).

## Staying fast with thousands of posts

A blyg can hold hundreds of your posts and thousands to read, and a phone
has to stay cool with all of them. So:

- Only the screen you're on is drawn. The reading list and the post list
  are drawn a page at a time (`Burrow.reading_page` takes a limit and
  says how many match), with more as you scroll. `draw()` in `core.js`
  skips a redraw that would change nothing.
- Nothing under an open post is redrawn: a read mark landing on the blyg
  updates the counts, and the stream is redrawn when you go back to it.
- The store's snapshot (`live`) holds your posts, the outbox and your
  marks, and is written on every change. The reading list has its own
  (`live-reading`, `Burrow.reading_json`), written only when it changed:
  after a refresh read it, and when the page is hidden.
- A hidden page does no work: the 30 s redraw for relative times waits
  until it's on screen.

`crates/blyg-web/phone-check.mjs` checks the phone's Read tab in WebKit
with iPhone emulation: a page of cards, a post that stays shown after the
outbox's send, a redraw and a second open, nothing rebuilt under it, and
no long stall while it opens. Run it against the demo, or live against a
disposable local blyg (its header says how).

To develop against a disposable local blyg (never a real one), run the
Worker with throwaway state on port 8790, then
`python3 crates/blyg-web/dev-proxy.py`. It serves `dist/` at
`http://localhost:8791/burrow/` and forwards everything else to the local
Worker with the browser's own `Host` and `Origin`, so the same-origin rules
behave as on a blyg. Sign in at `http://localhost:8791/studio/login`.

The build is reproducible (byte-identical `dist/` from two clean builds)
with a pinned toolchain, `Cargo.lock`, wasm-bindgen and wasm-opt. Size
(wasm, `wasm-release` profile): about 1.2 MB gzip. The crate's
`README.md` has the details: what runs in Rust, the size breakdown, and how
to try it on a phone.
