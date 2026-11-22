section: Development

- **Checks: a contract property named by one plain word needs an anchor.**
  `blyg-core/tests/it/contract_coverage.rs` counted a schema property as
  covered when any test held its name in quotes, so an unrelated JSON key
  `"why"` made `SpellcheckIssue.why` look tested, its honest exemption failed
  as stale and was deleted. A property whose name is one lowercase word
  (`why`, `raw`, `id`, `kind`) is now covered only by its quoted name within 6
  lines of its schema's or operation's name (or the operation's path), or of
  a sibling property that is not itself a short plain word. A failure for such
  a property says what the test must hold. Names like `ifNoneMatch` keep the
  bare rule.
