section: Development

- **Checks: a covered contract item says where and by what.**
  `blyg-core/tests/it/contract_coverage.rs` now keeps each test file's path
  and reports, for an item a test covers, the file, line and anchor that
  counted (`crates/…/file.rs:522 ("html" near sibling "…")`). The failure for a
  stale `[[exempt]]` or a covered `[[pending]]` prints it and says to report a
  line that is about something else as a false cover instead of deleting the
  entry: twice an honest exemption was nearly deleted because the message only
  said "stale".
