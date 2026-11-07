//! --- web-reading --- The Reader's side of Studio in a browser: the
//! sources (smart feeds, Background, Mentions, hoppers, subscriptions) and
//! the Threads / Fragments lens over the stream; subscriptions (subscribe
//! with a preview, unsubscribe, pause, rename, blogroll, poll, resync, OPML
//! in and out); mentions (inbound, hide one, outbound, the accept toggle);
//! hoppers (create, rename, describe, public, delete); thumbs; peeking at
//! a post you don't follow, and the browsing history.
//!
//! The rules are the app's: the sources pane is `reading/sources_vm.rs`
//! (compiled unchanged), mention rows and groups `reading/vm.rs`, OPML
//! `blyg_core::opml`. The page fetches (`www/web-reading.js`, through
//! `live.js` `call`); this says what to ask and what the answer means. In
//! the demo the same actions change the sample data here, and nothing is
//! sent anywhere.
//!
//! Speed: nothing here runs on start or on a keystroke. The lists are read
//! when their screen first opens; the stream's filter is one pass over the
//! rows it already orders (`wr_filter`), and the Mentions lens's keys are
//! worked out once per change of the rows.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use blyg_core::api::takes_whole_blyg;
use blyg_core::api::wire::{WireMention, WireSubscription};
use blyg_core::{Hopper, Kind, Mention, ReadingItem, Subscription, SubscriptionKind};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use wasm_bindgen::prelude::*;

use crate::api::{self, Request, enc};
use crate::read_api::key_of;
use crate::reading::sources_vm::{self as svm, Entry, Smart, Source};
use crate::reading::vm as rvm;
use crate::{Burrow, at, json, vm};

/// What the app's `Subscription` doesn't keep: how polling goes.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SubDiag {
    pub last_poll_at: Option<String>,
    pub fail_count: u64,
    /// The newest flag: (type, detail).
    pub flag: Option<(String, String)>,
}

/// An outbound Webmention (`direction=outbound`): one of your posts
/// telling another site it links to it.
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Outbound {
    pub id: String,
    #[serde(default)]
    pub item_id: String,
    #[serde(default)]
    pub version: u32,
    #[serde(default)]
    pub target: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub attempts: u32,
    #[serde(default)]
    pub last_error: Option<String>,
    #[serde(default)]
    pub created: String,
}

/// A fresh browsing trail id. blyg-core's reads the system clock, which
/// panics in a browser, so the page's crypto random is used there.
#[cfg(target_family = "wasm")]
fn new_trail() -> String {
    let mut b = [0u8; 8];
    let _ = getrandom::getrandom(&mut b);
    format!("{:016x}", u64::from_le_bytes(b) | 1)
}
#[cfg(not(target_family = "wasm"))]
fn new_trail() -> String {
    blyg_core::browse::new_trail()
}

/// A post read from its own blyg without subscribing (`GET /api/peek`).
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PeekView {
    pub origin: String,
    pub id: String,
    pub host: String,
    pub kind: String,
    pub version: u32,
    pub title: String,
    pub html: String,
    pub url: String,
    /// You follow its blyg already.
    pub followed: bool,
}

/// One step of the browsing history (`GET /api/browse`).
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Step {
    #[serde(default)]
    pub at: String,
    #[serde(default)]
    pub origin: String,
    #[serde(default)]
    pub remote_id: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub via: String,
}

type Stamp = (usize, u64, String);

/// The Reader's state in a browser. Nothing in it is persisted: the lists
/// are read again when their screens open after a reload.
#[derive(Default)]
pub struct WebReading {
    pub source: Source,
    /// The Threads / Fragments lens (`None`: both).
    pub lens: Option<Kind>,
    /// `GET /api/subscriptions` has been read (live; the demo's are seeded).
    pub subs_loaded: bool,
    pub diags: HashMap<String, SubDiag>,
    pub mentions: Option<Vec<Mention>>,
    pub outbound: Option<Vec<Outbound>>,
    pub accept_mentions: Option<bool>,
    /// The blyg's Mentions lens (`GET /api/reading?mentions=true`), as
    /// reading keys, once read.
    pub served_mentions: Option<HashSet<String>>,
    /// The Mentions lens worked out from the rows, for the rows it was
    /// worked out from.
    local_mentions: RefCell<(Option<Stamp>, HashSet<String>)>,
    /// Mentions seen on the Mentions screen this session (no "new" dot).
    pub seen: HashSet<String>,
    pub peek: Option<PeekView>,
    pub history: Option<Vec<Step>>,
    /// The blyg's `contractVersion`, for `wholeBlyg`.
    pub contract: Option<String>,
    /// This session's browsing trail.
    pub trail: String,
    /// Demo: the next id for a made-up subscription or hopper.
    next: u32,
}

/// The arguments of an action, as the page sends them (camelCase JSON).
#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct Args {
    id: String,
    url: String,
    title: Option<String>,
    name: Option<String>,
    description: Option<String>,
    public: Option<bool>,
    on: Option<bool>,
    key: String,
    thumb: i8,
    prev: Option<i8>,
    origin: String,
    via: String,
}

fn args(s: &str) -> Args {
    serde_json::from_str(s).unwrap_or_default()
}

fn toast(message: impl Into<String>) -> String {
    json(&json!({ "toast": message.into() }))
}

fn error(message: impl Into<String>) -> String {
    json(&json!({ "error": message.into() }))
}

fn req(r: Request) -> String {
    json(&json!({ "req": r }))
}

fn post(path: &str, body: Value) -> Request {
    Request {
        method: "POST",
        path: path.into(),
        body: Some(body),
    }
}

fn patch(path: String, body: Value) -> Request {
    Request {
        method: "PATCH",
        path,
        body: Some(body),
    }
}

fn delete(path: String) -> Request {
    Request {
        method: "DELETE",
        path,
        body: None,
    }
}

fn sub_path(id: &str) -> String {
    format!("/api/subscriptions/{}", enc(id))
}

fn hopper_path(id: &str) -> String {
    format!("/api/hoppers/{}", enc(id))
}

/// A source as the page names it: `smart:unread`, `sub:{id}`, `hopper:{id}`,
/// `own`, `mentions`.
pub fn source_key(s: &Source) -> String {
    match s {
        Source::Smart(m) => format!(
            "smart:{}",
            match m {
                Smart::Unread => "unread",
                Smart::Today => "today",
                Smart::Thumbed => "thumbed",
                Smart::All => "all",
                Smart::Background => "background",
                Smart::Disliked => "disliked",
                Smart::Threads => "threads",
                Smart::Fragments => "fragments",
            }
        ),
        Source::Folder(f) => format!("folder:{f}"),
        Source::Sub(id) => format!("sub:{id}"),
        Source::Own => "own".into(),
        Source::Hopper(id) => format!("hopper:{id}"),
        Source::Trail(id) => format!("trail:{id}"),
        Source::Mentions => "mentions".into(),
    }
}

pub fn parse_source(k: &str) -> Option<Source> {
    let (a, b) = k.split_once(':').unwrap_or((k, ""));
    Some(match (a, b) {
        ("smart", "unread") => Source::Smart(Smart::Unread),
        ("smart", "today") => Source::Smart(Smart::Today),
        ("smart", "thumbed") => Source::Smart(Smart::Thumbed),
        ("smart", "all") => Source::Smart(Smart::All),
        ("smart", "background") => Source::Smart(Smart::Background),
        ("smart", "disliked") => Source::Smart(Smart::Disliked),
        ("smart", "threads") => Source::Smart(Smart::Threads),
        ("smart", "fragments") => Source::Smart(Smart::Fragments),
        ("sub", id) if !id.is_empty() => Source::Sub(id.into()),
        ("hopper", id) if !id.is_empty() => Source::Hopper(id.into()),
        ("own", _) => Source::Own,
        ("mentions", _) => Source::Mentions,
        _ => return None,
    })
}

/// The held posts (not yours, not withdrawn) that point at `own_origin`:
/// a quote, stub or fork of one of your posts, or a link to your blyg. A
/// copy of `reading/mentions_feed.rs` `local_mentions` (that module also
/// holds the feed's GPUI state), keyed as this crate keys rows.
pub fn local_mentions(rows: &[ReadingItem], own_origin: &str) -> HashSet<String> {
    let Some(own) = blyg_core::profile::normalize_origin(own_origin) else {
        return HashSet::new();
    };
    let host = vm::url_host(&own).unwrap_or_default();
    if host.is_empty() {
        return HashSet::new();
    }
    let needle = format!("://{host}");
    rows.iter()
        .filter(|r| !crate::reading::own::is_own_row(r) && !svm::is_background(r))
        .filter(|r| {
            r.references().iter().any(|p| p.origin == own)
                || links_to(&r.content_md, &needle)
                || (r.content_md.is_empty() && links_to(&r.content_html, &needle))
        })
        .map(key_of)
        .collect()
}

/// `text` holds `://host` ending where a host ends (not `://host.other`).
fn links_to(text: &str, needle: &str) -> bool {
    text.match_indices(needle).any(|(i, _)| {
        text[i + needle.len()..]
            .chars()
            .next()
            .is_none_or(|c| !(c.is_alphanumeric() || c == '.' || c == '-'))
    })
}

/// A subscription and how its polling goes, from the contract's
/// `Subscription`.
pub fn sub_of(v: &Value) -> Option<(Subscription, SubDiag)> {
    let w: WireSubscription = serde_json::from_value(v.clone()).ok()?;
    let flag = v
        .get("flags")
        .and_then(Value::as_array)
        .and_then(|f| {
            f.iter()
                .max_by(|a, b| {
                    let at = |x: &Value| {
                        x.get("at")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string()
                    };
                    at(a).cmp(&at(b))
                })
                .cloned()
        })
        .map(|f| {
            let s = |k: &str| f.get(k).and_then(Value::as_str).unwrap_or("").to_string();
            (s("type"), s("detail"))
        });
    let d = SubDiag {
        last_poll_at: v
            .get("lastPollAt")
            .and_then(Value::as_str)
            .map(str::to_string),
        fail_count: v.get("failCount").and_then(Value::as_f64).unwrap_or(0.0) as u64,
        flag,
    };
    Some((Subscription::from(w), d))
}

fn hopper_of(v: &Value) -> Option<Hopper> {
    let s = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_string);
    Some(Hopper {
        id: s("id")?,
        name: s("name").unwrap_or_default(),
        slug: s("slug"),
        public: v.get("public").and_then(Value::as_bool).unwrap_or(false),
        count: 0,
        description: s("description").filter(|d| !d.trim().is_empty()),
        slug_frozen: v
            .get("slugFrozen")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

/// How a subscription is doing, in words ("" when all is well).
pub fn sub_state(s: &Subscription, d: Option<&SubDiag>, now: DateTime<Utc>) -> String {
    let polled = d
        .and_then(|d| d.last_poll_at.as_deref())
        .map(|t| format!("checked {}", vm::relative_time(t, now)));
    match s.status.as_str() {
        "paused" => "Paused".into(),
        "degraded" => {
            let why = d
                .and_then(|d| d.flag.as_ref())
                .map(|(t, detail)| {
                    if detail.is_empty() {
                        t.clone()
                    } else {
                        detail.clone()
                    }
                })
                .filter(|w| !w.trim().is_empty());
            let fails = d.map(|d| d.fail_count).unwrap_or(0);
            let mut out = String::from("Having trouble");
            if fails > 0 {
                out.push_str(&format!(
                    ": {fails} failed check{}",
                    if fails == 1 { "" } else { "s" }
                ));
            }
            if let Some(w) = why {
                out.push_str(&format!(" · {}", vm::truncate_chars(&w, 140)));
            }
            out
        }
        _ => polled.unwrap_or_default(),
    }
}

/// An item's text as a one-line title, from its HTML.
fn html_title(html: &str) -> String {
    // The first block's text: a thread's title line, a fragment's opening.
    let end = ["</h1>", "</h2>", "</h3>", "</p>", "</li>", "</blockquote>"]
        .iter()
        .filter_map(|t| html.find(t).map(|i| i + t.len()))
        .min()
        .unwrap_or(html.len());
    let text = blyg_render::plain_text_from_html(&html[..end]);
    vm::truncate_chars(
        text.split_whitespace().collect::<Vec<_>>().join(" ").trim(),
        90,
    )
}

#[derive(Serialize)]
struct SourceRow {
    /// `heading` | `smart` | `own` | `mentions` | `hopper` | `sub`.
    kind: &'static str,
    key: String,
    label: String,
    /// A number for the smart feeds (private, reader-local), a dot for the
    /// rest (`dot`).
    count: Option<usize>,
    dot: bool,
    on: bool,
    /// A subscription's state line (paused, trouble).
    note: Option<String>,
    letter: Option<String>,
}

#[derive(Serialize)]
struct SubRow {
    id: String,
    name: String,
    host: String,
    origin: String,
    feed_url: String,
    kind: &'static str,
    paused: bool,
    trouble: bool,
    state: String,
    in_blogroll: bool,
    own_title: bool,
    held: usize,
}

#[derive(Serialize)]
struct MentionOut {
    id: String,
    who: String,
    origin: String,
    relation: String,
    when: String,
    new: bool,
    hidden: bool,
    source: String,
    status: String,
}

#[derive(Serialize)]
struct MentionGroup {
    title: String,
    /// The local id of your post, to open it.
    item: Option<String>,
    rows: Vec<MentionOut>,
}

#[derive(Serialize)]
struct OutboundRow {
    title: String,
    target: String,
    host: String,
    status: String,
    error: Option<String>,
    when: String,
    failed: bool,
}

#[derive(Serialize)]
struct HopperOut {
    id: String,
    name: String,
    public: bool,
    description: String,
    url: Option<String>,
    held: usize,
}

impl Burrow {
    /// The subscriptions the pane lists: the blyg's once read; before
    /// that, the ones the reading list names (so the pane works offline
    /// and before anything is fetched).
    fn wr_subs(&self) -> Vec<Subscription> {
        if !self.is_live() || self.wr.subs_loaded {
            return self.rd.subs.clone();
        }
        let mut seen = HashSet::new();
        self.rd
            .rows
            .iter()
            .filter(|r| !crate::reading::own::is_own_row(r))
            .filter(|r| seen.insert(r.subscription_id.clone()))
            .map(|r| Subscription {
                id: r.subscription_id.clone(),
                kind: SubscriptionKind::Blyg,
                origin: r.origin.clone(),
                feed_url: String::new(),
                title: r.subscription_title.clone(),
                status: "active".into(),
                in_blogroll: false,
                title_follows_source: true,
                poll: Default::default(),
            })
            .collect()
    }

    /// The Mentions lens's keys: the blyg's when read, else worked out
    /// from the rows (again only when they changed).
    fn wr_mention_keys(&self) -> HashSet<String> {
        if let Some(s) = &self.wr.served_mentions {
            return s.clone();
        }
        let origin = self.origin_str();
        let stamp = (
            self.rd.rows.len(),
            self.rd.rows.iter().fold(0u64, |a, r| {
                a.wrapping_mul(31).wrapping_add(r.version as u64)
            }),
            origin.clone(),
        );
        let mut local = self.wr.local_mentions.borrow_mut();
        if local.0.as_ref() != Some(&stamp) {
            local.1 = local_mentions(&self.rd.rows, &origin);
            local.0 = Some(stamp);
        }
        local.1.clone()
    }

    /// The stream's rows for the chosen source and lens. With the default
    /// (All, both kinds) it only drops withdrawn posts, which only
    /// Background lists, as in the app.
    pub(crate) fn wr_filter<'a>(
        &self,
        rows: Vec<&'a ReadingItem>,
        now: DateTime<Utc>,
    ) -> Vec<&'a ReadingItem> {
        let lens = self.wr.lens;
        let kind_ok = |r: &ReadingItem| lens.is_none_or(|k| r.kind == k);
        match &self.wr.source {
            Source::Mentions => {
                let keys = self.wr_mention_keys();
                rows.into_iter()
                    .filter(|r| kind_ok(r) && !svm::is_background(r) && keys.contains(&key_of(r)))
                    .collect()
            }
            s => {
                let filed = HashMap::new();
                rows.into_iter()
                    .filter(|r| kind_ok(r) && svm::in_source(r, s, &filed, now))
                    .collect()
            }
        }
    }

    /// What the stream says when the chosen source shows nothing.
    pub(crate) fn wr_empty(&self) -> Option<String> {
        if self.wr.source == Source::default() && self.wr.lens.is_none() {
            return None;
        }
        let base = svm::empty_label(&self.wr.source);
        Some(match self.wr.lens {
            None => base.into(),
            Some(Kind::Thread) => format!("No threads here. {base}"),
            Some(Kind::Fragment) => format!("No fragments here. {base}"),
        })
    }

    fn wr_sub_name(&self, id: &str) -> String {
        self.rd
            .subs
            .iter()
            .find(|s| s.id == id)
            .map(svm::sub_name)
            .unwrap_or_else(|| "that subscription".into())
    }

    fn wr_put_sub(&mut self, s: Subscription, d: SubDiag) {
        // A rename shows on its rows too (their byline falls back to it).
        for r in self
            .rd
            .rows
            .iter_mut()
            .filter(|r| r.subscription_id == s.id)
        {
            if r.subscription_title != s.title {
                r.subscription_title = s.title.clone();
                self.reading_dirty = true;
            }
        }
        self.wr.diags.insert(s.id.clone(), d);
        match self.rd.subs.iter_mut().find(|x| x.id == s.id) {
            Some(x) => *x = s,
            None => self.rd.subs.push(s),
        }
    }

    fn wr_drop_sub(&mut self, id: &str) {
        self.rd.subs.retain(|s| s.id != id);
        self.wr.diags.remove(id);
        let before = self.rd.rows.len();
        self.rd.rows.retain(|r| r.subscription_id != id);
        if self.rd.rows.len() != before {
            self.reading_dirty = true;
        }
        if self.wr.source == Source::Sub(id.into()) {
            self.wr.source = Source::default();
        }
    }

    fn wr_drop_hopper(&mut self, id: &str) {
        self.rd.hoppers.retain(|h| h.id != id);
        for r in self.rd.rows.iter_mut() {
            if r.hoppers.iter().any(|h| h == id) {
                r.hoppers.retain(|h| h != id);
                self.reading_dirty = true;
            }
        }
        for h in self.store.hopped.values_mut() {
            h.retain(|x| x != id);
        }
        self.store.live.hops.retain(|h| h.hopper != id);
        if self.wr.source == Source::Hopper(id.into()) {
            self.wr.source = Source::default();
        }
    }

    fn wr_set_thumb(&mut self, key: &str, thumb: Option<i8>) {
        if let Some(r) = self.rd.rows.iter_mut().find(|r| key_of(r) == key)
            && r.thumb != thumb
        {
            r.thumb = thumb;
            self.reading_dirty = true;
        }
    }

    fn wr_whole_blyg(&self) -> bool {
        self.wr.contract.as_deref().is_some_and(takes_whole_blyg)
    }

    fn wr_subscribe_body(&self, url: &str) -> Value {
        let mut body = json!({ "url": url.trim() });
        if self.wr_whole_blyg() {
            body["wholeBlyg"] = json!(true);
        }
        body
    }

    fn wr_demo_id(&mut self, prefix: &str) -> String {
        self.wr.next += 1;
        format!("{prefix}-web-{}", self.wr.next)
    }

    /// The demo's side of an action: the sample data changes here.
    fn wr_demo(&mut self, action: &str, a: &Args, now: DateTime<Utc>) -> String {
        match action {
            "preview" => {
                let url = a.url.trim();
                let Ok(u) = url::Url::parse(url) else {
                    return error("That isn't an address. Try https://…");
                };
                let origin = format!("{}://{}", u.scheme(), u.host_str().unwrap_or(""));
                if self
                    .rd
                    .subs
                    .iter()
                    .any(|s| s.origin.trim_end_matches('/') == origin)
                {
                    return error("You already follow that blyg");
                }
                let feed = url.ends_with(".xml") || url.contains("/feed");
                json(&json!({ "preview": {
                    "kind": if feed { "rss" } else { "blyg" },
                    "title": u.host_str().unwrap_or(url),
                    "origin": origin,
                    "feedUrl": url,
                    "mismatch": Value::Null,
                    "hopper": Value::Null,
                }}))
            }
            "subscribe" => {
                let Ok(u) = url::Url::parse(a.url.trim()) else {
                    return error("That isn't an address. Try https://…");
                };
                let id = self.wr_demo_id("sub");
                let title = a
                    .title
                    .clone()
                    .filter(|t| !t.trim().is_empty())
                    .unwrap_or_else(|| u.host_str().unwrap_or("").to_string());
                self.wr_put_sub(
                    Subscription {
                        id,
                        kind: SubscriptionKind::Blyg,
                        origin: format!("{}://{}/", u.scheme(), u.host_str().unwrap_or("")),
                        feed_url: a.url.trim().into(),
                        title: title.clone(),
                        status: "active".into(),
                        in_blogroll: false,
                        title_follows_source: true,
                        poll: Default::default(),
                    },
                    SubDiag::default(),
                );
                toast(format!(
                    "Subscribed to {title}. In the demo, nothing is fetched."
                ))
            }
            "unsubscribe" => {
                let name = self.wr_sub_name(&a.id);
                self.wr_drop_sub(&a.id);
                toast(format!("Unsubscribed from {name}"))
            }
            "pause" | "rename" | "blogroll" => {
                let Some(mut s) = self.rd.subs.iter().find(|s| s.id == a.id).cloned() else {
                    return error("That subscription isn't here any more");
                };
                let d = self.wr.diags.get(&a.id).cloned().unwrap_or_default();
                let msg = match action {
                    "pause" => {
                        let on = a.on.unwrap_or(true);
                        s.status = if on { "paused" } else { "active" }.into();
                        if on { "Paused" } else { "Resumed" }.to_string()
                    }
                    "rename" => match a.title.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
                        Some(t) => {
                            s.title = t.into();
                            s.title_follows_source = false;
                            format!("Renamed to {t}")
                        }
                        None => {
                            s.title_follows_source = true;
                            "It takes its own name again".into()
                        }
                    },
                    _ => {
                        s.in_blogroll = a.on.unwrap_or(false);
                        if s.in_blogroll {
                            "On your blogroll".into()
                        } else {
                            "Off your blogroll".into()
                        }
                    }
                };
                self.wr_put_sub(s, d);
                toast(msg)
            }
            "poll" => toast("In the demo, there's nothing to check"),
            "resync" => toast("In the demo, it's already up to date"),
            "hopper_new" => {
                let name = a.name.clone().unwrap_or_default();
                let id = self.wr_demo_id("hop");
                self.rd.hoppers.push(Hopper {
                    id,
                    name: name.trim().into(),
                    slug: None,
                    public: false,
                    count: 0,
                    description: None,
                    slug_frozen: false,
                });
                toast(format!("Made the hopper {}", name.trim()))
            }
            "hopper_edit" => {
                let Some(h) = self.rd.hoppers.iter_mut().find(|h| h.id == a.id) else {
                    return error("That hopper isn't here any more");
                };
                if let Some(n) = &a.name {
                    h.name = n.trim().into();
                }
                if let Some(d) = &a.description {
                    h.description = Some(d.trim().to_string()).filter(|d| !d.is_empty());
                }
                if let Some(p) = a.public {
                    h.public = p;
                    if p && h.slug.is_none() {
                        h.slug = Some(slug(&h.name));
                    }
                }
                toast("Saved")
            }
            "hopper_delete" => {
                let name = self
                    .rd
                    .hoppers
                    .iter()
                    .find(|h| h.id == a.id)
                    .map(|h| h.name.clone())
                    .unwrap_or_default();
                self.wr_drop_hopper(&a.id);
                toast(format!(
                    "Deleted the hopper {name}. Its posts are still here."
                ))
            }
            "mention_hide" => {
                if let Some(m) = self
                    .wr
                    .mentions
                    .as_mut()
                    .and_then(|ms| ms.iter_mut().find(|m| m.id == a.id))
                {
                    m.hidden = a.on.unwrap_or(true);
                }
                toast(if a.on.unwrap_or(true) {
                    "Hidden"
                } else {
                    "Shown again"
                })
            }
            "accept_mentions" => {
                self.wr.accept_mentions = a.on;
                toast(if a.on == Some(true) {
                    "Webmentions are accepted"
                } else {
                    "Webmentions are turned away"
                })
            }
            "peek" | "peek_url" => toast("In the demo, Peek reads only the posts held here"),
            "history_clear" => {
                self.wr.history = Some(vec![]);
                toast("History cleared")
            }
            _ => {
                let _ = now;
                json(&json!({}))
            }
        }
    }
}

/// A hopper's address name, as the studio makes it (demo only).
fn slug(name: &str) -> String {
    let s: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect();
    s.split('-')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

#[wasm_bindgen]
impl Burrow {
    // ------------------------------------------------------------ sources

    /// The sources pane: smart feeds (with their private counts),
    /// Mentions, hoppers and subscriptions (with a dot when there's
    /// something to read), the chosen one marked; and the lens.
    pub fn wr_sources(&self, now_ms: f64) -> String {
        let now = at(now_ms);
        let subs = self.wr_subs();
        let mut hoppers = self.rd.hoppers.clone();
        for h in &mut hoppers {
            h.count = self
                .rd
                .rows
                .iter()
                .filter(|r| r.hoppers.contains(&h.id))
                .count() as u32;
        }
        let entries = svm::entries(
            &hoppers,
            &[],
            &HashMap::new(),
            &subs,
            &self.rd.rows,
            &HashSet::new(),
            now,
        );
        let cur = &self.wr.source;
        let mut out = Vec::with_capacity(entries.len() + 1);
        // Threads and Fragments are the lens here (it narrows any source),
        // so their smart feeds would say the same thing twice.
        let entries = entries.into_iter().filter(|e| {
            !matches!(
                e,
                Entry::Smart {
                    smart: Smart::Threads | Smart::Fragments,
                    ..
                }
            )
        });
        for e in entries {
            let row = match &e {
                Entry::Heading(h) => SourceRow {
                    kind: "heading",
                    key: String::new(),
                    label: (*h).into(),
                    count: None,
                    dot: false,
                    on: false,
                    note: None,
                    letter: None,
                },
                Entry::Smart { smart, unread } => SourceRow {
                    kind: "smart",
                    key: source_key(&Source::Smart(*smart)),
                    label: smart.label().into(),
                    count: (*unread > 0 && *smart != Smart::Background).then_some(*unread),
                    dot: false,
                    on: *cur == Source::Smart(*smart),
                    note: None,
                    letter: None,
                },
                Entry::Own => SourceRow {
                    kind: "own",
                    key: "own".into(),
                    label: crate::reading::own::OWN_LABEL.into(),
                    count: None,
                    dot: false,
                    on: *cur == Source::Own,
                    note: None,
                    letter: None,
                },
                Entry::Hopper(h) => SourceRow {
                    kind: "hopper",
                    key: source_key(&Source::Hopper(h.id.clone())),
                    label: h.name.clone(),
                    count: None,
                    dot: false,
                    on: *cur == Source::Hopper(h.id.clone()),
                    note: Some(format!(
                        "{}{}",
                        if h.public { "public · " } else { "" },
                        match h.count {
                            1 => "1 post here".to_string(),
                            n => format!("{n} posts here"),
                        }
                    )),
                    letter: None,
                },
                Entry::Sub { sub, unread, .. } => {
                    let d = self.wr.diags.get(&sub.id);
                    let state = sub_state(sub, d, now);
                    SourceRow {
                        kind: "sub",
                        key: source_key(&Source::Sub(sub.id.clone())),
                        label: svm::sub_name(sub),
                        count: None,
                        dot: *unread,
                        on: *cur == Source::Sub(sub.id.clone()),
                        note: (sub.status != "active").then_some(state),
                        letter: Some(svm::letter(&sub.title, &sub.origin)),
                    }
                }
                Entry::Folder { .. } | Entry::Trail(_) | Entry::Mentions { .. } => continue,
            };
            let after_background = matches!(
                e,
                Entry::Smart {
                    smart: Smart::Background,
                    ..
                }
            );
            out.push(row);
            if after_background {
                let keys = self.wr_mention_keys();
                out.push(SourceRow {
                    kind: "mentions",
                    key: "mentions".into(),
                    label: svm::MENTIONS_LABEL.into(),
                    count: None,
                    dot: self
                        .rd
                        .rows
                        .iter()
                        .any(|r| svm::needs_reading(r) && keys.contains(&key_of(r))),
                    on: *cur == Source::Mentions,
                    note: None,
                    letter: None,
                });
            }
        }
        json(&json!({
            "rows": out,
            "label": self.wr_source_label(),
            "lens": match self.wr.lens { None => "all", Some(Kind::Thread) => "thread", Some(Kind::Fragment) => "fragment" },
            "filtered": self.wr.source != Source::default() || self.wr.lens.is_some(),
            "subsLoaded": !self.is_live() || self.wr.subs_loaded,
        }))
    }

    /// The chosen source's name, with the lens.
    pub fn wr_source_label(&self) -> String {
        let name = svm::source_label(&self.wr.source, &[], &self.wr_subs(), &self.rd.hoppers);
        match self.wr.lens {
            None => name,
            Some(Kind::Thread) => format!("{name} · threads"),
            Some(Kind::Fragment) => format!("{name} · fragments"),
        }
    }

    /// Choose a source (`source_key`); an unknown one is All.
    pub fn wr_set_source(&mut self, key: &str) {
        self.wr.source = parse_source(key).unwrap_or_default();
    }

    /// The Threads / Fragments lens: `thread`, `fragment`, else both.
    pub fn wr_set_lens(&mut self, lens: &str) {
        self.wr.lens = match lens {
            "thread" => Some(Kind::Thread),
            "fragment" => Some(Kind::Fragment),
            _ => None,
        };
    }

    /// The source and lens back to All, both kinds.
    pub fn wr_clear_source(&mut self) {
        self.wr.source = Source::default();
        self.wr.lens = None;
    }

    pub fn wr_source(&self) -> String {
        source_key(&self.wr.source)
    }

    // ------------------------------------------------------------ lists

    /// Where a list is read from (live): `{path, query, limit, one}` for
    /// `subs`, `mentions`, `outbound`, `lens` (the Mentions lens), `history`
    /// or `settings`; `one`: a single request, not every page. `null` in
    /// the demo, whose lists are here already (`wr_demo_lists`).
    pub fn wr_list(&self, which: &str) -> String {
        if !self.is_live() {
            return "null".into();
        }
        let (path, query, limit, one) = match which {
            "subs" => ("/api/subscriptions", "", 100, false),
            "mentions" => ("/api/mentions", "direction=inbound", 100, false),
            "outbound" => ("/api/mentions", "direction=outbound", 100, false),
            "lens" => ("/api/reading", "mentions=true", 100, false),
            "history" => ("/api/browse", "limit=100", 100, true),
            "settings" => ("/api/settings", "", 0, true),
            "capabilities" => ("/api/capabilities", "", 0, true),
            _ => return "null".into(),
        };
        json(&json!({ "path": path, "query": query, "limit": limit, "one": one }))
    }

    /// Whether `which` has been read (or is the demo's).
    pub fn wr_has(&self, which: &str) -> bool {
        match which {
            "subs" => !self.is_live() || self.wr.subs_loaded,
            "mentions" => self.wr.mentions.is_some(),
            "outbound" => self.wr.outbound.is_some(),
            "lens" => self.wr.served_mentions.is_some(),
            "history" => self.wr.history.is_some(),
            "settings" => self.wr.accept_mentions.is_some(),
            "capabilities" => self.wr.contract.is_some(),
            _ => false,
        }
    }

    /// The demo's lists: its sample mentions and settings (subscriptions
    /// and hoppers are the reading seed's already).
    pub fn wr_demo_lists(&mut self, now_ms: f64) {
        if self.is_live() || self.wr.mentions.is_some() {
            return;
        }
        let s = crate::seed::reading_seed::seed(at(now_ms));
        self.wr.mentions = Some(s.mentions);
        self.wr.outbound = Some(vec![]);
        self.wr.accept_mentions = Some(s.settings.accept_mentions.unwrap_or(true));
        self.wr.history = Some(vec![]);
    }

    /// A list read: `json` is the collected items (a JSON array) for a
    /// paged list, or the one answer's body for `one`. Returns `{n}` or
    /// `{error}`.
    pub fn wr_loaded(&mut self, which: &str, body: &str) -> String {
        let v: Value = match serde_json::from_str(body) {
            Ok(v) => v,
            Err(e) => return error(e.to_string()),
        };
        let items = || v.as_array().cloned().unwrap_or_default();
        let n = match which {
            "subs" => {
                let list: Vec<(Subscription, SubDiag)> =
                    items().iter().filter_map(sub_of).collect();
                let n = list.len();
                self.rd.subs.clear();
                self.wr.diags.clear();
                for (s, d) in list {
                    self.wr.diags.insert(s.id.clone(), d);
                    self.rd.subs.push(s);
                }
                self.wr.subs_loaded = true;
                n
            }
            "mentions" => {
                let list: Vec<Mention> = items()
                    .into_iter()
                    .filter_map(|m| serde_json::from_value::<WireMention>(m).ok())
                    .map(Mention::from)
                    .collect();
                let n = list.len();
                self.wr.mentions = Some(list);
                n
            }
            "outbound" => {
                let list: Vec<Outbound> = items()
                    .into_iter()
                    .filter_map(|m| serde_json::from_value(m).ok())
                    .collect();
                let n = list.len();
                self.wr.outbound = Some(list);
                n
            }
            "lens" => {
                let keys: HashSet<String> = items()
                    .iter()
                    .filter_map(|e| {
                        let i = e.get("imported")?;
                        Some(format!(
                            "{}|{}",
                            i.get("subscriptionId")?.as_str()?,
                            i.get("remoteId")?.as_str()?
                        ))
                    })
                    .collect();
                let n = keys.len();
                self.wr.served_mentions = Some(keys);
                n
            }
            "history" => {
                let list: Vec<Step> = v
                    .get("items")
                    .and_then(Value::as_array)
                    .map(|a| {
                        a.iter()
                            .filter_map(|s| serde_json::from_value(s.clone()).ok())
                            .collect()
                    })
                    .unwrap_or_default();
                let n = list.len();
                self.wr.history = Some(list);
                n
            }
            "settings" => {
                self.wr.accept_mentions = Some(
                    v.get("acceptMentions")
                        .and_then(Value::as_bool)
                        .unwrap_or(true),
                );
                1
            }
            "capabilities" => {
                self.wr.contract = v
                    .get("contractVersion")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .or(Some(String::new()));
                1
            }
            _ => 0,
        };
        json(&json!({ "n": n }))
    }

    /// After the reading list was read again, the blyg's Mentions lens is
    /// stale: it is read again when next chosen.
    pub fn wr_forget(&mut self, which: &str) {
        match which {
            "lens" => self.wr.served_mentions = None,
            "subs" => self.wr.subs_loaded = false,
            "mentions" => {
                self.wr.mentions = None;
                self.wr.outbound = None;
            }
            "history" => self.wr.history = None,
            _ => {}
        }
    }

    // ------------------------------------------------------------ views

    /// The Subscriptions screen: each subscription with its state.
    pub fn wr_subs_view(&self, now_ms: f64) -> String {
        let now = at(now_ms);
        let mut held: HashMap<&str, usize> = HashMap::new();
        for r in &self.rd.rows {
            *held.entry(r.subscription_id.as_str()).or_default() += 1;
        }
        let rows: Vec<SubRow> = self
            .wr_subs()
            .iter()
            .map(|s| {
                let d = self.wr.diags.get(&s.id);
                SubRow {
                    id: s.id.clone(),
                    name: svm::sub_name(s),
                    host: rvm::host(&s.origin),
                    origin: s.origin.clone(),
                    feed_url: s.feed_url.clone(),
                    kind: match s.kind {
                        SubscriptionKind::Blyg => "blyg",
                        SubscriptionKind::Rss => "feed",
                    },
                    paused: s.status == "paused",
                    trouble: s.status == "degraded",
                    state: sub_state(s, d, now),
                    in_blogroll: s.in_blogroll,
                    own_title: !s.title_follows_source,
                    held: held.get(s.id.as_str()).copied().unwrap_or(0),
                }
            })
            .collect();
        json(&json!({ "rows": rows, "loaded": !self.is_live() || self.wr.subs_loaded }))
    }

    /// The Mentions screen: inbound mentions grouped under your posts
    /// (newest first, hidden ones marked), what you sent, and whether the
    /// blyg accepts Webmentions. Opening it marks them seen.
    pub fn wr_mentions_view(&mut self, now_ms: f64) -> String {
        let now = at(now_ms);
        let items = self.store.items();
        let mut ms: Vec<Mention> = self.wr.mentions.clone().unwrap_or_default();
        ms.sort_by(|a, b| b.first_seen.cmp(&a.first_seen));
        let rows: Vec<(rvm::MentionRow, &Mention)> = ms
            .iter()
            .map(|m| (rvm::mention_row(m, &self.wr.seen, now), m))
            .collect();
        let groups: Vec<MentionGroup> = rvm::group_mentions(rows, &items)
            .into_iter()
            .map(|(title, item, rows)| MentionGroup {
                title: vm::truncate_chars(&title, 90),
                item: item.map(|i| i.local_id.0.clone()),
                rows: rows
                    .into_iter()
                    .map(|r| {
                        let status = ms
                            .iter()
                            .find(|m| m.id == r.id)
                            .map(|m| m.status.clone())
                            .unwrap_or_default();
                        MentionOut {
                            id: r.id,
                            who: r.who,
                            origin: r.origin,
                            relation: r.relation,
                            when: r.when,
                            new: r.new,
                            hidden: r.hidden,
                            source: r.source,
                            status: match status.as_str() {
                                "verified" => String::new(),
                                "pending" => "not checked yet".into(),
                                "failed" => "couldn't be checked".into(),
                                "gone" => "the link is gone".into(),
                                s => s.into(),
                            },
                        }
                    })
                    .collect(),
            })
            .collect();
        let outbound: Vec<OutboundRow> = self
            .wr
            .outbound
            .clone()
            .unwrap_or_default()
            .into_iter()
            .map(|o| {
                let title = items
                    .iter()
                    .find(|i| i.server_id.as_ref().is_some_and(|s| s.0 == o.item_id))
                    .map(|i| vm::truncate_chars(&vm::item_title(i), 70))
                    .unwrap_or_else(|| "One of your posts".into());
                OutboundRow {
                    title,
                    host: rvm::host(&o.target),
                    status: match o.status.as_str() {
                        "sent" => "sent".into(),
                        "pending" => "waiting to send".into(),
                        "failed" => format!(
                            "failed after {} tr{}",
                            o.attempts,
                            if o.attempts == 1 { "y" } else { "ies" }
                        ),
                        "no_endpoint" => "that site takes no Webmentions".into(),
                        s => s.into(),
                    },
                    failed: o.status == "failed",
                    error: o.last_error.clone().filter(|e| !e.trim().is_empty()),
                    when: vm::relative_time(&o.created, now),
                    target: o.target,
                }
            })
            .collect();
        for m in &ms {
            self.wr.seen.insert(m.id.clone());
        }
        json(&json!({
            "groups": groups,
            "outbound": outbound,
            "accept": self.wr.accept_mentions,
            "loaded": self.wr.mentions.is_some(),
            "outboundLoaded": self.wr.outbound.is_some(),
        }))
    }

    /// The Hoppers screen.
    pub fn wr_hoppers_view(&self) -> String {
        let base = self.origin_str();
        let rows: Vec<HopperOut> = self
            .rd
            .hoppers
            .iter()
            .map(|h| HopperOut {
                id: h.id.clone(),
                name: h.name.clone(),
                public: h.public,
                description: h.description.clone().unwrap_or_default(),
                url: h
                    .slug
                    .as_deref()
                    .filter(|_| h.public)
                    .map(|s| format!("{}/h/{s}/", base.trim_end_matches('/'))),
                held: self
                    .rd
                    .rows
                    .iter()
                    .filter(|r| r.hoppers.contains(&h.id))
                    .count(),
            })
            .collect();
        json(&rows)
    }

    /// The browsing history, newest first.
    pub fn wr_history_view(&self, now_ms: f64) -> String {
        let now = at(now_ms);
        let rows: Vec<Value> = self
            .wr
            .history
            .clone()
            .unwrap_or_default()
            .into_iter()
            .map(|s| {
                json!({
                    "origin": s.origin,
                    "id": s.remote_id,
                    "title": s.title.clone().filter(|t| !t.trim().is_empty()).unwrap_or_else(|| s.remote_id.clone()),
                    "host": rvm::host(&s.origin),
                    "when": vm::relative_time(&s.at, now),
                })
            })
            .collect();
        json(&json!({ "rows": rows, "loaded": self.wr.history.is_some() }))
    }

    /// Under an open post: its thumb, and whether it can take one.
    pub fn wr_post_extra(&self, key: &str) -> String {
        let r = self.rd.rows.iter().find(|r| key_of(r) == key);
        json(&json!({
            "thumb": r.and_then(|r| r.thumb),
            "can": r.is_some_and(|r| !crate::reading::own::is_own_row(r)),
        }))
    }

    /// The post a peek read, as last shown.
    pub fn wr_peek_view(&self) -> String {
        json(&self.wr.peek)
    }

    // ------------------------------------------------------------ OPML

    /// An OPML file to import: its feeds, each marked when you follow it
    /// already (`blyg_core::opml`), or `{error}`.
    pub fn wr_opml(&self, text: &str) -> String {
        match blyg_core::opml::parse_bytes(text.as_bytes()) {
            Ok(p) => {
                let subs = self.wr_subs();
                let feeds: Vec<Value> = p
                    .feeds
                    .iter()
                    .map(|f| {
                        json!({
                            "title": if f.title.trim().is_empty() { rvm::host(&f.xml_url) } else { f.title.clone() },
                            "url": f.xml_url,
                            "folder": f.folder,
                            "followed": blyg_core::opml::followed(f, &subs).is_some(),
                        })
                    })
                    .collect();
                json(&json!({
                    "feeds": feeds,
                    "duplicates": p.duplicates,
                    "withoutFeed": p.without_feed,
                }))
            }
            Err(e) => error(e.to_string()),
        }
    }

    /// Every subscription as OPML 2.0, for another reader.
    pub fn wr_opml_export(&self) -> String {
        blyg_core::opml::export(&self.wr_subs())
    }

    // ------------------------------------------------------------ actions

    /// Start an action. Live: `{req}` for the page to send, then hand the
    /// answer to `wr_done`; offline, `{error}`. Demo: the sample data
    /// changes here and the answer is `{toast}` (or `{preview}`). Some
    /// answer at once either way: `{open}` (a peeked address is a post
    /// held here), `{error}` (a rule says no).
    pub fn wr_do(&mut self, action: &str, args_json: &str, now_ms: f64) -> String {
        let a = args(args_json);
        // Rules that hold in both modes.
        match action {
            "preview" | "subscribe" if a.url.trim().is_empty() => {
                return error("Type a blyg's or a feed's address");
            }
            "preview" | "subscribe" if url::Url::parse(a.url.trim()).is_err() => {
                return error("That isn't an address. Try https://…");
            }
            "hopper_new" if a.name.as_deref().unwrap_or("").trim().is_empty() => {
                return error("A hopper needs a name");
            }
            "hopper_edit" if a.name.as_deref().is_some_and(|n| n.trim().is_empty()) => {
                return error("A hopper needs a name");
            }
            "hopper_edit"
                if a.description
                    .as_deref()
                    .is_some_and(|d| d.trim().chars().count() > 280) =>
            {
                return error("A description is at most 280 characters");
            }
            "thumb" => {
                let Some(r) = self.rd.rows.iter().find(|r| key_of(r) == a.key) else {
                    return error("That post isn't here any more");
                };
                if crate::reading::own::is_own_row(r) {
                    return error("Your own posts don't take a thumb");
                }
                let thumb = match a.thumb {
                    1 => Some(1),
                    -1 => Some(-1),
                    _ => None,
                };
                let prev = r.thumb;
                self.wr_set_thumb(&a.key, thumb);
                if !self.is_live() {
                    return json(&json!({ "done": true }));
                }
                let (sub, rid) = a.key.split_once('|').unwrap_or(("", ""));
                let path = format!("/api/signals/{}/{}", enc(sub), enc(rid));
                let r = match thumb {
                    Some(t) => Request {
                        method: "PUT",
                        path,
                        body: Some(json!({ "thumb": t })),
                    },
                    None => delete(path),
                };
                return json(&json!({ "req": r, "prev": prev }));
            }
            "peek_url" => {
                let url = a.url.trim();
                if url::Url::parse(url).is_err() {
                    return error("That isn't an address. Try https://…");
                }
                let subs = self.wr_subs();
                if let Some(r) = rvm::blyg_item_for_url(url, &self.rd.rows, &subs) {
                    return json(&json!({ "open": key_of(r) }));
                }
            }
            "peek" => {
                if let Some(r) = self.rd.rows.iter().find(|r| {
                    blyg_core::post_key(&r.origin, &r.remote_id)
                        == blyg_core::post_key(&a.origin, &a.id)
                }) {
                    return json(&json!({ "open": key_of(r) }));
                }
            }
            _ => {}
        }
        if !self.is_live() {
            return self.wr_demo(action, &a, at(now_ms));
        }
        if self.offline {
            return error("That needs the blyg. Try again when you're back online.");
        }
        let r = match action {
            "preview" => post("/api/subscriptions", self.wr_subscribe_body(&a.url)),
            "subscribe" => {
                let mut body = self.wr_subscribe_body(&a.url);
                body["confirm"] = json!(true);
                if let Some(t) = a.title.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
                    body["title"] = json!(t);
                }
                post("/api/subscriptions", body)
            }
            "unsubscribe" => delete(sub_path(&a.id)),
            "pause" => patch(sub_path(&a.id), json!({ "paused": a.on.unwrap_or(true) })),
            "rename" => patch(
                sub_path(&a.id),
                json!({ "title": a.title.as_deref().map(str::trim).filter(|t| !t.is_empty()) }),
            ),
            "blogroll" => patch(
                sub_path(&a.id),
                json!({ "inBlogroll": a.on.unwrap_or(false) }),
            ),
            "poll" => post("/api/subscriptions/poll", json!({})),
            "resync" => post(&format!("{}/resync", sub_path(&a.id)), json!({})),
            "hopper_new" => post(
                "/api/hoppers",
                json!({ "name": a.name.as_deref().unwrap_or("").trim() }),
            ),
            "hopper_edit" => {
                let mut body = json!({});
                if let Some(n) = &a.name {
                    body["name"] = json!(n.trim());
                }
                if let Some(d) = &a.description {
                    body["description"] = json!(d.trim());
                }
                if let Some(p) = a.public {
                    body["public"] = json!(p);
                }
                patch(hopper_path(&a.id), body)
            }
            "hopper_delete" => delete(hopper_path(&a.id)),
            "mention_hide" => {
                if let Some(m) = self
                    .wr
                    .mentions
                    .as_mut()
                    .and_then(|ms| ms.iter_mut().find(|m| m.id == a.id))
                {
                    m.hidden = a.on.unwrap_or(true);
                }
                patch(
                    format!("/api/mentions/{}", enc(&a.id)),
                    json!({ "hidden": a.on.unwrap_or(true) }),
                )
            }
            "accept_mentions" => patch(
                "/api/settings".into(),
                json!({ "acceptMentions": a.on.unwrap_or(true) }),
            ),
            "peek_url" => post("/api/peek/links", json!({ "urls": [a.url.trim()] })),
            "peek" => Request::get(format!(
                "/api/peek?origin={}&id={}",
                enc(&a.origin),
                enc(&a.id)
            )),
            "history_clear" => delete("/api/browse".into()),
            _ => return error(format!("Burrow doesn't know how to {action}")),
        };
        req(r)
    }

    /// The blyg's answer to an action `wr_do` asked for. Returns what the
    /// page does next: `{toast}`, `{error}`, `{preview}`, `{peek}`,
    /// `{next: {action, args}}` (another action), `{fire}` (a request whose
    /// answer doesn't matter), `{reload: […]}` (lists to read again).
    pub fn wr_done(
        &mut self,
        action: &str,
        args_json: &str,
        status: u16,
        body: &str,
        now_ms: f64,
    ) -> String {
        let a = args(args_json);
        let v = match api::read(status, body, None) {
            Ok(v) => v,
            Err(e) => {
                // An optimistic change goes back.
                match action {
                    "thumb" => self.wr_set_thumb(&a.key, a.prev),
                    "mention_hide" => {
                        if let Some(m) = self
                            .wr
                            .mentions
                            .as_mut()
                            .and_then(|ms| ms.iter_mut().find(|m| m.id == a.id))
                        {
                            m.hidden = !a.on.unwrap_or(true);
                        }
                    }
                    _ => {}
                }
                let text = match (action, &e) {
                    ("preview" | "subscribe", api::ApiError::Rejected { status: 409, .. }) => {
                        "You already follow that".into()
                    }
                    ("peek", api::ApiError::NotFound) => {
                        "That post isn't on its blyg any more".into()
                    }
                    _ => e.message(),
                };
                return json(&json!({ "error": text, "code": e.to_json()["code"] }));
            }
        };
        let now = at(now_ms);
        match action {
            "preview" => {
                // A studio that subscribes without asking (201) did it.
                if let Some((s, d)) = v.get("id").and(sub_of(&v)) {
                    let t = svm::sub_name(&s);
                    self.wr_put_sub(s, d);
                    return json(
                        &json!({ "toast": format!("Subscribed to {t}"), "reload": ["reading"] }),
                    );
                }
                let s = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_string);
                let mismatch = v.get("siteMismatch").and_then(|m| {
                    let actual = m.get("actual").and_then(Value::as_str)?;
                    Some(format!(
                        "It says it's {} but it's served from {actual}",
                        m.get("asserted")
                            .and_then(Value::as_str)
                            .unwrap_or("another site")
                    ))
                });
                let hopper = v
                    .get("hopper")
                    .and_then(|h| h.get("title"))
                    .and_then(Value::as_str)
                    .map(str::to_string);
                json(&json!({ "preview": {
                    "kind": s("kind").unwrap_or_else(|| "blyg".into()),
                    "title": s("title").or(s("blygTitle")).unwrap_or_default(),
                    "origin": s("origin"),
                    "feedUrl": s("feedUrl"),
                    "mismatch": mismatch,
                    "hopper": hopper,
                }}))
            }
            "subscribe" => {
                let Some((s, d)) = sub_of(&v) else {
                    return json(&json!({ "toast": "Subscribed", "reload": ["subs", "reading"] }));
                };
                let t = svm::sub_name(&s);
                self.wr_put_sub(s, d);
                json(
                    &json!({ "toast": format!("Subscribed to {t}. Its posts arrive as the blyg reads them."), "reload": ["reading"] }),
                )
            }
            "unsubscribe" => {
                let name = self.wr_sub_name(&a.id);
                self.wr_drop_sub(&a.id);
                toast(format!("Unsubscribed from {name}"))
            }
            "pause" | "rename" | "blogroll" => {
                if let Some((s, d)) = sub_of(&v) {
                    self.wr_put_sub(s, d);
                }
                toast(match action {
                    "pause" if a.on.unwrap_or(true) => {
                        "Paused: it isn't checked until you resume it".to_string()
                    }
                    "pause" => "Resumed".into(),
                    "rename" if a.title.as_deref().is_some_and(|t| !t.trim().is_empty()) => {
                        "Renamed".into()
                    }
                    "rename" => "It takes its own name again".into(),
                    _ if a.on == Some(true) => "On your blogroll".into(),
                    _ => "Off your blogroll".into(),
                })
            }
            "poll" => {
                let n = v.get("polling").and_then(Value::as_u64).unwrap_or(0);
                json(&json!({
                    "toast": if n == 0 { "Nothing to check: every subscription is paused".to_string() } else { format!("Checking {n} subscription{}…", if n == 1 { "" } else { "s" }) },
                    "reload": ["reading-later", "subs-later"],
                }))
            }
            "resync" => {
                let changed = v.get("changed").and_then(Value::as_u64).unwrap_or(0);
                json(&json!({
                    "toast": if changed == 0 { "Up to date".to_string() } else { format!("Read again: {changed} changed") },
                    "reload": if changed > 0 { json!(["reading", "subs"]) } else { json!(["subs"]) },
                }))
            }
            "hopper_new" => match hopper_of(&v) {
                Some(h) => {
                    let name = h.name.clone();
                    self.rd.hoppers.push(h);
                    self.reading_dirty = true;
                    toast(format!("Made the hopper {name}"))
                }
                None => json(&json!({ "toast": "Made it", "reload": ["reading"] })),
            },
            "hopper_edit" => {
                if let Some(h) = hopper_of(&v)
                    && let Some(x) = self.rd.hoppers.iter_mut().find(|x| x.id == h.id)
                {
                    *x = h;
                    self.reading_dirty = true;
                }
                toast("Saved")
            }
            "hopper_delete" => {
                let name = self
                    .rd
                    .hoppers
                    .iter()
                    .find(|h| h.id == a.id)
                    .map(|h| h.name.clone())
                    .unwrap_or_default();
                self.wr_drop_hopper(&a.id);
                toast(format!(
                    "Deleted the hopper {name}. Its posts are still here."
                ))
            }
            "thumb" => json(&json!({ "done": true })),
            "mention_hide" => toast(if a.on.unwrap_or(true) {
                "Hidden"
            } else {
                "Shown again"
            }),
            "accept_mentions" => {
                self.wr.accept_mentions = Some(
                    v.get("acceptMentions")
                        .and_then(Value::as_bool)
                        .unwrap_or(a.on.unwrap_or(true)),
                );
                toast(if self.wr.accept_mentions == Some(true) {
                    "Webmentions are accepted"
                } else {
                    "Webmentions are turned away"
                })
            }
            "peek_url" => {
                let target = v
                    .get("links")
                    .and_then(Value::as_array)
                    .and_then(|l| l.first())
                    .and_then(|l| l.get("target"))
                    .filter(|t| t.is_object());
                match target {
                    Some(t) => json(&json!({ "next": { "action": "peek", "args": {
                        "origin": t.get("origin").and_then(Value::as_str).unwrap_or(""),
                        "id": t.get("id").and_then(Value::as_str).unwrap_or(""),
                        "via": "start",
                    }}})),
                    None => error(
                        "That address isn't a blyg post Burrow can read here. Open it in the browser instead.",
                    ),
                }
            }
            "peek" => {
                let p: blyg_core::api::browse::PeekItem = match serde_json::from_value(v) {
                    Ok(p) => p,
                    Err(e) => return error(format!("The post didn't read: {e}")),
                };
                let html = crate::studio::sanitize::sanitize_at(&p.content_html, &p.origin);
                let title = html_title(&html);
                let followed = p.subscription_id.is_some()
                    || self.rd.subs.iter().any(|s| {
                        blyg_core::profile::normalize_origin(&s.origin)
                            == blyg_core::profile::normalize_origin(&p.origin)
                    });
                let view = PeekView {
                    host: rvm::host(&p.origin),
                    origin: p.origin.clone(),
                    id: p.id.clone(),
                    kind: p.kind.clone(),
                    version: p.version,
                    title: title.clone(),
                    html,
                    url: p.url.clone(),
                    followed,
                };
                self.wr.peek = Some(view.clone());
                if self.wr.trail.is_empty() {
                    self.wr.trail = new_trail();
                }
                let via = match a.via.as_str() {
                    "link" | "quote" | "stub" | "back" => a.via.clone(),
                    _ => "start".into(),
                };
                let log = post(
                    "/api/browse",
                    json!({
                        "trail": self.wr.trail,
                        "origin": p.origin,
                        "remoteId": p.id,
                        "version": p.version,
                        "via": via,
                        "title": blyg_core::api::browse::clip_title(&title),
                    }),
                );
                self.wr.history = None;
                let _ = now;
                json(&json!({ "peek": view, "fire": log }))
            }
            "history_clear" => {
                self.wr.history = Some(vec![]);
                toast("History cleared")
            }
            _ => json(&json!({})),
        }
    }

    /// Set the blyg's `contractVersion` (from `/api/capabilities`).
    pub fn wr_set_contract(&mut self, contract: &str) {
        self.wr.contract = Some(contract.to_string());
    }
}

#[cfg(test)]
#[path = "web_reading_tests.rs"]
mod tests;
