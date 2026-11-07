// Starts Burrow in a browser. Every rule runs in Burrow's Rust, compiled to
// WebAssembly (pkg/blyg_web.js). Wider than 700px it's the desktop window
// (desk.js); narrower, the phone app (phone.js). Both draw the same
// `Burrow`, so turning a tablet or resizing a window keeps your place.

import init, { Burrow } from './pkg/blyg_web.js';
import { $, S, now, load, pref, applyTheme } from './core.js';
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
// opaque origin, where workers can't run.
function registerWorker() {
  if (!('serviceWorker' in navigator) || !/^https?:$/.test(location.protocol)) return;
  navigator.serviceWorker.register('sw.js').catch((err) => console.warn('No offline copy:', err?.message || err));
}

(async () => {
  try {
    await init();
    S.themes = JSON.parse(Burrow.themes());
    const dark = matchMedia('(prefers-color-scheme: dark)').matches;
    applyTheme(pref.get('theme') || (dark ? 'dark' : 'light'), [document.documentElement]);
    S.B = new Burrow(now(), await load());
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
    registerWorker();
  } catch (err) {
    const msg = `Burrow couldn't start: ${err?.message || err}. It needs a browser with WebAssembly, served over http (not file://).`;
    for (const l of [$('#loading'), $('#p-loading')]) { l.classList.add('err'); l.textContent = msg; }
    console.error(err);
  }
})();
