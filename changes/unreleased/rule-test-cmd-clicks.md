section: Development

- **Tests: none holds the platform key by hand.** `platform` is the cmd key on a
  Mac and the Windows key on Windows, so a test that built a cmd-click as
  `platform: true` (or `Modifiers::command()`) failed on Windows CI only. A
  scan in `crates/blyg-app/src/key_handlers_tests.rs`
  (`tests_dont_hold_the_platform_key_bare`) fails on such a test unless it is in
  the `Platform::Mac` arm of a `match` or says `the platform key itself` in a
  comment; write `Modifiers::secondary_key()` for a cmd-click. It also fails a
  hand-written `simulate_keystrokes("cmd-...")`; keys go through `keymap::keys`.
