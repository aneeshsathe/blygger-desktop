section: Development

- **Key handlers: a modifier read outside the keymap is on a reviewed
  list, and none reads the platform key alone.** `platform` is Cmd on a Mac
  and the Windows key on Windows, so a chord tested on it is dead there
  (About's close and the Reader's select-all were; the Publish sheet's
  Alt+1 to Alt+9 lost to the keymap's own Alt+1 on Windows). A scan in
  `crates/blyg-app/src/key_handlers_tests.rs` fails a condition that reads
  `modifiers.platform` without `modifiers.control` or `secondary()`, and a
  function that reads a modifier with no line in
  `crates/blyg-app/fixtures/key-handlers-allow.txt` (guard, mouse, chord or
  mac-only, with a reason). Each chord line is checked against the Mac and
  Windows key tables. The platform-key check is red on the sites it found;
  their owner fixes them.
