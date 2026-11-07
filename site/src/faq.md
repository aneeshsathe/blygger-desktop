# FAQ

### Why is it called Burrow, when everything says "blygger"?

Burrow is a blygger client: a client for the Blygger protocol. It was called
Blygger Desktop up to 0.6.0. Only the name you see changed. The bundle ID, the
`blygger` command, the config path (`~/.config/blygger/config`), the
`BLYGGER_*` environment variables, the data folder and the Keychain items kept
the old name, so existing installs carry on without migrating. See
[The rename](install.md#the-rename).

### My app is still called Blygger.app in Finder.

That's expected if it updated itself from 0.6.0 or earlier: the updater
replaces the app where it is, so the file keeps its name while being Burrow
inside. Run the [one-line installer](install.md) to get `Burrow.app`, or
rename it while it isn't running. See
[Blygger.app → Burrow.app](install.md#blyggerapp--burrowapp).

### Can I use Burrow with a stock Blygger blog?

Yes. Burrow 0.16 needs blygger-studio 0.49.12 or later, stock and unpatched.
Sign in with your browser, an API token from the studio's Client access page
(or, once connected, Burrow's **Blyg › Client Access…**), or your studio
password. On an older blyg nothing syncs: your posts stay on
your Mac, and Burrow says the blyg needs updating, then sends everything once
it is. See [Server requirements](server.md).

### Is my data safe? Where is it?

Your posts live on your blyg; Burrow keeps a local copy in
`~/Library/Application Support/org.blygger.desktop/`, so everything works
offline. Scratch notes and the notes drawer stay on your Mac until you make
them drafts. Your sign-in (browser grant, token or password) and AI keys are in the macOS Keychain, never in
a file, and never logged.

Burrow is vibecoded and provided as is, with no warranty. Keep backups of
your blyg.

### Does Burrow send anything anywhere else?

Only what a feature needs, when you use it:

- your blyg, with your sign-in;
- other blygs' **public** files, without your token, when you open a profile
  or a quoted post you don't hold;
- GitHub, about once a day, to check for updates (`auto-update = off` stops
  it);
- the AI provider you switched on, only when you ask it to generate;
- the filter lists for the browser pane's content blocking, about once a
  week (`content-blocking = false` stops it);
- the sites behind links you open, images in posts, and YouTube (through
  `youtube-nocookie.com`) when you click a video.

Burrow has no analytics or telemetry.

### Why can't I delete a published post?

Blygger doesn't delete published work. **Post › Withdraw…** withdraws it: the
post stays listed as withdrawn, visibly, and that can't be undone. Drafts and
scratch notes can be deleted (⇧⌘⌫).

### Why can't I fork this post?

Forking descends only from **pinned** versions. Pick a pin in the post's
version pill (`‹ vN ▾ ›`, or press [), then **Fork this pin**. If the post has
no pins, it can't be forked. Your own posts fork the same way, from ⌘Y: pick
a pinned version, then **Fork this pin**.

### How do I undo my edits to a published post?

**Discard changes** (⌥⌘Z, Post › Discard Changes…, or the toolbar's eraser)
puts the editor back to the version your blyg has published, after asking.
The public post doesn't change. On Windows it has no key: use the Post menu
or the toolbar. See [Discard changes](writing.md#discard-changes).

### What's the text after `|` in a link?

The link's **label**: `[[id | Gardens, not streams]]` reads "Gardens, not
streams" on the published page. Edit it to rename the link. The `[[` picker
writes it for you, from the post's heading or title. In a quote,
`![[id | note]]`, it's a note only you see. See
[Labelled links](writing.md#labelled-links).

### Why does Burrow say "Your blyg needs updating"?

Your blyg runs a studio older than 0.49.12 (or a patched Worker), whose API
Burrow no longer speaks, so nothing syncs until the blyg is updated. Your posts
and changes stay on your Mac and are sent once it is. Burrow's status bar has
an **Update your blyg** link. See [Server requirements](server.md).

### Why won't my quote go in?

Quotes (`![[id]]`, or `![[id | note]]`) only go in threads; ⌘T makes a
fragment a thread. A
partial quote's passage must be in the quoted post exactly as written; the
preview flags one that isn't, and publishing refuses it.

### Why doesn't Burrow show follower counts or likes?

Blygger shows responses as a list, never a count, and following is private.
Burrow follows the protocol's rules: no counts anywhere social. The one
exception is the [lineage glyph](reading.md#lineage), which says how many
posts a post draws on and how many draw on it, the same numbers the web
Studio shows.

### Do I need to sign in again after updating?

Usually not. Only if your sign-in was made before your blyg ran
blygger-studio 0.39, and you read on more than one Mac: the studio keeps read
state behind its own permission (`reading:state`), which an older sign-in
doesn't have. Until you sign in again, posts you read are marked read on
this Mac and wait to be sent; everything else syncs as usual, and Burrow says
once that read state is waiting. To sign in again: **Burrow › Disconnect…**
(keep the local copy), then connect with **Sign in with browser** and allow
every permission. With an API token, make a new one that includes
reading:state in **Blyg › Client Access…** (or the studio's **More › Client
access**).

### Why did Burrow open the sign-in sheet by itself?

Your blyg refused Burrow's sign-in: an API token expired (they last 30 days)
or was revoked, a browser sign-in ended, or the server changed (a blyg that
moved from the old patched Worker to a stock studio no longer takes the
old owner token). Burrow stops syncing rather than retrying, keeps every
change on your Mac, and says why. Sign in with your browser, or paste a new
token (made in the studio's **More › Client access**, or in another Burrow
connected to the blyg, under **Blyg › Client Access…**), and what waited is
sent.

### Who can act as me on my blyg? How do I revoke a token?

**Blyg › Client Access…** lists every app and token that can act as you:
browser sign-ins (Burrow's own is marked "this Burrow"), MCP apps and manual
API tokens, with what each may do and when it ends. ⌫ revokes the highlighted
one and ⇧⌫ revokes them all, each after a confirmation that warns when this
Burrow would be signed out too. `n` makes a new token: a name, REST API or
MCP, and its permissions (read only, to start). The token appears once, with
Copy; Burrow never keeps it. Your blyg shows client access to its own sign-in
only, so if Burrow signs in with the browser or a token it asks for the
studio password once, and keeps that sign-in in memory until it quits.

### Can I bring my feeds from another reader?

Yes. **Blyg › Import Subscriptions from OPML…** (or **Import OPML…** on the
Subscriptions screen, ⇧⌘S) reads the OPML file most feed readers export:
tick the feeds you want, and Burrow subscribes them a few at a time, files
them in an **Imported feeds** folder in the Reader, and keeps them out of
your public blogroll. **Blyg › Export Subscriptions as OPML…** goes the
other way. See
[Import and export subscriptions](reading.md#import-and-export-subscriptions-opml).

### Can I sign in with my Claude Pro/Max subscription?

Not directly: Anthropic's terms don't allow third-party apps to use claude.ai
logins. Install Claude Code and switch on the local Claude Code provider
instead. See [AI helpers](ai.md).

### macOS says Burrow "can't be opened" or "Not Opened".

Releases aren't notarized yet. The one-line installer avoids this; for a
browser download, see
[Downloading from the Releases page](install.md#downloading-from-the-releases-page-in-a-browser).

### How do I report a bug?

**Burrow › About Burrow › Copy build info** copies the version, commit, update
status and what your server supports (no paths, no token). Paste it into a
[GitHub issue](https://github.com/aneeshsathe/blygger-desktop/issues).
