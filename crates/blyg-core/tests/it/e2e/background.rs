//! Studio 0.64 e2e: background. The `burst`, `wordless` and `maintenance`
//! rules (studio 0.64, contract 5.13), a thread's "+N parts", and the
//! partial `backgroundRules` write. Burrow reads the studio's Background
//! lens and works none of it out itself (`blyg_core::background`), so each
//! test makes the posts on blyg `b`, has `a` poll them, and checks what
//! Burrow then shows against what the studio says.

use super::*;
use blyg_core::background::{BURST, BackgroundRules, burst_parts};

/// Every row Burrow holds (reading list, withdrawn posts, your own posts),
/// placed by what it read of the studio's lens, as the app builds them.
fn annotated(me: &LiveBackend) -> Vec<ReadingItem> {
    let mut rows = me.reading();
    rows.extend(me.background_reading());
    let origin = me.base_url().expect("connected");
    rows.extend(blyg_core::own::rows(&me.items(), &origin));
    if let Some(set) = me.background_set() {
        set.annotate(&mut rows);
    }
    rows
}

fn row_of<'a>(rows: &'a [ReadingItem], sub: &str, id: &str) -> &'a ReadingItem {
    rows.iter()
        .find(|r| r.subscription_id == sub && r.remote_id == id)
        .unwrap_or_else(|| panic!("{sub}/{id} isn't held"))
}

/// The studio's rules as stored (`GET /api/settings`).
fn stored_rules(base: &str) -> Value {
    let (st, v) = owner("GET", base, "/api/settings", None);
    assert_eq!(st, 200, "{v}");
    v["backgroundRules"].clone()
}

/// An item's private revision on `b`.
fn src_rev(id: &str) -> u64 {
    let (st, v) = owner_as(
        &cred_b(),
        "GET",
        &url_b(),
        &format!("/api/items/{id}"),
        None,
    );
    assert_eq!(st, 200, "{v}");
    v["revision"].as_u64().expect("a revision")
}

/// A thread's parts as the studio's own list says them: its
/// `ReadingEntry.parts` on `GET /api/reading` (every page, each checked
/// against the contract) as `(sub, id, label)`, empty when it lists none;
/// `None` when the list doesn't hold the thread.
fn studio_parts(base: &str, sub: &str, id: &str) -> Option<Vec<(String, String, String)>> {
    let mut cursor: Option<String> = None;
    loop {
        let mut path = String::from("/api/reading?limit=50");
        if let Some(c) = &cursor {
            path.push_str(&format!("&cursor={}", query_escape(c)));
        }
        let page = conforms("GET", base, &path);
        let entry = page["items"].as_array().unwrap().iter().find(|e| {
            if sub == OWN_SUBSCRIPTION {
                e["own"]["id"] == id
            } else {
                e["imported"]["subscriptionId"] == sub && e["imported"]["remoteId"] == id
            }
        });
        if let Some(e) = entry {
            let s = |v: &Value| v.as_str().unwrap_or_default().to_string();
            // The ReadingEntry's parts: absent while the burst rule is off.
            return Some(
                e["parts"]
                    .as_array()
                    .map(|ps| {
                        ps.iter()
                            .map(|p| (s(&p["sub"]), s(&p["id"]), s(&p["label"])))
                            .collect()
                    })
                    .unwrap_or_default(),
            );
        }
        cursor = Some(page["page"]["nextCursor"].as_str()?.to_string());
    }
}

/// The rules this studio offers; `None` (skip) on a studio before 0.64.
fn offers_burst(me: &LiveBackend) -> Option<BackgroundRules> {
    if !me.background_served() {
        eprintln!("skipped: this studio doesn't advertise setReadingPlacement");
        return None;
    }
    let rules = me.background_rules().unwrap().expect("served");
    if !rules.offers(BURST) {
        eprintln!("skipped: this studio's settings don't list the burst rule");
        return None;
    }
    Some(rules)
}

/// Pull until `ok` holds of the annotated rows (the lens is read on the
/// pull after the change counters move).
fn sync_until(me: &LiveBackend, what: &str, mut ok: impl FnMut(&[ReadingItem]) -> bool) {
    wait_until(what, Duration::from_secs(30), || {
        me.sync_now().unwrap();
        ok(&annotated(me))
    });
}

/// `b` publishes two fragments and then, within the 15-minute window, a
/// thread with words of its own quoting both. `a` polls it: both fragments
/// go to Background as "published with a thread" naming the thread, the thread
/// stays in the foreground and lists them as "+2 parts". With the burst
/// rule off (a partial write: the other rules keep their stored values)
/// the parts come back at once; on again, they fold again.
#[test]
#[ignore = "needs a local Worker: scripts/e2e-local.sh"]
fn a_burst_folds_under_its_thread_and_the_rule_turns_it_off() {
    let e = e2e();
    let src_url = url_b();
    let (d1, d2) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let me = manual(d1.path());
    me.sync_now().unwrap();
    let Some(rules) = offers_burst(&me) else {
        return;
    };
    assert!(rules.on(BURST), "on by default");
    // `superseded` would hold each part too, whatever `burst` says.
    let _superseded = superseded_off(&me);
    let source = backend_at(d2.path(), &src_url, cred_b(), false);
    let t = tag("burst");
    let (_, p1) = published(
        &source,
        Kind::Fragment,
        &format!("{t} the first part"),
        None,
    );
    let (_, p2) = published(
        &source,
        Kind::Fragment,
        &format!("{t} the second part"),
        None,
    );
    let sub = me.subscribe(&src_url, None).unwrap();
    let _unsub = Unsub(&me, sub.id.clone());
    sync_until_held(&me, &[&p1, &p2]);

    let thread_text =
        format!("{t} a thread with words of its own\n\n![[{p1}]]\n\n![[{p2}]]\n\nAnd a close.");
    let (_, thread) = published(&source, Kind::Thread, &thread_text, None);
    poll(&me, &sub.id, &thread, 1);
    let sid = sub.id.to_string();
    sync_until(&me, "the parts in Background", |rows| {
        row_of(rows, &sid, &p1).is_background() && row_of(rows, &sid, &p2).is_background()
    });

    let rows = annotated(&me);
    for p in [&p1, &p2] {
        let b = row_of(&rows, &sid, p).background.clone().unwrap();
        assert_eq!(b.reasons, [BURST], "{p}: {b:?}");
        assert_eq!(b.burst_thread(), Some(thread.as_str()), "detail.thread");
        assert_eq!(b.why().as_deref(), Some("published with a thread"));
    }
    assert!(
        !row_of(&rows, &sid, &thread).is_background(),
        "the thread stays in the foreground"
    );
    let thread_key = (sub.id.clone(), RemoteId::from(&thread));
    let parts = burst_parts(&rows);
    assert_eq!(
        parts.get(&thread_key).cloned().unwrap_or_default(),
        [
            (sub.id.clone(), RemoteId::from(&p1)),
            (sub.id.clone(), RemoteId::from(&p2))
        ],
        "+2 parts, in the order they were made"
    );
    // The studio's list says the same of the thread: the same parts in the
    // same order, each labelled as Burrow labels it (link-label.ts, ported
    // as pick::link_label).
    let mut said = studio_parts(&e.url, &sid, &thread).expect("the thread is listed");
    let mut want: Vec<(String, String, String)> = [&p1, &p2]
        .into_iter()
        .map(|p| {
            let r = row_of(&rows, &sid, p);
            let label = blyg_core::pick::link_label(&r.content_html, r.kind);
            (sid.clone(), p.to_string(), label)
        })
        .collect();
    // The studio orders parts by `created` alone (reading-data.ts), so two
    // parts made in the same millisecond come back in either order; Burrow
    // breaks that tie by the thread's quote order. Compare the order only
    // when the two were made at different times.
    let created = |p: &str| row_of(&rows, &sid, p).created.clone();
    if created(&p1) == created(&p2) {
        said.sort();
        want.sort();
    }
    assert_eq!(said, want, "ReadingEntry.parts: sub, id, label");

    // The burst rule off: one key written, the others as stored (wordless
    // turned off first, so "as stored" isn't just the default).
    let _wordless = RuleBack(&me, "wordless", rules.on("wordless"));
    me.save_background_rule("wordless", false).unwrap();
    let before = stored_rules(&e.url);
    assert_eq!(before["wordless"], json!(false));
    let _burst = RuleBack(&me, BURST, true);
    let back = me.save_background_rule(BURST, false).unwrap();
    assert!(!back.on(BURST));
    let after = stored_rules(&e.url);
    for rule in ["wordless", "maintenance", "minor"] {
        assert_eq!(before[rule], after[rule], "{rule} kept its stored value");
    }
    assert_eq!(after[BURST], json!(false));
    sync_until(&me, "the parts back in the foreground", |rows| {
        !row_of(rows, &sid, &p1).is_background() && !row_of(rows, &sid, &p2).is_background()
    });
    assert!(
        !burst_parts(&annotated(&me)).contains_key(&thread_key),
        "no parts while the rule is off"
    );
    assert_eq!(
        studio_parts(&e.url, &sid, &thread),
        Some(vec![]),
        "nor does the studio's list (its ReadingEntry has no \"parts\")"
    );

    // On again: the reasons were kept, so they fold again.
    me.save_background_rule(BURST, true).unwrap();
    sync_until(&me, "the parts folded again", |rows| {
        burst_parts(rows).get(&thread_key).map(Vec::len) == Some(2)
    });
}

/// Your own burst folds in your own reader too (studio 0.64: "own tangles
/// fold the same way in the owner's own feed"): your thread lists your
/// parts, keyed `~own`.
#[test]
#[ignore = "needs a local Worker: scripts/e2e-local.sh"]
fn your_own_burst_lists_its_parts() {
    let d = tempfile::tempdir().unwrap();
    let me = manual(d.path());
    me.sync_now().unwrap();
    if offers_burst(&me).is_none() {
        return;
    }
    let t = tag("ownburst");
    let (_, p1) = published(&me, Kind::Fragment, &format!("{t} my first part"), None);
    let (_, p2) = published(&me, Kind::Fragment, &format!("{t} my second part"), None);
    let (_, thread) = published(
        &me,
        Kind::Thread,
        &format!("{t} my thread, in my words\n\n![[{p1}]]\n\n![[{p2}]]"),
        None,
    );
    let key = (SubId::from(OWN_SUBSCRIPTION), RemoteId::from(&thread));
    sync_until(&me, "my parts listed", |rows| {
        burst_parts(rows).get(&key).map(Vec::len) == Some(2)
    });
    let rows = annotated(&me);
    assert!(row_of(&rows, OWN_SUBSCRIPTION, &p1).is_background());
    assert!(!row_of(&rows, OWN_SUBSCRIPTION, &thread).is_background());
    // The studio's list keys them the same: ReadingEntry.parts' sub is "~own".
    let said = studio_parts(&e2e().url, OWN_SUBSCRIPTION, &thread).expect("listed");
    let keys: Vec<(&str, &str)> = said.iter().map(|p| (p.0.as_str(), p.1.as_str())).collect();
    assert_eq!(
        keys,
        [
            (OWN_SUBSCRIPTION, p1.as_str()),
            (OWN_SUBSCRIPTION, p2.as_str())
        ]
    );
}

/// A post of only quotes is "no words of its own" (`wordless`); a version
/// republished with the same text and quotes is "republished unchanged"
/// (`maintenance`).
#[test]
#[ignore = "needs a local Worker: scripts/e2e-local.sh"]
fn wordless_and_unchanged_republishes_go_to_background() {
    let src_url = url_b();
    let (d1, d2) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let me = manual(d1.path());
    me.sync_now().unwrap();
    if offers_burst(&me).is_none() {
        return;
    }
    let source = backend_at(d2.path(), &src_url, cred_b(), false);
    let t = tag("wordless");
    let (_, quoted) = published(&source, Kind::Fragment, &format!("{t} quoted"), None);
    let (_, again) = published(
        &source,
        Kind::Fragment,
        &format!("{t} said once, then again"),
        None,
    );
    let sub = me.subscribe(&src_url, None).unwrap();
    let _unsub = Unsub(&me, sub.id.clone());
    sync_until_held(&me, &[&quoted, &again]);
    let (_, only_quotes) = published(&source, Kind::Thread, &format!("![[{quoted}]]"), None);
    poll(&me, &sub.id, &only_quotes, 1);

    // `again`, republished unchanged on `b` (the studio's publish needs no
    // edit): v2 with the same text.
    let (st, v) = owner_as(
        &cred_b(),
        "POST",
        &src_url,
        &format!("/api/items/{again}/publish"),
        Some(json!({ "expectedRevision": src_rev(&again) })),
    );
    assert_eq!(st, 200, "{v}");
    poll(&me, &sub.id, &again, 2);

    let sid = sub.id.to_string();
    sync_until(&me, "wordless and maintenance in Background", |rows| {
        row_of(rows, &sid, &only_quotes).is_background()
            && row_of(rows, &sid, &again).is_background()
    });
    let rows = annotated(&me);
    let why = |id: &str| row_of(&rows, &sid, id).background.clone().unwrap();
    assert!(why(&only_quotes).reasons.iter().any(|r| r == "wordless"));
    assert_eq!(
        why(&only_quotes).why().as_deref(),
        Some("no words of its own")
    );
    assert_eq!(why(&again).reasons, ["maintenance"]);
    assert_eq!(why(&again).why().as_deref(), Some("republished unchanged"));
}

/// Publish one of your own fragments straight through the owner API; its id.
fn own_published(text: &str) -> String {
    let e = e2e();
    let (st, v) = owner(
        "POST",
        &e.url,
        "/api/items",
        Some(json!({ "mode": "blank", "kind": "fragment", "contentMd": text })),
    );
    assert!(st == 200 || st == 201, "{st} {v}");
    let id = v["id"].as_str().unwrap().to_string();
    let (st, v) = owner(
        "POST",
        &e.url,
        &format!("/api/items/{id}/publish"),
        Some(json!({ "expectedRevision": server_rev(&id) })),
    );
    assert_eq!(st, 200, "{v}");
    id
}

/// Your own posts a test made, taken out of the Background lens when
/// dropped, pass or fail, so later tests don't walk a lens of many pages:
/// each is deleted if the studio lets it go, else given the owner's
/// Foreground choice. Clearing the choice isn't enough: studio 0.68's
/// `superseded` holds posts published minutes apart in Background anyway.
struct OwnPostsOut {
    api: Api,
    ids: Vec<String>,
}

impl Drop for OwnPostsOut {
    fn drop(&mut self) {
        use blyg_core::background::Placement;
        for id in &self.ids {
            let deleted = self
                .api
                .get_item(id)
                .and_then(|w| self.api.delete_item(id, w.revision))
                .is_ok();
            if !deleted {
                let key = (SubId::from(OWN_SUBSCRIPTION), RemoteId::from(id));
                let _ = self
                    .api
                    .set_reading_placement(&key, Some(Placement::Foreground));
            }
        }
    }
}

/// The studio expires a reading cursor whenever the reading collection
/// moves, as it does on every post a subscription's backfill imports
/// (studio 0.68: and on every neighbour that post supersedes). A Background
/// lens that changes under every walk is left as held for that pull: the
/// sync goes on without an error, the set isn't emptied, and a later pull
/// reads it whole.
#[test]
#[ignore = "needs a local Worker: scripts/e2e-local.sh"]
fn a_lens_that_changes_under_every_walk_waits_for_the_next_pull() {
    use blyg_core::background::{LENS_LIMIT, LENS_QUERY, Placement};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    let e = e2e();
    let api = Api::new(&e.url, e.cred.clone());
    if !api.serves_background().unwrap() {
        eprintln!("skipped: this studio has no Background");
        return;
    }
    let t = tag("lens-moves");
    let own = |id: &str| (SubId::from(OWN_SUBSCRIPTION), RemoteId::from(id));
    let mut made = OwnPostsOut {
        api: Api::new(&e.url, e.cred.clone()),
        ids: Vec::new(),
    };
    // More than one page of the lens: your own posts, moved there by hand.
    let mut placed = Vec::new();
    for i in 0..=LENS_LIMIT {
        let id = own_published(&format!("{t} {i}"));
        made.ids.push(id.clone());
        api.set_reading_placement(&own(&id), Some(Placement::Background))
            .unwrap();
        placed.push(id);
    }
    let flipper = own_published(&format!("{t} flipper"));
    made.ids.push(flipper.clone());

    // While `moving`, the reading collection moves before every later page
    // of the lens (a placement flipped), so each walk's cursor expires.
    let moving = Arc::new(AtomicBool::new(false));
    let (on, base, cred, flip_id) = (
        moving.clone(),
        e.url.clone(),
        e.cred.clone(),
        flipper.clone(),
    );
    // The flipper is in Background when the hook first fires: Foreground first.
    let mut flip = true;
    let hook: Hook = Box::new(move |req: &str| {
        let lens_page = req.starts_with("GET /api/reading?")
            && req.contains(LENS_QUERY)
            && req.contains("cursor=");
        if on.load(Ordering::SeqCst) && lens_page {
            flip = !flip;
            let to = if flip {
                Placement::Background
            } else {
                Placement::Foreground
            };
            Api::new(&base, cred.clone())
                .set_reading_placement(
                    &(SubId::from(OWN_SUBSCRIPTION), RemoteId::from(&flip_id)),
                    Some(to),
                )
                .unwrap();
        }
        false
    });
    let proxy = Proxy::start_with(&e.url, Some(hook));
    let dir = tempfile::tempdir().unwrap();
    let me = backend_at(dir.path(), &proxy.url, e.cred.clone(), false);
    let held = |me: &LiveBackend| {
        me.background_set()
            .map_or(0, |s| placed.iter().filter(|id| s.of(&own(id)).on).count())
    };
    me.sync_now().unwrap();
    assert_eq!(held(&me), placed.len(), "read whole while nothing moves");

    // Moved since, and moving under every walk of the lens.
    api.set_reading_placement(&own(&flipper), Some(Placement::Background))
        .unwrap();
    proxy.take_full();
    moving.store(true, Ordering::SeqCst);
    me.sync_now()
        .expect("a lens that keeps changing doesn't fail the sync");
    let firsts = proxy
        .take_full()
        .iter()
        .filter(|r| r.starts_with("GET /api/reading?") && r.contains(LENS_QUERY))
        .filter(|r| !r.contains("cursor="))
        .count();
    assert!(firsts > 1, "the walk was restarted: {firsts} first pages");
    assert_eq!(held(&me), placed.len(), "kept as held: nothing pruned");

    // Still again: the next pull reads it whole.
    moving.store(false, Ordering::SeqCst);
    api.set_reading_placement(&own(&flipper), None).unwrap();
    me.sync_now().unwrap();
    assert_eq!(held(&me), placed.len());
    assert!(!me.background_set().unwrap().of(&own(&flipper)).on);
}
