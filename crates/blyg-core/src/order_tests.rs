use super::*;
use crate::model::{Kind, LocalId, ServerId, Status};

fn rrow(sub: &str, id: &str, updated: Option<&str>, observed: &str) -> ReadingItem {
    ReadingItem {
        subscription_id: sub.into(),
        remote_id: id.into(),
        subscription_title: "Sample".into(),
        origin: "https://blyg.example.com/".into(),
        kind: Kind::Fragment,
        state: "current".into(),
        version: 1,
        created: updated.map(str::to_string),
        updated: updated.map(str::to_string),
        observed_at: observed.into(),
        content_md: format!("Sample note {id}"),
        content_html: String::new(),
        author: None,
        page: None,
        thumb: None,
        hoppers: vec![],
        pinned_version_retained: None,
        read_version: None,
        stub_of: None,
        forked_from: None,
        transclusions: vec![],
        lineage_known: true,
        source_url: None,
        generated: None,
        background: None,
    }
}

fn item(sid: Option<&str>, updated: &str) -> Item {
    Item {
        local_id: LocalId(format!("local-{}", sid.unwrap_or("new"))),
        server_id: sid.map(|s| ServerId(s.into())),
        kind: Kind::Fragment,
        status: Status::Public,
        version: 1,
        dirty: false,
        content_md: "Own post".into(),
        created: updated.into(),
        updated: updated.into(),
        permalink: None,
        stub_of: None,
        forked_from: None,
        show_responses: false,
        responses_mode: None,
        pending_sync: false,
        conflict: false,
        highlight: false,
        highlight_mode: None,
    }
}

fn ids(rows: &[ReadingItem]) -> Vec<String> {
    rows.iter().map(|r| r.remote_id.to_string()).collect()
}

#[test]
fn reading_is_newest_first_by_the_posts_own_date() {
    let mut v = vec![
        rrow(
            "s1",
            "a",
            Some("2026-11-15T10:00:00Z"),
            "2026-11-15T12:00:00Z",
        ),
        rrow(
            "s1",
            "b",
            Some("2026-11-15T11:00:00Z"),
            "2026-11-15T12:00:00Z",
        ),
        rrow("s1", "c", None, "2026-11-15T10:30:00Z"),
    ];
    sort_reading(&mut v);
    assert_eq!(ids(&v), ["b", "c", "a"]);
}

#[test]
fn reading_ties_go_by_subscription_then_id_descending() {
    let at = Some("2026-11-15T10:00:00Z");
    let obs = "2026-11-15T10:00:00Z";
    let mut v = vec![
        rrow("s1", "01A", at, obs),
        rrow("s2", "01A", at, obs),
        rrow("s1", "01C", at, obs),
        rrow("s1", "01B", at, obs),
    ];
    sort_reading(&mut v);
    let keys: Vec<_> = v
        .iter()
        .map(|r| format!("{}|{}", r.subscription_id, r.remote_id))
        .collect();
    assert_eq!(keys, ["s2|01A", "s1|01C", "s1|01B", "s1|01A"]);
    // Whatever order they came in.
    let mut w = v.clone();
    w.reverse();
    sort_reading(&mut w);
    assert_eq!(w, v);
}

#[test]
fn reading_dates_in_any_spelling_compare_as_instants() {
    let obs = "2026-11-15T12:00:00Z";
    let mut v = vec![
        rrow("s", "z", Some("2026-11-15T10:00:00Z"), obs),
        rrow("s", "offset", Some("2026-11-15T11:30:00+01:00"), obs), // 10:30Z
        rrow("s", "millis", Some("2026-11-15T10:00:00.500+00:00"), obs),
    ];
    sort_reading(&mut v);
    assert_eq!(ids(&v), ["offset", "millis", "z"]);
}

#[test]
fn a_backfill_observed_in_one_second_sorts_by_the_posts_dates() {
    // A backfill imports every post in the same second, and the server
    // lists them by `observed_at`, then its (random) ids: that order says
    // nothing. The list goes by each post's own date.
    let obs = "2026-11-15T18:30:43Z";
    let n = 3000;
    let mut v: Vec<ReadingItem> = (0..n)
        .map(|i| {
            // Ids that don't follow the dates (a scrambled permutation).
            let id = format!("{:05}", (i * 7919) % n);
            let at = crate::util::iso_from_ms(1_763_000_000_000 + i as i64 * 1000);
            rrow("sub", &id, Some(&at), obs)
        })
        .collect();
    v.sort_by(|a, b| b.remote_id.cmp(&a.remote_id)); // the server's order
    sort_reading(&mut v);
    let dates: Vec<_> = v.iter().map(ReadingItem::sort_at).collect();
    let mut want = dates.clone();
    want.sort_by(|a, b| b.cmp(a));
    assert_eq!(dates, want, "newest first");
    assert_eq!(v[0].remote_id, format!("{:05}", ((n - 1) * 7919) % n));
    // Several posts in one second: still one order.
    let mut same: Vec<ReadingItem> = (0..50)
        .map(|i| rrow("sub", &format!("{:03}", (i * 37) % 50), Some(obs), obs))
        .collect();
    sort_reading(&mut same);
    let got = ids(&same);
    let mut want: Vec<String> = (0..50).map(|i| format!("{i:03}")).collect();
    want.reverse();
    assert_eq!(got, want);
}

#[test]
fn reading_cmp_agrees_with_sort_reading() {
    let obs = "2026-11-15T10:00:00Z";
    let a = rrow("s", "a", Some("2026-11-15T10:00:01Z"), obs);
    let b = rrow("s", "b", Some("2026-11-15T10:00:00Z"), obs);
    assert_eq!(reading_cmp(&a, &b), Ordering::Less);
    assert_eq!(reading_cmp(&b, &a), Ordering::Greater);
    assert_eq!(reading_cmp(&a, &a), Ordering::Equal);
}

fn sids(v: &[Item]) -> Vec<Option<String>> {
    v.iter()
        .map(|i| i.server_id.as_ref().map(|s| s.0.clone()))
        .collect()
}

#[test]
fn own_posts_in_one_second_go_by_server_id_descending() {
    let t = "2026-11-15T18:30:43Z";
    let mut v = vec![
        item(Some("01A03"), t),
        item(Some("01A13"), t),
        item(Some("01A05"), t),
    ];
    sort_own(&mut v);
    let want = |s: &[&str]| s.iter().map(|x| Some(x.to_string())).collect::<Vec<_>>();
    assert_eq!(sids(&v), want(&["01A13", "01A05", "01A03"]));
    v.reverse();
    sort_own(&mut v);
    assert_eq!(sids(&v), want(&["01A13", "01A05", "01A03"]));
}

#[test]
fn own_dates_compare_as_instants_not_text() {
    // A pending local edit (`+00:00`, milliseconds) a moment after the
    // server's whole-second `Z`: as text, "…:43Z" > "…:43.123+00:00".
    let mut v = vec![
        item(Some("01A01"), "2026-11-15T18:30:43Z"),
        item(Some("01A02"), "2026-11-15T18:30:43.123+00:00"),
        item(Some("01A03"), "2026-11-15T19:30:42+01:00"), // 18:30:42Z
    ];
    sort_own(&mut v);
    let want = |s: &[&str]| s.iter().map(|x| Some(x.to_string())).collect::<Vec<_>>();
    assert_eq!(sids(&v), want(&["01A02", "01A01", "01A03"]));
    assert_eq!(own_cmp(&v[0], &v[1]), Ordering::Less);
}

#[test]
fn your_rows_merge_into_the_list_by_the_same_order() {
    let obs = "2026-11-15T12:00:00Z";
    let mut theirs = vec![
        rrow("s1", "a", Some("2026-11-15T11:00:00Z"), obs),
        rrow("s1", "b", Some("2026-11-15T10:00:00Z"), obs),
        rrow("s1", "c", Some("2026-11-15T09:00:00Z"), obs),
    ];
    sort_reading(&mut theirs);
    let mine = |id: &str, at: &str| {
        let i = item(Some(id), at);
        crate::own::row(&i, "https://blyg.example.com").unwrap()
    };
    let own = vec![
        mine("o1", "2026-11-15T09:00:00Z"), // ties with c: "~own" > "s1"
        mine("o2", "2026-11-15T10:30:00+00:00"),
    ];
    let merged = crate::own::merge(theirs.clone(), own.clone());
    assert_eq!(ids(&merged), ["a", "o2", "b", "o1", "c"]);
    // The same as sorting everything at once.
    let mut all = theirs;
    all.extend(own);
    sort_reading(&mut all);
    assert_eq!(all, merged);
    // Never unread, never thumbed.
    assert!(
        merged
            .iter()
            .filter(|r| crate::own::is_own_row(r))
            .all(|r| !r.is_unread() && r.thumb.is_none())
    );
}

#[test]
fn own_posts_without_a_date_or_server_id_still_sort_totally() {
    let mut v = vec![
        item(None, "2026-11-15T18:30:43Z"),
        item(Some("01A01"), "not a date"),
        item(Some("01A02"), "2026-11-15T18:30:43Z"),
    ];
    sort_own(&mut v);
    assert_eq!(
        sids(&v),
        vec![Some("01A02".to_string()), None, Some("01A01".to_string())]
    );
}

// ---- the Feed's order (studio 0.68, `readerOrder`) ----

/// `Settings.readerOrder`: read when the studio has it, pace-layered by
/// default, written alone (a partial PATCH keeps every other setting).
#[test]
fn reader_order_reads_and_writes_the_setting_alone() {
    use serde_json::json;
    assert_eq!(ReaderOrder::default(), ReaderOrder::PaceLayered);
    assert_eq!(
        ReaderOrder::from_settings(&json!({"readerOrder": "newest"})),
        Some(ReaderOrder::Newest)
    );
    assert_eq!(
        ReaderOrder::from_settings(&json!({"readerOrder": "pace-layered"})),
        Some(ReaderOrder::PaceLayered)
    );
    assert_eq!(
        ReaderOrder::from_settings(&json!({"readerOrder": "someday"})),
        Some(ReaderOrder::PaceLayered),
        "an unknown value reads as the default"
    );
    assert_eq!(ReaderOrder::from_settings(&json!({"siteTitle": "x"})), None);
    assert_eq!(
        ReaderOrder::Newest.patch(),
        json!({"readerOrder": "newest"}),
        "only the one key"
    );
    assert_eq!(ReaderOrder::Newest.label(), "Chronological");
    assert_eq!(ReaderOrder::PaceLayered.label(), "Pace-layered");
}

/// The Feed's places over reading rows: newest on top, and an edit that
/// isn't maintenance climbs one place (not to the top, as chronological
/// would put it).
#[test]
fn the_pace_layered_feed_climbs_an_edit_one_place() {
    let mut a = rrow(
        "s",
        "a",
        Some("2026-11-01T10:00:00Z"),
        "2026-11-01T10:00:00Z",
    );
    let b = rrow(
        "s",
        "b",
        Some("2026-11-01T11:00:00Z"),
        "2026-11-01T11:00:00Z",
    );
    let c = rrow(
        "s",
        "c",
        Some("2026-11-01T12:00:00Z"),
        "2026-11-01T12:00:00Z",
    );
    a.version = 2;
    a.updated = Some("2026-11-01T13:00:00Z".into());
    a.observed_at = "2026-11-01T13:00:00Z".into();
    let pos = feed_positions([&a, &b, &c], |_| None, |_| false);
    let mut keys: Vec<&Key> = pos.keys().collect();
    keys.sort_by_key(|k| pos[*k]);
    let ids: Vec<&str> = keys.iter().map(|k| k.1.as_str()).collect();
    assert_eq!(ids, ["c", "a", "b"]);
    // Chronological puts the edit on top.
    let mut rows = vec![b.clone(), c.clone(), a.clone()];
    sort_reading(&mut rows);
    assert_eq!(rows[0].remote_id.as_str(), "a");
}
