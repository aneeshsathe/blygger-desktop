section: Fixed

- **A link to a post with a formula reads the formula as TeX.** An
  unlabelled `[[id]]` link whose target holds a formula now shows it as
  `$e^{i\pi}$`, as Studio does, instead of the formula's symbols run into
  its source (`crates/blyg-render/src/links.rs`).
