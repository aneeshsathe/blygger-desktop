//! --- studio 0.71 --- Reading a [TK] gap's links before Burrow's own AI
//! writes it: a port of the studio's `src/tk-hydrate.ts` (decision #117)
//! for the local providers (codex, claude_code, the CLIs, the APIs). The
//! blyg's own provider does this on the blyg (`generate_on_blyg`).
//!
//! The links the author already placed, in the instruction or anywhere in
//! the draft, are read, so the model writes from what they say rather than
//! from what their addresses suggest:
//!
//! - an inline `[[id]]` link is the item Burrow holds (your post, or one
//!   you read), never fetched;
//! - an http(s) URL naming an item Burrow holds, on that item's own blyg,
//!   is that item;
//! - any other http(s) URL is fetched, bounded: [`MAX_PAGES`] pages, each
//!   read for at most [`PAGE_TIMEOUT`] and [`PAGE_BYTES`], text and HTML
//!   only, no cookies or credentials. Its text is cut at [`PAGE_CHARS`]
//!   characters and goes to the model as untrusted material
//!   (`prompts::tk_user_content`), never rendered.
//!
//! Nothing is fed that wasn't read: a link that couldn't be read is a
//! [`Failure`], and the caller asks before generating without it (the
//! checkpoint).

use std::io::Read;
use std::time::Duration;

use serde_json::{Value, json};

/// At most this many web pages and this many linked items are read per
/// generation (`HYDRATE_MAX_PAGES`, `HYDRATE_MAX_ITEMS`).
pub const MAX_PAGES: usize = 8;
pub const MAX_ITEMS: usize = 8;
/// A page's text is cut to this many characters before it reaches the
/// model (`HYDRATE_PAGE_CHARS`; characters, not bytes).
pub const PAGE_CHARS: usize = 30_000;
/// Less readable text than this is a page that needs a browser.
const MIN_PAGE_CHARS: usize = 200;
/// The most of a page Burrow reads; the rest is never downloaded.
pub const PAGE_BYTES: u64 = 4 * 1024 * 1024;
/// How long one page may take to arrive, connecting included.
pub const PAGE_TIMEOUT: Duration = Duration::from_secs(15);

/// A web page that was read.
#[derive(Debug, Clone, PartialEq)]
pub struct Page {
    pub url: String,
    pub title: String,
    pub text: String,
    /// Its disclosure (`StubCite`): `{source, url, retrieved, excerpt?}`.
    pub cited: Value,
}

/// A link that couldn't be read, as the author wrote it (a URL or
/// `[[id]]`), and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    pub link: String,
    pub reason: String,
}

/// What reading a gap's links found.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Hydration {
    /// Ids of held items the links name, in order, each once.
    pub items: Vec<String>,
    pub pages: Vec<Page>,
    pub failed: Vec<Failure>,
    /// Every distinct link found, read or not.
    pub total: usize,
}

impl Hydration {
    /// How many links were read.
    pub fn read(&self) -> usize {
        self.total - self.failed.len()
    }
}

/// The distinct http(s) URLs in `text`, in order (`extractUrls`): a URL's
/// trailing sentence punctuation is the sentence's.
pub fn extract_urls(text: &str) -> Vec<String> {
    let lower = text.to_ascii_lowercase();
    let mut out: Vec<String> = Vec::new();
    let mut at = 0;
    while let Some(i) = lower[at..].find("http") {
        let p = at + i;
        let rest = &lower[p..];
        let scheme = if rest.starts_with("https://") {
            8
        } else if rest.starts_with("http://") {
            7
        } else {
            at = p + 4;
            continue;
        };
        let end = text[p + scheme..]
            .find(|c: char| c.is_whitespace() || "<>()[]\"'`".contains(c))
            .map_or(text.len(), |n| p + scheme + n);
        at = end.max(p + scheme);
        if end == p + scheme {
            continue;
        }
        let raw = text[p..end].trim_end_matches(['.', ',', ';', ':', '!', '?', '*', '_', '~']);
        if let Ok(u) = url::Url::parse(raw) {
            let s = u.to_string();
            if !out.contains(&s) {
                out.push(s);
            }
        }
    }
    out
}

/// The distinct ids of inline `[[id]]` links (not `![[id]]` directives) in
/// `text` (`extractLinkIds`).
pub fn extract_link_ids(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for r in blyg_core::grammar::link_refs(text) {
        if text[..r.range.start].ends_with('!') {
            continue;
        }
        if !out.contains(&r.id) {
            out.push(r.id);
        }
    }
    out
}

/// What [`page_text`] reads from a page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageText {
    pub title: String,
    pub site_name: Option<String>,
    pub text: String,
}

const DROP_ELEMENTS: &[&str] = &[
    "script", "style", "noscript", "svg", "nav", "header", "footer", "aside", "form", "template",
    "iframe",
];

/// Byte offset of `needle` (ASCII, lowercase) in `hay`, ignoring ASCII case.
fn find_ci(hay: &str, needle: &str, from: usize) -> Option<usize> {
    hay.get(from..)?
        .as_bytes()
        .windows(needle.len())
        .position(|w| w.eq_ignore_ascii_case(needle.as_bytes()))
        .map(|i| from + i)
}

/// `<tag` followed by a non-name character (`<tag\b`).
fn find_open(hay: &str, tag: &str, from: usize) -> Option<usize> {
    let open = format!("<{tag}");
    let mut at = from;
    while let Some(i) = find_ci(hay, &open, at) {
        let next = hay.as_bytes().get(i + open.len()).copied();
        if next.is_none_or(|b| !b.is_ascii_alphanumeric() && b != b'-') {
            return Some(i);
        }
        at = i + 1;
    }
    None
}

/// Every `<tag …>…</tag>` (lazily, as the studio's regex) and every comment
/// replaced by a space.
fn drop_elements(html: &str) -> String {
    let mut out = html.to_string();
    for tag in DROP_ELEMENTS {
        let close = format!("</{tag}>");
        let mut s = String::with_capacity(out.len());
        let mut at = 0;
        while let Some(i) = find_open(&out, tag, at) {
            let Some(j) = find_ci(&out, &close, i) else {
                break;
            };
            s.push_str(&out[at..i]);
            s.push(' ');
            at = j + close.len();
        }
        s.push_str(&out[at..]);
        out = s;
    }
    let mut s = String::with_capacity(out.len());
    let mut at = 0;
    while let Some(i) = out[at..].find("<!--").map(|i| at + i) {
        let Some(j) = out[i + 4..].find("-->").map(|j| i + 4 + j) else {
            break;
        };
        s.push_str(&out[at..i]);
        s.push(' ');
        at = j + 3;
    }
    s.push_str(&out[at..]);
    s
}

/// The `content` of `<meta property|name="…">`.
fn meta(html: &str, property: &str) -> Option<String> {
    let mut at = 0;
    while let Some(i) = find_open(html, "meta", at) {
        let end = html[i..].find('>').map_or(html.len(), |n| i + n);
        let tag = &html[i..end];
        at = end;
        let names = |attr: &str| {
            [
                format!("{attr}=\"{property}\""),
                format!("{attr}='{property}'"),
            ]
            .iter()
            .any(|a| find_ci(tag, &a.to_ascii_lowercase(), 0).is_some())
        };
        if !(names("property") || names("name")) {
            continue;
        }
        let c = find_ci(tag, "content=", 0)?;
        let rest = &tag[c + 8..];
        let q = rest.chars().next().filter(|q| *q == '"' || *q == '\'')?;
        let body = &rest[1..];
        return Some(body[..body.find(q).unwrap_or(body.len())].to_string());
    }
    None
}

/// The first `<tag …>…</tag>` (greedy to the last close, as the studio's
/// `[\s\S]*`), if any.
fn region<'a>(html: &'a str, tag: &str) -> Option<&'a str> {
    let i = find_open(html, tag, 0)?;
    let close = format!("</{tag}>");
    let lower = html.to_ascii_lowercase();
    let j = lower.rfind(&close).filter(|j| *j > i)?;
    Some(&html[i..j + close.len()])
}

/// The readable text of an HTML page (`pageText`): its `article` or `main`
/// if it has one, the body otherwise, with scripts, navigation, forms and
/// the like dropped. Text only: nothing of it is ever drawn as HTML.
pub fn page_text(html: &str) -> PageText {
    let one_line = |s: &str| {
        blyg_core::grammar::selection_text(s)
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    };
    let title = meta(html, "og:title")
        .or_else(|| {
            let i = find_open(html, "title", 0)?;
            let start = i + html[i..].find('>')? + 1;
            let end = find_ci(html, "</title>", start)?;
            Some(html[start..end].to_string())
        })
        .unwrap_or_default();
    let site_name = meta(html, "og:site_name").map(|s| one_line(&s));
    let body = drop_elements(region(html, "body").unwrap_or(html));
    let main = region(&body, "article")
        .or_else(|| region(&body, "main"))
        .unwrap_or(&body);
    PageText {
        title: one_line(&title),
        site_name: site_name.filter(|s| !s.is_empty()),
        text: blyg_core::grammar::selection_text(main),
    }
}

/// A page as the network answered it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fetched {
    pub status: u16,
    /// Where it was read from, after redirects.
    pub url: String,
    /// The media type, lowercase, parameters dropped (`""` when none).
    pub content_type: String,
    pub body: String,
}

/// Reads one page. [`Web`] is the network; a test can answer itself.
pub trait Fetch: Send + Sync {
    fn get(&self, url: &str) -> Result<Fetched, String>;
}

/// The network, bounded: [`PAGE_TIMEOUT`], [`PAGE_BYTES`], five redirects,
/// no cookies, http(s) only.
pub struct Web;

impl Fetch for Web {
    fn get(&self, url: &str) -> Result<Fetched, String> {
        let agent = ureq::AgentBuilder::new()
            .timeout(PAGE_TIMEOUT)
            .redirects(5)
            .user_agent("Burrow (reading a link for a [TK] gap)")
            .build();
        let res = match agent
            .get(url)
            .set("accept", "text/html, text/plain;q=0.9")
            .call()
        {
            Ok(r) | Err(ureq::Error::Status(_, r)) => r,
            Err(ureq::Error::Transport(t)) => {
                let m = t.to_string();
                let low = m.to_ascii_lowercase();
                return Err(if low.contains("timed out") || low.contains("timeout") {
                    "timed out".into()
                } else {
                    m
                });
            }
        };
        let status = res.status();
        let final_url = res.get_url().to_string();
        let content_type = res.content_type().trim().to_ascii_lowercase();
        let has_type = res.header("content-type").is_some();
        if !(200..300).contains(&status) || !page_type(&content_type, has_type) {
            return Ok(Fetched {
                status,
                url: final_url,
                content_type: if has_type {
                    content_type
                } else {
                    String::new()
                },
                body: String::new(),
            });
        }
        let mut bytes = Vec::new();
        res.into_reader()
            .take(PAGE_BYTES)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        Ok(Fetched {
            status,
            url: final_url,
            content_type: if has_type {
                content_type
            } else {
                String::new()
            },
            body: String::from_utf8_lossy(&bytes).into_owned(),
        })
    }
}

/// A type Burrow reads as a page (no type at all is tried as one).
fn page_type(t: &str, has_type: bool) -> bool {
    !has_type || t.is_empty() || matches!(t, "text/html" | "application/xhtml+xml" | "text/plain")
}

/// Read `url` as a page (`fetchPage`), cited as retrieved `at`.
pub fn fetch_page(fetch: &dyn Fetch, url: &str, at: &str) -> Result<Page, Failure> {
    let fail = |reason: String| Failure {
        link: url.to_string(),
        reason,
    };
    let scheme_ok = url::Url::parse(url)
        .ok()
        .filter(|u| matches!(u.scheme(), "http" | "https"))
        .filter(|u| u.username().is_empty() && u.password().is_none());
    if scheme_ok.is_none() {
        return Err(fail("not a web page address".into()));
    }
    let res = fetch.get(url).map_err(fail)?;
    if !(200..300).contains(&res.status) {
        return Err(fail(match res.status {
            429 | 403 => format!("the site refused it (HTTP {})", res.status),
            s => format!("HTTP {s}"),
        }));
    }
    if !page_type(&res.content_type, !res.content_type.is_empty()) {
        return Err(fail(format!("not a web page ({})", res.content_type)));
    }
    let PageText {
        title,
        site_name,
        text,
    } = if res.content_type == "text/plain" {
        PageText {
            title: String::new(),
            site_name: None,
            text: res.body.trim().to_string(),
        }
    } else {
        page_text(&res.body)
    };
    // The studio counts UTF-16 units; characters are as near as Rust gets.
    let chars = text.chars().count();
    if chars < MIN_PAGE_CHARS {
        return Err(fail(
            "no readable text (the page may need a browser to run it)".into(),
        ));
    }
    let host = url::Url::parse(&res.url)
        .ok()
        .and_then(|u| u.host_str().map(str::to_string))
        .unwrap_or_default();
    let mut cited = json!({
        "source": site_name.unwrap_or(host),
        "url": url,
        "retrieved": at,
    });
    if !title.is_empty() {
        cited["excerpt"] = json!(title);
    }
    let text = if chars > PAGE_CHARS {
        let cut: String = text.chars().take(PAGE_CHARS).collect();
        format!("{cut}\n[…cut at {PAGE_CHARS} characters]")
    } else {
        text
    };
    Ok(Page {
        url: url.to_string(),
        title,
        text,
        cited,
    })
}

/// Read every link in `texts` a generation may draw on (`hydrate`).
/// `exclude` holds the ids that are already sources (the scope's own
/// `![[id]]` refs) or must not be (the item being written). `home(id)` is
/// the blyg a held item lives on (its origin, yours for your own posts), or
/// `None` when Burrow doesn't hold it.
pub fn hydrate(
    texts: &[&str],
    exclude: &[String],
    home: &mut dyn FnMut(&str) -> Option<String>,
    fetch: &dyn Fetch,
    at: &str,
) -> Hydration {
    let all = texts.join("\n\n");
    let mut h = Hydration::default();
    let mut seen: Vec<String> = exclude.to_vec();

    let ids: Vec<String> = extract_link_ids(&all)
        .into_iter()
        .filter(|id| !seen.contains(id))
        .collect();
    for (n, id) in ids.iter().enumerate() {
        seen.push(id.clone());
        if n >= MAX_ITEMS {
            h.failed.push(Failure {
                link: format!("[[{id}]]"),
                reason: format!("not read: over the limit of {MAX_ITEMS} linked items"),
            });
        } else if home(id).is_some() {
            h.items.push(id.clone());
        } else {
            h.failed.push(Failure {
                link: format!("[[{id}]]"),
                reason: "not held in Burrow (sync, or follow its blyg)".into(),
            });
        }
    }

    let urls = extract_urls(&all);
    let mut to_fetch: Vec<String> = Vec::new();
    for u in &urls {
        if let Some(id) = held_at(u, home) {
            if !seen.contains(&id) {
                seen.push(id.clone());
                h.items.push(id);
            }
            continue;
        }
        if to_fetch.len() >= MAX_PAGES {
            h.failed.push(Failure {
                link: u.clone(),
                reason: format!("not read: over the limit of {MAX_PAGES} pages"),
            });
        } else {
            to_fetch.push(u.clone());
        }
    }
    let results: Vec<Result<Page, Failure>> = std::thread::scope(|s| {
        let handles: Vec<_> = to_fetch
            .iter()
            .map(|u| s.spawn(move || fetch_page(fetch, u, at)))
            .collect();
        handles
            .into_iter()
            .zip(&to_fetch)
            .map(|(t, u)| {
                t.join().unwrap_or_else(|_| {
                    Err(Failure {
                        link: u.clone(),
                        reason: "couldn't read it".into(),
                    })
                })
            })
            .collect()
    });
    for r in results {
        match r {
            Ok(p) => h.pages.push(p),
            Err(f) => h.failed.push(f),
        }
    }
    h.total = ids.len() + urls.len();
    h
}

/// The held item a URL names (`heldItem`): an id in its path, held, whose
/// own blyg is the URL's host.
fn held_at(u: &str, home: &mut dyn FnMut(&str) -> Option<String>) -> Option<String> {
    let url = url::Url::parse(u).ok()?;
    let host = url.host_str()?;
    let path = url.path();
    let alphabet = blyg_core::grammar::ID_ALPHABET.as_bytes();
    let bytes = path.as_bytes();
    let mut i = 0;
    while i + 26 <= bytes.len() {
        let run = bytes[i..]
            .iter()
            .take_while(|b| alphabet.contains(b))
            .count();
        if run >= 26 {
            // The studio's `ID` matches each 26 in a longer run, left to right.
            let mut k = i;
            while k + 26 <= i + run {
                let id = &path[k..k + 26];
                if let Some(origin) = home(id)
                    && url::Url::parse(&origin)
                        .ok()
                        .and_then(|o| o.host_str().map(str::to_string))
                        .is_some_and(|h| h == host)
                {
                    return Some(id.to_string());
                }
                k += 26;
            }
            i += run;
        } else {
            i += run.max(1);
        }
    }
    None
}
