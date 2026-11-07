// The Blyg hub: one plain, labelled list of what isn't about the post in
// front of you, in both layouts. On a phone it's the fourth tab (Blyg); on
// a desk, the Blyg view beside Posts and Reading (⌃3). Each row opens the
// screen it names: the blyg's settings (writing.js), mentions, hoppers,
// subscriptions, peek and history (web-reading.js), the shelf (the
// layout's own). The theme is chosen right here; the demo's offline switch
// and reset sit at the end.
//
// It's drawn only while it's on screen, through `draw()` (a redraw that
// changes nothing costs a string compare), and never on a keystroke.

import { $, S, esc, draw, applyTheme } from './core.js';
import { go } from './web-reading.js';
import * as Wr from './writing.js';

const use = {}; // per layout: { shelf(), offline(), reset() }
/** A layout hands over what is its own (the shelf sheet, the demo's switches). */
export function hubUse(layout, fns) { use[layout] = fns; }

const row = (act, icon, label, sub, key = '') =>
  `<button type="button" class="hub-row" data-hub="${act}"><span class="hub-ico" aria-hidden="true">${icon}</span>`
  + `<span class="hub-l"><b>${label}</b><small>${sub}</small></span>${key ? `<kbd class="hub-key">${key}</kbd>` : ''}<span class="hub-go" aria-hidden="true">›</span></button>`;

function html(V) {
  const live = V.mode === 'live';
  const off = S.B.is_offline();
  const desk = S.layout === 'desk';
  const themes = S.themes.map((t) => {
    const on = t.id === S.theme?.id;
    return `<button type="button" class="hub-theme${on ? ' on' : ''}" data-theme="${esc(t.id)}" aria-pressed="${on}">${on ? '✓ ' : ''}${esc(t.name)}<small>${esc(t.family)} · ${t.dark ? 'dark' : 'light'}</small></button>`;
  }).join('');
  return `<div class="hub">
    <p class="hub-mode"><span class="modebadge ${V.mode}">${live ? 'Live' : 'Demo'}</span> ${esc(V.mode_label)} · <span class="hub-sync">${esc(V.sync)}</span></p>
    <section aria-labelledby="hub-h1"><h3 class="hub-h" id="hub-h1">Your blyg</h3>
      ${row('settings', '⚙', 'Settings', 'Title, author, bio, links, time zone, writing defaults')}
      ${row('mentions', '@', 'Mentions', 'Responses to your posts, what you sent, accepting Webmentions')}
      ${row('hoppers', '⊕', 'Hoppers', 'Make, rename, describe, make public, delete')}
    </section>
    <section aria-labelledby="hub-h2"><h3 class="hub-h" id="hub-h2">Reading</h3>
      ${row('subscribe', '＋', 'Subscribe', 'Follow a blyg or a feed, after a preview')}
      ${row('subs', '☷', 'Subscriptions', 'Pause, rename, blogroll, check now, OPML import and export')}
      ${row('peek-url', '⌕', 'Peek at a post', 'Read a post from a blyg you don’t follow')}
      ${row('history', '⟲', 'History', 'The posts you peeked at')}
    </section>
    <section aria-labelledby="hub-h3"><h3 class="hub-h" id="hub-h3">Writing</h3>
      ${row('shelf', '📚', 'Your shelf', 'Posts you keep at hand, in your order', desk ? '⌘B' : '')}
    </section>
    <section aria-labelledby="hub-h4"><h3 class="hub-h" id="hub-h4">Theme</h3>
      <div class="hub-themes" role="group" aria-label="Theme">${themes}</div>
    </section>
    ${live ? '' : `<section aria-labelledby="hub-h5"><h3 class="hub-h" id="hub-h5">The demo</h3>
      <div class="hub-acts"><button type="button" class="hub-btn" data-hub="offline" aria-pressed="${off}">${off ? '⇅ Go online' : '⇅ Go offline'}</button>
      <button type="button" class="hub-btn" data-hub="reset">↺ Reset sample data</button></div>
      <p class="hub-note">Invented sample posts, kept in this browser. Nothing is sent anywhere.</p>
    </section>`}
  </div>`;
}

/** Draw the hub into `host` from the layout's view `V` (only while it's
    on screen). */
export function drawHub(host, V) {
  if (!host || !S.B || !V) return;
  draw(host, html(V));
  if (!host._hub) {
    host._hub = true;
    host.addEventListener('click', (e) => {
      const t = e.target.closest('[data-theme]');
      if (t) { applyTheme(t.dataset.theme, [document.documentElement]); for (const f of ['#pv', '#p-pv']) { const el = $(f); if (el) el._shown = null; } S.render(); return; }
      const b = e.target.closest('[data-hub]');
      if (!b) return;
      const act = b.dataset.hub;
      const mine = use[S.layout] || {};
      if (act === 'settings') Wr.openSettings();
      else if (act === 'shelf') mine.shelf?.();
      else if (act === 'offline') mine.offline?.();
      else if (act === 'reset') mine.reset?.();
      else go(act);
    });
  }
}
