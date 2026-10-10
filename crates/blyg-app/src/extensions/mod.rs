//! --- extensions --- The main window's side of Burrow's extensions
//! (docs/SPEC.md § Extensions, docs/EXTENSIONS.md). The host itself is
//! `blyg_ext::Host`; this module runs it for the window:
//!
//! - this file: the window's extension state, starting the host after the
//!   first paint, its events (toasts naming the extension, status-bar
//!   notices, selecting an item), Reload Config, the publish hook, the
//!   config lines consent and Settings write, and putting text into the
//!   draft;
//! - `sheets`: the ⇧⌘P Extensions palette, the consent sheet and the
//!   Manage extensions sheet;
//! - `library`: a library (the bundled markdown-notes) in the notes drawer,
//!   and the quote picker's read-only Notes chip.
//!
//! The UI never waits on an extension: every call runs off the main thread
//! (the host's own timeouts bound it) and lands back here when it answers.
//! Nothing here needs a blyg: the host answers extensions from the window's
//! backend, whatever it holds (a blyg, the fake, or none).

mod library;
mod macros;
pub(crate) mod reading_slots; // --- reading slots ---
mod sheets;

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use blyg_core::ConfigStore;
use blyg_core::config::{Change, Config};
use blyg_core::{FRAGMENT_LIMIT, Kind, Status, published_len};
use blyg_ext::protocol::{CommandContext, ItemRef, ReadingRef, Screen};
use blyg_ext::{
    BackendApi, Capability, ConsentReply, ExtEvent, ExtState, Grants, Host, HostConfig, Timing,
};
use gpui_kit::*;

use crate::app::MainView;

pub(crate) use library::Library;
pub(crate) use sheets::Overlay;

gpui_kit::actions!(
    blygger,
    [
        /// ⇧⌘P: the Extensions palette (its commands, its libraries, and
        /// "Manage extensions…").
        ShowExtensions
    ]
);

/// The bundled notes extension's name.
pub use blyg_ext_notes::NAME as NOTES;

/// How long after the window opens the extensions start.
pub const START_DELAY: Duration = Duration::from_millis(1000);

/// How the app runs extensions: its own executable for the bundled one
/// (`blygger +ext markdown-notes`), and the data folder each extension's
/// storage lives under. A global, so tests can run the bundled extension
/// from the test binary over a temporary folder; the app derives it.
#[derive(Clone)]
pub struct Launch {
    pub program: PathBuf,
    /// markdown-notes: `program args…`.
    pub args: Vec<String>,
    /// cross-post: `program crosspost_args…`.
    pub crosspost_args: Vec<String>,
    /// --- reading slots --- reading-time and inspect: `program args…`.
    pub reading_time_args: Vec<String>,
    pub inspect_args: Vec<String>,
    pub data_dir: PathBuf,
    pub timing: Timing,
}

impl Global for Launch {}

impl Launch {
    /// The app's own: this executable, `+ext markdown-notes`.
    fn app(data_dir: PathBuf) -> Option<Launch> {
        Some(Launch {
            program: std::env::current_exe().ok()?,
            args: vec!["+ext".into(), NOTES.into()],
            crosspost_args: vec!["+ext".into(), blyg_ext_crosspost::NAME.into()],
            reading_time_args: vec!["+ext".into(), blyg_ext_reading_time::NAME.into()],
            inspect_args: vec!["+ext".into(), blyg_ext_inspect::NAME.into()],
            data_dir,
            timing: Timing::default(),
        })
    }
}

/// The host's configuration from the config file: what's enabled, what's
/// granted, each extension's settings, and the bundled markdown-notes
/// (whose manifest names the configured vault).
/// `cli::host_config` (the same view `+list-extensions` has), run from
/// `launch`'s program, data folder and timing.
pub fn host_config(store: &ConfigStore, launch: &Launch) -> HostConfig {
    let mut hc = crate::cli::host_config(store);
    hc.data_dir = launch.data_dir.clone();
    let notes = hc.settings.get(NOTES).cloned().unwrap_or_default();
    // Both are off until the config names them (`extension = …`).
    hc.bundled = vec![
        blyg_ext_notes::bundled(launch.program.clone(), launch.args.clone(), &notes),
        blyg_ext_crosspost::bundled(launch.program.clone(), launch.crosspost_args.clone()),
    ];
    hc.bundled.extend(reading_slots::bundled(launch)); // --- reading slots ---
    hc.timing = launch.timing.clone();
    hc
}

// ------------------------------------------------------------ config lines

/// `key`'s values with `add` appended (those not already there).
fn with_values(cfg: &Config, key: &'static str, add: &[String]) -> Option<(&'static str, Change)> {
    let mut v = cfg.list(key);
    let before = v.len();
    for a in add {
        if !v.iter().any(|x| x.trim() == a.trim()) {
            v.push(a.clone());
        }
    }
    (v.len() != before).then_some((key, Change::List(v)))
}

/// The config changes that grant `caps` to `name` (and enable it).
pub fn allow_changes(cfg: &Config, name: &str, caps: &[Capability]) -> Vec<(&'static str, Change)> {
    let mut out = vec![];
    out.extend(with_values(cfg, "extension", &[name.to_string()]));
    out.extend(with_values(
        cfg,
        "extension-allow",
        &Grants::allow_lines(name, caps),
    ));
    out
}

/// The config changes that point markdown-notes at `vault` (and enable it):
/// `extension = markdown-notes`, `extension-setting = markdown-notes
/// vault=<vault>` replacing an earlier vault line (other settings stay).
pub fn vault_changes(cfg: &Config, vault: &str) -> Vec<(&'static str, Change)> {
    let mut out = vec![];
    out.extend(with_values(cfg, "extension", &[NOTES.to_string()]));
    let line = format!("{NOTES} vault={vault}");
    let mut settings: Vec<String> = cfg
        .list("extension-setting")
        .into_iter()
        .filter(|l| {
            let mut w = l.split_whitespace();
            !(w.next() == Some(NOTES)
                && w.next()
                    .is_some_and(|kv| kv.split('=').next().map(str::trim) == Some("vault")))
        })
        .collect();
    settings.push(line);
    out.push(("extension-setting", Change::List(settings)));
    out
}

/// A folder as the config writes it: `~/…` under the home folder.
pub fn vault_text(path: &std::path::Path) -> String {
    let home = blyg_ext::capability::home_dir();
    if let Some(h) = home
        && let Ok(rest) = path.strip_prefix(&h)
    {
        let rest = rest.to_string_lossy().replace('\\', "/");
        return if rest.is_empty() {
            "~".into()
        } else {
            format!("~/{rest}")
        };
    }
    path.to_string_lossy().into_owned()
}

// ------------------------------------------------------------ state

/// A status-bar notice for one extension.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Notice {
    /// Enabled but not allowed (yet): a click opens the consent sheet.
    NeedsPermission,
    /// Failed or stopped: a click opens Manage extensions.
    Problem(String),
}

/// A consent request waiting for the user.
pub(crate) struct Ask {
    pub name: String,
    pub caps: Vec<Capability>,
    /// `burrow/requestCapability`: answer it.
    pub reply: Option<ConsentReply>,
}

/// The window's extension state (a field of `MainView`).
#[derive(Default)]
pub(crate) struct Extensions {
    pub host: Option<Host>,
    pub launch: Option<Launch>,
    /// `Host::start` has run (after the first paint).
    pub started: bool,
    pub notices: BTreeMap<String, Notice>,
    /// Consent requests not shown yet (a sheet was up).
    pub asks: std::collections::VecDeque<Ask>,
    /// `(name, missing)` already asked about this session.
    pub asked: Vec<(String, Vec<Capability>)>,
    /// `(name, caps)` the user unticked when allowing the rest.
    pub declined: Vec<(String, Vec<Capability>)>,
    pub overlay: Option<Overlay>,
    pub overlay_gen: usize,
    /// Events from the host's threads, for the UI to handle.
    pub inbox: Arc<std::sync::Mutex<std::collections::VecDeque<ExtEvent>>>,
    // --- library ---
    pub lib: Library,
    /// --- reading slots --- byline markers and the ⋯ sheet.
    pub slots: reading_slots::Slots,
}

impl Extensions {
    /// A palette, consent or manage sheet (or a reading entry's ⋯ sheet)
    /// is up (web views hide).
    pub fn has_overlay(&self) -> bool {
        self.overlay.is_some() || self.slots.sheet.is_some()
    }
}

impl MainView {
    // ------------------------------------------------------------ host

    /// Hook (`MainView::new`): make the host and start it about a second
    /// after the window's first frame. No data folder (a test without
    /// one), no host.
    pub(crate) fn ext_init(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let launch = cx.try_global::<Launch>().cloned().or_else(|| {
            cx.try_global::<crate::connection::Connection>()
                .and_then(|c| Launch::app(c.data_dir.clone()))
        });
        let Some(launch) = launch else {
            return;
        };
        let Some(conf) = cx.try_global::<crate::settings::AppConfig>() else {
            return;
        };
        let hc = host_config(&conf.store, &launch);
        // Host threads put events in the inbox; the UI drains it. In the
        // app a wake-up channel says when (tests drain it themselves with
        // `ext_pump`: GPUI's test scheduler refuses wake-ups from other
        // threads).
        let inbox = self.ext.inbox.clone();
        let (wake, woken) = async_channel::bounded::<()>(1);
        let live = !cfg!(test);
        // The window's backend is a switch: connecting or disconnecting a
        // blyg swaps what's inside it, so the host follows without rewiring.
        let host = Host::new(
            hc,
            Arc::new(BackendApi::new(self.backend.clone())),
            move |ev| {
                inbox
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .push_back(ev);
                if live {
                    let _ = wake.try_send(());
                }
            },
        );
        self.ext.host = Some(host.clone());
        self.ext.launch = Some(launch);
        self._tasks.push(cx.spawn_in(window, async move |this, cx| {
            while woken.recv().await.is_ok() {
                if this
                    .update_in(cx, |v, window, cx| v.ext_pump(window, cx))
                    .is_err()
                {
                    break;
                }
            }
        }));
        // Stop them when the app quits (the host's drop does it too).
        self._subs.push(cx.on_app_quit(move |_, cx| {
            let host = host.clone();
            cx.background_spawn(async move { host.shutdown() })
        }));
        if cfg!(test) {
            return; // tests start it themselves (`ext_start`)
        }
        // The first frame is long painted by then.
        self._tasks.push(cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(START_DELAY).await;
            let _ = this.update_in(cx, |v, window, cx| v.ext_start(window, cx));
        }));
    }

    /// Start the enabled extensions (once).
    pub(crate) fn ext_start(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(host) = self.ext.host.clone() else {
            return;
        };
        if self.ext.started {
            return; // later changes go through `ext_reload`
        }
        self.ext.started = true;
        host.start();
        self.ext_check_missing(window, cx);
    }

    /// Hook (Reload Config, and after consent or Settings wrote the
    /// config): hand the host the config as it is now. Off the main
    /// thread: stopping an extension waits for it.
    pub(crate) fn ext_reload(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(host), Some(launch)) = (self.ext.host.clone(), self.ext.launch.clone()) else {
            return;
        };
        if !self.ext.started {
            return; // the first start reads the config itself
        }
        let conf = crate::settings::get(cx);
        let hc = host_config(&conf.store, &launch);
        // Problems from a run that's being retried are stale now.
        self.ext
            .notices
            .retain(|_, n| matches!(n, Notice::NeedsPermission));
        let task = cx.background_spawn(async move { host.reload(hc) });
        cx.spawn_in(window, async move |this, cx| {
            task.await;
            let _ = this.update_in(cx, |v, window, cx| {
                v.ext_check_missing(window, cx);
                v.ext_lib_refresh(window, cx);
                cx.notify();
            });
        })
        .detach();
    }

    /// Enabled extensions that were allowed something but now ask for more
    /// (a new vault folder, a manifest that grew): ask once per session,
    /// else leave a notice.
    fn ext_check_missing(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(host) = self.ext.host.clone() else {
            return;
        };
        for s in host.status() {
            if !s.enabled || s.missing.is_empty() || s.granted.is_empty() {
                continue;
            }
            if matches!(s.state, ExtState::Missing) {
                continue;
            }
            let key = (s.name.clone(), s.missing.clone());
            if self.ext.declined.contains(&key) {
                continue; // the user unticked these: not a problem to nag about
            }
            if self.ext.asked.contains(&key) {
                self.ext
                    .notices
                    .insert(s.name.clone(), Notice::NeedsPermission);
                continue;
            }
            self.ext.asked.push(key);
            self.ext_ask(
                Ask {
                    name: s.name,
                    caps: s.missing,
                    reply: None,
                },
                window,
                cx,
            );
        }
        cx.notify();
    }

    /// Handle every event the host has sent since last time.
    pub(crate) fn ext_pump(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        loop {
            let ev = self
                .ext
                .inbox
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .pop_front();
            match ev {
                Some(ev) => self.ext_event(ev, window, cx),
                None => break,
            }
        }
    }

    /// What the host says, on the UI thread.
    pub(crate) fn ext_event(&mut self, ev: ExtEvent, window: &mut Window, cx: &mut Context<Self>) {
        match ev {
            ExtEvent::Started { name, .. } => {
                self.ext.slots.forget(&name); // --- reading slots ---
                if self.ext.notices.get(&name) != Some(&Notice::NeedsPermission) {
                    self.ext.notices.remove(&name);
                }
                self.ext_lib_refresh(window, cx);
            }
            ExtEvent::Failed { name, message, .. } => {
                self.ext.notices.insert(name, Notice::Problem(message));
            }
            ExtEvent::Stopped { name, message } => match message {
                Some(m) => {
                    self.ext.notices.insert(name, Notice::Problem(m));
                }
                None => {
                    if matches!(self.ext.notices.get(&name), Some(Notice::Problem(_))) {
                        self.ext.notices.remove(&name);
                    }
                }
            },
            ExtEvent::NeedsConsent { name, requested } => {
                self.ext.asked.push((name.clone(), requested.clone()));
                self.ext_ask(
                    Ask {
                        name,
                        caps: requested,
                        reply: None,
                    },
                    window,
                    cx,
                );
            }
            ExtEvent::Toast { name, text, detail } => {
                let sub = match detail {
                    Some(d) => format!("{d} · {name}"),
                    None => name,
                };
                self.show_toast(text, Some(sub.into()), cx);
            }
            ExtEvent::OpenItem { id, .. } => self.ext_open_item(&id, window, cx),
            ExtEvent::CapabilityRequested {
                name,
                capability,
                reply,
            } => self.ext_ask(
                Ask {
                    name,
                    caps: vec![capability],
                    reply: Some(reply),
                },
                window,
                cx,
            ),
            ExtEvent::BrowserPage { reply, .. } => self.ext_browser_page(reply, window, cx),
            ExtEvent::BrowserOpen { url, reply, .. } => {
                self.ext_browser_open(url, reply, window, cx)
            }
        }
        cx.notify();
    }

    /// `burrow/openItem`, a command's `open`: select the item.
    fn ext_open_item(
        &mut self,
        id: &blyg_core::LocalId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.backend.item(id).is_none() {
            return;
        }
        self.leave_reading();
        self.open_new_draft(id, window, cx);
    }

    /// Hook (publish completion): tell extensions holding
    /// `hooks:itemPublished`. Never blocks.
    pub(crate) fn ext_item_published(
        &self,
        id: &blyg_core::LocalId,
        outcome: &blyg_core::PublishOutcome,
        note: Option<&str>,
    ) {
        if let (Some(host), Some(item)) = (&self.ext.host, self.backend.item(id)) {
            host.item_published(&item, outcome, note);
        }
    }

    /// Write `changes` to the config file (comments kept), then reload the
    /// host. `false` (after a toast) when the file couldn't be written.
    pub(crate) fn ext_write_config(
        &mut self,
        changes: &[(&'static str, Change)],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if let Err(e) = crate::settings::write(changes, cx) {
            self.show_toast(e, None, cx);
            return false;
        }
        // Our own write isn't an edit to reload.
        self.watch_stamp = crate::settings::watch_stamp(&crate::settings::watched(cx));
        self.refresh_problems(cx);
        if self.ext.started {
            self.ext_reload(window, cx);
        } else {
            self.ext_start(window, cx);
        }
        true
    }

    /// Settings › Notes folder: point markdown-notes at `folder` (enabling
    /// it); the consent sheet follows for the new folder.
    pub(crate) fn ext_set_vault(
        &mut self,
        folder: &std::path::Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let text = vault_text(folder);
        let changes = vault_changes(crate::settings::get(cx).store.config(), &text);
        // A new folder is a new question.
        self.ext.asked.retain(|(n, _)| n != NOTES);
        if self.ext_write_config(&changes, window, cx) {
            self.show_toast(
                format!("Notes folder: {text}"),
                Some(format!("{NOTES} reads and writes it, once you allow it").into()),
                cx,
            );
        }
    }

    /// The vault the config names for markdown-notes, when it's enabled.
    pub(crate) fn ext_vault(&self, cx: &App) -> Option<String> {
        let cfg = cx
            .try_global::<crate::settings::AppConfig>()?
            .store
            .config();
        cfg.extensions_enabled()
            .iter()
            .any(|n| n == NOTES)
            .then(|| blyg_ext_notes::vault_setting(&cfg.extension_settings(NOTES)))
    }

    // ------------------------------------------------------------ context

    /// The screen a command runs from.
    fn ext_screen(&self, window: &Window, cx: &App) -> Screen {
        if self.notes.open && self.notes.has_keyboard(window, cx) {
            Screen::Notes
        } else if self.reading.view != crate::app::reading::View::Posts {
            Screen::Reading
        } else {
            Screen::Posts
        }
    }

    /// What a command runs on (the selection is filled in separately).
    fn ext_context(&self, screen: Screen) -> CommandContext {
        CommandContext {
            item: self
                .current
                .as_ref()
                .filter(|_| screen == Screen::Posts)
                .map(ItemRef::of),
            selection: None,
            reading: self.reading.opened.as_ref().map(|o| ReadingRef {
                subscription_id: o.item.subscription_id.clone(),
                remote_id: o.item.remote_id.clone(),
            }),
            screen,
        }
    }

    /// The text selected wherever the user is: the notes drawer (or its
    /// open note), the browser pane's page, the reading pane's post, else
    /// the editor. Web views answer asynchronously; `then` gets the text
    /// ("" when nothing is selected).
    pub(crate) fn ext_selection(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        then: impl FnOnce(&mut MainView, String, &mut Window, &mut Context<MainView>) + 'static,
    ) {
        if let Some(text) = self.ext_lib_selection(cx) {
            return then(self, text, window, cx);
        }
        if self.notes.open
            && let Some((sel, _)) = self.notes_selected_passage(cx)
        {
            return then(self, sel, window, cx);
        }
        let (tx, rx) = async_channel::bounded::<String>(1);
        let asked = if self.browser.open {
            self.browser.selection(tx.clone())
        } else if self.studio.reader.active() && self.reading.opened.is_some() {
            self.studio.reader.selection(tx.clone())
        } else {
            false
        };
        if !asked {
            let text = {
                let s = self.editor.read(cx);
                let r = s.selected_range();
                s.value().get(r).unwrap_or_default().to_string()
            };
            let text = if self.current.is_some() {
                text
            } else {
                String::new()
            };
            return then(self, text, window, cx);
        }
        // A page that never answers mustn't make the command look dead.
        cx.spawn(async move |_, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(500))
                .await;
            let _ = tx.try_send(String::new());
        })
        .detach();
        cx.spawn_in(window, async move |this, cx| {
            let text = rx.recv().await.unwrap_or_default();
            let _ = this.update_in(cx, |v, window, cx| then(v, text, window, cx));
        })
        .detach();
    }

    // ------------------------------------------------------------ into the draft

    /// "Copy into post": `block` into the draft (or scratch note) open in
    /// the editor, at its caret, or a new draft when none is open.
    pub(crate) fn ext_insert_into_draft(
        &mut self,
        block: String,
        from: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let block = block.trim_end().to_string();
        if block.trim().is_empty() {
            return self.show_toast("Nothing to copy", None, cx);
        }
        let target = self.current.clone().filter(|c| {
            matches!(c.status, Status::Draft | Status::Scratch)
                && self.notes.id.as_ref() != Some(&c.local_id)
        });
        match target {
            Some(item) => {
                // The caret before anything moves it (opening puts it at the end).
                let (text, cursor) = {
                    let s = self.editor.read(cx);
                    (s.value().to_string(), s.cursor())
                };
                self.leave_reading();
                self.back_to_search(window, cx);
                self.open(&item.local_id, window, cx);
                let now = self.editor.read(cx).value().to_string();
                let cursor = if now == text { cursor } else { now.len() };
                let (new_text, caret) = crate::app::reading::insert_block(&now, cursor, &block);
                self.splice_editor(&now, &new_text, Some(caret), window, cx);
                self.show_toast(
                    format!("Copied into “{}”", crate::vm::item_title(&item)),
                    Some(from.to_string().into()),
                    cx,
                );
            }
            None => {
                let kind = if published_len(&block) > FRAGMENT_LIMIT {
                    Kind::Thread
                } else {
                    Kind::Fragment
                };
                match self.backend.create_draft(kind, &format!("{block}\n\n")) {
                    Ok(id) => {
                        self.open_new_draft(&id, window, cx);
                        self.show_toast("New draft with it", Some(from.to_string().into()), cx);
                    }
                    Err(e) => self.show_toast(format!("Couldn't start a draft: {e}"), None, cx),
                }
            }
        }
    }

    // ------------------------------------------------------------ notices

    /// The status bar's extension notices: "markdown-notes needs
    /// permission" (a click asks), or why one stopped (a click manages).
    pub(crate) fn render_ext_notice(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (name, notice) = self.ext.notices.iter().next()?;
        let more = self.ext.notices.len() - 1;
        let warn = self.palette.on_status_text(self.palette.warn);
        let link = self.palette.on_status_text(self.palette.accent);
        let (text, action) = match notice {
            Notice::NeedsPermission => (format!("{name} needs permission"), "Allow…"),
            Notice::Problem(m) => (m.clone(), "Extensions…"),
        };
        let text = if more > 0 {
            format!("{text} (+{more})")
        } else {
            text
        };
        let n = name.clone();
        let asks = matches!(notice, Notice::NeedsPermission);
        Some(
            div()
                .id("ext-notice")
                .debug_selector(|| "ext-notice".into())
                .flex()
                .items_center()
                .gap(px(6.))
                .min_w_0()
                .max_w(px(360.))
                .overflow_hidden()
                .cursor_pointer()
                .on_click(cx.listener(move |this, _, window, cx| {
                    if asks {
                        this.ext_ask_again(&n, window, cx);
                    } else {
                        this.ext_open_manage(window, cx);
                    }
                }))
                .child(div().truncate().text_color(warn).child(text))
                .child("·")
                .child(
                    div()
                        .flex_none()
                        .text_color(link)
                        .hover(|s| s.underline())
                        .child(action),
                )
                .into_any_element(),
        )
    }

    /// Reopen the consent sheet for what `name` still lacks.
    pub(crate) fn ext_ask_again(
        &mut self,
        name: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(host) = self.ext.host.clone() else {
            return;
        };
        let Some(s) = host.status().into_iter().find(|s| s.name == name) else {
            return;
        };
        let caps = if s.missing.is_empty() {
            s.requested
        } else {
            s.missing
        };
        self.ext_show_consent(
            Ask {
                name: name.to_string(),
                caps,
                reply: None,
            },
            window,
            cx,
        );
    }

    /// Something else is on screen that a sheet mustn't cover.
    fn ext_blocked(&self) -> bool {
        self.sheet.is_some()
            || self.reading.sheet.is_some()
            || self.ai.has_overlay()
            || self.ext.has_overlay()
            || self.onboarding.flow.is_some()
            || self.onboarding.tutorial.is_some()
    }

    /// Ask now, or queue it with a notice when a sheet is up.
    fn ext_ask(&mut self, ask: Ask, window: &mut Window, cx: &mut Context<Self>) {
        // The same question already on screen or waiting: once is enough.
        if ask.reply.is_none() {
            let showing = matches!(
                &self.ext.overlay,
                Some(Overlay::Consent { name, reply: None, .. }) if *name == ask.name
            );
            let queued = self
                .ext
                .asks
                .iter()
                .any(|a| a.name == ask.name && a.reply.is_none());
            if showing || queued {
                return;
            }
        }
        if self.ext_blocked() {
            if ask.reply.is_none() {
                self.ext
                    .notices
                    .insert(ask.name.clone(), Notice::NeedsPermission);
            }
            self.ext.asks.push_back(ask);
            cx.notify();
            return;
        }
        self.ext_show_consent(ask, window, cx);
    }

    /// After an extension sheet closes: the next queued ask, if any.
    fn ext_next_ask(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.ext_blocked() {
            return;
        }
        if let Some(ask) = self.ext.asks.pop_front() {
            self.ext_show_consent(ask, window, cx);
        }
    }

    // ------------------------------------------------------------ demos

    /// `BLYGGER_DEMO=ext-…` (snapshots; the config decides what runs):
    /// `ext-consent` (an enabled extension's first start), `ext-palette`
    /// (⇧⌘P), `ext-manage`, `ext-notes` (the library in the drawer with a
    /// note open: `BLYGGER_DEMO_NOTE`, default the first in the folder).
    pub(crate) fn ext_demo(
        &mut self,
        scenario: &str,
        n: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match (scenario, n) {
            (_, 0) => self.ext_start(window, cx),
            (s, n) if s.starts_with("ext-slots") => self.slots_demo(s, n, window, cx), // --- reading slots ---
            ("ext-palette", 1) => self.ext_toggle_palette(window, cx),
            ("ext-manage", 1) => self.ext_open_manage(window, cx),
            ("ext-notes", 1) => self.ext_lib_show(window, cx),
            ("ext-notes", 2) => {
                let id = std::env::var("BLYGGER_DEMO_NOTE").ok().or_else(|| {
                    self.ext
                        .lib
                        .entries
                        .iter()
                        .find(|e| !e.is_dir)
                        .map(|e| e.id.clone())
                });
                if let Some(id) = id.filter(|i| i != "-") {
                    self.ext_lib_open(id, window, cx);
                }
            }
            _ => {}
        }
        cx.notify();
    }
}
