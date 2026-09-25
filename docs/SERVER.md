# Server requirements

Blygger Desktop talks to a blyg's **owner API**. The upstream reference Worker
(https://github.com/blygger/blygger-spec, `worker/`) has a write-only owner API behind a
studio password cookie. This app needs four small, additive extensions to it. They're
read-only or bookkeeping endpoints, with no schema changes. Until they're upstream, you
need a Worker that carries them.

| # | Extension | Why the app needs it |
|---|---|---|
| 1 | **Bearer-token owner auth**: `Authorization: Bearer <token>` is accepted wherever the owner session cookie is, when the Worker secret `BLYG_OWNER_TOKEN` is set. | A native app can't hold a studio cookie cleanly. |
| 2 | **Owner JSON reads**: `GET /api/items`, `GET /api/items/:id` (with `versions`), `GET /api/subscriptions`. | Upstream renders these lists as HTML only. |
| 3 | **Read extensions**: `GET /api/reading?limit&before=<cursor>` (opaque keyset cursor, pages ≤ limit), `GET /api/mentions`, `GET /api/settings` (public-safe fields only), `GET /api/hoppers`; `show_responses` added to the item JSON. | The reading list, mentions and settings screens. |
| 4 | **Client-recorded TK provenance**: `PUT /api/items/:id/tk-provenance {content_md?, scopes:[{index, model, sources?, at?} \| null]}` and `GET` of the same. Validated first, atomic with the text, never stores the instruction. | So text generated **in the app** is disclosed (`generated` + `blyg-tk-gen`) exactly like text the Worker generates itself. |

Field-level contracts: `docs/SPEC.md` § API and § Client-recorded provenance.

## Also used when present

- `DELETE /api/media/:id` (removes an upload; 404 unknown, 409 for the avatar)
  and `POST /api/media` answering `200 {…, duplicate: true}` for identical bytes
  on the same item. Without them, an abandoned paste leaves its file on the
  server.

## Degrading gracefully

- **No extension 1:** the app can't sign in with a token. (Password sign-in is planned.)
- **No extensions 2/3:** drafts still work locally and publish. Drafts written in the web studio
  won't appear, and the reading, mentions and settings screens say "not available on this server".
- **No extension 4:** before publishing text that was generated in the app, the app warns that
  it will go out **without** AI disclosure, and lets you cancel.

## Optional: server-side generation with Gemma 4

A Worker with a Workers AI binding (`"ai": {"binding": "AI"}`) can run TK generation on
`@cf/google/gemma-4-26b-a4b-it` with no API key. The app's "Generate on my blyg server" provider
uses whatever the Worker is configured to use.

## Status

The plan is to propose these extensions upstream. See the repo's issues for progress.
