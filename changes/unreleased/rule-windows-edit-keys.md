section: Development

- **Checks: a test may not send a redo key as a literal.** The scan in
  `crates/blyg-app/src/platform_text.rs` (`tests_dont_send_macos_only_caret_keys`)
  now reads every keystroke literal a test sends, also through
  `keymap::keys("...")`, and fails on `cmd-shift-z` or `ctrl-shift-z`: the
  Windows text field binds only Ctrl+Y for redo, so such a test passed on the
  Mac and failed on Windows CI. Tests call `platform_text::redo_keys()`, which
  `composer/assist_tests.rs` now does.
