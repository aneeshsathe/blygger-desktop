section: Fixed

- **Sync: a list that keeps changing no longer fails the sync.** While a
  new subscription's posts are being imported, a studio changes its reading
  list with every post, so a long read of it (the Background lens, the
  hoppers, the signals) could be cut off every time it was tried, and the
  whole sync failed with an error. Burrow now keeps what it holds of that
  list and reads it again on the next sync; nothing is removed. A list
  cut off three syncs in a row waits a few syncs before the next try
  (doubling, up to 32; Sync now still reads it), and Burrow says once
  what that means for the list: for Background, newly imported posts may
  show in the Feed until it settles (`crates/blyg-core/src/sync/churn.rs`).
