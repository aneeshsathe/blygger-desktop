section: Added

- **Picker: extension tabs (studio 0.71's picker sources).** An extension's
  manifest can add `[[picker-sources]]` tabs to the `[[` / `![[` popup and
  ⌘K's quote panel, after Burrow's own "items". Burrow asks the extension
  (`extension/picker.search`) 150 ms after the last key; its results are
  item ids only (others and repeats are dropped, at most 50), and ⏎ writes
  `[[id | label]]`. A tab is shown only for the forms it declares, the one
  you pick is remembered for the session, and an extension's error is said
  in the picker. `crates/blyg-ext/src/picker.rs`,
  `crates/blyg-app/src/composer/picker.rs`, `docs/EXTENSIONS.md`.
