//! Mentions in natural grammar (studio 0.66, #158; contacts.ts
//! `consumeHandles` / `handleCandidates`): `@lin's notes` and an
//! `@lin-driven agenda` both name Lin. A handle naming no contact falls back
//! to its longest prefix ending before a `-` or `_` that does; an exact or
//! longer handle still wins. Only `@` and the handle become the link; the
//! rest of the word stays the author's. The vectors are the studio's own
//! (`test/contacts.test.ts`), with invented names.

use blyg_render::{HandleCard, Kind, RenderOpts, Resolution, Resolver, render_preview};

/// A book with `lin` and, where asked, the hyphenated `lin-k`, on a studio
/// that reads natural grammar (0.66 on) unless `old` says it's 0.56-0.65.
struct Book {
    with_lin_k: bool,
    old: bool,
}

impl Resolver for Book {
    fn resolve(&self, _id: &str) -> Resolution {
        Resolution::NotFound
    }

    fn natural_handles(&self) -> bool {
        !self.old
    }

    fn contact(&self, handle: &str) -> Option<HandleCard> {
        let h = handle.to_ascii_lowercase();
        if h == "lin" {
            return Some(HandleCard {
                name: "Lin Okafor".into(),
                url: "https://lin.example.org/".into(),
                ids: vec![],
            });
        }
        (self.with_lin_k && h == "lin-k").then(|| HandleCard {
            name: "Lin K".into(),
            url: "https://k.example.org/".into(),
            ids: vec![],
        })
    }
}

const LIN: &str = "<span class=\"h-card\"><a class=\"u-url\" href=\"https://lin.example.org/\">@<span class=\"p-name\">Lin Okafor</span></a></span>";
const LIN_K: &str = "<span class=\"h-card\"><a class=\"u-url\" href=\"https://k.example.org/\">@<span class=\"p-name\">Lin K</span></a></span>";

fn html(md: &str, with_lin_k: bool) -> String {
    let opts = RenderOpts {
        data_line: false,
        ..Default::default()
    };
    let book = Book {
        with_lin_k,
        old: false,
    };
    render_preview(md, Kind::Fragment, &book, &opts).html
}

/// On studio 0.56-0.65 publish links only a whole-token handle, so the
/// preview does too: `@lin-driven` stays text there, `@lin's` still links.
#[test]
fn an_older_studio_keeps_the_whole_token_reading() {
    let opts = RenderOpts {
        data_line: false,
        ..Default::default()
    };
    let book = Book {
        with_lin_k: true,
        old: true,
    };
    let html = |md: &str| render_preview(md, Kind::Fragment, &book, &opts).html;
    assert_eq!(
        html("a @lin-driven agenda, @lin_ish"),
        "<p>a @lin-driven agenda, @lin_ish</p>\n"
    );
    assert_eq!(html("In @lin's case"), format!("<p>In {LIN}'s case</p>\n"));
    assert_eq!(html("@lin-k"), format!("<p>{LIN_K}</p>\n"));
    assert_eq!(html("@lin-k-led"), "<p>@lin-k-led</p>\n");
}

#[test]
fn a_possessive_names_the_contact() {
    assert_eq!(html("In @lin's case", false), format!("<p>In {LIN}'s case</p>\n"));
    assert_eq!(
        html("In @lin\u{2019}s case", false),
        format!("<p>In {LIN}\u{2019}s case</p>\n")
    );
}

#[test]
fn a_hyphenated_or_underscored_word_falls_back_to_the_handle() {
    assert_eq!(
        html("a @lin-driven agenda", false),
        format!("<p>a {LIN}-driven agenda</p>\n")
    );
    assert_eq!(
        html("@lin_ish, @lin-", false),
        format!("<p>{LIN}_ish, {LIN}-</p>\n")
    );
    assert_eq!(
        html("a @lin-driven-and-led plan", false),
        format!("<p>a {LIN}-driven-and-led plan</p>\n")
    );
    // Any case, as the handle itself.
    assert_eq!(
        html("the @LIN-led one", false),
        format!("<p>the {LIN}-led one</p>\n")
    );
}

#[test]
fn the_longest_handle_in_the_book_wins() {
    assert_eq!(
        html("@lin-k-led and @lin-led", true),
        format!("<p>{LIN_K}-led and {LIN}-led</p>\n")
    );
    assert_eq!(html("@lin-k", true), format!("<p>{LIN_K}</p>\n"));
}

#[test]
fn a_word_whose_prefixes_name_nobody_stays_text() {
    assert_eq!(
        html("@nobody-driven and @sam's", false),
        "<p>@nobody-driven and @sam's</p>\n"
    );
}
