//! Protocol 0.4, fifteenth revision: the nine readings of decision #102
//! (v0.4-plan §21) and the rendering half of #103, as the preview and the
//! publish check apply them.

mod common;

use blyg_render::{
    Found, ItemKind, Kind, RenderOpts, Resolution, Resolver, UnresolvedReason, absolutize_html,
    publish_grammar_errors, render_preview,
};
use common::FakeStore;

const A: &str = "01j9zq3k4m5n6p7q8r9s0t1v2w"; // local fragment, v3
const R: &str = "02abcdefghjkmnpqrstvwxyz01"; // remote fragment, v5, no page
const P: &str = "02abcdefghjkmnpqrstvwxyz02"; // remote, withdrawn, pin-retained v6
const N: &str = "02abcdefghjkmnpqrstvwxyz05"; // remote, withdrawn, nothing retained

fn opts() -> RenderOpts {
    RenderOpts {
        data_line: false,
        provenance: false,
        mount: "https://home.example/blyg".into(),
        self_id: Some(FakeStore::load().self_id()),
    }
}

fn render(md: &str, kind: Kind) -> blyg_render::Rendered {
    render_preview(md, kind, &FakeStore::load(), &opts())
}

fn reasons(r: &blyg_render::Rendered) -> Vec<UnresolvedReason> {
    r.stats
        .unresolved
        .iter()
        .map(|u| u.reason.clone())
        .collect()
}

/// (1) A directive line in a fragment is a publish error; the HTML keeps it
/// as literal text, as a reader shows an old one.
#[test]
fn directive_line_in_a_fragment_is_a_publish_error() {
    for md in [
        format!("Opening.\n\n![[{A}]]\n\nClosing."),
        format!("  ![[{A} | a label]]  "),
        format!("Para\n\t![[{A}]]"),
    ] {
        let out = render(&md, Kind::Fragment);
        assert_eq!(
            reasons(&out),
            vec![UnresolvedReason::DirectiveInFragment],
            "{md:?}"
        );
        assert!(out.html.contains("![["), "{md:?}: literal text");
        assert!(!out.html.contains("blyg-transclusion"), "{md:?}");
        let errs = publish_grammar_errors(&md, Kind::Fragment);
        assert_eq!(errs.len(), 1, "{md:?}");
        assert_eq!(errs[0].reason, UnresolvedReason::DirectiveInFragment);
        assert!(publish_grammar_errors(&md, Kind::Thread).is_empty());
    }
    // In code, inline, or not grammar at all: text, and it publishes.
    for md in [
        format!("```\n![[{A}]]\n```"),
        format!("\t![[{A}]]"),
        format!("Text\n\n    ![[{A}]]"),
        format!("see ![[{A}]] inline"),
        format!("`![[{A}]]`"),
        "![[not-an-id]]".to_string(),
    ] {
        assert!(
            render(&md, Kind::Fragment).stats.unresolved.is_empty(),
            "{md:?}"
        );
        assert!(
            publish_grammar_errors(&md, Kind::Fragment).is_empty(),
            "{md:?}"
        );
    }
    // A line that generated text leaves on its own is a directive too.
    let md = format!("[TK]x[=]one\n\n![[{A}]][/TK]");
    assert_eq!(
        publish_grammar_errors(&md, Kind::Fragment)[0].reason,
        UnresolvedReason::DirectiveInFragment
    );
    // Line numbers are the source's.
    let errs = publish_grammar_errors(&format!("a\r\n\r\n![[{A}]]"), Kind::Fragment);
    assert_eq!(errs[0].line, 2);
}

/// (2) The `@vN` reservation is the own-line form, with or without a label.
#[test]
fn reserved_version_is_the_own_line_form() {
    for md in [format!("![[{A}@v2]]"), format!("  ![[{A}@v2 | pinned]]")] {
        for kind in [Kind::Thread, Kind::Fragment] {
            let out = render(&md, kind);
            assert_eq!(
                reasons(&out),
                vec![UnresolvedReason::ReservedVersion],
                "{md:?} {kind:?}"
            );
            assert_eq!(
                publish_grammar_errors(&md, kind)[0].reason,
                UnresolvedReason::ReservedVersion
            );
        }
    }
    for md in [
        format!("Prose about ![[{A}@v2]] inline."),
        format!("Prose about ![[{A}@v2 | x]] inline."),
        format!("```\n![[{A}@v2]]\n```"),
    ] {
        for kind in [Kind::Thread, Kind::Fragment] {
            assert!(render(&md, kind).stats.unresolved.is_empty(), "{md:?}");
            assert!(publish_grammar_errors(&md, kind).is_empty(), "{md:?}");
        }
    }
    // The reserved line ends a partial quote's selection (§10.1).
    let md = format!("![[{A}]]\n> Small tools\n![[{A}@v2 | x]]");
    let out = render(&md, Kind::Thread);
    assert_eq!(out.stats.quotes, 1);
    assert_eq!(reasons(&out), vec![UnresolvedReason::ReservedVersion]);
}

/// (3) Code wins by CommonMark's block structure; outside code, the line
/// grammar comes first and surrounding whitespace of any amount is ignored.
#[test]
fn code_by_commonmark_block_structure() {
    let quotes = |md: &str| render(md, Kind::Thread).stats.quotes;
    // A tab or four spaces at the start of a document, or after a blank
    // line: an indented code block.
    assert_eq!(quotes(&format!("\t![[{A}]]")), 0);
    assert_eq!(quotes(&format!("Para.\n\n    ![[{A}]]")), 0);
    // Directly under a paragraph line CommonMark makes no code block.
    assert_eq!(quotes(&format!("Para.\n\t![[{A}]]")), 1);
    assert_eq!(quotes(&format!("Para.\n        ![[{A}]]")), 1);
    // Three spaces are never code.
    assert_eq!(quotes(&format!("   ![[{A}]]   ")), 1);
    // Fences, in a list item or a blockquote too.
    assert_eq!(quotes(&format!("~~~\n![[{A}]]\n~~~")), 0);
    assert_eq!(quotes(&format!("- item\n\n  ```\n  ![[{A}]]\n  ```")), 0);
    // Indented under a list item it is the item's paragraph, not code: the
    // line grammar reads it first.
    assert_eq!(quotes(&format!("- item\n\n    ![[{A}]]")), 1);
}

/// (4) A label is plain text in both forms, escaped where it is baked.
#[test]
fn labels_are_plain_text() {
    let md = format!("![[{A} | *my* <b>note</b> & \"it\"]]");
    let out = render(&md, Kind::Thread);
    assert!(
        out.html
            .contains(" data-blyg-label=\"*my* &lt;b&gt;note&lt;/b&gt; &amp; &quot;it&quot;\""),
        "{}",
        out.html
    );
    let md = format!("A [[{A} | *my* <b>note</b>]] link.");
    let out = render(&md, Kind::Fragment);
    assert!(
        out.html.contains(">*my* &lt;b&gt;note&lt;/b&gt;</a>"),
        "{}",
        out.html
    );
    // An unlabelled directive bakes no attribute.
    assert!(
        !render(&format!("![[{A}]]"), Kind::Thread)
            .html
            .contains("data-blyg-label")
    );
}

/// (5) A link to an item with no `page` names the item document.
#[test]
fn link_without_page_names_the_item_document() {
    let out = render(&format!("[[{R}]]"), Kind::Fragment);
    assert!(
        out.html
            .contains(&format!("href=\"https://blyg.example.com/items/{R}.json\"")),
        "{}",
        out.html
    );
    assert!(!out.html.contains("/f/"), "{}", out.html);
    // With a `page`, the page.
    let out = render("[[02abcdefghjkmnpqrstvwxyz06]]", Kind::Fragment);
    assert!(
        out.html
            .contains("href=\"https://cms.example.net/2026/10/templated-post/\"")
    );
}

/// (6) An author's relative URL resolves by RFC 3986 against the item's
/// identity origin; (7) a held source's against the source's origin.
#[test]
fn relative_urls_resolve_against_the_identity_origin() {
    let out = render(
        "[the archive](/archive/) and [notes](notes/x) and [up](../y) \
         ![a diagram](media/x7f2.png) [mail](mailto:a@example.com) [s](#sec)",
        Kind::Fragment,
    );
    for want in [
        "href=\"https://home.example/archive/\"",
        "href=\"https://home.example/blyg/notes/x\"",
        "href=\"https://home.example/y\"",
        "src=\"https://home.example/blyg/media/x7f2.png\"",
        "href=\"mailto:a@example.com\"",
        "href=\"https://home.example/blyg/#sec\"",
    ] {
        assert!(out.html.contains(want), "{want} in {}", out.html);
    }
    // A relative mount (no identity origin known) leaves URLs alone.
    let rel = render_preview(
        "[n](notes/x)",
        Kind::Fragment,
        &FakeStore::load(),
        &RenderOpts::default(),
    );
    assert!(rel.html.contains("href=\"notes/x\""));

    struct Held;
    impl Resolver for Held {
        fn resolve(&self, id: &str) -> Resolution {
            Resolution::Found(Found {
                origin: Some("https://remote.example/".into()),
                id: id.into(),
                version: 1,
                kind: ItemKind::Fragment,
                content_html: "<p>See <a href=\"notes/\">the notes</a> &amp; \
                               <img src=\"/media/a.png\" alt=\"\"></p>\n"
                    .into(),
                author: None,
                page: None,
            })
        }
    }
    let out = render_preview(&format!("![[{R}]]"), Kind::Thread, &Held, &opts());
    assert!(
        out.html
            .contains("<a href=\"https://remote.example/notes/\">the notes</a>"),
        "{}",
        out.html
    );
    assert!(
        out.html
            .contains("src=\"https://remote.example/media/a.png\"")
    );

    // Attribute values are unescaped to resolve and escaped again.
    assert_eq!(
        absolutize_html(
            "<a href=\"x?a=1&amp;b=2\" data-src=\"y\">t</a>",
            "https://h.example/b/"
        ),
        "<a href=\"https://h.example/b/x?a=1&amp;b=2\" data-src=\"y\">t</a>"
    );
}

/// (8) A pin-retained bake carries the retained version; with nothing
/// retained it is a publish error.
#[test]
fn pin_retained_bake_carries_the_retained_version() {
    let out = render(&format!("![[{P}]]"), Kind::Thread);
    assert_eq!(out.stats.transclusions[0].version, 6);
    assert!(out.html.contains("data-blyg-version=\"6\""));
    let out = render(&format!("![[{N}]]"), Kind::Thread);
    assert_eq!(reasons(&out), vec![UnresolvedReason::SourceWithdrawn]);
}

/// Whether any `blockquote.blyg-transclusion` opens inside an element with
/// the `blyg-tk-gen` class.
fn quote_inside_wrapper(html: &str) -> bool {
    let re = regex::Regex::new(r"<(/?)(\w+)([^>]*)>").unwrap();
    let mut stack: Vec<bool> = Vec::new();
    for c in re.captures_iter(html) {
        let (close, attrs) = (&c[1] == "/", &c[3]);
        if close {
            stack.pop();
            continue;
        }
        if &c[2] == "blockquote" && attrs.contains("blyg-transclusion") && stack.iter().any(|g| *g)
        {
            return true;
        }
        stack.push(attrs.contains("blyg-tk-gen"));
    }
    false
}

/// (9) A baked quote is never inside a `blyg-tk-gen` wrapper: the wrapper
/// closes before the quote and reopens after it.
#[test]
fn baked_quote_is_never_inside_the_wrapper() {
    for md in [
        format!("Start [TK]x[=]one\n![[{A}]]\ntwo[/TK] end"),
        format!("Intro.\n\n[TK]write it[=]Mine.\n\n![[{A}]]\n\nAfter the quote.[/TK]\n\nOutro."),
        format!("[TK]x[=]a\n![[{A}]]\nb\n![[{R}]]\nc[/TK]"),
    ] {
        let out = render(&md, Kind::Thread);
        assert!(!quote_inside_wrapper(&out.html), "{md:?}\n{}", out.html);
        assert!(out.stats.quotes >= 1, "{md:?}");
        // One scope, one generated[] entry, however many wrappers.
        assert_eq!(out.stats.ai_spans, 1, "{md:?}");
    }
    let out = render(
        &format!("[TK]x[=]a\n![[{A}]]\nb\n![[{R}]]\nc[/TK]"),
        Kind::Thread,
    );
    assert_eq!(out.html.matches("<div class=\"blyg-tk-gen\">").count(), 3);
    // Inline: the span closes and reopens around each quote.
    let out = render(
        &format!("Go [TK]x[=]a\n![[{A}]]\nb\n![[{R}]]\nc[/TK] on"),
        Kind::Thread,
    );
    assert!(!quote_inside_wrapper(&out.html), "{}", out.html);
    assert_eq!(out.html.matches("<span class=\"blyg-tk-gen\">").count(), 3);
    assert_eq!(out.html.matches("</span>").count(), 3);
}

/// #103: what another publisher wrote is isolated directionally in every
/// page Burrow shows.
#[test]
fn attributed_strings_and_quotes_are_bidi_isolated() {
    let css = blyg_render::studio_css();
    assert!(css.contains("blockquote.blyg-transclusion,"));
    assert!(css.contains("unicode-bidi: isolate"));
    let c = blyg_render::Citation {
        url: "https://blyg.example.com/f/abc/".into(),
        source: Some("Notes".into()),
        author: Some("\u{202E}evil".into()),
        excerpt: Some("words".into()),
        ..Default::default()
    };
    let html = blyg_render::stub_citation_html(&c, false);
    assert!(html.contains("<bdi>\u{202E}evil</bdi>"), "{html}");
    assert!(html.contains("&ldquo;<bdi>words</bdi>&rdquo;"), "{html}");
}
