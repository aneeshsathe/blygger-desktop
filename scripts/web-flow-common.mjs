// Helpers both UIs' journeys share (scripts/browser-web-flow-pass.mjs).

/** The journeys, in the order they run. */
export const JOURNEYS = ['open-to-read', 'catch-up', 'quote-publish', 'draft-revise', 'subscribe', 'tab-switch', 'sync-reading', 'sync-typing', 'back-and-scroll', 'keyboard'];
export const journeyList = (ctx) => JOURNEYS.filter((j) => !ctx.only.length || ctx.only.includes(j));

/** One Studio sign-in per origin for the whole pass (its sign-in budget
    is small); every journey starts from the session. */
const sessions = new Map();
export async function signIn(ctx, origin) {
  if (sessions.has(origin)) return sessions.get(origin);
  const browser = await ctx.pw.chromium.launch();
  const c = await browser.newContext();
  const login = await c.request.post(`${origin}/studio/login`, { form: { password: ctx.password() }, headers: { origin }, maxRedirects: 0 });
  if (login.status() >= 400) throw new Error(`Studio sign-in at ${origin} failed (${login.status()})`);
  const s = await c.storageState();
  await browser.close();
  sessions.set(origin, s);
  return s;
}

export async function waitFor(fn, ms, what) {
  const until = Date.now() + ms;
  for (;;) {
    if (await fn().catch(() => false)) return;
    if (Date.now() > until) throw new Error(`waited ${ms} ms for ${what}`);
    await new Promise((r) => setTimeout(r, 150));
  }
}

let late = 0;
/** n new posts on b, and blyg `a` told to read b again, until they're in
    its reading list: what a background sync brings in. */
export async function newPostsOnB(ctx, a, n) {
  const ids = [];
  for (let i = 0; i < n; i++) ids.push(await ctx.b.post(`Late invented note ${++late}: it arrived while you were reading.`));
  const subs = await a.all('/subscriptions');
  const bSub = subs.find((s) => (s.feedUrl || s.origin || s.url || '').includes(new URL(ctx.b.base).port)) ?? subs.find((s) => s.kind === 'blyg');
  if (bSub) await a.call('POST', `/subscriptions/${bSub.id}/resync`, undefined, { ok: true });
  await waitFor(async () => {
    const rows = await a.call('GET', '/reading/imported?limit=50');
    return (rows.items || []).filter((r) => /Late invented note/.test(r.contentMd || r.title || '')).length >= late;
  }, 30_000, 'b\'s new posts to be imported');
  return ids;
}

/** The feed site no blyg follows at the start of the subscribe journey. */
export function spareFeed(ctx) { return ctx.feeds[ctx.feeds.length - 1]; }
export const spareName = (ctx) => ctx.feedNames[ctx.feedNames.length - 1];

/** Unsubscribe `a` from `url` if it follows it (the journey subscribes). */
export async function unsubscribe(a, url) {
  const subs = await a.all('/subscriptions');
  const port = new URL(url).port;
  for (const s of subs) {
    const where = JSON.stringify([s.origin, s.feedUrl, s.url, s.siteUrl]);
    if (where.includes(`:${port}`)) await a.call('DELETE', `/subscriptions/${s.id}`, undefined, { ok: true });
  }
}

/** Every imported post unread again, so a catch-up journey has a list to
    read down (POST /reading/unread, 500 at a time). */
export async function markAllUnread(a) {
  const rows = await a.all('/reading/imported');
  for (let i = 0; i < rows.length; i += 500) {
    await a.call('POST', '/reading/unread', { items: rows.slice(i, i + 500).map((r) => ({ sub: r.subscriptionId, remoteId: r.remoteId })) }, { ok: true });
  }
  return rows.length;
}
export async function unreadOn(a) {
  return (await a.all('/reading/imported')).filter((r) => !r.readVersion || r.readVersion < r.version).length;
}
