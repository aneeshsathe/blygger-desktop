section: Development

- **Checks: a contract property's anchor must be a real one.**
  `blyg-core/tests/it/contract_coverage.rs` let a plain-word property count
  as covered next to its schema's name matched as a substring, so a test about
  `AgenticEditionInput` anchored `AgenticEdition`'s properties; the owner now
  has to appear as a whole identifier. It also let common words anchor each
  other as siblings (`origin`, `version`, `source`, `label`, `partial` sit
  together in unrelated reading and lineage rows), so `CatchUp.cards[].id`
  looked tested by `crates/blyg-ext/tests/reading.rs`; the plain words listed
  in `COMMON` no longer count as siblings. The catch-up card properties
  Burrow doesn't read moved into the existing catch-up exemption group.
