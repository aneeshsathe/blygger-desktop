//! --- own posts --- Your own published posts on the reading surface, and
//! the reading actions on them from the Posts view.
//!
//! In the Reading view your blyg is a source like the others ("Your blyg" in
//! the sources pane; its posts also sit in All, Today and the stream by
//! date), with the same reading pane and the same action chips. A row is
//! built here from your published `Item` (`row`) under the key
//! `("~own", server id)`, blygger-studio 0.45's own-items marker
//! (`OWN_SUBSCRIPTION`). It is never a subscription: it's never unread,
//! never counted in All unread, never thumbed, and nothing about it is sent
//! as read state or a signal (`is_own`, checked wherever a reading row's key
//! would reach the server). Its versions are your own history (current and
//! pinned only, as for anyone's post) and its pins are read from your public
//! pin files.
//!
//! The Posts view reaches the same actions (Reply · new stub, Quote into a
//! thread, Fork, Link post) through `ReplyToPost`, `QuotePost`, `ForkPost`
//! and `LinkToPost`: menu items with keys, and a row of chips above the
//! editor on a published post. They run the Reading view's own code
//! (`item_action`, `fork_pin`) on the post's row, so the drafts carry the
//! same references a reply to anyone else's post carries. Protocol 0.4 for
//! an own-origin source: `stub_of` and `forked_from` name the origin even
//! when it is the publisher's own (§5.6 rule 1, §5.9, §10.6: "a citation is
//! absolute"), a fork descends from a pinned version only (§5.6 rule 2), and
//! a quote's `transclusions[]` entry omits `origin` (§10.3), which the
//! server writes at publish.

use blyg_core::{Item, ReadingItem, RemoteRef, RemoteVersion, Status, StubOf, Version};
use gpui_kit::*;

use super::vm::{self, Key};
use super::{Load, View};
use crate::app::MainView;

gpui_kit::actions!(blygger, [ReplyToPost, QuotePost, ForkPost, LinkToPost]);

/// The subscription id of your own rows: studio 0.45's marker for own items
/// in `hopper_items`. Never a real subscription id (those are server ids).
pub const OWN: &str = "~own";

/// What the sources pane and the rows call your blyg.
pub const OWN_LABEL: &str = "Your blyg";

/// One of your own rows (never sent anywhere as a reading row).
pub fn is_own(key: &Key) -> bool {
    key.0 == OWN
}

pub fn is_own_row(r: &ReadingItem) -> bool {
    r.subscription_id == OWN
}

/// An origin as a citation spells it: absolute, with a trailing slash
/// (blygger-studio's `normalizeOrigin`, which a reference is compared in).
pub fn citation_origin(origin: &str) -> String {
    format!("{}/", origin.trim_end_matches('/'))
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
        subscription_id: OWN.into(),
        remote_id: sid,
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
    })
}

/// Every published post of yours as a row, in `items`' order.
pub fn rows(items: &[Item], origin: &str) -> Vec<ReadingItem> {
    items.iter().filter_map(|i| row(i, origin)).collect()
}

/// The reading rows with yours among them by date, newest first. The other
/// rows keep the order they came in; each of yours goes before the first
/// one older than it.
pub fn merge(theirs: Vec<ReadingItem>, mut own: Vec<ReadingItem>) -> Vec<ReadingItem> {
    if own.is_empty() {
        return theirs;
    }
    // `sort_at` is one normalized ISO spelling, so it compares as text.
    let at = |r: &ReadingItem| r.sort_at();
    own.sort_by_key(|r| std::cmp::Reverse(at(r)));
    let mut out = Vec::with_capacity(theirs.len() + own.len());
    let mut own = own.into_iter().peekable();
    for r in theirs {
        let t = at(&r);
        while own.peek().is_some_and(|o| at(o) > t) {
            out.extend(own.next());
        }
        out.push(r);
    }
    out.extend(own);
    out
}

/// The rows that aren't yours (the lineage index counts your posts from
/// the posts themselves).
pub fn without_own(rows: &[ReadingItem]) -> Vec<ReadingItem> {
    rows.iter().filter(|r| !is_own_row(r)).cloned().collect()
}

/// Your history as the reading pane's changelog: every version with its
/// note, `current` on the published one. The pill then shows the current
/// and pinned ones only, as for anyone's post (`vm::shown`).
pub fn changelog(own: &[Version], current: u32) -> Vec<RemoteVersion> {
    own.iter()
        .filter(|v| !v.endcap)
        .map(|v| RemoteVersion {
            version: v.version,
            at: v.published_at.clone(),
            note: v.note.clone(),
            pinned: v.pinned,
            current: v.version == current,
            media: vec![],
            lineage: Default::default(),
        })
        .collect()
}

/// The newest pinned version, what Fork on a post descends from.
pub fn newest_pin(own: &[Version]) -> Option<u32> {
    own.iter()
        .filter(|v| v.pinned && !v.endcap)
        .map(|v| v.version)
        .max()
}

/// --- own fork --- Forking one of your own pins (`ForkPost`, "Fork this
/// pin" on your row, the lineage ring's Fork on your post): the one switch.
/// Protocol 0.4 allows it (§5.6: `forked_from` names a pinned version,
/// `origin` required "even when it is the publisher's own") and
/// blygger-studio offers it beside each pinned version in your history;
/// off, every one of those places hides or refuses it and nothing else
/// changes.
pub const FORK_OWN: bool = true;

/// The reading pane's actions on your own row: no AI reply to yourself, and
/// no Fork when `FORK_OWN` is off.
pub fn own_action_ids(ids: Vec<&'static str>) -> Vec<&'static str> {
    ids.into_iter()
        .filter(|id| *id != "AI reply" && (FORK_OWN || !matches!(*id, "Fork" | "Fork this pin")))
        .collect()
}

/// The Posts view's actions (`POST_ACTIONS`, Fork only when `FORK_OWN`).
pub fn post_actions() -> Vec<(&'static str, &'static str)> {
    POST_ACTIONS
        .into_iter()
        .filter(|(id, _)| FORK_OWN || *id != "Fork")
        .collect()
}

/// The actions the Posts view's chips and keys offer, in order, with the
/// key action each runs.
pub const POST_ACTIONS: [(&str, &str); 4] = [
    ("Reply", "ReplyToPost"),
    ("Quote", "QuotePost"),
    ("Fork", "ForkPost"),
    ("Link post", "LinkToPost"),
];

/// A Posts-view chip: the Reading view's label and sentence for the same
/// action, Fork named for the pin it takes.
pub fn post_chip(id: &'static str, pin: Option<u32>) -> vm::ActionChip {
    let ctx = vm::ActionCtx {
        blyg: true,
        ..Default::default()
    };
    match id {
        "Fork" => vm::ActionChip {
            id,
            label: "Fork a pin".into(),
            tip: match pin {
                Some(v) => format!(
                    "Makes a new draft of yours from 📌 v{v}'s text, recorded as forked from \
                     it (forked_from): forks descend from pins only."
                ),
                None => "Makes a new draft of yours from this post's newest pinned version, \
                         recorded as forked from it (forked_from). Forks descend from pins \
                         only: pin a version in ⌘Y first."
                    .into(),
            },
            enabled: true,
        },
        other => vm::action_chip(other, &ctx),
    }
}

impl MainView {
    /// Your published posts as reading rows, at your blyg's origin.
    pub(super) fn own_rows(&self) -> Vec<ReadingItem> {
        match self.backend.base_url() {
            Some(origin) => rows(&self.backend.items(), &origin),
            None => vec![],
        }
    }

    /// The reading rows as the screen lists them: the backend's, with your
    /// own published posts among them.
    pub(super) fn reading_with_own(&self) -> Vec<ReadingItem> {
        vm::order(merge(self.backend.reading(), self.own_rows()))
    }

    /// The local id of your post `server_id`.
    pub(super) fn own_local_id(&self, server_id: &str) -> Option<blyg_core::LocalId> {
        self.backend
            .items()
            .into_iter()
            .find(|i| {
                i.server_id
                    .as_ref()
                    .is_some_and(|s| s.0.eq_ignore_ascii_case(server_id))
            })
            .map(|i| i.local_id)
    }

    /// The open own row's versions (your history) and, when the working copy
    /// has unpublished edits, the published text from your public item
    /// document. Nothing is marked read.
    pub(super) fn load_own_opened(
        &mut self,
        key: Key,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(local) = self.own_local_id(&key.1) else {
            return;
        };
        let dirty = self.backend.item(&local).is_some_and(|i| i.dirty);
        let origin = self
            .reading
            .opened
            .as_ref()
            .map(|o| o.item.origin.clone())
            .unwrap_or_default();
        let backend = self.backend.clone();
        let rid = key.1.clone();
        let task = cx.background_spawn(async move {
            let versions = backend.versions(&local);
            let published = dirty
                .then(|| crate::guarded(|| backend.public_item(&origin, &rid)).ok())
                .flatten();
            (versions, published)
        });
        let want = self.reading.want_version.take();
        cx.spawn_in(window, async move |this, cx| {
            let (versions, published) = task.await;
            let _ = this.update_in(cx, |v, _, cx| {
                let Some(o) = v.reading.opened.as_mut().filter(|o| o.key == key) else {
                    return;
                };
                if let Some(p) = published {
                    o.item.content_md = p.item.content_md;
                    o.item.content_html = p.item.content_html;
                }
                match versions.map(|vs| changelog(&vs, o.item.version)) {
                    Ok(log) => {
                        o.shown = Load::Ready(vm::shown(&log));
                        o.changelog = Load::Ready(log);
                    }
                    Err(e) => {
                        let failed: Load<Vec<RemoteVersion>> = Load::from_result(Err(e));
                        o.shown = match &failed {
                            Load::Failed(m) => Load::Failed(m.clone()),
                            _ => Load::Unavailable,
                        };
                        o.changelog = failed;
                    }
                }
                o.ix = o.shown.ready().and_then(|s| vm::current_ix(s));
                let pick = want
                    .and_then(|w| o.shown.ready()?.iter().position(|s| s.version == w))
                    .filter(|&i| Some(i) != o.ix);
                if let Some(i) = pick {
                    v.select_version(i, cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Fork `of` (a pinned version) into a new draft and open it: the
    /// Reading view's "Fork this pin", the lineage ring's Fork and the Posts
    /// view's Fork all come here.
    pub(super) fn fork_pin(&mut self, of: RemoteRef, window: &mut Window, cx: &mut Context<Self>) {
        let version = of.version;
        let backend = self.backend.clone();
        let task = cx.background_spawn(async move { backend.fork(&of) });
        cx.spawn_in(window, async move |this, cx| {
            let r = task.await;
            let _ = this.update_in(cx, |this, window, cx| match r {
                Ok(id) => {
                    this.open_new_draft(&id, window, cx);
                    this.show_toast(format!("Forked 📌 v{version}"), None, cx);
                }
                Err(e) => this.show_toast(format!("Couldn't fork: {e}"), None, cx),
            });
        })
        .detach();
    }

    /// Fork your post `row` from its newest pinned version (your history,
    /// read first), or say there's none to fork.
    pub(super) fn fork_own_newest_pin(
        &mut self,
        row: ReadingItem,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !FORK_OWN {
            return self.show_toast("Your own posts aren't forked here", None, cx);
        }
        let Some(local) = self.own_local_id(&row.remote_id) else {
            return;
        };
        let backend = self.backend.clone();
        let task = cx.background_spawn(async move { backend.versions(&local) });
        cx.spawn_in(window, async move |this, cx| {
            let r = task.await;
            let _ = this.update_in(cx, |this, window, cx| match r.map(|vs| newest_pin(&vs)) {
                Ok(Some(version)) => this.fork_pin(
                    RemoteRef {
                        origin: row.origin.clone(),
                        id: row.remote_id.clone(),
                        version,
                        cited: None,
                    },
                    window,
                    cx,
                ),
                Ok(None) => this.show_toast(
                    "No pinned version to fork",
                    Some("Forks descend from pins only: pin a version in ⌘Y first".into()),
                    cx,
                ),
                Err(e) => this.show_toast(format!("Couldn't read the versions: {e}"), None, cx),
            });
        })
        .detach();
    }

    // ------------------------------------------------------------ Posts view

    /// The post open in the Posts view as a reading row, when it's one of
    /// your published posts.
    pub(crate) fn current_own_row(&self) -> Option<ReadingItem> {
        let item = self.current.as_ref()?;
        row(item, &self.backend.base_url()?)
    }

    /// Hook: the actions on the root element.
    pub(super) fn own_post_actions(
        &self,
        d: Stateful<Div>,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        d.on_action(
            cx.listener(|this, _: &ReplyToPost, window, cx| this.post_action("Reply", window, cx)),
        )
        .on_action(
            cx.listener(|this, _: &QuotePost, window, cx| this.post_action("Quote", window, cx)),
        )
        .on_action(
            cx.listener(|this, _: &ForkPost, window, cx| this.post_action("Fork", window, cx)),
        )
        .on_action(
            cx.listener(|this, _: &LinkToPost, window, cx| {
                this.post_action("Link post", window, cx)
            }),
        )
    }

    /// Reply · new stub, Quote into a thread, Fork or Link post on the post
    /// in front of you: in the Reading view the open post (else the
    /// stream's selected one), in the Posts view your published post.
    pub(crate) fn post_action(
        &mut self,
        action: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.reading.view {
            View::Reading => self.reading_post_action(action, window, cx),
            View::Posts => {
                let Some(row) = self.current_own_row() else {
                    self.show_toast(
                        "Open one of your published posts first",
                        Some("Drafts can't be replied to, quoted, forked or linked yet".into()),
                        cx,
                    );
                    return;
                };
                match action {
                    "Fork" => self.fork_own_newest_pin(row, window, cx),
                    _ => self.item_action(row, action, window, cx),
                }
            }
            _ => {}
        }
    }

    fn reading_post_action(
        &mut self,
        action: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(o) = self.reading.opened.as_ref() {
            let pinned = o.pinned_on_screen().is_some();
            let own = is_own(&o.key);
            match action {
                "Fork" if pinned => self.reading_action("Fork this pin", window, cx),
                "Fork" if own => {
                    let row = o.item.clone();
                    self.fork_own_newest_pin(row, window, cx)
                }
                "Fork" => {
                    let tip = self
                        .reading_action_chips()
                        .into_iter()
                        .find(|c| c.id == "Fork")
                        .map(|c| c.tip)
                        .unwrap_or_default();
                    self.show_toast("Fork needs a pinned version", Some(tip.into()), cx)
                }
                _ => self.reading_action(action, window, cx),
            }
            return;
        }
        let row = self
            .reading
            .sel
            .clone()
            .and_then(|k| self.reading.rows.iter().find(|r| vm::key(r) == k).cloned());
        match (row, action) {
            (Some(r), "Fork") if is_own_row(&r) => self.fork_own_newest_pin(r, window, cx),
            (Some(_), "Fork") => self.show_toast(
                "Fork needs a pinned version",
                Some("Open the post (⏎) and pick 📌 vN in its ‹ vM ▾ ›".into()),
                cx,
            ),
            (Some(r), _) => self.item_action(r, action, window, cx),
            (None, _) => self.show_toast("Pick a post first", None, cx),
        }
    }

    /// Hook (the editor pane, above the text): the reading actions on your
    /// published post, each chip's tooltip ending with its key. `None` on a
    /// draft or scratch note.
    pub(crate) fn render_own_post_bar(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let row = self.current_own_row()?;
        let p = self.palette.on_page();
        let chips = post_actions().into_iter().map(|(id, action)| {
            let chip = post_chip(id, None);
            let key = crate::keymap::table()
                .into_iter()
                .find(|k| k.action == action && !k.key.is_empty())
                .map(|k| crate::keymap::glyphs(k.key));
            let tip = match key {
                Some(k) => format!("{}  {k}", chip.tip),
                None => chip.tip,
            };
            self.chip(format!("own-act-{id}"), chip.label)
                .debug_selector(move || format!("own-act-{id}"))
                .tooltip(move |_, cx| cx.new(|_| super::Tip(tip.clone())).into())
                .on_click(cx.listener(move |this, _, window, cx| {
                    cx.stop_propagation();
                    this.post_action(id, window, cx)
                }))
                .into_any_element()
        });
        Some(
            div()
                .id("own-post-bar")
                .debug_selector(|| "own-post-bar".into())
                .flex_none()
                .flex()
                .flex_wrap()
                .items_center()
                .gap(px(6.))
                .px(px(24.))
                .pt(px(10.))
                .pb(px(4.))
                .font_family(self.chrome())
                .text_size(px(11.5))
                .text_color(p.muted)
                .child(format!("Your post · v{} ·", row.version))
                .children(chips)
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    // Not `super::*`: that would bring gpui's `test` attribute in.
    use super::{
        FORK_OWN, OWN, POST_ACTIONS, changelog, citation_origin, is_own, is_own_row, merge,
        newest_pin, own_action_ids, post_actions, post_chip, row, vm,
    };
    use blyg_core::{
        Item, Kind, LocalId, ReadingItem, RemoteRef, ResponsesMode, ServerId, Status, Version,
    };

    const ORIGIN: &str = "https://blyg.example.com";
    const SID: &str = "7c9wk2n4h6q1x8v0z3m5rjy2ke";

    fn item(status: Status, version: u32, at: &str) -> Item {
        Item {
            local_id: LocalId("L1".into()),
            server_id: Some(ServerId(SID.into())),
            kind: Kind::Fragment,
            status,
            version,
            dirty: false,
            content_md: "Harbour notes, a sample".into(),
            created: at.into(),
            updated: at.into(),
            permalink: Some(format!("{ORIGIN}/f/{SID}")),
            stub_of: None,
            forked_from: None,
            show_responses: true,
            responses_mode: Some(ResponsesMode::Default),
            pending_sync: false,
            conflict: false,
            highlight: false,
            highlight_mode: None,
        }
    }

    fn reading(id: &str, at: &str) -> ReadingItem {
        let mut r = row(&item(Status::Public, 1, at), ORIGIN).unwrap();
        r.subscription_id = "sub-a".into();
        r.remote_id = id.into();
        r.read_version = None;
        r
    }

    #[test]
    fn only_published_posts_become_rows_read_and_unthumbed() {
        let r = row(&item(Status::Public, 3, "2026-10-01T10:00:00Z"), ORIGIN).unwrap();
        assert_eq!(vm::key(&r), (OWN.to_string(), SID.to_string()));
        assert!(is_own_row(&r) && is_own(&vm::key(&r)));
        assert_eq!(r.origin, "https://blyg.example.com/");
        assert_eq!(r.read_version, Some(3), "never unread");
        assert!(!r.is_unread() && !r.edited_since_read());
        assert!(r.thumb.is_none() && r.hoppers.is_empty());
        assert_eq!(
            r.page.as_deref(),
            Some("https://blyg.example.com/f/7c9wk2n4h6q1x8v0z3m5rjy2ke")
        );
        assert!(row(&item(Status::Draft, 0, "2026-10-01T10:00:00Z"), ORIGIN).is_none());
        assert!(row(&item(Status::Withdrawn, 2, "2026-10-01T10:00:00Z"), ORIGIN).is_none());
        let mut unnumbered = item(Status::Public, 1, "2026-10-01T10:00:00Z");
        unnumbered.server_id = None;
        assert!(row(&unnumbered, ORIGIN).is_none());
        // Unpublished edits: dated by when it was written, not the edit.
        let mut dirty = item(Status::Public, 1, "2026-10-01T10:00:00Z");
        dirty.updated = "2026-10-05T10:00:00Z".into();
        dirty.dirty = true;
        assert!(row(&dirty, ORIGIN).unwrap().updated.is_none());
    }

    #[test]
    fn a_self_stubs_lineage_reads_as_a_stub_of() {
        let mut it = item(Status::Public, 1, "2026-10-01T10:00:00Z");
        it.kind = Kind::Thread;
        it.stub_of = Some(RemoteRef {
            origin: "https://blyg.example.com/".into(),
            id: "0123456789abcdefghjkmnpqrs".into(),
            version: 2,
            cited: None,
        });
        let r = row(&it, ORIGIN).unwrap();
        let s = r.stub_of.unwrap();
        assert_eq!(s.origin.as_deref(), Some("https://blyg.example.com/"));
        assert_eq!(s.version, Some(2));
    }

    #[test]
    fn own_rows_merge_by_date_without_reordering_the_rest() {
        let theirs = vec![
            reading("a", "2026-10-05T10:00:00Z"),
            reading("b", "2026-10-03T10:00:00Z"),
            reading("c", "2026-10-04T10:00:00Z"), // out of order: kept so
        ];
        let mut mine = vec![];
        for (sid, at) in [
            ("m1", "2026-10-04T12:00:00Z"),
            ("m0", "2026-10-06T10:00:00Z"),
            ("m2", "2026-01-01T00:00:00Z"),
        ] {
            let mut r = row(&item(Status::Public, 1, at), ORIGIN).unwrap();
            r.remote_id = sid.into();
            mine.push(r);
        }
        let ids: Vec<String> = merge(theirs, mine)
            .into_iter()
            .map(|r| r.remote_id)
            .collect();
        assert_eq!(ids, ["m0", "a", "m1", "b", "c", "m2"]);
        assert!(merge(vec![], vec![]).is_empty());
    }

    #[test]
    fn own_history_shows_current_and_pins_and_forks_the_newest_pin() {
        let v = |n: u32, pinned: bool| Version {
            version: n,
            published_at: format!("2026-10-0{n}T10:00:00Z"),
            note: Some(format!("note {n}")),
            pinned,
            endcap: false,
        };
        let own = vec![v(1, true), v(2, false), v(3, true), v(4, false)];
        let log = changelog(&own, 4);
        assert_eq!(log.len(), 4);
        assert!(log[3].current && !log[2].current);
        let shown: Vec<u32> = vm::shown(&log).iter().map(|v| v.version).collect();
        assert_eq!(shown, [1, 3, 4], "unpinned history never shows");
        assert_eq!(newest_pin(&own), Some(3));
        assert_eq!(newest_pin(&own[1..2]), None);
    }

    #[test]
    fn post_chips_name_what_they_make() {
        let labels: Vec<String> = POST_ACTIONS
            .iter()
            .map(|(id, _)| post_chip(id, None).label)
            .collect();
        assert_eq!(
            labels,
            [
                "Reply · new stub",
                "Quote into a thread",
                "Fork a pin",
                "Link post · new fragment"
            ]
        );
        for (id, _) in POST_ACTIONS {
            let c = post_chip(id, Some(2));
            assert!(c.enabled && !c.tip.is_empty(), "{id}");
        }
        assert!(post_chip("Fork", Some(2)).tip.contains("📌 v2"));
        assert!(post_chip("Fork", None).tip.contains("pins only"));
    }

    #[test]
    fn every_post_action_is_a_bound_key_in_the_post_menu() {
        let table = crate::keymap::table();
        for (_, action) in POST_ACTIONS {
            let k = table
                .iter()
                .find(|k| k.action == action)
                .unwrap_or_else(|| panic!("{action} isn't in keymap::table()"));
            assert!(!k.key.is_empty(), "{action} has a key");
            assert!(k.menu.is_some_and(|m| m.starts_with("Post › ")), "{action}");
        }
    }

    #[test]
    fn your_row_offers_no_ai_reply_and_fork_follows_its_switch() {
        let ids = own_action_ids(vm::CURRENT_ACTIONS.to_vec());
        assert!(!ids.contains(&"AI reply"));
        assert!(ids.contains(&"Reply") && ids.contains(&"Quote") && ids.contains(&"Link post"));
        let pinned = own_action_ids(vm::PINNED_ACTIONS.to_vec());
        assert_eq!(ids.contains(&"Fork"), FORK_OWN);
        assert_eq!(pinned.contains(&"Fork this pin"), FORK_OWN);
        assert_eq!(post_actions().iter().any(|(id, _)| *id == "Fork"), FORK_OWN);
    }

    #[test]
    fn citations_spell_the_origin_with_a_slash() {
        assert_eq!(
            citation_origin("https://blyg.example.com"),
            "https://blyg.example.com/"
        );
        assert_eq!(
            citation_origin("https://blyg.example.com/m/"),
            "https://blyg.example.com/m/"
        );
    }
}
