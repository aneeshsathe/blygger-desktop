section: Added

- **Reading: the Agentic feed shows its edition (blygger-studio 0.69).** When
  an assistant has written an edition on your blyg (`PUT
  /api/agentic/default`), the Agentic feed tab draws it as Studio does: a
  masthead with the edition's title, its lede and who wrote it ("✦ written
  by your assistant"), then its sections, each post as a card with the
  assistant's headline, summary and reason. Posts not yet on this computer
  say so and arrive with the next sync. The tab takes the blyg's name for it
  (`agenticTitle`). Burrow reads editions only; it never writes one
  (`crates/blyg-app/src/reading/agentic.rs`, `crates/blyg-core/src/agentic.rs`).
- **Catch up: the blyg's summary first.** On a studio offering
  `summarizeCatchUp`, Catch up's summary (s) is asked of your blyg's text
  helpers model, written once per set of versions and kept there; Burrow's
  own AI writes it when the blyg doesn't offer one, can't write it, or
  "whole conversation" is on (`crates/blyg-app/src/reading/catchup.rs`).
- **Blyg › Dictionary…: your blyg's spellcheck words.** On studio 0.69 the
  blyg keeps a dictionary shared with Studio and other clients. Burrow's
  spellcheck accepts every word in it, Learn Spelling in the editor adds the
  word there too, and Blyg › Dictionary… lists your words, adds and takes
  them out (a write that meets another client's merges onto it; one made
  offline waits on this computer and is sent when the blyg is back). Installed
  sets are listed by name (`crates/blyg-app/src/dictionary.rs`,
  `crates/blyg-core/src/dictionary.rs`).
- **Blyg › AI on Your Blyg…: studio 0.69's settings.** A text helpers model,
  the Agentic feed tab's name, and, under the change classifier, judging
  other blygs' changes (with an optional instruction, and keeping a post in
  view when its note understates the change). Each shows only where the
  studio has its key, and saving sends only what's shown
  (`crates/blyg-app/src/server_ai.rs`).
