# Extensions

An extension adds commands, or a library of documents, to Burrow. It's a
separate program that Burrow starts and talks to. **Nothing runs unless your
config file names it**, and an extension gets only the permissions you grant
it.

Burrow comes with one extension, **markdown-notes**. You can also install
extensions other people write, or write your own.

## markdown-notes

markdown-notes reads and writes a folder of Markdown notes, such as an
Obsidian vault. It's kept apart from your blyg, so it works with no blyg
connected, and it never links notes to posts.

- **The notes panel** lists the folder, searches titles and bodies, and opens
  a note to read or edit.
- **Copy into post** puts the selection, or the whole note, into your draft at
  the cursor, as a quote or as it is (you pick).
- **Save selection to notes**, in the Extensions palette (⇧⌘P), makes a new
  note from the text you've selected.

It never adds frontmatter to a note (any frontmatter it finds is left as it
is), and it never deletes a note. If a note changed on disk after you opened
it, your save is refused, so it never overwrites the other change.

### Turning it on

Add these lines to the config file (**Burrow › Open Config File**), then
reload it (⇧⌘,):

```text
extension = markdown-notes
extension-setting = markdown-notes vault=~/Notes
```

`vault` is the folder of notes (it defaults to `~/Notes`). It has two more
settings:

```text
# where new notes go, inside the vault (default: the top of the vault)
extension-setting = markdown-notes folder=Inbox
# how often it looks for changed notes, in milliseconds (default 2000)
extension-setting = markdown-notes poll-ms=5000
```

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
| `fs:<folder>` | read and write files in that folder |
| `net` | use the network |

Burrow enforces the first eight: without the grant, the extension's request
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
