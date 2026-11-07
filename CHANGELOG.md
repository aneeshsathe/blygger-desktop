# Changelog

All notable changes to Burrow (called Blygger Desktop up to 0.6.0) are
documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses
[Semantic Versioning](https://semver.org/). Before 1.0, minor versions may
break things.

To cut a release: rename `[Unreleased]` to `[x.y.z] - YYYY-MM-DD`, bump
`version` in the root `Cargo.toml`, commit, and merge to `main`: the release
workflow sees the untagged version, tags the commit, and publishes that
section as the release notes. Add the release's section to
`site/src/releases.md` (what the app's "What's new" opens). If the release
teaches something new in the tour, set the newest entry's `version` in
`crates/blyg-app/src/onboarding/whats_new.rs` to the release's.

## [Unreleased]

## [0.18.1] - 2026-11-07

### Changed

- **What's new after an update** is a short card with a link to the new [release notes](https://aneeshsathe.github.io/blygger-desktop/releases.html) on the docs site, which the status bar's "What's new" link opens too, at the release's heading. The card used to list everything, and a big release no longer fit on it (it didn't scroll).

- **Burrow in a browser is fast with thousands of posts to read**, and no longer heats a phone. Measured live on a local blyg with 300 posts and 4,000 to read (WebKit, iPhone emulation): opening a post takes 43 ms (was 3.9 s), the Read tab 80 ms (was 1.3 s), going back 52 ms (was 1.3 s), and a second start shows your posts in 0.2 s (was 0.9 s). Idle, with the processor slowed four times to stand in for a phone's, it uses 0.2 s of CPU a minute (was 4.5 s).
  - Only the screen you're on is drawn, and the reading list and post list a page at a time, with more as you scroll (the window's reading list too). A redraw that changes nothing changes nothing on the page.
  - The reading list's excerpts are made once per post version, and it is ordered without copying every post.
  - The store's snapshot holds your posts, the outbox and your marks (120 KB, was 8 MB); the reading list has its own, written only when it changed and the page is hidden. A snapshot from 0.18.0 moves over on its own.
  - A refresh asks the blyg's `/api/changes` first and reads only the lists that moved, as the app does (with the app's full read every 10 minutes as a safety net). The outbox no longer redraws after every send.
  - Hidden (another app, a locked phone), the page does no work: the 30 s redraw for relative times waits until it's on screen again.

### Fixed

- **Burrow in a browser:** live mode finds the owner API at `{mount}/api` beside `{mount}/burrow/`, so it works on a blyg served under a path, not only at the host root (where it leaned on the deprecated host-root `/api` alias).
- **Nearby thoughts: the keys work after a click.** Clicking a rail row handed the keyboard to the window, so ⌥↑/⌥↓, ⌥⏎ and ⌥⇧⏎ did nothing until you clicked back into the text. A click now chooses the row and leaves the editor in charge.
- **Nearby thoughts: hover a row to see the post.** Resting the pointer on a row shows a card left of the rail with the post's title, who wrote it and its first lines, like the editor's link hover cards.
- **Nearby thoughts: no stuck or misleading status.** When building the index failed (an endpoint that was down, say), the footer switched to "By meaning" at the next pause with no index behind it and never tried again. It now keeps saying "Shared phrases only: why" and retries at most every 30 seconds while you write. Settings › Nearby says "Starts when you write in a post" before anything is loaded, and an empty rail with nothing to match says so.
- **Burrow in a browser, on a phone:** opening a post in Read no longer goes blank after a second. About 800 ms after a post opened, the outbox's send redrew the whole app three times, rebuilding every card in the reading list under the open post (4,000 cards, 64,000 elements), and wrote the whole store, every post's HTML included, to IndexedDB twice. That held the page for seconds and, on an iPhone, left the post view blank, every time the post was opened. Nothing under an open post is redrawn now.

## [0.18.0] - 2026-11-07

### Added

- **The shelf.** ⌥⌘B keeps a post you keep updating (a /now page, a reading log) at the top of the Write list, marked 📚, in your order. ⌘B opens the shelf: 1–9 or ⏎ opens a post, ⌥↑/⌥↓ reorder, ⌫ takes one off. Any post of yours can be shelved. The shelf is kept on this computer for now (store schema v17); syncing it waits on blygger-studio#135. On Windows the keys are Ctrl+B and Ctrl+Shift+A.
- **Lineage while you write.** A quiet line under the editor shows the open post's lineage glyph, "draws on N · drawn on by M" and the posts it links to. A click opens the lists, and a row opens that post (yours in the editor, others in Reading). ⌘J on Posts opens the lineage sheet for your published post, or the lists for a draft.
- **The neighbourhood map.** In the lineage sheet, − zooms out to two hops each way, nodes coloured by relation (fork, reply, quote), the second hop dashed and your own posts shaded. Arrows move, ⏎ re-centres, ⌫ walks back, o opens, + returns to the list. Read from what this Mac holds and capped, so it stays fast.
- **Hover cards and ⌘-click.** Resting the pointer on a `[[id]]`, `[[id | label]]` or `![[id | label]]` in the editor shows the post's title, author, version and first lines, with "label it" for an unlabelled link. ⌘-click (Ctrl-click on Windows) opens it. The stream's quote boxes and links show the same card. Withdrawn posts show as withdrawn; other people's unpinned past versions are never shown.
- **#thicket chat (experimental; it may be removed).** A tab beside the views (⇧⌘I, Blyg › #thicket Chat) keeps you in #thicket, a channel for Burrow users on Libera.Chat (IRC). Setup signs in with an existing Libera.Chat account (SASL PLAIN over TLS), or creates one (NickServ REGISTER, then the emailed VERIFY code); nothing connects until you do. The password goes in the Keychain (`irc:libera:<nick>`), the nick in `chat-nick`, and Burrow signs in on its own after that, reconnecting with backoff. The tab has the log (joins and parts kept quiet, `/me`), a user list with operators first, an unread dot and count, a stronger @ mark and a system notification when you're mentioned (`chat-notifications`). Settings › Chat hides it (`chat-tab`, on by default). The protocol is the new `blyg-irc` crate, written for Burrow (no tokio, no GPL code). On Windows the key is Ctrl+Shift+I, and only the tab's marks show for mentions.
- **Burrow in a browser, and on a phone (preview).** The new `blyg-web` crate compiles Burrow's own Rust (the list and omnibar rules, the counter, the Markdown preview, the reading view-model) to WebAssembly behind a small page: the main window at desktop widths, and at phone widths bottom tabs (Write · Read · Shelf), a ＋ for quick capture, swipe to mark read or quote, hold a sentence to quote or reply to it, and a paged bar above the keyboard with Markdown tools. It installs as an offline web app. For now it runs on the invented sample posts, kept in the browser; nothing is sent anywhere. Serving it from your own blyg at `/burrow` is proposed upstream (blygger-studio#139). See `crates/blyg-web/README.md`.
- **A terminal pane (experimental).** ⌃\` (View › Terminal) opens your login shell in a row at the foot of the window, under your posts, for a CLI agent (`claude`, `codex`…), `git` or `wrangler dev`; ⌃\` again hides it, and the shell keeps running until you end it. ⇧⌘T (Post › Send Selection to Terminal) pastes the editor's selection into it as a quote. Drag to select, ⌘C / ⌘V, 5,000 lines of scrollback, your theme's colours, bold, underline and 256/24-bit colour. Point an agent at your blyg's own MCP server to have it read your posts and write drafts (see the docs' "The terminal pane"). The shell gets your environment minus everything of Burrow's (no `BLYGGER_*` variable, no token), and there's no bridge into the app: Burrow and the shell meet only at the terminal. The engine is `alacritty_terminal` (Apache-2.0) on `portable-pty` (MIT), in the new `blyg-term` crate: libghostty-vt 1.0 has no crates.io package that builds without Zig or a download, so its bindings are written but kept behind an off-by-default build feature. `terminal = false` turns the pane off; `terminal-shell` runs another program. On Windows the keys are Ctrl+\`, Ctrl+Shift+T and Ctrl+Shift+C / V, and the shell is `%COMSPEC%`.
- **The command palette.** ⇧⌘P (View › Command Palette…) finds any action, theme, font, blyg, hopper or post of yours by typing part of its name, with aliases ("undo edits" finds Discard Changes). Each row shows its key; an action that can't run now stays listed with the reason. ⏎ runs it, ⌘⏎ runs it and keeps the palette open, and the empty field shows your last ten. Extension commands are its Extensions group. On Windows it's Ctrl+Shift+P.
- **Focus mode.** ⌥⌘F (View › Focus) leaves just the editor: the line you're typing on held at `focus-line` (default 0.45 of the height), other paragraphs dimmed (`focus-fade = paragraph | off`), and a hairline with the sync dot and a word count for this sitting (never stored). esc leaves; ⌘L still reaches the omnibar. On Windows it's Ctrl+Shift+F.
- **Serendipity (off by default).** `serendipity = own` (or Settings › Serendipity) makes ⌘N's empty editor show one of your published posts, weighted to ones nothing links to and to this week in another year, never the same one within a month: q quotes it, l links it, o opens it, r shows another, and typing dismisses it. `serendipity = on` includes posts from your subscriptions.
- **Small motion, optional sound.** A published post's row settles, the sync dot and the counter ease between colours, the list crossfades when you switch blygs, and focus mode's hairline slides in, all under 300 ms and none with Reduce Motion on (or `BLYGGER_REDUCE_MOTION=1`). `sounds = quiet` plays a quiet macOS system sound on publish and on quick capture; off by default, never on typing, nothing on Windows.
- **Trails.** On the reading screen `t` adds the post to a trail with a one-line why (⏎ keeps it, esc skips); ⇧T starts another trail. A trail bar shows the stops and "stop 2 of 4"; the Reader's sources pane lists trails, and a trail as the source lists its stops in order, so j/k walk it. ⌥⌘T (Blyg › Publish Trail…) writes the trail as a thread draft (`# name`, then `![[id | why]]` per stop); nothing is published. `h` on that sheet makes a private hopper of the stops. Kept on this Mac (store schema v19); withdrawn posts stay as "withdrawn" and are left out. On Windows: Shift+T, Ctrl+Alt+T.
- **Drag to quote, and the quote tray.** Drag a stream post, Reader row or lineage row into the editor: a quote on its own line in a thread, a `[[id | label]]` link in a fragment, where the pointer is. Dropped on the omnibar it starts a new thread quoting the post; on the reading post box it's quoted there. `q` on Reading or ⌥⌘K (Post › Put on Quote Tray) keeps a post on a tray above the status bar, per blyg in `state.json`; the `[[` / ⌘K picker lists the tray first, and placing a post (or ⌫) takes it off. ⇧⌘Q was left alone (it's the system's Log Out). On Windows: Ctrl+Alt+K.
- **Split view.** ⌥⌘⏎ (Post › Open Beside) opens the selected post beside the one you're writing: your own in a second editor with its own autosave and status, anyone else's read-only in native blocks. ⌥⌘[ / ⌥⌘] (View › Left Post / Right Post) move between them (⌘[ / ⌘] are taken by outdent/indent); the divider drags and is remembered. The mock's "Join into left" isn't built. On Windows: Ctrl+Alt+Enter, Ctrl+Alt+[ / ].
- **Nearby thoughts.** A rail on the editor's right shows up to five of your own published posts and the posts you hold that say what the paragraph under the caret says: ⌥↑/⌥↓ choose, ⌥⏎ links the shared phrase in place (`[[id | phrase]]`), ⌥⇧⏎ quotes the post on a new line (threads only), ⌥⌘U shows or hides the rail (Alt+Up/Down, Alt+Enter, Alt+Shift+Enter, Ctrl+Alt+U on Windows). It matches shared phrases through the search index by default. Settings › Nearby (`nearby-embedder = local`) adds potion-base-8M (MIT, about 31 MB, pinned and checksum-checked, run in pure Rust by the new `blyg-embed` crate) to match meaning offline; `endpoint` uses an Ollama or OpenAI-compatible server you run; `nearby-rerank = on` lets your AI provider order the best ten. The index is store schema v20, updated post by post in the background; withdrawn posts never appear and nothing is fetched to suggest. New keys: `nearby`, `nearby-embedder`, `nearby-endpoint`, `nearby-endpoint-model`, `nearby-rerank`.
- **Loose ends.** The omnibar reads operators: `is:orphan` (published, and nothing points at it), `is:draft`, `is:stale` (a draft untouched for a month), `is:leaf`, `has:tk` (an ungenerated TK), `links:` and `quotes:` (an id or a host), `kind:thread|fragment` and `v:<n>`; other words search as before, and operators alone never start a post. With the search empty, a muted line over the list says what's loose, in words ("2 orphans · 1 ungenerated TK"), and a click filters. Never a social count.
- **Capture from anywhere.** On macOS, Services › Capture in Burrow opens quick capture with the selected text quoted, and the page's address under it when the app sends one (an `NSServices` entry in the bundle's Info.plist). On Windows, quick capture offers text copied in the last ten seconds as a "quote clipboard" chip, never inserted on its own. A web address pasted into the omnibar offers "⏎ start a fragment with this link", titled from the page (fetched once, without the token); a blyg post you hold is quoted, `![[id]]`, in a thread instead.
- **Studio 0.56 parity.** Your blyg's contacts (`GET /api/contacts`, contract 5.7) come first in the @ popup, kept on this computer; where the blyg rewrites `@handle` on publish, picking a contact writes `@handle`, and publish's `unresolvedHandles` shows in the toast. Reading has a **Mentions** smart feed under All unread (`/api/reading?mentions=true` when served, else the posts you hold that quote or link to you): a list with dots, never a count. Older studios are never asked.
- **Burrow on the web, on your own posts.** Served from your blyg at `/burrow/` (beside `/studio`), the web and phone app now runs on your real posts and reading list instead of the sample data, through the same-origin owner API and your Studio session cookie (no token, nothing stored; signed out, it asks you to sign in at Studio). Edits are kept in the browser at once and sent after you pause, with the revision they were based on; offline they wait in an outbox and replay when the blyg is back; a post changed elsewhere shows both copies until you choose ("Keep mine" or "Use theirs"), never overwriting. New drafts, edits, publishing (only when you choose it), deleting drafts, discarding changes, read marks and hoppers go to the blyg; the shelf stays in the browser. Anywhere else it's the demo, labelled as such. `crates/blyg-web/dev-proxy.py` serves it beside a local `wrangler dev` blyg for development.

### Changed

- **⇧⌘P is the command palette.** The Extensions palette it used to open is Post › Extensions… (and the palette's Extensions group).

- **Typing and searching stay smooth at any size.** A keystroke no longer waits on the local database: your text is saved on a background thread, the search index catches up after a pause, and the preview and the omnibar's search run off the main thread. In a 20,000-character thread with the preview up, over 2,000 posts, a keystroke went from 30–50 ms of main-thread work (over 100 ms at worst) to about 1 ms; an omnibar keystroke from 20–35 ms to under 1 ms. Your own posts are read through a second database connection, so the Write screen never waits on a sync, and a big reading-list pull is merged in small steps, so the Reading screen waits tens of milliseconds behind one rather than seconds (store schema v18).

### Fixed

- Switching blygs no longer writes the editor's older text back over newer text the post already had (an edit saved elsewhere, or merged by a pull just before the switch).
- Typing now takes 0.18.0's background save path in the app itself (the blyg switcher didn't pass it on), and local images in scratch notes are no longer refused with "local images aren't supported here".

## [0.17.1] - 2026-11-03

### Fixed

- **Subscribing to a blyg's public hopper page or feed follows the whole blyg.** On studio 0.50 and later, that address can otherwise give a subscription to just that hopper, which Burrow can't show yet. Burrow now asks for the whole blyg in the preview, in the subscription itself, and in OPML imports. It only asks studios that understand the request (contract 5.1 or later), so studio 0.49 is unaffected.

## [0.17.0] - 2026-11-02

### Added

- **`bracket-picker = on | quotes | off`.** `quotes` lets only `![[` at the start of a line open the post picker, so `[[` in prose and TK scopes stays plain text; `off` stops typed brackets opening it at all. ⌘K always opens it. ([#53](https://github.com/aneeshsathe/blygger-desktop/issues/53))
- `scripts/reader-conformance.sh` runs Burrow's renderer against the blygger-spec reader conformance suite at a pinned commit; see `docs/CONFORMANCE-READER.md`.
- **Put several posts in a hopper at once.** With posts picked in the Reader (⌘-click, ⇧-click, ⌘A), **h**, ⌥⌘P and a picked post's **Hopper ›** menu act on all of them. A hopper holding some of them shows a dash.

### Changed

- **Burrow stays quick with thousands of posts in Reading.** A sync that finds nothing new no longer rewrites every post (it held the store for 8–15 s on a 4,000-post reading list; now well under 0.1 s), the reading list is kept parsed in memory, keeping the list's order is no longer quadratic, the unread badge no longer copies every post, and search narrows as you type. The store's write-ahead log is trimmed back after each checkpoint. Store schema v16 (the search index re-indexes a post only when its text changed).
- **The reader follows Burrow's theme**, not the Mac's: with `theme = light` under a dark macOS, the reader and the editor's preview no longer render dark.
- **A hopper lists its posts in its own order**, the most recently added first, as on your blyg, not by post date.
- Reading your hoppers is quicker: up to four are read at once, and they're still read only when something in them changed on your blyg.
- **A hopper change that crosses another one on your blyg is tried again.** When the blyg says the hopper changed while a rename, description or Make Private was being saved, Burrow reads it again and sends the change once more. Make Public is sent again only if the page keeps the address Burrow showed you; otherwise Burrow says why. Putting posts in or out is retried the same way.

### Fixed

- Switching blygs: events from the blyg you left (status, conflicts, sign-out) no longer reach the one you switched to, and reopening a blyg waits for its own folder, not one guessed from its public address.
- Signing in to a second blyg on the same host as one you already have (another port or path) is refused, instead of replacing the first one's sign-in and taking over its posts. ([#58](https://github.com/aneeshsathe/blygger-desktop/issues/58) tracks supporting it.)
- Windows: Ctrl+Shift+digit shortcuts, including the blyg switcher's, never fired; bindings now follow the keyboard layout. The switcher sheet is Ctrl+Shift+L on Windows.

## [0.16.0] - 2026-11-02

### Changed

- **Burrow now needs blygger-studio 0.49.12 or later** (its API contract 5). On an older blyg nothing syncs, and Burrow says "Your blyg needs updating". Your posts stay on this computer and are sent once the blyg is updated. Connecting to an older blyg is refused with the same message, and a blyg newer than Burrow understands asks you to update Burrow. This includes stock studios 0.46–0.49.11 and patched Worker forks.
- **Saving checks the post's revision.** Burrow sends the revision it last saw with every change instead of checking the post before each save. If the post changed on your blyg as well and you haven't edited it here, the change stops and the blyg's text shows. If you have edited it here, the post goes into conflict and you choose, so the blyg's text is never overwritten. A change only to something else on the post, such as an attachment added elsewhere, is retried.
- **Lists are read in pages.** Burrow reads up to 20 pages of 100 per pull and removes nothing locally until it has read a whole list to its end. A list that changes while it's being read is read again from the start, and a longer reading list is completed on the next pull.
- The blyg's API is found through its capabilities, under the blyg's own path, so a blyg mounted under a path works without a host-root `/api`. Sign-ins issued for the older host-root address keep refreshing.
- Media URLs are absolute.
- A post marked unread on another device is unread here too.
- The tour has a step on posts edited in two places.

### Removed

- Support for older studio API shapes: offset paging with totals, the Worker-fork extension routes (client provenance, the stock reading walk, `before`-paged imported reading) and the 0.47 labelled-links fallback.

### Known issues

- Burrow can only remove attachments it uploaded itself. Removing an attachment needs the post's revision, and the studio's media API doesn't say which post an attachment is on. Reported upstream as blygger/blygger-studio#103.

## [0.15.0] - 2026-11-01

### Added

- **Hoppers.** Your blyg's hoppers, its collections of posts, are listed under **Hoppers** in the Reader's sources pane, below Your blyg, each with its count and a globe when it's public. Pick one to list its posts. Press **h** on the reading screen, ⌥⌘P (Post › Put in Hopper…), or choose **Hopper ›** from a post's chips or right-click menu to put the post in a hopper or take it out; **New Hopper…** makes one and adds the post. Your own posts go in hoppers too. Adds and removals show at once and are sent in the background, so ones made offline are sent later. A hopper's right-click menu renames it, sets its description, makes it public (it gets its own page on your blyg; Burrow asks first) or private, and deletes it without touching its posts. Blyg › New Hopper… and the sources pane's menu make one.

### Fixed

- The check for a studio API contract Burrow doesn't speak refreshes an expired sign-in like any other request, and a sign-in the blyg refuses now stops the check instead of passing it.

## [0.14.0] - 2026-11-01

### Added

- **Several blygs, one click apart.** Sign in to more than one blyg and switch between them from the title bar: click the blyg's name, or press ⌃⌘0 (Blyg › Switch Blyg…), and pick one, or jump with ⌃⌘1–⌃⌘9 (Ctrl+Shift+digit on Windows). **Add a blyg…** signs in to another without signing out of the first; **Manage…** signs out of one, keeping or deleting its local copy. Each blyg keeps its own posts, drafts, reading and sign-in, and changes waiting on one blyg sync when you switch back to it. Your current blyg's data stays where it is. In the config file, `blyg` lines list your blygs and `blyg-url` is still the active one. CLI: `blygger +list-blygs`, and `--blyg <host>` on the OPML commands.
- **Follow the thicket from the reader** (Blygger Studio 0.49). In the reading pane, every quoted layer, quotes inside quotes included, says whose it is and opens in the reader. So do a stub's "responds to" line and any ordinary link that points at a blyg post: Burrow asks your blyg which links name posts (studio 0.49), or checks the linked pages itself on an older blyg. A post from a blyg you don't follow opens as a peek, read from its own blyg with nothing saved, under "You're not subscribed to this blyg — subscribe?". Links that aren't blyg posts open in the browser, so you keep your place. A breadcrumb above the post shows the path you took, and **⌘[** or "‹ back" steps back one post at a time.
- **Browsing history** (⇧⌘H, Blyg › Browsing History, or "history" above the post). A pane beside the reader lists this walk and earlier ones, as the tree they were, each step opening its post, with "clear history". It's private to you: kept in your blyg's own browsing log on studio 0.49, or on this Mac with an older blyg, and never published.
- **Background.** Withdrawn posts, the blygs' and your own, are listed under a new Background smart feed and nowhere else, so a retraction stays findable without crowding what you're reading.
- **Write while you read.** A post box runs along the foot of the reading screen, under the Stream and the Reader alike, so you can post without losing your place. `c` (with the list focused) or ⇧⌘K (Blyg › Compose While Reading) puts the caret in it; ⌘⏎ posts a fragment (a thread when it quotes a post or runs long), ⌘D keeps a draft, ⤢ moves it into the editor, and **❝ Quote** drops the open post's `![[…]]` in. Unsent text survives switching screens and restarts. The Posts screen is unchanged.
- **A revised post leads with what changed.** In the stream, a post past its first version opens with its latest changelog note, "V3: …", read from the author's changelog, or "V3: No changelog entry available" when they left none.

### Changed

- **Burrow opens on Reading**, not Posts, and the tour ends there too unless you started it while writing. It stays on Posts during first-run setup and when there's no blyg to read from.
- **A stub's passage is chosen in the editor's preview** (Blygger Studio 0.49). Select text inside the stub's quote in the preview and press **❝ quote only this** (or ⇧⌘D): the passage goes under that quote, whichever of several it is. **Quote another passage** adds a whole-post quote after the caret to select from. The separate passage chooser is gone.
- Burrow refuses to sync with a blyg whose studio speaks a newer private-API contract than it does (the 2.0+ contracts in Blygger Studio's pending API rework), saying so, rather than misreading it. Nothing local is touched.

### Fixed

- **A sync conflict you closed without resolving is no longer a dead end** ([#52](https://github.com/aneeshsathe/blygger-desktop/issues/52)). Publishing or withdrawing the post shows the conflict sheet again instead of “resolve the sync conflict first”, and while the post is open the status bar says **sync conflict · resolve…**, which opens it too.
- On a stock blyg whose reading list leaves withdrawn posts out (studio 0.49), withdrawn posts are no longer forgotten on a full refresh.
- Withdrawn posts no longer count toward the Reading screen's unread mark.

## [0.13.1] - 2026-10-19

### Fixed

- **Labels render as Blygger Studio 0.48 renders them.** The preview now matches studio 0.48.0, which fixed six label bugs that Burrow had copied on purpose from 0.47.0: a label in a generated block is escaped once (not `&amp;lt;`), a backtick in a label no longer turns a later link on the line into plain text, a `&` in a labelled link inside a web address is escaped in the link, a label made of `]` characters can no longer cut a link short, and choosing a passage in a stub keeps the quote's note (`![[id | note]]`).

## [0.13.0] - 2026-10-18

### Changed

- **Labelled links need Blygger Studio 0.47.** The `[[` picker writes labelled links, `[[id | label]]`, which an older studio shows as literal text or a broken link. On an older blyg everything syncs as usual except posts whose text has a labelled link or quote (outside code, generated text included): they wait until the blyg is updated or the labels go. Publishing one offers **⏎** to write the links bare (`[[id]]`) and publish. Withdraw and pin always go, since they don't send the text. On such a blyg the pickers write the bare form. Burrow says "Your blyg needs updating" once, on connecting or at launch, keeps a status-bar notice with an **Update your blyg** link, shows the same in About, and a waiting post's pill says why on hover. Burrow reads the version from the blyg's `blyg.json` `generator`. For another server it checks whether search results carry a `label`, and when it can't tell, it doesn't hold anything back. The "older than 0.40" limits sheet and its About section are gone, since every blyg that would have seen them now gets this notice. [Server requirements](https://aneeshsathe.github.io/blygger-desktop/server.html).
- **The `[[` and `![[` pickers write labelled links.** A pick writes `[[id | label]]` (a quote, `![[id | note]]`), studio 0.47's grammar: the label is the link's text on the published page, so editing the words after `|` renames the link, and a quote's note is only for you. The label is the post's heading, else its title, else a quoted excerpt. Picking a post with no heading labels the link with its title, `[[id | Title]]`, rather than a quoted excerpt (a deliberate difference from the studio). Quote into a thread from Reading writes the labelled form too. The preview renders labels as studio 0.47.0 does.
- The `[[` and `![[` pickers lead each row with the post's title again; a link's row adds "links as “…”" when the label differs from the title.
- The picker footer says what a pick writes: "⏎ inserts [[… | label]]", "⏎ inserts ![[… | note]] on its own line".
- Hovering "unpublished edits" in the status bar, or the edit mark in Posts, says how to publish (⌘⏎) or discard (⌥⌘Z) them.
- Unpublished edits have their own mark in Posts (`marker.edited`): a dot in the theme's edited colour, Δ in Fortress. The published and pinned marks have tooltips.
- Fortress's status bar shows your last two weeks of posts as a block-character strip instead of the z-level. Themes naming z-level still load, with a warning.

### Added

- **Discard changes.** A published post with unpublished edits can go back to the version it has published: ⌥⌘Z, Post › Discard Changes…, the toolbar's Discard, or "Discard changes…" on the current version in ⌘Y. A sheet asks first, then "Back to vN as published". The public post doesn't change. The toolbar's Discard button is an eraser.
- **Your own posts in Reading and Posts.** "Your blyg" is a Reading source: your published posts sit in All, Today and the stream by date, and alone under Your blyg, and they are never unread or counted. A published post in Posts has a row of chips above the text: Reply · new stub (⌥⌘R), Quote into a thread (⌥⌘Q) and Link post · new fragment (⌥⌘L), also in the Post menu. Your own post forks only from ⌘Y, with Fork this pin on a pinned version.
- **Windows wording in newer app text.** Lineage, the stub bar, the sources pane, Subscriptions, the stream, the notes drawer and About say Windows keys and "this PC" where they said ⌘ or "this Mac".
- Windows: Discard changes has no default key (Ctrl+Alt+Z is AltGr+Z). Use the Post menu or the toolbar.

### Fixed

- Fixed a run of spaces in the "still marked unpublished" toast.

## [0.12.0] - 2026-10-13

### Changed

- **Works with a stock blyg.** Burrow now runs fully against an unmodified Blygger Studio (0.40 and later, checked against 0.46.1), with no server patches. Reading syncs from the studio's own imported feed (`GET /api/reading/imported`, paged by cursor), read marks sync through its read-state routes, and lineage, citations, the full author and disclosed generation sources come with each post. Servers running the older owner-API extensions keep working. [Server requirements](https://aneeshsathe.github.io/blygger-desktop/server.html) and `docs/SERVER.md` describe what's needed.
- **Sign in with a studio token or the browser.** The server owner token from the old extensions is gone from sign-in. Use Sign in with browser, or a token from Studio → More → Client access (allow `reading:state` so read marks sync). If your blyg dropped the old owner token, Burrow opens the sign-in sheet after updating; sign in once and everything queued is sent.
- **Preview matches the studio 0.46.1 renderer.** Embeds are no longer baked into the HTML (the poster still shows), and `[TK]`, `[=]` and `[/TK]` inside code anywhere in a scope are plain text (studio 0.41.1).
- **"Drew on this".** A mention whose relation is `source` (a post that used yours as a generation source) shows as "drew on this".

### Added

- **Publish warns about quotes in generated text.** If a TK block's output holds an own-line `![[id]]` that its instruction didn't name, the publish sheet says so before you publish: publishing turns it into a quote and notifies that blyg. Preview now shows it as the quote publish will make (decision #60).
- **Generated text is marked as the publisher's own claim.** In the Reader, text a blyg marks as generated gets a tooltip, and the post ends with a short note that the disclosure is the publisher's own and isn't verified.

### Fixed

- **Reading sync against studio 0.40 and later.** Burrow asked for 500 rows a page in the old shape, which a stock studio refuses, so reading never synced there.
- **A refused sign-in stops syncing cleanly.** When the blyg rejects Burrow's token (for example after a server change), Burrow stops sending and pulling, keeps every queued change, and opens the sign-in sheet with an explanation, instead of retrying.
- **Authors and citations come through whole.** The full author object is kept, and `cited` survives on replies (`stub_of`) and remote references.

## [0.11.0] - 2026-10-09

### Added

- **Turn an extension off.** Manage extensions (⇧⌘P › Manage extensions…) has **Turn off** for any extension that's on, whatever state it's in. It stops the extension and keeps what you allowed, so Turn on brings it back as it was. **Forget permissions** turns it off and clears what you allowed, so the next Turn on asks again. The consent sheet gains **⌘⌫ turn off**, so you can back out the moment you see what an extension asks for.
- **Several notes folders.** markdown-notes can open any number of folders (Obsidian vaults, say). The Notes tab in the notes drawer has a chip for each folder, plus **Add folder…** and **Remove from Burrow** (which only removes the folder from Burrow and never touches your files), and remembers the folder you were in. Settings › Notes folders lists them, and you can add several at once. ⌘K's notes chip searches every folder. In the config they are `vault=` for the first and `vault-<label>=` for each one after.
- **Reading time and Inspect**, two bundled extensions ported from Blygger Studio 0.39, both off until you turn them on. **reading-time** puts an estimated reading time at the end of each post's byline in the Stream and the Reader ("· 4 min"; hover for the word count), counted the way the studio counts. **inspect** adds a ⋯ chip to each post you read, with an Inspect sheet showing the record Burrow holds (ids, versions, references, hashes) and its JSON with the bodies left out, with Copy JSON.
- **Extensions can add to what you read.** Two new slots in the extension protocol, behind the `reading.read` permission: a short marker at the end of each post's byline, and rows in a post's ⋯ sheet. See [Extensions](https://aneeshsathe.github.io/blygger-desktop/extensions.html).
- **Import and export subscriptions as OPML.** Blyg › Import Subscriptions from OPML… (or Import OPML… on the Subscriptions screen) reads an export from another feed reader, lists its feeds with the ones you already follow marked, and subscribes your blyg to the ones you tick, at a steady pace, filing them in an **Imported feeds** folder in the Reader, with a summary and Retry for any that fail. Imported feeds stay out of your public blogroll. Blyg › Export Subscriptions as OPML… saves all of them. `blygger +import-opml` and `+export-opml` do the same from a terminal.
- **Community extensions.** The docs now list extensions other people have made, from `extensions/community.toml`; submit yours with a pull request or the issue form. Burrow's maintainers don't review or vouch for them: an extension runs with your user's rights, so read its source and use it at your own risk.

### Changed

- **The lineage glyph shows counts.** Beside the kinds, the glyph shows how many posts a post draws on and how many known here draw on it ("2 · 3"). The ⌘J view and the action ring show the same numbers, by kind. On a blyg that runs the studio's lineage-glyph extension, Burrow uses the blyg's own counts; elsewhere it works them out from what it holds, using the same rules. Opening a post from ⌘J marks it read, as opening it from the list does.
- **Read state needs the `reading:state` permission** (Blygger Studio 0.39 moved marking posts read and unread there). Burrow now asks for it. **After updating, sign in again once** (Burrow › Disconnect…, then Sign in with browser, and allow every permission) so read marks sync between your Macs. Until then they're kept on this Mac and wait to be sent, and everything else syncs as usual. Studios before 0.39 are unaffected.
- **Cross-posting stays beside your work.** A cross-post run opens the browser in the usual foldable side pane, not over the whole window, and its Preview and Post sheets sit beside the page so you can see it. Fold the pane mid-run and the run carries on ("macro running · show pane" in the status bar). The pane unfolds itself before Burrow types into the page.
- **The tour teaches what's new.** After an update, "Show me what's new" starts at the first step for that release (NEW IN 0.11.0), and the tour now walks through extensions, Turn off, notes folders, reading time and inspect, OPML import, ⌘N, ✂ Clip and cross-post, on sample data, without changing your config. It also covers what 0.10.0 added, which its tour skipped.
- **markdown-notes no longer assumes `~/Notes`.** With no folder chosen it asks only to show messages, and the Notes tab offers **Choose a folder…**, so the consent sheet only ever names folders you picked. A config with `vault=` works as before.

## [0.10.0] - 2026-10-09

### Added

- **Burrow runs on Windows.** A 64-bit Windows build, from the same code, ships with every release as `Burrow-<version>-windows-x64.zip`: unzip it and run `blygger.exe`. Shortcuts use Ctrl where the Mac uses ⌘ (Versions is Ctrl+Shift+Y), a Menu button stands in for the menu bar, posts and the editor show in WebView2, and sign-ins are kept in Windows Credential Manager. It doesn't update itself yet, the browser pane is macOS only, and the build isn't code-signed. See [Windows](https://aneeshsathe.github.io/blygger-desktop/install.html#windows). The port is by Patrick Atwater ([@patwater](https://github.com/patwater)); thank you!
- CI checks every change on Windows too (fmt, clippy, tests and a release build).
- **Extensions.** Burrow can run extensions: separate programs that speak a small JSON-RPC protocol over stdio (BXP, documented in [Extensions](https://aneeshsathe.github.io/blygger-desktop/extensions.html)). Nothing runs until you turn an extension on with `extension = <name>`, and the first time it starts a sheet asks for each capability in plain words; what you allow is written to your config as `extension-allow` lines. An extension never sees your token and has no way to publish, withdraw or delete; reading files or going online is its own promise, which the sheet says. ⇧⌘P opens the Extensions palette (its commands, plus Manage extensions…), and `blygger +list-extensions` shows what's installed, on and allowed.
- **markdown-notes**, the first bundled extension, opens a folder of Markdown notes (an Obsidian vault, say) in a Notes tab of the notes drawer: browse, search, open, edit and save a note (its frontmatter is kept as it is, and a note changed on disk is never overwritten), and make new ones. Quote into post or Copy verbatim puts the selection or the whole note in your draft, Save selection to notes goes the other way, and ⌘K's quote picker gains a notes chip. It adds nothing to your notes and works with no blyg connected. Choose the folder in Settings › Markdown notes folder….
- **Clip page** (✂ Clip in the browser pane, ⇧⌘C): the selection, or the page's main article, becomes a quote in your draft with its title and link.
- **cross-post**, a second bundled extension, puts a published post on Substack Notes: its opening paragraph and the link, at most 280 characters (`max-chars`, `template`). It runs in the browser pane where you signed in by hand, shows the text to edit and then what the notes box holds, and posts only when you click Post. It's off until `extension = cross-post`, needs `extension-allow = cross-post browser.automate:https://substack.com`, and its recipe was checked against the live site on 2026-10-09. `blygger +list-extensions` now lists each extension's sites and macros, with when the macro was last checked. See [Extensions](https://aneeshsathe.github.io/blygger-desktop/extensions.html#cross-post).

### Changed

- **⌘N opens a new post** (Post › New Post, was New Draft). Instead of putting you in the search, it opens an empty editor with the caret in it, and leaves your search and the list as they were. What you type is a scratch note, saved on this Mac as you go ("New note · saved locally · ⌘D draft · ⌘⏎ publish"); ⌘D makes it a draft and ⌘⏎ publishes it. Leave without typing and nothing is kept. `new-note = draft` makes ⌘N start a draft instead; the omnibar's ⏎-to-create is unchanged.
- **The preview matches studio 0.36.1** byte for byte. An `[[id]]` link to a post that opens with a heading is labelled with that heading, without quotes; other links still show a quoted excerpt. A TK scope at the very start or end of a post shows as a block even with a stray newline or spaces between it and the edge. A link to, or quote of, a post whose blyg gives it a full permalink (a templated blyg) goes to that permalink. The Reader labels links in a post the same way.

## [0.9.0] - 2026-10-06

### Changed

- **Burrow follows blygger-studio 0.32's owner API.** The tests check against 0.32.1's `openapi.json`, and the end-to-end tests run against a real local studio 0.32.1.
- **Sign in with browser.** On studio 0.28 or later, connecting opens your studio's sign-in in the browser and asks you to allow Burrow (OAuth). Burrow keeps the grant in your Keychain and renews it; after 30 days, or if you revoke it under Client access, it asks you to sign in again. An **API token** from the studio's Client access page works too, as does a server's own owner token; the password stays for older studios. Studio 0.28 signed every session out, so a password sign-in has to happen once more.
- **Reply is the one response action.** A stub starts quoting the whole post; in the stub, **quote a passage instead** shows the post and the passage you select becomes the quote, and further passages go in as their own quotes. "Reply with this" is gone from the selection pill; "Quote in draft" (⇧⌘D) stays, and into a stub of the same post it sets or adds a passage.
- **Cheaper, quicker sync.** On studio 0.32, Burrow asks the blyg what changed before reading anything, so a pull with nothing new is one small request, and it checks every 15 seconds.
- **AI disclosure works on any studio 0.28 or later.** Text generated in Burrow is recorded with the post through the studio's own API; the Worker extension for it is only a fallback now.
- **Generated text from your blyg's own AI** goes straight into the editor.
- **The preview matches studio 0.32.2** byte for byte again, including its sanitizer for threads. A TK scope's sources are the posts its instruction names, not ones its output adds, as studio 0.32.2 records them.
- **The stock-server notice is shorter.** On a current studio, only read state across Macs needs an extension; AI disclosure (0.28), removing deleted images (0.26) and replies and forks (0.18) are upstream now.

### Added

- **Mark read and unread, several at once.** In the Reader, ⌘-click, ⇧-click and ⌘A pick posts; **r** and **u** (or the right-click menu) mark them read or unread. Marks sync in batches; unread syncs on a server that supports clearing read state, and otherwise stays on this Mac.
- **Subscription names follow their source** (studio 0.30). **Rename…** gives one your own name, and **Use the Blyg's Own Name** hands it back. **Check Now** checks one; **⇧⌘R** (Blyg › Check All Feeds Now) has your blyg check them all.
- **Highlight generated text** (studio 0.27): set your blyg's default in its site settings and override it per post from the ⌘G palette (**H** cycles default, on, off). The preview shows it as the page will.
- **A full-text picker for `[[` and `![[`.** Every word of your published posts and the blyg posts you read, searched on your Mac, filtered by source and blyg, newest or oldest first. Typing `[[` now opens it for links too. The blyg's *picker typing* setting chooses whether you type in the editor or in the picker.

### Fixed

- **A change the server rate-limits is no longer dropped.** Studio 0.28's work budgets answer 429; Burrow waits as asked and sends it then. Typing saves at most every three seconds per post.
- **A change your token lacks the access for waits** instead of being dropped, and Burrow says which permission it needs.
- **Images in drafts show again** on studio 0.28 or later, which keeps unpublished uploads private: Burrow reads them as you, from your blyg only.

## [0.8.2] - 2026-10-04

### Fixed

- A long title no longer runs past the hexagon in the lineage view: it fits two lines, broken between words.

## [0.8.1] - 2026-10-04

### Fixed

- The "What's new" card and the lineage view's side panel wrap their text instead of running past the edge.
- The lineage glyph shows in Reader mode too: on each list row and in the open post's header line.

## [0.8.0] - 2026-10-04

### Changed

- **Burrow follows blygger-studio 0.26's owner API.** Nothing it used changed; the tests now check against 0.26's `openapi.json`.
- **Who a post replies to, or forks, comes with the post** from studio 0.18 or later (and from the reading rows of a server with Burrow's extensions that say `lineage: true`), so Burrow no longer fetches each post's own document for it. Older servers work as before.
- **One response per post.** A reply that also quotes the post it replies to is listed once, as a reply; a fork outranks both. A reply or quote of a passage says so ("stubbed a passage of this").
- **Your own posts count as responses** to the posts they quote, reply to or fork.

### Added

- **Lineage.** A glyph beside each post's name in the stream shows which kinds of post it draws on and which draw on it (fork, reply, quote), never how many; it replaces the "↩" marker. **⌘J** (Post › Lineage…) opens the post's lineage: what it draws on above, its responses below, walkable one step at a time with ⏎ and ⌫.
- **Actions on a ring.** Space on the post in the middle of the lineage view opens fork, reply, quote, link post, versions and open in fixed places (the web Studio's layout). A letter previews what the action would make and what it means for the author; ⏎ does it.
- **What's new.** The first launch after an update opens the tutorial on what changed, with the choice to start at the new parts, take the whole tour, or skip it. The tour teaches lineage and the ring.

## [0.7.0] - 2026-10-02

### Changed

- **Burrow follows blygger-studio's documented owner API (studio 0.9 to 0.11).** Upstream redesigned `/api` as an OpenAPI contract and dropped the old routes, so saves, forks, pins, response settings, subscription pause and resume, mention hiding and site settings now use the new routes (`PATCH`, `POST /api/items {mode}`, `PUT …/versions/:v/pin`), and lists are read page by page. Burrow now needs studio 0.9 or later; 0.11 is current.
- The reading list loads from `/api/reading/imported`; upstream's `/api/reading` is now a different resource. Permalinks follow your blyg's site URL when it has one.
- **Sign in with your studio password, so any blygger-studio works.** The Connect sheet takes the password you use in your blyg's studio (or, as before, the owner token). The password is kept in your Keychain; Burrow signs in again by itself when the 30-day session ends.
- **A stock server is fully usable.** Without Burrow's server extensions, the reading list is built from upstream's own routes. The first time Burrow meets such a server it explains what's limited (AI disclosure for text generated in Burrow, read state across Macs, removing deleted images), and the About window lists it too.
- **Who a post replies to, or forks, shows in the reader** for posts from other blygs: Burrow fetches it from the post's own blyg, a few posts per sync.
- **An outdated server gets a clear message, not damage.** If your blyg's server is older than studio 0.9, Burrow says so when it starts, keeps a status-bar notice with a **How to update** link, and pushes nothing until the server is updated. Your edits wait and go up afterwards. The connect sheet names the problem too.
- Changing a draft between fragment and thread keeps the same draft on the server instead of making a new one.
- The tests check every request Burrow sends, and every reply they fake, against upstream's `openapi.json`, and the routes upstream lacks against a contract of their own.

### Fixed

- A blyg that lives under a path (`https://example.com/blyg`) now connects: its API is at the host root, which Burrow used to miss.
- A quoted post from another blyg shows its images in your draft's preview. Posts published before studio 0.11 use relative image paths, which used to resolve against your own blyg.
- `docs/SERVER.md` now lists only what upstream still lacks: bearer-token sign-in, the reader's reading rows, client-recorded AI provenance, read-state sync and attachment removal.

## [0.6.2] - 2026-09-30

### Fixed

- **Themes now carry through the whole app, and text reads everywhere.**
  - Konkan's subscriptions column showed dark text on dark. So did its selected rows in the Reader and the stream, Kumiko's title bar, and several menus, popovers and status-bar notices on the darker themes.
  - Every surface now measures its text against its own background. A new test checks each built-in theme.
- The Reader's subscriptions column draws the theme's sidebar ornaments. The theme's selected-row style, quote frames, markers and chip and sheet shapes now apply in the Reader, the stream, Mentions, Subscriptions and profiles, as they do in the posts list.
- The reading pane shows posts in the theme's writing font (e.g. ET Book in Portolan), with quote boxes, highlights and diff marks in the theme's colours.
- Sheets, cards, popovers, text fields, tooltips and chips take the theme's corners, edges and fonts. Portolan's sheets have a true double rule.
- Aizome's selection in the Reader's list and the stream was invisible, because the selected colour was the same as the page's.
- Cutaway's title bar is a deeper green, so its labels read.

## [0.6.1] - 2026-09-29

### Added

- **Themes.** Eight new themes join Paper and Ink, and they go beyond colour with ornament. The woody ones are **Cutaway** (a lit burrow room in a soil cross-section), **Kumiko** (Kyoto lattice and kintsugi), **Shola** (a mossy Western Ghats thicket) and **Fortress** (Dwarf Fortress stone and glyphs). The oceanic ones are **Portolan** (a chart with rhumb lines and soundings), **Aizome** (indigo, seigaiha waves, sashiko), **Saltspace** (weathered greys and tide lines) and **Konkan harbour** (a lit harbour in a night sea, with a tide sparkline of your writing). Pick one in Settings or with `theme = <name>`, and use `theme-dark` for another when macOS is dark. Themes are plain files: `blygger +copy-theme <name>` copies one into `~/.config/blygger/themes/` to edit, and it reloads when you save. Ornament slots can use your own SVG. `blygger +list-themes` lists them. The studio preview still shows your blyg's own style.
- The config file now reloads when it's edited outside the app (it used to reload only on ⌘⇧,).
- A documentation site at https://aneeshsathe.github.io/blygger-desktop/, and a shorter README.

### Changed

- **Blygger is now Burrow.** Burrow is a blygger client. The app, its menus (Burrow › Settings…, Burrow › Check for Updates…, Help › Burrow Tutorial), the About window, the window title and the app's messages say Burrow, and new installs are `Burrow.app`. Release assets are named `Burrow-<version>-macos-universal.{zip,dmg}`.
- Only the name changed. The bundle ID (`org.blygger.desktop`), your data folder and database, the Keychain items, the config file (`~/.config/blygger/config`), the `BLYGGER_*` environment variables, the `blygger` command and the key-binding contexts are all the same, so nothing needs migrating.
- **Updating from 0.6.0 or earlier:** the in-app update works as usual. The app replaces itself where it is, so it stays `Blygger.app` on disk (Finder and Spotlight show that name) while being Burrow inside. That's expected. To get `Burrow.app`, run the one-line installer, which replaces a `Blygger.app` in the same folder (posts, settings and sign-ins carry over), or rename the app while it isn't running. A Dock shortcut to the old name may need re-adding.
- The updater accepts a release zip named `Burrow-…` or `Blygger-…`, holding `Burrow.app` or `Blygger.app`, and prefers Burrow's. For a transition period, each release also publishes `Blygger-<version>-macos-universal.zip` and `Blygger-macos-universal.zip` (the same signed app in a folder called `Blygger.app`) so 0.6.0 and earlier can still update. The signed `SHA256SUMS` covers them too.

### Fixed

- Selecting text with the mouse in the reading pane and the preview works again: the selection stays until you quote it (⇧⌘D, or the new **Quote in draft** pill beside it), and dragging inside a quote no longer opens the original. ⌘C copies a selection in the reading pane. In the browser, **→ Draft** and **→ Notes** keep the page's selection, and the button reads **❝ Quote → Draft** while text is selected. In the stream, dragging over or double-clicking a post opens it where its text can be selected. The notes drawer's footer says when ⇧⌘D will quote your selection.

## [0.6.0] - 2026-09-29

### Added

- **Universal quoting.** Highlight text in the built-in browser, in notes or in the reading pane, then press ⇧⌘D (or **→ Draft** in the browser, or Post › Quote Selection in Draft). The passage goes into the open draft at the caret, or into a new draft, with its source. A post from a blyg you follow, quoted into a thread, becomes a partial quote of that post. Anything else becomes a blockquote followed by a link to the page.
- **Partial quotes (blygger-studio 0.8).** `![[id]]` followed directly by `>` lines quotes just that passage. A blank line in between makes it a whole quote plus your own blockquote. The preview flags a passage that isn't in the quoted post, and publishing refuses it. In the stream, a partial quote shows only its passage.
- Replying with a passage selected in the reading pane starts the reply as a partial quote of it. Replying to a long post with nothing selected leaves an empty quote line to fill in.
- **Link post:** a new action next to Fork that starts a fragment linking to the post (`[[id]]`) without responding to it. Reply is still the one way to respond.
- **Responses:** each post on the Mentions screen can follow the blyg's site setting, or show or hide its responses itself.
- **Site settings:** a time zone for dates on your pages, and **Show responses by default**, on blygs that report them.

### Changed

- The preview matches blygger-studio 0.8.3.
- Titles never come from quoted text. A reply with nothing of its own yet shows as "In response to `<host>`".

## [0.5.1] - 2026-09-28

### Fixed

- The one-line installer (`scripts/install.sh`) failed on stock macOS with
  `ASSET: unbound variable`. `/bin/bash` 3.2 in a UTF-8 locale reads the first
  byte of the `…` after `$ASSET` as part of the variable name. Braces fix it.

## [0.5.0] - 2026-09-28

### Added

- **`[[id]]` links (protocol 0.3).** An inline `[[id]]` links to one of your posts or one you read, without quoting it. The preview shows it as the published page will: the target's first words in quotes, linked. Links inside code stay as text. A link that can't be resolved is flagged in the preview, the status bar and the publish sheet, as quotes are; publishing refuses it. In the stream, a link reads as the post it points to and opens it.
- Quote boxes in the stream show the citation the quoting blyg recorded (`cited`) when the quoted post isn't held on this Mac.
- Site settings: an **Accept mentions** option, on blygs that report it.

### Changed

- The preview matches the reference client at blygger-studio v0.7.0 (protocol 0.3). The publish error for unresolvable quotes or links is now "one or more references do not resolve".

## [0.4.1] - 2026-09-27

### Changed

- When an update has downloaded and passed its checks, a sheet asks **Restart Now** or **Later**; in `auto-update = notify` mode it offers **Download and Install**. Check for Updates… asks again, and the status bar notice stays after Later.
- Tour: steps that open a pane, popup or mode (the full editor, versions, @ mentions, the stream pane, the original, Reader, notes) show ✓ and wait for Next instead of moving on by themselves. You can now pick someone from the @ popup.

### Fixed

- Opening the profile of some blygs crashed the app (a feed with a multi-byte character just after an `&…;` entity). Profile and original-post fetches now show an error instead of crashing if a page can't be read.
- Right-clicking a misspelled word crashed the app.

## [0.4.0] - 2026-09-27

### Added

- **The stream is the new default reading view.** Reading now opens as one
  scrolling timeline, newest first, drawn natively without a web view per
  post. Fragments show in full; threads show their title, a few lines and
  "Read more". Quotes appear as grey boxes with the quoted text; images and
  videos are small placeholders. Use j/k to move, ⏎ or Space to open the
  whole post in a side pane, and esc to close it; the stream keeps your
  place. A post counts as read once it has been on screen for a second, and
  unread posts get a dot. The Stream | Reader toggle (⌥⌘1 / ⌥⌘2) brings back
  the list-and-post layout and is remembered. Search (⌘F or /) filters the
  stream too.
- **Quotes open the post they quote.** Clicking a quote's text, in the stream
  or the reader, opens the original post beside it, at the quoted version
  when that version is pinned. The footer reads "quoted from `<name>` · v2 ·
  open original", and the name still opens the profile. "↳ stub of …" and
  "⑂ forked from …" work the same way. A post you don't follow is fetched
  from its author's public files, without your token, with a Subscribe
  button.
- **See who responded to a post.** The bottom of the reading pane lists who
  quoted, stubbed or forked the post you're reading, and when, from the posts
  already in your reading list. Your own posts list their verified mentions
  the same way. It's a list, never a count; the stream marks posts that have
  responses with a small ↩.
- **Reader: three panes, smart feeds and folders.** Reader mode (⌥⌘2) is
  laid out like NetNewsWire: sources on the left (smart feeds All unread,
  Today, Thumbed and All, then your folders, then subscriptions in no
  folder), the post list for the source you pick in the middle (⌘F or /
  searches within it), and the post on the right. Folders live on this Mac
  only and are never sent to your blyg: make one with Blyg › New Folder…,
  file a subscription by dragging it onto a folder, with "Move to folder ›",
  or on the Subscriptions screen; rename, reorder and delete them. ←/→ move
  between panes, j/k between posts, Space scrolls the post and then opens
  the next unread one, and ⌥⌘S hides the sources pane (drag its edge to
  resize it). Version stepping in the Reader moved to [ / ].
- **Notes drawer** (⇧⌘N, View › Notes): a scratchpad that slides in over the
  right edge while you read the Stream, the Reader, the browser pane or your
  Posts, and slides away with esc, ⇧⌘N or a click outside. Your notes are a
  local scratch note, "Reading notes", saved as you type and listed in
  Posts; ⌘D makes them a draft and ⋯ › New notes page starts a fresh page.
  **→ Notes** on a post adds it (`![[…]]` for a blyg post, a link for a feed
  post), the browser pane's → Notes adds the page's link, and text selected
  in a post or web page goes in as a quote with its source.
- **In-app browser.** Links in posts open in a browser pane that slides in
  from the right over the reading view: ⌘-click opens it over the whole
  reading area, ⌥-click opens your default browser, esc closes it, and ⇧⌘B
  brings back the last page. It has back, forward and reload, an address
  field that shows the page title, a progress bar, "Open in default
  browser", and "→ Notes", which adds the page's link to your notes. The
  pane has its own cookies (kept across launches on macOS 14 and later) and no bridge
  into the app, so pages never see your token. `open-links = browser`
  restores the old behaviour.
- **Ad and tracker blocking** in the browser pane, with uBlock Origin's
  default filter lists (uBlock filters, EasyList, EasyPrivacy and Peter
  Lowe's list) run as WebKit content blockers. The lists are downloaded about
  once a week and compiled in the background; a small built-in list covers
  you until the first download. 🛡 turns blocking off for one site, and
  `content-blocking = false` turns it off everywhere. Scriptlet filters
  can't run in WebKit content blockers and are skipped.
- **Spellcheck.** Misspelled words get a red wavy underline, using the macOS
  spell checker with your system languages and the words you've taught it.
  Right-click (or ctrl-click) one for suggestions, Learn Spelling and Ignore.
  Code, links, addresses, quotes (`![[…]]`) and TK markers are never checked.
  Checking happens in the background after you pause, only on the lines you
  changed, so typing stays instant. Turn it off with Edit › Spelling › Check
  Spelling While Typing, or `spellcheck = false`.
- **@-mentions.** Type `@` at the start of a word, in the editor or quick
  capture, to pick from the blygs you know: subscriptions, blogroll, authors
  in your reading, and profiles you've opened. ↑/↓ choose, ⏎ or ⇥ insert,
  esc keeps what you typed. Blyg has no handles, so a mention is a plain link
  to the blyg, `[Name](https://…/)`, and it notifies no one.
- **About Blygger** (Blygger menu and Help): version, commit, build date,
  architecture and macOS version; the auto-update setting, last check and
  whether an update is ready, with Check for Updates…; the blyg you're
  connected to (host only) and what its server supports; the data folder and
  config file with Reveal in Finder; links to the repository, this version's
  release notes and the license. **Copy build info** copies a summary for bug
  reports, without your token or file paths.
- **The tutorial covers the new reading features:** the Stream, quotes that
  open the original, who responded, Reader with sources and folders, the
  Notes drawer, links in the browser pane (a blank sample; nothing loads),
  and @-mentions and spellcheck while writing. Its highlight steps aside for
  panes and popups, and the tour puts your reading mode, notes and browser
  page back when it ends.

### Fixed

- **Reading shows each post's own date** (not when your blyg first imported
  it), so a new subscription no longer makes every post look like it arrived
  today. The list is sorted by when the author published or last edited a
  post; edited posts say "edited 3d ago" and still move to the top.

## [0.3.0] - 2026-09-25

### Added

- **Automatic, signed updates.** Blygger checks GitHub for a new release at
  launch and about once a day, downloads it in the background, and shows
  "Blygger X is ready · Restart to update" in the status bar (quitting
  installs it too). **Blygger › Check for Updates…** checks right away. An
  update is installed only if its `SHA256SUMS` has a valid Ed25519 signature
  from the project's release key, the zip matches it, and the new app is
  `org.blygger.desktop`, newer, and passes `codesign --verify`; otherwise
  nothing changes. `auto-update = install | notify | off` (default install)
  in the config. Releases now include `SHA256SUMS.sig`, and the one-line
  installer checks it when OpenSSL 3 is installed. People on 0.2.0 or
  earlier need to run the installer once more.
- **Read state syncs between your Macs.** A post you read on one Mac now reads
  as read on your others, and a fresh install no longer shows everything
  unread. It syncs through your own blyg when the server supports it (the
  optional owner-API extension 5, see `docs/SERVER.md`). Marking a post read
  is still instant and works offline; the change is sent when you're back
  online, and read state never goes backwards. The first sync uploads what
  you've already read on this Mac. On a server without the extension,
  nothing changes.

## [0.2.0] - 2026-09-25

### Added

- **Typing `![[` opens the quote picker** (#5). At the start of a line in a
  thread, `![[` opens the same picker as ⌘K, and what you type next filters
  it. ⏎ inserts the `![[id]]` line; esc puts the `![[` back. Pasting, fenced
  code and fragments don't trigger it (a fragment shows the threads hint).
- **Delete a draft** (#2). ⇧⌘⌫, Post › Delete Draft…, or the toolbar's
  Delete button deletes the current draft or scratch note after a short
  confirmation (⏎ deletes, esc cancels). The list moves on to the next post.
- **Withdraw a published post.** Post › Withdraw…, or the same toolbar slot
  on a published post, asks first, takes an optional note, and withdraws it.
  Withdrawn posts stay listed as withdrawn, and it can't be undone. Published
  work is never deleted, and ⇧⌘⌫ on a published post says so.
- **Search the reading list** (#6). ⌘F or `/` puts the caret in a search field
  above the reading list; typing filters it by title, author or blyg name, and
  text, case-insensitively, over posts already held on this Mac (nothing is
  fetched to search). Matches are highlighted, ↑/↓ and ⏎ work from the field,
  esc clears the search, and "No posts match “…”" says when nothing does.
- **Paste a link over selected text**, as in WordPress. Select some words
  and paste a web or mail address: they become `[words](address)`. Pasting
  anything else, or over a selected address or link, pastes as usual.

### Changed

- **Reading actions say what they create** (#3). The actions under a post now
  read "Quote into a thread" (adds `![[id]]` to a thread of yours), "Reply ·
  new stub" (a stub thread, `stub_of`) and "AI reply · new stub", each with a
  one-sentence tooltip. Fork shows on the current version too, greyed out,
  explaining that forks descend from pins only and pointing to the 📌 in
  ‹ vN ▾ › when the post has one.

### Fixed

- **Typing stopped working after the window sat in the background** (#4).
  A preview could keep the keyboard, or drop it to nowhere, so only menu
  shortcuts like paste worked. The app now takes the keyboard back when the
  window becomes active again and before hiding a preview.
- **The tour's Quotes step ringed the editor over the quote picker** (#1).
  A step's ring now hides while a sheet or picker covers the panes.

## [0.1.0] - 2026-09-25

The first release: a local-first writing studio for a Blygger blog.

### Added

- **One window, Notational Velocity style.** An always-there omnibar filters
  your posts as you type, with matches highlighted. ↑/↓ preview each post, ⏎
  opens it, and ⏎ with no match starts a draft seeded with the query.
- **Side-by-side list and editor.** Plain Markdown with no toolbar. Every
  keystroke saves locally, and changes sync to your blyg about 800 ms after you
  stop typing. There is no save button.
- **Fragments and threads.** ⌘T toggles the kind. The status bar shows a live
  1000-character counter for fragments (amber past 900, red past 1000) and
  says how to fix a fragment that's too long.
- **Publish with an optional version note** (⌘⏎), from a sheet under the
  title bar, with a toast that links to the published post (⌘O opens it).
- **Offline first.** Nothing waits on the network. Edits queue while you're
  offline and flush when you're back, and the status bar shows how many are
  waiting.
- **Conflict resolution.** When the server changed under you, a side-by-side
  sheet shows both versions: keep mine, take the server's, or keep both.
- **Images.** Paste or drop an image to upload it and insert its Markdown
  link.
- **Quick capture into scratch notes.** A global hotkey (⌃⌥B by default,
  configurable) opens a floating panel over any app. esc keeps a *scratch
  note* that lives only on your Mac (images included); ⌘D makes it a draft on
  your blyg and ⌘⏎ publishes it. Scratch notes are searchable and editable in
  the main list.
- **Full editor with a faithful preview.** ⌘1 write, ⌘2 list + editor +
  preview, ⌘3 editor + preview (⌘E toggles). The preview is a Rust port of the
  reference Worker's renderer (tested byte-for-byte against it): quotes of
  other posts, the AI-text tint, YouTube and remote images, in your blyg's own
  stylesheet.
- **Reading.** Your reading list with one entry per post however often it's
  edited; edited posts move to the top with the author's notes. Posts show as
  their blyg published them (sanitized). Subscriptions, mentions (a list,
  never a count), thumbs, and your blyg's site settings.
- **Versions and pins.** For other people's posts the `‹ vN ▾ ›` pill steps
  through only the current and pinned versions. ⌘Y shows your own full
  history with restore, and pinning asks you to confirm in words.
- **Quote, reply and fork.** ⌘K quotes a post you hold into a thread; Reply
  makes a stub; forks descend from pins only.
- **Profiles.** ⌘I (or click an author, a quote, or the stub/fork line) shows
  who someone is: bio, links, their blogroll, recent posts and the blygs they
  quote, stub or fork. Follow in one click; add to your blogroll separately.
  Fetched from public files only when you open one, never with your token.
- **Optional toolbar buttons** (`show-buttons`), generated from the same table
  as the shortcuts and menus, so every tooltip teaches the key.
- **First-run onboarding and a replayable tutorial** that runs on sample data.
- **Tufte-inspired theme** that follows the system's light or dark mode.
- **Switchable fonts** for writing and for the UI. Literata, Inter, Source
  Serif 4, iA Writer Quattro and ET Book are bundled, and system fonts are
  also offered. ⌘+ and ⌘− change the size.
- **AI helpers (TK)** that work with your own provider accounts: Anthropic or
  OpenAI API keys, Cloudflare Workers AI, a local Claude Code or Codex
  install, your blyg server, or Sign in with ChatGPT (unofficial). ⌘G fills a
  `[TK]` gap, ⇧⌘G shortens to fit, and a palette offers continue, outline,
  proofread and an AI reply. Generated text is always disclosed as generated.
- Keychain storage for the owner token and API keys. Secrets never go in a
  file.
- A universal macOS app (Apple silicon and Intel), shipped as a dmg and a
  zip with SHA-256 checksums.
