# Server requirements

Burrow talks to a blyg's **owner API**: blygger-studio's private authoring and reading
API (https://github.com/blygger/blygger-studio), documented as an OpenAPI contract
(`openapi.json`; route guide in its `docs/api.md`). Burrow 0.16 and later speak
**API contract 5.x only**, which means **blygger-studio 0.49.12 or later**. It is tested
end to end against **studio 0.49.12** (contract 5.0.0), and every route it uses is
checked against that `openapi.json` in its tests (`crates/blyg-core/tests/fixtures/`).

**A stock blygger-studio is fully supported.** Nothing in this app needs a patched
server: writing, publishing, versions, pins, media, subscriptions, mentions, settings,
the reading list, read state across Macs, hoppers, lineage, AI disclosure and the
browsing log all run on upstream's own routes. The old Worker fork's extensions are no
longer spoken; a fork must move to a stock studio.

## How Burrow checks the contract

When it connects, and before syncing (once a session, then on every sync while the
blyg is too old):

1. `GET {blyg-url}/api/capabilities`, under the blyg's **mount** (a blyg served at
   `https://blyg.example.com/blyg/` answers at `https://blyg.example.com/blyg/api/…`).
   It is authenticated, so a refused credential stops the check (`401`), never passing
   as "an older studio".
2. Its `contractVersion` decides: **major 5** is spoken. Anything older, or no
   capabilities route (a `404`: a studio before 0.49.7, or a Worker fork), is too old.
   A newer major is a contract this Burrow doesn't know yet.
3. A failed check (offline, a 5xx) decides nothing: changes wait for a verdict.

**Too old:** nothing is pushed or pulled. Burrow says "Your blyg needs updating" once
(on connecting, or when it starts), keeps a status-bar notice with an **Update your
blyg** link while it lasts, and shows the same in the About window. Every local change
stays on this Mac and syncs once the studio is updated; Burrow notices on the next sync
(a manual sync checks at once). The connect sheet refuses such a blyg with "update your
studio to 0.49.12 or later".

**Too new:** nothing syncs either, and Burrow asks to be updated.

## Signing in

- **Sign in with the browser** (the default): Burrow registers itself as an OAuth client
  of your blyg, opens the studio's consent page, and keeps the grant (a one-hour access
  token, renewed with a rotating refresh token until the grant ends, at most 30 days
  after your studio sign-in) in your Keychain. Signing out revokes it. The protected
  resource is `{blyg-url}/api` (under the mount). A grant made by an older Burrow for
  the host-rooted `{origin}/api` keeps refreshing as it was issued; nothing is migrated.
- Or make an **API token** in Studio → More → Client access (REST API, every permission:
  read, draft, publish, manage and `reading:state`; it lasts 30 days) and paste it. A
  token missing a permission gets a 403 that names it.
- Or the **studio password**: Burrow signs in at `{blyg-url}/studio/login` like the
  browser does, keeps the session cookie in memory only (the password is in your
  Keychain), and signs in again when the session ends.

When a blyg stops taking a saved credential (a revoked or expired token, an ended
sign-in), every call answers `401`. Burrow then stops syncing (no retries), keeps every
queued change, and opens the sign-in sheet once, browser first. Signing in again sends
what waited.

## Contract 5 essentials

- **Where:** every call is `{blyg-url}/api/…`, the mount included; Burrow no longer looks
  for `/api` at the host root.
- **Resources are camelCase** (`contentMd`, `stubOf`, `subscriptionId`, `readVersion`,
  `showResponsesDefault`, …). An item's `kind` stays `fragment` or `thread` after
  withdrawal; its state is in `status`.
- **Errors** are `{code, message, details?}`. Burrow shows `message`, and the entries of
  `details.issues` (validation failures) as details. `code` decides two cases:
  `revision-conflict` and `cursor-expired` (both `409`).
- **Media URLs are absolute**, under the mount; new uploads are
  `{blyg-url}/media/{id}/{sha256}.{ext}`. `POST /api/media` answers `200 {…, duplicate:
  true}` for identical bytes on the same item.

### Cursor pages

Every collection answers `{items, page: {limit, nextCursor}}`. `nextCursor: null` means
the list is exhausted; the cursor is opaque and passed back verbatim. `limit` is 1–100
(`GET /api/reading`: 1–50); `offset` and `total` are gone (an `offset` is a `400`).

- **One coherent acquisition.** Burrow reads a list from its first page to
  `nextCursor: null` before it acts on what's missing: **local rows are pruned only
  after a walk reached its end.** A walk stopped early (the reading list's page budget,
  20 pages of 100 a pull) is merged without pruning and read again next pull.
- **`409 cursor-expired`** (the list changed between two pages): every page read so far
  is thrown away and the walk starts again from the first page; a restarted page is
  never appended to the old prefix. After three restarts the list is left as held for
  this pull, and the next pull reads it again.
- Burrow pages with `limit=100` (the reading flags read `GET /api/reading?limit=1`).

### Revisions

Every item carries `revision`. Every command on an item sends the revision it was based
on, `expectedRevision`: saves (`PATCH /api/items/:id`), publish, withdraw, pin, restore,
delete, generate, attaching media (`POST /api/media` with `itemId`) and detaching it
(`DELETE /api/media/:id`). Burrow keeps each item's revision with what it last synced
(store schema 15).

On **`409 revision-conflict`** the item changed on the blyg since Burrow read it:

- Burrow reads the item again and merges it with conflict detection. If the text moved
  under a local edit, the post enters conflict (mine / theirs) and nothing is sent; you
  choose.
- If the text moved with no local edit, this Mac now shows the new text, and the command
  is **not** repeated over it (a publish or delete never acts on text you haven't seen);
  the command fails with "this post changed on your blyg since Burrow last read it".
- If only the revision moved (an attachment or a setting made elsewhere), the command
  goes once more with the fresh revision.

Nothing is ever sent blind over a change Burrow didn't see. Burrow 0.15 and earlier read
each item before every save to the same end; contract 5 makes that a single round-trip.

## Following links

The reader follows a quoted layer, a stub's target or an ordinary link that names
a blyg item into the reader, and keeps a private log of the walk. The routes Burrow
uses are in one file, `crates/blyg-core/src/api/browse.rs`.

**Which links name an item.** `POST /api/peek/links {urls}` (scope `owner:read`,
at most 40 URLs a call; Burrow sends longer lists in chunks) answers
`{links: [{url, target: {origin, id} | null}]}` for each distinct http(s) URL. The
studio looks at each page's §5.8 `<link rel="alternate" type="application/json">`
item document and remembers the answer for a week.

Where the route isn't served (a 404 or 403), Burrow remembers that for six hours and
classifies the links itself, by the studio's own rules (`src/peek.ts`):

1. a permalink of your own blyg (`{origin}f/{id}/` or `t/{id}/`) is that post;
2. a held post of a blyg subscription whose `page` is the link is that post;
3. otherwise the page is fetched with the token-less public client (its first
   256 KiB; 5 s to connect, 12 s in all, four at a time), and its first
   `rel="alternate"` JSON document is fetched: the link names the item when that
   document has a `blyg` key, a string `id` and an http(s) `origin`, and was served
   from that origin's scheme, host and port (§12.2; the studio is looser here).

Answers, hits and misses alike, are kept a week in the local `link_probe` table; a
network failure (offline, a 5xx) is asked again next time. No token ever goes to
another host.

**The browsing log.** One walk is a *trail* (8–64 characters, made by Burrow); each
step names the step it was reached from (`parentId`), so the log is a tree.

| Call | Body / query | Success |
|---|---|---|
| `GET /api/browse` | `cursor?`, `limit` (≤ 100), `trail?` | `{items: [{id, at, trail, parentId, origin, remoteId, version, via, title}], page}`, newest first |
| `POST /api/browse` | `{trail, parentId?, origin, remoteId, version?, via, title?}`; `via` is `start`, `stub`, `quote`, `link` or `back`; `title` ≤ 400 (the studio keeps 160) | `201 {id}` |
| `DELETE /api/browse` | `trail?` (none: everything) | `{ok: true}` |

The history pane reads the log page by page from the newest step. The writes need
`reading:state`. Steps are best-effort: they are sent from a background thread, never
through the outbox, and one that can't be sent (offline) is dropped. Where the studio
has no log (a 404), Burrow keeps it in the local `browse_log` table (the same columns)
and asks the studio again after six hours. A `403` (a credential without
`reading:state`) keeps the log on this Mac until the next launch or sign-in. Clearing
clears both. The log is never sent anywhere else and never appears on a public page.

## The reading list

`GET /api/reading/imported?limit=1..100&cursor=…` (`listImportedReading`): every
imported post with its Markdown, author, thumb, hoppers, read state, transclusions and
lineage, newest `observedAt` first, with no drops or repeats across pages. Rows are
camelCase (`subscriptionId`, `remoteId`, `withdrawn`, `l0`, `version`, `readVersion`,
`observedAt`, `contentMd`, `contentHtml` (sanitized), `author` `{name, url}`, `page`,
`sourceUrl`, `thumb`, `hoppers`, `transclusions`, `stubOf`, `forkedFrom`, `generated`);
dates are ISO 8601 UTC or null. Each row's lineage is the origin's own (null when none).

`GET /api/reading` (the studio's own list, also your own posts) leaves withdrawn entries
out unless `background=true`, which lists only them. Burrow reads it only for its
read-state flags.

## Read-state sync

The server keeps, per reading row, the highest version the owner has read. It never
lowers that value on a read, so every read write is idempotent and replay-safe; only a
clear ("mark unread") resets it, leaving a tombstone against stale reads.

**Capability.** `GET /api/reading` answers with top-level `readState: true` and
`readStateClear: true`; Burrow reads them from `GET /api/reading?limit=1` once a session.
Each imported row carries `readVersion` (an integer, or `null` when unread) and
`version`, the version held. The app reads the read version only when `readState` is
`true`.

**Writes** (owner auth, like every `/api` call). **Scope:** these four writes
(`markRead`, `markUnread`, `markReadBatch`, `markUnreadBatch`) need the bearer scope
**`reading:state`**; `owner:manage` alone is refused with `403`. The app spells the name
in one place, `READ_STATE_SCOPE` in `crates/blyg-core/src/api/oauth.rs`. The browser
sign-in asks for it when the studio lists it in `scopes_supported`. A client registered
before (Burrow 0.10) is reused; the studio grants it the new scope.

| Call | Body | Success | Errors |
|---|---|---|---|
| `PUT /api/reading/:sub/:remoteId/read` | `{version: integer ≥ 1, readAt?}` | `200 {ok: true, stored: bool, readVersion: n \| null}`. Stores `max(existing, version)`. For an unknown subscription or item, or a read older than the row's last clear: `stored: false`, nothing written. | `400` bad body, `401`, `403` |
| `POST /api/reading/read` | `{items: [{sub, remoteId, version, readAt?}]}`, at most 500 | `200 {ok: true, received: n}`. The same max-merge per row. Unknown or stale rows are skipped. | `400` for any malformed entry or more than 500 entries (nothing is written), `401`, `403` |
| `DELETE /api/reading/:sub/:remoteId/read` | none | `200 {ok: true, stored: false, readVersion: null}`. Clears the row and records when (the server's time). Idempotent. | `401`, `403` |
| `POST /api/reading/unread` | `{items: [{sub, remoteId}]}`, at most 500 | `200 {ok: true, received: n}`, all or nothing. | `400`, `401`, `403` |

`readAt` (ISO 8601 UTC, e.g. `2026-10-06T12:00:00Z`) is when the read was made on this
Mac: a read earlier than the row's last clear is ignored, so a queued read can't undo a
later "mark unread" from another device.

Unknown rows are acknowledged, never `404`, because the app reads a `404` from these
endpoints as "this server doesn't keep read state".

**What the app does:**

- Marking posts read or unread is instant and local: one post, a selection (⌘-click,
  ⇧-click, ⌘A in the Reader's list; r / u, or the row's menu), or the open post. It also
  queues one op per reading row (duplicates of the same post, too) in the outbox, so
  marks survive going offline and restarting. A row has at most one waiting op, and the
  last action wins: an unread replaces a waiting read and the other way round; repeated
  reads coalesce to the highest version. Ops queued by an older Burrow (snake_case
  bodies) are still read and sent in the new shape.
- Several waiting ops go as one `POST /api/reading/read` or `/unread` (500 per call), so
  marking a whole list is one write against the owner budget; a single mark is one
  `PUT` or `DELETE`.
- A `PUT` the server answers `stored: false` because of a later clear takes the server's
  state: the row is unread here too.
- A pull sets the local value to `max(local, server)`, except that a row marked unread
  here ignores server read versions at or below the one it was read at (its *floor*). A
  row this Mac had confirmed but the server now reports lower or unread was cleared on
  another device, and it becomes unread here; a row read here that the server hasn't
  got is queued again (with its `readAt`).
- The first time a database sees the capability, the app batch-uploads every read version
  it holds (500 per call) and records that in its `meta` table.
- A `403` on a write (a sign-in or token without `reading:state`) holds read state for
  the rest of the session: the ops stay queued (nothing is dropped), new marks keep
  queueing, and the rest of the outbox syncs as usual. The app says so once, with the
  server's reason: sign in with the browser again (or make a new token with
  `reading:state`). It doesn't retry until the next sign-in or launch, which sends what
  waited.

**Privacy.** Read state is owner-only. No public page, feed, `blyg.json`, item document
or export reads it. It holds only numbers (and the time of a clear) keyed by subscription
and item, never text. A change, including a clear, advances the `reading` revision of
`GET /api/changes`, so a mark on one device shows on another's next pull.

## Hoppers

Hoppers are the studio's collections of posts. Burrow uses these routes (all in
`crates/blyg-core/src/api/mod.rs`); every write needs the scope **`owner:manage`**,
which Burrow's sign-in already asks for.

| Call | Body | Success |
|---|---|---|
| `GET /api/hoppers` | `cursor?`, `limit` | `{items: [Hopper], page}`, created first |
| `GET /api/hoppers/{id}/memberships` | `cursor?`, `limit` | `{items: [{hopperId, subscriptionId, remoteId, addedAt, item}], page}` |
| `POST /api/hoppers` | `{name}` | `201 Hopper` |
| `PATCH /api/hoppers/{id}` | `{name?, public?, description?}` (description ≤ 280; empty clears it) | `Hopper` |
| `DELETE /api/hoppers/{id}` | | `{ok: true}` |
| `PUT /api/hoppers/{id}/items/{sub}/{remoteId}` | | `{ok: true}` |
| `DELETE /api/hoppers/{id}/items/{sub}/{remoteId}` | | `{ok: true}` |

`Hopper` is `{id, name, slug, public, slugFrozen, description, created}`. Your own posts
are members with the subscription `~own` and the item's id; the studio takes any of
your items there, and a public hopper's page shows only the published ones. A rename
recomputes the slug until the hopper is first made public, which freezes it. A public
hopper has a page at `{mount}/h/{slug}/`, listed under Collections on the homepage and
archive and never in the feed.

The list and each hopper's memberships are read on the first sync and whenever the
`hoppers` counter of `GET /api/changes` moves (one walk per hopper). Adding and
removing posts go through the outbox, so they wait while offline; a `404` drops the
change, and a `403` (a credential without `owner:manage`) holds the hopper changes
until the next launch or sign-in without blocking the rest. Making, renaming,
publishing and deleting a hopper are direct calls that need the blyg to answer.

## Other shapes worth knowing

- Items are created with `POST /api/items {mode: "blank" | "fork", …}`, which answers
  `201` with the item (its first `revision`). Edits are `PATCH /api/items/:id
  {contentMd?, kind?, responses?, highlight?, provenance?, expectedRevision}`; unknown
  fields are a `400`. A draft's kind changes in place until its first publish.
- Each TK scope's provenance (AI disclosure for text generated **in Burrow**) goes with
  the text, on `POST /api/items` and `PATCH /api/items/:id`. Generation on the blyg's own
  server (`POST /api/items/:id/generate {scope, expectedRevision}`) records its
  disclosure itself.
- Pins are `PUT /api/items/:id/versions/:version/pin {expectedRevision}`. An item's
  version history is `GET /api/items/:id/versions`.
- An item's `responses` is `"default" | "show" | "hide"`. The app works out whether a
  page shows responses from that and the settings' `showResponsesDefault`.
- Items carry no permalink. The app uses `{blyg-url}/f/{id}` (`t/` for threads);
  `blyg-url` includes any mount path.
- `GET /api/settings` carries many keys Burrow doesn't use (`customTheme*`, `font`,
  `feedOrder`, `frontPage`, `postsPath`, `links*`, `setup`, `defaults`, `feedOrders`, …).
  Burrow ignores them, and its `PATCH` sends only the keys it owns.
- Pause and resume are `PATCH /api/subscriptions/:id {paused}`. Settings, mention hiding
  and subscription edits are `PATCH` too.
- Mentions may have `relation: "source"` (generated text drew on your post).
- Contract 5.7.0 (studio 0.56), asked for only when the capabilities say 5.7 or later:
  `GET /api/contacts` (the owner's address book, `{handle, name, origin}`), publish's
  `unresolvedHandles` (it rewrites `@handle` to an h-card link), and `mentions=true` on
  `GET /api/reading`. The repo's OpenAPI fixture is still 5.0.0.
- `DELETE /api/media/:id` removes an upload, answering `{ok, outcome: "deleted" |
  "detached", revision?}`. It takes a JSON body: `{expectedRevision}` for an attachment
  (the post's revision), `{}` for a loose upload. Burrow remembers which post each
  attachment it made belongs to, so it can remove it at that post's revision.

## Degrading gracefully

- **A saved credential the blyg refuses (`401`):** sync stops (no retries), queued
  changes stay queued, and the sign-in sheet opens once, browser first.
- **Older than studio 0.49.12 (a contract before 5):** nothing syncs; see
  [How Burrow checks the contract](#how-burrow-checks-the-contract).
- **Change counters (`GET /api/changes`):** each sync first reads the counters and
  fetches only the collections whose domains moved, so the app syncs every 15 s. The
  reading list is walked again only when `reading` or `subscriptions` moved: a moved
  thumb reads `GET /api/signals`, a moved hopper reads the hoppers. As a safety net
  everything is read whole after 10 minutes; each such pass that finds nothing the
  counters hadn't said doubles the wait, up to 2 hours. See
  [Cloudflare's free D1 plan](#cloudflares-free-d1-plan).
- **The blyg fails (`5xx`):** its Worker or its database is down, or a `D1_ERROR` says
  the daily limit is spent. Changes stay queued; the status bar says "blyg
  unavailable" in grey, and Burrow waits 30 s, then twice as long each time, up to
  30 minutes, before trying again (opening the blyg again tries at once).
- **Work budgets:** a `429` keeps the change queued and retries after `Retry-After`; the
  app says "The blyg asked Burrow to slow down". Autosave pushes for one post are at
  least 3 s apart, about 20 writes a minute at most.
- **A list that keeps changing while it's read** (`cursor-expired` three times running):
  left as held for this pull, read again on the next.

## Cloudflare's free D1 plan

A studio on Cloudflare's free plan gets 5 million D1 row reads a day. Once they're
spent, every route that touches the database answers `500` until 00:00 UTC. What a
sync costs depends on the studio as much as on Burrow:

- **Update the studio.** Before 0.51 each page of the reading list
  (`GET /api/reading/imported`) scanned the whole `imported_items` table, about 5,000
  row reads a page on a blyg with a few thousand imported posts
  ([blygger-studio#113](https://github.com/blygger/blygger-studio/issues/113)). 0.49.12
  added the `observedAt` index, 0.51 stopped the page query from materializing the whole
  list, and 0.52 indexed the per-row hopper lookup. On 0.52 or later a page reads
  roughly the rows it returns.
- **Burrow 0.21 reads less.** A moved thumb or hopper no longer walks the reading list
  (issue #71), the 10-minute safety net backs off to 2 hours while it finds nothing, and
  a `5xx` backs off to 30 minutes instead of 2.
- **Set `sync-cadence = low`** in the [config file](config.md) for a blyg with a large
  reading list on the free plan: change checks every minute instead of every 15 s, the
  reading list at most every 10 minutes when only it moved (new imports, read marks),
  and the safety net hourly at first, up to every 6 hours.

The studio has no "changed since" read of the reading list yet: when `reading` moves
(an import, a read mark from any device), Burrow walks the whole list again.

## Updating an older server

Follow upstream's
[Releases and upgrading](https://github.com/blygger/blygger-studio#releases-and-upgrading):
read the changelog entries between your version and 0.49.12 (or later), each with its
**Migrations:** line. Coming from 0.8 or earlier, start with
[Upgrading to Blygger Studio 0.11](https://github.com/blygger/blygger-studio/blob/main/docs/upgrading-to-0.11.md),
which covers source and Worker-archive installs, the database migration, and
path-mounted blygs. A Worker fork that carried the old extensions moves to a stock
studio; Burrow needs none of them. Once the blyg is updated, Burrow notices on its next
sync (or at once on a manual sync) and sends what waited; there's nothing else to do in
the app.

## Optional: server-side generation with Gemma 4

A Worker with a Workers AI binding (`"ai": {"binding": "AI"}`) can run TK generation on
`@cf/google/gemma-4-26b-a4b-it` with no API key. The app's "Generate on my blyg server" provider
uses whatever the Worker is configured to use.

## Status

Upstream's plan (its `docs/migration.md` §3) was for this app to run on the documented
API alone, with OAuth in place of the fork's owner token. As of contract 5 it does,
entirely: Burrow 0.16 speaks nothing else.
