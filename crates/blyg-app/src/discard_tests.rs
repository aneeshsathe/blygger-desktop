//! Delete Draft… and Withdraw… in a headless window: real keystrokes and
//! clicks through GPUI's dispatch, against a zero-latency FakeBackend.

use std::sync::Arc;

use blyg_core::config::MemoryTokenStore;
use blyg_core::{Backend, ConfigStore, LocalId, Status};
use gpui_kit::{Entity, Modifiers, TestAppContext, VisualTestContext};

use super::Withdraw;
use crate::app::{MainView, Mode, Sheet};
use crate::fake::{FakeBackend, Timing};
use crate::prefs::Prefs;

const CONNECTED: &str = "# test config\nblyg-url = https://blyg.example.com\n";
const DRAFT: &str = "01J9QK3"; // the first row: a draft fragment
const PUBLIC: &str = "01J9PX1"; // the second: public v1

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

fn id(s: &str) -> LocalId {
    LocalId(s.into())
}

/// The backend call runs on the background executor; let it land.
fn settle(cx: &mut VisualTestContext, done: impl Fn() -> bool) {
    for _ in 0..50 {
        cx.run_until_parked();
        if done() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    cx.run_until_parked();
}

fn toast(view: &Entity<MainView>, cx: &mut VisualTestContext) -> String {
    view.read_with(cx, |v, _| {
        v.toast
            .as_ref()
            .map(|t| t.text.to_string())
            .unwrap_or_default()
    })
}

fn current(view: &Entity<MainView>, cx: &mut VisualTestContext) -> Option<LocalId> {
    view.read_with(cx, |v, _| v.current.as_ref().map(|c| c.local_id.clone()))
}

#[gpui_kit::test]
fn delete_asks_then_deletes_and_the_list_moves_on(cx: &mut TestAppContext) {
    let (view, fake, cx) = setup(cx);
    assert_eq!(current(&view, cx), Some(id(DRAFT)));
    cx.simulate_keystrokes(&crate::keymap::keys("cmd-shift-backspace"));
    cx.run_until_parked();
    view.read_with(cx, |v, _| match &v.sheet {
        Some(Sheet::DeleteDraft {
            id: sid,
            noun,
            title,
            ..
        }) => {
            assert_eq!(sid, &id(DRAFT));
            assert_eq!(*noun, "draft");
            assert!(title.starts_with("A lighthouse"), "{title}");
        }
        _ => panic!("no delete sheet"),
    });
    assert!(fake.item(&id(DRAFT)).is_some(), "nothing deleted before ⏎");
    cx.simulate_keystrokes("enter");
    settle(cx, || fake.item(&id(DRAFT)).is_none());
    assert!(fake.item(&id(DRAFT)).is_none(), "deleted");
    view.read_with(cx, |v, _| {
        assert!(v.sheet.is_none());
        assert_eq!(v.list.results().len(), 5);
        assert_eq!(v.list.selected(), Some(&id(PUBLIC)), "the next row");
        assert_eq!(v.mode, Mode::Search);
    });
    assert_eq!(current(&view, cx), Some(id(PUBLIC)));
    assert_eq!(toast(&view, cx), "Draft deleted");
    let text = view.read_with(cx, |v, cx| v.editor.read(cx).value().to_string());
    assert!(
        text.starts_with("Band on"),
        "the editor previews it: {text}"
    );
}

#[gpui_kit::test]
fn esc_cancels_and_keeps_the_draft(cx: &mut TestAppContext) {
    let (view, fake, cx) = setup(cx);
    cx.simulate_keystrokes(&crate::keymap::keys("cmd-shift-backspace"));
    cx.run_until_parked();
    assert!(view.read_with(cx, |v, _| matches!(
        v.sheet,
        Some(Sheet::DeleteDraft { .. })
    )));
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(view.read_with(cx, |v, _| v.sheet.is_none()));
    assert!(fake.item(&id(DRAFT)).is_some());
    assert_eq!(current(&view, cx), Some(id(DRAFT)));
}

#[gpui_kit::test]
fn delete_from_the_editor_with_text_selected(cx: &mut TestAppContext) {
    let (view, fake, cx) = setup(cx);
    cx.simulate_keystrokes("enter"); // open the draft: focus in the editor
    cx.run_until_parked();
    assert_eq!(view.read_with(cx, |v, _| v.mode), Mode::Edit);
    cx.simulate_keystrokes(&crate::keymap::keys("cmd-a"));
    cx.run_until_parked();
    let before = fake.item(&id(DRAFT)).unwrap().content_md;
    // ⇧⌘⌫ isn't a text-editing key: it asks, and the text is untouched.
    cx.simulate_keystrokes(&crate::keymap::keys("cmd-shift-backspace"));
    cx.run_until_parked();
    assert!(view.read_with(cx, |v, _| matches!(
        v.sheet,
        Some(Sheet::DeleteDraft { .. })
    )));
    assert_eq!(fake.item(&id(DRAFT)).unwrap().content_md, before);
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    // ⌘⌫ keeps its text meaning (delete to line start) and asks nothing.
    cx.simulate_keystrokes(&crate::keymap::keys("cmd-backspace"));
    cx.run_until_parked();
    assert!(view.read_with(cx, |v, _| v.sheet.is_none()));
    assert!(fake.item(&id(DRAFT)).is_some());
}

#[gpui_kit::test]
fn a_scratch_note_is_deleted_too(cx: &mut TestAppContext) {
    let (view, fake, cx) = setup(cx);
    let sid = fake
        .create_scratch(blyg_core::Kind::Fragment, "a scratch thought")
        .unwrap();
    view.update_in(cx, |v, window, cx| {
        v.refresh_from_backend(window, cx);
        v.list.select(&sid);
        v.load_selected(window, cx);
    });
    cx.run_until_parked();
    cx.simulate_keystrokes(&crate::keymap::keys("cmd-shift-backspace"));
    cx.run_until_parked();
    assert!(view.read_with(cx, |v, _| matches!(
        v.sheet,
        Some(Sheet::DeleteDraft {
            noun: "scratch note",
            ..
        })
    )));
    cx.simulate_keystrokes("enter");
    settle(cx, || fake.item(&sid).is_none());
    assert!(fake.item(&sid).is_none());
    assert_eq!(toast(&view, cx), "Scratch note deleted");
}

#[gpui_kit::test]
fn published_posts_are_never_deleted(cx: &mut TestAppContext) {
    let (view, fake, cx) = setup(cx);
    cx.simulate_keystrokes("down"); // the public post
    cx.run_until_parked();
    assert_eq!(current(&view, cx), Some(id(PUBLIC)));
    cx.simulate_keystrokes(&crate::keymap::keys("cmd-shift-backspace"));
    cx.run_until_parked();
    assert!(view.read_with(cx, |v, _| v.sheet.is_none()), "no sheet");
    assert_eq!(toast(&view, cx), "Published posts can't be deleted");
    assert_eq!(fake.item(&id(PUBLIC)).unwrap().status, Status::Public);
}

#[gpui_kit::test]
fn withdraw_asks_with_a_note_then_withdraws(cx: &mut TestAppContext) {
    let (view, fake, cx) = setup(cx);
    cx.simulate_keystrokes("down");
    cx.run_until_parked();
    cx.dispatch_action(Withdraw);
    cx.run_until_parked();
    view.read_with(cx, |v, _| match &v.sheet {
        Some(Sheet::Withdraw { id: sid, title, .. }) => {
            assert_eq!(sid, &id(PUBLIC));
            assert!(title.starts_with("Band on"), "{title}");
        }
        _ => panic!("no withdraw sheet"),
    });
    // The note field has focus.
    cx.simulate_input("no longer accurate");
    cx.simulate_keystrokes("enter");
    settle(cx, || {
        fake.item(&id(PUBLIC)).unwrap().status == Status::Withdrawn
    });
    let item = fake.item(&id(PUBLIC)).unwrap();
    assert_eq!(item.status, Status::Withdrawn);
    assert_eq!(item.version, 2);
    let last = fake.versions(&id(PUBLIC)).unwrap().pop().unwrap();
    assert!(last.endcap);
    assert_eq!(last.note.as_deref(), Some("no longer accurate"));
    assert_eq!(toast(&view, cx), "Withdrawn as v2 · “no longer accurate”");
    view.read_with(cx, |v, _| {
        assert!(v.sheet.is_none());
        assert_eq!(v.list.results().len(), 6, "it stays listed");
        assert_eq!(
            v.current.as_ref().map(|c| c.status),
            Some(Status::Withdrawn)
        );
    });
    // Once withdrawn, neither action asks again.
    cx.dispatch_action(Withdraw);
    cx.run_until_parked();
    assert!(view.read_with(cx, |v, _| v.sheet.is_none()));
    assert_eq!(toast(&view, cx), "Already withdrawn");
}

#[gpui_kit::test]
fn withdraw_esc_cancels_and_drafts_are_not_withdrawn(cx: &mut TestAppContext) {
    let (view, fake, cx) = setup(cx);
    // On a draft, Withdraw points at delete instead.
    cx.dispatch_action(Withdraw);
    cx.run_until_parked();
    assert!(view.read_with(cx, |v, _| v.sheet.is_none()));
    assert!(toast(&view, cx).contains("nothing to withdraw"));
    cx.simulate_keystrokes("down");
    cx.run_until_parked();
    cx.dispatch_action(Withdraw);
    cx.run_until_parked();
    assert!(view.read_with(cx, |v, _| matches!(v.sheet, Some(Sheet::Withdraw { .. }))));
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(view.read_with(cx, |v, _| v.sheet.is_none()));
    assert_eq!(fake.item(&id(PUBLIC)).unwrap().status, Status::Public);
}

#[gpui_kit::test]
fn the_toolbar_slot_deletes_or_withdraws(cx: &mut TestAppContext) {
    let (view, fake, cx) = setup(cx);
    let actions = |view: &Entity<MainView>, cx: &mut VisualTestContext| {
        view.read_with(cx, |v, cx| {
            v.toolbar_buttons(cx)
                .into_iter()
                .map(|b| b.action)
                .collect::<Vec<_>>()
        })
    };
    let a = actions(&view, cx);
    assert!(
        a.contains(&"DeleteDraft") && !a.contains(&"Withdraw"),
        "{a:?}"
    );
    // Click Delete: the same sheet as ⇧⌘⌫; its Cancel button keeps the draft.
    let b = cx.debug_bounds("tb-DeleteDraft").expect("Delete button");
    cx.simulate_click(b.center(), Modifiers::none());
    cx.run_until_parked();
    assert!(view.read_with(cx, |v, _| matches!(
        v.sheet,
        Some(Sheet::DeleteDraft { .. })
    )));
    // Let the sheet finish dropping in (its animation runs on the wall clock).
    std::thread::sleep(std::time::Duration::from_millis(300));
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    let cancel = cx.debug_bounds("sheet-cancel").expect("Cancel");
    cx.simulate_click(cancel.center(), Modifiers::none());
    cx.run_until_parked();
    assert!(view.read_with(cx, |v, _| v.sheet.is_none()));
    assert!(fake.item(&id(DRAFT)).is_some());
    // A published post: the slot says Withdraw, never Delete.
    cx.simulate_keystrokes("down");
    cx.run_until_parked();
    let a = actions(&view, cx);
    assert!(
        a.contains(&"Withdraw") && !a.contains(&"DeleteDraft"),
        "{a:?}"
    );
    let b = cx.debug_bounds("tb-Withdraw").expect("Withdraw button");
    cx.simulate_click(b.center(), Modifiers::none());
    cx.run_until_parked();
    assert!(view.read_with(cx, |v, _| matches!(v.sheet, Some(Sheet::Withdraw { .. }))));
}

// ------------------------------------------------------------ discard changes

/// The public post's text as published (v1).
const BAND: &str =
    "Band on Harbour Road: the trombone came in half a beat late and the tune was better for it.";

fn editor_text(view: &Entity<MainView>, cx: &mut VisualTestContext) -> String {
    view.read_with(cx, |v, cx| v.editor.read(cx).value().to_string())
}

/// Open the public post in the editor and type an edit.
fn edit_public(cx: &mut VisualTestContext) {
    cx.simulate_keystrokes("down");
    cx.run_until_parked();
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    cx.simulate_input(" Then the rain came.");
    cx.run_until_parked();
}

#[gpui_kit::test]
fn discard_changes_asks_then_goes_back_to_the_published_version(cx: &mut TestAppContext) {
    let (view, fake, cx) = setup(cx);
    edit_public(cx);
    let it = fake.item(&id(PUBLIC)).unwrap();
    assert!(it.dirty, "an unpublished edit");
    assert!(it.content_md.ends_with("Then the rain came."));
    cx.simulate_keystrokes(&crate::keymap::keys("alt-cmd-z"));
    cx.run_until_parked();
    view.read_with(cx, |v, _| match &v.sheet {
        Some(Sheet::DiscardChanges {
            id: sid,
            version,
            title,
            ..
        }) => {
            assert_eq!(sid, &id(PUBLIC));
            assert_eq!(*version, 1);
            assert!(title.starts_with("Band on"), "{title}");
        }
        _ => panic!("no discard sheet"),
    });
    assert!(fake.item(&id(PUBLIC)).unwrap().dirty, "nothing before ⏎");
    cx.simulate_keystrokes("enter");
    settle(cx, || !fake.item(&id(PUBLIC)).unwrap().dirty);
    let it = fake.item(&id(PUBLIC)).unwrap();
    assert_eq!(it.content_md, BAND);
    assert!(!it.dirty && !it.pending_sync);
    assert_eq!(it.version, 1, "nothing published");
    assert_eq!(editor_text(&view, cx), BAND, "the editor holds v1");
    assert_eq!(toast(&view, cx), "Back to v1 as published");
    view.read_with(cx, |v, _| {
        assert!(v.sheet.is_none());
        assert!(v.restoring.is_none());
        assert_eq!(v.mode, Mode::Edit, "still writing");
    });
    // Typing on is an ordinary edit again.
    cx.simulate_input(" Encore.");
    cx.run_until_parked();
    let it = fake.item(&id(PUBLIC)).unwrap();
    assert_eq!(it.content_md, format!("{BAND} Encore."));
    assert!(it.dirty);
}

#[gpui_kit::test]
fn discard_changes_esc_keeps_the_edits(cx: &mut TestAppContext) {
    let (view, fake, cx) = setup(cx);
    edit_public(cx);
    cx.dispatch_action(super::DiscardChanges);
    cx.run_until_parked();
    assert!(view.read_with(cx, |v, _| matches!(
        v.sheet,
        Some(Sheet::DiscardChanges { .. })
    )));
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(view.read_with(cx, |v, _| v.sheet.is_none()));
    let it = fake.item(&id(PUBLIC)).unwrap();
    assert!(it.dirty && it.content_md.ends_with("Then the rain came."));
    assert!(editor_text(&view, cx).ends_with("Then the rain came."));
}

/// Nothing to discard: a draft (Delete's), a clean published post, and an
/// edited one while offline. The toolbar button says the same.
#[gpui_kit::test]
fn discard_changes_says_why_not(cx: &mut TestAppContext) {
    let (view, fake, cx) = setup(cx);
    let button = |view: &Entity<MainView>, cx: &mut VisualTestContext| {
        view.read_with(cx, |v, cx| {
            v.toolbar_buttons(cx)
                .into_iter()
                .find(|b| b.action == "DiscardChanges")
                .expect("the Discard button")
        })
    };
    // The draft.
    cx.dispatch_action(super::DiscardChanges);
    cx.run_until_parked();
    assert!(view.read_with(cx, |v, _| v.sheet.is_none()));
    let not_published = crate::keymap::hint(crate::vm::DISCARD_NOT_PUBLISHED);
    assert_eq!(toast(&view, cx), not_published);
    assert_eq!(button(&view, cx).reason.as_deref(), Some(not_published));
    // The published post, clean.
    cx.simulate_keystrokes("down");
    cx.run_until_parked();
    cx.dispatch_action(super::DiscardChanges);
    cx.run_until_parked();
    assert!(view.read_with(cx, |v, _| v.sheet.is_none()));
    assert_eq!(toast(&view, cx), crate::vm::DISCARD_CLEAN);
    assert_eq!(
        button(&view, cx).reason.as_deref(),
        Some(crate::vm::DISCARD_CLEAN)
    );
    // Edited: enabled, with its key in the tooltip.
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    cx.simulate_input(" More.");
    cx.run_until_parked();
    let b = button(&view, cx);
    assert_eq!(b.reason, None);
    let key = crate::keymap::glyphs(&crate::keymap::keys("alt-cmd-z"));
    assert_eq!(b.tooltip, format!("Discard unpublished changes…  {key}"));
    // Offline: the published version is on the blyg.
    fake.set_offline(true);
    cx.run_until_parked();
    assert_eq!(
        button(&view, cx).reason.as_deref(),
        Some(crate::vm::DISCARD_OFFLINE)
    );
    cx.dispatch_action(super::DiscardChanges);
    cx.run_until_parked();
    assert!(view.read_with(cx, |v, _| v.sheet.is_none()));
    assert_eq!(toast(&view, cx), "Offline. Discard needs the blyg.");
    assert!(fake.item(&id(PUBLIC)).unwrap().dirty, "the edit stays");
}

/// Typing while the restore is on its way: those keystrokes aren't saved,
/// so the old text can't land on top of the restored copy, and the editor
/// ends up holding exactly what the blyg restored.
#[gpui_kit::test]
fn typing_during_a_discard_is_not_saved_back(cx: &mut TestAppContext) {
    let (view, fake, cx) = setup(cx);
    edit_public(cx);
    let edited = fake.item(&id(PUBLIC)).unwrap().content_md;
    view.update_in(cx, |v, window, cx| {
        v.restore_working_copy(id(PUBLIC), 1, true, window, cx);
        // The restore hasn't run yet (nothing has parked): type on.
        v.editor
            .update(cx, |s, cx| s.insert(" Typed meanwhile.", window, cx));
        v.after_edit(cx);
    });
    assert_eq!(
        fake.item(&id(PUBLIC)).unwrap().content_md,
        edited,
        "the keystrokes during the discard weren't saved"
    );
    settle(cx, || !fake.item(&id(PUBLIC)).unwrap().dirty);
    let it = fake.item(&id(PUBLIC)).unwrap();
    assert_eq!(it.content_md, BAND);
    assert!(!it.dirty);
    assert_eq!(editor_text(&view, cx), BAND);
    // And nothing saves the old text afterwards.
    cx.run_until_parked();
    assert_eq!(fake.item(&id(PUBLIC)).unwrap().content_md, BAND);
}

/// A discard that fails (offline by the time it runs) replaces nothing, so
/// what was typed meanwhile is saved after all.
#[gpui_kit::test]
fn a_failed_discard_keeps_what_was_typed(cx: &mut TestAppContext) {
    let (view, fake, cx) = setup(cx);
    edit_public(cx);
    view.update_in(cx, |v, window, cx| {
        v.restore_working_copy(id(PUBLIC), 1, true, window, cx);
        v.editor
            .update(cx, |s, cx| s.insert(" Typed meanwhile.", window, cx));
        v.after_edit(cx);
    });
    fake.set_offline(true);
    settle(cx, || {
        fake.item(&id(PUBLIC))
            .unwrap()
            .content_md
            .ends_with("Typed meanwhile.")
    });
    let it = fake.item(&id(PUBLIC)).unwrap();
    assert!(
        it.content_md
            .ends_with("Then the rain came. Typed meanwhile."),
        "{}",
        it.content_md
    );
    assert!(it.dirty);
    assert_eq!(toast(&view, cx), "Offline. Discard needs the blyg.");
    assert!(view.read_with(cx, |v, _| v.restoring.is_none()));
}

/// ⌘Y on an edited post: the current version offers "Discard changes…",
/// the same sheet and path as ⌥⌘Z.
#[gpui_kit::test]
fn the_version_browser_discards_changes_on_the_current_version(cx: &mut TestAppContext) {
    let (view, fake, cx) = setup(cx);
    edit_public(cx);
    cx.simulate_keystrokes(&crate::keymap::keys("cmd-y"));
    settle(cx, || false);
    view.update_in(cx, |v, window, cx| {
        let own = v.reading.own.as_mut().expect("the history is open");
        own.sel = Some(1);
        v.ask_restore(window, cx);
    });
    cx.run_until_parked();
    assert!(view.read_with(cx, |v, _| matches!(
        v.sheet,
        Some(Sheet::DiscardChanges { version: 1, .. })
    )));
    cx.simulate_keystrokes("enter");
    settle(cx, || !fake.item(&id(PUBLIC)).unwrap().dirty);
    assert_eq!(fake.item(&id(PUBLIC)).unwrap().content_md, BAND);
    assert_eq!(editor_text(&view, cx), BAND);
    assert!(
        view.read_with(cx, |v, _| v.reading.own.is_none()),
        "the history closes"
    );
}
