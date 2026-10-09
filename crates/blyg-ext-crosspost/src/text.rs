//! The text cross-post puts in the Substack Notes box: pure functions, no
//! I/O. The opening paragraph comes from [`blyg_ext::recipe::excerpt`] and
//! the cut from [`blyg_ext::recipe::cut`]; this module adds plain text
//! (no Markdown marks, no `![[id]]` embeds), counting characters as a
//! reader sees them (an emoji with a skin tone, a flag or a family is one,
//! and is never split), and fitting the whole note, link included, into
//! `max-chars`.

use blyg_ext::recipe::{DEFAULT_TEMPLATE, Vars, check_template, cut, excerpt, expand};
use unicode_segmentation::UnicodeSegmentation;

/// `max-chars` when it isn't set: the whole note, link included.
pub const DEFAULT_MAX_CHARS: usize = 280;

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
/// images dropped, `[text](url)` as its text, `**`, `__`, `~~` and
/// backticks removed, whitespace runs as one space.
pub fn plain(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
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
/// template) is used as it is.
pub fn prepare(
    markdown: Option<&str>,
    title: &str,
    permalink: &str,
    template: &str,
    max_chars: usize,
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
    // Everything but the excerpt, which stands in as one marker cluster.
    const MARK: &str = "\u{0}";
    let with_mark = tidy(&expand(template, &vars(MARK.into())));
    let rest = visible_len(&with_mark) - usize::from(with_mark.contains(MARK));
    let budget = max_chars.saturating_sub(rest);
    let excerpt = cut_visible(&body, budget);
    let was_cut = excerpt != body;
    let text = tidy(&expand(template, &vars(excerpt.clone())));
    let note = if body.is_empty() {
        Some("The post has no opening paragraph to quote".to_string())
    } else if visible_len(&text) > max_chars {
        Some(format!(
            "Over the {max_chars}-character limit (max-chars) even without the excerpt"
        ))
    } else if excerpt.is_empty() {
        Some(format!(
            "No room for the excerpt in {max_chars} characters (max-chars)"
        ))
    } else if was_cut {
        Some(format!("Cut to {max_chars} characters (max-chars)"))
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
    let Some(raw) = raw.map(str::trim).filter(|r| !r.is_empty()) else {
        return Ok(DEFAULT_MAX_CHARS);
    };
    match raw.parse::<usize>() {
        Ok(n) if n > 0 => Ok(n),
        _ => Err((
            DEFAULT_MAX_CHARS,
            format!("max-chars {raw:?} isn't a whole number above 0"),
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
    }
}
