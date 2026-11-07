//! --- site --- Headless GPUI tests of Blyg › Site… on a zero-latency
//! FakeBackend (whose "server" refuses what Studio refuses).

use std::sync::Arc;

use blyg_core::config::MemoryTokenStore;
use blyg_core::{Backend, ConfigStore};
use gpui_kit::{Entity, TestAppContext, VisualTestContext};

use super::{Load, Section, ShowSite};
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

fn settle(cx: &mut VisualTestContext) {
    for _ in 0..20 {
        cx.run_until_parked();
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}

fn open(view: &Entity<MainView>, cx: &mut VisualTestContext) {
    cx.dispatch_action(ShowSite);
    settle(cx);
    assert!(view.read_with(cx, |v, _| {
        v.site.sheet.as_ref().is_some_and(|s| s.load == Load::Ready)
    }));
}

#[gpui_kit::test]
fn every_section_draws(cx: &mut TestAppContext) {
    let (view, _, cx) = setup(cx);
    open(&view, cx);
    for s in Section::ALL {
        view.update(cx, |v, cx| {
            v.site.sheet.as_mut().unwrap().section = s;
            cx.notify();
        });
        cx.run_until_parked();
    }
    // A custom theme with a dark companion draws its editor and preview.
    view.update_in(cx, |v, window, cx| {
        v.site.sheet.as_mut().unwrap().section = Section::Appearance;
        v.site_choose_theme("custom", window, cx);
        v.site_toggle_dark_companion(window, cx);
    });
    cx.run_until_parked();
    // Esc closes it.
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(view.read_with(cx, |v, _| v.site.sheet.is_none()));
}

#[gpui_kit::test]
fn nothing_goes_until_save_then_only_the_changes(cx: &mut TestAppContext) {
    let (view, fake, cx) = setup(cx);
    open(&view, cx);
    view.update_in(cx, |v, window, cx| {
        v.site_choose_theme("custom", window, cx);
        v.site_edit(cx, |s| {
            s.form.font = Some("slab".into());
            s.form.hopper_feeds = Some(true);
            s.form.links_position = Some("bottom".into());
            s.form.extensions = Some(vec!["inspect".into()]);
        });
        let s = v.site.sheet.as_ref().unwrap();
        s.posts_path
            .update(cx, |st, cx| st.set_value("writing", window, cx));
        s.theme_name
            .update(cx, |st, cx| st.set_value("Pink Quill", window, cx));
        s.light[2].update(cx, |st, cx| st.set_value("#331122", window, cx));
    });
    settle(cx);
    // Not saved yet.
    assert_eq!(fake.site_config().unwrap().theme.as_deref(), Some("auto"));
    view.update_in(cx, |v, window, cx| v.save_site(window, cx));
    settle(cx);
    let c = fake.site_config().unwrap();
    assert_eq!(c.theme.as_deref(), Some("custom"));
    assert_eq!(c.custom_theme_name.as_deref(), Some("Pink Quill"));
    assert_eq!(c.custom_theme.clone().flatten().unwrap().ink, "#331122");
    assert_eq!(c.font.as_deref(), Some("slab"));
    assert_eq!(c.posts_path.as_deref(), Some("writing"));
    assert_eq!(c.hopper_feeds, Some(true));
    assert_eq!(c.links_position.as_deref(), Some("bottom"));
    assert!(
        c.server_extensions
            .iter()
            .any(|e| e.name == "inspect" && e.enabled)
    );
    view.read_with(cx, |v, _| {
        let s = v.site.sheet.as_ref().unwrap();
        assert!(s.saved && s.error.is_none(), "{:?}", s.error);
    });
}

#[gpui_kit::test]
fn a_bad_posts_path_is_caught_before_it_is_sent(cx: &mut TestAppContext) {
    let (view, fake, cx) = setup(cx);
    open(&view, cx);
    view.update_in(cx, |v, window, cx| {
        let s = v.site.sheet.as_ref().unwrap();
        s.posts_path
            .update(cx, |st, cx| st.set_value("feed", window, cx));
        v.save_site(window, cx);
    });
    settle(cx);
    let err = view.read_with(cx, |v, _| v.site.sheet.as_ref().unwrap().error.clone());
    assert!(err.unwrap().contains("already uses"));
    assert_eq!(
        fake.site_config().unwrap().posts_path.as_deref(),
        Some("posts")
    );
}

#[gpui_kit::test]
fn a_refusal_shows_the_servers_words(cx: &mut TestAppContext) {
    let (view, _, cx) = setup(cx);
    open(&view, cx);
    // A front page that isn't a published thread: the "server" refuses.
    view.update_in(cx, |v, window, cx| {
        v.site_edit(cx, |s| {
            s.form.front_page = Some("01nosuchthread000000000000".into())
        });
        v.save_site(window, cx);
    });
    settle(cx);
    let err = view.read_with(cx, |v, _| v.site.sheet.as_ref().unwrap().error.clone());
    assert_eq!(
        err.as_deref(),
        Some("Your blyg refused it: frontPage must be a published thread")
    );
}

#[gpui_kit::test]
fn a_published_thread_can_be_the_front_page_and_in_the_menu(cx: &mut TestAppContext) {
    let (view, fake, cx) = setup(cx);
    open(&view, cx);
    let threads = super::published_threads(&fake.items());
    let Some((id, _)) = threads.first().cloned() else {
        panic!("the sample blyg has a published thread");
    };
    view.update_in(cx, |v, window, cx| {
        v.site.sheet.as_mut().unwrap().picking = Some(super::Pick::FrontPage);
        v.site_pick_thread(id.clone(), cx);
        v.site.sheet.as_mut().unwrap().picking = Some(super::Pick::MenuThread);
        v.site_pick_thread(id.clone(), cx);
        v.save_site(window, cx);
    });
    settle(cx);
    let c = fake.site_config().unwrap();
    assert_eq!(c.front_page.as_deref(), Some(id.as_str()));
    assert_eq!(c.links_threads, Some(vec![id]));
}

#[gpui_kit::test]
fn turning_update_alerts_off_hides_the_notice(cx: &mut TestAppContext) {
    let (view, fake, cx) = setup(cx);
    assert!(fake.studio_update().is_some());
    open(&view, cx);
    view.update_in(cx, |v, window, cx| {
        v.site_edit(cx, |s| s.form.update_check = Some(false));
        v.save_site(window, cx);
    });
    settle(cx);
    assert_eq!(fake.studio_update(), None);
}
