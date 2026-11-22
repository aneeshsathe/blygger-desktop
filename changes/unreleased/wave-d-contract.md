section: Changed

- **Sync: a blyg on a newer studio keeps syncing.** When a blyg's studio moves to
  an API contract newer than this Burrow knows (or one that isn't a number),
  Burrow no longer stops syncing until it's updated. It syncs what the blyg's
  capabilities offer, sends your posts and changes as usual, and says once a
  session "Your blyg runs a newer studio". Until Burrow is updated it removes
  nothing on this Mac on that studio's word: items, reading posts,
  subscriptions, hoppers, thumbs, read marks, contacts and the shelf it doesn't
  list stay here (`api::Contract::Newer`, `api::Prune`, `sync/newer.rs`).
  Connecting to such a blyg works too.
