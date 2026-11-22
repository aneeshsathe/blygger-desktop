section: Development

- **Store: a served subscription list can't replace the table without the
  prune decision.** `Store::take_served_subscriptions(list, prune)` is the
  one way in for a list the blyg served; the wipe-and-rewrite beneath it is
  private, and a subscription made here goes through
  `Store::add_subscription`. A scan (`tests/it/list_prune_scan.rs`, with
  `tests/fixtures/list-prune-allow.txt`) fails when a function that deletes
  rows of a list table neither names `api::Prune` nor is private nor is on
  the reviewed list.
