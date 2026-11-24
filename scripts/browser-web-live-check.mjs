// Burrow web's live updates against a REAL local studio (run by
// scripts/browser-web-live-check.sh, which starts the studios and the dev
// proxy and says where everything is). Local only: every URL must be a
// loopback address. Tokens and the password are read from files and never
// printed.
//
// 1. Seeds `a` (the blyg under test: three own posts, a hopper) and `b`
//    (three posts), and subscribes `a` to `b`.
// 2. Opens Burrow web on `a` through dev-proxy.py in headless Chromium,
//    signed in with a Studio session, and leaves it alone.
// 3. Idle: counts the page's /api/ requests over 60 s with nothing
//    changing. The budgets come from the page's own refresh interval
//    (`Burrow.refresh_every`, src/live.rs `Pace::every`): on a studio with
//    `/api/changes` (15 s), at most 5 requests a minute; without it
//    (contract 6, 60 s), at most one conditional read per list per
//    interval, every one answered 304.
// 4. Makes changes through the API, as another client would, and times
//    each one until it's on screen with no interaction (budget: 20 s with
//    `/api/changes`, else one interval and 15 s): a
//    new own post, a republish, a withdrawal, a new subscription post, a
//    hopper change and a thumb; with studio 0.61, a post shelved elsewhere
//    (in the Shelf lens) and one taken off the shelf here (on the blyg).
import fs from 'node:fs';
import { createRequire } from 'node:module';

const env = (k) => {
  const v = process.env[k];
  if (!v) throw new Error(`${k} is not set`);
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

// With /api/changes (the 15 s poll); without it, from the page's interval.
const CHANGES_BUDGET_MS = 20_000;
const CHANGES_IDLE_MAX = 5;
const MARGIN_MS = 15_000;
const IDLE_MS = 60_000;
let BUDGET_MS = CHANGES_BUDGET_MS;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

function client(base, tokenFile) {
  base = local(base);
  const tok = secret(tokenFile);
  async function call(method, path, body) {
    for (let attempt = 0; attempt < 6; attempt++) {
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
  async function all(path) {
    const out = [];
    let cursor = null;
    do {
      const sep = path.includes('?') ? '&' : '?';
      const page = await call('GET', `${path}${sep}limit=100${cursor ? `&cursor=${encodeURIComponent(cursor)}` : ''}`);
      out.push(...(page.items ?? []));
      cursor = page.nextCursor;
    } while (cursor);
    return out;
  }
  async function publish(id, revision) {
    return call('POST', `/api/items/${id}/publish`, { expectedRevision: revision });
  }
  async function post(md) {
    const it = await call('POST', '/api/items', { mode: 'blank', contentMd: md, kind: 'fragment' });
    await publish(it.id, it.revision);
    return it.id;
  }
  return { base, call, all, publish, post };
}

const a = client(env('LIVE_URL_A'), 'LIVE_TOKEN_A_FILE');
const b = client(env('LIVE_URL_B'), 'LIVE_TOKEN_B_FILE');

// ------------------------------------------------------------ seed
// Invented sample content only. Distinct seconds keep the order stable.
// Posted a second apart, b's posts would each be `superseded` by the next
// (studio 0.69's Background rule: its blyg followed it within 15 min) and
// leave every list but Background, the Thumbed count among them. The check
// is about updates reaching the lists, so that rule is off on `a`.
await a.call('PATCH', '/api/settings', { backgroundRules: { superseded: false } });
for (let i = 1; i <= 3; i++) { await b.post(`Sample note B0${i}: invented notes about tide pools.`); await sleep(1100); }
for (let i = 1; i <= 3; i++) { await a.post(`Own post A0${i}: invented notes on example topics.`); await sleep(1100); }
// For the preview under the page's Content-Security-Policy: a bare video
// link (the preview's own script turns it into a poster) and a formula.
await a.post('Own post A05 with a video and $e^x$:\n\nhttps://www.youtube.com/watch?v=Qa1b2C3d4E5');
let sub = await a.call('POST', '/api/subscriptions', { url: b.base });
if (sub?.needsConfirm) sub = await a.call('POST', '/api/subscriptions', { url: b.base, confirm: true });
const subs = await a.all('/api/subscriptions');
const subId = subs[0]?.id;
if (!subId) throw new Error('subscribing a to b failed');
let imported = [];
for (let t = 0; t < 60 && imported.length < 3; t++) {
  imported = await a.all('/api/reading/imported');
  if (imported.length < 3) { if (t % 10 === 9) await a.call('POST', `/api/subscriptions/${subId}/resync`); await sleep(1000); }
}
if (imported.length < 3) throw new Error(`a imported ${imported.length} of b's 3 posts`);
const rid = (t) => imported.find((r) => (r.contentMd ?? '').includes(t))?.remoteId;
const hopper = await a.call('POST', '/api/hoppers', { name: 'Kelp hopper' });

// ------------------------------------------------------------ open
const browser = await chromium.launch();
let failed = 0;
try {
  const ctx = await browser.newContext({ viewport: { width: 1280, height: 900 } });
  const login = await ctx.request.post(`${ORIGIN}/studio/login`, {
    form: { password: secret('LIVE_PASSWORD_FILE') }, headers: { origin: ORIGIN }, maxRedirects: 0,
  });
  if (login.status() >= 400) throw new Error(`Studio sign-in failed (${login.status()})`);
  const page = await ctx.newPage();
  const api = [];
  page.on('request', (req) => {
    const u = new URL(req.url());
    if (u.pathname.startsWith('/api/')) api.push({ at: Date.now(), what: `${req.method()} ${u.pathname}` });
  });
  const answers = [];
  page.on('response', (res) => {
    const u = new URL(res.url());
    if (u.pathname.startsWith('/api/')) answers.push({ at: Date.now(), what: `${res.request().method()} ${u.pathname}`, status: res.status() });
  });
  page.on('pageerror', (e) => { console.log(`page error: ${e.message}`); failed++; });
  // Anything the policy refuses, in the page or a frame, is a failure.
  page.on('console', (m) => {
    if (/Content.Security.Policy/i.test(m.text())) { console.log(`FAIL: CSP: ${m.text()}`); failed++; }
  });
  await page.goto(`${ORIGIN}/burrow/`);
  await page.waitForSelector('#list .row', { timeout: 30_000 });
  await sleep(5000); // the first refresh, and the first scheduled check

  // Which cadence the page runs: the studio's /api/changes, or conditional
  // reads (contract 6). Asked of the page's own WebAssembly, the module
  // app.js already started.
  const changes = (await fetch(`${a.base}/api/changes`, { headers: { authorization: `Bearer ${secret('LIVE_TOKEN_A_FILE')}` } })).status === 200;
  const every = await page.evaluate(async (c) => {
    const m = await import(new URL('pkg/blyg_web.js', location.href).href);
    return m.Burrow.refresh_every(c);
  }, changes);
  BUDGET_MS = changes ? CHANGES_BUDGET_MS : every + MARGIN_MS;
  console.log(`cadence: ${changes ? '/api/changes' : 'conditional reads'}, every ${every / 1000} s; on screen within ${BUDGET_MS / 1000} s`);

  const rows = () => page.$$eval('#list .row', (cs) => cs.map((c) => c.innerText.replace(/\s+/g, ' ')));
  const cards = () => page.$$eval('#d-stream .card', (cs) => cs.map((c) => c.innerText.replace(/\s+/g, ' ')));
  const srcText = (key) => page.$eval(`[data-src="${key}"]`, (e) => e.innerText.replace(/\s+/g, ' ')).catch(() => '');

  /** Make a change, then time it until `seen()` holds (no interaction). */
  async function timed(name, change, seen) {
    await change();
    const t0 = Date.now();
    const from = api.length;
    while (Date.now() - t0 < BUDGET_MS) {
      if (await seen()) {
        const ms = Date.now() - t0;
        const reads = [...new Set(api.slice(from).map((r) => r.what.replace(/\/api\/hoppers\/[^/]+\//, '/api/hoppers/{id}/')))];
        console.log(`ok: ${name}: on screen after ${(ms / 1000).toFixed(1)} s (requests: ${reads.join(', ')})`);
        return ms;
      }
      await sleep(250);
    }
    console.log(`FAIL: ${name}: not on screen after ${BUDGET_MS / 1000} s`);
    failed++;
    return null;
  }

  // ---------------------------------------------------------- idle
  const idleFrom = api.length;
  const idleAnswers = answers.length;
  await sleep(IDLE_MS);
  const idle = api.slice(idleFrom);
  const idleList = [...new Set(idle.map((r) => r.what))].join(', ') || 'none';
  if (changes) {
    const perMin = idle.length * (60_000 / IDLE_MS);
    console.log(`${perMin <= CHANGES_IDLE_MAX ? 'ok' : 'FAIL'}: idle: ${idle.length} requests in ${IDLE_MS / 1000} s (${idleList})`);
    if (perMin > CHANGES_IDLE_MAX) failed++;
  } else {
    // Each list at most once per interval, and nothing changed: all 304,
    // but for a list's first read in this page (nothing kept to ask with:
    // the hoppers' and thumbs' own reads wait for a refresh that doesn't
    // read the whole reading list).
    const each = Math.ceil(IDLE_MS / every);
    const counts = {};
    for (const r of idle) counts[r.what] = (counts[r.what] ?? 0) + 1;
    const over = Object.entries(counts).filter(([, n]) => n > each);
    const before = new Set(answers.slice(0, idleAnswers).map((r) => r.what));
    const whole = answers.slice(idleAnswers).filter((r, i, all) =>
      r.status !== 304 && (before.has(r.what) || all.slice(0, i).some((x) => x.what === r.what)));
    const ok = over.length === 0 && whole.length === 0;
    console.log(`${ok ? 'ok' : 'FAIL'}: idle: ${idle.length} requests in ${IDLE_MS / 1000} s, at most ${each} per list (${idleList})` +
      (over.length ? `; asked more often: ${over.map(([w, n]) => `${w} ×${n}`).join(', ')}` : '') +
      (whole.length ? `; not 304: ${whole.map((r) => `${r.what} ${r.status}`).join(', ')}` : ''));
    if (!ok) failed++;
  }

  // ---------------------------------------------------------- posts
  const items = await a.all('/api/items');
  const find = (t) => items.find((i) => i.contentMd.includes(t));
  const times = {};
  times.newOwn = await timed('a new own post', () => a.post('Own post A04 NEW from another client.'),
    async () => (await rows()).some((r) => r.includes('A04')));
  const a01 = find('Own post A01');
  times.republish = await timed('a republish', async () => {
    const ed = await a.call('PATCH', `/api/items/${a01.id}`, { expectedRevision: a01.revision, contentMd: 'Own post A01 EDITED elsewhere: invented notes.' });
    await a.publish(a01.id, ed.revision);
  }, async () => (await rows()).some((r) => r.includes('A01 EDITED')));
  const a02 = find('Own post A02');
  times.withdraw = await timed('a withdrawal',
    () => a.call('POST', `/api/items/${a02.id}/withdraw`, { expectedRevision: a02.revision }),
    async () => (await rows()).some((r) => r.includes('A02') && /withdrawn/i.test(r)));

  // ---------------------------------------------------------- preview
  // The preview is a srcdoc frame: it inherits the page's policy on top of
  // its own, so its script must be allowed by both. It ran when the bare
  // video link became the poster.
  await page.click('#list .row:has-text("A05")');
  await page.click('[data-pa="preview"]');
  const poster = await page.frameLocator('#pv').locator('figure.blyg-yt .blyg-yt-poster')
    .waitFor({ timeout: 10_000 }).then(() => true, () => false);
  const formula = await page.frameLocator('#pv').locator('math').count().catch(() => 0);
  if (poster && formula > 0) console.log('ok: the preview\'s script runs under the page\'s policy (video poster, formula drawn)');
  else { console.log(`FAIL: preview: poster ${poster}, formulas ${formula}`); failed++; }
  await page.click('[data-pa="preview"]');

  // ---------------------------------------------------------- reading
  await page.click('.views [data-view="reading"]');
  await page.waitForSelector('#d-stream .card', { timeout: 10_000 });
  await sleep(3000);
  times.subscription = await timed('a new subscription post', async () => {
    await b.post('Sample note B04 NEW, published after the page opened.');
    await a.call('POST', `/api/subscriptions/${subId}/resync`);
  }, async () => (await cards()).some((c) => c.includes('B04')));
  times.hopper = await timed('a hopper change',
    () => a.call('PUT', `/api/hoppers/${encodeURIComponent(hopper.id)}/items/${encodeURIComponent(subId)}/${encodeURIComponent(rid('B01'))}`),
    async () => /1 post here/.test(await srcText(`hopper:${hopper.id}`)));
  times.thumb = await timed('a thumb', () => a.call('PUT', `/api/signals/${encodeURIComponent(subId)}/${encodeURIComponent(rid('B02'))}`, { thumb: 1 }),
    async () => /\b1\b/.test(await srcText('smart:thumbed')));
  // --- shelf sync --- (studio 0.61: getShelf/putShelf). The Shelf lens
  // open, a post shelved elsewhere shows; taking it off here reaches the blyg.
  const caps = await a.call('GET', '/api/capabilities');
  if ((caps.operations ?? []).includes('putShelf')) {
    await page.click('[data-src="smart:shelf"]');
    await sleep(1000);
    times.shelf = await timed('a post shelved elsewhere', async () => {
      const cur = await a.call('GET', '/api/shelf');
      await a.call('PUT', '/api/shelf', { entries: [...cur.entries, { sub: subId, remoteId: rid('B03') }], expectedRevision: cur.revision });
    }, async () => (await cards()).some((c) => c.includes('B03')));
    await page.click('#d-stream .card .ex');
    // The shelf button sits behind ⋯ More (since the chips row was split).
    await page.click('#d-moreRA');
    await page.waitForSelector('#d-shelfRA', { state: 'visible', timeout: 10_000 });
    const label = await page.$eval('#d-shelfRA', (e) => e.textContent);
    if (!/Remove from shelf/.test(label)) { console.log(`FAIL: the open post's shelf button says "${label}"`); failed++; }
    times.shelfHere = await timed('a post taken off the shelf here, on the blyg',
      () => page.click('#d-shelfRA'),
      async () => !(await a.call('GET', '/api/shelf')).entries.some((e) => e.remoteId === rid('B03')));
  } else {
    console.log('skip: shelf (the studio has no putShelf)');
  }
  const got = Object.values(times).filter((t) => t != null);
  if (got.length) console.log(`change → screen: max ${(Math.max(...got) / 1000).toFixed(1)} s, mean ${(got.reduce((x, y) => x + y, 0) / got.length / 1000).toFixed(1)} s`);
} finally {
  await browser.close();
}
console.log(failed ? `${failed} check(s) failed` : 'all checks passed');
process.exit(failed ? 1 : 0);
