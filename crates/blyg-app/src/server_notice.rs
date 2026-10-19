//! What Burrow tells you about your blyg's server.
//!
//! The "your blyg needs updating" notice: Burrow needs blygger-studio 0.47
//! or later (`blyg_core::floor`), because its `[[` picker writes labelled
//! links, `[[id | label]]`, that an older studio doesn't understand. On an
//! older blyg a post holding them waits (`blyg_core::floor::has_labels`):
//! its sync and its publish, which offers to write the links bare
//! (`HELD_*`). Everything else syncs. A toast when sync first notices (and
//! on connecting to such a blyg), then a status-bar notice for as long as it
//! stays true. The About window says the same.
//!
//! There used to be a second notice here, a sheet listing what a studio
//! older than 0.40 limits. The 0.47 floor made it unreachable: every blyg
//! old enough to see it already gets this one, and nothing is sent to it.
//!
//! A child module of `app` (like the update notice) so it can use
//! `MainView`'s toast.

use gpui_kit::*;

use super::MainView;

/// Where the notice's link goes: the server requirements, which link on to
/// upstream's upgrade guide.
pub const SERVER_HELP_URL: &str = "https://aneeshsathe.github.io/blygger-desktop/server.html";

/// The toast, once per session (and on connecting to an older blyg).
pub const OUTDATED_TOAST: &str = "Your blyg needs updating";
pub const OUTDATED_TOAST_SUB: &str =
    "Posts with labelled links wait until it's on blygger-studio 0.47 or later.";

/// The status bar's text while the server is outdated, and its link.
pub const OUTDATED_NOTICE: &str =
    "Your blyg needs updating · posts with labelled links wait until it's on studio 0.47";
pub const OUTDATED_LINK: &str = "Update your blyg";

/// The About window's line, under the connection.
pub const OUTDATED_ABOUT: &str = "Posts with labelled links wait until it's on blygger-studio \
     0.47 or later. Everything else syncs.";

/// The publish sheet on a post held for its labelled links.
pub const HELD_SHEET: &str =
    "This post has labelled links, which your blyg can't show until it's updated to studio 0.47.";
pub const HELD_FIX: &str = "write them as [[id]] and publish";

/// The list row's tooltip on a post that waits for them.
pub const HELD_TIP: &str = "Waiting: labelled links need studio 0.47";

impl MainView {
    /// Sync just found an outdated server (or a connect did).
    pub(super) fn server_outdated(&mut self, cx: &mut Context<Self>) {
        self.show_toast(
            OUTDATED_TOAST,
            Some(crate::keymap::hint(OUTDATED_TOAST_SUB).into()),
            cx,
        );
    }

    /// "Your blyg needs updating · posts with labelled links wait … ·
    /// Update your blyg", while true.
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
                .child(
                    div()
                        .truncate()
                        .text_color(warn)
                        .child(crate::keymap::hint(OUTDATED_NOTICE)),
                )
                .child("·")
                .child(
                    div()
                        .id("server-notice-link")
                        .cursor_pointer()
                        .text_color(link)
                        .hover(|s| s.underline())
                        .child(OUTDATED_LINK)
                        .on_click(|_, _, cx| cx.open_url(SERVER_HELP_URL)),
                )
                .into_any_element(),
        )
    }
}
