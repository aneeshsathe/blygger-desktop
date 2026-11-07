# Release notes

What each release of Burrow brings, newest first. After an update, Burrow's
status bar and its "What's new" card link here. Keys are the Mac's; on
Windows, Ctrl stands in for ⌘ (see [Keyboard shortcuts](keys.md) for the
exceptions). The [Changelog](changelog.md) lists every change in detail.

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

The web app (at `/burrow/` on your blyg) does most of what Studio does, and
stays fast on a phone:

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

