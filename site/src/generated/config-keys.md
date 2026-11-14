<!-- Generated from crates/blyg-core/src/config/keys.rs by scripts/gen-docs.sh.
     Don't edit it by hand: cargo test fails when it's out of date. -->

### `blyg-url`

- **Values:** an `http(s)://` address
- **Default:** unset

The blyg this app writes to, e.g. https://blyg.example.com. There is no default: with no blyg-url the app starts by asking you to connect one.

Your sign-in for this blyg is kept in the macOS Keychain, never in this file.


### `blyg`

- **Values:** an `http(s)://` address
- **Default:** none
- **Repeatable:** yes, one value per line

Every blyg you're signed in to, one line each, e.g. blyg = https://notes.example.org. The blyg switcher (click the blyg's name in the title bar, or Blyg › Switch To) lists them in this order, and ⌃⌘1 to ⌃⌘9 switch to the first nine. blyg-url is the one you're writing to now, and is always in the list whether a line names it or not. Burrow writes these lines when you add a second blyg and removes them when you're back to one. Each blyg keeps its own sign-in in the Keychain and its own posts, drafts and reading on this Mac.


### `sync-cadence`

- **Values:** `normal`, `low`
- **Default:** `normal`

How often Burrow asks your blyg what changed. normal (the default) asks every 15 seconds, like Studio, reads the reading list again at most once a minute when only it moved, and reads everything whole every 10 minutes as a safety net, backing off to every 2 hours while that finds nothing new. low asks every minute, reads the reading list at most every 10 minutes, and starts the safety net at an hour (up to 6 hours). Use low for a blyg on Cloudflare's free D1 plan with a large reading list: its daily row reads can run out, and the blyg then fails until midnight UTC. Opening a blyg always syncs it at once. Takes effect the next time Burrow opens the blyg.


### `theme`

- **Values:** `system`, `light`, `dark`, a built-in theme (`cutaway`, `kumiko`, `shola`, `fortress`, `portolan`, `aizome`, `saltspace`, `konkan`) or a theme file's name (see Themes)
- **Default:** `system`

Colour theme: system (follow macOS light and dark), light (Paper), dark (Ink), one of the built-in themes (woody: cutaway, kumiko, shola, fortress; oceanic: portolan, aizome, saltspace, konkan), or the name of a theme file in ~/.config/blygger/themes/. `blygger +list-themes` lists them all, and `blygger +copy-theme &lt;name>` copies a built-in there to edit. Theme files reload when you save them.


### `theme-dark`

- **Values:** `system`, `light`, `dark`, a built-in theme (`cutaway`, `kumiko`, `shola`, `fortress`, `portolan`, `aizome`, `saltspace`, `konkan`) or a theme file's name (see Themes)
- **Default:** unset

The theme to use while macOS is dark, whatever theme says, e.g. theme = cutaway with theme-dark = fortress. Unset, theme = system switches to dark and every other theme stays as it is.


### `layout`

- **Values:** `side`, `stacked`
- **Default:** `side`

Window layout: side puts the list to the left of the editor; stacked puts it above.


### `show-buttons`

- **Values:** `true` or `false`
- **Default:** `true`

Show the toolbar buttons in the title bar (New, Make draft, Publish, the views, Versions, Generate, Quick capture) and the Scratch · Draft · Publish row in quick capture. Every button's tooltip shows its shortcut. false keeps the window keyboard-only and minimal. Settings (⌘,) toggles it.


### `spellcheck`

- **Values:** `true` or `false`
- **Default:** `true`

Check spelling while you type, with the macOS spell checker (your system languages and the words you've taught it). Misspelled words get a red wavy underline; right-click one for suggestions, Learn Spelling and Ignore. Code, links, addresses and blyg markup are never checked. Edit › Spelling › Check Spelling While Typing toggles it.


### `bracket-picker`

- **Values:** `on`, `quotes`, `off`
- **Default:** `on`

Which typed brackets open the post picker. on: typing [[ anywhere outside code opens it for a link, and ![[ at the start of a line in a thread for a quote. quotes: only ![[ at the start of a line; [[ stays plain text, for brackets in prose and TK scopes. off: typed brackets never open it. ⌘K always opens the picker.


### `nearby`

- **Values:** `on`, `titles`, `off`
- **Default:** `on`

Nearby thoughts: a rail beside the editor that shows up to five of your own published posts, and posts you hold from blygs you follow, that say the same thing as the paragraph you're writing. ⌥⌘U shows or hides it; ⌥↑/⌥↓ choose, ⌥⏎ links the matched phrase ([[id | phrase]]), ⌥⇧⏎ quotes the post on a new line (threads only). on matches the whole of each post; titles only their titles; off turns it off. Only what this Mac already holds is offered, and nothing is fetched to suggest.


### `nearby-embedder`

- **Values:** `local`, `endpoint`, `none`
- **Default:** `none`

How Nearby thoughts tells that two paragraphs mean the same thing. none (the default) matches shared phrases through the full-text index. local uses a small embedding model run on this Mac (potion-base-8M, MIT, about 31 MB), downloaded once into Burrow's data folder when you choose it (Settings › Nearby) and checked against a pinned checksum; after that it works offline and nothing leaves this Mac. endpoint sends each paragraph, and the text of the posts being indexed, to the embeddings endpoint in nearby-endpoint (e.g. Ollama on this Mac).


### `nearby-endpoint`

- **Values:** an `http(s)://` address
- **Default:** unset

With nearby-embedder = endpoint: an OpenAI-compatible or Ollama server's address, e.g. http://localhost:11434 for Ollama or http://localhost:1234/v1 for an OpenAI-compatible server. Burrow posts to /api/embed (Ollama, when the address has no /v1) or /embeddings. No API key is sent: point it at a server you run. Text goes there only when you set this.


### `nearby-endpoint-model`

- **Values:** text
- **Default:** `nomic-embed-text`

The model nearby-endpoint embeds with, e.g. nomic-embed-text or mxbai-embed-large.


### `nearby-rerank`

- **Values:** `off`, `on`
- **Default:** `off`

on: after the rail fills, the AI provider you use for writing (ai-provider) puts the best ten in order of how closely each says what your paragraph says. That sends the paragraph, and the titles and an excerpt of those posts, to that provider. off by default.


### `auto-update`

- **Values:** `install`, `notify`, `off`
- **Default:** `install`

Updates from the project's GitHub releases. install downloads a new release in the background, checks its signature, and shows "Restart to update" in the status bar (quitting installs it too); notify only says a new release is available; off never checks on its own. Burrow › Check for Updates… always checks. An update is refused unless it's signed with the project's release key.


### `open-links`

- **Values:** `app`, `browser`
- **Default:** `app`

Where a link clicked in a post opens: app opens it in Burrow's browser pane (from the right, over the reading view; esc closes it), browser opens your default browser. ⌘-click opens the pane over the whole reading area, and ⌥-click does the other one (the default browser with app, the pane with browser).


### `content-blocking`

- **Values:** `true` or `false`
- **Default:** `true`

Block ads and trackers in the browser pane, with uBlock Origin's default filter lists (downloaded about once a week; a small built-in list until then). The shield button turns it off for one site. false turns it off everywhere.


### `chat-tab`

- **Values:** `true` or `false`
- **Default:** `true`

Show the #thicket chat tab beside the views in the title bar. Experimental: it may be removed. The tab chats in #thicket, a channel for Burrow users on Libera.Chat (an IRC network). Nothing connects to Libera.Chat until you set it up in the tab (sign in, or create an account). false hides the tab and disconnects. Settings (⌘,) toggles it.


### `chat-nick`

- **Values:** text
- **Default:** unset

Your Libera.Chat nick (also your account name), e.g. chat-nick = ada_example. The chat tab writes it when you finish setting up, and then signs in on its own each time Burrow opens (SASL over TLS). The password is in the macOS Keychain, never in this file. Unset: the tab shows its setup and connects to nothing.


### `chat-server`

- **Values:** text
- **Default:** `irc.libera.chat:6697`

The IRC server the chat tab uses, as host:port, always over TLS. Only Libera.Chat is supported for now.


### `chat-notifications`

- **Values:** `true` or `false`
- **Default:** `true`

A system notification when someone mentions your nick in #thicket (or messages you) while the chat tab isn't in front. false keeps only the tab's marks. Settings (⌘,) toggles it.


### `terminal`

- **Values:** `true` or `false`
- **Default:** `true`

The terminal pane at the foot of the window (⌃` shows and hides it): your login shell, for a CLI agent (claude, codex…), git or wrangler beside your posts. Experimental. The shell gets your environment minus everything of Burrow's (no BLYGGER_* variable, no token) and talks to Burrow only through the pane. false turns it off and ends a running shell.


### `terminal-shell`

- **Values:** text
- **Default:** unset

The program the terminal pane runs, with its arguments, instead of your login shell ($SHELL -l on macOS, %COMSPEC% on Windows), e.g. terminal-shell = /bin/zsh -l or terminal-shell = pwsh.


### `focus-line`

- **Values:** a number from 0.1 to 0.9
- **Default:** `0.45`

Focus mode (⌥⌘F, View › Focus) keeps the line you're typing on at this height in the editor, as a fraction from the top: 0.45 is a little above the middle, 0.1 near the top, 0.9 near the bottom. The text scrolls under it.


### `focus-fade`

- **Values:** `paragraph`, `off`
- **Default:** `paragraph`

In focus mode, paragraph dims every paragraph but the one you're in, so it reads at about a third; off leaves the text as it is.


### `serendipity`

- **Values:** `off`, `own`, `on`
- **Default:** `off`

Serendipity: ⌘N's empty editor shows one of your older posts, an orphan (nothing links to it) or one from this week in another year, never a withdrawn one, and not the same one again within a month. q quotes it, l links it, o opens it, r shows another, until you type. own shows your own posts only; on also shows posts you've read from your subscriptions; off (the default) shows nothing. Settings (⌘,) has it too.


### `sounds`

- **Values:** `off`, `quiet`
- **Default:** `off`

quiet plays a soft macOS system sound when a post is published and when quick capture keeps a note; never while you type. off (the default) is silent. Burrow plays no sounds on Windows.


### `font-family-writing`

- **Values:** text
- **Default:** `Literata`

Font for the editor and preview. Run `blygger +list-fonts` to see the choices.


### `font-family-ui`

- **Values:** text
- **Default:** `Inter`

Font for the list, the omnibar and sheets. Run `blygger +list-fonts` to see the choices.


### `font-size`

- **Values:** a number from 12 to 32
- **Default:** `19`

Writing font size in points, from 12 to 32. ⌘+ and ⌘− change it and write it back here.


### `capture-hotkey`

- **Values:** a key combination, such as `ctrl+alt+b`
- **Default:** `ctrl+alt+b`

Global hotkey for the quick-capture panel. Modifiers are ctrl, alt (option), shift and cmd, joined with +, e.g. ctrl+alt+b or cmd+shift+space.


### `capture-default`

- **Values:** `scratch`, `draft`
- **Default:** `scratch`

What quick capture keeps when you press esc or ⌘S, or click away: scratch keeps a scratch note that stays on this Mac (never synced, never published, until you make it a draft with ⌘D or publish it with ⌘⏎); draft saves a draft on your blyg.

In the capture panel, ⌘D always saves a draft and ⌘⏎ always publishes.


### `new-note`

- **Values:** `draft`, `scratch`
- **Default:** unset

What a new post starts as: scratch (a scratch note that stays on this Mac until ⌘D makes it a draft or ⌘⏎ publishes it) or draft (a draft on your blyg).

It applies to ⌘N (Post › New Post), which opens an empty editor, and to the omnibar's create (⏎ when the search finds nothing). Unset, ⌘N starts a scratch note and the omnibar creates a draft.


### `edited-posts`

- **Values:** `top`, `stay`
- **Default:** `top`

What happens to a reading-list post when its author edits it: top moves it to the top of the list; stay leaves it where it was.


### `ai-provider`

- **Values:** `claude-code`, `codex`, `chatgpt`, `openai`, `anthropic`, `cloudflare`, `server`, `none`
- **Default:** `none`

The AI provider the writing helpers use by default: claude-code or codex (your locally installed CLI), chatgpt, openai, anthropic, cloudflare, server (your blyg's own generate endpoint), or none.

none picks the first ready provider listed in ai-enable, in the order claude-code, anthropic, chatgpt, openai, codex, cloudflare, server.

API keys and sign-ins live in the macOS Keychain, never in this file.


### `ai-model`

- **Values:** text
- **Default:** unset

Model for ai-provider. Unset uses that provider's default.


### `ai-provider-model`

- **Values:** `provider=model`
- **Default:** none
- **Repeatable:** yes, one value per line

Model for one particular provider, as provider=model, e.g. anthropic=claude-sonnet-5. Repeat the key for more providers.


### `ai-enable`

- **Values:** `claude-code`, `codex`, `chatgpt`, `openai`, `anthropic`, `cloudflare`, `server`
- **Default:** none
- **Repeatable:** yes, one value per line

Providers that are switched on. Repeat the key for each one. None by default: Burrow uses no AI (not even a locally installed claude or codex CLI) until you enable a provider here or sign in to one in Settings, which updates this list. An empty `ai-enable =` switches every provider off again.


### `cloudflare-account-id`

- **Values:** text
- **Default:** unset

Cloudflare account ID for Workers AI. Not a secret; the API token is in the Keychain.


### `ai-style-prompt`

- **Values:** text
- **Default:** unset

Extra instructions appended to every generation prompt, e.g. "Write plainly. British spelling."


### `ai-disclose`

- **Values:** `always`
- **Default:** `always`

Generated text is always disclosed as generated when published. This is fixed: always is the only value.


### `tutorial-on-launch`

- **Values:** `true` or `false`
- **Default:** `false`

Show the interactive tutorial every time Burrow opens. The very first launch always shows it; after that it follows this setting.


### `extension`

- **Values:** text
- **Default:** none
- **Repeatable:** yes, one value per line

An extension to run, by name (lowercase kebab-case), e.g. extension = markdown-notes. Repeat the key for each one. None by default: nothing runs until you name it here. Extensions are the bundled markdown-notes and folders in ~/.config/blygger/extensions/; `blygger +list-extensions` lists them.

An extension is a separate program that talks to Burrow over its standard input and output. It never sees your blyg sign-in, API keys or the database: Burrow makes every call for it, and only the ones extension-allow grants.


### `extension-allow`

- **Values:** text
- **Default:** none
- **Repeatable:** yes, one value per line

A permission granted to an extension, as &lt;name> &lt;capability>, e.g. extension-allow = markdown-notes items.read. One capability per line. Burrow writes these when you click Allow in the permission sheet.

Capabilities: items.read (read your posts, drafts and scratch notes), items.write (create drafts and scratch notes and edit their text; an extension can never publish, withdraw, pin or delete), reading.read (read posts from your subscriptions already on this Mac), blyg.identity (your blyg's address, never the token), ui (show messages and open a post), hooks:itemPublished, hooks:itemSaved and hooks:itemCreated (be told when you publish, save or create a post), browser.capture (read the page open in the browser pane when you run one of its commands: address, title, selection and article text, never cookies or sign-ins), browser.automate:&lt;origin> (fill in and, when you confirm, post on that one website, signed in as you, e.g. browser.automate:https://social.example.com; exact origin, no path or wildcards), net (uses the network) and fs:&lt;path> (reads and writes files under path, e.g. fs:~/Notes).

net and fs: are declarations you agree to, not a sandbox: an extension runs as a program with your user's rights.


### `extension-setting`

- **Values:** text
- **Default:** none
- **Repeatable:** yes, one value per line

A setting for one extension, as &lt;name> key=value, e.g. extension-setting = markdown-notes vault=~/Notes. Keys are lowercase kebab-case. Repeatable; a later line for the same key wins. An extension sees its own settings and no other part of this file. markdown-notes reads one folder of notes per vault key: vault=~/Notes is the first, and each vault-&lt;label>, e.g. extension-setting = markdown-notes vault-work=~/Work/Vault, adds another (Settings › Notes folders writes these). With no vault it uses no folder at all.


### `config-file`

- **Values:** a file path (`~/` is expanded; a leading `?` makes it optional)
- **Default:** none
- **Repeatable:** yes, one value per line

Load another config file after this one. Relative paths are relative to this file. A leading ? makes the file optional (no error if it's missing), e.g. config-file = ?local.config. Repeatable.
