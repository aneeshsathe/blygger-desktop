//! Which backend the app runs on, and switching it at runtime.
//!
//! The UI (main window and quick capture) holds one `Arc<dyn Backend>`: a
//! `SwitchBackend` that delegates to the current backend. Connecting or
//! disconnecting a blyg swaps what's inside, so nothing else needs rewiring.
//!
//! - `BLYGGER_FAKE=1` → the seeded `FakeBackend` (in-memory token store).
//! - a `blyg-url` in the config and a token for its host → `LiveBackend`.
//! - otherwise → `Disconnected` (empty, refuses writes) until the "Connect
//!   your blyg" sheet succeeds.

use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};

use blyg_core::config::TokenStore;
use blyg_core::*;

type Sink = Arc<dyn Fn(CoreEvent) + Send + Sync>;

/// What the switch currently holds (for the UI's own decisions).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Fake,
    Live,
    Disconnected,
}

pub struct SwitchBackend {
    inner: RwLock<(Arc<dyn Backend>, Mode)>,
    sink: Mutex<Option<Sink>>,
    /// --- blyg switcher --- Bumped on every switch. Each backend's events
    /// carry the generation it was swapped in at and are dropped once it's
    /// no longer current: a blyg switched away from finishes its last sync
    /// pass on its own thread, and that pass's status, conflicts, errors or
    /// sign-out must never reach the views of the blyg now on screen.
    generation: Arc<AtomicU64>,
    /// --- blyg switcher --- The data folder of the live backend inside,
    /// as it was opened (`set_live_dir`). Not derived from its `base_url`:
    /// that's the public origin from the blyg's permalinks, whose host can
    /// differ from the address it was opened with (a custom domain), and
    /// would mark the wrong folder as closing.
    live_dir: Mutex<Option<std::path::PathBuf>>,
}

impl SwitchBackend {
    pub fn new(inner: Arc<dyn Backend>, mode: Mode) -> Arc<SwitchBackend> {
        Arc::new(SwitchBackend {
            inner: RwLock::new((inner, mode)),
            sink: Mutex::new(None),
            generation: Arc::new(AtomicU64::new(0)),
            live_dir: Mutex::new(None),
        })
    }

    /// --- blyg switcher --- Record the data folder the live backend inside
    /// was opened on (`None` for any other backend).
    pub fn set_live_dir(&self, dir: Option<std::path::PathBuf>) {
        *self.live_dir.lock().unwrap_or_else(|p| p.into_inner()) = dir;
    }

    /// `sink`, for the backend current at generation `generation`: silent
    /// once another backend has been swapped in.
    fn gated(&self, sink: Sink, generation: u64) -> Box<dyn Fn(CoreEvent) + Send + Sync> {
        let current = self.generation.clone();
        Box::new(move |e| {
            if current.load(Ordering::SeqCst) == generation {
                sink(e)
            }
        })
    }

    fn cur(&self) -> Arc<dyn Backend> {
        self.inner
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .0
            .clone()
    }

    pub fn mode(&self) -> Mode {
        self.inner.read().unwrap_or_else(|p| p.into_inner()).1
    }

    /// Replace the backend. The event sink moves over, and the UI hears
    /// `ItemsChanged` + the new sync status. `after` runs once the old
    /// backend is gone (on a background thread: dropping a `LiveBackend`
    /// joins its sync thread), e.g. to delete its database.
    pub fn switch(
        &self,
        next: Arc<dyn Backend>,
        mode: Mode,
        after: impl FnOnce() + Send + 'static,
    ) {
        let sink = self.sink.lock().unwrap_or_else(|p| p.into_inner()).clone();
        let old = {
            let mut g = self.inner.write().unwrap_or_else(|p| p.into_inner());
            // Silence the outgoing backend before the new one can speak.
            let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
            if let Some(s) = &sink {
                next.set_event_sink(self.gated(s.clone(), generation));
            }
            std::mem::replace(&mut *g, (next.clone(), mode))
        };
        if let Some(s) = sink {
            s(CoreEvent::ItemsChanged);
            s(CoreEvent::ReadingChanged);
            s(CoreEvent::SyncStatus(next.sync_status()));
        }
        std::thread::spawn(move || {
            drop(old);
            after();
        });
    }
}

// --- blyg switcher --- (`DB_OWNER_FILE`: which blyg a data folder's database belongs to)
use blyg_core::config::blygs::{self, DB_OWNER_FILE};

/// Open the live backend for `url` if the Keychain holds a credential (its
/// studio password, or its owner token) for it.
///
/// `data_dir` is the app's data dir; the blyg's own folder in it comes from
/// [`blygs::data_dir_for`] (the data dir itself for the blyg that owns it,
/// `blygs/<host>/` for the others), so several blygs never share a database.
/// A database there that belongs to a *different* blyg is moved aside first
/// (`blygger.<host>.db`), so one blyg's unpushed drafts can never be pushed
/// to another.
pub fn open_live(
    data_dir: &Path,
    url: &str,
    tokens: &Arc<dyn TokenStore>,
) -> Result<Option<Arc<dyn Backend>>> {
    Ok(
        open_live_with(data_dir, url, tokens, blyg_core::SyncOptions::default())?
            .map(|l| Arc::new(l) as Arc<dyn Backend>),
    )
}

/// `open_live` with sync options (`blygger +import-opml` opens it without
/// the sync worker).
pub fn open_live_with(
    root: &Path,
    url: &str,
    tokens: &Arc<dyn TokenStore>,
    opts: blyg_core::SyncOptions,
) -> Result<Option<LiveBackend>> {
    let Some(cred) = blyg_core::config::load_credential(tokens, url)? else {
        return Ok(None);
    };
    let data_dir = blygs::data_dir_for(root, url);
    let data_dir = data_dir.as_path();
    // --- blyg switcher --- the same blyg, switched away from a moment ago,
    // may still be finishing its last sync pass: one sync worker per database.
    if !closing::wait(data_dir, std::time::Duration::from_secs(20)) {
        return Err(CoreError::Other(
            "that blyg is still finishing its last sync; try again in a moment".into(),
        ));
    }
    std::fs::create_dir_all(data_dir).map_err(|e| CoreError::Storage(e.to_string()))?;
    let owner = data_dir.join(DB_OWNER_FILE);
    let db = data_dir.join(blyg_core::live::DB_FILE);
    if let Ok(prev) = std::fs::read_to_string(&owner)
        && !blygs::same(prev.trim(), url)
        && db.exists()
    {
        let host = crate::vm::url_host(prev.trim()).unwrap_or_else(|| "previous".into());
        for suffix in ["", "-wal", "-shm"] {
            let from = data_dir.join(format!("{}{suffix}", blyg_core::live::DB_FILE));
            if from.exists() {
                let to = data_dir.join(format!("blygger.{host}.db{suffix}"));
                std::fs::rename(&from, &to).map_err(|e| CoreError::Storage(e.to_string()))?;
            }
        }
    }
    let live = LiveBackend::open_with(data_dir, url, cred, opts)?;
    std::fs::write(&owner, url).map_err(|e| CoreError::Storage(e.to_string()))?;
    Ok(Some(live))
}

/// Delete the local copy (database, its WAL, cached themes and images).
pub fn delete_local_data(data_dir: &Path) {
    for suffix in ["", "-wal", "-shm"] {
        let _ =
            std::fs::remove_file(data_dir.join(format!("{}{suffix}", blyg_core::live::DB_FILE)));
    }
    let _ = std::fs::remove_file(data_dir.join(DB_OWNER_FILE));
    // The retired native preview's image cache, from older versions.
    let _ = std::fs::remove_dir_all(crate::images::cache_dir(data_dir));
    crate::app::studio::style_cache::forget(data_dir);
}

/// --- blyg switcher --- Delete one blyg's local copy: its own folder under
/// `blygs/`, or the database files in the data dir itself.
pub fn delete_blyg_data(root: &Path, dir: &Path) {
    if blygs::is_sub_dir(root, dir) {
        let _ = std::fs::remove_dir_all(dir);
    } else {
        delete_local_data(dir);
    }
}

/// --- blyg switcher --- Data folders whose backend is being dropped (its
/// sync thread finishing): opening the same blyg again waits for it.
pub mod closing {
    use std::collections::HashSet;
    use std::path::{Path, PathBuf};
    use std::sync::{Condvar, LazyLock, Mutex};
    use std::time::{Duration, Instant};

    static CLOSING: LazyLock<(Mutex<HashSet<PathBuf>>, Condvar)> =
        LazyLock::new(|| (Mutex::new(HashSet::new()), Condvar::new()));

    pub fn start(dir: &Path) {
        CLOSING
            .0
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(dir.to_path_buf());
    }

    pub fn done(dir: &Path) {
        CLOSING
            .0
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(dir);
        CLOSING.1.notify_all();
    }

    /// Wait until `dir` isn't closing. False on timeout.
    pub fn wait(dir: &Path, timeout: Duration) -> bool {
        let end = Instant::now() + timeout;
        let mut g = CLOSING.0.lock().unwrap_or_else(|p| p.into_inner());
        while g.contains(dir) {
            let left = end.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return false;
            }
            g = CLOSING
                .1
                .wait_timeout(g, left)
                .unwrap_or_else(|p| p.into_inner())
                .0;
        }
        true
    }
}

// ------------------------------------------------------------ app global

/// Checks a URL and credential before anything is saved (sign in, then
/// `GET /api/items`).
pub type Verifier = fn(
    &str,
    &blyg_core::api::auth::Credential,
) -> std::result::Result<usize, blyg_core::api::ConnectError>;

/// The real check.
pub fn live_verifier(
    url: &str,
    cred: &blyg_core::api::auth::Credential,
) -> std::result::Result<usize, blyg_core::api::ConnectError> {
    blyg_core::api::verify_connection(url, cred.clone())
}

/// The app's connection state (absent in headless tests, which run on a
/// `FakeBackend` directly).
pub struct Connection {
    pub switch: Arc<SwitchBackend>,
    pub data_dir: std::path::PathBuf,
    pub verify: Verifier,
}

impl gpui_kit::Global for Connection {}

pub fn mode(cx: &gpui_kit::App) -> Option<Mode> {
    cx.try_global::<Connection>().map(|c| c.switch.mode())
}

/// The connect sheet's check. Without the global (tests) it never touches
/// the network.
pub fn verifier(cx: &gpui_kit::App) -> Verifier {
    cx.try_global::<Connection>()
        .map(|c| c.verify)
        .unwrap_or(|_, _| Ok(0))
}

/// Verifier for fake mode: never touches the network.
pub fn fake_verifier(
    _: &str,
    _: &blyg_core::api::auth::Credential,
) -> std::result::Result<usize, blyg_core::api::ConnectError> {
    Ok(6)
}

/// After a successful connect: switch to the live backend for `url`.
/// `Ok(false)` when there's nothing to switch (fake mode, tests).
pub fn go_live(url: &str, cx: &mut gpui_kit::App) -> Result<bool> {
    let Some(conn) = cx.try_global::<Connection>() else {
        return Ok(false);
    };
    // --- onboarding --- (a sample-data session still goes live on connect)
    if conn.switch.mode() == Mode::Fake
        && !cx.has_global::<SampleSession>()
        && !cx.has_global::<BlygOpener>()
    {
        return Ok(false);
    }
    let (switch, root) = (conn.switch.clone(), conn.data_dir.clone());
    match open_blyg(url, cx)? {
        Some((next, mode)) => {
            swap(&switch, &root, next, mode, Some(url), || {});
            crate::refresh_menus(cx); // --- blyg switcher ---
            Ok(true)
        }
        None => Err(CoreError::Other("the token wasn't saved".into())),
    }
}

// --- blyg switcher ---

/// How a blyg's backend is opened. The app opens a `LiveBackend`; fake mode
/// and tests set a [`BlygOpener`] instead.
pub type Opener = Arc<dyn Fn(&str) -> Result<Option<(Arc<dyn Backend>, Mode)>> + Send + Sync>;

/// Opens blygs instead of `open_live` (fake mode, tests).
pub struct BlygOpener(pub Opener);

impl gpui_kit::Global for BlygOpener {}

/// Open `url`'s backend: `None` when there's no credential for it.
fn open_blyg(url: &str, cx: &gpui_kit::App) -> Result<Option<(Arc<dyn Backend>, Mode)>> {
    if let Some(o) = cx.try_global::<BlygOpener>() {
        return (o.0)(url);
    }
    let Some(conn) = cx.try_global::<Connection>() else {
        return Ok(None);
    };
    let tokens = crate::settings::get(cx).tokens.clone();
    Ok(open_live(&conn.data_dir, url, &tokens)?.map(|b| (b, Mode::Live)))
}

/// Swap in `next` (opened for `url`). The live backend going out is marked
/// closing until its sync thread has finished (`after` runs first), so
/// reopening that blyg right away waits for it instead of running two sync
/// workers on one database.
fn swap(
    switch: &SwitchBackend,
    root: &Path,
    next: Arc<dyn Backend>,
    mode: Mode,
    url: Option<&str>,
    after: impl FnOnce() + Send + 'static,
) {
    let next_dir = url
        .filter(|_| mode == Mode::Live)
        .map(|u| blygs::data_dir_for(root, u));
    let old_dir = std::mem::replace(
        &mut *switch.live_dir.lock().unwrap_or_else(|p| p.into_inner()),
        next_dir,
    )
    .filter(|_| switch.mode() == Mode::Live);
    if let Some(d) = &old_dir {
        closing::start(d);
    }
    switch.switch(next, mode, move || {
        after();
        if let Some(d) = old_dir {
            closing::done(&d);
        }
    });
}

/// What [`switch_to`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Switched {
    /// It's the active blyg already.
    Same,
    Changed,
    /// No sign-in for it in the Keychain: sign in again (the Connect sheet).
    NeedsSignIn,
}

/// Make `url` the active blyg: open its backend, point `blyg-url` at it, and
/// swap it in. The blyg switched away from is closed: its sync worker
/// finishes the pass it's in and stops, and anything still in its outbox
/// waits in its own database until it's active again.
pub fn switch_to(url: &str, cx: &mut gpui_kit::App) -> std::result::Result<Switched, String> {
    let Some(conn) = cx.try_global::<Connection>() else {
        return Err("Not connected".into());
    };
    let (switch, root) = (conn.switch.clone(), conn.data_dir.clone());
    let current = crate::settings::blyg_url(cx);
    if current.as_deref().is_some_and(|c| blygs::same(c, url))
        && switch.mode() != Mode::Disconnected
    {
        return Ok(Switched::Same);
    }
    let host = crate::vm::url_host(url).unwrap_or_else(|| url.to_string());
    let Some((next, mode)) =
        open_blyg(url, cx).map_err(|e| format!("Couldn't open {host}: {e}"))?
    else {
        return Ok(Switched::NeedsSignIn);
    };
    let cfg = crate::settings::get(cx).store.config().clone();
    crate::settings::write(&blygs::activate(&cfg, url), cx)?;
    swap(&switch, &root, next, mode, Some(url), || {});
    crate::refresh_menus(cx);
    Ok(Switched::Changed)
}

// --- onboarding ---
impl SwitchBackend {
    /// What the switch holds right now (the tutorial parks it and puts it back).
    pub fn current(&self) -> Arc<dyn Backend> {
        self.cur()
    }
}

/// This session runs on sample data because onboarding's "just try it"
/// chose it (not `BLYGGER_FAKE`): connecting a blyg still goes live.
pub struct SampleSession;

impl gpui_kit::Global for SampleSession {}

/// Onboarding's "Skip, just try it with sample data": run this session on
/// `fake`. False without the app global (headless tests). Already fake
/// (`BLYGGER_FAKE`) → nothing to switch.
pub fn use_sample_data(fake: Arc<dyn Backend>, cx: &mut gpui_kit::App) -> bool {
    let Some(conn) = cx.try_global::<Connection>() else {
        return false;
    };
    if conn.switch.mode() != Mode::Fake {
        conn.switch.clone().switch(fake, Mode::Fake, || {});
        cx.set_global(SampleSession);
    }
    true
}
// --- end onboarding ---

/// What signing out of a blyg did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignedOut {
    pub host: String,
    /// It was the active blyg.
    pub was_active: bool,
    /// The active blyg now, if any.
    pub active: Option<String>,
}

/// Forget the password and token (Keychain) and `blyg-url` (config); keep or delete the
/// local copy. Returns the host that was disconnected.
pub fn disconnect(
    delete_local: bool,
    cx: &mut gpui_kit::App,
) -> std::result::Result<String, String> {
    let url = crate::settings::blyg_url(cx).ok_or("Not connected to a blyg")?;
    disconnect_blyg(&url, delete_local, cx).map(|s| s.host)
}

/// Sign out of one blyg: forget its password and token (Keychain), take it
/// off the list (config), and keep or delete its local copy. Signing out of
/// the active blyg switches to the next one in the list, or to nothing.
pub fn disconnect_blyg(
    url: &str,
    delete_local: bool,
    cx: &mut gpui_kit::App,
) -> std::result::Result<SignedOut, String> {
    let url = url.to_string();
    let host = crate::vm::url_host(&url).unwrap_or(url.clone());
    let tokens = crate::settings::get(cx).tokens.clone();
    // A browser sign-in is revoked at the blyg too (best effort, off the
    // UI thread; offline, it just lapses within 30 days).
    if let Ok(Some(grant)) = blyg_core::config::load_grant(tokens.as_ref(), &url)
        && !grant.ended()
    {
        std::thread::spawn(move || {
            if let Err(e) = blyg_core::api::oauth::revoke(&grant) {
                eprintln!("blygger: couldn't revoke the browser sign-in: {e}");
            }
        });
    }
    blyg_core::config::delete_credential(tokens.as_ref(), &url)
        .map_err(|e| format!("Couldn't remove it from the Keychain: {e}"))?;
    let cfg = crate::settings::get(cx).store.config().clone();
    let was_active = cfg.blyg_url().is_some_and(|a| blygs::same(a, &url));
    let (changes, next) = blygs::remove(&cfg, &url);
    crate::settings::write(&changes, cx)?;
    crate::refresh_menus(cx);
    let out = SignedOut {
        host,
        was_active,
        active: next.clone(),
    };
    let Some(conn) = cx.try_global::<Connection>() else {
        return Ok(out);
    };
    let (switch, root) = (conn.switch.clone(), conn.data_dir.clone());
    // Fake mode never deletes real files.
    let fake = switch.mode() == Mode::Fake || cx.has_global::<BlygOpener>();
    let dir = blygs::data_dir_for(&root, &url);
    let wipe = {
        let (root, dir) = (root.clone(), dir.clone());
        move || {
            if delete_local && !fake {
                delete_blyg_data(&root, &dir);
            }
        }
    };
    if was_active {
        let target = match &next {
            Some(n) => open_blyg(n, cx).ok().flatten(),
            None => None,
        };
        match target {
            Some((b, m)) => swap(&switch, &root, b, m, next.as_deref(), wipe),
            None if switch.mode() != Mode::Fake => swap(
                &switch,
                &root,
                Arc::new(Disconnected),
                Mode::Disconnected,
                None,
                wipe,
            ),
            None => {}
        }
    } else if delete_local {
        std::thread::spawn(move || {
            closing::wait(&dir, std::time::Duration::from_secs(30));
            wipe();
        });
    }
    Ok(out)
}

/// --- blyg switcher --- Fake mode: each blyg is its own `FakeBackend`, kept
/// for the session. The active one (`active`) is `initial`; every other gets
/// a few posts of its own over the same sample reading.
pub fn fake_opener(initial: Arc<crate::fake::FakeBackend>, active: Option<&str>) -> Opener {
    use std::collections::HashMap;
    let mut fakes: HashMap<String, Arc<crate::fake::FakeBackend>> = HashMap::new();
    if let Some(a) = active {
        fakes.insert(blygs::identity(a), initial);
    }
    let fakes = Mutex::new(fakes);
    Arc::new(move |url: &str| {
        let mut g = fakes.lock().unwrap_or_else(|p| p.into_inner());
        let f = g
            .entry(blygs::identity(url))
            .or_insert_with(|| {
                let f = Arc::new(crate::fake::FakeBackend::new());
                f.replace_posts(&[
                    (
                        Kind::Fragment,
                        "Field note: the heron was back at the weir this morning, standing like a question mark.",
                    ),
                    (
                        Kind::Thread,
                        "Tide tables\n\nA list of the low tides worth walking out for this month, with the caveat that the sea keeps its own counsel.",
                    ),
                    (
                        Kind::Fragment,
                        "Moss grows fastest where nobody walks. Make of that what you will.",
                    ),
                ]);
                f
            })
            .clone();
        Ok(Some((f as Arc<dyn Backend>, Mode::Fake)))
    })
}

/// Nothing connected yet: an empty list, and every write explains why.
pub struct Disconnected;

fn not_connected<T>() -> Result<T> {
    Err(CoreError::Other("Connect your blyg first".into()))
}

impl Backend for Disconnected {
    fn items(&self) -> Vec<Item> {
        vec![]
    }
    fn search(&self, _: &str) -> Vec<Item> {
        vec![]
    }
    fn item(&self, _: &LocalId) -> Option<Item> {
        None
    }
    fn create_draft(&self, _: Kind, _: &str) -> Result<LocalId> {
        not_connected()
    }
    fn save(&self, _: &LocalId, _: &str) -> Result<()> {
        not_connected()
    }
    fn save_if_base(&self, _: &LocalId, _: &str, _: &str) -> Result<()> {
        not_connected()
    }
    fn set_kind(&self, _: &LocalId, _: Kind) -> Result<()> {
        not_connected()
    }
    fn sync_status(&self) -> SyncStatus {
        SyncStatus::Synced
    }
    fn set_event_sink(&self, _: Box<dyn Fn(CoreEvent) + Send + Sync>) {}
    fn resolve_conflict(&self, _: &LocalId, _: Resolution) -> Result<()> {
        not_connected()
    }
    fn reading(&self) -> Vec<ReadingItem> {
        vec![]
    }
    fn subscriptions(&self) -> Vec<Subscription> {
        vec![]
    }
    fn publish(&self, _: &LocalId, _: Option<&str>) -> Result<PublishOutcome> {
        not_connected()
    }
    fn withdraw(&self, _: &LocalId, _: Option<&str>) -> Result<u32> {
        not_connected()
    }
    fn pin(&self, _: &LocalId, _: u32) -> Result<()> {
        not_connected()
    }
    fn versions(&self, _: &LocalId) -> Result<Vec<Version>> {
        Ok(vec![])
    }
    fn restore(&self, _: &LocalId, _: u32) -> Result<()> {
        not_connected()
    }
    fn delete_draft(&self, _: &LocalId) -> Result<()> {
        not_connected()
    }
    fn upload_media(
        &self,
        _: Vec<u8>,
        _: &str,
        _: Option<&LocalId>,
        _: Option<&str>,
    ) -> Result<MediaRef> {
        not_connected()
    }
    fn fork(&self, _: &RemoteRef) -> Result<LocalId> {
        not_connected()
    }
    fn set_responses(&self, _: &LocalId, _: ResponsesMode) -> Result<bool> {
        not_connected()
    }
    fn set_highlight(&self, _: &LocalId, _: HighlightMode) -> Result<bool> {
        not_connected()
    }
    fn fetch_own_media(&self, _: &str) -> Result<(Vec<u8>, Option<String>)> {
        not_connected()
    }
    fn sync_now(&self) -> Result<()> {
        Ok(())
    }
    fn preview_subscription(&self, _: &str) -> Result<SubscribePreview> {
        not_connected()
    }
    fn subscribe(&self, _: &str, _: Option<&str>) -> Result<Subscription> {
        not_connected()
    }
    fn unsubscribe(&self, _: &str) -> Result<()> {
        not_connected()
    }
    fn set_subscription(&self, _: &str, _: Option<bool>, _: Option<&str>) -> Result<()> {
        not_connected()
    }
    fn pause_subscription(&self, _: &str, _: bool) -> Result<()> {
        not_connected()
    }
    fn signal(&self, _: &str, _: &str, _: Option<i8>) -> Result<()> {
        not_connected()
    }
    fn mentions(&self) -> Result<Vec<Mention>> {
        Ok(vec![])
    }
    fn set_mention_hidden(&self, _: &str, _: bool) -> Result<()> {
        not_connected()
    }
    fn settings(&self) -> Result<Settings> {
        not_connected()
    }
    fn save_settings(&self, _: &Settings) -> Result<()> {
        not_connected()
    }
}

/// Every method delegates to the current backend.
impl Backend for SwitchBackend {
    fn items(&self) -> Vec<Item> {
        self.cur().items()
    }
    fn search(&self, q: &str) -> Vec<Item> {
        self.cur().search(q)
    }
    // --- contacts ---
    fn contacts(&self) -> Vec<blyg_core::contacts::Contact> {
        self.cur().contacts()
    }
    fn refresh_contacts(&self) -> blyg_core::Result<()> {
        self.cur().refresh_contacts()
    }
    fn rewrites_handles(&self) -> bool {
        self.cur().rewrites_handles()
    }
    fn reading_mentions(&self) -> Option<Vec<(String, String)>> {
        self.cur().reading_mentions()
    }
    fn page_title(&self, url: &str) -> Option<String> {
        self.cur().page_title(url) // --- capture ---
    }
    fn loose_facts(&self) -> blyg_core::loose::Facts {
        self.cur().loose_facts() // --- loose ends ---
    }
    fn item(&self, id: &LocalId) -> Option<Item> {
        self.cur().item(id)
    }
    fn create_draft(&self, kind: Kind, content_md: &str) -> Result<LocalId> {
        self.cur().create_draft(kind, content_md)
    }
    fn save(&self, id: &LocalId, content_md: &str) -> Result<()> {
        self.cur().save(id, content_md)
    }
    /// The inner backend's own compare-and-save (atomic under its lock),
    /// not the trait default's compare-then-save.
    fn save_if_base(&self, id: &LocalId, content_md: &str, base_hash: &str) -> Result<()> {
        self.cur().save_if_base(id, content_md, base_hash)
    }
    /// The inner backend's own off-thread write (`LiveBackend`'s typing
    /// writer), not the trait default's `save` on the UI thread.
    fn save_typing(&self, id: &LocalId, content_md: &str) -> Result<()> {
        self.cur().save_typing(id, content_md)
    }
    fn own_item(&self, server_id: &str) -> Option<Item> {
        self.cur().own_item(server_id)
    }
    fn reading_with_id(&self, remote_id: &str) -> Vec<ReadingItem> {
        self.cur().reading_with_id(remote_id)
    }
    fn save_scratch_media(&self, bytes: &[u8], mime: &str) -> Result<String> {
        self.cur().save_scratch_media(bytes, mime)
    }
    fn scratch_media_file(&self, url: &str) -> Option<std::path::PathBuf> {
        self.cur().scratch_media_file(url)
    }
    fn set_kind(&self, id: &LocalId, kind: Kind) -> Result<()> {
        self.cur().set_kind(id, kind)
    }
    fn save_with_provenance(
        &self,
        id: &LocalId,
        content_md: &str,
        scopes: &[Option<ScopeProvenance>],
    ) -> Result<()> {
        self.cur().save_with_provenance(id, content_md, scopes)
    }
    // --- AI ---
    fn tracked_tk_provenance(
        &self,
        id: &LocalId,
    ) -> Option<(String, Vec<Option<ScopeProvenance>>)> {
        self.cur().tracked_tk_provenance(id)
    }
    fn provenance_available(&self) -> bool {
        self.cur().provenance_available()
    }
    fn sync_status(&self) -> SyncStatus {
        self.cur().sync_status()
    }
    // --- scratch notes ---
    fn create_scratch(&self, kind: Kind, content_md: &str) -> Result<LocalId> {
        self.cur().create_scratch(kind, content_md)
    }
    fn promote(&self, id: &LocalId, to: blyg_core::Promote) -> Result<blyg_core::Promoted> {
        self.cur().promote(id, to)
    }
    fn base_url(&self) -> Option<String> {
        self.cur().base_url()
    }
    fn set_event_sink(&self, sink: Box<dyn Fn(CoreEvent) + Send + Sync>) {
        let sink: Sink = Arc::from(sink);
        *self.sink.lock().unwrap_or_else(|p| p.into_inner()) = Some(sink.clone());
        // Under the read lock, so no switch slips in between.
        let g = self.inner.read().unwrap_or_else(|p| p.into_inner());
        let generation = self.generation.load(Ordering::SeqCst);
        g.0.set_event_sink(self.gated(sink, generation));
    }
    fn resolve_conflict(&self, id: &LocalId, how: Resolution) -> Result<()> {
        self.cur().resolve_conflict(id, how)
    }
    fn reading(&self) -> Vec<ReadingItem> {
        self.cur().reading()
    }
    fn unread_count(&self) -> usize {
        self.cur().unread_count()
    }
    fn subscriptions(&self) -> Vec<Subscription> {
        self.cur().subscriptions()
    }
    fn mark_read(&self, sub_id: &str, remote_id: &str) -> Result<()> {
        self.cur().mark_read(sub_id, remote_id)
    }
    fn set_read(&self, rows: &[(String, String)], read: bool) -> Result<usize> {
        self.cur().set_read(rows, read)
    }
    fn publish(&self, id: &LocalId, note: Option<&str>) -> Result<PublishOutcome> {
        self.cur().publish(id, note)
    }
    fn withdraw(&self, id: &LocalId, note: Option<&str>) -> Result<u32> {
        self.cur().withdraw(id, note)
    }
    fn pin(&self, id: &LocalId, version: u32) -> Result<()> {
        self.cur().pin(id, version)
    }
    fn versions(&self, id: &LocalId) -> Result<Vec<Version>> {
        self.cur().versions(id)
    }
    fn restore(&self, id: &LocalId, version: u32) -> Result<()> {
        self.cur().restore(id, version)
    }
    fn delete_draft(&self, id: &LocalId) -> Result<()> {
        self.cur().delete_draft(id)
    }
    fn upload_media(
        &self,
        bytes: Vec<u8>,
        mime: &str,
        item: Option<&LocalId>,
        alt: Option<&str>,
    ) -> Result<MediaRef> {
        self.cur().upload_media(bytes, mime, item, alt)
    }
    fn delete_media(&self, url: &str) -> Result<()> {
        self.cur().delete_media(url)
    }
    fn fork(&self, of: &RemoteRef) -> Result<LocalId> {
        self.cur().fork(of)
    }
    fn set_responses(&self, id: &LocalId, mode: ResponsesMode) -> Result<bool> {
        self.cur().set_responses(id, mode)
    }
    fn set_highlight(&self, id: &LocalId, mode: HighlightMode) -> Result<bool> {
        self.cur().set_highlight(id, mode)
    }
    fn pick_search(&self, q: &blyg_core::PickQuery) -> Vec<blyg_core::Pickable> {
        self.cur().pick_search(q)
    }
    fn picker_typing(&self) -> PickerTyping {
        self.cur().picker_typing()
    }
    fn fetch_own_media(&self, url: &str) -> Result<(Vec<u8>, Option<String>)> {
        self.cur().fetch_own_media(url)
    }
    fn sync_now(&self) -> Result<()> {
        self.cur().sync_now()
    }
    fn preview_subscription(&self, url: &str) -> Result<SubscribePreview> {
        self.cur().preview_subscription(url)
    }
    fn subscribe(&self, url: &str, title: Option<&str>) -> Result<Subscription> {
        self.cur().subscribe(url, title)
    }
    fn unsubscribe(&self, sub_id: &str) -> Result<()> {
        self.cur().unsubscribe(sub_id)
    }
    fn set_subscription(
        &self,
        sub_id: &str,
        in_blogroll: Option<bool>,
        title: Option<&str>,
    ) -> Result<()> {
        self.cur().set_subscription(sub_id, in_blogroll, title)
    }
    fn pause_subscription(&self, sub_id: &str, paused: bool) -> Result<()> {
        self.cur().pause_subscription(sub_id, paused)
    }
    fn rename_subscription(&self, sub_id: &str, title: Option<&str>) -> Result<()> {
        self.cur().rename_subscription(sub_id, title)
    }
    fn poll_subscriptions(&self) -> Result<u32> {
        self.cur().poll_subscriptions()
    }
    fn check_subscription(&self, sub_id: &str) -> Result<bool> {
        self.cur().check_subscription(sub_id)
    }
    fn signal(&self, sub_id: &str, remote_id: &str, thumb: Option<i8>) -> Result<()> {
        self.cur().signal(sub_id, remote_id, thumb)
    }
    fn mentions(&self) -> Result<Vec<Mention>> {
        self.cur().mentions()
    }
    fn set_mention_hidden(&self, mention_id: &str, hidden: bool) -> Result<()> {
        self.cur().set_mention_hidden(mention_id, hidden)
    }
    fn settings(&self) -> Result<Settings> {
        self.cur().settings()
    }
    fn save_settings(&self, settings: &Settings) -> Result<()> {
        self.cur().save_settings(settings)
    }
    fn remote_versions(&self, sub_id: &str, remote_id: &str) -> Result<Vec<RemoteVersion>> {
        self.cur().remote_versions(sub_id, remote_id)
    }
    fn remote_shown_versions(&self, sub_id: &str, remote_id: &str) -> Result<Vec<RemoteVersion>> {
        self.cur().remote_shown_versions(sub_id, remote_id)
    }
    fn remote_pinned(&self, sub_id: &str, remote_id: &str, version: u32) -> Result<PinnedVersion> {
        self.cur().remote_pinned(sub_id, remote_id, version)
    }
    fn pinned_diff_base(&self, sub_id: &str, remote_id: &str) -> Option<PinnedVersion> {
        self.cur().pinned_diff_base(sub_id, remote_id)
    }
    // --- reading ---
    fn read_extensions_available(&self) -> bool {
        self.cur().read_extensions_available()
    }
    fn create_stub(&self, of: &RemoteRef, content_md: &str) -> Result<LocalId> {
        self.cur().create_stub(of, content_md)
    }
    // --- profiles ---
    fn profile(&self, url: &str, refresh: bool) -> Result<blyg_core::Profile> {
        self.cur().profile(url, refresh)
    }
    fn cached_profile(&self, url: &str) -> Option<blyg_core::Profile> {
        self.cur().cached_profile(url)
    }
    // --- about ---
    fn read_state_sync(&self) -> bool {
        self.cur().read_state_sync()
    }

    fn server_outdated(&self) -> bool {
        self.cur().server_outdated()
    }

    fn server_extensions(&self) -> Option<bool> {
        self.cur().server_extensions()
    }

    fn cached_profiles(&self) -> Vec<blyg_core::Profile> {
        self.cur().cached_profiles()
    }

    // --- responses ---
    fn responses(&self, origin: &str, id: &str) -> Vec<blyg_core::Response> {
        self.cur().responses(origin, id)
    }
    // --- reader folders ---
    fn folders(&self) -> Vec<blyg_core::Folder> {
        self.cur().folders()
    }
    fn subscription_folders(&self) -> std::collections::HashMap<String, String> {
        self.cur().subscription_folders()
    }
    fn create_folder(&self, name: &str) -> Result<blyg_core::Folder> {
        self.cur().create_folder(name)
    }
    fn rename_folder(&self, id: &str, name: &str) -> Result<()> {
        self.cur().rename_folder(id, name)
    }
    fn delete_folder(&self, id: &str) -> Result<()> {
        self.cur().delete_folder(id)
    }
    fn move_folder(&self, id: &str, index: usize) -> Result<()> {
        self.cur().move_folder(id, index)
    }
    fn set_subscription_folder(&self, sub_id: &str, folder: Option<&str>) -> Result<()> {
        self.cur().set_subscription_folder(sub_id, folder)
    }
    // --- quote targets ---
    fn public_item(&self, origin: &str, id: &str) -> Result<blyg_core::PublicItem> {
        self.cur().public_item(origin, id)
    }
    fn public_pinned(&self, origin: &str, id: &str, version: u32) -> Result<PinnedVersion> {
        self.cur().public_pinned(origin, id, version)
    }
    // --- conflicts ---
    fn conflict_texts(&self, id: &LocalId) -> Option<(String, String)> {
        self.cur().conflict_texts(id)
    }
    // --- nearby ---
    fn nearby_vectors(&self, model: &str) -> Vec<blyg_core::nearby::NearbyVector> {
        self.cur().nearby_vectors(model)
    }
    fn put_nearby_vectors(
        &self,
        model: &str,
        docs: &[String],
        rows: &[blyg_core::nearby::NearbyVector],
    ) -> Result<()> {
        self.cur().put_nearby_vectors(model, docs, rows)
    }
    fn retain_nearby_vectors(
        &self,
        model: &str,
        keep: &std::collections::HashSet<String>,
    ) -> Result<()> {
        self.cur().retain_nearby_vectors(model, keep)
    }
    fn nearby_phrase_hits(&self, phrases: &[String]) -> Vec<(String, usize)> {
        self.cur().nearby_phrase_hits(phrases)
    }
    // --- end nearby ---
    // --- background ---
    fn background_reading(&self) -> Vec<ReadingItem> {
        self.cur().background_reading()
    }
    // --- hoppers ---
    fn hoppers(&self) -> Vec<Hopper> {
        self.cur().hoppers()
    }
    fn hopper_members(&self) -> std::collections::HashMap<(String, String), Vec<String>> {
        self.cur().hopper_members()
    }
    fn hopper_unheld(&self) -> Vec<ReadingItem> {
        self.cur().hopper_unheld()
    }
    fn hopper_order(&self) -> std::collections::HashMap<String, Vec<(String, String)>> {
        self.cur().hopper_order()
    }
    fn create_hopper(&self, name: &str) -> Result<Hopper> {
        self.cur().create_hopper(name)
    }
    fn update_hopper(&self, id: &str, edit: &HopperEdit) -> Result<Hopper> {
        self.cur().update_hopper(id, edit)
    }
    fn delete_hopper(&self, id: &str) -> Result<()> {
        self.cur().delete_hopper(id)
    }
    fn set_in_hopper(&self, hopper: &str, sub_id: &str, remote_id: &str, on: bool) -> Result<()> {
        self.cur().set_in_hopper(hopper, sub_id, remote_id, on)
    }
    // --- shelf --- (was missing: through the switcher the shelf read empty
    // and refused changes)
    fn shelf(&self) -> Vec<LocalId> {
        self.cur().shelf()
    }
    fn set_on_shelf(&self, id: &LocalId, on: bool) -> Result<()> {
        self.cur().set_on_shelf(id, on)
    }
    fn move_on_shelf(&self, id: &LocalId, index: usize) -> Result<()> {
        self.cur().move_on_shelf(id, index)
    }
    // --- trails ---
    fn trails(&self) -> Vec<blyg_core::Trail> {
        self.cur().trails()
    }
    fn create_trail(&self, name: &str) -> Result<blyg_core::Trail> {
        self.cur().create_trail(name)
    }
    fn rename_trail(&self, id: &str, name: &str) -> Result<()> {
        self.cur().rename_trail(id, name)
    }
    fn delete_trail(&self, id: &str) -> Result<()> {
        self.cur().delete_trail(id)
    }
    fn trail_stops(&self, trail: &str) -> Vec<blyg_core::TrailStop> {
        self.cur().trail_stops(trail)
    }
    fn add_trail_stop(&self, trail: &str, stop: &blyg_core::TrailStop) -> Result<usize> {
        self.cur().add_trail_stop(trail, stop)
    }
    fn set_trail_why(&self, trail: &str, index: usize, why: &str) -> Result<()> {
        self.cur().set_trail_why(trail, index, why)
    }
    fn remove_trail_stop(&self, trail: &str, index: usize) -> Result<()> {
        self.cur().remove_trail_stop(trail, index)
    }
    fn move_trail_stop(&self, trail: &str, from: usize, to: usize) -> Result<()> {
        self.cur().move_trail_stop(trail, from, to)
    }
    // --- browse ---
    fn classify_links(&self, urls: &[String]) -> Result<Vec<blyg_core::browse::LinkTarget>> {
        self.cur().classify_links(urls)
    }
    fn log_browse(&self, step: &blyg_core::browse::NewBrowseStep) -> Result<i64> {
        self.cur().log_browse(step)
    }
    fn browse_history(
        &self,
        trail: Option<&str>,
        offset: usize,
        limit: usize,
    ) -> Result<Vec<blyg_core::browse::BrowseStep>> {
        self.cur().browse_history(trail, offset, limit)
    }
    fn clear_browse(&self, trail: Option<&str>) -> Result<()> {
        self.cur().clear_browse(trail)
    }
    // --- lineage counts ---
    fn lineage_summaries(
        &self,
        keys: &[String],
        max_age_ms: i64,
    ) -> Option<std::collections::HashMap<String, blyg_core::lineage::LineageSummary>> {
        self.cur().lineage_summaries(keys, max_age_ms)
    }
    fn cached_lineage_summaries(
        &self,
        keys: &[String],
    ) -> Option<std::collections::HashMap<String, blyg_core::lineage::LineageSummary>> {
        self.cur().cached_lineage_summaries(keys)
    }
    fn lineage_graph(
        &self,
        centre: &blyg_core::lineage::Centre,
        max_age_ms: i64,
    ) -> Option<blyg_core::lineage::LineageGraph> {
        self.cur().lineage_graph(centre, max_age_ms)
    }
    fn cached_lineage_graph(
        &self,
        centre: &blyg_core::lineage::Centre,
    ) -> Option<blyg_core::lineage::LineageGraph> {
        self.cur().cached_lineage_graph(centre)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn switching_moves_the_sink_and_announces_itself() {
        let sw = SwitchBackend::new(Arc::new(Disconnected), Mode::Disconnected);
        let seen = Arc::new(Mutex::new(Vec::new()));
        let s2 = seen.clone();
        sw.set_event_sink(Box::new(move |e| s2.lock().unwrap().push(e)));
        assert!(sw.create_draft(Kind::Fragment, "x").is_err());
        let fake = Arc::new(crate::fake::FakeBackend::with_timing(
            crate::fake::Timing::instant(),
        ));
        let (tx, rx) = std::sync::mpsc::channel();
        sw.switch(fake.clone(), Mode::Fake, move || tx.send(()).unwrap());
        rx.recv_timeout(std::time::Duration::from_secs(5))
            .expect("`after` ran once the old backend was dropped");
        assert_eq!(sw.mode(), Mode::Fake);
        assert!(!sw.items().is_empty());
        let ev = seen.lock().unwrap().clone();
        assert!(ev.contains(&CoreEvent::ItemsChanged), "{ev:?}");
        // Events from the new backend reach the same sink.
        let id = sw.create_draft(Kind::Fragment, "hello").unwrap();
        assert!(sw.item(&id).is_some());
    }

    /// --- blyg switcher --- A blyg switched away from may still be
    /// finishing a sync pass; nothing it says reaches the window after.
    #[test]
    fn a_backend_switched_away_from_is_silenced() {
        // Two real backends on real SQLite, pointed at a closed local port:
        // every sync fails fast and says so through the sink.
        let open = |dir: &Path| -> Arc<LiveBackend> {
            let opts = blyg_core::SyncOptions {
                start_worker: false,
                ..Default::default()
            };
            Arc::new(
                LiveBackend::open_with(
                    dir,
                    "http://127.0.0.1:9",
                    blyg_core::api::auth::Credential::Token("t".into()),
                    opts,
                )
                .unwrap(),
            )
        };
        let (d1, d2) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let (one, two) = (open(d1.path()), open(d2.path()));
        let sw = SwitchBackend::new(one.clone(), Mode::Live);
        let seen = Arc::new(Mutex::new(Vec::new()));
        let s2 = seen.clone();
        sw.set_event_sink(Box::new(move |e| s2.lock().unwrap().push(e)));
        let _ = one.sync_now();
        assert!(
            !seen.lock().unwrap().is_empty(),
            "the current backend speaks"
        );

        let (tx, rx) = std::sync::mpsc::channel();
        sw.switch(two.clone(), Mode::Live, move || tx.send(()).unwrap());
        rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
        seen.lock().unwrap().clear();
        // `one` is still alive here (as it is while its last pass finishes).
        let _ = one.sync_now();
        assert_eq!(*seen.lock().unwrap(), vec![], "the old blyg is silent");
        let _ = two.sync_now();
        assert!(!seen.lock().unwrap().is_empty(), "the new one speaks");

        // A new sink (the window re-registering) goes to the current one
        // only: the old one stays silent.
        let fresh = Arc::new(Mutex::new(Vec::new()));
        let f2 = fresh.clone();
        sw.set_event_sink(Box::new(move |e| f2.lock().unwrap().push(e)));
        let _ = one.sync_now();
        assert!(fresh.lock().unwrap().is_empty());
    }

    #[test]
    fn save_if_base_reaches_the_inner_backend() {
        use blyg_core::content_hash;
        let sw = SwitchBackend::new(Arc::new(Disconnected), Mode::Disconnected);
        let id = LocalId("nope".into());
        let e = sw.save_if_base(&id, "x", "sha256:0").unwrap_err();
        assert!(e.to_string().contains("Connect your blyg"), "{e}");

        let fake = Arc::new(crate::fake::FakeBackend::with_timing(
            crate::fake::Timing::instant(),
        ));
        let (tx, rx) = std::sync::mpsc::channel();
        sw.switch(fake.clone(), Mode::Fake, move || tx.send(()).unwrap());
        rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
        let id = sw.create_draft(Kind::Fragment, "first").unwrap();
        let base = content_hash("first");
        sw.save_if_base(&id, "second", &base).unwrap();
        assert_eq!(sw.item(&id).unwrap().content_md, "second");
        let stale = sw.save_if_base(&id, "third", &base).unwrap_err();
        assert!(
            matches!(&stale, CoreError::Rejected { status: 409, details, .. }
                if details == &vec![content_hash("second")]),
            "{stale:?}"
        );
        assert_eq!(sw.item(&id).unwrap().content_md, "second");
    }

    /// Every additive `Backend` method with a default the inner backend
    /// overrides is forwarded, not answered by the default.
    #[test]
    fn additive_calls_reach_the_inner_backend() {
        let fake = Arc::new(
            crate::fake::FakeBackend::with_timing(crate::fake::Timing::instant())
                .without_media_cache(),
        );
        let sw = SwitchBackend::new(fake.clone(), Mode::Fake);
        let id = sw.create_draft(Kind::Fragment, "first").unwrap();
        sw.save_typing(&id, "typed").unwrap();
        assert_eq!(fake.item(&id).unwrap().content_md, "typed");
        let url = sw
            .save_scratch_media(b"\x89PNG\r\n\x1a\n", "image/png")
            .expect("the inner backend keeps local images");
        assert_eq!(sw.scratch_media_file(&url), fake.scratch_media_file(&url));
        assert!(sw.scratch_media_file(&url).is_some());
    }
}
