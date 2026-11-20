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

/// Unsubscribes when dropped, so a failed test leaves no subscription
/// behind (the next test's subscribe would get a 409).
struct Subscribed<'a>(&'a LiveBackend, SubId);

impl Drop for Subscribed<'_> {
    fn drop(&mut self) {
        let _ = self.0.unsubscribe(&self.1);
    }
}

/// Puts a rule back on when dropped.
struct RuleBackOn<'a>(&'a LiveBackend, &'static str);

impl Drop for RuleBackOn<'_> {
    fn drop(&mut self) {
        let _ = self.0.save_background_rule(self.1, true);
    }
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
    let _unsub = Subscribed(&me, sub.id.clone());
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

    // The burst rule off: one key written, the others as stored (wordless
    // turned off first, so "as stored" isn't just the default).
    let _wordless = RuleBackOn(&me, "wordless");
    me.save_background_rule("wordless", false).unwrap();
    let before = stored_rules(&e.url);
    assert_eq!(before["wordless"], json!(false));
    let _burst = RuleBackOn(&me, BURST);
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
    let _unsub = Subscribed(&me, sub.id.clone());
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
