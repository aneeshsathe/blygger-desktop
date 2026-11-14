// --- web-writing --- The writing and blyg side of Studio, for both
// layouts: images (file picker, paste, drop; the photo picker on a phone)
// with alt text; your post's versions (history, view, word diff, restore,
// pin), withdraw, forking a pinned version; TK generation on the blyg; the
// publish sheet's drafted note and change kind; the blyg's settings; and
// the `@` list with your contacts.
//
// Every rule is Burrow's Rust (src/writing.rs, `w_*`): it says what to ask
// the blyg and what the answer means. This draws, and fetches through
// live.js `call()` (same-origin /api/ only). Nothing here runs while you
// type, except the `@` list once you type an `@`.

import { Pager } from './pkg/blyg_web.js';
import { $, $$, S, now, esc, persist, touched, toast, outcome, insertText, nonce, isPhone, MD } from './core.js';
import { call } from './live.js';

const W = {};
S.writing = W;
/** Set by each layout (`phone`, `desk`): `openEditor()` opens the editor
    on the selected post; `afterDelete()` tidies up after a delete. */
const hooks = {};
export function setWritingHooks(layout, h) { hooks[layout] = h; }
const hook = (name) => hooks[S.layout]?.[name]?.();

const live = () => !!S.live;
const J = (s) => (typeof s === 'string' ? JSON.parse(s) : s);
const errText = (o) => o?.error?.text || o?.error?.message || o?.why || '';
/** The editor on screen: the phone's or the desk's. */
const editor = () => (isPhone() ? $('#p-ta') : $('#ed'));
const currentId = () => J(S.B.view(now())).current?.id ?? null;

/** A whole cursor-paged list, as live.js reads its own. */
async function readList(path) {
  const p = new Pager(path, '', 100);
  try {
    for (let next = p.next_path(); next !== undefined; next = p.next_path()) {
      const r = await call('GET', next);
      const err = J(p.feed(r.status, r.body, r.retryAfter));
      if (err) return { error: err };
    }
    return { json: p.items() };
  } finally { p.free(); }
}

/** Send what waits first, so the blyg has your words and their revision. */
async function flushed() { if (live()) await S.live.flush(); }

/** A command on item `id` (pin, withdraw, generate): send it, then read the
    item again, as publish does. `ok(body)` says what happened. */
async function command(id, req, ok) {
  const r = await call(req.method, req.path, req.body);
  const a = J(S.B.live_command_answer(id, r.status, r.body));
  if (a.conflict) {
    const g = await call('GET', a.fetch);
    S.B.live_conflict(id, g.status, g.body);
    await persist({ quiet: true });
    S.render({ loadEditor: true });
    toast('Changed elsewhere since you opened it. Choose which copy to keep, then try again.', 4200);
    return false;
  }
  if (a.error) { toast(esc(a.error.text), 4200); return false; }
  const g = await call('GET', a.fetch);
  const res = J(S.B.live_apply_item(id, g.status, g.body, ''));
  await persist({ quiet: true });
  S.render({ loadEditor: true });
  if (res.error) { toast(esc(res.error.text), 4200); return false; }
  toast(esc(ok(r.body)), 3600);
  return true;
}

// ---------------------------------------------------------------- the sheet
// One sheet for everything here: a bottom sheet on a phone, a panel over
// the window on a desk.
let back = null, savedSel = null, onClose = null;
function sheetEls() {
  let s = $('#w-sheet');
  if (!s) {
    document.body.insertAdjacentHTML('beforeend', `<div class="w-scrim" id="w-scrim" hidden></div>
      <div class="w-sheet" id="w-sheet" role="dialog" aria-modal="true" hidden></div>`);
    s = $('#w-sheet');
    $('#w-scrim').addEventListener('click', () => closeSheet());
    s.addEventListener('keydown', (e) => {
      if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); closeSheet(); }
    });
  }
  return s;
}
function openSheet(html, { label = '', close } = {}) {
  const s = sheetEls();
  if (s.hidden) {
    back = document.activeElement;
    const ta = editor();
    savedSel = back === ta ? [ta.selectionStart, ta.selectionEnd] : null;
  }
  onClose = close || null;
  s.innerHTML = `<div class="w-grab"></div>${html}`;
  s.setAttribute('aria-label', label);
  s.hidden = false; $('#w-scrim').hidden = false;
  document.documentElement.classList.add('w-open');
  const f = s.querySelector('[autofocus]') || s.querySelector('input, textarea, button');
  setTimeout(() => f?.focus({ preventScroll: true }), 60);
  return s;
}
export function closeSheet({ refocus = true } = {}) {
  const s = $('#w-sheet');
  if (!s || s.hidden) return;
  s.hidden = true; $('#w-scrim').hidden = true;
  s.innerHTML = '';
  document.documentElement.classList.remove('w-open');
  const c = onClose; onClose = null; c?.();
  if (!refocus) return;
  const ta = editor();
  if (savedSel && back === ta) { ta.focus({ preventScroll: true }); ta.setSelectionRange(savedSel[0], savedSel[1]); }
  else if (back && back !== document.body && document.contains(back)) back.focus?.({ preventScroll: true });
}
W.isOpen = () => !!$('#w-sheet') && !$('#w-sheet').hidden;

// ---------------------------------------------------------------- the ⋯ menu
export function openPostMenu() {
  const m = J(S.B.w_post_menu());
  if (!m.items.length) { toast('Open a post first'); return; }
  const s = openSheet(`<h4>This post</h4><div class="w-list">${m.items.map((i) =>
    `<button type="button" class="w-opt" data-act="${esc(i.act)}" ${i.on ? '' : 'disabled'}>${esc(i.label)}<small>${esc(i.sub)}</small></button>`).join('')}</div>`, { label: 'This post' });
  s.onclick = (e) => {
    const b = e.target.closest('[data-act]'); if (!b || b.disabled) return;
    const act = b.dataset.act;
    closeSheet({ refocus: act === 'image' });
    if (act === 'image') pickImage();
    else if (act === 'tk') openTk();
    else if (act === 'history') openHistory();
    else if (act === 'withdraw') askWithdraw();
    else if (act === 'copies') openCopies(); // --- syndication ---
    else if (act === 'discard') askDiscard();
    else if (act === 'delete') askDelete();
  };
}

/** Editor-bar tools that are this module's. True when handled. */
W.barTool = (name) => {
  if (name === 'upload') { pickImage(); return true; }
  if (name === 'gen') { openTk(); return true; }
  if (name === 'more') { openPostMenu(); return true; }
  return false;
};

// ---------------------------------------------------------------- images
let fileInput = null;
export function pickImage() {
  if (!currentId() && !editor().value) { toast('Write something first, then add an image'); return; }
  const plan = J(S.B.w_image_plan('image/png', 1));
  if (plan.plan === 'refuse') {
    if (!live()) { insertText(editor(), '![description](https://)', 2, 13); toast(esc(plan.why), 4200); return; }
    toast(esc(plan.why), 4200); return;
  }
  if (!fileInput) {
    fileInput = Object.assign(document.createElement('input'), { type: 'file', accept: 'image/*', hidden: true });
    document.body.append(fileInput);
    fileInput.addEventListener('change', () => { const f = fileInput.files?.[0]; fileInput.value = ''; if (f) askAlt(f); });
  }
  const ta = editor();
  savedSel = [ta.selectionStart, ta.selectionEnd];
  back = ta;
  fileInput.click();
}

/** Paste or drop an image into the editor. */
function imageFrom(dt) {
  return [...(dt?.files || [])].find((f) => f.type.startsWith('image/'));
}

function askAlt(file) {
  const plan = J(S.B.w_image_plan(file.type || 'application/octet-stream', file.size));
  if (plan.plan === 'refuse') { toast(esc(plan.why), 4200); return; }
  const ta = editor();
  const sel = document.activeElement === ta || back === ta ? [ta.selectionStart, ta.selectionEnd] : [ta.value.length, ta.value.length];
  const url = URL.createObjectURL(file);
  const s = openSheet(`<h4>Add an image</h4>
    <div class="w-img"><img src="${url}" alt=""><small>${esc(file.name || 'pasted image')} · ${Math.round(file.size / 1024)} KB${plan.plan === 'reencode' ? ' · resized to fit the blyg’s 5 MB' : ''}</small></div>
    <label class="w-lbl" for="w-alt">Alt text: what's in it, for someone who can't see it</label>
    <textarea id="w-alt" class="w-ta short" placeholder="A loaf of bread cooling on a window sill" autofocus></textarea>
    <div class="w-row"><span class="w-flex"></span><button type="button" data-x="cancel">Cancel</button><button type="button" class="pri" data-x="go">Upload</button></div>`,
  { label: 'Add an image', close: () => URL.revokeObjectURL(url) });
  savedSel = sel; back = ta;
  const go = () => { const alt = $('#w-alt').value.trim(); closeSheet(); upload(file, alt, plan.plan === 'reencode', sel); };
  s.onclick = (e) => { const b = e.target.closest('[data-x]'); if (!b) return; if (b.dataset.x === 'go') go(); else closeSheet(); };
  $('#w-alt').addEventListener('keydown', (e) => { if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); go(); } });
}

async function upload(file, alt, reencode, sel) {
  const ta = editor();
  const begin = J(S.B.w_upload_begin());
  // The placeholder on its own paragraph at the caret, as typing would.
  ta.focus({ preventScroll: true });
  ta.setSelectionRange(sel[0], sel[1]);
  const v = ta.value, at = sel[0];
  const pre = at === 0 ? '' : v.slice(0, at).endsWith('\n\n') ? '' : v.slice(0, at).endsWith('\n') ? '\n' : '\n\n';
  const post = v.slice(sel[1]).startsWith('\n\n') ? '' : v.slice(sel[1]).startsWith('\n') ? '\n' : '\n\n';
  insertText(ta, pre + begin.placeholder + post);
  const id = currentId();
  if (!id) { toast('Couldn’t place the image'); return; }
  toast('Uploading the image…', 8000);
  let blob = file, thumb = '';
  try {
    if (reencode) blob = await shrink(file, 2560, 5 * 1024 * 1024);
    thumb = await shrink(file, 720, 300_000, true);
  } catch {
    if (reencode) { finish(id, begin.n, ''); toast('That image couldn’t be read here. Try a JPEG or PNG.', 4200); return; }
  }
  await flushed();
  const fd = new FormData();
  const ext = (blob.type.split('/')[1] || 'png').replace('jpeg', 'jpg').replace('svg+xml', 'svg');
  fd.append('file', blob, `image.${ext}`);
  if (alt) fd.append('alt', alt);
  fd.append('inline', 'true');
  const r = await call('POST', '/api/media', fd);
  const a = J(S.B.w_upload_answer(r.status, r.body, alt, thumb));
  if (a.error) {
    finish(id, begin.n, '');
    toast(`Upload failed: ${esc(a.error.text)}`, 4800);
    return;
  }
  if (!finish(id, begin.n, a.md)) {
    // The placeholder went before the upload landed: nothing refers to it.
    const d = J(S.B.w_media_delete(a.id));
    call(d.req.method, d.req.path, d.req.body);
    toast('The image’s place was gone, so it was removed again');
    return;
  }
  toast(`Image added (${Math.round(blob.size / 1024)} KB)`);
}

/** Put `md` where placeholder `n` is (empty: take it out). */
function finish(id, n, md) {
  const ta = editor();
  if (currentId() === id) {
    const at = J(S.B.w_placeholder_at(ta.value, n));
    if (!at) return false;
    const [a, b] = [ta.selectionStart, ta.selectionEnd];
    let to = at.to;
    if (!md && ta.value.slice(to, to + 2) === '\n\n') to += 2;
    ta.setSelectionRange(at.from, to);
    insertText(ta, md);
    const delta = md.length - (to - at.from);
    const fix = (p) => (p >= to ? p + delta : p <= at.from ? p : at.from + md.length);
    ta.setSelectionRange(fix(a), fix(b));
    return true;
  }
  const ok = S.B.w_replace_placeholder(id, n, md, now());
  if (ok) touched();
  return ok;
}

/** An image as a smaller JPEG (or a `data:` URL for the preview). */
async function shrink(file, maxSide, maxBytes, dataUrl = false) {
  const img = await decode(file);
  let scale = Math.min(1, maxSide / Math.max(img.width, img.height));
  for (let i = 0; i < 6; i++) {
    const c = document.createElement('canvas');
    c.width = Math.max(1, Math.round(img.width * scale));
    c.height = Math.max(1, Math.round(img.height * scale));
    c.getContext('2d').drawImage(img, 0, 0, c.width, c.height);
    if (dataUrl) {
      const u = c.toDataURL('image/jpeg', 0.72);
      if (u.length <= maxBytes || i === 5) return u;
    } else {
      const b = await new Promise((res) => c.toBlob(res, 'image/jpeg', 0.85));
      if (b && (b.size <= maxBytes || i === 5)) return b;
    }
    scale *= 0.75;
  }
  throw new Error('too big');
}
function decode(file) {
  if (window.createImageBitmap) return createImageBitmap(file).catch(() => decodeImg(file));
  return decodeImg(file);
}
function decodeImg(file) {
  return new Promise((res, rej) => {
    const u = URL.createObjectURL(file);
    const i = new Image();
    i.onload = () => { URL.revokeObjectURL(u); res(i); };
    i.onerror = () => { URL.revokeObjectURL(u); rej(new Error('undecodable')); };
    i.src = u;
  });
}

// ---------------------------------------------------------------- versions
export async function openHistory() {
  const b = J(S.B.w_versions_begin());
  if (b.error) { toast(esc(b.error.text)); return; }
  let json = '';
  if (b.path) {
    openSheet('<h4>Versions</h4><p class="w-muted">Reading your versions…</p>', { label: 'Versions' });
    const l = await readList(b.path);
    if (l.error) { closeSheet(); toast(esc(l.error.text), 4200); return; }
    json = l.json;
  }
  showHistory(b.id, J(S.B.w_versions_view(b.id, json, now())));
}

function showHistory(id, h) {
  if (h.error) { toast(esc(errText(h))); return; }
  const s = openSheet(`<h4>Versions · ${esc(h.title)}</h4>
    <p class="w-muted">${h.status === 'withdrawn' ? 'Withdrawn.' : `v${h.current} is public${h.dirty ? ', and you have unpublished edits' : ''}.`} Only pinned versions are public; the rest are yours.</p>
    <div class="w-list w-vers">${h.rows.map((r) => `<div class="w-ver" data-v="${r.version}">
      <div class="w-vh"><b>v${r.version}</b>${r.badge ? ` <span class="w-badge${r.badge === '📌' ? ' pin' : ''}">${esc(r.badge === '📌' ? '📌 pinned' : r.badge)}</span>` : ''}<span class="w-flex"></span><small>${esc(r.when)}</small></div>
      <div class="w-note">${esc(r.note)}</div>
      <div class="w-acts">${r.actions.map((a) => `<button type="button" data-act="${a.act}" data-v="${r.version}" ${a.on ? '' : 'disabled'} title="${esc(a.tip)}">${esc(a.label)}</button>`).join('')}</div>
    </div>`).join('')}</div>
    <div class="w-row"><span class="w-flex"></span><button type="button" data-x="close">Close</button></div>`, { label: 'Versions' });
  s.onclick = async (e) => {
    if (e.target.closest('[data-x="close"]')) { closeSheet(); return; }
    const b = e.target.closest('[data-act]'); if (!b || b.disabled) return;
    const v = +b.dataset.v;
    const act = b.dataset.act;
    if (act === 'view' || act === 'diff') showVersion(id, v, act, h);
    else if (act === 'pin') askPin(id, v, h);
    else if (act === 'restore') askRestore(id, v, h, false);
    else if (act === 'discard') askRestore(id, v, h, true);
    else if (act === 'fork') { closeSheet({ refocus: false }); forkFlow(id, true, v); }
  };
}

async function reopenHistory(id) {
  const h = J(S.B.w_versions_view(id, '', now()));
  if (live()) { await openHistory(); return; }
  showHistory(id, h);
}

async function showVersion(id, v, how, h) {
  const b = J(S.B.w_version_begin(id, v));
  if (b.error) { toast(esc(b.error.text)); return; }
  if (b.path) {
    const r = await call('GET', b.path);
    const e = J(S.B.w_version_answer(id, v, r.status, r.body));
    if (e) { toast(esc(e.error.text), 4200); return; }
  }
  const row = h.rows.find((x) => x.version === v);
  const restore = row?.actions.find((a) => (a.act === 'restore' || a.act === 'discard') && a.on);
  const s = openSheet(`<h4>v${v}${how === 'diff' ? ' → the text now' : ''}</h4>
    <div class="w-seg"><button type="button" data-how="view" class="${how === 'view' ? 'on' : ''}">As published</button><button type="button" data-how="diff" class="${how === 'diff' ? 'on' : ''}">Diff vs now</button></div>
    ${how === 'diff'
      ? `<div class="w-diff">${S.B.w_version_diff(id, v)}</div><p class="w-muted"><del>struck</del> is in v${v} and not now; <ins>marked</ins> is new since.</p>`
      : '<iframe class="w-frame" title="The version as published" sandbox="allow-scripts allow-popups"></iframe>'}
    <div class="w-row"><button type="button" data-x="back">‹ Versions</button><span class="w-flex"></span>${restore ? `<button type="button" data-x="restore">${esc(restore.label)}</button>` : ''}</div>`, { label: `Version ${v}` });
  if (how !== 'diff') $('.w-frame', s).srcdoc = S.B.w_version_page(id, v, nonce(), S.theme?.dark ?? false);
  s.onclick = (e) => {
    const m = e.target.closest('[data-how]');
    if (m) { showVersion(id, v, m.dataset.how, h); return; }
    const x = e.target.closest('[data-x]'); if (!x) return;
    if (x.dataset.x === 'back') showHistory(id, J(S.B.w_versions_view(id, '', now())));
    else askRestore(id, v, h, restore.act === 'discard');
  };
}

function askRestore(id, v, h, discard) {
  const s = openSheet(`<h4>${discard ? 'Discard unpublished changes?' : `Restore v${v}?`}</h4>
    <p>${discard ? `The editor goes back to v${v} as published. You can't undo this.` : `The editor gets v${v}'s text back, to publish as v${h.current + 1} when you're ready. What you have now is replaced.`}</p>
    <div class="w-row"><span class="w-flex"></span><button type="button" data-x="no">Cancel</button><button type="button" class="pri" data-x="yes" autofocus>${discard ? 'Discard' : `Restore v${v}`}</button></div>`, { label: 'Restore' });
  s.onclick = async (e) => {
    const x = e.target.closest('[data-x]'); if (!x) return;
    if (x.dataset.x === 'no') { showHistory(id, J(S.B.w_versions_view(id, '', now()))); return; }
    closeSheet({ refocus: false });
    await flushed();
    let o = J(S.B.w_restore(id, v, now()));
    if (o.action === 'remote_restore') o = await S.live.restore(o);
    outcome(o);
    S.render({ loadEditor: true });
    touched();
  };
}

function askPin(id, v, h) {
  const s = openSheet(`<h4>Pin v${v} forever?</h4>
    <p>A pin is a promise: v${v} stays public at its own address forever, even if you edit or withdraw the post later. Others can quote it and fork it. You can't unpin.</p>
    <label class="w-lbl" for="w-pinw">Type <b>pin</b> to confirm</label>
    <input id="w-pinw" class="w-in" autocomplete="off" autocapitalize="off" spellcheck="false" placeholder="pin" autofocus>
    <p class="w-err" id="w-pinerr" hidden></p>
    <div class="w-row"><span class="w-flex"></span><button type="button" data-x="no">Cancel</button><button type="button" class="pri" data-x="yes">Pin v${v}</button></div>`, { label: 'Pin' });
  const go = async () => {
    await flushed();
    const r = J(S.B.w_pin(id, v, $('#w-pinw').value));
    if (r.error) { const e = $('#w-pinerr'); e.textContent = r.error.text; e.hidden = false; return; }
    if (r.done) { toast(esc(r.done)); reopenHistory(id); return; }
    $('#w-pinerr').hidden = true;
    $$('button', s).forEach((b) => { b.disabled = true; });
    if (await command(id, r.req, () => `Pinned v${v}: public forever`)) reopenHistory(id);
    else closeSheet();
  };
  $('#w-pinw').addEventListener('keydown', (e) => { if (e.key === 'Enter') { e.preventDefault(); go(); } });
  s.onclick = (e) => {
    const x = e.target.closest('[data-x]'); if (!x) return;
    if (x.dataset.x === 'no') showHistory(id, J(S.B.w_versions_view(id, '', now())));
    else go();
  };
}

// ---------------------------------------------------------------- copies elsewhere
// --- syndication --- (src/syndication.rs) where a published post is also
// posted: GET/PUT /api/items/:id/syndication. Each change is one PUT with
// the item's revision, an in-place rewrite: never a new version. Removing
// one asks first.
export async function openCopies() {
  const b = J(S.B.w_copies_begin());
  if (b.error) { toast(esc(b.error.text)); return; }
  openSheet('<h4>Copies elsewhere</h4><p class="w-muted">Reading the list…</p>', { label: 'Copies elsewhere' });
  const g = await call('GET', b.path);
  const v = J(S.B.w_copies_view(g.status, g.body));
  if (v.error) { closeSheet(); toast(esc(v.error.text), 4200); return; }
  showCopies(b, v, '');
}

function showCopies(b, v, error) {
  const { entries, served } = v;
  const s = openSheet(`<h4>Copies elsewhere · ${esc(b.title)}</h4>
    <p class="w-muted">Where you've posted this elsewhere. It's listed on the post as your claim; changing it never publishes a new version.</p>
    ${served ? '' : '<p class="w-muted">Your blyg doesn\'t keep this list yet (it needs a newer studio).</p>'}
    <div class="w-list">${entries.length ? entries.map((e, i) => `<div class="w-row w-copy"><b>${esc(e.host)}</b><span class="w-flex w-muted" title="${esc(e.url)}">${esc(e.url)}</span><small>v${esc(String(e.version))}</small><button type="button" data-rm="${i}">Remove</button></div>`).join('') : '<p class="w-muted">None listed yet.</p>'}</div>
    <label class="w-lbl" for="w-curl">Add a copy: its https: address, and the version it was made from (blank: v${esc(String(b.current))})</label>
    <div class="w-row"><input id="w-curl" class="w-in w-flex" autocomplete="off" inputmode="url" placeholder="https://…"><input id="w-cver" class="w-in" style="width:4.5rem" autocomplete="off" placeholder="v${esc(String(b.current))}"></div>
    ${error ? `<p class="w-err">${esc(error)}</p>` : ''}
    <div class="w-row"><span class="w-flex"></span><button type="button" data-x="close">Close</button><button type="button" data-x="add" ${served ? '' : 'disabled'}>Add</button></div>`, { label: 'Copies elsewhere' });
  const put = async (remove, url, ver) => {
    const r = J(S.B.w_copies_put(b.path, JSON.stringify(entries), remove, url, ver, b.current, v.revision));
    if (r.error) { showCopies(b, v, r.error.text); return; }
    const a = await call(r.req.method, r.req.path, r.req.body);
    const ans = J(S.B.w_copies_answer(b.id, v.revision, a.status, a.body));
    if (ans.error) { showCopies(b, v, ans.error.text); return; }
    await persist({ quiet: true });
    showCopies(b, { entries: ans.entries, revision: ans.revision, served: true }, '');
  };
  const add = () => { if (served) put('', $('#w-curl').value, $('#w-cver').value); };
  $('#w-curl').addEventListener('keydown', (e) => { if (e.key === 'Enter') { e.preventDefault(); add(); } });
  $('#w-cver').addEventListener('keydown', (e) => { if (e.key === 'Enter') { e.preventDefault(); add(); } });
  s.onclick = (e) => {
    const rm = e.target.closest('[data-rm]');
    if (rm) {
      const it = entries[+rm.dataset.rm];
      if (it && window.confirm(`Take ${it.host} off this post's copies elsewhere? The copy itself stays where it is.`)) put(it.url, '', '');
      return;
    }
    const x = e.target.closest('[data-x]'); if (!x) return;
    if (x.dataset.x === 'add') add(); else closeSheet();
  };
}
W.openCopies = openCopies;

// ---------------------------------------------------------------- withdraw, delete, discard
export function askWithdraw() {
  const o = J(S.B.w_withdraw_check());
  if (o.action !== 'confirm') { outcome(o); return; }
  const id = currentId();
  const s = openSheet(`<h4>${esc(o.title)}</h4><p>${esc(o.body)}</p>
    <label class="w-lbl" for="w-wnote">A note (optional), e.g. “no longer accurate”</label>
    <input id="w-wnote" class="w-in" autocomplete="off">
    <div class="w-row"><span class="w-flex"></span><button type="button" data-x="no" autofocus>Keep it</button><button type="button" class="danger" data-x="yes">Withdraw</button></div>`, { label: 'Withdraw' });
  const go = async () => {
    const note = $('#w-wnote').value;
    closeSheet({ refocus: false });
    await flushed();
    const r = J(S.B.w_withdraw(id, note, now()));
    if (r.error) { toast(esc(r.error.text), 4200); return; }
    if (r.done) { toast(esc(r.done), 4200); S.render({ loadEditor: true }); touched(); return; }
    await command(id, r.req, () => S.B.w_withdrawn_toast(id, note));
  };
  $('#w-wnote').addEventListener('keydown', (e) => { if (e.key === 'Enter') { e.preventDefault(); go(); } });
  s.onclick = (e) => { const x = e.target.closest('[data-x]'); if (!x) return; if (x.dataset.x === 'yes') go(); else closeSheet(); };
}
W.askWithdraw = askWithdraw;

function askDelete() {
  const o = J(S.B.delete_check());
  if (o.action === 'ask_withdraw') { askWithdraw(); return; }
  if (o.action !== 'confirm') { outcome(o); return; }
  const s = openSheet(`<h4>${esc(o.title)}</h4><p>${esc(o.body)}</p>
    <div class="w-row"><span class="w-flex"></span><button type="button" data-x="no" autofocus>Keep it</button><button type="button" class="danger" data-x="yes">Delete</button></div>`, { label: 'Delete' });
  s.onclick = (e) => {
    const x = e.target.closest('[data-x]'); if (!x) return;
    closeSheet({ refocus: false });
    if (x.dataset.x !== 'yes') return;
    outcome(S.B.delete());
    persist();
    hook('afterDelete');
    S.render({ loadEditor: true });
  };
}

function askDiscard() {
  const o = J(S.B.discard_check());
  if (o.action !== 'confirm') { outcome(o); return; }
  const s = openSheet(`<h4>${esc(o.title)}</h4><p>${esc(o.body)}</p>
    <div class="w-row"><span class="w-flex"></span><button type="button" data-x="no" autofocus>Keep the edits</button><button type="button" class="danger" data-x="yes">Discard</button></div>`, { label: 'Discard changes' });
  s.onclick = async (e) => {
    const x = e.target.closest('[data-x]'); if (!x) return;
    closeSheet({ refocus: false });
    if (x.dataset.x !== 'yes') return;
    let r = J(S.B.discard(now()));
    if (r.action === 'remote_restore') { await flushed(); r = await S.live.restore(r); }
    outcome(r);
    S.render({ loadEditor: true });
    touched();
  };
}

// ---------------------------------------------------------------- fork
/** Fork a pinned version: of a post you read (`key`), or your own (`own`). */
export async function askFork(key, own = false, only = null) { return forkFlow(key, own, only); }
async function forkFlow(key, own, only) {
  if (!key) { toast('Open a post to fork it'); return; }
  const p = J(S.B.w_fork_options_path(key, own));
  if (p.error) { toast(esc(p.error.text), 4800); return; }
  openSheet('<h4>Fork a pinned version</h4><p class="w-muted">Asking for its pins…</p>', { label: 'Fork' });
  const r = await call('GET', p.path);
  const rid = new URLSearchParams(p.path.split('?')[1]).get('id');
  const o = J(S.B.w_fork_options(r.status, r.body, rid, now()));
  if (o.error) { closeSheet({ refocus: false }); toast(esc(o.error.text), 5200); return; }
  const versions = only ? o.versions.filter((x) => x.version === only) : o.versions;
  const s = openSheet(`<h4>Fork a pinned version</h4>
    <p class="w-muted">A fork is a new draft of yours that starts as that pin's text, recorded as forked from it. It isn't a quote or a reply: it says where your text came from.</p>
    <div class="w-list">${versions.map((x) => `<button type="button" class="w-opt" data-v="${x.version}">📌 v${x.version} · ${esc(x.when)}<small>${esc(x.note)}</small></button>`).join('')}</div>
    <div class="w-row"><span class="w-flex"></span><button type="button" data-x="no">Cancel</button></div>`, { label: 'Fork' });
  s.onclick = async (e) => {
    if (e.target.closest('[data-x]')) { closeSheet(); return; }
    const b = e.target.closest('[data-v]'); if (!b) return;
    closeSheet({ refocus: false });
    const q = J(S.B.w_fork_request(o.origin, o.id, +b.dataset.v));
    const a = await call(q.req.method, q.req.path, q.req.body);
    const res = J(S.B.w_fork_answer(a.status, a.body));
    if (res.error) { toast(`Couldn’t fork: ${esc(res.error.text)}`, 4800); return; }
    await persist({ quiet: true });
    hook('openEditor');
    toast(`A new draft, forked from 📌 v${b.dataset.v}`);
  };
}

// ---------------------------------------------------------------- TK
export function openTk() {
  const t = J(S.B.w_tk_view());
  const s = openSheet(`<h4>✦ TK: text the blyg writes</h4>
    <p class="w-muted">Write <code>[TK]what to write here[/TK]</code> where you want generated text; the blyg's model fills it, and it's disclosed as generated when you publish.${t.live ? '' : ' The demo has no model: try it on your blyg.'}</p>
    ${t.errors.length ? `<p class="w-err">${t.errors.map(esc).join('<br>')}</p>` : ''}
    <div class="w-list">${t.scopes.length ? t.scopes.map((x) => `<div class="w-tk"><span>${esc(x.instruction)}${x.sources ? `<small>draws on ${x.sources} post${x.sources === 1 ? '' : 's'}</small>` : ''}</span>
      ${x.imported ? '<small>generated elsewhere</small>' : `<button type="button" data-gen="${x.index}" class="${x.done ? '' : 'pri'}">${x.done ? 'Regenerate' : 'Generate'}</button>`}</div>`).join('')
    : '<p class="w-muted">No TK scopes in this post yet.</p>'}</div>
    <div class="w-row"><button type="button" data-x="insert">Insert a TK here</button><span class="w-flex"></span><button type="button" data-x="close">Close</button></div>`, { label: 'TK' });
  s.onclick = async (e) => {
    const x = e.target.closest('[data-x]');
    if (x?.dataset.x === 'close') { closeSheet(); return; }
    if (x?.dataset.x === 'insert') { closeSheet(); setTimeout(() => MD.tk(editor()), 80); return; }
    const g = e.target.closest('[data-gen]'); if (!g) return;
    $$('[data-gen]', s).forEach((b) => { b.disabled = true; });
    g.textContent = 'Generating…';
    await flushed();
    const r = J(S.B.w_generate_request(+g.dataset.gen));
    if (r.error) { toast(esc(r.error.text), 4800); openTk(); return; }
    const ok = await command(r.id, r.req, (body) => {
      const m = S.B.constructor.w_generated_with(body);
      return m ? `Generated with ${m}. It's marked as generated when you publish.` : 'Generated';
    });
    if (W.isOpen()) openTk();
    if (!ok) return;
  };
}

// ---------------------------------------------------------------- the publish sheet
let note = null; // { id, el, offer, busy }
/** Mount the drafted note and change kind in a publish sheet: `host` holds
    them, `el` is the note's field. */
export function publishSheet(host, el) {
  const id = currentId();
  let box = host.querySelector('.w-pub');
  const offer = id ? J(S.B.w_note_offer(id)) : { draft: false, classify: false };
  S.B.w_note_begin(el.value, !!offer.in_note);
  note = { id, el, offer, busy: 0, model: null, prob: null, category: null, reason: '' };
  if (!offer.draft && !offer.classify) { box?.remove(); return; }
  if (!box) {
    box = document.createElement('div');
    box.className = 'w-pub';
    el.before(box);
  }
  box.onclick = (e) => {
    const c = e.target.closest('[data-cat]');
    if (c) { const t = J(S.B.w_note_choose(c.dataset.cat, el.value)); el.value = t.text; note.category = t.category; drawNote(); return; }
    if (e.target.closest('[data-x="draft"]')) draftNote();
  };
  if (!el._wNote) { el._wNote = true; el.addEventListener('input', drawNote); }
  drawNote();
  if (offer.classify) classify();
  if (offer.auto && !el.value.trim()) draftNote();
}
function drawNote() {
  if (!note) return;
  const box = note.el.parentElement?.querySelector('.w-pub');
  if (!box) return;
  const o = note.offer;
  const gen = S.B.w_note_generated(note.el.value);
  box.innerHTML = `${o.classify ? `<div class="w-cats" role="radiogroup" aria-label="Kind of change"><span>Kind</span>${note.busy & 2 ? '<small>classifying…</small>' : o.categories.map((c) => `<button type="button" role="radio" aria-checked="${note.category === c}" class="w-chip${note.category === c ? ' on' : ''}" data-cat="${c}">${c}${note.model === c && note.prob != null ? ` · ${Math.round(note.prob * 100)}%` : ''}</button>`).join('')}</div>
    ${!(note.busy & 2) && !note.model ? `<small class="w-muted">No kind suggested${note.reason ? `: ${esc(note.reason)}` : ''}. Choose one, or publish without.</small>` : ''}` : ''}
    ${o.draft ? `<div class="w-noterow"><button type="button" data-x="draft" ${note.busy & 1 ? 'disabled' : ''}>${note.busy & 1 ? 'Drafting a note…' : '✦ Draft a note'}</button>${gen ? '<small>Drafted by the model: it’s marked as generated unless you edit it.</small>' : ''}</div>` : ''}`;
}
async function draftNote() {
  if (!note || note.busy & 1) return;
  const n = note;
  n.busy |= 1; drawNote();
  await flushed();
  const r = J(S.B.w_note_request(n.id, 'note-draft'));
  let t;
  if (r.error) t = r;
  else { const a = await call(r.req.method, r.req.path, r.req.body); t = J(S.B.w_note_drafted(a.status, a.body)); }
  if (note !== n) return;
  n.busy &= ~1;
  if (t.error) toast(`No note could be drafted: ${esc(t.error.text)}. Write one, or publish without.`, 4800);
  else n.el.value = t.text;
  drawNote();
}
async function classify() {
  if (!note) return;
  const n = note;
  n.busy |= 2; drawNote();
  await flushed();
  const r = J(S.B.w_note_request(n.id, 'classify'));
  let c;
  if (r.error) c = { category: null, probability: null, reason: r.error.text, text: n.el.value };
  else { const a = await call(r.req.method, r.req.path, r.req.body); c = J(S.B.w_note_classified(a.status, a.body, n.el.value)); }
  if (note !== n) return;
  n.busy &= ~2;
  n.model = c.category; n.category = c.category; n.prob = c.probability; n.reason = c.reason || '';
  n.el.value = c.text;
  drawNote();
}
/** The publish outcome, with the note's provenance and kind for the blyg. */
export function withNote(o, text) {
  const f = J(S.B.w_note_finish(text));
  note = null;
  if (o?.action === 'remote_publish' && (f.generated || f.classified)) o.ex = f;
  return o;
}
/** live.js asks: the publish request with what the sheet decided. */
W.publishRequest = (o) => S.B.w_publish_request(o.id, o.note ?? '', !!o.ex.generated, !!o.ex.classified, o.ex.category ?? undefined);
W.publishWarning = (r) => { const w = S.B.constructor.w_publish_warning(r.status, r.body); if (w) setTimeout(() => toast(esc(w), 6000), 5400); };

// ---------------------------------------------------------------- settings
export async function openSettings() {
  if (live() && !J(S.B.w_settings_form()).known) {
    if (S.B.is_offline()) { toast('Your blyg’s settings are on the blyg: this needs a connection'); return; }
    const r = await call('GET', '/api/settings');
    if (r.status === 200) S.B.w_set_settings(r.body);
    else { toast(esc(J(S.B.constructor.api_error(r.status, r.body))?.text || 'Couldn’t read the settings'), 4200); return; }
  }
  const f = J(S.B.w_settings_form()).form;
  const tz = (() => { try { return Intl.supportedValuesOf('timeZone'); } catch { return []; } })();
  const text = (k, label, hint = '', type = 'text') => `<label class="w-lbl" for="ws-${k}">${label}${hint ? `<small>${hint}</small>` : ''}</label>
    <input id="ws-${k}" class="w-in" type="${type}" value="${esc(f[k])}" autocomplete="off" ${k === 'timezone' ? 'list="ws-tzs"' : ''}>`;
  const s = openSheet(`<h4>⚙ Your blyg</h4>
    <p class="w-muted">${live() ? 'What your blyg shows about itself. Saved to the blyg when you press Save.' : 'The demo’s pretend blyg: kept in this page only.'}</p>
    <form class="w-form" id="ws-form">
    ${text('siteTitle', 'Site title')}
    ${text('authorName', 'Author name')}
    <label class="w-lbl" for="ws-authorBio">Bio</label><textarea id="ws-authorBio" class="w-ta short">${esc(f.authorBio)}</textarea>
    <label class="w-lbl" for="ws-links">Links<small>one per line: Label | https://…</small></label><textarea id="ws-links" class="w-ta short" spellcheck="false" placeholder="Elsewhere | https://example.org/">${esc(f.links)}</textarea>
    ${text('authorUrl', 'Your address', 'authorUrl: your own page, for your h-card and mentions', 'url')}
    ${text('siteUrl', 'Site URL', 'siteUrl: where your blyg is published, if not here', 'url')}
    ${text('timezone', 'Time zone', 'dates on your pages; empty is UTC')}
    <datalist id="ws-tzs">${tz.map((z) => `<option value="${esc(z)}">`).join('')}</datalist>
    <fieldset class="w-fs"><legend>Writing defaults</legend>
      <label class="w-lbl" for="ws-picker">Typing [[ or ![[ opens<small>pickerTyping</small></label>
      <select id="ws-picker" class="w-in">${[['auto', 'the picker, where it fits (auto)'], ['editor', 'the picker in the editor'], ['panel', 'the picker in a panel']].map(([v, l]) => `<option value="${v}" ${f.pickerTyping === v ? 'selected' : ''}>${l}</option>`).join('')}</select>
      <label class="w-check"><input type="checkbox" id="ws-showResponsesDefault" ${f.showResponsesDefault ? 'checked' : ''}> Show responses under new posts</label>
      <label class="w-check"><input type="checkbox" id="ws-highlightGeneratedDefault" ${f.highlightGeneratedDefault ? 'checked' : ''}> Highlight generated text on new posts</label>
    </fieldset>
    </form>
    <p class="w-err" id="ws-err" hidden></p>
    <div class="w-row"><span class="w-flex"></span><button type="button" data-x="no">Cancel</button><button type="button" class="pri" data-x="save">Save</button></div>`, { label: 'Settings' });
  const save = async () => {
    const form = {
      siteTitle: $('#ws-siteTitle').value, authorName: $('#ws-authorName').value, authorBio: $('#ws-authorBio').value,
      authorUrl: $('#ws-authorUrl').value, siteUrl: $('#ws-siteUrl').value, timezone: $('#ws-timezone').value,
      links: $('#ws-links').value, pickerTyping: $('#ws-picker').value,
      showResponsesDefault: $('#ws-showResponsesDefault').checked, highlightGeneratedDefault: $('#ws-highlightGeneratedDefault').checked,
    };
    const r = J(S.B.w_settings_save(JSON.stringify(form)));
    const err = $('#ws-err');
    if (r.error) { err.textContent = r.error.text; err.hidden = false; return; }
    if (r.nothing) { closeSheet(); toast(esc(r.nothing)); return; }
    if (r.saved) { closeSheet(); toast(esc(r.saved)); return; }
    $$('button', s).forEach((b) => { b.disabled = true; });
    const a = await call(r.req.method, r.req.path, r.req.body);
    const res = J(S.B.w_settings_answer(a.status, a.body, location.origin));
    if (res.error) { err.textContent = res.error.text; err.hidden = false; $$('button', s).forEach((b) => { b.disabled = false; }); return; }
    closeSheet();
    await persist({ quiet: true });
    S.render();
    toast(esc(res.saved));
  };
  $('#ws-form').addEventListener('submit', (e) => { e.preventDefault(); save(); });
  s.onclick = (e) => { const x = e.target.closest('[data-x]'); if (!x) return; if (x.dataset.x === 'save') save(); else closeSheet(); };
}

// ---------------------------------------------------------------- contacts and the @ list
/** Live, once the blyg answered: read the address book in the background
    (the preview's h-cards and the @ list use it). */
W.liveStarted = async () => {
  if (S.wSettings) { S.B.w_set_settings(S.wSettings); S.wSettings = null; }
  if (S.B.w_contacts_read() || S.B.is_offline()) return;
  const r = await call('GET', '/api/contacts');
  if (S.B.w_set_contacts(r.status, r.body) > 0) S.render();
};

let atBox = null, atRows = [], atSel = 0, atTa = null;
function atEls() {
  if (!atBox) {
    document.body.insertAdjacentHTML('beforeend', '<div class="w-at" id="w-at" role="listbox" aria-label="Mention" hidden></div>');
    atBox = $('#w-at');
    atBox.addEventListener('mousedown', (e) => e.preventDefault());
    atBox.addEventListener('click', (e) => { const b = e.target.closest('[data-i]'); if (b) pickAt(+b.dataset.i); });
  }
  return atBox;
}
function hideAt() {
  if (atBox) atBox.hidden = true;
  atRows = [];
}
function placeAt(ta) {
  const box = atEls();
  const r = ta.getBoundingClientRect();
  const vv = window.visualViewport;
  if (isPhone()) {
    const bar = $('#p-bar')?.getBoundingClientRect();
    const bottom = bar ? bar.top : (vv ? vv.offsetTop + vv.height : innerHeight);
    box.style.left = '8px'; box.style.right = '8px'; box.style.width = '';
    box.style.top = ''; box.style.bottom = `${Math.max(0, innerHeight - bottom + 6)}px`;
  } else {
    box.style.left = `${r.left + 12}px`; box.style.right = ''; box.style.width = `${Math.min(420, r.width - 24)}px`;
    box.style.bottom = ''; box.style.top = `${Math.min(r.bottom - 8, innerHeight - 260)}px`;
  }
}
function drawAt() {
  const box = atEls();
  box.innerHTML = atRows.length
    ? atRows.map((r) => `<button type="button" role="option" class="${r.i === atSel ? 'on' : ''}" aria-selected="${r.i === atSel}" data-i="${r.i}">${r.contact ? '@ ' : ''}${esc(r.label)}<small>${esc(r.sub)}</small></button>`).join('')
    : '<p class="w-muted">No one matches. Keep typing, or press esc.</p>';
}
function pickAt(i) {
  const ta = atTa; if (!ta) return;
  const p = J(S.B.w_mention_pick(ta.value, ta.selectionEnd, i));
  hideAt();
  if (!p) return;
  ta.focus({ preventScroll: true });
  ta.setSelectionRange(p.from, p.to);
  insertText(ta, p.insert + ' ');
}
/** Attach the writing tools to an editor textarea: the @ list, paste and
    drop of images. */
export function attachEditor(ta) {
  ta.addEventListener('input', () => {
    const old = ta._wPrev ?? ta.value;
    const cur = ta.value;
    ta._wPrev = cur;
    const c = ta.selectionEnd;
    // Only when an @ was just typed, or the list is open: typing costs
    // nothing otherwise.
    const typedAt = cur.length === old.length + 1 && cur[c - 1] === '@';
    if (!typedAt && !atRows.length && (!atBox || atBox.hidden)) return;
    const res = J(S.B.w_mention(old, cur, c, now()));
    if (!res) { hideAt(); return; }
    atTa = ta;
    atRows = res.rows;
    atSel = 0;
    placeAt(ta);
    drawAt();
    atEls().hidden = false;
  });
  ta.addEventListener('focus', () => { ta._wPrev = ta.value; });
  ta.addEventListener('blur', () => setTimeout(() => { if (document.activeElement !== ta) { hideAt(); S.B.w_mention_close(); } }, 150));
  ta.addEventListener('keydown', (e) => {
    if (!atBox || atBox.hidden) return;
    if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); hideAt(); S.B.w_mention_close(); return; }
    if (!atRows.length) return;
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault(); e.stopPropagation();
      atSel = (atSel + (e.key === 'ArrowDown' ? 1 : atRows.length - 1)) % atRows.length;
      drawAt();
    } else if ((e.key === 'Enter' || e.key === 'Tab') && !e.metaKey && !e.ctrlKey && !e.altKey) {
      e.preventDefault(); e.stopPropagation();
      pickAt(atSel);
    }
  }, true);
  ta.addEventListener('paste', (e) => {
    const f = imageFrom(e.clipboardData);
    if (!f) return;
    e.preventDefault();
    back = ta;
    askAlt(f);
  });
  ta.addEventListener('dragover', (e) => { if ([...(e.dataTransfer?.items || [])].some((i) => i.kind === 'file')) e.preventDefault(); });
  ta.addEventListener('drop', (e) => {
    const f = imageFrom(e.dataTransfer);
    if (!f) return;
    e.preventDefault();
    ta.focus();
    back = ta;
    askAlt(f);
  });
}

export { W };
// --- end web-writing ---
