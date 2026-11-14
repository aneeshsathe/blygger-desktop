//! --- syndication --- Where a post has been copied to on other networks
//! (decision #98, protocol 0.4 §16.14): the item member
//! `syndication: [{url, version}]`.
//!
//! - An entry is a URL and a version: an absolute `https:` URL where the
//!   copy can be read, and the version of the item it was made from. No
//!   counts, no network name, no account data.
//! - It's the publisher's claim, written in place: adding, changing or
//!   removing an entry never publishes a version (no changelog entry, no
//!   feed entry).
//! - Readers show an entry as a plain link only after an `https:` check,
//!   never fold or hide another party's post because of one, and never take
//!   it to mean the item's author is the account that posted the copy.
//!
//! The studio side (blygger-studio's `feat/syndication`, contract 5.9.0;
//! every wire shape the client depends on is in this file, so a change to
//! it lands in one place):
//!
//! - A separate resource, not a member of the owner API's item:
//!   `GET /api/items/:id/syndication` → `{entries: [{url, version}],
//!   revision}`, and `PUT` of the same with `{entries, expectedRevision}`
//!   (the ITEM's revision, required, like a pin) → `{entries, revision}`.
//!   A `PUT` bumps the item's revision; it rewrites the live document in
//!   place (no version, changelog entry or feed entry). The blyg
//!   normalizes: one entry per URL (the first wins), at most 20.
//! - 400: an entry that isn't an absolute `https:` URL, or has an extra
//!   field. 409: a version above the latest, a draft or withdrawn item, or
//!   a stale revision (`revision-conflict`).
//! - Imported rows carry `syndication` (`GET /api/reading/imported`:
//!   nullable; `GET /api/reading`: optional), `https:` only.
//! - Support is detected by `GET /api/capabilities`' `operations`
//!   containing [`PUT_OPERATION`], never by the contract number.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::model::LocalId;

/// The capability operation that says the blyg takes syndication writes.
pub const PUT_OPERATION: &str = "putSyndication";

/// The item member's name (on the item and on an imported reading row).
pub const MEMBER: &str = "syndication";

/// The answer's and the body's key.
pub const BODY_KEY: &str = "entries";

/// `meta` key: "1" when the blyg serves syndication writes, as of the last
/// contract check.
pub const SERVED_META: &str = "syndication.served";

/// The most entries a post keeps (the studio's cap).
pub const MAX_ENTRIES: usize = 20;

/// The longest URL kept.
pub const MAX_URL: usize = 2048;

/// One copy of an item elsewhere.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SyndicationEntry {
    pub url: String,
    pub version: u32,
}

impl SyndicationEntry {
    pub fn new(url: impl Into<String>, version: u32) -> Self {
        SyndicationEntry {
            url: url.into(),
            version,
        }
    }

    /// The host to show ("bsky.app"), without a leading `www.`.
    pub fn host(&self) -> String {
        host_of(&self.url)
    }
}

/// The route for one item's list (`GET` and `PUT`).
pub fn route(server_id: &str) -> String {
    format!(
        "/api/items/{}/syndication",
        url::form_urlencoded::byte_serialize(server_id.as_bytes())
            .collect::<String>()
            .replace('+', "%20")
    )
}

/// Whether a `GET /api/capabilities` answer lists [`PUT_OPERATION`] in
/// its `operations` (an array of operation ids; a map is read by its keys).
pub fn serves(capabilities: &Value) -> bool {
    match capabilities.get("operations") {
        Some(Value::Array(a)) => a.iter().any(|o| o.as_str() == Some(PUT_OPERATION)),
        Some(Value::Object(m)) => m.contains_key(PUT_OPERATION),
        _ => false,
    }
}

/// `url` as an entry may carry it: trimmed, an absolute `https:` URL with a
/// host, no credentials, not too long. `Err` says why not, for the UI.
pub fn check_url(url: &str) -> Result<String, &'static str> {
    let url = url.trim();
    if url.is_empty() {
        return Err("Paste the copy's address");
    }
    if url.len() > MAX_URL {
        return Err("That address is too long");
    }
    let Ok(parsed) = url::Url::parse(url) else {
        return Err("That isn't a web address");
    };
    if parsed.scheme() != "https" {
        return Err("Only https: addresses can be listed");
    }
    if parsed.host_str().is_none_or(str::is_empty) {
        return Err("That address has no host");
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err("That address carries a sign-in; leave it out");
    }
    Ok(url.to_string())
}

/// Whether `url` may be shown as a link (§16.14 rule 5: never a non-`https:`
/// one).
pub fn is_https(url: &str) -> bool {
    check_url(url).is_ok()
}

/// The host of `url`, without `www.` (`""` if it has none).
pub fn host_of(url: &str) -> String {
    url::Url::parse(url.trim())
        .ok()
        .and_then(|u| u.host_str().map(str::to_string))
        .map(|h| h.strip_prefix("www.").map(str::to_string).unwrap_or(h))
        .unwrap_or_default()
}

/// A list as it's kept and sent: entries that pass [`check_url`] with a
/// version of 1 or more, one per URL (the first wins), at most
/// [`MAX_ENTRIES`], in the order given.
pub fn normalize(entries: &[SyndicationEntry]) -> Vec<SyndicationEntry> {
    let mut seen = std::collections::HashSet::new();
    entries
        .iter()
        .filter(|e| e.version >= 1)
        .filter_map(|e| {
            let url = check_url(&e.url).ok()?;
            seen.insert(url.clone())
                .then_some(SyndicationEntry::new(url, e.version))
        })
        .take(MAX_ENTRIES)
        .collect()
}

/// The member as served (`[{url, version}]`), tolerant: anything that
/// isn't an entry is skipped, and the rest normalized. `None` when `v`
/// isn't an array (absent or odd: unknown).
pub fn parse_member(v: Option<&Value>) -> Option<Vec<SyndicationEntry>> {
    let arr = v?.as_array()?;
    let raw: Vec<SyndicationEntry> = arr
        .iter()
        .filter_map(|e| {
            let o = e.as_object()?;
            let url = o.get("url")?.as_str()?;
            let version = o
                .get("version")?
                .as_u64()
                .and_then(|v| u32::try_from(v).ok())?;
            Some(SyndicationEntry::new(url, version))
        })
        .collect();
    Some(normalize(&raw))
}

/// The `PUT` body: the list, and the item's revision it was based on.
pub fn put_body(entries: &[SyndicationEntry], expected_revision: u64) -> Value {
    json!({ BODY_KEY: normalize(entries), "expectedRevision": expected_revision })
}

/// A `GET` or `PUT` answer: the list as the blyg keeps it, and the item's
/// revision (0: not reported).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Answer {
    pub entries: Vec<SyndicationEntry>,
    pub revision: u64,
}

/// Read a `GET` or `PUT` answer (`{entries, revision}`). `None` when it
/// has no list.
pub fn parse_answer(v: &Value) -> Option<Answer> {
    let entries = parse_member(v.get(BODY_KEY))?;
    Some(Answer {
        entries,
        revision: v.get("revision").and_then(Value::as_u64).unwrap_or(0),
    })
}

/// A stored list (the store's JSON column). Junk reads as empty.
pub fn from_column(s: Option<&str>) -> Vec<SyndicationEntry> {
    s.and_then(|s| serde_json::from_str::<Vec<SyndicationEntry>>(s).ok())
        .unwrap_or_default()
}

/// A list for the store's JSON column (`None` for an empty one).
pub fn to_column(entries: &[SyndicationEntry]) -> Option<String> {
    (!entries.is_empty()).then(|| serde_json::to_string(entries).unwrap_or_default())
}

/// A queued write of one post's list: the whole list, in place.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyndicationOp {
    pub local_id: String,
    pub server_id: String,
    pub entries: Vec<SyndicationEntry>,
}

/// Outbox key for one post's list (one write waits per post; the last
/// replaces it).
pub fn op_key(local_id: &LocalId) -> LocalId {
    LocalId(format!("syndication\u{1f}{}", local_id.0))
}

/// The entry for `url` replaced by `new` (or added at the end when `url`
/// isn't listed); `new` = `None` removes it.
pub fn edit(
    entries: &[SyndicationEntry],
    url: Option<&str>,
    new: Option<SyndicationEntry>,
) -> Vec<SyndicationEntry> {
    let mut out: Vec<SyndicationEntry> = Vec::with_capacity(entries.len() + 1);
    let mut placed = false;
    for e in entries {
        if Some(e.url.as_str()) == url {
            if let Some(n) = &new
                && !placed
            {
                out.push(n.clone());
                placed = true;
            }
        } else {
            out.push(e.clone());
        }
    }
    if let Some(n) = new
        && !placed
    {
        out.push(n);
    }
    normalize(&out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(url: &str, v: u32) -> SyndicationEntry {
        SyndicationEntry::new(url, v)
    }

    #[test]
    fn served_when_the_capabilities_list_the_operation() {
        assert!(serves(&json!({"contractVersion": "5.9.0",
            "operations": ["getSyndication", "pinItem", "putSyndication"]})));
        assert!(serves(&json!({"operations": {"putSyndication": {}}})));
        assert!(!serves(
            &json!({"contractVersion": "5.9.0", "operations": ["getSyndication"]})
        ));
        assert!(!serves(&json!({"contractVersion": "5.9.0"})));
    }

    #[test]
    fn only_https_urls_with_a_host_are_entries() {
        assert_eq!(
            check_url(" https://social.example.net/@a/1 ").unwrap(),
            "https://social.example.net/@a/1"
        );
        for bad in [
            "",
            "http://social.example.net/1",
            "javascript:alert(1)",
            "data:text/html,x",
            "ftp://x.example/a",
            "https://",
            "https://user:pw@social.example.net/1",
            "social.example.net/1",
        ] {
            assert!(check_url(bad).is_err(), "{bad}");
            assert!(!is_https(bad), "{bad}");
        }
    }

    #[test]
    fn normalize_drops_bad_entries_and_repeats() {
        let got = normalize(&[
            e("https://a.example/1", 2),
            e("http://b.example/1", 2),
            e("https://a.example/1", 3),
            e("https://c.example/1", 0),
            e("https://d.example/1", 1),
        ]);
        assert_eq!(
            got,
            [e("https://a.example/1", 2), e("https://d.example/1", 1)]
        );
    }

    #[test]
    fn the_member_parses_tolerantly() {
        let v = json!([
            {"url": "https://a.example/1", "version": 3},
            {"url": "http://b.example/1", "version": 3},
            {"url": "https://c.example/1"},
            {"url": "https://d.example/1", "version": 2, "likes": 40},
            "junk"
        ]);
        assert_eq!(
            parse_member(Some(&v)).unwrap(),
            [e("https://a.example/1", 3), e("https://d.example/1", 2)]
        );
        assert_eq!(parse_member(None), None);
        assert_eq!(parse_member(Some(&json!({"x": 1}))), None);
        assert_eq!(parse_member(Some(&json!([]))), Some(vec![]));
    }

    #[test]
    fn the_put_body_and_answer() {
        let list = [e("https://a.example/1", 1)];
        assert_eq!(
            put_body(&list, 7),
            json!({"entries": [{"url": "https://a.example/1", "version": 1}], "expectedRevision": 7})
        );
        let ans = json!({"entries": [{"url": "https://a.example/1", "version": 1}], "revision": 8});
        assert_eq!(
            parse_answer(&ans).unwrap(),
            Answer {
                entries: list.to_vec(),
                revision: 8
            }
        );
        assert_eq!(parse_answer(&json!({"entries": []})).unwrap().revision, 0);
        assert_eq!(parse_answer(&json!({"id": "x"})), None);
        assert_eq!(route("a b"), "/api/items/a%20b/syndication");
    }

    #[test]
    fn edit_adds_replaces_and_removes() {
        let list = vec![e("https://a.example/1", 1), e("https://b.example/1", 2)];
        let added = edit(&list, None, Some(e("https://c.example/1", 3)));
        assert_eq!(added.len(), 3);
        let replaced = edit(
            &list,
            Some("https://a.example/1"),
            Some(e("https://a.example/2", 2)),
        );
        assert_eq!(
            replaced,
            [e("https://a.example/2", 2), e("https://b.example/1", 2)]
        );
        let removed = edit(&list, Some("https://a.example/1"), None);
        assert_eq!(removed, [e("https://b.example/1", 2)]);
        // A bad new entry is dropped by the normalization.
        assert_eq!(edit(&list, None, Some(e("http://x.example", 1))), list);
    }

    #[test]
    fn hosts_drop_www() {
        assert_eq!(host_of("https://www.example.org/a"), "example.org");
        assert_eq!(host_of("https://bsky.app/profile/x"), "bsky.app");
        assert_eq!(host_of("nope"), "");
        assert_eq!(from_column(Some("junk")), vec![]);
        assert_eq!(to_column(&[]), None);
    }
}
