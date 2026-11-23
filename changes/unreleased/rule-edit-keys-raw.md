section: Development

- **The caret and edit-keys scan skips raw string literals.** A scan
  self-test that holds a sample `simulate_keystrokes("… cmd-shift-z")` call
  as data in an `r#"…"#` literal was read as a real call and failed
  `tests_dont_send_macos_only_caret_keys`. The scan now blanks raw strings
  (`r"…"`, `r#"…"#`, `br#"…"#`) before it reads a file, and still reads
  ordinary string literals, where real keystrokes are written
  (`crates/blyg-app/src/platform_text.rs`).
