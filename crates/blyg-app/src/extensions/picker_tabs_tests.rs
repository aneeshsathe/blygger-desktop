//! --- picker sources --- The `[[` / `![[` picker's extension tabs (studio
//! 0.71's `pickerSource`), with a real `Host` running a real extension:
//! this test binary's `picker_child`, which answers as `bxp-picker-ext`
//! does (`blyg_ext::picker::sample_answer`). The extension is installed in
//! a temporary extensions folder beside a temporary config file, as a user
//! would install one.

use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use blyg_core::config::{ConfigFiles, MemoryTokenStore};
use blyg_core::{Backend, ConfigStore, LocalId};
use blyg_ext::ext::{Extension, HostClient, serve};
use blyg_ext::picker::{PICKER_SEARCH, PickerForm, PickerSearchParams};
use blyg_ext::protocol::*;
use gpui_kit::{Entity, Modifiers, TestAppContext, VisualTestContext};

use crate::app::MainView;
use crate::composer::picker::TabRows;
use crate::fake::{FakeBackend, Timing};
use crate::prefs::Prefs;

const NAME: &str = "bxp-picker";
const DRAFT_THREAD: &str = "01J9H4C";
const A: &str = "0123456789abcdefghjkmnpqrs";

// ------------------------------------------------------------ the child

fn child_test() -> String {
    let m = module_path!();
    let m = m.split_once("::").map(|(_, rest)| rest).unwrap_or(m);
    format!("{m}::picker_child")
}

struct Picker;

impl Extension for Picker {
    fn initialize(
        &mut self,
        _host: &HostClient,
        _p: InitializeParams,
    ) -> Result<InitializeResult, RpcError> {
        Ok(InitializeResult {
            protocol_version: PROTOCOL_VERSION,
            name: NAME.into(),
            version: "0.0.1".into(),
            commands: vec![],
            sources: vec![],
            libraries: vec![],
        })
    }

    fn request(
        &mut self,
        _host: &HostClient,
        method: &str,
        p: serde_json::Value,
    ) -> Result<serde_json::Value, RpcError> {
        if method != PICKER_SEARCH {
            return Err(RpcError::method_not_found(method));
        }
        let p: PickerSearchParams = blyg_ext::rpc::params(p)?;
        blyg_ext::picker::sample_answer(&p).map_err(|e| RpcError::new(-1, e))
    }
}

/// Not a test: the picker extension's process for the tests below.
#[test]
#[ignore = "the picker-source child process of the picker tab tests"]
fn picker_child() {
    if std::env::args().any(|a| a == child_test()) {
        serve(std::io::stdin(), std::io::stdout(), &mut Picker);
    }
}

fn install(dir: &Path) {
    let exe = std::env::current_exe().expect("the test binary");
    let ext = dir.join("extensions").join(NAME);
    std::fs::create_dir_all(&ext).unwrap();
    let manifest = format!(
        r#"name = "{NAME}"
version = "0.0.1"
protocol = 1
command = ['{}', '--exact', '{}', '--ignored', '--quiet', '--test-threads=1']

[[picker-sources]]
id = "refs"
label = "references"

[[picker-sources]]
id = "glossary"
label = "glossary"
forms = ["link", "transclude"]
"#,
        exe.display(),
        child_test()
    );
    std::fs::write(ext.join(blyg_ext::MANIFEST_FILE), manifest).unwrap();
}

// ------------------------------------------------------------ the window

struct Env<'a> {
    _dir: tempfile::TempDir,
    view: Entity<MainView>,
    cx: &'a mut VisualTestContext,
}

fn setup(cx: &mut TestAppContext) -> Env<'_> {
    let dir = tempfile::tempdir().unwrap();
    install(dir.path());
    let config = dir.path().join("config");
    std::fs::write(
        &config,
        format!("blyg-url = https://blyg.example.com\nbracket-picker = on\nextension = {NAME}\n"),
    )
    .unwrap();
    let store = || ConfigStore::open(ConfigFiles::single(config.clone()));
    let prefs = Prefs::from_config(store().config());
    let launch = super::test_launch(dir.path().join("data"));
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::app::bind_keys(cx);
        crate::settings::init(store(), Arc::new(MemoryTokenStore::default()), None, cx);
        cx.set_global(launch);
    });
    let fake = Arc::new(FakeBackend::with_timing(Timing::instant()).without_media_cache());
    let backend: Arc<dyn Backend> = fake.clone();
    let (view, cx) = cx.add_window_view(move |window, cx| {
        MainView::new(backend, Some(fake), prefs, Instant::now(), window, cx)
    });
    cx.run_until_parked();
    let mut env = Env {
        _dir: dir,
        view,
        cx,
    };
    env.view
        .update_in(env.cx, |v, window, cx| v.ext_start(window, cx));
    env.wait("the extension running", |v, _| {
        v.ext
            .host
            .as_ref()
            .is_some_and(|h| h.picker_tabs(PickerForm::Link).len() == 2)
    });
    env
}

impl Env<'_> {
    /// Run the window until `done` (the extension answers from another
    /// process, in real time).
    fn wait(&mut self, what: &str, mut done: impl FnMut(&MainView, &gpui_kit::App) -> bool) {
        let view = self.view.clone();
        crate::test_wait::until(self.cx, what, |cx| {
            view.update_in(cx, |v, window, cx| v.ext_pump(window, cx));
            cx.executor()
                .advance_clock(crate::composer::picker::TAB_PAUSE);
            cx.update(|window, _| window.refresh());
            cx.run_until_parked();
            view.read_with(cx, |v, cx| done(v, cx))
        });
    }

    fn text(&mut self) -> String {
        self.view
            .read_with(self.cx, |v, cx| v.editor.read(cx).value().to_string())
    }

    fn type_text(&mut self, s: &str) {
        for c in s.chars() {
            self.cx.simulate_input(&c.to_string());
            self.cx.run_until_parked();
        }
    }

    /// Click it once it has stopped moving (the panel slides in).
    fn click(&mut self, selector: &'static str) {
        let b = crate::test_wait::still(self.cx, selector);
        self.cx.simulate_click(b.center(), Modifiers::none());
        self.cx.run_until_parked();
    }

    /// The editor open on the thread, the caret on a fresh last line.
    fn open_at_end(&mut self) -> String {
        self.view.update_in(self.cx, |v, window, cx| {
            v.open(&LocalId(DRAFT_THREAD.into()), window, cx);
            v.editor.update(cx, |s, cx| {
                s.focus(window, cx);
                let end = s.text().len();
                s.set_selected_range(end..end, cx);
            });
        });
        self.cx.run_until_parked();
        self.cx.simulate_keystrokes("enter");
        self.cx.run_until_parked();
        self.text()
    }

    fn popup_tabs(&mut self) -> Option<(Vec<String>, Option<usize>)> {
        self.view
            .read_with(self.cx, |v, cx| v.assist.read(cx).picker_tabs())
    }

    fn popup_rows(&mut self) -> Option<TabRows> {
        self.view
            .read_with(self.cx, |v, cx| v.assist.read(cx).picker_tab_rows())
    }
}

/// Typed `[[`: the tab row heads the popup, "items" first. A click opens
/// the extension's tab; what's typed after the brackets is its query; ⏎
/// writes the result as a labelled link. The tab is remembered: the next
/// `[[` opens on it, and "items" goes back to Burrow's own search.
#[gpui_kit::test]
fn the_link_popup_has_the_extension_tabs(cx: &mut TestAppContext) {
    let mut env = setup(cx);
    let before = env.open_at_end();
    env.type_text("See [[");
    assert_eq!(
        env.popup_tabs(),
        Some((vec!["references".into(), "glossary".into()], None)),
        "both tabs offer links; Burrow's own search is open"
    );
    env.click("pick-tab-0");
    assert_eq!(env.popup_tabs().unwrap().1, Some(0));
    env.type_text("polanyi");
    env.wait("the extension's results", |v, cx| {
        v.assist
            .read(cx)
            .picker_tab_rows()
            .is_some_and(|r| matches!(r, TabRows::Found(_)))
    });
    let rows = env.popup_rows().unwrap();
    assert_eq!(rows.status(), "2 results");
    assert_eq!(
        rows.rows()[0].meta.as_deref(),
        Some("refs · link · polanyi"),
        "the query and the form reached the extension"
    );
    env.cx.simulate_keystrokes("enter");
    env.cx.run_until_parked();
    assert_eq!(env.text(), format!("{before}See [[{A} | Polanyi 1966]]"));

    // Remembered: the next link opens on the extension's tab.
    env.type_text(" and [[");
    assert_eq!(env.popup_tabs().unwrap().1, Some(0));
    env.click("pick-tab-items");
    assert_eq!(env.popup_tabs().unwrap().1, None, "back to items");
}

/// An extension's error shows in the popup's line, named; nothing is
/// inserted, and esc leaves the typed text.
#[gpui_kit::test]
fn an_extension_error_is_said_in_the_popup(cx: &mut TestAppContext) {
    let mut env = setup(cx);
    let before = env.open_at_end();
    env.type_text("[[");
    env.click("pick-tab-1");
    env.type_text("broken");
    env.wait("the extension's error", |v, cx| {
        v.assist
            .read(cx)
            .picker_tab_rows()
            .is_some_and(|r| matches!(r, TabRows::Failed(_)))
    });
    let line = env.popup_rows().unwrap().status();
    assert!(line.starts_with("Couldn't search:"), "{line}");
    assert!(line.contains("the index is missing"), "{line}");
    env.cx.simulate_keystrokes("escape");
    env.cx.run_until_parked();
    assert_eq!(env.text(), format!("{before}[[broken"));
}

/// A line-leading `![[` is a quote: only the tab offered for quotes shows.
#[gpui_kit::test]
fn a_quote_offers_only_the_tabs_for_quotes(cx: &mut TestAppContext) {
    let mut env = setup(cx);
    env.open_at_end();
    env.type_text("![[");
    assert_eq!(
        env.popup_tabs(),
        Some((vec!["glossary".into()], None)),
        "references offers links only"
    );
}

/// ⌘K's panel (a quote, its own search box) has the tabs too: the one for
/// quotes, searched as the box is typed in, ⏎ writing the quote on its own
/// line with the result's label.
#[gpui_kit::test]
fn the_quote_panel_has_the_extension_tabs(cx: &mut TestAppContext) {
    use crate::app::reading::RSheet;
    let mut env = setup(cx);
    let before = env.open_at_end();
    env.cx.simulate_keystrokes(&crate::keymap::keys("cmd-k"));
    env.cx.run_until_parked();
    let panel_tab = |v: &MainView| match &v.reading.sheet {
        Some(RSheet::Quote { ext_tabs, .. }) => {
            Some((ext_tabs.tabs.len(), ext_tabs.tab, ext_tabs.rows.clone()))
        }
        _ => None,
    };
    let shown = env.view.read_with(env.cx, |v, _| panel_tab(v));
    assert_eq!(
        shown.map(|(n, tab, _)| (n, tab)),
        Some((1, None)),
        "the quote panel, one tab for quotes, items open"
    );
    env.click("pick-tab-0");
    let tab = env.view.read_with(env.cx, |v, _| panel_tab(v));
    assert_eq!(tab.map(|(_, t, _)| t), Some(Some(0)), "the tab is open");
    env.type_text("polanyi");
    let typed = env.view.read_with(env.cx, |v, cx| match &v.reading.sheet {
        Some(RSheet::Quote { input, .. }) => input.read(cx).value().to_string(),
        _ => "(closed)".into(),
    });
    assert_eq!(typed, "polanyi", "typed in the panel's box");
    env.wait("the extension's results in the panel", |v, _| {
        panel_tab(v).is_some_and(|(_, _, r)| matches!(r, TabRows::Found(_)))
    });
    let (_, tab, rows) = env.view.read_with(env.cx, |v, _| panel_tab(v)).unwrap();
    assert_eq!(tab, Some(0));
    assert_eq!(
        rows.rows()[0].meta.as_deref(),
        Some("glossary · transclude · polanyi")
    );
    env.cx.simulate_keystrokes("enter");
    env.cx.run_until_parked();
    assert_eq!(env.text(), format!("{before}![[{A} | Polanyi 1966]]\n"));
}
