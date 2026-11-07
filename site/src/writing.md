# Writing and publishing

## Search is the interface

The omnibar filters your posts as you type. ↑/↓ previews each post, ⏎ opens
it, and ⏎ with no match starts a new draft seeded with what you typed. ⌘L
comes back to the omnibar from anywhere, and so does esc from the editor.

**Loose ends.** The search also reads a few operators, alone or with words:

| Operator | Lists |
|---|---|
| `is:orphan` | published posts nothing points at: no post you read quotes or replies to it, no verified mention, and none of your own posts links to it |
| `is:draft` / `is:stale` | drafts / drafts untouched for a month |
| `is:leaf` | posts that quote nothing (no `![[id]]`, reply or fork) |
| `has:tk` | posts with a `[TK]` not generated yet |
| `links:<id or host>` | posts that link to that post (the first characters of its id do) or to that site |
| `quotes:<id or host>` | posts that quote, reply to or fork it |
| `kind:thread`, `kind:fragment`, `v:2` | by kind, by published version (`v:0`: never published) |

`is:draft tide` lists drafts that mention tides. ⏎ never starts a post from
operators alone. With the search empty, a quiet line over the list says
what's loose, in words ("2 orphans · 1 ungenerated TK"); click a part to list
those posts.

**A web address.** Paste one into the search and, when no post has it, ⏎
starts a fragment with that link, titled from the page (fetched once, without
your token). An address of a blyg post you hold starts a thread quoting it,
`![[id]]`, instead.

**⌘N** (Post › New Post) opens an empty editor with the caret in it. Your
search and the list stay as they were. What you type is a **scratch note**,
kept on this Mac and saved as you go; the status bar says "New note · saved
locally · ⌘D draft · ⌘⏎ publish". ⌘D makes it a draft on your blyg, and ⌘⏎
publishes it. If you leave without typing anything, nothing is kept. With
`new-note = draft`, ⌘N starts a draft instead.

Posts are **plain Markdown**, and there's no save button. Every keystroke is
saved on your Mac and synced shortly after you stop typing. A post's title is
its first line of text (a leading heading is the title), never a quote it
starts with.

### The shelf

Some posts you keep coming back to: a /now page, a reading log, a running
list of links. Put them on your **shelf** so you never have to search for
them. **⌥⌘B** (Post › Put on Shelf / Take Off) shelves the open post, or
takes it off. Shelved posts are marked 📚 and stay at the top of the list, in
your order, above a thin rule. When you search, the shelved posts that match
come first.

**⌘B** (Post › Shelf…) opens the shelf: **1–9** or ⏎ opens a post on it,
**⌥↑/⌥↓** reorder it, **⌫** takes the highlighted post off, and esc closes
it. Any post of yours can go on the shelf: a thread, a fragment, a draft or a
scratch note, up to 50 of them. A withdrawn post leaves it.

The shelf is kept on this Mac (on Windows, this computer; the keys are
Ctrl+B and Ctrl+Shift+A). It's not the same as pinning a version in ⌘Y, which
is public and permanent.

### Lineage while you write

Under the editor, one quiet line shows what the open post is part of: its
lineage glyph, how many posts it **draws on** (quotes, the post it replies
to or forks) and how many known here **draw on it**, and how many posts it
links to. A draft's line follows its text as you add or remove a quote. Click
the line to see the posts by name; click one to open it, yours in the editor
and anyone else's in Reading. **⌘J** (Ctrl+J on Windows) opens the full
lineage view for a published post, as in Reading; on a draft it opens the
lists.

### Nearby thoughts

A rail on the editor's right shows up to five of your own published posts,
and posts you hold from blygs you follow, that say what the paragraph you're
writing says, with the phrase you share (or the closest passage) under each.
It looks again a moment after you stop typing, or when you move to another
paragraph.

- **⌥↑ / ⌥↓** choose a row without leaving the text.
- **⌥⏎** links it: the phrase you share becomes `[[id | phrase]]` in place.
  When the match is by meaning rather than a phrase, `[[id | title]]` goes
  after the word at the caret.
- **⌥⇧⏎** quotes it as `![[id | title]]` on a new line after the paragraph
  (threads only).
- **⌥⌘U** shows or hides the rail.

On Windows the keys are Alt+Up/Alt+Down, Alt+Enter, Alt+Shift+Enter and
Ctrl+Alt+U. Posts you already link or quote aren't offered again, and
withdrawn posts never are. Nothing is fetched to make a suggestion: only
what this computer already holds.

Out of the box the rail matches **shared phrases**. **Settings › Nearby** can
add a small model that runs on your computer and matches **meaning**, so a
post about "the shore" turns up while you write about "tide pools":
potion-base-8M (MIT), about 31 MB, downloaded once and checked, then used
offline. Nothing you write leaves your computer with it. You can instead
point `nearby-endpoint` at an embeddings server you run (Ollama, or anything
OpenAI-compatible), and `nearby-rerank = on` lets your AI provider put the
best ten in order. Both of those send text out; see
[AI helpers](ai.md#nearby-thoughts-what-is-sent-where). `nearby = titles`
matches titles only, and `nearby = off` turns the rail off. See
[Configuration](config.md).

### Hover cards

Rest the pointer on a `[[…]]` or `![[…]]` in your text and a card says what
it points at: the title, who wrote it, the version and its first lines. A
link without a label offers **label it**, which writes the post's title in as
the label. **⌘-click** (Ctrl-click on Windows) opens the post. A withdrawn
post says so. The card only shows what this Mac already holds.

### Serendipity

Off unless you turn it on: Settings › Serendipity, or `serendipity = own` in
the config file. Then ⌘N's empty editor shows one of your own published
posts, more often one nothing links to yet, or one from this week in an
earlier year, and never the same one twice in a month. While the editor is
still empty, **q** quotes it into the new post, **l** links to it, **o** opens
it and **r** shows another; the first thing you type sends it away.
`serendipity = on` also offers posts from the blygs you read.
### Two posts side by side

**⌥⌘⏎** (Post › Open Beside) opens the selected post beside the one you're
writing: a reply and its source, two drafts that should be one, a fork and
its original. Your own post opens in a second editor that saves on its own
and has its own status line. Anyone else's opens read-only; drag its header
into your post to quote it. **⌥⌘[** and **⌥⌘]** (View › Left Post / Right
Post) move between them, the divider drags (Burrow remembers where), and ×
or ⌥⌘⏎ again closes the right side. From Reading, ⌥⌘⏎ goes to Posts with
the post you were reading beside the one you were writing.

On Windows the keys are Ctrl+Alt+Enter and Ctrl+Alt+[ / ].

## Fragments and threads

A blyg holds two kinds of post:

- a **fragment** is short: up to 1000 characters. A live counter in the
  status bar turns amber past 900 and red past 1000;
- a **thread** is long, with no limit, and can quote other posts.

⌘T switches a post between them (or click `◦ fragment` / `≡ thread` in the
status bar). An over-long fragment can't be published: the status bar says
"Too long for a fragment · ⌘T makes it a thread". The AI helper **Shorten to
fit 1000** (⇧⌘G) is the other way out; see [AI helpers](ai.md).

![A fragment over the limit](screenshots/over-limit.png)

## Publishing and versions

⌘⏎ publishes. A sheet drops from the title bar, "Publish … as v2", with an
optional **version note**; ⏎ publishes and esc cancels. A toast shows the
result and its address, and ⌘O opens the post on the web.

| | |
|---|---|
| ![The publish sheet](screenshots/publish-sheet.png) | ![Published](screenshots/published-toast.png) |

Every publish makes a new public version (v1, v2, …); editing a published
post leaves it `public v3 · unpublished edits` until you publish again, and
its row in Posts gets the edit mark. Hover either to see both ways out: ⌘⏎
publishes the edits as v4, ⌥⌘Z discards them. The published and pinned marks
say what they mean on hover too.
### Your blyg's AI in the publish sheet

When your blyg has its own AI set up (**Blyg › AI on Your Blyg…**), the
publish sheet for a second version on offers two helpers. Neither publishes
anything, and neither holds publishing up:

- **Draft a note** (⌥⌘N, or Post › Draft a Change Note): your blyg's
  change-notes model writes the version note from what changed and puts it in
  the field, to edit or keep. With "Draft automatically" on, an empty note is
  drafted as the sheet opens. A note published exactly as drafted is marked
  as generated in the changelog; edit it and it's yours.
- **The change classifier**: your blyg labels the change *typo*, *minor* or
  *major*, quietly under the field. Click another label to change it, or the
  chosen one to clear it. With "Start the note with it" on, the label leads
  the note (`minor: …`) in the field, where you see the note as it will
  publish. The label is kept with the version on your blyg, privately:
  readers see it only as words in your note.

On Windows the key is Ctrl+Shift+E.

**Versions** (⌘Y) lists every version of your post with its note. Open any of
them and **Restore** it into the editor; publishing it makes a new version,
since versions never go backwards.

### Discard changes

Changed your mind about the edits to a published post? **Discard changes**
puts the editor back to the version your blyg has published. Use ⌥⌘Z,
**Post › Discard Changes…**, the toolbar's eraser button, or **Discard
changes…** on the current version in ⌘Y. A sheet asks first ("Discard
unpublished changes? … The editor goes back to v3 as published. You can't
undo this."): ⏎ discards, esc keeps the edits. Then a toast says "Back to v3
as published".

The public post doesn't change: only your unpublished edits go. Discard is
unavailable, with the reason on hover, for a draft or scratch note (delete
those instead), a withdrawn post, a post with no edits, and while you're
offline, since it needs the blyg. If v3 disclosed generated text, the blyg
keeps the post marked unpublished after a discard, and the toast says so: ⌘⏎
publishes it again as v4.

On Windows Discard changes has no key (Ctrl+Alt+Z is AltGr+Z on some
layouts): use **Menu › Post › Discard Changes…** or the toolbar.

A version can be **pinned**: a pin is served unchanged forever, and it's what
other blygs can fork. Pinning can't be undone, so it asks you to type to
confirm.

![Versions of your own post](screenshots/versions-own.png)

### Delete and withdraw

Drafts and scratch notes can be deleted: ⇧⌘⌫ (Post › Delete Draft…) asks
first. **Published work is never deleted.** Post › Withdraw… withdraws a
published post instead, with an optional note. Withdrawn posts stay listed as
withdrawn, and you can't undo it, so Withdraw has no key on purpose.

### Edited in two places

Every change Burrow sends names the revision of the post it last saw, so
nothing on your blyg is ever overwritten by mistake. If a post changed on your
blyg as well (in the studio, or from another computer):

- and you haven't edited it here, Burrow stops the change and shows the blyg's
  text, so you see what changed before you act;
- and you have edited it here, the post goes into conflict. A sheet shows both
  copies with the differences marked: **1** keeps yours, **2** takes the
  server's (your blyg's), **3** keeps both (yours becomes a new draft). Esc
  puts it off; the mark in the status bar, or publishing the post, brings the
  sheet back.

A change only to something else on the post, such as an attachment added
elsewhere, is simply sent again.

## Quick capture and scratch notes

A global hotkey (⌃⌥B by default, `capture-hotkey`) opens a small panel over
any app. esc, ⌘S or clicking away keeps what you wrote as a **scratch note**:
it stays on this Mac, is never synced and never published, until you decide.
⌘D makes it a draft on your blyg, and ⌘⏎ publishes it (as a fragment if it
fits, otherwise as a thread).

Scratch notes show in the main list with a `scratch` pill, are searchable and
editable, and ⌘D or ⌘⏎ there promotes them too. Images pasted into a scratch
note stay on your Mac until it's promoted, when they're uploaded.

**Capture from anywhere.** On a Mac, select text in any app and choose
Services › **Capture in Burrow** (in the app's menu, or the right-click menu):
the panel opens with it quoted, and the page's address under it when the app
sends one (some browsers do). To give it a key, use System Settings ›
Keyboard › Keyboard Shortcuts › Services. On Windows, text you copied in the
last ten seconds shows in the panel as a **quote clipboard** chip; click it to
quote it. It's never put in for you.

- `capture-default = draft` makes esc save a draft instead.
- `new-note = scratch` makes the omnibar create scratch notes too, and
  `new-note = draft` makes ⌘N start drafts. Unset, ⌘N starts a scratch note
  and the omnibar creates a draft.

| | |
|---|---|
| ![Quick capture](screenshots/quick-capture.png) | ![Quick capture's button row](screenshots/quick-capture-buttons.png) |

## Links, mentions, images and spelling

- **Paste a link over text.** Select some text and paste a web address to link
  it, as in WordPress: the text becomes `[text](address)`.
- **Mentions.** Type `@` to pick from the blygs you know: the people in your
  blyg's address book first (studio 0.56's contacts), then subscriptions,
  blogroll and authors you read. Where your blyg turns `@handle` into a link
  when it publishes (studio 0.56), picking a contact writes `@handle`, and
  the popup shows the link it becomes; publish tells you about any handle it
  couldn't find. Otherwise it inserts a plain link to that blyg. Either way it
  notifies no one.
  The preview shows a contact's handle as the link it will become (the
  same h-card your blyg publishes). Manage the address book in Blyg ›
  Contacts… (see [Reading](reading.md#contacts)).
- **Images.** Paste or drop an image to upload it; it goes into the text
  where you pasted it, and the preview shows it. Until a published post shows
  it, an upload is private on studio 0.28 or later: only you (and Burrow,
  signed in as you) can see it.
- **Spellcheck** with the macOS spell checker, your languages and learned
  words: a red wavy underline, and right-click for suggestions, Learn Spelling
  and Ignore. Code, links, quotes and TK markers are skipped, and checking
  runs in the background after you pause, so typing stays instant.
  `spellcheck = false` turns it off.

| | |
|---|---|
| ![Picking a blyg after @](screenshots/mention-picker.png) | ![Spelling suggestions](screenshots/spellcheck.png) |

## The preview and the full editor

A live preview beside the editor shows the post exactly as your blyg will
publish it, in your blyg's own theme: quotes, links, AI-written spans, images
and video. Click a paragraph in the preview to jump to it.

- ⌘1 **Write**: the list and the editor.
- ⌘2 **Preview**: the list, the editor and the preview.
- ⌘3 **Full editor**: the editor and the preview, with the list hidden.
- ⌘E toggles the preview in place.

### Focus mode

⌥⌘F (View › Focus; Ctrl+Shift+F on Windows) leaves just the editor. The line
you're typing on stays put, a little above the middle of the window
(`focus-line`, 0.1–0.9, default 0.45), and the paragraphs you aren't in are
dimmed (`focus-fade = off` keeps them plain). The only chrome is a hairline at
the bottom with the sync dot and how many words you've written this sitting;
hover it for the status bar. That count is never stored. esc brings everything
back, and ⌘L still reaches the omnibar.

![An image in the preview](screenshots/image-preview.png)

## Quoting other posts

In a thread, `![[id]]` alone on a line **quotes** a post: yours, or one from a
blyg you follow. The quote is a snapshot taken when you publish; later edits
to the original never rewrite it. ⌘K (or typing `![[` at the start of a line)
opens a picker of the posts you hold, and ⏎ inserts the line with a note,
`![[id | note]]` (see [labelled links](#labelled-links) below). The picker
searches every word of your published posts and the blyg posts you read, on
your Mac, and filters by source (all, mine, imported) and blyg, newest or
oldest first. Where you type the search is the blyg's **picker typing**
setting: after the brackets in the editor (the default on a Mac), or in the
picker's own box. Quotes only go
in threads: in a fragment, Burrow says so and leaves the text alone.

### Partial quotes

`![[id]]` followed directly by `>` lines quotes **just that passage** of the
post:

```markdown
![[01J9ZK3Q7T5V8X2B4N6M0PQRST]]
> The passage you want, exactly as it is in the post.

What you have to say about it.
```

A blank line between `![[id]]` and the `>` lines makes it a whole quote
followed by your own blockquote instead. The preview flags a passage that
isn't in the quoted post, and publishing refuses it. In the stream, a partial
quote shows only its passage.

The easy way to write one is to select the passage while reading and press
⇧⌘D, or to reply and select it inside the stub's quote in the preview; see
[Reading, quoting and responses](reading.md#universal-quoting).

### `[[id]]` links

An inline `[[id]]` **links** to one of your posts or one you read, without
quoting it:

```markdown
As I said in [[01J9ZK3Q7T5V8X2B4N6M0PQRST]], the lighthouse is the point.
```

The preview shows it as the published page will: the target's first words in
quotes, linked. Links inside code stay as text. A link that can't be resolved
is flagged in the preview, the status bar and the publish sheet, as quotes
are; publishing refuses it ("one or more references do not resolve").

### Labelled links

A link can carry its own text after a `|`:

```markdown
As I said in [[01J9ZK3Q7T5V8X2B4N6M0PQRST | Gardens, not streams]], the
lighthouse is the point.
```

The **label** is what the link says on the published page, in place of the
target's first words. To **rename a link**, edit the words after `|`; the id
before it is what the link points at, so leave that alone. A quote takes a
label too, `![[id | note]]`, but there it's a **note** for you: the published
quote shows the quoted post, not the note. The preview renders labels as
blygger-studio does.

Typing `[[` anywhere outside code opens the same picker as `![[`, for a link.
What you type after the brackets keeps going into the editor as the search,
and a `]` closes the picker, so `[[id]]` typed in full is left as you typed
it. If you write brackets in prose (or in `[TK]` scopes) and would rather
nothing popped up, set `bracket-picker = quotes` in the
[config file](config.md) (only `![[` at the start of a line opens it) or
`bracket-picker = off` (typing never does); ⌘K still opens the picker.
Each row leads with the post's title. ⏎ writes the labelled form, and the
footer says so: "⏎ inserts [[… | label]]" for a link, "⏎ inserts ![[… |
note]] on its own line" for a quote. The label the picker writes is:

- the post's **heading**, when it opens with one;
- else its **title** (its first line), `[[id | Gardens, not streams]]`;
- else, when the title is longer than 80 characters or the post has no text,
  a **quoted excerpt** of its first words.

When that label isn't the title, the row shows it on a muted line, *links as
“…”*, so you know what the page will say. (For a post with no heading the web
studio writes a quoted excerpt of its first 60 characters; Burrow writes the
title, which is what you searched for and what a reader should see. Either
way the label is just your text, and you can change it.) **Quote into a
thread** from Reading writes the labelled form too.

Every studio Burrow syncs with (0.49.12 or later) understands labelled links.
On a blyg that needs updating nothing syncs, and the pickers write bare links
(`[[id]]`, `![[id]]`); publishing a post that has labels offers ⏎ to write
them bare. See [Server requirements](server.md).

| | |
|---|---|
| ![The [[ link picker](screenshots/picker-link.png) | ![The ![[ quote picker](screenshots/picker-quote.png) |

**Link post** (next to Fork on a post you're reading) starts a new fragment
holding `[[id]]`: a plain link, not a response. Reply is the one way to
respond.

### Replying to, quoting and linking your own posts

A published post of yours has a row of chips above its text in Posts: "Your
post · v3 ·" then **Reply · new stub** (⌥⌘R), **Quote into a thread** (⌥⌘Q)
and **Link post · new fragment** (⌥⌘L). They're in the Post menu too, and do
what the same actions do on a post you're reading: Reply starts a stub thread
of your own post, Quote puts `![[id | label]]` in a thread, and Link post
makes a fragment holding `[[id]]`. Drafts and scratch notes have no row.

There's no Fork there: your own post forks only from a pin. Open ⌘Y, pick a
pinned version, and choose **Fork this pin** for a new draft that descends
from it. On an unpinned version it's greyed out ("Pin one in ⌘Y to fork it").
Your posts are also a source in Reading; see
[Your blyg](reading.md#your-blyg).

## TK: gaps for later

Write `[TK]instruction[/TK]` where something is still to come:

```markdown
The harbour was quiet. [TK]one sentence on why, in my voice[/TK]
```

With the caret inside it, ⌘G fills it with AI (if you've switched a provider
on), written in place as `[TK]instruction[=]output[/TK]`. ⌘G inside it again
regenerates. The preview tints generated text, and it's always disclosed as
generated when you publish; an unfilled gap shows as `⚠ ungenerated` in the
preview. See [AI helpers](ai.md).

**A quote the AI added.** In a thread, a `![[id]]` on its own line inside a
gap's generated text is a real quote once you publish: it's baked into the
post and notifies that post's author. If the gap's instruction didn't name
that post (a model sometimes echoes a quote it saw), the preview shows it as
the quote publishing will make, and the publish sheet warns before you
publish. Delete the line, or name the post in the instruction if you meant
it.

**Highlighting generated text** (studio 0.27 or later): the blyg's public
pages can tint generated passages and mark them with a small robot that says
what you disclosed. Your blyg's settings (in the Reader's site settings) set
the default; the ⌘G palette sets it per post (default, on or off; **H** cycles
it), and the preview shows the post as its page will.
