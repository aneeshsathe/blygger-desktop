//! --- nearby --- the stored vectors and the phrase search, on real SQLite.

use std::collections::{HashMap, HashSet};

use super::Store;
use crate::api::wire::WireItem;
use crate::model::*;
use crate::nearby::{NearbyVector, own_key, reading_key};

fn published(id: &str, md: &str) -> WireItem {
    WireItem {
        id: id.into(),
        kind: "thread".into(),
        status: "public".into(),
        version: 1,
        created: "2030-01-01T00:00:00Z".into(),
        updated: "2030-01-01T00:00:00Z".into(),
        content_md: md.into(),
        ..Default::default()
    }
}

fn ri(sub: &str, rid: &str, state: &str, body: &str) -> ReadingItem {
    ReadingItem {
        subscription_id: sub.into(),
        remote_id: rid.into(),
        subscription_title: format!("{sub} title"),
        origin: format!("https://{sub}.example.com/"),
        kind: Kind::Thread,
        state: state.into(),
        version: 1,
        created: Some("2030-01-02T00:00:00Z".into()),
        updated: Some("2030-01-02T00:00:00Z".into()),
        observed_at: "2030-01-02T00:00:00Z".into(),
        content_md: body.into(),
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
        lineage_known: false,
        source_url: None,
        generated: None,
    }
}

fn kinds() -> HashMap<String, SubscriptionKind> {
    [
        ("lin".to_string(), SubscriptionKind::Blyg),
        ("feed".to_string(), SubscriptionKind::Rss),
    ]
    .into_iter()
    .collect()
}

fn seeded() -> (Store, LocalId) {
    let s = Store::open_in_memory().unwrap();
    s.merge_all(&[
        published(
            "MINE1",
            "# Gardens, not streams\n\nA blyg is tended, not fed.",
        ),
        published("MINE2", "# Tide pools\n\nWhat the tide leaves behind."),
    ])
    .unwrap();
    s.merge_reading(
        &[
            ri(
                "lin",
                "R1",
                "current",
                "# Rock pool economics\n\nTide pools, again.",
            ),
            ri("feed", "R2", "current", "An RSS post about tide pools"),
        ],
        true,
        &kinds(),
    )
    .unwrap();
    let mine1 = s.local_id_for_server("MINE1").unwrap();
    (s, mine1)
}

fn v(doc: &str, chunk: u32, x: f32) -> NearbyVector {
    NearbyVector {
        doc: doc.into(),
        chunk,
        hash: format!("h{chunk}"),
        vec: vec![x, 1.0 - x],
    }
}

fn docs(s: &Store, model: &str) -> Vec<String> {
    s.nearby_vectors(model)
        .unwrap()
        .into_iter()
        .map(|x| x.doc)
        .collect()
}

#[test]
fn vectors_round_trip_and_are_replaced_per_post() {
    let (s, mine1) = seeded();
    assert_eq!(s.schema_version().unwrap(), super::schema::latest());
    let own = own_key(&mine1.0);
    let held = reading_key("lin", "R1");
    s.put_nearby_vectors(
        "m1",
        &[own.clone(), held.clone()],
        &[v(&own, 0, 0.5), v(&own, 1, 0.25), v(&held, 0, 1.0)],
    )
    .unwrap();
    let got = s.nearby_vectors("m1").unwrap();
    assert_eq!(got.len(), 3);
    assert_eq!(got[0].vec, vec![0.5, 0.5]);
    assert!(s.nearby_vectors("other-model").unwrap().is_empty());
    // Re-embedding a post replaces all of its pieces.
    s.put_nearby_vectors("m1", std::slice::from_ref(&own), &[v(&own, 0, 0.0)])
        .unwrap();
    assert_eq!(docs(&s, "m1").iter().filter(|d| **d == own).count(), 1);
    // A post the store doesn't offer gets none: an unknown one, an RSS one.
    let rss = reading_key("feed", "R2");
    s.put_nearby_vectors("m1", std::slice::from_ref(&rss), &[v(&rss, 0, 1.0)])
        .unwrap();
    s.put_nearby_vectors("m1", &["own:nope".into()], &[v("own:nope", 0, 1.0)])
        .unwrap();
    assert_eq!(s.nearby_vectors("m1").unwrap().len(), 2);
}

#[test]
fn withdrawn_or_deleted_posts_lose_their_vectors() {
    let (s, mine1) = seeded();
    let own = own_key(&mine1.0);
    let held = reading_key("lin", "R1");
    s.put_nearby_vectors(
        "m1",
        &[own.clone(), held.clone()],
        &[v(&own, 0, 0.5), v(&held, 0, 0.5)],
    )
    .unwrap();
    s.set_status_version(&mine1, Status::Withdrawn, 2).unwrap();
    assert_eq!(docs(&s, "m1"), vec![held.clone()]);
    // Their post withdrawn: a tombstone row.
    s.merge_reading(&[ri("lin", "R1", "tombstone", "")], true, &kinds())
        .unwrap();
    assert!(s.nearby_vectors("m1").unwrap().is_empty());
}

#[test]
fn retain_drops_other_models_and_posts_no_longer_offered() {
    let (s, mine1) = seeded();
    let own = own_key(&mine1.0);
    let held = reading_key("lin", "R1");
    s.put_nearby_vectors("old", std::slice::from_ref(&own), &[v(&own, 0, 0.5)])
        .unwrap();
    s.put_nearby_vectors(
        "new",
        &[own.clone(), held.clone()],
        &[v(&own, 0, 0.5), v(&held, 0, 0.5)],
    )
    .unwrap();
    let keep: HashSet<String> = [held.clone()].into_iter().collect();
    s.retain_nearby_vectors("new", &keep).unwrap();
    assert!(s.nearby_vectors("old").unwrap().is_empty());
    assert_eq!(docs(&s, "new"), vec![held]);
}

#[test]
fn phrase_hits_use_the_indexes_and_offer_only_what_publish_takes() {
    let (s, mine1) = seeded();
    let mine2 = s.local_id_for_server("MINE2").unwrap();
    let hits = s
        .nearby_phrase_hits(&["tide pools".into(), "TENDED".into(), "ab".into()])
        .unwrap();
    let set: HashSet<(String, usize)> = hits.into_iter().collect();
    let want: HashSet<(String, usize)> = [
        (own_key(&mine2.0), 0),
        (reading_key("lin", "R1"), 0),
        (own_key(&mine1.0), 1),
    ]
    .into_iter()
    .collect();
    // Not the RSS post; a two-letter phrase isn't looked up.
    assert_eq!(set, want);
    // Typing (the text changed since it was indexed) is seen at once.
    s.save(&mine1, "# Gardens\n\nTide pools everywhere.", 0)
        .unwrap();
    let hits = s.nearby_phrase_hits(&["tide pools".into()]).unwrap();
    assert!(hits.contains(&(own_key(&mine1.0), 0)));
    let hits = s.nearby_phrase_hits(&["tended".into()]).unwrap();
    assert!(hits.is_empty(), "{hits:?}");
}
