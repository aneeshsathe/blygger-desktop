//! Sync: outbox flush, periodic pull, conflict detection, status tracking.
//!
//! `Engine` holds the store, the API client and the event sink. All network
//! work that touches items runs under `net` (a mutex), so the background
//! worker, `publish` and `sync_now` never interleave half-applied ops (e.g. a
//! pull that sees a server item whose create hasn't been recorded locally yet).
//! The store's own lock is never held across network I/O, so UI reads and
//! saves stay instant while a push is in flight.

mod background; // --- background --- the studio's Background lens and choices
mod churn; // a list that changes under every walk sits pulls out
pub(crate) mod newer; // --- newer contract --- syncing on, removing nothing
mod shelf; // --- shelf sync --- the studio's synced shelf
mod syndication; // --- syndication --- copies elsewhere, written in place
pub(crate) mod upkeep; // --- upkeep --- stale quotes, kept in meta
pub mod walk; // head reads and whole walks of the reading list
mod worker;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{Arc, Mutex, MutexGuard, RwLock};
use std::time::{Duration, Instant};

use crate::api::wire::ChangeState;
use crate::api::{Api, Contract, Prune, is_transient};
use crate::backend::{CoreError, Result};
use crate::model::*;
use crate::store::{Op, OpKind, Store};
use crate::util::{now_ms, parse_ts_ms};

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
pub const ITEMS_DEPS: &[&str] = &["items", "settings"];
pub const SUBSCRIPTIONS_DEPS: &[&str] = &["subscriptions"];
/// The rows also carry each post's thumb (`signals`) and hoppers (`hoppers`),
/// but those have their own, much smaller reads (`SIGNALS_DEPS`,
/// `HOPPERS_DEPS`): a moved thumb or membership never walks the whole list
/// again (issue #71). Nor does `subscriptions`: the studio moves it on
/// every feed poll (when it polled, the feed's etag), which changes no
/// row. What a subscription change means for the rows (one gone, renamed)
/// moves `reading` too, and a real change to the subscription list starts
/// a whole walk of its own (`walk::subscriptions_differ`).
pub const READING_DEPS: &[&str] = &["reading"];
/// The owner's thumbs on the held reading rows (`GET /api/signals`).
pub const SIGNALS_DEPS: &[&str] = &["signals"];
/// --- hoppers --- the hoppers, their memberships and the unheld posts.
pub const HOPPERS_DEPS: &[&str] = &["hoppers", "subscriptions"];
/// --- shelf sync --- the shelf (`GET /api/shelf`, studio 0.61).
pub const SHELF_DEPS: &[&str] = &["shelf"];

/// With `/api/changes`, everything is still read whole this often, as a
/// safety net (a trigger the server's dependency matrix misses, lineage
/// fills that wait on a later pull). Each such pass that finds nothing the
/// counters hadn't already said doubles the wait, up to `FULL_PULL_MAX`;
/// one that finds something goes back to this (`SyncOptions::full_pull_every`).
pub const FULL_PULL_EVERY: Duration = Duration::from_secs(10 * 60);
/// The longest the safety net backs off to (`SyncOptions::full_pull_max`).
pub const FULL_PULL_MAX: Duration = Duration::from_secs(2 * 60 * 60);

/// The safety net's next interval after a pass that read everything whole
/// because it was due: back to `opts.full_pull_every` when it found what
/// the counters missed (`surprise`), else twice `last`, up to
/// `opts.full_pull_max` (issue #71: fewer D1 reads). Native sync paces its
/// safety net with it.
pub fn next_full_every(last: Duration, surprise: bool, opts: &SyncOptions) -> Duration {
    if surprise {
        opts.full_pull_every
    } else {
        (last * 2).min(opts.full_pull_max.max(opts.full_pull_every))
    }
}

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

/// How hard Burrow polls the blyg (`sync-cadence` in the config file).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SyncCadence {
    /// Change checks every 15 s, like Studio.
    #[default]
    Normal,
    /// For a blyg on a metered or free database plan (Cloudflare D1's free
    /// tier): change checks every minute, the reading list read again at
    /// most every 10 minutes, the safety net hourly at first.
    Low,
}

impl SyncCadence {
    /// `normal` or `low` (anything else: normal).
    pub fn parse(s: &str) -> SyncCadence {
        if s.trim().eq_ignore_ascii_case("low") {
            SyncCadence::Low
        } else {
            SyncCadence::Normal
        }
    }
}

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
    /// Reading list pages to read per pull (100 rows each). A whole walk of
    /// a longer list goes on from its cursor in the next pull (soon after:
    /// `walk::WALK_PAUSE`), and prunes only once it reaches the end.
    pub reading_pages: u32,
    /// Public item documents fetched per pull to fill in lineage (0 = none).
    pub lineage_fetches: usize,
    /// The safety net's first interval: everything read whole this long
    /// after the last such pass ([`FULL_PULL_EVERY`]).
    pub full_pull_every: Duration,
    /// The longest the safety net backs off to while it finds nothing
    /// ([`FULL_PULL_MAX`]).
    pub full_pull_max: Duration,
    /// Background pulls read the head of the reading list again at most
    /// this often when its counter (`reading`: imports, read marks) moved.
    /// A manual sync, a real subscription change and the safety net don't
    /// wait.
    pub reading_gap: Duration,
    /// The first wait after a 5xx (the blyg's Worker or database failed,
    /// e.g. a `D1_ERROR` once the free tier's daily reads are spent).
    /// Doubles on each one, up to `server_backoff_max`.
    pub server_backoff_initial: Duration,
    pub server_backoff_max: Duration,
    /// Ask reads conditionally (`If-None-Match`; studio 0.65+ answers 304
    /// with no body when nothing changed). On by default; off asks every
    /// read whole, as Burrow did before, to measure the difference.
    pub conditional_reads: bool,
}

impl SyncOptions {
    /// The defaults for `cadence`.
    pub fn for_cadence(cadence: SyncCadence) -> SyncOptions {
        let normal = SyncOptions::default();
        match cadence {
            SyncCadence::Normal => normal,
            SyncCadence::Low => SyncOptions {
                pull_interval: Duration::from_secs(4 * 60),
                full_pull_every: Duration::from_secs(60 * 60),
                full_pull_max: Duration::from_secs(6 * 60 * 60),
                reading_gap: Duration::from_secs(10 * 60),
                server_backoff_initial: Duration::from_secs(2 * 60),
                server_backoff_max: Duration::from_secs(60 * 60),
                ..normal
            },
        }
    }
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
            full_pull_every: FULL_PULL_EVERY,
            full_pull_max: FULL_PULL_MAX,
            reading_gap: Duration::from_secs(60),
            server_backoff_initial: Duration::from_secs(30),
            server_backoff_max: Duration::from_secs(30 * 60),
            conditional_reads: true,
        }
    }
}

/// After `GET /api/changes` answers 404 (studio 0.67 removes it, with
/// contract 6), it isn't asked again for this long: sync goes by
/// conditional reads instead (`Engine::validating`).
pub const CHANGES_RECHECK: Duration = Duration::from_secs(3 * 60 * 60);

pub(crate) type Sink = Arc<dyn Fn(CoreEvent) + Send + Sync>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Health {
    Ok,
    Offline,
    Error,
    /// The blyg answered 5xx: its Worker or database failed (`quota`: a
    /// `D1_ERROR` that names the daily limit).
    Unavailable {
        quota: bool,
    },
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
    /// The server's contract was checked this session: one Burrow speaks,
    /// or a newer one (`Store::lists_prune` says which).
    compatible: bool,
    /// --- newer contract --- The app was told the blyg runs a newer
    /// studio (`CoreEvent::ServerNewer`), once a session.
    newer_told: bool,
    /// The server proved older than Burrow's minimum, studio 0.49.12 (its
    /// contract isn't 5.x); cleared once it isn't.
    outdated: bool,
    /// A reading pull found `GET /api/reading/imported` this session.
    reading_served: bool,
    /// The server answers `GET /api/changes` (`None` before a pull).
    changes: Option<bool>,
    /// When `GET /api/changes` last answered 404: not asked again until
    /// `CHANGES_RECHECK` has passed.
    changes_gone_at: Option<Instant>,
    /// When everything was last read whole.
    full_at: Option<Instant>,
    /// The safety net's current interval (`full_pull_every`, doubling while
    /// it finds nothing, up to `full_pull_max`).
    full_every: Duration,
    /// When a read of the reading list (head or whole) was last accepted.
    reading_at: Option<Instant>,
    /// When a whole walk of the reading list last reached its end.
    whole_at: Option<Instant>,
    /// A whole walk part-way through, carried on by the next pull.
    walk: Option<walk::Walk>,
    /// A head read found nothing to explain a moved `reading` counter (a
    /// change deep in the list, a read mark from another device): walk the
    /// whole list once the last whole walk is `full_pull_every` old.
    deep_pending: bool,
    /// Read marks went from here since the last head read: they move
    /// `reading` too, so a head read that finds nothing is explained.
    reads_sent: bool,
    /// The wait before the next try after a 5xx.
    server_backoff: Duration,
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
    /// Each list's run of reads that changed under every walk
    /// (`churn::Churn`), by the name its revisions are kept under.
    churn: HashMap<&'static str, churn::Churn>,
}

/// What to say when the blyg refused a sign-in made for its old API
/// address (`Api::audience_moved`).
pub fn audience_moved_message(api_url: &str) -> String {
    format!(
        "Your blyg now serves its API only at {api_url}, and Burrow's sign-in \
         was made for its old address, which this studio no longer takes. \
         Sign in again to carry on syncing; your changes are kept."
    )
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
/// A 5xx from the blyg: `Some(quota)`, where `quota` says the message names
/// Cloudflare D1's daily limit (`D1_ERROR: … exceeded D1's free tier daily
/// row read limit`). A stock studio says only "internal server error";
/// a Worker that passes the database's message on says which.
pub(crate) fn server_failure(e: &CoreError) -> Option<bool> {
    let CoreError::Rejected {
        status, message, ..
    } = e
    else {
        return None;
    };
    if *status < 500 {
        return None;
    }
    let m = message.to_ascii_lowercase();
    Some(m.contains("d1_error") && (m.contains("limit") || m.contains("exceeded")))
}

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
        api.set_conditional(opts.conditional_reads);
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
                newer_told: false,
                outdated: false,
                reading_served: false,
                changes: None,
                changes_gone_at: None,
                full_at: None,
                full_every: opts.full_pull_every,
                reading_at: None,
                whole_at: None,
                walk: None,
                deep_pending: false,
                reads_sent: false,
                server_backoff: opts.server_backoff_initial,
                rate_limited: false,
                scope_blocked: false,
                pushed: HashMap::new(),
                paced_until: None,
                read_denied: false,
                hopper_denied: false,
                read_flags: None,
                signed_out: false,
                churn: HashMap::new(),
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

    /// Whether this pull may read `list`: not while it sits out pulls after
    /// changing under walk after walk (`churn`). Ask only when the list
    /// would otherwise be read. `asked`: a pull the user asked for.
    fn churn_due(&self, list: &'static str, asked: bool) -> bool {
        self.state().churn.entry(list).or_default().due(asked)
    }

    /// `list` was read to its end.
    fn churn_read(&self, list: &'static str) {
        if let Some(c) = self.state().churn.get_mut(list) {
            c.read();
        }
    }

    /// A read of `list` that failed, as a pull takes it. The list is left
    /// as held, and not accepted, so a later pull asks it again:
    /// - when it changed under every walk (`CursorExpired`; a studio
    ///   expires a reading cursor on every post a subscription's backfill
    ///   imports). After a few in a row it sits out pulls (`churn`), said
    ///   once a session;
    /// - in the catch-all arm, as before: a refusal (a 4xx other than 401)
    ///   and an answer Burrow couldn't read (`Other`, a parse error).
    ///
    /// What stops the whole pull comes back: offline, rate-limited, a 5xx
    /// (`is_transient`) and signed out.
    fn left_as_held(&self, list: &'static str, e: CoreError) -> Result<()> {
        match e {
            CoreError::CursorExpired => {
                let first = self.state().churn.entry(list).or_default().expired();
                if first {
                    self.emit(CoreEvent::Error(churn::message(list)));
                }
                Ok(())
            }
            e if is_transient(&e) || matches!(e, CoreError::Unauthorized) => Err(e),
            _ => Ok(()),
        }
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
            Health::Unavailable { quota } => SyncStatus::Unavailable {
                pending: self.store.pending_count(),
                quota,
            },
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
        st.server_backoff = self.opts.server_backoff_initial;
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
        // A 5xx: the blyg's Worker or its database failed. Retrying every
        // few seconds can't make it recover sooner (a D1 free-tier day
        // ends at 00:00 UTC), so wait much longer, and say so quietly in
        // the status bar rather than as an error (issue #71). A manual sync
        // still tries at once.
        if let Some(quota) = server_failure(e) {
            let mut st = self.state();
            st.health = Health::Unavailable { quota };
            st.retry_at = Some(Instant::now() + st.server_backoff);
            st.server_backoff = (st.server_backoff * 2).min(self.opts.server_backoff_max);
            return;
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
            // A sign-in made for the host-root `/api` of a mounted blyg:
            // studio 0.65 dropped that address (studio#151). Say why, so
            // the new sign-in (which finds the API under the mount) makes
            // sense.
            if self.api.audience_moved() {
                self.emit(CoreEvent::Error(audience_moved_message(
                    &self.api.api_url(),
                )));
            }
        }
    }

    /// The wait before the next try after a 5xx (tests).
    #[cfg(test)]
    fn server_backoff(&self) -> Duration {
        self.state().server_backoff
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
            st.read_denied || st.hopper_denied || self.shelf_held().is_some()
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
    /// contract (`GET {blyg}/api/capabilities`) is 5.x or later,
    /// blygger-studio 0.49.12 or later. Checked once per session (one small
    /// read), and again while it fails, before anything is pushed or
    /// pulled. An older studio is `ServerOutdated` (said once, as
    /// `CoreEvent::ServerOutdated`): nothing is sent, and the local copy
    /// stays. A newer contract syncs on, but its lists remove nothing held
    /// here (`sync/newer.rs`; said once, as `CoreEvent::ServerNewer`).
    fn check_server(&self, push: bool) -> Result<()> {
        let _ = push;
        if self.state().compatible {
            return Ok(());
        }
        let contract = crate::api::contract(self.api.contract_version()?.as_deref());
        let Some(prune) = contract.prune() else {
            self.mark_outdated();
            return Err(CoreError::ServerOutdated);
        };
        self.store.set_lists_prune(prune);
        self.note_syndication_served(); // --- syndication ---
        self.note_background_served(); // --- background ---
        self.note_shelf_served(); // --- shelf sync ---
        {
            let mut st = self.state();
            st.compatible = true;
            st.outdated = false;
        }
        if let Contract::Newer(version) = contract {
            self.tell_newer(version); // --- newer contract ---
        }
        Ok(())
    }

    /// The blyg's contract, checked (once a session) before a read outside
    /// a pull replaces something held here (`LiveBackend`'s refreshes).
    pub(crate) fn check_contract(&self) -> Result<()> {
        self.check_server(false)
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
        if op.kind == OpKind::Syndication {
            return self.run_syndication(op); // --- syndication ---
        }
        if op.kind == OpKind::Shelf {
            return self.run_shelf(op); // --- shelf sync ---
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
            OpKind::DeleteRemote
            | OpKind::Read
            | OpKind::Unread
            | OpKind::Hopper
            | OpKind::Syndication
            | OpKind::Shelf => {
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
    /// of the provenance itself (e.g. a cited post that no longer resolves)
    /// retries without the posts, keeping the web pages (they cite nothing
    /// that must resolve) and the sources Burrow doesn't read; then with the
    /// pages alone; then without any source; then saves the text with no
    /// disclosure and says so. A refused write changes nothing, so
    /// each retry goes at the same revision.
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
        let mut tried = vec![scopes.to_vec()];
        for strip in [
            ScopeProvenance::without_posts,
            ScopeProvenance::only_pages,
            ScopeProvenance::bare,
        ] {
            let retry: Vec<Option<ScopeProvenance>> =
                scopes.iter().map(|p| p.as_ref().map(strip)).collect();
            if tried.contains(&retry) {
                continue;
            }
            match self.api.save_item_provenance(sid, rev, content, &retry) {
                Ok(w) => {
                    self.store.prov_pushed(id, content, &retry)?;
                    return Ok(w);
                }
                Err(CoreError::Rejected { status: 400, .. }) => tried.push(retry),
                Err(e) => return Err(e),
            }
        }
        self.emit(CoreEvent::Error(format!(
            "The blyg refused the AI provenance for this post ({message}); the text was saved without it"
        )));
        let none = vec![None; scopes.len()];
        // Clear what the server held, so nothing stale stays keyed to the
        // new text; failing that, the text alone.
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

    /// A pull asked for (a manual sync, a test): nothing waits on
    /// `reading_gap`.
    pub fn pull(&self) -> Result<()> {
        self.pull_as(false)
    }

    /// The worker's own pull: the reading list waits out `reading_gap`
    /// when only its own counter moved.
    pub(crate) fn pull_background(&self) -> Result<()> {
        self.pull_as(true)
    }

    fn pull_as(&self, background: bool) -> Result<()> {
        let _net = self.net_lock();
        let r = self.pull_locked(background);
        // Re-index any text the pull (or typing) changed (`Store::flush_fts`).
        let _ = self.store.flush_fts();
        match &r {
            Ok(()) => self.note_ok(),
            Err(e) => self.note_err(e),
        }
        self.emit_status();
        r
    }

    /// One pull. `GET /api/changes` comes first and each
    /// collection is fetched only when a domain it depends on moved since
    /// its last accepted fetch (everything, every `full_pull_every`, backing
    /// off while that finds nothing new). The
    /// revisions are captured before any data is loaded and recorded only
    /// after that collection's fetch succeeded, so a failed or partial
    /// fetch is simply done again (docs/d1-polling-cache-design.md R3).
    fn pull_locked(&self, background: bool) -> Result<()> {
        let net_due = {
            let st = self.state();
            st.full_at.is_none_or(|t| t.elapsed() >= st.full_every)
        };
        let checked = self.state().compatible;
        // --- newer contract --- A check from an earlier pull holds removals
        // back until the contract is read again (`Engine::lay_list`).
        self.store.contract_stale();
        self.api.new_pull();
        self.check_server(false)?;
        // --- newer contract --- The safety net checks the contract again
        // (one conditional read): a studio upgraded mid-session is seen
        // before its lists remove anything.
        if net_due && checked {
            self.recheck_contract();
        }
        let rev = self.revisions()?;
        // No change counters (studio 0.67+), but the blyg answers reads
        // conditionally: every list is asked each pull, and one that hasn't
        // changed comes back 304, as held (`api::conditional`). The safety
        // net then paces only the whole walks of the reading list.
        let validating = self.validating(&rev);
        let full = (rev.is_none() && !validating) || net_due;
        let stale = |consumer: &str, deps: &[&str]| full || !self.unchanged(&rev, consumer, deps);
        // Read only because the safety net is due: what such a read finds
        // is what the counters missed (`surprise`), which resets the net's
        // backoff. Without counters, every list but the reading list's
        // whole walk is read each pull anyway.
        let forced = |consumer: &str, deps: &[&str]| {
            full && !validating && self.unchanged(&rev, consumer, deps)
        };
        let mut surprise = false;

        // Each list is one coherent acquisition (`Api::collect`): read to
        // its end before anything held here is pruned. One that kept
        // changing while it was read is left as held, and not accepted, so
        // the next pull reads it again.
        if stale("items", ITEMS_DEPS) {
            let settings = full || !self.unchanged(&rev, "items", &["settings"]);
            if let Some(wires) = settled(self.api.list_items_with(settings))? {
                let mut out = self.lay_list(
                    || self.store.merge_all(&wires),
                    |mut a, b| {
                        a.changed |= b.changed;
                        // One Conflict event per item, though both lays saw it.
                        for c in b.conflicts {
                            if !a.conflicts.iter().any(|x| x.0 == c.0) {
                                a.conflicts.push(c);
                            }
                        }
                        a
                    },
                )?;
                out.changed |= self.store.merge_wire_syndication(&wires)?; // --- syndication ---
                for (local_id, mine, theirs) in out.conflicts {
                    self.emit(CoreEvent::Conflict {
                        local_id,
                        mine,
                        theirs,
                    });
                }
                if out.changed {
                    surprise |= forced("items", ITEMS_DEPS);
                    self.emit(CoreEvent::ItemsChanged);
                }
                self.accept(&rev, "items", ITEMS_DEPS)?;
            }
        }

        let mut reading_changed = false;
        // The subscription list changed in a way its rows show (one gone,
        // renamed…), not just its poll bookkeeping: walk the reading list
        // whole, so rows of a removed subscription leave.
        let mut subs_changed = false;
        if stale("subscriptions", SUBSCRIPTIONS_DEPS) {
            match self.api.list_subscriptions() {
                Ok(subs) => {
                    let held = self.hold_subscriptions(subs)?;
                    subs_changed = held.differ;
                    let changed = held.changed;
                    surprise |= changed && forced("subscriptions", SUBSCRIPTIONS_DEPS);
                    reading_changed |= changed;
                    self.accept(&rev, "subscriptions", SUBSCRIPTIONS_DEPS)?;
                }
                Err(CoreError::NotFound) => {
                    self.accept(&rev, "subscriptions", SUBSCRIPTIONS_DEPS)?
                }
                Err(CoreError::CursorExpired) => {}
                Err(e) => return Err(e),
            }
        }
        // Read state advances the `reading` revision, so it needs no
        // exception here. A background pull where `reading` moved (imports,
        // read marks, often this Mac's own) reads the head at most once a
        // `reading_gap`; the safety net, a real subscription change and a
        // whole walk under way don't wait (`walk::start`).
        let gap_open = !background
            || self
                .state()
                .reading_at
                .is_none_or(|t| t.elapsed() >= self.opts.reading_gap);
        // Validating, the head's first page says whether it moved (304).
        let moved = validating || !self.unchanged(&rev, "reading", READING_DEPS);
        let has_mark = self.store.meta(walk::HEAD_MARK).is_some();
        let start = {
            let st = self.state();
            walk::start(walk::Due {
                in_progress: st.walk.is_some(),
                full,
                subs_changed,
                deep_due: st.deep_pending
                    && st
                        .whole_at
                        .is_none_or(|t| t.elapsed() >= self.opts.full_pull_every),
                moved,
                gap_open,
                has_mark,
            })
        };
        let forced_reading = if validating {
            full
        } else {
            forced("reading", READING_DEPS)
        };
        let read = self.read_reading(&rev, start, forced_reading)?;
        surprise |= read.surprise;
        reading_changed |= read.changed;
        let reading_err = read.read_err;
        // Thumbs and hoppers ride on a whole walk (the cursor's generation
        // covers them); without one they have reads of their own, below.
        // The owner's thumbs, when only they moved: the signals list, not
        // the whole reading list again (issue #71).
        if let Some(walked) = &read.walked_whole {
            self.accept(walked, "signals", SIGNALS_DEPS)?;
        } else if stale("signals", SIGNALS_DEPS) && self.churn_due("signals", !background) {
            match self.fetch_signals() {
                Ok(changed) => {
                    surprise |= changed && forced("signals", SIGNALS_DEPS);
                    reading_changed |= changed;
                    self.churn_read("signals");
                    self.accept(&rev, "signals", SIGNALS_DEPS)?;
                }
                Err(e) => self.left_as_held("signals", e)?,
            }
        }
        // --- hoppers --- the hoppers and every membership (yours too), for
        // the sources pane; the rows' `hoppers` follow what's held here.
        if stale("hoppers", HOPPERS_DEPS) && self.churn_due("hoppers", !background) {
            match self.fetch_hoppers() {
                Ok(changed) => {
                    surprise |= changed && forced("hoppers", HOPPERS_DEPS);
                    reading_changed |= changed;
                    self.churn_read("hoppers");
                    self.accept(&rev, "hoppers", HOPPERS_DEPS)?;
                }
                Err(e) => self.left_as_held("hoppers", e)?,
            }
        } else {
            reading_changed |= self.store.sync_reading_hoppers()?;
        }
        // --- background --- the studio's Background lens, read again when
        // what it's worked out from moved (and, like the reading list, not
        // before `reading_gap` in a background pull).
        if self.background_due(
            stale("background", background::BACKGROUND_DEPS),
            full || gap_open,
        ) && self.churn_due("background", !background)
        {
            match self.fetch_background() {
                Ok(changed) => {
                    surprise |= changed && forced("background", background::BACKGROUND_DEPS);
                    reading_changed |= changed;
                    self.churn_read("background");
                    self.accept(&rev, "background", background::BACKGROUND_DEPS)?;
                }
                Err(e) => self.left_as_held("background", e)?,
            }
        }
        // --- shelf sync --- read when its counter moved, sent when it
        // changed here (sync/shelf.rs). A blyg without it: nothing.
        if self.shelf_served()
            && ((stale("shelf", SHELF_DEPS) && self.churn_due("shelf", !background))
                || self.shelf_unsent())
        {
            match self.sync_shelf() {
                Ok(changed) => {
                    surprise |= changed && forced("shelf", SHELF_DEPS);
                    if changed {
                        self.emit(CoreEvent::ItemsChanged);
                        reading_changed = true;
                    }
                    self.churn_read("shelf");
                    self.accept(&rev, "shelf", SHELF_DEPS)?;
                }
                Err(e) => self.left_as_held("shelf", e)?,
            }
        }
        // --- upkeep --- your threads with stale quotes (`GET /freshness`,
        // the blyg's database only), asked again when your items or your
        // reading moved. A refused or failed read leaves what's held.
        if stale("freshness", upkeep::FRESHNESS_DEPS) && self.churn_due("freshness", !background) {
            match self.fetch_freshness() {
                Ok(Some((_, changed))) => {
                    self.churn_read("freshness");
                    if changed {
                        surprise |= forced("freshness", upkeep::FRESHNESS_DEPS);
                        self.emit(CoreEvent::ItemsChanged);
                    }
                    self.accept(&rev, "freshness", upkeep::FRESHNESS_DEPS)?;
                }
                Ok(None) => self.accept(&rev, "freshness", upkeep::FRESHNESS_DEPS)?,
                Err(e) => self.left_as_held("freshness", e)?,
            }
        }
        // --- end upkeep ---
        if reading_changed {
            self.emit(CoreEvent::ReadingChanged);
        }
        if full && (rev.is_some() || validating) {
            let mut st = self.state();
            st.full_at = Some(Instant::now());
            // Found what the counters missed: back to the first interval.
            // Found nothing: wait twice as long next time, up to the cap.
            st.full_every = next_full_every(st.full_every, surprise, &self.opts);
        } else if read.surprise {
            // A safety-net walk carried on past its first pull found what
            // the counters missed.
            self.state().full_every = self.opts.full_pull_every;
        }
        reading_err.map_or(Ok(()), Err)
    }

    // ------------------------------------------------------------ revisions

    /// `GET /api/changes`, captured before anything is loaded. `None` when
    /// it's refused or unreadable, and everything is pulled whole. A new epoch (the database was restored or replaced)
    /// forgets every stored revision, so everything is read again.
    ///
    /// Deprecated in studio 0.65 and gone in 0.67 (contract 6): a 404 isn't
    /// asked again for `CHANGES_RECHECK`, and sync goes by conditional
    /// reads meanwhile (`validating`).
    fn revisions(&self) -> Result<Option<ChangeState>> {
        let gone = self
            .state()
            .changes_gone_at
            .is_some_and(|t| t.elapsed() < CHANGES_RECHECK);
        if gone {
            return Ok(None);
        }
        let c = match self.api.changes() {
            Ok(c) => c,
            Err(e) if is_transient(&e) || matches!(e, CoreError::Unauthorized) => return Err(e),
            // Refused or unreadable: pull the old way.
            Err(_) => None,
        };
        {
            let mut st = self.state();
            st.changes = Some(c.is_some());
            st.changes_gone_at = c.is_none().then(Instant::now);
        }
        let Some(c) = c else {
            return Ok(None);
        };
        if self.store.meta(CHANGES_EPOCH).as_deref() != Some(c.epoch.as_str()) {
            // --- upkeep --- `freshness`: the stale quotes (sync/upkeep.rs).
            for consumer in [
                "items",
                "subscriptions",
                "reading",
                "signals",
                "hoppers",
                "freshness",
                "shelf",      // --- shelf sync ---
                "background", // --- background ---
            ] {
                self.store
                    .delete_meta(&format!("{CHANGES_SEEN}{consumer}"))?;
            }
            // The reading list's stamps may have gone back with it: no head
            // read until a whole walk has set a new mark.
            self.store.delete_meta(walk::HEAD_MARK)?;
            self.store.set_meta(CHANGES_EPOCH, &c.epoch)?;
            let mut st = self.state();
            st.full_at = None;
            st.full_every = self.opts.full_pull_every;
            st.walk = None;
        }
        Ok(Some(c))
    }

    /// Sync goes by conditional reads: the blyg has no change counters
    /// (`rev` is `None`: studio 0.67+), and its answers carry an `ETag`.
    /// With counters, they stay the cheapest idle check (one small `GET`),
    /// and conditional reads only make the re-reads they call for cheaper.
    fn validating(&self, rev: &Option<ChangeState>) -> bool {
        rev.is_none() && self.opts.conditional_reads && self.api.conditional_reads()
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

    /// Read the owner's whole signals list (`GET /api/signals`) and lay it
    /// over the held reading rows (`Store::replace_thumbs`). A server
    /// without the route (404) leaves the rows' own thumbs. True when a row
    /// changed.
    fn fetch_signals(&self) -> Result<bool> {
        match self.api.signals()? {
            Some(list) => self.lay_list(|| self.store.replace_thumbs(&list), |a, b| a || b),
            None => Ok(false),
        }
    }

    // ------------------------------------------------------------ hoppers

    /// Read every hopper with its memberships and store them
    /// (`Store::replace_hoppers`). A server without hoppers (404) holds
    /// none. True when anything shown changed.
    pub fn fetch_hoppers(&self) -> Result<bool> {
        let Some(details) = self.api.hopper_details()? else {
            return self.lay_list(|| self.store.replace_hoppers(&[]), |a, b| a || b);
        };
        let subs: HashMap<SubId, (String, String)> = self
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
        self.lay_list(|| self.store.replace_hoppers(&fetched), |a, b| a || b)
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
            // --- newer contract --- the shelf isn't written to such a blyg,
            // nor by a sign-in that may only read it.
            || (op.kind == OpKind::Shelf && self.shelf_held().is_some())
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
        let cleared = self.lay_list(
            || Ok(self.store.reconcile_read_state(&server, clear)?.cleared),
            |a, b| a + b,
        )?;
        Ok(cleared > 0)
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
        // It moves `reading` on the blyg (`walk::Due`).
        self.state().reads_sent = true;
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

    /// Read the reading list (`GET /api/reading/imported`) as `start`
    /// says (sync/walk.rs): a head read, a whole walk, or the rest of one.
    /// Each page is merged as it arrives (and the UI told), removing
    /// nothing; at most `reading_pages` pages a pull. The read-state flags
    /// come from `GET /api/reading` (once a session).
    ///
    /// - A head read stops once a page reaches back past what the last read
    ///   covered, and is accepted without pruning. One that runs out of
    ///   pages first carries on as a whole walk.
    /// - A whole walk goes on next pull from its cursor, and only one that
    ///   reached `nextCursor: null` prunes: the rows that one coherent walk
    ///   didn't read.
    /// - When the list changes under a read (409 `cursor-expired`), it
    ///   starts again from the first page, and nothing read before counts
    ///   towards the prune: a restarted first page is never appended to an
    ///   old prefix. After `CURSOR_RESTARTS` the list is left as held for
    ///   this pull.
    fn read_reading(
        &self,
        rev: &Option<ChangeState>,
        start: walk::Start,
        forced: bool,
    ) -> Result<ReadOut> {
        use walk::{PageFacts, Read, Start, Step};
        let mut out = ReadOut::default();
        if start == Start::Nothing {
            return Ok(out);
        }
        // A failure here leaves a walk under way where it is; a flags read
        // that kept changing leaves the list as held for this pull.
        let flags = match self.read_flags() {
            Err(CoreError::CursorExpired) => return Ok(out),
            r => r?,
        };
        self.set_read_sync(flags.read_sync())?;
        self.set_read_clear(flags.read_clear())?;
        // Read marks sent from here since the last head read explain a
        // moved counter that the head shows nothing for.
        let reads_sent = std::mem::take(&mut self.state().reads_sent);
        let mark = self
            .store
            .meta(walk::HEAD_MARK)
            .and_then(|m| m.parse::<i64>().ok());
        let held = (start == Start::Continue)
            .then(|| self.state().walk.take())
            .flatten();
        let Some(mut r) = Read::begin(walk::Begin {
            start,
            held,
            rev: rev.clone(),
            forced,
            mark,
            budget: self.opts.reading_pages,
            reads_sent,
            prune: self.store.lists_prune(),
        }) else {
            return Ok(out);
        };
        let kinds: HashMap<SubId, SubscriptionKind> = self
            .store
            .subscriptions()
            .into_iter()
            .map(|s| (s.id, s.kind))
            .collect();
        let mut lineage = LineageBudget {
            left: self.opts.lineage_fetches,
            start: Instant::now(),
            failed: HashSet::new(),
        };
        // Without change counters a head read is how a pull learns whether
        // the list moved: a first page whose rows are as they were (a 304,
        // or only its cursor moved: a feed poll, a thumb) says it didn't.
        // What's deeper (a read mark on an old post from another device)
        // waits for the safety net's whole walk, which backs off while it
        // finds nothing; a deep walk on every feed poll would cost the
        // owner a walk of the whole list each time.
        let mut probe = start == Start::Head && self.validating(rev);
        loop {
            let page = match self.api.reading(
                crate::api::wire::IMPORTED_READING_MAX,
                r.walk.cursor.as_deref(),
            ) {
                Ok(Some(p)) => p,
                Ok(None) => {
                    // No imported reading list on this blyg: nothing to read.
                    self.state().reading_unavailable = true;
                    self.set_read_sync(false)?;
                    self.accept(rev, "reading", READING_DEPS)?;
                    self.state().reading_at = Some(Instant::now());
                    return Ok(out);
                }
                Err(CoreError::CursorExpired) => match r.on_expired(rev.clone()) {
                    Step::Fetch => continue,
                    // As held; read again next pull.
                    _ => {
                        self.stop_read(r);
                        return Ok(out);
                    }
                },
                Err(e) => {
                    // A whole walk goes on from here next pull (its cursor
                    // may still hold); a head read starts over.
                    self.stop_read(r);
                    return Err(e);
                }
            };
            {
                let mut st = self.state();
                st.reading_unavailable = false;
                st.reading_served = true;
            }
            if std::mem::take(&mut probe) && page.unchanged {
                // The head is as it was: nothing to merge, and no reason
                // for a deep walk (that's for a move the head can't show).
                self.state().reading_at = Some(Instant::now());
                return Ok(out);
            }
            // Only a server that advertises read state means it.
            let keep = self.read_sync_on();
            let items: Vec<ReadingItem> = page
                .items
                .into_iter()
                .map(|mut it| {
                    if !keep {
                        it.read_version = None;
                    }
                    it
                })
                .collect();
            r.walk.keys.extend(
                items
                    .iter()
                    .map(|i| (i.subscription_id.clone(), i.remote_id.clone())),
            );
            let stamps = || items.iter().filter_map(|i| parse_ts_ms(&i.observed_at));
            // --- newer contract --- A page may re-key a row (an RSS guid
            // changed), which removes the old one: on a check from an
            // earlier pull, the contract is read again first.
            if !items.is_empty() && self.store.lists_prune_now() == crate::store::Removal::Recheck {
                self.recheck_contract();
            }
            let (mut changed, written) =
                self.store
                    .merge_reading_page(&items, &r.walk.keys, &kinds)?;
            changed |= self.store.set_reading_syndication(&page.syndication)?; // --- syndication ---
            changed |= self.fill_lineage(&items, &r.walk.keys, &kinds, &mut lineage)?;
            if self.read_sync_on() {
                match self.reconcile_reads(&items) {
                    Ok(cleared) => changed |= cleared,
                    Err(e) => out.read_err = Some(e),
                }
            }
            if changed {
                out.changed = true;
                out.surprise |= r.walk.forced;
                // The UI shows this page's posts now, not after the walk
                // (events coalesce in the app).
                self.emit(CoreEvent::ReadingChanged);
            }
            let step = r.on_page(PageFacts {
                next: page.next,
                changed,
                oldest_ms: stamps().min(),
                newest_ms: stamps().max(),
                written_ms: written
                    .iter()
                    .filter_map(|&i| parse_ts_ms(&items[i].observed_at))
                    .collect(),
            });
            match step {
                Step::Fetch => {}
                Step::Stop => {
                    self.stop_read(r);
                    return Ok(out);
                }
                Step::Done { whole, prune } => {
                    // --- newer contract --- A walk may span pulls: the
                    // contract is checked again before it prunes.
                    let prune = prune && self.recheck_contract() == Prune::Allowed;
                    if prune && self.store.prune_reading_except(&r.walk.keys)? {
                        out.changed = true;
                        out.surprise |= r.walk.forced;
                        self.emit(CoreEvent::ReadingChanged);
                    }
                    self.finish_read(&r, whole)?;
                    if whole {
                        out.walked_whole = Some(r.walk.rev);
                    }
                    return Ok(out);
                }
            }
        }
    }

    /// A read stops part-way for this pull: keep a whole walk for the next
    /// (`Read::kept`), and ask for a deep walk if it found a reason.
    fn stop_read(&self, r: walk::Read) {
        let deep = r.deep;
        let kept = r.kept();
        let mut st = self.state();
        st.deep_pending |= deep;
        if kept.is_some() {
            st.walk = kept;
        }
    }

    /// A read of the reading list is done: accept the revisions captured
    /// before its first page, and remember how far back it covered.
    fn finish_read(&self, r: &walk::Read, whole: bool) -> Result<()> {
        self.accept(&r.walk.rev, "reading", READING_DEPS)?;
        if let Some(mark) = r.next_mark(whole) {
            self.store.set_meta(walk::HEAD_MARK, &mark.to_string())?;
        }
        let mut st = self.state();
        st.reading_at = Some(Instant::now());
        if whole {
            st.whole_at = st.reading_at;
            st.deep_pending = false;
        }
        st.deep_pending |= r.deep;
        Ok(())
    }

    /// A whole walk of the reading list is part-way through: the worker
    /// pulls again soon (`walk::WALK_PAUSE`) to carry it on. One the list
    /// kept changing under (back at its first page) waits for the usual
    /// pull instead.
    pub fn reading_walk_pending(&self) -> bool {
        self.state()
            .walk
            .as_ref()
            .is_some_and(|w| w.cursor.is_some())
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
        listed: &HashSet<(SubId, RemoteId)>,
        kinds: &std::collections::HashMap<SubId, SubscriptionKind>,
        budget: &mut LineageBudget,
    ) -> Result<bool> {
        let mut filled = false;
        let LineageBudget {
            left,
            start,
            failed,
        } = budget;
        for it in items {
            if *left == 0 || start.elapsed() >= LINEAGE_BUDGET {
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
            *left -= 1;
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
        // Merge the page again, as pulled: the merge fills lineage from the
        // cache it was just given. (It removes nothing.)
        self.store.merge_reading_rows(items, listed, kinds)
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

/// What one pull's read of the reading list did.
#[derive(Default)]
struct ReadOut {
    /// A row here changed (merged, pruned, read state taken).
    changed: bool,
    /// A safety-net walk found what the counters missed.
    surprise: bool,
    /// A whole walk reached the list's end in this pull, under these
    /// revisions (captured before its first page).
    walked_whole: Option<Option<ChangeState>>,
    /// Settling read state failed (the pull reports it once it's done).
    read_err: Option<CoreError>,
}

/// The lineage fetches one pull may still make (`Engine::fill_lineage`),
/// shared by all the pages it reads.
struct LineageBudget {
    left: usize,
    start: Instant,
    failed: HashSet<String>,
}

#[cfg(test)]
mod read_held_tests; // --- read/unread ---

#[cfg(test)]
mod cadence_tests; // issue #71
#[cfg(test)]
mod churn_tests;
