section: Fixed

- **Lineage: the action ring reads at 800 px.** In a narrow window the
  panel beside the graph goes under it, and each answer ("yes, as a
  reply") wraps under its question instead of standing one letter per line.
- **Reader: Browsing history leaves the post room.** The panel has its own
  ground, and in a narrow window the list column, then the sources, make
  way so the post keeps a reading width (`reading/pane_fit.rs`). The
  author's own site gets the same room.
- **Header keys are whole or gone.** The keys in the Reading, Responses and
  Subscriptions headers drop whole hints, last ones first, when the header
  is short, instead of cutting one mid-word with "…".
- **Side panels leave the editor room.** Beside ⌘Y's history, the notes
  drawer or the preview, the posts list makes way rather than squeezing the
  editor or the version's text to a sliver. In a window under 1000 px the
  notes drawer takes at most 42% of it. A browser pane too narrow for a page
  fills its room, and its toolbar no longer draws over the notes drawer.
- **Toasts wrap and stay clear.** A toast ends left of the native preview
  (it used to run under it, cut mid-word), and sits above the editor's
  footer and Reading's compose bar.
- **Sheets fit the window.** Sheets stop above the status bar and scroll.
  Site settings and AI on your blyg keep Save in view. Client access shows
  when each grant ends, the conflict sheet says its texts scroll, and
  Settings › AI says "↓ more below" while it has more below.
