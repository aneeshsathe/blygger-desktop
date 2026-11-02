//! Sync: outbox flush, periodic pull, conflict detection, status tracking.
//!
//! `Engine` holds the store, the API client and the event sink. All network
//! work that touches items runs under `net` (a mutex), so the background
//! worker, `publish` and `sync_now` never interleave half-applied ops (e.g. a
//! pull that sees a server item whose create hasn't been recorded locally yet).
//! The store's own lock is never held across network I/O, so UI reads and
//! saves stay instant while a push is in flight.

mod worker;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{Arc, Mutex, MutexGuard, RwLock};
use std::time::{Duration, Instant};

use crate::api::wire::ChangeState;
use crate::api::{Api, Contract, is_transient};
use crate::backend::{CoreError, Result};
use crate::model::*;
use crate::store::{Op, OpKind, Store};
use crate::util::now_ms;

pub(crate) use worker::{Msg, spawn};

/// `meta` key an older Burrow set when the server couldn't take TK
/// provenance. Every studio Burrow now syncs with takes it with the text,
/// so it is only ever cleared (once a push with provenance lands).
pub const PROVENANCE_UNAVAILABLE: &str = "tk_provenance_unavailable";

/// `meta` key: the change epoch (`GET /api/changes`) the
/// stored revisions belong to.
pub const CHANGES_EPOCH: &str = "changes_epoch";

/// `meta` key prefix: per consumer (`items`, `subscriptions`, `reading`),
/// a JSON map of domain → the revision its last accepted fetch was loaded
/// under (captured before the fetch, docs/d1-polling-cache-design.md R3).
pub const CHANGES_SEEN: &str = "changes_seen:";

/// What each pulled collection depends on, by `/api/changes` domain. A
/// collection is fetched again only when one of these moved.
const ITEMS_DEPS: &[&str] = &["items", "settings"];
const SUBSCRIPTIONS_DEPS: &[&str] = &["subscriptions"];
const READING_DEPS: &[&str] = &["reading", "subscriptions", "signals", "hoppers"];
/// --- hoppers --- the hoppers, their memberships and the unheld posts.
const HOPPERS_DEPS: &[&str] = &["hoppers", "subscriptions"];

/// With `/api/changes`, everything is still read whole this often, as a
/// safety net (a trigger the server's dependency matrix misses, lineage
/// fills that wait on a later pull).
const FULL_PULL_EVERY: Duration = Duration::from_secs(10 * 60);

/// One item's autosave pushes are at least this far apart (`force` ignores
/// it): about 20 writes a minute at most from continuous typing, well under
/// studio 0.28's budgets (60 writes a minute across all tokens, 120 for the
/// owner).
pub const SAVE_GAP: Duration = Duration::from_secs(3);

/// `meta` key: "1" while the server advertises read-state sync (`GET
/// /api/reading` said `readState: true`), "0" or absent otherwise.
/// Persisted so a read marked offline after a restart still queues.
pub const READ_SYNC: &str = "read_sync";

/// `meta` key set once this database's read state has been batch-uploaded
/// (the first time the server advertised read-state sync).
pub const READ_SYNC_UPLOADED: &str = "read_sync_uploaded";

/// `meta` key: "1" while the server also advertises `readStateClear`
/// (mark unread syncs, and reads carry `readAt`).
pub const READ_CLEAR: &str = "read_clear";

/// Timings. Defaults follow the spec; tests shrink them.
#[derive(Debug, Clone)]
pub struct SyncOptions {
    /// Push this long after the last save of an item.
    pub debounce: Duration,
    /// Pull `GET /api/items` + reading + subscriptions this often.
    pub pull_interval: Duration,
    pub backoff_initial: Duration,
    pub backoff_max: Duration,
    /// Run the background worker thread. Tests can turn it off and drive sync
    /// with `sync_now` for determinism.
    pub start_worker: bool,
    /// Reading list pages to read per pull (100 rows each). A list longer
    /// than this is merged without pruning, and read again next pull.
    pub reading_pages: u32,
    /// Public item documents fetched per pull to fill in lineage (0 = none).
    pub lineage_fetches: usize,
}

impl Default for SyncOptions {
    fn default() -> Self {
        SyncOptions {
            debounce: Duration::from_millis(800),
            pull_interval: Duration::from_secs(60),
            backoff_initial: Duration::from_secs(2),
            backoff_max: Duration::from_secs(120),
            start_worker: true,
            reading_pages: 20,
            lineage_fetches: 20,
        }
    }
}

pub(crate) type Sink = Arc<dyn Fn(CoreEvent) + Send + Sync>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Health {
    Ok,
    Offline,
    Error,
}

struct NetState {
    health: Health,
    backoff: Duration,
    retry_at: Option<Instant>,
    last_emitted: Option<SyncStatus>,
    /// Server lacks `GET /api/reading/imported`.
    reading_unavailable: bool,
    /// An outbox op is on the wire (`SyncStatus::Syncing`).
    pushing: bool,
    /// The server proved to speak contract 5 this session.
    compatible: bool,
    /// The server proved older than Burrow's minimum, studio 0.49.12 (its
    /// contract isn't 5.x); cleared once it isn't.
    outdated: bool,
    /// A reading pull found `GET /api/reading/imported` this session.
    reading_served: bool,
    /// The server answers `GET /api/changes` (`None` before a pull).
    changes: Option<bool>,
    /// When everything was last read whole.
    full_at: Option<Instant>,
    /// A 429 is being waited out (its notice was sent).
    rate_limited: bool,
    /// A 403 is holding the outbox (its notice was sent).
    scope_blocked: bool,
    /// When each item's last autosave push landed (`SAVE_GAP`).
    pushed: HashMap<LocalId, Instant>,
    /// The earliest time a save held back by `SAVE_GAP` may go.
    paced_until: Option<Instant>,
    /// --- read/unread --- The server refused a read-state write with 403:
    /// a credential without `reading:state` (a sign-in made before the
    /// scope existed). Read ops stay queued but aren't sent for the rest of the
    /// session (a new sign-in is a new session), and the rest of the outbox
    /// goes on without them.
    read_denied: bool,
    /// --- hoppers --- The server refused a hopper membership write with
    /// 403 (a credential without `owner:manage`): those ops stay queued,
    /// unsent, for the rest of the session; nothing else waits for them.
    hopper_denied: bool,
    /// The read-state flags (`GET /api/reading`), once read this session.
    read_flags: Option<crate::api::wire::ReadingFlags>,
    /// The blyg refused the stored credential (401): nothing is sent or
    /// pulled on its own until a new sign-in, or a manual sync.
    signed_out: bool,
}

/// What to say, once a session, when the blyg refuses a read-state write.
/// The studio names the scope (`reading:state`) in its challenge, and
/// `e` carries `oauth::missing_scope_message` (sign in again / a new token).
fn read_denied_message(e: &CoreError) -> String {
    let why = match e {
        CoreError::Rejected { message, .. } if !message.is_empty() => message.as_str(),
        _ => "The blyg refused to save read state for this sign-in.",
    };
    format!(
        "Read state isn't syncing. {why} Until then your read marks are kept on this \
         Mac and wait to be sent; everything else syncs as usual."
    )
}

/// --- hoppers --- What to say, once a session, when the blyg refuses a
/// hopper membership write (the 403's own message names the scope).
fn hopper_denied_message(e: &CoreError) -> String {
    let why = match e {
        CoreError::Rejected { message, .. } if !message.is_empty() => message.as_str(),
        _ => "The blyg refused to change a hopper for this sign-in.",
    };
    format!(
        "Hopper changes aren't syncing. {why} They're kept on this Mac and wait to be \
         sent; everything else syncs as usual."
    )
}

/// --- hoppers --- A 409 from a hopper write (the studio's `conflict`, or
/// a `revision-conflict`).
pub(crate) fn is_hopper_conflict(e: &CoreError) -> bool {
    matches!(
        e,
        CoreError::RevisionConflict | CoreError::Rejected { status: 409, .. }
    )
}

/// The time allowed per pull for fetching item documents (lineage).
const LINEAGE_BUDGET: Duration = Duration::from_secs(8);

/// A list read for a pull: `None` when it changed under every walk
/// (`CursorExpired`), so this pull leaves it as held and the next reads it.
fn settled<T>(r: Result<T>) -> Result<Option<T>> {
    match r {
        Ok(v) => Ok(Some(v)),
        Err(CoreError::CursorExpired) => Ok(None),
        Err(e) => Err(e),
    }
}

pub(crate) struct Engine {
    pub store: Store,
    pub api: Api,
    /// Other blygs' public files (lineage fills), with short timeouts.
    public: crate::api::public::PublicClient,
    pub opts: SyncOptions,
    net: Mutex<()>,
    sink: RwLock<Option<Sink>>,
    state: Mutex<NetState>,
    /// `<data dir>/scratch-media` (`crate::scratch_media`); `None` = no
    /// local images here (they can't be uploaded, so the op fails).
    pub scratch_dir: Option<std::path::PathBuf>,
}

impl Engine {
    pub fn new(store: Store, api: Api, opts: SyncOptions) -> Engine {
        Engine {
            store,
            api,
            public: crate::api::public::PublicClient::with_timeouts(
                Duration::from_secs(4),
                Duration::from_secs(6),
            ),
            net: Mutex::new(()),
            sink: RwLock::new(None),
            state: Mutex::new(NetState {
                health: Health::Ok,
                backoff: opts.backoff_initial,
                retry_at: None,
                last_emitted: None,
                reading_unavailable: false,
                pushing: false,
                compatible: false,
                outdated: false,
                reading_served: false,
                changes: None,
                full_at: None,
                rate_limited: false,
                scope_blocked: false,
                pushed: HashMap::new(),
                paced_until: None,
                read_denied: false,
                hopper_denied: false,
                read_flags: None,
                signed_out: false,
            }),
            opts,
            scratch_dir: None,
        }
    }

    /// The public origin (where `media/…` lives): learned from the server's
    /// permalinks, else the API base.
    pub fn public_origin(&self) -> String {
        self.store
            .public_origin()
            .unwrap_or_else(|| self.api.base_url().to_string())
            .trim_end_matches('/')
            .to_string()
    }

    /// Upload every local image `content` refers to (unattached, like a
    /// paste into a draft), returning `(name, absolute url)` for each one
    /// that made it, plus the error that stopped the rest (if any).
    pub fn upload_scratch_media(
        &self,
        content: &str,
    ) -> (Vec<(String, String)>, Option<CoreError>) {
        let mut done = Vec::new();
        for name in crate::scratch_media::refs(content) {
            let url = format!("{}{name}", crate::scratch_media::SCHEME);
            let Some(path) = self
                .scratch_dir
                .as_deref()
                .and_then(|d| crate::scratch_media::file(d, &url))
            else {
                return (
                    done,
                    Some(CoreError::Other(format!(
                        "an image in this note ({name}) is missing from this {}",
                        if cfg!(windows) { "PC" } else { "Mac" }
                    ))),
                );
            };
            let bytes = match std::fs::read(&path) {
                Ok(b) => b,
                Err(e) => return (done, Some(CoreError::Storage(e.to_string()))),
            };
            let mime = crate::scratch_media::mime_for(&name);
            match self.track(self.api.upload_media(&bytes, mime, None, None)) {
                Ok(m) => {
                    let abs = crate::scratch_media::absolute(&m.url, &self.public_origin());
                    done.push((name, abs));
                }
                Err(e) => return (done, Some(e)),
            }
        }
        (done, None)
    }

    pub fn debounce_ms(&self) -> i64 {
        self.opts.debounce.as_millis() as i64
    }

    pub fn net_lock(&self) -> MutexGuard<'_, ()> {
        self.net.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn state(&self) -> MutexGuard<'_, NetState> {
        self.state.lock().unwrap_or_else(|p| p.into_inner())
    }

    // ------------------------------------------------------------ events

    pub fn set_sink(&self, sink: Sink) {
        *self.sink.write().unwrap_or_else(|p| p.into_inner()) = Some(sink);
    }

    pub fn emit(&self, ev: CoreEvent) {
        let sink = self.sink.read().unwrap_or_else(|p| p.into_inner()).clone();
        if let Some(s) = sink {
            s(ev);
        }
    }

    pub fn status(&self) -> SyncStatus {
        let (health, pushing) = {
            let st = self.state();
            (st.health, st.pushing)
        };
        match health {
            Health::Offline => SyncStatus::Offline {
                pending: self.store.pending_count(),
            },
            Health::Error => SyncStatus::Error,
            Health::Ok if pushing => SyncStatus::Syncing,
            Health::Ok if self.earliest_due().is_some() => SyncStatus::Saving,
            Health::Ok => SyncStatus::Synced,
        }
    }

    /// Emit `SyncStatus` if it differs from the last one emitted.
    pub fn emit_status(&self) {
        let s = self.status();
        let changed = {
            let mut st = self.state();
            let c = st.last_emitted != Some(s);
            st.last_emitted = Some(s);
            c
        };
        if changed {
            self.emit(CoreEvent::SyncStatus(s));
        }
    }

    // ------------------------------------------------------------ health

    pub fn note_ok(&self) {
        let mut st = self.state();
        st.health = Health::Ok;
        st.backoff = self.opts.backoff_initial;
        st.retry_at = None;
        st.rate_limited = false;
        st.scope_blocked = false;
        st.signed_out = false;
    }

    /// Record a failed network call. Transient failures and auth failures
    /// schedule an exponential backoff for the worker. A 429 waits exactly
    /// the server's `Retry-After` instead, and says so once.
    pub fn note_err(&self, e: &CoreError) {
        if let CoreError::RateLimited { retry_after } = e {
            let first = {
                let mut st = self.state();
                st.retry_at = Some(Instant::now() + Duration::from_secs(*retry_after));
                !std::mem::replace(&mut st.rate_limited, true)
            };
            if first {
                self.emit(CoreEvent::Error(format!(
                    "The blyg asked Burrow to slow down; retrying in {retry_after}s"
                )));
            }
            return;
        }
        if let CoreError::Rejected { status: 403, .. } = e {
            let first = !std::mem::replace(&mut self.state().scope_blocked, true);
            if first {
                // The message names the scope the credential lacks.
                self.emit(CoreEvent::Error(format!(
                    "Changes are waiting: {e}. They'll be sent once you sign in with access to it."
                )));
            }
        }
        let mut st = self.state();
        let health = match e {
            CoreError::Offline => Health::Offline,
            CoreError::Unauthorized | CoreError::ServerOutdated => Health::Error,
            CoreError::Rejected { status: 403, .. } => Health::Error,
            e if is_transient(e) => Health::Error,
            _ => return,
        };
        st.health = health;
        st.retry_at = Some(Instant::now() + st.backoff);
        st.backoff = (st.backoff * 2).min(self.opts.backoff_max);
        // A 401 the credential can't recover from (`Api::exec` already
        // renewed a browser sign-in, or signed in again with the password):
        // the stored token is revoked, expired, or one the server no longer
        // takes (an owner token on a blyg that went stock). No retry loop:
        // the worker stops sending and pulling until a new sign-in (a new
        // session) or a manual sync, queued changes stay queued, and the UI
        // is told once, to ask for a sign-in.
        let newly_signed_out =
            matches!(e, CoreError::Unauthorized) && !std::mem::replace(&mut st.signed_out, true);
        drop(st);
        if newly_signed_out {
            self.emit(CoreEvent::SignedOut);
        }
    }

    /// The blyg refused the stored credential (401) this session.
    pub fn signed_out(&self) -> bool {
        self.state().signed_out
    }

    pub fn retry_at(&self) -> Option<Instant> {
        self.state().retry_at
    }

    /// The earliest time an autosave held back by `SAVE_GAP` may go, as of
    /// the last flush.
    pub fn paced_until(&self) -> Option<Instant> {
        self.state().paced_until
    }

    /// How often the worker pulls: a quarter of `pull_interval` (15 s by
    /// default, like Studio) when the server answers `/api/changes`, since an
    /// unchanged pull is then one small read; else the full interval.
    pub fn pull_every(&self) -> Duration {
        if self.state().changes == Some(true) {
            (self.opts.pull_interval / 4).max(Duration::from_secs(1))
        } else {
            self.opts.pull_interval
        }
    }

    /// Wrap a direct remote call so it feeds health/status.
    pub fn track<T>(&self, r: Result<T>) -> Result<T> {
        match &r {
            Ok(_) => self.note_ok(),
            Err(e) => self.note_err(e),
        }
        self.emit_status();
        r
    }

    // ------------------------------------------------------------ outbox

    /// Push due ops in order. `only` restricts to one item; `force` ignores
    /// the debounce. Stops at the first transient/auth failure.
    pub fn flush(&self, only: Option<&LocalId>, force: bool) -> Result<()> {
        let mut changed = false;
        let result = loop {
            let _net = self.net_lock();
            let op = match self.next_op(only, force) {
                Ok(Some(op)) => op,
                Ok(None) => break Ok(()),
                Err(e) => break Err(e),
            };
            if let Err(e) = self.check_server(true) {
                // Nothing is pushed; the op stays queued for a server that can take it.
                self.note_err(&e);
                break Err(e);
            }
            self.set_pushing(true);
            let ran = self.run_op(&op);
            self.set_pushing(false);
            match ran {
                // A read op held for the scope (`read_denied`): not a success,
                // so no other 403's notice is reset; the loop goes on to the
                // rest of the outbox, which skips the held reads.
                Ok(()) if self.read_held(&op) => {}
                Ok(()) => {
                    if op.kind == OpKind::Save {
                        self.state()
                            .pushed
                            .insert(op.local_id.clone(), Instant::now());
                    }
                    self.note_ok();
                    changed = true;
                }
                // A 403 is the credential, not the change: a token without
                // the scope this op needs (studio 0.28+). Like an ended
                // sign-in, the op waits for a credential that can send it.
                Err(e)
                    if is_transient(&e)
                        || matches!(
                            e,
                            CoreError::Unauthorized | CoreError::Rejected { status: 403, .. }
                        ) =>
                {
                    let _ = self.store.set_in_flight(op.seq, false);
                    self.note_err(&e);
                    break Err(e);
                }
                Err(e) => {
                    // The server refused this op outright; retrying won't help.
                    let _ = self.store.drop_op(op.seq);
                    changed = true;
                    self.emit(CoreEvent::Error(format!("couldn't sync a change: {e}")));
                }
            }
        };
        if changed {
            self.emit(CoreEvent::ItemsChanged);
        }
        self.emit_status();
        result
    }

    /// Whether the server proved older than Burrow's minimum, blygger-studio
    /// 0.49.12 (its private API contract isn't 5.x), as of the last check.
    pub fn server_outdated(&self) -> bool {
        self.state().outdated
    }

    /// When the next op is due, passing over the ones held for a scope this
    /// credential lacks (so the worker doesn't wake for them).
    pub fn earliest_due(&self) -> Option<i64> {
        let held = {
            let st = self.state();
            st.read_denied || st.hopper_denied
        };
        if !held {
            return self.store.earliest_due();
        }
        let conflicted = self.store.conflicted().unwrap_or_default();
        self.store
            .ops()
            .ok()?
            .into_iter()
            .filter(|o| !conflicted.contains(&o.local_id) && !self.read_held(o))
            .map(|o| o.not_before)
            .min()
    }

    /// Before a post is published: the check a push makes.
    pub fn check_publish(&self, content: &str) -> Result<()> {
        let _ = content;
        self.check_server(true)
    }

    /// Make sure the server speaks what Burrow sends: its private API
    /// contract (`GET {blyg}/api/capabilities`) is 5.x, blygger-studio
    /// 0.49.12 or later. Checked once per session (one small read), and
    /// again while it fails, before anything is pushed or pulled: a list
    /// in another shape would read as empty or ended, and what's held here
    /// would be pruned. An older studio is `ServerOutdated` (said once, as
    /// `CoreEvent::ServerOutdated`); a newer contract asks for a newer
    /// Burrow. Nothing is sent either way, and the local copy stays.
    fn check_server(&self, push: bool) -> Result<()> {
        let _ = push;
        if self.state().compatible {
            return Ok(());
        }
        match crate::api::contract(self.api.contract_version()?.as_deref()) {
            Contract::Supported => {
                let mut st = self.state();
                st.compatible = true;
                st.outdated = false;
                Ok(())
            }
            Contract::TooOld => {
                self.mark_outdated();
                Err(CoreError::ServerOutdated)
            }
            Contract::TooNew(c) => Err(CoreError::Other(format!(
                "This blyg runs studio API contract {c}, which this version of Burrow \
                 doesn't speak yet. Update Burrow (Burrow › Check for Updates…); nothing \
                 was synced, and your local copy is safe."
            ))),
        }
    }

    /// The server is too old: say so once (until it isn't).
    fn mark_outdated(&self) {
        let first = !std::mem::replace(&mut self.state().outdated, true);
        if first {
            self.emit(CoreEvent::ServerOutdated);
        }
    }

    /// Mark an op as on the wire; the status bar shows "syncing…" meanwhile.
    fn set_pushing(&self, on: bool) {
        self.state().pushing = on;
        if on {
            self.emit_status();
        }
    }

    fn next_op(&self, only: Option<&LocalId>, force: bool) -> Result<Option<Op>> {
        let ops = self.store.ops()?;
        let conflicted = self.store.conflicted()?;
        let now = now_ms();
        let mut blocked: HashSet<LocalId> = HashSet::new();
        let mut st = self.state();
        st.paced_until = None;
        for op in ops {
            if only.is_some_and(|o| *o != op.local_id) || blocked.contains(&op.local_id) {
                continue;
            }
            // Ops for one item run strictly in order: a later save waits for
            // the create that assigns its server id.
            // Read state the credential can't write waits, queued, for one
            // that can; it holds nothing else up.
            if self.read_held_locked(&st, &op) {
                continue;
            }
            if conflicted.contains(&op.local_id) || op.in_flight || (!force && op.not_before > now)
            {
                blocked.insert(op.local_id.clone());
                continue;
            }
            // Autosave pacing: the edits keep coalescing into this op meanwhile.
            if !force
                && op.kind == OpKind::Save
                && let Some(at) = st.pushed.get(&op.local_id).map(|t| *t + SAVE_GAP)
                && at > Instant::now()
            {
                st.paced_until = Some(st.paced_until.map_or(at, |p| p.min(at)));
                blocked.insert(op.local_id.clone());
                continue;
            }
            return Ok(Some(op));
        }
        Ok(None)
    }

    fn run_op(&self, op: &Op) -> Result<()> {
        if matches!(op.kind, OpKind::Read | OpKind::Unread) {
            return self.run_read(op);
        }
        if op.kind == OpKind::Hopper {
            return self.run_hopper(op); // --- hoppers ---
        }
        if op.kind == OpKind::DeleteRemote {
            // A draft an older Burrow retired: no row holds its revision, so
            // it's read first.
            self.store.set_in_flight(op.seq, true)?;
            let r = self
                .api
                .get_item(&op.payload)
                .and_then(|w| self.api.delete_item(&op.payload, w.revision));
            return match r {
                Ok(()) | Err(CoreError::NotFound) => self.store.drop_op(op.seq),
                // Changed between the read and the delete: next flush.
                Err(CoreError::RevisionConflict) => self.store.set_in_flight(op.seq, false),
                Err(e) => Err(e),
            };
        }
        let Some(mut row) = self.store.row(&op.local_id) else {
            return self.store.drop_op(op.seq);
        };
        if crate::scratch_media::has_refs(&row.item.content_md) {
            // A scratch note promoted offline: its local images go up first,
            // and the text is sent with their blyg URLs.
            let (done, err) = self.upload_scratch_media(&row.item.content_md);
            if !done.is_empty() && self.store.rewrite_media(&op.local_id, &done)? {
                self.emit(CoreEvent::ItemsChanged);
            }
            if let Some(e) = err {
                return Err(match e {
                    CoreError::Rejected {
                        status,
                        message,
                        details,
                    } => CoreError::Rejected {
                        status,
                        message: format!("an image couldn't be uploaded: {message}"),
                        details,
                    },
                    e => e,
                });
            }
            match self.store.row(&op.local_id) {
                Some(r) => row = r,
                None => return self.store.drop_op(op.seq),
            }
        }
        let item = &row.item;
        let content = item.content_md.clone();
        self.store.set_in_flight(op.seq, true)?;
        match op.kind {
            OpKind::Create => {
                if item.server_id.is_some() {
                    return self.store.drop_op(op.seq);
                }
                // The provenance goes with the text.
                let prov = self.create_provenance(&item.local_id, &content);
                let w = match self.api.create_item(
                    &content,
                    item.kind,
                    item.stub_of.as_ref(),
                    prov.as_deref(),
                ) {
                    Ok(w) => w,
                    // The provenance itself refused (e.g. a cited source that
                    // no longer resolves): create without it, and the push
                    // that follows sends it again (or reports it).
                    Err(CoreError::Rejected { status: 400, .. }) if prov.is_some() => {
                        let w = self.api.create_item(
                            &content,
                            item.kind,
                            item.stub_of.as_ref(),
                            None,
                        )?;
                        self.store.op_created(
                            op.seq,
                            &item.local_id,
                            &w.id,
                            &content,
                            item.kind,
                            w.revision,
                        )?;
                        return self.store.queue_prov_push(&item.local_id);
                    }
                    Err(e) => return Err(e),
                };
                self.store.op_created(
                    op.seq,
                    &item.local_id,
                    &w.id,
                    &content,
                    item.kind,
                    w.revision,
                )?;
                match prov {
                    Some(prov) => {
                        self.provenance_landed()?;
                        self.store.prov_pushed(&item.local_id, &content, &prov)
                    }
                    // Nothing tracked to disclose yet; a later push carries it.
                    None => self.store.queue_prov_push(&item.local_id),
                }
            }
            OpKind::Save => {
                let Some(sid) = &item.server_id else {
                    return self.store.op_lost_server(op.seq, &item.local_id);
                };
                let base = row.base_content.clone();
                let pushed = self.with_revision(&item.local_id, &sid.0, |rev| {
                    self.push_text(&item.local_id, &sid.0, rev, &content, base.as_deref())
                });
                match pushed {
                    Ok(Some(w)) => {
                        self.store
                            .op_saved(op.seq, &item.local_id, &content, w.revision)
                    }
                    // The blyg's text moved under this edit: the item is in
                    // conflict now, and its ops wait for the user's choice.
                    Ok(None) => self.store.set_in_flight(op.seq, false),
                    Err(CoreError::NotFound) => self.store.op_lost_server(op.seq, &item.local_id),
                    Err(e) => Err(e),
                }
            }
            OpKind::Recreate => {
                let Some(old) = &item.server_id else {
                    return self.store.drop_op(op.seq);
                };
                if row.server_kind == Some(item.kind) {
                    return self.store.drop_op(op.seq);
                }
                // A draft's kind can change until it is first published:
                // one `PATCH {contentMd, kind}` keeps its id. Provenance the
                // server holds is keyed to `base`; adopt it so the push that
                // follows remaps it onto the new text.
                let base = row.base_content.clone();
                let saved = self.with_revision(&item.local_id, &old.0, |rev| {
                    if self.store.provenance(&item.local_id).scopes.is_none()
                        && let Some(base) = &base
                        && let Some(server) = self.server_provenance(&old.0, rev)?
                        && server.iter().any(Option::is_some)
                    {
                        self.store.adopt_prov(&item.local_id, base, &server)?;
                    }
                    self.api.save_item_kind(&old.0, rev, &content, item.kind)
                });
                match saved {
                    Ok(Some(w)) => {
                        self.store.op_kind_saved(
                            op.seq,
                            &item.local_id,
                            &content,
                            item.kind,
                            w.revision,
                        )?;
                        self.store.queue_prov_push(&item.local_id)
                    }
                    Ok(None) => self.store.set_in_flight(op.seq, false),
                    Err(CoreError::NotFound) => self.store.op_lost_server(op.seq, &item.local_id),
                    Err(e) => Err(e),
                }
            }
            OpKind::DeleteRemote | OpKind::Read | OpKind::Unread | OpKind::Hopper => {
                unreachable!()
            }
        }
    }

    // ------------------------------------------------------------ revisions

    /// The revision to send with the next command on `id` (server id
    /// `sid`): the one stored with what was last synced, else read now
    /// (`GET /api/items/:id`, merged with conflict detection). `None` when
    /// that read found the blyg's text moved under a local edit: the item is
    /// in conflict now (`CoreEvent::Conflict` sent), and nothing is sent.
    fn revision_for(&self, id: &LocalId, sid: &str) -> Result<Option<u64>> {
        if let Some(r) = self.store.revision(id) {
            return Ok(Some(r));
        }
        self.reread(id, sid)
    }

    /// Read the item again and merge it with conflict detection; its fresh
    /// revision, or `None` when it entered conflict.
    fn reread(&self, id: &LocalId, sid: &str) -> Result<Option<u64>> {
        let w = self.api.get_item(sid)?;
        match self.store.check_server(id, &w)? {
            None => Ok(Some(w.revision)),
            Some((local_id, mine, theirs)) => {
                self.emit(CoreEvent::Conflict {
                    local_id,
                    mine,
                    theirs,
                });
                Ok(None)
            }
        }
    }

    /// Run an item command that carries the item's revision
    /// (`expectedRevision`). On 409 `revision-conflict` the item changed on
    /// the blyg since that revision was read: it is read again and merged
    /// with conflict detection. When the blyg's text moved under a local
    /// edit, the item enters conflict and nothing is sent (`Ok(None)`, the
    /// user chooses). When the text moved with no local edit, this Mac now
    /// shows the new text and the command isn't repeated over it
    /// (`RevisionConflict`: a publish or delete must not act on text the
    /// user hasn't seen). Only a revision that moved without the text (an
    /// attachment, a setting) is retried, once, with the fresh revision.
    /// Nothing is ever sent blind over a change it didn't see.
    pub(crate) fn with_revision<T>(
        &self,
        id: &LocalId,
        sid: &str,
        f: impl Fn(u64) -> Result<T>,
    ) -> Result<Option<T>> {
        let Some(rev) = self.revision_for(id, sid)? else {
            return Ok(None);
        };
        let base = self.store.row(id).and_then(|r| r.base_content);
        match f(rev) {
            Err(CoreError::RevisionConflict) => {}
            r => return r.map(Some),
        }
        let Some(rev) = self.reread(id, sid)? else {
            return Ok(None);
        };
        if self.store.row(id).and_then(|r| r.base_content) != base {
            return Err(CoreError::RevisionConflict);
        }
        f(rev).map(Some)
    }

    /// Push the working copy at revision `rev`. Studio keys TK provenance by
    /// scope position, so when the scopes changed since `base` (what the
    /// server holds), or tracked provenance changed, the text and the whole
    /// provenance array go together in one `PATCH {contentMd, provenance}`;
    /// otherwise it's a plain `PATCH {contentMd}`. Returns the item as
    /// stored.
    fn push_text(
        &self,
        id: &LocalId,
        sid: &str,
        rev: u64,
        content: &str,
        base: Option<&str>,
    ) -> Result<crate::api::wire::WireItem> {
        let Some(n) = crate::tk::scope_count(content) else {
            // Malformed TK (mid-typing): positions mean nothing yet. The
            // tracked provenance stays dirty for the next well-formed push.
            return self.api.save_item(sid, rev, content);
        };
        let t = self.store.provenance(id);
        let structure = base.is_none_or(|b| crate::tk::structure_changed(b, content));
        if !t.dirty && !structure {
            return self.api.save_item(sid, rev, content);
        }
        let scopes = match (&t.scopes, &t.keyed_to) {
            (Some(s), Some(k)) if k == content => s.clone(),
            (Some(s), Some(k)) => crate::tk::remap(k, s, content).unwrap_or_else(|| vec![None; n]),
            _ => {
                // Not tracked here: carry the server's own provenance (keyed
                // to `base`, which `rev` confirms it still holds) over.
                match self.server_provenance(sid, rev)? {
                    Some(server) if server.iter().any(Option::is_some) => base
                        .and_then(|b| crate::tk::remap(b, &server, content))
                        .unwrap_or_else(|| vec![None; n]),
                    // Nothing recorded anywhere: nothing can shift.
                    _ => return self.api.save_item(sid, rev, content),
                }
            }
        };
        let scopes: Vec<Option<ScopeProvenance>> = if crate::tk::validate(content, &scopes).is_ok()
        {
            self.with_versions(scopes)
        } else {
            vec![None; n]
        };
        self.push_provenance(id, sid, rev, content, &scopes)
    }

    /// The provenance the server holds for an item, as of revision `rev`
    /// (`GET /api/items/:id`). A newer revision there means the item moved
    /// since `rev`: `RevisionConflict`, so the caller reads it again with
    /// conflict detection rather than remap provenance keyed to text it
    /// hasn't seen.
    fn server_provenance(
        &self,
        sid: &str,
        rev: u64,
    ) -> Result<Option<Vec<Option<ScopeProvenance>>>> {
        let w = self.api.get_item(sid)?;
        if w.revision != rev {
            return Err(CoreError::RevisionConflict);
        }
        Ok(w.server_provenance())
    }

    /// Write `content` and `scopes` together at revision `rev`. A refusal
    /// of the provenance itself (e.g. a cited source that no longer
    /// resolves) retries without the citations, then saves the text with no
    /// disclosure and says so. A refused write changes nothing, so each
    /// retry goes at the same revision.
    fn push_provenance(
        &self,
        id: &LocalId,
        sid: &str,
        rev: u64,
        content: &str,
        scopes: &[Option<ScopeProvenance>],
    ) -> Result<crate::api::wire::WireItem> {
        let message = match self.api.save_item_provenance(sid, rev, content, scopes) {
            Ok(w) => {
                self.provenance_landed()?;
                self.store.prov_pushed(id, content, scopes)?;
                return Ok(w);
            }
            Err(CoreError::Rejected {
                status: 400,
                message,
                ..
            }) => message,
            Err(e) => return Err(e),
        };
        let bare: Vec<Option<ScopeProvenance>> = scopes
            .iter()
            .map(|p| {
                p.clone().map(|p| ScopeProvenance {
                    sources: vec![],
                    ..p
                })
            })
            .collect();
        match self.api.save_item_provenance(sid, rev, content, &bare) {
            Ok(w) => {
                self.store.prov_pushed(id, content, &bare)?;
                Ok(w)
            }
            Err(CoreError::Rejected { status: 400, .. }) => {
                self.emit(CoreEvent::Error(format!(
                    "The blyg refused the AI provenance for this post ({message}); the text was saved without it"
                )));
                let none = vec![None; scopes.len()];
                // Clear what the server held, so nothing stale stays keyed
                // to the new text; failing that, the text alone.
                let w = match self.api.save_item_provenance(sid, rev, content, &none) {
                    Err(CoreError::Rejected { status: 400, .. }) => {
                        self.api.save_item(sid, rev, content)?
                    }
                    r => r?,
                };
                // Don't retry (and re-warn) on every keystroke.
                self.store.prov_pushed(id, content, &none)?;
                Ok(w)
            }
            Err(e) => Err(e),
        }
    }

    /// A push with provenance landed: clear an older Burrow's "the server
    /// can't take it" mark.
    fn provenance_landed(&self) -> Result<()> {
        if self.store.meta(PROVENANCE_UNAVAILABLE).is_some() {
            self.store.delete_meta(PROVENANCE_UNAVAILABLE)?;
        }
        Ok(())
    }

    /// The provenance to send with a new item's `POST`: the tracked
    /// provenance, keyed to `content` and valid, when it discloses anything.
    fn create_provenance(
        &self,
        id: &LocalId,
        content: &str,
    ) -> Option<Vec<Option<ScopeProvenance>>> {
        let t = self.store.provenance(id);
        let scopes = match (t.scopes, t.keyed_to) {
            (Some(s), Some(k)) if k == content => s,
            (Some(s), Some(k)) => crate::tk::remap(&k, &s, content)?,
            _ => return None,
        };
        if !scopes.iter().any(Option::is_some) || crate::tk::validate(content, &scopes).is_err() {
            return None;
        }
        Some(self.with_versions(scopes))
    }

    /// Upstream records each cited source with its version, and the app
    /// may leave that to the server (`None`): fill it in from the item held
    /// here (own posts) or the reading list, and drop a source neither
    /// knows (the disclosure itself stays).
    fn with_versions(&self, scopes: Vec<Option<ScopeProvenance>>) -> Vec<Option<ScopeProvenance>> {
        if !scopes
            .iter()
            .flatten()
            .any(|p| p.sources.iter().any(|s| s.version.is_none()))
        {
            return scopes;
        }
        let reading = self.store.reading();
        let version_of = |id: &str| -> Option<u32> {
            let own = self
                .store
                .local_id_for_server(id)
                .and_then(|l| self.store.item(&l))
                .map(|i| i.version)
                .filter(|v| *v > 0);
            own.or_else(|| {
                reading
                    .iter()
                    .find(|r| r.remote_id == id)
                    .map(|r| r.version)
                    .filter(|v| *v > 0)
            })
        };
        scopes
            .into_iter()
            .map(|p| {
                p.map(|mut p| {
                    p.sources = p
                        .sources
                        .into_iter()
                        .filter_map(|mut s| {
                            if s.version.is_none() {
                                s.version = Some(version_of(&s.id)?);
                            }
                            Some(s)
                        })
                        .collect();
                    p
                })
            })
            .collect()
    }

    // ------------------------------------------------------------ pull

    pub fn pull(&self) -> Result<()> {
        let _net = self.net_lock();
        let r = self.pull_locked();
        match &r {
            Ok(()) => self.note_ok(),
            Err(e) => self.note_err(e),
        }
        self.emit_status();
        r
    }

    /// One pull. `GET /api/changes` comes first and each
    /// collection is fetched only when a domain it depends on moved since
    /// its last accepted fetch (everything, every `FULL_PULL_EVERY`). The
    /// revisions are captured before any data is loaded and recorded only
    /// after that collection's fetch succeeded, so a failed or partial
    /// fetch is simply done again (docs/d1-polling-cache-design.md R3).
    fn pull_locked(&self) -> Result<()> {
        self.check_server(false)?;
        let rev = self.revisions()?;
        let full = rev.is_none()
            || self
                .state()
                .full_at
                .is_none_or(|t| t.elapsed() >= FULL_PULL_EVERY);
        let stale = |consumer: &str, deps: &[&str]| full || !self.unchanged(&rev, consumer, deps);

        // Each list is one coherent acquisition (`Api::collect`): read to
        // its end before anything held here is pruned. One that kept
        // changing while it was read is left as held, and not accepted, so
        // the next pull reads it again.
        if stale("items", ITEMS_DEPS) {
            let settings = full || !self.unchanged(&rev, "items", &["settings"]);
            if let Some(wires) = settled(self.api.list_items_with(settings))? {
                let out = self.store.merge_all(&wires)?;
                for (local_id, mine, theirs) in out.conflicts {
                    self.emit(CoreEvent::Conflict {
                        local_id,
                        mine,
                        theirs,
                    });
                }
                if out.changed {
                    self.emit(CoreEvent::ItemsChanged);
                }
                self.accept(&rev, "items", ITEMS_DEPS)?;
            }
        }

        let mut reading_changed = false;
        if stale("subscriptions", SUBSCRIPTIONS_DEPS) {
            match self.api.list_subscriptions() {
                Ok(subs) => {
                    reading_changed |= self.store.replace_subscriptions(&subs)?;
                    self.accept(&rev, "subscriptions", SUBSCRIPTIONS_DEPS)?;
                }
                Err(CoreError::NotFound) => {
                    self.accept(&rev, "subscriptions", SUBSCRIPTIONS_DEPS)?
                }
                Err(CoreError::CursorExpired) => {}
                Err(e) => return Err(e),
            }
        }
        let mut reading_err = None;
        // Read state advances the `reading` revision, so it needs no
        // exception here.
        if stale("reading", READING_DEPS) {
            let fetched = self.fetch_reading()?;
            // A list that kept changing while it was read: as held, and
            // read again next pull.
            let mut whole = !matches!(fetched, Reading::Changing);
            if let Reading::Read(items, complete) = fetched {
                let kinds = self
                    .store
                    .subscriptions()
                    .into_iter()
                    .map(|s| (s.id, s.kind))
                    .collect();
                reading_changed |= self.store.merge_reading(&items, complete, &kinds)?;
                reading_changed |= self.fill_lineage(&items, complete, &kinds)?;
                if self.read_sync_on() {
                    match self.reconcile_reads(&items) {
                        Ok(cleared) => reading_changed |= cleared,
                        Err(e) => reading_err = Some(e),
                    }
                }
                // A walk that stopped at its page budget isn't done.
                whole = complete;
            }
            if whole {
                self.accept(&rev, "reading", READING_DEPS)?;
            }
        }
        // --- hoppers --- the hoppers and every membership (yours too), for
        // the sources pane; the rows' `hoppers` follow what's held here.
        if stale("hoppers", HOPPERS_DEPS) {
            match self.fetch_hoppers() {
                Ok(changed) => {
                    reading_changed |= changed;
                    self.accept(&rev, "hoppers", HOPPERS_DEPS)?;
                }
                Err(e) if is_transient(&e) || matches!(e, CoreError::Unauthorized) => {
                    return Err(e);
                }
                // A refused read leaves what's held; asked again next pull.
                Err(_) => {}
            }
        } else {
            reading_changed |= self.store.sync_reading_hoppers()?;
        }
        if reading_changed {
            self.emit(CoreEvent::ReadingChanged);
        }
        if full && rev.is_some() {
            self.state().full_at = Some(Instant::now());
        }
        reading_err.map_or(Ok(()), Err)
    }

    // ------------------------------------------------------------ revisions

    /// `GET /api/changes`, captured before anything is loaded. `None` when
    /// it's refused or unreadable, and everything is pulled whole. A new epoch (the database was restored or replaced)
    /// forgets every stored revision, so everything is read again.
    fn revisions(&self) -> Result<Option<ChangeState>> {
        let c = match self.api.changes() {
            Ok(c) => c,
            Err(e) if is_transient(&e) || matches!(e, CoreError::Unauthorized) => return Err(e),
            // Refused or unreadable: pull the old way.
            Err(_) => None,
        };
        self.state().changes = Some(c.is_some());
        let Some(c) = c else {
            return Ok(None);
        };
        if self.store.meta(CHANGES_EPOCH).as_deref() != Some(c.epoch.as_str()) {
            for consumer in ["items", "subscriptions", "reading", "hoppers"] {
                self.store
                    .delete_meta(&format!("{CHANGES_SEEN}{consumer}"))?;
            }
            self.store.set_meta(CHANGES_EPOCH, &c.epoch)?;
            self.state().full_at = None;
        }
        Ok(Some(c))
    }

    /// The revisions `consumer`'s last accepted fetch was loaded under.
    fn seen(&self, consumer: &str) -> BTreeMap<String, u64> {
        self.store
            .meta(&format!("{CHANGES_SEEN}{consumer}"))
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    /// Whether none of `deps` moved since `consumer` last fetched. A domain
    /// the server doesn't report counts as moved.
    fn unchanged(&self, rev: &Option<ChangeState>, consumer: &str, deps: &[&str]) -> bool {
        let Some(rev) = rev else {
            return false;
        };
        let seen = self.seen(consumer);
        deps.iter()
            .all(|d| rev.domains.get(*d).is_some_and(|n| seen.get(*d) == Some(n)))
    }

    /// `consumer`'s fetch succeeded: record the revisions captured before it.
    fn accept(&self, rev: &Option<ChangeState>, consumer: &str, deps: &[&str]) -> Result<()> {
        let Some(rev) = rev else {
            return Ok(());
        };
        let seen: BTreeMap<&str, u64> = deps
            .iter()
            .filter_map(|d| rev.domains.get(*d).map(|n| (*d, *n)))
            .collect();
        self.store.set_meta(
            &format!("{CHANGES_SEEN}{consumer}"),
            &serde_json::to_string(&seen).unwrap_or_default(),
        )
    }

    // ------------------------------------------------------------ hoppers

    /// Read every hopper with its memberships and store them
    /// (`Store::replace_hoppers`). A server without hoppers (404) holds
    /// none. True when anything shown changed.
    pub fn fetch_hoppers(&self) -> Result<bool> {
        let Some(details) = self.api.hopper_details()? else {
            return self.store.replace_hoppers(&[]);
        };
        let subs: HashMap<String, (String, String)> = self
            .store
            .subscriptions()
            .into_iter()
            .map(|s| (s.id, (s.title, s.origin)))
            .collect();
        let fetched: Vec<crate::store::FetchedHopper> = details
            .into_iter()
            .map(|d| crate::store::FetchedHopper {
                members: d
                    .members
                    .into_iter()
                    .map(|m| (m.subscription_id, m.remote_id, m.added_at))
                    .collect(),
                items: d
                    .items
                    .iter()
                    .filter_map(|v| {
                        let sub = v.get("subscriptionId")?.as_str()?;
                        let (title, origin) = subs.get(sub)?;
                        crate::api::wire::imported_item_row(v, title, origin)
                    })
                    .collect(),
                hopper: d.hopper,
            })
            .collect();
        self.store.replace_hoppers(&fetched)
    }

    /// Send a `hopper` op: `PUT` or `DELETE /api/hoppers/{id}/items/…`. A
    /// 404 (the hopper or the subscription is gone) drops it: the next read
    /// of the hoppers settles what's shown. A 403 holds every hopper op for
    /// the session (`hopper_denied`), and says so once.
    fn run_hopper(&self, op: &Op) -> Result<()> {
        let Ok(h) = serde_json::from_str::<crate::store::HopperOp>(&op.payload) else {
            return self.store.drop_op(op.seq);
        };
        self.store.set_in_flight(op.seq, true)?;
        let send = || {
            self.api
                .set_hopper_item(&h.hopper, &h.sub, &h.remote_id, h.on)
        };
        let sent = match send() {
            // The hopper changed on the blyg while this was applied (409):
            // read it again (gone is the 404 below) and send once more, as
            // a put or a take is the same change whatever it holds now.
            Err(e) if is_hopper_conflict(&e) => match self.api.hopper(&h.hopper) {
                Ok(_) => send(),
                Err(e) => Err(e),
            },
            r => r,
        };
        match sent {
            Ok(()) | Err(CoreError::NotFound) => self.store.drop_op(op.seq),
            // Twice: let it go, and read the hoppers whole next pull, so
            // what's shown here comes back to what the blyg has.
            Err(e) if is_hopper_conflict(&e) => {
                self.store.drop_op(op.seq)?;
                self.store.delete_meta(&format!("{CHANGES_SEEN}hoppers"))?;
                self.emit(CoreEvent::Error(
                    "A hopper kept changing on your blyg, so one change to it wasn't saved. \
                     Burrow will show the hopper as your blyg has it."
                        .into(),
                ));
                Ok(())
            }
            Err(e @ CoreError::Rejected { status: 403, .. }) => {
                self.store.set_in_flight(op.seq, false)?;
                let first = !std::mem::replace(&mut self.state().hopper_denied, true);
                if first {
                    self.emit(CoreEvent::Error(hopper_denied_message(&e)));
                }
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    // ------------------------------------------------------------ read state

    /// The server syncs read state (`readState`), as of the last reading
    /// pull. Still true once a write was
    /// refused with 403 (`read_denied`): marks keep queueing, and wait.
    pub fn read_sync_on(&self) -> bool {
        self.store.meta(READ_SYNC).as_deref() == Some("1")
    }

    /// `op` is a read-state op this session's credential may not send.
    fn read_held(&self, op: &Op) -> bool {
        self.read_held_locked(&self.state(), op)
    }

    fn read_held_locked(&self, st: &NetState, op: &Op) -> bool {
        (st.read_denied && matches!(op.kind, OpKind::Read | OpKind::Unread))
            || (st.hopper_denied && op.kind == OpKind::Hopper) // --- hoppers ---
    }

    /// A read-state write answered 403 (`e`, which names the missing scope,
    /// `reading:state`, when the server says): hold read ops for the rest
    /// of the session, queued, and say once how to get them sent. No retry
    /// until a new sign-in (a new session) or the next launch.
    fn read_denied(&self, e: &CoreError) -> Result<()> {
        let first = !std::mem::replace(&mut self.state().read_denied, true);
        if first {
            self.emit(CoreEvent::Error(read_denied_message(e)));
        }
        Ok(())
    }

    /// The server can also clear read state (`readStateClear`): unread
    /// syncs, and reads carry `readAt`.
    pub fn read_clear_on(&self) -> bool {
        self.read_sync_on() && self.store.meta(READ_CLEAR).as_deref() == Some("1")
    }

    fn set_read_sync(&self, on: bool) -> Result<()> {
        if self.read_sync_on() == on {
            return Ok(());
        }
        self.store.set_meta(READ_SYNC, if on { "1" } else { "0" })?;
        if !on {
            // Nothing to send them to any more.
            self.store.drop_read_ops()?;
            self.set_read_clear(false)?;
        }
        Ok(())
    }

    fn set_read_clear(&self, on: bool) -> Result<()> {
        let was = self.store.meta(READ_CLEAR).as_deref() == Some("1");
        if was == on {
            return Ok(());
        }
        self.store
            .set_meta(READ_CLEAR, if on { "1" } else { "0" })?;
        if !on {
            // Unread stays on this Mac (each row's floor holds it).
            self.store.drop_unread_ops()?;
        }
        Ok(())
    }

    /// After a pull from a server that syncs read state: the first time, send
    /// everything read here in batches; afterwards, settle each row the pull
    /// reported (`Store::reconcile_read_state`): queue reads the server
    /// lacks (made while it couldn't take them, or an RSS row that inherited
    /// read state from its renamed predecessor), queue unreads it lacks (on a
    /// server that can clear read state), and take the clears made on other
    /// devices. True when a row here changed.
    fn reconcile_reads(&self, pulled: &[ReadingItem]) -> Result<bool> {
        let clear = self.read_clear_on();
        if self.store.meta(READ_SYNC_UPLOADED).is_none() {
            if self.state().read_denied {
                return Ok(false);
            }
            let marks = self.store.read_marks(clear)?;
            for chunk in marks.chunks(crate::api::wire::READ_BATCH_MAX) {
                match self.api.put_reads(chunk) {
                    Ok(()) => {}
                    Err(CoreError::NotFound) => {
                        self.set_read_sync(false)?;
                        return Ok(false);
                    }
                    Err(e @ CoreError::Rejected { status: 403, .. }) => {
                        // Sent whole by a later session that may write.
                        self.read_denied(&e)?;
                        return Ok(false);
                    }
                    // Refused outright (a row it doesn't like): the rest
                    // still goes, and a retry wouldn't do better.
                    Err(CoreError::Rejected { .. }) => {}
                    Err(e) => return Err(e),
                }
            }
            self.store.set_meta(READ_SYNC_UPLOADED, "1")?;
            return Ok(false);
        }
        let server = pulled
            .iter()
            .map(|i| {
                (
                    (i.subscription_id.clone(), i.remote_id.clone()),
                    i.read_version,
                )
            })
            .collect();
        let plan = self.store.reconcile_read_state(&server, clear)?;
        Ok(plan.cleared > 0)
    }

    /// Run a `read` or `unread` op: with others of its kind that wait, as
    /// one batch (`POST /api/reading/read` or `/unread`, at most 500), else
    /// one `PUT` or `DELETE`. Unread ops reach the server only when it
    /// advertises `readStateClear`; otherwise they're dropped (unread stays
    /// on this Mac). `readAt` goes only to such a server too.
    fn run_read(&self, op: &Op) -> Result<()> {
        use crate::api::wire::{READ_BATCH_MAX, ReadMark, UnreadMark};
        let unread = op.kind == OpKind::Unread;
        if !self.read_sync_on() || (unread && !self.read_clear_on()) {
            return self.store.drop_op(op.seq);
        }
        let clear = self.read_clear_on();
        // This op first, then every other of its kind that's next in line
        // for its own row.
        let mut batch = vec![op.clone()];
        let mut seen: HashSet<LocalId> = HashSet::from([op.local_id.clone()]);
        for o in self.store.ops()? {
            if batch.len() >= READ_BATCH_MAX {
                break;
            }
            if !seen.insert(o.local_id.clone()) {
                continue;
            }
            if o.kind == op.kind && !o.in_flight && o.local_id.0.starts_with("read\u{1f}") {
                batch.push(o);
            }
        }
        for o in &batch {
            self.store.set_in_flight(o.seq, true)?;
        }
        let done = |this: &Self| -> Result<()> {
            for o in &batch {
                this.store.drop_op(o.seq)?;
            }
            Ok(())
        };
        let denied = |this: &Self, e: &CoreError| -> Result<()> {
            // Held, not dropped: the batch stays queued for a credential
            // that may write it (`read_denied`).
            for o in &batch {
                this.store.set_in_flight(o.seq, false)?;
            }
            this.read_denied(e)
        };
        let gone = |this: &Self| -> Result<()> {
            // The endpoint is gone (a downgraded server).
            done(this)?;
            if unread {
                this.set_read_clear(false)
            } else {
                this.set_read_sync(false)
            }
        };
        if unread {
            let marks: Vec<UnreadMark> = batch
                .iter()
                .filter_map(|o| serde_json::from_str(&o.payload).ok())
                .collect();
            let r = match marks.as_slice() {
                [] => Ok(()),
                [m] => self.api.delete_read(&m.sub, &m.remote_id),
                ms => self.api.post_unreads(ms),
            };
            return match r {
                Ok(()) => {
                    self.store.unread_acked(&marks)?;
                    done(self)
                }
                Err(CoreError::NotFound) => gone(self),
                Err(e @ CoreError::Rejected { status: 403, .. }) => denied(self, &e),
                Err(e) => {
                    for o in &batch[1..] {
                        self.store.set_in_flight(o.seq, false)?;
                    }
                    Err(e)
                }
            };
        }
        let marks: Vec<ReadMark> = batch
            .iter()
            .filter_map(|o| serde_json::from_str::<ReadMark>(&o.payload).ok())
            // Items start at version 1 and the server refuses a lower one
            // with a 400, which would sink the whole batch.
            .filter(|m| m.version >= 1)
            .map(|mut m| {
                if !clear {
                    m.read_at = None;
                }
                m
            })
            .collect();
        let r = match marks.as_slice() {
            [] => Ok(None),
            [m] => self
                .api
                .put_read(&m.sub, &m.remote_id, m.version, m.read_at.as_deref())
                .map(Some),
            ms => self.api.put_reads(ms).map(|()| None),
        };
        match r {
            Ok(ack) => {
                done(self)?;
                let mut changed = false;
                if let (Some(ack), [m]) = (ack, marks.as_slice()) {
                    changed = self
                        .store
                        .read_acked(m, ack.stored, ack.read_version, clear)?;
                }
                if changed {
                    self.emit(CoreEvent::ReadingChanged);
                }
                Ok(())
            }
            Err(CoreError::NotFound) => gone(self),
            Err(e @ CoreError::Rejected { status: 403, .. }) => denied(self, &e),
            // `readAt` refused (a strict body on a server that said it
            // could clear): stop sending it and retry without.
            Err(CoreError::Rejected {
                status: 400,
                details,
                ..
            }) if clear && details.iter().any(|d| d.contains("readAt")) => {
                for o in &batch {
                    self.store.set_in_flight(o.seq, false)?;
                }
                self.set_read_clear(false)?;
                self.run_read(op)
            }
            Err(e) => {
                // The flush retries `op` later or drops it; the rest of the
                // batch waits for its own turn.
                for o in &batch[1..] {
                    self.store.set_in_flight(o.seq, false)?;
                }
                Err(e)
            }
        }
    }

    /// One acquisition of `GET /api/reading/imported`: cursor pages from the
    /// first to `nextCursor: null`, at most `reading_pages` of them. The
    /// read-state flags come from `GET /api/reading` (once a session).
    ///
    /// When the list changes under the walk (409 `cursor-expired`), every
    /// page read so far is dropped and the walk starts again from the first
    /// page: a restarted first page is never appended to an old prefix. A
    /// walk that reached the end is `complete`, and only then may rows the
    /// answer left out be pruned. One stopped by the page budget is merged
    /// without pruning.
    fn fetch_reading(&self) -> Result<Reading> {
        let flags = self.read_flags()?;
        self.set_read_sync(flags.read_sync())?;
        self.set_read_clear(flags.read_clear())?;
        let pages = self.opts.reading_pages.max(1);
        'walk: for _ in 0..=crate::api::CURSOR_RESTARTS {
            let mut all: Vec<ReadingItem> = Vec::new();
            let mut cursor: Option<String> = None;
            for _ in 0..pages {
                let page = match self
                    .api
                    .reading(crate::api::wire::IMPORTED_READING_MAX, cursor.as_deref())
                {
                    Ok(Some(p)) => p,
                    Ok(None) => {
                        self.state().reading_unavailable = true;
                        self.set_read_sync(false)?;
                        return Ok(Reading::Unavailable);
                    }
                    Err(CoreError::CursorExpired) => continue 'walk,
                    Err(e) => return Err(e),
                };
                {
                    let mut st = self.state();
                    st.reading_unavailable = false;
                    st.reading_served = true;
                }
                // Only a server that advertises read state means it.
                let keep = self.read_sync_on();
                all.extend(page.items.into_iter().map(|mut it| {
                    if !keep {
                        it.read_version = None;
                    }
                    it
                }));
                match page.next {
                    // Opaque cursor: passed back verbatim.
                    Some(n) => cursor = Some(n),
                    None => return Ok(Reading::Read(all, true)),
                }
            }
            return Ok(Reading::Read(all, false));
        }
        Ok(Reading::Changing)
    }

    /// The read-state flags (`GET /api/reading`), read once a session. A
    /// server without the route keeps no read state.
    fn read_flags(&self) -> Result<crate::api::wire::ReadingFlags> {
        if let Some(f) = self.state().read_flags.clone() {
            return Ok(f);
        }
        let f = self.api.reading_flags()?.unwrap_or_default();
        self.state().read_flags = Some(f.clone());
        Ok(f)
    }

    /// Reading rows from an L0 or RSS-like source carry no lineage (who a
    /// post replies to or forks), so fetch the public item document of blyg
    /// posts that haven't been looked up yet: a few per pull, newest first, within
    /// a time budget, skipping a blyg for the rest of the pass once it fails.
    /// A fetched document is cached (the changelog cache), and the merge
    /// fills lineage from it from then on. True if any row changed.
    fn fill_lineage(
        &self,
        items: &[ReadingItem],
        complete: bool,
        kinds: &std::collections::HashMap<String, SubscriptionKind>,
    ) -> Result<bool> {
        let start = Instant::now();
        let mut failed: HashSet<String> = HashSet::new();
        let mut filled = false;
        let mut fetched = 0;
        for it in items {
            if fetched >= self.opts.lineage_fetches || start.elapsed() >= LINEAGE_BUDGET {
                break;
            }
            let blyg = kinds.get(&it.subscription_id) == Some(&SubscriptionKind::Blyg);
            if !blyg
                || it.state == "tombstone"
                || it.lineage_known
                || it.stub_of.is_some()
                || it.forked_from.is_some()
                || failed.contains(&it.origin)
                || self
                    .store
                    .cached_changelog(&it.origin, &it.remote_id)
                    .is_some()
            {
                continue;
            }
            fetched += 1;
            match self.public.item_doc(&it.origin, &it.remote_id) {
                Ok(doc) => {
                    let log = doc.versions_at(&it.origin);
                    self.store.put_changelog(&it.origin, &it.remote_id, &log)?;
                    filled |= Lineage::of_changelog(&log).is_some();
                }
                Err(_) => {
                    failed.insert(it.origin.clone());
                }
            }
        }
        if !filled {
            return Ok(false);
        }
        // Merge the whole pull again, as pulled: the merge fills lineage from
        // the cache it was just given. (A subset would read as "the server
        // dropped the rest" and prune them.)
        self.store.merge_reading(items, complete, kinds)
    }

    /// Whether the server serves the imported reading list, as of the last
    /// reading pull (`None` before one).
    pub fn server_extensions(&self) -> Option<bool> {
        let st = self.state();
        if st.reading_served {
            Some(true)
        } else if st.reading_unavailable {
            Some(false)
        } else {
            None
        }
    }

    pub fn reading_unavailable(&self) -> bool {
        self.state().reading_unavailable
    }

    /// Push everything (ignoring the debounce), then pull. Bypasses backoff.
    /// A blyg found too old is checked again first.
    pub fn sync_now(&self) -> Result<()> {
        self.state().retry_at = None;
        self.flush(None, true)?;
        self.pull()
    }
}

/// What one reading pull read.
enum Reading {
    /// The rows, and whether the walk reached the list's end.
    Read(Vec<ReadingItem>, bool),
    /// The server has no imported reading list.
    Unavailable,
    /// The list changed under every walk (`cursor-expired`): nothing taken.
    Changing,
}

#[cfg(test)]
mod read_held_tests; // --- read/unread ---
