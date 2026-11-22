section: Changed

- **Responses: a response opens its post in Burrow.** On the Responses
  screen (⇧⌘M), clicking who responded opens the post that mentioned yours
  in the reader, as a blyg link in a post does, rather than in the default
  browser. Its ↗ now opens the post's page in Burrow's browser pane (where
  links go; the default browser if the pane is off), not straight in the
  default browser; **o** / ⏎ on the row cursor still open the default
  browser. **On “…”** above each group opens your own post, the one those
  responses point at, in Posts (`crates/blyg-app/src/reading/mentions.rs`).
  Thanks to @patwater
  ([#83](https://github.com/aneeshsathe/blygger-desktop/pull/83)).
