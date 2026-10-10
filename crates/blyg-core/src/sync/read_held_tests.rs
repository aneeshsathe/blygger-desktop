//! --- read/unread --- Read state held for the scope (`read_denied`), on
//! real SQLite: the ops stay queued, new marks still queue, and the rest of
//! the outbox goes on without them. (The 403 itself is tested against a
//! real studio: `tests/e2e.rs`, `read_state_needs_its_own_scope`.)

use std::collections::HashMap;

use super::*;
use crate::api::auth::Credential;

fn row(rid: &str) -> ReadingItem {
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
        thumb: None,
        hoppers: vec![],
        pinned_version_retained: None,
        read_version: None,
        stub_of: None,
        forked_from: None,
        transclusions: vec![],
        lineage_known: false,
    }
}

/// An engine on an in-memory store with read sync on, two reading rows,
/// and a read op queued ahead of a draft's create. Nothing here touches the
/// network (the API points at a closed port).
fn engine() -> Engine {
    let store = Store::open_in_memory().unwrap();
    let kinds: HashMap<String, SubscriptionKind> =
        [("s1".to_string(), SubscriptionKind::Blyg)].into();
    store
        .merge_reading(&[row("A"), row("B")], true, &kinds)
        .unwrap();
    store.set_meta(READ_SYNC, "1").unwrap();
    assert_eq!(store.mark_read_rows("s1", "A", true).unwrap().len(), 1);
    store.create_draft(Kind::Fragment, "a draft", 0).unwrap();
    let api = Api::new("http://127.0.0.1:9", Credential::Token("t".into()));
    Engine::new(store, api, SyncOptions::default())
}

fn kinds_queued(e: &Engine) -> Vec<OpKind> {
    e.store.ops().unwrap().into_iter().map(|o| o.kind).collect()
}

#[test]
fn a_held_read_stays_queued_and_lets_the_rest_through() {
    let e = engine();
    assert_eq!(kinds_queued(&e), vec![OpKind::Read, OpKind::Create]);
    // Before a refusal, the read goes first.
    assert_eq!(e.next_op(None, true).unwrap().unwrap().kind, OpKind::Read);

    let refused = CoreError::Rejected {
        status: 403,
        message: crate::api::oauth::missing_scope_message(
            &[crate::api::oauth::READ_STATE_SCOPE.into()],
            Some("owner:read owner:draft owner:publish owner:manage offline_access"),
        ),
        details: vec![],
    };
    e.read_denied(&refused).unwrap();
    // The create goes; the read waits, still queued, and isn't picked again.
    let next = e.next_op(None, true).unwrap().unwrap();
    assert_eq!(next.kind, OpKind::Create);
    assert_eq!(kinds_queued(&e), vec![OpKind::Read, OpKind::Create]);
    let read = &e.store.ops().unwrap()[0];
    assert!(e.read_held(read) && !e.read_held(&next));
    e.store.drop_op(next.seq).unwrap();
    assert!(e.next_op(None, true).unwrap().is_none(), "no retry loop");

    // Marks made meanwhile still queue, for a later sign-in to send.
    assert!(e.read_sync_on());
    e.store.mark_read_rows("s1", "B", true).unwrap();
    assert_eq!(kinds_queued(&e), vec![OpKind::Read, OpKind::Read]);
    assert!(e.next_op(None, true).unwrap().is_none());

    // A new session (a new sign-in) sends them.
    let again = Engine::new(
        Store::open_in_memory().unwrap(),
        Api::new("http://127.0.0.1:9", Credential::Token("t".into())),
        SyncOptions::default(),
    );
    assert!(!again.state().read_denied);
}

#[test]
fn the_notice_says_to_sign_in_again_and_names_the_scope() {
    let refused = CoreError::Rejected {
        status: 403,
        message: crate::api::oauth::missing_scope_message(
            &[crate::api::oauth::READ_STATE_SCOPE.into()],
            Some("owner:read owner:draft owner:publish owner:manage offline_access"),
        ),
        details: vec![],
    };
    let m = read_denied_message(&refused);
    assert!(m.starts_with("Read state isn't syncing."), "{m}");
    assert!(m.contains("reading:state"), "{m}");
    assert!(m.contains("Sign in with the browser again"), "{m}");
    assert!(m.contains("kept on this Mac"), "{m}");
    // A Worker's plain 403 (no challenge) still reads sensibly.
    let m = read_denied_message(&CoreError::Rejected {
        status: 403,
        message: String::new(),
        details: vec![],
    });
    assert!(m.contains("refused to save read state"), "{m}");
}

#[test]
fn the_notice_is_sent_once_a_session() {
    let e = engine();
    let (tx, rx) = std::sync::mpsc::channel();
    e.set_sink(Arc::new(move |ev| {
        if let CoreEvent::Error(m) = ev {
            let _ = tx.send(m);
        }
    }));
    let refused = CoreError::Rejected {
        status: 403,
        message: "no".into(),
        details: vec![],
    };
    e.read_denied(&refused).unwrap();
    e.read_denied(&refused).unwrap();
    assert_eq!(rx.try_iter().count(), 1);
}
