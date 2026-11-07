//! --- contacts --- `@handle` → the contact's h-card, as the studio renders
//! it at publish and in its preview (studio `contacts.ts`: `handleSpots`,
//! `mentionLink`, `mentionCard`, `consumeHandles`; contract 5.7, studio
//! 0.56).
//!
//! A handle naming a contact becomes `[@Name](url)` in the Markdown, and
//! that link becomes the h-card after rendering, through the same token
//! splice as `[[id]]` links (`withMentionCards`). A handle naming nobody
//! stays literal prose. Handles inside code, quoted lines (`> …`), bracket
//! directives, Markdown links and autolinks are left alone.

use std::sync::OnceLock;

use regex::Regex;

use crate::util::escape_html;

/// What the h-card for one contact needs (the [`crate::Resolver`] answers
/// it per handle).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandleCard {
    /// The display name the page shows after `@`.
    pub name: String,
    /// Where the person can be found.
    pub url: String,
    /// The identifiers their own page vouches for (`self`, `bound`); never
    /// the `entered` ones.
    pub ids: Vec<String>,
}

/// The Markdown with each known handle replaced by its link, and each
/// link with the h-card that replaces it after rendering.
pub(crate) struct Consumed {
    pub text: String,
    pub cards: Vec<(String, String)>,
}

fn opaque_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"!?\[\[[^\]\n]*\]\]|!?\[[^\]\n]*\]\([^)\n]*\)|<[^>\n]*>").expect("opaque re")
    })
}

fn quoted_line_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?m)^[ \t]{0,3}>.*$").expect("quoted re"))
}

fn href_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"href="([^"]*)""#).expect("href re"))
}

/// JavaScript's `\w` (no `u` flag).
fn is_w(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}

/// `AT_HANDLE`'s match end for an `@` at `at`, or `None`:
/// `(?<![\w@./:+-])@([A-Za-z0-9][A-Za-z0-9_-]{0,38})(?![\w@]|[.-]\w)`,
/// backtracking as the regex would.
fn handle_end(b: &[u8], at: usize) -> Option<usize> {
    if at > 0 && (is_w(b[at - 1]) || b"@./:+-".contains(&b[at - 1])) {
        return None;
    }
    let first = at + 1;
    if !b.get(first).is_some_and(u8::is_ascii_alphanumeric) {
        return None;
    }
    let mut end = first + 1;
    while end < b.len() && end - first < 39 && (is_w(b[end]) || b[end] == b'-') {
        end += 1;
    }
    let after_ok = |e: usize| match b.get(e) {
        None => true,
        Some(&c) if is_w(c) || c == b'@' => false,
        Some(b'.' | b'-') => !b.get(e + 1).is_some_and(|&n| is_w(n)),
        Some(_) => true,
    };
    (first + 1..=end).rev().find(|&e| after_ok(e))
}

/// `handleSpots`: `(start, end, handle)` of each live `@handle`.
fn spots(md: &str) -> Vec<(usize, usize, &str)> {
    if !md.contains('@') {
        return Vec::new();
    }
    let mut skip = crate::markdown::code_ranges(md);
    for re in [opaque_re(), quoted_line_re()] {
        skip.extend(re.find_iter(md).map(|m| (m.start(), m.end())));
    }
    let b = md.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'@'
            && let Some(end) = handle_end(b, i)
        {
            if !crate::markdown::in_ranges(&skip, i) {
                out.push((i, end, &md[i + 1..end]));
            }
            i = end;
            continue;
        }
        i += 1;
    }
    out
}

/// `mentionLink`: the link `content_md` carries for a contact.
fn mention_link(card: &HandleCard) -> String {
    let mut name = String::with_capacity(card.name.len());
    for c in card.name.chars() {
        if matches!(c, '\\' | '[' | ']' | '*' | '_' | '`' | '<') {
            name.push('\\');
        }
        name.push(c);
    }
    let mut url = String::with_capacity(card.url.len());
    for c in card.url.chars() {
        match c {
            '(' => url.push_str("%28"),
            ')' => url.push_str("%29"),
            c if crate::util::is_js_ws(c) => url.push_str("%20"),
            c => url.push(c),
        }
    }
    format!("[@{name}]({url})")
}

/// `mentionCard`: the markup `content_html` carries.
fn mention_card(card: &HandleCard) -> String {
    let rendered = crate::markdown::render(&mention_link(card), None).0;
    let href = href_re()
        .captures(&rendered)
        .map(|c| c[1].to_string())
        .unwrap_or_else(|| escape_html(&card.url));
    let data: String = card
        .ids
        .iter()
        .map(|id| {
            format!(
                "<data class=\"u-url\" value=\"{}\"></data>",
                escape_html(id)
            )
        })
        .collect();
    format!(
        "<span class=\"h-card\"><a class=\"u-url\" href=\"{href}\">@<span class=\"p-name\">{}</span></a>{data}</span>",
        escape_html(&card.name)
    )
}

/// `consumeHandles`, with `contact` answering each handle (any case).
pub(crate) fn consume(md: &str, contact: &dyn Fn(&str) -> Option<HandleCard>) -> Consumed {
    let found = spots(md);
    if found.is_empty() {
        return Consumed {
            text: md.to_string(),
            cards: Vec::new(),
        };
    }
    let mut known: std::collections::HashMap<String, Option<HandleCard>> = Default::default();
    let mut cards: Vec<(String, String)> = Vec::new();
    let mut text = String::with_capacity(md.len() + 64);
    let mut last = 0;
    for (start, end, handle) in found {
        let card = known
            .entry(handle.to_ascii_lowercase())
            .or_insert_with(|| contact(handle))
            .clone();
        let Some(card) = card else { continue };
        let link = mention_link(&card);
        if !cards.iter().any(|(l, _)| *l == link) {
            cards.push((link.clone(), mention_card(&card)));
        }
        text.push_str(&md[last..start]);
        text.push_str(&link);
        last = end;
    }
    text.push_str(&md[last..]);
    Consumed { text, cards }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lin(h: &str) -> Option<HandleCard> {
        h.eq_ignore_ascii_case("lin").then(|| HandleCard {
            name: "Lin Okafor".into(),
            url: "https://lin.example.org/".into(),
            ids: vec!["https://social.example.net/@lin".into()],
        })
    }

    fn handles(md: &str) -> Vec<&str> {
        spots(md).into_iter().map(|(_, _, h)| h).collect()
    }

    #[test]
    fn handles_follow_the_studios_boundaries() {
        assert_eq!(handles("Tea with @lin."), ["lin"]);
        assert_eq!(handles("@lin, @sam-k and @a_b"), ["lin", "sam-k", "a_b"]);
        // Emails, paths, domains and doubled @ are not handles.
        assert!(handles("mail lin@example.org").is_empty());
        assert!(handles("see /@lin and x.@lin").is_empty());
        assert!(handles("@lin.example and @a@b.example").is_empty());
        // Inside code, quotes, links and directives: left alone.
        assert!(handles("`@lin` and\n\n    @lin indented").is_empty());
        assert!(handles("> @lin said").is_empty());
        assert!(handles("[@lin](https://x.example) <@lin> [[@lin]]").is_empty());
        // A trailing hyphen before a word backs off, as the regex does.
        assert_eq!(handles("@lin-x"), ["lin-x"]);
        assert_eq!(handles("@lin- x"), ["lin-"]);
    }

    #[test]
    fn known_handles_become_links_with_cards() {
        let c = consume("Tea with @Lin and @sam, again @lin", &lin);
        assert_eq!(
            c.text,
            "Tea with [@Lin Okafor](https://lin.example.org/) and @sam, again [@Lin Okafor](https://lin.example.org/)"
        );
        assert_eq!(c.cards.len(), 1);
        assert_eq!(
            c.cards[0].1,
            "<span class=\"h-card\"><a class=\"u-url\" href=\"https://lin.example.org/\">@<span class=\"p-name\">Lin Okafor</span></a><data class=\"u-url\" value=\"https://social.example.net/@lin\"></data></span>"
        );
    }

    #[test]
    fn names_and_urls_are_escaped_for_markdown() {
        let card = HandleCard {
            name: "A_[b]*".into(),
            url: "https://x.example/a (b)".into(),
            ids: vec![],
        };
        assert_eq!(
            mention_link(&card),
            "[@A\\_\\[b\\]\\*](https://x.example/a%20%28b%29)"
        );
    }
}
