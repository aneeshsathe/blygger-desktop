section: Changed

- **Quotes read text as the spec defines it** (blygger-studio 0.71, spec §2,
  decision #118). A partial quote's text now breaks at the start and end of
  every block: list items and lists, table cells and rows, definition lists,
  `div`s, figures, sections, `details` and `summary`, as well as paragraphs,
  headings, `br` and `hr`. A tight list item and the list nested in it are
  two lines, so a quote of both keeps the nesting, and table cells are
  quoted one per line. A draft quoting a parent item and its nested item as
  one line, or a table row as one line, now shows "quoted passage not found"
  until the quote is split into lines; a quote highlighted in the Reader
  across table cells comes in as an ordinary blockquote. An unlabelled
  `[[id]]` link's excerpt no longer runs table cells together
  (`crates/blyg-render/src/links.rs`).
- **A partial quote of list items keeps its bullets** (blygger-studio 0.69,
  spec §16.15, decision #114). Quoted lines that are list items in the
  quoted post bake as list items, in a list like the original's: an ordered
  run keeps its first number, and nesting is kept where both lines are
  quoted. The structure comes from the quoted post, never from the markers
  you type. Before, they baked as paragraphs
  (`crates/blyg-render/src/transclusion.rs`). The preview's parity fixtures
  come from Studio 0.71 (`docs/RENDER.md`).
