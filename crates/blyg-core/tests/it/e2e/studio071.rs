//! Studio 0.70/0.71 e2e (contract 6.3.0): a generated scope's web-page
//! sources (`{url, cited}`, decision #117) through an edit and through
//! someone else's reading list; the generate checkpoint and `proceed`; and
//! `readerMarkRead` written alone. Each test skips, saying so, on a studio
//! without the feature.

use super::*;
use blyg_core::generate::GenerateOutcome;
use blyg_core::mark_read::MarkRead;

/// A web page a scope's model read, as studio 0.70 records it.
fn page(url: &str) -> Value {
    json!({
        "url": url,
        "cited": { "source": "news.example.com", "url": url, "retrieved": "2030-01-01T00:00:00Z" }
    })
}

/// A studio of 0.70 or later (contract 6.3.0), which takes `{url, cited}`
/// sources: its settings' `readerMarkRead` (required there) says so, as
/// nothing in the capabilities does.
fn takes_web_sources() -> bool {
    let (st, v) = owner("GET", &e2e().url, "/api/settings", None);
    assert_eq!(st, 200, "{v}");
    v.get("readerMarkRead").is_some()
}

/// Provenance recorded on the studio (as its own generate does) with a web
/// page in one scope and a post of yours in the other: Burrow syncs the
/// item, and a hand-written scope typed in front moves both, the page
/// unchanged. Before 0.28.0's fix the page's scope read as nothing and was
/// pushed back as `null`, so the blyg lost that disclosure.
#[test]
#[ignore = "needs a local Worker: scripts/e2e-local.sh"]
fn a_web_source_survives_an_edit() {
    if !takes_web_sources() {
        eprintln!("skipped: this studio predates web sources (contract 6.3.0)");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let e = e2e();
    let b = manual(dir.path());
    let t = tag("web-source");
    let (_, src) = published(&b, Kind::Fragment, &format!("{t} a source"), None);
    let url = "https://news.example.com/an-invented-story";
    let text = format!(
        "{t}\n\n[TK]sum up {url}[=]A summary.[/TK]\n\n[TK]reply to ![[{src}]][=]A reply.[/TK]"
    );
    let id = b.create_draft(Kind::Fragment, &text).unwrap();
    b.sync_now().unwrap();
    let sid = sid_of(&b, &id);
    let recorded = json!([
        { "model": "web-model", "at": "2030-01-01T00:00:00Z", "sources": [page(url)] },
        { "model": "post-model", "sources": [{ "id": src, "version": 1 }] }
    ]);
    let (s, v) = owner(
        "PATCH",
        &e.url,
        &format!("/api/items/{sid}"),
        Some(json!({ "provenance": recorded, "expectedRevision": server_rev(&sid) })),
    );
    assert_eq!(s, 200, "{v}");
    b.sync_now().unwrap();
    b.save(&id, &format!("[TK]opening[=]By hand.[/TK]\n\n{text}"))
        .unwrap();
    b.sync_now().unwrap();
    let p = server_provenance(&sid);
    assert_eq!(p.as_array().unwrap().len(), 3, "{p}");
    assert!(p[0].is_null(), "{p}");
    assert_eq!(p[1]["model"], "web-model", "{p}");
    assert_eq!(p[1]["sources"], json!([page(url)]), "the page, as recorded");
    assert_eq!(p[2]["model"], "post-model", "{p}");
    assert_eq!(p[2]["sources"][0]["id"], src.as_str());

    // Published, the document discloses the page as `{url, cited}`: no id,
    // no origin, so nothing to notify.
    b.publish(&id, None).unwrap();
    let doc = public_json(&format!("{}/items/{sid}.json", e.url));
    let gen_sources = &doc["generated"][1]["sources"];
    assert_eq!(gen_sources[0]["url"], url, "{doc}");
    assert!(gen_sources[0].get("id").is_none(), "{doc}");
    assert!(gen_sources[0].get("origin").is_none(), "{doc}");
}

/// A post on `b` whose generated scope read a web page that is a post of
/// `a`'s: `a` reads it in its reading list with the disclosure, and isn't
/// told (a web source sends no mention, decision #117).
#[test]
#[ignore = "needs a local Worker: scripts/e2e-local.sh"]
fn a_followed_post_with_a_web_source_syncs_and_tells_no_one() {
    if !takes_web_sources() {
        eprintln!("skipped: this studio predates web sources (contract 6.3.0)");
        return;
    }
    let e = e2e();
    let src_url = url_b();
    let (d1, d2) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let me = manual(d1.path());
    let source = backend_at(d2.path(), &src_url, cred_b(), false);
    let t = tag("web-source-read");
    let (_, mine) = published(&me, Kind::Fragment, &format!("{t} mine"), None);
    let my_page = format!("{}/f/{mine}/", e.url);
    let text = format!("{t}\n\n[TK]sum up {my_page}[=]Their summary.[/TK]");
    let id = source.create_draft(Kind::Fragment, &text).unwrap();
    source.sync_now().unwrap();
    let rid = sid_of(&source, &id);
    let rev = owner_as(
        &cred_b(),
        "GET",
        &src_url,
        &format!("/api/items/{rid}"),
        None,
    )
    .1["revision"]
        .as_u64()
        .unwrap();
    let (s, v) = owner_as(
        &cred_b(),
        "PATCH",
        &src_url,
        &format!("/api/items/{rid}"),
        Some(json!({
            "provenance": [{ "model": "m", "at": "2030-01-01T00:00:00Z", "sources": [page(&my_page)] }],
            "expectedRevision": rev,
        })),
    );
    assert_eq!(s, 200, "{v}");
    source.sync_now().unwrap();
    source.publish(&id, None).unwrap();

    let sub = me.subscribe(&src_url, Some("Web source")).unwrap();
    let _unsub = Unsub(&me, sub.id.clone());
    sync_until_held(&me, &[&rid]);
    let r = reading_item(&me, &rid).expect("the post syncs");
    let g = r.generation();
    assert_eq!(g.len(), 1, "{:?}", r.generated);
    assert_eq!(g[0].pages.len(), 1);
    assert_eq!(g[0].pages[0].url, my_page);
    assert_eq!(g[0].notifies().count(), 0);
    assert!(
        r.references().is_empty(),
        "no lineage: {:?}",
        r.references()
    );
    // No mention reached `a` from it.
    let (st, m) = owner(
        "GET",
        &e.url,
        "/api/mentions?direction=inbound&limit=50",
        None,
    );
    assert_eq!(st, 200, "{m}");
    let told = m["items"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|x| x.to_string().contains(&rid));
    assert!(!told, "a web source sent a mention: {m}");
}

/// A link that can't be read stops generation at a checkpoint that saves
/// nothing; `proceed` goes past it. No model is needed to reach the
/// checkpoint (the links are read first), so this runs on a local Worker.
#[test]
#[ignore = "needs a local Worker: scripts/e2e-local.sh"]
fn generate_stops_at_a_checkpoint_until_told_to_proceed() {
    if !takes_web_sources() {
        eprintln!("skipped: this studio predates the checkpoint (contract 6.3.0)");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let b = manual(dir.path());
    let t = tag("checkpoint");
    let missing = "01aaaaaaaaaaaaaaaaaaaaaaaa";
    let text = format!("{t}\n\n[TK]sum up [[{missing}]][/TK]");
    let id = b.create_draft(Kind::Fragment, &text).unwrap();
    b.sync_now().unwrap();
    let sid = sid_of(&b, &id);
    let rev = server_rev(&sid);
    match b.generate_on_blyg(&id, 0, false).expect("an answer") {
        GenerateOutcome::Checkpoint(c) => {
            assert_eq!((c.read, c.total), (0, 1), "{c:?}");
            assert_eq!(c.failed.len(), 1);
            assert!(c.failed[0].link.contains(missing), "{c:?}");
            assert!(!c.failed[0].reason.is_empty());
        }
        other => panic!("expected a checkpoint: {other:?}"),
    }
    assert_eq!(server_item(&sid)["contentMd"], text, "nothing saved");
    assert_eq!(server_rev(&sid), rev, "nothing saved");
    match b.generate_on_blyg(&id, 0, true) {
        Ok(GenerateOutcome::Checkpoint(c)) => panic!("proceed was ignored: {c:?}"),
        Ok(GenerateOutcome::Generated(g)) => {
            let body = g.content_md.expect("the spliced body");
            assert!(body.contains(&g.text));
            assert_eq!(b.item(&id).unwrap().content_md, body, "held here too");
        }
        // The local Worker has no Workers AI binding (nothing billed).
        Err(err) => eprintln!("past the checkpoint; no AI on this local Worker ({err})"),
    }
}

/// `readerMarkRead`: read from the blyg (default "seen"), written alone.
#[test]
#[ignore = "needs a local Worker: scripts/e2e-local.sh"]
fn reader_mark_read_round_trips_alone() {
    let e = e2e();
    let dir = tempfile::tempdir().unwrap();
    let b = manual(dir.path());
    let before = owner("GET", &e.url, "/api/settings", None).1;
    let Some(got) = b.load_reader_mark_read().unwrap() else {
        assert!(before.get("readerMarkRead").is_none());
        eprintln!("skipped: this studio predates readerMarkRead (0.70)");
        return;
    };
    assert_eq!(
        MarkRead::parse(before["readerMarkRead"].as_str().unwrap()),
        Some(got)
    );
    assert_eq!(b.reader_mark_read(), Some(got));
    for want in [MarkRead::Opened, MarkRead::Never, MarkRead::Seen] {
        assert_eq!(b.save_reader_mark_read(want).unwrap(), want);
        let now = owner("GET", &e.url, "/api/settings", None).1;
        assert_eq!(now["readerMarkRead"], want.as_str());
        assert_eq!(
            now["readerOrder"], before["readerOrder"],
            "nothing else moved"
        );
        assert_eq!(now["siteTitle"], before["siteTitle"]);
        assert_eq!(b.reader_mark_read(), Some(want));
    }
    b.save_reader_mark_read(got).unwrap();
}
