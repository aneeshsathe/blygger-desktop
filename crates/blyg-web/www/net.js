// The page's one door to the network: every owner-API request goes through
// `call`, which refuses anything that isn't a same-origin /api/ path. The
// Studio session cookie (HttpOnly, so this page never sees it) is the only
// credential: there's no token, nothing is stored, and nothing is logged.
//
// index.html loads this module first, before the app and its WebAssembly,
// so the reads every live start makes go out at once instead of one after
// another once the wasm is running: the capabilities and settings and, on a
// first start (nothing held in this browser yet), the first page of each
// list. (Contract 6 has no `/api/changes`: lists are read conditionally,
// `If-None-Match` and a bodyless 304, src/live.rs `cond_answer`.) `call` hands each early answer to the first
// request for exactly that path, once, within a few seconds; the first
// refresh drops whatever is left (live.js `refresh`). Nothing here imports
// anything, so it runs as soon as it arrives.

const API = '/api/';

/** The blyg's mount: the app is served at {mount}/burrow/, the owner API at
    {mount}/api and the Studio at {mount}/studio. Empty for a blyg at the
    host root. Paths here (and from the Rust) are mount-relative `/api/…`;
    there is no host-root /api on a path-mounted blyg (studio 0.65). The
    rule is src/api.rs `mount_of`, copied because this runs before the
    WebAssembly; live.js checks the two agree. A mount is empty or plain
    segments (no empty or dot segment, no backslash, nothing
    percent-encoded): `//evil.example` would make the sign-in link and SITE
    another host. Anything else is `null`, and the page is unsupported. */
const MOUNT_RE = /^(?:\/(?!\.\.?(?:\/|$))[A-Za-z0-9._~-]+)*$/;
function mountOf(p) {
  const i = p.lastIndexOf('/burrow/');
  const m = i >= 0 ? p.slice(0, i) : '';
  return MOUNT_RE.test(m) ? m : null;
}
export const MOUNT = mountOf(location.pathname);

/** The WebAssembly, asked for as soon as this first module runs, for
    app.js to hand to the wasm's init. (A `<link rel="preload" as="fetch">`
    did this before, and WebKit fetched it a second time: 4.4 MB twice on
    every open on an iPhone.) */
export const WASM = fetch(new URL('pkg/blyg_web_bg.wasm', import.meta.url));
WASM.catch(() => {}); // a failure is app.js's to report, through init
/** The blyg's own address (origin and mount): the base its links are built
    on when its settings name no siteUrl. */
export const SITE = location.origin + (MOUNT ?? '');

/** Whether `path` is a same-origin owner-API path (and nothing else): no
    dot segments or backslashes, encoded or not, in the path (an encoded
    origin in the query is ordinary), and none at all without a mount. */
export function apiPath(path) {
  return MOUNT !== null && typeof path === 'string' && path.startsWith(API) && !/\/\.\.?(\/|$)|\\|\/\/|^[a-z]+:/i.test(path) &&
    !/%2e|%2f|%5c/i.test(path.split('?')[0]);
}

async function send(method, path, body, etag) {
  const headers = { accept: 'application/json' };
  // A conditional read: a 304 comes back bodyless, and the Rust hands back
  // the body it kept (`cond_answer`). `no-store` keeps the browser's own
  // cache out of it, so the 304 reaches the page as it came.
  if (etag) headers['if-none-match'] = etag;
  // --- web-writing --- A FormData body (an image upload) goes as multipart.
  const form = typeof FormData !== 'undefined' && body instanceof FormData;
  // --- end web-writing ---
  if (body !== undefined && body !== null && !form) headers['content-type'] = 'application/json';
  try {
    const res = await fetch(MOUNT + path, {
      method, headers, credentials: 'same-origin', cache: 'no-store', redirect: 'error', mode: 'same-origin',
      body: form ? body : body !== undefined && body !== null ? JSON.stringify(body) : undefined,
    });
    return { status: res.status, body: await res.text(), retryAfter: res.headers.get('retry-after') ?? undefined, www: res.headers.has('www-authenticate'), etag: res.headers.get('etag') ?? '' };
  } catch {
    return { status: 0, body: '', retryAfter: undefined, www: false, etag: '' };
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
  let held = false;
  try { held = localStorage.getItem(`burrow-web.${HELD}`) === '1'; } catch { /* no storage: read them */ }
  if (!held) for (const p of FIRST_PAGES) prefetch(p);
}

/** One owner-API request. Never throws: no answer is status 0. `etag`
    (a GET's) asks conditionally. */
export async function call(method, path, body, etag) {
  if (!apiPath(path)) throw new Error('Burrow only talks to this blyg’s own /api/');
  const e = method === 'GET' && !etag ? early.get(path) : undefined;
  if (e) {
    early.delete(path);
    if (Date.now() - e.at < EARLY_MS) return e.answer;
  }
  return send(method, path, body, method === 'GET' ? etag : undefined);
}
