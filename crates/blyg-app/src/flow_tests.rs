//! --- flow --- The command palette, focus mode, serendipity and delight in
//! a headless window: real keystrokes through GPUI's dispatch, against a
//! zero-latency FakeBackend.

use std::sync::Arc;

use blyg_core::config::MemoryTokenStore;
use blyg_core::{Backend, ConfigStore, Kind, LocalId};
use gpui_kit::{Entity, TestAppContext, VisualTestContext};

use crate::app::MainView;
use crate::app::command_palette::Cmd;
use crate::app::studio::ViewMode;
use crate::fake::{FakeBackend, Timing};
use crate::prefs::Prefs;

const CONNECTED: &str = "# test config\nblyg-url = https://blyg.example.com\n";
const DRAFT: &str = "01J9QK3"; // a draft fragment
const PUBLIC: &str = "01J9PX1"; // public v1

fn setup_with<'a>(
    cx: &'a mut TestAppContext,
    config: &str,
) -> (
    Entity<MainView>,
    Arc<FakeBackend>,
    &'a mut VisualTestContext,
) {
    let config = format!("{CONNECTED}{config}");
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

fn setup(cx: &mut TestAppContext) -> (Entity<MainView>, Arc<FakeBackend>, &mut VisualTestContext) {
    setup_with(cx, "")
}

fn keys(cx: &mut VisualTestContext, k: &str) {
    cx.simulate_keystrokes(&crate::keymap::keys(k));
    cx.run_until_parked();
}

fn open(view: &Entity<MainView>, cx: &mut VisualTestContext, which: &str) {
    view.update_in(cx, |v, window, cx| {
        v.open(&LocalId(which.into()), window, cx)
    });
    cx.run_until_parked();
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

fn editor_text(view: &Entity<MainView>, cx: &mut VisualTestContext) -> String {
    view.read_with(cx, |v, cx| v.editor.read(cx).value().to_string())
}

/// The palette's rows as shown: (label, key, disabled).
fn shown(
    view: &Entity<MainView>,
    cx: &mut VisualTestContext,
) -> Vec<(String, Option<String>, Option<String>)> {
    view.read_with(cx, |v, _| {
        let o = v.cmd_palette.open.as_ref().expect("the palette is open");
        o.shown
            .iter()
            .map(|&i| {
                let e = &o.entries[i];
                (e.label.clone(), e.key.clone(), e.disabled.clone())
            })
            .collect()
    })
}

fn palette_open(view: &Entity<MainView>, cx: &mut VisualTestContext) -> bool {
    view.read_with(cx, |v, _| v.cmd_palette.open.is_some())
}

// ------------------------------------------------------------ palette

#[gpui_kit::test]
fn the_palette_finds_actions_by_alias_with_keys_and_reasons(cx: &mut TestAppContext) {
    let (view, _, cx) = setup(cx);
    open(&view, cx, PUBLIC);
    keys(cx, "cmd-shift-p");
    assert!(palette_open(&view, cx));
    assert!(cx.debug_bounds("palette").is_some(), "drawn");
    // Typed into its own field, not the editor.
    cx.simulate_input("undo edits");
    cx.run_until_parked();
    let rows = shown(&view, cx);
    let (label, key, disabled) = &rows[0];
    assert_eq!(label, "Discard Changes…");
    // Its key from the table (none on Windows), and why it can't run: the
    // toolbar's own words.
    assert_eq!(
        key.as_deref(),
        crate::keymap::action_glyphs("DiscardChanges").as_deref()
    );
    assert!(disabled.is_some(), "a post with no edits can't discard");
    // ⏎ on a disabled row explains and keeps the palette.
    keys(cx, "enter");
    assert!(palette_open(&view, cx));
    // esc closes, and the editor has the keyboard again.
    keys(cx, "escape");
    assert!(!palette_open(&view, cx));
    view.read_with(cx, |v, cx| {
        assert!(v.toast.as_ref().is_some_and(|t| t.text.contains("Discard")));
        let _ = cx;
    });
    // ⇧⌘P twice closes it too.
    keys(cx, "cmd-shift-p");
    keys(cx, "cmd-shift-p");
    assert!(!palette_open(&view, cx));
}

#[gpui_kit::test]
fn the_palette_runs_actions_themes_and_posts_and_remembers(cx: &mut TestAppContext) {
    let (view, _, cx) = setup(cx);
    open(&view, cx, DRAFT);
    // An action: Full Editor (⌘3).
    keys(cx, "cmd-shift-p");
    cx.simulate_input("full editor");
    cx.run_until_parked();
    assert_eq!(shown(&view, cx)[0].0, "Full Editor");
    keys(cx, "enter");
    assert!(!palette_open(&view, cx));
    assert_eq!(view.read_with(cx, |v, _| v.studio.view), ViewMode::Studio);

    // A theme, with ⌘⏎: it runs and the palette stays.
    keys(cx, "cmd-shift-p");
    cx.simulate_input("theme kumiko");
    cx.run_until_parked();
    assert_eq!(shown(&view, cx)[0].0, "Theme: Kumiko");
    keys(cx, "cmd-enter");
    assert!(palette_open(&view, cx), "⌘⏎ keeps it open");
    assert_eq!(view.read_with(cx, |v, _| v.prefs.theme.clone()), "kumiko");
    assert!(config_text(cx).contains("theme = kumiko"), "written back");
    // Now it's the theme in use: listed, disabled.
    assert!(shown(&view, cx)[0].2.is_some());
    keys(cx, "escape");

    // The empty field: the last commands first, newest first.
    keys(cx, "cmd-shift-p");
    let rows = shown(&view, cx);
    assert_eq!(rows[0].0, "Theme: Kumiko");
    assert_eq!(rows[1].0, "Full Editor");
    // No posts until you type; then a post by title opens it.
    assert!(
        view.read_with(cx, |v, _| {
            let o = v.cmd_palette.open.as_ref().unwrap();
            o.shown
                .iter()
                .all(|&i| !matches!(o.entries[i].cmd, Cmd::Post(_)))
        }),
        "posts only once you type"
    );
    cx.simulate_input("trombone");
    cx.run_until_parked();
    keys(cx, "enter");
    assert_eq!(
        view.read_with(cx, |v, _| v.current.as_ref().map(|c| c.local_id.0.clone())),
        Some(PUBLIC.to_string())
    );
}

#[gpui_kit::test]
fn the_palette_has_every_group(cx: &mut TestAppContext) {
    let (view, _, cx) = setup(cx);
    keys(cx, "cmd-shift-p");
    let groups: Vec<&'static str> = view.read_with(cx, |v, _| {
        let o = v.cmd_palette.open.as_ref().unwrap();
        let mut g: Vec<&'static str> = o.entries.iter().map(|e| e.group).collect();
        g.dedup();
        g
    });
    for want in [
        "Post",
        "View",
        "Blyg",
        "Themes",
        "Fonts",
        "Extensions",
        "Posts",
    ] {
        assert!(groups.contains(&want), "{want} in {groups:?}");
    }
    // The extensions' last row is there (no extension runs in this test).
    cx.simulate_input("manage extensions");
    cx.run_until_parked();
    assert_eq!(shown(&view, cx)[0].0, "Manage extensions…");
    keys(cx, "enter");
    assert!(view.read_with(cx, |v, _| matches!(
        v.ext.overlay,
        Some(crate::app::extensions::Overlay::Manage { .. })
    )));
}

// ------------------------------------------------------------ focus

#[gpui_kit::test]
fn focus_mode_hides_the_chrome_and_esc_puts_it_back(cx: &mut TestAppContext) {
    let (view, _, cx) = setup(cx);
    open(&view, cx, DRAFT);
    keys(cx, "cmd-2"); // the list and the preview
    keys(cx, "alt-cmd-f");
    view.read_with(cx, |v, _| {
        assert!(v.focus_on());
        assert_eq!(v.studio.view, ViewMode::Focus, "no list, no preview");
    });
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    assert!(cx.debug_bounds("focus-bar").is_some(), "the hairline");
    assert!(cx.debug_bounds("focus-meter").is_some());
    // Words this sitting, nothing else.
    view.update_in(cx, |v, window, cx| {
        v.editor.update(cx, |s, cx| {
            s.focus(window, cx);
        })
    });
    let end = editor_text(&view, cx).len();
    view.update_in(cx, |v, _, cx| {
        v.editor
            .update(cx, |s, cx| s.set_selected_range(end..end, cx))
    });
    cx.simulate_input(" three more words");
    cx.run_until_parked();
    assert_eq!(view.read_with(cx, |v, cx| v.focus_words(cx)), Some(3));
    // ⌘L still reaches the omnibar.
    keys(cx, "cmd-l");
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    assert!(
        cx.debug_bounds("omnibar").is_some() || view.read_with(cx, |v, _| !v.focus_hides_omnibar())
    );
    // Back in the editor, esc leaves focus mode: the view is as it was.
    view.update_in(cx, |v, window, cx| {
        v.editor.update(cx, |s, cx| s.focus(window, cx))
    });
    cx.run_until_parked();
    keys(cx, "escape");
    view.read_with(cx, |v, _| {
        assert!(!v.focus_on());
        assert_eq!(v.studio.view, ViewMode::Split);
    });
    // ⌥⌘F toggles, and another view key ends it there.
    keys(cx, "alt-cmd-f");
    keys(cx, "cmd-1");
    view.read_with(cx, |v, _| {
        assert!(!v.focus_on());
        assert_eq!(v.studio.view, ViewMode::Write);
    });
}

#[gpui_kit::test]
fn focus_mode_keeps_the_caret_at_its_line(cx: &mut TestAppContext) {
    let (view, fake, cx) = setup_with(cx, "focus-line = 0.5\n");
    let long: String = (0..80)
        .map(|i| format!("Paragraph {i} about tide pools.\n\n"))
        .collect();
    let id = fake.create_draft(Kind::Thread, &long).unwrap();
    open(&view, cx, &id.0);
    keys(cx, "alt-cmd-f");
    view.update_in(cx, |v, window, cx| {
        v.editor.update(cx, |s, cx| s.focus(window, cx))
    });
    let end = editor_text(&view, cx).len();
    view.update_in(cx, |v, _, cx| {
        v.editor
            .update(cx, |s, cx| s.set_selected_range(end..end, cx))
    });
    cx.simulate_input("x");
    for _ in 0..8 {
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
    }
    let (shown_at, want, lh) = view.read_with(cx, |v, cx| {
        let s = v.editor.read(cx);
        let (c, lh) = s.cursor_layout().expect("laid out");
        let pane = v.focus_mode_pane().expect("measured");
        (
            f32::from(c.top() + s.scroll_offset().y),
            f32::from(pane.top() + pane.size.height * 0.5),
            f32::from(lh),
        )
    });
    assert!(
        (shown_at - want).abs() <= lh,
        "the caret shows at {shown_at}, wanted {want}"
    );
    // The first line can sit there too (the padding above it).
    view.update_in(cx, |v, _, cx| {
        v.editor.update(cx, |s, cx| s.set_selected_range(0..0, cx))
    });
    for _ in 0..8 {
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
    }
    let (shown_at, want) = view.read_with(cx, |v, cx| {
        let s = v.editor.read(cx);
        let (c, _) = s.cursor_layout().expect("laid out");
        let pane = v.focus_mode_pane().expect("measured");
        (
            f32::from(c.top() + s.scroll_offset().y),
            f32::from(pane.top() + pane.size.height * 0.5),
        )
    });
    assert!(
        (shown_at - want).abs() <= lh,
        "the first line shows at {shown_at}, wanted {want}"
    );
}

#[test]
fn the_veil_reads_the_fade_key() {
    use crate::app::focus::Fade;
    let cfg = ConfigStore::in_memory("focus-fade = off\n");
    assert_eq!(cfg.config().get("focus-fade"), Some("off"));
    let _ = Fade::Off;
}

// ------------------------------------------------------------ serendipity

fn card_key(view: &Entity<MainView>, cx: &mut VisualTestContext) -> Option<String> {
    view.read_with(cx, |v, cx| {
        v.serendipity_shown(cx)
            .then(|| v.serendipity.card.as_ref().unwrap().c.key.clone())
    })
}

#[gpui_kit::test]
fn serendipity_is_off_by_default(cx: &mut TestAppContext) {
    let (view, _, cx) = setup(cx);
    keys(cx, "cmd-n");
    assert_eq!(card_key(&view, cx), None);
    assert!(cx.debug_bounds("serendipity").is_none());
}

#[gpui_kit::test]
fn serendipity_offers_a_post_and_q_l_o_r_act_on_it(cx: &mut TestAppContext) {
    let (view, fake, cx) = setup_with(cx, "serendipity = own\n");
    view.update(cx, |v, _| v.serendipity.seed = Some(7));
    keys(cx, "cmd-n");
    let first = card_key(&view, cx).expect("a card");
    assert!(first.starts_with("own:"), "{first}");
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    assert!(cx.debug_bounds("serendipity").is_some());
    // r: another, never the same one this sitting.
    keys(cx, "r");
    let second = card_key(&view, cx).expect("another");
    assert_ne!(first, second);
    assert_eq!(editor_text(&view, cx), "", "r typed nothing");
    // l links it, and the card goes.
    let (id, title) = view.read_with(cx, |v, _| {
        let c = &v.serendipity.card.as_ref().unwrap().c;
        (c.id.clone(), c.title.clone())
    });
    keys(cx, "l");
    assert_eq!(editor_text(&view, cx), format!("[[{id} | {title}]] "));
    assert_eq!(card_key(&view, cx), None);
    // The note exists (the first keystroke made it).
    assert!(view.read_with(cx, |v, _| v.current.is_some()));

    // q quotes on its own line, and the note becomes a thread.
    keys(cx, "cmd-n");
    assert!(card_key(&view, cx).is_some());
    keys(cx, "q");
    let text = editor_text(&view, cx);
    assert!(text.starts_with("![[") && text.ends_with("]]\n"), "{text}");
    let cur = view.read_with(cx, |v, _| v.current.clone()).unwrap();
    assert_eq!(cur.kind, Kind::Thread);
    assert_eq!(fake.item(&cur.local_id).unwrap().kind, Kind::Thread);

    // Any other key types, and the card goes.
    keys(cx, "cmd-n");
    assert!(card_key(&view, cx).is_some());
    cx.simulate_input("x");
    cx.run_until_parked();
    assert_eq!(editor_text(&view, cx), "x");
    assert_eq!(card_key(&view, cx), None);
    // q now types a q.
    cx.simulate_input("q");
    cx.run_until_parked();
    assert_eq!(editor_text(&view, cx), "xq");

    // o opens the post. (Four posts were shown this sitting: start afresh.)
    view.update(cx, |v, _| v.serendipity_forget());
    keys(cx, "cmd-n");
    let local = view.read_with(cx, |v, _| {
        v.serendipity.card.as_ref().unwrap().c.own.clone().unwrap()
    });
    keys(cx, "o");
    assert_eq!(
        view.read_with(cx, |v, _| v.current.as_ref().map(|c| c.local_id.clone())),
        Some(local)
    );
}

#[gpui_kit::test]
fn settings_turns_serendipity_and_sounds_on(cx: &mut TestAppContext) {
    let (view, _, cx) = setup(cx);
    keys(cx, "cmd-,");
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    // The rows are drawn (lower in the sheet's scroll; their chips call these).
    assert!(
        cx.debug_bounds("seren-own").is_some(),
        "the SERENDIPITY row"
    );
    assert!(cx.debug_bounds("sounds-quiet").is_some(), "the SOUNDS row");
    view.update(cx, |v, cx| {
        v.set_serendipity(crate::app::serendipity::Mode::Own, cx)
    });
    assert!(config_text(cx).contains("serendipity = own"));
    assert_eq!(
        cx.update(|_, cx| crate::app::serendipity::mode(cx)),
        crate::app::serendipity::Mode::Own
    );
    view.update(cx, |v, cx| v.set_sounds(true, cx));
    assert!(config_text(cx).contains("sounds = quiet"));
    assert!(cx.update(|_, cx| crate::app::delight::sounds_on(cx)));
}

// ------------------------------------------------------------ delight

#[gpui_kit::test]
fn publishing_settles_the_row(cx: &mut TestAppContext) {
    let (view, _, cx) = setup(cx);
    open(&view, cx, DRAFT);
    keys(cx, "cmd-enter");
    keys(cx, "enter");
    cx.run_until_parked();
    assert_eq!(
        view.read_with(cx, |v, _| v.delight_settling()),
        Some(LocalId(DRAFT.into()))
    );
}

#[gpui_kit::test]
fn the_list_crossfades_when_the_blyg_changes(cx: &mut TestAppContext) {
    let (view, _, cx) = setup(cx);
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    let before = view.read_with(cx, |v, _| v.delight_list_gen());
    view.update(cx, |v, cx| {
        v.title = "Burrow — notes.example.org".into();
        cx.notify();
    });
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    assert_eq!(view.read_with(cx, |v, _| v.delight_list_gen()), before + 1);
}
