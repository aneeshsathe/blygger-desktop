# Release notes

What each release of Burrow brings, newest first. After an update, Burrow's
status bar and its "What's new" card link here. Keys are the Mac's; on
Windows, Ctrl stands in for ⌘ (see [Keyboard shortcuts](keys.md) for the
exceptions). The [Changelog](changelog.md) lists every change in detail.

## Burrow 0.27.2

A speed update. Searching from the omnibar reads only which posts match
instead of reading each match in full again, so results come back several
times sooner on a large reading list. Opening an unread post updates just
that post's read mark, instead of rebuilding the whole reading list. Nothing
else changes in how Burrow looks or works.

## Burrow 0.27.1

A security update. Burrow cleans the HTML of posts from blygs you follow
before it shows them, so a post can't run script in the app. The library
that does the cleaning had a flaw that could let script through; this
release updates it, along with the libraries Burrow uses for HTTPS. Nothing
else changes in how Burrow looks or works.

## Burrow 0.27.0

Burrow catches up with blygger-studio 0.64 and 0.65: new posts made from a
thread as you write it, formulas, clearer Background, and links from other
blygs kept apart from responses.

### New items from a thread

In a thread, put `![[+ ` before a passage and `]]` after it, and publishing
makes the passage a post of its own, which the thread then quotes. Write
`[[+ an idea]]` in a sentence, and it becomes a new post the thread links to.
The preview marks each one as a **new item**. Before anything goes out,
Burrow lists the new items and asks: ⏎ publishes them all, the new items
first, and esc goes back to your text. Afterwards your text shows the links
your blyg made, and ⌘Z stops there, so nothing is published twice. In a
fragment, the publish sheet says to press ⌘T for a thread first. See [New items from a thread](writing.md#new-items-from-a-thread-tangling).

### Formulas

Write LaTeX between dollars: `$x^2$` in a sentence, `$$…$$` on lines of
their own. The preview draws them, and shows one that doesn't parse in red;
the publish sheet names it and waits until it's fixed. `$5 and $10` stays money. Select some text
and press **⌃⌘M** to make it a formula, and **⌃⌘B** or **⌃⌘I** for bold or
italic (Edit › Format, or “maths” in the palette; Alt+M, Alt+B and Alt+I on
Windows, where Ctrl+B is still the Shelf and Burrow says so the first time).
In lists, a title with a formula shows its TeX, and in Reading a post of
only quotes is titled “Quotes: …” after what it quotes. Formulas in posts
you read are drawn too. Everything your blyg understands beyond plain
Markdown is now on one page: [Writing syntax](syntax.md).

### Background says why

- Three new rules, in **Settings** (⌘,) on the **BACKGROUND** row: **Parts
  published with a thread**, **Posts with no words of their own** and
  **Republished unchanged**. Turn any of them off to see those posts in your
  lists again. By keys: ⇥ to the rules, ←/→ to pick, space to turn one off
  or on. Each says ✓ when on and ○ … off when off, and a toast confirms.
- A post a rule moved there says why on a small badge, such as
  “published with ‘A morning on the harbour wall’”.
- A thread published with its parts says **+2 parts**: click it, or press
  **p**, to unfold them under its card, and click one or press **1**–**9**
  to open it.

### Links from other blygs

When someone's post links to yours, your blyg now gets a link notice. It
isn't a response, so it never shows on your page or among your responses:
Responses (⇧⌘M) lists it after them, under **Linked from · only you see
these**.

Responses works from the keyboard now: j/k move, **h** hides or unhides,
**r** replies and **o** or ⏎ opens.

### Previews match your blyg

- A quote with a label leads its attribution line with the label.
- A quote inside a quote gets its own smaller line.
- `/* notes to self */` don't appear in the preview, as they never appear on
  the page, and a comment mistake shows as a ⚠ line at the end.

### Ready for studio 0.65 and later

- Burrow asks your blyg "has this changed?" instead of reading lists again,
  so syncs are lighter, and it keeps working when studio 0.67 retires the
  old change counters.
- If your blyg lives under a path (`example.com/blyg/`) and studio 0.65
  refuses an old sign-in, Burrow says why. Sign in again; nothing you wrote is
  lost.

## Burrow 0.26.0

A lighter Burrow: two experiments and the web app are gone, the writing
screen can go quiet, and reading keeps fewer buttons in view.

### Faster sync with long reading lists

With more than 2,000 posts to read, Burrow used to read the newest 2,000
again every 40 seconds and show new posts only after all of them arrived.
Now a sync reads from the newest post down and stops once it reaches posts
it already has, usually one page, and each page shows as it arrives. A full
read still runs now and then, finishes even on long lists, and only then
clears out posts your blyg no longer holds. Feed polls on your blyg no longer
make Burrow read the list again.

Responses from your blyg now arrive compressed (about a fifth of the size),
and the search index is compacted in the background while you're idle.

### Try a theme or font before you keep it

In **Settings** (⌘,), rest the pointer on a theme, font or size, or move
along a row with the arrow keys, and the whole window shows it at once. A
**Preview** at the top of the page shows a post list, a paragraph with a
link and code, a quote and the status bar in what you're trying. ⏎ or a
click keeps it; esc, or leaving the row, puts back what you had. Nothing is
saved until you keep it. The command palette (⇧⌘P) does the same as you
move through its themes and fonts.

The theme row has **For: ☀ Light | ☾ Dark**, so you can choose a dark-mode
theme (`theme-dark`) as easily as the main one; the slot in use is marked.

### Quiet writing

**Settings › Quiet writing** (or View › Quiet Writing, or the palette) hides
the lineage line under the editor, the loose-ends line over the list and
Nearby thoughts, all at once. ⌘J still opens a post's lineage, the search's
`is:` and `has:` filters still work, and ⌥⌘U opens Nearby for this sitting.
It's off unless you turn it on. See [Quiet writing](writing.md#quiet-writing).

### Fewer buttons under a post

The chips under an open post fit on one line again. Quote, Reply, Catch up
and Background stay in view; Hopper, Add to shelf, Add to trail, Fork, Link
post, AI reply, Open on web and → Notes are under **⋯ More**. Every key
works as before (h, t, b, q, ⇧C, ⌥⌘B). The Stream groups them the same
way. Not sure which of shelf, hopper, trail, tray or
Background to use? See [Which one do I use?](reading.md#which-one-do-i-use).

### The author's site fills the reading pane

With **Open posts on the author's site** on, the page now takes the whole
reading pane, under the browser's own bar (back, forward, reload, address,
🛡 and open in your browser), instead of sitting in a box between Burrow's
header and the chips. j/k still move through posts and Space still pages
down. It works on Windows now too, instead of opening your default browser.

### Mark all read, and one unread number

- **Mark All Read…** for the source you're looking at: ⇧R on the reading
  screen, Blyg › Mark All Read…, a source's menu, or the palette. It asks
  first ("Mark 37 posts in Rue read?"), never touches your own posts, and
  syncs like any read mark. With a search, it covers what the search shows.
- The list's header, **All unread** and the status bar now count the same
  thing: posts you haven't read, or that changed since you read them, outside
  Background.
- **Posting from the box in Reading waits 3 seconds**: "Posting in 3 s · esc
  cancels · ⌘⏎ posts now". Esc, Cancel or an edit stops it before anything
  is sent.
- Searching the Stream marks the matches and says how many it found.

### Plainer words

- **Reply** is just Reply: no more "new stub". Responses say "replied to
  this", and a reply's line says "↳ reply to".
- The **Mentions** tab is now **Responses**: who quoted, replied to or forked
  your posts. Mentions is still a source in Reading, for posts that name you.
- One control per post for its responses: **On the post's page: Show · Hide
  · Site default**.
- The loose-ends line says "posts not linked yet" and "threads with outdated
  quotes", explains each on hover, and leaves posts younger than a week alone
  (so does `is:orphan`).
- **⋯ More** shows each item's key. The shelf is "Add to Shelf / Remove from
  Shelf" everywhere. Discover says "high activity"; Stale Quotes names the
  quoted post instead of an id.

### Getting started

- The Welcome card says what a blyg is and where to get one, and that you
  can try Burrow on sample data first.
- If you skip setup, the empty list offers **⏎ Connect your blyg… · try
  sample data**, "not connected" in the status bar opens Connect, and
  **Burrow › Connect…** is always in the menu.
- Connect states the blygger-studio version Burrow really needs (0.49.12 or
  later).
- Your first publish says the post goes public at your blyg's address, and
  that published posts can be withdrawn, not deleted.
- The Reading tour has a step for **⋯ More**.

### On Windows

Nine shortcuts typed characters on German, Polish, French or Hungarian
keyboards (Ctrl+Alt is AltGr there), so they move: Quote into a thread is
**Ctrl+Shift+Q**, Link post **Ctrl+Shift+I**, the quote tray
**Ctrl+Shift+T**, Nearby **Ctrl+Shift+U**, Stream **Alt+1**, Reader
**Alt+2** and the sources pane **Alt+S**. Hints and messages that showed Mac
key symbols now spell Windows keys.

### Removed

- **#thicket chat** is gone: not enough people used it.
- **The terminal pane** is gone until it's ready.
- **The web app** ("Burrow in a browser") is gone: it wasn't working well.
  The desktop app on Mac and Windows is Burrow. Releases no longer include
  `Burrow-web.zip`, and the page on serving it from your blyg is gone.
- If your [config file](config.md) still sets `chat-tab`, `terminal` or
  their other keys, Burrow ignores those lines quietly.

## Burrow 0.25.1

A quieter, quicker release: Burrow does less on the UI thread while you read,
and a few display bugs are fixed.

### Snappier reading

When your blyg syncs many things at once, the reading list refreshes once for
the whole burst instead of once per change. The ⌘Y history pane no longer
reads the database while it draws; it refreshes in the background when the
post or its responses change.

### Fixes

- In the ET Book theme, › and ‹ showed as "ffl" and "ffi" (in "Hopper ›",
  menus and elsewhere). The bundled ET Book fonts had the ligatures in those
  characters' places; they now draw the right marks.
- The Writing tour's shelf step could ring nothing if you moved on from
  "Back to writing" with Next. The step now brings the list back itself.

## Burrow 0.25.0

Mentions that say why they're there, Settings with room to breathe, and a web
app that feels like an app: it fills the page and steps from post to post.

### Mentions, and why

**Reading › Mentions** lists the posts that point at you: the ones your blyg
says name you (an @-mention), and the ones Burrow finds itself that quote,
reply to, fork or link to your blyg. A tag on each row says which: *named
you*, *quoted you*, *replied*, *forked you* or *linked you*. The list no
longer shows everything for a moment and then shrinks to a few: your blyg's
last answer is kept, so it's whole from the start. The web app lists and tags
them the same way.

### Settings is a full page

⌘, opens Settings over the whole window instead of a pop-up, and every row of
options and every explanation wraps to fit, at any window width and font.
Esc or ⌘, closes it.

### The web app

- **Post to post.** An open post has **‹ ›** for the previous and next post,
  in the list as you saw it. On a phone or an iPad, swipe sideways across the
  post: left for the next, right for the previous. ↑ ↓ on a keyboard do the
  same.
- **It fills the page** on a laptop or iPad, rather than sitting in a window
  drawn inside the page. Theme and layout are in the title bar, and **Keys**
  lists the shortcuts.
- **On a phone, swiping back from a post** no longer plays the slide twice.

## Burrow 0.24.0

Set a post aside without unfollowing anyone, keep anyone's posts on a shelf
that follows you from the desktop to the studio and the web app, and find
new blygs to read. All three match blygger-studio 0.61.

### Background and Foreground

- **↓ Background** under an open post (or **b**) moves it out of every other
  list: the stream, the feeds, Mentions and the counts. Nothing is marked
  read. **↑ Foreground** brings it back, **↺ Clear my choice** lets the rules
  decide again, and **⇊ Blyg to Background** moves a whole blyg.
- The **Background** list is now your blyg's own, the same one the studio
  shows: what you moved there, blygs you moved there, withdrawn posts and
  your own minor changes. **Settings › Reading › Background** turns the
  minor-change rule off or on.
- The web app has the same chips under an open post, and the rules in
  ⚙ Reading. See [Background and Foreground](reading.md#background-and-foreground).

### The shelf syncs, and holds anyone's posts

- With a blyg on studio 0.61, the shelf syncs: the studio, the web app and
  your other computers share it, and a change made in two places at once is
  merged rather than lost.
- **+ Add to shelf** among a post's chips in Reading puts someone else's
  post on it too, and the new **Shelf** lens in Reading's sources lists the
  shelf in your order. A shelved post stays in Shelf even in Background. See
  [The shelf](writing.md#the-shelf).

### Discover blygs

- **Blyg › Discover Blygs…** (or **Discover…** on the Subscriptions screen)
  lists the blygs on blygger.com's public directory that you don't follow
  yet, newest first, with how active each is. Ones listed since you last
  looked say **new**, and the studio agrees. ⏎ subscribes. Desktop only.
  See [Discover blygs](reading.md#discover-blygs).

### Also

- The Reading tour has steps for Background, adding to the shelf, the Shelf
  lens and Discover, and What's new runs them after the update.
- **Fixed:** the tour card no longer covers the Catch up sheet, and Burrow
  reads the Background list again only when it can have changed.

## Burrow 0.23.0

Catch up on a conversation without reading it backwards. The web app gains
the studio's Mark all read, Posts filters, Updates and Access tokens, shows
your own posts in Reading, keeps Reading in order and up to date while it's
open, and starts faster than the studio.

### Catch up

- **The conversation, oldest first.** A post that quotes a post that quotes
  another is a conversation. Press **⇧C** on the reading screen (or click the
  **Catch up** chip under the post, or Post › Catch Up…, or ⇧C in the lineage
  view) and a sheet lays it out from the start: each post's author, date and
  own words, and how the next one uses it.
- **j/k** move, **⏎** opens a post in the reader, and **w** adds the side
  replies you hold (Whole conversation). Posts this Mac doesn't hold show
  what was quoted of them, and nothing is fetched until you open one.
- **s** asks your AI provider for a short summary, in order and by author,
  shown at the top of the sheet as **AI summary**. It's for you: it never
  goes into a post.
- The Reading tour has a step for it, on the sample conversation. See
  [Catch up](reading.md#catch-up).

### The web app

- **Mark all read**: ✓ Mark all read… in the sources pane (⌃⌥A), or ✓ All
  read on a phone, marks every unread post of the source you're looking at,
  after asking.
- **Filters in Posts**: All · Drafts · Changes · Public · Withdrawn, with
  counts, as the studio's compose page has them (⌃⌥F steps through them).
- **Updates** (⚙ Blyg › Updates): your threads whose quotes have newer
  versions, stalest first, and Refresh, one or all.
- **Access tokens** (⚙ Blyg › Access tokens): what can act as you on your
  blyg, a manual token for a tool (shown once), and revoke one or all.
- **Your own posts in Reading**, as on the desktop: never unread, and nothing
  sent about them.
- **Fixed:** Reading came out shuffled after a big first load; it's now in
  the same order as the desktop and the studio. Posts changed elsewhere show
  within about 15 seconds while the tab is open. And Reading's first page
  shows as soon as it arrives, in about a third of a second with 3,000
  posts to read.
- Some of these need a recent studio; until yours offers them they stay
  hidden or say why. See Using it.

### For contributors

- **Development skills** in `skills/`: plain-Markdown checklists for the
  release, the tour, Windows and macOS, the pre-PR gate and more, for any
  coding agent or person. Thanks to @patwater for the idea.

## Burrow 0.22.0

The browser pane and cross-post now run on Windows. Links in posts open
beside your reading there too, sign-ins stay in the pane's own profile, and
the bundled cross-post posts to Substack Notes, Bluesky and Farcaster from
it, after the same two checks as on the Mac.

### Browser pane and cross-post on Windows

- **Links open beside your reading.** A link in a post opens in the pane, a
  WebView2 with its own profile, so you stay signed in to the sites you use
  across relaunches, apart from the preview. Ctrl+click opens it over the
  whole reading area, Alt+click goes to your default browser, and
  Ctrl+Shift+B hides it and brings it back. The pane's other keys are the
  Mac's with Ctrl.
- **cross-post works there too.** Turn it on as on the Mac: each site is its
  own permission, the palette's **Open …** rows let you sign in by hand, and
  nothing posts until you click Post on the second sheet. If a site's
  composer won't take the text, it's left on your clipboard for you to paste.
- **Not on Windows yet:** ad and tracker blocking (🛡 is greyed out) and
  "Open posts on the author's site".

## Burrow 0.21.0

Read a blyg as its author styled it, list where else your posts appear,
cross-post to Bluesky and Farcaster, and sync with far less load on your blyg.
Protocol 0.4's latest revisions are in too. The tour is now a quick tour of
the essentials plus nine feature tours, picked from its first screen, and
What's new runs just the new steps.

### Reading

- **Open posts on the author's site.** Turn it on in Settings › Reading, and an
  opened post shows its own page, styled by its author, where Burrow's reader
  was. j/k still move, Space pages down, and the post still counts as read.
  **Burrow reader | Author's site** above the post switches that one view, and
  a subscription's menu can always or never open its posts this way. Posts
  without a page of their own stay in Burrow's reader. The web app has the
  same choice under ⚙ Reading.
- **Changelogs.** An opened post can show its latest note, its full history,
  or an AI summary of its notes (Settings › Reading › Changelog). A summary is
  written only when you open the post and is labelled as the model's, not the
  author's words. Needs blygger-studio 0.58 or later.
- **Also posted at.** When another blyg lists where else a post appears, the
  reader shows plain "Also posted at …" links. Burrow never fetches them.

### Writing and cross-posting

- **Copies elsewhere.** Post › Copies Elsewhere… lists where else a published
  post appears, saved on your blyg without a new version. It appears once your
  blyg's studio supports it.
- **Bluesky and Farcaster.** The bundled cross-post can now post to Bluesky
  and Farcaster as well as Substack Notes, each with its own permission. The
  copy's link goes on the post's Copies Elsewhere list, or one click on
  **Record copy** adds it. Thanks to @patwater ([#69](https://github.com/aneeshsathe/blygger-desktop/pull/69)).
- **Typed `[[` stays text.** `bracket-picker` now defaults to `quotes`: only
  `![[` at the start of a line opens the picker, and ⌘K always does. Set
  `bracket-picker = on` for the old behaviour.
- **A fragment can't quote.** A `![[…]]` line in a fragment is refused at
  publish, with a note to quote from a thread or put the line in code.
- Burrow remembers the extension permissions you decline, and asks again only
  when an extension asks for something new.

### Sync and the protocol

- **Less load on your blyg.** A thumb or hopper change on another device no
  longer re-reads your whole reading list, and the background full sync backs
  off to every 2 hours while nothing changes. On Cloudflare's free D1 plan, add
  `sync-cadence = low` to your [config file](config.md). When the blyg returns
  a server error, Burrow backs off and says "blyg unavailable" quietly.
- **Unicode.** Text you write is saved clean and composed (NFC), shortened
  text never cuts a character in half, international blyg addresses show
  their `xn--` form, and quotes from right-to-left text can't reorder the
  words around them. A new `lang` setting marks the preview's language.

## Burrow 0.20.0

Your notes stay open while you write, the panes beside the editor resize, and
the web app shows its features instead of hiding them. The tour's new step
starts at the side tabs.

### Writing beside your notes

- **Notes stay open.** The notes are a pane at the window's right edge, beside
  the editor, not an overlay: clicking or typing in your draft no longer
  closes them. ⇧⌘N moves into the notes, esc goes back to your draft and
  leaves them open, and × or ⇧⌘N from inside closes them. Quote into post and
  Copy verbatim still land at the caret.
- **Side tabs.** A slim bar at the right edge has a Notes tab and a Nearby
  tab. Nearby thoughts is now a drawer too: closed until you open it (its tab,
  or ⌥⌘U), and open until you close it. Focus mode hides the bar.
- **Resize.** Drag the line between the editor and the ⌘E preview, or a
  pane's edge. A double-click resets it. Burrow remembers the sizes, and
  whether the notes and Nearby were open.

### Burrow in a browser

- **Nothing hides in a corner.** On a phone, a fourth tab, **Blyg**, lists
  Settings, Mentions, Hoppers, Subscribe, Subscriptions, Peek, History, your
  shelf and the theme. The Read tab has named buttons under its search, and a
  post you're reading has Quote, Reply, Hopper, Copy link, Unread and Fork in
  a row. On a desk, a **Blyg** view (⌃3) holds the same list, and an action
  row over the editor names what you can do with the post.
- **Serve it from your own blyg.** Each release now includes the web app as
  `Burrow-web-<version>.zip`, covered by the signed checksums.
  Burrow on your own blyg walks you, or your AI agent,
  through putting it at `/burrow/` on your blyg.

## Burrow 0.19.0

Burrow now does what Studio does, so you can run your blyg from Burrow alone,
on your Mac or PC and in a browser. The tour's new steps start at Blyg › Site….

### Running your blyg (Blyg menu)

- **Site…**: your blyg's public pages. Pick the theme (one of Studio's, or
  design your own with a live preview and contrast warnings), the typeface,
  a front page, the home page's order, the posts path, the menus on every
  page, your avatar and addresses, collection feeds, and which of Studio's
  extensions are on. When your blyg's studio has an update, the status bar
  says so. Nothing changes on your blyg until you press Save.
- **Contacts…**: your blyg's address book. Search, add, edit, check and
  remove people. Post › Add Author to Contacts… adds whoever wrote the post
  in front of you, and subscribing to a blyg offers to add it. An `@handle`
  in the preview shows as the link your blyg publishes.
- **Client Access…**: every app and token that can act as you on your blyg,
  Burrow included. Make a token (shown once) or revoke any of them.
- **AI on Your Blyg…**: your blyg's own AI (a model per job, house style,
  automatic change notes), apart from Burrow's own AI in Settings › AI.
  When it's set up, the publish sheet drafts the version note (⌥⌘N) and
  says whether the change looks like a typo, minor or major. You edit and
  confirm both.
- **Stale Quotes…**: threads whose quoted posts have newer versions are
  marked ↻ in Posts; this sheet refreshes their quotes. `is:stale-quote`
  finds them in the search.
- **Signals…**: your likes, dislikes and private activity log, with no counts.

### Reading and writing

- Mentions has **Received** and **Sent**: the Webmentions your blyg sent and
  where each one stands.
- Subscriptions show when each was last polled and why one is failing.
- New smart feeds: **Thumbed down**, **Threads** and **Fragments**.
- ⌘Y shows **Changes from vN**: a word diff between versions of your post.
- A pasted or dropped image asks for its **alt text**. **Post › Copy Link**
  copies a post's address.

### Burrow in a browser

The web app does most of what Studio does, and stays fast on a phone. It runs
from your own blyg at `/burrow/`; Studio doesn't serve it yet, so
Burrow on your own blyg shows how to add it:

- **Reading**: sources (smart feeds, Mentions, hoppers, subscriptions) with a
  Threads / Fragments lens; subscriptions (subscribe with a preview, pause,
  rename, check now, unsubscribe, OPML import and export); mentions received
  and sent; hoppers (make, rename, describe, public or private, delete);
  👍 / 👎; Peek at a post from a blyg you don't follow; your browsing history.
- **Writing**: images with alt text (photo picker, paste, drop); versions (as
  published, a word diff, restore, pin); withdraw; fork a pinned version; TK
  generation; the publish sheet's drafted note and kind of change; your
  blyg's settings; your contacts in the `@` list.

## Burrow 0.18.1

A patch release: fixes, nothing new to learn.

- What's new after an update is now this page. The card that used to list everything had grown too long to fit (and didn't scroll); it now links here, and so does the status bar's "What's new".
- Nearby thoughts: ⌥↑/⌥↓, ⌥⏎ (link) and ⌥⇧⏎ (quote) keep working after you click a row in the rail. Rest the pointer on a row to see the post. The rail says when it's matching shared phrases only and why, and tries the model again while you write.
- Burrow in a browser is fast with thousands of posts to read and no longer heats a phone: only the screen you're on is drawn, lists load a page at a time as you scroll, and a refresh reads only what changed. Opening a post on a phone no longer goes blank after a second.
- Burrow in a browser works on a blyg served under a path (`{mount}/burrow/`), not only at the host root.

## Burrow 0.18.0

- The shelf: ⌥⌘B keeps a post you keep updating (a /now page, a reading log) at the top of the list, marked 📚. ⌘B opens the shelf; 1–9 opens a post on it, ⌥↑/⌥↓ reorder it. It's kept on this Mac for now.
- Lineage while you write: a quiet line under the editor shows the open post's lineage glyph, what it draws on and what draws on it. A click opens the lists; ⌘J opens the lineage sheet for your own post.
- The lineage sheet zooms out: − shows the neighbourhood two hops each way, coloured by relation. ⏎ re-centres on a post, ⌫ walks back, + returns to the list.
- Hover cards: rest the pointer on a [[…]] or ![[…]] in the editor (or a quote or link in the Stream) to see what it is; “label it” names an unlabelled link. ⌘-click opens the post.
- #thicket (⇧⌘I), an experimental chat tab: talk with other Burrow users on Libera.Chat (IRC), with unread and mention marks and a notification when you're mentioned. Nothing connects until you set it up, Settings hides it, and it may be removed.
- A terminal pane (⌃`, experimental): your own shell under your posts, for a CLI agent, git or wrangler. It gets nothing of Burrow's (no token, no BLYGGER_* variable); ⇧⌘T sends the editor's selection to it, quoted.
- The command palette (⇧⌘P): every action, theme, font, blyg, hopper and post by name, each with its key; ⏎ runs it, ⌘⏎ keeps the palette open. Extensions are its Extensions group, and Post › Extensions… still lists them.
- Focus mode (⌥⌘F, View › Focus): just the editor, the line you're on held steady, other paragraphs dimmed, a hairline with the sync dot and this sitting's words. esc leaves; focus-line and focus-fade tune it.
- Serendipity, off unless you turn it on (Settings › Serendipity): ⌘N's empty editor shows one of your older posts; q quotes, l links, o opens, r another, and typing sends it away.
- Small motion: a published post's row settles, the sync dot and the counter ease between colours, the list crossfades when you switch blygs (none of it with Reduce Motion on). sounds = quiet adds a quiet system sound on publish and capture.
- Trails: t in Reading adds the post to a trail, with a word on why; ⇧T starts another. Trails are listed with your sources, and ⌥⌘T writes one as a thread of quotes in a new draft. Nothing is published.
- Drag a post into what you're writing to quote it (a link in a fragment), or onto the omnibar to start a thread. q or ⌥⌘K keeps it on the quote tray above the status bar, and ⌘K lists the tray first.
- Split view: ⌥⌘⏎ opens the selected post beside the one you're writing (yours editable, anyone else's read-only); ⌥⌘[ and ⌥⌘] move between them.
- Nearby thoughts: a rail beside the editor shows your posts, and posts you hold, that say what your paragraph says. ⌥⏎ links the shared phrase, ⌥⇧⏎ quotes it; ⌥⌘U hides the rail. Settings › Nearby adds a 31 MB model that runs on this Mac and matches meaning, not just words.
- Loose ends: the search reads is:orphan, is:draft, is:stale, is:leaf, has:tk, links:, quotes:, kind: and v:. With the search empty, a quiet line over the list says what's loose, in words (2 orphans · 1 ungenerated TK); a click filters.
- Capture from anywhere: select text in any app and choose Services › Capture in Burrow to open quick capture with it quoted. A web address pasted into the search starts a fragment with that link, titled from the page.
- Your contacts come first in the @ list; where your blyg turns @handle into a link (studio 0.56), picking one writes @handle, and publish says which handles it couldn't resolve. Reading has a Mentions feed under All unread.

## Burrow 0.17.0

- Burrow stays quick with thousands of posts in Reading: a sync that finds nothing new no longer holds up the window, and lists, search and marking posts read are many times faster.
- Pick several posts and press h to put them all in a hopper at once. A hopper lists its posts in its own order, the most recently added first.
- The reader follows Burrow's theme, not the Mac's. bracket-picker = quotes (or off) in the config stops typing [[ from opening the post picker.
- Switching blygs is safer: nothing from the blyg you left reaches the one you're on, and the switcher keys work on Windows.

## Burrow 0.16.0

- Burrow now needs blygger-studio 0.49.12 or later. On an older blyg, nothing syncs until it's updated; your posts stay on this computer and are sent afterwards.
- A post edited in two places is never overwritten: when it changed on your blyg too, Burrow shows both copies and you choose which to keep, or keep both.
- Long reading lists come in pages, and nothing is removed from this computer until the whole list has been read.

## Burrow 0.15.0

- Hoppers: your blyg's collections of posts, yours and others', are listed under Hoppers in the Reader's sources. Pick one to list its posts.
- h on the reading screen, ⌥⌘P, or Hopper › on a post puts it in a hopper or takes it out; New Hopper… makes one and adds the post. Adds made offline are sent later.
- Right-click a hopper to rename it, describe it, make it public (it gets a page on your blyg) or private, or delete it. Deleting a hopper never touches its posts.

## Burrow 0.14.0

- Several blygs: click the blyg's name in the title bar, or press ⌃⌘0, to switch, add one or sign out of one; ⌃⌘1–⌃⌘9 jump straight to one. Each keeps its own posts and reading.
- Burrow opens on Reading now. ⌘R goes to your posts and back.
- Write while you read: the box at the foot of Reading posts a fragment (⌘⏎) or keeps a draft (⌘D). c or ⇧⌘K puts the caret in it; esc keeps your text.
- A revised post leads with what changed in the stream: “V3: …”, from its author's changelog.
- Follow the thicket: in the reader, a quote, a stub's “responds to” line or a link to a blyg post opens that post. The breadcrumb shows the path; ⌘[ steps back.
- ⇧⌘H lists where following links has taken you. It's private, and never published.
- Withdrawn posts are listed under Background in the Reader, and nowhere else.
- A stub's passage is chosen in the preview: select it inside the quote, then ❝ quote only this or ⇧⌘D. The separate chooser is gone.

## Burrow 0.13.0

- Links have labels: [[ and ![[ write [[id | label]], the post's title (or its heading). The label is the link's text on the page, so edit the words after | to rename it; a quote's note is yours only. Rows lead with the title.
- Discard changes: a published post with unpublished edits goes back to the version it published with ⌥⌘Z, Post › Discard Changes…, the toolbar's eraser or ⌘Y. It asks first; the public post never changes.
- Your own posts: Your blyg is a Reading source, never unread or counted. A published post in Posts offers Reply · new stub, Quote into a thread and Link post above its text; Fork this pin in ⌘Y forks one.
- Labelled links need blygger-studio 0.47. On an older blyg only posts with them wait: Burrow says “Your blyg needs updating”, links to how, and publishing offers to write the links bare.

## Burrow 0.12.0

- Burrow works with a stock blyg: an unmodified blygger-studio 0.40 or later, no server patches. Reading, read marks, lineage and citations all sync.
- Sign in with browser, or with a token from Studio › More › Client access (allow reading:state so read marks sync). The old server owner token is gone from sign-in.
- If your blyg refuses Burrow's sign-in, Burrow stops syncing, keeps every change waiting, and opens the sign-in sheet to say why.
- Publishing warns when generated text left a ![[…]] on its own line that its instruction didn't ask for: it would quote that post and notify its author. The preview shows the quote.
- In the Reader, text a blyg marks as generated says, on hover and at the end of the post, that the mark is the publisher's own disclosure, not verified.
- A post that drew on yours for generated text is listed as “drew on this”.

## Burrow 0.11.0

- reading-time and inspect, two bundled extensions (off until you turn them on): “· 3 min” on each post you read, and ⋯ › inspect for the record behind it.
- The lineage glyph, ⌘J and the ring now count: how many posts a post draws on, and how many draw on it.
- Bring your feeds from another reader: Blyg › Import Subscriptions from OPML… (and Export). They land in an Imported feeds folder in the Reader.
- markdown-notes takes several folders, and none until you choose one: Add folder… in the drawer's Notes tab, or Settings › Notes folders.
- cross-post runs in the side pane, and you can fold the pane mid-run: the status bar says “macro running · show pane”.
- ⇧⌘P › Manage extensions can Turn off an extension or Forget its permissions; ⌘⌫ turns one off from the permission sheet.
- Sign in again once after this update so read state keeps syncing between your Macs (studio 0.39 or later): Burrow › Disconnect…, keep the local copy, then Sign in with browser. An API token needs a new one with reading:state.

## Burrow 0.10.0

- Extensions: ⇧⌘P lists what they offer; Burrow asks before one may do anything, and none can publish or see your sign-in.
- markdown-notes opens a folder of Markdown notes in the Notes drawer: search, edit, and quote a note into your post.
- cross-post puts a published post on Substack Notes from the browser pane, after you check the text twice.
- ⌘N opens an empty editor; it's kept on this Mac until ⌘D makes it a draft.
- ✂ Clip (⇧⌘C) in the browser pane quotes the page into a draft.

## Burrow 0.9.0

- Sign in with your browser: Burrow asks your studio for access and renews it.
- In the Reader, ⌘-click, ⇧-click and ⌘A pick posts; r and u mark them read or unread.
- Reply starts a stub quoting the whole post; choose a passage in the stub with quote a passage instead.
- [[ and ![[ open a full-text picker over your posts and the ones you read.
- Rename a subscription, or hand it back to its own name; ⇧⌘R checks every feed.

## Burrow 0.8.0

- Lineage: a glyph beside each post's name shows which kinds of post it draws on and which draw on it (fork, reply, quote).
- ⌘J opens a post's lineage: what it draws on above, its responses below. Walk it one step at a time with ⏎ and ⌫.
- Space on the post in the middle opens its actions on a ring: fork, reply, quote, link post, versions, open.
- One response per post: a reply that also quotes is listed once, as a reply, and a reply to a passage says so.

