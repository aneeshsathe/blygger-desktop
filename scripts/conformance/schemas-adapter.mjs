// Burrow as a blygger-spec schemas-suite implementation (conformance/schemas/
// in blygger-spec, the interface in its ADAPTERS.md). Run it through
// scripts/schemas-conformance.sh, which passes the adapter binary in
// BLYG_CONFORMANCE_BIN.
//
// grammar(): each §10.1 grammar case goes through the same adapter the reader
// suite drives (crates/blyg-app/examples/blyg-conformance.rs): Burrow's
// publish preview, with the case's text as a thread being published, your
// local fragment ({{L}}) as one of your posts, and the fake remote blyg's
// items as imported reading items. A case with `generate: false` has TK output
// the author wrote, so Burrow records no provenance for it.
//
// No samples(): Burrow publishes through the studio API, so the documents it
// publishes are the studio's, checked by the studio's column. No roundtrip()
// or staticFields(): Burrow does not import or store item documents; the
// studio imports them and Burrow reads the owner API (docs/CONFORMANCE.md).
import { execFileSync } from "node:child_process";

const BIN = process.env.BLYG_CONFORMANCE_BIN;
const LOCAL_ID = "01j9zq3k4m5n6p7q8r9s0t1b00"; // an id (§4: 26 chars, Crockford base32)
const SELF = { id: "01j9zq3k4m5n6p7q8r9s0t1bzz", origin: "https://burrow.example/" };

function run(cases) {
  if (!BIN) throw new Error("BLYG_CONFORMANCE_BIN is not set (use scripts/schemas-conformance.sh)");
  const out = execFileSync(BIN, ["--batch"], { input: JSON.stringify({ cases }), maxBuffer: 64 << 20 });
  return JSON.parse(out.toString("utf8"));
}

export default {
  id: "burrow",
  name: "Burrow",

  version() {
    return run([]).renderer.version;
  },

  grammar(cases, ctx) {
    const remote = ctx.remote.targets.map((t) => {
      const doc = ctx.remote.items[t.id] ?? {};
      const pins = Object.fromEntries(
        Object.entries(ctx.remote.pins[t.id] ?? {}).map(([n, p]) => [n, { content_md: p.content_md, content_html: p.content_html }]),
      );
      return { ...t, local: false, state: "published", content_md: doc.content_md ?? "", pins };
    });
    const local = { id: LOCAL_ID, origin: SELF.origin, local: true, kind: "fragment", version: 1, state: "published", page: `f/${LOCAL_ID}/`, ...ctx.local };
    const input = cases.map((c0) => {
      const c = ctx.subst(c0, LOCAL_ID);
      return { id: c.id, kind: "thread", self: SELF, content_md: c.content_md, items: [local, ...remote] };
    });
    const byId = new Map(run(input).results.map((r) => [r.id, r]));
    const results = cases.map((c) => {
      const r = byId.get(c.id) ?? { error: "no result" };
      if (r.error) return { case: c.id, error: true, errorText: r.error };
      return {
        case: c.id,
        error: false,
        html: r.content_html,
        transclusions: (r.transclusions ?? []).map((t) => ({ id: t.id, partial: !!t.selector, exact: t.selector?.exact })),
        tk_sources: c.generate === false ? [] : (r.generated ?? []).map((g) => g.sources.map((x) => x.id)),
      };
    });
    return { localId: LOCAL_ID, implementation: "Burrow (publish preview: blyg-render + StoreResolver)", results };
  },
};
