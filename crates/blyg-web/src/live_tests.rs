//! Live mode and the API layer, driven as `www/live.js` drives them, with
//! the blyg's answers written out by hand (shapes from contract 5.7.0).

use serde_json::{Value, json};

use crate::Burrow;
use crate::api::{self, ApiError, Mode, Pager};

/// 2026-11-06T12:00:00Z
const NOW: f64 = 1_793_966_400_000.0;
const BASE: &str = "https://blyg.example.com";

fn v(s: String) -> Value {
    serde_json::from_str(&s).unwrap()
}

fn wire(id: &str, rev: u64, md: &str, status: &str, version: u32) -> Value {
    json!({
        "id": id, "revision": rev, "kind": "fragment", "status": status,
        "created": "2026-11-01T10:00:00Z", "updated": "2026-11-05T10:00:00Z",
        "version": version, "contentMd": md, "dirty": false, "stubOf": null,
        "forkedFrom": null, "provenance": [], "responses": "default", "highlight": "default"
    })
}

fn live() -> Burrow {
    let mut b = Burrow::new_live(NOW, None, None, BASE);
    let items = json!([
        wire(
            "aaaaaaaaaaaaaaaaaaaaaaaaaa",
            3,
            "A draft about tides",
            "draft",
            0
        ),
        wire(
            "bbbbbbbbbbbbbbbbbbbbbbbbbb",
            7,
            "A published note",
            "public",
            2
        ),
    ]);
    b.live_items(&items.to_string());
    b
}

fn next(b: &Burrow) -> Value {
    v(b.outbox_next("[]"))
}

/// Answer the outbox's next entry with `status` and `body`.
fn answer(b: &mut Burrow, status: u16, body: &Value) -> Value {
    let mut n = next(b);
    let op = {
        n.as_object_mut().unwrap().remove("req");
        n
    };
    v(b.outbox_answer(&op.to_string(), status, &body.to_string(), None))
}

// ------------------------------------------------------------ errors

#[test]
fn answers_map_to_what_they_mean() {
    assert_eq!(api::read(200, "", None), Ok(Value::Null));
    assert_eq!(api::read(0, "", None), Err(ApiError::Offline));
    assert_eq!(
        api::read(
            401,
            r#"{"code":"unauthorized","message":"unauthorized"}"#,
            None
        ),
        Err(ApiError::Unauthorized)
    );
    assert_eq!(
        api::read(
            403,
            r#"{"code":"forbidden","message":"cross-origin owner request denied"}"#,
            None
        ),
        Err(ApiError::Forbidden {
            message: "cross-origin owner request denied".into()
        })
    );
    assert_eq!(api::read(404, "", None), Err(ApiError::NotFound));
    assert_eq!(
        api::read(
            409,
            r#"{"code":"revision-conflict","message":"stale","details":{}}"#,
            None
        ),
        Err(ApiError::RevisionConflict)
    );
    assert_eq!(
        api::read(
            409,
            r#"{"code":"cursor-expired","message":"restart"}"#,
            None
        ),
        Err(ApiError::CursorExpired)
    );
    assert_eq!(
        api::read(429, "{}", Some("12")),
        Err(ApiError::RateLimited { retry_after: 12 })
    );
    assert_eq!(
        api::read(429, "{}", None),
        Err(ApiError::RateLimited { retry_after: 60 })
    );
    let e = api::read(
        422,
        r#"{"code":"validation","message":"Invalid body","details":{"issues":[{"path":["contentMd"],"message":"too long"}]}}"#,
        None,
    )
    .unwrap_err();
    assert_eq!(
        e,
        ApiError::Rejected {
            status: 422,
            message: "Invalid body".into(),
            details: vec!["contentMd: too long".into()]
        }
    );
    assert_eq!(e.message(), "Invalid body: contentMd: too long");
    // A 500 with an HTML page still reads as a rejection, with a message.
    let e = api::read(500, "<html>oops</html>", None).unwrap_err();
    assert!(e.transient());
    assert_eq!(e.message(), "The blyg answered 500");
    // The JSON the page gets carries a code and a line to show, nothing else.
    let j = ApiError::Unauthorized.to_json();
    assert_eq!(j["code"], "unauthorized");
    assert!(j["text"].as_str().unwrap().contains("Sign in at Studio"));
}

#[test]
fn a_blyg_is_told_apart_from_a_demo_server() {
    let caps = r#"{"contractVersion":"5.7.0","endpoints":{}}"#;
    assert_eq!(
        api::detect(200, caps, false),
        Mode::Live {
            contract: "5.7.0".into()
        }
    );
    let unauth = r#"{"code":"unauthorized","message":"unauthorized"}"#;
    assert_eq!(api::detect(401, unauth, true), Mode::SignIn);
    // A 401 without the blyg's challenge isn't a blyg.
    assert_eq!(api::detect(401, "nope", false), Mode::Demo);
    assert_eq!(api::detect(404, "<h1>Not found</h1>", false), Mode::Demo);
    assert_eq!(api::detect(200, "<!doctype html>", false), Mode::Demo);
    assert_eq!(api::detect(0, "", false), Mode::Offline);
    assert!(matches!(
        api::detect(200, r#"{"contractVersion":"4.2.0"}"#, false),
        Mode::Unsupported { .. }
    ));
    assert!(matches!(
        api::detect(200, r#"{"contractVersion":"7.0.0"}"#, false),
        Mode::Unsupported { .. }
    ));
    assert_eq!(v(Burrow::detect(200, caps, false))["mode"], "live");
}

// ------------------------------------------------------------ paging

fn page(items: &[i32], next: Option<&str>) -> String {
    json!({ "items": items, "page": { "limit": 2, "nextCursor": next } }).to_string()
}

#[test]
fn a_list_is_read_to_its_last_page() {
    let mut p = Pager::new("/api/items", "", 2);
    assert_eq!(p.next_path().unwrap(), "/api/items?limit=2");
    assert_eq!(p.feed(200, &page(&[1, 2], Some("c/1+x")), None), Ok(true));
    // The cursor is opaque and goes back percent-encoded.
    assert_eq!(
        p.next_path().unwrap(),
        "/api/items?limit=2&cursor=c%2F1%2Bx"
    );
    assert_eq!(p.feed(200, &page(&[3], None), None), Ok(false));
    assert_eq!(p.next_path(), None);
    assert_eq!(p.take_items(), vec![json!(1), json!(2), json!(3)]);

    let p = Pager::new("/api/mentions", "direction=inbound", 50);
    assert_eq!(
        p.next_path().unwrap(),
        "/api/mentions?direction=inbound&limit=50"
    );
}

#[test]
fn an_expired_cursor_starts_the_list_again_never_partial() {
    let mut p = Pager::new("/api/items", "", 2);
    p.feed(200, &page(&[1, 2], Some("c1")), None).unwrap();
    let expired = r#"{"code":"cursor-expired","message":"list changed"}"#;
    assert_eq!(p.feed(409, expired, None), Ok(true));
    // Back to the first page, and what was read is dropped.
    assert_eq!(p.next_path().unwrap(), "/api/items?limit=2");
    p.feed(200, &page(&[9], None), None).unwrap();
    assert_eq!(p.take_items(), vec![json!(9)]);

    // It gives up after CURSOR_RESTARTS restarts.
    let mut p = Pager::new("/api/items", "", 2);
    let mut last = Ok(true);
    for _ in 0..=blyg_core::api::CURSOR_RESTARTS {
        last = p.feed(409, expired, None);
    }
    assert_eq!(last, Err(ApiError::CursorExpired));
    assert_eq!(p.next_path(), None);
}

#[test]
fn a_page_that_repeats_its_cursor_is_an_error_not_a_loop() {
    let mut p = Pager::new("/api/items", "", 2);
    p.feed(200, &page(&[1], Some("same")), None).unwrap();
    assert!(matches!(
        p.feed(200, &page(&[1], Some("same")), None),
        Err(ApiError::Rejected { .. })
    ));
    assert_eq!(p.next_path(), None);
    // And an answer that isn't a page is an error too.
    let mut p = Pager::new("/api/items", "", 2);
    assert!(p.feed(200, "[1,2]", None).is_err());
    let mut p = Pager::new("/api/items", "", 2);
    assert_eq!(p.feed(401, "{}", None), Err(ApiError::Unauthorized));
}

#[test]
fn requests_carry_the_revision_they_were_based_on() {
    let r = api::save_item("a b", 4, "text", None);
    assert_eq!((r.method, r.path.as_str()), ("PATCH", "/api/items/a%20b"));
    assert_eq!(
        r.body.unwrap(),
        json!({ "expectedRevision": 4, "contentMd": "text" })
    );
    let r = api::save_item("x", 1, "t", Some(blyg_core::Kind::Thread));
    assert_eq!(r.body.unwrap()["kind"], "thread");
    let r = api::publish("x", 9, Some("  fixed a typo "));
    assert_eq!(r.path, "/api/items/x/publish");
    assert_eq!(
        r.body.unwrap(),
        json!({ "expectedRevision": 9, "note": "fixed a typo" })
    );
    assert_eq!(
        api::publish("x", 9, Some("  ")).body.unwrap(),
        json!({ "expectedRevision": 9 })
    );
    assert_eq!(
        api::delete_item("x", 2).body.unwrap(),
        json!({ "expectedRevision": 2 })
    );
    assert_eq!(api::read_mark("s", "r", Some(3)).method, "PUT");
    assert_eq!(api::read_mark("s", "r", None).method, "DELETE");
    assert_eq!(
        api::read_mark("s/1", "r", None).path,
        "/api/reading/s%2F1/r/read"
    );
    let c = api::create_item("hello", blyg_core::Kind::Fragment, None);
    assert_eq!(
        c.body.unwrap(),
        json!({ "mode": "blank", "contentMd": "hello", "kind": "fragment" })
    );
}

// ------------------------------------------------------------ live mode

#[test]
fn live_mode_lists_the_blygs_posts_and_says_so() {
    let b = live();
    let view = v(b.view(NOW));
    assert_eq!(view["mode"], "live");
    assert!(
        view["mode_label"]
            .as_str()
            .unwrap()
            .contains("blyg.example.com")
    );
    assert_eq!(view["rows"].as_array().unwrap().len(), 2);
    assert!(b.is_live_mode());
    assert_eq!(next(&b), Value::Null, "nothing to send after a fresh read");
    // The demo says it's a demo.
    let d = Burrow::new(NOW, None);
    assert_eq!(v(d.view(NOW))["mode"], "demo");
    assert!(!d.is_live_mode());
}

#[test]
fn an_edit_is_saved_with_its_revision_and_the_revision_moves_on() {
    let mut b = live();
    b.open("aaaaaaaaaaaaaaaaaaaaaaaaaa");
    assert!(b.edit("A draft about tides, edited", NOW));
    assert_eq!(v(b.view(NOW))["current"]["pending"], true);
    let n = next(&b);
    assert_eq!(n["op"], "save");
    assert_eq!(n["req"]["method"], "PATCH");
    assert_eq!(n["req"]["path"], "/api/items/aaaaaaaaaaaaaaaaaaaaaaaaaa");
    assert_eq!(n["req"]["body"]["expectedRevision"], 3);
    assert_eq!(n["req"]["body"]["contentMd"], "A draft about tides, edited");
    let step = answer(
        &mut b,
        200,
        &wire(
            "aaaaaaaaaaaaaaaaaaaaaaaaaa",
            4,
            "A draft about tides, edited",
            "draft",
            0,
        ),
    );
    assert_eq!(step["stop"], false);
    assert_eq!(next(&b), Value::Null);
    assert_eq!(v(b.view(NOW))["current"]["pending"], false);
    // The next edit is based on revision 4.
    b.edit("Again", NOW);
    assert_eq!(next(&b)["req"]["body"]["expectedRevision"], 4);
}

#[test]
fn typing_while_a_save_is_in_flight_stays_pending() {
    let mut b = live();
    b.open("aaaaaaaaaaaaaaaaaaaaaaaaaa");
    b.edit("one", NOW);
    let mut n = next(&b);
    n.as_object_mut().unwrap().remove("req");
    b.edit("one two", NOW);
    let body = wire("aaaaaaaaaaaaaaaaaaaaaaaaaa", 4, "one", "draft", 0);
    b.outbox_answer(&n.to_string(), 200, &body.to_string(), None);
    let again = next(&b);
    assert_eq!(again["req"]["body"]["contentMd"], "one two");
    assert_eq!(again["req"]["body"]["expectedRevision"], 4);
    assert_eq!(v(b.view(NOW))["current"]["content"], "one two");
}

#[test]
fn a_new_draft_is_created_then_saved() {
    let mut b = live();
    let id = b.create_draft("Brand new", NOW);
    let n = next(&b);
    assert_eq!(n["op"], "create");
    assert_eq!(n["req"]["method"], "POST");
    assert_eq!(n["req"]["path"], "/api/items");
    assert_eq!(n["req"]["body"]["contentMd"], "Brand new");
    answer(
        &mut b,
        201,
        &wire("cccccccccccccccccccccccccc", 1, "Brand new", "draft", 0),
    );
    assert_eq!(next(&b), Value::Null);
    assert_eq!(b.current().as_deref(), Some(id.as_str()));
    b.edit("Brand new, more", NOW);
    let n = next(&b);
    assert_eq!(n["op"], "save");
    assert_eq!(n["req"]["path"], "/api/items/cccccccccccccccccccccccccc");
    assert_eq!(n["req"]["body"]["expectedRevision"], 1);
}

#[test]
fn scratch_notes_stay_in_this_browser() {
    let mut b = live();
    b.new_post();
    b.edit("a private thought", NOW);
    assert_eq!(next(&b), Value::Null);
    // ⌘D makes it a draft, which goes to the blyg.
    b.make_draft(NOW);
    assert_eq!(next(&b)["op"], "create");
}

#[test]
fn a_conflict_keeps_both_copies_until_you_choose() {
    let mut b = live();
    b.open("aaaaaaaaaaaaaaaaaaaaaaaaaa");
    b.edit("my words", NOW);
    let step = answer(
        &mut b,
        409,
        &json!({ "code": "revision-conflict", "message": "stale revision", "details": {} }),
    );
    assert_eq!(step["fetch"], "/api/items/aaaaaaaaaaaaaaaaaaaaaaaaaa");
    assert_eq!(step["conflict"], "aaaaaaaaaaaaaaaaaaaaaaaaaa");
    // Never silently sent again while the conflict stands.
    assert_eq!(next(&b), Value::Null);
    let theirs = wire(
        "aaaaaaaaaaaaaaaaaaaaaaaaaa",
        9,
        "their words, from Studio",
        "draft",
        0,
    );
    b.live_conflict("aaaaaaaaaaaaaaaaaaaaaaaaaa", 200, &theirs.to_string());
    let cur = &v(b.view(NOW))["current"];
    assert_eq!(cur["content"], "my words");
    assert_eq!(cur["conflict"]["server_rev"], 9);
    assert!(
        cur["conflict"]["excerpt"]
            .as_str()
            .unwrap()
            .contains("their words")
    );
    // It survives a reload.
    let mut b = Burrow::new_live(NOW, Some(b.to_json()), None, BASE);
    b.open("aaaaaaaaaaaaaaaaaaaaaaaaaa");
    assert_eq!(v(b.view(NOW))["current"]["conflict"]["server_rev"], 9);

    // Keep mine: sent over theirs, based on their revision.
    let mut mine = Burrow::new_live(NOW, Some(b.to_json()), None, BASE);
    mine.open("aaaaaaaaaaaaaaaaaaaaaaaaaa");
    mine.resolve_conflict(true);
    let n = next(&mine);
    assert_eq!(n["req"]["body"]["expectedRevision"], 9);
    assert_eq!(n["req"]["body"]["contentMd"], "my words");
    assert_eq!(v(mine.view(NOW))["current"]["conflict"], Value::Null);

    // Use theirs: their text, nothing to send.
    b.resolve_conflict(false);
    assert_eq!(
        v(b.view(NOW))["current"]["content"],
        "their words, from Studio"
    );
    assert_eq!(next(&b), Value::Null);
}

#[test]
fn a_conflict_whose_copy_cant_be_fetched_is_tried_again() {
    let mut b = live();
    b.open("aaaaaaaaaaaaaaaaaaaaaaaaaa");
    b.edit("mine", NOW);
    answer(
        &mut b,
        409,
        &json!({ "code": "revision-conflict", "message": "stale" }),
    );
    assert_eq!(next(&b), Value::Null);
    b.live_conflict("aaaaaaaaaaaaaaaaaaaaaaaaaa", 0, "");
    assert_eq!(next(&b)["req"]["body"]["contentMd"], "mine");
}

#[test]
fn offline_edits_wait_survive_a_reload_and_replay() {
    let mut b = live();
    b.set_offline(true);
    b.open("bbbbbbbbbbbbbbbbbbbbbbbbbb");
    b.edit("A published note, edited on a train", NOW);
    let view = v(b.view(NOW));
    assert!(
        view["sync"].as_str().unwrap().contains('1'),
        "{}",
        view["sync"]
    );
    // A send that finds no network stops, and keeps the edit.
    let step = answer(&mut b, 0, &json!(null));
    assert_eq!(step["stop"], true);
    assert_eq!(step["error"]["code"], "offline");
    // Reload with no network: the edit is still waiting.
    let mut b = Burrow::new_live(NOW, Some(b.to_json()), None, "");
    let n = next(&b);
    assert_eq!(
        n["req"]["body"]["contentMd"],
        "A published note, edited on a train"
    );
    assert_eq!(n["req"]["body"]["expectedRevision"], 7);
    // A published post's kind never goes with it.
    assert!(n["req"]["body"].get("kind").is_none());
    // A fresh list from the blyg doesn't overwrite it.
    let items = json!([wire(
        "bbbbbbbbbbbbbbbbbbbbbbbbbb",
        7,
        "A published note",
        "public",
        2
    )]);
    b.live_items(&items.to_string());
    b.open("bbbbbbbbbbbbbbbbbbbbbbbbbb");
    assert_eq!(
        v(b.view(NOW))["current"]["content"],
        "A published note, edited on a train"
    );
    // Back online, it goes.
    let mut done = wire(
        "bbbbbbbbbbbbbbbbbbbbbbbbbb",
        8,
        "A published note, edited on a train",
        "public",
        2,
    );
    done["dirty"] = json!(true);
    answer(&mut b, 200, &done);
    assert_eq!(next(&b), Value::Null);
    assert_eq!(v(b.view(NOW))["current"]["pending"], false);
}

#[test]
fn signed_out_stops_the_outbox_and_keeps_everything() {
    let mut b = live();
    b.open("aaaaaaaaaaaaaaaaaaaaaaaaaa");
    b.edit("kept", NOW);
    let step = answer(
        &mut b,
        401,
        &json!({ "code": "unauthorized", "message": "unauthorized" }),
    );
    assert_eq!(step["stop"], true);
    assert_eq!(step["error"]["code"], "unauthorized");
    assert_eq!(next(&b)["req"]["body"]["contentMd"], "kept");
}

#[test]
fn a_list_without_a_post_removes_it_unless_it_holds_unsent_words() {
    let mut b = live();
    b.open("aaaaaaaaaaaaaaaaaaaaaaaaaa");
    b.edit("unsent", NOW);
    b.live_items("[]");
    let rows = v(b.view(NOW))["rows"].as_array().unwrap().len();
    assert_eq!(rows, 1, "the edited draft stays, the clean post goes");
    // Saving it finds it gone: the text stays, as a new draft.
    let step = answer(
        &mut b,
        404,
        &json!({ "code": "not-found", "message": "no such item" }),
    );
    assert!(
        step["toast"]
            .as_str()
            .unwrap()
            .contains("deleted elsewhere")
    );
    assert_eq!(next(&b)["op"], "create");
}

#[test]
fn deleting_a_draft_deletes_it_on_the_blyg_with_its_revision() {
    let mut b = live();
    b.open("aaaaaaaaaaaaaaaaaaaaaaaaaa");
    b.delete();
    let n = next(&b);
    assert_eq!(n["op"], "delete");
    assert_eq!(n["req"]["method"], "DELETE");
    assert_eq!(n["req"]["body"]["expectedRevision"], 3);
    // A list read before the delete is sent doesn't bring it back.
    let items = json!([wire(
        "aaaaaaaaaaaaaaaaaaaaaaaaaa",
        3,
        "A draft about tides",
        "draft",
        0
    )]);
    b.live_items(&items.to_string());
    assert!(
        !v(b.view(NOW))["rows"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["id"] == "aaaaaaaaaaaaaaaaaaaaaaaaaa")
    );
    answer(&mut b, 200, &json!({ "ok": true }));
    assert_eq!(next(&b), Value::Null);
}

#[test]
fn publishing_waits_for_the_edits_then_reads_the_post_again() {
    let mut b = live();
    b.open("aaaaaaaaaaaaaaaaaaaaaaaaaa");
    b.edit("Ready to go", NOW);
    let o = v(b.publish("first light", NOW));
    assert_eq!(o["action"], "remote_publish");
    assert_eq!(o["note"], "first light");
    let id = o["id"].as_str().unwrap().to_string();
    // Not before the edit is on the blyg.
    assert!(v(b.live_publish_request(&id, "first light"))["error"].is_string());
    answer(&mut b, 200, &wire(&id, 4, "Ready to go", "draft", 0));
    let pr = v(b.live_publish_request(&id, "first light"));
    assert_eq!(pr["req"]["path"], format!("/api/items/{id}/publish"));
    assert_eq!(
        pr["req"]["body"],
        json!({ "expectedRevision": 4, "note": "first light" })
    );
    let a = v(b.live_command_answer(&id, 200, r#"{"ok":true,"revision":5,"version":1}"#));
    assert_eq!(a["fetch"], format!("/api/items/{id}"));
    let after = wire(&id, 5, "Ready to go", "public", 1);
    let done = v(b.live_apply_item(&id, 200, &after.to_string(), "first light"));
    assert_eq!(done["action"], "published");
    assert_eq!(done["version"], 1);
    assert_eq!(done["permalink"], format!("{BASE}/f/{id}"));
    assert_eq!(v(b.view(NOW))["current"]["status"], "public");
    // A stale publish is a conflict, fetched, not forced.
    let a = v(b.live_command_answer(
        &id,
        409,
        r#"{"code":"revision-conflict","message":"stale"}"#,
    ));
    assert_eq!(a["conflict"], id);
    assert_eq!(a["error"]["code"], "revision-conflict");
}

#[test]
fn discarding_changes_restores_on_the_blyg() {
    let mut b = live();
    b.open("bbbbbbbbbbbbbbbbbbbbbbbbbb");
    b.edit("A published note, changed", NOW);
    let o = v(b.discard(NOW));
    assert_eq!(o["action"], "remote_restore");
    assert_eq!(o["version"], 2);
    let r = v(b.live_restore_request("bbbbbbbbbbbbbbbbbbbbbbbbbb", 2));
    assert_eq!(
        r["req"]["path"],
        "/api/items/bbbbbbbbbbbbbbbbbbbbbbbbbb/restore"
    );
    assert_eq!(
        r["req"]["body"],
        json!({ "expectedRevision": 7, "version": 2 })
    );
}

#[test]
fn a_published_posts_kind_stays() {
    let mut b = live();
    b.open("bbbbbbbbbbbbbbbbbbbbbbbbbb");
    let o = v(b.toggle_kind(NOW));
    assert!(o["message"].as_str().unwrap().contains("keeps its kind"));
}

fn reading_row(sub: &str, rid: &str, version: u32, read: Option<u32>) -> Value {
    json!({
        "subscriptionId": sub, "remoteId": rid, "subscriptionTitle": "Tide Tables",
        "origin": "https://tides.example.com", "kind": "fragment", "version": version,
        "created": "2026-11-04T09:00:00Z", "updated": "2026-11-04T09:00:00Z",
        "observedAt": "2026-11-04T09:05:00Z", "contentMd": "High water at dawn.",
        "contentHtml": "<p>High water at dawn.</p>", "readVersion": read, "hoppers": []
    })
}

#[test]
fn reading_marks_go_to_the_blyg() {
    let mut b = live();
    let rows = json!([
        reading_row("sub1", "r1", 2, None),
        reading_row("sub1", "r2", 1, Some(1))
    ]);
    let hoppers = json!([{ "id": "h1", "name": "Tides", "slug": null, "public": false }]);
    let r = v(b.live_reading(&rows.to_string(), &hoppers.to_string()));
    // Two of theirs and your published note.
    assert_eq!(
        (r["rows"].as_u64(), r["hoppers"].as_u64()),
        (Some(3), Some(1))
    );
    let rv = v(b.reading_view(NOW, "", false));
    assert_eq!(rv["unread"], 1);
    b.set_read("sub1|r1", true);
    let n = next(&b);
    assert_eq!(n["op"], "read");
    assert_eq!(n["req"]["method"], "PUT");
    assert_eq!(n["req"]["path"], "/api/reading/sub1/r1/read");
    assert_eq!(n["req"]["body"], json!({ "version": 2 }));
    // Read again before it's sent: one mark, the latest.
    b.set_read("sub1|r1", false);
    let n = next(&b);
    assert_eq!(n["req"]["method"], "DELETE");
    answer(&mut b, 200, &json!({ "ok": true }));
    assert_eq!(next(&b), Value::Null);
    // A list read before a mark is sent keeps the mark on top.
    b.set_read("sub1|r2", false);
    b.live_reading(&rows.to_string(), &hoppers.to_string());
    assert_eq!(v(b.reading_view(NOW, "", false))["unread"], 2);
    answer(&mut b, 200, &json!({}));
    // Hoppers too.
    b.toggle_hopper("sub1|r1", "h1");
    let n = next(&b);
    assert_eq!(
        (n["op"].as_str(), n["req"]["method"].as_str()),
        (Some("hop"), Some("PUT"))
    );
    assert_eq!(n["req"]["path"], "/api/hoppers/h1/items/sub1/r1");
}

#[test]
fn the_page_only_calls_its_own_api_and_the_worker_never_touches_it() {
    let worker = include_str!("../www/sw.js");
    assert!(
        worker.contains(r"if (/\/(api|studio)(\/|$)/.test(u.pathname)) return false;"),
        "sw.js must pass /api/ and /studio/ straight to the network"
    );
    assert!(worker.contains("if (req.method !== 'GET' || !mine(req.url)) return;"));
    // One door: net.js `call`, which refuses anything but a same-origin
    // /api/ path. No other page module fetches anything.
    let net = include_str!("../www/net.js");
    assert!(net.contains("if (!apiPath(path)) throw"));
    assert!(net.contains("credentials: 'same-origin'"));
    assert!(net.contains("mode: 'same-origin'"));
    // Its early reads go through the same guard, and only GETs are taken.
    assert!(net.contains("if (apiPath(path) && !early.has(path))"));
    assert!(net.contains("const e = method === 'GET' && !etag ? early.get(path) : undefined;"));
    // Besides the app's own WebAssembly, a fixed path beside the page
    // (flow_tests.rs `the_wasm_is_fetched_once`).
    let wasm = "export const WASM = fetch(new URL('pkg/blyg_web_bg.wasm', import.meta.url));";
    assert!(net.contains(wasm));
    assert_eq!(
        net.replace(wasm, "").matches(" fetch(").count(),
        1,
        "net.js fetches in one place"
    );
    let live = include_str!("../www/live.js");
    for (name, src) in PAGE_MODULES {
        if *name != "net.js" {
            assert!(
                !src.contains("fetch("),
                "{name} must call net.js `call`, not fetch"
            );
        }
    }
    // Nothing in the page reads a cookie or sends a token.
    for src in [
        net,
        live,
        include_str!("../www/core.js"),
        include_str!("../www/app.js"),
        include_str!("../www/hub.js"),
    ] {
        assert!(!src.contains("document.cookie"));
        assert!(!src.to_lowercase().contains("authorization"));
    }
    // The Blyg list only opens other screens: it fetches nothing itself.
    assert!(!include_str!("../www/hub.js").contains("fetch("));
}

/// The page's modules (not the service worker), as index.html loads them.
const PAGE_MODULES: &[(&str, &str)] = &[
    ("app.js", include_str!("../www/app.js")),
    ("core.js", include_str!("../www/core.js")),
    ("desk.js", include_str!("../www/desk.js")),
    ("env.js", include_str!("../www/env.js")),
    ("hub.js", include_str!("../www/hub.js")),
    ("keys.js", include_str!("../www/keys.js")),
    ("live.js", include_str!("../www/live.js")),
    ("net.js", include_str!("../www/net.js")),
    ("parity.js", include_str!("../www/parity.js")),
    ("phone.js", include_str!("../www/phone.js")),
    ("web-reading.js", include_str!("../www/web-reading.js")),
    ("writing.js", include_str!("../www/writing.js")),
];

#[test]
fn a_start_asks_for_every_module_and_the_wasm_at_once() {
    let html = include_str!("../www/index.html");
    // Every module is preloaded (no import-by-import waterfall), after the
    // import map (a module fetched before it would make the browser drop
    // the map, and `env` wouldn't resolve).
    let map = html.find(r#"<script type="importmap">"#).unwrap();
    for (name, src) in PAGE_MODULES {
        let found = if *name == "net.js" {
            html.find(r#"<script type="module" async src="net.js">"#)
        } else {
            html.find(&format!(r#"<link rel="modulepreload" href="{name}">"#))
        };
        let at = found.unwrap_or_else(|| panic!("index.html must preload {name}"));
        assert!(at > map, "{name} is preloaded before the import map");
        // And every module a module imports is one of them.
        for line in src
            .lines()
            .filter(|l| l.starts_with("import ") || l.starts_with("} from "))
        {
            if let Some(dep) = line
                .split("from './")
                .nth(1)
                .and_then(|d| d.split('\'').next())
            {
                assert!(
                    PAGE_MODULES.iter().any(|(n, _)| *n == dep) || dep == "pkg/blyg_web.js",
                    "{name} imports {dep}, which index.html doesn't preload"
                );
            }
        }
    }
    assert!(html.contains(r#"<link rel="modulepreload" href="pkg/blyg_web.js">"#));
    // The wasm is asked for by net.js, the first module to run (a preload
    // was fetched twice by WebKit: flow_tests.rs `the_wasm_is_fetched_once`).
    assert!(include_str!("../www/net.js").contains("export const WASM = fetch("));
}

// ---------------------------------------------------------------- speed (0.18.1)

fn reading_rows(n: usize) -> Value {
    Value::Array(
        (0..n)
            .map(|i| reading_row("sub1", &format!("r{i}"), 1, (i % 2 == 0).then_some(1)))
            .collect(),
    )
}

#[test]
fn the_store_snapshot_leaves_the_reading_list_to_its_own() {
    let mut b = live();
    b.live_reading(&reading_rows(50).to_string(), "[]");
    // Every change writes `to_json`: it must not carry 50 (or 4,000) posts.
    let snap = b.to_json();
    assert!(!snap.contains("High water at dawn"), "{snap}");
    assert!(b.reading_dirty());
    let cache = b.reading_json();
    assert!(!b.reading_dirty());
    assert!(cache.contains("High water at dawn"));
    // A mark after the list was written still shows after a reload, sent
    // (settled) or not.
    b.set_read("sub1|r1", true);
    assert!(b.reading_dirty());
    answer(&mut b, 200, &json!({ "ok": true }));
    assert_eq!(next(&b), Value::Null);
    let again = Burrow::new_live(NOW, Some(b.to_json()), Some(cache.clone()), BASE);
    let rv = v(again.reading_view(NOW, "", false));
    assert_eq!(rv["total"], 51, "50 of theirs and your published note");
    assert_eq!(rv["unread"], 24);
    // Without its snapshot, the reading list holds only your own post
    // (built from the store) until read.
    let bare = Burrow::new_live(NOW, Some(b.to_json()), None, BASE);
    assert_eq!(v(bare.reading_view(NOW, "", false))["total"], 1);
    // The blyg's list is the truth again once read: the local marks go.
    let mut b = again;
    b.live_reading(&reading_rows(50).to_string(), "[]");
    assert_eq!(v(b.reading_view(NOW, "", false))["unread"], 25);
}

#[test]
fn a_snapshot_from_0_18_0_moves_its_reading_list_out() {
    let mut b = live();
    b.live_reading(&reading_rows(3).to_string(), "[]");
    // 0.18.0 kept the list inside the store's snapshot.
    let mut old: Value = serde_json::from_str(&b.to_json()).unwrap();
    old["live"]["reading"] = v(b.reading_json())["rows"].clone();
    let mut b = Burrow::new_live(NOW, Some(old.to_string()), None, BASE);
    // Three of theirs and your published note (built again, not doubled).
    assert_eq!(v(b.reading_view(NOW, "", false))["total"], 4);
    assert!(b.reading_dirty(), "written to its own snapshot next");
    assert!(!b.to_json().contains("High water at dawn"));
    assert!(b.reading_json().contains("High water at dawn"));
}

#[test]
fn the_stream_is_drawn_a_page_at_a_time() {
    // No posts of yours: every row here is theirs.
    let mut b = Burrow::new_live(NOW, None, None, BASE);
    b.live_reading(&reading_rows(120).to_string(), "[]");
    let all = v(b.reading_view(NOW, "", false));
    let page = v(b.reading_page(NOW, "", false, 40));
    assert_eq!(
        (page["total"].as_u64(), page["unread"].as_u64()),
        (Some(120), Some(60))
    );
    let rows = page["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 40);
    assert_eq!(rows[..], all["rows"].as_array().unwrap()[..40]);
    // Counts only (the badge under another screen): no rows at all.
    let none = v(b.reading_page(NOW, "", true, 0));
    assert_eq!(
        (
            none["total"].as_u64(),
            none["rows"].as_array().unwrap().len()
        ),
        (Some(60), 0)
    );
    assert_eq!(none["empty"], Value::Null);
    // A row's excerpt (its text after the title line) follows its text when
    // the blyg's list changes it, even at the same version.
    assert_eq!(rows[0]["excerpt"], "");
    let mut one = reading_rows(1);
    one[0]["contentMd"] = json!("Tides\n\nLow water at noon.");
    b.live_reading(&one.to_string(), "[]");
    assert_eq!(
        v(b.reading_page(NOW, "", false, 1))["rows"][0]["excerpt"],
        "Low water at noon."
    );
}

#[test]
fn the_next_page_of_the_stream_follows_the_cards_drawn() {
    let mut b = Burrow::new_live(NOW, None, None, BASE);
    b.live_reading(&reading_rows(120).to_string(), "[]");
    let all = v(b.reading_view(NOW, "", false));
    let all = all["rows"].as_array().unwrap();
    let next = v(b.reading_range(NOW, "", false, 100, 100));
    assert_eq!(next["total"], 120);
    assert_eq!(next["rows"].as_array().unwrap()[..], all[100..]);
    // Past the end: nothing, with the same counts.
    let past = v(b.reading_range(NOW, "", false, 120, 100));
    assert_eq!(
        (
            past["total"].as_u64(),
            past["rows"].as_array().unwrap().len()
        ),
        (Some(120), 0)
    );
    // Unread only pages through the unread rows.
    let unread = v(b.reading_range(NOW, "", true, 50, 100));
    assert_eq!(unread["rows"].as_array().unwrap().len(), 10);
}

#[test]
fn the_streams_order_is_kept_until_a_row_changes() {
    let mut b = Burrow::new_live(NOW, None, None, BASE);
    // The list's own order, chronological (the Feed defaults to
    // pace-layered, as native: `studio69_tests.rs`).
    b.w_set_settings(r#"{"readerOrder":"newest"}"#);
    let mut rows = reading_rows(3);
    for (i, r) in rows.as_array_mut().unwrap().iter_mut().enumerate() {
        r["updated"] = json!(format!("2026-11-0{}T09:00:00Z", i + 1));
        r["readVersion"] = json!(1);
    }
    b.live_reading(&rows.to_string(), "[]");
    let keys = |b: &Burrow| -> Vec<String> {
        v(b.reading_view(NOW, "", false))["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["key"].as_str().unwrap().to_string())
            .collect()
    };
    assert_eq!(keys(&b), ["sub1|r2", "sub1|r1", "sub1|r0"]);
    assert_eq!(keys(&b), ["sub1|r2", "sub1|r1", "sub1|r0"], "drawn again");
    // The oldest is edited after you read it: it moves to the top, and a
    // withdrawn post (not kept) leaves, each at the next draw.
    rows[0]["version"] = json!(2);
    rows[2]["withdrawn"] = json!(true);
    b.live_reading(&rows.to_string(), "[]");
    assert_eq!(keys(&b), ["sub1|r0", "sub1|r1"]);
    // Read it: back in its place.
    b.open_reading("sub1|r0", NOW);
    assert_eq!(keys(&b), ["sub1|r1", "sub1|r0"]);
}

#[test]
fn a_first_start_draws_the_first_page_while_the_rest_is_read() {
    // `readAll` hands the first page over with `so_far` while it reads on.
    let mut p = crate::live::Pager::new("/api/reading/imported", "", 2);
    let page = |rows: Value, next: Option<&str>| {
        json!({ "items": rows, "page": { "limit": 2, "nextCursor": next } }).to_string()
    };
    let rows = reading_rows(3);
    let rows = rows.as_array().unwrap();
    assert_eq!(
        p.next_path().as_deref(),
        Some("/api/reading/imported?limit=2")
    );
    assert_eq!(
        p.feed(200, &page(json!(rows[..2]), Some("c1")), None),
        "null"
    );
    let first = v(p.so_far());
    assert_eq!(first.as_array().unwrap().len(), 2);
    // Nothing of theirs is held yet (your own posts don't count), so the
    // page draws it at once.
    let mut b = live();
    assert_eq!(b.reading_held(), 0);
    b.live_reading(&first.to_string(), "[]");
    assert_eq!(b.reading_held(), 2);
    assert_eq!(p.feed(200, &page(json!(rows[2..]), None), None), "null");
    assert_eq!(p.next_path(), None);
    b.live_reading(&p.items(), "[]");
    assert_eq!(b.reading_held(), 3);
}

#[test]
fn live_mode_starts_without_the_demos_sample_posts() {
    let b = Burrow::new_live(NOW, None, None, BASE);
    assert_eq!(b.reading_held(), 0);
    assert_eq!(v(b.reading_view(NOW, "", false))["total"], 0);
    assert!(v(b.view(NOW))["rows"].as_array().unwrap().is_empty());
}

fn changes(epoch: &str, items: u64, reading: u64) -> String {
    changes_all(epoch, items, reading, 4, 0)
}

fn changes_all(epoch: &str, items: u64, reading: u64, hoppers: u64, signals: u64) -> String {
    json!({ "epoch": epoch, "domains": {
        "items": items, "settings": 1, "reading": reading, "subscriptions": 2,
        "signals": signals, "hoppers": hoppers, "feed": 9
    } })
    .to_string()
}

fn plan(b: &mut Burrow, status: u16, body: &str, now: f64) -> (bool, bool) {
    let p = v(b.changes_plan(status, body, None, true, now));
    (
        p["items"].as_bool().unwrap(),
        p["reading"].as_bool().unwrap(),
    )
}

/// Accept every list `p` (a `changes_plan` answer) said to read, as a
/// refresh that read them all would.
fn accept_all(b: &mut Burrow, p: &Value, now: f64) {
    let rev = p["rev"].to_string();
    for list in ["items", "reading", "hoppers", "signals"] {
        if p[list] == true {
            b.changes_accept(list, &rev, now, Some(String::new()));
        }
    }
}

/// Which of items, reading, hoppers, signals a plan reads.
fn lists(p: &Value) -> [bool; 4] {
    ["items", "reading", "hoppers", "signals"].map(|l| p[l].as_bool().unwrap())
}

#[test]
fn a_refresh_reads_only_what_changed() {
    let mut b = live();
    let soon = NOW + 1000.0;
    // Never read under these revisions: every list.
    let p = v(b.changes_plan(200, &changes("e1", 5, 7), None, true, NOW));
    assert_eq!(lists(&p), [true; 4]);
    accept_all(&mut b, &p, NOW);
    // Nothing moved: nothing to read.
    assert_eq!(
        plan(&mut b, 200, &changes("e1", 5, 7), soon),
        (false, false)
    );
    // Only the reading list moved (a poll imported posts, a mark landed).
    assert_eq!(plan(&mut b, 200, &changes("e1", 5, 8), soon), (false, true));
    // A new epoch (the database was restored): everything.
    assert_eq!(plan(&mut b, 200, &changes("e2", 5, 7), soon), (true, true));
    // No usable answer: everything, as before /api/changes. (No route at
    // all, contract 6, reads conditionally: `studio69_tests.rs`.)
    assert_eq!(plan(&mut b, 200, "{", soon), (true, true));
    // And everything now and then, as the native sync's safety net.
    let later = NOW + blyg_core::sync::FULL_PULL_EVERY.as_millis() as f64;
    assert_eq!(plan(&mut b, 200, &changes("e1", 5, 7), later), (true, true));
    // What was seen survives a reload: the items' with the store, the
    // reading list's with its own snapshot.
    let cache = b.reading_json();
    let mut again = Burrow::new_live(NOW, Some(b.to_json()), Some(cache), BASE);
    assert_eq!(
        plan(&mut again, 200, &changes("e1", 5, 7), soon),
        (false, false)
    );
    // An accept without a captured answer records nothing.
    let mut c = live();
    c.changes_accept("items", "null", NOW, Some(String::new()));
    c.changes_accept("items", "{", NOW, Some(String::new()));
    assert_eq!(plan(&mut c, 200, &changes("e1", 5, 7), NOW), (true, true));
}

#[test]
fn each_list_follows_its_own_counters_as_native_does() {
    // The web's map is native sync's, constant for constant.
    use blyg_core::sync::{HOPPERS_DEPS, ITEMS_DEPS, READING_DEPS, SHELF_DEPS, SIGNALS_DEPS};
    assert_eq!(
        crate::live::LISTS,
        &[
            ("items", ITEMS_DEPS),
            ("reading", READING_DEPS),
            ("hoppers", HOPPERS_DEPS),
            ("signals", SIGNALS_DEPS),
            ("shelf", SHELF_DEPS), // --- shelf sync ---
        ]
    );
    let mut b = live();
    let soon = NOW + 1000.0;
    let p = v(b.changes_plan(200, &changes_all("e1", 5, 7, 4, 0), None, true, NOW));
    accept_all(&mut b, &p, NOW);
    let at = |b: &mut Burrow, body: String| lists(&v(b.changes_plan(200, &body, None, true, soon)));
    // A hopper changed elsewhere (made, renamed, a post added): the
    // hoppers only, not the whole reading list.
    assert_eq!(
        at(&mut b, changes_all("e1", 5, 7, 5, 0)),
        [false, false, true, false]
    );
    // A thumb changed elsewhere: the signals only.
    assert_eq!(
        at(&mut b, changes_all("e1", 5, 7, 4, 1)),
        [false, false, false, true]
    );
    // Items or settings: the items only.
    assert_eq!(
        at(&mut b, changes_all("e1", 6, 7, 4, 0)),
        [true, false, false, false]
    );
    // A subscription moved (every feed poll moves it): the hoppers, never
    // the whole reading list (native's `READING_DEPS`, issue #71).
    let mut subs: Value = serde_json::from_str(&changes_all("e1", 5, 7, 4, 0)).unwrap();
    subs["domains"]["subscriptions"] = json!(3);
    assert_eq!(at(&mut b, subs.to_string()), [false, false, true, false]);
    // Accepting one list leaves the others' counters alone.
    let p = v(b.changes_plan(200, &changes_all("e1", 5, 7, 5, 1), None, true, soon));
    b.changes_accept("hoppers", &p["rev"].to_string(), soon, Some(String::new()));
    assert_eq!(
        at(&mut b, changes_all("e1", 5, 7, 5, 1)),
        [false, false, false, true]
    );
}

#[test]
fn each_refresh_keeps_its_own_change_state() {
    // Two refreshes overlap: the first planned under reading 7, then a
    // second under reading 8. The first finishing last must record 7,
    // not the 8 the later plan saw, or the change made between them
    // would never be read.
    let mut b = live();
    let first = v(b.changes_plan(200, &changes("e1", 5, 7), None, true, NOW));
    let second = v(b.changes_plan(200, &changes("e1", 5, 8), None, true, NOW));
    b.changes_accept(
        "reading",
        &second["rev"].to_string(),
        NOW,
        Some(String::new()),
    );
    b.changes_accept(
        "reading",
        &first["rev"].to_string(),
        NOW,
        Some(String::new()),
    );
    assert!(plan(&mut b, 200, &changes("e1", 5, 8), NOW + 1.0).1);
    // A run that failed accepts nothing: its lists are still due.
    let mut c = live();
    let _failed = v(c.changes_plan(200, &changes("e1", 5, 7), None, true, NOW));
    assert_eq!(
        plan(&mut c, 200, &changes("e1", 5, 7), NOW + 1.0),
        (true, true)
    );
}

#[test]
fn polling_paces_itself_and_backs_off() {
    let mut b = live();
    // With /api/changes: every 15 s, as Studio (and native's normal cadence).
    let p = v(b.changes_plan(200, &changes("e1", 5, 7), None, true, NOW));
    assert_eq!(p["every"], 15_000.0);
    // Without: native's whole pull interval.
    assert_eq!(
        v(b.changes_plan(404, "", None, true, NOW))["every"],
        60_000.0
    );
    // A 429: read nothing, wait what the blyg asked.
    let p = v(b.changes_plan(429, "", Some("42".into()), true, NOW));
    assert_eq!(p, json!({ "wait": 42_000.0 }));
    assert_eq!(
        v(b.changes_plan(429, "", None, true, NOW)),
        json!({ "wait": 60_000.0 })
    );
    // A 5xx: 30 s, doubling, up to 30 min; a good answer starts over.
    let waits: Vec<f64> = (0..8)
        .map(|_| {
            v(b.changes_plan(503, "", None, true, NOW))["wait"]
                .as_f64()
                .unwrap()
        })
        .collect();
    assert_eq!(
        waits,
        [30e3, 60e3, 120e3, 240e3, 480e3, 960e3, 1800e3, 1800e3]
    );
    b.changes_plan(200, &changes("e1", 5, 7), None, true, NOW);
    assert_eq!(v(b.changes_plan(500, "", None, true, NOW))["wait"], 30e3);
    // A failed read mid-refresh backs off the same way.
    assert_eq!(b.poll_failed(502, None), Some(60e3));
    assert_eq!(b.poll_failed(429, Some("7".into())), Some(7e3));
    assert_eq!(b.poll_failed(404, None), None);
    // Offline and signed out are errors, not plans.
    assert_eq!(
        v(b.changes_plan(0, "", None, true, NOW))["error"]["code"],
        "offline"
    );
    assert_eq!(
        v(b.changes_plan(401, "", None, true, NOW))["error"]["code"],
        "unauthorized"
    );
}

#[test]
fn hoppers_and_thumbs_changed_elsewhere_reach_the_rows() {
    let mut b = live();
    let mut rows = reading_rows(3);
    rows[0]["hoppers"] = json!(["h1"]);
    rows[1]["thumb"] = json!(1);
    let hoppers = json!([{ "id": "h1", "name": "Tides", "slug": null, "public": false }]);
    b.live_reading(&rows.to_string(), &hoppers.to_string());
    b.reading_json();
    let on = |b: &Burrow, key: &str| -> Vec<String> {
        v(b.hoppers(key))
            .as_array()
            .unwrap()
            .iter()
            .filter(|h| h["on"] == true)
            .map(|h| h["id"].as_str().unwrap().to_string())
            .collect()
    };
    assert_eq!(on(&b, "sub1|r0"), ["h1"]);
    // Elsewhere: r0 left h1, r2 joined it, and a new hopper h2 holds r1.
    let hoppers = json!([
        { "id": "h1", "name": "Tides", "slug": null, "public": false },
        { "id": "h2", "name": "Shells", "slug": null, "public": true }
    ]);
    let members = json!({
        "h1": [{ "subscriptionId": "sub1", "remoteId": "r2", "addedAt": "2026-11-05T00:00:00Z" }],
        "h2": [{ "subscriptionId": "sub1", "remoteId": "r1", "addedAt": "2026-11-05T00:00:00Z" }]
    });
    let r = v(b.live_hoppers(&hoppers.to_string(), &members.to_string()));
    assert_eq!(r["changed"], true);
    assert!(b.reading_dirty());
    assert!(on(&b, "sub1|r0").is_empty());
    assert_eq!(on(&b, "sub1|r1"), ["h2"]);
    assert_eq!(on(&b, "sub1|r2"), ["h1"]);
    // The same answer again changes nothing.
    b.reading_json();
    let r = v(b.live_hoppers(&hoppers.to_string(), &members.to_string()));
    assert_eq!(r["changed"], false);
    assert!(!b.reading_dirty());
    // An unsent change of yours stays on top of what the blyg says.
    b.toggle_hopper("sub1|r0", "h2");
    b.live_hoppers(&hoppers.to_string(), &members.to_string());
    assert_eq!(on(&b, "sub1|r0"), ["h2"]);

    // Thumbs: r1's went, r2 got a thumbs-down.
    let thumb = |b: &Burrow, key: &str| v(b.wr_post_extra(key))["thumb"].clone();
    assert_eq!(thumb(&b, "sub1|r1"), 1);
    let signals = json!([{ "subscriptionId": "sub1", "remoteId": "r2", "thumb": -1 }]);
    let r = v(b.live_signals(&signals.to_string()));
    assert_eq!(r["changed"], true);
    assert_eq!(thumb(&b, "sub1|r1"), Value::Null);
    assert_eq!(thumb(&b, "sub1|r2"), -1);
    assert_eq!(v(b.live_signals(&signals.to_string()))["changed"], false);
}

#[test]
fn the_safety_net_backs_off_per_list_as_native_does() {
    const MIN: f64 = 60_000.0;
    let body = changes("e1", 5, 7);
    let due = |b: &mut Burrow, at: f64, visible: bool| -> bool {
        v(b.changes_plan(200, &body, None, visible, at))["items"]
            .as_bool()
            .unwrap()
    };
    let read = |b: &mut Burrow, body: &str, at: f64, fetched: &str| {
        let p = v(b.changes_plan(200, body, None, true, at));
        b.changes_accept("items", &p["rev"].to_string(), at, Some(fetched.into()));
    };
    let mut b = live();
    let mut t = NOW;
    read(&mut b, &body, t, "list 1");
    // The first net: 10 min after the read, and never while hidden.
    assert!(!due(&mut b, t + 10.0 * MIN - 1.0, true));
    assert!(due(&mut b, t + 10.0 * MIN, true));
    assert!(!due(&mut b, t + 10.0 * MIN, false));
    // Each net read that finds the same list doubles the wait: 20, 40,
    // 80 min, then the 2 h cap.
    let mut waits = vec![];
    let mut every = 10.0 * MIN;
    for _ in 0..5 {
        t += every;
        read(&mut b, &body, t, "list 1");
        every = (1..)
            .map(|m| m as f64 * MIN)
            .find(|w| due(&mut b, t + w, true))
            .unwrap();
        waits.push(every / MIN);
    }
    assert_eq!(waits, [20.0, 40.0, 80.0, 120.0, 120.0]);
    // A net read that finds something new: back to 10 min.
    t += every;
    read(&mut b, &body, t, "list 2");
    assert!(due(&mut b, t + 10.0 * MIN, true));
    // Grown again (20 min), then the counters move: back to 10 min too.
    t += 10.0 * MIN;
    read(&mut b, &body, t, "list 2");
    assert!(!due(&mut b, t + 10.0 * MIN, true));
    let moved = changes("e1", 6, 7);
    read(&mut b, &moved, t + MIN, "list 3");
    assert_eq!(
        v(b.changes_plan(200, &moved, None, true, t + 11.0 * MIN))["items"],
        true
    );
    // Settled along with another list (`fetched: None`): its net is left
    // alone, still due 10 min after.
    let mut c = live();
    let p = v(c.changes_plan(200, &body, None, true, NOW));
    c.changes_accept("signals", &p["rev"].to_string(), NOW, None);
    let p = v(c.changes_plan(200, &body, None, true, NOW + 10.0 * MIN));
    assert_eq!(p["signals"], true);
}
