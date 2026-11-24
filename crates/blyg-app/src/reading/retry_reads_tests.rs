//! --- studio 0.71 --- "Retry reads": a read that failed says so with a
//! Retry reads link (and Blyg › Retry Reads), which reads it again and also
//! re-reads the lists on screen, as the studio's "retry reads" now reloads
//! the page's lists. Headless GPUI on the zero-latency FakeBackend.

use std::sync::Arc;

use blyg_core::config::MemoryTokenStore;
use blyg_core::{Backend, ConfigStore};
use gpui_kit::{Entity, Modifiers, TestAppContext, VisualTestContext};

use super::retry_reads::RetryReads;
use super::{Load, View};
use crate::app::MainView;
use crate::fake::{FakeBackend, Timing};
use crate::prefs::Prefs;

const CONNECTED: &str = "# test config\nblyg-url = https://blyg.example.com\n";

fn setup(cx: &mut TestAppContext) -> (Entity<MainView>, Arc<FakeBackend>, &mut VisualTestContext) {
    let prefs = Prefs::from_config(ConfigStore::in_memory(CONNECTED).config());
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::app::bind_keys(cx);
        crate::settings::init(
            ConfigStore::in_memory(CONNECTED),
            Arc::new(MemoryTokenStore::default()),
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

fn failed(view: &Entity<MainView>, cx: &mut VisualTestContext) -> bool {
    view.read_with(cx, |v, _| matches!(v.reading.mentions, Load::Failed(_)))
}

fn refreshes(view: &Entity<MainView>, cx: &mut VisualTestContext) -> u64 {
    view.read_with(cx, |v, _| v.reading.refreshes)
}

/// Responses that couldn't be read say so, with Retry reads beside the
/// note. Offline, retrying fails again and says why; back online, it reads
/// them, and the reading lists are read again too.
#[gpui_kit::test]
fn retry_reads_reads_the_failed_read_and_the_lists_again(cx: &mut TestAppContext) {
    let (view, fake, cx) = setup(cx);
    fake.set_offline(true);
    view.update_in(cx, |v, window, cx| v.show_view(View::Mentions, window, cx));
    crate::test_wait::until(cx, "the responses to fail", |cx| failed(&view, cx));
    crate::test_wait::settle_drawn(cx);
    assert!(
        cx.debug_bounds("retry-reads").is_some(),
        "the failed read offers Retry reads"
    );

    // Still offline: it fails again, and says so.
    let before = refreshes(&view, cx);
    let at = crate::test_wait::still(cx, "retry-reads");
    cx.simulate_click(at.center(), Modifiers::none());
    crate::test_wait::until(cx, "the retry to fail again", |cx| {
        !view.read_with(cx, |v, _| v.reading.retrying) && failed(&view, cx)
    });
    let toast = view.read_with(cx, |v, _| v.toast.as_ref().map(|t| t.text.to_string()));
    assert!(
        toast
            .as_deref()
            .is_some_and(|t| t.starts_with("Couldn't read")),
        "{toast:?}"
    );

    // Online: the action reads the responses and the lists again.
    fake.set_offline(false);
    view.update_in(cx, |_, window, cx| {
        window.dispatch_action(Box::new(RetryReads), cx)
    });
    crate::test_wait::until(cx, "the responses read", |cx| {
        view.read_with(cx, |v, _| {
            matches!(v.reading.mentions, Load::Ready(_)) && !v.reading.retrying
        })
    });
    assert!(
        refreshes(&view, cx) > before,
        "the reading lists were read again"
    );
    crate::test_wait::settle_drawn(cx);
    assert!(
        cx.debug_bounds("retry-reads").is_none(),
        "nothing failed now"
    );
}
