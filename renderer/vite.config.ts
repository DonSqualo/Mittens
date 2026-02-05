import { defineConfig } from 'vite';

export default defineConfig({
  base: '/victor/',
  server: {
    port: 3002,
    host: '0.0.0.0',
    open: false,
  },
  build: {
    target: 'esnext',
    outDir: 'dist',
  },
});
