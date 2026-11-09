// Localtime offline cache worker.
// Caches the shell, the asset manifest, every pack GLB listed in it and the
// pinned three.js module files, so a second visit works without the network.
// Served from assets/sw.js, where tools/serve.mjs allows the site-root scope.
'use strict';

const CACHE_PREFIX = 'localtime-assets-';
const CACHE_VERSION = 'ma3977eae';
const CACHE_NAME = CACHE_PREFIX + CACHE_VERSION;
const SCOPE = self.registration.scope;
const SHELL_URL = new URL('./index.html', SCOPE).href;
const SAMPLE_URL = new URL('./sample-export.json', SCOPE).href;
const MANIFEST_URL = new URL('./assets/manifest.json', SCOPE).href;
const ROOT_PATH = new URL('./', SCOPE).pathname;
const ASSETS_PREFIX = new URL('./assets/', SCOPE).pathname;
// The import map points "three" at the core module and "three/addons/" at the
// examples directory; the page loads GLTFLoader, and GLTFLoader itself imports
// BufferGeometryUtils, so that file is cached too or an offline visit breaks.
const THREE_CORE_URL = 'https://cdn.jsdelivr.net/npm/three@0.160.0/build/three.module.js';
const THREE_ADDONS_BASE = 'https://cdn.jsdelivr.net/npm/three@0.160.0/examples/jsm/';
const GLTF_LOADER_URL = THREE_ADDONS_BASE + 'loaders/GLTFLoader.js';
const MODULE_URLS = [THREE_CORE_URL, GLTF_LOADER_URL, THREE_ADDONS_BASE + 'utils/BufferGeometryUtils.js'];
const JSDELIVR_THREE_PREFIX = '/npm/three@0.160.0/';

function precache(cache, url) {
  // A missing optional entry must not abort the whole install.
  return cache.add(new Request(url, { cache: 'reload' })).catch(function () {});
}

async function packUrlsFromManifest() {
  var urls = [];
  try {
    var response = await fetch(MANIFEST_URL, { cache: 'no-store' });
    if (!response.ok) return urls;
    var manifest = await response.json();
    var packs = manifest && Array.isArray(manifest.packs) ? manifest.packs : [];
    for (var i = 0; i < packs.length; i += 1) {
      var file = packs[i] && packs[i].file;
      if (typeof file !== 'string' || !file) continue;
      try {
        var url = new URL(file, SCOPE).href;
        if (urls.indexOf(url) < 0) urls.push(url);
      } catch (error) {
        // ignore a malformed pack path
      }
    }
  } catch (error) {
    // a manifest that is absent at install time is retried by cache-first on demand
  }
  return urls;
}

self.addEventListener('install', function (event) {
  event.waitUntil(
    (async function () {
      var cache = await caches.open(CACHE_NAME);
      await precache(cache, SHELL_URL);
      // The sample export is the district's opening data; without it an offline visit would
      // synthesise a different day than the served one.
      await precache(cache, SAMPLE_URL);
      await precache(cache, MANIFEST_URL);
      var packUrls = await packUrlsFromManifest();
      for (var i = 0; i < packUrls.length; i += 1) {
        await precache(cache, packUrls[i]);
      }
      for (var k = 0; k < MODULE_URLS.length; k += 1) {
        await precache(cache, MODULE_URLS[k]);
      }
      await self.skipWaiting();
    })()
  );
});

self.addEventListener('activate', function (event) {
  event.waitUntil(
    (async function () {
      var names = await caches.keys();
      for (var i = 0; i < names.length; i += 1) {
        var name = names[i];
        if (name.indexOf(CACHE_PREFIX) === 0 && name !== CACHE_NAME) {
          await caches.delete(name);
        }
      }
      await self.clients.claim();
    })()
  );
});

async function cacheFirst(request) {
  var cache = await caches.open(CACHE_NAME);
  var hit = await cache.match(request);
  if (hit) return hit;
  try {
    var response = await fetch(request);
    if (response && (response.ok || response.type === 'opaque')) {
      cache.put(request, response.clone());
    }
    return response;
  } catch (error) {
    var fallback = await cache.match(request);
    if (fallback) return fallback;
    throw error;
  }
}

async function networkFirst(request) {
  var cache = await caches.open(CACHE_NAME);
  try {
    var response = await fetch(request);
    if (response && response.ok) {
      cache.put(request, response.clone());
    }
    return response;
  } catch (error) {
    var hit = await cache.match(request);
    if (hit) return hit;
    var shell = await cache.match(SHELL_URL);
    if (shell) return shell;
    throw error;
  }
}

self.addEventListener('fetch', function (event) {
  var request = event.request;
  if (request.method !== 'GET') return;
  var url;
  try {
    url = new URL(request.url);
  } catch (error) {
    return;
  }
  var sameOrigin = url.origin === self.location.origin;
  if (sameOrigin && (url.pathname === ROOT_PATH || /\/index\.html$/.test(url.pathname))) {
    event.respondWith(networkFirst(request));
    return;
  }
  if (sameOrigin && url.pathname.indexOf(ASSETS_PREFIX) === 0) {
    event.respondWith(cacheFirst(request));
    return;
  }
  // The opening data file is content-stable like the packs, so it is served from the cache
  // first and an offline visit opens the same day the served one does.
  if (sameOrigin && new URL(SAMPLE_URL).pathname === url.pathname) {
    event.respondWith(cacheFirst(request));
    return;
  }
  if (url.origin === 'https://cdn.jsdelivr.net' && url.pathname.indexOf(JSDELIVR_THREE_PREFIX) === 0) {
    event.respondWith(cacheFirst(request));
  }
});
