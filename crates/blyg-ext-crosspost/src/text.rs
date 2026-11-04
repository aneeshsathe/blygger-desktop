//! The text cross-post puts in a site's composer (Substack Notes, Bluesky
//! or Farcaster): pure functions, no I/O. The opening paragraph comes from [`blyg_ext::recipe::excerpt`] and
//! the cut from [`blyg_ext::recipe::cut`]; this module adds plain text
//! (no Markdown marks, no `![[id]]` embeds), counting characters as a
//! reader sees them (an emoji with a skin tone, a flag or a family is one,
//! and is never split), and fitting the whole note, link included, into
//! the site's [`Limit`]: characters for Substack Notes and Bluesky, bytes of
//! UTF-8 for Farcaster.

use blyg_ext::recipe::{DEFAULT_TEMPLATE, Vars, check_template, cut, excerpt, expand};
use unicode_segmentation::UnicodeSegmentation;

/// `max-chars` when it isn't set: the whole note, link included.
pub const DEFAULT_MAX_CHARS: usize = 280;

/// `bluesky-max-chars` when it isn't set: Bluesky's limit for a post, 300
/// grapheme clusters, link included.
pub const DEFAULT_BLUESKY_MAX_CHARS: usize = 300;

/// `farcaster-max-bytes` when it isn't set: Farcaster's limit for a cast's
/// text, 320 bytes of UTF-8, link included (a long cast may hold 1024).
pub const DEFAULT_FARCASTER_MAX_BYTES: usize = 320;

/// What a site's limit counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unit {
    /// Characters as a reader sees them ([`visible_len`]).
    Chars,
    /// Bytes of UTF-8 (`str::len`), as Farcaster counts a cast.
    Bytes,
}

/// The longest note a site takes, and the setting that sets it (named in
/// the preview sheet's note).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limit {
    pub max: usize,
    pub unit: Unit,
    pub setting: &'static str,
}

impl Limit {
    /// A limit in characters as a reader counts them.
    pub const fn chars(max: usize, setting: &'static str) -> Limit {
        Limit {
            max,
            unit: Unit::Chars,
            setting,
        }
    }

    /// A limit in bytes of UTF-8.
    pub const fn bytes(max: usize, setting: &'static str) -> Limit {
        Limit {
            max,
            unit: Unit::Bytes,
            setting,
        }
    }

    /// How long `s` is, in this limit's unit.
    pub fn len(&self, s: &str) -> usize {
        match self.unit {
            Unit::Chars => visible_len(s),
            Unit::Bytes => s.len(),
        }
    }

    /// `s` cut to `max` units ([`cut_visible`] or [`cut_bytes`]).
    pub fn cut(&self, s: &str, max: usize) -> String {
        match self.unit {
            Unit::Chars => cut_visible(s, max),
            Unit::Bytes => cut_bytes(s, max),
        }
    }

    /// "280 characters (max-chars)", "320 bytes (farcaster-max-bytes)".
    fn describe(&self) -> String {
        let unit = match self.unit {
            Unit::Chars => "characters",
            Unit::Bytes => "bytes",
        };
        format!("{} {unit} ({})", self.max, self.setting)
    }

    /// "280-character limit (max-chars)".
    fn describe_limit(&self) -> String {
        let unit = match self.unit {
            Unit::Chars => "character",
            Unit::Bytes => "byte",
        };
        format!("{}-{unit} limit ({})", self.max, self.setting)
    }
}

/// The note to post, and a line for the preview sheet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prepared {
    pub text: String,
    pub note: Option<String>,
}

/// How many characters a reader sees in `s` (extended grapheme clusters).
pub fn visible_len(s: &str) -> usize {
    s.graphemes(true).count()
}

/// `s` cut to at most `max` visible characters, at the last space that
/// fits, with "…" (counted) when anything was cut: [`cut`]'s rule, applied
/// to grapheme clusters instead of `char`s, so an emoji is never split.
pub fn cut_visible(s: &str, max: usize) -> String {
    let gs: Vec<&str> = s.graphemes(true).collect();
    if gs.len() <= max {
        return s.to_string();
    }
    // One char per cluster, a space where the cluster is whitespace: `cut`
    // then picks the same place it would in `s`, counted in clusters.
    let mask: String = gs
        .iter()
        .map(|g| {
            if g.chars().all(char::is_whitespace) {
                ' '
            } else {
                'x'
            }
        })
        .collect();
    let masked = cut(&mask, max);
    if masked.is_empty() {
        return String::new();
    }
    let kept = masked.chars().count() - 1; // less the "…"
    let mut out = gs[..kept].concat();
    out.push('…');
    out
}

/// `s` cut to at most `max` bytes of UTF-8, "…" (3 bytes) included when
/// anything was cut, at the last space that fits: [`cut_visible`]'s rule,
/// with clusters weighed by their bytes, so a cut never splits a character
/// or an emoji.
pub fn cut_bytes(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    const ELLIPSIS: &str = "…";
    let Some(room) = max.checked_sub(ELLIPSIS.len()) else {
        return String::new();
    };
    let gs: Vec<&str> = s.graphemes(true).collect();
    let mut used = 0;
    let fit = gs
        .iter()
        .take_while(|g| {
            used += g.len();
            used <= room
        })
        .count();
    let head = &gs[..fit];
    let space = head
        .iter()
        .rposition(|g| g.chars().all(char::is_whitespace))
        .filter(|&i| i > 0);
    let kept = match space {
        Some(i) => head[..i].concat().trim_end().to_string(),
        None => head.concat(),
    };
    format!("{kept}{ELLIPSIS}")
}

/// A `[text](url)` at the start of `s`: the text and what follows it.
fn link(s: &str) -> Option<(&str, &str)> {
    let inner = s.strip_prefix('[')?;
    let close = inner.find("](")?;
    let text = &inner[..close];
    if text.contains(['[', ']']) {
        return None;
    }
    let after = &inner[close + 2..];
    let end = after.find(')')?;
    Some((text, &after[end + 1..]))
}

/// A paragraph of Markdown as plain text: `![[id]]` embeds and `![…](…)`
/// images dropped, `[text](url)` as its text, a labelled `[[id | text]]`
/// link as its label (studio 0.47), `**`, `__`, `~~` and backticks removed,
/// whitespace runs as one space.
pub fn plain(s: &str) -> String {
    let s = &blyg_ext::grammar::with_link_labels(s);
    let mut out = String::with_capacity(s.len());
    let mut rest = s.as_str();
    'scan: while let Some(c) = rest.chars().next() {
        if let Some(r) = rest.strip_prefix("![[")
            && let Some(end) = r.find("]]")
        {
            out.push(' ');
            rest = &r[end + 2..];
            continue;
        }
        if let Some(r) = rest.strip_prefix('!')
            && let Some((_, after)) = link(r)
        {
            out.push(' ');
            rest = after;
            continue;
        }
        if let Some((text, after)) = link(rest) {
            out.push_str(text);
            rest = after;
            continue;
        }
        for mark in ["**", "__", "~~", "`"] {
            if let Some(r) = rest.strip_prefix(mark) {
                rest = r;
                continue 'scan;
            }
        }
        out.push(c);
        rest = &rest[c.len_utf8()..];
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The post's opening paragraph as plain text, whole: the first one that
/// reads as prose ([`excerpt`]: not a heading, a `>` quote, an image or
/// embed, a code block, a rule or a table). Empty when there is none.
pub fn opening(markdown: &str) -> String {
    plain(&excerpt(markdown, usize::MAX))
}

/// Trimmed, no spaces at line ends, and never more than one blank line in
/// a row (what's left of the template when the excerpt is empty).
fn tidy(s: &str) -> String {
    let mut out: Vec<&str> = vec![];
    for line in s.lines().map(str::trim_end) {
        if line.is_empty() && out.last().is_none_or(|l: &&str| l.is_empty()) {
            continue;
        }
        out.push(line);
    }
    while out.last().is_some_and(|l| l.is_empty()) {
        out.pop();
    }
    out.join("\n").trim().to_string()
}

/// The note: `template` with `{{title}}`, `{{permalink}}` and the post's
/// opening paragraph as `{{excerpt}}`, cut so the whole note is at most
/// `max_chars` visible characters (the excerpt gives way; the link and
/// the rest of the template never do). `markdown` is `None` when the host
/// didn't send the post's text: then `fallback` (the host's own expanded
/// template) is used as it is. Substack Notes' rule; [`prepare_within`]
/// takes any [`Limit`].
pub fn prepare(
    markdown: Option<&str>,
    title: &str,
    permalink: &str,
    template: &str,
    max_chars: usize,
    fallback: &str,
) -> Prepared {
    prepare_within(
        markdown,
        title,
        permalink,
        template,
        Limit::chars(max_chars, "max-chars"),
        fallback,
    )
}

/// [`prepare`], with the whole note fitted into `limit` (characters or
/// bytes) instead.
pub fn prepare_within(
    markdown: Option<&str>,
    title: &str,
    permalink: &str,
    template: &str,
    limit: Limit,
    fallback: &str,
) -> Prepared {
    let Some(markdown) = markdown else {
        return Prepared {
            text: tidy(fallback),
            note: None,
        };
    };
    let body = opening(markdown);
    let vars = |excerpt: String| Vars {
        title: title.trim().to_string(),
        excerpt,
        permalink: permalink.trim().to_string(),
    };
    // Everything but the excerpt, which stands in as a marker that weighs
    // nothing (one cluster, one byte, taken off again).
    const MARK: &str = "\u{0}";
    let with_mark = tidy(&expand(template, &vars(MARK.into())));
    let marked = with_mark.contains(MARK);
    let rest = limit.len(&with_mark) - if marked { limit.len(MARK) } else { 0 };
    let budget = limit.max.saturating_sub(rest);
    let excerpt = limit.cut(&body, budget);
    let was_cut = excerpt != body;
    let text = tidy(&expand(template, &vars(excerpt.clone())));
    let within = limit.describe();
    let note = if body.is_empty() {
        Some("The post has no opening paragraph to quote".to_string())
    } else if limit.len(&text) > limit.max {
        Some(format!(
            "Over the {} even without the excerpt",
            limit.describe_limit()
        ))
    } else if excerpt.is_empty() {
        Some(format!("No room for the excerpt in {within}"))
    } else if was_cut {
        Some(format!("Cut to {within}"))
    } else {
        None
    };
    Prepared { text, note }
}

/// The `template` setting: `\n` in it is a line break. `Err` (with the
/// default to use) when it names an unknown placeholder.
pub fn template_setting(raw: Option<&str>) -> Result<String, (String, String)> {
    let Some(raw) = raw.map(str::trim).filter(|r| !r.is_empty()) else {
        return Ok(DEFAULT_TEMPLATE.to_string());
    };
    let t = raw.replace("\\n", "\n");
    match check_template(&t) {
        Ok(()) => Ok(t),
        Err(e) => Err((DEFAULT_TEMPLATE.to_string(), e)),
    }
}

/// The `max-chars` setting. `Err` (with the default to use) when it isn't
/// a whole number above 0.
pub fn max_chars_setting(raw: Option<&str>) -> Result<usize, (usize, String)> {
    limit_setting("max-chars", raw, DEFAULT_MAX_CHARS)
}

/// A limit setting (`max-chars`, `bluesky-max-chars`,
/// `farcaster-max-bytes`): `default` when it isn't set, and `Err` (with
/// `default` to use) when it isn't a whole number above 0.
pub fn limit_setting(
    key: &str,
    raw: Option<&str>,
    default: usize,
) -> Result<usize, (usize, String)> {
    let Some(raw) = raw.map(str::trim).filter(|r| !r.is_empty()) else {
        return Ok(default);
    };
    match raw.parse::<usize>() {
        Ok(n) if n > 0 => Ok(n),
        _ => Err((
            default,
            format!("{key} {raw:?} isn't a whole number above 0"),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LINK: &str = "https://blyg.example.com/p/tide";

    fn note(md: &str, max: usize) -> Prepared {
        prepare(Some(md), "Tides", LINK, DEFAULT_TEMPLATE, max, "")
    }

    #[test]
    fn the_opening_paragraph_and_the_link() {
        let p = note(
            "The moon pulls the sea\ntwice a day.\n\nA second paragraph.\n",
            280,
        );
        assert_eq!(
            p.text,
            format!("The moon pulls the sea twice a day.\n\n{LINK}")
        );
        assert_eq!(p.note, None);
    }

    #[test]
    fn crlf_is_fine() {
        let p = note(
            "\r\n# Tides\r\n\r\nThe moon\r\npulls.\r\n\r\nMore.\r\n",
            280,
        );
        assert_eq!(p.text, format!("The moon pulls.\n\n{LINK}"));
        assert!(!p.text.contains('\r'));
    }

    #[test]
    fn headings_quotes_embeds_and_code_are_skipped() {
        let md = "# Title\n\n## Sub\n\n> A quote\n> goes on\n\n![[img01]]\n\n\
                  ![a gull](https://blyg.example.com/m/gull.png)\n\n```\ncode\n```\n\n\
                  The first real words.\n";
        assert_eq!(opening(md), "The first real words.");
        assert_eq!(
            note(md, 280).text,
            format!("The first real words.\n\n{LINK}")
        );
    }

    #[test]
    fn inline_markdown_becomes_plain_text() {
        let md = "Read **this** and `that` on [the shore](https://blyg.example.com/s) \
                  ![[img02]] with ~~no~~ __marks__ ![x](https://blyg.example.com/x.png) left.";
        assert_eq!(
            opening(md),
            "Read this and that on the shore with no marks left."
        );
        assert_eq!(plain("a [b] c [d](e"), "a [b] c [d](e");
    }

    #[test]
    fn a_labelled_link_reads_as_its_label() {
        let id = "0a1b2c3d4e5f6g7h8j9k0m1n2p";
        assert_eq!(
            plain(&format!("As in [[{id} | the **earlier** post]], yes.")),
            "As in the earlier post, yes."
        );
        assert_eq!(
            plain(&format!("Quoting ![[{id} | a note]] here")),
            "Quoting here"
        );
        // No label: left as written, as before.
        assert_eq!(plain(&format!("See [[{id}]].")), format!("See [[{id}]]."));
    }

    #[test]
    fn the_whole_note_fits_max_chars_link_included() {
        let words = "word ".repeat(100);
        let p = note(&words, 80);
        assert!(visible_len(&p.text) <= 80, "{}", visible_len(&p.text));
        assert!(p.text.ends_with(LINK));
        let (excerpt, _) = p.text.split_once("\n\n").unwrap();
        assert!(excerpt.ends_with("word…"), "{excerpt}");
        assert_eq!(p.note.as_deref(), Some("Cut to 80 characters (max-chars)"));
    }

    #[test]
    fn emoji_count_as_one_and_are_never_split() {
        let family = "👨‍👩‍👧‍👦"; // 7 chars, 1 cluster
        let thumbs = "👍🏽"; // 2 chars, 1 cluster
        assert_eq!(visible_len(family), 1);
        assert_eq!(visible_len(&format!("{thumbs}{family}🇳🇿")), 3);
        let s = family.repeat(10);
        let c = cut_visible(&s, 5);
        assert_eq!(c, format!("{}…", family.repeat(4)));
        assert_eq!(visible_len(&c), 5);
        // A char-counting cut would keep only part of the first family.
        assert_eq!(cut_visible(&s, 10), s);
        let words = format!("{thumbs} {thumbs} {thumbs} {thumbs}");
        assert_eq!(cut_visible(&words, 4), format!("{thumbs}…"));
        let p = prepare(Some(&s), "", "", "{{excerpt}}", 3, "");
        assert_eq!(p.text, format!("{family}{family}…"));
    }

    #[test]
    fn a_long_word_is_cut_inside() {
        let long = "x".repeat(400);
        assert_eq!(cut_visible(&long, 10), format!("{}…", "x".repeat(9)));
        let p = note(&long, 280);
        assert!(visible_len(&p.text) <= 280);
        assert!(p.text.ends_with(&format!("…\n\n{LINK}")));
    }

    #[test]
    fn an_empty_post_is_just_the_link() {
        for md in [
            "",
            "\n\n",
            "# Only a heading\n",
            "> only a quote",
            "![[img03]]",
        ] {
            let p = note(md, 280);
            assert_eq!(p.text, LINK, "{md:?}");
            assert_eq!(
                p.note.as_deref(),
                Some("The post has no opening paragraph to quote")
            );
        }
    }

    #[test]
    fn no_text_from_the_host_uses_its_template() {
        let p = prepare(None, "T", LINK, DEFAULT_TEMPLATE, 280, "Hi\n\n\n\nthere \n");
        assert_eq!(p.text, "Hi\n\nthere");
        assert_eq!(p.note, None);
    }

    #[test]
    fn templates_with_a_title_and_no_room() {
        let p = prepare(
            Some("Body text here."),
            "Tides",
            LINK,
            "{{title}}: {{excerpt}}\n{{permalink}}",
            280,
            "",
        );
        assert_eq!(p.text, format!("Tides: Body text here.\n{LINK}"));
        let p = note("Body text here.", 10);
        assert_eq!(p.text, LINK);
        assert!(p.note.unwrap().contains("even without the excerpt"));
        let p = note("Body text here.", visible_len(LINK) + 2);
        assert_eq!(p.text, LINK);
        assert!(p.note.unwrap().starts_with("No room for the excerpt"));
        let p = prepare(Some("Body."), "T", LINK, "{{permalink}}", 280, "");
        assert_eq!((p.text.as_str(), p.note), (LINK, None));
    }

    fn within(md: &str, limit: Limit) -> Prepared {
        prepare_within(Some(md), "Tides", LINK, DEFAULT_TEMPLATE, limit, "")
    }

    #[test]
    fn a_bluesky_post_fits_300_characters() {
        let limit = Limit::chars(DEFAULT_BLUESKY_MAX_CHARS, "bluesky-max-chars");
        let p = within(&"tide ".repeat(100), limit);
        assert!(visible_len(&p.text) <= 300, "{}", visible_len(&p.text));
        assert!(visible_len(&p.text) > 280, "uses the room it has");
        assert!(p.text.ends_with(LINK));
        assert_eq!(
            p.note.as_deref(),
            Some("Cut to 300 characters (bluesky-max-chars)")
        );
        let p = within("Short.", limit);
        assert_eq!((p.text, p.note), (format!("Short.\n\n{LINK}"), None));
    }

    #[test]
    fn a_cast_fits_320_bytes_not_characters() {
        let limit = Limit::bytes(DEFAULT_FARCASTER_MAX_BYTES, "farcaster-max-bytes");
        // "é" is one character and two bytes: 200 characters, 400 bytes.
        let p = within(&"éé ".repeat(67), limit);
        assert!(p.text.len() <= 320, "{}", p.text.len());
        assert!(p.text.len() > 310, "uses the room it has: {}", p.text.len());
        assert!(p.text.ends_with(LINK));
        let (excerpt, _) = p.text.split_once("\n\n").unwrap();
        assert!(excerpt.ends_with("éé…"), "cut on a word: {excerpt}");
        assert_eq!(
            p.note.as_deref(),
            Some("Cut to 320 bytes (farcaster-max-bytes)")
        );
        // The same post is short enough in characters.
        let chars = within(&"éé ".repeat(67), Limit::chars(320, "x"));
        assert_eq!(chars.note, None);
    }

    #[test]
    fn a_byte_cut_never_splits_a_character_or_an_emoji() {
        let family = "👨‍👩‍👧‍👦"; // 25 bytes, 1 cluster
        assert_eq!(family.len(), 25);
        let s = family.repeat(4);
        assert_eq!(cut_bytes(&s, 100), s);
        // 99 bytes: room for 96 before "…", so three families.
        assert_eq!(cut_bytes(&s, 99), format!("{}…", family.repeat(3)));
        assert_eq!(cut_bytes(&s, 27), "…");
        assert_eq!(cut_bytes(&s, 2), "");
        assert_eq!(cut_bytes("ab", 0), "");
        let words = "tide pool ".repeat(5);
        let c = cut_bytes(&words, 20);
        assert_eq!(c, "tide pool tide…");
        assert!(c.len() <= 20);
        // No space to cut at: cut inside the word, on a character.
        assert_eq!(cut_bytes(&"é".repeat(20), 10), "ééé…");
    }

    #[test]
    fn byte_limits_too_small_say_so() {
        let limit = Limit::bytes(20, "farcaster-max-bytes");
        let p = within("Body text here.", limit);
        assert_eq!(p.text, LINK);
        assert_eq!(
            p.note.as_deref(),
            Some("Over the 20-byte limit (farcaster-max-bytes) even without the excerpt")
        );
        let p = within(
            "Body text here.",
            Limit::bytes(LINK.len() + 3, "farcaster-max-bytes"),
        );
        assert_eq!(p.text, LINK);
        assert!(p.note.unwrap().starts_with("No room for the excerpt in"));
    }

    #[test]
    fn settings_parse_with_defaults() {
        assert_eq!(template_setting(None).unwrap(), DEFAULT_TEMPLATE);
        assert_eq!(template_setting(Some("  ")).unwrap(), DEFAULT_TEMPLATE);
        assert_eq!(
            template_setting(Some("{{title}}\\n{{permalink}}")).unwrap(),
            "{{title}}\n{{permalink}}"
        );
        let (fallback, why) = template_setting(Some("{{body}}")).unwrap_err();
        assert_eq!(fallback, DEFAULT_TEMPLATE);
        assert!(why.contains("{{body}}"), "{why}");
        assert_eq!(max_chars_setting(None).unwrap(), DEFAULT_MAX_CHARS);
        assert_eq!(max_chars_setting(Some(" 500 ")).unwrap(), 500);
        for bad in ["0", "-3", "lots", "2.5"] {
            assert_eq!(
                max_chars_setting(Some(bad)).unwrap_err().0,
                DEFAULT_MAX_CHARS
            );
        }
        let key = "farcaster-max-bytes";
        assert_eq!(limit_setting(key, None, 320).unwrap(), 320);
        assert_eq!(limit_setting(key, Some("1024"), 320).unwrap(), 1024);
        let (fallback, why) = limit_setting(key, Some("big"), 320).unwrap_err();
        assert_eq!(fallback, 320);
        assert!(why.starts_with("farcaster-max-bytes \"big\""), "{why}");
    }
}
