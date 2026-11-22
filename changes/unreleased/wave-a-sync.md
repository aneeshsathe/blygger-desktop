section: Changed

- **Reading: copies elsewhere show on every stream card.** When an author
  lists where else they posted something (`syndication`, studio 0.67.1),
  each stream card says "the author says it is also at …" under its source
  line, and so does the full view, however it was opened. The wording is
  Studio's. A click on a copy opens it and leaves the card alone
  (`crates/blyg-app/src/syndication_ui.rs`).
- **Sync: studio 0.67+ without `/changes`.** The real-studio e2e now checks
  sync on a studio that has removed `GET /api/changes` (contract 6.0.0):
  one 404, then conditional reads only, an idle pull that reads only the
  reading list's first page, and settings writes that keep `readerOrder`
  and the new Background rules (contract 6.1.0). Copies elsewhere follow
  the released `getSyndication`/`putSyndication` shapes
  (`crates/blyg-core/tests/it/syndication_contract.rs`).
