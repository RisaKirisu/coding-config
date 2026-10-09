// Only public Control app assets are cached. APIs and Project-origin DSH traffic
// never enter this cache. Updates activate after the previous app closes.
const CACHE = 'devvm-app-__ASSET_VERSION__';
const ASSETS = ['/', '/app/app.css', '/app/app.js', '/app/text-store.js', '/app/icon.svg', '/app/icon-192.png', '/app/icon-512.png', '/manifest.webmanifest'];
self.addEventListener('install', event => {
  event.waitUntil(caches.open(CACHE).then(cache => cache.addAll(ASSETS)));
});
self.addEventListener('activate', event => {
  event.waitUntil(caches.keys().then(keys => Promise.all(keys.filter(key => key.startsWith('devvm-app-') && key !== CACHE).map(key => caches.delete(key)))));
});
self.addEventListener('fetch', event => {
  const url = new URL(event.request.url);
  if (event.request.method !== 'GET' || url.origin !== self.location.origin || url.search || !ASSETS.includes(url.pathname)) return;
  event.respondWith(caches.open(CACHE).then(async cache => {
    const cached = await cache.match(url.pathname);
    return cached || fetch(event.request);
  }));
});
