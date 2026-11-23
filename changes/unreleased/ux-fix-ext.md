section: Changed

- **Tour: the automatic cross-post step shows what it teaches.** The tour
  runs a sample cross-post, opens the sample's published post set to
  cross-post to Bluesky, and waits for ⇧⌘P, whose rows for the post (one on,
  two off) are the ones the caption describes. Its macros and "Open <site>"
  rows say what they would do and load nothing (`extensions/tour.rs`).
- **Publish sheet: the line under the "Then cross-post to" chips names the
  keyboard's way to them.** "Or ⇧⌘P and type “automatically” to pick one by
  keyboard" (Ctrl+Shift+P on Windows): the palette's "Cross-post
  automatically to <site>" rows reach every site. The chips themselves are
  clicked (`extensions/auto_crosspost.rs`).
- **Extensions palette: a long list scrolls under its heading,** with the
  key row always in view; ↑/↓ keep the chosen row in view, and the tenth row
  has a key (0). A macro whose title names its site no longer repeats it
  ("Cross-post to Bluesky…", not "Cross-post to Bluesky… ↗ Bluesky").
- **A blyg on a newer studio: the toast speaks plainly.** No contract number;
  it says the blyg's studio is newer than this Burrow, and on Windows, where
  there's no updater, that the newest Burrow is on the Releases page on
  GitHub (`server_notice.rs`).
- **Windows: the browser pane's first Esc says what it did.** "Keyboard back
  in Burrow · Esc again closes the pane", so that Esc no longer looks like it
  did nothing. The guides say "closes" for the second Esc throughout.
- **Shorter Dictionary intro; AI on Your Blyg puts studio 0.69's rows
  first.** The text helpers' model and the Agentic feed's tab name come right
  after TK, and the header says where models and keys come from without
  "models.json" or "wrangler". The sample Catch up summary is about the
  sample thread (quotes and links).
- **Development: `BLYGGER_TEST_SPELLING=windows` works in debug builds.** A
  `burrow-verify.sh` scene shows the app's text as Windows spells it (read
  through `DevFlags`); release builds ignore it.
