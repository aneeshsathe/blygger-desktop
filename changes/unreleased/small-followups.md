section: Fixed

- **Ctrl+click on Windows follows a link in the editor and keeps the
  command palette open.** A ⌘-click (Ctrl+click on Windows) on a link in
  the editor opens what it names, and on a palette row runs it and keeps
  the palette open, as ⌘⏎ does. Both now read the held keys through
  `keymap::secondary_held`, as the Reader's clicks do, so the tests check
  the Windows Ctrl+click on a Mac too.
- **A cross-post started by hand no longer turns off an automatic one.**
  After a publish, an automatic cross-post counts as running while its
  text is still being prepared, before anything shows in the browser
  pane. A cross-post run by hand that ended in that moment, having posted
  without its copy being recorded, was taken for the automatic one, and
  turned that site off for the published post. The end of a run is now
  matched to the post and site it was for
  (`crates/blyg-app/src/extensions/auto_crosspost.rs`).
