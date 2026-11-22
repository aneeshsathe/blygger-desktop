section: Fixed

- **Sync: a blyg on blygger-studio 0.67 or later syncs again.** Studio 0.67
  moved to private API contract 6, whose only change Burrow meets is that
  `GET /api/changes` is gone, which sync has done without since 0.27.0.
  Burrow 0.27.2 and earlier refused any contract but 5 and stopped syncing
  with "doesn't speak yet". `crates/blyg-core/src/api/mod.rs`
