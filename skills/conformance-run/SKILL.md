---
name: conformance-run
description: Run the Blygger spec's conformance toolkit against a client. Use before a client release, after changing rendering, parsing, order or wire documents, or when a spec revision lands.
---

# Running the spec's conformance toolkit

The toolkit lives in the spec repo under `conformance/` (other repo: blygger-spec);
the paths in this table are inside it. Read its `README.md`; in short:

| Tool | Use it for |
|---|---|
| `reader/` | Rendering: `content_md` → expected `content_html` fixtures written from the spec text. Give the runner your renderer as an adapter command. |
| `schemas/` | Wire documents: JSON Schemas, cross-field rules, `![[id]]`/`[[id]]` syntax cases. Clients supply an adapter (`schemas/ADAPTERS.md`) (other repo: blygger-spec). |
| `feeds/` | List order: the pace-layered order's fixtures (publishing histories and the order they produce). |
| `clauses/` | The clause register: every normative sentence with an ID; map your tests to IDs. |
| `live/`, `alloy/`, `model/` | Network-wide invariants and protocol models; mostly for spec work, not clients. |

## Steps

1. Pull the spec repo at the revision your client targets; note the commit.
2. Write or update your client's adapter for `reader/` and `schemas/`.
3. Run them; record pass/fail counts in your client's docs with the spec
   commit.
4. For each failure: fix it, or file it as a known gap with the clause ID (and
   upstream, if the spec or studio is wrong; quote the clause).
5. Re-run whenever the spec bumps a revision or the studio's rendering changes.

## Never

Commit live blygs' item text to fixtures or reports: ids, URLs and findings
only. Use invented content for your own fixtures.

## In Burrow

- Reader conformance: `scripts/reader-conformance.sh` (see
  `docs/CONFORMANCE-READER.md`; expected results in
  `scripts/reader-conformance.expected`, pinned to a spec commit).
- Check `blyg-core/src/order.rs` against the spec's `conformance/feeds/` (other repo: blygger-spec)
  fixtures when either changes.
