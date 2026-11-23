section: Fixed

- **Publish: a long list of what generation read scrolls, and the sheet's
  keys stay on screen.** With 14 or more entries the note field and the
  ⏎/esc keys ran off the window. The list now scrolls inside 160 px
  (`ai/checkpoint.rs` `render_tk_read`), and any sheet taller than the
  window stops short of its bottom and scrolls (`sheet/mod.rs`).
- **What generation read: posts by title, never by id.** A post held here
  is named by its title and opens on a click; otherwise it is "a post on
  kit.blyg.example.com" or "a post of this blyg" (`ai/read.rs`, the
  Reader's list in `studio/reader.rs`). Only entries that tell someone are
  marked, under one line: "Web pages and your own posts tell no one." A
  page's site shows only when it isn't its host, and the Reader numbers
  each gap as the writer does ("TK 2", not the second gap that read
  something).
- **The checkpoint shows which link failed.** Details is open already when
  three or fewer links failed, and `d` shows or hides it. It says "links in
  this TK", counts "the other" right, says after esc to remove or replace
  the link, and ⏎ while another AI job runs keeps the sheet and says why.
  Once the blyg generates, the toast says "Your blyg noted what it read".
- **Settings › READ MARKS: ⇥ reaches "Count an entry as read".** Its own
  row (no longer under READER ORDER), ←/→ choose and space or ⏎ save, and
  each choice says what it means beside it; the 60% rule is in the guide's
  Reading page.
- **Tour: the date step highlights the newest card** and says "Click the
  date on the highlighted card".
- **Windows: Settings › NEARBY says "this PC"**, not "this Mac".
