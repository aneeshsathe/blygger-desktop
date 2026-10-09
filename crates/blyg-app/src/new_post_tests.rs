//! ⌘N, New Post: a fresh editor that becomes a scratch note (or a draft).

use std::sync::Arc;

use blyg_core::config::{MemoryTokenStore, NewNote};
use blyg_core::{Backend, ConfigStore, LocalId, Status};
use gpui_kit::{Entity, Focusable as _, TestAppContext, VisualTestContext};

use crate::app::{MainView, Mode};
use crate::fake::{FakeBackend, Timing};
use crate::prefs::Prefs;

const CONNECTED: &str = "# test config\nblyg-url = https://blyg.example.com\n";

fn setup<'a>(
    cx: &'a mut TestAppContext,
    config: &str,
) -> (
    Entity<MainView>,
    Arc<FakeBackend>,
    &'a mut VisualTestContext,
) {
    let config = config.to_string();
    let prefs = Prefs::from_config(ConfigStore::in_memory(&config).config());
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::app::bind_keys(cx);
        crate::settings::init(
            ConfigStore::in_memory(&config),
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

fn key(cx: &mut VisualTestContext, k: &str) {
    cx.simulate_keystrokes(&crate::keymap::keys(k));
    cx.run_until_parked();
}

fn typing(cx: &mut VisualTestContext, text: &str) {
    cx.simulate_input(text);
    cx.run_until_parked();
}

fn editor_text(view: &Entity<MainView>, cx: &mut VisualTestContext) -> String {
    view.read_with(cx, |v, cx| v.editor.read(cx).value().to_string())
}

fn omni_text(view: &Entity<MainView>, cx: &mut VisualTestContext) -> String {
    view.read_with(cx, |v, cx| v.omni.read(cx).value().to_string())
}

fn editor_focused(view: &Entity<MainView>, cx: &mut VisualTestContext) -> bool {
    let fh = view.read_with(cx, |v, cx| v.editor.read(cx).focus_handle(cx));
    cx.update(|window, _| fh.is_focused(window))
}

fn label(view: &Entity<MainView>, cx: &mut VisualTestContext) -> Option<&'static str> {
    view.read_with(cx, |v, _| v.new_post_label())
}

fn current_id(view: &Entity<MainView>, cx: &mut VisualTestContext) -> Option<LocalId> {
    view.read_with(cx, |v, _| v.current.as_ref().map(|c| c.local_id.clone()))
}

/// The list as shown: its query, its rows and the selected row.
fn list_state(
    view: &Entity<MainView>,
    cx: &mut VisualTestContext,
) -> (String, Vec<LocalId>, Option<LocalId>) {
    view.read_with(cx, |v, _| {
        (
            v.list.query().to_string(),
            v.list
                .results()
                .iter()
                .map(|i| i.local_id.clone())
                .collect(),
            v.list.selected().cloned(),
        )
    })
}

#[gpui_kit::test]
fn cmd_n_opens_an_empty_editor_and_leaves_the_search_alone(cx: &mut TestAppContext) {
    let (view, fake, cx) = setup(cx, CONNECTED);
    // A search with several matches, the second one selected.
    typing(cx, "the");
    key(cx, "down");
    let before = list_state(&view, cx);
    assert!(before.1.len() > 1, "{before:?}");
    let items = fake.items().len();

    key(cx, "cmd-n");
    view.read_with(cx, |v, _| {
        assert_eq!(v.mode, Mode::Edit);
        assert!(v.current.is_none());
    });
    assert_eq!(editor_text(&view, cx), "");
    assert!(editor_focused(&view, cx), "the caret is in the editor");
    assert_eq!(omni_text(&view, cx), "the", "the omnibar keeps its text");
    assert_eq!(list_state(&view, cx), before, "the list keeps its filter");
    assert_eq!(fake.items().len(), items, "nothing is created yet");
    assert_eq!(
        label(&view, cx),
        Some(crate::keymap::hint(
            "New note · saved locally · ⌘D draft · ⌘⏎ publish"
        ))
    );
}

#[gpui_kit::test]
fn the_first_keystroke_creates_a_scratch_note(cx: &mut TestAppContext) {
    let (view, fake, cx) = setup(cx, CONNECTED);
    let items = fake.items().len();
    key(cx, "cmd-n");
    typing(cx, "M");
    assert_eq!(fake.items().len(), items + 1);
    let id = current_id(&view, cx).expect("the note is open");
    let it = fake.item(&id).unwrap();
    assert_eq!((it.status, it.content_md.as_str()), (Status::Scratch, "M"));

    // Then it autosaves like any edit, locally, and stays one item.
    typing(cx, "oss on the north wall");
    let it = fake.item(&id).unwrap();
    assert_eq!(it.content_md, "Moss on the north wall");
    assert_eq!(it.status, Status::Scratch);
    assert!(!it.pending_sync, "nothing is queued for the server");
    assert_eq!(fake.items().len(), items + 1);
    assert!(label(&view, cx).is_some(), "still in the new-post mode");
    assert_eq!(omni_text(&view, cx), "");
}

#[gpui_kit::test]
fn cmd_d_makes_the_new_note_a_draft_with_the_same_id(cx: &mut TestAppContext) {
    let (view, fake, cx) = setup(cx, CONNECTED);
    key(cx, "cmd-n");
    typing(cx, "Kettle whistles in B flat");
    let id = current_id(&view, cx).unwrap();
    let items = fake.items().len();

    key(cx, "cmd-d");
    let it = fake.item(&id).expect("same local id");
    assert_eq!(it.status, Status::Draft);
    assert_eq!(fake.items().len(), items, "no duplicate");
    assert_eq!(current_id(&view, cx), Some(id));
    // Now it's an ordinary draft: the status bar says "draft".
    assert_eq!(label(&view, cx), None);
}

#[gpui_kit::test]
fn leaving_an_empty_new_post_creates_nothing(cx: &mut TestAppContext) {
    let (view, fake, cx) = setup(cx, CONNECTED);
    let items = fake.items();

    // Nothing typed, then back to searching.
    key(cx, "cmd-n");
    key(cx, "cmd-l");
    typing(cx, "lighthouse");
    assert_eq!(fake.items().len(), items.len());
    assert_eq!(label(&view, cx), None);

    // Typed, then erased, then ⌘N again: the blank note is deleted.
    key(cx, "cmd-n");
    typing(cx, "x");
    let id = current_id(&view, cx).unwrap();
    key(cx, "backspace");
    assert_eq!(fake.item(&id).unwrap().content_md, "");
    key(cx, "cmd-n");
    assert!(fake.item(&id).is_none(), "the empty scratch note is gone");
    assert_eq!(fake.items().len(), items.len());

    // Typed, erased, then another post opened from the omnibar.
    typing(cx, "y");
    let id = current_id(&view, cx).unwrap();
    key(cx, "backspace");
    key(cx, "cmd-l");
    typing(cx, "lighthouse");
    key(cx, "enter");
    assert!(fake.item(&id).is_none());
    assert_eq!(fake.items().len(), items.len());
    assert!(
        view.read_with(cx, |v, _| v.list.results().iter().all(|i| i.local_id != id)),
        "no blank row in the list"
    );

    // A new post with text is kept when left.
    key(cx, "cmd-n");
    typing(cx, "Keep me");
    let id = current_id(&view, cx).unwrap();
    key(cx, "cmd-n");
    assert_eq!(fake.item(&id).unwrap().content_md, "Keep me");
    assert_eq!(fake.items().len(), items.len() + 1);
}

#[gpui_kit::test]
fn new_note_draft_makes_cmd_n_start_a_draft(cx: &mut TestAppContext) {
    let (view, fake, cx) = setup(cx, &format!("{CONNECTED}new-note = draft\n"));
    assert_eq!(view.read_with(cx, |v, _| v.prefs.new_post), NewNote::Draft);
    key(cx, "cmd-n");
    assert_eq!(
        label(&view, cx),
        Some(crate::keymap::hint(
            "New draft · syncs to your blyg · ⌘⏎ publish"
        ))
    );
    typing(cx, "Tide tables");
    let id = current_id(&view, cx).unwrap();
    assert_eq!(fake.item(&id).unwrap().status, Status::Draft);
    assert!(label(&view, cx).is_some());

    // An emptied draft is deleted on leaving too.
    key(cx, "cmd-a");
    key(cx, "backspace");
    key(cx, "cmd-n");
    assert!(fake.item(&id).is_none());
}

#[gpui_kit::test]
fn the_omnibar_still_creates_from_a_search(cx: &mut TestAppContext) {
    let (view, fake, cx) = setup(cx, CONNECTED);
    key(cx, "cmd-n");
    key(cx, "cmd-l");
    typing(cx, "quokkas at dawn");
    view.read_with(cx, |v, _| assert!(v.list.wants_create()));
    key(cx, "enter");
    let first = fake.items().remove(0);
    assert_eq!(first.content_md, "quokkas at dawn");
    assert_eq!(
        first.status,
        Status::Draft,
        "the omnibar's default is a draft"
    );
    assert_eq!(current_id(&view, cx), Some(first.local_id));
    assert_eq!(label(&view, cx), None);
    assert_eq!(omni_text(&view, cx), "");
}
