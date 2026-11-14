//! `LiveBackend`: the real `Backend`, over the SQLite store, the owner API and
//! the sync worker.

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::Duration;

use crate::api::Api;
use crate::api::public::{PublicClient, not_pinned};
use crate::api::wire::WireItem;
use crate::backend::*;
use crate::model::*;
use crate::store::Store;
use crate::sync::{Engine, Msg, SyncOptions, is_hopper_conflict, spawn};

pub struct LiveBackend {
    engine: Arc<Engine>,
    /// Unauthenticated reads of other blygs' public surface. Holds no token.
    public: PublicClient,
    tx: Mutex<Sender<Msg>>,
    worker: Mutex<Option<JoinHandle<()>>>,
    /// --- browse --- The link prober's own short-timeout public client.
    probe: PublicClient,
    /// --- browse --- Session state of the browsing log.
    browse: crate::browse::reads::BrowseState,
    /// Saves from typing (`Backend::save_typing`), and the thread that
    /// writes them.
    typing: Arc<Typing>,
    writer: Mutex<Option<JoinHandle<()>>>,
}

/// Saves from typing, written to the store by a thread of their own so a
/// keystroke never waits on SQLite (a transaction, and behind it whatever
/// a pull is merging). The newest text per item wins: while one write
/// runs, the keystrokes after it collapse into the next. Until its write
/// has landed an item's text is here, and every read of the item through
/// `LiveBackend` sees it (`item` lays it over the stored row; everything
/// else writes it first, `LiveBackend::e`).
#[derive(Default)]
struct Typing {
    /// The newest unwritten text per item.
    pending: Mutex<HashMap<LocalId, Arc<str>>>,
    /// Signalled when `pending` gains a text, or on shutdown.
    wake: Condvar,
    /// Held while texts are written, so two writers (the thread, and a
    /// `settle` on another thread) never put one item's texts in the wrong
    /// order.
    write: Mutex<()>,
    stop: AtomicBool,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

impl Typing {
    /// Write every pending text. A text stays pending until it's written,
    /// so `item` never reads the store between the two. Returns whether
    /// anything was written.
    fn write(&self, engine: &Engine, tx: &Mutex<Sender<Msg>>) -> bool {
        let _order = lock(&self.write);
        let batch: Vec<(LocalId, Arc<str>)> = lock(&self.pending)
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        if batch.is_empty() {
            return false;
        }
        for (id, text) in &batch {
            if let Err(e) = engine.store.save(id, text, engine.debounce_ms()) {
                engine.emit(CoreEvent::Error(format!("Couldn't save: {e}")));
            }
        }
        {
            let mut pending = lock(&self.pending);
            for (id, text) in &batch {
                if pending.get(id).is_some_and(|t| Arc::ptr_eq(t, text)) {
                    pending.remove(id);
                }
            }
        }
        if let Ok(tx) = tx.lock() {
            let _ = tx.send(Msg::Wake);
        }
        engine.emit_status();
        true
    }
}

/// The typing writer: writes whatever is pending as soon as it's there;
/// after a pause (the push debounce) re-indexes the text it wrote for
/// search (`Store::flush_fts`).
fn spawn_writer(engine: Arc<Engine>, typing: Arc<Typing>, tx: Sender<Msg>) -> JoinHandle<()> {
    let tx = Mutex::new(tx);
    let pause = Duration::from_millis(engine.debounce_ms().max(1) as u64);
    std::thread::Builder::new()
        .name("blyg-typing".into())
        .spawn(move || {
            let mut unindexed = false;
            loop {
                {
                    let mut pending = lock(&typing.pending);
                    while pending.is_empty() && !typing.stop.load(Ordering::SeqCst) {
                        let (p, timeout) = typing
                            .wake
                            .wait_timeout(pending, pause)
                            .unwrap_or_else(|p| p.into_inner());
                        pending = p;
                        if timeout.timed_out() && pending.is_empty() && unindexed {
                            drop(pending);
                            let _ = engine.store.flush_fts();
                            unindexed = false;
                            pending = lock(&typing.pending);
                        }
                    }
                    if pending.is_empty() {
                        return; // stopped, and nothing left to write
                    }
                }
                unindexed |= typing.write(&engine, &tx);
            }
        })
        .expect("spawn typing writer")
}

pub const DB_FILE: &str = "blygger.db";

/// How long a fetched public changelog is trusted before refetching.
pub const CHANGELOG_TTL_MS: i64 = 2 * 60 * 1000;

/// --- contacts --- the address book as last fetched (JSON), and whether
/// the blyg served it ("1"/"0").
const CONTACTS_META: &str = "contacts.json";
const CONTACTS_SERVED_META: &str = "contacts.served";

/// --- loose ends --- your posts a verified mention names, as last fetched.
const MENTIONED_META: &str = "loose_ends.mentioned";

impl LiveBackend {
    /// Open (or create) `<data_dir>/blygger.db` and start syncing with the
    /// blyg at `base_url`.
    pub fn open(
        data_dir: &Path,
        base_url: &str,
        cred: impl Into<crate::api::auth::Credential>,
    ) -> Result<LiveBackend> {
        Self::open_with(data_dir, base_url, cred, SyncOptions::default())
    }

    pub fn open_with(
        data_dir: &Path,
        base_url: &str,
        cred: impl Into<crate::api::auth::Credential>,
        opts: SyncOptions,
    ) -> Result<LiveBackend> {
        let store = Store::open(&data_dir.join(DB_FILE))?;
        let scratch = crate::scratch_media::dir(data_dir);
        Ok(Self::from_parts(
            store,
            Api::new(base_url, cred),
            opts,
            Some(scratch),
        ))
    }

    fn from_parts(
        store: Store,
        api: Api,
        opts: SyncOptions,
        scratch_dir: Option<std::path::PathBuf>,
    ) -> LiveBackend {
        let start = opts.start_worker;
        let mut engine = Engine::new(store, api, opts);
        engine.scratch_dir = scratch_dir;
        let engine = Arc::new(engine);
        let (tx, rx) = channel();
        let typing = Arc::new(Typing::default());
        let writer = spawn_writer(engine.clone(), typing.clone(), tx.clone());
        let worker = start.then(|| spawn(engine.clone(), rx));
        LiveBackend {
            typing,
            writer: Mutex::new(Some(writer)),
            engine,
            public: PublicClient::new(),
            tx: Mutex::new(tx),
            worker: Mutex::new(worker),
            probe: crate::browse::probe::client(),
            browse: Default::default(),
        }
    }

    /// One stored reading row as pulled, tombstones included (`reading()`
    /// hides and collapses some).
    pub fn reading_row(&self, sub: &str, remote_id: &str) -> Option<ReadingItem> {
        self.engine
            .store
            .reading_row(sub, remote_id)
            .map(|(r, _)| r)
    }

    /// Pull from the server without pushing first (`sync_now` pushes, then pulls).
    pub fn pull_now(&self) -> Result<()> {
        self.engine.pull()
    }

    /// Whether the server has `GET /api/reading/imported` (false until the first pull proves otherwise).
    pub fn reading_available(&self) -> bool {
        !self.engine.reading_unavailable()
    }

    /// False only while an older Burrow's "the server can't take provenance"
    /// mark stands (cleared by the next push with provenance): every studio
    /// Burrow syncs with records it with the text.
    pub fn provenance_available(&self) -> bool {
        self.engine
            .store
            .meta(crate::sync::PROVENANCE_UNAVAILABLE)
            .is_none()
    }

    /// The TK provenance tracked for an item (`scopes` keyed to `keyed_to`).
    pub fn tracked_provenance(&self, id: &LocalId) -> crate::store::Tracked {
        self.engine.store.provenance(id)
    }

    /// `GET /api/hoppers` with counts; empty when the server doesn't have it.
    pub fn server_hoppers(&self) -> Result<Vec<Hopper>> {
        Ok(self
            .engine
            .track(self.engine.api.hoppers())?
            .unwrap_or_default())
    }

    fn send(&self, m: Msg) {
        if let Ok(tx) = self.tx.lock() {
            let _ = tx.send(m);
        }
    }

    fn wake(&self) {
        self.send(Msg::Wake);
        self.engine.emit_status();
    }

    /// The engine, with any typed text written first (`Typing`): what
    /// follows reads or changes the store as the user last left it.
    fn e(&self) -> &Engine {
        self.settle();
        &self.engine
    }

    /// Write pending typed text now, on this thread.
    fn settle(&self) {
        if !lock(&self.typing.pending).is_empty() {
            self.typing.write(&self.engine, &self.tx);
        }
    }

    fn row(&self, id: &LocalId) -> Result<crate::store::SyncRow> {
        self.e().store.row(id).ok_or(CoreError::NotFound)
    }

    fn server_id(&self, id: &LocalId) -> Result<String> {
        self.row(id)?
            .item
            .server_id
            .map(|s| s.0)
            .ok_or(CoreError::NotSynced)
    }

    /// Server id for an item, pushing it first if it's still local-only.
    fn ensure_on_server(&self, id: &LocalId) -> Result<String> {
        let row = self.row(id)?;
        if row.item.conflict {
            return Err(conflict_err());
        }
        if row.item.server_id.is_none() || row.item.pending_sync {
            self.e().flush(Some(id), true)?;
        }
        self.server_id(id)
    }

    /// Best-effort refresh of one item from `GET /api/items/:id` and its
    /// version history.
    fn refresh(&self, id: &LocalId, sid: &str) -> Option<WireItem> {
        let w = self.e().api.get_item_full(sid).ok()?;
        self.e().store.apply_server(id, &w).ok()?;
        Some(w)
    }

    /// A cached reading row and whether it came through a blyg-kind
    /// subscription (only those have a public item document and pins).
    fn remote_row(&self, sub_id: &str, remote_id: &str) -> Result<(ReadingItem, bool)> {
        let (it, sub_kind) = self
            .e()
            .store
            .reading_row(sub_id, remote_id)
            .ok_or(CoreError::NotFound)?;
        let blyg = match sub_kind.as_deref() {
            Some(k) => k == "blyg",
            None => self
                .e()
                .store
                .subscriptions()
                .iter()
                .any(|s| s.id == sub_id && s.kind == SubscriptionKind::Blyg),
        };
        Ok((it, blyg))
    }

    /// The public changelog, from the cache while it's fresh and covers the
    /// version we hold, else fetched; a failed fetch falls back to the cache.
    fn changelog(&self, it: &ReadingItem) -> Result<Vec<RemoteVersion>> {
        let cached = self.e().store.cached_changelog(&it.origin, &it.remote_id);
        if let Some((v, at)) = &cached {
            let fresh = crate::util::now_ms() - at < CHANGELOG_TTL_MS;
            let covers = v.iter().map(|r| r.version).max() >= Some(it.version);
            if fresh && covers {
                return Ok(v.clone());
            }
        }
        match self.public.item_doc(&it.origin, &it.remote_id) {
            Ok(doc) => {
                let v = doc.versions_at(&it.origin);
                self.e()
                    .store
                    .put_changelog(&it.origin, &it.remote_id, &v)?;
                Ok(v)
            }
            Err(e) => cached.map(|(v, _)| v).ok_or(e),
        }
    }

    fn pinned_for(&self, it: &ReadingItem, blyg: bool, version: u32) -> Result<PinnedVersion> {
        if !blyg {
            return Err(not_pinned(version));
        }
        if let Some(p) = self
            .e()
            .store
            .cached_pin(&it.origin, &it.remote_id, version)
        {
            return Ok(p);
        }
        // §8.4: no request for a version the changelog doesn't mark pinned.
        let pinned = self
            .changelog(it)?
            .iter()
            .any(|v| v.version == version && v.pinned);
        if !pinned {
            return Err(not_pinned(version));
        }
        match self.public.pinned(&it.origin, &it.remote_id, version) {
            Ok(p) => {
                self.e().store.put_pin(&p)?;
                Ok(p)
            }
            Err(e) => {
                if matches!(e, CoreError::Rejected { status: 404, .. }) {
                    // The changelog we trusted was wrong or stale.
                    self.e().store.drop_changelog(&it.origin, &it.remote_id)?;
                }
                Err(e)
            }
        }
    }

    /// Before a scratch note is promoted: upload its local images and put
    /// their blyg URLs in the text, so the draft is created with them. A
    /// failed upload refuses the promotion (the note stays scratch; files
    /// already uploaded are removed again, best effort). Offline, whatever
    /// made it is swapped in and the rest go up with the queued `create`
    /// (`Engine::run_op`).
    fn upload_scratch_media(&self, id: &LocalId, content: &str) -> Result<()> {
        if !crate::scratch_media::has_refs(content) {
            return Ok(());
        }
        let (done, err) = self.e().upload_scratch_media(content);
        match err {
            None | Some(CoreError::Offline) => {
                if !done.is_empty() {
                    self.e().store.rewrite_media(id, &done)?;
                }
                Ok(())
            }
            Some(e) => {
                for (_, url) in &done {
                    let _ = self.delete_media(url);
                }
                Err(match e {
                    CoreError::Rejected {
                        status,
                        message,
                        details,
                    } => CoreError::Rejected {
                        status,
                        message: format!("an image couldn't be uploaded ({message})"),
                        details,
                    },
                    e => e,
                })
            }
        }
    }

    /// Run an item command with its revision (`Engine::with_revision`),
    /// tracked for the status bar. An item whose text moved on the blyg
    /// under a local edit enters conflict, and the command isn't sent.
    fn command<T>(&self, id: &LocalId, sid: &str, f: impl Fn(u64) -> Result<T>) -> Result<T> {
        self.e()
            .track(self.e().with_revision(id, sid, f))?
            .ok_or_else(conflict_err)
    }

    fn permalink_for(&self, kind: Kind, sid: &str) -> String {
        let p = if kind == Kind::Thread { "t" } else { "f" };
        format!("{}/{p}/{sid}", self.e().api.public_base())
    }
}

// --- profiles ---

/// What a profile URL resolved to.
enum ProfileTarget {
    Blyg(String),
    Feed { feed: String, site: Option<String> },
}

/// Largest OPML / feed read for a profile.
const PROFILE_TEXT_MAX: u64 = 4 * 1024 * 1024;

impl LiveBackend {
    /// Resolve a URL the way subscribing does: what we already know (your
    /// own blyg, your subscriptions) first, then the subscribe preview (the
    /// owner Worker's §12.1 resolution). Only when that isn't available
    /// (no read extensions, not connected) do we probe the URL ourselves:
    /// `blyg.json` at it, then the URL as a feed. Two requests, no crawl.
    fn resolve_profile(&self, url: &str) -> Result<ProfileTarget> {
        use crate::profile::{normalize_origin, parse_feed, same_origin, under_origin};
        if let Some(own) = self.base_url()
            && under_origin(url, &own)
        {
            return Ok(ProfileTarget::Blyg(normalize_origin(&own).unwrap_or(own)));
        }
        for s in self.e().store.subscriptions() {
            let feed_match = same_origin(url, &s.feed_url);
            match s.kind {
                SubscriptionKind::Blyg if feed_match || under_origin(url, &s.origin) => {
                    return Ok(ProfileTarget::Blyg(
                        normalize_origin(&s.origin).unwrap_or(s.origin),
                    ));
                }
                SubscriptionKind::Rss if feed_match || same_origin(url, &s.origin) => {
                    return Ok(ProfileTarget::Feed {
                        feed: s.feed_url,
                        site: Some(s.origin),
                    });
                }
                _ => {}
            }
        }
        let preview_err = match self.preview_subscription(url) {
            Ok(p) => {
                return Ok(match p.kind {
                    SubscriptionKind::Blyg => {
                        let o = p.origin.unwrap_or_else(|| url.to_string());
                        ProfileTarget::Blyg(normalize_origin(&o).unwrap_or(o))
                    }
                    SubscriptionKind::Rss => ProfileTarget::Feed {
                        feed: p.feed_url.unwrap_or_else(|| url.to_string()),
                        site: p.origin,
                    },
                });
            }
            Err(CoreError::Offline) => return Err(CoreError::Offline),
            Err(e) => e,
        };
        if let Some(o) = normalize_origin(url)
            && let Some(m) = origin_url(&o, "blyg.json")
            && self
                .public
                .get_json(&m)
                .is_ok_and(|v| crate::profile::is_manifest(&v))
        {
            return Ok(ProfileTarget::Blyg(o));
        }
        match self.public.get_text(url, PROFILE_TEXT_MAX) {
            Ok(t) if parse_feed(&t).is_some() => Ok(ProfileTarget::Feed {
                feed: url.to_string(),
                site: None,
            }),
            Err(CoreError::Offline) => Err(CoreError::Offline),
            _ => Err(preview_err),
        }
    }

    fn fetch_profile(&self, url: &str) -> Result<crate::profile::Profile> {
        match self.resolve_profile(url)? {
            ProfileTarget::Blyg(origin) => self.fetch_blyg_profile(&origin),
            ProfileTarget::Feed { feed, site } => self.fetch_feed_profile(&feed, site),
        }
    }

    /// Manifest (required), then the blogroll when listed, the archive index
    /// and the feed (for titles and quote sources); each of those may fail.
    fn fetch_blyg_profile(&self, origin: &str) -> Result<crate::profile::Profile> {
        use crate::profile::*;
        let pub_ = &self.public;
        let at = |p: &str| origin_url(origin, p);
        let manifest_url =
            at("blyg.json").ok_or_else(|| CoreError::Other(format!("not a blyg: {origin}")))?;
        let m = parse_manifest(&pub_.get_json(&manifest_url)?, origin);
        let blogroll = m
            .blogroll
            .as_deref()
            .and_then(at)
            .and_then(|u| pub_.get_text(&u, PROFILE_TEXT_MAX).ok())
            .map(|t| parse_opml(&t))
            .unwrap_or_default();
        let index = at(m.items.as_deref().unwrap_or("items/index.json"))
            .and_then(|u| pub_.get_json(&u).ok())
            .map(|v| parse_index(&v))
            .unwrap_or_default();
        let feed_url = at(m.feed.as_deref().unwrap_or("feed.xml"));
        let feed = feed_url
            .as_deref()
            .and_then(|u| pub_.get_text(u, PROFILE_TEXT_MAX).ok())
            .and_then(|t| parse_feed(&t));
        let held = self.e().store.reading();
        let store = &self.e().store;
        let pinned = |id: &str| {
            held.iter()
                .filter(|r| r.remote_id == id && same_origin(&r.origin, origin))
                .any(|r| {
                    store
                        .cached_changelog(&r.origin, id)
                        .is_some_and(|(v, _)| v.iter().any(|x| x.pinned))
                })
        };
        let posts = blyg_posts(origin, &index, feed.as_ref(), &held, &pinned);
        let feed_quotes = feed
            .as_ref()
            .map(|f| {
                f.items
                    .iter()
                    .filter_map(|i| Some((i.blyg_id.clone()?, &i.quote_origins)))
                    .flat_map(|(id, os)| os.iter().map(move |o| (o.clone(), id.clone())))
                    .collect()
            })
            .unwrap_or_default();
        Ok(Profile {
            kind: ProfileKind::Blyg,
            origin: origin.to_string(),
            feed_url,
            name: m.name,
            title: m.title,
            bio: m.bio,
            avatar: m.avatar,
            links: m.links,
            has_blogroll: m.blogroll.is_some(),
            blogroll,
            posts,
            connections: vec![],
            feed_quotes,
            own: false,
            fetched_at: crate::util::now_ms(),
            stale: false,
        })
    }

    /// A plain feed: title, link, recent items. A feed that declares
    /// `<blyg:manifest>` is a blyg (the upgrade hook) and gets the full profile.
    fn fetch_feed_profile(
        &self,
        feed: &str,
        site: Option<String>,
    ) -> Result<crate::profile::Profile> {
        use crate::profile::*;
        let text = self.public.get_text(feed, PROFILE_TEXT_MAX)?;
        let f = parse_feed(&text)
            .ok_or_else(|| CoreError::Other(format!("{feed} isn't an RSS or Atom feed")))?;
        if let Some(m) = &f.manifest
            && let Some(o) = m.strip_suffix("blyg.json").and_then(normalize_origin)
        {
            return self.fetch_blyg_profile(&o);
        }
        let origin = f
            .link
            .clone()
            .or(site)
            .and_then(|l| normalize_origin(&l))
            .unwrap_or_else(|| feed.to_string());
        Ok(Profile {
            kind: ProfileKind::Feed,
            origin,
            feed_url: Some(feed.to_string()),
            name: f.title.clone(),
            title: None,
            bio: None,
            avatar: None,
            links: vec![],
            blogroll: vec![],
            has_blogroll: false,
            posts: feed_posts(&f),
            connections: vec![],
            feed_quotes: vec![],
            own: false,
            fetched_at: crate::util::now_ms(),
            stale: false,
        })
    }

    /// Mark your own blyg and compute connections from what's held locally.
    fn finish_profile(&self, mut p: crate::profile::Profile) -> crate::profile::Profile {
        use crate::profile::*;
        p.own = p.kind == ProfileKind::Blyg
            && self.base_url().is_some_and(|b| same_origin(&b, &p.origin));
        let mut held = self.e().store.reading();
        p.connections = if p.own {
            own_connections(&p.origin, &self.e().store.items(), &held)
        } else {
            // The owner API leaves lineage out: fill it from item documents
            // already fetched (the changelog cache), never fetching more.
            for r in held
                .iter_mut()
                .filter(|r| same_origin(&r.origin, &p.origin))
            {
                if let Some((log, _)) = self.e().store.cached_changelog(&r.origin, &r.remote_id)
                    && let Some(l) = crate::model::Lineage::of_changelog(&log)
                {
                    r.fill_lineage(l);
                }
            }
            connections(&p.origin, &held, &p.feed_quotes)
        };
        p
    }
}

/// `meta` key prefix: a media id this Mac attached → the post's local id
/// (removing an attachment needs its post's revision).
const MEDIA_ITEM: &str = "media_item:";

fn conflict_err() -> CoreError {
    CoreError::Rejected {
        status: 409,
        message: "resolve the sync conflict first".into(),
        details: vec![],
    }
}

impl Drop for LiveBackend {
    fn drop(&mut self) {
        self.settle();
        self.typing.stop.store(true, Ordering::SeqCst);
        self.typing.wake.notify_all();
        if let Some(h) = self.writer.lock().ok().and_then(|mut w| w.take()) {
            let _ = h.join();
        }
        self.send(Msg::Shutdown);
        if let Some(h) = self.worker.lock().ok().and_then(|mut w| w.take()) {
            let _ = h.join();
        }
    }
}

impl Backend for LiveBackend {
    // ---------------------------------------------------------------- local

    fn items(&self) -> Vec<Item> {
        self.e().store.items()
    }

    fn search(&self, query: &str) -> Vec<Item> {
        // --- loose ends --- operators filter; the words search as ever.
        let store = &self.e().store;
        crate::loose::search(
            query,
            |q| store.search(q),
            || store.items(),
            || self.loose_facts(),
        )
    }

    // --- contacts ---
    // --- syndication ---
    fn syndication(&self, id: &LocalId) -> Vec<crate::syndication::SyndicationEntry> {
        self.engine.store.syndication(id)
    }

    fn set_syndication(
        &self,
        id: &LocalId,
        entries: &[crate::syndication::SyndicationEntry],
    ) -> Result<Vec<crate::syndication::SyndicationEntry>> {
        let kept = self.e().store.set_syndication(id, entries)?;
        self.e().emit(CoreEvent::ItemsChanged);
        self.wake();
        Ok(kept)
    }

    fn refresh_syndication(
        &self,
        id: &LocalId,
    ) -> Result<Vec<crate::syndication::SyndicationEntry>> {
        let sid = self.server_id(id)?;
        let _net = self.e().net_lock();
        self.e().track(self.e().fetch_syndication(id, &sid))?;
        Ok(self.e().store.syndication(id))
    }

    fn syndication_served(&self) -> bool {
        self.engine.syndication_served()
    }

    fn reading_syndication(
        &self,
        sub_id: &str,
        remote_id: &str,
    ) -> Vec<crate::syndication::SyndicationEntry> {
        self.engine.store.reading_syndication(sub_id, remote_id)
    }
    // --- end syndication ---

    fn contacts(&self) -> Vec<crate::contacts::Contact> {
        self.engine
            .store
            .meta(CONTACTS_META)
            .and_then(|j| serde_json::from_str(&j).ok())
            .unwrap_or_default()
    }

    fn refresh_contacts(&self) -> Result<()> {
        let got = self.e().track(self.e().api.contacts())?;
        let store = &self.engine.store;
        store.set_meta(CONTACTS_SERVED_META, if got.is_some() { "1" } else { "0" })?;
        let json = serde_json::to_string(&got.unwrap_or_default())
            .map_err(|e| CoreError::Other(e.to_string()))?;
        store.set_meta(CONTACTS_META, &json)
    }

    fn rewrites_handles(&self) -> bool {
        self.engine.store.meta(CONTACTS_SERVED_META).as_deref() == Some("1")
    }

    fn reading_mentions(&self) -> Option<Vec<(String, String)>> {
        self.e()
            .track(self.e().api.reading_mentions())
            .ok()
            .flatten()
    }

    // --- contacts --- managing the book: each change, then the local copy
    // read again (a failed re-read keeps the old copy; the change stands).
    fn create_contact(
        &self,
        c: &crate::contacts::ContactInput,
    ) -> Result<crate::contacts::Contact> {
        let made = self.e().track(self.e().api.create_contact(c))?;
        let _ = self.refresh_contacts();
        Ok(made)
    }

    fn update_contact(
        &self,
        id: &str,
        c: &crate::contacts::ContactInput,
    ) -> Result<crate::contacts::Contact> {
        let made = self.e().track(self.e().api.update_contact(id, c))?;
        let _ = self.refresh_contacts();
        Ok(made)
    }

    fn delete_contact(&self, id: &str) -> Result<()> {
        self.e().track(self.e().api.delete_contact(id))?;
        let _ = self.refresh_contacts();
        Ok(())
    }

    fn resolve_contact(&self, id: &str) -> Result<crate::contacts::Contact> {
        let c = self.e().track(self.e().api.resolve_contact(id))?;
        let _ = self.refresh_contacts();
        Ok(c)
    }

    fn contact_seed(&self, origin: &str, title: &str) -> Option<crate::contacts::ContactSeed> {
        // `{origin}blyg.json` is always the manifest; its `author` names
        // the person. Unreadable: the subscription's title and origin.
        let author = url::Url::parse(origin)
            .and_then(|u| u.join("blyg.json"))
            .ok()
            .and_then(|u| self.public.get_json(u.as_str()).ok())
            .and_then(|m| serde_json::from_value::<Author>(m.get("author")?.clone()).ok());
        let seed = crate::contacts::ContactSeed::from_author(author.as_ref(), title, origin);
        crate::contacts::held(&self.contacts(), &seed.url, Some(origin))
            .is_none()
            .then_some(seed)
    }

    fn outbound_mentions(&self) -> Result<Option<Vec<crate::outbound::OutboundMention>>> {
        self.e().track(self.e().api.outbound_mentions())
    }

    fn mention_summary(&self) -> Result<Option<crate::outbound::MentionSummary>> {
        self.e().track(self.e().api.mention_summary())
    }

    fn mention_groups(&self) -> Result<Option<Vec<crate::outbound::MentionGroup>>> {
        self.e().track(self.e().api.mention_groups())
    }

    fn mention_source(&self, id: &str) -> Result<Option<crate::outbound::MentionSource>> {
        self.e().track(self.e().api.mention_source(id))
    }

    fn refresh_subscriptions(&self) -> Result<()> {
        let subs = self.e().track(self.e().api.list_subscriptions())?;
        if self.e().store.replace_subscriptions(&subs)? {
            self.e().emit(CoreEvent::ReadingChanged);
        }
        Ok(())
    }
    // --- end contacts ---

    // --- capture ---
    fn page_title(&self, url: &str) -> Option<String> {
        let (html, _) = self.public.get_page(url, 256 * 1024).ok()?;
        crate::api::public::html_title(&html)
    }

    // --- loose ends ---
    fn loose_facts(&self) -> crate::loose::Facts {
        let store = &self.engine.store;
        let mut f = store.loose_facts(self.base_url().as_deref());
        if let Some(ids) = store.meta(MENTIONED_META) {
            f.inbound.extend(ids.split_whitespace().map(str::to_string));
        }
        // --- upkeep --- the threads `GET /freshness` last named.
        f.stale_quotes = self
            .e()
            .cached_freshness()
            .into_iter()
            .map(|t| t.id.to_ascii_lowercase())
            .collect();
        f
    }

    /// Laid over with typed text not yet written: no wait on the writer.
    /// `pending` is held across the read, so a text the writer is landing
    /// right now is either still pending (laid over) or already stored.
    fn item(&self, id: &LocalId) -> Option<Item> {
        let pending = lock(&self.typing.pending);
        let item = self.engine.store.item(id)?;
        match pending.get(id) {
            Some(text) => Some(crate::store::saved_locally(&item, text)),
            None => Some(item),
        }
    }

    // --- nearby --- the index and the phrase search: the store, local only.

    fn nearby_vectors(&self, model: &str) -> Vec<crate::nearby::NearbyVector> {
        self.e().store.nearby_vectors(model).unwrap_or_default()
    }

    fn put_nearby_vectors(
        &self,
        model: &str,
        docs: &[String],
        rows: &[crate::nearby::NearbyVector],
    ) -> Result<()> {
        self.e().store.put_nearby_vectors(model, docs, rows)
    }

    fn retain_nearby_vectors(
        &self,
        model: &str,
        keep: &std::collections::HashSet<String>,
    ) -> Result<()> {
        self.e()
            .store
            .retain_nearby_vectors(model, keep)
            .map(|_| ())
    }

    fn nearby_phrase_hits(&self, phrases: &[String]) -> Vec<(String, usize)> {
        self.e()
            .store
            .nearby_phrase_hits(phrases)
            .unwrap_or_default()
    }
    // --- end nearby ---

    fn conflict_texts(&self, id: &LocalId) -> Option<(String, String)> {
        let row = self.e().store.row(id)?;
        row.item.conflict.then(|| {
            (
                row.item.content_md.clone(),
                row.theirs.clone().unwrap_or_default(),
            )
        })
    }

    fn create_draft(&self, kind: Kind, content_md: &str) -> Result<LocalId> {
        let id = self
            .e()
            .store
            .create_draft(kind, content_md, self.e().debounce_ms())?;
        self.wake();
        Ok(id)
    }

    fn save(&self, id: &LocalId, content_md: &str) -> Result<()> {
        self.e()
            .store
            .save(id, content_md, self.e().debounce_ms())?;
        self.wake();
        Ok(())
    }

    /// Handed to the typing writer; returns at once. A write that fails
    /// is reported as `CoreEvent::Error`.
    fn save_typing(&self, id: &LocalId, content_md: &str) -> Result<()> {
        lock(&self.typing.pending).insert(id.clone(), Arc::from(content_md));
        self.typing.wake.notify_one();
        Ok(())
    }

    fn save_if_base(&self, id: &LocalId, content_md: &str, base_hash: &str) -> Result<()> {
        self.e()
            .store
            .save_if_base(id, content_md, base_hash, self.e().debounce_ms())?;
        self.wake();
        Ok(())
    }

    fn set_kind(&self, id: &LocalId, kind: Kind) -> Result<()> {
        self.e().store.set_kind(id, kind, self.e().debounce_ms())?;
        self.wake();
        Ok(())
    }

    fn tracked_tk_provenance(
        &self,
        id: &LocalId,
    ) -> Option<(String, Vec<Option<ScopeProvenance>>)> {
        let t = LiveBackend::tracked_provenance(self, id);
        Some((t.keyed_to?, t.scopes?))
    }

    fn provenance_available(&self) -> bool {
        LiveBackend::provenance_available(self)
    }

    fn save_with_provenance(
        &self,
        id: &LocalId,
        content_md: &str,
        scopes: &[Option<ScopeProvenance>],
    ) -> Result<()> {
        crate::tk::validate(content_md, scopes).map_err(|message| CoreError::Rejected {
            status: 400,
            message,
            details: vec![],
        })?;
        self.e()
            .store
            .save_inner(id, content_md, Some(scopes), self.e().debounce_ms())?;
        self.wake();
        Ok(())
    }

    fn sync_status(&self) -> SyncStatus {
        self.engine.status()
    }

    /// The public origin (where `media/…` lives): learned from the
    /// server's permalinks, which carry the deployment's mount; the API base
    /// until the first one arrives (the same thing for a root mount).
    fn base_url(&self) -> Option<String> {
        // Asked on every preview render: never waits on the typing writer.
        let origin = self
            .engine
            .store
            .public_origin()
            .unwrap_or_else(|| self.engine.api.base_url().to_string());
        Some(origin.trim_end_matches('/').to_string())
    }

    fn set_event_sink(&self, sink: Box<dyn Fn(CoreEvent) + Send + Sync>) {
        self.engine.set_sink(Arc::from(sink));
    }

    fn resolve_conflict(&self, id: &LocalId, how: Resolution) -> Result<()> {
        self.e().store.resolve(id, how)?;
        self.e().emit(CoreEvent::ItemsChanged);
        self.wake();
        Ok(())
    }

    // The reading list and subscriptions don't hold your own posts' text:
    // no need to wait for the typing writer.
    fn reading(&self) -> Vec<ReadingItem> {
        self.engine.store.reading()
    }

    fn unread_count(&self) -> usize {
        self.engine.store.unread_count()
    }

    fn reading_with_id(&self, remote_id: &str) -> Vec<ReadingItem> {
        self.engine.store.reading_with_id(remote_id)
    }

    /// Typed text not yet written is laid over, as in `item`.
    fn own_item(&self, server_id: &str) -> Option<Item> {
        let id = self.engine.store.local_id_for_server(server_id)?;
        self.item(&id)
    }

    fn subscriptions(&self) -> Vec<Subscription> {
        self.engine.store.subscriptions()
    }

    fn mark_read(&self, sub_id: &str, remote_id: &str) -> Result<()> {
        // Instant and local; on a server that syncs read state (extension 5)
        // the same write queues one `read` op per row it raised.
        let sync = self.e().read_sync_on();
        let marked = self.e().store.mark_read_rows(sub_id, remote_id, sync)?;
        if !marked.is_empty() {
            self.e().emit(CoreEvent::ReadingChanged);
            if sync {
                self.wake();
            }
        }
        Ok(())
    }

    fn set_read(&self, rows: &[(String, String)], read: bool) -> Result<usize> {
        // Local and instant. Reads queue for a server that syncs read state;
        // unreads only for one that can clear it (else they stay here).
        let sync = self.e().read_sync_on();
        let n = if read {
            let marked = self.e().store.mark_many_read(rows, sync)?;
            marked.map_or(0, |m| m.len())
        } else {
            let queue = self.e().read_clear_on();
            let cleared = self.e().store.mark_many_unread(rows, queue)?;
            cleared.map_or(0, |m| m.len())
        };
        if n > 0 {
            self.e().emit(CoreEvent::ReadingChanged);
            if sync {
                self.wake();
            }
        }
        Ok(n)
    }

    // --- scratch notes ---

    fn create_scratch(&self, kind: Kind, content_md: &str) -> Result<LocalId> {
        let id = self.e().store.create_scratch(kind, content_md)?;
        self.e().emit(CoreEvent::ItemsChanged);
        Ok(id)
    }

    fn promote(&self, id: &LocalId, to: Promote) -> Result<Promoted> {
        let mut row = self.row(id)?;
        let kind = if row.item.status == Status::Scratch {
            // --- scratch media --- local images go up first.
            self.upload_scratch_media(id, &row.item.content_md)?;
            row = self.row(id)?;
            let kind = promotion_kind(row.item.kind, &row.item.content_md);
            self.e().store.promote_scratch(id, kind)?;
            self.e().emit(CoreEvent::ItemsChanged);
            self.wake();
            kind
        } else {
            row.item.kind
        };
        let published = match to {
            Promote::Draft => None,
            Promote::Publish { note } => Some(self.publish(id, note.as_deref())?),
        };
        Ok(Promoted { kind, published })
    }

    // --------------------------------------------------------------- remote

    fn publish(&self, id: &LocalId, note: Option<&str>) -> Result<PublishOutcome> {
        // --- access-ai --- one publish path, with or without the extras.
        self.publish_with(id, note, &crate::access_ai::PublishExtras::default())
    }

    fn publish_with(
        &self,
        id: &LocalId,
        note: Option<&str>,
        extras: &crate::access_ai::PublishExtras,
    ) -> Result<PublishOutcome> {
        let row = self.row(id)?;
        let content = row.item.content_md;
        // Decision #102: refused here, before anything is sent. A scratch
        // note's kind is chosen when it's promoted, and is checked then.
        if row.item.status != Status::Scratch
            && let Some(message) = crate::grammar::publish_error(row.item.kind, &content)
        {
            return Err(CoreError::Rejected {
                status: 400,
                message,
                details: vec![],
            });
        }
        self.e().check_publish(&content)?;
        if self.row(id)?.item.status == Status::Scratch {
            // A scratch note publishes by being promoted first.
            let note = note.map(str::to_string);
            return self
                .promote(id, Promote::Publish { note })?
                .published
                .ok_or_else(|| CoreError::Other("not published".into()));
        }
        let sid = self.ensure_on_server(id)?;
        let _net = self.e().net_lock();
        let row = self.row(id)?;
        // What the server holds right now is what gets published.
        let published = row
            .base_content
            .clone()
            .unwrap_or_else(|| row.item.content_md.clone());
        let p = self.command(id, &sid, |rev| {
            if *extras == crate::access_ai::PublishExtras::default() {
                self.e().api.publish(&sid, rev, note)
            } else {
                self.e().api.publish_with(&sid, rev, note, extras) // --- access-ai ---
            }
        })?;
        self.e().store.set_revision(id, p.revision)?;
        let permalink = self
            .refresh(id, &sid)
            .and_then(|w| w.permalink)
            .unwrap_or_else(|| self.permalink_for(row.item.kind, &sid));
        self.e()
            .store
            .after_publish(id, p.version, &permalink, &published)?;
        self.e().emit(CoreEvent::ItemsChanged);
        // --- contacts --- handles the blyg left as written.
        let warning = match (
            p.warning,
            crate::contacts::unresolved_warning(&p.unresolved_handles),
        ) {
            (Some(w), Some(u)) => Some(format!("{w} · {u}")),
            (w, u) => w.or(u),
        };
        Ok(PublishOutcome {
            version: p.version,
            permalink,
            warning,
        })
    }

    fn withdraw(&self, id: &LocalId, note: Option<&str>) -> Result<u32> {
        let sid = self.server_id(id)?;
        let _net = self.e().net_lock();
        let (v, revision) = self.command(id, &sid, |rev| self.e().api.withdraw(&sid, rev, note))?;
        self.e().store.set_revision(id, revision)?;
        if self.refresh(id, &sid).is_none() {
            self.e()
                .store
                .set_status_version(id, Status::Withdrawn, v)?;
        }
        self.e().emit(CoreEvent::ItemsChanged);
        Ok(v)
    }

    fn pin(&self, id: &LocalId, version: u32) -> Result<()> {
        let sid = self.server_id(id)?;
        let _net = self.e().net_lock();
        // Endcaps can't be pinned (§8; the server says 409 too). Refuse
        // locally, refreshing the version list first if it doesn't know `version`.
        let known = |vs: &[Version]| vs.iter().find(|v| v.version == version).cloned();
        let v = known(&self.e().store.versions(id)).or_else(|| {
            self.refresh(id, &sid);
            known(&self.e().store.versions(id))
        });
        if v.is_some_and(|v| v.endcap) {
            return Err(CoreError::Rejected {
                status: 409,
                message: "cannot pin an endcap version".into(),
                details: vec![],
            });
        }
        let p = self.command(id, &sid, |rev| self.e().api.pin(&sid, rev, version))?;
        self.e().store.set_revision(id, p.revision)?;
        self.refresh(id, &sid);
        Ok(())
    }

    fn versions(&self, id: &LocalId) -> Result<Vec<Version>> {
        let row = self.row(id)?;
        let Some(sid) = row.item.server_id else {
            return Ok(vec![]);
        };
        match self.e().track(self.e().api.get_item_full(&sid.0)) {
            Ok(w) => {
                let _net = self.e().net_lock();
                self.e().store.apply_server(id, &w)?;
                Ok(w.versions.unwrap_or_default())
            }
            Err(CoreError::Offline) => {
                let cached = self.e().store.versions(id);
                if cached.is_empty() {
                    Err(CoreError::Offline)
                } else {
                    Ok(cached)
                }
            }
            Err(e) => Err(e),
        }
    }

    /// Restoring the published version is Discard changes. The sync lock is
    /// held throughout, so no push of the old text runs during it (an
    /// in-flight one finishes first); `replace_from_server` then drops every
    /// edit op still waiting, so none runs after it either. A save made
    /// after the reload is a new edit: the app holds the editor's saves
    /// while a restore is on its way (blyg-app `discard.rs`). If the reload
    /// fails, the waiting ops stay and the edits sync back: the user is
    /// told the restore failed, and the text they see is what syncs.
    fn restore(&self, id: &LocalId, version: u32) -> Result<()> {
        let sid = self.server_id(id)?;
        let _net = self.e().net_lock();
        self.command(id, &sid, |rev| self.e().api.restore(&sid, rev, version))?;
        let w = self.e().track(self.e().api.get_item_full(&sid))?;
        self.e().store.replace_from_server(id, &w)?;
        self.e().emit(CoreEvent::ItemsChanged);
        self.e().emit_status();
        Ok(())
    }

    fn delete_draft(&self, id: &LocalId) -> Result<()> {
        let _net = self.e().net_lock();
        let row = self.row(id)?;
        if row.item.version > 0 {
            return Err(CoreError::Rejected {
                status: 409,
                message: "published items are withdrawn, not deleted".into(),
                details: vec![],
            });
        }
        if let Some(sid) = &row.item.server_id {
            match self.command(id, &sid.0, |rev| self.e().api.delete_item(&sid.0, rev)) {
                Ok(()) | Err(CoreError::NotFound) => {}
                Err(e) => return Err(e),
            }
        }
        self.e().store.delete_item(id)?;
        self.e().emit(CoreEvent::ItemsChanged);
        self.e().emit_status();
        Ok(())
    }

    fn upload_media(
        &self,
        bytes: Vec<u8>,
        mime: &str,
        item: Option<&LocalId>,
        alt: Option<&str>,
    ) -> Result<MediaRef> {
        let m = match item {
            Some(id) => {
                let sid = self.ensure_on_server(id)?;
                let _net = self.e().net_lock();
                let m = self.command(id, &sid, |rev| {
                    self.e()
                        .api
                        .upload_media(&bytes, mime, Some((&sid, rev)), alt)
                })?;
                if let Some(r) = m.revision {
                    self.e().store.set_revision(id, r)?;
                }
                // Removing it later needs the post it's attached to.
                self.e()
                    .store
                    .set_meta(&format!("{MEDIA_ITEM}{}", m.id), &id.0)?;
                m
            }
            None => self
                .e()
                .track(self.e().api.upload_media(&bytes, mime, None, alt))?,
        };
        Ok(MediaRef {
            url: m.url,
            mime: m.mime,
        })
    }

    /// An attachment is detached from its post at that post's revision, so
    /// one this Mac attached (`MEDIA_ITEM`) goes with its post's; a loose
    /// upload goes without one.
    fn delete_media(&self, url: &str) -> Result<()> {
        let id = crate::api::media::media_id(url).ok_or(CoreError::NotFound)?;
        let key = format!("{MEDIA_ITEM}{id}");
        let owner = self.e().store.meta(&key).map(LocalId);
        let sid = owner
            .as_ref()
            .and_then(|o| self.e().store.row(o))
            .and_then(|r| r.item.server_id)
            .map(|s| s.0);
        let r = match (&owner, &sid) {
            (Some(o), Some(sid)) => {
                let _net = self.e().net_lock();
                self.command(o, sid, |rev| self.e().api.delete_media(&id, Some(rev)))
                    .and_then(|rev| match rev {
                        Some(r) => self.e().store.set_revision(o, r),
                        None => Ok(()),
                    })
            }
            _ => self
                .e()
                .track(self.e().api.delete_media(&id, None))
                .map(|_| ()),
        };
        if matches!(r, Ok(()) | Err(CoreError::NotFound)) {
            self.e().store.delete_meta(&key)?;
        }
        r
    }

    fn fork(&self, of: &RemoteRef) -> Result<LocalId> {
        let _net = self.e().net_lock();
        // `POST /api/items {mode: "fork"}` answers with the new draft.
        let w = self.e().track(self.e().api.fork(of))?;
        let id = self.e().store.insert_from_server(&w)?;
        self.e().emit(CoreEvent::ItemsChanged);
        Ok(id)
    }

    fn set_responses(&self, id: &LocalId, mode: ResponsesMode) -> Result<bool> {
        let sid = self.server_id(id)?;
        let w = self.command(id, &sid, |rev| self.e().api.set_responses(&sid, rev, mode))?;
        let (showing, got) = (w.shows_responses(), w.responses_mode());
        self.e().store.set_revision(id, w.revision)?;
        self.e().store.set_responses(id, showing, got)?;
        self.e().emit(CoreEvent::ItemsChanged);
        Ok(showing)
    }

    fn set_highlight(&self, id: &LocalId, mode: HighlightMode) -> Result<bool> {
        let sid = self.server_id(id)?;
        let w = self.command(id, &sid, |rev| self.e().api.set_highlight(&sid, rev, mode))?;
        let (on, got) = (w.highlights(), w.highlight);
        self.e().store.set_revision(id, w.revision)?;
        self.e().store.set_highlight(id, on, got)?;
        self.e().emit(CoreEvent::ItemsChanged);
        Ok(on)
    }

    fn pick_search(&self, q: &crate::pick::PickQuery) -> Vec<crate::pick::Pickable> {
        self.e().store.pick_search(q).unwrap_or_default()
    }

    fn picker_typing(&self) -> PickerTyping {
        self.e().api.picker_typing().unwrap_or_default()
    }

    fn fetch_own_media(&self, url: &str) -> Result<(Vec<u8>, Option<String>)> {
        let api = &self.e().api;
        if api.is_own_media(url) {
            api.fetch_own_media(url, crate::backend::MEDIA_MAX_BYTES)
        } else {
            PublicClient::new().get_bytes(url, crate::backend::MEDIA_MAX_BYTES)
        }
    }

    fn sync_now(&self) -> Result<()> {
        self.e().sync_now()
    }

    fn preview_subscription(&self, url: &str) -> Result<SubscribePreview> {
        self.e().track(self.e().api.preview_subscription(url))
    }

    fn subscribe(&self, url: &str, title: Option<&str>) -> Result<Subscription> {
        let id = self.e().track(self.e().api.subscribe(url, title))?;
        let subs = self.e().api.list_subscriptions().unwrap_or_default();
        let sub = subs.iter().find(|s| s.id == id).cloned();
        let sub = match sub {
            Some(s) => {
                self.e().store.replace_subscriptions(&subs)?;
                s
            }
            None => {
                let s = Subscription {
                    id,
                    kind: SubscriptionKind::Blyg,
                    origin: url.to_string(),
                    feed_url: url.to_string(),
                    title: title.unwrap_or(url).to_string(),
                    status: "active".into(),
                    in_blogroll: false,
                    title_follows_source: title.is_none(),
                    poll: Default::default(),
                };
                let mut all = self.e().store.subscriptions();
                all.push(s.clone());
                self.e().store.replace_subscriptions(&all)?;
                s
            }
        };
        self.e().emit(CoreEvent::ReadingChanged);
        self.send(Msg::Pull);
        Ok(sub)
    }

    fn unsubscribe(&self, sub_id: &str) -> Result<()> {
        self.e().track(self.e().api.delete_subscription(sub_id))?;
        self.e().store.edit_subscription(sub_id, |_| false)?;
        self.e().emit(CoreEvent::ReadingChanged);
        self.send(Msg::Pull);
        Ok(())
    }

    fn set_subscription(
        &self,
        sub_id: &str,
        in_blogroll: Option<bool>,
        title: Option<&str>,
    ) -> Result<()> {
        self.e().track(
            self.e()
                .api
                .update_subscription(sub_id, in_blogroll, title.map(Some)),
        )?;
        self.e().store.edit_subscription(sub_id, |s| {
            if let Some(b) = in_blogroll {
                s.in_blogroll = b;
            }
            if let Some(t) = title.map(str::trim).filter(|t| !t.is_empty()) {
                s.title = t.to_string();
            }
            true
        })?;
        self.e().emit(CoreEvent::ReadingChanged);
        Ok(())
    }

    fn rename_subscription(&self, sub_id: &str, title: Option<&str>) -> Result<()> {
        let title = title.map(str::trim);
        if title == Some("") {
            return Err(CoreError::Rejected {
                status: 400,
                message: "a name can't be empty".into(),
                details: vec![],
            });
        }
        self.e()
            .track(self.e().api.update_subscription(sub_id, None, Some(title)))?;
        // The server's copy names it now (the source's own name comes back
        // with `title: null`); a failed refresh keeps the local edit.
        match self.e().api.list_subscriptions() {
            Ok(subs) => {
                self.e().store.replace_subscriptions(&subs)?;
            }
            Err(_) => self.e().store.edit_subscription(sub_id, |s| {
                if let Some(t) = title {
                    s.title = t.to_string();
                }
                s.title_follows_source = title.is_none();
                true
            })?,
        }
        self.e().emit(CoreEvent::ReadingChanged);
        Ok(())
    }

    fn poll_subscriptions(&self) -> Result<u32> {
        let n = self.e().track(self.e().api.poll_subscriptions())?;
        self.send(Msg::Pull);
        Ok(n)
    }

    fn check_subscription(&self, sub_id: &str) -> Result<bool> {
        let kind = self
            .e()
            .store
            .subscriptions()
            .into_iter()
            .find(|s| s.id == sub_id)
            .map(|s| s.kind);
        // Resync is for blyg subscriptions (409 on a feed): a feed is polled
        // with the rest.
        let changed = if kind == Some(SubscriptionKind::Rss) {
            self.e().track(self.e().api.poll_subscriptions())? > 0
        } else {
            self.e().track(self.e().api.resync_subscription(sub_id))?
        };
        self.send(Msg::Pull);
        Ok(changed)
    }

    fn pause_subscription(&self, sub_id: &str, paused: bool) -> Result<()> {
        self.e()
            .track(self.e().api.pause_subscription(sub_id, paused))?;
        self.e().store.edit_subscription(sub_id, |s| {
            s.status = if paused { "paused" } else { "active" }.into();
            true
        })?;
        self.e().emit(CoreEvent::ReadingChanged);
        Ok(())
    }

    fn signal(&self, sub_id: &str, remote_id: &str, thumb: Option<i8>) -> Result<()> {
        let r = match thumb {
            Some(t @ (1 | -1)) => self.e().api.set_signal(sub_id, remote_id, t),
            Some(_) => {
                return Err(CoreError::Rejected {
                    status: 400,
                    message: "thumb must be 1 or -1".into(),
                    details: vec![],
                });
            }
            None => self.e().api.clear_signal(sub_id, remote_id),
        };
        self.e().track(r)?;
        if self.e().store.set_thumb(sub_id, remote_id, thumb)? {
            self.e().emit(CoreEvent::ReadingChanged);
        }
        Ok(())
    }

    fn mentions(&self) -> Result<Vec<Mention>> {
        let v = self.e().track(self.e().api.mentions())?.unwrap_or_default();
        // --- loose ends --- the posts a verified mention names, kept for
        // `is:orphan` (the mentions themselves stay on the blyg).
        let mut ids: Vec<String> = v
            .iter()
            .filter(|m| m.status == "verified")
            .map(|m| m.target_item_id.to_ascii_lowercase())
            .collect();
        ids.sort();
        ids.dedup();
        let _ = self.engine.store.set_meta(MENTIONED_META, &ids.join(" "));
        Ok(v)
    }

    fn set_mention_hidden(&self, mention_id: &str, hidden: bool) -> Result<()> {
        self.e()
            .track(self.e().api.set_mention_hidden(mention_id, hidden))
    }

    /// `NotFound` means the server doesn't expose `GET /api/settings` yet.
    fn settings(&self) -> Result<Settings> {
        self.e()
            .track(self.e().api.settings())?
            .ok_or(CoreError::NotFound)
    }

    fn save_settings(&self, settings: &Settings) -> Result<()> {
        self.e().track(self.e().api.put_settings(settings))
    }

    fn remote_versions(&self, sub_id: &str, remote_id: &str) -> Result<Vec<RemoteVersion>> {
        let (it, blyg) = self.remote_row(sub_id, remote_id)?;
        if !blyg {
            // RSS/L0: no public changelog, just what the feed shows now.
            return Ok(vec![RemoteVersion {
                version: it.version,
                at: it.updated.clone().unwrap_or_else(|| it.observed_at.clone()),
                note: None,
                pinned: false,
                current: true,
                media: vec![],
                lineage: Default::default(),
            }]);
        }
        self.changelog(&it)
    }

    fn remote_pinned(&self, sub_id: &str, remote_id: &str, version: u32) -> Result<PinnedVersion> {
        let (it, blyg) = self.remote_row(sub_id, remote_id)?;
        self.pinned_for(&it, blyg, version)
    }

    fn pinned_diff_base(&self, sub_id: &str, remote_id: &str) -> Option<PinnedVersion> {
        let (it, blyg) = self.remote_row(sub_id, remote_id).ok()?;
        let read = it.read_version?;
        self.pinned_for(&it, blyg, read).ok()
    }

    // --- reading ---

    fn read_extensions_available(&self) -> bool {
        self.reading_available()
    }

    fn create_stub(&self, of: &RemoteRef, content_md: &str) -> Result<LocalId> {
        let id = self
            .e()
            .store
            .create_stub(of, content_md, self.e().debounce_ms())?;
        self.e().emit(CoreEvent::ItemsChanged);
        self.wake();
        Ok(id)
    }

    // --- scratch media ---

    fn save_scratch_media(&self, bytes: &[u8], mime: &str) -> Result<String> {
        let dir = self
            .e()
            .scratch_dir
            .as_deref()
            .ok_or_else(|| CoreError::Other("no place for local images".into()))?;
        crate::scratch_media::store(dir, bytes, mime)
    }

    fn scratch_media_file(&self, url: &str) -> Option<std::path::PathBuf> {
        crate::scratch_media::file(self.e().scratch_dir.as_deref()?, url)
    }

    // --- profiles ---

    fn profile(&self, url: &str, refresh: bool) -> Result<crate::profile::Profile> {
        use crate::profile::{PROFILE_TTL_MS, clean_url};
        let key =
            clean_url(url).ok_or_else(|| CoreError::Other(format!("not a web address: {url}")))?;
        let store = &self.e().store;
        if !refresh
            && let Some((p, at)) = store.cached_profile(&key)
            && crate::util::now_ms() - at < PROFILE_TTL_MS
        {
            return Ok(self.finish_profile(p));
        }
        match self.fetch_profile(&key) {
            Ok(p) => {
                store.put_profile(&[&key, &p.origin], &p)?;
                Ok(self.finish_profile(p))
            }
            Err(e) => match store.cached_profile(&key) {
                Some((mut p, _)) => {
                    p.stale = true;
                    Ok(self.finish_profile(p))
                }
                None => Err(e),
            },
        }
    }

    fn cached_profile(&self, url: &str) -> Option<crate::profile::Profile> {
        let key = crate::profile::clean_url(url)?;
        let (p, _) = self.e().store.cached_profile(&key)?;
        Some(self.finish_profile(p))
    }

    // --- site ---

    fn site_config(&self) -> Result<crate::site::SiteConfig> {
        self.e()
            .track(self.e().api.site_config())?
            .ok_or(CoreError::NotFound)
    }

    fn save_site_config(
        &self,
        base: &crate::site::SiteConfig,
        next: &crate::site::SiteConfig,
    ) -> Result<crate::site::SiteConfig> {
        let patch = next.patch(base);
        if patch.is_empty() {
            return Ok(next.clone());
        }
        let mut saved = self.e().track(self.e().api.put_site_config(patch))?;
        saved.update = self.e().api.cached_studio_update();
        Ok(saved)
    }

    fn studio_update(&self) -> Option<crate::site::StudioUpdate> {
        self.engine.api.cached_studio_update()
    }

    fn refresh_studio_update(&self, force: bool) -> Result<Option<crate::site::StudioUpdate>> {
        self.engine.api.refresh_studio_update(force)
    }
    // --- end site ---

    // --- about ---

    fn read_state_sync(&self) -> bool {
        self.e().read_sync_on()
    }

    fn server_outdated(&self) -> bool {
        self.e().server_outdated()
    }

    fn server_extensions(&self) -> Option<bool> {
        self.e().server_extensions()
    }

    fn cached_profiles(&self) -> Vec<crate::profile::Profile> {
        self.e().store.cached_profiles()
    }

    // --- responses ---

    fn responses(&self, origin: &str, id: &str) -> Vec<Response> {
        let mut out = self.e().store.responses_to(origin, id);
        if let Some(base) = self.base_url() {
            out.extend(crate::model::own_responses(
                &self.items(),
                &base,
                origin,
                id,
            ));
        }
        out
    }

    // --- reader folders --- local only: the store, nothing queued.

    fn folders(&self) -> Vec<Folder> {
        self.e().store.folders()
    }

    fn subscription_folders(&self) -> std::collections::HashMap<String, String> {
        self.e().store.subscription_folders()
    }

    fn create_folder(&self, name: &str) -> Result<Folder> {
        self.e().store.create_folder(name)
    }

    fn rename_folder(&self, id: &str, name: &str) -> Result<()> {
        self.e().store.rename_folder(id, name)
    }

    fn delete_folder(&self, id: &str) -> Result<()> {
        self.e().store.delete_folder(id)
    }

    fn move_folder(&self, id: &str, index: usize) -> Result<()> {
        self.e().store.move_folder(id, index)
    }

    fn set_subscription_folder(&self, sub_id: &str, folder: Option<&str>) -> Result<()> {
        self.e().store.set_subscription_folder(sub_id, folder)
    }

    // --- shelf --- local only: the store, nothing queued.

    fn shelf(&self) -> Vec<LocalId> {
        self.e().store.shelf()
    }

    fn set_on_shelf(&self, id: &LocalId, on: bool) -> Result<()> {
        self.e().store.set_on_shelf(id, on)?;
        self.e().emit(CoreEvent::ItemsChanged);
        Ok(())
    }

    fn move_on_shelf(&self, id: &LocalId, index: usize) -> Result<()> {
        self.e().store.move_on_shelf(id, index)?;
        self.e().emit(CoreEvent::ItemsChanged);
        Ok(())
    }

    // --- trails --- local only: the store, nothing queued.

    fn trails(&self) -> Vec<crate::trails::Trail> {
        self.e().store.trails()
    }

    fn create_trail(&self, name: &str) -> Result<crate::trails::Trail> {
        self.e().store.create_trail(name)
    }

    fn rename_trail(&self, id: &str, name: &str) -> Result<()> {
        self.e().store.rename_trail(id, name)
    }

    fn delete_trail(&self, id: &str) -> Result<()> {
        self.e().store.delete_trail(id)
    }

    fn trail_stops(&self, trail: &str) -> Vec<crate::trails::TrailStop> {
        self.e().store.trail_stops(trail)
    }

    fn add_trail_stop(&self, trail: &str, stop: &crate::trails::TrailStop) -> Result<usize> {
        self.e().store.add_trail_stop(trail, stop)
    }

    fn set_trail_why(&self, trail: &str, index: usize, why: &str) -> Result<()> {
        self.e().store.set_trail_why(trail, index, why)
    }

    fn remove_trail_stop(&self, trail: &str, index: usize) -> Result<()> {
        self.e().store.remove_trail_stop(trail, index)
    }

    fn move_trail_stop(&self, trail: &str, from: usize, to: usize) -> Result<()> {
        self.e().store.move_trail_stop(trail, from, to)
    }

    // --- quote targets ---

    fn public_item(&self, origin: &str, id: &str) -> Result<crate::backend::PublicItem> {
        // `PublicClient` holds no token: nothing but a public GET leaves.
        let doc = self.public.item_doc(origin, id)?;
        let versions = doc.versions_at(origin);
        let _ = self.e().store.put_changelog(origin, id, &versions);
        Ok(crate::backend::PublicItem {
            item: doc.reading_item(origin, id),
            versions,
        })
    }

    fn public_pinned(&self, origin: &str, id: &str, version: u32) -> Result<PinnedVersion> {
        if let Some(p) = self.e().store.cached_pin(origin, id, version) {
            return Ok(p);
        }
        // §8.4: no request for a version the changelog doesn't mark pinned.
        let log = match self.e().store.cached_changelog(origin, id) {
            Some((v, _)) if v.iter().any(|r| r.version == version) => v,
            _ => self.public.item_doc(origin, id)?.versions_at(origin),
        };
        if !log.iter().any(|v| v.version == version && v.pinned) {
            return Err(not_pinned(version));
        }
        let p = self.public.pinned(origin, id, version)?;
        self.e().store.put_pin(&p)?;
        Ok(p)
    }

    // --- lineage counts --- (`crate::lineage::reads`)

    fn lineage_summaries(
        &self,
        keys: &[String],
        max_age_ms: i64,
    ) -> Option<std::collections::HashMap<String, crate::lineage::LineageSummary>> {
        crate::lineage::reads::summaries(self.e(), keys, max_age_ms, crate::util::now_ms())
    }

    fn cached_lineage_summaries(
        &self,
        keys: &[String],
    ) -> Option<std::collections::HashMap<String, crate::lineage::LineageSummary>> {
        crate::lineage::reads::cached_summaries(self.e(), keys)
    }

    fn lineage_graph(
        &self,
        centre: &crate::lineage::Centre,
        max_age_ms: i64,
    ) -> Option<crate::lineage::LineageGraph> {
        crate::lineage::reads::graph(self.e(), centre, max_age_ms, crate::util::now_ms())
    }

    fn cached_lineage_graph(
        &self,
        centre: &crate::lineage::Centre,
    ) -> Option<crate::lineage::LineageGraph> {
        crate::lineage::reads::cached_graph(self.e(), centre)
    }

    // --- browse --- (`crate::browse::reads`)

    fn classify_links(&self, urls: &[String]) -> Result<Vec<crate::browse::LinkTarget>> {
        Ok(crate::browse::reads::classify(
            self.e(),
            &self.probe,
            urls,
            crate::util::now_ms(),
        ))
    }

    fn log_browse(&self, step: &crate::browse::NewBrowseStep) -> Result<i64> {
        crate::browse::reads::log(self.e(), &self.browse, step, crate::util::now_ms())
    }

    fn browse_history(
        &self,
        trail: Option<&str>,
        offset: usize,
        limit: usize,
    ) -> Result<Vec<crate::browse::BrowseStep>> {
        crate::browse::reads::history(
            self.e(),
            &self.browse,
            trail,
            offset,
            limit,
            crate::util::now_ms(),
        )
    }

    fn clear_browse(&self, trail: Option<&str>) -> Result<()> {
        crate::browse::reads::clear(self.e(), &self.browse, trail, crate::util::now_ms())
    }

    fn background_reading(&self) -> Vec<ReadingItem> {
        self.e().store.background_reading()
    }

    // --- hoppers ---
    fn hoppers(&self) -> Vec<Hopper> {
        self.e().store.hoppers()
    }

    fn hopper_members(&self) -> std::collections::HashMap<(String, String), Vec<String>> {
        self.e().store.hopper_members()
    }

    fn hopper_unheld(&self) -> Vec<ReadingItem> {
        self.e().store.hopper_unheld()
    }

    fn hopper_order(&self) -> std::collections::HashMap<String, Vec<(String, String)>> {
        self.e().store.hopper_order()
    }

    fn create_hopper(&self, name: &str) -> Result<Hopper> {
        let name = clean_hopper_name(name)?;
        let h = self.e().track(self.e().api.create_hopper(&name))?;
        let h = self.e().store.put_hopper(&h)?;
        self.e().emit(CoreEvent::ReadingChanged);
        Ok(h)
    }

    fn update_hopper(&self, id: &str, edit: &HopperEdit) -> Result<Hopper> {
        let mut edit = edit.clone();
        if let Some(n) = &edit.name {
            edit.name = Some(clean_hopper_name(n)?);
        }
        if let Some(d) = &edit.description {
            let d = d.trim();
            if d.chars().count() > HOPPER_DESCRIPTION_MAX {
                return Err(CoreError::Rejected {
                    status: 400,
                    message: format!(
                        "A description is at most {HOPPER_DESCRIPTION_MAX} characters"
                    ),
                    details: vec![],
                });
            }
            edit.description = Some(d.to_string());
        }
        let h = match self.e().track(self.e().api.update_hopper(id, &edit)) {
            Ok(h) => h,
            // The hopper changed on the blyg while the patch was applied
            // (studio 409): read it again, then send what's still wanted
            // once, when that's safe.
            Err(e) if is_hopper_conflict(&e) => return self.update_hopper_again(id, &edit),
            Err(e) => return Err(e),
        };
        let h = self.e().store.put_hopper(&h)?;
        self.e().emit(CoreEvent::ReadingChanged);
        Ok(h)
    }

    fn delete_hopper(&self, id: &str) -> Result<()> {
        match self.e().track(self.e().api.delete_hopper(id)) {
            // Already gone there: gone here too.
            Ok(()) | Err(CoreError::NotFound) => {}
            Err(e) => return Err(e),
        }
        self.e().store.remove_hopper(id)?;
        self.e().emit(CoreEvent::ReadingChanged);
        Ok(())
    }

    fn set_in_hopper(&self, hopper: &str, sub_id: &str, remote_id: &str, on: bool) -> Result<()> {
        if self
            .e()
            .store
            .set_hopper_member(hopper, sub_id, remote_id, on)?
        {
            self.e().emit(CoreEvent::ReadingChanged);
            self.wake();
        }
        Ok(())
    }

    // --- access-ai --- Client access goes with the studio session, never
    // through `track`: its 401 means "the studio password is needed", not
    // "Burrow's own sign-in ended".
    fn access_gate(&self) -> crate::access_ai::AccessGate {
        self.e().api.access_gate()
    }

    fn access_sign_in(&self, password: &str) -> Result<()> {
        self.e().api.access_sign_in(password)
    }

    fn own_client_id(&self) -> Option<String> {
        self.e().api.own_client_id()
    }

    fn authorizations(&self) -> Result<Vec<crate::access_ai::Authorization>> {
        self.e().api.authorizations()
    }

    fn create_authorization(
        &self,
        new: &crate::access_ai::NewAuthorization,
    ) -> Result<crate::access_ai::NewToken> {
        self.e().api.create_authorization(new)
    }

    fn revoke_authorization(&self, id: &str) -> Result<()> {
        self.e().api.revoke_authorization(id)
    }

    fn revoke_all_authorizations(&self) -> Result<()> {
        self.e().api.revoke_all_authorizations()
    }

    fn ai_models(&self) -> Result<crate::access_ai::AiModels> {
        self.e()
            .track(self.e().api.ai_models())?
            .ok_or(CoreError::NotFound)
    }

    fn server_ai_settings(&self) -> Result<crate::access_ai::ServerAiSettings> {
        self.e()
            .track(self.e().api.server_ai_settings())?
            .ok_or(CoreError::NotFound)
    }

    fn save_server_ai_settings(&self, s: &crate::access_ai::ServerAiSettings) -> Result<()> {
        self.e()
            .track(self.e().api.put_server_ai_settings(s))
            .map(|_| ())
    }

    fn draft_note(&self, id: &LocalId) -> Result<crate::access_ai::DraftedNote> {
        let sid = self.ensure_on_server(id)?;
        let _net = self.e().net_lock();
        self.command(id, &sid, |rev| self.e().api.draft_note(&sid, rev))
    }

    fn classify_change(&self, id: &LocalId) -> Result<crate::access_ai::ChangeClass> {
        let sid = self.ensure_on_server(id)?;
        let _net = self.e().net_lock();
        self.command(id, &sid, |rev| self.e().api.classify_change(&sid, rev))
    }
    // --- end access-ai ---
    // --- upkeep --- stale quotes, signals, a version's words.

    fn cached_stale_quotes(&self) -> Vec<crate::upkeep::ThreadFreshness> {
        self.e().cached_freshness()
    }

    fn stale_quotes(&self) -> Result<Vec<crate::upkeep::ThreadFreshness>> {
        match self.e().track(self.e().fetch_freshness())? {
            Some((list, changed)) => {
                if changed {
                    self.e().emit(CoreEvent::ItemsChanged);
                }
                Ok(list)
            }
            None => Err(CoreError::Other(
                "your blyg doesn't check quotes (update it)".into(),
            )),
        }
    }

    fn quote_freshness(&self, id: &LocalId) -> Result<crate::upkeep::ThreadFreshness> {
        let sid = self.server_id(id)?;
        self.e().track(self.e().api.item_freshness(&sid, true))
    }

    /// A refresh is a publish of the published words: the item's version,
    /// permalink and base move as after `publish`; its text doesn't.
    fn refresh_quotes(&self, id: &LocalId, note: Option<&str>) -> Result<crate::upkeep::Refreshed> {
        let sid = self.server_id(id)?;
        let _net = self.e().net_lock();
        let row = self.row(id)?;
        let published = row
            .base_content
            .clone()
            .unwrap_or_else(|| row.item.content_md.clone());
        let r = self.command(id, &sid, |rev| self.e().api.refresh_quotes(&sid, rev, note))?;
        self.e().store.set_revision(id, r.revision)?;
        let permalink = self
            .refresh(id, &sid)
            .and_then(|w| w.permalink)
            .or(row.item.permalink.clone())
            .unwrap_or_else(|| self.permalink_for(row.item.kind, &sid));
        self.e()
            .store
            .after_publish(id, r.version, &permalink, &published)?;
        // The thread is current now; the next pull confirms it.
        let rest: Vec<_> = self
            .e()
            .cached_freshness()
            .into_iter()
            .filter(|t| !t.id.eq_ignore_ascii_case(&sid))
            .collect();
        self.e().keep_freshness(rest)?;
        self.e().emit(CoreEvent::ItemsChanged);
        Ok(r)
    }

    fn version_text(&self, id: &LocalId, version: u32) -> Result<String> {
        let sid = self.server_id(id)?;
        let v = self.e().track(self.e().api.item_version(&sid, version))?;
        Ok(if v.kind == "withdrawn" {
            String::new()
        } else {
            v.content_md
        })
    }

    fn thumbs_page(
        &self,
        thumb: i8,
        cursor: Option<&str>,
    ) -> Result<crate::upkeep::Page<crate::upkeep::Thumb>> {
        self.e().track(self.e().api.thumbs(thumb, cursor))
    }

    fn interactions_page(
        &self,
        cursor: Option<&str>,
    ) -> Result<crate::upkeep::Page<crate::upkeep::Interaction>> {
        self.e().track(self.e().api.interactions(cursor))
    }
    // --- end upkeep ---
    // --- changelog display ---

    fn changelog_settings(&self) -> Result<Option<crate::changelog::ChangelogSettings>> {
        self.e().track(self.e().api.changelog_settings())
    }

    fn save_changelog_settings(
        &self,
        s: &crate::changelog::ChangelogSettings,
    ) -> Result<crate::changelog::ChangelogSettings> {
        self.e().track(self.e().api.put_changelog_settings(s))
    }

    fn summarize_reading_changelog(
        &self,
        sub_id: &str,
        remote_id: &str,
    ) -> Result<crate::changelog::ChangelogSummary> {
        let of = crate::changelog::SummaryOf::Imported {
            subscription_id: sub_id.to_string(),
            remote_id: remote_id.to_string(),
        };
        self.e().track(self.e().api.summarize_changelog(&of))
    }

    fn summarize_own_changelog(&self, id: &LocalId) -> Result<crate::changelog::ChangelogSummary> {
        let sid = self.server_id(id)?;
        let of = crate::changelog::SummaryOf::Own(sid);
        self.e().track(self.e().api.summarize_changelog(&of))
    }

    fn draft_note_history(&self, id: &LocalId) -> Result<crate::access_ai::DraftedNote> {
        let sid = self.ensure_on_server(id)?;
        let _net = self.e().net_lock();
        self.command(id, &sid, |rev| self.e().api.draft_note_history(&sid, rev))
    }
    // --- end changelog display ---
}

impl LiveBackend {
    /// --- hoppers --- After a 409 on `PATCH /api/hoppers/{id}`: read the
    /// hopper as the blyg has it now (held here from then on), and send
    /// what `edit` still changes, once, when that's safe ([`reapply`]).
    fn update_hopper_again(&self, id: &str, edit: &HopperEdit) -> Result<Hopper> {
        let held = self.e().store.hoppers().into_iter().find(|h| h.id == id);
        let fresh = match self.e().track(self.e().api.hopper(id)) {
            Ok(h) => h,
            Err(CoreError::NotFound) => {
                self.e().store.remove_hopper(id)?;
                self.e().emit(CoreEvent::ReadingChanged);
                return Err(hopper_conflict_error(
                    "This hopper was deleted on your blyg while Burrow was saving the change."
                        .into(),
                ));
            }
            Err(e) => return Err(e),
        };
        let stored = self.e().store.put_hopper(&fresh)?;
        self.e().emit(CoreEvent::ReadingChanged);
        let again = match reapply(edit, held.as_ref(), &fresh) {
            Reapply::Done => return Ok(stored),
            Reapply::Ask(why) => return Err(hopper_conflict_error(why)),
            Reapply::Retry(again) => again,
        };
        match self.e().track(self.e().api.update_hopper(id, &again)) {
            Ok(h) => {
                let h = self.e().store.put_hopper(&h)?;
                self.e().emit(CoreEvent::ReadingChanged);
                Ok(h)
            }
            Err(e) if is_hopper_conflict(&e) => Err(hopper_conflict_error(format!(
                "“{}” kept changing on your blyg, so Burrow didn't save this. It now shows the \
                 hopper as your blyg has it; try again in a moment.",
                fresh.name
            ))),
            Err(e) => Err(e),
        }
    }
}

/// --- hoppers --- A hopper change that wasn't made, in words (`Rejected`
/// 409, which the UI shows as is).
fn hopper_conflict_error(message: String) -> CoreError {
    CoreError::Rejected {
        status: 409,
        message,
        details: vec![],
    }
}

/// --- hoppers --- What to do with a hopper edit the blyg refused with 409,
/// now that the hopper has been read again.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Reapply {
    /// The hopper is already as the edit wants it.
    Done,
    /// Send this (what the edit still changes) once more.
    Retry(HopperEdit),
    /// Not safe to send again; this says why.
    Ask(String),
}

/// A name, a description and making it private are the same change against
/// whatever the hopper is now, so they're sent again. Making it public
/// publishes a page at the address the user was shown (its slug): sent
/// again only while that address still holds.
pub(crate) fn reapply(edit: &HopperEdit, held: Option<&Hopper>, fresh: &Hopper) -> Reapply {
    let mut left = HopperEdit::default();
    if let Some(n) = edit.name.as_ref().filter(|n| **n != fresh.name) {
        left.name = Some(n.clone());
    }
    if let Some(d) = edit.description.as_ref() {
        let want = Some(d.trim()).filter(|d| !d.is_empty());
        if want != fresh.description.as_deref() {
            left.description = Some(d.clone());
        }
    }
    if let Some(p) = edit.public.filter(|p| *p != fresh.public) {
        left.public = Some(p);
    }
    if left == HopperEdit::default() {
        return Reapply::Done;
    }
    if left.public == Some(true) && held.is_some_and(|h| h.slug != fresh.slug) {
        return Reapply::Ask(format!(
            "“{}” changed on your blyg while Burrow was making it public, and its page would \
             now have a different address. Burrow shows it as your blyg has it; make it public \
             again if you still want to.",
            fresh.name
        ));
    }
    Reapply::Retry(left)
}

/// --- hoppers --- A hopper name as sent: trimmed, never empty.
fn clean_hopper_name(name: &str) -> Result<String> {
    let n = name.trim();
    if n.is_empty() {
        return Err(CoreError::Rejected {
            status: 400,
            message: "A hopper needs a name".into(),
            details: vec![],
        });
    }
    Ok(n.to_string())
}

#[cfg(test)]
mod hopper_conflict_tests {
    use super::*;

    fn hopper(name: &str, slug: &str, public: bool) -> Hopper {
        Hopper {
            id: "h1".into(),
            name: name.into(),
            slug: Some(slug.into()),
            public,
            count: 0,
            description: None,
            slug_frozen: public,
        }
    }

    fn edit(f: impl FnOnce(&mut HopperEdit)) -> HopperEdit {
        let mut e = HopperEdit::default();
        f(&mut e);
        e
    }

    #[test]
    fn a_rename_or_description_is_sent_again_unless_already_so() {
        let held = hopper("Slow", "slow", false);
        let fresh = hopper("Slow media", "slow-media", false);
        let rename = edit(|e| e.name = Some("Long reads".into()));
        assert_eq!(
            reapply(&rename, Some(&held), &fresh),
            Reapply::Retry(rename.clone())
        );
        // Someone else already gave it that name: nothing to send.
        let same = edit(|e| e.name = Some("Slow media".into()));
        assert_eq!(reapply(&same, Some(&held), &fresh), Reapply::Done);
        let describe = edit(|e| e.description = Some("Takes its time.".into()));
        assert_eq!(
            reapply(&describe, Some(&held), &fresh),
            Reapply::Retry(describe.clone())
        );
        // An empty description clears it: already clear.
        let clear = edit(|e| e.description = Some("  ".into()));
        assert_eq!(reapply(&clear, Some(&held), &fresh), Reapply::Done);
    }

    #[test]
    fn making_it_public_is_sent_again_only_at_the_address_shown() {
        let held = hopper("Slow", "slow", false);
        let public = edit(|e| e.public = Some(true));
        // Same address: safe.
        let fresh = hopper("Slow", "slow", false);
        assert_eq!(
            reapply(&public, Some(&held), &fresh),
            Reapply::Retry(public.clone())
        );
        // Renamed elsewhere meanwhile: its page would be somewhere else.
        let moved = hopper("Slower", "slower", false);
        assert!(matches!(
            reapply(&public, Some(&held), &moved),
            Reapply::Ask(m) if m.contains("different address")
        ));
        // Made public elsewhere already.
        let done = hopper("Slow", "slow", true);
        assert_eq!(reapply(&public, Some(&held), &done), Reapply::Done);
        // Making it private never publishes anything: always safe.
        let private = edit(|e| e.public = Some(false));
        let fresh = hopper("Slower", "slower", true);
        assert_eq!(
            reapply(&private, Some(&held), &fresh),
            Reapply::Retry(private.clone())
        );
    }

    #[test]
    fn only_409s_count_as_a_hopper_conflict() {
        assert!(is_hopper_conflict(&CoreError::RevisionConflict));
        assert!(is_hopper_conflict(&hopper_conflict_error("x".into())));
        assert!(!is_hopper_conflict(&CoreError::Offline));
        assert!(!is_hopper_conflict(&CoreError::Rejected {
            status: 400,
            message: "no".into(),
            details: vec![],
        }));
    }
}
