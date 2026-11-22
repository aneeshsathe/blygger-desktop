section: Added

- **Reading: blygger-studio 0.68's tabs.** The Reader's sources now have the
  studio's Feed (the All feed, renamed), Agentic feed (a placeholder, as
  upstream; its prompt is Settings › AI's *Agentic feed prompt*), Saved,
  Background and Raw (what came in, newest first, with nothing moved to
  Background or folded). Burrow's other sources stay
  (`crates/blyg-app/src/reading/feed.rs`, `reading/sources_vm.rs`).
- **Reading: the pace-layered Feed.** The Feed is pace-layered by default,
  ported from the studio's reader order: new posts arrive on top and sink as
  others come, more slowly the more their blyg pinned them, and a new version
  climbs one place. Chronological is newest first. The choice is in the
  reading header and in Settings › READER ORDER, and is saved on the blyg as
  `readerOrder` (studio 0.68), else on this computer
  (`crates/blyg-core/src/pace.rs`, `order.rs`). Mentions and Background
  follow it too, as in the studio. The order approximates the studio's: it
  is worked out on this computer, a version counts as maintenance (and
  doesn't climb) only when your blyg's Background says so, so switching
  such a rule off makes those versions climb, and a post you haven't opened
  counts as its first and latest versions. By default, posts edited since
  you read them are no longer listed on top: a new version climbs one
  place. Chronological keeps the old order, edits on top.
- **Background: four more rules (studio 0.68).** Quotes refreshed, Including
  rewritten quotes, Small edits and Followed within 15 minutes, each with its
  badge and a switch in Settings, on by default; Including rewritten quotes
  acts only while Quotes refreshed is on (`crates/blyg-core/src/background.rs`,
  `change_size.rs`).
- **Reading: the shelf is called Saved**, as studio 0.68 calls it: “+ Save” and
  “✓ Saved” under a post, Post › Saved… (⌘B) and Post › Save / Remove from
  Saved (⌥⌘B; Ctrl+B and Ctrl+Shift+A on Windows), with the toast “Added to
  Saved”. Display only: nothing stored or synced is renamed.
