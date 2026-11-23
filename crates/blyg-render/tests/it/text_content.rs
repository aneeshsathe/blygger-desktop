//! Text content (spec §2, blygger-spec decision #118, 0.4 r23; studio 0.71)
//! and the list structure of a partial bake (decision #114, §16.15; studio
//! 0.69). The cases are the studio's own (`test/selection.test.ts`,
//! `test/partial-list-bake.test.ts`) and the spec's reader cases 104–112.
//!
//! §2: a line break at the start tag and the end tag of every block element
//! in its list, and at every `br` and `hr`. So a tight nested list's parent
//! item and its first nested item are two lines, and table cells are lines.

use blyg_render::{
    Found, ItemKind, Kind, RenderOpts, Resolution, Resolver, excerpt_from_html, locate_selection,
    plain_text_from_html, render_preview, selection_from_quote, selection_text,
};

const SRC: &str = "01j9zq3k4m5n6p7q8r9s0t1v8c";

/// One target, whatever the id; its HTML is rendered from Markdown with
/// the preview's own renderer, as a published fragment's would be.
struct One(String);

impl Resolver for One {
    fn resolve(&self, id: &str) -> Resolution {
        Resolution::Found(Found {
            origin: None,
            id: id.into(),
            version: 1,
            kind: ItemKind::Fragment,
            content_html: self.0.clone(),
            author: None,
            page: None,
        })
    }
}

fn md_html(md: &str) -> String {
    blyg_render::render_markdown(md)
}

/// A thread holding only `![[SRC]]` with `quote` attached, its HTML.
fn bake(target_html: &str, quote: &str) -> String {
    let opts = RenderOpts {
        data_line: false,
        provenance: false,
        ..Default::default()
    };
    let md = format!("![[{SRC}]]\n{}", quote_md(quote));
    render_preview(&md, Kind::Thread, &One(target_html.into()), &opts).html
}

/// A selection as an attached quote: `>` on each line, a bare `>` between.
fn quote_md(selection: &str) -> String {
    selection
        .split('\n')
        .map(|l| format!("> {l}"))
        .collect::<Vec<_>>()
        .join("\n>\n")
}

/// The body inside the partial quote's blockquote.
fn body(html: &str) -> &str {
    let open = "<blockquote class=\"blyg-transclusion blyg-partial\"";
    let at = html
        .find(open)
        .unwrap_or_else(|| panic!("no partial quote: {html}"));
    let rest = &html[at..];
    let start = rest.find(">\n").unwrap() + 2;
    let end = rest.rfind("\n</blockquote>").unwrap();
    &rest[start..end]
}

// ---------------------------------------------------------------- §2 lines

#[test]
fn a_tight_nested_list_is_two_lines() {
    let html = md_html("- tools\n  - a pencil\n- patience");
    assert_eq!(selection_text(&html), "tools\na pencil\npatience");
}

#[test]
fn start_tags_are_boundaries_too() {
    assert_eq!(selection_text("a<div>b</div>c"), "a\nb\nc");
    assert_eq!(selection_text("a<p class=\"x\">b"), "a\nb");
    assert_eq!(selection_text("<P>a</P><DIV>b</DIV>"), "a\nb");
    assert_eq!(selection_text("x<blockquote>y</blockquote>"), "x\ny");
}

#[test]
fn table_parts_and_cells_are_lines() {
    assert_eq!(
        selection_text("<table><tr><td>a</td><td>b</td></tr><tr><th>c</th></tr></table>"),
        "a\nb\nc"
    );
    let html = md_html("| name | use |\n|---|---|\n| awl | punching holes |");
    assert_eq!(selection_text(&html), "name\nuse\nawl\npunching holes");
}

#[test]
fn every_listed_element_is_a_block() {
    for tag in [
        "p",
        "h1",
        "h6",
        "li",
        "ul",
        "ol",
        "dl",
        "dt",
        "dd",
        "blockquote",
        "pre",
        "div",
        "table",
        "thead",
        "tbody",
        "tfoot",
        "tr",
        "td",
        "th",
        "figure",
        "figcaption",
        "section",
        "article",
        "aside",
        "header",
        "footer",
        "details",
        "summary",
    ] {
        assert_eq!(
            selection_text(&format!("a<{tag}>b</{tag}>c")),
            "a\nb\nc",
            "<{tag}>"
        );
    }
    for void in [
        "<br>",
        "<br/>",
        "<br />",
        "<hr>",
        "<hr/>",
        "<HR class=\"x\">",
    ] {
        assert_eq!(selection_text(&format!("a{void}b")), "a\nb", "{void}");
    }
}

#[test]
fn other_elements_are_not_blocks() {
    for (html, text) in [
        ("a<span>b</span>c", "abc"),
        ("a<em>b</em>c", "abc"),
        ("a<prefix>b</prefix>c", "abc"),
        ("a<bra>b", "ab"),
        ("a<hrx>b", "ab"),
        ("a<main>b</main>c", "abc"),
        ("a<nav>b</nav>c", "abc"),
    ] {
        assert_eq!(selection_text(html), text, "{html}");
    }
}

#[test]
fn a_definition_list_details_and_figure_are_lines() {
    assert_eq!(
        selection_text(
            "<dl><dt>bodkin</dt><dd>a blunt needle</dd></dl><hr><details><summary>More</summary>Hidden</details>\
             <figure>A drawing<figcaption>Fig. 1</figcaption></figure>"
        ),
        "bodkin\na blunt needle\nMore\nHidden\nA drawing\nFig. 1"
    );
}

/// The studio's `selection.test.ts`: the rule, not any one call site.
#[test]
fn the_studios_selection_rules_hold() {
    assert_eq!(
        selection_text("<p>first</p>\n<p>second</p>"),
        "first\nsecond"
    );
    assert_eq!(selection_text("<p>one\ntwo\nthree</p>"), "one two three");
    assert_eq!(selection_text("<p>a   b\t\tc</p>"), "a b c");
    assert_eq!(selection_text("<p>a</p><p></p><p>  </p><p>b</p>"), "a\nb");
    assert_eq!(selection_text("<p>a<br>b</p>"), "a\nb");
    assert_eq!(
        selection_text("<ul><li>one</li><li>two</li></ul>"),
        "one\ntwo"
    );
    assert!(!selection_text("<p>evil.</p><p>Next</p>").contains("evil.Next"));
    assert_eq!(
        selection_text("<p>the <em>thin</em> layer</p>"),
        "the thin layer"
    );
    assert_eq!(
        selection_text("<p>a &amp; b &lt;c&gt; &quot;d&quot;</p>"),
        "a & b <c> \"d\""
    );
    assert_eq!(
        selection_text("<blockquote><p>inner</p></blockquote><p>after</p>"),
        "inner\nafter"
    );
    assert_eq!(
        selection_text("<p>a</p><script>var x = 1;</script><p>b</p>"),
        "a\nb"
    );
    assert_eq!(selection_text("<p></p>"), "");
    assert_eq!(selection_text(""), "");
}

/// `plainTextFromHtml` shares the list: a boundary is a space there, so
/// cells and a block opened mid-text never weld.
#[test]
fn plain_text_breaks_at_the_same_boundaries() {
    assert_eq!(
        plain_text_from_html("<table><tr><td>left</td><td>right</td></tr></table>"),
        "left right"
    );
    assert_eq!(plain_text_from_html("a<div>b</div>c"), "a b c");
    assert_eq!(
        plain_text_from_html("<dl><dt>term</dt><dd>meaning</dd></dl>"),
        "term meaning"
    );
    assert_eq!(
        excerpt_from_html("<section>one</section><aside>two</aside>", 60),
        "one two"
    );
}

// ------------------------------------------------- partial quotes and §2

/// Reader case 111: a tight parent item and its nested item, two lines.
#[test]
fn a_partial_quote_of_a_tight_parent_and_child_keeps_the_nesting() {
    let target =
        "<ul>\n<li>tools\n<ul>\n<li>a pencil</li>\n</ul>\n</li>\n<li>patience</li>\n</ul>\n";
    let html = bake(target, "tools\na pencil");
    assert_eq!(
        body(&html),
        "<ul>\n<li><p>tools</p>\n<ul>\n<li>a pencil</li>\n</ul></li>\n</ul>"
    );
}

/// Reader case 112: as one line it is not in the text content.
#[test]
fn a_tight_parent_and_child_as_one_line_is_not_found() {
    let target =
        "<ul>\n<li>tools\n<ul>\n<li>a pencil</li>\n</ul>\n</li>\n<li>patience</li>\n</ul>\n";
    assert_eq!(locate_selection(target, "tools a pencil"), None);
    let html = bake(target, "tools a pencil");
    assert!(html.contains("blyg-transclusion unresolved"), "{html}");
}

#[test]
fn table_cells_quote_as_lines_not_as_a_row() {
    let target = md_html("| name | use |\n|---|---|\n| awl | punching holes |");
    assert!(locate_selection(&target, "awl\npunching holes").is_some());
    assert_eq!(locate_selection(&target, "awl punching holes"), None);
}

// ------------------------------------------------ #114: list structure

const TIGHT: &str =
    "Intro line.\n\n- first bullet\n- second bullet\n  - nested bullet\n- third bullet\n\nAfter.";
const LOOSE: &str = "- first, loose\n\n  with a second paragraph\n\n- second, loose\n\n  - nested, loose\n\n- third";
const ORDERED: &str = "Steps:\n\n1. one\n2. two\n3. three\n4. four";
const STARTED: &str = "7. seventh\n8. eighth\n9. ninth";
const MIXED: &str = "A paragraph.\n\n- a bullet\n- another\n\nA closing paragraph with <b>tags</b> & an ampersand.\n\n> a plain quote\n\n```\ncode line\n```";

#[test]
fn quoted_bullets_bake_as_a_list_and_paragraphs_as_paragraphs() {
    let html = bake(
        &md_html(TIGHT),
        "Intro line.\nfirst bullet\nsecond bullet\nnested bullet",
    );
    assert_eq!(
        body(&html),
        "<p>Intro line.</p>\n<ul>\n<li>first bullet</li>\n<li><p>second bullet</p>\n<ul>\n<li>nested bullet</li>\n</ul></li>\n</ul>"
    );
}

/// Reader case 104 and the studio's ordered-run test.
#[test]
fn a_quoted_ordered_run_is_numbered_as_the_source_numbers_it() {
    let target = md_html(ORDERED);
    assert_eq!(
        body(&bake(&target, "three\nfour")),
        "<ol start=\"3\">\n<li>three</li>\n<li>four</li>\n</ol>"
    );
    assert_eq!(
        body(&bake(&target, "one\ntwo")),
        "<ol>\n<li>one</li>\n<li>two</li>\n</ol>"
    );
    assert_eq!(
        body(&bake(&md_html(STARTED), "eighth\nninth")),
        "<ol start=\"8\">\n<li>eighth</li>\n<li>ninth</li>\n</ol>"
    );
}

/// Reader case 106: a selection inside one item is a line in that item.
#[test]
fn a_selection_inside_one_item_stays_in_its_item() {
    assert_eq!(
        body(&bake(&md_html(ORDERED), "hre")),
        "<ol start=\"3\">\n<li>hre</li>\n</ol>"
    );
}

/// Reader case 107 and the studio's loose-list tests.
#[test]
fn nesting_is_kept_when_both_lines_are_quoted_and_no_outer_item_is_invented() {
    let target = md_html(LOOSE);
    assert_eq!(
        body(&bake(&target, "second, loose\nnested, loose")),
        "<ul>\n<li><p>second, loose</p>\n<ul>\n<li>nested, loose</li>\n</ul></li>\n</ul>"
    );
    assert_eq!(
        body(&bake(&target, "nested, loose")),
        "<ul>\n<li>nested, loose</li>\n</ul>"
    );
    assert_eq!(
        body(&bake(&target, "first, loose\nwith a second paragraph")),
        "<ul>\n<li><p>first, loose</p>\n<p>with a second paragraph</p></li>\n</ul>"
    );
}

/// Reader case 109: the structure comes from the target, never the quoter.
#[test]
fn the_structure_comes_from_the_target_not_the_quoters_markers() {
    // Quoted as plain lines over a list: still a list.
    let html = render_preview(
        &format!("![[{SRC}]]\n> first bullet\n>\n> second bullet\n>\n> nested bullet"),
        Kind::Thread,
        &One(md_html(TIGHT)),
        &RenderOpts {
            data_line: false,
            provenance: false,
            ..Default::default()
        },
    )
    .html;
    assert!(body(&html).contains("<ul>"), "{html}");
    // Quoted with markers over prose: still paragraphs.
    let html = render_preview(
        &format!("![[{SRC}]]\n> - A paragraph."),
        Kind::Thread,
        &One(md_html(MIXED)),
        &RenderOpts {
            data_line: false,
            provenance: false,
            ..Default::default()
        },
    )
    .html;
    assert_eq!(body(&html), "<p>A paragraph.</p>");
}

#[test]
fn text_in_list_items_is_escaped_as_in_paragraphs() {
    assert_eq!(
        body(&bake(&md_html("- a <b> & c"), "a <b> & c")),
        "<ul>\n<li>a &lt;b&gt; &amp; c</li>\n</ul>"
    );
}

/// The studio's invariant, on every run of whole lines of every source and
/// on a cut inside each: the bake's text content is the selector's `exact`.
#[test]
fn a_bakes_text_is_its_selectors_exact_for_every_selection() {
    let mut checked = 0;
    let mut lists = 0;
    for md in [TIGHT, LOOSE, ORDERED, STARTED, MIXED] {
        let target = md_html(md);
        let hay = selection_text(&target);
        let lines: Vec<&str> = hay.split('\n').collect();
        for i in 0..lines.len() {
            for j in i..lines.len() {
                let run = lines[i..=j].join("\n");
                let mut picks = vec![run.clone()];
                let chars: Vec<char> = run.chars().collect();
                if chars.len() > 4 {
                    picks.push(chars[2..chars.len() - 2].iter().collect());
                }
                for pick in picks {
                    let exact = selection_from_quote(&quote_md(&pick));
                    if exact.is_empty() || !hay.contains(&exact) {
                        continue;
                    }
                    let html = bake(&target, &exact);
                    let b = body(&html);
                    assert_eq!(selection_text(b), exact, "{md:?} / {exact:?}: {b}");
                    lists += usize::from(b.contains("<li>"));
                    checked += 1;
                }
            }
        }
    }
    assert!(checked > 100, "{checked}");
    assert!(lists > 50, "{lists} of {checked} bakes held a list");
}
