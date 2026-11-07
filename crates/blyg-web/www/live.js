// Live mode: Burrow on the owner's own blyg. Served from the blyg at
// /burrow/, the page reaches the owner API at /api on the same origin. The
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
import { $, S, esc, now, load, persist, toast } from './core.js';

const API = '/api/';

/** Whether `path` is a same-origin owner-API path (and nothing else). */
export function apiPath(path) {
  return typeof path === 'string' && path.startsWith(API) && !/\/\.\.?(\/|$)|\\|\/\/|^[a-z]+:/i.test(path);
}

/** One owner-API request. Never throws: no answer is status 0. */
export async function call(method, path, body) {
  if (!apiPath(path)) throw new Error('Burrow only talks to this blyg’s own /api/');
  const headers = { accept: 'application/json' };
  if (body !== undefined && body !== null) headers['content-type'] = 'application/json';
  try {
    const res = await fetch(path, {
      method, headers, credentials: 'same-origin', cache: 'no-store', redirect: 'error', mode: 'same-origin',
      body: body !== undefined && body !== null ? JSON.stringify(body) : undefined,
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

/** A whole cursor-paged list (restarted on cursor-expired, never partial). */
async function readAll(path, query = '', limit = 100) {
  const p = new Pager(path, query, limit);
  try {
    for (let next = p.next_path(); next !== undefined; next = p.next_path()) {
      const r = await call('GET', next);
      const err = JSON.parse(p.feed(r.status, r.body, r.retryAfter));
      if (err) return { error: err };
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
  }
  S.B = Burrow.new_live(now(), await load('live'), online ? base : '');
  if (!online) S.B.set_offline(true);
  hooks();
}

let lastRefresh = 0;
/** Send what waits, then read the posts and the reading list again. */
export async function refresh() {
  if (!S.B?.is_live_mode()) return;
  await flush();
  if (S.B.is_offline()) return;
  const items = await readAll('/api/items');
  if (items.error) { problem(items.error); return; }
  S.B.live_items(items.json);
  const rows = await readAll('/api/reading/imported');
  const hops = await readAll('/api/hoppers');
  if (!rows.error || rows.error.code === 'not-found') {
    S.B.live_reading(rows.error ? '[]' : rows.json, hops.error ? '[]' : hops.json);
  } else problem(rows.error);
  lastRefresh = Date.now();
  await persist({ quiet: true });
  S.render();
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
    if (step.refresh) refreshAfter = true;
    if (step.error) {
      if (step.stop) { problem(step.error); failed = true; break; }
      skip.push(`${op.op}:${op.id}`);
      toast(esc(step.error.text), 4200);
      failed = true;
    }
    S.render();
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
  await refresh();
}

// ---------------------------------------------------------------- publish, restore
/** Publish `o.id` on the blyg (user-initiated only: the publish sheet or
    capture's Publish). Returns an Outcome, as the demo's publish does. */
export async function publish(o) {
  await flush();
  const pr = JSON.parse(S.B.live_publish_request(o.id, o.note ?? ''));
  if (pr.error) return { action: 'toast', message: pr.error };
  const r = await call(pr.req.method, pr.req.path, pr.req.body);
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
  return res.error ? { action: 'toast', message: res.error.text } : res;
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
  document.addEventListener('visibilitychange', () => {
    if (document.visibilityState !== 'visible' || !S.B?.is_live_mode()) return;
    if (!$('#signin').hidden || S.B.is_offline()) backOnline();
    else if (Date.now() - lastRefresh > 60_000) refresh();
  });
  // Offline: try again now and then (navigator.onLine isn't always right).
  setInterval(() => { if (S.B?.is_live_mode() && S.B.is_offline()) backOnline(); }, 20_000);
}

/** The sign-in panel's own buttons. */
export function initSignIn(retry) {
  $('#signin-retry').addEventListener('click', retry);
}
