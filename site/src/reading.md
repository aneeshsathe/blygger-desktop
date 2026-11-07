# Reading, quoting and responses

⌘R opens reading: the posts from the blygs and feeds your blyg follows. ⌘R
again goes back to your own posts. Subscriptions (⇧⌘S, or Blyg ›
Subscribe…) are kept on your blyg, so every studio sees the same list.
Pasting the address of one of a blyg's public hoppers (its page or its feed)
follows the whole blyg.

## The stream

Reading opens as **the stream** (⌥⌘1): one timeline of everything you follow,
newest first, drawn natively so it stays fast with hundreds of posts.
Fragments show in full; threads show a few lines and **Read more** (⏎ or
Space) opens the whole post in a side pane. j/k (or ↑/↓) move; esc closes
the pane and the stream keeps its place.

A post counts as read after a second on screen, and unread posts get a dot,
never a count. Posts are dated by when their author published or last edited
them, and an edit updates the post in place rather than adding a new one.
Quotes show as grey boxes with the quoted text (a quote inside a quote
nests, three layers deep), and `[[id]]` links read as the post they point
to. Rest the pointer on a quote box or a link to see the same card the
editor shows: title, author, version, first lines. A post that has been
revised leads with its latest changelog note,
"V3: …", read from the author's changelog once the post is on screen, or
"V3: No changelog entry available" when they left none.

![The stream](screenshots/stream.png)

## The Reader

**The Reader** (⌥⌘2) has three panes, like NetNewsWire: sources on the left
(All unread, Today, Thumbed, All, Background, then **Your blyg**, your
**hoppers**, your folders and the rest), the posts from that source in the
middle, and the post on the right.

- **Mentions**, under All unread, lists the posts you hold that mention you:
  ones that quote or reply to your posts, or link to your blyg (or, on studio
  0.56 and later, what your blyg's own mentions lens says). A dot says there's
  something to read; there's never a count.
- **Background** lists withdrawn posts, the blygs' and your own, newest
  first, and nothing else lists them: not the other feeds, the counts or the
  stream. A withdrawn post is never unread. Opened, it reads as withdrawn
  (with its retained pin, when the author kept one).

- **Hoppers** are your blyg's collections of posts (see
  [Hoppers](#hoppers) below). Picking one lists its posts.
- **Folders** stay on your Mac. Drag a subscription onto one to file it, or
  use its context menu; Blyg › New Folder… makes one.
- ←/→ move between the panes, j/k go to the next or previous post from
  anywhere, and Space scrolls the post, then moves to the next unread one.
- ⌥⌘S hides the sources pane, and its edge drags to resize.

![The three-pane Reader](screenshots/reader-three-pane.png)

### Your blyg

Your own published posts are in Reading too. They sit among everyone else's
by date in All, Today and the stream, and alone under **Your blyg** in the
sources pane (once you've published something). They read as "You", with the
same reading pane, lineage and actions: Quote into a thread, Reply · new
stub, Link post, Open on web and → Notes. There's no AI reply to yourself,
and no Fork here: your own post forks from a pin in ⌘Y (see
[Replying to, quoting and linking your own posts](writing.md#replying-to-quoting-and-linking-your-own-posts)).

Your posts are **never unread**: they get no dot, never count in All unread
or any folder's count, and have no 👍 / 👎. Nothing about them is sent as
read state. Opening one shows your current version and your pins, and, when
you have unpublished edits, the text as it's published.

### Hoppers

A hopper is a collection of posts on your blyg: other people's posts and your
own, kept together under a name. Your hoppers are listed under **Hoppers** in
the sources pane, below Your blyg, each with how many posts it holds and a
globe when it's public. Pick one to list its posts in the middle pane in the
hopper's own order: the post put in it most recently comes first, as on your
blyg. Withdrawn posts aren't listed there; they stay under Background.

**Put a post in a hopper.** With a post open or selected, press **h** (or
⌥⌘P, Post › Put in Hopper…), or choose **Hopper ›** from its action chips or
its right-click menu. Each hopper is listed with a check by the ones the post
is in. Choosing a hopper (click, ⏎, Space, or its number) puts the post in or
takes it out, and **New Hopper…** (n) makes one and puts the post in it. Your
own posts go in hoppers too. A change shows at once and is sent to your blyg
in the background, so one made offline is sent when you're back online.

**Several posts at once.** Pick posts in the list first (⌘-click, ⇧-click,
or ⌘A, as for marking posts read below), then press **h**, or
choose **Hopper ›** from a picked post's right-click menu. The picker names how
many posts it acts on. A hopper holding all of them has a check, one holding
some of them a dash. Choosing a hopper puts in the ones not there yet, or,
when it already holds them all, takes them all out. **New Hopper…** makes one
with all of them in it.

**Manage a hopper** from its right-click menu:

- **Rename…** gives it a new name. Once it's public, its address stays the same.
- **Description…** sets the line shown on its page (280 characters at most).
- **Make Public…** gives it its own page on your blyg (`/h/<name>/`), listed
  under Collections on your homepage and archive. It never appears in your
  feed. Burrow asks first, and then shows the address. **Make Private** takes
  the page down.
- **Delete Hopper…** deletes the hopper after asking. Its posts aren't
  touched.

New hoppers are made with **New Hopper…** in the sources pane's right-click
menu, or Blyg › New Hopper…. A new hopper is private. Making, renaming and
deleting a hopper happen on your blyg, so they need it to be reachable. If the
hopper changes on your blyg at the same moment (say, renamed in the web
studio), Burrow reads it again and sends a rename or description once more.
Making it public is sent again only if its page would keep the address
Burrow showed you; otherwise Burrow says so and shows the hopper as your blyg
now has it, and you can choose again.

| | |
|---|---|
| ![A hopper picked in the Reader](screenshots/reader-hoppers.png) | ![The Hopper picker on a post](screenshots/reader-hopper-picker.png) |

**Search** (⌘F, or `/` with the list focused) filters whatever you're reading
as you type, over the title, the author, the blyg and the text of the posts
already on your Mac. Nothing is fetched to search.

**Mark read or unread.** ⌘-click and ⇧-click pick several posts, ⌘A picks
every post in the list, and esc lets go. **r** marks the picks (or the open
post) read, **u** unread; the right-click menu says how many (*Mark 3
Unread*).

![Picking posts in the Reader](screenshots/reader-picks.png)

**Read state syncs** between your Macs through your blyg when your sign-in
allows **reading:state** (see [Server requirements](server.md)), and marking
unread syncs too. Otherwise it stays on this Mac.

**Subscriptions** take their name from the blyg or feed, and follow it when
it changes. Right-click one for **Rename…** (your own name, kept as you
set it), **Use the Blyg's Own Name** to hand it back, and **Check Now**.
**⇧⌘R** (Blyg › Check All Feeds Now) asks your blyg to poll every
subscription at once.

| | |
|---|---|
| ![A subscription's menu](screenshots/reader-sub-menu.png) | ![Renaming a subscription](screenshots/reader-sub-rename.png) |

## Write while you read

A post box runs along the foot of the reading screen, under the Stream and
under the Reader's panes alike, so you can write without losing your place.
It never covers the post you're reading. Press `c` (with the list focused)
or ⇧⌘K from anywhere in the window (Blyg › Compose While Reading), and type.
It grows as you write, up to eight lines.

- **⌘⏎** (or **Post**) publishes it as a fragment, the way quick capture
  does. A post that quotes another, or runs past 1,000 characters, goes as a
  thread; the footer says which.
- **⌘D** (or **Draft**) keeps it as a draft on your blyg.
- **⤢** moves the text into the full editor as a draft, for longer writing.
- **❝ Quote**, with a blyg post open, puts that post's `![[id]]` in the box.
- `[[`, `![[` and @-mentions work as they do in the editor.
- **esc** gives the keys back to the list and keeps your text. While the
  box has the keys, the list's single-key shortcuts don't fire.

Text you haven't posted is kept across screens and restarts. A post that
fails keeps the text and says why; trying again won't make a second post.
The Posts screen is unchanged.

Burrow now opens on Reading. It stays on Posts during the first-run setup
and the tour, and when there's no blyg to read from.

## Trails

A trail is a path through posts you chose to keep, with a word on why each
one is there: yours and other people's, in your order.

- **t** adds the post you're reading to the current trail and asks why
  (⏎ keeps it, esc skips). With no trail yet, it asks for a name first.
- **⇧T** starts another trail, with this post as its first stop.
- The **trail bar** above the post box shows the trail's name (click it for
  the next trail), its stops (click one to open it, drag to reorder, × to
  take it off) and "stop 2 of 4" for the post in front of you.
- The Reader's sources pane lists your trails under **Trails**. Pick one and
  the list shows its stops in order, so j/k walk it.
- **⌥⌘T** (Blyg › Publish Trail…) writes the trail as a thread of quotes,
  `# name` then `![[id | why]]` for each stop, into a new draft. Nothing is
  published until you publish it. **h** on that sheet makes a private
  hopper of the stops instead.

Trails stay on this Mac and tell no one. Posts from feeds can't be stops
(a trail publishes as quotes). A post its author withdraws stays on the trail
as "withdrawn", without its text, and is left out when you publish.

On Windows the keys are Shift+T and Ctrl+Alt+T.

## The quote tray

Found something you'll want to quote later? Press **q** on the reading
screen, or **⌥⌘K** anywhere (Post › Put on Quote Tray), and it goes on the
**quote tray**, a strip above the status bar. The tray is kept for each blyg.
When you quote, the `[[` / ⌘K picker lists the tray first, marked ❝; placing
a post takes it off. On the strip, ←/→ pick a post, ⏎ places it, ⌫ takes it
off.

You can also **drag** a post into what you're writing: a stream post, a
Reader row, a lineage row or a tray chip. In a thread it lands as a quote on
its own line, after the line you dropped it on; in a fragment as a link where
you dropped it. Dropped on the omnibar, it starts a new thread quoting it;
dropped on the post box under Reading, it's quoted there.

On Windows the key is Ctrl+Alt+K.

## Import and export subscriptions (OPML)

Moving from another feed reader? Export an OPML file there (Feedly,
NetNewsWire, Inoreader, Miniflux, Reeder and most others can), then choose
**Blyg › Import Subscriptions from OPML…**, or **Import OPML…** on the
Subscriptions screen (⇧⌘S).

- Burrow lists the file's feeds, grouped under the folders they had in the
  other reader. Feeds you already follow say **already following** and start
  unticked, and a feed listed twice appears once.
- 1–9 or Space tick and untick, ⌘A ticks all or none, ⏎ imports the ticked
  ones, and esc cancels.
- Every feed is subscribed on your blyg, as if you'd added it by hand. The
  import goes slowly, about 30 feeds a minute, because each new subscription
  makes your blyg fetch that feed's archive. Esc stops it after the feed in
  progress, and what's done stays done.
- Imported feeds go into a folder named **Imported feeds** in the Reader,
  made the first time you import (file them elsewhere whenever you like).
  Feeds you already followed stay where you filed them. The other reader's
  folders are only shown in the list; they don't become folders here.
- Imported feeds aren't added to your public blogroll.
- At the end Burrow says how many were added, already followed, or failed,
  with each failure's reason: *not a feed* (nothing at that address your blyg
  could read as a blyg or a feed), *unreachable* (Burrow couldn't reach your
  blyg), or *refused*. **r** retries the failed ones.

**Blyg › Export Subscriptions as OPML…** (or **Export OPML…** on the
Subscriptions screen) saves every subscription to an OPML file
(`burrow-subscriptions.opml`) that other readers can import. Your blyg's
public `blogroll.opml` lists only the subscriptions you put in your blogroll;
the export lists them all.

Both need a connected blyg, since your subscriptions live there. From a
terminal: `blygger +import-opml <file>` (add `--dry-run` to see what would be
added) and `blygger +export-opml <file>`.

## Acting on a post

The selected post shows its actions. Each says what it makes:

- **Quote into a thread**: `![[id | label]]` in a thread of yours, labelled
  with the post's heading or title (see
  [labelled links](writing.md#labelled-links)).
- **Reply · new stub**: a new thread that responds to the post (a *stub*,
  Blygger's reply). It starts quoting the whole post. A line above the
  editor says what the stub is doing. To quote only part of the post, select
  the text inside its quote in the editor's preview (**select a passage in
  the preview** shows it) and press **❝ quote only this**, or ⇧⌘D: the
  passage goes under that quote's `![[id]]` as `>` lines, whichever of
  several quotes it is. **Quote another passage** adds a whole-post quote
  after the caret to select from, for a running commentary; **quote whole
  post** goes back. A quote inside the quote, or a preview that hasn't
  caught up with your typing, offers nothing.

  ![Choosing a passage in the stub's preview](screenshots/stub-passage.png)

- **AI reply · new stub**: the same, with a first draft from AI.
- **Link post · new fragment**: a fragment that links to the post (`[[id]]`)
  without responding to it.
- **Fork**: a new thread from a *pinned* version of the post. Forking only
  descends from pins, so on the current version Fork is greyed out and its
  tooltip says where to find a pin.
- **→ Notes**: adds the post to your [notes](notes-and-browser.md).
- **Open on web**, and 👍 / 👎.

![Reading a post](screenshots/reading.png)

### Versions of other people's posts

The version pill in a post's header, `‹ vN ▾ ›`, steps through the current
version and the **pinned** ones (unpinned past versions of other people's
posts are never shown). On a pin you can **Quote this version**, **Fork this
pin**, or **Diff vs now**. When an author edits a post you've read, it shows
"edited", with the author's version notes since you read it.

![Versions of someone else's post](screenshots/versions.png)

## Follow the thicket

Blyg posts quote, stub and link each other, and the reader lets you walk
that web without leaving it.

- **Every quoted layer** opens in the reader, quotes inside quotes too. Each
  says whose it is ("quoted from … · v2 · open original"). Clicking the name
  opens the profile, anywhere else in the box the post.
- **A stub's "responds to" line** opens what it answers.
- **An ordinary link to a blyg post** opens it in the reader too. When a post
  opens, Burrow asks your blyg which of its links name blyg posts (the studio
  answers from its own cache), or checks the linked pages itself when it
  can't. Other links open in the browser, so you keep your place.
- **A post from a blyg you don't follow** opens as a peek, read from its own
  blyg with nothing saved, under "You're not subscribed to this blyg —
  subscribe?". That link opens the Subscribe sheet with the address filled
  in.

A breadcrumb above the post shows the path you took; click an earlier entry
to go straight back. **⌘[** (Blyg › Back) or **‹ back** steps back one post
at a time. Opening a post from the stream, the list or a source starts a new
walk.

### Browsing history

**⇧⌘H** (Blyg › Browsing History, or **history** above the post) opens a
pane beside the reader listing this walk and earlier ones, newest first,
indented as the tree they were. Each step opens its post. **clear history**
(click twice) empties it. Esc closes the pane.

The history is private. It's kept in your blyg's own browsing log, so the
web Studio's history pane shows the same walks; with a sign-in that lacks
`reading:state`, it stays on this Mac. It's never published or sent to anyone else.

## Universal quoting

Highlight text anywhere you read, then press **⇧⌘D** (Post › Quote Selection
in Draft):

- in the reading pane or the preview;
- in the built-in [browser](notes-and-browser.md#the-browser-pane);
- in your [notes](notes-and-browser.md#the-notes-drawer).

The passage goes into the draft open in the editor, at the caret, or into a
new draft if none is open, with where it came from:

- a passage from a post on a blyg you follow, quoted into a thread, becomes a
  **partial quote** of that post, `![[id]]` with the passage under it as `>`
  lines (see [Partial quotes](writing.md#partial-quotes));
- anything else becomes a blockquote followed by a link to the page.

Nothing selected? You get a short message, and nothing is made.

**The "Quote in draft" pill.** While you have text selected in the reading
pane or the preview, a small **Quote in draft** pill appears beside the
selection; clicking it does what ⇧⌘D does. The selection stays until you
quote it, and dragging inside a quote no longer opens the original. ⌘C
copies a selection in the reading pane. In the browser, the **→ Draft**
button reads **❝ Quote → Draft** while text is selected. In the stream,
dragging over or double-clicking a post opens it where its text can be
selected.

## Quotes, stubs and forks you can follow

Click a quote's text to open the post it quotes, at the quoted version, or
its author's name for their profile. The "↳ stub of" and "⑂ forked from"
lines under a post work the same way.

Each post lists who quoted, stubbed or forked it, from the blygs you follow
and your own published posts: a list, never a count. A post that both stubs
and quotes another is one response, listed as the stub, and a reply to a
passage says so. A post whose generated text used this one as a source is
listed as **drew on this**.

## Generated text

A blyg can mark text in its posts as AI-generated. In the stream's side pane
and the Reader, marked text is tinted, hovering it says it's marked as
generated, and the post ends with a short note: the mark is the publisher's
own disclosure, and Burrow can't verify it. Text without a mark may have been
generated too; a mark only says what the publisher chose to disclose.

## Lineage

Beside each post's name, in the stream and in Reader, a small **glyph** shows
its lineage: lines coming in on the left are the kinds of post it draws on,
lines going out on the right the kinds that draw on it. Each kind has its own
place (fork at the top, reply in the middle, quote at the bottom) and colour,
and a dotted line means only a passage. Beside it, **2 · 3** says how many:
the posts it draws on, then the posts known here that draw on it. Hover for
the counts by kind in words. These are the same numbers the web Studio's
lineage glyph shows: when your blyg runs blygger-studio with the
`lineage-glyph` extension turned on, Burrow asks it for them; otherwise it
counts what this Mac holds, by the same rules. The lineage glyph is the one
place Burrow shows counts: responses and mentions stay lists.

**⌘J** (Post › Lineage…), or a click on the glyph, opens the lineage view. The
post sits in a hexagon, what it draws on above it and what draws on it below.
The arrow keys move between them, ⏎ makes a neighbour the centre so you can
walk a conversation one step at a time, ⌫ walks back, **o** opens the selected
post in the reader (marking it read, as opening it from the list does), and
esc closes. Beside the hexagon it says how many each way. Responses are what
your blyg knows when it serves the lineage, else what this Mac holds: posts
in your reading list, your own posts, and verified mentions of your posts.

![The lineage view](screenshots/lineage.png)

Press **−** in the lineage view to zoom out to the **neighbourhood**: two
steps each way, what the neighbours draw on and what draws on them. Colours
are the glyph's (fork, reply, quote); a dashed hexagon is two steps away and
a shaded one is yours. The arrow keys move, ⏎ makes a post the centre, ⌫
walks back, **o** opens the selected post, and **+** goes back to the list.
The second step comes from what this Mac holds.

**Space** (or ⏎) on the post in the middle opens the **ring** of the reader's
actions, always in the same places: **F** fork, **R** reply, **Q** quote,
**L** link post, **V** versions, **O** open on the web. A letter previews the
action: a dashed node shows what it would make, and the panel answers the same
four questions for each (is it a response, is the author told, are their
words in yours, does it show under their post). Fork, reply and quote also
say how many of that kind are already known (**R · 2**). ⏎, or the same letter again,
does it; esc backs out. An action that can't be done (a fork of a post with
no pinned version, say) says why.

![The action ring, previewing Reply](screenshots/lineage-ring.png)

## Reading time and inspect

Two bundled [extensions](extensions.md#reading-time-and-inspect), off until
you turn them on, add to each post: **reading-time** ends its byline with
**· 4 min** (the word count on hover), and **inspect** adds **⋯** to its
actions, with the record Burrow holds for it (ids, versions, references,
hashes, and its JSON).

## Mentions and responses

**Mentions** (⇧⌘M) lists the verified responses to your own posts: who
quoted, stubbed or forked which post, and when. You can hide one, and your own
posts list their mentions under them in ⌘Y and in the reading pane.

Your blyg decides whether a post's public page shows its responses. In
**Site settings** (Blyg › Site Settings…), **Show responses by default** sets
it for the whole blyg, on blygs that report it. On the Mentions screen, each
post can **follow site setting** (the default), or **show** or **hide** its
responses itself. Site settings also hold the site's title, author, bio and
links, the time zone for dates on your pages, and **Accept mentions**, on
blygs that report them.

![Mentions](screenshots/mentions.png)

## Your blyg's public site

**Blyg › Site…** sets up the rest of your blyg's public site, as Studio's
settings page does (each section shows when your blyg's studio offers it):

- **Appearance:** the theme your pages use (Auto, Paper, Cream, Slate,
  Solarized Light and Dark, Nord, or your own), with a preview. **Custom**
  is Studio's theme editor: eight colours, a name, and an optional dark
  companion for readers in dark mode, with a warning when text would be
  hard to read. The **typeface** is one of eight that are already on your
  readers' devices. (Burrow's own look is in Settings, not here.)
- **Home page:** newest first or pace-layered, a **front page** (one of your
  published threads in place of the latest items) and the **posts path**
  where the latest items live (`/posts/` unless you change it).
- **Links:** the main menu on every page: where it sits, the threads it
  lists first, your own links, and the standard ones.
- **Profile:** your **avatar** (Upload…), the address your name links to,
  and the blyg's canonical address.
- **Network:** an RSS feed for each public collection (hopper).
- **Updates:** whether your blyg checks for new studio releases. When there
  is one, the status bar says so quietly, with a link to the release notes.
- **Extensions:** your studio's own extensions, on or off.

Nothing reaches your blyg until **Save** (⌘⏎). If your blyg refuses
something, the sheet says why in its words.

## Profiles

⌘I on a post (or a click on its author's address, its "↳ stub of" /
"⑂ forked from" line, a mention, or a blogroll entry) shows who wrote it:
name, bio, links, their blogroll, recent posts, and the blygs they quote,
stub and fork. **Follow** in one click (private; nobody is told), or **add
them to your own public blogroll**. ⇧⌘O opens any address, and Blyg › My
Profile shows yours as visitors see it.

Profiles are fetched from the blyg's public files without your token, only
when you open one, and kept for offline use. There are no follower counts
anywhere.

| | |
|---|---|
| ![A profile](screenshots/profile.png) | ![Your own profile, with blogroll toggles](screenshots/profile-own.png) |
