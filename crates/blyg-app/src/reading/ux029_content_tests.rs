//! --- ux 0.29 --- The desktop user-testing pass's content findings
//! (`docs/private/ux-pass-0-29-desktop.md`): what a row, a picker or a scene
//! shows.

use std::sync::Arc;

use blyg_core::config::MemoryTokenStore;
use blyg_core::{Backend, ConfigStore};
use gpui_kit::{Entity, TestAppContext, VisualTestContext};

use crate::app::MainView;
use crate::fake::{FakeBackend, Timing};
use crate::prefs::Prefs;
use crate::test_wait::settle;

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

/// Ada's post with Euler's identity in its first line and a display
/// formula after it, as the fake holds it.
fn maths_row(fake: &FakeBackend) -> blyg_core::ReadingItem {
    fake.reading()
        .into_iter()
        .find(|r| r.remote_id == crate::fake::maths_seed::ADA_MATHS)
        .expect("the maths sample")
}

/// I3: a Reader row says a formula's text, as the Stream's and the Agentic
/// feed's cards do, never its TeX.
#[gpui_kit::test]
fn a_reader_row_says_its_formula_not_the_tex(cx: &mut TestAppContext) {
    let (_, fake, _) = setup(cx);
    let r = maths_row(&fake);
    let title = super::vm::row_title(&r);
    assert!(title.contains("e^(iπ) + 1 = 0"), "{title}");
    assert!(!title.contains('\\'), "{title}");
    // What a quote of it is written with keeps the post's own words
    // (`![[id | label]]` matches the studio's picker).
    assert!(super::vm::quote_label(&r).contains("\\pi"));
}

/// I3: both quote pickers (the Reader's ⌘⇧Q and the editor's `![[`) list
/// the post's formulas as text.
#[gpui_kit::test]
fn the_quote_pickers_say_formulas_not_tex(cx: &mut TestAppContext) {
    let (_, fake, _) = setup(cx);
    let r = maths_row(&fake);
    let q = blyg_core::PickQuery::default();
    let row = blyg_core::pick::imported(&r, true, &q, &[]).expect("pickable");
    let text = crate::composer::picker::row_text(&row, crate::composer::picker::PickKind::Quote);
    assert!(text.title.contains("e^(iπ) + 1 = 0"), "{}", text.title);
    let excerpt = text.excerpt.unwrap_or_default();
    assert!(excerpt.contains("∫₀¹ x² dx = 1/3"), "{excerpt}");
    assert!(!excerpt.contains("\\frac"), "{excerpt}");
}

/// I7: `settings-background-locked` lands on BACKGROUND whatever rows sit
/// above it (0.28 put READ MARKS there, and ⇥ ×5 flipped "Count an entry as
/// read" instead), and its space leaves READ MARKS alone.
#[gpui_kit::test]
fn the_locked_background_scene_lands_on_background(cx: &mut TestAppContext) {
    use crate::app::pref_preview::Row;
    use crate::scenes::Scene;
    let (view, _, cx) = setup(cx);
    let marks_before = view.read_with(cx, |v, _| v.reading.count_read.setting.clone());
    for n in 0..3 {
        view.update_in(cx, |v, window, cx| {
            v.demo_step(Scene::SettingsBackgroundLocked, n, window, cx)
        });
        settle(cx);
    }
    let row = view.read_with(cx, |v, _| v.preview.cursor.as_ref().map(|(r, _)| *r));
    assert_eq!(row, Some(Row::Background));
    assert_eq!(
        view.read_with(cx, |v, _| v.reading.count_read.setting.clone()),
        marks_before,
        "READ MARKS is untouched"
    );
}

/// A window on the sample data with the fake's realistic latency, as a
/// `BLYGGER_DEMO` scene runs: the reading list arrives a moment after the
/// window opens.
fn setup_live(cx: &mut TestAppContext) -> (Entity<MainView>, &mut VisualTestContext) {
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
    let fake = Arc::new(FakeBackend::with_timing(Timing::realistic()).without_media_cache());
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
    (view, cx)
}

/// `tut-<id>+` played as the scene plays it: step 0 enters the step as the
/// window opens, step 1 does its key, step 2 waits; time passes between.
fn play_after_key<'a>(
    id: &'static str,
    cx: &'a mut TestAppContext,
) -> (Entity<MainView>, &'a mut VisualTestContext) {
    use crate::scenes::Scene;
    let (view, cx) = setup_live(cx);
    let scene = Scene::TutStep {
        id,
        after_key: true,
    };
    for n in 0..3 {
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(400));
        view.update_in(cx, |v, window, cx| v.demo_step(scene, n, window, cx));
        for _ in 0..4 {
            cx.executor()
                .advance_clock(std::time::Duration::from_millis(300));
            crate::test_wait::settle_drawn(cx);
        }
    }
    (view, cx)
}

/// I5: the Catch up step's ⇧C catches up on Rue's post, even when the
/// step began before the Stream had filled ("No post to catch up on").
#[gpui_kit::test]
fn the_catch_up_step_catches_up_on_rues_post(cx: &mut TestAppContext) {
    let (view, cx) = play_after_key("catch-up", cx);
    let sel = view.read_with(cx, |v, _| v.reading.sel.clone().map(|k| k.1));
    assert_eq!(
        sel.as_deref(),
        Some(crate::app::onboarding::steps::CATCH_UP),
        "Rue's post is the one in front of you"
    );
    assert!(
        view.read_with(cx, |v, _| matches!(
            v.reading.sheet,
            Some(super::RSheet::CatchUp(_))
        )),
        "⇧C laid the conversation out"
    );
}

/// I5: the "Generated" step's ⏎ opens Lin's marked post, not the newest.
#[gpui_kit::test]
fn the_generated_step_opens_lins_marked_post(cx: &mut TestAppContext) {
    let (view, cx) = play_after_key("generated", cx);
    let open = view.read_with(cx, |v, _| {
        v.reading
            .opened
            .as_ref()
            .map(|o| o.item.remote_id.to_string())
    });
    assert_eq!(
        open.as_deref(),
        Some(crate::app::onboarding::steps::GENERATED)
    );
}

/// I5: the hopper step shows two posts picked, and its h acts on them.
#[gpui_kit::test]
fn the_several_posts_hopper_step_has_two_picked(cx: &mut TestAppContext) {
    let (view, cx) = play_after_key("hopper-several", cx);
    let picked = view.read_with(cx, |v, _| v.reading.picked.set.len());
    assert_eq!(picked, 2, "two posts are picked");
}

/// I10: Windows names the return, tab and delete keys in words: a bare ⏎
/// ("⏎ read more", "Press ⏎") was left as the Mac glyph while "⌘⏎" read
/// "Ctrl+Enter".
#[test]
fn windows_spells_enter_tab_and_backspace_as_words() {
    use crate::platform_text::{Platform, spelled_as};
    let on_windows =
        |s: &str| spelled_as(Platform::Windows, || crate::keymap::hint_owned(s.into()));
    assert_eq!(on_windows("⏎ read more"), "Enter read more");
    assert_eq!(
        on_windows("use ⇥ and the arrow keys; a click or ⏎ keeps it"),
        "use Tab and the arrow keys; a click or Enter keeps it"
    );
    assert_eq!(on_windows("⌫ takes it off"), "Backspace takes it off");
    assert_eq!(on_windows("⌘⏎ posts"), "Ctrl+Enter posts");
    // The Mac keeps its glyphs.
    let on_mac = spelled_as(Platform::Mac, || {
        crate::keymap::hint_owned("⏎ read more".into())
    });
    assert_eq!(on_mac, "⏎ read more");
}

/// I10: the text size row's keys: "Ctrl+" lost its plus.
#[test]
fn windows_spells_the_size_keys_in_full() {
    assert_eq!(
        crate::keymap::windows_hint("⌘+  ⌘−  ⌘0"),
        "Ctrl+Plus  Ctrl+Minus  Ctrl+0"
    );
}

/// I10: a menu's key column (⋯ More's "+ Save ⌥⌘B") follows the wording
/// being shown, as every other hint does, so the Windows wording shows no
/// Mac glyph there either.
#[test]
fn a_menus_key_column_follows_the_spelling() {
    use crate::platform_text::{Platform, spelled_as};
    let k = spelled_as(Platform::Windows, || {
        crate::keymap::action_glyphs("ToggleShelf")
    })
    .expect("Save has a key");
    assert!(!k.contains(['⌘', '⌥', '⇧', '⌃']), "{k}");
    assert!(k.starts_with("Ctrl+"), "{k}");
}
