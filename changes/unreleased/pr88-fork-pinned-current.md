section: Fixed

- **Reading: a post whose current version is pinned can be forked.** The
  version pill shows such a version once, as the current one, so Fork stayed
  greyed out and its tooltip pointed to a pin there was no way to reach (or
  said the post had none). Fork is now live there: ⋯ More lists it as **Fork
  this pin**, and the pill reads `📌 vN · current`
  (`ActionCtx::current_pinned` in `crates/blyg-app/src/reading/vm.rs`).
  Thanks to @patwater
  ([#88](https://github.com/aneeshsathe/blygger-desktop/pull/88)).
