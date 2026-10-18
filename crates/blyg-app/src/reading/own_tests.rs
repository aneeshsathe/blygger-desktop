//! --- own posts --- Headless GPUI tests: your published posts as a
//! reading source, and the reading actions on them from both views, on a
//! zero-latency FakeBackend (its own posts are invented sample posts on
//! blyg.example.com).

use std::sync::Arc;

use blyg_core::config::MemoryTokenStore;
use blyg_core::{Backend, ConfigStore, Kind, LocalId};
use gpui_kit::{Entity, TestAppContext, VisualTestContext};

use super::own::{ForkPost, LinkToPost, OWN, QuotePost, ReplyToPost};
use super::sources_vm::{Smart, Source};
use super::{Load, View};
use crate::app::MainView;
use crate::fake::reading_seed::*;
use crate::fake::{FakeBackend, Timing};
use crate::prefs::Prefs;

const CONNECTED: &str = "# test config\nblyg-url = https://blyg.example.com\n";
/// Your origin as a citation spells it.
const ME: &str = "https://blyg.example.com/";
/// "On friction" in the Posts list (v1, 📌 v2, v3 current).
const FRICTION: &str = "01J9M2A";
const FRICTION_V2: &str =
    "On friction: each step between noticing and writing is a place ideas die.";
/// A draft of yours.
const DRAFT: &str = "01J9QK3";

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
    view.update(cx, |v, _| {
        v.reading.mode = super::stream_vm::ReadMode::Reader
    });
    (view, fake, cx)
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

fn open_post(view: &Entity<MainView>, id: &str, cx: &mut VisualTestContext) {
    view.update_in(cx, |v, window, cx| v.open(&LocalId(id.into()), window, cx));
    cx.run_until_parked();
}

fn new_items(fake: &FakeBackend, before: &[blyg_core::Item]) -> Vec<blyg_core::Item> {
    fake.items()
        .into_iter()
        .filter(|i| !before.iter().any(|b| b.local_id == i.local_id))
        .collect()
}

fn no_own_sends(fake: &FakeBackend) {
    let sent = fake.reading_sends();
    assert!(
        sent.iter().all(|s| !s.contains(&format!(":{OWN}:"))),
        "nothing about your own posts is sent: {sent:?}"
    );
}

#[gpui_kit::test]
fn your_posts_are_a_source_never_unread_and_nothing_is_sent(cx: &mut TestAppContext) {
    let (view, fake, cx) = setup(cx);
    let unread_before: usize = fake.reading().iter().filter(|r| r.is_unread()).count();
    cx.simulate_keystrokes(&crate::keymap::keys("cmd-r"));
    settle(cx);
    let own: Vec<String> = view.read_with(cx, |v, _| {
        v.reading_rows()
            .iter()
            .filter(|r| r.subscription_id == OWN)
            .map(|r| r.remote_id.clone())
            .collect()
    });
    // Your four published sample posts; drafts aren't reading rows.
    assert_eq!(own.len(), 4, "{own:?}");
    assert!(own.contains(&OWN_FRICTION.to_string()) && own.contains(&OWN_GARDENS.to_string()));
    view.read_with(cx, |v, _| {
        assert!(
            v.reading_rows()
                .iter()
                .filter(|r| r.subscription_id == OWN)
                .all(|r| { !r.is_unread() && r.thumb.is_none() && r.origin == ME })
        );
        // The unread count is the backend's alone.
        assert_eq!(
            v.reading_rows().iter().filter(|r| r.is_unread()).count(),
            unread_before
        );
    });

    // "Your blyg" lists them alone; Thumbed and All unread never do.
    let shown = |view: &Entity<MainView>, cx: &mut VisualTestContext| -> Vec<String> {
        view.read_with(cx, |v, _| {
            v.reading
                .shown_rows()
                .map(|r| r.subscription_id.clone())
                .collect()
        })
    };
    for (src, all_own, any_own) in [
        (Source::Own, true, true),
        (Source::Smart(Smart::Unread), false, false),
        (Source::Smart(Smart::Thumbed), false, false),
        (Source::Smart(Smart::All), false, true),
    ] {
        view.update(cx, |v, cx| v.select_source(src.clone(), cx));
        settle(cx);
        let subs = shown(&view, cx);
        assert_eq!(subs.iter().all(|s| s == OWN), all_own, "{src:?}: {subs:?}");
        assert_eq!(subs.iter().any(|s| s == OWN), any_own, "{src:?}: {subs:?}");
    }

    // Opening one marks nothing read; its pill steps current + pinned only.
    view.update(cx, |v, cx| v.select_source(Source::Own, cx));
    open_row(&view, OWN_FRICTION, cx);
    view.read_with(cx, |v, _| {
        let o = v.reading.opened.as_ref().unwrap();
        let shown: Vec<(u32, bool)> = o
            .shown
            .ready()
            .unwrap()
            .iter()
            .map(|v| (v.version, v.current))
            .collect();
        assert_eq!(shown, [(2, false), (3, true)], "v1 isn't pinned");
        assert!(matches!(o.changelog, Load::Ready(_)));
    });
    // r / u and the thumbs send nothing for it.
    cx.simulate_keystrokes("u");
    settle(cx);
    view.update(cx, |v, cx| {
        let key = v.reading.opened.as_ref().unwrap().key.clone();
        v.toggle_thumb_for(key, None, true, cx)
    });
    settle(cx);
    no_own_sends(&fake);
    assert!(
        !view.read_with(cx, |v, _| v
            .reading
            .rows
            .iter()
            .any(|r| r.subscription_id == OWN && r.is_unread())),
        "still never unread"
    );
}

#[gpui_kit::test]
fn reading_actions_on_your_post_carry_your_origin(cx: &mut TestAppContext) {
    let (view, fake, cx) = setup(cx);
    cx.simulate_keystrokes(&crate::keymap::keys("cmd-r"));
    settle(cx);
    open_row(&view, OWN_FRICTION, cx);
    // The chips are the reading pane's own, Fork greyed on the current one.
    let chips = view.read_with(cx, |v, _| v.reading_action_chips());
    let ids: Vec<&str> = chips.iter().map(|c| c.id).collect();
    for want in ["Quote", "Reply", "Fork", "Link post", "Open on web"] {
        assert!(ids.contains(&want), "{ids:?}");
    }
    assert!(!chips.iter().find(|c| c.id == "Fork").unwrap().enabled);

    // Reply · new stub: stub_of names your origin (a citation is absolute).
    let before = fake.items();
    view.update_in(cx, |v, window, cx| {
        v.reading_action_for_test("Reply", window, cx)
    });
    settle(cx);
    let made = new_items(&fake, &before);
    assert_eq!(made.len(), 1);
    let s = made[0].stub_of.as_ref().expect("a stub");
    assert_eq!(
        (s.origin.as_str(), s.id.as_str(), s.version),
        (ME, OWN_FRICTION, 3)
    );
    assert_eq!(made[0].kind, Kind::Thread);
    assert!(
        made[0]
            .content_md
            .starts_with(&format!("![[{OWN_FRICTION}]]"))
    );

    // Fork this pin, from your 📌 v2: its text, forked_from naming you.
    cx.simulate_keystrokes(&crate::keymap::keys("cmd-r"));
    settle(cx);
    open_row(&view, OWN_FRICTION, cx);
    cx.simulate_keystrokes("[");
    settle(cx);
    view.read_with(cx, |v, _| {
        let o = v.reading.opened.as_ref().unwrap();
        assert_eq!(o.pinned_on_screen().map(|p| p.version), Some(2));
        assert!(matches!(o.pins.get(&2), Some(Load::Ready(p)) if p.content_md == FRICTION_V2));
    });
    let before = fake.items();
    view.update_in(cx, |v, window, cx| {
        v.reading_action_for_test("Fork this pin", window, cx)
    });
    settle(cx);
    let made = new_items(&fake, &before);
    assert_eq!(made.len(), 1);
    let f = made[0].forked_from.as_ref().expect("a fork");
    assert_eq!(
        (f.origin.as_str(), f.id.as_str(), f.version),
        (ME, OWN_FRICTION, 2)
    );
    assert_eq!(made[0].content_md, FRICTION_V2);
    assert_eq!(made[0].kind, Kind::Fragment, "a fork keeps the pin's kind");
    no_own_sends(&fake);
}

#[gpui_kit::test]
fn the_posts_view_replies_quotes_forks_and_links_your_post(cx: &mut TestAppContext) {
    let (view, fake, cx) = setup(cx);
    open_post(&view, FRICTION, cx);
    assert!(view.read_with(cx, |v, _| v.current_own_row().is_some()));

    // ⌥⌘R with the caret in the editor: a stub of your post.
    view.update_in(cx, |v, window, cx| {
        use gpui_kit::Focusable as _;
        let fh = v.editor.read(cx).focus_handle(cx);
        window.focus(&fh, cx);
    });
    let before = fake.items();
    cx.simulate_keystrokes(&crate::keymap::keys("alt-cmd-r"));
    settle(cx);
    let made = new_items(&fake, &before);
    assert_eq!(made.len(), 1, "the key works while typing");
    let s = made[0].stub_of.as_ref().unwrap();
    assert_eq!(
        (s.origin.as_str(), s.id.as_str(), s.version),
        (ME, OWN_FRICTION, 3)
    );
    assert_eq!(view.read_with(cx, |v, _| v.reading.view), View::Posts);

    // Fork: the newest pin (📌 v2).
    open_post(&view, FRICTION, cx);
    let before = fake.items();
    cx.dispatch_action(ForkPost);
    settle(cx);
    let made = new_items(&fake, &before);
    let f = made[0].forked_from.as_ref().unwrap();
    assert_eq!((f.origin.as_str(), f.version), (ME, 2));
    assert_eq!(made[0].content_md, FRICTION_V2);

    // Link post · new fragment: [[id]], no stub_of.
    open_post(&view, FRICTION, cx);
    let before = fake.items();
    cx.dispatch_action(LinkToPost);
    settle(cx);
    let made = new_items(&fake, &before);
    assert_eq!(made[0].content_md, format!("[[{OWN_FRICTION}]]\n\n"));
    assert_eq!(made[0].kind, Kind::Fragment);
    assert!(made[0].stub_of.is_none() && made[0].forked_from.is_none());

    // Quote into a thread: a new thread quoting it (the open post isn't a
    // draft thread); the transclusion's origin is left out, as own.
    open_post(&view, FRICTION, cx);
    let before = fake.items();
    cx.dispatch_action(QuotePost);
    settle(cx);
    let made = new_items(&fake, &before);
    assert_eq!(made[0].kind, Kind::Thread);
    assert_eq!(made[0].content_md, format!("![[{OWN_FRICTION}]]\n\n"));

    // A post with no pin: Fork says so, and makes nothing.
    let gardens = LocalId("01J9K7T".into());
    open_post(&view, &gardens.0, cx);
    let before = fake.items().len();
    cx.dispatch_action(ForkPost);
    settle(cx);
    assert_eq!(fake.items().len(), before);
    assert!(view.read_with(cx, |v, _| {
        v.toast.as_ref().is_some_and(|t| t.text.contains("pinned"))
    }));

    // A draft: nothing to reply to yet.
    open_post(&view, DRAFT, cx);
    assert!(view.read_with(cx, |v, _| v.current_own_row().is_none()));
    let before = fake.items().len();
    cx.dispatch_action(ReplyToPost);
    settle(cx);
    assert_eq!(fake.items().len(), before);
    no_own_sends(&fake);
}

#[gpui_kit::test]
fn the_keys_act_on_the_open_reading_post_too(cx: &mut TestAppContext) {
    let (view, fake, cx) = setup(cx);
    cx.simulate_keystrokes(&crate::keymap::keys("cmd-r"));
    settle(cx);
    open_row(&view, RUE_TRUST, cx);
    let before = fake.items();
    cx.dispatch_action(ReplyToPost);
    settle(cx);
    let made = new_items(&fake, &before);
    assert_eq!(made[0].stub_of.as_ref().unwrap().id, RUE_TRUST);
    // Fork on someone's current version says where the pins are.
    cx.simulate_keystrokes(&crate::keymap::keys("cmd-r"));
    settle(cx);
    open_row(&view, RUE_TRUST, cx);
    let before = fake.items().len();
    cx.dispatch_action(ForkPost);
    settle(cx);
    assert_eq!(fake.items().len(), before);
    assert!(view.read_with(cx, |v, _| {
        v.toast.as_ref().is_some_and(|t| t.text.contains("pinned"))
    }));
}

/// A stub of your own post can choose a passage of it.
#[gpui_kit::test]
fn a_self_stub_quotes_a_passage_of_your_post(cx: &mut TestAppContext) {
    let (view, _, cx) = setup(cx);
    open_post(&view, "01J9K7T", cx);
    cx.dispatch_action(ReplyToPost);
    settle(cx);
    view.update_in(cx, |v, window, cx| v.toggle_stub_chooser(window, cx));
    settle(cx);
    let text = view.read_with(cx, |v, cx| {
        v.reading
            .stub
            .as_ref()
            .and_then(|c| c.post.as_ref())
            .map(|p| p.read(cx).value().to_string())
    });
    let text = text.expect("your post's text in the chooser");
    assert!(text.contains("Soil matters more than seeds."), "{text}");
}
