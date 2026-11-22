//! --- shelf sync --- the wire shapes, the merge and the cap.

use serde_json::json;

use super::*;

fn e(id: &str) -> Entry {
    // Imported entries under one invented subscription; `o…` are own.
    match id.strip_prefix('o') {
        Some(rest) => Entry::own(rest),
        None => Entry::new("sub-tide", id),
    }
}

fn list(ids: &[&str]) -> Vec<Entry> {
    ids.iter().map(|i| e(i)).collect()
}

#[test]
fn entries_map_to_the_wire_as_the_studio_names_them() {
    let body = put_body(&[Entry::own("it-now"), Entry::new("sub-tide", "r1")], 7);
    assert_eq!(
        body,
        json!({
            "entries": [{"sub": "~own", "remoteId": "it-now"}, {"sub": "sub-tide", "remoteId": "r1"}],
            "expectedRevision": 7
        })
    );
    let got = parse(&json!({
        "entries": [{"sub": "~own", "remoteId": "it-now"}, {"sub": "sub-tide", "remoteId": "r1"},
                    {"sub": "", "remoteId": "x"}, {"remoteId": "y"}],
        "revision": 3
    }))
    .unwrap();
    assert_eq!(
        got,
        Remote {
            entries: vec![Entry::own("it-now"), Entry::new("sub-tide", "r1")],
            revision: 3
        }
    );
    assert!(got.entries[0].is_own() && !got.entries[1].is_own());
    assert_eq!(parse(&json!({"entries": []})), None, "no revision");
    // A 409's body carries the current list.
    let conflict = json!({
        "code": "revision-conflict",
        "message": "shelf revision conflict; reload and try again",
        "details": {"current": {"entries": [{"sub": "~own", "remoteId": "a"}], "revision": 9}}
    });
    assert_eq!(
        conflict_current(&conflict),
        Some(Remote {
            entries: vec![Entry::own("a")],
            revision: 9
        })
    );
}

/// Read on `getShelf` (owner:read), written on `putShelf` too.
#[test]
fn read_on_get_shelf_and_written_on_put_shelf() {
    assert!(serves(
        &json!({"operations": ["getShelf", "putShelf", "listItems"]})
    ));
    assert!(serves(
        &json!({"operations": {"getShelf": {}, "putShelf": {}}})
    ));
    assert!(serves(&json!({"operations": ["getShelf"]})));
    assert!(!takes(&json!({"operations": ["getShelf"]})));
    assert!(takes(&json!({"operations": ["getShelf", "putShelf"]})));
    // The contract number alone never turns it on.
    assert!(!serves(
        &json!({"contractVersion": "5.10.0", "operations": ["listItems"]})
    ));
    assert!(!serves(&json!({"contractVersion": "5.10.0"})));
}

#[test]
fn a_put_never_carries_more_than_the_cap() {
    let many: Vec<Entry> = (0..60).map(|i| Entry::own(format!("it-{i}"))).collect();
    let body = put_body(&many, 0);
    assert_eq!(body["entries"].as_array().unwrap().len(), SHELF_MAX);
    let (kept, over) = cap(many.clone());
    assert_eq!((kept.len(), over), (SHELF_MAX, 10));
    assert_eq!(kept[..], many[..SHELF_MAX]);
    let (kept, over) = cap(many[..3].to_vec());
    assert_eq!((kept.len(), over), (3, 0));
    assert!(overflow_message(10).contains("10 posts"));
    assert!(overflow_message(1).contains("1 post didn't"));
}

#[test]
fn a_first_sync_takes_the_union_with_the_blygs_order_first() {
    // Local only: upload it.
    assert_eq!(merge(None, &list(&["oa", "r1"]), &[]), list(&["oa", "r1"]));
    // Both: the blyg's, then what only this side has, in its order.
    assert_eq!(
        merge(None, &list(&["r2", "oa", "r1"]), &list(&["r1", "ob"])),
        list(&["r1", "ob", "r2", "oa"])
    );
    // The blyg's only: take it.
    assert_eq!(merge(None, &[], &list(&["r1"])), list(&["r1"]));
}

#[test]
fn with_nothing_changed_on_the_blyg_the_local_list_goes_as_it_is() {
    let base = list(&["oa", "r1", "ob"]);
    for local in [
        list(&["oa", "r1", "ob"]),
        list(&["ob", "oa", "r1"]),
        list(&["oa", "r9", "ob"]),
        list(&["r9", "oa", "r1", "ob", "oc"]),
        list(&[]),
    ] {
        assert_eq!(merge(Some(&base), &local, &base), local, "{local:?}");
    }
}

#[test]
fn a_conflict_applies_this_sides_change_on_top_of_the_blygs_list() {
    let base = list(&["oa", "r1", "ob"]);
    // Elsewhere: r2 added at the end and oa taken off.
    let server = list(&["r1", "ob", "r2"]);

    // Here: oc added after r1. Both survive; oa stays off.
    assert_eq!(
        merge(Some(&base), &list(&["oa", "r1", "oc", "ob"]), &server),
        list(&["r1", "oc", "ob", "r2"])
    );
    // Here: ob taken off. Off on both sides' changes.
    assert_eq!(
        merge(Some(&base), &list(&["oa", "r1"]), &server),
        list(&["r1", "r2"])
    );
    // Here: ob moved to the top. The blyg's r2 keeps its place.
    assert_eq!(
        merge(Some(&base), &list(&["ob", "oa", "r1"]), &server),
        list(&["ob", "r1", "r2"])
    );
    // Here: something added first, while the blyg took off what it followed.
    assert_eq!(
        merge(Some(&base), &list(&["r7", "oa", "r1", "ob"]), &server),
        list(&["r7", "r1", "ob", "r2"])
    );
    // Both added the same entry: once.
    assert_eq!(
        merge(Some(&base), &list(&["oa", "r1", "ob", "r2"]), &server),
        list(&["r1", "ob", "r2"])
    );
}

#[test]
fn nothing_changed_here_takes_the_blygs_list() {
    let base = list(&["oa", "r1"]);
    let server = list(&["r1", "ob", "oa"]);
    assert_eq!(merge(Some(&base), &base, &server), server);
}

#[test]
fn place_after_keeps_local_only_entries_where_they_were() {
    let order = ["a", "local1", "b", "local2", "c"];
    let mut out = vec!["c", "a", "b"];
    place_after(&order, &mut out, |x| x.starts_with("local"));
    assert_eq!(out, ["c", "a", "local1", "b", "local2"]);
    let mut out = vec!["b"];
    place_after(&["local0", "a", "b"], &mut out, |x| x.starts_with("local"));
    assert_eq!(out, ["local0", "b"]);
}
