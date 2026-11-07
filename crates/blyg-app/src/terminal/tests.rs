//! --- terminal --- The pane in a headless window: real keystrokes through
//! GPUI's dispatch, the engine fed directly (no shell: GPUI's test scheduler
//! refuses wake-ups from the pty's threads, and `blyg-term`'s own tests run
//! a real pty).

use std::sync::Arc;

use blyg_core::config::MemoryTokenStore;
use blyg_core::{Backend, ConfigStore};
use blyg_term::GridPoint;
use gpui_kit::{ClipboardItem, Entity, TestAppContext, VisualTestContext};

use super::{TermEvent, TerminalView};
use crate::app::MainView;
use crate::fake::{FakeBackend, Timing};
use crate::prefs::Prefs;

const CONNECTED: &str = "# test config\nblyg-url = https://blyg.example.com\n";

fn setup_with<'a>(
    cx: &'a mut TestAppContext,
    config: &str,
) -> (Entity<MainView>, &'a mut VisualTestContext) {
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
    let (view, cx) = cx.add_window_view(move |window, cx| {
        MainView::new(
            backend,
            Some(fake),
            prefs,
            std::time::Instant::now(),
            window,
            cx,
        )
    });
    cx.run_until_parked();
    (view, cx)
}

fn keys(cx: &mut VisualTestContext, k: &str) {
    cx.simulate_keystrokes(&crate::keymap::keys(k));
    cx.run_until_parked();
}

fn pane(view: &Entity<MainView>, cx: &mut VisualTestContext) -> Option<Entity<TerminalView>> {
    view.read_with(cx, |v, _| v.terminal.view.clone())
}

fn open(view: &Entity<MainView>, cx: &mut VisualTestContext) -> bool {
    view.read_with(cx, |v, _| v.terminal.open)
}

fn focused(view: &Entity<MainView>, cx: &mut VisualTestContext) -> bool {
    let t = pane(view, cx).expect("a session");
    cx.update(|window, cx| t.read(cx).focus.is_focused(window))
}

fn sent(t: &Entity<TerminalView>, cx: &mut VisualTestContext) -> Vec<u8> {
    t.update(cx, |t, _| std::mem::take(&mut t.sent))
}

#[gpui_kit::test]
fn ctrl_backtick_shows_hides_and_keeps_the_session(cx: &mut TestAppContext) {
    let (view, cx) = setup_with(cx, CONNECTED);
    assert!(!open(&view, cx) && pane(&view, cx).is_none());
    keys(cx, "ctrl-`");
    assert!(open(&view, cx), "⌃` shows the pane");
    assert!(focused(&view, cx), "and gives it the keyboard");
    let first = pane(&view, cx).unwrap();
    // It's drawn: a row of its own, laid out into cells.
    let (cols, rows) = first.read_with(cx, |t, _| t.size());
    assert!(cols >= 2 && rows >= 1);
    keys(cx, "ctrl-`");
    assert!(!open(&view, cx), "again hides it");
    assert!(pane(&view, cx).is_some(), "the session keeps running");
    keys(cx, "ctrl-`");
    assert!(open(&view, cx));
    assert_eq!(pane(&view, cx).unwrap().entity_id(), first.entity_id());
}

#[gpui_kit::test]
fn keys_go_to_the_shell_not_the_window(cx: &mut TestAppContext) {
    let (view, cx) = setup_with(cx, CONNECTED);
    keys(cx, "ctrl-`");
    let t = pane(&view, cx).unwrap();
    cx.simulate_input("ls");
    cx.run_until_parked();
    keys(cx, "enter");
    keys(cx, "up");
    keys(cx, "ctrl-c");
    keys(cx, "escape");
    assert_eq!(sent(&t, cx), b"ls\r\x1b[A\x03\x1b");
    assert!(open(&view, cx), "esc went to the shell; the pane stays");
    // The app keeps its ⌘-keys: ⌘L is the omnibar. On Windows the pane keeps
    // plain Ctrl-keys (Ctrl+L clears a shell) and the app keeps Ctrl+Shift:
    // Ctrl+Shift+P is the command palette.
    if cfg!(target_os = "windows") {
        keys(cx, "cmd-shift-p");
        assert!(sent(&t, cx).is_empty());
        assert!(
            !focused(&view, cx),
            "Ctrl+Shift+P took the keyboard to the palette"
        );
    } else {
        keys(cx, "cmd-l");
        assert!(sent(&t, cx).is_empty());
        assert!(!focused(&view, cx), "⌘L took the keyboard to the omnibar");
    }
}

#[gpui_kit::test]
fn output_is_drawn_from_the_engine(cx: &mut TestAppContext) {
    let (view, cx) = setup_with(cx, CONNECTED);
    keys(cx, "ctrl-`");
    let t = pane(&view, cx).unwrap();
    t.update(cx, |t, cx| {
        t.feed(b"hello\r\n\x1b[1;31mred\x1b[0m and \x1b[4mplain\x1b[0m", cx)
    });
    cx.run_until_parked();
    let lines = t.update(cx, |t, _| t.screen_text());
    assert_eq!(lines[0], "hello");
    assert_eq!(lines[1], "red and plain");
    // A query is answered back through the pty (here: `sent`).
    t.update(cx, |t, cx| t.feed(b"\x1b[6n", cx));
    assert_eq!(sent(&t, cx), b"\x1b[2;14R");
    assert_eq!(t.read_with(cx, |t, _| t.engine_name()), blyg_term::ENGINE);
}

#[gpui_kit::test]
fn select_copy_and_paste(cx: &mut TestAppContext) {
    let (view, cx) = setup_with(cx, CONNECTED);
    keys(cx, "ctrl-`");
    let t = pane(&view, cx).unwrap();
    t.update(cx, |t, cx| {
        t.feed(b"first line\r\nsecond", cx);
        t.select(GridPoint { line: 0, col: 6 }, GridPoint { line: 1, col: 5 });
    });
    let copy = if cfg!(target_os = "windows") {
        "ctrl-shift-c"
    } else {
        "cmd-c"
    };
    let paste = if cfg!(target_os = "windows") {
        "ctrl-shift-v"
    } else {
        "cmd-v"
    };
    keys(cx, copy);
    let clip = cx.update(|_, cx| cx.read_from_clipboard().and_then(|c| c.text()));
    assert_eq!(clip.as_deref(), Some("line\nsecond"));
    // A paste into a program that asked for bracketed paste is wrapped, and
    // can't close the bracket itself.
    t.update(cx, |t, cx| t.feed(b"\x1b[?2004h", cx));
    cx.update(|_, cx| {
        cx.write_to_clipboard(ClipboardItem::new_string("echo hi\x1b[201~\n".into()))
    });
    keys(cx, paste);
    assert_eq!(sent(&t, cx), b"\x1b[200~echo hi\r\x1b[201~");
}

#[gpui_kit::test]
fn the_editors_selection_goes_in_quoted(cx: &mut TestAppContext) {
    let (view, cx) = setup_with(cx, CONNECTED);
    view.update_in(cx, |v, window, cx| {
        v.editor.update(cx, |e, cx| {
            e.set_value("A blyg is tended,\nnot fed.", window, cx);
            e.set_selected_range(0..26, cx);
        });
    });
    cx.run_until_parked();
    keys(cx, "cmd-shift-t");
    assert!(open(&view, cx), "⇧⌘T opens the pane");
    let t = pane(&view, cx).unwrap();
    assert_eq!(
        String::from_utf8(sent(&t, cx)).unwrap(),
        "> A blyg is tended,\r> not fed.\r\r"
    );
}

#[gpui_kit::test]
fn the_shell_ending_closes_the_pane(cx: &mut TestAppContext) {
    let (view, cx) = setup_with(cx, CONNECTED);
    keys(cx, "ctrl-`");
    let t = pane(&view, cx).unwrap();
    t.update(cx, |_, cx| cx.emit(TermEvent::Exited));
    cx.run_until_parked();
    assert!(!open(&view, cx));
    assert!(pane(&view, cx).is_none(), "the session is gone");
    keys(cx, "ctrl-`");
    assert!(open(&view, cx), "⌃` starts a new one");
}

#[gpui_kit::test]
fn terminal_off_keeps_it_closed(cx: &mut TestAppContext) {
    let (view, cx) = setup_with(cx, &format!("{CONNECTED}terminal = false\n"));
    keys(cx, "ctrl-`");
    assert!(!open(&view, cx) && pane(&view, cx).is_none());
    assert!(view.read_with(cx, |v, _| {
        v.toast
            .as_ref()
            .is_some_and(|t| t.text.contains("terminal is off"))
    }));
}
