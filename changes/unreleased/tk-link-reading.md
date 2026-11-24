section: Added

- **AI: Burrow's own providers read a gap's links first (studio 0.71).**
  ⌘G with codex, Claude Code, a CLI or an API key now does what your blyg
  does since studio 0.70: before writing, Burrow reads the links you placed
  in the instruction or the post. A `[[id]]` link, or the address of a post
  on its own blyg, is the post Burrow holds; any other http(s) address is
  fetched (at most 8 pages and 8 linked posts, 15 s and 4 MB a page, HTML
  and plain text only, no cookies), its text cut at 30,000 characters and
  given to the model fenced as untrusted material, with the studio's "Web
  pages" rule added to the system prompt. A link that can't be read stops
  at the same checkpoint ("1 of 2 links in this TK could not be read.
  Generate anyway?"); ⏎ goes on without it. The pages read are recorded
  with the gap (`pages`), and listed under the preview and in the Publish
  sheet. A post a gap only links is given to the model but not cited, since
  Burrow's provenance check (`tk::validate`) accepts only quoted sources.
  `crates/blyg-ai/src/hydrate.rs`, `crates/blyg-app/src/ai/generate.rs`,
  `site/src/ai.md`.
