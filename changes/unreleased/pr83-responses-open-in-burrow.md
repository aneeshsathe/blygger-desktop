section: Changed

- **Responses: a response opens its post in Burrow.** On the Responses
  screen (⇧⌘M), clicking who responded, or ⏎ on the row cursor, opens the
  post that mentioned yours in the reader, as a blyg link in a post does,
  rather than in the default browser. Its ↗ now opens the post's page in
  Burrow's browser pane (where links go; the default browser if the pane is
  off), not straight in the default browser; **o** on the row cursor still
  opens the default browser, and the header says so ("⏎ open · o open in
  browser ↗"). **On “…”** above each group opens your own post, the one
  those responses point at, in Posts, and underlines on hover as a link does
  (`crates/blyg-app/src/reading/mentions.rs`).
  Thanks to @patwater
  ([#83](https://github.com/aneeshsathe/blygger-desktop/pull/83)).
