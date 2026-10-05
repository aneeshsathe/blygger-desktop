# Server requirements

Burrow talks to a blyg's **owner API**. Since studio 0.9, the upstream reference Worker
(https://github.com/blygger/blygger-studio) documents that API as an OpenAPI contract
(`openapi.json`; route guide in its `docs/api.md`). The app follows that contract (studio
0.26 is current) and needs **0.9 or later**. Every route the app shares with upstream is
checked against `openapi.json` in its tests (`crates/blyg-core/tests/fixtures/`).

**A stock blygger-studio works.** Sign in with the studio password: Burrow signs in at
`{blyg-url}/studio/login` like the browser does, keeps the session cookie in memory
only (the password is in your Keychain), and signs in again when the 30-day session
ends. Writing, publishing, versions, pins, subscriptions, mentions, settings and the
reading list all work. The reading list is built from upstream's own routes:
`GET /api/reading` lists the posts, `GET /api/imports/{sub}/{id}` reads each new or
changed one, and thumbs and hoppers come from `/api/signals` and each hopper.

### What a stock server limits

| Feature | On a stock server |
|---|---|
| AI disclosure for text generated **in Burrow** | Not recorded. Before publishing such text, Burrow says it will go out without the disclosure, and lets you cancel. Generation on the blyg's own server records its disclosure itself. |
| Read state across Macs | Stays on each Mac. |
| Removing an image you pasted, then deleted | It stays on the server. |
| Who a post replies to, or forks | Studio 0.18 or later sends it with each post. An older one doesn't, so Burrow fetches it from the author's blyg, a few posts per sync. |

Burrow says this once, the first time it meets a stock server, and lists it in the
About window.

### Extensions for everything

A Worker that carries these extensions gets all of the above:

| # | Extension | Why the app needs it |
|---|---|---|
| 1 | *Optional.* **Bearer-token owner auth**: `Authorization: Bearer <token>` is accepted wherever the owner session cookie is, when the Worker secret `BLYG_OWNER_TOKEN` is set. | Sign in with a token instead of the password. Upstream is cookie-only; its OAuth plan replaces both. |
| 3 | **Reading rows for the app** (without it, Burrow builds the same rows from upstream's routes, more slowly): `GET /api/reading/imported?limit&before=<cursor>` → `{items, next, read_state}`. Imported items only (tombstones too), newest `observed_at` first, in the app's row shape: raw `content_md`, `origin`, `author`, `page`, `thumb`, `hoppers`, `read_version`, `pinned_version_retained` and `transclusions[]` with `cited`. `limit` defaults to 100, max 500; `next` is an opaque cursor passed back as `before`; a bad cursor is `400`. `content_html` is raw, as stored, so the app sanitizes it before display. | Upstream's `/api/reading` is a different resource: own and imported posts, rendered and sanitized, offset-paged (≤ 50), with no Markdown, author, thumb or hoppers. The reader can't use it as-is. |
| 4 | **Client-recorded TK provenance**: `PUT /api/items/:id/tk-provenance {content_md?, scopes:[{index, model, sources?, at?} \| null]}` and `GET` of the same. Validated first, atomic with the text, never stores the instruction. | So text generated **in the app** is disclosed (`generated` + `blyg-tk-gen`) exactly like text the Worker generates itself. Upstream's item has a read-only `provenance`; nothing can write it. |
| 5 | *Optional.* **Read-state sync**: `read_state: true` and a per-item `read_version` on `GET /api/reading/imported`; `PUT /api/reading/:sub/:remoteId/read` and `POST /api/reading/read`. See [Extension 5](#extension-5-read-state-sync). | So a post you read on one Mac reads as read on your others, and a fresh install doesn't show everything unread. |

Extension 2 (owner JSON reads) is upstream now, so its number is retired.
Field-level contracts: `docs/SPEC.md` § API and § Client-recorded provenance.

## Extension 5: read-state sync

The server keeps, per reading row, the highest version the owner has read. It never
lowers that value, so every write is idempotent and replay-safe.

**Migration.** One new table (in the reference Worker: `migrations/0012_read_state.sql`):

```sql
CREATE TABLE read_state (
  subscription_id TEXT NOT NULL,
  remote_id       TEXT NOT NULL,
  read_version    INTEGER NOT NULL,
  updated         TEXT NOT NULL,
  PRIMARY KEY (subscription_id, remote_id)
);
```

It also adds two triggers, so a row goes away when its imported item or its subscription
is deleted.

**Capability.** `GET /api/reading/imported` answers `{items, next, read_state: true}`, and each item
carries `read_version` (an integer, or `null` when unread). The app reads `read_version`
only when `read_state` is `true`. It checks the flag on every pull and remembers the
last answer.

**Writes** (owner bearer auth, like every `/api` call):

| Call | Body | Success | Errors |
|---|---|---|---|
| `PUT /api/reading/:sub/:remoteId/read` | `{version: integer ≥ 0}` | `200 {ok: true, stored: bool, read_version: n \| null}`. Stores `max(existing, version)`. For an unknown subscription or item: `stored: false`, nothing written. | `400` bad body, `401` |
| `POST /api/reading/read` | `{items: [{sub, remote_id, version}]}`, at most 500 | `200 {ok: true, received: n}`. The same max-merge per row. Unknown rows are skipped. | `400 {error, errors?: [{index, reason}]}` for any malformed entry or more than 500 entries (nothing is written); `401` |

Unknown rows are acknowledged, never `404`, because the app reads a `404` from these
endpoints as "this server doesn't have extension 5".

**What the app does:**

- Marking a post read stays instant and local. On a server with the capability, it also
  queues one `read` op per reading row it marked (duplicates of the same post, too) in the
  outbox. The outbox sends them like any other change, so they survive going offline and
  restarting. Repeated reads of one row coalesce to its highest version.
- A pull sets the local value to `max(local, server)`. It never lowers it.
- The first time a database sees the capability, the app batch-uploads every read version
  it holds (500 per call) and records that in its `meta` table. After that, a row held
  locally that is ahead of the server is queued again on the next pull.
- **Without extension 5**, or without extension 3, read state stays on this Mac. The app
  makes no requests to these endpoints and shows no errors. A `404` from them turns sync off
  and drops anything queued.

**Privacy.** The table is owner-only. No public page, feed, `blyg.json`, item document or
export reads it. It holds only numbers keyed by subscription and item, never text.

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

- **No extension 1:** sign in with the studio password instead.
- **A server older than studio 0.9:** the connect sheet says it needs updating. A
  blyg already connected gets a notice when Burrow starts (and a status-bar
  notice with a **How to update** link), and Burrow pushes nothing to it: on an
  old server a new route's 404 would read as "deleted" and duplicate your posts.
  Your edits wait in the outbox and go up once the server is updated. Burrow
  recognises an old server by `GET /api/items` answering without `total` (or
  404).
- **No extension 3:** the reading list comes from upstream's own routes (see above).
- **No extension 4:** before publishing text that was generated in the app, the app warns that
  it will go out **without** AI disclosure, and lets you cancel.
- **No extension 5:** read state stays on each Mac, as before.

## Updating an older server

Follow upstream's guide,
[Upgrading to Blygger Studio 0.11](https://github.com/blygger/blygger-studio/blob/main/docs/upgrading-to-0.11.md).
It covers source and Worker-archive installs, the one database migration, and
path-mounted blygs. A server that carries the extensions above needs them
re-applied on top of the new version, so update it from wherever its extensions
come from. Once it's updated, Burrow picks it up on its next sync; there's
nothing to do in the app.

## Optional: server-side generation with Gemma 4

A Worker with a Workers AI binding (`"ai": {"binding": "AI"}`) can run TK generation on
`@cf/google/gemma-4-26b-a4b-it` with no API key. The app's "Generate on my blyg server" provider
uses whatever the Worker is configured to use.

## Status

Upstream's plan (its `docs/migration.md` §3) is for this app to run on the documented
API alone, with OAuth in place of extension 1. The remaining extensions are proposed in
upstream issues #11 (provenance, read state, reading rows), #12 (lineage in reading
rows) and #7 (attachments).
