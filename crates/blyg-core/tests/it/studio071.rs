//! --- studio 0.71 --- Studio 0.70/0.71 (contract 6.3.0), without a studio:
//! a generated scope's web-page sources (`{url, cited}`, decision #117) read
//! from an item's provenance, a reading row's `generated` and the TK path;
//! the generate answer's checkpoint (`POST /items/{id}/generate`); and
//! `readerMarkRead` read and written alone. The e2e against a real studio
//! is `e2e/studio071.rs`.

use blyg_core::api::wire::{WireItem, imported_row};
use blyg_core::generate::{Checkpoint, GenerateOutcome, UnreadLink};
use blyg_core::mark_read::{MARK_READ_META, MARK_READ_SERVED_META, MarkRead};
use blyg_core::store::Store;
use blyg_core::*;
use serde_json::{Value, json};

const ID: &str = "0123456789abcdefghjkmnpqrs";

/// A web page a scope's model read, as studio 0.70 records it.
fn page() -> Value {
    json!({
        "url": "https://news.example.com/an-invented-story",
        "cited": {
            "source": "news.example.com",
            "url": "https://news.example.com/an-invented-story",
            "retrieved": "2030-01-01T00:00:00Z"
        }
    })
}

/// A remote source with its frozen human half (`cited`).
fn remote() -> Value {
    json!({
        "id": ID, "version": 2, "origin": "https://ada.example.com/",
        "cited": {
            "source": "Ada's blyg", "author": "Ada",
            "url": "https://ada.example.com/f/0123456789abcdefghjkmnpqrs/",
            "retrieved": "2030-01-01T00:00:00Z"
        }
    })
}

/// An own item as `GET /api/items/:id` serves it from a 0.70+ studio: one
/// scope that read a web page, one that drew on a remote post as well.
fn item() -> WireItem {
    serde_json::from_value(json!({
        "id": "S1", "revision": 4, "kind": "fragment", "status": "draft", "version": 0,
        "contentMd": format!(
            "[TK]sum up https://news.example.com/an-invented-story[=]A summary.[/TK]\n\n\
             [TK]reply to ![[{ID}]][=]A reply.[/TK]"
        ),
        "provenance": [
            { "model": "m1", "at": "2030-01-01T00:00:00Z", "sources": [page()] },
            { "model": "m2", "sources": [remote(), page()] }
        ]
    }))
    .unwrap()
}

#[test]
fn a_web_source_reads_from_an_items_provenance() {
    let p = item()
        .server_provenance()
        .expect("the item carries provenance");
    assert_eq!(p.len(), 2);
    let first = p[0]
        .as_ref()
        .expect("a scope that read a web page still reads");
    assert_eq!(first.model, "m1");
    assert!(first.sources.is_empty(), "no post: {first:?}");
    assert_eq!(first.pages.len(), 1);
    assert_eq!(
        first.pages[0].url,
        "https://news.example.com/an-invented-story"
    );
    let second = p[1].as_ref().expect("mixed sources read");
    assert_eq!(second.sources.len(), 1);
    assert_eq!(second.sources[0].id, ID);
    assert_eq!(second.pages.len(), 1);
    // Written back as served: one `sources` array, the remote source's
    // `cited` and the page's `{url, cited}` untouched.
    let back = serde_json::to_value(second).unwrap();
    assert_eq!(back["sources"], json!([remote(), page()]));
    assert_eq!(
        serde_json::to_value(first).unwrap()["sources"],
        json!([page()])
    );
}

/// A web page tells no other blyg (decision #117): only a post on another
/// blyg does.
#[test]
fn a_web_source_tells_no_one() {
    let p = item().server_provenance().unwrap();
    let first = p[0].as_ref().unwrap();
    assert_eq!(first.notifies().count(), 0, "a web page sends no mention");
    let second = p[1].as_ref().unwrap();
    let told: Vec<_> = second.notifies().map(|s| s.id.as_str()).collect();
    assert_eq!(told, vec![ID], "the remote post, and not the page");
}

/// The TK path: a page needs no `![[id]]` in its scope (the link may be
/// anywhere in the draft), and follows its scope when the text moves.
#[test]
fn a_web_source_follows_its_scope() {
    let it = item();
    let p = it.server_provenance().unwrap();
    tk::validate(&it.content_md, &p).expect("a page source is valid anywhere");
    let moved = format!("[TK]typed first[=]By hand.[/TK]\n\n{}", it.content_md);
    let r = tk::remap(&it.content_md, &p, &moved).unwrap();
    assert_eq!(r.len(), 3);
    assert!(r[0].is_none());
    assert_eq!(r[1].as_ref().unwrap().pages, p[0].as_ref().unwrap().pages);
    assert_eq!(r[2].as_ref().unwrap().pages.len(), 1);
    tk::validate(&moved, &r).unwrap();
}

/// What the store keeps round-trips (`items.tk_prov`).
#[test]
fn tracked_provenance_keeps_web_sources() {
    let p = item().server_provenance().unwrap();
    let s = serde_json::to_string(&p).unwrap();
    let back: Vec<Option<ScopeProvenance>> = serde_json::from_str(&s).unwrap();
    assert_eq!(back, p);
}

/// A reading row from a 0.70+ blyg whose post read a web page: it syncs,
/// and its disclosure reads.
#[test]
fn a_reading_rows_web_sources_read() {
    let row = json!({
        "subscriptionId": "S1", "subscriptionTitle": "Ada", "origin": "https://ada.example.com/",
        "remoteId": "R1", "kind": "fragment", "withdrawn": false, "l0": false, "version": 1,
        "readVersion": null, "created": "2030-01-01T00:00:00Z", "updated": null,
        "observedAt": "2030-01-01T00:00:05Z", "contentMd": "A summary.", "contentHtml": "<p>A summary.</p>",
        "author": {"name": "Ada"}, "page": "f/R1/", "sourceUrl": null, "pinnedVersionRetained": null,
        "thumb": null, "hoppers": [], "transclusions": null, "stubOf": null, "forkedFrom": null,
        "generated": [{ "model": "m", "at": "2030-01-01T00:00:00Z", "sources": [page(), remote()] }]
    });
    let r = imported_row(&row).expect("the row syncs");
    let g = r.generation();
    assert_eq!(g.len(), 1);
    assert_eq!(g[0].model, "m");
    assert_eq!(g[0].pages.len(), 1);
    assert_eq!(g[0].sources.len(), 1);
    // A web source is no lineage: it names no post.
    assert!(r.references().is_empty(), "{:?}", r.references());
}

// ------------------------------------------------------------- generate

#[test]
fn generate_answers_text_or_a_checkpoint() {
    let g = GenerateOutcome::from_value(json!({
        "text": "Hello.", "model": "m", "contentMd": "[TK]hi[=]Hello.[/TK]", "revision": 7
    }))
    .unwrap();
    match g {
        GenerateOutcome::Generated(g) => {
            assert_eq!(g.text, "Hello.");
            assert_eq!(g.model, "m");
            assert_eq!(g.content_md.as_deref(), Some("[TK]hi[=]Hello.[/TK]"));
            assert_eq!(g.revision, 7);
        }
        other => panic!("{other:?}"),
    }
    let c = GenerateOutcome::from_value(json!({
        "checkpoint": {
            "read": 1, "total": 2,
            "failed": [{ "link": "https://gone.example.com/", "reason": "HTTP 404" }]
        }
    }))
    .unwrap();
    assert_eq!(
        c,
        GenerateOutcome::Checkpoint(Checkpoint {
            read: 1,
            total: 2,
            failed: vec![UnreadLink {
                link: "https://gone.example.com/".into(),
                reason: "HTTP 404".into(),
            }],
        })
    );
    // An older studio's answer has no model: "unknown", as before.
    match GenerateOutcome::from_value(json!({ "text": "x" })).unwrap() {
        GenerateOutcome::Generated(g) => assert_eq!(g.model, "unknown"),
        other => panic!("{other:?}"),
    }
    assert!(GenerateOutcome::from_value(json!({ "neither": true })).is_err());
}

#[test]
fn the_request_says_proceed_only_when_asked() {
    let ask = blyg_core::generate::request_body(2, 9, false);
    assert_eq!(ask, json!({ "scope": 2, "expectedRevision": 9 }));
    let go = blyg_core::generate::request_body(2, 9, true);
    assert_eq!(
        go,
        json!({ "scope": 2, "expectedRevision": 9, "proceed": true })
    );
}

// ------------------------------------------------------------- readerMarkRead

#[test]
fn reader_mark_read_reads_from_settings_and_patches_alone() {
    let s = |v: Value| MarkRead::from_settings(&v);
    assert_eq!(s(json!({ "readerMarkRead": "seen" })), Some(MarkRead::Seen));
    assert_eq!(
        s(json!({ "readerMarkRead": "opened" })),
        Some(MarkRead::Opened)
    );
    assert_eq!(
        s(json!({ "readerMarkRead": "never" })),
        Some(MarkRead::Never)
    );
    // Absent: a studio before 0.70, so not offered (no default invented).
    assert_eq!(s(json!({ "readerOrder": "newest" })), None);
    // A value Burrow doesn't know reads as the contract's default.
    assert_eq!(
        s(json!({ "readerMarkRead": "later" })),
        Some(MarkRead::Seen)
    );
    assert_eq!(
        MarkRead::Opened.patch(),
        json!({ "readerMarkRead": "opened" })
    );
    for m in MarkRead::ALL {
        assert_eq!(MarkRead::parse(m.as_str()), Some(m));
        assert!(!m.label().is_empty());
    }
}

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

/// Not offered until the blyg's settings are seen to have the key; then
/// written to the blyg alone, and kept here only once it answers. Port 9
/// is the discard port, so a PATCH that's sent fails.
#[test]
fn reader_mark_read_is_offered_only_by_a_blyg_that_has_it() {
    let dir = tempfile::tempdir().unwrap();
    let b = backend(dir.path());
    assert_eq!(b.reader_mark_read(), None, "never read: not offered");
    assert!(b.save_reader_mark_read(MarkRead::Opened).is_err());
    let store = Store::open(&dir.path().join("blygger.db")).unwrap();
    store.set_meta(MARK_READ_SERVED_META, "0").unwrap();
    assert_eq!(b.reader_mark_read(), None, "an older studio: not offered");
    assert!(b.save_reader_mark_read(MarkRead::Opened).is_err());
    store.set_meta(MARK_READ_SERVED_META, "1").unwrap();
    store.set_meta(MARK_READ_META, "never").unwrap();
    assert_eq!(b.reader_mark_read(), Some(MarkRead::Never));
    assert!(
        b.save_reader_mark_read(MarkRead::Seen).is_err(),
        "sent, failed"
    );
    assert_eq!(b.reader_mark_read(), Some(MarkRead::Never), "not kept");
}
