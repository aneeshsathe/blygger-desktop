//! Studio 0.64 e2e: tangling (`![[+ …]]` spans and `[[+ …]]` placeholders,
//! studio 0.63; tangling plan §2).

use super::*;
use blyg_core::access_ai::PublishExtras;

fn tangle_yes() -> PublishExtras {
    PublishExtras {
        tangle: true,
        ..Default::default()
    }
}

/// The 409 count gate, `tangle: true`, the parts, and the rewritten working
/// copy taken as text and base, so a republish mints nothing new.
#[test]
#[ignore = "needs a local Worker: scripts/e2e-local.sh"]
fn a_tangled_thread_asks_then_publishes_its_parts_first() {
    let dir = tempfile::tempdir().unwrap();
    let e = e2e();
    let b = manual(dir.path());
    let t = tag("tangle");
    let text = format!(
        "{t} begins here.\n\n![[+ A passage for {t}, quoted.]]\n\nAnd [[+ an idea {t}]] linked.\n"
    );
    let id = b.create_draft(Kind::Thread, &text).unwrap();
    b.sync_now().unwrap();
    let sid = sid_of(&b, &id);

    // 1. Without `tangle: true` nothing is written: the count and the names.
    let before = server_items().len();
    match b.publish(&id, None) {
        Err(CoreError::TangleConfirm { count, parts }) => {
            assert_eq!(count, 2);
            assert_eq!(
                parts,
                [
                    format!("A passage for {t}, quoted."),
                    format!("an idea {t} (linked)")
                ]
            );
        }
        other => panic!("expected the tangle gate, got {:?}", other.map(|o| o.version)),
    }
    assert_eq!(server_items().len(), before, "a refused tangle mints nothing");
    assert_eq!(server_item(&sid)["status"], "draft");

    // 2. Confirmed: the parts first, then the thread.
    let out = b.publish_with(&id, None, &tangle_yes()).expect("tangle");
    assert_eq!(out.version, 1);
    assert_eq!(out.parts.len(), 2, "{:?}", out.parts);
    let rewritten = out.rewritten.clone().expect("the server's rewrite");
    let server_md = server_item(&sid)["contentMd"].as_str().unwrap().to_string();
    assert_eq!(rewritten, server_md);
    assert!(!rewritten.contains("[[+"), "{rewritten}");
    assert!(rewritten.contains(&format!("![[{}", out.parts[0])), "{rewritten}");
    assert!(rewritten.contains(&format!("[[{} | an idea {t}]]", out.parts[1])), "{rewritten}");
    let local = b.item(&id).unwrap();
    assert_eq!(local.content_md, server_md, "the working copy is the server's");
    assert!(!local.pending_sync && !local.conflict);
    let doc = public_json(&format!("{}/items/{sid}.json", e.url));
    assert_eq!(doc["transclusions"][0]["id"], out.parts[0].as_str(), "{doc}");
    let links: Vec<&str> = doc["links"]
        .as_array()
        .map(|a| a.iter().filter_map(|l| l["id"].as_str()).collect())
        .unwrap_or_default();
    assert!(links.contains(&out.parts[1].as_str()), "{doc}");
    // The parts are threads, published, and held here.
    let held: Vec<String> = b
        .items()
        .into_iter()
        .filter_map(|i| i.server_id.map(|s| s.0))
        .collect();
    for p in &out.parts {
        let w = server_item(p);
        assert_eq!(w["status"], "public", "{w}");
        assert_eq!(w["kind"], "thread", "{w}");
        assert!(held.contains(p), "part {p} is held locally");
    }
    assert!(server_item(&out.parts[1])["contentMd"]
        .as_str()
        .unwrap()
        .starts_with(&format!("# an idea {t}")));

    // 3. An edit on the text the editor now shows, then a republish: no new
    // items. (An editor that kept its own text would save the spans back.)
    let editor = out.rewritten.unwrap_or(text);
    b.save(&id, &format!("{editor}\nOne more line.\n")).unwrap();
    b.sync_now().unwrap();
    let count = server_items().len();
    let again = b.publish(&id, None).expect("republish");
    assert_eq!(again.version, 2);
    assert!(again.parts.is_empty() && again.rewritten.is_none());
    assert_eq!(server_items().len(), count, "a republish mints nothing");
}

/// Refusals: a span in a fragment, and more than ten spans. Burrow says the
/// studio's words before sending; the studio says the same when asked.
#[test]
#[ignore = "needs a local Worker: scripts/e2e-local.sh"]
fn tangle_refusals_match_the_studio() {
    let dir = tempfile::tempdir().unwrap();
    let e = e2e();
    let b = manual(dir.path());
    let t = tag("tangle-no");

    let frag = b
        .create_draft(Kind::Fragment, &format!("{t}\n\n![[+ a part]]\n"))
        .unwrap();
    let eleven: String = (0..11).map(|i| format!("[[+ idea {i} {t}]] ")).collect();
    let thread = b.create_draft(Kind::Thread, &eleven).unwrap();
    b.sync_now().unwrap();
    let before = server_items().len();
    for (id, want) in [
        (&frag, "tangling makes a thread; switch this draft to a thread"),
        (&thread, "one publish can make at most 10 new items; this has 11"),
    ] {
        match b.publish_with(id, None, &tangle_yes()) {
            Err(CoreError::Rejected { status, message, .. }) => {
                assert_eq!((status, message.as_str()), (400, want));
            }
            other => panic!("expected a refusal, got {:?}", other.map(|o| o.version)),
        }
        // The studio's own answer, asked directly, is the same.
        let sid = sid_of(&b, id);
        let (st, v) = owner(
            "POST",
            &e.url,
            &format!("/api/items/{sid}/publish"),
            Some(json!({ "expectedRevision": server_rev(&sid), "tangle": true })),
        );
        assert_eq!(st, 400, "{v}");
        assert_eq!(v["message"], want, "{v}");
    }
    assert_eq!(server_items().len(), before, "nothing was minted");
    for id in [&frag, &thread] {
        b.delete_draft(id).unwrap();
    }
}
