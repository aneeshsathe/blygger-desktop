section: Fixed

- **Ctrl+click on Windows follows a link in the editor and keeps the
  command palette open.** A ⌘-click (Ctrl+click on Windows) on a link in
  the editor opens what it names, and on a palette row runs it and keeps
  the palette open, as ⌘⏎ does. Both now read the held keys through
  `keymap::secondary_held`, as the Reader's clicks do, so the tests check
  the Windows Ctrl+click on a Mac too.
