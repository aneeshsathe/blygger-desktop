section: Fixed

- **Windows: Ctrl+W closes the About window, and Ctrl+A picks every post in
  the Reader.** Both read the ⌘ modifier, which on Windows is the Windows
  key, so they did nothing there although the Reader's header says Ctrl+A.
  They are keymap actions now (`CloseAbout`, `PickAll`), bound in the About
  window's and the reading screen's key contexts; the Reader's search field
  keeps its own Ctrl+A (`keymap.rs`).
