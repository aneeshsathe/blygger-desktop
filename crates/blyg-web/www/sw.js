// Burrow's service worker: keeps the app shell, the wasm and the fonts so
// Burrow opens with no network. Your posts aren't here; they're in
// IndexedDB. serve.sh stamps BUILD and FILES at build time, so each build
// gets a fresh cache and the old one is dropped once the new one is ready.

const BUILD = '__BUILD__';
const FILES = __FILES__;
const CACHE = `burrow-web-${BUILD}`;

self.addEventListener('install', (e) => {
  e.waitUntil(caches.open(CACHE).then((c) => c.addAll(['./', ...FILES])).then(() => self.skipWaiting()));
});

self.addEventListener('activate', (e) => {
  e.waitUntil((async () => {
    for (const k of await caches.keys()) if (k.startsWith('burrow-web-') && k !== CACHE) await caches.delete(k);
    await self.clients.claim();
  })());
});

// Cache first: everything here is versioned by BUILD. Anything else from
// this origin is fetched and kept; other origins are left alone. A page
// load with no network gets the cached shell.
self.addEventListener('fetch', (e) => {
  const req = e.request;
  if (req.method !== 'GET' || new URL(req.url).origin !== location.origin) return;
  e.respondWith((async () => {
    const cache = await caches.open(CACHE);
    const hit = await cache.match(req, { ignoreSearch: true });
    if (hit) return hit;
    try {
      const res = await fetch(req);
      if (res.ok && res.type === 'basic') cache.put(req, res.clone());
      return res;
    } catch (err) {
      if (req.mode === 'navigate') {
        const shell = (await cache.match('./')) || (await cache.match('index.html'));
        if (shell) return shell;
      }
      throw err;
    }
  })());
});
