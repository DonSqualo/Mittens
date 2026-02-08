import { defineConfig } from 'vite';

export default defineConfig({
  base: '/',
  server: {
    port: 3000,
    open: false,
    proxy: {
      '/ws': {
        target: 'ws://127.0.0.1:3100',
        ws: true,
        changeOrigin: true,
      },
      '/api/backends': {
        target: 'http://127.0.0.1:3100',
        changeOrigin: true,
      },
      '/api/graph': {
        target: 'http://127.0.0.1:3100',
        changeOrigin: true,
      },
      '/graph': {
        target: 'http://127.0.0.1:3100',
        changeOrigin: true,
      },
    },
  },
  build: {
    target: 'esnext',
    outDir: 'dist',
  },
});
