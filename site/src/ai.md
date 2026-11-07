# AI helpers

AI is optional and **off until you turn it on**: Burrow uses no provider, not
even an installed `claude` or `codex` CLI, until you enable one (`ai-enable` in
the config file) or sign in to one in Settings. Generation happens while you
write, you review it, and generated text is always disclosed when you
publish.

## Providers

You use your own accounts:

| Provider | How you sign in |
|---|---|
| Anthropic API | API key (stored in the Keychain) |
| OpenAI API | API key (stored in the Keychain) |
| Cloudflare Workers AI | Account ID and API token. Defaults to Gemma 4. |
| Local Claude Code or Codex | Uses your installed `claude` or `codex` CLI and whatever account it's signed in to |
| Your blyg server | Its `/generate` endpoint, if enabled |
| ChatGPT account | Sign in with ChatGPT. **Unofficial:** this uses the same sign-in as OpenAI's Codex CLI, which OpenAI doesn't offer to third-party apps. It may break or be blocked at any time. |

**Not offered:** signing in with a claude.ai (Pro/Max) subscription.
Anthropic's terms don't allow third-party apps to use claude.ai logins. To use
a Claude subscription, install Claude Code and pick the local Claude Code
provider.

## Connecting and generating

1. Turn a provider on. Either open **Settings › AI** (⌘, then "AI accounts…")
   and switch on Claude Code or Codex, paste an API key, or sign in, or add it to
   the config file:

   ```
   ai-enable = codex
   ai-enable = claude-code
   ai-provider = codex        # which one ⌘G uses (optional)
   ```

   API keys go straight to the Keychain and are never shown again: Settings
   shows "key saved" and a Remove button.
2. In a post, write a gap: `[TK]one sentence on why, in my voice[/TK]`.
3. With the caret inside it, press **⌘G**. The status bar shows
   `generating… (codex)`; **esc** cancels. The answer is written in place as
   `[TK]instruction[=]output[/TK]`. Press ⌘G inside it again to regenerate.

The config keys `ai-model` and `ai-provider-model` pick models, and
`ai-style-prompt` adds your own instructions to every prompt ("Write plainly.
British spelling."). See [Configuration](config.md#ai-provider).

## The helpers

⌘G anywhere outside a gap opens the helpers:

| Helper | What it does |
|---|---|
| Fill a gap here | Inserts an empty `[TK][/TK]` at the caret for you to fill in. |
| Shorten to fit 1000 | Also **⇧⌘G**. Rewrites a fragment to fit, shown as a word diff: ⏎ accepts, esc rejects. |
| Continue this thought | Writes on from the caret. |
| Outline a thread | Drafts the outline of a new thread. |
| Proofread | Typos and grammar only, applied as a plain edit. |
| AI reply | From a post you're reading: a reply stub with a first draft. |

Each generating helper writes its text as a TK span, so the preview tints it
and it's disclosed like any gap.

## AI on your blyg

Your blyg can have AI of its own, separate from Burrow's: models that run on
the blyg, for things the blyg does. **Blyg › AI on Your Blyg…** sets them, as
the studio's Settings › AI does:

| Row | What it does |
|---|---|
| TK generation | The model that fills `[TK]` gaps on the blyg, and the **house style** added to every one of them. |
| Change notes | The model that drafts version notes, and **Draft automatically** (an empty note is drafted when you publish). |
| Change classifier | A decision model (Clef, Jev) that labels each new version typo, minor or major, and whether the label **starts the note**. Off unless you choose one. |
| Smart Feed | A model and a scoring prompt, saved now for the studio's coming Smart Feed. |

↑/↓ choose a row, ←/→ change its model (or flip its switch; Space too),
"other…" types any model id, ⌘⏎ saves. The sheet lists each provider with
whether its key is set: keys are Worker secrets, set with `wrangler`, never
through Burrow. The model list is your studio's `models.json`. How the
helpers appear when you publish: [Writing](writing.md#your-blygs-ai-in-the-publish-sheet).

Burrow's own providers (above) never use these, and these never use Burrow's.

## Nearby thoughts: what is sent where

The Nearby rail (see [Writing](writing.md#nearby-thoughts)) needs no AI.
What leaves your computer depends on the settings:

| Setting | What is sent, and where |
| --- | --- |
| `nearby-embedder = none` (default) | Nothing. Phrases are matched in the local search index. |
| `nearby-embedder = local` | One download of the model (about 31 MB, from Hugging Face, pinned and checked). After that, nothing. |
| `nearby-embedder = endpoint` | The paragraph you're writing, and the text of your published posts and the posts you hold, to `nearby-endpoint` (no key is sent). Use a server you run, such as Ollama on this computer. |
| `nearby-rerank = on` | The paragraph, and the titles and a short excerpt of up to ten candidate posts, to your AI provider (`ai-provider`). Off by default. |

## Disclosure

Generated text is always disclosed as generated when you publish. Burrow
records which model wrote each gap and sends it to your blyg with the text
(blygger-studio 0.28 or later; see [Server requirements](server.md)). If
your blyg's studio is older, Burrow warns you before publishing generated
text, and Cancel is the default. Proofreading isn't generated prose and isn't
disclosed.

If generated text in a thread holds a quote (`![[id]]` on its own line) that
the gap's instruction didn't ask for, the publish sheet warns you: publishing
would quote that post and notify its author (see
[TK: gaps for later](writing.md#tk-gaps-for-later)).

When you read other blygs, text they mark as generated is the publisher's own
disclosure: Burrow shows it, and says it isn't verified (see
[Generated text](reading.md#generated-text)).

How the providers, sign-in, prompts and provenance work inside is in
[blyg-ai](dev/ai.md).
