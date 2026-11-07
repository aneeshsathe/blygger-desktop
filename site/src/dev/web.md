# blyg-web (preview)

`crates/blyg-web` is Burrow in a browser and on a phone: Burrow's own Rust
(the omnibar and list rules, the counter, the Markdown preview and the
reading view-model) compiled to WebAssembly, drawn by a small page of plain
ES modules. At phone widths it has bottom tabs (Write · Read · Shelf ·
Blyg), a ＋ for quick capture and a paged bar above the keyboard; it
installs as an offline web app.

## Where things are

Every feature sits where you'd look for it, by name (an icon and a word),
not behind a corner menu:

| What | Desk (wider than 700px) | Phone |
|---|---|---|
| This post: shelve, preview, image, TK, versions, make draft, discard, withdraw, delete | the action row over the editor (each with its key) | 📚 Shelf · 👁 Preview · ⋯ Post in the editor's top bar; ⋯ Post and ✦ Write TK on the bar's last page |
| A post you read: quote, reply, hopper, copy link, unread, fork, text size | the row over the post | the row under the post's top bar |
| Sources, subscribe, subscriptions, mentions, hoppers, peek, history | the sources pane, named at its top | the row under the Read tab's search |
| Your blyg's settings, mentions, hoppers, subscriptions, peek, history, shelf, theme (and the demo's offline switch and reset) | the ⚙ Blyg view (⌃3) | the Blyg tab |

Every keyboard shortcut still works; the desk's keys line lists them.
Destructive actions (delete, discard, withdraw, unsubscribe, deleting a
hopper, clearing history) still ask first.

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

<!-- --- web-reading --- -->
## Reading: sources, subscriptions, mentions, hoppers

The Read side does what Studio's does (0.19.0):

- **Sources.** On a desk, a pane left of the stream; on a phone,
  ☰ Sources in the row under the Read tab's search, with a "Showing …"
  line under it (✕ shows everything again). The smart feeds (All unread, Today, Thumbed, All, Background) with
  their private counts, Mentions (posts that mention you, as the blyg's
  `GET /api/reading?mentions=true` lens says, else worked out from what is
  held), your hoppers and your subscriptions, each with a dot when there's
  something to read, and the Threads / Fragments lens. The rules are the
  app's `reading/sources_vm.rs`, compiled unchanged; withdrawn posts show in
  Background only, as in the app.
- **Subscriptions.** Subscribe with a preview (a blyg's or a feed's address;
  `wholeBlyg` on a 5.1+ studio, so a hopper's address follows the whole
  blyg), each subscription's state (checked when, paused, or "having
  trouble" with its failed checks and the last error the studio flagged),
  rename (or give it back its own name), pause and resume, blogroll on or
  off, read again (resync), unsubscribe, check all now, OPML import (a file
  you pick; the feeds you follow already are marked, and it subscribes to
  the rest one at a time, waiting out a 429) and OPML export.
- **Mentions.** Responses to your posts, grouped under the post (newest
  first, a dot for new ones, never a count), each with its source and
  state; hide one or show it again; what you sent (outbound Webmentions with
  their state and last error); and Accept Webmentions (`acceptMentions`).
- **Hoppers.** Make, rename, describe (280 characters), make public (with
  its `/h/{slug}/` page) or private, delete (its posts stay).
- **Thumbs.** 👍 / 👎 under a post you read (`PUT`/`DELETE
  /api/signals/{sub}/{id}`), shown at once and taken back if the blyg
  refuses; Thumbed lists the 👍 ones.
- **Peek.** A post's address on a blyg you don't follow opens in a panel
  (`POST /api/peek/links`, then `GET /api/peek`), with "Subscribe to …". A
  post you hold opens in the reader instead. Links in posts you read that
  are blyg posts open in Peek too (one `POST /api/peek/links` per post,
  after it's shown). Each peek is logged to the studio's history (`POST
  /api/browse`), which History lists and can clear.

In the demo the same screens change the sample data in the browser and
send nothing. None of it adds work on start or on a keystroke: a list is
read when its screen first opens (and again after a minute or so), the
sources pane is drawn only with the reading view, and the stream's filter
is one pass over the rows it already orders. The code is
`src/web_reading.rs` (rules, requests, answers) and `www/web-reading.js`
(drawing), with hook lines marked `--- web-reading ---` in the shared files.
<!-- --- end web-reading --- -->

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
- The rows of named actions cost nothing on a keystroke: the desk's row
  over the editor is drawn from the view already in hand and replaced only
  when it changes, and the Blyg list is drawn only while it's on screen.

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
editor's bar (🖼 on the third page; ⋯ Post and ✦ Write TK on the last), the
editor's `⋯ Post` button (this post's sheet), `⑂ Fork` in the row under a
post you read, and Settings on the Blyg tab. On a desk: the same bar, the
action row over the editor (⌃⇧Y opens the versions), `⑂ Fork` over a post
you read, and Settings in the ⚙ Blyg view (⌃3). The rules
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
