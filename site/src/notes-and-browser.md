# Notes and the browser

## The notes drawer

**Notes while you read** (⇧⌘N, or View › Notes). A drawer slides in over the
right edge of the window, over the stream, the Reader, the browser pane or
your posts, with one rolling scratchpad, "Reading notes". Like any
[scratch note](writing.md#quick-capture-and-scratch-notes) it stays on your
Mac until you make it a draft (⌘D), and it's listed in your posts with a
`scratch` pill. esc or a click outside slides it away again.

Adding to it:

- **→ Notes** on a post adds a quote of it (`![[id]]` for a blyg post, a link
  for a feed post);
- **→ Notes** in the browser adds the page's link;
- with text selected in a post or a web page, ⇧⌘N (or → Notes) adds it as a
  quote with its source.

The drawer's header has **⌘D → draft**, **Open in editor**, and ⋯ › **New
notes page** (a fresh page; the old one stays in your posts). ⌘⏎ opens it in
the editor with the publish sheet. The drawer's editor has @-mentions,
spellcheck and paste-a-link-over-a-selection, and its footer says when ⇧⌘D
will quote your selection into your draft (see
[Universal quoting](reading.md#universal-quoting)).

![The notes drawer over the stream](screenshots/notes-drawer.png)

### Your Markdown notes: the Notes tab

With the bundled [markdown-notes](extensions.md#markdown-notes) extension
turned on, the drawer has two tabs: **Reading notes** and **Notes**. The Notes
tab opens folders of Markdown notes (Obsidian vaults, say), as many as you
like:

- a chip per folder switches between them (a folder Burrow isn't allowed to
  use yet shows in the warning colour, with **Allow…**);
- **Add folder…** picks more (several at once), and **Remove from Burrow**,
  beside the chosen folder's path, forgets it: the folder and its notes stay
  where they are;
- search, browse, open a note to edit it, or **Quote into post** / **Copy
  verbatim** to put it (or the selected part) in your draft.

There's no default folder. Until you choose one, the tab says so and offers
**Choose a folder…**. **Settings › Notes folders** lists the same folders,
each with Remove, and has its own **Add folder…**.

## The browser pane

A link in a post opens in a pane that slides in from the right, over the
reading view. ⌘-click opens it over the whole area, ⌥-click opens your default
browser instead, and esc closes it. ⇧⌘B brings back the last page.

The pane has back, forward and reload (⌘[ ⌘] ⌘R), an address field (⌘L), 🛡
(content blocking on or off for this site), ↗ (open in your default
browser), **→ Notes**, **→ Draft** (which reads **❝ Quote → Draft**
while text is selected on the page), and **✂ Clip**.

- **✂ Clip** (⇧⌘C, or Post › Clip Page to Draft) quotes the passage you
  selected on the page, or else the page's main article, in your draft, with
  the page's title and link. With no draft open it starts a new one (a thread
  when the clip is longer than a fragment).
- **Extensions run here too.** A macro, such as the bundled
  [cross-post](extensions.md#cross-post), uses this same side pane. You can
  fold the pane (esc, ×, ⇧⌘B) while it runs: the run goes on and the status
  bar shows **macro running · show pane**; click it or press ⇧⌘B to unfold.
  While a macro runs, the pane's bar has **■ Stop**.

- **Content blocking.** Ads and trackers are blocked with uBlock Origin's
  default filter lists, as WebKit content blockers, downloaded about once a
  week (a small built-in list until then). 🛡 turns blocking off for one site.
- **Privacy.** The pane has its own cookies and no access to the app: pages
  never see your token or keys. On macOS 14 and later, sign-ins in the pane
  survive a relaunch.
- **Downloads** go to your default browser.

`open-links = browser` makes links open in your default browser (⌥-click then
opens the pane), and `content-blocking = false` turns blocking off
everywhere. See [Configuration](config.md).
