//! The "your blyg's server needs updating" notice: Burrow needs
//! blygger-studio 0.9 or later, and an older server gets nothing pushed to it
//! (see `CoreError::ServerOutdated`). A toast when sync first notices, then a
//! status-bar notice for as long as it stays true. A child module of `app`
//! (like the update notice) so it can use `MainView`'s toast.

use gpui_kit::*;

use super::MainView;

/// Where the notice's link goes: the server requirements, which link on to
/// upstream's upgrade guide.
pub const SERVER_HELP_URL: &str = "https://aneeshsathe.github.io/blygger-desktop/server.html";

/// The toast, once per session.
pub const OUTDATED_TOAST: &str = "Your blyg's server needs updating";
pub const OUTDATED_TOAST_SUB: &str =
    "Burrow needs blygger-studio 0.9 or later. Nothing syncs until the server is updated.";

/// The status bar's text while the server is outdated.
pub const OUTDATED_NOTICE: &str = "Server needs updating · nothing syncs";

impl MainView {
    /// Sync just found an outdated server.
    pub(super) fn server_outdated(&mut self, cx: &mut Context<Self>) {
        self.show_toast(OUTDATED_TOAST, Some(OUTDATED_TOAST_SUB.into()), cx);
    }

    /// "Server needs updating · nothing syncs · How to update", while true.
    pub(super) fn render_server_notice(&self) -> Option<AnyElement> {
        if !self.backend.server_outdated() {
            return None;
        }
        // --- themes --- on the status bar's own ground, warning-coloured.
        let warn = self.palette.on_status_text(self.palette.warn);
        let link = self.palette.on_status_text(self.palette.accent);
        Some(
            div()
                .id("server-notice")
                .flex()
                .items_center()
                .gap(px(6.))
                .min_w_0()
                .overflow_hidden()
                .child(div().truncate().text_color(warn).child(OUTDATED_NOTICE))
                .child("·")
                .child(
                    div()
                        .id("server-notice-link")
                        .cursor_pointer()
                        .text_color(link)
                        .hover(|s| s.underline())
                        .child("How to update")
                        .on_click(|_, _, cx| cx.open_url(SERVER_HELP_URL)),
                )
                .into_any_element(),
        )
    }
}
