---
name: capability-gating
description: Decide whether a Blygger client may use a server feature. Use whenever client code calls an owner-API endpoint that not every studio has, checks contractVersion, or shows/hides UI by server support.
---

# Gate on what the blyg says it does, not on its version

Contract numbers have been reused: a studio release once claimed contract
5.9.0, the number two unmerged features also claimed, without shipping either.
So a version check can say "yes" for a feature that isn't there.

## Rules

1. **Read `GET /api/capabilities`** once per session (and again after a
   failed call that suggests the server changed). Gate each feature on the
   operation names it lists (e.g. `markReadBatch`, `listStaleThreads`,
   `refreshItem`, `getShelf`/`putShelf`).
2. **Some endpoints aren't advertised** (e.g. a session-only route). For those,
   ask the endpoint and treat **404 as "not offered here"**, silently; remember
   it for a few hours, not forever.
3. **Use contractVersion only as a floor** for the whole client ("speaks 5.x"),
   never as a per-feature switch.
4. **Hide, don't break.** An unoffered feature's UI is hidden or explains
   itself; it never throws or shows a raw error.
5. **Writes are partial.** `PATCH` only the keys you changed (settings above
   all). A client that writes a whole document back deletes fields added by
   newer servers (it has nearly happened with `Settings.discoverSeenAt`).

## Check

- grep the client for `contractVersion` / `contract_version` comparisons that
  guard a single feature; replace them with capability checks.
- grep for full-document `PUT`s of settings or other server documents.

## In Burrow

Gate in `blyg-core` where the call is made, and let the UI hide what isn't
offered. A remembered "not offered" answer lives in the store's meta (as the
lineage-glyph extension's 404 does), not in `<data dir>/state.json`.
