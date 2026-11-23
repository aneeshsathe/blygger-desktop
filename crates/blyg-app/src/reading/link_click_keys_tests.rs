//! --- browse --- A ⌘-click on a link, as each platform presses it: Ctrl
//! on Windows. ⌘ keeps a link that names a blyg item a link (it opens in
//! the browser pane over the whole reading area) instead of opening the
//! post in the reader. The stream read GPUI's `platform` modifier, the
//! Windows key there, so a Ctrl+click opened the post; and `open_link`'s
//! last resort for a click's keys read it too.

use std::sync::Arc;

use blyg_core::config::MemoryTokenStore;
use blyg_core::{Backend, ConfigStore, Kind, RemoteId};
use gpui_kit::{Entity, Modifiers, TestAppContext, VisualTestContext, point, px};

use super::stream_vm::ReadMode;
use crate::app::MainView;
use crate::app::browser::OpenMode;
use crate::fake::reading_seed::*;
use crate::fake::{FakeBackend, Timing};
use crate::platform_text::Platform;
use crate::prefs::Prefs;
use crate::test_wait::settle_drawn as settle;

const CONNECTED: &str = "# test config\nblyg-url = https://blyg.example.com\n";

/// A fragment in the stream whose whole text is one link to Ada's "Tides".
const LINKER: &str = "01k2l1nk0000000000000000001";

fn tides() -> String {
    format!("{ADA}f/{ADA_TIDES}/")
}

fn setup(
    p: Platform,
    cx: &mut TestAppContext,
) -> (Entity<MainView>, Arc<FakeBackend>, &mut VisualTestContext) {
    let prefs = Prefs::from_config(ConfigStore::in_memory(CONNECTED).config());
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::windows_keys_tests::bind_keys_as(p, cx);
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

/// The linking fragment, on top of the stream and selected (on screen).
fn a_post_with_a_link(view: &Entity<MainView>, fake: &FakeBackend, cx: &mut VisualTestContext) {
    fake.with_reading(|rows| {
        let mut r = rows
            .iter()
            .find(|r| r.origin == ADA)
            .expect("a post of Ada's")
            .clone();
        r.remote_id = RemoteId::from(LINKER);
        r.kind = Kind::Fragment;
        r.content_md = format!("[Tides]({})", tides());
        r.content_html.clear();
        rows.insert(0, r);
    });
    view.update_in(cx, |v, window, cx| {
        v.set_read_mode(ReadMode::Stream, window, cx)
    });
    settle(cx);
    let ix = view
        .read_with(cx, |v, _| {
            v.reading.shown_rows().position(|r| r.remote_id == LINKER)
        })
        .expect("the post is in the stream");
    view.update_in(cx, |v, window, cx| v.stream_select(ix, window, cx));
    settle(cx);
}

/// Click the start of the post's link with `m` held.
fn click_the_link(m: Modifiers, cx: &mut VisualTestContext) {
    let sel: &'static str = Box::leak(format!("stream-text-{LINKER}-1").into_boxed_str());
    let b = crate::test_wait::still(cx, sel);
    let at = point(b.origin.x + px(4.), b.origin.y + b.size.height / 2.);
    cx.simulate_click(at, m);
    settle(cx);
}

fn cmd(p: Platform) -> Modifiers {
    match p {
        Platform::Mac => Modifiers {
            platform: true,
            ..Default::default()
        },
        Platform::Windows => Modifiers {
            control: true,
            ..Default::default()
        },
    }
}

fn opened(view: &Entity<MainView>, cx: &mut VisualTestContext) -> Option<String> {
    view.read_with(cx, |v, _| {
        v.reading
            .opened
            .as_ref()
            .map(|o| o.item.remote_id.to_string())
    })
}

/// (open, mode, page) of the browser pane.
fn pane(view: &Entity<MainView>, cx: &mut VisualTestContext) -> (bool, OpenMode, String) {
    view.read_with(cx, |v, _| {
        (v.browser.open, v.browser.mode, v.browser.page.url.clone())
    })
}

fn cmd_click_keeps_a_blyg_link_a_link_with(p: Platform, cx: &mut TestAppContext) {
    let (view, fake, cx) = setup(p, cx);
    a_post_with_a_link(&view, &fake, cx);
    click_the_link(cmd(p), cx);
    assert_eq!(
        pane(&view, cx),
        (true, OpenMode::Full, tides()),
        "{p:?}: a {:?}-click opens the link over the whole reading area",
        cmd(p)
    );
    assert_ne!(
        opened(&view, cx).as_deref(),
        Some(ADA_TIDES),
        "{p:?}: and not the post in the reader"
    );
}

#[gpui_kit::test]
fn ctrl_click_keeps_a_blyg_link_a_link_on_the_windows_key_table(cx: &mut TestAppContext) {
    cmd_click_keeps_a_blyg_link_a_link_with(Platform::Windows, cx);
}

#[gpui_kit::test]
fn cmd_click_keeps_a_blyg_link_a_link_on_the_mac_key_table(cx: &mut TestAppContext) {
    cmd_click_keeps_a_blyg_link_a_link_with(Platform::Mac, cx);
}

/// The click lands on the link: a plain one opens the post it names.
#[gpui_kit::test]
fn a_plain_click_on_a_blyg_link_opens_the_post(cx: &mut TestAppContext) {
    let (view, fake, cx) = setup(Platform::Windows, cx);
    a_post_with_a_link(&view, &fake, cx);
    click_the_link(Modifiers::none(), cx);
    assert_eq!(opened(&view, cx).as_deref(), Some(ADA_TIDES));
    assert!(!pane(&view, cx).0, "no pane");
}

/// `open_link`'s last resort, the keys the window last saw (a link
/// followed outside a web view, on a Mac or Linux: Windows reads the
/// keyboard itself, `platform::held_link_modifiers`).
fn open_link_reads_the_held_keys_with(p: Platform, cx: &mut TestAppContext) {
    let (view, _, cx) = setup(p, cx);
    let url = "https://blyg.example.com/f/abc/";
    cx.simulate_modifiers_change(cmd(p));
    view.update_in(cx, |v, window, cx| v.open_link(url.into(), window, cx));
    settle(cx);
    assert_eq!(
        pane(&view, cx),
        (true, OpenMode::Full, url.to_string()),
        "{p:?}: {:?} held",
        cmd(p)
    );
}

#[gpui_kit::test]
fn open_link_reads_ctrl_on_the_windows_key_table(cx: &mut TestAppContext) {
    open_link_reads_the_held_keys_with(Platform::Windows, cx);
}

#[gpui_kit::test]
fn open_link_reads_cmd_on_the_mac_key_table(cx: &mut TestAppContext) {
    open_link_reads_the_held_keys_with(Platform::Mac, cx);
}
