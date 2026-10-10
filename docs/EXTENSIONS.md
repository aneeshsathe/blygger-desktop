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
when = "always"                   # always | editor | reading | notes | published

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
| `browser.capture` | `burrow/browser.page`: the page open in the browser pane (address, title, selection, readable text) while one of its commands runs; never cookies or sign-ins | yes |
| `browser.automate:<origin>` | run its `[[macros]]` on that one origin, and `burrow/browser.open` URLs on it | yes |
| `fs:<path>` | declares the folder it reads and writes | **no** |
| `net` | declares that it uses the network | **no** |

## Browser: capture and macros

Two capabilities reach the browser pane, both enforced by Burrow:

- `browser.capture` lets the extension call `burrow/browser.page` while one
  of its commands runs. The answer is `{url, canonicalUrl?, title,
  selection, markdown, author?}` (`PageCapture`), or `-32004` when the
  pane has no page. There is no method that returns cookies, storage or
  headers, or that runs script.
- `browser.automate:<origin>` names one website exactly: `http` or `https`,
  a host, an optional port, nothing after it. Burrow normalises it (lower
  case, `xn--` for international names, default port dropped:
  `HTTPS://Social.Example.com:443/` is `https://social.example.com`) and
  compares origins exactly, so no wildcards, parent domains or other
  ports. The consent sheet reads "fill in and, when you confirm, click Post
  on social.example.com, signed in as you".

A **macro** is a declarative recipe that Burrow runs in the pane, the one
the user signs in to by hand. The extension never sees that page:

```toml
capabilities = ["items.read", "browser.automate:https://social.example.com"]

[[sites]]
id = "social"
title = "Social Notes"
origin = "https://social.example.com"        # must be declared above
home = "https://social.example.com/notes"     # where to sign in; on origin
signed-out = "a[href*='sign-in']"            # optional: matches = signed out
content-blocking = false                     # optional: shield off during a run only
min-interval = "60s"                         # optional: the shortest gap between runs

[[macros]]
id = "cross-post-note"
title = "Cross-post to Social Notes…"
site = "social"
when = "published"                 # the default: a published post is open
template = "{{excerpt}}\n\n{{permalink}}"   # the default; {{title}} too
tested = "unverified"              # free text: when the selectors were last checked
steps = [
  { do = "open", url = "https://social.example.com/notes" },
  { do = "waitFor", selector = "div[contenteditable='true']", timeout = "20s" },
  { do = "focus",  selector = "div[contenteditable='true']" },
  { do = "insert", selector = "div[contenteditable='true']" },
  { do = "submit", selector = "button", text = "Post" },
  { do = "waitFor", selector = "div[contenteditable='true']", empty = true, timeout = "15s" },
  { do = "done", text = "Posted to Social Notes" },
]
```

Steps: `open {url}`, `waitFor {selector, text?, empty?, absent?, timeout?}`,
`assert {selector, text?}`, `focus {selector}`, `click {selector, text?}`,
`insert {selector}`, `submit {selector, text?}`, `done {text}`. A selector
is a `querySelectorAll` selector; a list (`a, b`) is tried part by part,
the first part with a match winning (so `[role='dialog'] [contenteditable],
[contenteditable]` prefers the one in a dialog). Of the matches, the first
visible one. With `text`, only matches whose `innerText` equals it
(whitespace collapsed, curly quotes folded), the innermost of nested ones
(a card's label, not the card); failing that, one whose placeholder or
`aria-label` equals it. `click` and `submit` wait for the element to be
enabled, and send pointer and mouse down/up before the click; `waitFor
absent` also counts a hidden element as gone. Durations are a whole number
and `ms`, `s` or `m`. A step waits 10 s unless it says otherwise; `open`
waits up to 30 s for a navigation to finish (whatever URL it ends on).

When a step's element isn't found, `macro.log` gets, under the error line,
an outline of what the page does have: its path (no query), the title's
length, and up to 25 text boxes, dialogs and buttons (tag, id, classes,
role, aria-label, placeholder, data-testid, name, visible, and a button's
label cut to 30 characters), at most 3 KB, never other page text, a value
or the post's text. With `BLYGGER_MACRO_TRACE=1` it gets one after every
step.

The manifest is refused unless: every site's origin is declared as
`browser.automate:`, and its `home` is on it; `min-interval` parses; each
macro names one of the manifest's sites and its template uses only
`{{title}}`, `{{excerpt}}` and `{{permalink}}`; the first step is `open`
and every `open` URL is on one of the sites; there is exactly one
`insert`, no `submit` before it and at least one after it; nothing
`click`s after `insert`, and nothing `open`s between it and the first
`submit`; `done` is the last step and only there; timeouts are more than 0
and at most 30 s; at most 64 steps. Errors name the step:
`macro "cross-post-note" step 2 (waitFor): timeout "45s" is over the 30s
limit`. Types and rules: `crates/blyg-ext/src/recipe.rs`.

Before a run, Burrow expands the template (one pass; CRLF becomes LF) and,
if the extension implements it, calls `extension/macro.prepare {macro,
item, permalink, text}` (10 s) → `{text, note?}`; method-not-found means
the template's text stands. The user sees and can edit the text, Burrow
runs the steps up to the first `submit`, shows what the composer now
holds, and runs `submit` only when the user clicks Post. Only granted
macros of running extensions are offered (`Host::macros`), as ⇧⌘P rows
titled "<title> ↗ <site title>", when a published post is open. The
template's `{{excerpt}}` is the post's opening paragraph (not a heading,
quote or embed), cut at 280 characters on a word boundary. An error from
`macro.prepare` is a toast, and nothing runs.

While a macro runs, `burrow/browser.page` answers `-32004` and
`burrow/browser.open` is refused (`-32003`): an extension never reads or
moves the page a macro fills in. `browser.open` shows the pane, full
width, at the URL.

The bundled `cross-post` extension (`crates/blyg-ext-crosspost`, run as
`blygger +ext cross-post`) is the worked example: one site, one macro for
Substack Notes (`tested = "2026-10-09"`), and a `macro.prepare` that fits
the opening paragraph and the link into `max-chars` (280) characters,
counted as grapheme clusters.

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
per capability per session; only capabilities the manifest declares),
`browser.page` (`browser.capture`), `browser.open {url}`
(`browser.automate:` for the URL's origin) → `{url}`.

Host → extension, also: `extension/macro.prepare` (10 s, optional; see
Browser above).

Errors: `-32000` timeout, `-32001` permission denied
(`data.capability`), `-32002` stale (`data.currentHash`: the item or
document changed since the hash you sent), `-32003` refused, `-32004` no
page in the browser pane.

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
blyg: it reads and writes folders of Markdown notes ("vaults") and needs
only `fs:<folder>` for each one and `ui`. It never adds frontmatter, never
links notes to posts, and never deletes. An edit is refused if the file
changed on disk since it was read; a new note never replaces an existing
file. Settings: `vault` (the first folder), `vault-<label>` (one more
folder each, e.g. `vault-work=~/Work/Vault`), `folder` (where new notes go
inside each), `poll-ms`. Each vault is its own library (`notes`,
`notes.<label>`). There's no default folder: with no vault it asks only
for `ui` and has no library.
