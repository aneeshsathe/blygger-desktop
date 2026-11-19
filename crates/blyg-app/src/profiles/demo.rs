//! `BLYGGER_DEMO=pf-…`: scripted walkthroughs of profiles for screenshots
//! (fake mode), calling the same methods the keys and clicks do.
//!
//! - `pf-reading`: Lin's thread open (the lineage line), then ⌘I.
//! - `pf-blogroll`: Lin's profile, then a blogroll entry's profile (back arrow).
//! - `pf-connections`: Lin's profile on the Connections tab.
//! - `pf-posts`: Rue's profile on the Posts tab.
//! - `pf-feed`: a plain RSS feed's card.
//! - `pf-own`: your own profile, with the blogroll toggles.
//! - `pf-ask`: ⇧⌘O "Open profile…".

use gpui_kit::*;

use super::vm::Tab;
use crate::app::MainView;
use crate::app::reading::View;
use crate::fake::profile_seed::TIDES;
use crate::fake::reading_seed::{LIN, LIN_GARDENS, OMAR, RUE};
use crate::scenes::Scene;

impl MainView {
    pub(crate) fn profile_demo(
        &mut self,
        scenario: Scene,
        n: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let open_lin = |this: &mut Self, window: &mut Window, cx: &mut Context<Self>| {
            this.show_view(View::Reading, window, cx);
            if let Some(k) = this
                .reading
                .rows
                .iter()
                .find(|r| r.remote_id == LIN_GARDENS)
                .map(crate::app::reading::vm::key)
            {
                this.open_reading(k, window, cx);
            }
        };
        match (scenario, n) {
            (Scene::PfReading, 0) => open_lin(self, window, cx),
            (Scene::PfReading, 1) => self.profile_for_context(window, cx),
            (Scene::PfBlogroll, 0) => open_lin(self, window, cx),
            (Scene::PfBlogroll, 1) => self.open_profile(LIN, window, cx),
            (Scene::PfBlogroll, 2) => self.open_profile(TIDES, window, cx),
            (Scene::PfConnections, 0) => open_lin(self, window, cx),
            (Scene::PfConnections, 1) => self.open_profile(LIN, window, cx),
            (Scene::PfConnections, 2) => self.set_profile_tab(Tab::Connections, cx),
            (Scene::PfPosts, 0) => self.show_view(View::Reading, window, cx),
            (Scene::PfPosts, 1) => self.open_profile(RUE, window, cx),
            (Scene::PfPosts, 2) => self.set_profile_tab(Tab::Posts, cx),
            (Scene::PfFeed, 0) => self.show_view(View::Reading, window, cx),
            (Scene::PfFeed, 1) => self.open_profile(format!("{OMAR}feed.xml"), window, cx),
            (Scene::PfOwn, 1) => self.open_own_profile(window, cx),
            (Scene::PfAsk, 1) => {
                self.ask_profile_url(window, cx);
                if let Some(i) = self.profiles.ask.clone() {
                    i.update(cx, |s, cx| {
                        s.set_value("https://tides.example.org/", window, cx)
                    });
                }
            }
            _ => {}
        }
        if n == 2 {
            crate::app::reading::demo::snapshot_later(window, cx);
        }
    }
}
