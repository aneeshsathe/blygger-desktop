section: Added

- **Agentic feed: Burrow's own AI writes the edition.** Your blyg's own
  assistant isn't built yet, so the tab stayed "No edition yet". Now ⌥⌘E in
  the Agentic feed tab (Ctrl+Shift+J on Windows; Blyg › Write an Agentic Feed
  Edition Now; or the line above the tab) has the provider you set up in
  Settings › AI write one: a title and lede, a lead, sections and briefs, each
  post with a headline, a summary and why it's there, signed "Burrow ·
  <provider>". Settings › Agentic feed (`agentic-writer = after-sync`) writes
  one in the background after a sync that brings new posts, at most every 3
  hours (`agentic-writer-gap`), with `agentic-writer-provider` and
  `agentic-writer-model`. Off by default. It sends the titles and opening
  words of up to 60 unread or new posts, and your blyg's Agentic feed prompt,
  to that provider, and the edition to your blyg, only where your sign-in may
  write one (`putAgentic`). A reply it can't use is said, and nothing is sent.
  `crates/blyg-core/src/agentic_write.rs`,
  `crates/blyg-app/src/reading/agentic_writer.rs`.
