//! --- newer contract --- A blyg whose studio speaks a contract major newer
//! than this Burrow knows (`api::Contract::Newer`; a version that isn't a
//! number counts too). Studio 0.67 moved to contract 6, and the Burrow of
//! the day refused to sync at all until a hotfix: every new major was an
//! outage. Now:
//!
//! - Sync goes on, each feature gated on the operations the blyg's
//!   capabilities list, as on any studio (skill `capability-gating`). Your
//!   writes (publish, edits, read marks, hoppers, the shelf) go through.
//! - Nothing held here is removed on the word of that studio's lists, until
//!   a Burrow that knows the contract: a page in a shape Burrow doesn't know
//!   would read as an empty or ended list. The store holds the one switch
//!   (`Store::lists_prune`, set by `Engine::check_server`), and every
//!   removal a list drives asks it:
//!   - your items (`Store::merge_all`);
//!   - reading rows after a whole walk (`walk::Read`'s `Step::Done`, and
//!     `Store::prune_reading` beneath it);
//!   - thumbs a signals list lacks (`Store::replace_thumbs`);
//!   - hoppers and memberships (`Store::replace_hoppers`);
//!   - read marks another device seems to have cleared
//!     (`Store::reconcile_read_state`);
//!   - subscriptions and contacts, kept here by [`keep_held`];
//!   - the shelf, merged as on a first sync (the union), not against the
//!     last agreed list (`Engine::sync_shelf`).
//! - The app is told once a session (`CoreEvent::ServerNewer`).

use crate::model::CoreEvent;

use super::Engine;

/// `fresh`, the list a newer studio served, with every `held` entry it
/// lacks (by `id`) kept after it, in held order.
pub(crate) fn keep_held<T: Clone, K: PartialEq>(
    held: &[T],
    fresh: Vec<T>,
    id: impl Fn(&T) -> K,
) -> Vec<T> {
    let mut out = fresh;
    for h in held {
        if !out.iter().any(|f| id(f) == id(h)) {
            out.push(h.clone());
        }
    }
    out
}

impl Engine {
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
mod tests {
    use super::keep_held;

    #[test]
    fn what_a_newer_list_lacks_is_kept_after_it() {
        let held = vec![(1, "a"), (2, "b"), (3, "c")];
        let fresh = vec![(2, "B"), (4, "d")];
        assert_eq!(
            keep_held(&held, fresh, |x| x.0),
            [(2, "B"), (4, "d"), (1, "a"), (3, "c")]
        );
        assert_eq!(keep_held(&held, vec![], |x| x.0), held);
    }
}
