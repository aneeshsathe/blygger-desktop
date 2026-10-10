# Extensions

An extension adds commands, or a library of documents, to Burrow. It's a
separate program that Burrow starts and talks to. **Nothing runs unless your
config file names it**, and an extension gets only the permissions you grant
it.

Burrow comes with two extensions, **markdown-notes** and **cross-post**. You
can also install extensions other people write, or write your own.

## markdown-notes

markdown-notes reads and writes folders of Markdown notes, such as Obsidian
vaults, as many as you like. It's kept apart from your blyg, so it works with
no blyg connected, and it never links notes to posts.

- **The notes panel** (the notes drawer's Notes tab) switches between your
  folders, lists the one you pick, searches titles and bodies, and opens a
  note to read or edit. **Add folder…** adds more; **Remove from Burrow**
  forgets a folder (its notes stay where they are).
- **Copy into post** puts the selection, or the whole note, into your draft at
  the cursor, as a quote or as it is (you pick).
- **Save selection to notes**, in the Extensions palette (⇧⌘P), makes a new
  note from the text you've selected.

It never adds frontmatter to a note (any frontmatter it finds is left as it
is), and it never deletes a note. If a note changed on disk after you opened
it, your save is refused, so it never overwrites the other change.

### Turning it on

The easy way: **Settings › Notes folders › Add folder…**, then pick one or
more folders. Burrow writes the config lines for you and asks for permission
to use those folders.

Or add the lines to the config file (**Burrow › Open Config File**) yourself,
then reload it (⇧⌘,):

```text
extension = markdown-notes
extension-setting = markdown-notes vault=~/Notes
extension-setting = markdown-notes vault-work=~/Work/Vault
```

`vault` is your first folder of notes, and each `vault-<label>` adds another
(the label is lowercase letters, digits and dashes). There's no default: with
no folder, the Notes tab offers **Choose a folder…**, and Burrow never reads a
folder you didn't pick. It has two more settings:

```text
# where new notes go, inside each folder (default: the top of the folder)
extension-setting = markdown-notes folder=Inbox
# how often it looks for changed notes, in milliseconds (default 2000)
extension-setting = markdown-notes poll-ms=5000
```

## cross-post

cross-post puts a post you've published on Substack Notes too: its opening
paragraph and the link back to your blyg. It runs in Burrow's browser pane,
where you're signed in to Substack, and it never posts without you clicking
Post.

### Turning it on

```text
extension = cross-post
extension-allow = cross-post items.read
extension-allow = cross-post ui
extension-allow = cross-post browser.automate:https://substack.com
```

The last line is the one that matters: it lets cross-post fill in, and when
you confirm, click Post on substack.com, signed in as you. It names that one
site exactly, so it can't act anywhere else. If you leave the `extension-allow`
lines out, Burrow asks for them the first time it starts cross-post.

Sign in to Substack **once, by hand**, in the browser pane: press ⇧⌘P and
pick **Open Substack Notes** (or View › Open Browser… and type
`substack.com`). The pane remembers it (on macOS 14 or
later; on older macOS, sign in before each run). cross-post never
signs in for you, never sees your sign-in, and stops with "Sign in to Substack
in this pane" if you're signed out.

### Cross-posting

With a published post open, press ⇧⌘P and pick **Cross-post to Substack
Notes…**. Every run shows **two confirmation sheets**, and no setting skips
them:

1. **The text.** You see the note it will post, and can edit it. Continue
   opens your Substack home feed in the pane, clicks "What's on your mind?",
   and puts the text in the note composer that opens.
2. **Post this to Substack Notes?** You see what the notes box now holds. Post
   clicks Substack's Post button; you can also click Post yourself, or cancel.

The run uses the same side pane as a link you click, with the sheets beside
it so you can see the page. You can fold the pane (esc, ×, or ⇧⌘B) while it
runs: the run keeps going, and the status bar shows **macro running · show
pane**. Click that, or press ⇧⌘B, to bring the pane back. The pane comes back
by itself before the text goes in, and for the second sheet. Afterwards
it's folded again if it was folded before, unless you folded or unfolded it
during the run, or you still need the page (you chose to click Post
yourself, or the run stopped with a problem).

The note is the post's first paragraph of prose (headings, quotes, images and
code are skipped, and Markdown marks are taken out), a blank line, and the
link. The whole note is at most 280 characters, the link included, counted as
a reader counts them (an emoji is one, and is never cut in half); the
paragraph is cut on a word boundary with "…" to fit. Two settings change it:

```text
# the note: {{excerpt}}, {{title}} and {{permalink}}; \n is a line break
extension-setting = cross-post template={{title}}\n\n{{excerpt}}\n\n{{permalink}}
# the longest note, link included (default 280)
extension-setting = cross-post max-chars=500
```

### Before you rely on it

- **The recipe was last checked on 2026-10-09** (`tested = "2026-10-09"`):
  it opens Substack's home feed, clicks "What's on your mind?", pastes into
  the composer that opens, and clicks its Post button. Substack can change its
  page at any time. When a step doesn't find what it looks for, the run stops, says
  which step, and **nothing is posted**. `blygger +list-extensions` shows each
  macro's `tested` value. The extension's `macro.log` (in Burrow's data
  folder, under `extensions/cross-post/`) then lists the text boxes and
  buttons the page did have (tags, classes, labels such as "Post"; never the
  page's text or yours), so the recipe can be fixed. Start Burrow with
  `BLYGGER_MACRO_TRACE=1` to log that after every step.
- **One post per click.** cross-post posts once each time you confirm. It
  never posts on a schedule, in the background, or again by itself, and it
  waits at least 60 seconds between two runs.
- **It's your account.** cross-post acts as you on Substack, the same as if
  you typed and clicked yourself. If Substack limits or acts on an account
  for automated posting, that's your risk; check its terms.

## Permissions and consent

The first time Burrow starts an extension, it shows what the extension asks
for, such as "markdown-notes wants to: read and write files in ~/Notes · show
messages". You can untick any item. **Allow** writes your answer into the
config file as `extension-allow` lines, and starts the extension:

```text
extension-allow = markdown-notes ui
extension-allow = markdown-notes fs:~/Notes
```

You can also write these lines yourself, or delete one to take a permission
back. Each line grants one capability:

| Capability | Lets the extension |
|---|---|
| `items.read` | read your own posts, drafts and scratch notes |
| `items.write` | create drafts and scratch notes, and edit their text (it can never publish) |
| `reading.read` | read the posts held from your subscriptions |
| `blyg.identity` | know your blyg's address (never your sign-in) |
| `ui` | show messages, and select a post in the list |
| `hooks:itemPublished`, `hooks:itemSaved`, `hooks:itemCreated` | be told when you publish, edit or create a post, with its text |
| `browser.capture` | read the page open in the browser pane when you run one of its commands (never cookies or sign-ins) |
| `browser.automate:<site>` | fill in and, when you confirm, click Post on that one site, signed in as you |
| `fs:<folder>` | read and write files in that folder |
| `net` | use the network |

Burrow enforces the first ten: without the grant, the extension's request
is refused. **`fs:` and `net` are the extension's own promise.** An extension
is a program that runs as you, so Burrow can't stop it from reading other
files or using the network. Only install extensions you trust.

Whatever it's granted, no extension can publish, withdraw, pin, delete or fork
a post, change your blyg's settings, or see your sign-in, your Keychain or
Burrow's database.

## Checking your extensions

```sh
blygger +list-extensions
```

lists every extension, bundled and installed, whether it's on, what it's
granted and what it still asks for, and any installed folder Burrow couldn't
read. `blygger +validate-config` warns about an `extension` line that names
no installed extension, a permission an extension asks for but hasn't been
granted, a grant it never asks for, and a setting it doesn't have.

If an extension crashes, Burrow restarts it after 1, 5 and then 30 seconds.
After three failures in ten minutes it stays stopped until you reload the
config. Its error output is kept in a log in Burrow's data folder, under
`extensions/<name>/stderr.log`.

## Installing someone else's extension

An extension is a folder with an `extension.toml` file in it. Copy the folder
into `~/.config/blygger/extensions/`, so that you have
`~/.config/blygger/extensions/<name>/extension.toml`, and then turn it on with
`extension = <name>` like markdown-notes. There's no store, download or
automatic update: you choose what to copy.

## Writing an extension

An extension can be written in any language: a Python or Node script, a
shell script, or a compiled program. Burrow runs the `command` in its
manifest from the extension's folder, and the two talk over its standard
input and output, one JSON-RPC 2.0 message per line. A small one:

```toml
name = "hello"
version = "0.1.0"
protocol = 1
description = "Says hello."
command = ["python3", "main.py"]
capabilities = ["ui"]

[[commands]]
id = "hello"
title = "Say hello"
when = "always"
```

The full protocol (the manifest, every method, the errors, the timeouts and
how Burrow restarts an extension) is in
[Extensions (BXP)](dev/extensions.md), from `docs/EXTENSIONS.md` in the
repository.
For an extension in Rust, the `blyg-ext` crate has the protocol types and a
`serve` loop, and markdown-notes (`crates/blyg-ext-notes`) is a worked
example.

## Submit your extension

Wrote an extension? List it on the
[Community extensions](generated/community-extensions.md) page. The page is
made from one file in the repository, `extensions/community.toml`, every time
the documentation site is built, so a merged entry shows up on its own.

> **Community extensions are used at your own risk.** The Burrow maintainers
> do not review, verify, audit or endorse them. An extension is a program that
> runs with your user's rights, and `fs:` and `net` are its own declarations,
> not something Burrow can enforce. Read the source before you install one. To
> report a malicious or misleading listing,
> [open an issue](https://github.com/aneeshsathe/blygger-desktop/issues/new)
> and it will be removed.

To submit one, open a pull request that adds an `[[extension]]` table to
`extensions/community.toml`, in alphabetical order by `name`:

```toml
[[extension]]
name = "wordcount"                  # the name in your extension.toml
title = "Word count"
description = "Shows the word count and reading time of the open draft."
repo = "https://github.com/example/burrow-ext-wordcount"
homepage = "https://example.com/wordcount"   # optional
author = "example"                  # your GitHub handle, not an email
capabilities = ["items.read", "ui"] # exactly what your manifest asks for
platforms = ["macos", "windows"]
added = 2026-10-09                  # a date, without quotes
```

If you'd rather not open a pull request, fill in the
[extension submission form](https://github.com/aneeshsathe/blygger-desktop/issues/new?template=extension-submission.yml)
and a maintainer will add the entry for you.

CI runs `scripts/community-extensions.py check` on the pull request (you can
run it yourself first). It checks that:

- the file is valid TOML, and every entry has all the fields above and no
  others (only `homepage` is optional);
- `name` is lowercase kebab-case (`word-count`), at most 40 characters, not
  already listed, and not the name of an extension that comes with Burrow;
- `title` (at most 60 characters) and `description` (one line, at most 160)
  are plain text, with no HTML or Markdown code;
- `repo` and `homepage` are `https://` addresses;
- `author` is a GitHub handle, and no field holds an email address;
- each capability is one Burrow knows (see
  [Permissions and consent](#permissions-and-consent)), and `platforms` is
  `macos`, `windows` or both;
- the entries are sorted by `name`.

A maintainer merges an entry after a light look at its format, and nothing
more: the source code isn't read, run or tested. List the source code's
repository, keep `capabilities` in step with your manifest, and open another
pull request when the details change, or to take the entry down.
