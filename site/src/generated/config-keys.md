<!-- Generated from crates/blyg-core/src/config/keys.rs by scripts/gen-docs.sh.
     Don't edit it by hand: cargo test fails when it's out of date. -->

### `blyg-url`

- **Values:** an `http(s)://` address
- **Default:** unset

The blyg this app writes to, e.g. https://blyg.example.com. There is no default: with no blyg-url the app starts by asking you to connect one.

The owner token for this blyg is kept in the macOS Keychain, never in this file.


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

An extension is a separate program that talks to Burrow over its standard input and output. It never sees your owner token, API keys or the database: Burrow makes every call for it, and only the ones extension-allow grants.


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
