section: Added

- **AI: ⌘G on your blyg stops when it can't read a link.** With the blyg
  server as the provider, ⌘G calls `generate_on_blyg`; a checkpoint opens a
  sheet ("1 of 2 links in the draft could not be read. Generate anyway?")
  with the failures under Details. ⏎ generates with `proceed`, esc leaves the
  draft untouched. Local providers are unchanged. `crates/blyg-app/src/ai/checkpoint.rs`.
- **AI: what each generation read.** The posts a gap drew on and the web
  pages it read (site and address), marked when publishing tells another
  blyg, under the preview and in the Publish sheet; posts you read list what
  their generated text read, escaped. `crates/blyg-app/src/ai/read.rs`,
  `crates/blyg-app/src/studio/reader.rs` (`generation_read`).
- **Reading: Count an entry as read.** Settings › READER ORDER offers the
  studio's choice (when I see it, when I open it, only when I mark it) when
  the blyg has it. "When I see it" uses the studio's rule (a card fully on
  screen, or 60% of the smaller of card and window, for a second) and sends
  marks in batches 3 seconds after the first, or when the stream stops being
  watched. Without the setting Burrow's earlier marks hold.
  `crates/blyg-app/src/reading/count_read.rs`.
- **Reading: a card's date opens its post** in the stream's side pane, and a
  Reader row's date opens it too; both underline on hover.
- **Tour: "A date opens its post; what counts as read"** in the reading
  tour (since 0.28.0).
