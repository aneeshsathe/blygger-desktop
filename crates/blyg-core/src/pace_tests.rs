//! Studio 0.68's pace-layered reader order (`src/feeds/pace-layered.ts`,
//! `src/feeds/reader-order.ts`), rule by rule. Invented items on one or two
//! sample blygs.

use serde_json::json;

use super::*;

const MIN: i64 = 60_000;
const T0: i64 = 1_800_000_000_000;

fn k(id: &str) -> Key {
    ("sub-a".into(), id.into())
}

fn ids(keys: &[Key]) -> Vec<&str> {
    keys.iter().map(|k| k.1.as_str()).collect()
}

fn v(version: u32, at: i64) -> PaceVersion {
    PaceVersion {
        version,
        at,
        pinned: false,
        quotes: vec![],
        maintenance: false,
    }
}

fn frag(id: &str, versions: Vec<PaceVersion>) -> PaceItem {
    PaceItem {
        key: k(id),
        thread: false,
        withdrawn: false,
        versions,
    }
}

#[test]
fn new_items_arrive_on_top() {
    let items = [
        frag("a", vec![v(1, T0)]),
        frag("b", vec![v(1, T0 + MIN)]),
        frag("c", vec![v(1, T0 + 2 * MIN)]),
    ];
    assert_eq!(ids(&pace_layered_order(&items)), ["c", "b", "a"]);
}

#[test]
fn a_new_version_climbs_one_place() {
    let items = [
        frag("a", vec![v(1, T0), v(2, T0 + 3 * MIN)]),
        frag("b", vec![v(1, T0 + MIN)]),
        frag("c", vec![v(1, T0 + 2 * MIN)]),
    ];
    assert_eq!(ids(&pace_layered_order(&items)), ["c", "a", "b"]);
}

#[test]
fn a_maintenance_version_climbs_none() {
    let mut v2 = v(2, T0 + 3 * MIN);
    v2.maintenance = true;
    assert_eq!(climb_for(&v2), 0);
    let items = [
        frag("a", vec![v(1, T0), v2]),
        frag("b", vec![v(1, T0 + MIN)]),
        frag("c", vec![v(1, T0 + 2 * MIN)]),
    ];
    assert_eq!(ids(&pace_layered_order(&items)), ["c", "b", "a"]);
}

/// A pinned item is a layer down: it sinks half as fast.
#[test]
fn pins_slow_the_sinking() {
    let mut pinned = v(1, T0);
    pinned.pinned = true;
    let items = [
        frag("a", vec![pinned]),
        frag("b", vec![v(1, T0 + MIN)]),
        frag("c", vec![v(1, T0 + 2 * MIN)]),
        frag("d", vec![v(1, T0 + 3 * MIN)]),
    ];
    // a sank 3 ticks at 0.5, b 2 ticks at 1.
    assert_eq!(ids(&pace_layered_order(&items)), ["d", "c", "a", "b"]);
    assert_eq!(sink_speed(0), 1.0);
    assert_eq!(sink_speed(1), 0.5);
    assert_eq!(layer_for(0.99), 0);
    assert_eq!(layer_for(1.0), 1);
    assert_eq!(layer_for(40.0), LAYERS.len() - 1);
}

/// A pinned thread's weight passes half to what it quotes.
#[test]
fn a_pinned_quote_lends_half_its_weight() {
    let mut t = v(1, T0 + MIN);
    t.pinned = true;
    t.quotes = vec![k("q")];
    let mut t2 = v(2, T0 + 2 * MIN);
    t2.pinned = true;
    t2.quotes = vec![k("q")];
    // q is quoted by t's pins: half of t's weight, 2/2 = 1, layer 1.
    let items = [
        frag("q", vec![v(1, T0)]),
        PaceItem {
            key: k("t"),
            thread: false,
            withdrawn: false,
            versions: vec![t, t2],
        },
        frag("x", vec![v(1, T0 + 3 * MIN)]),
        frag("y", vec![v(1, T0 + 4 * MIN)]),
        frag("z", vec![v(1, T0 + 5 * MIN)]),
    ];
    // t, pinned twice, is two layers down (a quarter per tick); q, at
    // weight 1, sinks at half: 2.5 deep after five ticks, not 4.
    assert_eq!(ids(&pace_layered_order(&items)), ["z", "t", "y", "x", "q"]);
    let pins = HashMap::from([(1usize, 2.0)]);
    let quoters = HashMap::from([(0usize, BTreeSet::from([1usize]))]);
    let w = pin_weights(&pins, &quoters);
    assert_eq!((w[&1], w[&0]), (2.0, 1.0));
}

/// A thread published with its parts enters as one, on top, its parts
/// beneath it in publication order (RFC-3 R3).
#[test]
fn a_thread_and_its_parts_enter_together() {
    let mut t = v(1, T0 + 10 * MIN);
    t.quotes = vec![k("p1"), k("p2")];
    let items = [
        frag("x", vec![v(1, T0)]),
        frag("p1", vec![v(1, T0 + 5 * MIN)]),
        frag("p2", vec![v(1, T0 + 8 * MIN)]),
        PaceItem {
            key: k("t"),
            thread: true,
            withdrawn: false,
            versions: vec![t],
        },
    ];
    assert_eq!(ids(&pace_layered_order(&items)), ["t", "p1", "p2", "x"]);
    // A part published more than 15 minutes before is its own arrival.
    let mut late = items.clone();
    late[1].versions[0].at = T0 + 10 * MIN - BURST_WINDOW_MS - 1;
    late[0].versions[0].at = T0 + 10 * MIN - BURST_WINDOW_MS - 2;
    assert_eq!(ids(&pace_layered_order(&late)), ["t", "p2", "p1", "x"]);
}

#[test]
fn withdrawn_items_are_left_out_of_the_replay_and_listed_last() {
    let mut gone = frag("gone", vec![v(1, T0 + 5 * MIN)]);
    gone.withdrawn = true;
    let items = [
        frag("a", vec![v(1, T0)]),
        gone,
        frag("b", vec![v(1, T0 + MIN)]),
    ];
    assert_eq!(ids(&pace_layered_order(&items)), ["b", "a"]);
    assert_eq!(ids(&reader_positions(&items)), ["b", "a", "gone"]);
}

/// The replay ranks the newest 500 entries; the rest follow newest first.
#[test]
fn the_replay_ranks_the_newest_500_and_the_rest_follow_newest_first() {
    assert_eq!(WINDOW, 500);
    let mut items: Vec<PaceItem> = (0..WINDOW + 2)
        .map(|i| frag(&format!("i{i:04}"), vec![v(1, T0 + i as i64 * MIN)]))
        .collect();
    // The oldest is pinned: inside the window it would sink slowly and
    // stay above the others, but it is outside, so it stays last.
    items[0].versions[0].pinned = true;
    let order = reader_positions(&items);
    assert_eq!(order.len(), WINDOW + 2);
    assert_eq!(order[0].1.as_str(), "i0501");
    assert_eq!(ids(&order[WINDOW..]), ["i0001", "i0000"]);
}

fn row(id: &str, version: u32, created: &str, updated: &str, observed: &str) -> ReadingItem {
    serde_json::from_value(json!({
        "subscription_id": "sub-a", "remote_id": id, "subscription_title": "Sample",
        "origin": "https://blyg.example.com/", "kind": "thread", "state": "current",
        "version": version, "created": created, "updated": updated, "observed_at": observed,
        "content_md": "", "content_html": "", "author": null, "page": null, "thumb": null,
        "hoppers": [],
        "transclusions": [
            {"id": "same", "version": 1},
            {"id": "here", "version": 2, "origin": "https://blyg.example.com"},
            {"id": "other", "version": 1, "origin": "https://other.example.com/"},
        ],
    }))
    .unwrap()
}

/// Studio `importedVersions` for an item with no changelog: its first and
/// latest versions, times clamped to when this reader saw it, its own
/// blyg's quotes only, and the current version maintenance when the
/// studio's Background says so.
#[test]
fn a_row_without_a_changelog_counts_as_its_first_and_latest_versions() {
    let mut r = row(
        "t",
        3,
        "2026-11-01T10:00:00Z",
        "2027-01-01T00:00:00Z", // future-dated: clamped
        "2026-11-02T08:00:00Z",
    );
    let vs = row_versions(&r, None);
    assert_eq!(vs.len(), 2);
    assert_eq!((vs[0].version, vs[1].version), (1, 3));
    assert_eq!(vs[0].at, parse_ts_ms("2026-11-01T10:00:00Z").unwrap());
    assert_eq!(vs[1].at, parse_ts_ms("2026-11-02T08:00:00Z").unwrap());
    assert!(vs[0].quotes.is_empty(), "only the current version quotes");
    assert_eq!(
        ids(&vs[1].quotes),
        ["same", "here"],
        "another blyg's quote doesn't count"
    );
    assert!(!vs[1].maintenance);
    r.background = Some(crate::background::EntryBackground {
        on: true,
        reasons: vec!["near-same".into()],
        ..Default::default()
    });
    assert!(row_versions(&r, None)[1].maintenance);
}

/// With a cached changelog, its versions and pins (the blyg's own pins:
/// decision #73), up to the version held.
#[test]
fn a_cached_changelog_gives_every_version_and_its_pins() {
    let r = row(
        "t",
        2,
        "2026-11-01T10:00:00Z",
        "2026-11-01T12:00:00Z",
        "2026-11-02T00:00:00Z",
    );
    let log: Vec<RemoteVersion> = serde_json::from_value(json!([
        {"version": 1, "at": "2026-11-01T10:00:00Z", "note": null, "pinned": true, "current": false},
        {"version": 2, "at": "2026-11-01T12:00:00Z", "note": "more", "pinned": false, "current": true},
        {"version": 3, "at": "2026-11-01T13:00:00Z", "note": null, "pinned": false, "current": false},
    ]))
    .unwrap();
    let vs = row_versions(&r, Some(&log));
    assert_eq!(vs.iter().map(|v| v.version).collect::<Vec<_>>(), [1, 2]);
    assert!(vs[0].pinned && !vs[1].pinned);
    assert_eq!(vs[0].quotes.len(), 2, "a pinned version carries the quotes");
}

#[test]
fn history_lists_each_row_once_in_key_order() {
    let a = row(
        "b",
        1,
        "2026-11-01T10:00:00Z",
        "2026-11-01T10:00:00Z",
        "2026-11-01T10:00:00Z",
    );
    let b = row(
        "a",
        1,
        "2026-11-01T11:00:00Z",
        "2026-11-01T11:00:00Z",
        "2026-11-01T11:00:00Z",
    );
    let h = history([&a, &b, &a], |_| None);
    assert_eq!(
        h.iter().map(|i| i.key.1.as_str()).collect::<Vec<_>>(),
        ["a", "b"]
    );
    assert!(h[0].thread);
}
