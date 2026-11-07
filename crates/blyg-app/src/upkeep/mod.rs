//! --- upkeep --- Studio's maintenance surfaces, in Burrow (blygger-studio
//! 0.56's Updates tab, Refresh, Signals page and "see the change"):
//!
//! - **Stale quotes** (Blyg › Stale Quotes…, `stale.rs`): your threads whose
//!   quoted posts have newer versions, stalest first, each with what changed
//!   per quote; refresh one, or all at once. A quiet ↻ marks such a thread in
//!   the posts list, and `is:stale-quote` (a loose end) lists them.
//! - **Signals** (Blyg › Signals…, `signals.rs`): your likes, dislikes and
//!   private activity log. Never published, and never a count.
//! - **The change between versions** (⌘Y, `diff.rs`): a word diff of a
//!   version of your post against the one before it.
//! - **Alt text** for an image you paste or drop (`alt.rs`), and **Copy
//!   Link** (Post › Copy Link) for the post in front of you.
//!
//! A child module of `app` (it reads `MainView`); `app.rs` keeps small hooks
//! marked `// --- upkeep ---`.

use std::collections::HashSet;

use super::*;

pub(crate) mod alt;
pub(crate) mod diff;
pub(crate) mod signals;
pub(crate) mod stale;

pub(crate) use alt::AltSheet;
pub(crate) use signals::SignalsSheet;
pub(crate) use stale::StaleSheet;

gpui_kit::actions!(blygger, [ShowStaleQuotes, ShowSignals, CopyLink]);

/// The list's mark on a thread with a stale quote: quiet, like the
/// edited dot, never a number.
pub const STALE_GLYPH: &str = "↻";

/// Its tooltip.
pub const STALE_TIP: &str =
    "A quote in this thread has a newer version · Blyg › Stale Quotes… refreshes it";

/// Something asked of the blyg for a sheet.
#[derive(Debug, Clone, PartialEq)]
pub enum Fetch<T> {
    Loading,
    Ready(T),
    Failed(String),
}

impl<T> Fetch<T> {
    pub fn ready(&self) -> Option<&T> {
        match self {
            Fetch::Ready(t) => Some(t),
            _ => None,
        }
    }

    pub fn of(r: blyg_core::Result<T>) -> Self {
        match r {
            Ok(t) => Fetch::Ready(t),
            Err(e) => Fetch::Failed(e.to_string()),
        }
    }
}

/// What the main window keeps for these surfaces.
#[derive(Default)]
pub(crate) struct Upkeep {
    /// Server ids (lowercase) of your threads with a stale quote, as the
    /// blyg last said: the list's ↻.
    pub stale_ids: HashSet<String>,
    /// ⌘Y's word diff, when one is shown.
    pub diff: Option<diff::VersionDiff>,
}

/// The link for "Copy Link": your post's permalink, or a reading post's page.
pub fn link_of(permalink: Option<&str>) -> Option<String> {
    permalink
        .map(str::trim)
        .filter(|u| u.starts_with("https://") || u.starts_with("http://"))
        .map(str::to_string)
}

impl MainView {
    /// `BLYGGER_DEMO=upkeep-stale|upkeep-signals|upkeep-diff|upkeep-alt`
    /// (screenshots, no OS input): the steps call what a key would. True
    /// when the scenario is one of these.
    pub(crate) fn upkeep_demo_step(
        &mut self,
        scenario: &str,
        n: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        const FRICTION: &str = "01J9M2A";
        match (scenario, n) {
            ("upkeep-stale", 1) => self.toggle_stale_sheet(window, cx),
            ("upkeep-stale", 2) => self.probe_selected(cx),
            ("upkeep-signals", 1) => self.toggle_signals_sheet(window, cx),
            ("upkeep-activity", 1) => self.toggle_signals_sheet(window, cx),
            ("upkeep-activity", 2) => self.signals_tab(signals::Tab::Activity, cx),
            ("upkeep-diff" | "upkeep-alt", 0) => self.open(&LocalId(FRICTION.into()), window, cx),
            ("upkeep-diff", 1) => {
                window.dispatch_action(super::reading::ShowVersions.boxed_clone(), cx)
            }
            ("upkeep-diff", 2) => self.toggle_version_diff(LocalId(FRICTION.into()), 2, 3, cx),
            ("upkeep-alt", 1) => self.upkeep_ask_alt(
                LocalId(FRICTION.into()),
                "https://blyg.example.com/media/tide.png".into(),
                window,
                cx,
            ),
            (s, _) => return s.starts_with("upkeep-"),
        }
        true
    }
}

impl MainView {
    /// Hook: this module's actions on the main window.
    pub(super) fn upkeep_actions(&self, d: Stateful<Div>, cx: &mut Context<Self>) -> Stateful<Div> {
        d.on_action(
            cx.listener(|this, _: &ShowStaleQuotes, window, cx| {
                this.toggle_stale_sheet(window, cx)
            }),
        )
        .on_action(
            cx.listener(|this, _: &ShowSignals, window, cx| this.toggle_signals_sheet(window, cx)),
        )
        .on_action(cx.listener(|this, _: &CopyLink, _, cx| this.copy_link(cx)))
    }

    /// Hook (after the list re-queries): the stale threads as last heard.
    pub(super) fn upkeep_reload_stale(&mut self) {
        self.upkeep.stale_ids = self
            .backend
            .cached_stale_quotes()
            .into_iter()
            .map(|t| t.id.to_ascii_lowercase())
            .collect();
    }

    /// Whether `item` is one of your threads with a stale quote.
    pub(crate) fn has_stale_quote(&self, item: &Item) -> bool {
        !self.upkeep.stale_ids.is_empty()
            && item
                .server_id
                .as_ref()
                .is_some_and(|s| self.upkeep.stale_ids.contains(&s.0.to_ascii_lowercase()))
    }

    /// Hook (a posts-list row's second line): ↻ on a thread with a stale quote.
    pub(super) fn upkeep_row_marker(
        &self,
        item: &Item,
        ix: usize,
        color: Hsla,
    ) -> Option<AnyElement> {
        if !self.has_stale_quote(item) {
            return None;
        }
        Some(
            div()
                .id(("row-stale-quote", ix))
                .debug_selector(move || format!("row-stale-quote-{ix}"))
                .text_color(color)
                .tooltip(|_, cx| cx.new(|_| reading::Tip(STALE_TIP.into())).into())
                .child(STALE_GLYPH)
                .into_any_element(),
        )
    }

    /// A small chip for these sheets (`chip.radius`, `chip.border`); greyed
    /// when it can't be used now.
    pub(crate) fn upkeep_chip(
        &self,
        id: impl Into<SharedString>,
        label: impl Into<SharedString>,
        enabled: bool,
    ) -> Stateful<Div> {
        let p = self.palette;
        crate::theme_ext::chip(div().id(ElementId::Name(id.into())), &self.theme, false)
            .flex_none()
            .px(px(8.))
            .py(px(2.))
            .text_size(px(11.5))
            .font_weight(FontWeight::MEDIUM)
            .when(enabled, |d| {
                d.cursor_pointer()
                    .text_color(p.ink)
                    .hover(|s| s.border_color(p.accent))
            })
            .when(!enabled, |d| d.text_color(p.muted).opacity(0.6))
            .child(label.into())
    }

    /// A muted line in a sheet (loading, empty, failed).
    pub(crate) fn upkeep_note(&self, s: impl Into<SharedString>) -> AnyElement {
        div()
            .py(px(6.))
            .text_size(px(12.5))
            .line_height(relative(1.45))
            .text_color(self.palette.muted)
            .child(s.into())
            .into_any_element()
    }

    /// Post › Copy Link: the post in front of you. In Reading, the open (or
    /// selected) post's page; elsewhere, your post's permalink.
    pub(crate) fn copy_link(&mut self, cx: &mut Context<Self>) {
        let url = if self.reading.view == reading::View::Reading {
            let key = self
                .reading
                .opened
                .as_ref()
                .map(|o| o.key.clone())
                .or_else(|| self.reading.sel.clone());
            key.and_then(|k| {
                self.reading
                    .rows
                    .iter()
                    .find(|r| reading::vm::key(r) == k)
                    .and_then(|r| link_of(r.page.as_deref()))
            })
        } else {
            self.current
                .as_ref()
                .and_then(|i| link_of(i.permalink.as_deref()))
        };
        match url {
            Some(u) => {
                cx.write_to_clipboard(ClipboardItem::new_string(u.clone()));
                self.show_toast("Link copied", Some(u.into()), cx);
            }
            None => self.show_toast(
                "No link to copy",
                Some("Open a published post first".into()),
                cx,
            ),
        }
    }
}

#[cfg(test)]
mod unit_tests {
    use super::link_of;

    #[test]
    fn only_web_addresses_are_copied() {
        assert_eq!(
            link_of(Some(" https://blyg.example.com/t/abc ")).as_deref(),
            Some("https://blyg.example.com/t/abc")
        );
        assert_eq!(link_of(Some("media/x.png")), None);
        assert_eq!(link_of(None), None);
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod ui_tests;
