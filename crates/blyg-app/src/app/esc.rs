//! --- esc --- Windows: a WebView2 that has the keyboard keeps every key
//! from the app. Esc pressed in one (`webview2_host::on_escape`) and a
//! click anywhere else in the window take the keyboard back, as macOS does
//! by itself.

use super::*;
use crate::platform_text::{Text, ui_text_per_platform};

/// The toast after the pane's first Esc, which only takes the keyboard
/// back: without it that Esc looked like it did nothing (user testing,
/// 0.28).
pub(crate) const PANE_ESC_HINT: Text = ui_text_per_platform(
    "Keyboard back in Burrow · esc again closes the pane",
    "Keyboard back in Burrow · Esc again closes the pane",
);

impl MainView {
    /// --- esc --- Esc was pressed in a web view that had the keyboard (the
    /// preview, the reader's post or the author's site; Windows only, where
    /// WebView2 keeps keys from the app). The keyboard comes back to the
    /// app and Esc is handled there as if pressed in it: the open sheet,
    /// palette or author's-site view closes, as on macOS. After this
    /// update, because Esc's handlers update this view.
    pub(crate) fn escape_from_web_view(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.hand_back_keyboard_from_web_views();
        Self::dispatch_escape(window, cx);
    }

    /// --- esc --- Esc was pressed in the browser pane's page. The page saw
    /// it too (a site's own pop-up or menu closes), so it isn't also the
    /// app's "close the pane", which could fold away a half-written post
    /// or a cross-post's composer: the keyboard comes back to the app, and
    /// only an open sheet or palette closes. A second Esc, now in the app,
    /// closes the pane; a toast says so (`PANE_ESC_HINT`).
    pub(crate) fn escape_from_pane(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.hand_back_keyboard_from_web_views();
        let over = self.sheet.is_open()
            || self.cmd_palette.open.is_some()
            || self.ext.has_overlay()
            || self.reading.sheet.is_some()
            || self.ai.has_overlay();
        if over {
            Self::dispatch_escape(window, cx);
        } else if self.browser.open {
            self.show_toast(PANE_ESC_HINT.get(), None, cx);
        }
    }

    fn hand_back_keyboard_from_web_views(&mut self) {
        self.studio.hand_back_keyboard();
        self.reading.site.hand_back_keyboard();
        self.browser.with(|s| s.focus_parent());
    }

    fn dispatch_escape(window: &mut Window, cx: &mut Context<Self>) {
        window.defer(cx, |window, cx| {
            if let Ok(esc) = Keystroke::parse("escape") {
                window.dispatch_keystroke(esc, cx);
            }
        });
    }

    /// --- esc --- Windows: a click anywhere in the app (never in a page,
    /// whose clicks don't reach GPUI) takes the keyboard back from the web
    /// views. On macOS clicking the app's view does that by itself; on
    /// Windows the page would keep it, and Esc or typing in a palette
    /// opened by that click would go to the page. Runs after the pane's own
    /// click handler, which may leave the page its keyboard this once
    /// (`Browser::kept_click`).
    pub(crate) fn reclaim_keyboard_on_click(&mut self) {
        self.studio.hand_back_keyboard();
        self.reading.site.hand_back_keyboard();
        if !std::mem::take(&mut self.browser.kept_click) {
            self.browser.with(|s| s.focus_parent());
        }
    }

    /// The window's own click handler (`reclaim_keyboard_on_click`), on
    /// Windows only; `browser_actions` adds it to the window's root.
    pub(crate) fn esc_click_reclaims(
        &self,
        d: Stateful<Div>,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        d.when(cfg!(target_os = "windows"), |d| {
            d.on_any_mouse_down(cx.listener(|this, _, _, _| this.reclaim_keyboard_on_click()))
        })
    }
}
