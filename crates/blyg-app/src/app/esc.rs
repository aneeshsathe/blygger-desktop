//! --- esc --- Windows: a WebView2 that has the keyboard keeps every key
//! from the app. Esc pressed in one (`webview2_host::on_escape`) and a
//! click anywhere else in the window take the keyboard back, as macOS does
//! by itself.

use super::*;

impl MainView {
    /// --- esc --- Esc was pressed in a web view that had the keyboard (the
    /// preview, the reader's post, the browser pane's page; Windows only,
    /// where WebView2 keeps keys from the app). The keyboard comes back to
    /// the app and Esc is handled there as if pressed in it: the open
    /// sheet, palette, author's-site view or pane closes, as on macOS. After this update,
    /// because Esc's handlers update this view.
    pub(crate) fn escape_from_web_view(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.studio.hand_back_keyboard();
        self.reading.site.hand_back_keyboard();
        self.browser.with(|s| s.focus_parent());
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
