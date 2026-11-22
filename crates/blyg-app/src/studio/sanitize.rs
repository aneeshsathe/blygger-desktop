//! Someone else's published HTML, made safe to show (spec §14: "store
//! verbatim, sanitize at render").
//!
//! An allowlist ([`ammonia`], on html5ever): ordinary text markup, links,
//! images, tables and figures, plus the classes and `data-*` attributes the
//! renderer emits ([`CLASSES`], `language-*`, `data-blyg-*`, `data-ytid`).
//! No other class or `data-*` name survives: the pages this is shown on
//! listen for their own (the web app's `data-goto`, `data-wr-*`; native's
//! `.blyg-qorigin`, `[data-line]`) and style their own as buttons, and a
//! post mustn't reach them. Everything else goes: scripts,
//! styles, event handlers, `javascript:` and other odd URLs, forms, objects,
//! embeds, SVG, and every `<iframe>` that isn't a youtube-nocookie embed.
//! A `<math>` formula is redrawn from its TeX annotation, never passed
//! through (`blyg_render::math`).
//! Every image gets `referrerpolicy="no-referrer"`.
//!
//! The page's CSP (`blyg_render::reader_csp`) is the backstop, not the
//! first line: content never gets a script nonce.

use std::borrow::Cow;
use std::sync::OnceLock;

use ammonia::{Builder, UrlRelative};

const YOUTUBE_EMBEDS: [&str; 2] = [
    "https://www.youtube-nocookie.com/embed/",
    "https://youtube-nocookie.com/embed/",
];

/// `https://www.youtube-nocookie.com/embed/<id>[?query]`, nothing else.
pub fn is_youtube_embed(src: &str) -> bool {
    YOUTUBE_EMBEDS.iter().any(|prefix| {
        src.strip_prefix(prefix).is_some_and(|rest| {
            let (id, query) = rest.split_once('?').unwrap_or((rest, ""));
            !id.is_empty()
                && id
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
                && query
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "=&_-.%".contains(c))
        })
    })
}

/// The classes the renderer puts in a post (blyg-render, which matches the
/// studio's; its parity fixtures hold them), and the reader's attachments
/// box. A code block's `language-*` is kept too.
pub const CLASSES: &[&str] = &[
    "blyg-attachments",
    "blyg-link-unresolved",
    "blyg-partial",
    "blyg-tangle-label",
    "blyg-tangle-link",
    "blyg-tangle-new",
    "blyg-tk-gen",
    "blyg-transclusion",
    "blyg-yt",
    "blyg-yt-poster",
    "h-card",
    "math-error",
    "math-source",
    "nested",
    "p-name",
    "provenance",
    "u-url",
    "unresolved",
];

/// `value`'s classes the renderer emits, or `None` when none is.
fn known_classes(value: &str) -> Option<Cow<'_, str>> {
    let kept: Vec<&str> = value
        .split_ascii_whitespace()
        .filter(|c| {
            CLASSES.contains(c) || c.strip_prefix("language-").is_some_and(|l| !l.is_empty())
        })
        .collect();
    (!kept.is_empty()).then(|| kept.join(" ").into())
}

fn builder() -> &'static Builder<'static> {
    static B: OnceLock<Builder<'static>> = OnceLock::new();
    B.get_or_init(allowlist)
}

/// The allowlist, as a fresh builder.
fn allowlist() -> Builder<'static> {
    {
        let mut b = Builder::default();
        b.add_tags(["iframe"])
            // Dropped with everything inside them (not just the tags).
            .clean_content_tags(
                [
                    "script", "style", "template", "noscript", "title", "textarea", "select",
                    "object", "embed", "applet", "svg", "math", "frameset", "noembed",
                ]
                .into_iter()
                .collect(),
            )
            .rm_tags(["map", "area"])
            .add_generic_attributes(["class", "dir", "data-ytid"])
            .add_generic_attribute_prefixes(["data-blyg-"])
            .add_tag_attributes("img", ["loading", "decoding", "referrerpolicy"])
            .add_tag_attributes(
                "iframe",
                [
                    "src",
                    "title",
                    "allow",
                    "allowfullscreen",
                    "width",
                    "height",
                    "loading",
                    "referrerpolicy",
                ],
            )
            .add_tag_attributes("li", ["value"])
            .add_tag_attributes("ol", ["reversed", "type"])
            .set_tag_attribute_value("img", "referrerpolicy", "no-referrer")
            .url_schemes(["http", "https", "mailto", "data"].into_iter().collect())
            .attribute_filter(|element, attribute, value| {
                let v = value.trim_start().to_ascii_lowercase();
                match (element, attribute) {
                    ("iframe", "src") => is_youtube_embed(value.trim()).then(|| value.into()),
                    // Inline images only; a data: link is a navigation.
                    ("img", "src") if v.starts_with("data:") => v
                        .starts_with("data:image/")
                        .then(|| value.into())
                        .filter(|_| !v.starts_with("data:image/svg")),
                    (_, "src" | "href" | "cite") if v.starts_with("data:") => None,
                    (_, "class") => known_classes(value),
                    _ => Some(Cow::Borrowed(value)),
                }
            });
        b
    }
}

/// Clean `html` for the reading view.
pub fn sanitize(html: &str) -> String {
    clean_with(builder(), html)
}

/// Clean with `b`. A `math` element never reaches ammonia: each top-level
/// formula is taken out first and redrawn afterwards from its TeX
/// annotation, or dropped without one (spec §14, `blyg_render::math`).
fn clean_with(b: &Builder<'_>, html: &str) -> String {
    let holes = blyg_render::math::hole_math(html);
    holes.fill(&drop_empty_iframes(&b.clean(holes.html()).to_string()))
}

/// [`sanitize`], with relative `src`/`href` made absolute against `base`
/// (the origin of the post the HTML belongs to). For HTML shown on a page
/// whose `<base>` is another blyg: a quoted post, say. Versions published
/// before studio 0.11 carry relative media paths (`/blyg/media/x.png`).
pub fn sanitize_at(html: &str, base: &str) -> String {
    let base = if base.ends_with('/') {
        base.to_string()
    } else {
        format!("{base}/")
    };
    match url::Url::parse(&base) {
        Ok(u) if matches!(u.scheme(), "http" | "https") => {
            let mut b = allowlist();
            b.url_relative(UrlRelative::RewriteWithBase(u));
            clean_with(&b, html)
        }
        _ => sanitize(html),
    }
}

/// An `<iframe>` whose `src` the filter refused would be an empty frame:
/// remove it outright. Runs on ammonia's normalized output, where every
/// iframe is `<iframe …>…</iframe>` with double-quoted attributes.
fn drop_empty_iframes(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(at) = rest.find("<iframe") {
        out.push_str(&rest[..at]);
        let tail = &rest[at..];
        let open_end = tail.find('>').map_or(tail.len(), |i| i + 1);
        let close = tail
            .find("</iframe>")
            .map_or(tail.len(), |i| i + "</iframe>".len());
        let keeps = tail[..open_end].contains(" src=\"");
        if keeps {
            out.push_str(&tail[..close.max(open_end)]);
        }
        rest = &tail[close.max(open_end)..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clean(s: &str) -> String {
        sanitize(s)
    }

    #[test]
    fn scripts_and_handlers_go() {
        let out = clean(
            "<p onclick=\"alert(1)\" onmouseover='x()'>hi<script>alert(1)</script></p>\
             <img src=\"https://example.com/a.png\" onerror=\"alert(2)\">\
             <style>body{display:none}</style><SCRIPT SRC=//evil.example.com/x.js></SCRIPT>\
             <svg onload=alert(3)><circle/></svg><math><mi>x</mi></math>",
        );
        assert!(!out.to_lowercase().contains("script"), "{out}");
        assert!(
            !out.contains("onclick") && !out.contains("onmouseover"),
            "{out}"
        );
        assert!(!out.contains("onerror") && !out.contains("alert"), "{out}");
        assert!(
            !out.contains("display:none") && !out.contains("svg"),
            "{out}"
        );
        assert!(out.contains("<p>hi</p>"), "{out}");
        assert!(out.contains("src=\"https://example.com/a.png\""), "{out}");
    }

    #[test]
    fn javascript_and_data_links_go() {
        for bad in [
            "<a href=\"javascript:alert(1)\">x</a>",
            "<a href=\" JaVaScRiPt:alert(1)\">x</a>",
            "<a href=\"jav&#x09;ascript:alert(1)\">x</a>",
            "<a href=\"vbscript:msgbox(1)\">x</a>",
            "<a href=\"data:text/html,<script>alert(1)</script>\">x</a>",
            "<a href=\"file:///etc/passwd\">x</a>",
        ] {
            let out = clean(bad);
            assert!(!out.contains("href"), "{bad} → {out}");
            assert!(out.contains(">x</a>"), "{bad} → {out}");
        }
        let img = clean(
            "<img src=\"javascript:alert(1)\"><img src=\"data:image/svg+xml,<svg onload=alert(1)>\">",
        );
        assert!(!img.contains("src="), "{img}");
        // Inline raster images and ordinary links stay.
        let ok = clean(
            "<img src=\"data:image/png;base64,iVBORw0KGgo=\"><a href=\"https://example.com/x\">y</a>\
             <a href=\"f/01k2abc/\">rel</a><a href=\"mailto:someone@example.com\">m</a>",
        );
        assert!(
            ok.contains("src=\"data:image/png;base64,iVBORw0KGgo=\""),
            "{ok}"
        );
        assert!(ok.contains("href=\"https://example.com/x\""), "{ok}");
        assert!(ok.contains("href=\"f/01k2abc/\""), "{ok}");
        assert!(ok.contains("href=\"mailto:someone@example.com\""), "{ok}");
        assert!(ok.contains("rel=\"noopener noreferrer\""), "{ok}");
    }

    #[test]
    fn iframes_only_for_youtube_nocookie_embeds() {
        let out = clean(
            "<p>a</p><iframe src=\"https://evil.example.com/x\"></iframe>\
             <iframe src=\"https://www.youtube.com/embed/Qa1b2C3d4E5\"></iframe>\
             <iframe src=\"https://www.youtube-nocookie.com/embed/../../evil\"></iframe>\
             <iframe srcdoc=\"<script>alert(1)</script>\"></iframe><p>b</p>",
        );
        assert_eq!(out, "<p>a</p><p>b</p>");
        let yt = clean(
            "<iframe src=\"https://www.youtube-nocookie.com/embed/Qa1b2C3d4E5?autoplay=1\" \
             allow=\"autoplay; encrypted-media\" allowfullscreen onload=\"x()\"></iframe>",
        );
        assert!(
            yt.contains("src=\"https://www.youtube-nocookie.com/embed/Qa1b2C3d4E5?autoplay=1\""),
            "{yt}"
        );
        assert!(!yt.contains("onload"), "{yt}");
    }

    #[test]
    fn forms_objects_and_embeds_go() {
        let out = clean(
            "<form action=\"https://evil.example.com/\"><input name=q><button>Go</button></form>\
             <object data=\"x.swf\"><param name=a value=b>fallback</object>\
             <embed src=\"x.swf\"><base href=\"https://evil.example.com/\">\
             <meta http-equiv=\"refresh\" content=\"0;url=https://evil.example.com/\">\
             <link rel=stylesheet href=\"https://evil.example.com/x.css\">",
        );
        for bad in [
            "form", "input", "object", "embed", "base", "meta", "link", "evil",
        ] {
            assert!(!out.contains(bad), "{bad}: {out}");
        }
    }

    #[test]
    fn the_youtube_facade_and_protocol_markup_survive() {
        let facade = blyg_render::youtube_facade_html(
            "Qa1b2C3d4E5",
            "https://www.youtube.com/watch?v=Qa1b2C3d4E5",
        );
        let quote = "<blockquote class=\"blyg-transclusion\" data-blyg-id=\"01k2abc\" \
                     data-blyg-version=\"3\" data-blyg-origin=\"https://ada.blyg.example.com/\">\
                     <p>Quoted <span class=\"blyg-tk-gen\">words</span>.</p></blockquote>";
        let out = clean(&format!("{quote}{facade}"));
        assert!(out.contains("class=\"blyg-transclusion\""), "{out}");
        assert!(out.contains("data-blyg-id=\"01k2abc\""), "{out}");
        assert!(out.contains("data-blyg-version=\"3\""), "{out}");
        assert!(
            out.contains("data-blyg-origin=\"https://ada.blyg.example.com/\""),
            "{out}"
        );
        assert!(out.contains("<span class=\"blyg-tk-gen\">"), "{out}");
        assert!(
            out.contains("<figure class=\"blyg-yt\" data-ytid=\"Qa1b2C3d4E5\">"),
            "{out}"
        );
        assert!(out.contains("class=\"blyg-yt-poster\""), "{out}");
        assert!(
            out.contains("src=\"https://i.ytimg.com/vi/Qa1b2C3d4E5/hqdefault.jpg\""),
            "{out}"
        );
        assert!(out.contains("<figcaption>"), "{out}");
    }

    #[test]
    fn images_never_send_a_referrer() {
        let out = clean(
            "<p><img src=\"media/a.png\" alt=\"A\"></p>\
             <p><img src=\"https://cdn.example.com/b.jpg\" referrerpolicy=\"unsafe-url\"></p>",
        );
        assert_eq!(out.matches("<img").count(), 2, "{out}");
        assert_eq!(
            out.matches("referrerpolicy=\"no-referrer\"").count(),
            2,
            "{out}"
        );
        assert!(!out.contains("unsafe-url"), "{out}");
        assert!(out.contains("src=\"media/a.png\""), "relative kept: {out}");
    }

    #[test]
    fn relative_urls_resolve_against_the_posts_origin() {
        let html = r#"<p><img src="/blyg/media/a.png" alt=""> <a href="t/X/">x</a> <img src="https://cdn.example.org/b.png" alt=""></p>"#;
        let out = sanitize_at(html, "https://ada.example.net/blyg");
        assert!(
            out.contains(r#"src="https://ada.example.net/blyg/media/a.png""#),
            "{out}"
        );
        assert!(
            out.contains(r#"href="https://ada.example.net/blyg/t/X/""#),
            "{out}"
        );
        assert!(
            out.contains(r#"src="https://cdn.example.org/b.png""#),
            "{out}"
        );
        // Still the same allowlist.
        assert!(
            !sanitize_at("<script>x</script><p>ok</p>", "https://ada.example.net/")
                .contains("script")
        );
        // No usable base: relative URLs are left as they are.
        assert!(sanitize_at(html, "not a url").contains(r#"src="/blyg/media/a.png""#));
    }

    #[test]
    fn formulas_are_redrawn_never_passed_through() {
        let tex = |t: &str| {
            format!(
                "<math display=\"block\"><semantics><mi>x</mi><annotation encoding=\"application/x-tex\">{t}</annotation></semantics></math>"
            )
        };
        let out = sanitize_at(
            &format!("<p>Before</p>{}<p>after</p>", tex("\\frac{a}{b}")),
            "https://ada.blyg.example.com/",
        );
        assert!(out.starts_with("<p>Before</p>"), "{out}");
        assert!(out.ends_with("<p>after</p>"), "{out}");
        assert!(out.contains("frac{a}{b}"), "the formula survives: {out}");
        // Hostile TeX and markup inside the formula never become markup.
        for hostile in [
            tex("x&lt;/annotation&gt;&lt;script&gt;alert(1)&lt;/script&gt;"),
            tex("x\" onmouseover=\"alert(1)"),
            "<math><annotation encoding=\"application/x-tex\">x</annotation><script>alert(1)</script><mi onclick=\"alert(2)\">y</mi><a href=\"javascript:alert(3)\">z</a></math>".into(),
            "<math><mi onclick=\"alert(1)\">x</mi><script>alert(2)</script></math>".into(),
            "<p title=\"<math><annotation encoding='application/x-tex'>t</annotation></math>\">p</p>".into(),
        ] {
            let out = sanitize(&hostile);
            // Only our own drawing, on the allowlist, ever reaches the page
            // (its annotation holds the TeX as text, quotes and all).
            let mut outside = String::new();
            let mut rest = out.as_str();
            while let Some(at) = rest.find("<math") {
                let end = rest[at..].find("</math>").expect("closed") + at + 7;
                assert!(blyg_render::math::allowed_math(&rest[at..end]), "{hostile} → {out}");
                outside.push_str(&rest[..at]);
                rest = &rest[end..];
            }
            outside.push_str(rest);
            assert!(!out.contains("<script"), "{hostile} → {out}");
            assert!(
                !outside.contains("onmouseover=") && !outside.contains("onclick="),
                "{hostile} → {out}"
            );
            assert!(!out.contains("javascript:"), "{hostile} → {out}");
            assert!(!out.contains(">y<") && !out.contains(">z<"), "{hostile} → {out}");
        }
        // No annotation: the formula is dropped.
        assert_eq!(sanitize("<p>a<math><mi>x</mi></math>b</p>"), "<p>ab</p>");
    }

    #[test]
    fn no_ids_or_inline_styles() {
        // No DOM clobbering of the host script's globals, no CSS tricks.
        let out = clean("<p id=\"__blyg\" name=\"x\" style=\"position:fixed;inset:0\">a</p>");
        assert_eq!(out, "<p>a</p>");
    }

    #[test]
    fn the_apps_own_hooks_never_survive() {
        // The pages this HTML is shown on listen for their own `data-*`
        // names and classes (the web app's `data-goto`, a link's
        // `data-wr-id`/`data-wr-origin` opening Peek, native's
        // `.blyg-qorigin` and `[data-line]`) and style their own classes as
        // buttons. Someone else's post carries none of them.
        let out = clean(
            "<p data-goto=\"posts\" data-act=\"delete\" data-line=\"3\" data-x=\"1\" \
             class=\"wr-btn danger item-content card\">a</p>\
             <a href=\"https://far.example.org/\" data-wr-id=\"X1\" \
             data-wr-origin=\"https://evil.example/\" class=\"blyg-qorigin\">b</a>\
             <span class=\"blyg-tk-gen wr-btn\" data-blyg-id=\"01k\">c</span>",
        );
        for bad in [
            "data-goto",
            "data-act",
            "data-line",
            "data-x",
            "data-wr-",
            "wr-btn",
            "danger",
            "item-content",
            "card",
            "blyg-qorigin",
            "class=\"\"",
        ] {
            assert!(!out.contains(bad), "{bad}: {out}");
        }
        assert!(out.contains("<p>a</p>"), "{out}");
        assert!(
            out.contains("<span class=\"blyg-tk-gen\" data-blyg-id=\"01k\">c</span>"),
            "{out}"
        );
    }

    /// Every class and `data-*` name the renderer puts in a post (the
    /// studio's, which blyg-render's parity fixtures hold) is kept, but
    /// for an author's own raw-HTML classes.
    #[test]
    fn what_the_renderer_emits_survives() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../blyg-render/tests/fixtures/parity");
        let mut seen = 0;
        for f in std::fs::read_dir(&dir).unwrap() {
            let path = f.unwrap().path();
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            if name.starts_with('_') || name == "tr_sanitize_target.json" {
                continue;
            }
            let fixture: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
            let Some(html) = fixture["html"].as_str() else {
                continue;
            };
            let out = clean(html);
            for (tag, attr, value) in attrs(html) {
                if tag == "math" || value.starts_with("tml-") {
                    continue; // redrawn from its TeX annotation
                }
                if attr == "class" {
                    for c in value.split_whitespace() {
                        assert!(out.contains(c), "{name}: class {c} went: {out}");
                    }
                    seen += 1;
                } else if attr.starts_with("data-") {
                    assert!(out.contains(&attr), "{name}: {attr} went: {out}");
                    seen += 1;
                }
            }
        }
        assert!(seen > 100, "the fixtures hold classes: {seen}");
    }

    /// The classes only the page shell writes (`blyg_render::article_html`
    /// and the stub citation): put round the body after it's cleaned, never
    /// in a post's own HTML.
    const SHELL_CLASSES: [&str; 6] = [
        "compact",
        "fragment",
        "item-content",
        "label",
        "stub-cite",
        "thread",
    ];

    /// Every class blyg-render writes into a post's body survives on its
    /// own (no substring luck); the shell's are written only by the shell
    /// and appear in no studio body.
    #[test]
    fn every_class_the_renderer_writes_in_a_body_survives() {
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../blyg-render/src");
        let mut body = std::collections::BTreeSet::new();
        for f in std::fs::read_dir(&src).unwrap() {
            let path = f.unwrap().path();
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            let text = std::fs::read_to_string(&path).unwrap();
            for chunk in text.split("class=\\\"").skip(1) {
                let Some(end) = chunk.find("\\\"") else { continue };
                let value = &chunk[..end];
                if value.contains('{') || value.contains('"') {
                    continue; // built from a variable; its values are literals elsewhere
                }
                for c in value.split_whitespace() {
                    if name == "shell.rs" {
                        assert!(SHELL_CLASSES.contains(&c), "shell.rs writes {c}: list it");
                    } else {
                        body.insert(c.to_string());
                    }
                }
            }
        }
        assert!(body.len() > 15, "{body:?}");
        for c in &body {
            assert!(
                known_classes(c).as_deref() == Some(c.as_str()),
                "the renderer writes class {c} in a body; the sanitizer drops it"
            );
            let out = clean(&format!("<p class=\"{c}\">x</p>"));
            assert_eq!(out, format!("<p class=\"{c}\">x</p>"));
        }
        // And no studio body (the parity fixtures) carries a shell class.
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../blyg-render/tests/fixtures/parity");
        for f in std::fs::read_dir(&dir).unwrap() {
            let path = f.unwrap().path();
            let fixture: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
            let Some(html) = fixture["html"].as_str() else {
                continue;
            };
            for (_, attr, value) in attrs(html) {
                if attr == "class" {
                    for c in value.split_whitespace() {
                        assert!(!SHELL_CLASSES.contains(&c), "{path:?}: a body with {c}");
                    }
                }
            }
        }
    }

    /// `(tag, attribute, value)` for each double-quoted attribute.
    fn attrs(html: &str) -> Vec<(String, String, String)> {
        let mut out = Vec::new();
        for open in html.split('<').skip(1) {
            let Some(end) = open.find('>') else { continue };
            let mut parts = open[..end].splitn(2, char::is_whitespace);
            let tag = parts.next().unwrap_or("").to_string();
            let mut rest = parts.next().unwrap_or("");
            while let Some(eq) = rest.find("=\"") {
                let name = rest[..eq].trim().to_string();
                let after = &rest[eq + 2..];
                let Some(close) = after.find('"') else { break };
                out.push((tag.clone(), name, after[..close].to_string()));
                rest = &after[close + 1..];
            }
        }
        out
    }
}
