// Burrow web's flow pass (run by scripts/browser-web-flow-pass.sh, which
// starts the studios, the feed sites and the dev proxy). Local only: every
// URL must be a loopback address. Tokens and the password are read from
// files and never printed. Input is Playwright's, inside the page.
//
// FLOW_PHASE=seed seeds the blygs; journeys walks them; all (the default)
// does both.
import fs from 'node:fs';
import path from 'node:path';
import { createRequire } from 'node:module';
import { metricsInit } from './web-flow-metrics.mjs';
import { burrowJourneys } from './web-flow-burrow.mjs';
import { studioJourneys } from './web-flow-studio.mjs';

const env = (k, d) => {
  const v = process.env[k] ?? d;
  if (v === undefined || v === '') throw new Error(`${k} is not set`);
  return v;
};
const local = (u) => {
  if (!/^http:\/\/(127\.0\.0\.1|localhost):\d+$/.test(u)) throw new Error(`not a local URL: ${u}`);
  return u;
};
const PHASE = env('FLOW_PHASE', 'all');
const OUT = env('FLOW_OUT');
const require = createRequire(`${env('PLAYWRIGHT_DIR')}/`);
const pw = require('playwright');
const secret = (k) => fs.readFileSync(env(k), 'utf8').trim();
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

function client(base, tokenFile) {
  base = local(base);
  const tok = secret(tokenFile);
  async function call(method, p, body, { ok = false } = {}) {
    for (let attempt = 0; attempt < 8; attempt++) {
      const res = await fetch(base + '/api' + p, {
        method,
        headers: { authorization: `Bearer ${tok}`, accept: 'application/json', ...(body ? { 'content-type': 'application/json' } : {}) },
        body: body ? JSON.stringify(body) : undefined,
      });
      const text = await res.text();
      if (res.status === 429) { await sleep(1000 * (attempt + 1)); continue; }
      if (res.status >= 400 && !ok) throw new Error(`${method} ${p} -> ${res.status} ${text.slice(0, 200)}`);
      try { return text ? JSON.parse(text) : null; } catch { return null; }
    }
    throw new Error(`${method} ${p}: still rate-limited`);
  }
  async function all(p) {
    // A list that changes while it's read (an import running) answers
    // cursor-expired: read it again from the top.
    for (let attempt = 0; ; attempt++) {
      const out = [];
      let cursor = null;
      let expired = false;
      do {
        const sep = p.includes('?') ? '&' : '?';
        const page = await call('GET', `${p}${sep}limit=100${cursor ? `&cursor=${encodeURIComponent(cursor)}` : ''}`, undefined, { ok: attempt < 8 });
        if (page?.code === 'cursor-expired') { expired = true; break; }
        out.push(...(page?.items ?? []));
        cursor = page?.page?.nextCursor ?? page?.nextCursor;
      } while (cursor);
      if (!expired) return out;
      await sleep(500);
    }
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

const blygs = {
  burrow: client(env('FLOW_URL_BURROW'), 'FLOW_TOKEN_BURROW_FILE'),
  studio: client(env('FLOW_URL_STUDIO'), 'FLOW_TOKEN_STUDIO_FILE'),
};
const b = client(env('FLOW_URL_B'), 'FLOW_TOKEN_B_FILE');
const sites = JSON.parse(fs.readFileSync(env('FLOW_FEEDS_FILE'), 'utf8')).map((s) => (typeof s === 'number' ? { port: s, name: '' } : s));
const feeds = sites.map((s) => `http://127.0.0.1:${s.port}`);
const feedNames = await Promise.all(sites.map(async (s, i) => s.name || (await fetch(`${feeds[i]}/feed.xml`).then((r) => r.text())).match(/<title>([^<]*)<\/title>/)?.[1] || ''));

// ------------------------------------------------------------------ seed
// Invented content only. b: fragments, long threads, a chain of quotes
// inside quotes, made a second apart so each has its own time. Each blyg
// under test: the same own posts, b and the ten feed sites followed.
const topics = ['tide pools', 'kelp forests', 'sea otters', 'barnacles', 'sand dunes', 'lichen', 'salt marshes', 'estuaries'];
const longThread = (t, n) => `# Field notes ${n}: a long look at ${t}\n\n` +
  Array.from({ length: 14 }, (_, i) => `Paragraph ${i + 1}, invented, about ${t}. ` + `It goes on for a while, as long posts do, so reading it means scrolling. `.repeat(4)).join('\n\n');

async function seedB() {
  const ids = [];
  for (let i = 0; i < 24; i++) {
    const t = topics[i % topics.length];
    if (i % 4 === 0) ids.push(await b.post(longThread(t, i + 1), 'thread'));
    else ids.push(await b.post(`Sample note B${String(i + 1).padStart(2, '0')}: a short invented thought about ${t}.`));
    await sleep(1050);
  }
  // Quotes inside quotes: C3 quotes C2, which quotes C1.
  const c1 = await b.post('# The first word on tide pools\n\nAn invented claim that tide pools are small oceans. ' + 'More invented words. '.repeat(20), 'thread');
  await sleep(1050);
  const c2 = await b.post(`# A reply about tide pools\n\nQuoting the first word:\n\n![[${c1}]]\n\nAnd an invented answer to it. ` + 'Answer words. '.repeat(20), 'thread');
  await sleep(1050);
  await b.post(`# A third view on tide pools\n\nQuoting the reply, which quotes the first:\n\n![[${c2}]]\n\nThe third invented view. ` + 'View words. '.repeat(20), 'thread');
  await sleep(1050);
  return ids;
}

async function seedBlyg(a, name) {
  await a.call('PATCH', '/settings', { backgroundRules: { superseded: false } }, { ok: true });
  for (let i = 1; i <= 6; i++) await a.post(`Own post A0${i}: invented notes on example topic ${i}.`);
  await a.post('# My invented essay on rock pools\n\nIt starts here. ' + 'Invented sentences go on. '.repeat(30), 'thread');
  await a.draft('Unfinished draft about mudflats, not yet published.');
  const subIds = [];
  // The last site is left for the subscribe journey.
  for (const url of [b.base, ...feeds.slice(0, -1)]) {
    let s = await a.call('POST', '/subscriptions', { url }, { ok: true });
    if (s?.needsConfirm) s = await a.call('POST', '/subscriptions', { url, confirm: true }, { ok: true });
    if (s?.id) subIds.push(s.id);
  }
  const subs = await a.all('/subscriptions');
  let imported = [];
  for (let t = 0; t < 120; t++) {
    imported = await a.all('/reading/imported');
    if (imported.length >= 300) break;
    if (t % 15 === 14) for (const s of subs) await a.call('POST', `/subscriptions/${s.id}/resync`, undefined, { ok: true });
    await sleep(1000);
  }
  await a.call('POST', '/hoppers', { name: 'Kelp hopper' }, { ok: true });
  console.log(`seeded ${name}: ${subs.length} subscriptions, ${imported.length} reading posts`);
  return { subscriptions: subs.length, reading: imported.length };
}

if (PHASE === 'seed' || PHASE === 'all') {
  const t0 = Date.now();
  await seedB();
  const seeded = {};
  for (const [name, a] of Object.entries(blygs)) seeded[name] = await seedBlyg(a, name);
  fs.writeFileSync(path.join(OUT, 'seed.json'), JSON.stringify(seeded, null, 2));
  console.log(`seeded in ${((Date.now() - t0) / 1000).toFixed(0)} s`);
}
if (PHASE === 'seed') process.exit(0);

// ------------------------------------------------------------------ journeys
const ctx = {
  pw, sleep, secret, OUT, blygs, b, feeds, feedNames,
  origins: { burrow: local(env('FLOW_ORIGIN_BURROW')), studio: local(env('FLOW_ORIGIN_STUDIO')) },
  password: () => secret('FLOW_PASSWORD_FILE'),
  only: (process.env.FLOW_ONLY || '').split(',').filter(Boolean),
  browsers: (process.env.FLOW_BROWSERS || 'chromium,webkit').split(',').filter(Boolean),
  metricsInit,
};
const targets = (process.env.FLOW_TARGETS || 'burrow,studio').split(',').filter(Boolean);
const results = [];
if (targets.includes('burrow')) results.push(...await burrowJourneys(ctx));
if (targets.includes('studio')) results.push(...await studioJourneys(ctx));

// ------------------------------------------------------------------ report
const prior = fs.existsSync(path.join(OUT, 'flow.json')) ? JSON.parse(fs.readFileSync(path.join(OUT, 'flow.json'), 'utf8')) : { journeys: [] };
const keyOf = (j) => `${j.target}|${j.journey}|${j.viewport}|${j.browser}`;
const merged = new Map(prior.journeys.map((j) => [keyOf(j), j]));
for (const j of results) merged.set(keyOf(j), j);
const journeys = [...merged.values()];
fs.writeFileSync(path.join(OUT, 'flow.json'), JSON.stringify({ at: new Date().toISOString(), journeys }, null, 2));

const budgets = JSON.parse(fs.readFileSync(env('FLOW_BUDGETS'), 'utf8'));
const broken = [];
for (const j of results.filter((r) => r.target === 'burrow')) {
  const bud = budgets.journeys?.[j.journey]?.[j.viewport];
  if (!bud) continue;
  const t = j.totals;
  const check = (what, got, max) => { if (max !== undefined && got > max) broken.push(`${j.journey} ${j.viewport} ${j.browser}: ${what} ${got} > ${max}`); };
  check('settle ms (worst step)', t.worstSettleMs, bud.worstSettleMs);
  check('settle ms (sum)', t.settleMs, bud.settleMs);
  check('layout shift', +t.cls.toFixed(3), bud.cls);
  check('anchor jumps', t.jumps, bud.jumps ?? 0);
  check('flicker', t.flicker, bud.flicker ?? 0);
  check('short spinners', t.shortSpinners, bud.shortSpinners ?? 0);
  check('empty flashes', t.emptyFlashes, bud.emptyFlashes ?? 0);
  check('duplicate requests', t.dupRequests, bud.dupRequests ?? 0);
  check('inputs', t.inputs, bud.inputs);
  check('focus misses', t.focusMisses, bud.focusMisses ?? 0);
  check('kept-state losses', t.lost, bud.lost ?? 0);
  check('failed steps', t.errors, 0);
}
const md = summary(journeys);
fs.writeFileSync(path.join(OUT, 'summary.md'), md);
console.log(md);
if (broken.length) {
  console.log(`\nBUDGETS BROKEN (${broken.length}):\n  ${broken.join('\n  ')}`);
  process.exit(1);
}
console.log('every Burrow journey is within its budget');

function summary(js) {
  const rows = [];
  const names = [...new Set(js.map((j) => `${j.journey}|${j.viewport}|${j.browser}`))].sort();
  rows.push('| journey | viewport | browser | UI | settle sum / worst ms | CLS | jumps | flicker | short spinners | empty flashes | requests (dup) | long tasks | inputs | focus misses | lost |');
  rows.push('|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|');
  for (const n of names) {
    for (const target of ['burrow', 'studio']) {
      const j = js.find((x) => `${x.journey}|${x.viewport}|${x.browser}` === n && x.target === target);
      if (!j) continue;
      const t = j.totals;
      const [journey, viewport, browser] = n.split('|');
      rows.push(`| ${journey} | ${viewport} | ${browser} | ${target} | ${t.settleMs} / ${t.worstSettleMs} | ${t.cls.toFixed(3)} | ${t.jumps} | ${t.flicker} | ${t.shortSpinners} | ${t.emptyFlashes} | ${t.requests} (${t.dupRequests}) | ${t.longTasks} | ${t.inputs} | ${t.focusMisses} | ${t.lost}${t.errors ? ` · ${t.errors} failed` : ''} |`);
    }
  }
  return rows.join('\n') + '\n';
}
