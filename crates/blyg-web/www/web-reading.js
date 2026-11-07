// --- web-reading --- The Reader's side of Studio, in a browser: sources
// (smart feeds, Background, Mentions, hoppers, subscriptions) and the
// Threads / Fragments lens over the stream; subscriptions (subscribe with a
// preview, pause, rename, blogroll, unsubscribe, check now, resync, OPML in
// and out); mentions (yours, hide one, what you sent, the accept toggle);
// hoppers (make, rename, describe, public, delete); thumbs; peeking at a
// post you don't follow, and the browsing history.
//
// Every rule is Burrow's Rust (src/web_reading.rs, with the app's
// reading/sources_vm.rs); this draws and fetches. Live, every request goes
// through live.js `call` (same-origin /api/ only). Nothing here runs on
// start or on a keystroke: a list is read when its screen first opens.
//
// Desk: a sources pane left of the stream, with Subscribe, Subscriptions,
// Mentions, Hoppers, Peek and History named at its top. Phone: the same
// names in a row under the Read tab's search ("☰ Sources" opens the list
// as a sheet). Both layouts also list them on the Blyg tab (hub.js).

import { Pager } from './pkg/blyg_web.js';
import { $, $$, S, esc, now, persist, toast, isPhone } from './core.js';
import { call } from './live.js';

const panel = () => $('#wr-panel');
const body = () => $('#wr-body');
let screen = null;      // the panel's screen: { name, ...state }
let stack = [];         // screens to go back to
let back = null;        // focus before the panel opened
let use = {};           // the layouts' own: { phone: {openReading}, desk: {openReading} }
const loadedAt = {};    // list → when it was read (ms)
const loading = {};     // list → its read in flight

/** A layout hands over what the panel needs of it (`openReading`). */
export function wrUse(layout, fns) { use[layout] = fns; }

// ---------------------------------------------------------------- lists
/** Read `which` (subs, mentions, outbound, lens, history, settings,
    capabilities) if it isn't here or is older than `maxAge` ms. The demo's
    are here already. Resolves when it's read; redraws the panel. */
export async function ensure(which, maxAge = Infinity) {
  if (!S.B) return;
  if (!S.live) { S.B.wr_demo_lists(now()); return; }
  const at = loadedAt[which], has = S.B.wr_has(which);
  if (at === undefined ? has : Date.now() - at < (has ? maxAge : 15_000)) return;
  if (S.B.is_offline()) return;
  loading[which] ??= (async () => {
    try {
      const spec = JSON.parse(S.B.wr_list(which));
      if (!spec) return;
      let text;
      if (spec.one) {
        const r = await call('GET', spec.path + (spec.query ? `?${spec.query}` : ''));
        if (r.status !== 200) { failed(r); return; }
        text = r.body;
      } else {
        const p = new Pager(spec.path, spec.query, spec.limit);
        try {
          for (let next = p.next_path(); next !== undefined; next = p.next_path()) {
            const r = await call('GET', next);
            const err = JSON.parse(p.feed(r.status, r.body, r.retryAfter));
            if (err) { if (err.code !== 'not-found') toast(esc(err.text), 4200); return; }
          }
          text = p.items();
        } finally { p.free(); }
      }
      const out = JSON.parse(S.B.wr_loaded(which, text));
      if (out.error) toast(esc(out.error), 4200);
    } finally { loadedAt[which] = Date.now(); delete loading[which]; }
  })();
  await loading[which];
  redraw();
}
function failed(r) {
  const e = JSON.parse(S.B.constructor.api_error(r.status, r.body));
  if (e?.code === 'unauthorized') S.live?.signIn();
  else if (e && e.code !== 'not-found') toast(esc(e.text), 4200);
}

// ---------------------------------------------------------------- actions
/** Run an action: live, its request goes to the blyg and the answer back
    to Burrow; in the demo, the sample data changes. Returns the outcome. */
export async function act(action, args = {}) {
  let o = JSON.parse(S.B.wr_do(action, JSON.stringify(args), now()));
  if (o.open) { openHeld(o.open); return o; }
  if (o.req) {
    if (o.prev !== undefined) args = { ...args, prev: o.prev };
    redraw();
    const r = await call(o.req.method, o.req.path, o.req.body);
    o = JSON.parse(S.B.wr_done(action, JSON.stringify(args), r.status, r.body, now()));
    if (o.code === 'unauthorized') S.live?.signIn();
  }
  if (o.error) toast(esc(o.error), 4200);
  if (o.toast) toast(esc(o.toast), 3200);
  if (o.fire) call(o.fire.method, o.fire.path, o.fire.body);
  for (const what of o.reload ?? []) reload(what);
  if (o.next) return act(o.next.action, o.next.args);
  if (S.live && !o.error) persist({ quiet: true });
  S.render();
  redraw();
  return o;
}
function reload(what) {
  const later = what.endsWith('-later');
  const list = what.replace(/-later$/, '');
  const go = () => {
    if (list === 'reading') { S.B.wr_forget('lens'); S.live?.refresh(); }
    else { S.B.wr_forget(list); delete loadedAt[list]; ensure(list); }
  };
  if (later) setTimeout(go, 8000); else go();
}

/** A held post: open it in the reader. */
function openHeld(key) {
  closePanel();
  use[S.layout]?.openReading?.(key);
}

// ---------------------------------------------------------------- sources
/** The sources list (the desk's pane, the phone's sheet). */
function sourcesHtml(d, { manage = true } = {}) {
  const lens = (k, label) => `<button type="button" data-lens="${k}" class="${d.lens === k ? 'on' : ''}" aria-pressed="${d.lens === k}">${label}</button>`;
  const rows = d.rows.map((r) => {
    if (r.kind === 'heading') return `<div class="wr-h">${esc(r.label)}</div>`;
    const mark = r.count != null ? `<span class="wr-n">${r.count}</span>` : r.dot ? '<span class="wr-dot" aria-label="something to read"></span>' : '';
    const icon = r.kind === 'sub' ? `<span class="wr-let" aria-hidden="true">${esc(r.letter)}</span>`
      : r.kind === 'hopper' ? '<span class="wr-ico" aria-hidden="true">⊕</span>'
      : r.kind === 'mentions' ? '<span class="wr-ico" aria-hidden="true">@</span>'
      : '';
    return `<button type="button" class="wr-src${r.on ? ' on' : ''}" data-src="${esc(r.key)}" aria-pressed="${r.on}">${icon}<span class="wr-l">${esc(r.label)}${r.note ? `<small class="${/trouble/i.test(r.note) ? 'bad' : ''}">${esc(r.note)}</small>` : ''}</span>${mark}</button>`;
  }).join('');
  return `${manage ? `<div class="wr-more" role="toolbar" aria-label="Subscriptions and more">
      <button type="button" class="pri" data-go="subscribe">＋ Subscribe</button>
      <button type="button" data-go="subs">☷ Subscriptions</button>
      <button type="button" data-go="mentions">@ Mentions</button>
      <button type="button" data-go="hoppers">⊕ Hoppers</button>
      <button type="button" data-go="peek-url">⌕ Peek</button>
      <button type="button" data-go="history">⟲ History</button>
    </div>` : ''}
    <div class="wr-lens" role="group" aria-label="Lens">${lens('all', 'All')}${lens('thread', 'Threads')}${lens('fragment', 'Fragments')}</div>
    ${rows}`;
}
function chooseSource(key) {
  S.B.wr_set_source(key);
  if (key === 'mentions') ensure('lens').then(() => S.render());
  for (const s of ['#p-stream', '#d-stream']) { const el = $(s); if (el) el.scrollTop = 0; }
  S.render();
}
function chooseLens(lens) {
  S.B.wr_set_lens(lens);
  for (const s of ['#p-stream', '#d-stream']) { const el = $(s); if (el) el.scrollTop = 0; }
  S.render();
}
/** Clicks in a sources list: a source, the lens, or a "More" screen. */
function sourcesClick(e, { close = false } = {}) {
  const src = e.target.closest('[data-src]');
  if (src) { chooseSource(src.dataset.src); if (close) closePanel(); else redraw(); return true; }
  const l = e.target.closest('[data-lens]');
  if (l) { chooseLens(l.dataset.lens); redraw(); return true; }
  const g = e.target.closest('[data-go]');
  if (g) { go(g.dataset.go); return true; }
  return false;
}

let paneReady = false;
/** Desk: the sources pane, drawn with the reading view (and only then). */
export function deskSources() {
  const host = $('#d-wr-sources');
  if (!host || !S.B) return;
  if (!paneReady) {
    paneReady = true;
    host.addEventListener('click', (e) => sourcesClick(e));
    ensure('subs');
  }
  const html = sourcesHtml(JSON.parse(S.B.wr_sources(now())));
  if (host._html !== html) { host.innerHTML = html; host._html = html; }
}

/** Phone: the line under the search saying what the stream shows. */
export function phoneChip() {
  const chip = $('#p-wrChip');
  if (!chip || !S.B) return;
  const src = S.B.wr_source();
  const label = S.B.wr_source_label();
  const filtered = src !== 'smart:all' || label !== 'All';
  chip.hidden = !filtered;
  const html = filtered ? `<span>Showing <b>${esc(label)}</b></span><button type="button" data-clear aria-label="Show everything">✕</button>` : '';
  if (chip._html !== html) { chip.innerHTML = html; chip._html = html; }
}

// ---------------------------------------------------------------- the panel
/** Open the panel on `name` (sources, subs, subscribe, mentions, hoppers,
    peek-url, peek, history, opml), stacking the current one for ‹. */
export function go(name, state = {}) {
  const p = panel();
  if (p.hidden) {
    back = document.activeElement;
    p.hidden = false;
    stack = [];
    document.documentElement.classList.add('wr-open');
  } else if (screen && screen.name !== name) stack.push(screen);
  screen = { name, ...state };
  draw({ focus: true });
}
function goBack() {
  if (!stack.length) { closePanel(); return; }
  screen = stack.pop();
  draw({ focus: true });
}
export function closePanel() {
  const p = panel();
  if (!p || p.hidden) return;
  p.hidden = true;
  screen = null; stack = [];
  document.documentElement.classList.remove('wr-open');
  if (back && document.contains(back)) back.focus?.({ preventScroll: true });
  back = null;
}
function redraw() { if (screen) draw(); }

const TITLES = {
  sources: 'Sources', subs: 'Subscriptions', subscribe: 'Subscribe', mentions: 'Mentions',
  hoppers: 'Hoppers', 'peek-url': 'Peek at a post', peek: 'Peek', history: 'History', opml: 'Import OPML',
};

function draw({ focus = false } = {}) {
  if (!screen) return;
  $('#wr-h').textContent = screen.name === 'peek' && screen.view ? screen.view.host : TITLES[screen.name];
  $('#wr-back').hidden = !stack.length;
  const html = SCREENS[screen.name]?.() ?? '';
  const b = body();
  // Keep what's typed and the scroll across a redraw.
  const typed = $$('input[name], textarea[name]', b).map((el) => [el.name, el.value, el === document.activeElement, el.selectionStart]);
  const top = b.scrollTop;
  if (b._html !== html) {
    b.innerHTML = html; b._html = html;
    for (const [n, val, had, at] of typed) {
      const el = $(`[name="${n}"]`, b);
      if (!el || el.value === val) continue;
      el.value = val;
      if (had) { el.focus({ preventScroll: true }); try { el.setSelectionRange(at, at); } catch {} }
    }
    b.scrollTop = focus ? 0 : top;
  }
  if (focus) ($('[autofocus]', b) || $('.wr-x', panel()))?.focus({ preventScroll: true });
}

const loadingP = '<p class="wr-muted">Reading it from the blyg…</p>';
const SCREENS = {
  sources() {
    ensure('subs', 120_000);
    return `<div class="wr-sources in-sheet">${sourcesHtml(JSON.parse(S.B.wr_sources(now())), { manage: false })}</div>`;
  },

  subs() {
    ensure('subs', 60_000);
    const v = JSON.parse(S.B.wr_subs_view(now()));
    const rows = v.rows.map((s) => {
      const open = screen.open === s.id;
      return `<div class="wr-card${s.trouble ? ' bad' : ''}${s.paused ? ' paused' : ''}">
        <div class="wr-row"><span class="wr-let" aria-hidden="true">${esc(s.name.slice(0, 1).toUpperCase())}</span>
          <div class="wr-l"><b>${esc(s.name)}</b><small>${esc(s.host)} · ${s.kind}${s.held ? ` · ${s.held} held` : ''}${s.in_blogroll ? ' · on your blogroll' : ''}</small>
          ${s.state ? `<small class="${s.trouble ? 'bad' : ''}">${esc(s.state)}</small>` : ''}</div>
          <button type="button" class="wr-btn" data-show="${esc(s.id)}">Posts</button>
          <button type="button" class="wr-btn" data-open="${esc(s.id)}" aria-expanded="${open}" aria-label="Manage ${esc(s.name)}">${open ? '▴' : '⋯'}</button></div>
        ${open ? `<div class="wr-manage">
          <label class="wr-field">Name <input name="name-${esc(s.id)}" value="${esc(s.name)}" autocomplete="off"></label>
          <div class="wr-acts"><button type="button" class="wr-btn" data-rename="${esc(s.id)}">Rename</button>
            ${s.own_title ? `<button type="button" class="wr-btn" data-ownname="${esc(s.id)}">Use its own name</button>` : ''}</div>
          <div class="wr-acts">
            <button type="button" class="wr-btn" data-pause="${esc(s.id)}" data-on="${!s.paused}">${s.paused ? 'Resume' : 'Pause'}</button>
            <button type="button" class="wr-btn" data-blogroll="${esc(s.id)}" data-on="${!s.in_blogroll}" aria-pressed="${s.in_blogroll}">${s.in_blogroll ? '✓ On your blogroll' : 'Add to your blogroll'}</button>
            ${s.kind === 'blyg' ? `<button type="button" class="wr-btn" data-resync="${esc(s.id)}" title="Read every post of it again">Read again</button>` : ''}
            <button type="button" class="wr-btn danger" data-unsub="${esc(s.id)}">${screen.confirm === s.id ? 'Yes, unsubscribe' : 'Unsubscribe'}</button>
          </div>
          ${s.feed_url ? `<small class="wr-muted">${esc(s.feed_url)}</small>` : ''}
        </div>` : ''}</div>`;
    }).join('');
    return `<div class="wr-acts top">
        <button type="button" class="wr-btn pri" data-go="subscribe">＋ Subscribe</button>
        <button type="button" class="wr-btn" data-poll>Check all now</button>
        <button type="button" class="wr-btn" data-opml-in>Import OPML…</button>
        <button type="button" class="wr-btn" data-opml-out>Export OPML</button></div>
      ${!v.loaded ? loadingP : ''}
      ${rows || (v.loaded ? '<p class="wr-muted">No subscriptions yet. ＋ Subscribe to a blyg or a feed.</p>' : '')}`;
  },

  subscribe() {
    const p = screen.preview;
    return `<form class="wr-form" data-form="subscribe">
        <label class="wr-field">A blyg's or a feed's address
          <input name="url" type="url" inputmode="url" autocomplete="off" autocapitalize="off" spellcheck="false" placeholder="https://blyg.example.com" autofocus value="${esc(screen.url ?? '')}"></label>
        ${p ? `<div class="wr-card"><div class="wr-l"><b>${esc(p.title || p.origin || '')}</b>
            <small>${p.kind === 'rss' ? 'a feed' : p.kind === 'hopper' ? 'a hopper' : 'a blyg'}${p.origin ? ` · ${esc(p.origin)}` : ''}</small>
            ${p.feedUrl ? `<small class="wr-muted">${esc(p.feedUrl)}</small>` : ''}
            ${p.hopper ? `<small>That's the hopper “${esc(p.hopper)}”. Burrow follows the whole blyg.</small>` : ''}
            ${p.mismatch ? `<small class="bad">⚠ ${esc(p.mismatch)}</small>` : ''}</div>
          <label class="wr-field">Call it <input name="title" autocomplete="off" value="${esc(p.title ?? '')}"></label></div>` : ''}
        <div class="wr-acts">${p
          ? '<button type="submit" class="wr-btn pri" data-sub>Subscribe</button><button type="button" class="wr-btn" data-repreview>Look again</button>'
          : '<button type="submit" class="wr-btn pri">Look it up</button>'}</div>
        ${screen.busy ? '<p class="wr-muted">Asking the blyg…</p>' : ''}
      </form>`;
  },

  mentions() {
    ensure('mentions', 60_000); ensure('outbound', 60_000); ensure('settings', 300_000);
    const v = JSON.parse(S.B.wr_mentions_view(now()));
    const groups = v.groups.map((g) => `<div class="wr-group"><div class="wr-h">${esc(g.title)}</div>
      ${g.rows.map((m) => `<div class="wr-row wr-mention${m.hidden ? ' hidden' : ''}">
        ${m.new ? '<span class="wr-dot" aria-label="new"></span>' : ''}
        <div class="wr-l"><b>${esc(m.who)}</b> ${esc(m.relation)} <small>${esc(m.origin)} · ${esc(m.when)}${m.status ? ` · ${esc(m.status)}` : ''}${m.hidden ? ' · hidden' : ''}</small></div>
        <a class="wr-btn" href="${esc(m.source)}" target="_blank" rel="noopener noreferrer">Open</a>
        <button type="button" class="wr-btn" data-hide="${esc(m.id)}" data-on="${!m.hidden}">${m.hidden ? 'Show' : 'Hide'}</button></div>`).join('')}</div>`).join('');
    const out = v.outbound.map((o) => `<div class="wr-row${o.failed ? ' bad' : ''}"><div class="wr-l"><b>${esc(o.title)}</b> → ${esc(o.host)}
        <small>${esc(o.status)} · ${esc(o.when)}${o.error ? ` · ${esc(o.error)}` : ''}</small></div>
        <a class="wr-btn" href="${esc(o.target)}" target="_blank" rel="noopener noreferrer">Open</a></div>`).join('');
    return `<label class="wr-toggle"><input type="checkbox" data-accept ${v.accept ? 'checked' : ''} ${v.accept == null ? 'disabled' : ''}>
        Accept Webmentions <small>Other sites can tell your blyg they replied to, quoted or linked a post.</small></label>
      <div class="wr-h">Responses to your posts</div>
      ${!v.loaded ? loadingP : groups || '<p class="wr-muted">No one has responded to a post of yours yet.</p>'}
      <div class="wr-h">What you sent</div>
      ${!v.outboundLoaded ? loadingP : out || '<p class="wr-muted">Nothing sent yet. Publishing a post that links another site tells it.</p>'}`;
  },

  hoppers() {
    const hs = JSON.parse(S.B.wr_hoppers_view());
    const rows = hs.map((h) => {
      const open = screen.open === h.id;
      return `<div class="wr-card"><div class="wr-row"><span class="wr-ico" aria-hidden="true">⊕</span>
          <div class="wr-l"><b>${esc(h.name)}</b><small>${h.held} held here${h.public ? ' · public' : ' · private'}</small>
          ${h.description ? `<small>${esc(h.description)}</small>` : ''}
          ${h.url ? `<small><a href="${esc(h.url)}" target="_blank" rel="noopener">${esc(h.url.replace(/^https?:\/\//, ''))}</a></small>` : ''}</div>
          <button type="button" class="wr-btn" data-show-hop="${esc(h.id)}">Posts</button>
          <button type="button" class="wr-btn" data-open="${esc(h.id)}" aria-expanded="${open}" aria-label="Edit ${esc(h.name)}">${open ? '▴' : '⋯'}</button></div>
        ${open ? `<div class="wr-manage">
          <label class="wr-field">Name <input name="hname-${esc(h.id)}" value="${esc(h.name)}" autocomplete="off"></label>
          <label class="wr-field">Description <textarea name="hdesc-${esc(h.id)}" maxlength="280" rows="2" placeholder="One line, shown on its page">${esc(h.description)}</textarea></label>
          <label class="wr-toggle"><input type="checkbox" name="hpub-${esc(h.id)}" ${h.public ? 'checked' : ''}> Public <small>Anyone can read it at your blyg's /h/ page and follow its feed.</small></label>
          <div class="wr-acts"><button type="button" class="wr-btn pri" data-hsave="${esc(h.id)}">Save</button>
            <button type="button" class="wr-btn danger" data-hdel="${esc(h.id)}">${screen.confirm === h.id ? 'Yes, delete it' : 'Delete'}</button></div>
          <small class="wr-muted">Deleting a hopper keeps its posts.</small></div>` : ''}</div>`;
    }).join('');
    return `<form class="wr-form wr-inline" data-form="hopper"><input name="newhop" placeholder="A new hopper's name" autocomplete="off" aria-label="A new hopper's name">
        <button type="submit" class="wr-btn pri">Make</button></form>
      ${rows || '<p class="wr-muted">No hoppers yet. A hopper collects posts, yours and theirs; make one public to share it.</p>'}`;
  },

  'peek-url'() {
    return `<form class="wr-form" data-form="peek">
        <label class="wr-field">A post's address on its blyg
          <input name="peekurl" type="url" inputmode="url" autocomplete="off" autocapitalize="off" spellcheck="false" placeholder="https://blyg.example.com/t/…" autofocus></label>
        <div class="wr-acts"><button type="submit" class="wr-btn pri">Peek</button></div>
        <p class="wr-muted">Read a post from a blyg you don't follow, without subscribing. Links in posts you read open here too.</p>
      </form>`;
  },

  peek() {
    const p = screen.view;
    if (!p) return loadingP;
    return `<article class="post wr-peek"><div class="by">${esc(p.host)} · ${esc(p.kind)} · v${p.version}</div>
        <div class="item-content">${p.html}</div></article>
      <div class="wr-acts">${p.url ? `<a class="wr-btn" href="${esc(p.url)}" target="_blank" rel="noopener noreferrer">Open on its blyg</a>` : ''}
        ${p.followed ? '<span class="wr-muted">You follow this blyg</span>' : `<button type="button" class="wr-btn pri" data-follow="${esc(p.origin)}">Subscribe to ${esc(p.host)}</button>`}</div>`;
  },

  history() {
    ensure('history', 30_000);
    const v = JSON.parse(S.B.wr_history_view(now()));
    return `${!v.loaded ? loadingP : v.rows.length
      ? v.rows.map((r) => `<button type="button" class="wr-row wr-hist" data-peek-o="${esc(r.origin)}" data-peek-id="${esc(r.id)}"><div class="wr-l"><b>${esc(r.title)}</b><small>${esc(r.host)} · ${esc(r.when)}</small></div></button>`).join('')
        + `<div class="wr-acts"><button type="button" class="wr-btn danger" data-clear-hist>${screen.confirm ? 'Yes, clear it' : 'Clear history'}</button></div>`
      : '<p class="wr-muted">Nothing yet. Posts you peek at are listed here.</p>'}`;
  },

  opml() {
    const o = screen.opml;
    if (!o) return '';
    if (o.error) return `<p class="bad">${esc(o.error)}</p>`;
    const fresh = o.feeds.filter((f) => !f.followed);
    const picked = screen.picked ?? new Set(fresh.map((f) => f.url));
    screen.picked = picked;
    const run = screen.run;
    return `<p>${o.feeds.length} feed${o.feeds.length === 1 ? '' : 's'} in the file${o.duplicates ? `, ${o.duplicates} listed twice` : ''}${o.withoutFeed ? `, ${o.withoutFeed} without a feed address (skipped)` : ''}. ${fresh.length ? '' : 'You follow every one already.'}</p>
      ${run ? `<p class="wr-prog">${run.done} of ${run.total}${run.failed ? ` · ${run.failed} failed` : ''}${run.over ? ' · done' : '…'}</p>` : ''}
      <div class="wr-opml">${o.feeds.map((f) => `<label class="wr-row${f.followed ? ' paused' : ''}"><input type="checkbox" data-feed="${esc(f.url)}" ${f.followed ? 'disabled' : picked.has(f.url) ? 'checked' : ''}>
        <div class="wr-l"><b>${esc(f.title)}</b><small>${esc(f.url)}${f.folder ? ` · ${esc(f.folder)}` : ''}${f.followed ? ' · following' : ''}${run?.errors?.[f.url] ? ` · <span class="bad">${esc(run.errors[f.url])}</span>` : ''}</small></div></label>`).join('')}</div>
      <div class="wr-acts">${run && !run.over
        ? '<button type="button" class="wr-btn" data-opml-stop>Stop</button>'
        : `<button type="button" class="wr-btn pri" data-opml-go ${picked.size ? '' : 'disabled'}>Subscribe to ${picked.size}</button>`}</div>`;
  },
};

// ---------------------------------------------------------------- the panel's events
async function onClick(e) {
  if (e.target.closest('[data-wr-close]')) { closePanel(); return; }
  if (e.target.closest('#wr-back')) { goBack(); return; }
  if (screen?.name === 'sources' && sourcesClick(e, { close: true })) return;
  const t = e.target.closest('button, [data-feed]');
  if (!t) return;
  const d = t.dataset;
  if (d.go) { go(d.go); return; }
  // subscriptions
  if (d.show) { chooseSource(`sub:${d.show}`); closePanel(); return; }
  if (d.showHop) { chooseSource(`hopper:${d.showHop}`); closePanel(); return; }
  if (d.open) { screen.open = screen.open === d.open ? null : d.open; screen.confirm = null; draw(); return; }
  if (d.rename) { await act('rename', { id: d.rename, title: $(`[name="name-${CSS.escape(d.rename)}"]`, body())?.value ?? '' }); return; }
  if (d.ownname) { await act('rename', { id: d.ownname, title: '' }); return; }
  if (d.pause) { await act('pause', { id: d.pause, on: d.on === 'true' }); return; }
  if (d.blogroll) { await act('blogroll', { id: d.blogroll, on: d.on === 'true' }); return; }
  if (d.resync) { await act('resync', { id: d.resync }); return; }
  if (d.unsub) {
    if (screen.confirm !== d.unsub) { screen.confirm = d.unsub; draw(); return; }
    screen.confirm = null; screen.open = null;
    await act('unsubscribe', { id: d.unsub });
    return;
  }
  if ('poll' in d) { await act('poll'); return; }
  if ('opmlIn' in d) { $('#wr-opml-file').click(); return; }
  if ('opmlOut' in d) { download(S.B.wr_opml_export(), 'burrow-subscriptions.opml'); return; }
  // subscribe
  if ('repreview' in d) { screen.preview = null; draw(); return; }
  // mentions
  if (d.hide) { await act('mention_hide', { id: d.hide, on: d.on === 'true' }); return; }
  // hoppers
  if (d.hsave) {
    const id = d.hsave, f = (n) => $(`[name="${n}-${CSS.escape(id)}"]`, body());
    const o = await act('hopper_edit', { id, name: f('hname').value, description: f('hdesc').value, public: f('hpub').checked });
    if (!o.error) { screen.open = null; draw(); }
    return;
  }
  if (d.hdel) {
    if (screen.confirm !== d.hdel) { screen.confirm = d.hdel; draw(); return; }
    screen.confirm = null; screen.open = null;
    await act('hopper_delete', { id: d.hdel });
    return;
  }
  // peek, history
  if (d.follow) { go('subscribe', { url: d.follow }); return; }
  if (d.peekO) { peek(d.peekO, d.peekId, 'start'); return; }
  if ('clearHist' in d) {
    if (!screen.confirm) { screen.confirm = true; draw(); return; }
    screen.confirm = false;
    await act('history_clear');
    return;
  }
  // OPML
  if ('opmlGo' in d) { importOpml(); return; }
  if ('opmlStop' in d) { if (screen.run) screen.run.stop = true; return; }
}

async function onSubmit(e) {
  const f = e.target.closest('form[data-form]');
  if (!f) return;
  e.preventDefault();
  const kind = f.dataset.form;
  if (kind === 'subscribe') {
    const url = f.url.value.trim();
    if (!screen.preview || screen.url !== url) {
      screen.url = url; screen.busy = true; draw();
      if (S.live && !S.B.wr_has('capabilities')) await ensure('capabilities');
      const o = await act('preview', { url });
      if (screen?.name !== 'subscribe') return;
      screen.busy = false;
      screen.preview = o.preview ?? null;
      if (!o.preview && o.toast) { closePanel(); return; } // subscribed without asking
      draw();
      $('[name="title"]', body())?.focus();
      return;
    }
    screen.busy = true; draw();
    const o = await act('subscribe', { url, title: f.title?.value ?? '' });
    if (screen?.name !== 'subscribe') return;
    screen.busy = false;
    if (o.error) { draw(); return; }
    screen.preview = null; screen.url = '';
    goBack();
    return;
  }
  if (kind === 'hopper') {
    const o = await act('hopper_new', { name: f.newhop.value });
    if (!o.error) { f.newhop.value = ''; draw(); }
    return;
  }
  if (kind === 'peek') {
    const url = f.peekurl.value.trim();
    screen.busy = true;
    const o = await act('peek_url', { url });
    if (o.peek) showPeek(o.peek);
  }
}

/** Read a post you don't follow, in the panel. */
export async function peek(origin, id, via = 'link') {
  if (!panel().hidden && screen?.name === 'peek') screen.view = null;
  else go('peek', { view: null });
  draw();
  const o = await act('peek', { origin, id, via });
  if (o.peek) showPeek(o.peek);
  else if (!o.open && screen?.name === 'peek' && !screen.view) goBack();
}
function showPeek(view) {
  if (screen?.name === 'peek') { screen.view = view; draw({ focus: true }); }
  else go('peek', { view });
  const art = $('.wr-peek .item-content', body());
  if (art) { $$('a', art).forEach((a) => { a.target = '_blank'; a.rel = 'noopener noreferrer'; }); classifyLinks(art); }
}

function download(text, name) {
  const a = document.createElement('a');
  a.href = URL.createObjectURL(new Blob([text], { type: 'text/x-opml' }));
  a.download = name;
  document.body.append(a); a.click(); a.remove();
  setTimeout(() => URL.revokeObjectURL(a.href), 1000);
}

async function importOpml() {
  const o = screen.opml;
  const urls = o.feeds.filter((f) => !f.followed && screen.picked.has(f.url));
  const run = screen.run = { done: 0, total: urls.length, failed: 0, errors: {}, stop: false, over: false };
  draw();
  if (S.live && !S.B.wr_has('capabilities')) await ensure('capabilities');
  for (const f of urls) {
    if (run.stop) break;
    for (let tries = 0; tries < 3; tries++) {
      let x = JSON.parse(S.B.wr_do('subscribe', JSON.stringify({ url: f.url, title: f.title }), now()));
      if (x.req) {
        const r = await call(x.req.method, x.req.path, x.req.body);
        if (r.status === 429) { await new Promise((ok) => setTimeout(ok, (+r.retryAfter || 10) * 1000)); continue; }
        x = JSON.parse(S.B.wr_done('subscribe', JSON.stringify({ url: f.url }), r.status, r.body, now()));
      }
      if (x.error) { run.failed++; run.errors[f.url] = x.error; }
      break;
    }
    run.done++;
    if (screen?.run === run) draw();
  }
  run.over = true;
  if (screen?.run === run) {
    o.feeds = JSON.parse(S.B.wr_opml(screen.text)).feeds ?? o.feeds;
    screen.picked = new Set();
    draw();
  }
  toast(`Subscribed to ${run.done - run.failed} of ${run.total}${run.failed ? ` · ${run.failed} failed` : ''}`, 4200);
  if (S.live) setTimeout(() => S.live.refresh(), 4000);
  S.render();
}

// ---------------------------------------------------------------- a post you read
/** After a reading post is drawn into `host` (`key`): 👍 / 👎 under it, and
    its links to blyg posts open in Peek. */
export function opened(key, host) {
  if (!S.B || !host) return;
  const x = JSON.parse(S.B.wr_post_extra(key));
  host.querySelector('.wr-postbar')?.remove();
  if (x.can) {
    const bar = document.createElement('div');
    bar.className = 'wr-postbar';
    bar.dataset.key = key;
    bar.innerHTML = thumbs(x.thumb);
    host.append(bar);
  }
  if (!host._wr) {
    host._wr = true;
    host.addEventListener('click', async (e) => {
      const t = e.target.closest('.wr-postbar [data-thumb]');
      if (t) {
        const bar = t.closest('.wr-postbar');
        const cur = JSON.parse(S.B.wr_post_extra(bar.dataset.key)).thumb;
        const want = +t.dataset.thumb === cur ? 0 : +t.dataset.thumb;
        const o = act('thumb', { key: bar.dataset.key, thumb: want });
        bar.innerHTML = thumbs(want || null);
        await o;
        bar.innerHTML = thumbs(JSON.parse(S.B.wr_post_extra(bar.dataset.key)).thumb);
        return;
      }
      const a = e.target.closest('a[data-wr-id]');
      if (a && !e.metaKey && !e.ctrlKey && !e.shiftKey) { e.preventDefault(); peek(a.dataset.wrOrigin, a.dataset.wrId, 'link'); }
    });
  }
  const content = host.querySelector('.item-content');
  if (content) classifyLinks(content);
}
function thumbs(t) {
  return `<button type="button" data-thumb="1" aria-pressed="${t === 1}" class="${t === 1 ? 'on' : ''}" title="Thumbs up: it goes in Thumbed">👍</button><button type="button" data-thumb="-1" aria-pressed="${t === -1}" class="${t === -1 ? 'on' : ''}" title="Thumbs down">👎</button>`;
}

const linkTargets = new Map(); // url → {origin, id} | null
/** Ask the blyg which of these links are blyg posts (one request, after
    the post is shown), and mark them to open in Peek. */
async function classifyLinks(root) {
  if (!S.live || S.B.is_offline()) return;
  const links = $$('a[href]', root).filter((a) => /^https?:/i.test(a.href) && new URL(a.href).origin !== location.origin && !a.closest('blockquote.blyg-transclusion'));
  if (!links.length) return;
  const ask = [...new Set(links.map((a) => a.href))].filter((u) => !linkTargets.has(u)).slice(0, 50);
  if (ask.length) {
    const r = await call('POST', '/api/peek/links', { urls: ask });
    if (r.status !== 200) return;
    try {
      for (const l of JSON.parse(r.body).links ?? []) linkTargets.set(l.url, l.target?.origin && l.target?.id ? l.target : null);
    } catch { return; }
    for (const u of ask) if (!linkTargets.has(u)) linkTargets.set(u, null);
  }
  for (const a of links) {
    const t = linkTargets.get(a.href);
    if (t) { a.dataset.wrOrigin = t.origin; a.dataset.wrId = t.id; a.title = a.title || 'Opens in Peek'; }
  }
}

// ---------------------------------------------------------------- start
export function initWebReading() {
  const p = panel();
  if (!p) return;
  p.addEventListener('click', onClick);
  p.addEventListener('submit', onSubmit);
  p.addEventListener('change', (e) => {
    const t = e.target;
    if (t.matches('[data-accept]')) act('accept_mentions', { on: t.checked });
    else if (t.matches('[data-feed]') && screen?.picked) {
      if (t.checked) screen.picked.add(t.dataset.feed); else screen.picked.delete(t.dataset.feed);
      draw();
    }
  });
  // Esc closes the panel before anything else sees it.
  document.addEventListener('keydown', (e) => {
    if (e.key !== 'Escape' || p.hidden) return;
    e.preventDefault(); e.stopImmediatePropagation();
    if (stack.length) goBack(); else closePanel();
  }, true);
  // Keys typed in the panel belong to it, not to the window's shortcuts.
  p.addEventListener('keydown', (e) => { if (e.key !== 'Escape') e.stopPropagation(); });
  $('#wr-opml-file').addEventListener('change', async (e) => {
    const f = e.target.files?.[0];
    e.target.value = '';
    if (!f) return;
    const text = await f.text();
    const o = JSON.parse(S.B.wr_opml(text));
    go('opml', { opml: o, text });
  });
  // Phone: the row under the Read tab's search, and the "Showing …" line's ✕.
  $('#p-wrBar')?.addEventListener('click', (e) => { const b = e.target.closest('[data-go]'); if (b) go(b.dataset.go); });
  $('#p-wrChip')?.addEventListener('click', (e) => {
    if (e.target.closest('[data-clear]')) { S.B.wr_clear_source(); $('#p-stream').scrollTop = 0; S.render(); }
    else go('sources');
  });
}
export const panelOpen = () => !panel()?.hidden;
export { isPhone };
