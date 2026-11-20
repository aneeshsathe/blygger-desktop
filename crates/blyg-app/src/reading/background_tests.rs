//! --- background --- The reading screen's Background rules: where a row
//! is listed, and which chips it gets.

use blyg_core::background::{BackgroundSet, Placement};
use blyg_core::{Kind, OWN_SUBSCRIPTION, ReadingItem};
use serde_json::json;

use super::background::{BLYG, CLEAR, PLACE, chips};
use super::own::with_background;
use super::sources_vm::{Smart, Source, in_source, needs_reading};

fn row(sub: &str, id: &str, state: &str, at: &str) -> ReadingItem {
    serde_json::from_value(json!({
        "subscription_id": sub, "remote_id": id, "subscription_title": "Ada",
        "origin": "https://ada.blyg.example.com/", "kind": "fragment", "state": state,
        "version": 2, "created": at, "updated": at, "observed_at": at,
        "content_md": "A sample post", "content_html": "", "author": null, "page": null,
        "thumb": null, "hoppers": [],
    }))
    .unwrap()
}

fn entry(
    sub: &str,
    id: &str,
    reasons: &[&str],
    placement: Option<&str>,
    blyg: bool,
) -> serde_json::Value {
    if sub == OWN_SUBSCRIPTION {
        json!({"source": "own", "own": {"id": id}, "background": {"reasons": reasons, "placement": placement, "blyg": blyg}})
    } else {
        json!({"source": "imported", "imported": {"subscriptionId": sub, "remoteId": id},
               "background": {"reasons": reasons, "placement": placement, "blyg": blyg}})
    }
}

fn ids(rows: &[ReadingItem], source: &Source) -> Vec<String> {
    let now = chrono::Utc::now();
    rows.iter()
        .filter(|r| in_source(r, source, &Default::default(), now))
        .map(|r| r.remote_id.to_string())
        .collect()
}

#[test]
fn background_posts_leave_every_other_list_for_background() {
    let mut main = vec![
        row("s", "moved", "current", "2026-11-05T10:00:00Z"),
        row("s", "plain", "current", "2026-11-04T10:00:00Z"),
        row(
            OWN_SUBSCRIPTION,
            "mine-typo",
            "current",
            "2026-11-03T10:00:00Z",
        ),
    ];
    main[2].kind = Kind::Thread;
    let mut withdrawn = vec![
        row("s", "gone", "tombstone", "2026-11-02T10:00:00Z"),
        row("s", "kept", "tombstone", "2026-11-01T10:00:00Z"),
    ];
    let mut set = BackgroundSet::from_entries(
        &[
            entry("s", "moved", &[], Some("background"), false),
            entry(OWN_SUBSCRIPTION, "mine-typo", &["minor"], None, false),
            entry("s", "gone", &["withdrawn"], None, false),
        ],
        None,
    );
    // You brought one withdrawn post back to the foreground.
    set.place(
        &("s".into(), "kept".into()),
        true,
        Some(Placement::Foreground),
    );
    set.annotate(&mut main);
    set.annotate(&mut withdrawn);
    let rows = with_background(super::vm::order(main), withdrawn);
    assert_eq!(ids(&rows, &Source::Smart(Smart::All)), ["plain", "kept"]);
    assert_eq!(
        ids(&rows, &Source::Smart(Smart::Background)),
        ["moved", "mine-typo", "gone"]
    );
    assert!(
        ids(&rows, &Source::Smart(Smart::Threads)).is_empty(),
        "your minor change too"
    );
    assert!(ids(&rows, &Source::Own).is_empty());
    assert!(
        rows.iter()
            .filter(|r| r.is_background())
            .all(|r| !needs_reading(r))
    );
}

#[test]
fn without_the_studios_answer_withdrawn_is_background() {
    let main = vec![row("s", "live", "current", "2026-11-05T10:00:00Z")];
    let withdrawn = vec![row("s", "gone", "tombstone", "2026-11-02T10:00:00Z")];
    let rows = with_background(super::vm::order(main), withdrawn);
    assert_eq!(ids(&rows, &Source::Smart(Smart::All)), ["live"]);
    assert_eq!(ids(&rows, &Source::Smart(Smart::Background)), ["gone"]);
}

#[test]
fn chips_offer_the_studios_choices_only_when_served() {
    let mut r = row("s", "p", "current", "2026-11-05T10:00:00Z");
    assert!(
        chips(&r, false).is_empty(),
        "an older studio: no Background UI"
    );
    let set = BackgroundSet::default();
    set.annotate(std::slice::from_mut(&mut r));
    let c = chips(&r, true);
    assert_eq!(c.iter().map(|c| c.id).collect::<Vec<_>>(), [PLACE, BLYG]);
    assert_eq!(c[0].label, "↓ Background");
    assert_eq!(c[1].label, "⇊ Blyg to Background");

    let set = BackgroundSet::from_entries(&[entry("s", "p", &[], Some("background"), true)], None);
    set.annotate(std::slice::from_mut(&mut r));
    let c = chips(&r, true);
    assert_eq!(
        c.iter().map(|c| c.id).collect::<Vec<_>>(),
        [PLACE, CLEAR, BLYG]
    );
    assert_eq!(c[0].label, "↑ Foreground");
    assert_eq!(c[2].label, "⇈ Blyg to Foreground");

    let mut mine = row(OWN_SUBSCRIPTION, "o", "current", "2026-11-05T10:00:00Z");
    BackgroundSet::default().annotate(std::slice::from_mut(&mut mine));
    assert_eq!(
        chips(&mine, true).iter().map(|c| c.id).collect::<Vec<_>>(),
        [PLACE]
    );
}
