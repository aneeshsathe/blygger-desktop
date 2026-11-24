//! --- ux 0.28 --- The 0.28.0 user-testing pass's Reading findings: one name
//! for the reader order, the Agentic feed's edition across the reading
//! pane, formulas in cards as readable text, locked Background rules that
//! don't act, Responses' ⏎ opening in Burrow, and Saved naming its key.

use blyg_core::ReadingItem;
use blyg_core::agentic::parse_edition;
use serde_json::json;

use super::maths_text::{plain_maths, tex_text};
use super::sources_vm::{Smart, Source, empty_label};
use crate::platform_text::{Platform, spelled_as};

/// A post with formulas, as the maths scenes publish it.
const MATHS: &str = "Euler's identity, $e^{i\\pi} + 1 = 0$, ties five constants together.\n\n$$\\int_0^1 x^2\\,dx = \\frac{1}{3}$$\n\nAnd money stays money: it cost $5 and $10.";

fn row(md: &str) -> ReadingItem {
    serde_json::from_value(json!({
        "subscription_id": "sub-a", "remote_id": "r1", "subscription_title": "Ada's harbour",
        "origin": "https://ada.blyg.example.com/", "kind": "fragment", "state": "current",
        "version": 1, "created": "2026-11-20T09:00:00Z", "updated": "2026-11-20T09:00:00Z",
        "observed_at": "2026-11-20T09:00:00Z",
        "content_md": md, "content_html": "", "author": null, "page": null,
        "thumb": null, "hoppers": [],
    }))
    .unwrap()
}

/// TeX reads as text: symbols for commands, raised and lowered marks where
/// Unicode has them, fractions with a slash, spacing commands gone.
#[test]
fn tex_reads_as_text() {
    assert_eq!(tex_text("e^{i\\pi} + 1 = 0"), "e^(iπ) + 1 = 0");
    assert_eq!(
        tex_text("\\int_0^1 x^2\\,dx = \\frac{1}{3}"),
        "∫₀¹ x² dx = 1/3"
    );
    assert_eq!(
        tex_text("\\sum_{n=1}^{\\infty} \\frac{1}{n^2} = \\frac{\\pi^2}{6}"),
        "∑ₙ₌₁^∞ 1/n² = π²/6"
    );
    assert_eq!(tex_text("A = \\pi r^2"), "A = π r²");
    assert_eq!(tex_text("\\sqrt{x+1} \\le \\alpha"), "√(x+1) ≤ α");
    assert_eq!(tex_text("\\frac{a+b}{2}"), "(a+b)/2");
    assert_eq!(tex_text("\\text{if } x \\to \\infty"), "if x → ∞");
    // Nothing a reader would see as markup is left.
    for t in [MATHS, "x_{n} * y_1", "\\mathbf{v} \\cdot \\mathbf{w}"] {
        let s = tex_text(t);
        assert!(!s.contains('\\') && !s.contains('{'), "{t} → {s}");
    }
}

/// Text with formulas in it (an assistant's summary): each formula as text,
/// the dollars gone; money stays money.
#[test]
fn formulas_in_text_read_as_text() {
    assert_eq!(
        plain_maths("Why $e^{i\\pi} + 1 = 0$ matters, for $5."),
        "Why e^(iπ) + 1 = 0 matters, for $5."
    );
    assert_eq!(
        plain_maths("$$\\int_0^1 x^2\\,dx$$ is a third"),
        "∫₀¹ x² dx is a third"
    );
    assert_eq!(plain_maths("No maths here."), "No maths here.");
}

/// User testing: an Agentic feed card's excerpt showed raw TeX
/// (`\int_0^1 …`). Its words (and headline, and the assistant's summary)
/// show formulas as text.
#[test]
fn agentic_cards_show_formulas_as_text() {
    let e = parse_edition(&json!({
        "revision": 1, "updatedAt": "2026-11-21T07:00:00Z", "mode": "sections", "draft": null,
        "layout": {
            "title": "Morning edition", "lede": null,
            "agent": { "name": "resident", "model": null },
            "generatedAt": "2026-11-21T06:55:00Z",
            "sections": [{ "title": "Lead", "kind": "lead", "note": null, "entries": [
                { "sub": "sub-a", "remoteId": "r1", "entry": {}, "present": "full" },
                { "sub": "sub-a", "remoteId": "r1", "entry": {},
                  "summary": "On $\\int_0^1 x^2\\,dx$ and why it is a third." },
            ] }]
        }
    }))
    .unwrap();
    let held = row(MATHS);
    let entries = &e.layout.as_ref().unwrap().sections[0].entries;
    let c = super::agentic::card(&entries[0], Some(&held));
    let words = format!("{} {}", c.headline, c.excerpt.clone().unwrap_or_default());
    assert!(!words.contains('\\') && !words.contains('{'), "{words}");
    assert!(c.headline.contains("e^(iπ) + 1 = 0"), "{}", c.headline);
    assert!(
        c.excerpt.as_deref().unwrap().contains("∫₀¹ x² dx = 1/3"),
        "{c:?}"
    );
    let c = super::agentic::card(&entries[1], Some(&held));
    assert_eq!(
        c.summary.as_deref(),
        Some("On ∫₀¹ x² dx and why it is a third.")
    );
}

/// User testing: stream cards showed `$e^{i\pi}$` and `$$…$$` as code. A
/// formula reads as text there too; a code span stays code.
#[test]
fn stream_cards_show_formulas_as_text() {
    let blocks = super::stream::post_blocks(&row(&format!("{MATHS}\n\nRun `ls -l` first.")));
    let text: String = blocks
        .iter()
        .map(|b| b.plain())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        !text.contains('\\') && !text.contains("$e") && !text.contains("$$"),
        "{text}"
    );
    assert!(text.contains("e^(iπ) + 1 = 0"), "{text}");
    assert!(text.contains("∫₀¹ x² dx = 1/3"), "{text}");
    assert!(text.contains("$5 and $10"), "money stays money: {text}");
    let code = blocks.iter().any(|b| match b {
        blyg_render::Block::Para(s) => s.iter().any(|s| s.code && s.text == "ls -l"),
        _ => false,
    });
    assert!(code, "a code span stays code");
}

/// User testing: Saved's empty state named no key. It names ⌥⌘B, spelled
/// as each platform's keys are.
#[test]
fn saved_says_which_key_saves() {
    let src = Source::Smart(Smart::Shelf);
    let mac = spelled_as(Platform::Mac, || empty_label(&src));
    assert!(mac.contains("⌥⌘B") && mac.contains("“+ Save”"), "{mac}");
    let win = spelled_as(Platform::Windows, || empty_label(&src));
    assert!(win.contains("Ctrl+Shift+A") && !win.contains('⌘'), "{win}");
}

// The running app on the sample blyg.
mod app {
    use std::sync::Arc;

    use blyg_core::config::MemoryTokenStore;
    use blyg_core::{Backend, ConfigStore};
    use gpui_kit::{Entity, TestAppContext, VisualTestContext};

    use crate::app::MainView;
    use crate::app::reading::View;
    use crate::app::reading::sources_vm::{Smart, Source};
    use crate::fake::{FakeBackend, Timing};
    use crate::prefs::Prefs;
    use crate::test_wait::settle_drawn as settle;

    const CONNECTED: &str = "# test config\nblyg-url = https://blyg.example.com\n";

    fn setup(
        cx: &mut TestAppContext,
    ) -> (Entity<MainView>, Arc<FakeBackend>, &mut VisualTestContext) {
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

    fn keys(cx: &mut VisualTestContext, k: &str) {
        cx.simulate_keystrokes(&crate::keymap::keys(k));
        cx.run_until_parked();
    }

    /// User testing: the reading header said "Pace-layered | Chronological"
    /// and Settings › READER ORDER "Newest first | Pace-layered". Both say
    /// the studio's words, in the same order.
    #[gpui_kit::test]
    fn the_reader_order_has_one_name(cx: &mut TestAppContext) {
        for o in blyg_core::order::ReaderOrder::ALL {
            assert_eq!(
                super::super::feed::order_label(o),
                o.label(),
                "the studio's words"
            );
        }
        let (view, _, cx) = setup(cx);
        view.update_in(cx, |v, window, cx| v.show_view(View::Reading, window, cx));
        settle(cx);
        for name in ["Pace-layered", "Chronological"] {
            assert!(
                cx.debug_bounds(Box::leak(
                    format!("feed-order-label-{name}").into_boxed_str()
                ))
                .is_some(),
                "the header says {name}"
            );
        }
        keys(cx, "cmd-,");
        settle(cx);
        let pace = cx.debug_bounds("reader-order-label-Pace-layered");
        let chrono = cx.debug_bounds("reader-order-label-Chronological");
        let (Some(pace), Some(chrono)) = (pace, chrono) else {
            panic!("Settings says Pace-layered and Chronological");
        };
        assert!(pace.top() < chrono.top(), "in the header's order");
        assert!(cx.debug_bounds("reader-order-label-Newest first").is_none());
    }

    /// User testing: the edition was squeezed into the list column while
    /// the wide pane said "Pick a post". With no post open it takes the
    /// reading pane; a card still opens its post.
    #[gpui_kit::test]
    fn the_edition_takes_the_reading_pane(cx: &mut TestAppContext) {
        let (view, _, cx) = setup(cx);
        view.update_in(cx, |v, window, cx| {
            v.show_view(View::Reading, window, cx);
            v.reading.mode = super::super::stream_vm::ReadMode::Reader;
            v.select_source(Source::Smart(Smart::Agentic), cx);
        });
        settle(cx);
        let ed = cx
            .debug_bounds("agentic-edition")
            .expect("the edition is drawn");
        let window = cx.update(|window, _| window.viewport_size());
        assert!(
            ed.size.width.as_f32() > window.width.as_f32() * 0.5,
            "the edition is wider than the list column: {ed:?} in {window:?}"
        );
        assert!(
            cx.debug_bounds("reader-list-pane").is_none(),
            "no list column beside it"
        );
        // j opens the edition's first post; the edition moves aside for it.
        keys(cx, "j");
        settle(cx);
        assert!(view.read_with(cx, |v, _| v.reading.opened.is_some()));
        assert!(cx.debug_bounds("agentic-edition").is_some());
        assert!(
            cx.debug_bounds("reader-list-pane").is_some(),
            "the edition in the list column, beside the post"
        );
        // esc puts the edition back across the pane.
        keys(cx, "escape");
        settle(cx);
        view.read_with(cx, |v, _| {
            assert!(v.reading.opened.is_none());
            assert_eq!(v.reading.view, View::Reading, "still Reading");
        });
        assert!(cx.debug_bounds("reader-list-pane").is_none());
    }

    /// User testing: a locked Background rule (a sign-in that can't place)
    /// still took the keyboard's focus, and space did nothing. Locked rules
    /// aren't keyboard stops.
    #[gpui_kit::test]
    fn locked_background_rules_take_no_keys(cx: &mut TestAppContext) {
        let (view, _, cx) = setup(cx);
        keys(cx, "cmd-,");
        settle(cx);
        assert!(view.read_with(cx, |v, _| !v.background_rule_picks().is_empty()));
        view.update(cx, |v, _| v.reading.background.placing = false);
        assert!(
            view.read_with(cx, |v, _| v.background_rule_picks().is_empty()),
            "no stops on locked rules"
        );
    }

    /// User testing: on Responses ↗ and the name open in Burrow but ⏎ went
    /// to the default browser. ⏎ opens the response's post in Burrow's
    /// reader; o still opens the default browser, and the header says so.
    #[gpui_kit::test]
    fn enter_opens_a_response_in_burrow(cx: &mut TestAppContext) {
        let (view, _, cx) = setup(cx);
        keys(cx, "cmd-shift-m");
        settle(cx);
        let hint = view.read_with(cx, |v, _| v.mentions_screen_strings()[1].clone());
        assert!(hint.contains("⏎ open"), "{hint}");
        assert!(hint.contains("o open in browser ↗"), "{hint}");
        keys(cx, "j");
        keys(cx, "enter");
        settle(cx);
        view.read_with(cx, |v, _| {
            assert_eq!(v.reading.view, View::Reading, "the reader, in Burrow");
            assert!(!v.browser.open, "not as a web page");
        });
    }
}
