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
//! A site runs once per post: a post whose copies elsewhere already list
//! one on that site is skipped, and a run that posted without its copy
//! being recorded turns that site off for the post, so publishing v2
//! doesn't post it again. A (post, site) is queued or running once.
//!
//! The choices live in `state.json` (`auto_cross_post`), by blyg host and
//! post, as `<extension>/<macro>` keys, or for the session only on the
//! sample data and in tests. They're read once (when the Publish sheet or a
//! palette opens, or a publish needs them), never while drawing.

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
    /// The run started from the queue, until it ends (or never starts):
    /// (post, extension, macro id).
    running: Option<(LocalId, String, String)>,
}

/// Which macro run ended, for `auto_crosspost_after_run`: its post (a
/// macro on a published post makes a copy of one; `None` otherwise), its
/// extension and its macro id.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Ran<'a> {
    pub post: Option<&'a LocalId>,
    pub ext: &'a str,
    pub macro_id: &'a str,
}

impl Ran<'_> {
    /// It is the queued run `(post, extension, macro id)`.
    fn is(&self, queued: &(LocalId, String, String)) -> bool {
        self.post == Some(&queued.0) && self.ext == queued.1 && self.macro_id == queued.2
    }
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

/// A chip's text: its tick and its site (`☑ Bluesky`). The chips are
/// clicked; the keyboard's way to a site is the palette (`keys_hint`).
pub fn chip_label(site: &str, on: bool) -> String {
    let tick = if on { "☑" } else { "☐" };
    format!("{tick} {site}")
}

/// The line under the chips: the keyboard's way to them (the palette's
/// "Cross-post automatically to <site>" rows), and what ticking one means.
pub(crate) fn keys_hint() -> &'static str {
    crate::keymap::hint(
        "Or ⇧⌘P and type “automatically” to pick one by keyboard. Remembered for this post. \
         Each one still shows you the text, and posts only when you click Post.",
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
        let on = !self
            .auto_crosspost_chosen(id, cx)
            .contains(&macro_key(ext, macro_id));
        self.auto_set_choice(id, ext, macro_id, on, cx);
        cx.notify();
    }

    /// Turn cross-posting to one site on (`on`) or off for post `id`, and
    /// write it to `state.json` (in the tour, for the session only).
    pub(crate) fn auto_crosspost_set(
        &mut self,
        id: &LocalId,
        ext: &str,
        macro_id: &str,
        on: bool,
        cx: &App,
    ) {
        self.auto_set_choice(id, ext, macro_id, on, cx);
    }

    /// Forget the choices read, so the next use reads `state.json` again
    /// (the tour's sample choice goes with the tour).
    pub(crate) fn auto_crosspost_reload(&mut self) {
        self.ext.auto.chosen = None;
    }

    fn auto_set_choice(&mut self, id: &LocalId, ext: &str, macro_id: &str, on: bool, cx: &App) {
        let key = post_key(&self.auto_host(cx), id);
        let m = macro_key(ext, macro_id);
        let chosen = self.auto_load(cx);
        let set = chosen.entry(key.clone()).or_default();
        if on {
            set.insert(m);
        } else {
            set.remove(&m);
        }
        let now: Vec<String> = set.iter().cloned().collect();
        if now.is_empty() {
            chosen.remove(&key);
        }
        if self.ext_tour_on() {
            return; // --- onboarding --- the sample's, never written.
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
            let auto = &self.ext.auto;
            let queued = auto
                .queue
                .iter()
                .any(|q| q.0 == *id && q.1 == m.ext && q.2 == m.spec.id)
                || auto
                    .running
                    .as_ref()
                    .is_some_and(|r| r.0 == *id && r.1 == m.ext && r.2 == m.spec.id);
            if chosen.contains(&macro_key(&m.ext, &m.spec.id))
                && !queued
                && !self.auto_has_copy(id, &m.site.origin)
            {
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

    /// Start the next queued run, unless one is going (or being prepared).
    pub(crate) fn auto_crosspost_next(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.browser.automation.running() || self.ext.auto.running.is_some() {
            return;
        }
        while let Some((id, ext, macro_id, _)) = self.ext.auto.queue.pop_front() {
            self.ext.auto.running = Some((id.clone(), ext.clone(), macro_id.clone()));
            if self.ext_run_macro_for(ext, macro_id, Some(id), window, cx) {
                return;
            }
            self.ext.auto.running = None;
            // Refused before it started (the grant went, offline): the next.
        }
    }

    /// Hook (a queued run's text couldn't be prepared, so it never
    /// started): the next one.
    pub(crate) fn auto_crosspost_not_started(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.ext.auto.running = None;
        self.auto_crosspost_next(window, cx);
    }

    /// Hook (macro run `ran` ended with `out`; `listed`: its copy was
    /// recorded on the post): a queued run that posted without a record
    /// turns its site off for the post, which couldn't tell next time. Then
    /// start the next queued run, or, when the page still needs the user,
    /// stop the rest. What to add to the run's toast in that case.
    ///
    /// A queued run counts as running from the moment it's handed over,
    /// while its text is still being prepared and no macro runs in the pane:
    /// another run (a manual one) can start and end in that gap. Its end
    /// leaves the queued run, its choice and the rest of the queue alone.
    pub(crate) fn auto_crosspost_after_run(
        &mut self,
        ran: Ran<'_>,
        out: &Outcome,
        listed: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<String> {
        match self.ext.auto.running.take() {
            Some(queued) if !ran.is(&queued) => {
                self.ext.auto.running = Some(queued);
                return None;
            }
            Some((id, ext, macro_id)) if out.counts_as_post() && !listed => {
                self.auto_set_choice(&id, &ext, &macro_id, false, cx);
            }
            _ => {}
        }
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

    /// Read the choices from `state.json` if they haven't been yet: when
    /// the Publish sheet or a palette opens, so drawing never reads a file.
    pub(crate) fn auto_crosspost_load(&mut self, cx: &App) {
        self.auto_load(cx);
    }

    /// `auto_crosspost_chosen` as last read (for render and the palette,
    /// which can't change the view; `auto_crosspost_load` reads it first).
    fn auto_chosen_now(&self, id: &LocalId, cx: &App) -> BTreeSet<String> {
        let key = post_key(&self.auto_host(cx), id);
        self.ext
            .auto
            .chosen
            .as_ref()
            .and_then(|c| c.get(&key).cloned())
            .unwrap_or_default()
    }

    /// Post `id` has a copy listed on `origin`'s site (its copies
    /// elsewhere, as the macro run records them).
    fn auto_has_copy(&self, id: &LocalId, origin: &str) -> bool {
        let Some(site) = crate::vm::url_host(origin) else {
            return false;
        };
        self.backend
            .syndication(id)
            .iter()
            .any(|e| crate::vm::url_host(&e.url).as_deref() == Some(site.as_str()))
    }

    /// Forget post `id`'s choices (it was deleted).
    pub(crate) fn auto_crosspost_forget(&mut self, id: &LocalId, cx: &App) {
        let key = post_key(&self.auto_host(cx), id);
        self.auto_load(cx).remove(&key);
        self.ext.auto.queue.retain(|q| q.0 != *id);
        if let Some(d) = Self::tray_data_dir(cx) {
            let _ = blyg_core::state::AppState::update(&d, |s| {
                s.auto_cross_post.remove(&key);
            });
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
                    .child(chip_label(&m.site.title, on))
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
                        .id("auto-crosspost-keys")
                        .debug_selector(|| "auto-crosspost-keys".into())
                        .mt(px(4.))
                        .text_size(px(11.5))
                        .text_color(p.muted)
                        .child(keys_hint()),
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
