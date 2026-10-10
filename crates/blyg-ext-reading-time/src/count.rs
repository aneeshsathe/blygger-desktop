//! How long an entry takes to read, from the HTML the reading view already
//! holds. A port of blygger-studio's `extensions/reading-time/ui/count.ts`
//! (studio 0.39.0), rule for rule, so the two clients show the same
//! number for the same post.
//!
//! Words are runs of letters and digits. Han, kana and Hangul are not
//! written with spaces, so those characters are counted one by one at their
//! own rate rather than as one enormous "word".

use std::sync::LazyLock;

use regex::Regex;

/// Words per minute for space-separated scripts.
pub const WORDS_PER_MINUTE: f64 = 230.0;
/// Characters per minute for Han, kana and Hangul.
pub const CJK_PER_MINUTE: f64 = 500.0;

// `/[\p{Script=Han}\p{Script=Hiragana}\p{Script=Katakana}\p{Script=Hangul}]/gu`
static CJK: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"[\p{Script=Han}\p{Script=Hiragana}\p{Script=Katakana}\p{Script=Hangul}]")
        .expect("valid")
});
// `/[\p{L}\p{N}][\p{L}\p{N}'’-]*/gu`
static WORD: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[\p{L}\p{N}][\p{L}\p{N}'’-]*").expect("valid"));
// The opening of `/<(script|style)\b[\s\S]*?<\/\1>/gi`; JavaScript's `\b`
// without the `u` flag is ASCII.
static RAW_OPEN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)<(script|style)(?-u:\b)").expect("valid"));
// `/<[^>]*>/g`
static TAG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<[^>]*>").expect("valid"));
// `/&[#\w]+;/g`, `\w` being ASCII without the `u` flag.
static ENTITY: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"&[#A-Za-z0-9_]+;").expect("valid"));

/// What [`reading_time`] finds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReadingTime {
    pub words: usize,
    pub cjk: usize,
    pub minutes: f64,
}

/// `<script>…</script>` and `<style>…</style>` become a space: the first
/// closing tag of the same name (any case) ends one; an opening with no
/// closing is left for the tag rule, as the Studio's regular expression
/// leaves it.
fn without_raw_text(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut done = 0; // copied up to here
    let mut from = 0; // look for the next opening from here
    while let Some(m) = RAW_OPEN.captures_at(html, from) {
        let whole = m.get(0).expect("match");
        let name = m[1].to_ascii_lowercase();
        let close = format!("</{name}>");
        let rest = &html[whole.end()..];
        match find_ascii_ci(rest, &close) {
            Some(at) => {
                out.push_str(&html[done..whole.start()]);
                out.push(' ');
                done = whole.end() + at + close.len();
                from = done;
            }
            None => {
                // No match here; JavaScript tries the next position.
                from = whole.start() + 1;
            }
        }
    }
    out.push_str(&html[done..]);
    out
}

/// Where `needle` (ASCII, lower case) first occurs in `hay`, ignoring
/// ASCII case.
fn find_ascii_ci(hay: &str, needle: &str) -> Option<usize> {
    let (h, n) = (hay.as_bytes(), needle.as_bytes());
    (0..h.len().checked_sub(n.len())? + 1).find(|&i| h[i..i + n.len()].eq_ignore_ascii_case(n))
}

/// The text of an HTML fragment, near enough for counting: tags become
/// spaces.
pub fn text_of(html: &str) -> String {
    let s = without_raw_text(html);
    let s = TAG.replace_all(&s, " ");
    ENTITY.replace_all(&s, " ").into_owned()
}

/// Words, CJK characters and minutes for an HTML fragment.
pub fn reading_time(html: &str) -> ReadingTime {
    let text = text_of(html);
    let cjk = CJK.find_iter(&text).count();
    let words = WORD.find_iter(&CJK.replace_all(&text, " ")).count();
    ReadingTime {
        words,
        cjk,
        minutes: words as f64 / WORDS_PER_MINUTE + cjk as f64 / CJK_PER_MINUTE,
    }
}

/// "< 1 min", "4 min", or "" when there is nothing to read. Rounds half
/// up, as `Math.round` does for a positive number.
pub fn format_minutes(minutes: f64) -> String {
    if minutes <= 0.0 || minutes.is_nan() {
        String::new()
    } else if minutes < 1.0 {
        "< 1 min".into()
    } else {
        format!("{} min", (minutes + 0.5).floor())
    }
}

/// The hover text: "460 words", "6 characters", "1 words, 2 characters"
/// (the Studio's words exactly, plural and all).
pub fn count_label(t: &ReadingTime) -> String {
    let mut parts = vec![];
    if t.words > 0 {
        parts.push(format!("{} words", t.words));
    }
    if t.cjk > 0 {
        parts.push(format!("{} characters", t.cjk));
    }
    parts.join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    // The Studio's own examples (test-ui/default-extensions.test.ts).
    #[test]
    fn counts_words_from_html_and_cjk_characters_separately() {
        let words: Vec<String> = (0..460).map(|i| format!("word{i}")).collect();
        let t = reading_time(&format!("<p>{}</p>", words.join(" ")));
        assert_eq!((t.words, t.cjk, t.minutes), (460, 0, 2.0));
        // Tags, entities, scripts and styles are not words; an apostrophe
        // or hyphen does not split one.
        let t = reading_time(
            "<p>It’s <em>well-known</em>&nbsp;now</p><script>var a = 1</script><style>p{}</style>",
        );
        assert_eq!(t.words, 3);
        let t = reading_time("<p>日本語の文章</p>");
        assert_eq!((t.words, t.cjk), (0, 6));
        let t = reading_time("<p>Blygger 日本</p>");
        assert_eq!((t.words, t.cjk), (1, 2));
        assert_eq!(format_minutes(0.0), "");
        assert_eq!(format_minutes(0.2), "< 1 min");
        assert_eq!(format_minutes(3.6), "4 min");
    }

    #[test]
    fn rounds_like_math_round_and_labels_like_the_studio() {
        assert_eq!(format_minutes(1.0), "1 min");
        assert_eq!(format_minutes(2.5), "3 min");
        assert_eq!(format_minutes(2.49), "2 min");
        assert_eq!(format_minutes(-1.0), "");
        let t = reading_time("<p>one two three</p>");
        assert_eq!(format_minutes(t.minutes), "< 1 min");
        assert_eq!(count_label(&t), "3 words");
        assert_eq!(
            count_label(&reading_time("<p>Blygger 日本</p>")),
            "1 words, 2 characters"
        );
        assert_eq!(count_label(&reading_time("")), "");
    }

    #[test]
    fn raw_text_ends_at_its_own_closing_tag_in_any_case() {
        assert_eq!(reading_time("<SCRIPT type=x>a b c</Script> one").words, 1);
        // `<scripts>` isn't a script (the `\b`), so only its tag goes.
        assert_eq!(reading_time("<scripts>a b</scripts>").words, 2);
        // An unclosed script is just a tag; its text counts.
        assert_eq!(reading_time("<script>a b").words, 2);
        // A style inside a script's text ends nothing.
        assert_eq!(
            reading_time("<script>x</style>y</script>z <style>q</style>w").words,
            2
        );
        // Kana, Hangul and Han all count as characters.
        let t = reading_time("カタカナ ひらがな 한국어 漢字");
        assert_eq!((t.words, t.cjk), (0, 4 + 4 + 3 + 2));
        // Numbers are words; a lone apostrophe isn't.
        assert_eq!(reading_time("1984 ’ - 3.5").words, 3);
        assert_eq!(text_of("a&amp;b&#8217;c&;d"), "a b c&;d");
    }
}
