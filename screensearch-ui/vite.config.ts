import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import path from 'path'

// https://vitejs.dev/config/
export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: {
      '@': path.resolve(__dirname, './src'),
    },
  },
  server: {
    port: 5173,
    proxy: {
      '/api': {
        target: 'http://localhost:3131',
        changeOrigin: true,
      },
      '/health': {
        target: 'http://localhost:3131',
        changeOrigin: true,
      },
    },
  },
  build: {
    // Chunk size warning threshold (in KB)
    chunkSizeWarningLimit: 500,
    rollupOptions: {
      output: {
        // Manual chunk splitting for better caching
        manualChunks(id) {
          if (!id.includes('node_modules')) return undefined
          if (/[\\/]node_modules[\\/](react|react-dom)[\\/]/.test(id)) {
            return 'vendor-react'
          }
          if (/[\\/]node_modules[\\/](@tanstack[\\/]react-query|axios)[\\/]/.test(id)) {
            return 'vendor-query'
          }
          if (/[\\/]node_modules[\\/](framer-motion|lucide-react)[\\/]/.test(id)) {
            return 'vendor-ui'
          }
          if (/[\\/]node_modules[\\/]react-markdown[\\/]/.test(id)) {
            return 'vendor-markdown'
          }
          return undefined
        },
      },
    },
    // Enable source maps for debugging production issues
    sourcemap: false,
    // Use Vite 8's built-in Oxc minifier.
    minify: 'oxc',
    // Target modern browsers for smaller bundle
    target: 'es2020',
  },
})
