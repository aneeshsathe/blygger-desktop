section: Fixed

- **⌘G on the blyg: a keystroke while it generates no longer loses the
  generated text.** An edit typed during the request used to be pushed at
  the revision the generate answer reported, over the words the blyg had
  just written (their provenance stayed). It is now refused and read again
  with conflict detection, so both texts are kept for you to choose
  (`crates/blyg-core/src/live.rs`).
- **Provenance: a refused citation keeps the scope's web pages.** When the
  blyg refuses the provenance of an edit (a cited post that no longer
  resolves), Burrow retries without the posts and keeps the pages first,
  before it drops every source (`crates/blyg-core/src/sync/mod.rs`).
- **Provenance: sources and fields Burrow doesn't know are kept.** A source
  of a new kind, a page without its citation, a post whose version isn't a
  number, or a field Burrow doesn't know on a scope or a source no longer
  drops the whole scope's disclosure (pushed back as `null` on the next
  edit); each is written back as the blyg served it
  (`crates/blyg-core/src/model.rs`).
- **Count an entry as read: seen marks are no longer lost.** The marks
  waiting to go together are written when Burrow quits, when the window
  closes, and before switching blygs, always to the blyg they were seen
  on. Choosing another rule drops the marks “when I see it” collected. A
  first read of the choice that failed (offline at launch) is tried again
  the next time Reading opens, and a value Burrow doesn't know reads as
  “when I open it”, the safer rule
  (`crates/blyg-app/src/reading/count_read.rs`).
- **What generation read: long lists show the first 50 entries**, then how
  many more, in the writer and under a post in the reader.
