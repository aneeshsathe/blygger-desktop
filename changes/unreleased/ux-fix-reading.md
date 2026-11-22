section: Changed

- **Reading: one name for the reader order.** Settings › READER ORDER now
  says what the reading header says, in the same order: Pace-layered and
  Chronological (the studio's words), not "Newest first"
  (`crates/blyg-app/src/reading/feed.rs`).
- **Agentic feed: the edition is a page.** With no post open, the edition
  (masthead, sections and cards) takes the reading pane in larger type, as
  the studio sets it, rather than the list column beside an empty pane. A
  card, or j/k, opens its post with the edition beside it; esc brings the
  page back (`crates/blyg-app/src/reading/agentic.rs`, `reading/list.rs`).
- **Reading: formulas in cards read as text.** Agentic feed cards and stream
  cards showed TeX (`$e^{i\pi}$`, `\int_0^1 …`). A formula now reads as
  text, `e^(iπ)` or `∫₀¹ x² dx = 1/3`, in the post's words, the
  assistant's headline and summary, and the stream; the Reader still draws
  it (`crates/blyg-app/src/reading/maths_text.rs`, `reading/stream.rs`).
- **Saved: the empty list names the key.** "Nothing saved yet. ⌥⌘B or
  “+ Save” on any post puts it here" (Ctrl+Shift+A on Windows)
  (`crates/blyg-app/src/reading/sources_vm.rs`).
