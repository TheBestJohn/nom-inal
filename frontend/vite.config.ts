import { defineConfig, type Plugin } from 'vite'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'
import { createHash } from 'node:crypto'
import fs from 'node:fs'
import path from 'node:path'

/** Files under public/ the shell needs offline; the bundle supplies the rest. */
const PUBLIC_SHELL = ['/manifest.webmanifest', '/icons/icon.svg', '/icons/icon-192.png']

/**
 * Emit the service worker with the built shell's file list inside it.
 *
 * `sw.js` at the project root is the worker, written by hand; the only thing
 * it cannot know until now is what the build produced, because Vite hashes
 * every file name. This fills in that list and a version derived from it, so
 * the worker's bytes change exactly when the shell does and a browser that
 * re-fetches it installs the new one. A plugin of a dozen lines, rather than
 * a PWA plugin that would generate a worker of its own design.
 */
function serviceWorker(): Plugin {
  return {
    name: 'nom-inal-service-worker',
    apply: 'build',
    generateBundle(_options, bundle) {
      // The barcode decoder is loaded on demand and is no use offline — a
      // lookup needs the network — so it is not part of the shell.
      const built = Object.keys(bundle)
        .filter((name) => name !== 'index.html' && !name.endsWith('.map'))
        .filter((name) => !name.startsWith('assets/zxing-'))
        .map((name) => `/${name}`)
        .sort()
      const precache = ['/', ...PUBLIC_SHELL, ...built]
      const version = createHash('sha256').update(precache.join('\n')).digest('hex').slice(0, 12)
      const source = fs
        .readFileSync(path.resolve(import.meta.dirname, 'sw.js'), 'utf8')
        .replace('__VERSION__', version)
        .replace('__PRECACHE__', JSON.stringify(precache))
      this.emitFile({ type: 'asset', fileName: 'sw.js', source })
    },
  }
}

export default defineConfig({
  plugins: [react(), tailwindcss(), serviceWorker()],
  resolve: {
    // shadcn/ui components are written against the `@/` alias.
    alias: { '@': path.resolve(import.meta.dirname, './src') },
  },
  server: {
    host: true,
    port: 5173,
    // In dev the SPA talks to the API through this proxy, so the browser only
    // ever sees one origin — the same arrangement nginx provides in production.
    proxy: {
      '/api': {
        target: process.env.VITE_API_PROXY ?? 'http://localhost:8080',
        changeOrigin: true,
      },
      // MCP lives on the API too; proxied so the URL Settings shows works in dev.
      '/mcp': {
        target: process.env.VITE_API_PROXY ?? 'http://localhost:8080',
        changeOrigin: true,
      },
    },
  },
  build: {
    outDir: 'dist',
    sourcemap: true,
    rollupOptions: {
      output: {
        // Recharts is by far the largest dependency and changes far less often
        // than app code. Splitting it out means shipping a new build only
        // invalidates the small app chunk, not ~450 kB of charting library.
        manualChunks: {
          charts: ['recharts'],
          vendor: ['react', 'react-dom', 'react-router-dom', '@tanstack/react-query'],
          // Only fetched when a barcode is scanned, and named so the service
          // worker can leave it out of the shell.
          zxing: ['@zxing/browser'],
        },
      },
    },
  },
})
