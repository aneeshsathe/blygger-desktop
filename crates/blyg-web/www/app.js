// Burrow's main window in a browser. Every rule (filtering, selection,
// counters, labels, publish/delete/discard decisions, the preview) runs in
// Burrow's Rust, compiled to WebAssembly (pkg/blyg_web.js). This file only
// draws what `Burrow.view()` returns and forwards keys and edits to it.

import init, { Burrow } from './pkg/blyg_web.js';

const $ = (s) => document.querySelector(s);
const win = $('#win'), omni = $('#omni'), ed = $('#ed'), list = $('#list'), pv = $('#pv');
const now = () => Date.now();

// ---------------------------------------------------------------- storage
// The whole store is one JSON value under one IndexedDB key. Every call is
// guarded: a private window or blocked storage just means no persistence.
const DB = 'burrow-web', OS = 'kv', KEY = 'store';
function idb() {
  return new Promise((resolve, reject) => {
    const req = indexedDB.open(DB, 1);
    req.onupgradeneeded = () => req.result.createObjectStore(OS);
    req.onsuccess = () => resolve(req.result);
    req.onerror = () => reject(req.error);
  });
}
async function load() {
  try {
    const db = await idb();
    return await new Promise((resolve) => {
      const r = db.transaction(OS).objectStore(OS).get(KEY);
      r.onsuccess = () => resolve(typeof r.result === 'string' ? r.result : undefined);
      r.onerror = () => resolve(undefined);
    });
  } catch { return undefined; }
}
let dbp;
async function persist() {
  try {
    dbp ??= idb();
    const db = await dbp;
    await new Promise((resolve, reject) => {
      const tx = db.transaction(OS, 'readwrite');
      tx.objectStore(OS).put(B.to_json(), KEY);
      tx.oncomplete = resolve; tx.onerror = () => reject(tx.error);
    });
    return true;
  } catch { return false; }
}
const pref = {
  get(k) { try { return localStorage.getItem('burrow-web.' + k); } catch { return null; } },
  set(k, v) { try { localStorage.setItem('burrow-web.' + k, v); } catch {} },
};

// ---------------------------------------------------------------- state
let B;                 // the wasm Burrow
let V;                 // the last view
let shownId;           // the item the editor holds
let previewing = false;
let mode = 'search';   // 'search' | 'edit'

function esc(s) { return String(s).replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c])); }

// ---------------------------------------------------------------- render
function render({ loadEditor = false } = {}) {
  V = JSON.parse(B.view(now()));
  $('#wtitle').textContent = V.title;
  document.title = V.title;
  $('#hint').textContent = mode === 'search' ? V.hint : '';

  list.innerHTML = V.rows.length
    ? V.rows.map((r) => `<div class="row${r.selected ? ' sel' : ''}" role="option" aria-selected="${r.selected}" data-id="${esc(r.id)}">
        <span class="ti">${r.title_html}</span>
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
  } else if (c && (loadEditor || id !== shownId || (document.activeElement !== ed && ed.value !== c.content))) {
    if (ed.value !== c.content) ed.value = c.content;
    ed.placeholder = c.status === 'new' ? 'Write something…' : '';
    shownId = id;
  } else if (!c) {
    ed.value = ''; ed.placeholder = ''; shownId = null;
  }

  // status bar
  $('#kind').textContent = c ? c.kind_label : '';
  const cnt = $('#cnt');
  cnt.textContent = c ? c.counter : '';
  cnt.className = 'cnt ' + (c?.level || '');
  const ban = $('#ban');
  if (c?.banner) { ban.textContent = c.banner; ban.className = 'ban over'; }
  else if (c?.new_post) { ban.textContent = c.new_post; ban.className = 'ban'; }
  else if (previewing && c) { ban.textContent = B.preview_stats(); ban.className = 'ban'; }
  else { ban.textContent = ''; ban.className = 'ban'; }
  const sy = $('#sync');
  sy.className = 'sync ' + V.sync_dot;
  sy.querySelector('span').textContent = V.sync;
  $('#ver').textContent = c ? c.version : '';
  if (previewing) schedulePreview();
}

// ---------------------------------------------------------------- preview
let pvT, pvShown;
function nonce() {
  const a = new Uint8Array(16);
  crypto.getRandomValues(a);
  return [...a].map((b) => b.toString(16).padStart(2, '0')).join('');
}
function schedulePreview() {
  clearTimeout(pvT);
  pvT = setTimeout(() => {
    const page = B.preview_page(nonce(), currentTheme?.dark ?? false);
    // Same page but for the nonce: leave the frame alone (no flicker).
    const key = page.replace(/nonce-[0-9a-f]+|nonce="[0-9a-f]+"/g, '');
    if (key !== pvShown) { pv.srcdoc = page; pvShown = key; }
  }, 120);
}
function togglePreview() {
  previewing = !previewing;
  win.classList.toggle('previewing', previewing);
  pvShown = null;
  render();
}

// ---------------------------------------------------------------- toast / hud / sheet
let toastT;
function toast(html, ms = 2600) {
  const t = $('#toast'); t.innerHTML = html; t.classList.add('show');
  clearTimeout(toastT); toastT = setTimeout(() => t.classList.remove('show'), ms);
}
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
function shake() {
  const sb = $('#sb'); sb.classList.remove('shake'); void sb.offsetWidth; sb.classList.add('shake');
}
function outcome(o) {
  switch (o.action) {
    case 'toast': toast(esc(o.message)); break;
    case 'shake': shake(); toast(esc(o.message)); break;
  }
  return o;
}

// ---------------------------------------------------------------- sync
// Each keystroke is in the store at once ("saved in this browser"); ~800 ms
// after typing stops the store is written to IndexedDB, the demo's push.
let syncT;
function touched() {
  render();
  clearTimeout(syncT);
  if (B.is_offline()) { persist(); return; }
  syncT = setTimeout(async () => {
    B.sync_phase('syncing'); render();
    const ok = await persist();
    setTimeout(() => { B.sync_phase(ok ? 'synced' : 'error'); render(); }, 350);
  }, 800);
}

// ---------------------------------------------------------------- actions
function caretToEnd() { ed.focus(); ed.setSelectionRange(ed.value.length, ed.value.length); }
function openSelected() { mode = 'edit'; render({ loadEditor: true }); caretToEnd(); }
function toSearch() { mode = 'search'; omni.focus(); omni.select(); render(); }

function newPost() {
  B.new_post(); mode = 'edit'; render({ loadEditor: true }); ed.focus();
}
function toggleKind() {
  const o = outcome(JSON.parse(B.toggle_kind(now())));
  if (o.action === 'toast') touched();
}
function askPublish() {
  const o = outcome(JSON.parse(B.publish_check()));
  if (o.action !== 'sheet') return;
  openSheet(`<h4>Publish “${esc(o.title)}” as v${o.next_version}</h4>
    <input id="note" placeholder="Version note (optional), e.g. “fixed a typo”" aria-label="Version note">
    <div class="sk"><span><kbd>⏎</kbd> publish</span><span><kbd>esc</kbd> cancel</span></div>`,
  () => {
    const note = $('#note').value;
    closeSheet();
    B.sync_phase('syncing'); render();
    win.querySelector('#sync span').textContent = 'publishing…';
    setTimeout(async () => {
      const r = JSON.parse(B.publish(note, now()));
      if (r.action === 'published') {
        await persist();
        B.sync_phase('synced');
        render({ loadEditor: true });
        toast(`Published v${r.version}${r.note ? ` · “${esc(r.note)}”` : ''}<br><small>${esc(r.permalink.replace(/^https?:\/\//, ''))} (a pretend blyg)</small>`, 3200);
      }
    }, 650);
  });
}
function askDelete() {
  const o = outcome(JSON.parse(B.delete_check()));
  if (o.action !== 'confirm') return;
  openSheet(`<h4>${esc(o.title)}</h4><p>${esc(o.body)}</p>
    <div class="sk"><span><kbd>⏎</kbd> delete</span><span><kbd>esc</kbd> cancel</span></div>`,
  () => { closeSheet(); outcome(JSON.parse(B.delete())); render({ loadEditor: true }); persist(); });
}
function askDiscard() {
  const o = outcome(JSON.parse(B.discard_check()));
  if (o.action !== 'confirm') return;
  openSheet(`<h4>${esc(o.title)}</h4><p>${esc(o.body)}</p>
    <div class="sk"><span><kbd>⏎</kbd> discard</span><span><kbd>esc</kbd> keep the edits</span></div>`,
  () => { closeSheet(); outcome(JSON.parse(B.discard(now()))); render({ loadEditor: true }); touched(); });
}
function makeDraft() {
  const o = outcome(JSON.parse(B.make_draft(now())));
  if (o.message === 'Saved as a draft') touched();
}
function openPermalink() {
  const c = V.current;
  toast(c?.permalink
    ? `On a real blyg, ⌘O opens ${esc(c.permalink.replace(/^https?:\/\//, ''))}. This one is pretend.`
    : 'Not published yet');
}

// ---------------------------------------------------------------- events
omni.addEventListener('focus', () => { mode = 'search'; render(); });
omni.addEventListener('input', () => { B.set_query(omni.value); render({ loadEditor: true }); });
omni.addEventListener('keydown', (e) => {
  if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
    e.preventDefault();
    B.move_selection(e.key === 'ArrowDown' ? 1 : -1);
    render({ loadEditor: true }); hud([e.key === 'ArrowDown' ? '↓' : '↑']);
  } else if (e.key === 'Enter' && !(e.metaKey || e.ctrlKey)) {
    e.preventDefault(); hud(['⏎']);
    const o = JSON.parse(B.enter(now()));
    if (o.action === 'create') { omni.value = ''; toast('New draft created'); openSelected(); touched(); }
    else if (o.action === 'open') openSelected();
  } else if (e.key === 'Escape') {
    omni.value = ''; B.set_query(''); render({ loadEditor: true }); hud(['esc']);
  }
});
ed.addEventListener('focus', () => {
  if (mode === 'search' && V?.wants_create) {
    const id = B.create_draft(V.query.trim(), now());
    omni.value = ''; mode = 'edit'; render({ loadEditor: true }); caretToEnd(); touched();
    return id;
  }
  mode = 'edit'; render();
});
ed.addEventListener('input', () => { if (B.edit(ed.value, now())) touched(); else render(); });
list.addEventListener('click', (e) => {
  const r = e.target.closest('.row'); if (!r) return;
  B.open(r.dataset.id); openSelected();
});
$('#kind').addEventListener('click', toggleKind);
$('#sheet').addEventListener('keydown', (e) => {
  if (e.key === 'Enter') { e.preventDefault(); e.stopPropagation(); sheetOpen?.onEnter(); }
  if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); closeSheet(); }
});

// ⌘ is Burrow's modifier; the browser keeps ⌘N/⌘T/⌘L, so ⌃ and ⌥ work too.
// `e.code` because ⌥ changes `e.key` on a Mac (⌥N is "˜").
document.addEventListener('keydown', (e) => {
  if (sheetOpen || !B) return;
  const mod = e.metaKey || e.ctrlKey || e.altKey;
  const code = e.code;
  if (mod && e.key === 'Enter') { e.preventDefault(); hud(['⌘', '⏎']); askPublish(); }
  else if (mod && e.shiftKey && (e.key === 'Backspace' || e.key === 'Delete')) { e.preventDefault(); hud(['⇧', '⌘', '⌫']); askDelete(); }
  else if (mod && e.altKey && (e.metaKey || e.ctrlKey) && code === 'KeyZ') { e.preventDefault(); hud(['⌥', '⌘', 'Z']); askDiscard(); }
  else if (mod && !e.shiftKey && code === 'KeyT') { e.preventDefault(); hud(['⌘', 'T']); toggleKind(); }
  else if (mod && !e.shiftKey && code === 'KeyL') { e.preventDefault(); hud(['⌘', 'L']); toSearch(); }
  else if (mod && !e.shiftKey && code === 'KeyE') { e.preventDefault(); hud(['⌘', 'E']); togglePreview(); }
  else if (mod && !e.shiftKey && code === 'KeyN') { e.preventDefault(); hud(['⌘', 'N']); newPost(); }
  else if (mod && !e.shiftKey && code === 'KeyD') { e.preventDefault(); hud(['⌘', 'D']); makeDraft(); }
  else if (mod && !e.shiftKey && code === 'KeyO') { e.preventDefault(); hud(['⌘', 'O']); openPermalink(); }
  else if (e.key === 'Escape' && document.activeElement === ed) { hud(['esc']); toSearch(); }
});

// ---------------------------------------------------------------- chrome
let themes = [], currentTheme;
const FALLBACK_SERIF = 'ETBook, "Iowan Old Style", Charter, Georgia, serif';
function applyTheme(id) {
  const t = themes.find((x) => x.id === id) || themes[0];
  if (!t) return;
  currentTheme = t;
  for (const [k, v] of t.vars) win.style.setProperty(k, v);
  const font = (f, fallback) => (f ? `"${f}", ${fallback}` : fallback);
  win.style.setProperty('--fbody', font(t.font_writing === 'ET Book' ? 'ETBook' : t.font_writing, FALLBACK_SERIF));
  win.style.setProperty('--fui', font(t.font_ui === 'ET Book' ? 'ETBook' : t.font_ui, t.font_ui ? 'Inter, system-ui, sans-serif' : FALLBACK_SERIF));
  win.style.setProperty('--radius', `${t.radius}px`);
  $('#theme').value = t.id;
  pref.set('theme', t.id);
  pvShown = null;
  if (previewing) schedulePreview();
}
function setLayout(l) {
  win.classList.toggle('side', l === 'side');
  document.querySelectorAll('[data-layout]').forEach((b) => b.classList.toggle('on', b.dataset.layout === l));
  pref.set('layout', l);
}
document.querySelectorAll('[data-layout]').forEach((b) => (b.onclick = () => setLayout(b.dataset.layout)));
$('#theme').onchange = (e) => applyTheme(e.target.value);
$('#offline').onclick = (e) => {
  const off = !B.is_offline();
  const pending = B.set_offline(off);
  e.currentTarget.setAttribute('aria-pressed', String(off));
  e.currentTarget.textContent = off ? 'Go online' : 'Go offline';
  if (off) toast('Offline. Keep writing: nothing is lost.');
  else if (pending) { toast(`Back online · ${pending} change${pending === 1 ? '' : 's'} synced`); touched(); }
  else toast('Back online');
  render();
};
$('#reset').onclick = async () => {
  B.reset(now()); omni.value = ''; mode = 'search'; await persist();
  render({ loadEditor: true }); toast('Back to the sample data');
};

// ---------------------------------------------------------------- start
(async () => {
  try {
    await init();
    themes = JSON.parse(Burrow.themes());
    $('#theme').innerHTML = themes.map((t) => `<option value="${esc(t.id)}">${esc(t.name)} (${esc(t.family)}, ${t.dark ? 'dark' : 'light'})</option>`).join('');
    const dark = matchMedia('(prefers-color-scheme: dark)').matches;
    applyTheme(pref.get('theme') || (dark ? 'dark' : 'light'));
    setLayout(pref.get('layout') || 'stacked');
    B = new Burrow(now(), await load());
    render({ loadEditor: true });
    $('#loading').classList.add('gone');
    omni.focus();
    // Relative times ("2m") keep moving.
    setInterval(() => { if (document.activeElement !== ed) render(); }, 30_000);
  } catch (err) {
    const l = $('#loading');
    l.classList.add('err');
    l.textContent = `Burrow couldn't start: ${err?.message || err}. It needs a browser with WebAssembly, served over http (not file://).`;
    console.error(err);
  }
})();
