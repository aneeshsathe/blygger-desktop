//! Headless GPUI tests of onboarding and the tutorial: real keystrokes on
//! the test platform, the app's `SwitchBackend` in place, and a recording
//! backend standing in for the user's real blyg.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use blyg_core::config::MemoryTokenStore;
use blyg_core::*;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};

use super::FlowStep;
use super::steps::{self, Region, STEPS};
use crate::app::{MainView, Sheet};
use crate::connection::{Connection, Mode, SwitchBackend};
use crate::fake::{FakeBackend, Timing};
use crate::prefs::Prefs;

const CONNECTED: &str = "# test config\nblyg-url = https://blyg.example.com\n";

/// The user's "real" backend: a FakeBackend with one post of its own, and
/// a log of every call that reaches it.
struct Recording {
    inner: FakeBackend,
    calls: Mutex<Vec<&'static str>>,
}

impl Recording {
    fn new() -> Arc<Recording> {
        let inner = FakeBackend::with_timing(Timing::instant()).without_media_cache();
        inner
            .create_draft(Kind::Fragment, "Only on the real blyg: harbour gulls")
            .unwrap();
        Arc::new(Recording {
            inner,
            calls: Mutex::new(vec![]),
        })
    }
    fn rec(&self, name: &'static str) {
        self.calls.lock().unwrap().push(name);
    }
    fn take(&self) -> Vec<&'static str> {
        std::mem::take(&mut *self.calls.lock().unwrap())
    }
}

impl Backend for Recording {
    fn items(&self) -> Vec<Item> {
        self.rec("items");
        self.inner.items()
    }
    fn search(&self, q: &str) -> Vec<Item> {
        self.rec("search");
        self.inner.search(q)
    }
    fn item(&self, id: &LocalId) -> Option<Item> {
        self.rec("item");
        self.inner.item(id)
    }
    fn create_draft(&self, kind: Kind, md: &str) -> Result<LocalId> {
        self.rec("create_draft");
        self.inner.create_draft(kind, md)
    }
    fn save(&self, id: &LocalId, md: &str) -> Result<()> {
        self.rec("save");
        self.inner.save(id, md)
    }
    fn set_kind(&self, id: &LocalId, kind: Kind) -> Result<()> {
        self.rec("set_kind");
        self.inner.set_kind(id, kind)
    }
    fn save_with_provenance(
        &self,
        id: &LocalId,
        md: &str,
        scopes: &[Option<ScopeProvenance>],
    ) -> Result<()> {
        self.rec("save_with_provenance");
        self.inner.save_with_provenance(id, md, scopes)
    }
    fn sync_status(&self) -> SyncStatus {
        self.rec("sync_status");
        self.inner.sync_status()
    }
    fn base_url(&self) -> Option<String> {
        self.rec("base_url");
        self.inner.base_url()
    }
    fn set_event_sink(&self, sink: Box<dyn Fn(CoreEvent) + Send + Sync>) {
        self.rec("set_event_sink");
        self.inner.set_event_sink(sink)
    }
    fn resolve_conflict(&self, id: &LocalId, how: Resolution) -> Result<()> {
        self.rec("resolve_conflict");
        self.inner.resolve_conflict(id, how)
    }
    fn reading(&self) -> Vec<ReadingItem> {
        self.rec("reading");
        self.inner.reading()
    }
    fn subscriptions(&self) -> Vec<Subscription> {
        self.rec("subscriptions");
        self.inner.subscriptions()
    }
    fn create_scratch(&self, kind: Kind, md: &str) -> Result<LocalId> {
        self.rec("create_scratch");
        self.inner.create_scratch(kind, md)
    }
    fn promote(&self, id: &LocalId, to: Promote) -> Result<Promoted> {
        self.rec("promote");
        self.inner.promote(id, to)
    }
    fn publish(&self, id: &LocalId, note: Option<&str>) -> Result<PublishOutcome> {
        self.rec("publish");
        self.inner.publish(id, note)
    }
    fn withdraw(&self, id: &LocalId, note: Option<&str>) -> Result<u32> {
        self.rec("withdraw");
        self.inner.withdraw(id, note)
    }
    fn pin(&self, id: &LocalId, v: u32) -> Result<()> {
        self.rec("pin");
        self.inner.pin(id, v)
    }
    fn versions(&self, id: &LocalId) -> Result<Vec<Version>> {
        self.rec("versions");
        self.inner.versions(id)
    }
    fn restore(&self, id: &LocalId, v: u32) -> Result<()> {
        self.rec("restore");
        self.inner.restore(id, v)
    }
    fn delete_draft(&self, id: &LocalId) -> Result<()> {
        self.rec("delete_draft");
        self.inner.delete_draft(id)
    }
    fn upload_media(
        &self,
        bytes: Vec<u8>,
        mime: &str,
        item: Option<&LocalId>,
        alt: Option<&str>,
    ) -> Result<MediaRef> {
        self.rec("upload_media");
        self.inner.upload_media(bytes, mime, item, alt)
    }
    fn fork(&self, of: &RemoteRef) -> Result<LocalId> {
        self.rec("fork");
        self.inner.fork(of)
    }
    fn set_responses(&self, id: &LocalId, mode: ResponsesMode) -> Result<bool> {
        self.rec("set_responses");
        self.inner.set_responses(id, mode)
    }
    fn set_highlight(&self, id: &LocalId, mode: HighlightMode) -> Result<bool> {
        self.rec("set_highlight");
        self.inner.set_highlight(id, mode)
    }
    fn sync_now(&self) -> Result<()> {
        self.rec("sync_now");
        self.inner.sync_now()
    }
    fn preview_subscription(&self, url: &str) -> Result<SubscribePreview> {
        self.rec("preview_subscription");
        self.inner.preview_subscription(url)
    }
    fn subscribe(&self, url: &str, title: Option<&str>) -> Result<Subscription> {
        self.rec("subscribe");
        self.inner.subscribe(url, title)
    }
    fn unsubscribe(&self, sub: &str) -> Result<()> {
        self.rec("unsubscribe");
        self.inner.unsubscribe(sub)
    }
    fn set_subscription(&self, sub: &str, b: Option<bool>, t: Option<&str>) -> Result<()> {
        self.rec("set_subscription");
        self.inner.set_subscription(sub, b, t)
    }
    fn pause_subscription(&self, sub: &str, paused: bool) -> Result<()> {
        self.rec("pause_subscription");
        self.inner.pause_subscription(sub, paused)
    }
    fn signal(&self, sub: &str, id: &str, thumb: Option<i8>) -> Result<()> {
        self.rec("signal");
        self.inner.signal(sub, id, thumb)
    }
    fn mentions(&self) -> Result<Vec<Mention>> {
        self.rec("mentions");
        self.inner.mentions()
    }
    fn set_mention_hidden(&self, id: &str, hidden: bool) -> Result<()> {
        self.rec("set_mention_hidden");
        self.inner.set_mention_hidden(id, hidden)
    }
    fn settings(&self) -> Result<Settings> {
        self.rec("settings");
        self.inner.settings()
    }
    fn save_settings(&self, s: &Settings) -> Result<()> {
        self.rec("save_settings");
        self.inner.save_settings(s)
    }
    fn read_extensions_available(&self) -> bool {
        self.rec("read_extensions_available");
        self.inner.read_extensions_available()
    }
    fn provenance_available(&self) -> bool {
        self.rec("provenance_available");
        self.inner.provenance_available()
    }
    fn tracked_tk_provenance(
        &self,
        id: &LocalId,
    ) -> Option<(String, Vec<Option<ScopeProvenance>>)> {
        self.rec("tracked_tk_provenance");
        self.inner.tracked_tk_provenance(id)
    }
}

struct Env<'a> {
    view: Entity<MainView>,
    real: Arc<Recording>,
    switch: Arc<SwitchBackend>,
    dir: tempfile::TempDir,
    cx: &'a mut VisualTestContext,
}

/// The window as main.rs builds it: a `SwitchBackend` over the "real"
/// backend in the `Connection` global, with a data dir for `state.json`.
fn setup<'a>(cx: &'a mut TestAppContext, config: &str, onboarded: bool) -> Env<'a> {
    setup_mode(cx, config, onboarded, Mode::Fake)
}

fn setup_mode<'a>(
    cx: &'a mut TestAppContext,
    config: &str,
    onboarded: bool,
    mode: Mode,
) -> Env<'a> {
    // This version already seen: no what's-new card.
    let seen = onboarded.then_some(env!("CARGO_PKG_VERSION"));
    setup_full(cx, config, seen, mode, false)
}

/// The tour with its sample extensions running (the bundled ones, as this
/// test binary's children; see `extensions::test_launch`).
fn setup_ext<'a>(cx: &'a mut TestAppContext) -> Env<'a> {
    setup_full(
        cx,
        CONNECTED,
        Some(env!("CARGO_PKG_VERSION")),
        Mode::Fake,
        true,
    )
}

/// `seen`: onboarded, having last launched that version.
fn setup_full<'a>(
    cx: &'a mut TestAppContext,
    config: &str,
    seen: Option<&str>,
    mode: Mode,
    extensions: bool,
) -> Env<'a> {
    let dir = tempfile::tempdir().unwrap();
    if let Some(seen) = seen {
        state::AppState::update(dir.path(), |s| {
            s.onboarded = true;
            s.seen_version = Some(seen.into());
        })
        .unwrap();
    }
    if extensions {
        let launch = crate::app::extensions::test_launch(dir.path().join("ext-data"));
        cx.update(|cx| cx.set_global(launch));
    }
    let real = Recording::new();
    let switch = SwitchBackend::new(real.clone(), mode);
    let config = config.to_string();
    let prefs = Prefs::from_config(ConfigStore::in_memory(&config).config());
    let data_dir = dir.path().to_path_buf();
    let sw = switch.clone();
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::app::bind_keys(cx);
        crate::settings::init(
            ConfigStore::in_memory(&config),
            Arc::new(MemoryTokenStore::default()),
            None,
            cx,
        );
        cx.set_global(Connection {
            switch: sw,
            data_dir,
            verify: |_, _| Ok(1),
        });
    });
    let backend: Arc<dyn Backend> = switch.clone();
    let (view, cx) = cx.add_window_view(move |window, cx| {
        MainView::new(backend, None, prefs, std::time::Instant::now(), window, cx)
    });
    cx.run_until_parked();
    Env {
        view,
        real,
        switch,
        dir,
        cx,
    }
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

fn flow_step(e: &mut Env) -> Option<FlowStep> {
    e.view
        .read_with(e.cx, |v, _| v.onboarding.flow.as_ref().map(|f| f.step))
}

fn tour_step(e: &mut Env) -> Option<&'static str> {
    e.view.read_with(e.cx, |v, _| {
        v.onboarding.tutorial.as_ref().map(|t| STEPS[t.step].id)
    })
}

/// Let a step's ✓ pause run out.
fn settle(e: &mut Env) {
    e.cx.executor().advance_clock(Duration::from_secs(4));
    e.cx.run_until_parked();
}

fn real_is_back(e: &Env) -> bool {
    let cur = e.switch.current();
    std::ptr::addr_eq(Arc::as_ptr(&cur), Arc::as_ptr(&e.real))
}

/// Run the window until `done`: the tour's extensions answer from other
/// processes, in real time.
fn wait(e: &mut Env, what: &str, mut done: impl FnMut(&mut Env) -> bool) {
    let t = std::time::Instant::now();
    loop {
        e.cx.run_until_parked();
        e.view
            .update_in(e.cx, |v, window, cx| v.ext_pump(window, cx));
        e.cx.update(|window, _| window.refresh());
        e.cx.run_until_parked();
        if done(e) {
            return;
        }
        assert!(
            t.elapsed() < Duration::from_secs(30),
            "timed out waiting for {what}"
        );
        std::thread::sleep(Duration::from_millis(15));
    }
}

/// The tour's sample extensions are up: both notes folders listed and the
/// reading slots running.
fn tour_extensions_ready(e: &mut Env) {
    wait(e, "the tour's sample extensions", |e| {
        e.view.read_with(e.cx, |v, _| {
            let slots = v
                .ext
                .host
                .as_ref()
                .map(|h| h.entry_slots().len())
                .unwrap_or(0);
            v.ext_tour_on() && v.ext_libraries().len() >= 2 && slots >= 2
        })
    });
}

fn in_reader(e: &mut Env) -> bool {
    e.view.read_with(e.cx, |v, _| {
        v.reading.mode == crate::app::reading::stream_vm::ReadMode::Reader && v.reading.sources_open
    })
}

fn drawn(e: &mut Env, sel: &'static str) -> bool {
    e.cx.debug_bounds(sel).is_some()
}

fn wait_drawn(e: &mut Env, step: &str, sel: &'static str) {
    wait(e, &format!("{step}: {sel} on screen"), |e| drawn(e, sel));
}

#[gpui_kit::test]
fn first_launch_without_a_blyg_shows_onboarding(cx: &mut TestAppContext) {
    let mut e = setup(cx, "# fresh\n", false);
    assert_eq!(flow_step(&mut e), Some(FlowStep::Welcome));
    assert!(
        e.view.read_with(e.cx, |v, _| v.sheet.is_none()),
        "the Connect sheet waits for its step"
    );
    // ⏎ goes on to "Connect your blyg"; ⏎ there opens the existing sheet.
    e.cx.simulate_keystrokes("enter");
    e.cx.run_until_parked();
    assert_eq!(flow_step(&mut e), Some(FlowStep::Connect));
    e.cx.simulate_keystrokes("enter");
    e.cx.run_until_parked();
    assert!(
        e.view
            .read_with(e.cx, |v, _| matches!(v.sheet, Some(Sheet::Connect { .. })))
    );
    // esc on the sheet comes back to the step.
    e.cx.simulate_keystrokes("escape");
    e.cx.run_until_parked();
    assert_eq!(flow_step(&mut e), Some(FlowStep::Connect));
    assert!(e.view.read_with(e.cx, |v, _| v.sheet.is_none()));
}

#[gpui_kit::test]
fn connected_but_never_onboarded_skips_onboarding(cx: &mut TestAppContext) {
    // Set up before onboarding existed: don't make them walk through it.
    let mut e = setup(cx, CONNECTED, false);
    assert_eq!(flow_step(&mut e), None);
}

#[gpui_kit::test]
fn onboarded_and_connected_launches_straight_in(cx: &mut TestAppContext) {
    let mut e = setup(cx, CONNECTED, true);
    assert_eq!(flow_step(&mut e), None);
    assert_eq!(tour_step(&mut e), None);
}

#[gpui_kit::test]
fn skip_lands_in_the_app(cx: &mut TestAppContext) {
    let mut e = setup_mode(cx, "# fresh\n", false, Mode::Disconnected);
    assert_eq!(flow_step(&mut e), Some(FlowStep::Welcome));
    e.cx.simulate_keystrokes("escape");
    e.cx.run_until_parked();
    assert_eq!(flow_step(&mut e), None);
    assert_eq!(tour_step(&mut e), None);
    assert!(state::AppState::load(e.dir.path()).onboarded, "remembered");
    assert!(
        !config_text(e.cx).contains("onboard"),
        "app state, not config"
    );
    // The omnibar has the keyboard: typing filters.
    e.cx.simulate_input("harbour");
    e.cx.run_until_parked();
    // (the seed's Harbour Road post and the real backend's own)
    assert_eq!(e.view.read_with(e.cx, |v, _| v.list.results().len()), 2);
}

#[gpui_kit::test]
fn sample_data_runs_the_session_on_the_fake(cx: &mut TestAppContext) {
    let mut e = setup_mode(cx, "# fresh\n", false, Mode::Disconnected);
    e.cx.simulate_keystrokes("enter");
    e.cx.simulate_keystrokes("s");
    e.cx.run_until_parked();
    assert_eq!(flow_step(&mut e), Some(FlowStep::Ai));
    assert!(!real_is_back(&e), "the switch holds sample data");
    assert_eq!(e.switch.mode(), Mode::Fake);
    assert!(e.view.read_with(e.cx, |v, _| v.list.results().len() >= 6));
    assert!(!config_text(e.cx).contains("blyg-url"));
}

#[gpui_kit::test]
fn buttons_or_keyboard_writes_the_config(cx: &mut TestAppContext) {
    let mut e = setup(cx, CONNECTED, true);
    // Straight to the step (as "Show onboarding again" would get there).
    e.view.update_in(e.cx, |v, window, cx| {
        v.open_onboarding(FlowStep::Buttons, window, cx)
    });
    e.cx.run_until_parked();
    assert_eq!(flow_step(&mut e), Some(FlowStep::Buttons));
    e.cx.simulate_keystrokes("2");
    e.cx.run_until_parked();
    assert!(
        config_text(e.cx).contains("show-buttons = false"),
        "{}",
        config_text(e.cx)
    );
    assert_eq!(flow_step(&mut e), Some(FlowStep::Tutorial));
    // Back, and choose buttons: the same line is rewritten in place.
    e.cx.simulate_keystrokes("left 1");
    e.cx.run_until_parked();
    let text = config_text(e.cx);
    assert!(text.contains("show-buttons = true"), "{text}");
    assert_eq!(text.matches("show-buttons").count(), 1, "{text}");
    // ⏎ on the last step starts the tour.
    e.cx.simulate_keystrokes("enter");
    e.cx.run_until_parked();
    assert_eq!(tour_step(&mut e), Some("search"));
    assert!(state::AppState::load(e.dir.path()).onboarded);
}

#[gpui_kit::test]
fn tutorial_steps_advance_on_their_key_only(cx: &mut TestAppContext) {
    let mut e = setup(cx, CONNECTED, true);
    e.cx.dispatch_action(super::ShowTutorial);
    e.cx.run_until_parked();
    assert_eq!(tour_step(&mut e), Some("search"));

    // ⌘T isn't this step's key.
    e.cx.simulate_keystrokes(&crate::keymap::keys("cmd-t"));
    settle(&mut e);
    assert_eq!(tour_step(&mut e), Some("search"));
    // ↓ is.
    e.cx.simulate_keystrokes("down");
    e.cx.run_until_parked();
    assert!(
        e.view
            .read_with(e.cx, |v, _| v.onboarding.tutorial.as_ref().unwrap().done)
    );
    settle(&mut e);
    assert_eq!(tour_step(&mut e), Some("create"));

    // ⏎ with no match creates the seeded draft (in the sample data).
    e.cx.simulate_keystrokes(&crate::keymap::keys("cmd-t"));
    settle(&mut e);
    assert_eq!(tour_step(&mut e), Some("create"));
    e.cx.simulate_keystrokes("enter");
    e.cx.run_until_parked();
    let (cur, fake) = e
        .view
        .read_with(e.cx, |v, _| (v.current.clone(), v.fake.clone()));
    assert!(cur.unwrap().content_md.starts_with("tide pools"));
    settle(&mut e);
    assert_eq!(tour_step(&mut e), Some("new"));

    // ⌘N opens the empty editor; the step stays for a look, then Next.
    e.cx.simulate_keystrokes(&crate::keymap::keys("cmd-n"));
    settle(&mut e);
    assert!(tour_done(&mut e));
    assert_eq!(tour_step(&mut e), Some("new"), "it stays");
    e.cx.simulate_keystrokes(&crate::keymap::keys("alt-cmd-right"));
    settle(&mut e);
    assert_eq!(tour_step(&mut e), Some("autosave"));

    // Typing advances "Just write"; ↓ doesn't.
    e.cx.simulate_keystrokes(&crate::keymap::keys("cmd-l down"));
    settle(&mut e);
    assert_eq!(tour_step(&mut e), Some("autosave"));
    let autosave = STEPS.iter().position(|s| s.id == "autosave").unwrap();
    e.view.update_in(e.cx, |v, window, cx| {
        v.tutorial_enter(autosave, window, cx);
    });
    e.cx.simulate_input(" at low tide");
    settle(&mut e);
    assert_eq!(tour_step(&mut e), Some("kinds"));
    e.cx.simulate_keystrokes(&crate::keymap::keys("cmd-t"));
    settle(&mut e);
    assert_eq!(tour_step(&mut e), Some("capture"));
    // The scratch note is in the tutorial's fake; ⌘D makes it a draft.
    e.cx.simulate_keystrokes(&crate::keymap::keys("cmd-d"));
    settle(&mut e);
    assert_eq!(tour_step(&mut e), Some("publish"));
    let fake = fake.expect("the tutorial's fake");
    assert!(
        fake.items()
            .iter()
            .any(|i| i.content_md.contains("quick capture") && i.status == Status::Draft)
    );

    // Next and Back work at any time.
    e.cx.simulate_keystrokes(&crate::keymap::keys("alt-cmd-right"));
    e.cx.run_until_parked();
    assert_eq!(tour_step(&mut e), Some("publish-note"));
    e.cx.simulate_keystrokes(&crate::keymap::keys("alt-cmd-left"));
    e.cx.run_until_parked();
    assert_eq!(tour_step(&mut e), Some("publish"));
}

#[gpui_kit::test]
fn tutorial_ring_never_draws_over_a_picker(cx: &mut TestAppContext) {
    let mut e = setup(cx, CONNECTED, true);
    e.cx.dispatch_action(super::ShowTutorial);
    e.cx.run_until_parked();
    let quotes = STEPS.iter().position(|s| s.id == "quotes").unwrap();
    e.view.update_in(e.cx, |v, window, cx| {
        v.tutorial_enter(quotes, window, cx);
    });
    settle(&mut e);
    let region = STEPS[quotes].region;
    let ring = |e: &mut Env| {
        e.view
            .update_in(e.cx, |v, window, cx| v.tutorial_ring(region, window, cx))
    };
    assert!(ring(&mut e).is_some(), "the editor is ringed before ⌘K");
    e.cx.simulate_keystrokes(&crate::keymap::keys("cmd-k"));
    e.cx.run_until_parked();
    assert!(e.view.read_with(e.cx, |v, _| v.reading.sheet.is_some()));
    assert_eq!(ring(&mut e), None, "the picker covers the editor");
}

fn enter(e: &mut Env, id: &str) -> usize {
    let i = STEPS
        .iter()
        .position(|s| s.id == id)
        .unwrap_or_else(|| panic!("no step {id}"));
    e.view
        .update_in(e.cx, |v, window, cx| v.tutorial_enter(i, window, cx));
    e.cx.run_until_parked();
    i
}

fn ring_now(e: &mut Env, region: Region) -> Option<(f32, f32, f32, f32)> {
    e.view
        .update_in(e.cx, |v, window, cx| v.tutorial_ring(region, window, cx))
}

fn tour_done(e: &mut Env) -> bool {
    e.view.read_with(e.cx, |v, _| {
        v.onboarding.tutorial.as_ref().is_some_and(|t| t.done)
    })
}

/// Do what step `id` asks, the way the user would (keys where there are
/// keys; the click's own call for a click).
fn do_step(e: &mut Env, id: &str) {
    match id {
        "search" => e.cx.simulate_keystrokes("down"),
        "create" => e.cx.simulate_keystrokes("enter"),
        "autosave" => e.cx.simulate_input(" at dusk"),
        "kinds" => e.cx.simulate_keystrokes(&crate::keymap::keys("cmd-t")),
        "capture" => e.cx.simulate_keystrokes(&crate::keymap::keys("cmd-d")),
        "publish" => e.cx.simulate_keystrokes(&crate::keymap::keys("cmd-enter")),
        "publish-note" => e.cx.simulate_keystrokes("enter"),
        "full-editor" => e.cx.simulate_keystrokes(&crate::keymap::keys("cmd-3")),
        "write-view" => e.cx.simulate_keystrokes(&crate::keymap::keys("cmd-1")),
        "tk" => e.cx.simulate_keystrokes(&crate::keymap::keys("cmd-g")),
        "shorten" => {
            e.cx.simulate_keystrokes(&crate::keymap::keys("cmd-shift-g"))
        }
        "versions" => e.cx.simulate_keystrokes(&crate::keymap::keys("cmd-y")),
        "quotes" => e.cx.simulate_keystrokes(&crate::keymap::keys("cmd-k")),
        "mentions" => e.cx.simulate_input("@"),
        "stream" => e.cx.simulate_keystrokes("enter"),
        "original" => e.view.update_in(e.cx, |v, window, cx| {
            let origin = v
                .reading
                .rows
                .iter()
                .find(|r| r.remote_id == steps::QUOTED)
                .map(|r| r.origin.clone())
                .expect("the quoted post is held");
            v.open_original(origin, steps::QUOTED.into(), Some(1), window, cx)
        }),
        "lineage" => e.cx.simulate_keystrokes(&crate::keymap::keys("cmd-j")),
        "ring" => e.cx.simulate_keystrokes("space"),
        "reader" => e.cx.simulate_keystrokes(&crate::keymap::keys("alt-cmd-2")),
        "notes" => {
            e.cx.simulate_keystrokes(&crate::keymap::keys("cmd-shift-n"))
        }
        "browser" => e.cx.simulate_keystrokes("escape"),
        "new" => e.cx.simulate_keystrokes(&crate::keymap::keys("cmd-n")),
        // A click on ⋯ (the chip's own call).
        "slots" => e.view.update_in(e.cx, |v, window, cx| {
            let item = v
                .reading
                .sel
                .as_ref()
                .and_then(|k| {
                    v.reading
                        .rows
                        .iter()
                        .find(|r| r.subscription_id == k.0 && r.remote_id == k.1)
                })
                .cloned()
                .expect("a post is selected");
            v.slots_open(item, window, cx)
        }),
        // A click on the second folder's chip.
        "notes-library" => e.view.update_in(e.cx, |v, window, cx| {
            let second = v.ext_libraries().into_iter().nth(1).expect("two folders");
            v.ext_lib_choose(Some(second.library.id), window, cx)
        }),
        "clip" => {
            e.cx.simulate_keystrokes(&crate::keymap::keys("cmd-shift-c"))
        }
        "cross-post" => e.cx.simulate_keystrokes("escape"),
        "extensions" => {
            e.cx.simulate_keystrokes(&crate::keymap::keys("cmd-shift-p"))
        }
        // Manage extensions… is the palette's last row.
        "manage" => e.cx.simulate_keystrokes("up enter"),
        "consent" => {
            e.cx.simulate_keystrokes(&crate::keymap::keys("cmd-backspace"))
        }
        other => panic!("no way to do step {other}"),
    }
    e.cx.run_until_parked();
}

#[gpui_kit::test]
fn every_step_advances_on_its_own_key(cx: &mut TestAppContext) {
    let mut e = setup_ext(cx);
    e.cx.dispatch_action(super::ShowTutorial);
    e.cx.run_until_parked();
    tour_extensions_ready(&mut e);
    e.real.take();
    for (i, step) in STEPS.iter().enumerate() {
        if step.keys.is_empty() {
            continue; // the last step waits for Finish
        }
        enter(&mut e, step.id);
        assert!(!tour_done(&mut e), "{}: done before its key", step.id);
        do_step(&mut e, step.id);
        assert!(tour_done(&mut e), "{}: its key didn't count", step.id);
        // Leave what the key opened (a proposal, the picker) so it moves on.
        settle(&mut e);
        if step.stay {
            // It opened something to look at: it waits for Next.
            assert_eq!(tour_step(&mut e), Some(step.id), "{} stays", step.id);
            e.cx.simulate_keystrokes(&crate::keymap::keys("alt-cmd-right"));
            settle(&mut e);
        } else if tour_step(&mut e) == Some(step.id) {
            e.cx.simulate_keystrokes("escape");
            settle(&mut e);
        }
        assert_eq!(
            tour_step(&mut e),
            Some(STEPS[i + 1].id),
            "{} moves on after its key",
            step.id
        );
    }
    assert_eq!(e.real.take(), Vec::<&str>::new(), "all on sample data");
}

#[gpui_kit::test]
fn every_step_rings_something_on_screen(cx: &mut TestAppContext) {
    let mut e = setup_ext(cx);
    e.cx.dispatch_action(super::ShowTutorial);
    e.cx.run_until_parked();
    tour_extensions_ready(&mut e);
    let (vw, vh) = e.cx.update(|window, _| {
        let s = window.viewport_size();
        (f32::from(s.width), f32::from(s.height))
    });
    for step in STEPS {
        enter(&mut e, step.id);
        settle(&mut e);
        let ring = ring_now(&mut e, step.region);
        if step.region == Region::Whole {
            assert_eq!(ring, None, "{}: no ring", step.id);
            continue;
        }
        let (x, y, w, h) =
            ring.unwrap_or_else(|| panic!("{}: no ring on {:?}", step.id, step.region));
        assert!(w > 20. && h > 20., "{}: {w}×{h}", step.id);
        assert!(
            x >= -1. && y >= -1. && x + w <= vw + 1. && y + h <= vh + 1.,
            "{}: ({x}, {y}, {w}, {h}) is off the {vw}×{vh} window",
            step.id
        );
        // What the caption points at is inside the ring.
        let inside: &[&str] = match step.region {
            // The toggle in the Stream; in Reader mode, the sources pane.
            Region::Reader if !in_reader(&mut e) => &["mode-stream", "mode-reader"],
            Region::StreamPost => &["stream-actions"],
            Region::ReaderPost => &["responses"],
            Region::BrowserChrome => &["browser-shield", "browser-clip", "browser-close"],
            Region::MacroRun => &["browser-macro-stop", "browser-close"],
            Region::NotesDrawer => &["notes-tab-library", "lib-vault-0", "lib-vault-add"],
            _ => &[],
        };
        // The drawer's folders come from the extension: wait for them.
        if step.region == Region::NotesDrawer {
            wait_drawn(&mut e, step.id, "lib-vault-1");
        }
        for sel in inside {
            let b =
                e.cx.debug_bounds(sel)
                    .unwrap_or_else(|| panic!("{}: no {sel} on screen", step.id));
            let (bx, by) = (f32::from(b.left()), f32::from(b.top()));
            let (bx1, by1) = (f32::from(b.right()), f32::from(b.bottom()));
            // The browser pane may still be sliding in from the right (the
            // ring is where it lands): only its left and vertical edges.
            let sliding = matches!(step.region, Region::BrowserChrome | Region::MacroRun);
            assert!(
                bx >= x - 1. && by >= y - 1. && (sliding || bx1 <= x + w + 1.) && by1 <= y + h + 1.,
                "{}: {sel} ({bx}, {by})–({bx1}, {by1}) is outside the ring ({x}, {y}, {w}, {h})",
                step.id
            );
        }
    }
}

#[gpui_kit::test]
fn the_ring_steps_aside_for_the_drawer_the_browser_and_popups(cx: &mut TestAppContext) {
    let mut e = setup(cx, CONNECTED, true);
    e.cx.dispatch_action(super::ShowTutorial);
    e.cx.run_until_parked();

    // The notes drawer slides over the post pane the notes step rings.
    enter(&mut e, "notes");
    assert!(ring_now(&mut e, Region::ReaderPost).is_some());
    e.cx.simulate_keystrokes(&crate::keymap::keys("cmd-shift-n"));
    e.cx.run_until_parked();
    assert!(e.view.read_with(e.cx, |v, _| v.notes.open));
    assert_eq!(
        ring_now(&mut e, Region::ReaderPost),
        None,
        "under the drawer"
    );

    // The browser pane covers the Reader, but its own step rings it.
    enter(&mut e, "browser");
    assert!(e.view.read_with(e.cx, |v, _| v.browser.open));
    assert!(ring_now(&mut e, Region::BrowserChrome).is_some());
    assert_eq!(ring_now(&mut e, Region::ReaderPost), None, "under the pane");
    assert!(
        ring_now(&mut e, Region::Reader).is_some(),
        "the sources pane is left of it"
    );
    // Nothing was loaded for the sample.
    e.view.read_with(e.cx, |v, _| {
        assert!(v.browser.loads.is_empty(), "{:?}", v.browser.loads);
        assert!(!v.browser.alive(), "no web view for the sample");
    });

    // The stream's side pane: the post list's ring shrinks beside it.
    enter(&mut e, "stream");
    assert!(e.view.read_with(e.cx, |v, _| !v.browser.open), "tidied");
    let full = ring_now(&mut e, Region::Stream).expect("the stream");
    e.cx.simulate_keystrokes("enter");
    e.cx.run_until_parked();
    let beside = ring_now(&mut e, Region::Stream).expect("beside the pane");
    assert!(beside.2 < full.2 * 0.6, "{beside:?} vs {full:?}");

    // The @-mention popup sits in the editor: no ring over it.
    enter(&mut e, "mentions");
    assert!(ring_now(&mut e, Region::Editor).is_some());
    e.cx.simulate_input("@");
    e.cx.run_until_parked();
    assert!(
        e.view
            .read_with(e.cx, |v, cx| v.assist.read(cx).mention_open())
    );
    assert_eq!(ring_now(&mut e, Region::Editor), None, "the popup is up");
}

#[gpui_kit::test]
fn the_tour_puts_reading_mode_notes_and_browser_back(cx: &mut TestAppContext) {
    let mut e = setup(cx, CONNECTED, true);
    // The user reads in Reader mode.
    e.cx.simulate_keystrokes(&crate::keymap::keys("alt-cmd-2"));
    e.cx.run_until_parked();
    assert_eq!(
        state::AppState::load(e.dir.path()).reading_mode.as_deref(),
        Some("reader")
    );
    e.cx.dispatch_action(super::ShowTutorial);
    e.cx.run_until_parked();
    enter(&mut e, "stream");
    assert_eq!(
        state::AppState::load(e.dir.path()).reading_mode.as_deref(),
        Some("stream")
    );
    enter(&mut e, "notes");
    e.cx.simulate_keystrokes(&crate::keymap::keys("cmd-shift-n"));
    e.cx.simulate_input("a note in the tour");
    e.cx.run_until_parked();
    enter(&mut e, "browser");
    e.view
        .update_in(e.cx, |v, window, cx| v.finish_tutorial(window, cx));
    e.cx.run_until_parked();
    assert_eq!(
        state::AppState::load(e.dir.path()).reading_mode.as_deref(),
        Some("reader"),
        "the user's mode"
    );
    assert_eq!(
        state::AppState::load(e.dir.path()).notes_note,
        None,
        "the tour's note isn't remembered"
    );
    e.view.read_with(e.cx, |v, _| {
        assert!(!v.notes.open && !v.browser.open);
        assert!(v.browser.page.url.is_empty(), "no sample page left for ⇧⌘B");
        assert_eq!(
            v.reading.mode,
            crate::app::reading::stream_vm::ReadMode::Reader
        );
    });
}

#[gpui_kit::test]
fn tutorial_on_launch_shows_it_on_start(cx: &mut TestAppContext) {
    let mut e = setup(cx, &format!("{CONNECTED}tutorial-on-launch = true\n"), true);
    assert_eq!(tour_step(&mut e), Some("search"));
    assert_eq!(flow_step(&mut e), None);
    // The checkbox writes the key back.
    e.view
        .update(e.cx, |v, cx| v.set_tutorial_on_launch(false, cx));
    assert!(config_text(e.cx).contains("tutorial-on-launch = false"));
}

#[gpui_kit::test]
fn finishing_restores_the_real_backend(cx: &mut TestAppContext) {
    let mut e = setup(cx, CONNECTED, true);
    e.cx.simulate_input("harb");
    e.cx.run_until_parked();
    e.cx.dispatch_action(super::ShowTutorial);
    e.cx.run_until_parked();
    assert!(!real_is_back(&e));
    // From here on nothing may reach the real backend.
    e.real.take();

    // Publish in the tour: jump to the step, ⌘⏎, a note, ⏎.
    let publish = STEPS.iter().position(|s| s.id == "publish").unwrap();
    e.view
        .update_in(e.cx, |v, window, cx| v.tutorial_enter(publish, window, cx));
    e.cx.run_until_parked();
    e.cx.simulate_keystrokes(&crate::keymap::keys("cmd-enter"));
    settle(&mut e);
    assert_eq!(tour_step(&mut e), Some("publish-note"));
    e.cx.simulate_input("first version");
    e.cx.simulate_keystrokes("enter");
    settle(&mut e);
    let fake = e.view.read_with(e.cx, |v, _| v.fake.clone()).unwrap();
    assert!(
        fake.item(&LocalId(super::steps::DRAFT.into()))
            .is_some_and(|i| i.status == Status::Public),
        "published in the sample data"
    );
    // Walk the rest of the tour with its keys and Next.
    for _ in 0..STEPS.len() {
        e.cx.simulate_keystrokes(&crate::keymap::keys("alt-cmd-right"));
        e.cx.run_until_parked();
    }
    settle(&mut e);
    assert_eq!(tour_step(&mut e), None, "finished");
    let during: Vec<_> = e.real.take();
    // Finishing re-attaches the real backend (its event sink) and reads it.
    let first_other = during.iter().position(|c| *c == "set_event_sink");
    assert!(first_other.is_some(), "{during:?}");
    assert!(real_is_back(&e));
    assert_eq!(e.switch.mode(), Mode::Fake, "the mode it had before");
    e.view.read_with(e.cx, |v, _| {
        assert_eq!(v.list.query(), "harb", "the query came back");
        assert!(
            v.list
                .results()
                .iter()
                .all(|i| i.content_md.contains("arb"))
        );
        assert!(v.fake.is_none());
    });
    // The real backend never saw the tour's publish.
    assert!(
        e.real
            .inner
            .item(&LocalId(super::steps::DRAFT.into()))
            .is_some_and(|i| i.status == Status::Draft)
    );
}

#[gpui_kit::test]
fn no_call_reaches_the_real_backend_during_the_tour(cx: &mut TestAppContext) {
    let mut e = setup(cx, CONNECTED, true);
    e.cx.dispatch_action(super::ShowTutorial);
    e.cx.run_until_parked();
    e.real.take();
    for step in 0..STEPS.len() {
        e.view
            .update_in(e.cx, |v, window, cx| v.tutorial_enter(step, window, cx));
        e.cx.run_until_parked();
    }
    e.cx.simulate_keystrokes(&crate::keymap::keys("cmd-3 cmd-1 cmd-r cmd-r cmd-y"));
    settle(&mut e);
    assert_eq!(e.real.take(), Vec::<&str>::new());
    e.view
        .update_in(e.cx, |v, window, cx| v.finish_tutorial(window, cx));
    e.cx.run_until_parked();
    assert!(real_is_back(&e));
    assert!(
        !e.real.take().is_empty(),
        "the window reads the real backend again"
    );
}

#[gpui_kit::test]
fn replay_from_settings(cx: &mut TestAppContext) {
    let mut e = setup(cx, CONNECTED, true);
    e.cx.simulate_keystrokes(&crate::keymap::keys("cmd-,"));
    e.cx.run_until_parked();
    assert!(
        e.view
            .read_with(e.cx, |v, _| matches!(v.sheet, Some(Sheet::Settings { .. })))
    );
    e.view
        .update_in(e.cx, |v, window, cx| v.help_replay(window, cx));
    e.cx.run_until_parked();
    assert_eq!(tour_step(&mut e), Some("search"));
    assert!(e.view.read_with(e.cx, |v, _| v.sheet.is_none()));
    // "Show onboarding again" ends the tour and opens the flow.
    e.cx.simulate_keystrokes(&crate::keymap::keys("cmd-,"));
    e.cx.run_until_parked();
    e.view
        .update_in(e.cx, |v, window, cx| v.help_onboarding(window, cx));
    e.cx.run_until_parked();
    assert_eq!(tour_step(&mut e), None);
    assert_eq!(flow_step(&mut e), Some(FlowStep::Welcome));
    assert!(real_is_back(&e));
}

/// ⏎ in the @ popup inserts the mention; the tour stays on the step (it
/// used to move on as soon as @ opened the popup).
#[gpui_kit::test]
fn the_mentions_step_lets_you_pick_someone(cx: &mut TestAppContext) {
    let mut e = setup(cx, CONNECTED, true);
    e.cx.dispatch_action(super::ShowTutorial);
    e.cx.run_until_parked();
    enter(&mut e, "mentions");
    e.cx.simulate_input("@");
    settle(&mut e);
    assert!(tour_done(&mut e));
    assert_eq!(tour_step(&mut e), Some("mentions"));
    assert!(
        e.view
            .read_with(e.cx, |v, cx| v.assist.read(cx).mention_open())
    );
    e.cx.simulate_keystrokes("enter");
    settle(&mut e);
    assert_eq!(tour_step(&mut e), Some("mentions"), "still here after ⏎");
    let text = e
        .view
        .read_with(e.cx, |v, cx| v.editor.read(cx).value().to_string());
    assert!(text.contains("]("), "a mention link was inserted: {text}");
}

// --- what's new --- the first launch after an update.

static NEWS: super::whats_new::Release = super::whats_new::Release {
    version: "9.9.9",
    items: &["Something new"],
    first_step: Some("lineage"),
};

fn news_up(e: &Env) -> bool {
    e.view.read_with(e.cx, |v, _| {
        v.onboarding
            .tutorial
            .as_ref()
            .is_some_and(|t| !t.news.is_empty())
    })
}

#[gpui_kit::test]
fn whats_new_opens_the_tour_with_a_way_out(cx: &mut TestAppContext) {
    let mut e = setup(cx, CONNECTED, true);
    assert!(
        e.view
            .read_with(e.cx, |v, _| v.onboarding.tutorial.is_none()),
        "seen: no card"
    );
    e.view.update_in(e.cx, |v, window, cx| {
        v.start_whats_new(vec![&NEWS], window, cx)
    });
    e.cx.run_until_parked();
    assert!(news_up(&e));
    // The step's own key does nothing while the card is up.
    e.cx.simulate_keystrokes("down");
    e.cx.run_until_parked();
    assert!(news_up(&e) && !tour_done(&mut e));
    // ⌥⌘→: what's new, straight to its first step.
    e.cx.simulate_keystrokes(&crate::keymap::keys("alt-cmd-right"));
    e.cx.run_until_parked();
    assert!(!news_up(&e));
    let step = e.view.read_with(e.cx, |v, _| {
        v.onboarding.tutorial.as_ref().map(|t| STEPS[t.step].id)
    });
    assert_eq!(step, Some("lineage"));
    // Skip: the card's other way out ends the tour.
    e.view.update_in(e.cx, |v, window, cx| {
        v.finish_tutorial(window, cx);
        v.start_whats_new(vec![&NEWS], window, cx);
    });
    e.cx.run_until_parked();
    e.cx.simulate_keystrokes("escape");
    e.cx.run_until_parked();
    assert!(
        e.view
            .read_with(e.cx, |v, _| v.onboarding.tutorial.is_none()),
        "esc skips"
    );
}

#[gpui_kit::test]
fn an_update_remembers_the_version_once_seen(cx: &mut TestAppContext) {
    let e = setup(cx, CONNECTED, true);
    assert_eq!(
        state::AppState::load(e.dir.path()).seen_version.as_deref(),
        Some(env!("CARGO_PKG_VERSION"))
    );
}

// --- 0.10.0 and 0.11.0 --- the steps that teach them, on the real UI.

/// Each new step lands on its feature: what its caption names is drawn in
/// the state the step sets up, and after its key.
#[gpui_kit::test]
fn the_new_steps_show_their_features(cx: &mut TestAppContext) {
    let mut e = setup_ext(cx);
    let before = config_text(e.cx);
    e.cx.dispatch_action(super::ShowTutorial);
    e.cx.run_until_parked();
    tour_extensions_ready(&mut e);

    // ⌘N: an empty editor, a scratch note once typed in.
    enter(&mut e, "new");
    do_step(&mut e, "new");
    assert!(e.view.read_with(e.cx, |v, _| v.new_post.is_some()));
    assert!(tour_done(&mut e));

    // reading-time's "· N min" and inspect's ⋯, then its sheet.
    enter(&mut e, "slots");
    wait_drawn(&mut e, "slots", "slot-byline-reading-time");
    assert!(drawn(&mut e, "slot-more"), "the ⋯ chip");
    do_step(&mut e, "slots");
    wait_drawn(&mut e, "slots", "slot-sheet");
    assert!(tour_done(&mut e));

    // The lineage glyph's counts, and the map's.
    enter(&mut e, "lineage");
    wait_drawn(&mut e, "lineage", "lineage-glyph-counts");
    do_step(&mut e, "lineage");
    wait_drawn(&mut e, "lineage", "lineage-count-up");
    assert!(drawn(&mut e, "lineage-count-down"));

    // The drawer's Notes tab: a chip per folder, Add folder…, Remove.
    enter(&mut e, "notes-library");
    for sel in [
        "notes-tab-library",
        "lib-vault-0",
        "lib-vault-1",
        "lib-vault-2",
        "lib-vault-add",
        "lib-vault-remove",
    ] {
        wait_drawn(&mut e, "notes-library", sel);
    }
    do_step(&mut e, "notes-library");
    assert!(tour_done(&mut e));
    // Remove from Burrow and Add folder… write nothing in the tour.
    e.view.update_in(e.cx, |v, window, cx| {
        v.ext_remove_vault("vault-garden", window, cx);
        v.ext_pick_vaults(window, cx);
    });
    e.cx.run_until_parked();

    // ✂ Clip on the sample page: a quote with its link in a draft.
    enter(&mut e, "clip");
    assert!(drawn(&mut e, "browser-clip"));
    do_step(&mut e, "clip");
    assert!(tour_done(&mut e));
    let text = e
        .view
        .read_with(e.cx, |v, cx| v.editor.read(cx).value().to_string());
    assert!(
        text.contains("blyg.example.com/tide-tables"),
        "clipped: {text}"
    );

    // cross-post's run in the pane, then folded: the status bar's mark.
    enter(&mut e, "cross-post");
    wait_drawn(&mut e, "cross-post", "browser-macro-stop");
    assert!(!drawn(&mut e, "macro-folded"));
    do_step(&mut e, "cross-post");
    wait_drawn(&mut e, "cross-post", "macro-folded");
    assert!(tour_done(&mut e));
    assert!(
        ring_now(&mut e, Region::MacroRun).is_some(),
        "the ring moves to the status bar"
    );

    // ⇧⌘P, then Manage: Turn off and Forget permissions on the sample.
    enter(&mut e, "extensions");
    do_step(&mut e, "extensions");
    wait_drawn(&mut e, "extensions", "ext-sheet");
    assert!(drawn(&mut e, "ext-pal-0"));
    enter(&mut e, "manage");
    do_step(&mut e, "manage");
    wait_drawn(&mut e, "manage", "ext-off-markdown-notes");
    assert!(drawn(&mut e, "ext-forget-markdown-notes"));
    assert!(tour_done(&mut e));
    e.view.update_in(e.cx, |v, window, cx| {
        v.ext_turn_off(crate::app::extensions::NOTES, true, window, cx)
    });
    e.cx.run_until_parked();

    // The consent sheet's ⌘⌫: the sheet goes, nothing is written.
    enter(&mut e, "consent");
    wait_drawn(&mut e, "consent", "ext-turn-off");
    do_step(&mut e, "consent");
    assert!(tour_done(&mut e));
    e.view.read_with(e.cx, |v, _| {
        assert!(v.ext.overlay.is_none());
        assert!(v.ext.notices.is_empty(), "{:?}", v.ext.notices);
    });

    // Nothing the tour did reached the config file.
    assert_eq!(config_text(e.cx), before);
}

/// Finishing the tour stops its extensions and puts the user's back.
#[gpui_kit::test]
fn the_tour_puts_the_users_extensions_back(cx: &mut TestAppContext) {
    let mut e = setup_ext(cx);
    let mine = e
        .view
        .read_with(e.cx, |v, _| v.ext.host.as_ref().map(|h| h.status().len()));
    e.cx.dispatch_action(super::ShowTutorial);
    e.cx.run_until_parked();
    tour_extensions_ready(&mut e);
    enter(&mut e, "notes-library");
    wait_drawn(&mut e, "notes-library", "lib-vault-1");
    enter(&mut e, "consent");
    e.view
        .update_in(e.cx, |v, window, cx| v.finish_tutorial(window, cx));
    e.cx.run_until_parked();
    e.view.read_with(e.cx, |v, _| {
        assert!(!v.ext_tour_on());
        assert!(v.ext.overlay.is_none(), "the sample sheet went");
        assert!(!v.ext.lib.tab);
        assert!(v.ext.notices.is_empty(), "{:?}", v.ext.notices);
        // The user's host: nothing on, as before the tour.
        assert_eq!(v.ext.host.as_ref().map(|h| h.status().len()), mine);
        assert!(v.ext_libraries().is_empty());
        assert!(!v.ext.started, "not started before the tour, nor after");
    });
    assert_eq!(
        state::AppState::load(e.dir.path()).notes_vault,
        None,
        "the sample folder isn't remembered"
    );
}

/// The first launch after updating from 0.10.0 to 0.11.0: the card lists
/// 0.11.0's news, and "Show me what's new" lands on its first step.
#[gpui_kit::test]
fn a_launch_after_updating_from_0_10_starts_at_0_11s_first_step(cx: &mut TestAppContext) {
    super::whats_new::TEST_VERSION.with(|v| *v.borrow_mut() = Some("0.11.0"));
    let mut e = setup_full(cx, CONNECTED, Some("0.10.0"), Mode::Fake, false);
    super::whats_new::TEST_VERSION.with(|v| *v.borrow_mut() = None);
    assert!(news_up(&e), "the card is up");
    let versions: Vec<_> = e.view.read_with(e.cx, |v, _| {
        v.onboarding
            .tutorial
            .as_ref()
            .unwrap()
            .news
            .iter()
            .map(|r| r.version)
            .collect()
    });
    assert_eq!(versions, ["0.11.0"]);
    assert_eq!(
        state::AppState::load(e.dir.path()).seen_version.as_deref(),
        Some("0.11.0")
    );
    // ⏎ on the card: Show me what's new.
    e.cx.simulate_keystrokes("enter");
    e.cx.run_until_parked();
    assert!(!news_up(&e));
    let first_new = STEPS.iter().find(|s| s.since == "0.11.0").map(|s| s.id);
    assert_eq!(tour_step(&mut e), first_new);
    assert_eq!(tour_step(&mut e), Some("slots"));
}
