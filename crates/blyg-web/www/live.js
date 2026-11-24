// Live mode: Burrow on the owner's own blyg. Served from the blyg at
// /burrow/, the page reaches the owner API at {mount}/api on the same origin. The
// Studio session cookie (HttpOnly, so this page never sees it) is the only
// credential: there's no token, nothing is stored, and nothing is logged.
// Every request goes through `call` (net.js), which refuses anything that
// isn't a same-origin /api/ path. What to send and what an answer means is
// Burrow's Rust (src/api.rs, src/live.rs); this only fetches.
//
// Local first: edits land in the store at once and persist to IndexedDB;
// the outbox (edits, deletes, read marks, hopper changes) is sent after
// typing stops, with the revision each edit was based on. Offline, it
// waits and replays when the blyg answers again. A revision conflict
// never overwrites: the blyg's copy is fetched and you choose.

import { Burrow, Pager } from './pkg/blyg_web.js';
import { $, S, esc, now, load, persist, persistReading, toast, pref } from './core.js';
import { MOUNT, SITE, apiPath, call, dropEarly, HELD } from './net.js';

export { MOUNT, SITE, apiPath, call };

// The sign-in link follows the mount too.
if (MOUNT !== null) for (const a of document.querySelectorAll('a[href^="/studio/login"]')) {
  a.href = `${MOUNT}/studio/login?return_to=${encodeURIComponent(`${MOUNT}/burrow/`)}`;
}

/** What this origin is: `{mode: live|signin|unsupported|demo|offline}`. */
export async function detect() {
  if (!/^https?:$/.test(location.protocol)) return { mode: 'demo' };
  // net.js worked the mount out before the WebAssembly was here; the Rust
  // has the rule (src/api.rs `mount_of`). Never guess another /api.
  if (MOUNT === null || Burrow.mount_of(location.pathname) !== MOUNT) {
    return { mode: 'unsupported', why: 'Burrow can’t tell where this blyg’s API is from this page’s address.' };
  }
  const r = await call('GET', '/api/capabilities');
  // Kept for `start`, so the shelf's and the parity screens' capability
  // checks don't read it a second time.
  detected = r.status === 200 ? r.body : undefined;
  return JSON.parse(Burrow.detect(r.status, r.body, r.www));
}
let detected;

/** `GET path`, conditionally once it was read before: a 304 comes back as
    the body last read (src/live.rs `cond_answer`), never as an empty
    list. `same`: the page's rows are as last read. */
async function getCond(path) {
  let r = await call('GET', path, undefined, S.B.cond_etag(path));
  let a = JSON.parse(S.B.cond_answer(path, r.status, r.body, r.etag));
  if (a.again) {
    r = await call('GET', path);
    a = JSON.parse(S.B.cond_answer(path, r.status, r.body, r.etag));
  }
  return { status: a.status ?? r.status, body: a.body ?? r.body, retryAfter: r.retryAfter, same: !!a.same };
}

/** A whole cursor-paged list (restarted on cursor-expired, never partial).
    A failure carries the answer's status and Retry-After, for the backoff.
    `first`, when given, gets the first page's items (JSON) as soon as it's
    in, while the rest is read. `same`: every page was as last read, so
    there's nothing to install. */
async function readAll(path, query = '', limit = 100, first) {
  const p = new Pager(path, query, limit);
  try {
    let pages = 0;
    let same = true;
    for (let next = p.next_path(); next !== undefined; next = p.next_path()) {
      const r = takeHead(next) ?? await getCond(next);
      same &&= r.same;
      const err = JSON.parse(p.feed(r.status, r.body, r.retryAfter));
      if (err) return { error: err, status: r.status, retryAfter: r.retryAfter };
      if (first && !pages++ && p.next_path() !== undefined) first(p.so_far());
    }
    return { json: p.items(), same };
  } finally { p.free(); }
}

/** Contract 6, a held reading list: whether its first page moved (src/live.rs
    `changes_plan`, `reading: 'head'`). A 304, or rows as they were, says
    nothing came in; a deeper change waits for the safety net. */
async function headMoved(path) {
  const p = new Pager(path, '', 100);
  try {
    const first = p.next_path();
    const r = await getCond(first);
    const err = JSON.parse(p.feed(r.status, r.body, r.retryAfter));
    if (err) return { error: err, status: r.status, retryAfter: r.retryAfter };
    if (!r.same) headPage = { path: first, r };
    return { moved: !r.same };
  } finally { p.free(); }
}
/** The first page `headMoved` just read, for the whole read that follows
    it: reading it twice was a duplicate request on every sync. Taken once. */
let headPage = null;
function takeHead(path) {
  if (headPage?.path !== path) return null;
  const r = headPage.r;
  headPage = null;
  return r;
}

// ---------------------------------------------------------------- start
let saved;
/** The live store and reading list as last written, read from IndexedDB
    once: app.js starts this while the WebAssembly compiles. */
export function loadSaved() {
  saved ??= Promise.all([load('live'), load('live-reading')]);
  return saved;
}

/** Open the live store (as last persisted, for offline) on this blyg. */
export async function start(online) {
  let base = SITE;
  if (online) {
    const st = await call('GET', '/api/settings');
    if (st.status === 200) base = Burrow.public_base(st.body, SITE);
    // --- web-writing --- The settings, for the publish sheet's AI offer.
    if (st.status === 200) S.wSettings = st.body;
    // --- end web-writing ---
  }
  const [store, reading] = await loadSaved();
  saved = undefined; // read once: a later start (none today) reads again
  S.B = Burrow.new_live(now(), store, reading, online ? base : '');
  if (!online) S.B.set_offline(true);
  if (online && detected) S.B.wr_loaded('capabilities', detected);
  detected = undefined;
  hooks();
  // --- web-writing --- Settings now; contacts once the page has drawn.
  if (S.wSettings) { S.B.w_set_settings(S.wSettings); S.wSettings = null; }
  if (online) setTimeout(() => S.writing?.liveStarted(), 1500);
  // --- end web-writing ---
}

// ---------------------------------------------------------------- refresh
// How updates arrive (docs/SPEC.md, "Burrow on the web"): while the tab is
// visible and online, a check every 15 s with `GET /api/changes` (Studio's
// cadence; a list is read again only when its counters moved), or, on a
// blyg without it (contract 6), every 60 s by conditional reads: every list
// asked with `If-None-Match`, a 304 for each that didn't change, and the
// reading list only by its first page until its safety net is due. Hidden,
// it waits; shown again, it checks at once. A 429 waits its Retry-After and
// a 5xx backs off (src/live.rs `Pace`). Each list's safety net (read whole
// anyway after 10 min, doubling to 2 h while that finds nothing, src/live.rs
// `Net`) runs only while visible.
let refreshing = null; // the run in progress
let refreshAgain = false; // a call came in during it: one more run after
let pollT = 0;
let nextDelay = 15_000; // until the next check, as the last run decided
let holdUntil = 0; // a 429 or 5xx: no check before this (Date.now() ms)
let refreshFailed = false;

/** Send what waits, then read again what changed on the blyg: its
    `/api/changes` revisions say which lists moved since they were last
    read (src/live.rs `changes_plan`), so a refresh that finds nothing new
    costs one request, not every post. One run at a time: a call while one
    runs queues exactly one more run after it (so a change made meanwhile
    is still read), and returns when that's done. */
export function refresh() {
  if (!S.B?.is_live_mode()) return Promise.resolve();
  if (refreshing) { refreshAgain = true; return refreshing; }
  clearTimeout(pollT);
  refreshing = (async () => {
    try {
      do { refreshAgain = false; await refreshOnce(); } while (refreshAgain && !S.B.is_offline());
    } finally { refreshing = null; dropEarly(); schedule(); }
  })();
  return refreshing;
}

/** One refresh. Each run plans under its own `/api/changes` answer
    (`plan.rev`) and records it for a list only once that list was read
    and installed, so a failed or overlapping read never marks a list
    seen. */
async function refreshOnce() {
  // The outbox is sent after typing stops anyway; here only if it holds
  // something (an empty flush redraws the page). What was read early
  // (net.js) predates what it sends, so it goes.
  if (JSON.parse(S.B.outbox_next_settled('[]', now()))) { dropEarly(); await flush({ all: false }); }
  if (S.B.is_offline()) return;
  // Contract 6 has no /api/changes: once it answered 404 it isn't asked
  // again for a while (src/live.rs `changes_due`), and the plan is made as
  // if it had answered 404 again.
  const ch = S.B.changes_due(now()) ? await call('GET', '/api/changes') : { status: 404, body: '', retryAfter: undefined };
  const plan = JSON.parse(S.B.changes_plan(ch.status, ch.body, ch.retryAfter, document.visibilityState === 'visible', now()));
  if (plan.error) { problem(plan.error); return; }
  if (plan.wait) { hold(plan.wait); return; }
  holdUntil = 0;
  nextDelay = plan.every;
  const rev = JSON.stringify(plan.rev);
  // The lists are read side by side, and each is drawn as soon as it's in:
  // your posts don't wait for thousands of reading rows.
  let changed = false;
  let undrawn = false; // changed, and not drawn yet
  let whole = false; // the whole reading list was read
  let failure = null;
  const fail = (r) => { failure ??= r; };
  const drawn = () => { changed = true; S.render(); };
  const jobs = [];
  if (plan.items) {
    jobs.push(readAll('/api/items').then((items) => {
      if (items.error) { fail(items); return; }
      // Every page a 304 (or as it was): nothing to install or draw.
      if (!(plan.validating && items.same)) { S.B.live_items(items.json); drawn(); }
      S.B.changes_accept('items', rev, now(), items.json);
    }));
  }
  // Contract 6, a held reading list: only its head, unless it moved.
  let readWhole = plan.reading === true;
  let headMovedNow = false; // the head just read moved: install whatever the pages say
  headPage = null;
  if (plan.reading === 'head') {
    const head = await headMoved('/api/reading/imported');
    if (head.error && head.error.code !== 'not-found') fail(head);
    else readWhole = headMovedNow = !!head.moved;
  }
  if (readWhole) {
    // A first start (nothing held yet) draws the first page at once, in
    // the list's order, and puts the rest in behind it.
    const empty = S.B.reading_held() === 0;
    const hopsP = readAll('/api/hoppers');
    jobs.push(readAll('/api/reading/imported', '', 100, empty ? (page) => {
      if (S.B.reading_held() === 0) { S.B.live_reading(page, '[]'); drawn(); }
    } : undefined).then(async (rows) => {
      const hops = await hopsP;
      if (!rows.error || rows.error.code === 'not-found') {
        if (plan.validating && rows.same && hops.same && !headMovedNow) {
          // Every page as it was: nothing to install.
          S.B.changes_accept('reading', rev, now(), rows.json + hops.json);
          return;
        }
        S.B.live_reading(rows.error ? '[]' : rows.json, hops.error ? '[]' : hops.json);
        // Each row carries its hoppers and thumb: those are settled too.
        // (What was fetched paces the list's safety net, `fetched` in
        // `changes_accept`; the lists settled with it leave theirs alone.)
        if (!rows.error && !hops.error) {
          S.B.changes_accept('reading', rev, now(), rows.json + hops.json);
          for (const l of ['hoppers', 'signals']) S.B.changes_accept(l, rev, now(), undefined);
          whole = true;
        }
        drawn();
      } else fail(rows);
    }));
  } else {
    if (plan.hoppers) {
      jobs.push(readHoppers().then((r) => {
        if (r.error) { fail(r); return; }
        if (JSON.parse(S.B.live_hoppers(r.hoppers, r.members)).changed) changed = undrawn = true;
        S.B.changes_accept('hoppers', rev, now(), r.hoppers + r.members);
      }));
    }
    if (plan.signals) {
      jobs.push(readAll('/api/signals').then((sig) => {
        if (sig.error && sig.error.code !== 'not-found') { fail(sig); return; }
        // No route: the rows' own thumbs stand.
        if (!sig.error && JSON.parse(S.B.live_signals(sig.json)).changed) changed = undrawn = true;
        S.B.changes_accept('signals', rev, now(), sig.error ? '' : sig.json);
      }));
    }
  }
  await Promise.all(jobs);
  // --- shelf sync --- after your posts are in, so shelved ones map to
  // them; also when this side's shelf waits to be sent.
  if (!failure && (plan.shelf || S.B.shelf_state() === 'waiting')) {
    const sh = await syncShelf(rev);
    if (sh.fail) fail(sh.fail);
    else if (sh.changed) changed = undrawn = true;
  }
  if (failure) {
    // A read that failed: back off on a 429 or 5xx, say what went wrong.
    const w = S.B.poll_failed(failure.status, failure.retryAfter);
    if (w) hold(w);
    refreshFailed = true;
    problem(failure.error);
  } else if (refreshFailed && !S.B.is_offline()) {
    refreshFailed = false;
    S.B.sync_phase('synced');
    if (!changed) S.render({ loadEditor: false });
  }
  if (!changed) return;
  if (undrawn) S.render();
  await persist({ quiet: true });
  // From now on, a start reads only what changed (net.js reads no first
  // pages early).
  if (await persistReading() && whole) pref.set(HELD, '1');
}

/** --- shelf sync --- Read the blyg's shelf (`GET /api/shelf`) and send
    this side's changes: src/shelf.rs merges them onto it and says what to
    `PUT`; a 409 merges onto the list it carries and sends once more. On a
    blyg without `getShelf`/`putShelf` the shelf stays in this browser.
    Returns `{changed}`, or `{fail}` for the refresh's backoff. */
async function syncShelf(rev) {
  if (!S.B.wr_ops_known()) {
    const c = await call('GET', '/api/capabilities');
    if (c.status === 200) S.B.wr_loaded('capabilities', c.body);
  }
  if (!S.B.shelf_served()) {
    if (S.B.wr_ops_known()) S.B.changes_accept('shelf', rev, now(), undefined);
    return {};
  }
  const g = await getCond('/api/shelf');
  let step = JSON.parse(S.B.shelf_answer(g.status, g.body));
  let changed = !!step.changed;
  for (let i = 0; step.put && i < 2; i++) {
    if (step.toast) toast(esc(step.toast), 4200);
    const r = await call(step.put.method, step.put.path, step.put.body);
    step = JSON.parse(S.B.shelf_stored(r.status, r.body, i === 0));
    changed ||= !!step.changed;
    if (step.error) { step.status = r.status; step.retryAfter = r.retryAfter; }
  }
  if (step.error) {
    const e = step.error;
    if (e.code === 'offline' || e.code === 'unauthorized' || e.code === 'rate-limited') {
      return { fail: { error: e, status: step.status ?? g.status, retryAfter: step.retryAfter ?? g.retryAfter } };
    }
    toast(esc(e.text), 4200);
    return { changed };
  }
  S.B.changes_accept('shelf', rev, now(), g.body);
  return { changed };
}

/** The hoppers and each one's memberships, as JSON for `live_hoppers`. A
    blyg without hoppers has none. */
async function readHoppers() {
  const hops = await readAll('/api/hoppers');
  if (hops.error) return hops.error.code === 'not-found' ? { hoppers: '[]', members: '{}' } : hops;
  const members = {};
  for (const h of JSON.parse(hops.json)) {
    if (typeof h?.id !== 'string') continue;
    const m = await readAll(`/api/hoppers/${encodeURIComponent(h.id)}/memberships`);
    if (m.error && m.error.code !== 'not-found') return m;
    members[h.id] = m.error ? [] : JSON.parse(m.json);
  }
  return { hoppers: hops.json, members: JSON.stringify(members) };
}

/** No check for `ms`: the blyg asked to slow down, or is failing. */
function hold(ms) {
  nextDelay = ms;
  holdUntil = Date.now() + ms;
}

/** The next check, while the page is live, visible, online and signed in. */
function schedule() {
  clearTimeout(pollT);
  if (!S.B?.is_live_mode() || S.B.is_offline() || document.visibilityState !== 'visible') return;
  if (!$('#signin').hidden) return;
  pollT = setTimeout(refresh, Math.max(nextDelay, holdUntil - Date.now(), 1000));
}

// ---------------------------------------------------------------- outbox
let flushing = null;
let again = false;
let againAll = false;
/** Send the outbox, one entry at a time. Returns when it's empty or stuck.
    `all: false` (a send nobody asked for: the pause after typing, a
    refresh) leaves a post still being typed for the save after typing
    stops (src/live.rs `outbox_next_settled`): sending it mid-sentence sent
    one save per keystroke burst. What you ask for (publish, a command)
    sends everything. */
export function flush({ all = true } = {}) {
  if (!S.B?.is_live_mode() || S.B.is_offline()) return Promise.resolve();
  if (flushing) { again = true; againAll ||= all; return flushing; }
  flushing = (async () => {
    try {
      let everything = all;
      do { again = false; await sendAll(everything); everything = againAll; againAll = false; } while (again && !S.B.is_offline());
      // --- shelf sync --- a shelf change goes in a refresh (`syncShelf`).
      if (S.B.shelf_state() === 'waiting') setTimeout(refresh, 0);
    } finally { flushing = null; }
  })();
  return flushing;
}

async function sendAll(all) {
  const skip = [];
  let failed = false, refreshAfter = false, sent = 0;
  for (let i = 0; i < 400; i++) {
    const next = JSON.parse(all ? S.B.outbox_next(JSON.stringify(skip)) : S.B.outbox_next_settled(JSON.stringify(skip), now()));
    if (!next) break;
    if (!sent++) { S.B.sync_phase('syncing'); S.render(); }
    const { req, ...op } = next;
    const r = await call(req.method, req.path, req.body);
    const step = JSON.parse(S.B.outbox_answer(JSON.stringify(op), r.status, r.body, r.retryAfter));
    if (step.fetch) {
      const g = await call('GET', step.fetch);
      const got = S.B.live_conflict(step.conflict, g.status, g.body);
      failed = true;
      skip.push(`${op.op}:${op.id}`);
      if (got !== 'true') { problem(JSON.parse(got)); break; }
    }
    if (step.toast) toast(esc(step.toast), 4200);
    if (step.refresh || (op.op === 'delete' && !step.error && !step.fetch)) refreshAfter = true;
    if (step.error) {
      if (step.stop) { problem(step.error); failed = true; break; }
      skip.push(`${op.op}:${op.id}`);
      toast(esc(step.error.text), 4200);
      failed = true;
    }
  }
  if (!sent) {
    if (!S.B.is_offline()) S.B.sync_phase('synced');
    S.render();
    return;
  }
  await persist({ quiet: true });
  if (!S.B.is_offline()) S.B.sync_phase(failed ? 'error' : 'synced');
  S.render({ loadEditor: false });
  if (refreshAfter) setTimeout(refresh, 0);
}

/** Something the user should know about the connection. */
function problem(err) {
  if (err.code === 'offline') { goOffline(); return; }
  if (err.code === 'unauthorized') { signIn(); return; }
  if (err.code === 'rate-limited') { setTimeout(flush, (err.retry_after || 60) * 1000); }
  toast(esc(err.text), 4200);
  S.B.sync_phase('error');
  S.render();
}

function goOffline() {
  if (!S.B.is_offline()) { S.B.set_offline(true); toast('Offline. Keep writing: your changes are kept and sent when the blyg is back.', 3600); }
  S.render();
}

async function backOnline() {
  if (!S.B?.is_live_mode()) return;
  const r = await call('GET', '/api/capabilities');
  const m = JSON.parse(Burrow.detect(r.status, r.body, r.www));
  if (m.mode === 'offline') return;
  if (m.mode === 'signin') { signIn(); return; }
  if (S.B.is_offline()) {
    const pending = S.B.set_offline(false);
    if (pending) toast(`Back online · sending ${pending} change${pending === 1 ? '' : 's'}`);
  }
  hideSignIn();
  holdUntil = 0;
  await refresh();
}

// ---------------------------------------------------------------- publish, restore
/** Publish `o.id` on the blyg (user-initiated only: the publish sheet or
    capture's Publish). Returns an Outcome, as the demo's publish does. */
export async function publish(o) {
  await flush();
  // --- web-writing --- With a drafted note or a change kind (o.ex), the
  // request says so (noteGenerated, changeCategory); the blyg's warnings show.
  const pr = JSON.parse(o.ex && S.writing ? S.writing.publishRequest(o) : S.B.live_publish_request(o.id, o.note ?? ''));
  // --- end web-writing ---
  if (pr.error) return { action: 'toast', message: pr.error };
  const r = await call(pr.req.method, pr.req.path, pr.req.body);
  S.writing?.publishWarning(r); // --- web-writing ---
  return await afterCommand(o.id, r, o.note ?? '');
}

/** Discard changes: the blyg's working copy goes back to `o.version`. */
export async function restore(o) {
  await flush();
  const rr = JSON.parse(S.B.live_restore_request(o.id, o.version));
  if (rr.error) return { action: 'toast', message: rr.error };
  const r = await call(rr.req.method, rr.req.path, rr.req.body);
  const res = await afterCommand(o.id, r, '');
  return res.action === 'published' ? { action: 'toast', message: `Back to v${o.version} as published` } : res;
}

async function afterCommand(id, r, note) {
  const a = JSON.parse(S.B.live_command_answer(id, r.status, r.body));
  if (a.conflict) {
    const g = await call('GET', a.fetch);
    S.B.live_conflict(id, g.status, g.body);
    await persist({ quiet: true });
    return { action: 'toast', message: 'Changed elsewhere since you opened it. Choose which copy to keep, then publish.' };
  }
  if (a.error) {
    if (a.error.code === 'offline' || a.error.code === 'unauthorized') problem(a.error);
    return { action: 'toast', message: a.error.text };
  }
  const g = await call('GET', a.fetch);
  const res = JSON.parse(S.B.live_apply_item(id, g.status, g.body, note));
  await persist({ quiet: true });
  changedHere();
  return res.error ? { action: 'toast', message: res.error.text } : res;
}

/** A publish, restore, withdraw or delete landed on the blyg: read what
    else it moved (the list's order, Reading) now, not at the next check. */
export function changedHere() {
  if (S.B?.is_live_mode() && !S.B.is_offline()) setTimeout(refresh, 0);
}

// ---------------------------------------------------------------- sign in
/** No Studio session: sign in at Studio (in a new tab), then come back. */
export function signIn(why) {
  const box = $('#signin');
  $('#signin-h').textContent = why ? 'Burrow can’t use this blyg' : 'Sign in at Studio';
  $('#signin-why').textContent = why || 'Burrow uses your Studio session. Sign in at Studio, then come back to this tab.';
  box.hidden = false;
  if (S.B?.is_live_mode()) S.B.sync_phase('error');
}
function hideSignIn() { $('#signin').hidden = true; }

let hooked = false;
function hooks() {
  if (hooked) return;
  hooked = true;
  window.addEventListener('online', backOnline);
  window.addEventListener('offline', () => { if (S.B?.is_live_mode()) goOffline(); });
  // Back from signing in, or from another app: catch up.
  // The reading list is written when it changed and you leave the page
  // (not on every mark: it's thousands of posts).
  window.addEventListener('pagehide', () => { persistReading(); });
  // Hidden, the checks stop; shown again, one at once (unless the blyg
  // asked to wait), then every 15 s.
  document.addEventListener('visibilitychange', () => {
    if (document.visibilityState === 'hidden') { clearTimeout(pollT); persistReading(); return; }
    if (!S.B?.is_live_mode()) return;
    if (!$('#signin').hidden || S.B.is_offline()) backOnline();
    else if (Date.now() >= holdUntil) refresh();
    else schedule();
  });
  // Offline: try again now and then (navigator.onLine isn't always right).
  setInterval(() => { if (S.B?.is_live_mode() && S.B.is_offline()) backOnline(); }, 20_000);
}

/** The sign-in panel's own buttons. */
export function initSignIn(retry) {
  $('#signin-retry').addEventListener('click', retry);
}
