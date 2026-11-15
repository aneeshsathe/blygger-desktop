// The page's one door to the network: every owner-API request goes through
// `call`, which refuses anything that isn't a same-origin /api/ path. The
// Studio session cookie (HttpOnly, so this page never sees it) is the only
// credential: there's no token, nothing is stored, and nothing is logged.
//
// index.html loads this module first, before the app and its WebAssembly,
// so the reads every live start makes go out at once instead of one after
// another once the wasm is running: the capabilities and settings, the
// change counters and, on a first start (nothing held in this browser yet),
// the first page of each list. `call` hands each early answer to the first
// request for exactly that path, once, within a few seconds; the first
// refresh drops whatever is left (live.js `refresh`). Nothing here imports
// anything, so it runs as soon as it arrives.

const API = '/api/';

/** The blyg's mount: the app is served at {mount}/burrow/, the owner API at
    {mount}/api and the Studio at {mount}/studio. Empty for a blyg at the
    host root. Paths here (and from the Rust) are mount-relative `/api/…`. */
export const MOUNT = (() => {
  const p = location.pathname;
  const i = p.lastIndexOf('/burrow/');
  return i > 0 ? p.slice(0, i) : '';
})();

/** Whether `path` is a same-origin owner-API path (and nothing else). */
export function apiPath(path) {
  return typeof path === 'string' && path.startsWith(API) && !/\/\.\.?(\/|$)|\\|\/\/|^[a-z]+:/i.test(path);
}

async function send(method, path, body) {
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

// ---------------------------------------------------------------- early reads
const EARLY_MS = 10_000; // an early answer older than this is never used
const early = new Map(); // path -> { at, answer: Promise }

/** Start reading `path` now, for the first `call('GET', path)` to take. */
function prefetch(path) {
  if (apiPath(path) && !early.has(path)) early.set(path, { at: Date.now(), answer: send('GET', path) });
}

/** Forget the early reads nobody took (after the first refresh, or when
    the page isn't live). */
export function dropEarly() {
  early.clear();
}

/** The first page of each list a first start reads. These must be the
    paths `Pager` (src/api.rs) asks for first, or they're never taken:
    scripts/browser-web-perf-check.sh checks that no path is read twice. */
export const FIRST_PAGES = ['/api/items?limit=100', '/api/reading/imported?limit=100', '/api/hoppers?limit=100'];
/** Set once this browser holds the live reading list (core.js
    `persistReading`); from then on a start reads only what changed. */
export const HELD = 'live-held';

// Only where the app can be live: over http(s), served at …/burrow/.
if (/^https?:$/.test(location.protocol) && location.pathname.lastIndexOf('/burrow/') >= 0) {
  prefetch('/api/capabilities');
  prefetch('/api/settings');
  prefetch('/api/changes');
  let held = false;
  try { held = localStorage.getItem(`burrow-web.${HELD}`) === '1'; } catch { /* no storage: read them */ }
  if (!held) for (const p of FIRST_PAGES) prefetch(p);
}

/** One owner-API request. Never throws: no answer is status 0. */
export async function call(method, path, body) {
  if (!apiPath(path)) throw new Error('Burrow only talks to this blyg’s own /api/');
  const e = method === 'GET' ? early.get(path) : undefined;
  if (e) {
    early.delete(path);
    if (Date.now() - e.at < EARLY_MS) return e.answer;
  }
  return send(method, path, body);
}
