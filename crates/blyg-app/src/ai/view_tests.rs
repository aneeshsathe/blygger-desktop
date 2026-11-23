//! Headless tests of the AI UI: real keystrokes through GPUI's dispatch,
//! a FakeBackend, and a scripted provider (never a real AI or CLI).

use std::sync::Arc;
use std::time::Duration;

use blyg_ai::cli::CliLocator;
use blyg_ai::{AiError, Endpoints, Provider};
use blyg_core::config::{MemoryTokenStore, TokenStore};
use blyg_core::{Backend, ConfigStore, LocalId};
use gpui_kit::{Entity, TestAppContext, VisualTestContext};

use super::{MainView, Overlay};
use crate::ai::AiGlobal;
use crate::ai::tests_support::FakeProvider;
use crate::app::Mode;
use crate::fake::{FakeBackend, Timing};
use crate::prefs::Prefs;

const CONFIG: &str = "# test config\nblyg-url = https://blyg.example.com\n";
/// The seeded draft that opens first.
const DRAFT: &str = "01J9QK3";
const SECRET: &str = "sk-test-0000aaaa1111bbbb2222cccc3333dddd";

struct Env<'a> {
    view: Entity<MainView>,
    fake: Arc<FakeBackend>,
    tokens: Arc<MemoryTokenStore>,
    cx: &'a mut VisualTestContext,
}

fn setup(cx: &mut TestAppContext, provider: Option<FakeProvider>) -> Env<'_> {
    setup_with(cx, CONFIG, provider)
}

fn setup_with<'a>(
    cx: &'a mut TestAppContext,
    config: &str,
    provider: Option<FakeProvider>,
) -> Env<'a> {
    let config = config.to_string();
    let prefs = Prefs::from_config(ConfigStore::in_memory(&config).config());
    let tokens = Arc::new(MemoryTokenStore::default());
    let t2 = tokens.clone();
    let empty = std::env::temp_dir().join("blygger-ai-tests-no-cli");
    let _ = std::fs::create_dir_all(&empty);
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::app::bind_keys(cx);
        crate::settings::init(ConfigStore::in_memory(&config), t2, None, cx);
        cx.set_global(AiGlobal {
            test_provider: provider.map(|p| Arc::new(p) as Arc<dyn Provider>),
            timeout: Some(Duration::from_secs(60)),
            endpoints: Some(Endpoints {
                cli: CliLocator::only(&empty),
                ..Endpoints::default()
            }),
        });
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
    Env {
        view,
        fake,
        tokens,
        cx,
    }
}

impl Env<'_> {
    fn text(&mut self) -> String {
        self.view
            .read_with(self.cx, |v, cx| v.editor.read(cx).value().to_string())
    }

    /// Open the first draft and put `text` in it, caret at the end.
    fn open_with(&mut self, text: &str) {
        self.cx.simulate_keystrokes("enter");
        self.cx.run_until_parked();
        assert_eq!(self.view.read_with(self.cx, |v, _| v.mode), Mode::Edit);
        self.view.update_in(self.cx, |v, window, cx| {
            let old = v.editor.read(cx).value().to_string();
            v.splice_editor(&old, text, Some(text.len()), window, cx);
        });
        self.cx.run_until_parked();
        assert_eq!(self.text(), text);
    }

    fn wait(&mut self, what: &str, f: impl Fn(&MainView) -> bool) {
        let view = self.view.clone();
        crate::test_wait::until(self.cx, what, |cx| {
            cx.executor().advance_clock(Duration::from_millis(30));
            cx.run_until_parked();
            view.read_with(cx, |v, _| f(v))
        });
    }

    fn wait_idle(&mut self) {
        self.wait("the AI job to finish", |v| v.ai.job.is_none());
    }

    fn toast(&mut self) -> String {
        self.view.read_with(self.cx, |v, _| {
            v.toast
                .as_ref()
                .map(|t| {
                    format!(
                        "{} | {}",
                        t.text,
                        t.sub.as_ref().map(|s| s.to_string()).unwrap_or_default()
                    )
                })
                .unwrap_or_default()
        })
    }

    fn tracked(&self) -> Option<(String, Vec<Option<blyg_core::ScopeProvenance>>)> {
        self.fake.tracked_tk_provenance(&LocalId(DRAFT.into()))
    }
}

#[gpui_kit::test]
fn cmd_g_fills_the_scope_and_records_provenance(cx: &mut TestAppContext) {
    let mut e = setup(
        cx,
        Some(FakeProvider::replying(&[
            "Because the sea keeps its own time.",
        ])),
    );
    e.open_with("Weather again. [TK]say why, briefly[/TK]");
    // Caret just inside the scope.
    e.view.update_in(e.cx, |v, _, cx| {
        v.editor
            .update(cx, |s, cx| s.set_selected_range(20..20, cx));
    });
    e.cx.simulate_keystrokes(&crate::keymap::keys("cmd-g"));
    e.wait_idle();
    assert_eq!(
        e.text(),
        "Weather again. [TK]say why, briefly[=]Because the sea keeps its own time.[/TK]"
    );
    let saves = e.fake.provenance_saves();
    assert_eq!(saves.last(), Some(&(LocalId(DRAFT.into()), 1)), "{saves:?}");
    let (keyed, scopes) = e.tracked().expect("tracked");
    assert_eq!(keyed, e.text());
    let p = scopes[0].as_ref().expect("generated scope has provenance");
    assert_eq!(p.model, "fake-model-1");
    assert!(p.at.is_some());
    assert!(e.toast().contains("fake-model-1"), "{}", e.toast());

    // Regenerate with ⌘G again: two scopes now, the first keeps its record.
    e.view.update_in(e.cx, |v, window, cx| {
        let old = v.editor.read(cx).value().to_string();
        let new = format!("{old} [TK]and end it[/TK]");
        let end = new.len();
        v.splice_editor(&old, &new, Some(end), window, cx);
    });
    e.cx.simulate_keystrokes(&crate::keymap::keys("cmd-g"));
    e.wait_idle();
    assert_eq!(e.fake.provenance_saves().last().unwrap().1, 2);
    let (_, scopes) = e.tracked().unwrap();
    assert!(scopes.iter().all(Option::is_some), "{scopes:?}");

    // Rewriting a generated scope entirely by hand → null provenance.
    e.view.update_in(e.cx, |v, window, cx| {
        let old = v.editor.read(cx).value().to_string();
        let new = old.replacen(
            "Because the sea keeps its own time.",
            "Honestly nobody knows.",
            1,
        );
        v.splice_editor(&old, &new, None, window, cx);
    });
    let (_, scopes) = e.tracked().unwrap();
    assert!(scopes[0].is_none() && scopes[1].is_some(), "{scopes:?}");
}

#[gpui_kit::test]
fn esc_cancels_a_running_generation(cx: &mut TestAppContext) {
    let mut e = setup(cx, Some(FakeProvider::hanging()));
    e.open_with("[TK]write something[/TK]");
    e.cx.simulate_keystrokes(&crate::keymap::keys("cmd-g"));
    e.view.read_with(e.cx, |v, _| {
        let job = v.ai.job.as_ref().expect("running");
        assert_eq!(job.verb, "generating");
        assert_eq!(job.provider.config_name(), "codex");
        assert!(v.render_ai_status().is_some());
    });
    e.cx.simulate_keystrokes("escape");
    e.cx.run_until_parked();
    e.view.read_with(e.cx, |v, _| {
        assert!(v.ai.job.is_none());
        assert_eq!(
            v.mode,
            Mode::Edit,
            "esc cancelled instead of leaving the editor"
        );
    });
    assert!(e.toast().contains("cancelled"));
    // The cancelled job's thread would have answered by now.
    crate::test_wait::wall_clock(Duration::from_millis(30));
    e.cx.executor().advance_clock(Duration::from_millis(100));
    e.cx.run_until_parked();
    assert_eq!(e.text(), "[TK]write something[/TK]", "nothing written");
    assert!(e.fake.provenance_saves().is_empty());
}

#[gpui_kit::test]
fn a_hung_provider_times_out(cx: &mut TestAppContext) {
    let mut e = setup(cx, Some(FakeProvider::hanging()));
    e.open_with("[TK]write something[/TK]");
    e.cx.simulate_keystrokes(&crate::keymap::keys("cmd-g"));
    e.cx.executor().advance_clock(Duration::from_secs(61));
    e.cx.run_until_parked();
    assert!(e.view.read_with(e.cx, |v, _| v.ai.job.is_none()));
    assert!(e.toast().contains("took longer than 60 s"), "{}", e.toast());
}

#[gpui_kit::test]
fn shorten_proposes_then_accepts_or_rejects(cx: &mut TestAppContext) {
    let mut e = setup(cx, Some(FakeProvider::replying(&["Short and sweet."])));
    let long = "word ".repeat(260);
    e.open_with(long.trim_end());
    e.cx.simulate_keystrokes(&crate::keymap::keys("cmd-shift-g"));
    e.wait("the proposal", |v| {
        matches!(v.ai.overlay, Some(Overlay::Shorten { .. }))
    });
    assert_eq!(e.text(), long.trim_end(), "nothing changes until accepted");
    // esc keeps the original.
    e.cx.simulate_keystrokes("escape");
    e.cx.run_until_parked();
    assert!(e.view.read_with(e.cx, |v, _| v.ai.overlay.is_none()));
    assert_eq!(e.text(), long.trim_end());

    e.cx.simulate_keystrokes(&crate::keymap::keys("cmd-shift-g"));
    e.wait("the proposal", |v| {
        matches!(v.ai.overlay, Some(Overlay::Shorten { .. }))
    });
    e.cx.simulate_keystrokes("enter");
    e.cx.run_until_parked();
    let t = e.text();
    assert!(
        t.starts_with("[TK]shorten to fit 1000") && t.ends_with("[=]Short and sweet.[/TK]"),
        "{t}"
    );
    assert_eq!(e.fake.provenance_saves().last().unwrap().1, 1);
    let (_, scopes) = e.tracked().unwrap();
    assert!(scopes[0].is_some());
}

#[gpui_kit::test]
fn proofread_is_applied_without_disclosure(cx: &mut TestAppContext) {
    let reply = r#"[{"original":"Teh cat","replacement":"The cat","reason":"typo"}]"#;
    let mut e = setup(cx, Some(FakeProvider::replying(&[reply])));
    e.open_with("Teh cat sat on the mat.");
    // No TK under the caret: ⌘G opens the helper palette.
    e.cx.simulate_keystrokes(&crate::keymap::keys("cmd-g"));
    e.cx.run_until_parked();
    assert!(e.view.read_with(e.cx, |v, _| matches!(
        v.ai.overlay,
        Some(Overlay::Palette { .. })
    )));
    e.cx.simulate_keystrokes("5"); // Proofread
    e.wait("suggestions", |v| {
        matches!(v.ai.overlay, Some(Overlay::Proofread { .. }))
    });
    e.cx.simulate_keystrokes("enter");
    e.cx.run_until_parked();
    assert_eq!(e.text(), "The cat sat on the mat.");
    assert!(
        e.fake.provenance_saves().is_empty(),
        "proofread isn't disclosed"
    );
    assert!(e.tracked().is_none());
}

#[gpui_kit::test]
fn palette_inserts_a_gap_and_continues(cx: &mut TestAppContext) {
    let mut e = setup(
        cx,
        Some(FakeProvider::replying(&["And then the fog lifted."])),
    );
    e.open_with("The harbour was quiet.");
    e.cx.simulate_keystrokes(&crate::keymap::keys("cmd-g"));
    e.cx.run_until_parked();
    e.cx.simulate_keystrokes("enter"); // first row: fill a gap here
    e.cx.run_until_parked();
    assert_eq!(e.text(), "The harbour was quiet.[TK][/TK]");
    // Undo that and continue the thought instead.
    e.view.update_in(e.cx, |v, window, cx| {
        let old = v.editor.read(cx).value().to_string();
        v.splice_editor(&old, "The harbour was quiet.", Some(22), window, cx);
    });
    e.cx.simulate_keystrokes(&crate::keymap::keys("cmd-g"));
    e.cx.run_until_parked();
    e.cx.simulate_keystrokes("3"); // continue this thought
    e.wait_idle();
    let t = e.text();
    assert!(
        t.starts_with("The harbour was quiet. [TK]continue this thought"),
        "{t}"
    );
    assert!(t.ends_with("[=]And then the fog lifted.[/TK]"), "{t}");
    assert_eq!(e.fake.provenance_saves().last().unwrap().1, 1);
}

#[gpui_kit::test]
fn publishing_undisclosed_generated_text_warns_first(cx: &mut TestAppContext) {
    let mut e = setup(cx, Some(FakeProvider::replying(&["Out."])));
    e.fake.set_provenance_available(false);
    e.open_with("Hi. [TK]go on[/TK]");
    e.cx.simulate_keystrokes(&crate::keymap::keys("cmd-g"));
    e.wait_idle();
    assert!(e.text().contains("[=]Out.[/TK]"));

    e.cx.simulate_keystrokes(&crate::keymap::keys("cmd-enter"));
    e.cx.run_until_parked();
    e.view.read_with(e.cx, |v, _| {
        assert!(matches!(v.ai.overlay, Some(Overlay::PublishWarning { .. })));
        assert!(!v.sheet.is_open(), "no publish sheet behind the warning");
    });
    // ⏎ is Cancel (the default).
    e.cx.simulate_keystrokes("enter");
    e.cx.run_until_parked();
    e.view.read_with(e.cx, |v, _| {
        assert!(v.ai.overlay.is_none() && !v.sheet.is_open());
    });
    assert_eq!(e.fake.item(&LocalId(DRAFT.into())).unwrap().version, 0);

    // P publishes anyway: on to the normal publish sheet.
    e.cx.simulate_keystrokes(&crate::keymap::keys("cmd-enter"));
    e.cx.run_until_parked();
    e.cx.simulate_keystrokes("p");
    e.cx.run_until_parked();
    e.view.read_with(e.cx, |v, _| {
        assert!(v.ai.overlay.is_none());
        assert!(v.sheet.is::<crate::app::PublishSheet>());
    });
}

#[gpui_kit::test]
fn publishing_with_the_extension_does_not_warn(cx: &mut TestAppContext) {
    let mut e = setup(cx, Some(FakeProvider::replying(&["Out."])));
    e.open_with("Hi. [TK]go on[/TK]");
    e.cx.simulate_keystrokes(&crate::keymap::keys("cmd-g"));
    e.wait_idle();
    e.cx.simulate_keystrokes(&crate::keymap::keys("cmd-enter"));
    e.cx.run_until_parked();
    e.view.read_with(e.cx, |v, _| {
        assert!(v.ai.overlay.is_none());
        assert!(v.sheet.is::<crate::app::PublishSheet>());
    });
}

#[gpui_kit::test]
fn no_provider_says_how_to_set_one_up(cx: &mut TestAppContext) {
    let mut e = setup(cx, None);
    e.open_with("[TK]x[/TK]");
    e.cx.simulate_keystrokes(&crate::keymap::keys("cmd-g"));
    e.cx.run_until_parked();
    let t = e.toast();
    assert!(
        t.contains("AI isn't ready") && t.contains("Settings › AI"),
        "{t}"
    );
}

#[gpui_kit::test]
fn an_enabled_cli_that_isnt_installed_says_so(cx: &mut TestAppContext) {
    let mut e = setup_with(cx, &format!("{CONFIG}ai-enable = codex\n"), None);
    e.open_with("[TK]x[/TK]");
    e.cx.simulate_keystrokes(&crate::keymap::keys("cmd-g"));
    e.cx.run_until_parked();
    let t = e.toast();
    assert!(t.contains("isn't installed") && t.contains("codex"), "{t}");
}

#[gpui_kit::test]
fn a_failing_cli_says_what_to_do(cx: &mut TestAppContext) {
    let p = FakeProvider::failing(AiError::CliFailed {
        name: "codex".into(),
        code: Some(1),
        message: "not logged in".into(),
    });
    let mut e = setup(cx, Some(p));
    e.open_with("[TK]x[/TK]");
    e.cx.simulate_keystrokes(&crate::keymap::keys("cmd-g"));
    e.wait_idle();
    let t = e.toast();
    assert!(t.contains("exit code 1") && t.contains("signed in"), "{t}");
    assert_eq!(e.text(), "[TK]x[/TK]");
}

#[gpui_kit::test]
fn api_keys_go_to_the_keychain_and_are_never_echoed(cx: &mut TestAppContext) {
    let mut e = setup(cx, None);
    e.view
        .update_in(e.cx, |v, window, cx| v.ai_open_settings(window, cx));
    e.cx.run_until_parked();
    let input = e.view.read_with(e.cx, |v, _| match &v.ai.overlay {
        Some(Overlay::Settings(s)) => s.anthropic_key.clone(),
        _ => panic!("settings open"),
    });
    e.view.update_in(e.cx, |_, window, cx| {
        input.update(cx, |s, cx| s.focus(window, cx));
    });
    e.cx.simulate_input(SECRET);
    e.cx.simulate_keystrokes("enter");
    e.cx.run_until_parked();

    assert_eq!(
        e.tokens
            .get(blyg_ai::accounts::ANTHROPIC_KEY_ACCOUNT)
            .unwrap()
            .as_deref(),
        Some(SECRET)
    );
    let (field, message, row) = e.view.read_with(e.cx, |v, cx| match &v.ai.overlay {
        Some(Overlay::Settings(s)) => (
            s.anthropic_key.read(cx).value().to_string(),
            s.message.clone(),
            s.rows
                .iter()
                .find(|r| r.kind == blyg_ai::ProviderKind::AnthropicApi)
                .cloned()
                .unwrap(),
        ),
        _ => panic!("settings open"),
    });
    assert_eq!(field, "", "the field is cleared at once");
    let (msg, is_err) = message.unwrap();
    assert!(!is_err && msg.contains("saved"), "{msg}");
    assert!(!msg.contains(SECRET) && !msg.contains("sk-test"));
    assert!(!e.toast().contains("sk-test"));
    assert!(row.enabled);
    assert_eq!(crate::ai::settings::status_label(&row).0, "signed in");
    let config = e.cx.update(|_, cx| {
        crate::settings::get(cx)
            .store
            .text()
            .unwrap_or_default()
            .to_string()
    });
    assert!(config.contains("ai-enable = anthropic"), "{config}");
    assert!(!config.contains("sk-test"), "never in the config file");

    // A bad paste is refused without repeating it.
    e.view.update_in(e.cx, |_, window, cx| {
        input.update(cx, |s, cx| {
            s.set_value("sk-test with spaces", window, cx);
            s.focus(window, cx);
        });
    });
    e.cx.simulate_keystrokes("enter");
    e.cx.run_until_parked();
    let msg = e.view.read_with(e.cx, |v, _| match &v.ai.overlay {
        Some(Overlay::Settings(s)) => s.message.clone().unwrap().0,
        _ => panic!(),
    });
    assert!(!msg.contains("sk-test"), "{msg}");

    // Remove forgets it and switches the provider off.
    e.view.update_in(e.cx, |v, _, cx| {
        v.ai_remove(blyg_ai::ProviderKind::AnthropicApi, cx)
    });
    assert_eq!(
        e.tokens
            .get(blyg_ai::accounts::ANTHROPIC_KEY_ACCOUNT)
            .unwrap(),
        None
    );
}

#[gpui_kit::test]
fn settings_switch_a_cli_on_and_pick_it(cx: &mut TestAppContext) {
    let e = setup(cx, None);
    e.view
        .update_in(e.cx, |v, window, cx| v.ai_open_settings(window, cx));
    e.view.update_in(e.cx, |v, window, cx| {
        v.ai_toggle(blyg_ai::ProviderKind::LocalCodex, true, cx);
        v.ai_use(blyg_ai::ProviderKind::LocalCodex, window, cx);
    });
    let config = e.cx.update(|_, cx| {
        crate::settings::get(cx)
            .store
            .text()
            .unwrap_or_default()
            .to_string()
    });
    assert!(config.contains("ai-enable = codex"), "{config}");
    assert!(config.contains("ai-provider = codex"), "{config}");
    // The CLI isn't installed here, so the row says so.
    let word = e.view.read_with(e.cx, |v, _| match &v.ai.overlay {
        Some(Overlay::Settings(s)) => {
            crate::ai::settings::status_label(
                s.rows
                    .iter()
                    .find(|r| r.kind == blyg_ai::ProviderKind::LocalCodex)
                    .unwrap(),
            )
            .0
        }
        _ => panic!(),
    });
    assert_eq!(word, "not installed");
}

// --- follow-ups --- AI reply to a reading item.

const RUE_TRUST: &str = crate::fake::reading_seed::RUE_TRUST;

impl Env<'_> {
    /// ⌘R, then open a reading row (its current version).
    fn open_reading(&mut self, remote_id: &str) {
        self.cx.simulate_keystrokes(&crate::keymap::keys("cmd-r"));
        self.cx.run_until_parked();
        let key = self.view.read_with(self.cx, |v, _| {
            v.reading_rows()
                .iter()
                .find(|r| r.remote_id == remote_id)
                .map(crate::app::reading::vm::key)
                .expect("row shown")
        });
        self.view
            .update_in(self.cx, |v, window, cx| v.open_reading(key, window, cx));
        self.settle();
    }

    fn settle(&mut self) {
        crate::test_wait::settle(self.cx);
    }

    fn stubs(&self) -> Vec<blyg_core::Item> {
        self.fake
            .items()
            .into_iter()
            .filter(|i| i.stub_of.is_some())
            .collect()
    }
}

#[gpui_kit::test]
fn ai_reply_drafts_a_stub_with_a_disclosed_scope(cx: &mut TestAppContext) {
    let mut e = setup(cx, Some(FakeProvider::replying(&["Trust compounds, too."])));
    e.open_reading(RUE_TRUST);
    let current = e
        .fake
        .reading()
        .into_iter()
        .find(|r| r.remote_id == RUE_TRUST)
        .unwrap();
    assert!(e.view.read_with(e.cx, |v, _| {
        v.pill_model().unwrap().actions.contains(&"AI reply")
    }));
    e.view.update_in(e.cx, |v, window, cx| {
        v.reading_action_for_test("AI reply", window, cx)
    });
    e.wait_idle();

    let stubs = e.stubs();
    assert_eq!(stubs.len(), 1);
    let stub = &stubs[0];
    assert_eq!(stub.stub_of.as_ref().unwrap().id, RUE_TRUST);
    assert_eq!(stub.stub_of.as_ref().unwrap().version, current.version);
    assert_eq!(
        stub.status,
        blyg_core::Status::Draft,
        "reviewed, not published"
    );
    assert_eq!(stub.version, 0);
    let text = e.text();
    assert!(text.starts_with(&format!("![[{RUE_TRUST}]]\n\n")), "{text}");
    assert!(text.contains("[=]Trust compounds, too.[/TK]"), "{text}");
    assert_eq!(stub.content_md, text);
    assert_eq!(
        e.view
            .read_with(e.cx, |v, _| v.current.as_ref().map(|c| c.local_id.clone())),
        Some(stub.local_id.clone()),
        "the stub is open for review"
    );
    // Provenance for the generated scope.
    let (_, scopes) = e.fake.tracked_tk_provenance(&stub.local_id).unwrap();
    assert_eq!(scopes.len(), 1);
    assert_eq!(scopes[0].as_ref().unwrap().model, "fake-model-1");
    assert!(e.toast().starts_with("Reply drafted"), "{}", e.toast());
}

#[gpui_kit::test]
fn ai_reply_is_never_offered_on_a_pinned_version(cx: &mut TestAppContext) {
    let mut e = setup(cx, Some(FakeProvider::replying(&["x"])));
    e.open_reading(RUE_TRUST);
    // Step back to the pinned v3.
    e.cx.simulate_keystrokes("left");
    e.settle();
    let actions = e
        .view
        .read_with(e.cx, |v, _| v.pill_model().unwrap().actions);
    assert!(actions.contains(&"Fork this pin"), "{actions:?}");
    assert!(!actions.contains(&"AI reply"), "{actions:?}");
    // Even if asked directly, nothing happens on a pinned view.
    e.view.update_in(e.cx, |v, window, cx| {
        v.reading_action_for_test("AI reply", window, cx)
    });
    e.cx.run_until_parked();
    assert!(e.stubs().is_empty());
    assert!(e.view.read_with(e.cx, |v, _| v.ai.job.is_none()));
}

#[gpui_kit::test]
fn the_palette_reply_row_follows_the_open_reading_item(cx: &mut TestAppContext) {
    let mut e = setup(cx, Some(FakeProvider::replying(&["Agreed, mostly."])));
    e.open_with("Notes on trust.");
    e.cx.simulate_keystrokes(&crate::keymap::keys("cmd-g"));
    e.cx.run_until_parked();
    let reply_enabled = |e: &mut Env| {
        e.view.read_with(e.cx, |v, _| match &v.ai.overlay {
            Some(Overlay::Palette { entries, .. }) => entries
                .iter()
                .find(|x| x.action == crate::ai::palette::Action::Reply)
                .unwrap()
                .enabled(),
            _ => panic!("no palette"),
        })
    };
    assert!(!reply_enabled(&mut e), "nothing open in Reading yet");
    e.cx.simulate_keystrokes("escape");
    e.cx.run_until_parked();

    e.open_reading(RUE_TRUST);
    // Back to the draft, ⌘G: the row is on now.
    e.view.update_in(e.cx, |v, window, cx| {
        v.show_view(crate::app::reading::View::Posts, window, cx);
        v.open(&LocalId(DRAFT.into()), window, cx);
    });
    e.cx.run_until_parked();
    e.cx.simulate_keystrokes(&crate::keymap::keys("cmd-g"));
    e.cx.run_until_parked();
    assert!(reply_enabled(&mut e));
    e.cx.simulate_keystrokes("6"); // Reply to a reading item
    e.wait_idle();
    let stubs = e.stubs();
    assert_eq!(stubs.len(), 1);
    assert!(
        stubs[0].content_md.contains("[=]Agreed, mostly.[/TK]"),
        "{}",
        stubs[0].content_md
    );
}

// --- studio 0.71 --- ⌘G on the blyg: a link it can't read stops it at a
// checkpoint (the fake blyg can't read a `.invalid` host), and what it read
// is listed.

/// The blyg's own generation, as ⌘G picks it.
fn on_blyg() -> FakeProvider {
    FakeProvider {
        kind: blyg_ai::ProviderKind::BlygServer,
        ..FakeProvider::replying(&["never asked"])
    }
}

const UNREAD: &str = "Fog. [TK]sum up https://gone.invalid/a and https://news.example.com/b[/TK]";

impl Env<'_> {
    /// The draft holds `text` on the blyg too, the caret inside its gap.
    fn on_the_blyg(&mut self, text: &str) {
        self.open_with(text);
        self.view.update_in(self.cx, |v, _, cx| {
            v.editor
                .update(cx, |s, cx| s.set_selected_range(10..10, cx));
        });
        let fake = self.fake.clone();
        let want = text.to_string();
        crate::test_wait::until(self.cx, "the draft to reach the blyg", move |cx| {
            cx.executor().advance_clock(Duration::from_secs(2));
            cx.run_until_parked();
            fake.item(&LocalId(DRAFT.into()))
                .is_some_and(|i| !i.pending_sync && i.content_md == want)
        });
    }

    fn checkpoint(&mut self) -> Option<(String, bool, Vec<String>)> {
        self.view.read_with(self.cx, |v, _| {
            v.sheet
                .get::<super::checkpoint::TkCheckpointSheet>()
                .map(|s| {
                    (
                        super::checkpoint::heading(&s.checkpoint),
                        s.details,
                        s.checkpoint.failed.iter().map(|f| f.link.clone()).collect(),
                    )
                })
        })
    }

    fn click(&mut self, selector: &'static str) {
        let at = crate::test_wait::still(self.cx, selector);
        self.cx
            .simulate_click(at.center(), gpui_kit::Modifiers::none());
        self.cx.run_until_parked();
    }
}

#[gpui_kit::test]
fn a_link_the_blyg_cant_read_stops_at_the_checkpoint_and_esc_keeps_the_draft(
    cx: &mut TestAppContext,
) {
    let mut e = setup(cx, Some(on_blyg()));
    e.on_the_blyg(UNREAD);
    e.cx.simulate_keystrokes(&crate::keymap::keys("cmd-g"));
    e.wait("the checkpoint", |v| {
        v.sheet.is::<super::checkpoint::TkCheckpointSheet>()
    });
    let (heading, details, failed) = e.checkpoint().unwrap();
    assert_eq!(
        heading,
        "1 of 2 links in this TK could not be read. Generate anyway?"
    );
    assert_eq!(failed, vec!["https://gone.invalid/a".to_string()]);
    // The keys are shown, and with one failure Details is open already.
    assert!(e.cx.debug_bounds("tk-checkpoint-generate").is_some());
    assert!(e.cx.debug_bounds("tk-checkpoint-cancel").is_some());
    assert!(e.cx.debug_bounds("tk-checkpoint-details-key").is_some());
    assert!(details, "Details opens by itself for a few failures");
    assert!(e.cx.debug_bounds("tk-checkpoint-failed-0").is_some());
    // d hides it and shows it again; so does a click.
    e.cx.simulate_keystrokes("d");
    e.cx.run_until_parked();
    assert!(!e.checkpoint().unwrap().1, "d closed Details");
    assert!(e.cx.debug_bounds("tk-checkpoint-failed-0").is_none());
    e.click("tk-checkpoint-details");
    assert!(e.checkpoint().unwrap().1, "Details opened");
    assert!(e.cx.debug_bounds("tk-checkpoint-failed-0").is_some());
    // esc: nothing generated, nothing changed, and it says what to do.
    e.cx.simulate_keystrokes("escape");
    e.cx.run_until_parked();
    assert!(e.checkpoint().is_none(), "the sheet closed");
    let toast = e.toast();
    assert!(
        toast.starts_with("Nothing was generated | Remove or replace the link"),
        "{toast}"
    );
    assert_eq!(e.text(), UNREAD);
    assert_eq!(
        e.fake.item(&LocalId(DRAFT.into())).unwrap().content_md,
        UNREAD,
        "the blyg saved nothing"
    );
    assert!(e.tracked().is_none(), "no provenance");
}

#[gpui_kit::test]
fn enter_generates_anyway_and_lists_what_it_read(cx: &mut TestAppContext) {
    let mut e = setup(cx, Some(on_blyg()));
    e.on_the_blyg(UNREAD);
    e.cx.simulate_keystrokes(&crate::keymap::keys("cmd-g"));
    e.wait("the checkpoint", |v| {
        v.sheet.is::<super::checkpoint::TkCheckpointSheet>()
    });
    e.cx.simulate_keystrokes("enter");
    e.wait("the text the blyg generated", |v| {
        v.ai.job.is_none() && !v.sheet.is_open()
    });
    assert_eq!(
        e.toast(),
        "Generated on your blyg | Your blyg noted what it read"
    );
    let view = e.view.clone();
    crate::test_wait::until(e.cx, "the spliced text in the editor", move |cx| {
        cx.run_until_parked();
        view.read_with(cx, |v, cx| {
            v.editor
                .read(cx)
                .value()
                .contains("[=]A line the fake blyg wrote.[/TK]")
        })
    });
    // The page it read is recorded with the scope, and listed.
    let (_, scopes) = e.tracked().expect("the blyg's provenance");
    let p = scopes[0].as_ref().expect("the scope's");
    assert_eq!(p.pages.len(), 1);
    assert_eq!(p.pages[0].url, "https://news.example.com/b");
    let read = e.view.read_with(e.cx, |v, cx| v.tk_read(cx));
    assert_eq!(read.len(), 1, "{read:?}");
    let (scope, entries) = &read[0];
    assert_eq!(*scope, 0);
    // The page's site is its host: said once, as its address.
    assert_eq!(entries[0].what, "read");
    assert_eq!(
        entries[0].url.as_deref(),
        Some("https://news.example.com/b")
    );
    assert!(!entries[0].tells, "a web page tells no one");
    // The publish sheet shows it before anything is sent.
    e.cx.simulate_keystrokes(&crate::keymap::keys("cmd-enter"));
    e.cx.run_until_parked();
    assert!(
        e.cx.debug_bounds("tk-read").is_some(),
        "listed in the sheet"
    );
    assert!(e.cx.debug_bounds("tk-read-0-0").is_some());
}

#[gpui_kit::test]
fn a_local_provider_never_stops_at_a_checkpoint(cx: &mut TestAppContext) {
    let mut e = setup(cx, Some(FakeProvider::replying(&["Grey all day."])));
    e.on_the_blyg(UNREAD);
    e.cx.simulate_keystrokes(&crate::keymap::keys("cmd-g"));
    e.wait_idle();
    assert!(e.checkpoint().is_none());
    assert!(e.text().contains("[=]Grey all day.[/TK]"), "{}", e.text());
    let (_, scopes) = e.tracked().expect("tracked");
    let p = scopes[0].as_ref().unwrap();
    assert_eq!(p.model, "fake-model-1");
    assert!(p.pages.is_empty(), "Burrow's own provider reads no pages");
}

/// What generation read is listed up to 50 entries, then how many more:
/// the blyg's own generate decides how many pages a scope read.
#[gpui_kit::test]
fn a_long_list_of_what_generation_read_shows_the_first_fifty(cx: &mut TestAppContext) {
    let mut e = setup(cx, None);
    let text = "[TK]sum up the news[=]A summary.[/TK]";
    e.open_with(text);
    let pages: Vec<serde_json::Value> = (0..60)
        .map(|i| {
            let url = format!("https://news.example.com/{i}");
            serde_json::json!({ "url": url, "cited": { "source": "News", "url": url, "retrieved": "2030-01-01" } })
        })
        .collect();
    let scope: blyg_core::ScopeProvenance =
        serde_json::from_value(serde_json::json!({ "model": "m", "sources": pages })).unwrap();
    e.fake
        .save_with_provenance(&LocalId(DRAFT.into()), text, &[Some(scope)])
        .unwrap();
    e.cx.run_until_parked();
    e.cx.simulate_keystrokes(&crate::keymap::keys("cmd-enter"));
    e.cx.run_until_parked();
    assert!(e.cx.debug_bounds("tk-read-0-49").is_some(), "the 50th");
    assert!(e.cx.debug_bounds("tk-read-0-50").is_none(), "no 51st");
    assert!(e.cx.debug_bounds("tk-read-more").is_some(), "+10 more");
}

// --- studio 0.71 user testing ---

/// Finding 3: ⏎ while another AI job runs did nothing. The sheet stays,
/// and a toast says why.
#[gpui_kit::test]
fn enter_while_another_ai_job_runs_keeps_the_checkpoint_and_says_why(cx: &mut TestAppContext) {
    let mut e = setup(cx, Some(on_blyg()));
    e.on_the_blyg(UNREAD);
    e.cx.simulate_keystrokes(&crate::keymap::keys("cmd-g"));
    e.wait("the checkpoint", |v| {
        v.sheet.is::<super::checkpoint::TkCheckpointSheet>()
    });
    e.view.update(e.cx, |v, _| {
        v.ai.job = Some(super::Job {
            id: 999,
            provider: blyg_ai::ProviderKind::BlygServer,
            cancel: Default::default(),
            verb: "generating",
        })
    });
    e.cx.simulate_keystrokes("enter");
    e.cx.run_until_parked();
    assert!(e.checkpoint().is_some(), "the sheet stays");
    assert_eq!(
        e.toast(),
        "Another AI job is still running | When it's done, press ⏎ again"
    );
    e.view.update(e.cx, |v, _| v.ai.job = None);
}

/// Sixty pages read, in a 720 px window.
fn a_long_list(cx: &mut TestAppContext) -> Env<'_> {
    let mut e = setup(cx, None);
    e.cx.simulate_resize(gpui_kit::size(gpui_kit::px(1100.), gpui_kit::px(720.)));
    let text = "[TK]sum up the news[=]A summary.[/TK]";
    e.open_with(text);
    let pages: Vec<serde_json::Value> = (0..60)
        .map(|i| {
            let url = format!("https://site{i}.example.org/harbour/tides/{i}");
            serde_json::json!({ "url": url, "cited": { "url": url, "retrieved": "2030-01-01" } })
        })
        .collect();
    let scope: blyg_core::ScopeProvenance =
        serde_json::from_value(serde_json::json!({ "model": "m", "sources": pages })).unwrap();
    e.fake
        .save_with_provenance(&LocalId(DRAFT.into()), text, &[Some(scope)])
        .unwrap();
    e.cx.run_until_parked();
    e
}

fn window_h(e: &mut Env) -> f32 {
    e.cx.update(|window, _| f32::from(window.viewport_size().height))
}

/// The BLOCKER: fourteen or more entries pushed the note field and the
/// keys off the window. The list scrolls inside a fixed height now.
#[gpui_kit::test]
fn a_long_list_scrolls_and_the_publish_keys_stay_on_screen(cx: &mut TestAppContext) {
    let mut e = a_long_list(cx);
    e.cx.simulate_keystrokes(&crate::keymap::keys("cmd-enter"));
    e.cx.run_until_parked();
    let list = e.cx.debug_bounds("tk-read-list").expect("the list");
    assert!(
        f32::from(list.size.height) <= super::checkpoint::READ_LIST_MAX_H + 0.5,
        "{list:?}"
    );
    let keys = e.cx.debug_bounds("publish-keys").expect("the keys");
    let h = window_h(&mut e);
    assert!(f32::from(keys.bottom()) <= h, "{keys:?} in {h}");
    assert!(e.cx.debug_bounds("tk-read-tells-no-one").is_some());
}

/// Any tall sheet stops short of the window's bottom and scrolls.
#[gpui_kit::test]
fn a_tall_sheet_stops_short_of_the_window_bottom(cx: &mut TestAppContext) {
    let mut e = setup(cx, Some(on_blyg()));
    e.cx.simulate_resize(gpui_kit::size(gpui_kit::px(1100.), gpui_kit::px(600.)));
    e.open_with(UNREAD);
    let failed = (0..40)
        .map(|i| blyg_core::generate::UnreadLink {
            link: format!("https://gone{i}.invalid/a"),
            reason: "HTTP 404".into(),
        })
        .collect();
    let checkpoint = blyg_core::generate::Checkpoint {
        read: 0,
        total: 40,
        failed,
    };
    e.view.update_in(e.cx, |v, window, cx| {
        v.tk_checkpoint_open(
            LocalId(DRAFT.into()),
            0,
            blyg_ai::ProviderKind::BlygServer,
            checkpoint,
            window,
            cx,
        )
    });
    e.cx.run_until_parked();
    assert!(
        !e.checkpoint().unwrap().1,
        "Details waits for a click with this many"
    );
    e.cx.simulate_keystrokes("d");
    e.cx.run_until_parked();
    assert!(e.cx.debug_bounds("tk-checkpoint-failed-39").is_some());
    let sheet = e.cx.debug_bounds("sheet").expect("the sheet");
    let h = window_h(&mut e);
    assert!(f32::from(sheet.bottom()) <= h, "{sheet:?} in {h}");
}

/// Finding 2: a post of yours that generation drew on is named by its
/// title and opens here, never by its id; one not held, by its blyg.
#[gpui_kit::test]
fn a_post_held_here_is_listed_by_its_title(cx: &mut TestAppContext) {
    let e = setup(cx, None);
    let own = e
        .fake
        .items()
        .into_iter()
        .find(|i| i.server_id.is_some() && i.local_id.0 != DRAFT)
        .expect("a published post");
    let source = |id: &str, origin: Option<&str>| blyg_core::ProvenanceSource {
        id: id.into(),
        origin: origin.map(str::to_string),
        ..Default::default()
    };
    let names = crate::ai::read::Names::new(&*e.fake, None);
    let mine = source(&own.server_id.as_ref().unwrap().0, None);
    let entry = crate::ai::read::post(&mine, names.get(&mine), true);
    let title = crate::vm::item_title(&own);
    assert_eq!(entry.what, format!("drew on your post “{title}”"));
    assert_eq!(
        entry.link.as_ref().map(|(_, o)| o.clone()),
        Some(crate::ai::read::Open::Own(own.local_id.clone()))
    );
    let far = source("01kunheld", Some("https://kit.blyg.example.com"));
    assert_eq!(names.get(&far), None);
    let entry = crate::ai::read::post(&far, None, true);
    assert_eq!(entry.what, "drew on a post on kit.blyg.example.com");
}
