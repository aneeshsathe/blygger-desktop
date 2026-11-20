//! Your own published posts as reading rows, for every client's Reading
//! list (the app's `reading/own.rs`). A row is
//! keyed `("~own", server id)` ([`OWN_SUBSCRIPTION`], blygger-studio 0.45's
//! own-items marker). It is never a subscription: read at its version (so
//! never unread), never thumbed, and nothing about it is sent as read state
//! or a signal. It takes its place in the list by [`crate::order`].

use std::cmp::Ordering;

use crate::model::{Item, OWN_SUBSCRIPTION, ReadingItem, RemoteId, Status, StubOf};
use crate::order;

/// One of your own rows (never sent anywhere as a reading row).
pub fn is_own_row(r: &ReadingItem) -> bool {
    r.subscription_id == OWN_SUBSCRIPTION
}

/// An origin as a citation spells it: absolute, with a trailing slash
/// (blygger-studio's `normalizeOrigin`, which a reference is compared in).
pub fn citation_origin(origin: &str) -> String {
    // The one spelling of §12.2 (`crate::origin`); a string that isn't a
    // URL keeps the old shape.
    crate::origin::canonical(origin).unwrap_or_else(|| format!("{}/", origin.trim_end_matches('/')))
}

/// Your published post as a reading row; `None` for a draft, a scratch
/// note, a withdrawn post, or one the server hasn't numbered yet. Read at
/// its version (so never unread), dated by when it was written (`updated`
/// only while the working copy is the published text).
pub fn row(item: &Item, origin: &str) -> Option<ReadingItem> {
    if item.status != Status::Public || item.version == 0 {
        return None;
    }
    let sid = item.server_id.as_ref()?.0.clone();
    Some(ReadingItem {
        subscription_id: OWN_SUBSCRIPTION.into(),
        remote_id: RemoteId::from(sid),
        subscription_title: "You".into(),
        origin: citation_origin(origin),
        kind: item.kind,
        state: "current".into(),
        version: item.version,
        created: Some(item.created.clone()),
        updated: (!item.dirty).then(|| item.updated.clone()),
        observed_at: item.created.clone(),
        content_md: item.content_md.clone(),
        content_html: String::new(),
        author: None,
        page: item.permalink.clone(),
        thumb: None,
        hoppers: vec![],
        pinned_version_retained: None,
        read_version: Some(item.version),
        stub_of: item.stub_of.as_ref().map(|s| StubOf {
            origin: Some(s.origin.clone()),
            id: Some(s.id.clone()),
            version: Some(s.version),
            url: None,
            cited: s.cited.clone(),
        }),
        forked_from: item.forked_from.clone(),
        transclusions: vec![],
        lineage_known: true,
        source_url: item.permalink.clone(),
        generated: None,
        background: None,
    })
}

/// --- background --- Your withdrawn post as a row for the Background
/// lens: a tombstone without its text (as blygger-studio shows an endcap),
/// read at its version, dated by the withdrawal. `None` for anything not
/// withdrawn.
pub fn withdrawn_row(item: &Item, origin: &str) -> Option<ReadingItem> {
    if item.status != Status::Withdrawn {
        return None;
    }
    let public = Item {
        status: Status::Public,
        dirty: false,
        ..item.clone()
    };
    let mut r = row(&public, origin)?;
    r.state = "tombstone".into();
    r.content_md.clear();
    r.stub_of = None;
    r.forked_from = None;
    Some(r)
}

/// Every published post of yours as a row, in `items`' order.
pub fn rows(items: &[Item], origin: &str) -> Vec<ReadingItem> {
    items.iter().filter_map(|i| row(i, origin)).collect()
}

/// The reading rows with yours among them in list order
/// ([`order::reading_cmp`]). `theirs` is already in that order (as a store
/// or [`order::sort_reading`] leaves it) and keeps it; each of yours goes
/// before the first row it sorts ahead of.
pub fn merge(theirs: Vec<ReadingItem>, mut own: Vec<ReadingItem>) -> Vec<ReadingItem> {
    if own.is_empty() {
        return theirs;
    }
    order::sort_reading(&mut own);
    let mut out = Vec::with_capacity(theirs.len() + own.len());
    let mut own = own
        .into_iter()
        .map(|r| (r.sort_at(), r))
        .collect::<Vec<_>>()
        .into_iter()
        .peekable();
    for r in theirs {
        let t = r.sort_at();
        while own
            .peek()
            .is_some_and(|(o_at, o)| order::reading_cmp_at(o_at, o, &t, &r) == Ordering::Less)
        {
            out.extend(own.next().map(|(_, o)| o));
        }
        out.push(r);
    }
    out.extend(own.map(|(_, o)| o));
    out
}
