# Server requirements

Burrow talks to a blyg's **owner API**. Since studio 0.9, the upstream reference Worker
(https://github.com/blygger/blygger-studio) documents that API as an OpenAPI contract
(`openapi.json`; route guide in its `docs/api.md`). The app follows that contract and
needs **0.9 or later**; it is tested end to end against **studio 0.46.1**. Every route
the app shares with upstream is checked against `openapi.json` in its tests
(`crates/blyg-core/tests/fixtures/`).

**A stock blygger-studio is fully supported.** On studio 0.40 or later nothing in this
app needs a patched server: writing, publishing, versions, pins, subscriptions, mentions,
settings, the reading list, read state across Macs, lineage and AI disclosure all run on
upstream's own routes.

## Signing in

- **Sign in with the browser** (studio 0.28+, the default): Burrow registers itself as
  an OAuth client of your blyg, opens the studio's consent page, and keeps the grant (a
  one-hour access token, renewed with a rotating refresh token until the grant ends, at
  most 30 days after your studio sign-in) in your Keychain. Signing out revokes it.
- Or make an **API token** in Studio → More → Client access (REST API, every permission: read, draft,
  publish, manage, and on studio 0.39+ `reading:state`; it lasts 30 days) and paste it.
  A token missing a permission gets a 403 that names it.
- On an older studio, or as a fallback, the **studio password**: Burrow signs in at
  `{blyg-url}/studio/login` like the browser does, keeps the session cookie in memory
  only (the password is in your Keychain), and signs in again when the session ends.

A stock studio has no owner token in its environment: that was the old Worker fork's
extension 1 (`BLYG_OWNER_TOKEN`). When a blyg stops taking a saved credential (a revoked
or expired token, an ended sign-in, or such an owner token after the blyg moved to a
stock studio), every call answers `401`. Burrow then stops syncing (no retries), keeps
every queued change, and opens the sign-in sheet once, browser first. Signing in again
sends what waited.

## What each studio version brings

| Feature | Stock studio |
|---|---|
| Reading list | **0.40+:** `GET /api/reading/imported` (`listImportedReading`): every imported post with its Markdown, author, thumb, hoppers, read state, transclusions and lineage, keyset-paged by an opaque `cursor`, 100 a page. **0.9–0.39:** built from `GET /api/reading` (which posts there are) plus `GET /api/imports/{sub}/{id}` for each new or changed one, with thumbs and hoppers from `/api/signals` and each hopper; slower. |
| Read state across Macs | **0.39+** (`reading:state` scope), unread too; see [Read-state sync](#read-state-sync). Older: stays on each Mac. |
| Who a post replies to, or forks | **0.18+** sends it with each post (on 0.40+ with its `cited`). Older: Burrow fetches it from the author's blyg, a few posts per sync. |
| AI disclosure for text generated **in Burrow** | **0.28+:** each TK scope's provenance goes with the text (`PATCH /api/items/:id {content_md, provenance}`, and on `POST /api/items`); on 0.41+ a remote source carries its `origin`. Older: before publishing such text Burrow says it will go out without the disclosure, and lets you cancel. Generation on the blyg's own server records its disclosure itself. |
| Removing an image you pasted, then deleted | **0.26+.** Older: it stays on the server. |
| Syncing every 15 s on change counters | **0.32+** (`GET /api/changes`). Older: read whole every 60 s. |

On a studio older than 0.40 Burrow says once that the server is limited, and lists the
limits in the About window.

### Shapes worth knowing (studio 0.40+)

- `GET /api/reading/imported?limit=1..100&cursor=…` → `{items, next, limit}`. Rows are
  camelCase (`subscriptionId`, `remoteId`, `withdrawn`, `l0`, `version`, `readVersion`,
  `observedAt`, `contentMd`, `contentHtml` (sanitized), `author` `{name, url}`, `page`,
  `sourceUrl`, `thumb`, `hoppers`, `transclusions`, `stubOf`, `forkedFrom`, `generated`),
  newest `observedAt` first, with no drops or repeats across pages. A `limit` out of range
  or a cursor the server didn't issue is `400`; `before` is not accepted. There are no
  page-level flags: read state is advertised on `GET /api/reading` (`read_state`,
  `read_state_clear`), and each row's lineage is the origin's own (null when none).
- `GET /api/settings` carries many keys Burrow doesn't use (`custom_theme*`, `font`,
  `feed_order`, `front_page`, `posts_path`, `links_*`, `setup`, `defaults`,
  `feed_orders`, …). Burrow ignores them, and its `PATCH` sends only the keys it owns.
- Mentions may have `relation: "source"` (0.41: generated text drew on your post).
- Hopper routes take `~own` as the subscription with your own item's id (0.45).

## Legacy: the old Worker fork's extensions

Before upstream had all of the above, the maintainer's Worker fork carried extensions.
Burrow still speaks them where it is cheap, so a fork keeps working, but **a stock studio
needs none of them**.

| # | Extension | Now |
|---|---|---|
| 1 | Bearer-token owner auth with the Worker secret `BLYG_OWNER_TOKEN`. | Superseded by studio 0.28's OAuth and manual tokens. Such a token still goes in the "API token" field on a fork; a stock studio refuses it (`401`, see above). |
| 2 | Owner JSON reads. | Upstream since 0.9; retired. |
| 3 | `GET /api/reading/imported?limit&before=<cursor>` → `{items, next, read_state, read_state_clear, lineage}` with snake_case rows, at most 500 a page. | Upstream since 0.40 under the same path with its own shape (above). Burrow tells them apart on the walk's first page (which sends only `limit=100`, taken by both): a page with the fork's `read_state` flag is the fork's, and later pages use `before`; otherwise `cursor`. |
| 4 | `PUT /api/items/:id/tk-provenance`. | Retired by studio 0.28's `provenance` on `PATCH /api/items/:id`; used only when a server refuses that field. |
| 5 | Read-state sync. | Upstream since 0.39 (below), same write routes and bodies. |

Upstream's scoped tokens may not reach a fork's own extension routes.

## Read-state sync

Upstream since studio 0.39, read and unread (clearing read state leaves a tombstone
against stale reads); before that it was the fork's extension 5, with the same write
routes and bodies. The server keeps, per reading row, the highest version
the owner has read. It never lowers that value on a read, so every read write is
idempotent and replay-safe; only a clear resets it.

**Capability.**

- Upstream: `GET /api/reading` answers with top-level `read_state: true` (and
  `read_state_clear: true`), and each imported entry carries `readVersion` (an integer,
  or `null` when unread) and `version`, the version held. On 0.40+ the same `readVersion`
  is on every `GET /api/reading/imported` row; Burrow reads the flags from
  `GET /api/reading?limit=1` once a session.
- The fork: `GET /api/reading/imported` answers `{items, next, read_state: true}`, and
  each item carries `read_version`.

The app reads the read version only when `read_state` is `true`.

**Writes** (owner auth, like every `/api` call). **Scope:** these four writes
(`markRead`, `markUnread`, `markReadBatch`, `markUnreadBatch`) need the bearer scope
**`reading:state`**; `owner:manage` alone is refused with `403 insufficient_scope`. The
app spells the name in one place, `READ_STATE_SCOPE` in
`crates/blyg-core/src/api/oauth.rs`. The browser sign-in asks for it when the studio
lists it in `scopes_supported` (an older studio would refuse the whole request for a
scope it doesn't know). A client registered before (Burrow 0.10) is reused; the studio
grants it the new scope.

| Call | Body | Success | Errors |
|---|---|---|---|
| `PUT /api/reading/:sub/:remoteId/read` | `{version: integer ≥ 1, read_at?}` | `200 {ok: true, stored: bool, read_version: n \| null}`. Stores `max(existing, version)`. For an unknown subscription or item, or a read older than the row's last clear: `stored: false`, nothing written. | `400` bad body, `401`, `403` |
| `POST /api/reading/read` | `{items: [{sub, remote_id, version, read_at?}]}`, at most 500 | `200 {ok: true, received: n}`. The same max-merge per row. Unknown or stale rows are skipped. | `400` for any malformed entry or more than 500 entries (nothing is written), `401`, `403` |
| `DELETE /api/reading/:sub/:remoteId/read` | none | `200 {ok: true, stored: false, read_version: null}`. Clears the row and records `unread_at` (the server's time). Idempotent. | `401`, `403` |
| `POST /api/reading/unread` | `{items: [{sub, remote_id}]}`, at most 500 | `200 {ok: true, received: n}`, all or nothing. | `400`, `401`, `403` |

`read_at` (ISO-8601 with an offset, e.g. `2026-10-06T12:00:00Z`) is sent only to a
server that advertises `read_state_clear`: a read earlier than the row's `unread_at` is
ignored, so a queued read can't undo a later "mark unread" from another device. Upstream
bodies are strict, so the app **never** sends `read_at` to a server that doesn't
advertise the flag. Upstream's `400` body is `{error, issues: [{path, message}]}`.

Unknown rows are acknowledged, never `404`, because the app reads a `404` from these
endpoints as "this server doesn't keep read state".

**What the app does:**

- Marking posts read or unread is instant and local: one post, a selection (⌘-click,
  ⇧-click, ⌘A in the Reader's list; r / u, or the row's menu), or the open post. On a
  server with the capability it also queues one op per reading row (duplicates of the
  same post, too) in the outbox, so marks survive going offline and restarting. A row has
  at most one waiting op, and the last action wins: an unread replaces a waiting read and
  the other way round; repeated reads coalesce to the highest version.
- Several waiting ops go as one `POST /api/reading/read` or `/unread` (500 per call), so
  marking a whole list is one write against the owner budget; a single mark is one
  `PUT` or `DELETE`.
- A read carries `read_at`: when it was marked on this Mac, kept with the row until the
  server confirms it (reads from before this was tracked count as the epoch, so any clear
  wins over them). A `PUT` the server answers `stored: false` because of a later clear
  takes the server's state: the row is unread here too.
- A pull sets the local value to `max(local, server)`, except that a row marked unread
  here ignores server read versions at or below the one it was read at (its *floor*). On
  a server with `read_state_clear`, a row this Mac had confirmed but the server now
  reports lower or unread was cleared on another device, and it becomes unread here; a
  row read here that the server hasn't got is queued again (with its `read_at`).
- **Unread without `read_state_clear`** (the fork's extension 5) stays on
  this Mac: no unread op is sent, and the floor keeps the server's old read version from
  re-marking the row read on every pull. Reading the post again ends it. If the server
  later gains `read_state_clear`, those local unreads are sent then.
- The first time a database sees the capability, the app batch-uploads every read version
  it holds (500 per call) and records that in its `meta` table.
- A `403` on a write (a sign-in or token without `reading:state`, such as one made before
  studio 0.39) holds read state for the rest of the session: the ops stay queued (nothing
  is dropped), new marks keep queueing, and the rest of the outbox syncs as usual. The app
  says so once, with the server's reason: sign in with the browser again (or make a new
  token with `reading:state`). It doesn't retry until the next sign-in or launch, which
  sends what waited.
- **Without the capability**, read state stays on this Mac. The app makes no requests to
  these endpoints and shows no errors. A `404` from them turns sync off and drops anything
  queued.

**Privacy.** Read state is owner-only. No public page, feed, `blyg.json`, item document
or export reads it. It holds only numbers (and the time of a clear) keyed by subscription
and item, never text. A change, including a clear, advances the `reading` revision of
`GET /api/changes`, so a mark on one device shows on another's next pull.

## Also used when present

- `POST /api/media` answering `200 {…, duplicate: true}` for identical bytes on
  the same item. Not upstream (upstream issue #7).
- `DELETE /api/media/:id` (removes an upload) is upstream since studio 0.16,
  answering `{ok, outcome: "deleted" | "detached"}`. On an older server it 404s, and
  an abandoned paste leaves its file there.

## What the app uses from upstream

These follow `openapi.json` exactly. The differences from the pre-0.9 API that
mattered here:

- Collections answer `{items, total, offset, limit}`; the app pages with
  `offset`/`limit` (100 a page).
- Items are created with `POST /api/items {mode: "blank" | "fork", …}`, which
  answers `201` with the item. `POST /api/fork` is gone.
- Edits are `PATCH /api/items/:id {content_md?, kind?, responses?}`. A draft's kind
  changes in place until its first publish, so changing it no longer makes a new
  draft. Unknown fields are a 400.
- Pins are `PUT /api/items/:id/versions/:version/pin`, with no body.
- An item's `responses` is `"default" | "show" | "hide"`. The app works out whether a
  page shows responses from that and settings' `show_responses_default`.
- Items carry no permalink. The app uses `{blyg-url}/f/{id}` (`t/` for threads);
  `blyg-url` includes any mount path.
- Pause and resume are `PATCH /api/subscriptions/:id {paused}`. Settings, mention
  hiding and subscription edits are `PATCH` too.
- Errors are `{error, errors?, issues?}`. The app shows `issues` (validation
  failures) as details.

## Degrading gracefully

- **A saved credential the blyg refuses (`401`):** sync stops (no retries), queued
  changes stay queued, and the sign-in sheet opens once, browser first.
- **A server older than studio 0.9:** the connect sheet says it needs updating. A
  blyg already connected gets a notice when Burrow starts (and a status-bar
  notice with a **How to update** link), and Burrow pushes nothing to it: on an
  old server a new route's 404 would read as "deleted" and duplicate your posts.
  Your edits wait in the outbox and go up once the server is updated. Burrow
  recognises an old server by `GET /api/items` answering without `total` (or
  404).
- **Studio older than 0.40:** the reading list is built from upstream's older routes
  (see above).
- **Older than studio 0.28:** before publishing text that was generated
  in the app, the app warns that it will go out **without** AI disclosure, and lets you cancel.
- **Studio 0.32+ (`GET /api/changes`):** each sync first reads the change counters and
  fetches only the collections whose domains moved (everything, every 10 minutes), so the
  app syncs every 15 s instead of every 60 s. An older server is read whole every 60 s.
- **Work budgets (studio 0.28+):** a `429` keeps the change queued and retries after
  `Retry-After`; the app says "The blyg asked Burrow to slow down". Autosave pushes for
  one post are at least 3 s apart, about 20 writes a minute at most.
- **Older than studio 0.39:** read state stays on each Mac.

## Updating an older server

Follow upstream's guide,
[Upgrading to Blygger Studio 0.11](https://github.com/blygger/blygger-studio/blob/main/docs/upgrading-to-0.11.md).
It covers source and Worker-archive installs, the one database migration, and
path-mounted blygs. A fork that carries the legacy extensions doesn't need
them on studio 0.40 or later: a stock studio is fully supported. Once it's updated, Burrow picks it up on its next sync; there's
nothing to do in the app.

## Optional: server-side generation with Gemma 4

A Worker with a Workers AI binding (`"ai": {"binding": "AI"}`) can run TK generation on
`@cf/google/gemma-4-26b-a4b-it` with no API key. The app's "Generate on my blyg server" provider
uses whatever the Worker is configured to use.

## Status

Upstream's plan (its `docs/migration.md` §3) was for this app to run on the documented
API alone, with OAuth in place of extension 1. As of studio 0.40 it does: reading rows
(#11, #61), lineage in them (#12) and read state are upstream. Only the media
`duplicate` reply (#7) is still a fork's.
