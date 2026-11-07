// Starts Burrow in a browser. Every rule runs in Burrow's Rust, compiled to
// WebAssembly (pkg/blyg_web.js). Wider than 700px it's the desktop window
// (desk.js); narrower, the phone app (phone.js). Both draw the same
// `Burrow`, so turning a tablet or resizing a window keeps your place.
//
// Served from a blyg (at /burrow/), it runs LIVE on your own posts through
// the same-origin owner API and your Studio session (live.js). Anywhere
// else (a local demo server), /api/capabilities isn't a blyg's, and it
// runs the DEMO on invented sample posts, labelled as such.

import init, { Burrow } from './pkg/blyg_web.js';
import { $, S, now, load, pref, applyTheme } from './core.js';
import * as live from './live.js';
import { initDesk, render as renderDesk, showDesk, deskEditorFocused } from './desk.js';
import { initPhone, render as renderPhone, showPhone } from './phone.js';

const narrow = matchMedia('(max-width: 700px)');
function chooseLayout() {
  S.layout = narrow.matches ? 'phone' : 'desk';
  S.render = S.layout === 'phone' ? renderPhone : renderDesk;
  document.documentElement.dataset.layout = S.layout;
}

// Offline: a service worker keeps the shell and the wasm, so the app opens
// with no network once it has been opened once. Not on file:// or in an
// opaque origin, where workers can't run. Its scope is this directory
// (/burrow/), and it lets every request outside it, /api/ and /studio/
// above all, go straight to the network (sw.js `mine`).
function registerWorker() {
  if (!('serviceWorker' in navigator) || !/^https?:$/.test(location.protocol)) return;
  navigator.serviceWorker.register('sw.js', { scope: './' }).catch((err) => console.warn('No offline copy:', err?.message || err));
}

/** Which Burrow: live on this blyg, or the demo. */
async function openStore() {
  const m = await live.detect();
  const cached = m.mode === 'offline' || m.mode === 'signin' ? await load('live') : undefined;
  if (m.mode === 'live' || cached) {
    S.live = live;
    document.documentElement.dataset.mode = 'live';
    await live.start(m.mode === 'live');
    return m;
  }
  document.documentElement.dataset.mode = 'demo';
  S.B = new Burrow(now(), await load('store'));
  return m;
}

(async () => {
  try {
    await init();
    S.themes = JSON.parse(Burrow.themes());
    const dark = matchMedia('(prefers-color-scheme: dark)').matches;
    applyTheme(pref.get('theme') || (dark ? 'dark' : 'light'), [document.documentElement]);
    const m = await openStore();
    chooseLayout();
    initDesk();
    initPhone();
    (S.layout === 'phone' ? showPhone : showDesk)();
    narrow.addEventListener('change', () => {
      chooseLayout();
      (S.layout === 'phone' ? showPhone : showDesk)();
    });
    // Relative times ("2m") keep moving.
    setInterval(() => {
      const typing = S.layout === 'phone' ? document.activeElement?.id === 'p-ta' : deskEditorFocused();
      if (!typing) S.render();
    }, 30_000);
    live.initSignIn(() => location.reload());
    if (m.mode === 'signin') live.signIn();
    else if (m.mode === 'unsupported') live.signIn(m.why);
    if (m.mode === 'live') live.refresh();
    registerWorker();
  } catch (err) {
    const msg = `Burrow couldn't start: ${err?.message || err}. It needs a browser with WebAssembly, served over http (not file://).`;
    for (const l of [$('#loading'), $('#p-loading')]) { l.classList.add('err'); l.textContent = msg; }
    console.error(err);
  }
})();
