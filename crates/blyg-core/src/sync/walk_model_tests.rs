//! Model-based property tests of the reading list's reads (sync/walk.rs).
//!
//! A model of the studio's `GET /api/reading/imported` (rows newest
//! `observedAt` first, keyset pages whose cursor expires when the list
//! changes, a `reading` counter, an epoch) and of the client's held rows,
//! head mark, walk and timers. The client side drives the real
//! [`Read`] step machine and [`start`] exactly as `Engine::pull_locked` and
//! `Engine::read_reading` do; only fetch, merge, prune and accept are
//! modelled. Random histories mix pulls (foreground and background, some
//! with the list changing part-way through) with new head posts, rows
//! written late below the head mark, edits that move `observedAt`, edits
//! and read marks that keep it, deletions, unsubscribes, counter noise,
//! this Mac's own read marks and restored databases (a new epoch).
//!
//! Properties, checked on every pull of every history:
//! - (f) a pull makes at most `reading_pages` page requests;
//! - (c) rows are removed only by a whole walk that reached the end, and
//!   only rows the server no longer has;
//! - (b) after a whole walk completes, the rows held are the server's;
//! - (e) a post new at the head is merged by the first or second request
//!   of the next foreground pull.
//!
//! and once the server goes quiet:
//! - (a) the client accepts the server's `reading` revision and holds
//!   exactly its rows within a bounded time (the safety net's longest
//!   interval plus a walk);
//! - (d) after that, a pull reads no pages unless the safety net is due or
//!   its walk is under way, and the safety net backs off.
//!
//! `cargo test -p blyg-core model_` runs 256 histories; the ignored
//! `model_long_histories` runs many more, longer ones:
//! `cargo test -p blyg-core model_long -- --ignored`.

use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use proptest::prelude::*;

use super::*;
use crate::api::wire::ChangeState;
use crate::sync::{SyncOptions, next_full_every};

type Key = (String, String);

const SEC: i64 = 1000;
const PULL_INTERVAL: i64 = 60 * SEC;
const GAP: i64 = 60 * SEC;
const EVERY: i64 = 600 * SEC;
const MAX: i64 = 7200 * SEC;

fn pause() -> i64 {
    WALK_PAUSE.as_millis() as i64
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Row {
    stamp: i64,
    ver: u32,
}

/// The studio's reading list.
#[derive(Debug)]
struct Server {
    rows: BTreeMap<Key, Row>,
    epoch: u32,
    /// The `reading` counter.
    rev: u64,
    /// Moves whenever the list's membership or order changes: a cursor
    /// from an older generation is 409 `cursor-expired`. A change that
    /// keeps a row's stamp (an edit or read mark) doesn't move it.
    generation: u64,
    /// The subscription list (only real changes: one removed).
    subs: u64,
    page_size: usize,
    next_id: u32,
}

impl Server {
    /// Newest `observedAt` first; ties by key, as a keyset would.
    fn ordered(&self) -> Vec<(Key, Row)> {
        let mut v: Vec<_> = self.rows.iter().map(|(k, r)| (k.clone(), *r)).collect();
        v.sort_by(|a, b| b.1.stamp.cmp(&a.1.stamp).then(b.0.cmp(&a.0)));
        v
    }

    #[allow(clippy::type_complexity)]
    fn page(&self, cursor: Option<&str>) -> Result<(Vec<(Key, Row)>, Option<String>), ()> {
        let from = match cursor {
            None => 0,
            Some(c) => {
                let (g, at) = c.split_once(':').expect("our cursor");
                if g.parse::<u64>().unwrap() != self.generation {
                    return Err(());
                }
                at.parse::<usize>().unwrap()
            }
        };
        let all = self.ordered();
        let to = (from + self.page_size).min(all.len());
        let next = (to < all.len()).then(|| format!("{}:{to}", self.generation));
        Ok((all[from..to].to_vec(), next))
    }

    fn revs(&self) -> ChangeState {
        ChangeState {
            epoch: format!("e{}", self.epoch),
            domains: [("reading".to_string(), self.rev)].into(),
        }
    }

    /// The list changed (and so did its counter).
    fn moved(&mut self) {
        self.generation += 1;
        self.rev += 1;
    }

    fn nth(&self, at: usize) -> Option<Key> {
        let n = self.rows.len();
        (n > 0).then(|| self.rows.keys().nth(at % n).unwrap().clone())
    }

    fn newest(&self) -> Option<(Key, Row)> {
        self.ordered().into_iter().next()
    }
}

/// The client: `Engine`'s reading state, with time in ms.
#[derive(Debug)]
struct Client {
    local: BTreeMap<Key, Row>,
    epoch: Option<u32>,
    /// The `reading` revision last accepted.
    seen: Option<u64>,
    mark: Option<i64>,
    walk: Option<Walk>,
    deep_pending: bool,
    reads_sent: bool,
    full_at: Option<i64>,
    full_every: i64,
    reading_at: Option<i64>,
    whole_at: Option<i64>,
    subs: u64,
    budget: u32,
}

/// What one pull did.
#[derive(Debug, Default)]
struct PullLog {
    full: bool,
    start: Option<Start>,
    in_progress: bool,
    requests: u32,
    /// A complete read: `Some(true)` a whole walk, `Some(false)` a head read.
    done: Option<bool>,
    removed: Vec<Key>,
    /// The request each key was written by.
    written: BTreeMap<Key, u32>,
    /// The list changed part-way through.
    interrupted: bool,
}

#[derive(Debug, Clone)]
enum Ev {
    /// A new post at the head.
    Head {
        sub: u8,
    },
    /// A row the studio wrote late: stamped this long ago.
    Late {
        sub: u8,
        back_s: u32,
    },
    /// A change that restamps a row (an edit, a withdrawal).
    Edit {
        at: usize,
    },
    /// A change that keeps the stamp (an edit that kept it, another
    /// device's read mark).
    Silent {
        at: usize,
    },
    Delete {
        at: usize,
    },
    Unsubscribe {
        sub: u8,
    },
    /// The counter moved and the list didn't.
    Noise,
    /// A read mark from this Mac.
    OwnRead,
    /// The database was restored: a new epoch, every `keep`th row kept.
    Restore {
        keep: u8,
    },
    Pull {
        background: bool,
        /// A list change before this request.
        interrupt: Option<(u8, Box<Ev>)>,
    },
}

struct Sim {
    s: Server,
    c: Client,
    now: i64,
}

impl Sim {
    fn new(rows: &[(u8, u32)], page_size: usize, budget: u32) -> Sim {
        let now = 10 * 24 * 3600 * SEC;
        let mut s = Server {
            rows: BTreeMap::new(),
            epoch: 1,
            rev: 1,
            generation: 1,
            subs: 1,
            page_size,
            next_id: 0,
        };
        for &(sub, age_s) in rows {
            s.next_id += 1;
            s.rows.insert(
                (format!("s{sub}"), format!("r{}", s.next_id)),
                Row {
                    stamp: now - age_s as i64 * SEC,
                    ver: 1,
                },
            );
        }
        let c = Client {
            local: BTreeMap::new(),
            epoch: None,
            seen: None,
            mark: None,
            walk: None,
            deep_pending: false,
            reads_sent: false,
            full_at: None,
            full_every: EVERY,
            reading_at: None,
            whole_at: None,
            subs: 0,
            budget,
        };
        Sim { s, c, now }
    }

    /// A change on the server (not a pull).
    fn apply(&mut self, ev: &Ev) {
        let now = self.now;
        let s = &mut self.s;
        match ev {
            Ev::Head { sub } | Ev::Late { sub, .. } => {
                let back = match ev {
                    Ev::Late { back_s, .. } => *back_s as i64 * SEC,
                    _ => 0,
                };
                s.next_id += 1;
                s.rows.insert(
                    (format!("s{sub}"), format!("r{}", s.next_id)),
                    Row {
                        stamp: now - back,
                        ver: 1,
                    },
                );
                s.moved();
            }
            Ev::Edit { at } => {
                if let Some(k) = s.nth(*at) {
                    let r = s.rows.get_mut(&k).unwrap();
                    r.stamp = now;
                    r.ver += 1;
                    s.moved();
                }
            }
            Ev::Silent { at } => {
                if let Some(k) = s.nth(*at) {
                    s.rows.get_mut(&k).unwrap().ver += 1;
                    s.rev += 1;
                }
            }
            Ev::Delete { at } => {
                if let Some(k) = s.nth(*at) {
                    s.rows.remove(&k);
                    s.moved();
                }
            }
            Ev::Unsubscribe { sub } => {
                let id = format!("s{sub}");
                s.rows.retain(|k, _| k.0 != id);
                s.subs += 1;
                s.moved();
            }
            Ev::Noise => s.rev += 1,
            Ev::OwnRead => {
                s.rev += 1;
                self.c.reads_sent = true;
            }
            Ev::Restore { keep } => {
                let keep = (*keep as usize).max(1);
                let mut n = 0;
                s.rows.retain(|_, _| {
                    n += 1;
                    n % keep == 0
                });
                s.epoch += 1;
                s.rev = 1;
                s.generation += 1;
            }
            Ev::Pull { .. } => unreachable!("a pull isn't a server change"),
        }
    }

    fn opts() -> SyncOptions {
        SyncOptions {
            full_pull_every: Duration::from_millis(EVERY as u64),
            full_pull_max: Duration::from_millis(MAX as u64),
            ..SyncOptions::default()
        }
    }

    /// One pull, as `Engine::pull_locked` does the reading list's part,
    /// checking (f), (c), (b) and (e).
    fn pull(&mut self, background: bool, mut interrupt: Option<(u8, Box<Ev>)>) -> PullLog {
        let now = self.now;
        let mut log = PullLog::default();
        // (e): the server's newest row, if it's a post this Mac hasn't got
        // (new, or restamped).
        let head = self
            .s
            .newest()
            .filter(|(k, r)| self.c.local.get(k).is_none_or(|l| l.stamp != r.stamp))
            .map(|(k, _)| k);

        // `revisions()`: a new epoch forgets what was accepted.
        let c = &mut self.c;
        if c.epoch != Some(self.s.epoch) {
            c.seen = None;
            c.mark = None;
            c.full_at = None;
            c.full_every = EVERY;
            c.walk = None;
            c.epoch = Some(self.s.epoch);
        }
        let rev = Some(self.s.revs());
        let full = c.full_at.is_none_or(|t| now - t >= c.full_every);
        let subs_changed = c.subs != self.s.subs;
        c.subs = self.s.subs;
        let gap_open = !background || c.reading_at.is_none_or(|t| now - t >= GAP);
        let moved = c.seen != Some(self.s.rev);
        let forced = full && !moved;
        let st = start(Due {
            in_progress: c.walk.is_some(),
            full,
            subs_changed,
            deep_due: c.deep_pending && c.whole_at.is_none_or(|t| now - t >= EVERY),
            moved,
            gap_open,
            has_mark: c.mark.is_some(),
        });
        log.full = full;
        log.start = Some(st);
        log.in_progress = c.walk.is_some();

        let mut surprise = false;
        if st != Start::Nothing {
            let reads_sent = std::mem::take(&mut self.c.reads_sent);
            let held = (st == Start::Continue)
                .then(|| self.c.walk.take())
                .flatten();
            let mut r = Read::begin(
                st,
                held,
                rev.clone(),
                forced,
                self.c.mark,
                self.c.budget,
                reads_sent,
            )
            .expect("something to read");
            loop {
                if interrupt
                    .as_ref()
                    .is_some_and(|(n, _)| *n as u32 == log.requests)
                {
                    let (_, ev) = interrupt.take().unwrap();
                    self.apply(&ev);
                    log.interrupted = true;
                }
                log.requests += 1;
                let (rows, next) = match self.s.page(r.walk.cursor.as_deref()) {
                    Ok(p) => p,
                    Err(()) => match r.on_expired(rev.clone()) {
                        Step::Fetch => continue,
                        _ => {
                            self.stop(r);
                            break;
                        }
                    },
                };
                r.walk.keys.extend(rows.iter().map(|(k, _)| k.clone()));
                let mut changed = false;
                let mut written_ms = Vec::new();
                for (k, row) in &rows {
                    if self.c.local.get(k) != Some(row) {
                        self.c.local.insert(k.clone(), *row);
                        changed = true;
                        written_ms.push(row.stamp);
                        log.written.entry(k.clone()).or_insert(log.requests);
                    }
                }
                surprise |= changed && r.walk.forced;
                let step = r.on_page(PageFacts {
                    next,
                    changed,
                    oldest_ms: rows.iter().map(|(_, r)| r.stamp).min(),
                    newest_ms: rows.iter().map(|(_, r)| r.stamp).max(),
                    written_ms,
                });
                match step {
                    Step::Fetch => {}
                    Step::Stop => {
                        self.stop(r);
                        break;
                    }
                    Step::Done { whole } => {
                        if whole {
                            let keys = &r.walk.keys;
                            let before: Vec<Key> = self.c.local.keys().cloned().collect();
                            self.c.local.retain(|k, _| keys.contains(k));
                            log.removed = before
                                .into_iter()
                                .filter(|k| !self.c.local.contains_key(k))
                                .collect();
                            surprise |= !log.removed.is_empty() && r.walk.forced;
                        }
                        // `finish_read`
                        let c = &mut self.c;
                        c.seen = r
                            .walk
                            .rev
                            .as_ref()
                            .and_then(|v| v.domains.get("reading").copied());
                        if let Some(m) = r.next_mark(whole) {
                            c.mark = Some(m);
                        }
                        c.reading_at = Some(now);
                        if whole {
                            c.whole_at = Some(now);
                            c.deep_pending = false;
                        }
                        c.deep_pending |= r.deep;
                        log.done = Some(whole);
                        break;
                    }
                }
            }
        }
        let c = &mut self.c;
        if full {
            c.full_at = Some(now);
            let next = next_full_every(
                Duration::from_millis(c.full_every as u64),
                surprise,
                &Self::opts(),
            );
            c.full_every = next.as_millis() as i64;
        } else if surprise {
            c.full_every = EVERY;
        }

        // (f) the page budget.
        assert!(
            log.requests <= self.c.budget.max(2),
            "(f) {} page requests in one pull, budget {}: {log:?}",
            log.requests,
            self.c.budget
        );
        // (c) nothing removed but by a complete walk, and only rows gone.
        if !log.removed.is_empty() {
            assert_eq!(log.done, Some(true), "(c) removed outside a complete walk");
            for k in &log.removed {
                assert!(
                    !self.s.rows.contains_key(k),
                    "(c) removed {k:?}, still on the server"
                );
            }
        }
        // (b) a complete walk leaves exactly the server's rows.
        if log.done == Some(true) {
            let held: BTreeSet<_> = self.c.local.keys().collect();
            let served: BTreeSet<_> = self.s.rows.keys().collect();
            assert_eq!(held, served, "(b) a complete walk left a different set");
        }
        // (e) a new head post comes with the first or second request.
        if let Some(k) = head
            && !background
            && !log.interrupted
        {
            let at = log.written.get(&k);
            assert!(
                at.is_some_and(|&n| n <= 2),
                "(e) the new head post {k:?} wasn't merged by request 2 \
                 (written at {at:?}): {log:?}"
            );
        }
        log
    }

    /// `stop_read`.
    fn stop(&mut self, r: Read) {
        let deep = r.deep;
        if let Some(w) = r.kept() {
            self.c.walk = Some(w);
        }
        self.c.deep_pending |= deep;
    }

    fn converged(&self) -> bool {
        // A revision is only comparable within its epoch.
        self.c.epoch == Some(self.s.epoch)
            && self.c.seen == Some(self.s.rev)
            && self.c.local == self.s.rows
    }

    /// The worker's cadence while nothing changes: soon while a walk has a
    /// cursor (`reading_walk_pending`), else every pull interval.
    fn wait(&mut self) {
        let pending = self.c.walk.as_ref().is_some_and(|w| w.cursor.is_some());
        self.now += if pending { pause() } else { PULL_INTERVAL };
    }

    /// (a) and (d): the server goes quiet.
    fn quiet(&mut self) {
        let pages = self.s.rows.len().div_ceil(self.s.page_size).max(1);
        let pulls = pages.div_ceil(self.c.budget as usize) as i64 + 2;
        // The safety net's longest wait, a deep walk's, and the walk itself.
        let bound = MAX + EVERY + 2 * PULL_INTERVAL + pulls * PULL_INTERVAL;
        let from = self.now;
        // Converged, and nothing left to do: a deep walk asked for (by a
        // head read that found nothing for a moved counter) may still be
        // owed, since this Mac can't know the rows already match.
        let settled = |sim: &Sim| sim.converged() && !sim.c.deep_pending && sim.c.walk.is_none();
        while !settled(self) {
            assert!(
                self.now - from <= bound,
                "(a) not settled {}s after the server went quiet: \
                 seen {:?} of {}, held {} of {} rows ({} differ), deep {}, walk {:?}",
                (self.now - from) / SEC,
                self.c.seen,
                self.s.rev,
                self.c.local.len(),
                self.s.rows.len(),
                self.s
                    .rows
                    .iter()
                    .filter(|(k, r)| self.c.local.get(*k) != Some(*r))
                    .count(),
                self.c.deep_pending,
                self.c.walk.as_ref().map(|w| (&w.cursor, w.keys.len())),
            );
            self.wait();
            self.pull(true, None);
        }
        // (d) converged: only the safety net reads, and it backs off.
        let until = self.now + 3 * MAX;
        let mut last_full: Option<i64> = None;
        let mut gaps = Vec::new();
        while self.now < until {
            self.wait();
            let log = self.pull(true, None);
            assert!(self.converged(), "(d) a quiet pull lost convergence");
            if log.full {
                if let Some(t) = last_full {
                    gaps.push(self.now - t);
                }
                last_full = Some(self.now);
            }
            assert!(
                log.requests == 0 || log.full || log.in_progress,
                "(d) a quiet, converged pull read {} pages: {log:?}",
                log.requests
            );
        }
        assert!(
            gaps.windows(2).all(|w| w[1] >= w[0] || w[0] >= MAX),
            "(d) the quiet safety net didn't back off: {gaps:?}"
        );
    }

    fn run(&mut self, evs: &[(Ev, u8)]) {
        for (ev, dt) in evs {
            self.now += *dt as i64 * SEC + 1;
            match ev {
                Ev::Pull {
                    background,
                    interrupt,
                } => {
                    self.pull(*background, interrupt.clone());
                }
                ev => self.apply(ev),
            }
        }
        self.quiet();
    }
}

fn change() -> impl Strategy<Value = Ev> {
    prop_oneof![
        3 => (0u8..4).prop_map(|sub| Ev::Head { sub }),
        2 => (0u8..4, 0u32..3 * 3600).prop_map(|(sub, back_s)| Ev::Late { sub, back_s }),
        2 => any::<usize>().prop_map(|at| Ev::Edit { at }),
        1 => any::<usize>().prop_map(|at| Ev::Silent { at }),
        1 => any::<usize>().prop_map(|at| Ev::Delete { at }),
        1 => (0u8..4).prop_map(|sub| Ev::Unsubscribe { sub }),
        1 => Just(Ev::Noise),
        1 => Just(Ev::OwnRead),
        1 => (1u8..4).prop_map(|keep| Ev::Restore { keep }),
    ]
}

fn event() -> impl Strategy<Value = (Ev, u8)> {
    let pull = (
        any::<bool>(),
        proptest::option::weighted(0.15, (0u8..4, change().prop_map(Box::new))),
    )
        .prop_map(|(background, interrupt)| Ev::Pull {
            background,
            interrupt,
        });
    (prop_oneof![5 => pull, 4 => change()], 0u8..120)
}

/// A history: the server's first rows (subscription, age in s), the page
/// size, the page budget, and the events.
#[allow(clippy::type_complexity)]
fn history(
    max_rows: usize,
    max_events: usize,
) -> impl Strategy<Value = (Vec<(u8, u32)>, usize, u32, Vec<(Ev, u8)>)> {
    (
        proptest::collection::vec((0u8..4, 0u32..5 * 24 * 3600), 0..max_rows),
        1usize..6,
        1u32..5,
        proptest::collection::vec(event(), 0..max_events),
    )
}

fn check(rows: &[(u8, u32)], page_size: usize, budget: u32, evs: &[(Ev, u8)]) {
    Sim::new(rows, page_size, budget).run(evs);
}

proptest! {
    #![proptest_config(ProptestConfig {
        failure_persistence: None,
        ..ProptestConfig::default()
    })]

    #[test]
    fn model_reads_converge_prune_right_and_stay_cheap(
        (rows, page_size, budget, evs) in history(60, 40)
    ) {
        check(&rows, page_size, budget, &evs);
    }
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 5000,
        failure_persistence: None,
        ..ProptestConfig::default()
    })]

    #[test]
    #[ignore = "slow: the long run (see the module docs)"]
    fn model_long_histories(
        (rows, page_size, budget, evs) in history(300, 150)
    ) {
        check(&rows, page_size, budget, &evs);
    }
}

/// The 0.25 bug, shrunk: a list longer than one pull's pages (here 14
/// one-row pages, two a pull) must be walked across pulls and accepted.
/// Dropping the walk at the budget (each pull from page 1 again) fails (a):
/// "not settled 8940s after the server went quiet: seen None of 1, held 2
/// of 14 rows".
#[test]
fn model_a_list_longer_than_the_budget_is_walked_across_pulls() {
    check(&[(0, 0); 14], 1, 1, &[]);
}

/// Shrunk from (e) with a budget of one page: a walk under way met a 409
/// for the new post and spent the pull's only request on it. A pull now
/// always has room for the 409 and the first page again.
#[test]
fn model_a_new_post_shows_while_a_walk_is_under_way() {
    let pull = || {
        (
            Ev::Pull {
                background: false,
                interrupt: None,
            },
            0,
        )
    };
    check(
        &[(0, 0); 6],
        1,
        1,
        &[pull(), (Ev::Head { sub: 0 }, 0), pull()],
    );
}
