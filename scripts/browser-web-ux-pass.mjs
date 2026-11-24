// Burrow web's usability pass (run by scripts/browser-web-ux-pass.sh, which
// starts the studios and the dev proxy and says where everything is).
// Local only: every URL must be a loopback address. Tokens and the password
// are read from files and never printed. Input is Playwright's, inside the
// page; nothing reaches the OS.
//
// Each persona walks its journeys and every step:
//   - takes a screenshot ($UX_SHOTS/<nn>-<persona>-<step>.png);
//   - notes failed (≥ 400 but 304, or no answer) requests, page errors and
//     console errors since the last step, elements wider than the viewport,
//     text cut off by its box, and on the phone tap targets under 40 px;
//   - times itself.
// Everything lands in $UX_SHOTS/observations.json. Look at the shots.
import fs from 'node:fs';
import path from 'node:path';
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
const SHOTS = env('UX_SHOTS');
const require = createRequire(`${env('PLAYWRIGHT_DIR')}/`);
const { chromium } = require('playwright');
const secret = (k) => fs.readFileSync(env(k), 'utf8').trim();
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

function client(base, tokenFile) {
  base = local(base);
  const tok = secret(tokenFile);
  async function call(method, p, body, { ok = false } = {}) {
    for (let attempt = 0; attempt < 6; attempt++) {
      const res = await fetch(base + '/api' + p, {
        method,
        headers: { authorization: `Bearer ${tok}`, accept: 'application/json', ...(body ? { 'content-type': 'application/json' } : {}) },
        body: body ? JSON.stringify(body) : undefined,
      });
      const text = await res.text();
      if (res.status === 429) { await sleep(1000 * (attempt + 1)); continue; }
      if (res.status >= 400 && !ok) throw new Error(`${method} ${p} -> ${res.status} ${text.slice(0, 200)}`);
      return text ? JSON.parse(text) : null;
    }
    throw new Error(`${method} ${p}: still rate-limited`);
  }
  async function all(p) {
    const out = [];
    let cursor = null;
    do {
      const sep = p.includes('?') ? '&' : '?';
      const page = await call('GET', `${p}${sep}limit=100${cursor ? `&cursor=${encodeURIComponent(cursor)}` : ''}`);
      out.push(...(page.items ?? []));
      cursor = page.nextCursor;
    } while (cursor);
    return out;
  }
  async function draft(md, kind = 'fragment') {
    return call('POST', '/items', { mode: 'blank', contentMd: md, kind });
  }
  async function post(md, kind = 'fragment') {
    const it = await draft(md, kind);
    await call('POST', `/items/${it.id}/publish`, { expectedRevision: it.revision });
    return it.id;
  }
  return { base, call, all, draft, post };
}

const a = client(env('LIVE_URL_A'), 'LIVE_TOKEN_A_FILE');
const b = client(env('LIVE_URL_B'), 'LIVE_TOKEN_B_FILE');

// ------------------------------------------------------------------ seed
// Invented sample content only. Bursts would otherwise send most of b's
// posts to Background (studio 0.69's `superseded` rule), so it's off on a.
const t0 = Date.now();
await a.call('PATCH', '/settings', { backgroundRules: { superseded: false } }, { ok: true });
const topics = ['tide pools', 'kelp forests', 'sea otters', 'barnacles', 'sand dunes', 'lichen', 'salt marshes', 'estuaries'];
for (let i = 0; i < 24; i++) {
  const t = topics[i % topics.length];
  if (i % 4 === 0) {
    await b.post(`# Field notes ${i + 1}: a longer look at ${t} through a whole season of invented walks\n\n` +
      `Invented paragraph one about ${t}. `.repeat(12) + '\n\n' + `A second paragraph, also invented, about ${t}. `.repeat(10), 'thread');
  } else {
    await b.post(`Sample note B${String(i + 1).padStart(2, '0')}: a short invented thought about ${t}.`);
  }
}
// The awkward ones: a very long unbroken word, a wide code block, a table.
await b.post('Unbroken: ' + 'Supercalifragilisticexpialidocious'.repeat(5) + ' and then normal words again.');
await b.post('# Code sample thread\n\nA wide code block:\n\n```\nconst invented = ["' + 'x'.repeat(160) + '"];\n```\n\n| column one | column two | column three | column four |\n|---|---|---|---|\n| invented cell | another invented cell | a third | a fourth one |', 'thread');

// a's own posts: published fragments and a thread, a draft, a withdrawn one.
for (let i = 1; i <= 5; i++) await a.post(`Own post A0${i}: invented notes on example topic ${i}.`);
await a.post('# My invented essay on rock pools\n\nIt starts here. ' + 'Invented sentences go on. '.repeat(20), 'thread');
await a.draft('Unfinished draft about mudflats, not yet published.');
const toWithdraw = await a.draft('A post I will withdraw later.');
await a.call('POST', `/items/${toWithdraw.id}/publish`, { expectedRevision: toWithdraw.revision });
const w = await a.call('GET', `/items/${toWithdraw.id}`);
await a.call('POST', `/items/${toWithdraw.id}/withdraw`, { expectedRevision: w.revision }, { ok: true });

let sub = await a.call('POST', '/subscriptions', { url: b.base });
if (sub?.needsConfirm) sub = await a.call('POST', '/subscriptions', { url: b.base, confirm: true });
const subs = await a.all('/subscriptions');
const subId = subs[0]?.id;
if (!subId) throw new Error('subscribing a to b failed');
let imported = [];
for (let t = 0; t < 90 && imported.length < 26; t++) {
  imported = await a.all('/reading/imported');
  if (imported.length < 26) { if (t % 10 === 9) await a.call('POST', `/subscriptions/${subId}/resync`); await sleep(1000); }
}
await a.call('POST', '/hoppers', { name: 'Kelp hopper' });
console.log(`seeded in ${((Date.now() - t0) / 1000).toFixed(0)} s: ${imported.length} reading posts`);

// ------------------------------------------------------------------ audit
const obs = { steps: [], started: new Date().toISOString() };
let shotN = 0;
let session = null;

async function newPersona(name, viewport, { mobile = false } = {}) {
  // One Studio sign-in for the whole pass (the studio's sign-in budget is
  // small); every persona starts from its session cookie.
  if (!session) {
    const first = await browser.newContext();
    const login = await first.request.post(`${ORIGIN}/studio/login`, {
      form: { password: secret('LIVE_PASSWORD_FILE') }, headers: { origin: ORIGIN }, maxRedirects: 0,
    });
    if (login.status() >= 400) throw new Error(`Studio sign-in failed (${login.status()})`);
    session = await first.storageState();
    await first.close();
  }
  const ctx = await browser.newContext({ viewport, isMobile: mobile, hasTouch: mobile, deviceScaleFactor: mobile ? 2 : 1, storageState: session });
  const page = await ctx.newPage();
  page.setDefaultTimeout(5000);
  const p = { name, page, ctx, mobile, viewport, bad: [], errors: [], last: Date.now() };
  page.on('response', (res) => {
    const s = res.status();
    if (s >= 400) p.bad.push(`${res.request().method()} ${new URL(res.url()).pathname} → ${s}`);
  });
  page.on('requestfailed', (req) => p.bad.push(`${req.method()} ${new URL(req.url()).pathname} → failed (${req.failure()?.errorText})`));
  page.on('pageerror', (e) => p.errors.push(`page error: ${e.message}`));
  page.on('console', (m) => { if (m.type() === 'error' || m.type() === 'warning') p.errors.push(`console ${m.type()}: ${m.text().slice(0, 300)}`); });
  page.on('dialog', (d) => { p.errors.push(`native dialog: ${d.type()} "${d.message()}"`); d.dismiss().catch(() => {}); });
  return p;
}

/** What a script can notice on the screen now. */
async function layout(page, mobile) {
  return page.evaluate((mobile) => {
    const vw = window.innerWidth;
    const out = { pageScrollsSideways: document.scrollingElement.scrollWidth > vw + 1, wide: [], clipped: [], small: [] };
    const visible = (el) => {
      const r = el.getBoundingClientRect();
      if (r.width === 0 || r.height === 0) return false;
      const cs = getComputedStyle(el);
      if (cs.visibility === 'hidden' || cs.display === 'none' || +cs.opacity === 0) return false;
      return r.bottom > 0 && r.top < window.innerHeight && !el.closest('[hidden],[inert]');
    };
    const name = (el) => {
      const id = el.id ? `#${el.id}` : '';
      const cls = typeof el.className === 'string' && el.className ? '.' + el.className.trim().split(/\s+/).slice(0, 2).join('.') : '';
      return `${el.tagName.toLowerCase()}${id}${cls} "${(el.innerText || el.value || el.getAttribute('aria-label') || '').replace(/\s+/g, ' ').trim().slice(0, 50)}"`;
    };
    for (const el of document.querySelectorAll('body *')) {
      if (!visible(el)) continue;
      const r = el.getBoundingClientRect();
      if (r.right > vw + 2 && out.wide.length < 12 && !el.closest('.pstrip,.shelfstrip,[style*="overflow"]')) {
        // Only report the outermost offender.
        if (!out.wide.some((w) => w.el === el.parentElement)) out.wide.push({ what: name(el), right: Math.round(r.right) });
      }
      const cs = getComputedStyle(el);
      if (el.children.length === 0 && (el.innerText || '').trim() && el.scrollWidth > el.clientWidth + 2 &&
          (cs.overflowX === 'hidden' || cs.overflow === 'hidden' || cs.overflowX === 'clip') && out.clipped.length < 12) {
        out.clipped.push({ what: name(el), ellipsis: cs.textOverflow === 'ellipsis', need: el.scrollWidth, have: el.clientWidth });
      }
      if (mobile && el.matches('button,a,[role=button],input,select,textarea,[role=tab]') && (r.height < 40 || r.width < 40) && out.small.length < 25) {
        out.small.push({ what: name(el), w: Math.round(r.width), h: Math.round(r.height) });
      }
    }
    out.wide = out.wide.map((w) => `${w.what} (right edge ${w.right})`);
    return out;
  }, mobile);
}

async function step(p, label, note, fn) {
  const t = Date.now();
  let error = null;
  try { if (fn) await fn(); } catch (e) { error = String(e.message || e).split('\n')[0].slice(0, 300); }
  const ms = Date.now() - t;
  await sleep(250);
  const file = `${String(++shotN).padStart(3, '0')}-${p.name}-${label}.png`;
  await p.page.screenshot({ path: path.join(SHOTS, file) }).catch(() => {});
  const lay = await layout(p.page, p.mobile).catch((e) => ({ err: String(e) }));
  const rec = { persona: p.name, step: label, note, shot: file, ms, error, bad: p.bad.splice(0), errors: p.errors.splice(0), layout: lay };
  obs.steps.push(rec);
  const flags = [error && `ERROR ${error}`, rec.bad.length && `${rec.bad.length} bad requests`, rec.errors.length && `${rec.errors.length} errors`,
    lay.pageScrollsSideways && 'scrolls sideways', lay.wide?.length && `${lay.wide.length} wide`, lay.clipped?.length && `${lay.clipped.length} clipped`].filter(Boolean);
  console.log(`${file} ${ms} ms${flags.length ? ' — ' + flags.join(', ') : ''}`);
  for (const x of [...rec.bad, ...rec.errors]) console.log(`    ${x}`);
  fs.writeFileSync(path.join(SHOTS, 'observations.json'), JSON.stringify(obs, null, 2));
  return !error;
}

const text = (page, sel) => page.$eval(sel, (e) => e.innerText.replace(/\s+/g, ' ').trim()).catch(() => null);
const mod = 'Control';
/** Close a Reading panel (subscriptions, mentions…) the way a person would. */
async function closePanel(pg) {
  if (await pg.isVisible('#wr-panel')) await pg.click('#wr-panel .wr-x').catch(() => {});
}
const browser = await chromium.launch();

try {
  // ================================================================ first week (desktop)
  {
    const p = await newPersona('firstweek-desk', { width: 1440, height: 900 });
    const pg = p.page;
    await step(p, 'open', 'first open of /burrow/', async () => {
      const t = Date.now();
      await pg.goto(`${ORIGIN}/burrow/`);
      await pg.waitForSelector('#list .row', { timeout: 30_000 });
      p.firstList = Date.now() - t;
    });
    await sleep(4000);
    await step(p, 'settled', 'after the first refresh');
    await step(p, 'reading-tab', 'click Reading', () => pg.click('.views [data-view="reading"]'));
    await step(p, 'blyg-tab', 'click ⚙ Blyg', () => pg.click('.views [data-view="blyg"]'));
    await pg.evaluate(() => document.querySelector('#bbody .bscroll')?.scrollTo(0, 99999)).catch(() => {});
    await step(p, 'blyg-bottom', 'Blyg view scrolled to the bottom');
    await step(p, 'help-hover', 'what the page says about keys (below the window)', () => pg.evaluate(() => window.scrollTo(0, 99999)));
    await p.ctx.close();
  }

  // ================================================================ daily reader (desktop)
  {
    const p = await newPersona('reader-desk', { width: 1440, height: 900 });
    const pg = p.page;
    await pg.goto(`${ORIGIN}/burrow/`);
    await pg.waitForSelector('#list .row', { timeout: 30_000 });
    await step(p, 'reading', 'open Reading (⌃2)', async () => { await pg.keyboard.press(`${mod}+2`); await pg.waitForSelector('#d-stream .card', { timeout: 15_000 }); });
    await step(p, 'unread-badge', `tab badge says "${await text(pg, '#d-unread')}"`);
    await step(p, 'open-first', 'click the first card', () => pg.click('#d-stream .card'));
    await step(p, 'next-post', 'next post (›)', () => pg.click('[data-ra="next"]'));
    await step(p, 'more-menu', '⋯ More', () => pg.click('#d-moreRA'));
    await step(p, 'hopper-sheet', '⊕ Hopper', () => pg.click('#d-moreMenu [data-ra="hopper"]'));
    await pg.keyboard.press('Escape');
    for (const src of ['smart:all', 'smart:raw', 'smart:agentic', 'smart:shelf', 'smart:background', 'smart:thumbed', 'smart:unread']) {
      await step(p, `src-${src.split(':')[1]}`, `source ${src}`, async () => {
        const b = await pg.$(`[data-src="${src}"]`);
        if (!b) throw new Error(`no source button ${src}`);
        await b.click();
      });
    }
    await step(p, 'feed-again', 'back to the Feed', () => pg.click('[data-src="smart:all"]'));
    await step(p, 'order-chrono', 'Feed order: chronological', async () => {
      const b = await pg.$('[data-order="newest"]');
      if (!b) throw new Error('no order control');
      await b.click();
    });
    await step(p, 'order-pace', 'Feed order: pace-layered', async () => { const b = await pg.$('[data-order="pace-layered"]'); if (!b) throw new Error('no order control'); await b.click(); });
    await step(p, 'lens-threads', 'lens Threads', () => pg.click('[data-lens="thread"]'));
    await step(p, 'lens-all', 'lens All', () => pg.click('[data-lens="all"]'));
    await step(p, 'search', 'search "kelp" in Reading', async () => { await pg.click('#omni'); await pg.keyboard.type('kelp'); });
    await step(p, 'search-none', 'search with no match', async () => { await pg.fill('#omni', 'zzzz-nothing'); });
    await pg.fill('#omni', ''); await pg.press('#omni', 'Escape');
    await step(p, 'long-word', 'open the post with a long unbroken word', async () => {
      await pg.fill('#omni', 'Unbroken');
      await pg.click('#d-stream .card');
    });
    await step(p, 'code-post', 'open the post with a wide code block', async () => {
      await pg.fill('#omni', 'Code sample');
      await pg.click('#d-stream .card');
    });
    await pg.fill('#omni', '');
    await pg.press('#omni', 'Escape');
    await step(p, 'subscriptions', '☷ Subscriptions', () => pg.click('#d-wr-sources [data-go="subs"]'));
    await step(p, 'subscribe', '＋ Subscribe', async () => { await closePanel(pg); await pg.click('#d-wr-sources [data-go="subscribe"]'); });
    await step(p, 'subscribe-bad', 'subscribe to a nonsense URL', async () => {
      const inp = await pg.$('#wr-body input');
      if (!inp) throw new Error('no URL field');
      await inp.fill('not a url at all');
      await pg.keyboard.press('Enter');
      await sleep(1500);
    });
    await closePanel(pg);
    await step(p, 'mentions', '@ Mentions', () => pg.click('#d-wr-sources [data-go="mentions"]'));
    await closePanel(pg);
    await step(p, 'history', '⟲ History', () => pg.click('#d-wr-sources [data-go="history"]'));
    await closePanel(pg);
    await step(p, 'reading-settings', '⚙ Reading', () => pg.click('#d-wr-sources [data-go="reading"]'));
    await closePanel(pg);
    await step(p, 'markall', '✓ Mark all read…', async () => { const b = await pg.$('#d-wr-sources [data-go="markall"]'); if (!b) throw new Error('no mark-all'); await b.click(); });
    await pg.keyboard.press('Escape');
    // Scroll the stream to its end: does more load?
    await step(p, 'scrolled', 'stream scrolled to the end', () => pg.evaluate(() => document.querySelector('#d-stream').scrollTo(0, 1e6)));
    await p.ctx.close();
  }

  // ================================================================ writer (desktop)
  {
    const p = await newPersona('writer-desk', { width: 1440, height: 900 });
    const pg = p.page;
    await pg.goto(`${ORIGIN}/burrow/`);
    await pg.waitForSelector('#list .row', { timeout: 30_000 });
    await step(p, 'type-omni', 'type a new title into the omnibar', async () => { await pg.click('#omni'); await pg.keyboard.type('Invented idea about drift wood'); });
    await step(p, 'enter-create', '⏎ creates a draft', () => pg.keyboard.press('Enter'));
    await step(p, 'type-body', 'type the body', async () => { await pg.click('#ed'); await pg.keyboard.press('End'); await pg.keyboard.type('\n\nSome invented words about drift wood on a beach.'); });
    await sleep(2500);
    await step(p, 'synced', `after a pause: status "${await text(pg, '#sync')}"`);
    await step(p, 'preview', 'preview (⌃E)', () => pg.keyboard.press(`${mod}+e`));
    await step(p, 'preview-off', 'preview off (⌃E)', () => pg.keyboard.press(`${mod}+e`));
    await step(p, 'publish-sheet', 'publish (⌃⏎)', async () => { await pg.click('#ed'); await pg.keyboard.press(`${mod}+Enter`); });
    await step(p, 'publish-go', 'note + ⏎ in the sheet', async () => {
      const n = await pg.$('#sheet textarea, #sheet input, #note');
      if (n) await n.fill('First version');
      await pg.keyboard.press('Enter');
      await sleep(2500);
    });
    const items = await a.all('/items');
    const mine = items.find((i) => (i.contentMd || '').includes('drift wood'));
    p.published = mine ? `${mine.status} v${mine.version ?? '?'}` : 'not on the blyg';
    await step(p, 'published', `on the blyg: ${p.published}`);
    await step(p, 'revise', 'edit the published post', async () => { await pg.click('#ed'); await pg.keyboard.press('End'); await pg.keyboard.type(' A revision.'); await sleep(2000); });
    await step(p, 'versions', 'versions (⌃⇧Y)', () => pg.keyboard.press(`${mod}+Shift+Y`));
    await pg.keyboard.press('Escape');
    await step(p, 'discard-ask', 'discard changes (⌥⌘Z → ⌃⌥Z)', () => pg.keyboard.press(`${mod}+Alt+z`));
    await pg.keyboard.press('Escape');
    // An over-long fragment.
    await step(p, 'new-long', 'new post (⌃N), 1100 characters', async () => {
      await pg.keyboard.press(`${mod}+n`);
      await sleep(300);
      await pg.click('#ed');
      await pg.keyboard.insertText('Long invented text. '.repeat(55));
    });
    await step(p, 'publish-long', 'publish the over-long fragment', () => pg.keyboard.press(`${mod}+Enter`));
    await pg.keyboard.press('Escape');
    await step(p, 'make-draft', 'make it a draft (⌃D)', () => pg.keyboard.press(`${mod}+d`));
    await step(p, 'delete-ask', 'delete draft (⇧⌃⌫)', () => pg.keyboard.press(`${mod}+Shift+Backspace`));
    await step(p, 'delete-go', 'confirm', () => pg.keyboard.press('Enter'));
    // A conflict: the post changes elsewhere while it's open here.
    await step(p, 'conflict-setup', 'open my essay', async () => { await pg.fill('#omni', 'rock pools'); await pg.keyboard.press('Enter'); });
    const essay = (await a.all('/items')).find((i) => (i.contentMd || '').includes('rock pools'));
    if (essay) await a.call('PATCH', `/items/${essay.id}`, { expectedRevision: essay.revision, contentMd: essay.contentMd + '\n\nChanged on another device.' });
    await step(p, 'conflict', 'type while the blyg has a newer copy', async () => { await pg.click('#ed'); await pg.keyboard.press('End'); await pg.keyboard.type(' Typed here.'); await sleep(4000); });
    await step(p, 'withdrawn', 'find the withdrawn post', async () => { await pg.fill('#omni', 'withdraw'); });
    await p.ctx.close();
  }

  // ================================================================ keyboard-only (desktop)
  {
    const p = await newPersona('keys-desk', { width: 1440, height: 900 });
    const pg = p.page;
    await pg.goto(`${ORIGIN}/burrow/`);
    await pg.waitForSelector('#list .row', { timeout: 30_000 });
    const focus = () => pg.evaluate(() => { const e = document.activeElement; return e ? `${e.tagName.toLowerCase()}${e.id ? '#' + e.id : ''} "${(e.innerText || e.getAttribute('aria-label') || e.placeholder || '').trim().slice(0, 40)}"` : 'none'; });
    const tabs = [];
    for (let i = 0; i < 14; i++) { await pg.keyboard.press('Tab'); tabs.push(await focus()); }
    await step(p, 'tab-order', `Tab ×14 from load: ${tabs.join(' → ')}`);
    await pg.keyboard.press(`${mod}+l`);
    await step(p, 'ctrl-l', `⌃L focus: ${await focus()}`);
    await step(p, 'arrows', '↓ ↓ in the omnibar', async () => { await pg.keyboard.press('ArrowDown'); await pg.keyboard.press('ArrowDown'); });
    await step(p, 'enter-open', '⏎ opens', async () => { await pg.keyboard.press('Enter'); p.f = await focus(); });
    await step(p, 'esc-back', `after ⏎ focus was ${p.f}; esc`, () => pg.keyboard.press('Escape'));
    await step(p, 'ctrl-2', '⌃2 Reading', async () => { await pg.keyboard.press(`${mod}+2`); p.f = await focus(); });
    await step(p, 'reading-down', `focus after ⌃2: ${p.f}; ↓ ↓`, async () => { await pg.keyboard.press('ArrowDown'); await pg.keyboard.press('ArrowDown'); p.f = await focus(); });
    const rtabs = [];
    for (let i = 0; i < 10; i++) { await pg.keyboard.press('Tab'); rtabs.push(await focus()); }
    await step(p, 'reading-tabs', `Tab ×10 in Reading: ${rtabs.join(' → ')}`);
    await step(p, 'mark-all-keys', '⌃⌥A', () => pg.keyboard.press(`${mod}+Alt+a`));
    await pg.keyboard.press('Escape');
    await step(p, 'ctrl-3', '⌃3 Blyg', async () => { await pg.keyboard.press(`${mod}+3`); p.f = await focus(); });
    await step(p, 'shelf-keys', `focus after ⌃3: ${p.f}; ⌃B shelf`, () => pg.keyboard.press(`${mod}+b`));
    await pg.keyboard.press('Escape');
    await p.ctx.close();
  }

  // ================================================================ phone thumb user
  {
    const p = await newPersona('phone', { width: 390, height: 844 }, { mobile: true });
    const pg = p.page;
    await step(p, 'open', 'first open on a phone', async () => {
      await pg.goto(`${ORIGIN}/burrow/`);
      await pg.waitForFunction(() => document.querySelector('#p-loading.gone') && document.querySelectorAll('#p-list .prow').length > 0, null, { timeout: 60_000 });
    });
    await sleep(3000);
    await step(p, 'write-tab', 'Write tab settled');
    await step(p, 'read-tab', 'tap Read', () => pg.tap('.tab[data-tab="read"]'));
    await sleep(1500);
    await step(p, 'read-settled', `Read; tab badge "${await text(pg, '#p-tabn')}", unread button "${await text(pg, '#p-unreadOnly')}"`);
    await step(p, 'strip-scrolled', 'the toolbar strip scrolled to its end', () => pg.evaluate(() => document.querySelector('#p-wrBar').scrollTo(1e4, 0)));
    await step(p, 'sources', '☰ Sources', () => pg.tap('#p-wrBar [data-go="sources"]'));
    await step(p, 'sources-agentic', 'Agentic feed', async () => { const b = await pg.$('#wr-body [data-src="smart:agentic"]'); if (!b) throw new Error('no Agentic source'); await b.tap(); });
    await step(p, 'sources-feed', 'back to the Feed', async () => { await pg.tap('#p-wrBar [data-go="sources"]'); await pg.tap('#wr-body [data-src="smart:all"]'); });
    await step(p, 'open-card', 'tap the first card', () => pg.tap('#p-stream .card'));
    await sleep(800);
    await step(p, 'post-more', '⋯ More', () => pg.tap('#p-moreBtn'));
    await step(p, 'post-next', 'next post ›', () => pg.tap('#p-nextPost'));
    await step(p, 'long-word', 'the long-word post', async () => {
      await pg.tap('#p-back2'); await sleep(500);
      await pg.fill('#p-rq', 'Unbroken'); await sleep(500);
      await pg.tap('#p-stream .card');
    });
    await step(p, 'code-post', 'the wide code post', async () => {
      await pg.tap('#p-back2'); await sleep(500);
      await pg.fill('#p-rq', 'Code sample'); await sleep(500);
      await pg.tap('#p-stream .card');
    });
    await step(p, 'thread-post', 'a long thread', async () => {
      await pg.tap('#p-back2'); await sleep(500);
      await pg.fill('#p-rq', 'Field notes 1:'); await sleep(500);
      await pg.tap('#p-stream .card');
    });
    await pg.tap('#p-back2').catch(() => {});
    await pg.fill('#p-rq', '');
    await step(p, 'unread-only', 'tap ● N (unread only)', () => pg.tap('#p-unreadOnly'));
    await step(p, 'fab', 'tap ＋ (quick capture)', () => pg.tap('#p-fab'));
    await step(p, 'capture-type', 'type a capture', () => pg.fill('#p-capTa', 'A quick invented thought from a phone.'));
    await step(p, 'capture-keep', 'Keep as note', () => pg.tap('#p-capKeep'));
    await step(p, 'write-tab2', 'Write tab', () => pg.tap('.tab[data-tab="write"]'));
    await step(p, 'open-post', 'tap my first post', () => pg.tap('#p-list .prow'));
    await sleep(800);
    await step(p, 'editor-focus', 'tap into the editor', () => pg.tap('#p-ta'));
    await step(p, 'editor-bar-2', 'editor bar page 2', () => pg.evaluate(() => { const s = document.querySelector('#p-bar .ebpages, #p-bar [class*=page], #p-bar > *'); s?.scrollBy?.(400, 0); }));
    await step(p, 'post-menu', '⋯ Post', () => pg.tap('#p-wmenu'));
    await pg.keyboard.press('Escape');
    await step(p, 'back', '‹ Posts', () => pg.tap('#p-back'));
    await step(p, 'shelf-tab', 'Shelf tab', () => pg.tap('.tab[data-tab="shelf"]'));
    await pg.keyboard.press('Escape');
    await step(p, 'blyg-tab', 'Blyg tab', () => pg.tap('.tab[data-tab="blyg"]'));
    await step(p, 'blyg-scrolled', 'Blyg scrolled', () => pg.evaluate(() => document.querySelector('#p-hub').scrollTo(0, 1e6)));
    await step(p, 'search-start', 'Write: search with no match', async () => { await pg.tap('.tab[data-tab="write"]'); await pg.fill('#p-q', 'Something brand new from the phone'); });
    await p.ctx.close();
  }

  // ================================================================ first week (phone)
  {
    const p = await newPersona('firstweek-phone', { width: 390, height: 844 }, { mobile: true });
    const pg = p.page;
    await pg.goto(`${ORIGIN}/burrow/`);
    await pg.waitForFunction(() => document.querySelector('#p-loading.gone'), null, { timeout: 60_000 });
    await sleep(3000);
    await step(p, 'subscribe', 'Read › ＋ Subscribe', async () => { await pg.tap('.tab[data-tab="read"]'); await pg.tap('#p-wrBar [data-go="subscribe"]'); });
    await pg.keyboard.press('Escape');
    await step(p, 'hoppers', 'Read › ⊕ Hoppers', async () => { await closePanel(pg); await pg.tap('#p-wrBar [data-go="hoppers"]'); });
    await closePanel(pg);
    await step(p, 'reading-settings', 'Read › ⚙ Reading', async () => { await pg.evaluate(() => document.querySelector('#p-wrBar').scrollTo(1e4, 0)); await pg.tap('#p-wrBar [data-go="reading"]'); });
    await closePanel(pg);
    await p.ctx.close();
  }
} finally {
  await browser.close();
  obs.finished = new Date().toISOString();
  fs.writeFileSync(path.join(SHOTS, 'observations.json'), JSON.stringify(obs, null, 2));
}
const flagged = obs.steps.filter((s) => s.error || s.bad.length || s.errors.length || s.layout?.pageScrollsSideways);
console.log(`${obs.steps.length} steps, ${flagged.length} flagged; shots and observations.json in ${SHOTS}`);
