section: Changed

- **Sync: a blyg on a newer studio keeps syncing.** When a blyg's studio moves to
  an API contract newer than this Burrow knows (or one that isn't a number),
  Burrow no longer stops syncing until it's updated. It syncs what the blyg's
  capabilities offer, sends your posts and changes as usual, and says once a
  session "Your blyg runs a newer studio". Until Burrow is updated it removes
  nothing on this Mac on that studio's word: items, reading posts,
  subscriptions, hoppers, thumbs, read marks, contacts and shelf entries it
  doesn't list stay here (`api::Contract::Newer`, `api::Prune`,
  `sync/newer.rs`). The shelf is read but not written to such a blyg: a change
  to it waits until Burrow is updated. Connecting to such a blyg works too.
- **Features follow what the blyg lists, not its version number.** Contacts,
  changelog summaries and sent-mention groups are offered when the blyg's
  capabilities list their operations (`listContacts`, `summarizeChangelog`,
  `listMentionGroups` and `getMentionSummary`), so a newer or unnumbered
  contract gets them too (`api::offers`). Following a whole blyg
  (`wholeBlyg`) is sent to every studio but the oldest, and asked again
  without it if refused.
