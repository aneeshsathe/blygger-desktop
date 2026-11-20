//! The main window: omnibar, list, editor, status bar, sheets and toasts.
//! Behaviour decisions live in `vm`; this file wires them to GPUI.

use std::sync::Arc;
use std::time::Duration;

use blyg_core::api::auth::Credential;
use blyg_core::{Backend, CoreError, CoreEvent, Item, Kind, LocalId, Resolution, SyncStatus};
use gpui_kit::base::input::{
    Enter, Escape, InputEvent, InputState, MoveDown, MoveUp, Paste, TextareaState,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::fake::FakeBackend;
use crate::prefs::{self, LayoutPref, Prefs};
use crate::theme::{Palette, Rule, Theme};
use crate::vm::{self, DiffOp, EnterAction, ListModel, PublishDecision};

gpui_kit::actions!(
    blygger,
    [
        FocusSearch,
        Publish,
        ToggleKind,
        TogglePreview,
        OpenPermalink,
        OpenSettings,
        FontBigger,
        FontSmaller,
        FontReset,
        NewDraft,
        Quit,
        FakeToggleOffline,
        FakeConflict,
        ShowCapture,
        OpenConfigFile,
        ReloadConfig,
        Disconnect,
    ]
);

#[cfg(test)]
#[path = "ui_tests.rs"]
mod ui_tests;
// --- themes ---
#[cfg(test)]
#[path = "theme_tests.rs"]
mod theme_tests;

#[cfg(test)]
#[path = "posts_perf_tests.rs"]
mod posts_perf_tests; // the Posts screen at scale
// --- composer --- (its GPUI tests need MainView's insides)
#[cfg(test)]
#[path = "composer/assist_tests.rs"]
mod composer_tests;

#[path = "demo.rs"]
mod demo;
// --- composer --- (the cm-* demo scenarios)
#[path = "composer/demo.rs"]
mod composer_demo;

#[path = "scratch.rs"]
pub(crate) mod scratch;
// --- new post --- ⌘N: a fresh editor that becomes a scratch note (or a draft)
#[path = "new_post.rs"]
pub(crate) mod new_post;
// --- AI --- (the window's AI UI; logic in crate::ai)
#[path = "ai/view.rs"]
mod ai_view;
// --- full editor ---
#[path = "studio/mod.rs"]
pub(crate) mod studio;
// --- reading & versions: the screens live in reading/ (a child module so
// they can extend MainView); the hooks below are marked the same way.
#[path = "reading/mod.rs"]
pub(crate) mod reading;
// --- onboarding --- (first-run flow + the tutorial; hooks marked the same way)
#[path = "onboarding/mod.rs"]
pub(crate) mod onboarding;
// --- buttons --- the optional toolbar (show-buttons)
#[path = "toolbar.rs"]
pub(crate) mod toolbar;

// Settings: a full-window page over the app (rows shared by every section).
#[path = "settings_page.rs"]
pub(crate) mod settings_page;
// --- profiles --- (the profile sheet; hooks marked the same way)
#[path = "profiles/mod.rs"]
pub(crate) mod profiles;
// --- delete & withdraw --- (drafts are deleted, published posts withdrawn)
#[path = "discard.rs"]
pub(crate) mod discard;
// --- auto-update --- (the status-bar notice; logic in crate::update)
#[path = "server_notice.rs"]
pub(crate) mod server_notice;
#[path = "update/view.rs"]
mod update_view;
// --- browser --- (the in-app browser pane; hooks marked the same way)
#[path = "browser/mod.rs"]
// The pane is macOS and Windows only (WKWebView, WebView2); elsewhere links
// open in the default browser, which leaves its machinery unused.
#[cfg_attr(not(any(target_os = "macos", target_os = "windows")), allow(dead_code))]
pub(crate) mod browser;
// --- notes --- (the notes drawer; hooks marked the same way)
#[path = "notes/mod.rs"]
pub(crate) mod notes;
// --- extensions --- (the host, its sheets and the notes library; hooks marked the same way)
#[path = "extensions/mod.rs"]
pub(crate) mod extensions;
// --- blyg switcher --- (several blygs, one active; hooks marked the same way)
#[path = "switcher.rs"]
pub(crate) mod switcher;
// --- shelf --- (posts kept to hand; hooks marked the same way)
#[path = "shelf.rs"]
pub(crate) mod shelf;
// --- access-ai --- client access, AI on the blyg, the publish sheet's AI
// (hooks marked the same way)
#[path = "access.rs"]
pub(crate) mod access;
#[cfg(test)]
#[path = "access_ai_tests.rs"]
mod access_ai_tests;
#[path = "note_ai.rs"]
pub(crate) mod note_ai;
#[path = "server_ai.rs"]
pub(crate) mod server_ai;
// --- upkeep --- (stale quotes, signals, version diffs, alt text, copy link;
// hooks marked the same way)
#[path = "upkeep/mod.rs"]
pub(crate) mod upkeep;

// --- syndication --- copies elsewhere (decision #98; hooks marked the same way)
#[path = "syndication_ui.rs"]
pub(crate) mod syndication;

// --- tangles --- trails, the quote tray and drag-to-quote, split view
// (hooks marked `// --- trails ---`, `// --- tray ---`, `// --- split ---`)
#[path = "split.rs"]
pub(crate) mod split;
#[cfg(test)]
#[path = "split_tests.rs"]
mod split_tests;
#[path = "trails.rs"]
pub(crate) mod trails;
#[cfg(test)]
#[path = "trails_tests.rs"]
mod trails_tests;
#[path = "tray.rs"]
pub(crate) mod tray;
#[cfg(test)]
#[path = "tray_tests.rs"]
mod tray_tests;
// --- lineage --- (the open post's lineage under the editor; hooks marked the same way)
#[path = "lineage_strip.rs"]
pub(crate) mod lineage_strip;
// --- hover cards --- (what a [[id]] is, on hover; hooks marked the same way)
#[path = "hover_card.rs"]
pub(crate) mod hover_card;
// --- flow --- (the command palette, focus mode, serendipity, delight;
// hooks marked `// --- palette ---`, `// --- focus ---`, `// --- serendipity ---`,
// `// --- delight ---`)
#[path = "palette.rs"]
pub(crate) mod command_palette;
#[path = "delight.rs"]
pub(crate) mod delight;
#[cfg(test)]
#[path = "flow_tests.rs"]
mod flow_tests;
#[path = "focus.rs"]
pub(crate) mod focus;
#[path = "serendipity.rs"]
pub(crate) mod serendipity;
// --- nearby --- (Nearby thoughts: the rail beside the editor; hooks marked the same way)
#[path = "nearby/mod.rs"]
pub(crate) mod nearby;
// --- side --- (the side drawers, their tab bar, the editor | preview divider)
#[path = "side.rs"]
pub(crate) mod side;
#[cfg(test)]
#[path = "side_tests.rs"]
mod side_tests;

// --- loose ends --- the omnibar's operators' summary over the list.
#[path = "loose_ends.rs"]
pub(crate) mod loose_ends;

// --- quiet --- Quiet writing: one setting that hides the lineage line,
// loose ends and Nearby thoughts (hooks marked the same way).
#[path = "quiet.rs"]
pub(crate) mod quiet;
#[cfg(test)]
#[path = "quiet_tests.rs"]
mod quiet_tests;

// --- preview --- trying a theme, font or size before keeping it.
#[path = "pref_preview.rs"]
pub(crate) mod pref_preview;
#[cfg(test)]
#[path = "pref_preview_tests.rs"]
mod pref_preview_tests;

// --- capture --- a web address pasted into the omnibar.
#[path = "capture_link.rs"]
pub(crate) mod capture_link;

// --- site --- Blyg › Site…: the public site's settings (hooks marked the same way)
#[path = "site/mod.rs"]
pub(crate) mod site;
// --- end site ---

pub const CONTEXT: &str = "Blygger";

/// The empty list while no blyg is connected (setup skipped): Enter or a
/// click connects; the other half starts the sample data, as setup's "S"
/// does.
pub const EMPTY_CONNECT: &str = "⏎ Connect your blyg…";
pub const EMPTY_SAMPLE: &str = "try sample data";

/// The first publish sheet's muted line (v1 only): it goes public, and a
/// published post can be withdrawn (visibly) but never deleted.
pub fn first_publish_note(host: Option<&str>) -> String {
    match host {
        Some(h) => {
            format!("This goes public at {h}. Published posts can be withdrawn, not deleted.")
        }
        None => {
            "This goes public on your blyg. Published posts can be withdrawn, not deleted.".into()
        }
    }
}

/// The Connect sheet's opening line, with Burrow's server floor taken from
/// `blyg_core::api::MIN_STUDIO` (never typed out: it went stale once).
pub const CONNECT_INTRO: &str = concat!(
    "Burrow writes to your own blyg, which needs blygger-studio ",
    blyg_core::min_studio!(),
    " or later. Enter its address and choose how to sign in. The address \
     goes in your config file."
);

/// Every binding lives in `keymap::table()`.
pub fn bind_keys(cx: &mut App) {
    crate::keymap::bind_keys(cx);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Search,
    Edit,
}

// --- writing --- the editor, publishing, conflicts, pasted links and images
mod publish;
mod upload;
mod writing;

/// --- sheets --- one main-window sheet at a time; each kind in `sheet/`.
#[path = "sheet/mod.rs"]
pub(crate) mod sheet;
use settings_page::SettingsSheet;
use sheet::conflict::ConflictSheet;
pub(crate) use sheet::connect::{CancelOnDrop, ConnectSheet, ConnectWay, DisconnectSheet};
use sheet::publish::{HeldLabelsSheet, PublishSheet};

struct Toast {
    id: usize,
    text: SharedString,
    sub: Option<SharedString>,
    leaving: bool,
}

/// The omnibar's search off the UI thread. Each search takes the next
/// generation; results that come back for an older one are dropped.
#[derive(Default)]
struct OmniSearch {
    generation: u64,
    /// The query typed whose results haven't come back yet.
    pending: Option<String>,
    task: Option<Task<()>>,
    /// Edits saved from the editor so far (`after_edit`): a copy of the
    /// open item read before the latest one is read again.
    edits: u64,
}

pub struct MainView {
    backend: Arc<dyn Backend>,
    fake: Option<Arc<FakeBackend>>,
    focus: FocusHandle,
    omni: Entity<InputState>,
    editor: Entity<TextareaState>,
    /// --- composer --- @-mentions and spellcheck over the editor.
    assist: Entity<crate::composer::Assist>,
    list: ListModel,
    /// The omnibar's search running off the UI thread (`query_typed`).
    search: OmniSearch,
    loose_ends: loose_ends::LooseEnds, // --- loose ends ---
    list_scroll: UniformListScrollHandle,
    current: Option<Item>,
    mode: Mode,
    loading_editor: bool,
    /// --- discard changes --- A restore of this post is on its way: the
    /// editor doesn't save it meanwhile (`discard::restore_working_copy`).
    restoring: Option<LocalId>,
    /// The full editor: view mode and the live preview (studio/mod.rs).
    studio: studio::Studio,
    /// The open sheet, if any (`sheet/`).
    sheet: sheet::SheetSlot,
    toast: Option<Toast>,
    toast_seq: usize,
    shake_gen: usize,
    sync: SyncStatus,
    /// The blyg origin for `media/…` links (`Backend::base_url`).
    base_url: Option<String>,
    publishing: bool,
    /// The first pull after connecting (or launching) is running.
    loading: bool,
    /// Reading-list posts not yet read (reader-local, never social).
    unread: usize,
    pub prefs: Prefs,
    /// What the config file says (`prefs` minus unsaved dev overrides);
    /// changes are written back as the difference from this.
    persisted: Prefs,
    /// Problems in the config file, shown as a dismissible banner.
    config_problems: Vec<blyg_core::config::Diagnostic>,
    problems_dismissed: bool,
    /// `Burrow — <host>` from the config's `blyg-url`.
    title: String,
    /// The title last given to the native window (Window menu, Mission Control).
    native_title: String,
    palette: Palette,
    /// The resolved theme (`palette` is `theme.palette`), for ornaments.
    theme: Arc<Theme>,
    /// The config files' and themes folder's state when last read, so an
    /// edit on disk reloads (see `watch_config`).
    watch_stamp: u64,
    now: chrono::DateTime<chrono::Utc>,
    first_frame: Option<std::time::Instant>,
    /// --- AI --- ⌘G jobs, the helper palette, proposals, Settings › AI.
    ai: ai_view::AiView,
    // --- reading & versions ---
    reading: reading::State,
    // --- onboarding ---
    onboarding: onboarding::State,
    // --- profiles ---
    profiles: profiles::State,
    // --- browser ---
    browser: browser::Browser,
    // --- notes ---
    notes: notes::Notes,
    // --- extensions ---
    ext: extensions::Extensions,
    // --- new post --- ⌘N's fresh editor, until it's left
    new_post: Option<new_post::NewPost>,
    // --- lineage ---
    lineage_strip: lineage_strip::State,
    // --- hover cards ---
    hover: hover_card::State,
    // --- flow ---
    cmd_palette: command_palette::State,
    focus_mode: focus::State,
    serendipity: serendipity::State,
    delight: delight::Delight,
    // --- end flow ---
    // --- tray --- quotes picked up, not placed yet
    tray: tray::Tray,
    // --- split --- a second post beside the one being written
    split: split::Split,
    nearby: nearby::State,              // --- nearby ---
    side: side::Side,                   // --- side ---
    site: site::State,                  // --- site ---
    contacts: reading::contacts::State, // --- contacts --- Mentions › Sent
    upkeep: upkeep::Upkeep,             // --- upkeep ---
    quiet: quiet::Quiet,                // --- quiet ---
    preview: pref_preview::State,       // --- preview ---
    _tasks: Vec<Task<()>>,
    _subs: Vec<Subscription>,
}

impl MainView {
    pub fn new(
        backend: Arc<dyn Backend>,
        fake: Option<Arc<FakeBackend>>,
        prefs: Prefs,
        launched: std::time::Instant,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let omni = cx.new(|cx| InputState::new(window, cx).placeholder("Search or start writing…"));
        let editor = cx.new(|cx| TextareaState::new(window, cx).soft_wrap(true));
        let mut prefs = prefs;
        let theme = crate::theme::resolve(&prefs, window.appearance(), cx);
        prefs.adopt_theme_fonts(&theme);
        let palette = theme.palette;
        crate::theme::set_reader_theme(&theme, prefs.writing());
        // --- composer ---
        let assist = {
            let (editor, backend) = (editor.clone(), backend.clone());
            cx.new(|cx| crate::composer::Assist::new(editor, backend, palette, window, cx))
        };

        let mut subs = Vec::new();
        subs.push(cx.subscribe_in(&omni, window, Self::on_omni_event));
        subs.push(cx.subscribe_in(&editor, window, Self::on_editor_event));
        subs.push(cx.observe_window_appearance(window, |this, window, cx| {
            this.apply_theme(window, cx);
            cx.notify();
        }));
        // A WebView can end up holding (or dropping) the keyboard while the
        // window is in the background; typing must work when it comes back.
        subs.push(cx.observe_window_activation(window, |this, window, _| {
            if window.is_window_active() {
                this.studio.reclaim_keyboard();
                this.browser.reclaim_lost_keyboard(); // --- browser ---
            }
        }));

        // Core events arrive on a background thread; hop them onto the UI.
        let (tx, rx) = async_channel::unbounded::<CoreEvent>();
        backend.set_event_sink(Box::new(move |ev| {
            let _ = tx.try_send(ev);
        }));
        let mut tasks = Vec::new();
        tasks.push(cx.spawn_in(window, async move |this, cx| {
            while let Ok(ev) = rx.recv().await {
                // A sync burst queues many events at once: take what's
                // queued and handle it as one batch, so it refreshes once.
                let mut batch = vec![ev];
                while let Ok(ev) = rx.try_recv() {
                    batch.push(ev);
                }
                if this
                    .update_in(cx, |v, window, cx| v.on_core_events(batch, window, cx))
                    .is_err()
                {
                    break;
                }
            }
        }));
        // Keep relative times fresh.
        tasks.push(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_secs(30))
                    .await;
                if this
                    .update(cx, |v, cx| {
                        v.now = chrono::Utc::now();
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
            }
        }));

        let sync = backend.sync_status();
        let base_url = backend.base_url();
        let persisted = crate::settings::prefs(cx);
        let config_problems = crate::settings::current_diagnostics(cx);
        let blyg_url = crate::settings::blyg_url(cx);
        let title = vm::window_title(blyg_url.as_deref());
        let notice = cx.global_mut::<crate::settings::AppConfig>().notice.take();
        // --- reading & versions ---
        let reading = reading::State::new(&*backend, cx);
        // --- browser ---
        let browser = browser::Browser::new(
            cx.try_global::<crate::connection::Connection>()
                .map(|c| c.data_dir.clone()),
            prefs.content_blocking,
        );
        // --- notes ---
        let notes = notes::Notes::new(
            cx.try_global::<crate::connection::Connection>()
                .map(|c| c.data_dir.clone()),
        );
        let mut this = Self {
            browser,                 // --- browser ---
            notes,                   // --- notes ---
            ext: Default::default(), // --- extensions ---
            reading,
            onboarding: onboarding::State::new(cx), // --- onboarding ---
            profiles: Default::default(),           // --- profiles ---
            backend,
            fake,
            focus: cx.focus_handle(),
            omni,
            editor,
            assist, // --- composer ---
            list: ListModel::default(),
            search: OmniSearch::default(),
            loose_ends: Default::default(), // --- loose ends ---
            list_scroll: UniformListScrollHandle::new(),
            current: None,
            mode: Mode::Search,
            loading_editor: false,
            restoring: None,
            studio: studio::Studio::new(
                cx.try_global::<crate::connection::Connection>()
                    .map(|c| c.data_dir.clone()),
            ),
            sheet: Default::default(),
            toast: None,
            toast_seq: 0,
            shake_gen: 0,
            sync,
            base_url,
            publishing: false,
            loading: false,
            unread: 0,
            prefs,
            persisted,
            config_problems,
            problems_dismissed: false,
            title,
            native_title: String::new(),
            palette,
            theme,
            watch_stamp: 0,
            now: chrono::Utc::now(),
            first_frame: Some(launched),
            ai: Default::default(),            // --- AI ---
            new_post: None,                    // --- new post ---
            lineage_strip: Default::default(), // --- lineage ---
            hover: Default::default(),         // --- hover cards ---
            // --- flow ---
            cmd_palette: Default::default(),
            focus_mode: Default::default(),
            serendipity: Default::default(),
            delight: Default::default(),
            // --- tray ---
            tray: tray::Tray::load(
                Self::tray_data_dir(cx),
                blyg_url
                    .as_deref()
                    .and_then(crate::vm::url_host)
                    .unwrap_or_default(),
            ),
            split: split::Split::new(Self::tray_data_dir(cx)), // --- split ---
            side: side::Side::new(
                cx.try_global::<crate::connection::Connection>()
                    .map(|c| c.data_dir.clone()),
            ), // --- side ---
            nearby: Default::default(),                        // --- nearby ---
            site: Default::default(),                          // --- site ---
            contacts: Default::default(),                      // --- contacts ---
            upkeep: Default::default(),                        // --- upkeep ---
            quiet: quiet::Quiet::new(cx),                      // --- quiet ---
            preview: Default::default(),                       // --- preview ---
            _tasks: tasks,
            _subs: subs,
        };
        this.reload_shelf(); // --- shelf ---
        this.upkeep_reload_stale(); // --- upkeep ---
        let results = this.backend.search("");
        this.list.set_query("", results);
        this.load_selected(window, cx);
        this.loose_ends_refresh(cx); // --- loose ends ---
        this.studio_init(window, cx);
        this.browser_init(window, cx); // --- browser ---
        this.watch_config(window, cx);
        this.ext_init(window, cx); // --- extensions ---
        this.nearby_init(cx); // --- nearby ---
        this.site_init(cx); // --- site ---
        this.omni.update(cx, |s, cx| s.focus(window, cx));
        if let Some(n) = notice {
            this.show_toast(n, None, cx);
        }
        // Withdrawn posts are background: never unread (studio 0.49).
        this.unread = this.backend.unread_count();
        let mode = crate::connection::mode(cx);
        if mode == Some(crate::connection::Mode::Live) {
            this.initial_sync(None, window, cx);
        }
        // --- onboarding --- (first launch, or tutorial-on-launch)
        if !this.onboarding_launch(blyg_url.is_some(), window, cx)
            && (mode == Some(crate::connection::Mode::Disconnected)
                || (blyg_url.is_none() && mode != Some(crate::connection::Mode::Fake)))
        {
            this.open_connect(window, cx);
        }
        this.notes_reopen(window, cx); // --- notes ---
        this
    }

    /// Pull everything once, showing "loading your posts…" until it's done;
    /// the list fills in from SQLite as the pull lands.
    fn initial_sync(
        &mut self,
        connected_to: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.loading = true;
        cx.notify();
        let backend = self.backend.clone();
        let task = cx.background_spawn(async move {
            let r = backend.sync_now();
            (r, backend.items().len())
        });
        cx.spawn_in(window, async move |this, cx| {
            let (result, n) = task.await;
            let _ = this.update_in(cx, |v, window, cx| {
                v.loading = false;
                v.requery(window, cx);
                if crate::dev_flags::get().timing {
                    println!("initial-sync items={n} ok={}", result.is_ok());
                }
                match (result, connected_to) {
                    // Connected to a blyg below the studio floor: say that,
                    // not "connected" (the status bar keeps saying it).
                    (Ok(()) | Err(CoreError::ServerOutdated), _) if v.backend.server_outdated() => {
                        v.server_outdated(cx)
                    }
                    (Ok(()), Some(host)) => v.show_toast(
                        format!("Connected to {host}"),
                        Some(
                            crate::keymap::hint_owned(format!(
                                "{n} post{} loaded · the token is in your Keychain",
                                if n == 1 { "" } else { "s" }
                            ))
                            .into(),
                        ),
                        cx,
                    ),
                    (Ok(()), None) => {}
                    (Err(CoreError::Offline), _) => v.show_toast(
                        crate::keymap::hint("Offline: showing what's on this Mac"),
                        Some("Changes sync when the blyg is reachable".into()),
                        cx,
                    ),
                    // An ended browser sign-in: ask for a new one (the
                    // sheet says why) rather than retrying.
                    (Err(CoreError::Unauthorized), _) if !v.sheet.is_open() => {
                        v.signed_out(window, cx);
                    }
                    (Err(e), _) => {
                        v.show_toast(format!("Couldn't load from the blyg: {e}"), None, cx)
                    }
                }
            });
        })
        .detach();
    }

    // ------------------------------------------------------------ config

    /// Write whatever changed in `prefs` back to the config file.
    fn save_prefs(&mut self, cx: &mut Context<Self>) {
        let changes = self.prefs.changes_from(&self.persisted);
        match crate::settings::write(&changes, cx) {
            Ok(()) => {
                // Only the keys that changed were written, so a dev override
                // of another key (BLYGGER_THEME) stays unsaved.
                let mut p = self.persisted.clone();
                for (key, _) in &changes {
                    match *key {
                        "font-family-writing" => {
                            p.writing_font = self.prefs.writing_font.clone();
                            p.writing_font_set = self.prefs.writing_font_set;
                        }
                        "font-family-ui" => {
                            p.ui_font = self.prefs.ui_font.clone();
                            p.ui_font_set = self.prefs.ui_font_set;
                        }
                        "font-size" => p.font_size = self.prefs.font_size,
                        "theme" => p.theme = self.prefs.theme.clone(),
                        "theme-dark" => p.theme_dark = self.prefs.theme_dark.clone(),
                        "layout" => p.layout = self.prefs.layout,
                        "capture-hotkey" => p.hotkey = self.prefs.hotkey.clone(),
                        "show-buttons" => p.show_buttons = self.prefs.show_buttons, // --- buttons ---
                        // --- reader view / changelog ---
                        "open-posts-on-site" => p.author_site = self.prefs.author_site,
                        _ => {}
                    }
                }
                self.persisted = p;
                self.refresh_problems(cx);
                // Our own write isn't an edit to reload.
                self.watch_stamp = crate::settings::watch_stamp(&crate::settings::watched(cx));
            }
            Err(e) => self.show_toast(e, None, cx),
        }
    }

    fn refresh_problems(&mut self, cx: &mut Context<Self>) {
        let problems = crate::settings::current_diagnostics(cx);
        if problems != self.config_problems {
            self.problems_dismissed = false;
        }
        self.config_problems = problems;
        self.title = vm::window_title(crate::settings::blyg_url(cx).as_deref());
    }

    /// ⌘⇧, — re-read the config file and apply fonts, theme, layout and the
    /// hotkey without a restart.
    fn reload_config(&mut self, _: &ReloadConfig, window: &mut Window, cx: &mut Context<Self>) {
        self.reload_config_now(window, cx);
    }

    /// Poll the config files and the themes folder (cheap `stat`s, once a
    /// second, off the main thread) and reload when one changes on disk.
    fn watch_config(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let paths = crate::settings::watched(cx);
        self.watch_stamp = crate::settings::watch_stamp(&paths);
        if cfg!(test) {
            return;
        }
        cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(1000))
                    .await;
                let Ok(paths) = this.update(cx, |_, cx| crate::settings::watched(cx)) else {
                    break;
                };
                let stamp = cx
                    .background_spawn(async move { crate::settings::watch_stamp(&paths) })
                    .await;
                let alive = this.update_in(cx, |v, window, cx| {
                    if stamp != v.watch_stamp {
                        v.watch_stamp = stamp;
                        v.reload_config_now(window, cx);
                    }
                });
                if alive.is_err() {
                    break;
                }
            }
        })
        .detach();
    }

    fn reload_config_now(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        crate::settings::reload(cx);
        crate::theme::reload(cx);
        self.watch_stamp = crate::settings::watch_stamp(&crate::settings::watched(cx));
        let fresh = crate::settings::prefs(cx);
        let hotkey_changed = fresh.hotkey != self.prefs.hotkey;
        self.persisted = fresh.clone();
        self.preview.active = None; // --- preview --- the file wins
        self.prefs = fresh;
        let mut msg = "Config reloaded".to_string();
        if hotkey_changed && let Err(e) = crate::capture::set_hotkey(&self.prefs.hotkey, cx) {
            msg = format!("Config reloaded, but the hotkey didn't change: {e}");
        }
        self.problems_dismissed = false;
        self.refresh_problems(cx);
        // --- composer ---
        crate::composer::set_spellcheck(self.prefs.spellcheck, cx);
        crate::refresh_menus(cx);
        if !self.config_problems.is_empty() {
            let n = self.config_problems.len();
            msg = format!("{msg} · {n} problem{}", if n == 1 { "" } else { "s" });
        }
        self.apply_prefs_live(window, cx);
        self.ext_reload(window, cx); // --- extensions ---
        self.nearby_reload(cx); // --- nearby ---
        self.quiet_reload(cx); // --- quiet ---
        self.show_toast(msg, None, cx);
    }

    fn open_config_file(&mut self, _: &OpenConfigFile, _: &mut Window, cx: &mut Context<Self>) {
        self.open_config_file_now(cx);
    }

    fn open_config_file_now(&mut self, cx: &mut Context<Self>) {
        match crate::settings::open_config_file(cx) {
            Ok(path) => self.show_toast(
                format!("Opening {}", blyg_core::config::paths::tilde(&path)),
                Some(crate::keymap::hint("Save it, then ⌘⇧, reloads").into()),
                cx,
            ),
            Err(e) => self.show_toast(e, None, cx),
        }
    }

    /// No blyg connected, no posts and nothing typed: the empty list offers
    /// Connect (and sample data), and Enter connects.
    pub(crate) fn offers_connect(&self, cx: &App) -> bool {
        crate::connection::mode(cx) == Some(crate::connection::Mode::Disconnected)
            && self.list.results().is_empty()
            && self.list.trimmed_query().is_empty()
    }

    fn open_connect(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let url = cx.new(|cx| InputState::new(window, cx).placeholder("https://blyg.example.com"));
        let token = cx.new(|cx| {
            InputState::new(window, cx)
                .masked(true)
                .placeholder("API token")
        });
        let t2 = token.clone();
        self._subs.push(
            cx.subscribe_in(&url, window, move |this, _, ev, window, cx| {
                if let InputEvent::PressEnter { .. } = ev {
                    // Signing in with the browser needs only the address.
                    if this
                        .sheet
                        .get::<ConnectSheet>()
                        .is_some_and(|s| s.way == ConnectWay::Browser)
                    {
                        this.submit_connect(window, cx);
                    } else {
                        t2.update(cx, |s, cx| s.focus(window, cx));
                    }
                }
            }),
        );
        self._subs
            .push(cx.subscribe_in(&token, window, |this, _, ev, window, cx| {
                if let InputEvent::PressEnter { .. } = ev {
                    this.submit_connect(window, cx);
                }
            }));
        // A configured blyg without a usable credential (the Keychain entry
        // went away, or its browser sign-in ended): offer its address.
        let mut error = None;
        if let Some(u) = crate::settings::blyg_url(cx) {
            let tokens = crate::settings::get(cx).tokens.clone();
            if let Ok(Some(g)) = blyg_core::config::load_grant(tokens.as_ref(), &u)
                && g.ended()
            {
                error = Some(format!(
                    "Your browser sign-in to {} has ended. Sign in again.",
                    vm::url_host(&u).unwrap_or(u.clone())
                ));
            }
            url.update(cx, |s, cx| s.set_value(u, window, cx));
        }
        url.update(cx, |s, cx| s.focus(window, cx));
        self.sheet.open(ConnectSheet {
            url,
            token,
            way: ConnectWay::Browser,
            error,
            busy: false,
            waiting: None,
        });
        cx.notify();
    }

    /// The Connect sheet's way in: browser sign-in, API token or password.
    pub(crate) fn set_connect_way(
        &mut self,
        pick: ConnectWay,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(ConnectSheet {
            url,
            token,
            way,
            error,
            busy,
            waiting,
        }) = self.sheet.get_mut()
        {
            *way = pick;
            *error = None;
            // Choosing another way cancels a browser sign-in in progress.
            if waiting.take().is_some() {
                *busy = false;
            }
            match pick {
                ConnectWay::Browser => url.update(cx, |s, cx| s.focus(window, cx)),
                ConnectWay::Token | ConnectWay::Password => {
                    let hint = if pick == ConnectWay::Password {
                        "Studio password"
                    } else {
                        "API token"
                    };
                    token.update(cx, |s, cx| {
                        s.set_value("", window, cx);
                        s.set_placeholder(hint, window, cx);
                        s.focus(window, cx);
                    });
                }
            }
        }
        cx.notify();
    }

    fn submit_connect(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(ConnectSheet {
            url,
            token,
            way,
            busy,
            ..
        }) = self.sheet.get()
        else {
            return;
        };
        if *busy {
            return;
        }
        let way = *way;
        let (u, t) = (
            url.read(cx).value().to_string(),
            token.read(cx).value().to_string(),
        );
        if way == ConnectWay::Browser {
            let checked = crate::settings::validate_blyg_url(&u)
                .and_then(|u| crate::settings::check_one_per_host(&u, cx).map(|()| u));
            match checked {
                Ok(u) => self.start_browser_sign_in(u, window, cx),
                Err(e) => self.set_connect_state(Some(e), false, cx),
            }
            return;
        }
        let by_password = way == ConnectWay::Password;
        let checked = crate::settings::validate_blyg_url(&u).and_then(|u| {
            // A password is taken as typed; a pasted token is trimmed.
            let secret = if by_password {
                t.clone()
            } else {
                t.trim().to_string()
            };
            match (secret.is_empty(), by_password) {
                (true, true) => Err("Type your blyg's studio password".to_string()),
                (true, false) => {
                    Err("Paste an API token: make one in Studio → More → Client access".to_string())
                }
                (false, true) => Ok((u, Credential::Password(secret))),
                (false, false) => Ok((u, Credential::Token(secret))),
            }
        });
        let (u, t) = match checked {
            Ok(v) => v,
            Err(e) => {
                self.set_connect_state(Some(e), false, cx);
                return;
            }
        };
        // Verify before saving anything: sign in, then GET /api/items.
        self.set_connect_state(None, true, cx);
        let verify = crate::connection::verifier(cx);
        let task = cx.background_spawn({
            let (u, t) = (u.clone(), t.clone());
            async move { verify(&u, &t) }
        });
        cx.spawn_in(window, async move |this, cx| {
            let verdict = task.await;
            let _ = this.update_in(cx, |v, window, cx| match verdict {
                Err(e) => v.set_connect_state(Some(e.to_string()), false, cx),
                Ok(_) => v.finish_connect(&u, &t, window, cx),
            });
        })
        .detach();
    }

    /// Sign in with the browser: discover and register (in the
    /// background), open the authorize page, wait for the loopback
    /// callback, exchange the code, verify, then save like any credential.
    fn start_browser_sign_in(&mut self, url: String, window: &mut Window, cx: &mut Context<Self>) {
        // Only against a real blyg: never from tests or BLYGGER_FAKE's
        // sample data (registering a client writes to the server).
        let real = cx
            .try_global::<crate::connection::Connection>()
            .is_some_and(|c| {
                c.switch.mode() != crate::connection::Mode::Fake
                    || cx.has_global::<crate::connection::SampleSession>()
            });
        if !real {
            return self.set_connect_state(
                Some("Browser sign-in needs a real blyg; use a token or password here.".into()),
                false,
                cx,
            );
        }
        let cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
        if let Some(ConnectSheet {
            error,
            busy,
            waiting,
            ..
        }) = self.sheet.get_mut()
        {
            *error = None;
            *busy = true;
            *waiting = Some(CancelOnDrop(cancel.clone()));
        }
        cx.notify();
        let tokens = crate::settings::get(cx).tokens.clone();
        let verify = crate::connection::verifier(cx);
        let start = cx.background_spawn({
            let url = url.clone();
            async move { blyg_core::api::oauth::BrowserSignIn::start(&url, tokens.as_ref()) }
        });
        cx.spawn_in(window, async move |this, cx| {
            let started = start.await;
            if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                return; // the sheet closed, or another way was chosen
            }
            let sign_in = match started {
                Ok(Some(s)) => s,
                other => {
                    let _ = this.update_in(cx, |v, window, cx| {
                        let msg = match other {
                            Ok(_) => {
                                // No OAuth routes: the password works.
                                v.set_connect_way(ConnectWay::Password, window, cx);
                                "This blyg has no browser sign-in. Use its studio password."
                                    .to_string()
                            }
                            Err(CoreError::Offline) => {
                                "Couldn't reach that address. Check the URL and your connection."
                                    .into()
                            }
                            Err(e) => e.to_string(),
                        };
                        v.set_connect_state(Some(msg), false, cx);
                        v.clear_connect_wait();
                    });
                    return;
                }
            };
            let _ = this.update(cx, |_, cx| cx.open_url(&sign_in.authorize_url));
            let task = cx.background_spawn({
                let (url, cancel) = (url.clone(), cancel.clone());
                async move {
                    let grant = sign_in.finish(&cancel, blyg_core::api::oauth::SIGN_IN_TIMEOUT)?;
                    let cred = Credential::OAuth(blyg_core::api::oauth::OAuthSession::new(grant));
                    verify(&url, &cred)
                        .map(|_| cred)
                        .map_err(|e| CoreError::Other(e.to_string()))
                }
            });
            let verdict = task.await;
            if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                return; // the sheet closed, or another way was chosen
            }
            let _ = this.update_in(cx, |v, window, cx| {
                v.clear_connect_wait();
                match verdict {
                    Ok(cred) => v.finish_connect(&url, &cred, window, cx),
                    Err(CoreError::Offline) => v.set_connect_state(
                        Some(
                            "Couldn't reach that address. Check the URL and your connection."
                                .into(),
                        ),
                        false,
                        cx,
                    ),
                    Err(e) => v.set_connect_state(Some(e.to_string()), false, cx),
                }
            });
        })
        .detach();
    }

    /// The browser sign-in is over: drop its cancel flag (nothing waits on
    /// it any more).
    fn clear_connect_wait(&mut self) {
        if let Some(ConnectSheet { waiting, .. }) = self.sheet.get_mut() {
            waiting.take();
        }
    }

    fn set_connect_state(&mut self, err: Option<String>, is_busy: bool, cx: &mut Context<Self>) {
        if let Some(ConnectSheet { error, busy, .. }) = self.sheet.get_mut() {
            *error = err;
            *busy = is_busy;
        }
        cx.notify();
    }

    /// The check passed: save the credential and URL, switch to the live
    /// backend, and load everything once.
    fn finish_connect(
        &mut self,
        url: &str,
        cred: &Credential,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let url = match crate::settings::connect(url, cred, cx) {
            Ok(u) => u,
            Err(e) => return self.set_connect_state(Some(e), false, cx),
        };
        let host = vm::url_host(&url).unwrap_or(url.clone());
        match crate::connection::go_live(&url, cx) {
            Ok(true) => {
                self.sheet.close();
                self.base_url = self.backend.base_url();
                self.refresh_problems(cx);
                self.focus_after_sheet(window, cx);
                self.initial_sync(Some(host), window, cx);
            }
            Ok(false) => {
                // Fake mode (or headless tests): nothing to switch.
                self.sheet.close();
                self.refresh_problems(cx);
                self.focus_after_sheet(window, cx);
                if self.backend.server_outdated() {
                    return self.server_outdated(cx);
                }
                self.show_toast(
                    format!("Connected to {host}"),
                    Some(
                        crate::keymap::hint(match cred {
                            Credential::Password(_) => "The password is in your Keychain",
                            Credential::Token(_) => "The token is in your Keychain",
                            Credential::OAuth(_) => "Your sign-in is in your Keychain",
                        })
                        .into(),
                    ),
                    cx,
                );
            }
            Err(e) => self.set_connect_state(
                Some(format!("Couldn't open the local database: {e}")),
                false,
                cx,
            ),
        }
        cx.notify();
    }

    fn on_disconnect(&mut self, _: &Disconnect, window: &mut Window, cx: &mut Context<Self>) {
        let Some(url) = crate::settings::blyg_url(cx) else {
            return self.show_toast("Not connected to a blyg", None, cx);
        };
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        self.sheet.open(DisconnectSheet {
            host: vm::url_host(&url).unwrap_or(url),
            focus,
        });
        cx.notify();
    }

    /// Forget the token and `blyg-url`; optionally delete the local copy.
    fn disconnect_now(&mut self, delete_local: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.sheet.close();
        match crate::connection::disconnect(delete_local, cx) {
            Ok(host) => {
                self.current = None;
                self.base_url = None;
                self.loading = false;
                self.unread = 0;
                self.refresh_problems(cx);
                self.requery(window, cx);
                self.show_toast(
                    format!("Disconnected from {host}"),
                    Some(if delete_local {
                        "The token and the local copy are gone".into()
                    } else {
                        crate::keymap::hint("The token is gone; your posts stay on this Mac").into()
                    }),
                    cx,
                );
                // --- blyg switcher --- the next signed-in blyg is active now;
                // with none left (or none that opens), connect one.
                if crate::settings::blyg_url(cx).is_some()
                    && crate::connection::mode(cx) != Some(crate::connection::Mode::Disconnected)
                {
                    self.blyg_switched(window, cx);
                } else {
                    self.open_connect(window, cx);
                }
            }
            Err(e) => self.show_toast(e, None, cx),
        }
        cx.notify();
    }

    // ------------------------------------------------------------ helpers

    // --- writing --- loading the editor: app/writing.rs

    fn requery(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.settle_search(window, cx);
        let results = self.backend.search(self.list.query());
        self.requery_with(results, None, window, cx);
    }

    /// `requery` for a change the sync worker made (`ItemsChanged`): the
    /// search, and the open item's fresh copy, are read on a background
    /// thread. A search typed meanwhile is searched again here, since the
    /// one under way may predate the change.
    fn requery_later(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.search.generation += 1;
        let generation = self.search.generation;
        let typed = self.search.pending.clone();
        let q = typed
            .clone()
            .unwrap_or_else(|| self.list.query().to_string());
        let current = self.current.as_ref().map(|c| c.local_id.clone());
        let edits = self.search.edits;
        let backend = self.backend.clone();
        let task = cx.background_spawn(async move {
            let results = backend.search(&q);
            let fresh = current.map(|id| {
                let item = backend.item(&id);
                (id, item)
            });
            (q, results, fresh)
        });
        self.search.task = Some(cx.spawn_in(window, async move |this, cx| {
            let (q, results, mut fresh) = task.await;
            let _ = this.update_in(cx, |v, window, cx| {
                if v.search.generation != generation {
                    return;
                }
                v.search.pending = None;
                v.search.task = None;
                // Typed into since it was read: read it again.
                if v.search.edits != edits {
                    fresh = None;
                }
                if typed.is_some() {
                    v.apply_query(&q, results, window, cx);
                } else {
                    v.requery_with(results, fresh, window, cx);
                }
            });
        }));
    }

    /// The rest of `requery`, given the search's results and a fresh copy
    /// of the open item (`None` inside: it's gone). One read for another
    /// item than the one open now is read again.
    fn requery_with(
        &mut self,
        results: Vec<Item>,
        fresh: Option<(LocalId, Option<Item>)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.reload_shelf(); // --- shelf ---
        self.upkeep_reload_stale(); // --- upkeep ---
        self.list.refresh(results);
        self.loose_ends_refresh(cx); // --- loose ends ---
        self.new_post_sync_selection(); // --- new post ---
        // Refresh the current item's metadata without clobbering the editor.
        if let Some(cur) = self.current.as_ref().map(|c| c.local_id.clone()) {
            let fresh = match fresh {
                Some((id, fresh)) if id == cur => fresh,
                _ => self.backend.item(&cur),
            };
            match fresh {
                Some(fresh) => {
                    let editor_text = self.editor.read(cx).value();
                    if fresh.content_md != editor_text.as_ref() && self.mode == Mode::Search {
                        self.current = Some(fresh);
                        self.load_current_into_editor(window, cx);
                    } else {
                        self.current = Some(fresh);
                    }
                }
                None => self.load_selected(window, cx),
            }
        } else if self.mode == Mode::Search {
            self.load_selected(window, cx);
        }
        cx.notify();
    }

    // --- writing --- opening and creating posts: app/writing.rs

    pub fn refresh_from_backend(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.requery(window, cx);
    }

    fn show_toast(
        &mut self,
        text: impl Into<SharedString>,
        sub: Option<SharedString>,
        cx: &mut Context<Self>,
    ) {
        self.toast_seq += 1;
        let id = self.toast_seq;
        let ms = if sub.is_some() { 3200 } else { 2600 };
        self.toast = Some(Toast {
            id,
            text: text.into(),
            sub,
            leaving: false,
        });
        cx.notify();
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(ms))
                .await;
            let _ = this.update(cx, |v, cx| {
                if let Some(t) = v.toast.as_mut().filter(|t| t.id == id) {
                    t.leaving = true;
                    cx.notify();
                }
            });
            cx.background_executor()
                .timer(Duration::from_millis(220))
                .await;
            let _ = this.update(cx, |v, cx| {
                if v.toast.as_ref().is_some_and(|t| t.id == id) {
                    v.toast = None;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn close_sheet(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.sheet.close();
        self.end_preview_from(pref_preview::Origin::Settings, window, cx); // --- preview ---
        self.focus_after_sheet(window, cx);
        cx.notify();
    }

    fn focus_after_sheet(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.mode == Mode::Edit && (self.current.is_some() || self.new_post.is_some()) {
            self.editor.update(cx, |s, cx| s.focus(window, cx));
        } else {
            self.omni.update(cx, |s, cx| s.focus(window, cx));
        }
    }

    /// Apply `prefs` to the UI and write the changed keys to the config file.
    fn apply_prefs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.drop_preview_overlay(); // --- preview ---
        self.apply_prefs_live(window, cx);
        self.save_prefs(cx);
    }

    fn apply_prefs_live(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.apply_theme(window, cx);
        crate::capture::prefs_changed(&self.prefs, cx);
        cx.notify();
    }

    /// Resolve the theme for the prefs and this window's appearance, and
    /// take its palette and suggested fonts.
    fn apply_theme(&mut self, window: &Window, cx: &mut Context<Self>) {
        // --- preview --- the window's appearance, or the slot being tried.
        let appearance = self.preview_appearance(window);
        let theme = crate::theme::resolve(&self.prefs, appearance, cx);
        self.prefs.adopt_theme_fonts(&theme);
        crate::fonts::ensure(self.prefs.writing().bundled, cx);
        crate::fonts::ensure(self.prefs.ui().bundled, cx);
        crate::fonts::ensure(theme.font_chrome.and_then(|f| f.bundled), cx);
        self.palette = theme.palette;
        crate::theme::set_reader_theme(&theme, self.prefs.writing());
        self.theme = theme;
    }

    // ------------------------------------------------------------ events

    fn on_omni_event(
        &mut self,
        _: &Entity<InputState>,
        ev: &InputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match ev {
            InputEvent::Change => self.query_typed(window, cx),
            InputEvent::Focus => {
                self.mode = Mode::Search;
                cx.notify();
            }
            InputEvent::PressEnter {
                secondary: true, ..
            } => {
                self.settle_search(window, cx);
                self.publish(window, cx)
            }
            InputEvent::PressEnter { .. } => match self.settle_search(window, cx).list.enter() {
                EnterAction::Open(id) => self.open(&id, window, cx),
                EnterAction::Create(seed) => self.create_from(&seed, window, cx),
                EnterAction::LinkFragment(url) => self.capture_link(url, window, cx), // --- capture ---
                // Not connected, nothing here, nothing typed: Enter connects
                // (the empty list says so).
                EnterAction::Nothing if self.offers_connect(cx) => self.open_connect(window, cx),
                EnterAction::Nothing => {}
            },
            InputEvent::Blur => {}
        }
    }

    // --- writing --- the editor's events: app/writing.rs

    /// A batch of queued core events (`coalesce_core_events`): one refresh
    /// for a burst of "something changed" events.
    pub(crate) fn on_core_events(
        &mut self,
        batch: Vec<CoreEvent>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let batch = coalesce_core_events(batch);
        // ReadingChanged refreshes the reading rows itself: ItemsChanged in
        // the same batch needn't (both re-read the store as it is now).
        let reading_follows = batch.iter().any(|e| matches!(e, CoreEvent::ReadingChanged));
        for ev in batch {
            self.handle_core_event(ev, reading_follows, window, cx);
        }
    }

    pub(crate) fn on_core_event(
        &mut self,
        ev: CoreEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.handle_core_event(ev, false, window, cx);
    }

    fn handle_core_event(
        &mut self,
        ev: CoreEvent,
        reading_follows: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if matches!(ev, CoreEvent::ItemsChanged | CoreEvent::ReadingChanged) {
            self.lineage_strip.stale(); // --- lineage ---
            self.nearby_store_changed(&ev, cx); // --- nearby ---
            // --- own posts --- the ⌘Y history's cached post and responses.
            if !(reading_follows && matches!(ev, CoreEvent::ItemsChanged)) {
                self.refresh_own_post(cx);
            }
        }
        match ev {
            CoreEvent::ItemsChanged => {
                // --- follow-ups --- a queued promotion uploaded local images.
                if let Some(id) = self.current.as_ref().map(|c| c.local_id.clone()) {
                    self.scratch_sync_editor(&id, window, cx);
                }
                // --- own posts --- your published posts are reading rows too.
                if self.reading.view == reading::View::Reading && !reading_follows {
                    self.reading_changed(cx);
                }
                self.requery_later(window, cx)
            }
            CoreEvent::SyncStatus(s) => {
                self.sync = s;
                cx.notify();
            }
            // --- blyg switcher --- a conflict queued by a blyg switched away
            // from just now names an item this blyg doesn't have.
            CoreEvent::Conflict { local_id, .. } if self.backend.item(&local_id).is_none() => {}
            CoreEvent::Conflict {
                local_id,
                mine,
                theirs,
            } => self.show_conflict_sheet(local_id, &mine, &theirs, window, cx),
            CoreEvent::ReadingChanged => {
                // Withdrawn posts are background: never unread (studio 0.49).
                self.unread = self.backend.unread_count();
                // --- reading & versions ---
                self.reading_changed(cx);
                cx.notify();
            }
            CoreEvent::Error(msg) => self.show_toast(msg, None, cx),
            CoreEvent::ServerOutdated => self.server_outdated(cx),
            // The "older than 0.40" sheet is gone: below the 0.47 floor the
            // outdated notice already says it all (`server_notice`).
            CoreEvent::ServerLimited => {}
            CoreEvent::SignedOut => {
                if !self.sheet.is_open() {
                    self.signed_out(window, cx);
                }
            }
        }
    }

    /// The blyg refused the stored credential (401): a revoked or expired
    /// token, an ended browser sign-in, or an owner token a blyg no longer
    /// takes. Sync has stopped (no retries) and changes wait in the outbox;
    /// ask for a new sign-in, browser first.
    fn signed_out(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.show_toast(
            "Your blyg signed Burrow out",
            Some(
                crate::keymap::hint("Sign in again to sync; your changes wait on this Mac").into(),
            ),
            cx,
        );
        self.open_connect(window, cx);
        if let Some(ConnectSheet { error, .. }) = self.sheet.get_mut()
            && error.is_none()
        {
            *error = Some(
                "Your blyg no longer accepts Burrow's saved sign-in. Sign in with the \
                 browser, or paste a new API token from Studio → Access, or use the \
                 Studio password. Changes made meanwhile are kept and sync after."
                    .into(),
            );
        }
    }

    // ------------------------------------------------------------ actions

    fn move_selection(&mut self, delta: isize, window: &mut Window, cx: &mut Context<Self>) {
        // ↑/↓ move through what was typed, even before its search is back.
        self.settle_search(window, cx);
        // --- new post --- ↑/↓ go on from the row selected before ⌘N.
        let resumed = self.new_post_resume_selection();
        if self.list.move_selection(delta).is_some() || resumed {
            if let Some(ix) = self.list.selected_index() {
                self.list_scroll.scroll_to_item(ix, ScrollStrategy::Nearest);
            }
            self.load_selected(window, cx);
        }
    }

    fn focus_search(&mut self, _: &FocusSearch, window: &mut Window, cx: &mut Context<Self>) {
        self.leave_reading(); // --- reading & versions ---
        self.back_to_search(window, cx);
    }

    fn back_to_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.mode = Mode::Search;
        self.omni.update(cx, |s, cx| {
            s.focus(window, cx);
            s.select_all(window, cx);
        });
        cx.notify();
    }

    // --- writing --- ⌘N, ⌘T, ⌘E (app/writing.rs) and publishing (app/publish.rs)

    fn open_settings(&mut self, _: &OpenSettings, window: &mut Window, cx: &mut Context<Self>) {
        if self.sheet.is::<SettingsSheet>() {
            self.close_sheet(window, cx);
            return;
        }
        let current = self.prefs.hotkey.clone();
        let hotkey = cx.new(|cx| {
            InputState::new(window, cx)
                .default_value(current)
                .placeholder("ctrl+alt+B")
        });
        let sub = cx.subscribe_in(&hotkey, window, |this, input, ev, window, cx| {
            if let InputEvent::PressEnter { .. } = ev {
                let text = input.read(cx).value().to_string();
                this.apply_hotkey(text, window, cx);
            }
        });
        self._subs.push(sub);
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        self.settings_preview_reset(window, cx); // --- preview ---
        self.sheet.open(SettingsSheet {
            hotkey,
            hotkey_error: None,
            focus,
            scroll: ScrollHandle::new(),
        });
        self.changelog_settings_refresh(cx); // --- reader view / changelog ---
        self.background_rules_refresh(cx); // --- background ---
        cx.notify();
    }

    fn apply_hotkey(&mut self, text: String, window: &mut Window, cx: &mut Context<Self>) {
        let clash = crate::keymap::clash_with_hotkey(&text).map(|k| {
            format!(
                "{} is already {} in Burrow",
                crate::keymap::glyphs(k.key),
                k.label
            )
        });
        let set = match clash {
            Some(e) => Err(e),
            None => crate::capture::set_hotkey(&text, cx),
        };
        let err = match set {
            Ok(()) => {
                self.prefs.hotkey = text.trim().to_string();
                self.apply_prefs(window, cx);
                self.show_toast(
                    format!(
                        "Quick capture is now {}",
                        prefs::hotkey_glyphs(&self.prefs.hotkey)
                    ),
                    None,
                    cx,
                );
                None
            }
            Err(e) => Some(e),
        };
        if let Some(SettingsSheet {
            hotkey_error,
            focus,
            ..
        }) = self.sheet.get_mut()
        {
            *hotkey_error = err;
            if hotkey_error.is_none() {
                window.focus(focus, cx);
            }
        }
        cx.notify();
    }

    fn font_bigger(&mut self, _: &FontBigger, window: &mut Window, cx: &mut Context<Self>) {
        self.drop_preview_overlay(); // --- preview ---
        self.prefs.bigger();
        self.apply_prefs(window, cx);
    }
    fn font_smaller(&mut self, _: &FontSmaller, window: &mut Window, cx: &mut Context<Self>) {
        self.drop_preview_overlay(); // --- preview ---
        self.prefs.smaller();
        self.apply_prefs(window, cx);
    }
    fn font_reset(&mut self, _: &FontReset, window: &mut Window, cx: &mut Context<Self>) {
        self.drop_preview_overlay(); // --- preview ---
        self.prefs.reset_size();
        self.apply_prefs(window, cx);
    }

    // --- writing --- conflicts (app/publish.rs), pasted links and images
    // (app/upload.rs), saving edits (app/writing.rs)

    /// Set the omnibar text programmatically (set_value emits no event).
    fn set_query_text(&mut self, q: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.omni
            .update(cx, |s, cx| s.set_value(q.to_string(), window, cx));
        self.query_changed(window, cx);
    }

    /// The omnibar's text was set: search now, on this thread.
    fn query_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let q = self.omni.read(cx).value().to_string();
        let typing = self.search.pending.take().is_some();
        if typing {
            // Whatever was typed before is superseded.
            self.search.generation += 1;
            self.search.task = None;
        }
        if q == self.list.query() && !typing {
            return;
        }
        let results = self.backend.search(&q);
        self.apply_query(&q, results, window, cx);
    }

    /// A keystroke in the omnibar: the search runs on a background thread
    /// (searching thousands of posts costs frames), and only the newest
    /// keystroke's results are listed. Until they're back the list shows
    /// the previous query's; ⏎, ↑/↓ and focusing the editor finish the
    /// search first (`settle_search`), so they act on what was typed.
    fn query_typed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let q = self.omni.read(cx).value().to_string();
        let waiting = self.search.pending.as_deref();
        if waiting == Some(q.as_str()) || (waiting.is_none() && q == self.list.query()) {
            return;
        }
        self.search.generation += 1;
        let generation = self.search.generation;
        self.search.pending = Some(q.clone());
        self.mode = Mode::Search;
        let backend = self.backend.clone();
        let task = cx.background_spawn(async move {
            let results = backend.search(&q);
            (q, results)
        });
        self.search.task = Some(cx.spawn_in(window, async move |this, cx| {
            let (q, results) = task.await;
            let _ = this.update_in(cx, |v, window, cx| {
                if v.search.generation == generation {
                    v.search.pending = None;
                    v.search.task = None;
                    v.apply_query(&q, results, window, cx);
                }
            });
        }));
        cx.notify();
    }

    /// A typed search still on its way: finish it now, on this thread.
    /// Returns `self`, for whatever reads the list next.
    fn settle_search(&mut self, window: &mut Window, cx: &mut Context<Self>) -> &mut Self {
        if let Some(q) = self.search.pending.take() {
            self.search.generation += 1;
            self.search.task = None;
            let results = self.backend.search(&q);
            self.apply_query(&q, results, window, cx);
        }
        self
    }

    /// List `results` for the query `q`: the selection jumps to the first.
    fn apply_query(
        &mut self,
        q: &str,
        results: Vec<Item>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.list.set_query(q, results);
        self.list_scroll.scroll_to_item(0, ScrollStrategy::Top);
        self.mode = Mode::Search;
        self.load_selected(window, cx);
    }
}

impl Focusable for MainView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

// ================================================================ rendering

/// The list's size along the layout axis: a width beside the editor
/// (`layout = side`) or a height above it (`layout = stacked`).
#[derive(Debug, Clone, Copy)]
enum ListLength {
    Width(Pixels),
    Height(Pixels),
}

pub(crate) const TITLEBAR_H: f32 = 34.;
const OMNI_H: f32 = 46.;
const STATUS_H: f32 = 30.;

/// A burst of queued core events, as one batch to handle: the "something
/// changed" events (`ItemsChanged`, `ReadingChanged`) and the sync status
/// are kept once, at their last place, since their handlers only re-read
/// (or keep the latest); every other event is kept, in order.
pub(crate) fn coalesce_core_events(batch: Vec<CoreEvent>) -> Vec<CoreEvent> {
    fn idempotent(e: &CoreEvent) -> Option<u8> {
        match e {
            CoreEvent::ItemsChanged => Some(0),
            CoreEvent::ReadingChanged => Some(1),
            CoreEvent::SyncStatus(_) => Some(2),
            _ => None,
        }
    }
    let mut last = [None::<usize>; 3];
    for (i, e) in batch.iter().enumerate() {
        if let Some(k) = idempotent(e) {
            last[k as usize] = Some(i);
        }
    }
    batch
        .into_iter()
        .enumerate()
        .filter(|(i, e)| idempotent(e).is_none_or(|k| last[k as usize] == Some(*i)))
        .map(|(_, e)| e)
        .collect()
}

pub(crate) fn shake_offset(t: f32) -> f32 {
    // keyframes: 25% -6, 50% +6, 75% -3, 100% 0
    let k = [
        (0.0, 0.0),
        (0.25, -6.0),
        (0.5, 6.0),
        (0.75, -3.0),
        (1.0, 0.0),
    ];
    for w in k.windows(2) {
        let ((t0, v0), (t1, v1)) = (w[0], w[1]);
        if t <= t1 {
            let f = if t1 > t0 { (t - t0) / (t1 - t0) } else { 0.0 };
            return v0 + (v1 - v0) * f;
        }
    }
    0.0
}

impl Render for MainView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(t0) = self.first_frame.take() {
            window.on_next_frame(move |_, _| {
                if crate::dev_flags::get().timing {
                    let epoch = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_millis())
                        .unwrap_or(0);
                    println!(
                        "first-frame {} ms after main(); epoch_ms={epoch}",
                        t0.elapsed().as_millis()
                    );
                }
            });
        }
        self.preview_frame(window, cx); // --- preview ---
        let p = self.palette;
        let size = self.prefs.font_size;
        let ui_font: SharedString = self.prefs.ui().family.into();
        let body_font: SharedString = self.prefs.writing().family.into();
        let onboarding = self.onboarding_frame(window, cx); // --- onboarding ---
        let profile_overlay = self.render_profile_overlay(cx); // --- profiles ---

        if self.native_title != self.title {
            window.set_window_title(&self.title);
            self.native_title = self.title.clone();
        }

        // --- full editor: which panes are on screen (⌘1/⌘2/⌘3/⌘E) ---
        self.studio_frame(window, cx);
        self.focus_frame(window, cx); // --- focus ---
        self.serendipity_frame(cx); // --- serendipity ---
        self.notes_frame(window, cx); // --- notes ---
        self.nearby.window_w = f32::from(window.viewport_size().width); // --- side ---
        self.nearby.dock_w = self.nearby_dock_w(cx); // --- side ---
        self.browser_frame(window, cx); // --- browser --- (after the drawers fit)
        self.site_frame(window, cx); // --- reader view --- the author's site
        self.tray_frame(cx); // --- tray ---
        self.split_frame(cx); // --- split ---
        // --- reading compose ---
        self.compose_frame(window, cx);
        // --- end reading compose ---
        let show_list = self.studio.view.list_visible();
        let show_preview = self.studio.view.preview_visible();
        let viewport = window.viewport_size();
        // --- side --- the docked drawers and the tab bar take the right.
        let content_w =
            (viewport.width - self.side_room_at(cx.background_executor().now())).max(px(0.));
        let stacked = self.prefs.layout == LayoutPref::Stacked;
        let list_w = (content_w * if show_preview { 0.24 } else { 0.38 }).round();
        let rest = if stacked || !show_list {
            content_w
        } else {
            content_w - list_w - px(1.)
        };
        let preview_beside = show_preview && self.reading.own.is_none() && !self.split.is_open();
        let pane_w = if preview_beside {
            // --- side --- the editor's share of the row (its divider drags)
            (rest * self.side_preview_share(rest)).floor()
        } else {
            rest
        };
        let pane_w = self.split_pane_w(pane_w); // --- split ---
        // --- end full editor ---
        let list_len = if stacked {
            ListLength::Height((viewport.height * 0.34).round())
        } else {
            ListLength::Width(list_w)
        };
        let measure = px(size * 36.0);
        let side_pad = ((pane_w - measure) / 2.).max(px(34.));
        let (top_pad, bottom_pad) = self.focus_pads(cx).unwrap_or((px(26.), px(40.))); // --- focus ---

        // Project the palette + measure onto the editor each frame (cheap: copies).
        let assist_font = self.prefs.ui().family;
        self.assist.update(cx, |a, _| {
            a.set_palette(p); // --- composer ---
            a.set_ui_font(assist_font);
        });
        self.editor.update(cx, |s, _| {
            s.set_editor_style(gpui_kit::base::input::InputEditorStyle {
                foreground: p.ink,
                muted_foreground: p.muted,
                background: p.editor,
                border: p.line,
                selection: p.text_selection,
                caret: p.accent,
                ..Default::default()
            });
            s.set_editor_paddings(Edges {
                top: top_pad,       // --- focus ---
                bottom: bottom_pad, // --- focus ---
                left: side_pad,
                right: side_pad,
            });
        });
        for input in [&self.omni].into_iter().chain(self.sheet.inputs()) {
            input.update(cx, |s, _| {
                s.set_editor_style(p.field());
            });
        }

        // --- buttons --- how the toolbar fits (None: show-buttons = false)
        let toolbar = self.toolbar_fit(window, cx).filter(|_| !self.focus_on()); // --- focus ---
        let show_title = toolbar.as_ref().is_none_or(|(fit, _)| fit.title());
        div()
            .id("blygger")
            .key_context(CONTEXT)
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::focus_search))
            .on_action(cx.listener(Self::on_publish))
            .on_action(cx.listener(Self::toggle_kind))
            .on_action(cx.listener(Self::toggle_preview))
            .on_action(cx.listener(Self::open_permalink))
            .on_action(cx.listener(Self::open_settings))
            .on_action(cx.listener(Self::font_bigger))
            .on_action(cx.listener(Self::font_smaller))
            .on_action(cx.listener(Self::font_reset))
            .on_action(cx.listener(Self::new_draft))
            .on_action(cx.listener(Self::fake_offline))
            .on_action(cx.listener(Self::fake_conflict))
            .on_action(cx.listener(Self::reload_config))
            .on_action(cx.listener(Self::open_config_file))
            .on_action(cx.listener(Self::on_disconnect))
            .on_action(cx.listener(Self::make_draft))
            .on_action(cx.listener(Self::ai_generate)) // --- AI ---
            .on_action(cx.listener(Self::ai_shorten)) // --- AI ---
            // --- full editor ---
            .on_action(cx.listener(Self::view_write))
            .on_action(cx.listener(Self::view_split))
            .on_action(cx.listener(Self::view_studio))
            .map(|d| self.reading_actions(d, cx)) // --- reading & versions ---
            .map(|d| self.onboarding_actions(d, cx)) // --- onboarding ---
            .map(|d| self.profile_actions(d, cx)) // --- profiles ---
            .map(|d| self.discard_actions(d, cx)) // --- delete & withdraw ---
            .map(|d| self.browser_actions(d, cx)) // --- browser ---
            .map(|d| self.notes_actions(d, cx)) // --- notes ---
            .map(|d| self.ext_actions(d, cx)) // --- extensions ---
            .map(|d| self.switcher_actions(d, cx)) // --- blyg switcher ---
            .map(|d| self.shelf_actions(d, cx)) // --- shelf ---
            .map(|d| self.access_actions(d, cx)) // --- access-ai ---
            .map(|d| self.server_ai_actions(d, cx)) // --- access-ai ---
            .map(|d| self.note_ai_actions(d, cx)) // --- access-ai ---
            .map(|d| self.contacts_actions(d, cx)) // --- contacts ---
            .map(|d| self.upkeep_actions(d, cx)) // --- upkeep ---
            .map(|d| self.syndication_actions(d, cx)) // --- syndication ---
            .map(|d| self.trails_actions(d, cx)) // --- trails ---
            .map(|d| self.tray_actions(d, cx)) // --- tray ---
            .map(|d| self.split_actions(d, cx)) // --- split ---
            .map(|d| self.palette_actions(d, cx)) // --- palette ---
            .map(|d| self.focus_actions(d, cx)) // --- focus ---
            .map(|d| self.nearby_actions(d, cx)) // --- nearby ---
            .map(|d| self.quiet_actions(d, cx)) // --- quiet ---
            .map(|d| self.site_actions(d, cx)) // --- site ---
            .size_full()
            .relative()
            .flex()
            .flex_col()
            .bg(p.bg)
            .text_color(p.ink)
            .font_family(ui_font.clone())
            // Windows shows the title in its own title bar.
            // --- blyg switcher --- the active blyg is the title (a button),
            // on Windows too.
            .child(
                self.render_titlebar(
                    &ui_font,
                    show_title && cfg!(target_os = "macos"),
                    show_title
                        .then(|| self.render_blyg_title(&ui_font, cx))
                        .flatten(),
                ),
            )
            .children(crate::platform::menu_button(p, cx)) // Windows menu
            .child(self.render_view_switcher(cx)) // --- reading & versions ---
            .children(self.render_toolbar(toolbar, &ui_font, cx)) // --- buttons ---
            // --- side --- the screen, with the docked drawers (notes,
            // Nearby) and their tab bar to its right.
            .map(|d| {
                let main = div()
                    .size_full()
                    .flex()
                    .flex_col()
                    .children(self.render_problems(cx))
                    // --- reading & versions: other screens replace omnibar + list + editor
                    .map(|d| match self.render_reading_body(&body_font, cx) {
                        Some(body) => d.child(body),
                        // --- focus --- (the omnibar only while ⌘L has it)
                        None => d
                            .when(!self.focus_hides_omnibar(), |d| {
                                d.child(self.render_omnibar(&ui_font, size, cx))
                            })
                            .child(
                                div()
                                    .flex_1()
                                    .min_h_0()
                                    .flex()
                                    .when(stacked, |d| d.flex_col())
                                    .when(!stacked, |d| d.flex_row())
                                    .when(show_list, |d| {
                                        d.child(self.render_list(list_len, &ui_font, cx))
                                    })
                                    .child({
                                        let editor =
                                            match self.render_own_versions(&body_font, size, cx) {
                                                Some(v) => v,
                                                // --- split --- the right post beside it
                                                None => self.split_wrap(
                                                    self.render_editor_pane(
                                                        &body_font, size, side_pad, cx,
                                                    )
                                                    .into_any_element(),
                                                    cx,
                                                ),
                                            };
                                        let row =
                                            div().flex_1().min_w_0().min_h_0().flex().flex_row();
                                        // --- full editor --- (not beside ⌘Y's history,
                                        // whose body is the reader's WebView); --- side ---
                                        // the divider between them drags.
                                        if preview_beside {
                                            let preview = self.studio_pane(&p);
                                            row.child(
                                                self.side_preview_row(editor, preview, rest, cx),
                                            )
                                        } else {
                                            row.child(editor)
                                        }
                                    }),
                            ),
                    })
                    .children(self.render_tray(cx)) // --- tray ---
                    .into_any_element();
                d.child(self.side_row(main, cx))
            })
            // --- focus --- a hairline in place of the status bar
            .map(|d| match self.render_focus_bar(cx) {
                Some(bar) => d.child(bar),
                None => d.child(self.render_status_bar(cx)),
            })
            .children(self.render_browser_pane(cx)) // --- browser ---
            .children(self.render_sheet(&ui_font, &body_font, cx))
            .children(self.render_ai_overlay(&ui_font, &body_font, cx)) // --- AI ---
            .children(self.render_ext_overlay(&ui_font, cx)) // --- extensions ---
            .children(self.render_cmd_palette(&ui_font, cx)) // --- palette ---
            .children(self.render_slots_sheet(&ui_font, cx)) // --- reading slots ---
            .children(self.render_reading_sheet(cx)) // --- reading & versions ---
            .children(self.render_trail_publish(cx)) // --- trails ---
            .children(self.render_site_overlay(window, cx)) // --- site ---
            .children(profile_overlay) // --- profiles ---
            .children(self.render_toast())
            .children(onboarding) // --- onboarding ---
            .children(crate::platform::menu_panel(p, window, cx)) // Windows menu
    }
}

impl MainView {
    fn render_titlebar(
        &self,
        ui_font: &SharedString,
        show_title: bool,
        blyg_title: Option<AnyElement>, // --- blyg switcher ---
    ) -> impl IntoElement {
        let p = self.palette;
        let plate = crate::ornament::title_plate(&self.theme);
        let connected = blyg_title.is_some(); // --- blyg switcher ---
        div()
            .id("titlebar")
            // On Windows the OS title bar moves the window, and a drag area
            // here would answer HTCAPTION for the toolbar and Menu buttons
            // drawn over it, so clicks on them would drag the window instead.
            .when(!cfg!(target_os = "windows"), |d| {
                d.window_control_area(WindowControlArea::Drag)
            })
            .h(px(TITLEBAR_H))
            .flex_none()
            .relative()
            .overflow_hidden()
            .flex()
            .items_center()
            .justify_center()
            .bg(p.bar)
            .border_b_1()
            .border_color(p.bar_line)
            .children(crate::ornament::titlebar_band(&self.theme))
            // --- buttons --- the toolbar hides the title when it needs the room
            // --- blyg switcher --- with a blyg, the title is the switcher.
            .when_some(blyg_title, |d, t| d.child(t))
            .when(show_title && !connected, |d| {
                d.child(
                    div()
                        .font_family(ui_font.clone())
                        .font_weight(FontWeight::MEDIUM)
                        .text_size(px(12.))
                        .text_color(p.bar_ink)
                        .when(plate, |d| d.px(px(8.)).rounded(px(3.)).bg(p.bar))
                        .child(self.title.clone()),
                )
            })
    }

    /// Config file problems, with line numbers, in a dismissible banner.
    fn render_problems(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.config_problems.is_empty() || self.problems_dismissed {
            return None;
        }
        let p = self.palette;
        let n = self.config_problems.len();
        let errors = self.config_problems.iter().any(|d| d.is_error());
        const SHOWN: usize = 4;
        let lines = self
            .config_problems
            .iter()
            .take(SHOWN)
            .map(|d| div().truncate().child(d.short()));
        Some(
            div()
                .id("config-problems")
                .flex_none()
                .flex()
                .gap(px(10.))
                .px(px(14.))
                .py(px(7.))
                .rule_b(&p)
                .bg(p.panel())
                .font_family(self.theme.chrome_font(&self.prefs))
                .text_size(px(11.5))
                .text_color(p.muted)
                .child(
                    div()
                        .flex_none()
                        .size(px(7.))
                        .mt(px(5.))
                        .rounded_full()
                        .bg(if errors { p.over } else { p.amber }),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap(px(1.))
                        .child(
                            div()
                                .text_color(p.ink)
                                .font_weight(FontWeight::MEDIUM)
                                .child(crate::keymap::hint_owned(format!(
                                    "The config file has {n} problem{} · ⌘⇧, reloads it after you fix {}",
                                    if n == 1 { "" } else { "s" },
                                    if n == 1 { "it" } else { "them" }
                                ))),
                        )
                        .children(lines)
                        .when(n > SHOWN, |d| {
                            d.child(format!(
                                "…and {} more (blygger +validate-config lists them all)",
                                n - SHOWN
                            ))
                        }),
                )
                .child(
                    div()
                        .id("open-config")
                        .flex_none()
                        .cursor_pointer()
                        .hover(|s| s.text_color(p.ink))
                        .on_click(cx.listener(|this, _, _, cx| this.open_config_file_now(cx)))
                        .child("Open"),
                )
                .child(
                    div()
                        .id("dismiss-problems")
                        .flex_none()
                        .cursor_pointer()
                        .hover(|s| s.text_color(p.ink))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.problems_dismissed = true;
                            cx.notify();
                        }))
                        .child("✕"),
                )
                .into_any_element(),
        )
    }

    fn render_omnibar(
        &self,
        ui_font: &SharedString,
        size: f32,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let p = self.palette;
        let hint = if self.mode == Mode::Search {
            self.list.hint()
        } else {
            String::new()
        };
        div()
            .id("omnibar")
            .map(|d| self.omnibar_quote_drop(d, cx)) // --- tray ---
            .h(px(OMNI_H))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(8.))
            .px(px(14.))
            .rule_b(&p)
            .child(div().text_size(px(16.)).text_color(p.muted).child("⌕"))
            .child(
                div()
                    .flex_1()
                    .font_family(ui_font.clone())
                    .text_size(px(size - 1.))
                    .capture_action(cx.listener(|this, _: &MoveUp, window, cx| {
                        cx.stop_propagation();
                        this.move_selection(-1, window, cx);
                    }))
                    .capture_action(cx.listener(|this, _: &MoveDown, window, cx| {
                        cx.stop_propagation();
                        this.move_selection(1, window, cx);
                    }))
                    .capture_action(cx.listener(|this, _: &Escape, window, cx| {
                        cx.stop_propagation();
                        this.set_query_text("", window, cx);
                    }))
                    .child(gpui_kit::base::input::Input::new(&self.omni)),
            )
            .when(!hint.is_empty(), |d| {
                d.child(
                    div()
                        .flex_none()
                        .font_family(self.theme.chrome_font(&self.prefs))
                        .text_size(px(12.))
                        .text_color(p.muted)
                        .child(hint),
                )
            })
    }

    fn render_list(
        &self,
        len: ListLength,
        ui_font: &SharedString,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let p = self.palette.on_side();
        let lsize = self.prefs.list_size();
        let count = self.list.results().len();
        // --- themes --- the sidebar's ground, then the rows, then the mist.
        let base = div()
            .flex_none()
            .relative()
            .overflow_hidden()
            .bg(p.bg)
            .text_color(p.ink)
            .border_color(self.palette.side_line)
            .font_family(ui_font.clone())
            .children(crate::ornament::sidebar_ground(&self.theme));
        let base = match len {
            ListLength::Width(w) => base.w(w).h_full().border_r_1(),
            ListLength::Height(h) => base.h(h).w_full().border_b_1(),
        };
        if count == 0 && self.offers_connect(cx) {
            // Setup was skipped: say how to connect, or try sample data.
            return base.child(
                div()
                    .relative()
                    .p(px(14.))
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .children(crate::ornament::empty_art(&self.theme))
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .gap(px(6.))
                            .text_size(px(lsize))
                            .text_color(p.muted)
                            .child(
                                div()
                                    .id("empty-connect")
                                    .debug_selector(|| "empty-connect".into())
                                    .cursor_pointer()
                                    .text_color(p.ink)
                                    .hover(|s| s.underline())
                                    .child(crate::keymap::hint(EMPTY_CONNECT))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.open_connect(window, cx)
                                    })),
                            )
                            .child("·")
                            .child(
                                div()
                                    .id("empty-sample")
                                    .debug_selector(|| "empty-sample".into())
                                    .cursor_pointer()
                                    .hover(|s| s.underline())
                                    .child(EMPTY_SAMPLE)
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.onboarding_sample_data(window, cx)
                                    })),
                            ),
                    ),
            );
        }
        if count == 0 {
            let q = self.list.trimmed_query().to_string();
            // --- loose ends --- / --- capture --- operators and addresses.
            let msg = self
                .list
                .empty_message()
                .unwrap_or_else(|| "Nothing here yet. Type to start writing.".to_string());
            return base.child(
                div()
                    .relative()
                    .p(px(14.))
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .when(q.is_empty(), |d| {
                        d.children(crate::ornament::empty_art(&self.theme))
                    })
                    .child(
                        div()
                            .italic()
                            .text_size(px(lsize))
                            .text_color(p.muted)
                            .child(msg),
                    ),
            );
        }
        // --- loose ends --- the summary in words, over the rows.
        base.flex()
            .flex_col()
            .children(self.render_loose_ends(cx))
            .child(
                div().w_full().flex_1().min_h_0().child(
                    // --- delight --- the list crossfades on a blyg switch.
                    self.delight_list(
                        uniform_list(
                            "items",
                            count,
                            cx.processor(
                                move |this, range: std::ops::Range<usize>, _window, cx| {
                                    range
                                        .map(|ix| this.render_row(ix, lsize, cx))
                                        .collect::<Vec<_>>()
                                },
                            ),
                        )
                        .track_scroll(&self.list_scroll)
                        .size_full()
                        .into_any_element(),
                    ),
                ),
            )
            .children(crate::ornament::scroll_edge(&self.theme))
    }

    fn render_row(&self, ix: usize, lsize: f32, cx: &mut Context<Self>) -> AnyElement {
        let item = &self.list.results()[ix];
        let selected = self.list.selected() == Some(&item.local_id);
        // --- themes --- sidebar colours; the selected row's own.
        let p = if selected {
            self.palette.on_sel()
        } else {
            self.palette.on_side()
        };
        let chrome = self.theme.chrome_font(&self.prefs);
        let title: SharedString = vm::item_title(item).into();
        let highlights: Vec<_> = vm::highlight_ranges(&title, self.list.query())
            .into_iter()
            .map(|r| {
                (
                    r,
                    HighlightStyle {
                        color: Some(p.accent),
                        font_weight: Some(FontWeight::SEMIBOLD),
                        ..Default::default()
                    },
                )
            })
            .collect();
        let (pill_text, pill_pub) = vm::pill(item);
        // Once per row: the label scan reads the whole post.
        let held = self.labels_held(&item.content_md);
        let pub_tip: Option<SharedString> = pill_pub.then(|| vm::published_tip(item).into());
        let id = item.local_id.clone();
        let row = div().id(("row", ix)).py(px(6.)).text_color(p.ink);
        // --- shelf --- a hairline under the last shelved row (absolute, so
        // every row keeps the one height `uniform_list` needs).
        let shelf_end = ix + 1 == self.list.shelf_len() && ix + 1 < self.list.results().len();
        let row = row.when(shelf_end, |d| {
            d.relative().child(
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .bottom_0()
                    .h(px(1.))
                    .bg(self.palette.side_line),
            )
        });
        let settle_id = item.local_id.clone(); // --- delight ---
        let el = crate::ornament::row(&self.theme, row, selected)
            .on_click(cx.listener(move |this, _, window, cx| this.open(&id, window, cx)))
            .child(
                div()
                    .text_size(px(lsize))
                    .line_height(relative(1.35))
                    .flex()
                    .items_center()
                    .gap(px(5.))
                    // --- shelf --- a post kept to hand.
                    .when(self.list.is_on_shelf(&item.local_id), |d| {
                        d.child(
                            div()
                                .id(("row-shelf", ix))
                                .flex_none()
                                .text_size(px(lsize * 0.8))
                                .tooltip(|_, cx| {
                                    cx.new(|_| {
                                        reading::Tip(
                                            crate::keymap::hint(
                                                "On your shelf · ⌘B opens it · ⌥⌘B takes it off",
                                            )
                                            .into(),
                                        )
                                    })
                                    .into()
                                })
                                .child(shelf::GLYPH),
                        )
                    })
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .truncate()
                            .child(StyledText::new(title).with_highlights(highlights)),
                    ),
            )
            .child(
                div()
                    .mt(px(2.))
                    .flex()
                    .items_center()
                    .gap(px(7.))
                    .font_family(chrome)
                    .text_size(px(11.))
                    .text_color(p.muted)
                    // Unpublished edits in their own colour (`color-edited`)
                    // and slot (`marker.edited`, Fortress's Δ), so they don't
                    // read as unread (`marker.new`, the accent dot).
                    .when_some(vm::edits_tip(item), |d, tip| {
                        let tip = format!("Unpublished edits: {tip}");
                        d.child(
                            div()
                                .id(("row-edited", ix))
                                .tooltip(move |_, cx| cx.new(|_| reading::Tip(tip.clone())).into())
                                .child(
                                    crate::ornament::marker(
                                        &self.theme,
                                        blyg_core::config::theme::Slot::MarkerEdited,
                                        p.edited,
                                        chrome,
                                    )
                                    .unwrap_or_else(|| {
                                        div()
                                            .size(px(6.))
                                            .rounded_full()
                                            .bg(p.edited)
                                            .into_any_element()
                                    }),
                                ),
                        )
                    })
                    .child(vm::kind_label(item.kind))
                    .children(self.upkeep_row_marker(item, ix, p.muted)) // --- upkeep ---
                    .child(
                        div()
                            .id(("row-pill", ix))
                            .px(px(6.))
                            .rounded(px(self.theme.chip_radius))
                            .border_1()
                            .line_height(px(15.))
                            .border_color(if pill_pub { p.accent } else { p.line })
                            .when_some(pub_tip.clone().filter(|_| !held), |d, tip| {
                                d.text_color(p.accent).tooltip(move |_, cx| {
                                    cx.new(|_| reading::Tip(tip.to_string())).into()
                                })
                            })
                            .when(pill_pub, |d| d.text_color(p.accent))
                            .child(pill_text)
                            // Below studio 0.47: this post waits for its labels.
                            .when(held, |d| {
                                let (bg, ink) = self.palette.tip();
                                d.tooltip(move |_, cx| {
                                    cx.new(|_| toolbar::ButtonTip {
                                        text: crate::keymap::hint(server_notice::HELD_TIP).into(),
                                        reason: None,
                                        bg,
                                        ink,
                                        font: chrome,
                                    })
                                    .into()
                                })
                            }),
                    )
                    .when_some(pub_tip, |d, tip| {
                        d.children(
                            crate::ornament::marker(
                                &self.theme,
                                blyg_core::config::theme::Slot::MarkerPinned,
                                p.accent,
                                chrome,
                            )
                            .map(|m| {
                                div()
                                    .id(("row-published", ix))
                                    .tooltip(move |_, cx| {
                                        cx.new(|_| reading::Tip(tip.to_string())).into()
                                    })
                                    .child(m)
                            }),
                        )
                    })
                    .child(vm::relative_time(&item.updated, self.now)),
            )
            .map(|cell| crate::ornament::row_wrap(&self.theme, cell));
        self.delight_row(el, &settle_id) // --- delight ---
    }

    // --- writing --- the editor pane (app/writing.rs), the conflict sheet
    // (app/publish.rs)

    fn render_status_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.palette.on_status();
        let chrome = self.theme.chrome_font(&self.prefs);
        let disconnected =
            crate::connection::mode(cx) == Some(crate::connection::Mode::Disconnected);
        let (sync_text, dot) = if disconnected {
            ("not connected".to_string(), vm::SyncDot::Grey)
        } else if self.loading && !self.publishing {
            ("loading your posts…".to_string(), vm::SyncDot::Busy)
        } else {
            vm::sync_label(self.sync, self.publishing)
        };
        let dot_color = match dot {
            vm::SyncDot::Green => p.green,
            vm::SyncDot::Amber => p.amber,
            vm::SyncDot::Busy => p.accent,
            vm::SyncDot::Grey => p.grey,
            vm::SyncDot::Red => p.over,
        };
        // --- themes --- the dot is a mark on the status bar's own ground.
        let dot_color =
            crate::theme_ext::readable(dot_color, self.palette.status, crate::theme_ext::MARK);
        // --- follow-ups --- Reading / Mentions / Subscriptions show their
        // own status, not the current post's.
        let screen = reading::vm::screen_status(self.reading.view, self.unread);
        let item = self.current.as_ref().filter(|_| screen.post);
        let chars = item.map(|i| i.char_count()).unwrap_or(0);
        let kind = item.map(|i| i.kind).unwrap_or(Kind::Fragment);
        let (count, level) = vm::counter(kind, chars);
        let banner = vm::banner(kind, chars);

        let dot_el = div().size(px(7.)).rounded_full().bg(dot_color);
        let dot_el = if dot == vm::SyncDot::Busy {
            dot_el
                .with_animation(
                    "pulse",
                    Animation::new(Duration::from_millis(1400)).repeat(),
                    |d, t| {
                        let tri = 1.0 - (2.0 * t - 1.0).abs();
                        d.opacity(1.0 - 0.75 * tri)
                    },
                )
                .into_any_element()
        } else {
            self.delight_dot(dot_el, dot_color) // --- delight ---
        };

        let bar = div()
            .id("status")
            .h(px(STATUS_H))
            .flex_none()
            .relative()
            .flex()
            .items_center()
            .gap(px(12.))
            .px(px(14.))
            .border_t_1()
            .border_color(self.palette.status_line)
            .bg(self.palette.status)
            .font_family(chrome)
            .text_size(px(11.5))
            .text_color(p.muted)
            .when_some(item, |d, item| {
                d.child(
                    div()
                        .id("kind")
                        .cursor_pointer()
                        .hover(|s| s.text_color(p.ink))
                        .on_click(
                            cx.listener(|this, _, window, cx| this.toggle_kind_now(window, cx)),
                        )
                        .child(vm::kind_label(item.kind)),
                )
                // --- delight --- the counter's colour eases.
                .child(
                    self.delight_counter(
                        div()
                            .when(level == vm::Level::Over, |d| {
                                d.font_weight(FontWeight::SEMIBOLD)
                            })
                            .child(count),
                        match level {
                            vm::Level::Warn => self.palette.on_status_text(p.warn),
                            vm::Level::Over => self.palette.on_status_text(p.over),
                            _ => p.muted,
                        },
                    ),
                )
            })
            // --- full editor ---
            .children(self.studio_status(&p).filter(|_| screen.post))
            .child(
                div()
                    .flex_1()
                    .flex()
                    .justify_center()
                    .text_color(self.palette.on_status_text(p.over))
                    .children(banner),
            )
            .children(screen.to_read.map(|t| div().id("to-read").child(t)))
            .children(self.render_server_notice())
            .children(self.render_ext_notice(cx)) // --- extensions ---
            .children(self.render_macro_status(cx)) // --- browser macros ---
            .children(self.render_update_notice(cx)) // --- auto-update ---
            .children(self.render_ai_status()) // --- AI ---
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(dot_el)
                    .child(sync_text)
                    // Not connected: a click opens Connect.
                    .id("sync-status")
                    .when(disconnected, |d| {
                        d.debug_selector(|| "sync-status-connect".into())
                            .cursor_pointer()
                            .hover(|s| s.text_color(p.ink).underline())
                            .tooltip(|_, cx| {
                                cx.new(|_| reading::Tip("Connect your blyg…".into())).into()
                            })
                            .on_click(cx.listener(|this, _, window, cx| {
                                if !this.sheet.is_open() {
                                    this.open_connect(window, cx)
                                }
                            }))
                    }),
            )
            // --- conflicts --- an item still in conflict says so, and a
            // click shows the resolve sheet again (issue #52).
            .children(item.filter(|i| i.conflict).map(|i| {
                let id = i.local_id.clone();
                div()
                    .id("conflict-marker")
                    .text_color(p.over)
                    .cursor_pointer()
                    .hover(|d| d.underline())
                    .child("sync conflict · resolve…")
                    .tooltip(|_, cx| {
                        cx.new(|_| {
                            reading::Tip(
                                "The server's copy changed while you edited. Click to choose \
                                 which to keep; publishing waits until then."
                                    .into(),
                            )
                        })
                        .into()
                    })
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.reopen_conflict(&id, window, cx);
                    }))
                    .into_any_element()
            }))
            // --- new post --- "New note · saved locally · …" while in it.
            .children(match self.new_post_label() {
                Some(l) => Some(div().child(l.to_string()).into_any_element()),
                // "public vN · unpublished edits" says how to publish or
                // discard them on hover.
                None => item.map(|item| {
                    let tip = vm::edits_tip(item);
                    div()
                        .id("version")
                        .child(vm::version_label(item))
                        .when_some(tip, |d, tip| {
                            d.tooltip(move |_, cx| cx.new(|_| reading::Tip(tip.clone())).into())
                        })
                        .into_any_element()
                }),
            })
            // --- themes ---
            .children(crate::ornament::status(
                &self.theme,
                self.status_data(),
                chrome,
            ));

        if self.shake_gen > 0 {
            bar.with_animation(
                ("shake", self.shake_gen),
                Animation::new(Duration::from_millis(350)),
                |d, t| d.left(px(shake_offset(t))),
            )
            .into_any_element()
        } else {
            bar.into_any_element()
        }
    }

    /// --- themes --- What the status ornaments show: posts touched per day
    /// over the last two weeks (the tide and block sparklines).
    fn status_data(&self) -> crate::ornament::StatusData {
        if !self
            .theme
            .has(blyg_core::config::theme::Slot::StatusOrnament)
        {
            return crate::ornament::StatusData {
                activity: Vec::new(),
            };
        }
        let mut activity = vec![0u32; 14];
        for item in self.list.results() {
            if let Ok(t) = chrono::DateTime::parse_from_rfc3339(&item.updated) {
                let days = (self.now - t.with_timezone(&chrono::Utc)).num_days();
                if (0..14).contains(&days) {
                    activity[13 - days as usize] += 1;
                }
            }
        }
        crate::ornament::StatusData { activity }
    }

    fn render_toast(&self) -> Option<AnyElement> {
        let t = self.toast.as_ref()?;
        let p = self.palette;
        let leaving = t.leaving;
        let el = div()
            .absolute()
            // --- full editor: clear of the native preview ---
            .right(self.studio_toast_right())
            .bottom(px(STATUS_H + 12.))
            .max_w(px(420.))
            .px(px(12.))
            .py(px(9.))
            .rounded(px(self.theme.toast_radius))
            .bg(p.toast)
            .text_color(p.toast_ink)
            .map(|d| crate::ornament::border(d, self.theme.toast_border, p.toast_ink.opacity(0.4)))
            .font_family(self.theme.chrome_font(&self.prefs))
            .text_size(px(12.5))
            .shadow_md()
            .child(t.text.clone())
            .when_some(t.sub.clone(), |d, sub| {
                d.child(div().opacity(0.7).child(sub))
            });
        Some(
            el.with_animation(
                ElementId::NamedInteger("toast".into(), (t.id * 2 + leaving as usize) as u64),
                Animation::new(Duration::from_millis(200)).with_easing(ease_out_quint()),
                move |d, t| {
                    let v = if leaving { 1.0 - t } else { t };
                    d.opacity(v).mb(px(-8.0 * (1.0 - v)))
                },
            )
            .into_any_element(),
        )
    }
}

#[cfg(test)]
mod coalesce_tests {
    use super::{CoreEvent, coalesce_core_events};

    fn names(b: &[CoreEvent]) -> Vec<String> {
        b.iter()
            .map(|e| match e {
                CoreEvent::Error(m) => format!("Error({m})"),
                CoreEvent::SyncStatus(_) => "SyncStatus".into(),
                e => format!("{e:?}"),
            })
            .collect()
    }

    #[test]
    fn a_burst_of_reading_changed_is_one() {
        let out = coalesce_core_events(vec![CoreEvent::ReadingChanged; 50]);
        assert_eq!(names(&out), ["ReadingChanged"]);
    }

    #[test]
    fn events_with_data_keep_their_order() {
        let out = coalesce_core_events(vec![
            CoreEvent::ReadingChanged,
            CoreEvent::Error("a".into()),
            CoreEvent::ItemsChanged,
            CoreEvent::ReadingChanged,
            CoreEvent::Error("b".into()),
            CoreEvent::ItemsChanged,
        ]);
        assert_eq!(
            names(&out),
            ["Error(a)", "ReadingChanged", "Error(b)", "ItemsChanged"]
        );
    }
}
