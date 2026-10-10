//! --- capture --- Clip Page (the pane's "✂ Clip", ⇧⌘C in the pane, Post ›
//! Clip Page to Draft): the passage selected on the page, else its readable
//! main content, as Markdown, quoted into the draft with the page's link.
//!
//! [`CAPTURE_JS`] reads the page (through `BrowserSurface::eval_json`, apart
//! from the page's own scripts): the address, the title (`og:title` first),
//! `<link rel=canonical>`, `<meta name=author>`, the selection, and the HTML
//! of the article (`article`, `main`, `[role=main]`, else the block with the
//! most paragraph text). [`html_to_markdown`] (pure) turns that into
//! Markdown through an ammonia allowlist. The result is a [`PageCapture`],
//! the same thing `burrow/browser.page` answers an extension with: never
//! cookies, storage or headers.

use std::collections::{HashMap, HashSet};

use blyg_core::{FRAGMENT_LIMIT, Kind, published_len};
use blyg_ext::protocol::PageCapture;
use gpui_kit::*;
use serde::Deserialize;

use crate::app::MainView;

/// The longest Markdown a capture keeps (characters).
pub const MAX_MARKDOWN: usize = 20_000;

/// How long the page gets to answer.
const CAPTURE_WAIT: std::time::Duration = std::time::Duration::from_secs(3);

/// Reads the page; evaluates to a JSON string ([`parse_capture`]).
pub const CAPTURE_JS: &str = r#"(function () {
  "use strict";
  const abs = (u) => { try { return new URL(u, location.href).href; } catch (e) { return ""; } };
  const meta = (sel) => {
    const m = document.querySelector(sel);
    return m ? (m.getAttribute("content") || "").trim() : "";
  };
  let selection = "", selectionHtml = "";
  const sel = window.getSelection();
  if (sel && sel.rangeCount && !sel.isCollapsed) {
    selection = sel.toString();
    const box = document.createElement("div");
    for (let i = 0; i < sel.rangeCount; i++) box.appendChild(sel.getRangeAt(i).cloneContents());
    selectionHtml = box.innerHTML;
  }
  let main = document.querySelector("article") || document.querySelector("main")
    || document.querySelector("[role=main]");
  if (!main) {
    let best = null, score = 0;
    for (const el of document.querySelectorAll("div, section")) {
      let n = 0;
      for (const p of el.querySelectorAll(":scope > p")) n += (p.innerText || "").length;
      if (n > score) { score = n; best = el; }
    }
    main = best || document.body;
  }
  const canon = document.querySelector("link[rel=canonical]");
  return JSON.stringify({
    url: location.href,
    title: meta("meta[property='og:title']") || document.title || "",
    canonical: canon ? abs(canon.getAttribute("href") || "") : "",
    author: meta("meta[name=author]") || meta("meta[property='article:author']"),
    selection: selection,
    selectionHtml: selectionHtml.slice(0, 400000),
    html: main ? main.innerHTML.slice(0, 400000) : ""
  });
})()"#;

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct Raw {
    url: String,
    title: String,
    canonical: String,
    author: String,
    selection: String,
    selection_html: String,
    html: String,
}

/// A page read by [`CAPTURE_JS`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Captured {
    pub page: PageCapture,
    /// The selection as Markdown (empty when nothing is selected).
    pub selection_markdown: String,
}

impl Captured {
    /// What Clip Page quotes: the selection, else the article.
    pub fn clip_markdown(&self) -> &str {
        if self.selection_markdown.trim().is_empty() {
            &self.page.markdown
        } else {
            &self.selection_markdown
        }
    }

    /// The link the quote cites: the canonical URL, else the address.
    pub fn link(&self) -> &str {
        self.page.canonical_url.as_deref().unwrap_or(&self.page.url)
    }
}

/// [`CAPTURE_JS`]'s answer (`None` for no page or garbage).
pub fn parse_capture(json: &str) -> Option<Captured> {
    let raw: Raw = serde_json::from_str::<Option<Raw>>(json).ok().flatten()?;
    if !super::is_web_url(&raw.url) {
        return None;
    }
    let canonical = Some(raw.canonical.trim().to_string()).filter(|c| super::is_web_url(c));
    let base = canonical.as_deref().unwrap_or(&raw.url);
    let selection_markdown = if raw.selection.trim().is_empty() {
        String::new()
    } else {
        let md = html_to_markdown(&raw.selection_html, Some(base));
        if md.trim().is_empty() {
            raw.selection.trim().to_string()
        } else {
            md
        }
    };
    Some(Captured {
        page: PageCapture {
            url: raw.url.clone(),
            canonical_url: canonical.clone(),
            title: raw.title.trim().to_string(),
            selection: raw.selection.trim().to_string(),
            markdown: html_to_markdown(&raw.html, Some(base)),
            author: Some(raw.author.trim().to_string()).filter(|a| !a.is_empty()),
        },
        selection_markdown,
    })
}

// ------------------------------------------------------------ HTML → Markdown

const TAGS: &[&str] = &[
    "p",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "ul",
    "ol",
    "li",
    "a",
    "blockquote",
    "pre",
    "code",
    "em",
    "strong",
    "b",
    "i",
    "br",
    "img",
];

/// Elements dropped with everything inside them.
const DROP: &[&str] = &[
    "script", "style", "noscript", "template", "svg", "nav", "footer", "aside", "form", "button",
    "iframe", "select", "textarea", "head", "title",
];

/// The readable part of `html` as Markdown: paragraphs, headings, lists,
/// links, quotes, code, emphasis; an image becomes a link to it. Relative
/// links resolve against `base`. At most [`MAX_MARKDOWN`] characters.
pub fn html_to_markdown(html: &str, base: Option<&str>) -> String {
    let mut b = ammonia::Builder::empty();
    b.tags(TAGS.iter().copied().collect())
        .clean_content_tags(DROP.iter().copied().collect())
        .generic_attributes(HashSet::new())
        .tag_attributes(HashMap::from([
            ("a", HashSet::from(["href"])),
            ("img", HashSet::from(["src", "alt"])),
        ]))
        .url_schemes(HashSet::from(["http", "https", "mailto"]))
        .link_rel(None)
        .strip_comments(true);
    match base.and_then(|u| url::Url::parse(u).ok()) {
        Some(u) => b.url_relative(ammonia::UrlRelative::RewriteWithBase(u)),
        None => b.url_relative(ammonia::UrlRelative::Deny),
    };
    let clean = b.clean(html).to_string();
    let tree = parse(&clean);
    let md = blocks(&tree).join("\n\n");
    cap(md.trim(), MAX_MARKDOWN)
}

fn cap(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let head: String = s.chars().take(max.saturating_sub(1)).collect();
    // On a paragraph boundary when there's one in the last quarter.
    let cut = head
        .rfind("\n\n")
        .filter(|&i| i > head.len() * 3 / 4)
        .unwrap_or(head.len());
    format!("{}…", head[..cut].trim_end())
}

/// An element still open while parsing: its tag, attributes and children.
type Open = (String, Vec<(String, String)>, Vec<Node>);

#[derive(Debug)]
enum Node {
    Text(String),
    El {
        tag: String,
        attrs: Vec<(String, String)>,
        kids: Vec<Node>,
    },
}

impl Node {
    fn attr(&self, name: &str) -> Option<&str> {
        match self {
            Node::El { attrs, .. } => attrs
                .iter()
                .find(|(k, _)| k == name)
                .map(|(_, v)| v.as_str()),
            Node::Text(_) => None,
        }
    }
}

fn decode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let Some(end) = rest[..rest.len().min(12)].find(';') else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let name = &rest[1..end];
        let c = match name {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" | "#39" => Some('\''),
            "nbsp" => Some('\u{a0}'),
            n if n.starts_with("#x") || n.starts_with("#X") => u32::from_str_radix(&n[2..], 16)
                .ok()
                .and_then(char::from_u32),
            n if n.starts_with('#') => n[1..].parse().ok().and_then(char::from_u32),
            _ => None,
        };
        match c {
            Some(c) => {
                out.push(c);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// A tree of ammonia's output (well formed: quoted attributes, closed
/// elements, `br` and `img` void).
fn parse(html: &str) -> Vec<Node> {
    let mut stack: Vec<Open> = vec![(String::new(), vec![], vec![])];
    let mut rest = html;
    while !rest.is_empty() {
        let Some(lt) = rest.find('<') else {
            push_text(&mut stack, rest);
            break;
        };
        if lt > 0 {
            push_text(&mut stack, &rest[..lt]);
        }
        rest = &rest[lt..];
        let Some(gt) = tag_end(rest) else {
            push_text(&mut stack, rest);
            break;
        };
        let inner = &rest[1..gt];
        rest = &rest[gt + 1..];
        if let Some(name) = inner.strip_prefix('/') {
            let name = name.trim().to_ascii_lowercase();
            if let Some(pos) = stack.iter().rposition(|(t, _, _)| *t == name)
                && pos > 0
            {
                while stack.len() > pos {
                    let (tag, attrs, kids) = stack.pop().expect("non-empty");
                    let el = Node::El { tag, attrs, kids };
                    stack.last_mut().expect("root").2.push(el);
                }
            }
            continue;
        }
        let inner = inner.trim_end_matches('/');
        let (name, attrs) = split_tag(inner);
        if matches!(name.as_str(), "br" | "img") {
            stack.last_mut().expect("root").2.push(Node::El {
                tag: name,
                attrs,
                kids: vec![],
            });
        } else if !name.is_empty() && !name.starts_with('!') {
            stack.push((name, attrs, vec![]));
        }
    }
    while stack.len() > 1 {
        let (tag, attrs, kids) = stack.pop().expect("non-empty");
        stack
            .last_mut()
            .expect("root")
            .2
            .push(Node::El { tag, attrs, kids });
    }
    stack.pop().map(|r| r.2).unwrap_or_default()
}

fn push_text(stack: &mut [Open], t: &str) {
    if let Some(top) = stack.last_mut() {
        top.2.push(Node::Text(decode(t)));
    }
}

/// The `>` that ends the tag at the start of `s` (quotes respected).
fn tag_end(s: &str) -> Option<usize> {
    let mut quote: Option<char> = None;
    for (i, c) in s.char_indices().skip(1) {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (None, '"' | '\'') => quote = Some(c),
            (None, '>') => return Some(i),
            _ => {}
        }
    }
    None
}

fn split_tag(inner: &str) -> (String, Vec<(String, String)>) {
    let inner = inner.trim();
    let name_end = inner
        .find(|c: char| c.is_whitespace())
        .unwrap_or(inner.len());
    let name = inner[..name_end].to_ascii_lowercase();
    let mut attrs = vec![];
    let mut rest = inner[name_end..].trim_start();
    while !rest.is_empty() {
        let key_end = rest
            .find(|c: char| c == '=' || c.is_whitespace())
            .unwrap_or(rest.len());
        let key = rest[..key_end].to_ascii_lowercase();
        rest = rest[key_end..].trim_start();
        let mut value = String::new();
        if let Some(r) = rest.strip_prefix('=') {
            let r = r.trim_start();
            if let Some(q) = r.chars().next().filter(|c| *c == '"' || *c == '\'') {
                let body = &r[1..];
                let end = body.find(q).unwrap_or(body.len());
                value = decode(&body[..end]);
                rest = body.get(end + 1..).unwrap_or("").trim_start();
            } else {
                let end = r.find(char::is_whitespace).unwrap_or(r.len());
                value = decode(&r[..end]);
                rest = r[end..].trim_start();
            }
        }
        if !key.is_empty() {
            attrs.push((key, value));
        }
    }
    (name, attrs)
}

fn is_block(tag: &str) -> bool {
    matches!(
        tag,
        "p" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "ul" | "ol" | "li" | "blockquote" | "pre"
    )
}

/// The Markdown blocks of `nodes` (inline runs between blocks become
/// paragraphs).
fn blocks(nodes: &[Node]) -> Vec<String> {
    let mut out = vec![];
    let mut run = String::new();
    let flush = |run: &mut String, out: &mut Vec<String>| {
        let p = paragraph(run);
        if !p.is_empty() {
            out.push(p);
        }
        run.clear();
    };
    for n in nodes {
        match n {
            Node::El { tag, kids, .. } if is_block(tag) => {
                flush(&mut run, &mut out);
                match tag.as_str() {
                    "p" => {
                        let p = paragraph(&inline_all(kids));
                        if !p.is_empty() {
                            out.push(p);
                        }
                    }
                    h if h.starts_with('h') => {
                        let level = h[1..].parse::<usize>().unwrap_or(1).clamp(1, 6);
                        let text = paragraph(&inline_all(kids)).replace('\n', " ");
                        if !text.is_empty() {
                            out.push(format!("{} {text}", "#".repeat(level)));
                        }
                    }
                    "ul" | "ol" => {
                        let list = list(tag == "ol", kids);
                        if !list.is_empty() {
                            out.push(list);
                        }
                    }
                    "li" => {
                        let item = blocks(kids).join("\n\n");
                        if !item.trim().is_empty() {
                            out.push(indent_item("- ", &item));
                        }
                    }
                    "blockquote" => {
                        let inner = blocks(kids).join("\n\n");
                        if !inner.trim().is_empty() {
                            out.push(
                                inner
                                    .lines()
                                    .map(|l| {
                                        if l.is_empty() {
                                            ">".to_string()
                                        } else {
                                            format!("> {l}")
                                        }
                                    })
                                    .collect::<Vec<_>>()
                                    .join("\n"),
                            );
                        }
                    }
                    "pre" => {
                        let code = text_of(kids);
                        let code = code.trim_matches('\n');
                        if !code.trim().is_empty() {
                            let fence = if code.contains("```") { "~~~" } else { "```" };
                            out.push(format!("{fence}\n{code}\n{fence}"));
                        }
                    }
                    _ => {}
                }
            }
            n => run.push_str(&inline(n)),
        }
    }
    flush(&mut run, &mut out);
    out
}

fn list(ordered: bool, kids: &[Node]) -> String {
    let mut items = vec![];
    let mut n = 0;
    for k in kids {
        let body = match k {
            Node::El { tag, kids, .. } if tag == "li" => blocks(kids).join("\n\n"),
            Node::Text(t) if t.trim().is_empty() => continue,
            other => blocks(std::slice::from_ref(other)).join("\n\n"),
        };
        if body.trim().is_empty() {
            continue;
        }
        n += 1;
        let marker = if ordered {
            format!("{n}. ")
        } else {
            "- ".to_string()
        };
        items.push(indent_item(&marker, &body));
    }
    items.join("\n")
}

/// `marker` before the first line, the rest indented to line up.
fn indent_item(marker: &str, body: &str) -> String {
    let pad = " ".repeat(marker.chars().count());
    body.lines()
        .enumerate()
        .map(|(i, l)| {
            if i == 0 {
                format!("{marker}{l}")
            } else if l.is_empty() {
                String::new()
            } else {
                format!("{pad}{l}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn inline_all(nodes: &[Node]) -> String {
    nodes.iter().map(inline).collect()
}

/// Inline Markdown (`\n` only for `<br>`).
fn inline(n: &Node) -> String {
    match n {
        Node::Text(t) => escape(&t.replace(['\n', '\r', '\t'], " ")),
        Node::El { tag, kids, .. } => match tag.as_str() {
            "br" => "\n".into(),
            "em" | "i" => wrap("*", &inline_all(kids)),
            "strong" | "b" => wrap("**", &inline_all(kids)),
            "code" => {
                let t = text_of(kids).replace('\n', " ");
                if t.trim().is_empty() {
                    String::new()
                } else if t.contains('`') {
                    format!("`` {t} ``")
                } else {
                    format!("`{t}`")
                }
            }
            "a" => {
                let text = collapse(&inline_all(kids).replace('\n', " "));
                match n.attr("href").filter(|h| !h.is_empty()) {
                    Some(href) => {
                        let dest = destination(href);
                        if text.is_empty() {
                            format!("<{href}>")
                        } else {
                            format!("[{text}]({dest})")
                        }
                    }
                    None => text,
                }
            }
            "img" => match n.attr("src").filter(|s| !s.is_empty()) {
                Some(src) => {
                    let alt = n.attr("alt").map(collapse).unwrap_or_default();
                    let alt = if alt.is_empty() {
                        "image".to_string()
                    } else {
                        escape(&alt)
                    };
                    format!("[{alt}]({})", destination(src))
                }
                None => String::new(),
            },
            // A block inside an inline (rare after cleaning): its text.
            _ => inline_all(kids),
        },
    }
}

fn wrap(mark: &str, s: &str) -> String {
    let t = s.trim();
    if t.is_empty() {
        return s.to_string();
    }
    let lead = if s.starts_with(' ') { " " } else { "" };
    let trail = if s.ends_with(' ') { " " } else { "" };
    format!("{lead}{mark}{t}{mark}{trail}")
}

fn destination(url: &str) -> String {
    url.replace(' ', "%20")
        .replace('(', "%28")
        .replace(')', "%29")
}

/// Raw text (code, `pre`).
fn text_of(nodes: &[Node]) -> String {
    let mut s = String::new();
    for n in nodes {
        match n {
            Node::Text(t) => s.push_str(t),
            Node::El { tag, .. } if tag == "br" => s.push('\n'),
            Node::El { kids, .. } => s.push_str(&text_of(kids)),
        }
    }
    s.replace('\u{a0}', " ")
}

fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if matches!(c, '\\' | '*' | '_' | '[' | ']' | '`' | '<') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

fn collapse(s: &str) -> String {
    s.split(|c: char| c.is_whitespace() || c == '\u{a0}')
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// An inline run as a paragraph: spaces collapsed per line, and a line
/// that would read as a heading, quote or list marker escaped.
fn paragraph(run: &str) -> String {
    run.split('\n')
        .map(collapse)
        .filter(|l| !l.is_empty())
        .map(|l| {
            let first = l.split(' ').next().unwrap_or("");
            let numbered = first.len() > 1
                && first.ends_with('.')
                && first[..first.len() - 1].chars().all(|c| c.is_ascii_digit());
            if l.starts_with(['#', '>', '-', '+']) {
                format!("\\{l}")
            } else if numbered {
                l.replacen('.', "\\.", 1)
            } else {
                l
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

// ------------------------------------------------------------ the app side

impl MainView {
    /// Read the page open in the pane (`None` when there's none, or it
    /// didn't answer). What `burrow/browser.page` answers an extension with.
    pub(crate) fn browser_capture(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Option<async_channel::Receiver<Option<Captured>>> {
        if !self.browser.open || !super::is_web_url(&self.browser.page.url) {
            return None;
        }
        let (tx, rx) = async_channel::bounded::<String>(2);
        self.browser.with(|s| s.eval_json(CAPTURE_JS, tx.clone()))?;
        let (out_tx, out_rx) = async_channel::bounded(1);
        cx.spawn(async move |_, cx| {
            let t = tx.clone();
            cx.background_executor()
                .spawn({
                    let timer = cx.background_executor().timer(CAPTURE_WAIT);
                    async move {
                        timer.await;
                        let _ = t.try_send("null".into());
                    }
                })
                .detach();
            drop(tx);
            let json = rx.recv().await.unwrap_or_default();
            let _ = out_tx.try_send(parse_capture(&json));
        })
        .detach();
        Some(out_rx)
    }

    /// ✂ Clip / ⇧⌘C / Post › Clip Page to Draft.
    pub(crate) fn browser_clip(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // --- onboarding --- the tour's clip step, on its sample page.
        if self.onboarding.tutorial.is_some() {
            self.tutorial_key(crate::app::onboarding::Key::Clip, window, cx);
            if self.browser_tour_page() {
                let block = self.browser_tour_clip();
                return self.clip_into_draft(block, window, cx);
            }
        }
        let Some(rx) = self.browser_capture(cx) else {
            return self.show_toast("Open a page in the browser pane to clip it", None, cx);
        };
        cx.spawn_in(window, async move |this, cx| {
            let got = rx.recv().await.ok().flatten();
            let _ = this.update_in(cx, |v, window, cx| match got {
                None => v.show_toast("The page didn't answer; try again", None, cx),
                Some(c) => {
                    let md = c.clip_markdown().trim().to_string();
                    if md.is_empty() {
                        return v.show_toast("Nothing to clip on this page", None, cx);
                    }
                    let block =
                        crate::app::notes::quote_with_source(&md, &c.page.title, Some(c.link()));
                    v.clip_into_draft(block, window, cx);
                }
            });
        })
        .detach();
    }

    /// The quote's target rules (`selection_target`): the open draft at its
    /// caret, else a new draft starting with an empty line (a thread when
    /// it's over a fragment's length).
    fn clip_into_draft(&mut self, block: String, window: &mut Window, cx: &mut Context<Self>) {
        let target = self.selection_target();
        self.close_browser(window, cx);
        match target {
            Some(item) => {
                let (text, cursor) = {
                    let s = self.editor.read(cx);
                    (s.value().to_string(), s.cursor())
                };
                self.leave_reading();
                self.back_to_search(window, cx);
                self.open(&item.local_id, window, cx);
                let now = self.editor.read(cx).value().to_string();
                let cursor = if now == text { cursor } else { now.len() };
                let (new_text, caret) = crate::app::reading::insert_block(&now, cursor, &block);
                self.splice_editor(&now, &new_text, Some(caret), window, cx);
                self.show_toast(
                    format!("Clipped into “{}”", crate::vm::item_title(&item)),
                    None,
                    cx,
                );
            }
            None => {
                let kind = if published_len(&block) > FRAGMENT_LIMIT {
                    Kind::Thread
                } else {
                    Kind::Fragment
                };
                match self.backend.create_draft(kind, &format!("\n{block}\n")) {
                    Ok(id) => {
                        self.open_new_draft(&id, window, cx);
                        self.show_toast("New draft with the clipped page", None, cx);
                    }
                    Err(e) => self.show_toast(format!("Couldn't start a draft: {e}"), None, cx),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    // Not `super::*`: gpui's `test` attribute would shadow the standard one.
    use super::{MAX_MARKDOWN, html_to_markdown, parse_capture};

    #[test]
    fn paragraphs_headings_and_emphasis() {
        let md = html_to_markdown(
            "<h1>Tide  tables</h1><p>The <em>sea</em> is <strong>slow</strong> &amp; patient.</p>\
             <p>Second\nline</p>",
            None,
        );
        assert_eq!(
            md,
            "# Tide tables\n\nThe *sea* is **slow** & patient.\n\nSecond line"
        );
    }

    #[test]
    fn links_resolve_and_images_become_links() {
        let md = html_to_markdown(
            "<p>See <a href=\"/f/abc/\" onclick=\"x()\">the log</a> and \
             <img src=\"img/pool.png\" alt=\"A pool\"> <img src=\"x.png\"></p>",
            Some("https://blyg.example.com/t/1/"),
        );
        assert_eq!(
            md,
            "See [the log](https://blyg.example.com/f/abc/) and \
             [A pool](https://blyg.example.com/t/1/img/pool.png) \
             [image](https://blyg.example.com/t/1/x.png)"
        );
        // No base: relative links are dropped, the text stays.
        assert_eq!(
            html_to_markdown("<p><a href=\"/x\">rel</a></p>", None),
            "rel"
        );
        // javascript: never survives.
        assert_eq!(
            html_to_markdown("<p><a href=\"javascript:alert(1)\">x</a></p>", None),
            "x"
        );
    }

    #[test]
    fn lists_quotes_and_code() {
        let md = html_to_markdown(
            "<ul><li>one</li><li>two <code>a*b</code></li></ul>\
             <ol><li>first</li><li><p>second</p><ul><li>nested</li></ul></li></ol>\
             <blockquote><p>quoted</p><p>twice</p></blockquote>\
             <pre><code>fn main() {\n    let x = 1 * 2;\n}</code></pre>",
            None,
        );
        assert_eq!(
            md,
            "- one\n- two `a*b`\n\n1. first\n2. second\n\n   - nested\n\n\
             > quoted\n>\n> twice\n\n```\nfn main() {\n    let x = 1 * 2;\n}\n```"
        );
    }

    #[test]
    fn scripts_styles_and_chrome_are_dropped_and_markdown_escaped() {
        let md = html_to_markdown(
            "<nav><a href=\"/\">Home</a></nav><style>p{}</style><script>evil()</script>\
             <p># not a heading, 2. not a list, *not* [a link] <b></b></p>\
             <div><p>kept</p></div><form><input value=x></form>",
            None,
        );
        assert_eq!(
            md,
            "\\# not a heading, 2. not a list, \\*not\\* \\[a link\\]\n\nkept"
        );
        assert_eq!(html_to_markdown("<p>1. first</p>", None), "1\\. first");
    }

    #[test]
    fn long_pages_are_capped() {
        let p = "word ".repeat(2000);
        let html = format!("<p>{p}</p>").repeat(5);
        let md = html_to_markdown(&html, None);
        assert!(md.chars().count() <= MAX_MARKDOWN);
        assert!(md.ends_with('…'));
    }

    #[test]
    fn a_capture_prefers_the_selection_and_the_canonical_link() {
        let json = r#"{"url": "https://news.example.com/story?utm=1",
            "title": "Harbour log", "canonical": "https://news.example.com/story",
            "author": " A. Gardener ", "selection": "the sea",
            "selectionHtml": "the <em>sea</em>", "html": "<p>The whole story.</p>"}"#;
        let c = parse_capture(json).unwrap();
        assert_eq!(c.clip_markdown(), "the *sea*");
        assert_eq!(c.link(), "https://news.example.com/story");
        assert_eq!(c.page.markdown, "The whole story.");
        assert_eq!(c.page.author.as_deref(), Some("A. Gardener"));
        assert_eq!(c.page.selection, "the sea");
        // No selection: the article; a bad canonical is ignored.
        let json = r#"{"url": "https://news.example.com/story", "title": "Harbour log",
            "canonical": "javascript:alert(1)", "html": "<p>The whole story.</p>"}"#;
        let c = parse_capture(json).unwrap();
        assert_eq!(c.clip_markdown(), "The whole story.");
        assert_eq!(c.page.canonical_url, None);
        assert_eq!(c.link(), "https://news.example.com/story");
        assert!(parse_capture("null").is_none());
        assert!(parse_capture(r#"{"url":"file:///etc/passwd"}"#).is_none());
    }

    #[test]
    fn the_draft_block_quotes_with_the_source() {
        let block = crate::app::notes::quote_with_source(
            "The whole story.\n\nSecond paragraph.",
            "Harbour log",
            Some("https://news.example.com/story"),
        );
        assert_eq!(
            block,
            "> The whole story.\n>\n> Second paragraph.\n>\n> — [Harbour log](https://news.example.com/story)"
        );
    }
}
