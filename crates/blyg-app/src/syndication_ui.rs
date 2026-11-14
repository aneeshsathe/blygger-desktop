//! --- syndication --- Copies elsewhere (decision #98, protocol 0.4
//! §16.14; `blyg_core::syndication`).
//!
//! - **Your published post** (Post › Copies Elsewhere…, and a chip in ⌘Y):
//!   a sheet listing where you've posted it elsewhere, each copy with its
//!   host, address and the version it was made from. Paste an `https:`
//!   address to add one (the version defaults to the current one), edit or
//!   remove it. The list is written to the blyg in place: never a new
//!   version, nothing in the feed.
//! - **After a cross-post** (a browser macro on a published post): the
//!   pane's bar offers "＋ Record copy" until the copy is recorded; it takes
//!   the page on screen when it's the copy (`blyg_ext_crosspost::copy`).
//! - **Reading** someone's post: their entries as plain links under the
//!   header, "Also posted at host", as the author's claim: nothing is
//!   fetched from the copies, nothing is folded or hidden, and no count is
//!   shown.
//!
//! A child module of `app` (it reads `MainView`); hooks elsewhere are
//! marked `// --- syndication ---`.

use blyg_core::syndication::{self as synd, SyndicationEntry};
use blyg_core::{ReadingItem, Status};

use super::*;

gpui_kit::actions!(
    blygger,
    [
        /// Post › Copies Elsewhere…: where this published post has been
        /// posted on other networks.
        ShowCopiesElsewhere,
    ]
);

/// What the sheet says about the list.
pub const SHEET_NOTE: &str = "Where you've posted this elsewhere. It's listed on the post as \
     your claim; changing it never publishes a new version.";

/// Said when the blyg doesn't keep the list (yet).
pub const NOT_SERVED_NOTE: &str =
    "Your blyg doesn't keep this list yet (it needs a newer studio): it stays on this Mac.";

/// The reader's label.
pub const ALSO_POSTED: &str = "Also posted at";

/// Its tooltip.
pub const ALSO_POSTED_TIP: &str =
    "Where the author says they posted this too. Burrow doesn't check or fetch the copies.";

/// The sheet.
pub(crate) struct CopiesSheet {
    pub id: LocalId,
    pub title: String,
    /// The post's current version: what a new entry defaults to.
    pub current: u32,
    pub entries: Vec<SyndicationEntry>,
    pub url: Entity<InputState>,
    pub version: Entity<InputState>,
    /// The address of the entry being edited (`None`: adding).
    pub editing: Option<String>,
    pub error: Option<String>,
    pub served: bool,
}

/// A copy recorded right after a cross-post: the post and the run's site.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingCopy {
    pub id: LocalId,
    pub version: u32,
    pub title: String,
    pub ext: String,
    pub site_title: String,
    pub origin: String,
    pub home: String,
}

impl PendingCopy {
    /// The page `url` as the copy to record, or why not.
    pub fn check(&self, url: &str) -> Result<String, &'static str> {
        let u = blyg_ext_crosspost::copy::copy_url(&self.ext, &self.origin, Some(&self.home), url)?;
        synd::check_url(&u)
    }
}

/// An entry as typed: the address checked, the version a number ≥ 1
/// (blank: `current`).
pub fn parse_entry(
    url: &str,
    version: &str,
    current: u32,
) -> std::result::Result<SyndicationEntry, String> {
    let url = synd::check_url(url).map_err(str::to_string)?;
    let v = version.trim().trim_start_matches(['v', 'V']);
    let version = if v.is_empty() {
        current.max(1)
    } else {
        match v.parse::<u32>() {
            Ok(n) if n >= 1 && n <= current.max(1) => n,
            _ => {
                return Err(format!(
                    "The version is a number from 1 to {}",
                    current.max(1)
                ));
            }
        }
    };
    Ok(SyndicationEntry::new(url, version))
}

/// The entries a reader may show as links: `https:` only (§16.14 rule 5).
pub fn shown_links(entries: &[SyndicationEntry]) -> Vec<(String, String)> {
    entries
        .iter()
        .filter(|e| synd::is_https(&e.url))
        .map(|e| (e.host(), e.url.clone()))
        .filter(|(h, _)| !h.is_empty())
        .collect()
}

impl MainView {
    /// Hook: the actions, on the main window's root.
    pub(super) fn syndication_actions(
        &self,
        d: Stateful<Div>,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        d.on_action(cx.listener(|this, _: &ShowCopiesElsewhere, window, cx| {
            this.show_copies_for_current(window, cx)
        }))
    }

    fn show_copies_for_current(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if matches!(self.sheet, Some(Sheet::Copies(_))) {
            return self.close_sheet(window, cx);
        }
        let Some(id) = self.current.as_ref().map(|c| c.local_id.clone()) else {
            return self.show_toast(
                "Open a published post to list its copies elsewhere",
                None,
                cx,
            );
        };
        self.show_copies(id, window, cx);
    }

    /// Open the Copies elsewhere sheet for `id` (a published post).
    pub(crate) fn show_copies(&mut self, id: LocalId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(item) = self.backend.item(&id) else {
            return;
        };
        if item.status != Status::Public || item.server_id.is_none() {
            return self.show_toast(
                "Publish it first to list its copies elsewhere",
                Some("Only a published post has copies to point at".into()),
                cx,
            );
        }
        let url =
            cx.new(|cx| InputState::new(window, cx).placeholder("https://… the copy's address"));
        let version =
            cx.new(|cx| InputState::new(window, cx).placeholder(format!("v{}", item.version)));
        for input in [&url, &version] {
            let sub = cx.subscribe_in(input, window, |this, _, ev, window, cx| {
                if let InputEvent::PressEnter { .. } = ev {
                    this.copies_commit(window, cx);
                }
            });
            self._subs.push(sub);
        }
        url.update(cx, |s, cx| s.focus(window, cx));
        self.sheet_gen += 1;
        self.reload_copies(id.clone(), cx);
        self.sheet = Some(Sheet::Copies(Box::new(CopiesSheet {
            entries: self.backend.syndication(&id),
            id,
            title: vm::truncate_chars(&item.title(), 48),
            current: item.version,
            url,
            version,
            editing: None,
            error: None,
            served: self.backend.syndication_served(),
        })));
        cx.notify();
    }

    /// Read the post's list from the blyg again (in the background); the
    /// sheet shows it if it's still open on that post. An edit made here
    /// meanwhile wins (the store keeps a waiting edit over what's read).
    fn reload_copies(&mut self, id: LocalId, cx: &mut Context<Self>) {
        let backend = self.backend.clone();
        let rid = id.clone();
        let task = cx.background_spawn(async move {
            let r = backend.refresh_syndication(&rid);
            (
                r.err(),
                backend.syndication(&rid),
                backend.syndication_served(),
            )
        });
        cx.spawn(async move |this, cx| {
            let (err, list, served) = task.await;
            let _ = this.update(cx, |v, cx| {
                if let Some(Sheet::Copies(s)) = v.sheet.as_mut()
                    && s.id == id
                {
                    s.entries = list;
                    s.served = served;
                    if let Some(e) = err.filter(|_| served) {
                        s.error = Some(format!("Couldn't read the list from your blyg: {e}"));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// ⏎ or Add/Save: add the typed entry, or replace the one being edited.
    pub(crate) fn copies_commit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(Sheet::Copies(s)) = self.sheet.as_ref() else {
            return;
        };
        let url = s.url.read(cx).value().to_string();
        let version = s.version.read(cx).value().to_string();
        let entry = match parse_entry(&url, &version, s.current) {
            Ok(e) => e,
            Err(why) => {
                if let Some(Sheet::Copies(s)) = self.sheet.as_mut() {
                    s.error = Some(why);
                }
                return cx.notify();
            }
        };
        let list = synd::edit(&s.entries, s.editing.as_deref(), Some(entry));
        self.copies_write(list, window, cx);
    }

    /// Remove the entry for `url`.
    pub(crate) fn copies_remove(&mut self, url: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(Sheet::Copies(s)) = self.sheet.as_ref() else {
            return;
        };
        let list = synd::edit(&s.entries, Some(url), None);
        self.copies_write(list, window, cx);
    }

    /// Put the entry for `url` in the fields to change it.
    pub(crate) fn copies_edit(&mut self, url: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(Sheet::Copies(s)) = self.sheet.as_mut() else {
            return;
        };
        let Some(e) = s.entries.iter().find(|e| e.url == url).cloned() else {
            return;
        };
        s.editing = Some(e.url.clone());
        s.error = None;
        let (u, v) = (s.url.clone(), s.version.clone());
        u.update(cx, |st, cx| {
            st.set_value(e.url.clone(), window, cx);
            st.focus(window, cx);
        });
        v.update(cx, |st, cx| st.set_value(e.version.to_string(), window, cx));
        cx.notify();
    }

    fn copies_write(
        &mut self,
        list: Vec<SyndicationEntry>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(Sheet::Copies(s)) = self.sheet.as_ref() else {
            return;
        };
        let id = s.id.clone();
        match self.backend.set_syndication(&id, &list) {
            Ok(kept) => {
                if let Some(Sheet::Copies(s)) = self.sheet.as_mut() {
                    s.entries = kept;
                    s.editing = None;
                    s.error = None;
                    let (u, v) = (s.url.clone(), s.version.clone());
                    u.update(cx, |st, cx| {
                        st.set_value("", window, cx);
                        st.focus(window, cx);
                    });
                    v.update(cx, |st, cx| st.set_value("", window, cx));
                }
            }
            Err(e) => {
                if let Some(Sheet::Copies(s)) = self.sheet.as_mut() {
                    s.error = Some(e.to_string());
                }
            }
        }
        cx.notify();
    }

    pub(crate) fn render_copies_sheet(
        &self,
        s: &CopiesSheet,
        cx: &mut Context<Self>,
    ) -> (f32, AnyElement) {
        let p = self.palette;
        let rows = s.entries.iter().enumerate().map(|(i, e)| {
            let (edit_url, remove_url) = (e.url.clone(), e.url.clone());
            let editing = s.editing.as_deref() == Some(e.url.as_str());
            div()
                .flex()
                .items_center()
                .gap(px(8.))
                .py(px(5.))
                .rule_t(&p)
                .text_size(px(12.5))
                .child(
                    div()
                        .flex_none()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(if editing { p.accent } else { p.ink })
                        .child(e.host()),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .text_ellipsis()
                        .whitespace_nowrap()
                        .text_color(p.muted)
                        .child(e.url.clone()),
                )
                .child(
                    div()
                        .flex_none()
                        .text_color(p.muted)
                        .child(format!("v{}", e.version)),
                )
                .child(
                    self.upkeep_chip(format!("copy-edit-{i}"), "Edit", true)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.copies_edit(&edit_url, window, cx)
                        })),
                )
                .child(
                    self.upkeep_chip(format!("copy-remove-{i}"), "Remove", true)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.copies_remove(&remove_url, window, cx)
                        })),
                )
        });
        let field = |el: AnyElement| {
            crate::theme_ext::field_box(div(), &self.theme)
                .px(px(10.))
                .py(px(7.))
                .text_size(px(13.))
                .child(el)
        };
        let body = div()
            .capture_action(cx.listener(|this, _: &Escape, window, cx| {
                cx.stop_propagation();
                this.close_sheet(window, cx);
            }))
            .child(
                div()
                    .mb(px(6.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_size(px(13.))
                    .child(format!("Copies elsewhere · “{}”", s.title)),
            )
            .child(
                div()
                    .mb(px(8.))
                    .text_size(px(12.))
                    .line_height(relative(1.45))
                    .text_color(p.muted)
                    .child(SHEET_NOTE),
            )
            .when(!s.served, |d| {
                d.child(
                    div()
                        .mb(px(8.))
                        .px(px(10.))
                        .py(px(6.))
                        .rounded(px(self.theme.corner(7.)))
                        .text_size(px(12.))
                        .bg(p.notice_bg)
                        .text_color(p.notice)
                        .child(NOT_SERVED_NOTE),
                )
            })
            .when(s.entries.is_empty(), |d| {
                d.child(
                    div()
                        .py(px(6.))
                        .text_size(px(12.5))
                        .italic()
                        .text_color(p.muted)
                        .child("None listed yet."),
                )
            })
            .children(rows)
            .child(
                div()
                    .mt(px(10.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(div().flex_1().min_w_0().child(field(
                        gpui_kit::base::input::Input::new(&s.url).into_any_element(),
                    )))
                    .child(div().w(px(64.)).flex_none().child(field(
                        gpui_kit::base::input::Input::new(&s.version).into_any_element(),
                    )))
                    .child(
                        self.upkeep_chip(
                            "copy-commit",
                            if s.editing.is_some() { "Save" } else { "Add" },
                            true,
                        )
                        .on_click(
                            cx.listener(|this, _, window, cx| this.copies_commit(window, cx)),
                        ),
                    ),
            )
            .when_some(s.error.clone(), |d, e| {
                d.child(
                    div()
                        .mt(px(6.))
                        .text_size(px(11.5))
                        .text_color(p.over)
                        .child(e),
                )
            })
            .child(
                div()
                    .mt(px(10.))
                    .text_size(px(11.))
                    .text_color(p.muted)
                    .child(format!(
                        "https: addresses only · the version is the one the copy was made from \
                         (blank: v{}) · ⏎ {} · esc closes",
                        s.current,
                        if s.editing.is_some() { "saves" } else { "adds" }
                    )),
            );
        (520., body.into_any_element())
    }

    /// Hook (⌘Y's chips on your published post): "Copies elsewhere (n)…".
    pub(crate) fn syndication_chip(
        &self,
        id: &LocalId,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let item = self.backend.item(id)?;
        if item.status != Status::Public {
            return None;
        }
        let n = self.backend.syndication(id).len();
        let label = if n == 0 {
            "Copies elsewhere…".to_string()
        } else {
            format!("Copies elsewhere ({n})…")
        };
        let id = id.clone();
        Some(
            self.upkeep_chip("own-copies", label, true)
                .on_click(
                    cx.listener(move |this, _, window, cx| {
                        this.show_copies(id.clone(), window, cx)
                    }),
                )
                .into_any_element(),
        )
    }

    /// Hook (the reading pane's notes, under the header): an imported
    /// post's copies elsewhere as plain links. Nothing is fetched; nothing
    /// is folded or hidden.
    pub(crate) fn render_reading_copies(&self, item: &ReadingItem) -> Option<AnyElement> {
        if item.state == "tombstone" {
            return None;
        }
        let links = shown_links(
            &self
                .backend
                .reading_syndication(&item.subscription_id, &item.remote_id),
        );
        if links.is_empty() {
            return None;
        }
        let p = self.palette.on_page();
        let n = links.len();
        Some(
            div()
                .id("reading-copies")
                .mb(px(10.))
                .flex()
                .flex_wrap()
                .items_center()
                .gap(px(6.))
                .font_family(self.chrome())
                .text_size(px(11.5))
                .text_color(p.muted)
                .tooltip(|_, cx| cx.new(|_| reading::Tip(ALSO_POSTED_TIP.to_string())).into())
                .child(ALSO_POSTED)
                .children(links.into_iter().enumerate().map(move |(i, (host, url))| {
                    div()
                        .id(ElementId::Name(format!("reading-copy-{i}").into()))
                        .cursor_pointer()
                        .text_color(p.ink)
                        .hover(|s| s.text_color(p.accent))
                        .on_click(move |_, _, cx| cx.open_url(&url))
                        .child(if i + 1 < n {
                            format!("{host} ↗ ·")
                        } else {
                            format!("{host} ↗")
                        })
                }))
                .into_any_element(),
        )
    }

    // ------------------------------------------------------------ after a cross-post

    /// Hook (a macro on a published post finished, and may have posted):
    /// list the copy at once when the run read its link (`captured`) and
    /// it's the copy, else offer "＋ Record copy" in the pane's bar.
    /// Returns the host listed, if any.
    pub(crate) fn syndication_after_macro(
        &mut self,
        pending: Option<PendingCopy>,
        captured: Option<String>,
    ) -> Option<String> {
        self.browser.automation.copy = None;
        let pending = pending?;
        if let Some(url) = captured.and_then(|u| pending.check(&u).ok())
            && self.list_copy(&pending, &url).is_ok()
        {
            return Some(synd::host_of(&url));
        }
        self.browser.automation.copy = Some(pending);
        None
    }

    /// Add `url` to `pending`'s post at the cross-posted version.
    fn list_copy(&self, pending: &PendingCopy, url: &str) -> blyg_core::Result<()> {
        let list = synd::edit(
            &self.backend.syndication(&pending.id),
            Some(url),
            Some(SyndicationEntry::new(url, pending.version)),
        );
        self.backend.set_syndication(&pending.id, &list).map(|_| ())
    }

    /// Hook (the browser pane's bar): "＋ Record copy" while a cross-post's
    /// copy waits to be recorded.
    pub(crate) fn render_record_copy(
        &self,
        url: &str,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let pending = self.browser.automation.copy.as_ref()?;
        let p = self.palette;
        let check = pending.check(url);
        let tip: SharedString = match &check {
            Ok(_) => format!(
                "List this page on “{}” as its copy on {} (no new version)",
                pending.title, pending.site_title
            )
            .into(),
            Err(why) => (*why).into(),
        };
        let ok = check.is_ok();
        Some(
            div()
                .id("browser-record-copy")
                .flex_none()
                .px(px(8.))
                .py(px(2.))
                .rounded(px(5.))
                .text_size(px(12.))
                .when(ok, |d| {
                    d.cursor_pointer()
                        .text_color(p.accent)
                        .hover(|s| s.bg(p.pick()))
                })
                .when(!ok, |d| d.text_color(p.muted).opacity(0.7))
                .tooltip(move |_, cx| cx.new(|_| reading::Tip(tip.to_string())).into())
                .on_click(cx.listener(|this, _, window, cx| this.record_copy(window, cx)))
                .child("＋ Record copy")
                .into_any_element(),
        )
    }

    /// "＋ Record copy": the page on screen as the post's copy.
    pub(crate) fn record_copy(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(pending) = self.browser.automation.copy.clone() else {
            return;
        };
        let url = self.browser.page.url.clone();
        let url = match pending.check(&url) {
            Ok(u) => u,
            Err(why) => return self.show_toast(why, None, cx),
        };
        match self.list_copy(&pending, &url) {
            Ok(()) => {
                self.browser.automation.copy = None;
                self.show_toast(
                    format!(
                        "Listed on “{}”: its copy on {}",
                        pending.title,
                        synd::host_of(&url)
                    ),
                    Some("Post › Copies Elsewhere… changes it · no new version".into()),
                    cx,
                );
            }
            Err(e) => self.show_toast(format!("Couldn't list the copy: {e}"), None, cx),
        }
        cx.notify();
    }
}

#[cfg(test)]
mod unit_tests {
    use super::{PendingCopy, parse_entry, shown_links};
    use blyg_core::LocalId;
    use blyg_core::syndication::SyndicationEntry;

    #[test]
    fn a_typed_entry_is_https_and_a_published_version() {
        let e = parse_entry(" https://social.example.net/@me/1 ", "", 3).unwrap();
        assert_eq!(
            e,
            SyndicationEntry::new("https://social.example.net/@me/1", 3)
        );
        assert_eq!(
            parse_entry("https://a.example/1", "v2", 3).unwrap().version,
            2
        );
        assert!(parse_entry("http://a.example/1", "", 3).is_err());
        assert!(parse_entry("https://a.example/1", "4", 3).is_err());
        assert!(parse_entry("https://a.example/1", "0", 3).is_err());
        assert!(parse_entry("https://a.example/1", "two", 3).is_err());
    }

    #[test]
    fn readers_link_https_entries_only() {
        let got = shown_links(&[
            SyndicationEntry::new("https://www.social.example.net/@a/1", 1),
            SyndicationEntry::new("javascript:alert(1)", 1),
            SyndicationEntry::new("http://plain.example.net/1", 1),
        ]);
        assert_eq!(
            got,
            [(
                "social.example.net".to_string(),
                "https://www.social.example.net/@a/1".to_string()
            )]
        );
    }

    #[test]
    fn a_pending_copy_takes_the_note_page_only() {
        let c = PendingCopy {
            id: LocalId("l1".into()),
            version: 2,
            title: "Tides".into(),
            ext: blyg_ext_crosspost::NAME.into(),
            site_title: "Substack Notes".into(),
            origin: blyg_ext_crosspost::ORIGIN.into(),
            home: "https://substack.com/home".into(),
        };
        assert!(c.check("https://substack.com/home").is_err());
        assert_eq!(
            c.check("https://substack.com/@writer/note/c-42").unwrap(),
            "https://substack.com/@writer/note/c-42"
        );
    }
}
