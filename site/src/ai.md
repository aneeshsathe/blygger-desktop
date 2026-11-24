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

### When your blyg can't read a link

With **your blyg server** as the provider, ⌘G asks your blyg to fill the gap
(blygger-studio 0.70 and later). Your blyg first reads every link in the
gap, posts and web pages alike. If some can't be read, it stops and saves
nothing, and Burrow asks: "1 of 2 links in this TK could not be read.
Generate anyway?" **Details** lists each link and why it failed; it is open
already when three or fewer failed, and **d** shows or hides it. **⏎**
generates anyway: what could be read is drawn on, and the model sees only
the address of the rest and is told not to describe it. **esc** cancels,
leaves the draft as it was, and reminds you to remove or replace the link
before you try again. Burrow's other providers never stop here.

### What each generation read

After your blyg fills a gap, Burrow lists what it read, gap by gap (each
gap by its number in the post): the posts it drew on, by title when Burrow
holds them (click one to open it) and otherwise by their blyg, and the web
pages it read, by address (and site, when that isn't the address's host). A
post on another blyg is marked "tells its author when you publish"; your own
posts and web pages tell no one. The list is under the preview (⌘2 or ⌘3)
and in the Publish sheet, so you can check it before you publish; a long
one scrolls. It shows the first 50 entries, then how many more there are.

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
| Text helpers | Studio 0.69: the model for small writing jobs on the blyg, such as Catch up's summary. |
| Agentic feed | The **tab name** (studio 0.69) your assistant's editions go under, and a model and scoring prompt saved for the studio's own. |
| Change notes | The model that drafts version notes, and **Draft automatically** (an empty note is drafted when you publish). |
| Change classifier | A decision model (Clef, Jev) that labels each new version typo, minor or major, and whether the label **starts the note**. Off unless you choose one. |
| Judge other blygs' changes | Studio 0.69: the classifier also labels new versions of posts you read that Background's rules leave open; a typo or minor label sends them to Background. An optional **instruction** has it read the version note too, and **Keep in view if the note understates it** keeps such a post in view, marked. |

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

## Agentic feed: what is sent where

Burrow's own AI can write your blyg's Agentic feed edition (see
[Reading](reading.md)). It runs when you press ⌥⌘E in the Agentic feed tab,
and, with `agentic-writer = after-sync` (Settings › Agentic feed), after a
sync that brings new posts, at most every `agentic-writer-gap` hours (3 by
default). Off by default.

| What | Where it goes |
| --- | --- |
| The source name, title and opening words (about 280 characters) of up to 60 posts you hold that are unread or arrived since the last edition (never your own posts, never withdrawn or Background ones), and your blyg's Agentic feed prompt (Blyg › AI on Your Blyg…) | The provider in `agentic-writer-provider` (default: the one the writing helpers use), with `agentic-writer-model` |
| The edition it writes: headlines, summaries and reasons, the posts by subscription and id, "Burrow · *provider*" and the model | Your blyg (`PUT /api/agentic/default`), only where your sign-in may write editions |

The model names posts by number, never by id; Burrow drops any it wasn't
given, holds every field to your blyg's limits, and sends nothing when the
reply isn't usable. The tab shows every word it wrote as the assistant's
(✦), as it does an outside assistant's.

## Disclosure

Generated text is always disclosed as generated when you publish. Burrow
records which model wrote each gap and sends it to your blyg with the text
(every studio Burrow works with takes it; see [Server requirements](server.md)).
If a blyg ever refuses it, Burrow warns you before publishing generated
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
