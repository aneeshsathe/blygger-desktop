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
#[cfg_attr(
    not(any(target_os = "macos", target_os = "windows")),
    expect(dead_code, reason = "a macOS and Windows feature")
)]
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

// --- posts --- the Posts screen: search and selection (app/posts.rs), and
// drawing the omnibar, list and rows (app/posts_render.rs).
mod posts;
mod posts_render;

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
// --- format --- bold, italic and formulas
pub(crate) mod format;

/// --- sheets --- one main-window sheet at a time; each kind in `sheet/`.
#[path = "sheet/mod.rs"]
pub(crate) mod sheet;
use settings_page::SettingsSheet;
use sheet::conflict::ConflictSheet;
pub(crate) use sheet::connect::{CancelOnDrop, ConnectSheet, ConnectWay, DisconnectSheet};
use sheet::publish::{HeldLabelsSheet, PublishSheet, TangleSheet};

// The main window's features, each a child module extending `MainView`.
mod chrome; // title bar, config-problems banner, status bar, toasts
mod connect; // connecting and disconnecting a blyg
pub(crate) mod esc; // Esc and clicks take the keyboard back from web views (Windows)
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
    pub(crate) assist: Entity<crate::composer::Assist>,
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
        // Back in front: the keyboard a WebView held or dropped, a rule changed on the web.
        subs.push(cx.observe_window_activation(window, |this, window, cx| {
            if window.is_window_active() {
                this.studio.reclaim_keyboard();
                this.browser.reclaim_lost_keyboard(); // --- browser ---
                this.count_read_on_front(cx); // --- studio 0.71 --- chosen on the web
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
        this.dictionary_refresh(true, cx); // --- dictionary ---
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

    // --- writing --- loading the editor: app/writing.rs

    // --- writing --- opening and creating posts: app/writing.rs

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
            CoreEvent::ReadMarked(rows) => self.read_marked(&rows, reading_follows, cx),
            CoreEvent::Error(msg) => self.show_toast(msg, None, cx),
            CoreEvent::ServerOutdated => self.server_outdated(cx),
            CoreEvent::ServerNewer(contract) => self.server_newer(&contract, cx),
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

    // --- writing --- ⌘N, ⌘T, ⌘E (app/writing.rs) and publishing (app/publish.rs)

    // --- writing --- conflicts (app/publish.rs), pasted links and images
    // (app/upload.rs), saving edits (app/writing.rs)
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
        let show_list = self.side_list_shown(window); // --- side ---
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
            .map(|d| self.format_actions(d, cx)) // --- format ---
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
                                            let preview = self.studio_pane(&p, cx);
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
            .children(self.render_toast(cx))
            .children(onboarding) // --- onboarding ---
            .children(crate::platform::menu_panel(p, window, cx)) // Windows menu
    }
}

impl MainView {
    // --- writing --- the editor pane (app/writing.rs), the conflict sheet
    // (app/publish.rs)
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
