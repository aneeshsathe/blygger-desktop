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
import { $, S, now, load, pref, applyTheme, placeOnStart } from './core.js';
import * as live from './live.js';
import { dropEarly, WASM } from './net.js';
import { initDesk, render as renderDesk, showDesk, deskEditorFocused } from './desk.js';
import { initPhone, render as renderPhone, showPhone } from './phone.js';
// --- web-reading ---
import { initWebReading } from './web-reading.js';
// --- end web-reading ---
import './parity.js'; // --- web-parity --- studio-parity screens

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

/** Live, the app is the page: the theme and layout controls move into the
 *  title bar, with a Keys button for the keyboard list (style.css). */
function liveChrome() {
  const tb = document.querySelector('#win .tb'), controls = document.querySelector('#desk .controls');
  const keys = document.querySelector('#desk .keys');
  if (!tb || !controls || tb.contains(controls)) return;
  const b = document.createElement('button');
  b.type = 'button'; b.className = 'ghost'; b.textContent = 'Keys';
  b.setAttribute('aria-pressed', 'false');
  b.onclick = () => b.setAttribute('aria-pressed', String(keys?.classList.toggle('on') ?? false));
  controls.append(b);
  tb.append(controls);
}

/** Which Burrow: live on this blyg, or the demo. */
async function openStore() {
  const m = await live.detect();
  const cached = m.mode === 'offline' || m.mode === 'signin' ? await load('live') : undefined;
  if (m.mode === 'live' || cached) {
    S.live = live;
    document.documentElement.dataset.mode = 'live';
    liveChrome();
    await live.start(m.mode === 'live');
    return m;
  }
  document.documentElement.dataset.mode = 'demo';
  S.B = new Burrow(now(), await load('store'));
  return m;
}

(async () => {
  try {
    // What this browser holds is read while the WebAssembly compiles.
    if (/^https?:$/.test(location.protocol)) live.loadSaved();
    await init({ module_or_path: WASM });
    S.themes = JSON.parse(Burrow.themes());
    const dark = matchMedia('(prefers-color-scheme: dark)').matches;
    applyTheme(pref.get('theme') || (dark ? 'dark' : 'light'), [document.documentElement]);
    const m = await openStore();
    chooseLayout();
    initDesk();
    initPhone();
    initWebReading(); // --- web-reading --- --- end web-reading ---
    (S.layout === 'phone' ? showPhone : showDesk)(placeOnStart());
    document.documentElement.dataset.ready = ''; // style.css: the desk shows from here
    narrow.addEventListener('change', () => {
      chooseLayout();
      (S.layout === 'phone' ? showPhone : showDesk)();
    });
    // Relative times ("2m") keep moving, while the page is on screen: a
    // hidden page (another app, a locked phone) does no work.
    const tick = () => {
      const typing = S.layout === 'phone' ? document.activeElement?.id === 'p-ta' : deskEditorFocused();
      if (!typing && document.visibilityState === 'visible') S.render();
    };
    setInterval(tick, 30_000);
    document.addEventListener('visibilitychange', tick);
    live.initSignIn(() => location.reload());
    if (m.mode === 'signin') live.signIn();
    else if (m.mode === 'unsupported') live.signIn(m.why);
    // The offline copy is fetched once the first lists are in, not beside
    // them: its install reads every file of the build again.
    if (m.mode === 'live') live.refresh().finally(registerWorker);
    else { dropEarly(); registerWorker(); } // net.js's early reads are for a live start only
  } catch (err) {
    const msg = `Burrow couldn't start: ${err?.message || err}. It needs a browser with WebAssembly, served over http (not file://).`;
    for (const l of [$('#loading'), $('#p-loading')]) { l.classList.add('err'); l.textContent = msg; }
    console.error(err);
  }
})();
