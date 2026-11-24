//! --- agentic writer --- Burrow's own AI writes the blyg's Agentic feed
//! edition: the job the app runs (a scripted provider and the fake blyg,
//! never a real model), when it runs after a sync, what the tab says while
//! it writes and when it can't, and the key that asks for one.

use std::sync::Arc;

use blyg_ai::{AiError, CancelFlag, ProviderKind};
use blyg_core::Backend;
use blyg_core::agentic::DEFAULT_FEED;

use super::agentic_writer::{Job, Outcome, agent_name, failed_words, run, writing_words};
use crate::ai::tests_support::FakeProvider;
use crate::fake::{FakeBackend, Timing};
use crate::platform_text::{Platform, spelled_as};

const HOUR: i64 = 3_600_000;

fn fake() -> Arc<FakeBackend> {
    Arc::new(FakeBackend::with_timing(Timing::instant()).without_media_cache())
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

fn job(auto: bool) -> Job {
    Job {
        auto,
        now_ms: now_ms(),
        gap_ms: 3 * HOUR,
        model: None,
        agent: agent_name(ProviderKind::LocalCodex),
    }
}

const REPLY: &str = r#"{"title": "Invented paper", "lede": "Tides, mostly.",
  "sections": [{"title": "Lead", "kind": "lead",
    "entries": [{"ref": 1, "headline": "Tides were wrong", "reason": "You follow tides."},
                {"ref": 999, "headline": "Never given"}]}]}"#;

#[test]
fn burrow_is_named_as_the_assistant() {
    assert_eq!(
        agent_name(ProviderKind::LocalCodex),
        "Burrow · Codex (local)"
    );
    assert_eq!(
        agent_name(ProviderKind::AnthropicApi),
        "Burrow · Anthropic API key"
    );
}

/// Asked for, it reads the reading list, asks the provider once (posts by
/// number, never their ids), and sends what it wrote: the fake blyg then
/// serves the edition, Burrow and the model named.
#[test]
fn a_written_edition_reaches_the_blyg() {
    let f = fake();
    let p = FakeProvider::replying(&[REPLY]);
    let out = run(f.as_ref(), &p, &job(false), &CancelFlag::new());
    let Outcome::Written(ed) = out else {
        panic!("written: {out:?}");
    };
    let l = ed.layout.as_ref().expect("laid out");
    assert_eq!(l.title.as_deref(), Some("Invented paper"));
    assert_eq!(l.agent.name, "Burrow · Codex (local)");
    assert_eq!(l.agent.model.as_deref(), Some("fake-model-1"));
    assert_eq!(ed.keys().len(), 1, "the one it wasn't given is left out");
    let served = f.agentic_edition(DEFAULT_FEED).unwrap().unwrap();
    assert_eq!(served.revision, ed.revision);
    assert_eq!(f.agentic_puts(), 1);
    let seen = p.seen.lock().unwrap();
    assert_eq!(seen.len(), 1, "one call to the provider");
    assert!(seen[0].1.contains("[1] "), "posts by number: {}", seen[0].1);
}

/// Output it can't use is said, and nothing is sent.
#[test]
fn unusable_output_sends_nothing() {
    let f = fake();
    for reply in [
        "Sorry, I can't.",
        r#"{"sections": [{"title": "x", "entries": [{"ref": 999}]}]}"#,
    ] {
        let p = FakeProvider::replying(&[reply]);
        let out = run(f.as_ref(), &p, &job(false), &CancelFlag::new());
        assert!(matches!(out, Outcome::Failed(_)), "{out:?}");
    }
    let p = FakeProvider::failing(AiError::Network("unreachable".into()));
    let Outcome::Failed(why) = run(f.as_ref(), &p, &job(false), &CancelFlag::new()) else {
        panic!("failed");
    };
    assert!(!why.is_empty());
    assert_eq!(f.agentic_puts(), 0, "nothing sent");
}

/// After a sync it writes only when the gap since the last edition has
/// passed and posts arrived since; otherwise the provider isn't asked.
#[test]
fn after_a_sync_it_writes_only_when_due() {
    let f = fake();
    let p = FakeProvider::replying(&[REPLY]);
    // The fake's sample edition is days old, and its posts are newer.
    assert!(matches!(
        run(f.as_ref(), &p, &job(true), &CancelFlag::new()),
        Outcome::Written(_)
    ));
    // Just written: not due again for three hours.
    let again = Job {
        now_ms: now_ms() + HOUR,
        ..job(true)
    };
    assert!(matches!(
        run(f.as_ref(), &p, &again, &CancelFlag::new()),
        Outcome::Skipped
    ));
    assert_eq!(p.seen.lock().unwrap().len(), 1, "asked once");
    // Asked for, it writes whatever the gap.
    assert!(matches!(
        run(f.as_ref(), &p, &job(false), &CancelFlag::new()),
        Outcome::Written(_)
    ));
}

/// A blyg that doesn't let this sign-in write editions is never asked,
/// and nor is the provider.
#[test]
fn a_blyg_without_put_agentic_is_not_written_to() {
    let f = fake();
    f.set_agentic_writable(false);
    let p = FakeProvider::replying(&[REPLY]);
    assert!(matches!(
        run(f.as_ref(), &p, &job(false), &CancelFlag::new()),
        Outcome::NotOffered
    ));
    assert!(p.seen.lock().unwrap().is_empty());
}

/// What the tab says while it writes and when it couldn't, and how one
/// gets made when there's none: Settings, the key, or an assistant.
#[test]
fn the_tab_says_who_is_writing_and_how_one_gets_made() {
    assert_eq!(
        writing_words("Codex (local)"),
        "Writing an edition with Codex (local)…"
    );
    assert!(
        failed_words("the model said nothing usable").starts_with("Couldn't write an edition: ")
    );
    let note = super::agentic::empty_note("Daily digest");
    assert!(note.starts_with("No edition yet."), "{note}");
    assert!(note.contains("Settings › Agentic feed"), "{note}");
    assert!(note.contains(crate::keymap::hint("⌥⌘E")), "{note}");
    assert!(note.contains("Blyg › AI on Your Blyg…"), "{note}");
    let win = spelled_as(Platform::Windows, || {
        super::agentic::empty_note("Daily digest")
    });
    assert!(win.contains("Ctrl+Shift+J"), "{win}");
    assert!(!win.contains('⌘'), "{win}");
}

/// The command is in the keymap (so in the menus and the palette), with
/// its key.
#[test]
fn write_an_edition_now_is_a_command_with_a_key() {
    // In macOS terms on every platform; the Windows respelling is asserted below.
    let row = crate::keymap::mac_table()
        .into_iter()
        .find(|k| k.action == "WriteAgenticEdition")
        .expect("in the keymap");
    assert_eq!(row.key, "alt-cmd-e");
    assert!(
        row.label.starts_with("Write an edition now"),
        "{}",
        row.label
    );
    assert!(row.menu.is_some(), "in a menu, and so in the palette");
    assert_eq!(
        crate::keymap::respell_for_windows("alt-cmd-e"),
        "ctrl-shift-j"
    );
}

mod app {
    use std::sync::Arc;
    use std::time::Duration;

    use blyg_ai::Provider;
    use blyg_core::config::MemoryTokenStore;
    use blyg_core::{Backend, ConfigStore};
    use gpui_kit::{Entity, TestAppContext, VisualTestContext};

    use super::super::agentic_writer::WriterState;
    use super::{FakeProvider, REPLY, fake};
    use crate::ai::AiGlobal;
    use crate::app::MainView;
    use crate::app::reading::View;
    use crate::app::reading::sources_vm::{Smart, Source};
    use crate::fake::FakeBackend;
    use crate::prefs::Prefs;

    const CONFIG: &str = "# test config\nblyg-url = https://blyg.example.com\n";

    fn setup<'a>(
        cx: &'a mut TestAppContext,
        config: &str,
        provider: FakeProvider,
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
            cx.set_global(AiGlobal {
                test_provider: Some(Arc::new(provider) as Arc<dyn Provider>),
                timeout: Some(Duration::from_secs(60)),
                endpoints: None,
            });
        });
        let f = fake();
        let backend: Arc<dyn Backend> = f.clone();
        let f2 = f.clone();
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
        (view, f, cx)
    }

    fn wait(
        cx: &mut VisualTestContext,
        view: &Entity<MainView>,
        what: &str,
        f: impl Fn(&MainView) -> bool,
    ) {
        let view = view.clone();
        crate::test_wait::until(cx, what, |cx| {
            cx.executor().advance_clock(Duration::from_millis(30));
            cx.run_until_parked();
            view.read_with(cx, |v, _| f(v))
        });
    }

    /// While the provider works the tab says "Writing an edition with …"
    /// (read before the test executor runs the job to its end).
    #[gpui_kit::test]
    fn the_tab_says_it_is_writing(cx: &mut TestAppContext) {
        let (view, _f, cx) = setup(cx, CONFIG, FakeProvider::replying(&[REPLY]));
        view.update_in(cx, |v, window, cx| {
            v.show_view(View::Reading, window, cx);
            v.reading.mode = super::super::stream_vm::ReadMode::Reader;
            v.select_source(Source::Smart(Smart::Agentic), cx);
            v.agentic_write_now(cx);
            assert_eq!(
                v.reading.agentic_writer.state(),
                &WriterState::Writing {
                    provider: "Codex (local)".into()
                }
            );
        });
    }

    /// ⌥⌘E in the Agentic feed tab: the tab then shows the edition Burrow
    /// wrote.
    #[gpui_kit::test]
    fn the_key_writes_an_edition_and_the_tab_shows_it(cx: &mut TestAppContext) {
        let (view, f, cx) = setup(cx, CONFIG, FakeProvider::replying(&[REPLY]));
        view.update_in(cx, |v, window, cx| {
            v.show_view(View::Reading, window, cx);
            v.reading.mode = super::super::stream_vm::ReadMode::Reader;
            v.select_source(Source::Smart(Smart::Agentic), cx);
        });
        cx.run_until_parked();
        cx.simulate_keystrokes(&crate::keymap::keys("alt-cmd-e"));
        wait(cx, &view, "the edition is written", |v| {
            *v.reading.agentic_writer.state() == WriterState::Idle
                && v.reading.agentic.edition().is_some_and(|e| {
                    e.layout
                        .as_ref()
                        .is_some_and(|l| l.agent.name == "Burrow · Codex (local)")
                })
        });
        assert_eq!(f.agentic_puts(), 1);
    }

    /// With `agentic-writer = after-sync`, a sync that brings posts starts
    /// it in the background; off (the default), nothing is written.
    #[gpui_kit::test]
    fn a_sync_writes_one_only_when_turned_on(cx: &mut TestAppContext) {
        let on = format!("{CONFIG}agentic-writer = after-sync\n");
        let (view, f, cx) = setup(cx, &on, FakeProvider::replying(&[REPLY]));
        view.update(cx, |v, cx| v.reading_changed(cx));
        wait(cx, &view, "written after the sync", |_| {
            f.agentic_puts() == 1
        });
        let _ = view;
    }

    #[gpui_kit::test]
    fn off_by_default_a_sync_writes_nothing(cx: &mut TestAppContext) {
        let p = FakeProvider::replying(&[REPLY]);
        let (view, f, cx) = setup(cx, CONFIG, p);
        view.update(cx, |v, cx| v.reading_changed(cx));
        cx.executor().advance_clock(Duration::from_millis(500));
        cx.run_until_parked();
        assert_eq!(f.agentic_puts(), 0);
        view.read_with(cx, |v, _| {
            assert_eq!(v.reading.agentic_writer.state(), &WriterState::Idle)
        });
    }

    /// A failure is said calmly in the tab, with a way to try again.
    #[gpui_kit::test]
    fn a_failure_is_shown_with_a_retry(cx: &mut TestAppContext) {
        let (view, f, cx) = setup(cx, CONFIG, FakeProvider::replying(&["no json here"]));
        view.update_in(cx, |v, window, cx| {
            v.show_view(View::Reading, window, cx);
            v.reading.mode = super::super::stream_vm::ReadMode::Reader;
            v.select_source(Source::Smart(Smart::Agentic), cx);
            v.agentic_write_now(cx);
        });
        wait(cx, &view, "the failure", |v| {
            matches!(v.reading.agentic_writer.state(), WriterState::Failed(_))
        });
        assert_eq!(f.agentic_puts(), 0);
        crate::test_wait::settle_drawn(cx);
        assert!(cx.debug_bounds("agentic-writer-retry").is_some(), "a retry");
    }
}

#[test]
fn the_fake_blyg_starts_with_an_empty_edition_on_request() {
    let f = fake();
    f.agentic_clear();
    let e = f.agentic_edition(DEFAULT_FEED).unwrap().unwrap();
    assert!(e.is_empty());
    assert_eq!(e.revision, 0);
}
