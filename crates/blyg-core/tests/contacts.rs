//! --- contacts --- Studio 0.56 parity against the in-process mock:
//! `GET /api/contacts` (cached locally), `unresolvedHandles` on publish,
//! and `GET /api/reading?mentions=true`, all gated on contract 5.7.

mod common;

use blyg_core::*;
use common::*;
use serde_json::json;

fn row(sub: &str, rid: &str) -> serde_json::Value {
    json!({
        "subscription_id": sub, "remote_id": rid, "subscription_title": "Lin",
        "origin": "https://lin.example.org/", "kind": "fragment", "state": "current",
        "version": 1, "created": null, "updated": null, "observed_at": "2030-01-01T00:00:00Z",
        "content_md": "hello", "content_html": "<p>hello</p>",
        "author": null, "page": null, "thumb": null, "hoppers": []
    })
}

fn env_at(contract: &str) -> Env {
    let env = Env::new();
    {
        let mut st = env.mock.state();
        st.contract_version = Some(contract.into());
        // Not in the 5.0.0 OpenAPI fixture.
        st.unchecked.push("/api/contacts".into());
        st.unchecked.push("/api/reading".into());
        st.unchecked.push("/api/items".into());
        st.contacts = Some(vec![
            json!({ "handle": "lin", "name": "Lin Okafor", "origin": "https://lin.example.org" }),
        ]);
        st.reading = Some(vec![row("S1", "a"), row("S1", "b")]);
        st.mention_keys = Some(vec![("S1".into(), "b".into())]);
    }
    env
}

#[test]
fn contacts_are_fetched_and_kept_from_5_7() {
    let env = env_at("5.7.0");
    let b = env.manual();
    assert!(b.contacts().is_empty(), "nothing until fetched");
    assert!(!b.rewrites_handles());
    b.refresh_contacts().unwrap();
    let c = b.contacts();
    assert_eq!(c.len(), 1);
    assert_eq!(c[0].handle, "lin");
    assert!(b.rewrites_handles());
    drop(b);
    // Kept on this Mac: the next launch has them before any fetch.
    let b = env.manual();
    assert_eq!(b.contacts().len(), 1);
    assert_eq!(b.reading_mentions(), Some(vec![("S1".into(), "b".into())]));
}

#[test]
fn an_older_contract_is_never_asked() {
    let env = env_at("5.6.0");
    let b = env.manual();
    b.refresh_contacts().unwrap();
    assert!(b.contacts().is_empty());
    assert!(!b.rewrites_handles());
    assert_eq!(b.reading_mentions(), None);
    let log = env.mock.state().log.clone();
    assert!(
        !log.iter()
            .any(|l| l.contains("/api/contacts") || l.contains("mentions=true")),
        "{log:?}"
    );
}

#[test]
fn unresolved_handles_come_back_as_a_publish_warning() {
    let env = env_at("5.7.0");
    env.mock.state().unresolved_handles = vec!["sam".into()];
    let b = env.manual();
    let id = b
        .create_draft(Kind::Fragment, "Tea with @lin and @sam")
        .unwrap();
    let out = b.publish(&id, None).unwrap();
    assert_eq!(
        out.warning.as_deref(),
        Some("not in your contacts, left as written: @sam")
    );
}
