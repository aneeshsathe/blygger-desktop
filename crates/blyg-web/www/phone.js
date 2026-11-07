// Burrow on a phone (700px wide or less), after the approved prototype:
// bottom tabs Write · Read · Shelf and a ＋ for quick capture; the editor
// with its paged bar riding on the keyboard; the reading stream you swipe
// (right: read; left: quote, reply, hopper) and posts whose sentences you
// can hold. Every rule is Burrow's Rust; this draws and forwards.

import {
  $, $$, S, now, esc, persist, pref, touched, toast, shake, outcome, publishedToast, copyText,
  finishPublish, conflictBanner,
  showPreview, editorBar, runTool, renderPicks, insertPick, renderPost, holdSentences,
  swipeCards, cardHtml, applyTheme, draw, onNearEnd, settle,
} from './core.js';
import { setOffline, resetData } from './desk.js';
// --- web-writing ---
import * as Wr from './writing.js';
// --- end web-writing ---

const app = $('#phone'), ta = $('#p-ta'), q = $('#p-q'), pv = $('#p-pv');
let V;
let pview = 'write';     // 'write' | 'edit' | 'read' | 'post'
let shownId = null;      // the item the editor holds
let previewing = false;
let unreadOnly = false;
let readKey = null;      // the open reading post
let pickMode = 'link';   // the link sheet: 'link' | 'quote'
let savedSel = null;     // the editor's selection while a sheet is up
let sheetBack = null;    // what had focus before a sheet

// ---------------------------------------------------------------- views
const ROOT_OF = { edit: 'write', post: 'read' };
function show(v, { push = true } = {}) {
  const was = pview;
  pview = v;
  app.dataset.view = v;
  for (const n of ['write', 'edit', 'read', 'post']) {
    const el = $('#v-' + n);
    el.classList.remove('off-r', 'off-l');
    if (n === v) { el.removeAttribute('inert'); continue; }
    el.setAttribute('inert', '');
    // The root under an open detail slides left; the rest wait to the right.
    el.classList.add(ROOT_OF[v] === n ? 'off-l' : 'off-r');
  }
  const tab = ROOT_OF[v] ?? v;
  $$('.tab').forEach((t) => t.classList.toggle('on', t.dataset.tab === tab));
  // A detail view is a history entry, so the system back gesture closes it.
  if (push && ROOT_OF[v] && !ROOT_OF[was]) { try { history.pushState({ burrow: v }, ''); } catch {} }
  if (!ROOT_OF[v] && ROOT_OF[was] && history.state?.burrow) { try { history.back(); } catch {} }
  render();
}
window.addEventListener('popstate', () => {
  if (S.layout !== 'phone') return;
  if (pview === 'edit') leaveEditor();
  else if (pview === 'post') show('read', { push: false });
});

// ---------------------------------------------------------------- render
// Only the screen you're on is drawn, and long lists a page at a time
// (more as you scroll): a blyg can hold hundreds of posts and thousands to
// read, and a phone redrawing all of them on every change ran hot.
const PAGE = 40;
let listN = PAGE * 2;    // Write rows drawn
let streamN = PAGE;      // Read cards drawn

export function render({ loadEditor = false } = {}) {
  if (!S.B) return;
  V = JSON.parse(S.B.view(now()));
  if (S.layout === 'phone') document.title = pview === 'edit' ? V.title : 'Burrow';

  if (pview === 'write') renderList();

  // The editor.
  const c = V.current;
  if (pview === 'edit' && c) {
    const id = c.id ?? '(new)';
    if (loadEditor || id !== shownId || ta.value !== c.content && (document.activeElement !== ta || S.live)) {
      if (ta.value !== c.content) {
        const [a, b] = [ta.selectionStart, ta.selectionEnd];
        ta.value = c.content;
        if (document.activeElement === ta && id === shownId) ta.setSelectionRange(a, b);
      }
      ta.placeholder = c.status === 'new' ? 'Write something…' : '';
      shownId = id;
    }
    $('#p-kind').textContent = c.kind_label;
    const cnt = $('#p-cnt');
    cnt.textContent = c.counter;
    cnt.className = c.level || '';
    const ban = $('#p-ban');
    ban.textContent = c.banner || c.new_post || '';
    ban.className = 'flex' + (c.banner ? ' over' : '');
    $('#p-shelfBtn').className = 'ic ' + (c.shelved ? 'on' : 'off');
    $('#p-shelfBtn').setAttribute('aria-pressed', String(c.shelved));
    const so = $('#p-stubof');
    so.hidden = !c.stub_of; so.textContent = c.stub_of ?? '';
    conflictBanner($('#p-conflict'), c, () => { render({ loadEditor: true }); touched(); });
    if (previewing) schedulePreview();
  }
  $('#p-mode').textContent = V.mode === 'live' ? 'Live' : 'Demo';
  $('#p-mode').title = V.mode_label;
  $('#p-mode').className = 'modebadge ' + V.mode;
  const sy = $('#p-sync');
  sy.className = 'psync ' + V.sync_dot;
  sy.querySelector('span').textContent = V.sync;

  renderStream();
}

/** Write: the shelf strip, then the posts (shelved first, a line under). */
function renderList() {
  draw($('#p-shelf'), V.shelf.length
    ? V.shelf.map((r) => `<button type="button" class="chip" data-shelf="${esc(r.id)}">📚 ${esc(r.title)}</button>`).join('')
    : '<span class="empty">Hold a post to shelve it</span>');
  const query = V.query.trim();
  const rows = V.rows.slice(0, listN);
  draw($('#p-list'), rows.map((r, i) => `<button type="button" role="listitem" class="prow${i === V.shelf_len - 1 && V.rows.length > V.shelf_len ? ' shelved-last' : ''}" data-id="${esc(r.id)}" data-shelved="${r.shelved}">
      <div class="ti">${r.shelved ? '📚 ' : ''}${r.title_html}</div>
      <div class="meta">${esc(r.kind)} <span class="pill${r.pill_pub ? ' pub' : ''}">${esc(r.pill)}</span>${r.dot ? `<span class="dot${r.dot === 'amber' ? ' amber' : ''}" title="${esc(r.dot_tip || 'unpublished edits')}"></span>` : ''}<span>${esc(r.when)}</span></div></button>`).join('')
    + (V.rows.length > rows.length ? `<p class="list-more">${V.rows.length - rows.length} more…</p>` : '')
    + (V.wants_create && query
      ? `<button type="button" class="prow start" data-new="1"><div class="ti">＋ Start “${esc(query)}”</div><div class="meta">${V.rows.length ? 'a new post' : 'no matches · tap to write it'}</div></button>`
      : ''));
}

/** Read: the unread counts always; the cards only on the Read screen (not
    under an open post, so marking it read redraws nothing you can't see). */
function renderStream() {
  const reading = pview === 'read';
  const R = JSON.parse(S.B.reading_page(now(), $('#p-rq').value, unreadOnly, reading ? streamN : 0));
  const u = $('#p-unreadOnly');
  u.textContent = `● ${R.unread}`;
  u.setAttribute('aria-pressed', String(unreadOnly));
  u.setAttribute('aria-label', unreadOnly ? `Showing unread only (${R.unread}); show all` : `${R.unread} unread; show only those`);
  $('#p-tabn').textContent = R.unread ? String(R.unread) : '';
  if (!reading) return;
  draw($('#p-stream'), R.rows.length
    ? R.rows.map(cardHtml).join('') + (R.total > R.rows.length ? `<p class="stream-more">${R.total - R.rows.length} more…</p>` : '')
    : `<p class="stream-empty">${esc(R.empty || (unreadOnly ? 'All read.' : 'Nothing matches.'))}</p>`);
}
/** Back to the top of the stream: one page again. */
function streamFromTop() {
  streamN = PAGE;
  $('#p-stream').scrollTop = 0;
}

let pvT;
function schedulePreview() { clearTimeout(pvT); pvT = setTimeout(() => showPreview(pv), 150); }

// ---------------------------------------------------------------- sheets
function sheet(id) {
  $$('.psheet').forEach((s) => s.classList.remove('on'));
  $('#p-scrim').classList.toggle('on', !!id);
  if (id) {
    sheetBack = document.activeElement;
    if (sheetBack === ta) savedSel = [ta.selectionStart, ta.selectionEnd];
    $('#' + id).classList.add('on');
    $$('.pview, .tabs, .fab').forEach((el) => el.setAttribute('aria-hidden', 'true'));
  } else {
    $$('.pview, .tabs, .fab').forEach((el) => el.removeAttribute('aria-hidden'));
  }
}
function closeSheet({ refocus = true } = {}) {
  sheet(null);
  if (refocus && pview === 'edit' && savedSel) {
    ta.focus({ preventScroll: true });
    ta.setSelectionRange(savedSel[0], savedSel[1]);
  } else if (refocus && sheetBack && sheetBack !== document.body && document.contains(sheetBack)) {
    sheetBack.focus?.({ preventScroll: true });
  }
  savedSel = null;
}

// ---------------------------------------------------------------- write
function openEditor({ caretEnd = true } = {}) {
  previewing = false; app.classList.remove('previewing'); pv.hidden = true;
  show('edit');
  render({ loadEditor: true });
  if (caretEnd) {
    ta.focus({ preventScroll: true });
    ta.setSelectionRange(ta.value.length, ta.value.length);
  }
}
function leaveEditor() {
  ta.blur();
  if (q.value) { q.value = ''; S.B.set_query(''); }
  show('write', { push: false });
}
function startFromQuery() {
  const title = V.query.trim();
  S.B.create_draft(title, now());
  q.value = '';
  openEditor(); touched();
}
function askPublish() {
  const o = outcome(S.B.publish_check(), $('.edstat'));
  if (o.action !== 'sheet') return;
  $('#p-pubH').textContent = `Publish “${o.title}” as v${o.next_version}`;
  $('#p-pubNote').value = '';
  sheet('sh-pub');
  Wr.publishSheet($('#sh-pub'), $('#p-pubNote')); // --- web-writing ---
}
function doPublish() {
  const note = $('#p-pubNote').value;
  closeSheet({ refocus: false });
  S.B.sync_phase('syncing'); render();
  $('#p-sync span').textContent = 'publishing…';
  setTimeout(async () => {
    const r = await finishPublish(Wr.withNote(JSON.parse(S.B.publish(note, now())), note)); // --- web-writing ---
    if (r.action === 'published') {
      await persist();
      S.B.sync_phase('synced');
      render({ loadEditor: true });
      publishedToast(r);
    } else outcome(r);
  }, S.live ? 0 : 450);
}
function openPicker(quote) {
  pickMode = quote ? 'quote' : 'link';
  $('#p-linkH').textContent = quote ? 'Quote a post' : 'Link a post';
  $('#p-linkQ').value = '';
  renderPicks($('#p-linkList'), '');
  sheet('sh-link');
}
function togglePreview() {
  previewing = !previewing;
  app.classList.toggle('previewing', previewing);
  pv.hidden = !previewing;
  pv._shown = null;
  if (previewing) { ta.blur(); showPreview(pv); }
  render();
}
function barTool(name) {
  if (Wr.W.barTool(name)) return; // --- web-writing ---
  if (name === 'link') openPicker(false);
  else if (name === 'quote') openPicker(true);
  else if (name === 'kind') { const o = outcome(S.B.toggle_kind(now()), $('.edstat')); if (o.action === 'toast') touched(); }
  else if (name === 'shelf') toggleShelf();
  else if (name === 'preview') togglePreview();
  else if (name === 'publish') askPublish();
}
function toggleShelf() {
  const o = outcome(S.B.toggle_shelf());
  if (/shelf/.test(o.message || '')) touched();
}

// ---------------------------------------------------------------- shelf
function renderShelfSheet() {
  const rows = V.shelf;
  $('#p-shelfSheet').innerHTML = rows.length
    ? rows.map((r, i) => `<div class="shrow"><span class="n">${i + 1}</span>
        <button type="button" class="opt2" data-shelf="${esc(r.id)}">${esc(r.title)}<small>${esc(r.kind)} · ${esc(r.pill)} · ${esc(r.when)}</small></button>
        <button type="button" class="ic" data-up="${esc(r.id)}" data-i="${i}" aria-label="Move up" ${i ? '' : 'disabled'}>▲</button>
        <button type="button" class="ic" data-down="${esc(r.id)}" data-i="${i}" aria-label="Move down" ${i < rows.length - 1 ? '' : 'disabled'}>▼</button>
        <button type="button" class="ic" data-off="${esc(r.id)}" aria-label="Take off the shelf">✕</button></div>`).join('')
    : '<p class="muted">Nothing on your shelf yet. Hold a post in Write, or tap 📚 in the editor.</p>';
}
function openShelved(id) {
  closeSheet({ refocus: false });
  S.B.open_from_shelf(id);
  q.value = '';
  openEditor();
}

// ---------------------------------------------------------------- read
function openReading(key) {
  const p = JSON.parse(S.B.open_reading(key, now()));
  if (!p) return;
  readKey = key;
  renderPost($('#p-postBody'), p);
  $('#v-post .pscroll').scrollTop = 0;
  persist();
  show('post');
}
function reply(key, passage = '') {
  const o = JSON.parse(S.B.reply_to(key, passage, now()));
  if (o.action !== 'create') return;
  q.value = '';
  openEditor();
  touched();
  toast(passage ? 'A stub replying to that passage' : 'A stub replying to this post');
}
function quote(key, passage = '') {
  const o = JSON.parse(S.B.quote_new(key, passage, now()));
  if (o.action !== 'create') return;
  q.value = '';
  openEditor({ caretEnd: false });
  ta.focus({ preventScroll: true }); ta.setSelectionRange(0, 0);
  touched();
  toast(passage ? 'A new thread quoting that passage' : 'A new thread quoting it');
}
function hopperSheet(key) {
  readKey = key;
  const hs = JSON.parse(S.B.hoppers(key));
  $('#p-hopList').innerHTML = hs.length
    ? hs.map((h) => `<button type="button" class="opt2${h.on ? ' on' : ''}" data-hop="${esc(h.id)}" aria-pressed="${h.on}">${h.on ? '✓ ' : ''}${esc(h.name)}</button>`).join('')
    : '<p class="muted">No hoppers yet.</p>';
  sheet('sh-hop');
}
function textSize(d) {
  const cur = parseFloat(pref.get('rsize')) || 1.2;
  const next = Math.max(0.9, Math.min(2, Math.round((cur + d) * 100) / 100));
  app.style.setProperty('--rsize', `${next}rem`);
  pref.set('rsize', String(next));
}

// ---------------------------------------------------------------- the keyboard
// iOS Safari and Android Chrome shrink the visual viewport, not the
// layout one, when the keyboard opens. Pin the app to the visual viewport
// so the editor bar rides on the keyboard, and drop the tabs while typing.
function trackViewport() {
  const vv = window.visualViewport;
  const set = () => {
    const h = vv ? vv.height : window.innerHeight;
    const top = vv ? vv.offsetTop : 0;
    app.style.setProperty('--vv-h', `${h}px`);
    app.style.setProperty('--vv-top', `${top}px`);
    const typing = document.activeElement?.matches?.('#phone textarea, #phone input');
    app.classList.toggle('kb', !!typing && window.innerHeight - h > 120);
  };
  vv?.addEventListener('resize', set);
  vv?.addEventListener('scroll', set);
  window.addEventListener('resize', set);
  document.addEventListener('focusin', () => setTimeout(set, 50));
  document.addEventListener('focusout', () => setTimeout(set, 50));
  set();
}

// ---------------------------------------------------------------- events
export function initPhone() {
  trackViewport();
  // The app never scrolls as a whole: focusing a field in a sheet or view
  // that is still sliding in would otherwise scroll it out of place.
  for (const el of [app, ...$$('.pview', app)]) el.addEventListener('scroll', () => { el.scrollTop = 0; el.scrollLeft = 0; });
  const rs = parseFloat(pref.get('rsize'));
  if (rs) app.style.setProperty('--rsize', `${rs}rem`);

  // tabs, ＋
  $('#p-tabs').addEventListener('click', (e) => {
    const t = e.target.closest('.tab'); if (!t) return;
    if (t.dataset.tab === 'shelf') { renderShelfSheet(); sheet('sh-shelf'); return; }
    if (t.dataset.tab === 'read') streamFromTop();
    show(t.dataset.tab);
  });
  $('#p-fab').addEventListener('click', () => {
    $('#p-capTa').value = ''; capCount();
    sheet('sh-cap');
    $('#p-capTa').focus({ preventScroll: true });
  });
  $('#p-scrim').addEventListener('click', () => closeSheet());
  document.addEventListener('keydown', (e) => {
    if (S.layout !== 'phone') return;
    if (e.key === 'Escape' && $('.psheet.on')) { e.preventDefault(); closeSheet(); }
  });

  // write
  q.addEventListener('input', () => { S.B.set_query(q.value); listN = PAGE * 2; render(); });
  onNearEnd($('#p-writeScroll'), () => { if (pview === 'write' && V.rows.length > listN) { listN += PAGE * 2; renderList(); } });
  q.addEventListener('keydown', (e) => {
    if (e.key !== 'Enter') return;
    e.preventDefault();
    const o = JSON.parse(S.B.enter(now()));
    q.blur();
    if (o.action === 'create') { q.value = ''; openEditor(); touched(); toast('New draft created'); }
    else if (o.action === 'open') openEditor();
  });
  let hold = null;
  const list = $('#p-list');
  list.addEventListener('pointerdown', (e) => {
    const r = e.target.closest('.prow[data-id]'); if (!r) return;
    const at = { x: e.clientX, y: e.clientY };
    clearTimeout(hold?.t);
    hold = { r, at, fired: false, t: setTimeout(() => {
      hold.fired = true;
      const on = r.dataset.shelved !== 'true';
      const o = outcome(S.B.set_on_shelf(r.dataset.id, on));
      if (o.action === 'nothing') { toast(on ? 'On your shelf 📚' : 'Taken off your shelf'); navigator.vibrate?.(10); touched(); }
    }, 550) };
  });
  list.addEventListener('pointermove', (e) => { if (hold && Math.hypot(e.clientX - hold.at.x, e.clientY - hold.at.y) > 8) clearTimeout(hold.t); });
  ['pointerup', 'pointercancel'].forEach((ev) => list.addEventListener(ev, () => clearTimeout(hold?.t)));
  list.addEventListener('contextmenu', (e) => { if (e.target.closest('.prow')) e.preventDefault(); });
  list.addEventListener('click', (e) => {
    if (hold?.fired) { hold = null; return; }
    if (e.target.closest('[data-new]')) { startFromQuery(); return; }
    const r = e.target.closest('.prow[data-id]'); if (!r) return;
    S.B.open(r.dataset.id); q.value = ''; openEditor();
  });
  $('#p-shelf').addEventListener('click', (e) => { const c = e.target.closest('[data-shelf]'); if (c) openShelved(c.dataset.shelf); });

  // editor
  $('#p-back').addEventListener('click', () => leaveEditor());
  ta.addEventListener('input', () => { if (S.B.edit(ta.value, now())) touched(); else render(); });
  $('#p-kind').addEventListener('click', () => barTool('kind'));
  $('#p-shelfBtn').addEventListener('click', toggleShelf);
  $('#p-prevBtn').addEventListener('click', togglePreview);
  editorBar($('#p-bar'), (tool) => runTool(tool, ta, barTool));
  // --- web-writing ---
  Wr.attachEditor(ta);
  Wr.setWritingHooks('phone', { openEditor: () => openEditor(), afterDelete: () => leaveEditor() });
  $('#p-wmenu').addEventListener('click', () => Wr.openPostMenu());
  $('#p-wfork').addEventListener('click', () => Wr.askFork(readKey));
  $('#p-wset').addEventListener('click', () => { closeSheet({ refocus: false }); Wr.openSettings(); });
  // --- end web-writing ---

  // the link/quote sheet
  $('#p-linkQ').addEventListener('input', () => renderPicks($('#p-linkList'), $('#p-linkQ').value));
  $('#p-linkList').addEventListener('click', (e) => {
    const b = e.target.closest('[data-pick]'); if (!b) return;
    closeSheet();
    if (insertPick(ta, b.dataset.pick, b.dataset.label, pickMode === 'quote')) {
      toast(pickMode === 'quote' ? 'Quoted · it shows in the preview' : 'Linked · they can see the thread now');
    }
  });

  // publish
  $('#p-pubCancel').addEventListener('click', () => closeSheet());
  $('#p-pubGo').addEventListener('click', doPublish);

  // capture
  const capTa = $('#p-capTa');
  function capCount() {
    const c = JSON.parse(S.B.constructor.capture_counter(capTa.value));
    const el = $('#p-capCnt'); el.textContent = c.label; el.className = c.level || '';
  }
  capTa.addEventListener('input', capCount);
  const capture = async (publish) => {
    const text = capTa.value;
    let o = JSON.parse(S.B.capture(text, publish, now()));
    if (o.action === 'toast' && /Nothing/.test(o.message)) { shake($('#sh-cap')); toast(esc(o.message)); return; }
    capTa.blur();
    closeSheet({ refocus: false });
    if (o.action === 'remote_publish') { render(); o = await finishPublish(o); }
    if (o.action === 'published') {
      publishedToast(o);
      persist();
    } else {
      outcome(o);
      touched();
    }
    show('write');
    $('#p-writeScroll').scrollTop = 0;
  };
  $('#p-capKeep').addEventListener('click', () => capture(false));
  $('#p-capPub').addEventListener('click', () => capture(true));

  // shelf sheet
  $('#p-shelfSheet').addEventListener('click', (e) => {
    const b = e.target.closest('button'); if (!b) return;
    if (b.dataset.shelf) { openShelved(b.dataset.shelf); return; }
    if (b.dataset.up) S.B.move_on_shelf(b.dataset.up, +b.dataset.i - 1);
    else if (b.dataset.down) S.B.move_on_shelf(b.dataset.down, +b.dataset.i + 1);
    else if (b.dataset.off) { outcome(S.B.set_on_shelf(b.dataset.off, false)); toast('Taken off your shelf'); }
    touched();
    renderShelfSheet();
  });

  // read
  $('#p-rq').addEventListener('input', () => { streamFromTop(); renderStream(); });
  $('#p-unreadOnly').addEventListener('click', () => { unreadOnly = !unreadOnly; streamFromTop(); renderStream(); });
  onNearEnd($('#p-stream'), () => {
    if (pview !== 'read' || !$('#p-stream .stream-more')) return;
    streamN += PAGE;
    renderStream();
  });
  swipeCards($('#p-stream'), {
    onRight: (key, c) => { outcome(S.B.set_read(key, true)); persist(); renderStream(); settle(c); },
    onTap: (key) => openReading(key),
  });
  $('#p-stream').addEventListener('click', (e) => {
    const b = e.target.closest('.acts [data-act]'); if (!b) return;
    const key = b.dataset.key;
    if (b.dataset.act === 'quote') quote(key);
    else if (b.dataset.act === 'reply') reply(key);
    else hopperSheet(key);
  });
  $('#p-back2').addEventListener('click', () => show('read'));
  $('#p-replyBtn').addEventListener('click', () => reply(readKey));
  $('#p-hopBtn').addEventListener('click', () => hopperSheet(readKey));
  $('#p-smaller').addEventListener('click', () => textSize(-0.1));
  $('#p-bigger').addEventListener('click', () => textSize(0.1));
  $('#p-postBody').addEventListener('click', (e) => { const g = e.target.closest('[data-goto]'); if (g) openReading(g.dataset.goto); });
  holdSentences($('#p-postBody'), $('#p-pmenu'), (a, text) => {
    if (a === 'reply') reply(readKey, text);
    else if (a === 'quote') quote(readKey, text);
    else copyText(S.B.link_markup(readKey)).then((ok) => toast(ok ? 'Link copied · paste it into a post' : 'Couldn’t reach the clipboard'));
  });
  $('#p-hopList').addEventListener('click', (e) => {
    const b = e.target.closest('[data-hop]'); if (!b) return;
    outcome(S.B.toggle_hopper(readKey, b.dataset.hop));
    closeSheet({ refocus: false });
    persist(); renderStream();
  });

  // theme, offline, reset
  $('#p-theme').addEventListener('click', () => {
    $('#p-themeList').innerHTML = S.themes.map((t) => `<button type="button" class="opt2${t.id === S.theme?.id ? ' on' : ''}" data-theme="${esc(t.id)}">${t.id === S.theme?.id ? '✓ ' : ''}${esc(t.name)}<small>${esc(t.family)} · ${t.dark ? 'dark' : 'light'}</small></button>`).join('');
    sheet('sh-theme');
  });
  $('#p-themeList').addEventListener('click', (e) => {
    const b = e.target.closest('[data-theme]'); if (!b) return;
    applyTheme(b.dataset.theme, [document.documentElement]);
    pv._shown = null;
    closeSheet({ refocus: false });
    S.render();
  });
  $('#p-offline').addEventListener('click', () => { closeSheet({ refocus: false }); setOffline(!S.B.is_offline()); });
  $('#p-reset').addEventListener('click', async () => { closeSheet({ refocus: false }); readKey = null; await resetData(); show('write'); });
  app.dataset.view = 'write';
}

export function showPhone() {
  render({ loadEditor: true });
  $('#p-loading').classList.add('gone');
}
