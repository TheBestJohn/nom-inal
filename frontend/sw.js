/*
 * The service worker, written by hand.
 *
 * It does two things. It keeps the built shell — index.html and the hashed
 * bundles it references — so the app opens with no network, and it keeps the
 * one API response that makes an offline open worth anything: today's diary.
 * Nothing else is cached. Writes never are: a POST that "succeeded" from a
 * cache would be a lie the diary told you. Photos never are: they are large,
 * private, and served with an ownership check on every read.
 *
 * Today's diary is served network-first with the cache as the fallback,
 * not stale-while-revalidate. The diary is written to constantly, and every
 * write is followed by a re-read of the day; a worker that answered that
 * re-read from cache and refreshed in the background would show the day as
 * it was before the entry you just logged, every time. Offline is the only
 * moment the cached copy should win, so it wins only when the network fails.
 *
 * The placeholders below are filled in at build time from the emitted
 * bundle (see the plugin in vite.config.ts): the asset names are content
 * hashes, so the version changes exactly when the shell does, and a browser
 * that fetches this file — nginx sends it uncached — installs the new one.
 */

const VERSION = '__VERSION__'
const PRECACHE = __PRECACHE__

const SHELL = `nom-inal-shell-${VERSION}`
const DATA = 'nom-inal-data'

self.addEventListener('install', (event) => {
  event.waitUntil(
    caches
      .open(SHELL)
      .then((cache) => cache.addAll(PRECACHE))
      .then(() => self.skipWaiting()),
  )
})

self.addEventListener('activate', (event) => {
  event.waitUntil(
    caches
      .keys()
      .then((keys) =>
        Promise.all(
          keys
            .filter((key) => key.startsWith('nom-inal-shell-') && key !== SHELL)
            .map((key) => caches.delete(key)),
        ),
      )
      .then(() => self.clients.claim()),
  )
})

self.addEventListener('fetch', (event) => {
  const request = event.request
  if (request.method !== 'GET') return

  const url = new URL(request.url)
  if (url.origin !== self.location.origin) return

  // Today's diary, and only today's — plus the profile, which the app reads
  // before it will show anything at all. The cache key carries a digest of
  // the bearer token so a second account on the same browser never sees the
  // first one's day when offline.
  if (url.pathname === '/api/v1/diary/day') {
    const date = url.searchParams.get('date')
    if (date && date !== localToday()) return
    event.respondWith(accountData(request))
    return
  }
  if (url.pathname === '/api/v1/auth/me') {
    event.respondWith(accountData(request))
    return
  }

  // Every other API call, and MCP, go straight to the network.
  if (url.pathname.startsWith('/api/') || url.pathname === '/mcp') return

  // A shared recipe's page is a document the server renders about somebody
  // else's recipe, not part of this shell. It changes whenever they correct
  // the recipe, and it has nothing to do with this app working offline, so
  // the worker stays out of it entirely.
  if (url.pathname.startsWith('/r/')) return

  // A page load: the freshest shell if it can be had, the cached one if not.
  if (request.mode === 'navigate') {
    event.respondWith(networkFirst(request, SHELL, '/'))
    return
  }

  // Hashed bundles never change under their name, so a cached one is right
  // for as long as it exists.
  if (url.pathname.startsWith('/assets/')) {
    event.respondWith(cacheFirst(request, SHELL))
    return
  }

  event.respondWith(networkFirst(request, SHELL))
})

async function accountData(request) {
  const cache = await caches.open(DATA)
  const key = await dataKey(request)
  try {
    const response = await fetch(request)
    // The key carries no date, so today's copy overwrites yesterday's and
    // an account never holds more than one day.
    if (response.ok) await cache.put(key, response.clone())
    return response
  } catch (err) {
    const cached = await cache.match(key)
    if (cached) return cached
    throw err
  }
}

async function networkFirst(request, cacheName, fallbackUrl) {
  try {
    const response = await fetch(request)
    if (response.ok) {
      const cache = await caches.open(cacheName)
      await cache.put(request, response.clone())
    }
    return response
  } catch (err) {
    const cached =
      (await caches.match(request)) ?? (fallbackUrl && (await caches.match(fallbackUrl)))
    if (cached) return cached
    throw err
  }
}

async function cacheFirst(request, cacheName) {
  const cached = await caches.match(request)
  if (cached) return cached
  const response = await fetch(request)
  if (response.ok) {
    const cache = await caches.open(cacheName)
    await cache.put(request, response.clone())
  }
  return response
}

/** An account-scoped request's cache key: its path plus who asked. */
async function dataKey(request) {
  const token = request.headers.get('authorization') ?? ''
  const digest = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(token))
  const who = Array.from(new Uint8Array(digest).slice(0, 8))
    .map((b) => b.toString(16).padStart(2, '0'))
    .join('')
  const url = new URL(request.url)
  url.searchParams.delete('date')
  url.searchParams.set('who', who)
  return url.toString()
}

/** The local calendar date, matching what the app sends as `date`. */
function localToday() {
  const now = new Date()
  const m = String(now.getMonth() + 1).padStart(2, '0')
  const d = String(now.getDate()).padStart(2, '0')
  return `${now.getFullYear()}-${m}-${d}`
}
