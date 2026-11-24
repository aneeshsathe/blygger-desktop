//! What Burrow reads of a publish's answer (`publishItem`, `POST
//! /api/items/{id}/publish`). The tangle e2e (e2e/tangle.rs) publishes for
//! real; this pins the answer's shape, without a server.

use blyg_core::api::wire::Published;
use serde_json::json;

/// A tangled thread's publish answers with the `parts` it made, in source
/// order, each by its id and version; `Live::publish` then reads each part
/// whole and holds it.
#[test]
fn a_publish_reads_the_parts_it_made() {
    // publishItem's 200, as studio 0.69 sends it after `tangle: true`.
    let p: Published = serde_json::from_value(json!({
        "ok": true, "revision": 7, "version": 1,
        "parts": [{ "id": "part-a", "version": 1 }, { "id": "part-b", "version": 2 }]
    }))
    .expect("a publishItem answer");
    assert_eq!((p.version, p.revision), (1, 7));
    let parts: Vec<(&str, u32)> = p.parts.iter().map(|x| (x.id.as_str(), x.version)).collect();
    assert_eq!(parts, [("part-a", 1), ("part-b", 2)], "source order");

    // An untangled publish has no "parts": none made.
    let p: Published =
        serde_json::from_value(json!({ "ok": true, "revision": 8, "version": 2 })).unwrap();
    assert!(p.parts.is_empty());
}
