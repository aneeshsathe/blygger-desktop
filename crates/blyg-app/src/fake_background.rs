//! --- background --- Sample posts for studio 0.64's Background rules:
//! Lin's thread published with two parts (a burst: "+2 parts"), Ada's post
//! of only quotes ("no words of its own") and Rue's note republished
//! unchanged; and studio 0.68's measured rules: Ada's typo fix (a small
//! edit), Rue's threads whose quotes were refreshed (one with a quote
//! rewritten), and Lin's note followed within minutes by another. The fake
//! works out their reasons as the studio's lens would ([`entries`]), with
//! the studio's own measures (`blyg_core::change_size::is_near_same`,
//! `blyg_core::background::supersedes`), under the rules as set. All
//! invented, on `blyg.example.com`.

use blyg_core::background::{
    BURST, BackgroundRules, EntryBackground, Key, QUOTES_MATERIAL, QUOTES_REFRESHED, RULES,
    SUPERSEDED, supersedes,
};
use blyg_core::change_size::is_near_same;
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

// --- studio 0.68's measured rules ---
/// Ada's post, v2 a typo fix of v1 (a small edit).
pub const ADA_SMALL: &str = "01k2ada0sma11edit000000001";
/// Lin's quick note, and the one Lin followed it with six minutes later.
pub const LIN_DRAFT: &str = "01k211n0draft0000000000001";
pub const LIN_FOLLOW: &str = "01k211n0f0110w000000000001";
/// Rue's thread quoting Ada's post: v2 refreshed the quote to Ada's v2.
pub const RUE_REFRESH: &str = "01k2rve0refresh00000000001";
/// Rue's thread quoting Lin's follow-up, whose text Lin rewrote.
pub const RUE_REWRITE: &str = "01k2rve0rewr1te00000000001";

const ADA_SMALL_V1: &str = "Every harbour keeps two clocks. One is on the tower and runs on \
    the town's time; the other is the water itself, which keeps teh moon's. Fishermen read \
    the second one without looking, the way you know a stair in the dark. Visitors read \
    the first and miss the boat.";
const ADA_SMALL_V2: &str = "Every harbour keeps two clocks. One is on the tower and runs on \
    the town's time; the other is the water itself, which keeps the moon's. Fishermen read \
    the second one without looking, the way you know a stair in the dark. Visitors read \
    the first and miss the boat.";
const LIN_FOLLOW_V1: &str = "The ferry's bell rings twice for the last crossing.";
const LIN_FOLLOW_V2: &str = "Nobody rings the ferry bell any more; a recording plays at dusk.";

/// The burst thread's key (the `rd-burst` scene opens it).
pub fn thread_key() -> Key {
    (SubId::from("sub-lin"), RemoteId::from(LIN_BURST))
}

#[expect(
    clippy::too_many_arguments,
    reason = "one argument per field it fills; a struct would only rename them"
)]
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
    let mut rows = vec![
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
        // --- studio 0.68's measured rules ---
        row(
            "sub-ada",
            "Ada",
            ADA,
            ADA_SMALL,
            Kind::Fragment,
            2,
            now - d(2),
            ADA_SMALL_V2,
            &[],
        ),
        lin(
            LIN_DRAFT,
            Kind::Fragment,
            now - d(1) - h(6),
            "Ferry notes, part one: the bell.",
            &[],
        ),
        row(
            "sub-lin",
            "Lin",
            LIN,
            LIN_FOLLOW,
            Kind::Fragment,
            2,
            now - d(1) - h(6) + m(6),
            LIN_FOLLOW_V2,
            &[],
        ),
        row(
            "sub-rue",
            "Rue",
            RUE,
            RUE_REFRESH,
            Kind::Thread,
            2,
            now - d(1) - h(4),
            &format!("# Two clocks\n\nAda on harbour time:\n\n![[{ADA_SMALL}]]"),
            &[ADA_SMALL],
        ),
        row(
            "sub-rue",
            "Rue",
            RUE,
            RUE_REWRITE,
            Kind::Thread,
            2,
            now - d(1) - h(5),
            &format!("# The ferry bell\n\nLin heard it:\n\n![[{LIN_FOLLOW}]]"),
            &[LIN_FOLLOW],
        ),
    ];
    bake_quotes(&mut rows);
    rows
}

/// What a refreshed quote's text change makes of the refresh (studio
/// `refreshReason`): rewritten beyond a small edit is `quotes-material`.
fn refresh_reason(was: &str, now: &str) -> &'static str {
    if is_near_same(was, now) {
        QUOTES_REFRESHED
    } else {
        QUOTES_MATERIAL
    }
}

/// Each `![[id]]` line's quote baked into the HTML as a blyg publishes it
/// (a `blyg-transclusion` snapshot of the quoted post), so the Reader pane
/// shows the quoted words (user testing, R7).
fn bake_quotes(rows: &mut [ReadingItem]) {
    let texts: Vec<(String, String, String)> = rows
        .iter()
        .map(|r| {
            (
                r.remote_id.to_string(),
                r.origin.clone(),
                r.content_md.clone(),
            )
        })
        .collect();
    for r in rows.iter_mut() {
        for (id, origin, md) in &texts {
            let line = format!("<p>![[{id}]]</p>");
            if r.content_html.contains(&line) {
                let quote = format!(
                    "<blockquote class=\"blyg-transclusion\" data-blyg-id=\"{id}\" \
                     data-blyg-version=\"1\" data-blyg-origin=\"{origin}\">\n{}</blockquote>",
                    blyg_render::render_markdown(md)
                );
                r.content_html = r.content_html.replace(&line, &quote);
            }
        }
    }
}

/// The sample posts' Background entries, as the studio's lens would list
/// them with `rules` (rule → on; a rule not listed is on).
pub fn entries(rules: &[(String, bool)]) -> Vec<(Key, EntryBackground)> {
    let set = BackgroundRules {
        rules: rules.to_vec(),
    };
    let on = |rule: &str| set.counts(rule);
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
    // --- studio 0.68's measured rules ---
    if is_near_same(ADA_SMALL_V1, ADA_SMALL_V2) {
        add(key("sub-ada", ADA_SMALL), "near-same", None);
    }
    // Lin's follow-up came six minutes after the draft was last active.
    if supersedes(6 * 60_000, 0) {
        add(
            key("sub-lin", LIN_DRAFT),
            SUPERSEDED,
            Some(json!({ "by": LIN_FOLLOW })),
        );
    }
    add(
        key("sub-rue", RUE_REFRESH),
        refresh_reason(ADA_SMALL_V1, ADA_SMALL_V2),
        None,
    );
    add(
        key("sub-rue", RUE_REWRITE),
        refresh_reason(LIN_FOLLOW_V1, LIN_FOLLOW_V2),
        None,
    );
    out
}

/// Every rule on, as a new studio 0.68 starts.
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

    /// Studio 0.68's measured rules on the sample posts, by the studio's
    /// own measures, and "including rewritten quotes" only under "quotes
    /// refreshed".
    #[test]
    fn the_measured_rules_place_the_samples_as_the_studio_would() {
        let reason = |rules: &[(String, bool)], id: &str| {
            entries(rules)
                .into_iter()
                .find(|(k, _)| &*k.1 == id)
                .map(|(_, e)| e.reasons[0].clone())
        };
        let all = rules();
        assert_eq!(reason(&all, ADA_SMALL).as_deref(), Some("near-same"));
        assert_eq!(reason(&all, LIN_DRAFT).as_deref(), Some("superseded"));
        assert_eq!(reason(&all, LIN_FOLLOW), None, "the newer one stays");
        assert_eq!(
            reason(&all, RUE_REFRESH).as_deref(),
            Some("quotes-refreshed")
        );
        assert_eq!(
            reason(&all, RUE_REWRITE).as_deref(),
            Some("quotes-material")
        );
        let with = |rule: &str, on: bool| -> Vec<(String, bool)> {
            all.iter()
                .map(|(r, v)| (r.clone(), if r == rule { on } else { *v }))
                .collect()
        };
        assert_eq!(reason(&with("quotes-material", false), RUE_REWRITE), None);
        let refreshed_off = with("quotes-refreshed", false);
        assert_eq!(reason(&refreshed_off, RUE_REWRITE), None);
        assert_eq!(reason(&refreshed_off, RUE_REFRESH), None);
        assert_eq!(reason(&with("near-same", false), ADA_SMALL), None);
        assert_eq!(reason(&with("superseded", false), LIN_DRAFT), None);
        assert_eq!(rows(Utc::now()).len(), 10);
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
