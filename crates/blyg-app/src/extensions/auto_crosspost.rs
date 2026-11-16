//! --- auto cross-post --- Per post, the cross-post sites to start by
//! themselves when the post is published: the Publish sheet's "then
//! cross-post to" chips, and the palette's "Cross-post automatically"
//! rows, turn each one on or off for the open post.
//!
//! "Automatically" only starts the run. Each one still shows its two sheets
//! (the text, then "Post this to the site?"), and nothing is posted until
//! the user clicks Post (docs/SPEC.md § Browser). The runs go one after
//! another. A run that leaves the page to the user (a failure, the box not
//! taking the text, a stop after Post was clicked) stops the rest, and the
//! toast names the sites still to do.
//!
//! The choices live in `state.json` (`auto_cross_post`), by blyg host and
//! post, as `<extension>/<macro>` keys, or for the session only on the
//! sample data and in tests.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use blyg_core::LocalId;
use blyg_ext::MacroEntry;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::sheets::{Row, RowAction};
use crate::app::MainView;
use crate::app::browser::automation::Outcome;

/// What's chosen, and what's still to run after a publish.
#[derive(Default)]
pub(crate) struct Auto {
    /// `"<host> <local id>"` → `<ext>/<macro>` keys, once read.
    chosen: Option<BTreeMap<String, BTreeSet<String>>>,
    /// The runs still to start: (post, extension, macro id, site title).
    queue: VecDeque<(LocalId, String, String, String)>,
}

/// A macro's key in the choices.
pub fn macro_key(ext: &str, id: &str) -> String {
    format!("{ext}/{id}")
}

fn post_key(host: &str, id: &LocalId) -> String {
    format!("{host} {}", id.0)
}

/// Whether the next run goes ahead after `out`: yes after a post, or after
/// a cancel before Post (that site was skipped); no when the page still
/// needs the user.
pub fn goes_on_after(out: &Outcome) -> bool {
    matches!(
        out,
        Outcome::Posted { .. }
            | Outcome::Cancelled {
                after_submit: false
            }
    )
}

impl MainView {
    fn auto_host(&self, cx: &App) -> String {
        crate::settings::blyg_url(cx)
            .and_then(|u| crate::vm::url_host(&u))
            .unwrap_or_default()
    }

    fn auto_load(&mut self, cx: &App) -> &mut BTreeMap<String, BTreeSet<String>> {
        if self.ext.auto.chosen.is_none() {
            let loaded = Self::tray_data_dir(cx)
                .map(|d| blyg_core::state::AppState::load(&d).auto_cross_post)
                .unwrap_or_default()
                .into_iter()
                .map(|(k, v)| (k, v.into_iter().collect()))
                .collect();
            self.ext.auto.chosen = Some(loaded);
        }
        self.ext.auto.chosen.get_or_insert_with(Default::default)
    }

    /// The `<ext>/<macro>` keys chosen for post `id`.
    pub(crate) fn auto_crosspost_chosen(&mut self, id: &LocalId, cx: &App) -> BTreeSet<String> {
        let key = post_key(&self.auto_host(cx), id);
        self.auto_load(cx).get(&key).cloned().unwrap_or_default()
    }

    /// The macros a post can be cross-posted with when it's published: the
    /// granted ones of running extensions that apply to published posts,
    /// where there's a browser pane to run them in.
    pub(crate) fn auto_crosspost_sites(&self) -> Vec<MacroEntry> {
        if !crate::app::browser::pane_available() {
            return vec![];
        }
        let Some(host) = &self.ext.host else {
            return vec![];
        };
        host.macros()
            .into_iter()
            .filter(|m| m.spec.when == blyg_ext::protocol::When::Published)
            .collect()
    }

    /// Turn cross-posting to one site on or off for post `id`.
    pub(crate) fn toggle_auto_crosspost(
        &mut self,
        id: &LocalId,
        ext: &str,
        macro_id: &str,
        cx: &mut Context<Self>,
    ) {
        let key = post_key(&self.auto_host(cx), id);
        let m = macro_key(ext, macro_id);
        let chosen = self.auto_load(cx);
        let set = chosen.entry(key.clone()).or_default();
        if !set.remove(&m) {
            set.insert(m);
        }
        let now: Vec<String> = set.iter().cloned().collect();
        if now.is_empty() {
            chosen.remove(&key);
        }
        if let Some(d) = Self::tray_data_dir(cx) {
            let _ = blyg_core::state::AppState::update(&d, |s| {
                if now.is_empty() {
                    s.auto_cross_post.remove(&key);
                } else {
                    s.auto_cross_post.insert(key, now);
                }
            });
        }
        cx.notify();
    }

    /// Hook (after a publish succeeded): queue the chosen sites for `id`
    /// and start the first. The site titles queued, for the toast.
    pub(crate) fn auto_crosspost_after_publish(
        &mut self,
        id: &LocalId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<String> {
        let chosen = self.auto_crosspost_chosen(id, cx);
        if chosen.is_empty() {
            return vec![];
        }
        let mut titles = vec![];
        for m in self.auto_crosspost_sites() {
            if chosen.contains(&macro_key(&m.ext, &m.spec.id)) {
                titles.push(m.site.title.clone());
                self.ext
                    .auto
                    .queue
                    .push_back((id.clone(), m.ext, m.spec.id, m.site.title));
            }
        }
        if !titles.is_empty() {
            cx.defer_in(window, |this, window, cx| {
                this.auto_crosspost_next(window, cx)
            });
        }
        titles
    }

    /// Start the next queued run, unless one is going.
    pub(crate) fn auto_crosspost_next(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.browser.automation.running() {
            return;
        }
        while let Some((id, ext, macro_id, _)) = self.ext.auto.queue.pop_front() {
            if self.ext_run_macro_for(ext, macro_id, Some(id), window, cx) {
                return;
            }
            // Refused before it started (the grant went, offline): the next.
        }
    }

    /// Hook (a macro run ended with `out`): start the next queued run, or,
    /// when the page still needs the user, stop the rest. What to add to
    /// the run's toast in that case.
    pub(crate) fn auto_crosspost_after_run(
        &mut self,
        out: &Outcome,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<String> {
        if self.ext.auto.queue.is_empty() {
            return None;
        }
        if goes_on_after(out) {
            cx.defer_in(window, |this, window, cx| {
                this.auto_crosspost_next(window, cx)
            });
            return None;
        }
        let rest: Vec<String> = self.ext.auto.queue.drain(..).map(|q| q.3).collect();
        Some(crate::keymap::hint_owned(format!(
            "Not started: {} (⇧⌘P cross-posts it)",
            rest.join(", ")
        )))
    }

    /// `auto_crosspost_chosen` without remembering what was read (for
    /// render and the palette, which can't change the view).
    fn auto_chosen_now(&self, id: &LocalId, cx: &App) -> BTreeSet<String> {
        let key = post_key(&self.auto_host(cx), id);
        match &self.ext.auto.chosen {
            Some(c) => c.get(&key).cloned().unwrap_or_default(),
            None => Self::tray_data_dir(cx)
                .map(|d| blyg_core::state::AppState::load(&d).auto_cross_post)
                .and_then(|mut m| m.remove(&key))
                .unwrap_or_default()
                .into_iter()
                .collect(),
        }
    }

    /// The palette's rows for the open post: "Cross-post automatically to
    /// <site>", on or off.
    pub(crate) fn auto_crosspost_rows(&self, id: Option<&LocalId>, cx: &App) -> Vec<Row> {
        let Some(id) = id else {
            return vec![];
        };
        let chosen = self.auto_chosen_now(id, cx);
        self.auto_crosspost_sites()
            .into_iter()
            .map(|m| {
                let on = chosen.contains(&macro_key(&m.ext, &m.spec.id));
                Row {
                    action: RowAction::AutoCrossPost {
                        ext: m.ext.clone(),
                        id: m.spec.id.clone(),
                    },
                    label: format!(
                        "Cross-post automatically to {}: {}",
                        m.site.title,
                        if on { "on" } else { "off" }
                    ),
                    detail: if on {
                        "Turn off: publishing this post won't start it".into()
                    } else {
                        format!(
                            "Turn on: publishing this post starts it; you still check it and click Post on {}",
                            m.site.title
                        )
                    },
                    from: Some(m.ext),
                }
            })
            .collect()
    }

    /// The Publish sheet's "Then cross-post to" row for post `id`: a chip
    /// per site, ticked when chosen. `None` with no sites to offer.
    pub(crate) fn render_auto_crosspost(
        &self,
        id: &LocalId,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let sites = self.auto_crosspost_sites();
        if sites.is_empty() {
            return None;
        }
        let chosen = self.auto_chosen_now(id, cx);
        let p = self.palette;
        let mut row = div()
            .id("auto-crosspost")
            .mt(px(10.))
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(6.))
            .text_size(px(12.))
            .child(div().text_color(p.muted).child("Then cross-post to"));
        for (n, m) in sites.into_iter().enumerate() {
            let on = chosen.contains(&macro_key(&m.ext, &m.spec.id));
            let (pid, ext, macro_id) = (id.clone(), m.ext.clone(), m.spec.id.clone());
            row = row.child(
                div()
                    .id(SharedString::from(format!("auto-crosspost-{n}")))
                    .debug_selector(move || format!("auto-crosspost-{n}"))
                    .px(px(8.))
                    .py(px(2.))
                    .rounded(px(self.theme.chip_radius))
                    .border_1()
                    .border_color(if on { p.accent } else { p.line })
                    .when(on, |d| d.text_color(p.accent))
                    .cursor_pointer()
                    .hover(|s| s.border_color(p.accent))
                    .child(format!("{} {}", if on { "☑" } else { "☐" }, m.site.title))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.toggle_auto_crosspost(&pid, &ext, &macro_id, cx)
                    })),
            );
        }
        Some(
            div()
                .child(row)
                .child(
                    div()
                        .mt(px(4.))
                        .text_size(px(11.5))
                        .text_color(p.muted)
                        .child(
                            "Remembered for this post. Each one still shows you the text, \
                             and posts only when you click Post.",
                        ),
                )
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    // Not `super::*`: `gpui_kit::*` brings its own `test` attribute.
    use super::{LocalId, Outcome, goes_on_after, macro_key, post_key};

    #[test]
    fn runs_go_on_after_a_post_or_a_skip_only() {
        assert!(goes_on_after(&Outcome::Posted {
            message: "Posted".into(),
            via: crate::app::browser::automation::insert::Via::Paste,
        }));
        assert!(goes_on_after(&Outcome::Cancelled {
            after_submit: false
        }));
        assert!(!goes_on_after(&Outcome::Cancelled { after_submit: true }));
        assert!(!goes_on_after(&Outcome::Handed { manual: true }));
    }

    #[test]
    fn keys_name_the_blyg_and_the_post() {
        assert_eq!(macro_key("cross-post", "bluesky"), "cross-post/bluesky");
        assert_eq!(
            post_key("blyg.example.com", &LocalId("01J9".into())),
            "blyg.example.com 01J9"
        );
    }
}
