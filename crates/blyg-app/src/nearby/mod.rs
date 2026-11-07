//! --- nearby --- Nearby thoughts: a drawer at the window's right edge
//! (`side.rs`, beside the editor) that shows up to five of your own
//! published posts, and posts you hold from blygs you follow, that say the
//! same thing as the paragraph under the caret. ⌥↑/⌥↓ choose without
//! leaving the text, ⌥⏎ writes `[[id | phrase]]` over the phrase they share
//! (or `[[id | title]]` at the caret when they share meaning, not words),
//! ⌥⇧⏎ writes `![[id | title]]` on a new line after the paragraph (threads
//! only). It's closed until you open it: its tab in the side tab bar, or
//! ⌥⌘U, opens it; it stays open until its ×, its tab or ⌥⌘U closes it.
//! Its left edge drags (a double-click resets it). Whether it's open and
//! its width are kept in `state.json`. Nothing is matched while it's closed.
//!
//! What "the same thing" means is the `nearby-embedder` key:
//! - none (the default): shared phrases, through the trigram indexes.
//! - local: potion-base-8M, a static embedding model run here in pure Rust
//!   (`blyg-embed`), downloaded once when the user chooses it.
//! - endpoint: an Ollama or OpenAI-compatible server the user runs.
//!
//! and `nearby-rerank = on` lets the AI provider order the best ten.
//!
//! Nothing runs on the main thread but reading the editor: a keystroke
//! only re-arms a 300 ms timer; the paragraph's phrases, the store's
//! phrase search, the embedding and the ranking run in the background, and
//! so does the index (built once, then kept current as posts change, a
//! changed post's changed pieces only). Only what this Mac holds is
//! offered; nothing is fetched to suggest; withdrawn posts never appear.
//!
//! A child module of `app` (it reads `MainView`); `app.rs` keeps small
//! hooks marked `// --- nearby ---`.

pub mod engine;

use std::ops::Range;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use blyg_core::config::Change;
use blyg_embed::download::POTION_BASE_8M;
use blyg_embed::{Embedder, StaticModel};

use self::engine::{Ask, Corpus, Doc, Index, Progress, Suggestion};
use super::*;
use crate::composer::picker::{self, PickKind};

gpui_kit::actions!(
    blygger_nearby,
    [ToggleNearby, NearbyUp, NearbyDown, NearbyLink, NearbyQuote]
);

/// How long after the last keystroke the rail looks again.
pub const DEBOUNCE: Duration = Duration::from_millis(300);
/// How long after a store change the index catches up.
pub const REINDEX_DELAY: Duration = Duration::from_secs(2);
/// The rail's width until its edge is dragged.
pub const RAIL_W: f32 = 236.;
/// The rail's widths: its default and the range a drag keeps it in.
pub const SPAN: super::side::Span = super::side::Span {
    default: RAIL_W,
    min: 200.,
    max: 440.,
};
/// The rail never takes more than this share of the window.
pub const MAX_SHARE: f32 = 0.35;
/// How long after a failed index build the rail tries again.
pub const RETRY_INDEX: Duration = Duration::from_secs(30);

/// Tests (and only tests): an embedder instead of the configured one, and
/// where models are kept.
#[derive(Default)]
pub struct NearbyGlobal {
    pub embedder: Option<Arc<dyn Embedder>>,
    pub models_dir: Option<PathBuf>,
}

impl Global for NearbyGlobal {}

/// Where vectors come from, as configured.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    None,
    Local,
    Endpoint {
        url: String,
        model: String,
    },
    /// `NearbyGlobal::embedder`.
    Test,
}

/// What the rail's footer says about its matching.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum Status {
    /// Shared phrases only (no embedder chosen).
    #[default]
    Phrases,
    Downloading,
    Loading,
    Indexing,
    Ready,
    Failed(String),
}

#[derive(Default)]
struct Shared {
    /// Whose posts these are (the blyg's address): another blyg's are
    /// another corpus.
    scope: String,
    own: Option<(u64, Arc<Vec<Doc>>)>,
    held: Option<(u64, Arc<Vec<Doc>>)>,
    index: Option<Arc<Index>>,
}

/// The window's rail state.
#[derive(Default)]
pub struct State {
    /// The drawer is open (⌥⌘U, its tab; remembered in state.json).
    pub open: bool,
    /// The width the user chose (dragged).
    pub width: f32,
    /// The latest drag of its edge (the width is saved once it stops).
    width_gen: u64,
    /// The window's width this frame (`side.rs` fits the drawer to it).
    pub window_w: f32,
    /// The width it takes this frame (0 when it's not on screen).
    pub dock_w: f32,
    /// The caret when last looked at (the watch ignores a blink).
    last_cursor: Option<(usize, Option<String>)>,
    /// What the rail shows (at most `engine::SHOWN`).
    pub rows: Vec<Suggestion>,
    /// The ranked candidates before the cut (for a re-rank).
    pool: Vec<Suggestion>,
    pub sel: usize,
    /// The paragraph the rows are for.
    para: Option<Range<usize>>,
    ask_no: u64,
    timer: Option<Task<()>>,
    query: Option<Task<()>>,
    observe: Option<Subscription>,
    shared: Arc<Mutex<Shared>>,
    stamp_own: u64,
    stamp_held: u64,
    source: Option<Source>,
    embedder: Option<Arc<dyn Embedder>>,
    pub status: Status,
    progress: Arc<Progress>,
    download: Arc<(AtomicU64, AtomicU64)>,
    loading: Option<Task<()>>,
    indexing: Option<Task<()>>,
    reindex: Option<Task<()>>,
    index_dirty: bool,
    cancel: Arc<AtomicBool>,
    poll: Option<Task<()>>,
    reranked: Option<String>,
    rerank: Option<Task<()>>,
    corpus: Corpus,
    /// The last build's numbers (tests, the footer's tooltip).
    pub built: Option<engine::Built>,
    /// When the last index build failed (a retry waits [`RETRY_INDEX`]).
    index_failed_at: Option<Instant>,
    /// The row whose card shows (its id), and the one the mouse is resting
    /// on while the delay runs.
    pub hover: Option<String>,
    hover_wait: Option<(String, Task<()>)>,
}

fn lock(m: &Mutex<Shared>) -> std::sync::MutexGuard<'_, Shared> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

/// The corpus as of the stamps, rebuilt where it's stale. Off the main
/// thread: it reads every post.
fn corpus(
    backend: &dyn Backend,
    shared: &Mutex<Shared>,
    scope: &str,
    stamps: (u64, u64),
) -> (Corpus, bool) {
    let (own, held) = {
        let mut s = lock(shared);
        if s.scope != scope {
            *s = Shared {
                scope: scope.to_string(),
                ..Default::default()
            };
        }
        (
            s.own.clone().filter(|(st, _)| *st == stamps.0),
            s.held.clone().filter(|(st, _)| *st == stamps.1),
        )
    };
    let mut changed = false;
    let own = match own {
        Some((_, o)) => o,
        None => {
            changed = true;
            let o = Arc::new(engine::own_docs(&backend.items()));
            lock(shared).own = Some((stamps.0, o.clone()));
            o
        }
    };
    let held = match held {
        Some((_, h)) => h,
        None => {
            let h = Arc::new(engine::held_docs(
                &backend.reading(),
                &backend.subscriptions(),
            ));
            let mut s = lock(shared);
            let same = s.held.as_ref().is_some_and(|(_, old)| **old == *h);
            changed |= !same;
            s.held = Some((stamps.1, h.clone()));
            h
        }
    };
    (Corpus { own, held }, changed)
}

/// An embeddings endpoint as an `Embedder`.
struct Endpoint(blyg_ai::embeddings::EmbeddingsEndpoint);

impl Embedder for Endpoint {
    fn id(&self) -> String {
        format!("endpoint:{}#{}", self.0.url, self.0.model)
    }

    fn embed(&self, texts: &[&str]) -> blyg_embed::Result<Vec<Vec<f32>>> {
        let mut v = self.0.embed(texts).map_err(|e| match e {
            blyg_ai::AiError::Network(m) => blyg_embed::EmbedError::Network(m),
            other => blyg_embed::EmbedError::Provider(other.to_string()),
        })?;
        v.iter_mut().for_each(|x| blyg_embed::vector::normalize(x));
        Ok(v)
    }

    fn batch(&self) -> usize {
        32
    }
}

/// `nearby` as configured: `on`, `titles` or `off`.
fn mode(cx: &App) -> String {
    cx.try_global::<crate::settings::AppConfig>()
        .map(|a| a.store.config().nearby().to_string())
        .unwrap_or_else(|| "on".into())
}

fn source(cx: &App) -> Source {
    if cx
        .try_global::<NearbyGlobal>()
        .is_some_and(|g| g.embedder.is_some())
    {
        return Source::Test;
    }
    let Some(a) = cx.try_global::<crate::settings::AppConfig>() else {
        return Source::None;
    };
    let c = a.store.config();
    match c.nearby_embedder() {
        "local" => Source::Local,
        "endpoint" => match c.nearby_endpoint() {
            Some(url) => Source::Endpoint {
                url: url.to_string(),
                model: c.nearby_endpoint_model().to_string(),
            },
            None => Source::None,
        },
        _ => Source::None,
    }
}

fn rerank_on(cx: &App) -> bool {
    cx.try_global::<crate::settings::AppConfig>()
        .is_some_and(|a| a.store.config().nearby_rerank())
}

/// Where downloaded models live: the data dir's `models/`.
pub fn models_dir(cx: &App) -> PathBuf {
    cx.try_global::<NearbyGlobal>()
        .and_then(|g| g.models_dir.clone())
        .unwrap_or_else(|| blyg_core::config::data_dir().join("models"))
}

/// The similarity below which a post isn't offered, per kind of vector:
/// a static model's cosines run lower than a transformer's.
fn min_sim(s: &Source) -> f32 {
    match s {
        Source::Endpoint { .. } => 0.6,
        _ => 0.45,
    }
}

/// The byte just after the word at `at` (where a link written at the caret
/// goes).
fn word_end(text: &str, at: usize) -> usize {
    let at = at.min(text.len());
    text[at..]
        .char_indices()
        .find(|(_, c)| !c.is_alphanumeric())
        .map_or(text.len(), |(i, _)| at + i)
}

impl MainView {
    fn nearby_data_dir(&self, cx: &App) -> Option<PathBuf> {
        cx.try_global::<crate::connection::Connection>()
            .map(|c| c.data_dir.clone())
    }

    /// Hook (new): whether the drawer was left open and how wide, and a
    /// watch on the caret, so moving to another paragraph refreshes the
    /// rail as typing does.
    pub(super) fn nearby_init(&mut self, cx: &mut Context<Self>) {
        let state = self
            .nearby_data_dir(cx)
            .map(|d| blyg_core::state::AppState::load(&d))
            .unwrap_or_default();
        self.nearby.open = state.nearby_open;
        self.nearby.width = SPAN.restore(state.nearby_width);
        let editor = self.editor.clone();
        self.nearby.observe = Some(cx.observe(&editor, |this, _, cx| {
            let cur = this.editor.read(cx).cursor();
            let seen = (cur, this.current.as_ref().map(|c| c.local_id.0.clone()));
            if this.nearby.last_cursor.as_ref() == Some(&seen) {
                return;
            }
            this.nearby.last_cursor = Some(seen);
            if this.nearby.timer.is_some() || !this.nearby_shown(cx) {
                return;
            }
            let inside = this
                .nearby
                .para
                .as_ref()
                .is_some_and(|r| cur >= r.start && cur <= r.end);
            if !inside {
                this.nearby_typed(cx);
            }
        }));
        if self.nearby_shown(cx) {
            self.nearby_typed(cx);
        }
    }

    /// The rail is on screen: it's open and [`Self::nearby_available`].
    pub(crate) fn nearby_shown(&self, cx: &App) -> bool {
        self.nearby.open && self.nearby_available(cx)
    }

    /// The rail can show (its tab is in the tab bar): Posts, a post open,
    /// `nearby` not off.
    pub(crate) fn nearby_available(&self, cx: &App) -> bool {
        self.reading.view == reading::View::Posts && self.current.is_some() && mode(cx) != "off"
    }

    /// --- side --- The width the drawer takes this frame (0 when it's not
    /// on screen).
    pub(crate) fn nearby_dock_w(&self, cx: &App) -> f32 {
        if !self.nearby_shown(cx) {
            return 0.;
        }
        let window_w = self.nearby.window_w;
        SPAN.fit(self.nearby.width, window_w, MAX_SHARE)
    }

    /// --- side --- Set the drawer's width (saved once it stops changing).
    pub(crate) fn nearby_set_width(&mut self, w: f32, cx: &mut Context<Self>) {
        if (w - self.nearby.width).abs() < 0.5 {
            return;
        }
        self.nearby.width = w;
        self.nearby.width_gen += 1;
        let gen_ = self.nearby.width_gen;
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(super::side::SAVE_MS))
                .await;
            let _ = this.update(cx, |v, cx| {
                if v.nearby.width_gen == gen_ {
                    v.nearby_save_layout(cx);
                }
            });
        })
        .detach();
        cx.notify();
    }

    /// Persist whether the drawer is open, and its width.
    fn nearby_save_layout(&self, cx: &App) {
        if let Some(d) = self.nearby_data_dir(cx) {
            let (open, w) = (self.nearby.open, SPAN.saved(self.nearby.width));
            let _ = blyg_core::state::AppState::update(&d, |s| {
                s.nearby_open = open;
                s.nearby_width = w;
            });
        }
    }

    /// Hook (editor change): look again after a pause. One timer, re-armed
    /// by every keystroke; nothing else happens on the keystroke.
    pub(super) fn nearby_typed(&mut self, cx: &mut Context<Self>) {
        if !self.nearby.open {
            return;
        }
        self.nearby.timer = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(DEBOUNCE).await;
            let _ = this.update(cx, |v, cx| {
                v.nearby.timer = None;
                v.nearby_query(cx);
            });
        }));
    }

    /// Hook (store events): what the rail can offer changed.
    pub(super) fn nearby_store_changed(&mut self, ev: &CoreEvent, cx: &mut Context<Self>) {
        match ev {
            CoreEvent::ItemsChanged => self.nearby.stamp_own += 1,
            CoreEvent::ReadingChanged => self.nearby.stamp_held += 1,
            _ => return,
        }
        if self.nearby.embedder.is_none() || !self.nearby.open {
            return;
        }
        self.nearby.reindex = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(REINDEX_DELAY).await;
            let _ = this.update(cx, |v, cx| v.nearby_reindex(cx));
        }));
    }

    /// Hook (config reloaded or changed in Settings): the embedder may be
    /// another one now.
    pub(super) fn nearby_reload(&mut self, cx: &mut Context<Self>) {
        let src = source(cx);
        if self.nearby.source.as_ref() != Some(&src) {
            self.nearby_resolve(cx);
        }
        self.nearby.reranked = None;
        self.nearby_typed(cx);
    }

    fn scope(&self) -> String {
        self.base_url.clone().unwrap_or_default()
    }

    /// Find the embedder `nearby-embedder` names: nothing to do, the
    /// endpoint, or the local model (loaded off the main thread, after
    /// downloading it the first time).
    fn nearby_resolve(&mut self, cx: &mut Context<Self>) {
        let src = source(cx);
        self.nearby.source = Some(src.clone());
        self.nearby.embedder = None;
        self.nearby.loading = None;
        self.nearby.indexing = None;
        self.nearby.index_failed_at = None;
        self.nearby.cancel.store(true, Ordering::Relaxed);
        self.nearby.cancel = Arc::new(AtomicBool::new(false));
        lock(&self.nearby.shared).index = None;
        match src {
            Source::None => self.nearby.status = Status::Phrases,
            Source::Test => {
                self.nearby.embedder = cx
                    .try_global::<NearbyGlobal>()
                    .and_then(|g| g.embedder.clone());
                self.nearby_reindex(cx);
            }
            Source::Endpoint { url, model } => {
                self.nearby.embedder = Some(Arc::new(Endpoint(
                    blyg_ai::embeddings::EmbeddingsEndpoint::new(&url, &model),
                )));
                self.nearby_reindex(cx);
            }
            Source::Local => {
                let dir = models_dir(cx);
                if POTION_BASE_8M.installed(&dir) {
                    self.nearby_load_model(dir, cx);
                } else {
                    self.nearby_download(dir, cx);
                }
            }
        }
        cx.notify();
    }

    fn nearby_load_model(&mut self, models: PathBuf, cx: &mut Context<Self>) {
        self.nearby.status = Status::Loading;
        let task = cx.background_spawn(async move {
            StaticModel::load(POTION_BASE_8M.id, &POTION_BASE_8M.dir(&models))
        });
        self.nearby.loading = Some(cx.spawn(async move |this, cx| {
            let r = task.await;
            let _ = this.update(cx, |v, cx| {
                v.nearby.loading = None;
                match r {
                    Ok(m) => {
                        v.nearby.embedder = Some(Arc::new(m));
                        v.nearby_reindex(cx);
                    }
                    Err(e) => v.nearby.status = Status::Failed(e.to_string()),
                }
                cx.notify();
            });
        }));
    }

    /// Fetch the local model (the user chose it): pinned files, checked,
    /// with the progress in the rail's footer and in Settings.
    fn nearby_download(&mut self, models: PathBuf, cx: &mut Context<Self>) {
        if cfg!(test)
            && cx
                .try_global::<NearbyGlobal>()
                .is_none_or(|g| g.models_dir.is_none())
        {
            // Never from a test.
            self.nearby.status = Status::Failed("no downloads in tests".into());
            return;
        }
        self.nearby.status = Status::Downloading;
        let dl = self.nearby.download.clone();
        let cancel = self.nearby.cancel.clone();
        let m2 = models.clone();
        let task = cx.background_spawn(async move {
            blyg_embed::download::download(
                &POTION_BASE_8M,
                &m2,
                &blyg_embed::download::Http,
                &mut |d, t| {
                    dl.0.store(d, Ordering::Relaxed);
                    dl.1.store(t, Ordering::Relaxed);
                },
                &cancel,
            )
        });
        self.nearby_poll(cx);
        self.nearby.loading = Some(cx.spawn(async move |this, cx| {
            let r = task.await;
            let _ = this.update(cx, |v, cx| {
                v.nearby.loading = None;
                match r {
                    Ok(()) => v.nearby_load_model(models, cx),
                    Err(e) => {
                        v.nearby.status = Status::Failed(e.to_string());
                        v.show_toast(
                            "The Nearby model didn't download",
                            Some(e.to_string().into()),
                            cx,
                        );
                    }
                }
                cx.notify();
            });
        }));
    }

    /// Repaint while something reports progress.
    fn nearby_poll(&mut self, cx: &mut Context<Self>) {
        if self.nearby.poll.is_some() {
            return;
        }
        self.nearby.poll = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(250))
                    .await;
                let busy = this
                    .update(cx, |v, cx| {
                        cx.notify();
                        matches!(v.nearby.status, Status::Downloading | Status::Indexing)
                    })
                    .unwrap_or(false);
                if !busy {
                    break;
                }
            }
            let _ = this.update(cx, |v, _| v.nearby.poll = None);
        }));
    }

    /// Bring the index up to date (in the background; a second call while
    /// one runs makes it go again after).
    fn nearby_reindex(&mut self, cx: &mut Context<Self>) {
        let Some(embedder) = self.nearby.embedder.clone() else {
            return;
        };
        if self.nearby.indexing.is_some() {
            self.nearby.index_dirty = true;
            return;
        }
        self.nearby.index_dirty = false;
        if lock(&self.nearby.shared).index.is_none() {
            self.nearby.status = Status::Indexing;
        }
        let backend = self.backend.clone();
        let shared = self.nearby.shared.clone();
        let progress = self.nearby.progress.clone();
        let cancel = self.nearby.cancel.clone();
        let scope = self.scope();
        let stamps = (self.nearby.stamp_own, self.nearby.stamp_held);
        let task = cx.background_spawn(async move {
            let (c, _) = corpus(&*backend, &shared, &scope, stamps);
            let prev = lock(&shared).index.clone();
            let r = engine::build_index(
                &*backend,
                &*embedder,
                &c,
                prev.as_deref(),
                &progress,
                &|| cancel.load(Ordering::Relaxed),
            );
            if let Ok((idx, _)) = &r
                && !cancel.load(Ordering::Relaxed)
            {
                lock(&shared).index = Some(Arc::new(idx.clone()));
            }
            r.map(|(_, b)| b)
        });
        self.nearby_poll(cx);
        self.nearby.indexing = Some(cx.spawn(async move |this, cx| {
            let r = task.await;
            let _ = this.update(cx, |v, cx| {
                v.nearby.indexing = None;
                match r {
                    Ok(b) => {
                        v.nearby.status = Status::Ready;
                        v.nearby.built = Some(b);
                        v.nearby.index_failed_at = None;
                    }
                    Err(blyg_embed::EmbedError::Model(m)) if m == "cancelled" => {}
                    Err(e) => {
                        v.nearby.status = Status::Failed(e.to_string());
                        v.nearby.index_failed_at = Some(cx.background_executor().now());
                    }
                }
                if v.nearby.index_dirty {
                    v.nearby_reindex(cx);
                }
                v.nearby_query(cx);
                cx.notify();
            });
        }));
    }

    /// Look now: the paragraph under the caret, ranked off the main thread.
    pub(crate) fn nearby_query(&mut self, cx: &mut Context<Self>) {
        if !self.nearby_shown(cx) {
            if !self.nearby.rows.is_empty() {
                self.nearby.rows.clear();
                cx.notify();
            }
            return;
        }
        if self.nearby.source.is_none() {
            self.nearby_resolve(cx);
        }
        let (text, cursor) = {
            let s = self.editor.read(cx);
            (s.value().to_string(), s.cursor())
        };
        let para = engine::paragraph_at(&text, cursor);
        let plain = para
            .as_ref()
            .map(|p| engine::paragraph_plain(&text, p))
            .unwrap_or_default();
        let letters = plain.chars().filter(|c| c.is_alphanumeric()).count();
        let Some(para) = para.filter(|_| letters >= engine::MIN_PARAGRAPH) else {
            self.nearby.para = None;
            self.nearby.ask_no += 1;
            if !self.nearby.rows.is_empty() {
                self.nearby.rows.clear();
                self.nearby.pool.clear();
                cx.notify();
            }
            return;
        };
        let ask = Ask {
            linked: engine::linked_ids(&text),
            text,
            para: para.clone(),
            exclude: self
                .current
                .as_ref()
                .map(|c| blyg_core::nearby::own_key(&c.local_id.0)),
            titles_only: mode(cx) == "titles",
        };
        self.nearby.ask_no += 1;
        let ask_no = self.nearby.ask_no;
        let backend = self.backend.clone();
        let shared = self.nearby.shared.clone();
        let scope = self.scope();
        let stamps = (self.nearby.stamp_own, self.nearby.stamp_held);
        let embedder = self.nearby.embedder.clone();
        let floor = min_sim(self.nearby.source.as_ref().unwrap_or(&Source::None));
        let plain2 = plain.clone();
        let task = cx.background_spawn(async move {
            let (c, changed) = corpus(&*backend, &shared, &scope, stamps);
            let phrases = engine::phrases(&ask.text, &ask.para);
            let needles: Vec<String> = phrases.iter().map(|p| p.needle.clone()).collect();
            let hits = backend.nearby_phrase_hits(&needles);
            let index = lock(&shared).index.clone();
            let mut failed = None;
            let q = match (&embedder, &index) {
                (Some(e), Some(_)) => match e.embed(&[plain2.as_str()]) {
                    Ok(mut v) => v.pop(),
                    Err(err) => {
                        failed = Some(err.to_string());
                        None
                    }
                },
                _ => None,
            };
            let rows = engine::rank(
                &ask,
                &c,
                index.as_deref().zip(q.as_deref()),
                &hits,
                &phrases,
                floor,
            );
            (rows, changed, c, failed)
        });
        self.nearby.query = Some(cx.spawn(async move |this, cx| {
            let (rows, changed, c, failed) = task.await;
            let _ = this.update(cx, |v, cx| {
                if v.nearby.ask_no != ask_no {
                    return;
                }
                v.nearby.para = Some(para.clone());
                v.nearby.corpus = c;
                let indexed = lock(&v.nearby.shared).index.is_some();
                if let Some(f) = failed {
                    v.nearby.status = Status::Failed(f);
                } else if matches!(v.nearby.status, Status::Failed(_))
                    && indexed
                    && v.nearby.embedder.is_some()
                    && v.nearby.indexing.is_none()
                    && v.nearby.loading.is_none()
                {
                    // The endpoint answers again (and there's an index to
                    // ask: without one, nothing is matched by meaning).
                    v.nearby.status = Status::Ready;
                }
                v.nearby_set_rows(rows, cx);
                // An index build that failed (the endpoint was down, say)
                // is tried again, at most every RETRY_INDEX, rather than
                // leaving the rail on shared phrases for the session.
                let retry = !indexed
                    && v.nearby.indexing.is_none()
                    && v.nearby.loading.is_none()
                    && v.nearby.index_failed_at.is_some_and(|at| {
                        cx.background_executor().now().duration_since(at) >= RETRY_INDEX
                    });
                if (changed || retry) && v.nearby.embedder.is_some() {
                    v.nearby_reindex(cx);
                }
                if rerank_on(cx) {
                    v.nearby_rerank(plain.clone(), para.start, ask_no, cx);
                }
            });
        }));
    }

    fn nearby_set_rows(&mut self, pool: Vec<Suggestion>, cx: &mut Context<Self>) {
        let was = self.nearby.rows.get(self.nearby.sel).map(|s| s.id.clone());
        self.nearby.pool = pool.clone();
        self.nearby.rows = pool.into_iter().take(engine::SHOWN).collect();
        self.nearby.sel = was
            .and_then(|id| self.nearby.rows.iter().position(|s| s.id == id))
            .unwrap_or(0);
        self.nearby_hover_keep();
        cx.notify();
    }

    // ------------------------------------------------------------ hover

    /// The mouse entered (or left) row `i`: after a moment its card shows
    /// the post (title, who, its first lines), as a link's hover card does
    /// in the editor.
    fn nearby_hover(&mut self, id: String, hovered: bool, cx: &mut Context<Self>) {
        if !hovered {
            if self
                .nearby
                .hover_wait
                .as_ref()
                .is_some_and(|(w, _)| *w == id)
            {
                self.nearby.hover_wait = None;
            }
            if self.nearby.hover.as_ref() == Some(&id) {
                self.nearby.hover = None;
                cx.notify();
            }
            return;
        }
        if self.nearby.hover.as_ref() == Some(&id)
            || self
                .nearby
                .hover_wait
                .as_ref()
                .is_some_and(|(w, _)| *w == id)
        {
            return;
        }
        if self.nearby.hover.take().is_some() {
            cx.notify();
        }
        let shown = id.clone();
        let task = cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(super::hover_card::DELAY)
                .await;
            let _ = this.update(cx, |v, cx| {
                v.nearby.hover_wait = None;
                if v.nearby.rows.iter().any(|s| s.id == shown) {
                    v.nearby.hover = Some(shown);
                    cx.notify();
                }
            });
        });
        self.nearby.hover_wait = Some((id, task));
    }

    /// A card for a row that left the rail goes with it.
    fn nearby_hover_keep(&mut self) {
        let rows = &self.nearby.rows;
        if self
            .nearby
            .hover
            .as_ref()
            .is_some_and(|h| !rows.iter().any(|s| s.id == *h))
        {
            self.nearby.hover = None;
        }
        if self
            .nearby
            .hover_wait
            .as_ref()
            .is_some_and(|(h, _)| !rows.iter().any(|s| s.id == *h))
        {
            self.nearby.hover_wait = None;
        }
    }

    /// The card for a row: the post as this Mac holds it.
    pub(crate) fn nearby_card(&self, s: &Suggestion) -> super::hover_card::Card {
        use super::hover_card::{Card, Place, clip_excerpt, kind_word};
        let doc = self.nearby.corpus.docs().find(|d| d.key == s.key);
        let text = doc.map(|d| d.text.as_str()).unwrap_or_default();
        Card {
            id: s.id.clone(),
            title: s.title.clone(),
            meta: format!("{} · {}", s.who, kind_word(s.kind)),
            excerpt: clip_excerpt(text),
            note: None,
            place: match s.key.strip_prefix("own:") {
                Some(local) => Place::Own(LocalId(local.to_string())),
                None => Place::Held {
                    origin: String::new(),
                },
            },
            label_it: None,
            quote: false,
        }
    }

    /// The hovered row's card, left of the rail (it sits at the window's
    /// right edge).
    fn render_nearby_card(&self, s: &Suggestion) -> Option<AnyElement> {
        if self.nearby.hover.as_ref() != Some(&s.id) {
            return None;
        }
        let p = self.palette;
        let font = SharedString::from(self.theme.chrome_font(&self.prefs));
        let card = self.nearby_card(s);
        let body = super::hover_card::card_body(&card, p, font)
            .id("nearby-card")
            .debug_selector(|| "nearby-card".into())
            .occlude()
            .child(
                div()
                    .mt(px(2.))
                    .text_size(px(10.5))
                    .text_color(p.muted)
                    .child(crate::keymap::hint("⌥⏎ links it · ⌥⇧⏎ quotes it")),
            );
        Some(
            div()
                .absolute()
                .top_0()
                .left_0()
                .child(
                    deferred(
                        anchored()
                            .anchor(Anchor::TopRight)
                            .offset(point(px(-12.), px(-5.)))
                            .snap_to_window_with_margin(px(8.))
                            .child(body),
                    )
                    .with_priority(1),
                )
                .into_any_element(),
        )
    }

    /// `nearby-rerank = on`: the AI provider orders the candidates, once
    /// per set of candidates for a paragraph. Its answer replaces the rows
    /// only if nothing newer was asked meanwhile.
    fn nearby_rerank(&mut self, paragraph: String, at: usize, ask_no: u64, cx: &mut Context<Self>) {
        let pool = self.nearby.pool.clone();
        if pool.len() < 2 {
            return;
        }
        let key = format!(
            "{at}:{}",
            pool.iter()
                .map(|s| s.id.as_str())
                .collect::<Vec<_>>()
                .join(",")
        );
        if self.nearby.reranked.as_deref() == Some(key.as_str()) {
            return;
        }
        self.nearby.reranked = Some(key);
        let Ok(picked) = crate::ai::pick(cx) else {
            return;
        };
        let (system, user) = engine::rerank_prompt(&paragraph, &pool, &self.nearby.corpus);
        let task = cx.background_spawn(async move {
            let req = blyg_ai::GenRequest {
                max_tokens: Some(60),
                ..blyg_ai::GenRequest::new(system, user)
            };
            picked.provider.generate(req, &mut |_| {})
        });
        self.nearby.rerank = Some(cx.spawn(async move |this, cx| {
            let r = task.await;
            let _ = this.update(cx, |v, cx| {
                if v.nearby.ask_no != ask_no {
                    return;
                }
                if let Ok(out) = r {
                    let ordered = engine::apply_rerank(&pool, &out.text);
                    v.nearby.rows = ordered.into_iter().take(engine::SHOWN).collect();
                    v.nearby.sel = 0;
                    cx.notify();
                }
            });
        }));
    }

    // ------------------------------------------------------------ actions

    pub(super) fn nearby_actions(&self, d: Stateful<Div>, cx: &mut Context<Self>) -> Stateful<Div> {
        d.on_action(cx.listener(|this, _: &ToggleNearby, _, cx| this.nearby_toggle(cx)))
            .on_action(cx.listener(|this, _: &NearbyUp, _, cx| this.nearby_move(-1, cx)))
            .on_action(cx.listener(|this, _: &NearbyDown, _, cx| this.nearby_move(1, cx)))
            .on_action(cx.listener(|this, _: &NearbyLink, window, cx| this.nearby_link(window, cx)))
            .on_action(
                cx.listener(|this, _: &NearbyQuote, window, cx| this.nearby_quote(window, cx)),
            )
    }

    /// ⌥⌘U (or its tab): open or close the drawer.
    pub(crate) fn nearby_toggle(&mut self, cx: &mut Context<Self>) {
        if mode(cx) == "off" {
            self.show_toast(
                "Nearby thoughts is off",
                Some("Settings › Nearby turns it on (nearby = on)".into()),
                cx,
            );
            return;
        }
        self.nearby_set_open(!self.nearby.open, cx);
    }

    /// Open or close the drawer (and remember it).
    pub(crate) fn nearby_set_open(&mut self, open: bool, cx: &mut Context<Self>) {
        self.nearby.open = open;
        self.nearby.hover = None;
        self.nearby_save_layout(cx);
        if open {
            self.nearby_query(cx);
        }
        cx.notify();
    }

    /// Whether ⌥↑/⌥↓/⌥⏎ act on the rail: it's showing rows and the editor
    /// has the keyboard.
    fn nearby_keys_live(&self, window: &Window, cx: &mut Context<Self>) -> bool {
        self.nearby_shown(cx)
            && !self.nearby.rows.is_empty()
            && self.editor.read(cx).focus_handle(cx).is_focused(window)
    }

    fn nearby_move(&mut self, delta: isize, cx: &mut Context<Self>) {
        let n = self.nearby.rows.len();
        if n == 0 || !self.nearby.open {
            cx.propagate();
            return;
        }
        self.nearby.sel = (self.nearby.sel as isize + delta).rem_euclid(n as isize) as usize;
        cx.notify();
    }

    /// ⌥⏎: link the chosen post over the phrase it shares, or at the caret.
    pub(crate) fn nearby_link(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.nearby_keys_live(window, cx) {
            cx.propagate();
            return;
        }
        let Some(s) = self.nearby.rows.get(self.nearby.sel).cloned() else {
            return;
        };
        let (text, cursor) = {
            let e = self.editor.read(cx);
            (e.value().to_string(), e.cursor())
        };
        let below = self.backend.server_outdated();
        // The phrase, if it's still there as it was.
        let phrase = s.phrase.as_ref().filter(|p| {
            text.get(p.range.clone()).is_some_and(|t| {
                t.split_whitespace()
                    .map(|w| w.to_lowercase().replace('’', "'"))
                    .collect::<Vec<_>>()
                    .join(" ")
                    == p.needle
            })
        });
        let (new_text, caret) = match phrase {
            Some(p) => {
                let shown = text[p.range.clone()].to_string();
                picker::insert(
                    &text,
                    p.range.start,
                    p.range.end,
                    PickKind::Link,
                    &s.id,
                    blyg_core::floor::picker_label(below, &shown),
                )
            }
            None => {
                let at = word_end(&text, cursor);
                let link = blyg_core::grammar::link_source(
                    &s.id,
                    blyg_core::floor::picker_label(below, &s.label),
                );
                let space = text[..at]
                    .chars()
                    .next_back()
                    .is_some_and(|c| !c.is_whitespace());
                let ins = if space { format!(" {link}") } else { link };
                let mut out = text.clone();
                out.insert_str(at, &ins);
                (out, at + ins.len())
            }
        };
        self.splice_editor(&text, &new_text, Some(caret), window, cx);
        self.nearby_written(&s, "Linked", cx);
    }

    /// ⌥⇧⏎: quote the chosen post on a new line after the paragraph.
    pub(crate) fn nearby_quote(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.nearby_keys_live(window, cx) {
            cx.propagate();
            return;
        }
        let Some(s) = self.nearby.rows.get(self.nearby.sel).cloned() else {
            return;
        };
        if !reading::vm::can_quote_into(self.current.as_ref()) {
            self.show_toast(
                "Quotes go in threads",
                Some(crate::keymap::hint("⌘T makes this a thread").into()),
                cx,
            );
            return;
        }
        let (text, cursor) = {
            let e = self.editor.read(cx);
            (e.value().to_string(), e.cursor())
        };
        let at = engine::paragraph_at(&text, cursor)
            .map(|r| r.end)
            .unwrap_or(cursor);
        let below = self.backend.server_outdated();
        let directive = blyg_core::grammar::directive_source(
            &s.id,
            blyg_core::floor::picker_label(below, &s.label),
        );
        let (new_text, caret) = reading::vm::insert_directive(&text, at, &directive);
        self.splice_editor(&text, &new_text, Some(caret), window, cx);
        self.nearby_written(&s, "Quoted", cx);
    }

    /// After a link or quote: the post leaves the rail (it's linked now),
    /// and a toast says what was written.
    fn nearby_written(&mut self, s: &Suggestion, verb: &str, cx: &mut Context<Self>) {
        self.nearby.rows.retain(|r| r.id != s.id);
        self.nearby.pool.retain(|r| r.id != s.id);
        self.nearby.sel = self
            .nearby
            .sel
            .min(self.nearby.rows.len().saturating_sub(1));
        self.nearby_hover_keep();
        let from = if s.own {
            "yours".to_string()
        } else {
            s.who.clone()
        };
        self.show_toast(format!("{verb} “{}”", s.title), Some(from.into()), cx);
        cx.notify();
    }

    // ------------------------------------------------------------ settings

    /// Settings › Nearby: the mode, the matcher, the re-rank.
    fn set_nearby_setting(&mut self, key: &'static str, value: &str, cx: &mut Context<Self>) {
        if let Err(e) = crate::settings::write(&[(key, Change::Set(value.to_string()))], cx) {
            self.show_toast(e, None, cx);
            return;
        }
        self.watch_stamp = crate::settings::watch_stamp(&crate::settings::watched(cx));
        if key == "nearby-embedder" {
            // Chosen again: try again (a failed download, say).
            self.nearby.source = None;
        }
        self.nearby_reload(cx);
        cx.notify();
    }

    /// Words for the footer and Settings.
    pub(crate) fn nearby_status_words(&self) -> String {
        let pct = |d: u64, t: u64| (d * 100).checked_div(t).unwrap_or(0);
        if self.nearby.source.is_none() {
            // Nothing is loaded until the rail is first asked (a post open
            // in Posts): say so, rather than "shared phrases" for a model.
            return "Starts when you write in a post".into();
        }
        match &self.nearby.status {
            Status::Phrases => "Matching shared phrases".into(),
            Status::Downloading => {
                let (d, t) = (
                    self.nearby.download.0.load(Ordering::Relaxed),
                    self.nearby.download.1.load(Ordering::Relaxed),
                );
                format!("Downloading the model… {}%", pct(d, t))
            }
            Status::Loading => "Loading the model…".into(),
            Status::Indexing => {
                let (d, t) = (
                    self.nearby.progress.done.load(Ordering::Relaxed),
                    self.nearby.progress.total.load(Ordering::Relaxed),
                );
                format!("Reading your posts… {}%", pct(d, t))
            }
            Status::Ready => match self.nearby.source.as_ref() {
                Some(Source::Local) => format!("By meaning · {}", POTION_BASE_8M.name),
                Some(Source::Endpoint { model, .. }) => format!("By meaning · {model}"),
                _ => "By meaning".into(),
            },
            Status::Failed(e) => format!("Shared phrases only: {e}"),
        }
    }

    pub(crate) fn render_nearby_setting(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let theme = &*self.theme;
        let (m, emb, rerank) = match cx.try_global::<crate::settings::AppConfig>() {
            Some(a) => {
                let c = a.store.config();
                (
                    c.nearby().to_string(),
                    c.nearby_embedder().to_string(),
                    c.nearby_rerank(),
                )
            }
            None => ("on".into(), "none".into(), false),
        };
        let chip = |id: &'static str, label: String, on: bool| {
            div()
                .id(id)
                .debug_selector(move || id.into())
                .px(px(10.))
                .py(px(3.))
                .map(|d| crate::theme_ext::chip(d, theme, on))
                .cursor_pointer()
                .hover(|s| s.border_color(p.accent).bg(p.hover()))
                .child(label)
        };
        let set = |key: &'static str, v: &'static str| {
            cx.listener(move |this: &mut Self, _: &ClickEvent, _, cx| {
                this.set_nearby_setting(key, v, cx)
            })
        };
        let size_mb = POTION_BASE_8M.total_size() as f64 / 1_000_000.0;
        let installed = POTION_BASE_8M.installed(&models_dir(cx));
        let local_label = if installed {
            "This Mac's model".to_string()
        } else {
            format!("Download a model ({size_mb:.0} MB)")
        };
        let note = match emb.as_str() {
            "local" => format!(
                "{} ({}, {}). Runs on this Mac; nothing leaves it. {}",
                POTION_BASE_8M.name,
                POTION_BASE_8M.licence,
                POTION_BASE_8M.source.trim_start_matches("https://"),
                self.nearby_status_words()
            ),
            "endpoint" => format!(
                "Paragraphs and your posts go to nearby-endpoint (config file). {}",
                self.nearby_status_words()
            ),
            _ => "Shared phrases, from the search index. A model on this Mac also finds \
                  posts that say the same thing in other words."
                .to_string(),
        };
        div()
            .flex()
            .gap(px(12.))
            .py(px(7.))
            .child(
                div()
                    .w(px(92.))
                    .flex_none()
                    .pt(px(4.))
                    .text_size(px(10.5))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(p.muted)
                    .child("NEARBY"),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap(px(6.))
                            .child(
                                chip("nearby-on", "Whole posts".into(), m == "on")
                                    .on_click(set("nearby", "on")),
                            )
                            .child(
                                chip("nearby-titles", "Titles only".into(), m == "titles")
                                    .on_click(set("nearby", "titles")),
                            )
                            .child(
                                chip("nearby-off", "Off".into(), m == "off")
                                    .on_click(set("nearby", "off")),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap(px(6.))
                            .child(
                                chip("nearby-phrases", "Shared phrases".into(), emb == "none")
                                    .on_click(set("nearby-embedder", "none")),
                            )
                            .child(
                                chip("nearby-local", local_label, emb == "local")
                                    .on_click(set("nearby-embedder", "local")),
                            )
                            .when(emb == "endpoint", |d| {
                                d.child(chip("nearby-endpoint", "Endpoint".into(), true))
                            })
                            .child(
                                chip("nearby-rerank", "Re-rank with AI".into(), rerank).on_click(
                                    cx.listener(move |this, _, _, cx| {
                                        this.set_nearby_setting(
                                            "nearby-rerank",
                                            if rerank { "off" } else { "on" },
                                            cx,
                                        )
                                    }),
                                ),
                            ),
                    )
                    .child(div().text_size(px(11.)).text_color(p.muted).child(note)),
            )
            .into_any_element()
    }

    // ------------------------------------------------------------ render

    /// Hook (`side.rs`): the drawer, docked right of the editor.
    pub(super) fn render_nearby_rail(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.nearby_shown(cx) {
            return None;
        }
        let p = self.palette;
        let chrome = self.theme.chrome_font(&self.prefs);
        let sel = self.nearby.sel;
        let semantic = self.nearby.embedder.is_some();
        let rows = self.nearby.rows.iter().enumerate().map(|(i, s)| {
            let on = i == sel;
            let sub = format!("{} · {}", s.who, vm::kind_label(s.kind));
            div()
                .id(("nearby-row", i))
                .debug_selector(move || format!("nearby-row-{i}"))
                .flex()
                .flex_col()
                .gap(px(1.))
                .px(px(8.))
                .py(px(5.))
                .rounded(px(5.))
                .cursor_pointer()
                .relative()
                .when(on, |d| d.bg(p.sel))
                .hover(|d| d.bg(p.sel.opacity(0.6)))
                .on_hover({
                    let id = s.id.clone();
                    cx.listener(move |this, hovered: &bool, _, cx| {
                        this.nearby_hover(id.clone(), *hovered, cx)
                    })
                })
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.nearby.sel = i;
                    // The keys act from the editor: keep it (or put it back)
                    // in charge of the keyboard.
                    this.editor.update(cx, |e, cx| e.focus(window, cx));
                    cx.notify();
                }))
                .children(self.render_nearby_card(s))
                .child(
                    div()
                        .truncate()
                        .text_color(p.ink)
                        .font_weight(FontWeight::MEDIUM)
                        .child(s.title.clone()),
                )
                .child(div().truncate().text_size(px(10.5)).child(sub))
                .child(
                    div()
                        .text_size(px(10.5))
                        .line_height(px(14.))
                        .max_h(px(28.))
                        .overflow_hidden()
                        .when(!s.semantic, |d| d.text_color(p.accent))
                        .child(s.why.clone()),
                )
        });
        let nothing_held = self.nearby.para.is_some()
            && self.nearby.corpus.own.is_empty()
            && self.nearby.corpus.held.is_empty();
        let empty = self.nearby.rows.is_empty().then(|| {
            div()
                .px(px(8.))
                .py(px(6.))
                .text_size(px(11.))
                .child(if nothing_held {
                    "Nothing to match yet: your published posts, and posts from blygs you \
                     follow, show up here."
                } else if self.nearby.para.is_none() {
                    "Nothing yet. Write a few words; your posts that say the same thing show up here."
                } else if semantic {
                    "Nothing nearby for this paragraph."
                } else {
                    "Nothing yet. Keep writing; matches appear as phrases recur."
                })
        });
        Some(
            div()
                .id("nearby-rail")
                .debug_selector(|| "nearby-rail".into())
                .relative()
                .flex_none()
                .w(px(self.nearby_dock_w(cx)))
                .h_full()
                .flex()
                .flex_col()
                .bg(p.editor)
                .border_l_1()
                .border_color(p.line)
                // A click on the rail leaves the keyboard with the editor
                // (the window's own focus would take it, and ⌥↑/⌥↓/⌥⏎/⌥⇧⏎
                // act only while the editor has it).
                .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
                .font_family(chrome)
                .text_size(px(11.5))
                .line_height(relative(1.3))
                .text_color(p.muted)
                .child(
                    div()
                        .flex_none()
                        .px(px(10.))
                        .pt(px(8.))
                        .pb(px(4.))
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .child(
                                    div()
                                        .flex_1()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_size(px(10.5))
                                        .child("NEARBY"),
                                )
                                .child(
                                    div()
                                        .id("nearby-close")
                                        .debug_selector(|| "nearby-close".into())
                                        .px(px(4.))
                                        .cursor_pointer()
                                        .hover(|h| h.text_color(p.ink))
                                        .child("×")
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.nearby_set_open(false, cx)
                                        })),
                                ),
                        )
                        .child(
                            div()
                                .text_size(px(10.))
                                .child(crate::keymap::hint("⌥↑↓ pick · ⌥⏎ link · ⌥⇧⏎ quote")),
                        ),
                )
                .child(
                    div()
                        .id("nearby-rows")
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scroll()
                        .px(px(4.))
                        .flex()
                        .flex_col()
                        .gap(px(2.))
                        .children(rows)
                        .children(empty),
                )
                .child(
                    div()
                        .flex_none()
                        .px(px(10.))
                        .py(px(6.))
                        .border_t_1()
                        .border_color(p.line)
                        .text_size(px(10.))
                        .truncate()
                        .debug_selector(|| "nearby-status".into())
                        .child(self.nearby_status_words()),
                )
                // --- side --- its left edge drags; a double-click resets it.
                .child(div().absolute().top_0().bottom_0().left(px(-2.)).child(
                    super::side::edge_handle(
                        "nearby-edge",
                        super::side::Which::Nearby,
                        &p,
                        cx.listener(|this, _, _, cx| this.nearby_set_width(RAIL_W, cx)),
                    ),
                ))
                .into_any_element(),
        )
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
