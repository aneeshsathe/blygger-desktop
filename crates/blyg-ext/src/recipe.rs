//! Macro recipes: the declarative steps a `[[macros]]` entry runs in the
//! browser pane, and their static rules. Pure: no I/O, no browser. The
//! host's runner (in the app) executes the steps; this module only says
//! what a valid recipe is, and fills in the text.
//!
//! ```toml
//! steps = [
//!   { do = "open", url = "https://social.example.com/notes" },
//!   { do = "waitFor", selector = "div[contenteditable='true']", timeout = "20s" },
//!   { do = "focus", selector = "div[contenteditable='true']" },
//!   { do = "insert", selector = "div[contenteditable='true']" },
//!   { do = "submit", selector = "button", text = "Post" },
//!   { do = "waitFor", selector = "div[contenteditable='true']", empty = true },
//!   { do = "done", text = "Posted" },
//! ]
//! ```
//!
//! A selector is a `querySelectorAll` selector; with `text`, the first
//! match whose `innerText.trim()` equals it exactly.
//!
//! The rules ([`check_macro`]): the first step is `open`; every `open` URL
//! is on one of the manifest's sites; exactly one `insert`; at least one
//! `submit` after it and none before; no `click`, and no `open` before the
//! first `submit`, once the text is in (only the previewed text is typed,
//! and only `submit` follows it, after the user confirms); `done` is the
//! last step and only there; timeouts are positive and at most 30 s.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::protocol::{MacroSpec, SiteSpec};

/// A step without a `timeout` waits this long for its element.
pub const DEFAULT_STEP_TIMEOUT: Duration = Duration::from_secs(10);
/// The longest `timeout` a step may ask for.
pub const MAX_STEP_TIMEOUT: Duration = Duration::from_secs(30);
/// The longest a whole run may take, not counting the time the user spends
/// on the preview and confirm sheets.
pub const MAX_RUN: Duration = Duration::from_secs(180);
/// The most steps a recipe may have.
pub const MAX_STEPS: usize = 64;
/// A macro's text when its manifest gives no `template`.
pub const DEFAULT_TEMPLATE: &str = "{{excerpt}}\n\n{{permalink}}";
/// The placeholders a template may use.
pub const PLACEHOLDERS: &[&str] = &["title", "excerpt", "permalink"];

/// One step, as written (`{ do = "waitFor", selector = "…" }`). Unknown
/// keys are ignored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "do")]
pub enum Step {
    /// Load `url` in the pane (on one of the manifest's sites).
    #[serde(rename = "open")]
    Open { url: String },
    /// Wait until `selector` (with `text`) matches; with `empty`, until its
    /// match has no text; with `absent`, until nothing matches.
    #[serde(rename = "waitFor")]
    WaitFor {
        selector: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        text: Option<String>,
        #[serde(default, skip_serializing_if = "is_false")]
        empty: bool,
        #[serde(default, skip_serializing_if = "is_false")]
        absent: bool,
        /// A duration (`"20s"`, `"500ms"`); [`DEFAULT_STEP_TIMEOUT`] when
        /// absent, at most [`MAX_STEP_TIMEOUT`].
        #[serde(default, skip_serializing_if = "Option::is_none")]
        timeout: Option<String>,
    },
    /// Stop (nothing posted) unless `selector` (with `text`) matches now.
    #[serde(rename = "assert")]
    Assert {
        selector: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        text: Option<String>,
    },
    /// Focus the match.
    #[serde(rename = "focus")]
    Focus { selector: String },
    /// Click the match (only before `insert`: opening a composer, say).
    #[serde(rename = "click")]
    Click {
        selector: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        text: Option<String>,
    },
    /// Put the previewed text into the match. Exactly one per recipe.
    #[serde(rename = "insert")]
    Insert { selector: String },
    /// Click the match to post; the first one runs only after the user
    /// confirms on the Post sheet.
    #[serde(rename = "submit")]
    Submit {
        selector: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        text: Option<String>,
    },
    /// The end: `text` is the success message.
    #[serde(rename = "done")]
    Done {
        #[serde(default)]
        text: String,
    },
}

fn is_false(b: &bool) -> bool {
    !*b
}

impl Step {
    /// The `do` name: `open`, `waitFor`, ….
    pub fn name(&self) -> &'static str {
        match self {
            Step::Open { .. } => "open",
            Step::WaitFor { .. } => "waitFor",
            Step::Assert { .. } => "assert",
            Step::Focus { .. } => "focus",
            Step::Click { .. } => "click",
            Step::Insert { .. } => "insert",
            Step::Submit { .. } => "submit",
            Step::Done { .. } => "done",
        }
    }

    /// The element selector (`None` for `open` and `done`).
    pub fn selector(&self) -> Option<&str> {
        match self {
            Step::WaitFor { selector, .. }
            | Step::Assert { selector, .. }
            | Step::Focus { selector }
            | Step::Click { selector, .. }
            | Step::Insert { selector }
            | Step::Submit { selector, .. } => Some(selector),
            Step::Open { .. } | Step::Done { .. } => None,
        }
    }

    /// The exact `innerText.trim()` the match must have, if any (not
    /// `done`'s message).
    pub fn match_text(&self) -> Option<&str> {
        match self {
            Step::WaitFor { text, .. }
            | Step::Assert { text, .. }
            | Step::Click { text, .. }
            | Step::Submit { text, .. } => text.as_deref(),
            _ => None,
        }
    }

    /// How long the step may wait: its `timeout`, else
    /// [`DEFAULT_STEP_TIMEOUT`] (also for an unparsable one, which
    /// [`check_macro`] refuses).
    pub fn timeout(&self) -> Duration {
        match self {
            Step::WaitFor {
                timeout: Some(t), ..
            } => parse_duration(t).unwrap_or(DEFAULT_STEP_TIMEOUT),
            _ => DEFAULT_STEP_TIMEOUT,
        }
    }
}

/// `"20s"`, `"500ms"`, `"2m"` (a whole number and a unit; a space between
/// is allowed). `Err` says what's wrong.
pub fn parse_duration(s: &str) -> Result<Duration, String> {
    let t = s.trim();
    let split = t.find(|c: char| !c.is_ascii_digit()).unwrap_or(t.len());
    let (num, unit) = t.split_at(split);
    let bad = || format!("{s:?} isn't a duration like \"20s\", \"500ms\" or \"2m\"");
    let n: u64 = num.parse().map_err(|_| bad())?;
    let d = match unit.trim_start() {
        "ms" => Some(Duration::from_millis(n)),
        "s" => Some(Duration::from_secs(n)),
        "m" => n.checked_mul(60).map(Duration::from_secs),
        _ => None,
    };
    d.ok_or_else(bad)
}

/// What a template's placeholders become. CRLF and lone CR become LF.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Vars {
    pub title: String,
    pub excerpt: String,
    pub permalink: String,
}

fn lf(s: &str) -> String {
    s.replace("\r\n", "\n").replace('\r', "\n")
}

/// The placeholder names in `template`, in order, as written inside the
/// braces (trimmed): `{{ title }}` is `title`.
fn placeholders(template: &str) -> Vec<(usize, usize, &str)> {
    let mut out = vec![];
    let mut at = 0;
    while let Some(open) = template[at..].find("{{") {
        let start = at + open;
        let Some(close) = template[start + 2..].find("}}") else {
            break;
        };
        let end = start + 2 + close + 2;
        out.push((start, end, template[start + 2..end - 2].trim()));
        at = end;
    }
    out
}

/// Fill in `{{title}}`, `{{excerpt}}` and `{{permalink}}` (spaces inside
/// the braces allowed). One pass: a value that itself contains `{{…}}` is
/// left as it is, never expanded again. An unknown placeholder stays as
/// written ([`check_template`] refuses it at load). Line endings become LF.
pub fn expand(template: &str, vars: &Vars) -> String {
    let template = lf(template);
    let mut out = String::with_capacity(template.len() + vars.excerpt.len());
    let mut at = 0;
    for (start, end, name) in placeholders(&template) {
        let value = match name {
            "title" => &vars.title,
            "excerpt" => &vars.excerpt,
            "permalink" => &vars.permalink,
            _ => continue,
        };
        out.push_str(&template[at..start]);
        out.push_str(&lf(value));
        at = end;
    }
    out.push_str(&template[at..]);
    out
}

/// `Err` naming the first placeholder [`expand`] doesn't know.
pub fn check_template(template: &str) -> Result<(), String> {
    for (_, _, name) in placeholders(&lf(template)) {
        if !PLACEHOLDERS.contains(&name) {
            return Err(format!(
                "template: unknown placeholder {{{{{name}}}}}; use {{{{title}}}}, \
                 {{{{excerpt}}}} or {{{{permalink}}}}"
            ));
        }
    }
    Ok(())
}

/// The first paragraph of a post's Markdown that reads as prose (not a
/// heading, a `>` quote, a `![[id]]` or `![…](…)` image, a code fence, a
/// rule or a table), its lines joined with spaces, cut to at most
/// `max_chars` characters on a word boundary with "…". Empty when there is
/// none. CRLF is fine.
pub fn excerpt(markdown: &str, max_chars: usize) -> String {
    let text = lf(markdown);
    let mut in_fence = false;
    let mut para: Vec<&str> = vec![];
    let mut found: Option<String> = None;
    let prose = |p: &[&str]| -> Option<String> {
        let first = p.first()?.trim_start();
        let skip = first.starts_with('#')
            || first.starts_with('>')
            || first.starts_with("![")
            || first.starts_with('|')
            || first.starts_with("---")
            || first.starts_with("***");
        (!skip).then(|| {
            p.iter()
                .map(|l| l.trim())
                .filter(|l| !l.is_empty())
                .collect::<Vec<_>>()
                .join(" ")
        })
    };
    for line in text.lines() {
        let t = line.trim_start();
        if t.starts_with("```") || t.starts_with("~~~") {
            in_fence = !in_fence;
            para.clear();
            continue;
        }
        if in_fence {
            continue;
        }
        if t.is_empty() {
            if let Some(p) = prose(&para) {
                found = Some(p);
                break;
            }
            para.clear();
        } else {
            para.push(line);
        }
    }
    let p = found.or_else(|| if in_fence { None } else { prose(&para) });
    cut(&p.unwrap_or_default(), max_chars)
}

/// `s` cut to at most `max` characters, at the last space that fits, with
/// "…" (counted) when anything was cut.
pub fn cut(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    if max == 0 {
        return String::new();
    }
    let head: String = s.chars().take(max - 1).collect();
    let at = head.rfind(char::is_whitespace).filter(|&i| i > 0);
    let kept = match at {
        Some(i) => head[..i].trim_end(),
        None => head.as_str(),
    };
    format!("{kept}…")
}

/// Whether `url` is on `origin` (both compared normalised).
pub fn url_on(url: &str, origin: &str) -> bool {
    match (
        blyg_core::config::parse::url_origin(url),
        crate::capability::normalize_origin(origin),
    ) {
        (Some(a), Ok(b)) => a == b,
        _ => false,
    }
}

/// The index of the first `submit`: the host runs the steps before it,
/// shows the confirm sheet, and runs it and the rest only after Post.
pub fn first_submit(steps: &[Step]) -> Option<usize> {
    steps.iter().position(|s| matches!(s, Step::Submit { .. }))
}

/// The index of the (one) `insert`.
pub fn insert_index(steps: &[Step]) -> Option<usize> {
    steps.iter().position(|s| matches!(s, Step::Insert { .. }))
}

/// Why a site or macro isn't valid. Its `Display` is the manifest
/// diagnostic: `macro "cross-post" step 3 (click): …`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecipeError {
    /// `site "…"` or `macro "…"`.
    pub what: String,
    /// 1-based, when the problem is one step.
    pub step: Option<(usize, &'static str)>,
    pub message: String,
}

impl std::fmt::Display for RecipeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.step {
            Some((n, name)) => write!(f, "{} step {n} ({name}): {}", self.what, self.message),
            None => write!(f, "{}: {}", self.what, self.message),
        }
    }
}

impl std::error::Error for RecipeError {}

/// Check a site; `Ok` is its normalised origin.
pub fn check_site(site: &SiteSpec) -> Result<String, RecipeError> {
    let e = |m: String| RecipeError {
        what: format!("site {:?}", site.id),
        step: None,
        message: m,
    };
    if site.title.trim().is_empty() {
        return Err(e("has no title".into()));
    }
    let origin =
        crate::capability::normalize_origin(&site.origin).map_err(|m| e(format!("origin: {m}")))?;
    if !url_on(&site.home, &origin) {
        return Err(e(format!(
            "home {:?} must be an http(s) URL on {origin}",
            site.home
        )));
    }
    if site
        .signed_out
        .as_deref()
        .is_some_and(|s| s.trim().is_empty())
    {
        return Err(e("signed-out is empty; leave it out".into()));
    }
    if let Some(m) = &site.min_interval {
        parse_duration(m).map_err(|m| e(format!("min-interval: {m}")))?;
    }
    Ok(origin)
}

/// Check a macro against the manifest's `sites` (already checked with
/// [`check_site`], origins normalised).
pub fn check_macro(m: &MacroSpec, sites: &[SiteSpec]) -> Result<(), RecipeError> {
    let what = format!("macro {:?}", m.id);
    let whole = |msg: String| RecipeError {
        what: what.clone(),
        step: None,
        message: msg,
    };
    let at = |i: usize, s: &Step, msg: String| RecipeError {
        what: what.clone(),
        step: Some((i + 1, s.name())),
        message: msg,
    };
    if m.title.trim().is_empty() {
        return Err(whole("has no title".into()));
    }
    if !sites.iter().any(|s| s.id == m.site) {
        return Err(whole(format!(
            "site {:?} isn't one of this extension's [[sites]]",
            m.site
        )));
    }
    check_template(&m.template).map_err(whole)?;
    let steps = &m.steps;
    if steps.is_empty() {
        return Err(whole("has no steps".into()));
    }
    if steps.len() > MAX_STEPS {
        return Err(whole(format!(
            "has {} steps (at most {MAX_STEPS})",
            steps.len()
        )));
    }
    let origins: Vec<&str> = sites.iter().map(|s| s.origin.as_str()).collect();
    // Each step on its own.
    for (i, s) in steps.iter().enumerate() {
        if let Some(sel) = s.selector()
            && sel.trim().is_empty()
        {
            return Err(at(i, s, "selector is empty".into()));
        }
        if s.match_text().is_some_and(|t| t.trim().is_empty()) {
            return Err(at(i, s, "text is empty; leave it out".into()));
        }
        match s {
            Step::Open { url } => {
                if blyg_core::config::parse::url_origin(url).is_none() {
                    return Err(at(i, s, format!("{url:?} isn't an http(s) URL")));
                }
                if !origins.iter().any(|o| url_on(url, o)) {
                    return Err(at(
                        i,
                        s,
                        format!(
                            "{url:?} isn't on one of this extension's sites ({})",
                            origins.join(", ")
                        ),
                    ));
                }
            }
            Step::WaitFor {
                empty,
                absent,
                timeout,
                ..
            } => {
                if *empty && *absent {
                    return Err(at(i, s, "empty and absent can't both be set".into()));
                }
                if let Some(t) = timeout {
                    let d = parse_duration(t).map_err(|m| at(i, s, format!("timeout: {m}")))?;
                    if d.is_zero() {
                        return Err(at(i, s, "timeout must be more than 0".into()));
                    }
                    if d > MAX_STEP_TIMEOUT {
                        return Err(at(i, s, format!("timeout {t:?} is over the 30s limit")));
                    }
                }
            }
            _ => {}
        }
    }
    // The shape.
    if !matches!(steps[0], Step::Open { .. }) {
        return Err(at(
            0,
            &steps[0],
            "the first step must be open (a macro starts by loading a page on its site)".into(),
        ));
    }
    let inserts: Vec<usize> = steps
        .iter()
        .enumerate()
        .filter(|(_, s)| matches!(s, Step::Insert { .. }))
        .map(|(i, _)| i + 1)
        .collect();
    let ins = match inserts.as_slice() {
        [] => return Err(whole("has no insert step (exactly one is needed)".into())),
        [one] => one - 1,
        many => {
            let list: Vec<String> = many.iter().map(|n| n.to_string()).collect();
            return Err(whole(format!(
                "has {} insert steps ({}); exactly one is allowed",
                many.len(),
                list.join(", ")
            )));
        }
    };
    if let Some(i) = steps[..ins]
        .iter()
        .position(|s| matches!(s, Step::Submit { .. }))
    {
        return Err(at(
            i,
            &steps[i],
            "submit before insert: there's nothing to post yet".into(),
        ));
    }
    for (i, s) in steps.iter().enumerate().skip(ins + 1) {
        if matches!(s, Step::Click { .. }) {
            return Err(at(
                i,
                s,
                "click after insert: once the text is in, only submit may click".into(),
            ));
        }
    }
    let Some(sub) = steps[ins..]
        .iter()
        .position(|s| matches!(s, Step::Submit { .. }))
        .map(|k| ins + k)
    else {
        return Err(whole("has no submit step after insert".into()));
    };
    for (i, s) in steps.iter().enumerate().take(sub).skip(ins + 1) {
        if matches!(s, Step::Open { .. }) {
            return Err(at(
                i,
                s,
                "open between insert and submit would lose the text".into(),
            ));
        }
    }
    let last = steps.len() - 1;
    for (i, s) in steps.iter().enumerate() {
        if matches!(s, Step::Done { .. }) && i != last {
            return Err(at(i, s, "done must be the last step".into()));
        }
    }
    if !matches!(steps[last], Step::Done { .. }) {
        return Err(at(last, &steps[last], "the last step must be done".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::When;

    fn site() -> SiteSpec {
        SiteSpec {
            id: "social".into(),
            title: "Social".into(),
            origin: "https://social.example.com".into(),
            home: "https://social.example.com/notes".into(),
            signed_out: Some("a[href*='sign-in']".into()),
            content_blocking: true,
            min_interval: Some("60s".into()),
        }
    }

    const BOX: &str = "div[contenteditable='true']";

    fn good_steps() -> Vec<Step> {
        vec![
            Step::Open {
                url: "https://social.example.com/notes".into(),
            },
            Step::WaitFor {
                selector: BOX.into(),
                text: None,
                empty: false,
                absent: false,
                timeout: Some("20s".into()),
            },
            Step::Click {
                selector: "button".into(),
                text: Some("New note".into()),
            },
            Step::Focus {
                selector: BOX.into(),
            },
            Step::Insert {
                selector: BOX.into(),
            },
            Step::Submit {
                selector: "button".into(),
                text: Some("Post".into()),
            },
            Step::WaitFor {
                selector: BOX.into(),
                text: None,
                empty: true,
                absent: false,
                timeout: Some("15s".into()),
            },
            Step::Done {
                text: "Posted".into(),
            },
        ]
    }

    fn mac(steps: Vec<Step>) -> MacroSpec {
        MacroSpec {
            id: "cross-post".into(),
            title: "Cross-post…".into(),
            detail: String::new(),
            site: "social".into(),
            when: When::Published,
            template: DEFAULT_TEMPLATE.into(),
            tested: "unverified".into(),
            steps,
        }
    }

    fn err_of(steps: Vec<Step>) -> String {
        check_macro(&mac(steps), &[site()]).unwrap_err().to_string()
    }

    #[test]
    fn durations() {
        assert_eq!(parse_duration("20s"), Ok(Duration::from_secs(20)));
        assert_eq!(parse_duration("500ms"), Ok(Duration::from_millis(500)));
        assert_eq!(parse_duration(" 2m "), Ok(Duration::from_secs(120)));
        assert_eq!(parse_duration("30 s"), Ok(Duration::from_secs(30)));
        assert_eq!(parse_duration("0s"), Ok(Duration::ZERO));
        for bad in ["", "20", "s", "1.5s", "-1s", "20sec", "20S", "ten s", "1h"] {
            assert!(parse_duration(bad).is_err(), "accepted {bad:?}");
        }
        assert!(parse_duration("99999999999999999999m").is_err());
        assert!(parse_duration(&format!("{}m", u64::MAX)).is_err());
    }

    #[test]
    fn templates_expand_once_and_normalise_line_endings() {
        let v = Vars {
            title: "Hello".into(),
            excerpt: "line one\r\nline two {{permalink}}".into(),
            permalink: "https://blyg.example.com/p/1".into(),
        };
        assert_eq!(
            expand("{{title}}: {{ excerpt }}\r\n\r\n{{permalink}}", &v),
            "Hello: line one\nline two {{permalink}}\n\nhttps://blyg.example.com/p/1"
        );
        assert_eq!(
            expand(DEFAULT_TEMPLATE, &v),
            "line one\nline two {{permalink}}\n\nhttps://blyg.example.com/p/1"
        );
        assert_eq!(expand("a\rb", &Vars::default()), "a\nb");
        assert_eq!(expand("{{nope}} {{title", &v), "{{nope}} {{title");
        assert_eq!(expand("$1 \\0 {{title}}$", &v), "$1 \\0 Hello$");
        assert!(check_template("{{title}} {{ excerpt }}\r\n{{permalink}}").is_ok());
        assert!(check_template("no placeholders").is_ok());
        let e = check_template("{{title}} {{author}}").unwrap_err();
        assert!(e.contains("{{author}}"), "{e}");
    }

    #[test]
    fn excerpts_take_the_first_prose_paragraph() {
        let md = "# Title\r\n\r\n> a quote\r\n\r\n![[abc123]]\r\n\r\n```\r\ncode\r\n\r\nmore\r\n```\r\n\r\nFirst real\r\nparagraph here.\r\n\r\nSecond.";
        assert_eq!(excerpt(md, 280), "First real paragraph here.");
        assert_eq!(excerpt("only one line", 280), "only one line");
        assert_eq!(excerpt("# just a heading", 280), "");
        assert_eq!(excerpt("", 280), "");
        assert_eq!(excerpt("one two three four", 12), "one two…");
        assert_eq!(excerpt("abcdefghijkl", 5), "abcd…");
        let emoji = "🦀🦀🦀 🦀🦀🦀 🦀🦀🦀";
        let got = excerpt(emoji, 8);
        assert_eq!(got, "🦀🦀🦀…");
        assert!(got.chars().count() <= 8);
        assert_eq!(cut("abc", 0), "");
    }

    #[test]
    fn the_example_recipe_is_valid_and_splits_at_submit() {
        let m = mac(good_steps());
        check_macro(&m, &[site()]).unwrap();
        assert_eq!(insert_index(&m.steps), Some(4));
        assert_eq!(first_submit(&m.steps), Some(5));
        assert_eq!(m.steps[1].timeout(), Duration::from_secs(20));
        assert_eq!(m.steps[3].timeout(), DEFAULT_STEP_TIMEOUT);
        assert_eq!(m.steps[5].match_text(), Some("Post"));
        assert_eq!(m.steps[0].selector(), None);
        assert_eq!(check_site(&site()).unwrap(), "https://social.example.com");
    }

    #[test]
    fn every_open_is_on_a_site() {
        let mut s = good_steps();
        s[0] = Step::Open {
            url: "https://other.example.com/notes".into(),
        };
        let e = err_of(s.clone());
        assert!(e.starts_with("macro \"cross-post\" step 1 (open):"), "{e}");
        assert!(e.contains("isn't on one of this extension's sites"), "{e}");
        s[0] = Step::Open {
            url: "http://social.example.com/notes".into(),
        };
        assert!(err_of(s.clone()).contains("isn't on"), "scheme counts");
        s[0] = Step::Open {
            url: "https://social.example.com:8443/".into(),
        };
        assert!(err_of(s.clone()).contains("isn't on"), "port counts");
        s[0] = Step::Open {
            url: "javascript:alert(1)".into(),
        };
        assert!(err_of(s.clone()).contains("isn't an http(s) URL"));
        s[0] = Step::Open {
            url: "HTTPS://Social.Example.com:443/notes?x#y".into(),
        };
        assert!(check_macro(&mac(s.clone()), &[site()]).is_ok());
        // A second site of the same manifest is fine too.
        let mut other = site();
        other.id = "other".into();
        other.origin = "https://other.example.com".into();
        other.home = "https://other.example.com/".into();
        s[0] = Step::Open {
            url: "https://other.example.com/x".into(),
        };
        assert!(check_macro(&mac(s), &[site(), other]).is_ok());
    }

    #[test]
    fn exactly_one_insert_with_a_submit_after_it() {
        let mut s = good_steps();
        s.remove(4);
        assert!(err_of(s).contains("has no insert step"));
        let mut s = good_steps();
        s.insert(
            5,
            Step::Insert {
                selector: BOX.into(),
            },
        );
        let e = err_of(s);
        assert!(e.contains("2 insert steps (5, 6)"), "{e}");
        let mut s = good_steps();
        s.remove(5);
        assert!(err_of(s).contains("no submit step after insert"));
        let mut s = good_steps();
        s.insert(
            1,
            Step::Submit {
                selector: "button".into(),
                text: None,
            },
        );
        let e = err_of(s);
        assert!(e.contains("step 2 (submit): submit before insert"), "{e}");
    }

    #[test]
    fn nothing_clicks_or_navigates_once_the_text_is_in() {
        let mut s = good_steps();
        s.insert(
            5,
            Step::Click {
                selector: "button".into(),
                text: None,
            },
        );
        let e = err_of(s);
        assert!(e.contains("step 6 (click): click after insert"), "{e}");
        let mut s = good_steps();
        s.insert(
            7,
            Step::Click {
                selector: "a".into(),
                text: None,
            },
        );
        assert!(err_of(s).contains("click after insert"), "after submit too");
        let mut s = good_steps();
        s.insert(
            5,
            Step::Open {
                url: "https://social.example.com/".into(),
            },
        );
        assert!(err_of(s).contains("would lose the text"));
        let mut s = good_steps();
        s.insert(
            6,
            Step::Open {
                url: "https://social.example.com/mine".into(),
            },
        );
        assert!(
            check_macro(&mac(s), &[site()]).is_ok(),
            "open after submit is fine"
        );
    }

    #[test]
    fn done_comes_last_and_open_first() {
        let mut s = good_steps();
        s.pop();
        assert!(err_of(s).contains("step 7 (waitFor): the last step must be done"));
        let mut s = good_steps();
        s.insert(2, Step::Done { text: "x".into() });
        assert!(err_of(s).contains("step 3 (done): done must be the last step"));
        let mut s = good_steps();
        s.remove(0);
        assert!(err_of(s).contains("step 1 (waitFor): the first step must be open"));
        assert!(err_of(vec![]).contains("has no steps"));
    }

    #[test]
    fn timeouts_and_selectors_are_checked() {
        let with_timeout = |t: &str| {
            let mut s = good_steps();
            s[1] = Step::WaitFor {
                selector: BOX.into(),
                text: None,
                empty: false,
                absent: false,
                timeout: Some(t.into()),
            };
            check_macro(&mac(s), &[site()]).map_err(|e| e.to_string())
        };
        assert!(with_timeout("30s").is_ok());
        assert!(with_timeout("30000ms").is_ok());
        assert!(
            with_timeout("30001ms")
                .unwrap_err()
                .contains("over the 30s limit")
        );
        assert!(
            with_timeout("1m")
                .unwrap_err()
                .contains("over the 30s limit")
        );
        assert!(with_timeout("0ms").unwrap_err().contains("more than 0"));
        let e = with_timeout("20 seconds").unwrap_err();
        assert!(e.contains("step 2 (waitFor): timeout:"), "{e}");
        let mut s = good_steps();
        s[3] = Step::Focus {
            selector: "  ".into(),
        };
        assert!(err_of(s).contains("step 4 (focus): selector is empty"));
        let mut s = good_steps();
        s[5] = Step::Submit {
            selector: "button".into(),
            text: Some(" ".into()),
        };
        assert!(err_of(s).contains("text is empty"));
        let mut s = good_steps();
        s[6] = Step::WaitFor {
            selector: BOX.into(),
            text: None,
            empty: true,
            absent: true,
            timeout: None,
        };
        assert!(err_of(s).contains("empty and absent"));
        let many: Vec<Step> = std::iter::repeat_n(
            Step::Focus {
                selector: BOX.into(),
            },
            MAX_STEPS + 1,
        )
        .collect();
        assert!(err_of(many).contains("at most"));
    }

    #[test]
    fn the_macro_needs_its_site_and_a_known_template() {
        let mut m = mac(good_steps());
        m.site = "nowhere".into();
        let e = check_macro(&m, &[site()]).unwrap_err().to_string();
        assert_eq!(
            e,
            "macro \"cross-post\": site \"nowhere\" isn't one of this extension's [[sites]]"
        );
        let mut m = mac(good_steps());
        m.template = "{{body}}".into();
        assert!(
            check_macro(&m, &[site()])
                .unwrap_err()
                .to_string()
                .contains("unknown placeholder {{body}}")
        );
        let mut m = mac(good_steps());
        m.title = " ".into();
        assert!(check_macro(&m, &[site()]).is_err());
    }

    #[test]
    fn sites_are_checked_and_normalised() {
        let mut s = site();
        s.origin = "HTTPS://Social.Example.com:443/".into();
        assert_eq!(check_site(&s).unwrap(), "https://social.example.com");
        s.home = "https://social.example.com.evil.example/notes".into();
        let e = check_site(&s).unwrap_err().to_string();
        assert!(e.starts_with("site \"social\": home"), "{e}");
        let mut s = site();
        s.home = "http://social.example.com/notes".into();
        assert!(check_site(&s).is_err(), "home on another scheme");
        let mut s = site();
        s.origin = "https://social.example.com/notes".into();
        assert!(check_site(&s).unwrap_err().to_string().contains("origin:"));
        let mut s = site();
        s.min_interval = Some("a minute".into());
        assert!(
            check_site(&s)
                .unwrap_err()
                .to_string()
                .contains("min-interval")
        );
        let mut s = site();
        s.min_interval = Some("2m".into());
        assert_eq!(s.min_interval(), Duration::from_secs(120));
        assert!(check_site(&s).is_ok());
        let mut s = site();
        s.signed_out = Some("".into());
        assert!(check_site(&s).is_err());
        let mut s = site();
        s.title = "".into();
        assert!(check_site(&s).is_err());
    }

    #[test]
    fn steps_round_trip_through_toml_and_json() {
        #[derive(Deserialize, Serialize)]
        struct W {
            steps: Vec<Step>,
        }
        let text = r#"
steps = [
  { do = "open", url = "https://social.example.com/notes" },
  { do = "waitFor", selector = "div", timeout = "20s", someday = true },
  { do = "assert", selector = "h1", text = "Notes" },
  { do = "focus", selector = "div" },
  { do = "click", selector = "button", text = "New" },
  { do = "insert", selector = "div" },
  { do = "submit", selector = "button", text = "Post" },
  { do = "waitFor", selector = "div", absent = true },
  { do = "done", text = "Posted" },
]
"#;
        let w: W = toml::from_str(text).unwrap();
        assert_eq!(w.steps.len(), 9);
        let names: Vec<&str> = w.steps.iter().map(Step::name).collect();
        assert_eq!(
            names,
            [
                "open", "waitFor", "assert", "focus", "click", "insert", "submit", "waitFor",
                "done"
            ]
        );
        let json = serde_json::to_value(&w.steps).unwrap();
        assert_eq!(json[1]["do"], "waitFor");
        assert!(json[1].get("empty").is_none(), "false flags are left out");
        assert_eq!(json[7]["absent"], true);
        let back: Vec<Step> = serde_json::from_value(json).unwrap();
        assert_eq!(back, w.steps);
        let bad: Result<W, _> = toml::from_str("steps = [{ do = \"eval\", js = \"x\" }]");
        assert!(bad.is_err(), "no eval step");
        let bad: Result<W, _> = toml::from_str("steps = [{ do = \"focus\" }]");
        assert!(bad.is_err(), "selector is required");
    }

    #[test]
    fn url_on_compares_origins() {
        assert!(url_on(
            "https://Social.Example.com/a?b",
            "https://social.example.com/"
        ));
        assert!(!url_on(
            "https://social.example.com.evil.example/",
            "https://social.example.com"
        ));
        assert!(!url_on("about:blank", "https://social.example.com"));
        assert!(!url_on("https://social.example.com/", "not an origin"));
    }
}
