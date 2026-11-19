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
///
/// `cadence` is the config file's `sync-cadence` (issue #71).
pub fn open_live(
    data_dir: &Path,
    url: &str,
    tokens: &Arc<dyn TokenStore>,
    cadence: blyg_core::SyncCadence,
) -> Result<Option<Arc<dyn Backend>>> {
    let opts = blyg_core::SyncOptions::for_cadence(cadence);
    Ok(open_live_with(data_dir, url, tokens, opts)?.map(|l| Arc::new(l) as Arc<dyn Backend>))
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
    let settings = crate::settings::get(cx);
    let (tokens, cadence) = (
        settings.tokens.clone(),
        settings.store.config().sync_cadence(),
    );
    Ok(open_live(&conn.data_dir, url, &tokens, cadence)?.map(|b| (b, Mode::Live)))
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

// `impl Backend for Disconnected`: build.rs writes it from the `Backend`
// trait (`build_forwarding.rs`). A required method answers from its return
// type (`Err` "Connect your blyg first", `None`, `false`, empty); a
// defaulted one keeps the trait's default. The methods below are the
// answers that differ, and the impl calls them instead.
include!(concat!(env!("OUT_DIR"), "/disconnected.rs"));

impl Disconnected {
    /// Nothing to send: not "offline", not "conflict".
    fn sync_status(&self) -> SyncStatus {
        SyncStatus::Synced
    }
    /// Nothing to pull: a sync succeeds at doing nothing.
    fn sync_now(&self) -> Result<()> {
        Ok(())
    }
    /// No versions, no mentions: empty lists, not an error.
    fn versions(&self, _: &LocalId) -> Result<Vec<Version>> {
        Ok(vec![])
    }
    fn mentions(&self) -> Result<Vec<Mention>> {
        Ok(vec![])
    }
    /// Refused outright (the default would look the item up, then save).
    fn save_if_base(&self, _: &LocalId, _: &str, _: &str) -> Result<()> {
        not_connected()
    }
    /// Never fetched (the default fetches anonymously over the network).
    fn fetch_own_media(&self, _: &str) -> Result<(Vec<u8>, Option<String>)> {
        not_connected()
    }
    /// Refreshing, clearing or marking nothing succeeds (there's nothing
    /// held to change).
    fn refresh_contacts(&self) -> Result<()> {
        Ok(())
    }
    fn refresh_subscriptions(&self) -> Result<()> {
        Ok(())
    }
    fn clear_browse(&self, _: Option<&str>) -> Result<()> {
        Ok(())
    }
    fn mark_read(&self, _: &str, _: &str) -> Result<()> {
        Ok(())
    }
    fn set_read(&self, rows: &[(String, String)], read: bool) -> Result<usize> {
        if read {
            Ok(rows.len())
        } else {
            Err(CoreError::Other(
                "marking unread isn't supported here".into(),
            ))
        }
    }
    /// Keeps no Nearby vectors: the app embeds again each run.
    fn put_nearby_vectors(
        &self,
        _: &str,
        _: &[String],
        _: &[blyg_core::nearby::NearbyVector],
    ) -> Result<()> {
        Ok(())
    }
    fn retain_nearby_vectors(&self, _: &str, _: &std::collections::HashSet<String>) -> Result<()> {
        Ok(())
    }
    /// Not known to be missing: the reading screens stay empty rather
    /// than saying "not available on this server".
    fn read_extensions_available(&self) -> bool {
        true
    }
    fn provenance_available(&self) -> bool {
        true
    }
    fn shelf_state(&self) -> blyg_core::shelf::SyncState {
        blyg_core::shelf::SyncState::Local
    }
}

// `impl Backend for SwitchBackend`: every method hands the call to the
// current backend. build.rs writes it from the `Backend` trait
// (`build_forwarding.rs`), so a new trait method is forwarded without a
// line here. A method the impl below defines under a trait method's name
// is called instead of the forward.
include!(concat!(env!("OUT_DIR"), "/switch_forwarding.rs"));

impl SwitchBackend {
    /// `Backend::set_event_sink`: keep the sink for the next backend, and
    /// gate it by generation so a switched-away backend's events are dropped.
    fn set_event_sink(&self, sink: Box<dyn Fn(CoreEvent) + Send + Sync>) {
        let sink: Sink = Arc::from(sink);
        *self.sink.lock().unwrap_or_else(|p| p.into_inner()) = Some(sink.clone());
        // Under the read lock, so no switch slips in between.
        let g = self.inner.read().unwrap_or_else(|p| p.into_inner());
        let generation = self.generation.load(Ordering::SeqCst);
        g.0.set_event_sink(self.gated(sink, generation));
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
