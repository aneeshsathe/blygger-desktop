//! Behaviour beyond byte parity: `data-line` source mapping, stats, safety,
//! and the page shell.

mod common;

use blyg_render::{
    Kind, NoResolver, RenderOpts, UnresolvedReason, article_html, page_shell, render_preview,
};
use common::{FakeStore, fixtures_dir};

const A: &str = "01j9zq3k4m5n6p7q8r9s0t1v2w"; // local fragment, v3
const R: &str = "02abcdefghjkmnpqrstvwxyz01"; // remote fragment, v5
const U: &str = "03zzzzzzzzzzzzzzzzzzzzzzzz"; // unknown

fn opts() -> RenderOpts {
    let store = FakeStore::load();
    RenderOpts {
        self_id: Some(store.self_id()),
        ..RenderOpts::default()
    }
}

fn strip_lines(html: &str) -> String {
    regex::Regex::new(r#" data-line="\d+""#)
        .unwrap()
        .replace_all(html, "")
        .into_owned()
}

#[test]
fn data_line_on_top_level_blocks() {
    let md = "# Heading\n\nA paragraph\nover two lines.\n\n- a\n- b\n\n```\ncode\n```\n\n> quote";
    let out = render_preview(md, Kind::Fragment, &NoResolver, &RenderOpts::default());
    assert!(out.html.contains("<h1 data-line=\"0\">Heading</h1>"));
    assert!(out.html.contains("<p data-line=\"2\">A paragraph"));
    assert!(out.html.contains("<ul data-line=\"5\">"));
    assert!(out.html.contains("<pre data-line=\"8\"><code>code"));
    assert!(out.html.contains("<blockquote data-line=\"12\">"));
    assert_eq!(out.line_map, vec![(0, 0), (2, 1), (5, 2), (8, 3), (12, 4)]);
    // Nested blocks carry no line.
    assert!(out.html.contains("<li>a</li>"));
}

#[test]
fn data_line_through_quotes_and_tk_blocks() {
    let store = FakeStore::load();
    let md = format!(
        "Intro\n\n![[{A}]]\n\n[TK]two paragraphs[=]One.\n\nTwo.[/TK]\n\n![[{U}]]\n\nhttps://youtu.be/Qa1b2C3d4E5\n\nEnd"
    );
    let out = render_preview(&md, Kind::Thread, &store, &opts());
    let h = &out.html;
    assert!(h.contains("<p data-line=\"0\">Intro</p>"), "{h}");
    assert!(h.contains(&format!(
        "<blockquote class=\"blyg-transclusion\" data-blyg-id=\"{A}\" data-blyg-version=\"3\" data-line=\"2\">"
    )));
    // The provenance line still lands inside the quote.
    assert!(h.contains("<p class=\"provenance\"><a href=\"/blyg/f/01j9zq3k4m5n6p7q8r9s0t1v2w/\">fragment ↗</a> · snapshot of v3</p></blockquote>"));
    assert!(
        h.contains("<div class=\"blyg-tk-gen\" data-line=\"4\"><p>One.</p>\n<p>Two.</p>\n</div>"),
        "{h}"
    );
    assert!(h.contains("<blockquote class=\"blyg-transclusion unresolved\" data-line=\"8\"><p>⚠ unresolvable: unknown item</p></blockquote>"));
    // A bare video link stays a link (stock studio upgrades it on screen,
    // never in the HTML), and still counts as a video.
    assert!(
        h.contains("<p data-line=\"10\"><a href=\"https://youtu.be/Qa1b2C3d4E5\">"),
        "{h}"
    );
    assert_eq!(out.stats.videos, 1);
    assert!(h.contains("<p data-line=\"12\">End</p>"));
    assert_eq!(
        out.line_map,
        vec![(0, 0), (2, 1), (4, 2), (8, 3), (10, 4), (12, 5)]
    );
}

/// Turning line attributes on must change nothing else, for every fixture.
#[test]
fn data_line_is_purely_additive() {
    let store = FakeStore::load();
    let with = opts();
    let without = RenderOpts {
        data_line: false,
        ..opts()
    };
    let dir = fixtures_dir().join("parity");
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.file_name().unwrap().to_string_lossy().starts_with('_') {
            continue;
        }
        let f: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let kind = if f["kind"] == "thread" {
            Kind::Thread
        } else {
            Kind::Fragment
        };
        let md = f["md"].as_str().unwrap();
        let a = render_preview(md, kind, &store, &with);
        let b = render_preview(md, kind, &store, &without);
        assert_eq!(strip_lines(&a.html), b.html, "{}", f["name"]);
        assert_eq!(
            a.line_map.len(),
            a.html.matches(" data-line=\"").count(),
            "{}",
            f["name"]
        );
        assert!(b.line_map.is_empty());
    }
}

#[test]
fn stats_for_the_status_bar() {
    let store = FakeStore::load();
    let md = format!(
        "![[{A}]]\n\n![[{R}]]\n\n![[{U}]]\n\n![[{A}@v2]]\n\n\
         One [TK]a[=]alpha[/TK], two [TK]b[/TK], three [TK]nested [TK]x[/TK] y[/TK].\n\n\
         ![p](https://images.example.org/p.png) ![q](/local.png)\n\n\
         https://youtu.be/Qa1b2C3d4E5\n\n[TK]block[=]![r](/r.png)[/TK]"
    );
    let out = render_preview(&md, Kind::Thread, &store, &opts());
    let s = &out.stats;
    assert_eq!(s.quotes, 2);
    assert_eq!(
        s.transclusions
            .iter()
            .map(|t| t.id.as_str())
            .collect::<Vec<_>>(),
        [A, R]
    );
    assert_eq!(
        s.transclusions[1].origin.as_deref(),
        Some("https://blyg.example.com/")
    );
    assert_eq!(s.unresolved.len(), 2);
    assert_eq!(s.unresolved[0].line, 4);
    assert_eq!(s.unresolved[0].reason, UnresolvedReason::UnknownItem);
    assert_eq!(s.unresolved[0].directive, format!("![[{U}]]"));
    assert_eq!(s.unresolved[1].reason, UnresolvedReason::ReservedVersion);
    assert_eq!(
        s.unresolved[0].reason.human(),
        "it isn't in your posts or your reading list"
    );
    assert_eq!(s.ai_spans, 2);
    assert_eq!(s.ungenerated, 1);
    assert_eq!(s.tk_errors.len(), 1);
    assert_eq!(s.images, 3);
    assert_eq!(s.videos, 1);
}

#[test]
fn ungenerated_scope_is_visible_and_tinted() {
    let out = render_preview(
        "Before [TK]summarise it[/TK] after.",
        Kind::Fragment,
        &NoResolver,
        &RenderOpts::default(),
    );
    assert!(
        out.html
            .contains("<span class=\"blyg-tk-gen\">⚠ ungenerated — summarise it</span>")
    );
}

#[test]
fn fragments_never_resolve_quotes() {
    let store = FakeStore::load();
    let out = render_preview(&format!("![[{A}]]"), Kind::Fragment, &store, &opts());
    assert!(
        out.html
            .contains(&format!("<p data-line=\"0\">![[{A}]]</p>"))
    );
    assert_eq!(out.stats.quotes, 0);
}

#[test]
fn quotes_inside_tk_are_sources_not_quotes() {
    let store = FakeStore::load();
    let md = format!("[TK]summarise ![[{A}]][=]A summary.[/TK]");
    let out = render_preview(&md, Kind::Thread, &store, &opts());
    assert_eq!(out.stats.quotes, 0);
    assert!(!out.html.contains("blyg-transclusion"));
}

#[test]
fn self_quote_is_flagged() {
    let store = FakeStore::load();
    let md = format!("![[{}]]", store.self_id());
    let out = render_preview(&md, Kind::Thread, &store, &opts());
    assert_eq!(out.stats.unresolved[0].reason, UnresolvedReason::SelfQuote);
}

#[test]
fn crlf_is_normalised() {
    let out = render_preview(
        "a\r\n\r\nb",
        Kind::Fragment,
        &NoResolver,
        &RenderOpts::default(),
    );
    assert_eq!(
        out.html,
        "<p data-line=\"0\">a</p>\n<p data-line=\"2\">b</p>\n"
    );
}

#[test]
fn content_can_never_inject_markup() {
    let md = "<script>alert(1)</script>\n\n<img src=x onerror=alert(1)> [x](javascript:alert(1)) \
              ![y](javascript:alert(2)) <a href=\"javascript:x\">a</a> \"><svg onload=alert(3)>";
    let out = render_preview(md, Kind::Thread, &NoResolver, &RenderOpts::default());
    let h = &out.html;
    assert!(!h.contains("<script"), "{h}");
    assert!(!h.contains("<img src=\"x\""), "{h}");
    assert!(!h.contains("<svg"), "{h}");
    assert!(!h.contains("href=\"javascript:"), "{h}");
    assert!(!h.contains("src=\"javascript:"), "{h}");
    assert!(h.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
}

#[test]
fn shell_wraps_an_article() {
    let out = render_preview("Hello", Kind::Fragment, &NoResolver, &RenderOpts::default());
    let page = page_shell(
        ":root{--ink:#111}",
        &article_html(Kind::Fragment, &out.html, None, None),
    );
    assert!(page.starts_with("<!doctype html>"));
    assert!(page.contains(
        "<article class=\"fragment\">\n\n<div class=\"item-content\">\n<p data-line=\"0\">Hello</p>"
    ));
    assert!(page.contains("Content-Security-Policy"));
    assert!(page.contains("youtube-nocookie.com/embed/"));
    assert!(page.contains(".blyg-tk-gen {"));
}

/// Deterministic fuzzing over the grammar's sharp edges: nothing may panic,
/// and line attributes stay purely additive.
#[test]
fn fuzz_no_panics() {
    const PIECES: &[&str] = &[
        "[TK]",
        "[=]",
        "[/TK]",
        "![[",
        "01j9zq3k4m5n6p7q8r9s0t1v2w",
        "]]",
        "@v2",
        "*",
        "_",
        "~~",
        "`",
        "```\n",
        "\n",
        "\n\n",
        "  ",
        "\t",
        "é",
        "😀",
        "\u{E000}",
        "\u{E001}",
        "\u{E002}",
        "http://",
        "https://",
        "www.",
        ".com",
        "example",
        "@",
        "<",
        ">",
        "&",
        "&amp;",
        "(",
        ")",
        "[",
        "]",
        "$&",
        "$'",
        "- ",
        "1. ",
        "> ",
        "# ",
        "\\",
        "|",
        "---",
        "youtu.be/",
        "Qa1b2C3d4E5",
        "\r\n",
        ":",
        "//",
        "mailto:",
        "xn--",
        "a",
        " b ",
    ];
    let store = FakeStore::load();
    let with = opts();
    let without = RenderOpts {
        data_line: false,
        ..opts()
    };
    let mut seed: u64 = 0x5eed_1234_abcd_ef01;
    let mut next = || {
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (seed >> 33) as usize
    };
    for _ in 0..1500 {
        let n = 1 + next() % 40;
        let md: String = (0..n).map(|_| PIECES[next() % PIECES.len()]).collect();
        for kind in [Kind::Fragment, Kind::Thread] {
            let a = render_preview(&md, kind, &store, &with);
            let b = render_preview(&md, kind, &store, &without);
            assert_eq!(strip_lines(&a.html), b.html, "{md:?}");
            // No TK marker ever ships (the sentinels are U+E000–E002).
            assert!(
                !a.html.contains(['\u{E000}', '\u{E001}', '\u{E002}']),
                "marker leaked: {md:?}"
            );
        }
    }
}

/// Decision #60: an own-line directive left in a block scope's output is a
/// quote, as publish makes it; the prose around it stays disclosed as
/// generated, and the quote sits between the generated blocks.
#[test]
fn directive_in_block_output_is_a_quote() {
    let store = FakeStore::load();
    let md = format!("Intro.\n\n[TK]write a bridge[=]bridge line\n\n![[{A}]]\n\nafter[/TK]\n\nEnd");
    let out = render_preview(&md, Kind::Thread, &store, &opts());
    let h = strip_lines(&out.html);
    let quote = format!(
        "<blockquote class=\"blyg-transclusion\" data-blyg-id=\"{A}\" data-blyg-version=\"3\">"
    );
    let gen1 = h.find("<div class=\"blyg-tk-gen\"><p>bridge line</p>\n</div>");
    let q = h.find(&quote);
    let gen2 = h.find("<div class=\"blyg-tk-gen\"><p>after</p>\n</div>");
    assert!(gen1.is_some() && gen1 < q && q < gen2, "{h}");
    assert!(!h.contains("![["), "{h}");
    assert_eq!(out.stats.transclusions.len(), 1);
    assert_eq!(out.stats.ai_spans, 1);
    // Every top-level block keeps its source line.
    for at in [
        "<div class=\"blyg-tk-gen\" data-line=\"2\">",
        "data-blyg-version=\"3\" data-line=\"4\">",
        "<div class=\"blyg-tk-gen\" data-line=\"6\">",
    ] {
        assert!(out.html.contains(at), "{at}: {}", out.html);
    }

    // An output that is only the directive leaves no empty generated block.
    let md = format!("Text.\n\n[TK]summarise ![[{A}]][=]![[{A}]][/TK]");
    let h = render_preview(&md, Kind::Thread, &store, &opts()).html;
    assert!(!h.contains("blyg-tk-gen"), "{h}");
    assert!(h.contains("data-blyg-version=\"3\""), "{h}");

    // An attached `>` run is a partial quote, as anywhere else in a thread.
    let md = format!("[TK]bridge[=]One.\n![[{A}]]\n> Small tools\n\nTwo.[/TK]");
    let out = render_preview(&md, Kind::Thread, &store, &opts());
    assert!(out.html.contains("blyg-partial"), "{}", out.html);
    assert!(out.stats.transclusions[0].selector.is_some());

    // In code it stays text, and a fragment never quotes.
    let md = format!("[TK]x[=]Look:\n\n```\n![[{A}]]\n```[/TK]");
    let out = render_preview(&md, Kind::Thread, &store, &opts());
    assert!(out.stats.transclusions.is_empty());
    let md = format!("[TK]x[=]bridge\n\n![[{A}]][/TK]");
    let out = render_preview(&md, Kind::Fragment, &store, &opts());
    assert!(
        out.html.contains(&format!("<p>![[{A}]]</p>")),
        "{}",
        out.html
    );
}

#[test]
fn unrequested_output_directives_are_named() {
    use blyg_render::unrequested_output_directives as unrequested;
    // Echoed by the output, not named by the instruction.
    assert_eq!(
        unrequested(&format!(
            "Intro.\n\n[TK]write a bridge[=]line\n![[{A}]]\nafter[/TK]"
        )),
        vec![A.to_string()]
    );
    // Named by the instruction: a source, no warning.
    assert!(unrequested(&format!("[TK]open with ![[{A}]][=]x\n![[{A}]]\ny[/TK]")).is_empty());
    // Inside a sentence, in code, ungenerated or impyrt: never a quote.
    assert!(unrequested(&format!("[TK]x[=]see ![[{A}]] here[/TK]")).is_empty());
    assert!(unrequested(&format!("[TK]x[=]```\n![[{A}]]\n```[/TK]")).is_empty());
    assert!(unrequested(&format!("[TK]x ![[{U}]][/TK]")).is_empty());
    assert!(unrequested(&format!("[TK]impyrt=a\n![[{A}]]\nb[/TK]")).is_empty());
    // Each id once, in order.
    assert_eq!(
        unrequested(&format!("[TK]x[=]![[{U}]]\n![[{A}]]\n![[{U}]][/TK]")),
        vec![U.to_string(), A.to_string()]
    );
}
