//! --- background --- Sample posts for studio 0.64's Background rules:
//! Lin's thread published with two parts (a burst: "+2 parts"), Ada's post
//! of only quotes ("no words of its own") and Rue's note republished
//! unchanged. The fake works out their reasons as the studio's lens would
//! ([`entries`]), under the rules as set. All invented, on
//! `blyg.example.com`.

use blyg_core::background::{BURST, EntryBackground, Key, RULES};
use blyg_core::{Kind, ReadingItem, RemoteId, SubId};
use chrono::{DateTime, Duration, Utc};
use serde_json::json;

use super::reading_seed::{ADA, LIN, RUE};

/// Lin's thread, and the two parts published with it.
pub const LIN_BURST: &str = "01k211n0burst0000000000001";
pub const LIN_PART_1: &str = "01k211n0burst0000000000002";
pub const LIN_PART_2: &str = "01k211n0burst0000000000003";
/// Ada's post of only a quote.
pub const ADA_WORDLESS: &str = "01k2ada0w0rd1ess0000000001";
/// Rue's note, v2 the same as v1.
pub const RUE_SAME: &str = "01k2rve0same00000000000001";

/// The burst thread's key (the `rd-burst` scene opens it).
pub fn thread_key() -> Key {
    (SubId::from("sub-lin"), RemoteId::from(LIN_BURST))
}

#[allow(clippy::too_many_arguments)]
fn row(
    sub: &str,
    title: &str,
    origin: &str,
    id: &str,
    kind: Kind,
    version: u32,
    at: DateTime<Utc>,
    md: &str,
    quotes: &[&str],
) -> ReadingItem {
    let at = at.to_rfc3339();
    let mut r: ReadingItem = serde_json::from_value(json!({
        "subscription_id": sub, "remote_id": id, "subscription_title": title,
        "origin": origin, "kind": kind, "state": "current", "version": version,
        "created": at, "updated": at, "observed_at": at, "content_md": md,
        "content_html": blyg_render::render_markdown(md),
        "author": {"name": title, "url": origin}, "page": null, "thumb": null,
        "hoppers": [],
    }))
    .expect("a sample row");
    r.read_version = Some(version);
    r.transclusions = quotes
        .iter()
        .map(|q| serde_json::from_value(json!({ "id": q })).expect("a quote"))
        .collect();
    r
}

/// The sample rows (read, so no count changes).
pub fn rows(now: DateTime<Utc>) -> Vec<ReadingItem> {
    let (d, h) = (Duration::days, Duration::hours);
    let m = Duration::minutes;
    let lin = |id, kind, at, md: &str, quotes: &[&str]| {
        row("sub-lin", "Lin", LIN, id, kind, 1, at, md, quotes)
    };
    vec![
        lin(
            LIN_PART_1,
            Kind::Fragment,
            now - d(1) - m(9),
            "The tide clock at the ferry runs on the moon, not the town hall.",
            &[],
        ),
        lin(
            LIN_PART_2,
            Kind::Fragment,
            now - d(1) - m(6),
            "Low water uncovers the old slipway, stone by numbered stone.",
            &[],
        ),
        lin(
            LIN_BURST,
            Kind::Thread,
            now - d(1),
            &format!(
                "# A morning on the harbour wall\n\nTwo things I noticed on the walk, \
                 written up as I went.\n\n![[{LIN_PART_1}]]\n\n![[{LIN_PART_2}]]\n\n\
                 Both of them are about waiting for water."
            ),
            &[LIN_PART_1, LIN_PART_2],
        ),
        row(
            "sub-ada",
            "Ada",
            ADA,
            ADA_WORDLESS,
            Kind::Thread,
            1,
            now - d(1) - h(2),
            &format!("![[{LIN_PART_2}]]"),
            &[LIN_PART_2],
        ),
        row(
            "sub-rue",
            "Rue",
            RUE,
            RUE_SAME,
            Kind::Fragment,
            2,
            now - d(1) - h(3),
            "Reading lists are promises we make to a later self.",
            &[],
        ),
    ]
}

/// The sample posts' Background entries, as the studio's lens would list
/// them with `rules` (rule → on; a rule not listed is on).
pub fn entries(rules: &[(String, bool)]) -> Vec<(Key, EntryBackground)> {
    let on = |rule: &str| !rules.iter().any(|(r, v)| r == rule && !v);
    let key = |sub: &str, id: &str| (SubId::from(sub), RemoteId::from(id));
    let mut out = Vec::new();
    let mut add = |k: Key, reason: &str, detail: Option<serde_json::Value>| {
        if on(reason) {
            out.push((
                k,
                EntryBackground {
                    on: true,
                    reasons: vec![reason.into()],
                    detail,
                    ..Default::default()
                },
            ));
        }
    };
    for part in [LIN_PART_1, LIN_PART_2] {
        add(
            key("sub-lin", part),
            BURST,
            Some(json!({ "thread": LIN_BURST })),
        );
    }
    add(key("sub-ada", ADA_WORDLESS), "wordless", None);
    add(key("sub-rue", RUE_SAME), "maintenance", None);
    out
}

/// Every rule on, as a new studio 0.64 starts.
pub fn rules() -> Vec<(String, bool)> {
    RULES.iter().map(|r| (r.to_string(), true)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// User testing (R7): the burst thread's quotes show as the quoted
    /// words, as a blyg bakes them, never as raw `![[id]]` text.
    #[test]
    fn the_burst_thread_bakes_its_quotes() {
        let rows = rows(Utc::now());
        let thread = rows
            .iter()
            .find(|r| &*r.remote_id == LIN_BURST)
            .expect("the thread");
        assert!(
            !thread.content_html.contains("![["),
            "{}",
            thread.content_html
        );
        assert!(
            thread
                .content_html
                .contains(&format!("data-blyg-id=\"{LIN_PART_2}\"")),
            "{}",
            thread.content_html
        );
        assert!(
            thread
                .content_html
                .contains("Low water uncovers the old slipway")
        );
        let wordless = rows
            .iter()
            .find(|r| &*r.remote_id == ADA_WORDLESS)
            .expect("Ada's quote");
        assert!(!wordless.content_html.contains("![["));
    }

    /// User testing (R6): a post of only a quote is titled from what it
    /// quotes, not "…".
    #[test]
    fn a_post_of_only_a_quote_is_titled_from_the_quote() {
        let rows = rows(Utc::now());
        let wordless = rows
            .iter()
            .find(|r| &*r.remote_id == ADA_WORDLESS)
            .expect("Ada's quote");
        let title = crate::app::reading::vm::post_title(wordless);
        assert!(title.starts_with("Quotes: Low water uncovers"), "{title}");
    }
}
