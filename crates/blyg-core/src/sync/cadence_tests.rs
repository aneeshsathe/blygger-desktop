//! Issue #71: the low-usage cadence, the hard backoff after a 5xx, and
//! thumbs laid over held rows without a reading walk, on real SQLite. (The
//! request counts are measured against a real studio: `tests/e2e.rs`,
//! `reading_is_walked_only_when_reading_moves`.)

use std::collections::HashMap;

use super::*;
use crate::api::auth::Credential;

fn row(rid: &str, thumb: Option<i8>) -> ReadingItem {
    ReadingItem {
        subscription_id: "s1".into(),
        remote_id: rid.into(),
        subscription_title: "s1".into(),
        origin: "https://a.example/".into(),
        kind: Kind::Fragment,
        state: "current".into(),
        version: 1,
        created: None,
        updated: None,
        observed_at: "2026-01-01T00:00:00Z".into(),
        content_md: rid.into(),
        content_html: String::new(),
        author: None,
        page: Some(format!("https://a.example/f/{rid}")),
        thumb,
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

/// An engine on an in-memory store; the API points at a closed port.
fn engine() -> Engine {
    let store = Store::open_in_memory().unwrap();
    let api = Api::new("http://127.0.0.1:9", Credential::Token("t".into()));
    Engine::new(
        store,
        api,
        SyncOptions {
            start_worker: false,
            ..SyncOptions::default()
        },
    )
}

fn status_err(status: u16, message: &str) -> CoreError {
    CoreError::Rejected {
        status,
        message: message.into(),
        details: vec![],
    }
}

#[test]
fn cadence_parses_and_low_polls_less() {
    assert_eq!(SyncCadence::parse("low"), SyncCadence::Low);
    assert_eq!(SyncCadence::parse(" LOW "), SyncCadence::Low);
    assert_eq!(SyncCadence::parse("normal"), SyncCadence::Normal);
    assert_eq!(SyncCadence::parse("bogus"), SyncCadence::Normal);
    let n = SyncOptions::for_cadence(SyncCadence::Normal);
    let l = SyncOptions::for_cadence(SyncCadence::Low);
    assert_eq!(n.full_pull_every, FULL_PULL_EVERY);
    assert_eq!(n.full_pull_max, FULL_PULL_MAX);
    for (low, normal) in [
        (l.pull_interval, n.pull_interval),
        (l.full_pull_every, n.full_pull_every),
        (l.full_pull_max, n.full_pull_max),
        (l.reading_gap, n.reading_gap),
        (l.server_backoff_initial, n.server_backoff_initial),
    ] {
        assert!(low > normal, "{low:?} vs {normal:?}");
    }
    // Everything else is the same.
    assert_eq!(l.debounce, n.debounce);
    assert_eq!(l.reading_pages, n.reading_pages);
}

#[test]
fn a_5xx_backs_off_hard_and_quietly() {
    let e = engine();
    let initial = e.opts.server_backoff_initial;
    let before = Instant::now();
    e.note_err(&status_err(500, "internal server error"));
    assert_eq!(
        e.status(),
        SyncStatus::Unavailable {
            pending: 0,
            quota: false
        }
    );
    let wait = e.retry_at().unwrap() - before;
    assert!(wait >= initial, "{wait:?}");
    // Each failure doubles the wait, up to the cap.
    assert_eq!(e.server_backoff(), initial * 2);
    for _ in 0..20 {
        e.note_err(&status_err(503, "unavailable"));
    }
    assert_eq!(e.server_backoff(), e.opts.server_backoff_max);
    assert!(e.retry_at().unwrap() - Instant::now() > initial * 8);
    // The ordinary (offline) backoff is untouched by it.
    assert_eq!(e.state().backoff, e.opts.backoff_initial);
    // A success ends it.
    e.note_ok();
    assert_eq!(e.status(), SyncStatus::Synced);
    assert_eq!(e.server_backoff(), initial);
    assert!(e.retry_at().is_none());
}

#[test]
fn a_d1_quota_error_says_so() {
    let e = engine();
    e.note_err(&status_err(
        500,
        "D1_ERROR: Exceeded D1's free tier daily row read limit",
    ));
    assert_eq!(
        e.status(),
        SyncStatus::Unavailable {
            pending: 0,
            quota: true
        }
    );
    assert_eq!(
        server_failure(&status_err(500, "D1_ERROR: no such table")),
        Some(false)
    );
    assert_eq!(server_failure(&status_err(422, "D1_ERROR limit")), None);
    assert_eq!(server_failure(&CoreError::Offline), None);
}

#[test]
fn a_4xx_is_not_a_server_failure() {
    let e = engine();
    e.note_err(&status_err(400, "bad"));
    assert_eq!(e.status(), SyncStatus::Synced);
    assert!(e.retry_at().is_none());
}

#[test]
fn thumbs_are_laid_over_held_rows() {
    let e = engine();
    let kinds: HashMap<String, SubscriptionKind> =
        [("s1".to_string(), SubscriptionKind::Blyg)].into();
    e.store
        .merge_reading(
            &[row("A", Some(1)), row("B", None), row("C", Some(-1))],
            true,
            &kinds,
        )
        .unwrap();
    let thumbs = |e: &Engine| {
        let mut v: Vec<(String, Option<i8>)> = e
            .store
            .reading()
            .into_iter()
            .map(|r| (r.remote_id, r.thumb))
            .collect();
        v.sort();
        v
    };
    // A cleared (A), a new one (B), one kept (C), one on a post not held.
    let list = vec![
        ("s1".to_string(), "B".to_string(), -1),
        ("s1".to_string(), "C".to_string(), -1),
        ("s1".to_string(), "Z".to_string(), 1),
    ];
    assert!(e.store.replace_thumbs(&list).unwrap());
    assert_eq!(
        thumbs(&e),
        vec![
            ("A".into(), None),
            ("B".into(), Some(-1)),
            ("C".into(), Some(-1))
        ]
    );
    // The same list again changes nothing.
    assert!(!e.store.replace_thumbs(&list).unwrap());
}
