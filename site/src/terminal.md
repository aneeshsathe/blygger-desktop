# The terminal pane (experimental)

> **Experimental.** The terminal pane is new in 0.18.0. It works, but it's
> plainer than a full terminal app: see [What it doesn't do yet](#what-it-doesnt-do-yet).

Burrow has a terminal at the foot of the window: your own login shell, under
your posts, so you can talk to a CLI agent (`claude`, `codex`…), run `git`, or
keep `wrangler dev` going without leaving the burrow.

- **⌃\`** (Ctrl+\` on Windows; View › Terminal) shows the pane and puts the
  keyboard in it. Press it again to hide the pane. Hiding keeps the shell
  running; **end** in the pane's header, or `exit`, ends it.
- **⇧⌘T** (Ctrl+Shift+T on Windows; Post › Send Selection to Terminal) pastes
  the text selected in the editor into the shell as a Markdown quote
  (`> …`), then a blank line, so you can add your question and press ⏎. It's
  never run for you.
- The pane is a row of its own between your posts and the status bar. It never
  covers the editor.

While the pane has the keyboard it gets every key, including ⏎, the arrows,
esc and ⌃C, except the app's ⌘-keys (Ctrl+Shift-keys on Windows) and ⌃\`
itself. Drag to select; **⌘C** copies the selection and **⌘V** pastes
(Ctrl+Shift+C / Ctrl+Shift+V on Windows). The wheel scrolls back through the
last 5,000 lines (⇧PageUp / ⇧PageDown too); in a full-screen program it
moves that program's cursor instead.

Colours follow your theme: the shell's default text and background are the
editor's ink and paper, and the sixteen ANSI colours come from the theme's
palette. Programs that use their own 256 or 24-bit colours get them.

## What it's for

### Ask an agent to draft from your own posts

Your blyg already runs an MCP server, the way agents read and write a blyg.
Its address is in your blyg's capabilities (`/api/capabilities`, under
`endpoints.mcp`), usually `https://blyg.example.com/mcp`. Point the agent in
the pane at it, for example with Claude Code:

```sh
claude mcp add --transport http blyg https://blyg.example.com/mcp
claude
```

Then ask: *"Write a fragment that connects Lanterns to Gardens, not streams,
and link both."* The agent reads your posts through your blyg and saves a
draft there; Burrow picks the draft up on its next sync, and you edit and
publish it yourself. Other agents (`codex`, …) take an MCP server's URL in
their own configuration; see their documentation.

The agent signs in to your blyg **itself**: through your studio's sign-in
when the agent supports it, or with a token you create for it in your
studio's settings. Give that token the MCP resource and only what it needs:
`owner:read` to read, `owner:draft` to write drafts. Leave out
`owner:publish`, so publishing stays yours, in Burrow. Burrow never passes
its own token to the pane.

### Debug an extension

Run one by hand and watch what it says:

```sh
blygger +ext markdown-notes
```

or read an extension's log in Burrow's data directory while you use it in the
window above.

### A local studio

Run the reference Worker with `wrangler dev` in the pane while you try a
change, with Burrow connected to it, and watch its log as you publish to it.

### Git on a notes vault

A markdown-notes vault is a folder. `cd` into it and `git log -p`, `git diff`
or commit, beside the notes drawer that edits it.

## Safety

The pane runs **your** shell with **your** rights, as Terminal.app does. What
it never gets is anything of Burrow's:

- **A scrubbed environment.** The shell starts from your environment minus
  every `BLYGGER_*` variable, the variables Burrow sets for itself (`GPUI_*`)
  and the identity of whatever terminal started Burrow. `TERM` is
  `xterm-256color` and `TERM_PROGRAM` is `Burrow`.
- **No token, anywhere.** Burrow never puts its token, an AI key, its
  database or its data directory into the shell's environment, into a file
  for it, or on the clipboard.
- **No bridge into the app.** Burrow talks to the shell only through the
  terminal: what you type goes in, what the shell prints comes out. There is
  no socket, no IPC and no command a program in the pane can send to Burrow.
  An agent changes your blyg only through your blyg (its MCP server or
  API), and Burrow sees the change the way it sees any other: on sync.
- **The clipboard stays yours.** A program in the pane can't read or set the
  clipboard through the terminal (OSC 52 is off). Only ⌘C and ⌘V cross.

`blygger +…` commands you run in the pane read the Keychain as you, the same
as running them in Terminal.app.

## Settings

| Key | Default | |
|---|---|---|
| `terminal` | `true` | `false` turns the pane off (and ends a running shell). |
| `terminal-shell` | your login shell | The program to run instead, with its arguments: `terminal-shell = /bin/zsh -l`, `terminal-shell = pwsh`. |

On macOS the default is `$SHELL -l`; on Windows `%COMSPEC%` (cmd.exe).

## What it doesn't do yet

- **The engine is Alacritty's, not Ghostty's.** The pane was designed around
  libghostty-vt 1.0, Ghostty's terminal core. No Rust package ships that
  library without building Ghostty with Zig or downloading it while building,
  and Burrow builds with plain cargo, so the pane uses
  [`alacritty_terminal`](https://crates.io/crates/alacritty_terminal)
  (Apache-2.0), the core Zed's terminal uses. Bindings for libghostty-vt 1.0
  are written and kept behind a build option, off and untested.
- No mouse reporting to programs (a click in `vim` or `htop` doesn't reach
  them), no tabs or splits, no search in the scrollback, no links.
- Input methods that compose characters (dead keys, Japanese input) don't go
  through the pane yet; ⌥-composed characters do.
- One pane, one shell.
