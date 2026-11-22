section: Development

- **Check: every e2e subscription is dropped by a guard that names it.**
  `blyg-core/tests/it/e2e/subscribe_guard.rs` now reads the e2e sources
  recursively, skips comments and strings, and requires each `.subscribe(`
  to be bound with `let NAME = …;` and followed, within three lines of the
  statement's end, by an `Unsub(…)` or `ApiUnsub(…)` that names `NAME`. A
  guard for a different subscription, or a subscribe whose result isn't
  bound, fails with the fix spelled out. Self-tests run the scan on the
  unguarded site from the studio 0.68 run and on guarded, multi-line and
  mismatched forms.
