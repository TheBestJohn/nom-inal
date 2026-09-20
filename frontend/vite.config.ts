import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'
import path from 'node:path'

export default defineConfig({
  plugins: [react(), tailwindcss()],
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
        },
      },
    },
  },
})
