//! Parity with the reference Worker: every fixture in `tests/fixtures/parity`
//! was produced by the Worker's own TypeScript renderer (`gen_parity.mjs`).
//! HTML is compared after collapsing whitespace between tags.

mod common;

use blyg_render::{Attachment, Kind, RenderOpts, media_html, preview_media, render_preview};
use common::{FakeStore, fixtures_dir, normalize};
use serde_json::Value;

/// Fixtures whose output is known to differ, with the reason. Keep this
/// list honest and short; `docs/RENDER.md` explains each entry.
const KNOWN_DIVERGENCES: &[(&str, &str)] = &[
    // Decision #60: an own-line `![[id]]` in a block scope's output is a
    // quote at publish. The studio's preview (and its publish, for a scope
    // with recorded provenance) renders the block on its own and shows the
    // directive as text; this preview bakes it, as the decision says publish
    // does. `tests/preview.rs` pins the output.
    (
        "tr_block_tk_directive_output",
        "decision #60: directive in block TK output is a quote",
    ),
    (
        "tr_block_tk_directive_split",
        "decision #60: directive in block TK output is a quote",
    ),
    (
        "tr_block_tk_directive_attached",
        "decision #60: directive in block TK output is a quote",
    ),
    (
        "tr_in_tk_block",
        "decision #60: directive in block TK output is a quote",
    ),
];

/// Where protocol 0.4's fifteenth revision (decision #102, 2026-11-12) and
/// #81's label bake differ from the fixtures (stock studio 0.49.1), the
/// expected output is rewritten here, so the rest of each case is still
/// compared byte for byte. `docs/RENDER.md` ("Decision #102") explains each.
///
/// #81: a directive's label is baked as `data-blyg-label` (escaped), in
/// the order of the case's top-level quotes; `""` is no label.
const SPEC_LABELS: &[(&str, &[&str])] = &[
    (
        "label_directive_forms",
        &["tight", "padded", "", "either | or"],
    ),
    ("label_directive_in_code", &["real"]),
    ("label_directive_partial", &["the tools line"]),
    ("label_directive_quote", &["note to self"]),
    ("label_directive_remote", &["a friend&#39;s line"]),
    ("label_tk_output_directive", &["echoed"]),
];

/// #102 (9): a baked quote is never inside a `blyg-tk-gen` wrapper; an
/// inline span that a quote interrupts closes before it and reopens after.
const SPEC_REWRITES: &[(&str, &[(&str, &str)])] = &[
    (
        "tr_inline_tk_multiline_directive",
        &[
            ("one</p>", "one</span></p>"),
            ("<p>two</span>", "<p><span class=\"blyg-tk-gen\">two</span>"),
        ],
    ),
    (
        "label_tk_output_directive",
        &[
            ("one</p>", "one</span></p>"),
            ("<p>two</span>", "<p><span class=\"blyg-tk-gen\">two</span>"),
        ],
    ),
    (
        "tr_ungenerated_tk_with_directive",
        &[
            ("use</p>", "use</span></p>"),
            (
                "<p>please</span>",
                "<p><span class=\"blyg-tk-gen\">please</span>",
            ),
        ],
    ),
    // #102 (2): the reserved form with a label is reserved too.
    (
        "label_directive_malformed",
        &[(
            "<p>![[01j9zq3k4m5n6p7q8r9s0t1v2w@v2 | pinned]]</p>",
            "<blockquote class=\"blyg-transclusion unresolved\"><p>⚠ unresolvable: explicit-version references (@vN) are reserved, not supported in v0.1</p></blockquote>",
        )],
    ),
];

/// #102 (1) and (2): publish errors the fixtures (which publish both) lack.
const SPEC_ERRORS: &[(&str, &[(&str, &str)])] = &[
    (
        "tr_in_fragment",
        &[("![[01j9zq3k4m5n6p7q8r9s0t1v2w]]", FRAGMENT_DIRECTIVE)],
    ),
    (
        "label_directive_fragment",
        &[("![[01j9zq3k4m5n6p7q8r9s0t1v2w | note]]", FRAGMENT_DIRECTIVE)],
    ),
    (
        "label_directive_malformed",
        &[(
            "![[01j9zq3k4m5n6p7q8r9s0t1v2w@v2 | pinned]]",
            "explicit-version references (@vN) are reserved, not supported in v0.1",
        )],
    ),
];

const FRAGMENT_DIRECTIVE: &str = "a fragment cannot carry a transclusion directive; quote from a thread, or put the line in code";

/// The fixture's HTML (normalised) as protocol 0.4 r15 has it.
fn spec_expected(name: &str, html: &str, no_page: &[String]) -> String {
    let mut out = normalize(html);
    // #102 (5): a link to an imported item with no `page` names its item
    // document. The provenance line (`from …`) is presentation and keeps
    // the convention.
    for url in no_page {
        let (base, id) = url
            .rsplit_once("/f/")
            .or_else(|| url.rsplit_once("/t/"))
            .unwrap();
        let id = id.trim_end_matches('/');
        let from = format!("<a href=\"{url}\">");
        let to = format!("<a href=\"{base}/items/{id}.json\">");
        let mut rebuilt = String::new();
        let mut rest = out.as_str();
        while let Some(i) = rest.find(&from) {
            let after = &rest[i + from.len()..];
            rebuilt.push_str(&rest[..i]);
            rebuilt.push_str(if after.starts_with("from ") {
                &from
            } else {
                &to
            });
            rest = after;
        }
        rebuilt.push_str(rest);
        out = rebuilt;
    }
    if let Some((_, labels)) = SPEC_LABELS.iter().find(|(n, _)| *n == name) {
        let open = "<blockquote class=\"blyg-transclusion";
        let mut rebuilt = String::new();
        let mut rest = out.as_str();
        let mut labels = labels.iter();
        while let Some(i) = rest.find(open) {
            let gt = i + rest[i..].find('>').unwrap();
            rebuilt.push_str(&rest[..gt]);
            match labels.next() {
                Some(l) if !l.is_empty() => {
                    rebuilt.push_str(&format!(" data-blyg-label=\"{l}\""));
                }
                _ => {}
            }
            rest = &rest[gt..];
        }
        rebuilt.push_str(rest);
        out = rebuilt;
    }
    for (_, pairs) in SPEC_REWRITES.iter().filter(|(n, _)| *n == name) {
        for (from, to) in *pairs {
            assert!(out.contains(from), "{name}: rewrite {from:?} found nothing");
            out = out.replacen(from, to, 1);
        }
    }
    out
}

fn load_fixtures() -> Vec<Value> {
    let dir = fixtures_dir().join("parity");
    let mut paths: Vec<_> = std::fs::read_dir(&dir)
        .expect("parity dir")
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .filter(|p| !p.file_name().unwrap().to_string_lossy().starts_with('_'))
        .collect();
    paths.sort();
    paths
        .into_iter()
        .map(|p| serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap())
        .collect()
}

#[test]
fn parity_with_worker() {
    let store = FakeStore::load();
    let opts = RenderOpts {
        data_line: false,
        provenance: true,
        mount: store.mount(),
        self_id: Some(store.self_id()),
    };
    let fixtures = load_fixtures();
    assert!(
        fixtures.len() >= 80,
        "expected the full corpus, found {}",
        fixtures.len()
    );

    // Imported items with no `page`, by the URL the convention gave them.
    let no_page: Vec<String> = store.json["imported"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["page"].is_null())
        .map(|r| {
            let seg = if r["kind"] == "thread" { "t" } else { "f" };
            format!(
                "{}{seg}/{}/",
                r["origin"].as_str().unwrap(),
                r["remote_id"].as_str().unwrap()
            )
        })
        .collect();
    let mut exact = 0;
    let mut normalized = 0;
    let mut rewritten = 0;
    let mut failures = Vec::new();
    let mut unexpected_passes = Vec::new();
    for f in &fixtures {
        let name = f["name"].as_str().unwrap();
        let kind = if f["kind"] == "thread" {
            Kind::Thread
        } else {
            Kind::Fragment
        };
        let md = f["md"].as_str().unwrap();
        let expected = f["html"].as_str().unwrap();
        let got = render_preview(md, kind, &store, &opts);
        let known = KNOWN_DIVERGENCES.iter().any(|(n, _)| *n == name);
        if got.html == expected {
            exact += 1;
        }
        let spec = spec_expected(name, expected, &no_page);
        if spec != normalize(expected) {
            rewritten += 1;
        }
        if normalize(&got.html) == spec {
            normalized += 1;
            if known {
                unexpected_passes.push(name.to_string());
            }
        } else if !known {
            failures.push(format!(
                "--- {name}\n  md:       {md:?}\n  expected: {spec}\n  got:      {}",
                normalize(&got.html)
            ));
        }

        // Unresolved quotes and links (reason and directive, in order) and
        // quotes must agree with the Worker too.
        let mut errors: Vec<(String, String)> = f["errors"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| {
                let s = |k: &str| e[k].as_str().unwrap().to_string();
                (s("directive"), s("reason"))
            })
            .collect();
        if let Some((_, extra)) = SPEC_ERRORS.iter().find(|(n, _)| *n == name) {
            assert!(errors.is_empty(), "{name}: the fixture has errors now");
            errors = extra
                .iter()
                .map(|(d, r)| (d.to_string(), r.to_string()))
                .collect();
        }
        let got_errors: Vec<(String, String)> = got
            .stats
            .unresolved
            .iter()
            .map(|u| (u.directive.clone(), u.reason.to_string()))
            .collect();
        if got_errors != errors {
            failures.push(format!(
                "--- {name}: unresolved\n  expected: {errors:?}\n  got:      {got_errors:?}"
            ));
        }
        if kind == Kind::Thread && !known {
            let ids: Vec<String> = f["transclusions"]
                .as_array()
                .unwrap()
                .iter()
                .map(|t| t["id"].as_str().unwrap().to_string())
                .collect();
            let got_ids: Vec<String> = got
                .stats
                .transclusions
                .iter()
                .map(|t| t.id.clone())
                .collect();
            if got_ids != ids {
                failures.push(format!(
                    "--- {name}: transclusions\n  expected: {ids:?}\n  got:      {got_ids:?}"
                ));
            }
        }
        let tk_errors = f["tk"]["errors"].as_array().unwrap().len();
        if got.stats.tk_errors.len() != tk_errors {
            failures.push(format!(
                "--- {name}: TK errors\n  expected: {tk_errors}\n  got:      {}",
                got.stats.tk_errors.len()
            ));
        }
    }
    eprintln!(
        "parity: {normalized}/{} match after whitespace normalisation ({exact} byte-identical, {rewritten} rewritten to protocol 0.4 r15); {} known divergences",
        fixtures.len(),
        KNOWN_DIVERGENCES.len()
    );
    assert!(
        unexpected_passes.is_empty(),
        "now passing, remove from KNOWN_DIVERGENCES: {unexpected_passes:?}"
    );
    assert!(
        failures.is_empty(),
        "{} parity failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// Attachments: `mediaHtml`, which public pages put after the content. The
/// desktop preview's strip wraps exactly that (the Worker's own strip,
/// `previewMedia`, went with its server-rendered studio in 0.10).
#[test]
fn media_parity_with_worker() {
    let text = std::fs::read_to_string(fixtures_dir().join("parity/_media.json")).unwrap();
    let doc: Value = serde_json::from_str(&text).unwrap();
    let mount = doc["mount"].as_str().unwrap();
    let cases = doc["cases"].as_array().unwrap();
    assert!(cases.len() >= 3);
    for c in cases {
        let media: Vec<Attachment> = c["media"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| Attachment {
                key: m["r2_key"].as_str().unwrap().to_string(),
                alt: m["alt"].as_str().map(str::to_string),
                inline: m["inline"] == 1,
            })
            .collect();
        let name = &c["name"];
        let content = c["content_html"].as_str().unwrap_or("");
        assert_eq!(
            media_html(&media, mount, content),
            c["media_html"],
            "{name}"
        );
        let strip = format!(
            "<div id=\"preview-media\">{}</div>",
            c["media_html"].as_str().unwrap()
        );
        assert_eq!(preview_media(&media, mount, content), strip, "{name}");
    }
}

/// The bracket grammar (studio grammar.ts and link-label.ts, 0.48): how each
/// text's links and directive read, what generated text keeps, the long
/// form and the default link label, against the studio's own functions.
#[test]
fn grammar_parity_with_worker() {
    use blyg_render::{
        ItemKind, directive_ref, label_text, link_label, link_refs, link_source,
        strip_generated_grammar,
    };
    let text = std::fs::read_to_string(fixtures_dir().join("parity/_grammar.json")).unwrap();
    let g: Value = serde_json::from_str(&text).unwrap();
    let id = g["id"].as_str().unwrap();
    let texts = g["texts"].as_array().unwrap();
    assert!(texts.len() >= 40, "expected the grammar vectors");
    for t in texts {
        let text = t["text"].as_str().unwrap();
        let links: Vec<Value> = link_refs(text)
            .into_iter()
            .map(|l| serde_json::json!({ "match": &text[l.range], "id": l.id, "label": l.label }))
            .collect();
        assert_eq!(Value::Array(links), t["links"], "links of {text:?}");
        let directive = directive_ref(text)
            .map(|d| serde_json::json!({ "id": d.id, "label": d.label }))
            .unwrap_or(Value::Null);
        assert_eq!(directive, t["directive"], "directive of {text:?}");
        assert_eq!(
            strip_generated_grammar(text),
            t["stripped"].as_str().unwrap(),
            "stripGeneratedGrammar({text:?})"
        );
    }
    for l in g["labels"].as_array().unwrap() {
        let text = l["text"].as_str().unwrap();
        assert_eq!(
            label_text(text),
            l["label_text"].as_str().unwrap(),
            "labelText({text:?})"
        );
        assert_eq!(
            link_source(id, Some(text)),
            l["link_source"].as_str().unwrap(),
            "linkSource({text:?})"
        );
    }
    for h in g["link_labels"].as_array().unwrap() {
        let kind = if h["kind"] == "thread" {
            ItemKind::Thread
        } else {
            ItemKind::Fragment
        };
        let html = h["content_html"].as_str().unwrap();
        assert_eq!(
            link_label(html, kind),
            h["label"].as_str().unwrap(),
            "linkLabel({html:?})"
        );
    }
    // Studio 0.48: choosing a passage keeps the directive line as written.
    let stubs = g["stubs"].as_array().unwrap();
    assert!(!stubs.is_empty(), "expected the stub vectors");
    for s in stubs {
        let md = s["md"].as_str().unwrap();
        let passage = s["passage"].as_str();
        assert_eq!(
            blyg_render::stub_quote::with_stub_quote(md, id, passage),
            s["out"].as_str().unwrap(),
            "withStubQuote({md:?}, {passage:?})"
        );
    }
    // Studio 0.48: an own-line directive in TK output takes the same
    // whitespace as `DIRECTIVE_LINE` (NBSP, FF, VT, CR).
    let outputs = g["unrequested"].as_array().unwrap();
    assert!(!outputs.is_empty(), "expected the TK output vectors");
    for o in outputs {
        let md = o["md"].as_str().unwrap();
        let ids: Vec<Value> = blyg_render::unrequested_output_directives(md)
            .into_iter()
            .map(Value::String)
            .collect();
        assert_eq!(
            Value::Array(ids),
            o["ids"],
            "unrequestedOutputDirectives({md:?})"
        );
    }
}
