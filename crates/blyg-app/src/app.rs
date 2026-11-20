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

/// --- sheets --- one main-window sheet at a time; each kind in `sheet/`.
#[path = "sheet/mod.rs"]
pub(crate) mod sheet;
use settings_page::SettingsSheet;
use sheet::conflict::ConflictSheet;
pub(crate) use sheet::connect::{CancelOnDrop, ConnectSheet, ConnectWay, DisconnectSheet};
use sheet::publish::{HeldLabelsSheet, PublishSheet};

// The main window's features, each a child module extending `MainView`.
mod chrome; // title bar, config-problems banner, status bar, toasts
mod connect; // connecting and disconnecting a blyg
mod looks; // config file, prefs, theme, fonts, Settings' hotkey
#[cfg(test)]
mod size_cap; // app.rs's line cap

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
    /// The config file as last read (`app/looks.rs`).
    config: looks::ConfigFile,
    /// `Burrow — <host>` from the config's `blyg-url`.
    title: String,
    /// The title last given to the native window (Window menu, Mission Control).
    native_title: String,
    palette: Palette,
    /// The resolved theme (`palette` is `theme.palette`), for ornaments.
    theme: Arc<Theme>,
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
        let config = looks::ConfigFile::new(cx);
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
            config,
            title,
            native_title: String::new(),
            palette,
            theme,
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

    // ------------------------------------------------------------ helpers

    fn set_editor_text(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.loading_editor = true;
        self.editor
            .update(cx, |s, cx| s.set_value(text.to_string(), window, cx));
        self.loading_editor = false;
        self.assist.update(cx, |a, cx| a.reset(cx)); // --- composer ---
    }

    /// Load the selected row into the editor (NV preview-as-you-move).
    fn load_selected(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.leave_new_post(window, cx); // --- new post ---
        let q = self.list.trimmed_query().to_string();
        if self.list.wants_create() {
            self.current = None;
            self.set_editor_text("", window, cx);
            let ph = format!(
                "⏎ creates a new {}: “{q}”",
                scratch::new_note_noun(self.prefs.new_note)
            );
            self.editor
                .update(cx, |s, cx| s.set_placeholder(ph, window, cx));
        } else {
            self.current = self.list.selected_item().cloned();
            let text = self
                .current
                .as_ref()
                .map(|i| i.content_md.clone())
                .unwrap_or_default();
            self.set_editor_text(&text, window, cx);
            self.editor
                .update(cx, |s, cx| s.set_placeholder("", window, cx));
        }
        self.refresh_preview(true, cx);
        cx.notify();
    }

    /// Re-render the preview (after a typing pause unless `now`).
    fn refresh_preview(&mut self, now: bool, cx: &mut Context<Self>) {
        self.studio_refresh(now, cx);
    }

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

    fn load_current_into_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self
            .current
            .as_ref()
            .map(|i| i.content_md.clone())
            .unwrap_or_default();
        self.set_editor_text(&text, window, cx);
        self.refresh_preview(true, cx);
    }

    fn open(&mut self, id: &LocalId, window: &mut Window, cx: &mut Context<Self>) {
        // A search typed just before lands first, not over the open post.
        self.settle_search(window, cx);
        // --- new post --- opening another post leaves the new one.
        if self
            .new_post
            .as_ref()
            .is_some_and(|np| np.id.as_ref() != Some(id))
        {
            self.leave_new_post(window, cx);
        }
        self.list.select(id);
        if self.current.as_ref().map(|c| &c.local_id) != Some(id) {
            self.current = self.backend.item(id);
            self.load_current_into_editor(window, cx);
        }
        self.mode = Mode::Edit;
        self.editor.update(cx, |s, cx| {
            s.focus(window, cx);
            let end = s.text().len();
            s.set_selected_range(end..end, cx);
        });
        cx.notify();
    }

    fn create_from(&mut self, seed: &str, window: &mut Window, cx: &mut Context<Self>) {
        match scratch::create_note(&*self.backend, self.prefs.new_note, seed) {
            Ok((id, created)) => {
                self.set_query_text("", window, cx);
                let results = self.backend.search("");
                self.list.set_query("", results);
                self.list.select(&id);
                self.current = None;
                self.open(&id, window, cx);
                self.list_scroll.scroll_to_item(0, ScrollStrategy::Top);
                self.show_toast(created, None, cx);
            }
            Err(e) => self.show_toast(
                format!(
                    "Couldn't create a {}: {e}",
                    scratch::new_note_noun(self.prefs.new_note)
                ),
                None,
                cx,
            ),
        }
    }

    pub fn refresh_from_backend(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.requery(window, cx);
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

    fn on_editor_event(
        &mut self,
        _: &Entity<TextareaState>,
        ev: &InputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match ev {
            InputEvent::Focus => {
                if self.mode == Mode::Search && self.settle_search(window, cx).list.wants_create() {
                    let seed = self.list.trimmed_query().to_string();
                    self.create_from(&seed, window, cx);
                } else {
                    self.mode = Mode::Edit;
                    cx.notify();
                }
            }
            InputEvent::Change if !self.loading_editor => {
                // --- new post --- the first keystroke creates the note.
                self.new_post_first_keystroke(cx);
                // Typing `![[` at the start of a line opens the quote picker,
                // and `[[` the link picker.
                let typed = self.typed_picker(cx);
                self.after_edit(cx);
                self.nearby_typed(cx); // --- nearby ---
                if let Some((kind, typed)) = typed {
                    self.open_picker_from_typing(kind, typed, window, cx);
                }
            }
            _ => {}
        }
    }

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

    /// ⌘N: a fresh, empty editor (`new_post.rs`). The omnibar and the list
    /// are left as they are.
    fn new_draft(&mut self, _: &NewDraft, window: &mut Window, cx: &mut Context<Self>) {
        self.start_new_post(window, cx);
    }

    fn on_publish(&mut self, _: &Publish, window: &mut Window, cx: &mut Context<Self>) {
        // Under a sheet, ⌘⏎ is the sheet's own (`SheetView::submit`): it
        // never publishes.
        if !self.sheet.is_open() {
            self.publish(window, cx);
        } else if let Some(submit) = self.sheet.submit() {
            submit(self, window, cx);
        }
    }

    fn publish(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(item) = self.current.clone() else {
            return;
        };
        // --- AI --- warn before publishing generated text undisclosed.
        if self.ai_publish_guard(&item, window, cx) {
            return;
        }
        match vm::publish_decision(&item) {
            PublishDecision::Shake => {
                self.shake_gen += 1;
                self.show_toast(
                    crate::keymap::hint(
                        "Too long to publish as a fragment. Press ⌘T to make it a thread.",
                    ),
                    None,
                    cx,
                );
            }
            PublishDecision::Empty => self.show_toast("Nothing to publish yet", None, cx),
            PublishDecision::AlreadyPublished(v) => {
                self.show_toast(format!("Already published as v{v}"), None, cx)
            }
            // Below studio 0.47 a post with labelled links can't go as it is.
            PublishDecision::Sheet { .. } if self.labels_held(&item.content_md) => {
                let focus = cx.focus_handle();
                window.focus(&focus, cx);
                self.sheet.open(HeldLabelsSheet {
                    id: item.local_id,
                    focus,
                });
                cx.notify();
            }
            PublishDecision::Sheet {
                title,
                next_version,
            } => {
                let note = cx.new(|cx| {
                    InputState::new(window, cx)
                        .placeholder("Version note (optional), e.g. “fixed a typo”")
                });
                let sub = cx.subscribe_in(&note, window, |this, note, ev, window, cx| {
                    if let InputEvent::PressEnter { .. } = ev {
                        let text = note.read(cx).value().to_string();
                        this.do_publish(text, window, cx);
                    }
                });
                self._subs.push(sub);
                note.update(cx, |s, cx| s.focus(window, cx));
                self.sheet.open(PublishSheet {
                    id: item.local_id,
                    title,
                    next_version,
                    note,
                    ai: Default::default(), // --- access-ai ---
                });
                self.note_ai_start(next_version, window, cx); // --- access-ai ---
                cx.notify();
            }
        }
    }

    /// The blyg is below studio 0.47 and `text` has labelled links: it
    /// waits (`blyg_core::floor`).
    pub(crate) fn labels_held(&self, text: &str) -> bool {
        self.backend.server_outdated() && blyg_core::floor::has_labels(text)
    }

    /// The held-labels sheet's `⏎`: write the open post's labelled links
    /// bare (`[[id]]`, `![[id]]`), then publish it.
    fn publish_without_labels(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(HeldLabelsSheet { id, .. }) = self.sheet.take() else {
            return;
        };
        if self.current.as_ref().map(|c| &c.local_id) == Some(&id) {
            let text = self.editor.read(cx).value().to_string();
            let bare = blyg_core::floor::strip_labels(&text);
            if bare != text {
                self.splice_editor(&text, &bare, None, window, cx);
            }
        }
        self.focus_after_sheet(window, cx);
        self.start_publish(id, None, window, cx);
    }

    fn do_publish(&mut self, note: String, window: &mut Window, cx: &mut Context<Self>) {
        let Some(PublishSheet { id, ai, .. }) = self.sheet.take() else {
            return;
        };
        self.focus_after_sheet(window, cx);
        let extras = ai.extras(&note); // --- access-ai ---
        let note_opt = Some(note.trim().to_string()).filter(|n| !n.is_empty());
        self.start_publish_with(id, note_opt, extras, window, cx);
    }

    fn start_publish(
        &mut self,
        id: LocalId,
        note_opt: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.start_publish_with(id, note_opt, Default::default(), window, cx);
    }

    /// --- access-ai --- `start_publish` with the note's provenance and the
    /// confirmed change category.
    fn start_publish_with(
        &mut self,
        id: LocalId,
        note_opt: Option<String>,
        extras: blyg_core::access_ai::PublishExtras,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.publishing = true;
        cx.notify();
        let backend = self.backend.clone();
        let task = cx.background_spawn({
            let id = id.clone();
            let note = note_opt.clone();
            async move {
                if extras == Default::default() {
                    scratch::publish_via(&*backend, &id, note.as_deref())
                } else {
                    backend.publish_with(&id, note.as_deref(), &extras) // --- access-ai ---
                }
            }
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |v, window, cx| {
                v.publishing = false;
                if crate::dev_flags::get().timing {
                    match &result {
                        Ok(out) => println!("publish ok v{} {}", out.version, out.permalink),
                        Err(e) => println!("publish failed: {e}"),
                    }
                }
                match result {
                    Ok(out) => {
                        v.ext_item_published(&id, &out, note_opt.as_deref()); // --- extensions ---
                        v.delight_published(&id, cx); // --- delight ---
                        let head = match &note_opt {
                            Some(n) => format!("Published v{} · “{n}”", out.version),
                            None => format!("Published v{}", out.version),
                        };
                        // --- browser macros --- a granted macro can cross-post it.
                        let cross = if v.macro_cross_post_hint() {
                            crate::keymap::hint(" · ⇧⌘P to cross-post")
                        } else {
                            ""
                        };
                        let sub = crate::keymap::hint_owned(format!(
                            "{} · ⌘O opens it{cross}",
                            vm::short_permalink(&out.permalink)
                        ));
                        let head = match out.warning {
                            Some(w) => format!("{head} · {w}"),
                            None => head,
                        };
                        v.show_toast(head, Some(sub.into()), cx);
                    }
                    Err(CoreError::Offline) => {
                        v.show_toast("Offline. Publish again when you're back online.", None, cx)
                    }
                    // --- conflicts --- refused for a conflict: the sheet
                    // again, not a toast with no way out (issue #52).
                    Err(_) if v.reopen_conflict(&id, window, cx) => {}
                    Err(e) => v.show_toast(format!("Publish failed: {e}"), None, cx),
                }
                // --- follow-ups --- promotion may have rewritten local images.
                v.scratch_sync_editor(&id, window, cx);
                v.requery(window, cx);
            });
        })
        .detach();
    }

    fn toggle_kind(&mut self, _: &ToggleKind, window: &mut Window, cx: &mut Context<Self>) {
        self.toggle_kind_now(window, cx);
    }

    fn toggle_kind_now(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(item) = self.current.clone() else {
            return;
        };
        let kind = match item.kind {
            Kind::Fragment => Kind::Thread,
            Kind::Thread => Kind::Fragment,
        };
        match self.backend.set_kind(&item.local_id, kind) {
            Ok(()) => {
                self.requery(window, cx);
                self.show_toast(
                    match kind {
                        Kind::Thread => {
                            "Now a thread: no length limit, and it can quote other blygs"
                        }
                        Kind::Fragment => "Now a fragment: 1000 characters max",
                    },
                    None,
                    cx,
                );
            }
            Err(e) => self.show_toast(format!("Couldn't change kind: {e}"), None, cx),
        }
    }

    fn toggle_preview(&mut self, _: &TogglePreview, window: &mut Window, cx: &mut Context<Self>) {
        self.toggle_preview_now(window, cx);
    }

    /// ⌘E: the preview on or off in place; the editor keeps the keyboard.
    fn toggle_preview_now(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.studio_set_view(self.studio.view.toggle_preview(), cx);
    }

    fn open_permalink(&mut self, _: &OpenPermalink, _: &mut Window, cx: &mut Context<Self>) {
        match self.current.as_ref().and_then(|i| i.permalink.clone()) {
            Some(url) => {
                cx.open_url(&url);
                self.show_toast("Opening in your browser…", None, cx);
            }
            None => self.show_toast("Not published yet", None, cx),
        }
    }

    fn fake_offline(&mut self, _: &FakeToggleOffline, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(f) = &self.fake {
            let off = !f.is_offline();
            f.set_offline(off);
            self.show_toast(
                if off {
                    "Wi-Fi off (simulated). Keep writing: nothing is lost."
                } else {
                    "Back online"
                },
                None,
                cx,
            );
        }
    }

    fn fake_conflict(&mut self, _: &FakeConflict, _: &mut Window, _cx: &mut Context<Self>) {
        if let (Some(f), Some(cur)) = (&self.fake, &self.current) {
            f.trigger_conflict(&cur.local_id);
        }
    }

    fn resolve(&mut self, how: Resolution, window: &mut Window, cx: &mut Context<Self>) {
        let Some(ConflictSheet { id, .. }) = self.sheet.take() else {
            return;
        };
        match self.backend.resolve_conflict(&id, how) {
            Ok(()) => {
                if self.current.as_ref().is_some_and(|c| c.local_id == id) {
                    self.current = self.backend.item(&id);
                    self.load_current_into_editor(window, cx);
                }
                self.requery(window, cx);
                self.show_toast(
                    match how {
                        Resolution::KeepMine => {
                            "Kept your version; the server copy is saved in version history"
                        }
                        Resolution::TakeServer => "Took the server's version",
                        Resolution::KeepBoth => "Kept both: your version is now a separate draft",
                    },
                    None,
                    cx,
                );
            }
            Err(e) => self.show_toast(format!("Couldn't resolve: {e}"), None, cx),
        }
        self.focus_after_sheet(window, cx);
        cx.notify();
    }

    // ------------------------------------------------------------ images

    /// A web address pasted over selected text links the text: `[text](url)`.
    fn paste_link(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.current.is_none() {
            return false;
        }
        let Some(clip) = cx.read_from_clipboard().and_then(|c| c.text()) else {
            return false;
        };
        let (text, sel) = {
            let s = self.editor.read(cx);
            (s.value().to_string(), s.selected_range())
        };
        let Some((new, caret)) = vm::link_paste(&text, sel, &clip) else {
            return false;
        };
        self.splice_editor(&text, &new, Some(caret), window, cx);
        true
    }

    fn paste_image(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(item) = cx.read_from_clipboard() else {
            return false;
        };
        let image = item.entries().iter().find_map(|e| match e {
            ClipboardEntry::Image(img) => Some(img.clone()),
            _ => None,
        });
        if let Some(img) = image {
            let mime = img.format.mime_type();
            self.start_upload(img.bytes, mime.to_string(), window, cx);
            return true;
        }
        // Copied image files arrive as paths.
        let paths: Vec<std::path::PathBuf> = item
            .entries()
            .iter()
            .filter_map(|e| match e {
                ClipboardEntry::ExternalPaths(p) => Some(p.paths().to_vec()),
                _ => None,
            })
            .flatten()
            .filter(|p| vm::mime_for_path(p).is_some())
            .collect();
        if paths.is_empty() {
            return false;
        }
        self.upload_paths(paths, window, cx);
        true
    }

    fn upload_paths(
        &mut self,
        paths: Vec<std::path::PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        for p in paths {
            let Some(mime) = vm::mime_for_path(&p) else {
                continue;
            };
            match std::fs::read(&p) {
                Ok(bytes) => self.start_upload(bytes, mime.to_string(), window, cx),
                Err(e) => self.show_toast(format!("Couldn't read {}: {e}", p.display()), None, cx),
            }
        }
    }

    fn start_upload(
        &mut self,
        bytes: Vec<u8>,
        mime: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(id) = self.current.as_ref().map(|c| c.local_id.clone()) else {
            self.show_toast("Open or create a post first", None, cx);
            return;
        };
        // --- follow-ups --- a scratch note keeps its images on this Mac.
        if self.scratch_keep_image(&bytes, &mime, window, cx) {
            return;
        }
        // Insert the placeholder at the caret, as an undoable edit.
        let (text, cursor) = {
            let s = self.editor.read(cx);
            (s.value().to_string(), s.cursor())
        };
        let (new_text, range) = vm::insert_placeholder(&text, cursor);
        self.splice_editor(&text, &new_text, Some(range.end), window, cx);
        let size_kb = bytes.len().div_ceil(1024);
        let backend = self.backend.clone();
        // Uploaded *unattached*: the Worker appends every attachment to the
        // public page, so an attached image that's also inline would show
        // twice. Inline, it shows once, where it was pasted.
        let task = cx.background_spawn(async move {
            let r = backend.upload_media(bytes.clone(), &mime, None, None);
            // The text gets the absolute URL: the Worker renders a relative
            // `media/…` page-relative, which breaks under /f/<id>/.
            r.map(|m| {
                let inline = crate::vm::absolute_url(&m.url, backend.base_url().as_deref())
                    .unwrap_or_else(|| m.url.clone());
                (m, inline)
            })
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |v, window, cx| {
                let editing_same = v.current.as_ref().is_some_and(|c| c.local_id == id);
                let text = if editing_same {
                    v.editor.read(cx).value().to_string()
                } else {
                    v.backend
                        .item(&id)
                        .map(|i| i.content_md)
                        .unwrap_or_default()
                };
                let replaced = match &result {
                    Ok((_, inline)) => vm::replace_placeholder(&text, inline),
                    Err(_) => vm::remove_placeholder(&text),
                };
                // --- upkeep --- once the image is in place, ask for its alt text.
                let alt_for = result
                    .as_ref()
                    .ok()
                    .filter(|_| replaced.is_some() && editing_same)
                    .map(|(_, inline)| inline.clone());
                // The placeholder was deleted before the upload finished:
                // nothing refers to the file, so remove it again.
                if replaced.is_none()
                    && let Ok((m, _)) = &result
                {
                    let (b, url) = (v.backend.clone(), m.url.clone());
                    cx.background_spawn(async move {
                        let _ = b.delete_media(&url);
                    })
                    .detach();
                }
                if let Some(new_text) = replaced {
                    if editing_same {
                        v.splice_editor(&text, &new_text, None, window, cx);
                    } else if v.restoring.as_ref() != Some(&id) {
                        let _ = v.backend.save(&id, &new_text);
                    }
                }
                match result {
                    Ok((m, _)) => {
                        let ext = m.url.rsplit('.').next().unwrap_or("");
                        v.show_toast(format!("Image uploaded ({size_kb} KB {ext})"), None, cx)
                    }
                    Err(e) => v.show_toast(format!("Upload failed: {e}"), None, cx),
                }
                if let Some(url) = alt_for {
                    v.upkeep_ask_alt(id.clone(), url, window, cx); // --- upkeep ---
                }
            });
        })
        .detach();
    }

    /// Apply `old -> new` to the editor as a minimal, undoable replacement,
    /// keeping scroll. The caret goes to `caret` if given, else stays put
    /// (shifted if the edit was before it). Then saves.
    fn splice_editor(
        &mut self,
        old: &str,
        new: &str,
        caret: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (range, insert) = vm::splice(old, new);
        let prev_cursor = self.editor.read(cx).cursor();
        self.editor.update(cx, |s, cx| {
            s.set_selected_range(range.clone(), cx);
            s.replace(insert.to_string(), window, cx);
            let c = caret.unwrap_or_else(|| vm::shift_cursor(prev_cursor, &range, insert.len()));
            let c = c.min(s.text().len());
            s.set_selected_range(c..c, cx);
        });
        self.after_edit(cx);
    }

    /// Save the editor's text to the backend and refresh derived state.
    fn after_edit(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.current.as_ref().map(|c| c.local_id.clone()) else {
            return;
        };
        // --- discard changes --- the blyg is replacing this text: saving
        // it now could put the old text back after the restore.
        if self.restoring.as_ref() == Some(&id) {
            self.refresh_preview(false, cx);
            cx.notify();
            return;
        }
        let text = self.editor.read(cx).value();
        self.search.edits += 1;
        let saved = self.ai_save_edit(&id, &text);
        if let Err(e) = &saved {
            self.show_toast(format!("Couldn't save: {e}"), None, cx);
        }
        // The row as the save leaves it, worked out here rather than read
        // back from the store: a keystroke never waits on SQLite.
        let fresh = match (&saved, self.current.as_ref()) {
            (Ok(()), Some(cur)) if cur.local_id == id => {
                Some(blyg_core::store::saved_locally(cur, &text))
            }
            _ => self.backend.item(&id),
        };
        if let Some(fresh) = fresh {
            self.list.update_item(fresh.clone());
            self.current = Some(fresh);
        }
        self.refresh_preview(false, cx);
        cx.notify();
    }

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

    fn render_editor_pane(
        &self,
        body_font: &SharedString,
        size: f32,
        side_pad: Pixels,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        // --- themes --- a lit harbour (the editor inset in a sea), the
        // margins' ornaments, and the divider band under the text.
        let harbour = crate::ornament::harbour(&self.theme);
        let inset = if harbour.is_some() { 10. } else { 0. };
        let margins = crate::ornament::editor_margins(&self.theme, f32::from(side_pad) - inset);
        let band = crate::ornament::divider(&self.theme);
        let p = self.palette;
        let radius = self.theme.radius;
        let paper = div()
            .size_full()
            .relative()
            .flex()
            .flex_col()
            .bg(p.editor)
            .when_some(harbour, |d, (_, glow)| {
                d.rounded(px(radius)).overflow_hidden().shadow(vec![
                    BoxShadow {
                        color: p.line,
                        offset: point(px(0.), px(0.)),
                        blur_radius: px(0.),
                        spread_radius: px(1.),
                        inset: false,
                    },
                    BoxShadow {
                        color: glow.opacity(0.28),
                        offset: point(px(0.), px(0.)),
                        blur_radius: px(30.),
                        spread_radius: px(0.),
                        inset: false,
                    },
                ])
            })
            // --- own posts --- Reply, Quote, Fork, Link on your published post.
            .children(self.render_own_post_bar(cx))
            // --- stub quotes --- a stub's hint line and passage chooser.
            .children(self.render_stub_bar(cx))
            .child(
                // --- nearby --- the rail sits right of the text.
                div().flex_1().min_h_0().flex().child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .h_full()
                        .relative()
                        .child(gpui_kit::base::input::Textarea::new(&self.editor))
                        .children(margins)
                        .child(self.assist.clone()) // --- composer ---
                        .map(|d| self.hover_card_editor(d, cx)) // --- hover cards ---
                        .children(self.render_focus_fade(cx)) // --- focus ---
                        .children(self.render_serendipity(side_pad, cx)), // --- serendipity ---
                ), // --- nearby --- (the rail is a side drawer now: `side.rs`)
            )
            .children(self.render_lineage_strip(cx)) // --- lineage ---
            .children(band);
        div()
            .id("editor-pane")
            .debug_selector(|| "editor-pane".into())
            .flex_1()
            .min_w_0()
            .min_h_0()
            .relative()
            .when_some(harbour, |d, (sea, _)| d.bg(sea).p(px(inset)))
            .font_family(body_font.clone())
            .text_size(px(size))
            .line_height(relative(1.6))
            // --- composer --- the mention popup / spelling menu keys come first.
            .map(|d| self.composer_keys(d, cx))
            .map(|d| self.serendipity_keys(d, cx)) // --- serendipity ---
            .capture_action(cx.listener(|this, _: &Escape, window, cx| {
                if !this.sheet.is_open() {
                    cx.stop_propagation();
                    // --- focus --- esc leaves focus mode first.
                    if this.focus_escape(window, cx) {
                        return;
                    }
                    // --- AI --- esc cancels a running generation first.
                    if !this.ai_escape(window, cx) {
                        this.back_to_search(window, cx);
                    }
                }
            }))
            .capture_action(cx.listener(|this, a: &Enter, window, cx| {
                if a.secondary {
                    cx.stop_propagation();
                    this.publish(window, cx);
                }
            }))
            .capture_action(cx.listener(|this, _: &Paste, window, cx| {
                this.assist.update(cx, |a, _| a.note_paste()); // --- composer ---
                if this.paste_link(window, cx) || this.paste_image(window, cx) {
                    cx.stop_propagation();
                }
            }))
            .map(|d| self.editor_quote_drop(d, cx)) // --- tray ---
            .on_drop(cx.listener(|this, paths: &ExternalPaths, window, cx| {
                let imgs: Vec<_> = paths
                    .paths()
                    .iter()
                    .filter(|p| vm::mime_for_path(p).is_some())
                    .cloned()
                    .collect();
                if !imgs.is_empty() {
                    this.upload_paths(imgs, window, cx);
                }
            }))
            .child(paper)
    }

    // --- composer ---
    /// While the mention popup or the spelling menu is up, ↑/↓/⏎/⇥/esc are
    /// theirs (checked before the editor's own and the pane's esc).
    fn composer_keys(&self, d: Stateful<Div>, cx: &mut Context<Self>) -> Stateful<Div> {
        use crate::composer::AssistKey;
        use gpui_kit::base::input::IndentInline;
        fn route(
            this: &mut MainView,
            key: AssistKey,
            window: &mut Window,
            cx: &mut Context<MainView>,
        ) {
            if this.assist.read(cx).wants_keys()
                && this
                    .assist
                    .update(cx, |a, cx| a.handle_key(key, window, cx))
            {
                cx.stop_propagation();
            }
        }
        d.capture_action(
            cx.listener(|this, _: &MoveUp, window, cx| route(this, AssistKey::Up, window, cx)),
        )
        .capture_action(
            cx.listener(|this, _: &MoveDown, window, cx| route(this, AssistKey::Down, window, cx)),
        )
        .capture_action(cx.listener(|this, a: &Enter, window, cx| {
            if !a.secondary {
                route(this, AssistKey::Enter, window, cx)
            }
        }))
        .capture_action(
            cx.listener(|this, _: &IndentInline, window, cx| {
                route(this, AssistKey::Tab, window, cx)
            }),
        )
        .capture_action(
            cx.listener(|this, _: &Escape, window, cx| route(this, AssistKey::Escape, window, cx)),
        )
    }

    /// --- conflicts --- The resolve sheet for an item in conflict.
    pub(crate) fn show_conflict_sheet(
        &mut self,
        id: LocalId,
        mine: &str,
        theirs: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (m, t) = vm::word_diff(mine, theirs);
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        self.sheet.open(ConflictSheet {
            id,
            mine: m,
            theirs: t,
            focus,
        });
        cx.notify();
    }

    /// --- conflicts --- Show the resolve sheet again for an item that is
    /// still in conflict, from the texts the store kept (the event that
    /// first showed it fires only once; issue #52). False when it isn't in
    /// conflict, or another sheet is up.
    pub(crate) fn reopen_conflict(
        &mut self,
        id: &LocalId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.sheet.is_open() {
            return false;
        }
        let Some((mine, theirs)) = self.backend.conflict_texts(id) else {
            return false;
        };
        self.show_conflict_sheet(id.clone(), &mine, &theirs, window, cx);
        true
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
