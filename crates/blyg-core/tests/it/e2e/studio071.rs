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
    // no origin, so nothing to notify. (`generated` lists only the
    // generated scopes, the page's first.)
    b.publish(&id, None).unwrap();
    let doc = public_json(&format!("{}/items/{sid}.json", e.url));
    assert_eq!(doc["generated"].as_array().map(Vec::len), Some(2), "{doc}");
    let gen_sources = &doc["generated"][0]["sources"];
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
            // What the blyg recorded is kept here too: the writer lists what
            // the scope read, and an edit that moves the scopes carries it.
            let (keyed, scopes) = b.tracked_tk_provenance(&id).expect("its provenance");
            assert_eq!(keyed, body);
            assert!(scopes.first().is_some_and(Option::is_some), "{scopes:?}");
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

// ------------------------------------------------------ the stand-in model

/// The port of the stand-in model: a studio built with a `models.local.json`
/// naming an `openai-chat` provider at `http://127.0.0.1:47871/v1` and the
/// model `e2e-model` generates through it. Any other studio can't reach it,
/// and the tests that need it skip, saying so.
const MODEL_PORT: u16 = 47871;
const MODEL: &str = "e2e-model";

/// What the stand-in model writes.
const GENERATED: &str = "A line the stand-in model wrote.";

/// Answer one Chat Completions request on `l` with [`GENERATED`], running
/// `during` once the request has arrived and before the answer goes back
/// (the seconds a real model takes). False when what connected wasn't a
/// request (the test unblocking an accept nothing else reached).
fn answer_once(l: &std::net::TcpListener, during: impl FnOnce()) -> bool {
    use std::io::{Read, Write};
    let Ok((mut s, _)) = l.accept() else {
        return false;
    };
    let mut got = Vec::new();
    let mut buf = [0u8; 8192];
    let head_end = loop {
        if let Some(at) = got.windows(4).position(|w| w == b"\r\n\r\n") {
            break at + 4;
        }
        match s.read(&mut buf) {
            Ok(0) | Err(_) => return false,
            Ok(n) => got.extend_from_slice(&buf[..n]),
        }
    };
    let head = String::from_utf8_lossy(&got[..head_end]).to_ascii_lowercase();
    let len = head
        .lines()
        .find_map(|l| l.strip_prefix("content-length:"))
        .and_then(|v| v.trim().parse::<usize>().ok())
        .unwrap_or(0);
    while got.len() < head_end + len {
        match s.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => got.extend_from_slice(&buf[..n]),
        }
    }
    during();
    let body = json!({
        "id": "chatcmpl-e2e", "object": "chat.completion", "model": MODEL,
        "choices": [{ "index": 0, "finish_reason": "stop",
                      "message": { "role": "assistant", "content": GENERATED } }]
    })
    .to_string();
    let _ = write!(
        s,
        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
    );
    true
}

/// `generate_on_blyg` with the stand-in model answering, `during` run while
/// the blyg waits for it. `None` (and why, on stderr) when this studio
/// can't reach the stand-in.
fn generate_with_model(
    b: &LiveBackend,
    id: &LocalId,
    during: impl FnOnce() + Send,
) -> Option<blyg_core::generate::Generated> {
    let Ok(l) = std::net::TcpListener::bind(("127.0.0.1", MODEL_PORT)) else {
        eprintln!("skipped: port {MODEL_PORT} for the stand-in model is taken");
        return None;
    };
    std::thread::scope(|s| {
        let model = s.spawn(|| answer_once(&l, during));
        let out = b.generate_on_blyg(id, 0, true);
        if !model.is_finished() {
            // Nothing reached the model: let the accept return.
            let _ = std::net::TcpStream::connect(("127.0.0.1", MODEL_PORT));
        }
        let asked = model.join().unwrap();
        match out {
            Ok(GenerateOutcome::Generated(g)) if asked => Some(g),
            Ok(other) => panic!("expected text from the stand-in: {other:?}"),
            Err(e) => {
                eprintln!(
                    "skipped: this studio doesn't generate with the stand-in model \
                     (a models.local.json naming {MODEL} at 127.0.0.1:{MODEL_PORT}): {e}"
                );
                None
            }
        }
    })
}

/// Make the stand-in the blyg's TK model; the old one is back when the
/// guard drops.
fn use_stand_in_model() -> SettingsBack {
    let e = e2e();
    let (st, v) = owner("GET", &e.url, "/api/settings", None);
    assert_eq!(st, 200, "{v}");
    let back = SettingsBack(e.url.clone(), json!({ "aiModelTk": v["aiModelTk"] }));
    let (st, v) = owner(
        "PATCH",
        &e.url,
        "/api/settings",
        Some(json!({ "aiModelTk": MODEL })),
    );
    assert_eq!(st, 200, "{v}");
    back
}

/// A keystroke while the blyg generates (the editor stays live for the
/// seconds a model takes) leaves a local edit that never saw the generated
/// text. Its push must not land over that text: it is refused (409) and
/// read again with conflict detection, so both texts are kept. Before the
/// fix, `generate_on_blyg` stored the answer's revision under the edit, the
/// push went through at it, and the blyg lost the generated words while
/// their provenance stayed.
#[test]
#[ignore = "needs a local Worker: scripts/e2e-local.sh"]
fn an_edit_typed_while_the_blyg_generates_never_overwrites_it() {
    if !takes_web_sources() {
        eprintln!("skipped: this studio predates contract 6.3.0");
        return;
    }
    let _model = use_stand_in_model();
    let dir = tempfile::tempdir().unwrap();
    let b = manual(dir.path());
    let t = tag("generate-race");
    let text = format!("{t}\n\n[TK]write one line[/TK]");
    let typed = format!("{t} typed meanwhile\n\n[TK]write one line[/TK]");
    let id = b.create_draft(Kind::Fragment, &text).unwrap();
    b.sync_now().unwrap();
    let sid = sid_of(&b, &id);
    let Some(g) = generate_with_model(&b, &id, || b.save(&id, &typed).unwrap()) else {
        return;
    };
    assert!(
        g.content_md
            .as_deref()
            .is_some_and(|c| c.contains(GENERATED)),
        "{g:?}"
    );
    assert!(b.item(&id).unwrap().pending_sync, "the typed edit waits");
    let _ = b.sync_now();
    let server = server_item(&sid)["contentMd"].as_str().unwrap().to_string();
    assert!(
        server.contains(GENERATED),
        "the blyg kept what it generated: {server}"
    );
    let it = b.item(&id).unwrap();
    assert!(it.conflict, "both texts are kept, for the writer to choose");
    assert_eq!(it.content_md, typed, "the typed text is kept here");

    // Without a keystroke, the item takes the blyg's text and revision:
    // the next edit goes straight through.
    let id2 = b.create_draft(Kind::Fragment, &text).unwrap();
    b.sync_now().unwrap();
    let sid2 = sid_of(&b, &id2);
    let Some(g) = generate_with_model(&b, &id2, || {}) else {
        return;
    };
    let body = g.content_md.expect("the spliced body");
    assert_eq!(b.item(&id2).unwrap().content_md, body, "held here");
    let edited = format!("{body}\n\nA tail by hand.");
    b.save(&id2, &edited).unwrap();
    b.sync_now().unwrap();
    assert_eq!(server_item(&sid2)["contentMd"], edited.as_str());
    assert!(!b.item(&id2).unwrap().conflict);
}

// ------------------------------------------------------ provenance pushes

/// A refused citation (here a post at version 0, which the contract's
/// `exclusiveMinimum` refuses) is dropped, and the web pages the scope
/// read stay: they cite nothing that must resolve. Before the fix the
/// retry dropped the pages too.
#[test]
#[ignore = "needs a local Worker: scripts/e2e-local.sh"]
fn a_refused_citation_keeps_the_web_pages() {
    if !takes_web_sources() {
        eprintln!("skipped: this studio predates web sources (contract 6.3.0)");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let b = manual(dir.path());
    let t = tag("refused-citation");
    let (_, src) = published(&b, Kind::Fragment, &format!("{t} a source"), None);
    let url = "https://news.example.com/a-kept-page";
    let text = format!("{t}\n\n[TK]sum up ![[{src}]] and {url}[=]A summary.[/TK]");
    let id = b.create_draft(Kind::Fragment, &t).unwrap();
    b.sync_now().unwrap();
    let sid = sid_of(&b, &id);
    let scope: ScopeProvenance = serde_json::from_value(json!({
        "model": "web-model",
        "sources": [{ "id": src, "version": 0 }, page(url)]
    }))
    .unwrap();
    b.save_with_provenance(&id, &text, &[Some(scope)]).unwrap();
    b.sync_now().unwrap();
    assert_eq!(server_item(&sid)["contentMd"], text.as_str());
    let p = server_provenance(&sid);
    assert_eq!(p[0]["model"], "web-model", "{p}");
    assert_eq!(p[0]["sources"], json!([page(url)]), "the page stays: {p}");
}

/// Unknown fields on a scope and on its sources, as the blyg holds them,
/// survive an edit that moves the scope: Burrow writes them back as it
/// read them. Before the fix they were dropped on the way back.
#[test]
#[ignore = "needs a local Worker: scripts/e2e-local.sh"]
fn unknown_provenance_fields_survive_an_edit() {
    if !takes_web_sources() {
        eprintln!("skipped: this studio predates web sources (contract 6.3.0)");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let e = e2e();
    let b = manual(dir.path());
    let t = tag("unknown-fields");
    let (_, src) = published(&b, Kind::Fragment, &format!("{t} a source"), None);
    let url = "https://news.example.com/unknown-fields";
    let text = format!("{t}\n\n[TK]reply to ![[{src}]] and {url}[=]A reply.[/TK]");
    let id = b.create_draft(Kind::Fragment, &text).unwrap();
    b.sync_now().unwrap();
    let sid = sid_of(&b, &id);
    let recorded = json!([{
        "model": "m", "at": "2030-01-01T00:00:00Z",
        "temperature": 0.2, "via": { "tool": "an-invented-tool" },
        "sources": [{ "id": src, "version": 1, "weight": 0.5 }, page(url)]
    }]);
    let (s, v) = owner(
        "PATCH",
        &e.url,
        &format!("/api/items/{sid}"),
        Some(json!({ "provenance": recorded, "expectedRevision": server_rev(&sid) })),
    );
    assert_eq!(s, 200, "{v}");
    let held = server_provenance(&sid)[0].clone();
    assert_eq!(
        held["temperature"], 0.2,
        "the studio keeps a scope's own fields: {held}"
    );
    b.sync_now().unwrap();
    b.save(&id, &format!("[TK]opening[=]By hand.[/TK]\n\n{text}"))
        .unwrap();
    b.sync_now().unwrap();
    let p = server_provenance(&sid);
    assert_eq!(p.as_array().unwrap().len(), 2, "{p}");
    assert!(p[0].is_null(), "{p}");
    assert_eq!(p[1], held, "as the blyg held it");
}
