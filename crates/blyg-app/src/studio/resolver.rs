//! `![[id]]` → a quote and `[[id]]` → a link, from the local store only: your own items
//! (`Backend::items`) first, then imported reading items
//! (`Backend::reading`), in the Worker's `resolveTarget` order. Nothing is
//! ever fetched (SPEC § Protocol philosophy 3).
//!
//! The preview's resolver (`StoreResolver::lazy`) is built on the render
//! thread and looks up only the ids the text names, each once, as the
//! render asks for them: no copy of either list is taken, and nothing on
//! the UI thread. `StoreResolver::new` resolves from lists already in hand.

use std::cell::{OnceCell, RefCell};
use std::collections::HashMap;
use std::sync::Arc;

use blyg_core::{Backend, Item, Kind, ReadingItem, Status, SubscriptionKind};
use blyg_render::{
    Found, ItemKind, RenderOpts, Resolution, Resolver, UnresolvedReason, render_preview,
};

/// How deep a quoted thread's own quotes are baked (a quote of a quote of a
/// quote is still shown; deeper ones become the unresolved marker).
const MAX_DEPTH: usize = 3;

pub struct StoreResolver {
    /// Your own items by server id: all of them (`new`), or those looked
    /// up so far (`lazy`, `None` = not yours).
    own: RefCell<HashMap<String, Option<Item>>>,
    remote: RefCell<HashMap<String, Vec<(ReadingItem, bool)>>>,
    /// Where an id not in `own` / `remote` yet is looked up (`lazy`).
    source: Option<Arc<dyn Backend>>,
    /// The RSS subscriptions' ids (`lazy`: read on first need).
    rss: OnceCell<Vec<String>>,
    mount: String,
    /// Ids being baked right now (cycle and depth guard).
    stack: RefCell<Vec<String>>,
    /// --- contacts --- The address book, when the blyg rewrites `@handle`
    /// (`snapshot`: read now; `lazy`: on the first handle). Empty: every
    /// `@handle` stays prose, as publish leaves it on such a blyg.
    handles: OnceCell<Vec<blyg_core::contacts::Contact>>,
}

/// --- contacts --- The book, when publish would rewrite handles.
fn handle_book(backend: &dyn Backend) -> Vec<blyg_core::contacts::Contact> {
    if backend.rewrites_handles() {
        backend.contacts()
    } else {
        Vec::new()
    }
}

fn item_kind(k: Kind) -> ItemKind {
    match k {
        Kind::Fragment => ItemKind::Fragment,
        Kind::Thread => ItemKind::Thread,
    }
}

pub fn render_kind(k: Kind) -> blyg_render::Kind {
    match k {
        Kind::Fragment => blyg_render::Kind::Fragment,
        Kind::Thread => blyg_render::Kind::Thread,
    }
}

impl StoreResolver {
    /// `mount` prefixes provenance links to your own items (`{mount}/f/{id}/`).
    pub fn new(
        items: Vec<Item>,
        reading: Vec<ReadingItem>,
        rss_subs: &[String],
        mount: &str,
    ) -> Self {
        let own = items
            .into_iter()
            .filter_map(|i| i.server_id.clone().map(|s| (s.0, Some(i))))
            .collect();
        let mut remote: HashMap<String, Vec<(ReadingItem, bool)>> = HashMap::new();
        for r in reading {
            let rss = rss_subs.contains(&r.subscription_id);
            remote
                .entry(r.remote_id.clone())
                .or_default()
                .push((r, rss));
        }
        StoreResolver {
            own: RefCell::new(own),
            remote: RefCell::new(remote),
            source: None,
            rss: OnceCell::new(),
            mount: mount.to_string(),
            stack: RefCell::new(Vec::new()),
            handles: OnceCell::new(), // --- contacts ---
        }
    }

    /// Everything the backend holds locally, copied now.
    pub fn snapshot(backend: &dyn Backend, mount: &str) -> Self {
        let rss: Vec<String> = backend
            .subscriptions()
            .into_iter()
            .filter(|s| s.kind == SubscriptionKind::Rss)
            .map(|s| s.id)
            .collect();
        let r = Self::new(backend.items(), backend.reading(), &rss, mount);
        let _ = r.handles.set(handle_book(backend)); // --- contacts ---
        r
    }

    /// Resolves from what `backend` holds locally, looking each id up the
    /// first time the render asks for it (`Backend::own_item`,
    /// `Backend::reading_with_id`): the same answers as a snapshot of
    /// `items()` and `reading()`, without copying either.
    pub fn lazy(backend: Arc<dyn Backend>, mount: &str) -> Self {
        StoreResolver {
            source: Some(backend),
            ..Self::empty(mount)
        }
    }

    /// Your own item with server id `id`.
    fn own_item(&self, id: &str) -> Option<Item> {
        if let Some(hit) = self.own.borrow().get(id) {
            return hit.clone();
        }
        let src = self.source.as_ref()?;
        let found = src.own_item(id);
        self.own.borrow_mut().insert(id.to_string(), found.clone());
        found
    }

    /// The held reading rows with remote id `id`, each with whether it
    /// came through an RSS subscription.
    fn remote_rows(&self, id: &str) -> Vec<(ReadingItem, bool)> {
        if let Some(rows) = self.remote.borrow().get(id) {
            return rows.clone();
        }
        let Some(src) = self.source.as_ref() else {
            return Vec::new();
        };
        let rss = self.rss.get_or_init(|| {
            src.subscriptions()
                .into_iter()
                .filter(|s| s.kind == SubscriptionKind::Rss)
                .map(|s| s.id)
                .collect()
        });
        let rows: Vec<(ReadingItem, bool)> = src
            .reading_with_id(id)
            .into_iter()
            .map(|r| {
                let is_rss = rss.contains(&r.subscription_id);
                (r, is_rss)
            })
            .collect();
        self.remote
            .borrow_mut()
            .insert(id.to_string(), rows.clone());
        rows
    }

    /// Resolves nothing (a document without `[[`: no snapshot needed).
    pub fn empty(mount: &str) -> Self {
        Self::new(Vec::new(), Vec::new(), &[], mount)
    }

    /// Your own published item, baked as its snapshot would be.
    fn own(&self, id: &str, item: &Item) -> Resolution {
        match item.status {
            // A scratch note is never on the server: as unquotable as a draft.
            Status::Draft | Status::Scratch => {
                return Resolution::Unavailable(UnresolvedReason::Draft);
            }
            Status::Withdrawn => return Resolution::Unavailable(UnresolvedReason::Withdrawn),
            Status::Public => {}
        }
        if self.stack.borrow().iter().any(|s| s == id) {
            return Resolution::Unavailable(UnresolvedReason::Circular);
        }
        if self.stack.borrow().len() >= MAX_DEPTH {
            return Resolution::Unavailable(UnresolvedReason::Other(
                "quoted too deeply to preview".into(),
            ));
        }
        self.stack.borrow_mut().push(id.to_string());
        let opts = RenderOpts {
            data_line: false,
            provenance: true,
            mount: self.mount.clone(),
            self_id: Some(id.to_string()),
        };
        // The local copy is the working copy; for a post with unpublished
        // edits that is newer than the published snapshot publish would bake.
        let html = render_preview(&item.content_md, render_kind(item.kind), self, &opts).html;
        self.stack.borrow_mut().pop();
        Resolution::Found(Found {
            origin: None,
            id: id.to_string(),
            version: item.version,
            kind: item_kind(item.kind),
            content_html: html,
            author: None,
            page: None,
        })
    }

    fn imported(&self, id: &str, rows: &[(ReadingItem, bool)]) -> Resolution {
        let mut origins: Vec<&str> = rows.iter().map(|(r, _)| r.origin.as_str()).collect();
        origins.sort_unstable();
        origins.dedup();
        if origins.len() > 1 {
            return Resolution::Ambiguous;
        }
        let (r, rss) = &rows[0];
        if *rss {
            return Resolution::RssNotQuotable;
        }
        let version = if r.state == "tombstone" {
            match r.pinned_version_retained {
                Some(v) => v,
                None => return Resolution::Unavailable(UnresolvedReason::SourceWithdrawn),
            }
        } else {
            r.version
        };
        Resolution::Found(Found {
            origin: Some(r.origin.clone()),
            id: id.to_string(),
            version,
            kind: item_kind(r.kind),
            // Stored verbatim (spec §14: sanitize at render); the page CSP
            // is the backstop. Relative URLs belong to the quoted post's
            // blyg, not the page it's quoted into.
            content_html: super::sanitize::sanitize_at(&r.content_html, &r.origin),
            author: r
                .author
                .as_ref()
                .and_then(|a| a.name.clone())
                .filter(|n| !n.trim().is_empty()),
            page: r.page.clone(),
        })
    }
}

impl Resolver for StoreResolver {
    fn resolve(&self, id: &str) -> Resolution {
        if let Some(item) = self.own_item(id) {
            return self.own(id, &item);
        }
        let rows = self.remote_rows(id);
        if rows.is_empty() {
            Resolution::NotFound
        } else {
            self.imported(id, &rows)
        }
    }

    /// --- contacts --- `@handle` → its contact's h-card, as publish
    /// renders it (only where the blyg rewrites handles).
    fn contact(&self, handle: &str) -> Option<blyg_render::HandleCard> {
        let book = self
            .handles
            .get_or_init(|| self.source.as_deref().map(handle_book).unwrap_or_default());
        let c = book
            .iter()
            .find(|c| c.handle.eq_ignore_ascii_case(handle))?;
        Some(blyg_render::HandleCard {
            name: c.display_name().to_string(),
            url: c.origin.clone(),
            ids: c.checked_ids().into_iter().map(str::to_string).collect(),
        })
    }

    /// A `[[id]]` link bakes nothing, so the cycle and depth guards that
    /// stop a quote from containing itself don't apply: a guarded post is
    /// still linked, its excerpt taken from its own text.
    fn resolve_link(&self, id: &str) -> Resolution {
        let r = self.resolve(id);
        let guarded = matches!(
            r,
            Resolution::Unavailable(UnresolvedReason::Circular | UnresolvedReason::Other(_))
        );
        match self.own_item(id).filter(|_| guarded) {
            Some(item) => {
                let opts = RenderOpts {
                    data_line: false,
                    provenance: false,
                    mount: self.mount.clone(),
                    self_id: Some(id.to_string()),
                };
                let empty = StoreResolver::empty(&self.mount);
                Resolution::Found(Found {
                    origin: None,
                    id: id.to_string(),
                    version: item.version,
                    kind: item_kind(item.kind),
                    content_html: render_preview(
                        &item.content_md,
                        render_kind(item.kind),
                        &empty,
                        &opts,
                    )
                    .html,
                    author: None,
                    page: None,
                })
            }
            None => r,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use blyg_core::{Author, LocalId, ServerId};

    const OWN: &str = "0123456789abcdefghjkmnpqrs";
    const DRAFT: &str = "0123456789abcdefghjkmnpqrt";
    const THEIRS: &str = "zyxwvtsrqpnmkjhgfedcba9876";

    fn item(id: &str, status: Status, md: &str) -> Item {
        Item {
            local_id: LocalId(format!("local-{id}")),
            server_id: Some(ServerId(id.into())),
            kind: Kind::Fragment,
            status,
            version: 2,
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

    fn reading(id: &str, origin: &str, sub: &str) -> ReadingItem {
        ReadingItem {
            subscription_id: sub.into(),
            remote_id: id.into(),
            subscription_title: "Field Notes".into(),
            origin: origin.into(),
            kind: Kind::Fragment,
            state: "current".into(),
            version: 4,
            created: None,
            updated: None,
            observed_at: String::new(),
            content_md: "Tides keep time.".into(),
            content_html: "<p>Tides keep time.</p>\n".into(),
            author: Some(Author {
                name: Some("Field Notes".into()),
                extra: Default::default(),
                url: None,
            }),
            page: None,
            thumb: None,
            hoppers: vec![],
            pinned_version_retained: None,
            read_version: None,
            stub_of: None,
            forked_from: None,
            transclusions: vec![],
            lineage_known: false,
            source_url: None,
            generated: None,
        }
    }

    fn quote(r: &StoreResolver, id: &str) -> blyg_render::Rendered {
        render_preview(
            &format!("Intro\n\n![[{id}]]\n"),
            blyg_render::Kind::Thread,
            r,
            &RenderOpts::default(),
        )
    }

    #[test]
    fn own_items_first_then_imported() {
        let r = StoreResolver::new(
            vec![
                item(OWN, Status::Public, "Soil *matters*."),
                item(DRAFT, Status::Draft, "not yet"),
            ],
            vec![
                reading(THEIRS, "https://notes.example.org/", "s1"),
                // The same id imported too: own wins.
                reading(OWN, "https://notes.example.org/", "s1"),
            ],
            &[],
            "https://blyg.example.com",
        );
        let Resolution::Found(f) = r.resolve(OWN) else {
            panic!("own")
        };
        assert_eq!(f.origin, None);
        assert_eq!(f.version, 2);
        assert_eq!(f.content_html, "<p>Soil <em>matters</em>.</p>\n");

        let Resolution::Found(f) = r.resolve(THEIRS) else {
            panic!("imported")
        };
        assert_eq!(f.origin.as_deref(), Some("https://notes.example.org/"));
        assert_eq!(f.author.as_deref(), Some("Field Notes"));
        assert_eq!(f.version, 4);

        assert_eq!(
            r.resolve(DRAFT),
            Resolution::Unavailable(UnresolvedReason::Draft)
        );
        assert_eq!(
            r.resolve("00000000000000000000000000"),
            Resolution::NotFound
        );

        let out = quote(&r, THEIRS);
        assert_eq!(out.stats.quotes, 1);
        assert!(out.html.contains("blyg-transclusion"));
        assert!(out.html.contains("Tides keep time."));
        let out = quote(&r, DRAFT);
        assert_eq!(out.stats.unresolved.len(), 1);
        assert_eq!(out.stats.unresolved[0].reason, UnresolvedReason::Draft);
    }

    #[test]
    fn ambiguous_rss_and_withdrawn_sources() {
        let mut tomb = reading(THEIRS, "https://notes.example.org/", "s1");
        tomb.state = "tombstone".into();
        let r = StoreResolver::new(vec![], vec![tomb.clone()], &[], "/blyg");
        assert_eq!(
            r.resolve(THEIRS),
            Resolution::Unavailable(UnresolvedReason::SourceWithdrawn)
        );
        tomb.pinned_version_retained = Some(3);
        let r = StoreResolver::new(vec![], vec![tomb], &[], "/blyg");
        assert!(matches!(r.resolve(THEIRS), Resolution::Found(f) if f.version == 3));

        let r = StoreResolver::new(
            vec![],
            vec![reading(THEIRS, "https://feed.example.org/", "rss1")],
            &["rss1".to_string()],
            "/blyg",
        );
        assert_eq!(r.resolve(THEIRS), Resolution::RssNotQuotable);

        let r = StoreResolver::new(
            vec![],
            vec![
                reading(THEIRS, "https://notes.example.org/", "s1"),
                reading(THEIRS, "https://copy.example.net/", "s2"),
            ],
            &[],
            "/blyg",
        );
        assert_eq!(r.resolve(THEIRS), Resolution::Ambiguous);
    }

    #[test]
    fn quoted_threads_nest_without_cycles() {
        let mut a = item(OWN, Status::Public, &format!("A quotes B\n\n![[{DRAFT}]]"));
        a.kind = Kind::Thread;
        let mut b = item(DRAFT, Status::Public, &format!("B quotes A\n\n![[{OWN}]]"));
        b.kind = Kind::Thread;
        let r = StoreResolver::new(vec![a, b], vec![], &[], "/blyg");
        let out = quote(&r, OWN);
        assert_eq!(out.stats.quotes, 1);
        // B is baked inside A; B's quote of A is circular.
        assert!(out.html.contains("B quotes A"));
        assert!(out.html.contains("unresolved"));
    }

    #[test]
    fn empty_resolves_nothing() {
        let r = StoreResolver::empty("/blyg");
        assert_eq!(r.resolve(OWN), Resolution::NotFound);
    }

    #[test]
    fn links_resolve_to_anchors_even_in_a_cycle() {
        let own = item(OWN, Status::Public, &format!("Loops back to [[{OWN}]]."));
        let r = StoreResolver::new(vec![own], vec![], &[], "https://blyg.example.com/blyg");
        let opts = RenderOpts {
            data_line: false,
            mount: "https://blyg.example.com/blyg".into(),
            ..RenderOpts::default()
        };
        let out = render_preview(
            &format!("See [[{OWN}]]."),
            blyg_render::Kind::Fragment,
            &r,
            &opts,
        );
        assert!(
            out.html.contains(&format!(
                "<a href=\"https://blyg.example.com/blyg/f/{OWN}/\">“Loops back to"
            )),
            "{}",
            out.html
        );
        assert_eq!(out.stats.links, 1);
        assert!(out.stats.unresolved.is_empty());
    }
}
