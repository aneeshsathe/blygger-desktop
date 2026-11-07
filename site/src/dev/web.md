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

## Writing on your blyg

<!-- web-writing -->
The writing side of Studio, in both layouts. On a phone it's in the
editor's bar (🖼 on the third page; ⋯ and ✦ on the last), the editor's ⋯
button (this post's sheet), ⑂ on a post you read, and ⚙ in the ◐ sheet. On
a desk: the same bar, `⋯ Post` in the status bar (⌃⇧Y opens the versions),
`⑂ Fork` over a post you read, and `⚙ Your blyg` in the header. The rules
are in `src/writing.rs` (`w_*`), the drawing in `www/writing.js`.

- **Images.** From the file picker (on a phone, the photo picker), a
  paste or a drop, with alt text. The upload is `POST /api/media`
  (multipart: `file`, `alt`, `inline=true`), unattached as the app's are;
  a numbered placeholder holds the place, and the answer's URL replaces
  it as `![alt](url)`. An image over the blyg's 5 MB, or of a type it
  doesn't take, is made a JPEG in the browser first. Uploads need a
  connection: offline (or in the demo) it says so and nothing is queued.
  The sandboxed preview can't send your cookie, so this session's uploads
  show there from a small `data:` copy.
- **Versions.** `GET /api/items/{id}/versions` (paged) lists them with
  what each can do: view one as published or as a word diff against the
  text now (`GET …/versions/{v}`), restore it into the editor, pin it
  (`PUT …/versions/{v}/pin`, after typing *pin*: a pin is public forever),
  or fork it. Withdrawing (`POST …/withdraw`, with an optional note) is
  confirmed in the app; deleting a published post offers it instead.
- **Forking** a pinned version, yours or one you read, asks
  `GET /api/fork-options` for the pins and makes the new draft with
  `POST /api/items {mode: "fork", source}`.
- **TK.** The ✦ sheet lists the post's `[TK]…[/TK]` scopes and has the
  blyg write each one (`POST …/generate {scope, expectedRevision}`);
  the post is read again afterwards, as after a publish.
- **The publish sheet** offers an AI-drafted note (`POST …/note-draft`)
  and the change kind (`POST …/classify`) when the blyg is set up for them
  (`aiModelChangelog`, `aiModelClassify`) and the post is already out.
  You edit and confirm both; a drafted note you didn't change is sent as
  `noteGenerated`, the kind as `changeCategory`, and the kind leads the
  note when the blyg's `changeCategoryInNote` says so.
- **Settings.** Title, author, bio, links, `authorUrl`, `siteUrl`, time
  zone and the writing defaults, saved with `PATCH /api/settings` when
  you press Save (only what changed).
- **`@handle`s.** The `@` list offers your contacts (`GET /api/contacts`,
  read once after start) beside the blygs you read, and the preview shows
  a contact's `@handle` as the published page does: a link with its
  h-card.
<!-- end web-writing -->
