//! §10.2 (clause C-10.2-04, blygger-spec reader case `closure-contains-self`):
//! a thread may not quote a local thread whose local closure contains the
//! thread being published. The studio refuses that publish, so the preview
//! counts it as an unresolved quote at the top, not only as a circular
//! marker one level down.

use super::*;
use blyg_core::{LocalId, ServerId};

const ME: &str = "0123456789abcdefghjkmnpqrs";
const B: &str = "0123456789abcdefghjkmnpqrt";
const OTHER: &str = "0123456789abcdefghjkmnpqrv";

fn thread(id: &str, md: &str) -> Item {
    Item {
        local_id: LocalId(format!("local-{id}")),
        server_id: Some(ServerId(id.into())),
        kind: Kind::Thread,
        status: Status::Public,
        version: 1,
        dirty: false,
        content_md: md.into(),
        created: String::new(),
        updated: String::new(),
        permalink: None,
        stub_of: None,
        forked_from: None,
        show_responses: true,
        responses_mode: None,
        pending_sync: false,
        conflict: false,
        highlight: false,
        highlight_mode: None,
    }
}

fn publish(r: &StoreResolver, self_id: &str, md: &str) -> blyg_render::Rendered {
    let opts = RenderOpts {
        self_id: Some(self_id.into()),
        ..RenderOpts::default()
    };
    render_preview(md, blyg_render::Kind::Thread, r, &opts)
}

fn store() -> Vec<Item> {
    vec![
        thread(ME, &format!("Mine quotes B\n\n![[{B}]]")),
        thread(B, &format!("B quotes mine\n\n![[{ME}]]")),
    ]
}

#[test]
fn a_quote_whose_closure_contains_the_publishing_thread_is_refused() {
    let r = StoreResolver::new(store(), vec![], &[], "/blyg").publishing(Some(ME));
    let out = publish(&r, ME, &format!("Mine quotes B\n\n![[{B}]]"));
    assert_eq!(out.stats.unresolved.len(), 1, "{}", out.html);
    assert_eq!(out.stats.unresolved[0].reason, UnresolvedReason::Circular);
}

#[test]
fn quoting_itself_is_still_the_self_quote_refusal() {
    let r = StoreResolver::new(store(), vec![], &[], "/blyg").publishing(Some(ME));
    let out = publish(&r, ME, &format!("![[{ME}]]"));
    assert_eq!(out.stats.unresolved.len(), 1, "{}", out.html);
    assert_eq!(out.stats.unresolved[0].reason, UnresolvedReason::SelfQuote);
}

#[test]
fn a_cycle_the_publishing_thread_is_not_in_publishes() {
    let r = StoreResolver::new(store(), vec![], &[], "/blyg").publishing(Some(OTHER));
    let out = publish(&r, OTHER, &format!("Another thread quotes B\n\n![[{B}]]"));
    assert!(out.stats.unresolved.is_empty(), "{:?}", out.stats.unresolved);
    assert_eq!(out.stats.quotes, 1);
}
