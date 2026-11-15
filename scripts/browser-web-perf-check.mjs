// Burrow web's start-up and scroll cost against a REAL local studio with a
// large reading list (run by scripts/browser-web-perf-check.sh, which starts
// the studios and the dev proxy and says where everything is). Local only:
// every URL must be a loopback address. Tokens and the password are read
// from files and never printed.
//
// 1. Seeds `b` with PERF_POSTS invented posts (default 3000) and `a` (the
//    blyg under test) with 16 own posts, subscribes `a` to `b` and waits
//    until `a` holds them all. A seed that's already there is kept.
// 2. Cold start, in a fresh browser profile signed in with a Studio
//    session: time to the first own-posts list, then to Reading shown (more
//    than 50 cards); requests and bytes until it settles. The first screen
//    must already be in its final order: the cards shown first are the
//    cards at the top once every page is in.
// 3. Scroll: 60 wheel steps of 600 px over the stream, counting long tasks.
// 4. Warm start: a reload in the same profile.
// 5. With PERF_STUDIO=1, the same cold and warm numbers for the studio's
//    own Reading page, for comparison (reported, not budgeted).
//
// The budgets are loose, so a slow machine doesn't flake; the numbers are
// printed either way.
import fs from 'node:fs';
import { createRequire } from 'node:module';

const env = (k, d) => {
  const v = process.env[k] ?? d;
  if (v === undefined) throw new Error(`${k} is not set`);
  return v;
};
const local = (u) => {
  if (!/^http:\/\/(127\.0\.0\.1|localhost):\d+$/.test(u)) throw new Error(`not a local URL: ${u}`);
  return u;
};
const ORIGIN = local(env('LIVE_ORIGIN'));
const require = createRequire(`${env('PLAYWRIGHT_DIR')}/`);
const { chromium } = require('playwright');
const secret = (k) => fs.readFileSync(env(k), 'utf8').trim();
const N = Number(env('PERF_POSTS', '3000'));
const OWN = 16;
const VERBOSE = env('PERF_VERBOSE', '') === '1';
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

// Budgets (ms, counts) on localhost, at 3k posts.
const BUDGET = {
  coldPosts: 1500,
  coldReading: 1200,
  // Besides the reading list's pages (100 posts each, read one after
  // another behind the first screen): the app's files and the other reads.
  coldRequests: 30,
  scrollLongTasks: 0,
  warmReading: 600,
};

function client(base, tokenFile) {
  base = local(base);
  const tok = secret(tokenFile);
  async function call(method, path, body) {
    for (let attempt = 0; attempt < 8; attempt++) {
      const res = await fetch(base + path, {
        method,
        headers: { authorization: `Bearer ${tok}`, accept: 'application/json', ...(body ? { 'content-type': 'application/json' } : {}) },
        body: body ? JSON.stringify(body) : undefined,
      });
      const text = await res.text();
      if (res.status === 429) { await sleep(1000 * (attempt + 1)); continue; }
      if (res.status >= 400) throw new Error(`${method} ${path} -> ${res.status} ${text.slice(0, 200)}`);
      return text ? JSON.parse(text) : null;
    }
    throw new Error(`${method} ${path}: still rate-limited`);
  }
  async function count(path) {
    let n = 0;
    let cursor = null;
    do {
      const page = await call('GET', `${path}?limit=100${cursor ? `&cursor=${encodeURIComponent(cursor)}` : ''}`);
      n += (page.items ?? []).length;
      cursor = page.page?.nextCursor ?? page.nextCursor;
    } while (cursor);
    return n;
  }
  async function post(md) {
    const it = await call('POST', '/api/items', { mode: 'blank', contentMd: md, kind: 'fragment' });
    await call('POST', `/api/items/${it.id}/publish`, { expectedRevision: it.revision });
  }
  return { base, call, count, post };
}

const a = client(env('LIVE_URL_A'), 'LIVE_TOKEN_A_FILE');
const b = client(env('LIVE_URL_B'), 'LIVE_TOKEN_B_FILE');

// ------------------------------------------------------------ seed
// Invented sample content only.
const words = ['moss', 'kettle', 'harbour', 'lantern', 'orchard', 'pebble', 'meadow', 'teapot', 'willow', 'gravel'];
async function seed() {
  const t0 = Date.now();
  const have = await b.count('/api/items');
  if (have < N) {
    console.log(`seeding ${N - have} posts on b…`);
    let next = have;
    const worker = async () => {
      for (let i = next++; i < N; i = next++) {
        await b.post(`Bulk note ${String(i).padStart(4, '0')} about the ${words[i % 10]}. Invented sample text for a load test on an example blyg, with a few more words so the excerpt has something to show.`);
      }
    };
    await Promise.all(Array.from({ length: 6 }, worker));
  }
  const own = await a.count('/api/items');
  for (let i = own + 1; i <= OWN; i++) await a.post(`Own post A${String(i).padStart(2, '0')}: invented notes on example topics.`);
  let subs = (await a.call('GET', '/api/subscriptions?limit=100')).items ?? [];
  if (!subs.length) {
    let sub = await a.call('POST', '/api/subscriptions', { url: b.base });
    if (sub?.needsConfirm) sub = await a.call('POST', '/api/subscriptions', { url: b.base, confirm: true });
    subs = (await a.call('GET', '/api/subscriptions?limit=100')).items ?? [];
  }
  const subId = subs[0]?.id;
  if (!subId) throw new Error('subscribing a to b failed');
  let held = await a.count('/api/reading/imported');
  for (let t = 0; held < N && t < 200; t++) {
    await a.call('POST', `/api/subscriptions/${subId}/resync`);
    held = await a.count('/api/reading/imported');
    if (t % 10 === 9) console.log(`  a holds ${held} of ${N}…`);
  }
  if (held < N) throw new Error(`a imported ${held} of b's ${N} posts`);
  console.log(`seed: ${held} reading posts, ${OWN} own posts (${Math.round((Date.now() - t0) / 1000)} s)`);
}

// ------------------------------------------------------------ measure
const longTasks = () => {
  window.__lt = [];
  new PerformanceObserver((l) => { for (const e of l.getEntries()) window.__lt.push(e.duration); }).observe({ type: 'longtask', buffered: true });
};

// One Studio sign-in for the whole run (the studio budgets password
// sign-ins), shared by every fresh profile as its cookie. With a kept state
// (BLYG_PERF_STATE), the session is kept beside it for the next run.
let session;
async function signIn(browser) {
  const ctx = await browser.newContext();
  const kept = process.env.LIVE_SESSION_FILE;
  try {
    if (kept && fs.existsSync(kept)) {
      const cookies = JSON.parse(fs.readFileSync(kept, 'utf8'));
      await ctx.addCookies(cookies);
      const r = await ctx.request.get(`${ORIGIN}/api/capabilities`, { headers: { accept: 'application/json' } });
      if (r.status() === 200) { session = { cookies, origins: [] }; return; }
      await ctx.clearCookies();
    }
    for (let attempt = 0; attempt < 6; attempt++) {
      const r = await ctx.request.post(`${ORIGIN}/studio/login`, {
        form: { password: secret('LIVE_PASSWORD_FILE') }, headers: { origin: ORIGIN }, maxRedirects: 0,
      });
      if (r.status() === 429) { await sleep(1000 * Number(r.headers()['retry-after'] || 10)); continue; }
      if (r.status() >= 400) throw new Error(`Studio sign-in failed (${r.status()})`);
      session = { cookies: (await ctx.storageState()).cookies, origins: [] };
      if (kept) fs.writeFileSync(kept, JSON.stringify(session.cookies), { mode: 0o600 });
      return;
    }
    throw new Error('Studio sign-in: still rate-limited');
  } finally {
    await ctx.close();
  }
}

async function open(browser) {
  const ctx = await browser.newContext({ viewport: { width: 1280, height: 900 }, storageState: session });
  await ctx.addInitScript(longTasks);
  const page = await ctx.newPage();
  const net = { n: 0, bytes: 0, wire: 0, api: [], all: [], reset() { this.n = 0; this.bytes = 0; this.wire = 0; this.api = []; this.all = []; } };
  page.on('requestfinished', async (req) => {
    net.n++;
    const res = await req.response();
    try { net.bytes += (await res.body()).length; } catch { /* no body */ }
    try { net.wire += (await req.sizes()).responseBodySize; } catch { /* gone */ }
    const u = new URL(req.url());
    if (u.pathname.startsWith('/api/')) net.api.push(`${req.method()} ${u.pathname}${u.search}`);
    net.all.push(`${req.method()} ${u.pathname}${u.search.slice(0, 40)} ${res.status()}${res.fromServiceWorker() ? ' (sw)' : ''}`);
  });
  page.on('pageerror', (e) => { console.log(`page error: ${e.message}`); errors++; });
  return { ctx, page, net };
}

async function scrollJank(page, sel) {
  await page.evaluate(() => { window.__lt = []; });
  const box = await page.$eval(sel, (el) => { const r = el.getBoundingClientRect(); return { x: r.x + r.width / 2, y: r.y + r.height / 2 }; });
  await page.mouse.move(box.x, box.y);
  for (let i = 0; i < 60; i++) { await page.mouse.wheel(0, 600); await page.waitForTimeout(16); }
  await page.waitForTimeout(300);
  const lt = await page.evaluate(() => window.__lt);
  return { longTasks: lt.length, longest: Math.round(Math.max(0, ...lt)) };
}

const topCards = (page, n) => page.$$eval('#d-stream .card', (cs, n) => cs.slice(0, n).map((c) => c.dataset.key), n);
const readingShown = () => document.querySelectorAll('#d-stream .card').length > 50;

let errors = 0;
const results = [];
function check(name, value, budget, unit = 'ms') {
  const ok = value <= budget;
  console.log(`${ok ? 'ok' : 'FAIL'}: ${name}: ${value} ${unit} (budget ${budget})`);
  if (!ok) errors++;
  results.push({ name, value });
}

async function burrow(browser) {
  const { ctx, page, net } = await open(browser);
  let t0 = Date.now();
  await page.goto(`${ORIGIN}/burrow/`);
  await page.waitForSelector('#list .row', { timeout: 60_000 });
  const coldPosts = Date.now() - t0;
  await page.click('.views [data-view="reading"]');
  await page.waitForFunction(readingShown, null, { timeout: 60_000 });
  const coldReading = Date.now() - t0;
  const first = await topCards(page, 12);
  await page.waitForFunction(() => /more…/.test(document.querySelector('#d-stream').innerText), null, { timeout: 60_000 }).catch(() => {});
  await page.waitForTimeout(3000);
  const settled = await topCards(page, 12);
  check('cold: first own-posts list', coldPosts, BUDGET.coldPosts);
  check('cold: Reading shown', coldReading, BUDGET.coldReading);
  const pages = net.api.filter((x) => x.startsWith('GET /api/reading/imported')).length;
  check('cold: requests besides the reading pages', net.n - pages, BUDGET.coldRequests, `requests (+ ${pages} reading pages)`);
  console.log(`     cold: ${(net.bytes / 1e6).toFixed(2)} MB decoded, ${(net.wire / 1e6).toFixed(2)} MB on the wire, ${net.api.length} API calls`);
  if (VERBOSE) {
    const w = await page.evaluate(() => performance.getEntriesByType('resource').map((e) => [Math.round(e.startTime), Math.round(e.responseEnd), new URL(e.name).pathname + new URL(e.name).search.slice(0, 12)]));
    console.log(w.map(([s, e, n]) => `       ${String(s).padStart(5)} ${String(e).padStart(5)}  ${n}`).join('\n'));
  }
  const dup = [...new Set(net.api.filter((x, i) => net.api.indexOf(x) !== i))];
  console.log(`${dup.length ? 'FAIL' : 'ok'}: cold: no owner-API read twice${dup.length ? ` (${dup.join(', ')})` : ''}`);
  if (dup.length) errors++;
  const same = JSON.stringify(first) === JSON.stringify(settled);
  console.log(`${same ? 'ok' : 'FAIL'}: the first screen is in its final order (${first.length} cards)`);
  if (!same) { errors++; console.log(`     first:   ${first.join(' ')}\n     settled: ${settled.join(' ')}`); }
  const sj = await scrollJank(page, '#d-stream');
  check('scroll: long tasks over 60 steps', sj.longTasks, BUDGET.scrollLongTasks, `(longest ${sj.longest} ms)`);

  net.reset();
  t0 = Date.now();
  await page.reload();
  await page.waitForSelector('#list .row', { timeout: 60_000 });
  const warmPosts = Date.now() - t0;
  await page.click('.views [data-view="reading"]');
  await page.waitForFunction(readingShown, null, { timeout: 60_000 });
  const warmReading = Date.now() - t0;
  await page.waitForTimeout(2000);
  console.log(`     warm: first list ${warmPosts} ms, ${net.n} requests, ${(net.bytes / 1e6).toFixed(2)} MB decoded, ${(net.wire / 1e6).toFixed(2)} MB on the wire, API: ${[...new Set(net.api.map((x) => x.split('?')[0]))].join(', ')}`);
  check('warm: Reading shown', warmReading, BUDGET.warmReading);
  await ctx.close();
}

async function studio(browser) {
  const { ctx, page, net } = await open(browser);
  let t0 = Date.now();
  await page.goto(`${ORIGIN}/studio/`);
  await page.waitForSelector('text=Own post A', { timeout: 60_000 });
  const fp = Date.now() - t0;
  await page.goto(`${ORIGIN}/studio/reading`);
  await page.waitForSelector('text=Bulk note', { timeout: 60_000 });
  const fr = Date.now() - t0;
  await page.waitForTimeout(3000);
  console.log(`studio: cold first own list ${fp} ms, Reading ${fr} ms (after the compose page), ${net.n} requests, ${(net.bytes / 1e6).toFixed(2)} MB decoded, ${(net.wire / 1e6).toFixed(2)} MB on the wire, ${net.api.length} API calls`);
  const sj = await scrollJank(page, 'body');
  console.log(`studio: scroll ${sj.longTasks} long tasks (longest ${sj.longest} ms)`);
  net.reset();
  t0 = Date.now();
  await page.goto(`${ORIGIN}/studio/reading`);
  await page.waitForSelector('text=Bulk note', { timeout: 60_000 });
  console.log(`studio: warm Reading ${Date.now() - t0} ms, ${net.n} requests, ${(net.bytes / 1e6).toFixed(2)} MB decoded`);
  await ctx.close();
}

await seed();
const browser = await chromium.launch();
try {
  await signIn(browser);
  const runs = Number(env('PERF_RUNS', '1'));
  for (let i = 0; i < runs; i++) await burrow(browser);
  if (env('PERF_STUDIO', '') === '1') await studio(browser);
} finally {
  await browser.close();
}
console.log(errors ? `${errors} check(s) failed` : 'all checks passed');
process.exit(errors ? 1 : 0);
