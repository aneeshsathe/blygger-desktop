// Live mode: Burrow on the owner's own blyg. Served from the blyg at
// /burrow/, the page reaches the owner API at {mount}/api on the same origin. The
// Studio session cookie (HttpOnly, so this page never sees it) is the only
// credential: there's no token, nothing is stored, and nothing is logged.
// Every request goes through `call`, which refuses anything that isn't a
// same-origin /api/ path. What to send and what an answer means is Burrow's
// Rust (src/api.rs, src/live.rs); this only fetches.
//
// Local first: edits land in the store at once and persist to IndexedDB;
// the outbox (edits, deletes, read marks, hopper changes) is sent after
// typing stops, with the revision each edit was based on. Offline, it
// waits and replays when the blyg answers again. A revision conflict
// never overwrites: the blyg's copy is fetched and you choose.

import { Burrow, Pager } from './pkg/blyg_web.js';
import { $, S, esc, now, load, persist, persistReading, toast } from './core.js';

const API = '/api/';

/** The blyg's mount: the app is served at {mount}/burrow/, the owner API at
    {mount}/api and the Studio at {mount}/studio. Empty for a blyg at the
    host root. Paths here (and from the Rust) are mount-relative `/api/…`. */
export const MOUNT = (() => {
  const p = location.pathname;
  const i = p.lastIndexOf('/burrow/');
  return i > 0 ? p.slice(0, i) : '';
})();

// The sign-in link follows the mount too.
for (const a of document.querySelectorAll('a[href^="/studio/login"]')) {
  a.href = `${MOUNT}/studio/login?return_to=${encodeURIComponent(`${MOUNT}/burrow/`)}`;
}

/** Whether `path` is a same-origin owner-API path (and nothing else). */
export function apiPath(path) {
  return typeof path === 'string' && path.startsWith(API) && !/\/\.\.?(\/|$)|\\|\/\/|^[a-z]+:/i.test(path);
}

/** One owner-API request. Never throws: no answer is status 0. */
export async function call(method, path, body) {
  if (!apiPath(path)) throw new Error('Burrow only talks to this blyg’s own /api/');
  const headers = { accept: 'application/json' };
  // --- web-writing --- A FormData body (an image upload) goes as multipart.
  const form = typeof FormData !== 'undefined' && body instanceof FormData;
  // --- end web-writing ---
  if (body !== undefined && body !== null && !form) headers['content-type'] = 'application/json';
  try {
    const res = await fetch(MOUNT + path, {
      method, headers, credentials: 'same-origin', cache: 'no-store', redirect: 'error', mode: 'same-origin',
      body: form ? body : body !== undefined && body !== null ? JSON.stringify(body) : undefined,
    });
    return { status: res.status, body: await res.text(), retryAfter: res.headers.get('retry-after') ?? undefined, www: res.headers.has('www-authenticate') };
  } catch {
    return { status: 0, body: '', retryAfter: undefined, www: false };
  }
}

/** What this origin is: `{mode: live|signin|unsupported|demo|offline}`. */
export async function detect() {
  if (!/^https?:$/.test(location.protocol)) return { mode: 'demo' };
  const r = await call('GET', '/api/capabilities');
  return JSON.parse(Burrow.detect(r.status, r.body, r.www));
}

/** A whole cursor-paged list (restarted on cursor-expired, never partial).
    A failure carries the answer's status and Retry-After, for the backoff. */
async function readAll(path, query = '', limit = 100) {
  const p = new Pager(path, query, limit);
  try {
    for (let next = p.next_path(); next !== undefined; next = p.next_path()) {
      const r = await call('GET', next);
      const err = JSON.parse(p.feed(r.status, r.body, r.retryAfter));
      if (err) return { error: err, status: r.status, retryAfter: r.retryAfter };
    }
    return { json: p.items() };
  } finally { p.free(); }
}

// ---------------------------------------------------------------- start
/** Open the live store (as last persisted, for offline) on this blyg. */
export async function start(online) {
  let base = location.origin;
  if (online) {
    const st = await call('GET', '/api/settings');
    if (st.status === 200) base = Burrow.public_base(st.body, location.origin);
    // --- web-writing --- The settings, for the publish sheet's AI offer.
    if (st.status === 200) S.wSettings = st.body;
    // --- end web-writing ---
  }
  S.B = Burrow.new_live(now(), await load('live'), await load('live-reading'), online ? base : '');
  if (!online) S.B.set_offline(true);
  hooks();
  // --- web-writing --- Settings now; contacts once the page has drawn.
  if (S.wSettings) { S.B.w_set_settings(S.wSettings); S.wSettings = null; }
  if (online) setTimeout(() => S.writing?.liveStarted(), 1500);
  // --- end web-writing ---
}

// ---------------------------------------------------------------- refresh
// How updates arrive (docs/SPEC.md, "Burrow on the web"): while the tab is
// visible and online, a `GET /api/changes` every 15 s (Studio's cadence),
// which costs one small request when nothing moved; a list is read again
// only when its counters moved. Hidden, it waits; shown again, it checks at
// once. A 429 waits its Retry-After and a 5xx backs off (src/live.rs `Pace`).
// Each list's safety net (read whole anyway after 10 min, doubling to 2 h
// while that finds nothing, src/live.rs `Net`) runs only while visible.
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
    } finally { refreshing = null; schedule(); }
  })();
  return refreshing;
}

/** One refresh. Each run plans under its own `/api/changes` answer
    (`plan.rev`) and records it for a list only once that list was read
    and installed, so a failed or overlapping read never marks a list
    seen. */
async function refreshOnce() {
  // The outbox is sent after typing stops anyway; here only if it holds
  // something (an empty flush redraws the page).
  if (JSON.parse(S.B.outbox_next('[]'))) await flush();
  if (S.B.is_offline()) return;
  const ch = await call('GET', '/api/changes');
  const plan = JSON.parse(S.B.changes_plan(ch.status, ch.body, ch.retryAfter, document.visibilityState === 'visible', now()));
  if (plan.error) { problem(plan.error); return; }
  if (plan.wait) { hold(plan.wait); return; }
  holdUntil = 0;
  nextDelay = plan.every;
  const rev = JSON.stringify(plan.rev);
  // A read that failed: back off on a 429 or 5xx, say what went wrong.
  const failed = (r) => {
    const w = S.B.poll_failed(r.status, r.retryAfter);
    if (w) hold(w);
    refreshFailed = true;
    problem(r.error);
  };
  let changed = false;
  if (plan.items) {
    const items = await readAll('/api/items');
    if (items.error) { failed(items); return; }
    S.B.live_items(items.json);
    S.B.changes_accept('items', rev, now(), items.json);
    changed = true;
  }
  if (plan.reading) {
    const rows = await readAll('/api/reading/imported');
    const hops = await readAll('/api/hoppers');
    if (!rows.error || rows.error.code === 'not-found') {
      S.B.live_reading(rows.error ? '[]' : rows.json, hops.error ? '[]' : hops.json);
      // Each row carries its hoppers and thumb: those are settled too.
      // (What was fetched paces the list's safety net, `fetched` in
      // `changes_accept`; the lists settled with it leave theirs alone.)
      if (!rows.error && !hops.error) {
        S.B.changes_accept('reading', rev, now(), rows.json + hops.json);
        for (const l of ['hoppers', 'signals']) S.B.changes_accept(l, rev, now(), undefined);
      }
      changed = true;
    } else { failed(rows); return; }
  } else {
    if (plan.hoppers) {
      const r = await readHoppers();
      if (r.error) { failed(r); return; }
      if (JSON.parse(S.B.live_hoppers(r.hoppers, r.members)).changed) changed = true;
      S.B.changes_accept('hoppers', rev, now(), r.hoppers + r.members);
    }
    if (plan.signals) {
      const sig = await readAll('/api/signals');
      if (sig.error && sig.error.code !== 'not-found') { failed(sig); return; }
      // No route: the rows' own thumbs stand.
      if (!sig.error && JSON.parse(S.B.live_signals(sig.json)).changed) changed = true;
      S.B.changes_accept('signals', rev, now(), sig.error ? '' : sig.json);
    }
  }
  if (refreshFailed && !S.B.is_offline()) { refreshFailed = false; S.B.sync_phase('synced'); if (!changed) S.render({ loadEditor: false }); }
  if (!changed) return;
  await persist({ quiet: true });
  await persistReading();
  S.render();
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
/** Send the outbox, one entry at a time. Returns when it's empty or stuck. */
export function flush() {
  if (!S.B?.is_live_mode() || S.B.is_offline()) return Promise.resolve();
  if (flushing) { again = true; return flushing; }
  flushing = (async () => {
    try {
      do { again = false; await sendAll(); } while (again && !S.B.is_offline());
    } finally { flushing = null; }
  })();
  return flushing;
}

async function sendAll() {
  const skip = [];
  let failed = false, refreshAfter = false, sent = 0;
  for (let i = 0; i < 400; i++) {
    const next = JSON.parse(S.B.outbox_next(JSON.stringify(skip)));
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
