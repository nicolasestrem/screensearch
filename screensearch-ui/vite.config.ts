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
        manualChunks: {
          // Core React runtime - rarely changes, cached long-term
          'vendor-react': ['react', 'react-dom'],
          // Data fetching layer - changes occasionally
          'vendor-query': ['@tanstack/react-query', 'axios'],
          // UI utilities - animation, icons
          'vendor-ui': ['framer-motion', 'lucide-react'],
          // Heavy dependencies that are conditionally loaded
          'vendor-markdown': ['react-markdown'],
        },
      },
    },
    // Enable source maps for debugging production issues
    sourcemap: false,
    // Minification settings
    minify: 'esbuild',
    // Target modern browsers for smaller bundle
    target: 'es2020',
  },
})
