# Burrow — spec

Burrow is a blygger client (called Blygger Desktop up to 0.6.0; only the
displayed name changed, see "Naming" below). A native macOS client for a Blygger blog ("blyg") running the Blygger reference
Worker (blygger-studio) 0.49.12 or later, stock and fully supported: Burrow speaks
the studio's private API contract 5.x only (`blyg_core::api::CONTRACT_MAJOR`). On an
older blyg (or a Worker fork) nothing syncs: local changes wait until the studio is
updated, and Burrow says so once, with a status-bar notice. `docs/SERVER.md` has the
details. Goal: **zero
friction between a thought and a published post.** The feel target is
Notational Velocity: one window, type to search, ⏎ to create, nothing ever
waits on the network.

The agreed interaction design is the clickable mock in `docs/prototype/index.html`
(open it in a browser, and play the ▶ demos). When this document and the mock
disagree on *feel*, follow the mock. On *API behaviour*, this document wins.

## Decisions (settled)

| Area | Decision |
|---|---|
| UI | **GPUI** (Zed's framework) + `gpui-component` where it helps (text input/editor). |
| Local data | **SQLite** via `rusqlite` (`bundled`, FTS5). Local-first: UI reads only from SQLite. DuckDB was considered and declined because this is an OLTP/tiny-write workload, and DuckDB can read the SQLite file later for analytics. |
| Network | Blocking HTTP (`ureq`, rustls) on background threads. No tokio in the app. |
| Auth | Base URL + a bearer: **Sign in with browser** (OAuth, PKCE, loopback redirect; the default), else an API token (Studio → More → Client access), else the studio password. Credentials live in the **macOS Keychain** (Windows Credential Manager on Windows; `keyring` crate), never in a file. See `crates/blyg-core/src/api/oauth.rs`. |
| Platform | macOS first; Windows (x64) is supported too, from the same code: platform code sits behind `cfg(target_os = …)`, mostly in `crates/blyg-app/src/platform.rs`, and the Windows build must keep building and passing CI (see `site/src/dev/building.md`, "Keeping Windows building"). |
| Layout | **Side by side**: item list on the left (~38%), editor on the right. |
| Theme | **Tufte colours** (paper `#fffff8`, ink `#111`, accent `#a4271b`; dark: `#161513` / `#e4dfd3` / `#e0775a`), follows system light/dark. Default in the mock was dark. |
| Fonts | ET Book reads poorly on screen per the user. **Fonts are user-switchable in the app** (separate choices for writing font and UI font) with screen-first defaults: writing = **Literata** (bundled, OFL), UI/list = **Inter** (bundled, OFL). Offer at least: Literata, Source Serif 4, iA Writer Quattro (bundled, OFL); system New York, Charter, SF Pro, Menlo; ET Book stays available as an option. Size adjustable (⌘+ / ⌘−). |
| Quick capture | Global hotkey, default **⌃⌥B**, **configurable** in settings. |
| Scope | Everything in §Scope is in v1. |

## Interaction spec (main window)

- **Omnibar** at top, always there. Typing filters the list live (substring,
  case-insensitive, matched text highlighted). `↑/↓` move the selection, and the
  editor previews the selected item as you move (NV behaviour).
  `⏎` opens the selection; **`⏎` with no matches creates a draft seeded with the
  query** and puts the caret at the end of it. `esc` clears the query.
- **Editor**: plain Markdown text, no toolbar. Every keystroke saves locally;
  push to the server ~800 ms after typing stops. There is no save button.
- **Status bar** (bottom): `◦ fragment` / `≡ thread` (clickable, toggles),
  counter `214 / 1000` (amber >900, red >1000, for fragments only; threads show
  "N chars · no limit"), a centre banner when over the limit ("Too long for a
  fragment · ⌘T makes it a thread"), sync state (`synced` / `saved on this Mac`
  / `syncing…` / `offline · N changes waiting`), version (`draft` / `public v3`
  / `public v3 · unpublished edits`).
- **List rows**: title (first line), then `◦ fragment`/`≡ thread`, a status pill
  (`draft` or `vN`), a dot for unpublished edits, and relative time.
- **New post** (`⌘N`, Post › New Post): opens a fresh, empty editor with the caret
  in it, without touching the omnibar: its text, the list's filter and its
  scroll stay as they were. No row shows as selected while the new post isn't
  in the list (it's selected once it's listed); ↑/↓ go on from the row selected
  before ⌘N, which opens that post and leaves the new one. Nothing exists until the first real keystroke,
  which creates a local scratch note (`Backend::create_scratch`; a draft with
  `new-note = draft`) that then autosaves like any edit. `⌘D` makes it a draft
  (same local id), `⌘⏎` publishes it through the publish sheet. Leaving it while
  it's empty (another post, a search, `⌘N` again) deletes it, so there are no
  blank items. While it's open the status bar says "New note · saved locally ·
  ⌘D draft · ⌘⏎ publish" ("New draft · syncs to your blyg · ⌘⏎ publish" with
  `new-note = draft`). The omnibar's create (`⏎` with no match) is unchanged.
  Code: `crates/blyg-app/src/new_post.rs`.
- **Shelf** (`shelf.rs`, `store/shelf.rs`; user request 2026-11-06): your own
  posts kept to hand, for the living posts you keep updating. **⌥⌘B** (Post ›
  Put on Shelf / Take Off; Ctrl+Shift+A on Windows, since ⇧⌘B is the browser)
  shelves the open post or takes it off, with a toast ("On your shelf · ⌘B" /
  "Taken off your shelf"). Shelved posts that match the omnibar come first in
  the list, in shelf order (`vm::ListModel::set_shelf`), each with 📚 before its
  title, and a hairline under the last one; the rest keep the backend's order.
  **⌘B** (Post › Shelf…) drops a sheet listing the shelf: 1–9 / ⏎ open a post
  (clearing a search that hides it), ↑/↓ move, ⌥↑/⌥↓ reorder, ⌫ takes the
  highlighted post off, esc closes. Any post of yours (thread, fragment, draft,
  scratch note; a scratch note keeps its place when promoted, same `LocalId`);
  withdrawn posts are left out, deleted ones drop out by cascade. At most 50 posts
  (studio's `PUT /shelf` limit; a 51st is refused with a toast). **Local
  only for now**: store table `shelf(local_id, position)` (schema v17),
  `Backend::shelf` / `set_on_shelf` / `move_on_shelf`, never queued or sent.
  blygger-studio#137 (closes #135; contract 5.8.0, unreleased as of
  2026-11-06) adds owner-only `GET/PUT {mount}/api/shelf` →
  `{itemIds, revision}` (≤ 50 ids; `expectedRevision`, else 409
  `revision-conflict` with `details.current`; unknown and withdrawn ids
  dropped; change domain `shelf`; 404 below 5.8). Burrow moves onto it once
  released, keeping scratch notes and unsynced drafts local. Named "shelf", never "pin": a pin is
  a public, permanent version pin (§ Versions & pins).
- **Lineage while writing** (`lineage_strip.rs`; proposal 2, 2026-11-06): under
  the editor on Posts, one quiet 26px line for the open post: its lineage
  glyph (the same `lineage_vm::Glyph` as the stream's, counted by the same
  rules) and "draws on N · drawn on by M", plus "links to K posts" when its
  text has `[[id]]` links (listed, never counted: the glyph counts forks,
  stubs and quotes only, rule 1's exception). Nothing named, nothing shown. A
  published post's sides come from `lineage::Local` (reading rows, your
  published posts, verified mentions of yours, the same sources as the
  Reading lineage); a draft's "draws on" is read live from its text
  (`Item::references`), re-derived only when its references change (a
  fingerprint of its directive ids, not every keystroke). A click on the line
  opens three short lists (Draws on / Drawn on by / Links to, five each); a row
  opens the post: yours in the editor, anyone else's in Reading (a held row at
  once, else the token-less public fetch, as "open original" does). **⌘J** on
  Posts opens the lineage sheet for your own published post; on a draft it
  opens the lists. Built on the main thread from the Reading view's rows when
  they're loaded, else off it (`Backend::reading`, `items`).
- **Hover cards and ⌘-click** (`hover_card.rs`; proposal 4, 2026-11-06): resting
  the pointer (350 ms) on a `[[id]]`, `[[id | label]]` or `![[id | label]]` in
  the editor shows a card: title, author or "You", version, the first lines,
  and for an unlabelled link to a live post **label it**, which writes the
  target's title as the label (`[[id | title]]`). ⌘-click (Ctrl-click on
  Windows) opens the target: your own post in the editor, anyone else's in
  Reading through "open original". The card reads only what this Mac holds
  (your posts, then reading rows); it never fetches. A withdrawn post shows as
  withdrawn, with no excerpt and no "label it". A quote of an older version of
  someone else's post shows the held current text with "Quoted at vN; this is
  vM": an unpinned past version is never shown. The stream's quote boxes and
  `[[id]]` links show the same card as a tooltip. The WebView reader pane
  doesn't (its clicks already open the original).
- **Reading actions on your own post** (`reading/own.rs`): a published post of
  yours shows a row of chips above the editor, "Your post · vN ·" then **Reply ·
  new stub** (⌥⌘R), **Quote into a thread** (⌥⌘Q) and **Link post · new
  fragment** (⌥⌘L), each with the Reading view's sentence and its key in the
  tooltip; the same three are in the Post menu. They run the Reading view's own
  code on the post's reading row (`item_action`), so a self-stub is a stub like
  any other: `stub_of` names your origin (protocol 0.4 §10.6, "a citation is
  absolute"), the body opens with `![[id]]`, and the stub editor's passage
  chooser shows your post. Quote puts `![[id | label]]` in a thread (its
  `transclusions[]` entry leaves `origin` out, §10.3); Link post makes a
  fragment holding `[[id]]`. Drafts and scratch notes show no row, and the keys
  say to open a published post. In the Reading view the same keys act on the
  open post (else the stream's selected one). There is no Fork here: your own
  post is forked only from ⌘Y (§ Versions & pins).
- **Keys**: `⌘L` focus omnibar · `⌘N` new post · `esc` (in editor) back to omnibar ·
  `⌘⏎` publish · `⌘T` fragment⇄thread · `⌘E` preview (see § Full editor) · `⌘O` open
  permalink in browser · `⌘,` settings · `⌘+/⌘−` font size · `⇧⌘⌫` delete a
  draft or scratch note · `⌥⌘Z` discard a published post's unpublished changes.
- **Delete / Withdraw** (rule 5 below): `⇧⌘⌫` (Post › Delete Draft…) on a draft
  or scratch note drops a sheet: "Delete this draft? … This can't be undone."
  `⏎` deletes, `esc` cancels; the list moves on to the next post and a toast
  says "Draft deleted". Published posts are never deleted: `⇧⌘⌫` on one says
  so, and Post › Withdraw… asks instead, with an optional note ("Withdrawn
  posts stay listed as withdrawn. You can't undo this."), then calls
  `withdraw(id, note)`. Withdraw has no key on purpose, since it's permanent.
  (`⌘⌫` stays delete-to-line-start in the editor.)
- **Discard changes** (`⌥⌘Z` on macOS, and no key on Windows, where Ctrl+Alt+Z
  is AltGr+Z and types a letter on some layouts; `keymap::WINDOWS_KEYLESS`.
  Post › Discard Changes…, the toolbar's Discard,
  and "Discard changes…" on the current version in ⌘Y): a published post with
  unpublished edits goes back to the version it has published. A sheet asks
  first ("Discard unpublished changes? “…” The editor goes back to vN as
  published. You can't undo this."), `⏎` discards and `esc` keeps the edits;
  then `restore(id, item.version)` and a toast "Back to vN as published". This
  is upstream's "discard changes" (blygger-studio `restoreVersion` on the
  current version): the public post doesn't change, and the copy is clean
  again, except when vN disclosed generated text, whose provenance a restore
  can't carry back, so the blyg keeps it marked unpublished (the toast says
  so, and the list's dot stays). It's disabled, with the reason as its tooltip
  or toast, for a draft or scratch note (Delete covers those), a withdrawn
  post, a post with no edits, and offline ("Discard needs the blyg"). The
  places that say a post has edits name both ways out on hover: the status
  bar's "public vN · unpublished edits" and the list row's edit marker show
  "⌘⏎ publishes them as vN+1 · ⌥⌘Z discards them" (`vm::edits_tip`, through
  `keymap::hint`; on Windows, "Ctrl+Enter … · Discard Changes in the Post
  menu"). The toolbar's Discard is an eraser, so it doesn't read as Undo.
  Local-first: core's `restore` holds the sync lock, so no push of the old
  text runs during it, and drops the item's waiting outbox ops when it reloads
  the item; while the restore is on its way the editor doesn't save
  (`MainView::restoring`, checked in `after_edit`), so keystrokes typed
  meanwhile can't land on top of the restored copy; then the editor gets the
  restored text without saving. If the restore fails nothing was replaced, and
  what was typed meanwhile is saved then. Every restore (⌘Y's Restore vN… too)
  goes through this path, `discard::restore_working_copy`.
- **Publish** (`⌘⏎`): a sheet drops from the title bar: "Publish "…" as vN+1",
  with an optional version note input. `⏎` publishes and `esc` cancels. If the
  item is a fragment over 1000 chars, don't open the sheet: shake the status bar
  and say ⌘T. A toast shows the result: "Published v2 · blyg.example.com/f/… · ⌘O opens it".
- **Paste a link over selected text** (as WordPress does): when the selection
  is on one line and the clipboard holds only a web or `mailto:` address, ⌘V
  makes it `[text](url)` (brackets escaped, `<…>` around an address with
  parentheses; spaces caught in the selection stay outside). An address or an
  existing link that's selected is simply replaced. One undo step.
- **@-mentions** (editor and quick capture): typing `@` at the start of a word
  opens a small popup under it, filtered as you type, of the blygs you know
  (subscriptions, blogrolls, authors in your reading, profiles already fetched;
  matched on author name, site title and host; recently read and quoted ones
  first). `↑/↓` choose, `⏎`/`⇥` insert, esc closes it and keeps the typed text.
  Not in code or on a paste. There's no identity layer (rule 6), so the pick is
  a plain link, `[Author or title or host](https://blyg.example.org/)`, and
  nothing else: a plain link notifies nobody.
- **Spellcheck** (`spellcheck = true`, Edit › Spelling › Check Spelling While
  Typing): the macOS spell checker (system languages, learned words), a red
  wavy underline, and a menu on right-click/ctrl-click with suggestions, Learn
  Spelling and Ignore. Code, links and their targets, addresses, `![[id]]`, the
  TK markers and front matter are skipped. It runs ~300 ms after typing stops,
  only on changed lines, off the UI thread; typing never waits on it.
- **Paste/drop an image**: this inserts `![uploading…]()` at the caret, uploads
  via `POST /api/media` (without `item_id`), then replaces the placeholder with
  `![](<blyg origin>/media/<id>.<ext>)`. Two Worker behaviours decide this: public
  pages append every *attachment* (media with the item's id) after the text, so
  an attached image that's also inline shows twice; and a relative `media/…` in
  the text renders page-relative, which 404s under `/f/<id>/`. If the placeholder
  is gone by the time the upload finishes, the file is deleted again
  (`DELETE /api/media/:id`).
- **Conflict** (the server changed since the last sync and there are local edits):
  a sheet with a side-by-side view "On this Mac" vs "On the server", with keys `1`
  keep mine / `2` take the server's / `3` keep both.
- **Offline**: nothing blocks. The status shows the queued count, and the queue
  flushes when the network is back.
- **Quick capture**: the global hotkey shows a small floating panel over any
  app, containing a text area and a fragment counter. `esc` keeps it as a local
  scratch note (see § Scratch notes), `⌘D` saves a draft, `⌘⏎` publishes.

## Scope (v1, all in)

Writing: fragments & threads, autosave, publish with note, paste/drop images,
quick capture, markdown preview, withdraw, version history + restore, pin
(irrevocable, so it confirms), delete drafts, quote another blyg (picker, ⌘K or typing `![[`, that
inserts `![[<id> | note]]`; typing `[[` links, `[[<id> | label]]`), fork.
Reading & managing: open on the web, read subscriptions (reading list), subscribe
and unsubscribe, blogroll flag, blyg settings (title, bio, links), mentions &
responses.

Phase 1 (parallel agents): core crate, writing UI, and Worker read endpoints.
Phase 2: reading/management UI, quote picker, fork, settings UI.

## Architecture

```
crates/blyg-core   model.rs (types) · backend.rs (Backend trait, the UI seam)
                   api/  (ureq client, one fn per endpoint, typed errors)
                   store/ (rusqlite: items, versions cache, outbox, reading, subs, FTS5)
                   sync/ (worker thread: debounce, outbox flush, periodic pull, conflicts)
                   live.rs (LiveBackend: impl Backend over store+api+sync)
crates/blyg-app    GPUI app. Talks only to `Arc<dyn Backend>`. Contains a
                   FakeBackend (in-memory, seeded) so the UI runs with no server:
                   `BLYGGER_FAKE=1 cargo run`.
```

The `Backend` trait and the model types in `blyg-core/src/{backend,model}.rs`
are the contract. Changing them is the orchestrator's call. An agent who needs
a change says so in its final report and doesn't make it unilaterally.
(Adding *private* helpers is fine.)

Threads and the keystroke path (the typing budget is a quarter frame of UI-thread work per
keystroke, at 2,000 posts and a 20k-character thread: `blyg-app/src/posts_perf_tests.rs`):
- **Typing never touches SQLite on the UI thread.** The editor saves with
  `Backend::save_typing`: `LiveBackend` keeps the newest text per item in memory and a
  `blyg-typing` thread writes it (newest wins; keystrokes typed during a write collapse into the
  next). Until it lands, `item` lays the text over the stored row and every other call writes it
  first, so nothing ever reads an older text; closing the backend writes what's left. A failed
  write is a `CoreEvent::Error`. The app works out the list row's new state itself
  (`store::saved_locally`) rather than reading it back. The writer, not the UI, emits the sync
  status after a save.
- **The trigram index of your posts is brought up to date lazily** (schema v18): a changed text
  only notes what the index still holds (`items_fts_stale`); `Store::flush_fts` re-indexes after a
  typing pause, after each pull and at open. Searches (the omnibar, the picker) treat noted rows
  as unindexed and check their text directly, so they never miss a word.
- **Two connections.** The store's writes, and the reading list's cache, use one connection; the
  reads of your own posts the UI makes (the list, search, one item, the picker, the sync status,
  the public origin) use a second, read-only one, so they never wait for a pull's merge (WAL). A
  pulled reading list is merged 100 rows per transaction, so nothing waits seconds behind it.
- **The omnibar searches off the UI thread**, newest keystroke wins (a generation number);
  until its results are back the list shows the previous query's, and ⏎, ↑/↓, opening a post
  or focusing the editor finish the search first so they act on what was typed. A sync's
  `ItemsChanged` refreshes the list the same way.
- **The preview's resolver is built on the render thread**, after the typing pause, and looks
  up only the ids the text names (`Backend::own_item`, `Backend::reading_with_id`), copying
  neither list.

Configuration: one Ghostty-style plain-text file, `~/.config/blygger/config` (list every key with
`blygger +show-config --default --docs`). Secrets live in the Keychain. App state (SQLite db, caches,
media) lives in `~/Library/Application Support/org.blygger.desktop/`.

## API (owner, bearer auth)

Base: the configured `blyg-url` (with any mount path) for public pages, the studio and
the API: every call is `{blyg-url}/api/…` (a blyg at `https://host/blyg` has its API at
`https://host/blyg/api`). Every `/api` call carries either `Authorization: Bearer
<token>` (a browser sign-in's access token or a manual API token) or the studio session
cookie `blyg_session`, from `POST {blyg-url}/studio/login` (form field `password`; a 302
with `Set-Cookie`; 30 days). JSON bodies are sent with `content-type: application/json`.
Error bodies are `{code, message, details?}` (`details.issues: [{path, message}]` on
validation failures); a wrong method is `405`.

**Contract guard.** Before any list is read, once a session, sync reads `GET
{blyg-url}/api/capabilities` (`Api::contract_version`, authenticated) and checks its
`contractVersion` (`api::contract`): major 5 is spoken (`CONTRACT_MAJOR`); anything
older, or a 404 (a studio before 0.49.7, a Worker fork), is `ServerOutdated` ("Burrow
needs blygger-studio 0.49.12 or later", `api::MIN_STUDIO`) and nothing is pushed or
pulled; a newer major stops with "update Burrow". The connect check does the same
(`ConnectError::Outdated` / `TooNew`).

The authoritative contract is upstream blygger-studio's OpenAPI document
(`openapi.json`, contract 5.0.0 from studio v0.49.12; vendored as
`crates/blyg-core/tests/fixtures/openapi.json`, and the test mock checks all traffic
against it). Its route guide is `docs/api.md` in https://github.com/blygger/blygger-studio.
The key points:

- **Cursor pages.** Every collection → `{items, page: {limit, nextCursor}}`; `limit`
  1–100 (`GET /api/reading` 1–50); `nextCursor: null` means exhausted; no `offset`, no
  `total`. `Api::collect` walks one coherent acquisition from the first page to
  `null`; only then may the caller prune rows the answer left out. A `409
  cursor-expired` (`CoreError::CursorExpired`) drops every page read and restarts from
  the first (`CURSOR_RESTARTS` = 3, then the error goes up and the list is left as held
  for that pull). A restarted page is never appended to an old prefix.
- **Revisions.** Every item carries `revision` (kept in `items.revision`, schema 15).
  Every item command sends `expectedRevision`. A `409 revision-conflict`
  (`CoreError::RevisionConflict`) goes through `Engine::with_revision`: re-read the
  item and merge with conflict detection; a text that moved under a local edit enters
  conflict; one that moved with no local edit is shown and the command is not
  repeated (`RevisionConflict` to the caller); a revision that moved without the text
  is retried once.
- Resources are camelCase; `wire.rs` maps them onto the snake_case model types (the
  store's format is unchanged).
- Item = `{id, revision, kind, status, version, dirty, created, updated, contentMd,
  responses, highlight, provenance, stubOf, forkedFrom, …}`. `kind` stays `fragment` or
  `thread` after withdrawal; the state is `status`. No permalink: the app derives
  `{blyg-url}/f|t/{id}`. `responses` is `"default" | "show" | "hide"`; `default`
  follows settings' `showResponsesDefault`.
- `GET /api/items/:id` → Item; `GET /api/items/:id/versions` → the version history (a
  withdraw marker is `kind: "withdrawn"`). `GET /api/items/summary` counts them.
- `POST /api/items {mode: "blank", kind, contentMd, stubOf?, provenance?}` → `201 Item`.
  `{mode: "fork", source: {origin, id, version}}` forks a pinned version.
- `PATCH /api/items/:id {contentMd?, kind?, stubOf?, responses?, highlight?,
  provenance?, expectedRevision}` → Item. Unknown fields are a 400. Kind changes until
  the first publish (409 after).
- `POST /api/items/:id/publish {expectedRevision, note?}` → `{ok, version, revision,
  warning?}`; `POST …/withdraw`, `POST …/restore {version}`, `DELETE /api/items/:id`
  (drafts only), `PUT /api/items/:id/versions/:version/pin`, `POST …/generate {scope}`:
  each with `expectedRevision`, each answering the new `revision`.
- `POST /api/media` multipart `file` (+`itemId` and `expectedRevision`, `alt`) → `201
  {id, url, mime, revision?}`, `200 {…, duplicate: true}` for the same bytes on the
  same item. `url` is absolute under the mount, `media/{id}/{sha256}.{ext}`. `DELETE
  /api/media/:id` takes `{expectedRevision}` for an attachment (the item's), `{}`
  otherwise; the backend keeps the item of each attachment it made (`meta`
  `media_item:{id}`).
- `PATCH /api/mentions/:id {hidden}`, `PUT/DELETE /api/signals/:sub/:remoteId {thumb}`
- Subscriptions: `GET/POST /api/subscriptions`, `PATCH /api/subscriptions/:id
  {inBlogroll?, title?, paused?}`, `DELETE …`, `POST …/resync` → `{ok, changed: n}`
- `GET /api/settings`, `PATCH /api/settings {...}` (camelCase keys; only the keys
  Burrow owns are sent)
- `GET /api/mentions?direction=inbound` (`sourceAuthor` an object)
- `GET /api/hoppers` (cursor pages of `Hopper = {id, name, slug, public, slugFrozen,
  description, created}`); `GET /api/hoppers/:id/memberships` (cursor pages of
  `{hopperId, subscriptionId, remoteId, addedAt, item}`, the held item an
  `ImportedItem` or null).
- `POST /api/hoppers {name}` → `201 Hopper`; `PATCH /api/hoppers/:id {name?, public?,
  description?}` (a rename recomputes the slug unless `slugFrozen`; making public freezes
  it); `DELETE /api/hoppers/:id`. `PUT`/`DELETE /api/hoppers/:id/items/:sub/:remoteId`
  adds or removes a post (`~own` and your item's id for your own). All need `owner:manage`.

**Reading** (`listImportedReading`; see `docs/SERVER.md`):

- `GET /api/reading/imported?limit=1..100&cursor=<opaque>` → cursor pages: every
  imported item, newest `observedAt` first (then subscription and remote id), no drops
  or repeats across pages. A bad cursor or a limit out of range is 400. Rows are
  camelCase (`subscriptionId`, `remoteId`, `subscriptionTitle`, `origin`, `kind`,
  `withdrawn`, `l0`, `version`, `readVersion`, `created`, `updated`, `observedAt`,
  `contentMd`, `contentHtml` (sanitized), `author`, `page` (origin-relative),
  `sourceUrl`, `thumb`, `hoppers`, `pinnedVersionRetained`, `transclusions`, `stubOf`,
  `forkedFrom`, `generated`); dates are ISO 8601 UTC or null. `wire::imported_row` maps
  them onto `ReadingItem` (`withdrawn` → `state: "tombstone"`). Each row's lineage is
  the origin's own, so it is known for every row but an L0 or withdrawn one.
- A pull is one acquisition (`Engine::fetch_reading`), up to `reading_pages` pages
  (20); only a walk that reached `nextCursor: null` prunes rows. A walk stopped by the
  page budget is merged without pruning; `cursor-expired` restarts it from the first page.
- Read state is advertised on `GET /api/reading` (`readState`, `readStateClear`), read
  once a session from `GET /api/reading?limit=1`.

**Read-state sync** (see `docs/SERVER.md`): each row's `readVersion`; `PUT
/api/reading/:sub/:remoteId/read {version, readAt?}`, `POST /api/reading/read {items:
[{sub, remoteId, version, readAt?}]}` (≤ 500), `DELETE …/read` and `POST
/api/reading/unread {items: [{sub, remoteId}]}`. Reads store `max(existing, version)`.
The writes need the scope `reading:state` (`oauth::READ_STATE_SCOPE`), not
`owner:manage`. Outbox payloads queued by an older Burrow (snake_case) still decode.

**A refused credential.** A `401` the credential can't recover from (after the
password sign-in or the browser grant's renewal is retried once) stops the sync worker:
no retries until a new sign-in or a manual sync, queued ops stay queued, and
`CoreEvent::SignedOut` (once a session) makes the app open the sign-in sheet.

Transclusion syntax is `![[<26-char id>]]` alone on its own line, and is only
valid in threads. See upstream `worker/src/transclusion.ts` for which ids resolve
(local and/or imported).

## Safety

- **Never write to the production blyg while developing.** Test against a mock
  HTTP server (core) or a local `wrangler dev` of the reference Worker (integration).
  The first real publish belongs to the user.
- Never log or print the token.

## Reading

**One entry per post, however many times it's edited.** An edit updates the existing row (the "edited · vN" badge plus a diff since you last read it) and never creates a new unread item. Cross-subscription duplicates (someone's blyg + their RSS feed) collapse to one row, preferring the blyg row. RSS items whose id changes on edit are matched by their resolved page URL. Tombstones are hidden unless signalled or hoppered. The mock is at `docs/prototype/ai-and-reading.html` §5.

**The stream is the default reading view** (`reading/stream.rs`, issue #1). A Stream | Reader toggle heads the reading screen (⌥⌘1 / ⌥⌘2, remembered in `state.json` as `reading_mode`). Stream: one native GPUI timeline in a virtualized `list`, newest first, drawn from each post's Markdown (`blyg_render::native_blocks`, the same markdown-it engine; no WebView per post). Each post shows its author and blyg, date, kind and the lineage line (↳ stub of / ⑂ forked from); fragments show in full, threads their title, about four lines and "Read more". A `![[id]]` is a grey quote box with the quoted text when it's held here, else the excerpt the quoting blyg cited (`transclusions[].cited`, protocol 0.3), else a note that it isn't held. An inline `[[id]]` link (protocol 0.3 §16.2) reads as the held target's excerpt in quotes, as the published page's anchor does (else "linked post"), and opens the post like a quote box. Images, embeds and tables are compact placeholders. j/k (↑/↓) select, and the selected post shows the actions (Read more, Quote into a thread, Reply · new stub, AI reply, Link post, Open on web, thumbs). ⏎, Space or "Read more" open the post in a side pane (the sanitized WebView reader with its pill and actions); j/k then move the pane along; esc closes it and the stream keeps its place. A post is read once it has been at least half on screen (or filled half the screen) for one second while the window is active (`stream_vm::ReadTracker`, `Backend::mark_read`); a post opened in the pane is read too. Unread posts get a dot, never a count. The search filters the stream as it filters the list. Reader is the list + post layout below.

**Reader is three panes, NetNewsWire style** (`reading/sources.rs`, `sources_vm.rs`): sources | posts | the post. The sources pane lists smart feeds (All unread: never read or edited since you read it; Today: the post's own date, local time; Thumbed: 👍; All, the default), then **Your blyg** (below), then **Hoppers** (below), then the user's **folders** with their subscriptions, then the subscriptions in no folder. Each subscription shows its name (or host) with its cached profile avatar, else a letter badge, and a dot when something is unread; smart feeds and folders show a muted unread count (reader-local, private: allowed by rule 1). Picking a source filters the list (the search is scoped to it); a post read in a source stays listed until the source changes. **Folders are local only**: the store's `folders` / `folder_members` tables (schema v7), one folder per subscription, never sent to the blyg and not the server's hoppers (below) (`Backend::folders`, `subscription_folders`, `create_folder`, `rename_folder`, `delete_folder`, which unfiles, `move_folder`, `set_subscription_folder`). File a subscription by dragging it onto a folder (onto the "Subscriptions" heading unfiles it), from its context menu (Move to folder ›), or on the Subscriptions screen; folders are made with Blyg › New Folder… or the pane's context menu, renamed, deleted, and reordered by dragging or Move Up / Move Down. Keys: ←/→ move between the panes, ↑/↓ within one (the sources pane picks as it moves; in the post pane they scroll), j/k next/previous post from anywhere, ⏎ goes right, Space pages through the post and then opens the next unread one, esc returns to the list, [ / ] step through versions (←/→ still do in the stream's pane). ⌥⌘S (Blyg › Sources Pane, or the header's button) hides the pane; its edge drags to resize (`state.json`: `reader_sources_hidden`, `reader_sources_width`). **Picking and marking** (`reading/marks.rs`, model `pick_vm.rs`): ⌘-click adds or drops a post, ⇧-click adds the run from the anchor, ⌘A picks everything the list shows, esc lets go; **r** marks the picks (else the open post) read and **u** unread, as does the row menu (right-click: Mark Read / Mark N Unread). Marks are local and instant (`Backend::set_read`); how they reach the server is docs/SERVER.md § Read-state sync. **Subscriptions** (studio 0.30): a subscription's menu in the pane has **Rename…** (a name of your own, kept when the blyg renames itself), **Use the Blyg's Own Name** once renamed (`title: null`; `Subscription::title_follows_source`), and **Check Now** (a blyg's index reconciled at once, `POST /subscriptions/{id}/resync`; a feed is polled with the rest); the pane's menu and **⇧⌘R** (Blyg › Check All Feeds Now) poll every feed (`POST /subscriptions/poll`). A pull follows each, and another a few seconds later.

**OPML import and export** (`blyg_core::opml`, `reading/opml.rs`; blygger/blygger-studio#63). Blyg › Import Subscriptions from OPML… (and Import OPML… on the Subscriptions screen; no key) opens a file picker, then a sheet in the Publish/Delete chrome listing the file's feeds: one OPML parser (`opml::outlines`, also behind `profile::parse_opml`) reads OPML 1.0/2.0 leniently (nested outlines flattened, the folder kept for display only, `xmlUrl` required, `title` else `text`, HTML entities and double escaping undone, BOM / UTF-16 / Windows-1252-labelled-UTF-8 decoded), at most 5 MB and 2,000 feeds, never fetching. Duplicates collapse by normalised feed URL; a feed already followed (same normalised feed URL, or under a blyg subscription's origin) shows "already following" and starts unticked. Keys: 1–9 / Space tick, ↑/↓, ⌘A all or none, ⏎ Import N, esc cancel. The run (`opml::run_import`, off the main thread) sends `POST /api/subscriptions {url, confirm:true}` per feed with no title (the source names itself) and no separate resolve, two at a time with starts ≥ 2 s apart (≤ 30 a minute, far under the owner write budget, because each subscribe backfills the archive server-side); a 429 holds every worker for its `Retry-After` and retries that feed (5 times at most). 201 is added, 409 already following, 422 not a feed (the server's reason shown), Offline unreachable, anything else refused with its reason; nothing fails the whole run. Esc stops after the requests in flight. Each added subscription is filed (by the id in the 201) in the local Reader folder **Imported feeds**, made on first use and reused by name; 409s are never moved, and the OPML's folders never become folders. `createSubscription` has no `in_blogroll` member and new rows default to `in_blogroll = 0`, so imports stay out of the public blogroll without a PATCH. The summary gives the counts and each failure with Retry failed (r). Blyg › Export Subscriptions as OPML… saves OPML 2.0 of every subscription (save dialog, `burrow-subscriptions.opml`), in the studio's `blogroll.opml` shape: `text`/`title` the name, `type="rss"`, `xmlUrl` the feed, `htmlUrl` the origin. Both need a connected blyg. CLI: `blygger +import-opml <file> [--dry-run]` (skips feeds already followed, files into the same folder; opens the local database without the sync worker) and `blygger +export-opml <file>`.

**Hoppers** (`reading/hoppers.rs`, `store/hoppers.rs`; blygger-studio's hoppers, collections of posts on the blyg). The sources pane lists them under a **Hoppers** heading after Your blyg (before the folders), in the server's order (created first), each with its count and a globe when public; ↑/↓ step through them like any source. A hopper as a source (`Source::Hopper`) lists the held rows whose `hoppers` names it, own rows included, never withdrawn ones; a member this Mac doesn't hold (only in the hopper's `items`, e.g. from a subscription since dropped) is kept in `hopper_items.item_json`, shown read, and only under that hopper. **Adding and removing** is local first: `Backend::set_in_hopper(hopper, sub, remote_id, on)` updates the cache, the row's `hoppers` and the count at once and queues an outbox op (`OpKind::Hopper`, keyed per hopper and post, so the last action wins and an offline add is never lost); it is sent as `PUT`/`DELETE /api/hoppers/{id}/items/{sub}/{remoteId}`. A 404 drops the op (the hopper or post is gone); a 403 holds the hopper ops for the session, says so once, and doesn't block the rest of the outbox. Own posts are members as `("~own", id)` (studio 0.45). **Making, renaming, describing, making public or private and deleting** a hopper (`create_hopper`, `update_hopper(HopperEdit)`, `delete_hopper`) call the blyg directly and update the cache from its answer: they need the blyg (offline they fail with "Your blyg can't be reached right now…", and nothing changes), since the studio derives the slug and the public page. A name is trimmed and required; a description is at most 280 characters (empty clears it). Making a hopper public asks first (it gets a page at `{blyg-url}/h/{slug}/`, listed under Collections, never in the feed), freezes its slug, and shows the address; deleting asks first and leaves the posts alone. **Sync**: the hopper list and every hopper's memberships (`GET /api/hoppers`, then `GET /api/hoppers/{id}` each) are read on the first pull and whenever the `hoppers` change counter moves, and cached in `hoppers` / `hopper_items` (schema v14) with waiting ops reapplied on top; after each reading pull the rows' `hoppers` follow that cache for the hoppers it knows (a hopper not read here yet keeps the row's word). **UI**: a post's **Hopper ›** (its action chips, the row menu's submenu, h on the reading screen, ⌥⌘P / Post › Put in Hopper…) opens a picker listing each hopper with a check for membership; choosing one toggles it, n or **New Hopper…** makes one and adds the post. The hopper's context menu has Rename…, Description…, Make Public… / Make Private and Delete Hopper…; New Hopper… is on the pane's menu and Blyg › New Hopper….

**Your own posts are a source too** (`reading/own.rs`; blygger-studio 0.45 put own posts in hoppers with the `~own` marker). Your published posts are reading rows keyed `("~own", id)`, built from your published items (`own::row`; drafts, scratch notes and withdrawn posts are not), at your origin, by date among the other rows: in All, Today and the stream, and alone under **Your blyg** in the sources pane (shown once you have a published post). They read as "You", with the same reading pane, lineage glyph and line, and the same action chips (Quote into a thread, Reply · new stub, Link post, Open on web, → Notes), without AI reply (you don't AI-reply to yourself) and without Fork or Fork this pin (your post forks from ⌘Y only). They are never unread (read at their version), so they're never in All unread or any count and never get a dot; they have no 👍/👎 and are never in Thumbed; and nothing about them is sent: no read state (`mark_read`, `set_read`, r / u skip them) and no signals. Opening one shows the current and pinned versions of your own history (`Backend::versions`; ‹ / › and [ / ] step to a pin, its text from your public pin file), and, when the working copy has unpublished edits, the published text from your public item document; Responses lists your verified mentions and what this Mac holds, and the lineage counts come from your posts as before (your rows aren't counted twice, and aren't asked of the node by reading key). In the lineage ring your post now offers Reply too; its Fork says your post forks from ⌘Y.

**Quote boxes and lineage lines open the original** (`reading/original.rs`, issue #3). In the reader's WebView each resolved quote box gets a footer, "quoted from &lt;name&gt; · v2 · open original" (added by the app's reader layer after sanitizing, `studio::reader::quote_footers`; blyg-render's output is unchanged): the name opens the origin's profile, a click anywhere else in the box opens the original post. The stream's quote boxes and the "↳ stub of …" / "⑂ forked from …" lines (stream and reader header) work the same way: the name goes to the profile, the rest to the post. A post held in the reading store opens at once, at the quoted version when that version is current or pinned (unpinned versions are never shown); one that isn't held is fetched from its author's public item document (`Backend::public_item`, `public_pinned`: the token-less `PublicClient`, only on that click) and shown with Subscribe / Reply · new stub / Open on web, never marked read.

**Browsing the thicket** (`reading/browse.rs`, blygger-studio 0.49.0). Every quoted layer at any depth says whose it is and opens in the reader: in the WebView each layer of a quote inside a quote gets its own footer, and a layer without `data-blyg-origin` is from the blyg of the layer that baked it (upstream's `injectNestedProvenance`); in the stream, quote boxes nest three layers deep. An ordinary link that names a blyg item opens in the reader too: when a post opens, its body's http(s) links (at most 40) are asked of the backend in the background (`Backend::classify_links`: `POST /api/peek/links` on a 0.49+ node; elsewhere, or on its 404/403, probed with the token-less public client in peek.ts's order and cached a week in the store's `link_probe`), kept for the session, and a link not yet answered is asked when it's clicked. A held item opens at once; one that isn't held is a peek, read from its origin's public item document with nothing stored, under "You're not subscribed to this blyg — subscribe?", which opens the Subscribe sheet prefilled. A stub peeked that way shows what it responds to, and that opens too. Links that aren't blyg items, and any ⌘/⌥-click, go where links go. Following a quote, a stub's source or a link from the open post is a step of a **walk**; opening a post from the stream, the list or a source starts a new one, and closing the pane ends it. The reading pane's top line shows "‹ back", a breadcrumb of the walk (earlier entries jump back) and "history". **⌘[** (the reading screen's own key; Blyg › Back) retraces the walk one entry at a time. Each step is logged privately (`Backend::log_browse`: the studio's `browse_log` on a 0.49+ node, else, or with a credential lacking `reading:state`, the local `browse_log` table), a tree in which each step names the step it was reached from (via start, stub, quote, link or back). A walk's first entry is logged only once something is followed from it, so the log holds walks, not every post read (a deliberate difference from the studio, which logs every opened entry). **⇧⌘H** (Blyg › Browsing History, or "history") shows the **history pane** beside the post: this walk, then earlier walks newest first, indented as the tree they were, each step opening its post, with "clear history" (two clicks, `clear_browse`). Never on the wire to anyone else, never on a public page. esc closes the pane first. Hovering or focusing never logs or changes the walk.

**Withdrawn posts are background** (studio 0.49.0; `reading/sources_vm.rs` `is_background`, `own::with_background`). The sources pane's **Background** smart feed (after All) lists withdrawn posts only, newest first: the blygs' tombstones, signalled or not (`Backend::background_reading`), and your own withdrawn posts (`own::withdrawn_row`, a tombstone row without its text). Every other smart feed, folder, subscription, Your blyg, the counts and the stream leave them out. Withdrawn rows are held read at their version: never unread, no count, no dot. **A revised post leads with its latest changelog entry** (`reading/change_lead.rs`): on a stream card for someone's blyg post past v1 (not RSS, not yours, not withdrawn), the first line is `V3: <note>` for the current version, read from the origin's changelog (`Backend::remote_versions`) once the virtualized stream draws the card, three at a time, kept in memory per (post, version) for the session; with no note, or no answer (asked again after a minute), `V3: No changelog entry available`.

**Compose while reading** (`reading/compose.rs`). A post box runs along the foot of the reading screen, under the Stream and under the Reader's three panes alike: a layout row, never over the post, one line until you type more, up to eight lines. `c` (with the list focused) or ⇧⌘K (Blyg › Compose While Reading, from anywhere in the window) puts the caret in it; esc returns the keys to the list and keeps the text; while it has the keys the list's single keys don't fire. **Post** (⌘⏎) publishes through quick capture's path (a scratch note promoted with `Promote::Publish`, the same floor rules); **Draft** (⌘D) saves a draft on the blyg; **⤢** moves the text into the editor as a draft. It posts a fragment, or a thread when it quotes a post or runs past 1000 characters; the footer says which. A failed post keeps the text with the error, and the retry reuses the draft it left. @-mentions, spelling and the `[[` / `![[` pickers work as in quick capture; with a blyg post open, **❝ Quote** puts its `![[id]]` in the box. Unsent text is kept in `state.json` (`compose_draft`), across screens and restarts (not in `BLYGGER_FAKE`). **Burrow opens on Reading** (`launch_into_reading`), and the tour ends there unless it was started from a post being written; it stays on Posts during onboarding or the tour, over the Connect sheet, or with no blyg to read from.

**Responses: who quoted, stubbed or forked a post** (`reading/responses.rs`, issue #7). A list, never a count: who · "quoted this" / "stubbed this" / "forked this" · when, each opening that post. Two sources: for your own posts, the verified, unhidden mentions (`GET /api/mentions`), listed under the post in ⌘Y and in the reading pane when your own post is open there; for anyone's post, "seen in your network": reading rows, and your own published posts, whose `transclusions`, `stub_of` or `forked_from` point at it (`Backend::responses`, an index lookup on the derived `reading_refs` table, schema v6, rewritten on every upsert; a quote without an origin is from the quoting post's own blyg). One response per post: a stub thread both stubs and quotes its target, which is one act, so the strongest relation wins (fork > stub > quote), and a quote with a `selector` marks it partial ("stubbed a passage of this"; `reading_refs.partial`, schema v9). This is the one-hop lineage blygger-studio's lineage view computes server-side (studio PR #35, `GET /api/lineage`), with the same rules, from what this Mac holds. The list sits at the bottom of the reading pane, and stays a list without a count; in the stream the lineage glyph (below) marks a post that has any.

**Lineage** (`reading/lineage.rs`, model `lineage_vm.rs`; mock `docs/prototype/lineage.html`). A **glyph** beside each stream post's name replaces the old "↩": lines in from the left are the kinds of post it draws on, lines out to the right the kinds that draw on it, in fixed slots (fork, reply, quote; amber, accent, green), dotted when every one of that kind is of a passage, then **"up · down"**: how many it draws on and how many known here draw on it (rule 1's lineage exception, 2026-10-09). The counts are blygger-studio's `lineage-glyph` extension's (studio PR #53, ported from commit dc632c5 in `blyg-core/src/lineage.rs`): one reference per (post, target), fork > stub > quote, a `{url}` stub counts as an ancestor, a bare quote means the quoting post's own blyg; descendants are held posts that reference it, one per post, plus, for your own posts, verified mentions (hidden ones too) from posts not already counted. Plain totals, uncapped, as the studio prints them; the tooltip gives the counts by kind in words. **Where the numbers come from:** on a node that serves the extension (`GET /api/ext/lineage-glyph/summaries` and `/lineage`, `owner:read`), from it: summaries for the reading rows (50 keys a request, by reading entry key `imported:["sub","id"]`) and the graph for ⌘J, cached in the store's `lineage_cache` (fresh for 30 s, the studio's `FRESH_MS`; asked again when the reading list changes or opens). Whether it's served is found out by asking: a 404 means "not enabled there", silently, remembered for six hours (meta `lineage_glyph`), and its cached answers are dropped. Otherwise, and offline before any answer, Burrow counts from what this Mac holds with the same rules (`lineage::Local`). **⌘J** (or a click on the glyph) opens the lineage sheet: the post in a hexagon, what it draws on above, what draws on it below ("what your blyg knows" when the node served the graph, else "what this Mac holds": reading rows, your published posts, verified mentions of your posts), at most five a row around the selection, with the count each way beside the hexagon and on the side list's headings. A `{url}` stub is a neighbour that's a page, not a post: `o` opens it in the browser and it can't be the centre. Arrows move; ⏎ on a neighbour makes it the centre and ⌫ walks back; `o` opens the selected post in the reader; esc closes. **Space** (or ⏎) on the centre opens the **ring**: the reader's actions in blygger-studio's compass positions (right Fork, lower right Reply, lower left Quote, left Link post, upper left Versions, upper right Open). A letter (f r q l v o) previews the action: a dashed node where its result would go, and the same four facts for every action (a response? the author told? their words in yours? shown under their post?). Each of Fork, Reply and Quote shows how many of that kind are already known ("R · 2"), and the panel says so in words. ⏎ or the same letter again does it; esc backs out a layer at a time. The ring uses existing actions only. Opening a post from the sheet (`o`, Versions) goes through the reader's own open, so a held post is marked read as a click in the list marks it (`Backend::mark_read`; subject to change in review). Until 2026-10-09 Burrow's glyph showed kinds only while the web Studio's showed counts; both now show the same numbers.

**The neighbourhood map** (`reading/neighbourhood.rs`, view `neighbourhood_view.rs`; proposal 3, 2026-11-06). In the lineage sheet, **−** zooms out to two hops: five rows, the centre in the middle, what it draws on one and two rows above, what draws on it one and two rows below. Nodes are hexagons coloured by their relation to the node they hang off (fork amber, reply accent, quote green; dotted when of a passage), dashed for the second hop, shaded for your own posts; a held cross-link between neighbours is drawn too. The second hop is read from what this Mac holds (`lineage::Local::up`/`down` of each first-hop node), never fetched, and capped at seven a row and 21 a side ("+N" for the rest), so the map stays a constant size at thousands of reading posts. Keyboard only: arrows move (between rows to the nearest place), ⏎ on a node makes it the centre (on the centre, back to the list), ⌫ walks back, **o** opens the selected post (a page in the browser), **+** returns to the list, esc closes. The side panel names the selected post, its author and relation, and says when it isn't held here.

**Trails** (`trails.rs`, `store/trails.rs`, `blyg_core::trails`; proposal 5, 2026-11-07). A trail is a named, ordered path through posts, yours and others', each stop with a one-line "why". Local to this Mac: store tables `trails` and `trail_stops` (schema v19; `Backend::trails`, `create_trail`, `rename_trail`, `delete_trail`, `trail_stops`, `add_trail_stop`, `set_trail_why`, `remove_trail_stop`, `move_trail_stop`), never sent anywhere. On the reading screen **t** adds the open (else selected) post to the current trail and opens an inline why field in the **trail bar** (⏎ keeps it, esc skips; with no trail yet it asks for a name first); **⇧T** starts a new trail with that post as its first stop. While the field has the keys the reading keys stand back. The bar (over the compose box) shows the trail's name (a click steps to the next trail), its stops as chips (a click opens, drag to reorder, × removes) and "stop 2 of 4" for the post in front of you. The sources pane lists trails under **Trails** (before the folders); a trail as the source (`Source::Trail`) lists its stops in trail order, so j/k walk it. Only blyg posts and your own can be stops (a trail publishes as quotes); a feed's post is refused. A stop whose post was withdrawn stays as a "withdrawn" chip without its title, is not listed in the source, and is left out of what's published. **⌥⌘T** (Blyg › Publish Trail…) opens a sheet: ⏎ writes `# name`, then `![[id | why]]` per stop (the post's title when there's no why; bare below studio 0.47), each followed by a blank line, into a new thread **draft** in the editor; nothing is published until you publish it. **h** on the sheet instead makes a private hopper (the trail's name) holding the stops. Adding a post to a trail notifies nobody, and nothing is fetched.

**Drag to quote, and the quote tray** (`tray.rs`; proposal 7). Stream cards, Reader rows and the lineage sheet's list rows can be dragged (in-window GPUI drag; withdrawn posts can't). Dropped on the editor, the post goes in where the pointer is, as the `[[`/⌘K picker would write it: in a thread `![[id | label]]` on its own line after the line under the pointer; in a fragment `[[id | label]]` inline. Dropped on the omnibar, it starts a new thread draft that quotes it; on the reading compose box it's quoted there; on the tray strip it goes on the tray. A feed's post is refused with a note. The **quote tray** is a strip above the status bar: **q** on the reading screen or **⌥⌘K** (Post › Put on Quote Tray; ⇧⌘Q is the system's Log Out) puts the open post on it, at most 24, newest last. It's kept per blyg (by host) in `state.json` (`quote_tray`), except during the tour, which has its own tray held in memory only. Chips drag like posts; ←/→ select, ⏎ places the selected one at the caret, ⌫ takes it off. The `[[` / ⌘K picker lists tray items first (marked ❝, filtered by the query); placing one takes it off the tray.

**Split view** (`split.rs`; proposal 15). **⌥⌘⏎** (Post › Open Beside) puts the selected post to the right of the open one: on Posts the selected row (as moving the selection opened it on the left, the left goes back to the post open before it); on Reading the open (else selected) post, and the window goes to Posts. Your own post on the right is a second editor with its own autosave (`Backend::save`) and status line; anyone else's is read-only, drawn with the stream's native blocks, and its header drags into the left post as a quote. **⌥⌘[** / **⌥⌘]** (View › Left Post / Right Post) move the keyboard between them (⌘[ / ⌘] are the editor's outdent/indent and the reader's Back). The divider drags; its place is kept in `state.json` (`split_left`, thousandths). × or ⌥⌘⏎ on the same row closes it; opening the right post on the left closes it too, so one post is never in two editors. The preview pane hides while the split is open. The mock's "Join into left" is not built (merging then deleting the right post is irreversible; for a published post it would be a withdrawal).

**Posts are dated by their author, not by the import.** The list sorts newest first by the post's own `updated` (else `created`), so an author's edit moves the post up; a row says "edited 3d ago" when `updated` is more than ten minutes after `created`, else shows when it was published. `observed_at` (when your blyg imported it) is only the fallback for a post with no usable date. Dates parse as RFC 3339 or RFC 2822 (`ReadingItem::post_time`); the store keeps the normalized sort key in `reading.sort_at` (indexed).

**Read state syncs through your blyg when the server supports it.** Read state is a number per reading row: the highest version you've read. Marking a post read is always instant and local, and it marks every duplicate of the same post. When the blyg advertises read-state sync (`readState`; `docs/SERVER.md`), the same mark also queues a `read` op per row in the outbox, which is sent like any other change (offline-safe, retried, coalesced to the highest version per row). A pull merges `max(local, server)`, so read state never goes backwards. The first time a database sees the capability, it uploads everything it has read in batches, once, and records that in `meta`. So a post read on one Mac reads as read on your others, and a fresh install isn't all unread. Without the extension, read state stays on this Mac, and nothing is sent. Nothing new is shown in the UI. The writes need the bearer scope `reading:state` (kept in `oauth::READ_STATE_SCOPE`), which the browser sign-in asks for. A sign-in or token without it (one made before the scope existed) gets a 403: the read ops stay queued, the rest of the outbox syncs, and Burrow says once that read state is waiting and to sign in again; the next sign-in sends what waited.

**Search, Notational Velocity style.** A search field sits above the reading list (the Posts omnibar isn't on this screen). ⌘F or `/` (with the list focused) puts the caret in it; typing filters the list live: a case-insensitive substring over the title, the author and blyg (subscription, origin) names, and the post's text (`content_md`, or the published HTML's text when an item has no Markdown), over posts **already held locally**. Nothing is fetched to search. Title matches are highlighted as in the Posts list. ↑/↓ move through the matches from the field, ⏎ goes to the list (opening the first match when nothing is open), and esc clears the search (esc on an empty field returns to the list; esc in the list with a search clears it before it leaves the screen). An empty result shows "No posts match “…”". The open post stays open while it still matches; otherwise the selection and the reader clear, so typing never opens (or marks read) anything.

**The post is shown as its blyg published it.** The body is the published `content_html` (a pin's own `content_html` when the pill is on a pin), with its transclusion snapshots already baked in, followed by the attached images from the item document's `media[]`. It is sanitized (an `ammonia` allowlist: no scripts, handlers, `javascript:` URLs, forms, objects, or iframes other than youtube-nocookie embeds; images get `referrerpolicy="no-referrer"`) and shown in a WKWebView under a strict CSP, with `<base href>` at the author's origin (protocol media paths such as `media/x.png` are origin-relative) and the app's reader theme (Literata, Tufte palette, light/dark). Only an item without HTML falls back to rendering its `content_md` with `blyg-render`, resolving quotes from items held for the same origin. The header, pill, thumbs, notes, diff and actions stay native. The same view shows the current version in ⌘Y. Only one WebView is on screen at a time: the studio preview hides wherever the reader shows.

## AI / TK

The blyg's AI feature is TK: `[TK]instruction[/TK]` → `[TK]instruction[=]output[/TK]`. See `docs/BLYGGER-SPEC-DIGEST.md` §AI and the mock at `docs/prototype/ai-and-reading.html`. Text generated in the app must be recorded as provenance (studio 0.28+'s `tk-provenance` route, `docs/SERVER.md`) so that it's disclosed as `blyg-tk-gen`.

- **Quotes generated text echoed (decision #60).** An own-line `![[id]]` in a scope's output is a real quote once a thread is published, and notifies the quoted blyg. When the scope's instruction never named that id (usually a model echoing a directive), the publish sheet warns before publishing (`studio::echoed_quote_warning`, over `blyg_render::unrequested_output_directives`), and the preview already renders it as the quote publish will make.
- **The generated mark is the publisher's claim (spec §5.7, decision #59).** In the reading pane, every `blyg-tk-gen` span or block gets a tooltip, and the post ends with a small note (`studio::reader::GENERATED_NOTE`): the disclosure is the publisher's own, not verified. The app never offers a direct claude.ai or ChatGPT subscription login (Anthropic prohibits it; OpenAI has no sanctioned route).

## AI providers & sign-in

The app will be used by **other people** too, so every user signs in to their own accounts. Providers:

| Provider | How | Notes |
|---|---|---|
| ChatGPT account | Sign in with ChatGPT (browser PKCE + device-code fallback) via OpenAI's Codex OAuth client → ChatGPT Codex Responses endpoint, the way pi does it | Requested feature. Unofficial: it may break or be blocked by OpenAI, and the UI says so once. |
| OpenAI API key | Paste the key (Keychain) | Official. |
| Anthropic API key | Paste the key (Keychain) | Official. |
| Cloudflare Workers AI | Account ID + API token (Keychain); OpenAI-compatible chat completions | Default model **Gemma 4** `@cf/google/gemma-4-26b-a4b-it`. |
| Local Claude Code / Codex | Spawn the user's installed `claude -p` / `codex exec` | Uses whatever account those tools are signed into. |
| Blyg server | Existing `/api/items/:id/generate` | Off by default today. A Worker can generate with Gemma 4 through its Workers AI binding (see `docs/SERVER.md`). |

**Not built:** a direct claude.ai (Pro/Max) subscription login. Anthropic's terms prohibit third-party apps from offering claude.ai login or using subscription limits. The local Claude Code bridge is the supported way to use a Claude subscription.

Disclosure: **always on**. Edited reading items move **to the top**. Helpers in v1: fill a gap (TK), shorten to fit 1000, continue this thought, outline a thread, proofread (not disclosed), reply to a reading item.

## Onboarding & tutorial

The first launch is an onboarding flow: connect a blyg (its address, then Sign in with browser, an API token from Studio → More → Client access, or the studio password), then optionally connect AI accounts. After that comes an **interactive tutorial** that walks through the features (the ▶ demos in the mocks are the script), with a "Show this tutorial every time I open Burrow" checkbox. It can be re-enabled or replayed from Settings › Help. **After an update**, the first launch of a newer version opens the tutorial on a "What's new in Burrow x.y" card (`onboarding/whats_new.rs`, compared with `seen_version` in state.json): ⏎ starts at the first new step, "Take the whole tour" starts at the beginning, esc or "Skip tutorial" closes it. A first run never sees it.

**Every release updates the tour and the docs (the maintainer's rule).** A new feature gets a step in `onboarding/steps.rs` that lands on it (its `since` is the release), steps whose UI changed are edited, and the release's what's-new entry starts at the first of its own steps; `whats_new.rs`'s tests fail otherwise, and fail for an x.y.0 version with no entry. A step that teaches the running version's news says "NEW IN x.y.z" on its card. The tour runs on the FakeBackend and also on **sample extensions** (`extensions/tour.rs`): the user's extension host is parked and a tour host runs the bundled markdown-notes (two sample folders allowed, one not), reading-time and inspect over invented notes in `<data dir>/tour-sample/`, deleted when the tour ends (and cleared when the next one starts); nothing in the tour writes the config file (Allow, Turn on/off, Forget permissions and the notes folders' Add and Remove say what they would do instead). The cross-post step shows a sample run in the pane (■ Stop, then "macro running · show pane" once folded) with nothing running, and ✂ Clip on the sample page clips its sample text. The `opml` step rings the Subscriptions screen's header; its **Import OPML…** chip opens the preview of a sample export (one feed already followed, two new) instead of a file picker, and importing it subscribes in the sample data. The reading pane's WebView draws over GPUI, so the tour shows it only where it is clear of the tour card (the stream's side pane, or the Reader's post pane, in a wide enough window; `tutorial_hides_reader`). 0.12.0's steps open a sample thread whose TK output echoed a quote (`echoed-quote`, ⌘⏎ shows the warning) and Lin's fork with a sentence marked generated (`generated`, ⏎ shows the note). 0.13.0's steps open the sample published post with an unpublished edit (`discard`: ⌥⌘Z, or on Windows the Post menu or the toolbar's eraser, opens the confirm sheet) and the chips on your published post (`own-post`: ⌥⌘R or a chip makes a stub of it); the `quotes` step also counts a typed `[[` (`Key::Picker`, the link or quote picker open) and teaches labels, and the `reader` step names Your blyg.

## Protocol philosophy → UI rules (from the creator's talk, blygger.org/talks/2026-09-24-blygger/)

Core principle: *"anything the protocol can't verify, it declines to represent."* The app is a **studio**. It follows these rules:

1. **No counts, anywhere social.** Responses and mentions are shown as *a list, never a count*: who, origin, relation, when. Mentions and responses never get numeric badges; use a dot for "something new". There are no follower lists or follower counts: following is client-local and invisible. (A reader-local unread count for *your own* reading list is fine; it's private state, not a social metric. So are a post's word count and reading time, which describe the text, not its audience: the opt-in `reading-time` extension.)
   **One exception, the lineage glyph (2026-10-09, the user's decision).** The lineage glyph beside a post, the ⌘J lineage view and its action ring show *how many* posts a post draws on and how many known here draw on it, by kind (stub, quote, fork), next to the kinds, because the web Studio's `lineage-glyph` extension shows the same numbers and the data is there. The numbers are blygger-studio's, counted by its rules (from the node when it serves the extension, else from this Mac). The exception covers those three surfaces only: responses lists, mentions, profiles and everything else social stay lists without counts.
2. **Generation happens in the studio, at authoring time, with review.** Publishing never generates. The TK tint is an authoring aid: published bytes are identical, apart from the `generated` metadata and the `blyg-tk-gen` class. Provenance is self-asserted, so the app always records it (studio 0.28+'s `tk-provenance` route), and the reading pane, which tints someone else's marked text, says the mark is the publisher's own and unverified.
3. **Transclusion is quoting, and it's snapshotted at publish.** Later edits to the source never rewrite the quote. The picker offers only what's already held (your own posts plus imported items from blyg subscriptions). It never fetches by URL. The picker opens with ⌘K, or by typing `![[` at the start of a line in a thread (after optional indent, outside a fenced code block): the typed `![[` comes out, what you type next filters the picker, ⏎ inserts the whole line in its labelled form, `![[id | note]]`, and esc puts the `![[` back so it can be typed literally. Typing `[[` anywhere outside code opens the same picker for a link, and ⏎ inserts `[[id | label]]` (studio 0.47). The label is the link's text on the published page, so editing the words after `|` renames the link; a quote's note is only for you (the published quote ignores it). The footer says so: "⏎ inserts [[… | label]]" / "⏎ inserts ![[… | note]] on its own line". Only typing triggers it, never a paste, and the `bracket-picker` config key (`on`, `quotes`: only `![[`, `off`) can turn either typed trigger off (issue #53: brackets in prose and TK scopes); ⌘K always opens it. In a fragment it shows the "Quotes go in threads" toast and leaves the text alone. Each row leads with the post's title; a link's row adds a muted *links as “…”* line when the label it writes isn't that title. **A deliberate difference from the studio:** for a post with no heading, the studio's `paletteInsert` writes the search hit's label, the post's first 60 characters in quotes with the first line and the body run together (`[[id | “Gardens, not streams A note on how I…”]]`); Burrow writes the post's title instead (`[[id | Gardens, not streams]]`, `blyg_core::pick::pick_label`), because that is what the reader of the link should see and what the writer was searching for. A heading's label is the same in both, and the quoted excerpt stays when the title is longer than 80 characters (a one-paragraph post) or the post has no text. The published page is unaffected either way: a label is the author's text, whatever wrote it.
4. **Forking descends from pins only.** Pins are the costly, irrevocable signal: confirm with plain words ("This version will be served forever. You can't undo this.").
5. **No deletes of published work. Withdraw instead**: permanent, visible, and irreversible. Only drafts can be discarded. The UI says "Withdraw", never "Delete", for published items.
6. **No identity layer.** No @handles or accounts. The origin (domain) is the name, and author names are optional decoration.
7. **Stubs are the reply shape.** They work on your own posts too: a stub of your own post (`stub_of` with your origin, §10.6) from the reading actions, and a fork of your own pin (`forked_from` with your origin, §5.6) from ⌘Y only, as blygger-studio offers it (`resolveOwnFork`, the version history's fork beside a pin). **Quotes are written labelled** (studio 0.47's `[[id | label]]` grammar, as the quote picker writes them): Quote into a thread, the lineage map's Quote and → Notes write `![[id | label]]` (`vm::quote_label`: the post's opening heading, else its title; `blyg_core::grammar::directive_source`). Reply's `![[id]]` and Link post's `[[id]]` stay bare. "Reply to a reading item" creates a stub thread (`stub_of`), usually transcluding the source. The reading actions name the primitive they create: **Reply · new stub** (`stub_of`), **Quote into a thread** (`![[id]]` in a thread of yours), **Fork** (`forked_from`, from a pin) and **Link post · new fragment** (a new fragment holding `[[id]]`, a plain link with no `stub_of` and no mention; blyg posts only), each with a one-sentence tooltip saying what gets made. Reply stays the one "I am responding" action, and (blygger-studio 0.31) it always opens quoting the whole post: `![[id]]` and a blank line, the server's own prefill. The passage is chosen in the editor's own preview (studio 0.49; `reading/stub_bar.rs`): a hint line above the text names what the draft does (the whole post; a passage; N passages, a running commentary; or no quote, a response by link). Selecting text inside one of the stub's quote boxes in the preview offers **❝ quote only this** (or ⇧⌘D), which writes the passage as `>` lines under *that* quote's `![[id]]`, whichever of several it is (`with_stub_quote_nth`, normalized by `normalize_selection` as publish compares it). The page reports the selected box's place among all boxes (`stubsel:`); the app accepts it only for a top-level quote of the target, on the page rendered from the draft's current text (a generation number per render), and only when the page's quotes of the target pair one-for-one with the draft's directives (`quote_count`). A nested quote, another post's quote, the provenance line, or a stale preview offer nothing. **select a passage in the preview** shows the preview (re-adding the quote if it was deleted); once a passage exists it reads **quote another passage** and adds a whole-post `![[id]]` after the caret's block to select from (`add_whole_stub_quote`); **quote whole post** takes a passage out again. The separate chooser pane is gone. The text logic is `blyg_render::stub_quote`, a port of the studio's `stub-quote.ts`: the first own-line `![[id]]` outside code and its attached `>` run, found the way publish finds them. The reading pane's selection pill only quotes into a draft (⇧⌘D); ⇧⌘D into a stub of the same post goes through the same functions, so it never adds a second directive.
8. **Titles are the author's own words.** Items stay titleless on the wire; the app derives a title (`blyg_core::plain_title`) from the first line of text, so a leading heading is the title on every surface (posts list, reading list, stream). Blockquote lines are skipped, including the `>` run attached to a `![[id]]` (a partial transclusion), so a thread is never named by what it quotes; a stub with nothing of its own yet reads "In response to &lt;host&gt;" (blygger-studio 0.8.2).
9. **Tolerate the unknown.** Ignore unknown kinds and fields; never reject. The wire is v0.3 and pre-1.0 unstable.

## Client-recorded provenance (studio 0.28+, see `docs/SERVER.md`)

- `PUT /api/items/:id/tk-provenance {content_md?, scopes: [ {index, model, sources?:[{id,version}], at?} | null ]}` → `{ok, disclosed}`. Validation runs first and nothing is written on a 400. `scopes.length` must equal the number of TK scopes in the (new) working copy.
- `GET /api/items/:id/tk-provenance` → `{scopes: [...]}`.
- **The server keys provenance by scope POSITION.** A plain `PUT /api/items/:id` that adds, removes or reorders scopes shifts disclosure onto the wrong span. Rule for blyg-core: **whenever the set of TK scopes changes, push the text with the combined call** (`content_md` + the full `scopes` array), never a plain PUT. Track provenance locally per scope (model, sources, at). A scope the user rewrote entirely by hand → `null`.

## Versions & pins (user requirement + spec §5.2/§8.4, mock: `docs/prototype/versions.html`)

- **Reading list: one entry per post** (latest version). A **version browser** (⌘Y) opens from any post.
- **Other people's posts:** the version UI shows **only the current version and pinned versions**. Unpinned versions don't appear at all, in any form (§8.4, and a deliberate product decision). The pinned list comes from the public item document's `changelog` (`pinned: true`) and each pinned body from `{origin}items/{id}/v{n}.json`. **Compact control:** there's no sidebar. The version pill in the post header is a `‹ vN ▾ ›` control: the arrows step through current + pinned versions, and the pill opens a small dropdown list. Actions are shown inline, labelled with what they create: **Quote into a thread** / **Reply · new stub** / **AI reply · new stub** / **Fork** / **Link post · new fragment** / **Open on web** for the current version; **Quote this version** / **Fork this pin** / **Diff vs now** / **Back to current** for a pinned one. Fork is available only on pinned versions: on the current version it's shown **greyed out**, and its tooltip says why and where to go ("Fork needs a pinned version: pick 📌 vN in ‹ vM ▾ › (or press [), then Fork this pin", or that the post has no pins). Every action chip has a one-sentence tooltip saying what gets made (a stub thread, `![[id]]` in a thread, a quoted pin with a link, a forked thread draft).
- **"Edited since you read it":** store only `read_version` (a number). Show the author's changelog notes for the versions in between. A **text diff only when the version you read was pinned** (both sides public). Never retain the unpinned text of past versions of other people's posts.
- **Withdrawal of others' posts:** drop the content locally (*"Local hoarding past withdrawal is nonconforming"*), except pinned versions, which may be retained with attribution linking the pin.
- **Own posts:** your history is private to you. Every version can be opened and restored (a restore loads it into the editor; publishing makes vN+1, and versions never go backwards). The current version has no Restore, but while the post has unpublished edits it offers **Discard changes…**, which restores it: the same sheet and code path as ⌥⌘Z (see Discard changes above). Pinning uses a type-to-confirm sheet ("You can't undo this"). **Fork this pin** on a pinned version of yours makes a new draft from it, `forked_from` naming your own origin (protocol 0.4 §5.6 rule 1: origin "REQUIRED, even when it is the publisher's own"; rule 2: a pinned version only; blygger-studio's version history offers the same). On an unpinned version it's greyed, "Pin one in ⌘Y to fork it". This is the only place your own posts fork (`vm::own_actions`, switch `own::FORK_OWN`).
- Public fetches to other origins are **unauthenticated**: never send the owner token anywhere but the user's own blyg.

## Profiles (mock: `docs/prototype/profiles.html`, agreed as-is)

Who someone is, whom they read, and one click to follow them or anyone they quote, stub or fork.
- **Sources, all public:** a blyg's `blyg.json` (author name, bio, avatar resolved against the origin, links), `blogroll.opml` only when the manifest lists it (OPML outlines: title, xmlUrl, htmlUrl), `items/index.json` (recent items; titles from `feed.xml` or from posts already held), or, for a plain RSS/Atom feed, a simpler card (title, link, recent items). A feed with `<blyg:manifest>` is treated as a blyg.
- **Privacy:** fetched with the token-less `PublicClient` (no token, no cookies) and **only when the user opens a profile**, never in the background or on a poll. Cached in SQLite (`profiles`, with `fetched_at`); opening a profile younger than an hour uses the cache without a request, ↻ refreshes, and a failed fetch falls back to the cached copy (marked stale). The origin sees the user's IP, as a browser visit would.
- **Discovery** reuses the subscribe preview's resolution: your own blyg and your subscriptions first, then `POST /api/subscriptions` (preview). Only when that's unavailable does core probe `{url}blyg.json`, then the URL as a feed. No crawling.
- **Connections** (origins a blyg quotes, stubs and forks) come from that blyg's posts already held locally (`stub_of`, `forked_from`, `transclusions`, `data-blyg-origin` in their HTML) plus quote sources in their feed. Each row says "stubbed 2 posts" / "forked 1 pin" / "quoted 3 times": the author's own data, not a social metric. Your own profile's connections come from your published posts.
- **No counts:** no follower, subscriber or reader counts anywhere. Lists only.
- **Follow** = subscribe preview + subscribe in one step, with a toast; following is private and client-local. Unfollow lives in Subscriptions. **Add to my blogroll** is separate and deliberate (subscribes first if needed, then sets `in_blogroll`), because the blogroll is public.
- **Entry points:** ⌘I (the reading item's origin; your own blyg anywhere else), the author's address in the reading header, the lineage line under it (`↳ stub of …`, `⑂ forked from … vN 📌`), the origin in each mention, blogroll and connection entries inside a profile (profile → profile, with a back arrow), Blyg › My Profile, and ⇧⌘O "Open profile…" (paste any URL).
- **Own profile:** what visitors see (your public manifest), "Edit site settings…", and every subscription with its blogroll toggle.
- **Sheet:** slides in from the right over the reading pane; esc closes, ↑/↓ and ⏎ move and open, F follows the selected entry, ⇥ switches tabs, ← goes back. It suppresses the native web views like other sheets.
- Core: `Backend::profile(url, refresh)` and `Backend::cached_profile(url)` (`blyg_core::profile`); `ReadingItem` carries `stub_of`, `forked_from` and `transclusions` (read leniently from the reading JSON; the owner-API reading extension should pass them through from the item document).

- **Lineage** ("↳ stub of", "⑂ forked from", quote origins) comes from the reading item when the server sends it, and otherwise from the post's public item document (`items/{id}.json`), which the app already fetches, unauthenticated, when a post is opened. The reference Worker's importer doesn't keep `stub_of`/`forked_from`. Connections use only documents already fetched; nothing is fetched just for them.

## Full editor ("studio mode"; mock: `docs/prototype/studio.html`)

A local full editor like the web studio: **source | live preview**, the preview showing the document exactly as published.
- **Modes:** ⌘1 write (list + editor), ⌘2 list + editor + preview, ⌘3 full editor (editor + preview, list hidden). ⌘E toggles the preview in place. There is one renderer for all of them (retire the separate native pulldown→GPUI preview).
- **Renderer:** a Rust port of the reference Worker's studio preview pipeline. Markdown with markdown-it semantics (`html: false`, linkify), plus the common embeds extension (a bare YouTube link on its own line → the click-to-load facade `figure.blyg-yt`; off-origin `<img>` gets `referrerpolicy="no-referrer"`; a failed remote image → a visible link fallback). TK scopes → output wrapped in `blyg-tk-gen` with the studio tint; ungenerated → `⚠ ungenerated — <instruction>`. Transclusions `![[id]]` (threads) resolve from the **local store only** (own published items + imported reading items), rendered as `blockquote.blyg-transclusion` with provenance; unresolved → the `.unresolved` marker. **Parity tests** use fixtures generated by running the Worker's own TS renderer via node.
- **View:** a WKWebView (e.g. `wry`) embedded in the GPUI window. Styled with the blyg's own public `/style.css` (fetched, cached, and used offline), plus the studio additions (tint, unresolved). No JS from content; our own small script handles the YouTube click-to-play (youtube-nocookie) and the image fallback. Links open in the default browser; no in-view navigation.
- **Speed:** re-render ~100 ms after typing pauses, patching the DOM in place (no flicker, scroll kept). Source ↔ preview jump via block line maps (`data-line`).
- **Status bar** counts quotes, AI spans, videos and images, and warns about unresolved quotes before publish.

## Scratch notes (local-only)

Quick capture is for collecting thoughts, not for deciding. So:
- **Quick capture saves a local scratch note by default.** esc, ⌘S or clicking away saves it. ⌘D saves it as a **draft** on the blyg instead (synced, unpublished). ⌘⏎ **publishes** it immediately.
- **Scratch notes are local-only.** They're stored in the local SQLite database, never enqueued to the outbox, never pushed or pulled, and never sent anywhere unless the user invokes AI on them. They show in the main list with a `scratch` pill, are searchable, and are fully editable.
- **Promotion:** ⌘D (make draft) creates the server draft from the scratch note (the same item keeps its local id, so there's no duplicate). ⌘⏎ publishes, creating it first if needed. The kind is chosen at promotion (`blyg_core::promotion_kind`): fragment when ≤ 1000 (server count), otherwise a thread; a note the user already made a thread (⌘T) stays one. `Backend::promote` returns the chosen kind, and the UI says so ("published as a thread"). A scratch note's length never blocks ⌘⏎. Offline, `Promote::Publish` still promotes (the draft's `create` queues in the outbox) and the publish fails with `Offline`. There's no demotion back to scratch once a note is on the server; drafts are discarded or withdrawn instead.
- **Images in scratch notes stay local.** A paste or drop into a scratch note copies the image to
  `<data dir>/scratch-media/<sha256>.<ext>` and inserts `![](blyg-local:<sha256>.<ext>)`; both previews
  show it from disk. Promotion uploads each one (without `item_id`) and rewrites the references to
  `<blyg origin>/media/<id>.<ext>` before the item is created. A failed upload refuses the promotion
  (the note stays scratch). Offline, the uploads go with the queued `create`.
- ⌘N (New Post) starts a **scratch note** in an empty editor (see § Interaction spec, "New post"), unless the config sets `new-note = draft`. The main window's omnibar create stays a **draft** (unchanged), unless the config sets `new-note = scratch`.
- Core: `Status::Scratch` (stored as `items.status = 'scratch'`), `Backend::create_scratch(kind, content_md)`, `Backend::promote(id, to: Promote::Draft|Promote::Publish{note}) -> Promoted{kind, published}`. `save`, `set_kind`, `search` and `delete_draft` work on scratch items locally; `publish` on one promotes it first. The sync engine ignores local-only items. Tests: scratch never hits the network (mock server asserts zero requests), promotion keeps the id, and offline promotion queues.
- Config keys: `capture-default = scratch|draft` (default scratch) and `new-note = draft|scratch` (unset by default: ⌘N starts a scratch note and the omnibar creates a draft; when set, both follow it; `Config::new_post` / `Config::new_note`).

### Notes drawer

A scratchpad for running notes while reading (issue: "notes while I browse"). Code: `crates/blyg-app/src/notes/`.

- **Where:** a drawer that slides in over the right edge of the window (380 px, an overlay with a shadow; it never adds a layout column), over the Stream, the Reader, the browser pane and the Posts screen. ⇧⌘N or View › Notes toggles it; esc (while it has the keyboard) or a click outside slides it back, and the keyboard returns where it was. Web views it overlaps (the reader, the preview, the browser pane) are cut off at its edge while it's out, as the browser pane does. The browser pane makes room: its right edge meets the drawer's left edge (slide mode moves over, full mode is inset), sliding alongside it, so its chrome stays uncovered. A click on an add action (→ Notes) while the drawer is out isn't a click away.
- **Backed by a scratch note:** one rolling note, "Reading notes" (`# Reading notes` as its first line), made a thread so `![[id]]` quotes are valid in it. It's created with the first real text, autosaved on every edit, local only, listed in Posts with its `scratch` pill, and remembered in `state.json` (`notes_note`; otherwise the newest scratch note titled "Reading notes…" is picked up). The header shows its title, **⌘D → draft** (promotes it; the drawer starts a new page), **Open in editor**, and ⋯ › **New notes page** (a fresh "Reading notes · Sep 27"; the old one stays in Posts). ⌘⏎ in the drawer opens it in the editor with the publish sheet. The editor is the same textarea setup as quick capture, with @-mentions, spellcheck and paste-a-link-over-a-selection.
- **Adding:** "→ Notes" in the stream's action row, the reader's action row and the side pane appends `![[id]]` for a blyg post (or, when the note was made a fragment, `[title](page url)` with the first line as a `>` quote) and `[title](url)` for a feed post, then opens the drawer with the caret on a new line below. The browser pane's → Notes appends `[title](url)`. With text selected in the reading pane's post or the browser page, ⇧⌘N (or → Notes) quotes it as a `>` blockquote ending `> — [title](url)` (read with a small script, `window.getSelection()`, at most 4000 characters).
- Not possible (yet): selecting the stream's native text (GPUI text has no selection), and dropping a link from another app onto the drawer (GPUI only accepts dropped files).
- **Universal quoting** (⇧⌘D, Post › Quote Selection in Draft, the browser pane's **→ Draft**; `notes/quote.rs`): the passage highlighted in the notes drawer (when it has the keyboard), else the browser page, else the reading pane's post, goes into the draft or scratch note open in the editor, at its caret, or into a new draft when none is open (a thread for a blyg passage, else a fragment unless it's too long). The Markdown is one pure function, `reading::vm::quote_block`: a passage from a blyg post we hold (the reading pane's post, or a browser page on a followed blyg's origin whose path names a held post's id), going into a thread, and really in the version we hold, is a partial transclusion, `![[id]]` with the passage attached as `>` lines (no blank line between; a paragraph break is a bare `>` line, studio's `quoteLines`). Anything else (a fragment, which can't transclude; a web page; a passage not in the held version) is a `>` blockquote of the passage ending `> — [title](url)`. From the notes, a passage inside a quote that ends in such a source line keeps that link; the note has no source URL of its own, so anything else goes in without one. Nothing selected: a toast, and nothing is made.

## Buttons (optional toolbar)

Keyboard-first, but not keyboard-only. Config `show-buttons = true|false` (**default true** for new installs; the maintainer sets `false`). It's toggled in Settings (⌘,) and offered in the first-run tutorial ("Buttons or keyboard?").
- A quiet toolbar in the title-bar row, with icons plus short labels: **New** (⌘N: a scratch note, or a draft with `new-note = draft`), **Make draft**, **Publish**, **View: Write / Preview / Full editor**, **Versions**, **Generate (AI)**, **Discard** (Discard changes, ⌥⌘Z), **Delete** / **Withdraw** (one slot: Delete on a draft or scratch note, Withdraw on a published post; never Delete for published work), **Quick capture**. The quick-capture panel gets the same row: **Scratch · Draft · Publish**.
- **Generated from the single keymap table** (action, key, context, menu label, icon, button label), so buttons, menus and shortcuts can't drift. Every tooltip shows the shortcut, so the buttons teach the keys.
- Buttons are disabled with a reason in the tooltip when unavailable (e.g. Publish on an over-limit fragment: "Too long for a fragment. ⌘T makes it a thread").
- With `show-buttons = false`, the window is exactly the minimalist layout in the mocks.
- As built (`crates/blyg-app/src/toolbar.rs`): the rows live in `keymap::table()` (`icon`, `button`), the order in `keymap::TOOLBAR` and `keymap::CAPTURE_ROW`; a click dispatches the row's action. The row sits between the traffic lights and the view switcher; the centred title shows only when there's room, and a narrow window gets icons only. Buttons other than Quick capture work on the Posts screen and wait for an open sheet. Disabled reasons reuse `vm::publish_decision` / `vm::make_draft_blocked`. In quick capture the row replaces the key hints (each button shows its key); with `capture-default = draft` there's no Scratch button. Reading (⌘R) and Quote (⌘K) have no button: the view switcher already is Reading, and Quote is thread-only. Delete and Withdraw share a slot (`toolbar::visible`, `vm::discard`), with Discard just before them (`vm::discard_changes` gives its disabled reason); Withdraw is the one button without a key, so its tooltip is just "Withdraw…". Icons: thirteen Lucide icons (ISC). Tests: `keymap` (every button bound, key in tooltip), `menu_tests` (menus = table), `toolbar_tests`, `discard_tests` (Discard changes too, with typing during the restore), capture tests.

## Updates (in-app, signed)

Config `auto-update = install | notify | off` (**default install**). Burrow › Check for Updates… always checks, whatever the key says, and answers with a toast ("You're up to date (0.3.0)").

- **When:** 15 s after launch, then about every 24 h while running. `state.json` remembers the last check that found nothing newer (`last_update_check`), so relaunches within a day don't call the API again; a check that found an update isn't recorded, so a relaunch looks again. A failed check retries in an hour. Never in `cfg(test)`, with `BLYGGER_FAKE=1`, or with `BLYGGER_NO_UPDATE=1`.
- **What:** `GET https://api.github.com/repos/<repo>/releases/latest` (unauthenticated, with a User-Agent). Drafts, prereleases and pre-release tags are skipped; the tag is compared as semver against `CARGO_PKG_VERSION`.
- **install:** download and verify in the background, then a sheet: "Burrow X is ready to install" with **Restart Now** and **Later** (asked once per release; Check for Updates… asks again), and the status-bar notice "Burrow X is ready · Restart to update · What's new". Restart installs and relaunches; quitting with an update ready installs it too (no relaunch). **notify:** a sheet "Burrow X is available" with **Download and Install** and **Later**, and the notice "Burrow X is available · Download · What's new"; Download proceeds as install. **off:** no automatic checks.
- **Security model** (all must pass, or nothing is installed):
  1. HTTPS only, from `api.github.com`, `github.com`, and GitHub's release-asset storage hosts (`objects.githubusercontent.com`, `release-assets.githubusercontent.com`). Redirects are followed by hand and every hop is checked.
  2. The release workflow signs `SHA256SUMS` with the project's Ed25519 key (secret `UPDATE_SIGNING_KEY`, `scripts/sign-sums.sh`, OpenSSL 3 `pkeyutl -sign -rawin`) and publishes `SHA256SUMS.sig`: the **raw 64-byte signature** over the exact bytes of `SHA256SUMS` (not base64). CI verifies it with the key embedded in the app before uploading, so a wrong key fails the release.
  3. The app embeds the public key (`update/verify.rs`, `RELEASE_PUBLIC_KEY_B64`, raw 32 bytes base64) and checks the signature with `ed25519-dalek`'s `verify_strict` before downloading the zip.
  4. `Burrow-<ver>-macos-universal.zip` (or, when a release has only that, the pre-rename `Blygger-<ver>-macos-universal.zip`) must match its SHA-256 line in the signed `SHA256SUMS`.
  5. The zip is extracted with `ditto -x -k` into a staging folder on the same volume as the running bundle; the extracted `Burrow.app` (or `Blygger.app`) must have `CFBundleIdentifier = org.blygger.desktop`, a `CFBundleShortVersionString` equal to the release's version and strictly newer than the running one, its executable, and pass `codesign --verify --strict` (the ad-hoc signature). These checks run again right before installing.
  6. Only the bundle the app runs from is replaced (from `current_exe()` → `…/X.app/Contents/MacOS/blygger`, and only if that bundle is `org.blygger.desktop`). The swap: old bundle → `.X.app.old`, new bundle in, old removed; if the new one can't be moved in, the old one is put back. A detached `/bin/sh` waits for the process to exit and runs `open -n <bundle>`.
  7. Not running from a bundle (`cargo run`), a translocated copy, or an unwritable folder → notify-only: "Can't update in place: …; download from the release page". A release without `SHA256SUMS.sig` (0.2.0 and earlier) is notify-only too. Quarantine is neither added nor stripped (the app's own downloads aren't quarantined).
- `scripts/install.sh` also checks `SHA256SUMS.sig` when OpenSSL 3 is installed (and the release has one).
- As built: `crates/blyg-app/src/update/` (`check.rs`, `verify.rs`, `net.rs` with the `Http` trait, `install.rs` with the `Tools` trait, `mod.rs` GPUI glue, `view.rs` the status-bar notice). Tests are offline: a fake HTTP client serving a release signed with a throwaway key, fake bundles in temp dirs, swap and rollback. Debug builds only: `BLYGGER_UPDATE_URL` (a local test server; plain HTTP to localhost allowed), `BLYGGER_UPDATE_PUBKEY` (a test key) and `BLYGGER_UPDATE_SMOKE=restart|quit` (act on a ready update without input) for manual smoke tests.

## About window

**Burrow › About Burrow** (the standard app-menu place) and **Help › About Burrow** open one small window (`ShowAbout`; a second request brings it forward):

- **Build:** version, commit short SHA (`-dirty` if the tree was dirty), build date (UTC), profile, architecture of the running slice (and "universal binary" when the executable is fat), macOS version.
- **Updates:** `auto-update`, the last check (`state.json` `last_update_check`), the updater's state (ready to install, available, checking, or why checks are off), and a **Check for Updates…** button (the menu's check).
- **Connection:** the blyg's host only (never the token), and what the backend has recorded about the server: reading extensions (`read_extensions_available`), read-state sync (meta `read_sync`, `Backend::read_state_sync`), provenance (`provenance_available`).
- **Files:** the data directory and config file, each with **Reveal in Finder**.
- **Links:** GitHub, this version's release page (`/releases/tag/v<version>`), License. **Copy build info** puts a plain-text block with all of the above except the paths (they contain the user name) on the clipboard, for bug reports.
- As built: `crates/blyg-app/build.rs` embeds `git rev-parse --short HEAD` and `git status --porcelain` (as `unknown` when git is missing or the source isn't a checkout of this repo) and the build time (`SOURCE_DATE_EPOCH` when set; `scripts/bundle.sh` sets one for all slices). It re-runs only when HEAD, the index, the refs or a file under `crates/` changes. The release workflow fails on a dirty tree. `crates/blyg-app/src/about/` (`info.rs` is the pure data and copy text). Snapshot: `BLYGGER_DEMO=about`.

## In-app browser

Links in posts open in a browser pane instead of the default browser (`open-links = app | browser`, **default app**). Code: `crates/blyg-app/src/browser/`.

- **Where:** a click opens the pane from the right, over the reading view (two thirds of the window); ⌘-click opens it over the whole area below the title bar; ⌥-click opens the default browser (with `open-links = browser`, a click goes to the default browser and ⌥-click to the pane). esc closes it; ⇧⌘B (View › Show/Hide Browser Pane) brings back the last page, or with none yet opens the pane blank with the address field focused (as View › Open Browser… does). Clicking the same link again doesn't reload. No tabs: `target=_blank` and `window.open` navigate the pane. The reader's and the preview's web views are cut off at the pane's edge (or hidden) while it's open. Other screens open links through `MainView::open_link` (click modifiers decide) or `MainView::open_url_in_app(url, OpenMode)`.
- **Chrome (GPUI):** back, forward, reload/stop (⌘[ ⌘] ⌘R), the address field showing the page title (click or ⌘L to edit; ⏎ loads; only http(s), a bare host gets `https://`), a load progress bar, 🛡 (content blocking on/off for this host), ↗ (open in the default browser), → Notes (appends `[title](url)`, or the page's selection quoted with that link, to the notes drawer; § Notes drawer), ×.
- **Engine:** the same WKWebView (wry) as the reader, a separate instance: created on first use, dropped 3 minutes after the pane closes (its WebContent process exits); reopening then loads the last URL again.
- **Security:** no IPC handler or host script (`window.ipc` is undefined in the page), so pages have no bridge to the app and never see the owner token or keys. Its own website data store: persistent under a fixed identifier on macOS 14+ (sign-ins survive a relaunch, apart from the reader and preview, which use the default store), in memory before macOS 14. Navigation: http(s), `about:blank` and `about:srcdoc` only; `mailto:` goes to the system; `file:`, `data:`, `javascript:`, `blob:` and custom schemes are refused. Downloads are cancelled and handed to the default browser. The bundle allows http:// in web content only (`NSAllowsArbitraryLoadsInWebContent`).
- **Content blocking** (`content-blocking = true | false`, **default true**): WebKit content rule lists (`WKContentRuleList`) built from uBlock Origin's default lists: uBlock filters (ads, badware, privacy, quick fixes, unbreak), EasyList, EasyPrivacy and Peter Lowe's list, fetched from their canonical URLs about weekly (checked on first use and every 6 h while running; never with `BLYGGER_FAKE` or `BLYGGER_NO_BLOCKLIST_DOWNLOAD`) into `<data dir>/browser/lists/`. Until the first download a small bundled list of ad and tracker hosts is used. Conversion: Brave's `adblock` crate (content-blocking feature) in a child process (`blygger +convert-blocklists`, so its memory goes back to the system), `##` rules become `css-display-none`, `#@#` exceptions are folded into the generic rule they undo, rules WebKit can't parse are dropped. WebKit's per-list limit is 150 000 rules, so rules are split into lists of 60 000, each ending with every exception (`ignore-previous-rules` only reaches its own list). Compiled lists are cached by WebKit in `<data dir>/browser/compiled/`, keyed by the lists' content hash (`manifest.json`); a launch looks them up in ~0.1 s. The first page waits at most 0.4 s for the lists. The shield is per host (`state.json`, `browser_unblocked_hosts`), applied before each main-frame navigation. Not possible with content blockers: scriptlets (`##+js`), `$redirect`, `$removeparam` (so uBO's URL-tracking-parameter list is left out), `$csp`, procedural cosmetics (`:has-text`, `:upward`, …) and entity (`site.*`) cosmetics.
- **Checks:** `scripts/browser-block-check.sh` serves a local page with "ads" (a third-party image and script, a `.ad-slot` box) and runs `BLYGGER_DEMO=br-block`, which probes the page with the shield on, off, and on again (`browser-probe …` lines). `br-slide`, `br-full`, `br-reader` and `br-teardown` are the other scenarios.

## Extensions

Extensions add commands and libraries to Burrow without touching its core. Nothing runs unless the config names it.

- **Model:** an extension is a separate process that speaks newline-delimited JSON-RPC 2.0 over stdin/stdout ("BXP", the Burrow Extension Protocol, `protocolVersion: 1`). The wire shape borrows from MCP (an `initialize` handshake with `protocolVersion` and capabilities, `notifications/*`, `shutdown`), but the methods are Burrow's own. Any executable that speaks it can be an extension (Python, Node, Rust, a shell script). In-process plugins (no stable ABI, a crash takes the UI down) were rejected. WASM is deferred: the protocol doesn't depend on the transport, so a WASM runner can be added later as a second runner, and it is where enforcement would come from.
- **Isolation:** every call runs on a background thread with a per-method timeout (initialize 5 s, library search 2 s, read 5 s, command 60 s, shutdown 2 s), so the UI never waits on an extension. A crash restarts it with backoff (1 s, 5 s, 30 s), and after three failures in 10 minutes it stays stopped until Reload Config. The child gets a scrubbed environment (no `BLYGGER_*`) and its own `storageDir` under the data dir. Its stderr goes to a capped log there. The extension never receives the owner token, AI credentials, the Keychain service, the database or the data dir. Burrow makes every API call for it. The protocol has no method to publish, withdraw, pin, restore, delete, fork, subscribe or change blyg settings.
- **Capabilities** (the manifest declares them, the config grants them):
  `items.read` (own items' metadata and Markdown), `items.write` (create drafts and scratch notes, save a working copy; it can never publish), `reading.read` (subscription posts already held locally), `blyg.identity` (the blyg's origin, never the token), `ui` (toasts, open an item), `hooks:itemPublished`, `hooks:itemSaved`, `hooks:itemCreated` (receive those notifications), `browser.capture` and `browser.automate:<origin>` (see Browser below), `net` and `fs:<path>`. All but `net` and `fs:` are enforced by the host. A call without the grant fails with `-32001 permission denied: <capability>`. **`fs:` and `net` are declarations, not enforcement:** a native subprocess runs with the user's rights, and the consent sheet says so plainly.
- **Grants live in the config file**, as repeatable keys, which also makes them reviewable and portable:
  ```
  extension = markdown-notes
  extension-allow = markdown-notes ui
  extension-allow = markdown-notes fs:~/Notes
  extension-setting = markdown-notes vault=~/Notes
  ```
  `extension = <name>` enables one, `extension-allow = <name> <capability>` grants one capability, `extension-setting = <name> key=value` is one setting (the extension sees only its own settings). Names and setting keys are lowercase kebab-case, and an unknown capability is a config error.
- **Consent:** the first start of an enabled extension, or a manifest that asks for capabilities not granted yet, shows a sheet ("markdown-notes wants to: read and write files in ~/Notes · show messages"). Each capability can be unticked. Allow writes the `extension-allow` lines (comment-preserving write-back) and starts it. Not now leaves it stopped, with a status-bar notice that reopens the sheet. An extension may ask again with `burrow/requestCapability`, once per capability per session.
- **Install and discovery:** an extension is a folder with an `extension.toml` manifest (name, version, protocol, command, capabilities, commands, libraries, settings) in `~/.config/blygger/extensions/<name>/` (`BLYGGER_EXTENSIONS_DIR` overrides it). Install means copying a folder. There's no registry, download or auto-update. Third-party extensions are allowed in v1. A folder can't shadow a bundled extension's name. `blygger +list-extensions` shows what's installed, enabled, granted and running.
- **Contributions:** commands go in the Extensions palette (Post › Extensions…, and the command palette's Extensions group), filtered by where they apply (always, editor, reading, notes). A **library** contribution (`extension/library.list`, `library.search`, `library.read`, `library.write`, the last refusing a stale `baseHash` with `-32002`) gives the host a browsable, searchable, writable collection of Markdown documents. The quote picker can show a library as a read-only chip. Hooks deliver publish, save and create events to extensions granted them. Automatic export happens only for published posts, through `hooks:itemPublished`. Drafts are never sent on their own, only through an explicit command.
- **Browser:** the browser pane is an extension surface behind two enforced capabilities. `browser.capture` gives `burrow/browser.page` while one of the extension's commands runs: the page's address, canonical URL, title, selection, readable text as Markdown and author (`-32004` with no page), never cookies, storage, headers or script. `browser.automate:<origin>` names one website exactly (http(s), host, optional port, no path or wildcards; normalised: lower-case host, `xn--` for international names, default port dropped) and lets the extension's `[[macros]]` run there and `burrow/browser.open` load URLs on it; the consent sheet says "fill in and, when you confirm, click Post on social.example.com, signed in as you". A macro is a declarative recipe in the manifest, run by Burrow, never stepped by the extension: a `[[sites]]` entry (origin, sign-in `home`, optional `signed-out` selector, `content-blocking`, `min-interval`) and steps `open`, `waitFor`, `assert`, `focus`, `click`, `insert`, `submit`, `done`, over `querySelectorAll` selectors with an optional exact text. Its text comes from a template (`{{title}}`, `{{excerpt}}`, `{{permalink}}`) and, optionally, `extension/macro.prepare`. The rules are checked when the manifest loads (`crates/blyg-ext/src/recipe.rs`): every `open` on a declared site, the first step an `open`, exactly one `insert` with a `submit` after it and none before, no `click` after `insert`, `done` last, timeouts at most 30 s. Every run shows the text first (editable), fills the composer in the pane the user watches, shows what the composer holds, and submits only when the user clicks Post; nothing is scheduled, repeated or run in the background. A run uses the ordinary side pane (from the right, two thirds of the window; an open pane is reused as it is, full width after a ⌘-click), and the Preview and Post sheets sit beside it over the app's content so the page stays in view (over the window, the web view hidden, when the pane is full width or less than 360 pt is left beside it). The user can fold the pane while a run goes on (esc, ×, ⇧⌘B): the run keeps going, the web view stays alive (parked off the window's edge, never torn down while a macro runs), and the status bar shows "macro running · show pane" (a click or ⇧⌘B unfolds it). Before any step that types in the page (`focus`, `insert`) and for the Post sheet, the pane unfolds itself: a macro only fills in the pane the user watches. When the run ends the pane goes back to how it was before (folded again if it was folded), except when the user folded or unfolded it during the run, or the page still matters: "I'll click Post myself", a failure (signed out, a step missed), or a stop after Post was clicked. Code: `browser_run_open`, `macro_unfold`, `macro_restore_pane`, `macro_sheet_right`. The pane and macros share the pane's own website data store, where the user signs in by hand; Burrow never reads cookies and never fills a sign-in form.
- **Reading slots** (`blyg_ext::reading`, `extensions/reading_slots.rs`): blygger-studio's extension slots `entryByline` and `entryActions`, over BXP. Behind `reading.read`, an extension can put a short marker at the end of each reading entry's byline (`entry-byline`, `extension/entry.byline`, asked off the main thread when an entry is first drawn and cached by entry and version, so nothing waits and no answer shows nothing) and rows in the entry's ⋯ sheet (`[[entry-actions]]`, `extension/entry.action`): a "⋯" chip in the stream's and the reader's actions lists them, and the answer is drawn natively (text, label/value fields, a monospaced code block with Copy). **Bundled, both off by default:** `reading-time` ("· 4 min" on the byline, the word count on hover) and `inspect` (the record Burrow holds for the entry: ids, versions, references, hashes, and its JSON with bodies elided), ports of the studio 0.39.0 extensions with the same rules, run as `blygger +ext reading-time` / `+ext inspect`. docs/EXTENSIONS.md § Reading slots.
- **Bundled: markdown-notes.** A standalone notes reader and writer over folders of Markdown files (Obsidian vaults, say), deliberately kept apart from blygs, so it works as its own app and with no blyg connected. **Several vaults:** `extension-setting = markdown-notes vault=~/Notes` is the first, and each `vault-<label>=<folder>` (e.g. `vault-work=~/Work/Vault`) adds one; a key per vault survives the config's "later line for the same key wins", and labels follow the config's kebab-case key grammar, so existing `vault=` configs read as before. Each vault is its own library (`notes`, `notes.<label>`), titled by its folder's name (or the label when it's more than that name), and the manifest asks for `fs:<folder>` for each, so consent names every folder. There is no default folder: with no vault it asks only for `ui` and has no library. A vault without its grant refuses with permission denied while the others work. It browses each folder tree, searches titles and bodies (an in-memory index, refreshed by polling), reads a note, creates notes and saves edits. A save is refused if the file changed on disk since it was read, so nothing is overwritten, and it never deletes. Burrow shows notes in its own notes panel. **Copy into post** inserts the selection or the whole note into the current draft at the cursor, either as a quote or verbatim (the user picks). This is a host action, not `items.write`. The **Save selection to notes** command writes a new plain note in the first vault it can use (a command carries no library, so not the drawer's chosen one). It adds no ids or frontmatter to notes (existing frontmatter is kept untouched), and nothing is exported when you publish. It needs only `fs:<folder>` per vault and `ui`, and it runs as `blygger +ext markdown-notes`, through the same host path as a third-party extension. Opt-in through the config and consent like any other.
- **In the window** (`crates/blyg-app/src/extensions/`): the host starts about a second after the window opens, answers extensions from the window's backend (a switch, so connecting or disconnecting a blyg needs no rewiring, and nothing needs a blyg), follows Reload Config, and stops its children on quit. Its toasts name the extension; a failure or a pending permission is a status-bar notice (a click opens Manage extensions or the consent sheet). The **consent sheet** (the Publish/Delete sheets' chrome) lists each capability in `blyg_ext::describe`'s words with a tick (1–9 toggle), then `CONSENT_CAVEAT`; ⏎ Allow writes the ticked `extension-allow` lines (unticked ones aren't asked again this session), esc Not now leaves the notice, ⌘⌫ Turn off backs out (as Manage's Turn off). **Extensions…** (Post menu; its rows are also the command palette's Extensions group) lists the running extensions' commands for the screen, "Browse <library>", and **Manage extensions…** (what's installed, its state, what's allowed and not, Turn on, Allow…, manifest problems; **Turn off**, in any state, removes its `extension` line, so the reload stops it and its notice goes, while its `extension-allow` and `extension-setting` lines stay for the next Turn on; **Forget permissions** turns it off and removes its `extension-allow` lines too). A command runs off the main thread with the selection (notes drawer, browser pane, reading pane or editor), the open item and the screen. A **library** is a tab in the notes drawer ("Reading notes | Notes"; markdown-notes' tab shows once it's on, even with no folder, where it says so with **Choose a folder…**). markdown-notes' vaults get a switcher above it: a chip per vault (one not allowed yet in the warning colour, and choosing it shows "Burrow needs your permission…" with **Allow…**), **Add folder…** (the folder picker, several at once), and the chosen vault's folder with **Remove from Burrow** (drops its `vault…` line and its `fs:` grant; never touches the folder). The chosen vault is remembered (`state.json`, `notes_vault`). Under it: breadcrumbs, a search box (each keystroke searches again, only the newest answer shows; after 2 s "Notes didn't answer"), folders and notes; a note opens in an editor showing its body (frontmatter kept aside and written back unchanged), Save names the hash it was read at, and a refused save says "Changed on disk" and offers Reload or Keep mine as a new note. **Quote into post** / **Copy verbatim** put the selection, or the whole note, into the open draft at its caret (a new draft when none is open). The quote picker (⌘K) has a "notes" chip that searches every vault (with more than one, each row names its vault; a vault not allowed yet is skipped) and quotes the note as a `>` blockquote naming it, never `![[…]]`. Search, New note, Quote into post and Copy verbatim act on the chosen vault. Settings › Notes folders lists the vaults, each with Remove, and **Add folder…** (several at once) writes `extension = markdown-notes` and a `vault` (the first) or `vault-<label>` line per folder (comments kept), then the consent sheet asks for the new folders. Publishing calls `Host::item_published`.

## Several blygs (the blyg switcher; `switcher.rs`, `blyg_core::config::blygs`)

- Sign in to as many blygs as you like; one is active. `blyg-url` is the active blyg (unchanged meaning);
  the repeatable `blyg` key lists every signed-in blyg in switcher order, written only once there are two
  and dropped again at one. A blyg is its host (as the Keychain keys its sign-in), so Burrow holds one blyg
  per host: signing in to another on a known host (a different port or path) is refused with "sign out of
  that one first", since it would take over the first one's sign-in and database.
- Each blyg has its own data: the blyg that owns the data dir's database keeps it (today's layout, so
  nothing moves for an existing install); every other blyg uses `<data dir>/blygs/<host>/` with the same
  `blygger.db`. Drafts, reading, outbox and read state never mix. `state.json`'s `blyg_cards` keeps each
  blyg's name and avatar for the switcher.
- The title bar shows the active blyg (avatar or initial, host, ▾) as a button. Click it, Blyg › Switch
  Blyg… (⌃⌘0) or Blyg › Switch To (⌃⌘1–⌃⌘9): a sheet lists every blyg
  (1–9 / ↑↓⏎), Add a blyg… (the Connect sheet; the others stay signed in) and Manage… (sign out of one:
  1 keeps its posts on this Mac, 2 deletes them). On Windows the blygs are Ctrl+Shift+1–9 and the sheet
  Ctrl+Shift+L (Ctrl+Shift+0 is the system's input-language key there, and never reaches the app); the
  keys are bound through the keyboard layout's mapper, because Windows reports Shift+digit as the
  character it types (Ctrl+Shift+1 arrives as Ctrl+! on a US layout).
- Switching is instant: unsaved editor text is saved, the backend swapped, the posts, reading, mentions
  and subscriptions reload on the same screen, and the old blyg's sheets close. The old blyg stops
  syncing; its outbox waits in its own database and syncs when it is active again (one sync worker at a
  time; a quick A→B→A waits for A's old worker to stop). Nothing the old blyg's last pass says (status,
  conflicts, errors, a sign-out) reaches the window after the switch. A blyg with no credential left opens
  Connect with its address filled in.
- Disconnect signs out of the active blyg and switches to the next; with none left, Connect opens.
- CLI: `blygger +list-blygs`; `--blyg <host>` on `+import-opml` / `+export-opml`.

## #thicket chat (experimental; `chat/`, `crates/blyg-irc`)

**Experimental: this may be removed** if not enough people use it or people don't like it. It's marked so in the tab, its setup screen, Settings, the menu and the docs.

- **What:** a built-in IRC client limited to one channel, `#thicket` on Libera.Chat (`irc.libera.chat:6697`, TLS), in a tab after the views in the title bar: "#thicket" with an *experimental* badge (never "Thicket" alone: thicket already names following quotes). ⇧⌘I (Ctrl+Shift+I; Blyg › #thicket Chat (Experimental)) brings it to the front and again goes back to Posts. It's a `reading::View` (`View::Chat`, not in `View::ALL`), drawn outside the reading keys. The code is channel-agnostic; only the constant `THICKET` picks the channel.
- **Engine:** `crates/blyg-irc`, written for Burrow (MIT): a message parser (tags, prefix, CTCP, formatting codes, rfc1459 folding), a pure session state machine (CAP LS 302, CAP REQ sasl, AUTHENTICATE PLAIN, 900/903/904, 001, 005 PREFIX/CHANMODES, JOIN, NAMES 353/366, MODE, PING), a `Room` (members, a bounded in-memory log of 1,000 lines), and a blocking client on its own thread over rustls (ring) with the system's roots (`rustls-native-certs`), reconnecting with backoff (2, 5, 15, 30, 60 s; a refused password stops). Halloy is GPL-3.0 and the async IRC crates bring tokio, so neither is used. Nothing in it logs.
- **Setup** (until `chat-nick` is set): "I have a Libera.Chat account" (nick, password; SASL; kept only once 900 says it worked) or "Create an account" (nick, password of 8+ characters with no spaces, email; connect without SASL, `NickServ REGISTER <password> <email>` after 001, NickServ's notices shown verbatim, then the emailed code: `NickServ VERIFY REGISTER <nick> <code>`, accepting the whole pasted line; NickServ's "has now been verified" finishes it and reconnects with SASL). **Nothing connects until the user acts in setup**, and the tour (step `chat`, 0.18.0) shows the setup screen and never connects. Libera's registration guide gives the REGISTER flow; the VERIFY syntax is the standard NickServ (Atheme) one.
- **Secrets:** the password is in the Keychain under the app's service (`org.blygger.desktop`), account `irc:<network>:<nick>` (`irc:libera:ada_example`), through the same `TokenStore` (Credential Manager on Windows); the nick is `chat-nick`, the server `chat-server`. Sign out (the tab's header) deletes both. After setup Burrow signs in at launch (never on the sample data).
- **The screen:** header (#thicket, EXPERIMENTAL, the topic, the connection quietly: connecting / signing in / connected / offline · retrying in N s), the log (time, nick, `* nick` for /me, `-nick-` for notices, joins/parts/quits/nick changes in small muted lines, mentions marked), a user list (ops `@` first, then `+`, then the rest), and a compose box (⏎ sends; `/me`; `//` escapes a slash; other commands are refused with a toast).
- **Marks and notifications:** while the tab isn't in front (or the window isn't active), others' messages count as unread (a dot and a count on the tab) and a mention of your nick or a private message is a stronger accent "@n". A mention posts a system notification (macOS User Notifications from Burrow.app, and one Dock bounce while inactive; nothing on Windows yet), unless `chat-notifications = false`. Opening the tab clears the marks.
- **Default and switch:** `chat-tab` is **on** by default: the tab costs nothing until set up (nothing connects), and a trial no one can find can't tell us whether people want it. Settings › Chat hides it (and disconnects), and has "Notify me of mentions".

## Terminal pane (experimental; `terminal/`, `crates/blyg-term`)

Fable's proposal 11, without its `blygger +mcp` verb: blygs already serve MCP (`endpoints.mcp` in `/api/capabilities`), so an agent in the pane is pointed at the blyg's own MCP server and signs in to it itself (site page `terminal.md`). **Experimental**, so marked in the pane's header, the menu and the docs.

- **What:** the user's login shell in a pane at the foot of the window: a layout row between the body and the status bar (like the reading compose box), about a third of the window's height, never over the editor. ⌃\` (Ctrl+\` on Windows; View › Terminal (Experimental)) shows it, starting the shell the first time, and gives it the keyboard; ⌃\` with the keyboard in it hides it. Hiding keeps the shell; the header's "end", the shell exiting, or `terminal = false` ends it. ⇧⌘T (Ctrl+Shift+T; Post › Send Selection to Terminal) pastes the editor's selection as a Markdown quote plus a blank line, never followed by ⏎. Header: the shell's name, *experimental*, "scrubbed env · no BLYGGER_* · no token", "⌃\` hide", "end".
- **Engine:** behind `blyg_term::Engine` (`feed`, `resize`, `snapshot`, `encode_key`, `take_replies`, `scroll`, `text_between`). **In use: `alacritty_terminal` 0.26** (Apache-2.0; its grid and VT parser only, not its pty or event loop). libghostty-vt 1.0 (MIT, tagged 2026-11-06) was the design's choice; every crates.io route to it builds Ghostty with Zig or downloads a prebuilt archive during the build (`ghostty-vt-sys`, `libghostty-vt-sys`, `mnml-libghostty-vt-sys`: Zig; `ghostty-vt-rs-sys`: curl at build time, no Windows or Intel Mac archive; `spyc-vt-sys`: vendored archives of a pre-1.0 commit, macOS and Linux only; `gpui-libghostty`: downloads at build time), and CI builds with plain cargo. Hand-written bindings against the 1.0 headers (terminal + render state) sit behind `blyg-term`'s `libghostty` feature, off, type-checked but never linked (`GHOSTTY_VT_LIB_DIR` names a library the user built). Answers to queries (DSR, DA, OSC 10/11 with the theme's ink and paper, size reports) go back through the pty; OSC 52 is off both ways; synchronized updates (mode 2026) are honoured.
- **Pty:** `portable-pty` 0.9 (wezterm, MIT): a Unix pty on macOS, ConPTY on Windows; reader, writer and waiter threads; the output reaches the UI thread through a wake channel and is fed to the engine there. Dropping the session kills the one process it started. The shell: `terminal-shell` (a program and arguments) or `$SHELL -l` (the password database's shell if unset) on macOS, `%COMSPEC%` on Windows, in the home directory.
- **Drawing:** GPUI, no second layer: a canvas lays out the grid (Menlo / Consolas, 12.5 px, cells from the face's advance), resizes the engine and the pty to fit, and paints background runs and the selection as quads, text as one shaped run per row (split at double-width characters so every glyph lands on its cell, `force_width` the cell), bold/italic/underline/strike/dim/inverse/hidden, and the cursor (a block when focused, an outline when not, a bar or an underline). Default colours are the editor's ink and paper; ANSI 0–15 come from the theme (over, green, amber, accent and mixes), 16–255 the xterm cube, 24-bit as asked.
- **Keys:** a keystroke interceptor gives the focused pane every key ahead of the window's bindings (so ↑, ⏎, esc and ⌃C reach the shell, not the list), except ⌘-keys on macOS and Ctrl+Shift-keys on Windows (the app's) and ⌃\`. ⌘C / ⌘V (Ctrl+Shift+C / V) copy the selection and paste (bracketed when the program asked; end markers and escapes removed). xterm's legacy encoding: application cursor mode, modifier parameters, ⌥ as Meta unless it composed a character. ⇧PageUp/⇧PageDown and the wheel scroll the 5,000-line history; on the alternate screen the wheel sends arrows. Not yet: mouse reporting, IME composition, search, links, tabs.
- **Safety:** the shell is the user's with the user's rights (as Terminal.app's), so it keeps their environment minus every `BLYGGER_*` variable (one is a test token), `GPUI_*` (what Burrow sets for itself) and the launching terminal's identity (`TERM_PROGRAM*`, `ITERM_*`, `TMUX`…), with `TERM=xterm-256color`, `COLORTERM=truecolor`, `TERM_PROGRAM=Burrow` (`blyg_term::env::scrub`, tested on the real child). Burrow never puts a token, an AI key, the database or the data dir into its environment, a file or the clipboard for it. **No IPC bridge:** Burrow and the shell meet only at the pty; an agent changes the blyg through the blyg (MCP or API), and Burrow sees that on sync like any other change.
- **Default and switch:** `terminal` is **on** by default (nothing runs until ⌃\`); `terminal = false` turns it off and ends a running shell. The tour's step `terminal` (0.18.0) counts the key showing the pane; in the app's tests the pane runs no shell.
## Flow: palette, focus, serendipity, motion (0.18.0; Fable's proposals 8, 6, 9, 13)

- **Command palette** (`palette.rs`, module `app::command_palette`): ⇧⌘P (Ctrl+Shift+P; View › Command Palette…) opens a sheet with a field and a list. It lists every action in the keymap table (one row per action, its menu leaf as the label, its menu as the group; main-window and global keys only, and not the tour's arrows, the numbered blyg keys or the palette itself), the themes, the writing and interface fonts, the blygs (switch), the hoppers (Reading › Reader on that hopper), the running extensions' rows as one **Extensions** group, and your posts by title (not withdrawn). Matching is fuzzy over the label, the group and a few aliases per action ("undo edits" → Discard Changes, "zen" → Focus). Every row shows its key (Windows spelling on Windows). An action that can't run now stays listed, dimmed, with the toolbar's reason, and choosing it says why. The empty field shows the last 10 commands first (`state.json`, `palette_recent`), never posts. ↑/↓ move, ⏎ runs and closes, ⌘⏎ runs and keeps it open, esc closes and gives the focus back. ⇧⌘P was the Extensions palette; that is now Post › Extensions… (no key) and the palette's Extensions group.
- **Focus mode** (`focus.rs`): ⌥⌘F (Ctrl+Shift+F on Windows, since Ctrl+Alt is AltGr; View › Focus) builds on ⌘3: the editor alone, no omnibar, list, preview or toolbar. The view isn't saved, so the next launch opens in the view you left. **Typewriter scroll**: the caret's line is held at `focus-line` (0.1–0.9 of the pane's height, default 0.45), with padding above and below so the first and last lines can reach it. **Fading** (`focus-fade = paragraph | off`): paragraphs other than the caret's are under a veil of the editor's background. The chrome is a hairline at the bottom (the sync dot and a private word meter for this sitting: words written since focus mode began, never stored or shown elsewhere); hovering it shows the status bar. ⌘L still reaches the omnibar (it shows while it has the keyboard), and esc leaves. *Deviation:* gpui-base's textarea has no scroll policy or per-range colour, so both are done at the app level: the scroll offset is corrected each frame toward the line, and the veil is a canvas painted over the lines outside the paragraph, measured from the textarea's own layout.
- **Serendipity** (`serendipity.rs`; `serendipity = off | own | on`, **off** by default; Settings › Serendipity): ⌘N's empty editor shows a card with one published post of yours: never withdrawn, weighted toward orphans (nothing of yours links to, quotes or stubs it) and posts from this week in another year, and not shown again for 30 days (`state.json`, `serendipity_seen`). `on` also offers posts held from your subscriptions; `own` never does. While the editor is empty: **q** quotes it (`![[id | title]]`, the note becomes a thread), **l** links it (`[[id | title]]`), **o** opens it, **r** shows another. The first keystroke of the post dismisses the card. The tour's step shows the card on the sample posts without changing the config.
- **Motion and sound** (`delight.rs`): every animation is under 300 ms and a cut with macOS's Reduce Motion (or `BLYGGER_REDUCE_MOTION=1`): a published post's row lifts and settles, the sync dot's colour eases between states, the counter's colour eases, the list crossfades when the blyg changes, focus mode's hairline slides in. `sounds = off | quiet` (off by default): a quiet macOS system sound by name (NSSound: Pop on publish, Tink on capture; nothing bundled or downloaded), never on typing, nothing on Windows. No streaks, counts or confetti.
## Nearby thoughts (`nearby/`, `crates/blyg-embed`; Fable proposal 1)

- **What:** a rail on the editor's right (Posts, a post open; 236 pt) listing up to five of your own published posts and current posts held from blyg subscriptions that say the same thing as the **paragraph under the caret** (never the whole document). Each row: title, who (You / the blyg's name) · kind, and why (the shared phrase in quotes, accent-coloured, or the closest passage). Footer: how it's matching (shared phrases / downloading / reading your posts N% / by meaning · model / shared phrases only: why). ⌥⌘U (Ctrl+Alt+U; View › Nearby Thoughts) shows or hides it, remembered in state.json (`nearby_hidden`). Quick capture has no rail.
- **Keys** (`Blygger > Input`, out-ranking the text field, which binds none of them; they pass through when the rail has no rows or the editor isn't focused): ⌥↑/⌥↓ choose; ⌥⏎ writes `[[id | phrase]]` over the shared phrase in place (the phrase as typed is the label), or `[[id | title]]` after the word at the caret when the match is by meaning only; ⌥⇧⏎ writes `![[id | title]]` on its own line after the paragraph, threads only (a fragment gets the "Quotes go in threads" toast). Labels are dropped below studio 0.47 (`floor::picker_label`). A written row leaves the rail; already-linked or quoted targets, and the post itself, are never offered.
- **Speed:** a keystroke only re-arms a 300 ms timer (and the caret watch compares an offset). After the pause the paragraph is read on the main thread; its phrases, the store's phrase search, the embedding and the ranking run in the background, and only the newest ask lands.
- **Matching** (`nearby-embedder`): **none** (default) = distinctive 2–4-word phrases of the paragraph (no stop word at either end, no punctuation, links, code or URLs inside, one word of 4+ letters; at most 12) looked up in `items_fts`/`reading_fts` (`Backend::nearby_phrase_hits`); a row needs the phrase in the title, or in your own post, or 3+ words. **local** = potion-base-8M (MinishLab, MIT; 256 dims, bge-base-en-v1.5's 29,528-token WordPiece vocabulary), a model2vec static model run in pure Rust (`blyg-embed`: safetensors read directly, BERT normaliser + pre-tokeniser + WordPiece, mean-pool of known tokens, normalised; no C, no ONNX). Downloaded only when chosen (Settings › Nearby, or the key), into `<data dir>/models/potion-base-8M@bf8b056/`, pinned to Hugging Face commit bf8b056 with sizes and sha256 (`model.safetensors` 30,236,760 B, `tokenizer.json` 683,666 B), written to `.part` and checked before kept; offline after. **endpoint** = `nearby-endpoint` (Ollama `/api/embed`, or OpenAI-compatible `…/v1/embeddings`), model `nearby-endpoint-model`, no credential sent. With an index, each post's best piece by cosine (title piece only with `nearby = titles`) above a floor (0.45 local, 0.6 endpoint); a shared phrase adds 0.15 and supplies the ⌥⏎ phrase; yours +0.03. `nearby-rerank = on` (off by default) asks the AI provider to order the best ten (paragraph + titles + 300-character excerpts), once per paragraph and candidate set.
- **Index:** store v20 `nearby_vectors(model, doc, chunk, hash, vec)`; `doc` = `own:<local_id>` | `reading:<sub>/<remote_id>`; piece 0 is the title, then the text in paragraph-sized pieces (joined to 160+ characters, long ones cut at sentence ends, at most 24); vectors f16. Built in the background from the store's vectors on first use, then kept current (2 s after ItemsChanged/ReadingChanged): an unchanged post keeps its rows, a changed one re-embeds only pieces whose text hash is new; posts no longer offered and other models' rows are deleted. Triggers delete a post's vectors when it's deleted, withdrawn (status not public) or tombstoned, and `put_nearby_vectors` refuses posts no longer offered, so a slow indexer can't bring a withdrawn post back. Brute-force cosine; measured in `nearby::tests::speed_at_scale`.
- **Never:** fetch anything to suggest (only what's held), offer a withdrawn post, RSS posts (a link can't name them) or drafts, send text anywhere unless `endpoint` or `rerank` was set.

## Loose ends, capture from anywhere, contacts (0.18.0; Fable's proposals 10, 12, 14)

- **Loose ends (omnibar operators; `blyg_core::loose`, `loose_ends.rs`).** `Backend::search` reads operators off the query before the store: `is:orphan` (published, and nothing points at it: no held post quotes, stubs or forks it (`reading_refs`), no verified mention names it (the ids from the last `GET /api/mentions`, kept in the store's meta), no other post of yours quotes, links, stubs or forks it), `is:draft`, `is:leaf` (no `![[id]]`, stub or fork), `is:stale` (a draft untouched for 30 days), `has:tk` (an ungenerated `[TK]` scope), `links:<id|host>` (an inline `[[id]]` whose id starts so, or whose held post is on that host, or a web link to it), `quotes:<id|host>` (a `![[id]]`, stub or fork, likewise), `kind:thread|fragment`, `v:<n>`. A word that isn't one of these (`is:blue`) searches as text; the other words search as before (FTS from 3 characters), and a query without a `:` costs one scan and nothing else, so the omnibar's off-thread search and its generation numbers are untouched. Operators alone never create on ⏎ (the hint says "nothing loose", the list "Nothing loose here."). With the omnibar empty, a muted line over the list says what's loose, in words ("2 orphans · 1 ungenerated TK · 1 draft older than a month"), each part a click to its operator; worked out on a background thread after the list changes. It describes the author's own text, never anyone's attention: no social count.
- **Capture from anywhere (`capture_service.rs`, `capture_link.rs`).** macOS: an `NSServices` entry, "Capture in Burrow" (`packaging/Info.plist.in`; `NSMessage captureInBurrow`, text send types), and a services provider registered at launch (`NSApp.servicesProvider`, an objc2 class). macOS sends the selected text, and from a browser sometimes the page's `public.url`; quick capture opens holding `> text` and `> — [host](url)` (the reading pane's `quote_block`), after whatever it already held. No Accessibility permission: a Service gets only what the user selected and sent. Services can't take ⌃⌥B as their key (`NSKeyEquivalent` is ⌘-based), so the user assigns one in System Settings › Keyboard › Shortcuts › Services if wanted. Windows: the clipboard's text, polled once a second, is offered in the panel as a "quote clipboard" chip when it changed in the last 10 s, never inserted silently. The omnibar (and only the omnibar): a lone `http(s)://` address with no matches offers "⏎ start a fragment with this link"; ⏎ fetches the page's `<title>` once with the token-less public client (`Backend::page_title`) and starts `[title](url)` (the address as its own text without one). An address naming a blyg post held here starts a thread with `![[id]]` instead, as the notes drawer does.
- **Studio 0.56 parity (contract 5.7.0; `blyg_core::contacts`, `api/contacts.rs`, `reading/mentions_feed.rs`).** Asked only of a studio whose capabilities say 5.7 or later. `GET /api/contacts` (`{handle, name, origin}`, cursor pages) is fetched in the background when the @ popup opens (at most every ten minutes) and kept in the store's meta, so the popup reads it instantly. The @ popup lists contacts first (then subscriptions, reading, profiles, as before); picking one writes `@handle` where the blyg rewrites handles on publish (its second line shows `@lin → lin.example.org`), else the plain link as before. `unresolvedHandles` from publish becomes the publish toast's warning ("not in your contacts, left as written: @sam"). Reading has **Mentions** under All unread: the held posts that mention you, from `GET /api/reading?mentions=true` when served, else from what this Mac holds (a quote, stub or fork of one of your posts, or a link to your blyg's host). A list with a dot, never a count. The repo's OpenAPI fixture is 5.0.0 and lacks these shapes; they follow the studio 0.56 summary, and the mock serves them under `unchecked` paths. Not yet: "Add to contacts…", and the preview rendering `@handle` as the h-card link.

## The public site (Blyg › Site…; `site/`, `blyg_core::site`, `api/site.rs`)

Studio parity for the settings that run the blyg's public site, which Site Settings (title, author, bio, masthead links, time zone, writing defaults) doesn't hold. One sheet (menu and command palette only, no key), in Studio's sections, each shown only when the studio reports its keys:

- **Appearance:** the reading theme (Auto, Studio's six presets, Custom) as swatches, and a preview of a public page in the chosen palette and typeface (light and dark side by side for Auto and for a custom theme with a dark companion). Custom is Studio's theme editor: eight `#rrggbb` colours (`page paper ink inkSoft rule pencil genBg genRule`), a name (one line, at most 40 characters), the dark flag, an optional dark companion (the pair must be light + dark), "start from" a preset, and Studio's advisory WCAG contrast warnings (4.5:1). Typeface: Studio's eight system stacks, previewed with the stack's macOS or Windows face.
- **Home page:** the order (`feedOrders` as the studio lists them, extensions' too), the static front page (a published thread of yours, or the latest items), the posts path (Studio's rules: 1–40 of `a-z0-9-`, not a name the blyg uses).
- **Links:** where the main menu sits, the threads it lists first (pick, reorder, remove; at most 30), your own links (`Label | url` lines), and the four standard links. The masthead links stay in Site Settings.
- **Profile:** the avatar (Upload… sends the image with no post, `POST /api/media`, and sets `avatarMediaId` to its id; shown only after an upload, as in Studio), `authorUrl`, `siteUrl`.
- **Network:** collection (hopper) feeds, `hopperFeeds`.
- **Updates:** `updateCheck` and `updateFeedUrl`, and what the studio last found.
- **Extensions:** the studio's own (from `GET /api/capabilities` `extensions`), on or off; one not compiled into the build shows but can't be turned on.

Nothing is sent until Save (or ⌘⏎), which checks what Studio checks and then sends `PATCH /api/settings` with only the keys that changed (`SiteConfig::patch`; a key the studio didn't report is never sent). A refusal shows the server's words ("Your blyg refused it: …"). The sheet stays open, saying Saved.

**Studio update alerts.** `GET /api/update-state` (which also starts the studio's daily release check) gives the newest release the studio has seen; the version it runs is `blyg.json`'s `generator`. Read at most every six hours per blyg (asked every ten minutes, from the cache), and not at all with `updateCheck` off. While the studio is behind, the status bar shows a quiet "Your blyg's studio has an update: x.y.z · Release notes" where the "needs updating" notice goes (which wins).

## Naming

The app is **Burrow** ("Burrow is a blygger client"): the menus, the About
window, the window title, the bundle (`Burrow.app`) and the release assets
(`Burrow-<ver>-macos-universal.{zip,dmg}`). Up to 0.6.0 it was Blygger. The
rename is display-only. These keep the old name, because existing installs
depend on them: the bundle ID `org.blygger.desktop`, the Keychain service and
`blygger-ai.*` accounts, the data folder and `blygger.db`, the browser pane's
data store, `~/.config/blygger/config` and the `BLYGGER_*` variables, the
`blygger` binary (`CFBundleExecutable`) and CLI, the GPUI key contexts and
action namespaces (user keybinding configs may name them), and the crates.
"Blygger" and "blyg" still name the protocol and platform.

Releases also publish `Blygger-<ver>-macos-universal.zip` (and the version-less
`Blygger-macos-universal.zip`), holding the same signed bundle in a folder
named `Blygger.app`, because 0.6.0's updater accepts only that. An install
updated from 0.6.0 keeps its on-disk name `Blygger.app`.
