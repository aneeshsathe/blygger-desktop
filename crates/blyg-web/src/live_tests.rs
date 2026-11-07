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
    let mut b = Burrow::new_live(NOW, None, BASE);
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
        api::detect(200, r#"{"contractVersion":"6.0.0"}"#, false),
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
    let mut b = Burrow::new_live(NOW, Some(b.to_json()), BASE);
    b.open("aaaaaaaaaaaaaaaaaaaaaaaaaa");
    assert_eq!(v(b.view(NOW))["current"]["conflict"]["server_rev"], 9);

    // Keep mine: sent over theirs, based on their revision.
    let mut mine = Burrow::new_live(NOW, Some(b.to_json()), BASE);
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
    let mut b = Burrow::new_live(NOW, Some(b.to_json()), "");
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
    assert_eq!(
        (r["rows"].as_u64(), r["hoppers"].as_u64()),
        (Some(2), Some(1))
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
        worker.contains(r"if (/^\/(api|studio)(\/|$)/.test(u.pathname)) return false;"),
        "sw.js must pass /api/ and /studio/ straight to the network"
    );
    assert!(worker.contains("if (req.method !== 'GET' || !mine(req.url)) return;"));
    let live = include_str!("../www/live.js");
    assert!(live.contains("if (!apiPath(path)) throw"));
    assert!(live.contains("credentials: 'same-origin'"));
    // Nothing in the page reads a cookie or sends a token.
    for src in [
        live,
        include_str!("../www/core.js"),
        include_str!("../www/app.js"),
    ] {
        assert!(!src.contains("document.cookie"));
        assert!(!src.to_lowercase().contains("authorization"));
    }
}
