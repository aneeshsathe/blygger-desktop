section: Development

- **Studio 0.71 (contract 6.3.0) in blyg-core.** The vendored contract and
  `[target]` move to studio v0.71.0. Generating on the blyg
  (`Backend::generate_on_blyg`, `Api::generate`) answers
  `blyg_core::generate::GenerateOutcome`: the text, or studio 0.70's
  checkpoint (`read` of `total` links, and the ones that failed, with why),
  which saves nothing; `proceed` generates past it. `readerMarkRead`
  ("Count an entry as read": seen, opened, never) is read and written alone
  (`Backend::load_reader_mark_read`, `save_reader_mark_read`,
  `reader_mark_read`; `blyg_core::mark_read`), offered only by a blyg whose
  settings have it. The UI for both comes next.
