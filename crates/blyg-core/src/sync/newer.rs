//! --- newer contract --- A blyg whose studio speaks a contract major newer
//! than this Burrow knows (`api::Contract::Newer`; a version that isn't a
//! number counts too). Studio 0.67 moved to contract 6, and the Burrow of
//! the day refused to sync at all until a hotfix: every new major was an
//! outage. Now:
//!
//! - Sync goes on, each feature gated on the operations the blyg's
//!   capabilities list, as on any studio (skill `capability-gating`). Your
//!   writes (publish, edits, read marks, hoppers) go through. The shelf is
//!   the exception: it's written whole, so a union sent back would bring
//!   back entries another device took off; it's read and shown here, and a
//!   change made here waits, queued, for a Burrow that knows the contract
//!   (`Engine::show_shelf_union`, held in `Engine::next_op`).
//! - Nothing held here is removed on the word of that studio's lists, until
//!   a Burrow that knows the contract: a page in a shape Burrow doesn't know
//!   would read as an empty or ended list. The store holds the one switch
//!   (`Store::lists_prune`). It starts not checked (nothing removed, and
//!   nothing called newer either: `Store::newer_contract`),
//!   `Engine::check_server` sets it, and it's checked again on each
//!   safety-net pull and before a removal that may come long after the
//!   session's first check (a walk that spans pulls, a subscription list
//!   that drops one: [`Engine::recheck_contract`]; a list laid on a check
//!   from an earlier pull: [`Engine::lay_list`]), so a studio upgraded
//!   mid-session is seen first. A store function that deletes list rows
//!   names the decision, is private, or is on a reviewed list
//!   (`tests/it/list_prune_scan.rs`). What a newer contract's lists leave:
//!   - your items (`Store::merge_all`);
//!   - reading rows after a whole walk (`walk::Read`'s `Step::Done`, and
//!     `Store::prune_reading` beneath it);
//!   - an RSS post's old row, re-keyed under its new guid
//!     (`merge_reading_page`): both rows stay, the old one's thumb, shelf
//!     and hopper places with it;
//!   - thumbs a signals list lacks (`Store::replace_thumbs`);
//!   - hoppers and memberships (`Store::replace_hoppers`);
//!   - read marks another device seems to have cleared
//!     (`Store::reconcile_read_state`);
//!   - subscriptions (`Store::take_served_subscriptions`, the one way a
//!     served list reaches the table; [`Engine::hold_subscriptions`] checks
//!     the contract again first), and contacts (by id, else handle:
//!     [`same_contact`]), kept here by [`keep_held`];
//!   - the shelf, shown as the union of the blyg's and this side's, less
//!     what this side took off since the blyg last agreed.
//! - Your own removals apply here at once, whatever the contract: an
//!   unsubscribe takes its rows (`Store::drop_subscription_rows`), a
//!   deleted contact, hopper or draft goes from the store.
//! - Left as the blyg says, on purpose: what the studio works out and
//!   Burrow only caches, which a later read puts right and which holds
//!   nothing of yours. The Background set (`fetch_background`), stale
//!   quotes (`upkeep.freshness`), the mentions lens cache and each row's
//!   syndication list (`set_reading_syndication`, `merge_wire_syndication`).
//! - The app is told once a session (`CoreEvent::ServerNewer`).

use crate::api::{Contract, Prune};
use crate::backend::Result;
use crate::contacts::Contact;
use crate::model::{CoreEvent, Subscription};

use super::{Engine, walk};

/// `fresh`, the list a newer studio served, with every `held` entry it
/// lacks (none in it is the `same`) kept after it, in held order.
pub(crate) fn keep_held<T: Clone>(
    held: &[T],
    fresh: Vec<T>,
    same: impl Fn(&T, &T) -> bool,
) -> Vec<T> {
    let mut out = fresh;
    for h in held {
        if !out.iter().any(|f| same(f, h)) {
            out.push(h.clone());
        }
    }
    out
}

/// The same contact: by the studio's id when both have one (a renamed
/// handle is still that contact), else by handle.
pub(crate) fn same_contact(a: &Contact, b: &Contact) -> bool {
    if a.id.is_empty() || b.id.is_empty() {
        a.handle == b.handle
    } else {
        a.id == b.id
    }
}

/// What [`Engine::hold_subscriptions`] did.
pub(crate) struct HeldSubscriptions {
    /// The list changed in a way its rows show (`walk::subscriptions_differ`).
    pub differ: bool,
    /// The store changed.
    pub changed: bool,
}

impl Engine {
    /// Hold the blyg's subscription list here (a pull, a refresh, a
    /// subscribe, a rename): one that would drop a subscription checks the
    /// contract again first, and the store keeps what the list lacks
    /// unless its lists may prune.
    pub(crate) fn hold_subscriptions(&self, subs: Vec<Subscription>) -> Result<HeldSubscriptions> {
        let held = self.store.subscriptions();
        let drops = held.iter().any(|h| !subs.iter().any(|s| s.id == h.id));
        let prune = if drops {
            self.recheck_contract()
        } else {
            self.store.lists_prune()
        };
        let (changed, now) = self.store.take_served_subscriptions(&subs, prune)?;
        Ok(HeldSubscriptions {
            differ: walk::subscriptions_differ(&held, &now),
            changed,
        })
    }

    /// Run a read that lays a list over what's held here (`merge_all`,
    /// `replace_thumbs`, `replace_hoppers`, `reconcile_read_state`). On a
    /// check from an earlier pull, a removal it would make is held back
    /// (`Store::take_held_back`): the contract is read again first, and on
    /// a contract that still allows it the read is laid again, removing
    /// this time. Pulls between safety nets on a studio without change
    /// counters run each list every pull, so an upgrade is seen before
    /// those lists remove anything.
    pub(crate) fn lay_list<T>(
        &self,
        lay: impl Fn() -> Result<T>,
        both: impl FnOnce(T, T) -> T,
    ) -> Result<T> {
        self.store.take_held_back();
        let first = lay()?;
        if self.store.take_held_back() && self.recheck_contract() == Prune::Allowed {
            let again = lay()?;
            return Ok(both(first, again));
        }
        Ok(first)
    }

    /// Read the contract again now (one conditional `GET
    /// /api/capabilities`), before something is removed on a list's word:
    /// a studio upgraded since the last check is seen first. What the
    /// blyg's lists may remove, as of now; a failed read removes nothing
    /// this time, and a contract now too old is left to the next pull's
    /// check to report (known no more: not called newer meanwhile).
    pub(crate) fn recheck_contract(&self) -> Prune {
        let Ok(version) = self.api.contract_version() else {
            return Prune::Withheld;
        };
        let contract = crate::api::contract(version.as_deref());
        let Some(prune) = contract.prune() else {
            self.state().compatible = false;
            self.store.forget_contract();
            return Prune::Withheld;
        };
        self.store.set_lists_prune(prune);
        if let Contract::Newer(version) = contract {
            self.tell_newer(version);
        }
        prune
    }

    /// Say once a session that the blyg runs a newer studio (`version`:
    /// its contract).
    pub(super) fn tell_newer(&self, version: String) {
        let first = !std::mem::replace(&mut self.state().newer_told, true);
        if first {
            self.emit(CoreEvent::ServerNewer(version));
        }
    }
}

#[cfg(test)]
#[path = "newer_tests.rs"]
mod engine_tests;

#[cfg(test)]
mod tests {
    use super::keep_held;

    #[test]
    fn what_a_newer_list_lacks_is_kept_after_it() {
        let held = vec![(1, "a"), (2, "b"), (3, "c")];
        let fresh = vec![(2, "B"), (4, "d")];
        let same = |a: &(i32, &str), b: &(i32, &str)| a.0 == b.0;
        assert_eq!(
            keep_held(&held, fresh, same),
            [(2, "B"), (4, "d"), (1, "a"), (3, "c")]
        );
        assert_eq!(keep_held(&held, vec![], same), held);
    }
}
