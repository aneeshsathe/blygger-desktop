use serde_json::json;

use super::*;

fn imported(sub: &str, id: &str, background: Value) -> Value {
    json!({
        "key": format!("imported:[\"{sub}\",\"{id}\"]"),
        "source": "imported",
        "imported": { "subscriptionId": sub, "remoteId": id },
        "background": background,
    })
}

fn own(id: &str, background: Value) -> Value {
    json!({ "key": format!("own:{id}"), "source": "own", "own": { "id": id }, "background": background })
}

fn k(sub: &str, id: &str) -> Key {
    (sub.into(), id.into())
}

fn row(sub: &str, id: &str, state: &str) -> ReadingItem {
    let mut r: ReadingItem = serde_json::from_value(json!({
        "subscription_id": sub, "remote_id": id, "subscription_title": "Sample",
        "origin": "https://blyg.example.com/", "kind": "fragment", "state": state,
        "version": 1, "created": null, "updated": null, "observed_at": "2026-11-01T00:00:00Z",
        "content_md": "", "content_html": "", "author": null, "page": null, "thumb": null,
        "hoppers": [],
    }))
    .unwrap();
    r.background = None;
    r
}

#[test]
fn entries_parse_with_their_reasons_choice_and_blyg() {
    let items = [
        imported(
            "sub-a",
            "p1",
            json!({"reasons": ["withdrawn"], "placement": null, "blyg": false}),
        ),
        imported(
            "sub-a",
            "p2",
            json!({"reasons": [], "placement": "background", "blyg": true}),
        ),
        own(
            "o1",
            json!({"reasons": ["minor"], "placement": null, "blyg": false, "detail": {"category": "typo"}}),
        ),
        // A studio before 0.61: no `background`, not an entry of the set.
        json!({"source": "imported", "imported": {"subscriptionId": "sub-a", "remoteId": "p3"}}),
    ];
    let set = BackgroundSet::from_entries(&items, None).indexed();
    assert_eq!(set.len(), 3);
    let p1 = set.of(&k("sub-a", "p1"));
    assert!(p1.on && p1.reasons == ["withdrawn"] && p1.placement.is_none());
    assert_eq!(p1.why(), None, "withdrawn says so itself");
    let p2 = set.of(&k("sub-a", "p2"));
    assert_eq!(p2.placement, Some(Placement::Background));
    assert_eq!(p2.why().as_deref(), Some("you moved this"));
    let o1 = set.of(&k(OWN_SUBSCRIPTION, "o1"));
    assert!(o1.on);
    assert_eq!(o1.why().as_deref(), Some("minor change (typo)"));
    // Not listed: in the foreground, but its blyg is known to be in Background.
    let p3 = set.of(&k("sub-a", "p3"));
    assert!(!p3.on && p3.blyg && p3.placement.is_none());
    assert!(!set.of(&k("sub-b", "x")).blyg);
}

#[test]
fn a_row_is_background_by_the_set_once_known_else_when_withdrawn() {
    let mut gone = row("sub-a", "gone", "tombstone");
    let live = row("sub-a", "live", "current");
    assert!(
        gone.is_background() && !live.is_background(),
        "the old rule"
    );
    // The studio lists `live` (moved there) and not `gone` (moved to Foreground).
    let mut set = BackgroundSet::from_entries(
        &[imported(
            "sub-a",
            "live",
            json!({"reasons": [], "placement": "background", "blyg": false}),
        )],
        None,
    );
    set.place(&k("sub-a", "gone"), true, Some(Placement::Foreground));
    let mut rows = vec![gone.clone(), live];
    set.annotate(&mut rows);
    assert!(
        !rows[0].is_background(),
        "a Foreground choice beats withdrawal"
    );
    assert_eq!(
        rows[0].background.as_ref().unwrap().placement,
        Some(Placement::Foreground)
    );
    assert!(rows[1].is_background());
    gone.background = None;
    assert!(gone.is_background());
}

#[test]
fn placing_follows_the_studios_precedence() {
    let mut set = BackgroundSet::from_entries(
        &[imported(
            "s",
            "minor",
            json!({"reasons": ["minor"], "placement": null, "blyg": false}),
        )],
        None,
    );
    let key = k("s", "minor");
    // The entry's choice beats a reason…
    set.place(&key, false, Some(Placement::Foreground));
    assert!(!set.of(&key).on);
    assert_eq!(set.foreground, vec![key.clone()]);
    // …and cleared, the rules decide again (the reason was dropped with the
    // entry's listing, so it's in the foreground until the studio says).
    set.place(&key, false, None);
    assert!(!set.of(&key).on && set.foreground.is_empty());
    // Background, then cleared: a withdrawn post goes back to Background.
    let gone = k("s", "gone");
    set.place(&gone, true, Some(Placement::Background));
    assert!(set.of(&gone).on);
    set.place(&gone, true, None);
    let e = set.of(&gone);
    assert!(e.on && e.placement.is_none() && e.reasons == ["withdrawn"]);
    // A live post cleared with no reason: foreground.
    let plain = k("s", "plain");
    set.place(&plain, false, Some(Placement::Background));
    set.place(&plain, false, None);
    assert!(!set.of(&plain).on);
}

#[test]
fn a_whole_blyg_moves_except_its_entries_with_a_choice() {
    let rows = vec![
        row("s", "a", "current"),
        row("s", "b", "current"),
        row("t", "c", "current"),
    ];
    let mut set = BackgroundSet::default();
    set.place(&k("s", "b"), false, Some(Placement::Foreground));
    set.place_blyg(&SubId::from("s"), true, &rows);
    assert!(set.of(&k("s", "a")).on && set.of(&k("s", "a")).blyg);
    assert!(!set.of(&k("s", "b")).on, "its own Foreground choice wins");
    assert!(!set.of(&k("t", "c")).on);
    assert!(set.blyg_on(&SubId::from("s")));
    set.place_blyg(&SubId::from("s"), false, &rows);
    assert!(!set.of(&k("s", "a")).on && !set.blyg_on(&SubId::from("s")));
}

#[test]
fn a_new_read_keeps_foreground_choices_the_studio_doesnt_contradict() {
    let mut prev = BackgroundSet::default();
    prev.place(&k("s", "a"), false, Some(Placement::Foreground));
    prev.place(&k("s", "b"), false, Some(Placement::Foreground));
    // Moved to Background elsewhere (in the studio): listed again.
    let next = BackgroundSet::from_entries(
        &[imported(
            "s",
            "b",
            json!({"reasons": [], "placement": "background", "blyg": false}),
        )],
        Some(&prev),
    );
    assert_eq!(next.foreground, vec![k("s", "a")]);
    assert!(next.of(&k("s", "b")).on);
    let round = BackgroundSet::from_json(&next.to_json()).unwrap();
    assert_eq!(round.entries, next.entries);
    assert_eq!(round.foreground, next.foreground);
}

#[test]
fn placement_bodies_name_the_source() {
    assert_eq!(
        placement_body(&k("sub-1", "p"), Some(Placement::Background)),
        json!({"source": "imported", "sub": "sub-1", "id": "p", "placement": "background"})
    );
    assert_eq!(
        placement_body(&k(OWN_SUBSCRIPTION, "o"), None),
        json!({"source": "own", "id": "o", "placement": null})
    );
}

#[test]
fn rules_default_on_and_patch_only_the_one_changed() {
    let r = BackgroundRules::from_settings(&json!({"backgroundRules": {"minor": false}})).unwrap();
    assert!(!r.on("minor"));
    let r = BackgroundRules::from_settings(&json!({"backgroundRules": {}})).unwrap();
    assert!(r.on("minor"), "a rule not sent reads as on");
    assert!(!r.offers("minor"), "…but isn't offered");
    assert!(BackgroundRules::from_settings(&json!({"siteTitle": "x"})).is_none());
    assert_eq!(
        BackgroundRules::patch("minor", false),
        json!({"backgroundRules": {"minor": false}})
    );
}

/// Studio 0.61 refuses `burst` (strict schema, 400), so only the rules a
/// studio's settings list are offered; 0.64 lists all four, in its order.
#[test]
fn only_the_rules_the_studio_lists_are_offered() {
    let old = BackgroundRules::from_settings(&json!({"backgroundRules": {"minor": true}})).unwrap();
    assert_eq!(old.rules, vec![("minor".to_string(), true)]);
    assert!(!old.offers(BURST) && !old.offers("wordless") && !old.offers("maintenance"));
    let new = BackgroundRules::from_settings(&json!({"backgroundRules": {
        "minor": true, "maintenance": true, "wordless": false, "burst": true, "later": true,
    }}))
    .unwrap();
    let names: Vec<&str> = new.rules.iter().map(|(r, _)| r.as_str()).collect();
    assert_eq!(names, ["burst", "wordless", "maintenance", "minor"]);
    assert!(!new.on("wordless") && new.on(BURST));
    assert_eq!(
        BackgroundRules::patch(BURST, false),
        json!({"backgroundRules": {"burst": false}}),
        "partial: the other rules keep their stored values"
    );
    for rule in RULES {
        assert!(!rule_label(rule).0.is_empty(), "{rule} has a label");
    }
}

#[test]
fn each_reason_says_why_as_the_studio_does() {
    let why = |reasons: &[&str], detail: Value| {
        let entry = imported(
            "s",
            "p",
            json!({"reasons": reasons, "placement": null, "blyg": false, "detail": detail}),
        );
        BackgroundSet::from_entries(&[entry], None)
            .of(&k("s", "p"))
            .why()
    };
    let burst = json!({"thread": "t"});
    assert_eq!(
        why(&["burst"], burst.clone()).as_deref(),
        Some("part of a burst")
    );
    assert_eq!(
        why(&["wordless"], Value::Null).as_deref(),
        Some("no words of its own")
    );
    assert_eq!(
        why(&["maintenance", "wordless"], Value::Null).as_deref(),
        Some("republished unchanged")
    );
    assert_eq!(why(&["withdrawn", "burst"], burst), None);
}

/// Studio 0.64's "+N parts": the parts are the Background entries with
/// reason `burst` naming the thread, of its own blyg, oldest first.
#[test]
fn a_threads_parts_are_its_burst_entries() {
    let part = |sub: &str, id: &str, thread: &str| {
        let b = json!({"reasons": ["burst"], "placement": null, "blyg": false, "detail": {"thread": thread}});
        if sub == OWN_SUBSCRIPTION {
            own(id, b)
        } else {
            imported(sub, id, b)
        }
    };
    let set = BackgroundSet::from_entries(
        &[
            part("s", "p2", "t"),
            part("s", "p1", "t"),
            part("s", "gone", "t"),
            part(OWN_SUBSCRIPTION, "o1", "mine"),
            // Wordless, not a part.
            imported(
                "s",
                "w",
                json!({"reasons": ["wordless"], "placement": null, "blyg": false}),
            ),
        ],
        None,
    )
    .indexed();
    let mut rows = vec![
        row("s", "t", "current"),
        row("s", "p2", "current"),
        row("s", "p1", "current"),
        row("s", "gone", "tombstone"),
        row("s", "w", "current"),
        row(OWN_SUBSCRIPTION, "mine", "current"),
        row(OWN_SUBSCRIPTION, "o1", "current"),
    ];
    rows[1].created = Some("2026-11-01T10:05:00Z".into());
    rows[2].created = Some("2026-11-01T10:01:00Z".into());
    set.annotate(&mut rows);
    let parts = burst_parts(&rows);
    assert_eq!(parts.len(), 2);
    assert_eq!(
        parts[&k("s", "t")],
        [k("s", "p1"), k("s", "p2")],
        "oldest first; a withdrawn part isn't listed"
    );
    assert_eq!(
        parts[&k(OWN_SUBSCRIPTION, "mine")],
        [k(OWN_SUBSCRIPTION, "o1")]
    );
    assert_eq!(parts_label(2, false), "+2 parts");
    assert_eq!(parts_label(1, true), "−1 part");

    // Made in the same second: in the order the thread quotes them.
    rows[1].created = rows[2].created.clone();
    rows[0].transclusions = serde_json::from_value(json!([{"id": "p2"}, {"id": "p1"}])).unwrap();
    assert_eq!(
        burst_parts(&rows)[&k("s", "t")],
        [k("s", "p2"), k("s", "p1")]
    );

    // The burst rule off: the studio no longer counts the reason, the parts
    // are back in the foreground, and the thread lists none.
    let off = BackgroundSet::from_entries(
        &[imported(
            "s",
            "p1",
            json!({"reasons": [], "placement": "background", "blyg": false, "detail": {"thread": "t"}}),
        )],
        None,
    );
    off.annotate(&mut rows);
    assert!(burst_parts(&rows).is_empty());
}

#[test]
fn served_only_when_the_capabilities_list_the_operation() {
    assert!(serves(
        &json!({"operations": ["getShelf", "setReadingPlacement"]})
    ));
    assert!(serves(&json!({"operations": {"setReadingPlacement": {}}})));
    assert!(!serves(
        &json!({"contractVersion": "5.10.0", "operations": ["getShelf"]})
    ));
    assert!(!serves(&json!({"contractVersion": "5.10.0"})));
}

#[test]
fn the_store_keeps_the_lens_only_while_served_and_counts_unread_by_it() {
    let s = crate::store::Store::open_in_memory().unwrap();
    let mut rows = vec![
        row("s", "a", "current"),
        row("s", "b", "current"),
        row("s", "c", "current"),
    ];
    for r in &mut rows {
        r.thumb = Some(1);
    }
    s.merge_reading(&rows, true, &Default::default()).unwrap();
    assert_eq!(s.unread_count(), 3);
    let set = BackgroundSet::from_entries(
        &[imported(
            "s",
            "a",
            json!({"reasons": [], "placement": "background", "blyg": false}),
        )],
        None,
    );
    assert!(s.put_background_set(&set).unwrap());
    assert!(s.background_set().is_none(), "not served: nothing is used");
    assert_eq!(s.unread_count(), 3);
    s.set_background_served(true).unwrap();
    assert!(!s.put_background_set(&set).unwrap(), "unchanged");
    assert_eq!(s.background_set().unwrap().len(), 1);
    assert_eq!(s.unread_count(), 2, "a Background post never counts");
    assert!(
        s.reading().iter().all(|r| r.background.is_none()),
        "never stored with the rows"
    );
    s.set_background_served(false).unwrap();
    assert!(s.background_set().is_none() && s.meta(SET_META).is_none());
}
