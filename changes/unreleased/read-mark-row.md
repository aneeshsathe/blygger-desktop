section: Changed

- **Reading: opening a post no longer rebuilds the reading list.** Opening
  an unread post (or a stream card counting as seen) marks that one row
  read in place: its dot, the header's "N to read", the status bar and the
  sources pane's counts follow at once, without reading every post again
  (about 43 ms at 10,000 posts before). All unread still drops a post read
  elsewhere; a read state from another device still refreshes the list.
  A local read mark is now `CoreEvent::ReadMarked` with the rows it raised
  (`blyg-core/src/model.rs`); `crates/blyg-app/src/reading/read_mark.rs`.
