//! `Backend::save_if_base`: a save that only lands on the text it was based
//! on. Local, so a real `LiveBackend` on a temp dir with the sync worker off
//! (nothing is ever pushed; port 9 is the discard port and is never dialled).

use blyg_core::*;

fn backend(dir: &std::path::Path) -> LiveBackend {
    LiveBackend::open_with(
        dir,
        "http://127.0.0.1:9",
        "unused-test-credential",
        SyncOptions {
            start_worker: false,
            ..SyncOptions::default()
        },
    )
    .unwrap()
}

#[test]
fn saves_on_the_base_it_was_given() {
    let dir = tempfile::tempdir().unwrap();
    let b = backend(dir.path());
    let id = b.create_draft(Kind::Fragment, "first").unwrap();
    b.save_if_base(&id, "second", &content_hash("first"))
        .unwrap();
    assert_eq!(b.item(&id).unwrap().content_md, "second");
}

#[test]
fn refuses_a_stale_base_and_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let b = backend(dir.path());
    let id = b.create_draft(Kind::Fragment, "first").unwrap();
    b.save(&id, "edited in the app").unwrap();
    let err = b
        .save_if_base(&id, "from elsewhere", &content_hash("first"))
        .unwrap_err();
    match err {
        CoreError::Rejected {
            status,
            message,
            details,
        } => {
            assert_eq!(status, 409);
            assert_eq!(message, STALE_MESSAGE);
            assert_eq!(details, vec![content_hash("edited in the app")]);
        }
        other => panic!("expected a 409, got {other:?}"),
    }
    assert_eq!(b.item(&id).unwrap().content_md, "edited in the app");
}

#[test]
fn works_on_scratch_notes_and_misses_unknown_ids() {
    let dir = tempfile::tempdir().unwrap();
    let b = backend(dir.path());
    let id = b.create_scratch(Kind::Fragment, "a scratch").unwrap();
    b.save_if_base(&id, "a scratch, edited", &content_hash("a scratch"))
        .unwrap();
    assert_eq!(b.item(&id).unwrap().content_md, "a scratch, edited");
    assert!(matches!(
        b.save_if_base(&LocalId("nope".into()), "x", &content_hash("")),
        Err(CoreError::NotFound)
    ));
}
