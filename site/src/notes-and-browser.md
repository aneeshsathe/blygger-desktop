# Notes and the browser

## The notes pane

**Notes to read while you write** (⇧⌘N, View › Notes, or the **Notes** tab at
the window's right edge). The notes open in a pane docked at the right of the
window, beside the editor, the stream, the Reader or the browser pane: the
rest of the window fits to its left, so nothing is covered, and clicking or
typing in your draft never closes it.

- **Moving between them.** ⇧⌘N opens the pane and puts the keyboard in it;
  **esc** puts the keyboard back where it was (your draft, at its caret) and
  leaves the pane open; ⇧⌘N again from your draft goes back into the notes.
- **Closing** is up to you: the pane's **×**, its tab, or ⇧⌘N while the
  keyboard is in it.
- **The Notes tab** opens or closes the pane without taking the keyboard
  from your draft, for reading beside it; click in the pane to write there.
- **Its width.** Drag its left edge (a double-click puts the default back).
  The text keeps a readable line length however wide you make it.
- **It remembers** which note is open and where you'd scrolled while you
  work, and whether it was open and how wide across launches.

It holds one rolling scratchpad, "Reading notes". Like any
[scratch note](writing.md#quick-capture-and-scratch-notes) it stays on your
Mac until you make it a draft (⌘D), and it's listed in your posts with a
`scratch` pill.

Adding to it:

- **→ Notes** on a post adds a quote of it (`![[id]]` for a blyg post, a link
  for a feed post);
- **→ Notes** in the browser adds the page's link;
- with text selected in a post or a web page, ⇧⌘N (or → Notes) adds it as a
  quote with its source.

The pane's header has **⌘D → draft**, **Open in editor**, and ⋯ › **New
notes page** (a fresh page; the old one stays in your posts). ⌘⏎ opens it in
the editor with the publish sheet. The pane's editor has @-mentions,
spellcheck and paste-a-link-over-a-selection, and its footer says when ⇧⌘D
will quote your selection into your draft (see
[Universal quoting](reading.md#universal-quoting)).

![The notes pane, with a markdown-notes note open, and Nearby beside a draft](screenshots/notes-drawer.png)

### Your Markdown notes: the Notes tab

With the bundled [markdown-notes](extensions.md#markdown-notes) extension
turned on, the pane has two tabs: **Reading notes** and **Notes**. The Notes
tab opens folders of Markdown notes (Obsidian vaults, say), as many as you
like:

- a chip per folder switches between them (a folder Burrow isn't allowed to
  use yet shows in the warning colour, with **Allow…**);
- **Add folder…** picks more (several at once), and **Remove from Burrow**,
  beside the chosen folder's path, forgets it: the folder and its notes stay
  where they are;
- search, browse, open a note to edit it, or **Quote into post** / **Copy
  verbatim** to put it (or the selected part) in your draft at its caret.
  The note stays open in the pane, where you were in it, so you can keep
  reading it while you write.

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

With **Open posts on the author's site** on, an opened post's own page shows
in the reading layout, in a web view like this pane's (the same cookies,
blocking and shields), and the pane stays free for links. See
[Read it on the author's site](reading.md#read-it-on-the-authors-site).
