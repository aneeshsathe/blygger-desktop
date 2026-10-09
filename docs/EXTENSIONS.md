# Burrow extensions (BXP, protocol 1)

An extension is a separate program that Burrow starts and talks to over its
stdin/stdout. It can add commands to the Extensions palette (⇧⌘P), offer a
library of documents to the notes panel (the bundled `markdown-notes` is
one), and, with the user's permission, read and edit the user's own posts.
Code: `crates/blyg-ext` (protocol, manifest, grants, host) and
`crates/blyg-ext-notes` (the bundled extension).

## Trust: what Burrow enforces, and what it can't

An extension runs as you. Burrow enforces everything that goes through
Burrow: without a grant, a `burrow/*` call is refused with `-32001`. There
is no method that publishes, withdraws, pins, deletes, forks or changes the
blyg's settings, and none that returns a token. The extension never sees
the Keychain, the data directory or the database, and its environment is
scrubbed (no `BLYGGER_*`).

`fs:<path>` and `net` are **declarations, not enforcement**: Burrow can't
stop a native program from reading other files or using the network. The
consent sheet says so in plain words (`blyg_ext::describe`,
`blyg_ext::CONSENT_CAVEAT`). Only install extensions you trust. A WASM
runner that enforces both is possible later without changing the protocol.

## Installing and enabling

Copy the extension's folder into `~/.config/blygger/extensions/<name>/` (the
folder holds `extension.toml`). Nothing runs until the config file says so:

```
extension = hello
extension-allow = hello ui
extension-allow = hello fs:~/Notes
extension-setting = hello greeting=hi
```

The first start of an extension that asks for capabilities and has none
granted shows the consent sheet; Allow writes the `extension-allow` lines.

## Manifest (`extension.toml`)

```toml
name = "hello"                    # kebab-case, at most 40 characters
version = "0.1.0"
protocol = 1
description = "Says hello."
command = ["python3", "main.py"]  # run from this folder; a bare name is looked up on PATH
capabilities = ["ui"]             # see below

[[commands]]                      # Extensions palette rows
id = "hello"
title = "Say hello"
detail = "A friendly message"
when = "always"                   # always | editor | reading | notes

[[libraries]]                     # documents for the notes panel
id = "notes"
title = "Notes"
writable = true

[[settings]]                      # extension-setting = hello <key>=<value>
key = "greeting"
kind = "text"                     # text | path | bool | number
docs = "What to say."
```

Unknown keys are ignored. On Windows a bare `command` name also tries the
`PATHEXT` extensions (`.exe`, `.cmd`, …); paths may use `\`.

## Capabilities

| Capability | Lets the extension | Enforced |
|---|---|---|
| `items.read` | `burrow/listItems`, `getItem`, `searchItems` over your own posts, drafts and scratch notes | yes |
| `items.write` | `burrow/createDraft` (draft or scratch), `burrow/saveItem` (working copy; never publishes) | yes |
| `reading.read` | `burrow/listReading`: posts held locally from subscriptions | yes |
| `blyg.identity` | the blyg's origin in `initialize` (never a token) | yes |
| `ui` | `burrow/toast`, `burrow/openItem` | yes |
| `hooks:itemPublished`, `hooks:itemSaved`, `hooks:itemCreated` | the matching notifications, with the post's text | yes |
| `fs:<path>` | declares the folder it reads and writes | **no** |
| `net` | declares that it uses the network | **no** |

## Wire format

JSON-RPC 2.0, one JSON object per line (LF or CRLF), both directions on the
one channel. Requests carry numeric ids; each side numbers its own. Fields
are camelCase, and unknown fields are ignored on both sides. Types:
`crates/blyg-ext/src/protocol.rs`.

Host → extension, requests (timeouts): `initialize` (5 s) →
`{protocolVersion: 1, name, version}`; `extension/command {id, context}`
(60 s) → `{toast?, open?}`; `extension/library.list {library?, path?}`,
`library.read {id}` → `{id, title, markdown, body, hash}`,
`library.write {id?, title?, folder?, markdown, baseHash?}` → `{id, hash}`
(5 s each); `library.search {query, limit}` and `source.search` (2 s);
`shutdown` (2 s, then the process is killed).

Host → extension, notifications: `burrow/itemPublished`, `burrow/itemSaved`,
`burrow/itemCreated` (each behind its hook), `burrow/settingsChanged`.

Extension → host: `burrow/listItems`, `getItem`, `searchItems`,
`createDraft`, `saveItem {id, contentMd, baseHash?}`, `listReading`,
`openItem`, `toast`, `requestCapability {capability}` (asks the user once
per capability per session; only capabilities the manifest declares).

Errors: `-32000` timeout, `-32001` permission denied
(`data.capability`), `-32002` stale (`data.currentHash`: the item or
document changed since the hash you sent), `-32003` refused.

Hashes are `"sha256:" + hex(SHA-256(text as UTF-8))`, the blyg's
`content_hash`. A write that names a `baseHash` lands only if the current
text still has it; nothing is ever merged or overwritten blindly.

## Lifecycle

Burrow starts enabled extensions shortly after its window opens. A crash
restarts the extension after 1 s, 5 s, then 30 s; the third failure in ten
minutes stops it until Reload Config. Three timed-out requests in a row
restart it. Its stderr goes to `<data dir>/extensions/<name>/stderr.log`
(rotated at 1 MB), and `initialize` hands it that folder as its own
`storageDir`.

## markdown-notes

Bundled, opt-in (`extension = markdown-notes`), and independent of any
blyg: it reads and writes a folder of Markdown notes and needs only
`fs:<vault>` and `ui`. It never adds frontmatter, never links notes to
posts, and never deletes. An edit is refused if the file changed on disk
since it was read; a new note never replaces an existing file. Settings:
`vault` (default `~/Notes`), `folder` (where new notes go), `poll-ms`.
