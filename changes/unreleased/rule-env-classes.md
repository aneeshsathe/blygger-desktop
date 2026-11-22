section: Development

- **Check: a `BLYGGER_*` variable the build ignores is said at startup, and
  every name has a class.** `crates/blyg-app/src/dev_flags.rs` lists each
  `BLYGGER_*` name in `CLASSES` as read in every build, read by debug builds
  only, read only by the test suite, or read elsewhere (another crate, or a
  script that exports it). A tester who set `BLYGGER_TEST_SPELLING=windows`
  against a build that ignored it saw Mac wording and was told nothing; a
  misspelt name (`BLYGGER_FAKEE=1`) or a debug-only name set against a
  release build did the same. What a developer sees now: a debug build
  refuses to start when such a variable is set, and prints each name with why
  (not a variable Burrow reads, with a "did you mean" when one name is a
  single edit away; read only by the test suite; read only by debug builds),
  then exits with code 2. A release build starts, and says each one once on
  stderr. Values are never read or printed. A new variable cannot be added
  without a class: the test `every_blygger_name_has_a_class` fails first.
  `blygger --version` and `--help` are handled after the environment check,
  as they always were, so in a debug build they too stop while such a
  variable is set.
