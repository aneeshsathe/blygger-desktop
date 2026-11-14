//! --- reader view --- "Open posts on the author's site"
//! (`open-posts-on-site`, off by default).
//!
//! With it on, an opened post shows its own public page (its `page`), as
//! its author styled it, where Burrow's reader would show it: the same
//! place in the reading layout (the list or stream beside it), with the
//! post's header, version pill, notes and action row around it as before.
//!
//! Why its own web view and not the browser pane: the reader pane's web view
//! has a bridge into the app (quotes, links, selection) and shows only
//! sanitized HTML, so a live page can't go there; the browser pane is a
//! slide-over with its own chrome, keyboard and history, made for wandering
//! off. A post's page is a step of reading, so it gets a web view of the
//! browser's own kind (`browser::surface`: no bridge, http(s) only, the
//! browser's cookie jar, block lists and shields) placed in the reader's
//! slot. Links clicked in it stay in it; ⇧⌘B and the browser pane are
//! untouched.
//!
//! - j/k and ↑/↓ keep moving through posts: the view never takes the keyboard
//!   on load, and when a click gives it the keyboard outside a text field it
//!   is handed back to the list (as a click in Burrow's reader does).
//!   Space pages down and, at the end, opens the next unread post.
//! - The toggle above the post switches this one view between "Burrow
//!   reader" and "Author's site". A subscription's menu can always or never
//!   open its posts this way (`state.json`, `author_site_subs`).
//! - A post with no page of its own opens in Burrow's reader, and a quiet
//!   line says so. A pinned version, a diff or a withdrawn post is Burrow's.
//! - It counts as read exactly as before (opening marks it read).
//! - Elsewhere than macOS there is no web view to embed: the page opens in
//!   the default browser when a post is opened (not while moving with the
//!   keys), and Burrow's reader shows the post here.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use async_channel::{Receiver, Sender};
use blyg_core::ReadingItem;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::View;
use super::vm::Key;
use crate::app::MainView;
use crate::app::browser::surface::{self, BrowserEvent, BrowserSurface};

/// How long a site view no longer shown keeps its web view (and page).
pub const TEARDOWN_AFTER: Duration = Duration::from_secs(120);
/// How often the keyboard is checked while the view is up.
const KEYS_EVERY: Duration = Duration::from_millis(400);

/// The quiet line when a post would open on its site but has no page.
pub const NO_PAGE: &str = "No page of its own · shown in Burrow's reader";

/// What an opened post shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Shows {
    /// Burrow's reader (the setting is off, or this view says so).
    Reader,
    /// Its own page, at this URL.
    Site(String),
    /// It would open on its site, but it has no page: Burrow's reader.
    NoPage,
}

/// The post's own page: its declared `page`, made absolute against its
/// origin; http(s) only (`vm::page_url`, which the web app shares).
pub fn page_url(r: &ReadingItem) -> Option<String> {
    super::vm::page_url(r)
}

/// This view's choice beats the subscription's, which beats the setting.
pub fn wants_site(setting: bool, sub: Option<bool>, item: Option<bool>) -> bool {
    item.or(sub).unwrap_or(setting)
}

/// What `r` shows when its site is (or isn't) wanted.
pub fn shows(r: &ReadingItem, want: bool) -> Shows {
    if !want || r.state == "tombstone" {
        return Shows::Reader;
    }
    match page_url(r) {
        Some(u) => Shows::Site(u),
        None => Shows::NoPage,
    }
}

/// Whether this build can show a page in the reading layout.
pub fn embeddable() -> bool {
    cfg!(any(target_os = "macos", test))
}

/// The page's address for the strip above it: host and path.
pub fn strip_label(url: &str) -> String {
    let t = url
        .trim_start_matches("https://")
        .trim_start_matches("http://");
    t.trim_end_matches('/').to_string()
}

/// The script that pages down (`true`: it was at the end already).
pub const PAGE_DOWN_JS: &str = "(function(){var e=document.scrollingElement||document.documentElement;\
var end=e.scrollTop+window.innerHeight>=e.scrollHeight-4;\
if(!end)window.scrollBy({top:Math.round(window.innerHeight*0.88),behavior:'smooth'});\
return JSON.stringify(end);})()";

/// A small scroll for ↑/↓.
pub fn nudge_js(down: bool) -> String {
    format!(
        "(function(){{window.scrollBy({{top:{},behavior:'smooth'}});return 'null';}})()",
        if down { 90 } else { -90 }
    )
}

/// Whether the page's focus is a place to type (then it keeps the keys).
pub const EDITING_JS: &str = "(function(){var a=document.activeElement;\
return JSON.stringify(!!a&&(a.isContentEditable||/^(INPUT|TEXTAREA|SELECT)$/.test(a.tagName)));})()";

/// The web view and where it was last put, shared with the canvas.
#[derive(Default)]
struct Placed {
    surface: Option<Box<dyn BrowserSurface>>,
    frame: Option<Bounds<Pixels>>,
    visible: bool,
    suppressed: bool,
    /// The browser pane's or the notes drawer's left edge: cut off there.
    clip_right: Option<Pixels>,
}

impl Placed {
    fn place(&mut self, bounds: Bounds<Pixels>) {
        let Some(s) = self.surface.as_mut() else {
            return;
        };
        let mut bounds = bounds;
        if let Some(edge) = self.clip_right {
            bounds.size.width = bounds.size.width.min(edge - bounds.origin.x);
            if bounds.size.width < px(40.) {
                if self.visible {
                    s.set_visible(false);
                    self.visible = false;
                }
                return;
            }
        }
        if self.frame != Some(bounds) {
            s.set_frame(bounds);
            self.frame = Some(bounds);
        }
        if !self.visible && !self.suppressed {
            s.set_visible(true);
            self.visible = true;
        }
    }

    fn hide(&mut self) {
        if let (Some(s), true) = (self.surface.as_mut(), self.visible) {
            s.set_visible(false);
            self.visible = false;
        }
    }
}

/// The reader's author's-site view (inside `reading::State`).
pub struct SiteView {
    placed: Rc<RefCell<Placed>>,
    events: Sender<BrowserEvent>,
    events_rx: Option<Receiver<BrowserEvent>>,
    /// Why there's no web view (Burrow's reader is shown instead).
    pub failed: Option<String>,
    /// The URL loaded in the view.
    loaded: Option<String>,
    pub loading: bool,
    /// This one view's choice (the toggle above the post), for its post.
    pub item: Option<(Key, bool)>,
    /// Per subscription: always (`true`) or never (`false`).
    pub subs: BTreeMap<String, bool>,
    data_dir: Option<PathBuf>,
    generation: u64,
    teardown: Option<Task<()>>,
    keys: Option<Task<()>>,
    dark: Option<bool>,
    /// Moving with the keys: pages aren't handed to the default browser.
    pub moving: bool,
    /// URLs loaded (tests).
    #[cfg(test)]
    pub loads: Vec<String>,
}

impl SiteView {
    pub fn new(data_dir: Option<PathBuf>) -> SiteView {
        let (tx, rx) = async_channel::unbounded();
        let subs = data_dir
            .as_deref()
            .map(blyg_core::state::AppState::load)
            .map(|s| s.author_site_subs)
            .unwrap_or_default();
        SiteView {
            placed: Rc::default(),
            events: tx,
            events_rx: Some(rx),
            failed: None,
            loaded: None,
            loading: false,
            item: None,
            subs,
            data_dir,
            generation: 0,
            teardown: None,
            keys: None,
            dark: None,
            moving: false,
            #[cfg(test)]
            loads: Vec::new(),
        }
    }

    /// The web view exists.
    pub fn alive(&self) -> bool {
        self.placed.borrow().surface.is_some()
    }

    /// On screen (tests).
    #[cfg(test)]
    pub fn visible(&self) -> bool {
        self.placed.borrow().visible
    }

    fn with<R>(&self, f: impl FnOnce(&mut dyn BrowserSurface) -> R) -> Option<R> {
        self.placed
            .borrow_mut()
            .surface
            .as_mut()
            .map(|s| f(s.as_mut()))
    }
}

impl MainView {
    /// What the open post shows, as the setting, its subscription and this
    /// view's toggle decide.
    pub(crate) fn site_shows(&self, o: &super::Opened) -> Shows {
        let s = &self.reading.site;
        let item = s
            .item
            .as_ref()
            .filter(|(k, _)| *k == o.key)
            .map(|(_, on)| *on);
        let sub = s.subs.get(&o.item.subscription_id).copied();
        shows(&o.item, wants_site(self.prefs.author_site, sub, item))
    }

    /// The page the reading layout shows instead of Burrow's reader, if any.
    fn site_target(&self) -> Option<String> {
        if self.reading.view != View::Reading || !embeddable() {
            return None;
        }
        let o = self.reading.opened.as_ref()?;
        if o.pinned_on_screen().is_some() || o.diff_vs_now {
            return None;
        }
        match self.site_shows(o) {
            Shows::Site(u) => Some(u),
            _ => None,
        }
    }

    /// The open post shows its own page (not Burrow's reader).
    pub(crate) fn site_view_shown(&self) -> bool {
        self.reading.site.failed.is_none() && self.site_target().is_some()
    }

    /// Hook (`open_reading`): a new post is open. This view's toggle is
    /// for the post it was used on only; elsewhere than macOS, a post that
    /// opens on its site is handed to the default browser.
    pub(crate) fn site_opened(&mut self, cx: &mut Context<Self>) {
        let Some(o) = self.reading.opened.as_ref() else {
            return;
        };
        if self
            .reading
            .site
            .item
            .as_ref()
            .is_some_and(|(k, _)| *k != o.key)
        {
            self.reading.site.item = None;
        }
        if embeddable() || std::mem::take(&mut self.reading.site.moving) {
            return;
        }
        if let Shows::Site(url) = self.site_shows(o) {
            cx.open_url(&url);
            self.show_toast(
                "Opening the author's page in your browser…",
                Some("Burrow's reader shows it here".into()),
                cx,
            );
        }
    }

    /// The toggle above the post: this one view, Burrow's reader or the
    /// author's site.
    pub(crate) fn set_site_for_view(&mut self, on: bool, cx: &mut Context<Self>) {
        let Some(o) = self.reading.opened.as_ref() else {
            return;
        };
        let key = o.key.clone();
        let url = page_url(&o.item);
        self.reading.site.item = Some((key, on));
        if on
            && !embeddable()
            && let Some(u) = url
        {
            cx.open_url(&u);
            self.show_toast("Opening the author's page in your browser…", None, cx);
        }
        cx.notify();
    }

    /// A subscription's menu: always (`Some(true)`), never (`Some(false)`)
    /// or as Settings says (`None`). Kept in `state.json`.
    pub(crate) fn set_site_for_sub(
        &mut self,
        sub: String,
        on: Option<bool>,
        cx: &mut Context<Self>,
    ) {
        match on {
            Some(v) => self.reading.site.subs.insert(sub.clone(), v),
            None => self.reading.site.subs.remove(&sub),
        };
        if let Some(dir) = self.reading.site.data_dir.clone() {
            let subs = self.reading.site.subs.clone();
            let _ = blyg_core::state::AppState::update(&dir, |s| s.author_site_subs = subs);
        }
        // The open post follows its subscription's new choice.
        if self
            .reading
            .opened
            .as_ref()
            .is_some_and(|o| o.item.subscription_id == sub)
        {
            self.reading.site.item = None;
        }
        cx.notify();
    }

    /// Per frame, before layout (after `browser_frame`): make the web view
    /// when a post first shows its page, load a new page when the post
    /// changes, and hide it whenever it isn't on screen or something GPUI
    /// draws would be under it.
    pub(crate) fn site_frame(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let want = self.site_target();
        if want.is_some() && self.reading.site.failed.is_none() && !self.reading.site.alive() {
            self.site_create(window, cx);
        }
        if let Some(url) = want.as_ref()
            && self.reading.site.alive()
            && self.reading.site.loaded.as_ref() != Some(url)
        {
            #[cfg(test)]
            self.reading.site.loads.push(url.clone());
            self.reading.site.with(|s| s.load_url(url));
            self.reading.site.loaded = Some(url.clone());
            self.reading.site.loading = true;
        }
        let dark = self.palette.dark;
        if self.reading.site.dark != Some(dark) && self.reading.site.alive() {
            self.reading.site.dark = Some(dark);
            self.reading.site.with(|s| s.set_dark(dark));
        }
        let dropdown = self.reading.opened.as_ref().is_some_and(|o| o.dropdown);
        // As the reader's own web view: every GPUI overlay that can sit over
        // the body hides it (a native view draws above all GPUI content).
        let covered = self.sheet.is_some()
            || self.ai.has_overlay()
            || self.ext.has_overlay()
            || self.reading.sheet.is_some()
            || dropdown
            || self.onboarding.flow.is_some()
            || self.tutorial_hides_reader(window)
            || self.profile_sheet_open()
            || self.reading.src_menu.is_some();
        let showing = want.is_some() && !covered;
        // The browser pane and the notes drawer cut it off at their edge.
        let vw = window.viewport_size().width;
        let room = self.side_room();
        let pane = self.browser.open.then(|| self.browser.left_edge(vw - room));
        let notes = self.notes.drawn().then(|| (vw - room).max(px(0.)));
        let edge = match (pane, notes) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
        {
            let mut p = self.reading.site.placed.borrow_mut();
            if p.clip_right != edge {
                p.clip_right = edge;
                p.frame = None;
            }
            p.suppressed = !showing;
            if !showing {
                p.hide();
            }
        }
        // Not shown: drop the web view (and its process) after a while.
        if want.is_some() {
            self.reading.site.teardown = None;
        } else if self.reading.site.alive() && self.reading.site.teardown.is_none() {
            self.reading.site.generation += 1;
            let gen_ = self.reading.site.generation;
            self.reading.site.teardown = Some(cx.spawn(async move |this, cx| {
                cx.background_executor().timer(TEARDOWN_AFTER).await;
                let _ = this.update(cx, |v, _| {
                    if v.reading.site.generation == gen_ && v.site_target().is_none() {
                        v.reading.site.placed.borrow_mut().surface = None;
                        v.reading.site.placed.borrow_mut().frame = None;
                        v.reading.site.loaded = None;
                        v.reading.site.keys = None;
                        v.reading.site.teardown = None;
                    }
                });
            }));
        }
    }

    fn site_create(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.browser_start_blocker(cx);
        let factory = cx
            .try_global::<surface::FactoryGlobal>()
            .map(|g| g.0.clone())
            .unwrap_or_else(surface::default_factory);
        let (rules, blocking) = self.browser.blocking_for();
        let s = &mut self.reading.site;
        match factory(window, s.events.clone(), rules, blocking) {
            Ok(view) => {
                let mut p = s.placed.borrow_mut();
                p.surface = Some(view);
                p.frame = None;
                p.visible = false;
                s.loaded = None;
                s.dark = None;
            }
            Err(msg) => {
                eprintln!("blygger: {msg}");
                s.failed = Some(msg);
                return;
            }
        }
        if let Some(rx) = s.events_rx.take() {
            self._tasks.push(cx.spawn_in(window, async move |this, cx| {
                while let Ok(ev) = rx.recv().await {
                    if this
                        .update_in(cx, |v, window, cx| v.site_event(ev, window, cx))
                        .is_err()
                    {
                        break;
                    }
                }
            }));
        }
        self.site_watch_keys(window, cx);
    }

    fn site_event(&mut self, ev: BrowserEvent, _window: &mut Window, cx: &mut Context<Self>) {
        match ev {
            BrowserEvent::Changed => {
                let loading = self.reading.site.with(|s| s.state().loading);
                if let Some(l) = loading
                    && l != self.reading.site.loading
                {
                    self.reading.site.loading = l;
                    cx.notify();
                }
            }
            BrowserEvent::OpenExternally(url) => cx.open_url(&url),
            // `target=_blank`: here, as the browser pane does.
            BrowserEvent::NewWindow(url) => {
                self.reading.site.with(|s| s.load_url(&url));
            }
            BrowserEvent::Crashed => {
                if let Some(url) = self.reading.site.loaded.clone()
                    && self.site_target().is_some()
                {
                    self.reading.site.with(|s| s.load_url(&url));
                }
            }
        }
    }

    /// While the view is up: a click that gave the page the keyboard outside
    /// a text field hands it back to the list, so j/k keep moving.
    fn site_watch_keys(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.reading.site.keys.is_some() {
            return;
        }
        self.reading.site.keys = Some(cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor().timer(KEYS_EVERY).await;
                let ask = this.update(cx, |v, _| {
                    if !v.reading.site.alive() {
                        return None;
                    }
                    let page = v.site_view_shown()
                        && v.reading
                            .site
                            .with(|s| s.page_has_keyboard())
                            .unwrap_or(false);
                    if !page {
                        return Some(None);
                    }
                    let (tx, rx) = async_channel::bounded::<String>(1);
                    v.reading.site.with(|s| s.eval_json(EDITING_JS, tx));
                    Some(Some(rx))
                });
                let rx = match ask {
                    Ok(Some(rx)) => rx,
                    _ => break,
                };
                let Some(rx) = rx else { continue };
                let editing = rx.recv().await.unwrap_or_default() == "true";
                if editing {
                    continue;
                }
                let _ = this.update_in(cx, |v, window, cx| {
                    v.reading.site.with(|s| s.focus_parent());
                    v.reading.pane = super::sources_vm::Pane::Post;
                    window.focus(&v.reading.focus, cx);
                    cx.notify();
                });
            }
        }));
    }

    /// ↑/↓ in the post pane: scroll the page a little. False when no page
    /// is shown.
    pub(crate) fn site_nudge(&mut self, down: bool) -> bool {
        if !self.site_view_shown() || !self.reading.site.alive() {
            return false;
        }
        let (tx, _rx) = async_channel::bounded::<String>(1);
        self.reading.site.with(|s| s.eval_json(&nudge_js(down), tx));
        true
    }

    /// Space: page down; at the page's end, the next post to read. False
    /// when no page is shown.
    pub(crate) fn site_page_down(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if !self.site_view_shown() || !self.reading.site.alive() {
            return false;
        }
        let (tx, rx) = async_channel::bounded::<String>(1);
        self.reading.site.with(|s| s.eval_json(PAGE_DOWN_JS, tx));
        cx.spawn_in(window, async move |this, cx| {
            if rx.recv().await.unwrap_or_default() == "true" {
                let _ = this.update_in(cx, |v, window, cx| v.open_next_unread(window, cx));
            }
        })
        .detach();
        true
    }

    /// The body when the post shows its page: a strip naming it (and
    /// opening it in the default browser), then the web view.
    pub(crate) fn render_site_pane(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette.on_page();
        let url = self.site_target().unwrap_or_default();
        let open = url.clone();
        let placed = self.reading.site.placed.clone();
        let strip = div()
            .flex_none()
            .px(px(32.))
            .py(px(4.))
            .flex()
            .items_center()
            .gap(px(8.))
            .font_family(self.chrome())
            .text_size(px(11.))
            .text_color(p.muted)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .child(format!("{} · as its author styled it", strip_label(&url))),
            )
            .when(self.reading.site.loading, |d| d.child("loading…"))
            .child(
                div()
                    .id("site-open-browser")
                    .debug_selector(|| "site-open-browser".into())
                    .cursor_pointer()
                    .hover(|s| s.text_color(p.accent))
                    .child("Open in browser ↗")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.open_url(&open);
                        this.show_toast("Opening in your browser…", None, cx);
                    })),
            );
        div()
            .id("site-body")
            .flex_1()
            .min_h_0()
            .min_w_0()
            .w_full()
            .flex()
            .flex_col()
            .child(strip)
            .child(
                div().flex_1().min_h_0().bg(p.bg).child(
                    canvas(
                        |bounds, _, _| bounds,
                        move |_, bounds, _, _| placed.borrow_mut().place(bounds),
                    )
                    .size_full(),
                ),
            )
            .into_any_element()
    }

    /// Above the post: "Burrow reader | Author's site" for a post with a
    /// page of its own; the quiet line when it has none but would open on
    /// its site.
    pub(crate) fn render_site_toggle(
        &self,
        o: &super::Opened,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if o.item.state == "tombstone" || o.pinned_on_screen().is_some() {
            return None;
        }
        let p = self.palette.on_page();
        let shows = self.site_shows(o);
        if shows == Shows::NoPage {
            return Some(
                div()
                    .id("site-no-page")
                    .debug_selector(|| "site-no-page".into())
                    .font_family(self.chrome())
                    .text_size(px(11.))
                    .text_color(p.muted)
                    .italic()
                    .child(NO_PAGE)
                    .into_any_element(),
            );
        }
        page_url(&o.item)?;
        let on = matches!(shows, Shows::Site(_)) && embeddable();
        let seg = |id: &'static str, label: &'static str, active: bool| {
            div()
                .id(id)
                .debug_selector(move || id.into())
                .px(px(7.))
                .py(px(1.))
                .rounded(px(self.theme.corner(5.)))
                .cursor_pointer()
                .when(active, |d| d.bg(p.hover()).text_color(p.ink))
                .when(!active, |d| d.hover(|s| s.text_color(p.ink)))
                .child(label)
        };
        Some(
            div()
                .flex()
                .items_center()
                .gap(px(2.))
                .px(px(2.))
                .py(px(1.))
                .rounded(px(self.theme.corner(6.)))
                .border_1()
                .border_color(p.line)
                .font_family(self.chrome())
                .text_size(px(11.))
                .text_color(p.muted)
                .child(
                    seg("site-reader", "Burrow reader", !on)
                        .on_click(cx.listener(|this, _, _, cx| this.set_site_for_view(false, cx))),
                )
                .child(
                    seg("site-author", "Author's site", on)
                        .on_click(cx.listener(|this, _, _, cx| this.set_site_for_view(true, cx))),
                )
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{Shows, page_url, shows, strip_label, wants_site};
    use blyg_core::ReadingItem;

    fn item(origin: &str, page: Option<&str>) -> ReadingItem {
        ReadingItem {
            subscription_id: "sub-1".into(),
            remote_id: "01abc".into(),
            subscription_title: "Ada".into(),
            origin: origin.into(),
            kind: blyg_core::Kind::Fragment,
            state: "current".into(),
            version: 1,
            created: None,
            updated: None,
            observed_at: "2026-11-01T00:00:00Z".into(),
            content_md: "Tide tables.".into(),
            content_html: String::new(),
            author: None,
            page: page.map(str::to_string),
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

    #[test]
    fn the_page_is_the_declared_one_made_absolute() {
        let r = item("https://ada.blyg.example.com/", Some("f/01abc"));
        assert_eq!(
            page_url(&r).as_deref(),
            Some("https://ada.blyg.example.com/f/01abc")
        );
        let r = item("https://ada.blyg.example.com/blyg/", Some("/t/01abc/"));
        assert_eq!(
            page_url(&r).as_deref(),
            Some("https://ada.blyg.example.com/blyg/t/01abc/")
        );
        let r = item(
            "https://feed.example.org/rss",
            Some("https://feed.example.org/2026/tides"),
        );
        assert_eq!(
            page_url(&r).as_deref(),
            Some("https://feed.example.org/2026/tides")
        );
        assert_eq!(page_url(&item("https://a.example.com/", None)), None);
        assert_eq!(page_url(&item("https://a.example.com/", Some("  "))), None);
        assert_eq!(
            page_url(&item("https://a.example.com/", Some("javascript:alert(1)"))),
            None,
            "http(s) only, whatever the page says"
        );
        assert_eq!(
            page_url(&item("file:///tmp/", Some("x"))),
            None,
            "http(s) only"
        );
    }

    #[test]
    fn the_view_beats_the_subscription_which_beats_the_setting() {
        assert!(!wants_site(false, None, None), "off by default");
        assert!(wants_site(true, None, None));
        assert!(!wants_site(true, Some(false), None));
        assert!(wants_site(false, Some(true), None));
        assert!(!wants_site(true, Some(true), Some(false)));
        assert!(wants_site(false, Some(false), Some(true)));
    }

    #[test]
    fn no_page_falls_back_quietly_and_a_withdrawn_post_is_burrows() {
        let r = item("https://ada.blyg.example.com/", Some("f/01abc"));
        assert_eq!(shows(&r, false), Shows::Reader);
        assert_eq!(
            shows(&r, true),
            Shows::Site("https://ada.blyg.example.com/f/01abc".into())
        );
        assert_eq!(
            shows(&item("https://ada.blyg.example.com/", None), true),
            Shows::NoPage
        );
        let mut gone = r.clone();
        gone.state = "tombstone".into();
        assert_eq!(shows(&gone, true), Shows::Reader);
        assert_eq!(
            strip_label("https://ada.blyg.example.com/f/01abc/"),
            "ada.blyg.example.com/f/01abc"
        );
    }
}
