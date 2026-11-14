// Shared by the desktop window (desk.js) and the phone app (phone.js): the
// wasm `Burrow`, storage, sync, toasts and sheets, the paged editor bar,
// and the Read tab's post view (sentences you can hold) and swipe cards.
// Every rule lives in Burrow's Rust; this draws and forwards.

export const $ = (s, root = document) => root.querySelector(s);
export const $$ = (s, root = document) => [...root.querySelectorAll(s)];
export const now = () => Date.now();
export const esc = (s) => String(s ?? '').replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]));
export const reducedMotion = () => matchMedia('(prefers-reduced-motion: reduce)').matches;

export const S = {
  B: null,          // the wasm Burrow
  layout: null,     // 'desk' | 'phone'
  themes: [],
  theme: null,
  render: () => {}, // the active layout's render
  live: null,       // live.js, on a blyg (null: the demo)
};
export const isLive = () => !!S.live;
export const isPhone = () => S.layout === 'phone';

// ---------------------------------------------------------------- storage
// The store is one JSON value under one IndexedDB key: `store` for the
// demo's sample data, `live` for your blyg (its cache and outbox), so the
// two never mix. Live, the reading list is apart, under `live-reading`.
// Every call is guarded: a private window or blocked storage just means no
// persistence.
const DB = 'burrow-web', OS = 'kv';
const storeKey = () => (S.live ? 'live' : 'store');
function idb() {
  return new Promise((resolve, reject) => {
    const req = indexedDB.open(DB, 1);
    req.onupgradeneeded = () => req.result.createObjectStore(OS);
    req.onsuccess = () => resolve(req.result);
    req.onerror = () => reject(req.error);
  });
}
export async function load(key = 'store') {
  try {
    const db = await idb();
    return await new Promise((resolve) => {
      const r = db.transaction(OS).objectStore(OS).get(key);
      r.onsuccess = () => resolve(typeof r.result === 'string' ? r.result : undefined);
      r.onerror = () => resolve(undefined);
    });
  } catch { return undefined; }
}
let dbp;
async function put(key, value) {
  try {
    dbp ??= idb();
    const db = await dbp;
    await new Promise((resolve, reject) => {
      const tx = db.transaction(OS, 'readwrite');
      tx.objectStore(OS).put(value, key);
      tx.oncomplete = resolve; tx.onerror = () => reject(tx.error);
    });
    return true;
  } catch { return false; }
}
let saving = null;
/** Write the store to IndexedDB. Live, a change saved this way (a read
    mark, a hopper, a delete) also goes to the blyg after a pause, unless
    `quiet` (the outbox's own writes). Calls in one turn make one write.
    The snapshot holds your posts, the outbox and your marks; the reading
    list (thousands of posts) has its own, `persistReading`. */
export function persist({ quiet = false } = {}) {
  if (S.live && !quiet) scheduleSend();
  saving ??= new Promise((resolve) => setTimeout(() => {
    saving = null;
    put(storeKey(), S.B.to_json()).then(resolve);
  }, 0));
  return saving;
}
/** Live: write the reading list, when it changed since it was last
    written (after a refresh read it, and when the page is hidden). */
export function persistReading() {
  if (!S.live || !S.B?.reading_dirty()) return Promise.resolve(true);
  return put('live-reading', S.B.reading_json());
}
export const pref = {
  get(k) { try { return localStorage.getItem('burrow-web.' + k); } catch { return null; } },
  set(k, v) { try { localStorage.setItem('burrow-web.' + k, v); } catch {} },
};

// ---------------------------------------------------------------- sync
// Each keystroke is in the store at once ("saved in this browser"); ~800 ms
// after typing stops the store is written to IndexedDB, the demo's push.
// Live, it is written at once and the outbox goes to the blyg after the
// pause (live.js), each edit with the revision it was based on.
let syncT;
function scheduleSend() {
  clearTimeout(syncT);
  if (!S.B.is_offline()) syncT = setTimeout(() => S.live.flush(), 800);
}
export function touched() {
  S.render();
  clearTimeout(syncT);
  if (S.live) { persist(); return; }
  if (S.B.is_offline()) { persist(); return; }
  syncT = setTimeout(async () => {
    S.B.sync_phase('syncing'); S.render();
    const ok = await persist();
    setTimeout(() => { S.B.sync_phase(ok ? 'synced' : 'error'); S.render(); }, 350);
  }, 800);
}

// ---------------------------------------------------------------- drawing
/** Set `el`'s HTML, unless it's what is already there: a redraw that
    changes nothing (the 30 s tick, an outbox step) costs a string compare,
    not a rebuilt list, and leaves an open card or a scroll alone. */
export function draw(el, html) {
  if (el._html === html) return false;
  el.innerHTML = html;
  el._html = html;
  return true;
}
/** Long lists are drawn a page at a time: `more()` runs when `scroller`
    (an element, or `window` for the page) nears its end, to draw the next
    page. */
export function onNearEnd(scroller, more) {
  let queued = false;
  scroller.addEventListener('scroll', () => {
    if (queued) return;
    queued = true;
    requestAnimationFrame(() => {
      queued = false;
      const el = scroller === window ? document.scrollingElement : scroller;
      if (el.scrollTop + el.clientHeight > el.scrollHeight - 1200) more();
    });
  }, { passive: true });
}

// ---------------------------------------------------------------- toast
let toastT;
export function toast(html, ms = 2400) {
  const t = $(isPhone() ? '#p-toast' : '#toast');
  if (!t) return;
  t.innerHTML = html; t.classList.add('show');
  clearTimeout(toastT); toastT = setTimeout(() => t.classList.remove('show'), ms);
}
export function shake(el) {
  if (!el || reducedMotion()) return;
  el.classList.remove('shake'); void el.offsetWidth; el.classList.add('shake');
}
/** Show what an `Outcome` from Rust says; returns it parsed. */
export function outcome(json, shakeEl) {
  const o = typeof json === 'string' ? JSON.parse(json) : json;
  if (o?.action === 'toast') toast(esc(o.message));
  if (o?.action === 'shake') { shake(shakeEl); toast(esc(o.message)); }
  return o;
}
export function publishedToast(r) {
  const where = esc(r.permalink.replace(/^https?:\/\//, ''));
  toast(`Published v${r.version}${r.note ? ` · “${esc(r.note)}”` : ''}<br><small>${S.live
    ? `<a href="${esc(r.permalink)}" target="_blank" rel="noopener">${where}</a>`
    : `${where} (a pretend blyg)`}</small>`, S.live ? 5200 : 3200);
}
/** Finish a publish: live, `remote_publish` goes to the blyg. */
export async function finishPublish(o) {
  return o?.action === 'remote_publish' && S.live ? await S.live.publish(o) : o;
}
/** Finish a discard: live, `remote_restore` goes to the blyg. */
export async function finishDiscard(o) {
  return o?.action === 'remote_restore' && S.live ? await S.live.restore(o) : o;
}
/** The conflict banner under the editor: the blyg's copy beside yours. */
export function conflictBanner(el, c, onChoose) {
  el.hidden = !c?.conflict;
  if (!c?.conflict) { el.innerHTML = ''; el._rev = null; return; }
  if (el._rev === c.conflict.server_rev) return;
  el._rev = c.conflict.server_rev;
  el.innerHTML = `<b>Changed elsewhere.</b> The blyg has a newer copy (revision ${c.conflict.server_rev})${c.conflict.excerpt ? `: <q>${esc(c.conflict.excerpt)}</q>` : ''}.
    <span class="cbtns"><button type="button" data-keep="mine">Keep mine</button><button type="button" data-keep="theirs">Use theirs</button></span>`;
  el.onclick = (e) => {
    const b = e.target.closest('[data-keep]');
    if (!b) return;
    outcome(S.B.resolve_conflict(b.dataset.keep === 'mine'));
    el._rev = null;
    onChoose();
  };
}
export async function copyText(text) {
  try { await navigator.clipboard.writeText(text); return true; } catch { return false; }
}

// ---------------------------------------------------------------- preview
export function nonce() {
  const a = new Uint8Array(16);
  crypto.getRandomValues(a);
  return [...a].map((b) => b.toString(16).padStart(2, '0')).join('');
}
/** Load the open post's preview into `frame`, unless it's already there. */
export function showPreview(frame) {
  const page = S.B.preview_page(nonce(), S.theme?.dark ?? false);
  const key = page.replace(/nonce-[0-9a-f]+|nonce="[0-9a-f]+"/g, '');
  if (frame._shown !== key) { frame.srcdoc = page; frame._shown = key; }
}

// ---------------------------------------------------------------- text edits
/** Replace the selection of `ta` with `text`, as typing would (so undo
    works where the browser supports it), and fire `input`. */
export function insertText(ta, text, selectFrom, selectTo) {
  ta.focus();
  const start = ta.selectionStart;
  let ok = false;
  try { ok = document.execCommand('insertText', false, text); } catch { ok = false; }
  if (!ok) {
    ta.setRangeText(text, ta.selectionStart, ta.selectionEnd, 'end');
    ta.dispatchEvent(new Event('input', { bubbles: true }));
  }
  if (selectFrom != null) ta.setSelectionRange(start + selectFrom, start + (selectTo ?? selectFrom));
}
function lineBounds(ta) {
  const v = ta.value;
  const a = v.lastIndexOf('\n', ta.selectionStart - 1) + 1;
  let b = v.indexOf('\n', ta.selectionEnd);
  if (b < 0) b = v.length;
  return [a, b];
}
/** Wrap the selection in `l`…`r` (or insert `l r` with the caret between). */
function wrap(ta, l, r = l, placeholder = '') {
  const sel = ta.value.slice(ta.selectionStart, ta.selectionEnd) || placeholder;
  insertText(ta, l + sel + r, l.length, l.length + sel.length);
}
/** Prefix every selected line (toggling it off when all have it). */
function prefixLines(ta, prefix, numbered = false) {
  const [a, b] = lineBounds(ta);
  const lines = ta.value.slice(a, b).split('\n');
  const re = numbered ? /^\d+\.\s/ : new RegExp('^' + prefix.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'));
  const off = lines.every((l) => re.test(l));
  const out = lines.map((l, i) => (off ? l.replace(re, '') : (numbered ? `${i + 1}. ` : prefix) + l)).join('\n');
  ta.setSelectionRange(a, b);
  insertText(ta, out, out.length);
}
function block(ta, text, caretAt) {
  const v = ta.value, s = ta.selectionStart;
  const before = s > 0 && v[s - 1] !== '\n' ? '\n\n' : (s > 1 && v[s - 2] !== '\n' && s > 0 ? '\n' : '');
  const after = v[ta.selectionEnd] && v[ta.selectionEnd] !== '\n' ? '\n\n' : '';
  insertText(ta, before + text + after, before.length + (caretAt ?? text.length));
}

/** The Markdown helpers on the editor bar's second and third pages. */
export const MD = {
  bold: (ta) => wrap(ta, '**', '**', 'bold'),
  italic: (ta) => wrap(ta, '*', '*', 'italic'),
  strike: (ta) => wrap(ta, '~~', '~~', 'struck'),
  code: (ta) => wrap(ta, '`', '`', 'code'),
  heading: (ta) => prefixLines(ta, '## '),
  bullets: (ta) => prefixLines(ta, '- '),
  numbers: (ta) => prefixLines(ta, '', true),
  quote: (ta) => prefixLines(ta, '> '),
  fence: (ta) => {
    const sel = ta.value.slice(ta.selectionStart, ta.selectionEnd);
    block(ta, '```\n' + sel + '\n```', 4 + sel.length);
  },
  link: (ta) => {
    const sel = ta.value.slice(ta.selectionStart, ta.selectionEnd) || 'text';
    insertText(ta, `[${sel}](https://)`, sel.length + 3, sel.length + 11);
  },
  image: (ta) => insertText(ta, '![description](https://)', 2, 13),
  rule: (ta) => block(ta, '---'),
  tk: (ta) => {
    const sel = ta.value.slice(ta.selectionStart, ta.selectionEnd);
    insertText(ta, `[TK]${sel || 'what to write here'}[/TK]`, 4, 4 + (sel || 'what to write here').length);
  },
};

// ---------------------------------------------------------------- the editor bar
// Paged screens of tools above the keyboard, as Apple Notes does: swipe
// sideways (or drag with a mouse, or scroll with a trackpad) and they snap
// a page at a time; the dots show where you are and jump when tapped.
// Publish stays at the right on every page.
export const BAR_PAGES = [
  { name: 'Link and kind', tools: [
    ['link', '[[ link', 'Link a post'], ['quote', '❝ quote', 'Quote a post (threads)'],
    ['kind', '◦⇄≡', 'Fragment ⇄ thread'], ['tk', 'TK', 'Ask for generated text: [TK]…[/TK]'] ] },
  { name: 'Markdown', tools: [
    ['bold', '<b>B</b>', 'Bold'], ['italic', '<i>I</i>', 'Italic'], ['heading', 'H', 'Heading'],
    ['bullets', '•', 'Bulleted list'], ['numbers', '1.', 'Numbered list'], ['quoteBlock', '❝¶', 'Block quote'] ] },
  { name: 'More Markdown', tools: [
    ['code', '‹›', 'Inline code'], ['fence', '```', 'Code block'], ['mdlink', '🔗', 'Web link'],
    ['image', '🖼', 'Image'], ['rule', '―', 'Rule'], ['strike', '<s>S</s>', 'Strikethrough'] ] },
  { name: 'Post', tools: [['hide', '⌄ Done', 'Hide the keyboard']] },
];
// Shelve and Preview are named over the editor (the phone's top bar, the
// desk's action row), so the last page keeps the post's own sheet.
// --- web-writing --- The image tool uploads one (www/writing.js); the
// last page gains TK generation on the blyg and the post's ⋯ menu.
BAR_PAGES[2].tools[3] = ['upload', '🖼', 'Image: upload one, with alt text'];
BAR_PAGES[3].tools.unshift(['more', '⋯ Post', 'This post: image, versions, TK, withdraw'], ['gen', '✦ Write TK', 'Generate the TK text on your blyg']);
// --- end web-writing ---

/** Build the bar into `host`. `act(name)` runs a tool; returns the bar. */
export function editorBar(host, act) {
  host.classList.add('edbar');
  host.innerHTML = `
    <button type="button" class="pg-arrow prev" aria-label="Previous tools" tabindex="-1">‹</button>
    <div class="pager" role="toolbar" aria-label="Editing tools">${BAR_PAGES.map((p, i) => `
      <div class="page" role="group" aria-label="${esc(p.name)}" data-page="${i}">${p.tools.map(([id, label, title]) =>
        `<button type="button" data-tool="${id}" title="${esc(title)}" aria-label="${esc(title)}">${label}</button>`).join('')}</div>`).join('')}
    </div>
    <button type="button" class="pg-arrow next" aria-label="More tools" tabindex="-1">›</button>
    <div class="pgdots">${BAR_PAGES.map((p, i) => `<button type="button" class="pgdot${i ? '' : ' on'}" data-go="${i}" aria-label="${esc(p.name)}" tabindex="-1"></button>`).join('')}</div>
    <button type="button" class="pri" data-tool="publish">Publish</button>`;
  const pager = $('.pager', host);
  const page = () => Math.round(pager.scrollLeft / Math.max(1, pager.clientWidth));
  const go = (i) => {
    const n = Math.max(0, Math.min(BAR_PAGES.length - 1, i));
    pager.scrollTo({ left: n * pager.clientWidth, behavior: reducedMotion() ? 'auto' : 'smooth' });
  };
  const sync = () => {
    const p = page();
    $$('.pgdot', host).forEach((d, i) => d.classList.toggle('on', i === p));
    host.dataset.page = p;
    host.classList.toggle('at-start', p === 0);
    host.classList.toggle('at-end', p === BAR_PAGES.length - 1);
  };
  host.dataset.page = '0';
  host.classList.add('at-start');
  pager.addEventListener('scroll', () => requestAnimationFrame(sync), { passive: true });
  // Keep the caret (and the phone keyboard) in the textarea when a tool is
  // pressed: the press must not move focus.
  host.addEventListener('mousedown', (e) => { if (e.target.closest('button')) e.preventDefault(); });
  host.addEventListener('click', (e) => {
    if (host._dragged) return;
    const d = e.target.closest('[data-go]');
    if (d) { go(+d.dataset.go); return; }
    if (e.target.closest('.prev')) { go(page() - 1); return; }
    if (e.target.closest('.next')) { go(page() + 1); return; }
    const t = e.target.closest('[data-tool]');
    if (t) act(t.dataset.tool);
  });
  // A mouse drags the pages like a finger; a wheel turns them.
  let drag = null;
  pager.addEventListener('pointerdown', (e) => {
    if (e.pointerType !== 'mouse' || e.button !== 0) return;
    drag = { x: e.clientX, left: pager.scrollLeft, moved: false };
    host._dragged = false;
  });
  window.addEventListener('pointermove', (e) => {
    if (!drag) return;
    const dx = e.clientX - drag.x;
    if (Math.abs(dx) > 5) { drag.moved = true; pager.classList.add('dragging'); }
    if (drag.moved) pager.scrollLeft = drag.left - dx;
  });
  window.addEventListener('pointerup', () => {
    if (!drag) return;
    const moved = drag.moved, from = drag.left;
    drag = null;
    pager.classList.remove('dragging');
    if (moved) {
      host._dragged = true;
      setTimeout(() => { host._dragged = false; }, 0);
      const delta = pager.scrollLeft - from, w = pager.clientWidth;
      go(Math.round(from / w) + (Math.abs(delta) > w * 0.2 ? Math.sign(delta) : 0));
    }
  });
  let wheelT, wheelAcc = 0;
  pager.addEventListener('wheel', (e) => {
    if (Math.abs(e.deltaX) > Math.abs(e.deltaY)) return; // a trackpad's sideways swipe scrolls natively
    e.preventDefault();
    wheelAcc += e.deltaY;
    clearTimeout(wheelT);
    wheelT = setTimeout(() => { if (Math.abs(wheelAcc) > 20) go(page() + Math.sign(wheelAcc)); wheelAcc = 0; }, 60);
  }, { passive: false });
  host.goToPage = go;
  host.page = page;
  return host;
}

/** Run an editor-bar tool on `ta`; `extra` handles the app-level ones. */
export function runTool(name, ta, extra) {
  const md = { bold: MD.bold, italic: MD.italic, heading: MD.heading, bullets: MD.bullets,
    numbers: MD.numbers, quoteBlock: MD.quote, code: MD.code, fence: MD.fence, mdlink: MD.link,
    image: MD.image, rule: MD.rule, strike: MD.strike, tk: MD.tk }[name];
  if (md) { md(ta); return; }
  if (name === 'hide') { ta.blur(); return; }
  extra(name);
}

// ---------------------------------------------------------------- the link/quote picker
/** Fill `list` with what [[ link or ❝ quote can point at. */
export function renderPicks(list, query) {
  const rows = JSON.parse(S.B.pick_targets(query));
  list.innerHTML = rows.length
    ? rows.map((r) => `<button type="button" class="opt2" data-pick="${esc(r.id)}" data-label="${esc(r.label)}">${esc(r.title)}<small>${esc(r.sub)}</small></button>`).join('')
    : `<p class="muted">Nothing matches “${esc(query)}”.</p>`;
}
/** Insert a pick into `ta`: `[[id | label]]`, or a quote on its own line. */
export function insertPick(ta, id, label, quote) {
  const o = outcome(S.B.pick_markup(id, label, quote));
  if (o.action !== 'insert') return false;
  if (o.block) {
    const v = ta.value, s = ta.selectionStart;
    const pre = s > 0 && v[s - 1] !== '\n' ? '\n' : '';
    insertText(ta, `${pre}${o.text}\n`);
  } else insertText(ta, o.text);
  return true;
}

// ---------------------------------------------------------------- the post view
/** Split each paragraph's text into sentences you can hold. Only text
    nodes are touched; links, emphasis and quotes keep their markup. */
export function sentencify(root) {
  const blocks = $$('p, li', root).filter((b) => !b.closest('blockquote.blyg-transclusion, pre'));
  let n = 0;
  for (const b of blocks) {
    const walker = document.createTreeWalker(b, NodeFilter.SHOW_TEXT);
    const texts = [];
    while (walker.nextNode()) texts.push(walker.currentNode);
    for (const t of texts) {
      if (t.parentElement.closest('a, code')) continue;
      const parts = t.textContent.split(/(?<=[.?!…])\s+/);
      if (!t.textContent.trim()) continue;
      const frag = document.createDocumentFragment();
      parts.forEach((p, i) => {
        if (!p) return;
        const s = document.createElement('span');
        s.className = 's'; s.dataset.s = String(n++);
        s.textContent = p;
        frag.append(s);
        if (i < parts.length - 1) frag.append(' ');
      });
      t.replaceWith(frag);
    }
  }
}

/** Draw a post into `host` (`.post`): title, byline, lineage, its HTML. */
export function renderPost(host, p) {
  const lin = (l) => (l.key ? `<button type="button" class="lin" data-goto="${esc(l.key)}">${esc(l.text)}</button>` : `<span class="lin">${esc(l.text)}</span>`);
  const thread = /thread/.test(p.kind);
  host.innerHTML = `${thread ? `<h3>${esc(p.title)}</h3>` : ''}<div class="by">${esc(p.by)}</div>
    ${p.lineage.length || p.drawn_on_by.length ? `<div class="lineage" aria-label="Lineage">${p.lineage.map(lin).join('')}${p.drawn_on_by.map(lin).join('')}</div>` : ''}
    ${/* --- syndication --- escaped, https-checked HTML from src/syndication.rs */ S.B.reading_copies_html(p.key)}
    <div class="item-content">${p.html}</div>`;
  const content = $('.item-content', host);
  // A thread's first line is its title: shown above, so not twice. A
  // fragment has no title line; its text is the post.
  const first = content.firstElementChild;
  if (thread && first && first.nextElementSibling && first.textContent.trim() === p.title.trim()) first.remove();
  $$('a', content).forEach((a) => { a.target = '_blank'; a.rel = 'noopener noreferrer'; });
  $$('img', content).forEach((img) => { img.loading = 'lazy'; img.addEventListener('error', () => img.replaceWith(Object.assign(document.createElement('span'), { className: 'img-missing', textContent: `[image: ${img.alt || 'not available offline'}]` }))); });
  sentencify(content);
}

/** Hold a sentence (≈450 ms, without moving) to act on just that passage.
    `menu` is shown above it; `onPick(action, text)` runs the choice. */
export function holdSentences(host, menu, onPick) {
  let t = null, start = null;
  const clear = () => { clearTimeout(t); t = null; };
  host.addEventListener('pointerdown', (e) => {
    const s = e.target.closest('.s');
    if (!s || e.button > 0) return;
    start = { x: e.clientX, y: e.clientY };
    clear();
    t = setTimeout(() => select(s), 450);
  });
  host.addEventListener('pointermove', (e) => {
    if (t && start && Math.hypot(e.clientX - start.x, e.clientY - start.y) > 8) clear();
  });
  ['pointerup', 'pointercancel', 'pointerleave'].forEach((ev) => host.addEventListener(ev, clear));
  // The system's own long-press menu would cover ours.
  host.addEventListener('contextmenu', (e) => { if (e.target.closest('.s')) e.preventDefault(); });
  function select(s) {
    $$('.s.hl', host).forEach((x) => x.classList.remove('hl'));
    s.classList.add('hl');
    menu.dataset.text = s.textContent.trim();
    const box = menu.offsetParent?.getBoundingClientRect() ?? { top: 0, left: 0 };
    const r = s.getBoundingClientRect();
    menu.classList.add('on');
    const top = r.top - box.top - menu.offsetHeight - 8;
    menu.style.top = `${top < 4 ? r.bottom - box.top + 8 : top}px`;
    navigator.vibrate?.(10);
    $('button', menu)?.focus({ preventScroll: true });
  }
  menu.addEventListener('click', (e) => {
    const b = e.target.closest('[data-pm]');
    if (!b) return;
    const text = menu.dataset.text;
    hideMenu();
    onPick(b.dataset.pm, text);
  });
  function hideMenu() { menu.classList.remove('on'); $$('.s.hl', host).forEach((x) => x.classList.remove('hl')); }
  document.addEventListener('pointerdown', (e) => { if (!menu.contains(e.target) && !e.target.closest('.s')) hideMenu(); });
  return { hide: hideMenu, select };
}

// ---------------------------------------------------------------- swipe cards
/** Swipe a card right past the threshold to `onRight`, left to reveal its
    actions (`.acts`), which stay open until you tap or swipe elsewhere.
    Vertical scrolling is left to the browser (`touch-action: pan-y`). */
export function swipeCards(list, { onRight, onTap }) {
  let d = null;
  const OPEN = 168;
  const closeAll = (except) => $$('.card.open', list).forEach((c) => { if (c !== except) { c.classList.remove('open'); c.style.transform = ''; } });
  list.addEventListener('pointerdown', (e) => {
    const c = e.target.closest('.card');
    if (!c || e.target.closest('.acts button') || e.button > 0) return;
    d = { c, x: e.clientX, y: e.clientY, dx: 0, base: c.classList.contains('open') ? -OPEN : 0, axis: null, id: e.pointerId };
  });
  list.addEventListener('pointermove', (e) => {
    if (!d || e.pointerId !== d.id) return;
    const dx = e.clientX - d.x, dy = e.clientY - d.y;
    if (!d.axis && Math.hypot(dx, dy) > 8) d.axis = Math.abs(dx) > Math.abs(dy) ? 'x' : 'y';
    if (d.axis !== 'x') return;
    d.dx = dx;
    d.c.classList.add('dragging');
    try { d.c.setPointerCapture(d.id); } catch {}
    const x = Math.max(-OPEN - 40, d.base + dx);
    d.c.style.transform = `translateX(${x}px)`;
    d.c.parentElement.classList.toggle('to-read', x > 0);
  });
  const end = () => {
    if (!d) return;
    const { c, dx, base, axis } = d;
    d = null;
    c.classList.remove('dragging');
    if (axis !== 'x') return;
    c._swiped = true; setTimeout(() => { c._swiped = false; }, 50);
    const x = base + dx;
    if (x > 90) {
      c.style.transform = 'translateX(100%)';
      setTimeout(() => onRight(c.dataset.key, c), reducedMotion() ? 0 : 200);
    } else if (x < -70) {
      closeAll(c);
      c.classList.add('open');
      c.style.transform = `translateX(${-OPEN}px)`;
      $('.acts button', c.parentElement)?.focus({ preventScroll: true });
    } else { c.classList.remove('open'); c.style.transform = ''; }
  };
  list.addEventListener('pointerup', end);
  list.addEventListener('pointercancel', end);
  list.addEventListener('click', (e) => {
    const c = e.target.closest('.card');
    if (!c || c._swiped) return;
    if (c.classList.contains('open')) { closeAll(); return; }
    closeAll();
    onTap(c.dataset.key);
  });
  return { closeAll };
}

/** After a swipe's action: a card the redraw kept (it was already read, so
    nothing changed) slides back into place. */
export function settle(c) {
  if (!c?.isConnected) return;
  c.style.transform = '';
  c.parentElement?.classList.remove('to-read');
}

/** A stream row: the card, with "✓ Read" under its left edge and the
    Quote / Reply / Hopper actions under its right. */
export function cardHtml(r) {
  return `<div class="swipewrap">
    <div class="bgL" aria-hidden="true">✓ Read</div>
    <div class="acts" aria-label="Actions"><button type="button" data-act="quote" data-key="${esc(r.key)}">❝<small>Quote</small></button><button type="button" data-act="reply" data-key="${esc(r.key)}">↩<small>Reply</small></button><button type="button" data-act="hopper" data-key="${esc(r.key)}">⊕<small>Hopper</small></button></div>
    <div class="card${r.unread ? ' unread' : ''}" data-key="${esc(r.key)}" tabindex="0" role="button" aria-label="${esc(r.who)}: ${esc(r.title)}">
      <div class="who">${r.unread ? '<span class="dot" aria-label="unread"></span>' : ''}<b>${esc(r.who)}</b> <span>${esc(r.host)} · ${esc(r.when)}</span>${r.glyph ? `<span class="glyph">${esc(r.glyph)}</span>` : ''}${r.badge && r.badge !== 'new' ? `<span class="badge">${esc(r.badge)}</span>` : ''}</div>
      <div class="ex"><i>${esc(r.title)}</i>${r.excerpt ? ' ' + esc(r.excerpt) : ''}</div>
      ${r.hoppers.length ? `<div class="hop">⊕ ${r.hoppers.map(esc).join(', ')}</div>` : ''}
    </div></div>`;
}

// ---------------------------------------------------------------- themes
const FALLBACK_SERIF = 'ETBook, "Iowan Old Style", Charter, Georgia, serif';
export function applyTheme(id, roots) {
  const t = S.themes.find((x) => x.id === id) || S.themes[0];
  if (!t) return;
  S.theme = t;
  const font = (f, fallback) => (f ? `"${f === 'ET Book' ? 'ETBook' : f}", ${fallback}` : fallback);
  for (const root of roots) {
    for (const [k, v] of t.vars) root.style.setProperty(k, v);
    root.style.setProperty('--fbody', font(t.font_writing, FALLBACK_SERIF));
    root.style.setProperty('--fui', font(t.font_ui, t.font_ui ? 'Inter, system-ui, sans-serif' : FALLBACK_SERIF));
    root.style.setProperty('--radius', `${t.radius}px`);
  }
  const bg = t.vars.find(([k]) => k === '--c-bar')?.[1];
  if (bg) $('meta[name="theme-color"]:not([media])')?.setAttribute('content', bg);
  pref.set('theme', t.id);
}
