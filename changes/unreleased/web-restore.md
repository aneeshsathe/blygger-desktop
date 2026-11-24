section: Added

- **Burrow in a browser is back.** The web app (`crates/blyg-web`), removed
  in 0.26.0, is served again by your blyg at `{mount}/burrow/` and speaks
  studio 0.69 (contract 6; contract 5 still works). It finds the API beside
  itself at `{mount}/api`, so a blyg under a path works without the
  host-root `/api` that studio 0.65 dropped. With no `/api/changes`, it
  refreshes by conditional reads: each list asked with `If-None-Match`, an
  unchanged one answering a bodyless 304 that the page reads as what it
  already holds, and the reading list asked only by its first page until
  that moves. Reading has studio 0.68's tabs: Feed (in the blyg's reader
  order, pace-layered or chronological, chosen in the sources pane), Mentions,
  Agentic feed, Saved (the shelf), Background and Raw. Not in the web app
  yet: syndication as native 0.28 has it, full-text search, the dictionary and
  the Agentic feed's helpers (`site/src/dev/web.md`).
