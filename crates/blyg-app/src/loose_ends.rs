//! --- loose ends --- The muted summary above the Posts list, in words
//! (proposal 10): "2 orphans · 1 ungenerated TK · 1 draft older than a
//! month". Each part is a one-click `is:orphan` / `has:tk` / `is:stale`.
//! It describes your own text, never anyone's attention: no social count.
//! Worked out on a background thread after the list changes.
//!
//! The operators themselves live in `blyg_core::loose`; `Backend::search`
//! reads them.

use blyg_core::loose::{Facts, Summary};
use gpui_kit::*;

use crate::app::MainView;

#[derive(Default)]
pub(crate) struct LooseEnds {
    pub summary: Summary,
    task: Option<Task<()>>,
}

impl MainView {
    /// Work the summary out again, off this thread.
    pub(crate) fn loose_ends_refresh(&mut self, cx: &mut Context<Self>) {
        let backend = self.backend.clone();
        let task = cx.background_spawn(async move {
            let items = backend.items();
            let facts: Facts = backend.loose_facts();
            Summary::of(&items, &facts)
        });
        self.loose_ends.task = Some(cx.spawn(async move |this, cx| {
            let summary = task.await;
            let _ = this.update(cx, |v, cx| {
                if v.loose_ends.summary != summary {
                    v.loose_ends.summary = summary;
                    cx.notify();
                }
                v.loose_ends.task = None;
            });
        }));
    }

    /// The summary line over the list, when the omnibar is empty and
    /// something is loose.
    pub(crate) fn render_loose_ends(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let parts = self.loose_ends.summary.parts();
        if parts.is_empty() || !self.list.trimmed_query().is_empty() {
            return None;
        }
        let p = self.palette.on_side();
        let mut row = div()
            .id("loose-ends")
            .debug_selector(|| "loose-ends".into())
            .flex_none()
            .flex()
            .flex_wrap()
            .gap_x(px(4.))
            .px(px(14.))
            .py(px(6.))
            .text_size(px(11.5))
            .text_color(p.muted)
            .border_b_1()
            .border_color(self.palette.side_line)
            .font_family(self.theme.chrome_font(&self.prefs));
        for (i, (words, query)) in parts.into_iter().enumerate() {
            if i > 0 {
                row = row.child("·");
            }
            row = row.child(
                div()
                    .id(SharedString::from(format!("loose-{query}")))
                    .debug_selector(move || format!("loose-{query}"))
                    .cursor_pointer()
                    .hover(|s| s.text_color(p.ink))
                    .child(words)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.set_query_text(query, window, cx);
                        this.omni.update(cx, |s, cx| s.focus(window, cx));
                    })),
            );
        }
        Some(row.into_any_element())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use blyg_core::{Backend, ConfigStore, Status};
    use gpui_kit::{Entity, TestAppContext, VisualTestContext};

    use crate::app::MainView;
    use crate::fake::{FakeBackend, Timing};
    use crate::prefs::Prefs;

    fn setup(
        cx: &mut TestAppContext,
    ) -> (Entity<MainView>, Arc<FakeBackend>, &mut VisualTestContext) {
        let config = "blyg-url = https://blyg.example.com\n";
        let prefs = Prefs::from_config(ConfigStore::in_memory(config).config());
        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::app::bind_keys(cx);
            crate::settings::init(
                ConfigStore::in_memory(config),
                Arc::new(blyg_core::config::MemoryTokenStore::default()),
                None,
                cx,
            );
        });
        let fake = Arc::new(FakeBackend::with_timing(Timing::instant()).without_media_cache());
        let backend: Arc<dyn Backend> = fake.clone();
        let f2 = fake.clone();
        let (view, cx) = cx.add_window_view(move |window, cx| {
            MainView::new(
                backend,
                Some(f2),
                prefs,
                std::time::Instant::now(),
                window,
                cx,
            )
        });
        cx.run_until_parked();
        (view, fake, cx)
    }

    #[gpui_kit::test]
    fn operators_filter_the_list_and_alone_create_nothing(cx: &mut TestAppContext) {
        let (view, fake, cx) = setup(cx);
        let drafts = fake
            .items()
            .iter()
            .filter(|i| i.status == Status::Draft)
            .count();
        assert!(drafts > 0, "the sample has drafts");
        cx.simulate_input("is:draft");
        cx.run_until_parked();
        view.read_with(cx, |v, _| {
            assert_eq!(v.list.results().len(), drafts);
            assert!(v.list.results().iter().all(|i| i.status == Status::Draft));
        });
        // Nothing matches: ⏎ creates nothing.
        let before = fake.items().len();
        cx.simulate_input(" kind:fragment v:99");
        cx.run_until_parked();
        view.read_with(cx, |v, _| {
            assert!(v.list.results().is_empty());
            assert!(!v.list.wants_create());
            assert_eq!(
                v.list.empty_message().as_deref(),
                Some("Nothing loose here.")
            );
        });
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        assert_eq!(fake.items().len(), before);
    }

    #[gpui_kit::test]
    fn the_summary_is_words_and_a_click_filters(cx: &mut TestAppContext) {
        let (view, _fake, cx) = setup(cx);
        view.update(cx, |v, cx| v.loose_ends_refresh(cx));
        cx.run_until_parked();
        let words = view.read_with(cx, |v, _| v.loose_ends.summary.words());
        assert!(!words.is_empty(), "the sample has loose ends");
        assert!(!words.chars().any(|c| c == '%'), "{words}");
        let (_, query) = view.read_with(cx, |v, _| v.loose_ends.summary.parts()[0].clone());
        let sel: &'static str = match query {
            "is:orphan" => "loose-is:orphan",
            "has:tk" => "loose-has:tk",
            _ => "loose-is:stale",
        };
        let at = cx.debug_bounds(sel).expect("the summary is drawn").center();
        cx.simulate_click(at, gpui_kit::Modifiers::none());
        cx.run_until_parked();
        view.read_with(cx, |v, _| assert_eq!(v.list.query(), query));
    }
}
