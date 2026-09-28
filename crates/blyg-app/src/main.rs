//! Blygger Desktop: a Notational-Velocity-style GPUI client for a blyg.
//!
//! With a `blyg-url` in the config and its owner token in the Keychain, the
//! app runs on `LiveBackend` (local SQLite + sync with the blyg). With none,
//! it asks you to connect one. `BLYGGER_FAKE=1 cargo run -p blyg-app` runs
//! the full UI on in-memory sample data instead (see `connection.rs`).
//!
//! Settings come from one plain-text config file (see `blygger +show-config
//! --default --docs`); `blygger +action` runs a command-line action instead
//! of the app (see `cli.rs`).

mod about; // --- about ---
mod ai;
mod app;
mod capture;
mod cli;
mod composer; // --- composer --- (@-mentions and spellcheck)
mod connection;
mod fake;
mod fonts;
mod images;
mod keymap;
mod platform;
mod prefs;
mod settings;
mod theme;
mod update;
mod vm;

use std::process::ExitCode;
use std::sync::Arc;
use std::time::Instant;

use blyg_core::config::{ConfigFiles, MemoryTokenStore, TokenStore};
use blyg_core::{Backend, ConfigStore};
use gpui_kit::*;

use app::MainView;
use connection::{Connection, Disconnected, Mode, SwitchBackend};
use fake::FakeBackend;
use prefs::Prefs;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(code) = cli::run(&args) {
        return code;
    }
    let launched = Instant::now();

    let files = ConfigFiles::discover();
    let notice = settings::migrate_on_launch(&files.primary);
    let store = ConfigStore::open(files);
    for d in settings::diagnostics(store.loaded()) {
        eprintln!("blygger: {d}");
    }
    let mut prefs = Prefs::from_config(store.config());
    prefs.apply_theme_override();
    // Fake mode never touches the real Keychain.
    let fake_mode = std::env::var_os("BLYGGER_FAKE").is_some();
    // BLYGGER_TEST_TOKEN (automation against a local `wrangler dev`): an
    // in-memory token for the configured blyg-url, and no Keychain at all.
    let tokens: Arc<dyn TokenStore> = if fake_mode {
        Arc::new(MemoryTokenStore::default())
    } else if let Ok(t) = std::env::var("BLYGGER_TEST_TOKEN") {
        let m = MemoryTokenStore::default();
        if let Some(u) = store.config().blyg_url() {
            let _ = m.set(u, t.trim());
        }
        Arc::new(m)
    } else {
        Arc::new(blyg_core::config::KeychainTokenStore)
    };

    // Which backend: fake (dev), live (a configured blyg with a token), or
    // disconnected until the Connect sheet succeeds.
    let data_dir = blyg_core::config::data_dir();
    images::init(&data_dir);
    let fake = fake_mode.then(|| Arc::new(FakeBackend::new()));
    // BLYGGER_FAKE_NO_PROVENANCE=1: act like a Worker without the provenance
    // extension (to see the publish-without-disclosure warning).
    if let Some(f) = &fake
        && std::env::var_os("BLYGGER_FAKE_NO_PROVENANCE").is_some()
    {
        f.set_provenance_available(false);
    }
    let (inner, mode): (Arc<dyn Backend>, Mode) = match (&fake, store.config().blyg_url()) {
        (Some(f), _) => (f.clone(), Mode::Fake),
        (None, Some(url)) => match connection::open_live(&data_dir, url, &*tokens) {
            Ok(Some(live)) => (live, Mode::Live),
            Ok(None) => {
                eprintln!("blygger: no owner token for {url} in the Keychain; connect again");
                (Arc::new(Disconnected), Mode::Disconnected)
            }
            Err(e) => {
                eprintln!("blygger: couldn't open the local database: {e}");
                (Arc::new(Disconnected), Mode::Disconnected)
            }
        },
        (None, None) => (Arc::new(Disconnected), Mode::Disconnected),
    };
    if std::env::var_os("BLYGGER_TIMING").is_some() {
        println!("backend={mode:?}");
    }
    let switch = SwitchBackend::new(inner, mode);
    let backend: Arc<dyn Backend> = switch.clone();
    let connection = Connection {
        switch,
        data_dir,
        verify: if fake_mode {
            connection::fake_verifier
        } else {
            blyg_core::api::verify_connection
        },
    };

    let state_dir = connection.data_dir.clone(); // --- auto-update ---

    let app = gpui_kit::application();
    // Dock click with no window open → reopen the main window.
    {
        let (b, f) = (backend.clone(), fake.clone());
        app.on_reopen(move |cx| {
            let alive =
                capture::main_window(cx).is_some_and(|h| h.update(cx, |_, _, _| ()).is_ok());
            if !alive {
                let prefs = settings::prefs(cx);
                open_main(b.clone(), f.clone(), prefs, Instant::now(), cx);
            }
        });
    }
    app.run(move |cx: &mut App| {
        gpui_kit::init(cx);
        platform::set_dock_icon_unless_bundled();
        settings::init(store, tokens, notice, cx);
        ai::init(cx);
        cx.set_global(connection);
        // A remote image (a profile avatar) landed in the cache: repaint.
        {
            let (tx, rx) = async_channel::unbounded::<()>();
            images::set_on_loaded(move || {
                let _ = tx.try_send(());
            });
            cx.spawn(async move |cx| {
                while rx.recv().await.is_ok() {
                    cx.update(|cx| cx.refresh_windows());
                }
            })
            .detach();
        }
        fonts::ensure(prefs.writing().bundled, cx);
        fonts::ensure(prefs.ui().bundled, cx);
        // Status bar, hints and pills are always Inter (as in the mock).
        fonts::ensure(Some(fonts::Bundle::Inter), cx);

        app::bind_keys(cx);
        cx.on_action(|_: &app::Quit, cx| cx.quit());
        cx.on_action(|_: &app::ShowCapture, cx| capture::toggle(cx));
        // --- composer --- the system spell checker, on unless `spellcheck = false`.
        composer::init(composer::system_engine(), prefs.spellcheck, cx);
        cx.on_action(|_: &composer::ToggleSpellcheck, cx| {
            if let Err(e) = composer::toggle_spellcheck(cx) {
                eprintln!("blygger: {e}");
            }
        });
        cx.set_menus(menus(prefs.spellcheck));
        capture::init(backend.clone(), &prefs, cx);
        // --- auto-update --- (off in fake mode, tests and BLYGGER_NO_UPDATE)
        update::init(state_dir, cx);
        about::init(cx); // --- about ---

        open_main(backend.clone(), fake.clone(), prefs.clone(), launched, cx);
        // Polite activation in automation: don't steal focus from the user.
        cx.activate(!no_activate());
    });
    ExitCode::SUCCESS
}

fn open_main(
    backend: Arc<dyn Backend>,
    fake: Option<Arc<FakeBackend>>,
    prefs: Prefs,
    launched: Instant,
    cx: &mut App,
) {
    let bounds = Bounds::centered(None, size(px(1100.), px(720.)), cx);
    let opts = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        titlebar: Some(TitlebarOptions {
            title: Some(vm::window_title(settings::blyg_url(cx).as_deref()).into()),
            appears_transparent: true,
            traffic_light_position: Some(point(px(12.), px(11.))),
        }),
        window_min_size: Some(size(px(640.), px(360.))),
        app_id: Some(blyg_core::config::APP_ID.into()),
        focus: !no_activate(),
        ..Default::default()
    };
    match cx.open_window(opts, |window, cx| {
        cx.new(|cx| MainView::new(backend, fake, prefs, launched, window, cx))
    }) {
        Ok(handle) => {
            capture::set_main(handle, cx);
            if no_activate() {
                let _ = handle.update(cx, |_, window, _| order_front_regardless(window));
            }
            if let Ok(scenario) = std::env::var("BLYGGER_DEMO") {
                let _ = handle.update(cx, |view, window, cx| view.run_demo(&scenario, window, cx));
            }
        }
        Err(e) => eprintln!("blygger: couldn't open the main window: {e}"),
    }
}

/// Run a backend call that parses other people's content (profiles, public
/// items) on a background thread, turning a parser panic into an error. A
/// panic that escapes a background task aborts the whole app.
pub(crate) fn guarded<T>(f: impl FnOnce() -> blyg_core::Result<T>) -> blyg_core::Result<T> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).unwrap_or_else(|_| {
        Err(blyg_core::CoreError::Other(
            "couldn't read that page (it has something Blygger didn't expect)".into(),
        ))
    })
}

/// `BLYGGER_NO_ACTIVATE=1`: never take focus from the frontmost app (used by
/// automated screenshot runs so they can't swallow the user's keystrokes).
pub fn no_activate() -> bool {
    std::env::var_os("BLYGGER_NO_ACTIVATE").is_some()
}

/// Show a window without activating the app (automation only): an app that
/// never activates doesn't get its windows ordered in otherwise.
pub fn order_front_regardless(window: &Window) {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let Ok(handle) = HasWindowHandle::window_handle(window) else {
        return;
    };
    if let RawWindowHandle::AppKit(h) = handle.as_raw() {
        let view = h.ns_view.as_ptr() as *mut objc2::runtime::AnyObject;
        // SAFETY: ns_view is a live NSView owned by this window; we're on the
        // main thread (GPUI's foreground executor).
        unsafe {
            let ns_window: *mut objc2::runtime::AnyObject = objc2::msg_send![view, window];
            if !ns_window.is_null() {
                let _: () = objc2::msg_send![ns_window, orderFrontRegardless];
            }
        }
    }
}

/// --- composer --- Rebuild the menu bar (the Spelling item's check mark).
pub(crate) fn refresh_menus(cx: &mut App) {
    cx.set_menus(menus(composer::spellcheck_enabled(cx)));
}

fn menus(spellcheck: bool) -> Vec<Menu> {
    use gpui_kit::base::input::{Copy, Cut, Paste, Redo, SelectAll, Undo};
    vec![
        Menu {
            name: "Blygger".into(),
            items: vec![
                // --- about --- (also in Help)
                MenuItem::action("About Blygger", about::ShowAbout),
                MenuItem::separator(),
                MenuItem::action("Settings…", app::OpenSettings),
                MenuItem::action("Check for Updates…", update::CheckForUpdates), // --- auto-update ---
                MenuItem::action("Open Config File", app::OpenConfigFile),
                MenuItem::action("Reload Config", app::ReloadConfig),
                MenuItem::action("Quick Capture", app::ShowCapture),
                MenuItem::separator(),
                MenuItem::action("Disconnect…", app::Disconnect),
                MenuItem::separator(),
                MenuItem::action("Quit Blygger", app::Quit),
            ],
            disabled: false,
        },
        Menu {
            name: "Edit".into(),
            items: vec![
                MenuItem::action("Undo", Undo),
                MenuItem::action("Redo", Redo),
                MenuItem::separator(),
                MenuItem::action("Cut", Cut),
                MenuItem::action("Copy", Copy),
                MenuItem::action("Paste", Paste),
                MenuItem::action("Select All", SelectAll),
                // --- composer ---
                MenuItem::separator(),
                MenuItem::submenu(Menu {
                    name: "Spelling".into(),
                    items: vec![
                        MenuItem::action("Check Spelling While Typing", composer::ToggleSpellcheck)
                            .checked(spellcheck),
                    ],
                    disabled: false,
                }),
            ],
            disabled: false,
        },
        Menu {
            name: "Post".into(),
            items: vec![
                MenuItem::action("New Draft", app::NewDraft),
                MenuItem::action("Find…", app::FocusSearch),
                MenuItem::separator(),
                MenuItem::action("Publish…", app::Publish),
                MenuItem::action("Make Draft", app::scratch::MakeDraft),
                MenuItem::action("Fragment ⇄ Thread", app::ToggleKind),
                MenuItem::action("Open on the Web", app::OpenPermalink),
                MenuItem::separator(),
                MenuItem::action("Generate (TK)", ai::AiGenerate),
                MenuItem::action("Shorten to Fit 1000", ai::AiShorten),
                // --- reading & versions ---
                MenuItem::separator(),
                MenuItem::action("Versions…", app::reading::ShowVersions),
                MenuItem::action("Quote…", app::reading::QuotePicker),
                // --- delete & withdraw ---
                MenuItem::separator(),
                MenuItem::action("Delete Draft…", app::discard::DeleteDraft),
                MenuItem::action("Withdraw…", app::discard::Withdraw),
            ],
            disabled: false,
        },
        // --- reading & versions ---
        Menu {
            name: "Blyg".into(),
            items: vec![
                MenuItem::action("Reading", app::reading::ShowReading),
                // --- stream ---
                MenuItem::action("Stream", app::reading::stream::StreamMode),
                MenuItem::action("Reader", app::reading::stream::ReaderMode),
                // --- reader folders ---
                MenuItem::action("Sources Pane", app::reading::sources::ToggleSources),
                MenuItem::action("New Folder…", app::reading::sources::NewFolder),
                MenuItem::action("Mentions", app::reading::ShowMentions),
                MenuItem::action("Subscriptions", app::reading::ShowSubscriptions),
                MenuItem::separator(),
                MenuItem::action("Subscribe…", app::reading::SubscribeTo),
                MenuItem::action("Site Settings…", app::reading::SiteSettings),
                // --- profiles ---
                MenuItem::separator(),
                MenuItem::action("Profile", app::profiles::ShowProfile),
                MenuItem::action("Open Profile…", app::profiles::OpenProfile),
                MenuItem::action("My Profile", app::profiles::MyProfile),
            ],
            disabled: false,
        },
        Menu {
            name: "View".into(),
            items: vec![
                // --- full editor ---
                MenuItem::action("Write", app::studio::ViewWrite),
                MenuItem::action("List + Editor + Preview", app::studio::ViewSplit),
                MenuItem::action("Full Editor", app::studio::ViewStudio),
                MenuItem::action("Preview", app::TogglePreview),
                MenuItem::separator(),
                MenuItem::action("Bigger", app::FontBigger),
                MenuItem::action("Smaller", app::FontSmaller),
                MenuItem::action("Actual Size", app::FontReset),
                // --- browser ---
                MenuItem::separator(),
                MenuItem::action("Browser Pane", app::browser::ToggleBrowser),
                MenuItem::action("Notes", app::notes::ToggleNotes), // --- notes ---
            ],
            disabled: false,
        },
        // --- onboarding ---
        Menu {
            name: "Help".into(),
            items: vec![
                MenuItem::action("Blygger Tutorial", app::onboarding::ShowTutorial),
                // --- about --- (also in the Blygger menu)
                MenuItem::separator(),
                MenuItem::action("About Blygger", about::ShowAbout),
            ],
            disabled: false,
        },
    ]
}

// --- buttons ---
#[cfg(test)]
mod menu_tests {
    use gpui_kit::MenuItem;

    /// Menus, shortcuts and toolbar buttons all come from `keymap::table()`:
    /// every menu item here is a table row with the same menu path and
    /// action, and every row with a menu is in a menu.
    #[test]
    fn menus_match_the_keymap_table() {
        let table = crate::keymap::table();
        let mut shown = Vec::new();
        for menu in super::menus(true) {
            let items: Vec<(String, MenuItem)> = if menu.name == "Edit" {
                // The standard text-editing items are gpui-base's actions;
                // only its submenus (Spelling) are ours.
                menu.items
                    .into_iter()
                    .filter_map(|i| match i {
                        MenuItem::Submenu(sub) => Some(sub),
                        _ => None,
                    })
                    .flat_map(|sub| {
                        let prefix = format!("Edit › {}", sub.name);
                        sub.items.into_iter().map(move |i| (prefix.clone(), i))
                    })
                    .collect()
            } else {
                let name = menu.name.to_string();
                menu.items.into_iter().map(|i| (name.clone(), i)).collect()
            };
            for (menu_name, item) in items {
                if let MenuItem::Action { name, action, .. } = item {
                    let path = format!("{menu_name} › {name}");
                    let action = action.name().rsplit("::").next().unwrap_or_default();
                    assert!(
                        table
                            .iter()
                            .any(|k| k.menu == Some(path.as_str()) && k.action == action),
                        "menu item {path} ({action}) isn't in keymap::table()"
                    );
                    shown.push(path);
                }
            }
        }
        for k in table.iter().filter_map(|k| k.menu) {
            assert!(
                shown.iter().any(|s| s == k),
                "{k} is in the table but no menu"
            );
        }
    }
}
