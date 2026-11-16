//! --- contacts --- The Mentions smart feed (studio 0.56's lens, proposal
//! 14), under All unread: the held posts that mention you. Both sources at
//! once, each row tagged with why (`mentions_vm`): the blyg's `GET
//! /api/reading?mentions=true` when it serves it (contract 5.7: the post's
//! own h-cards name you), and what this Mac holds: posts that quote, stub
//! or fork one of yours, or link to your blyg's address. The lens's answer
//! only adds rows, so the list doesn't shrink when it arrives, and its
//! last answer is kept (`Backend::reading_mentions_cached`) for the first
//! paint. A list with dots, never a count; a plain link still notifies
//! nobody (rule 6).

use std::cell::{Ref, RefCell};
use std::collections::{HashMap, HashSet};

use blyg_core::ReadingItem;
use gpui_kit::*;

use super::mentions_vm::{self, Why};
use super::sources_vm::{Source, is_background, needs_reading};
use super::vm::{Key, key};
use crate::app::MainView;

type Stamp = (usize, u64, Option<String>);

#[derive(Default)]
pub struct Feed {
    /// Worked out from the rows (`stamp` says which rows), kept until
    /// they change: each post and why.
    local: RefCell<(Option<Stamp>, HashMap<Key, Why>)>,
    /// What the blyg's lens says, once asked or as last kept (`None`: not
    /// served, or not known yet).
    served: Option<HashSet<Key>>,
    asking: bool,
}

impl Feed {
    /// The local half, worked out again when `rows` changed.
    fn local(&self, rows: &[ReadingItem], own_origin: Option<&str>) -> Ref<'_, HashMap<Key, Why>> {
        let stamp = (
            rows.len(),
            rows.iter().fold(0u64, |a, r| {
                a.wrapping_mul(31).wrapping_add(r.version as u64)
            }),
            own_origin.map(str::to_string),
        );
        if self.local.borrow().0.as_ref() != Some(&stamp) {
            let mut local = self.local.borrow_mut();
            local.1 = own_origin
                .map(|o| mentions_vm::local_mentions(rows, o))
                .unwrap_or_default();
            local.0 = Some(stamp);
        }
        Ref::map(self.local.borrow(), |l| &l.1)
    }

    fn named(&self, k: &Key) -> bool {
        self.served.as_ref().is_some_and(|s| s.contains(k))
    }

    /// The lens's answer (`None`: not served). It only adds to the feed.
    pub fn set_served(&mut self, keys: Option<HashSet<Key>>) {
        self.served = keys;
    }

    /// The keys of the posts that mention you: the lens's and the local
    /// ones together (the local again only when the rows changed).
    #[cfg(test)]
    pub fn keys(&self, rows: &[ReadingItem], own_origin: Option<&str>) -> HashSet<Key> {
        let mut keys: HashSet<Key> = self.local(rows, own_origin).keys().cloned().collect();
        keys.extend(self.served.iter().flatten().cloned());
        keys
    }

    /// Why the row `k` is in the feed, as its tags ("named you" first; at
    /// most two). Reads the local half as last worked out.
    pub fn tags(&self, k: &Key) -> Vec<&'static str> {
        let local = self.local.borrow().1.get(k).copied();
        mentions_vm::tags(self.named(k), local)
    }

    /// Anything in the feed to read (its dot).
    pub fn has_unread(&self, rows: &[ReadingItem], own_origin: Option<&str>) -> bool {
        let local = self.local(rows, own_origin);
        rows.iter().any(|r| {
            let k = key(r);
            (local.contains_key(&k) || self.named(&k)) && !is_background(r) && needs_reading(r)
        })
    }
}

impl super::State {
    /// `shown` for the Mentions feed: the searched rows the feed holds
    /// (never your own, never Background).
    pub(crate) fn mentions_shown(&mut self, searched: &[usize]) -> Vec<usize> {
        let f = &self.mentioned;
        let local = f.local(&self.rows, self.own_origin.as_deref());
        searched
            .iter()
            .copied()
            .filter(|&i| {
                self.rows.get(i).is_some_and(|r| {
                    let k = key(r);
                    !is_background(r)
                        && !super::own::is_own_row(r)
                        && (local.contains_key(&k) || f.named(&k))
                })
            })
            .collect()
    }
}

impl MainView {
    /// A reading row's tags: why it's in the Mentions feed, while that
    /// feed is the one listed; none elsewhere.
    pub(crate) fn mention_tags(&self, k: &Key) -> Vec<&'static str> {
        let r = &self.reading;
        if r.mode == super::stream_vm::ReadMode::Reader && r.source == Source::Mentions {
            r.mentioned.tags(k)
        } else {
            vec![]
        }
    }

    /// The lens's answer as last kept, for the first paint (local: a
    /// meta read when Reading opens).
    pub(crate) fn load_kept_reading_mentions(&mut self) {
        let kept = self.backend.reading_mentions_cached();
        self.reading
            .mentioned
            .set_served(kept.map(|k| k.into_iter().collect()));
    }

    /// Ask the blyg's mentions lens (in the background) when the feed is
    /// picked; its posts join the local ones when it answers.
    pub(crate) fn load_reading_mentions(&mut self, cx: &mut Context<Self>) {
        if self.reading.source != Source::Mentions || self.reading.mentioned.asking {
            return;
        }
        self.reading.mentioned.asking = true;
        let backend = self.backend.clone();
        let task = cx.background_spawn(async move { backend.reading_mentions() });
        cx.spawn(async move |this, cx| {
            let got = task.await;
            let _ = this.update(cx, |v, cx| {
                let f = &mut v.reading.mentioned;
                f.asking = false;
                if let Some(keys) = got {
                    f.set_served(Some(keys.into_iter().collect()));
                    v.reading.refilter();
                    cx.notify();
                }
            });
        })
        .detach();
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use blyg_core::ReadingItem;

    use super::{Feed, is_background, key};
    use crate::fake::reading_seed;

    fn local_mentions(rows: &[ReadingItem], own: &str) -> HashSet<super::Key> {
        super::mentions_vm::local_mentions(rows, own)
            .into_keys()
            .collect()
    }

    #[test]
    fn posts_linking_to_your_blyg_mention_you() {
        let now = chrono::Utc::now();
        let sd = reading_seed::seed(now);
        let mut rows = sd.reading.clone();
        let i = rows
            .iter()
            .position(|r| !super::super::own::is_own_row(r) && !is_background(r))
            .unwrap();
        rows.swap(0, i);
        rows[0].content_md = "As [Sam](https://blyg.example.com/t/x/) wrote".into();
        let k = local_mentions(&rows, "https://blyg.example.com/");
        assert!(k.contains(&key(&rows[0])));
        // A host that merely starts the same isn't you.
        rows[0].content_md = "see https://blyg.example.community/".into();
        assert!(!local_mentions(&rows, "https://blyg.example.com").contains(&key(&rows[0])));
    }

    #[test]
    fn the_lens_adds_to_the_local_feed() {
        let now = chrono::Utc::now();
        let mut rows: Vec<ReadingItem> = reading_seed::seed(now).reading;
        let i = rows
            .iter()
            .position(|r| !super::super::own::is_own_row(r) && !is_background(r))
            .unwrap();
        rows.swap(0, i);
        rows[0].content_md = "see https://blyg.example.com/".into();
        let own = Some("https://blyg.example.com");
        let mut f = Feed::default();
        let before = f.keys(&rows, own);
        assert!(before.contains(&key(&rows[0])));
        assert_eq!(f.tags(&key(&rows[0])), ["linked you"]);
        let named = ("s".to_string(), "r".to_string());
        f.set_served(Some(HashSet::from([named.clone(), key(&rows[0])])));
        let after = f.keys(&rows, own);
        assert!(after.is_superset(&before));
        assert!(after.contains(&named));
        assert_eq!(f.tags(&key(&rows[0])), ["named you", "linked you"]);
        assert_eq!(f.tags(&named), ["named you"]);
    }
}
