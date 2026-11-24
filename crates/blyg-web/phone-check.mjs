// A regression check for the phone layout's Read tab, in WebKit with iPhone
// emulation (0.18.1: opening a reading post went blank on an iPhone, and
// every screen was slow on a blyg with thousands of posts to read).
//
//   node crates/blyg-web/phone-check.mjs [url]
//
// `url` defaults to the demo (`./crates/blyg-web/serve.sh`, then
// http://127.0.0.1:8787/). For live mode, point it at `dev-proxy.py`
// (http://localhost:8791/burrow/) on a disposable local blyg and set
// BURROW_PW to that blyg's Studio password: it signs in with it and never
// prints it. Never point it at a real blyg.
//
// It needs Playwright with WebKit (`npm i playwright && npx playwright
// install webkit` in any scratch directory, then run with NODE_PATH set to
// that directory's node_modules). It checks that:
//   - the stream draws a page of cards, not the whole list;
//   - opening a post shows it, and it stays shown after the outbox's send
//     (~800 ms later), the 30 s redraw and a second open of the same post;
//   - nothing under an open post is rebuilt, and the main thread never
//     stalls for long while one opens.
import { createRequire } from 'module';

const require = createRequire(import.meta.url); // honours NODE_PATH
const { webkit, devices } = require('playwright');

const url = process.argv[2] || 'http://127.0.0.1:8787/';
const browser = await webkit.launch();
const ctx = await browser.newContext({ ...devices['iPhone 13'] });
// Note every main-thread stall over 50 ms.
await ctx.addInitScript(() => {
  window.__stall = 0;
  let last = performance.now();
  const tick = () => { const n = performance.now(); window.__stall = Math.max(window.__stall, n - last); last = n; setTimeout(tick, 10); };
  tick();
});
if (process.env.BURROW_PW) {
  const origin = new URL(url).origin;
  const mount = new URL(url).pathname.replace(/\/burrow\/?$/, '');
  const r = await ctx.request.post(`${origin}${mount}/studio/login`, { form: { password: process.env.BURROW_PW }, headers: { origin }, maxRedirects: 0 });
  if (r.status() >= 400) throw new Error(`sign-in failed: ${r.status()}`);
}
const page = await ctx.newPage();
const errors = [];
page.on('pageerror', (e) => errors.push(String(e)));
let failed = 0;
const check = (ok, what) => { console.log(`${ok ? 'ok  ' : 'FAIL'} ${what}`); if (!ok) failed++; };
const view = () => page.evaluate(() => document.querySelector('#phone').dataset.view);
const post = () => page.evaluate(() => {
  const v = document.querySelector('#v-post');
  const b = document.querySelector('#p-postBody');
  const r = b.getBoundingClientRect();
  return { text: b.textContent.trim().length, inert: v.hasAttribute('inert'), vis: getComputedStyle(v).visibility, h: r.height, left: Math.round(v.getBoundingClientRect().left) };
});
const shown = (p) => p.text > 0 && !p.inert && p.vis === 'visible' && p.h > 0 && p.left === 0;

await page.goto(url);
await page.waitForFunction(() => document.querySelector('#p-loading.gone') && document.querySelectorAll('#p-list .prow').length > 0, null, { timeout: 120000 });
await page.waitForFunction(() => +document.querySelector('#p-tabn').textContent > 0, null, { timeout: 180000 });
await page.waitForTimeout(1500);
await page.tap('.tab[data-tab="read"]');
await page.waitForFunction(() => document.querySelectorAll('#p-stream .card').length > 0);
const cards = await page.evaluate(() => document.querySelectorAll('#p-stream .card').length);
check(cards <= 40, `the stream draws a page of cards (${cards})`);

const key = await page.evaluate(() => (document.querySelector('#p-stream .card.unread') || document.querySelector('#p-stream .card')).dataset.key);
// Mark the stream: a rebuild under the open post would drop the mark.
await page.evaluate(() => { document.querySelector('#p-stream').firstElementChild.dataset.kept = '1'; window.__stall = 0; });
await page.tap(`#p-stream .card[data-key="${key}"]`);
await page.waitForFunction(() => document.querySelector('#phone').dataset.view === 'post');
await page.waitForTimeout(400);
check(shown(await post()), 'the post shows');
await page.waitForTimeout(2600); // past the outbox's send and its redraws
check(shown(await post()) && (await view()) === 'post', 'the post is still shown after the send');
// The 30 s tick's redraw, now.
await page.evaluate(async () => (await import(new URL('core.js', location.href).href)).S.render());
await page.waitForTimeout(200);
check(shown(await post()), 'the post is still shown after a redraw');
check(await page.evaluate(() => !!document.querySelector('#p-stream [data-kept]')), 'nothing under the open post was rebuilt');
const stall = await page.evaluate(() => Math.round(window.__stall));
check(stall < 500, `no long stall while it opened (${stall} ms)`);

await page.tap('#p-back2');
await page.waitForFunction(() => document.querySelector('#phone').dataset.view === 'read');
await page.waitForTimeout(600);
await page.tap(`#p-stream .card[data-key="${key}"]`);
await page.waitForFunction(() => document.querySelector('#phone').dataset.view === 'post');
await page.waitForTimeout(2600);
check(shown(await post()), 'the same post shows when opened again');
await page.goBack();
await page.waitForTimeout(600);
check((await view()) === 'read', 'the back gesture returns to Read');
check(errors.length === 0, `no page errors${errors.length ? ': ' + errors.join('; ') : ''}`);

await browser.close();
process.exit(failed ? 1 : 0);
