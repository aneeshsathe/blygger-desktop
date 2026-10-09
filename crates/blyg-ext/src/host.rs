//! The extension host: starts the enabled extensions as child processes,
//! keeps them running (restart with backoff), answers their `burrow/*`
//! calls, and gives the UI blocking, time-limited calls into them.
//!
//! The UI never waits on an extension on its main thread: run
//! [`Host::command`], [`Host::library_search`] and friends on a background
//! executor (each has a hard timeout), and receive [`ExtEvent`]s through the
//! sink passed to [`Host::new`] (called from host threads; hop to the UI
//! from there, e.g. through an `async_channel`).
//!
//! Lifecycle per extension (a "runner" thread):
//! spawn → `initialize` (5 s) → running → on crash/EOF restart after 1 s,
//! 5 s, 30 s; the third failure inside 10 minutes stops it for good (until
//! [`Host::reload`]). Three consecutive timed-out requests restart it.
//! [`Host::shutdown`] (and dropping the host) sends `shutdown`, waits 2 s,
//! then kills the child: only the PIDs this host started.

use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, RwLock};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use crate::api::HostApi;
use crate::capability::Capability;
use crate::grants::Grants;
use crate::manifest::{Diagnostic, Installed, Origin, discover};
use crate::protocol::*;
use crate::rpc::{Connection, Incoming, params, to_value};
use crate::spawn::{
    STDERR_LOG_MAX, StderrLog, exe_extensions, is_runnable, path_dirs, resolve_program,
    scrubbed_env,
};

/// Timeouts and backoff. `Default` is what the app uses; tests shorten it.
#[derive(Debug, Clone)]
pub struct Timing {
    pub initialize: Duration,
    /// `source.search`, `library.search` (interactive).
    pub search: Duration,
    /// `source.read`, `library.list`, `library.read`, `library.write`.
    pub read: Duration,
    pub command: Duration,
    /// How long `shutdown` may take before the child is killed.
    pub shutdown: Duration,
    /// Waits before each restart; the last repeats.
    pub backoff: Vec<Duration>,
    /// Failures counted inside this window …
    pub failure_window: Duration,
    /// … stop the extension at this many.
    pub max_failures: usize,
    /// Consecutive timed-out requests that restart it.
    pub timeouts_before_restart: u32,
    /// How long a `burrow/requestCapability` waits for the user.
    pub consent: Duration,
    /// How long `burrow/browser.page` and `burrow/browser.open` wait for
    /// the window to answer.
    pub browser: Duration,
}

impl Default for Timing {
    fn default() -> Self {
        Timing {
            initialize: Duration::from_secs(5),
            search: Duration::from_secs(2),
            read: Duration::from_secs(5),
            command: Duration::from_secs(60),
            shutdown: Duration::from_secs(2),
            backoff: vec![
                Duration::from_secs(1),
                Duration::from_secs(5),
                Duration::from_secs(30),
            ],
            failure_window: Duration::from_secs(600),
            max_failures: 3,
            timeouts_before_restart: 3,
            consent: Duration::from_secs(300),
            browser: Duration::from_secs(5),
        }
    }
}

/// Everything the host needs from the config file and the app, as plain
/// values (the config layer reads `extension`, `extension-allow` and
/// `extension-setting`; see [`Grants::from_allow_lines`] and
/// [`crate::grants::settings_from_lines`]).
#[derive(Debug, Clone)]
pub struct HostConfig {
    /// `extension = <name>` lines: nothing else runs.
    pub enabled: Vec<String>,
    pub grants: Grants,
    /// `name -> key -> value`.
    pub settings: BTreeMap<String, BTreeMap<String, String>>,
    /// Where installed extensions live (`ConfigFiles::extensions_dir()`).
    pub extensions_dir: Option<PathBuf>,
    /// The app's data directory: each extension gets
    /// `<data_dir>/extensions/<name>/` as its own storage.
    pub data_dir: PathBuf,
    /// Extensions built into the app (markdown-notes).
    pub bundled: Vec<Installed>,
    pub host: HostInfo,
    pub timing: Timing,
}

impl HostConfig {
    pub fn new(data_dir: PathBuf, host_version: &str) -> Self {
        HostConfig {
            enabled: vec![],
            grants: Grants::default(),
            settings: BTreeMap::new(),
            extensions_dir: None,
            data_dir,
            bundled: vec![],
            host: HostInfo {
                name: "Burrow".into(),
                version: host_version.into(),
                platform: std::env::consts::OS.into(),
            },
            timing: Timing::default(),
        }
    }
}

/// `<data_dir>/extensions/<name>/`.
pub fn storage_dir(data_dir: &std::path::Path, name: &str) -> PathBuf {
    data_dir.join("extensions").join(name)
}

/// What the host tells the UI. Sent from host threads.
#[derive(Debug)]
pub enum ExtEvent {
    /// Running and initialized.
    Started { name: String, version: String },
    /// A start or a run ended badly; a restart follows unless `Stopped`
    /// comes next. `stderr` is the last lines it wrote.
    Failed {
        name: String,
        message: String,
        stderr: Vec<String>,
    },
    /// Not running any more. `message` is `None` for an orderly stop
    /// (reload, quit) and says why otherwise ("stopped after 3 failures;
    /// Reload Config to retry").
    Stopped {
        name: String,
        message: Option<String>,
    },
    /// Enabled, but the user hasn't answered its consent sheet yet: show it
    /// (with [`crate::consent_sentence`] over `requested`). Not started.
    NeedsConsent {
        name: String,
        requested: Vec<Capability>,
    },
    /// `burrow/toast`.
    Toast {
        name: String,
        text: String,
        detail: Option<String>,
    },
    /// `burrow/openItem`: select this item in the list.
    OpenItem {
        name: String,
        id: blyg_core::LocalId,
    },
    /// `burrow/requestCapability`: ask the user, then [`ConsentReply::answer`].
    /// If granted, also write the `extension-allow` line to the config.
    CapabilityRequested {
        name: String,
        capability: Capability,
        reply: ConsentReply,
    },
    /// `burrow/browser.page` while one of the extension's commands runs:
    /// read the page in the browser pane and answer `Some`, or `None` when
    /// the pane is closed or has no page (`-32004`).
    BrowserPage {
        name: String,
        reply: UiReply<Option<PageCapture>>,
    },
    /// `burrow/browser.open`, `url` on an origin the user granted (checked
    /// already): show the pane at `url` and answer the URL it's loading,
    /// or why not (`-32003`).
    BrowserOpen {
        name: String,
        url: String,
        reply: UiReply<Result<String, String>>,
    },
}

/// The window's answer to a browser event. Dropping it answers "no page"
/// (or "refused").
pub struct UiReply<T>(Sender<T>);

impl<T> UiReply<T> {
    /// A reply and where its answer arrives (what the host does; for the
    /// app's tests).
    pub fn pair() -> (UiReply<T>, Receiver<T>) {
        let (tx, rx) = mpsc::channel();
        (UiReply(tx), rx)
    }

    pub fn answer(self, v: T) {
        let _ = self.0.send(v);
    }
}

impl<T> std::fmt::Debug for UiReply<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("UiReply")
    }
}

/// The answer to a [`ExtEvent::CapabilityRequested`]. Dropping it answers no.
pub struct ConsentReply(Sender<bool>);

impl ConsentReply {
    pub fn answer(self, granted: bool) {
        let _ = self.0.send(granted);
    }
}

impl std::fmt::Debug for ConsentReply {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ConsentReply")
    }
}

/// Where an extension is in its life.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExtState {
    /// Installed, not in the config's `extension` lines.
    Disabled,
    /// Enabled, not installed (a typo, or the folder was removed).
    Missing,
    /// Waiting for the user's consent.
    NeedsConsent,
    Starting,
    Running,
    /// Waiting to restart after a failure.
    Restarting {
        failures: usize,
    },
    /// Gave up (or the manifest/program is unusable); `Reload Config` retries.
    Failed {
        message: String,
    },
    Stopped,
}

/// How long `extension/macro.prepare` may take.
pub const MACRO_PREPARE_TIMEOUT: Duration = Duration::from_secs(10);

/// One extension, for a manage-extensions sheet or `+list-extensions`.
#[derive(Debug, Clone)]
pub struct ExtensionStatus {
    pub name: String,
    pub version: String,
    pub description: String,
    pub bundled: bool,
    pub enabled: bool,
    pub state: ExtState,
    pub requested: Vec<Capability>,
    pub granted: Vec<Capability>,
    /// Requested and not granted.
    pub missing: Vec<Capability>,
    pub commands: Vec<CommandSpec>,
    pub sources: Vec<SourceSpec>,
    pub libraries: Vec<LibrarySpec>,
    /// From the manifest (macros aren't replaced at `initialize`).
    pub sites: Vec<SiteSpec>,
    pub macros: Vec<MacroSpec>,
}

/// A palette row: run with `Host::command(&ext, &command.id, …)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaletteEntry {
    pub ext: String,
    pub command: CommandSpec,
}

/// A browser macro a running extension offers ([`Host::macros`]), with the
/// site it runs on. The app runs `spec.steps` itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacroEntry {
    pub ext: String,
    pub site: SiteSpec,
    pub spec: MacroSpec,
}

/// A library a running extension offers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryRef {
    pub ext: String,
    pub library: LibrarySpec,
}

/// Why a call into an extension didn't produce an answer.
#[derive(Debug, Clone, PartialEq)]
pub enum ExtError {
    /// Not enabled, not consented to, crashed and restarting, or stopped.
    NotRunning,
    /// No answer in time ("markdown-notes didn't answer").
    Timeout,
    /// `library.write` with a `baseHash` the document no longer has.
    Stale { current_hash: String },
    /// The extension answered with an error.
    Rpc(RpcError),
}

impl ExtError {
    /// A status-bar message naming the extension.
    pub fn message(&self, ext: &str) -> String {
        match self {
            ExtError::NotRunning => format!("{ext} isn't running"),
            ExtError::Timeout => format!("{ext} didn't answer"),
            ExtError::Stale { .. } => format!("{ext}: the note changed on disk; reopen it to edit"),
            ExtError::Rpc(e) => format!("{ext}: {}", e.message),
        }
    }

    fn of(e: RpcError) -> ExtError {
        match e.code {
            codes::TIMEOUT => ExtError::Timeout,
            codes::STALE => ExtError::Stale {
                current_hash: e
                    .data
                    .as_ref()
                    .and_then(|d| d.get("currentHash"))
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
            },
            _ => ExtError::Rpc(e),
        }
    }
}

impl std::fmt::Display for ExtError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message("the extension"))
    }
}

impl std::error::Error for ExtError {}

type Sink = Arc<dyn Fn(ExtEvent) + Send + Sync>;

enum Ctl {
    Stop,
    /// The connection of generation `n` ended.
    Exited(u64),
    /// Too many timeouts on generation `n`.
    Restart(u64),
}

#[derive(Default, Clone)]
struct Contributions {
    commands: Vec<CommandSpec>,
    sources: Vec<SourceSpec>,
    libraries: Vec<LibrarySpec>,
}

/// A running (or restarting) extension.
struct Ext {
    installed: Installed,
    granted: RwLock<Vec<Capability>>,
    settings: Mutex<BTreeMap<String, String>>,
    asked: Mutex<HashSet<String>>,
    conn: Mutex<Option<Connection>>,
    generation: AtomicU64,
    timeouts: AtomicU32,
    state: Mutex<ExtState>,
    contributions: Mutex<Contributions>,
    /// `extension/command` requests in flight (`browser.page` needs one).
    commands: AtomicU32,
    ctl: Mutex<Sender<Ctl>>,
    thread: Mutex<Option<JoinHandle<()>>>,
}

impl Ext {
    fn name(&self) -> &str {
        &self.installed.manifest.name
    }

    fn set_state(&self, s: ExtState) {
        *lock(&self.state) = s;
    }

    fn granted(&self) -> Vec<Capability> {
        self.granted
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
    }

    fn allows(&self, cap: &Capability) -> bool {
        self.granted().iter().any(|g| cap.covered_by(g))
    }

    fn connection(&self) -> Option<Connection> {
        lock(&self.conn).clone().filter(|c| !c.is_closed())
    }

    fn send_ctl(&self, c: Ctl) {
        let _ = lock(&self.ctl).send(c);
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

struct Shared {
    api: Arc<dyn HostApi>,
    sink: Sink,
}

struct State {
    config: HostConfig,
    installed: Vec<Installed>,
    diagnostics: Vec<Diagnostic>,
    running: BTreeMap<String, Arc<Ext>>,
}

struct Inner {
    shared: Arc<Shared>,
    state: Mutex<State>,
}

impl Drop for Inner {
    fn drop(&mut self) {
        let running = std::mem::take(&mut lock(&self.state).running);
        stop_all(running.into_values().collect());
    }
}

/// The extension host. Cheap to clone (a handle); dropping the last clone
/// shuts every extension down.
#[derive(Clone)]
pub struct Host {
    inner: Arc<Inner>,
}

impl Host {
    /// A host over `config`, answering extensions from `api`, reporting to
    /// `sink`. Nothing starts until [`start`](Self::start).
    pub fn new(
        config: HostConfig,
        api: Arc<dyn HostApi>,
        sink: impl Fn(ExtEvent) + Send + Sync + 'static,
    ) -> Host {
        let (installed, diagnostics) = discover(&config.bundled, config.extensions_dir.as_deref());
        Host {
            inner: Arc::new(Inner {
                shared: Arc::new(Shared {
                    api,
                    sink: Arc::new(sink),
                }),
                state: Mutex::new(State {
                    config,
                    installed,
                    diagnostics,
                    running: BTreeMap::new(),
                }),
            }),
        }
    }

    /// Start every enabled extension that has consent (the app calls this
    /// about a second after the window opens). One that asks for
    /// capabilities and has none granted gets [`ExtEvent::NeedsConsent`]
    /// instead. Already running ones are left alone.
    pub fn start(&self) {
        let mut st = lock(&self.inner.state);
        let enabled = st.config.enabled.clone();
        for name in enabled {
            if st.running.contains_key(&name) {
                continue;
            }
            self.start_one(&mut st, &name);
        }
    }

    fn start_one(&self, st: &mut State, name: &str) {
        let Some(inst) = st
            .installed
            .iter()
            .find(|i| i.manifest.name == name)
            .cloned()
        else {
            return;
        };
        let requested = inst.manifest.capabilities.clone();
        if !requested.is_empty() && !st.config.grants.has_any(name) {
            (self.inner.shared.sink)(ExtEvent::NeedsConsent {
                name: name.into(),
                requested,
            });
            return;
        }
        let granted: Vec<Capability> = st
            .config
            .grants
            .of(name)
            .iter()
            .filter(|g| requested.iter().any(|r| r.covered_by(g)))
            .cloned()
            .collect();
        let (ctl_tx, ctl_rx) = mpsc::channel();
        let ext = Arc::new(Ext {
            contributions: Mutex::new(Contributions {
                commands: inst.manifest.commands.clone(),
                sources: inst.manifest.sources.clone(),
                libraries: inst.manifest.libraries.clone(),
            }),
            installed: inst,
            granted: RwLock::new(granted),
            settings: Mutex::new(st.config.settings.get(name).cloned().unwrap_or_default()),
            asked: Mutex::new(HashSet::new()),
            conn: Mutex::new(None),
            generation: AtomicU64::new(0),
            timeouts: AtomicU32::new(0),
            state: Mutex::new(ExtState::Starting),
            commands: AtomicU32::new(0),
            ctl: Mutex::new(ctl_tx),
            thread: Mutex::new(None),
        });
        let ctx = RunCtx {
            shared: self.inner.shared.clone(),
            data_dir: st.config.data_dir.clone(),
            host: st.config.host.clone(),
            timing: st.config.timing.clone(),
        };
        let e = ext.clone();
        let t = std::thread::Builder::new()
            .name(format!("bxp-run-{name}"))
            .spawn(move || run(e, ctx, ctl_rx))
            .expect("spawn runner");
        *lock(&ext.thread) = Some(t);
        st.running.insert(name.to_string(), ext);
    }

    /// Apply a changed config (Reload Config, or after consent was written):
    /// stop what's no longer enabled, restart what's grants changed, start
    /// what's new (or failed before), and send `burrow/settingsChanged` to
    /// those whose settings changed.
    pub fn reload(&self, config: HostConfig) {
        let mut to_stop = vec![];
        {
            let mut st = lock(&self.inner.state);
            let (installed, diagnostics) =
                discover(&config.bundled, config.extensions_dir.as_deref());
            st.installed = installed;
            st.diagnostics = diagnostics;
            let names: Vec<String> = st.running.keys().cloned().collect();
            for name in names {
                let ext = st.running[&name].clone();
                let still = config.enabled.contains(&name)
                    && st
                        .installed
                        .iter()
                        .any(|i| i.manifest == ext.installed.manifest);
                let wanted: Vec<Capability> = config
                    .grants
                    .of(&name)
                    .iter()
                    .filter(|g| {
                        ext.installed
                            .manifest
                            .capabilities
                            .iter()
                            .any(|r| r.covered_by(g))
                    })
                    .cloned()
                    .collect();
                let mut had = ext.granted();
                let mut want = wanted.clone();
                had.sort();
                want.sort();
                let failed = matches!(
                    *lock(&ext.state),
                    ExtState::Failed { .. } | ExtState::Stopped
                );
                if !still || had != want || failed {
                    to_stop.push(st.running.remove(&name).expect("present"));
                    continue;
                }
                let settings = config.settings.get(&name).cloned().unwrap_or_default();
                let changed = *lock(&ext.settings) != settings;
                if changed {
                    *lock(&ext.settings) = settings.clone();
                    if let Some(c) = ext.connection() {
                        c.notify(
                            methods::SETTINGS_CHANGED,
                            &SettingsChanged {
                                settings,
                                granted: ext.granted().iter().map(Capability::as_string).collect(),
                            },
                        );
                    }
                }
            }
            st.config = config;
        }
        stop_all(to_stop);
        self.start();
    }

    /// Stop every extension (`shutdown`, a grace period, then kill).
    /// Blocks up to about `Timing::shutdown`.
    pub fn shutdown(&self) {
        let running = std::mem::take(&mut lock(&self.inner.state).running);
        stop_all(running.into_values().collect());
    }

    /// Every known extension (installed or enabled), name order.
    pub fn status(&self) -> Vec<ExtensionStatus> {
        let st = lock(&self.inner.state);
        let mut out = vec![];
        for inst in &st.installed {
            let m = &inst.manifest;
            let enabled = st.config.enabled.contains(&m.name);
            let granted = st.config.grants.of(&m.name).to_vec();
            let missing = st.config.grants.missing(&m.name, &m.capabilities);
            let (state, contrib) = match st.running.get(&m.name) {
                Some(e) => (lock(&e.state).clone(), lock(&e.contributions).clone()),
                None => (
                    if !enabled {
                        ExtState::Disabled
                    } else if !m.capabilities.is_empty() && !st.config.grants.has_any(&m.name) {
                        ExtState::NeedsConsent
                    } else {
                        ExtState::Stopped
                    },
                    Contributions {
                        commands: m.commands.clone(),
                        sources: m.sources.clone(),
                        libraries: m.libraries.clone(),
                    },
                ),
            };
            out.push(ExtensionStatus {
                name: m.name.clone(),
                version: m.version.clone(),
                description: m.description.clone(),
                bundled: matches!(inst.origin, Origin::Bundled { .. }),
                enabled,
                state,
                requested: m.capabilities.clone(),
                granted,
                missing,
                commands: contrib.commands,
                sources: contrib.sources,
                libraries: contrib.libraries,
                sites: m.sites.clone(),
                macros: m.macros.clone(),
            });
        }
        for name in &st.config.enabled {
            if !st.installed.iter().any(|i| &i.manifest.name == name) {
                out.push(ExtensionStatus {
                    name: name.clone(),
                    version: String::new(),
                    description: String::new(),
                    bundled: false,
                    enabled: true,
                    state: ExtState::Missing,
                    requested: vec![],
                    granted: vec![],
                    missing: vec![],
                    commands: vec![],
                    sources: vec![],
                    libraries: vec![],
                    sites: vec![],
                    macros: vec![],
                });
            }
        }
        out.sort_by(|a, b| a.name.cmp(&b.name));
        out
    }

    /// Problems found discovering extensions (bad manifests, name clashes).
    pub fn diagnostics(&self) -> Vec<Diagnostic> {
        lock(&self.inner.state).diagnostics.clone()
    }

    /// The Extensions palette: running extensions' commands that apply on
    /// `screen`, by extension name then manifest order.
    pub fn palette(&self, screen: Screen, has_item: bool) -> Vec<PaletteEntry> {
        let st = lock(&self.inner.state);
        let mut out = vec![];
        for (name, e) in &st.running {
            if *lock(&e.state) != ExtState::Running {
                continue;
            }
            for c in &lock(&e.contributions).commands {
                if c.applies(screen, has_item) {
                    out.push(PaletteEntry {
                        ext: name.clone(),
                        command: c.clone(),
                    });
                }
            }
        }
        out
    }

    /// The browser macros of running extensions whose site origin the user
    /// granted (`browser.automate:<origin>`), by extension name then
    /// manifest order. The palette filters them with
    /// [`MacroSpec::applies`]; a macro whose grant is missing isn't listed.
    pub fn macros(&self) -> Vec<MacroEntry> {
        let st = lock(&self.inner.state);
        let mut out = vec![];
        for (name, e) in &st.running {
            if *lock(&e.state) != ExtState::Running {
                continue;
            }
            for (spec, site) in e.installed.manifest.macros_with_sites() {
                if e.allows(&Capability::BrowserAutomate(site.origin.clone())) {
                    out.push(MacroEntry {
                        ext: name.clone(),
                        site: site.clone(),
                        spec: spec.clone(),
                    });
                }
            }
        }
        out
    }

    /// One macro of a running extension, if its site is granted: what the
    /// app checks again just before a run.
    pub fn macro_entry(&self, ext: &str, id: &str) -> Option<MacroEntry> {
        self.macros()
            .into_iter()
            .find(|m| m.ext == ext && m.spec.id == id)
    }

    /// Ask the extension to shape a macro's text
    /// (`extension/macro.prepare`, 10 s). `Ok(None)` when it doesn't
    /// implement the method: use `params.text` (the expanded template).
    pub fn macro_prepare(
        &self,
        ext: &str,
        params: &MacroPrepareParams,
    ) -> Result<Option<MacroPrepareResult>, ExtError> {
        match self.request(
            ext,
            methods::MACRO_PREPARE,
            to_value(params).map_err(ExtError::Rpc)?,
            MACRO_PREPARE_TIMEOUT,
        ) {
            Ok(r) => Ok(Some(r)),
            Err(ExtError::Rpc(e)) if e.code == codes::METHOD_NOT_FOUND => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// The libraries of running extensions, for the notes panel.
    pub fn libraries(&self) -> Vec<LibraryRef> {
        let st = lock(&self.inner.state);
        let mut out = vec![];
        for (name, e) in &st.running {
            if *lock(&e.state) != ExtState::Running {
                continue;
            }
            for l in &lock(&e.contributions).libraries {
                out.push(LibraryRef {
                    ext: name.clone(),
                    library: l.clone(),
                });
            }
        }
        out
    }

    fn ext(&self, name: &str) -> Option<Arc<Ext>> {
        lock(&self.inner.state).running.get(name).cloned()
    }

    fn request<T: DeserializeOwned>(
        &self,
        name: &str,
        method: &str,
        p: Value,
        timeout: Duration,
    ) -> Result<T, ExtError> {
        let ext = self.ext(name).ok_or(ExtError::NotRunning)?;
        let conn = ext.connection().ok_or(ExtError::NotRunning)?;
        let generation = ext.generation.load(Ordering::SeqCst);
        match conn.request(method, p, timeout) {
            Ok(v) => {
                ext.timeouts.store(0, Ordering::SeqCst);
                serde_json::from_value(v).map_err(|e| {
                    ExtError::Rpc(RpcError::new(
                        codes::INTERNAL_ERROR,
                        format!("unexpected answer: {e}"),
                    ))
                })
            }
            Err(e) if e.code == codes::TIMEOUT && !conn.is_closed() => {
                let n = ext.timeouts.fetch_add(1, Ordering::SeqCst) + 1;
                let limit = lock(&self.inner.state)
                    .config
                    .timing
                    .timeouts_before_restart;
                if n >= limit {
                    ext.timeouts.store(0, Ordering::SeqCst);
                    ext.send_ctl(Ctl::Restart(generation));
                }
                Err(ExtError::Timeout)
            }
            Err(e) if e.code == codes::TIMEOUT => Err(ExtError::NotRunning),
            Err(e) => Err(ExtError::of(e)),
        }
    }

    fn timing(&self) -> Timing {
        lock(&self.inner.state).config.timing.clone()
    }

    /// Run a palette command (blocking, up to `Timing::command`).
    pub fn command(
        &self,
        ext: &str,
        id: &str,
        context: CommandContext,
    ) -> Result<CommandResult, ExtError> {
        let p = to_value(&CommandParams {
            id: id.into(),
            context,
        })
        .map_err(ExtError::Rpc)?;
        // While it runs, the extension may read the browser pane's page.
        struct Running(Option<Arc<Ext>>);
        impl Drop for Running {
            fn drop(&mut self) {
                if let Some(e) = &self.0 {
                    e.commands.fetch_sub(1, Ordering::SeqCst);
                }
            }
        }
        let _running = Running(self.ext(ext).inspect(|e| {
            e.commands.fetch_add(1, Ordering::SeqCst);
        }));
        self.request(ext, methods::COMMAND, p, self.timing().command)
    }

    /// Search a read-only source (blocking, up to `Timing::search`).
    pub fn source_search(
        &self,
        ext: &str,
        source: &str,
        query: &str,
        limit: usize,
    ) -> Result<Vec<SourceEntry>, ExtError> {
        let p = json!(SourceSearchParams {
            source: source.into(),
            query: query.into(),
            limit
        });
        self.request(ext, methods::SOURCE_SEARCH, p, self.timing().search)
    }

    pub fn source_read(
        &self,
        ext: &str,
        source: &str,
        id: &str,
    ) -> Result<SourceDocument, ExtError> {
        let p = json!(SourceReadParams {
            source: source.into(),
            id: id.into()
        });
        self.request(ext, methods::SOURCE_READ, p, self.timing().read)
    }

    /// One folder of a library (blocking, up to `Timing::read`).
    pub fn library_list(
        &self,
        ext: &str,
        p: &LibraryListParams,
    ) -> Result<Vec<LibraryEntry>, ExtError> {
        self.request(ext, methods::LIBRARY_LIST, json!(p), self.timing().read)
    }

    /// Search a library (blocking, up to `Timing::search`). The entries'
    /// ids are library ids for [`library_read`](Self::library_read).
    pub fn library_search(
        &self,
        ext: &str,
        p: &LibrarySearchParams,
    ) -> Result<Vec<SourceEntry>, ExtError> {
        self.request(ext, methods::LIBRARY_SEARCH, json!(p), self.timing().search)
    }

    pub fn library_read(
        &self,
        ext: &str,
        p: &LibraryReadParams,
    ) -> Result<LibraryDocument, ExtError> {
        self.request(ext, methods::LIBRARY_READ, json!(p), self.timing().read)
    }

    /// Create (no `id`) or overwrite (`id` + `baseHash`) a document.
    /// [`ExtError::Stale`] when it changed since it was read.
    pub fn library_write(
        &self,
        ext: &str,
        p: &LibraryWriteParams,
    ) -> Result<LibraryWritten, ExtError> {
        self.request(ext, methods::LIBRARY_WRITE, json!(p), self.timing().read)
    }

    /// Tell extensions holding `hooks:itemPublished` that `item` was just
    /// published (call after `Backend::publish` returns `outcome`, with the
    /// item as it is now). Never blocks.
    pub fn item_published(
        &self,
        item: &blyg_core::Item,
        outcome: &blyg_core::PublishOutcome,
        note: Option<&str>,
    ) {
        let n = ItemPublished {
            item: ItemRef::of(item),
            version: outcome.version,
            permalink: outcome.permalink.clone(),
            content_md: item.content_md.clone(),
            content_hash: blyg_core::content_hash(&item.content_md),
            note: note.map(str::to_string),
            at: item.updated.clone(),
        };
        self.broadcast(&Capability::HookPublished, methods::ITEM_PUBLISHED, &n);
    }

    /// `hooks:itemSaved` (debounce in the caller: one per pause in typing).
    pub fn item_saved(&self, item: &blyg_core::Item) {
        self.broadcast(&Capability::HookSaved, methods::ITEM_SAVED, &changed(item));
    }

    /// `hooks:itemCreated`.
    pub fn item_created(&self, item: &blyg_core::Item) {
        self.broadcast(
            &Capability::HookCreated,
            methods::ITEM_CREATED,
            &changed(item),
        );
    }

    fn broadcast<P: serde::Serialize>(&self, cap: &Capability, method: &str, p: &P) {
        let exts: Vec<Arc<Ext>> = lock(&self.inner.state).running.values().cloned().collect();
        for e in exts {
            if e.allows(cap)
                && let Some(c) = e.connection()
            {
                c.notify(method, p);
            }
        }
    }
}

fn changed(item: &blyg_core::Item) -> ItemChanged {
    ItemChanged {
        item: ItemRef::of(item),
        content_md: item.content_md.clone(),
        content_hash: blyg_core::content_hash(&item.content_md),
    }
}

fn stop_all(exts: Vec<Arc<Ext>>) {
    for e in &exts {
        e.send_ctl(Ctl::Stop);
    }
    for e in exts {
        let t = lock(&e.thread).take();
        if let Some(t) = t {
            let _ = t.join();
        }
    }
}

// ------------------------------------------------------------------ runner

struct RunCtx {
    shared: Arc<Shared>,
    data_dir: PathBuf,
    host: HostInfo,
    timing: Timing,
}

impl RunCtx {
    fn emit(&self, e: ExtEvent) {
        (self.shared.sink)(e);
    }
}

struct Live {
    child: Child,
    conn: Connection,
}

fn run(ext: Arc<Ext>, ctx: RunCtx, ctl: Receiver<Ctl>) {
    let name = ext.name().to_string();
    let mut failures: Vec<Instant> = vec![];
    loop {
        ext.set_state(ExtState::Starting);
        let generation = ext.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let storage = storage_dir(&ctx.data_dir, &name);
        let _ = std::fs::create_dir_all(&storage);
        let log = StderrLog::new(storage.join("stderr.log"), STDERR_LOG_MAX);
        let message = match launch(&ext, &ctx, generation, &log) {
            Ok(mut live) => {
                ext.set_state(ExtState::Running);
                ext.timeouts.store(0, Ordering::SeqCst);
                ctx.emit(ExtEvent::Started {
                    name: name.clone(),
                    version: ext.installed.manifest.version.clone(),
                });
                let why = loop {
                    match ctl.recv() {
                        Ok(Ctl::Stop) | Err(_) => {
                            stop_child(&ext, &mut live, &ctx.timing);
                            ext.set_state(ExtState::Stopped);
                            ctx.emit(ExtEvent::Stopped {
                                name,
                                message: None,
                            });
                            return;
                        }
                        Ok(Ctl::Exited(g)) if g == generation => {
                            let status = wait_briefly(&mut live.child);
                            break format!("{name} stopped unexpectedly{status}");
                        }
                        Ok(Ctl::Restart(g)) if g == generation => {
                            kill(&mut live.child);
                            break format!("{name} stopped answering");
                        }
                        Ok(_) => {} // a previous generation's news
                    }
                };
                *lock(&ext.conn) = None;
                live.conn.close();
                why
            }
            Err(m) => m,
        };
        failures.push(Instant::now());
        failures.retain(|t| t.elapsed() < ctx.timing.failure_window);
        // Let the stderr thread catch up with a child that just died.
        std::thread::sleep(Duration::from_millis(50));
        ctx.emit(ExtEvent::Failed {
            name: name.clone(),
            message: message.clone(),
            stderr: log.tail(),
        });
        if failures.len() >= ctx.timing.max_failures {
            let m = format!(
                "{name} stopped after {} failures; Reload Config to retry",
                failures.len()
            );
            ext.set_state(ExtState::Failed { message: m.clone() });
            ctx.emit(ExtEvent::Stopped {
                name,
                message: Some(m),
            });
            // Wait for the host to let go (a Stop, or the channel closing).
            while let Ok(c) = ctl.recv() {
                if matches!(c, Ctl::Stop) {
                    break;
                }
            }
            return;
        }
        ext.set_state(ExtState::Restarting {
            failures: failures.len(),
        });
        let b = &ctx.timing.backoff;
        let wait = b
            .get(failures.len() - 1)
            .or(b.last())
            .copied()
            .unwrap_or_default();
        let deadline = Instant::now() + wait;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match ctl.recv_timeout(left) {
                Ok(Ctl::Stop) | Err(RecvTimeoutError::Disconnected) => {
                    ext.set_state(ExtState::Stopped);
                    ctx.emit(ExtEvent::Stopped {
                        name,
                        message: None,
                    });
                    return;
                }
                Ok(_) => continue,
                Err(RecvTimeoutError::Timeout) => break,
            }
        }
    }
}

fn wait_briefly(child: &mut Child) -> String {
    for _ in 0..20 {
        if let Ok(Some(s)) = child.try_wait() {
            return match s.code() {
                Some(c) => format!(" (exit code {c})"),
                None => " (killed by a signal)".into(),
            };
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    kill(child);
    String::new()
}

/// Kill and reap a child this host started.
fn kill(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

fn stop_child(ext: &Ext, live: &mut Live, timing: &Timing) {
    *lock(&ext.conn) = None;
    let _ = live
        .conn
        .request(methods::SHUTDOWN, json!({}), timing.shutdown);
    live.conn.close();
    let deadline = Instant::now() + timing.shutdown;
    while Instant::now() < deadline {
        if let Ok(Some(_)) = live.child.try_wait() {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    kill(&mut live.child);
}

fn launch(ext: &Arc<Ext>, ctx: &RunCtx, generation: u64, log: &StderrLog) -> Result<Live, String> {
    let name = ext.name().to_string();
    let m = &ext.installed.manifest;
    let storage = storage_dir(&ctx.data_dir, &name);
    std::fs::create_dir_all(&storage)
        .map_err(|e| format!("{name} didn't start: its storage folder: {e}"))?;
    let env = scrubbed_env(|k| std::env::var_os(k));
    let (program, args, cwd) = match &ext.installed.origin {
        Origin::Bundled { program, args } => (program.clone(), args.clone(), storage.clone()),
        Origin::Installed { dir } => {
            let argv0 = m.command.first().cloned().unwrap_or_default();
            let windows = cfg!(windows);
            let pathext = env
                .iter()
                .find(|(k, _)| k == "PATHEXT")
                .and_then(|(_, v)| v.to_str().map(str::to_string));
            let exts = exe_extensions(pathext.as_deref());
            let program = resolve_program(
                &argv0,
                Some(dir),
                &path_dirs(&env),
                windows,
                &exts,
                is_runnable,
            )
            .ok_or_else(|| format!("{name} didn't start: can't find {argv0:?}"))?;
            (program, m.command[1..].to_vec(), dir.clone())
        }
    };
    let mut cmd = Command::new(&program);
    cmd.args(&args)
        .current_dir(&cwd)
        .env_clear()
        .envs(env.iter().map(|(k, v)| (k.as_str(), v)))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("{name} didn't start: {e}"))?;
    let stdin = child.stdin.take().expect("piped");
    let stdout = child.stdout.take().expect("piped");
    if let Some(err) = child.stderr.take() {
        log.drain(err);
    }
    let (conn, incoming) = Connection::spawn(stdout, stdin);
    *lock(&ext.conn) = Some(conn.clone());

    // Answer the extension's requests on a thread of their own; when the
    // stream ends, tell the runner.
    {
        let ext = ext.clone();
        let conn = conn.clone();
        let shared = ctx.shared.clone();
        let consent = ctx.timing.consent;
        let browser = ctx.timing.browser;
        let _ = std::thread::Builder::new()
            .name(format!("bxp-serve-{name}"))
            .spawn(move || {
                while let Ok(msg) = incoming.recv() {
                    if let Incoming::Request { id, method, params } = msg {
                        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            answer(&ext, &shared, (consent, browser), &method, params)
                        }))
                        .unwrap_or_else(|_| {
                            Err(RpcError::new(
                                codes::INTERNAL_ERROR,
                                "Burrow failed to answer",
                            ))
                        });
                        conn.respond(id, r);
                    }
                }
                ext.send_ctl(Ctl::Exited(generation));
            });
    }

    let granted = ext.granted();
    let init = InitializeParams {
        protocol_version: PROTOCOL_VERSION,
        host: ctx.host.clone(),
        granted: granted.iter().map(Capability::as_string).collect(),
        settings: lock(&ext.settings).clone(),
        storage_dir: storage,
        blyg_origin: if granted.contains(&Capability::BlygIdentity) {
            ctx.shared.api.blyg_origin()
        } else {
            None
        },
    };
    let fail = |mut child: Child, conn: &Connection, msg: String| {
        *lock(&ext.conn) = None;
        conn.close();
        kill(&mut child);
        Err(msg)
    };
    let r = match conn.request(methods::INITIALIZE, json!(init), ctx.timing.initialize) {
        Ok(v) => v,
        Err(e) if e.code == codes::TIMEOUT && !conn.is_closed() => {
            return fail(
                child,
                &conn,
                format!("{name} didn't start: no answer to initialize"),
            );
        }
        Err(e) if e.code == codes::TIMEOUT => {
            return fail(child, &conn, format!("{name} didn't start: it exited"));
        }
        Err(e) => return fail(child, &conn, format!("{name} didn't start: {}", e.message)),
    };
    let r: InitializeResult = match params(r) {
        Ok(r) => r,
        Err(e) => return fail(child, &conn, format!("{name} didn't start: {}", e.message)),
    };
    if r.protocol_version == 0 || r.protocol_version > PROTOCOL_VERSION {
        let m = format!(
            "{name} didn't start: it speaks protocol {}",
            r.protocol_version
        );
        return fail(child, &conn, m);
    }
    {
        let mut c = lock(&ext.contributions);
        if !r.commands.is_empty() {
            c.commands = r.commands;
        }
        if !r.sources.is_empty() {
            c.sources = r.sources;
        }
        if !r.libraries.is_empty() {
            c.libraries = r.libraries;
        }
    }
    Ok(Live { child, conn })
}

/// One `burrow/*` request from an extension.
fn answer(
    ext: &Arc<Ext>,
    shared: &Shared,
    (consent, browser): (Duration, Duration),
    method: &str,
    p: Value,
) -> Result<Value, RpcError> {
    let name = ext.name().to_string();
    let granted = ext.granted();
    crate::api::check(&granted, method, &p)?;
    match method {
        methods::TOAST => {
            let p: ToastParams = params(p)?;
            (shared.sink)(ExtEvent::Toast {
                name,
                text: p.text,
                detail: p.detail,
            });
            Ok(json!({}))
        }
        methods::OPEN_ITEM => {
            let p: OpenItemParams = params(p)?;
            (shared.sink)(ExtEvent::OpenItem {
                name,
                id: blyg_core::LocalId(p.id),
            });
            Ok(json!({}))
        }
        methods::REQUEST_CAPABILITY => {
            let p: RequestCapabilityParams = params(p)?;
            let cap = Capability::parse(&p.capability).ok_or_else(|| {
                RpcError::invalid_params(format!("unknown capability {:?}", p.capability))
            })?;
            let granted = if ext.allows(&cap) {
                true
            } else if !ext
                .installed
                .manifest
                .capabilities
                .iter()
                .any(|r| cap.covered_by(r))
            {
                // Only what the manifest declares can ever be granted.
                false
            } else if !lock(&ext.asked).insert(cap.as_string()) {
                false // asked once this session already
            } else {
                let (tx, rx) = mpsc::channel();
                (shared.sink)(ExtEvent::CapabilityRequested {
                    name,
                    capability: cap.clone(),
                    reply: ConsentReply(tx),
                });
                let yes = rx.recv_timeout(consent).unwrap_or(false);
                if yes {
                    ext.granted
                        .write()
                        .unwrap_or_else(|p| p.into_inner())
                        .push(cap);
                }
                yes
            };
            to_value(&CapabilityAnswer { granted })
        }
        methods::BROWSER_PAGE => {
            let no_page = |why: &str| RpcError::new(codes::NO_PAGE, why);
            if ext.commands.load(Ordering::SeqCst) == 0 {
                return Err(no_page("only while one of its commands runs"));
            }
            let (tx, rx) = mpsc::channel();
            (shared.sink)(ExtEvent::BrowserPage {
                name,
                reply: UiReply(tx),
            });
            match rx.recv_timeout(browser) {
                Ok(Some(page)) => to_value(&page),
                _ => Err(no_page("no page open in the browser pane")),
            }
        }
        methods::BROWSER_OPEN => {
            // `check` above refused a URL off the granted origins.
            let p: BrowserOpenParams = params(p)?;
            let (tx, rx) = mpsc::channel();
            (shared.sink)(ExtEvent::BrowserOpen {
                name,
                url: p.url,
                reply: UiReply(tx),
            });
            match rx.recv_timeout(browser) {
                Ok(Ok(url)) => to_value(&BrowserOpened { url }),
                Ok(Err(why)) => Err(RpcError::new(codes::REFUSED, why)),
                Err(_) => Err(RpcError::new(
                    codes::REFUSED,
                    "the browser pane didn't open",
                )),
            }
        }
        _ => shared.api.call(&name, &granted, method, p),
    }
}
