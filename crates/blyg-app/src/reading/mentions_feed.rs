//! --- contacts --- The Mentions smart feed (studio 0.56's lens, proposal
//! 14), under All unread: the held posts that mention you. From the
//! blyg's `GET /api/reading?mentions=true` when it serves it (contract
//! 5.7), else from what this Mac holds: posts that quote, stub or fork one
//! of yours, or link to your blyg's address. A list with dots, never a
//! count; a plain link still notifies nobody (rule 6).

use std::collections::HashSet;

use blyg_core::ReadingItem;
use gpui_kit::*;

use super::sources_vm::{Source, is_background, needs_reading};
use super::vm::{Key, key};
use crate::app::MainView;

type Stamp = (usize, u64, Option<String>);

#[derive(Default)]
pub struct Feed {
    /// Worked out from the rows (`stamp` says which rows), kept until
    /// they change.
    local: std::cell::RefCell<(Option<Stamp>, HashSet<Key>)>,
    /// What the blyg's lens says, once asked (`None`: not served, or not
    /// asked yet).
    served: Option<HashSet<Key>>,
    asking: bool,
}

/// The held posts (not yours, not withdrawn) that point at `own_origin`:
/// a quote, stub or fork of one of your posts, or a link to your blyg.
pub fn local_mentions(rows: &[ReadingItem], own_origin: &str) -> HashSet<Key> {
    let Some(own) = blyg_core::profile::normalize_origin(own_origin) else {
        return HashSet::new();
    };
    let host = crate::vm::url_host(&own).unwrap_or_default();
    let needle = format!("://{host}");
    rows.iter()
        .filter(|r| !super::own::is_own_row(r) && !is_background(r) && !host.is_empty())
        .filter(|r| {
            r.references().iter().any(|p| p.origin == own)
                || links_to(&r.content_md, &needle)
                || (r.content_md.is_empty() && links_to(&r.content_html, &needle))
        })
        .map(key)
        .collect()
}

/// `text` holds `://host` ending where a host ends (not `://host.other`).
fn links_to(text: &str, needle: &str) -> bool {
    text.match_indices(needle).any(|(i, _)| {
        text[i + needle.len()..]
            .chars()
            .next()
            .is_none_or(|c| !(c.is_alphanumeric() || c == '.' || c == '-'))
    })
}

impl Feed {
    /// The keys of the posts that mention you: the lens's when served,
    /// else worked out from `rows` (again only when they changed).
    pub fn keys(&self, rows: &[ReadingItem], own_origin: Option<&str>) -> HashSet<Key> {
        if let Some(s) = &self.served {
            return s.clone();
        }
        let stamp = (
            rows.len(),
            rows.iter().fold(0u64, |a, r| {
                a.wrapping_mul(31).wrapping_add(r.version as u64)
            }),
            own_origin.map(str::to_string),
        );
        let mut local = self.local.borrow_mut();
        if local.0.as_ref() != Some(&stamp) {
            local.1 = own_origin
                .map(|o| local_mentions(rows, o))
                .unwrap_or_default();
            local.0 = Some(stamp);
        }
        local.1.clone()
    }

    /// Anything in the feed to read (its dot).
    pub fn has_unread(&self, rows: &[ReadingItem], own_origin: Option<&str>) -> bool {
        let keys = self.keys(rows, own_origin);
        !keys.is_empty()
            && rows
                .iter()
                .any(|r| keys.contains(&key(r)) && needs_reading(r))
    }
}

impl super::State {
    /// `shown` for the Mentions feed: the searched rows the feed holds.
    pub(crate) fn mentions_shown(&mut self, searched: &[usize]) -> Vec<usize> {
        let keys = self.mentioned.keys(&self.rows, self.own_origin.as_deref());
        searched
            .iter()
            .copied()
            .filter(|&i| {
                self.rows
                    .get(i)
                    .is_some_and(|r| !is_background(r) && keys.contains(&key(r)))
            })
            .collect()
    }
}

impl MainView {
    /// Ask the blyg's mentions lens (in the background) when the feed is
    /// picked; until it answers, or when it isn't served, the local feed.
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
                    f.served = Some(keys.into_iter().collect());
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

    use super::{Feed, is_background, key, local_mentions};
    use crate::fake::reading_seed;

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
    fn the_lens_wins_when_served() {
        let mut f = Feed::default();
        let rows: Vec<ReadingItem> = vec![];
        assert!(f.keys(&rows, Some("https://blyg.example.com")).is_empty());
        f.served = Some(HashSet::from([("s".to_string(), "r".to_string())]));
        assert_eq!(f.keys(&rows, None).len(), 1);
    }
}
