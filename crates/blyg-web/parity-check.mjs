// --- web-parity --- A live check of the studio-parity features on the web
// (mark all read, the Posts filters, Updates), in Chromium,
// against two DISPOSABLE local studios:
//
//   BURROW_PW=… BURROW_B=http://127.0.0.1:<port b> \
//     node crates/blyg-web/parity-check.mjs http://localhost:<proxy>/burrow/
//
// `url` is `dev-proxy.py` in front of studio `a` (scripts/e2e-wrangler.sh
// start a …; dev-proxy.py --upstream 127.0.0.1:<port a>), serving a fresh
// `serve.sh --build-only`. `BURROW_B` is studio `b`, which `a` subscribes
// to. BURROW_PW is their Studio password (e2e-wrangler.sh password): it
// signs in with it and never prints it, nor any token. It seeds invented
// posts ("Parity sample …") and refuses anything but a loopback address.
// Never point it at a real blyg.
//
// It needs Playwright with Chromium (`npm i playwright && npx playwright
// install chromium` in any scratch directory, then run with NODE_PATH set
// to that directory's node_modules).
import { createRequire } from 'module';

const require = createRequire(import.meta.url); // honours NODE_PATH
const { chromium } = require('playwright');

const url = process.argv[2];
const B = process.env.BURROW_B;
const PW = process.env.BURROW_PW;
const local = (u) => /^http:\/\/(localhost|127\.0\.0\.1|\[::1\]):\d+/.test(u || '');
if (!local(url) || !local(B) || !PW) {
  console.error('usage: BURROW_PW=… BURROW_B=http://127.0.0.1:<port> node parity-check.mjs http://localhost:<port>/burrow/ (loopback only)');
  process.exit(2);
}
const A = new URL(url).origin;

let failed = 0;
const check = (ok, what) => { console.log(`${ok ? 'ok  ' : 'FAIL'} ${what}`); if (!ok) failed++; };
const wait = (ms) => new Promise((r) => setTimeout(r, ms));

const browser = await chromium.launch();

/** A signed-in Studio session on `origin`: `api(method, path, body)`. */
async function session(origin) {
  const ctx = await browser.newContext({ viewport: { width: 1280, height: 900 } });
  for (let i = 0; ; i++) {
    const r = await ctx.request.post(`${origin}/studio/login`, { form: { password: PW }, headers: { origin }, maxRedirects: 0 });
    if (r.status() === 429 && i < 8) { await wait(1000 * (+(r.headers()['retry-after'] || 10))); continue; }
    if (r.status() >= 400) throw new Error(`sign-in at ${origin} failed: ${r.status()}`);
    break;
  }
  async function api(method, path, body) {
    for (let i = 0; i < 6; i++) {
      const r = await ctx.request.fetch(`${origin}${path}`, {
        method, headers: { origin, accept: 'application/json', ...(body ? { 'content-type': 'application/json' } : {}) },
        data: body ? JSON.stringify(body) : undefined,
      });
      if (r.status() === 429) { await wait(1000 * (+(r.headers()['retry-after'] || 2))); continue; }
      const text = await r.text();
      if (r.status() >= 400) throw new Error(`${method} ${path} → ${r.status()} ${text.slice(0, 200)}`);
      return text ? JSON.parse(text) : null;
    }
    throw new Error(`${method} ${path}: rate limited`);
  }
  const create = (md, kind = 'fragment') => api('POST', '/api/items', { mode: 'blank', contentMd: md, kind });
  const publish = async (it) => ({ ...it, ...(await api('POST', `/api/items/${it.id}/publish`, { expectedRevision: it.revision })) });
  return { ctx, api, create, publish };
}

const a = await session(A);
const b = await session(B);

// ------------------------------------------------------------ seed
// b: three posts to read; one gets a v2 later, under a quote of a's.
const bPosts = [];
for (const n of [1, 2, 3]) bPosts.push(await b.publish(await b.create(`Parity sample B${n}: invented notes about tides.`)));
// a: a draft, two published posts (one edited after), one withdrawn.
await a.create('Parity sample draft: never published.');
const pub1 = await a.publish(await a.create('Parity sample public: published once.'));
const pub2 = await a.publish(await a.create('Parity sample edited: published, then changed.'));
await a.api('PATCH', `/api/items/${pub2.id}`, { expectedRevision: pub2.revision, contentMd: 'Parity sample edited: published, then changed. More.' });
const gone = await a.publish(await a.create('Parity sample withdrawn: taken back.'));
await a.api('POST', `/api/items/${gone.id}/withdraw`, { expectedRevision: gone.revision });
void pub1;
const sub = await a.api('POST', '/api/subscriptions', { url: B, confirm: true });
const subId = sub?.subscription?.id ?? sub?.id;
let imported = [];
for (let i = 0; i < 40 && imported.length < 3; i++) {
  await wait(1000);
  imported = (await a.api('GET', '/api/reading/imported?limit=100')).items ?? [];
}
check(imported.length >= 3, `a imported b's posts (${imported.length})`);
// a thread quoting B1, then B1 gets a v2 on b and a reads b again: stale.
const quoted = bPosts[0].id;
await a.publish(await a.create(`Parity sample thread quoting B1.\n\n![[${quoted}]]`, 'thread'));
const b1 = await b.api('GET', `/api/items/${quoted}`);
await b.api('PATCH', `/api/items/${quoted}`, { expectedRevision: b1.revision, contentMd: 'Parity sample B1: invented notes about tides, revised.' });
await b.publish({ id: quoted, revision: (await b.api('GET', `/api/items/${quoted}`)).revision });
if (subId) await a.api('POST', `/api/subscriptions/${encodeURIComponent(subId)}/resync`);
let stale = [];
for (let i = 0; i < 20 && !stale.length; i++) { stale = (await a.api('GET', '/api/freshness')).items ?? []; if (!stale.length) await wait(1000); }
check(stale.length === 1, `a's /api/freshness lists the thread (${stale.length})`);

// ------------------------------------------------------------ the page
const page = await a.ctx.newPage();
const errors = [];
page.on('pageerror', (e) => errors.push(String(e)));
const logged = [];
page.on('console', (m) => logged.push(m.text()));
await page.goto(url);
await page.waitForSelector('#list .row', { timeout: 30_000 });
await page.waitForFunction(() => document.documentElement.dataset.mode === 'live');
await wait(1500);
const toastText = () => page.evaluate(() => document.querySelector('#toast')?.textContent || '');

// --- 1. mark all read
await page.click('.views [data-view="reading"]');
await page.waitForSelector('#d-stream .card', { timeout: 30_000 });
const unread = async () => +(await page.evaluate(() => document.querySelector('#d-unread').textContent || '0'));
check((await unread()) >= 3, `Reading shows b's posts unread (${await unread()})`);
await page.keyboard.press('Control+Alt+KeyA');
await page.waitForSelector('#wr-panel:not([hidden]) [data-markall-go]', { timeout: 10_000 });
const askText = await page.evaluate(() => document.querySelector('#wr-body').textContent);
check(/Mark 3 posts read in All\?/.test(askText), 'Mark all read asks first, with the count and source');
await page.click('[data-markall-go]');
await wait(1500);
check(/Marked 3 posts read/.test(await toastText()), 'it says what it did');
check((await unread()) === 0, 'nothing unread on the page');
const reading = (await a.api('GET', '/api/reading?limit=50')).items ?? [];
const imp = reading.filter((e) => e.imported);
check(imp.length >= 3 && imp.every((e) => e.imported.readVersion === e.imported.version), 'the blyg has every post read (GET /api/reading)');

// --- 2. Posts filters
await page.click('.views [data-view="posts"]');
await page.waitForSelector('#d-pfilter [data-pf]');
const chips = await page.$$eval('#d-pfilter [data-pf]', (cs) => Object.fromEntries(cs.map((c) => [c.dataset.pf, +c.querySelector('b').textContent])));
check(chips.all === 5 && chips.drafts === 1 && chips.changes === 1 && chips.public === 3 && chips.withdrawn === 1,
  `filter counts match the studio's summary (${JSON.stringify(chips)})`);
const summary = await a.api('GET', '/api/items/summary');
check(['all', 'drafts', 'changes', 'public', 'withdrawn'].every((k) => summary[k] === chips[k]), 'and GET /api/items/summary agrees');
await page.click('#d-pfilter [data-pf="withdrawn"]');
const rows = await page.$$eval('#list .row', (rs) => rs.map((r) => r.textContent));
check(rows.length === 1 && /withdrawn: taken back/.test(rows[0]), 'Withdrawn shows the withdrawn post only');
await page.click('#omni');
await page.keyboard.type('Parity sample public');
const searched = await page.$$eval('#list .row', (rs) => rs.length);
check(searched === 1, 'a search looks through every post, whatever the filter');
await page.keyboard.press('Escape');
await page.keyboard.press('Control+Alt+KeyF');
check(await page.evaluate(() => document.querySelector('#d-pfilter [data-pf="all"]').getAttribute('aria-pressed') === 'true'), '⌃⌥F steps to the next filter (back to All)');

// --- 3. Updates
await page.keyboard.press('Control+Digit3');
await page.click('#d-hub [data-hub="updates"]');
await page.waitForSelector('#wr-body [data-upd-refresh]', { timeout: 20_000 });
const upd = await page.evaluate(() => document.querySelector('#wr-body').textContent);
check(/quoting B1/.test(upd) && /stale quote/.test(upd), 'Updates lists the thread with its stale quote');
await page.click('#wr-body [data-upd-probe]');
await page.waitForFunction(() => /→ v2/.test(document.querySelector('#wr-body').textContent), null, { timeout: 20_000 }).catch(() => {});
check(/v1 → v2/.test(await page.evaluate(() => document.querySelector('#wr-body').textContent)), 'Quotes shows what changed (v1 → v2)');
await page.click('#wr-body [data-upd-refresh]');
await wait(2500);
check(/Refreshed/.test(await toastText()), 'Refresh says what it did');
const thread = (await a.api('GET', '/api/items?limit=100')).items.find((i) => /quoting B1/.test(i.contentMd));
check(thread?.version === 2, `the thread is v2 on the blyg (v${thread?.version})`);
check(((await a.api('GET', '/api/freshness')).items ?? []).length === 0, 'the blyg has nothing stale left');
await page.keyboard.press('Escape');

check(!errors.length, `no page errors${errors.length ? `: ${errors.join(' | ')}` : ''}`);
await browser.close();
console.log(failed ? `${failed} FAILED` : 'all ok');
process.exit(failed ? 1 : 0);
