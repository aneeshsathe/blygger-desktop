//! Studio 0.64 e2e: link notices. A post on `b` that links to `a`'s post
//! with `[[id]]` sends `a` a Webmention that `a` files as a `link` notice:
//! counted in its group's `links`, never in `visible`, never a response.

use super::*;

/// `b` follows `a` and holds `a`'s post, then publishes a fragment that
/// links to it. `a` gets a verified `link` mention; its group counts it in
/// `links` and not in `visible`; `b`'s outbound row names no target
/// version (a link names a page). Hiding the notice works and changes no
/// count of responses.
#[test]
#[ignore = "needs a local Worker: scripts/e2e-local.sh"]
fn a_link_from_another_blyg_is_a_notice_not_a_response() {
    let e = e2e();
    let (d1, d2) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let a = manual(d1.path());
    let b = backend_at(d2.path(), &url_b(), cred_b(), false);
    let (_, target) = published(&a, Kind::Fragment, &tag("linked"), None);

    let sub = b.subscribe(&e.url, Some("Links to")).unwrap();
    let _unsub = Unsub(&b, sub.id.clone());
    sync_until_held(&b, &[&target]);
    let (linker, _) = published(
        &b,
        Kind::Fragment,
        &format!("{} See [[{target}]].", tag("linker")),
        None,
    );
    let linker_sid = sid_of(&b, &linker);

    // The outbound row: a link names a page, not a version.
    let mut sent = Vec::new();
    wait_until("the outbound link", Duration::from_secs(15), || {
        sent = b
            .outbound_mentions()
            .expect("outbound")
            .expect("served")
            .into_iter()
            .filter(|o| o.item_id == linker_sid)
            .collect();
        !sent.is_empty()
    });
    assert!(sent[0].target.contains(&target), "{sent:?}");
    assert_eq!(sent[0].target_version, None, "{sent:?}");

    // `a` verifies it as a link notice.
    let mut notice = None;
    wait_every(
        "the verified link notice",
        Duration::from_secs(30),
        Duration::from_millis(500),
        || {
            notice = a
                .mentions()
                .expect("mentions")
                .into_iter()
                .find(|m| m.target_item_id.eq_ignore_ascii_case(&target));
            notice.as_ref().is_some_and(|m| m.status == "verified")
        },
    );
    let m = notice.unwrap();
    assert_eq!(m.relation.as_deref(), Some("link"), "{m:?}");
    assert!(blyg_core::outbound::is_link_notice(m.relation.as_deref()));

    // The raw wire, as the contract has it.
    let (st, v) = owner(
        "GET",
        &e.url,
        &format!("/api/mention-groups?targetItemId={target}"),
        None,
    );
    assert_eq!(st, 200, "{v}");
    let raw = &v["items"][0];
    assert_eq!(
        (raw["links"].as_u64(), raw["visible"].as_u64()),
        (Some(1), Some(0)),
        "{v}"
    );
    let group = |a: &LiveBackend| {
        a.mention_groups()
            .expect("groups")
            .expect("served")
            .into_iter()
            .find(|g| g.target_item_id.eq_ignore_ascii_case(&target))
            .expect("the post's group")
    };
    let g = group(&a);
    assert_eq!(
        (g.total, g.links, g.visible, g.responses()),
        (1, 1, 0, 0),
        "{g:?}"
    );

    // Hide works on a notice, and it stays a notice.
    a.set_mention_hidden(&m.id, true).expect("hide");
    assert!(
        a.mentions()
            .unwrap()
            .iter()
            .any(|x| x.id == m.id && x.hidden)
    );
    let g = group(&a);
    assert_eq!((g.links, g.visible), (1, 0), "{g:?}");
    a.set_mention_hidden(&m.id, false).expect("unhide");
}
