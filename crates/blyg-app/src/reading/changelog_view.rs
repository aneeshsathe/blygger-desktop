//! --- changelog display --- What an opened post shows of its changelog,
//! as the blyg's `readerChangelogDisplay` asks (studio 0.58, contract
//! 5.8.0; `blyg_core::changelog`): the latest note (the default: Burrow
//! shows it as before, on stream cards and in "edited since you read it"),
//! every note newest first, or the blyg's AI summary of them.
//!
//! A summary is asked for only when a post is opened, never for a card,
//! and once per (post, version) a session (the blyg keeps it too). It is
//! always labelled with the model and "not the author's words". Until it
//! arrives, or when it can't be written, the history shows.
//!
//! Your own posts in the reader use the same setting, with your versions'
//! notes. Settings (⌘,) chooses the reader's mode and your item pages'
//! (`changelogDisplay`, which the studio draws).
//!
//! #96: nothing here, or anywhere in Burrow's reading, routes, sorts or
//! hides a post by what its note says.

use std::collections::HashMap;

use blyg_core::changelog::{
    ChangelogDisplay, ChangelogSettings, ChangelogSummary, Shown, history_row, shown,
};
use blyg_core::{CoreError, SubscriptionKind};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::Load;
use super::vm::Key;
use crate::app::MainView;

/// The heading over a summary.
pub const SUMMARY_HEADING: &str = "Changes so far";
/// While the blyg writes it.
pub const SUMMARIZING: &str = "Summarizing the changelog…";

/// The changelog's state (inside `reading::State`).
#[derive(Default)]
pub struct Changelogs {
    /// The blyg's changelog settings (`Unavailable`: a studio before 0.58).
    pub settings: Load<ChangelogSettings>,
    /// Summaries by (post, version), asked once a session.
    pub summaries: HashMap<(Key, u32), Load<ChangelogSummary>>,
    /// Saving a Settings choice.
    pub saving: bool,
}

impl MainView {
    /// The reader's mode (the latest note until the blyg says otherwise).
    pub(crate) fn reader_changelog_mode(&self) -> ChangelogDisplay {
        self.reading
            .changelogs
            .settings
            .ready()
            .map(ChangelogSettings::reader_mode)
            .unwrap_or_default()
    }

    /// Read the blyg's changelog settings once (a post opening asks).
    pub(crate) fn ensure_changelog_settings(&mut self, cx: &mut Context<Self>) {
        if matches!(self.reading.changelogs.settings, Load::Idle) {
            self.changelog_settings_refresh(cx);
        }
    }

    /// Read them again (Settings opening asks, so a change made in Studio
    /// shows).
    pub(crate) fn changelog_settings_refresh(&mut self, cx: &mut Context<Self>) {
        if matches!(self.reading.changelogs.settings, Load::Loading) {
            return;
        }
        if !matches!(self.reading.changelogs.settings, Load::Ready(_)) {
            self.reading.changelogs.settings = Load::Loading;
        }
        let backend = self.backend.clone();
        let task = cx.background_spawn(async move { backend.changelog_settings() });
        cx.spawn(async move |this, cx| {
            let r = task.await;
            let _ = this.update(cx, |v, cx| {
                v.reading.changelogs.settings = match r {
                    Ok(Some(s)) => Load::Ready(s),
                    Ok(None) => Load::Unavailable,
                    // Offline or refused: keep what was read before.
                    Err(e) => match std::mem::take(&mut v.reading.changelogs.settings) {
                        Load::Ready(s) => Load::Ready(s),
                        _ => Load::Failed(e.to_string()),
                    },
                };
                v.changelog_maybe_summarize(cx);
                cx.notify();
            });
        })
        .detach();
    }

    /// Settings: save one of the two choices on the blyg.
    pub(crate) fn save_changelog_display(
        &mut self,
        reader: bool,
        d: ChangelogDisplay,
        cx: &mut Context<Self>,
    ) {
        let Load::Ready(cur) = &mut self.reading.changelogs.settings else {
            return;
        };
        let patch = if reader {
            if cur.reader == Some(d) {
                return;
            }
            cur.reader = Some(d);
            ChangelogSettings {
                reader: Some(d),
                ..Default::default()
            }
        } else {
            if cur.page == Some(d) {
                return;
            }
            cur.page = Some(d);
            ChangelogSettings {
                page: Some(d),
                ..Default::default()
            }
        };
        self.reading.changelogs.saving = true;
        cx.notify();
        let backend = self.backend.clone();
        let task = cx.background_spawn(async move { backend.save_changelog_settings(&patch) });
        cx.spawn(async move |this, cx| {
            let r = task.await;
            let _ = this.update(cx, |v, cx| {
                v.reading.changelogs.saving = false;
                match r {
                    Ok(s) if s.served() => {
                        v.reading.changelogs.settings = Load::Ready(s);
                        v.changelog_maybe_summarize(cx);
                    }
                    Ok(_) => {}
                    Err(e) => {
                        v.show_toast(
                            format!("Couldn't save the changelog setting: {e}"),
                            None,
                            cx,
                        );
                        v.changelog_settings_refresh(cx);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Ask the blyg for the open post's summary, when the mode is summary,
    /// the post's changelog is in and says more than its first version, and
    /// it hasn't been asked this session. Only an opened post asks.
    pub(crate) fn changelog_maybe_summarize(&mut self, cx: &mut Context<Self>) {
        if self.reader_changelog_mode() != ChangelogDisplay::Summary
            || self.reading.view != super::View::Reading
        {
            return;
        }
        let Some(o) = self.reading.opened.as_ref() else {
            return;
        };
        if o.item.state == "tombstone" || super::original::is_external(&o.key) {
            return;
        }
        let blyg = self
            .reading
            .subs
            .iter()
            .find(|s| s.id == o.item.subscription_id)
            .is_none_or(|s| s.kind == SubscriptionKind::Blyg);
        if !blyg {
            return;
        }
        let Some(log) = o.changelog.ready() else {
            return;
        };
        if !matches!(
            shown(ChangelogDisplay::Summary, log, o.item.version),
            Shown::Summary(_)
        ) {
            return;
        }
        let at = (o.key.clone(), o.item.version);
        if self.reading.changelogs.summaries.contains_key(&at) {
            return;
        }
        self.reading
            .changelogs
            .summaries
            .insert(at.clone(), Load::Loading);
        let backend = self.backend.clone();
        let own = super::own::is_own(&o.key)
            .then(|| self.own_local_id(&o.key.1))
            .flatten();
        if super::own::is_own(&o.key) && own.is_none() {
            self.reading
                .changelogs
                .summaries
                .insert(at, Load::Failed("this post isn't on your blyg yet".into()));
            return;
        }
        let (sub, rid) = o.key.clone();
        let task = cx.background_spawn(async move {
            match own {
                Some(id) => backend.summarize_own_changelog(&id),
                None => backend.summarize_reading_changelog(&sub, &rid),
            }
        });
        cx.spawn(async move |this, cx| {
            let r = task.await;
            let _ = this.update(cx, |v, cx| {
                let l = match r {
                    Ok(s) => Load::Ready(s),
                    Err(CoreError::NotFound) => Load::Unavailable,
                    Err(e) => Load::Failed(e.to_string()),
                };
                v.reading.changelogs.summaries.insert(at, l);
                cx.notify();
            });
        })
        .detach();
    }

    /// Above the open post's body, in the notes area: the history, or the
    /// summary (with the history until it's in). Nothing in latest mode:
    /// the stream's card and "edited since you read it" show that, as before.
    pub(crate) fn render_changelog_block(&self, o: &super::Opened) -> Option<AnyElement> {
        let mode = self.reader_changelog_mode();
        if mode == ChangelogDisplay::Latest
            || o.item.state == "tombstone"
            || o.pinned_on_screen().is_some()
        {
            return None;
        }
        let log = o.changelog.ready()?;
        let p = self.palette.on_page();
        let rows = |lines: &[blyg_core::changelog::NoteLine]| {
            div().flex().flex_col().children(
                lines
                    .iter()
                    .map(|l| div().child(history_row(l)).into_any_element()),
            )
        };
        let base = div()
            .id("changelog-block")
            .mt(px(8.))
            .mb(px(10.))
            .pl(px(10.))
            .border_l_2()
            .border_color(p.line)
            .font_family(self.chrome())
            .text_size(px(12.))
            .text_color(p.muted)
            .flex()
            .flex_col()
            .gap(px(3.));
        let el = match shown(mode, log, o.item.version) {
            Shown::Nothing | Shown::Latest { .. } => return None,
            Shown::History(lines) => base
                .debug_selector(|| "changelog-history".into())
                .child(div().text_color(p.ink).child("Changelog"))
                .child(rows(&lines)),
            Shown::Summary(lines) => {
                let got = self
                    .reading
                    .changelogs
                    .summaries
                    .get(&(o.key.clone(), o.item.version));
                match got {
                    Some(Load::Ready(s)) => {
                        let label = s.label();
                        base.debug_selector(|| "changelog-summary".into())
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(6.))
                                    .child(div().text_color(p.ink).child(SUMMARY_HEADING))
                                    .child(
                                        div()
                                            .px(px(5.))
                                            .rounded(px(4.))
                                            .border_1()
                                            .border_color(p.line)
                                            .text_size(px(10.5))
                                            .child("AI summary"),
                                    ),
                            )
                            .child(div().text_color(p.ink).child(s.summary.clone()))
                            .child(div().italic().text_size(px(11.)).child(label))
                    }
                    other => {
                        let why = match other {
                            Some(Load::Failed(e)) => format!("No summary: {e}"),
                            Some(Load::Unavailable) => {
                                "No summary: your blyg doesn't write them".into()
                            }
                            _ => SUMMARIZING.into(),
                        };
                        base.debug_selector(|| "changelog-summary-pending".into())
                            .child(div().italic().child(why))
                            .child(div().text_color(p.ink).child("Changelog"))
                            .child(rows(&lines))
                    }
                }
            }
        };
        Some(el.into_any_element())
    }

    /// Settings › Reading: the author's-site toggle, and the blyg's two
    /// changelog choices.
    pub(crate) fn render_reader_view_settings(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let theme = &*self.theme;
        let chip = |id: SharedString, label: &'static str, on: bool| {
            div()
                .id(id.clone())
                .debug_selector(move || id.to_string())
                .px(px(10.))
                .py(px(3.))
                .map(|d| crate::theme_ext::chip(d, theme, on))
                .cursor_pointer()
                .hover(|s| s.border_color(p.accent).bg(p.hover()))
                .child(label)
        };
        let label = |t: &'static str| {
            div()
                .w(px(92.))
                .flex_none()
                .pt(px(4.))
                .text_size(px(10.5))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(p.muted)
                .child(t)
        };
        let on = self.prefs.author_site;
        let site_row = div()
            .flex()
            .gap(px(12.))
            .py(px(7.))
            .child(label("READING"))
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap(px(6.))
                            .child(chip("author-site-off".into(), "Burrow's reader", !on).on_click(
                                cx.listener(|this, _, window, cx| {
                                    this.set_author_site(false, window, cx)
                                }),
                            ))
                            .child(
                                chip("author-site-on".into(), "Open posts on the author's site", on)
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.set_author_site(true, window, cx)
                                    })),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(p.muted)
                            .child(if super::site_view::embeddable() {
                                "Shows a post's own page, as its author styled it, where the reader would. The toggle above a post switches that one view."
                            } else {
                                "Opens a post's own page in your default browser."
                            }),
                    ),
            );
        let changelog: AnyElement = match &self.reading.changelogs.settings {
            Load::Ready(s) => {
                let line = |reader: bool, title: &'static str| {
                    let cur = if reader { s.reader } else { s.page }.unwrap_or_default();
                    div()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap(px(6.))
                        .child(div().text_size(px(11.)).text_color(p.muted).child(title))
                        .children(ChangelogDisplay::ALL.into_iter().map(|d| {
                            let id = format!(
                                "changelog-{}-{}",
                                if reader { "reader" } else { "page" },
                                d.as_str()
                            );
                            chip(id.into(), d.label(), cur == d)
                                .tooltip(move |_, cx| {
                                    cx.new(|_| super::Tip(d.hint().into())).into()
                                })
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.save_changelog_display(reader, d, cx)
                                }))
                        }))
                };
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .child(line(true, "In your reader, show"))
                    .child(line(false, "On your post pages, show"))
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(p.muted)
                            .child("Saved on your blyg. A summary is written from the notes by its change-notes model, labelled as AI, and only when you open a post."),
                    )
                    .into_any_element()
            }
            Load::Unavailable => div()
                .pt(px(4.))
                .text_size(px(11.5))
                .text_color(p.muted)
                .child("Your blyg needs blygger-studio 0.58 or later to show more of a changelog.")
                .into_any_element(),
            Load::Failed(e) => div()
                .pt(px(4.))
                .text_size(px(11.5))
                .text_color(p.muted)
                .child(format!("Couldn't read your blyg's changelog settings: {e}"))
                .into_any_element(),
            _ => div()
                .pt(px(4.))
                .text_size(px(11.5))
                .text_color(p.muted)
                .child("Reading your blyg's settings…")
                .into_any_element(),
        };
        div()
            .flex()
            .flex_col()
            .child(site_row)
            .child(
                div()
                    .flex()
                    .gap(px(12.))
                    .py(px(7.))
                    .child(label("CHANGELOG"))
                    .child(div().flex_1().child(changelog)),
            )
            .into_any_element()
    }

    /// Settings' toggle: `open-posts-on-site`, written to the config file.
    pub(crate) fn set_author_site(
        &mut self,
        on: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.prefs.author_site == on {
            return;
        }
        self.prefs.author_site = on;
        self.reading.site.item = None;
        self.apply_prefs(window, cx);
    }
}
