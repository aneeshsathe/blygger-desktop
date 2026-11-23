section: Fixed

- **Connecting a blyg puts its “Count an entry as read” in force at once.**
  Connecting (from the sample data, say) swapped the backend but kept the
  reading state of the one before: its rule, rows, folders and order. Until
  Settings opened or Burrow restarted, entries were marked read by the old
  rule. Every place the backend changes (switching, connecting, signing out,
  the sample data, the tour) now rebuilds the reading state in one function,
  which writes the waiting seen marks first and reads the new blyg's rule
  (`crates/blyg-app/src/reading/count_read.rs`).
- **⌘G on the blyg: a word typed while it generates is kept, with no
  conflict.** A keystroke elsewhere in the post while the blyg wrote ended in
  the conflict sheet every time. The editor now merges the generated text
  with what was typed, and that merge is pushed over the blyg's text
  (`Backend::keep_generated`). A keystroke in the scope the blyg filled
  still ends in the conflict sheet, so neither text is lost
  (`crates/blyg-app/src/ai/generate.rs`, `crates/blyg-core/src/live.rs`).
