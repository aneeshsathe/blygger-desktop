section: Added

- **Reading: Retry reads, which also re-reads the lists (studio 0.71).** A
  read that failed (Responses, a post's history) now says so with **Retry
  reads** beside it, and **Blyg › Retry Reads** does the same from anywhere.
  It asks your blyg again (a pull), reads again what failed on screen, and
  re-reads every list on screen: your posts, the reading rows and the unread
  counts, as the studio's "retry reads" now reloads the page's lists too. One
  runs at a time; one that fails again says why.
  `crates/blyg-app/src/reading/retry_reads.rs`.
