//! --- upkeep --- The sample blyg's stale quotes, signals and activity log
//! (`blyg_core::upkeep`), for `BLYGGER_FAKE=1` and the UI tests.
//!
//! One published thread ("Notes on The Field Guide to Small Gardens") has
//! fallen behind: a quote of Lin's bench note is two versions old (one held
//! here, one only at Lin's blyg), and one quote is current. Refreshing it
//! publishes a new version with the same words. The signals are the
//! sample reading list's own thumbs and hopper memberships.

use std::collections::HashSet;

use blyg_core::upkeep::{
    Interaction, InteractionKind, Page, QuoteFreshness, QuoteStatus, Refreshed, ThreadFreshness,
    Thumb,
};
use blyg_core::{CoreError, Item, LocalId, Result, Status, Version};
use chrono::Utc;

use super::State;

/// The sample thread that has a stale quote.
pub const STALE_THREAD: &str = "01J9K7T";

/// What the fake remembers.
#[derive(Debug, Default)]
pub struct FakeUpkeep {
    /// Threads refreshed since launch (current now).
    refreshed: HashSet<LocalId>,
    /// Tests: more stale threads, by local id.
    extra: Vec<LocalId>,
}

fn sid(item: &Item) -> String {
    item.server_id
        .as_ref()
        .map(|s| s.0.clone())
        .unwrap_or_default()
}

/// One stale thread's report, as the blyg would make it.
fn report(item: &Item) -> ThreadFreshness {
    let quotes = vec![
        QuoteFreshness {
            id: "01k211n0bench0000000000001".into(),
            origin: Some("https://lin.example.org/".into()),
            baked: 1,
            held: Some(2),
            live: Some(3),
            partial: false,
            status: QuoteStatus::Behind,
            reason: None,
        },
        QuoteFreshness {
            id: "01k2shore00000000000000003".into(),
            origin: Some("https://shoreline.example.net/".into()),
            baked: 2,
            held: Some(2),
            live: Some(2),
            partial: true,
            status: QuoteStatus::Current,
            reason: None,
        },
    ];
    ThreadFreshness {
        id: sid(item),
        version: item.version,
        dirty: item.dirty,
        stale: 1,
        blocking: 0,
        behind: 2,
        quotes,
    }
}

impl FakeUpkeep {
    /// Tests: make another of the sample's posts stale.
    #[cfg(test)]
    pub fn add(&mut self, id: LocalId) {
        self.extra.push(id);
    }
}

/// The stale threads now (published threads only, refreshed ones left out).
pub(super) fn stale(st: &State) -> Vec<ThreadFreshness> {
    let u = &st.upkeep;
    let mut ids: Vec<LocalId> = vec![LocalId(STALE_THREAD.into())];
    ids.extend(u.extra.iter().cloned());
    let mut out: Vec<ThreadFreshness> = ids
        .iter()
        .filter(|id| !u.refreshed.contains(*id))
        .filter_map(|id| st.items.iter().find(|i| &i.local_id == id))
        .filter(|i| i.status == Status::Public && i.version > 0)
        .map(report)
        .collect();
    blyg_core::upkeep::sort_stalest_first(&mut out);
    out
}

/// What the list has heard (the sample's pull has always run).
pub(super) fn cached(st: &State) -> Vec<ThreadFreshness> {
    stale(st)
}

/// `GET /items/{id}/freshness`: the report, or every quote current.
pub(super) fn one(st: &State, id: &LocalId) -> Result<ThreadFreshness> {
    let item = st
        .items
        .iter()
        .find(|i| &i.local_id == id)
        .ok_or(CoreError::NotFound)?;
    if let Some(t) = stale(st).into_iter().find(|t| t.id == sid(item)) {
        return Ok(t);
    }
    Ok(ThreadFreshness {
        id: sid(item),
        version: item.version,
        dirty: item.dirty,
        quotes: Vec::new(),
        stale: 0,
        blocking: 0,
        behind: 0,
    })
}

/// `POST /items/{id}/refresh`: a new version, the words unchanged.
pub(super) fn refresh(st: &mut State, id: &LocalId, note: Option<&str>) -> Result<Refreshed> {
    let t = one(st, id)?;
    let conflict = |message: &str| CoreError::Rejected {
        status: 409,
        message: message.into(),
        details: vec![],
    };
    if t.stale == 0 {
        return Err(conflict("nothing is stale"));
    }
    if t.dirty {
        return Err(conflict(
            "the working copy has unpublished edits: publish or discard them first",
        ));
    }
    let item = st
        .items
        .iter_mut()
        .find(|i| &i.local_id == id)
        .ok_or(CoreError::NotFound)?;
    item.version += 1;
    item.updated = Utc::now().to_rfc3339();
    let version = item.version;
    st.versions.entry(id.clone()).or_default().push(Version {
        version,
        published_at: Utc::now().to_rfc3339(),
        note: note.map(str::to_string),
        pinned: false,
        endcap: false,
    });
    st.upkeep.refreshed.insert(id.clone());
    Ok(Refreshed {
        revision: version as u64 + 100,
        version,
        refreshed: t
            .quotes
            .iter()
            .filter(|q| q.status.is_stale())
            .map(|q| q.id.clone())
            .collect(),
        resynced: t
            .quotes
            .iter()
            .filter(|q| q.status == QuoteStatus::Behind)
            .count() as u32,
        warning: None,
    })
}

/// `GET /items/{id}/versions/{v}`: the sample's own history, or the
/// current words for the current version.
pub(super) fn version_text(st: &State, id: &LocalId, version: u32) -> Result<String> {
    if let Some(t) = st.rd.own_texts.get(&(id.clone(), version)) {
        return Ok(t.clone());
    }
    let item = st
        .items
        .iter()
        .find(|i| &i.local_id == id)
        .ok_or(CoreError::NotFound)?;
    let endcap = st
        .versions
        .get(id)
        .and_then(|vs| vs.iter().find(|v| v.version == version))
        .is_some_and(|v| v.endcap);
    if endcap {
        return Ok(String::new());
    }
    if version >= 1 && version <= item.version {
        // Published words: what was last synced (the editor may hold edits).
        return Ok(st
            .synced
            .get(id)
            .cloned()
            .unwrap_or_else(|| item.content_md.clone()));
    }
    Err(CoreError::NotFound)
}

fn label(md: &str) -> Option<String> {
    blyg_core::plain_title(md)
}

/// `GET /thumbs?thumb=…`: the reading list's thumbs, newest first.
pub(super) fn thumbs(st: &State, thumb: i8) -> Page<Thumb> {
    let mut items: Vec<Thumb> = st
        .rd
        .reading
        .iter()
        .filter(|r| r.thumb == Some(thumb))
        .map(|r| Thumb {
            thumb,
            at: r.observed_at.clone(),
            origin: r.origin.clone(),
            remote_id: r.remote_id.clone(),
            subscription_id: Some(r.subscription_id.clone()),
            label: label(&r.content_md),
        })
        .collect();
    items.sort_by(|a, b| b.at.cmp(&a.at));
    Page { items, next: None }
}

/// `GET /interactions`: thumbs and hopper adds, newest first.
pub(super) fn interactions(st: &State) -> Page<Interaction> {
    let mut out = Vec::new();
    let mut n = 0;
    let mut push = |out: &mut Vec<Interaction>,
                    kind: InteractionKind,
                    r: &blyg_core::ReadingItem,
                    hopper: Option<(&str, &str)>| {
        n += 1;
        out.push(Interaction {
            id: n,
            at: r.observed_at.clone(),
            kind,
            origin: r.origin.clone(),
            remote_id: r.remote_id.clone(),
            version: Some(r.version),
            own_item_id: None,
            own_version: None,
            hopper_id: hopper.map(|h| h.0.to_string()),
            hopper_name: hopper.map(|h| h.1.to_string()),
            backfilled: false,
            subscription_id: Some(r.subscription_id.clone()),
            label: label(&r.content_md),
        });
    };
    for r in &st.rd.reading {
        match r.thumb {
            Some(1) => push(&mut out, InteractionKind::ThumbUp, r, None),
            Some(-1) => push(&mut out, InteractionKind::ThumbDown, r, None),
            _ => {}
        }
    }
    for (h, s, rid) in &st.rd.hopper_items {
        let Some(r) = st
            .rd
            .reading
            .iter()
            .find(|r| &r.subscription_id == s && &r.remote_id == rid)
        else {
            continue;
        };
        let name = st
            .rd
            .hoppers
            .iter()
            .find(|x| &x.id == h)
            .map(|x| x.name.as_str())
            .unwrap_or("a hopper");
        push(
            &mut out,
            InteractionKind::HopperAdd,
            r,
            Some((h.as_str(), name)),
        );
    }
    out.sort_by(|a, b| b.at.cmp(&a.at));
    Page {
        items: out,
        next: None,
    }
}
