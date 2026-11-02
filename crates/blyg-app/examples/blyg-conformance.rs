//! blyg-conformance: Burrow's renderer as a blygger-spec reader-suite renderer
//! (`conformance/reader/` in blygger-spec, the renderer protocol in its README).
//!
//! ```sh
//! cargo build -p blyg-app --example blyg-conformance
//! cd <blygger-spec>/conformance/reader
//! node run.mjs --cmd "<target>/debug/examples/blyg-conformance --batch" --batch \
//!     --out <scratch dir> --name burrow
//! ```
//!
//! Without `--batch` it reads one case (JSON) on stdin and prints one result;
//! with it, `{cases: [...]}` in and `{renderer, results: [{id, ...}]}` out.
//!
//! Burrow publishes through the studio API, so the server renders the wire
//! document. What Burrow itself computes is the preview, which is built to be
//! what publish produces, so that is what this drives, with the app's own
//! code: `blyg_render::render_preview`, the studio's `StoreResolver` (your
//! posts first, then imported reading items) and the quoted-HTML sanitizer,
//! and the TK provenance the app records when it generates a scope
//! (`generate::prepare_fill`). Those three app modules are compiled in
//! from `src/` as they are, not copied. The case becomes the local store:
//! `items` with `local: true` are your posts, the rest reading items, and
//! `self` is the post being published (public at version 1 when it has a
//! `prior_content_md`, else a draft).
//!
//! Output: `content_html` (no `data-line`, no provenance line, which the
//! public page adds, not the wire; links absolute under `self.origin`),
//! `transclusions[]` (`Stats::transclusions`) and `generated[]` (one entry
//! per generated scope, its recorded sources). `links[]` is not reported:
//! Burrow does not compute it. A publish the studio refuses for what the
//! preview counts (an unresolved quote or link, an ungenerated or malformed
//! TK scope) is `{error}`.

use std::io::Read;

use blyg_core::{Item, Kind, LocalId, ReadingItem, ScopeProvenance, ServerId, Status};
use blyg_render::{RenderOpts, render_preview};
use serde_json::{Value, json};

// Top-level, so the resolver's `super::sanitize` is this crate's `sanitize`.
#[allow(dead_code)]
#[path = "../src/ai/generate.rs"]
mod generate;
#[allow(dead_code)]
#[path = "../src/studio/resolver.rs"]
mod resolver;
#[allow(dead_code)]
#[path = "../src/studio/sanitize.rs"]
mod sanitize;

use resolver::{StoreResolver, render_kind};

fn s<'a>(v: &'a Value, k: &str) -> &'a str {
    v.get(k).and_then(Value::as_str).unwrap_or("")
}

fn kind(k: &str) -> Kind {
    if k == "thread" {
        Kind::Thread
    } else {
        Kind::Fragment
    }
}

fn item(id: &str, kind: Kind, status: Status, version: u32, md: &str) -> Item {
    Item {
        local_id: LocalId(format!("local-{id}")),
        server_id: Some(ServerId(id.into())),
        kind,
        status,
        version,
        dirty: false,
        content_md: md.into(),
        created: String::new(),
        updated: String::new(),
        permalink: None,
        stub_of: None,
        forked_from: None,
        show_responses: true,
        responses_mode: None,
        pending_sync: false,
        conflict: false,
        highlight: false,
        highlight_mode: None,
    }
}

/// An imported item as the reading cache holds it. A tombstone keeps no
/// content unless a pin backs it, and then only the pinned version's
/// (store/reading.rs `retainable`).
fn reading(v: &Value) -> Option<ReadingItem> {
    let origin = s(v, "origin").to_string();
    let mut version = v.get("version").and_then(Value::as_u64).unwrap_or(1) as u32;
    let mut md = s(v, "content_md").to_string();
    let mut html = s(v, "content_html").to_string();
    let (state, retained) = match s(v, "state") {
        "published" => ("current", None),
        "withdrawn" => {
            md.clear();
            html.clear();
            ("tombstone", None)
        }
        "withdrawn-retained" => {
            let r = v.get("retained").and_then(Value::as_u64)? as u32;
            let pin = v.get("pins")?.get(r.to_string())?;
            md = s(pin, "content_md").into();
            html = s(pin, "content_html").into();
            version = r;
            ("tombstone", Some(r))
        }
        // Someone else's draft never reaches the importer.
        _ => return None,
    };
    Some(ReadingItem {
        subscription_id: format!("sub {origin}"),
        remote_id: s(v, "id").into(),
        subscription_title: String::new(),
        origin,
        kind: kind(s(v, "kind")),
        state: state.into(),
        version,
        created: None,
        updated: None,
        observed_at: String::new(),
        content_md: md,
        content_html: html,
        author: None,
        page: v.get("page").and_then(Value::as_str).map(String::from),
        thumb: None,
        hoppers: vec![],
        pinned_version_retained: retained,
        read_version: None,
        stub_of: None,
        forked_from: None,
        transclusions: vec![],
        lineage_known: false,
        source_url: None,
        generated: None,
    })
}

fn render(case: &Value) -> Value {
    let me = &case["self"];
    let self_id = s(me, "id");
    let md = s(case, "content_md");
    let case_kind = kind(s(case, "kind"));

    let mut items = Vec::new();
    let mut held = Vec::new();
    for v in case["items"].as_array().into_iter().flatten() {
        if v.get("local").and_then(Value::as_bool) == Some(true) {
            let status = match s(v, "state") {
                "draft" => Status::Draft,
                "withdrawn" | "withdrawn-retained" => Status::Withdrawn,
                _ => Status::Public,
            };
            let version = v.get("version").and_then(Value::as_u64).unwrap_or(0) as u32;
            items.push(item(
                s(v, "id"),
                kind(s(v, "kind")),
                status,
                version,
                s(v, "content_md"),
            ));
        } else if let Some(r) = reading(v) {
            held.push(r);
        }
    }
    // The post being published is one of your own: its working copy is the
    // case's text (what the store holds while you edit it).
    let published_before = me.get("prior_content_md").is_some();
    items.push(item(
        self_id,
        case_kind,
        if published_before {
            Status::Public
        } else {
            Status::Draft
        },
        u32::from(published_before),
        md,
    ));

    let mount = s(me, "origin").trim_end_matches('/').to_string();
    let resolver = StoreResolver::new(items.clone(), held.clone(), &[], &mount);
    let opts = RenderOpts {
        data_line: false,
        provenance: false,
        mount,
        self_id: Some(self_id.into()),
    };
    let out = render_preview(md, render_kind(case_kind), &resolver, &opts);

    let st = &out.stats;
    let mut refused: Vec<String> = st
        .unresolved
        .iter()
        .map(|u| format!("{}: {}", u.directive, u.reason))
        .collect();
    if st.ungenerated > 0 {
        refused.push(format!("{} ungenerated TK scope(s)", st.ungenerated));
    }
    refused.extend(st.tk_errors.iter().map(|e| format!("TK: {}", e.reason)));
    if !refused.is_empty() {
        return json!({ "error": refused.join("; ") });
    }

    let transclusions: Vec<Value> = st
        .transclusions
        .iter()
        .map(|q| {
            let mut t = json!({ "id": q.id, "version": q.version });
            if let Some(o) = &q.origin {
                t["origin"] = json!(o);
            }
            if let Some(sel) = &q.selector {
                let mut x = json!({ "exact": sel.exact });
                if let Some(p) = &sel.prefix {
                    x["prefix"] = json!(p);
                }
                if let Some(p) = &sel.suffix {
                    x["suffix"] = json!(p);
                }
                t["selector"] = x;
            }
            t
        })
        .collect();

    // As if the app had generated every scope with output, the way it does
    // from the editor: the scope index is the one provenance is keyed by
    // (`blyg_core::tk::parse`, as `under_caret` finds it), `prepare_fill`
    // lists the sources, and `provenance_with` records what is kept.
    let mut generated = Vec::new();
    let scopes = blyg_core::tk::parse(md).unwrap_or_default();
    for (i, sc) in scopes.iter().enumerate() {
        if sc.output.is_none() {
            continue;
        }
        // A source the app can't read: it can't generate, so no record.
        let Ok(job) = generate::prepare_fill(md, i, &items, &held, None) else {
            continue;
        };
        let p = ScopeProvenance {
            model: "conformance".into(),
            sources: job.sources,
            at: None,
        };
        let kept = generate::provenance_with(None, md, i, p).and_then(|v| v.get(i).cloned());
        let Some(Some(p)) = kept else { continue };
        let sources: Vec<Value> = p
            .sources
            .iter()
            .map(|p| {
                let mut x = json!({ "id": p.id });
                if let Some(v) = p.version {
                    x["version"] = json!(v);
                }
                if let Some(o) = &p.origin {
                    x["origin"] = json!(o);
                }
                x
            })
            .collect();
        generated.push(json!({ "sources": sources }));
    }

    json!({
        "content_html": out.html,
        "transclusions": transclusions,
        "generated": generated,
    })
}

fn main() {
    let mut input = String::new();
    if let Err(e) = std::io::stdin().read_to_string(&mut input) {
        eprintln!("blyg-conformance: reading stdin: {e}");
        std::process::exit(2);
    }
    let v: Value = match serde_json::from_str(&input) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("blyg-conformance: stdin is not JSON: {e}");
            std::process::exit(2);
        }
    };
    let batch = std::env::args().any(|a| a == "--batch");
    let out = if batch {
        let results: Vec<Value> = v["cases"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|c| {
                let mut r = render(c);
                r["id"] = c["id"].clone();
                r
            })
            .collect();
        json!({
            "renderer": { "name": "burrow", "version": env!("CARGO_PKG_VERSION") },
            "results": results,
        })
    } else {
        render(&v)
    };
    println!("{out}");
}
