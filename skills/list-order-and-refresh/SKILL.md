---
name: list-order-and-refresh
description: Get list ordering and live updates right in a Blygger client. Use when a client shows reading or own-post lists, when posts appear in the wrong order or don't update, or when adding polling, caching or pagination.
---

# Order and refresh: the two bugs every client ships once

A web client once showed Reading in effectively random order and never updated
while its tab was open. Both bugs were invisible in unit tests.

## Order

- **The backend's order is not an order.** The studio's imported-reading
  endpoint orders by `observed_at`, which is identical for every post in a
  backfill. Re-sort on the client, always.
- **One function, shared.** Put the comparator in one module that every client
  surface (store, native UI, web) calls. A second copy will drift.
- **Compare instants, not strings.** `…Z` and `+00:00`, with and without
  milliseconds, all appear. Parse RFC 3339 first.
- **Always tie-break** (e.g. source then id, descending). Second-granularity
  timestamps tie constantly after quick actions.
- **Check the spec's order fixtures** (`conformance/feeds/`, the pace-layered
  order) and say in your docs which rule you follow and where it differs.
- **First page in final order.** When paging in, the first screen must already
  be ordered; later pages may only insert newer rows above, never reshuffle.

## Refresh

- **Poll while visible.** A cheap `GET /api/changes` every ~15 s (the studio's
  pace) while the view is visible and online; full re-read only for lists whose
  change counters moved. Stop while hidden; check at once on becoming visible.
- **`/changes` is going away.** Studio 0.65 deprecates `GET /api/changes` and
  0.67 removes it; Burrow then falls back to conditional reads (ETag /
  `If-None-Match`, 304) of each list, every 60 s.
- **Refresh right after your own writes** (publish, restore, withdraw, delete).
- **Per-list dependencies.** Hoppers and thumbs/signals have their own counters;
  don't hang them off the reading counter.
- **Single-flight.** A refresh requested during a refresh queues exactly one
  follow-up; each run records counters only after its own successful read.
- **Back off.** Honour `Retry-After` on 429; double from ~30 s to ~30 min on
  5xx; reset on success. A safety-net full re-read (~10 min) should double
  while it finds nothing, up to ~2 h. Server reads cost the owner money.

## Test it

A live check against a real studio (`real-server-testing`): make each kind of
change through the API and assert it reaches the screen within ~20 s with no
interaction, and that an idle client stays at a few requests a minute.

## In Burrow

- The one order is `blyg-core/src/order.rs`; the store and `blyg-app`
  both call it. Change the rule there, with its tests, and nowhere
  else.
- Native sync's dependency map (`ITEMS_DEPS`, `READING_DEPS`, `HOPPERS_DEPS`,
  `SIGNALS_DEPS`) and the safety-net backoff (`next_full_every`) live in
  `blyg-core/src/sync/mod.rs`.
- **A list longer than one pull's page budget.** Burrow 0.25.1 re-walked the
  newest 2,000 reading rows every ~40 s, forever, on a blyg with 5,000: a walk
  that never reached its end was never accepted, and never pruned. A moved
  counter reads the head (`sync/walk.rs`), whole walks go on from their cursor
  across pulls, and a counter that moves for bookkeeping (`subscriptions` on
  every feed poll) never walks a list on its own. Test with more rows than the
  budget (`a_reading_list_past_the_page_budget_is_read_by_its_head`).
