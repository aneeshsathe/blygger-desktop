section: Fixed

- **Web: your own posts in Reading are yours.** They keep their place in the
  Feed by date, as the studio's Feed has them, but no longer offer ● Unread,
  a swipe to mark read, or ↓ Background. A post of yours with edits you
  haven't published says "unpublished edits" on its card and in the reader,
  instead of showing the working copy as the public version
  (`crates/blyg-web/src/read_api.rs`).
- **Web: one name, Saved.** The button is "+ Save", the phone's tab, the
  sheet and Blyg's row say Saved, and Saved's empty state names a button
  that exists.
- **Web: › and ‹ draw as themselves** in the ET Book theme (the page now
  serves the desktop's fixed ET Book faces, as 0.25.1 did for the app),
  and bold ET Book is drawn, not made up by the browser.
- **Web: tables in posts** have cell padding and rules and scroll sideways
  when wide; **a wide code block** shows its scroll bar.
- **Web: the status bar tells the truth.** A scratch note says "on this
  device only", never "synced". A post changed elsewhere says "edited
  elsewhere · choose a copy" in amber beside the Keep mine / Use theirs
  banner, not a red "sync error".
- **Web: an over-long fragment shakes** at ⌃⏎ instead of opening the publish
  sheet, and ⌃D says when keeping it makes it a thread.
- **Web: the Agentic feed's empty state** says what writes an edition and
  what's coming, without a studio version number.
- **Web: Subscribe and Peek take a bare address** ("blyg.example.com"):
  Burrow adds `https://` and says its own error, not the browser's.
- **Web: key hints say the keys the page binds** (⌃, not ⌘, which browsers
  keep), in tooltips, the key help, the HUD and Blyg's list, from one table
  (`crates/blyg-web/www/keys.js`; the Rust's hints spell them the same way).
- **Web: Reading by keyboard.** Tab from the search goes to the list, ⏎ opens
  a card and ↑↓ move between cards; the search says ↑↓ step through posts.
- **Web: no `/api/changes` on a blyg without it.** The page asks only when
  the capabilities list `getChanges`, so a contract-6 blyg logs no 404 on
  every start.
- **Web: smaller things.** Toasts sit where they don't cover Publish or the
  search box, and a toast's link is readable; the Feed's order control says
  what each order does and keeps its place; the reader pane clears when you
  choose another source; Publish dims on a post with nothing new; the Posts
  list is taller; phone tap targets are 44 px; the phone's filter chips fade
  at the edge to say they scroll; "In your reader, show" reads as English.
