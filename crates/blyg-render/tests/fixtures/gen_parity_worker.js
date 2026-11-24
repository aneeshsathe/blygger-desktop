// The Worker side of gen_parity.mjs: bundled together with the reference
// Worker's own render modules (`worker-src/*` is an esbuild alias for the
// Worker's src/) and run inside workerd through Miniflare, because the preview
// sanitizer (importer/sanitize.ts) is built on workerd's HTMLRewriter.
//
// POST / with { store, corpus, spec, vectors, media } returns every fixture's
// output. Nothing here re-implements Worker logic: render() is the body of
// read-api.ts's POST /api/preview handler (fragment) and threadPreview()
// (thread), with a fake D1 in place of the database.

import { renderMarkdown } from "worker-src/markdown.ts";
import { parseScopes, unrequestedOutputDirectives } from "worker-src/tk.ts";
import { annotateTkPreview } from "worker-src/authoring.ts";
import { applyInternalLinks, previewInternalLinks, previewTransclusions, resolveBlockLinks } from "worker-src/transclusion.ts";
import { injectNestedProvenance, injectProvenance, transclusionProvenance, mediaHtml } from "worker-src/pages.ts";
import { previewComments } from "worker-src/comments.ts";
import { consumeHandles, withMentionCards } from "worker-src/contacts.ts";
import { directiveLines } from "worker-src/directives.ts";
import { previewTangles } from "worker-src/read-api.ts";
import { DIRECTIVE_LINE, label, labelText, linkInline, linkSource, stripGeneratedGrammar } from "worker-src/grammar.ts";
import { linkLabel } from "worker-src/link-label.ts";
import { withStubQuote } from "worker-src/ui/stub-quote.ts";

// --- A fake D1 over fake_store.json ----------------------------------------------
// Answers exactly the queries resolveTarget() and transclusionProvenance() issue.
// The Rust FakeResolver (tests/common/mod.rs) implements the same store.
function fakeDb(store) {
  const answer = (sql, args) => {
    const s = sql.replace(/\s+/g, " ").trim();
    if (s === "SELECT * FROM items WHERE id = ?") {
      return { first: store.items.find((i) => i.id === args[0]) ?? null };
    }
    if (s === "SELECT * FROM versions WHERE item_id = ? AND version = ?") {
      const it = store.items.find((i) => i.id === args[0] && i.version === args[1]);
      return { first: it ? { item_id: it.id, version: it.version, content_html: it.content_html } : null };
    }
    if (s.startsWith("SELECT v.transclusions AS t FROM items i JOIN versions v")) {
      return { first: { t: null } }; // the fake store records no nested closures
    }
    if (s.startsWith("SELECT ii.*, s.origin AS sub_origin")) {
      const rows = store.imported.filter((r) => r.remote_id === args[0]).map((r) => ({ ...r, sub_origin: r.origin }));
      return { all: { results: rows } };
    }
    if (s.startsWith("SELECT ii.kind AS kind, ii.page AS page, s.title AS title")) {
      const r = store.imported.find((x) => x.remote_id === args[0] && x.origin === args[1]);
      return { first: r ? { kind: r.kind, page: r.page, title: r.sub_title } : null };
    }
    // An origin's item template (§16.6e; resolveInternalLinks, studio 0.67):
    // the fake store keeps none, so a link with no `page` takes the default
    // `items/{id}.json`, as Burrow's store (which holds no templates) does.
    if (s === "SELECT surface FROM subscriptions WHERE kind = 'blyg' AND origin = ? LIMIT 1") {
      return { first: null };
    }
    // The address book (consumeHandles; studio 0.66 asks for every candidate
    // handle, any case).
    if (s.startsWith("SELECT * FROM contacts WHERE")) {
      const asked = new Set(JSON.parse(args[0]).map((h) => h.toLowerCase()));
      return { all: { results: (store.contacts ?? []).filter((c) => asked.has(c.handle.toLowerCase())) } };
    }
    if (s === "SELECT 1 FROM contacts LIMIT 1") {
      return { first: store.contacts?.length ? { 1: 1 } : null };
    }
    throw new Error("fake D1: unexpected query: " + s);
  };
  return {
    prepare(sql) {
      let args = [];
      const stmt = {
        bind: (...a) => ((args = a), stmt),
        first: async () => answer(sql, args).first,
        all: async () => answer(sql, args).all,
      };
      return stmt;
    },
  };
}

// --- The preview pipeline (read-api.ts, POST /api/preview) -------------------------
// previewOperation (fragment) and threadPreview (thread) as of studio 0.68,
// step for step. `[[id]]` resolves against siteOrigin; here the mount plus
// "/", which is what the Rust side derives from `mount`.
async function render(store, c, fragmentDirectiveReason) {
  const db = fakeDb(store);
  const origin = store.mount + "/";
  // A thread's `![[+ …]]` spans and `[[+ …]]` placeholders (studio 0.62).
  const tangles = c.kind === "fragment" ? { md: c.md, finish: (h) => h, errors: [] } : await previewTangles(db, c.md, origin);
  // Comments to self are dropped and @handles resolved before anything else.
  const handles = await consumeHandles(db, previewComments(tangles.md));
  const tk = annotateTkPreview(handles.text);
  const tkOut = {
    scopes: tk.scopes.map((s) => ({ instruction: s.instruction, generated: s.output !== null, block: s.block, source_ids: s.sourceIds })),
    errors: parseScopes(c.md).errors,
  };
  const links = withMentionCards(await previewInternalLinks(db, tk.text, origin), handles.cards);
  const blocks = await resolveBlockLinks(tk, async (md) => withMentionCards(await previewInternalLinks(db, md, origin), handles.cards));
  const splice = (html) => [links, ...blocks.docs].reduce((acc, doc) => applyInternalLinks(acc, doc), tk.finish(html));
  const linkErrors = [...links.errors, ...blocks.errors];
  if (c.kind === "fragment") {
    // Fragments never resolve transclusions, and are not sanitized. A
    // directive line in one, in the body or in a generated block's output,
    // is a publish error (model.ts publishScoped, studio 0.67, #161): the
    // preview shows it as text, and the errors carry what publish refuses.
    const quoted = [links.text, ...tk.blockSources.values()].flatMap(directiveLines);
    const directiveErrors = quoted.map((directive) => ({ directive, reason: fragmentDirectiveReason }));
    return { html: splice(renderMarkdown(links.text)), tk: tkOut, transclusions: [], errors: [...directiveErrors, ...linkErrors] };
  }
  // threadPreview(): the generated blocks go to the walk, which bakes their
  // quotes between the block's wrappers (#105; outside them since 0.67, #102).
  const resolved = await previewTransclusions(db, links.text, store.self_id, tk);
  const preview = splice(resolved.html);
  // The studio's thread preview adds the provenance lines (injectProvenance,
  // then injectNestedProvenance) only when every quote resolved. The desktop
  // preview adds them always, so the generator does too, hiding unresolved
  // markers from them as they would be absent on the published page.
  const provenance = await transclusionProvenance(db, resolved.transclusions, store.mount);
  const MARKER = 'class="blyg-transclusion unresolved"';
  const HIDDEN = 'class="\u0000unresolved-marker"';
  const injected = await injectNestedProvenance(db, injectProvenance(preview.split(MARKER).join(HIDDEN), provenance), store.mount);
  return {
    html: tangles.finish(injected.split(HIDDEN).join(MARKER)),
    tk: tkOut,
    transclusions: resolved.transclusions,
    errors: [...resolved.errors, ...linkErrors, ...tangles.errors],
  };
}

export default {
  async fetch(req) {
    const { store, corpus, spec, vectors, media, grammar, fragmentDirectiveReason } = await req.json();
    const cases = [];
    for (const c of corpus) cases.push({ name: c.name, kind: c.kind, md: c.md, ...(await render(store, c, fragmentDirectiveReason)) });
    return Response.json({
      cases,
      // The CommonMark spec examples through renderMarkdown (html:false, linkify, embeds).
      spec: spec.examples.map((e) => ({ example: e.example, section: e.section, md: e.md, html: renderMarkdown(e.md) })),
      // linkify-it's and markdown-it's own linkify test vectors, as paragraphs.
      vectors: vectors.inputs.map((md) => ({ md, html: renderMarkdown(md) })),
      // Attachments: the public pages' mediaHtml, which appends what the
      // content does not already show.
      media: media.cases.map((c) => ({ ...c, media_html: mediaHtml(c.media, media.mount, c.content_html ?? "") })),
      // The bracket grammar (grammar.ts, link-label.ts): how each text's links
      // and directive read, what generated text keeps, and the long form.
      grammar: {
        texts: grammar.texts.map((text) => {
          const d = DIRECTIVE_LINE.exec(text);
          return {
            text,
            links: [...text.matchAll(linkInline())].map((m) => ({ match: m[0], id: m[1], label: label(m[2]) ?? null })),
            directive: d ? { id: d[1], label: label(d[2]) ?? null } : null,
            stripped: stripGeneratedGrammar(text),
          };
        }),
        labels: grammar.labels.map((text) => ({ text, label_text: labelText(text), link_source: linkSource(grammar.id, text) })),
        link_labels: grammar.html.map((h) => ({ ...h, label: linkLabel(h.content_html, h.kind) })),
        // The stub editor's passage chooser (ui/stub-quote.ts).
        stubs: grammar.stubs.map((c) => ({ ...c, out: withStubQuote(c.md, grammar.id, c.passage) })),
        // Own-line directives in TK output the instruction did not ask for (tk.ts).
        unrequested: grammar.outputs.map((md) => ({ md, ids: unrequestedOutputDirectives(md) })),
      },
    });
  },
};
