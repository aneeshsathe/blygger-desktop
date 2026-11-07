//! --- contacts --- `BLYGGER_DEMO=ct-…`: the contacts screens for
//! screenshots (fake mode), calling the same methods the menus do.
//!
//! - `ct-book`: the Contacts sheet.
//! - `ct-author`: Add Author to Contacts on a reading post (the filled form).
//! - `ct-sent`: Mentions › Sent.
//! - `ct-subs`: the Subscriptions screen with each poll's status.

use gpui_kit::*;

use super::super::View;
use crate::app::MainView;

impl MainView {
    pub(crate) fn contacts_demo(
        &mut self,
        scenario: &str,
        n: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match (scenario, n) {
            ("ct-book", 0) => self.toggle_contacts_sheet(window, cx),
            ("ct-author", 0) => {
                self.reading.mode = super::super::stream_vm::ReadMode::Reader;
                self.show_view(View::Reading, window, cx);
            }
            ("ct-author", 1) => {
                let k = self
                    .reading
                    .rows
                    .iter()
                    .find(|r| r.remote_id == crate::fake::reading_seed::ADA_FINISHED)
                    .map(super::super::vm::key);
                if let Some(k) = k {
                    self.open_reading(k, window, cx);
                }
            }
            ("ct-author", 2) => self.add_author_to_contacts(window, cx),
            ("ct-sent", 0) => self.show_view(View::Mentions, window, cx),
            ("ct-sent", 1) => self.set_mentions_side(true, cx),
            ("ct-subs", 0) => self.show_view(View::Subscriptions, window, cx),
            _ => {}
        }
    }
}
