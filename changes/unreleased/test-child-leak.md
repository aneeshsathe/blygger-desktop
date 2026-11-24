section: Development

- **Tests: a test's extension processes end with it.** GPUI's test
  contexts never drop the app, so a window's extension host was never
  dropped in a test, and every extension child (the test binary run as
  `reading_time_child`, `inspect_child`, `markdown_notes_child`,
  `cross_post_child`, …) lived until the whole test binary exited: one
  `cargo test -p blyg-app` had 135 alive at once. In a test the host is now
  stopped when the test's thread ends (`extensions/test_hosts.rs`), which
  brings the peak down to four per test thread, and
  `extensions/children_tests.rs` checks it on macOS and Windows. The tour
  tests whose steps don't show the sample extensions no longer start them,
  and a test without a `Launch` global gets no extension host at all.
