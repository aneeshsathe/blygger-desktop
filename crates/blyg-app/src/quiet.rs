//! --- quiet --- Quiet writing (`quiet-writing`, docs/SPEC.md § Quiet
//! writing): one setting that hides the layers which sit around the editor
//! on their own, so the writing screen is the list and the page again.
//!
//! While it's on:
//! - the lineage line under the editor isn't drawn (or built) until ⌘J
//!   opens its lists, and folding them hides it again (`lineage_strip.rs`);
//! - the loose-ends line over the list isn't drawn, and its summary isn't
//!   worked out (`loose_ends.rs`); the omnibar's operators still work;
//! - Nearby thoughts has no drawer and no side tab, and matches nothing.
//!   ⌥⌘U (View › Nearby Thoughts) opens it **for this sitting** only: its
//!   tab comes back while it's open, and closing it, quitting, or changing
//!   the setting puts it away. What `state.json` remembers (`nearby_open`)
//!   is left as it was, for when quiet writing is off (`nearby/mod.rs`).
//!
//! Hover cards, the quote tray and focus mode are unchanged: they appear
//! only when asked for. `nearby = off` still turns Nearby off on its own.
//! Settings › Quiet writing, View › Quiet Writing and the palette toggle
//! it; it's written to the config file, so a hand edit applies on reload.
//!
//! A child module of `app` (it reads `MainView`); `app.rs` keeps small hooks
//! marked `// --- quiet ---`.

use std::cell::Cell;
use std::rc::Rc;

use super::*;

gpui_kit::actions!(
    blygger,
    [
        /// View › Quiet Writing: hide or show the lineage line, the loose-ends
        /// line and Nearby thoughts.
        ToggleQuietWriting
    ]
);

/// The config key.
pub const KEY: &str = "quiet-writing";

/// `quiet-writing = true`, as configured (false without a config).
pub fn on(cx: &App) -> bool {
    cx.try_global::<crate::settings::AppConfig>()
        .is_some_and(|c| c.store.config().get(KEY) == Some("true"))
}

/// The window's quiet-writing state.
pub(crate) struct Quiet {
    /// The setting as last applied: a change (Settings, the menu, or the
    /// file reloaded) puts away what was opened for the sitting.
    was: bool,
    /// Where Settings' row was drawn (the tour's ring).
    pub(crate) row: Rc<Cell<Option<Bounds<Pixels>>>>,
}

impl Quiet {
    pub(crate) fn new(cx: &App) -> Self {
        Self {
            was: on(cx),
            row: Default::default(),
        }
    }
}

impl MainView {
    /// Quiet writing hides the layers now. The tour's lineage step still
    /// shows the line it teaches.
    pub(crate) fn quiet_now(&self, cx: &App) -> bool {
        on(cx) && !self.tutorial_on_step("lineage-writing")
    }

    /// Hook: the action on the main window.
    pub(super) fn quiet_actions(&self, d: Stateful<Div>, cx: &mut Context<Self>) -> Stateful<Div> {
        d.on_action(
            cx.listener(|this, _: &ToggleQuietWriting, _, cx| this.toggle_quiet_writing(cx)),
        )
    }

    /// View › Quiet Writing (and the palette): flip it, and say what it did.
    pub(crate) fn toggle_quiet_writing(&mut self, cx: &mut Context<Self>) {
        let now = !on(cx);
        if !self.set_quiet_writing(now, cx) {
            return;
        }
        if now {
            self.show_toast(
                "Quiet writing on",
                Some(
                    crate::keymap::hint(
                        "Lineage, loose ends and Nearby are hidden · ⌘J and ⌥⌘U still reach them",
                    )
                    .into(),
                ),
                cx,
            );
        } else {
            self.show_toast(
                "Quiet writing off",
                Some("The lineage line, loose ends and Nearby are back".into()),
                cx,
            );
        }
    }

    /// Write `quiet-writing` and apply it. False when it couldn't be
    /// written (the toast says why).
    pub(crate) fn set_quiet_writing(&mut self, value: bool, cx: &mut Context<Self>) -> bool {
        if on(cx) != value {
            if let Err(e) = crate::settings::write(
                &[(KEY, blyg_core::config::Change::Set(value.to_string()))],
                cx,
            ) {
                self.show_toast(e, None, cx);
                return false;
            }
            // Our own write isn't an edit to reload.
            self.watch_stamp = crate::settings::watch_stamp(&crate::settings::watched(cx));
        }
        self.quiet_reload(cx);
        true
    }

    /// Hook (config reloaded or written): apply a change. What was opened
    /// for the sitting goes; the hidden layers stop, or start, their work.
    pub(super) fn quiet_reload(&mut self, cx: &mut Context<Self>) {
        let now = on(cx);
        if now == self.quiet.was {
            return;
        }
        self.quiet.was = now;
        self.nearby.sitting = false;
        self.nearby.hover = None;
        self.lineage_strip.open = false;
        self.loose_ends_refresh(cx);
        // Shown: look at the paragraph; hidden: drop the rows.
        self.nearby_query(cx);
        cx.notify();
    }

    /// Settings › Quiet writing.
    pub(crate) fn render_quiet_setting(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let theme = &*self.theme;
        let chip = |id: &'static str, label: &'static str, on: bool| {
            div()
                .id(id)
                .debug_selector(move || id.into())
                .px(px(10.))
                .py(px(3.))
                .map(|d| crate::theme_ext::chip(d, theme, on))
                .cursor_pointer()
                .hover(|s| s.border_color(p.accent).bg(p.hover()))
                .child(label)
        };
        use crate::app::settings_page::{chips, note, row};
        let quiet = on(cx);
        let at = self.quiet.row.clone();
        // Records where the row is drawn (for the tour's ring).
        let marker = canvas(move |b, _, _| at.set(Some(b)), |_, _, _, _| {})
            .absolute()
            .inset_0();
        row(
            &p,
            "QUIET WRITING",
            div()
                .flex()
                .flex_col()
                .gap(px(4.))
                .child(
                    chips()
                        .child(
                            chip("quiet-writing-off", "Off", !quiet).on_click(cx.listener(
                                |this, _, _, cx| {
                                    this.set_quiet_writing(false, cx);
                                },
                            )),
                        )
                        .child(chip("quiet-writing-on", "On", quiet).on_click(cx.listener(
                            |this, _, _, cx| {
                                this.set_quiet_writing(true, cx);
                            },
                        ))),
                )
                .child(note(
                    &p,
                    crate::keymap::hint(
                        "Hides the lineage line under the editor, the loose-ends line over the \
                         list and Nearby thoughts, for a plainer page. ⌘J, the omnibar's is: \
                         operators and ⌥⌘U (for this sitting) still reach them; hover cards, the \
                         quote tray and focus mode are unchanged. View › Quiet Writing toggles it.",
                    ),
                )),
        )
        .relative()
        .child(marker)
        .into_any_element()
    }
}
