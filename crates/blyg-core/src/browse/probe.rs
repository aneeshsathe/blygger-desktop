//! --- browse --- Telling which links name a blyg item without the blyg's
//! help: a port of blygger-studio 0.49's `src/peek.ts` (`classifyLinks`) for
//! a node that doesn't serve `POST /api/peek/links`.
//!
//! In the studio's order, for each distinct http(s) link (fragment dropped):
//!
//! 1. a permalink of our own blyg (`{origin}f/{id}/` or `t/{id}/`) is that
//!    item, with no request;
//! 2. a held post of a blyg subscription whose `page` is the link is that
//!    post, with no request;
//! 3. a cached answer younger than a week (hits and misses alike);
//! 4. otherwise the page is fetched with the token-less public client (its
//!    first 256 KiB, bounded in time), its first `<link rel="alternate"
//!    type="application/json" href=…>` (§5.8) is fetched, and the link names
//!    the item that document describes, when the document is a blyg item
//!    document (`blyg` key, string `id`, an http(s) `origin`) served from the
//!    origin it claims (§12.2). The answer is cached, except when the network
//!    failed (offline, a 5xx): that is asked again next time.
//!
//! One deliberate difference: the studio accepts a document from anywhere
//! under the claimed origin's parent path (for a root origin, any host); here
//! the document must come from the claimed origin's own scheme, host and
//! port.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use serde_json::Value;

use super::{ItemRef, LinkTarget};
use crate::api::public::PublicClient;
use crate::backend::CoreError;
use crate::store::Store;

/// How long an answer is trusted (the studio's `PROBE_TTL_MS`).
pub const PROBE_TTL_MS: i64 = 7 * 24 * 3600 * 1000;
/// How much of a page is read for its `<link>` tags (`PAGE_BYTES`).
pub const PAGE_BYTES: u64 = 256 * 1024;
/// The largest item document read.
const DOC_BYTES: u64 = 2 * 1024 * 1024;
/// Pages probed at once (the studio's `mapLimit(urls, 4, …)`).
const PARALLEL: usize = 4;

/// The client for probes: short timeouts, and a ceiling on each request.
pub fn client() -> PublicClient {
    PublicClient::bounded(
        Duration::from_secs(5),
        Duration::from_secs(8),
        Duration::from_secs(12),
    )
}

/// `url` without its fragment, when it is an http(s) URL, folded the way a
/// URL from the network is before it meets a written origin (§12.2,
/// §15.4 step 2: lowercase scheme and host, no default port, uppercase
/// escapes), so [`own_permalink`]'s prefix test compares one spelling.
pub fn lookup_key(url: &str) -> Option<String> {
    crate::origin::fold_url(url)
}

/// The studio's item-id rule (`peek.ts` `ID_RE`).
fn valid_id(id: &str) -> bool {
    let n = id.chars().count();
    (1..=256).contains(&n)
        && !id
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || c == '/')
}

/// `{origin}f/{id}` or `t/{id}`, with or without the trailing slash, for
/// one of `own` (normalized origins, ending in `/`).
pub fn own_permalink(key: &str, own: &[String]) -> Option<ItemRef> {
    for o in own {
        let Some(rest) = key.strip_prefix(o.as_str()) else {
            continue;
        };
        let rest = rest.strip_suffix('/').unwrap_or(rest);
        let Some(id) = rest.strip_prefix("f/").or_else(|| rest.strip_prefix("t/")) else {
            continue;
        };
        if valid_id(id) && !id.contains(['?', '#']) {
            return Some(ItemRef {
                origin: o.clone(),
                id: id.to_string(),
            });
        }
    }
    None
}

fn attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let lower = tag.to_ascii_lowercase();
    let mut from = 0;
    while let Some(i) = lower[from..].find(name) {
        let at = from + i;
        from = at + name.len();
        // A whole attribute name: preceded by whitespace.
        if !lower[..at].ends_with(|c: char| c.is_ascii_whitespace()) {
            continue;
        }
        let rest = lower[from..].trim_start();
        let Some(rest) = rest.strip_prefix('=') else {
            continue;
        };
        let rest_t = rest.trim_start();
        let start = tag.len() - rest_t.len();
        let quote = rest_t.chars().next()?;
        if quote != '"' && quote != '\'' {
            continue;
        }
        let val = &tag[start + 1..];
        let end = val.find(quote)?;
        return Some(&val[..end]);
    }
    None
}

/// §5.8: the first `<link rel="alternate" type="application/json" href>` in
/// the page's first [`PAGE_BYTES`], resolved against `base` (the URL the page
/// was served from). The studio's `alternateJsonHref`.
pub fn alternate_json_href(html: &str, base: &str) -> Option<String> {
    let mut end = html.len().min(PAGE_BYTES as usize);
    while !html.is_char_boundary(end) {
        end -= 1;
    }
    let head = &html[..end];
    let lower = head.to_ascii_lowercase();
    let mut from = 0;
    while let Some(i) = lower[from..].find("<link") {
        let at = from + i;
        from = at + 5;
        // `<link` as a whole tag name.
        if !lower[from..].starts_with(|c: char| c.is_ascii_whitespace() || c == '>' || c == '/') {
            continue;
        }
        let Some(close) = head[at..].find('>') else {
            break;
        };
        let tag = &head[at..at + close + 1];
        let rel = attr(tag, "rel").unwrap_or_default().to_ascii_lowercase();
        let ty = attr(tag, "type").unwrap_or_default().to_ascii_lowercase();
        if rel.split_ascii_whitespace().any(|r| r == "alternate")
            && ty.starts_with("application/json")
        {
            let Some(href) = attr(tag, "href").filter(|h| !h.is_empty()) else {
                continue;
            };
            let base = url::Url::parse(base).ok()?;
            return base
                .join(&href.replace("&amp;", "&"))
                .ok()
                .map(|u| u.to_string());
        }
    }
    None
}

/// The item a fetched document describes, when it is a blyg item document
/// served (`at`, after redirects) from the origin it claims.
pub fn item_of_doc(doc: &Value, at: &str) -> Option<ItemRef> {
    doc.get("blyg")?;
    let id = doc.get("id")?.as_str().filter(|i| valid_id(i))?;
    let origin = crate::profile::normalize_origin(doc.get("origin")?.as_str()?)?;
    let o = url::Url::parse(&origin).ok()?;
    let a = url::Url::parse(at).ok()?;
    let same = o.scheme() == a.scheme()
        && o.host_str() == a.host_str()
        && o.port_or_known_default() == a.port_or_known_default();
    same.then(|| ItemRef {
        origin,
        id: id.to_string(),
    })
}

/// What one probe learned: `Ok(answer)` is worth caching; `Err` is a
/// network failure, asked again next time.
fn probe(public: &PublicClient, url: &str) -> Result<Option<ItemRef>, ()> {
    let failed = |e: CoreError| match e {
        CoreError::Offline => Err(()),
        CoreError::Rejected { status, .. } if status >= 500 || status == 429 => Err(()),
        _ => Ok(None),
    };
    let (html, at) = match public.get_page(url, PAGE_BYTES) {
        Ok(p) => p,
        Err(e) => return failed(e),
    };
    let Some(href) = alternate_json_href(&html, &at) else {
        return Ok(None);
    };
    match public.get_json_at(&href, DOC_BYTES) {
        Ok((doc, at)) => Ok(item_of_doc(&doc, &at)),
        Err(e) => failed(e),
    }
}

/// Classify `urls` locally (see the module docs). Order follows `urls`; a
/// link that isn't http(s) is `None`.
pub fn classify(
    store: &Store,
    public: &PublicClient,
    own: &[String],
    urls: &[String],
    now: i64,
) -> Vec<LinkTarget> {
    let keys: Vec<Option<String>> = urls.iter().map(|u| lookup_key(u)).collect();
    let mut answers: std::collections::HashMap<String, Option<ItemRef>> = Default::default();
    let mut ask: Vec<String> = Vec::new();
    for k in keys.iter().flatten() {
        if answers.contains_key(k) || ask.contains(k) {
            continue;
        }
        let known = own_permalink(k, own)
            .map(Some)
            .or_else(|| store.held_item_at(k).map(Some))
            .or_else(|| store.cached_link_probe(k, now - PROBE_TTL_MS));
        match known {
            Some(a) => {
                answers.insert(k.clone(), a);
            }
            None => ask.push(k.clone()),
        }
    }
    if !ask.is_empty() {
        let next = AtomicUsize::new(0);
        let found: Vec<(String, Result<Option<ItemRef>, ()>)> = std::thread::scope(|s| {
            let workers: Vec<_> = (0..PARALLEL.min(ask.len()))
                .map(|_| {
                    s.spawn(|| {
                        let mut got = Vec::new();
                        loop {
                            let i = next.fetch_add(1, Ordering::Relaxed);
                            let Some(k) = ask.get(i) else { break };
                            got.push((k.clone(), probe(public, k)));
                        }
                        got
                    })
                })
                .collect();
            workers
                .into_iter()
                .flat_map(|w| w.join().unwrap_or_default())
                .collect()
        });
        for (k, r) in found {
            match r {
                Ok(a) => {
                    let _ = store.put_link_probe(&k, a.as_ref(), now);
                    answers.insert(k, a);
                }
                Err(()) => {
                    answers.insert(k, None);
                }
            }
        }
    }
    urls.iter()
        .zip(keys)
        .map(|(u, k)| LinkTarget {
            url: u.clone(),
            target: k.and_then(|k| answers.get(&k).cloned().flatten()),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn finds_the_alternate_item_document() {
        let html = r#"<html><head>
            <link rel="stylesheet" href="/s.css">
            <LINK type="application/json; charset=utf-8" REL='Alternate Index' href="../items/abc.json?x=1&amp;y=2">
            <link rel="alternate" type="application/json" href="/other.json">
            </head>"#;
        assert_eq!(
            alternate_json_href(html, "https://blyg.example.com/b/f/abc/").as_deref(),
            Some("https://blyg.example.com/b/f/items/abc.json?x=1&y=2")
        );
        // An RSS alternate, a `<linkx>` and an empty href don't count.
        let html = r#"<link rel="alternate" type="application/rss+xml" href="/feed.xml">
            <linkx rel="alternate" type="application/json" href="/no.json">
            <link rel="alternate" type="application/json" href="">"#;
        assert_eq!(alternate_json_href(html, "https://blyg.example.com/"), None);
        assert_eq!(alternate_json_href("", "https://blyg.example.com/"), None);
    }

    #[test]
    fn only_a_blyg_document_from_its_own_origin_names_an_item() {
        let doc = json!({"blyg": "0.4", "id": "abc", "origin": "https://blyg.example.com/b"});
        let got = item_of_doc(&doc, "https://blyg.example.com/b/items/abc.json").unwrap();
        assert_eq!(got.origin, "https://blyg.example.com/b/");
        assert_eq!(got.id, "abc");
        // Served from elsewhere than it claims (§12.2).
        assert_eq!(
            item_of_doc(&doc, "https://evil.example.org/items/abc.json"),
            None
        );
        // Not a blyg document, or no usable id.
        let plain = json!({"id": "abc", "origin": "https://blyg.example.com/"});
        assert_eq!(item_of_doc(&plain, "https://blyg.example.com/x.json"), None);
        let bad = json!({"blyg": "0.4", "id": "a/b", "origin": "https://blyg.example.com/"});
        assert_eq!(item_of_doc(&bad, "https://blyg.example.com/x.json"), None);
    }

    #[test]
    fn own_permalinks_need_no_request() {
        let own = vec!["https://blyg.example.com/b/".to_string()];
        let hit = |u: &str| own_permalink(u, &own).map(|r| r.id);
        assert_eq!(
            hit("https://blyg.example.com/b/f/abc/").as_deref(),
            Some("abc")
        );
        assert_eq!(
            hit("https://blyg.example.com/b/t/xyz").as_deref(),
            Some("xyz")
        );
        assert_eq!(hit("https://blyg.example.com/b/f/abc/v2/"), None);
        assert_eq!(hit("https://blyg.example.com/f/abc/"), None);
        assert_eq!(hit("https://blyg.example.com/b/about/"), None);
    }

    #[test]
    fn a_local_classification_keeps_order_and_uses_the_cache() {
        let store = Store::open_in_memory().unwrap();
        let own = vec!["https://blyg.example.com/".to_string()];
        let hit = ItemRef {
            origin: "https://other.example.org/".into(),
            id: "q1".into(),
        };
        store
            .put_link_probe("https://other.example.org/f/q1/", Some(&hit), 1_000)
            .unwrap();
        store
            .put_link_probe("https://example.org/wiki", None, 1_000)
            .unwrap();
        let urls: Vec<String> = [
            "mailto:someone@example.org",
            "https://example.org/wiki#Section",
            "https://blyg.example.com/f/mine/",
            "https://other.example.org/f/q1/",
        ]
        .map(String::from)
        .to_vec();
        // Nothing here needs the network: everything is known or not a page.
        let got = classify(&store, &client(), &own, &urls, 2_000);
        assert_eq!(got.len(), 4);
        assert_eq!(got[0].target, None);
        assert_eq!(got[1].url, "https://example.org/wiki#Section");
        assert_eq!(got[1].target, None);
        assert_eq!(got[2].target.as_ref().map(|t| t.id.as_str()), Some("mine"));
        assert_eq!(got[3].target, Some(hit));
    }
}
