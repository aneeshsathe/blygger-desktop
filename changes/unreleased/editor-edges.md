section: Fixed

- **Editor: the edge cases of studio#168.** Bold or italic over several
  lines no longer counts a line of only asterisks (`**`, a rule) as already
  formatted, so it no longer strips it; formatting a selection of only
  spaces puts the markers after the spaces, not around them; ⌃⌘M over a
  displayed `$$…$$` formula leaves it alone instead of wrapping it again;
  and a save that fails with no message says "Couldn't save (no reason
  given)" instead of an empty toast. Formatting a selection whose edge
  falls inside an emoji or an accented letter takes exactly the selection.
  The studio's other cases don't apply to Burrow: it has no code-block
  command, hard-break button, Tab list indent, link URL cleanup, read-only
  editor regions or React state, its keys are read by the GPUI keymap, and
  its toasts already replace each other.
  `crates/blyg-app/src/app/format.rs`,
  `crates/blyg-app/src/editor_edges_tests.rs`.
