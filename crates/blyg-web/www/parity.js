// --- web-parity --- What the studio does that the web lacked:
//
// - Mark all read (✓ in the sources, ⌃⌥A on a desk): every unread post of
//   the chosen source and lens, after a confirmation, sent in batches of
//   500 to POST /api/reading/read; a batch that fails is put back.
// - The Posts filters (All · Drafts · Changes · Public · Withdrawn, with
//   counts) over the list; ⌃⌥F steps through them on a desk.
// - Updates (⚙ Blyg › Updates): threads whose quotes have newer versions,
//   each quote's state, and Refresh (one, or all that can be).
//
// The screens are drawn in the reading panel (web-reading.js `wrExtend`).
// Every rule is Burrow's Rust (src/parity/); this draws and fetches. Every
// request goes through live.js `call` (same-origin /api/ only). Features
// are gated on the operation names /api/capabilities advertises.

import { $, S, esc, now, persist, persistReading, toast } from './core.js';
import { call } from './live.js';
import { wrExtend, wrScreen, wrRedraw, ensure, closePanel } from './web-reading.js';

const loadingP = '<p class="wr-muted">Reading it from the blyg…</p>';
const plural = (n, one, many = `${one}s`) => `${n} ${n === 1 ? one : many}`;
const body = () => $('#wr-body');

/** An answer's `{error, code}`: sign in again on `unauthorized`, else say it. */
function said(o) {
  if (o?.code === 'unauthorized') S.live?.signIn();
  else if (o?.error) toast(esc(o.error), 4200);
}

// ---------------------------------------------------------------- mark all read
function markAllScreen() {
  if (S.live && !S.B.wr_ops_known()) ensure('capabilities');
  const a = JSON.parse(S.B.mark_all_ask(now()));
  if (!a.known) return loadingP;
  if (!a.can) return '<p class="wr-muted">Your blyg doesn’t take read marks in bulk (it doesn’t offer markReadBatch). Mark posts read as you open them.</p>';
  if (a.offline) return '<p class="wr-muted">You’re offline. Mark all read when the blyg is back.</p>';
  if (!a.n) return `<p>Nothing unread in <b>${esc(a.label)}</b>.</p>`;
  return `<p>Mark <b>${plural(a.n, 'post')}</b> read in <b>${esc(a.label)}</b>?</p>
    <p class="wr-muted">Every unread post in this source and lens, not only the ones on screen. Posts updated since you read them count too.</p>
    <div class="wr-acts"><button type="button" class="wr-btn pri" data-markall-go autofocus>Mark ${a.n} read</button>
      <button type="button" class="wr-btn" data-wr-close>Cancel</button></div>`;
}

async function markAll() {
  const o = JSON.parse(S.B.mark_all_read(now()));
  closePanel();
  if (o.error) { toast(esc(o.error), 4200); return; }
  S.render();
  let failed = 0, signIn = false;
  for (const b of o.batches) {
    const r = await call(b.req.method, b.req.path, b.req.body);
    if (r.status >= 200 && r.status < 300) continue;
    S.B.mark_all_rollback(JSON.stringify(b.prior));
    failed += b.prior.length;
    if (r.status === 401) signIn = true;
  }
  if (signIn) S.live?.signIn();
  toast(failed
    ? `Marked ${o.n - failed} of ${o.n} read · ${failed} couldn’t reach the blyg`
    : `Marked ${plural(o.n, 'post')} read`, failed ? 4200 : 2400);
  persist({ quiet: true });
  persistReading();
  S.render();
}

// ---------------------------------------------------------------- posts filters
/** The filter chips over the Posts list: `{html, on, empty, all}`. */
export function filterChips() {
  const f = JSON.parse(S.B.post_filters());
  const html = f.chips.map((c) => `<button type="button" class="pf${c.on ? ' on' : ''}" data-pf="${c.key}" aria-pressed="${c.on}" title="${esc(c.tip)}">${esc(c.label)} <b>${c.n}</b></button>`).join('');
  return { html, on: f.on, empty: f.empty, all: f.chips[0]?.n ?? 0 };
}
/** Choose filter `key` (`next` steps on). `keep`: the open post stays open. */
export function pickFilter(key, keep) {
  const label = key === 'next' ? S.B.next_post_filter(keep) : S.B.set_post_filter(key, keep);
  return label;
}

// ---------------------------------------------------------------- updates
let updLoading = null;
async function updLoad() {
  if (updLoading || !S.live) return;
  updLoading = (async () => {
    try {
      if (!S.B.wr_ops_known()) await ensure('capabilities');
      if (!S.B.wr_can('listStaleThreads')) return;
      const r = await call('GET', '/api/freshness');
      said(JSON.parse(S.B.upd_loaded(r.status, r.body)));
    } finally { updLoading = null; wrRedraw(); }
  })();
}
async function updProbe(sid) {
  const r = await call('GET', S.B.upd_probe_path(sid));
  S.B.upd_probed(sid, r.status, r.body);
  if (r.status === 401) S.live?.signIn();
  wrRedraw();
}
function updatesScreen() {
  if (!S.live) return '<p class="wr-muted">Updates lists your threads whose quotes have newer versions, on your blyg. The demo has none.</p>';
  const v = JSON.parse(S.B.upd_view());
  if (S.B.wr_ops_known() && !v.can) return noUpdates;
  if (!v.loaded) { updLoad(); return loadingP; }
  if (v.error) return `<p class="bad">${esc(v.error)}</p><div class="wr-acts"><button type="button" class="wr-btn" data-upd-reload>Try again</button></div>`;
  const s = wrScreen();
  const rows = v.rows.map((t) => `<div class="wr-card${t.failed ? ' bad' : ''}"><div class="wr-row"><span class="wr-ico" aria-hidden="true">↻</span>
      <div class="wr-l"><b>${esc(t.title)}</b><small>${esc(t.line)}</small>
      ${t.why ? `<small class="wr-muted">${esc(t.why)}</small>` : ''}${t.failed ? `<small class="bad">${esc(t.failed)}</small>` : ''}</div>
      ${v.canProbe ? `<button type="button" class="wr-btn" data-upd-probe="${esc(t.sid)}" aria-expanded="${s.open === t.sid}">${s.open === t.sid ? '▴' : 'Quotes'}</button>` : ''}
      ${v.canRefresh ? `<button type="button" class="wr-btn pri" data-upd-refresh="${esc(t.sid)}" ${t.why || s.busy ? 'disabled' : ''} title="${esc(t.why || 'Publish the same words with the quotes brought up to date: a new version')}">Refresh</button>` : ''}</div>
    ${s.open === t.sid ? `<div class="wr-manage">${t.quotes
      ? t.quotes.map((q) => `<div class="wr-row"><div class="wr-l"><b>${esc(q.label)}</b>${q.partial ? ' <small>excerpt</small>' : ''}<small class="${q.blocking ? 'bad' : ''}">${esc(q.words)}</small></div></div>`).join('') || '<p class="wr-muted">No quotes.</p>'
      : t.probe_error ? `<p class="bad">${esc(t.probe_error)}</p>` : '<p class="wr-muted">Asking the quoted blygs…</p>'}</div>` : ''}
  </div>`).join('');
  const any = v.rows.some((t) => !t.why);
  return `<p class="wr-muted">Your threads whose quotes have newer versions, stalest first. A refresh publishes the same words with the quotes brought up to date: a new version. Let small drift wait, and refresh in batches.</p>
    ${v.rows.length && v.canRefresh ? `<label class="wr-field">Version note <input name="updnote" autocomplete="off" value="${esc(v.note)}"></label>
    <div class="wr-acts top"><button type="button" class="wr-btn pri" data-upd-all ${any && !s.busy ? '' : 'disabled'}>Refresh all</button><button type="button" class="wr-btn" data-upd-reload>Check again</button></div>` : ''}
    ${rows || '<p class="wr-muted">Nothing is stale right now.</p>'}
    ${!v.rows.length ? '<div class="wr-acts"><button type="button" class="wr-btn" data-upd-reload>Check again</button></div>' : ''}`;
}
const noUpdates = '<p class="wr-muted">Your blyg doesn’t list stale quotes (it doesn’t offer listStaleThreads).</p>';

/** Refresh thread `sid`; resolves to whether it did. */
async function refreshOne(sid) {
  const note = $('[name="updnote"]', body())?.value ?? '';
  const o = JSON.parse(S.B.upd_refresh_request(sid, note));
  if (o.error) { toast(esc(o.error), 4200); return false; }
  const r = await call(o.req.method, o.req.path, o.req.body);
  const out = JSON.parse(S.B.upd_refreshed(sid, r.status, r.body));
  // The post itself, as after a publish: read again, or the conflict shown.
  const a = JSON.parse(S.B.live_command_answer(o.id, r.status, r.body));
  if (a.fetch) {
    const g = await call('GET', a.fetch);
    if (a.conflict) S.B.live_conflict(a.conflict, g.status, g.body);
    else S.B.live_apply_item(o.id, g.status, g.body, '');
  }
  await persist({ quiet: true });
  S.render();
  if (out.error) { said(out); return false; }
  return out.toast;
}
async function refreshMany(sids) {
  const s = wrScreen();
  s.busy = true; wrRedraw();
  let ok = 0, last = '';
  try {
    for (const sid of sids) { const done = await refreshOne(sid); if (done) { ok++; last = done; } }
  } finally { s.busy = false; wrRedraw(); }
  if (sids.length === 1 && ok) toast(`Refreshed · ${esc(last)}`);
  else if (sids.length > 1) toast(ok === sids.length ? `Refreshed ${plural(ok, 'thread')}` : `Refreshed ${ok} of ${sids.length} threads · the list says what went wrong`, 4200);
}

// ---------------------------------------------------------------- the panel
wrExtend({
  titles: { markall: 'Mark all read', updates: 'Updates' },
  screens: { markall: markAllScreen, updates: updatesScreen },
  async click(t, d, s) {
    if ('markallGo' in d) { markAll(); return true; }
    if ('updReload' in d) { S.B.upd_forget(); wrRedraw(); return true; }
    if (d.updProbe) {
      s.open = s.open === d.updProbe ? null : d.updProbe;
      wrRedraw();
      if (s.open) updProbe(d.updProbe);
      return true;
    }
    if (d.updRefresh) { await refreshMany([d.updRefresh]); return true; }
    if ('updAll' in d) {
      const v = JSON.parse(S.B.upd_view());
      await refreshMany(v.rows.filter((r) => !r.why).map((r) => r.sid));
      return true;
    }
    return false;
  },
});
