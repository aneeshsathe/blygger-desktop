//! The sample data: the same invented posts `BLYGGER_FAKE=1` runs on
//! (ported from `crates/blyg-app/src/fake.rs`, `FakeBackend::with_timing`,
//! `sample_reading` and `studio_sample`), so the browser shows what the
//! native app shows in fake mode. All of it is made up; `blyg.example.com`
//! stands in for a blyg.

use blyg_core::{
    Author, HighlightMode, Hopper, Item, Kind, LocalId, ReadingItem, ResponsesMode, ServerId,
    Status, Subscription,
};
use chrono::{DateTime, Duration, Utc};

pub const ORIGIN: &str = "https://blyg.example.com";

/// The imported post a thread can quote (`fake.rs` `SAMPLE_QUOTE_ID`).
pub const SAMPLE_QUOTE_ID: &str = "01j9t4r7c8m2q5v6w3x8y9z0ab";
const SAMPLE_SUB: &str = "sub-shoreline";
const SAMPLE_ORIGIN: &str = "https://notes.example.org/";

/// A post's permalink on the sample blyg (`fake.rs` `permalink`).
pub fn permalink(kind: Kind, server_id: &str) -> String {
    let k = if kind == Kind::Thread { "t" } else { "f" };
    format!("{ORIGIN}/{k}/{server_id}")
}

/// A fresh item with the fake's defaults.
pub fn blank(id: &str, kind: Kind, status: Status, content_md: &str, at: &str) -> Item {
    Item {
        local_id: LocalId(id.to_string()),
        server_id: None,
        kind,
        status,
        version: 0,
        dirty: false,
        content_md: content_md.to_string(),
        created: at.to_string(),
        updated: at.to_string(),
        permalink: None,
        stub_of: None,
        forked_from: None,
        show_responses: true,
        responses_mode: Some(ResponsesMode::Show),
        pending_sync: false,
        conflict: false,
        highlight: false,
        highlight_mode: Some(HighlightMode::Default),
    }
}

/// The full editor's sample thread (`fake.rs` `studio_sample`): a quote, a
/// generated TK span, a YouTube link and a quote that can't be resolved.
pub fn studio_sample() -> String {
    format!(
        "Notes from the tide pools\n\n\
         Low tide came at six, and the rock shelf opened like a *book*.\n\n\
         ![[{SAMPLE_QUOTE_ID}]]\n\n\
         [TK]one sentence on why the pools reset[=]Twice a day the sea refills every pool, so each low tide starts a fresh page.[/TK]\n\n\
         The best short film about it is still this one:\n\n\
         https://www.youtube.com/watch?v=Qa1b2C3d4E5\n\n\
         ![[0123456789abcdefghjkmnpqrs]]\n\n\
         More after the next [low tide](https://example.com/tides)."
    )
}

/// The six posts of the native fake, then the full editor's sample thread
/// as a draft, newest first.
pub fn items(now: DateTime<Utc>) -> Vec<Item> {
    let ago = |d: Duration| (now - d).to_rfc3339();
    let seed: [(&str, Kind, Status, u32, Duration, &str); 6] = [
        (
            "01J9QK3",
            Kind::Fragment,
            Status::Draft,
            0,
            Duration::minutes(2),
            "A lighthouse keeper's log is mostly weather, and that is the point.",
        ),
        (
            "01J9PX1",
            Kind::Fragment,
            Status::Public,
            1,
            Duration::days(2),
            "Band on Harbour Road: the trombone came in half a beat late and the tune was better for it.",
        ),
        (
            "01J9M2A",
            Kind::Fragment,
            Status::Public,
            3,
            Duration::days(4),
            "On friction: every extra step between noticing something and writing it down is a chance to lose it.",
        ),
        (
            "01J9K7T",
            Kind::Thread,
            Status::Public,
            2,
            Duration::days(6),
            "Notes on The Field Guide to Small Gardens\n\nThe guide argues that a garden is a *conversation* before it is a plan. Three things stuck with me…\n\n1. Soil matters more than seeds.\n2. Watering is a habit, not a chore.\n3. Weeds are **information**.\n\n> A garden is something you do together with the weather.\n\nMore in [the guide](https://example.com/field-guide).",
        ),
        (
            "01J9H4C",
            Kind::Thread,
            Status::Draft,
            0,
            Duration::days(9),
            "Slow mornings: a reply to a letter from a friend\n\nThe letter frames it as a question of time. I think it's rhythm…",
        ),
        (
            "01J9E8B",
            Kind::Fragment,
            Status::Public,
            1,
            Duration::days(13),
            "Marginalia were comment threads before paper had a word for it.",
        ),
    ];
    let mut out: Vec<Item> = seed
        .into_iter()
        .map(|(id, kind, status, version, age, body)| {
            let at = ago(age);
            let server = format!("{id}ABCDEFGHJKMNPQRS");
            let public = status == Status::Public;
            Item {
                server_id: Some(ServerId(server.clone())),
                version,
                permalink: public.then(|| permalink(kind, &server)),
                ..blank(id, kind, status, body, &at)
            }
        })
        .collect();
    let at = ago(Duration::days(1));
    out.push(blank(
        "01J9T1D",
        Kind::Thread,
        Status::Draft,
        &studio_sample(),
        &at,
    ));
    out
}

/// The native fake's reading data (subscriptions, the reading list with
/// stubs, forks and quotes between posts, hoppers), compiled unchanged.
/// It names the sample blyg as `super::ORIGIN`.
#[path = "../../blyg-app/src/fake_reading.rs"]
#[allow(dead_code)]
pub mod reading_seed;

/// Everything the Read tab shows: the fake's reading seed plus the
/// shoreline post, with hopper membership filled in from the seed.
pub struct Reading {
    pub rows: Vec<ReadingItem>,
    pub subs: Vec<Subscription>,
    pub hoppers: Vec<Hopper>,
}

pub fn reading(now: DateTime<Utc>) -> Reading {
    let s = reading_seed::seed(now);
    let mut rows = s.reading;
    rows.push(shoreline(now));
    for (hopper, sub, id) in &s.hopper_items {
        if let Some(r) = rows
            .iter_mut()
            .find(|r| &r.subscription_id == sub && &r.remote_id == id)
        {
            r.hoppers.push(hopper.clone());
        }
    }
    Reading {
        rows,
        subs: s.subs,
        hoppers: s.hoppers,
    }
}

/// The shoreline post (`fake.rs` `sample_reading`).
fn shoreline(now: DateTime<Utc>) -> ReadingItem {
    let when = (now - Duration::days(1)).to_rfc3339();
    ReadingItem {
        subscription_id: SAMPLE_SUB.into(),
        remote_id: SAMPLE_QUOTE_ID.into(),
        subscription_title: "Shoreline Notes".into(),
        origin: SAMPLE_ORIGIN.into(),
        kind: Kind::Fragment,
        state: "current".into(),
        version: 2,
        created: Some(when.clone()),
        updated: Some(when.clone()),
        observed_at: when,
        content_md: "A tide pool is a small ocean that forgets, twice a day, that it belongs to a larger one.".into(),
        content_html: "<p>A tide pool is a small ocean that forgets, twice a day, that it belongs to a larger one.</p>\n".into(),
        author: Some(Author {
            name: Some("Shoreline Notes".into()),
            extra: Default::default(),
            url: None,
        }),
        page: None,
        thumb: None,
        hoppers: vec![],
        pinned_version_retained: None,
        read_version: None,
        stub_of: None,
        forked_from: None,
        transclusions: vec![],
        lineage_known: false,
        source_url: None,
        generated: None,
    }
}
