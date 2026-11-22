section: Fixed

- **Full editor: the live preview no longer jumps when you start a new
  line.** Pressing ⏎ moved the caret before the preview had re-rendered, so
  the preview scrolled by the old line numbers, often to the next paragraph,
  and then back once the render landed. It now waits for the render
  (`Studio::renders` in `crates/blyg-app/src/studio/mod.rs`). The wait is
  longest on Windows, where the preview renders more slowly. Thanks to @patwater
  ([#88](https://github.com/aneeshsathe/blygger-desktop/pull/88)).
