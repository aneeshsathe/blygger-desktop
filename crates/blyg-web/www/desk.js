// Burrow's main window in a browser (wider than 700px). Every rule
// (filtering, selection, counters, labels, publish/delete/discard
// decisions, the shelf, the reading list, the preview) runs in Burrow's
// Rust; this draws what `Burrow.view()` returns and forwards keys to it.

import {
  $, $$, S, now, esc, persist, pref, touched, toast, outcome, publishedToast, copyText,
  finishPublish, finishDiscard, conflictBanner,
  showPreview, editorBar, runTool, renderPicks, insertPick, renderPost, holdSentences,
  swipeCards, cardHtml, applyTheme, draw, onNearEnd, settle,
} from './core.js';
// --- web-reading ---
import { deskSources, opened, wrUse } from './web-reading.js';
// --- end web-reading ---
// --- web-writing ---
import * as Wr from './writing.js';
// --- end web-writing ---

const win = $('#win'), omni = $('#omni'), ed = $('#ed'), list = $('#list'), pv = $('#pv');

let V;                 // the last view
let shownId;           // the item the editor holds
let previewing = false;
let mode = 'search';   // 'search' | 'edit'
let view = 'posts';    // 'posts' | 'reading'
let rq = '';           // the reading search
let rcur = null;       // the open reading post's key
let rkeys = [];        // the reading list, in order

// ---------------------------------------------------------------- render
export function render({ loadEditor = false } = {}) {
  if (!S.B) return;
  V = JSON.parse(S.B.view(now()));
  $('#wtitle').textContent = view === 'reading' ? 'Reading' : V.title;
  if (S.layout === 'desk') document.title = view === 'reading' ? 'Reading · Burrow' : V.title;
  $('#hint').textContent = view === 'posts' && mode === 'search' ? V.hint : '';

  list.innerHTML = V.rows.length
    ? V.rows.map((r, i) => `<div class="row${r.selected ? ' sel' : ''}${i === V.shelf_len - 1 && V.rows.length > V.shelf_len ? ' shelved-last' : ''}" role="option" aria-selected="${r.selected}" data-id="${esc(r.id)}">
        ${r.shelved ? '<span class="shelfmark" title="On your shelf">📚</span>' : ''}<span class="ti">${r.title_html}</span>
        <span class="meta">${r.dot ? `<span class="dot" title="${esc(r.dot_tip || 'unpublished edits')}"></span>` : ''}${esc(r.kind)}
        <span class="pill${r.pill_pub ? ' pub' : ''}">${esc(r.pill)}</span><span>${esc(r.when)}</span></span></div>`).join('')
    : `<div class="empty">No matches. Press ⏎ to start “${esc(V.query.trim())}”.</div>`;
  list.querySelector('.row.sel')?.scrollIntoView({ block: 'nearest' });

  const c = V.current;
  const id = c ? (c.id ?? '(new)') : null;
  if (V.wants_create && mode === 'search') {
    ed.value = '';
    ed.placeholder = `⏎ creates a new draft: “${V.query.trim()}”`;
    shownId = null;
  } else if (c && (loadEditor || id !== shownId || ed.value !== c.content && (document.activeElement !== ed || S.live))) {
    // Live, a change from outside (the blyg's copy, a conflict choice)
    // reaches the editor even while it has focus: every keystroke is in
    // the store at once, so a difference can only come from there.
    if (ed.value !== c.content) {
      const [a, b] = [ed.selectionStart, ed.selectionEnd];
      ed.value = c.content;
      if (document.activeElement === ed && id === shownId) ed.setSelectionRange(a, b);
    }
    ed.placeholder = c.status === 'new' ? 'Write something…' : '';
    shownId = id;
  } else if (!c) {
    ed.value = ''; ed.placeholder = ''; shownId = null;
  }
  const so = $('#d-stubof');
  so.hidden = !c?.stub_of;
  so.textContent = c?.stub_of ?? '';
  conflictBanner($('#d-conflict'), c, () => { render({ loadEditor: true }); touched(); });
  $('#d-mode').textContent = V.mode_label;
  $('#d-mode').className = 'modebadge ' + V.mode;

  // status bar
  $('#kind').textContent = c ? c.kind_label : '';
  const cnt = $('#cnt');
  cnt.textContent = c ? c.counter : '';
  cnt.className = 'cnt ' + (c?.level || '');
  const ban = $('#ban');
  if (c?.banner) { ban.textContent = c.banner; ban.className = 'ban over'; }
  else if (c?.new_post) { ban.textContent = c.new_post; ban.className = 'ban'; }
  else if (previewing && c) { ban.textContent = S.B.preview_stats(); ban.className = 'ban'; }
  else { ban.textContent = ''; ban.className = 'ban'; }
  const sy = $('#sync');
  sy.className = 'sync ' + V.sync_dot;
  sy.querySelector('span').textContent = V.sync;
  $('#ver').textContent = c ? `${c.shelved ? '📚 ' : ''}${c.version}` : '';
  if (previewing) schedulePreview();
  renderReading();
}

// The stream is drawn a page at a time; more as you scroll or step past
// the last card drawn.
const RPAGE = 100;
let rlimit = RPAGE;
let rmore = false;
function renderReading() {
  const R = JSON.parse(S.B.reading_page(now(), rq, false, view === 'reading' ? rlimit : 0));
  $('#d-unread').textContent = R.unread ? String(R.unread) : '';
  if (view !== 'reading') return;
  rkeys = R.rows.map((r) => r.key);
  rmore = R.total > R.rows.length;
  const host = $('#d-stream');
  draw(host, R.rows.length
    ? R.rows.map(cardHtml).join('') + (rmore ? `<div class="empty">${R.total - R.rows.length} more…</div>` : '')
    : `<div class="empty">${esc(R.empty || 'Nothing matches.')}</div>`);
  host.querySelector('.card.cur')?.classList.remove('cur');
  if (rcur) host.querySelector(`.card[data-key="${CSS.escape(rcur)}"]`)?.classList.add('cur');
  deskSources(); // --- web-reading --- the sources pane --- end web-reading ---
}

// ---------------------------------------------------------------- preview
let pvT;
function schedulePreview() {
  clearTimeout(pvT);
  pvT = setTimeout(() => showPreview(pv), 120);
}
function togglePreview() {
  previewing = !previewing;
  win.classList.toggle('previewing', previewing);
  pv._shown = null;
  render();
}

// ---------------------------------------------------------------- hud / sheet
function hud(keys) {
  const h = $('#hud'); h.innerHTML = keys.map((k) => `<kbd>${k}</kbd>`).join('');
  const c = h.innerHTML; setTimeout(() => { if (h.innerHTML === c) h.innerHTML = ''; }, 1100);
}
let sheetOpen = null; // { onEnter, back }
function openSheet(html, onEnter) {
  const s = $('#sheet'); s.innerHTML = html; s.classList.add('show');
  const back = document.activeElement;
  sheetOpen = { onEnter, back };
  const input = s.querySelector('input');
  setTimeout(() => (input || s).focus(), 60);
  if (!input) s.tabIndex = -1;
}
function closeSheet() {
  $('#sheet').classList.remove('show');
  const back = sheetOpen?.back; sheetOpen = null;
  (back && back !== document.body ? back : ed).focus();
}
const shakeEl = () => $('#sb');

// ---------------------------------------------------------------- actions
function caretToEnd() { ed.focus(); ed.setSelectionRange(ed.value.length, ed.value.length); }
function openSelected() { setView('posts'); mode = 'edit'; render({ loadEditor: true }); caretToEnd(); }
function toSearch() { mode = 'search'; omni.focus(); omni.select(); render(); }

function newPost() {
  setView('posts');
  S.B.new_post(); mode = 'edit'; render({ loadEditor: true }); ed.focus();
}
function toggleKind() {
  const o = outcome(S.B.toggle_kind(now()), shakeEl());
  if (o.action === 'toast') touched();
}
function toggleShelf() {
  const o = outcome(S.B.toggle_shelf());
  if (/shelf/.test(o.message || '')) touched();
}
function askPublish() {
  const o = outcome(S.B.publish_check(), shakeEl());
  if (o.action !== 'sheet') return;
  openSheet(`<h4>Publish “${esc(o.title)}” as v${o.next_version}</h4>
    <input id="note" placeholder="Version note (optional), e.g. “fixed a typo”" aria-label="Version note">
    <div class="sk"><span><kbd>⏎</kbd> publish</span><span><kbd>esc</kbd> cancel</span></div>`,
  () => {
    const note = $('#note').value;
    closeSheet();
    S.B.sync_phase('syncing'); render();
    win.querySelector('#sync span').textContent = 'publishing…';
    setTimeout(async () => {
      const r = await finishPublish(Wr.withNote(JSON.parse(S.B.publish(note, now())), note)); // --- web-writing ---
      if (r.action === 'published') {
        await persist();
        S.B.sync_phase('synced');
        render({ loadEditor: true });
        publishedToast(r);
      } else { S.B.sync_phase(S.live ? 'error' : 'synced'); outcome(r); render(); }
    }, S.live ? 0 : 650);
  });
  Wr.publishSheet($('#sheet'), $('#note')); // --- web-writing ---
}
function askDelete() {
  const o = outcome(S.B.delete_check(), shakeEl());
  if (o.action === 'ask_withdraw') { Wr.askWithdraw(); return; } // --- web-writing ---
  if (o.action !== 'confirm') return;
  openSheet(`<h4>${esc(o.title)}</h4><p>${esc(o.body)}</p>
    <div class="sk"><span><kbd>⏎</kbd> delete</span><span><kbd>esc</kbd> cancel</span></div>`,
  () => { closeSheet(); outcome(S.B.delete()); render({ loadEditor: true }); persist(); });
}
function askDiscard() {
  const o = outcome(S.B.discard_check(), shakeEl());
  if (o.action !== 'confirm') return;
  openSheet(`<h4>${esc(o.title)}</h4><p>${esc(o.body)}</p>
    <div class="sk"><span><kbd>⏎</kbd> discard</span><span><kbd>esc</kbd> keep the edits</span></div>`,
  async () => { closeSheet(); outcome(await finishDiscard(JSON.parse(S.B.discard(now())))); render({ loadEditor: true }); touched(); });
}
function makeDraft() {
  const o = outcome(S.B.make_draft(now()), shakeEl());
  if (o.message === 'Saved as a draft') touched();
}
function openPermalink() {
  const c = V.current;
  toast(c?.permalink
    ? `On a real blyg, ⌘O opens ${esc(c.permalink.replace(/^https?:\/\//, ''))}. This one is pretend.`
    : 'Not published yet');
}
/** ⌘K / ⇧⌘K: pick a post to link or quote at the caret. */
function askPick(quote) {
  if (view !== 'posts' || !V.current) { toast('Open a post to link from it'); return; }
  const at = [ed.selectionStart, ed.selectionEnd];
  openSheet(`<h4>${quote ? 'Quote a post' : 'Link a post'}</h4>
    <input id="pickq" placeholder="Your posts and what you read" aria-label="Search posts" autocomplete="off">
    <div class="picks" id="picks"></div>
    <div class="sk"><span><kbd>⏎</kbd> the first</span><span><kbd>esc</kbd> cancel</span></div>`,
  () => { const b = $('#picks [data-pick]'); if (b) pick(b); });
  const q = $('#pickq'), picks = $('#picks');
  renderPicks(picks, '');
  q.addEventListener('input', () => renderPicks(picks, q.value));
  picks.onclick = (e) => { const b = e.target.closest('[data-pick]'); if (b) pick(b); };
  function pick(b) {
    closeSheet();
    ed.focus(); ed.setSelectionRange(at[0], at[1]);
    if (insertPick(ed, b.dataset.pick, b.dataset.label, quote)) toast(quote ? 'Quoted · it shows in the preview' : 'Linked');
  }
}
/** ⌘B: the shelf, in order. */
function askShelf() {
  const rows = V.shelf;
  openSheet(`<h4>📚 Your shelf</h4>${rows.length
    ? `<div class="picks">${rows.map((r, i) => `<button type="button" class="opt2" data-shelf="${esc(r.id)}"><span class="n">${i + 1}</span> ${esc(r.title)}<small>${esc(r.kind)} · ${esc(r.pill)} · ${esc(r.when)}</small></button>`).join('')}</div>`
    : '<p>Nothing on your shelf yet. ⌥⌘B shelves the open post.</p>'}
    <div class="sk"><span><kbd>⏎</kbd> the first</span><span><kbd>esc</kbd> close</span></div>`,
  () => { const b = $('#sheet [data-shelf]'); if (b) openShelved(b.dataset.shelf); });
  $('#sheet').onclick = (e) => { const b = e.target.closest('[data-shelf]'); if (b) openShelved(b.dataset.shelf); };
}
function openShelved(id) {
  closeSheet();
  S.B.open_from_shelf(id); omni.value = ''; openSelected();
}

// ---------------------------------------------------------------- reading
function setView(v) {
  if (view === v) return;
  view = v;
  $$('.views [data-view]').forEach((b) => { b.classList.toggle('on', b.dataset.view === v); b.setAttribute('aria-selected', String(b.dataset.view === v)); });
  $('#pbody').hidden = v !== 'posts';
  $('#rbody').hidden = v !== 'reading';
  $('#sb').style.visibility = v === 'posts' ? '' : 'hidden';
  omni.placeholder = v === 'posts' ? 'Search or start writing…' : 'Search what you read…';
  omni.value = v === 'posts' ? V?.query ?? '' : rq;
  render();
}
function openReading(key) {
  const p = JSON.parse(S.B.open_reading(key, now()));
  if (!p) return;
  rcur = key;
  renderPost($('#d-post'), p);
  opened(key, $('#d-post')); // --- web-reading --- thumbs, links to Peek --- end web-reading ---
  $('#d-ractions').hidden = false;
  $('#d-rpane .rscroll').scrollTop = 0;
  persist();
  render();
}
function readAction(a, key = rcur, passage = '') {
  if (!key) return;
  if (a === 'reply' || a === 'quote') {
    const o = JSON.parse(a === 'reply' ? S.B.reply_to(key, passage, now()) : S.B.quote_new(key, passage, now()));
    if (o.action === 'create') {
      toast(a === 'reply' ? (passage ? 'A stub replying to that passage' : 'A stub replying to this post') : 'A new thread quoting it');
      omni.value = '';
      openSelected(); touched();
    }
  } else if (a === 'link') {
    copyText(S.B.link_markup(key)).then((ok) => toast(ok ? 'Link copied · paste it into a post' : 'Couldn’t reach the clipboard'));
  } else if (a === 'unread') {
    outcome(S.B.set_read(key, false)); persist(); render();
  } else if (a === 'hopper') {
    const hs = JSON.parse(S.B.hoppers(key));
    openSheet(`<h4>⊕ Put it in a hopper</h4><div class="picks">${hs.map((h) => `<button type="button" class="opt2${h.on ? ' on' : ''}" data-hop="${esc(h.id)}">${h.on ? '✓ ' : ''}${esc(h.name)}</button>`).join('')}</div>
      <div class="sk"><span><kbd>esc</kbd> close</span></div>`, closeSheet);
    $('#sheet').onclick = (e) => {
      const b = e.target.closest('[data-hop]'); if (!b) return;
      outcome(S.B.toggle_hopper(key, b.dataset.hop)); closeSheet(); persist(); render();
    };
  } else if (a === 'smaller' || a === 'bigger') {
    const post = $('#d-post');
    const cur = parseFloat(getComputedStyle(post).getPropertyValue('--rsize')) || 19;
    const next = Math.max(14, Math.min(30, cur + (a === 'bigger' ? 2 : -2)));
    post.style.setProperty('--rsize', `${next}px`);
    pref.set('desk-rsize', String(next));
  }
}

// ---------------------------------------------------------------- events
export function initDesk() {
  wrUse('desk', { openReading: (k) => { setView('reading'); openReading(k); } }); // --- web-reading --- --- end web-reading ---
  omni.addEventListener('focus', () => { if (view === 'posts') { mode = 'search'; render(); } });
  omni.addEventListener('input', () => {
    if (view === 'reading') { rq = omni.value; rlimit = RPAGE; $('#d-stream').scrollTop = 0; render(); return; }
    S.B.set_query(omni.value); render({ loadEditor: true });
  });
  omni.addEventListener('keydown', (e) => {
    if (view === 'reading') {
      if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
        e.preventDefault();
        const i = rkeys.indexOf(rcur) + (e.key === 'ArrowDown' ? 1 : -1);
        if (i >= rkeys.length && rmore) { rlimit += RPAGE; renderReading(); }
        const k = rkeys[Math.max(0, Math.min(rkeys.length - 1, i))];
        if (k) openReading(k);
      } else if (e.key === 'Escape') { omni.value = ''; rq = ''; rlimit = RPAGE; render(); }
      return;
    }
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault();
      S.B.move_selection(e.key === 'ArrowDown' ? 1 : -1);
      render({ loadEditor: true }); hud([e.key === 'ArrowDown' ? '↓' : '↑']);
    } else if (e.key === 'Enter' && !(e.metaKey || e.ctrlKey)) {
      e.preventDefault(); hud(['⏎']);
      const o = JSON.parse(S.B.enter(now()));
      if (o.action === 'create') { omni.value = ''; toast('New draft created'); openSelected(); touched(); }
      else if (o.action === 'open') openSelected();
    } else if (e.key === 'Escape') {
      omni.value = ''; S.B.set_query(''); render({ loadEditor: true }); hud(['esc']);
    }
  });
  ed.addEventListener('focus', () => {
    if (mode === 'search' && V?.wants_create) {
      S.B.create_draft(V.query.trim(), now());
      omni.value = ''; mode = 'edit'; render({ loadEditor: true }); caretToEnd(); touched();
      return;
    }
    mode = 'edit'; render();
  });
  ed.addEventListener('input', () => { if (S.B.edit(ed.value, now())) touched(); else render(); });
  list.addEventListener('click', (e) => {
    const r = e.target.closest('.row'); if (!r) return;
    S.B.open(r.dataset.id); openSelected();
  });
  $('#kind').addEventListener('click', toggleKind);
  $('#sheet').addEventListener('keydown', (e) => {
    if (e.key === 'Enter') { e.preventDefault(); e.stopPropagation(); sheetOpen?.onEnter(); }
    if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); closeSheet(); }
  });
  editorBar($('#d-bar'), (tool) => runTool(tool, ed, (name) => {
    if (name === 'link') askPick(false);
    else if (name === 'quote') askPick(true);
    else if (name === 'kind') toggleKind();
    else if (name === 'shelf') toggleShelf();
    else if (name === 'preview') togglePreview();
    else if (name === 'publish') askPublish();
    else Wr.W.barTool(name); // --- web-writing ---
  }));
  // --- web-writing ---
  Wr.attachEditor(ed);
  Wr.setWritingHooks('desk', { openEditor: () => openSelected(), afterDelete: () => render({ loadEditor: true }) });
  $('#d-wmenu').addEventListener('click', () => Wr.openPostMenu());
  $('#d-wfork').addEventListener('click', () => Wr.askFork(rcur));
  $('#d-wset').addEventListener('click', () => Wr.openSettings());
  // --- end web-writing ---
  $$('.views [data-view]').forEach((b) => b.addEventListener('click', () => setView(b.dataset.view)));

  // reading
  swipeCards($('#d-stream'), {
    onRight: (key, c) => { outcome(S.B.set_read(key, true)); persist(); render(); settle(c); },
    onTap: (key) => openReading(key),
  });
  $('#d-stream').addEventListener('click', (e) => {
    const b = e.target.closest('.acts [data-act]'); if (!b) return;
    readAction(b.dataset.act, b.dataset.key);
  });
  // The stream scrolls itself, or (when the window is taller) the page does.
  const moreReading = () => { if (S.layout === 'desk' && view === 'reading' && rmore) { rlimit += RPAGE; renderReading(); } };
  onNearEnd($('#d-stream'), moreReading);
  onNearEnd(window, moreReading);
  $('#d-ractions').addEventListener('click', (e) => { const b = e.target.closest('[data-ra]'); if (b) readAction(b.dataset.ra); });
  $('#d-post').addEventListener('click', (e) => { const g = e.target.closest('[data-goto]'); if (g) openReading(g.dataset.goto); });
  holdSentences($('#d-post'), $('#d-pmenu'), (a, text) => readAction(a === 'link' ? 'link' : a, rcur, a === 'link' ? '' : text));
  const rs = parseFloat(pref.get('desk-rsize'));
  if (rs) $('#d-post').style.setProperty('--rsize', `${rs}px`);

  // ⌘ is Burrow's modifier; the browser keeps ⌘N/⌘T/⌘L, so ⌃ and ⌥ work too.
  // `e.code` because ⌥ changes `e.key` on a Mac (⌥N is "˜").
  document.addEventListener('keydown', (e) => {
    if (sheetOpen || !S.B || S.layout !== 'desk') return;
    if (S.writing?.isOpen()) return; // --- web-writing ---
    if ((e.metaKey || e.ctrlKey) && e.shiftKey && e.code === 'KeyY') { e.preventDefault(); hud(['⌃', '⇧', 'Y']); Wr.openHistory(); return; } // --- web-writing ---
    const mod = e.metaKey || e.ctrlKey || e.altKey;
    const cmd = e.metaKey || e.ctrlKey;
    const code = e.code;
    if (e.ctrlKey && !e.shiftKey && (code === 'Digit1' || code === 'Digit2')) { e.preventDefault(); hud(['⌃', code.slice(-1)]); setView(code === 'Digit1' ? 'posts' : 'reading'); omni.focus(); }
    else if (mod && e.key === 'Enter') { e.preventDefault(); hud(['⌘', '⏎']); askPublish(); }
    else if (mod && e.shiftKey && (e.key === 'Backspace' || e.key === 'Delete')) { e.preventDefault(); hud(['⇧', '⌘', '⌫']); askDelete(); }
    else if (mod && e.altKey && cmd && code === 'KeyZ') { e.preventDefault(); hud(['⌥', '⌘', 'Z']); askDiscard(); }
    else if (mod && e.altKey && cmd && code === 'KeyB') { e.preventDefault(); hud(['⌥', '⌘', 'B']); toggleShelf(); }
    else if (mod && !e.shiftKey && code === 'KeyB') { e.preventDefault(); hud(['⌘', 'B']); askShelf(); }
    else if (mod && code === 'KeyK') { e.preventDefault(); hud(e.shiftKey ? ['⇧', '⌘', 'K'] : ['⌘', 'K']); askPick(e.shiftKey); }
    else if (mod && !e.shiftKey && code === 'KeyT') { e.preventDefault(); hud(['⌘', 'T']); toggleKind(); }
    else if (mod && !e.shiftKey && code === 'KeyL') { e.preventDefault(); hud(['⌘', 'L']); if (view === 'posts') toSearch(); else omni.focus(); }
    else if (mod && !e.shiftKey && code === 'KeyE') { e.preventDefault(); hud(['⌘', 'E']); togglePreview(); }
    else if (mod && !e.shiftKey && code === 'KeyN') { e.preventDefault(); hud(['⌘', 'N']); newPost(); }
    else if (mod && !e.shiftKey && code === 'KeyD') { e.preventDefault(); hud(['⌘', 'D']); makeDraft(); }
    else if (mod && !e.shiftKey && code === 'KeyO') { e.preventDefault(); hud(['⌘', 'O']); openPermalink(); }
    else if (e.key === 'Escape' && document.activeElement === ed) { hud(['esc']); toSearch(); }
  });

  // chrome
  const setLayout = (l) => {
    win.classList.toggle('side', l === 'side');
    $$('[data-layout]').forEach((b) => b.classList.toggle('on', b.dataset.layout === l));
    pref.set('layout', l);
  };
  $$('[data-layout]').forEach((b) => (b.onclick = () => setLayout(b.dataset.layout)));
  setLayout(pref.get('layout') || 'stacked');
  $('#theme').innerHTML = S.themes.map((t) => `<option value="${esc(t.id)}">${esc(t.name)} (${esc(t.family)}, ${t.dark ? 'dark' : 'light'})</option>`).join('');
  $('#theme').value = S.theme?.id ?? '';
  $('#theme').onchange = (e) => { applyTheme(e.target.value, [document.documentElement]); pv._shown = null; S.render(); };
  $('#offline').onclick = () => setOffline(!S.B.is_offline());
  $('#reset').onclick = resetData;
}

/** Shared by both layouts' chrome. */
export function setOffline(off) {
  const pending = S.B.set_offline(off);
  for (const b of [$('#offline'), $('#p-offline')]) {
    b.setAttribute('aria-pressed', String(off));
    b.textContent = off ? 'Go online' : 'Go offline';
  }
  if (off) toast('Offline. Keep writing: nothing is lost.');
  else if (pending) { toast(`Back online · ${pending} change${pending === 1 ? '' : 's'} synced`); touched(); }
  else toast('Back online');
  S.render();
}
export async function resetData() {
  S.B.reset(now()); omni.value = ''; mode = 'search'; rcur = null;
  $('#d-post').innerHTML = '<p class="muted">Pick a post to read it.</p>';
  $('#d-ractions').hidden = true;
  await persist();
  S.render({ loadEditor: true }); toast('Back to the sample data');
}

export function showDesk() {
  $('#theme').value = S.theme?.id ?? '';
  render({ loadEditor: true });
  $('#loading').classList.add('gone');
  if (!document.activeElement || document.activeElement === document.body) omni.focus();
}
export const deskEditorFocused = () => document.activeElement === ed;
