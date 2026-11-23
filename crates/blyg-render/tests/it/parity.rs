//! Parity with the reference Worker: every fixture in `tests/fixtures/parity`
//! was produced by the Worker's own TypeScript renderer (`gen_parity.mjs`).
//! HTML is compared after collapsing whitespace between tags, and formulas
//! by shape (`math_shape.rs`).

use crate::common::{FakeStore, fixtures_dir, normalize};
use crate::math_shape;
use blyg_render::{Attachment, Kind, RenderOpts, media_html, preview_media, render_preview};
use serde_json::Value;

/// Fixtures whose output is known to differ for good, with the reason. Keep
/// this list honest and short; `docs/RENDER.md` explains each entry.
const KNOWN_DIVERGENCES: &[(&str, &str)] = &[];

/// Cases that wait for a work package being built alongside this re-sync
/// (studio 0.64.1 parity): WP1 tangling. Each is expected to
/// fail until its package lands; once it passes, the test says so and the
/// entry must go. Empty this list after merging WP1.
const PENDING: &[(&str, &str)] = &[
    // WP1: `![[+ …]]` spans preview as a "new item" quote and `[[+ …]]`
    // placeholders as a linked span (studio `read-api.ts` previewTangles).
];

/// Studio changes this crate has not followed yet. Expected to fail; each
/// passing entry must go. Empty since the 0.64.1 re-sync adopted labelled
/// and nested provenance, lazy continuation, sanitized generated blocks and
/// comments to self (`docs/RENDER.md`, "Studio 0.64.1").
const TO_ADOPT: &[(&str, &str)] = &[];

/// Where protocol 0.4's fifteenth revision (decision #102, 2026-11-12)
/// differs from the fixtures (stock studio), the expected output is
/// rewritten here, so the rest of each case is still compared byte for
/// byte. `docs/RENDER.md` ("Decision #102") explains each.
///
/// #102 (9): an inline span that a quote interrupts closes before it and
/// reopens after. (Studio 0.67 #161 took up the block form, the fragment's
/// directive line and the no-page link address; the fixtures have them.)
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
    // #102 (9) drops a wrapper left empty; studio 0.67 (#161) leaves an
    // empty `div.blyg-tk-gen` between two quotes in a row (blank prose
    // between them is flushed as a part). Not yet reported upstream.
    (
        "tr_block_tk_two_quotes",
        &[(
            "</blockquote><div class=\"blyg-tk-gen\"></div><blockquote",
            "</blockquote><blockquote",
        )],
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

/// #102 (2): publish errors the fixture (which publishes it) lacks.
const SPEC_ERRORS: &[(&str, &[(&str, &str)])] = &[(
    "label_directive_malformed",
    &[(
        "![[01j9zq3k4m5n6p7q8r9s0t1v2w@v2 | pinned]]",
        "explicit-version references (@vN) are reserved, not supported in v0.1",
    )],
)];

/// The fixture's HTML (normalised) as protocol 0.4 r15 has it.
fn spec_expected(name: &str, html: &str) -> String {
    let mut out = normalize(html);
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

/// Which expected-to-fail list names `name`, if any.
fn expected_to_fail(name: &str) -> Option<&'static str> {
    [
        ("KNOWN_DIVERGENCES", KNOWN_DIVERGENCES),
        ("PENDING", PENDING),
        ("TO_ADOPT", TO_ADOPT),
    ]
    .into_iter()
    .find(|(_, list)| list.iter().any(|(n, _)| *n == name))
    .map(|(list, _)| list)
}

#[test]
fn expected_to_fail_lists_name_real_cases_once() {
    let names: Vec<String> = load_fixtures()
        .iter()
        .map(|f| f["name"].as_str().unwrap().to_string())
        .collect();
    let mut seen = std::collections::HashSet::new();
    for (n, _) in KNOWN_DIVERGENCES.iter().chain(PENDING).chain(TO_ADOPT) {
        assert!(names.iter().any(|x| x == n), "{n}: no such fixture");
        assert!(seen.insert(*n), "{n}: listed twice");
    }
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

    let mut exact = 0;
    let mut matched = 0;
    let mut rewritten = 0;
    let mut formulas = 0;
    let mut failures = Vec::new();
    let mut unexpected_passes = Vec::new();
    let mut expected_fails = 0;
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
        let mut problems = Vec::new();
        if got.html == expected {
            exact += 1;
        }
        let spec = spec_expected(name, expected);
        if spec != normalize(expected) {
            rewritten += 1;
        }
        let (spec, studio_math) = math_shape::shape(&spec);
        assert!(
            studio_math.is_empty(),
            "{name}: the studio's own formulas fail the shape check: {studio_math:?}"
        );
        if expected.contains("<math") {
            formulas += 1;
        }
        let (got_html, got_math) = math_shape::shape(&normalize(&got.html));
        for p in got_math {
            problems.push(format!("--- {name}: formula\n  {p}"));
        }
        if got_html != spec {
            problems.push(format!(
                "--- {name}\n  md:       {md:?}\n  expected: {spec}\n  got:      {got_html}"
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
            problems.push(format!(
                "--- {name}: unresolved\n  expected: {errors:?}\n  got:      {got_errors:?}"
            ));
        }
        if kind == Kind::Thread {
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
                problems.push(format!(
                    "--- {name}: transclusions\n  expected: {ids:?}\n  got:      {got_ids:?}"
                ));
            }
        }
        let tk_errors = f["tk"]["errors"].as_array().unwrap().len();
        if got.stats.tk_errors.len() != tk_errors {
            problems.push(format!(
                "--- {name}: TK errors\n  expected: {tk_errors}\n  got:      {}",
                got.stats.tk_errors.len()
            ));
        }

        match (expected_to_fail(name), problems.is_empty()) {
            (None, true) => matched += 1,
            (None, false) => failures.extend(problems),
            (Some(list), true) => unexpected_passes.push(format!("{name} (in {list})")),
            (Some(_), false) => expected_fails += 1,
        }
    }
    eprintln!(
        "parity: {matched}/{} match ({exact} byte-identical, {rewritten} rewritten to the spec (r15, r21), {formulas} with formulas compared by shape); {expected_fails} expected to fail ({} known, {} pending WP1/WP2, {} to adopt)",
        fixtures.len(),
        KNOWN_DIVERGENCES.len(),
        PENDING.len(),
        TO_ADOPT.len()
    );
    assert!(
        unexpected_passes.is_empty(),
        "now passing, remove from its list: {unexpected_passes:?}"
    );
    assert!(
        failures.is_empty(),
        "{} parity failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// The stub chooser's `![[+ …]]` vectors (`withStubQuote`), expected to
/// fail until WP1 lands; then set this to `None`.
const STUB_SPANS_PENDING: Option<&str> = None;

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
    // Studio 0.62: a directive inside a `![[+ …]]` span is out of bounds, as
    // code is (those vectors wait for WP1, `STUB_SPANS_PENDING`).
    let stubs = g["stubs"].as_array().unwrap();
    assert!(!stubs.is_empty(), "expected the stub vectors");
    let mut span_misses = 0;
    for s in stubs {
        let md = s["md"].as_str().unwrap();
        let passage = s["passage"].as_str();
        let got = blyg_render::stub_quote::with_stub_quote(md, id, passage);
        if STUB_SPANS_PENDING.is_some() && md.contains("![[+") {
            span_misses += usize::from(got != s["out"].as_str().unwrap());
            continue;
        }
        assert_eq!(
            got,
            s["out"].as_str().unwrap(),
            "withStubQuote({md:?}, {passage:?})"
        );
    }
    if let Some(why) = STUB_SPANS_PENDING {
        assert!(
            span_misses > 0,
            "the span stub vectors pass now: set STUB_SPANS_PENDING to None ({why})"
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
