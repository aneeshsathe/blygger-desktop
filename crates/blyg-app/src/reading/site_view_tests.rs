//! --- reader view --- "Open posts on the author's site" in the reading
//! layout, on a stub web view: the setting, the per-view toggle, the
//! subscription's override, j/k, the no-page fallback, and read marks.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use blyg_core::config::MemoryTokenStore;
use blyg_core::{Backend, ConfigStore};
use gpui_kit::{Bounds, Entity, Pixels, TestAppContext, VisualTestContext};

use super::site_view::Shows;
use crate::app::MainView;
use crate::app::browser::surface::{BrowserSurface, FactoryGlobal, PageState};
use crate::fake::reading_seed::*;
use crate::fake::{FakeBackend, Timing};
use crate::prefs::Prefs;

#[derive(Default)]
struct Log {
    loads: Vec<String>,
    created: usize,
}

struct Stub(Rc<RefCell<Log>>);

impl BrowserSurface for Stub {
    fn set_frame(&mut self, _: Bounds<Pixels>) {}
    fn set_visible(&mut self, _: bool) {}
    fn load_url(&mut self, url: &str) {
        self.0.borrow_mut().loads.push(url.to_string());
    }
    fn back(&mut self) {}
    fn forward(&mut self) {}
    fn reload(&mut self) {}
    fn stop(&mut self) {}
    fn state(&self) -> PageState {
        PageState::default()
    }
    fn refresh_blocking(&mut self) {}
    fn focus_parent(&mut self) {}
}

fn setup<'a>(
    cx: &'a mut TestAppContext,
    config: &str,
) -> (
    Entity<MainView>,
    Arc<FakeBackend>,
    &'a mut VisualTestContext,
    Rc<RefCell<Log>>,
) {
    let config = format!("blyg-url = https://blyg.example.com\n{config}");
    let prefs = Prefs::from_config(ConfigStore::in_memory(&config).config());
    let log = Rc::new(RefCell::new(Log::default()));
    let l = log.clone();
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::app::bind_keys(cx);
        crate::settings::init(
            ConfigStore::in_memory(&config),
            Arc::new(MemoryTokenStore::default()),
            None,
            cx,
        );
        cx.set_global(FactoryGlobal(Rc::new(move |_, _, _, _| {
            l.borrow_mut().created += 1;
            Ok(Box::new(Stub(l.clone())) as Box<dyn BrowserSurface>)
        })));
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
    view.update(cx, |v, _| {
        v.reading.mode = super::stream_vm::ReadMode::Reader
    });
    (view, fake, cx, log)
}

fn settle(cx: &mut VisualTestContext) {
    for _ in 0..20 {
        cx.run_until_parked();
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}

fn open_row(view: &Entity<MainView>, id: &str, cx: &mut VisualTestContext) {
    let key = view.read_with(cx, |v, _| {
        v.reading_rows()
            .iter()
            .find(|r| r.remote_id == id)
            .map(super::vm::key)
            .expect("row shown")
    });
    view.update_in(cx, |v, window, cx| v.open_reading(key, window, cx));
    settle(cx);
}

fn page_of(origin: &str, kind: &str, id: &str) -> String {
    format!("{origin}{kind}/{id}")
}

#[gpui_kit::test]
fn off_by_default_the_reader_shows_posts(cx: &mut TestAppContext) {
    let (view, _, cx, log) = setup(cx, "");
    cx.simulate_keystrokes(&crate::keymap::keys("cmd-r"));
    cx.run_until_parked();
    open_row(&view, RUE_TRUST, cx);
    view.read_with(cx, |v, _| {
        assert!(!v.prefs.author_site);
        assert!(!v.site_view_shown());
        assert!(v.reader_doc().is_some(), "Burrow's reader, as before");
    });
    assert_eq!(log.borrow().created, 0, "no web view made");
}

#[gpui_kit::test]
fn on_a_post_opens_on_its_page_and_j_k_still_move(cx: &mut TestAppContext) {
    let (view, fake, cx, log) = setup(cx, "open-posts-on-site = true\n");
    cx.simulate_keystrokes(&crate::keymap::keys("cmd-r"));
    cx.run_until_parked();
    cx.simulate_keystrokes("down");
    settle(cx);
    let first = view.read_with(cx, |v, _| {
        let o = v.reading.opened.as_ref().unwrap();
        assert!(v.site_view_shown());
        assert!(v.reader_doc().is_none(), "the reader's web view gives way");
        assert!(v.reading.site.visible());
        o.item.remote_id.clone()
    });
    assert_eq!(first, RUE_TRUST);
    assert_eq!(log.borrow().created, 1);
    assert_eq!(
        log.borrow().loads,
        vec![page_of(RUE, "f", RUE_TRUST)],
        "its own page"
    );
    // It still counts as read.
    let read = fake
        .reading()
        .into_iter()
        .find(|r| r.remote_id == RUE_TRUST)
        .unwrap()
        .read_version;
    assert_eq!(read, Some(5));
    // The next post: its page, in the same web view.
    cx.simulate_keystrokes("down");
    settle(cx);
    let second = view.read_with(cx, |v, _| v.reading.opened.as_ref().unwrap().item.clone());
    assert_ne!(second.remote_id, RUE_TRUST);
    assert_eq!(log.borrow().created, 1, "one web view");
    assert_eq!(
        log.borrow().loads.last(),
        super::site_view::page_url(&second).as_ref()
    );

    // The toggle: Burrow's reader for this one view…
    view.update(cx, |v, cx| v.set_site_for_view(false, cx));
    settle(cx);
    view.read_with(cx, |v, _| {
        assert!(!v.site_view_shown());
        assert!(v.reader_doc().is_some());
    });
    // …and the next post opens on its page again.
    cx.simulate_keystrokes("up");
    settle(cx);
    view.read_with(cx, |v, _| {
        assert_eq!(v.reading.opened.as_ref().unwrap().item.remote_id, RUE_TRUST);
        assert!(v.site_view_shown());
    });
}

#[gpui_kit::test]
fn a_subscription_can_say_never_and_no_page_falls_back(cx: &mut TestAppContext) {
    let (view, _, cx, _log) = setup(cx, "open-posts-on-site = true\n");
    cx.simulate_keystrokes(&crate::keymap::keys("cmd-r"));
    cx.run_until_parked();
    open_row(&view, RUE_TRUST, cx);
    let sub = view.read_with(cx, |v, _| {
        v.reading
            .opened
            .as_ref()
            .unwrap()
            .item
            .subscription_id
            .clone()
    });
    view.update(cx, |v, cx| v.set_site_for_sub(sub.clone(), Some(false), cx));
    settle(cx);
    view.read_with(cx, |v, _| {
        assert!(!v.site_view_shown(), "never, for this subscription");
        let o = v.reading.opened.as_ref().unwrap();
        assert_eq!(v.site_shows(o), Shows::Reader);
    });
    // A post with no page: Burrow's reader, quietly.
    view.update(cx, |v, cx| {
        v.set_site_for_sub(sub, None, cx);
        if let Some(o) = v.reading.opened.as_mut() {
            o.item.page = None;
        }
    });
    settle(cx);
    view.read_with(cx, |v, _| {
        let o = v.reading.opened.as_ref().unwrap();
        assert_eq!(v.site_shows(o), Shows::NoPage);
        assert!(!v.site_view_shown());
        assert!(v.reader_doc().is_some());
    });
}
