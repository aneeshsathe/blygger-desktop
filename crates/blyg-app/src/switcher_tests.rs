//! The blyg switcher in a headless window: two blygs, each its own
//! `FakeBackend` behind the app's `SwitchBackend`, switched by the sheet,
//! the keys and the menu actions.

use std::sync::Arc;

use blyg_core::config::{MemoryTokenStore, TokenStore};
use blyg_core::{Backend, ConfigStore, Kind, LocalId};
use gpui_kit::{Entity, TestAppContext, VisualTestContext};

use super::super::{MainView, Sheet, reading};
use super::{ShowBlygSwitcher, SwitchBlyg1, SwitchBlyg2};
use crate::connection::{BlygOpener, Connection, Mode, SwitchBackend};
use crate::fake::{FakeBackend, Timing};
use crate::prefs::Prefs;

const A: &str = "https://blyg.example.com";
const B: &str = "https://notes.example.org";
const TWO: &str = "blyg-url = https://blyg.example.com\nblyg = https://blyg.example.com\nblyg = https://notes.example.org\n";

struct World {
    a: Arc<FakeBackend>,
    b: Arc<FakeBackend>,
    tokens: Arc<MemoryTokenStore>,
    _dir: tempfile::TempDir,
}

fn setup(cx: &mut TestAppContext) -> (Entity<MainView>, World, &mut VisualTestContext) {
    let a = Arc::new(FakeBackend::with_timing(Timing::instant()).without_media_cache());
    let b = Arc::new(FakeBackend::with_timing(Timing::instant()).without_media_cache());
    b.replace_posts(&[
        (Kind::Fragment, "Heron at the weir"),
        (Kind::Fragment, "Moss grows where nobody walks"),
    ]);
    let tokens = Arc::new(MemoryTokenStore::default());
    tokens.set(A, "tok-a").unwrap();
    tokens.set(B, "tok-b").unwrap();
    let dir = tempfile::tempdir().unwrap();
    let switch = SwitchBackend::new(a.clone(), Mode::Fake);
    let backend: Arc<dyn Backend> = switch.clone();
    let prefs = Prefs::from_config(ConfigStore::in_memory(TWO).config());
    {
        let (a, b, tokens, data_dir) = (
            a.clone(),
            b.clone(),
            tokens.clone(),
            dir.path().to_path_buf(),
        );
        cx.update(move |cx| {
            gpui_kit::init(cx);
            super::super::bind_keys(cx);
            crate::settings::init(ConfigStore::in_memory(TWO), tokens, None, cx);
            cx.set_global(Connection {
                switch,
                data_dir,
                verify: crate::connection::fake_verifier,
            });
            cx.set_global(BlygOpener(Arc::new(move |url: &str| {
                let f = if url.contains("notes.") {
                    b.clone()
                } else {
                    a.clone()
                };
                Ok(Some((f as Arc<dyn Backend>, Mode::Fake)))
            })));
        });
    }
    let f = a.clone();
    let (view, cx) = cx.add_window_view(move |window, cx| {
        MainView::new(
            backend,
            Some(f),
            prefs,
            std::time::Instant::now(),
            window,
            cx,
        )
    });
    cx.run_until_parked();
    (
        view,
        World {
            a,
            b,
            tokens,
            _dir: dir,
        },
        cx,
    )
}

fn config_text(cx: &mut VisualTestContext) -> String {
    cx.update(|_, cx| {
        crate::settings::get(cx)
            .store
            .text()
            .unwrap_or_default()
            .to_string()
    })
}

fn active(cx: &mut VisualTestContext) -> Option<String> {
    cx.update(|_, cx| crate::settings::blyg_url(cx))
}

fn posts(view: &Entity<MainView>, cx: &mut VisualTestContext) -> usize {
    view.read_with(cx, |v, _| v.list.results().len())
}

#[gpui_kit::test]
fn the_switcher_lists_both_blygs_and_switching_reloads_the_views(cx: &mut TestAppContext) {
    let (view, w, cx) = setup(cx);
    assert_eq!(posts(&view, cx), 6, "blyg A's sample posts");

    // The title is the switcher's button.
    cx.update(|_, cx| {
        view.update(cx, |v, cx| {
            let ui: gpui_kit::SharedString = "Inter".into();
            assert!(v.render_blyg_title(&ui, cx).is_some());
        })
    });

    cx.dispatch_action(ShowBlygSwitcher);
    cx.run_until_parked();
    view.read_with(cx, |v, _| {
        let Some(Sheet::Blygs(s)) = &v.sheet else {
            panic!("the switcher sheet is open");
        };
        let hosts: Vec<&str> = s.entries.iter().map(|e| e.host.as_str()).collect();
        assert_eq!(hosts, ["blyg.example.com", "notes.example.org"]);
        assert!(s.entries[0].active && !s.entries[1].active);
        assert_eq!(s.sel, 0, "the active blyg is highlighted");
    });

    // The reading screen stays on screen across the switch, reloaded.
    view.update_in(cx, |v, window, cx| {
        v.sheet = None;
        v.show_view(reading::View::Reading, window, cx);
    });
    cx.dispatch_action(ShowBlygSwitcher);
    cx.run_until_parked();
    cx.simulate_keystrokes("2");
    cx.run_until_parked();
    assert_eq!(active(cx).as_deref(), Some(B));
    assert!(config_text(cx).contains("blyg-url = https://notes.example.org"));
    view.read_with(cx, |v, _| {
        assert!(v.sheet.is_none(), "the switcher closed");
        assert_eq!(v.reading.view, reading::View::Reading);
        assert_eq!(v.reading.rows.len(), w.b.reading().len());
        assert_eq!(
            v.toast.as_ref().map(|t| t.text.to_string()).as_deref(),
            Some("Switched to notes.example.org")
        );
    });
    // Back to the posts list: blyg B's own posts.
    view.update_in(cx, |v, window, cx| {
        v.show_view(reading::View::Posts, window, cx)
    });
    cx.run_until_parked();
    assert_eq!(posts(&view, cx), 2);
    assert!(view.read_with(cx, |v, _| {
        v.list
            .results()
            .iter()
            .all(|i| w.b.item(&i.local_id).is_some())
    }));

    // ⌃⌘1 goes back to A; the list order didn't change.
    cx.dispatch_action(SwitchBlyg1);
    cx.run_until_parked();
    assert_eq!(active(cx).as_deref(), Some(A));
    assert_eq!(posts(&view, cx), 6);
    assert!(
        config_text(cx)
            .contains("blyg = https://blyg.example.com\nblyg = https://notes.example.org")
    );
    // Switching to the active blyg does nothing.
    cx.dispatch_action(SwitchBlyg1);
    cx.run_until_parked();
    assert_eq!(active(cx).as_deref(), Some(A));
    assert!(w.tokens.get(B).unwrap().is_some());
}

#[gpui_kit::test]
fn switching_never_loses_the_editors_text(cx: &mut TestAppContext) {
    let (view, w, cx) = setup(cx);
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    cx.simulate_input(" Weather again.");
    cx.run_until_parked();
    cx.dispatch_action(SwitchBlyg2);
    cx.run_until_parked();
    assert_eq!(active(cx).as_deref(), Some(B));
    assert!(
        w.a.item(&LocalId("01J9QK3".into()))
            .unwrap()
            .content_md
            .ends_with("Weather again."),
        "blyg A kept the edit"
    );
    // Nothing of A's leaked into B, and the editor no longer shows A's post.
    assert!(
        w.b.items()
            .iter()
            .all(|i| !i.content_md.contains("Weather again."))
    );
    view.read_with(cx, |v, cx| {
        assert!(
            v.current
                .as_ref()
                .is_none_or(|c| w.b.item(&c.local_id).is_some())
        );
        assert!(!v.editor.read(cx).value().contains("Weather again."));
    });
}

#[gpui_kit::test]
fn manage_signs_out_of_another_blyg(cx: &mut TestAppContext) {
    let (view, w, cx) = setup(cx);
    cx.dispatch_action(super::ManageBlygs);
    cx.run_until_parked();
    cx.simulate_keystrokes("down enter");
    cx.run_until_parked();
    view.read_with(cx, |v, _| {
        let Some(Sheet::Blygs(s)) = &v.sheet else {
            panic!("still open");
        };
        assert!(s.manage);
        assert_eq!(s.confirm, Some(1), "asks before signing out");
    });
    // esc backs out of the question, not the sheet.
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(view.read_with(
        cx,
        |v, _| matches!(&v.sheet, Some(Sheet::Blygs(s)) if s.confirm.is_none())
    ));
    assert!(w.tokens.get(B).unwrap().is_some());
    cx.simulate_keystrokes("enter 1");
    cx.run_until_parked();
    assert_eq!(w.tokens.get(B).unwrap(), None, "B's sign-in is gone");
    assert!(w.tokens.get(A).unwrap().is_some(), "A's stays");
    assert_eq!(active(cx).as_deref(), Some(A));
    let text = config_text(cx);
    assert!(!text.contains("notes.example.org"), "{text}");
    assert!(
        !text.contains("blyg = "),
        "back to one blyg: no list\n{text}"
    );
    view.read_with(cx, |v, _| {
        let Some(Sheet::Blygs(s)) = &v.sheet else {
            panic!("still open");
        };
        assert_eq!(s.entries.len(), 1);
    });
    assert_eq!(posts(&view, cx), 6, "still on A");
}

#[gpui_kit::test]
fn signing_out_of_the_active_blyg_switches_to_the_next(cx: &mut TestAppContext) {
    let (view, w, cx) = setup(cx);
    cx.dispatch_action(super::super::Disconnect);
    cx.run_until_parked();
    cx.simulate_keystrokes("1");
    cx.run_until_parked();
    assert_eq!(w.tokens.get(A).unwrap(), None);
    assert_eq!(active(cx).as_deref(), Some(B));
    assert_eq!(posts(&view, cx), 2, "B's posts");
    assert!(
        view.read_with(cx, |v, _| !matches!(v.sheet, Some(Sheet::Connect { .. }))),
        "no Connect sheet while a blyg is left"
    );
}
