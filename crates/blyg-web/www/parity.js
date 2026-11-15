// --- web-parity --- What the studio does that the web lacked, as screens
// in the reading panel (web-reading.js `wrExtend`):
//
// - Mark all read (✓ in the sources, ⌃⌥A on a desk): every unread post of
//   the chosen source and lens, after a confirmation, sent in batches of
//   500 to POST /api/reading/read; a batch that fails is put back.
//
// Every rule is Burrow's Rust (src/parity/); this draws and fetches. Every
// request goes through live.js `call` (same-origin /api/ only). Features
// are gated on the operation names /api/capabilities advertises.

import { S, esc, now, persist, persistReading, toast } from './core.js';
import { call } from './live.js';
import { wrExtend, ensure, closePanel } from './web-reading.js';

const loadingP = '<p class="wr-muted">Reading it from the blyg…</p>';
const plural = (n, one, many = `${one}s`) => `${n} ${n === 1 ? one : many}`;

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

wrExtend({
  titles: { markall: 'Mark all read' },
  screens: { markall: markAllScreen },
  click(t, d) {
    if ('markallGo' in d) { markAll(); return true; }
    return false;
  },
});
